//! What the map editor's simplified view draws of each placed object (round SV1): its render mesh, read the way the
//! game-accurate view reads it - the same mesh choice ([`crate::world::mesh_and_set`]), the same parts, programs and
//! draw classes ([`crate::material::prepare_untextured`]) - as plain geometry: positions, the first texture
//! coordinates and 32-bit indices, one [`Part`] per draw. Each part keeps what the simplified view needs of its
//! material: whether the game alpha-tests it (leaf cards, fences, grilles), its diffuse texture small (64 texels wide:
//! its alpha cuts the leaf cards' holes) and the texture's mean colour (a flat tint: trees read green, rocks grey).
//!
//! Until round SV1 the simplified view drew each blueprint's Havok collision; for vegetation that is the render
//! mesh's leaf cards without their textures, drawn as solid "spike balls".

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use bc2_formats::shaderdb::query;
use rayon::prelude::*;

use crate::files::{Files, level_folders, package_stack};
use crate::material::{self, BLENDED, Db, Prepared, TESTED};
use crate::placement::{self, Placements};
use crate::world::{Placed, mesh_and_set};
use crate::{f32_at, half, u32_at};

/// The widest texture level kept for a part (texels).
pub const TEXTURE_WIDTH: u32 = 64;

/// Levels of detail read at most (BFBC2's mesh sets list up to 4).
pub const MAX_LODS: usize = 8;

/// A decoded texture level: its size and RGBA texels, row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    /// Width and height.
    pub size: [u32; 2],
    /// RGBA, 4 bytes a texel.
    pub rgba: Vec<u8>,
}

/// One draw of a mesh as plain geometry.
#[derive(Debug, Clone)]
pub struct Part {
    /// The mesh part's name.
    pub name: String,
    /// Its first index in [`SimpleMesh::indices`].
    pub first: u32,
    /// Indices drawn (three a triangle).
    pub count: u32,
    /// The game alpha-tests it ([`material::TESTED`]): texels of its texture under [`Part::alpha_ref`] are holes.
    pub tested: bool,
    /// The game blends it ([`material::BLENDED`]: glass, decals).
    pub blended: bool,
    /// The alpha-test reference, 1 - 255.
    pub alpha_ref: u8,
    /// Its diffuse texture's name.
    pub texture_name: Option<String>,
    /// Its diffuse texture, small ([`TEXTURE_WIDTH`]).
    pub texture: Option<Arc<Picture>>,
    /// The texture's mean colour (of the texels an alpha test keeps).
    pub tint: Option<[u8; 3]>,
}

/// A render mesh's level of detail as plain geometry, in the mesh's space (composite parts placed by their rest
/// bones): LOD 0, the game view's, with the mesh set's other levels in [`SimpleMesh::lods`].
#[derive(Debug, Clone, Default)]
pub struct SimpleMesh {
    /// The mesh's name.
    pub mesh: String,
    /// Its level of detail (0 the finest).
    pub lod: usize,
    /// LOD 0's: the mesh set's coarser levels (1, 2 ...), each as plain geometry; empty where the set has one.
    pub lods: Vec<Arc<SimpleMesh>>,
    /// Positions.
    pub positions: Vec<[f32; 3]>,
    /// The first texture coordinates, one per position (0, 0 where the layout has none).
    pub uvs: Vec<[f32; 2]>,
    /// Triangle indices into the positions.
    pub indices: Vec<u32>,
    /// The draws.
    pub parts: Vec<Part>,
}

impl SimpleMesh {
    /// Indices of every part (what the game view's draws of the same mesh draw: [`material::Draw::index_count`]).
    #[must_use]
    pub fn index_count(&self) -> usize {
        self.parts.iter().map(|p| p.count as usize).sum()
    }

    /// Nothing solid of it shows in the game: every part blended (DICE's invisible walls,
    /// `objects/objects/invisiblewall/invwall_mesh`: one blended, untextured part; light shafts, caustics, vista
    /// smoke, glass on its own), or an occlusion mesh (`terrains/sp_01/occludermeshes/occ_mesh1`,
    /// `terrains/mp_005/props/mp05_occluder`: untextured slabs the renderer culls with).
    #[must_use]
    pub fn invisible(&self) -> bool {
        let name = self.mesh.to_lowercase();
        self.parts.iter().all(|p| p.blended) || name.contains("occluder") || name.contains("/occ_mesh")
    }

    /// The parts the simplified view draws: the opaque and alpha-tested ones; the blended ones (glass, decals) only
    /// when the mesh has nothing else.
    pub fn drawn_parts(&self) -> impl Iterator<Item = &Part> {
        let only_blended = self.parts.iter().all(|p| p.blended);
        self.parts.iter().filter(move |p| only_blended || !p.blended)
    }

    /// Its level of detail `k` (0: itself; past the coarsest: the coarsest).
    #[must_use]
    pub fn at_lod(&self, k: usize) -> &Self {
        match k.checked_sub(1) {
            None => self,
            Some(i) => self.lods.get(i).or(self.lods.last()).map_or(self, |m| m.as_ref()),
        }
    }
}

/// What a placed thing is drawn as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// The mesh at this place of [`Simple::meshes`].
    Mesh(usize),
    /// Its blueprint was read and names no mesh (invisible walls, triggers, volumes, logic), or its mesh draws
    /// nothing: the game shows nothing there.
    Nothing,
}

/// A level's render meshes for the simplified view.
#[derive(Debug, Clone, Default)]
pub struct Simple {
    /// The meshes.
    pub meshes: Vec<Arc<SimpleMesh>>,
    /// A placed level-partition instance's shape by its blueprint (lower case).
    pub blueprints: HashMap<String, Shape>,
    /// A group member's shape by its group file and item ([`crate::world::placed_key`]).
    pub members: HashMap<String, Shape>,
    /// The placed blueprints' destructible parts (lower case; [`components`]).
    pub components: HashMap<String, Vec<Component>>,
    /// What was found while reading.
    pub notes: Vec<String>,
    /// Reading took (seconds).
    pub seconds: f64,
}

impl Simple {
    /// The shape of a placed thing: `None` when it was not read (a blueprint placed later).
    #[must_use]
    pub fn shape(&self, p: &Placed) -> Option<Shape> {
        match &p.group {
            Some(_) => self.members.get(&crate::world::placed_key(p)).copied(),
            None => self.blueprints.get(&p.blueprint.to_lowercase()).copied(),
        }
    }

    /// Reads a level's render meshes: a prepared level from its folders (`work\levels\<lv>`), a Package-only level
    /// from its bundles in place (`package`; [`Files::bundles`], read in rounds as [`crate::world::World::load_package`]
    /// reads them). `level`: the partition's name (`MP_005`).
    ///
    /// # Errors
    /// The level's shader database cannot be read, or a Package level's bundles cannot.
    pub fn load(opfor: &Path, lv: &str, level: &str, placed: &[Placed], package: bool) -> Result<Self, String> {
        let t = Instant::now();
        let mut out = if package {
            let stack = package_stack(&opfor.join("game"), lv)
                .ok_or_else(|| format!("{lv}: not in the game's bundle registry (package.mft)"))?;
            let files = Files::bundles(&stack)?;
            let mut rounds = 0;
            loop {
                rounds += 1;
                let made = Self::load_from(&files, lv, level, placed);
                let missed = files.missed();
                if missed.is_empty() || rounds >= 40 {
                    break made?;
                }
                files.fetch(&missed)?;
            }
        } else {
            Self::load_from(&Files::new(&level_folders(opfor, lv)), lv, level, placed)?
        };
        out.seconds = t.elapsed().as_secs_f64();
        Ok(out)
    }

    /// [`Simple::load`] from a set of files.
    ///
    /// # Errors
    /// The level's shader database cannot be read.
    pub fn load_from(files: &Files, lv: &str, level: &str, placed: &[Placed]) -> Result<Self, String> {
        let lv = lv.to_lowercase();
        let db = Db::parse(
            &files
                .read(&format!("levels/{lv}/shaders/dx11_single.res"))
                .ok_or_else(|| format!("{lv}: no Dx11 shader database in its files"))?,
        )?;
        let mut notes = Vec::new();
        let placements = placement::read(files, level).unwrap_or_else(|e| {
            notes.push(format!("placements: {e}"));
            Placements::default()
        });
        // each placed thing's mesh, as the game view chooses it (the default shader set: the geometry is the same)
        let mut blueprint_mesh = HashMap::new();
        let mut wanted: Vec<String> = Vec::new();
        let mut of_placed: Vec<Option<String>> = Vec::with_capacity(placed.len());
        for p in placed {
            let (mesh, _) = mesh_and_set(files, &placements, &mut blueprint_mesh, p);
            let mesh = mesh.map(|m| m.to_lowercase());
            if let Some(m) = &mesh
                && !wanted.contains(m)
            {
                wanted.push(m.clone());
            }
            of_placed.push(mesh);
        }
        wanted.sort();
        let textures: Mutex<HashMap<String, Option<Arc<Picture>>>> = Mutex::new(HashMap::new());
        let made: Vec<(String, Option<SimpleMesh>, Vec<String>)> = wanted
            .par_iter()
            .map(|m| {
                let pr = material::prepare_untextured(files, &db, m, None, 0);
                let notes = pr.notes.iter().map(|n| format!("{m}: {n}")).collect();
                let mut s = simple_mesh(files, &db, &pr, 0, &textures);
                // the coarser levels the mesh set lists, while their data is there
                for k in 1..MAX_LODS {
                    let pr = material::prepare_untextured(files, &db, m, None, k);
                    let l = simple_mesh(files, &db, &pr, k, &textures);
                    if l.index_count() == 0 {
                        break;
                    }
                    s.lods.push(Arc::new(l));
                }
                (m.clone(), (s.index_count() > 0).then_some(s), notes)
            })
            .collect();
        let mut meshes = Vec::new();
        let mut index = HashMap::new();
        for (name, mesh, n) in made {
            notes.extend(n);
            if let Some(mesh) = mesh {
                index.insert(name, meshes.len());
                meshes.push(Arc::new(mesh));
            }
        }
        let mut out = Self {
            meshes,
            notes,
            ..Self::default()
        };
        for (p, mesh) in placed.iter().zip(&of_placed) {
            let shape = match mesh {
                // a mesh that could not be read is not "nothing": the view keeps the collision
                Some(m) => index.get(m).map(|&k| Shape::Mesh(k)),
                // no mesh named: nothing drawn, when the blueprint itself was read
                None => files
                    .kind(&format!("{}.dbx", p.blueprint))
                    .is_some()
                    .then_some(Shape::Nothing),
            };
            let Some(shape) = shape else { continue };
            if p.group.is_some() {
                out.members.insert(crate::world::placed_key(p), shape);
            } else {
                let bp = p.blueprint.to_lowercase();
                if matches!(shape, Shape::Mesh(_)) && !out.components.contains_key(&bp) {
                    let c = components(files, &bp);
                    out.components.insert(bp.clone(), c);
                }
                out.blueprints.insert(bp, shape);
            }
        }
        Ok(out)
    }

    /// A blueprint's destructible parts (empty: it has none).
    #[must_use]
    pub fn components_of(&self, blueprint: &str) -> &[Component] {
        self.components
            .get(&blueprint.to_lowercase())
            .map_or(&[], Vec::as_slice)
    }
}

/// A destructible part of a blueprint (`GameSharedResources.PartComponentData`): its name and its health states, the
/// healthiest first - each the health it starts at and the mesh parts it shows (`MeshName`: `part_1`, `part_1_dest`,
/// `ruin_downed`; empty for none: the part is gone).
#[derive(Debug, Clone, PartialEq)]
pub struct Component {
    /// Its name (`part_1`, `ruin`).
    pub name: String,
    /// (health, mesh name), the healthiest first.
    pub states: Vec<(f32, String)>,
}

/// Which of a destructible object's states the simplified view shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum State {
    /// Every part at its healthiest state (as the level starts).
    #[default]
    Intact,
    /// Every part at its last state (a destroyed part shows its ruin or nothing).
    Destroyed,
    /// Every mesh part, whatever its state (as the game-accurate view draws the mesh).
    All,
    /// Round VW1: every part at its health state of this rank (0 the healthiest), or its last when it has fewer:
    /// the steps between intact and destroyed ([`stage_count`]).
    Stage(u8),
}

/// How many states a destructible object steps through: the most health states any of its parts has (`SP_01`'s guard
/// tower: 3 - `part_1` / `part_1_dest` / gone, `ruin` / `ruin_downed`); 0 for none.
#[must_use]
pub fn stage_count(components: &[Component]) -> usize {
    components.iter().map(|c| c.states.len()).max().unwrap_or(0)
}

/// The health state a component shows in a state: (its mesh name), `None` for every part (`State::All`).
fn chosen(c: &Component, state: State) -> Option<&str> {
    let s = match state {
        State::All => return None,
        State::Intact => c.states.first(),
        State::Destroyed => c.states.last(),
        State::Stage(k) => c.states.get(usize::from(k)).or_else(|| c.states.last()),
    };
    s.map(|(_, m)| m.as_str())
}

/// Whether a collision part (its name's hash, `bc2_formats::havok::Parts::hashes`) is in a state of a blueprint's
/// destructible parts: a part a health state names (`part_1_dest`) when that state is its component's in the state;
/// a part no state names always.
#[must_use]
pub fn collision_part_shown(components: &[Component], hash: u32, state: State) -> bool {
    if state == State::All {
        return true;
    }
    let owner = components.iter().find(|c| {
        c.states
            .iter()
            .any(|(_, m)| !m.is_empty() && bc2_formats::havok::part_hash(m) == hash)
    });
    let Some(c) = owner else { return true };
    chosen(c, state).is_some_and(|m| !m.is_empty() && bc2_formats::havok::part_hash(m) == hash)
}

/// A blueprint's destructible parts, from its partition (`<blueprint>.dbx`).
#[must_use]
pub fn components(files: &Files, blueprint: &str) -> Vec<Component> {
    let Some(data) = files.read(&format!("{blueprint}.dbx")) else {
        return Vec::new();
    };
    let Ok(tree) = bc2_formats::dbx::parse_tree(&data) else {
        return Vec::new();
    };
    let by_guid: HashMap<String, bc2_formats::dbx::Node<'_, '_>> = tree
        .instances()
        .filter_map(|i| Some((i.attr("guid")?.to_lowercase(), i)))
        .collect();
    let mut out = Vec::new();
    for inst in tree
        .instances()
        .filter(|i| i.attr("type") == Some("GameSharedResources.PartComponentData"))
    {
        let name = inst.child("Name").map_or("", |n| n.text()).to_lowercase();
        let mut states: Vec<(f32, String)> = inst
            .child("HealthStates")
            .map(|h| h.children())
            .into_iter()
            .flatten()
            .filter_map(|item| by_guid.get(&item.attr("ref")?.to_lowercase()))
            .map(|s| {
                let health = s
                    .child("Health")
                    .and_then(|h| h.value().ok())
                    .and_then(|v| v.parse::<f32>().ok())
                    .unwrap_or(0.0);
                (health, s.child("MeshName").map_or("", |n| n.text()).to_lowercase())
            })
            .collect();
        states.sort_by(|a, b| b.0.total_cmp(&a.0));
        if !name.is_empty() && !states.is_empty() {
            out.push(Component { name, states });
        }
    }
    out
}

/// Whether a mesh part (`export|lod0|part_1_Wood`) shows in a state of a blueprint's destructible parts: a part whose
/// name starts with a health state's mesh name (the longest that fits: `ruin_downed_Concrete` is `ruin_downed`'s, not
/// `ruin`'s) shows when that is its component's mesh in the state; a part no state names always shows.
#[must_use]
pub fn part_shown(components: &[Component], part: &str, state: State) -> bool {
    if state == State::All || components.is_empty() {
        return true;
    }
    let base = part.rsplit('|').next().unwrap_or(part).to_lowercase();
    let fits = |m: &str| !m.is_empty() && (base == m || base.starts_with(&format!("{m}_")));
    let owner = components
        .iter()
        .flat_map(|c| c.states.iter().map(move |(_, m)| (c, m)))
        .filter(|(_, m)| fits(m))
        .max_by_key(|(_, m)| m.len());
    let Some((c, m)) = owner else { return true };
    chosen(c, state).is_some_and(|s| s == m)
}

/// A vertex layout's elements: (format, usage, offset, stream), and its stride.
fn layout_elements(layout: &[u8]) -> Vec<(u8, u8, u8, u8)> {
    let count = layout.get(64).copied().map_or(0, usize::from);
    (0..count.min(16))
        .filter_map(|i| {
            let e = layout.get(4 * i..4 * i + 4)?;
            Some((*e.first()?, *e.get(1)?, *e.get(2)?, *e.get(3)?))
        })
        .collect()
}

/// A vertex element's first `n` values (float formats: 2 - 4 floats, 6 - 8 halves; 0x0C four bytes).
fn element(data: &[u8], at: usize, fmt: u8, n: usize) -> Option<[f32; 4]> {
    let mut v = [0.0f32; 4];
    let (count, size) = match fmt {
        2 => (2, 4),
        3 => (3, 4),
        4 => (4, 4),
        6 => (2, 2),
        7 => (3, 2),
        8 => (4, 2),
        0x0C => (4, 1),
        _ => return None,
    };
    if at + count * size > data.len() {
        return None;
    }
    for (k, slot) in v.iter_mut().enumerate().take(count.min(n)) {
        let o = at + k * size;
        *slot = match size {
            4 => f32_at(data, o),
            2 => half(u16::from_le_bytes([*data.get(o)?, *data.get(o + 1)?])),
            _ => f32::from(*data.get(o)?),
        };
    }
    Some(v)
}

/// Texture name endings DICE's art uses for colour textures.
const DIFFUSE: [&str; 8] = ["_d", "_diffuse", "_dif", "_diff", "_c", "_color", "_colour", "_albedo"];
/// ... and for the others (normal, specular, mask, height, environment, detail maps).
const OTHER: [&str; 14] = [
    "_n",
    "_nm",
    "_normal",
    "_nrm",
    "_s",
    "_spec",
    "_specular",
    "_m",
    "_mask",
    "_h",
    "_height",
    "_e",
    "_env",
    "_detail",
];

/// The texture the simplified view draws a part with: a name the game's art calls diffuse or colour; else, on DICE's
/// layered building materials (`GuardTower_Oc_1` an occlusion mask, then `Dt_` detail layers), the base detail layer
/// (`Dt_Planks_Old_001`, not the `Dt_Dirt` / `Dt_Rust` overlays); else the first colour texture bound (not a normal
/// map, a mask, an occlusion map, a cube or an engine texture).
fn diffuse_name(textures: &[(u32, String)]) -> Option<String> {
    let leaf = |n: &str| n.rsplit('/').next().unwrap_or(n).to_lowercase();
    let mut sorted: Vec<&(u32, String)> = textures.iter().filter(|(_, n)| !n.starts_with("engine:")).collect();
    sorted.sort_by_key(|(slot, _)| *slot);
    let overlay = |l: &str| ["dirt", "rust", "scratch", "soot"].iter().any(|w| l.contains(w));
    let colour = |l: &str| {
        !OTHER.iter().any(|s| l.ends_with(s))
            && !l.starts_with("nm_")
            && !l.starts_with("msk_")
            && !l.contains("_oc")
            && !l.contains("mask")
            && !l.contains("normal")
            && !l.contains("envmap")
    };
    let find = |f: &dyn Fn(&str) -> bool| sorted.iter().find(|(_, n)| f(&leaf(n))).map(|(_, n)| n.clone());
    find(&|l| DIFFUSE.iter().any(|s| l.ends_with(s)))
        .or_else(|| find(&|l| l.starts_with("dt_") && !overlay(l)))
        .or_else(|| find(&|l| l.starts_with("dt_")))
        .or_else(|| find(&colour))
}

/// A mesh's draws (prepared at level of detail `lod`) as plain geometry; the diffuse textures read through
/// `textures` (each once).
#[must_use]
#[allow(clippy::implicit_hasher, reason = "the loader's own cache")]
pub fn simple_mesh(
    files: &Files,
    db: &Db,
    pr: &Prepared,
    lod: usize,
    textures: &Mutex<HashMap<String, Option<Arc<Picture>>>>,
) -> SimpleMesh {
    let low = pr.mesh.to_lowercase();
    let mut out = SimpleMesh {
        mesh: pr.mesh.clone(),
        lod,
        ..SimpleMesh::default()
    };
    let isz = if pr.wide { 4 } else { 2 };
    for d in &pr.draws {
        let key = query::lower(format!("{low}_lod{lod}/{}/standard", d.part).as_bytes()).into_owned();
        let Some(layout) = db.parts.get(&key).and_then(|g| db.layouts.get(g)) else {
            continue;
        };
        let els = layout_elements(layout);
        let pos = els.iter().find(|e| e.1 == 1 && e.3 == 0).copied();
        let Some(pos) = pos else { continue };
        let zero: Vec<&(u8, u8, u8, u8)> = els.iter().filter(|e| e.1 == 0 && e.3 == 0).collect();
        let uv = zero
            .iter()
            .find(|e| matches!(e.0, 2 | 6))
            .or_else(|| zero.iter().find(|e| matches!(e.0, 3 | 4 | 7 | 8)))
            .map(|e| **e);
        let bone = els.iter().find(|e| e.1 == 2 && e.3 == 0).copied();
        let bones = d.bones.as_ref().filter(|_| bone.is_some());
        let idx: Vec<u32> = (0..d.index_count as usize)
            .map(|i| {
                let at = (d.first_index as usize + i) * isz;
                if pr.wide {
                    u32_at(&pr.indices, at)
                } else {
                    pr.indices.get(at..at + 2).map_or(0, |b| {
                        u32::from(u16::from_le_bytes([
                            b.first().copied().unwrap_or(0),
                            b.get(1).copied().unwrap_or(0),
                        ]))
                    })
                }
            })
            .collect();
        let Some(&top) = idx.iter().max() else { continue };
        let base = out.positions.len() as u32;
        let stride = d.stride as usize;
        let mut ok = true;
        for v in 0..=top as usize {
            let at = d.vertex_offset as usize + v * stride;
            let Some(p) = element(&pr.vertices, at + usize::from(pos.2), pos.0, 3) else {
                ok = false;
                break;
            };
            let mut p = [p[0], p[1], p[2]];
            if let (Some(b), Some(bones)) = (bone, bones)
                && let Some(i) = element(&pr.vertices, at + usize::from(b.2), b.0, 1)
            {
                let k = i[0] as usize;
                if let (Some(r0), Some(r1), Some(r2)) = (bones.get(3 * k), bones.get(3 * k + 1), bones.get(3 * k + 2)) {
                    let row = |r: &[f32; 4]| r[0] * p[0] + r[1] * p[1] + r[2] * p[2] + r[3];
                    p = [row(r0), row(r1), row(r2)];
                }
            }
            let t = uv
                .and_then(|u| element(&pr.vertices, at + usize::from(u.2), u.0, 2))
                .map_or([0.0, 0.0], |t| [t[0], t[1]]);
            out.positions.push(p);
            out.uvs.push(t);
        }
        if !ok {
            out.positions.truncate(base as usize);
            out.uvs.truncate(base as usize);
            continue;
        }
        let first = out.indices.len() as u32;
        out.indices.extend(idx.iter().map(|i| i + base));
        let texture_name = diffuse_name(&d.textures);
        let texture = texture_name.as_ref().and_then(|n| {
            if let Some(t) = textures.lock().unwrap_or_else(PoisonError::into_inner).get(n) {
                return t.clone();
            }
            let pic = files
                .read(&format!("{n}.res"))
                .and_then(|b| decode_texture(&b, TEXTURE_WIDTH))
                .map(Arc::new);
            textures
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(n.clone(), pic.clone());
            pic
        });
        let tested = d.class == TESTED;
        let alpha_ref = d.state.map(|s| s.alpha_ref as u8).filter(|&r| r > 0).unwrap_or(128);
        // a mean with a channel near zero and another strong is a channel mask (`Pallet_t01v01_C`: 179, 0, 0), not a
        // colour: no tint
        let tint = texture
            .as_ref()
            .and_then(|t| mean_colour(t, if tested { alpha_ref } else { 0 }))
            .filter(|c| !(c.iter().min().is_some_and(|&v| v < 12) && c.iter().max().is_some_and(|&v| v > 90)))
            .map(|c| {
                // DICE's tiling detail layers (`Dt_Sandbags`, `Dt_Rust_001`) are drawn over the object's own colour
                // mask: their hue is a hint, kept at 40 %
                let detail = texture_name
                    .as_deref()
                    .is_some_and(|n| n.rsplit('/').next().unwrap_or(n).to_lowercase().starts_with("dt_"));
                if !detail {
                    return c;
                }
                let lum = (u32::from(c[0]) * 54 + u32::from(c[1]) * 183 + u32::from(c[2]) * 19) / 256;
                c.map(|v| ((lum * 6 + u32::from(v) * 4) / 10) as u8)
            });
        out.parts.push(Part {
            name: d.part.clone(),
            first,
            count: idx.len() as u32,
            tested,
            blended: d.class == BLENDED,
            alpha_ref,
            texture_name,
            texture,
            tint,
        });
    }
    out
}

/// The mean colour of the texels whose alpha is at least `min_alpha`.
#[must_use]
pub fn mean_colour(p: &Picture, min_alpha: u8) -> Option<[u8; 3]> {
    let mut sum = [0u64; 3];
    let mut n = 0u64;
    for t in p.rgba.as_chunks::<4>().0 {
        if t[3] >= min_alpha {
            for (s, v) in sum.iter_mut().zip(t) {
                *s += u64::from(*v);
            }
            n += 1;
        }
    }
    (n > 0).then(|| sum.map(|s| (s / n) as u8))
}

/// A PC `ITexture` (with or without its `RES` header) decoded at its first level no wider or taller than `max_w`
/// (the smallest when all are larger), alpha kept: DXT1 (1-bit alpha), DXT3, DXT5, B8G8R8A8 and L8. `None` for a cube, a
/// normal map (BC5) or another format.
#[must_use]
pub fn decode_texture(data: &[u8], max_w: u32) -> Option<Picture> {
    let pc = if data.starts_with(b"RES") {
        data.get(64..)?
    } else {
        data
    };
    let (typ, fmt, w, h, mips) = (
        u32_at(pc, 4),
        u32_at(pc, 8),
        u32_at(pc, 16),
        u32_at(pc, 20),
        u32_at(pc, 28),
    );
    if typ == 1 || w == 0 || h == 0 || !(1..=15).contains(&mips) {
        return None;
    }
    let mut at = 92usize;
    let mut pick = None;
    for m in 0..mips {
        let size = u32_at(pc, 32 + 4 * m as usize) as usize;
        let (lw, lh) = ((w >> m).max(1), (h >> m).max(1));
        pick = Some((at, size, lw, lh));
        if lw.max(lh) <= max_w {
            break;
        }
        at += size;
    }
    let (at, size, lw, lh) = pick?;
    let level = pc.get(at..at + size)?;
    let rgba = decode_level(level, lw as usize, lh as usize, fmt)?;
    Some(Picture { size: [lw, lh], rgba })
}

fn rgb565(c: u16) -> [u8; 3] {
    let r = u32::from((c >> 11) & 31);
    let g = u32::from((c >> 5) & 63);
    let b = u32::from(c & 31);
    [(r * 255 / 31) as u8, (g * 255 / 63) as u8, (b * 255 / 31) as u8]
}

/// A colour block's 16 texels (`punch`: DXT1's three-colour mode makes index 3 transparent).
#[allow(clippy::indexing_slicing, reason = "a block of 8 bytes, palettes of 4")]
fn colour_block(b: &[u8], punch: bool) -> [[u8; 4]; 16] {
    let c0 = u16::from_le_bytes([b[0], b[1]]);
    let c1 = u16::from_le_bytes([b[2], b[3]]);
    let (a, z) = (rgb565(c0), rgb565(c1));
    let mix = |p: u16, q: u16, d: u16| -> [u8; 4] {
        let f = |k: usize| ((u16::from(a[k]) * p + u16::from(z[k]) * q) / d) as u8;
        [f(0), f(1), f(2), 255]
    };
    let pal: [[u8; 4]; 4] = if c0 > c1 || !punch {
        [
            [a[0], a[1], a[2], 255],
            [z[0], z[1], z[2], 255],
            mix(2, 1, 3),
            mix(1, 2, 3),
        ]
    } else {
        [
            [a[0], a[1], a[2], 255],
            [z[0], z[1], z[2], 255],
            mix(1, 1, 2),
            [0, 0, 0, 0],
        ]
    };
    let bits = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    std::array::from_fn(|k| pal[((bits >> (2 * k)) & 3) as usize])
}

/// A DXT5 alpha block's 16 values.
#[allow(clippy::indexing_slicing, reason = "a block of 8 bytes, a palette of 8")]
fn alpha_block(b: &[u8]) -> [u8; 16] {
    let (a0, a1) = (u32::from(b[0]), u32::from(b[1]));
    let pal: [u8; 8] = if a0 > a1 {
        std::array::from_fn(|k| match k {
            0 => a0 as u8,
            1 => a1 as u8,
            _ => (((8 - k as u32) * a0 + (k as u32 - 1) * a1) / 7) as u8,
        })
    } else {
        std::array::from_fn(|k| match k {
            0 => a0 as u8,
            1 => a1 as u8,
            6 => 0,
            7 => 255,
            _ => (((6 - k as u32) * a0 + (k as u32 - 1) * a1) / 5) as u8,
        })
    };
    let mut bits = 0u64;
    for k in 0..6 {
        bits |= u64::from(b[2 + k]) << (8 * k);
    }
    std::array::from_fn(|k| pal[((bits >> (3 * k)) & 7) as usize])
}

#[allow(
    clippy::indexing_slicing,
    reason = "blocks are cut to their sizes, texels within the level"
)]
fn decode_level(level: &[u8], w: usize, h: usize, fmt: u32) -> Option<Vec<u8>> {
    let mut out = vec![0u8; w * h * 4];
    match fmt {
        9 => {
            let src = level.get(..w * h * 4)?;
            for (d, s) in out.as_chunks_mut::<4>().0.iter_mut().zip(src.as_chunks::<4>().0) {
                *d = [s[2], s[1], s[0], s[3]];
            }
        }
        10 => {
            let src = level.get(..w * h)?;
            for (d, s) in out.as_chunks_mut::<4>().0.iter_mut().zip(src) {
                *d = [*s, *s, *s, 255];
            }
        }
        0 | 18 | 1 | 2 => {
            let bpb = if matches!(fmt, 0 | 18) { 8 } else { 16 };
            let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
            let data = level.get(..bw * bh * bpb)?;
            for (i, blk) in data.chunks_exact(bpb).enumerate() {
                let texels: [[u8; 4]; 16] = match fmt {
                    0 | 18 => colour_block(blk, true),
                    1 => {
                        let mut t = colour_block(&blk[8..], false);
                        for (k, px) in t.iter_mut().enumerate() {
                            let nib = (blk[k / 2] >> (4 * (k % 2))) & 15;
                            px[3] = nib * 17;
                        }
                        t
                    }
                    _ => {
                        let mut t = colour_block(&blk[8..], false);
                        let a = alpha_block(&blk[..8]);
                        for (px, a) in t.iter_mut().zip(a) {
                            px[3] = a;
                        }
                        t
                    }
                };
                let (by, bx) = (i / bw, i % bw);
                for (k, px) in texels.iter().enumerate() {
                    let (x, y) = (bx * 4 + k % 4, by * 4 + k / 4);
                    if x < w && y < h {
                        out[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(px);
                    }
                }
            }
        }
        _ => return None,
    }
    Some(out)
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::unwrap_used)]
mod tests {
    use super::*;

    fn texture(fmt: u32, w: u32, h: u32, levels: &[Vec<u8>]) -> Vec<u8> {
        let mut pc = vec![0u8; 92];
        for (k, v) in [(2, fmt), (4, w), (5, h), (7, levels.len() as u32)] {
            pc[4 * k..4 * k + 4].copy_from_slice(&v.to_le_bytes());
        }
        for (m, l) in levels.iter().enumerate() {
            pc[32 + 4 * m..36 + 4 * m].copy_from_slice(&(l.len() as u32).to_le_bytes());
        }
        for l in levels {
            pc.extend(l);
        }
        pc
    }

    #[test]
    fn dxt1_keeps_its_one_bit_alpha() {
        // c0 <= c1: three colours and transparent; every texel index 3
        let blk = [0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let p = decode_texture(&texture(0, 4, 4, &[blk.to_vec()]), 64).unwrap();
        assert_eq!(p.size, [4, 4]);
        assert!(p.rgba.as_chunks::<4>().0.iter().all(|t| t[3] == 0));
        // c0 > c1: four opaque colours, index 0 white
        let blk = [0xFF, 0xFF, 0x00, 0x00, 0, 0, 0, 0];
        let p = decode_texture(&texture(0, 4, 4, &[blk.to_vec()]), 64).unwrap();
        assert_eq!(&p.rgba[..4], &[255, 255, 255, 255]);
        assert_eq!(mean_colour(&p, 1), Some([255, 255, 255]));
    }

    #[test]
    fn dxt5_alpha_and_the_level_no_wider_than_asked() {
        // alpha 255 / 0 by index 0 / 1, all texels index 1 (transparent); colour white
        let mut blk = vec![255u8, 0, 0x49, 0x92, 0x24, 0x49, 0x92, 0x24];
        blk.extend([0xFF, 0xFF, 0x00, 0x00, 0, 0, 0, 0]);
        let big = vec![0u8; 16 * 4];
        let pc = texture(2, 8, 8, &[big, blk]);
        let p = decode_texture(&pc, 4).unwrap();
        assert_eq!(p.size, [4, 4]);
        assert!(p.rgba.as_chunks::<4>().0.iter().all(|t| t[3] == 0));
        assert_eq!(mean_colour(&p, 128), None);
        // a cube, BC5 and a short file are not decoded
        let mut cube = pc.clone();
        cube[4] = 1;
        assert!(decode_texture(&cube, 4).is_none());
        assert!(decode_texture(&texture(19, 4, 4, &[vec![0; 16]]), 4).is_none());
        assert!(decode_texture(b"short", 4).is_none());
    }

    #[test]
    fn the_diffuse_texture_by_its_name() {
        let t = |v: &[(u32, &str)]| v.iter().map(|(s, n)| (*s, (*n).to_owned())).collect::<Vec<_>>();
        assert_eq!(
            diffuse_name(&t(&[(0, "a/rock_n"), (1, "a/rock_d"), (2, "a/rock_s")])).as_deref(),
            Some("a/rock_d")
        );
        assert_eq!(
            diffuse_name(&t(&[(2, "a/leaf_normal"), (1, "a/leaf_alpha")])).as_deref(),
            Some("a/leaf_alpha")
        );
        assert_eq!(diffuse_name(&t(&[(0, "engine:x"), (1, "a/b_n")])), None);
        // DICE's layered building materials: the occlusion mask, the overlays, then the base detail layer
        assert_eq!(
            diffuse_name(&t(&[
                (2, "o/GuardTower_Oc_1"),
                (3, "t/Dt_Dirt_001"),
                (4, "t/Dt_Planks_Old_001"),
                (5, "t/Nm_Planks_Old_001"),
            ]))
            .as_deref(),
            Some("t/Dt_Planks_Old_001")
        );
        assert_eq!(
            diffuse_name(&t(&[(2, "o/Barrack_Oc_1"), (3, "t/Dt_Rust_001")])).as_deref(),
            Some("t/Dt_Rust_001")
        );
    }

    #[test]
    fn destruction_states_pick_their_parts() {
        // SP_01's guard tower (`objects/buildings/guardtower`): its three parts' health states
        let c = |name: &str, s: &[(f32, &str)]| Component {
            name: name.into(),
            states: s.iter().map(|(h, m)| (*h, (*m).to_owned())).collect(),
        };
        let parts = [
            c("part_1", &[(500.0, "part_1"), (400.0, "part_1_dest"), (0.0, "")]),
            c("ruin", &[(250.0, "ruin"), (0.0, "ruin_downed")]),
        ];
        let shown = |p: &str, s: State| part_shown(&parts, p, s);
        assert!(shown("export|lod0|part_1_Wood", State::Intact));
        assert!(!shown("export|lod0|part_1_dest_Metal", State::Intact));
        assert!(shown("export|lod0|ruin_Concrete", State::Intact));
        assert!(!shown("export|lod0|ruin_downed_Concrete_LOD2", State::Intact));
        assert!(!shown("export|lod0|part_1_Wood", State::Destroyed));
        assert!(!shown("export|lod0|ruin_Concrete", State::Destroyed));
        assert!(shown("export|lod0|ruin_downed_Concrete_LOD2", State::Destroyed));
        // a part no state names, and every part with All
        assert!(shown("lod0|window_Glass", State::Destroyed));
        assert!(shown("export|lod0|part_1_dest_Metal", State::All));
        // round VW1: the steps between, and the collision parts by their names' hashes
        assert_eq!(stage_count(&parts), 3);
        assert!(shown("export|lod0|part_1_dest_Metal", State::Stage(1)));
        assert!(shown("export|lod0|ruin_downed_Concrete_LOD2", State::Stage(1)));
        assert!(!shown("export|lod0|part_1_dest_Metal", State::Stage(2)));
        let h = bc2_formats::havok::part_hash;
        let col = |p: &str, s: State| collision_part_shown(&parts, h(p), s);
        assert!(col("part_1", State::Intact) && !col("part_1_dest", State::Intact) && col("ruin", State::Intact));
        assert!(col("part_1_dest", State::Stage(1)) && col("ruin_downed", State::Stage(1)));
        assert!(!col("part_1", State::Destroyed) && !col("part_1_dest", State::Destroyed));
        assert!(col("ruin_downed", State::Destroyed) && col("window", State::Destroyed));
        assert!(col("part_1_dest", State::All));
    }
}
