//! Convert HTML-ish strings from Qobuz (biographies, album reviews)
//! into Slint-friendly plain text. Slint's `Text` is single-style, so
//! we cannot render inline strong/em formatting — those tags are
//! stripped but their content stays inline. Paragraph and line-break
//! structure IS preserved: `<br>` collapses to `\n`, `</p>` to a
//! blank line so Text renders the paragraphs separated visually.

/// Render an HTML-ish blurb into plain text with paragraph breaks.
pub fn strip_html(input: &str) -> String {
    // Normalize break/paragraph tags into newlines first (case-
    // insensitive, with and without self-closing slash and whitespace).
    let normalized = normalize_breaks(input);

    // Drop every other tag — strong/em/i/b/a/etc lose styling but
    // keep their text content because the rest of the loop emits
    // characters as-is when not inside a tag.
    let mut out = String::with_capacity(normalized.len());
    let mut in_tag = false;
    for ch in normalized.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }

    let decoded = decode_entities(&out);
    collapse_blank_lines(&decoded)
}

fn normalize_breaks(input: &str) -> String {
    // Hand-rolled lowercase walk so we match `<BR>`, `<Br />`, `</P>`,
    // etc. without paying for a full regex dep.
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            if let Some((replacement, advance)) = match_break_or_paragraph(&bytes[i..]) {
                out.push_str(replacement);
                i += advance;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Try to match `<br>` (any spacing/case) or `</p>` (any case) at the
/// start of `b`. Returns the replacement string + byte length to skip
/// when matched.
fn match_break_or_paragraph(b: &[u8]) -> Option<(&'static str, usize)> {
    // <br>, <br/>, <br />, <BR>, <Br/>, ...
    if b.len() >= 4
        && b[0] == b'<'
        && (b[1] == b'b' || b[1] == b'B')
        && (b[2] == b'r' || b[2] == b'R')
    {
        // Walk to the closing '>'
        let mut j = 3usize;
        while j < b.len() && b[j] != b'>' {
            // Only allow whitespace and a single '/' inside <br..>
            if !b[j].is_ascii_whitespace() && b[j] != b'/' {
                return None;
            }
            j += 1;
        }
        if j < b.len() && b[j] == b'>' {
            return Some(("\n", j + 1));
        }
        return None;
    }
    // </p>, </P>
    if b.len() >= 4
        && b[0] == b'<'
        && b[1] == b'/'
        && (b[2] == b'p' || b[2] == b'P')
        && b[3] == b'>'
    {
        return Some(("\n\n", 4));
    }
    None
}

fn decode_entities(input: &str) -> String {
    // Hand-roll a few of the entities Qobuz uses most. Anything we
    // don't recognise is left as-is rather than being garbled.
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            if let Some((replacement, advance)) = match_entity(&bytes[i..]) {
                out.push_str(replacement);
                i += advance;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn match_entity(b: &[u8]) -> Option<(&'static str, usize)> {
    const TABLE: &[(&[u8], &str)] = &[
        (b"&amp;", "&"),
        (b"&lt;", "<"),
        (b"&gt;", ">"),
        (b"&quot;", "\""),
        (b"&apos;", "'"),
        (b"&#39;", "'"),
        (b"&nbsp;", " "),
        (b"&copy;", "\u{00A9}"),
        (b"&#169;", "\u{00A9}"),
        (b"&#xa9;", "\u{00A9}"),
        (b"&reg;", "\u{00AE}"),
        (b"&mdash;", "\u{2014}"),
        (b"&ndash;", "\u{2013}"),
        (b"&hellip;", "\u{2026}"),
        (b"&ldquo;", "\u{201C}"),
        (b"&rdquo;", "\u{201D}"),
        (b"&lsquo;", "\u{2018}"),
        (b"&rsquo;", "\u{2019}"),
    ];
    for (needle, replacement) in TABLE {
        if b.len() >= needle.len() && b[..needle.len()] == **needle {
            return Some((replacement, needle.len()));
        }
    }
    None
}

fn collapse_blank_lines(input: &str) -> String {
    // Slint's word wrap renders `\n\n` as a clean paragraph break.
    // Trim leading/trailing whitespace overall and prevent runs of
    // 3+ newlines from blowing the layout open.
    let trimmed = input.trim();
    let mut out = String::with_capacity(trimmed.len());
    let mut consecutive_newlines = 0;
    for ch in trimmed.chars() {
        if ch == '\n' {
            consecutive_newlines += 1;
            if consecutive_newlines <= 2 {
                out.push(ch);
            }
        } else {
            consecutive_newlines = 0;
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_inline_formatting() {
        let html = "<p>One <strong>bold</strong> and <em>italic</em>.</p>";
        let plain = strip_html(html);
        assert_eq!(plain, "One bold and italic.");
    }

    #[test]
    fn converts_br_to_newline() {
        let html = "Line 1<br>Line 2<br />Line 3";
        assert_eq!(strip_html(html), "Line 1\nLine 2\nLine 3");
    }

    #[test]
    fn converts_paragraphs() {
        let html = "<p>First.</p><p>Second.</p>";
        assert_eq!(strip_html(html), "First.\n\nSecond.");
    }

    #[test]
    fn decodes_common_entities() {
        let html = "Rock &amp; Roll &mdash; &ldquo;the rest&rdquo;.";
        assert_eq!(strip_html(html), "Rock & Roll \u{2014} \u{201C}the rest\u{201D}.");
    }

    #[test]
    fn collapses_excess_newlines() {
        let html = "<p>A</p><p>B</p><p>C</p>";
        let out = strip_html(html);
        // Three paragraphs separated by single blank line, no extra
        // padding above or below.
        assert_eq!(out, "A\n\nB\n\nC");
    }
}
