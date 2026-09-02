//! The purchased copy of a Qobuz track, when it is on disk.
//!
//! Contract `qt-frontend/2026-09-01-purchases-local-playback/00-CONTRACT.md`
//! §6, first slice: a Qobuz row keeps its catalog identity (`play_id` is the
//! Qobuz track id, so now-playing, history, scrobbling and QConnect see the
//! same track they always did) and only the BYTES change — the registered
//! download plays instead of the stream. The preference is implicit for now:
//! a purchased file on disk wins; there is no per-album "Play with" select
//! yet (§5.1), and no per-track override (§5.2).
//!
//! The registry row is a claim, not a fact: the user may have moved or
//! deleted the folder, or the disk may be a share that is off today. Every
//! candidate goes through the same bounded reachability probe the local
//! audible step uses, off the UI thread, so a dead mount costs the probe
//! budget and not a wedge. A row that fails the probe is skipped here and
//! pruned by the next Purchases visit (`get_downloaded_purchase_track_ids`).
//!
//! DSD is DSD: a `.dsf`/`.dff` copy rides the additive `DsdFile` ticket the
//! Local Library already uses, and because that path cannot seek, a play that
//! asks for a start offset (session resume, a takeback at position > 0)
//! streams instead (§7.4). Nothing here touches the protected audio path —
//! the ticket is performed by `audible_qt::play_ticket`, the one matcher every
//! source goes through.

use std::path::PathBuf;

use qbz_library::Reach;
use qbz_source::PlaybackTicket;

/// A registered download of a Qobuz track whose file answered the probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PurchasedCopy {
    pub path: PathBuf,
    pub format_id: u32,
}

impl PurchasedCopy {
    /// `.dsf` / `.dff` — the containers the player streams through its DSD
    /// path. Decided by extension exactly as the local source decides it.
    pub fn is_dsd(&self) -> bool {
        is_dsd_path(&self.path)
    }

    /// The ticket that plays this copy under the CATALOG id.
    pub fn ticket(&self, play_id: u64, start_secs: u64) -> PlaybackTicket {
        if self.is_dsd() {
            PlaybackTicket::DsdFile {
                path: self.path.clone(),
                play_id,
            }
        } else {
            PlaybackTicket::File {
                path: self.path.clone(),
                play_id,
                seek_secs: (start_secs > 0).then_some(start_secs as f64),
            }
        }
    }
}

fn is_dsd_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("dsf") || e.eq_ignore_ascii_case("dff"))
        .unwrap_or(false)
}

/// Order the registry rows of one track: best format first, and within a
/// format the newest download first (the order the accessor hands them in).
/// Pure, so it is testable without a database.
pub(crate) fn rank_candidates(rows: Vec<(i64, String)>) -> Vec<(u32, PathBuf)> {
    let mut out: Vec<(u32, PathBuf)> = rows
        .into_iter()
        .map(|(format_id, path)| (format_id.max(0) as u32, PathBuf::from(path)))
        .collect();
    out.sort_by(|a, b| {
        qbz_offline_cache::purchases_service::format_rank(b.0)
            .cmp(&qbz_offline_cache::purchases_service::format_rank(a.0))
    });
    out
}

/// The best purchased copy of `track_id` that is on disk RIGHT NOW, or `None`
/// when the account never downloaded it, every registered file is gone, or
/// the disk holding it is unreachable. Never blocks the caller's thread.
pub(crate) async fn resolve_purchased_copy(track_id: u64) -> Option<PurchasedCopy> {
    let rows = tokio::task::spawn_blocking(move || {
        crate::library_db_qt::with_db(false, |db| {
            db.get_downloaded_purchase_files(track_id as i64)
        })
    })
    .await
    .ok()
    .flatten()?;
    if rows.is_empty() {
        return None;
    }
    let candidates = rank_candidates(rows);
    tokio::task::spawn_blocking(move || {
        for (format_id, path) in candidates {
            match qbz_library::probe_default(&path) {
                Reach::Present => return Some(PurchasedCopy { path, format_id }),
                Reach::Missing => log::info!(
                    "[qbz-qt] purchase: registry row for track {track_id} (format {format_id}) points at a missing file: {}",
                    path.display()
                ),
                Reach::Unreachable => log::warn!(
                    "[qbz-qt] purchase: the disk holding track {track_id} (format {format_id}) is unreachable: {}",
                    path.display()
                ),
            }
        }
        None
    })
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dsd_copy_rides_the_dsd_ticket_under_the_catalog_id() {
        let copy = PurchasedCopy {
            path: PathBuf::from("/m/Various Artists/A [DSF][DSD128]/01 - x.dsf"),
            format_id: 56,
        };
        assert!(copy.is_dsd());
        match copy.ticket(189_898_763, 0) {
            PlaybackTicket::DsdFile { play_id, path } => {
                assert_eq!(play_id, 189_898_763, "the Qobuz id, never a local row id");
                assert_eq!(path, copy.path);
            }
            other => panic!(
                "expected a DsdFile ticket, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn a_flac_copy_rides_the_file_ticket_and_carries_the_start_offset() {
        let copy = PurchasedCopy {
            path: PathBuf::from("/m/A/B [FLAC][24-bit,96kHz]/01 - x.FLAC"),
            format_id: 7,
        };
        assert!(!copy.is_dsd());
        match copy.ticket(42, 0) {
            PlaybackTicket::File { seek_secs, .. } => assert_eq!(seek_secs, None),
            _ => panic!("expected a File ticket"),
        }
        match copy.ticket(42, 30) {
            PlaybackTicket::File {
                seek_secs, play_id, ..
            } => {
                assert_eq!(seek_secs, Some(30.0));
                assert_eq!(play_id, 42);
            }
            _ => panic!("expected a File ticket"),
        }
    }

    #[test]
    fn candidates_are_ranked_best_format_first_and_newest_within_a_format() {
        let ranked = rank_candidates(vec![
            (6, "/cd-new.flac".to_string()),
            (55, "/dsd64.dsf".to_string()),
            (6, "/cd-old.flac".to_string()),
            (5, "/mp3.mp3".to_string()),
        ]);
        let order: Vec<(u32, String)> = ranked
            .into_iter()
            .map(|(f, p)| (f, p.to_string_lossy().into_owned()))
            .collect();
        assert_eq!(
            order,
            vec![
                (55, "/dsd64.dsf".to_string()),
                (6, "/cd-new.flac".to_string()),
                (6, "/cd-old.flac".to_string()),
                (5, "/mp3.mp3".to_string()),
            ]
        );
    }
}
