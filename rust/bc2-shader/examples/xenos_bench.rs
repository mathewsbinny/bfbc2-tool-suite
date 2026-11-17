//! The translator alone, no Python (`rust/BENCHMARKS.md`, "E3f"): decodes and translates every program of a corpus
//! file - records of `u32` length (little-endian), a `u8` stage (1 pixel) and the blob - with the plain interface
//! (the program's own interpolators, `float4` inputs), one thread then all cores; best of 5.
//!
//! `cargo run --release -p bc2-shader --example xenos_bench -- <corpus.bin>`
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::cast_precision_loss
)]

use std::time::Instant;

use bc2_shader::hlsl::{self, Interface};
use bc2_shader::xenos::{self, Shader};
use rayon::prelude::*;

fn plain(sh: &Shader) -> Interface {
    let mut inter: Vec<i64> = sh
        .header
        .interpolators
        .iter()
        .map(|i| i64::from(i.usage_index))
        .collect();
    inter.sort_unstable();
    let mut iface = Interface::new(inter);
    iface.vs_inputs = sh
        .header
        .vertex_elements
        .iter()
        .map(|e| (i64::from(e.usage_index), "float4".into()))
        .collect();
    iface.vs_inputs.sort();
    iface.vs_inputs.dedup_by_key(|e| e.0);
    iface
}

fn best<T>(f: impl Fn() -> T) -> (f64, T) {
    let mut t = f64::INFINITY;
    let mut out = f();
    for _ in 0..5 {
        let a = Instant::now();
        out = f();
        t = t.min(a.elapsed().as_secs_f64());
    }
    (t, out)
}

fn main() {
    let path = std::env::args().nth(1).expect("a corpus file");
    let data = std::fs::read(path).expect("readable");
    let mut progs: Vec<(&[u8], bool)> = Vec::new();
    let mut o = 0;
    while o + 5 <= data.len() {
        let n = u32::from_le_bytes(data[o..o + 4].try_into().unwrap()) as usize;
        progs.push((&data[o + 5..o + 5 + n], data[o + 4] != 0));
        o += 5 + n;
    }
    let n = progs.len() as f64;
    let (td, shs) = best(|| {
        progs
            .iter()
            .map(|(b, p)| xenos::parse(b, Some(*p)).unwrap())
            .collect::<Vec<_>>()
    });
    let ifaces: Vec<Interface> = shs.iter().map(plain).collect();
    let (tt, texts) = best(|| {
        shs.iter()
            .zip(&ifaces)
            .map(|(s, i)| hlsl::translate(s, i))
            .collect::<Vec<_>>()
    });
    let (tdp, _) = best(|| {
        progs
            .par_iter()
            .map(|(b, p)| xenos::parse(b, Some(*p)).unwrap())
            .collect::<Vec<_>>()
    });
    let pairs: Vec<(&Shader, &Interface)> = shs.iter().zip(&ifaces).collect();
    let (ttp, _) = best(|| hlsl::translate_all(&pairs));
    let ok = texts.iter().filter(|t| t.is_ok()).count();
    let bytes: usize = texts.iter().flatten().map(String::len).sum();
    println!(
        "{} programs, {ok} translated ({:.1} MB of HLSL)",
        progs.len(),
        bytes as f64 / 1e6
    );
    println!("decode, one thread:    {:.3} s ({:.1} us each)", td, 1e6 * td / n);
    println!("translate, one thread: {:.3} s ({:.1} us each)", tt, 1e6 * tt / n);
    println!("decode, all cores:     {tdp:.3} s");
    println!("translate, all cores:  {ttp:.3} s");
}
