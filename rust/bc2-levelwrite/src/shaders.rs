//! The shader entries a level needs (round XF1, audit AU1 G-N1).
//!
//! A level draws a mesh only through its own shader databases (`levels/<lv>/shaders/dx{9,10,11}_single.res`): every
//! entry is `<mesh>/<shader>` with its programs, and every part `<mesh>_lod<N>/<part>/<technique>` names its vertex
//! layout. There is no shared database: `mp_common`, `sp_common`, `nam_common` and `ww2_common` hold none, and DICE's
//! levels carry the entries of the commons' meshes they draw in their own files (SHDB-LVL-1). A part drawn with no
//! program for its layout crashes the renderer at 0x98BD6E (RNDR-DRAW-6). So a mesh an export brings in (an object
//! placed from another level or set, another mode's layer, another set's soldiers) needs its entries added to the
//! level's databases: [`add_entries`] finds them in the databases of the levels that draw the mesh and merges them
//! ([`bc2_formats::shaderdb::merge`]), and [`uncovered`] is the check's view.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

use bc2_formats::dbx::edit::EditDbx;
use bc2_formats::shaderdb::merge::{Kind, Merger, coverage};
use bc2_formats::shaderdb::{self, open, query};
use serde::{Deserialize, Serialize};

use crate::export::{Found, Game, StagedEntry, read_found};

/// The mesh assets a shader entry names (the partition's first instance).
pub const MESH_TYPES: [&str; 4] = [
    "Render.SkinnedMeshAsset",
    "Render.RigidMeshAsset",
    "Render.CompositeMeshAsset",
    "Render.TreeMeshAsset",
];

/// A mesh a level reaches: its name as entries spell it (the asset's `Name`, lower case) and its partition.
pub type Reached = BTreeMap<String, String>;

/// What [`add_entries`] did to one database.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatabaseAdds {
    /// The database (`levels/<lv>/shaders/dx11_single.res`).
    pub database: String,
    /// Meshes the level reaches that the base's database had no entry for, before.
    pub missing: usize,
    /// (mesh, the level whose database gave its entries, entries added).
    pub added: Vec<(String, String, usize)>,
    /// Meshes no level's database of the game copy has: they stay without entries (the stock levels reach such
    /// meshes too: variants they never draw).
    pub nowhere: Vec<String>,
}

fn err(why: impl Into<String>) -> crate::export::ExportError {
    crate::export::ExportError(why.into())
}

/// The mesh name a partition gives entries, when its first instance is a mesh asset: its `Name` field (the
/// partition's path where it has none), lower case.
fn mesh_name(name: &str, d: &EditDbx) -> Option<String> {
    let first = *d.instances().first()?;
    let t = d.attr(first, b"type")?;
    if !MESH_TYPES.iter().any(|m| m.as_bytes() == t) {
        return None;
    }
    let own = d
        .child(first, b"Name")
        .and_then(|x| d.text(x))
        .filter(|x| !x.is_empty())
        .map(|x| String::from_utf8_lossy(x).into_owned());
    Some(
        own.unwrap_or_else(|| name.trim_end_matches(".dbx").to_owned())
            .to_ascii_lowercase(),
    )
}

/// Whether an entry name (lower case) is a partition.
fn is_dbx(name: &str) -> bool {
    std::path::Path::new(name)
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("dbx"))
}

/// The partitions a partition's references name (`<path>.dbx`, lower case).
fn named(d: &EditDbx) -> Vec<String> {
    let mut out = Vec::new();
    for e in d.instances() {
        for x in d.walk(e) {
            if let Some(r) = d.attr(x, b"ref")
                && let Some(i) = r.iter().rposition(|&c| c == b'/')
            {
                let p = String::from_utf8_lossy(r.get(..i).unwrap_or_default()).to_ascii_lowercase();
                out.push(format!("{p}.dbx"));
            }
        }
    }
    out
}

/// Reads partitions for [`reach`]: each name's bytes (`None`: no such entry), in the order asked.
pub type GetMany<'a> = dyn FnMut(&[String]) -> crate::export::Result<Vec<Option<Vec<u8>>>> + 'a;

/// Every mesh reachable from `start` (partition names, lower case): each partition read through `get` (the level's
/// own entries first, then the bundles it loads), every reference followed. The meshes a level can draw, each with
/// the first partition (by name) of the first step of the walk that reaches it.
///
/// The walk goes a step at a time (round HG1): every partition first named in the last step is read in one call of
/// `get`, so each bundle is decompressed once per step - about ten for a stock level - not once per partition (a
/// stock level's reach reads thousands of partitions from `mp_common`'s payload of hundreds of MB; read one at a time
/// without the bundle cache's disk files, that took minutes in a release build and over an hour in a debug one). The
/// partitions walked, and so the meshes, are the ones a walk one partition at a time finds.
///
/// # Errors
/// A bundle does not read.
pub fn reach(start: impl IntoIterator<Item = String>, get: &mut GetMany<'_>) -> crate::export::Result<Reached> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut step: Vec<String> = start.into_iter().filter(|n| seen.insert(n.clone())).collect();
    let mut out = Reached::new();
    while !step.is_empty() {
        step.sort_unstable();
        let got = get(&step)?;
        let mut next = Vec::new();
        for (n, data) in step.iter().zip(got) {
            let Some(data) = data else { continue };
            let Ok(d) = EditDbx::parse(&data) else { continue };
            if let Some(m) = mesh_name(n, &d) {
                out.entry(m).or_insert_with(|| n.clone());
            }
            next.extend(named(&d).into_iter().filter(|x| seen.insert(x.clone())));
        }
        step = next;
    }
    Ok(out)
}

/// The entries of `names` among `live` (the bundles' live entries), read together: one pass per bundle.
fn read_live<S: std::hash::BuildHasher>(
    live: &HashMap<String, Found, S>,
    names: &[&String],
) -> crate::export::Result<Vec<Option<Vec<u8>>>> {
    let found: Vec<(usize, &Found)> = names
        .iter()
        .enumerate()
        .filter_map(|(i, n)| live.get(n.as_str()).map(|f| (i, f)))
        .collect();
    let data = read_found(&found.iter().map(|x| x.1).collect::<Vec<_>>())?;
    let mut out = vec![None; names.len()];
    for ((i, _), d) in found.into_iter().zip(data) {
        if let Some(slot) = out.get_mut(i) {
            *slot = Some(d);
        }
    }
    Ok(out)
}

/// [`reach`] over staged entries (which win) and the live entries of the bundles a level loads.
///
/// # Errors
/// A bundle does not read.
pub fn reach_staged<S: std::hash::BuildHasher>(
    ours: &[StagedEntry],
    live: &HashMap<String, Found, S>,
) -> crate::export::Result<Reached> {
    let own: HashMap<String, &StagedEntry> = ours.iter().map(|e| (e.name.to_ascii_lowercase(), e)).collect();
    let start: Vec<String> = own.keys().filter(|k| is_dbx(k)).cloned().collect();
    reach(start, &mut |names: &[String]| {
        let theirs: Vec<&String> = names.iter().filter(|n| !own.contains_key(n.as_str())).collect();
        let mut read = read_live(live, &theirs)?.into_iter();
        Ok(names
            .iter()
            .map(|n| match own.get(n.as_str()) {
                Some(e) => Some(e.data.clone()),
                None => read.next().flatten(),
            })
            .collect())
    })
}

/// Whether an entry is one of a level's own shader databases (`levels/<lv>/shaders/<file>`), and its file name.
#[must_use]
pub fn database_file(name: &str, type_name: &str) -> Option<String> {
    let low = name.to_ascii_lowercase();
    if !type_name.to_ascii_lowercase().ends_with("shaderdatabase") || !low.starts_with("levels/") {
        return None;
    }
    let (dir, file) = low.rsplit_once('/')?;
    dir.ends_with("/shaders").then(|| file.to_owned())
}

/// The database's kind for [`open::open`] and the merge.
fn opened<'a>(data: &'a [u8], file: &str, place: &str) -> crate::export::Result<(shaderdb::ShaderDb<'a>, Kind)> {
    let dx = open::dx_in_name(file).unwrap_or(10);
    let o = open::open(data, Some(if dx == 11 { 10 } else { dx }), Some(false), place).map_err(err)?;
    let kind = Kind {
        dx,
        big: o.big_endian,
        key_size: o.db.key_size(),
        version: o.db.version,
    };
    Ok((o.db, kind))
}

/// The meshes of `meshes` a database has no entry for.
fn not_in(db: &shaderdb::ShaderDb<'_>, meshes: &[String]) -> Vec<String> {
    let parts: HashSet<Vec<u8>> = db.parts.iter().filter_map(|p| query::part_mesh(&p.name)).collect();
    meshes
        .iter()
        .zip(coverage(db, meshes))
        .filter(|(m, c)| *c == 0 && !parts.contains(m.as_bytes()))
        .map(|(m, _)| m.clone())
        .collect()
}

/// Every level folder of the game copy (the shared `_common` folders and this editor's installed exports left
/// out): DICE's own (those `Dist\win32` has) by name first, then the ones only `Package` has (mode levels, other
/// sets' and OPFOR's ports) by name. The order the export looks for another level's data in, so the stock levels
/// are preferred and the answer does not depend on what else is installed where a stock level has it.
#[must_use]
pub fn game_levels(game: &Game) -> Vec<String> {
    let list = |top: PathBuf| -> BTreeSet<String> {
        let Ok(rd) = std::fs::read_dir(&top) else {
            return BTreeSet::new();
        };
        rd.flatten()
            .filter(|e| e.path().join("level-00.fbrb").is_file())
            .map(|e| e.file_name().to_string_lossy().to_ascii_lowercase())
            .filter(|n| !n.ends_with("_common") && !crate::export::is_custom(game, n))
            .collect()
    };
    let dice = list(game.root.join("Dist").join("win32").join("levels"));
    let package = list(game.root.join("Package").join("levels"));
    dice.iter()
        .cloned()
        .chain(package.into_iter().filter(|n| !dice.contains(n)))
        .collect()
}

/// A level's own database of the given file name (`dx11_single.res`), as the game reads it (its `Package` copy
/// before its `Dist` one).
fn level_database(game: &Game, lv: &str, file: &str) -> crate::export::Result<Option<Vec<u8>>> {
    let bundles: Vec<PathBuf> = game.level_bundles(lv, "level");
    let live = game.live(&bundles)?;
    let want = format!("levels/{lv}/shaders/{file}");
    match live.get(&want) {
        Some(f) => Ok(read_found(&[f])?.pop()),
        None => Ok(None),
    }
}

/// Adds to each of the level's shader databases in `entries` the entries of the meshes in `reached` it lacks: from
/// the databases of `prefer` (the mode's donor, then the levels the meshes came from) and then of every level of the
/// game copy by name, the first that has a mesh giving it. The merged file reads back and passes the database
/// checks (SHDB-CHK-1); its entries draw with the programs they have in their source (the merge's own verification).
///
/// # Errors
/// A bundle or database does not read, or the merge fails (more than 65,535 programs).
pub fn add_entries(
    game: &Game,
    entries: &mut [StagedEntry],
    reached: &Reached,
    prefer: &[String],
) -> crate::export::Result<Vec<DatabaseAdds>> {
    let meshes: Vec<String> = reached.keys().cloned().collect();
    let mut out = Vec::new();
    let mut order: Vec<String> = Vec::new();
    for p in prefer {
        let p = p.to_ascii_lowercase();
        if !order.contains(&p) {
            order.push(p);
        }
    }
    let mut all: Option<Vec<String>> = None;
    for e in entries.iter_mut() {
        let Some(file) = database_file(&e.name, &e.type_name) else {
            continue;
        };
        let missing = {
            let (db, _) = opened(&e.data, &file, &e.name)?;
            not_in(&db, &meshes)
        };
        let mut rec = DatabaseAdds {
            database: e.name.clone(),
            missing: missing.len(),
            ..DatabaseAdds::default()
        };
        if missing.is_empty() {
            out.push(rec);
            continue;
        }
        // the donors: each level's database whose entries cover meshes still missing, in order
        let mut left: Vec<String> = missing.clone();
        let mut donors: Vec<(String, Vec<u8>, Vec<String>)> = Vec::new();
        let rest = all.get_or_insert_with(|| game_levels(game)).clone();
        for lv in order.iter().chain(rest.iter().filter(|l| !order.contains(*l))) {
            if left.is_empty() {
                break;
            }
            let Some(data) = level_database(game, lv, &file)? else {
                continue;
            };
            let gives: Vec<String> = {
                let (db, _) = opened(&data, &file, lv)?;
                let lacking: HashSet<String> = not_in(&db, &left).into_iter().collect();
                left.iter().filter(|m| !lacking.contains(*m)).cloned().collect()
            };
            if gives.is_empty() {
                continue;
            }
            left.retain(|m| !gives.contains(m));
            donors.push((lv.clone(), data, gives));
        }
        rec.nowhere = left;
        if !donors.is_empty() {
            let merged = {
                let (base, kind) = opened(&e.data, &file, &e.name)?;
                let need: Vec<String> = donors.iter().flat_map(|d| d.2.iter().cloned()).collect();
                let mut m = Merger::new(base, kind, need, false).map_err(|x| err(format!("{}: {x}", e.name)))?;
                let dbs: Vec<(shaderdb::ShaderDb<'_>, Kind, &str)> = donors
                    .iter()
                    .map(|(lv, data, _)| opened(data, &file, lv).map(|(d, k)| (d, k, lv.as_str())))
                    .collect::<crate::export::Result<_>>()?;
                for (db, k, lv) in &dbs {
                    m.add_donor(db, k, lv).map_err(|x| err(format!("{}: {x}", e.name)))?;
                }
                let bytes = m.finish().map_err(|x| err(format!("{}: {x}", e.name)))?;
                let mut per: BTreeMap<(String, String), usize> = BTreeMap::new();
                for (name, lv) in &m.added {
                    let low = String::from_utf8_lossy(&query::lower(name)).into_owned();
                    let mesh = donors
                        .iter()
                        .flat_map(|d| d.2.iter())
                        .find(|ms| low.starts_with(&format!("{ms}/")) || low == **ms)
                        .cloned()
                        .unwrap_or(low);
                    *per.entry((mesh, lv.clone())).or_default() += 1;
                }
                rec.added = per.into_iter().map(|((m, l), n)| (m, l, n)).collect();
                bytes
            };
            // read back: it opens, passes the database checks and covers what it was given
            let (again, _) = opened(&merged, &file, &e.name)?;
            let problems = check_db(&again);
            if !problems.is_empty() {
                return Err(err(format!(
                    "{}: the merged database fails its checks: {}",
                    e.name,
                    problems.into_iter().take(5).collect::<Vec<_>>().join("; ")
                )));
            }
            e.data = merged;
        }
        out.push(rec);
    }
    Ok(out)
}

/// SHDB-CHK-1 on a database.
#[must_use]
pub fn check_db(db: &shaderdb::ShaderDb<'_>) -> Vec<String> {
    let input = query::CheckInput {
        big_endian: db.big_endian,
        numbers_big_endian: db.big_endian,
        params: db.params.len(),
        vs: db.vs.len(),
        ps: db.ps.len(),
        programs: db.programs.clone(),
        keys: db.keys.clone(),
        layouts: db.layouts.clone(),
        entries: db.entries.iter().map(|n| (&*n.name, n.guid)).collect(),
        fragments: db.fragments.iter().map(|n| n.guid).collect(),
        parts: db.parts.iter().map(|n| n.guid).collect(),
        groups: db
            .groups
            .iter()
            .map(|(g, idx)| (*g, idx.iter().map(|&i| u32::from(i)).collect()))
            .collect(),
    };
    query::check(&input).unwrap_or_else(|| vec!["the check could not read the database".into()])
}

/// The meshes of `reached` a database (its bytes, its file name) has no entry for.
///
/// # Errors
/// The database does not open.
pub fn uncovered(data: &[u8], file: &str, reached: &Reached) -> crate::export::Result<Vec<String>> {
    let meshes: Vec<String> = reached.keys().cloned().collect();
    let (db, _) = opened(data, file, file)?;
    Ok(not_in(&db, &meshes))
}

/// The meshes a stock level reaches (from every partition of its own level bundle, through what it loads).
///
/// # Errors
/// A bundle does not read.
pub fn stock_reach(game: &Game, lv: &str) -> crate::export::Result<Reached> {
    let mut loaded = game.loaded(lv, "level");
    loaded.extend(game.startup());
    let live = game.live(&loaded)?;
    let own = game.live(&game.level_bundles(lv, "level"))?;
    let start: Vec<String> = own.keys().filter(|k| is_dbx(k)).cloned().collect();
    reach(start, &mut |names: &[String]| read_live(&live, &names.iter().collect::<Vec<_>>()))
}

/// The meshes of a stock level's reach ([`stock_reach`]) its own database of `file` has no entry for (DICE's own:
/// variants and pieces it never draws), the allowance the export check gives; all of them where it has no such
/// database.
///
/// # Errors
/// A bundle or database does not read.
pub fn stock_uncovered(game: &Game, lv: &str, file: &str, reached: &Reached) -> crate::export::Result<HashSet<String>> {
    let Some(data) = level_database(game, lv, file)? else {
        return Ok(reached.keys().cloned().collect());
    };
    Ok(uncovered(&data, file, reached)?.into_iter().collect())
}
