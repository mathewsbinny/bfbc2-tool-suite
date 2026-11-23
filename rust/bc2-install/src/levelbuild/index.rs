//! The build's inputs: the stock bundles and their index, the bundles the level loads (less what nam_common's
//! deleted markers hide), and our builds' files (`wake_install.stock_bundles`, `ondemand_bundles`, `stock_index`,
//! `masked_names`, `live_entries`, `our_files`, `nested_roots`, `primary_type`).
//!
//! The stock index was a pickle in `work\ww2` (`install_index.pkl`, keyed by the bundles' sizes and dates); it is
//! now read from the bundles' headers on all cores and kept for the process (keyed the same way), which takes about
//! as long as the pickle took to load.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::UNIX_EPOCH;

use rayon::prelude::*;
use regex::{Regex, RegexBuilder};

use super::{OMap, Result, bundle_one, pjoin, read, rel_slash, to_xml, walk};

/// One stock bundle of the index: its live entries `(lower name, name, type)` in the order the bundle lists them (a
/// name listed twice keeps its first place and its last entry, as a `dict`).
#[derive(Debug, Clone, Default)]
pub struct Bundle {
    /// The bundle's path.
    pub path: String,
    /// `(lower name, name, type)` of every live entry.
    pub names: Vec<(String, String, String)>,
    at: HashMap<String, usize>,
}

impl Bundle {
    fn new(path: &str, entries: impl Iterator<Item = (String, String)>) -> Self {
        let mut b = Self {
            path: path.to_owned(),
            ..Self::default()
        };
        for (name, t) in entries {
            let low = name.to_lowercase();
            if let Some(&i) = b.at.get(&low) {
                if let Some(slot) = b.names.get_mut(i) {
                    *slot = (low, name, t);
                }
            } else {
                b.at.insert(low.clone(), b.names.len());
                b.names.push((low, name, t));
            }
        }
        b
    }

    /// `names.get(low)`: the entry's (name, type).
    #[must_use]
    pub fn get(&self, low: &str) -> Option<(&str, &str)> {
        let (_, n, t) = self.names.get(*self.at.get(low)?)?;
        Some((n.as_str(), t.as_str()))
    }

    /// `low in names`.
    #[must_use]
    pub fn has(&self, low: &str) -> bool {
        self.at.contains_key(low)
    }
}

/// `stock_index()`: the stock bundles in priority order, each with its live entries.
#[derive(Debug, Default)]
pub struct StockIndex {
    /// The bundles, in [`stock_bundles`]' order (an unreadable one left out).
    pub bundles: Vec<Bundle>,
    /// The bundles that could not be read (`<path>: <error>`): left out this run, the index not kept.
    pub unread: Vec<String>,
    first: HashMap<String, (usize, usize)>,
}

impl StockIndex {
    fn new(bundles: Vec<Bundle>, unread: Vec<String>) -> Self {
        let mut first = HashMap::new();
        for (bi, b) in bundles.iter().enumerate() {
            for (ni, (low, _, _)) in b.names.iter().enumerate() {
                first.entry(low.clone()).or_insert((bi, ni));
            }
        }
        Self { bundles, unread, first }
    }

    /// An index of these bundles' entries (`(bundle, [(name, type)])`, in order: a test's, or the Python shape's).
    #[must_use]
    pub fn from_bundles(bundles: Vec<(String, Vec<(String, String)>)>) -> Self {
        let b = bundles
            .into_iter()
            .map(|(p, es)| Bundle::new(&p, es.into_iter()))
            .collect();
        Self::new(b, Vec::new())
    }

    /// The first bundle holding a lower name, and its (name, type) there (`where.setdefault` over the index).
    #[must_use]
    pub fn first(&self, low: &str) -> Option<(&str, &str, &str)> {
        let &(bi, ni) = self.first.get(low)?;
        let b = self.bundles.get(bi)?;
        let (_, n, t) = b.names.get(ni)?;
        Some((b.path.as_str(), n.as_str(), t.as_str()))
    }

    /// `stock_types[low]`: the type of the first bundle's entry of that name.
    #[must_use]
    pub fn stock_type(&self, low: &str) -> Option<&str> {
        self.first(low).map(|(_, _, t)| t)
    }
}

fn ci_suffix(name: &str, suffix: &str) -> bool {
    name.len() >= suffix.len()
        && name
            .get(name.len() - suffix.len()..)
            .is_some_and(|s| s.eq_ignore_ascii_case(suffix))
}

/// `glob(os.path.join(top, '**', '*<suffix>'), recursive=True)` (Windows: the pattern without regard to case; names
/// starting with a dot skipped, as glob skips them).
fn glob_tree(top: &str, suffix: &str, out: &mut Vec<String>) {
    let Ok(rd) = fs::read_dir(top) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let p = pjoin(top, &name);
        if Path::new(&p).is_dir() {
            glob_tree(&p, suffix, out);
        } else if ci_suffix(&name, suffix) {
            out.push(p);
        }
    }
}

/// `glob(os.path.join(folder, '<prefix>*<suffix>'))` (one folder, without regard to case).
fn glob_one(folder: &str, prefix: &str, suffix: &str, out: &mut Vec<String>) {
    let Ok(rd) = fs::read_dir(folder) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name.len() < prefix.len() + suffix.len() {
            continue;
        }
        let head_ok = name.get(..prefix.len()).is_some_and(|h| h.eq_ignore_ascii_case(prefix));
        if head_ok && ci_suffix(&name, suffix) {
            out.push(pjoin(folder, &name));
        }
    }
}

/// `stock_bundles()`: the stock bundles to copy from, in priority order - Package before Dist, the levels' first;
/// not the WW2 levels' own (`lv` and `ww2_levels`), the Onslaught or OPFOR builds, ww2_common, the on-demand bundles
/// or the global streaming ones.
#[must_use]
pub fn stock_bundles(game: &str, lv: &str, ww2_levels: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for w in ["Package", "Dist\\win32"] {
        let top = pjoin(game, &w.replace('\\', "/"));
        let mut found = Vec::new();
        glob_tree(&top, ".fbrb", &mut found);
        found.sort();
        let (lvs, rest): (Vec<String>, Vec<String>) = found.into_iter().partition(|b| b.contains("\\levels\\"));
        out.extend(lvs);
        out.extend(rest);
    }
    let mut names: Vec<&str> = ww2_levels.iter().map(String::as_str).chain([lv]).collect();
    names.sort_unstable();
    names.dedup();
    let own = Regex::new(&format!(
        r"(?i)[\\/]levels[\\/](ons_|opfor_|ww2_common[\\/]|{}[\\/])",
        names.join("|")
    ));
    let ondemand = Regex::new(r"(?i)[\\/]async[\\/]ondemand_");
    let streaming = Regex::new(r"(?i)[\\/]streaming_[^\\/]*-00\.fbrb$");
    let (Ok(own), Ok(ondemand), Ok(streaming)) = (own, ondemand, streaming) else {
        return Vec::new();
    };
    out.into_iter()
        .filter(|b| !own.is_match(b) && !ondemand.is_match(b) && !streaming.is_match(b))
        .collect()
}

/// `ondemand_bundles()`: the bundles the game provides by itself - the on-demand ones and the global streaming ones.
#[must_use]
pub fn ondemand_bundles(game: &str) -> Vec<String> {
    let mut out = Vec::new();
    glob_one(&pjoin(game, "Package/async"), "ondemand_", "-00.fbrb", &mut out);
    glob_one(&pjoin(game, "Dist/win32/async"), "ondemand_", "-00.fbrb", &mut out);
    glob_one(&pjoin(game, "Package"), "streaming_", "-00.fbrb", &mut out);
    glob_one(&pjoin(game, "Dist/win32"), "streaming_", "-00.fbrb", &mut out);
    out.sort();
    out
}

/// A bundle's signature in the index's key: (path, size, whole seconds of its write time).
fn signature(b: &str) -> (String, u64, i64) {
    let (size, secs) = fs::metadata(b).map_or((0, 0), |m| {
        let secs = m
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        (m.len(), i64::try_from(secs).unwrap_or(0))
    });
    (b.to_owned(), size, secs)
}

type Sig = Vec<(String, u64, i64)>;

static INDEX: LazyLock<Mutex<Option<(Sig, Arc<StockIndex>)>>> = LazyLock::new(|| Mutex::new(None));

/// `stock_index()`: the index of [`stock_bundles`], read on all cores and kept for the process while every bundle
/// keeps its size and date (not kept when a bundle could not be read: the next call tries it again).
#[must_use]
pub fn stock_index(game: &str, lv: &str, ww2_levels: &[String]) -> Arc<StockIndex> {
    let bundles = stock_bundles(game, lv, ww2_levels);
    let sig: Sig = bundles.iter().map(|b| signature(b)).collect();
    {
        let memo = INDEX.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((s, ix)) = memo.as_ref()
            && *s == sig
        {
            return Arc::clone(ix);
        }
    }
    let read: Vec<std::result::Result<Bundle, String>> = bundles
        .par_iter()
        .map(|b| {
            bc2_gamefs::bundles::entries_path(b)
                .map(|h| {
                    Bundle::new(
                        b,
                        h.entries
                            .into_iter()
                            .filter(|e| !e.is_deleted())
                            .map(|e| (e.name, e.type_name)),
                    )
                })
                .map_err(|e| format!("{b}: {}: {e}", error_kind(&e)))
        })
        .collect();
    let mut ok = Vec::new();
    let mut unread = Vec::new();
    for r in read {
        match r {
            Ok(b) => ok.push(b),
            Err(e) => unread.push(e),
        }
    }
    let ix = Arc::new(StockIndex::new(ok, unread));
    if ix.unread.is_empty() {
        *INDEX.lock().unwrap_or_else(PoisonError::into_inner) = Some((sig, Arc::clone(&ix)));
    }
    ix
}

fn error_kind(e: &bc2_formats::fbrb::FbrbError) -> &'static str {
    if matches!(e, bc2_formats::fbrb::FbrbError::Io(_)) {
        "OSError"
    } else {
        "FbrbError"
    }
}

/// Forgets the kept index (a test, or a game copy changed under the process).
pub fn forget_index() {
    *INDEX.lock().unwrap_or_else(PoisonError::into_inner) = None;
}

fn same_path(a: &str, b: &str) -> bool {
    let norm = |p: &str| p.replace('/', "\\").to_lowercase();
    norm(a) == norm(b)
}

/// `masked_names()`: the lower names `masked_by`'s deleted entries hide (empty when it cannot be read).
#[must_use]
pub fn masked_names(masked_by: &str) -> HashSet<String> {
    bc2_gamefs::bundles::entries_path(masked_by).map_or_else(
        |_| HashSet::new(),
        |h| {
            h.entries
                .iter()
                .filter(|e| e.is_deleted())
                .map(|e| e.name.to_lowercase())
                .collect()
        },
    )
}

/// `live_entries(b)`: a loaded bundle's (name, type) the level can use - its deleted markers left out, and for a
/// bundle of `masked` the names `hide` holds.
///
/// # Errors
/// The bundle cannot be read.
pub fn live_entries(b: &str, masked: &[String], hide: &HashSet<String>) -> Result<Vec<(String, String)>> {
    let h = bc2_gamefs::bundles::entries_path(b).map_err(|e| format!("{b}: {e}"))?;
    let masked_here = masked.iter().any(|m| same_path(m, b));
    Ok(h.entries
        .into_iter()
        .filter(|e| !(e.is_deleted() || masked_here && hide.contains(&e.name.to_lowercase())))
        .map(|e| (e.name, e.type_name))
        .collect())
}

/// `nested_roots(root, roots)`: the sub-folders of root (relative, a trailing `/`) that are roots of their own.
#[must_use]
pub fn nested_roots(root: &str, roots: &[String]) -> Vec<String> {
    let pre = format!("{}/", root.trim_end_matches('/'));
    roots
        .iter()
        .filter(|r| r.to_lowercase().starts_with(&pre.to_lowercase()))
        .map(|r| format!("{}/", r.get(pre.len()..).unwrap_or_default().trim_end_matches('/')))
        .collect()
}

/// What `our_files` reads: the build folder and its roots, earliest first.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct OursSpec {
    /// `work\ww2`.
    pub w: String,
    /// `OURS`: the roots, earliest first (a later root's file of the same name wins).
    pub roots: Vec<String>,
    /// `SKIP`: per root, the relative prefixes that are not level data.
    #[serde(default)]
    pub skip: HashMap<String, Vec<String>>,
    /// `--vietnam-vo`: the `vo` root left out.
    #[serde(default)]
    pub vietnam_vo: bool,
    /// `STOCK_OVER_OURS`: stems (`_lod<n>_data` / `_soot` taken off) of ours left out.
    #[serde(default)]
    pub stock_over_ours: Vec<String>,
}

/// One of our files: (its name as the build wrote it, its path).
pub type OurFile = (String, String);

static SHA: LazyLock<Mutex<HashMap<(String, u64, u128), String>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn file_sha(p: &str) -> Option<String> {
    let m = fs::metadata(p).ok()?;
    let ns = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    let key = (p.to_owned(), m.len(), ns);
    if let Some(s) = SHA.lock().unwrap_or_else(PoisonError::into_inner).get(&key) {
        return Some(s.clone());
    }
    let s = bc2_gamefs::sha256::file(p).ok()?;
    SHA.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(key, s.clone());
    Some(s)
}

fn same_file_content(a: &str, b: &str) -> bool {
    let size = |p: &str| fs::metadata(p).map(|m| m.len()).ok();
    if size(a) != size(b) {
        return false;
    }
    file_sha(a) == file_sha(b)
}

static LOD_TAIL: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"_lod\d+_(data|soot)$").ok());

/// `our_files()`: {lower name: (name, path)} of the builds' `.dbx` / `.res` files (the later root wins, in the place
/// the first gave it) and the names two builds give with other contents.
#[must_use]
pub fn our_files(spec: &OursSpec) -> (OMap<OurFile>, Vec<String>) {
    let mut out: OMap<OurFile> = OMap::new();
    let mut clash = Vec::new();
    for root in &spec.roots {
        if spec.vietnam_vo && root == "vo" {
            continue;
        }
        let top = pjoin(&spec.w, root);
        let mut skip: Vec<String> = spec.skip.get(root).cloned().unwrap_or_default();
        skip.extend(nested_roots(root, &spec.roots));
        let skip: Vec<String> = skip.iter().map(|s| s.to_lowercase()).collect();
        for p in walk(&top) {
            let f = crate::pytext::basename(&p);
            if !(f.ends_with(".dbx") || f.ends_with(".res")) {
                continue;
            }
            let rel = rel_slash(&p, &top);
            let low = rel.to_lowercase();
            if skip.iter().any(|s| low.starts_with(s.as_str())) {
                continue;
            }
            let stem = low.get(..low.len() - 4).unwrap_or_default();
            if !spec.stock_over_ours.is_empty() {
                let bare = LOD_TAIL
                    .as_ref()
                    .map_or_else(|| stem.to_owned(), |r| r.replace(stem, "").into_owned());
                if spec.stock_over_ours.contains(&bare) {
                    continue;
                }
            }
            if let Some((_, old)) = out.get(&low)
                && !same_file_content(old, &p)
            {
                clash.push(format!(
                    "{rel}: {} replaced by {}",
                    crate::pytext::relpath(old, &spec.w),
                    crate::pytext::relpath(&p, &spec.w)
                ));
            }
            out.insert(&low, (rel, p));
        }
    }
    (out, clash)
}

static PRIMARY: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r#"primaryInstance="([^"]+)""#).ok());

/// `primary_type(data)`: the partition's primary instance's (type, guid), each `None` when the XML has none.
///
/// # Errors
/// A damaged partition.
pub fn primary_type(data: &[u8]) -> Result<(Option<String>, Option<String>)> {
    let x = to_xml(data)?;
    Ok(primary_of_xml(&x))
}

/// [`primary_type`] of the XML.
#[must_use]
pub fn primary_of_xml(x: &str) -> (Option<String>, Option<String>) {
    let Some(g) = PRIMARY.as_ref().and_then(|r| r.captures(x)).and_then(|c| c.get(1)) else {
        return (None, None);
    };
    let g = g.as_str();
    let t = RegexBuilder::new(&format!(r#"<instance guid="{}" type="([^"]+)""#, regex::escape(g)))
        .case_insensitive(true)
        .build()
        .ok()
        .and_then(|r| r.captures(x).and_then(|c| c.get(1)).map(|m| m.as_str().to_owned()));
    (t, Some(g.to_owned()))
}

/// The primary instance's guid of a partition file (`primary_type(open(path).read())[1]`).
///
/// # Errors
/// The file cannot be read or is damaged.
pub fn primary_guid_of(path: &str) -> Result<Option<String>> {
    Ok(primary_type(&read(path)?)?.1)
}

/// `MESH_TYPES`: a mesh asset's type -> its mesh set's bundle type.
#[must_use]
pub fn mesh_set_type(asset: &str) -> Option<&'static str> {
    match asset {
        "Render.RigidMeshAsset" => Some("RigidMeshSet"),
        "Render.SkinnedMeshAsset" => Some("SkinnedMeshSet"),
        "Render.CompositeMeshAsset" => Some("CompositeMeshSet"),
        "Render.TreeMeshAsset" => Some("TreeMeshSet"),
        _ => None,
    }
}

/// The lower names the loaded bundles hold (`loaded`: every live entry of `bundles` that exists).
#[must_use]
pub fn loaded_set(bundles: &[String], masked: &[String], hide: &HashSet<String>) -> HashSet<String> {
    let mut out = HashSet::new();
    for b in bundles {
        if Path::new(b).is_file()
            && let Ok(es) = live_entries(b, masked, hide)
        {
            out.extend(es.into_iter().map(|(n, _)| n.to_lowercase()));
        }
    }
    out
}

/// A stock partition's bytes by its lower name, from the first bundle holding it.
///
/// # Errors
/// In no bundle, or the bundle cannot be read.
pub fn stock_data(index: &StockIndex, low: &str) -> Result<Vec<u8>> {
    let (b, n, _) = index.first(low).ok_or_else(|| format!("KeyError: '{low}'"))?;
    bundle_one(b, n)
}
