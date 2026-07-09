//! Sanitization helpers for sensitive values in log output.
//!
//! Masks UUIDs and numeric IDs so logs remain useful for debugging
//! without exposing full identifiers to security scanners or uploaded
//! log files.

use std::fmt::Display;

/// Mask a UUID string, preserving first and last 8 characters.
///
/// `"123e4567-e89b-12d3-a456-426614174000"` → `"123e4567-****-****-****-****14174000"`
///
/// Non-UUID strings are returned with middle characters masked.
pub fn mask_uuid(uuid: &str) -> String {
    // Standard UUID: 8-4-4-4-12 = 36 chars
    if uuid.len() == 36 && uuid.chars().filter(|c| *c == '-').count() == 4 {
        let first = &uuid[..8];
        let last = &uuid[28..];
        format!("{first}-****-****-****-****{last}")
    } else if uuid.len() > 8 {
        let quarter = uuid.len() / 4;
        let first = &uuid[..quarter];
        let last = &uuid[uuid.len() - quarter..];
        format!("{first}****{last}")
    } else {
        "****".to_string()
    }
}

/// Mask a numeric or string ID, preserving at most the first 4 characters.
///
/// `12345678` → `"1234****"`
/// `42` → `"****"`
pub fn mask_id(id: impl Display) -> String {
    let s = id.to_string();
    if s.len() > 4 {
        format!("{}****", &s[..4])
    } else {
        "****".to_string()
    }
}

/// Redact a URL for logging: keep the scheme/host/path and the query
/// parameter *keys*, but replace every query value and any fragment with a
/// placeholder. This keeps navigation logs useful for debugging the flow
/// (which page was reached, which params were present) without exposing the
/// values themselves — authorization codes, access tokens, encoded redirect
/// targets, captcha tokens, etc.
///
/// `"https://play.qobuz.com/discover?code_autorisation=hg8mr52J"`
/// → `"https://play.qobuz.com/discover?code_autorisation=<redacted>"`
///
/// Pure string manipulation so it never panics on malformed input:
/// `"about:blank"` and other query-less URLs are returned unchanged.
pub fn redact_url(raw: &str) -> String {
    // Peel off the fragment first so a `#access_token=...` (implicit flow)
    // can never survive in the output.
    let (before_fragment, had_fragment) = match raw.split_once('#') {
        Some((base, _)) => (base, true),
        None => (raw, false),
    };

    let (base, query) = match before_fragment.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (before_fragment, None),
    };

    let mut out = String::from(base);

    if let Some(query) = query {
        let mut first = true;
        for pair in query.split('&') {
            if pair.is_empty() {
                continue;
            }
            out.push(if first { '?' } else { '&' });
            first = false;
            // Key before the first '=' is kept; the value (and any further
            // '=' inside it, e.g. base64 padding) is dropped.
            let key = pair.split('=').next().unwrap_or("");
            out.push_str(key);
            out.push_str("=<redacted>");
        }
        if first {
            // Query delimiter was present but empty (`...?`).
            out.push('?');
        }
    }

    if had_fragment {
        out.push_str("#<redacted>");
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_uuid_standard() {
        assert_eq!(
            mask_uuid("123e4567-e89b-12d3-a456-426614174000"),
            "123e4567-****-****-****-****14174000"
        );
    }

    #[test]
    fn test_mask_uuid_short() {
        assert_eq!(mask_uuid("abcd"), "****");
    }

    #[test]
    fn test_mask_uuid_non_standard() {
        assert_eq!(mask_uuid("abcdef1234567890"), "abcd****7890");
    }

    #[test]
    fn test_mask_id_long() {
        assert_eq!(mask_id(12345678), "1234****");
    }

    #[test]
    fn test_mask_id_short() {
        assert_eq!(mask_id(42), "****");
    }

    #[test]
    fn test_mask_id_zero() {
        assert_eq!(mask_id(0), "****");
    }

    #[test]
    fn test_mask_id_five_digits() {
        assert_eq!(mask_id(10001), "1000****");
    }

    #[test]
    fn test_redact_url_oauth_code() {
        assert_eq!(
            redact_url("https://play.qobuz.com/discover?code_autorisation=hg8mr52J"),
            "https://play.qobuz.com/discover?code_autorisation=<redacted>"
        );
    }

    #[test]
    fn test_redact_url_multiple_params_and_base64_value() {
        // Captcha-style URL: base64 values with '=' padding must be fully dropped.
        assert_eq!(
            redact_url("https://www.google.com/recaptcha/api2/anchor?ar=1&k=6LesW24a&co=aHR0cHM6Mw..&hl=en"),
            "https://www.google.com/recaptcha/api2/anchor?ar=<redacted>&k=<redacted>&co=<redacted>&hl=<redacted>"
        );
    }

    #[test]
    fn test_redact_url_no_query() {
        assert_eq!(redact_url("about:blank"), "about:blank");
        assert_eq!(
            redact_url("https://www.qobuz.com/signin/oauth"),
            "https://www.qobuz.com/signin/oauth"
        );
    }

    #[test]
    fn test_redact_url_fragment() {
        assert_eq!(
            redact_url("https://example.com/cb#access_token=abc123&id=9"),
            "https://example.com/cb#<redacted>"
        );
    }

    #[test]
    fn test_redact_url_query_and_fragment() {
        assert_eq!(
            redact_url("https://example.com/cb?code=xyz#access_token=abc"),
            "https://example.com/cb?code=<redacted>#<redacted>"
        );
    }
}
