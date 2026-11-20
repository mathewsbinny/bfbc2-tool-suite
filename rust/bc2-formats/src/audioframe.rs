//! The rest of `bfbc2-console-convert/audio.py`'s framing, round F3: an EA-XMA stream's parts as XMA2 packets per
//! stream (`xma_parts`), the RIFF WAVE ffmpeg's xma2 decoder reads (`xma2_riff`), and an EA-XAS sound's frames cut
//! into DICE's blocks (`_xas_stream`). [`crate::wave`] has the Wave resource and the stream framing (round F0).
//!
//! The same bytes as the reference. Where the reference raises (an `AudioError`, `struct.error`, a division by
//! zero) this answers the same error with the same message ([`Error`]; round RT2, so the reference could be
//! retired); [`Error::Refused`] is left for counts past any real one and arguments only the reference took.
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

/// Why there is no answer: the reference's error, or an input the port does not take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A count past any real one (the reference would build it), or an argument only the reference took.
    Refused,
    /// The reference's `AudioError(message)`.
    Audio(String),
    /// The reference's `struct.error(message)`: a value past its field's width.
    Struct(String),
    /// The reference's `ZeroDivisionError` (a block of no samples).
    ZeroDivision,
}

impl Error {
    /// The Python exception's name (`AudioError`, `struct.error`, `ZeroDivisionError`), `None` for [`Error::Refused`].
    #[must_use]
    pub fn python_type(&self) -> Option<&'static str> {
        match self {
            Self::Refused => None,
            Self::Audio(_) => Some("AudioError"),
            Self::Struct(_) => Some("struct.error"),
            Self::ZeroDivision => Some("ZeroDivisionError"),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused => f.write_str("left to the Python reference"),
            Self::Audio(m) | Self::Struct(m) => f.write_str(m),
            Self::ZeroDivision => f.write_str("integer division or modulo by zero"),
        }
    }
}

impl std::error::Error for Error {}

type Result<T> = std::result::Result<T, Error>;

/// `audio.xma_parts(st)`: the XMA2 packets of each XMA stream (2 channels each, `(channels + 1) // 2` streams), each
/// block's part padded with zeros to whole packets.
///
/// # Errors
/// [`Error::Audio`] where the reference raises its `AudioError` (a block ending before a stream, a part that does not
/// fit, bytes other than zeros after the parts), with its message; [`Error::Refused`] for a stream count past any
/// real one (1,024).
pub fn xma_parts(channels: i64, blocks: &[&[u8]]) -> Result<Vec<Vec<u8>>> {
    let k = channels.saturating_add(1).div_euclid(2).max(0);
    if k > 1024 {
        return Err(Error::Refused);
    }
    let mut out = vec![Vec::new(); k as usize];
    for (bi, b) in blocks.iter().enumerate() {
        let mut o = 0usize;
        for (i, part) in out.iter_mut().enumerate() {
            let Some(word) = b.get(o..o + 4) else {
                return Err(Error::Audio(format!("EA-XMA: block {bi} ends before stream {i}")));
            };
            let n = (u32::from_be_bytes(word.try_into().map_err(|_| Error::Refused)?) >> 2) as usize;
            if n < 4 || o + n > b.len() {
                return Err(Error::Audio(format!(
                    "EA-XMA: block {bi}, stream {i}: {n} bytes at {o} do not fit {}",
                    b.len()
                )));
            }
            part.extend_from_slice(b.get(o + 4..o + n).ok_or(Error::Refused)?);
            part.resize(part.len() + (XMA_PACKET - (n - 4) % XMA_PACKET) % XMA_PACKET, 0);
            o += n;
        }
        if o != b.len() && b.get(o..).is_some_and(|t| t.iter().any(|&x| x != 0)) {
            return Err(Error::Audio(format!(
                "EA-XMA: block {bi}: the streams end at {o} of {}",
                b.len()
            )));
        }
    }
    Ok(out)
}

/// `audio.xma2_riff(packets, channels, rate, samples)`: a RIFF WAVE of XMA2 packets (an `XMA2WAVEFORMATEX`, one
/// stream).
///
/// # Errors
/// [`Error::Struct`] where `struct` refuses a value (channels past 16 bits; rate, its byte rate or samples past 32
/// bits; a negative value), with its message; [`Error::Refused`] for data of 4 GiB or more.
pub fn xma2_riff(packets: &[u8], channels: i64, rate: i64, samples: i64) -> Result<Vec<u8>> {
    // struct.pack's checks, in its order: the first value past its field's width names the field's format
    let h = |v: i64| u16::try_from(v).map_err(|_| Error::Struct("'H' format requires 0 <= number <= 65535".into()));
    let w =
        |v: i64| u32::try_from(v).map_err(|_| Error::Struct("'I' format requires 0 <= number <= 4294967295".into()));
    let (ch16, rate32) = (h(channels)?, w(rate)?);
    let byte_rate = i64::from(rate32) * i64::from(ch16) * 2;
    let mut fmt = Vec::with_capacity(52);
    fmt.extend_from_slice(&0x166u16.to_le_bytes());
    fmt.extend_from_slice(&ch16.to_le_bytes());
    fmt.extend_from_slice(&rate32.to_le_bytes());
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
    let data_len = u32::try_from(packets.len()).map_err(|_| Error::Refused)?;
    let mut body = b"WAVEfmt ".to_vec();
    body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    body.extend_from_slice(&fmt);
    body.extend_from_slice(b"data");
    body.extend_from_slice(&data_len.to_le_bytes());
    body.extend_from_slice(packets);
    let body_len = u32::try_from(body.len()).map_err(|_| Error::Refused)?;
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
/// [`Error::ZeroDivision`] for a rate of 0 (the reference's `ZeroDivisionError`); [`Error::Refused`] for more than
/// 2^24 blocks.
pub fn xas_blocks(frames: &[u8], n: u64, channels: u64, rate: u32) -> Result<Vec<(u64, &[u8])>> {
    let per = xas_block_samples(rate);
    if per == 0 {
        return Err(Error::ZeroDivision);
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
        assert_eq!(
            xma_parts(3, &[&trailing]),
            Err(Error::Audio("EA-XMA: block 0: the streams end at 16 of 20".into()))
        );
        assert_eq!(
            xma_parts(5, &[&b]),
            Err(Error::Audio("EA-XMA: block 0 ends before stream 2".into()))
        );
        Ok(())
    }

    #[test]
    fn riff_layout() -> Result<()> {
        let r = xma2_riff(b"abc", 1, 44_100, 7)?;
        assert_eq!(r.len(), 8 + 4 + 8 + 52 + 8 + 3);
        assert_eq!(&r[12..16], b"fmt ");
        assert_eq!(
            xma2_riff(b"", 1 << 16, 1, 1),
            Err(Error::Struct("'H' format requires 0 <= number <= 65535".into()))
        );
        assert_eq!(
            xma2_riff(b"", 2, -1, 1).map_err(|e| e.to_string()),
            Err("'I' format requires 0 <= number <= 4294967295".into())
        );
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
        assert_eq!(xas_blocks(&frames, 1, 1, 0), Err(Error::ZeroDivision));
        Ok(())
    }
}
