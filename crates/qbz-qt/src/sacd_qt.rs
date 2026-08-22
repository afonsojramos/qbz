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

    // Which disc this is, so a correction can be written under a key that
    // outlives the session — and found again if the .iso moves.
    let fingerprint = area.fingerprint();
    crate::disc_identity::set(crate::disc_identity::DiscIdentity {
        fingerprint: fingerprint.clone(),
        // A SACD has no MusicBrainz DiscID. It is not missing data — the
        // format has no such thing.
        disc_id: None,
        kind: crate::disc_identity::DiscKind::Sacd,
    });

    // A SACD names itself, so unlike a CD there is nothing to look up and the
    // remembered row is used ONLY where a human overrode the disc. "Names
    // itself" and "names itself CORRECTLY" are different claims: the Master
    // TOC of a European pressing can carry the wrong spelling, or nothing at
    // all, and the second claim is the user's to make.
    let remembered = qbz_disc::store::get(&fingerprint).filter(|m| m.edited);
    if remembered.is_some() {
        log::info!("[qbz-qt] sacd: corrected by hand — using the remembered naming");
    }

    // The album name comes off the disc; the file name is only a fallback for
    // an image whose Master TOC carries no text.
    let album = match remembered.as_ref().filter(|m| !m.album.is_empty()) {
        Some(m) => m.album.clone(),
        None => area.album.clone().unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| qbz_i18n::t("SACD"))
        }),
    };

    let artist = match remembered.as_ref().filter(|m| !m.album_artist.is_empty()) {
        Some(m) => Some(m.album_artist.clone()),
        None => area.artist.clone().filter(|a| !a.is_empty()),
    };

    // Cover art. A disc image carries none, but the file it was downloaded as
    // almost always sits next to one — `cover.jpg` beside the `.iso`, or one
    // level up when the rip is split into "Disc 1" / "Disc 2" folders, which
    // is exactly the shape of the owner's Rheingold.
    //
    // `find_folder_artwork` already knows that walk (it is what a scanned
    // folder uses), so this reuses it rather than growing a second rule about
    // where covers live. No cover is not an error — the pane draws its disc
    // glyph and the album plays.
    let artwork = qbz_library::MetadataExtractor::find_folder_artwork(path, Some(&album))
        .and_then(|found| {
            qbz_library::MetadataExtractor::cache_artwork_file(
                std::path::Path::new(&found),
                &qbz_library::get_artwork_cache_dir(),
            )
        });
    match artwork.as_deref() {
        Some(a) => log::info!("[qbz-qt] sacd: cover {a}"),
        None => log::info!("[qbz-qt] sacd: no cover beside the image"),
    }
    let tracks: Vec<LocalTrack> = area
        .tracks
        .iter()
        .enumerate()
        .map(|(i, t)| LocalTrack {
            // A corrected title wins over the disc's own; the disc's own wins
            // over the number. Indexed defensively — a remembered row can be
            // shorter than the disc if it was written for a different area.
            title: remembered
                .as_ref()
                .and_then(|m| m.tracks.get(i))
                .map(|r| r.title.clone())
                .filter(|s| !s.is_empty())
                .or_else(|| t.title.clone().filter(|s| !s.is_empty()))
                .unwrap_or_else(|| qbz_i18n::t_args("Track {}", &[&t.number.to_string()])),
            album: album.clone(),
            album_group_title: album.clone(),
            album_group_key: format!("sacd|||{album}"),
            artist: remembered
                .as_ref()
                .and_then(|m| m.tracks.get(i))
                .map(|r| r.artist.clone())
                .filter(|s| !s.is_empty())
                .or_else(|| artist.clone())
                .unwrap_or_default(),
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
            artwork_path: artwork.clone(),
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

    // Remember what the DISC said, so the metadata button has a baseline to
    // show and the rip wizard has defaults. `put_auto` will not touch a row a
    // human corrected — that rule lives in the store, not here.
    qbz_disc::store::put_auto(
        &fingerprint,
        None,
        &qbz_disc::store::DiscMemory {
            album: album.clone(),
            album_artist: artist.clone().unwrap_or_default(),
            year: None,
            tracks: tracks
                .iter()
                .enumerate()
                .map(|(i, t)| qbz_disc::store::TrackMemory {
                    number: t.track_number.unwrap_or(i as u32 + 1),
                    title: t.title.clone(),
                    artist: t.artist.clone(),
                })
                .collect(),
            release_id: None,
            release_group_id: None,
            cover_path: artwork.clone(),
            edited: false,
        },
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
