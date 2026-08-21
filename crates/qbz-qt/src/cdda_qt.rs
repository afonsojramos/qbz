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

/// What MusicBrainz knows about a disc, reduced to what a track list needs.
#[derive(Debug, Default)]
pub struct DiscMeta {
    pub album: Option<String>,
    pub artist: Option<String>,
    pub year: Option<i32>,
    /// Track titles in disc order. Shorter than the disc if MusicBrainz and
    /// the drive disagree about the track count — the caller must index
    /// defensively rather than assume alignment.
    pub titles: Vec<String>,
}

/// Ask MusicBrainz what this disc is.
///
/// Best effort by design: no network, a rate limit, an unknown disc or a
/// malformed answer all give `None`, and the caller keeps its honest
/// "Track N" names. A CD that plays with plain numbers is a small
/// disappointment; a CD labelled with the WRONG album is a bug the user has to
/// notice and undo.
async fn lookup_musicbrainz(disc_id: &str) -> Option<DiscMeta> {
    // MusicBrainz requires a descriptive User-Agent and blocks clients that
    // do not send one. It also rate-limits to ~1 request/second, which this
    // respects by only ever asking once per disc open.
    let client = reqwest::Client::builder()
        .user_agent(concat!(
            "QBZ/",
            env!("CARGO_PKG_VERSION"),
            " (https://github.com/vicrodh/qbz)"
        ))
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;

    let body = client
        .get(qbz_disc::discid::lookup_url(disc_id))
        .send()
        .await
        .ok()?
        .json::<serde_json::Value>()
        .await
        .ok()?;

    // The answer can list SEVERAL releases for one disc (different pressings
    // of the same record). They share the geometry, so the first is as good as
    // any for titles — and picking one is better than showing a chooser for a
    // difference the user cannot hear.
    let release = body.get("releases")?.as_array()?.first()?;
    let media = release
        .get("media")?
        .as_array()?
        .iter()
        .find(|m| m.get("tracks").is_some())?;

    let titles: Vec<String> = media
        .get("tracks")?
        .as_array()?
        .iter()
        .filter_map(|t| t.get("title")?.as_str().map(str::to_string))
        .collect();

    let artist = release
        .get("artist-credit")
        .and_then(|a| a.as_array())
        .and_then(|a| a.first())
        .and_then(|a| a.get("name"))
        .and_then(|n| n.as_str())
        .map(str::to_string);

    let year = release
        .get("date")
        .and_then(|d| d.as_str())
        .and_then(|d| d.get(0..4))
        .and_then(|y| y.parse::<i32>().ok());

    Some(DiscMeta {
        album: release
            .get("title")
            .and_then(|t| t.as_str())
            .map(str::to_string),
        artist,
        year,
        titles,
    })
}

/// Read the table of contents of the first drive that has an audio disc.
/// BLOCKING — spinning a drive up takes a second or two.
fn read_first_disc() -> Result<(std::path::PathBuf, qbz_disc::Toc), String> {
    let devices = qbz_disc::list_devices();
    if devices.is_empty() {
        return Err(qbz_i18n::t("No optical drive found."));
    }
    // Several drives are legal; take the first that actually HAS an audio disc
    // rather than assuming /dev/sr0 is the interesting one.
    let mut last_err = None;
    for dev in &devices {
        match qbz_disc::read_toc(dev) {
            Ok(toc) => return Ok((dev.clone(), toc)),
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

/// Read the disc in the drive, name it if MusicBrainz knows it, and publish it
/// as the ephemeral session. Returns the number of audio tracks, or an error
/// string already translated for a toast.
pub async fn open_disc() -> Result<usize, String> {
    let (dev, toc) = tokio::task::spawn_blocking(read_first_disc)
        .await
        .map_err(|e| format!("{e}"))??;

    let audio: Vec<qbz_disc::TocTrack> = toc.audio_tracks().cloned().collect();
    let skipped = toc.tracks.len() - audio.len();
    if skipped > 0 {
        // Mixed-mode disc. Say so rather than quietly showing fewer tracks
        // than the case insert lists.
        log::info!(
            "[qbz-qt] cd: {skipped} data track(s) skipped on {}",
            dev.display()
        );
    }

    // The Disc ID is computed from the AUDIO tracks' geometry, which is what
    // MusicBrainz hashes. Failing to compute one (an empty or absurd disc) is
    // not an error — it just means no names.
    let starts: Vec<u32> = audio.iter().map(|t| t.start_lsn).collect();
    let meta = match qbz_disc::discid::disc_id(&starts, toc.leadout_lsn) {
        Some(id) => {
            log::info!("[qbz-qt] cd: disc id {id}");
            lookup_musicbrainz(&id).await
        }
        None => None,
    }
    .unwrap_or_default();

    let album = meta
        .album
        .clone()
        .filter(|a| !a.is_empty())
        .unwrap_or_else(disc_label);
    if let Some(a) = meta.album.as_deref() {
        log::info!(
            "[qbz-qt] cd: identified as {:?} by {:?} ({} titles)",
            a,
            meta.artist.as_deref().unwrap_or("?"),
            meta.titles.len()
        );
    } else {
        log::info!("[qbz-qt] cd: not identified — tracks keep their numbers");
    }

    let tracks: Vec<LocalTrack> = audio
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let mut lt = to_local_track(t, &album);
            lt.file_path = qbz_disc::CdRef {
                device: dev.clone(),
                start_lsn: t.start_lsn,
                sectors: t.sectors,
            }
            .to_path_string();
            // Index defensively: MusicBrainz and the drive can disagree about
            // the track count (a hidden track, a mixed-mode disc), and pairing
            // them by position without checking is how track 5 gets track 6's
            // name.
            if let Some(title) = meta.titles.get(i).filter(|s| !s.is_empty()) {
                lt.title = title.clone();
            }
            if let Some(artist) = meta.artist.as_deref() {
                lt.artist = artist.to_string();
                lt.album_artist = Some(artist.to_string());
            }
            // `LocalTrack.year` is u32; a release date before year zero is not a
            // thing, so a negative parse is simply dropped.
            lt.year = meta.year.and_then(|y| u32::try_from(y).ok());
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
    Ok(count)
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
