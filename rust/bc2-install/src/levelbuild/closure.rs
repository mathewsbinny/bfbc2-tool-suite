//! `wake_install.closure`: everything the level reaches that nothing it loads has - our partitions and resources,
//! the stock partitions (each from the first stock bundle holding it, a level-local copy by its alias) with their
//! resources (from their own bundle, and each one it lacks from the first other bundle that has it), and the
//! resources of what the loaded bundles hold but not with their resources; the references that resolve nowhere,
//! the surface shaders, and the references naming an instance their partition does not have.
//!
//! The walk goes in waves, as the Python did: a wave's partitions are read bundle by bundle (one read per bundle),
//! their references are the next wave. The order of everything (the take's, the unresolved references', the report's
//! ties) is the Python's: the first wave starts with the level, the level assets, the starts the caller gives and the
//! partition names our mesh sets hold (in the order they first appear in each set).

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use rayon::prelude::*;
use regex::Regex;

use super::index::{OurFile, StockIndex, live_entries};
use super::{NONRES, OMap, Result, bundle_read, read, to_xml};

/// Where a staged file's bytes come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Src {
    /// A file (ours, or one the build wrote).
    Path(String),
    /// A stock bundle's entry: (bundle, entry name).
    Stock(String, String),
    /// Bytes the build made.
    Bytes(Vec<u8>),
}

/// A file the level bundle takes: its name in the bundle, its bundle type (`None`: to be found), its source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Take {
    /// The name in the bundle.
    pub name: String,
    /// The bundle type, `None` until the build finds it (`res_type`).
    pub kind: Option<String>,
    /// Its bytes' source.
    pub src: Src,
}

/// What the closure walks over besides the level and the inputs: the level's bindings (OPFOR's).
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ClosureSpec {
    /// The game copy (the report's bundle paths are relative to it).
    pub game: String,
    /// `work\ww2`.
    pub w: String,
    /// `LEVEL`: the level partition (`levels/wake_island_s`).
    pub level: String,
    /// `LV`.
    pub lv: String,
    /// `WW2_LEVELS`: every WW2 level.
    pub ww2_levels: Vec<String>,
    /// The build folders of the WW2 levels under `work\ww2` (`wake` and the others' names).
    pub level_build_roots: Vec<String>,
    /// `LEVEL_ROOT`: this level's build folder.
    pub level_root: String,
    /// `LOADED`: the bundles the level loads with it.
    pub loaded_bundles: Vec<String>,
    /// `MASKED`: the bundles `masked_by`'s deleted markers hide names of.
    pub masked: Vec<String>,
    /// `MASKED_BY`.
    pub masked_by: String,
}

/// `closure(...)`'s result.
#[derive(Debug, Default)]
pub struct Closure {
    /// {lower name: (name, type, source)} in the order the walk took them.
    pub take: OMap<Take>,
    /// The references resolving nowhere: (partition, a referrer, `ours` / `stock`), in the walk's order.
    pub unresolved: Vec<(String, String, String)>,
    /// The surface shaders referenced (lower names), sorted.
    pub shaders: Vec<String>,
    /// Our partitions the level does not reach, sorted.
    pub unreached: Vec<String>,
    /// Ours that a loaded bundle also has, sorted.
    pub dup_loaded: Vec<String>,
    /// Every partition the walk met, sorted.
    pub seen: Vec<String>,
    /// (partition, guid, referrer, `ours` / `stock`) of the references naming an instance their partition lacks.
    pub dangling: Vec<(String, String, String, String)>,
}

static REF: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r#"ref="([A-Za-z0-9_/.!\- ]+)/[0-9A-Fa-f-]{36}""#).ok());
static REF_GUID: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r#"ref="([A-Za-z0-9_/.!\- ]+)/([0-9A-Fa-f-]{36})""#).ok());
static INSTANCE: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r#"<instance guid="([^"]+)""#).ok());
static NAME_FIELD: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r#"<field name="Name">([^<]+)</field>"#).ok());
static ALIAS: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"^levels/[^/]+/(.+/.+)\.dbx$").ok());
static DECAL: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"^(?:terrains/[^/]+/materials/[^/]+|levels/[^/]+/roads)$").ok());

fn caps1<'a>(r: &LazyLock<Option<Regex>>, x: &'a str) -> Vec<&'a str> {
    r.as_ref().map_or_else(Vec::new, |r| {
        r.captures_iter(x)
            .filter_map(|c| c.get(1))
            .map(|m| m.as_str())
            .collect()
    })
}

/// `TERRAIN_WATER.fullmatch(p)`: `terrains/<x>/<x>-water`.
#[must_use]
pub fn terrain_water(p: &str) -> bool {
    let Some(rest) = p.strip_prefix("terrains/") else {
        return false;
    };
    let Some((x, tail)) = rest.split_once('/') else {
        return false;
    };
    !x.is_empty() && tail.strip_suffix("-water") == Some(x)
}

/// `TERRAIN_DECAL_REFS.fullmatch(p)`.
#[must_use]
pub fn terrain_decal(p: &str) -> bool {
    DECAL.as_ref().is_some_and(|r| r.is_match(p))
}

fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'/' || b == b'-'
}

/// The partition names a mesh set's bytes hold, in the order they first appear (`(?<![A-Za-z0-9_/-])
/// ([A-Za-z][A-Za-z0-9_-]*/[A-Za-z0-9_/-]+)(?![A-Za-z0-9_/-])`: a whole run of name characters starting with a
/// letter, with a `/` that has a character after it).
#[must_use]
pub fn mesh_set_names(data: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut i = 0;
    while i < data.len() {
        if !data.get(i).copied().is_some_and(is_name_byte) {
            i += 1;
            continue;
        }
        let start = i;
        while data.get(i).copied().is_some_and(is_name_byte) {
            i += 1;
        }
        let run = data.get(start..i).unwrap_or_default();
        let first_ok = run.first().is_some_and(u8::is_ascii_alphabetic);
        let slash = run.iter().position(|&b| b == b'/');
        if first_ok
            && let Some(s) = slash
            && s + 1 < run.len()
        {
            let s = String::from_utf8_lossy(run).into_owned();
            if seen.insert(s.clone()) {
                out.push(s);
            }
        }
    }
    out
}

struct Refs {
    guids: HashSet<String>,
    refs_g: Vec<(String, String)>,
    refs: Vec<String>,
}

fn read_refs(data: &[u8]) -> Result<Refs> {
    let x = to_xml(data)?;
    let guids = caps1(&INSTANCE, &x).into_iter().map(str::to_lowercase).collect();
    let refs_g = REF_GUID.as_ref().map_or_else(Vec::new, |r| {
        r.captures_iter(&x)
            .filter_map(|c| Some((c.get(1)?.as_str().to_lowercase(), c.get(2)?.as_str().to_lowercase())))
            .collect()
    });
    let refs = caps1(&REF, &x).into_iter().map(str::to_lowercase).collect();
    Ok(Refs { guids, refs_g, refs })
}

/// The walk's state.
struct Walk<'a> {
    index: &'a StockIndex,
    res_of: HashMap<&'a str, Vec<&'a str>>,
    dbx_of: HashMap<&'a str, Vec<&'a str>>,
    all_res: Vec<(&'a str, &'a str)>,
    names_of: HashMap<&'a str, usize>,
    loaded_stems: HashSet<String>,
    preloaded: &'a HashSet<String>,
    take: OMap<Take>,
}

impl<'a> Walk<'a> {
    fn bundles_under(&self, stem: &str) -> HashSet<&'a str> {
        let i = self.all_res.partition_point(|(n, _)| *n < stem);
        self.all_res
            .get(i..)
            .unwrap_or_default()
            .iter()
            .take_while(|(n, _)| n.starts_with(stem))
            .map(|(_, b)| *b)
            .collect()
    }

    fn stem_res(&self, b: &str, stem: &str) -> Vec<&'a str> {
        let (Some(lst), Some(parts)) = (self.res_of.get(b), self.dbx_of.get(b)) else {
            return Vec::new();
        };
        let su = format!("{stem}_");
        let j = parts.partition_point(|p| *p < su.as_str());
        let longer: Vec<String> = parts
            .get(j..)
            .unwrap_or_default()
            .iter()
            .take_while(|p| p.starts_with(&su))
            .map(|p| format!("{p}_"))
            .collect();
        let i = lst.partition_point(|n| *n < stem);
        let mut out = Vec::new();
        for n in lst
            .get(i..)
            .unwrap_or_default()
            .iter()
            .take_while(|n| n.starts_with(stem))
        {
            let s = n.get(..n.len().saturating_sub(4)).unwrap_or_default();
            let s_ = format!("{s}_");
            if (s == stem || s.starts_with(&su)) && !longer.iter().any(|l| s_.starts_with(l.as_str())) {
                out.push(*n);
            }
        }
        out
    }

    fn stock_res(&mut self, b: &'a str, stem: &str, as_stem: &str) {
        let mut got: HashSet<&str> = HashSet::new();
        let under = self.bundles_under(stem);
        let mut order: Vec<&'a str> = vec![b];
        order.extend(
            self.index
                .bundles
                .iter()
                .map(|x| x.path.as_str())
                .filter(|x| *x != b && under.contains(x)),
        );
        let cut = stem.chars().count() as isize - as_stem.chars().count() as isize;
        for (k, bb) in order.into_iter().enumerate() {
            for low in self.stem_res(bb, stem) {
                if got.contains(low)
                    || self.loaded_stems.contains(low.get(..low.len() - 4).unwrap_or_default())
                    || self.preloaded.contains(low)
                {
                    continue;
                }
                got.insert(low);
                let new = format!("{as_stem}{}", low.get(stem.len()..).unwrap_or_default());
                let Some(bundle) = self.names_of.get(bb).and_then(|&i| self.index.bundles.get(i)) else {
                    continue;
                };
                let Some((name, t)) = bundle.get(low) else { continue };
                let out_name = super::py_tail(name, cut);
                self.take.set_default(
                    &new,
                    Take {
                        name: out_name,
                        kind: Some(t.to_owned()),
                        src: Src::Stock(bb.to_owned(), name.to_owned()),
                    },
                );
            }
            if k == 0 && got.is_empty() && stem != as_stem {
                break;
            }
        }
    }
}

/// Inputs of [`closure`] besides the spec.
pub struct Inputs<'a> {
    /// The level partition as edited (`edit_level`).
    pub level_data: &'a [u8],
    /// Our files.
    pub ours: &'a OMap<OurFile>,
    /// The stock index.
    pub index: &'a StockIndex,
    /// The lower names the loaded bundles hold.
    pub loaded: &'a HashSet<String>,
    /// The lower names the level's own ingame / loader bundles hold.
    pub preloaded: &'a HashSet<String>,
    /// More partitions to start from: (lower partition, why).
    pub starts: &'a [(String, String)],
}

/// `closure(level_data, ours, index, loaded, preloaded, report, starts)`: the walk, its report lines appended.
///
/// # Errors
/// A partition or bundle that cannot be read.
#[allow(clippy::too_many_lines, reason = "wake_install.closure, step for step")]
pub fn closure(spec: &ClosureSpec, inp: &Inputs<'_>, report: &mut Vec<String>) -> Result<Closure> {
    let index = inp.index;
    let ours = inp.ours;
    let mut stock_dbx: HashMap<&str, (&str, &str)> = HashMap::new();
    let mut alias: HashMap<String, (&str, &str)> = HashMap::new();
    let mut res_of: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut dbx_of: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut names_of: HashMap<&str, usize> = HashMap::new();
    for (bi, b) in index.bundles.iter().enumerate() {
        names_of.entry(b.path.as_str()).or_insert(bi);
        let mut res = Vec::new();
        let mut parts = Vec::new();
        for (low, name, _) in &b.names {
            if let Some(stem) = low.strip_suffix(".dbx") {
                stock_dbx.entry(stem).or_insert((b.path.as_str(), name.as_str()));
                if let Some(a) = ALIAS.as_ref().and_then(|r| r.captures(low)).and_then(|c| c.get(1)) {
                    alias
                        .entry(a.as_str().to_owned())
                        .or_insert((b.path.as_str(), name.as_str()));
                }
                parts.push(stem);
            } else if low.ends_with(".res") {
                res.push(low.as_str());
            }
        }
        res.sort_unstable();
        parts.sort_unstable();
        res_of.insert(b.path.as_str(), res);
        dbx_of.insert(b.path.as_str(), parts);
    }
    let mut all_res: Vec<(&str, &str)> = res_of
        .iter()
        .flat_map(|(b, lst)| lst.iter().map(move |n| (*n, *b)))
        .collect();
    all_res.sort_unstable();
    let loaded_stems: HashSet<String> = inp
        .loaded
        .iter()
        .filter_map(|n| n.strip_suffix(".res").map(str::to_owned))
        .collect();
    let ours_dbx: HashSet<&str> = ours.keys().filter_map(|k| k.strip_suffix(".dbx")).collect();
    let mut w = Walk {
        index,
        res_of,
        dbx_of,
        all_res,
        names_of,
        loaded_stems,
        preloaded: inp.preloaded,
        take: OMap::new(),
    };
    let mut unresolved: OMap<String> = OMap::new();
    let mut shaders: HashSet<String> = HashSet::new();
    let mut reached_ours: HashSet<String> = HashSet::new();
    let mut dup_loaded: HashSet<String> = HashSet::new();
    // loaded_where: the loaded bundles' partitions (LOADED first), read once
    let hide = super::index::masked_names(&spec.masked_by);
    let mut where_: HashMap<String, (String, String)> = HashMap::new();
    for b in &spec.loaded_bundles {
        if std::path::Path::new(b).exists()
            && let Ok(es) = live_entries(b, &spec.masked, &hide)
        {
            for (n, _) in es {
                let low = n.to_lowercase();
                if low.ends_with(".dbx") {
                    where_.entry(low).or_insert_with(|| (b.clone(), n.clone()));
                }
            }
        }
    }
    let level_xml = to_xml(inp.level_data)?;
    let level_assets: HashSet<String> = caps1(&NAME_FIELD, &level_xml)
        .into_iter()
        .map(str::to_lowercase)
        .collect();
    let mut todo: Vec<(String, String)> = vec![(spec.level.clone(), "start".into())];
    let mut sorted_ours: Vec<&str> = ours_dbx.iter().copied().collect();
    sorted_ours.sort_unstable();
    todo.extend(
        sorted_ours
            .into_iter()
            .filter(|p| level_assets.contains(*p))
            .map(|p| (p.to_owned(), "level asset".to_owned())),
    );
    todo.extend(inp.starts.iter().cloned());
    for (low, (_, path)) in ours.iter() {
        if low.ends_with("_mesh.res") {
            for s in mesh_set_names(&read(path)?) {
                let p = s.to_lowercase();
                if ours_dbx.contains(p.as_str()) || stock_dbx.contains_key(p.as_str()) || alias.contains_key(&p) {
                    todo.push((p, "mesh set".into()));
                }
            }
        }
    }
    let mut guids_of: HashMap<String, HashSet<String>> = HashMap::new();
    let mut refs_g: Vec<(String, String, String)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    while !todo.is_empty() {
        let mut wave: OMap<Vec<(String, String, Option<String>)>> = OMap::new();
        let mut loaded_wave: OMap<Vec<(String, String)>> = OMap::new();
        let mut ours_read: Vec<(String, Option<Vec<u8>>)> = Vec::new();
        let mut k = 0;
        while let Some((p, by)) = todo.get(k).cloned() {
            k += 1;
            if !seen.insert(p.clone()) {
                continue;
            }
            let local = format!("{}/{p}", spec.level);
            if !ours_dbx.contains(p.as_str()) && ours_dbx.contains(local.as_str()) {
                todo.push((local, by));
                continue;
            }
            let pdbx = format!("{p}.dbx");
            if ours_dbx.contains(p.as_str()) {
                reached_ours.insert(p.clone());
                if inp.loaded.contains(&pdbx) {
                    dup_loaded.insert(p.clone());
                }
                if p == spec.level {
                    ours_read.push((p, None));
                } else {
                    let path = ours.get(&pdbx).map(|(_, path)| path.clone()).unwrap_or_default();
                    ours_read.push((p, Some(read(&path)?)));
                }
                continue;
            }
            if inp.loaded.contains(&pdbx) || inp.preloaded.contains(&pdbx) {
                let under = w.bundles_under(&p);
                let first = index
                    .bundles
                    .iter()
                    .map(|b| b.path.as_str())
                    .find(|b| under.contains(b) && !w.stem_res(b, &p).is_empty());
                if let Some(first) = first {
                    w.stock_res(first, &p, &p);
                }
                if let Some((b, n)) = where_.get(&pdbx) {
                    if !loaded_wave.contains(b) {
                        loaded_wave.insert(b, Vec::new());
                    }
                    if let Some(v) = loaded_wave.get_mut(b) {
                        v.push((p.clone(), n.clone()));
                    }
                }
                continue;
            }
            let mut hit = stock_dbx.get(p.as_str()).copied();
            let mut as_name = None;
            if hit.is_none()
                && let Some(a) = alias.get(&p)
            {
                hit = Some(*a);
                as_name = Some(p.clone());
            }
            if let Some((b, n)) = hit {
                if !wave.contains(b) {
                    wave.insert(b, Vec::new());
                }
                if let Some(v) = wave.get_mut(b) {
                    v.push((p.clone(), n.to_owned(), as_name));
                }
                continue;
            }
            if terrain_water(&p) || terrain_decal(&p) {
                continue;
            }
            if p.contains("/shaders/") || p.starts_with("terrains/") {
                shaders.insert(p);
                continue;
            }
            if !unresolved.contains(&p) {
                unresolved.insert(&p, by);
            }
        }
        // the partitions read in the Python's order: ours as met, then the loaded bundles', then the stock ones
        let mut items: Vec<(String, Vec<u8>)> = Vec::new();
        for (p, data) in ours_read {
            items.push((p, data.unwrap_or_else(|| inp.level_data.to_vec())));
        }
        for (b, list) in loaded_wave.iter() {
            let names: Vec<String> = list.iter().map(|(_, n)| n.clone()).collect();
            let mut data = bundle_read(b, &names)?;
            for (p, n) in list {
                items.push((p.clone(), data.remove(n).unwrap_or_default()));
            }
        }
        let wave = wave.into_vec();
        for (b, list) in &wave {
            let names: Vec<String> = list.iter().map(|(_, n, _)| n.clone()).collect();
            let mut data = bundle_read(b, &names)?;
            for (p, name, as_name) in list {
                let out_name = match as_name {
                    Some(a) => format!("{a}{}", name.get(name.len().saturating_sub(4)..).unwrap_or_default()),
                    None => name.clone(),
                };
                w.take.insert(
                    &format!("{p}.dbx"),
                    Take {
                        name: out_name,
                        kind: Some(NONRES.into()),
                        src: Src::Stock(b.clone(), name.clone()),
                    },
                );
                let bpath = index
                    .bundles
                    .iter()
                    .map(|x| x.path.as_str())
                    .find(|x| x == b)
                    .unwrap_or_default();
                let stem = name
                    .get(..name.len().saturating_sub(4))
                    .unwrap_or_default()
                    .to_lowercase();
                w.stock_res(bpath, &stem, p);
                items.push((p.clone(), data.remove(name).unwrap_or_default()));
            }
        }
        let refs: Vec<Result<Refs>> = items.par_iter().map(|(_, d)| read_refs(d)).collect();
        let mut nxt = Vec::new();
        for ((p, _), r) in items.iter().zip(refs) {
            let r = r?;
            guids_of.insert(p.clone(), r.guids);
            refs_g.extend(r.refs_g.into_iter().map(|(t, g)| (t, g, p.clone())));
            nxt.extend(r.refs.into_iter().map(|t| (t, p.clone())));
        }
        todo = nxt;
    }
    let mut take = w.take;
    for (low, (rel, path)) in ours.iter() {
        if let Some(stem) = low.strip_suffix(".res")
            && level_assets.contains(stem)
        {
            take.insert(
                low,
                Take {
                    name: rel.clone(),
                    kind: None,
                    src: Src::Path(path.clone()),
                },
            );
        }
    }
    let other_lv: Vec<&String> = spec.ww2_levels.iter().filter(|l| **l != spec.lv).collect();
    for (low, (rel, path)) in ours.iter() {
        let Some(stem) = low.strip_suffix(".res") else { continue };
        let parts: Vec<&str> = stem.split('_').collect();
        let mut owner = None;
        for k in (1..=parts.len()).rev() {
            let c = parts.get(..k).unwrap_or_default().join("_");
            if ours_dbx.contains(c.as_str()) {
                owner = Some(c);
                break;
            }
        }
        let other_level = other_lv
            .iter()
            .any(|l| low.starts_with(&format!("terrains/{l}/")) || low.starts_with(&format!("levels/{l}/")));
        let other_build = owner.is_none() && other_level_build(spec, path);
        if owner.as_ref().is_none_or(|o| reached_ours.contains(o)) && !other_level && !other_build {
            take.insert(
                low,
                Take {
                    name: rel.clone(),
                    kind: None,
                    src: Src::Path(path.clone()),
                },
            );
        }
    }
    let mut reached_sorted: Vec<&String> = reached_ours.iter().collect();
    reached_sorted.sort_unstable();
    for p in reached_sorted {
        if *p != spec.level {
            let key = format!("{p}.dbx");
            if let Some((rel, path)) = ours.get(&key) {
                take.insert(
                    &key,
                    Take {
                        name: rel.clone(),
                        kind: Some(NONRES.into()),
                        src: Src::Path(path.clone()),
                    },
                );
            }
        }
    }
    let mut unreached: Vec<String> = ours
        .keys()
        .filter_map(|k| k.strip_suffix(".dbx"))
        .filter(|s| !reached_ours.contains(*s))
        .map(str::to_owned)
        .collect();
    unreached.sort_unstable();
    let mut by_src: OMap<usize> = OMap::new();
    for (_, t) in take.iter() {
        let key = match &t.src {
            Src::Stock(b, _) => crate::pytext::relpath(b, &spec.game),
            _ => "ours".to_owned(),
        };
        let n = by_src.get(&key).copied().unwrap_or(0);
        by_src.insert(&key, n + 1);
    }
    let copied = take
        .iter()
        .filter(|(_, t)| matches!(t.src, Src::Stock(..)) && t.kind.as_deref() == Some(NONRES))
        .count();
    report.push(format!(
        "closure: {} partitions reached; {} ours, {copied} stock partitions copied; {} files in all ({} of ours also in \
         a loaded bundle)",
        seen.len(),
        reached_ours.len(),
        take.len(),
        dup_loaded.len()
    ));
    let mut by = by_src.into_vec();
    by.sort_by_key(|x| std::cmp::Reverse(x.1));
    for (k, v) in by.iter().take(20) {
        report.push(format!("  {v:6}  {k}"));
    }
    report.push(format!(
        "  our partitions not reached from the level: {}",
        unreached.len()
    ));
    let unresolved: Vec<(String, String, String)> = unresolved
        .into_vec()
        .into_iter()
        .map(|(p, by)| {
            let kind = if reached_ours.contains(&by) { "ours" } else { "stock" };
            (p, by, kind.to_owned())
        })
        .collect();
    let mut rg: Vec<(String, String, String)> = refs_g;
    rg.sort_unstable();
    rg.dedup();
    let mut dangling = Vec::new();
    for (t, g, by) in rg {
        let have = guids_of
            .get(&t)
            .or_else(|| guids_of.get(&format!("{}/{t}", spec.level)));
        if let Some(have) = have
            && !have.contains(&g)
        {
            let kind = if reached_ours.contains(&by) { "ours" } else { "stock" };
            dangling.push((t, g, by, kind.to_owned()));
        }
    }
    let mine: Vec<&(String, String, String, String)> = dangling.iter().filter(|d| d.3 == "ours").collect();
    report.push(format!(
        "closure by guid: {} references name an instance their partition does not have - {} made by our partitions, \
         {} by stock partitions",
        dangling.len(),
        mine.len(),
        dangling.len() - mine.len()
    ));
    for (t, g, by, _) in mine.iter().take(30) {
        report.push(format!("  {t}/{g}   (from {by})"));
    }
    let mut shaders: Vec<String> = shaders.into_iter().collect();
    shaders.sort_unstable();
    let mut dup_loaded: Vec<String> = dup_loaded.into_iter().collect();
    dup_loaded.sort_unstable();
    let mut seen: Vec<String> = seen.into_iter().collect();
    seen.sort_unstable();
    Ok(Closure {
        take,
        unresolved,
        shaders,
        unreached,
        dup_loaded,
        seen,
        dangling,
    })
}

/// `other_level_build(path)`: a file in another WW2 level's build folder under `work\ww2` (not this level's).
#[must_use]
pub fn other_level_build(spec: &ClosureSpec, path: &str) -> bool {
    let rel = super::rel_slash(path, &spec.w).to_lowercase();
    let top = rel.split('/').next().unwrap_or_default();
    spec.level_build_roots.iter().any(|r| r.to_lowercase() == top) && top != spec.level_root.to_lowercase()
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::assert_is_empty, reason = "test assertions")]
mod tests {
    use super::*;

    #[test]
    fn mesh_set_names_as_the_pattern() {
        let got = mesh_set_names(b"\x00Objects/A_b/C\x00x/y\x009z/q a/ Obj/x/y Objects/A_b/C");
        assert_eq!(got, ["Objects/A_b/C", "x/y", "Obj/x/y"]);
        // a run with a name character before it is not a start
        assert!(mesh_set_names(b"_a/b").is_empty());
        assert!(mesh_set_names(b"ab/").is_empty());
    }

    #[test]
    fn terrain_conventions() {
        assert!(terrain_water("terrains/wake_island_s/wake_island_s-water"));
        assert!(!terrain_water("terrains/a/b-water"));
        assert!(!terrain_water("terrains/a/a-water/x"));
        assert!(terrain_decal("terrains/x/materials/road"));
        assert!(terrain_decal("levels/x/roads"));
        assert!(!terrain_decal("levels/x/roads/y"));
    }
}
