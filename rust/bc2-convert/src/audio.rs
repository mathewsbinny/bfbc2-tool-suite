//! EA-XAS v1 audio (BFBC2's PC in-memory sounds), ported from `bfbc2-console-convert/audio.py`: the encoder
//! (`xas_encode_frames`) and the decoder (`xas_decode_frames`); the format: that module's text.
//!
//! | Python (`audio.py`) | here |
//! |---|---|
//! | `xas_encode_frames(pcm)` | [`encode`] (`pcm` interleaved, the frames as bytes) |
//! | `xas_decode_frames(f)` | [`decode_frames`] |
//! | `encode_xas` of many sounds | [`encode_all`] |
//!
//! A frame holds 128 samples of one channel in four groups of 32; the frames of a sound are frame-major (frame 0
//! of every channel, then frame 1 ...), 76 bytes each. Every group is encoded on its own (its first two samples
//! stored at 12 bits, the other 30 as 4-bit ADPCM from one of four predictors), so the groups are encoded on all
//! cores, and many sounds at once by [`encode_all`].
//!
//! Every byte is the reference's. The reference is NumPy; its float steps are exact here in integers:
//! - the shift a coefficient set needs, `ceil(log2(max(r, 1) / 7.0))` of an integer residual `r`, is the least `m`
//!   with `r <= 7 * 2^m` (`r < 2^26`: a quotient `r / 7` is never within float64 rounding of a power of two
//!   unless it is one, and then `log2` is exact; `tests\test_audio_native.py` checks every `r` to `2^26`);
//! - `rint((target - pred) / step)`: the step is a power of two, so the quotient is exact and `rint` rounds half
//!   to even ([`rint_shift`]);
//! - the squared error is summed in float64, of integers below `2^53` (30 values below `2^32`): exact, a `u64`.
//!
//! The search makes the reference's choice among the reference's candidates (each coefficient set with the shift
//! its residual needs and the next one finer / coarser: the least squared error, the first in its order on a tie),
//! with less work (BENCHMARKS.md, E3a; the same bytes):
//! - a candidate whose shift equals the one before it (a clipped shift) is the same candidate again, later in the
//!   reference's order: never its choice, skipped;
//! - the likely winner is tried first (the set with the least residual, at its own shift then the finer one, then
//!   the other sets; every coarser shift last: 93 % of BF1943's groups take the least-residual set, almost none a
//!   coarser shift), so the best error is small early;
//! - a candidate stops as soon as its running error passes the best so far, or equals it while coming later in
//!   the reference's order (errors only grow).
//!
//! ```
//! use bc2_convert::audio;
//!
//! // one channel, 128 samples of a ramp: one frame
//! let pcm: Vec<i16> = (0..128).map(|i| i * 100).collect();
//! let frames = audio::encode(&pcm, 1)?;
//! assert_eq!(frames.len(), 76);
//! let back = audio::decode_frames(&frames)?;
//! assert_eq!(back.len(), 128);
//! assert!(back.iter().zip(&pcm).all(|(a, b)| (a - b).abs() < 64));
//! # Ok::<(), audio::AudioError>(())
//! ```

use rayon::prelude::*;
use thiserror::Error;

/// The bytes of one frame (128 samples of one channel).
pub const FRAME: usize = 76;
/// The samples of one frame.
pub const FRAME_SAMPLES: usize = 128;
/// The coefficient table of the reference (`COEFS`): set `i`'s pair is `(COEFS[i], COEFS[i + 4])`; the decoder
/// reads the 4-bit set field as it is, so sets 4 - 15 (no encoder writes them) index into the rest of the table.
pub const COEFS: [i64; 20] = [
    0, 240, 460, 392, 0, 0, -208, -220, 0, 1, 3, 4, 7, 8, 10, 11, 0, -1, -3, -4,
];

/// Groups a worker takes at a time: small sounds stay on one thread.
const MIN_UNITS: usize = 64;

/// Why audio is not encoded or decoded.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AudioError {
    /// Interleaved samples whose count is not a multiple of the channels.
    #[error("{samples} samples are not whole frames of {channels} channels")]
    NotWhole {
        /// the samples given
        samples: usize,
        /// the channels said
        channels: usize,
    },
    /// Frame data whose length is not a multiple of 76.
    #[error("EA-XAS: {0} bytes, not whole 76-byte frames")]
    NotFrames(usize),
}

/// The frames of interleaved int16 `pcm` (`channels` per sample), the last frame padded with zeros: frame-major,
/// `ceil(samples / 128) * channels * 76` bytes, as `audio.xas_encode_frames(pcm).tobytes()`. No channels: no
/// bytes.
///
/// # Errors
/// [`AudioError::NotWhole`] when `pcm.len()` is not a multiple of `channels`.
pub fn encode(pcm: &[i16], channels: usize) -> Result<Vec<u8>, AudioError> {
    let n = samples(pcm, channels)?;
    let units = n.div_ceil(FRAME_SAMPLES) * channels;
    let mut out = vec![0u8; units * FRAME];
    out.par_chunks_mut(FRAME)
        .with_min_len(MIN_UNITS)
        .enumerate()
        .for_each(|(u, frame)| encode_unit(pcm, channels, u, frame));
    Ok(out)
}

/// [`encode`] of many sounds `(pcm, channels)`, on all cores.
#[must_use]
pub fn encode_all(items: &[(&[i16], usize)]) -> Vec<Result<Vec<u8>, AudioError>> {
    items.par_iter().map(|&(pcm, ch)| encode(pcm, ch)).collect()
}

/// The int16 samples of frames (76 bytes each), 128 a frame: `audio.xas_decode_frames` flattened.
///
/// # Errors
/// [`AudioError::NotFrames`] when the length is not a multiple of 76.
pub fn decode_frames(data: &[u8]) -> Result<Vec<i16>, AudioError> {
    if !data.len().is_multiple_of(FRAME) {
        return Err(AudioError::NotFrames(data.len()));
    }
    let mut out = vec![0i16; data.len() / FRAME * FRAME_SAMPLES];
    out.par_chunks_mut(FRAME_SAMPLES)
        .with_min_len(MIN_UNITS)
        .zip(data.par_chunks(FRAME).with_min_len(MIN_UNITS))
        .for_each(|(o, f)| decode_unit(f, o));
    Ok(out)
}

fn samples(pcm: &[i16], channels: usize) -> Result<usize, AudioError> {
    if channels == 0 {
        return Ok(0);
    }
    if !pcm.len().is_multiple_of(channels) {
        return Err(AudioError::NotWhole {
            samples: pcm.len(),
            channels,
        });
    }
    Ok(pcm.len() / channels)
}

/// Frame `u` (frame `u / channels` of channel `u % channels`) into `frame` (76 bytes).
fn encode_unit(pcm: &[i16], channels: usize, u: usize, frame: &mut [u8]) {
    let (f, c) = (u / channels, u % channels);
    let (words, _) = frame.as_chunks_mut::<4>();
    let (head, rows) = words.split_at_mut(4.min(words.len()));
    let mut packed = [[0u8; 15]; 4];
    for (gi, (word, p)) in head.iter_mut().zip(packed.iter_mut()).enumerate() {
        let first = f * FRAME_SAMPLES + gi * 32;
        let group: [i64; 32] =
            std::array::from_fn(|j| pcm.get((first + j) * channels + c).map_or(0, |&v| i64::from(v)));
        let (lo, hi, nib) = encode_group(&group);
        *word = (u32::from(lo) | u32::from(hi) << 16).to_le_bytes();
        for (b, [a, z]) in p.iter_mut().zip(nib.as_chunks::<2>().0) {
            *b = ((a & 15) << 4 | (z & 15)) as u8;
        }
    }
    let mut cols = packed.map(IntoIterator::into_iter);
    for row in rows {
        for (b, col) in row.iter_mut().zip(cols.iter_mut()) {
            *b = col.next().unwrap_or(0);
        }
    }
}

/// `ceil(log2(max(r, 1) / 7.0))`: the least `m` with `r <= 7 * 2^m` (see the module docs).
#[must_use]
pub fn need(r: i64) -> i64 {
    let r4 = r.max(1).saturating_mul(4);
    let mut e = 0i64;
    while e < 62 && (7i64 << e) < r4 {
        e += 1;
    }
    e - 2
}

/// `rint(a / 2^s)`, rounding half to even (NumPy's `rint` of an exact quotient), `1 <= s < 63`.
#[must_use]
pub fn rint_shift(a: i64, s: u32) -> i64 {
    let q = a >> s;
    let rem = a - (q << s);
    let half = 1i64 << (s - 1);
    if rem > half || (rem == half && q & 1 == 1) {
        q + 1
    } else {
        q
    }
}

/// One group of 32 samples: the header's low word (the first sample at 12 bits, the coefficient set), its high
/// word (the second sample, the shift), and the 30 nibbles (-8 .. 7).
#[must_use]
pub fn encode_group(group: &[i64; 32]) -> (u16, u16, [i64; 30]) {
    let [g0, g1, cur @ ..] = *group;
    let q0 = (((g0 + 8) >> 4) << 4).clamp(-32768, 32752);
    let q1 = (((g1 + 8) >> 4) << 4).clamp(-32768, 32752);
    // the reference's candidates: (the order they are tried in here, their place in its order, set, shift)
    let mut cands = [(0i64, 0usize, 0i64, 0i64); 12];
    let mut len = 0;
    for c in 0..4i64 {
        let (c0, c1) = coefs(c);
        // the largest residual of the prediction from the source samples: the shift it needs
        let prev1 = std::iter::once(q1).chain(cur.iter().copied().take(29));
        let prev2 = [q0, q1].into_iter().chain(cur.iter().copied().take(28));
        let resid = cur
            .iter()
            .zip(prev1.zip(prev2))
            .map(|(&x, (p1, p2))| (x * 256 - (p1 * c0 + p2 * c1)).abs())
            .max()
            .unwrap_or(0);
        let nd = need(resid);
        let mut last_k = -1;
        for (di, d) in [-1i64, 0, 1].into_iter().enumerate() {
            let k = (20 - nd - d).clamp(0, 15);
            if k == last_k {
                continue; // the same candidate again, later: never the reference's choice
            }
            last_k = k;
            // least residual first, its own shift then the finer one; every coarser shift last (they rarely win)
            let order = resid * 4
                + match d {
                    0 => 0,
                    -1 => 1,
                    _ => 1 << 40,
                };
            if let Some(slot) = cands.get_mut(len) {
                *slot = (order, c as usize * 3 + di, c, k);
                len += 1;
            }
        }
    }
    let cands = cands.get_mut(..len).unwrap_or_default();
    cands.sort_unstable();
    let mut best_err = u64::MAX;
    let mut best_i = usize::MAX;
    let (mut best_c, mut best_k, mut best_nib) = (0i64, 0i64, [0i64; 30]);
    let mut nib = [0i64; 30];
    for &(_, i, c, k) in &*cands {
        let (c0, c1) = coefs(c);
        let shift = (20 - k) as u32;
        let step = 1i64 << shift;
        let (mut p2, mut p1) = (q0, q1);
        let mut err = 0u64;
        let mut whole = true;
        for (v_out, &x) in nib.iter_mut().zip(cur.iter()) {
            let pred = p1 * c0 + p2 * c1;
            let v = rint_shift(x * 256 - pred, shift).clamp(-8, 7);
            let smp = ((v * step + pred + 128) >> 8).clamp(-32768, 32767);
            let diff = smp - x;
            err += (diff * diff) as u64;
            // the reference keeps the least error, the first in its order on a tie; errors only grow
            if err > best_err || (err == best_err && i > best_i) {
                whole = false;
                break;
            }
            *v_out = v;
            p2 = p1;
            p1 = smp;
        }
        if whole {
            best_err = err;
            best_i = i;
            best_c = c;
            best_k = k;
            best_nib = nib;
        }
    }
    let lo = ((q0 & 0xFFF0) | best_c) & 0xFFFF;
    let hi = ((q1 & 0xFFF0) | best_k) & 0xFFFF;
    (lo as u16, hi as u16, best_nib)
}

/// Coefficient set `i`'s pair `(COEFS[i], COEFS[i + 4])`, `0 <= i < 16`.
fn coefs(i: i64) -> (i64, i64) {
    let i = i as usize;
    (
        COEFS.get(i).copied().unwrap_or(0),
        COEFS.get(i + 4).copied().unwrap_or(0),
    )
}

fn sign16(v: u32) -> i64 {
    i64::from(v as u16 as i16)
}

/// One 76-byte frame into its 128 samples.
fn decode_unit(frame: &[u8], out: &mut [i16]) {
    let (words, _) = frame.as_chunks::<4>();
    let (head, rows) = words.split_at(4.min(words.len()));
    let (groups, _) = out.as_chunks_mut::<32>();
    for (gi, (word, o)) in head.iter().zip(groups.iter_mut()).enumerate() {
        let header = u32::from_le_bytes(*word);
        let (lo, hi) = (header & 0xFFFF, header >> 16);
        let s0 = sign16(lo & 0xFFF0);
        let s1 = sign16(hi & 0xFFF0);
        let (c0, c1) = coefs(i64::from(lo & 15));
        let step = 1i64 << (20 - (hi & 15));
        let nibs = rows.iter().flat_map(|row| {
            let b = i64::from(row.get(gi).copied().unwrap_or(0));
            [b >> 4, b & 15]
        });
        let [first, second, rest @ ..] = o;
        *first = s0 as i16;
        *second = s1 as i16;
        let (mut p2, mut p1) = (s0, s1);
        for (slot, n) in rest.iter_mut().zip(nibs) {
            let n = (n ^ 8) - 8;
            let smp = ((n * step + p1 * c0 + p2 * c1 + 128) >> 8).clamp(-32768, 32767);
            *slot = smp as i16;
            p2 = p1;
            p1 = smp;
        }
    }
}

#[cfg(test)]
#[path = "audio_tests.rs"]
mod tests;
