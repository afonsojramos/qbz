//! The registry: one process-wide table, and the ONE place a raw reference
//! becomes a normalised one.

use std::sync::{Arc, OnceLock};

use qbz_models::QueueTrack;

use crate::art::{ArtRef, ArtSize};
use crate::error::SourceError;
use crate::id::{MediaRef, RawRef, SourceId};
use crate::meta::ItemMeta;
use crate::playback::PlaybackTicket;
use crate::source::Source;
use crate::sources::{LocalSource, PlexSource, QobuzSource};

/// Every source in the process.
pub struct SourceRegistry {
    sources: Vec<Arc<dyn Source>>,
    // Typed handles for the three that exist. This is NOT a plugin table: the
    // auth path has to hand each of them something only IT understands (a Qobuz
    // client, a Plex creds impl, a user directory + the ephemeral store), and
    // `&Arc<dyn Source>` cannot express that. Three concrete accessors beat one
    // downcast.
    qobuz: Arc<QobuzSource>,
    plex: Arc<PlexSource>,
    local: Arc<LocalSource>,
}

impl SourceRegistry {
    /// THE registration site. Adding a fourth source is one line here.
    ///
    /// Order is NOT load-bearing: [`SourceRegistry::claim`] asks everyone (see
    /// its docs), which removes an invariant a future author could break by
    /// inserting a source in the wrong slot.
    pub fn with_defaults() -> Self {
        let qobuz = Arc::new(QobuzSource::new());
        let plex = Arc::new(PlexSource::new());
        let local = Arc::new(LocalSource::new());
        Self {
            sources: vec![
                qobuz.clone() as Arc<dyn Source>,
                plex.clone() as Arc<dyn Source>,
                local.clone() as Arc<dyn Source>,
            ],
            qobuz,
            plex,
            local,
        }
    }

    /// The registered source with this id.
    pub fn get(&self, id: SourceId) -> Option<&Arc<dyn Source>> {
        self.sources.iter().find(|s| s.id() == id)
    }

    /// The Qobuz source, for `set_client` from the auth path.
    pub fn qobuz(&self) -> &QobuzSource {
        &self.qobuz
    }

    /// The Plex source, for `set_creds` from the settings path.
    pub fn plex(&self) -> &PlexSource {
        &self.plex
    }

    /// The local source.
    ///
    /// PUBLIC because it carries the `library.db` accessors
    /// `local_state::with_db` and `library_db_qt::with_db` forward to (design
    /// §9 stage 1). The pool itself stays crate-private behind them.
    pub fn local(&self) -> &LocalSource {
        &self.local
    }

    /// Normalise a caller's reference — the ONE place id-shape knowledge is
    /// applied, on EVERY path including playback.
    ///
    /// A two-pass walk that refuses to guess:
    ///
    /// **Pass 1 — POSITIVE ownership only.** Every source is asked, and a
    /// source may answer `Some` only on evidence it *owns*, never on "nobody
    /// else looked like it".
    ///
    /// - exactly one `Some(Ok)` → that is the answer;
    /// - any `Some(Err)` → returned immediately. A recognised-but-wrong shape
    ///   wins over a weaker positive claim: that is what turns bug 2 into a log
    ///   line at the point of the mistake instead of a 404 two layers down;
    /// - more than one `Some(Ok)` → [`SourceError::Ambiguous`] listing the
    ///   candidates. It cannot happen with today's three (their predicates are
    ///   disjoint), and the day it can, the registry says so instead of
    ///   picking.
    ///
    /// **Pass 2 — none of the above.** `Ambiguous` when the id is a bare
    /// numeric (genuinely undecidable without a source word, survey §6.3),
    /// otherwise `Unclaimed`. **The registry never falls back to Qobuz** —
    /// `myqbz_add_qt::source_from_str`'s `_ => Qobuz` (survey IC-6) is exactly
    /// the fallback being deleted, and re-creating it here would re-create IC-4
    /// and IC-6 in one place instead of two.
    pub fn claim(&self, raw: &RawRef) -> Result<MediaRef, SourceError> {
        let mut hits: Vec<MediaRef> = Vec::new();
        for s in &self.sources {
            match s.claim(raw) {
                None => {}
                Some(Err(e)) => return Err(e),
                Some(Ok(m)) => hits.push(m),
            }
        }
        match hits.len() {
            1 => Ok(hits.remove(0)),
            0 => {
                if raw.numeric().is_some() {
                    Err(SourceError::Ambiguous {
                        id: raw.id.clone(),
                        kind: raw.kind,
                        candidates: self.sources.iter().map(|s| s.id()).collect(),
                    })
                } else {
                    Err(SourceError::Unclaimed(raw.clone()))
                }
            }
            _ => Err(SourceError::Ambiguous {
                id: raw.id.clone(),
                kind: raw.kind,
                candidates: hits.iter().map(|m| m.source()).collect(),
            }),
        }
    }

    fn owner(&self, item: &MediaRef) -> Result<&Arc<dyn Source>, SourceError> {
        self.get(item.source()).ok_or(SourceError::NotConfigured {
            by: item.source(),
            why: "source is not registered",
        })
    }

    /// Expand an item into playable queue tracks, IN ORDER.
    pub async fn tracks(&self, item: &MediaRef) -> Result<Vec<QueueTrack>, SourceError> {
        self.owner(item)?.tracks(item).await
    }

    /// Claim + expand, for the common call shape.
    pub async fn tracks_of(&self, raw: &RawRef) -> Result<Vec<QueueTrack>, SourceError> {
        let item = self.claim(raw)?;
        self.tracks(&item).await
    }

    /// Row-level display fields, without expanding the item.
    pub async fn meta(&self, item: &MediaRef) -> Result<ItemMeta, SourceError> {
        self.owner(item)?.meta(item).await
    }

    /// The CHEAP artwork phase. BLOCKING; run it on `spawn_blocking`.
    pub fn artwork(&self, item: &MediaRef, size: ArtSize) -> ArtRef {
        match self.get(item.source()) {
            Some(s) => s.artwork(item, size),
            None => ArtRef::Unavailable("source is not registered"),
        }
    }

    /// The EXPENSIVE artwork phase, keyed by [`ArtRef`] rather than
    /// [`MediaRef`] — the windowed pipeline it serves has no item in scope.
    pub fn thumbnail(&self, source: SourceId, art: &ArtRef, size: ArtSize) -> ArtRef {
        match self.get(source) {
            Some(s) => s.thumbnail(art, size),
            None => art.clone(),
        }
    }

    /// THE playback router. Replaces BOTH copies of `play_audible`
    /// (local_playback.rs:237-245 and local_album_actions.rs:439-450).
    ///
    /// It claims FIRST, then dispatches — so the playback path goes through the
    /// SAME single normalisation point as everything else, and `MediaRef`'s
    /// no-public-constructor property actually covers it. Matching on
    /// `track.source` here instead would bypass `claim` entirely and leave bug
    /// 2's structural argument unsupported on the one path bug 2 lives on.
    pub async fn playback(&self, track: &QueueTrack) -> Result<PlaybackTicket, SourceError> {
        let item = self.claim(&RawRef::from_queue_track(track))?;
        self.owner(&item)?.playback(&item, track).await
    }

    /// Bind every source to the active user.
    ///
    /// Call this as the FIRST statement of `auth_qt::bind_per_user_stores`
    /// (auth_qt.rs:202): `myqbz_qt::init_for_user` at `:206` runs the mixtape
    /// migrations through `library_db_qt::with_db(true, …)`, and against an
    /// unbound pool those return `None` and a fresh account can never create a
    /// collection (library_db_qt.rs:41-49).
    pub fn bind_user(&self, uid: u64, dir: &std::path::Path) {
        for s in &self.sources {
            s.bind_user(uid, dir);
        }
    }

    /// Drop every per-user handle and cache. Call from `auth_qt::logout`
    /// (auth_qt.rs:416-442).
    pub fn teardown(&self) {
        for s in &self.sources {
            s.teardown();
        }
    }
}

impl Default for SourceRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

static REGISTRY: OnceLock<SourceRegistry> = OnceLock::new();

/// The process-wide registry.
///
/// ONE per process; bound and torn down by the auth path, never by a view.
/// It is `'static`, which is what lets [`crate::RegistryResolver`] be moved
/// into a spawned task — something `qbz_mixtape::ProdItemResolver<'a, L>`, which
/// borrows a stack-local client clone, cannot do.
pub fn registry() -> &'static SourceRegistry {
    REGISTRY.get_or_init(SourceRegistry::with_defaults)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::ItemKind;

    #[test]
    fn the_three_predicates_are_disjoint_on_the_real_id_table() {
        let r = SourceRegistry::with_defaults();

        // Qobuz album id (barcode) with the source word.
        let m = r
            .claim(&RawRef::new("qobuz", ItemKind::Album, "0060254702523"))
            .unwrap();
        assert_eq!(m.source(), SourceId::QOBUZ);

        // Plex content-hash album key inside a mixtape `Local` item.
        let m = r
            .claim(&RawRef {
                kind: Some(ItemKind::Album),
                id: "plex:5677211365378243606".into(),
                is_local: Some(true),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(m.source(), SourceId::PLEX);

        // Local metadata-mode group key (real row).
        let m = r
            .claim(&RawRef {
                kind: Some(ItemKind::Album),
                id: "HIT ME HARD AND SOFT|Billie Eilish".into(),
                is_local: Some(true),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(m.source(), SourceId::LOCAL);
    }

    #[test]
    fn a_bare_numeric_with_no_evidence_is_refused_not_guessed() {
        // survey §6.3 / IC-4: `2954` can be a local row id, a Plex rating key
        // OR a Qobuz track id. Guessing is what favourites a random Qobuz
        // track when the user hearts a local one.
        let err = SourceRegistry::with_defaults()
            .claim(&RawRef {
                kind: Some(ItemKind::Track),
                id: "2954".into(),
                ..Default::default()
            })
            .unwrap_err();
        assert!(matches!(err, SourceError::Ambiguous { .. }));
    }

    #[test]
    fn an_unknown_shape_is_unclaimed_not_qobuz() {
        let err = SourceRegistry::with_defaults()
            .claim(&RawRef {
                kind: Some(ItemKind::Album),
                id: "who-knows".into(),
                ..Default::default()
            })
            .unwrap_err();
        assert!(matches!(err, SourceError::Unclaimed(_)));
    }

    #[test]
    fn a_recognised_but_wrong_shape_stops_the_walk() {
        // BUG 2's report path: the album boundary key in TRACK position is
        // rejected by Plex, and the walk does not fall through to a weaker
        // claimant and a 404 two layers down.
        let err = SourceRegistry::with_defaults()
            .claim(&RawRef {
                kind: Some(ItemKind::Track),
                id: "plex:5677211365378243606".into(),
                is_local: Some(true),
                ..Default::default()
            })
            .unwrap_err();
        match err {
            SourceError::BadIdShape { by, .. } => assert_eq!(by, SourceId::PLEX),
            other => panic!("expected BadIdShape, got {other:?}"),
        }
    }

    #[test]
    fn every_registered_source_is_reachable_by_id() {
        let r = SourceRegistry::with_defaults();
        for id in [SourceId::QOBUZ, SourceId::PLEX, SourceId::LOCAL] {
            assert_eq!(r.get(id).map(|s| s.id()), Some(id));
        }
    }
}
