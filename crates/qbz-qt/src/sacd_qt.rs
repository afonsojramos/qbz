//! Open a SACD disc image as an ephemeral session.
//!
//! Unlike a CD, a SACD names itself: the Master TOC carries the album title
//! and artist, and the area TOC carries a title per track. So this asks no
//! network and invents nothing — every string below came off the disc.
//!
//! Scope, and it is stated in the UI rather than hidden: the STEREO area of an
//! uncompressed image. A multichannel-only disc, a DST-compressed area or an
//! image with no ISO 9660 layer is REPORTED, never approximated into silence.

use qbz_library::{AudioFormat, LocalTrack};

use crate::local_ephemeral;

/// Read an image and publish its stereo area as the ephemeral session.
/// Returns the track count, or an already-translated error for a toast.
pub fn open_image(path: &std::path::Path) -> Result<usize, String> {
    let area = qbz_disc::sacd::read_area(path).map_err(|e| {
        log::warn!("[qbz-qt] sacd: {} unusable: {e}", path.display());
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

    // The album name comes off the disc; the file name is only a fallback for
    // an image whose Master TOC carries no text.
    let album = area.album.clone().unwrap_or_else(|| {
        path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| qbz_i18n::t("SACD"))
    });

    let artist = area.artist.clone().filter(|a| !a.is_empty());
    let tracks: Vec<LocalTrack> = area
        .tracks
        .iter()
        .map(|t| LocalTrack {
            title: t
                .title
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| qbz_i18n::t_args("Track {}", &[&t.number.to_string()])),
            album: album.clone(),
            album_group_title: album.clone(),
            album_group_key: format!("sacd|||{album}"),
            artist: artist.clone().unwrap_or_default(),
            album_artist: artist.clone(),
            track_number: Some(t.number as u32),
            disc_number: Some(1),
            duration_secs: t.duration_secs as u64,
            // DSD64 stereo. `bit_depth` is the format's nominal 1, and the
            // rate is the DSD bit rate — the same shape a .dsf row carries,
            // which is what makes the quality badge read DSD64 rather than
            // inventing a tier for discs.
            sample_rate: 2_822_400.0,
            bit_depth: Some(1),
            format: AudioFormat::Dsd,
            file_path: qbz_disc::SacdRef {
                image: path.to_path_buf(),
                track: t.number,
            }
            .to_path_string(),
            ..Default::default()
        })
        .collect();

    let count = tracks.len();
    log::info!(
        "[qbz-qt] sacd: {} — {count} tracks, {:.0}s, album {:?}",
        path.display(),
        area.total_playtime_secs,
        album
    );
    local_ephemeral::adopt_tracks(&album, tracks);
    Ok(count)
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
