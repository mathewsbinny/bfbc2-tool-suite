//! The terrain's visual paint and its colour / occlusion map written from a project's strokes (round E5-rest; spec
//! `transforms/terrain_edit.md` TEDIT-VT-1 / -2, TEDIT-ATL-1 / -2, TEDIT-SH-1 - 3, TEDIT-OCC-1, TEDIT-MM-2).
//!
//! **The layers** ([`LayerModel`]): a terrain's patch shaders are named by the layers they blend
//! (`Terrains/MP_005/MP_005/PatchMaskShaders/Dunes_Asphalt_DryGrass_Destruction`), each layer either a rule layer
//! (computed in the shader from slope and height: no tile) or a tiled layer (a mask tile of the atlas), and a
//! shader's tiled count is how many of its layers are tiled. A layer's name may hold underscores, so the names are
//! split by the layer counts the visual terrain stores. A cell's tile slot k is the k-th tiled layer in the name's
//! order (TEDIT-VT-2: on DICE's terrains where every layer has one kind, each aligned block names one tile per layer
//! that way, and not reversed).
//!
//! **The paint** ([`paint`]): a visual paint layer is a raster of weights over the height field's samples (the
//! project's own [`crate::files::Target::Paint`]). Each 16 m patch cell the paint reaches takes the layer: its new
//! layer set's patch shader (the terrain's own; else one with more tiled layers whose extra masks are an empty tile;
//! else a clone of a shader of the same kinds with one tiled layer's textures retargeted, TEDIT-SH-1 / -2); every
//! aligned block of flag x flag cells it touches gets new mask tiles (the old masks resampled, the paint laid over
//! them) in atlas slots no other cell names (TEDIT-ATL-1 / -2), the cells' flags their shader's tiled count
//! (TEDIT-VT-1). Everything else keeps DICE's bytes ([`bc2_formats::visual_terrain`] writes them back exactly).
//!
//! **The colour / occlusion map** ([`colour_raster`], [`write_colour`], [`reshade`]): the map's level-0 texels as a
//! raster, painted by the editor or re-shaded after a sculpt by the ratio of new to old sky visibility (LIGHT-BAKE-6,
//! `bc2_terragen::visual::sky_visibility_over`); written back with [`bc2_formats::itexture::splice`], so only the
//! blocks under the edit (and the mip texels below them) change.
//!
//! **The pairing** ([`pairing`]): which physics material lies under each tiled layer where its mask is strong, from
//! the terrain's own material map: the editor's visual / material pairing (TEDIT-MM-2).

use std::collections::{BTreeMap, BTreeSet};

use bc2_formats::itexture::{self, Format, Picture, Rect};
use bc2_formats::terrain_codec::MaterialMap;
use bc2_formats::visual_terrain::{PatchShader, VisualTerrain};

use crate::delta::Raster;
use crate::write::LevelWriteError;

fn refused(why: impl Into<String>) -> LevelWriteError {
    LevelWriteError::Refused(why.into())
}

// ------------------------------------------------------------------------------------------------ the layers

/// A terrain's paint layers, from its patch shaders' names (the module's "The layers").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerModel {
    /// The layers, in DICE's drawing order (every shader's name lists its layers in this order).
    pub layers: Vec<String>,
    /// Per layer, whether it is tiled (a mask tile) rather than a rule layer.
    pub tiled: Vec<bool>,
    /// Per patch shader, its layers (indices into `layers`) in its name's order.
    pub shaders: Vec<Vec<usize>>,
    /// The patch shaders' common prefix (`Terrains/<T>/<T>/PatchMaskShaders/`).
    pub prefix: String,
}

/// Every split of `t` into `l` runs of tokens joined by `_`.
fn splits(t: &[&str], l: usize, cur: &mut Vec<String>, all: &mut Vec<Vec<String>>) {
    if l == 1 {
        cur.push(t.join("_"));
        all.push(cur.clone());
        cur.pop();
        return;
    }
    for i in 1..=t.len().saturating_sub(l - 1) {
        cur.push(t.get(..i).unwrap_or_default().join("_"));
        splits(t.get(i..).unwrap_or_default(), l - 1, cur, all);
        cur.pop();
    }
}

/// Each name (the part after the prefix) split into its `layers` layer names: a name of as many tokens as layers is
/// split at every `_`; the others take the one split that names at most one layer not seen yet, repeated until
/// nothing more splits. `None` for a name no split settles.
#[must_use]
pub fn segment(names: &[(String, usize)]) -> Vec<Option<Vec<String>>> {
    let toks: Vec<Vec<&str>> = names.iter().map(|(n, _)| n.split('_').collect()).collect();
    let mut vocab: BTreeSet<String> = BTreeSet::new();
    let mut out: Vec<Option<Vec<String>>> = vec![None; names.len()];
    loop {
        let mut progress = false;
        for (k, (_, l)) in names.iter().enumerate() {
            let Some(t) = toks.get(k) else { continue };
            if out.get(k).is_none_or(Option::is_some) || *l == 0 || t.len() < *l || t.len() > 24 {
                continue;
            }
            let mut all = Vec::new();
            splits(t, *l, &mut Vec::new(), &mut all);
            let mut scored: Vec<(usize, Vec<String>)> = all
                .into_iter()
                .map(|a| (a.iter().filter(|x| !vocab.contains(*x)).count(), a))
                .collect();
            scored.sort();
            let Some((best, first)) = scored.first().cloned() else {
                continue;
            };
            let ties = scored.iter().filter(|s| s.0 == best).count();
            if ties == 1 && (best <= 1 || scored.len() == 1) {
                vocab.extend(first.iter().cloned());
                if let Some(slot) = out.get_mut(k) {
                    *slot = Some(first);
                }
                progress = true;
            }
        }
        if !progress {
            break;
        }
    }
    out
}

impl LayerModel {
    /// The layers of a visual terrain's patch shaders.
    ///
    /// # Errors
    /// A name no split settles, a layer that is tiled in one shader and a rule layer in another (TEDIT-VT-2: the
    /// terrain is not painted), or two names that list two layers in both orders.
    pub fn of(vt: &VisualTerrain) -> Result<Self, String> {
        let first = vt.shaders.first().ok_or("the visual terrain has no patch shader")?;
        let prefix = first
            .name
            .rsplit_once('/')
            .map(|(p, _)| format!("{p}/"))
            .ok_or("a patch shader name has no folder")?;
        let names: Vec<(String, usize)> = vt
            .shaders
            .iter()
            .map(|s| {
                (
                    s.name.rsplit('/').next().unwrap_or_default().to_owned(),
                    s.layers as usize,
                )
            })
            .collect();
        let segs = segment(&names);
        let unsplit: Vec<&str> = names
            .iter()
            .zip(&segs)
            .filter(|(_, s)| s.is_none())
            .map(|(n, _)| n.0.as_str())
            .collect();
        if !unsplit.is_empty() {
            return Err(format!(
                "{} of {} patch shader names do not split into their layers (first: {})",
                unsplit.len(),
                names.len(),
                unsplit.first().copied().unwrap_or_default()
            ));
        }
        let segs: Vec<Vec<String>> = segs.into_iter().flatten().collect();
        // the layers, each once, in first appearance; then DICE's order from the names' pairs
        let mut layers: Vec<String> = Vec::new();
        for s in &segs {
            for l in s {
                if !layers.contains(l) {
                    layers.push(l.clone());
                }
            }
        }
        let idx = |l: &String| layers.iter().position(|x| x == l).unwrap_or(0);
        let mut before: BTreeSet<(usize, usize)> = BTreeSet::new();
        for s in &segs {
            for (i, a) in s.iter().enumerate() {
                for b in s.iter().skip(i + 1) {
                    before.insert((idx(a), idx(b)));
                }
            }
        }
        if let Some((a, b)) = before.iter().find(|(a, b)| before.contains(&(*b, *a))) {
            return Err(format!(
                "layers {} and {} are named in both orders",
                layers.get(*a).map_or("", String::as_str),
                layers.get(*b).map_or("", String::as_str)
            ));
        }
        // Kahn's sort, ties by first appearance
        let n = layers.len();
        let mut order: Vec<usize> = Vec::with_capacity(n);
        let mut left: BTreeSet<usize> = (0..n).collect();
        while let Some(&next) = left
            .iter()
            .find(|&&c| !left.iter().any(|&o| o != c && before.contains(&(o, c))))
        {
            order.push(next);
            left.remove(&next);
        }
        let sorted: Vec<String> = order.iter().filter_map(|&k| layers.get(k).cloned()).collect();
        let at = |l: &String| sorted.iter().position(|x| x == l).unwrap_or(0);
        let shaders: Vec<Vec<usize>> = segs.iter().map(|s| s.iter().map(at).collect()).collect();
        // the kinds: a layer of a rule-only shader is a rule layer; every shader's tiled count must then be its
        // other layers'
        let mut tiled = vec![true; sorted.len()];
        for (s, ls) in vt.shaders.iter().zip(&shaders) {
            if s.tiled == 0 {
                for &l in ls {
                    if let Some(t) = tiled.get_mut(l) {
                        *t = false;
                    }
                }
            }
        }
        let wrong = vt
            .shaders
            .iter()
            .zip(&shaders)
            .filter(|(s, ls)| ls.iter().filter(|&&l| tiled.get(l) == Some(&true)).count() != s.tiled as usize)
            .count();
        if wrong > 0 {
            return Err(format!(
                "{wrong} of {} patch shaders have a tiled count their layers' kinds do not give (a layer tiled in one \
                 shader and a rule layer in another: TEDIT-VT-2)",
                vt.shaders.len()
            ));
        }
        Ok(Self {
            layers: sorted,
            tiled,
            shaders,
            prefix,
        })
    }

    /// A layer's index by name (any case).
    #[must_use]
    pub fn index(&self, name: &str) -> Option<usize> {
        self.layers.iter().position(|l| l.eq_ignore_ascii_case(name))
    }

    /// The tiled layers, the ones a paint can lay (in DICE's order).
    #[must_use]
    pub fn paintable(&self) -> Vec<&str> {
        self.layers
            .iter()
            .zip(&self.tiled)
            .filter(|(_, t)| **t)
            .map(|(l, _)| l.as_str())
            .collect()
    }

    /// A shader's tiled layers in slot order (TEDIT-VT-2).
    #[must_use]
    pub fn tiled_layers(&self, shader: usize) -> Vec<usize> {
        self.shaders.get(shader).map_or_else(Vec::new, |ls| {
            ls.iter()
                .copied()
                .filter(|&l| self.tiled.get(l) == Some(&true))
                .collect()
        })
    }

    /// The shader drawing exactly `set`.
    #[must_use]
    pub fn find(&self, set: &BTreeSet<usize>) -> Option<usize> {
        self.shaders
            .iter()
            .position(|ls| ls.len() == set.len() && ls.iter().all(|l| set.contains(l)))
    }

    /// The shader drawing `set` and the fewest other layers, all of them tiled (their masks left empty draw `set`
    /// exactly); `None` when there is none.
    #[must_use]
    pub fn find_superset(&self, set: &BTreeSet<usize>) -> Option<usize> {
        self.shaders
            .iter()
            .enumerate()
            .filter(|(_, ls)| {
                set.iter().all(|l| ls.contains(l))
                    && ls
                        .iter()
                        .all(|l| set.contains(l) || self.tiled.get(*l) == Some(&true))
            })
            .min_by_key(|(k, ls)| (ls.len(), *k))
            .map(|(k, _)| k)
    }

    /// A layer set's patch shader name, in DICE's order.
    #[must_use]
    pub fn name(&self, set: &BTreeSet<usize>) -> String {
        let parts: Vec<&str> = set
            .iter()
            .filter_map(|&l| self.layers.get(l).map(String::as_str))
            .collect();
        format!("{}{}", self.prefix, parts.join("_"))
    }
}

// ------------------------------------------------------------------------------------------------ the atlas

/// The mask-tile atlas (an L8 texture of one level, TEDIT-ATL-1) with its patch maps' layout.
#[derive(Debug, Clone)]
pub struct Atlas {
    /// The resource's bytes.
    pub bytes: Vec<u8>,
    width: usize,
    height: usize,
    per_row: usize,
    rows: usize,
    tile: usize,
    border: usize,
}

impl Atlas {
    /// The atlas and its layout (`header`: a patch map's tile size, border, tiles per row, rows).
    ///
    /// # Errors
    /// Not an L8 texture, or the layout does not fit it.
    pub fn of(bytes: &[u8], header: [u32; 5]) -> Result<Self, String> {
        let h = itexture::Header::read(bytes).ok_or("the mask atlas is not a PC texture")?;
        if h.format != Format::L8.code() {
            return Err(format!("the mask atlas is format {}, not L8 (10)", h.format));
        }
        let [tile, border, per_row, rows, _] = header.map(|v| v as usize);
        let (width, height) = (h.width as usize, h.height as usize);
        if per_row * tile > width || rows * tile > height || tile <= 2 * border || width % 4 != 0 {
            return Err(format!(
                "the mask atlas ({width} x {height}) does not hold {per_row} x {rows} tiles of {tile}"
            ));
        }
        if bytes.len() < itexture::HEADER + width * height {
            return Err("the mask atlas is short".into());
        }
        Ok(Self {
            bytes: bytes.to_vec(),
            width,
            height,
            per_row,
            rows,
            tile,
            border,
        })
    }

    /// Tiles it holds.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.per_row * self.rows
    }

    /// A tile's inside side (64 on DICE's).
    #[must_use]
    pub fn span(&self) -> usize {
        self.tile - 2 * self.border
    }

    fn at(&self, t: usize, x: usize, y: usize) -> usize {
        let (c, r) = (t % self.per_row.max(1), t / self.per_row.max(1));
        itexture::HEADER + (r * self.tile + y) * self.width + c * self.tile + x
    }

    /// Tile `t`'s texel (`x`, `y`) inside its border (clamped).
    #[must_use]
    pub fn get(&self, t: usize, x: usize, y: usize) -> u8 {
        let s = self.span();
        let (x, y) = (x.min(s - 1) + self.border, y.min(s - 1) + self.border);
        self.bytes.get(self.at(t, x, y)).copied().unwrap_or(0)
    }

    /// Writes tile `t`'s inside (`span` x `span`, row by row) and its border (each border texel the nearest inside
    /// texel: DICE's on 96 - 98 % of their border texels, TEDIT-ATL-1).
    pub fn put(&mut self, t: usize, inside: &[u8]) {
        let s = self.span();
        for y in 0..self.tile {
            for x in 0..self.tile {
                let (ix, iy) = (
                    x.saturating_sub(self.border).min(s - 1),
                    y.saturating_sub(self.border).min(s - 1),
                );
                let v = inside.get(iy * s + ix).copied().unwrap_or(0);
                let at = self.at(t, x, y);
                if let Some(b) = self.bytes.get_mut(at) {
                    *b = v;
                }
            }
        }
    }

    /// Whether it is the same size as another (the layout is the patch maps').
    #[must_use]
    pub fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }
}

// ------------------------------------------------------------------------------------------------ the paint

/// A patch shader the paint needs that the terrain lacks (TEDIT-SH-1): a clone of `donor`'s entry under `name` in
/// every database, `textures` renamed in it (the replaced layer's own textures to the painted layer's).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewShader {
    /// The new entry's name.
    pub name: String,
    /// The entry it is cloned from (the terrain's own).
    pub donor: String,
    /// (old, new) texture names renamed in the clone.
    pub textures: Vec<(String, String)>,
}

/// What [`paint`] wrote.
#[derive(Debug, Clone, Default)]
pub struct Painted {
    /// The visual terrain.
    pub visual: Vec<u8>,
    /// The mask-tile atlas.
    pub atlas: Vec<u8>,
    /// Patch shaders added to the visual terrain, to be cloned into every database.
    pub added: Vec<NewShader>,
    /// Cells whose shader, tiles or flags changed.
    pub cells: usize,
    /// Tiles written.
    pub tiles: usize,
    /// Cells drawn with a shader of more tiled layers than they paint (their extra masks the empty tile).
    pub padded: usize,
    /// Lines for the export's report.
    pub notes: Vec<String>,
}

/// The textures each entry of a database samples (`levels::scan`'s), by entry name: what [`paint`] needs to clone a
/// shader for a new combination (`None`: no clone is made, and a paint that needs one is refused).
pub type EntryTextures = BTreeMap<String, BTreeSet<String>>;

/// The textures of a Dx9 database's patch mask shader entries ([`EntryTextures`]).
///
/// # Errors
/// The database does not read.
pub fn entry_textures(db: &[u8]) -> Result<EntryTextures, String> {
    use bc2_formats::shaderdb::{levels, merge::latin, open};
    let o = open::open(db, Some(9), Some(false), "the Dx9 database")?;
    let scan = levels::scan(&o.db).map_err(|e| e.to_string())?;
    Ok(scan
        .iter()
        .map(|e| (latin(e.name), e))
        .filter(|(n, _)| n.to_ascii_lowercase().contains("/patchmaskshaders/"))
        .map(|(n, e)| (n, e.textures.iter().map(|t| latin(t.2)).collect()))
        .collect())
}

/// Each layer's own textures: those every entry of a shader with the layer samples and no entry without it does.
#[must_use]
pub fn layer_textures(model: &LayerModel, textures: &EntryTextures) -> Vec<BTreeSet<String>> {
    let tex_of = |k: usize| -> Option<&BTreeSet<String>> {
        let name = format!(
            "{}{}",
            model.prefix,
            model
                .shaders
                .get(k)?
                .iter()
                .filter_map(|&l| model.layers.get(l).map(String::as_str))
                .collect::<Vec<_>>()
                .join("_")
        );
        textures
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(&name))
            .map(|(_, t)| t)
    };
    (0..model.layers.len())
        .map(|l| {
            let mut with: Option<BTreeSet<String>> = None;
            let mut without: BTreeSet<String> = BTreeSet::new();
            for (k, ls) in model.shaders.iter().enumerate() {
                let Some(t) = tex_of(k) else { continue };
                if ls.contains(&l) {
                    with = Some(with.map_or_else(|| t.clone(), |w| w.intersection(t).cloned().collect()));
                } else {
                    without.extend(t.iter().cloned());
                }
            }
            with.map_or_else(BTreeSet::new, |w| w.difference(&without).cloned().collect())
        })
        .collect()
}

/// A clone for layer set `want` (TEDIT-SH-1 / -2): a shader of the terrain with the same layer count whose layers
/// differ from `want` in one tiled layer at the same place in the order (so its tiled slots line up), that layer's
/// own textures renamed to the wanted layer's (pairwise in name order; both must have some, as many).
fn clone_for(
    model: &LayerModel,
    want: &BTreeSet<usize>,
    own: &[BTreeSet<String>],
) -> Option<(usize, Vec<(String, String)>)> {
    let wv: Vec<usize> = want.iter().copied().collect();
    let mut best: Option<(usize, Vec<(String, String)>)> = None;
    for (k, ls) in model.shaders.iter().enumerate() {
        let mut sorted = ls.clone();
        sorted.sort_unstable();
        if sorted.len() != wv.len() {
            continue;
        }
        let diff: Vec<(usize, usize)> = sorted
            .iter()
            .zip(&wv)
            .filter(|(a, b)| a != b)
            .map(|(a, b)| (*a, *b))
            .collect();
        let [(old, new)] = diff.as_slice() else { continue };
        if model.tiled.get(*old) != Some(&true) || model.tiled.get(*new) != Some(&true) {
            continue;
        }
        let (Some(o), Some(n)) = (own.get(*old), own.get(*new)) else {
            continue;
        };
        if o.is_empty() || o.len() != n.len() {
            continue;
        }
        let pairs: Vec<(String, String)> = o.iter().cloned().zip(n.iter().cloned()).collect();
        if best.is_none() {
            best = Some((k, pairs));
        }
    }
    best
}

/// A painted layer: its name and its weights (0 - 255) over the height field's samples.
#[derive(Debug, Clone, Copy)]
pub struct PaintLayer<'a> {
    /// The layer (one of [`LayerModel::paintable`]).
    pub layer: &'a str,
    /// The weights, one per height-field sample.
    pub weights: &'a Raster,
}

/// The visual terrain and atlas with `paints` laid on the main patch map (the module's "The paint"). `hf_size` is
/// the main height field's samples a side; `textures` the level's Dx9 database's entry textures, for a combination
/// the terrain lacks (`None`: such a paint is refused). Nothing painted: the base's bytes.
///
/// # Errors
/// The files do not read; the terrain's layers are not one kind each (TEDIT-VT-2); a layer is not one of the
/// terrain's tiled layers; a combination has no shader and none to clone; the atlas has no room (TEDIT-ATL-1).
#[allow(
    clippy::too_many_lines,
    reason = "the paint's steps in order: coverage, shaders, tiles, slots, cells"
)]
pub fn paint(
    visual: &[u8],
    atlas: &[u8],
    hf_size: u32,
    paints: &[PaintLayer<'_>],
    textures: Option<&EntryTextures>,
) -> Result<Painted, LevelWriteError> {
    let mut vt = VisualTerrain::decode(visual).map_err(|e| refused(format!("the visual terrain: {e}")))?;
    let model = LayerModel::of(&vt).map_err(|e| refused(format!("the terrain's paint layers: {e}")))?;
    let map = vt.maps.first().ok_or_else(|| refused("the visual terrain has no patch map"))?;
    let mut at = Atlas::of(atlas, map.header).map_err(refused)?;
    let w = map.header[4] as usize;
    let n = hf_size as usize;
    let q = (n / w.max(1)).max(1);
    let span = at.span();
    let mut painted: Vec<(usize, &Raster)> = Vec::new();
    for p in paints {
        let l = model
            .index(p.layer)
            .ok_or_else(|| refused(format!("{} is not a layer of this terrain", p.layer)))?;
        if model.tiled.get(l) != Some(&true) {
            return Err(refused(format!(
                "{} is a rule layer (drawn from slope and height, TERR-VT-3): it is not painted",
                p.layer
            )));
        }
        if (p.weights.width(), p.weights.height()) != (hf_size, hf_size) {
            return Err(refused(format!(
                "the paint layer {} is {} x {}, the height field {n}",
                p.layer,
                p.weights.width(),
                p.weights.height()
            )));
        }
        painted.push((l, p.weights));
    }
    // 1. coverage: per cell, the layers the paint reaches in it
    let mut reach: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for &(l, r) in &painted {
        for cz in 0..w {
            for cx in 0..w {
                let hit = (cz * q..((cz + 1) * q).min(n))
                    .any(|j| (cx * q..((cx + 1) * q).min(n)).any(|i| r.get(i as u32, j as u32) > 0));
                if hit {
                    reach.entry(cz * w + cx).or_default().insert(l);
                }
            }
        }
    }
    if reach.is_empty() {
        return Ok(Painted {
            visual: visual.to_vec(),
            atlas: atlas.to_vec(),
            ..Painted::default()
        });
    }
    // the blocks' flag: the main map's most common one (8 on MP_005)
    let mut fc: BTreeMap<u8, usize> = BTreeMap::new();
    for f in map.flags.iter().flatten().filter(|f| **f > 0) {
        *fc.entry(*f).or_default() += 1;
    }
    let flag = fc
        .iter()
        .max_by_key(|(f, c)| (**c, **f))
        .map_or(8usize, |(f, _)| usize::from(*f))
        .clamp(1, w.max(1));
    // 2. each reached cell's new shader: the terrain's own, else a tiled superset, else a clone
    let own = textures.map(|t| layer_textures(&model, t));
    let mut model = model;
    let mut added: Vec<NewShader> = Vec::new();
    let mut new_shader: BTreeMap<usize, usize> = BTreeMap::new();
    let mut padded = 0usize;
    for (&k, ls) in &reach {
        let s = usize::from(map.shader.get(k).copied().unwrap_or(0));
        let mut want: BTreeSet<usize> = model.shaders.get(s).map(|v| v.iter().copied().collect()).unwrap_or_default();
        want.extend(ls.iter().copied());
        let pick = if let Some(x) = model.find(&want) {
            x
        } else if let Some(x) = model.find_superset(&want) {
            padded += 1;
            x
        } else {
            let name = model.name(&want);
            let own = own.as_ref().ok_or_else(|| {
                refused(format!("{name}: the terrain has no patch shader for this layer combination (TEDIT-SH-1)"))
            })?;
            let (donor, tex) = clone_for(&model, &want, own).ok_or_else(|| {
                refused(format!(
                    "{name}: the terrain has no patch shader for this layer combination and none of the same kinds \
                     whose textures can be retargeted (TEDIT-SH-1 / -2)"
                ))
            })?;
            let d = vt
                .shaders
                .get(donor)
                .cloned()
                .ok_or_else(|| refused("a patch shader index past the list"))?;
            if vt.shaders.len() >= 256 {
                return Err(refused(
                    "a visual terrain holds at most 256 patch shaders (a cell's index is a byte)",
                ));
            }
            added.push(NewShader {
                name: name.clone(),
                donor: d.name.clone(),
                textures: tex,
            });
            // no undergrowth for a cloned combination (the donor's undergrowth entry draws the donor's layers)
            vt.shaders.push(PatchShader {
                name,
                undergrowth: String::new(),
                layers: d.layers,
                tiled: d.tiled,
                groups: Vec::new(),
            });
            model.shaders.push(want.iter().copied().collect());
            vt.shaders.len() - 1
        };
        new_shader.insert(k, pick);
    }
    let map = vt
        .maps
        .first()
        .cloned()
        .ok_or_else(|| refused("the visual terrain has no patch map"))?;
    // 3. each cell's mask of layer l at its local (u, v) in [0, 1): its tile's texel, 0 when it has no such mask
    let shader_of = |k: usize| -> usize {
        new_shader
            .get(&k)
            .copied()
            .unwrap_or_else(|| usize::from(map.shader.get(k).copied().unwrap_or(0)))
    };
    let base_atlas = at.clone();
    let old_mask = |k: usize, l: usize, u: f64, v: f64| -> u8 {
        let s = usize::from(map.shader.get(k).copied().unwrap_or(0));
        let Some(slot) = model.tiled_layers(s).iter().position(|&x| x == l) else {
            return 0;
        };
        let (Some(t), Some(f)) = (
            map.tiles.get(k).and_then(|t| t.get(slot.min(3))),
            map.flags.get(k).and_then(|f| f.get(slot.min(3))),
        ) else {
            return 0;
        };
        let f = usize::from(*f).max(1);
        let (cx, cz) = (k % w, k / w);
        let part = span as f64 / f as f64;
        let x = ((cx % f) as f64 * part + u * part) as usize;
        let y = ((cz % f) as f64 * part + v * part) as usize;
        base_atlas.get(usize::from(*t), x, y)
    };
    // 4. the blocks the paint reaches, and each one's new tiles (layer -> inside texels)
    let blocks: BTreeSet<(usize, usize)> = reach.keys().map(|&k| ((k % w) / flag, (k / w) / flag)).collect();
    let px_per_cell = (span / flag).max(1);
    let mut new_tiles: BTreeMap<(usize, usize, usize), Vec<u8>> = BTreeMap::new();
    for &(bx, bz) in &blocks {
        let cells: Vec<usize> = (0..flag)
            .flat_map(|dz| (0..flag).map(move |dx| (bx * flag + dx, bz * flag + dz)))
            .filter(|&(x, z)| x < w && z < w)
            .map(|(x, z)| z * w + x)
            .collect();
        let layers: BTreeSet<usize> = cells.iter().flat_map(|&k| model.tiled_layers(shader_of(k))).collect();
        for l in layers {
            let mut inside = vec![0u8; span * span];
            for y in 0..span {
                for x in 0..span {
                    let (cx, cz) = (bx * flag + x / px_per_cell, bz * flag + y / px_per_cell);
                    if cx >= w || cz >= w {
                        continue;
                    }
                    let k = cz * w + cx;
                    if !model.tiled_layers(shader_of(k)).contains(&l) {
                        continue;
                    }
                    let (u, v) = (
                        ((x % px_per_cell) as f64 + 0.5) / px_per_cell as f64,
                        ((y % px_per_cell) as f64 + 0.5) / px_per_cell as f64,
                    );
                    let mut m = f64::from(old_mask(k, l, u, v));
                    for &(pl, r) in &painted {
                        if pl != l {
                            continue;
                        }
                        let (sx, sz) = (
                            ((cx as f64 + u) * q as f64).floor() as u32,
                            ((cz as f64 + v) * q as f64).floor() as u32,
                        );
                        let wgt =
                            f64::from(r.get(sx.min(hf_size - 1), sz.min(hf_size - 1)).min(255)) / 255.0;
                        m += (255.0 - m) * wgt;
                    }
                    if let Some(slot) = inside.get_mut(y * span + x) {
                        *slot = m.round().clamp(0.0, 255.0) as u8;
                    }
                }
            }
            new_tiles.insert((bx, bz, l), inside);
        }
    }
    // 5. atlas slots: those no cell outside the blocks names, equal tiles (the empty one) written once
    let in_block = |k: usize| blocks.contains(&((k % w) / flag, (k / w) / flag));
    let mut kept: BTreeSet<usize> = BTreeSet::new();
    for k in (0..w * w).filter(|&k| !in_block(k)) {
        if let (Some(t), Some(f)) = (map.tiles.get(k), map.flags.get(k)) {
            for (t, f) in t.iter().zip(f) {
                if *f > 0 {
                    kept.insert(usize::from(*t));
                }
            }
        }
    }
    let free_before = at.capacity().saturating_sub(kept.len());
    let mut free = (0..at.capacity()).filter(|t| !kept.contains(t));
    let mut placed: BTreeMap<Vec<u8>, usize> = BTreeMap::new();
    let mut slot_of: BTreeMap<(usize, usize, usize), usize> = BTreeMap::new();
    for (key, inside) in &new_tiles {
        let t = if let Some(&t) = placed.get(inside) {
            t
        } else {
            let t = free.next().ok_or_else(|| {
                refused(format!(
                    "the mask atlas holds {} tiles and the paint needs more (TEDIT-ATL-1: growing it is not written)",
                    at.capacity()
                ))
            })?;
            at.put(t, inside);
            placed.insert(inside.clone(), t);
            t
        };
        slot_of.insert(*key, t);
    }
    // 6. the cells of the blocks: shader, tiles, flags (TEDIT-VT-1)
    let mut out_map = map.clone();
    let mut cells = 0usize;
    for k in (0..w * w).filter(|&k| in_block(k)) {
        let s = shader_of(k);
        let (bx, bz) = ((k % w) / flag, (k / w) / flag);
        let mut tiles = [0u16; 4];
        let mut flags = [0u8; 4];
        for (slot, l) in model.tiled_layers(s).into_iter().enumerate().take(4) {
            if let (Some(t), Some(f)) = (tiles.get_mut(slot), flags.get_mut(slot)) {
                *t = slot_of.get(&(bx, bz, l)).map_or(0, |t| *t as u16);
                *f = flag as u8;
            }
        }
        let before = (
            out_map.shader.get(k).copied(),
            out_map.tiles.get(k).copied(),
            out_map.flags.get(k).copied(),
        );
        if let Some(x) = out_map.shader.get_mut(k) {
            *x = s as u8;
        }
        if let Some(x) = out_map.tiles.get_mut(k) {
            *x = tiles;
        }
        if let Some(x) = out_map.flags.get_mut(k) {
            *x = flags;
        }
        if before != (Some(s as u8), Some(tiles), Some(flags)) {
            cells += 1;
        }
    }
    if let Some(m) = vt.maps.first_mut() {
        *m = out_map;
    }
    let notes = vec![format!(
        "visual paint: {} layer(s) over {} cells in {} blocks ({flag} x {flag} cells a tile); {cells} cells rewritten, \
         {} tiles written ({free_before} atlas slots free), {padded} cells drawn by a shader with more tiled layers \
         (their extra masks empty), {} patch shader(s) cloned for new combinations",
        painted.len(),
        reach.len(),
        blocks.len(),
        placed.len(),
        added.len()
    )];
    Ok(Painted {
        visual: vt.encode(),
        atlas: at.bytes,
        tiles: placed.len(),
        added,
        cells,
        padded,
        notes,
    })
}

/// The cells of the main patch map whose patch shader draws layer `l` (`None`: any layer), as a mask over the patch
/// map: for the editor's view of where a layer lies.
#[must_use]
pub fn cells_with(vt: &VisualTerrain, model: &LayerModel, l: usize) -> Vec<bool> {
    vt.maps.first().map_or_else(Vec::new, |m| {
        m.shader
            .iter()
            .map(|s| model.shaders.get(usize::from(*s)).is_some_and(|ls| ls.contains(&l)))
            .collect()
    })
}

// ------------------------------------------------------------------------------------------------ the pairing

/// A tiled layer's paired material (TEDIT-MM-2): the material map's name most cells carry where the layer's mask is
/// strong (200 or more), and that share of them.
#[derive(Debug, Clone, PartialEq)]
pub struct Pair {
    /// The material's name.
    pub material: String,
    /// The share of the strong-mask samples on it.
    pub share: f64,
    /// Strong-mask samples counted.
    pub samples: usize,
}

/// Each tiled layer's paired material from the terrain's own data (TEDIT-MM-2): the visual terrain's masks against
/// its material map, sampled at every fourth texel of each tile.
///
/// # Errors
/// The files do not read, or the terrain's layers are not one kind each.
pub fn pairing(
    visual: &[u8],
    atlas: &[u8],
    hf_size: u32,
    materials: &MaterialMap,
) -> Result<BTreeMap<String, Pair>, LevelWriteError> {
    let vt = VisualTerrain::decode(visual).map_err(|e| refused(format!("the visual terrain: {e}")))?;
    let model = LayerModel::of(&vt).map_err(refused)?;
    let map = vt.maps.first().ok_or_else(|| refused("no patch map"))?;
    let at = Atlas::of(atlas, map.header).map_err(refused)?;
    let w = map.header[4] as usize;
    let span = at.span();
    let cells_per_sample = f64::from(materials.width) / f64::from(hf_size.max(1));
    let q = f64::from(hf_size) / w as f64;
    let mut counts: BTreeMap<usize, BTreeMap<u8, usize>> = BTreeMap::new();
    for k in 0..w * w {
        let s = usize::from(map.shader.get(k).copied().unwrap_or(0));
        let (cx, cz) = (k % w, k / w);
        for (slot, l) in model.tiled_layers(s).into_iter().enumerate().take(4) {
            let (Some(t), Some(f)) = (
                map.tiles.get(k).and_then(|t| t.get(slot)),
                map.flags.get(k).and_then(|f| f.get(slot)),
            ) else {
                continue;
            };
            let f = usize::from(*f).max(1);
            let part = span / f;
            for y in (0..part).step_by(4) {
                for x in (0..part).step_by(4) {
                    let v = at.get(usize::from(*t), (cx % f) * part + x, (cz % f) * part + y);
                    if v < 200 {
                        continue;
                    }
                    let (sx, sz) = (
                        (cx as f64 + (x as f64 + 0.5) / part as f64) * q,
                        (cz as f64 + (y as f64 + 0.5) / part as f64) * q,
                    );
                    let top = materials.width.saturating_sub(1) as usize;
                    let (mx, mz) = (
                        ((sx * cells_per_sample) as usize).min(top),
                        ((sz * cells_per_sample) as usize).min(top),
                    );
                    let m = materials.get(mx, mz);
                    *counts.entry(l).or_default().entry(m).or_default() += 1;
                }
            }
        }
    }
    Ok(counts
        .into_iter()
        .filter_map(|(l, c)| {
            let total: usize = c.values().sum();
            let (&m, &n) = c.iter().max_by_key(|(m, n)| (**n, std::cmp::Reverse(**m)))?;
            Some((
                model.layers.get(l)?.clone(),
                Pair {
                    material: materials.names.get(usize::from(m))?.clone(),
                    share: n as f64 / total.max(1) as f64,
                    samples: total,
                },
            ))
        })
        .collect())
}

// ------------------------------------------------------------------------------------------------ the colour map

/// Whether a colour / occlusion map is grey (L8): its raster then holds grey levels, not 5:6:5 colours.
#[must_use]
pub fn colour_is_grey(map: &[u8]) -> bool {
    itexture::Header::read(map).is_some_and(|h| h.format == Format::L8.code())
}

/// A colour / occlusion map's level 0 as a raster (DXT1: each texel's 5:6:5 colour; L8: its grey level); `None` for
/// another format (SP_04's: TGEN-OCC-1).
#[must_use]
pub fn colour_raster(map: &[u8]) -> Option<Raster> {
    let grey = colour_is_grey(map);
    let pic = itexture::decode_level(map, 0)?;
    let samples: Vec<u16> = pic
        .rgb
        .iter()
        .map(|c| if grey { u16::from(c[0]) } else { itexture::pack565(*c) })
        .collect();
    Some(Raster::from_samples(pic.w as u32, pic.h as u32, &samples))
}

/// A raster sample as a colour (the map's format).
#[must_use]
pub fn colour_of(v: u16, grey: bool) -> [u8; 3] {
    if grey {
        [v.min(255) as u8; 3]
    } else {
        itexture::unpack565(v)
    }
}

/// A colour as a raster sample (the map's format).
#[must_use]
pub fn sample_of(c: [u8; 3], grey: bool) -> u16 {
    if grey { u16::from(c[0]) } else { itexture::pack565(c) }
}

/// The colour / occlusion map with the raster's changed texels (TEDIT-OCC-1): the base's bytes when none changed;
/// else only the DXT1 blocks (or L8 texels) under the change and the mip texels below them rewritten
/// ([`itexture::splice`]).
///
/// # Errors
/// The base is not a DXT1 or L8 map, or the raster is not its size.
pub fn write_colour(base: &[u8], raster: &Raster) -> Result<Vec<u8>, LevelWriteError> {
    let grey = colour_is_grey(base);
    let before = colour_raster(base).ok_or_else(|| refused("the colour / occlusion map is not DXT1 or L8"))?;
    if (before.width(), before.height()) != (raster.width(), raster.height()) {
        return Err(refused(format!(
            "a colour stroke of {} x {} texels on a map of {} x {}",
            raster.width(),
            raster.height(),
            before.width(),
            before.height()
        )));
    }
    let (a, b) = (before.samples(), raster.samples());
    let Some(region) = crate::terrainedit::changed_region(&a, &b, before.width() as usize) else {
        return Ok(base.to_vec());
    };
    let mut pic = itexture::decode_level(base, 0).ok_or_else(|| refused("the map does not decode"))?;
    for (k, (x, y)) in a.iter().zip(&b).enumerate() {
        if x != y
            && let Some(t) = pic.rgb.get_mut(k)
        {
            *t = colour_of(*y, grey);
        }
    }
    itexture::splice(
        base,
        &pic,
        Rect {
            x0: region.x0,
            y0: region.z0,
            x1: region.x1,
            y1: region.z1,
        },
    )
    .ok_or_else(|| refused("the colour / occlusion map did not take the edit"))
}

/// What [`reshade`] did.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Reshaded {
    /// Texels changed.
    pub texels: usize,
    /// The mean ratio of new to old sky visibility over them.
    pub mean_ratio: f64,
}

/// Re-shades a colour / occlusion raster after a sculpt (TEDIT-OCC-1, LIGHT-BAKE-6): over the texels of `area`
/// (world min x, min z, max x, max z; the map covers [`bc2_terragen::visual::OCCLUSION_M`] centred on the origin)
/// each texel's colour scaled by the ratio of its sky visibility on the new ground to the old (clamped to 1/4 - 4),
/// where they differ; the rest of the map unchanged. DICE painted its maps, so the edit's effect is laid over its
/// painting rather than the map computed again.
#[must_use]
pub fn reshade(
    colour: &Raster,
    grey: bool,
    old: &(dyn Fn(f64, f64) -> f64 + Sync),
    new: &(dyn Fn(f64, f64) -> f64 + Sync),
    area: [f64; 4],
) -> (Raster, Reshaded) {
    use bc2_terragen::visual::{OCCLUSION_M, sky_visibility_over};
    let w = colour.width() as usize;
    let per_m = w as f64 / OCCLUSION_M;
    let tex = |v: f64| ((v + OCCLUSION_M / 2.0) * per_m).floor().clamp(0.0, w as f64) as usize;
    let a = [tex(area[0]), tex(area[1]), (tex(area[2]) + 1).min(w), (tex(area[3]) + 1).min(w)];
    let mut out = colour.clone();
    if a[2] <= a[0] || a[3] <= a[1] {
        return (out, Reshaded::default());
    }
    let (vo, vn) = rayon::join(
        || sky_visibility_over(old, w, Some(a)),
        || sky_visibility_over(new, w, Some(a)),
    );
    let aw = a[2] - a[0];
    let mut done = Reshaded::default();
    let mut sum = 0.0;
    for (k, (o, n)) in vo.iter().zip(&vn).enumerate() {
        let ratio = (n / o.max(1e-6)).clamp(0.25, 4.0);
        if (ratio - 1.0).abs() < 0.002 {
            continue;
        }
        let (x, y) = ((a[0] + k % aw) as u32, (a[1] + k / aw) as u32);
        let c = colour_of(colour.get(x, y), grey);
        let scaled = c.map(|v| (f64::from(v) * ratio).round().clamp(0.0, 255.0) as u8);
        out.set(x, y, sample_of(scaled, grey));
        done.texels += 1;
        sum += ratio;
    }
    done.mean_ratio = if done.texels > 0 { sum / done.texels as f64 } else { 1.0 };
    (out, done)
}

/// What is wrong with an exported colour / occlusion map against its base (XVAL-TERR-3), in words: it decodes in the
/// base's format, size, flags and level sizes.
#[must_use]
pub fn check_colour(base: &[u8], edited: &[u8]) -> Vec<String> {
    match (itexture::Header::read(base), itexture::Header::read(edited)) {
        (Some(b), Some(e)) if b == e && edited.len() == base.len() => {
            if itexture::decode_level(edited, 0).is_some() {
                Vec::new()
            } else {
                vec!["does not decode".into()]
            }
        }
        (Some(_), Some(_)) => vec!["its header or size is not the base's".into()],
        _ => vec!["does not read as a PC texture".into()],
    }
}

/// What is wrong with an exported visual terrain and mask atlas against their base (XVAL-TERR-3), in words: they
/// decode; the blocks, road blocks, patch-map headers and the base's patch shaders are the base's (shaders may be
/// added after them); every cell that differs from the base names a shader in the list, its tiles inside the atlas,
/// its non-zero flags its shader's tiled count (TEDIT-VT-1); the atlas keeps its size and format.
#[must_use]
pub fn check_visual(base: &[u8], edited: &[u8], atlas_base: &[u8], atlas: &[u8]) -> Vec<String> {
    let (b, e) = match (VisualTerrain::decode(base), VisualTerrain::decode(edited)) {
        (Ok(b), Ok(e)) => (b, e),
        (_, Err(e)) => return vec![format!("the visual terrain does not decode: {e}")],
        (Err(e), _) => return vec![format!("the base visual terrain does not decode: {e}")],
    };
    let mut out = Vec::new();
    if e.roads != b.roads || e.maps.len() != b.maps.len() || e.shaders.get(..b.shaders.len()) != Some(&b.shaders[..])
    {
        out.push("its road blocks, block count or the base's patch shaders are not the base's".into());
    }
    match (itexture::Header::read(atlas_base), itexture::Header::read(atlas)) {
        (Some(x), Some(y)) if x == y && atlas.len() == atlas_base.len() => {}
        _ => out.push("the mask atlas's header or size is not the base's".into()),
    }
    for (bm, em) in b.maps.iter().zip(&e.maps) {
        if bm.header != em.header || bm.bounds != em.bounds || bm.heightfield != em.heightfield {
            out.push("a patch map's header, bounds or height field is not the base's".into());
            continue;
        }
        let capacity = (em.header[2] * em.header[3]) as usize;
        let mut bad = 0usize;
        for k in 0..em.shader.len() {
            let same = bm.shader.get(k) == em.shader.get(k)
                && bm.tiles.get(k) == em.tiles.get(k)
                && bm.flags.get(k) == em.flags.get(k);
            if same {
                continue;
            }
            let s = usize::from(em.shader.get(k).copied().unwrap_or(0));
            let Some(sh) = e.shaders.get(s) else {
                bad += 1;
                continue;
            };
            let flags = em.flags.get(k).copied().unwrap_or_default();
            let tiles = em.tiles.get(k).copied().unwrap_or_default();
            let nz = flags.iter().filter(|f| **f > 0).count();
            if nz != sh.tiled as usize
                || tiles
                    .iter()
                    .zip(flags)
                    .any(|(t, f)| f > 0 && usize::from(*t) >= capacity)
            {
                bad += 1;
            }
        }
        if bad > 0 {
            out.push(format!(
                "{bad} rewritten cells name no shader, a tile past the atlas, or flags not their shader's tiled count"
            ));
        }
    }
    out
}

/// A picture of a raster for tests and the editor's views: the colour raster decoded.
#[must_use]
pub fn colour_picture(r: &Raster, grey: bool) -> Picture {
    Picture {
        w: r.width() as usize,
        h: r.height() as usize,
        rgb: r.samples().iter().map(|v| colour_of(*v, grey)).collect(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::too_many_lines)]
mod tests {
    use super::*;
    use bc2_formats::visual_terrain::PatchMap;

    const PRE: &str = "Terrains/T/T/PatchMaskShaders/";

    fn shader(name: &str, layers: u32, tiled: u32) -> PatchShader {
        PatchShader {
            name: format!("{PRE}{name}"),
            undergrowth: String::new(),
            layers,
            tiled,
            groups: Vec::new(),
        }
    }

    /// A 256 m terrain of 16 x 16 cells, every cell the rule-only `Base_Destr`, an empty 1,024 x 512 atlas.
    fn terrain() -> (Vec<u8>, Vec<u8>) {
        let vt = VisualTerrain {
            roads: vec![VisualTerrain::EMPTY_ROADS.to_vec(); 2],
            shaders: vec![
                shader("Base_Destr", 2, 0),
                shader("Base_Grass_Destr", 3, 1),
                shader("Base_Rock_Destr", 3, 1),
                shader("Base_Grass_Rock_Destr", 4, 2),
                shader("Base_Sand_Destr", 3, 1),
                shader("Base_Snow_Bank_Destr", 3, 1),
            ],
            maps: vec![PatchMap::rule_only(
                [-128.0, 0.0, -128.0, 128.0, 500.0, 128.0],
                [66, 1, 15, 7],
                16,
                0,
                "Terrains/T/T.Heightfield-000",
                "Terrains/T/T.MaskTileAtlas",
            )],
        };
        let atlas = itexture::encode(
            &Picture {
                w: 1024,
                h: 512,
                rgb: vec![[0; 3]; 1024 * 512],
            },
            Format::L8,
            0,
            false,
        );
        (vt.encode(), atlas)
    }

    fn disc(n: u32, cx: f64, cz: f64, r: f64) -> Raster {
        let mut w = Raster::new(n, n, 0);
        for j in 0..n {
            for i in 0..n {
                if (f64::from(i) - cx).hypot(f64::from(j) - cz) < r {
                    w.set(i, j, 255);
                }
            }
        }
        w
    }

    #[test]
    fn layer_names_split_and_kinds_found() {
        let (v, _) = terrain();
        let m = LayerModel::of(&VisualTerrain::decode(&v).unwrap()).unwrap();
        assert_eq!(m.layers, ["Base", "Grass", "Rock", "Sand", "Snow_Bank", "Destr"]);
        assert_eq!(m.paintable(), ["Grass", "Rock", "Sand", "Snow_Bank"]);
        assert_eq!(m.tiled_layers(3), [1, 2]);
        assert_eq!(m.find(&[0, 1, 5].into_iter().collect()), Some(1));
        // Base + Destr + Grass, drawn by Base_Grass_Rock_Destr with an empty Rock mask
        assert_eq!(m.find_superset(&[0, 1, 5].into_iter().collect()), Some(1));
        assert_eq!(m.find_superset(&[0, 1, 3, 5].into_iter().collect()), None);
        assert_eq!(m.name(&[0, 3, 5].into_iter().collect()), format!("{PRE}Base_Sand_Destr"));
        // a layer tiled in one shader and a rule layer in another: not one kind each
        let mut bad = VisualTerrain::decode(&v).unwrap();
        bad.shaders.push(shader("Grass_Destr", 2, 0));
        assert!(LayerModel::of(&bad).unwrap_err().contains("TEDIT-VT-2"));
        // DICE's underscores (SP_04): each name split by its layer count
        let names: Vec<(String, usize)> = [
            ("Snow_Mountain_Destruction", 3),
            ("Snow_Mountain_Cliff_m", 3),
            ("Snow_Mountain_Cliff_m_Destruction", 4),
            ("Snow_Mountain_Cliff_m_Gravel_m_Destruction", 5),
        ]
        .iter()
        .map(|(n, l)| ((*n).to_owned(), *l))
        .collect();
        let s = segment(&names);
        assert_eq!(
            s[3].as_deref().unwrap(),
            ["Snow", "Mountain", "Cliff_m", "Gravel_m", "Destruction"]
        );
    }

    #[test]
    fn a_paint_takes_the_cells_shaders_tiles_and_flags() {
        let (v, a) = terrain();
        let n = 256;
        // nothing painted: the base's bytes
        let none = Raster::new(n, n, 0);
        let p = paint(&v, &a, n, &[PaintLayer { layer: "Grass", weights: &none }], None).unwrap();
        assert_eq!((p.visual == v, p.atlas == a), (true, true));
        // grass on a 20 m disc around (64, 64): the cells it reaches draw Base_Grass_Destr
        let grass = disc(n, 64.0, 64.0, 20.0);
        let p = paint(&v, &a, n, &[PaintLayer { layer: "Grass", weights: &grass }], None).unwrap();
        assert!(check_visual(&v, &p.visual, &a, &p.atlas).is_empty(), "{:?}", check_visual(&v, &p.visual, &a, &p.atlas));
        let vt = VisualTerrain::decode(&p.visual).unwrap();
        let m = &vt.maps[0];
        let k = 4 * 16 + 4; // the cell at (64, 64)
        assert_eq!(m.shader[k], 1);
        assert_eq!(m.flags[k], [8, 0, 0, 0]);
        assert_eq!(m.shader[15 * 16 + 15], 0, "far away: untouched");
        // the cell's mask is full at the disc's centre, empty far from it inside the same tile
        let at = Atlas::of(&p.atlas, m.header).unwrap();
        let t = usize::from(m.tiles[k][0]);
        assert_eq!(at.get(t, 4 * 8 + 4, 4 * 8 + 4), 255);
        let far = 7 * 16; // cell (0, 7) is in the same 8 x 8 block, 100 m away
        assert_eq!(m.tiles[far][0], m.tiles[k][0], "one tile a block");
        assert_eq!(m.shader[far], 0, "a cell the paint did not reach keeps its shader");
        assert!(p.tiles >= 1 && p.cells >= 4, "{:?}", p.notes);
        // rock over the grass: Base_Grass_Rock_Destr, both masks kept
        let rock = disc(n, 64.0, 64.0, 6.0);
        let p2 = paint(
            &p.visual,
            &p.atlas,
            n,
            &[PaintLayer { layer: "Rock", weights: &rock }],
            None,
        )
        .unwrap();
        let vt2 = VisualTerrain::decode(&p2.visual).unwrap();
        let m2 = &vt2.maps[0];
        assert_eq!(m2.shader[k], 3);
        assert_eq!(m2.flags[k], [8, 8, 0, 0]);
        let at2 = Atlas::of(&p2.atlas, m2.header).unwrap();
        // texel (32, 32) of the block's tile is sample (65, 65), near the discs' centre
        assert_eq!(at2.get(usize::from(m2.tiles[k][0]), 32, 32), 255, "the grass kept");
        assert_eq!(at2.get(usize::from(m2.tiles[k][1]), 32, 32), 255, "the rock laid");
        assert!(check_visual(&v, &p2.visual, &a, &p2.atlas).is_empty());
        // a rule layer and an unknown layer are refused
        assert!(paint(&v, &a, n, &[PaintLayer { layer: "Base", weights: &grass }], None).is_err());
        assert!(paint(&v, &a, n, &[PaintLayer { layer: "Mud", weights: &grass }], None).is_err());
    }

    #[test]
    fn a_new_combination_is_a_retextured_clone() {
        let (v, a) = terrain();
        let n = 256;
        let grass = disc(n, 64.0, 64.0, 20.0);
        let p = paint(&v, &a, n, &[PaintLayer { layer: "Grass", weights: &grass }], None).unwrap();
        let sand = disc(n, 64.0, 64.0, 6.0);
        let layer = [PaintLayer { layer: "Sand", weights: &sand }];
        // Base_Grass_Sand_Destr is not a shader of the terrain, and no superset draws it: refused without textures
        let e = paint(&p.visual, &p.atlas, n, &layer, None).unwrap_err().to_string();
        assert!(e.contains("TEDIT-SH-1"), "{e}");
        let tex = |names: &[&str]| -> BTreeSet<String> { names.iter().map(|s| (*s).to_owned()).collect() };
        let textures: EntryTextures = [
            ("Base_Destr", tex(&["base_c", "ao"])),
            ("Base_Grass_Destr", tex(&["base_c", "ao", "grass_c"])),
            ("Base_Rock_Destr", tex(&["base_c", "ao", "rock_c"])),
            ("Base_Grass_Rock_Destr", tex(&["base_c", "ao", "grass_c", "rock_c"])),
            ("Base_Sand_Destr", tex(&["base_c", "ao", "sand_c"])),
            ("Base_Snow_Bank_Destr", tex(&["base_c", "ao", "snow_c"])),
        ]
        .into_iter()
        .map(|(n, t)| (format!("{PRE}{n}"), t))
        .collect();
        let p2 = paint(&p.visual, &p.atlas, n, &layer, Some(&textures)).unwrap();
        assert_eq!(
            p2.added,
            [NewShader {
                name: format!("{PRE}Base_Grass_Sand_Destr"),
                donor: format!("{PRE}Base_Grass_Rock_Destr"),
                textures: vec![("rock_c".into(), "sand_c".into())],
            }]
        );
        let vt = VisualTerrain::decode(&p2.visual).unwrap();
        assert_eq!(vt.shaders.len(), 7);
        assert_eq!((vt.shaders[6].layers, vt.shaders[6].tiled), (4, 2));
        assert_eq!(vt.maps[0].shader[4 * 16 + 4], 6);
        assert!(check_visual(&v, &p2.visual, &a, &p2.atlas).is_empty());
        let own = layer_textures(&LayerModel::of(&VisualTerrain::decode(&v).unwrap()).unwrap(), &textures);
        assert_eq!(own[3], tex(&["sand_c"]));
        assert_eq!(own[0], tex(&["ao", "base_c"]), "Base is in every shader: every shared texture is its own");
    }

    #[test]
    fn the_colour_map_is_painted_and_reshaded_in_place() {
        let (w, h) = (256usize, 256usize);
        let pic = Picture {
            w,
            h,
            rgb: (0..w * h).map(|k| [120, 110 + (k % 7) as u8, 100]).collect(),
        };
        for f in [Format::Dxt1, Format::L8] {
            let map = itexture::encode(&pic, f, 2, true);
            let grey = colour_is_grey(&map);
            let r = colour_raster(&map).unwrap();
            assert_eq!(write_colour(&map, &r).unwrap(), map, "{f:?}: nothing changed");
            // a pit sculpted at the origin: the texels around it darker, the rest byte for byte
            let flat = |_: f64, _: f64| 50.0;
            let pit = |x: f64, z: f64| if x.hypot(z) < 30.0 { 20.0 } else { 50.0 };
            let (shaded, done) = reshade(&r, grey, &flat, &pit, [-40.0, -40.0, 40.0, 40.0]);
            assert!(done.texels > 0 && done.mean_ratio < 1.0, "{f:?}: {done:?}");
            let out = write_colour(&map, &shaded).unwrap();
            assert!(check_colour(&map, &out).is_empty());
            let hd = itexture::Header::read(&map).unwrap();
            let l0 = hd.level_start(0)..hd.level_start(1);
            let changed = out[l0.clone()].iter().zip(&map[l0]).filter(|(a, b)| a != b).count();
            // the map covers 2,048 m with 256 texels (8 m each): the pit's ~8 x 8 texels and their blocks
            assert!(changed > 0 && changed < 2000, "{f:?}: {changed}");
            let back = itexture::decode_level(&out, 0).unwrap();
            assert!(back.at(128, 128)[0] < pic.at(128, 128)[0], "{f:?}: the pit darker");
            assert_eq!(back.at(10, 10), itexture::decode_level(&map, 0).unwrap().at(10, 10));
        }
    }
}
