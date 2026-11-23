//! Terrain resources decoded whole and written again (round E5; spec `formats/terrain.md` TERR-HF-4 - TERR-HF-11,
//! TERR-MM-1 - TERR-MM-4, `transforms/terrain_edit.md` TEDIT-HF-3 / TEDIT-MM-1): the main height-field block with its
//! tail (the depth map, the min / max tree, the lower envelope), a sub-terrain block's min / max quadtree, and the
//! physics material map (`TerrainMaterialMap`).
//!
//! What a writer computes and what it keeps:
//!
//! - [`MainBlock::encode`] writes the block from its parts; DICE's 30 main blocks decode and encode byte for byte.
//! - [`minmax_tree`] computes the min / max tree from the heights (TERR-HF-10: equal to the file's on DICE's 30
//!   terrains); [`MainBlock::rebuild`] recomputes the nodes over an edited [`Region`] only.
//! - The lower envelope (TERR-HF-11) is not DICE's computation: [`envelope_under`] builds grids under the heights
//!   (each vertex at or under every sample of the grid squares it is a corner of, so every triangle between vertices
//!   lies under the samples it spans), and [`MainBlock::rebuild`] keeps DICE's grids where they still lie under the
//!   ground and lowers the vertices over an edited region that no longer do (TEDIT-HF-3).
//! - The depth map is not derived (TERR-HF-9): a sculpt keeps it; a new block writes all 255.
//! - [`MaterialMap::encode`] codes every row as DICE does (TERR-MM-2: all 30 DICE maps byte for byte).
//!
//! ```
//! use bc2_formats::terrain_codec::{MainBlock, MaterialMap, Region};
//!
//! // a new 1,024-sample block of 1 m, 0 - 500 m, a slope along x; then a hill raised and its derived parts rebuilt
//! let n = 1024usize;
//! let heights: Vec<u16> = (0..n * n).map(|i| ((i % n) * 30) as u16).collect();
//! let mut b = MainBlock::new([-512.0, 0.0, -512.0, 512.0, 500.0, 512.0], heights, None)?;
//! let bytes = b.encode();
//! assert_eq!(MainBlock::decode(&bytes)?, b);
//! b.heights[500 * n + 500] = 60_000;
//! b.rebuild(Region::around(500, 500, 1));
//! assert_eq!(b.tree, bc2_formats::terrain_codec::minmax_tree(&b.heights, 1024));
//!
//! // a material map of 2,048 x 2,048 cells: every cell `defaultmaterial`, one `grass`
//! let mut m = MaterialMap::new(2048, vec!["defaultmaterial".into(), "grass".into()])?;
//! m.set(10, 20, 1);
//! assert_eq!(MaterialMap::decode(&m.encode())?, m);
//! # Ok::<(), bc2_formats::terrain_codec::CodecError>(())
//! ```

use thiserror::Error;

use crate::bytes;
use crate::terrain::{HEADER_LEN, HeaderWords};

/// Why a terrain resource could not be decoded or made.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CodecError {
    /// The height field's header or heights do not read ([`crate::terrain::parse`]'s refusal).
    #[error("{0}")]
    Heightfield(#[from] crate::terrain::TerrainError),
    /// The tail is not a main block's (TERR-HF-9 - TERR-HF-11), with what is wrong.
    #[error("not a main height-field block's tail: {0}")]
    Tail(String),
    /// The material map does not read (TERR-MM-1 - TERR-MM-2), with what is wrong.
    #[error("not a TerrainMaterialMap: {0}")]
    Material(String),
    /// The parts given to make a resource do not fit together, with what is wrong.
    #[error("cannot write the terrain resource: {0}")]
    Shape(String),
}

/// A result of this module.
pub type Result<T> = std::result::Result<T, CodecError>;

/// A rectangle of samples (or material cells): columns `x0 .. x1` along x, rows `z0 .. z1` along z (ends excluded).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Region {
    /// First column.
    pub x0: usize,
    /// First row.
    pub z0: usize,
    /// One past the last column.
    pub x1: usize,
    /// One past the last row.
    pub z1: usize,
}

impl Region {
    /// The square of samples within `r` of (`x`, `z`) (cut at 0; [`Region::clip`] cuts the far side).
    #[must_use]
    pub fn around(x: usize, z: usize, r: usize) -> Self {
        Self {
            x0: x.saturating_sub(r),
            z0: z.saturating_sub(r),
            x1: x + r + 1,
            z1: z + r + 1,
        }
    }

    /// The whole of an `n` x `n` grid.
    #[must_use]
    pub fn all(n: usize) -> Self {
        Self {
            x0: 0,
            z0: 0,
            x1: n,
            z1: n,
        }
    }

    /// Cut to an `n` x `n` grid.
    #[must_use]
    pub fn clip(self, n: usize) -> Self {
        Self {
            x0: self.x0.min(n),
            z0: self.z0.min(n),
            x1: self.x1.min(n),
            z1: self.z1.min(n),
        }
    }

    /// Whether it holds no sample.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.x0 >= self.x1 || self.z0 >= self.z1
    }

    /// The smallest region holding both (an empty one is left out).
    #[must_use]
    pub fn union(self, o: Self) -> Self {
        if self.is_empty() {
            return o;
        }
        if o.is_empty() {
            return self;
        }
        Self {
            x0: self.x0.min(o.x0),
            z0: self.z0.min(o.z0),
            x1: self.x1.max(o.x1),
            z1: self.z1.max(o.z1),
        }
    }

    /// Whether it meets the inclusive rectangle `x0 ..= x1`, `z0 ..= z1`.
    fn meets(&self, x0: usize, z0: usize, x1: usize, z1: usize) -> bool {
        !self.is_empty() && self.x0 <= x1 && x0 < self.x1 && self.z0 <= z1 && z0 < self.z1
    }
}

// ------------------------------------------------------------------------------------------------- the heights

/// The lowest and highest value of the inclusive rectangle of an `n`-wide grid.
fn bounds(h: &[u16], n: usize, x0: usize, z0: usize, x1: usize, z1: usize) -> [u16; 2] {
    let mut lo = u16::MAX;
    let mut hi = 0;
    for z in z0..=z1 {
        for &v in h.get(z * n + x0..=z * n + x1).unwrap_or_default() {
            lo = lo.min(v);
            hi = hi.max(v);
        }
    }
    [lo, hi]
}

/// The nodes of a min / max tree over an `n` x `n` block, as (level side in samples, nodes per side): the root
/// (side n) first, down to cells of 16 samples (TERR-HF-5, TERR-HF-10).
fn tree_levels(n: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut size = n;
    while size >= 16 {
        out.push((size, n / size));
        size /= 2;
    }
    out
}

/// The node `(r, c)` of a level of side `size`: its inclusive rectangle (its samples and the row and column after
/// them inside the block).
fn node_rect(n: usize, size: usize, r: usize, c: usize) -> (usize, usize, usize, usize) {
    let last = n.saturating_sub(1);
    (
        c * size,
        r * size,
        ((c + 1) * size).min(last),
        ((r + 1) * size).min(last),
    )
}

/// The min / max tree of an `n` x `n` block of heights (rows along z): (min, max) per node, the root first, each
/// level's nodes row by row, down to cells of 16 samples; a node covers its samples and the row and column after them
/// inside the block (TERR-HF-10; a sub-terrain's quadtree is the same, TERR-HF-5).
#[must_use]
pub fn minmax_tree(heights: &[u16], n: usize) -> Vec<[u16; 2]> {
    let mut out = Vec::new();
    for (size, k) in tree_levels(n) {
        for r in 0..k {
            for c in 0..k {
                let (x0, z0, x1, z1) = node_rect(n, size, r, c);
                out.push(bounds(heights, n, x0, z0, x1, z1));
            }
        }
    }
    out
}

/// The tree's nodes over `region` computed again (the rest kept).
fn update_tree(tree: &mut [[u16; 2]], heights: &[u16], n: usize, region: Region) {
    let mut at = 0;
    for (size, k) in tree_levels(n) {
        for r in 0..k {
            for c in 0..k {
                let (x0, z0, x1, z1) = node_rect(n, size, r, c);
                if region.meets(x0, z0, x1, z1)
                    && let Some(slot) = tree.get_mut(at)
                {
                    *slot = bounds(heights, n, x0, z0, x1, z1);
                }
                at += 1;
            }
        }
    }
}

/// The lower envelope's grid sides, the coarsest first: 2, 3, 5, ..., n / 8 + 1 (TERR-HF-11).
#[must_use]
pub fn envelope_sides(n: usize) -> Vec<usize> {
    let mut sides = Vec::new();
    let mut k = n / 8;
    while k >= 1 {
        sides.push(k + 1);
        k /= 2;
    }
    sides.reverse();
    sides
}

/// The samples vertex (`i`, `j`) of a grid of `side` bounds from below: the grid squares it is a corner of, as an
/// inclusive rectangle (x0, z0, x1, z1); the last square of a row reaches the block's last sample.
fn vertex_window(n: usize, side: usize, i: usize, j: usize) -> (usize, usize, usize, usize) {
    let step = n / side.saturating_sub(1).max(1);
    let last = n.saturating_sub(1);
    let lo = |v: usize| v.saturating_sub(1) * step;
    let hi = |v: usize| {
        if v + 1 >= side.saturating_sub(1) {
            last
        } else {
            ((v + 1) * step).min(last)
        }
    };
    (lo(j), lo(i), hi(j), hi(i))
}

/// Grids under the heights (TEDIT-HF-3, for a new block): each vertex the lowest sample of the grid squares it is a
/// corner of, so the triangles between vertices lie at or under every sample they span. The coarsest grid first,
/// each row by row (rows along z).
#[must_use]
pub fn envelope_under(heights: &[u16], n: usize) -> Vec<Vec<u16>> {
    envelope_sides(n)
        .into_iter()
        .map(|side| {
            let mut g = Vec::with_capacity(side * side);
            for i in 0..side {
                for j in 0..side {
                    let (x0, z0, x1, z1) = vertex_window(n, side, i, j);
                    g.push(bounds(heights, n, x0, z0, x1, z1)[0]);
                }
            }
            g
        })
        .collect()
}

/// The vertices whose squares meet `region` lowered to the lowest sample of their squares where that is lower (the
/// rest of each grid kept as it is).
fn lower_envelope(grids: &mut [Vec<u16>], heights: &[u16], n: usize, region: Region) {
    for (g, side) in grids.iter_mut().zip(envelope_sides(n)) {
        for i in 0..side {
            for j in 0..side {
                let (x0, z0, x1, z1) = vertex_window(n, side, i, j);
                if region.meets(x0, z0, x1, z1)
                    && let Some(v) = g.get_mut(i * side + j)
                {
                    *v = (*v).min(bounds(heights, n, x0, z0, x1, z1)[0]);
                }
            }
        }
    }
}

/// The height of a grid of `side` vertices over an `n`-sample block at sample (`x`, `z`), each square split along its
/// (i, j)-(i+1, j+1) diagonal (the probe's `below`).
#[must_use]
pub fn envelope_at(grid: &[u16], side: usize, n: usize, x: usize, z: usize) -> f64 {
    let step = n / side.saturating_sub(1).max(1);
    let row = (z / step).min(side.saturating_sub(2));
    let col = (x / step).min(side.saturating_sub(2));
    // grid sizes stay far below 2^32: exact in f64
    let real = |v: usize| f64::from(v as u32);
    let fz = (real(z) - real(row * step)) / real(step);
    let fx = (real(x) - real(col * step)) / real(step);
    let vert = |r: usize, c: usize| f64::from(grid.get(r * side + c).copied().unwrap_or(0));
    let (v00, v01) = (vert(row, col), vert(row, col + 1));
    let (v10, v11) = (vert(row + 1, col), vert(row + 1, col + 1));
    if fx >= fz {
        v00 + (v01 - v00) * fx + (v11 - v01) * fz
    } else {
        v00 + (v10 - v00) * fz + (v11 - v10) * fx
    }
}

// ------------------------------------------------------------------------------------------------- the depth map

/// The depth map run-length coded by rows (TERR-HF-9): the row offsets (in `u16` units) and the runs.
fn encode_depth(depth: &[u8], n: usize) -> (Vec<u32>, Vec<u16>) {
    let mut offsets = Vec::with_capacity(n);
    let mut runs = Vec::new();
    for row in depth.chunks(n.max(1)).take(n) {
        offsets.push(runs.len() as u32);
        let mut k = 0;
        while let Some(&v) = row.get(k) {
            let len = row.get(k..).unwrap_or_default().iter().take_while(|&&x| x == v).count();
            runs.push(len as u16);
            runs.push(u16::from(v));
            k += len;
        }
    }
    (offsets, runs)
}

/// The depth map's runs read back: `n` x `n` values, and the number of `u16`s the runs took.
fn decode_depth(offsets: &[u32], runs: &[u16], n: usize) -> Result<(Vec<u8>, usize)> {
    let mut depth = Vec::with_capacity(n * n);
    let mut at = 0usize;
    for (row, &off) in offsets.iter().enumerate() {
        if off as usize != at {
            return Err(CodecError::Tail(format!("row {row}: offset {off}, expected {at}")));
        }
        let mut filled = 0;
        while filled < n {
            let (Some(&count), Some(&value)) = (runs.get(at), runs.get(at + 1)) else {
                return Err(CodecError::Tail(format!("row {row}: the runs end early")));
            };
            let count = usize::from(count);
            let Ok(byte) = u8::try_from(value) else {
                return Err(CodecError::Tail(format!("row {row}: run value {value}")));
            };
            if count == 0 || filled + count > n {
                return Err(CodecError::Tail(format!("row {row}: run ({count}, {value})")));
            }
            depth.extend(std::iter::repeat_n(byte, count));
            filled += count;
            at += 2;
        }
    }
    Ok((depth, at))
}

/// A: the length in bytes of the row offsets, the depth map's runs and the padding to 16 (header 0x2D).
fn first_len(depth: &[u8], n: usize) -> u32 {
    let rle = 2 * encode_depth(depth, n).1.len();
    (4 * n + rle + (16 - (4 * n + rle) % 16) % 16) as u32
}

// ------------------------------------------------------------------------------------------------- main blocks

/// A main height-field block (n = 1,024 or 2,048 samples a side; TERR-HF-1 - TERR-HF-4, TERR-HF-9 - TERR-HF-11),
/// every part decoded.
#[derive(Debug, Clone, PartialEq)]
pub struct MainBlock {
    /// World bounds: min x, y, z.
    pub min: [f32; 3],
    /// World bounds: max x, y, z.
    pub max: [f32; 3],
    /// Samples per side.
    pub size: u32,
    /// The header's other words (`first_len` is written from the tail, whatever it holds).
    pub header: HeaderWords,
    /// The stored heights, rows along z: height = min y + value / 65535 x (max y - min y).
    pub heights: Vec<u16>,
    /// The depth map, one byte per sample (TERR-HF-9).
    pub depth: Vec<u8>,
    /// The min / max tree (TERR-HF-10).
    pub tree: Vec<[u16; 2]>,
    /// The lower envelope's grids, the coarsest first (TERR-HF-11).
    pub envelope: Vec<Vec<u16>>,
}

/// The header words a new block of `n` samples has (DICE's: log2(n) - 3, log2(n) - 2, 0, n / 256, 0, 0).
#[must_use]
pub fn main_header(n: u32) -> HeaderWords {
    let log = n.max(1).ilog2();
    HeaderWords {
        log_a: log.saturating_sub(3),
        log_b: log.saturating_sub(2),
        w24: 0,
        w26: (n / 256) as u16,
        w28: 0,
        b2c: 0,
        first_len: 0,
    }
}

impl MainBlock {
    /// A new main block from its bounds (min x, y, z, max x, y, z) and stored heights (n x n, rows along z), the
    /// depth map given or all 255, the tree and the envelope built under the heights, DICE's header words.
    ///
    /// # Errors
    /// The heights are not a square of a power of two of at least 16 samples a side, or the depth map is not as long.
    pub fn new(bounds: [f32; 6], heights: Vec<u16>, depth: Option<Vec<u8>>) -> Result<Self> {
        let n = heights.len().isqrt();
        if n * n != heights.len() || !n.is_power_of_two() || n < 16 {
            return Err(CodecError::Shape(format!(
                "{} heights are not a square of a power of two of 16 or more",
                heights.len()
            )));
        }
        let depth = depth.unwrap_or_else(|| vec![255; n * n]);
        if depth.len() != n * n {
            return Err(CodecError::Shape(format!(
                "a depth map of {} values for {} samples",
                depth.len(),
                n * n
            )));
        }
        let [x0, y0, z0, x1, y1, z1] = bounds;
        let mut header = main_header(n as u32);
        header.first_len = first_len(&depth, n);
        Ok(Self {
            min: [x0, y0, z0],
            max: [x1, y1, z1],
            size: n as u32,
            header,
            tree: minmax_tree(&heights, n),
            envelope: envelope_under(&heights, n),
            heights,
            depth,
        })
    }

    /// Reads a main block (a sub-terrain block or another tail is refused).
    ///
    /// # Errors
    /// [`CodecError::Heightfield`] for the header and heights, [`CodecError::Tail`] when the tail is not a main
    /// block's whole.
    pub fn decode(data: &[u8]) -> Result<Self> {
        let hf = crate::terrain::parse(data)?;
        let n = hf.size as usize;
        if hf.header.log_b == 0 || n < 16 {
            return Err(CodecError::Tail("a sub-terrain block (header word 0x20 is 0)".into()));
        }
        let end = HEADER_LEN + 2 * n * n;
        let tail = data.get(end..).unwrap_or_default();
        let offsets: Vec<u32> = bytes::u32s(tail.get(..4 * n).unwrap_or_default()).collect();
        if offsets.len() != n {
            return Err(CodecError::Tail("shorter than its row offsets".into()));
        }
        let rest: Vec<u16> = bytes::u16s(tail.get(4 * n..).unwrap_or_default()).collect();
        let (depth, used) = decode_depth(&offsets, &rest, n)?;
        let rle = 2 * used;
        let pad = (16 - (4 * n + rle) % 16) % 16;
        if hf.header.first_len as usize != 4 * n + rle + pad {
            return Err(CodecError::Tail(format!(
                "A is {}, the depth map and its padding {}",
                hf.header.first_len,
                4 * n + rle + pad
            )));
        }
        if tail
            .get(4 * n + rle..4 * n + rle + pad)
            .is_none_or(|p| p.iter().any(|&b| b != 0))
        {
            return Err(CodecError::Tail("the padding is not zeros".into()));
        }
        let nodes = tree_levels(n).iter().map(|(_, k)| k * k).sum::<usize>();
        let sides = envelope_sides(n);
        let grids: usize = sides.iter().map(|s| s * s).sum();
        if tail.len() != 4 * n + rle + pad + 4 * nodes + 2 * grids {
            return Err(CodecError::Tail(format!(
                "{} bytes, the parts {}",
                tail.len(),
                4 * n + rle + pad + 4 * nodes + 2 * grids
            )));
        }
        let after: Vec<u16> = bytes::u16s(tail.get(4 * n + rle + pad..).unwrap_or_default()).collect();
        let (tree, grid_values) = after.split_at(2 * nodes);
        let tree = tree.as_chunks::<2>().0.to_vec();
        let mut envelope = Vec::with_capacity(sides.len());
        let mut at = 0;
        for s in sides {
            envelope.push(grid_values.get(at..at + s * s).unwrap_or_default().to_vec());
            at += s * s;
        }
        Ok(Self {
            min: hf.min,
            max: hf.max,
            size: hf.size,
            header: hf.header,
            heights: hf.raw,
            depth,
            tree,
            envelope,
        })
    }

    /// The block's file (A in the header written from the depth map's length).
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let n = self.size as usize;
        let (offsets, runs) = encode_depth(&self.depth, n);
        let rle = 2 * runs.len();
        let pad = (16 - (4 * n + rle) % 16) % 16;
        let h = &self.header;
        let mut out = Vec::with_capacity(HEADER_LEN + 2 * n * n + 4 * n + rle + pad + 4 * self.tree.len() + 50_000);
        for v in self.min.iter().chain(&self.max) {
            out.extend(v.to_le_bytes());
        }
        out.extend(self.size.to_le_bytes());
        out.extend(h.log_a.to_le_bytes());
        out.extend(h.log_b.to_le_bytes());
        out.extend(h.w24.to_le_bytes());
        out.extend(h.w26.to_le_bytes());
        out.extend(h.w28.to_le_bytes());
        out.push(h.b2c);
        out.extend(((4 * n + rle + pad) as u32).to_le_bytes());
        out.extend(self.heights.iter().flat_map(|v| v.to_le_bytes()));
        out.extend(offsets.iter().flat_map(|v| v.to_le_bytes()));
        out.extend(runs.iter().flat_map(|v| v.to_le_bytes()));
        out.resize(out.len() + pad, 0);
        out.extend(self.tree.iter().flatten().flat_map(|v| v.to_le_bytes()));
        out.extend(self.envelope.iter().flatten().flat_map(|v| v.to_le_bytes()));
        out
    }

    /// The derived parts made right after the heights changed over `region` (TEDIT-HF-3): the tree's nodes over it
    /// computed again, the envelope's vertices over it lowered where the new heights dip under them (DICE's grids
    /// kept elsewhere and where they still lie under the ground); the depth map kept.
    pub fn rebuild(&mut self, region: Region) {
        let n = self.size as usize;
        let region = region.clip(n);
        update_tree(&mut self.tree, &self.heights, n, region);
        lower_envelope(&mut self.envelope, &self.heights, n, region);
    }

    /// The share of samples at or under the finest envelope grid (with half a unit's slack, as the spec's probe
    /// counts them); 1.0 for a block whose envelope lies under all its heights.
    #[must_use]
    pub fn under_share(&self) -> f64 {
        let n = self.size as usize;
        let (Some(g), Some(&side)) = (self.envelope.last(), envelope_sides(n).last()) else {
            return 1.0;
        };
        let mut under = 0usize;
        for z in 0..n {
            for x in 0..n {
                let h = f64::from(self.heights.get(z * n + x).copied().unwrap_or(0));
                under += usize::from(h >= envelope_at(g, side, n, x, z) - 0.5);
            }
        }
        f64::from(under as u32) / f64::from((n * n).max(1) as u32)
    }
}

/// A sub-terrain block's quadtree computed from its stored heights (TERR-HF-5), as `(min, max)` nodes.
#[must_use]
pub fn sub_quadtree(hf: &crate::terrain::Heightfield) -> Vec<[u16; 2]> {
    minmax_tree(&hf.raw, hf.size as usize)
}

// ------------------------------------------------------------------------------------------------- material maps

/// Cells a block of the material map holds: 512 a row, 256 rows (TERR-MM-2).
pub const BLOCK_W: usize = 512;
/// Rows of a material map block.
pub const BLOCK_H: usize = 256;
/// Names a material map can hold (4-bit cells).
pub const MAX_NAMES: usize = 16;

/// A physics material map (`TerrainMaterialMap`, TERR-MM-1 - TERR-MM-4): a width x width grid of material indices
/// (rows along z, covering the main block), and the materials' names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialMap {
    /// Cells a side (2,048 or 4,096).
    pub width: u32,
    /// The cells' indices into `names`, row by row (rows along z).
    pub cells: Vec<u8>,
    /// The materials, index 0 first (`defaultmaterial` on DICE's).
    pub names: Vec<String>,
}

/// One row of 512 cells coded as DICE codes it (TERR-MM-2).
fn encode_material_row(cells: &[u8], out: &mut Vec<u8>) {
    let pairs: Vec<u8> = cells
        .chunks(2)
        .map(|p| p.first().map_or(0, |a| a & 15) | (p.get(1).map_or(0, |b| b & 15) << 4))
        .collect();
    let mut k = 0;
    while let Some(&b) = pairs.get(k) {
        let mut left = pairs
            .get(k..)
            .unwrap_or_default()
            .iter()
            .take_while(|&&x| x == b)
            .count();
        k += left;
        while left > 0 {
            if left == 1 {
                out.push(b);
                left = 0;
            } else {
                let take = left.min(256);
                out.extend([b, b, (take - 1) as u8]);
                left -= take;
            }
        }
    }
}

/// One coded row read back as 256 pairs (512 cells).
fn decode_material_row(row: &[u8], out: &mut Vec<u8>) -> std::result::Result<(), String> {
    let mut pairs = Vec::with_capacity(BLOCK_W / 2);
    let mut prev: Option<u8> = None;
    let mut q = 0;
    while let Some(&x) = row.get(q) {
        q += 1;
        if prev == Some(x) {
            let c = *row.get(q).ok_or("a run without its count")?;
            q += 1;
            pairs.extend(std::iter::repeat_n(x, usize::from(c)));
            prev = None;
        } else {
            pairs.push(x);
            prev = Some(x);
        }
    }
    if pairs.len() != BLOCK_W / 2 {
        return Err(format!("{} pairs", pairs.len()));
    }
    for p in pairs {
        out.extend([p & 15, p >> 4]);
    }
    Ok(())
}

impl MaterialMap {
    /// A map of `width` x `width` cells, every cell index 0.
    ///
    /// # Errors
    /// The width is not a multiple of 512, or there are no names or more than 16.
    pub fn new(width: u32, names: Vec<String>) -> Result<Self> {
        let w = width as usize;
        if w == 0 || !w.is_multiple_of(BLOCK_W) {
            return Err(CodecError::Shape(format!(
                "a material map {width} cells wide (a multiple of 512)"
            )));
        }
        if names.is_empty() || names.len() > MAX_NAMES {
            return Err(CodecError::Shape(format!("{} material names (1 - 16)", names.len())));
        }
        Ok(Self {
            width,
            cells: vec![0; w * w],
            names,
        })
    }

    /// The index of cell (`x`, `z`) (0 outside).
    #[must_use]
    pub fn get(&self, x: usize, z: usize) -> u8 {
        let w = self.width as usize;
        if x >= w {
            return 0;
        }
        self.cells.get(z * w + x).copied().unwrap_or(0)
    }

    /// Sets cell (`x`, `z`) (nothing outside).
    pub fn set(&mut self, x: usize, z: usize, index: u8) {
        let w = self.width as usize;
        if x < w
            && let Some(c) = self.cells.get_mut(z * w + x)
        {
            *c = index & 15;
        }
    }

    /// A material's index, added to the names when the map lacks it (`None`: 16 names already).
    pub fn index_of(&mut self, name: &str) -> Option<u8> {
        if let Some(k) = self.names.iter().position(|n| n.eq_ignore_ascii_case(name)) {
            return Some(k as u8);
        }
        if self.names.len() >= MAX_NAMES {
            return None;
        }
        self.names.push(name.to_owned());
        Some((self.names.len() - 1) as u8)
    }

    /// Reads a material map.
    ///
    /// # Errors
    /// [`CodecError::Material`] when a part does not read whole.
    pub fn decode(d: &[u8]) -> Result<Self> {
        let bad = |s: String| CodecError::Material(s);
        let width = u32::from_le_bytes(bytes::array(d, 0).ok_or_else(|| bad("no header".into()))?);
        let nb = usize::from(u16::from_le_bytes(
            bytes::array(d, 4).ok_or_else(|| bad("no header".into()))?,
        ));
        let w = width as usize;
        let per_row = w / BLOCK_W;
        if per_row == 0 || !w.is_multiple_of(BLOCK_W) || nb != per_row * (w / BLOCK_H) {
            return Err(bad(format!("{width} cells wide in {nb} blocks")));
        }
        let mut cells = vec![0u8; w * w];
        let mut o = 6;
        let mut row = Vec::with_capacity(BLOCK_W);
        for k in 0..nb {
            let len =
                u32::from_le_bytes(bytes::array(d, o).ok_or_else(|| bad(format!("block {k}: cut short")))?) as usize;
            let data = d
                .get(o + 4..o + 4 + len)
                .ok_or_else(|| bad(format!("block {k}: cut short")))?;
            let table: Vec<u16> = bytes::u16s(
                d.get(o + 4 + len..o + 4 + len + 2 * BLOCK_H)
                    .ok_or_else(|| bad(format!("block {k}: no row lengths")))?,
            )
            .collect();
            o += 4 + len + 2 * BLOCK_H;
            let (by, bx) = (k / per_row, k % per_row);
            let mut at = 0;
            for (r, &rl) in table.iter().enumerate() {
                let rl = usize::from(rl);
                let coded = data
                    .get(at..at + rl)
                    .ok_or_else(|| bad(format!("block {k} row {r}: past its block")))?;
                at += rl;
                row.clear();
                decode_material_row(coded, &mut row).map_err(|e| bad(format!("block {k} row {r}: {e}")))?;
                let start = (by * BLOCK_H + r) * w + bx * BLOCK_W;
                if let Some(dst) = cells.get_mut(start..start + BLOCK_W) {
                    dst.copy_from_slice(&row);
                }
            }
            if at != len {
                return Err(bad(format!("block {k}: its rows take {at} of {len} bytes")));
            }
        }
        let size = u32::from_le_bytes(bytes::array(d, o).ok_or_else(|| bad("no names".into()))?) as usize;
        let names_raw = d.get(o + 4..).unwrap_or_default();
        if names_raw.len() != size || names_raw.last().is_some_and(|&b| b != 0) {
            return Err(bad(format!("names of {size} bytes, {} to the end", names_raw.len())));
        }
        let names = names_raw
            .split(|&b| b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect();
        Ok(Self { width, cells, names })
    }

    /// The map's file.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let w = self.width as usize;
        let per_row = w / BLOCK_W;
        let nb = per_row * (w / BLOCK_H);
        let mut out = Vec::with_capacity(6 + nb * (4 + 2 * BLOCK_H + 1024));
        out.extend(self.width.to_le_bytes());
        out.extend((nb as u16).to_le_bytes());
        let mut block = Vec::new();
        let mut table = Vec::with_capacity(BLOCK_H);
        for k in 0..nb {
            let (by, bx) = (k / per_row, k % per_row);
            block.clear();
            table.clear();
            for r in 0..BLOCK_H {
                let start = (by * BLOCK_H + r) * w + bx * BLOCK_W;
                let before = block.len();
                encode_material_row(self.cells.get(start..start + BLOCK_W).unwrap_or_default(), &mut block);
                table.push((block.len() - before) as u16);
            }
            out.extend((block.len() as u32).to_le_bytes());
            out.extend(&block);
            out.extend(table.iter().flat_map(|v| v.to_le_bytes()));
        }
        let names: Vec<u8> = self.names.iter().flat_map(|n| n.bytes().chain([0])).collect();
        out.extend((names.len() as u32).to_le_bytes());
        out.extend(names);
        out
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn a_new_block_round_trips_and_its_envelope_lies_under() {
        let n = 1024;
        // a bumpy field: a slope with a pit and a spike
        let mut h: Vec<u16> = (0..n * n).map(|i| ((i % n) * 20 + (i / n) * 7) as u16).collect();
        h[300 * n + 300] = 0;
        h[700 * n + 100] = 65_535;
        let mut b = MainBlock::new([-512.0, 0.0, -512.0, 512.0, 500.0, 512.0], h, None).unwrap();
        assert_eq!(b.header.log_a, 7);
        assert_eq!(b.header.log_b, 8);
        assert_eq!(b.header.w26, 4);
        assert_eq!(b.tree.len(), 5461);
        assert_eq!(b.envelope.iter().map(Vec::len).sum::<usize>(), 22_363);
        assert!((b.under_share() - 1.0).abs() < f64::EPSILON);
        let bytes = b.encode();
        let back = MainBlock::decode(&bytes).unwrap();
        assert!(PartialEq::eq(&back, &b), "decoded block differs");
        assert_eq!(back.encode(), bytes);
        // a pit dug, rebuilt over its region: the tree is the computed one, the envelope still under
        for z in 500..520 {
            for x in 600..640 {
                b.heights[z * n + x] = 3;
            }
        }
        let before = b.envelope.clone();
        b.rebuild(Region {
            x0: 600,
            z0: 500,
            x1: 640,
            z1: 520,
        });
        assert!(PartialEq::eq(&b.tree, &minmax_tree(&b.heights, n)));
        assert!((b.under_share() - 1.0).abs() < f64::EPSILON);
        // the coarse grids far from the pit kept
        assert_eq!(b.envelope[7][0], before[7][0]);
        assert!(PartialEq::ne(&b.envelope, &before));
    }

    #[test]
    fn depth_maps_are_maximal_runs() {
        let n = 16;
        let mut d = vec![255u8; n * n];
        d[3] = 0;
        d[4] = 0;
        let (off, runs) = encode_depth(&d, n);
        assert_eq!(off[0], 0);
        assert_eq!(&runs[..6], &[3, 255, 2, 0, 11, 255]);
        assert_eq!(off[1], 6);
        let (back, used) = decode_depth(&off, &runs, n).unwrap();
        assert_eq!((back, used), (d, runs.len()));
        assert!(decode_depth(&[0, 7], &runs, n).is_err());
        let refused = MainBlock::new([0.0; 6], vec![0; 10], None).unwrap_err();
        assert!(refused.to_string().contains("square"), "{refused}");
    }

    #[test]
    fn material_rows_code_as_dice_codes_them() {
        let mut out = Vec::new();
        let mut cells = vec![1u8; 512];
        cells[0] = 2;
        cells[1] = 3;
        encode_material_row(&cells, &mut out);
        // one pair 0x32, then 255 pairs of 0x11
        assert_eq!(out, [0x32, 0x11, 0x11, 254]);
        let mut back = Vec::new();
        decode_material_row(&out, &mut back).unwrap();
        assert_eq!(back, cells);
        let mut m = MaterialMap::new(1024, vec!["defaultmaterial".into()]).unwrap();
        let k = m.index_of("Grass").unwrap();
        assert_eq!(k, 1);
        assert_eq!(m.index_of("grass"), Some(1));
        m.set(1023, 1023, k);
        m.set(5000, 0, k);
        let bytes = m.encode();
        assert!(PartialEq::eq(&MaterialMap::decode(&bytes).unwrap(), &m));
        assert!(MaterialMap::decode(&bytes[..bytes.len() - 1]).is_err());
        assert!(MaterialMap::new(1000, vec!["a".into()]).is_err());
    }
}
