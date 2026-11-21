//! Frostbite `Wave` sound resources (BFBC2 Xbox 360 / PC, Battlefield 1943) and EA's audio stream framing (SPS v1,
//! SNR / SNS v0): a port of the container code of `bfbc2-console-convert/audio.py` (`read_wave`, `write_wave_pc`,
//! `read_sps`, `read_snr`, `write_sps`), round F0. The layouts are in that module's docstring; the codecs (EA-XAS,
//! EA-XMA) are not here.
//!
//! The same words, segments and blocks as the reference, the same bytes written. Where the reference raises, the
//! readers stop at the same point with its error (round RT3, so the readers' references could be retired):
//! [`Error::Audio`] its `AudioError`'s message, [`Error::Struct`] `struct.error`'s in CPython's words (a read past the
//! end that its checks let through). A NaN sample rate (BF1943's float word) is read as the file holds it.

/// The padding after every blob.
pub const PAD: &[u8; 3] = b"\xad\xde\xad";

/// Where the reference raises.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The reference's `AudioError(message)`.
    Audio(String),
    /// The reference's `struct.error(message)`.
    Struct(String),
    /// A value past the field's width when writing.
    TooLarge,
}

impl Error {
    /// The Python exception's name (`audio.py` raises it): `AudioError`, `struct.error`; `TooLarge` is the
    /// writers' (`struct.error` too).
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Audio(_) => "AudioError",
            Self::Struct(_) | Self::TooLarge => "struct.error",
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Audio(m) | Self::Struct(m) => f.write_str(m),
            Self::TooLarge => f.write_str("a blob too large for its u32 size"),
        }
    }
}

impl std::error::Error for Error {}

type Result<T> = std::result::Result<T, Error>;

fn audio(m: impl Into<String>) -> Error {
    Error::Audio(m.into())
}

/// `struct.unpack_from` of `size` bytes at `at` past the buffer's end: CPython 3.13's message.
fn short(size: usize, at: usize, len: usize) -> Error {
    Error::Struct(format!(
        "unpack_from requires a buffer of at least {} bytes for unpacking {size} bytes at offset {at} (actual buffer \
         size is {len})",
        at + size
    ))
}

/// Which game and platform a Wave is from (`audio.SOURCES`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// BFBC2, Xbox 360 (big-endian).
    Bfbc2Xbox,
    /// BFBC2, PC (little-endian).
    Bfbc2Pc,
    /// Battlefield 1943, Xbox 360 (big-endian).
    Bf1943,
}

/// One segment: (audio data, stream information).
pub type Segment = (Vec<u8>, Vec<u8>);

/// A Wave resource (`audio.Wave`).
#[derive(Debug, Clone, PartialEq)]
pub struct Wave {
    /// Word 0: flags << 16 | id.
    pub flags_id: u32,
    /// Total samples.
    pub total: u32,
    /// Intro samples (BF1943: the loop start).
    pub intro: u32,
    /// Channels.
    pub channels: u32,
    /// Outro samples (BF1943: total - loop end, when there is an outro).
    pub outro: i64,
    /// Word 0x14 (0 for BF1943).
    pub word14: u32,
    /// Intro, loop, outro.
    pub segments: [Option<Segment>; 3],
    /// BF1943's sample rate, word 0x04 and loop end.
    pub bf1943: Option<(f32, u32, u32)>,
}

/// `struct.unpack_from(e + 'I', b, at)[0]`, or its `struct.error`.
fn u32_at(b: &[u8], at: usize, big: bool) -> Result<u32> {
    let w: [u8; 4] = b
        .get(at..at + 4)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| short(4, at, b.len()))?;
    Ok(if big {
        u32::from_be_bytes(w)
    } else {
        u32::from_le_bytes(w)
    })
}

fn pad4(n: usize) -> usize {
    (4 - n % 4) % 4
}

/// `audio.read_wave(b, source)`.
///
/// # Errors
/// The reference's error where it raises (its `AudioError`, or `struct.error` for a stream-information size word
/// past the end).
#[allow(clippy::many_single_char_names, reason = "the reference's names (b, n, m, o)")]
pub fn read_wave(b: &[u8], source: Source) -> Result<Wave> {
    let big = source != Source::Bfbc2Pc;
    let mut words = [0u32; 7];
    let mut o;
    let mut rate = 0f32;
    if source == Source::Bf1943 {
        if b.len() < 40 {
            return Err(audio(format!(
                "Wave: {} bytes, shorter than a Battlefield 1943 header",
                b.len()
            )));
        }
        rate = f32::from_bits(u32_at(b, 0, true)?);
        for (k, w) in words.iter_mut().take(6).enumerate() {
            *w = u32_at(b, 4 + 4 * k, true)?;
        }
        o = 28;
    } else {
        if b.len() < 36 {
            return Err(audio(format!("Wave: {} bytes, shorter than a BFBC2 header", b.len())));
        }
        for (k, w) in words.iter_mut().take(6).enumerate() {
            *w = u32_at(b, 4 * k, big)?;
        }
        o = 24;
    }
    let mut segs: [Option<Segment>; 3] = [None, None, None];
    for (i, seg) in segs.iter_mut().enumerate() {
        if o + 4 > b.len() {
            return Err(audio(format!(
                "Wave: segment {i} at {o:#x} runs past the end ({} bytes)",
                b.len()
            )));
        }
        let n = u32_at(b, o, big)? as usize;
        o += 4;
        if n == 0 {
            continue;
        }
        if o + n + 4 > b.len() {
            return Err(audio(format!(
                "Wave: segment {i}: {n} bytes at {o:#x} run past the end ({} bytes)",
                b.len()
            )));
        }
        let data = slice(b, o, o + n);
        o += n + pad4(n);
        let m = u32_at(b, o, big)? as usize;
        o += 4;
        let info = slice(b, o, o + m);
        if info.len() != m {
            return Err(audio(format!(
                "Wave: segment {i}: {m} bytes of stream information run past the end"
            )));
        }
        o = (o + m + pad4(m)).min(b.len());
        *seg = Some((data, info));
    }
    if o != b.len() {
        return Err(audio(format!(
            "Wave: the segments end at {o:#x}, the file at {:#x}",
            b.len()
        )));
    }
    if segs[1].is_none() {
        return Err(audio("Wave: no loop segment"));
    }
    if source == Source::Bf1943 {
        let [w04, ch, total, loop_start, loop_end, fid, _] = words;
        let outro = if segs[2].is_some() {
            i64::from(total) - i64::from(loop_end)
        } else {
            0
        };
        return Ok(Wave {
            flags_id: fid,
            total,
            intro: loop_start,
            channels: ch,
            outro,
            word14: 0,
            segments: segs,
            bf1943: Some((rate, w04, loop_end)),
        });
    }
    let [fid, total, intro, ch, outro, w14, _] = words;
    Ok(Wave {
        flags_id: fid,
        total,
        intro,
        channels: ch,
        outro: i64::from(outro),
        word14: w14,
        segments: segs,
        bf1943: None,
    })
}

/// `audio.write_wave_pc(w)` from its six words and three segments.
///
/// # Errors
/// [`Error::Audio`] without a loop segment; [`Error::TooLarge`] for a blob past `u32`.
pub fn write_wave_pc(words: &[u32; 6], segments: &[Option<(&[u8], &[u8])>; 3]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for w in words {
        out.extend_from_slice(&w.to_le_bytes());
    }
    for (i, s) in segments.iter().enumerate() {
        let Some((data, info)) = s else {
            if i == 1 {
                return Err(audio("Wave: no loop segment"));
            }
            out.extend_from_slice(&0u32.to_le_bytes());
            continue;
        };
        for blob in [data, info] {
            out.extend_from_slice(&u32::try_from(blob.len()).map_err(|_| Error::TooLarge)?.to_le_bytes());
            out.extend_from_slice(blob);
            out.extend_from_slice(PAD.get(..pad4(blob.len())).unwrap_or(&[]));
        }
    }
    Ok(out)
}

/// An EA stream (`audio.Stream`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stream {
    /// 0 (BF1943) or 1 (BFBC2).
    pub version: u32,
    /// The codec.
    pub codec: u32,
    /// Channels.
    pub channels: u32,
    /// Sample rate.
    pub rate: u32,
    /// 0 in memory, 1 stream / SPS.
    pub type_: u32,
    /// Samples.
    pub samples: u32,
    /// The audio blocks: (samples, data).
    pub blocks: Vec<(u32, Vec<u8>)>,
}

/// `_ea_header(h1, h2)`.
fn ea_header(h1: u32, h2: u32, blocks: Vec<(u32, Vec<u8>)>) -> Result<Stream> {
    if (h2 >> 29) & 1 != 0 {
        return Err(audio(
            "EA stream: the loop flag is set (no Wave seen has it; its loop start word is not handled)",
        ));
    }
    Ok(Stream {
        version: h1 >> 28,
        codec: (h1 >> 24) & 15,
        channels: ((h1 >> 18) & 0x3F) + 1,
        rate: h1 & 0x3FFFF,
        type_: h2 >> 30,
        samples: h2 & 0x1FFF_FFFF,
        blocks,
    })
}

/// `s[a:b]` as Python slices it (empty when `a >= b`).
fn slice(s: &[u8], a: usize, b: usize) -> Vec<u8> {
    let (a, b) = (a.min(s.len()), b.min(s.len()));
    if a >= b {
        Vec::new()
    } else {
        s.get(a..b).unwrap_or(&[]).to_vec()
    }
}

/// `audio.read_sps(s)`.
///
/// # Errors
/// The reference's error where it raises (`AudioError`, or `struct.error` for a header or audio block too short for
/// its words).
pub fn read_sps(s: &[u8]) -> Result<Stream> {
    let mut o = 0usize;
    let mut hdr: Option<(u32, u32)> = None;
    let mut blocks = Vec::new();
    while o + 4 <= s.len() {
        let kind = s.get(o).copied().unwrap_or(0);
        let n = u32_at(s, o, true)? as usize & 0xFF_FFFF;
        if n < 4 || o + n > s.len() {
            return Err(audio(format!(
                "SPS: block {kind:#x} of {n} bytes at {o:#x} does not fit ({} bytes)",
                s.len()
            )));
        }
        match kind {
            0x48 => {
                // struct.unpack_from('>II', s, o + 4): one read of 8 bytes
                if o + 12 > s.len() {
                    return Err(short(8, o + 4, s.len()));
                }
                let (h1, h2) = (u32_at(s, o + 4, true)?, u32_at(s, o + 8, true)?);
                // the loop flag is checked as the header is read (the reference's _ea_header)
                ea_header(h1, h2, Vec::new())?;
                hdr = Some((h1, h2));
            }
            0x44 => blocks.push((u32_at(s, o + 4, true)?, slice(s, o + 8, o + n))),
            0x45 => {
                o += n;
                break;
            }
            _ => return Err(audio(format!("SPS: unknown block kind {kind:#x} at {o:#x}"))),
        }
        o += n;
    }
    let Some((h1, h2)) = hdr else {
        return Err(audio("SPS: no header block"));
    };
    if o != s.len() {
        return Err(audio(format!("SPS: ends at {o:#x} of {:#x}", s.len())));
    }
    ea_header(h1, h2, blocks)
}

/// Audio blocks: (samples, data).
type Blocks = Vec<(u32, Vec<u8>)>;

fn sns_blocks(s: &[u8], mut o: usize) -> Result<(Blocks, usize)> {
    let mut blocks = Vec::new();
    while o + 8 <= s.len() {
        let h = u32_at(s, o, true)?;
        let (flags, n) = (h >> 24, (h & 0xFF_FFFF) as usize);
        if n < 8 || o + n > s.len() {
            return Err(audio(format!(
                "SNS: block of {n} bytes at {o:#x} does not fit ({} bytes)",
                s.len()
            )));
        }
        blocks.push((u32_at(s, o + 4, true)?, slice(s, o + 8, o + n)));
        o += n;
        if flags & 0x80 != 0 {
            break;
        }
    }
    Ok((blocks, o))
}

/// `audio.read_snr(s, sns)`.
///
/// # Errors
/// The reference's `AudioError` where it raises.
pub fn read_snr(s: &[u8], sns: Option<&[u8]>) -> Result<Stream> {
    if s.len() < 8 {
        return Err(audio(format!("SNR: {} bytes, shorter than the header", s.len())));
    }
    let (h1, h2) = (u32_at(s, 0, true)?, u32_at(s, 4, true)?);
    ea_header(h1, h2, Vec::new())?;
    let blocks = if s.len() == 8 {
        let Some(sns) = sns else {
            return Err(audio("SNR: a streamed sound (header only); pass its waveDataSns entry"));
        };
        let (blocks, o) = sns_blocks(sns, 0)?;
        if o != sns.len() {
            return Err(audio(format!(
                "SNS: the blocks end at {o:#x}, the stream at {:#x}",
                sns.len()
            )));
        }
        blocks
    } else {
        let (blocks, o) = sns_blocks(s, 8)?;
        if o != s.len() {
            return Err(audio(format!(
                "SNR: the blocks end at {o:#x}, the data at {:#x}",
                s.len()
            )));
        }
        blocks
    };
    ea_header(h1, h2, blocks)
}

/// `audio.write_sps(st)` from the stream's two header words (`Stream.header()`) and blocks.
///
/// # Errors
/// [`Error::TooLarge`] where a block's word does not fit a `u32`.
pub fn write_sps(header: (u32, u32), blocks: &[(u32, &[u8])]) -> Result<Vec<u8>> {
    let mut out = b"\x48\x00\x00\x0c".to_vec();
    out.extend_from_slice(&header.0.to_be_bytes());
    out.extend_from_slice(&header.1.to_be_bytes());
    for (n, data) in blocks {
        let size = u32::try_from(8 + data.len()).map_err(|_| Error::TooLarge)?;
        out.extend_from_slice(&(0x4400_0000 | size).to_be_bytes());
        out.extend_from_slice(&n.to_be_bytes());
        out.extend_from_slice(data);
    }
    out.extend_from_slice(b"\x45\x00\x00\x04");
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn wave_round_trip() {
        let words = [0x0001_da91, 1000, 0, 2, 0, 0];
        let data = b"abcde".as_slice();
        let segs = [
            None,
            Some((data, b"".as_slice())),
            Some((b"xy".as_slice(), b"info".as_slice())),
        ];
        let b = write_wave_pc(&words, &segs).unwrap();
        let w = read_wave(&b, Source::Bfbc2Pc).unwrap();
        assert_eq!(w.total, 1000);
        assert_eq!(w.segments[1], Some((b"abcde".to_vec(), Vec::new())));
        assert_eq!(w.segments[2], Some((b"xy".to_vec(), b"info".to_vec())));
        assert!(read_wave(&b[..b.len() - 1], Source::Bfbc2Pc).is_err());
        assert!(read_wave(&b, Source::Bfbc2Xbox).is_err());
        assert_eq!(
            write_wave_pc(&words, &[None, None, None]),
            Err(Error::Audio("Wave: no loop segment".into()))
        );
        // the reference's words where it raises (round RT3)
        assert_eq!(
            read_wave(&b[..30], Source::Bfbc2Pc).unwrap_err().to_string(),
            "Wave: 30 bytes, shorter than a BFBC2 header"
        );
        let e = read_wave(&b[..b.len() - 1], Source::Bfbc2Pc).unwrap_err();
        assert_eq!((e.kind(), e.to_string().starts_with("Wave: ")), ("AudioError", true));
        // a one-byte loop blob with its padding cut: the info size word is read past the end (struct.error)
        let segs1 = [None, Some((b"a".as_slice(), b"".as_slice())), None];
        let one = write_wave_pc(&words, &segs1).unwrap();
        let mut short_info = one[..32].to_vec();
        short_info.extend_from_slice(&[b'a', 0xad, 0xde]);
        short_info.extend_from_slice(&[0; 4]);
        let e = read_wave(&short_info, Source::Bfbc2Pc).unwrap_err();
        assert_eq!(
            (e.kind(), e.to_string()),
            (
                "struct.error",
                "unpack_from requires a buffer of at least 40 bytes for unpacking 4 bytes at offset 36 (actual \
                 buffer size is 39)"
                    .into()
            )
        );
    }

    #[test]
    fn sps_round_trip() {
        let st = (1 << 28 | 4 << 24 | 1 << 18 | 0xBB80, 1 << 30 | 0x12C);
        let b = write_sps(st, &[(100, b"one".as_slice()), (200, b"two!".as_slice())]).unwrap();
        let s = read_sps(&b).unwrap();
        assert_eq!(
            (s.version, s.codec, s.channels, s.rate, s.type_, s.samples),
            (1, 4, 2, 48_000, 1, 300)
        );
        assert_eq!(s.blocks, vec![(100, b"one".to_vec()), (200, b"two!".to_vec())]);
        let mut looped = b.clone();
        looped[8] |= 0x20;
        assert_eq!(
            read_sps(&looped).unwrap_err().to_string(),
            "EA stream: the loop flag is set (no Wave seen has it; its loop start word is not handled)"
        );
        assert_eq!(
            read_sps(&b[..b.len() - 1]).unwrap_err().to_string(),
            format!("SPS: ends at {:#x} of {:#x}", b.len() - 4, b.len() - 1)
        );
        // a header block of 8 bytes: its two words read past the end (struct.error, one read of 8 bytes)
        let e = read_sps(b"\x48\x00\x00\x08\x00\x00\x00\x00").unwrap_err();
        assert_eq!(
            (e.kind(), e.to_string()),
            (
                "struct.error",
                "unpack_from requires a buffer of at least 12 bytes for unpacking 8 bytes at offset 4 (actual \
                 buffer size is 8)"
                    .into()
            )
        );
    }

    #[test]
    fn snr() {
        let mut s = (2u32 << 24 | 0x5622).to_be_bytes().to_vec();
        s.extend_from_slice(&50u32.to_be_bytes());
        let mut block = (0x80u32 << 24 | 0xB).to_be_bytes().to_vec();
        block.extend_from_slice(&50u32.to_be_bytes());
        block.extend_from_slice(b"abc");
        let mut whole = s.clone();
        whole.extend_from_slice(&block);
        assert_eq!(read_snr(&whole, None).unwrap().blocks, vec![(50, b"abc".to_vec())]);
        assert_eq!(read_snr(&s, Some(&block)).unwrap().blocks, vec![(50, b"abc".to_vec())]);
        assert_eq!(
            read_snr(&s, None),
            Err(Error::Audio(
                "SNR: a streamed sound (header only); pass its waveDataSns entry".into()
            ))
        );
    }
}
