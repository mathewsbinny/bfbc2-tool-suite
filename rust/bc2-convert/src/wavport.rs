//! `bfbc2-console-convert/audio.py`'s whole-resource paths in Rust (round F4): `convert` (an Xbox 360 Wave, BFBC2's
//! or Battlefield 1943's, as BFBC2's PC Wave: every segment decoded from EA-XMA and encoded as EA-XAS v1 in DICE's
//! blocks) and `decode` (a Wave's segments as int16 samples), with `segment_stream`, `decode_xma`, `decode_xas`,
//! over the Rust readers and writers (`bc2_formats::wave`, F0; `bc2_formats::audioframe`, F3; [`crate::audio`]'s
//! codec, E3a).
//!
//! EA-XMA is decoded in process by `bc2_xma` (round PR1b-1c: FFmpeg's xma2 decoder ported, its C path's samples to
//! the bit), where the Python reference ran ffmpeg once per XMA stream (`-f wav -i pipe:0 -f s16le`). Every function
//! answers `None` where the Python raises (an `AudioError`, NumPy's or `struct`'s errors) and where a Rust reader
//! refuses what the Python reference may read: the caller then runs the Python, whose answer or error it is.
//!
//! `bc2_xma` is LGPL-2.1-or-later and linked only with the crate's `xma` feature (round XM2); without it
//! [`decode_xma_part`] gives `None`, so an EA-XMA stream does not convert (the EA-XAS paths are unchanged).

use bc2_formats::{audioframe, wave};

use crate::audio as xas;

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

/// One XMA stream's packets (one or two channels) decoded as ffmpeg's C path decodes the XMA2 RIFF
/// `audio.xma2_riff` makes of them (XMA-DEC-1): interleaved int16 samples. Damaged packets decode as ffmpeg decodes
/// them (its errors skipped, samples given); a stream that gives no samples is empty (the caller pads it with silence
/// to the header's count, as it pads a short one), where the ffmpeg run raised its `AudioError` for an empty output.
/// `None` always in a build without the `xma` feature (no decoder linked).
#[must_use]
#[cfg(feature = "xma")]
pub fn decode_xma_part(packets: &[u8], channels: usize, rate: u32) -> Option<Vec<i16>> {
    // the RIFF's average bytes a second (rate * channels * 2) sets ffmpeg's demuxer packets
    let byte_rate = rate.saturating_mul(channels as u32).saturating_mul(2);
    let d = bc2_xma::decode(packets, channels, rate, byte_rate).ok()?;
    Some(bc2_xma::to_s16(&d.planar))
}

/// [`decode_xma_part`] in a build without the `xma` feature: the decoder is not linked, so no stream decodes.
#[must_use]
#[cfg(not(feature = "xma"))]
pub fn decode_xma_part(_packets: &[u8], _channels: usize, _rate: u32) -> Option<Vec<i16>> {
    None
}

/// `decode_xma(st)`: interleaved int16 samples, cut or padded with silence to the header's count.
fn decode_xma(st: &Stream) -> Option<Vec<i16>> {
    if st.codec != 3 {
        return None;
    }
    let views: Vec<&[u8]> = st.blocks.iter().map(|(_, d)| d.as_slice()).collect();
    let parts = audioframe::xma_parts(i64::from(st.channels), &views).ok()?;
    if parts.is_empty() {
        return None; // min() of nothing: ValueError
    }
    let ch_all = st.channels as usize;
    let mut cols: Vec<(usize, Vec<i16>)> = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        let ch = 2.min(ch_all.checked_sub(2 * i)?);
        // the RIFF's own checks (a field past its width is the Python's struct.error)
        audioframe::xma2_riff(&[], ch as i64, i64::from(st.rate), i64::from(st.samples)).ok()?;
        cols.push((ch, decode_xma_part(p, ch, st.rate)?));
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
/// (`streams.get(i)`).
#[must_use]
pub fn convert(b: &[u8], source: &str, streams: &[Option<&[u8]>; 3], word14: u32) -> Option<Vec<u8>> {
    convert_checked(b, source, streams, word14).ok()
}

/// The EA codec names (`audio.CODECS`).
fn codec_name(c: u32) -> &'static str {
    match c {
        2 => "PCM16BE",
        3 => "EA-XMA",
        4 => "EA-XAS",
        5 => "EALayer3 v1",
        6 => "EALayer3 v2 PCM",
        7 => "EALayer3 v2 Spike",
        _ => "?",
    }
}

/// [`convert`], or why the Wave does not convert (round OR8: the sound porter's report; the Python's `AudioError`
/// texts where they say the same).
///
/// # Errors
/// A Wave that does not read, a segment that is not EA-XMA, has another channel count than the Wave or does not
/// decode, a converted segment that does not fit the PC Wave.
pub fn convert_checked(b: &[u8], source: &str, streams: &[Option<&[u8]>; 3], word14: u32) -> Result<Vec<u8>, String> {
    let source = if source == "bfbc2" { "bfbc2-xbox" } else { source };
    let src = match source {
        "bfbc2-xbox" | "bf1943" => source_of(source).ok_or_else(|| format!("convert: source '{source}'"))?,
        _ => return Err(format!("convert: source '{source}' (bfbc2 or bf1943)")),
    };
    let w = wave::read_wave(b, src).map_err(|e| e.to_string())?;
    let mut segs: Vec<Option<Vec<u8>>> = Vec::new();
    for i in 0..3 {
        if w.segments
            .get(i)
            .ok_or("the Wave has fewer than three segments")?
            .is_none()
        {
            segs.push(None);
            continue;
        }
        let st = segment_stream(&w, src, i, streams)
            .ok_or_else(|| format!("segment {i}: its EA stream does not read (or its streamed part is missing)"))?;
        if st.codec != 3 {
            return Err(format!(
                "segment {i}: codec {} ({}), not EA-XMA",
                st.codec,
                codec_name(st.codec)
            ));
        }
        if st.channels != w.channels {
            return Err(format!(
                "segment {i}: {} channels, the Wave says {}",
                st.channels, w.channels
            ));
        }
        let pcm = decode_xma(&st).ok_or_else(|| format!("segment {i}: the EA-XMA stream does not decode"))?;
        let ch = st.channels as usize;
        let frames = xas::encode(&pcm, ch).map_err(|e| format!("segment {i}: {e}"))?;
        let blocks = audioframe::xas_blocks(&frames, u64::from(st.samples), ch as u64, st.rate)
            .map_err(|e| format!("segment {i}: {e}"))?;
        if st.rate >= 1 << 18 || st.channels == 0 || st.channels > 64 || st.samples >= 1 << 30 {
            return Err(format!(
                "segment {i}: {} channels at {} Hz, {} samples: not an EA-XAS header",
                st.channels, st.rate, st.samples
            ));
        }
        let header = (
            1u32 << 28 | 4 << 24 | (st.channels - 1) << 18 | st.rate,
            1u32 << 30 | st.samples,
        );
        let views: Vec<(u32, &[u8])> = blocks
            .iter()
            .map(|(k, d)| u32::try_from(*k).map(|k| (k, *d)))
            .collect::<Result<_, _>>()
            .map_err(|_| format!("segment {i}: a block past 4 GB"))?;
        segs.push(Some(
            wave::write_sps(header, &views).map_err(|e| format!("segment {i}: {e}"))?,
        ));
    }
    let word14 = if source == "bfbc2-xbox" { w.word14 } else { word14 };
    let outro = u32::try_from(w.outro).map_err(|_| "the Wave's outro does not fit".to_owned())?;
    let words = [w.flags_id, w.total, w.intro, w.channels, outro, word14];
    let view = |i: usize| segs.get(i).and_then(Option::as_ref).map(|s| (s.as_slice(), &[][..]));
    wave::write_wave_pc(&words, &[view(0), view(1), view(2)]).map_err(|e| e.to_string())
}

/// `audio.decode(b, source, streams)`: `(samples interleaved, channels, rate)`.
#[must_use]
pub fn decode(b: &[u8], source: &str, streams: &[Option<&[u8]>; 3]) -> Option<(Vec<i16>, usize, u32)> {
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
            3 => decode_xma(&st)?,
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
