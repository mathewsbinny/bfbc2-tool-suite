//! The batch functions' bounded in-flight work (round PR1b-1, memory): the inputs are decoded on all cores while
//! the calling thread turns the results into Python objects in input order, and a decoded result is dropped as soon
//! as its Python object exists. Only a window of results is decoded ahead of the one being built: at least
//! `min_items` items, more while their inputs stay under [`WINDOW_BYTES`] (as `weight` counts them), never more than
//! [`WINDOW_ITEMS`]. Before, every input's Rust result was alive at once beside the Python objects built from them,
//! and the building waited for the last decode. The results are the same objects in the same order (each item is
//! decoded alone, by the same function).

use std::collections::BTreeMap;
use std::sync::mpsc;
use std::sync::{Condvar, Mutex, PoisonError};

use pyo3::prelude::*;

/// The input bytes decoded ahead of the result being built (past `min_items` items).
pub const WINDOW_BYTES: usize = 64 << 20;

/// The most items decoded ahead of the result being built.
pub const WINDOW_ITEMS: usize = 4096;

/// What is decoded and not yet built: items and their input bytes.
#[derive(Default)]
struct Window {
    items: usize,
    bytes: usize,
    stop: bool,
}

/// The gate between the decoding and the building.
struct Gate {
    open: Mutex<Window>,
    freed: Condvar,
    min_items: usize,
}

impl Gate {
    /// Waits until an item of `w` bytes may start; false once the building stopped.
    fn enter(&self, w: usize) -> bool {
        let mut g = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if g.stop {
                return false;
            }
            let fits =
                g.items < self.min_items || (g.bytes.saturating_add(w) <= WINDOW_BYTES && g.items < WINDOW_ITEMS);
            if fits {
                g.items += 1;
                g.bytes = g.bytes.saturating_add(w);
                return true;
            }
            g = self.freed.wait(g).unwrap_or_else(PoisonError::into_inner);
        }
    }

    fn leave(&self, w: usize) {
        let mut g = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        g.items = g.items.saturating_sub(1);
        g.bytes = g.bytes.saturating_sub(w);
        self.freed.notify_all();
    }

    fn stop(&self) {
        self.open.lock().unwrap_or_else(PoisonError::into_inner).stop = true;
        self.freed.notify_all();
    }
}

/// `f` of each item on all cores (the interpreter released), `emit` given each result in input order with the
/// interpreter held while the next ones decode; at least two items per core in flight.
///
/// # Errors
/// The first error `emit` returns (nothing more is decoded).
pub fn map<T, R>(
    py: Python<'_>,
    items: &[T],
    weight: impl Fn(&T) -> usize + Sync,
    f: impl Fn(&T) -> R + Sync + Send,
    emit: impl FnMut(R) -> PyResult<()>,
) -> PyResult<()>
where
    T: Sync,
    R: Send,
{
    map_min(py, items, weight, 2 * rayon::current_num_threads(), f, emit)
}

/// [`map`] with at least `min_items` items in flight (one per core for inputs as large as a bundle).
///
/// # Errors
/// As [`map`].
pub fn map_min<T, R>(
    py: Python<'_>,
    items: &[T],
    weight: impl Fn(&T) -> usize + Sync,
    min_items: usize,
    f: impl Fn(&T) -> R + Sync + Send,
    mut emit: impl FnMut(R) -> PyResult<()>,
) -> PyResult<()>
where
    T: Sync,
    R: Send,
{
    let gate = Gate {
        open: Mutex::new(Window::default()),
        freed: Condvar::new(),
        min_items: min_items.max(1),
    };
    let (tx, rx) = mpsc::channel::<(usize, R)>();
    let rx = Mutex::new(rx); // `detach` wants what it borrows to be shareable
    std::thread::scope(|s| {
        let (gate, f, weight) = (&gate, &f, &weight);
        s.spawn(move || {
            rayon::scope(|sc| {
                for (i, it) in items.iter().enumerate() {
                    if !gate.enter(weight(it)) {
                        break;
                    }
                    let tx = tx.clone();
                    sc.spawn(move |_| {
                        // The receiver is gone only when the building stopped: the result is not wanted then.
                        let _ = tx.send((i, f(it)));
                    });
                }
            });
        });
        let mut early: BTreeMap<usize, R> = BTreeMap::new();
        let mut out = Ok(());
        for (next, it) in items.iter().enumerate() {
            let r = match early.remove(&next) {
                Some(r) => Some(r),
                None => loop {
                    match py.detach(|| rx.lock().unwrap_or_else(PoisonError::into_inner).recv()) {
                        Ok((i, r)) if i == next => break Some(r),
                        Ok((i, r)) => {
                            early.insert(i, r);
                        }
                        Err(_) => break None, // the decoding thread ended early (a panic, raised at the join)
                    }
                },
            };
            let Some(r) = r else { break };
            let w = weight(it);
            out = emit(r);
            gate.leave(w);
            if out.is_err() {
                break;
            }
        }
        gate.stop();
        drop(rx);
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_gate() {
        let g = Gate {
            open: Mutex::new(Window::default()),
            freed: Condvar::new(),
            min_items: 2,
        };
        assert!(g.enter(WINDOW_BYTES));
        assert!(g.enter(WINDOW_BYTES)); // under min_items: in whatever its size
        g.leave(WINDOW_BYTES);
        g.leave(WINDOW_BYTES);
        for _ in 0..WINDOW_ITEMS {
            assert!(g.enter(0));
        }
        g.stop();
        assert!(!g.enter(0)); // stopped: the full window does not block
    }
}
