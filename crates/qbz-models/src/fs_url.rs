//! `file://` URLs and "is this an absolute local path" — ONE definition.
//!
//! Sites used to build `format!("file://{p}")` by hand and to test
//! `starts_with('/')` for "absolute path". Both are Unix-only:
//! `file://C:\Users\…` parses `C:` as a URL **host**, and `C:\…` does not
//! start with `/`. Windows lost every local cover, the folder tree and the
//! runtime-tinted icons — silently, because a URL that resolves to nothing
//! renders as an empty rectangle, not as an error. Everything routes through
//! here now.

/// Absolute local path, by SHAPE (so the tests run on every host):
/// `/…` (POSIX), `X:\…` / `X:/…` (drive), `\\server\share…` (UNC).
///
/// Shape, not `Path::is_absolute`: that answers for the HOST it runs on, so a
/// Linux CI run would call `C:\x` relative and the Windows-only regression
/// would pass every test on the machine that runs them.
pub fn is_local_abs_path(s: &str) -> bool {
    if s.starts_with('/') {
        return true;
    }
    let b = s.as_bytes();
    if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')
    {
        return true;
    }
    s.starts_with("\\\\")
}

/// `file://` URL for a raw filesystem path. Idempotent for inputs that already
/// carry the scheme.
///
/// Qt's `QUrl` parses `#` as a fragment and `?` as a query, so a cover under
/// `…/Album #1/cover.jpg` resolves to nothing when concatenated raw.
/// Percent-encode exactly those two plus `%` itself (first, or the escapes we
/// add would be double-decoded). Spaces are left alone: `QUrl` accepts them.
///
/// Backslashes become `/` only for a Windows-SHAPED input (drive or UNC):
/// on Unix a backslash is a legal filename character. A drive path gets
/// the empty-authority form
/// `file:///C:/…`, which is what `QUrl` and WinRT both accept. A UNC path puts
/// the server in the authority — `file://nas/music/x.jpg` — because
/// `file:////nas/…` is not a form anything resolves.
pub fn file_url(path: &str) -> String {
    if path.starts_with("file://") {
        return path.to_string();
    }
    // Backslash-to-slash ONLY for Windows-shaped inputs. A backslash is a
    // LEGAL filename character on Unix, so rewriting it unconditionally would
    // point `/home/v/AC\DC/cover.jpg` at a directory that does not exist --
    // the same defect class as rewriting separators in a qrc alias.
    let win_shaped = path.starts_with("\\\\")
        || {
            let b = path.as_bytes();
            b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':'
        };
    let normalised = if win_shaped {
        path.replace('\\', "/")
    } else {
        path.to_string()
    };
    let mut out = String::with_capacity(normalised.len() + 8);
    out.push_str("file://");
    let body: &str = if let Some(unc) = normalised.strip_prefix("//") {
        unc
    } else if normalised.starts_with('/') {
        &normalised
    } else {
        // `C:/x` and any relative remnant: the empty authority needs the
        // third slash.
        out.push('/');
        &normalised
    };
    for ch in body.chars() {
        match ch {
            '%' => out.push_str("%25"),
            '#' => out.push_str("%23"),
            '?' => out.push_str("%3F"),
            _ => out.push(ch),
        }
    }
    out
}

/// The inverse of [`file_url`]: a `file://` URL back to a filesystem path.
///
/// `trim_start_matches("file://")` is NOT the inverse — it leaves the three
/// percent-escapes in place and, on Windows, a leading `/` before the drive,
/// so the result names a file that does not exist. That silently broke
/// whatever opened it (a tint decode, a copy) for exactly the paths the
/// escaping exists to support.
pub fn local_path(url: &str) -> String {
    let raw = url.strip_prefix("file://").unwrap_or(url);
    let raw = raw
        .replace("%23", "#")
        .replace("%3F", "?")
        .replace("%25", "%");
    // `file:///C:/x` → raw `/C:/x` → `C:/x`.
    let b = raw.as_bytes();
    if b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b':' {
        return raw[1..].to_string();
    }
    raw
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posix_paths_round_trip_and_escape() {
        assert_eq!(
            file_url("/home/v/Album #1/cover?.jpg"),
            "file:///home/v/Album %231/cover%3F.jpg"
        );
        assert_eq!(
            local_path("file:///home/v/Album%20%231/c%25.jpg"),
            "/home/v/Album%20#1/c%.jpg"
        );
        assert_eq!(file_url("file:///already"), "file:///already");
    }

    #[test]
    fn drive_paths_get_the_empty_authority_form() {
        assert_eq!(
            file_url("C:\\Users\\v\\cover.jpg"),
            "file:///C:/Users/v/cover.jpg"
        );
        assert_eq!(
            file_url("C:/Users/v/cover.jpg"),
            "file:///C:/Users/v/cover.jpg"
        );
        assert_eq!(
            local_path("file:///C:/Users/v/cover.jpg"),
            "C:/Users/v/cover.jpg"
        );
    }

    #[test]
    fn unc_paths_put_the_server_in_the_authority() {
        assert_eq!(
            file_url("\\\\nas\\music\\cover.jpg"),
            "file://nas/music/cover.jpg"
        );
    }

    #[test]
    fn drive_paths_round_trip_through_both_directions() {
        // The regression that started this: a tinted icon directory.
        let p = "C:\\Users\\blitz\\AppData\\Local\\qbz\\icon-tints\\4285f4";
        let u = file_url(p);
        assert_eq!(u, "file:///C:/Users/blitz/AppData/Local/qbz/icon-tints/4285f4");
        assert_eq!(local_path(&u), "C:/Users/blitz/AppData/Local/qbz/icon-tints/4285f4");
        assert_eq!(file_url(&u), u, "file_url must be idempotent");
    }

    #[test]
    fn a_backslash_in_a_unix_filename_is_not_a_separator() {
        // Legal on Unix; rewriting it would name a path that does not exist.
        assert_eq!(
            file_url("/home/v/AC\\DC/cover.jpg"),
            "file:///home/v/AC\\DC/cover.jpg"
        );
    }

    #[test]
    fn absolute_path_shape_is_host_independent() {
        assert!(is_local_abs_path("/x"));
        assert!(is_local_abs_path("C:\\x"));
        assert!(is_local_abs_path("c:/x"));
        assert!(is_local_abs_path("\\\\nas\\x"));
        assert!(!is_local_abs_path("x/y"));
        assert!(!is_local_abs_path("https://x"));
        assert!(!is_local_abs_path("Album|Artist"));
        assert!(!is_local_abs_path("C:x")); // drive-relative is not absolute
    }
}
