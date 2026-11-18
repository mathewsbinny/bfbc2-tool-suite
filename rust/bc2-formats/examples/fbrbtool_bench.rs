//! Times one FbrbTool command line in process: `cargo run --release --example fbrbtool_bench -- <runs> <args...>`.
#![allow(clippy::print_stdout, clippy::unwrap_used, clippy::indexing_slicing, missing_docs)]

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let runs: usize = a[0].parse().unwrap();
    let args = a[1..].to_vec();
    let mut times = Vec::new();
    for _ in 0..runs {
        if args[0] == "unpack" {
            let _ = std::fs::remove_dir_all(&args[2]);
        }
        let t = std::time::Instant::now();
        let o = bc2_formats::fbrb::tool::run(&args, None);
        times.push(t.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(o.code, 0, "{o:?}");
    }
    times.sort_by(f64::total_cmp);
    println!("best {:.0} ms, median {:.0} ms", times[0], times[times.len() / 2]);
}
