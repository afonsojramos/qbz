//! Qobuz word-synced (wsync) document — the persistable native form.
//!
//! Amended Q5 (spec Addendum 2): the cache keeps the shared `synced_lrc`
//! column (Tauri-compat, LRC-with-gap-markers emission — lossy at word level
//! only) AND an additive `qobuz_wsync_json` column holding this document.
//! Readers prefer wsync when present.
//!
//! The serialized shape mirrors the Qobuz wire content union exactly
//! (`{"type":"wsync","lang":…,"lines":[{"line","start","end","words":[{"word","start","end"}]}]}`,
//! live captures `qbz-nix-docs/qobuz-api/lyrics-doc-wsync.json`), so a wire
//! content union parses as a [`QobuzWsync`] and a stored [`QobuzWsync`] is
//! wire-shaped. The Qobuz wrapper metadata (`translation_langs`, `writers`)
//! is carried as additive optional fields so it survives the cache.

use serde::{Deserialize, Serialize};

use qbz_qobuz::{QobuzLyricsContent, QobuzLyricsDocument};

use crate::lrc::{emit_lrc, LrcSpan};
use crate::model::{LyricsDoc, LyricsLine, LyricsProvider, Word};

/// Discriminator tag — always `"wsync"` on output; accepts `"lsync"` on input
/// (the yaml's guess, kept as alias like the wire DTO does) and defaults when
/// absent (stored docs are self-describing but tolerant).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum WsyncTag {
    #[default]
    #[serde(rename = "wsync", alias = "lsync")]
    Wsync,
}

/// Native word-synced lyrics document (cache column `qobuz_wsync_json`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QobuzWsync {
    #[serde(rename = "type", default)]
    pub tag: WsyncTag,
    #[serde(default)]
    pub lang: Option<String>,
    #[serde(default)]
    pub lines: Vec<QobuzWsyncLine>,
    /// Wrapper metadata passthrough (additive; absent on wire content unions).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub translation_langs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writers: Option<String>,
}

/// One synced line, wire-shaped. Gap separator lines come as
/// `{"line":"","words":[]}` with no `start`/`end` (live divergence — see
/// `qbz-qobuz/src/lyrics.rs`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QobuzWsyncLine {
    #[serde(default)]
    pub line: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<QobuzWsyncWord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<i64>,
}

/// One word stamp, wire-shaped (`word`/`start`/`end`, ms from track start).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QobuzWsyncWord {
    #[serde(default)]
    pub word: String,
    #[serde(default)]
    pub start: i64,
    #[serde(default)]
    pub end: i64,
}

impl QobuzWsync {
    /// Build from a fetched Qobuz document. `None` unless the document's
    /// content is the synced variant. ALL lines are preserved verbatim
    /// (including empty gap separators) — this is the native form.
    pub fn from_document(doc: &QobuzLyricsDocument) -> Option<Self> {
        match doc.original.as_ref()? {
            QobuzLyricsContent::Synced { lang, lines } => Some(Self {
                tag: WsyncTag::Wsync,
                lang: lang.clone(),
                lines: lines
                    .iter()
                    .map(|line| QobuzWsyncLine {
                        line: line.line.clone(),
                        words: line
                            .words
                            .iter()
                            .map(|word| QobuzWsyncWord {
                                word: word.word.clone(),
                                start: word.start,
                                end: word.end,
                            })
                            .collect(),
                        start: line.start,
                        end: line.end,
                    })
                    .collect(),
                translation_langs: doc.translation_langs.clone(),
                writers: doc.writers.clone(),
            }),
            QobuzLyricsContent::Plain { .. } => None,
        }
    }

    /// Convert to the domain model. Gap separator lines (empty text) and
    /// unplaceable lines (text without a `start` stamp) are skipped — in
    /// wsync the previous line already carries its explicit `end`, so nothing
    /// is lost. Native word stamps are preserved.
    pub fn to_doc(&self) -> LyricsDoc {
        let lines: Vec<LyricsLine> = self
            .lines
            .iter()
            .filter(|line| !line.line.trim().is_empty())
            .filter_map(|line| {
                let start = line.start?;
                Some(LyricsLine {
                    time_ms: Some(start),
                    end_ms: line.end,
                    text: line.line.clone(),
                    words: if line.words.is_empty() {
                        None
                    } else {
                        Some(
                            line.words
                                .iter()
                                .map(|word| Word {
                                    start: word.start,
                                    end: word.end,
                                    text: word.word.clone(),
                                })
                                .collect(),
                        )
                    },
                })
            })
            .collect();
        LyricsDoc {
            synced: !lines.is_empty(),
            lines,
            provider: LyricsProvider::Qobuz,
            translation_langs: self.translation_langs.clone(),
            writers: self.writers.clone(),
        }
    }

    /// Emit the cross-frontend LRC form (gap markers carry end-of-vocal;
    /// lossy at word level only — documented Q5 trade-off).
    pub fn to_lrc(&self) -> String {
        let spans: Vec<LrcSpan> = self
            .lines
            .iter()
            .filter(|line| !line.line.trim().is_empty())
            .filter_map(|line| {
                let start_ms = line.start?;
                Some(LrcSpan {
                    start_ms,
                    end_ms: line.end,
                    text: line.line.clone(),
                })
            })
            .collect();
        emit_lrc(&spans)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_document() -> QobuzLyricsDocument {
        // Shape mirrors the live capture lyrics-doc-wsync.json (LUNCH /
        // Billie Eilish): regular lines with word stamps + a gap separator
        // line without stamps.
        let json = r#"{
            "album_id": "gvcirtodd95kc",
            "track_id": 266725027,
            "translation_langs": ["es", "fr"],
            "publishers": [{"copyright": "(c) Example", "zones": ["WW"]}],
            "writers": "Billie Eilish O'Connell",
            "original": {
                "type": "wsync",
                "lang": "en",
                "lines": [
                    {"line": "Oh, mm-mm", "start": 1750, "end": 4770,
                     "words": [
                        {"word": "Oh,", "start": 1750, "end": 2480},
                        {"word": "mm-mm", "start": 2480, "end": 4770}
                     ]},
                    {"line": "", "words": []},
                    {"line": "I could eat that girl for lunch", "start": 19690, "end": 22180,
                     "words": [
                        {"word": "I", "start": 19690, "end": 19890},
                        {"word": "could", "start": 19890, "end": 20180},
                        {"word": "eat", "start": 20180, "end": 20560},
                        {"word": "that", "start": 20560, "end": 20850},
                        {"word": "girl", "start": 20850, "end": 21310},
                        {"word": "for", "start": 21310, "end": 21600},
                        {"word": "lunch", "start": 21600, "end": 22180}
                     ]}
                ]
            }
        }"#;
        serde_json::from_str(json).expect("valid document fixture")
    }

    #[test]
    fn from_document_preserves_lines_words_and_metadata() {
        let wsync = QobuzWsync::from_document(&sample_document()).expect("synced doc");
        assert_eq!(wsync.lang.as_deref(), Some("en"));
        assert_eq!(wsync.lines.len(), 3); // gap separator preserved natively
        assert_eq!(wsync.lines[0].words.len(), 2);
        assert_eq!(wsync.translation_langs, vec!["es", "fr"]);
        assert_eq!(wsync.writers.as_deref(), Some("Billie Eilish O'Connell"));
    }

    #[test]
    fn from_document_is_none_for_plain() {
        let json = r#"{
            "track_id": 29006863,
            "original": {"type": "plain", "lang": "en", "lines": [{"line": "Night"}]}
        }"#;
        let doc: QobuzLyricsDocument = serde_json::from_str(json).unwrap();
        assert!(QobuzWsync::from_document(&doc).is_none());
    }

    #[test]
    fn to_doc_skips_gaps_and_preserves_words() {
        let wsync = QobuzWsync::from_document(&sample_document()).unwrap();
        let doc = wsync.to_doc();
        assert!(doc.synced);
        assert_eq!(doc.provider, LyricsProvider::Qobuz);
        assert_eq!(doc.lines.len(), 2); // gap separator dropped from render model
        assert_eq!(doc.lines[0].time_ms, Some(1750));
        assert_eq!(doc.lines[0].end_ms, Some(4770));
        let words = doc.lines[1].words.as_ref().expect("words preserved");
        assert_eq!(words.len(), 7);
        assert_eq!(words[6].text, "lunch");
        assert_eq!(words[6].start, 21_600);
        assert_eq!(doc.translation_langs, vec!["es", "fr"]);
        assert_eq!(doc.writers.as_deref(), Some("Billie Eilish O'Connell"));
    }

    #[test]
    fn to_lrc_emits_gap_markers_from_native_ends() {
        let wsync = QobuzWsync::from_document(&sample_document()).unwrap();
        let lrc = wsync.to_lrc();
        // Vocal ends at 4770, next line starts at 19690 -> gap marker.
        assert!(lrc.contains("[00:01.750] Oh, mm-mm"));
        assert!(lrc.contains("[00:04.770]\n"));
        assert!(lrc.contains("[00:19.690] I could eat that girl for lunch"));
        // wsync -> model -> LRC -> parse: bounds recoverable by the parser.
        let parsed = crate::lrc::parse_lrc(&lrc);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].end_ms, Some(4770));
        assert_eq!(parsed[1].end_ms, Some(22_180));
        // …and word stamps are gone in the LRC form (documented lossiness).
        assert!(parsed.iter().all(|line| line.words.is_none()));
    }

    #[test]
    fn stored_json_round_trips_and_stays_wire_shaped() {
        let wsync = QobuzWsync::from_document(&sample_document()).unwrap();
        let json = serde_json::to_string(&wsync).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "wsync");
        assert_eq!(value["lines"][0]["words"][0]["word"], "Oh,");
        let back: QobuzWsync = serde_json::from_str(&json).unwrap();
        assert_eq!(back, wsync);

        // A bare wire content union (no metadata, as fetched from the CDN)
        // parses too.
        let wire = r#"{"type":"wsync","lang":"en","lines":[{"line":"hi","start":1,"end":2,
            "words":[{"word":"hi","start":1,"end":2}]}]}"#;
        let parsed: QobuzWsync = serde_json::from_str(wire).unwrap();
        assert_eq!(parsed.lines.len(), 1);
        assert!(parsed.translation_langs.is_empty());

        // The yaml-era "lsync" tag is tolerated on input.
        let lsync = r#"{"type":"lsync","lines":[{"line":"hi","start":1,"end":2}]}"#;
        let parsed: QobuzWsync = serde_json::from_str(lsync).unwrap();
        assert_eq!(parsed.lines[0].start, Some(1));
    }
}
