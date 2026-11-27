//! The WW2 HUD pictures (round OR4): the weapon icons' silhouettes drawn from the loadout pictures and fitted into a
//! reclaimed atlas box (Project OPFOR's `weapon_icons.py`: `picture`, `silhouette`, `fit`), and BF1943's propeller
//! plane painted in a Vietnam minimap box's colours (`minimap_icons.py`: `box_colours`, `paint`).

use super::resample::{Filter, resize};
use super::{Pic, clip, rolled, to_u8};
use crate::{texcheck, texwrite};

/// The icons' fill at the silhouette's top and bottom (DICE's: about 252 to 198).
pub const TOP: f64 = 252.0;
/// See [`TOP`].
pub const BOTTOM: f64 = 198.0;
/// The icons' height at most.
pub const MAX_H: usize = 32;
/// DICE's icons are drawn taller than the weapons' proportions (the Garand 104 x 24).
pub const STRETCH: f64 = 1.3;
/// Coverage that still counts as solid: barrels and bayonets stay a pixel wide.
pub const THIN: f64 = 0.28;

/// A float grid (`(h, w)` `float64`).
#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    /// Columns.
    pub w: usize,
    /// Rows.
    pub h: usize,
    /// The values, row by row.
    pub v: Vec<f64>,
}

/// `picture(stem)` of a loadout picture's resource: (coverage 0..1, luminance 0..1), each `(h, w)` - the DXT5
/// level's alpha / 255 and the mean of its colour / 255.
///
/// # Errors
/// The resource has no level or the level does not decode.
pub fn picture(res: &[u8]) -> Result<(Grid, Grid), String> {
    let lv = texcheck::levels(res).ok_or("the picture's levels do not read")?;
    let l0 = lv.first().ok_or("the picture has no level")?;
    let (w, h) = (l0.2 as usize, l0.3 as usize);
    let d = res.get(l0.4.clone()).ok_or("the picture's level is cut short")?;
    let fmt = res
        .get(8..12)
        .and_then(|s| s.try_into().ok())
        .map_or(0, u32::from_le_bytes);
    let a = texwrite::alpha5(d, w, h).ok_or("the picture's alpha does not decode")?;
    let (_, _, rgb) = texcheck::decode(d, w as i64, h as i64, fmt).ok_or("the picture does not decode")?;
    let cover = a.iter().map(|&v| v as f64 / 255.0).collect();
    let lum = rgb
        .iter()
        .map(|p| {
            let mut s = -0.0f64;
            for c in p {
                s += f64::from(*c) / 255.0;
            }
            s / 3.0
        })
        .collect();
    Ok((Grid { w, h, v: cover }, Grid { w, h, v: lum }))
}

fn grid_to_l(g: &Grid, x0: usize, y0: usize, w: usize, h: usize) -> Pic {
    let mut out = Pic::new(w, h, 1);
    for y in 0..h {
        for x in 0..w {
            out.px[y * w + x] = to_u8(g.v[(y0 + y) * g.w + x0 + x] * 255.0);
        }
    }
    out
}

/// `silhouette(stem, width, max_h)` of a loadout picture's (coverage, luminance): a DICE-style icon - RGBA, at most
/// `width` x `max_h` - white with a soft grey gradient, darkened slightly where the picture is dark, a one-pixel dark
/// outline, drawn 1.3 x taller than the weapon's proportions, thin parts kept a pixel wide.
///
/// # Errors
/// Nothing of the picture is covered.
pub fn silhouette(cover: &Grid, lum: &Grid, width: usize, max_h: Option<usize>) -> Result<Pic, String> {
    let (mut x0, mut x1, mut y0, mut y1) = (usize::MAX, 0, usize::MAX, 0);
    for y in 0..cover.h {
        for x in 0..cover.w {
            if cover.v[y * cover.w + x] > 0.3 {
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
    }
    if x0 == usize::MAX {
        return Err("zero-size array to reduction operation minimum which has no identity".to_owned());
    }
    let (w0, h0) = (x1 + 1 - x0, y1 + 1 - y0);
    let mh = max_h.filter(|&m| m != 0).unwrap_or(MAX_H);
    let s = ((width as f64 - 2.0) / w0 as f64).min((mh as f64 - 2.0) / (h0 as f64 * STRETCH));
    let w = ((w0 as f64 * s).round_ties_even() as i64).max(1) as usize + 2;
    let h = ((h0 as f64 * s * STRETCH).round_ties_even() as i64).max(1) as usize + 2;
    let a8 = resize(&grid_to_l(cover, x0, y0, w0, h0), w - 2, h - 2, Filter::Lanczos);
    let l8 = resize(&grid_to_l(lum, x0, y0, w0, h0), w - 2, h - 2, Filter::Box);
    let mut cv = vec![0.0f64; w * h];
    let mut shade = vec![0.0f64; w * h];
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = (y - 1) * (w - 2) + x - 1;
            cv[y * w + x] = f64::from(a8.px[i]) / 255.0;
            shade[y * w + x] = f64::from(l8.px[i]) / 255.0;
        }
    }
    let rows: Vec<usize> = (0..h)
        .filter(|&y| cv[y * w..(y + 1) * w].iter().copied().fold(f64::NEG_INFINITY, f64::max) > THIN)
        .collect();
    let (Some(&top), Some(&bot)) = (rows.first(), rows.last()) else {
        return Err("zero-size array to reduction operation minimum which has no identity".to_owned());
    };
    let smax = shade.iter().copied().fold(f64::NEG_INFINITY, f64::max).max(1e-6);
    let span = ((bot - top) as i64).max(1) as f64;
    let solid: Vec<bool> = cv.iter().map(|&v| v > THIN).collect();
    let mut out = Pic::new(w, h, 4);
    for y in 0..h {
        let grad = TOP + (BOTTOM - TOP) * clip((y as f64 - top as f64) / span, 0.0, 1.0);
        for x in 0..w {
            let at = y * w + x;
            let fill = grad * (0.86 + 0.14 * clip(shade[at] / smax, 0.0, 1.0));
            let mut near = false;
            for dy in [-1isize, 0, 1] {
                for dx in [-1isize, 0, 1] {
                    near |= solid[rolled(h, w, y, x, dy, dx)];
                }
            }
            let (v, al) = if solid[at] {
                (fill, 255.0)
            } else if near {
                (0.0, 230.0)
            } else {
                (0.0, 0.0)
            };
            let v = to_u8(clip(v, 0.0, 255.0));
            out.px[at * 4..at * 4 + 4].copy_from_slice(&[v, v, v, to_u8(al)]);
        }
    }
    Ok(out)
}

/// `fit(pic, w, h)`: an RGBA picture in a `w x h` box - scaled down (Lanczos) to fit when larger, centred on a
/// transparent ground.
#[must_use]
pub fn fit(pic: &Pic, w: usize, h: usize) -> Pic {
    let (mut pw, mut ph) = (pic.w, pic.h);
    let mut p = pic.clone();
    if pw > w || ph > h {
        let s = (w as f64 / pw as f64).min(h as f64 / ph as f64);
        pw = ((pw as f64 * s) as usize).max(1);
        ph = ((ph as f64 * s) as usize).max(1);
        p = resize(pic, pw, ph, Filter::Lanczos);
    }
    let mut out = Pic::new(w, h, 4);
    let (y0, x0) = (
        (h as isize - ph as isize).div_euclid(2),
        (w as isize - pw as isize).div_euclid(2),
    );
    for y in 0..ph {
        for x in 0..pw {
            let (yy, xx) = (y0 + y as isize, x0 + x as isize);
            if yy >= 0 && xx >= 0 && (yy as usize) < h && (xx as usize) < w {
                let o = (yy as usize * w + xx as usize) * 4;
                let s = (y * pw + x) * 4;
                out.px[o..o + 4].copy_from_slice(&p.px[s..s + 4]);
            }
        }
    }
    out
}

/// `box_colours(box)`: (fill, outline) of a Vietnam icon box, in the atlas's byte order - the darkest opaque colour
/// (by the sum of its three channels, the first in sorted order), and of the opaque colours more than 60 brighter,
/// the most common (the first in sorted order on a tie). `None`: no opaque texel, or none brighter.
#[must_use]
pub fn box_colours(bx: &Pic) -> Option<([u8; 3], [u8; 3])> {
    let mut cols: std::collections::BTreeMap<[u8; 3], usize> = std::collections::BTreeMap::new();
    for p in bx.px.chunks_exact(4) {
        if p[3] == 255 {
            *cols.entry([p[0], p[1], p[2]]).or_default() += 1;
        }
    }
    let lum = |c: &[u8; 3]| c.iter().map(|&v| i64::from(v)).sum::<i64>();
    let mut outline: Option<([u8; 3], i64)> = None;
    for c in cols.keys() {
        if outline.is_none_or(|(_, l)| lum(c) < l) {
            outline = Some((*c, lum(c)));
        }
    }
    let (outline, lmin) = outline?;
    let mut fill: Option<([u8; 3], usize)> = None;
    for (c, &n) in &cols {
        if lum(c) > lmin + 60 && fill.is_none_or(|(_, m)| n > m) {
            fill = Some((*c, n));
        }
    }
    Some((fill?.0, outline))
}

/// `paint(box, plane)`: the plane in a box's style - BF1943's white to the box's fill, its outline to the box's
/// outline (by the plane's luminance between its darkest and brightest visible texels), its alpha kept; transparent
/// texels carry the outline colour (no light fringe when filtered).
///
/// # Errors
/// (Python's `SystemExit`) the plane is not the box's size; the box has no colours to take, or the plane no visible
/// texel.
pub fn paint(bx: &Pic, plane: &Pic) -> Result<Pic, String> {
    if (plane.w, plane.h, plane.ch) != (bx.w, bx.h, bx.ch) || bx.ch != 4 {
        return Err(format!("plane ({}, {}) vs box ({}, {})", plane.h, plane.w, bx.h, bx.w));
    }
    let (fill, outline) = box_colours(bx).ok_or("attempt to get argmin of an empty sequence")?;
    let lum: Vec<f64> = plane
        .px
        .chunks_exact(4)
        .map(|p| {
            let mut s = -0.0f64;
            for &c in &p[..3] {
                s += f64::from(c);
            }
            s / 3.0
        })
        .collect();
    let seen: Vec<f64> = lum
        .iter()
        .zip(plane.px.chunks_exact(4))
        .filter(|(_, p)| p[3] > 0)
        .map(|(l, _)| *l)
        .collect();
    if seen.is_empty() {
        return Err("zero-size array to reduction operation minimum which has no identity".to_owned());
    }
    let lo = seen.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = seen.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = if hi - lo < 1.0 { 1.0 } else { hi - lo };
    let mut out = Pic::new(bx.w, bx.h, 4);
    for (i, p) in plane.px.chunks_exact(4).enumerate() {
        let t = clip((lum[i] - lo) / span, 0.0, 1.0);
        let o = &mut out.px[i * 4..i * 4 + 4];
        for j in 0..3 {
            let v = f64::from(outline[j]) * (1.0 - t) + f64::from(fill[j]) * t;
            o[j] = to_u8(v.round_ties_even());
        }
        o[3] = p[3];
        if p[3] == 0 {
            o[..3].copy_from_slice(&outline);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bar_makes_an_outlined_icon() {
        let (w, h) = (64, 16);
        let mut cover = Grid {
            w,
            h,
            v: vec![0.0; w * h],
        };
        for y in 6..10 {
            for x in 4..60 {
                cover.v[y * w + x] = 1.0;
            }
        }
        let lum = Grid {
            w,
            h,
            v: vec![0.5; w * h],
        };
        let icon = silhouette(&cover, &lum, 40, None).ok();
        let icon = icon.as_ref();
        assert_eq!(icon.map(|p| p.w), Some(40));
        // the outline ring around a solid middle
        assert_eq!(icon.map(|p| p.px[3]), Some(230));
        let mid = icon.map(|p| p.px[((p.h / 2) * p.w + p.w / 2) * 4..((p.h / 2) * p.w + p.w / 2) * 4 + 4].to_vec());
        assert_eq!(mid.map(|m| m[3]), Some(255));
        let fitted = icon.map(|p| fit(p, 20, 8));
        assert_eq!(fitted.map(|p| (p.w, p.h)), Some((20, 8)));
    }

    #[test]
    fn a_box_takes_the_plane() {
        let mut bx = Pic::new(4, 1, 4);
        bx.px = vec![10, 10, 10, 255, 200, 100, 50, 255, 200, 100, 50, 255, 0, 0, 0, 0];
        assert_eq!(box_colours(&bx), Some(([200, 100, 50], [10, 10, 10])));
        let mut plane = Pic::new(4, 1, 4);
        plane.px = vec![255, 255, 255, 255, 0, 0, 0, 255, 128, 128, 128, 128, 9, 9, 9, 0];
        let out = paint(&bx, &plane).ok();
        assert_eq!(
            out.map(|p| p.px),
            Some(vec![
                200, 100, 50, 255, 10, 10, 10, 255, 105, 55, 30, 128, 10, 10, 10, 0
            ])
        );
    }
}
