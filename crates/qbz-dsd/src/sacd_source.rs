//! A SACD track presented as a [`DsdDemuxer`], so every DSD delivery this
//! player already has — PCM conversion, DoP, native — plays a disc image
//! without knowing what one is.
//!
//! The dependency runs qbz-dsd -> qbz-disc and never the other way: qbz-disc
//! owns bytes and geometry, this file owns the audio contract.

use qbz_disc::sacd::{SacdTrack, SacdTrackReader};

use crate::demux::{DsdDemuxer, DsdError, DsdStreamInfo, DsdTags};

/// DSD64: the only rate the Scarlet Book stereo area uses.
const DSD64: u32 = 2_822_400;
/// One uncompressed stereo frame, and 1/75 s of audio.
const FRAME: usize = 9408;
/// Bytes ONE channel contributes to a frame.
const FRAME_PER_CH: usize = FRAME / 2;
/// Sectors pulled from the image per refill. 200 sectors is ~0.57 s of audio
/// and keeps the read amortised without holding much.
const REFILL_SECTORS: usize = 200;

/// How the two channels sit inside one frame.
///
/// Both schemes exist in the wild — DFF interleaves per BYTE, DSF per block —
/// so this was not obvious from first principles and was not left to a guess.
///
/// MEASURED on the owner's Rheingold, track 4, by converting twelve seconds
/// with the real FIR and looking at where the ENERGY lands:
///
///   BlockPerChannel   77.0 % of energy in the audible band, 23 % above it
///   ByteInterleaved   99.8 % audible, 0.2 % above
///
/// A correctly decoded DSD64 stream MUST carry a large ultrasonic component —
/// that rising noise floor is the defining artefact of 1-bit sigma-delta, and
/// no filter puts it back once it is gone. Byte-interleaving effectively
/// decimates the stream and folds that noise away, so its absence is proof
/// the bits were scrambled. `BlockPerChannel` is therefore the layout, and
/// the argument is physical rather than statistical.
///
/// (Three statistical probes tried before this one — per-frame level
/// variance, L/R envelope correlation and a crude spectral centroid — all
/// came back inconclusive or contradictory. They were measuring the noise
/// floor, not the music.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelLayout {
    /// `[ch0 4704][ch1 4704]` — one contiguous block per channel per frame.
    BlockPerChannel,
    /// `ch0 ch1 ch0 ch1 …` one byte at a time.
    ByteInterleaved,
}

pub struct SacdDemuxer {
    reader: SacdTrackReader,
    info: DsdStreamInfo,
    layout: ChannelLayout,
    /// Audio payload read but not yet split into channels. Only whole frames
    /// are consumed, so a partial frame waits here for the next refill.
    pending: Vec<u8>,
    done: bool,
}

impl Default for ChannelLayout {
    /// The measured layout. A caller that does not care gets the right one.
    fn default() -> Self {
        Self::BlockPerChannel
    }
}

impl SacdDemuxer {
    /// Open with the measured layout.
    pub fn open_default(
        image: &std::path::Path,
        track: &SacdTrack,
    ) -> Result<Self, DsdError> {
        Self::open(image, track, ChannelLayout::default())
    }

    pub fn open(
        image: &std::path::Path,
        track: &SacdTrack,
        layout: ChannelLayout,
    ) -> Result<Self, DsdError> {
        let reader = SacdTrackReader::open(image, track)
            .map_err(|e| DsdError::Corrupt(e.to_string()))?;
        Ok(Self {
            reader,
            info: DsdStreamInfo {
                dsd_rate: DSD64,
                channels: 2,
                // Bits per channel. The area TOC gives a duration, and DSD64
                // is exactly 2 822 400 bits/s, so this is the count rather
                // than an estimate.
                sample_count: (track.duration_secs * DSD64 as f64) as u64,
                // SACD is MSB-first, like DFF and unlike DSF's usual LSB-first.
                // Getting this backwards does not fail — it plays, as noise.
                lsb_first: false,
                tags: DsdTags {
                    title: track.title.clone(),
                    track_number: Some(track.number as u32),
                    ..Default::default()
                },
            },
            layout,
            pending: Vec::new(),
            done: false,
        })
    }

    fn refill(&mut self) -> Result<(), DsdError> {
        if self.done {
            return Ok(());
        }
        let mut chunk = Vec::new();
        match self.reader.next_chunk(&mut chunk, REFILL_SECTORS) {
            Ok(0) => self.done = true,
            Ok(_) => self.pending.extend_from_slice(&chunk),
            Err(e) => return Err(DsdError::Corrupt(e.to_string())),
        }
        Ok(())
    }
}

impl DsdDemuxer for SacdDemuxer {
    fn info(&self) -> &DsdStreamInfo {
        &self.info
    }

    fn read_planar(
        &mut self,
        out: &mut [Vec<u8>],
        max_bytes_per_ch: usize,
    ) -> Result<usize, DsdError> {
        if out.len() < 2 {
            return Err(DsdError::UnsupportedChannels(out.len() as u16));
        }
        // Work in whole frames: a frame is the unit the channels are split on,
        // and splitting a partial one puts the two channels out of step for
        // the rest of the track.
        let want_frames = (max_bytes_per_ch / FRAME_PER_CH).max(1);
        while self.pending.len() < want_frames * FRAME && !self.done {
            self.refill()?;
        }
        let have = (self.pending.len() / FRAME).min(want_frames);
        if have == 0 {
            return Ok(0);
        }

        for i in 0..have {
            let frame = &self.pending[i * FRAME..(i + 1) * FRAME];
            match self.layout {
                ChannelLayout::BlockPerChannel => {
                    out[0].extend_from_slice(&frame[..FRAME_PER_CH]);
                    out[1].extend_from_slice(&frame[FRAME_PER_CH..]);
                }
                ChannelLayout::ByteInterleaved => {
                    for pair in frame.chunks_exact(2) {
                        out[0].push(pair[0]);
                        out[1].push(pair[1]);
                    }
                }
            }
        }
        self.pending.drain(..have * FRAME);
        Ok(have * FRAME_PER_CH)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_splits_into_equal_channel_halves() {
        assert_eq!(FRAME, 9408);
        assert_eq!(FRAME_PER_CH, 4704);
        // Each channel contributes exactly 1/75 s of DSD64: 2822400/75 bits
        // = 37632 bits = 4704 bytes. If this ever stops holding, the frame
        // size or the rate has changed and the split is wrong.
        assert_eq!(FRAME_PER_CH * 8, (DSD64 / 75) as usize);
    }

    #[test]
    fn the_two_layouts_disagree_which_is_why_one_had_to_be_measured() {
        // A deliberately asymmetric frame: the first half all 0xAA, the
        // second all 0x55.
        let mut frame = vec![0xAAu8; FRAME_PER_CH];
        frame.extend(std::iter::repeat(0x55).take(FRAME_PER_CH));

        let block0: Vec<u8> = frame[..FRAME_PER_CH].to_vec();
        let inter0: Vec<u8> = frame.chunks_exact(2).map(|p| p[0]).collect();
        assert!(block0.iter().all(|b| *b == 0xAA));
        // Byte-interleaving the same frame gives a channel that is half 0xAA
        // and half 0x55 — a completely different signal, which is why picking
        // the wrong one is audible rather than subtle.
        assert!(inter0.iter().any(|b| *b == 0x55));
    }
}
