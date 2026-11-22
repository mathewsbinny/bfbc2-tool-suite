//! The caches' overall budget as `bfbc2core.fbrb._budget` runs it after a bundle read (round D1's
//! `caches.maybe_prune_total`, round O3): every suite cache and project cache together brought under
//! `BFBC2_CACHE_TOTAL_GB` (default 20 GiB), the least recently used entries of any cache deleted first, at most once
//! per 15 minutes across processes (the cache root's `.pruned-total` stamp), under the root's `.prune-total.lock`.
//! A Python tool that reads a bundle runs it once (its first read); `bfbc2.exe`'s ports call [`after_bundle_read`]
//! where the Python's `fbrb.read` / `read_many` would. Never fails (a cache that cannot be pruned is only bigger).
//!
//! The cache table and the pruning are [`bc2_gamefs::budget`]'s (round PR1b-2: one implementation, for this command
//! line and `bfbc2core.caches` through `bc2_native`).

pub use bc2_gamefs::budget::{MB, PRUNE_EVERY, SPECS, Spec, TOTAL_ENV, folder};

/// `fbrb._budget()` after a bundle read in a ported tool's process: `caches.maybe_prune_total()` unless the root's
/// stamp says it ran in the last 15 minutes. The Python asks once per process; so does this (a second call does
/// nothing).
pub fn after_bundle_read() {
    static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let opfor = bc2_menu::common::find_opfor(None).ok();
    let _ = bc2_gamefs::budget::maybe_prune_total(opfor.as_deref());
}
