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
    /// MusicBrainz release id, which is also a key to its cover art.
    pub release_id: Option<String>,
    /// The RELEASE GROUP id — the album as a work, rather than one pressing
    /// of it. Cover art lives here far more reliably: a specific pressing
    /// often has none of its own, and the owner's Fear Inoculum answers 500
    /// for its release while the group answers 200.
    pub release_group_id: Option<String>,
}

/// Ask MusicBrainz what this disc is.
///
/// Best effort, but NOT silent. The first version swallowed every failure into
/// one `None` and logged "not identified", so a disc nobody has submitted, a
/// dropped connection and a 503 from an overloaded server were indistinguishable
/// — and when the owner hit one, the log could not say which. Every exit below
/// says what happened, because the next person to see "Track 1" needs to know
/// whether to retry, wait, or submit the disc.
///
/// A 503 is RETRIED once after a pause: MusicBrainz rate-limits anonymous
/// clients and explicitly asks them to back off rather than give up. Its 503
/// body is valid JSON with an `error` field, which is exactly why the naive
/// parse mistook it for an unknown disc — `.json()` succeeded and `releases`
/// simply was not there.
async fn lookup_musicbrainz(disc_id: &str) -> Option<DiscMeta> {
    // MusicBrainz requires a descriptive User-Agent and blocks clients that do
    // not send one.
    let client = reqwest::Client::builder()
        .user_agent(concat!(
            "QBZ/",
            env!("CARGO_PKG_VERSION"),
            " (https://github.com/vicrodh/qbz)"
        ))
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| log::warn!("[qbz-qt] cd: http client: {e}"))
        .ok()?;

    let url = qbz_disc::discid::lookup_url(disc_id);
    let mut body: Option<serde_json::Value> = None;

    for attempt in 0..2 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
        }
        let resp = match client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => {
                log::warn!("[qbz-qt] cd: lookup request failed: {e}");
                return None;
            }
        };
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();

        if status.as_u16() == 503 {
            log::warn!(
                "[qbz-qt] cd: MusicBrainz is rate-limiting (503){}",
                if attempt == 0 { ", retrying once" } else { ", giving up" }
            );
            continue;
        }
        if !status.is_success() {
            log::warn!(
                "[qbz-qt] cd: MusicBrainz answered {status}: {}",
                &text[..text.len().min(200)]
            );
            return None;
        }
        match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(v) => {
                body = Some(v);
                break;
            }
            Err(e) => {
                log::warn!(
                    "[qbz-qt] cd: lookup answer was not JSON ({e}): {}",
                    &text[..text.len().min(200)]
                );
                return None;
            }
        }
    }
    let body = body?;

    // An `error` field is how MusicBrainz reports a problem in a 200-shaped
    // body too. Naming it beats reporting an unknown disc.
    if let Some(err) = body.get("error").and_then(|e| e.as_str()) {
        log::warn!("[qbz-qt] cd: MusicBrainz error: {err}");
        return None;
    }

    let releases = body.get("releases").and_then(|r| r.as_array());
    match releases {
        None => {
            log::warn!(
                "[qbz-qt] cd: answer has no `releases` (keys: {:?})",
                body.as_object().map(|o| o.keys().collect::<Vec<_>>())
            );
            return None;
        }
        // THE one case that genuinely means "nobody has submitted this disc".
        Some(r) if r.is_empty() => {
            log::info!("[qbz-qt] cd: MusicBrainz does not know this disc yet");
            return None;
        }
        _ => {}
    }
    let release = releases?.first()?;

    // Several releases can share a disc (different pressings of one record).
    // They share the geometry, so the first is as good as any for titles, and
    // picking one beats a chooser for a difference nobody can hear.
    let media = release
        .get("media")
        .and_then(|m| m.as_array())
        .and_then(|m| m.iter().find(|m| m.get("tracks").is_some()));
    let Some(media) = media else {
        log::warn!("[qbz-qt] cd: release {:?} carries no track list", release.get("title"));
        return None;
    };

    let titles: Vec<String> = media
        .get("tracks")
        .and_then(|t| t.as_array())
        .map(|t| {
            t.iter()
                .filter_map(|x| x.get("title")?.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

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
        release_id: release
            .get("id")
            .and_then(|i| i.as_str())
            .map(str::to_string),
        release_group_id: release
            .get("release-group")
            .and_then(|g| g.get("id"))
            .and_then(|i| i.as_str())
            .map(str::to_string),
    })
}

/// Fetch the front cover for a MusicBrainz release and cache it.
///
/// A CD carries no artwork — the disc is the artwork, and it is in a case the
/// computer cannot see. The Cover Art Archive is keyed by the same release id
/// the disc lookup already returned, so this costs one more request and no
/// new identity.
///
/// Everything about it is optional: no cover, no network, a redirect that
/// leads nowhere — all give `None`, and the pane draws its disc glyph. A
/// missing cover is a cosmetic gap; a wrong one is a lie about what you are
/// holding.
async fn fetch_cover(release_id: &str, group_id: Option<&str>) -> Option<String> {
    // Two keys, in order. A RELEASE is one pressing and often has no art of
    // its own — the owner's disc answers 500 for its release and 200 for its
    // group — while the RELEASE GROUP is the album as a work and is where the
    // canonical cover lives. Trying only the first is why the first version
    // came back empty.
    for key in [
        Some(format!("release/{release_id}")),
        group_id.map(|g| format!("release-group/{g}")),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(p) = fetch_cover_at(&key).await {
            return Some(p);
        }
    }
    None
}

async fn fetch_cover_at(key: &str) -> Option<String> {
    // 500px: the pane draws it at 224 and the grid at 220, so anything larger
    // is bytes nobody looks at.
    let url = format!("https://coverartarchive.org/{key}/front-500");
    let client = reqwest::Client::builder()
        .user_agent(concat!(
            "QBZ/",
            env!("CARGO_PKG_VERSION"),
            " (https://github.com/vicrodh/qbz)"
        ))
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .ok()?;
    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        // 404 is the ordinary answer for something nobody has photographed;
        // 500 happens for a release whose art lives on its group instead.
        log::info!("[qbz-qt] cd: no cover at {key} ({})", resp.status());
        return None;
    }
    let bytes = resp.bytes().await.ok()?;
    if bytes.is_empty() {
        return None;
    }

    // Write it beside the thumbnails the rest of Local Library uses, keyed by
    // the release so a disc re-inserted tomorrow does not fetch it again.
    let dir = qbz_library::get_artwork_cache_dir();
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!("cd-{}.jpg", key.replace('/', "-")));
    if !path.exists() {
        std::fs::write(&path, &bytes).ok()?;
    }
    log::info!("[qbz-qt] cd: cover from {key} — {} bytes", bytes.len());
    qbz_library::MetadataExtractor::cache_artwork_file(&path, &dir)
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

    // The cover comes AFTER the session is on screen, never before it.
    // Measured on the owner's disc: the Cover Art Archive took 9.4 s to
    // answer — it redirects to archive.org, which is slow — and awaiting it
    // here meant ten seconds of nothing happening after a click. The album
    // appears with its names immediately and the art lands when it lands.
    if let Some(id) = meta.release_id.clone() {
        let group = meta.release_group_id.clone();
        crate::spawn(async move {
            if let Some(art) = fetch_cover(&id, group.as_deref()).await {
                local_ephemeral::set_session_artwork(&art);
            }
        });
    }
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
