//! Terrain height fields (`Terrains/<Level>/<Level>.Heightfield-000.res`, type `TerrainHeightfield`, in each
//! level's `terrain-00.fbrb`), ported from `core/bfbc2core/terrain.py`; the tail's layout from
//! `bfbc2-console-convert/terrain.py`.
//!
//! Layout (little-endian; checked against the in-game terrain samples of 13 campaign levels, DESIGN.md section 7):
//!
//! | offset | | |
//! |---|---|---|
//! | `0x00` | 6 x `f32` | world bounds: min x, y, z, max x, y, z |
//! | `0x18` | 3 x `u32` | size n (samples per side), log2(n) - 3, log2(n) - 2 (0 on a sub-terrain) |
//! | `0x24` | 2 x `u16` | 0, n / 256 |
//! | `0x28` | `u32`, `u8` | 0, 0 in every level |
//! | `0x2D` | `u32` | A: the length in bytes of the tail's first section |
//! | `0x31` | n x n `u16` | heights, rows along z: height = min y + value / 65535 * (max y - min y) |
//!
//! Then the tail ([`Tail`]): on a main block (n = 1024 or 2048) n `u32` offsets (in `u16` units) into the first
//! section, the first section (A bytes) and a part of fixed length (62,474 bytes at 1024, 256,012 at 2048); on a
//! sub-terrain (the ring of `Heightfield-001` ... around the main block, n = 64 or 128) a min / max quadtree of
//! 16-sample cells. What the sections hold is not decoded.
//!
//! | Python (`bfbc2core.terrain`) | here |
//! |---|---|
//! | `read(path)` / `parse(data)` | [`parse`] |
//! | `Heightfield.raw`, `.heights`, `.min`, `.max`, `.size`, `.cell` | [`Heightfield::raw`], [`Heightfield::heights`], [`Heightfield::min`], ... |
//! | `Heightfield.height(x, z)` | [`Heightfield::height`] |
//! | `Heightfield.mesh(stride)` | [`Heightfield::mesh`] |
//! | `ValueError` | [`TerrainError`] |
//!
//! The heights are computed as the reference computes them (in `f64`, then rounded to `f32`), so they are
//! bit-identical to its NumPy arrays.
//!
//! ```
//! use bc2_formats::terrain;
//!
//! // a 2 x 2 height field: bounds (0, 10, 0) - (8, 20, 8), heights 0, 65535 / 0, 32768
//! let mut data = Vec::new();
//! for v in [0.0f32, 10.0, 0.0, 8.0, 20.0, 8.0] {
//!     data.extend(v.to_le_bytes());
//! }
//! data.extend(2u32.to_le_bytes());
//! data.resize(0x31, 0);
//! for h in [0u16, 65535, 0, 32768] {
//!     data.extend(h.to_le_bytes());
//! }
//! let hf = terrain::parse(&data)?;
//! assert_eq!(hf.size, 2);
//! assert_eq!(hf.heights()[1], 20.0);
//! assert_eq!(hf.cell(), (4.0, 4.0));
//! assert_eq!(hf.height(2.0, 0.0), 15.0);
//! # Ok::<(), terrain::TerrainError>(())
//! ```

use thiserror::Error;

use crate::bytes;

/// The header's length in bytes: the heights start here.
pub const HEADER_LEN: usize = 0x31;

/// The tail's fixed-length part after the first section, by size (`bfbc2-console-convert/terrain.py` `FIXED`).
pub const FIXED: [(u32, usize); 2] = [(1024, 62_474), (2048, 256_012)];

/// Why a height field could not be read.
///
/// The Python reference raises `ValueError` for each (a file too short for the header raised `struct.error` before
/// `terrain.parse`; it is a `ValueError` now in both readers).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TerrainError {
    /// The file is shorter than the 49-byte header.
    #[error("not a TerrainHeightfield (the file has {len} bytes, the header 49)")]
    TooShort {
        /// The file's length.
        len: usize,
    },
    /// The size is 0 or not a power of two, or the file is too short for its heights.
    #[error("not a TerrainHeightfield (size {size})")]
    BadSize {
        /// The size the header gives.
        size: u32,
    },
}

/// The bytes after the heights, as far as their layout is known (`bfbc2-console-convert/terrain.py`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Tail {
    /// A main block's tail: `size` offsets into the first section, the first section (`A` bytes as `u16`s), then
    /// the part of fixed length ([`FIXED`]).
    Sections {
        /// Offsets in `u16` units into `first`.
        offsets: Vec<u32>,
        /// The first section.
        first: Vec<u16>,
        /// The fixed-length part.
        fixed: Vec<u16>,
    },
    /// A sub-terrain's min / max quadtree: (min, max) per node, the root first.
    Quadtree(Vec<[u16; 2]>),
    /// A layout not described above (the bytes are kept as they are).
    Unknown(Vec<u8>),
}

/// A decoded height field.
#[derive(Debug, Clone, PartialEq)]
pub struct Heightfield {
    /// World bounds: min x, y, z.
    pub min: [f32; 3],
    /// World bounds: max x, y, z.
    pub max: [f32; 3],
    /// Samples per side.
    pub size: u32,
    /// The header's other words: `0x1C`, `0x20` (log2(n) - 3, log2(n) - 2 or 0), `0x24`, `0x26` (0, n / 256),
    /// `0x28` (0), `0x2C` (a byte, 0) and `0x2D` (A, the tail's first section in bytes).
    pub header: HeaderWords,
    /// The stored values, row by row (rows along z, columns along x), `size * size` of them.
    pub raw: Vec<u16>,
    /// The bytes after the heights.
    pub tail: Tail,
}

/// The header's words after the bounds and the size (see [`Heightfield::header`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HeaderWords {
    /// `0x1C`: log2(size) - 3.
    pub log_a: u32,
    /// `0x20`: log2(size) - 2 (0 on a sub-terrain).
    pub log_b: u32,
    /// `0x24`: 0.
    pub w24: u16,
    /// `0x26`: size / 256.
    pub w26: u16,
    /// `0x28`: 0.
    pub w28: u32,
    /// `0x2C`: 0.
    pub b2c: u8,
    /// `0x2D`: the tail's first section in bytes (A).
    pub first_len: u32,
}

/// `N` bytes at `at` (the caller checked the length; zero otherwise).
fn take<const N: usize>(data: &[u8], at: usize) -> [u8; N] {
    bytes::array(data, at).unwrap_or([0; N])
}

fn u32_at(data: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(take(data, at))
}

/// The number of nodes of a sub-terrain's min / max quadtree of 16-sample cells: 1 + 4 + ... + (n / 16)^2.
#[must_use]
pub fn quadtree_nodes(size: u32) -> usize {
    let levels = (size / 16).bit_width();
    (0..levels).map(|k| 4usize.pow(k)).sum()
}

/// Reads a `TerrainHeightfield`.
///
/// # Errors
///
/// [`TerrainError`] when the file is shorter than its header, or its size is 0, not a power of two or larger than
/// the file holds (as the Python reference refuses it).
pub fn parse(data: &[u8]) -> Result<Heightfield, TerrainError> {
    if data.len() < HEADER_LEN {
        return Err(TerrainError::TooShort { len: data.len() });
    }
    let [x0, y0, z0, x1, y1, z1]: [f32; 6] = bytes::first(bytes::f32s(data));
    let (min, max) = ([x0, y0, z0], [x1, y1, z1]);
    let size = u32_at(data, 0x18);
    // in 128 bits: the largest size's heights do not fit in 64
    let end = HEADER_LEN as u128 + 2 * u128::from(size) * u128::from(size);
    if size == 0 || !size.is_power_of_two() || end > data.len() as u128 {
        return Err(TerrainError::BadSize { size });
    }
    let end = end as usize;
    let header = HeaderWords {
        log_a: u32_at(data, 0x1C),
        log_b: u32_at(data, 0x20),
        w24: u16::from_le_bytes(take(data, 0x24)),
        w26: u16::from_le_bytes(take(data, 0x26)),
        w28: u32_at(data, 0x28),
        b2c: data.get(0x2C).copied().unwrap_or(0),
        first_len: u32_at(data, 0x2D),
    };
    let raw = bytes::u16s(data.get(HEADER_LEN..end).unwrap_or_default()).collect();
    let tail = tail_of(data.get(end..).unwrap_or_default(), size, header.first_len);
    Ok(Heightfield {
        min,
        max,
        size,
        header,
        raw,
        tail,
    })
}

/// The tail's layout (see [`Tail`]); [`Tail::Unknown`] when neither layout fits its length.
fn tail_of(tail: &[u8], size: u32, first_len: u32) -> Tail {
    let n = size as usize;
    if let Some(&(_, fixed)) = FIXED.iter().find(|(s, _)| *s == size) {
        let a = first_len as usize;
        if a.is_multiple_of(2) && tail.len() == 4 * n + a + fixed {
            let (offsets, rest) = tail.split_at(4 * n);
            let (first, fixed) = rest.split_at(a);
            return Tail::Sections {
                offsets: bytes::u32s(offsets).collect(),
                first: bytes::u16s(first).collect(),
                fixed: bytes::u16s(fixed).collect(),
            };
        }
    } else if first_len == 0 && size >= 16 && tail.len() == 4 * quadtree_nodes(size) {
        let values: Vec<u16> = bytes::u16s(tail).collect();
        return Tail::Quadtree(values.as_chunks::<2>().0.to_vec());
    }
    Tail::Unknown(tail.to_vec())
}

impl Heightfield {
    /// The vertical scale: metres per stored unit, `(max y - min y) / 65535` in `f64`.
    fn scale(&self) -> f64 {
        (f64::from(self.max[1]) - f64::from(self.min[1])) / 65535.0
    }

    /// The heights in metres, row by row: `min y + value * ((max y - min y) / 65535)` in `f64`, rounded to `f32`
    /// (the reference's NumPy expression, bit for bit).
    #[must_use]
    pub fn heights(&self) -> Vec<f32> {
        let (lo, scale) = (f64::from(self.min[1]), self.scale());
        self.raw.iter().map(|&r| (lo + f64::from(r) * scale) as f32).collect()
    }

    /// The height in metres of sample (`i` along x, `j` along z), or `None` outside the grid.
    #[must_use]
    pub fn sample(&self, i: usize, j: usize) -> Option<f32> {
        let n = self.size as usize;
        if i >= n || j >= n {
            return None;
        }
        let r = *self.raw.get(j * n + i)?;
        Some((f64::from(self.min[1]) + f64::from(r) * self.scale()) as f32)
    }

    /// The size of a grid square in metres along x and z.
    #[must_use]
    pub fn cell(&self) -> (f64, f64) {
        let n = f64::from(self.size);
        (
            (f64::from(self.max[0]) - f64::from(self.min[0])) / n,
            (f64::from(self.max[2]) - f64::from(self.min[2])) / n,
        )
    }

    /// The height at world (`x`, `z`), each grid square split along its (i, j)-(i+1, j+1) diagonal, as the game
    /// splits them; outside the grid the edge squares extend. The reference's arithmetic, step for step.
    #[must_use]
    #[expect(clippy::cast_precision_loss, reason = "grid indices are below 2^32, exact in f64")]
    pub fn height(&self, x: f64, z: f64) -> f64 {
        let (cell_x, cell_z) = self.cell();
        let col = (x - f64::from(self.min[0])) / cell_x;
        let row = (z - f64::from(self.min[2])) / cell_z;
        // NumPy's clip(floor(u), 0, n - 2): maximum, then minimum (so -1 on a 1 x 1 grid, whose index wraps)
        let top = i64::from(self.size) - 2;
        let clip = |v: f64| (v.floor() as i64).max(0).min(top);
        let (ci, ri) = (clip(col), clip(row));
        let (frac_x, frac_z) = (col - ci as f64, row - ri as f64);
        let size = i64::from(self.size);
        let at = |c: i64, r: i64| {
            self.sample(c.rem_euclid(size) as usize, r.rem_euclid(size) as usize)
                .unwrap_or(f32::NAN)
        };
        let (h00, h10, h01, h11) = (at(ci, ri), at(ci + 1, ri), at(ci, ri + 1), at(ci + 1, ri + 1));
        if frac_x > frac_z {
            f64::from(h00) + f64::from(h10 - h00) * frac_x + f64::from(h11 - h10) * frac_z
        } else {
            f64::from(h00) + f64::from(h01 - h00) * frac_z + f64::from(h11 - h01) * frac_x
        }
    }

    /// A triangle mesh on every `stride`-th sample (a stride of 0 is taken as 1): vertices (x, height, z) row by
    /// row, then the triangles (a, c, d) of every square followed by the (a, d, b) of every square, where a is a
    /// square's (i, j) corner, b (i+1, j), c (i, j+1), d (i+1, j+1): the reference's vertices and triangles exactly.
    #[must_use]
    #[expect(clippy::cast_precision_loss, reason = "sample indices are below 2^32, exact in f64")]
    pub fn mesh(&self, stride: usize) -> (Vec<[f32; 3]>, Vec<[u32; 3]>) {
        let stride = stride.max(1);
        let size = self.size as usize;
        let picked: Vec<usize> = (0..size).step_by(stride).collect();
        let (cell_x, cell_z) = self.cell();
        let xs: Vec<f32> = picked
            .iter()
            .map(|&col| (f64::from(self.min[0]) + col as f64 * cell_x) as f32)
            .collect();
        let heights = self.heights();
        let mut verts = Vec::with_capacity(picked.len() * picked.len());
        for &row in &picked {
            let z = (f64::from(self.min[2]) + row as f64 * cell_z) as f32;
            for (&col, &x) in picked.iter().zip(&xs) {
                verts.push([x, heights.get(row * size + col).copied().unwrap_or(f32::NAN), z]);
            }
        }
        let per_row = picked.len();
        let squares = per_row.saturating_sub(1);
        let corner = |row: usize, col: usize| (row * per_row + col) as u32;
        let mut tris = Vec::with_capacity(2 * squares * squares);
        for half in 0..2 {
            for row in 0..squares {
                for col in 0..squares {
                    let (v00, v10) = (corner(row, col), corner(row, col + 1));
                    let (v01, v11) = (corner(row + 1, col), corner(row + 1, col + 1));
                    tris.push(if half == 0 { [v00, v01, v11] } else { [v00, v11, v10] });
                }
            }
        }
        (verts, tris)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
mod tests {
    use super::*;

    /// A height field of `n` x `n` samples with the given values and tail.
    fn file(n: u32, values: &[u16], words: [u32; 2], first_len: u32, tail: &[u8]) -> Vec<u8> {
        let mut d = Vec::new();
        for v in [-100.0f32, 5.0, -50.0, 100.0, 105.0, 150.0] {
            d.extend(v.to_le_bytes());
        }
        d.extend(n.to_le_bytes());
        d.extend(words[0].to_le_bytes());
        d.extend(words[1].to_le_bytes());
        d.extend(0u16.to_le_bytes());
        d.extend(((n / 256) as u16).to_le_bytes());
        d.extend(0u32.to_le_bytes());
        d.push(0);
        d.extend(first_len.to_le_bytes());
        assert_eq!(d.len(), HEADER_LEN);
        for v in values {
            d.extend(v.to_le_bytes());
        }
        d.extend(tail);
        d
    }

    #[test]
    fn refuses_what_the_reference_refuses() {
        assert_eq!(parse(&[0; 48]), Err(TerrainError::TooShort { len: 48 }));
        assert_eq!(
            parse(&file(0, &[], [0, 0], 0, &[])),
            Err(TerrainError::BadSize { size: 0 })
        );
        assert_eq!(
            parse(&file(3, &[0; 9], [0, 0], 0, &[])),
            Err(TerrainError::BadSize { size: 3 })
        );
        assert_eq!(
            parse(&file(4, &[0; 15], [0, 0], 0, &[])),
            Err(TerrainError::BadSize { size: 4 })
        );
        // a size whose heights overflow 32 bits is refused, not wrapped
        for size in [1 << 31, u32::MAX] {
            assert_eq!(
                parse(&file(size, &[], [0, 0], 0, &[])),
                Err(TerrainError::BadSize { size })
            );
        }
        assert!(TerrainError::BadSize { size: 3 }.to_string().contains("size 3"));
        assert!(TerrainError::TooShort { len: 2 }.to_string().contains("2 bytes"));
    }

    #[test]
    fn heights_mesh_and_lookup() {
        let values: Vec<u16> = (0..16).map(|k| k * 4000).collect();
        let hf = parse(&file(4, &values, [0, 0], 0, &[])).unwrap();
        assert_eq!(hf.raw, values);
        assert_eq!(hf.min, [-100.0, 5.0, -50.0]);
        let h = hf.heights();
        assert_eq!(h[0], 5.0);
        assert_eq!(h[15], (5.0 + 60_000.0 * (100.0 / 65535.0)) as f32);
        assert_eq!(hf.cell(), (50.0, 50.0));
        assert_eq!(hf.sample(4, 0), None);
        // corners of square (0, 0): exact samples
        assert_eq!(hf.height(-100.0, -50.0), f64::from(h[0]));
        // both halves of a square, and a point beyond the grid (the edge square extends)
        let lower = hf.height(-60.0, -40.0);
        let upper = hf.height(-90.0, -10.0);
        assert!(lower > f64::from(h[0]) && upper > f64::from(h[0]));
        assert!(hf.height(1000.0, 1000.0).is_finite());
        let (v, t) = hf.mesh(2);
        assert_eq!(v.len(), 4);
        assert_eq!(v[1], [0.0, h[2], -50.0]);
        assert_eq!(t, vec![[0, 2, 3], [0, 3, 1]]);
        let (v, t) = hf.mesh(0);
        assert_eq!((v.len(), t.len()), (16, 18));
        assert_eq!(hf.tail, Tail::Unknown(Vec::new()));
    }

    #[test]
    fn tails() {
        // a sub-terrain: 64 samples, 1 + 4 + 16 quadtree nodes
        assert_eq!(quadtree_nodes(64), 21);
        assert_eq!(quadtree_nodes(128), 85);
        let mut q = Vec::new();
        for k in 0..21u16 {
            q.extend(k.to_le_bytes());
            q.extend((k + 1).to_le_bytes());
        }
        let hf = parse(&file(64, &[7; 64 * 64], [3, 0], 0, &q)).unwrap();
        assert_eq!(hf.header.log_a, 3);
        match &hf.tail {
            Tail::Quadtree(nodes) => assert_eq!((nodes.len(), nodes[20]), (21, [20, 21])),
            other => panic!("{other:?}"),
        }
        // a main block: offsets, the first section, the fixed part
        let n = 1024u32;
        let mut tail = Vec::new();
        for k in 0..n {
            tail.extend((2 * k).to_le_bytes());
        }
        tail.extend([1, 0, 2, 0]);
        tail.extend(vec![0u8; 62_474]);
        let hf = parse(&file(n, &vec![0; (n * n) as usize], [7, 8], 4, &tail)).unwrap();
        assert_eq!(hf.header.w26, 4);
        match &hf.tail {
            Tail::Sections { offsets, first, fixed } => {
                assert_eq!(
                    (offsets.len(), offsets[3], first.as_slice(), fixed.len()),
                    (1024, 6, &[1u16, 2][..], 31_237)
                );
            }
            other => panic!("{other:?}"),
        }
        // the same with a wrong first-section length: unknown
        let hf = parse(&file(n, &vec![0; (n * n) as usize], [7, 8], 6, &tail)).unwrap();
        assert!(matches!(hf.tail, Tail::Unknown(ref b) if b.len() == tail.len()));
    }
}
