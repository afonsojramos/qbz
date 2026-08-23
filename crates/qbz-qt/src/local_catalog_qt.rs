//! Background bootstrap for the derived Local Library catalog.
//!
//! C is intentionally invisible: current Local Library readers remain the
//! session fallback. This module only schedules the frontend-agnostic worker
//! after boot has had time to paint, coalesces progress into logs, and signals
//! bounded cancellation when the Qt event loop exits.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use qbz_local_catalog::{BootstrapOutcome, CatalogError, ProjectionOutcome};

static STARTED: AtomicBool = AtomicBool::new(false);
static CANCELLED: AtomicBool = AtomicBool::new(false);
static CATCHUP_RUNNING: AtomicBool = AtomicBool::new(false);
static CATCHUP_REQUESTED: AtomicBool = AtomicBool::new(false);

pub(crate) fn start() {
    if STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    CANCELLED.store(false, Ordering::Release);
    CATCHUP_RUNNING.store(true, Ordering::Release);
    crate::spawn(async {
        // Component.onCompleted triggers `on_boot` before the first paint.
        // This delay keeps source probes and SQLite open off that critical path.
        tokio::time::sleep(Duration::from_secs(1)).await;
        if CANCELLED.load(Ordering::Acquire) {
            worker_finished();
            return;
        }
        let Some(data_dir) = dirs::data_dir().map(|root| root.join("qbz")) else {
            log::warn!("[local-catalog] fallback=missing-data-directory");
            worker_finished();
            return;
        };
        let result = tokio::task::spawn_blocking(move || {
            let mut last_publish = std::time::Instant::now();
            let mut last_source = None;
            let bootstrap = qbz_local_catalog::bootstrap_legacy_caches_with_progress(
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
            )?;
            let projection = if matches!(bootstrap, BootstrapOutcome::Activated { .. })
                && !CANCELLED.load(Ordering::Acquire)
            {
                let mut last_projection = std::time::Instant::now();
                let mut last_projection_source = None;
                Some(qbz_local_catalog::reconcile_legacy_caches_with_progress(
                    &data_dir,
                    &CANCELLED,
                    |progress| {
                        let source_changed =
                            last_projection_source.as_ref() != Some(&progress.source);
                        if source_changed
                            || progress.source_complete
                            || last_projection.elapsed() >= Duration::from_secs(2)
                        {
                            log::info!(
                                "[local-catalog] phase=catch-up generation={} source={:?} rows={} checkpoint={} complete={} prune_authorized={}",
                                progress.generation,
                                progress.source,
                                progress.rows_written,
                                progress.checkpoint_cursor,
                                progress.source_complete,
                                progress.prune_authorized
                            );
                            last_projection = std::time::Instant::now();
                            last_projection_source = Some(progress.source.clone());
                        }
                    },
                )?)
            } else {
                None
            };
            Ok::<_, CatalogError>((bootstrap, projection))
        })
        .await;
        match result {
            Ok(Ok((bootstrap, projection))) => {
                match bootstrap {
                    BootstrapOutcome::Activated {
                        generation,
                        track_count,
                        resumed_rows,
                    } => log::info!(
                        "[local-catalog] phase=active generation={generation} tracks={track_count} resumed_rows={resumed_rows}"
                    ),
                    BootstrapOutcome::Paused {
                        generation,
                        source,
                        committed_rows,
                    } => log::info!(
                        "[local-catalog] phase=paused generation={generation} source={source:?} committed_rows={committed_rows}"
                    ),
                    BootstrapOutcome::Fallback(reason) => {
                        log::warn!("[local-catalog] fallback={reason:?}")
                    }
                }
                if let Some(projection) = projection {
                    log_projection(projection);
                }
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
        worker_finished();
    });
}

/// Coalesced post-write hook for local scan and media-server sync completion.
/// If the boot worker is still running, one follow-up pass is remembered.
pub(crate) fn request_catch_up() {
    CATCHUP_REQUESTED.store(true, Ordering::Release);
    if CATCHUP_RUNNING.swap(true, Ordering::AcqRel) {
        return;
    }
    crate::spawn(async {
        tokio::time::sleep(Duration::from_millis(500)).await;
        CATCHUP_REQUESTED.store(false, Ordering::Release);
        if CANCELLED.load(Ordering::Acquire) {
            worker_finished();
            return;
        }
        let Some(data_dir) = dirs::data_dir().map(|root| root.join("qbz")) else {
            log::warn!("[local-catalog] fallback=missing-data-directory");
            worker_finished();
            return;
        };
        let result = tokio::task::spawn_blocking(move || {
            let mut last_publish = std::time::Instant::now();
            let mut last_source = None;
            qbz_local_catalog::reconcile_legacy_caches_with_progress(
                &data_dir,
                &CANCELLED,
                |progress| {
                    let source_changed = last_source.as_ref() != Some(&progress.source);
                    if source_changed
                        || progress.source_complete
                        || last_publish.elapsed() >= Duration::from_secs(2)
                    {
                        log::info!(
                            "[local-catalog] phase=catch-up generation={} source={:?} rows={} checkpoint={} complete={} prune_authorized={}",
                            progress.generation,
                            progress.source,
                            progress.rows_written,
                            progress.checkpoint_cursor,
                            progress.source_complete,
                            progress.prune_authorized
                        );
                        last_publish = std::time::Instant::now();
                        last_source = Some(progress.source.clone());
                    }
                },
            )
        })
        .await;
        match result {
            Ok(Ok(outcome)) => log_projection(outcome),
            Ok(Err(CatalogError::InsufficientSpace {
                required_bytes,
                available_bytes,
            })) => log::warn!(
                "[local-catalog] phase=catch-up-paused reason=low-disk required_bytes={required_bytes} available_bytes={available_bytes}"
            ),
            Ok(Err(error)) => log::warn!("[local-catalog] fallback=catch-up-error error={error}"),
            Err(error) => log::warn!("[local-catalog] fallback=catch-up-join error={error}"),
        }
        worker_finished();
    });
}

fn log_projection(outcome: ProjectionOutcome) {
    match outcome {
        ProjectionOutcome::UpToDate {
            generation,
            track_count,
        } => log::info!(
            "[local-catalog] phase=fresh generation={generation} tracks={track_count}"
        ),
        ProjectionOutcome::Activated {
            generation,
            track_count,
            changed_sources,
            resumed_rows,
        } => log::info!(
            "[local-catalog] phase=catch-up-active generation={generation} tracks={track_count} changed_sources={changed_sources} resumed_rows={resumed_rows}"
        ),
        ProjectionOutcome::Paused {
            generation,
            source,
            committed_rows,
        } => log::info!(
            "[local-catalog] phase=catch-up-paused generation={generation} source={source:?} committed_rows={committed_rows}"
        ),
    }
}

fn worker_finished() {
    CATCHUP_RUNNING.store(false, Ordering::Release);
    if CATCHUP_REQUESTED.swap(false, Ordering::AcqRel) && !CANCELLED.load(Ordering::Acquire) {
        request_catch_up();
    }
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
