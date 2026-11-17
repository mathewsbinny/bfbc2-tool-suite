//! `navwalk.walkable`'s per-surface rules (`bfbc2-auto-navkit/tools/navwalk.py`), ported from the Python reference,
//! which stays the fallback:
//!
//! | Python (`navwalk.py`) | here |
//! |---|---|
//! | `headroom` | [`Grid::headroom`] |
//! | `stair_slope` | [`Grid::stair_slope`] |
//! | `narrow_lips` | [`Grid::narrow_lips`] |
//! | `walkable`'s landing loop (object surfaces near a stair surface) | [`Grid::landing`] |
//! | `near_objects` | [`Grid::near_objects`] |
//! | `on_objects` (per triangle, after the reference's bounding-box filter) | [`Grid::on_objects`] |
//!
//! The reference computes in `float64` (`s.y.astype(float)`, Python floats); the one `float32` comparison
//! (`narrow_lips`' flat test on the normals) is NumPy 2's: the Python float is cast to `float32`.
#![allow(clippy::many_single_char_names, reason = "the reference's names")]

use crate::{Floats, NavError, pymath};

/// A sample run (`navsample.Samples`): per column the range of its surfaces, per surface its height (`float64`), the
/// grid's pitch and origin.
#[derive(Debug, Clone)]
pub struct Grid {
    nx: usize,
    nz: usize,
    start: Vec<usize>,
    y: Vec<f64>,
    pitch: f64,
    min_x: f64,
    min_z: f64,
    col_of: Vec<usize>,
}

/// `stair_slope`'s answer: per surface the slope (degrees), the gradient along x, along z.
pub type Slopes = (Vec<f64>, Vec<f64>, Vec<f64>);

/// One triangle of an object, world space: three (x, y, z) corners.
pub type Tri = [[f64; 3]; 3];

/// `narrow_lips`' optional tests: `rise` (`LIP_OBJECT_RISE`) and `flat` (the normals' y and `cos(flat)`).
#[derive(Debug, Clone, Copy, Default)]
pub struct LipTests<'a> {
    /// The candidate within this height of both sides.
    pub rise: Option<f64>,
    /// Both sides' normals' y at least this (`cos(radians(flat))`): (per-surface normal y, the cosine).
    pub flat: Option<(&'a Floats, f64)>,
}

impl Grid {
    /// The grid of `nx` x `nz` columns with column starts `start` (`nx * nz + 1`, from 0 to the surfaces, ascending).
    ///
    /// # Errors
    /// [`NavError::Shape`] when the starts do not describe `y`.
    pub fn new(
        nx: usize,
        nz: usize,
        start: Vec<usize>,
        y: Vec<f64>,
        pitch: f64,
        min_x: f64,
        min_z: f64,
    ) -> Result<Grid, NavError> {
        let cols = nx
            .checked_mul(nz)
            .ok_or_else(|| NavError::Shape("grid too large".into()))?;
        if start.len() != cols + 1
            || start.first() != Some(&0)
            || start.last() != Some(&y.len())
            || start.windows(2).any(|w| w.first() > w.get(1))
        {
            return Err(NavError::Shape(format!(
                "{} column starts for {cols} columns and {} surfaces",
                start.len(),
                y.len()
            )));
        }
        let mut col_of = Vec::with_capacity(y.len());
        for (c, (a, b)) in start.iter().zip(start.iter().skip(1)).enumerate() {
            col_of.extend(std::iter::repeat_n(c, b - a));
        }
        Ok(Grid {
            nx,
            nz,
            start,
            y,
            pitch,
            min_x,
            min_z,
            col_of,
        })
    }

    /// The number of surfaces.
    #[must_use]
    pub fn len(&self) -> usize {
        self.y.len()
    }

    /// No surfaces.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.y.is_empty()
    }

    fn y(&self, k: usize) -> f64 {
        self.y.get(k).copied().unwrap_or(0.0)
    }

    fn check(&self, name: &str, len: usize) -> Result<(), NavError> {
        if len == self.len() {
            Ok(())
        } else {
            Err(NavError::Shape(format!(
                "{name}: {len} values for {} surfaces",
                self.len()
            )))
        }
    }

    /// The surfaces of column (i, j); none outside the grid.
    fn column(&self, i: i64, j: i64) -> std::ops::Range<usize> {
        match (usize::try_from(i), usize::try_from(j)) {
            (Ok(i), Ok(j)) if i < self.nx && j < self.nz => {
                let c = j * self.nx + i;
                self.start.get(c).copied().unwrap_or(0)..self.start.get(c + 1).copied().unwrap_or(0)
            }
            _ => 0..0,
        }
    }

    /// The column (i, j) of surface k.
    fn cell(&self, k: usize) -> (i64, i64) {
        let c = self.col_of.get(k).copied().unwrap_or(0);
        ((c % self.nx.max(1)) as i64, (c / self.nx.max(1)) as i64)
    }

    /// The surfaces flagged in `mask`, in order.
    fn flagged(mask: &[bool]) -> impl Iterator<Item = usize> + '_ {
        mask.iter().enumerate().filter_map(|(k, &m)| m.then_some(k))
    }

    /// `navwalk.headroom`: per surface the gap to the next surface above it (more than 0.05 m higher) in its
    /// column, infinite when none; surfaces flagged in `ignore` are not counted and keep infinity.
    ///
    /// # Errors
    /// [`NavError::Shape`] when `ignore` does not have one flag per surface.
    pub fn headroom(&self, ignore: Option<&[bool]>) -> Result<Vec<f64>, NavError> {
        if let Some(ig) = ignore {
            self.check("ignore", ig.len())?;
        }
        let ign = |k: usize| ignore.and_then(|ig| ig.get(k).copied()).unwrap_or(false);
        let mut out = vec![f64::INFINITY; self.len()];
        let mut ks: Vec<usize> = Vec::new();
        for w in self.start.windows(2) {
            let (a, b) = (w.first().copied().unwrap_or(0), w.get(1).copied().unwrap_or(0));
            if b - a < 2 {
                continue;
            }
            ks.clear();
            ks.extend((a..b).filter(|&k| !ign(k)));
            if ks.len() < 2 {
                continue;
            }
            for &k in &ks {
                let yk = self.y(k);
                let lim = yk + 0.05;
                // the first of the sorted heights above the limit: the least of them
                let higher = ks
                    .iter()
                    .map(|&n| self.y(n))
                    .filter(|&v| v > lim)
                    .fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.min(v))));
                if let (Some(h), Some(o)) = (higher, out.get_mut(k)) {
                    *o = h - yk;
                }
            }
        }
        Ok(out)
    }

    /// The height on column (i, j) nearest `yref` within `tol` (the first of equals), or `None`
    /// (`stair_slope`'s `layer`).
    fn layer(&self, i: i64, j: i64, yref: f64, tol: f64) -> Option<f64> {
        let mut best: Option<f64> = None;
        for k in self.column(i, j) {
            let d = (self.y(k) - yref).abs();
            if d <= tol && best.is_none_or(|b| d < (b - yref).abs()) {
                best = Some(self.y(k));
            }
        }
        best
    }

    /// `navwalk.stair_slope(s, mask, r, tol)`: for the surfaces in `mask`, the slope (degrees) and the gradients
    /// along x and z from the heights `r` samples either side on the same layer (within `tol`); 90 and 0, 0 where no
    /// neighbour is found (and for every other surface).
    ///
    /// # Errors
    /// [`NavError::Shape`] when `mask` does not have one flag per surface.
    pub fn stair_slope(&self, mask: &[bool], r: i64, tol: f64) -> Result<Slopes, NavError> {
        self.check("mask", mask.len())?;
        let n = self.len();
        let (mut out, mut gxo, mut gzo) = (vec![90.0; n], vec![0.0; n], vec![0.0; n]);
        let (two_r, r_f) = ((2 * r) as f64, r as f64);
        for k in Self::flagged(mask) {
            let (i, j) = self.cell(k);
            let yk = self.y(k);
            let (a, b, cc, d) = (
                self.layer(i + r, j, yk, tol),
                self.layer(i - r, j, yk, tol),
                self.layer(i, j + r, yk, tol),
                self.layer(i, j - r, yk, tol),
            );
            if a.is_none() && b.is_none() && cc.is_none() && d.is_none() {
                continue;
            }
            // central difference, or one-sided where a neighbour is missing (narrow stairs)
            let diff = |p: Option<f64>, m: Option<f64>| match (p, m) {
                (Some(p), Some(m)) => (p - m) / (two_r * self.pitch),
                (Some(p), None) => (p - yk) / (r_f * self.pitch),
                (None, Some(m)) => (yk - m) / (r_f * self.pitch),
                (None, None) => 0.0,
            };
            let (gx, gz) = (diff(a, b), diff(cc, d));
            if let (Some(o), Some(x), Some(z)) = (out.get_mut(k), gxo.get_mut(k), gzo.get_mut(k)) {
                (*o, *x, *z) = (pymath::slope_deg(gx, gz), gx, gz);
            }
        }
        Ok((out, gxo, gzo))
    }

    /// `narrow_lips`' `side`: walking from column (i, j) by (di, dj) up to `width` samples, on each column the
    /// surface nearest `yref` within 0.7 m per sample; the first that is in `base`, with its distance in samples.
    #[expect(clippy::too_many_arguments, reason = "the reference's arguments")]
    fn lip_side(&self, base: &[bool], i: i64, j: i64, yref: f64, di: i64, dj: i64, width: i64) -> Option<(usize, i64)> {
        for t in 1..=width {
            let (a, b) = (i + di * t, j + dj * t);
            if a < 0 || b < 0 || a as usize >= self.nx || b as usize >= self.nz {
                return None;
            }
            let lim = 0.7 * t as f64;
            let mut best: Option<usize> = None;
            for n in self.column(a, b) {
                let d = (self.y(n) - yref).abs();
                if d <= lim && best.is_none_or(|q| d < (self.y(q) - yref).abs()) {
                    best = Some(n);
                }
            }
            let best = best?;
            if base.get(best).copied().unwrap_or(false) {
                return Some((best, t));
            }
        }
        None
    }

    /// `navwalk.narrow_lips(s, base, cand, slope, width, rise, flat)` with `lim` = `tan(radians(slope))`: the `cand`
    /// surfaces with a `base` surface within `width` samples on both opposite sides (an axis or a diagonal) and a
    /// slope within the limit between those two; `tests` adds the rise and flat conditions.
    ///
    /// # Errors
    /// [`NavError::Shape`] when a flag or normal array does not have one value per surface.
    pub fn narrow_lips(
        &self,
        base: &[bool],
        cand: &[bool],
        lim: f64,
        width: i64,
        tests: LipTests<'_>,
    ) -> Result<Vec<bool>, NavError> {
        self.check("base", base.len())?;
        self.check("cand", cand.len())?;
        if let Some((ny, _)) = tests.flat {
            self.check("normal", ny.len())?;
        }
        let mut out = vec![false; self.len()];
        for k in Self::flagged(cand) {
            let (i, j) = self.cell(k);
            let yk = self.y(k);
            for (di, dj) in [(1i64, 0i64), (0, 1), (1, 1), (1, -1)] {
                let Some(p) = self.lip_side(base, i, j, yk, di, dj, width) else {
                    continue;
                };
                let Some(m) = self.lip_side(base, i, j, yk, -di, -dj, width) else {
                    continue;
                };
                let d = (p.1 + m.1) as f64 * self.pitch * if di != 0 && dj != 0 { 2f64.sqrt() } else { 1.0 };
                let (yp, ym) = (self.y(p.0), self.y(m.0));
                if let Some(rise) = tests.rise
                    && ((yk - yp).abs() > rise || (yk - ym).abs() > rise)
                {
                    continue;
                }
                if let Some((ny, c)) = tests.flat
                    && Self::below_flat(ny, p.0, m.0, c)
                {
                    continue;
                }
                if (yp - ym).abs() / d <= lim {
                    if let Some(o) = out.get_mut(k) {
                        *o = true;
                    }
                    break;
                }
            }
        }
        Ok(out)
    }

    /// `min(normal[p, 1], normal[m, 1]) < c`: Python's `min` (the first unless the second is less) on the normals'
    /// dtype, the Python float `c` cast to it (NumPy 2).
    fn below_flat(ny: &Floats, p: usize, m: usize, c: f64) -> bool {
        match ny {
            Floats::F32(v) => {
                let (a, b) = (v.get(p).copied().unwrap_or(0.0), v.get(m).copied().unwrap_or(0.0));
                (if b < a { b } else { a }) < c as f32
            }
            Floats::F64(v) => {
                let (a, b) = (v.get(p).copied().unwrap_or(0.0), v.get(m).copied().unwrap_or(0.0));
                (if b < a { b } else { a }) < c
            }
        }
    }

    /// `walkable`'s landings: the `cand` surfaces within two columns of a `stair` surface and within 0.9 m of its
    /// height.
    ///
    /// # Errors
    /// [`NavError::Shape`] when a flag array does not have one flag per surface.
    pub fn landing(&self, stair: &[bool], cand: &[bool]) -> Result<Vec<bool>, NavError> {
        self.check("stair", stair.len())?;
        self.check("cand", cand.len())?;
        let mut out = vec![false; self.len()];
        for k in Self::flagged(stair) {
            let (i, j) = self.cell(k);
            let yk = self.y(k);
            for dj in -2..=2 {
                for di in -2..=2 {
                    for n in self.column(i + di, j + dj) {
                        if cand.get(n).copied().unwrap_or(false)
                            && (self.y(n) - yk).abs() <= 0.9
                            && let Some(o) = out.get_mut(n)
                        {
                            *o = true;
                        }
                    }
                }
            }
        }
        Ok(out)
    }

    /// The columns `i0 ..= i1` (`max(0, ceil(lo))`, `min(n - 1, floor(hi))`) of a span `lo .. hi` in samples.
    ///
    /// # Errors
    /// [`NavError::Shape`] when an end is not finite (`math.ceil` / `math.floor` raise in the reference).
    fn span(lo: f64, hi: f64, n: usize) -> Result<std::ops::RangeInclusive<i64>, NavError> {
        if !(lo.is_finite() && hi.is_finite()) {
            return Err(NavError::Shape("a triangle's span is not finite".into()));
        }
        let a = lo.ceil().max(0.0);
        let b = hi.floor().min(n as f64 - 1.0);
        if a > b {
            #[expect(clippy::reversed_empty_ranges, reason = "an empty span")]
            return Ok(1..=0);
        }
        Ok((a as i64)..=(b as i64))
    }

    /// Python's `min` / `max` of three floats (the first unless a later one is less / greater).
    fn py_min3(a: f64, b: f64, c: f64) -> f64 {
        let m = if b < a { b } else { a };
        if c < m { c } else { m }
    }

    fn py_max3(a: f64, b: f64, c: f64) -> f64 {
        let m = if b > a { b } else { a };
        if c > m { c } else { m }
    }

    /// NumPy's `min` / `max` of three floats (NaN when any is).
    fn np_min3(a: f64, b: f64, c: f64) -> f64 {
        if a.is_nan() || b.is_nan() || c.is_nan() {
            return f64::NAN;
        }
        a.min(b).min(c)
    }

    fn np_max3(a: f64, b: f64, c: f64) -> f64 {
        if a.is_nan() || b.is_nan() || c.is_nan() {
            return f64::NAN;
        }
        a.max(b).max(c)
    }

    /// The reference raises (`math.ceil` of a NaN or an infinity) on a triangle with such an x or z.
    fn finite_xz(t: &Tri) -> Result<(), NavError> {
        if t.iter().all(|v| v[0].is_finite() && v[2].is_finite()) {
            Ok(())
        } else {
            Err(NavError::Shape("a triangle with a non-finite x or z".into()))
        }
    }

    /// `navwalk.near_objects(s, tris, grow, below, above)`: the surfaces within `grow` m (horizontally) of a
    /// triangle's bounding box and from `below` m under to `above` m over it.
    ///
    /// # Errors
    /// [`NavError::Shape`] on a triangle the reference would fail on (a non-finite x or z).
    pub fn near_objects(&self, tris: &[Tri], grow: f64, below: f64, above: f64) -> Result<Vec<bool>, NavError> {
        let mut out = vec![false; self.len()];
        for t in tris {
            Self::finite_xz(t)?;
            let [a, b, c] = t;
            let (x0, x1) = (
                Self::np_min3(a[0], b[0], c[0]) - grow,
                Self::np_max3(a[0], b[0], c[0]) + grow,
            );
            let (z0, z1) = (
                Self::np_min3(a[2], b[2], c[2]) - grow,
                Self::np_max3(a[2], b[2], c[2]) + grow,
            );
            let is = Self::span((x0 - self.min_x) / self.pitch, (x1 - self.min_x) / self.pitch, self.nx)?;
            let js = Self::span((z0 - self.min_z) / self.pitch, (z1 - self.min_z) / self.pitch, self.nz)?;
            let (y0, y1) = (
                Self::np_min3(a[1], b[1], c[1]) - below,
                Self::np_max3(a[1], b[1], c[1]) + above,
            );
            for j in js {
                for i in is.clone() {
                    for k in self.column(i, j) {
                        let yk = self.y(k);
                        if y0 <= yk
                            && yk <= y1
                            && let Some(o) = out.get_mut(k)
                        {
                            *o = true;
                        }
                    }
                }
            }
        }
        Ok(out)
    }

    /// `navwalk.on_objects(s, tris, tol)` after its bounding-box filter (`tris` = the triangles it keeps, in
    /// order): the surfaces within `tol` m (vertically) of a triangle over their column.
    ///
    /// # Errors
    /// [`NavError::Shape`] on a triangle the reference would fail on (a non-finite x or z).
    pub fn on_objects(&self, tris: &[Tri], tol: f64) -> Result<Vec<bool>, NavError> {
        let mut out = vec![false; self.len()];
        for t in tris {
            Self::finite_xz(t)?;
            let [[ax, ay, az], [bx, by, bz], [cx, cy, cz]] = *t;
            let det = (bz - cz) * (ax - cx) + (cx - bx) * (az - cz);
            if det.abs() < 1e-9 {
                continue;
            }
            let is = Self::span(
                (Self::py_min3(ax, bx, cx) - self.min_x) / self.pitch,
                (Self::py_max3(ax, bx, cx) - self.min_x) / self.pitch,
                self.nx,
            )?;
            let js = Self::span(
                (Self::py_min3(az, bz, cz) - self.min_z) / self.pitch,
                (Self::py_max3(az, bz, cz) - self.min_z) / self.pitch,
                self.nz,
            )?;
            for j in js {
                let pz = self.min_z + j as f64 * self.pitch;
                for i in is.clone() {
                    let px = self.min_x + i as f64 * self.pitch;
                    let l1 = ((bz - cz) * (px - cx) + (cx - bx) * (pz - cz)) / det;
                    let l2 = ((cz - az) * (px - cx) + (ax - cx) * (pz - cz)) / det;
                    if l1 < -1e-6 || l2 < -1e-6 || 1.0 - l1 - l2 < -1e-6 {
                        continue;
                    }
                    let yt = l1 * ay + l2 * by + (1.0 - l1 - l2) * cy;
                    for k in self.column(i, j) {
                        if (self.y(k) - yt).abs() <= tol
                            && let Some(o) = out.get_mut(k)
                        {
                            *o = true;
                        }
                    }
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
#[path = "rules_tests.rs"]
mod tests;
