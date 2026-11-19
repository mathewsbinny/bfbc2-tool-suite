//! The rest of `bfbc2-console-convert/audio.py`'s framing, round F3: an EA-XMA stream's parts as XMA2 packets per
//! stream (`xma_parts`), the RIFF WAVE ffmpeg's xma2 decoder reads (`xma2_riff`), and an EA-XAS sound's frames cut
//! into DICE's blocks (`_xas_stream`). [`crate::wave`] has the Wave resource and the stream framing (round F0).
//!
//! The same bytes as the reference. Where the reference raises (an `AudioError`, `struct.error`, a division by
//! zero) this answers an [`Error`] and the Python runs, so the error stays its own.
//!
//! ```
//! use bc2_formats::audioframe;
//!
//! let riff = audioframe::xma2_riff(&[0; 2048], 2, 48_000, 1_000)?;
//! assert_eq!(&riff[..4], b"RIFF");
//! assert_eq!(audioframe::xas_block_samples(48_000), 256 * 38);
//! # Ok::<(), audioframe::Error>(())
//! ```

/// An XMA2 packet's size.
pub const XMA_PACKET: usize = 2048;
/// An EA-XAS frame's size (one channel).
pub const XAS_FRAME: usize = 76;

/// Why the input is left to the reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Where the reference raises (or a count past any real one).
    Refused,
    /// A value past its field's width (`struct.error`).
    TooLarge,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Refused => "left to the Python reference",
            Self::TooLarge => "a value past its field's width",
        })
    }
}

impl std::error::Error for Error {}

type Result<T> = std::result::Result<T, Error>;

/// `audio.xma_parts(st)`: the XMA2 packets of each XMA stream (2 channels each, `(channels + 1) // 2` streams), each
/// block's part padded with zeros to whole packets.
///
/// # Errors
/// [`Error::Refused`] where the reference raises (a block ending before a stream, a part that does not fit, bytes
/// other than zeros after the parts) or the stream count is past any real one (1,024).
pub fn xma_parts(channels: i64, blocks: &[&[u8]]) -> Result<Vec<Vec<u8>>> {
    let k = channels.saturating_add(1).div_euclid(2).max(0);
    if k > 1024 {
        return Err(Error::Refused);
    }
    let mut out = vec![Vec::new(); k as usize];
    for b in blocks {
        let mut o = 0usize;
        for part in &mut out {
            let word = b.get(o..o + 4).ok_or(Error::Refused)?;
            let n = (u32::from_be_bytes(word.try_into().map_err(|_| Error::Refused)?) >> 2) as usize;
            if n < 4 || o + n > b.len() {
                return Err(Error::Refused);
            }
            part.extend_from_slice(b.get(o + 4..o + n).ok_or(Error::Refused)?);
            part.resize(part.len() + (XMA_PACKET - (n - 4) % XMA_PACKET) % XMA_PACKET, 0);
            o += n;
        }
        if o != b.len() && b.get(o..).is_some_and(|t| t.iter().any(|&x| x != 0)) {
            return Err(Error::Refused);
        }
    }
    Ok(out)
}

/// `audio.xma2_riff(packets, channels, rate, samples)`: a RIFF WAVE of XMA2 packets (an `XMA2WAVEFORMATEX`, one
/// stream).
///
/// # Errors
/// [`Error::TooLarge`] where `struct` refuses a value (channels past 16 bits; rate, its byte rate, samples or the
/// data's length past 32 bits; a negative value).
pub fn xma2_riff(packets: &[u8], channels: i64, rate: i64, samples: i64) -> Result<Vec<u8>> {
    let h = |v: i64| u16::try_from(v).map_err(|_| Error::TooLarge);
    let w = |v: i64| u32::try_from(v).map_err(|_| Error::TooLarge);
    let byte_rate = rate
        .checked_mul(channels)
        .and_then(|v| v.checked_mul(2))
        .ok_or(Error::TooLarge)?;
    let mut fmt = Vec::with_capacity(52);
    fmt.extend_from_slice(&0x166u16.to_le_bytes());
    fmt.extend_from_slice(&h(channels)?.to_le_bytes());
    fmt.extend_from_slice(&w(rate)?.to_le_bytes());
    fmt.extend_from_slice(&w(byte_rate)?.to_le_bytes());
    for v in [XMA_PACKET as u16, 16, 34, 1] {
        fmt.extend_from_slice(&v.to_le_bytes());
    }
    let streams: u32 = if channels == 1 { 4 } else { 3 };
    for v in [streams, w(samples)?, XMA_PACKET as u32, 0, 0, 0, 0] {
        fmt.extend_from_slice(&v.to_le_bytes());
    }
    fmt.extend_from_slice(&[0, 4]);
    fmt.extend_from_slice(&1u16.to_le_bytes());
    let data_len = u32::try_from(packets.len()).map_err(|_| Error::TooLarge)?;
    let mut body = b"WAVEfmt ".to_vec();
    body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    body.extend_from_slice(&fmt);
    body.extend_from_slice(b"data");
    body.extend_from_slice(&data_len.to_le_bytes());
    body.extend_from_slice(packets);
    let body_len = u32::try_from(body.len()).map_err(|_| Error::TooLarge)?;
    let mut out = Vec::with_capacity(body.len() + 8);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&body_len.to_le_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// `audio.xas_block_samples(rate)`: DICE's EA-XAS block length for a sample rate, `256 * ceil(rate / 1280)`.
#[must_use]
pub fn xas_block_samples(rate: u32) -> u64 {
    256 * u64::from(rate).div_ceil(1280)
}

/// `audio._xas_stream`'s blocks: a sound's frames (frame-major) in DICE's blocks, `(samples, bytes)`; every block but
/// the last [`xas_block_samples`] long, the last the rest (empty when `n` is a multiple of the block). Slices past
/// the end of `frames` are cut short as Python's are.
///
/// # Errors
/// [`Error::Refused`] for a rate of 0 (the reference's `ZeroDivisionError`) or more than 2^24 blocks.
pub fn xas_blocks(frames: &[u8], n: u64, channels: u64, rate: u32) -> Result<Vec<(u64, &[u8])>> {
    let per = xas_block_samples(rate);
    if per == 0 {
        return Err(Error::Refused);
    }
    let full = n / per;
    if full > 1 << 24 {
        return Err(Error::Refused);
    }
    let size = (per / 128)
        .checked_mul(channels)
        .and_then(|v| v.checked_mul(XAS_FRAME as u64))
        .ok_or(Error::Refused)?;
    let len = frames.len() as u64;
    let cut = |a: u64, b: u64| -> &[u8] {
        let (a, b) = (a.min(len) as usize, b.min(len) as usize);
        frames.get(a..b.max(a)).unwrap_or_default()
    };
    let mut blocks: Vec<(u64, &[u8])> = (0..full)
        .map(|i| (per, cut(i.saturating_mul(size), (i + 1).saturating_mul(size))))
        .collect();
    blocks.push((n - full * per, cut(full.saturating_mul(size), u64::MAX)));
    Ok(blocks)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use super::*;

    fn part(n: usize, fill: u8) -> Vec<u8> {
        let mut p = ((n as u32) << 2).to_be_bytes().to_vec();
        p.extend(std::iter::repeat_n(fill, n - 4));
        p
    }

    #[test]
    fn parts_padded_per_stream() -> Result<()> {
        let mut b = part(10, 1);
        b.extend(part(6, 2));
        let got = xma_parts(3, &[&b])?;
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].len(), XMA_PACKET);
        assert_eq!(&got[0][..6], &[1; 6]);
        assert_eq!(&got[1][..2], &[2, 2]);
        let mut trailing = b.clone();
        trailing.extend([0, 0, 0]);
        assert!(xma_parts(3, &[&trailing]).is_ok());
        trailing.push(9);
        assert_eq!(xma_parts(3, &[&trailing]), Err(Error::Refused));
        assert_eq!(xma_parts(5, &[&b]), Err(Error::Refused));
        Ok(())
    }

    #[test]
    fn riff_layout() -> Result<()> {
        let r = xma2_riff(b"abc", 1, 44_100, 7)?;
        assert_eq!(r.len(), 8 + 4 + 8 + 52 + 8 + 3);
        assert_eq!(&r[12..16], b"fmt ");
        assert_eq!(xma2_riff(b"", 1 << 16, 1, 1), Err(Error::TooLarge));
        assert_eq!(xma2_riff(b"", 2, -1, 1), Err(Error::TooLarge));
        Ok(())
    }

    #[test]
    fn blocks_of_dice() -> Result<()> {
        let per = xas_block_samples(48_000) as usize;
        let size = per / 128 * 2 * XAS_FRAME;
        let frames = vec![7u8; size * 2 + 10];
        let b = xas_blocks(&frames, 2 * per as u64 + 5, 2, 48_000)?;
        assert_eq!(b.len(), 3);
        assert_eq!((b[2].0, b[2].1.len()), (5, 10));
        let exact = xas_blocks(&frames[..size], per as u64, 2, 48_000)?;
        assert_eq!((exact[1].0, exact[1].1.len()), (0, 0));
        assert_eq!(xas_blocks(&frames, 1, 1, 0), Err(Error::Refused));
        Ok(())
    }
}
