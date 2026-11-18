//! The PC texture reader of the converter's checks: a PC `ITexture`'s levels and one level decoded to pixels, a port
//! of `bfbc2-console-convert/texture.py`'s `levels` and `decode` (round F0; their NumPy is the reference, kept).
//!
//! [`decode`] gives the reference's array as rows of `(r, g, b)` `i32`s: DXT1 (formats 0, 18), DXT3 / DXT5 (1, 2:
//! the colour block, the alpha dropped), BC5 (19, 20: red and green from the two alpha-style blocks, blue 0) and the
//! 8-bit format 10 (the value in all three; rows as stored, a padded pitch allowed). The integer arithmetic is the
//! reference's (NumPy's `//` on non-negative values), so every pixel is the same. Where the reference raises (a format
//! it does not decode, data shorter than the level) this gives `None` and the Python decodes it.

/// The PC header's size: 8 `u32` and 15 level sizes.
pub const HEADER: usize = 92;

/// One level: (slice, level, width, height, its byte range in the file).
pub type Level = (u32, u32, u32, u32, std::ops::Range<usize>);

/// `levels(pc)`: (slice, level, width, height, the level's byte range) of a PC `ITexture` in order, or `None` where
/// the reference raises (a header cut short, more than 15 levels). The ranges are cut at the file's end as the
/// reference's slices are.
#[must_use]
pub fn levels(pc: &[u8]) -> Option<Vec<Level>> {
    let w = |k: usize| -> Option<u32> { pc.get(4 * k..4 * k + 4)?.try_into().ok().map(u32::from_le_bytes) };
    let (typ, width, height, mips) = (w(1)?, w(4)?, w(5)?, w(7)?);
    let sizes: Vec<u32> = (0..15).map(|k| w(8 + k)).collect::<Option<_>>()?;
    let slices = if typ == 1 { 6 } else { 1 };
    let mut out = Vec::new();
    let mut pos = HEADER;
    for s in 0..slices {
        for m in 0..mips {
            let size = *sizes.get(m as usize)? as usize;
            let a = pos.min(pc.len());
            let b = (pos + size).min(pc.len()).max(a);
            out.push((
                s,
                m,
                width.checked_shr(m).unwrap_or(0).max(1),
                height.checked_shr(m).unwrap_or(0).max(1),
                a..b,
            ));
            pos += size;
        }
    }
    Some(out)
}

fn rgb(v: i32) -> [i32; 3] {
    [
        ((v >> 11) & 31) * 255 / 31,
        ((v >> 5) & 63) * 255 / 63,
        (v & 31) * 255 / 31,
    ]
}

/// A DXT colour block's 16 texels.
fn color(c: &[u8], force4: bool) -> [[i32; 3]; 16] {
    let byte = |i: usize| i32::from(c.get(i).copied().unwrap_or(0));
    let (c0, c1) = (byte(0) | (byte(1) << 8), byte(2) | (byte(3) << 8));
    let (p0, p1) = (rgb(c0), rgb(c1));
    let four = c0 > c1 || force4;
    let mix = |f: fn(i32, i32) -> i32| -> [i32; 3] {
        std::array::from_fn(|k| f(p0.get(k).copied().unwrap_or(0), p1.get(k).copied().unwrap_or(0)))
    };
    #[allow(clippy::manual_midpoint)] // NumPy's (p0 + p1) // 2 of the reference, on values 0..255
    let pal = if four {
        [p0, p1, mix(|a, b| (2 * a + b) / 3), mix(|a, b| (a + 2 * b) / 3)]
    } else {
        [p0, p1, mix(|a, b| (a + b) / 2), [0; 3]]
    };
    let bits = u32::from_le_bytes([
        c.get(4).copied().unwrap_or(0),
        c.get(5).copied().unwrap_or(0),
        c.get(6).copied().unwrap_or(0),
        c.get(7).copied().unwrap_or(0),
    ]);
    std::array::from_fn(|k| pal.get(((bits >> (2 * k)) & 3) as usize).copied().unwrap_or([0; 3]))
}

/// A BC4-style (DXT5 alpha) block's 16 values.
fn alpha(a: &[u8]) -> [i32; 16] {
    let byte = |i: usize| i32::from(a.get(i).copied().unwrap_or(0));
    let (a0, a1) = (byte(0), byte(1));
    let big = a0 > a1;
    // 8-value mode (a0 > a1): six interpolated values; 6-value mode: four interpolated, then 0 and 255
    let pal: [i32; 8] = std::array::from_fn(|i| {
        let k = i as i32 - 1;
        match i {
            0 => a0,
            1 => a1,
            _ if big => ((7 - k) * a0 + k * a1) / 7,
            2..=5 => ((5 - k) * a0 + k * a1) / 5,
            6 => 0,
            _ => 255,
        }
    });
    let mut bits = 0u64;
    for k in 0..6 {
        bits |= u64::from(a.get(2 + k).copied().unwrap_or(0)) << (8 * k);
    }
    std::array::from_fn(|k| pal.get(((bits >> (3 * k)) & 7) as usize).copied().unwrap_or(0))
}

/// `decode(level, w, h, fmt)` as (rows, columns, the pixels row by row), or `None` where the reference raises.
#[must_use]
pub fn decode(level: &[u8], w: i64, h: i64, fmt: u32) -> Option<(usize, usize, Vec<[i32; 3]>)> {
    if fmt == 10 {
        let (w, h) = (w.max(1) as usize, h.max(1) as usize);
        let pitch = w.max(level.len() / h);
        let rows = level.get(..pitch.checked_mul(h)?)?;
        let mut px = Vec::with_capacity(w * h);
        for row in rows.chunks_exact(pitch) {
            px.extend(row.get(..w)?.iter().map(|&v| [i32::from(v); 3]));
        }
        return Some((h, w, px));
    }
    let bpb = match fmt {
        0 | 18 => 8,
        1 | 2 | 19 | 20 => 16,
        _ => return None,
    };
    let blocks = |v: i64| ((v + 3).div_euclid(4)).max(1) as usize;
    let (bw, bh) = (blocks(w), blocks(h));
    let data = level.get(..bw.checked_mul(bh)?.checked_mul(bpb)?)?;
    let (out_h, out_w) = (h.max(1) as usize, w.max(1) as usize);
    let mut px = vec![[0i32; 3]; out_h * out_w];
    for (i, blk) in data.chunks_exact(bpb).enumerate() {
        let texels: [[i32; 3]; 16] = match fmt {
            0 | 18 => color(blk, false),
            1 | 2 => color(blk.get(8..)?, true),
            _ => {
                let (r, g) = (alpha(blk.get(..8)?), alpha(blk.get(8..)?));
                std::array::from_fn(|k| [r.get(k).copied().unwrap_or(0), g.get(k).copied().unwrap_or(0), 0])
            }
        };
        let (by, bx) = (i / bw, i % bw);
        for (t, v) in texels.iter().enumerate() {
            let (y, x) = (by * 4 + t / 4, bx * 4 + t % 4);
            if y < out_h && x < out_w {
                *px.get_mut(y * out_w + x)? = *v;
            }
        }
    }
    Some((out_h, out_w, px))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn dxt1_block() {
        // c0 = white (0xFFFF) > c1 = black: four colours; texels 0..15 take indices 0, 1, 2, 3, 0, ...
        let blk = [0xFF, 0xFF, 0, 0, 0b1110_0100, 0b1110_0100, 0b1110_0100, 0b1110_0100];
        let (h, w, px) = decode(&blk, 4, 4, 0).unwrap();
        assert_eq!((h, w), (4, 4));
        assert_eq!(px[0], [255, 255, 255]);
        assert_eq!(px[1], [0, 0, 0]);
        assert_eq!(px[2], [170, 170, 170]);
        assert_eq!(px[3], [85, 85, 85]);
        assert_eq!(decode(&blk[..7], 4, 4, 0), None);
        assert_eq!(decode(&blk, 4, 4, 3), None);
    }

    #[test]
    fn bc5_and_l8() {
        let mut blk = [0u8; 16];
        blk[0] = 200;
        blk[1] = 100;
        blk[8] = 10;
        blk[9] = 20; // six-value mode: index 7 is 255
        blk[10] = 7; // texel 0 of the green block: index 7
        let (_, _, px) = decode(&blk, 2, 2, 19).unwrap();
        assert_eq!(px[0], [200, 255, 0]);
        let (h, w, px) = decode(&[1, 2, 9, 3, 4, 9], 2, 2, 10).unwrap();
        assert_eq!((h, w), (2, 2));
        assert_eq!(px, vec![[1; 3], [2; 3], [3; 3], [4; 3]]);
    }

    #[test]
    fn levels_of_a_header() {
        let mut pc = vec![0u8; HEADER];
        for (k, v) in [(4, 8u32), (5, 4), (7, 2), (8, 32), (9, 8)] {
            pc[4 * k..4 * k + 4].copy_from_slice(&v.to_le_bytes());
        }
        pc.extend([0u8; 40]);
        let lv = levels(&pc).unwrap();
        assert_eq!(lv, vec![(0, 0, 8, 4, 92..124), (0, 1, 4, 2, 124..132)]);
        assert_eq!(levels(&pc[..50]), None);
    }
}
