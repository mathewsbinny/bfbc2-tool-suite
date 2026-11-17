//! [`Level::search`] over one sector window with arrays instead of maps (round E3c): `build_graph`'s group joining
//! runs dozens of searches over the same window, each a breadth-first walk over up to all of its surfaces. The
//! surfaces of the window's columns get dense local numbers (row by row they are contiguous ranges of the level's), so
//! the walk's visited marks and back links are arrays, and the neighbours found are kept for the later searches (on a
//! surface's own column a neighbour depends only on the surface, the direction and the rule).
//!
//! The answers are [`Level::search`]'s: the same queue order, the same first target, the same path, the same
//! cells entered into the cell map.

#![allow(clippy::many_single_char_names, reason = "the reference's names")]

use std::collections::VecDeque;

use rayon::prelude::*;

use crate::{CellMap, Level, NavError, Rule};

/// Directions in the search's order (di outer, dj inner, the centre left out) and their slot.
const DIRS: [(i64, i64); 8] = [(-1, -1), (-1, 0), (-1, 1), (0, -1), (0, 1), (1, -1), (1, 0), (1, 1)];

/// The memo's marks: not looked up yet, no neighbour; a neighbour is stored as its surface + 2.
const UNKNOWN: u32 = 0;
const NONE: u32 = 1;

/// One sector window's surfaces, numbered, with the search's arrays.
#[derive(Debug)]
pub struct WindowSearch<'a> {
    level: &'a Level,
    gi0: i64,
    gj0: i64,
    window: (i64, i64),
    rule: Rule,
    /// Per window row: (the row's first surface in the level, its end, its first local number).
    rows: Vec<(usize, usize, usize)>,
    /// Per local surface and direction: the neighbour found (memo).
    memo: Vec<u32>,
    /// Per local surface: the search that last reached it.
    seen: Vec<u32>,
    /// Per local surface: the surface it was reached from (`usize::MAX`: a start).
    prev: Vec<usize>,
    stamp: u32,
    /// The memo was filled for every kept surface (on the first search).
    filled: bool,
}

impl<'a> WindowSearch<'a> {
    /// The window (start, stop) of the sector whose local sample (0, 0) is the level's (gi0, gj0); `None` when it
    /// holds more surfaces than the memo can number.
    #[must_use]
    pub fn new(level: &'a Level, (gi0, gj0): (i64, i64), window: (i64, i64), rule: Rule) -> Option<Self> {
        let nx = level.nx as i64;
        let nz = level.nz as i64;
        let mut rows = Vec::new();
        let mut n = 0usize;
        for j in window.0..window.1 {
            let gj = gj0 + j;
            let (a, b) = ((gi0 + window.0).clamp(0, nx), (gi0 + window.1).clamp(0, nx));
            if !(0..nz).contains(&gj) || a >= b {
                rows.push((0, 0, n));
                continue;
            }
            let row = (gj * nx) as usize;
            let first = *level.start.get(row + a as usize)?;
            let end = *level.start.get(row + b as usize)?;
            rows.push((first, end, n));
            n += end - first;
        }
        if n.checked_mul(8)? >= u32::MAX as usize || level.len() + 2 >= u32::MAX as usize {
            return None;
        }
        Some(WindowSearch {
            level,
            gi0,
            gj0,
            window,
            rule,
            rows,
            memo: vec![UNKNOWN; n * 8],
            seen: vec![0; n],
            prev: vec![usize::MAX; n],
            stamp: 0,
            filled: false,
        })
    }

    /// The number of surfaces in the window.
    #[must_use]
    pub fn len(&self) -> usize {
        self.seen.len()
    }

    /// No surfaces in the window.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }

    /// The local number of surface k, if its column is in the window.
    #[must_use]
    pub fn local(&self, k: usize) -> Option<usize> {
        let c = *self.level.col_of.get(k)?;
        let nx = self.level.nx.max(1);
        let (gi, gj) = ((c % nx) as i64, (c / nx) as i64);
        let (i, j) = (gi - self.gi0, gj - self.gj0);
        if i < self.window.0 || i >= self.window.1 || j < self.window.0 || j >= self.window.1 {
            return None;
        }
        let &(first, end, base) = self.rows.get((j - self.window.0) as usize)?;
        (first..end).contains(&k).then(|| base + k - first)
    }

    /// The neighbour of k (local number `lk`, read from its cell (i, j)) in direction slot `d`.
    fn neighbour(&mut self, k: usize, lk: usize, (i, j): (i64, i64), d: usize) -> Option<usize> {
        let (di, dj) = DIRS.get(d).copied().unwrap_or((0, 0));
        let (gi, gj) = (self.gi0 + i, self.gj0 + j);
        let c = self.level.col_of.get(k).copied().unwrap_or(usize::MAX);
        let nx = self.level.nx.max(1);
        if (gi, gj) != ((c % nx) as i64, (c / nx) as i64) {
            // a cell that is not the surface's own column (a node carried from another pitch): no memo
            return self.level.neighbour(k, gi, gj, di, dj, None, self.rule);
        }
        let slot = lk * 8 + d;
        match self.memo.get(slot).copied() {
            Some(NONE) => None,
            Some(v) if v != UNKNOWN => Some(v as usize - 2),
            _ => {
                let n = self.level.neighbour(k, gi, gj, di, dj, None, self.rule);
                if let Some(m) = self.memo.get_mut(slot) {
                    *m = n.map_or(NONE, |n| n as u32 + 2);
                }
                n
            }
        }
    }

    /// Every kept surface's neighbours on its own column, all directions, found at once in parallel (what the
    /// searches would find one by one: the first searches of a sector walk most of it, the failed ones whole).
    fn prefill(&mut self) {
        self.filled = true;
        let mut ks = vec![usize::MAX; self.seen.len()];
        for &(first, end, base) in &self.rows {
            for k in first..end {
                if let Some(v) = ks.get_mut(base + k - first) {
                    *v = k;
                }
            }
        }
        let (level, rule) = (self.level, self.rule);
        let nx = level.nx.max(1);
        self.memo.par_chunks_mut(8).zip(ks.par_iter()).for_each(|(slots, &k)| {
            if !level.keep.get(k).copied().unwrap_or(false) {
                return;
            }
            let c = level.col_of.get(k).copied().unwrap_or(0);
            let (gi, gj) = ((c % nx) as i64, (c / nx) as i64);
            for (slot, &(di, dj)) in slots.iter_mut().zip(&DIRS) {
                let n = level.neighbour(k, gi, gj, di, dj, None, rule);
                *slot = n.map_or(NONE, |n| n as u32 + 2);
            }
        });
    }

    /// [`Level::search`] over this window from `starts` to the first surface `targets` accepts, entering the
    /// surfaces reached into `cells` (and, with `reached`, their local numbers, starts not counted). `Ok(None)` also when a start lies outside the window: the caller then runs
    /// [`Level::search`] (for starts in the window, `Err` / `Ok(Some(..))` / `Ok(None)` is its answer).
    ///
    /// # Errors
    /// [`NavError::Cell`] when a surface taken from the queue has no cell.
    pub fn search(
        &mut self,
        starts: &[usize],
        targets: &dyn Fn(usize) -> bool,
        cells: &mut dyn CellMap,
        mut reached: Option<&mut Vec<usize>>,
    ) -> Result<Result<Option<Vec<usize>>, ()>, NavError> {
        let mut local = Vec::with_capacity(starts.len());
        for &k in starts {
            match self.local(k) {
                Some(l) => local.push(l),
                None => return Ok(Err(())),
            }
        }
        if !self.filled {
            self.prefill();
        }
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.seen.iter_mut().for_each(|s| *s = 0);
            self.stamp = 1;
        }
        let stamp = self.stamp;
        for &l in &local {
            if let (Some(s), Some(p)) = (self.seen.get_mut(l), self.prev.get_mut(l)) {
                *s = stamp;
                *p = usize::MAX;
            }
        }
        let mut queue: VecDeque<(usize, usize)> = starts.iter().copied().zip(local).collect();
        let inside = |v: i64, w: (i64, i64)| w.0 <= v && v < w.1;
        while let Some((k, lk)) = queue.pop_front() {
            let (i, j) = cells.get(k).ok_or(NavError::Cell(k as i64))?;
            for (d, &(di, dj)) in DIRS.iter().enumerate() {
                if !inside(i + di, self.window) || !inside(j + dj, self.window) {
                    continue;
                }
                let Some(n) = self.neighbour(k, lk, (i, j), d) else {
                    continue;
                };
                // n is in a column of the window (its cell's column is); a surface the numbering does not hold
                // cannot be one (kept for safety: the generic search then answers)
                let Some(ln) = self.local(n) else {
                    return Ok(Err(()));
                };
                if self.seen.get(ln) == Some(&stamp) {
                    continue;
                }
                if let (Some(s), Some(p)) = (self.seen.get_mut(ln), self.prev.get_mut(ln)) {
                    *s = stamp;
                    *p = k;
                }
                cells.set_default(n, (i + di, j + dj));
                if let Some(r) = reached.as_deref_mut() {
                    r.push(ln);
                }
                if targets(n) {
                    let mut path = vec![n];
                    let mut at = ln;
                    while let Some(&p) = self.prev.get(at) {
                        if p == usize::MAX {
                            break;
                        }
                        path.push(p);
                        match self.local(p) {
                            Some(l) => at = l,
                            None => break,
                        }
                    }
                    path.reverse();
                    return Ok(Ok(Some(path)));
                }
                queue.push_back((n, ln));
            }
        }
        Ok(Ok(None))
    }
}
