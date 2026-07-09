//! Qobuz lyrics wire DTOs.
//!
//! Two-step contract (inferred spec `qobuz-api-inferred-openapi-v9.9.0.0-beta.yaml:759-806`,
//! reconciled against LIVE captures 2026-06-10 — samples in
//! `qbz-nix-docs/qobuz-api/lyrics-*.json`):
//!   1. `GET /track/lyricsUrl` (signed RPC) -> [`QobuzLyricsUrls`] envelope with the CDN URL.
//!   2. `GET <lyrics_url>` (pre-signed CloudFront URL) -> [`QobuzLyricsDocument`].
//!
//! Divergences found live vs the yaml (each noted on the affected field):
//!   - Step-1 `track_id` is a JSON NUMBER (yaml said string); `album_id` IS a string.
//!   - The CDN payload is NOT the bare content union the yaml describes
//!     (`LyricsContentDto`): it is a wrapper `{album_id, track_id,
//!     translation_langs, publishers, writers, original}` with the content
//!     union under `original`.
//!   - The synced discriminator is `"wsync"` (word-synced), not `"lsync"`:
//!     each line carries per-WORD timestamps in addition to line start/end.
//!   - Plain lines are objects `{"line": "..."}`, not bare strings.
//!   - Miss = HTTP 404 with `{"status":"error","code":404,"message":"Lyrics
//!     are not available for this track."}` (sample: lyrics-miss-response.json).
//!
//! All fields stay serde-tolerant (`Option` / `#[serde(default)]`) — the
//! endpoint is a beta-era delta and may still move.

use serde::{Deserialize, Deserializer};

/// Accept either a JSON string or a JSON number and normalize to `String`.
///
/// The yaml declares `track_id` as string ("string form, even though the query
/// param is numeric", yaml:205) but the live capture (lyrics-url-response.json)
/// shows `track_id` as a JSON NUMBER while `album_id` is a string (album slug
/// or barcode, e.g. `"gvcirtodd95kc"` / `"5052205066164"`). Tolerate both.
fn de_opt_string_or_number<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNumber {
        String(String),
        Number(serde_json::Number),
    }

    Ok(Option::<StringOrNumber>::deserialize(deserializer)?.map(|v| match v {
        StringOrNumber::String(s) => s,
        StringOrNumber::Number(n) => n.to_string(),
    }))
}

/// Step-1 envelope returned by `GET /track/lyricsUrl`.
///
/// Yaml `LyricsUrlsDto` (`:194-219`). Live shape (lyrics-url-response.json):
/// `{"track_id": 266725027, "album_id": "gvcirtodd95kc", "lyrics_url": "https://…cloudfront.net/…"}`
/// — `track_id` is a number (yaml said string), `translation_requested` is
/// simply ABSENT when no translation was asked, and the URL is a pre-signed
/// CloudFront URL (self-authorizing: `Expires`/`Signature`/`Key-Pair-Id`).
#[derive(Debug, Clone, Deserialize)]
pub struct QobuzLyricsUrls {
    #[serde(default, deserialize_with = "de_opt_string_or_number")]
    pub track_id: Option<String>,
    #[serde(default, deserialize_with = "de_opt_string_or_number")]
    pub album_id: Option<String>,
    /// URL to the original-language lyrics document (Qobuz CDN host).
    #[serde(default)]
    pub lyrics_url: Option<String>,
    /// Present only when the `translation` query param was sent AND a matching
    /// translation exists (yaml:208-213). We never send `translation` in v1,
    /// so this stays `None` (confirmed absent in the live capture).
    #[serde(default)]
    pub translation_requested: Option<QobuzLyricsTranslation>,
}

/// Translation pointer inside [`QobuzLyricsUrls`] (yaml `LyricsTranslationDto`, `:221-231`).
/// Unverified live (v1 never requests translations); kept yaml-shaped + tolerant.
#[derive(Debug, Clone, Deserialize)]
pub struct QobuzLyricsTranslation {
    #[serde(default)]
    pub lyrics_url: Option<String>,
    /// ISO 639-1 code of the returned translation.
    #[serde(default)]
    pub lang: Option<String>,
}

/// Step-2 CDN lyrics document — the REAL wire shape (live divergence: the
/// yaml's `LyricsContentDto` union is nested under `original`, wrapped with
/// catalog metadata; samples lyrics-doc-wsync.json / lyrics-doc-plain.json).
#[derive(Debug, Clone, Deserialize)]
pub struct QobuzLyricsDocument {
    #[serde(default, deserialize_with = "de_opt_string_or_number")]
    pub track_id: Option<String>,
    #[serde(default, deserialize_with = "de_opt_string_or_number")]
    pub album_id: Option<String>,
    /// ISO 639-1 codes a translation exists for (e.g. `["pt","de","fr","es","it"]`).
    #[serde(default)]
    pub translation_langs: Vec<String>,
    /// Publisher copyright entries with applicability zones.
    #[serde(default)]
    pub publishers: Vec<QobuzLyricsPublisher>,
    /// Songwriter credits as a single display string.
    #[serde(default)]
    pub writers: Option<String>,
    /// The actual lyrics content (the yaml's "LyricsContentDto" union).
    #[serde(default)]
    pub original: Option<QobuzLyricsContent>,
}

/// One publisher credit inside [`QobuzLyricsDocument`] (live-only; not in the yaml).
#[derive(Debug, Clone, Deserialize)]
pub struct QobuzLyricsPublisher {
    #[serde(default)]
    pub copyright: Option<String>,
    /// Zone codes the copyright applies to (e.g. `["WW"]`, `["WW","FR"]`).
    #[serde(default)]
    pub zones: Vec<String>,
}

/// Lyrics content union, discriminated on `type`.
///
/// Live discriminator is `"wsync"` (word-synced — richer than the yaml's
/// `"lsync"` guess: per-word timestamps inside each line). `"lsync"` is kept
/// as an alias in case line-only documents exist in the catalog (same shape
/// minus `words`, which defaults to empty).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum QobuzLyricsContent {
    /// Time-synced lyrics; line + per-word timestamps, ms from track start.
    #[serde(rename = "wsync", alias = "lsync")]
    Synced {
        /// ISO 639-1 of the document language (present in live captures).
        #[serde(default)]
        lang: Option<String>,
        #[serde(default)]
        lines: Vec<QobuzLyricsLine>,
    },
    /// Plain (non-synced) lyrics: text-only lines, no timestamps.
    #[serde(rename = "plain")]
    Plain {
        #[serde(default)]
        lang: Option<String>,
        #[serde(default)]
        lines: Vec<QobuzLyricsPlainLine>,
    },
}

impl QobuzLyricsContent {
    /// True when this document carries per-line timestamps.
    pub fn is_synced(&self) -> bool {
        matches!(self, QobuzLyricsContent::Synced { .. })
    }

    /// Number of lines in the document.
    pub fn line_count(&self) -> usize {
        match self {
            QobuzLyricsContent::Synced { lines, .. } => lines.len(),
            QobuzLyricsContent::Plain { lines, .. } => lines.len(),
        }
    }
}

/// One synced line.
///
/// Live divergence from the yaml's `LyricsLineDto` (`{line,start,end}` all
/// required): instrumental-gap separator lines come as `{"line":"","words":[]}`
/// with NO `start`/`end` at all — both must be `Option`. Regular lines carry
/// line-level `start`/`end` plus per-word stamps (lyrics-doc-wsync.json).
#[derive(Debug, Clone, Deserialize)]
pub struct QobuzLyricsLine {
    #[serde(default)]
    pub line: String,
    /// Per-word timestamps ("wsync"); empty for gap lines and "lsync" docs.
    #[serde(default)]
    pub words: Vec<QobuzLyricsWord>,
    /// Line start, ms from track start. Absent on gap lines.
    #[serde(default)]
    pub start: Option<i64>,
    /// Line end, ms from track start. Absent on gap lines.
    #[serde(default)]
    pub end: Option<i64>,
}

/// One word inside a synced line (live-only; not in the yaml).
#[derive(Debug, Clone, Deserialize)]
pub struct QobuzLyricsWord {
    #[serde(default)]
    pub word: String,
    /// Word start, ms from track start.
    #[serde(default)]
    pub start: i64,
    /// Word end, ms from track start.
    #[serde(default)]
    pub end: i64,
}

/// One plain line. Live wire form is an object `{"line": "..."}`
/// (lyrics-doc-plain.json); the yaml guessed bare strings (`:287-303`) —
/// accept both.
#[derive(Debug, Clone)]
pub struct QobuzLyricsPlainLine {
    pub line: String,
}

impl<'de> Deserialize<'de> for QobuzLyricsPlainLine {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Text(String),
            Object {
                #[serde(default)]
                line: String,
            },
        }

        Ok(match Repr::deserialize(deserializer)? {
            Repr::Text(line) => QobuzLyricsPlainLine { line },
            Repr::Object { line } => QobuzLyricsPlainLine { line },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fixtures are excerpts of the live captures of 2026-06-10 (full
    // sanitized copies live in qbz-nix-docs/qobuz-api/lyrics-*.json; the
    // synced/plain docs are truncated to a few lines here to avoid vendoring
    // full copyrighted lyrics in the repo — the STRUCTURE is verbatim).
    const URL_RESPONSE: &str = include_str!("../tests/fixtures/lyrics-url-response.json");
    const DOC_WSYNC: &str = include_str!("../tests/fixtures/lyrics-doc-wsync.json");
    const DOC_PLAIN: &str = include_str!("../tests/fixtures/lyrics-doc-plain.json");
    const MISS_RESPONSE: &str = include_str!("../tests/fixtures/lyrics-miss-response.json");

    #[test]
    fn deserialize_live_lyrics_url_response() {
        let urls: QobuzLyricsUrls =
            serde_json::from_str(URL_RESPONSE).expect("parse step-1 envelope");
        // Live shape: numeric track_id normalized to string; album_id is a slug.
        assert_eq!(urls.track_id.as_deref(), Some("266725027"));
        assert_eq!(urls.album_id.as_deref(), Some("gvcirtodd95kc"));
        let url = urls.lyrics_url.expect("lyrics_url present");
        assert!(url.starts_with("https://"));
        assert!(urls.translation_requested.is_none());
    }

    #[test]
    fn deserialize_live_wsync_document() {
        let doc: QobuzLyricsDocument =
            serde_json::from_str(DOC_WSYNC).expect("parse wsync document");
        assert_eq!(doc.track_id.as_deref(), Some("266725027"));
        assert_eq!(doc.album_id.as_deref(), Some("gvcirtodd95kc"));
        assert!(!doc.translation_langs.is_empty());
        assert!(!doc.publishers.is_empty());
        assert!(doc.writers.is_some());

        let content = doc.original.expect("original content present");
        assert!(content.is_synced());
        match &content {
            QobuzLyricsContent::Synced { lang, lines } => {
                assert_eq!(lang.as_deref(), Some("en"));
                assert_eq!(lines.len(), 3);
                // Regular line: text + line stamps + word stamps.
                let first = &lines[0];
                assert_eq!(first.line, "Oh, mm-mm");
                assert_eq!(first.start, Some(1750));
                assert_eq!(first.end, Some(4770));
                assert_eq!(first.words.len(), 2);
                assert_eq!(first.words[0].word, "Oh,");
                assert_eq!(first.words[0].start, 1750);
                assert_eq!(first.words[0].end, 2480);
                // Gap line: empty text, no words, NO start/end on the wire.
                let gap = &lines[1];
                assert!(gap.line.is_empty());
                assert!(gap.words.is_empty());
                assert!(gap.start.is_none());
                assert!(gap.end.is_none());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn deserialize_live_plain_document() {
        let doc: QobuzLyricsDocument =
            serde_json::from_str(DOC_PLAIN).expect("parse plain document");
        assert_eq!(doc.track_id.as_deref(), Some("29006863"));
        let content = doc.original.expect("original content present");
        assert!(!content.is_synced());
        match &content {
            QobuzLyricsContent::Plain { lang, lines } => {
                assert_eq!(lang.as_deref(), Some("en"));
                assert_eq!(lines.len(), 4);
                // Live plain lines are OBJECTS {"line": "..."}.
                assert_eq!(lines[0].line, "Night follows me when you're gone");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn miss_response_is_not_a_lyrics_envelope() {
        // Live miss = HTTP 404 with Qobuz's standard error body. The client
        // maps non-200 to Ok(None) before parsing, but make sure the error
        // body could never masquerade as a usable envelope.
        let parsed: Result<QobuzLyricsUrls, _> = serde_json::from_str(MISS_RESPONSE);
        if let Ok(urls) = parsed {
            assert!(urls.lyrics_url.is_none());
        }
    }

    #[test]
    fn lyrics_url_response_tolerates_string_ids() {
        // Yaml-shaped variant (track_id as string) must also parse — the wire
        // showed a number, the spec says string; accept both.
        let json = r#"{"track_id":"1","album_id":"2","lyrics_url":"https://example.com/l.json"}"#;
        let urls: QobuzLyricsUrls = serde_json::from_str(json).expect("parse string-id variant");
        assert_eq!(urls.track_id.as_deref(), Some("1"));
        assert_eq!(urls.album_id.as_deref(), Some("2"));
    }

    #[test]
    fn lsync_alias_parses_yaml_shape() {
        // The yaml's "lsync" guess ({line,start,end}, no words) must still
        // parse via the alias in case line-only docs exist in the catalog.
        let json = r#"{"type":"lsync","lang":"fr","lines":[{"line":"hi","start":100,"end":200}]}"#;
        let content: QobuzLyricsContent =
            serde_json::from_str(json).expect("parse lsync alias");
        match content {
            QobuzLyricsContent::Synced { lang, lines } => {
                assert_eq!(lang.as_deref(), Some("fr"));
                assert_eq!(lines.len(), 1);
                assert_eq!(lines[0].start, Some(100));
                assert!(lines[0].words.is_empty());
            }
            _ => panic!("expected synced"),
        }
    }

    #[test]
    fn plain_lines_tolerate_bare_strings() {
        // Yaml-shaped plain variant (lines as bare strings) must also parse.
        let json = r#"{"type":"plain","lines":["first line","second line"]}"#;
        let content: QobuzLyricsContent =
            serde_json::from_str(json).expect("parse string-lines plain");
        assert_eq!(content.line_count(), 2);
        match content {
            QobuzLyricsContent::Plain { lines, .. } => {
                assert_eq!(lines[0].line, "first line");
            }
            _ => panic!("expected plain"),
        }
    }
}
