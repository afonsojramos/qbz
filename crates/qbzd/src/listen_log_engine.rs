// crates/qbzd/src/listen_log_engine.rs — the daemon's listen log.
//
// A sibling of `scrobble_engine` on the same CoreEvent bus, WITHOUT its
// `settings.enabled` gate: the scrobbler's switch is about sending plays
// elsewhere; the log is local and on by default (its own switch lives in
// `listen_meta.paused`, shared with the desktop's Settings toggle). Until
// this existed a headless streamer contributed zero history.
//
// Edges (events_bridge.rs): `TrackStarted` opens a row and closes the
// previous one — the bus has no TrackEnded (nobody emits it), so the close
// reason is INFERRED: natural when the last position sat within 2 s of the
// duration, skip otherwise. `PositionUpdated` (only sent while playing)
// feeds the accumulator. `PlaybackStateChanged{Stopped}` closes the row with
// the same inference. The task is aborted on shutdown; `shutdown_blocking`
// closes whatever is open first.

use std::sync::Arc;

use qbz_app::listen_log::{meta_from_queue_track, ListenLogger, Origin};
use qbz_models::{CoreEvent, PlaybackState};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use crate::paths::ProfileRoots;

/// The bound logger, so the daemon's shutdown path can close the open row
/// after aborting the task.
pub struct ListenLogTask {
    pub handle: JoinHandle<()>,
    logger: Option<Arc<ListenLogger>>,
}

impl ListenLogTask {
    /// Abort the subscriber and close the row in flight as `shutdown`.
    pub async fn stop(self) {
        self.handle.abort();
        let _ = self.handle.await;
        if let Some(logger) = self.logger {
            logger.shutdown_blocking();
        }
    }
}

/// Open the daemon-root store (`<data>/listen/listen_log.db`, origin
/// `qbzd:<hostname>`) and subscribe. A failed open logs and yields an inert
/// task — the daemon must boot without a history file.
pub async fn spawn(roots: &ProfileRoots, rx: broadcast::Receiver<CoreEvent>) -> ListenLogTask {
    let hostname = qbz_app::settings::bundle::hostname();
    let logger = match ListenLogger::open(roots.data.clone(), Origin::Daemon { hostname }).await {
        Ok(l) => l,
        Err(e) => {
            log::warn!("[listen-log] open failed; the daemon records no history: {e}");
            return ListenLogTask {
                handle: tokio::spawn(async {}),
                logger: None,
            };
        }
    };
    let handle = tokio::spawn(run(Arc::clone(&logger), rx));
    ListenLogTask {
        handle,
        logger: Some(logger),
    }
}

async fn run(logger: Arc<ListenLogger>, mut rx: broadcast::Receiver<CoreEvent>) {
    use broadcast::error::RecvError;
    loop {
        match rx.recv().await {
            Ok(CoreEvent::TrackStarted { track, .. }) => {
                let meta = meta_from_queue_track(&track, None, None, None);
                logger.track_started(meta, true).await;
            }
            Ok(CoreEvent::PositionUpdated { position_secs, .. }) => {
                logger.tick(position_secs * 1_000, true).await;
            }
            Ok(CoreEvent::PlaybackStateChanged { state }) => {
                if state == PlaybackState::Stopped {
                    logger.stopped(true).await;
                }
            }
            Ok(_) => {}
            Err(RecvError::Lagged(n)) => {
                // Position ticks were dropped: the accumulator simply misses
                // them (a gap > 5 s credits nothing), which is the honest
                // outcome — never extrapolate.
                log::debug!("[listen-log] bus lagged by {n} events");
            }
            Err(RecvError::Closed) => return,
        }
    }
}
