//! The mesh viewer's model in Rust (round GR3): `bfbc2-mesh-viewer\meshview.py`'s `Index`, `load_mesh`,
//! `stack_models` and `render` ported, so the map editor's asset preview shows a mesh as the mesh viewer read and drew
//! it - the same index (its cache shared, [`cache`]), the same parts, vertices, texture coordinates, indices, colour
//! textures and, for the side / top / front pictures, the same pixels ([`render`]). The Python stays the reference
//! (and `meshview.py --png` the picture command line); `tests/test_meshview_gr3.py`'s recorded answers and the
//! editor's `gr3` tests hold the two together.
//!
//! How a mesh is read (the console converter's `FORMAT.md` sections 3-4): `<mesh>_lod<N>_data.res` (`MeshData`) names
//! its parts and their vertex and index ranges; each part's vertex layout is in a level's Dx9 shader database
//! (`<mesh>_lod<N>/<part>/standard`); the mesh blueprint (`<mesh>.dbx`) gives each surface its shader, and the
//! database entry `<mesh>/<shader>` names the shader's textures. Positions are the rest positions.

#![allow(clippy::many_single_char_names, reason = "pixel and colour names: x, y, r, g, b")]

pub mod cache;
pub mod render;

use std::collections::HashMap;
use std::sync::LazyLock;

use bc2_formats::{dbx, meshdata};
use regex::Regex;

use crate::{meshconv, texcheck};

/// `meshview.COLOUR`: a colour texture's name ending.
static COLOUR: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?i)_(d|dm|cm|c|col|color|colour|diff|diffuse|alb)$").ok());
/// `meshview.NOT_COLOUR`: textures never drawn as the colour.
static NOT_COLOUR: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r"(?i)^textures/(sky|_general|_mask)/|dirtmask|_(n|nm|s|spec|m|mask|ao|cube|env)$").ok()
});
/// The surfaces of a mesh blueprint's XML (`mesh_surfaces`' pattern).
static SURFACE: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r#"<field name="Name">([^<]*)</field>\s*<field ref="([^"]+)/[0-9a-fA-F-]{36}" name="Shader""#).ok()
});
/// `_lod\d+_data\.res$`.
static LOD: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"_lod(\d+)_data\.res$").ok());

/// `texture.FORMATS`' keys: the PC formats the picture reads.
const FORMATS: [u32; 11] = [0, 18, 1, 2, 19, 20, 3, 13, 5, 10, 9];

fn is(r: &LazyLock<Option<Regex>>, s: &str) -> bool {
    r.as_ref().is_some_and(|r| r.is_match(s))
}

/// The index's three tables, in their dicts' order.
#[derive(Debug, Default, Clone)]
pub struct Tables {
    /// (entry name lower case, bundle, entry name as the bundle holds it).
    pub where_: Vec<(String, String, String)>,
    /// (mesh part name lower case, its vertex layout record after the GUID).
    pub parts: Vec<(String, Vec<u8>)>,
    /// (shader database entry name lower case, its texture names).
    pub entries: Vec<(String, Vec<String>)>,
}

/// `meshview.Index`: which bundle holds each entry (the first stock bundle), each mesh part's vertex layout and each
/// shader database entry's texture names, every level's Dx9 database read.
#[derive(Debug, Default, Clone)]
pub struct Index {
    where_: HashMap<String, (String, String)>,
    order: Vec<String>,
    parts: HashMap<String, Vec<u8>>,
    entries: Vec<(String, Vec<String>)>,
    entry_at: HashMap<String, usize>,
}

impl Index {
    /// The index from its tables.
    #[must_use]
    pub fn from_tables(t: Tables) -> Self {
        let mut ix = Self::default();
        for (k, b, n) in t.where_ {
            if !ix.where_.contains_key(&k) {
                ix.order.push(k.clone());
                ix.where_.insert(k, (b, n));
            }
        }
        ix.parts = t.parts.into_iter().collect();
        for (k, tex) in t.entries {
            if !ix.entry_at.contains_key(&k) {
                ix.entry_at.insert(k.clone(), ix.entries.len());
                ix.entries.push((k, tex));
            }
        }
        ix
    }

    /// The game copy's index at `root` (the OPFOR folder): its cache, or built and cached ([`cache::load`]).
    ///
    /// # Errors
    /// As [`cache::load`].
    pub fn load(root: &str, say: &mut dyn FnMut(&str)) -> Result<Self, bc2_gamefs::meshindex::LoadError> {
        cache::load(root, say).map(Self::from_tables)
    }

    /// Entries indexed.
    #[must_use]
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Nothing indexed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// `where.items()` in the dict's order: (entry name lower case, bundle, entry name as the bundle holds it).
    pub fn rows(&self) -> impl Iterator<Item = (&str, &str, &str)> {
        self.order
            .iter()
            .filter_map(|k| self.where_.get(k).map(|(b, n)| (k.as_str(), b.as_str(), n.as_str())))
    }

    /// `key in where` (the key as the index holds it: lower case).
    #[must_use]
    pub fn contains(&self, key: &str) -> bool {
        self.where_.contains_key(key)
    }

    /// `sorted(parts)`: the mesh part names (lower case), sorted.
    #[must_use]
    pub fn part_names(&self) -> Vec<&str> {
        let mut v: Vec<&str> = self.parts.keys().map(String::as_str).collect();
        v.sort_unstable();
        v
    }

    /// The bundle and entry name of an entry (any case).
    #[must_use]
    pub fn locate(&self, name: &str) -> Option<(&str, &str)> {
        self.where_
            .get(&name.to_lowercase())
            .map(|(b, n)| (b.as_str(), n.as_str()))
    }

    /// `Index.read(name)`: the entry's bytes from its bundle; `None` when no stock bundle has it or the bundle does
    /// not read.
    #[must_use]
    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        let (b, n) = self.locate(name)?;
        bc2_gamefs::bundles::read_path(b, &[n]).ok()?.into_iter().next()
    }

    /// Several entries, each bundle read once for all of its names (on all cores): `None` for a name no stock
    /// bundle has or whose bundle does not read.
    #[must_use]
    pub fn read_many(&self, names: &[String]) -> HashMap<String, Option<Vec<u8>>> {
        let mut by: Vec<(String, Vec<(String, String)>)> = Vec::new();
        let mut out = HashMap::new();
        for n in names {
            match self.locate(n) {
                Some((b, e)) => match by.iter_mut().find(|(x, _)| x == b) {
                    Some((_, v)) => v.push((n.clone(), e.to_owned())),
                    None => by.push((b.to_owned(), vec![(n.clone(), e.to_owned())])),
                },
                None => {
                    out.insert(n.clone(), None);
                }
            }
        }
        let items: Vec<(String, Vec<String>)> = by
            .iter()
            .map(|(b, v)| (b.clone(), v.iter().map(|(_, e)| e.clone()).collect()))
            .collect();
        for ((_, wanted), got) in by.iter().zip(bc2_gamefs::bundles::read_many(&items)) {
            match got {
                Ok(datas) => {
                    for ((n, _), d) in wanted.iter().zip(datas) {
                        out.insert(n.clone(), Some(d));
                    }
                }
                Err(_) => {
                    for (n, _) in wanted {
                        out.insert(n.clone(), None);
                    }
                }
            }
        }
        out
    }

    /// A mesh part's vertex layout record (`<mesh>_lod<N>/<part>/standard`, any case).
    #[must_use]
    pub fn layout(&self, key: &str) -> Option<&[u8]> {
        self.parts.get(&key.to_lowercase()).map(Vec::as_slice)
    }

    /// A shader database entry's texture names (`<mesh>/<shader>`, any case); empty for none.
    #[must_use]
    pub fn textures(&self, entry: &str) -> &[String] {
        self.entry_at
            .get(&entry.to_lowercase())
            .and_then(|&i| self.entries.get(i))
            .map_or(&[], |(_, t)| t.as_slice())
    }

    /// `meshview.py find <text>`'s lines: the mesh names containing `text` (lower case), sorted, as the bundles
    /// spell them where LOD 0 is there.
    #[must_use]
    pub fn find(&self, text: &str) -> Vec<String> {
        let text = text.to_lowercase();
        let mut hits: Vec<String> = self
            .order
            .iter()
            .filter(|n| n.ends_with("_data.res") && n.contains(&text))
            .map(|n| match LOD.as_ref().and_then(|r| r.find(n)) {
                Some(m) => n.get(..m.start()).unwrap_or(n).to_owned(),
                None => n.clone(),
            })
            .collect();
        hits.sort();
        hits.dedup();
        hits.into_iter()
            .map(|h| match self.where_.get(&format!("{h}_lod0_data.res")) {
                Some((_, name)) => name.get(..name.len().saturating_sub(14)).unwrap_or(name).to_owned(),
                None => h,
            })
            .collect()
    }

    /// The levels of detail a mesh has (`<mesh>_lod<N>_data.res` indexed), in order.
    #[must_use]
    pub fn lods(&self, mesh: &str) -> Vec<usize> {
        let low = mesh.to_lowercase();
        (0..8)
            .filter(|k| self.where_.contains_key(&format!("{low}_lod{k}_data.res")))
            .collect()
    }

    /// The mesh name as the bundles spell it (`_clean` and the case `main` takes from the index).
    #[must_use]
    pub fn mesh_name(&self, given: &str, lod: usize) -> String {
        let name = clean(given);
        match self.where_.get(&format!("{name}_lod{lod}_data.res").to_lowercase()) {
            Some((_, n)) => n
                .get(..n.len().saturating_sub(format!("_lod{lod}_data.res").len()))
                .unwrap_or(&name)
                .to_owned(),
            None => name,
        }
    }
}

/// `_clean(name)`: `/` for `\`, the `_lod<N>_data` and `.res` / `.dbx` endings dropped.
#[must_use]
pub fn clean(name: &str) -> String {
    static END: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"(?i)(_lod\d+_data)?\.(res|dbx)$").ok());
    let n = name.replace('\\', "/");
    match END.as_ref() {
        Some(r) => r.replace(&n, "").into_owned(),
        None => n,
    }
}

/// An RGB picture, row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// Width.
    pub w: usize,
    /// Height.
    pub h: usize,
    /// 3 bytes a pixel.
    pub rgb: Vec<u8>,
}

impl Image {
    /// A pixel (clamped into the picture).
    #[must_use]
    pub fn at(&self, x: usize, y: usize) -> [u8; 3] {
        let i = (y.min(self.h.saturating_sub(1)) * self.w + x.min(self.w.saturating_sub(1))) * 3;
        match self.rgb.get(i..i + 3) {
            Some(&[r, g, b]) => [r, g, b],
            _ => [0; 3],
        }
    }
}

/// `mesh_surfaces(ix, name)`: (surface name lower case, shader asset path), in the dict's order.
#[must_use]
pub fn mesh_surfaces(ix: &Index, name: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let put = |out: &mut Vec<(String, String)>, k: String, v: String| match out.iter_mut().find(|(x, _)| *x == k) {
        Some(slot) => slot.1 = v,
        None => out.push((k, v)),
    };
    if let Some(b) = ix.read(&format!("{name}.dbx"))
        && let Ok(x) = dbx::to_xml(&b)
        && let Some(r) = SURFACE.as_ref()
    {
        for c in r.captures_iter(&x) {
            if let (Some(n), Some(s)) = (c.get(1), c.get(2)) {
                put(&mut out, n.as_str().to_lowercase(), s.as_str().to_owned());
            }
        }
    }
    if out.is_empty() {
        let pre = format!("{}/", name.to_lowercase());
        for (e, _) in &ix.entries {
            if let Some(rest) = e.strip_prefix(&pre) {
                put(
                    &mut out,
                    rest.rsplit('/').next().unwrap_or(rest).to_owned(),
                    rest.to_owned(),
                );
            }
        }
    }
    out
}

/// What a texture is for, from its name (DICE's endings): the asset preview's channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Channel {
    /// The colour (what the mesh viewer draws).
    Colour,
    /// A normal map.
    Normal,
    /// Specular / gloss.
    Specular,
    /// A mask (channel masks, dirt masks).
    Mask,
    /// Ambient occlusion.
    Occlusion,
    /// A tiling detail layer (`Dt_`).
    Detail,
    /// A cube or environment map.
    Environment,
    /// Anything else.
    Other,
}

impl Channel {
    /// Every channel, in the preview's order.
    pub const ALL: [Self; 8] = [
        Self::Colour,
        Self::Normal,
        Self::Specular,
        Self::Mask,
        Self::Occlusion,
        Self::Detail,
        Self::Environment,
        Self::Other,
    ];

    /// Its name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Colour => "Colour",
            Self::Normal => "Normal",
            Self::Specular => "Specular",
            Self::Mask => "Mask",
            Self::Occlusion => "Occlusion",
            Self::Detail => "Detail",
            Self::Environment => "Environment",
            Self::Other => "Other",
        }
    }

    /// A texture's channel by its name.
    #[must_use]
    pub fn of(texture: &str) -> Self {
        let leaf = texture.rsplit('/').next().unwrap_or(texture).to_lowercase();
        let ends = |s: &[&str]| s.iter().any(|e| leaf.ends_with(e));
        if ends(&["_n", "_nm", "_normal", "_nrm"]) || leaf.starts_with("nm_") {
            Self::Normal
        } else if ends(&["_s", "_spec", "_specular", "_g", "_gloss"]) {
            Self::Specular
        } else if ends(&["_cube", "_env"]) || leaf.contains("envmap") || leaf.contains("cube") {
            Self::Environment
        } else if ends(&["_ao"]) || leaf.contains("_oc") {
            Self::Occlusion
        } else if leaf.starts_with("dt_") {
            Self::Detail
        } else if ends(&["_m", "_mask"]) || leaf.contains("mask") || leaf.starts_with("msk_") {
            Self::Mask
        } else if is(&COLOUR, &leaf) {
            Self::Colour
        } else {
            Self::Other
        }
    }
}

/// One texture resource decoded at its first level as `_texture` decodes it (`texture.levels`, `texture.decode`):
/// `None` for a format the picture does not read.
#[must_use]
pub fn decode_texture(b: &[u8]) -> Option<Image> {
    let fmt = u32::from_le_bytes(b.get(8..12)?.try_into().ok()?);
    let lv = texcheck::levels(b)?;
    let (_, _, w, h, range) = lv.first()?.clone();
    if !FORMATS.contains(&fmt) {
        return None;
    }
    let data = b.get(range)?;
    let (rows, cols, px) = texcheck::decode(data, i64::from(w), i64::from(h), fmt)?;
    let mut rgb = Vec::with_capacity(rows * cols * 3);
    for p in px {
        rgb.extend(p.map(|v| v.clamp(0, 255) as u8));
    }
    Some(Image { w: cols, h: rows, rgb })
}

/// `_texture(ix, mesh, shader)`'s order: the colour-named textures of the entry, then the others not excluded.
#[must_use]
pub fn colour_candidates(ix: &Index, mesh: &str, shader: &str) -> Vec<String> {
    let tex = ix.textures(&format!("{mesh}/{shader}"));
    let mut pick: Vec<String> = tex
        .iter()
        .filter(|t| is(&COLOUR, t) && !is(&NOT_COLOUR, t))
        .cloned()
        .collect();
    for t in tex {
        if !is(&NOT_COLOUR, t) && !pick.contains(t) {
            pick.push(t.clone());
        }
    }
    pick
}

/// `_texture(ix, mesh, shader)`: the colour map's name and picture, or `None`.
#[must_use]
pub fn colour_texture(ix: &Index, mesh: &str, shader: &str) -> Option<(String, Image)> {
    colour_candidates(ix, mesh, shader).into_iter().find_map(|t| {
        let b = ix.read(&format!("{t}.res"))?;
        decode_texture(&b).map(|img| (t, img))
    })
}

/// One part of a mesh's level of detail, as `load_mesh` reads it.
#[derive(Debug, Clone)]
pub struct Part {
    /// The part's name.
    pub name: String,
    /// The surface it was matched to (lower case), if any.
    pub surface: Option<String>,
    /// That surface's shader (the database entry `<mesh>/<shader>`), if any.
    pub shader: Option<String>,
    /// The vertices its triangles use, from the lowest index to the highest.
    pub positions: Vec<[f32; 3]>,
    /// Normals, where the layout has a readable one.
    pub normals: Option<Vec<[f32; 3]>>,
    /// Texture coordinates, where the layout has them.
    pub uvs: Option<Vec<[f32; 2]>>,
    /// Triangles, into [`Part::positions`].
    pub indices: Vec<[u32; 3]>,
    /// The colour texture's name.
    pub texture: Option<String>,
    /// The colour texture.
    pub image: Option<std::sync::Arc<Image>>,
}

/// A mesh's level of detail read: its parts and what was skipped, in words.
#[derive(Debug, Clone, Default)]
pub struct Mesh {
    /// The mesh name as the bundles spell it.
    pub name: String,
    /// Its level of detail.
    pub lod: usize,
    /// The parts.
    pub parts: Vec<Part>,
    /// The parts skipped and why (`load_mesh`'s stderr lines).
    pub notes: Vec<String>,
}

/// The values of one element (`_values`' formats: 3 / 4 floats, 1 - 4 halves).
fn element_values(block: &[u8], n: usize, stride: usize, f: u8, off: usize) -> Option<(Vec<f32>, usize)> {
    let (halves, cols, size) = match f {
        3 => (false, 3, 12),
        4 => (false, 4, 16),
        5 => (true, 1, 2),
        6 => (true, 2, 4),
        7 => (true, 3, 6),
        8 => (true, 4, 8),
        _ => return None,
    };
    if off + size > stride {
        return None;
    }
    let mut v = Vec::with_capacity(n * cols);
    for r in 0..n {
        let e = block.get(r * stride + off..r * stride + off + size)?;
        if halves {
            for c in e.as_chunks::<2>().0 {
                v.push(half(u16::from_le_bytes(*c)));
            }
        } else {
            v.extend(e.as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)));
        }
    }
    Some((v, cols))
}

/// `float32` of an IEEE half (NumPy's `astype(np.float32)` of `<f2`).
fn half(h: u16) -> f32 {
    let sign = u32::from(h >> 15) << 31;
    let exp = u32::from((h >> 10) & 0x1f);
    let mant = u32::from(h & 0x3ff);
    let bits = match exp {
        0x1f => sign | 0xff << 23 | mant << 13,
        0 if mant == 0 => sign,
        0 => sign | (f32::from(mant as u16) * f32::powi(2.0, -24)).to_bits(),
        e => sign | (e + 112) << 23 | mant << 13,
    };
    f32::from_bits(bits)
}

/// The usages a part's vertices hold: usage -> (values, columns).
type Attributes = HashMap<u8, (Vec<f32>, usize)>;

/// `_read_part`: the part's triangles (indices less the lowest) and its vertex values (each usage's first element of
/// stream 0, the texture coordinates' element `uv` for usage 0); `None` where the data is cut short.
#[allow(clippy::too_many_arguments, reason = "load_mesh's values, as it holds them")]
fn read_part(
    b: &[u8],
    vbase: usize,
    ibase: usize,
    wide: bool,
    part: &meshdata::MeshPart,
    els: &[meshconv::Element],
    stride: usize,
    uv: Option<usize>,
) -> Option<(Vec<[u32; 3]>, usize, Attributes)> {
    let [triangles, _, first_index, vertex_offset] = part.counts.map(|c| c as usize);
    let isz = if wide { 4 } else { 2 };
    let start = ibase.checked_add(first_index.checked_mul(isz)?)?;
    let raw = b.get(start..start.checked_add(triangles.checked_mul(3 * isz)?)?)?;
    let idx: Vec<u32> = if wide {
        raw.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c)).collect()
    } else {
        raw.as_chunks::<2>()
            .0
            .iter()
            .map(|c| u32::from(u16::from_le_bytes(*c)))
            .collect()
    };
    let lo = *idx.iter().min()?;
    let hi = *idx.iter().max()?;
    let n = (hi - lo) as usize + 1;
    let vstart = vbase
        .checked_add(vertex_offset)?
        .checked_add((lo as usize).checked_mul(stride)?)?;
    let block = b.get(vstart..vstart.checked_add(n.checked_mul(stride)?)?)?;
    let mut attr: Attributes = HashMap::new();
    for (i, &(f, usage, off, stream)) in els.iter().enumerate() {
        if stream != 0 || (usage == 0 && Some(i) != uv) || attr.contains_key(&usage) {
            continue;
        }
        if let Some(v) = element_values(block, n, stride, f, usize::from(off)) {
            attr.insert(usage, v);
        }
    }
    let tris = idx.as_chunks::<3>().0.iter().map(|t| t.map(|i| i - lo)).collect();
    Some((tris, n, attr))
}

/// The first `k` columns of each row of a usage's values (zeros past its columns).
fn columns<const K: usize>(v: &(Vec<f32>, usize), n: usize) -> Vec<[f32; K]> {
    let (vals, cols) = v;
    (0..n)
        .map(|r| {
            std::array::from_fn(|c| {
                if c < *cols {
                    vals.get(r * cols + c).copied().unwrap_or(0.0)
                } else {
                    0.0
                }
            })
        })
        .collect()
}

/// `load_mesh(ix, name, lod)`: the mesh's parts at a level of detail with their colour textures. Depth-only copies
/// (`_zonly`) are left out; a part without a vertex layout in any level's database, or without positions, is skipped
/// with a note.
///
/// # Errors
/// No stock bundle has the level's data, or it does not parse.
pub fn load_mesh(ix: &Index, name: &str, lod: usize) -> Result<Mesh, String> {
    let data_name = format!("{name}_lod{lod}_data.res");
    let b = ix
        .read(&data_name)
        .ok_or_else(|| format!("{data_name}: in no stock bundle (meshview.py find <text> lists mesh names)"))?;
    let m = meshdata::parse_mesh(&b, false).map_err(|e| format!("{data_name}: {e:?}"))?;
    let surfaces = mesh_surfaces(ix, name);
    let vbase = m.data;
    let ibase = vbase + m.sizes[0] as usize;
    let wide = m.sizes[2] != 0;
    let mut out = Mesh {
        name: name.to_owned(),
        lod,
        ..Mesh::default()
    };
    let mut textures: HashMap<String, Option<(String, std::sync::Arc<Image>)>> = HashMap::new();
    for p in &m.parts {
        let pname: String = p.name.iter().map(|&c| char::from(c)).collect();
        let low = pname.to_lowercase();
        if low.ends_with("_zonly") {
            continue;
        }
        let Some(rec) = ix.layout(&format!("{name}_lod{lod}/{pname}/standard")) else {
            out.notes.push(format!(
                "  {pname}: no vertex layout in any level's shader database - skipped"
            ));
            continue;
        };
        let Some((els, stride)) = meshconv::elements(rec) else {
            out.notes
                .push(format!("  {pname}: its vertex layout is cut short - skipped"));
            continue;
        };
        let uv = meshconv::texcoord_element(&els);
        let Some((indices, n, attr)) = read_part(&b, vbase, ibase, wide, p, &els, stride, uv) else {
            out.notes
                .push(format!("  {pname}: its vertices or indices are cut short - skipped"));
            continue;
        };
        let Some(pos) = attr.get(&1) else {
            out.notes.push(format!("  {pname}: no position element - skipped"));
            continue;
        };
        // the longest surface name the part's name ends with (the first of equals: max's)
        let mut surf: Option<&(String, String)> = None;
        for s in surfaces.iter().filter(|s| low.ends_with(s.0.as_str())) {
            if surf.is_none_or(|b| s.0.len() > b.0.len()) {
                surf = Some(s);
            }
        }
        let tex = surf.and_then(|(_, shader)| {
            textures
                .entry(shader.clone())
                .or_insert_with(|| colour_texture(ix, name, shader).map(|(t, i)| (t, std::sync::Arc::new(i))))
                .clone()
        });
        out.parts.push(Part {
            name: pname,
            surface: surf.map(|s| s.0.clone()),
            shader: surf.map(|s| s.1.clone()),
            positions: columns::<3>(pos, n),
            normals: attr.get(&4).map(|v| columns::<3>(v, n)),
            uvs: attr.get(&0).map(|v| columns::<2>(v, n)),
            indices,
            texture: tex.as_ref().map(|t| t.0.clone()),
            image: tex.map(|t| t.1),
        });
    }
    Ok(out)
}

/// A blueprint's `Mesh` reference (`path/guid` -> path), from its partition's XML (`bc2_render::world::mesh_of`'s
/// rule).
#[must_use]
pub fn blueprint_mesh(xml: &str) -> Option<String> {
    let at = xml.find("name=\"Mesh\"")?;
    let head = xml.get(..at)?;
    let r = head.get(head.rfind("ref=\"")? + 5..)?;
    let r = r.get(..r.find('"')?)?;
    (r != "null").then(|| r.get(..r.rfind('/')?).map(str::to_owned))?
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn names_and_channels() {
        assert_eq!(clean("Objects\\A\\B_Mesh_lod0_data.res"), "Objects/A/B_Mesh");
        assert_eq!(clean("a/b.dbx"), "a/b");
        assert_eq!(Channel::of("textures/x/rock_d"), Channel::Colour);
        assert_eq!(Channel::of("textures/x/rock_n"), Channel::Normal);
        assert_eq!(Channel::of("textures/x/rock_s"), Channel::Specular);
        assert_eq!(Channel::of("t/Dt_Planks_Old_001"), Channel::Detail);
        assert_eq!(Channel::of("t/GuardTower_Oc_1"), Channel::Occlusion);
        assert_eq!(Channel::of("t/x_mask"), Channel::Mask);
        assert!(is(&COLOUR, "a/US_Garand_CM") && !is(&NOT_COLOUR, "a/US_Garand_CM"));
        assert!(is(&NOT_COLOUR, "textures/sky/x_d"));
        assert_eq!(
            blueprint_mesh(r#"<field ref="objects/a/a_mesh/0123" name="Mesh"/>"#).as_deref(),
            Some("objects/a/a_mesh")
        );
        assert_eq!(blueprint_mesh(r#"<field ref="null" name="Mesh"/>"#), None);
    }

    #[test]
    fn halves_as_numpy() {
        assert!((half(0x3c00) - 1.0).abs() < f32::EPSILON);
        assert!((half(0xc000) + 2.0).abs() < f32::EPSILON);
        assert!(half(0x0001) > 0.0);
        assert!(half(0x7c00).is_infinite());
        assert!(half(0x7e00).is_nan());
    }

    #[test]
    fn find_and_lods() {
        let t = Tables {
            where_: vec![
                (
                    "a/b_mesh_lod0_data.res".into(),
                    "x.fbrb".into(),
                    "A/B_Mesh_lod0_data.res".into(),
                ),
                (
                    "a/b_mesh_lod1_data.res".into(),
                    "x.fbrb".into(),
                    "A/B_Mesh_lod1_data.res".into(),
                ),
                (
                    "a/c_mesh_lod1_data.res".into(),
                    "x.fbrb".into(),
                    "A/C_Mesh_lod1_data.res".into(),
                ),
            ],
            parts: vec![],
            entries: vec![("a/b_mesh/s".into(), vec!["t/x_d".into()])],
        };
        let ix = Index::from_tables(t);
        assert_eq!(ix.find("mesh"), vec!["A/B_Mesh", "a/c_mesh"]);
        assert_eq!(ix.lods("A/B_Mesh"), vec![0, 1]);
        assert_eq!(ix.mesh_name("a\\b_mesh.dbx", 0), "A/B_Mesh");
        assert_eq!(ix.textures("A/B_Mesh/S"), &["t/x_d".to_owned()]);
    }
}
