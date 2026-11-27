//! Stage 10 of the console Onslaught port (round OR11): the port of Project OPFOR's `console_deps.py`'s build and
//! `apply` up to the ledger - every PC partition a console Onslaught port references but does not load, found in the
//! stock PC level bundles of the user's install and copied into the level's Package level-00 bundle with its
//! resources, until nothing is missing.
//!
//! A reference that resolves in no bundle the level loads becomes null at run time, and the engine reads it unchecked
//! (automated load tests 2026-10-30: Atacama's console AA-BMD3, Isla Inocentes' composite meshes, Valparaiso's Mi-28
//! tracer). Loaded, not copied: the level's Package level / ingame / loader bundles, its Dist copies, mp_common,
//! sp_common and the startup bundles. The sources: every stock PC level bundle, the PC map's own levels first, Package
//! before Dist, read in memory. A partition's resources (mesh, LOD data, Havok, textures) are found by its name, not
//! by references: a partition can resolve while its `.res` is in no bundle the level loads.
//!
//! The work folder's files are the retired Python's byte for byte (rust/BENCHMARKS.md "OR11"); the ledger's parts stay
//! in OPFOR's `console_deps.py`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::LazyLock;

use bc2_pyrt::py::OMap;
use regex::Regex;

use super::files::{drop_xml, manifest_lines, opfor_installed, under, write_manifest};
use super::port::{StageOut, walk_all};
use super::{OnsError, R, Source, dbx, fbrb, join, joins, pack, read_text, rmtree, stem4, write};

/// The console levels and the PC maps they are built on (`console_common.PC_MAP`).
pub const PC_MAP: [(&str, &str); 4] = [
    ("ons_mp_002", "mp_002"),
    ("ons_mp_004", "mp_004"),
    ("ons_mp_005", "mp_005"),
    ("ons_mp_008", "mp_008"),
];

static REF: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used, reason = "the pattern is a constant, compiled by the tests")]
    Regex::new(r#"ref="([A-Za-z0-9_!/\-]+)/[0-9A-Fa-f\-]{36}""#).expect("a constant pattern")
});

/// The stage's arguments.
#[derive(Debug, Clone)]
pub struct Args {
    /// Project OPFOR's folder.
    pub root: String,
    /// Its game copy.
    pub game: String,
    /// The level (lower case).
    pub lv: String,
    /// The bundle the stage builds from (the ledger's `source_bundle`).
    pub src_bundle: String,
    /// `--apply` (else `--plan`: what is missing and where it comes from; nothing packed or installed).
    pub write: bool,
}

/// `listing(bundle)`: {lower name: name} of a bundle's live entries, from FbrbTool's `list`.
fn listing(bundle: &str) -> R<OMap<String, String>> {
    let mut out = OMap::default();
    for line in bc2_pyrt::py::splitlines(&fbrb(&["list", bundle])?) {
        let p = bc2_pyrt::py::split(line);
        if p.len() >= 3 && p[0].chars().all(|c| c.is_ascii_digit()) && p[1] != "*deleted*" {
            let last = p[p.len() - 1];
            out.insert(last.to_lowercase(), last.to_owned());
        }
    }
    Ok(out)
}

/// `name.rsplit('.', 1)[0]`.
fn stem(n: &str) -> &str {
    n.rsplit_once('.').map_or(n, |(s, _)| s)
}

/// A source bundle read in memory: its live entries from the header, the partitions' references (DbxEdit's XML of
/// each, made in memory) once.
struct Src {
    path: String,
    entries: HashMap<String, bc2_formats::fbrb::Entry>,
    refs: HashMap<String, HashSet<String>>,
}

impl Src {
    fn open(path: &str) -> R<Self> {
        let h = bc2_gamefs::bundles::entries_path(path).map_err(|e| OnsError::Data(format!("{path}: {e}")))?;
        let entries = h
            .entries
            .into_iter()
            .filter(|e| !e.is_deleted())
            .map(|e| (e.name.to_lowercase(), e))
            .collect();
        Ok(Self {
            path: path.to_owned(),
            entries,
            refs: HashMap::new(),
        })
    }

    fn read(&self, names: &[&str]) -> R<Vec<Vec<u8>>> {
        bc2_gamefs::bundles::read_path(&self.path, names).map_err(|e| OnsError::Data(format!("{}: {e}", self.path)))
    }

    /// The entry's fbrb.manifest line, as FbrbTool's unpack writes it.
    fn line(&self, low: &str) -> R<String> {
        let e = self
            .entries
            .get(low)
            .ok_or_else(|| OnsError::Data(format!("KeyError: '{low}'")))?;
        Ok(format!(
            "{}|{}|{:08X}|{}|{}",
            e.name, e.type_name, e.flags, e.size2, e.size
        ))
    }

    fn refs_of(&mut self, name: &str) -> R<HashSet<String>> {
        if !self.refs.contains_key(name) {
            let data = self.read(&[name])?.pop().unwrap_or_default();
            let text = bc2_formats::dbx::to_xml(&data).map_err(|e| OnsError::Data(format!("{name}: {e}")))?;
            self.refs.insert(
                name.to_owned(),
                REF.captures_iter(&text).map(|c| c[1].to_lowercase()).collect(),
            );
        }
        Ok(self.refs.get(name).cloned().unwrap_or_default())
    }
}

/// Where a loaded partition is read: the level's own unpack, or a bundle in memory.
#[derive(Clone)]
enum At {
    File(String),
    Entry(String, String),
}

/// `refs_of(dbx)` of an unpacked partition: its XML (DbxEdit's, made when missing).
fn file_refs(path: &str) -> R<HashSet<String>> {
    let x = format!("{}.xml", stem4(path));
    if !std::path::Path::new(&x).is_file() {
        let o = bc2_tools::dbxedit::run(&["xml".to_owned(), path.to_owned()], None);
        if o.code != 0 || !std::path::Path::new(&x).is_file() {
            return Err(OnsError::Exit(format!(
                "DbxEdit xml failed on {path}:\n{}{}",
                o.stdout, o.stderr
            )));
        }
    }
    Ok(REF
        .captures_iter(&read_text(&x)?)
        .map(|c| c[1].to_lowercase())
        .collect())
}

/// Names with a prefix rule (`s == part or s.startswith(part + '_')`), answered from a sorted index.
struct Prefixed {
    /// name -> its first place (the dict's insertion order).
    by: BTreeMap<String, usize>,
}

impl Prefixed {
    fn new<'a>(names: impl IntoIterator<Item = &'a String>) -> Self {
        let mut by = BTreeMap::new();
        for (i, n) in names.into_iter().enumerate() {
            by.entry(n.clone()).or_insert(i);
        }
        Self { by }
    }

    /// The first name (in insertion order) that is `part` or starts with `part_`.
    fn first(&self, part: &str) -> Option<&String> {
        let mut best: Option<(&String, usize)> = self.by.get_key_value(part).map(|(k, v)| (k, *v));
        let pre = format!("{part}_");
        for (k, v) in self.by.range(pre.clone()..) {
            if !k.starts_with(&pre) {
                break;
            }
            if best.is_none_or(|(_, b)| *v < b) {
                best = Some((k, *v));
            }
        }
        best.map(|(k, _)| k)
    }
}

/// `glob(os.path.join(top, '*', 'level-00.fbrb'))`: (folder name, path) of each folder holding a level-00.fbrb, less
/// the levels Project OPFOR installed (its WW2 levels: not stock PC data, spec XCONS-ONS-10).
fn level_dirs(top: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(top) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.starts_with('.') {
                continue;
            }
            let p = joins(top, &[&n, "level-00.fbrb"]);
            if std::path::Path::new(&p).exists() && !opfor_installed(&join(top, &n)) {
                out.push((n, p));
            }
        }
    }
    out
}

/// `build(lv)` and `apply(lv, write)` up to the ledger.
///
/// # Errors
/// A tool's failure, a refusal in words, a file that does not read.
#[allow(clippy::too_many_lines, reason = "console_deps.build, step for step")]
pub fn build(a: &Args) -> R<StageOut> {
    let lv = a.lv.as_str();
    let pc = PC_MAP
        .iter()
        .find(|(l, _)| *l == lv)
        .map(|(_, p)| *p)
        .ok_or_else(|| OnsError::Data(format!("KeyError: '{lv}'")))?;
    let g = a.game.as_str();
    let dist = "Dist\\win32";
    let work = joins(&a.root, &["work", "console_deps"]);
    let bundle = joins(g, &["Package", "levels", lv, "level-00.fbrb"]);
    let src = joins(&work, &[lv, "pkg"]);
    rmtree(&src)?;
    fbrb(&["unpack", &a.src_bundle, &src])?;
    dbx(&["xml", &src])?;
    // the level's own Package bundle is read as the bundle this stage builds from: the game's copy holds this stage's
    // own earlier copies on a redo (spec XCONS-ONS-10)
    let own = |p: &str| -> String {
        if p == bundle {
            a.src_bundle.clone()
        } else {
            p.to_owned()
        }
    };
    let isfile = |p: &str| std::path::Path::new(&own(p)).is_file();
    let mut loaded: HashSet<String> = HashSet::new();
    let mut loaded_from: Vec<String> = Vec::new();
    for w in ["Package", dist] {
        for s in ["level", "ingame", "loader"] {
            loaded_from.push(joins(g, &[w, "levels", lv, &format!("{s}-00.fbrb")]));
        }
    }
    for c in ["mp_common", "sp_common"] {
        loaded_from.push(joins(g, &[dist, "levels", c, "level-00.fbrb"]));
    }
    for w in ["Package", dist] {
        loaded_from.push(joins(g, &[w, "async", "startup-00.fbrb"]));
    }
    for b in loaded_from.iter().filter(|b| isfile(b)) {
        loaded.extend(listing(&own(b))?.keys().map(|n| stem(n).to_owned()));
    }
    // sources: stock PC level bundles - the PC map's own first, Package before Dist
    let mut cands: Vec<(u8, bool, String)> = Vec::new();
    for w in ["Package", dist] {
        let mut found = level_dirs(&joins(g, &[w, "levels"]));
        found.sort_by(|x, y| x.1.cmp(&y.1));
        for (name, b) in found {
            let name = name.to_lowercase();
            if name.starts_with("ons_")
                || name.starts_with("opfor_")
                || ["mp_common", "sp_common", "mp2_common"].contains(&name.as_str())
            {
                continue;
            }
            cands.push((u8::from(!name.starts_with(pc)), w != "Package", b));
        }
    }
    cands.sort();
    let mut sources: Vec<(String, OMap<String, String>)> = Vec::new();
    for (_, _, b) in cands {
        let l = listing(&b)?;
        sources.push((b, l));
    }
    let names_of: HashMap<&str, &OMap<String, String>> = sources.iter().map(|(b, l)| (b.as_str(), l)).collect();
    let mut where_: HashMap<String, String> = HashMap::new();
    for (b, names) in &sources {
        for low in names.keys() {
            where_.entry(stem(low).to_owned()).or_insert_with(|| b.clone());
        }
    }
    let mut xmls = Vec::new();
    walk_all(&src, &mut xmls);
    xmls.retain(|f| f.ends_with(".xml"));
    let mut refs: HashSet<String> = HashSet::new();
    for x in &xmls {
        refs.extend(REF.captures_iter(&read_text(x)?).map(|c| c[1].to_lowercase()));
    }
    // partitions the level loads, where to read them (its own Package bundle unpacked in src; the others in memory)
    let mut loaded_dbx: HashMap<String, At> = HashMap::new();
    let pkg_top = join(g, "Package");
    let mut dbx_from = vec![
        joins(g, &[dist, "levels", lv, "level-00.fbrb"]),
        joins(g, &["Package", "levels", lv, "level-00.fbrb"]),
    ];
    for c in ["sp_common", "mp_common"] {
        dbx_from.push(joins(g, &[dist, "levels", c, "level-00.fbrb"]));
    }
    for b in dbx_from.iter().filter(|b| isfile(b)) {
        let pkg = b.starts_with(&pkg_top);
        for (low, name) in &listing(&own(b))?.items {
            if let Some(part) = low.strip_suffix(".dbx") {
                let at = if pkg {
                    At::File(under(&src, name))
                } else {
                    At::Entry(b.clone(), name.clone())
                };
                loaded_dbx.insert(part.to_owned(), at);
            }
        }
    }
    // resources are found by their partition's name (a partition can resolve while its .res is in no bundle the
    // level loads: Atacama's AA-BMD3), so every partition the level reaches has its resources checked too
    let mut res_from: Vec<String> = Vec::new();
    for w in ["Package", dist] {
        for s in ["level", "ingame", "loader"] {
            res_from.push(joins(g, &[w, "levels", lv, &format!("{s}-00.fbrb")]));
        }
    }
    for c in ["mp_common", "sp_common"] {
        for s in ["level", "ingame"] {
            res_from.push(joins(g, &[dist, "levels", c, &format!("{s}-00.fbrb")]));
        }
    }
    let mut loaded_res: BTreeSet<String> = BTreeSet::new();
    for b in res_from.iter().filter(|b| isfile(b)) {
        for n in listing(&own(b))?.keys() {
            if let Some(s) = n.strip_suffix(".res") {
                loaded_res.insert(s.to_owned());
            }
        }
    }
    let mut res_where: OMap<String, String> = OMap::default();
    for (b, names) in &sources {
        for low in names.keys() {
            if let Some(s) = low.strip_suffix(".res") {
                res_where.entry_or(s.to_owned(), || b.clone());
            }
        }
    }
    let res_index = Prefixed::new(res_where.keys());
    let loaded_res_has = |part: &str| {
        let pre = format!("{part}_");
        loaded_res.contains(part)
            || loaded_res
                .range(pre.clone()..)
                .next()
                .is_some_and(|k| k.starts_with(&pre))
    };
    let mut open: HashMap<String, Src> = HashMap::new();
    let mut need: BTreeMap<String, String> = BTreeMap::new();
    let mut res_need: OMap<String, String> = OMap::default();
    let mut unresolved: BTreeSet<String> = BTreeSet::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut todo: Vec<String> = refs.iter().cloned().collect();
    todo.sort();
    let more = |got: HashSet<String>, seen: &HashSet<String>, todo: &mut Vec<String>| {
        let mut add: Vec<String> = got.into_iter().filter(|r| !seen.contains(r)).collect();
        add.sort();
        todo.extend(add);
    };
    while let Some(part) = todo.pop() {
        if !seen.insert(part.clone()) {
            continue;
        }
        if let Some(first) = res_index.first(&part)
            && !loaded_res_has(&part)
        {
            let b = res_where.get(first).cloned().unwrap_or_default();
            res_need.insert(part.clone(), b);
        }
        if let Some(at) = loaded_dbx.get(&part).cloned() {
            let got = match at {
                At::File(p) => file_refs(&p)?,
                At::Entry(b, n) => {
                    if !open.contains_key(&b) {
                        open.insert(b.clone(), Src::open(&b)?);
                    }
                    open.get_mut(&b).map_or_else(|| Ok(HashSet::new()), |s| s.refs_of(&n))?
                }
            };
            more(got, &seen, &mut todo);
            continue;
        }
        if loaded.contains(&part) {
            continue;
        }
        let Some(b) = where_.get(&part).cloned() else {
            unresolved.insert(part);
            continue;
        };
        need.insert(part.clone(), b.clone());
        let dbx_name = names_of
            .get(b.as_str())
            .and_then(|n| n.get(&format!("{part}.dbx")).cloned());
        if let Some(n) = dbx_name {
            if !open.contains_key(&b) {
                open.insert(b.clone(), Src::open(&b)?);
            }
            let got = open.get_mut(&b).map_or_else(|| Ok(HashSet::new()), |s| s.refs_of(&n))?;
            more(got, &seen, &mut todo);
        }
    }
    for (part, b) in &res_need.items {
        need.entry(part.clone()).or_insert_with(|| b.clone());
    }
    // copy each needed partition with its resources from its source bundle
    let man = join(&src, "fbrb.manifest");
    let mut lines = manifest_lines(&man)?;
    let mut have: HashSet<String> = lines
        .iter()
        .map(|l| l.split('|').next().unwrap_or_default().to_lowercase())
        .collect();
    let mut added = 0usize;
    let mut by_source: BTreeMap<String, usize> = BTreeMap::new();
    for (part, b) in &need {
        if !open.contains_key(b) {
            open.insert(b.clone(), Src::open(b)?);
        }
        let s = open.get(b).ok_or_else(|| OnsError::Data(format!("{b}: not opened")))?;
        let pre = format!("{part}_");
        let take: Vec<(String, String)> = names_of.get(b.as_str()).map_or_else(Vec::new, |n| {
            n.items
                .iter()
                .filter(|(low, _)| {
                    let st = stem(low);
                    (st == part || (low.ends_with(".res") && st.starts_with(&pre))) && !have.contains(low.as_str())
                })
                .cloned()
                .collect()
        });
        let datas = if take.is_empty() {
            Vec::new()
        } else {
            s.read(&take.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>())?
        };
        for ((low, name), data) in take.iter().zip(datas) {
            if have.contains(low) {
                continue;
            }
            write(&under(&src, name), &data)?;
            lines.push(s.line(low)?);
            have.insert(low.clone());
            added += 1;
        }
        let key = bc2_pyrt::py::basename(&bc2_pyrt::py::dirname(b));
        *by_source.entry(key).or_insert(0) += 1;
    }
    write_manifest(&man, &lines)?;
    drop_xml(&src)?;
    let from: Vec<String> = by_source.iter().map(|(k, v)| format!("{k} {v}")).collect();
    let mut out = StageOut::default();
    out.printed.push(format!(
        "{lv}: {} partitions it references but does not load, {added} files copied in (from {}); {} found nowhere",
        need.len(),
        if from.is_empty() {
            "-".to_owned()
        } else {
            from.join(", ")
        },
        unresolved.len()
    ));
    for (p, b) in need.iter().take(40) {
        out.printed.push(format!(
            "  {p}  <- {}",
            bc2_pyrt::py::basename(&bc2_pyrt::py::dirname(b))
        ));
    }
    if !unresolved.is_empty() {
        let first: Vec<&str> = unresolved.iter().take(20).map(String::as_str).collect();
        out.printed.push(format!(
            "  nowhere: {}{}",
            first.join(", "),
            if unresolved.len() > 20 { " ..." } else { "" }
        ));
    }
    if !a.write || added == 0 {
        return Ok(out);
    }
    let built = joins(&work, &[lv, "level-00.fbrb"]);
    pack(&src, &built)?;
    out.writes.push((bundle, Source::File(built)));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_name_with_a_prefix_in_insertion_order() {
        let names: Vec<String> = ["a_lod1", "b", "a", "a_lod0", "ab_x"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        let ix = Prefixed::new(&names);
        assert_eq!(ix.first("a").map(String::as_str), Some("a_lod1"));
        assert_eq!(ix.first("b").map(String::as_str), Some("b"));
        assert_eq!(ix.first("ab").map(String::as_str), Some("ab_x"));
        assert_eq!(ix.first("c"), None);
    }

    #[test]
    fn stems_and_the_pattern() {
        assert_eq!(stem("a/b.c.dbx"), "a/b.c");
        assert_eq!(stem("noext"), "noext");
        let x = r#"<field ref="Objects/X!y/12345678-1234-1234-1234-123456789abc" name="A" />"#;
        let got: Vec<String> = REF.captures_iter(x).map(|c| c[1].to_owned()).collect();
        assert_eq!(got, vec!["Objects/X!y".to_owned()]);
    }
}
