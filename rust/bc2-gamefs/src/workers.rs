//! The suite's one worker knob (round PR1b-2, audit AU5 ALG-11 / PR0 item 11): `BFBC2_WORKERS` sizes the Rust core's
//! one pool - rayon's global pool, which every batch reader, the bundle reads, the mesh index and the editor's CPU
//! work run on - in `bc2_native`, `bfbc2.exe`, the map editor and `mapexport.exe`; the Python tools' workers without a
//! variable of their own take it too (`bfbc2core.workers`). Unset: `RAYON_NUM_THREADS`, else one per logical core.

/// The knob's variable.
pub const ENV: &str = "BFBC2_WORKERS";

/// The pool's size: [`ENV`] (a whole number, at least 1), else `RAYON_NUM_THREADS`, else the logical cores.
#[must_use]
pub fn count() -> usize {
    let var = |n: &str| {
        std::env::var(n)
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|&v| v > 0)
    };
    var(ENV)
        .or_else(|| var("RAYON_NUM_THREADS"))
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, std::num::NonZero::get))
}

/// Makes the process's pool with [`count`] workers (named `bc2-worker-<n>`). Once per process, before any parallel
/// work; a later call, or a pool already made, leaves the pool as it is.
pub fn init() {
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(count())
        .thread_name(|i| format!("bc2-worker-{i}"))
        .build_global();
}

#[cfg(test)]
mod tests {
    #[test]
    fn at_least_one() {
        assert!(super::count() >= 1);
        super::init();
        super::init();
        assert!(rayon::current_num_threads() >= 1);
    }
}
