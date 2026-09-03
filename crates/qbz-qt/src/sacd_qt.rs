//! Open a SACD disc image and adopt its virtual tracks into the local library.
//!
//! Unlike a CD, a SACD names itself: the Master TOC carries the album title
//! and artist, and the area TOC carries a title per track. So this asks no
//! network and invents nothing — every string below came off the disc.
//!
//! Scope, and it is stated in the UI rather than hidden: the STEREO area of an
//! uncompressed image. A multichannel-only disc, a DST-compressed area or an
//! image with no ISO 9660 layer is REPORTED, never approximated into silence.
//!
//! The rows themselves come from `qbz_library::sacd_scan::build_image_rows`,
//! the same builder the folder scan uses for the images it finds; this file
//! only adds what a manual open needs — the session identity, the ephemeral
//! session and the translated fallback labels.

use qbz_library::sacd_scan::{image_facts, observation_token};

use crate::local_ephemeral;

/// Read an image and publish its stereo area as the ephemeral session.
/// Returns the track count, or an already-translated error for a toast.
pub fn open_image(path: &std::path::Path) -> Result<usize, String> {
    let labels = qbz_library::SacdLabels {
        album: qbz_i18n::t("SACD"),
        track: qbz_i18n::t("Track {}"),
    };
    let rows = qbz_library::build_image_rows(path, &labels).map_err(|e| {
        log::warn!("[qbz-qt] sacd: selected image unusable: {e}");
        match e {
            qbz_disc::sacd::SacdError::NoStereoArea => {
                qbz_i18n::t("That image has no stereo audio area.")
            }
            qbz_disc::sacd::SacdError::Dst => {
                qbz_i18n::t("This disc is DST-compressed, which QBZ cannot play yet.")
            }
            qbz_disc::sacd::SacdError::Iso(_) => qbz_i18n::t("That file is not a disc image."),
            other => format!("{other}"),
        }
    })?;

    // Which disc this is, so a correction can be written under a key that
    // outlives the session — and found again if the .iso moves.
    crate::disc_identity::set(crate::disc_identity::DiscIdentity {
        fingerprint: rows.fingerprint.clone(),
        // A SACD has no MusicBrainz DiscID. It is not missing data — the
        // format has no such thing.
        disc_id: None,
        kind: crate::disc_identity::DiscKind::Sacd,
    });

    let count = rows.tracks.len();
    log::info!(
        "[qbz-qt] sacd: {count} tracks, {:.0}s, album {:?}",
        rows.total_playtime_secs,
        rows.album
    );

    // The explicit Scarlet Book parse above is the discovery gate. Only a
    // complete generation reaches the transaction; a missing/NAS-down image,
    // malformed TOC or unsupported DST area leaves prior catalogue rows
    // untouched and still falls back to the in-memory session behaviour.
    persist(rows.import);

    local_ephemeral::adopt_tracks(&rows.album, rows.tracks);
    Ok(count)
}

/// Re-commit corrected naming/artwork from the currently open SACD without
/// blocking the Qt thread. The snapshot is taken before scheduling so a late
/// worker can never adopt rows from a newer session under the old fingerprint.
pub fn persist_current_session(fingerprint: &str) {
    let tracks = crate::local_ephemeral::tracks_snapshot();
    let Some(image) = tracks
        .first()
        .and_then(|track| qbz_disc::SacdRef::parse(&track.file_path))
        .map(|reference| reference.image)
    else {
        return;
    };
    if tracks.iter().any(|track| {
        qbz_disc::SacdRef::parse(&track.file_path)
            .map(|reference| reference.image != image)
            .unwrap_or(true)
    }) {
        log::warn!("[qbz-qt] sacd: refusing a mixed-image session snapshot");
        return;
    }
    let facts = image_facts(&image);
    let import = qbz_library::SacdImageImport {
        fingerprint: fingerprint.to_string(),
        image_path: image.to_string_lossy().into_owned(),
        image_size_bytes: facts.size_bytes,
        image_modified_ns: facts.modified_ns,
        observed_at: observation_token(),
        tracks,
    };
    crate::spawn(async move {
        let _ = tokio::task::spawn_blocking(move || persist(import)).await;
    });
}

fn persist(import: qbz_library::SacdImageImport) -> bool {
    let started = std::time::Instant::now();
    let Some(result) = crate::library_db_qt::with_db(true, |db| db.import_sacd_image(&import))
    else {
        log::warn!("[qbz-qt] sacd: catalogue adoption failed; session remains available");
        return false;
    };
    if result.stale {
        log::debug!("[qbz-qt] sacd: stale catalogue snapshot ignored");
        return true;
    }
    log::info!(
        "[qbz-qt] sacd: catalogue adopted inserted={} updated={} removed={} elapsed_ms={}",
        result.inserted,
        result.updated,
        result.removed,
        started.elapsed().as_millis()
    );
    crate::local_catalog_qt::request_catch_up();
    // Native readers republish after catch-up; this also refreshes the
    // contractually retained legacy fallback immediately.
    crate::local_bridge_ops::reload_browse();
    true
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_sacd_row_carries_the_shape_a_dsd_badge_expects() {
        // The badge reads (format, depth, rate). A disc track has to look
        // exactly like a .dsf row here, or Local Library would need a second
        // rule for discs — and a second rule is a second thing to forget.
        let r = qbz_disc::SacdRef {
            image: std::path::PathBuf::from("/m/d.iso"),
            track: 4,
        };
        assert_eq!(r.to_path_string(), "sacd:/m/d.iso#4");
        assert!(qbz_disc::SacdRef::is_sacd_path(&r.to_path_string()));
    }
}
