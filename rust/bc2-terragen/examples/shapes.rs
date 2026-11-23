//! The generator's shape against DICE's biome medians (rust/BENCHMARKS.md TG1):
//! `cargo run --release -p bc2-terragen --example shapes -- [size] [seed] [biome]`.
#![allow(clippy::print_stdout, missing_docs)]

use std::time::Instant;

use bc2_terragen::{Biome, Settings, Terrain};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let size = args.first().and_then(|s| s.parse().ok()).unwrap_or(1024);
    let seed = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(7);
    let only: Option<Biome> = args.get(2).and_then(|s| s.parse().ok());
    for b in Biome::ALL.into_iter().filter(|b| only.is_none_or(|o| o == *b)) {
        let t0 = Instant::now();
        let mut st = Settings::new(b, size, seed);
        st.erosion = args.get(3).and_then(|s| s.parse().ok());
        let Ok(t) = Terrain::new(&st) else { continue };
        let gen_s = t0.elapsed().as_secs_f64();
        let s = t.shape();
        let g = b.targets();
        println!(
            "{b:9} {size} seed {seed}: {gen_s:.1} s | slope p10/50/90/99 {:.1} {:.1} {:.1} {:.1} (DICE {:?}) | hi {:.3} ({}) | beta {:.2} ({}) | r8 {:.2} r64 {:.2} (DICE r64 {}) | edge-centre {:.1} ({}) | wet {:.3} ({}) | range {}",
            s.slope[0],
            s.slope[1],
            s.slope[2],
            s.slope[3],
            g.slope,
            s.hi,
            g.hi,
            s.beta,
            g.beta,
            s.rough8,
            s.rough64,
            g.rough64,
            s.edge_minus_centre,
            g.edge_minus_centre,
            s.wet,
            g.wet,
            t.height.range_m
        );
    }
}
