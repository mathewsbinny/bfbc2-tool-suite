//! The offline sampler's level side (`navsample_offline.py`: `level_geometry`, `destroyed_parts`, `part_names`,
//! `load_level`, `run_level`, `detailed_rects`, `sample_level`, `main`'s `like` / `validate`): a prepared level's
//! collision and terrain read, its runs sampled and written as the in-game sampler names them.
//!
//! What is new against the reference: the detailed run can take DICE's own detailed sectors besides the level's own
//! choice (KYG-DET-1: [`Detailed::WithDice`]), and a vehicle-shaped run ([`super::Body::vehicle`], KYG-PAR-2) at
//! the vehicle pitch.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use rayon::prelude::*;
use regex::Regex;

use super::{Body, Geometry, Grid, KIND_CONVEX, Samples, Stats};
use crate::hulls::{self, Piece};
use crate::inputs::{self, Root, ToolRun};
use crate::ntpath::{self as os, Fail, Res, join, join2, navkit_error, raise};
use crate::offline::Field;
use crate::pyjson::{self, Value};

/// The rigid-body names whose collision the AI queries see (`BODIES`: 'h' and 'c'; 'e' bodies are another layer).
pub const BODIES: &str = "hc";
/// A detailed sector's square grows by this much on each side (`detailed_rects`' `margin`).
pub const DETAIL_MARGIN: f64 = 1.0;

/// `_xml_text(path)`: the file as Python's text mode reads it (UTF-8, errors replaced, universal newlines); empty
/// when it cannot be read.
fn xml_text(path: &str) -> String {
    std::fs::read(path).map_or_else(|_| String::new(), |d| os::universal(&String::from_utf8_lossy(&d)))
}

/// `part_names(havok_res)`: the asset's `PartInfoDatas` names, lower case, in order (from the converted
/// `<name>_Havok.xml` next to the `.res`).
#[must_use]
pub fn part_names(havok_res: &str) -> Vec<String> {
    let stem = havok_res.get(..havok_res.len().saturating_sub(4)).unwrap_or_default();
    let t = xml_text(&format!("{stem}.xml"));
    let open = "<array name=\"PartInfoDatas\">";
    let Some((_, body)) = t.split_once(open) else {
        return Vec::new();
    };
    let body = body.split_once("</array>").map_or(body, |(b, _)| b);
    #[allow(clippy::unwrap_used, reason = "a fixed pattern")]
    let re = Regex::new(r#"<complex>\s*<field name="Name">([^<]*)</field>"#).unwrap();
    re.captures_iter(body).map(|c| c[1].to_lowercase()).collect()
}

/// `destroyed_parts(blueprint_xml)`: the mesh parts a blueprint shows only once damaged - the `MeshName`s of its
/// `PartComponentData` health states other than each component's full-health one (the highest `Health`), minus the
/// full-health ones.
///
/// # Errors
/// A `Health` the reference's `float()` refuses (its `ValueError`).
#[allow(clippy::unwrap_used, reason = "fixed patterns")]
pub fn destroyed_parts(blueprint_xml: &str) -> Res<BTreeSet<String>> {
    let t = xml_text(blueprint_xml);
    if !t.contains("PartComponentData") {
        return Ok(BTreeSet::new());
    }
    let inst =
        Regex::new(r#"(?s)<instance guid="([^"]+)" type="GameSharedResources.HealthStateData">(.*?)</instance>"#)
            .unwrap();
    let mesh = Regex::new(r#"<field name="MeshName">([^<]*)</field>"#).unwrap();
    let health = Regex::new(r#"<field name="Health">([-0-9.eE]+)</field>"#).unwrap();
    let comp = Regex::new(r#"(?s)type="GameSharedResources.PartComponentData">(.*?)</instance>"#).unwrap();
    let hs = Regex::new(r#"(?s)<array name="HealthStates">(.*?)</array>"#).unwrap();
    let item = Regex::new(r#"<item ref="([^"]+)""#).unwrap();
    // states[guid] = (mesh, health): a later instance of the same guid replaces the earlier (a dict)
    let mut states: Vec<(String, f64)> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    for c in inst.captures_iter(&t) {
        let body = &c[2];
        let mn = mesh.captures(body).map(|m| m[1].to_lowercase()).unwrap_or_default();
        let hp = match health.captures(body) {
            Some(h) => py_float(&h[1])?,
            None => 0.0,
        };
        let k = c[1].to_lowercase();
        if let Some(&i) = at.get(&k) {
            states[i] = (mn, hp);
        } else {
            at.insert(k, states.len());
            states.push((mn, hp));
        }
    }
    let (mut shown, mut later) = (BTreeSet::new(), BTreeSet::new());
    for c in comp.captures_iter(&t) {
        let refs: Vec<usize> = hs
            .captures(&c[1])
            .map(|h| {
                item.captures_iter(&h[1])
                    .filter_map(|r| at.get(&r[1].to_lowercase()).copied())
                    .collect()
            })
            .unwrap_or_default();
        if refs.is_empty() {
            continue;
        }
        // max(st, key=health): the first of the highest
        let mut top = refs[0];
        for &r in &refs {
            if states[r].1 > states[top].1 {
                top = r;
            }
        }
        if !states[top].0.is_empty() {
            shown.insert(states[top].0.clone());
        }
        // `s is not top`: the same state object (the same guid) is top
        later.extend(
            refs.iter()
                .filter(|&&r| r != top && !states[r].0.is_empty())
                .map(|&r| states[r].0.clone()),
        );
    }
    Ok(later.difference(&shown).cloned().collect())
}

/// Python's `float(text)` for the regex's `[-0-9.eE]+` texts.
fn py_float(s: &str) -> Res<f64> {
    s.parse::<f64>()
        .ok()
        .filter(|_| !s.eq_ignore_ascii_case("inf") && !s.eq_ignore_ascii_case("nan"))
        .ok_or_else(|| {
            raise(
                "ValueError",
                format!("could not convert string to float: {}", os::str_repr(s)),
            )
        })
}

/// One placed object of `objects.json` / `group_instances.json`: its blueprint and matrix.
fn placed(v: &Value) -> Res<(String, [f64; 16])> {
    let bp = v.get("bp").and_then(Value::as_str).ok_or(Fail::Handoff)?.to_owned();
    let m = v.get("m").and_then(Value::as_list).ok_or(Fail::Handoff)?;
    let mut out = [0.0; 16];
    if m.len() != 16 {
        return Err(Fail::Handoff);
    }
    for (o, x) in out.iter_mut().zip(m) {
        *o = match x {
            Value::Float(f) => *f,
            Value::Int(i) => *i as f64,
            Value::Bool(b) => f64::from(u8::from(*b)),
            _ => return Err(Fail::Handoff),
        };
    }
    Ok((bp, out))
}

/// One blueprint's pieces with their destroyed-state flag (`cache[bp]`).
type Cached = Vec<(Piece, bool)>;

/// What [`level_geometry`] read (its log line's counts).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GeometryInfo {
    /// Placed objects.
    pub objects: usize,
    /// Static-group members.
    pub members: usize,
    /// Triangles before the body and intact selection.
    pub triangles: usize,
    /// Of them convex.
    pub convex: usize,
    /// Of them in destroyed-state parts.
    pub destroyed: usize,
    /// Triangles per body letter.
    pub bodies: Vec<(char, usize)>,
    /// Blueprints without a collision asset.
    pub missing: usize,
    /// Packfiles the Rust reader hands to the reference (`hulls::asset_bodies` refused them): their bodies are left
    /// out here (none on the 15 regression levels).
    pub refused: Vec<String>,
}

impl GeometryInfo {
    /// The reference's log line.
    #[must_use]
    pub fn line(&self) -> String {
        let bodies: Vec<String> = self
            .bodies
            .iter()
            .map(|(c, n)| format!("{c} {}", thousands(*n)))
            .collect();
        format!(
            "  geometry: {} objects, {} group members, {} triangles ({} convex, {} in destroyed-state parts); bodies \
             {}; {} blueprints without a collision asset",
            self.objects,
            self.members,
            thousands(self.triangles),
            thousands(self.convex),
            thousands(self.destroyed),
            bodies.join(", "),
            self.missing
        )
    }
}

/// `f'{n:,}'`.
#[must_use]
pub fn thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (k, ch) in s.chars().enumerate() {
        if k > 0 && (s.len() - k).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// `level_geometry(level_dir, extra_bases, box)` then `.bodies(letters).intact()`: the collision of a prepared level
/// (`objects.json`, `group_instances.json`, the unpacked bundles' `*_Havok.res`), in the world, the triangles of
/// bodies named by `letters` that are there while the objects are intact; `bx` (minX, minZ, maxX, maxZ): only objects
/// within 300 m of it and triangles reaching into it (plus 2 m).
///
/// # Errors
/// The object lists missing or damaged, a blueprint's `Health` that is not a number.
#[allow(clippy::too_many_lines, reason = "the reference's one function")]
pub fn level_geometry(
    level_dir: &str,
    extra_bases: &[String],
    bx: Option<[f64; 4]>,
    letters: &str,
) -> Res<(Geometry, GeometryInfo)> {
    let mut bases = vec![join2(level_dir, "pkg")?, join2(level_dir, "dist")?];
    bases.extend(extra_bases.iter().cloned());
    let index = bc2_scene::collision::index(&bases);
    let havok = index.resolved().unwrap_or_else(|_| index.havok.clone());
    let load = |name: &str| -> Res<Vec<Value>> {
        let p = join2(level_dir, name)?;
        let d = os::read(&p)?;
        match pyjson::loads_utf8(&d) {
            Some(Value::List(l)) => Ok(l),
            _ => Err(raise("JSONDecodeError", format!("{p}: not a list"))),
        }
    };
    let objs = load("objects.json")?;
    let groups = load("group_instances.json")?;
    let all: Vec<(String, [f64; 16])> = objs.iter().chain(&groups).map(placed).collect::<Res<_>>()?;
    // each blueprint's bodies once, on all cores
    let mut wanted: Vec<&str> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (bp, _) in &all {
        if seen.insert(bp.as_str()) {
            wanted.push(bp);
        }
    }
    let got: Vec<(String, Option<Res<Cached>>, bool)> = wanted
        .par_iter()
        .map(|&bp| {
            let Some(path) = havok.get(bp) else {
                return (bp.to_owned(), None, false);
            };
            let path = path.to_string_lossy().into_owned();
            let Ok(data) = std::fs::read(&path) else {
                return (bp.to_owned(), Some(Ok(Vec::new())), false);
            };
            let Some(pieces) = hulls::asset_bodies(&data) else {
                return (path, Some(Ok(Vec::new())), true);
            };
            if pieces.is_empty() {
                return (bp.to_owned(), Some(Ok(Vec::new())), false);
            }
            let r = (|| -> Res<Cached> {
                let mut later = BTreeSet::new();
                for base in &bases {
                    let x = format!(
                        "{}{}{bp}.xml",
                        base,
                        if base.ends_with(['\\', '/']) { "" } else { "\\" }
                    );
                    if os::isfile(&x) {
                        later = destroyed_parts(&x)?;
                        break;
                    }
                }
                let names = if later.is_empty() {
                    Vec::new()
                } else {
                    part_names(&path)
                };
                let gone: BTreeSet<i32> = names
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| later.contains(*n))
                    .map(|(k, _)| k as i32)
                    .collect();
                Ok(pieces
                    .into_iter()
                    .map(|p| {
                        let off = gone.contains(&p.part);
                        (p, off)
                    })
                    .collect())
            })();
            (bp.to_owned(), Some(r), false)
        })
        .collect();
    let mut info = GeometryInfo {
        objects: objs.len(),
        members: groups.len(),
        ..GeometryInfo::default()
    };
    let mut cache: HashMap<&str, Cached> = HashMap::new();
    for ((key, r, refused), &bp) in got.into_iter().zip(&wanted) {
        if refused {
            info.refused.push(key);
        }
        match r {
            None => {
                info.missing += 1;
                cache.insert(bp, Vec::new());
            }
            Some(r) => {
                cache.insert(bp, r?);
            }
        }
    }
    let mut geo = Geometry::default();
    let mut per_body: std::collections::BTreeMap<u8, usize> = std::collections::BTreeMap::new();
    let mut solid_id = 0i32;
    let keep_letter = |c: u8| letters.as_bytes().contains(&c);
    for (bp, m) in &all {
        let pieces = cache.get(bp.as_str()).map_or(&[][..], Vec::as_slice);
        if pieces.is_empty() {
            continue;
        }
        if let Some(b) = bx {
            let (px, pz) = (m[12], m[14]);
            if !(b[0] - 300.0 <= px && px <= b[2] + 300.0 && b[1] - 300.0 <= pz && pz <= b[3] + 300.0) {
                continue;
            }
        }
        for (piece, gone) in pieces {
            let letter = piece.name.chars().next().map_or(b'?', |c| c as u32 as u8);
            let mut kept: Vec<super::geom::Tri> = Vec::new();
            for t in &piece.tris {
                let w = t.map(|p| {
                    [0, 1, 2]
                        .map(|j| (libm::fma(p[2], m[8 + j], libm::fma(p[1], m[4 + j], p[0] * m[j])) + 0.0) + m[12 + j])
                });
                if let Some(b) = bx {
                    let xs = [w[0][0], w[1][0], w[2][0]];
                    let zs = [w[0][2], w[1][2], w[2][2]];
                    if !(super::max3(xs) >= b[0] - 2.0
                        && super::min3(xs) <= b[2] + 2.0
                        && super::max3(zs) >= b[1] - 2.0
                        && super::min3(zs) <= b[3] + 2.0)
                    {
                        continue;
                    }
                }
                kept.push(w);
            }
            if kept.is_empty() {
                continue;
            }
            let n = kept.len();
            info.triangles += n;
            *per_body.entry(letter).or_default() += n;
            if piece.kind == KIND_CONVEX {
                info.convex += n;
            }
            if *gone {
                info.destroyed += n;
            }
            let sid = if piece.kind == KIND_CONVEX {
                solid_id += 1;
                solid_id - 1
            } else {
                -1
            };
            if !keep_letter(letter) || *gone {
                continue;
            }
            geo.kind.extend(std::iter::repeat_n(piece.kind, n));
            geo.solid.extend(std::iter::repeat_n(sid, n));
            geo.vt.extend(std::iter::repeat_n(piece.root_kind, n));
            geo.tris.extend(kept);
        }
    }
    info.bodies = per_body.into_iter().map(|(c, n)| (char::from(c), n)).collect();
    Ok((geo, info))
}

/// `common_bases(root)`: `work\navkit\common\sp_common`, `mp_common`.
///
/// # Errors
/// A path the join refuses.
pub fn common_bases(r: &Root) -> Res<Vec<String>> {
    ["sp_common", "mp_common"]
        .iter()
        .map(|c| join(&r.work, &["navkit", "common", c]))
        .collect()
}

/// A level's height field as the sampler reads it: the heights (`float32`), the size, min and cell.
#[derive(Debug, Clone, PartialEq)]
pub struct Terrain {
    /// `(size, size)` heights, row `j` (z) then column `i` (x).
    pub heights: Vec<f32>,
    /// Samples per side.
    pub size: usize,
    /// `min[0]`, `min[2]`.
    pub min: (f64, f64),
    /// Metres per sample along x and z.
    pub cell: (f64, f64),
}

impl Terrain {
    /// A terrain height field file's terrain.
    #[must_use]
    pub fn of(hf: &bc2_formats::terrain::Heightfield) -> Self {
        Terrain {
            heights: hf.heights(),
            size: hf.size as usize,
            min: (f64::from(hf.min[0]), f64::from(hf.min[2])),
            cell: hf.cell(),
        }
    }

    /// The sampler's view of it.
    #[must_use]
    pub fn field(&self) -> Field<'_> {
        Field {
            heights: &self.heights,
            size: self.size,
            min: self.min,
            cell: self.cell,
        }
    }
}

/// `load_level(root, lv, box, bodies)`: the level's geometry (the body letters, intact parts) and terrain.
///
/// # Errors
/// The level not prepared, its object lists damaged, the terrain unreadable.
pub fn load_level(
    r: &Root,
    lv: &str,
    bx: Option<[f64; 4]>,
    letters: &str,
    tool: ToolRun<'_>,
) -> Res<(Geometry, GeometryInfo, Option<Terrain>)> {
    let (geo, info) = level_geometry(&r.level_work(lv)?, &common_bases(r)?, bx, letters)?;
    let terrain = inputs::level_terrain(&r.root, lv, tool)?.map(|h| Terrain::of(&h));
    Ok((geo, info, terrain))
}

/// The four numbers of a settings list (`box`, or `y` with two).
fn numbers(v: Option<&Value>) -> Res<Vec<f64>> {
    let l = v.and_then(Value::as_list).ok_or(Fail::Handoff)?;
    l.iter()
        .map(|x| match x {
            Value::Float(f) => Ok(*f),
            Value::Int(i) => Ok(*i as f64),
            _ => Err(Fail::Handoff),
        })
        .collect()
}

/// A level's sampling settings (`navkit.json`): box, heights, sector offsets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    /// (minX, minZ, maxX, maxZ).
    pub bx: [f64; 4],
    /// (bottom, top).
    pub y: [f64; 2],
    /// Sector offsets.
    pub off: (i64, i64),
}

impl Settings {
    /// The level's `navkit.json`.
    ///
    /// # Errors
    /// Not prepared, or the file damaged.
    pub fn of(r: &Root, lv: &str) -> Res<Self> {
        let s = r.settings(lv)?;
        let b = numbers(s.get("box"))?;
        let y = numbers(s.get("y"))?;
        let int = |k: &str| {
            s.get(k)
                .and_then(Value::as_int)
                .and_then(|i| i64::try_from(i).ok())
                .ok_or(Fail::Handoff)
        };
        match (b.as_slice(), y.as_slice()) {
            ([a, b2, c, d], [y0, y1]) => Ok(Settings {
                bx: [*a, *b2, *c, *d],
                y: [*y0, *y1],
                off: (int("xoff")?, int("zoff")?),
            }),
            _ => Err(Fail::Handoff),
        }
    }
}

/// `detailed_rects`: the sectors' squares plus `margin`, and their box.
#[must_use]
pub fn detailed_rects(ids: &[(i64, i64)], off: (i64, i64), margin: f64) -> (Vec<[f64; 4]>, [f64; 4]) {
    let rects: Vec<[f64; 4]> = ids
        .iter()
        .map(|&(i, j)| {
            let (x, z) = (((i + off.0) * 100) as f64, ((j + off.1) * 100) as f64);
            [x - margin, z - margin, x + 100.0 + margin, z + 100.0 + margin]
        })
        .collect();
    let bx = [
        rects.iter().map(|r| r[0]).fold(f64::INFINITY, f64::min),
        rects.iter().map(|r| r[1]).fold(f64::INFINITY, f64::min),
        rects.iter().map(|r| r[2]).fold(f64::NEG_INFINITY, f64::max),
        rects.iter().map(|r| r[3]).fold(f64::NEG_INFINITY, f64::max),
    ];
    (rects, bx)
}

/// Which detailed sectors a 0.2 m run samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detailed {
    /// The level's own choice (`detailed.choose` on the 0.8 m samples: the reference's).
    Own,
    /// The level's own choice and, on a level with DICE path data, DICE's own detailed sectors (KYG-DET-1).
    WithDice,
}

/// `detailed_choice`: the level's own detailed sectors from its 0.8 m samples (`detailed.choose`: the flagged
/// sectors of `level_dir`'s objects, the common blueprints in `common`, and the stacked sectors of `s08`), sorted.
///
/// # Errors
/// The level's files unreadable.
pub fn own_detailed(level_dir: &str, common: &[String], s08: &str, off: (i64, i64)) -> Res<Vec<(i64, i64)>> {
    let mut chosen = inputs::flagged_sectors(level_dir, common, off.0, off.1)?;
    for (k, n) in inputs::stacked_columns(s08, off.0, off.1)? {
        if n >= inputs::STACKED_MIN {
            chosen.insert(k);
        }
    }
    Ok(chosen.into_iter().collect())
}

/// Where a level's inputs are: a prepared level (`work\levels\<level>`) or an export's stage prepared like one.
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    /// The prepared level's folder (`objects.json`, `group_instances.json`, its unpacked bundles).
    pub level_dir: String,
    /// The other folders whose blueprints its objects use (`common_bases`; a stage's base level first).
    pub extra: Vec<String>,
    /// The common folders the detailed choice reads (`common_bases(root)[::-1]`: mp_common, sp_common).
    pub common: Vec<String>,
    /// The OPFOR-like folder whose `work\levels\<level>\terrain` holds the height field.
    pub terrain_root: String,
    /// The level's sampling settings.
    pub settings: Settings,
    /// DICE's own detailed sectors (empty without DICE path data, or when not asked for).
    pub dice: Vec<(i64, i64)>,
}

impl Source {
    /// A prepared level of `r` (DICE's detailed sectors read when `dice`).
    ///
    /// # Errors
    /// The level not prepared, DICE's path data unreadable.
    pub fn level(r: &Root, lv: &str, dice: bool) -> Res<Self> {
        Ok(Source {
            level_dir: r.level_work(lv)?,
            extra: common_bases(r)?,
            common: common_bases(r)?.into_iter().rev().collect(),
            terrain_root: r.root.clone(),
            settings: Settings::of(r, lv)?,
            dice: if dice {
                inputs::dice_detailed_sectors(r, lv)?
            } else {
                Vec::new()
            },
        })
    }

    /// Its geometry (the body letters, intact parts, within `bx`) and terrain.
    ///
    /// # Errors
    /// The object lists damaged, the terrain unreadable.
    pub fn load(
        &self,
        lv: &str,
        bx: Option<[f64; 4]>,
        letters: &str,
        tool: ToolRun<'_>,
    ) -> Res<(Geometry, GeometryInfo, Option<Terrain>)> {
        let (geo, info) = level_geometry(&self.level_dir, &self.extra, bx, letters)?;
        let terrain = inputs::level_terrain(&self.terrain_root, lv, tool)?.map(|h| Terrain::of(&h));
        Ok((geo, info, terrain))
    }
}

/// The runs' file suffixes and bodies: the normal and the high soldier capsule (`CAP_BOTTOMS`).
#[must_use]
pub fn soldier_runs() -> [(&'static str, Body); 2] {
    [
        ("", Body::soldier(super::BOTTOM)),
        ("_hi", Body::soldier(super::BOTTOM_HIGH)),
    ]
}

/// One sampling run's outcome: the files written and what was counted.
#[derive(Debug, Clone, PartialEq)]
pub struct Written {
    /// The files, in the runs' order.
    pub paths: Vec<String>,
    /// The counts.
    pub stats: Stats,
    /// The grid's size.
    pub grid: (usize, usize),
    /// Columns sampled (the mask's, else all).
    pub sampled: usize,
    /// Seconds spent sampling (not reading).
    pub seconds: f64,
}

/// Samples a grid with the given runs and writes each run's file (whole or not at all).
///
/// # Errors
/// The sampler's refusal or a write that failed.
pub fn run_grid(grid: &Grid, geo: &Geometry, terrain: Option<&Terrain>, runs: &[(String, Body)]) -> Res<Written> {
    let t0 = std::time::Instant::now();
    let field = terrain.map(Terrain::field);
    let bodies: Vec<Body> = runs.iter().map(|r| r.1).collect();
    let (out, stats) =
        super::sample(grid, geo, field.as_ref(), &bodies).map_err(|e| raise("ValueError", e.to_string()))?;
    let seconds = t0.elapsed().as_secs_f64();
    let mut paths = Vec::new();
    for ((path, _), s) in runs.iter().zip(&out) {
        write(path, s)?;
        paths.push(path.clone());
    }
    Ok(Written {
        paths,
        stats,
        grid: (grid.nx, grid.nz),
        sampled: grid
            .mask
            .as_ref()
            .map_or(grid.ncols(), |m| m.iter().filter(|&&b| b).count()),
        seconds,
    })
}

/// `write(path, s)`: the version-3 file, whole or not at all.
///
/// # Errors
/// The writer's refusal or the file system's.
pub fn write(path: &str, s: &Samples) -> Res<()> {
    if let Some(d) = Path::new(path).parent() {
        os::makedirs(&d.to_string_lossy())?;
    }
    bc2_gamefs::atomic::write_with(Path::new(path), |f| s.write_to(f))
        .map_err(|e| raise("OSError", format!("{path}: {e}")))
}

/// What [`sample_level`] does.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelOptions {
    /// The 0.8 m pair over the level's box.
    pub coarse: bool,
    /// The 0.2 m pair over the detailed sectors (`None`: none).
    pub detailed: Option<Detailed>,
    /// The vehicle-shaped run at the vehicle pitch.
    pub vehicle: bool,
    /// The body letters (`BODIES`).
    pub letters: String,
    /// Where the files go (`work\navsample_offline` by default).
    pub out: Option<String>,
}

impl Default for LevelOptions {
    fn default() -> Self {
        LevelOptions {
            coarse: true,
            detailed: Some(Detailed::Own),
            vehicle: false,
            letters: BODIES.to_owned(),
            out: None,
        }
    }
}

/// What [`sample_level`] did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LevelReport {
    /// The 0.8 m pair.
    pub coarse: Option<Written>,
    /// The detailed sectors sampled at 0.2 (sorted).
    pub detailed: Vec<(i64, i64)>,
    /// Of them DICE's that the level's own choice leaves out.
    pub dice_only: Vec<(i64, i64)>,
    /// The 0.2 m pair.
    pub fine: Option<Written>,
    /// The vehicle run.
    pub vehicle: Option<Written>,
    /// The geometry read.
    pub geometry: GeometryInfo,
    /// Seconds reading the level.
    pub load_seconds: f64,
}

/// `out_dir(root)`: `work\navsample_offline`.
///
/// # Errors
/// A path the join refuses.
pub fn out_dir(r: &Root) -> Res<String> {
    join2(&r.work, "navsample_offline")
}

/// `sample_level(root, lv)` (and its new runs): the 0.8 m pair over the level's box; the 0.2 m pair over the
/// detailed sectors chosen from those samples (and DICE's, [`Detailed::WithDice`]); the vehicle run. The geometry
/// is read once. `log` takes each line.
///
/// # Errors
/// The level not prepared or unreadable; a file not written.
pub fn sample_level(
    r: &Root,
    lv: &str,
    opt: &LevelOptions,
    log: &mut dyn FnMut(&str) -> Res<()>,
    tool: ToolRun<'_>,
) -> Res<LevelReport> {
    let src = Source::level(r, lv, opt.detailed == Some(Detailed::WithDice))?;
    let out = match &opt.out {
        Some(o) => o.clone(),
        None => out_dir(r)?,
    };
    sample_source(&src, lv, &out, opt, log, tool)
}

/// [`sample_level`] on a [`Source`], the files into `out`.
///
/// # Errors
/// The level unreadable; a file not written.
pub fn sample_source(
    src: &Source,
    lv: &str,
    out: &str,
    opt: &LevelOptions,
    log: &mut dyn FnMut(&str) -> Res<()>,
    tool: ToolRun<'_>,
) -> Res<LevelReport> {
    let s = src.settings;
    let t0 = std::time::Instant::now();
    // The geometry's box: the level's, and - for a detailed run - every sector square it may sample: the squares
    // the box touches (a stacked sector chosen from the 0.8 m samples), the flagged sectors and DICE's. The
    // reference's sample_level crops to the level's box alone and so samples a detailed sector outside it without
    // its objects (sp_009: 6 flagged sectors at x 500-600, the box ends at x 100); its `sample --pitch 0.2` crops to
    // the detailed box. A wider crop leaves every run inside the level's box the same (no triangle 2 m outside a box
    // reaches its columns).
    let mut gbox = s.bx;
    if let Some(which) = opt.detailed {
        let grow = |b: &mut [f64; 4], o: [f64; 4]| {
            *b = [b[0].min(o[0]), b[1].min(o[1]), b[2].max(o[2]), b[3].max(o[3])];
        };
        let snap = |v: f64, up: bool| {
            if up {
                (v / 100.0).ceil() * 100.0
            } else {
                (v / 100.0).floor() * 100.0
            }
        };
        let m = DETAIL_MARGIN;
        grow(
            &mut gbox,
            [
                snap(s.bx[0], false) - m,
                snap(s.bx[1], false) - m,
                snap(s.bx[2], true) + m,
                snap(s.bx[3], true) + m,
            ],
        );
        let mut ids: Vec<(i64, i64)> = inputs::flagged_sectors(&src.level_dir, &src.common, s.off.0, s.off.1)?
            .into_iter()
            .collect();
        if which == Detailed::WithDice {
            ids.extend(src.dice.iter().copied());
        }
        if !ids.is_empty() {
            grow(&mut gbox, detailed_rects(&ids, s.off, m).1);
        }
    }
    let (geo, info, terrain) = src.load(lv, Some(gbox), &opt.letters, tool)?;
    let mut rep = LevelReport {
        load_seconds: t0.elapsed().as_secs_f64(),
        ..LevelReport::default()
    };
    log(&info.line())?;
    for p in &info.refused {
        log(&format!(
            "  warning: {p}: the Rust Havok reader refused it - its bodies are left out"
        ))?;
    }
    rep.geometry = info;
    let path = |suf: &str| join2(out, &format!("{lv}_navkit{suf}.bin"));
    let runs = |ps: &str| -> Res<Vec<(String, Body)>> {
        soldier_runs()
            .iter()
            .map(|(suf, b)| Ok((path(&format!("{suf}{ps}"))?, *b)))
            .collect()
    };
    let s08 = path("")?;
    if opt.coarse {
        let grid = Grid::from_box(0.8, s.bx, s.y, None);
        log(&format!(
            "== offline sample {lv}: box {:?}, y {:?}, pitch 0.8 -> {} x {} columns",
            s.bx, s.y, grid.nx, grid.nz
        ))?;
        let w = run_grid(&grid, &geo, terrain.as_ref(), &runs("")?)?;
        log(&run_line(&w))?;
        rep.coarse = Some(w);
    }
    if let Some(which) = opt.detailed {
        if !os::isfile(&s08) {
            return Err(navkit_error(format!(
                "{lv}: no 0.8 m samples ({s08}) to choose the detailed sectors from"
            )));
        }
        let own = own_detailed(&src.level_dir, &src.common, &s08, s.off)?;
        let mut ids: BTreeSet<(i64, i64)> = own.iter().copied().collect();
        if which == Detailed::WithDice {
            for &d in &src.dice {
                if ids.insert(d) {
                    rep.dice_only.push(d);
                }
            }
        }
        for suf in ["", "_hi"] {
            let old = path(&format!("{suf}_p02"))?;
            if os::isfile(&old) {
                std::fs::remove_file(&old).map_err(|e| os::os_error(&old, &e))?;
            }
        }
        rep.detailed = ids.into_iter().collect();
        if rep.detailed.is_empty() {
            log(&format!("  {lv}: no detailed sectors - no 0.2 m runs"))?;
        } else {
            let (rects, bx) = detailed_rects(&rep.detailed, s.off, DETAIL_MARGIN);
            let grid = Grid::from_box(0.2, bx, s.y, Some(&rects));
            log(&format!(
                "== offline sample {lv}: {} detailed sectors ({} of them DICE's only), pitch 0.2 -> {} x {} columns",
                rep.detailed.len(),
                rep.dice_only.len(),
                grid.nx,
                grid.nz
            ))?;
            let w = run_grid(&grid, &geo, terrain.as_ref(), &runs("_p02")?)?;
            log(&run_line(&w))?;
            rep.fine = Some(w);
        }
    }
    if opt.vehicle {
        let grid = Grid::from_box(super::VEHICLE_PITCH, s.bx, s.y, None);
        log(&format!(
            "== offline sample {lv}: the vehicle (radius {}, height {}, step {}), pitch {} -> {} x {} columns",
            super::VEHICLE_RADIUS,
            super::VEHICLE_HEIGHT,
            super::VEHICLE_STEP,
            super::VEHICLE_PITCH,
            grid.nx,
            grid.nz
        ))?;
        let w = run_grid(&grid, &geo, terrain.as_ref(), &[(path("_veh")?, Body::vehicle())])?;
        log(&run_line(&w))?;
        rep.vehicle = Some(w);
    }
    Ok(rep)
}

/// A run's log line.
#[must_use]
pub fn run_line(w: &Written) -> String {
    format!(
        "  sampled {} x {} columns ({} sampled, {} with geometry): {} surfaces in {:.1} s -> {}",
        w.grid.0,
        w.grid.1,
        thousands(w.sampled),
        thousands(w.stats.columns_with_hits),
        thousands(w.stats.surfaces),
        w.seconds,
        w.paths.join(", ")
    )
}

/// An in-game file's grid (`Grid.like`: its columns only when it is a detailed-zone run), read without its records.
///
/// # Errors
/// Not a sample file, or unreadable.
pub fn grid_of_file(path: &str) -> Res<(Grid, crate::navsample::Header)> {
    let f = super::compare::FileColumns::open(path).map_err(|e| raise("ValueError", e.to_string()))?;
    let counts = f.counts();
    Ok((Grid::like(&f.header, Some(&counts)), f.header))
}

/// A grid's extent (the columns' x / z range) as a box.
#[must_use]
pub fn grid_box(g: &Grid) -> [f64; 4] {
    [
        g.min_x,
        g.min_z,
        g.x(g.nx.saturating_sub(1)),
        g.z(g.nz.saturating_sub(1)),
    ]
}

/// `like`: an in-game file's grid sampled with its capsule bottom (`navsample_offline`'s `like`), written to `out`
/// when given. `geo` needs the triangles over the grid's box ([`grid_box`]; the reference reads the whole level: the
/// same samples).
///
/// # Errors
/// The in-game file unreadable, the sampler's refusal, a write that failed.
pub fn like(path: &str, geo: &Geometry, terrain: Option<&Terrain>, out: Option<&str>) -> Res<Samples> {
    let (grid, h) = grid_of_file(path)?;
    let body = Body::Capsule {
        radius: super::RADIUS,
        height: super::HEIGHT,
        bottom: h.cap_bottom,
    };
    let field = terrain.map(Terrain::field);
    let (mut s, _) =
        super::sample(&grid, geo, field.as_ref(), &[body]).map_err(|e| raise("ValueError", e.to_string()))?;
    let s = s.pop().ok_or(Fail::Handoff)?;
    if let Some(p) = out {
        write(p, &s)?;
    }
    Ok(s)
}

/// A sample file's header and per-column counts (versions 1 to 3).
///
/// # Errors
/// Not a sample file.
pub fn header_and_counts(data: &[u8]) -> Res<(crate::navsample::Header, Vec<u8>)> {
    let (h, runs) = load_runs(data)?;
    let counts = runs
        .start
        .windows(2)
        .map(|w| (w[1] - w[0]).clamp(0, 255) as u8)
        .collect();
    Ok((h, counts))
}

/// A sample file's header (its first 56 bytes) and version.
///
/// # Errors
/// Not a sample file (magic, size, version).
pub fn header_of(data: &[u8]) -> Res<(crate::navsample::Header, u32)> {
    let bad = || raise("ValueError", "not a navsample file");
    if data.len() < crate::navsample::HEADER_SIZE || &data[..4] != b"ONS1" {
        return Err(bad());
    }
    let u32_at = |o: usize| u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
    let f = |o: usize| f64::from(f32::from_bits(u32_at(o)));
    let version = u32_at(4);
    if !(1..=3).contains(&version) {
        return Err(bad());
    }
    Ok((
        crate::navsample::Header {
            pitch: f(8),
            min_x: f(12),
            min_z: f(16),
            nx: i64::from(u32_at(20) as i32),
            nz: i64::from(u32_at(24) as i32),
            y_top: f(28),
            y_bottom: f(32),
            radius: f(36),
            cap_bottom: f(40),
            cap_top: f(44),
            ray_radius: f(48),
        },
        version,
    ))
}

/// A sample file's header and surfaces (versions 1 to 3, `navsample.load`).
///
/// # Errors
/// Not a sample file.
pub fn load_runs(data: &[u8]) -> Res<(crate::navsample::Header, crate::navsample::Runs)> {
    let (h, version) = header_of(data)?;
    let runs = match version {
        3 => crate::navsample::parse_v3(data, h.nx, h.nz),
        1 | 2 => crate::navsample::parse_runs(
            data,
            crate::navsample::HEADER_SIZE,
            (h.nx.max(0) * h.nz.max(0)) as usize,
            version,
        ),
        _ => return Err(raise("ValueError", "not a navsample file")),
    }
    .map_err(|e| raise("ValueError", e.to_string()))?;
    Ok((h, runs))
}
