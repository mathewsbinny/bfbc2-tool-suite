//! An XMA2 decoder: FFmpeg's (`libavcodec/wmaprodec.c`'s `xma2` decoder, FFmpeg 9.0) ported to Rust, so the suite
//! decodes the Xbox 360's EA-XMA sounds without running ffmpeg (round PR1b-1c).
//!
//! The decoder follows FFmpeg's C code operation for operation, from the WAV demuxer's packets (`wavdec.c`, the
//! PCM packet size rule of `pcm.c`) through the XMA wrapper (`xma_decode_packet`: its sample FIFO, the 64 samples
//! the decoder skips at the start, the trim at the end) and the WMA Pro frames, to the float samples; [`to_s16`] is
//! libswresample's float to 16-bit conversion. Its samples are FFmpeg's C path's to the bit (`ffmpeg -cpuflags 0`);
//! FFmpeg's default run uses SIMD transforms that add in other orders and differ from it, and from this decoder, by
//! one in the last bit on about one 16-bit sample in 7,000 (rust/BENCHMARKS.md, "PR1b-1c").
//!
//! Licence: this crate is a translation of FFmpeg's code and tables, so it is under FFmpeg's licence, the GNU Lesser
//! General Public License 2.1 or later (README.md beside this crate), not the suite's MIT licence.

#![allow(
    clippy::indexing_slicing,
    reason = "a codec port: every index is within a buffer FFmpeg sizes the same way (frame and window offsets,               coefficients masked to the block, symbols of complete code tables); random packets are tested not to               panic (tests::random_packets_do_not_panic)"
)]
#![allow(
    clippy::cast_precision_loss,
    reason = "FFmpeg's int to float conversions, as C converts them"
)]
#![allow(clippy::doc_markdown, reason = "FFmpeg's identifiers named as they are spelled")]

mod bits;
mod mdct;
#[allow(
    clippy::unreadable_literal,
    dead_code,
    reason = "FFmpeg's tables as they are written, every size kept"
)]
mod tables;
mod wmapro;

use wmapro::{PACKET, SAMPLES_PER_FRAME, Stream};

/// Why a stream could not be decoded.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// Not a RIFF WAVE with an XMA2 `fmt ` (format 0x166, `XMA2WAVEFORMATEX`) and a `data` chunk.
    #[error("not an XMA2 RIFF: {0}")]
    Riff(&'static str),
    /// A layout FFmpeg's decoder refuses (channels other than 1 or 2 in one stream, more than one stream, rate 0).
    #[error("XMA2: {0}")]
    Unsupported(&'static str),
}

/// A decoded stream: each channel's samples (floats, FFmpeg's `fltp`), and the errors FFmpeg would have logged on
/// the way (damaged data; 0 for a sound that decodes cleanly).
#[derive(Debug, Clone, PartialEq)]
pub struct Decoded {
    /// Each channel's samples.
    pub planar: Vec<Vec<f32>>,
    /// The errors FFmpeg would have logged.
    pub errors: u32,
}

/// What [`parse_riff`] reads of an XMA2 RIFF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Riff<'a> {
    /// The channels (`nChannels`).
    pub channels: usize,
    /// The sample rate.
    pub rate: u32,
    /// The average bytes a second (`nAvgBytesPerSec`).
    pub byte_rate: u32,
    /// The XMA streams (`NumStreams`).
    pub streams: u16,
    /// The `data` chunk: the 2048-byte packets.
    pub data: &'a [u8],
}

fn le16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn le32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// The `fmt ` and `data` of a RIFF WAVE of XMA2 (as `bc2_formats::audioframe::xma2_riff` writes it).
///
/// # Errors
/// [`Error::Riff`] for anything else.
pub fn parse_riff(b: &[u8]) -> Result<Riff<'_>, Error> {
    if b.get(..4) != Some(b"RIFF") || b.get(8..12) != Some(b"WAVE") {
        return Err(Error::Riff("no RIFF WAVE header"));
    }
    let (mut at, mut fmt, mut data) = (12usize, None, None);
    while let (Some(tag), Some(len)) = (b.get(at..at + 4), le32(b, at + 4)) {
        let body = at + 8;
        let end = body.saturating_add(len as usize);
        if tag == b"fmt " {
            fmt = b.get(body..end.min(b.len()));
        } else if tag == b"data" {
            data = Some(&b[body.min(b.len())..end.min(b.len())]);
            break;
        }
        at = end + (len as usize & 1);
    }
    let f = fmt.ok_or(Error::Riff("no fmt chunk"))?;
    if le16(f, 0) != Some(0x166) {
        return Err(Error::Riff("not format 0x166"));
    }
    if le16(f, 16) != Some(34) {
        return Err(Error::Riff("not an XMA2WAVEFORMATEX (34 bytes of extra data)"));
    }
    Ok(Riff {
        channels: usize::from(le16(f, 2).ok_or(Error::Riff("short fmt"))?),
        rate: le32(f, 4).ok_or(Error::Riff("short fmt"))?,
        byte_rate: le32(f, 8).ok_or(Error::Riff("short fmt"))?,
        streams: le16(f, 18).ok_or(Error::Riff("short fmt"))?,
        data: data.ok_or(Error::Riff("no data chunk"))?,
    })
}

/// `ff_pcm_default_packet_size` for the demuxer's XMA2 packets (`block_align` 2048, the header's byte rate).
fn demux_packet_size(byte_rate: u32) -> usize {
    let block_align: i64 = PACKET as i64;
    let bitrate = i64::from(byte_rate) * 8;
    let max_samples = i64::from(i32::MAX) / block_align;
    let nb = if bitrate > 0 {
        let n = (bitrate / 8 / 10 / block_align).clamp(1, max_samples);
        1i64 << (63 - n.leading_zeros())
    } else {
        (4096 / block_align).clamp(1, max_samples)
    };
    (block_align * nb) as usize
}

/// The XMA wrapper's state (`XMADecodeCtx` for one stream) and what it gave.
struct Xma {
    s: Stream,
    fifo: Vec<Vec<f32>>,
    out: Vec<Vec<f32>>,
    flushed: bool,
    trim_start: u32,
    trim_end: u32,
    skip_samples: usize,
}

impl Xma {
    /// `xma_decode_packet` on what is left of a demuxer packet (empty: draining); its return value.
    fn call(&mut self, pkt: &[u8]) -> i32 {
        let mut got_stream = false;
        let mut ret = 0;
        if !self.s.eof_done {
            let c = self.s.decode_packet(pkt);
            ret = c.ret;
            got_stream = c.got;
        }
        let mut eof = false;
        if pkt.is_empty() {
            eof = self.s.eof_done;
        }
        if self.s.trim_start != 0 {
            self.trim_start = self.s.trim_start;
        }
        if self.s.trim_end != 0 {
            self.trim_end = self.s.trim_end;
        }
        if got_stream {
            for (q, f) in self.fifo.iter_mut().zip(&self.s.frame) {
                q.extend_from_slice(&f[..SAMPLES_PER_FRAME]);
            }
        } else if ret < 0 {
            return ret;
        }
        if self.s.packet_done || self.s.packet_loss {
            self.s.skip_packets = self.s.skip_packets.saturating_sub(1);
            let mut nb = self.fifo[0].len();
            if !eof && !pkt.is_empty() {
                nb -= nb.min(4096);
            }
            if (nb > 0 || eof || pkt.is_empty()) && !self.flushed {
                if eof {
                    let trim = i64::from(self.trim_end) + i64::from(self.trim_start) - 128 - 64;
                    nb -= trim.clamp(0, nb as i64) as usize;
                    self.flushed = true;
                }
                for (o, q) in self.out.iter_mut().zip(&mut self.fifo) {
                    let taken: Vec<f32> = q.drain(..nb).collect();
                    // decode.c's skip_samples: the first 64 samples of the decoder's output are dropped
                    let skip = self.skip_samples.min(taken.len());
                    o.extend_from_slice(&taken[skip..]);
                }
                self.skip_samples -= self.skip_samples.min(nb);
            }
        }
        ret
    }
}

/// Decodes one XMA2 stream (`data`: its 2048-byte packets) of `channels` (1 or 2) at `rate`, as `ffmpeg -f wav`
/// decodes the RIFF that holds it with that header (`byte_rate`: the header's average bytes a second, which sets the
/// demuxer's packet size, and so what a damaged packet takes with it).
///
/// # Errors
/// [`Error::Unsupported`] where FFmpeg's decoder does not open.
pub fn decode(data: &[u8], channels: usize, rate: u32, byte_rate: u32) -> Result<Decoded, Error> {
    // XMA2WAVEFORMATEX: one stream of the file's channels (2 when there are 2, else 1)
    let nb = if 2 > channels { 1 } else { 2 };
    if channels != nb {
        return Err(Error::Unsupported("one stream of 1 or 2 channels"));
    }
    let s = Stream::new(nb, rate).ok_or(Error::Unsupported("a stream the decoder does not open"))?;
    let mut x = Xma {
        s,
        fifo: vec![Vec::new(); nb],
        out: vec![Vec::new(); nb],
        flushed: false,
        trim_start: 0,
        trim_end: 0,
        skip_samples: 64,
    };
    let size = demux_packet_size(byte_rate);
    for chunk in data.chunks(size) {
        let mut pos = 0usize;
        let mut idle = 0;
        while pos < chunk.len() {
            let r = x.call(&chunk[pos..]);
            if r < 0 {
                break; // decode.c drops the rest of the packet
            }
            let used = r as usize;
            if used >= chunk.len() - pos {
                break;
            }
            if used == 0 {
                idle += 1;
                if idle > 4 * (chunk.len() / PACKET + 1) * 64 {
                    break;
                }
            }
            pos += used;
        }
    }
    // draining: until a call gives no frame
    x.call(&[]);
    x.call(&[]);
    Ok(Decoded {
        planar: x.out,
        errors: x.s.errors,
    })
}

/// Decodes an XMA2 RIFF as `ffmpeg -f wav -i -` does ([`parse_riff`], then [`decode`]).
///
/// # Errors
/// As [`parse_riff`] and [`decode`]; a RIFF of more than one stream is [`Error::Unsupported`].
pub fn decode_riff(riff: &[u8]) -> Result<Decoded, Error> {
    let r = parse_riff(riff)?;
    if r.streams != 1 {
        return Err(Error::Unsupported("more than one XMA stream"));
    }
    decode(r.data, r.channels, r.rate, r.byte_rate)
}

/// libswresample's float to signed 16-bit conversion, interleaved (`av_clip_int16(lrintf(x * 32768))`: round half
/// to even; a product out of the 32-bit range converts to the "integer indefinite" value, -32768, as x86 does).
#[must_use]
pub fn to_s16(planar: &[Vec<f32>]) -> Vec<i16> {
    let n = planar.iter().map(Vec::len).min().unwrap_or(0);
    let mut out = Vec::with_capacity(n * planar.len());
    for i in 0..n {
        for ch in planar {
            let v = ch[i] * 32768.0;
            let r = v.round_ties_even();
            let s = if r.is_nan() || !(-2_147_483_648.0..2_147_483_648.0).contains(&r) {
                i16::MIN
            } else {
                (r as i32).clamp(-32768, 32767) as i16
            };
            out.push(s);
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn packet_size_follows_the_pcm_rule() {
        assert_eq!(demux_packet_size(48000 * 2 * 2), 16384);
        assert_eq!(demux_packet_size(22050 * 2), 4096);
        assert_eq!(demux_packet_size(0), 4096);
        assert_eq!(demux_packet_size(8000 * 2), 2048);
    }

    #[test]
    fn s16_rounds_half_to_even_and_saturates() {
        let p = vec![vec![
            0.5 / 32768.0,
            1.5 / 32768.0,
            -0.5 / 32768.0,
            1.0,
            -1.0,
            2.0,
            70000.0,
            f32::NAN,
        ]];
        assert_eq!(to_s16(&p), vec![0, 2, 0, 32767, -32768, 32767, -32768, -32768]);
    }

    #[test]
    fn riff_is_read() {
        let mut b = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        let mut f = Vec::new();
        for v in [0x166u16, 2] {
            f.extend_from_slice(&v.to_le_bytes());
        }
        f.extend_from_slice(&44100u32.to_le_bytes());
        f.extend_from_slice(&(44100u32 * 4).to_le_bytes());
        for v in [2048u16, 16, 34, 1] {
            f.extend_from_slice(&v.to_le_bytes());
        }
        f.extend_from_slice(&[0; 32]);
        b.extend_from_slice(&(f.len() as u32).to_le_bytes());
        b.extend_from_slice(&f);
        b.extend_from_slice(b"data\x04\0\0\0abcd");
        let r = parse_riff(&b).unwrap();
        assert_eq!((r.channels, r.rate, r.streams, r.data), (2, 44100, 1, &b"abcd"[..]));
        assert!(matches!(decode(&[], 3, 44100, 0), Err(Error::Unsupported(_))));
        // no packets: the end-of-stream frame only, all of it trimmed by the 64 skipped samples or kept as zeros
        let d = decode(&[], 2, 44100, 44100 * 4).unwrap();
        assert!(d.planar.iter().all(|c| c.iter().all(|&v| v == 0.0)));
    }

    /// Random packets (and random packets behind valid-looking headers) decode without a panic or a hang, as FFmpeg
    /// decodes damaged streams: errors counted, samples given.
    #[test]
    fn random_packets_do_not_panic() {
        let mut seed = 0x1234_5678u32;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for round in 0..60 {
            let packets = 1 + (next() % 6) as usize;
            let mut data: Vec<u8> = (0..packets * PACKET).map(|_| next() as u8).collect();
            if round % 2 == 0 {
                for p in 0..packets {
                    // a frame count, the first frame at bit 32 + a little, no skip
                    let h = (1u32 << 26) | ((next() % 64) << 11);
                    data[p * PACKET..p * PACKET + 4].copy_from_slice(&h.to_be_bytes());
                }
            }
            let ch = 1 + (round % 2) as usize;
            let rate = [22050, 32000, 44100, 48000][(round % 4) as usize];
            let d = decode(&data, ch, rate, rate * ch as u32 * 2).unwrap();
            assert_eq!(d.planar.len(), ch);
            assert!(
                d.planar
                    .iter()
                    .all(|c| c.iter().all(|v| v.is_finite() || v.is_nan() || v.is_infinite()))
            );
        }
    }
}
