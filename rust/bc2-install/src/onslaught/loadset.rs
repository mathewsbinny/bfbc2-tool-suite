//! Stage 4 of the console Onslaught port (round OR9): the port of Project OPFOR's `console_loadset.py`'s `plan` and
//! `apply` up to the ledger - the level's load set without PC `mp2_common`.
//!
//! The console's Onslaught levels load `package:levels/mp2_common`, the console DLC's shared bundle; PC has a bundle of
//! that name with other content, whose `*deleted*` markers trim mp_common data the ported levels place (see-through
//! buildings, invisible guns on Nelson Bay). So the level's `package.mft` blocks lose their mp2_common targets, and
//! the files the level takes only from PC mp2_common are copied into its own Package level bundle: the folders of
//! [`LEVELS`] (or, for Atacama, a pattern), then every partition the level's bundle references that only mp2_common
//! provides, and what those reference in turn (each with its resources).
//!
//! The work folder's files are the Python's byte for byte (rust/BENCHMARKS.md "OR9"); the install (one ledger commit,
//! the bundle before the manifest) stays in OPFOR's `console_loadset.py`.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::port::{StageOut, read_text_utf8, unpack_cached, walk_all};
use super::{OnsError, R, Source, dbx, fbrb, join, joins, latin1, pack, read, rmtree, write};

/// What a level copies from PC mp2_common besides the closure: entry prefixes, or (Atacama) a pattern of names no
/// bundle the level loads carries.
pub enum Take {
    /// Entries under these prefixes (lower case).
    Prefixes(&'static [&'static str]),
    /// Entries whose lower-case name the pattern finds and no bundle the level loads anyway has.
    Match(&'static str),
}

/// The per-level lists (`console_loadset.LEVELS`).
pub const LEVELS: [(&str, Take); 4] = [
    (
        "ons_mp_008",
        Take::Prefixes(&[
            "objects/vehicles/land/cav/",
            "animations/vehicles/land/cav/",
            "sound/vehicles/land/cav/",
            "textures/vehicles/cav/",
            "effects/destruction/specific/vehicles/land/cav/",
            "objects/vehicles/common/animtree/vehicledynamicpassenger3animtree",
            "objects/vehicles/common/animtree/vehiclesittinggunneranimtree",
        ]),
    ),
    // Valparaiso: the Mi-28 Havoc is the only partition it takes from PC mp2_common alone
    (
        "ons_mp_002",
        Take::Prefixes(&[
            "objects/vehicles/air/mi28/",
            "animations/vehicles/air/mi28/",
            "sound/vehicles/air/mi28_havoc/",
            "textures/vehicles/mi28/",
            "effects/destruction/specific/vehicles/air/mi28/",
            "systems/mesh/mi28.",
        ]),
    ),
    // Isla Inocentes: the Mi-28 too, the wind power plant and the QLZ-87's animation tree
    (
        "ons_mp_004",
        Take::Prefixes(&[
            "objects/vehicles/air/mi28/",
            "animations/vehicles/air/mi28/",
            "sound/vehicles/air/mi28_havoc/",
            "textures/vehicles/mi28/",
            "effects/destruction/specific/vehicles/air/mi28/",
            "systems/mesh/mi28.",
            "objects/objects/windpowerplant/",
            "systems/mesh/windpowerplant",
            "objects/weapons/stationary/ru_gmg_qlz87/stationary_rgl_animtree.",
        ]),
    ),
    // Atacama: the AH-64 and Mi-28 with their cockpit HUDs, the ZU-23, the TOW-2, the tracer gun
    (
        "ons_mp_005",
        Take::Match("ah64|mi28|havoc|zu23|zu_23|tow2|hud_helicopter|tracer_gun_firing"),
    ),
];

static REF_RX: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used, reason = "a constant pattern, compiled by the tests")]
    Regex::new(r#"ref="([A-Za-z0-9_/\-]+)/[0-9A-Fa-f\-]{36}""#).expect("a constant pattern")
});

/// The game copy's and work folder's paths of the stage.
struct Paths {
    game: String,
    mft: String,
    mp2: String,
    work: String,
}

/// Python's `str.isdigit()` on a listing's size column (ASCII digits: FbrbTool's listing).
fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

/// The live entries of an FbrbTool listing (`len(p) >= 3 and p[0].isdigit() and p[1] != '*deleted*'`): their names.
fn listed(bundle: &str) -> R<Vec<String>> {
    let out = fbrb(&["list", bundle])?;
    Ok(bc2_pyrt::py::splitlines(&out)
        .into_iter()
        .filter_map(|line| {
            let p = bc2_pyrt::py::split(line);
            match p.as_slice() {
                [a, b, .., last] if p.len() >= 3 && is_digits(a) && *b != "*deleted*" => Some((*last).to_owned()),
                _ => None,
            }
        })
        .collect())
}

/// `manifest_without_mp2(text, lv)`: the manifest with the mp2_common targets removed from the level's own
/// `BundleSource` blocks, and how many.
///
/// # Panics
/// Never: the patterns are constants and an escaped level name.
#[must_use]
pub fn manifest_without_mp2(text: &str, lv: &str) -> (String, usize) {
    #[allow(clippy::expect_used, reason = "an escaped level name in a constant pattern")]
    let src = Regex::new(&format!("(?i)^BundleSource game:levels/{}/", regex::escape(lv))).expect("a pattern");
    #[allow(clippy::expect_used, reason = "a constant pattern")]
    let tgt = Regex::new("(?i)^BundleTarget package:levels/mp2_common/").expect("a pattern");
    let mut out: Vec<&str> = Vec::new();
    let mut removed = 0;
    let mut block: Option<&str> = None;
    for line in text.split('\n') {
        if line.starts_with("BundleSource ") {
            block = Some(line);
        } else if bc2_pyrt::py::strip(line).is_empty() {
            block = None;
        }
        if block.is_some_and(|b| src.is_match(b)) && tgt.is_match(line) {
            removed += 1;
            continue;
        }
        out.push(line);
    }
    (out.join("\n"), removed)
}

/// `loaded_names(lv)`: every entry (with extension, lower case) of the bundles the level loads besides mp2_common.
fn loaded_names(p: &Paths, lv: &str) -> R<HashSet<String>> {
    let mut names = HashSet::new();
    let mut bundles = vec![joins(&p.game, &["Package", "levels", lv, "level-00.fbrb"])];
    for c in ["mp_common", "sp_common"] {
        bundles.push(joins(&p.game, &["Dist", "win32", "levels", c, "level-00.fbrb"]));
    }
    for b in bundles {
        names.extend(listed(&b)?.into_iter().map(|n| n.to_lowercase()));
    }
    Ok(names)
}

/// `cav_entries(lv)`: the mp2_common entries the level's [`LEVELS`] entry names.
fn cav_entries(p: &Paths, take: &Take, lv: &str) -> R<Vec<String>> {
    let (rx, have) = match take {
        Take::Match(m) => (
            Some(Regex::new(m).map_err(|e| OnsError::Data(e.to_string()))?),
            loaded_names(p, lv)?,
        ),
        Take::Prefixes(_) => (None, HashSet::new()),
    };
    let mut rows = Vec::new();
    for name in listed(&p.mp2)? {
        let low = name.to_lowercase();
        let hit = match (take, &rx) {
            (Take::Match(_), Some(rx)) => rx.is_match(&low) && !have.contains(&low),
            (Take::Prefixes(pre), _) => pre.iter().any(|x| low.starts_with(x)),
            _ => false,
        };
        if hit {
            rows.push(name);
        }
    }
    Ok(rows)
}

/// `bundle_names(bundle)`: the live entries' names without extension (lower case).
fn bundle_names(bundle: &str) -> R<HashSet<String>> {
    Ok(listed(bundle)?
        .into_iter()
        .map(|n| {
            let l = n.to_lowercase();
            l.rsplit_once('.').map_or(l.clone(), |x| x.0.to_owned())
        })
        .collect())
}

/// The partitions an XML text references (lower case).
fn refs_in(path: &str) -> R<Vec<String>> {
    let t = String::from_utf8_lossy(&read(path)?).into_owned();
    Ok(REF_RX.captures_iter(&t).map(|c| c[1].to_lowercase()).collect())
}

/// `closure_entries(lv)`: every mp2_common entry the level needs - partitions its bundle references that only
/// mp2_common provides, then what those reference in turn, each with its resources (the partition's `.res` and
/// `<name>_*.res`). The partitions, sorted.
fn closure_entries(p: &Paths, lv: &str) -> R<(Vec<String>, Vec<String>)> {
    let scan = joins(&p.work, &[lv, "scan"]);
    rmtree(&scan)?;
    fbrb(&[
        "unpack",
        &joins(&p.game, &["Package", "levels", lv, "level-00.fbrb"]),
        &scan,
    ])?;
    dbx(&["xml", &scan])?;
    let mut refs: HashSet<String> = HashSet::new();
    let mut files = Vec::new();
    walk_all(&scan, &mut files);
    for f in files.iter().filter(|f| f.ends_with(".xml")) {
        refs.extend(refs_in(f)?);
    }
    let mut others: HashSet<String> = HashSet::new();
    let mut bundles: Vec<String> = ["level", "ingame", "loader"]
        .iter()
        .map(|s| joins(&p.game, &["Package", "levels", lv, &format!("{s}-00.fbrb")]))
        .collect();
    for c in ["mp_common", "sp_common"] {
        bundles.push(joins(&p.game, &["Dist", "win32", "levels", c, "level-00.fbrb"]));
    }
    bundles.push(joins(&p.game, &["Package", "async", "startup-00.fbrb"]));
    bundles.push(joins(&p.game, &["Dist", "win32", "async", "startup-00.fbrb"]));
    for b in bundles {
        if Path::new(&b).is_file() {
            others.extend(bundle_names(&b)?);
        }
    }
    rmtree(&scan)?;
    let mp2_dir = join(&p.work, "mp2_common");
    if !Path::new(&mp2_dir).is_dir() {
        fbrb(&["unpack", &p.mp2, &mp2_dir])?;
    }
    // rows: lower-case name -> name, in the listing's order (a dict: a name again keeps its first place)
    let mut rows: Vec<(String, String)> = Vec::new();
    let mut row_at: HashMap<String, usize> = HashMap::new();
    for name in listed(&p.mp2)? {
        let low = name.to_lowercase();
        if let Some(&i) = row_at.get(&low) {
            if let Some(r) = rows.get_mut(i) {
                r.1 = name;
            }
        } else {
            row_at.insert(low.clone(), rows.len());
            rows.push((low, name));
        }
    }
    let stem = |low: &str| low.rsplit_once('.').map_or(low.to_owned(), |x| x.0.to_owned());
    let mut stems: HashMap<String, Vec<String>> = HashMap::new();
    for (low, _) in &rows {
        stems.entry(stem(low)).or_default().push(low.clone());
    }
    let name_of: HashMap<&str, &str> = rows.iter().map(|(l, n)| (l.as_str(), n.as_str())).collect();
    let mut need: HashSet<String> = HashSet::new();
    // the order the partitions are visited in does not change the closure (nor the XML files written)
    let mut todo: Vec<String> = refs
        .iter()
        .filter(|r| stems.contains_key(*r) && !others.contains(*r))
        .cloned()
        .collect();
    todo.sort();
    while let Some(part) = todo.pop() {
        if need.contains(&part) {
            continue;
        }
        need.insert(part.clone());
        let dbx_name = stems
            .get(&part)
            .and_then(|v| v.iter().find(|x| x.ends_with(".dbx")))
            .and_then(|x| name_of.get(x.as_str()));
        if let Some(d) = dbx_name {
            let f = joins(&mp2_dir, &d.split('/').collect::<Vec<_>>());
            let x = format!("{}.xml", super::stem4(&f));
            if !Path::new(&x).is_file() {
                dbx(&["xml", &f])?;
            }
            for r in refs_in(&x)? {
                if stems.contains_key(&r) && !others.contains(&r) && !need.contains(&r) {
                    todo.push(r);
                }
            }
        }
    }
    let mut out = Vec::new();
    for (low, name) in &rows {
        let s = stem(low);
        if need.contains(&s) || (low.ends_with(".res") && need.iter().any(|p| s.starts_with(&format!("{p}_")))) {
            out.push(name.clone());
        }
    }
    let mut sorted: Vec<String> = need.into_iter().collect();
    sorted.sort();
    Ok((out, sorted))
}

/// What stage 4 is given.
#[derive(Debug, Clone)]
pub struct Args {
    /// The OPFOR folder (`console_common.OPFOR`): its `work\console_loadset\` is written.
    pub root: String,
    /// The game copy: read only.
    pub game: String,
    /// The Onslaught level (lower case).
    pub lv: String,
    /// `--apply` (the bundle packed, the manifest's new text) rather than `--plan`.
    pub write: bool,
}

/// `console_loadset.plan(lv)` and, with `write`, `apply`'s build: the writes are the level's bundle (when files were
/// added), then `package.mft` (when its blocks changed); the lines after the install in [`StageOut::after`].
pub fn build(a: &Args) -> R<StageOut> {
    let lv = a.lv.as_str();
    let p = Paths {
        game: a.game.clone(),
        mft: joins(&a.game, &["Dist", "win32", "package.mft"]),
        mp2: joins(&a.game, &["Package", "levels", "mp2_common", "level-00.fbrb"]),
        work: joins(&a.root, &["work", "console_loadset"]),
    };
    let text: String = read(&p.mft)?.iter().map(|&c| char::from(c)).collect();
    let (_, removed) = manifest_without_mp2(&text, lv);
    let mut files = match LEVELS.iter().find(|(l, _)| *l == lv) {
        Some((_, take)) => cav_entries(&p, take, lv)?,
        None => Vec::new(),
    };
    let (auto, _need) = closure_entries(&p, lv)?;
    let have: HashSet<String> = files.iter().map(|x| x.to_lowercase()).collect();
    files.extend(auto.into_iter().filter(|f| !have.contains(&f.to_lowercase())));
    let mut out = StageOut::default();
    out.printed.push(format!(
        "{lv}: package.mft - {removed} mp2_common targets removed from its blocks; {} files copied from PC \
         mp2_common into its Package level bundle",
        files.len()
    ));
    if !a.write {
        return Ok(out);
    }
    let bundle = joins(&a.game, &["Package", "levels", lv, "level-00.fbrb"]);
    let src = joins(&p.work, &[lv, "pkg"]);
    let mp2 = join(&p.work, "mp2_common");
    rmtree(&src)?;
    fbrb(&["unpack", &bundle, &src])?;
    unpack_cached(&p.mp2, &mp2)?;
    let mut mp2_lines: HashMap<String, String> = HashMap::new();
    for l in bc2_pyrt::py::splitlines(&read_text_utf8(&join(&mp2, "fbrb.manifest"))?) {
        if l.contains('|') && !l.starts_with('#') {
            mp2_lines.insert(l.split('|').next().unwrap_or_default().to_lowercase(), l.to_owned());
        }
    }
    let man = join(&src, "fbrb.manifest");
    let mut lines: Vec<String> = bc2_pyrt::py::splitlines(&read_text_utf8(&man)?)
        .into_iter()
        .map(str::to_owned)
        .collect();
    let have: HashSet<String> = lines
        .iter()
        .filter(|l| l.contains('|'))
        .map(|l| l.split('|').next().unwrap_or_default().to_lowercase())
        .collect();
    let mut added = 0usize;
    for f in &files {
        if have.contains(&f.to_lowercase()) {
            continue;
        }
        let parts: Vec<&str> = f.split('/').collect();
        write(&joins(&src, &parts), &read(&joins(&mp2, &parts))?)?;
        lines.push(
            mp2_lines
                .get(&f.to_lowercase())
                .cloned()
                .ok_or_else(|| OnsError::Data(format!("KeyError: '{}'", f.to_lowercase())))?,
        );
        added += 1;
    }
    if added == 0 {
        out.printed.push(format!("{lv}: every file it needs is in its bundle"));
    } else {
        write(&man, (lines.join("\n") + "\n").as_bytes())?;
        let built = joins(&p.work, &[lv, "level-00.fbrb"]);
        pack(&src, &built)?;
        out.writes.push((bundle.clone(), Source::File(built)));
        out.after.push(format!(
            "  {added} files added to {} (the bundle before it is kept by the install ledger)",
            rel_to(&bundle, &a.root)
        ));
    }
    // the manifest: this level's blocks lose mp2_common; other levels' blocks unchanged (the current text)
    let cur: String = read(&p.mft)?.iter().map(|&c| char::from(c)).collect();
    let (cur_new, _) = manifest_without_mp2(&cur, lv);
    if cur_new != cur {
        out.writes.push((p.mft.clone(), Source::Bytes(latin1(&cur_new))));
        out.after
            .push("  package.mft edited (the manifest before it is kept by the install ledger)".into());
    }
    Ok(out)
}

/// `os.path.relpath(path, root)` for a path under root.
fn rel_to(path: &str, root: &str) -> String {
    crate::pytext::relpath(path, root)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn manifest_blocks() {
        let _ = &*REF_RX;
        let t = "BundleSource game:levels/ons_mp_002/level-00.fbrb\r\nBundleTarget package:levels/mp2_common/level-00.fbrb\r\nBundleTarget package:levels/mp_common/level-00.fbrb\r\n\r\nBundleSource game:levels/mp_002/level-00.fbrb\r\nBundleTarget package:levels/mp2_common/level-00.fbrb\r\n";
        let (n, k) = manifest_without_mp2(t, "ons_mp_002");
        assert_eq!(k, 1);
        assert_eq!(
            n,
            "BundleSource game:levels/ons_mp_002/level-00.fbrb\r\nBundleTarget package:levels/mp_common/level-00.fbrb\r\n\r\nBundleSource game:levels/mp_002/level-00.fbrb\r\nBundleTarget package:levels/mp2_common/level-00.fbrb\r\n"
        );
        assert!(is_digits("12") && !is_digits("") && !is_digits("1a"));
    }
}
