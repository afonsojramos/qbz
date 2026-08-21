//! Open the audio CD in the drive as an ephemeral session.
//!
//! A disc is exactly what the ephemeral model is for: content from outside the
//! index, playable, and gone when you take it out. Nothing here touches
//! `library.db`.
//!
//! What it does NOT do is invent metadata. A CD-DA carries no titles — the
//! disc in the owner's drive reports its MCN as all zeros and only its ISRCs
//! are populated — so tracks are named by their number and the album is called
//! "Audio CD". A DiscID lookup is the obvious next step and is deliberately
//! not faked here: a wrong title is worse than an honest number.

use qbz_library::{AudioFormat, LocalTrack};

use crate::local_ephemeral;

/// Human label for a disc we know nothing about beyond its shape.
fn disc_label() -> String {
    qbz_i18n::t("Audio CD")
}

/// Map one TOC entry to the `LocalTrack` the rest of the app already knows how
/// to carry.
///
/// `file_path` is a `cdda:` reference, NOT a path: it is what
/// `LocalSource::playback` parses back into a device and a sector range. That
/// is why nothing downstream may stat it — see `qbz_disc::CdRef`.
fn to_local_track(track: &qbz_disc::cdda::TocTrack, album: &str) -> LocalTrack {
    let reference = qbz_disc::CdRef {
        device: std::path::PathBuf::new(), // filled by the caller
        start_lsn: track.start_lsn,
        sectors: track.sectors,
    };
    let _ = reference;
    LocalTrack {
        // `t_args`, not `tf`: this is one track, not a count. A plural form
        // here would put a bogus singular/plural pair in eight catalogues.
        title: qbz_i18n::t_args("Track {}", &[&track.number.to_string()]),
        album: album.to_string(),
        album_group_title: album.to_string(),
        album_group_key: format!("cdda|||{album}"),
        track_number: Some(track.number as u32),
        disc_number: Some(1),
        duration_secs: track.duration_secs(),
        // A CD is 44.1 kHz / 16-bit / stereo by definition. These are not
        // guesses and not defaults — the format has no other shape.
        sample_rate: qbz_disc::CDDA_SAMPLE_RATE as f64,
        bit_depth: Some(qbz_disc::CDDA_BITS as u32),
        format: AudioFormat::Wav,
        ..Default::default()
    }
}

/// Read the disc in the first drive that has one and publish it as the
/// ephemeral session. Returns the number of audio tracks, or an error string
/// already translated for a toast.
pub fn open_disc() -> Result<usize, String> {
    let devices = qbz_disc::list_devices();
    if devices.is_empty() {
        return Err(qbz_i18n::t("No optical drive found."));
    }

    // Several drives are legal; take the first one that actually has an audio
    // disc rather than assuming /dev/sr0 is the interesting one.
    let mut last_err = None;
    for dev in &devices {
        match qbz_disc::read_toc(dev) {
            Ok(toc) => {
                let album = disc_label();
                let audio: Vec<_> = toc.audio_tracks().collect();
                let skipped = toc.tracks.len() - audio.len();
                if skipped > 0 {
                    // Mixed-mode disc. Say so rather than quietly showing
                    // fewer tracks than the case insert lists.
                    log::info!(
                        "[qbz-qt] cd: {skipped} data track(s) skipped on {}",
                        dev.display()
                    );
                }
                let tracks: Vec<LocalTrack> = audio
                    .iter()
                    .map(|t| {
                        let mut lt = to_local_track(t, &album);
                        lt.file_path = qbz_disc::CdRef {
                            device: dev.clone(),
                            start_lsn: t.start_lsn,
                            sectors: t.sectors,
                        }
                        .to_path_string();
                        lt
                    })
                    .collect();
                let count = tracks.len();
                log::info!(
                    "[qbz-qt] cd: {} — {count} audio tracks, fingerprint {}",
                    dev.display(),
                    toc.fingerprint()
                );
                local_ephemeral::adopt_tracks(&album, tracks);
                return Ok(count);
            }
            Err(e) => {
                log::info!("[qbz-qt] cd: {} unusable: {e}", dev.display());
                last_err = Some(e);
            }
        }
    }
    Err(match last_err {
        Some(qbz_disc::CdError::NoDisc) => qbz_i18n::t("No disc in the drive."),
        Some(qbz_disc::CdError::NotAudio) => qbz_i18n::t("That disc has no audio tracks."),
        Some(e) => format!("{e}"),
        None => qbz_i18n::t("No optical drive found."),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cd_track_carries_the_formats_only_possible_shape() {
        let t = qbz_disc::cdda::TocTrack {
            number: 7,
            start_lsn: 285_735,
            sectors: 70_833,
            is_audio: true,
        };
        let lt = to_local_track(&t, "Audio CD");
        assert_eq!(lt.sample_rate, 44_100.0);
        assert_eq!(lt.bit_depth, Some(16));
        assert_eq!(lt.track_number, Some(7));
        // 15:44, the real length of the owner's longest track.
        assert_eq!(lt.duration_secs, 944);
    }

    #[test]
    fn the_reference_survives_the_round_trip_a_playback_will_make() {
        let r = qbz_disc::CdRef {
            device: std::path::PathBuf::from("/dev/sr0"),
            start_lsn: 46_577,
            sectors: 53_470,
        };
        let s = r.to_path_string();
        // This is the exact test `LocalSource::playback` performs.
        assert!(qbz_disc::CdRef::is_cd_path(&s));
        let back = qbz_disc::CdRef::parse(&s).expect("a reference we just wrote must parse");
        assert_eq!(back.device, r.device);
        assert_eq!(back.start_lsn, 46_577);
        assert_eq!(back.sectors, 53_470);
    }
}
