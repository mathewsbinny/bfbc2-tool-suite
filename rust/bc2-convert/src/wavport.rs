//! `bfbc2-console-convert/audio.py`'s whole-resource paths in Rust (round F4): `convert` (an Xbox 360 Wave, BFBC2's
//! or Battlefield 1943's, as BFBC2's PC Wave: every segment decoded through ffmpeg's xma2 decoder and encoded as
//! EA-XAS v1 in DICE's blocks) and `decode` (a Wave's segments as int16 samples), with `segment_stream`,
//! `decode_xma`, `decode_xas` and the ffmpeg glue, over the Rust readers and writers (`bc2_formats::wave`, F0;
//! `bc2_formats::audioframe`, F3; [`crate::audio`]'s codec, E3a).
//!
//! ffmpeg runs as the Python runs it (`ffmpeg_decode`: `-v error -f wav -i pipe:0 -f s16le -acodec pcm_s16le
//! pipe:1`, the file on its standard input); the caller passes the executable `audio.ffmpeg_path()` found. Every
//! function answers `None` where the Python raises (an `AudioError`, NumPy's or `struct`'s errors), where a Rust
//! reader refuses what the Python reference may read, and where ffmpeg fails or cannot start: the caller then runs
//! the Python, whose answer or error it is.

use std::io::{Read as _, Write as _};
use std::process::{Command, Stdio};

use bc2_formats::{audioframe, wave};

use crate::audio as xas;

/// The ffmpeg executable (`audio.ffmpeg_path()`), or none yet: a sound that needs it then sets `wanted` and is not
/// converted (the caller finds ffmpeg and asks again; most sounds - PC Waves, errors - never need it).
#[derive(Debug, Default)]
pub struct Ffmpeg<'a> {
    /// The executable's path.
    pub path: Option<&'a str>,
    /// Set when a sound needed ffmpeg and `path` was `None`.
    pub wanted: std::sync::atomic::AtomicBool,
}

/// A decoded stream (`audio.Stream`'s fields the conversion reads).
struct Stream {
    codec: u32,
    channels: u32,
    rate: u32,
    samples: u32,
    blocks: Vec<(u32, Vec<u8>)>,
}

fn stream_of(s: wave::Stream) -> Stream {
    Stream {
        codec: s.codec,
        channels: s.channels,
        rate: s.rate,
        samples: s.samples,
        blocks: s.blocks,
    }
}

fn in_memory(st: &Stream) -> u64 {
    st.blocks.iter().map(|(n, _)| u64::from(*n)).sum()
}

/// `audio.SOURCES`' names.
fn source_of(s: &str) -> Option<wave::Source> {
    Some(match s {
        "bfbc2-xbox" => wave::Source::Bfbc2Xbox,
        "bfbc2-pc" => wave::Source::Bfbc2Pc,
        "bf1943" => wave::Source::Bf1943,
        _ => return None,
    })
}

/// `segment_stream(w, i, streams)`.
fn segment_stream(w: &wave::Wave, src: wave::Source, i: usize, streams: &[Option<&[u8]>; 3]) -> Option<Stream> {
    let (data, info) = w.segments.get(i)?.as_ref()?;
    let sns = streams.get(i).copied().flatten();
    if src == wave::Source::Bf1943 {
        if !info.is_empty() && data.len() > 8 {
            let st = stream_of(wave::read_snr(data, None).ok()?);
            return (in_memory(&st) == u64::from(st.samples)).then_some(st);
        }
        return wave::read_snr(data, sns).ok().map(stream_of);
    }
    let st = if data.len() == 4 {
        stream_of(wave::read_sps(sns?).ok()?)
    } else {
        stream_of(wave::read_sps(data).ok()?)
    };
    (in_memory(&st) == u64::from(st.samples)).then_some(st)
}

/// `ffmpeg_decode(data, 'wav', channels)`: the int16 samples ffmpeg gives, interleaved.
fn ffmpeg_decode(ffmpeg: &str, data: &[u8], channels: usize) -> Option<Vec<i16>> {
    let mut child = Command::new(ffmpeg)
        .args([
            "-v",
            "error",
            "-f",
            "wav",
            "-i",
            "pipe:0",
            "-f",
            "s16le",
            "-acodec",
            "pcm_s16le",
            "pipe:1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let stdin = child.stdin.take();
    let input = data.to_vec();
    let writer = std::thread::spawn(move || {
        if let Some(mut s) = stdin {
            let _ = s.write_all(&input);
        }
    });
    let stderr = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut e = Vec::new();
        if let Some(mut s) = stderr {
            let _ = s.read_to_end(&mut e);
        }
    });
    let mut out = Vec::new();
    if let Some(mut s) = child.stdout.take() {
        let _ = s.read_to_end(&mut out);
    }
    let _ = reader.join();
    let _ = writer.join();
    let status = child.wait().ok()?;
    if status.code() != Some(0) || (out.is_empty() && !data.is_empty()) {
        return None; // the Python's AudioError, with ffmpeg's message
    }
    if channels == 0 || !out.len().is_multiple_of(2 * channels) {
        return None; // NumPy's reshape error
    }
    Some(out.as_chunks::<2>().0.iter().map(|p| i16::from_le_bytes(*p)).collect())
}

/// `decode_xma(st)`: interleaved int16 samples, cut or padded with silence to the header's count.
fn decode_xma(st: &Stream, ffmpeg: &Ffmpeg<'_>) -> Option<Vec<i16>> {
    if st.codec != 3 {
        return None;
    }
    let views: Vec<&[u8]> = st.blocks.iter().map(|(_, d)| d.as_slice()).collect();
    let parts = audioframe::xma_parts(i64::from(st.channels), &views).ok()?;
    if parts.is_empty() {
        return None; // min() of nothing: ValueError
    }
    let Some(ffmpeg) = ffmpeg.path else {
        ffmpeg.wanted.store(true, std::sync::atomic::Ordering::Relaxed);
        return None;
    };
    let ch_all = st.channels as usize;
    let mut cols: Vec<(usize, Vec<i16>)> = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        let ch = 2.min(ch_all.checked_sub(2 * i)?);
        let riff = audioframe::xma2_riff(p, ch as i64, i64::from(st.rate), i64::from(st.samples)).ok()?;
        cols.push((ch, ffmpeg_decode(ffmpeg, &riff, ch)?));
    }
    let n = cols.iter().map(|(ch, c)| c.len() / ch).min().unwrap_or(0);
    let want = st.samples as usize;
    let mut pcm = Vec::with_capacity(want * ch_all);
    for row in 0..want {
        if row < n {
            for (ch, c) in &cols {
                pcm.extend_from_slice(c.get(row * ch..(row + 1) * ch)?);
            }
        } else {
            pcm.extend(std::iter::repeat_n(0i16, ch_all));
        }
    }
    Some(pcm)
}

/// `decode_xas(st)`: interleaved int16 samples, cut to the header's count.
fn decode_xas(st: &Stream) -> Option<Vec<i16>> {
    let ch = st.channels as usize;
    let mut data: Vec<u8> = Vec::with_capacity(st.blocks.iter().map(|(_, b)| b.len()).sum());
    for (_, b) in &st.blocks {
        data.extend_from_slice(b);
    }
    if ch == 0 || !data.len().is_multiple_of(xas::FRAME * ch) {
        return None;
    }
    let dec = xas::decode_frames(&data).ok()?;
    let frames = data.len() / (xas::FRAME * ch);
    let per = xas::FRAME_SAMPLES;
    let total = (frames * per).min(st.samples as usize);
    if dec.len() < frames * ch * per {
        return None;
    }
    let mut out = vec![0i16; total * ch];
    for (c, chunk) in dec.chunks_exact(per).enumerate() {
        // frame f, channel k: its 128 samples go to rows f * 128 .. f * 128 + 127, column k
        let (f, k) = (c / ch, c % ch);
        for (s, &v) in chunk.iter().enumerate() {
            let row = f * per + s;
            if row < total
                && let Some(o) = out.get_mut(row * ch + k)
            {
                *o = v;
            }
        }
    }
    Some(out)
}

/// `audio.convert(b, source, streams, word14)`: the PC Wave's bytes. `streams`: each segment's `waveDataSns` bytes
/// (`streams.get(i)`); `ffmpeg`: see [`Ffmpeg`].
#[must_use]
pub fn convert(
    b: &[u8],
    source: &str,
    streams: &[Option<&[u8]>; 3],
    word14: u32,
    ffmpeg: &Ffmpeg<'_>,
) -> Option<Vec<u8>> {
    let source = if source == "bfbc2" { "bfbc2-xbox" } else { source };
    if !matches!(source, "bfbc2-xbox" | "bf1943") {
        return None;
    }
    let src = source_of(source)?;
    let w = wave::read_wave(b, src).ok()?;
    let mut segs: Vec<Option<Vec<u8>>> = Vec::new();
    for i in 0..3 {
        if w.segments.get(i)?.is_none() {
            segs.push(None);
            continue;
        }
        let st = segment_stream(&w, src, i, streams)?;
        if st.codec != 3 || st.channels != w.channels {
            return None;
        }
        let pcm = decode_xma(&st, ffmpeg)?;
        let ch = st.channels as usize;
        let frames = xas::encode(&pcm, ch).ok()?;
        let blocks = audioframe::xas_blocks(&frames, u64::from(st.samples), ch as u64, st.rate).ok()?;
        if st.rate >= 1 << 18 || st.channels == 0 || st.channels > 64 || st.samples >= 1 << 30 {
            return None;
        }
        let header = (
            1u32 << 28 | 4 << 24 | (st.channels - 1) << 18 | st.rate,
            1u32 << 30 | st.samples,
        );
        let views: Vec<(u32, &[u8])> = blocks
            .iter()
            .map(|(k, d)| u32::try_from(*k).map(|k| (k, *d)))
            .collect::<Result<_, _>>()
            .ok()?;
        segs.push(Some(wave::write_sps(header, &views).ok()?));
    }
    let word14 = if source == "bfbc2-xbox" { w.word14 } else { word14 };
    let outro = u32::try_from(w.outro).ok()?;
    let words = [w.flags_id, w.total, w.intro, w.channels, outro, word14];
    let view = |i: usize| segs.get(i).and_then(Option::as_ref).map(|s| (s.as_slice(), &[][..]));
    wave::write_wave_pc(&words, &[view(0), view(1), view(2)]).ok()
}

/// `audio.decode(b, source, streams)`: `(samples interleaved, channels, rate)`.
#[must_use]
pub fn decode(
    b: &[u8],
    source: &str,
    streams: &[Option<&[u8]>; 3],
    ffmpeg: &Ffmpeg<'_>,
) -> Option<(Vec<i16>, usize, u32)> {
    let src = source_of(source)?;
    let w = wave::read_wave(b, src).ok()?;
    let mut out: Vec<i16> = Vec::new();
    let mut channels: Option<usize> = None;
    let mut rate = None;
    for i in 0..3 {
        if w.segments.get(i)?.is_none() {
            continue;
        }
        let st = segment_stream(&w, src, i, streams)?;
        rate = Some(st.rate);
        let pcm = match st.codec {
            3 => decode_xma(&st, ffmpeg)?,
            4 => decode_xas(&st)?,
            _ => return None,
        };
        let ch = st.channels as usize;
        if channels.is_some_and(|c| c != ch) {
            return None; // np.vstack's ValueError
        }
        channels = Some(ch);
        out.extend(pcm);
    }
    Some((out, channels?, rate?))
}
