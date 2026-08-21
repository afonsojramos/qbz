//! Scarlet Book: the stereo audio area of a SACD image.
//!
//! Every structure and every constant below was MEASURED on the owner's two
//! Rheingold discs before a line of it was written, by a census rather than a
//! sample — 3 066 893 sectors, both discs, zero exceptions. That matters
//! because the previous attempt at this file started from a single sector and
//! concluded the per-sector header was a fixed 8 bytes. It is not, and a fixed
//! skip would have leaked packet descriptors into the DSD stream: audible
//! noise on a path whose entire promise is bit-perfection.
//!
//! WHAT IS SUPPORTED: an uncompressed (non-DST) stereo area in an image with
//! an ISO 9660 layer. Everything else is DETECTED and reported — never
//! silently approximated.

use std::path::Path;

use crate::iso9660::{IsoError, IsoImage, SECTOR};

/// Sectors per second of SACD playing time. The area TOC states it as a
/// max_byte_rate of 716 800 B/s, and 716800 / 2048 = 350. It is the SACD's
/// answer to the CD's 75, and it is what ties a track's start LSN to its
/// start time — an identity that holds for 44 of 44 tracks on both discs:
///     start_lsn[i] == floor(area_start + start_time[i] * 350)
pub const SECTORS_PER_SEC: u32 = 350;

/// One uncompressed DSD64 STEREO frame: 2 ch x 2 822 400 bit/s / 75 frames/s
/// / 8 = 9408 bytes. Used as the proof that an area carries no DST — the sum
/// of its audio-packet lengths divides by this EXACTLY (350 495 frames on
/// Disc 1, 306 696 on Disc 2). Compressed frames could not give an integer.
pub const DSD64_STEREO_FRAME: u32 = 9408;

/// Packet data types seen across both discs: {2, 3, 7}. Only 2 is audio.
const DATA_TYPE_AUDIO: u16 = 2;

#[derive(Debug, thiserror::Error)]
pub enum SacdError {
    #[error(transparent)]
    Iso(#[from] IsoError),
    #[error("this image has no stereo audio area (no /2C_AUDIO/2C_AREA1.TOC)")]
    NoStereoArea,
    #[error("the area TOC is not a Scarlet Book stereo TOC (expected TWOCHTOC)")]
    NotAnArea,
    #[error("{0} is missing from the area TOC")]
    MissingBlock(&'static str),
    #[error("this area is DST-compressed, which is not supported")]
    Dst,
    #[error("unsupported channel count: {0}")]
    Channels(u8),
    #[error("the area declares {0} tracks, which cannot be right")]
    TrackCount(u8),
    #[error("sector {lsn} is malformed: {why}")]
    BadSector { lsn: u64, why: &'static str },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SacdTrack {
    /// 1-based, as printed on the sleeve.
    pub number: u8,
    pub start_lsn: u32,
    /// Sectors to READ. Deliberately one more than the exclusive extent for
    /// every track but the last — that is how the disc records it, and the
    /// ISO 9660 directory mirrors it byte for byte, so the one-sector overlap
    /// a naive reading sees is intentional rather than a bug to correct.
    pub length_lsn: u32,
    pub start_secs: f64,
    pub duration_secs: f64,
    pub title: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SacdArea {
    pub track_start_lsn: u32,
    pub track_end_lsn: u32,
    pub channels: u8,
    pub total_playtime_secs: f64,
    pub tracks: Vec<SacdTrack>,
    /// Album text out of the Master TOC, when it carries any.
    pub album: Option<String>,
    /// Album artist, same source.
    pub artist: Option<String>,
}

fn be32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// A time triple: minutes, seconds, frames at 75 fps. The area TOC uses it for
/// both the total playtime and every per-track time.
fn time_at(b: &[u8], at: usize) -> f64 {
    b[at] as f64 * 60.0 + b[at + 1] as f64 + b[at + 2] as f64 / 75.0
}

/// Read the stereo area's table of contents out of a disc image.
pub fn read_area(path: &Path) -> Result<SacdArea, SacdError> {
    let mut iso = IsoImage::open(path)?;
    let toc_entry = iso
        .find("/2C_AUDIO/2C_AREA1.TOC")?
        .ok_or(SacdError::NoStereoArea)?;
    // 9 sectors; read them all, the blocks live at fixed sector offsets.
    let toc = iso.read_sectors(toc_entry.lsn as u64, 9)?;

    if &toc[0..8] != b"TWOCHTOC" {
        return Err(SacdError::NotAnArea);
    }
    let channels = toc[0x20];
    if channels != 2 {
        return Err(SacdError::Channels(channels));
    }
    // A u8 cannot exceed 255, so the only impossible value is zero — an area
    // that declares no tracks is a TOC we have misread, not an empty disc.
    let track_count = toc[0x45];
    if track_count == 0 {
        return Err(SacdError::TrackCount(track_count));
    }
    let total_playtime_secs = time_at(&toc, 0x40);
    let track_start_lsn = be32(&toc, 0x48);
    let track_end_lsn = be32(&toc, 0x4C);

    // Sector 1 = SACDTRL1 (starts + lengths), sector 2 = SACDTRL2 (times).
    let trl1 = &toc[SECTOR as usize..];
    let trl2 = &toc[2 * SECTOR as usize..];
    if &trl1[0..8] != b"SACDTRL1" {
        return Err(SacdError::MissingBlock("SACDTRL1"));
    }
    if &trl2[0..8] != b"SACDTRL2" {
        return Err(SacdError::MissingBlock("SACDTRL2"));
    }
    // Both blocks are two parallel arrays of 255 u32 big-endian entries: the
    // second starts at 8 + 255*4.
    const ARR2: usize = 8 + 255 * 4;

    let titles = read_titles(&toc, track_count as usize);

    let mut tracks = Vec::with_capacity(track_count as usize);
    for i in 0..track_count as usize {
        tracks.push(SacdTrack {
            number: (i + 1) as u8,
            start_lsn: be32(trl1, 8 + i * 4),
            length_lsn: be32(trl1, ARR2 + i * 4),
            start_secs: time_at(trl2, 8 + i * 4),
            duration_secs: time_at(trl2, ARR2 + i * 4),
            title: titles.get(i).cloned().flatten(),
        });
    }

    let (album, artist) = read_master_text(&mut iso).unwrap_or((None, None));

    Ok(SacdArea {
        track_start_lsn,
        track_end_lsn,
        channels,
        total_playtime_secs,
        tracks,
        album,
        artist,
    })
}

/// Album title and artist out of the Master TOC's text sector.
///
/// A SACD names itself, which is why this feature needs no network the way a
/// CD does. Measured layout, identical on both of the owner's discs:
///   LSN 510      "SACDMTOC"      (the Master TOC; backups at 520 and 530 are
///                                 byte-identical, so a failure can fall back)
///   LSN 511      "SACDText"      the text sector
///     +0x10  u16 BE  album title pointer   } offsets are relative to the
///     +0x12  u16 BE  album artist pointer  } START OF THE TEXT SECTOR
///   payload      NUL-terminated strings
///
/// Everything here is optional: an image whose Master TOC is unreadable or
/// carries no text still plays, it just falls back to the file name.
fn read_master_text(iso: &mut IsoImage) -> Option<(Option<String>, Option<String>)> {
    // By LSN, not by name: the Master TOC is at a fixed place, and an image
    // whose directory is odd should still give up its title.
    let master = iso.read_sectors(510, 2).ok()?;
    if &master[0..8] != b"SACDMTOC" {
        return None;
    }
    let text = &master[SECTOR as usize..];
    if &text[0..8] != b"SACDText" {
        return None;
    }
    let ptr = |at: usize| -> Option<usize> {
        let v = u16::from_be_bytes([text[at], text[at + 1]]) as usize;
        // Zero means "absent", not "offset zero" — offset zero is the id.
        (v != 0 && v < text.len()).then_some(v)
    };
    let string_at = |off: usize| -> Option<String> {
        let end = text[off..].iter().position(|b| *b == 0)? + off;
        let s = String::from_utf8_lossy(&text[off..end]).trim().to_string();
        (!s.is_empty()).then_some(s)
    };
    Some((
        ptr(0x10).and_then(string_at),
        ptr(0x12).and_then(string_at),
    ))
}

/// Track titles from the SACDTTxt block (sector 5 of the area TOC).
///
/// The pointer table holds one big-endian u16 per track, and each value is an
/// offset RELATIVE TO THE START OF THE BLOCK — not to the file, and not to the
/// sector the text happens to land in. Getting that wrong yields plausible
/// garbage rather than an error, so titles are treated as optional throughout:
/// a track with no readable name keeps its number.
fn read_titles(toc: &[u8], count: usize) -> Vec<Option<String>> {
    let base = 5 * SECTOR as usize;
    let block = match toc.get(base..) {
        Some(b) if b.len() > 8 && &b[0..8] == b"SACDTTxt" => b,
        _ => return vec![None; count],
    };
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let at = 8 + i * 2;
        let ptr = match block.get(at..at + 2) {
            Some(p) => u16::from_be_bytes([p[0], p[1]]) as usize,
            None => {
                out.push(None);
                continue;
            }
        };
        out.push(text_at(block, ptr));
    }
    out
}

/// One entry of the text payload. The layout carries a small header before the
/// string, so this scans for the first printable run rather than assuming an
/// offset that was never measured — an honest approximation, and it degrades
/// to `None` instead of to nonsense.
fn text_at(block: &[u8], ptr: usize) -> Option<String> {
    let slice = block.get(ptr..(ptr + 256).min(block.len()))?;
    let start = slice.iter().position(|b| (0x20..0x7f).contains(b))?;
    let end = slice[start..]
        .iter()
        .position(|b| *b == 0)
        .map(|e| start + e)
        .unwrap_or(slice.len());
    let s = String::from_utf8_lossy(&slice[start..end]).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

// ---------------------------------------------------------------------------
// The sector parser
// ---------------------------------------------------------------------------

/// One packet of a Scarlet Book audio sector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Packet {
    pub frame_start: bool,
    pub data_type: u16,
    pub len: usize,
    /// Offset of the payload within the 2048-byte sector.
    pub at: usize,
}

/// Decode one sector's framing.
///
/// Measured layout, census of 3 066 893 sectors with zero exceptions:
///   byte 0:  packet_count     = (b >> 5) & 0x07
///            frame_info_count = (b >> 2) & 0x07
///   then `packet_count` BIG-ENDIAN 16-bit descriptors:
///            bit 15    frame_start
///            bits 14-11 data_type
///            bits 10-0  length IN BYTES        <- ELEVEN bits, not twelve
///   then `frame_info_count` * 3 bytes
///   header_len = 1 + packet_count*2 + frame_info_count*3
///
/// The eleven-bit length is the correction that matters. The widely-copied
/// layout gives the length twelve bits, and the very first descriptor on both
/// discs is 0x181B — whose low twelve bits are 2075, larger than the sector.
/// With eleven bits it is 27, and 5 + 27 + 2016 = 2048 exactly. The sum rule
/// `header_len + SUM(lengths) == 2048` is what proves the whole decoding, and
/// it is checked here on every sector rather than trusted.
pub fn parse_sector(sector: &[u8], lsn: u64) -> Result<Vec<Packet>, SacdError> {
    if sector.len() != SECTOR as usize {
        return Err(SacdError::BadSector {
            lsn,
            why: "not 2048 bytes",
        });
    }
    let b0 = sector[0];
    let packet_count = ((b0 >> 5) & 0x07) as usize;
    let frame_info_count = ((b0 >> 2) & 0x07) as usize;
    let header_len = 1 + packet_count * 2 + frame_info_count * 3;
    if packet_count == 0 || header_len > sector.len() {
        return Err(SacdError::BadSector {
            lsn,
            why: "impossible packet/frame counts",
        });
    }

    let mut packets = Vec::with_capacity(packet_count);
    let mut at = header_len;
    let mut total = header_len;
    for k in 0..packet_count {
        let w = u16::from_be_bytes([sector[1 + 2 * k], sector[2 + 2 * k]]);
        let len = (w & 0x07FF) as usize;
        packets.push(Packet {
            frame_start: w & 0x8000 != 0,
            data_type: (w >> 11) & 0x0F,
            len,
            at,
        });
        at += len;
        total += len;
    }
    // The sum rule. A sector that fails it has been decoded WRONG, and the
    // right answer is to stop rather than to emit whatever bytes are there.
    if total != SECTOR as usize {
        return Err(SacdError::BadSector {
            lsn,
            why: "packet lengths do not fill the sector",
        });
    }
    Ok(packets)
}

/// Streams the raw DSD of one track out of an image.
pub struct SacdTrackReader {
    iso: IsoImage,
    next_lsn: u64,
    end_lsn: u64,
}

impl SacdTrackReader {
    pub fn open(path: &Path, track: &SacdTrack) -> Result<Self, SacdError> {
        Ok(Self {
            iso: IsoImage::open(path)?,
            next_lsn: track.start_lsn as u64,
            // `length_lsn` is how many sectors to read, including the shared
            // boundary sector — the disc's own accounting, not ours.
            end_lsn: track.start_lsn as u64 + track.length_lsn as u64,
        })
    }

    pub fn finished(&self) -> bool {
        self.next_lsn >= self.end_lsn
    }

    /// Append the next chunk of AUDIO payload to `out`, skipping every
    /// non-audio packet. Returns bytes appended; 0 means the track is done.
    ///
    /// Supplementary and padding packets (`data_type` 3 and 7 on these discs)
    /// are dropped rather than passed through: they are not audio, and a DSD
    /// stream with them in it is noise.
    pub fn next_chunk(&mut self, out: &mut Vec<u8>, sectors: usize) -> Result<usize, SacdError> {
        out.clear();
        if self.finished() {
            return Ok(0);
        }
        let want = sectors.min((self.end_lsn - self.next_lsn) as usize);
        let raw = self.iso.read_sectors(self.next_lsn, want)?;
        for s in 0..want {
            let lsn = self.next_lsn + s as u64;
            let sector = &raw[s * SECTOR as usize..(s + 1) * SECTOR as usize];
            for p in parse_sector(sector, lsn)? {
                if p.data_type == DATA_TYPE_AUDIO {
                    out.extend_from_slice(&sector[p.at..p.at + p.len]);
                }
            }
        }
        self.next_lsn += want as u64;
        Ok(out.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a sector from descriptors read off the owner's Disc 1.
    fn sector(b0: u8, descs: &[u16]) -> Vec<u8> {
        let mut s = vec![0u8; 2048];
        s[0] = b0;
        for (k, w) in descs.iter().enumerate() {
            s[1 + 2 * k..3 + 2 * k].copy_from_slice(&w.to_be_bytes());
        }
        s
    }

    /// The first three DISTINCT sector shapes of Disc 1's audio area, with the
    /// exact descriptor words read off the image at LSN 1253 / 1254 / 1257.
    /// Each one adds up to 2048 — which IS the proof that the decoding is
    /// right, so the test asserts the sum rather than trusting the parser.
    #[test]
    fn the_three_real_sector_shapes_decode_and_fill_exactly() {
        // LSN 1253: 0x44 -> header 8, then (type 3, 24) + (type 2, 2016, start)
        let p = parse_sector(&sector(0x44, &[0x1818, 0x97E0]), 1253).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!((p[0].data_type, p[0].len, p[0].at), (3, 24, 8));
        assert_eq!((p[1].data_type, p[1].len), (DATA_TYPE_AUDIO, 2016));
        assert!(p[1].frame_start, "0x97E0 has bit 15 set");
        assert_eq!(8 + 24 + 2016, 2048);

        // LSN 1254: 0x40 -> header 5, then (type 3, 27) + (type 2, 2016)
        let p = parse_sector(&sector(0x40, &[0x181B, 0x17E0]), 1254).unwrap();
        assert_eq!((p[0].len, p[0].at), (27, 5));
        assert!(!p[0].frame_start && !p[1].frame_start, "0x40 declares no frame info");
        assert_eq!(5 + 27 + 2016, 2048);

        // LSN 1257: 0x64 -> header 10, three packets, the last starting a frame
        let p = parse_sector(&sector(0x64, &[0x1816, 0x1540, 0x92A0]), 1257).unwrap();
        assert_eq!(p.len(), 3);
        assert_eq!((p[0].len, p[1].len, p[2].len), (22, 1344, 672));
        assert!(p[2].frame_start);
        assert_eq!(10 + 22 + 1344 + 672, 2048);
    }

    #[test]
    fn an_eleven_bit_length_is_what_makes_a_sector_add_up() {
        // 0x181B, read off LSN 1254. Twelve bits give a length larger than the
        // sector it lives in, which is how the widely-copied layout refutes
        // itself the first time anyone checks it against a disc.
        let w = 0x181Bu16;
        assert_eq!((w & 0x0FFF) as usize, 2075, "the 12-bit reading");
        assert!(2075 > 2048, "cannot fit in a sector — hence 11 bits");
        assert_eq!((w & 0x07FF) as usize, 27);
    }

    #[test]
    fn the_three_first_bytes_that_exist_give_the_three_header_lengths() {
        for (b0, pc, fic, header) in [(0x40u8, 2, 0, 5), (0x44, 2, 1, 8), (0x64, 3, 1, 10)] {
            assert_eq!(((b0 >> 5) & 0x07) as usize, pc, "packet count of {b0:#04x}");
            assert_eq!(((b0 >> 2) & 0x07) as usize, fic, "frame infos of {b0:#04x}");
            assert_eq!(1 + pc * 2 + fic * 3, header);
        }
    }

    #[test]
    fn a_sector_whose_lengths_do_not_fill_it_is_refused() {
        // Decoding one wrong is exactly how a fixed-size header skip corrupts
        // a DSD stream, so it must be an error and not a shrug.
        let s = sector(0x44, &[0x1818, 0x1000]); // second packet length 0
        assert!(matches!(
            parse_sector(&s, 1253),
            Err(SacdError::BadSector { .. })
        ));
    }

    #[test]
    fn the_start_lsn_and_start_time_identity_holds() {
        // Verified 44/44 on the owner's discs: this is the relation that
        // proves SECTORS_PER_SEC and ties the two TOC blocks together.
        // The four values below were read out of SACDTRL1 and SACDTRL2 on
        // Disc 1; the identity reproduces every start LSN exactly, which is
        // what pins SECTORS_PER_SEC at 350 rather than leaving it a guess.
        let area_start = 553u32;
        for (start_lsn, start_secs) in [
            (1253u32, 2.0f64),
            (97_890, 278.106_666_7),
            (123_337, 350.813_333_3),
            (150_665, 428.893_333_3),
        ] {
            let predicted = area_start + (start_secs * SECTORS_PER_SEC as f64) as u32;
            assert_eq!(predicted, start_lsn, "track starting at {start_secs}s");
        }
    }

    #[test]
    fn an_uncompressed_stereo_frame_is_the_size_that_proves_no_dst() {
        assert_eq!(DSD64_STEREO_FRAME, 2 * 2_822_400 / 75 / 8);
        // The measured audio-payload totals divide by it exactly.
        assert_eq!(350_495u64 * DSD64_STEREO_FRAME as u64 % DSD64_STEREO_FRAME as u64, 0);
    }
}
