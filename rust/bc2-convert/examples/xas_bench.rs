//! Times the EA-XAS encoder on decoded sounds (rust/BENCHMARKS.md, "E3a"):
//!
//! ```text
//! cargo run --release -p bc2-convert --example xas_bench -- <corpus.bin> <corpus.idx>
//! ```
//!
//! `corpus.bin`: int16 little-endian samples, each sound's interleaved; `corpus.idx`: a line per sound, `offset
//! (in samples) frames channels`. Prints the best of three runs of: the library's encoder one sound at a time on
//! one thread, on every core (each sound's groups split), the batch (`encode_all`); and on one thread the search
//! without its two shortcuts (every candidate to the end, as the reference), each checked to give the same bytes.
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_precision_loss,
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    missing_docs
)]

use std::time::Instant;

use bc2_convert::audio::{self, COEFS, FRAME, FRAME_SAMPLES, need, rint_shift};

/// The reference's search, every candidate to the end (no skipped duplicates, no early stop).
fn straight_group(g: &[i64; 32]) -> (u16, u16, [i64; 30]) {
    let q0 = (((g[0] + 8) >> 4) << 4).clamp(-32768, 32752);
    let q1 = (((g[1] + 8) >> 4) << 4).clamp(-32768, 32752);
    let mut best_err = u64::MAX;
    let mut best = (0i64, 0i64, [0i64; 30]);
    for c in 0..4usize {
        let (c0, c1) = (COEFS[c], COEFS[c + 4]);
        let mut r = 0i64;
        for t in 0..30 {
            let p1 = if t == 0 { q1 } else { g[t + 1] };
            let p2 = match t {
                0 => q0,
                1 => q1,
                _ => g[t],
            };
            r = r.max((g[t + 2] * 256 - (p1 * c0 + p2 * c1)).abs());
        }
        let nd = need(r);
        for d in [-1i64, 0, 1] {
            let k = (20 - nd - d).clamp(0, 15);
            let s = (20 - k) as u32;
            let step = 1i64 << s;
            let (mut p2, mut p1) = (q0, q1);
            let mut err = 0u64;
            let mut nib = [0i64; 30];
            for t in 0..30 {
                let pred = p1 * c0 + p2 * c1;
                let v = rint_shift(g[t + 2] * 256 - pred, s).clamp(-8, 7);
                let x = ((v * step + pred + 128) >> 8).clamp(-32768, 32767);
                err += ((x - g[t + 2]) * (x - g[t + 2])) as u64;
                nib[t] = v;
                p2 = p1;
                p1 = x;
            }
            if err < best_err {
                best_err = err;
                best = (c as i64, k, nib);
            }
        }
    }
    let (c, k, nib) = best;
    (
        (((q0 & 0xFFF0) | c) & 0xFFFF) as u16,
        (((q1 & 0xFFF0) | k) & 0xFFFF) as u16,
        nib,
    )
}

/// `audio::encode` on one thread with a given group search.
/// A group search: the header's two words and the nibbles.
type Search = fn(&[i64; 32]) -> (u16, u16, [i64; 30]);

fn encode_with(pcm: &[i16], ch: usize, group: Search) -> Vec<u8> {
    let n = pcm.len() / ch;
    let frames = n.div_ceil(FRAME_SAMPLES);
    let mut out = vec![0u8; frames * ch * FRAME];
    for (u, frame) in out.chunks_mut(FRAME).enumerate() {
        let (f, c) = (u / ch, u % ch);
        for gi in 0..4 {
            let first = f * FRAME_SAMPLES + gi * 32;
            let g: [i64; 32] = std::array::from_fn(|j| pcm.get((first + j) * ch + c).map_or(0, |&v| i64::from(v)));
            let (lo, hi, nib) = group(&g);
            frame[gi * 4..gi * 4 + 4].copy_from_slice(&(u32::from(lo) | u32::from(hi) << 16).to_le_bytes());
            for r in 0..15 {
                frame[16 + r * 4 + gi] = ((nib[2 * r] & 15) << 4 | (nib[2 * r + 1] & 15)) as u8;
            }
        }
    }
    out
}

fn best<F: FnMut() -> Vec<Vec<u8>>>(mut f: F) -> (f64, Vec<Vec<u8>>) {
    let mut best = f64::MAX;
    let mut out = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        out = f();
        best = best.min(t.elapsed().as_secs_f64());
    }
    (best, out)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bin = std::fs::read(&args[1]).expect("corpus.bin");
    let samples: Vec<i16> = bin.as_chunks::<2>().0.iter().map(|p| i16::from_le_bytes(*p)).collect();
    let idx = std::fs::read_to_string(&args[2]).expect("corpus.idx");
    let sounds: Vec<(&[i16], usize)> = idx
        .lines()
        .map(|l| {
            let v: Vec<usize> = l.split_whitespace().map(|x| x.parse().unwrap()).collect();
            (&samples[v[0]..v[0] + v[1] * v[2]], v[2])
        })
        .collect();
    println!(
        "{} sounds, {} samples ({:.0} s of audio at 48 kHz)",
        sounds.len(),
        samples.len(),
        samples.len() as f64 / 48000.0
    );
    let one = rayon::ThreadPoolBuilder::new().num_threads(1).build().unwrap();
    let (t1, a) = best(|| one.install(|| sounds.iter().map(|&(p, c)| audio::encode(p, c).unwrap()).collect()));
    println!("encode, one thread:                {t1:.3} s");
    let (tn, b) = best(|| sounds.iter().map(|&(p, c)| audio::encode(p, c).unwrap()).collect());
    println!(
        "encode, {} threads, one at a time: {tn:.3} s",
        rayon::current_num_threads()
    );
    let (tb, c) = best(|| audio::encode_all(&sounds).into_iter().map(Result::unwrap).collect());
    println!(
        "encode_all, {} threads:            {tb:.3} s",
        rayon::current_num_threads()
    );
    let (ts, d) = best(|| sounds.iter().map(|&(p, c)| encode_with(p, c, straight_group)).collect());
    println!("the straight search, one thread:  {ts:.3} s");
    let (tl, e) = best(|| {
        sounds
            .iter()
            .map(|&(p, c)| encode_with(p, c, audio::encode_group))
            .collect()
    });
    println!("the library's search, one thread:  {tl:.3} s");
    assert!(a == b && b == c && c == d && d == e, "the encoders differ");
    println!("all the same bytes");
}
