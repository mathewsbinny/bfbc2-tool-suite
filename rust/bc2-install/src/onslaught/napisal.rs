//! The Napisal-level fixes (round OR13): ports of Project OPFOR's `apply_ai_template.py` (here),
//! `apply_combat_squads.py` ([`super::squads`]) and `apply_sector_offsets.py` ([`super::sectoroff`]) - edits to
//! Napisal's Onslaught levels (ONS_MP_002, ONS_MP_004, ONS_MP_005GR, ONS_MP_008) in OPFOR's game copy.
//!
//! **The AI template** (`apply_ai_template.py`): Napisal's AI spawners use `objects/soldiers/templates/defaultsoldier`,
//! whose sensing is a placeholder (combat field of view 0, no alert range): his AI aim and never fire. Per level,
//! from its own load set (its `BundleSource game:levels/<lv>/level-00.fbrb` block in `package.mft`) and the templates
//! its AI spawners use:
//! - `sp_common` not loaded (ONS_MP_002): the grunt template chain (`SpawnTemplate_Enemy_Grunt` ->
//!   `Enemy_Grunt_Sense`, `Enemy_Grunt_Bhv`, from the user's own `sp_common` bundle) packed into the level's bundles,
//!   then the swap;
//! - `sp_common` loaded (ONS_MP_004, ONS_MP_005GR): the swap only;
//! - no spawner on DefaultSoldier (ONS_MP_008): skipped.
//!
//! The swap sets `Template` on every SSAiPlayer spawner whose template is DefaultSoldier, copied (DbxEdit `copyfield`)
//! from a campaign spawner of `sp_004_b` that uses the grunt template. The edited level must report exactly the
//! DbxEdit check errors the original does (Napisal's ONS_MP_005GR has 4 before any edit).
//!
//! OPFOR's scripts keep the command lines, the install ledger's calls (is-top, the check before the work, the
//! install, the revert) and the scratch clean-up; the work folder's files are the retired Python's byte for byte
//! (rust/BENCHMARKS.md "OR13").

use std::sync::LazyLock;

use regex::Regex;

use super::port::{read_text_utf8, write_text_utf8};
use super::{OnsError, R, copy, dbx, fbrb, join, joins, pack, read_text, rmtree, write};

/// The levels the fix knows (`LEVELS`): lower case -> the level partition's name.
pub const LEVELS: [(&str, &str); 4] = [
    ("ons_mp_002", "ONS_MP_002"),
    ("ons_mp_004", "ONS_MP_004"),
    ("ons_mp_005gr", "ONS_MP_005GR"),
    ("ons_mp_008", "ONS_MP_008"),
];
/// The grunt template chain packed where `sp_common` is not loaded (`PARTS`).
pub const PARTS: [&str; 3] = [
    "Objects/Soldiers/Templates/SpawnTemplate_Enemy_Grunt.dbx",
    "Objects/Soldiers/Templates/AI/Enemy_Grunt_Sense.dbx",
    "Objects/Soldiers/Templates/AI/Enemy_Grunt_Bhv.dbx",
];
/// The template every DefaultSoldier spawner is given.
pub const TARGET: &str = "objects/soldiers/templates/spawntemplate_enemy_grunt/2eedbef1-52de-4b20-819c-e71097c15cbe";
/// The placeholder template's partition (with its slash).
pub const PLACEHOLDER: &str = "objects/soldiers/templates/defaultsoldier/";

#[allow(clippy::expect_used, reason = "constant patterns, compiled by the tests")]
static TEMPLATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<field (?:name="Template" ref="([^"]*)"|ref="([^"]*)" name="Template")\s*/>"#).expect("pattern")
});
#[allow(clippy::expect_used, reason = "constant patterns, compiled by the tests")]
static GUID_TAIL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"/[0-9a-f-]{36}$").expect("pattern"));

// ------------------------------------------------------------------------------------------------ shared

/// `os.path.relpath(p, root)` for a path under `root` (else the path as it is).
pub(crate) fn rel(p: &str, root: &str) -> String {
    let b = root.trim_end_matches(['\\', '/']);
    if p.len() > b.len() && p[..b.len()].eq_ignore_ascii_case(b) && p[b.len()..].starts_with(['\\', '/']) {
        p[b.len() + 1..].replace('/', "\\")
    } else {
        p.to_owned()
    }
}

/// `format(x, '.0%')`.
pub(crate) fn pct(x: f64) -> String {
    format!("{:.0}%", x * 100.0)
}

/// `bytes.find(pat) >= 0`'s position.
pub(crate) fn find_bytes(hay: &[u8], pat: &[u8]) -> Option<usize> {
    if pat.is_empty() {
        return Some(0);
    }
    hay.windows(pat.len()).position(|w| w == pat)
}

/// DbxEdit's XML of a partition, written beside it, read as `errors='replace'` text.
fn xml_of(path: &str) -> R<String> {
    dbx(&["xml", path])?;
    read_text(&format!("{}.xml", super::stem4(path)))
}

fn level_name(lv: &str) -> R<&'static str> {
    LEVELS
        .iter()
        .find(|(l, _)| *l == lv)
        .map(|(_, n)| *n)
        .ok_or_else(|| OnsError::Data(format!("KeyError: '{lv}'")))
}

/// `load_set(lv)`: the bundles that load with the level's `level-00.fbrb` (its `package.mft` block's targets, lower
/// case).
pub fn load_set(game: &str, lv: &str) -> R<Vec<String>> {
    let mft = joins(game, &["Dist", "win32", "package.mft"]);
    let text = read_text(&mft)?;
    let lines = bc2_pyrt::py::splitlines(&text);
    let src = format!("BundleSource game:levels/{lv}/level-00.fbrb").to_lowercase();
    for (i, line) in lines.iter().enumerate() {
        if bc2_pyrt::py::strip(line).to_lowercase() == src {
            let mut out = Vec::new();
            for x in &lines[i + 1..] {
                let Some(rest) = x.strip_prefix("BundleTarget ") else {
                    break;
                };
                out.push(bc2_pyrt::py::strip(rest).to_lowercase());
            }
            return Ok(out);
        }
    }
    Err(OnsError::Exit(format!("no \"{src}\" block in package.mft")))
}

/// `bundles(lv)`: the level's Dist and Package `level-00.fbrb` that exist.
#[must_use]
pub fn bundles(game: &str, lv: &str) -> Vec<String> {
    [
        joins(game, &["Dist", "win32", "levels", lv, "level-00.fbrb"]),
        joins(game, &["Package", "levels", lv, "level-00.fbrb"]),
    ]
    .into_iter()
    .filter(|b| std::path::Path::new(b).is_file())
    .collect()
}

/// `level_entry(listing, lv)`: the level partition's name as FbrbTool's listing spells it.
fn level_entry(listing: &str, lv: &str) -> R<Option<String>> {
    let want = format!("levels/{}.dbx", level_name(lv)?).to_lowercase();
    Ok(bc2_pyrt::py::splitlines(listing)
        .into_iter()
        .filter(|x| !bc2_pyrt::py::strip(x).is_empty())
        .filter_map(|x| bc2_pyrt::py::split(x).last().map(|s| (*s).to_owned()))
        .find(|t| t.to_lowercase() == want))
}

/// `template(body)`: the Template field's ref.
fn template(body: &str) -> Option<String> {
    TEMPLATE
        .captures(body)
        .map(|c| c.get(1).or_else(|| c.get(2)).map_or("", |m| m.as_str()).to_owned())
}

/// `ai_spawners(xml)`: (guid, template ref) of every SSAiPlayer spawner, sorted.
pub fn ai_spawners(xml: &str) -> R<Vec<(String, Option<String>)>> {
    let inst = bc2_formats::xmlinst::instances(xml, Some(".SoldierSpawnEntityData"))
        .ok_or_else(|| OnsError::Data("an instance the reader does not read".into()))?;
    let mut out: Vec<(String, Option<String>)> = inst
        .into_iter()
        .filter(|(_, _, b)| b.contains("SSAiPlayer"))
        .map(|(_, g, b)| (g.to_owned(), template(b)))
        .collect();
    out.sort();
    Ok(out)
}

fn on_placeholder(r: Option<&String>) -> bool {
    r.is_some_and(|r| r.starts_with(PLACEHOLDER))
}

// ------------------------------------------------------------------------------------------------ the AI template

/// `decide(lv)`'s answer.
#[derive(Debug, Clone)]
pub struct Decision {
    /// `pack+swap`, `swap` or `skip: ...`.
    pub path: String,
    /// The level's load set.
    pub loads: Vec<String>,
    /// The AI spawners' templates (partition, or `none`) and how many use each, in the order first met.
    pub counts: Vec<(String, usize)>,
    /// (bundle, level entry) of each bundle holding the level partition.
    pub found: Vec<(String, String)>,
}

impl Decision {
    /// `str(counts)`: the dict's repr.
    #[must_use]
    pub fn counts_repr(&self) -> String {
        let items: Vec<String> = self
            .counts
            .iter()
            .map(|(k, v)| format!("{}: {v}", bc2_pyrt::fmt::str_repr(k)))
            .collect();
        format!("{{{}}}", items.join(", "))
    }
}

/// `level_xml(bundle, entry)`: the entry's XML, read from the bundle in memory.
fn level_xml(bundle: &str, entry: &str) -> R<String> {
    let h = bc2_gamefs::bundles::entries_path(bundle).map_err(|e| OnsError::Data(format!("{bundle}: {e}")))?;
    // fbrb.names: {lower name: entry} of the live entries, a later one of a name winning
    let real = h
        .entries
        .iter()
        .rev()
        .find(|e| !e.is_deleted() && e.name.to_lowercase() == entry.to_lowercase())
        .ok_or_else(|| OnsError::Data(format!("KeyError: '{}'", entry.to_lowercase())))?
        .name
        .clone();
    let data = bc2_gamefs::bundles::read_path(bundle, &[real.as_str()])
        .map_err(|e| OnsError::Data(format!("{bundle}: {e}")))?
        .pop()
        .unwrap_or_default();
    bc2_formats::dbx::to_xml(&data).map_err(|e| OnsError::Data(format!("{real}: {e}")))
}

/// `decide(lv)`.
pub fn decide(game: &str, lv: &str) -> R<Decision> {
    let loads = load_set(game, lv)?;
    let mut found = Vec::new();
    for b in bundles(game, lv) {
        if let Some(e) = level_entry(&fbrb(&["list", &b])?, lv)? {
            found.push((b, e));
        }
    }
    let Some((lb, le)) = found.last() else {
        return Err(OnsError::Exit(format!(
            "{lv}: no bundle holds levels/{}.dbx",
            level_name(lv)?
        )));
    };
    let mut counts: Vec<(String, usize)> = Vec::new();
    for (_, r) in ai_spawners(&level_xml(lb, le)?)? {
        let k = GUID_TAIL
            .replace(r.as_deref().filter(|s| !s.is_empty()).unwrap_or("none"), "")
            .into_owned();
        match counts.iter_mut().find(|(c, _)| *c == k) {
            Some((_, n)) => *n += 1,
            None => counts.push((k, 1)),
        }
    }
    let n: usize = counts
        .iter()
        .filter(|(k, _)| format!("{k}/") == PLACEHOLDER)
        .map(|(_, v)| v)
        .sum();
    let path = if n == 0 {
        "skip: no AI spawner uses DefaultSoldier".to_owned()
    } else if loads.iter().any(|l| l == "game:levels/sp_common/level-00.fbrb") {
        "swap".to_owned()
    } else {
        "pack+swap".to_owned()
    };
    Ok(Decision {
        path,
        loads,
        counts,
        found,
    })
}

/// `--apply`'s and `--plan`'s first line of a level.
#[must_use]
pub fn decision_line(lv: &str, d: &Decision) -> String {
    format!(
        "{lv}: {}; loads {}; AI spawner templates {}",
        d.path,
        d.loads.join(", "),
        d.counts_repr()
    )
}

/// A packed partition: (its path in the bundle, the file, its manifest line).
pub type Part = (String, String, String);
/// `sources()`: the grunt template chain's partitions, the donor level's file and the donor spawner's GUID.
pub type Sources = (Vec<Part>, String, String);

/// `unpacked(bundle, name)`: a cached unpack under `work\combat_a`, made again when the bundle changed (its stamp).
fn unpacked(root: &str, bundle: &str, name: &str) -> R<String> {
    let d = joins(root, &["work", "combat_a", name]);
    let st = super::stamp_path(&d);
    let stamp = super::stamp_of(&[bundle])?;
    let fresh = super::exists(&d)
        && std::path::Path::new(&st).is_file()
        && read_text_utf8(&st).ok().as_deref() == Some(stamp.as_str())
        && std::path::Path::new(&join(&d, "fbrb.manifest")).is_file();
    if fresh {
        return Ok(d);
    }
    let _ = std::fs::remove_dir_all(&d);
    fbrb(&["unpack", bundle, &d])?;
    write(&st, stamp.as_bytes())?;
    Ok(d)
}

/// `sources()`.
pub fn sources(root: &str, game: &str) -> R<Sources> {
    let sp = unpacked(
        root,
        &joins(game, &["Dist", "win32", "levels", "sp_common", "level-00.fbrb"]),
        "sp_common_src",
    )?;
    let lines = super::port::manifest_lines(&join(&sp, "fbrb.manifest"))?;
    let mut parts: Vec<Part> = Vec::new();
    for p in PARTS {
        let line = lines.iter().find(|x| x.starts_with(&format!("{p}|")));
        let f = join(&sp, &p.replace('/', "\\"));
        let Some(line) = line.filter(|_| std::path::Path::new(&f).is_file()) else {
            return Err(OnsError::Exit(format!("{p} is not in sp_common")));
        };
        parts.push((p.to_owned(), f, line.clone()));
    }
    let sense = xml_of(&parts[1].1)?;
    let bhv = xml_of(&parts[2].1)?;
    let tpl = xml_of(&parts[0].1)?;
    let target_guid = TARGET.rsplit('/').next().unwrap_or("").to_uppercase();
    if !tpl.contains(&target_guid)
        || !tpl.contains("objects/soldiers/templates/ai/enemy_grunt_sense/39cfde85-7de4-4960-a4e0-338875585542")
        || !tpl.contains("objects/soldiers/templates/ai/enemy_grunt_bhv/fe4a0390-6f39-420a-975b-f1587a2245c4")
        || !sense.contains("<field name=\"CombatFOVMultiplier\">6</field>")
        || !bhv.contains("<field name=\"MaxAttackBursts\">3</field>")
    {
        return Err(OnsError::Exit(
            "sp_common template chain differs from the recorded one".into(),
        ));
    }
    let donor_dir = unpacked(
        root,
        &joins(game, &["Dist", "win32", "levels", "sp_004_B", "level-00.fbrb"]),
        "sp_004_b_src",
    )?;
    let donor_dbx = joins(&donor_dir, &["levels", "SP_004_B.dbx"]);
    let donor = ai_spawners(&xml_of(&donor_dbx)?)?
        .into_iter()
        .filter(|(_, r)| r.as_deref() == Some(TARGET))
        .map(|(g, _)| g)
        .min()
        .ok_or_else(|| OnsError::Exit("no sp_004_b spawner uses the grunt template".into()))?;
    Ok((parts, donor_dbx, donor))
}

/// `dbx_check(f)`'s ERROR lines, sorted (`errs`): DbxEdit's check run, exit 0 or 1 with its count line.
fn check_errors(f: &str) -> R<Vec<String>> {
    let o = bc2_tools::dbxedit::run(&["check".to_owned(), f.to_owned()], None);
    if !(o.code == 0 || o.code == 1) || !o.stdout.contains("error(s)") {
        return Err(OnsError::Exit(format!(
            "DbxEdit check failed to run on {f}:\n{}\n{}",
            o.stdout, o.stderr
        )));
    }
    let mut e: Vec<String> = bc2_pyrt::py::splitlines(&o.stdout)
        .into_iter()
        .filter(|l| l.starts_with("ERROR"))
        .map(|l| bc2_pyrt::py::strip(l).to_owned())
        .collect();
    e.sort();
    Ok(e)
}

/// `check(before, after, sp)`: every former DefaultSoldier spawner holds the target, none is left on it, and the
/// only lines that changed are those Template lines.
fn check_swap(before: &str, after: &str, sp: &[(String, Option<String>)]) -> R<()> {
    let now = ai_spawners(after)?;
    let get = |g: &str| now.iter().find(|(x, _)| x == g).and_then(|(_, r)| r.as_deref());
    if sp.iter().any(|(g, _)| get(g) != Some(TARGET)) || now.iter().any(|(_, r)| on_placeholder(r.as_ref())) {
        return Err(OnsError::Exit(
            "check: a DefaultSoldier AI spawner does not hold the target Template".into(),
        ));
    }
    let (a, b) = (bc2_pyrt::py::splitlines(before), bc2_pyrt::py::splitlines(after));
    let changed: Vec<usize> = (0..a.len().min(b.len())).filter(|&i| a[i] != b[i]).collect();
    if a.len() != b.len()
        || changed.len() != sp.len()
        || changed
            .iter()
            .any(|&i| !b[i].contains("name=\"Template\"") || !b[i].contains(TARGET))
    {
        return Err(OnsError::Exit(format!(
            "check: {} lines changed, expected {} Template lines",
            changed.len(),
            sp.len()
        )));
    }
    Ok(())
}

/// `edit(src, out, donor_dbx, donor)`: (the spawners swapped, the check's words).
fn edit(root: &str, src: &str, out: &str, donor_dbx: &str, donor: &str) -> R<(usize, String)> {
    let before = xml_of(src)?;
    let sp: Vec<(String, Option<String>)> = ai_spawners(&before)?
        .into_iter()
        .filter(|(_, r)| on_placeholder(r.as_ref()))
        .collect();
    let cmds: Vec<String> = sp
        .iter()
        .map(|(g, _)| format!("copyfield={donor_dbx}|{donor}|Template|{g}"))
        .collect();
    let cmd_file = joins(root, &["work", "combat_a", "commands.txt"]);
    write_text_utf8(&cmd_file, &(cmds.join("\n") + "\n"))?;
    let r = dbx(&["edit", src, out, &format!("@{cmd_file}")])?;
    if r.contains("NOT FOUND") {
        return Err(OnsError::Exit(format!("edit: {r}")));
    }
    check_swap(&before, &xml_of(out)?, &sp)?;
    let (e0, e1) = (check_errors(src)?, check_errors(out)?);
    if e1 != e0 {
        let mut new: Vec<&String> = e1.iter().filter(|x| !e0.contains(x)).collect();
        new.sort();
        new.dedup();
        let new: Vec<&str> = new.iter().map(|s| s.as_str()).collect();
        return Err(OnsError::Exit(format!("DbxEdit check: new errors\n{}", new.join("\n"))));
    }
    let chk = if e1.is_empty() {
        "DbxEdit check: 0 errors".to_owned()
    } else {
        format!("DbxEdit check: {} error(s), as the original", e1.len())
    };
    Ok((sp.len(), chk))
}

/// `pack_parts(d, parts)`: the template chain copied into the unpack, appended at its manifest's tail.
fn pack_parts(d: &str, parts: &[Part]) -> R<()> {
    let m = join(d, "fbrb.manifest");
    let mut text = read_text_utf8(&m)?;
    let names: Vec<String> = bc2_pyrt::py::splitlines(&text)
        .iter()
        .map(|x| x.split('|').next().unwrap_or("").to_lowercase())
        .collect();
    for (p, f, line) in parts {
        if names.contains(&p.to_lowercase()) {
            return Err(OnsError::Exit(format!("{p} is already in the level bundle")));
        }
        copy(f, &join(d, &p.replace('/', "\\")))?;
        text = format!("{}\n{line}\n", text.trim_end_matches(['\r', '\n']));
    }
    write(&m, text.as_bytes())
}

/// What a bundle's edit came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Swapped {
    /// No DefaultSoldier spawner left (and the chain there where it is packed): left as it is.
    Already,
    /// The bundle built: (the file to install, the spawners swapped, the check's words).
    Packed(String, usize, String),
}

/// `apply`'s bundle step after the ledger's check: the bundle unpacked into `work\combat_a\<lv>\b<k>`, the swap, the
/// chain packed (`pack+swap`), the bundle packed and checked.
pub fn edit_bundle(root: &str, lv: &str, k: usize, bundle: &str, entry: &str, path: &str, s: &Sources) -> R<Swapped> {
    let (parts, donor_dbx, donor) = s;
    let d = joins(root, &["work", "combat_a", lv, &format!("b{k}")]);
    let _ = std::fs::remove_dir_all(&d);
    fbrb(&["unpack", bundle, &d])?;
    let src = join(&d, &entry.replace('/', "\\"));
    let listing = read_text_utf8(&join(&d, "fbrb.manifest"))?.to_lowercase();
    let have: Vec<bool> = PARTS
        .iter()
        .map(|p| listing.contains(&format!("{}|", p.to_lowercase())))
        .collect();
    let remaining = ai_spawners(&xml_of(&src)?)?
        .iter()
        .filter(|(_, r)| on_placeholder(r.as_ref()))
        .count();
    if remaining == 0 && (path == "swap" || have.iter().all(|h| *h)) {
        return Ok(Swapped::Already);
    }
    if (have.iter().any(|h| *h) && !have.iter().all(|h| *h)) || (path == "swap" && have.iter().any(|h| *h)) {
        return Err(OnsError::Exit(format!(
            "{}: the template partitions are partly present - inconsistent state, stopping",
            rel(bundle, root)
        )));
    }
    let edited = joins(
        root,
        &["work", "combat_a", lv, &format!("{}_b{k}.dbx", level_name(lv)?)],
    );
    let (n, chk) = edit(root, &src, &edited, donor_dbx, donor)?;
    copy(&edited, &src)?;
    if path == "pack+swap" {
        pack_parts(&d, parts)?;
    }
    let packed = joins(root, &["work", "combat_a", lv, &format!("level-00_b{k}.fbrb")]);
    pack(&d, &packed)?;
    if path == "pack+swap" {
        let l = fbrb(&["list", &packed])?;
        if parts.iter().any(|(p, _, _)| !l.contains(p.as_str())) {
            return Err(OnsError::Exit("packed bundle lacks a template partition".into()));
        }
    }
    Ok(Swapped::Packed(packed, n, chk))
}

/// `rmtree` of a folder or file that may be absent (the scratch the wrappers name).
pub fn drop_path(p: &str) -> R<()> {
    if std::path::Path::new(p).is_file() {
        std::fs::remove_file(p).map_err(|e| super::io_err(p, &e))
    } else {
        rmtree(p)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn templates_spawners_and_counts() {
        let x = r#"<instance guid="B" type="Entity.SoldierSpawnEntityData"><field name="Template" ref="objects/soldiers/templates/defaultsoldier/AAAA" />SSAiPlayer</instance>
<instance guid="A" type="Entity.SoldierSpawnEntityData"><field ref="x/y" name="Template" />SSAiPlayer</instance>
<instance guid="C" type="Entity.SoldierSpawnEntityData">SSHumanPlayer</instance>"#;
        let s = ai_spawners(x).unwrap();
        assert_eq!(
            s,
            vec![
                ("A".to_owned(), Some("x/y".to_owned())),
                (
                    "B".to_owned(),
                    Some("objects/soldiers/templates/defaultsoldier/AAAA".to_owned())
                )
            ]
        );
        let d = Decision {
            path: "swap".into(),
            loads: vec![],
            counts: vec![("objects/a".into(), 2), ("none".into(), 1)],
            found: vec![],
        };
        assert_eq!(d.counts_repr(), "{'objects/a': 2, 'none': 1}");
        assert_eq!(rel(r"C:\o\game\Dist\x.fbrb", r"C:\o"), r"game\Dist\x.fbrb");
        assert_eq!(pct(0.825), "82%");
        assert_eq!(pct(0.835), "84%");
        assert_eq!(find_bytes(b"abcdef", b"cd"), Some(2));
    }
}
