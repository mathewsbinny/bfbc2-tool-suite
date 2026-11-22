//! Decodes every XMA2 RIFF in a pack file and compares the 16-bit samples with FFmpeg's, saved beside it.
//!
//! `cargo run --release -p bc2-xma --example xma_compare -- PACK REFDIR EXT [STEP]`
//!
//! PACK: records of `<u32 name length><name><u32 RIFF length><RIFF><u32 channels><u32 samples>` (little-endian);
//! REFDIR/<index>.<EXT>: FFmpeg's `-f s16le` output for record `index` (EXT `c16`: `-cpuflags 0`, `s16`: default).
//! Prints the streams equal sample for sample, the samples that differ and the largest difference.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::many_single_char_names,
    clippy::doc_markdown,
    reason = "a measuring tool over files it was handed"
)]

use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(pack), Some(refdir), Some(ext)) = (args.get(1), args.get(2), args.get(3)) else {
        eprintln!("usage: xma_compare PACK REFDIR EXT [STEP]");
        std::process::exit(2);
    };
    let step: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(1);
    let b = std::fs::read(pack).expect("pack");
    let mut at = 0usize;
    let rd = |at: &mut usize| {
        let v = u32::from_le_bytes(b[*at..*at + 4].try_into().unwrap());
        *at += 4;
        v as usize
    };
    let (mut streams, mut same, mut samples, mut differ, mut maxd, mut lens, mut errs) = (0, 0, 0u64, 0u64, 0i32, 0, 0);
    let mut index = 0usize;
    let t0 = std::time::Instant::now();
    let mut dtime = std::time::Duration::ZERO;
    while at < b.len() {
        let n = rd(&mut at);
        let name = String::from_utf8_lossy(&b[at..at + n]).to_string();
        at += n;
        let r = rd(&mut at);
        let riff = &b[at..at + r];
        at += r;
        let _ch = rd(&mut at);
        let _samples = rd(&mut at);
        let i = index;
        index += 1;
        if !i.is_multiple_of(step) {
            continue;
        }
        let refp = Path::new(refdir).join(format!("{i}.{ext}"));
        let Ok(want) = std::fs::read(&refp) else { continue };
        let want: Vec<i16> = want.as_chunks::<2>().0.iter().map(|p| i16::from_le_bytes(*p)).collect();
        let t = std::time::Instant::now();
        let got = match bc2_xma::decode_riff(riff) {
            Ok(d) => {
                errs += d.errors;
                bc2_xma::to_s16(&d.planar)
            }
            Err(e) => {
                println!("{i} {name}: {e}");
                continue;
            }
        };
        dtime += t.elapsed();
        streams += 1;
        if got.len() != want.len() {
            lens += 1;
            println!("{i} {name}: {} samples, ffmpeg {}", got.len(), want.len());
        }
        let mut d_here = 0u64;
        for (a, w) in got.iter().zip(&want) {
            let d = (i32::from(*a) - i32::from(*w)).abs();
            if d > 0 {
                d_here += 1;
                maxd = maxd.max(d);
            }
        }
        samples += want.len() as u64;
        differ += d_here;
        if d_here == 0 && got.len() == want.len() {
            same += 1;
        } else if d_here > 0 && std::env::var_os("XMA_VERBOSE").is_some() {
            println!("{i} {name}: {d_here} samples differ");
        }
    }
    println!(
        "{streams} streams: {same} identical, {lens} of another length; {differ} of {samples} samples differ \
         (largest {maxd}); {errs} decoder errors; decode {:.1} s of {:.1} s",
        dtime.as_secs_f64(),
        t0.elapsed().as_secs_f64()
    );
}
