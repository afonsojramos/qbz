//! WASAPI exclusive-mode output (Windows) — the bit-perfect path.
//!
//! Shape: `alsa_direct.rs` on the OUTSIDE, so `PlaybackEngine` can clone the
//! arm it already has for ALSA. NOT on the inside: WASAPI's COM objects are
//! `!Send`, so exactly one render thread owns every COM handle and this struct
//! is a channel plus atomics.
//!
//! Everything above the `#[cfg(windows)]` block is pure and compiles on every
//! host, which is the point — the format ladder, the PCM packing and the
//! period arithmetic are where the platform-specific mistakes live, and they
//! are testable on the machine that runs CI as well as on the one that ships.
//!
//! # The one that bites
//!
//! Windows 24-in-32 is **LEFT-aligned**: the 24 valid bits sit in the HIGH
//! bytes of the 32-bit container. ALSA's `S24_LE` is right-aligned. Porting
//! the ALSA packing verbatim shifts every sample down 8 bits and plays 48 dB
//! quiet, which sounds like a volume bug rather than a format bug — so
//! `s24_in_32_is_left_aligned_with_zero_low_byte` exists to fail loudly.

use crate::backend::BitPerfectMode;

/// A format rung, in the order the ladder tries them.
///
/// `S24Packed` first: it is the shape a DAC that wants 24-bit really wants,
/// and some USB Audio Class devices accept only it. `F32` last and separate —
/// see [`Rung::bit_perfect_mode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rung {
    /// 3 bytes per sample, no padding. ALSA's `S24_3LE`.
    S24Packed,
    /// 32-bit container, 32 valid bits.
    S32,
    /// 32-bit container, 24 valid bits, LEFT-aligned (Windows' own shape).
    S24In32,
    S16,
    /// 32-bit IEEE float. A COMPATIBILITY rung: the device mixes rather than
    /// takes the samples as given, so it is never bit-perfect.
    F32,
}

/// The order `WasapiDirectStream::new` probes. Measured on the owner's
/// Cambridge Audio USB Audio 2.0 (research/05-spike-results.md): only
/// `S24In32` is accepted, at 44.1/48/88.2/96/192 kHz — so on that DAC the
/// ladder is one rung deep. Other devices differ, which is why it stays a
/// ladder.
pub const LADDER: [Rung; 5] = [
    Rung::S24Packed,
    Rung::S32,
    Rung::S24In32,
    Rung::S16,
    Rung::F32,
];

impl Rung {
    pub fn container_bits(self) -> u16 {
        match self {
            Rung::S24Packed => 24,
            Rung::S16 => 16,
            _ => 32,
        }
    }

    pub fn valid_bits(self) -> u16 {
        match self {
            Rung::S24Packed | Rung::S24In32 => 24,
            Rung::S16 => 16,
            _ => 32,
        }
    }

    pub fn is_float(self) -> bool {
        matches!(self, Rung::F32)
    }

    /// Bytes per FRAME, i.e. per sample across every channel.
    pub fn block_align(self, channels: u16) -> u16 {
        self.container_bits() / 8 * channels
    }

    /// The float rung means the device took floats and will convert them, so
    /// the samples that reach the DAC are not the ones we produced.
    pub fn bit_perfect_mode(self) -> BitPerfectMode {
        if self.is_float() {
            BitPerfectMode::Disabled
        } else {
            BitPerfectMode::DirectHardware
        }
    }
}

/// f32 samples to the wire bytes of one rung, appended to `out`.
///
/// Scaling matches `alsa_direct.rs` so the two backends cannot disagree about
/// what full scale is. The `S24In32` arm is the one that differs, and it
/// differs on purpose: `v << 8`, low byte zero.
pub fn pack_f32(samples: &[f32], rung: &Rung, out: &mut Vec<u8>) {
    match rung {
        Rung::S16 => {
            for &s in samples {
                let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        Rung::S24Packed => {
            for &s in samples {
                let v = (s.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32;
                out.extend_from_slice(&v.to_le_bytes()[..3]);
            }
        }
        Rung::S24In32 => {
            for &s in samples {
                let v = (s.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32;
                // LEFT-aligned: the valid 24 bits occupy the HIGH bytes.
                out.extend_from_slice(&(v << 8).to_le_bytes());
            }
        }
        Rung::S32 => {
            for &s in samples {
                let v = (s.clamp(-1.0, 1.0) as f64 * i32::MAX as f64).round() as i32;
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        Rung::F32 => {
            for &s in samples {
                out.extend_from_slice(&s.to_le_bytes());
            }
        }
    }
}

/// 100-ns units to frames, rounded to nearest.
pub fn hns_to_frames(hns: i64, rate: u32) -> u32 {
    ((hns as f64 * rate as f64 / 10_000_000.0) + 0.5) as u32
}

/// Frames to 100-ns units, rounded to nearest.
pub fn frames_to_hns(frames: u32, rate: u32) -> i64 {
    ((frames as f64 * 10_000_000.0 / rate as f64) + 0.5) as i64
}

/// Round a period up so `frames * block_align` is a whole number of 128-byte
/// blocks.
///
/// This is pre-alignment: `AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED` is what an
/// unaligned period earns, and recovering from it costs a `GetBufferSize`, a
/// fresh `IAudioClient` and a second `Initialize`. Asking for an aligned
/// period in the first place means the error usually never fires — measured on
/// the owner's DAC, it did not fire at all. Keep the retry anyway: Intel HDA
/// devices are the documented case that still needs it.
pub fn aligned_period_hns(requested_hns: i64, rate: u32, block_align: u16) -> i64 {
    let frames = hns_to_frames(requested_hns, rate).max(1);
    let bytes = frames as u64 * block_align as u64;
    let aligned_bytes = bytes.div_ceil(128) * 128;
    let aligned_frames = (aligned_bytes / block_align as u64) as u32;
    frames_to_hns(aligned_frames, rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The -48 dB trap. Windows puts the 24 valid bits in the HIGH bytes;
    /// porting ALSA's right-aligned S24_LE here plays quiet, not broken, which
    /// is the kind of defect that survives a listening test.
    #[test]
    fn s24_in_32_is_left_aligned_with_zero_low_byte() {
        let mut out = Vec::new();
        pack_f32(&[1.0, -1.0, 0.0], &Rung::S24In32, &mut out);
        let w: Vec<i32> = out
            .chunks(4)
            .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        assert_eq!(w[0], 8_388_607 << 8);
        assert_eq!(w[1], -8_388_607 << 8);
        assert_eq!(w[2], 0);
        assert!(
            out.chunks(4).all(|b| b[0] == 0),
            "the low byte of every container must be zero"
        );
    }

    #[test]
    fn s24_packed_matches_alsa_s24_3le_byte_order() {
        let mut out = Vec::new();
        pack_f32(&[1.0], &Rung::S24Packed, &mut out);
        assert_eq!(out, vec![0xFF, 0xFF, 0x7F]); // little-endian 0x7FFFFF
    }

    #[test]
    fn s32_and_s16_scale_like_alsa_direct() {
        let mut out = Vec::new();
        pack_f32(&[1.0], &Rung::S32, &mut out);
        assert_eq!(i32::from_le_bytes([out[0], out[1], out[2], out[3]]), i32::MAX);
        out.clear();
        pack_f32(&[-1.0], &Rung::S16, &mut out);
        assert_eq!(i16::from_le_bytes([out[0], out[1]]), -32767);
    }

    #[test]
    fn packing_is_clamped_not_wrapped() {
        // A sample above full scale must saturate. Wrapping would invert the
        // waveform's peaks, which is audible as a click, not as clipping.
        let mut out = Vec::new();
        pack_f32(&[2.0, -2.0], &Rung::S16, &mut out);
        assert_eq!(i16::from_le_bytes([out[0], out[1]]), 32767);
        assert_eq!(i16::from_le_bytes([out[2], out[3]]), -32767);
    }

    #[test]
    fn ladder_order_and_geometry() {
        assert_eq!(
            LADDER,
            [
                Rung::S24Packed,
                Rung::S32,
                Rung::S24In32,
                Rung::S16,
                Rung::F32
            ]
        );
        assert_eq!(Rung::S24Packed.block_align(2), 6);
        assert_eq!(Rung::S24In32.block_align(2), 8);
        assert_eq!(Rung::S16.block_align(2), 4);
        assert_eq!(Rung::S24In32.valid_bits(), 24);
        assert_eq!(Rung::S24In32.container_bits(), 32);
        assert_eq!(Rung::S24Packed.valid_bits(), 24);
        assert_eq!(Rung::S24Packed.container_bits(), 24);
    }

    #[test]
    fn float_rung_is_not_bit_perfect() {
        assert_eq!(Rung::F32.bit_perfect_mode(), BitPerfectMode::Disabled);
        assert_eq!(
            Rung::S24Packed.bit_perfect_mode(),
            BitPerfectMode::DirectHardware
        );
        assert_eq!(
            Rung::S24In32.bit_perfect_mode(),
            BitPerfectMode::DirectHardware
        );
    }

    #[test]
    fn aligned_period_is_a_multiple_of_128_bytes() {
        // 10 ms at 44.1k stereo 24-packed: 441 frames * 6 B = 2646 B, which is
        // not a whole number of 128-byte blocks.
        let hns = aligned_period_hns(100_000, 44100, 6);
        let frames = hns_to_frames(hns, 44100);
        assert_eq!(frames as u64 * 6 % 128, 0);
        assert!(
            (frames as i64 - 441).abs() <= 22,
            "stay near the requested period, got {frames} frames"
        );
    }

    #[test]
    fn aligned_period_leaves_an_already_aligned_one_alone() {
        // 3 ms at 192k stereo 24-in-32: 576 frames * 8 B = 4608 B = 36 blocks.
        // This is the owner's DAC's minimum period, measured; it must not move.
        let hns = aligned_period_hns(30_000, 192_000, 8);
        assert_eq!(hns_to_frames(hns, 192_000), 576);
    }

    #[test]
    fn period_conversions_round_trip() {
        for (rate, frames) in [(44100u32, 441u32), (192_000, 576), (96_000, 960)] {
            let hns = frames_to_hns(frames, rate);
            assert_eq!(hns_to_frames(hns, rate), frames, "rate {rate}");
        }
    }
}
