//! "Rip this CD into my library": the Qt side of the job.
//!
//! Everything domain-shaped lives in `qbz-rip` (read, encode, tag, name); this
//! file only asks the user where to put it, turns the open session into a
//! plan, and reports progress.
//!
//! The DESTINATION is always asked, never guessed. A ripper that decides where
//! your music goes is a ripper you have to clean up after, and the app already
//! knows several plausible folders — which is exactly why it must not pick one.

use cxx_qt_lib::QString;

use crate::local_bridge::ui;

/// Kick off a rip of the currently open CD session.
pub fn start() {
    if !crate::local_ephemeral::session_is_cd() {
        return;
    }
    crate::spawn(async move {
        let picked = tokio::task::spawn_blocking(crate::local_ephemeral::pick_folder_for_rip)
            .await
            .ok()
            .flatten();
        let Some(dest) = picked else { return };

        let Some(plan) = build_plan(std::path::PathBuf::from(dest)) else {
            crate::toast_qt::error(qbz_i18n::t("Nothing to rip."));
            return;
        };
        let count = plan.tracks.len();
        let album = plan.album.clone();

        ui(|mut b| {
            b.as_mut().set_local_rip_active(true);
            b.as_mut().set_local_rip_progress(QString::from("0%"));
        });

        let outcome = tokio::task::spawn_blocking(move || {
            qbz_rip::rip(&plan, |p| {
                // The bar is per-DISC, not per-track: a listener watching a
                // 79-minute album wants to know how far the album is, and a
                // per-track bar that resets seven times reads like a stall.
                let overall = (p.track_index as f32 + p.fraction) / p.track_count as f32;
                let text = format!(
                    "{}/{} · {}%",
                    p.track_index + 1,
                    p.track_count,
                    (overall * 100.0) as u32
                );
                ui(move |mut b| {
                    b.as_mut().set_local_rip_progress(QString::from(text.as_str()));
                });
                true
            })
        })
        .await;

        ui(|mut b| {
            b.as_mut().set_local_rip_active(false);
            b.as_mut().set_local_rip_progress(QString::default());
        });

        match outcome {
            Ok(Ok(files)) => {
                log::info!("[qbz-qt] rip: {} files written", files.len());
                crate::toast_qt::success(qbz_i18n::tf(
                    "Ripped {} track to your library",
                    "Ripped {} tracks to your library",
                    count as i64,
                    &[&count.to_string()],
                ));
                // The folder is outside the index until somebody scans it.
                // Saying so beats the user wondering why the album is not in
                // Local Library yet.
                log::info!("[qbz-qt] rip: {album:?} — run a folder scan to index it");
            }
            Ok(Err(qbz_rip::RipError::Cancelled)) => {
                log::info!("[qbz-qt] rip cancelled");
            }
            Ok(Err(e)) => {
                log::warn!("[qbz-qt] rip failed: {e}");
                crate::toast_qt::error(format!("{e}"));
            }
            Err(e) => log::warn!("[qbz-qt] rip task failed: {e}"),
        }
    });
}

/// Turn the open session into a plan. Returns `None` when the session is not
/// a CD, or holds no track this can read.
fn build_plan(destination: std::path::PathBuf) -> Option<qbz_rip::RipPlan> {
    let rows = crate::local_ephemeral::tracks_snapshot();
    if rows.is_empty() {
        return None;
    }
    let album = rows[0].album.clone();
    let album_artist = rows[0]
        .album_artist
        .clone()
        .filter(|a| !a.is_empty())
        .unwrap_or_else(|| rows[0].artist.clone());

    let tracks: Vec<qbz_rip::RipTrack> = rows
        .iter()
        .filter_map(|t| {
            let r = qbz_disc::CdRef::parse(&t.file_path)?;
            Some(qbz_rip::RipTrack {
                number: t.track_number.unwrap_or(0),
                title: t.title.clone(),
                artist: if t.artist.is_empty() {
                    album_artist.clone()
                } else {
                    t.artist.clone()
                },
                source: qbz_rip::RipSource::Cd {
                    device: r.device,
                    start_lsn: r.start_lsn,
                    sectors: r.sectors,
                },
            })
        })
        .collect();

    (!tracks.is_empty()).then(|| qbz_rip::RipPlan {
        destination,
        album,
        album_artist,
        year: rows[0].year,
        tracks,
    })
}
