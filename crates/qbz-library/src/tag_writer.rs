//! Direct embedded-tag writer (frontend-agnostic port of the Tauri
//! `v2_library_write_album_metadata_to_files` lofty loop). The Slint and Tauri
//! frontends both call this so the lofty logic lives in one place. Progress is
//! reported through an `on_progress` closure (no Tauri event bus); the caller
//! orchestrates the DB update + sidecar removal.

use std::path::Path;

use crate::{LibraryError, LocalTrack};

/// Album-level fields written into every file's embedded tags. A `None`
/// (or blank) field REMOVES that tag (direct write is destructive, unlike the
/// override-only sidecar).
pub struct AlbumTagWrite {
    pub album_title: String,
    pub album_artist: String, // "" => remove the AlbumArtist tag
    pub year: Option<u32>,    // None => remove the date
    pub genre: Option<String>,
    pub catalog_number: Option<String>,
}

/// One file's per-track fields.
pub struct TrackTagWrite {
    pub file_path: String,
    pub title: String,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
}

/// Should this file's per-track ARTIST be rewritten to the album artist?
///
/// ARTIST is TRACK scope — a release with guest performers legitimately holds a
/// different artist per track — but the editor offers exactly one artist field.
/// The DB index resolves that by renaming the track artist ONLY where it still
/// equals the value that was uniform across the album before the edit
/// (`Database::update_album_group_metadata`). The file writer has to agree with
/// it, or the two halves of one save disagree.
///
/// It used to call `set_artist(album_artist)` unconditionally on every file,
/// which meant a various-artists release — where the editor leaves the artist
/// field EMPTY precisely because the track artists disagree — had an empty
/// ARTIST written over every one of its files. Irreversibly.
fn should_rename_artist(
    current: Option<&str>,
    previous_uniform: Option<&str>,
    new_artist: &str,
) -> bool {
    if new_artist.trim().is_empty() {
        return false;
    }
    let Some(previous) = previous_uniform.map(str::trim).filter(|p| !p.is_empty()) else {
        // The album had no single prior artist, so there is no value this edit
        // can be said to be renaming. Leave every file's ARTIST alone.
        return false;
    };
    current.map(str::trim) == Some(previous)
}

/// The artist shared by every file, or `None` when they disagree or any is
/// blank. Read from the FILES rather than the library index, because the files
/// are what is about to be overwritten.
fn uniform_file_artist(paths: &[&str]) -> Option<String> {
    use lofty::prelude::*;

    let mut shared: Option<String> = None;
    for path in paths {
        let file = lofty::read_from_path(Path::new(path)).ok()?;
        let tag = file.primary_tag().or_else(|| file.first_tag())?;
        let artist = tag.artist()?.trim().to_string();
        if artist.is_empty() {
            return None;
        }
        match &shared {
            None => shared = Some(artist),
            Some(seen) if *seen == artist => {}
            Some(_) => return None,
        }
    }
    shared
}

/// Write embedded tags to each file. Dedups by `file_path` keeping the FIRST
/// occurrence (order preserved). `on_progress(current, total)` is called
/// BEFORE each file write (1-based; total = deduped count). Partial-failure
/// unsafe by design: returns `Err` on the first failing file with prior files
/// already modified. Does NOT touch the DB or the sidecar.
pub fn write_album_tags_to_files(
    album: &AlbumTagWrite,
    tracks: &[TrackTagWrite],
    mut on_progress: impl FnMut(usize, usize),
) -> Result<(), LibraryError> {
    use lofty::config::WriteOptions;
    use lofty::prelude::*;
    use lofty::tag::{ItemKey, Tag};

    // Dedup by file_path, first wins, original order preserved.
    let mut seen = std::collections::HashSet::new();
    let unique: Vec<&TrackTagWrite> = tracks
        .iter()
        .filter(|t| seen.insert(t.file_path.clone()))
        .collect();
    let total = unique.len();

    // Read the prior per-track artist BEFORE the first write, so the rename
    // rule sees the album as it was rather than as the loop has left it.
    // Skipped entirely when there is no album artist to rename anything to.
    let previous_artist = if album.album_artist.trim().is_empty() {
        None
    } else {
        let paths: Vec<&str> = unique.iter().map(|t| t.file_path.as_str()).collect();
        uniform_file_artist(&paths)
    };

    for (i, track) in unique.iter().enumerate() {
        on_progress(i + 1, total);

        let path = Path::new(&track.file_path);
        if !path.is_file() {
            return Err(LibraryError::Metadata(
                "One or more audio files were not found on disk.".to_string(),
            ));
        }

        let mut tagged_file = lofty::read_from_path(path)
            .map_err(|_| LibraryError::Metadata("Failed to read audio file tags.".to_string()))?;

        // ALWAYS the format's PRIMARY tag type. There used to be a "primary, or
        // else the first tag I can find" fallback here, and it silently threw
        // away the user's edit:
        //
        // lofty will not WRITE a tag type a container only supports READING, and
        // it refuses silently — the save loop skips non-writable tags with
        // `continue` rather than erroring, because callers are not expected to
        // know which ones are read-only. A FLAC that carries an ID3v2 tag (an
        // ordinary shape; plenty of tooling stamps one on) has NO Vorbis
        // comments, so `primary_tag_mut()` was `None` and the fallback handed
        // back that ID3v2 tag. Every edited field went into it, `save_to_path`
        // dropped the lot, and this function returned `Ok(())`. The user pressed
        // Save, saw a success toast, and nothing changed on disk.
        //
        // Reading through `first_tag()` is still fine and still happens in
        // `uniform_file_artist` — read-only tags are, after all, readable. It is
        // only the WRITE target that must be the primary type.
        let primary_type = tagged_file.primary_tag_type();
        if tagged_file.primary_tag_mut().is_none() {
            tagged_file.insert_tag(Tag::new(primary_type));
        }

        {
            let tag = tagged_file.primary_tag_mut().ok_or_else(|| {
                LibraryError::Metadata("Failed to access audio file tags.".to_string())
            })?;

            tag.set_title(track.title.trim().to_string());
            tag.set_album(album.album_title.trim().to_string());

            // ARTIST is track scope — see `should_rename_artist`.
            let current_artist = tag.artist().map(|a| a.into_owned());
            if should_rename_artist(
                current_artist.as_deref(),
                previous_artist.as_deref(),
                &album.album_artist,
            ) {
                tag.set_artist(album.album_artist.trim().to_string());
            }

            if let Some(no) = track.track_number {
                tag.set_track(no);
            }
            if let Some(disc) = track.disc_number {
                tag.set_disk(disc);
            }

            // Album artist (not part of the Accessor trait).
            if album.album_artist.trim().is_empty() {
                tag.remove_key(ItemKey::AlbumArtist);
            } else {
                tag.insert_text(ItemKey::AlbumArtist, album.album_artist.trim().to_string());
            }

            // Year.
            if let Some(year) = album.year {
                tag.set_date(lofty::tag::items::Timestamp {
                    year: year as u16,
                    ..Default::default()
                });
            } else {
                tag.remove_date();
            }

            // Genre.
            if let Some(g) = album
                .genre
                .as_ref()
                .map(|g| g.trim())
                .filter(|g| !g.is_empty())
            {
                tag.set_genre(g.to_string());
            } else {
                tag.remove_genre();
            }

            // Catalog number.
            if let Some(c) = album
                .catalog_number
                .as_ref()
                .map(|c| c.trim())
                .filter(|c| !c.is_empty())
            {
                tag.insert_text(ItemKey::CatalogNumber, c.to_string());
            } else {
                tag.remove_key(ItemKey::CatalogNumber);
            }
        }

        tagged_file
            .save_to_path(path, WriteOptions::default())
            .map_err(|_| {
                LibraryError::Metadata(
                    "Failed to write tags to audio files. Check that the album folder is mounted \
                     read-write and you have permissions."
                        .to_string(),
                )
            })?;
    }

    Ok(())
}

/// Returns `Some(v)` iff every non-blank track shares one
/// `album_artist ?? artist`, else `None`. Empty / all-blank => `None`.
/// Port of the Tauri `library_compute_track_artist_match`.
pub fn compute_track_artist_match(tracks: &[LocalTrack]) -> Option<String> {
    let mut artists: std::collections::HashSet<String> = std::collections::HashSet::new();
    for track in tracks {
        let value = track
            .album_artist
            .as_deref()
            .unwrap_or(track.artist.as_str())
            .trim();
        if value.is_empty() {
            continue;
        }
        artists.insert(value.to_string());
        if artists.len() > 1 {
            return None;
        }
    }
    artists.into_iter().next()
}

// ─── Purchase downloads ──────────────────────────────────────────────────────

/// Everything a freshly downloaded purchased track should carry in its embedded
/// tags. Every field comes from the API payload the download already fetched
/// (`/track/get` runs before every purchase download), so nothing here is
/// guessed from the file.
#[derive(Debug, Clone, Default)]
pub struct PurchaseTagWrite {
    pub title: String,
    /// Track subtitle ("Live", "Remastered"). Written to its own tag rather than
    /// glued onto TITLE, so the value survives losslessly and TITLE keeps
    /// matching the on-disk filename.
    pub version: Option<String>,
    /// The track's OWN performer. This is the field that makes a purpose-built
    /// writer necessary — see [`write_purchase_tags`].
    pub artist: String,
    pub album_title: String,
    pub album_artist: String,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub year: Option<u32>,
    pub genre: Option<String>,
    pub label: Option<String>,
    pub isrc: Option<String>,
    pub composer: Option<String>,
    pub copyright: Option<String>,
}

/// Tag a freshly downloaded purchased file, optionally embedding cover art.
///
/// **Why this is not `write_album_tags_to_files`.** That writer serves the
/// metadata EDITOR, and two of its defining behaviours are wrong here:
///
///  1. It never writes ARTIST outright. It renames the track artist only where
///     the file's existing value still matches the album's previously-uniform
///     one (`should_rename_artist`), and it derives that prior value by READING
///     the files. A just-downloaded file has no tags at all, so the uniform
///     lookup yields `None`, the rename is declined, and every purchased track
///     would land with NO artist. A purchase knows the real per-track performer
///     from the API, so it writes it directly and skips the rename machinery.
///  2. It is destructive by design — a `None` field REMOVES that tag, which is
///     what an editor means by clearing a field. A download has nothing to
///     clear, so absent fields are skipped instead.
///
/// It is also single-file and independent: the album loop tags each track as it
/// lands, so a cancelled or partly-failed album still leaves correct tags on
/// whatever did download.
///
/// **Failure is never fatal to the download.** The file is the deliverable; tags
/// are an enhancement over the reference, which wrote none at all. Callers log
/// the error and move on.
pub fn write_purchase_tags(
    file_path: &str,
    meta: &PurchaseTagWrite,
    cover_jpeg: Option<&[u8]>,
) -> Result<(), LibraryError> {
    use lofty::config::WriteOptions;
    use lofty::picture::{MimeType, Picture, PictureType};
    use lofty::prelude::*;
    use lofty::tag::{ItemKey, Tag};

    let path = Path::new(file_path);
    let mut tagged_file = lofty::read_from_path(path)
        .map_err(|e| LibraryError::Metadata(format!("Failed to read downloaded file tags: {e}")))?;

    // ALWAYS the format's PRIMARY tag type — never a "first tag we can find"
    // fallback. This is the difference between tagging the file and silently
    // tagging nothing:
    //
    // lofty will not WRITE a tag type a container only supports READING. A FLAC
    // may legitimately arrive carrying an ID3v2 tag; for such a file
    // `primary_tag_mut()` (VorbisComments) is `None` while `first_tag_mut()` is
    // `Some(id3v2)`. A fallback would happily fill that ID3v2 tag, and then
    // `save_to_path` would SKIP it — `tagged_file.rs` does `continue` on any
    // non-writable tag rather than erroring, precisely because callers are not
    // expected to know which ones are read-only. The result is a file with none
    // of its tags and a `Ok(())` return: no log line, no failure, nothing to
    // notice. The same trap costs six fields plus all artwork on an MP3 that
    // carries only ID3v1, whose key set is eight entries wide.
    //
    // Inserting the primary type is safe unconditionally — `insert_tag` replaces
    // any same-type tag — but it is done only when absent so an existing tag's
    // unrelated fields survive.
    let primary_type = tagged_file.primary_tag_type();
    if tagged_file.primary_tag_mut().is_none() {
        tagged_file.insert_tag(Tag::new(primary_type));
    }

    {
        let tag = tagged_file.primary_tag_mut().ok_or_else(|| {
            LibraryError::Metadata("Failed to access downloaded file tags.".to_string())
        })?;

        let set_text = |tag: &mut Tag, key: ItemKey, value: Option<&str>| {
            if let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) {
                tag.insert_text(key, value.to_string());
            }
        };

        // Empties are SKIPPED, not written. A download has nothing to clear, and
        // an explicit empty TITLE/ALBUM is worse than an absent one — it defeats
        // any later repair pass that looks for missing tags.
        if !meta.title.trim().is_empty() {
            tag.set_title(meta.title.trim().to_string());
        }
        if !meta.album_title.trim().is_empty() {
            tag.set_album(meta.album_title.trim().to_string());
        }

        // Written unconditionally, from the API — never inferred from the file.
        if !meta.artist.trim().is_empty() {
            tag.set_artist(meta.artist.trim().to_string());
        }

        // `Track.track_number` is a non-optional `u32` that DEFAULTS to 0 when
        // the API omits it, so an unguarded write stamps `TRACKNUMBER=0` on
        // singles and unnumbered releases. Zero is not a track number.
        if let Some(no) = meta.track_number.filter(|n| *n > 0) {
            tag.set_track(no);
        }
        if let Some(disc) = meta.disc_number.filter(|d| *d > 0) {
            tag.set_disk(disc);
        }
        if let Some(year) = meta.year {
            tag.set_date(lofty::tag::items::Timestamp {
                year: year as u16,
                ..Default::default()
            });
        }
        if let Some(genre) = meta.genre.as_deref().map(str::trim).filter(|g| !g.is_empty()) {
            tag.set_genre(genre.to_string());
        }

        set_text(tag, ItemKey::AlbumArtist, Some(meta.album_artist.as_str()));
        set_text(tag, ItemKey::TrackSubtitle, meta.version.as_deref());
        set_text(tag, ItemKey::Label, meta.label.as_deref());
        set_text(tag, ItemKey::Isrc, meta.isrc.as_deref());
        set_text(tag, ItemKey::Composer, meta.composer.as_deref());
        set_text(tag, ItemKey::CopyrightMessage, meta.copyright.as_deref());

        if let Some(bytes) = cover_jpeg.filter(|b| !b.is_empty()) {
            // Drop any existing embedded art before pushing. On the download path
            // the file is always brand new, so today this is a no-op — but
            // `push_picture` APPENDS, so the moment anything re-tags an existing
            // file (a retry against a file the rename did not replace, or a
            // future re-tag action) the art would accumulate a second copy and
            // bloat every track by the size of the cover.
            //
            // BOTH types are removed, not just `CoverFront`: vendor-tagged files
            // commonly store the front cover as `Other`, and clearing only
            // `CoverFront` would leave that one in place and produce exactly the
            // duplicate this guard exists to prevent.
            tag.remove_picture_type(PictureType::CoverFront);
            tag.remove_picture_type(PictureType::Other);
            // `unchecked` skips lofty's format sniffing: the bytes come from the
            // album's `image.large`, which Qobuz serves as JPEG, and the mime is
            // declared rather than inferred.
            tag.push_picture(
                Picture::unchecked(bytes.to_vec())
                    .pic_type(PictureType::CoverFront)
                    .mime_type(MimeType::Jpeg)
                    .build(),
            );
        }
    }

    tagged_file
        .save_to_path(path, WriteOptions::default())
        .map_err(|e| LibraryError::Metadata(format!("Failed to write downloaded file tags: {e}")))
}

#[cfg(test)]
mod editor_write_target_tests {
    use super::*;

    /// A FLAC that arrives carrying an ID3v2 tag — an ordinary shape, and the
    /// one that exposed a silent data-loss bug in the metadata editor.
    fn flac_with_leading_id3v2() -> Vec<u8> {
        let mut frame = b"TIT2".to_vec();
        let text = b"\x03OldTitle"; // 0x03 = UTF-8
        frame.extend_from_slice(&(text.len() as u32).to_be_bytes());
        frame.extend_from_slice(&[0x00, 0x00]);
        frame.extend_from_slice(text);

        let size = frame.len() as u32;
        let syncsafe = [
            ((size >> 21) & 0x7F) as u8,
            ((size >> 14) & 0x7F) as u8,
            ((size >> 7) & 0x7F) as u8,
            (size & 0x7F) as u8,
        ];

        let mut out = b"ID3".to_vec();
        out.extend_from_slice(&[0x04, 0x00, 0x00]);
        out.extend_from_slice(&syncsafe);
        out.extend_from_slice(&frame);

        // A minimal but valid FLAC: the marker plus one final STREAMINFO block.
        out.extend_from_slice(b"fLaC");
        out.extend_from_slice(&[0x80, 0x00, 0x00, 0x22]);
        let mut info = vec![0u8; 34];
        info[0..2].copy_from_slice(&4096u16.to_be_bytes());
        info[2..4].copy_from_slice(&4096u16.to_be_bytes());
        let sample_rate: u32 = 44_100;
        info[10] = (sample_rate >> 12) as u8;
        info[11] = ((sample_rate >> 4) & 0xFF) as u8;
        info[12] = (((sample_rate & 0x0F) as u8) << 4) | (1 << 1);
        info[13] = 0xF0;
        out.extend_from_slice(&info);
        out
    }

    /// The metadata editor must write into the tag type the container can
    /// actually SAVE, not merely into the first tag the file happens to carry.
    ///
    /// This is the regression for a silent data-loss bug: the write target used
    /// to fall back to `first_tag_mut()`, which for this file is the ID3v2 tag.
    /// lofty refuses to write ID3v2 to FLAC and skips it WITHOUT erroring, so
    /// every edited field was discarded while the call returned `Ok(())` — the
    /// user pressed Save, got a success toast, and the file never changed.
    ///
    /// Verified to fail against the old target selection before being accepted.
    #[test]
    fn editing_a_flac_that_carries_id3v2_actually_writes() {
        use lofty::prelude::*;

        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("legacy.flac");
        std::fs::write(&file, flac_with_leading_id3v2()).unwrap();

        // The fixture must genuinely present the trap, or the test proves nothing.
        let before = lofty::read_from_path(&file).expect("readable FLAC fixture");
        assert!(
            before.tag(lofty::tag::TagType::Id3v2).is_some(),
            "fixture must carry ID3v2"
        );
        assert!(
            before.tag(lofty::tag::TagType::VorbisComments).is_none(),
            "fixture must not already have Vorbis comments"
        );

        let album = AlbumTagWrite {
            album_title: "Edited Album".to_string(),
            album_artist: "Edited Artist".to_string(),
            year: Some(1999),
            genre: Some("Jazz".to_string()),
            catalog_number: None,
        };
        let tracks = vec![TrackTagWrite {
            file_path: file.to_string_lossy().to_string(),
            title: "Edited Title".to_string(),
            track_number: Some(4),
            disc_number: Some(2),
        }];

        write_album_tags_to_files(&album, &tracks, |_, _| {}).expect("the edit must succeed");

        let after = lofty::read_from_path(&file).expect("re-read");
        let vorbis = after
            .tag(lofty::tag::TagType::VorbisComments)
            .expect("the edit must land in VORBIS COMMENTS — the type FLAC can write");

        assert_eq!(vorbis.title().as_deref(), Some("Edited Title"));
        assert_eq!(vorbis.album().as_deref(), Some("Edited Album"));
        assert_eq!(vorbis.track(), Some(4));
        assert_eq!(vorbis.disk(), Some(2));
        assert_eq!(vorbis.genre().as_deref(), Some("Jazz"));
    }
}

#[cfg(test)]
mod tests {
    use super::should_rename_artist;

    #[test]
    fn renames_only_the_artist_the_album_actually_had() {
        // The ordinary case: one artist across the album, user renames it.
        assert!(should_rename_artist(
            Some("Seiji Yokoyama"),
            Some("Seiji Yokoyama"),
            "Yokoyama Seiji"
        ));
        // A guest track inside an otherwise uniform album keeps its performer.
        assert!(!should_rename_artist(
            Some("MAKE-UP"),
            Some("Seiji Yokoyama"),
            "Yokoyama Seiji"
        ));
    }

    #[test]
    fn never_blanks_the_track_artist() {
        // The various-artists shape: the editor leaves the field empty because
        // the track artists disagree. Writing that empty value over 247 files
        // is the data loss this guard exists to stop.
        assert!(!should_rename_artist(Some("MAKE-UP"), None, ""));
        assert!(!should_rename_artist(Some("MAKE-UP"), Some("MAKE-UP"), ""));
        assert!(!should_rename_artist(Some("MAKE-UP"), Some("MAKE-UP"), "   "));
    }

    #[test]
    fn no_uniform_prior_artist_means_nothing_is_renamed() {
        assert!(!should_rename_artist(Some("MAKE-UP"), None, "Various Artists"));
        assert!(!should_rename_artist(Some("MAKE-UP"), Some(""), "Various Artists"));
        assert!(!should_rename_artist(None, Some("MAKE-UP"), "Yokoyama Seiji"));
    }
}
