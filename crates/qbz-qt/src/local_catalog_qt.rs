//! Background bootstrap for the derived Local Library catalog.
//!
//! C is intentionally invisible: current Local Library readers remain the
//! session fallback. This module only schedules the frontend-agnostic worker
//! after boot has had time to paint, coalesces progress into logs, and signals
//! bounded cancellation when the Qt event loop exits.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use qbz_local_catalog::{BootstrapOutcome, CatalogError};

static STARTED: AtomicBool = AtomicBool::new(false);
static CANCELLED: AtomicBool = AtomicBool::new(false);

pub(crate) fn start() {
    if STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    CANCELLED.store(false, Ordering::Release);
    crate::spawn(async {
        // Component.onCompleted triggers `on_boot` before the first paint.
        // This delay keeps source probes and SQLite open off that critical path.
        tokio::time::sleep(Duration::from_secs(1)).await;
        if CANCELLED.load(Ordering::Acquire) {
            return;
        }
        let Some(data_dir) = dirs::data_dir().map(|root| root.join("qbz")) else {
            log::warn!("[local-catalog] fallback=missing-data-directory");
            return;
        };
        let result = tokio::task::spawn_blocking(move || {
            let mut last_publish = std::time::Instant::now();
            let mut last_source = None;
            qbz_local_catalog::bootstrap_legacy_caches_with_progress(
                &data_dir,
                &CANCELLED,
                |progress| {
                    let source_changed = last_source.as_ref() != Some(&progress.source);
                    if source_changed
                        || progress.source_complete
                        || last_publish.elapsed() >= Duration::from_secs(2)
                    {
                        log::info!(
                            "[local-catalog] phase=bootstrap generation={} source={:?} rows={} checkpoint={} complete={}",
                            progress.generation,
                            progress.source,
                            progress.committed_rows,
                            progress.checkpoint_cursor,
                            progress.source_complete
                        );
                        last_publish = std::time::Instant::now();
                        last_source = Some(progress.source.clone());
                    }
                },
            )
        })
        .await;
        match result {
            Ok(Ok(BootstrapOutcome::Activated {
                generation,
                track_count,
                resumed_rows,
            })) => log::info!(
                "[local-catalog] phase=active generation={generation} tracks={track_count} resumed_rows={resumed_rows}"
            ),
            Ok(Ok(BootstrapOutcome::Paused {
                generation,
                source,
                committed_rows,
            })) => log::info!(
                "[local-catalog] phase=paused generation={generation} source={source:?} committed_rows={committed_rows}"
            ),
            Ok(Ok(BootstrapOutcome::Fallback(reason))) => {
                log::warn!("[local-catalog] fallback={reason:?}")
            }
            Ok(Err(CatalogError::InsufficientSpace {
                required_bytes,
                available_bytes,
            })) => log::warn!(
                "[local-catalog] phase=paused reason=low-disk required_bytes={required_bytes} available_bytes={available_bytes}"
            ),
            Ok(Err(error)) => log::warn!("[local-catalog] fallback=bootstrap-error error={error}"),
            Err(error) => log::warn!("[local-catalog] fallback=worker-join error={error}"),
        }
    });
}

pub(crate) fn cancel() {
    CANCELLED.store(true, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_latch_is_visible_to_the_worker() {
        CANCELLED.store(false, Ordering::Release);
        cancel();
        assert!(CANCELLED.load(Ordering::Acquire));
        CANCELLED.store(false, Ordering::Release);
    }
}
