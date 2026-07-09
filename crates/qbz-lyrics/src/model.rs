//! Lyrics domain model.
//!
//! [`LyricsPayload`] keeps the EXACT camelCase serde shape of Tauri's
//! `src-tauri/src/lyrics/mod.rs:15-27` so a later re-pointed `v2_lyrics_get`
//! stays wire-identical for the Svelte store (spec §2.2.2). [`LyricsDoc`] is
//! the structured form both frontends consume — the service parses internally
//! (native wsync or LRC) and returns structured lines, so no frontend
//! re-implements the TS parser.

use serde::{Deserialize, Serialize};

use crate::lrc;

/// Lyrics source provider. Tauri's enum (`src-tauri/src/lyrics/mod.rs:29-50`)
/// plus the new first-party `Qobuz` variant; serialized lowercase
/// (`'lrclib' | 'ovh' | 'qobuz'` on the JS side).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LyricsProvider {
    Lrclib,
    Ovh,
    Qobuz,
}

impl LyricsProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Lrclib => "lrclib",
            Self::Ovh => "ovh",
            Self::Qobuz => "qobuz",
        }
    }

    /// Parse a stored provider string. Unknown values collapse to `Lrclib`
    /// (parity with Tauri `mod.rs:44-49`); `"qobuz"` round-trips (spec §1.4).
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(value: &str) -> Self {
        match value {
            "ovh" => Self::Ovh,
            "qobuz" => Self::Qobuz,
            _ => Self::Lrclib,
        }
    }
}

/// Wire-compatible lyrics payload (Tauri `LyricsPayload`, camelCase serde).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LyricsPayload {
    pub track_id: Option<u64>,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration_secs: Option<u64>,
    pub plain: Option<String>,
    pub synced_lrc: Option<String>,
    pub provider: LyricsProvider,
    pub cached: bool,
}

/// One word inside a synced line (Qobuz wsync only; `start`/`end` are ms from
/// track start). Word-level timing is richer than LRC — preserved natively in
/// the model for word-anchored karaoke fill (spec Addendum 2, Q2 enriched).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Word {
    pub start: i64,
    pub end: i64,
    pub text: String,
}

/// One lyrics line in the domain model.
///
/// - Synced lines carry `time_ms` (and usually `end_ms`); plain lines carry
///   neither.
/// - `words` is `Some` only for Qobuz wsync documents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LyricsLine {
    pub time_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub text: String,
    pub words: Option<Vec<Word>>,
}

/// Parsed, render-ready lyrics document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LyricsDoc {
    pub lines: Vec<LyricsLine>,
    pub synced: bool,
    pub provider: LyricsProvider,
    /// ISO 639-1 codes Qobuz offers translations for (metadata passthrough;
    /// empty for non-Qobuz providers).
    pub translation_langs: Vec<String>,
    /// Songwriter credits display string (Qobuz metadata passthrough).
    pub writers: Option<String>,
}

impl LyricsDoc {
    /// Empty document (status "no lyrics").
    pub fn empty(provider: LyricsProvider) -> Self {
        Self {
            lines: Vec::new(),
            synced: false,
            provider,
            translation_langs: Vec::new(),
            writers: None,
        }
    }

    /// Build from an LRC text blob (fallback-provider path).
    pub fn from_lrc(lrc_text: &str, provider: LyricsProvider) -> Self {
        let lines = lrc::parse_lrc(lrc_text);
        let synced = !lines.is_empty();
        Self {
            lines,
            synced,
            provider,
            translation_langs: Vec::new(),
            writers: None,
        }
    }

    /// Build from a plain text blob (no timestamps).
    pub fn from_plain_text(plain: &str, provider: LyricsProvider) -> Self {
        Self {
            lines: lrc::parse_plain(plain),
            synced: false,
            provider,
            translation_langs: Vec::new(),
            writers: None,
        }
    }

    /// Build from a wire payload. Synced wins over plain: try `synced_lrc`
    /// first; fall to `plain` only when the LRC parse produced zero lines
    /// (parity with the TS `parsePayload`, `lyricsStore.ts:201-214`).
    pub fn from_payload(payload: &LyricsPayload) -> Self {
        if let Some(lrc_text) = payload.synced_lrc.as_deref() {
            if !lrc_text.trim().is_empty() {
                let doc = Self::from_lrc(lrc_text, payload.provider);
                if !doc.lines.is_empty() {
                    return doc;
                }
            }
        }
        if let Some(plain) = payload.plain.as_deref() {
            if !plain.trim().is_empty() {
                return Self::from_plain_text(plain, payload.provider);
            }
        }
        Self::empty(payload.provider)
    }

    /// Build from a cached row. Readers PREFER the native wsync document when
    /// present (amended Q5): word timestamps survive the cache; the LRC column
    /// stays the cross-frontend lingua franca.
    pub fn from_cached(payload: &LyricsPayload, qobuz_wsync_json: Option<&str>) -> Self {
        if payload.provider == LyricsProvider::Qobuz {
            if let Some(json) = qobuz_wsync_json {
                match serde_json::from_str::<crate::wsync::QobuzWsync>(json) {
                    Ok(wsync) => {
                        let doc = wsync.to_doc();
                        if !doc.lines.is_empty() {
                            return doc;
                        }
                    }
                    Err(e) => {
                        log::warn!("[Lyrics] unparsable cached wsync json ({}); falling back to LRC", e);
                    }
                }
            }
        }
        Self::from_payload(payload)
    }

    /// Join line texts with `\n` (uniform plain rendering / copy-lyrics).
    pub fn plain_text(&self) -> String {
        self.lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Cache key: `"{norm_artist}::{norm_title}::{duration_or_0}"`
/// (parity: Tauri `mod.rs:98-103`).
pub fn build_cache_key(title: &str, artist: &str, duration_secs: Option<u64>) -> String {
    let normalized_title = normalize(title);
    let normalized_artist = normalize(artist);
    let duration = duration_secs.unwrap_or(0);
    format!("{}::{}::{}", normalized_artist, normalized_title, duration)
}

/// Normalize = lowercase + whitespace-collapse (parity: Tauri `mod.rs:105-111`).
pub fn normalize(value: &str) -> String {
    value
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_normalizes_and_defaults_duration() {
        assert_eq!(
            build_cache_key("  My   Song ", "The  ARTIST", Some(215)),
            "the artist::my song::215"
        );
        assert_eq!(build_cache_key("A", "B", None), "b::a::0");
    }

    #[test]
    fn provider_round_trips_qobuz_and_collapses_unknowns() {
        for provider in [
            LyricsProvider::Lrclib,
            LyricsProvider::Ovh,
            LyricsProvider::Qobuz,
        ] {
            assert_eq!(LyricsProvider::from_str(provider.as_str()), provider);
        }
        // Unknown strings collapse to Lrclib (Tauri parity).
        assert_eq!(LyricsProvider::from_str("musixmatch"), LyricsProvider::Lrclib);
    }

    #[test]
    fn payload_serde_shape_is_tauri_wire_compatible() {
        let payload = LyricsPayload {
            track_id: Some(42),
            title: "T".into(),
            artist: "A".into(),
            album: None,
            duration_secs: Some(200),
            plain: Some("p".into()),
            synced_lrc: None,
            provider: LyricsProvider::Qobuz,
            cached: true,
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["trackId"], 42);
        assert_eq!(json["durationSecs"], 200);
        assert_eq!(json["syncedLrc"], serde_json::Value::Null);
        assert_eq!(json["provider"], "qobuz");
        assert_eq!(json["cached"], true);
    }

    #[test]
    fn from_payload_prefers_synced_falls_back_to_plain() {
        let mut payload = LyricsPayload {
            track_id: None,
            title: "T".into(),
            artist: "A".into(),
            album: None,
            duration_secs: None,
            plain: Some("line one\nline two".into()),
            synced_lrc: Some("[00:01.00] hello\n[00:03.00] world".into()),
            provider: LyricsProvider::Lrclib,
            cached: false,
        };
        let doc = LyricsDoc::from_payload(&payload);
        assert!(doc.synced);
        assert_eq!(doc.lines.len(), 2);
        assert_eq!(doc.lines[0].text, "hello");

        // Garbage LRC (zero parsed lines) falls back to plain.
        payload.synced_lrc = Some("no timestamps here".into());
        let doc = LyricsDoc::from_payload(&payload);
        assert!(!doc.synced);
        assert_eq!(doc.lines.len(), 2);
        assert_eq!(doc.lines[0].text, "line one");
        assert_eq!(doc.lines[0].time_ms, None);

        // Nothing usable -> empty.
        payload.synced_lrc = None;
        payload.plain = Some("   ".into());
        let doc = LyricsDoc::from_payload(&payload);
        assert!(doc.lines.is_empty());
    }

    #[test]
    fn from_cached_prefers_wsync_over_lrc() {
        let payload = LyricsPayload {
            track_id: Some(1),
            title: "T".into(),
            artist: "A".into(),
            album: None,
            duration_secs: None,
            plain: Some("hello".into()),
            synced_lrc: Some("[00:01.00] hello".into()),
            provider: LyricsProvider::Qobuz,
            cached: true,
        };
        let wsync_json = r#"{"type":"wsync","lang":"en","lines":[
            {"line":"hello","start":1000,"end":2000,
             "words":[{"word":"hello","start":1000,"end":2000}]}
        ]}"#;
        let doc = LyricsDoc::from_cached(&payload, Some(wsync_json));
        assert!(doc.synced);
        let words = doc.lines[0].words.as_ref().expect("native words preserved");
        assert_eq!(words[0].text, "hello");

        // Without the column, the LRC path applies (no words).
        let doc = LyricsDoc::from_cached(&payload, None);
        assert!(doc.synced);
        assert!(doc.lines[0].words.is_none());

        // Corrupt wsync json degrades to the LRC path instead of erroring.
        let doc = LyricsDoc::from_cached(&payload, Some("{not json"));
        assert!(doc.synced);
        assert!(doc.lines[0].words.is_none());
    }

    #[test]
    fn plain_text_joins_lines() {
        let doc = LyricsDoc::from_plain_text("a\n\n b \nc", LyricsProvider::Ovh);
        assert_eq!(doc.plain_text(), "a\nb\nc");
    }
}
