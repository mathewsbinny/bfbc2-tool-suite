//! The headers of mesh resources: `MeshData` (a mesh LOD's parts and where its vertices and indices are) and the
//! mesh sets (`RigidMeshSet`, `CompositeMeshSet`, `SkinnedMeshSet`, `TreeMeshSet`: a mesh's LODs, shader names,
//! bounds, parts), Xbox 360 (big-endian) or PC (little-endian). A port of `bfbc2-console-convert/mesh.py`'s `parse` /
//! `write` and `meshset.py`'s `parse` / `write`, round F0; layouts in `bfbc2-console-convert/FORMAT.md` 3 and 4.
//!
//! The same values in the same order (floats bit for bit) and the same PC bytes written. Where the reference raises
//! (a read past the end, a name without its NUL, sizes that do not cover the file, bytes after the last LOD) this
//! returns [`Error`] and the Python reads or writes it itself, so the error stays the reference's. A NaN is left to
//! the reference too.

/// Why a header is left to the reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Where the reference raises (or a NaN).
    Refused,
}

type Result<T> = std::result::Result<T, Error>;

struct Reader<'a> {
    b: &'a [u8],
    p: usize,
    big: bool,
}

impl<'a> Reader<'a> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N]> {
        let v: [u8; N] = self
            .b
            .get(self.p..self.p + N)
            .and_then(|s| s.try_into().ok())
            .ok_or(Error::Refused)?;
        self.p += N;
        Ok(v)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take::<1>()?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        let v = self.take::<2>()?;
        Ok(if self.big {
            u16::from_be_bytes(v)
        } else {
            u16::from_le_bytes(v)
        })
    }
    fn u32(&mut self) -> Result<u32> {
        let v = self.take::<4>()?;
        Ok(if self.big {
            u32::from_be_bytes(v)
        } else {
            u32::from_le_bytes(v)
        })
    }
    fn u64(&mut self) -> Result<u64> {
        let v = self.take::<8>()?;
        Ok(if self.big {
            u64::from_be_bytes(v)
        } else {
            u64::from_le_bytes(v)
        })
    }
    fn f32(&mut self) -> Result<f32> {
        let v = f32::from_bits(self.u32()?);
        if v.is_nan() { Err(Error::Refused) } else { Ok(v) }
    }
    fn floats<const N: usize>(&mut self) -> Result<[f32; N]> {
        let mut out = [0f32; N];
        for o in &mut out {
            *o = self.f32()?;
        }
        Ok(out)
    }
    fn cstr(&mut self) -> Result<&'a [u8]> {
        let rest = self.b.get(self.p..).ok_or(Error::Refused)?;
        let e = rest.iter().position(|&c| c == 0).ok_or(Error::Refused)?;
        let s = rest.get(..e).ok_or(Error::Refused)?;
        self.p += e + 1;
        Ok(s)
    }
}

/// One part of a `MeshData` header (`mesh.parse`'s part dict).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeshPart {
    /// Its name.
    pub name: Vec<u8>,
    /// triangles, vertices, first index, vertex offset.
    pub counts: [u32; 4],
    /// stride, k, fa, fb.
    pub bytes: [u8; 4],
    /// The bones it uses.
    pub bones: Vec<u16>,
}

/// A `MeshData` header (`mesh.parse`'s dict).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeshHeader {
    /// The first word.
    pub h0: u32,
    /// Two bytes.
    pub h1: [u8; 2],
    /// The parts.
    pub parts: Vec<MeshPart>,
    /// Four lists of part indices.
    pub lists: [Vec<u8>; 4],
    /// vertex bytes, index bytes, wide indices.
    pub sizes: [u32; 3],
    /// Where the vertices start.
    pub data: usize,
}

/// `mesh.parse(b, big_endian)`.
///
/// # Errors
/// Where the reference raises.
pub fn parse_mesh(b: &[u8], big: bool) -> Result<MeshHeader> {
    let mut r = Reader { b, p: 0, big };
    let h0 = r.u32()?;
    let h1 = [r.u8()?, r.u8()?];
    let n = r.u8()?;
    let mut parts = Vec::with_capacity(usize::from(n));
    for _ in 0..n {
        let name = r.cstr()?.to_vec();
        let counts = [r.u32()?, r.u32()?, r.u32()?, r.u32()?];
        let bytes = [r.u8()?, r.u8()?, r.u8()?, r.u8()?];
        let nb = r.u16()?;
        let bones = (0..nb).map(|_| r.u16()).collect::<Result<Vec<_>>>()?;
        parts.push(MeshPart {
            name,
            counts,
            bytes,
            bones,
        });
    }
    let mut lists: [Vec<u8>; 4] = Default::default();
    for l in &mut lists {
        let n = r.u32()? as usize;
        *l = b.get(r.p..r.p + n).ok_or(Error::Refused)?.to_vec();
        r.p += n;
    }
    let sizes = [r.u32()?, r.u32()?, r.u32()?];
    let data = r.p;
    if data as u64 + u64::from(sizes[0]) + u64::from(sizes[1]) != b.len() as u64 {
        return Err(Error::Refused);
    }
    Ok(MeshHeader {
        h0,
        h1,
        parts,
        lists,
        sizes,
        data,
    })
}

fn le32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn fit32(v: usize) -> Result<u32> {
    u32::try_from(v).map_err(|_| Error::Refused)
}

/// `mesh.write(m, vertices, indices)`: the PC header (its sizes those of `vertices` and `indices`, wide indices 0)
/// and the data.
///
/// # Errors
/// Where the reference raises (more than 255 parts, more than 65,535 bones, sizes past `u32`).
pub fn write_mesh(m: &MeshHeader, vertices: &[u8], indices: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    le32(&mut out, m.h0);
    out.extend_from_slice(&m.h1);
    out.push(u8::try_from(m.parts.len()).map_err(|_| Error::Refused)?);
    for p in &m.parts {
        out.extend_from_slice(&p.name);
        out.push(0);
        for c in p.counts {
            le32(&mut out, c);
        }
        out.extend_from_slice(&p.bytes);
        let nb = u16::try_from(p.bones.len()).map_err(|_| Error::Refused)?;
        out.extend_from_slice(&nb.to_le_bytes());
        for b in &p.bones {
            out.extend_from_slice(&b.to_le_bytes());
        }
    }
    for l in &m.lists {
        le32(&mut out, fit32(l.len())?);
        out.extend_from_slice(l);
    }
    le32(&mut out, fit32(vertices.len())?);
    le32(&mut out, fit32(indices.len())?);
    le32(&mut out, 0);
    out.extend_from_slice(vertices);
    out.extend_from_slice(indices);
    Ok(out)
}

/// A mesh set's kind (`meshset.KINDS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `RigidMeshSet`.
    Rigid,
    /// `CompositeMeshSet`.
    Composite,
    /// `SkinnedMeshSet`.
    Skinned,
    /// `TreeMeshSet`.
    Tree,
}

impl Kind {
    /// The kind of a type name.
    #[must_use]
    pub fn of(name: &str) -> Option<Self> {
        Some(match name {
            "RigidMeshSet" => Self::Rigid,
            "CompositeMeshSet" => Self::Composite,
            "SkinnedMeshSet" => Self::Skinned,
            "TreeMeshSet" => Self::Tree,
            _ => return None,
        })
    }
    fn composite_like(self) -> bool {
        matches!(self, Self::Composite | Self::Tree)
    }
}

/// A composite part: hash, name, bounds, transform.
pub type Part = (u32, Vec<u8>, [f32; 6], [f32; 12]);

/// One LOD of a mesh set (`meshset.parse`'s LOD dict).
#[derive(Debug, Clone, PartialEq)]
pub struct Lod {
    /// The first byte.
    pub x: u8,
    /// Shader names per set.
    pub shaders: u8,
    /// (name, shader names).
    pub sets: Vec<(Vec<u8>, Vec<Vec<u8>>)>,
    /// Bounds.
    pub bounds: [f32; 6],
    /// The `MeshData` size.
    pub size: u32,
    /// The word after it.
    pub extra: u32,
    /// The flag byte (`None` for a skinned set).
    pub flag: Option<u8>,
    /// Composite and tree sets: the parts and each shader's part mask.
    pub parts: Option<(Vec<Part>, Vec<[u64; 3]>)>,
    /// Tree sets: the billboard shader and the value after it.
    pub tree: Option<(Vec<u8>, u32)>,
    /// Skinned sets: the bones and their name hashes.
    pub bones: Option<(Vec<u32>, Vec<u32>)>,
}

/// A mesh set (`meshset.parse`'s dict).
#[derive(Debug, Clone, PartialEq)]
pub struct MeshSet {
    /// The first word.
    pub a: u32,
    /// The byte after it.
    pub z: u8,
    /// The LOD settings' name.
    pub lod_settings: Vec<u8>,
    /// The LODs.
    pub lods: Vec<Lod>,
}

/// `meshset.parse(b, kind, big_endian, flag)`; `flag` is whether a rigid / composite / tree set's LODs have the flag
/// byte (the reference's `flag`, `None` for BFBC2's rule).
///
/// # Errors
/// Where the reference raises.
#[allow(clippy::many_single_char_names)] // the reference's names
pub fn parse_set(b: &[u8], kind: Kind, big: bool, flag: Option<bool>) -> Result<MeshSet> {
    let has_flag = flag.unwrap_or(kind != Kind::Skinned);
    let mut r = Reader { b, p: 0, big };
    let a = r.u32()?;
    let z = r.u8()?;
    let lod_settings = r.cstr()?.to_vec();
    let count = r.u32()?;
    let mut lods = Vec::new();
    for _ in 0..count {
        let (x, n, sets) = (r.u8()?, r.u8()?, r.u8()?);
        let sets = (0..sets)
            .map(|_| {
                let name = r.cstr()?.to_vec();
                let shaders = (0..n).map(|_| Ok(r.cstr()?.to_vec())).collect::<Result<Vec<_>>>()?;
                Ok((name, shaders))
            })
            .collect::<Result<Vec<_>>>()?;
        let bounds = r.floats::<6>()?;
        let (size, extra) = (r.u32()?, r.u32()?);
        let flag = if kind == Kind::Skinned {
            None
        } else if has_flag {
            Some(r.u8()?)
        } else {
            Some(0)
        };
        let parts = if kind.composite_like() {
            let np = r.u32()?;
            let mut parts = Vec::new();
            for _ in 0..np {
                let h = r.u32()?;
                let name = r.cstr()?.to_vec();
                let bounds = r.floats::<6>()?;
                let transform = r.floats::<12>()?;
                parts.push((h, name, bounds, transform));
            }
            let masks = (0..n)
                .map(|_| Ok([r.u64()?, r.u64()?, r.u64()?]))
                .collect::<Result<Vec<_>>>()?;
            Some((parts, masks))
        } else {
            None
        };
        let tree = if kind == Kind::Tree {
            Some((r.cstr()?.to_vec(), r.u32()?))
        } else {
            None
        };
        let bones = if kind == Kind::Skinned {
            let nb = r.u32()?;
            let bones = (0..nb).map(|_| r.u32()).collect::<Result<Vec<_>>>()?;
            let hashes = (0..nb).map(|_| r.u32()).collect::<Result<Vec<_>>>()?;
            Some((bones, hashes))
        } else {
            None
        };
        lods.push(Lod {
            x,
            shaders: n,
            sets,
            bounds,
            size,
            extra,
            flag,
            parts,
            tree,
            bones,
        });
    }
    if r.p != b.len() {
        return Err(Error::Refused);
    }
    Ok(MeshSet {
        a,
        z,
        lod_settings,
        lods,
    })
}

fn float(out: &mut Vec<u8>, v: f32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// `meshset.write(s, kind)`: the PC bytes.
///
/// # Errors
/// Where the reference raises (more than 255 sets, a missing flag, the parts or bones a kind needs absent, a count
/// past `u32`).
pub fn write_set(s: &MeshSet, kind: Kind) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    le32(&mut out, s.a);
    out.push(s.z);
    out.extend_from_slice(&s.lod_settings);
    out.push(0);
    le32(&mut out, fit32(s.lods.len())?);
    for l in &s.lods {
        out.extend_from_slice(&[l.x, l.shaders, u8::try_from(l.sets.len()).map_err(|_| Error::Refused)?]);
        for (name, shaders) in &l.sets {
            out.extend_from_slice(name);
            out.push(0);
            for sh in shaders {
                out.extend_from_slice(sh);
                out.push(0);
            }
        }
        for v in l.bounds {
            float(&mut out, v);
        }
        le32(&mut out, l.size);
        le32(&mut out, l.extra);
        if kind != Kind::Skinned {
            out.push(l.flag.ok_or(Error::Refused)?);
        }
        if kind.composite_like() {
            let (parts, masks) = l.parts.as_ref().ok_or(Error::Refused)?;
            le32(&mut out, fit32(parts.len())?);
            for (h, name, bounds, transform) in parts {
                le32(&mut out, *h);
                out.extend_from_slice(name);
                out.push(0);
                for &v in bounds.iter().chain(transform) {
                    float(&mut out, v);
                }
            }
            for m in masks {
                for v in m {
                    out.extend_from_slice(&v.to_le_bytes());
                }
            }
        }
        if kind == Kind::Tree {
            let (billboard, value) = l.tree.as_ref().ok_or(Error::Refused)?;
            out.extend_from_slice(billboard);
            out.push(0);
            le32(&mut out, *value);
        }
        if kind == Kind::Skinned {
            let (bones, hashes) = l.bones.as_ref().ok_or(Error::Refused)?;
            le32(&mut out, fit32(bones.len())?);
            for &b in bones.iter().chain(hashes) {
                le32(&mut out, b);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn mesh_header_round_trip() {
        let m = MeshHeader {
            h0: 7,
            h1: [1, 2],
            parts: vec![MeshPart {
                name: b"part0".to_vec(),
                counts: [10, 30, 0, 0],
                bytes: [32, 1, 2, 3],
                bones: vec![4, 5],
            }],
            lists: [vec![0], vec![], vec![0], vec![]],
            sizes: [6, 4, 0],
            data: 0,
        };
        let b = write_mesh(&m, b"vvvvvv", b"iiii").unwrap();
        let back = parse_mesh(&b, false).unwrap();
        assert_eq!(back.parts, m.parts);
        assert_eq!(back.sizes, [6, 4, 0]);
        assert_eq!(&b[back.data..back.data + 6], b"vvvvvv");
        assert_eq!(parse_mesh(&b[..b.len() - 1], false), Err(Error::Refused));
        assert_eq!(parse_mesh(&b, true), Err(Error::Refused));
    }

    #[test]
    fn mesh_sets_round_trip() {
        for kind in [Kind::Rigid, Kind::Composite, Kind::Skinned, Kind::Tree] {
            let lod = Lod {
                x: 1,
                shaders: 1,
                sets: vec![(b"set".to_vec(), vec![b"shader".to_vec()])],
                bounds: [0.0, 1.0, -1.0, 2.5, 3.0, 4.0],
                size: 100,
                extra: 0,
                flag: (kind != Kind::Skinned).then_some(1),
                parts: kind
                    .composite_like()
                    .then(|| (vec![(9, b"p".to_vec(), [0.5; 6], [1.0; 12])], vec![[1, 2, 3]])),
                tree: (kind == Kind::Tree).then(|| (b"bb".to_vec(), 1)),
                bones: (kind == Kind::Skinned).then(|| (vec![1, 2], vec![3, 4])),
            };
            let s = MeshSet {
                a: 3,
                z: 0,
                lod_settings: b"lod".to_vec(),
                lods: vec![lod.clone(), lod],
            };
            let b = write_set(&s, kind).unwrap();
            assert_eq!(parse_set(&b, kind, false, None).unwrap(), s, "{kind:?}");
            assert_eq!(parse_set(&b[..b.len() - 1], kind, false, None), Err(Error::Refused));
        }
        assert_eq!(Kind::of("TreeMeshSet"), Some(Kind::Tree));
        assert_eq!(Kind::of("Mesh"), None);
    }
}
