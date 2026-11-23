//! Water: `Terrains/<Terrain>/<Terrain>.Water.Mesh.res` (type `WaterMesh`) and `.Water.res` (type `VisualWater`),
//! in each level's `terrain-00.fbrb`; ported from `core/bfbc2core/water.py`.
//!
//! `WaterMesh` (little-endian; read on all levels with a terrain bundle, each file consumed exactly): `u32` bodies,
//! then per body `u32` vertex count, the vertices (`f32` x, y, z), `u32` index count and the `u32` indices
//! (triangles). Bodies are joined, each body's indices moved past the vertices before it.
//!
//! `VisualWater` (the drawing copy; the parts used here): `u32` 10, `u32` vertex stride, `u32` vertex bytes, the
//! vertices, `u32` index bytes, the `u32` indices, `u32` slot count and the slots' shader paths (each 0-terminated
//! and followed by one 0 byte), the fragment shader's path, `u32` range count and per range (`u32` slot, `u32`
//! triangle count, `u32` first index, `u32` index count).
//!
//! | Python (`bfbc2core.water`) | here |
//! |---|---|
//! | `read_mesh(path)` / `parse_mesh(data)` | [`parse_mesh`] |
//! | `read_visual(path)` / `parse_visual(data)` | [`parse_visual`] |
//! | the shader per triangle in `read(folder, name)` | [`triangle_shaders`] |
//!
//! ```
//! use bc2_formats::water;
//!
//! // one body: a triangle
//! let mut data = Vec::new();
//! for v in [1u32, 3] {
//!     data.extend(v.to_le_bytes());
//! }
//! for v in [0.0f32, 4.5, 0.0, 1.0, 4.5, 0.0, 0.0, 4.5, 1.0] {
//!     data.extend(v.to_le_bytes());
//! }
//! for v in [3u32, 0, 1, 2] {
//!     data.extend(v.to_le_bytes());
//! }
//! let mesh = water::parse_mesh(&data)?;
//! assert_eq!(mesh.triangles, vec![[0, 1, 2]]);
//! assert_eq!(mesh.vertices[2], [0.0, 4.5, 1.0]);
//! # Ok::<(), water::WaterError>(())
//! ```

use thiserror::Error;

use crate::bytes;

/// Why a water file could not be read (the Python reference raises `ValueError` or `struct.error` for each).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum WaterError {
    /// The file ends inside a field or an array.
    #[error("the file ends at byte {len}, a field needs byte {needed}")]
    PastEnd {
        /// The file's length.
        len: usize,
        /// One past the last byte the field needs.
        needed: u64,
    },
    /// A body's index count is not a multiple of 3.
    #[error("{count} indices are not whole triangles")]
    NotTriangles {
        /// The index count.
        count: u32,
    },
    /// Bytes are left after the last body.
    #[error("{0} unread bytes - not the WaterMesh layout")]
    Unread(usize),
    /// A shader path has no terminating 0 byte.
    #[error("a string at byte {0} has no end")]
    StringWithoutEnd(usize),
}

/// A `WaterMesh`: every body's vertices and triangles, joined.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WaterMesh {
    /// The vertices (x, y, z).
    pub vertices: Vec<[f32; 3]>,
    /// The triangles: indices into `vertices`.
    pub triangles: Vec<[u32; 3]>,
}

/// The parts of a `VisualWater` the shaders per triangle come from.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VisualWater {
    /// The index buffer.
    pub indices: Vec<u32>,
    /// The slots' shader paths.
    pub slots: Vec<String>,
    /// The fragment shader's path (`Systems/Water/WaterOceanFragment`).
    pub fragment: String,
    /// The draw ranges: (slot, triangle count, first index, index count).
    pub ranges: Vec<[u32; 4]>,
}

/// A bounds-checked little-endian reader over a file.
struct Reader<'a> {
    data: &'a [u8],
    at: u64,
}

impl<'a> Reader<'a> {
    fn bytes(&mut self, n: u64) -> Result<&'a [u8], WaterError> {
        let end = self.at + n;
        let past = || WaterError::PastEnd {
            len: self.data.len(),
            needed: end,
        };
        let s = self
            .data
            .get(usize::try_from(self.at).map_err(|_| past())?..usize::try_from(end).map_err(|_| past())?)
            .ok_or_else(past)?;
        self.at = end;
        Ok(s)
    }

    fn u32(&mut self) -> Result<u32, WaterError> {
        Ok(bytes::u32s(self.bytes(4)?).next().unwrap_or(0))
    }
}

/// Groups of `K` values.
fn groups<T: Copy, const K: usize>(values: &[T]) -> impl Iterator<Item = [T; K]> + '_ {
    values.as_chunks::<K>().0.iter().copied()
}

/// Reads a `WaterMesh`: every body's vertices and triangles, joined (a body's indices offset by the vertices of
/// the bodies before it, wrapping at 2^32 as the reference's `uint32` arrays do).
///
/// # Errors
///
/// [`WaterError`] when the file ends early, a body's index count is not a multiple of 3, or bytes are left over.
pub fn parse_mesh(data: &[u8]) -> Result<WaterMesh, WaterError> {
    let mut r = Reader { data, at: 0 };
    let bodies = r.u32()?;
    let mut out = WaterMesh::default();
    let mut base = 0u32;
    for _ in 0..bodies {
        let verts = r.u32()?;
        let coords: Vec<f32> = bytes::f32s(r.bytes(12 * u64::from(verts))?).collect();
        out.vertices.extend(groups::<f32, 3>(&coords));
        let count = r.u32()?;
        let indices: Vec<u32> = bytes::u32s(r.bytes(4 * u64::from(count))?).collect();
        if !count.is_multiple_of(3) {
            return Err(WaterError::NotTriangles { count });
        }
        out.triangles
            .extend(groups::<u32, 3>(&indices).map(|t| t.map(|i| i.wrapping_add(base))));
        base = base.wrapping_add(verts);
    }
    let left = data.len() as u64 - r.at;
    if left != 0 {
        return Err(WaterError::Unread(left as usize));
    }
    Ok(out)
}

/// Reads the index buffer, slots and draw ranges of a `VisualWater`.
///
/// The slot names are read as the reference reads them: up to each 0 byte, as ASCII (any other byte becomes
/// U+FFFD), one more 0 byte skipped after each slot (not after the fragment shader's name).
///
/// # Errors
///
/// [`WaterError`] when the file ends early or a name has no end.
pub fn parse_visual(data: &[u8]) -> Result<VisualWater, WaterError> {
    let mut r = Reader { data, at: 0 };
    let _format = r.u32()?;
    let _stride = r.u32()?;
    let size = r.u32()?;
    r.at += u64::from(size);
    let ib = r.u32()?;
    let at = r.at;
    let indices = bytes::u32s(r.bytes(4 * u64::from(ib / 4))?).collect();
    r.at = at + u64::from(ib);
    let slots = r.u32()?;
    let mut names = Vec::new();
    for _ in 0..=u64::from(slots) {
        let start = usize::try_from(r.at).unwrap_or(usize::MAX);
        let end = data
            .get(start..)
            .and_then(|s| s.iter().position(|&b| b == 0))
            .map(|p| start + p)
            .ok_or(WaterError::StringWithoutEnd(start))?;
        names.push(
            data.get(start..end)
                .unwrap_or_default()
                .iter()
                .map(|&b| {
                    if b.is_ascii() {
                        char::from(b)
                    } else {
                        char::REPLACEMENT_CHARACTER
                    }
                })
                .collect::<String>(),
        );
        r.at = end as u64 + 1;
        if names.len() as u64 <= u64::from(slots) && data.get(end + 1) == Some(&0) {
            r.at += 1;
        }
    }
    let count = r.u32()?;
    let mut ranges = Vec::new();
    for _ in 0..count {
        ranges.push(bytes::first(bytes::u32s(r.bytes(16)?)));
    }
    let fragment = names.pop().unwrap_or_default();
    Ok(VisualWater {
        indices,
        slots: names,
        fragment,
        ranges,
    })
}

/// The shader of each triangle of `mesh` from `visual`'s draw ranges, and the slots, as `water.read` gives them:
/// `None` when the two files' indices differ. A range's triangles past the mesh are ignored; a slot past the
/// slot list is named `slot <n>`; a triangle no range covers has an empty name.
#[must_use]
pub fn triangle_shaders(mesh: &WaterMesh, visual: &VisualWater) -> Option<Vec<String>> {
    let flat = mesh.triangles.iter().flatten();
    if visual.indices.len() != 3 * mesh.triangles.len() || !visual.indices.iter().eq(flat) {
        return None;
    }
    let mut shader = vec![String::new(); mesh.triangles.len()];
    for &[slot, ntri, first, _] in &visual.ranges {
        let name = visual
            .slots
            .get(slot as usize)
            .cloned()
            .unwrap_or_else(|| format!("slot {slot}"));
        let start = (first / 3) as usize;
        let end = start.saturating_add(ntri as usize).min(shader.len());
        for s in shader.get_mut(start..end).unwrap_or_default() {
            s.clone_from(&name);
        }
    }
    Some(shader)
}

// ------------------------------------------------------------------------------------ the visual water, written

/// A `VisualWater` decoded whole (round TG2; WATER-VIS-1 - 4): every part kept, so DICE's 28 visual waters decode
/// and encode byte for byte (`tests/visual_water_corpus.rs`) and a writer changes only what it means to.
///
/// Each slot is a pair of NUL-terminated names (the second empty on DICE's but for the ocean slot's, which names
/// the water fragment shader: what [`parse_visual`] reads as the fragment); the bytes after the ranges (the far plane,
/// `f32` 8192, on DICE's) are kept as they are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualWaterFile {
    /// The format word (10).
    pub format: u32,
    /// Bytes a vertex (36 on the PC).
    pub stride: u32,
    /// The vertices as stored.
    pub vertices: Vec<u8>,
    /// The index buffer.
    pub indices: Vec<u32>,
    /// The slots: (shader entry, second name).
    pub slots: Vec<(String, String)>,
    /// The draw ranges: (slot, triangle count, first index, index count).
    pub ranges: Vec<[u32; 4]>,
    /// The bytes after the ranges.
    pub tail: Vec<u8>,
}

/// DICE's bytes after a visual water's ranges: the far plane, `f32` 8192.0 (6000 on `SP_004_B` alone).
pub const VISUAL_WATER_TAIL: [u8; 4] = [0, 0, 0, 0x46];

impl VisualWaterFile {
    /// Reads a visual water whole.
    ///
    /// # Errors
    /// [`WaterError`] when a part runs past the end or a name has no end.
    pub fn decode(data: &[u8]) -> Result<Self, WaterError> {
        let mut r = Reader { data, at: 0 };
        let format = r.u32()?;
        let stride = r.u32()?;
        let vb = r.u32()?;
        let vertices = r.bytes(u64::from(vb))?.to_vec();
        let ib = r.u32()?;
        let indices = bytes::u32s(r.bytes(u64::from(ib))?).collect();
        let count = r.u32()?;
        let name = |r: &mut Reader<'_>| -> Result<String, WaterError> {
            let start = usize::try_from(r.at).unwrap_or(usize::MAX);
            let end = data
                .get(start..)
                .and_then(|s| s.iter().position(|&b| b == 0))
                .map(|p| start + p)
                .ok_or(WaterError::StringWithoutEnd(start))?;
            r.at = end as u64 + 1;
            Ok(data
                .get(start..end)
                .unwrap_or_default()
                .iter()
                .map(|&b| char::from(b))
                .collect())
        };
        let mut slots = Vec::new();
        for _ in 0..count {
            let a = name(&mut r)?;
            let b = name(&mut r)?;
            slots.push((a, b));
        }
        let nr = r.u32()?;
        let mut ranges = Vec::new();
        for _ in 0..nr {
            ranges.push(bytes::first(bytes::u32s(r.bytes(16)?)));
        }
        let tail = data
            .get(usize::try_from(r.at).unwrap_or(usize::MAX)..)
            .unwrap_or_default()
            .to_vec();
        Ok(Self {
            format,
            stride,
            vertices,
            indices,
            slots,
            ranges,
            tail,
        })
    }

    /// The file.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.vertices.len() + 4 * self.indices.len() + 256);
        out.extend(self.format.to_le_bytes());
        out.extend(self.stride.to_le_bytes());
        out.extend((self.vertices.len() as u32).to_le_bytes());
        out.extend(&self.vertices);
        out.extend((4 * self.indices.len() as u32).to_le_bytes());
        out.extend(self.indices.iter().flat_map(|v| v.to_le_bytes()));
        out.extend((self.slots.len() as u32).to_le_bytes());
        for (a, b) in &self.slots {
            for s in [a, b] {
                out.extend(s.chars().map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?')));
                out.push(0);
            }
        }
        out.extend((self.ranges.len() as u32).to_le_bytes());
        out.extend(self.ranges.iter().flatten().flat_map(|v| v.to_le_bytes()));
        out.extend(&self.tail);
        out
    }
}

/// A PC visual water vertex (WATER-VIS-2, 36 bytes, 18 halves) of DICE's open-water slot (round TG2, measured on
/// every level's open-water triangles: `tests/visual_water_corpus.rs`): the position (x, y, z, 1), the normal
/// (0, 1, 0, 0), the tangent (1, 0, 0, 0), the texture coordinate (x / 512, z / 512) and four zeros.
#[must_use]
pub fn open_water_vertex(x: f32, y: f32, z: f32) -> [u8; 36] {
    let v = [
        x,
        y,
        z,
        1.0,
        0.0,
        1.0,
        0.0,
        0.0,
        1.0,
        0.0,
        0.0,
        0.0,
        x / 512.0,
        z / 512.0,
        0.0,
        0.0,
        0.0,
        0.0,
    ];
    let mut out = [0u8; 36];
    for (o, f) in out.as_chunks_mut::<2>().0.iter_mut().zip(v) {
        *o = bytes::f16_bits(f).to_le_bytes();
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
mod tests {
    use super::*;

    fn le(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    fn two_bodies() -> Vec<u8> {
        let mut d = le(&[2, 3]);
        d.extend(le(&[0; 9]));
        d.extend(le(&[3, 0, 1, 2, 1]));
        d.extend(le(&[0; 3]));
        d.extend(le(&[3, 0, 0, 0]));
        d
    }

    fn visual(indices: &[u32], ranges: &[[u32; 4]]) -> Vec<u8> {
        let mut d = le(&[10, 36, 4, 7]);
        d.extend(le(&[4 * indices.len() as u32]));
        d.extend(le(indices));
        d.extend(le(&[2]));
        d.extend(b"Water/A\0\0Water/B\xff\0\0Systems/Water/WaterOceanFragment\0");
        d.extend(le(&[ranges.len() as u32]));
        for r in ranges {
            d.extend(le(r));
        }
        d.extend(le(&[0]));
        d.push(0x46);
        d
    }

    #[test]
    fn bodies_are_joined() {
        let m = parse_mesh(&two_bodies()).unwrap();
        assert_eq!(m.vertices.len(), 4);
        assert_eq!(m.triangles, vec![[0, 1, 2], [3, 3, 3]]);
        assert_eq!(parse_mesh(&le(&[0])).unwrap(), WaterMesh::default());
    }

    #[test]
    fn damaged_meshes() {
        let d = two_bodies();
        for cut in 0..d.len() {
            assert!(parse_mesh(&d[..cut]).is_err(), "{cut}");
        }
        let mut long = d.clone();
        long.push(0);
        assert_eq!(parse_mesh(&long), Err(WaterError::Unread(1)));
        let mut odd = le(&[1, 0, 2, 0, 0]);
        assert_eq!(parse_mesh(&odd), Err(WaterError::NotTriangles { count: 2 }));
        odd.truncate(12);
        assert!(matches!(parse_mesh(&odd), Err(WaterError::PastEnd { .. })));
        assert!(WaterError::Unread(3).to_string().contains("3 unread"));
        assert!(WaterError::NotTriangles { count: 2 }.to_string().contains("2 indices"));
        assert!(WaterError::StringWithoutEnd(9).to_string().contains("byte 9"));
    }

    #[test]
    fn shaders_per_triangle() {
        let mesh = parse_mesh(&two_bodies()).unwrap();
        let v = parse_visual(&visual(&[0, 1, 2, 3, 3, 3], &[[1, 1, 3, 3], [0, 1, 0, 3]])).unwrap();
        assert_eq!(v.slots, vec!["Water/A".to_owned(), "Water/B\u{fffd}".to_owned()]);
        assert_eq!(v.fragment, "Systems/Water/WaterOceanFragment");
        assert_eq!(
            triangle_shaders(&mesh, &v).unwrap(),
            vec!["Water/A".to_owned(), "Water/B\u{fffd}".to_owned()]
        );
        // an unknown slot, a range past the end, a triangle no range covers
        let v = parse_visual(&visual(&[0, 1, 2, 3, 3, 3], &[[5, 9, 3, 3]])).unwrap();
        assert_eq!(
            triangle_shaders(&mesh, &v).unwrap(),
            vec![String::new(), "slot 5".to_owned()]
        );
        // indices that differ from the mesh's
        let v = parse_visual(&visual(&[0, 1, 2], &[])).unwrap();
        assert_eq!(triangle_shaders(&mesh, &v), None);
        let v = parse_visual(&visual(&[0, 1, 2, 3, 3, 4], &[])).unwrap();
        assert_eq!(triangle_shaders(&mesh, &v), None);
    }

    #[test]
    fn damaged_visuals() {
        let d = visual(&[0, 1, 2], &[[0, 1, 0, 3]]);
        // every cut before the last range's end fails
        let end = d.len() - 5;
        for cut in 0..end {
            assert!(parse_visual(&d[..cut]).is_err(), "{cut}");
        }
        assert!(parse_visual(&d[..end]).is_ok());
        // a huge vertex size or slot count runs past the end, never panics
        let mut big = d.clone();
        big[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse_visual(&big).is_err());
        let mut many = d;
        many[32..36].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse_visual(&many).is_err());
    }
}
