//! `audio.py`'s reference readers (`read_wave_reference`, `read_sps_reference`, `read_snr_reference`,
//! `xma_parts_reference`), for what the core leaves to them: the `AudioError` each raises (its text), and the values
//! of the one resource the core refuses that the reference reads (a Battlefield 1943 Wave whose float rate is NaN).
//! A `struct.error` (a header block cut short) is the Python's own: handed off.

use bc2_formats::wave::{Segment, Source};

use super::{AResult, Outcome, PYTHON, audio_err};

/// A Wave as `convert` uses it.
pub struct WaveR {
    pub source: Source,
    pub flags_id: u32,
    pub total: u32,
    pub intro: u32,
    pub channels: u32,
    pub outro: i64,
    pub word14: u32,
    pub segments: [Option<Segment>; 3],
}

/// An EA stream as `convert` uses it.
pub struct Stream {
    pub codec: u32,
    pub channels: u32,
    pub rate: u32,
    pub samples: u32,
    pub blocks: Vec<(u32, Vec<u8>)>,
}

/// `struct.unpack_from(e + 'I', b, o)[0]`, or the Python's `struct.error`.
fn u32_at(b: &[u8], o: usize, big: bool) -> AResult<u32> {
    let w: [u8; 4] = b.get(o..o + 4).and_then(|s| s.try_into().ok()).ok_or(PYTHON)?;
    Ok(if big {
        u32::from_be_bytes(w)
    } else {
        u32::from_le_bytes(w)
    })
}

fn pad4(n: usize) -> usize {
    (4 - n % 4) % 4
}

/// `b[a:c]` as Python slices it.
fn slice(b: &[u8], a: usize, c: usize) -> Vec<u8> {
    let (a, c) = (a.min(b.len()), c.min(b.len()));
    if a >= c { Vec::new() } else { b[a..c].to_vec() }
}

/// `read_wave_reference(b, source)`.
#[allow(clippy::many_single_char_names, reason = "the reference's names (b, e, n, m, o, w)")]
pub fn read_wave(b: &[u8], source: Source) -> AResult<WaveR> {
    let big = source != Source::Bfbc2Pc;
    let mut w = [0u32; 6];
    let mut o;
    if source == Source::Bf1943 {
        if b.len() < 40 {
            return Err(audio_err(format!(
                "Wave: {} bytes, shorter than a Battlefield 1943 header",
                b.len()
            )));
        }
        for (k, x) in w.iter_mut().enumerate() {
            *x = u32_at(b, 4 + 4 * k, true)?;
        }
        o = 28;
    } else {
        if b.len() < 36 {
            return Err(audio_err(format!(
                "Wave: {} bytes, shorter than a BFBC2 header",
                b.len()
            )));
        }
        for (k, x) in w.iter_mut().enumerate() {
            *x = u32_at(b, 4 * k, big)?;
        }
        o = 24;
    }
    let mut segs: [Option<Segment>; 3] = [None, None, None];
    for (i, seg) in segs.iter_mut().enumerate() {
        if o + 4 > b.len() {
            return Err(audio_err(format!(
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
            return Err(audio_err(format!(
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
            return Err(audio_err(format!(
                "Wave: segment {i}: {m} bytes of stream information run past the end"
            )));
        }
        o = (o + m + pad4(m)).min(b.len());
        *seg = Some((data, info));
    }
    if o != b.len() {
        return Err(audio_err(format!(
            "Wave: the segments end at {o:#x}, the file at {:#x}",
            b.len()
        )));
    }
    if segs[1].is_none() {
        return Err(audio_err("Wave: no loop segment"));
    }
    if source == Source::Bf1943 {
        let [_w04, ch, total, loop_start, loop_end, fid] = w;
        let outro = if segs[2].is_some() {
            i64::from(total) - i64::from(loop_end)
        } else {
            0
        };
        return Ok(WaveR {
            source,
            flags_id: fid,
            total,
            intro: loop_start,
            channels: ch,
            outro,
            word14: 0,
            segments: segs,
        });
    }
    let [fid, total, intro, ch, outro, w14] = w;
    Ok(WaveR {
        source,
        flags_id: fid,
        total,
        intro,
        channels: ch,
        outro: i64::from(outro),
        word14: w14,
        segments: segs,
    })
}

/// `_ea_header(h1, h2)`'s fields (codec, channels, rate, samples).
fn ea_header(h1: u32, h2: u32) -> AResult<(u32, u32, u32, u32)> {
    if (h2 >> 29) & 1 != 0 {
        return Err(audio_err(
            "EA stream: the loop flag is set (no Wave seen has it; its loop start word is not handled)",
        ));
    }
    Ok(((h1 >> 24) & 15, ((h1 >> 18) & 0x3F) + 1, h1 & 0x3FFFF, h2 & 0x1FFF_FFFF))
}

fn stream(h: (u32, u32, u32, u32), blocks: Vec<(u32, Vec<u8>)>) -> Stream {
    Stream {
        codec: h.0,
        channels: h.1,
        rate: h.2,
        samples: h.3,
        blocks,
    }
}

/// `read_sps_reference(s)`.
pub fn read_sps(s: &[u8]) -> AResult<Stream> {
    let mut o = 0usize;
    let mut hdr = None;
    let mut blocks = Vec::new();
    while o + 4 <= s.len() {
        let kind = s[o];
        let n = (usize::from(s[o + 1]) << 16) | (usize::from(s[o + 2]) << 8) | usize::from(s[o + 3]);
        if n < 4 || o + n > s.len() {
            return Err(audio_err(format!(
                "SPS: block {kind:#x} of {n} bytes at {o:#x} does not fit ({} bytes)",
                s.len()
            )));
        }
        match kind {
            0x48 => hdr = Some(ea_header(u32_at(s, o + 4, true)?, u32_at(s, o + 8, true)?)?),
            0x44 => blocks.push((u32_at(s, o + 4, true)?, slice(s, o + 8, o + n))),
            0x45 => {
                o += n;
                break;
            }
            _ => {
                return Err(audio_err(format!("SPS: unknown block kind {kind:#x} at {o:#x}")));
            }
        }
        o += n;
    }
    let Some(h) = hdr else {
        return Err(audio_err("SPS: no header block"));
    };
    if o != s.len() {
        return Err(audio_err(format!("SPS: ends at {o:#x} of {:#x}", s.len())));
    }
    Ok(stream(h, blocks))
}

/// An EA stream's audio blocks: (samples, data).
type Blocks = Vec<(u32, Vec<u8>)>;

/// `_sns_blocks(s, o)`.
fn sns_blocks(s: &[u8], mut o: usize) -> AResult<(Blocks, usize)> {
    let mut blocks = Vec::new();
    while o + 8 <= s.len() {
        let h = u32_at(s, o, true)?;
        let (flags, n) = (h >> 24, (h & 0xFF_FFFF) as usize);
        if n < 8 || o + n > s.len() {
            return Err(audio_err(format!(
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

/// `read_snr_reference(s, sns)`.
pub fn read_snr(s: &[u8], sns: Option<&[u8]>) -> AResult<Stream> {
    if s.len() < 8 {
        return Err(audio_err(format!("SNR: {} bytes, shorter than the header", s.len())));
    }
    let h = ea_header(u32_at(s, 0, true)?, u32_at(s, 4, true)?)?;
    let blocks = if s.len() == 8 {
        let Some(sns) = sns else {
            return Err(audio_err(
                "SNR: a streamed sound (header only); pass its waveDataSns entry",
            ));
        };
        let (blocks, o) = sns_blocks(sns, 0)?;
        if o != sns.len() {
            return Err(audio_err(format!(
                "SNS: the blocks end at {o:#x}, the stream at {:#x}",
                sns.len()
            )));
        }
        blocks
    } else {
        let (blocks, o) = sns_blocks(s, 8)?;
        if o != s.len() {
            return Err(audio_err(format!(
                "SNR: the blocks end at {o:#x}, the data at {:#x}",
                s.len()
            )));
        }
        blocks
    };
    Ok(stream(h, blocks))
}

/// `xma_parts_reference(st)`: the error it raises (the core reads every stream it does not refuse).
pub fn xma_parts(st: &Stream) -> AResult<Vec<Vec<u8>>> {
    let k = (st.channels as usize).div_ceil(2);
    for (bi, (_, b)) in st.blocks.iter().enumerate() {
        let mut o = 0usize;
        for i in 0..k {
            if o + 4 > b.len() {
                return Err(audio_err(format!("EA-XMA: block {bi} ends before stream {i}")));
            }
            let n = (u32_at(b, o, true)? >> 2) as usize;
            if n < 4 || o + n > b.len() {
                return Err(audio_err(format!(
                    "EA-XMA: block {bi}, stream {i}: {n} bytes at {o} do not fit {}",
                    b.len()
                )));
            }
            o += n;
        }
        if o != b.len() && b[o..].iter().any(|x| *x != 0) {
            return Err(audio_err(format!(
                "EA-XMA: block {bi}: the streams end at {o} of {}",
                b.len()
            )));
        }
    }
    Err::<Vec<Vec<u8>>, Outcome>(PYTHON)
}
