//! Pillow 12.3's filled polygon (`ImageDraw.polygon(xy, fill=ink)` on an 8-bit picture), span for span: the vertices
//! truncated toward zero to `int` (`_imaging.c` `_draw_polygon`), the edge list (`Draw.c` `ImagingDrawPolygon`: a
//! horizontal run continuing another merged into it, the closing edge unless the last vertex is the first), and the
//! scan line filler (`polygon_generic`: the crossings in `float`, an edge's last row counted twice unless it is the
//! scan's last, the "discontiguous corners" fix-up, `ROUND_UP` / `ROUND_DOWN` of each span's ends - the positive side
//! in `float`, the negative in `double` as the C macros promote them) clipped as `hline8` clips.

/// C's `(int)` of a `double` as x86-64 converts it (`cvttsd2si`): toward zero, `INT_MIN` when out of range or NaN.
fn c_int(v: f64) -> i32 {
    if v.is_nan() || v <= -2_147_483_649.0 || v >= 2_147_483_648.0 {
        i32::MIN
    } else {
        v as i32
    }
}

/// `ROUND_UP(f)` of a `float`: `f >= 0 ? floor(f + 0.5F) : -floor(fabs(f) + 0.5F)`.
fn round_up(f: f32) -> i32 {
    if f64::from(f) >= 0.0 {
        c_int(f64::from(f + 0.5f32).floor())
    } else {
        c_int(-(f64::from(f).abs() + 0.5).floor())
    }
}

/// `ROUND_DOWN(f)` of a `float`: `f >= 0 ? ceil(f - 0.5F) : -ceil(fabs(f) - 0.5F)`.
fn round_down(f: f32) -> i32 {
    if f64::from(f) >= 0.0 {
        c_int(f64::from(f - 0.5f32).ceil())
    } else {
        c_int(-(f64::from(f).abs() - 0.5).ceil())
    }
}

/// Draw.c's `Edge`.
#[derive(Debug, Clone, Copy)]
struct Edge {
    x0: i32,
    y0: i32,
    xmin: i32,
    ymin: i32,
    xmax: i32,
    ymax: i32,
    dx: f32,
}

impl Edge {
    /// `add_edge`.
    fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        let dx = if y0 == y1 {
            0.0
        } else {
            x1.wrapping_sub(x0) as f32 / y1.wrapping_sub(y0) as f32
        };
        Self {
            x0,
            y0,
            xmin: x0.min(x1),
            ymin: y0.min(y1),
            xmax: x0.max(x1),
            ymax: y0.max(y1),
            dx,
        }
    }

    /// The edge's crossing of row `y` (`(y - y0) * dx + x0` in `float`).
    fn at(&self, y: i32) -> f32 {
        y.wrapping_sub(self.y0) as f32 * self.dx + self.x0 as f32
    }
}

/// Fills the polygon `xy` (at least 2 vertices; Pillow raises `TypeError` for fewer) on a `w x h` picture: `span(y,
/// x0, x1)` hears each run of pixels Pillow sets (inclusive, clipped to the picture), in Pillow's order.
///
/// # Errors
/// Fewer than 2 vertices ("coordinate list must contain at least 2 coordinates").
pub fn fill(xy: &[(f64, f64)], w: usize, h: usize, mut span: impl FnMut(usize, usize, usize)) -> Result<(), String> {
    if xy.len() < 2 {
        return Err("coordinate list must contain at least 2 coordinates".to_owned());
    }
    let xsize = i32::try_from(w).unwrap_or(i32::MAX);
    let ysize = i32::try_from(h).unwrap_or(i32::MAX);
    let mut hline = |x0: i32, y: i32, x1: i32| {
        if y < 0 || y >= ysize {
            return;
        }
        let x0 = if x0 < 0 {
            0
        } else if x0 >= xsize {
            return;
        } else {
            x0
        };
        if x1 < 0 {
            return;
        }
        let x1 = x1.min(xsize - 1);
        if x0 <= x1 {
            span(y as usize, x0 as usize, x1 as usize);
        }
    };
    let p: Vec<(i32, i32)> = xy.iter().map(|&(x, y)| (c_int(x), c_int(y))).collect();
    let count = p.len();
    let mut e: Vec<Edge> = Vec::with_capacity(count);
    let mut i = 0;
    while i < count - 1 {
        let (x0, y0) = p[i];
        let (x1, y1) = p[i + 1];
        if y0 == y1 && i != 0 && y0 == p[i - 1].1 {
            // a horizontal run right after another: merged into it when both run the same way
            let px = p[i - 1].0;
            if let Some(last) = e.last_mut() {
                if x1 > x0 && x0 > px {
                    last.xmax = x1;
                    i += 1;
                    continue;
                } else if x1 < x0 && x0 < px {
                    last.xmin = x1;
                    i += 1;
                    continue;
                }
            }
        }
        e.push(Edge::new(x0, y0, x1, y1));
        i += 1;
    }
    if p[i] != p[0] {
        e.push(Edge::new(p[i].0, p[i].1, p[0].0, p[0].1));
    }
    // polygon_generic
    let mut ymin = ysize - 1;
    let mut ymax = 0;
    let mut table: Vec<Edge> = Vec::with_capacity(e.len());
    for ed in &e {
        ymin = ymin.min(ed.ymin);
        ymax = ymax.max(ed.ymax);
        if ed.ymin == ed.ymax {
            hline(ed.xmin, ed.ymin, ed.xmax);
            continue;
        }
        table.push(*ed);
    }
    ymin = ymin.max(0);
    ymax = ymax.min(ysize);
    let mut xx: Vec<f32> = Vec::with_capacity(table.len() * 2);
    let mut y = ymin;
    while y <= ymax {
        xx.clear();
        for (i, cur) in table.iter().enumerate() {
            if y < cur.ymin || y > cur.ymax {
                continue;
            }
            let v = cur.at(y);
            xx.push(v);
            if y == cur.ymax && y < ymax {
                // "needed to draw consistent polygons"
                xx.push(v);
            } else if (y == cur.ymin || y == cur.ymax) && cur.dx != 0.0 {
                // "connect discontiguous corners"
                for o in &table[..i] {
                    if (y != o.ymin && y != o.ymax) || o.dx == 0.0 {
                        continue;
                    }
                    let last = xx[xx.len() - 1];
                    if last.round() == o.at(y).round() {
                        let off = if y == cur.ymax { -1 } else { 1 };
                        let yo = y.wrapping_add(off);
                        let adj = cur.at(yo);
                        if yo >= o.ymin && yo <= o.ymax {
                            let adj_o = o.at(yo);
                            let n = xx.len() - 1;
                            if last > adj + 1.0 && last > adj_o + 1.0 {
                                xx[n] = adj.max(adj_o).round() + 1.0;
                            } else if last < adj - 1.0 && last < adj_o - 1.0 {
                                xx[n] = adj.min(adj_o).round() - 1.0;
                            }
                            break;
                        }
                    }
                }
            }
        }
        xx.sort_by(f32::total_cmp);
        let mut k = 1;
        while k < xx.len() {
            hline(round_up(xx[k - 1]), y, round_down(xx[k]));
            k += 2;
        }
        y += 1;
    }
    Ok(())
}

/// `ImageDraw.Draw(img).polygon(xy, fill=ink)` on a `w x h` 8-bit (`'L'`) picture's pixels.
///
/// # Errors
/// See [`fill`].
pub fn draw(px: &mut [u8], w: usize, h: usize, xy: &[(f64, f64)], ink: u8) -> Result<(), String> {
    fill(xy, w, h, |y, x0, x1| {
        if let Some(row) = px.get_mut(y * w + x0..=y * w + x1) {
            row.fill(ink);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(w: usize, h: usize, polys: &[Vec<(f64, f64)>]) -> Vec<u8> {
        let mut px = vec![0u8; w * h];
        for (k, p) in polys.iter().enumerate() {
            let ink = u8::try_from(k % 250 + 1).unwrap_or(1);
            assert!(draw(&mut px, w, h, p, ink).is_ok());
        }
        px
    }

    #[test]
    fn rounding_is_pillows() {
        assert_eq!((round_up(2.5), round_up(-2.5), round_up(-0.0)), (3, -3, 0));
        assert_eq!((round_down(2.5), round_down(-2.5), round_down(2.4999)), (2, -2, 2));
        assert_eq!((c_int(-1.9), c_int(3e10), c_int(f64::NAN)), (-1, i32::MIN, i32::MIN));
    }

    /// 400 xorshift polygons (triangles, every fifth an n-gon, every seventh on half pixels, many off the picture) on a
    /// 50 x 40 picture: the SHA-1 Pillow 12.3's `ImageDraw.polygon` gives the same polygons.
    #[test]
    fn random_polygons_are_pillows() {
        let mut s: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        let mut polys = Vec::new();
        for k in 0..400u64 {
            let n = if k % 5 != 0 { 3 } else { 3 + next() % 5 };
            let mut xy = Vec::new();
            for _ in 0..n {
                let mut x = ((next() >> 40) as i64 - (1 << 23)) as f64 / f64::from(1 << 17) + 20.0;
                let mut y = ((next() >> 40) as i64 - (1 << 23)) as f64 / f64::from(1 << 17) + 15.0;
                if k % 7 == 0 {
                    x = (x * 2.0).round_ties_even() / 2.0;
                    y = (y * 2.0).round_ties_even() / 2.0;
                }
                xy.push((x, y));
            }
            polys.push(xy);
        }
        let mut px = vec![0u8; 50 * 40];
        for (k, p) in polys.iter().enumerate() {
            assert!(draw(&mut px, 50, 40, p, u8::try_from(k % 255 + 1).unwrap_or(0)).is_ok());
        }
        assert_eq!(
            sha1_smol::Sha1::from(&px).digest().to_string(),
            "bcc816ef6f4b477a5357b6a9396663cb87447922"
        );
    }

    #[test]
    fn a_square_fills_its_box() {
        let px = picture(8, 8, &[vec![(1.0, 1.0), (5.0, 1.0), (5.0, 4.0), (1.0, 4.0)]]);
        let filled: Vec<usize> = (0..64).filter(|&i| px[i] != 0).collect();
        let want: Vec<usize> = (1..=4).flat_map(|y| (1..=5).map(move |x| y * 8 + x)).collect();
        assert_eq!(filled, want);
        assert!(draw(&mut [0u8; 4], 2, 2, &[(0.0, 0.0)], 1).is_err());
    }
}
