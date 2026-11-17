//! [`Level::search`] over one sector window with arrays instead of maps (round E3c): `build_graph`'s group joining
//! runs dozens of searches over the same window, each a breadth-first walk over up to all of its surfaces. The
//! surfaces of the window's columns get dense local numbers (row by row they are contiguous ranges of the level's), so
//! the walk runs on local numbers: its visited marks and back links are arrays, the cell map of the window's surfaces
//! is an array ([`WindowSearch`] is the joining's [`CellMap`]), and every kept surface's neighbours on its own column
//! (which depend only on the surface, the direction and the rule) are found once, in parallel, as local numbers.
//!
//! The answers are [`Level::search`]'s: the same queue order, the same first target, the same path, the same
//! cells entered into the cell map.

#![allow(clippy::many_single_char_names, reason = "the reference's names")]

use std::collections::VecDeque;

use rayon::prelude::*;

use crate::{CellMap, FastMap, Level, NavError, Rule};

/// Directions in the search's order (di outer, dj inner, the centre left out).
const DIRS: [(i64, i64); 8] = [(-1, -1), (-1, 0), (-1, 1), (0, -1), (0, 1), (1, -1), (1, 0), (1, 1)];

/// The memo's marks: not looked up yet, no neighbour; a neighbour is its local number + 2.
const UNKNOWN: u32 = 0;
const NONE: u32 = 1;

/// A start's back link.
const START: u32 = u32::MAX;

/// No cell entered yet.
const UNSET: (i64, i64) = (i64::MIN, i64::MIN);

/// One sector window's surfaces, numbered, with the search's arrays and the cell map.
#[derive(Debug)]
pub struct WindowSearch<'a> {
    level: &'a Level,
    gi0: i64,
    gj0: i64,
    window: (i64, i64),
    rule: Rule,
    /// Per window row: (the row's first surface in the level, its end, its first local number).
    rows: Vec<(usize, usize, usize)>,
    /// Per local number: the surface.
    ks: Vec<usize>,
    /// Per local surface and direction: the neighbour on its own column (memo).
    memo: Vec<u32>,
    /// Per local surface: the search that last reached it.
    seen: Vec<u32>,
    /// Per local surface: the local surface it was reached from ([`START`]: a start).
    prev: Vec<u32>,
    stamp: u32,
    /// The memo was filled for every kept surface (on the first search).
    filled: bool,
    /// The cell map: per local surface its cell ([`UNSET`]: none) ...
    cell: Vec<(i64, i64)>,
    /// ... and the cells of surfaces outside the window.
    extra: FastMap<usize, (i64, i64)>,
}

impl<'a> WindowSearch<'a> {
    /// The window (start, stop) of the sector whose local sample (0, 0) is the level's (gi0, gj0); `None` when it
    /// holds more surfaces than the arrays can number.
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
        if n.checked_mul(8)? >= (u32::MAX - 2) as usize {
            return None;
        }
        let mut ks = vec![usize::MAX; n];
        for &(first, end, base) in &rows {
            for k in first..end {
                if let Some(v) = ks.get_mut(base + k - first) {
                    *v = k;
                }
            }
        }
        Some(WindowSearch {
            level,
            gi0,
            gj0,
            window,
            rule,
            rows,
            ks,
            memo: vec![UNKNOWN; n * 8],
            seen: vec![0; n],
            prev: vec![START; n],
            stamp: 0,
            filled: false,
            cell: vec![UNSET; n],
            extra: FastMap::default(),
        })
    }

    /// The number of surfaces in the window.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ks.len()
    }

    /// No surfaces in the window.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ks.is_empty()
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

    /// `cells[k] = cell` (a dict's assignment: the value replaced).
    pub fn insert(&mut self, k: usize, cell: (i64, i64)) {
        match self.local(k) {
            Some(l) => {
                if let Some(c) = self.cell.get_mut(l) {
                    *c = cell;
                }
            }
            None => {
                self.extra.insert(k, cell);
            }
        }
    }

    /// The neighbour (as a local number) of every kept surface on its own column, in every direction whose
    /// neighbour lies in the window: found at once, in parallel.
    fn prefill(&mut self) {
        self.filled = true;
        let (level, rule) = (self.level, self.rule);
        let me = &*self;
        let filled: Vec<[u32; 8]> = me
            .ks
            .par_iter()
            .map(|&k| {
                let mut slots = [UNKNOWN; 8];
                if !level.keep.get(k).copied().unwrap_or(false) {
                    return slots;
                }
                let c = level.col_of.get(k).copied().unwrap_or(0);
                let nx = level.nx.max(1);
                let (gi, gj) = ((c % nx) as i64, (c / nx) as i64);
                for (slot, &(di, dj)) in slots.iter_mut().zip(&DIRS) {
                    *slot = match level.neighbour(k, gi, gj, di, dj, None, rule) {
                        None => NONE,
                        // a neighbour outside the window: left for the search to compute (it never asks)
                        Some(n) => me.local(n).map_or(UNKNOWN, |l| l as u32 + 2),
                    };
                }
                slots
            })
            .collect();
        for (dst, src) in self.memo.as_chunks_mut::<8>().0.iter_mut().zip(&filled) {
            dst.copy_from_slice(src);
        }
    }

    /// The neighbour of local surface `lk` (surface k, cell (i, j)) in direction `d`, as a local number: `Ok(None)`
    /// for none, `Err(())` for one outside the window.
    fn step(&mut self, lk: usize, k: usize, (i, j): (i64, i64), d: usize) -> Result<Option<usize>, ()> {
        let (di, dj) = DIRS.get(d).copied().unwrap_or((0, 0));
        let (gi, gj) = (self.gi0 + i, self.gj0 + j);
        let nx = self.level.nx as i64;
        let own =
            (0..nx).contains(&gi) && gj >= 0 && self.level.col_of.get(k).is_some_and(|&c| c as i64 == gj * nx + gi);
        let slot = lk * 8 + d;
        if own {
            match self.memo.get(slot).copied() {
                Some(NONE) => return Ok(None),
                Some(v) if v != UNKNOWN => return Ok(Some(v as usize - 2)),
                _ => {}
            }
        }
        let Some(n) = self.level.neighbour(k, gi, gj, di, dj, None, self.rule) else {
            if own && let Some(m) = self.memo.get_mut(slot) {
                *m = NONE;
            }
            return Ok(None);
        };
        let ln = self.local(n).ok_or(())?;
        if own && let Some(m) = self.memo.get_mut(slot) {
            *m = ln as u32 + 2;
        }
        Ok(Some(ln))
    }

    /// [`Level::search`] over this window, with this window as the cell map, from `starts` to the first surface
    /// `targets` accepts (and, with `reached`, the local numbers of the surfaces it reached, starts not counted).
    /// `Ok(Err(()))` when a start lies outside the window (before anything changed): the caller then runs
    /// [`Level::search`] with this window as its cell map.
    ///
    /// # Errors
    /// [`NavError::Cell`] when a surface taken from the queue has no cell.
    pub fn search(
        &mut self,
        starts: &[usize],
        targets: &dyn Fn(usize) -> bool,
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
                *p = START;
            }
        }
        let mut queue: VecDeque<usize> = local.into();
        let w = self.window;
        let inside = |v: i64| w.0 <= v && v < w.1;
        while let Some(lk) = queue.pop_front() {
            let k = self.ks.get(lk).copied().unwrap_or(usize::MAX);
            let (i, j) = match self.cell.get(lk) {
                Some(&c) if c != UNSET => c,
                _ => return Err(NavError::Cell(k as i64)),
            };
            for (d, &(di, dj)) in DIRS.iter().enumerate() {
                if !inside(i + di) || !inside(j + dj) {
                    continue;
                }
                // (a neighbour outside the window cannot be: its column is the cell's, in the window)
                let Ok(found) = self.step(lk, k, (i, j), d) else {
                    return Ok(Err(()));
                };
                let Some(ln) = found else { continue };
                if self.seen.get(ln) == Some(&stamp) {
                    continue;
                }
                if let (Some(s), Some(p)) = (self.seen.get_mut(ln), self.prev.get_mut(ln)) {
                    *s = stamp;
                    *p = lk as u32;
                }
                if let Some(c) = self.cell.get_mut(ln)
                    && *c == UNSET
                {
                    *c = (i + di, j + dj);
                }
                if let Some(r) = reached.as_deref_mut() {
                    r.push(ln);
                }
                let n = self.ks.get(ln).copied().unwrap_or(usize::MAX);
                if targets(n) {
                    let mut path = vec![n];
                    let mut at = ln;
                    while let Some(&p) = self.prev.get(at) {
                        if p == START {
                            break;
                        }
                        at = p as usize;
                        path.push(self.ks.get(at).copied().unwrap_or(usize::MAX));
                    }
                    path.reverse();
                    return Ok(Ok(Some(path)));
                }
                queue.push_back(ln);
            }
        }
        Ok(Ok(None))
    }
}

impl CellMap for WindowSearch<'_> {
    fn get(&self, k: usize) -> Option<(i64, i64)> {
        match self.local(k) {
            Some(l) => self.cell.get(l).copied().filter(|&c| c != UNSET),
            None => self.extra.get(&k).copied(),
        }
    }

    fn set_default(&mut self, k: usize, cell: (i64, i64)) {
        match self.local(k) {
            Some(l) => {
                if let Some(c) = self.cell.get_mut(l)
                    && *c == UNSET
                {
                    *c = cell;
                }
            }
            None => {
                self.extra.entry(k).or_insert(cell);
            }
        }
    }
}
