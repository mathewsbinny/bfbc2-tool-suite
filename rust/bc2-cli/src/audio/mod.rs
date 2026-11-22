//! Round O3's `audio` group: `bfbc2 audio bfbc2|bf1943 <input> <output> [--stream N=FILE]`, the command line of
//! `bfbc2-console-convert\audio.py` (an Xbox 360 Wave resource, BFBC2's or Battlefield 1943's, as BFBC2's PC Wave)
//! over the Rust core: the Wave and EA stream framing (`bc2_formats::wave`, round F0), the EA-XMA parts and the XMA2
//! RIFF (`bc2_formats::audioframe`, F3), EA-XMA decoded in process (`bc2_xma`, round PR1b-1c: no ffmpeg), the EA-XAS encoder
//! (`bc2_convert::audio`, E3a) and DICE's blocks.
//!
//! The readers' errors: where the core refuses a resource or stream, [`reference`] reads it as the Python reference
//! does, for the `AudioError` it raises (or, for a Battlefield 1943 sample rate that is NaN, the values: the core
//! leaves those to the reference). `argparse`'s texts at the console's width or `COLUMNS` are [`crate::argfmt`]'s
//! (round RT2). Handed to the Python: `--` and single-dash options other than `-h`; a `struct.error` of a cut header block; an `int()` the port does not
//! read exactly (non-ASCII digits, a literal past 200 characters in its error); an XMA stream the in-process
//! decoder gives no samples for.

#![allow(
    clippy::indexing_slicing,
    reason = "indexes within lengths checked just before (the arguments, the segments, the decoded samples)"
)]

mod reference;

use bc2_convert::audio as xas;
use bc2_formats::{audioframe, wave};

use crate::pyfmt::str_repr;
use crate::pyproc::{Fail, Py, PyErr, PyResult, Tool, read_file, write_file};
use reference::{Stream, WaveR};

/// The group's tools.
pub const TOOLS: &[Tool] = &[Tool {
    name: "audio",
    script: "bfbc2-console-convert\\audio.py",
    log: None,
    run,
}];

const USAGE: &str = "usage: audio.py [-h] [--stream N=FILE] {bfbc2,bf1943} input output\n";
const HELP: &str = "usage: audio.py [-h] [--stream N=FILE] {bfbc2,bf1943} input output\n\nAn Xbox 360 Wave resource \
     (BFBC2 / Battlefield 1943) as BFBC2 PC.\n\npositional arguments:\n  {bfbc2,bf1943}\n  input\n  output\n\noptions:\n  \
     -h, --help       show this help message and exit\n  --stream N=FILE  a streamed segment's waveDataSns_<N>.res (N: \
     0 intro, 1\n                   loop, 2 outro)\n";
const WHAT_TO_DO: &str = "Check that the resource is a Wave from BFBC2 (Xbox 360 or PC) or Battlefield 1943 (Xbox \
     360); a streamed sound needs its <name>_waveDataSns_<n>.res from the streaming bundle; decoding EA-XMA needs ffmpeg \
     (PATH or $FFMPEG).";
const CHOICES: [&str; 2] = ["bfbc2", "bf1943"];

/// How a conversion stops: `AudioError(msg)` (printed by `main`: `error: ...` and what to do, exit code 1), or as a
/// command line does.
pub enum Outcome {
    Audio(String),
    Fail(Fail),
}

impl From<Fail> for Outcome {
    fn from(f: Fail) -> Self {
        Self::Fail(f)
    }
}

type AResult<T> = Result<T, Outcome>;

fn audio_err(msg: impl Into<String>) -> Outcome {
    Outcome::Audio(msg.into())
}

const PYTHON: Outcome = Outcome::Fail(Fail::Python);

// ------------------------------------------------------------------------------------------------ argparse

struct Args {
    source: String,
    input: String,
    output: String,
    streams: Vec<String>,
}

/// `parser.error(message)`: the usage (at the console's width) and the message on stderr, exit code 2.
fn error(py: &mut Py, message: &str) -> PyResult<Option<Args>> {
    py.eprint(crate::argfmt::usage_or("audio.py", USAGE)?.trim_end_matches('\n'));
    py.eprint(&format!("audio.py: error: {message}"));
    Err(Fail::Err(PyErr::new("SystemExit", "2")))
}

/// The help on stdout, exit code 0.
fn help(py: &mut Py) -> PyResult<Option<Args>> {
    py.write(&crate::argfmt::help_or("audio.py", HELP)?)?;
    Ok(None)
}

/// Whether `argparse` takes an argument as an option string: it starts with '-', is not '-' alone and is not a
/// negative number (the parser has no option that looks like one).
fn is_option(a: &str) -> bool {
    let Some(rest) = a.strip_prefix('-') else {
        return false;
    };
    if rest.is_empty() {
        return false;
    }
    let digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    let neg = match rest.split_once('.') {
        None => digits(rest),
        Some((i, f)) => digits(i) && !f.is_empty() && digits(f),
    };
    !neg
}

/// A long option as `argparse` resolves it: exact, or a unique prefix of `--help` / `--stream` (an abbreviation);
/// None: none of the parser's.
fn resolve(name: &str) -> Option<&'static str> {
    const LONG: [&str; 2] = ["--help", "--stream"];
    if let Some(f) = LONG.iter().find(|f| **f == name) {
        return Some(f);
    }
    let hits: Vec<&'static str> = LONG
        .into_iter()
        .filter(|f| name.len() > 2 && f.starts_with(name))
        .collect();
    match hits.as_slice() {
        [one] => Some(one),
        _ => None,
    }
}

/// `ap.parse_args(argv)`: Some(arguments), None after the help, Err(SystemExit 2) after an error.
fn parse(py: &mut Py, argv: &[String]) -> PyResult<Option<Args>> {
    const POS: [&str; 3] = ["source", "input", "output"];
    let mut vals: Vec<String> = Vec::new();
    let mut streams = Vec::new();
    let mut extras: Vec<String> = Vec::new();
    let mut i = 0;
    while i < argv.len() {
        let a = argv[i].as_str();
        if is_option(a) {
            if a == "-h" {
                return help(py);
            }
            if a == "--" || !a.starts_with("--") {
                return Err(Fail::Python);
            }
            let (name, inline) = match a.split_once('=') {
                Some((n, v)) => (n, Some(v)),
                None => (a, None),
            };
            match resolve(name) {
                Some("--help") => {
                    if inline.is_some() {
                        return Err(Fail::Python);
                    }
                    return help(py);
                }
                Some(_) => {
                    if let Some(v) = inline {
                        streams.push(v.to_owned());
                    } else {
                        match argv.get(i + 1) {
                            Some(v) if v == "--" => return Err(Fail::Python),
                            Some(v) if !is_option(v) => {
                                streams.push(v.clone());
                                i += 1;
                            }
                            _ => return error(py, "argument --stream: expected one argument"),
                        }
                    }
                }
                // no option, with a space: argparse takes it for a positional value (round RT3: left to it)
                None if a.contains(' ') => return Err(Fail::Python),
                None => extras.push(a.to_owned()),
            }
            i += 1;
            continue;
        }
        // a run of values: the positionals still to fill take one each, as many as the run holds
        let mut run = 0;
        while i + run < argv.len() && !is_option(&argv[i + run]) {
            run += 1;
        }
        let take = (POS.len() - vals.len()).min(run);
        for v in &argv[i..i + take] {
            if vals.is_empty() && !CHOICES.contains(&v.as_str()) {
                let choices: Vec<String> = CHOICES.iter().map(|c| str_repr(c)).collect();
                let msg = format!(
                    "argument source: invalid choice: {} (choose from {})",
                    str_repr(v),
                    choices.join(", ")
                );
                return error(py, &msg);
            }
            vals.push(v.clone());
        }
        extras.extend(argv[i + take..i + run].iter().cloned());
        i += run;
    }
    if vals.len() < POS.len() {
        let msg = format!("the following arguments are required: {}", POS[vals.len()..].join(", "));
        return error(py, &msg);
    }
    if !extras.is_empty() {
        return error(py, &format!("unrecognized arguments: {}", extras.join(" ")));
    }
    let mut it = vals.into_iter();
    let (source, input, output) = (
        it.next().unwrap_or_default(),
        it.next().unwrap_or_default(),
        it.next().unwrap_or_default(),
    );
    Ok(Some(Args {
        source,
        input,
        output,
        streams,
    }))
}

/// `int(text)` (base 10): Some(value) (None past i128: no segment's index), or its `ValueError`.
fn py_int(text: &str) -> PyResult<Option<i128>> {
    let invalid = || {
        let r = str_repr(text);
        if r.chars().count() > 200 {
            return Fail::Python;
        }
        Fail::Err(PyErr::new(
            "ValueError",
            format!("invalid literal for int() with base 10: {r}"),
        ))
    };
    let s = text.trim_matches(bc2_pyrt::py::py_space);
    if !s.is_ascii() {
        return Err(Fail::Python);
    }
    let body = s.strip_prefix(['-', '+']).unwrap_or(s);
    let neg = s.starts_with('-');
    let b = body.as_bytes();
    let ok = b.first().is_some_and(u8::is_ascii_digit)
        && b.last().is_some_and(u8::is_ascii_digit)
        && b.iter().all(|c| c.is_ascii_digit() || *c == b'_')
        && !b.windows(2).any(|w| w == b"__");
    if !ok {
        return Err(invalid());
    }
    let digits: String = body.chars().filter(|c| *c != '_').collect();
    Ok(digits.parse::<i128>().ok().map(|v| if neg { -v } else { v }))
}

// ------------------------------------------------------------------------------------------------ the conversion

/// `read_wave(b, source)`: the core's reader, else the reference's answer.
fn read_wave(b: &[u8], source: &str) -> AResult<WaveR> {
    let src = match source {
        "bfbc2-xbox" => wave::Source::Bfbc2Xbox,
        "bfbc2-pc" => wave::Source::Bfbc2Pc,
        _ => wave::Source::Bf1943,
    };
    match wave::read_wave(b, src) {
        Ok(w) => Ok(WaveR {
            source: src,
            flags_id: w.flags_id,
            total: w.total,
            intro: w.intro,
            channels: w.channels,
            outro: w.outro,
            word14: w.word14,
            segments: w.segments,
        }),
        Err(_) => reference::read_wave(b, src),
    }
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

fn read_sps(s: &[u8]) -> AResult<Stream> {
    match wave::read_sps(s) {
        Ok(st) => Ok(stream_of(st)),
        Err(_) => reference::read_sps(s),
    }
}

fn read_snr(s: &[u8], sns: Option<&[u8]>) -> AResult<Stream> {
    match wave::read_snr(s, sns) {
        Ok(st) => Ok(stream_of(st)),
        Err(_) => reference::read_snr(s, sns),
    }
}

fn in_memory(st: &Stream) -> u64 {
    st.blocks.iter().map(|(n, _)| u64::from(*n)).sum()
}

/// `segment_stream(w, i, streams)`.
fn segment_stream(w: &WaveR, i: usize, streams: &[(i128, Vec<u8>)]) -> AResult<Stream> {
    let Some((data, info)) = w.segments.get(i).and_then(Option::as_ref) else {
        return Err(PYTHON);
    };
    // a dict: the last value given for a key
    let sns = streams
        .iter()
        .rev()
        .find(|(k, _)| *k == i as i128)
        .map(|(_, v)| v.as_slice());
    if w.source == wave::Source::Bf1943 {
        if !info.is_empty() && data.len() > 8 {
            let st = read_snr(data, None)?;
            if in_memory(&st) != u64::from(st.samples) {
                return Err(audio_err(format!(
                    "segment {i}: in memory with stream information, {} of {} samples in memory (the rest is \
                     streamed)",
                    in_memory(&st),
                    st.samples
                )));
            }
            return Ok(st);
        }
        return read_snr(data, sns);
    }
    let st = if data.len() == 4 {
        let Some(sns) = sns else {
            let w4: [u8; 4] = data.as_slice().try_into().unwrap_or([0; 4]);
            let n = if w.source == wave::Source::Bfbc2Xbox {
                u32::from_be_bytes(w4)
            } else {
                u32::from_le_bytes(w4)
            };
            return Err(audio_err(format!(
                "segment {i}: streamed ({n} samples); pass its waveDataSns entry"
            )));
        };
        read_sps(sns)?
    } else {
        read_sps(data)?
    };
    if in_memory(&st) != u64::from(st.samples) {
        return Err(audio_err(format!(
            "segment {i}: {} of {} samples in memory (the head of a streamed sound; pass its waveDataSns entry)",
            in_memory(&st),
            st.samples
        )));
    }
    Ok(st)
}

/// `CODECS.get(codec, '?')`.
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

/// `decode_xma(st)`: interleaved int16 samples, cut or padded with silence to the header's count.
fn decode_xma(st: &Stream) -> AResult<Vec<i16>> {
    let views: Vec<&[u8]> = st.blocks.iter().map(|(_, d)| d.as_slice()).collect();
    let parts = match audioframe::xma_parts(i64::from(st.channels), &views) {
        Ok(p) => p,
        Err(_) => reference::xma_parts(st)?,
    };
    let ch_all = st.channels as usize;
    let mut cols: Vec<(usize, Vec<i16>)> = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        let ch = 2.min(ch_all - 2 * i);
        // the RIFF's own checks (a field past its width is the Python's struct.error)
        audioframe::xma2_riff(&[], ch as i64, i64::from(st.rate), i64::from(st.samples)).map_err(|_| PYTHON)?;
        // decoded in process (bc2_xma, round PR1b-1c); a stream it gives no samples for is the Python's
        cols.push((ch, bc2_convert::wavport::decode_xma_part(p, ch, st.rate).ok_or(PYTHON)?));
    }
    let n = cols.iter().map(|(ch, c)| c.len() / ch).min().unwrap_or(0);
    let want = st.samples as usize;
    let mut pcm = Vec::with_capacity(want * ch_all);
    for row in 0..want {
        if row < n {
            for (ch, c) in &cols {
                pcm.extend_from_slice(&c[row * ch..(row + 1) * ch]);
            }
        } else {
            pcm.extend(std::iter::repeat_n(0i16, ch_all));
        }
    }
    Ok(pcm)
}

/// `convert(b, source, streams)`: the PC Wave's bytes.
fn convert(b: &[u8], source: &str, streams: &[(i128, Vec<u8>)]) -> AResult<Vec<u8>> {
    let source = if source == "bfbc2" { "bfbc2-xbox" } else { source };
    let w = read_wave(b, source)?;
    let mut segs: Vec<Option<Vec<u8>>> = Vec::new();
    for i in 0..3 {
        if w.segments[i].is_none() {
            segs.push(None);
            continue;
        }
        let st = segment_stream(&w, i, streams)?;
        if st.codec != 3 {
            return Err(audio_err(format!(
                "segment {i}: codec {} ({}), not EA-XMA",
                st.codec,
                codec_name(st.codec)
            )));
        }
        if st.channels != w.channels {
            return Err(audio_err(format!(
                "segment {i}: {} channels, the Wave says {}",
                st.channels, w.channels
            )));
        }
        let pcm = decode_xma(&st)?;
        let ch = st.channels as usize;
        let frames = xas::encode(&pcm, ch).map_err(|_| PYTHON)?;
        let blocks = audioframe::xas_blocks(&frames, u64::from(st.samples), ch as u64, st.rate).map_err(|_| PYTHON)?;
        let header = (
            1u32 << 28 | 4 << 24 | (st.channels - 1) << 18 | st.rate,
            1u32 << 30 | st.samples,
        );
        let views: Vec<(u32, &[u8])> = blocks.iter().map(|(k, d)| (*k as u32, *d)).collect();
        segs.push(Some(wave::write_sps(header, &views).map_err(|_| PYTHON)?));
    }
    let word14 = if source == "bfbc2-xbox" { w.word14 } else { 0 };
    let outro = u32::try_from(w.outro).map_err(|_| PYTHON)?;
    let words = [w.flags_id, w.total, w.intro, w.channels, outro, word14];
    let view = |i: usize| segs[i].as_ref().map(|s| (s.as_slice(), &[][..]));
    wave::write_wave_pc(&words, &[view(0), view(1), view(2)]).map_err(|_| PYTHON)
}

/// `main(argv)`.
pub fn run(py: &mut Py, argv: &[String]) -> PyResult<i32> {
    let a = match parse(py, argv) {
        Ok(Some(a)) => a,
        Ok(None) => return Ok(0),
        Err(Fail::Err(e)) if e.kind == "SystemExit" => return Ok(2),
        Err(e) => return Err(e),
    };
    let mut streams: Vec<(i128, Vec<u8>)> = Vec::new();
    for s in &a.streams {
        let (n, p) = s.split_once('=').unwrap_or((s.as_str(), ""));
        let key = py_int(n)?;
        let data = read_file(p)?;
        streams.push((key.unwrap_or(i128::MAX), data));
    }
    let data = read_file(&a.input)?;
    let out = match convert(&data, &a.source, &streams) {
        Ok(o) => o,
        Err(Outcome::Audio(msg)) => {
            py.print(&format!("error: {msg}\n{WHAT_TO_DO}"))?;
            return Ok(1);
        }
        Err(Outcome::Fail(f)) => return Err(f),
    };
    write_file(&a.output, &out)?;
    py.print(&format!("{}: {} bytes", a.output, out.len()))?;
    Ok(0)
}
