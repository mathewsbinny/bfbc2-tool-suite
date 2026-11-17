//! `navbuild.build_graph`'s cell picking: an area's centre node and its portals' nodes go on the area's cell nearest
//! a point, chosen by Python's `min` over the cells (the first of equals) with the reference's keys, computed as
//! Python computes them (`x ** 2` as [`pymath::sq`]).

use crate::pymath::sq;

/// The largest integer a float holds exactly: the reference's int arithmetic is exact, the port's only up to here.
const EXACT: i64 = 1 << 53;

/// The index of the least key, the first of equals (Python's `min(..., key=...)`); `None` for no keys.
fn argmin(keys: impl Iterator<Item = f64>) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    for (n, k) in keys.enumerate() {
        if best.is_none_or(|(_, b)| k < b) {
            best = Some((n, k));
        }
    }
    best.map(|b| b.0)
}

fn exact(v: i64) -> bool {
    (-EXACT..=EXACT).contains(&v)
}

/// The centre cell of an area (`build_graph`: `min(cells, key=(i - ci) ** 2 + (j - cj) ** 2)` with (ci, cj) the
/// cells' mean): its index in `cells` ((i, j) each), or `None` for no cells or values a float cannot hold exactly.
#[must_use]
pub fn centre_cell(cells: &[(i64, i64)]) -> Option<usize> {
    if cells.iter().any(|&(i, j)| !exact(i) || !exact(j)) {
        return None;
    }
    let (si, sj) = cells
        .iter()
        .fold((0i128, 0i128), |(a, b), &(i, j)| (a + i128::from(i), b + i128::from(j)));
    let (si, sj) = (i64::try_from(si).ok()?, i64::try_from(sj).ok()?);
    if !exact(si) || !exact(sj) {
        return None;
    }
    // int / int: the exact quotient rounded once, as f64 division of the exact operands gives it
    let n = cells.len() as f64;
    let (ci, cj) = (si as f64 / n, sj as f64 / n);
    argmin(cells.iter().map(|&(i, j)| sq(i as f64 - ci) + sq(j as f64 - cj)))
}

/// For each point (mx, mz), the cell of an area nearest it (`build_graph`'s portal nodes:
/// `min(cells, key=(ox + i * pitch - mx) ** 2 + (oz + j * pitch - mz) ** 2)`): its index in `cells`; `None` for no
/// cells or cell indices a float cannot hold exactly.
#[must_use]
pub fn nearest_cells(cells: &[(i64, i64)], ox: f64, oz: f64, pitch: f64, points: &[(f64, f64)]) -> Option<Vec<usize>> {
    if cells.iter().any(|&(i, j)| !exact(i) || !exact(j)) {
        return None;
    }
    let at: Vec<(f64, f64)> = cells
        .iter()
        .map(|&(i, j)| (ox + i as f64 * pitch, oz + j as f64 * pitch))
        .collect();
    points
        .iter()
        .map(|&(mx, mz)| argmin(at.iter().map(|&(x, z)| sq(x - mx) + sq(z - mz))))
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn centres() {
        assert_eq!(centre_cell(&[]), None);
        assert_eq!(centre_cell(&[(0, 0), (1, 0), (2, 0)]), Some(1));
        // equal keys: the first
        assert_eq!(centre_cell(&[(0, 0), (1, 0), (0, 1), (1, 1)]), Some(0));
        assert_eq!(centre_cell(&[(5, 5), (-1, 3), (2, 2), (2, 3)]), Some(3));
        assert_eq!(centre_cell(&[(EXACT + 1, 0)]), None);
        assert_eq!(centre_cell(&[(EXACT, 0), (EXACT, 0)]), None, "a sum past 2 ** 53");
    }

    #[test]
    fn nearest() {
        let cells = [(0, 0), (1, 0), (2, 0), (2, 1)];
        assert_eq!(
            nearest_cells(
                &cells,
                100.0,
                200.0,
                0.8,
                &[(100.0, 200.0), (101.7, 200.9), (100.3, 200.0)]
            ),
            Some(vec![0, 3, 0])
        );
        assert_eq!(nearest_cells(&[], 0.0, 0.0, 0.8, &[(0.0, 0.0)]), None);
        assert_eq!(nearest_cells(&[], 0.0, 0.0, 0.8, &[]), Some(vec![]));
        assert_eq!(nearest_cells(&[(0, -EXACT - 1)], 0.0, 0.0, 0.8, &[]), None);
        // a NaN key first stays (nothing is less than it)
        assert_eq!(argmin([f64::NAN, 1.0, 0.0].into_iter()), Some(0));
        assert_eq!(argmin([2.0, f64::NAN, 1.0].into_iter()), Some(2));
    }
}
