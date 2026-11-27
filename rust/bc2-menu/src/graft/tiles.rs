//! `aptgraft.py`'s `split_image_part` (round OR14: `add_crosshair`'s `images` tiles, Project OPFOR's
//! `ww2\ww2_crosshairs.py`), step for step: a textured part re-pointed at tiles of its image.

#![allow(
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    reason = "the reference's names; each index is checked just before it, as the reference indexes"
)]

use crate::error::{MenuError, MenuResult};
use bc2_formats::aptmodel::{Rid, Val};

use super::{GEO, Movie, apt_err, f32_bits, fl};

/// A tile of a source image: its pixels (x, y, w, h) drawn from target image `image` at (dx, dy) (`aptgraft.Tile`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tile {
    /// x of the source image's pixels.
    pub x: f64,
    /// y.
    pub y: f64,
    /// Width.
    pub w: f64,
    /// Height.
    pub h: f64,
    /// The target image character.
    pub image: i64,
    /// x in the target image.
    pub dx: f64,
    /// y in the target image.
    pub dy: f64,
}

/// Python's `min(iterable)` of floats: the first, replaced by each later one that is smaller (NaN as Python's
/// comparisons leave it).
fn py_min(v: impl IntoIterator<Item = f64>) -> f64 {
    let mut it = v.into_iter();
    let mut cur = it.next().unwrap_or(f64::NAN);
    for x in it {
        if x < cur {
            cur = x;
        }
    }
    cur
}

/// Python's `max(iterable)`: the first, replaced by each later one that is larger.
fn py_max(v: impl IntoIterator<Item = f64>) -> f64 {
    let mut it = v.into_iter();
    let mut cur = it.next().unwrap_or(f64::NAN);
    for x in it {
        if x > cur {
            cur = x;
        }
    }
    cur
}

/// A field as `_fl` reads it (a pointer: the reference's TypeError).
fn flv(v: Val) -> MenuResult<f64> {
    match v {
        Val::Num(x) => Ok(fl(x)),
        Val::Ref(_) => Err(MenuError::Reference),
    }
}

/// One new part: its fields (field 2, the vertex list, is `Val::Num(0)` here: the caller points it at the vertices)
/// and its vertex fields.
pub type Piece = (Vec<Val>, Vec<u32>);

/// `split_image_part` of a part's fields and its vertex fields: the new parts' fields and vertices, one per tile the
/// rectangle overlaps.
pub fn split_values(v: &[Val], verts_raw: &[Val], tiles: &[Tile]) -> MenuResult<Vec<Piece>> {
    let mut verts = Vec::with_capacity(verts_raw.len());
    for &x in verts_raw {
        verts.push(flv(x)?);
    }
    let field = |k: usize| -> MenuResult<f64> { flv(*v.get(k).ok_or(MenuError::Reference)?) };
    let (a, b, c, d, tx, ty) = (field(4)?, field(5)?, field(8)?, field(9)?, field(16)?, field(17)?);
    let det = a * d - b * c;
    let count = *v.get(1).ok_or(MenuError::Reference)?;
    if count != Val::Num(2) || verts.len() != 12 || det.abs() < 1e-9 || (a * b).abs() + (c * d).abs() > 1e-6 {
        return Err(apt_err(
            "a textured part that is not one axis-aligned rectangle cannot be split into tiles",
        ));
    }
    let px: Vec<(f64, f64)> = (0..12)
        .step_by(2)
        .map(|k| {
            (
                a * verts[k] + c * verts[k + 1] + tx,
                b * verts[k] + d * verts[k + 1] + ty,
            )
        })
        .collect();
    let (x0, x1) = (py_min(px.iter().map(|p| p.0)), py_max(px.iter().map(|p| p.0)));
    let (y0, y1) = (py_min(px.iter().map(|p| p.1)), py_max(px.iter().map(|p| p.1)));
    let eps = 1e-3 * py_max([1.0, x1 - x0, y1 - y0]);
    let mut corner = Vec::with_capacity(6);
    for &(qx, qy) in &px {
        let cx = if (qx - x0).abs() < eps {
            Some(false)
        } else if (qx - x1).abs() < eps {
            Some(true)
        } else {
            None
        };
        let cy = if (qy - y0).abs() < eps {
            Some(false)
        } else if (qy - y1).abs() < eps {
            Some(true)
        } else {
            None
        };
        let (Some(cx), Some(cy)) = (cx, cy) else {
            return Err(apt_err(
                "a textured part whose triangles are not a rectangle in its image cannot be split",
            ));
        };
        corner.push((cx, cy));
    }
    let mut out = Vec::new();
    for t in tiles {
        // max(x0, sx): sx where it is larger, else x0 (Python's max of two)
        let mx = |p: f64, q: f64| if q > p { q } else { p };
        let mn = |p: f64, q: f64| if q < p { q } else { p };
        let r = (mx(x0, t.x), mx(y0, t.y), mn(x1, t.x + t.w), mn(y1, t.y + t.h));
        if r.2 - r.0 <= eps || r.3 - r.1 <= eps {
            continue;
        }
        let mut nv = Vec::with_capacity(12);
        for &(cx, cy) in &corner {
            let (qx, qy) = (if cx { r.2 } else { r.0 }, if cy { r.3 } else { r.1 });
            let (qx, qy) = (qx - tx, qy - ty); // pixel -> vertex: the inverse of the 2 x 2 matrix
            nv.push(f32_bits((d * qx - c * qy) / det)?);
            nv.push(f32_bits((-b * qx + a * qy) / det)?);
        }
        let mut m: Vec<Val> = v[4..v.len().min(20)].to_vec();
        m[12] = Val::Num(f32_bits(tx + t.dx - t.x)?);
        m[13] = Val::Num(f32_bits(ty + t.dy - t.y)?);
        let mut f = vec![v[0], Val::Num(2), Val::Num(0), v[3]];
        f.extend(m);
        f.extend([Val::Num(t.image as u32), Val::Num(0)]);
        out.push((f, nv));
    }
    Ok(out)
}

/// `split_image_part(part, tiles)` of a movie's part node: the new part nodes (the part's tag, `.geo`).
pub fn split_image_part(m: &mut Movie, part: Rid, tiles: &[Tile]) -> MenuResult<Vec<Rid>> {
    let v = m.vals(part)?.clone();
    let Val::Ref(vn) = *v.get(2).ok_or(MenuError::Reference)? else {
        return Err(MenuError::Reference);
    };
    let verts = m.vals(vn)?.clone();
    let tag = m.tag(part)?.to_owned();
    let mut out = Vec::new();
    for (mut f, nv) in split_values(&v, &verts, tiles)? {
        let vw = m.new_words(GEO, nv.into_iter().map(Val::Num).collect(), "geo.vertices");
        f[2] = Val::Ref(vw);
        out.push(m.new_words(GEO, f, &tag));
    }
    Ok(out)
}
