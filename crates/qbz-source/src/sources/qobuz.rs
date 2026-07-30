//! The Qobuz streaming catalog.
//!
//! Track/meta resolution REUSES `qbz-mixtape`'s existing async free fns
//! (`resolve_qobuz_album` / `_track` / `_playlist`, enqueue.rs:218-327) rather
//! than re-deriving the mappers; that crate is not modified and stays a leaf.

use std::collections::HashMap;
use std::sync::{Mutex, RwLock};

use qbz_models::QueueTrack;

use crate::art::{ArtRef, ArtSize};
use crate::error::SourceError;
use crate::id::{ItemKind, MediaRef, RawRef, SourceId};
use crate::meta::{ItemMeta, QualityHint, SourceBadge};
use crate::playback::PlaybackTicket;
use crate::source::Source;

/// Memo ceiling, mirroring `artwork_qt::MEMO_CAP` (artwork_qt.rs:82).
const MEMO_CAP: usize = 8192;

/// The Qobuz source.
pub struct QobuzSource {
    /// `std::sync::RwLock`, NOT tokio's — this crate has no tokio dep (§8).
    /// Its guard is `!Send`, so `#[async_trait]`'s `Send` future bound makes
    /// "hold the guard across an await" a COMPILE ERROR rather than a rule.
    /// Every accessor clones the client inside a block and drops the guard
    /// before the first `.await`: `myqbz_play_qt.rs:117-130` records that
    /// holding the guard instead DEADLOCKS against the playback path.
    client: RwLock<Option<qbz_qobuz::QobuzClient>>,
    /// The uid the client belongs to. A `bind_user` with a different uid clears
    /// the client even if nobody remembered to call `teardown` — the leak class
    /// this port already shipped once (auth_qt.rs:193-197).
    owner: Mutex<Option<u64>>,
    /// MediaRef id → its CDN cover url, filled by `tracks`/`meta`.
    ///
    /// `artwork` is the CHEAP phase and must not hit the network, and a Qobuz
    /// id carries no cover on its own — so a cold id answers
    /// `Unavailable` rather than blocking a row on an HTTP round-trip.
    art: RwLock<HashMap<String, String>>,
}

impl QobuzSource {
    /// A source with no client published yet.
    pub fn new() -> Self {
        Self {
            client: RwLock::new(None),
            owner: Mutex::new(None),
            art: RwLock::new(HashMap::new()),
        }
    }

    /// Publish the shared Qobuz client. `None` clears it.
    ///
    /// MUST be called AFTER `core.set_session` (auth_qt.rs:145, :284 and the
    /// offline entry), never after `try_init_api`: `qbz-core` replaces its own
    /// client inside `try_init_api` (`qbz-core/src/core.rs:384`), which
    /// `ensure_api_initialized` calls at auth_qt.rs:83 and :277 — i.e. BEFORE
    /// `set_session` on both the login and the silent-restore paths. A
    /// `QobuzSource` holding a client `qbz-core` has since replaced fails
    /// SILENTLY.
    pub fn set_client(&self, c: Option<qbz_qobuz::QobuzClient>) {
        if let Ok(mut slot) = self.client.write() {
            *slot = c;
        }
    }

    /// A CLONE of the client — one clone per call site, replacing the one clone
    /// per call the frontend does today. The guard is dropped before the
    /// caller's first `.await`.
    fn client(&self) -> Result<qbz_qobuz::QobuzClient, SourceError> {
        let c = { self.client.read().ok().and_then(|g| g.clone()) };
        c.ok_or(SourceError::NotConfigured {
            by: SourceId::QOBUZ,
            why: "no client",
        })
    }

    fn backend(msg: String) -> SourceError {
        SourceError::Backend {
            by: SourceId::QOBUZ,
            msg,
        }
    }

    fn memoize_art(&self, id: &str, url: Option<String>) {
        let Some(url) = url.filter(|u| !u.is_empty()) else {
            return;
        };
        if let Ok(mut memo) = self.art.write() {
            if memo.len() >= MEMO_CAP {
                memo.clear();
            }
            memo.insert(id.to_string(), url);
        }
    }

    /// Pre-downscale a Qobuz cover url to a per-cell target size.
    ///
    /// Moved from `myqbz_qt::small_qobuz_url` (myqbz_qt.rs:376-401), whose doc
    /// comment says in capitals "CALLERS MUST GATE THIS ON
    /// `AlbumSource::Qobuz`" — a caller-enforced invariant, and the exact
    /// anti-pattern the seam removes. Here it lives INSIDE the Qobuz source, so
    /// there is no caller left to forget the gate.
    fn small_url(url: &str, target: u32) -> String {
        if url.is_empty() {
            return String::new();
        }
        const TOKENS: [&str; 8] = [
            "_50.jpg", "_100.jpg", "_150.jpg", "_230.jpg", "_300.jpg", "_600.jpg", "_max.jpg",
            "_org.jpg",
        ];
        let lower = url.to_lowercase();
        for tok in TOKENS {
            if let Some(pos) = lower.rfind(tok) {
                let mut out = String::with_capacity(url.len());
                out.push_str(&url[..pos]);
                out.push_str(&format!("_{target}.jpg"));
                out.push_str(&url[pos + tok.len()..]);
                return out;
            }
        }
        url.to_string()
    }

    fn numeric_id(item: &MediaRef) -> Result<u64, SourceError> {
        item.id().parse::<u64>().map_err(|_| SourceError::BadIdShape {
            by: SourceId::QOBUZ,
            id: item.id().to_string(),
            why: "qobuz track / playlist / artist ids are numeric",
        })
    }
}

impl Default for QobuzSource {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse an ISO `YYYY-MM-DD` release date's year.
fn year_of(date: Option<&String>) -> Option<u32> {
    date?.get(..4)?.parse::<u32>().ok()
}

#[async_trait::async_trait]
impl Source for QobuzSource {
    fn id(&self) -> SourceId {
        SourceId::QOBUZ
    }

    fn claim(&self, raw: &RawRef) -> Option<Result<MediaRef, SourceError>> {
        // POSITIVE ownership only. The second arm is the rule
        // `resolve_bulk_qobuz_track_ids` (myqbz_play_qt.rs:246-248) already
        // uses, and the only correct one for an unlabelled queue row. There is
        // deliberately NO "everything else is Qobuz" fallback — that is
        // `myqbz_add_qt::source_from_str`'s `_ => Qobuz` (survey IC-6), which
        // this seam exists to delete.
        if !(raw.source == Some(SourceId::QOBUZ)
            || (raw.source.is_none() && raw.is_local == Some(false)))
        {
            return None;
        }
        let id = raw.id.trim();
        if id.is_empty() {
            return Some(Err(SourceError::BadIdShape {
                by: SourceId::QOBUZ,
                id: raw.id.clone(),
                why: "empty id",
            }));
        }
        let kind = raw.kind.unwrap_or(ItemKind::Album);
        Some(match kind {
            // A Qobuz ALBUM id is opaque and NOT always numeric — a barcode
            // (`0060254702523`) or a base36-ish slug (`e94no900otyrz`), survey
            // §6.1. It stays verbatim.
            ItemKind::Album => Ok(MediaRef::new(SourceId::QOBUZ, ItemKind::Album, id)),
            ItemKind::Track | ItemKind::Playlist | ItemKind::Artist => {
                if id.parse::<u64>().is_ok() {
                    Ok(MediaRef::new(SourceId::QOBUZ, kind, id))
                } else {
                    // enqueue.rs:196-207 parses both and errors on failure;
                    // here it is a named error at the claim point instead.
                    Err(SourceError::BadIdShape {
                        by: SourceId::QOBUZ,
                        id: id.to_string(),
                        why: "qobuz track / playlist / artist ids are numeric",
                    })
                }
            }
            ItemKind::Folder => Err(SourceError::Unsupported {
                by: SourceId::QOBUZ,
                kind: ItemKind::Folder,
            }),
        })
    }

    async fn tracks(&self, item: &MediaRef) -> Result<Vec<QueueTrack>, SourceError> {
        let client = self.client()?;
        // The mappers stay in `qbz-mixtape`; this is the routing only. Their
        // "album X has 0 tracks" case comes back as an Err STRING, so it lands
        // in `Backend` rather than `NotFound` — converting it would mean
        // sniffing the message, which is the class of thing this crate exists
        // to stop doing.
        let tracks = match item.kind() {
            ItemKind::Album => qbz_mixtape::enqueue::resolve_qobuz_album(&client, item.id())
                .await
                .map_err(Self::backend)?,
            ItemKind::Track => {
                qbz_mixtape::enqueue::resolve_qobuz_track(&client, Self::numeric_id(item)?)
                    .await
                    .map_err(Self::backend)?
            }
            ItemKind::Playlist => {
                qbz_mixtape::enqueue::resolve_qobuz_playlist(&client, Self::numeric_id(item)?)
                    .await
                    .map_err(Self::backend)?
            }
            kind => {
                return Err(SourceError::Unsupported {
                    by: SourceId::QOBUZ,
                    kind,
                })
            }
        };
        self.memoize_art(
            item.id(),
            tracks.first().and_then(|t| t.artwork_url.clone()),
        );
        Ok(tracks)
    }

    async fn meta(&self, item: &MediaRef) -> Result<ItemMeta, SourceError> {
        let client = self.client()?;
        let meta = match item.kind() {
            ItemKind::Album => {
                let a = client
                    .get_album(item.id())
                    .await
                    .map_err(|e| Self::backend(e.to_string()))?;
                let m = ItemMeta {
                    title: a.title.clone(),
                    subtitle: a.artist.name.clone(),
                    year: year_of(a.release_date_original.as_ref()),
                    track_count: a.tracks_count.or(a.track_count),
                    duration_secs: a.duration.map(|d| d as u64),
                    quality: QualityHint {
                        bit_depth: a.maximum_bit_depth,
                        sample_rate_khz: a.maximum_sampling_rate,
                        format: "FLAC",
                    },
                    art: ArtRef::None,
                    badge: SourceBadge::Qobuz,
                    // The V2 wire carries the real release type; fall back to
                    // the kind key.
                    kind_label: match a.release_type.as_deref() {
                        Some("ep") => "ep",
                        Some("single") => "single",
                        _ => "album",
                    },
                };
                with_art(m, self, item, a.image.best().cloned())
            }
            ItemKind::Track => {
                let t = client
                    .get_track(Self::numeric_id(item)?)
                    .await
                    .map_err(|e| Self::backend(e.to_string()))?;
                let art = t
                    .album
                    .as_ref()
                    .and_then(|a| a.image.best().cloned());
                let m = ItemMeta {
                    title: t.title.clone(),
                    subtitle: t
                        .performer
                        .as_ref()
                        .map(|p| p.name.clone())
                        .unwrap_or_default(),
                    year: None,
                    track_count: None,
                    duration_secs: Some(t.duration as u64),
                    quality: QualityHint {
                        bit_depth: t.maximum_bit_depth,
                        sample_rate_khz: t.maximum_sampling_rate,
                        format: "FLAC",
                    },
                    art: ArtRef::None,
                    badge: SourceBadge::Qobuz,
                    kind_label: "track",
                };
                with_art(m, self, item, art)
            }
            ItemKind::Playlist => {
                let p = client
                    .get_playlist(Self::numeric_id(item)?)
                    .await
                    .map_err(|e| Self::backend(e.to_string()))?;
                let art = p
                    .image_rectangle
                    .as_ref()
                    .and_then(|v| v.first().cloned())
                    .or_else(|| p.images300.as_ref().and_then(|v| v.first().cloned()))
                    .or_else(|| p.images.as_ref().and_then(|v| v.first().cloned()));
                let m = ItemMeta {
                    title: p.name.clone(),
                    subtitle: p.owner.name.clone(),
                    year: None,
                    track_count: Some(p.tracks_count),
                    duration_secs: Some(p.duration as u64),
                    quality: QualityHint::default(),
                    art: ArtRef::None,
                    badge: SourceBadge::Qobuz,
                    kind_label: "playlist",
                };
                with_art(m, self, item, art)
            }
            kind => {
                return Err(SourceError::Unsupported {
                    by: SourceId::QOBUZ,
                    kind,
                })
            }
        };
        Ok(meta)
    }

    fn artwork(&self, item: &MediaRef, size: ArtSize) -> ArtRef {
        // CHEAP phase: memo only. A Qobuz id carries no cover on its own, and
        // this method must never hit the network.
        let Some(url) = self.art.read().ok().and_then(|m| m.get(item.id()).cloned()) else {
            return ArtRef::Unavailable("qobuz cover not resolved yet");
        };
        let sized = Self::small_url(&url, size.qobuz_px());
        ArtRef::Fetch {
            // The RAW (un-downscaled) url is the stable memo key across sizes;
            // `sized` is what gets fetched (artwork_qt.rs:231-238).
            cache_key: url,
            url: sized,
        }
    }

    async fn playback(
        &self,
        item: &MediaRef,
        _track: &QueueTrack,
    ) -> Result<PlaybackTicket, SourceError> {
        // The core owns the tier walk, the offline-cache lookup and the stream
        // setup (`core().play_track_resolved`); this crate does not link it.
        if item.kind() != ItemKind::Track {
            return Err(SourceError::Unsupported {
                by: SourceId::QOBUZ,
                kind: item.kind(),
            });
        }
        Ok(PlaybackTicket::Catalog {
            track_id: Self::numeric_id(item)?,
        })
    }

    fn bind_user(&self, uid: u64, _dir: &std::path::Path) {
        let previous = self.owner.lock().ok().and_then(|mut o| o.replace(uid));
        if previous.is_some() && previous != Some(uid) {
            // A rebind to a DIFFERENT uid clears the client even if nobody
            // remembered to tear down; the new one arrives separately, after
            // `set_session`.
            self.set_client(None);
            if let Ok(mut m) = self.art.write() {
                m.clear();
            }
        }
    }

    fn teardown(&self) {
        // MANDATORY override: the trait default is a no-op, and inheriting it
        // here would leave the PREVIOUS ACCOUNT's authenticated `QobuzClient`
        // alive in a `'static` registry across a logout, serving the next
        // account's requests.
        self.set_client(None);
        if let Ok(mut o) = self.owner.lock() {
            *o = None;
        }
        if let Ok(mut m) = self.art.write() {
            m.clear();
        }
    }
}

/// Memoize the cover url on the source and stamp the resolved [`ArtRef`] into
/// the metadata being built. One helper for the three `meta` arms.
fn with_art(
    mut meta: ItemMeta,
    src: &QobuzSource,
    item: &MediaRef,
    url: Option<String>,
) -> ItemMeta {
    src.memoize_art(item.id(), url);
    meta.art = Source::artwork(src, item, ArtSize::Card);
    meta
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claimed(raw: &RawRef) -> Result<MediaRef, SourceError> {
        QobuzSource::new()
            .claim(raw)
            .expect("QobuzSource should claim this reference")
    }

    #[test]
    fn owns_opaque_album_ids_verbatim() {
        // Real ids from `mixtape_collection_items` / `album_play_history`
        // (survey §6.1): a barcode AND a base36-ish slug. Neither is parsed.
        for id in ["0060254702523", "e94no900otyrz", "e98mhimqarxfb"] {
            let m = claimed(&RawRef::new("qobuz", ItemKind::Album, id)).expect("qobuz album");
            assert_eq!(m.source(), SourceId::QOBUZ);
            assert_eq!(m.id(), id);
        }
    }

    #[test]
    fn owns_numeric_track_ids() {
        // Real `QueueTrack.id` (survey §6.1).
        let m = claimed(&RawRef::new("qobuz", ItemKind::Track, "139578884")).expect("qobuz track");
        assert_eq!(m.kind(), ItemKind::Track);
        assert_eq!(m.id(), "139578884");
    }

    #[test]
    fn a_non_numeric_track_id_is_a_named_error() {
        // enqueue.rs:196-199 errors here today; now it errors at the claim.
        let err = claimed(&RawRef::new("qobuz", ItemKind::Track, "not-a-number")).unwrap_err();
        assert!(matches!(
            err,
            SourceError::BadIdShape {
                by: SourceId::QOBUZ,
                ..
            }
        ));
    }

    #[test]
    fn never_claims_by_elimination() {
        let q = QobuzSource::new();
        // survey IC-4/IC-6: a bare numeric with no source word is NOT Qobuz's.
        assert!(q
            .claim(&RawRef {
                kind: Some(ItemKind::Track),
                id: "2954".into(),
                ..Default::default()
            })
            .is_none());
        // A local group key is not Qobuz's either.
        assert!(q
            .claim(&RawRef {
                kind: Some(ItemKind::Album),
                id: "/home/u/Music/Artist/Album".into(),
                ..Default::default()
            })
            .is_none());
        // A `plex:` key with the umbrella `is_local = true` is not Qobuz's.
        assert!(q
            .claim(&RawRef {
                kind: Some(ItemKind::Album),
                id: "plex:5677211365378243606".into(),
                is_local: Some(true),
                ..Default::default()
            })
            .is_none());
    }

    #[test]
    fn an_unlabelled_non_local_queue_row_is_qobuz() {
        // myqbz_play_qt.rs:246-248's rule: `is_local == false` is the ONLY
        // correct disambiguator for a bare numeric with no source word.
        let raw = RawRef {
            kind: Some(ItemKind::Track),
            id: "139578884".into(),
            is_local: Some(false),
            ..Default::default()
        };
        assert_eq!(claimed(&raw).expect("qobuz").id(), "139578884");
    }

    #[test]
    fn cover_urls_are_downscaled_per_size_class() {
        // myqbz_qt.rs:378-401 — real CDN url shape (survey §6.1).
        let url = "https://static.qobuz.com/images/covers/23/25/0060254702523_600.jpg";
        assert_eq!(
            QobuzSource::small_url(url, ArtSize::Row.qobuz_px()),
            "https://static.qobuz.com/images/covers/23/25/0060254702523_50.jpg"
        );
        // A url with no size token passes through unchanged.
        assert_eq!(QobuzSource::small_url("https://x/cover.png", 50), "https://x/cover.png");
        assert_eq!(QobuzSource::small_url("", 50), "");
    }

    #[test]
    fn artwork_is_unavailable_not_none_before_a_resolve() {
        let q = QobuzSource::new();
        let item = MediaRef::new(SourceId::QOBUZ, ItemKind::Album, "0060254702523");
        assert!(matches!(
            Source::artwork(&q, &item, ArtSize::Row),
            ArtRef::Unavailable(_)
        ));
    }
}
