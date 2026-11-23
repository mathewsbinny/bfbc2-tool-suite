//! The `VisualTerrain` resource (`Terrains/<T>/<T>.Visual.res`) decoded whole and written again (round TG2; spec
//! `formats/terrain.md` TERR-VT-1 - TERR-VT-4, TERR-VT-7): the road blocks (kept as their bytes), the patch shaders
//! with their undergrowth entries, and one patch map per terrain block. DICE's 30 visual terrains decode and encode
//! byte for byte (`tests/visual_terrain_corpus.rs`), so a writer that changes one part (a generated terrain's patch
//! maps, its renamed patch shaders) leaves every other byte as DICE wrote it.
//!
//! The drawing side reads the same grammar in `bc2-render` (`terrain::parse_visual`); this one keeps every byte.
//!
//! ```
//! use bc2_formats::visual_terrain::{PatchMap, VisualTerrain};
//!
//! // one block, no roads, no patch shaders, a 4 x 4 patch map
//! let vt = VisualTerrain {
//!     roads: vec![VisualTerrain::EMPTY_ROADS.to_vec(); 2],
//!     shaders: Vec::new(),
//!     maps: vec![PatchMap::rule_only([-32.0, 0.0, -32.0, 32.0, 500.0, 32.0], [66, 1, 15, 7], 4, 0,
//!         "Terrains/T/T.Heightfield-000", "Terrains/T/T.MaskTileAtlas")],
//! };
//! let bytes = vt.encode();
//! assert_eq!(VisualTerrain::decode(&bytes)?, vt);
//! # Ok::<(), bc2_formats::visual_terrain::VtError>(())
//! ```

use thiserror::Error;

/// Why a visual terrain does not decode.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum VtError {
    /// The file ends inside a part.
    #[error("the visual terrain ends inside {0}")]
    Short(&'static str),
    /// A string has no terminating 0.
    #[error("a string at byte {0} has no end")]
    String(usize),
    /// The patch map count is not the block count, or bytes follow the last patch map.
    #[error("{0}")]
    Layout(String),
}

/// A result of this module.
pub type Result<T> = std::result::Result<T, VtError>;

/// One undergrowth mesh entry of a patch shader (TERR-VT-3, VEG-UG-2 / -3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UgMesh {
    /// The mesh set (`TerrainUndergrowthType.Mesh`).
    pub mesh: String,
    /// `<mesh>/<guid>` on `NAM_MP_003`, else empty.
    pub asset: String,
    /// The cache shader entry (`Terrains/<T>/Shaders/..._Cache`), or empty.
    pub shader: String,
    /// The 12 numbers as stored (48 bytes).
    pub values: [u8; 48],
    /// The two bytes that end the entry (01 00 on DICE's).
    pub tail: [u8; 2],
}

/// A patch shader (TERR-VT-3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchShader {
    /// Its patch mask shader entry (`Terrains/<T>/<T>/PatchMaskShaders/<layers>`).
    pub name: String,
    /// Its undergrowth mask shader entry (`.../UgMaskShaders/<layers>`), or empty.
    pub undergrowth: String,
    /// Layers (1 - 8).
    pub layers: u32,
    /// Tiled masks (0 - 4): 0 is a rule-only shader (TGEN-VIS-1).
    pub tiled: u32,
    /// Undergrowth groups of mesh entries.
    pub groups: Vec<Vec<UgMesh>>,
}

/// One terrain block's patch map (TERR-VT-4).
#[derive(Debug, Clone, PartialEq)]
pub struct PatchMap {
    /// min x, y, z, max x, y, z.
    pub bounds: [f32; 6],
    /// Tile size (66), border (1), tiles per row - 1, rows - 1, cells per side W.
    pub header: [u32; 5],
    /// Per cell (rows along z), four mask tiles.
    pub tiles: Vec<[u16; 4]>,
    /// Per cell, four flags (TERR-VT-7).
    pub flags: Vec<[u8; 4]>,
    /// The shader map's side (S = W on DICE's).
    pub shader_side: u32,
    /// Each shader-map cell's patch shader index.
    pub shader: Vec<u8>,
    /// The block's height field (`Terrains/<T>/<T>.Heightfield-000`).
    pub heightfield: String,
    /// The mask-tile atlas.
    pub atlas: String,
}

impl PatchMap {
    /// A patch map whose `side` x `side` cells all draw patch shader `shader` with no tiles and no flags (a
    /// rule-only shader's cells, TEDIT-VT-1): `atlas` the tile size, border and atlas layout words of the header.
    #[must_use]
    pub fn rule_only(
        bounds: [f32; 6],
        atlas: [u32; 4],
        side: u32,
        shader: u8,
        heightfield: &str,
        atlas_name: &str,
    ) -> Self {
        let cells = (side * side) as usize;
        Self {
            bounds,
            header: [atlas[0], atlas[1], atlas[2], atlas[3], side],
            tiles: vec![[0; 4]; cells],
            flags: vec![[0; 4]; cells],
            shader_side: side,
            shader: vec![shader; cells],
            heightfield: heightfield.to_owned(),
            atlas: atlas_name.to_owned(),
        }
    }
}

/// A `VisualTerrain`, every part kept.
#[derive(Debug, Clone, PartialEq)]
pub struct VisualTerrain {
    /// 2N road blocks, each block's full-detail then each block's low-detail (TERR-VT-2), as their bytes.
    pub roads: Vec<Vec<u8>>,
    /// The patch shaders.
    pub shaders: Vec<PatchShader>,
    /// One patch map per terrain block (N).
    pub maps: Vec<PatchMap>,
}

struct Rd<'a> {
    d: &'a [u8],
    o: usize,
}

impl Rd<'_> {
    fn take(&mut self, n: usize, what: &'static str) -> Result<&[u8]> {
        let s = self.d.get(self.o..self.o + n).ok_or(VtError::Short(what))?;
        self.o += n;
        Ok(s)
    }

    fn u32(&mut self, what: &'static str) -> Result<u32> {
        let b = self.take(4, what)?;
        Ok(u32::from_le_bytes(b.try_into().map_err(|_| VtError::Short(what))?))
    }

    fn string(&mut self) -> Result<String> {
        let rest = self.d.get(self.o..).ok_or(VtError::String(self.o))?;
        let end = rest.iter().position(|&b| b == 0).ok_or(VtError::String(self.o))?;
        let s: String = rest.iter().take(end).map(|&b| char::from(b)).collect();
        self.o += end + 1;
        Ok(s)
    }
}

/// A latin-1 string's bytes (characters past U+00FF become `?`), and its terminator.
fn put_str(out: &mut Vec<u8>, s: &str) {
    out.extend(s.chars().map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?')));
    out.push(0);
}

impl VisualTerrain {
    /// An empty road block: no strips, no vertices, 28-byte vertices, no indices (16 bytes).
    pub const EMPTY_ROADS: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

    /// Reads a visual terrain.
    ///
    /// # Errors
    /// [`VtError`] when a part runs past the end, a string has no end, the counts disagree or bytes are left.
    #[allow(
        clippy::indexing_slicing,
        clippy::many_single_char_names,
        clippy::too_many_lines,
        reason = "every slice comes from `take`, which checked its length; the grammar's names (n, m, g, k, s) and                   its parts in one walk"
    )]
    pub fn decode(data: &[u8]) -> Result<Self> {
        let mut r = Rd { d: data, o: 0 };
        let n = r.u32("the block count")? as usize;
        let mut roads = Vec::with_capacity(2 * n);
        for _ in 0..2 * n {
            let start = r.o;
            let strips = r.u32("a road block")?;
            for _ in 0..strips {
                r.string()?;
                r.string()?;
                r.string()?;
                r.take(8 + 24 + 4, "a road strip")?;
            }
            let _v = r.u32("a road block")?;
            let vb = r.u32("a road block")? as usize;
            let i = r.u32("a road block")? as usize;
            r.take(vb, "road vertices")?;
            r.take(4 * i, "road indices")?;
            roads.push(data[start..r.o].to_vec());
        }
        let m = r.u32("the patch shader count")?;
        let mut shaders = Vec::with_capacity(m as usize);
        for _ in 0..m {
            let name = r.string()?;
            let undergrowth = r.string()?;
            let layers = r.u32("a patch shader")?;
            let tiled = r.u32("a patch shader")?;
            let g = r.u32("a patch shader")?;
            let mut groups = Vec::with_capacity(g as usize);
            for _ in 0..g {
                let k = r.u32("an undergrowth group")?;
                let mut group = Vec::with_capacity(k as usize);
                for _ in 0..k {
                    let mesh = r.string()?;
                    let asset = r.string()?;
                    let shader = r.string()?;
                    let mut values = [0u8; 48];
                    values.copy_from_slice(r.take(48, "an undergrowth entry")?);
                    let t = r.take(2, "an undergrowth entry")?;
                    group.push(UgMesh {
                        mesh,
                        asset,
                        shader,
                        values,
                        tail: [t[0], t[1]],
                    });
                }
                groups.push(group);
            }
            shaders.push(PatchShader {
                name,
                undergrowth,
                layers,
                tiled,
                groups,
            });
        }
        if r.u32("the patch map count")? as usize != n {
            return Err(VtError::Layout("the patch map count is not the block count".into()));
        }
        let mut maps = Vec::with_capacity(n);
        for _ in 0..n {
            let b = r.take(24, "a patch map's bounds")?;
            let mut bounds = [0f32; 6];
            for (k, v) in bounds.iter_mut().enumerate() {
                *v = f32::from_le_bytes([b[4 * k], b[4 * k + 1], b[4 * k + 2], b[4 * k + 3]]);
            }
            let mut header = [0u32; 5];
            for h in &mut header {
                *h = r.u32("a patch map's header")?;
            }
            let cells = (header[4] as usize).pow(2);
            let tb = r.take(8 * cells, "the mask tiles")?;
            let tiles = tb
                .as_chunks::<8>()
                .0
                .iter()
                .map(|c| {
                    [
                        u16::from_le_bytes([c[0], c[1]]),
                        u16::from_le_bytes([c[2], c[3]]),
                        u16::from_le_bytes([c[4], c[5]]),
                        u16::from_le_bytes([c[6], c[7]]),
                    ]
                })
                .collect();
            let fb = r.take(4 * cells, "the mask flags")?;
            let flags = fb.as_chunks::<4>().0.to_vec();
            let s = r.u32("the shader map")?;
            let shader = r.take((s as usize).pow(2), "the shader map")?.to_vec();
            let heightfield = r.string()?;
            let atlas = r.string()?;
            maps.push(PatchMap {
                bounds,
                header,
                tiles,
                flags,
                shader_side: s,
                shader,
                heightfield,
                atlas,
            });
        }
        if r.o != data.len() {
            return Err(VtError::Layout(format!(
                "{} bytes after the last patch map",
                data.len() - r.o
            )));
        }
        Ok(Self { roads, shaders, maps })
    }

    /// The file.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend((self.maps.len() as u32).to_le_bytes());
        for r in &self.roads {
            out.extend(r);
        }
        out.extend((self.shaders.len() as u32).to_le_bytes());
        for s in &self.shaders {
            put_str(&mut out, &s.name);
            put_str(&mut out, &s.undergrowth);
            out.extend(s.layers.to_le_bytes());
            out.extend(s.tiled.to_le_bytes());
            out.extend((s.groups.len() as u32).to_le_bytes());
            for g in &s.groups {
                out.extend((g.len() as u32).to_le_bytes());
                for m in g {
                    put_str(&mut out, &m.mesh);
                    put_str(&mut out, &m.asset);
                    put_str(&mut out, &m.shader);
                    out.extend(m.values);
                    out.extend(m.tail);
                }
            }
        }
        out.extend((self.maps.len() as u32).to_le_bytes());
        for m in &self.maps {
            for v in m.bounds {
                out.extend(v.to_le_bytes());
            }
            for v in m.header {
                out.extend(v.to_le_bytes());
            }
            for t in &m.tiles {
                for v in t {
                    out.extend(v.to_le_bytes());
                }
            }
            for f in &m.flags {
                out.extend(f);
            }
            out.extend(m.shader_side.to_le_bytes());
            out.extend(&m.shader);
            put_str(&mut out, &m.heightfield);
            put_str(&mut out, &m.atlas);
        }
        out
    }

    /// Renames every name that carries terrain `from`'s to terrain `to`'s (TGEN-REQ-4): the patch and undergrowth
    /// mask shader entries, the undergrowth entries' cache shaders, the patch maps' height fields and atlas, where
    /// they start `Terrains/<from>/` (any case); other names (shared undergrowth meshes) are kept. Returns how many
    /// names changed.
    pub fn rename_terrain(&mut self, from: &str, to: &str) -> usize {
        let mut changed = 0;
        let mut ren = |s: &mut String| {
            if let Some(r) = rename(s, from, to) {
                *s = r;
                changed += 1;
            }
        };
        for s in &mut self.shaders {
            ren(&mut s.name);
            ren(&mut s.undergrowth);
            for g in &mut s.groups {
                for m in g {
                    ren(&mut m.shader);
                }
            }
        }
        for m in &mut self.maps {
            ren(&mut m.heightfield);
            ren(&mut m.atlas);
        }
        changed
    }
}

/// `name` with terrain `from`'s prefix (`Terrains/<from>/`, any case) and every later `/<from>/` or `/<from>.`
/// segment (`Terrains/MP_005/MP_005/PatchMaskShaders/...`, `Terrains/MP_005/MP_005.MaskTileAtlas`) named `to`;
/// `None` when the name is not the terrain's.
#[must_use]
pub fn rename(name: &str, from: &str, to: &str) -> Option<String> {
    let low = name.to_ascii_lowercase();
    let pre = format!("terrains/{}/", from.to_ascii_lowercase());
    if !low.starts_with(&pre) {
        return None;
    }
    let rest = name.get(pre.len()..)?;
    let f = from.to_ascii_lowercase();
    // the segment after the folder that is the terrain's own name (`MP_005/` or `MP_005.` or `MP_005-`)
    let rest = if rest.len() >= f.len()
        && rest.get(..f.len()).is_some_and(|s| s.eq_ignore_ascii_case(&f))
        && rest
            .as_bytes()
            .get(f.len())
            .is_some_and(|b| matches!(b, b'/' | b'.' | b'-'))
    {
        format!("{to}{}", rest.get(f.len()..).unwrap_or_default())
    } else {
        rest.to_owned()
    };
    Some(format!("Terrains/{to}/{rest}"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn renames_the_terrains_names_only() {
        assert_eq!(
            rename(
                "Terrains/MP_005/MP_005/PatchMaskShaders/Dunes_Destruction",
                "MP_005",
                "TG_A"
            )
            .as_deref(),
            Some("Terrains/TG_A/TG_A/PatchMaskShaders/Dunes_Destruction")
        );
        assert_eq!(
            rename("Terrains/mp_005/MP_005.MaskTileAtlas", "MP_005", "TG_A").as_deref(),
            Some("Terrains/TG_A/TG_A.MaskTileAtlas")
        );
        assert_eq!(
            rename("Terrains/MP_005/Shaders/MP5_UGCache", "MP_005", "TG_A").as_deref(),
            Some("Terrains/TG_A/Shaders/MP5_UGCache")
        );
        assert_eq!(rename("Terrains/MP_0055/Shaders/X", "MP_005", "TG_A"), None);
        assert_eq!(rename("vegetation/undergrowth/x", "MP_005", "TG_A"), None);
    }

    #[test]
    fn a_patch_shader_with_undergrowth_round_trips() {
        let mut vt = VisualTerrain {
            roads: vec![VisualTerrain::EMPTY_ROADS.to_vec(); 2],
            shaders: vec![PatchShader {
                name: "Terrains/MP_005/MP_005/PatchMaskShaders/Dunes".into(),
                undergrowth: "Terrains/MP_005/MP_005/UgMaskShaders/Dunes".into(),
                layers: 1,
                tiled: 0,
                groups: vec![vec![UgMesh {
                    mesh: "Terrains/MP_005/Undergrowth/mp05_DryGrass_Mesh".into(),
                    asset: String::new(),
                    shader: "Terrains/MP_005/Shaders/MP5_UGCache".into(),
                    values: [3; 48],
                    tail: [1, 0],
                }]],
            }],
            maps: vec![PatchMap::rule_only(
                [-512.0, 0.0, -512.0, 512.0, 500.0, 512.0],
                [66, 1, 15, 7],
                64,
                0,
                "Terrains/MP_005/MP_005.Heightfield-000",
                "Terrains/MP_005/MP_005.MaskTileAtlas",
            )],
        };
        let b = vt.encode();
        assert_eq!(VisualTerrain::decode(&b).unwrap(), vt);
        assert_eq!(vt.rename_terrain("MP_005", "TG_A"), 5);
        assert_eq!(
            vt.shaders[0].groups[0][0].mesh,
            "Terrains/MP_005/Undergrowth/mp05_DryGrass_Mesh"
        );
        assert_eq!(vt.maps[0].atlas, "Terrains/TG_A/TG_A.MaskTileAtlas");
        let mut short = b.clone();
        short.pop();
        assert!(VisualTerrain::decode(&short).is_err());
    }
}
