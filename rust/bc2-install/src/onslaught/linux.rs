//! Stage 2 of the console Onslaught port (round OR9): the port of Project OPFOR's `console_linux_base.py`'s `build` -
//! the console Onslaught gameplay (stage 1's level, [`super::build`]) put on the PC map's Linux server level, whose
//! world carries the AI physics the PC client's lacks.
//!
//! 1. the Linux `Levels/<PC map>.dbx` renamed to `Levels/<ONS>` (its partitions' prefix, except `ui/` and
//!    `materials_*`), with the console `LevelData`'s AI fields;
//! 2. the Linux level's own files copied into stage 1's level bundle under the new name (new partition GUIDs:
//!    `uuid5('opfor-linux:<new name>')`);
//! 3. the merge: Linux instances that are not world (nor needed by it) dropped, stage 1's non-world instances grafted,
//!    the `LevelData`'s gameplay fields copied; the `Objects` list's world items pointed at the Linux world of the same
//!    type, Linux world entities the list lacks added (the terrain before the first item);
//! 4. cull slots (`ReferenceObjectData.Enumeration`) as the Linux level's: the same object, else the same blueprint at
//!    the same position (to 0.1 m), else the nearest unused same-blueprint object within 50 m, else 0;
//! 5. DbxEdit's check, then packed (`<work>\onslaught\build_<ons>\level-00.linux.fbrb`) for the ledger's install.
//!
//! The work folder's files are the Python's byte for byte (rust/BENCHMARKS.md "OR9"). The ledger's parts (the bundle
//! this stage builds from, the install) stay in OPFOR's `console_linux_base.py`.

use std::collections::{HashMap, HashSet};

use bc2_formats::dbxedit::{Node, parse_xml};

use super::{OnsError, R, Source, dbx, fbrb, join, joins, pack, read, rmtree, size, stem4, write};
use crate::onslaught::port::{StageOut, find, read_text_utf8, write_text_utf8};

/// The instance types that make the world (kept from the Linux level).
const WORLD: [&str; 11] = [
    "LevelData",
    "TerrainEntityData",
    "ForestEntityData",
    "TreeMemberData",
    "MiddlegrowthModelGroupEntityData",
    "MiddlegrowthModelGroupMemberData",
    "RigidModelGroupEntityData",
    "RigidModelGroupMemberData",
    "StaticModelGroupEntityData",
    "StaticModelGroupMemberData",
    "StaticEnvmapEntityData",
];
/// The world's entities (the `Objects` list's world items), in the order missing ones are added.
const WORLD_ENTITIES: [&str; 6] = [
    "TerrainEntityData",
    "ForestEntityData",
    "MiddlegrowthModelGroupEntityData",
    "RigidModelGroupEntityData",
    "StaticModelGroupEntityData",
    "StaticEnvmapEntityData",
];
/// The console `LevelData`'s AI fields copied onto the Linux base.
const AI_FIELDS: [&str; 5] = [
    "HasAI",
    "HasFlyingAI",
    "PathfindingTypes",
    "PathObjectCount",
    "MaxLocalPathfindingObjects",
];
/// The console `LevelData`'s gameplay fields copied in the merge.
const LEVEL_FIELDS: [&str; 15] = [
    "Objects",
    "EventConnections",
    "VoiceOvers",
    "VoiceOverLines",
    "DestructibleLocalPathfindingParts",
    "ExplosiveObjects",
    "GameMode",
    "AttackerTeam",
    "FactionTeam1",
    "FactionTeam2",
    "LevelRoundTimeStatName",
    "MaxVehicleHeight",
    "HardcorePlayerHealthModifier",
    "MinObjectBoundingRadius",
    "RichPresence",
];

/// An element's attribute (`Element.get`): the first of that name.
fn attr<'a>(n: &'a Node, name: &str) -> Option<&'a str> {
    n.attributes.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

fn kids(n: &Node) -> &[Node] {
    n.children.as_deref().unwrap_or(&[])
}

/// `Element.iter()`: the element and every element under it, in document order.
fn walk<'a>(n: &'a Node, out: &mut Vec<&'a Node>) {
    out.push(n);
    for k in kids(n) {
        walk(k, out);
    }
}

/// `e.find("<tag>[@name='<name>']")`: the first child of that tag and name.
fn child<'a>(n: &'a Node, tag: &str, name: &str) -> Option<&'a Node> {
    kids(n).iter().find(|k| k.tag == tag && attr(k, "name") == Some(name))
}

/// A level's XML tree: its instances by GUID (upper case; a dict's order) and their short types.
struct Tree {
    root: Node,
}

impl Tree {
    /// `(guid upper, short type, instance)` of every `instance` element, a dict's way (a GUID again: its first
    /// place, the last element).
    fn instances(&self) -> Vec<(String, String, &Node)> {
        let mut all = Vec::new();
        walk(&self.root, &mut all);
        let mut out: Vec<(String, String, &Node)> = Vec::new();
        let mut at: HashMap<String, usize> = HashMap::new();
        for i in all.into_iter().filter(|n| n.tag == "instance") {
            let g = attr(i, "guid").unwrap_or_default().to_uppercase();
            let t = attr(i, "type")
                .unwrap_or_default()
                .rsplit('.')
                .next()
                .unwrap_or_default()
                .to_owned();
            if let Some(&k) = at.get(&g) {
                if let Some(e) = out.get_mut(k) {
                    e.1 = t;
                    e.2 = i;
                }
            } else {
                at.insert(g.clone(), out.len());
                out.push((g, t, i));
            }
        }
        out
    }
}

/// `tree(dbx)`: DbxEdit's XML of a .dbx (written beside it) parsed.
fn tree(dbx_path: &str) -> R<Tree> {
    dbx(&["xml", dbx_path])?;
    let x = format!("{}.xml", stem4(dbx_path));
    let b = read(&x)?;
    let text = String::from_utf8(b).map_err(|e| OnsError::Data(format!("{x}: {e}")))?;
    let root = parse_xml(&text, &x).map_err(|e| OnsError::Data(format!("{x}: {e}")))?;
    Ok(Tree { root })
}

/// `level_data(types)`: the one `LevelData`.
fn level_data(inst: &[(String, String, &Node)]) -> R<String> {
    let g: Vec<&String> = inst
        .iter()
        .filter(|(_, t, _)| t == "LevelData")
        .map(|(g, _, _)| g)
        .collect();
    match g.as_slice() {
        [one] => Ok((*one).clone()),
        _ => Err(OnsError::Exit(format!("expected one LevelData, found {}", g.len()))),
    }
}

/// `objects(inst, lg)`: the `LevelData`'s `Objects` items' references, upper case ('' for a null item).
fn objects(ld: &Node) -> Vec<String> {
    child(ld, "array", "Objects").map_or_else(Vec::new, |a| {
        kids(a)
            .iter()
            .map(|it| attr(it, "ref").unwrap_or_default().to_uppercase())
            .collect()
    })
}

/// `refs(e)`: the instance GUIDs every reference under an element names (upper case; a partition path's last part).
fn refs(e: &Node) -> Vec<String> {
    let mut all = Vec::new();
    walk(e, &mut all);
    all.into_iter()
        .filter_map(|x| attr(x, "ref").filter(|r| !r.is_empty()))
        .map(|r| r.to_uppercase().rsplit('/').next().unwrap_or_default().to_owned())
        .collect()
}

/// A placed object's cull slot, blueprint (the reference's folder name) and position.
type Slot = (i64, String, [f64; 3]);

/// Python's `int(text)` of a field's text (surrounding white space allowed).
fn py_int(t: &str) -> R<i64> {
    bc2_pyrt::py::strip(t)
        .parse::<i64>()
        .map_err(|_| OnsError::Data(format!("invalid literal for int() with base 10: {t:?}")))
}

/// Python's `float(text)`.
fn py_float(t: &str) -> R<f64> {
    bc2_pyrt::py::strip(t)
        .parse::<f64>()
        .map_err(|_| OnsError::Data(format!("could not convert string to float: {t:?}")))
}

/// `slots(root)`: every `Entity.ReferenceObjectData`'s slot, blueprint and position, by GUID (as written; a dict).
fn slots(t: &Tree) -> R<Vec<(String, Slot)>> {
    let mut all = Vec::new();
    walk(&t.root, &mut all);
    let mut out: Vec<(String, Slot)> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    for i in all.into_iter().filter(|n| n.tag == "instance") {
        if attr(i, "type") != Some("Entity.ReferenceObjectData") {
            continue;
        }
        let g = attr(i, "guid").unwrap_or_default().to_owned();
        let e = child(i, "field", "Enumeration").ok_or_else(|| OnsError::Data(format!("{g}: no Enumeration field")))?;
        let tr = child(i, "complex", "Transform").ok_or_else(|| OnsError::Data(format!("{g}: no Transform")))?;
        let r = child(i, "field", "ReferencedObject")
            .ok_or_else(|| OnsError::Data(format!("{g}: no ReferencedObject field")))?;
        let r = attr(r, "ref").unwrap_or_default().to_lowercase();
        let en = if e.text.is_empty() { 0 } else { py_int(&e.text)? };
        let bp = if r.contains('/') {
            let parts: Vec<&str> = r.split('/').collect();
            parts.get(parts.len() - 2).copied().unwrap_or_default().to_owned()
        } else {
            String::new()
        };
        let comps: Vec<&str> = tr.text.split('/').collect();
        let mut p = Vec::new();
        for c in comps.iter().skip(12).take(3) {
            p.push(py_float(c)?);
        }
        let pos = [
            p.first().copied().unwrap_or(f64::NAN),
            p.get(1).copied().unwrap_or(f64::NAN),
            p.get(2).copied().unwrap_or(f64::NAN),
        ];
        if p.len() != 3 {
            // a tuple of fewer than three: math.dist raises on it; no DbxEdit transform is that short
            return Err(OnsError::Data(format!("{g}: a transform without a position")));
        }
        let v = (en, bp, pos);
        if let Some(&k) = at.get(&g) {
            if let Some(x) = out.get_mut(k) {
                x.1 = v;
            }
        } else {
            at.insert(g.clone(), out.len());
            out.push((g, v));
        }
    }
    Ok(out)
}

/// A position rounded to 0.1 m as a dict key (Python's float equality: -0.0 is 0.0, a NaN never equal).
fn pos_key(bp: &str, p: [f64; 3]) -> Option<(String, [u64; 3])> {
    let mut k = [0u64; 3];
    for (o, c) in k.iter_mut().zip(p) {
        let r = bc2_pyrt::fmt::round_to(c, 1);
        if r.is_nan() {
            return None;
        }
        *o = (r + 0.0).to_bits();
    }
    Some((bp.to_owned(), k))
}

/// `enum_cmds(merged, linux)`: the cull slots as the Linux level's (rule 4); the commands and the objects left at 0
/// per blueprint (in first-seen order).
/// The commands and the count of objects left at 0 per blueprint.
type EnumCmds = (Vec<String>, Vec<(String, usize)>);

fn enum_cmds(merged: &Tree, linux: &Tree) -> R<EnumCmds> {
    let m = slots(merged)?;
    let lin = slots(linux)?;
    let mut lg: HashMap<String, &Slot> = HashMap::new();
    for (g, v) in &lin {
        lg.insert(g.to_lowercase(), v);
    }
    let mut byk: HashMap<(String, [u64; 3]), i64> = HashMap::new();
    for (_, (e, bp, p)) in &lin {
        if let Some(k) = pos_key(bp, *p) {
            byk.insert(k, *e);
        }
    }
    let mut new: Vec<(String, i64)> = Vec::new();
    let mut rest: Vec<usize> = Vec::new();
    for (ix, (g, (_e, bp, p))) in m.iter().enumerate() {
        if let Some(l) = lg.get(&g.to_lowercase()) {
            new.push((g.clone(), l.0));
        } else if let Some(&e) = pos_key(bp, *p).and_then(|k| byk.get(&k)) {
            new.push((g.clone(), e));
        } else {
            rest.push(ix);
        }
    }
    let mut used: HashSet<i64> = new.iter().map(|(_, e)| *e).collect();
    let mut alone: Vec<(String, usize)> = Vec::new();
    for ix in rest {
        let Some((g, (_e, bp, p))) = m.get(ix) else { continue };
        let mut c: Vec<(f64, i64)> = lin
            .iter()
            .filter(|(_, (le, lbp, _))| lbp == bp && *le != 0 && !used.contains(le))
            .map(|(_, (le, _, q))| (bc2_pyrt::math::dist3(*p, *q), *le))
            .collect();
        // sorted((distance, slot)): a tuple's order (a NaN distance compares as Python's: never less)
        c.sort_by(|a, b| py_cmp(a.0, b.0).then(a.1.cmp(&b.1)));
        match c.first() {
            Some(&(d, le)) if d < 50.0 => {
                new.push((g.clone(), le));
                used.insert(le);
            }
            _ => {
                new.push((g.clone(), 0));
                match alone.iter_mut().find(|(b, _)| b == bp) {
                    Some(x) => x.1 += 1,
                    None => alone.push((bp.clone(), 1)),
                }
            }
        }
    }
    let first: HashMap<&str, i64> = m.iter().map(|(g, s)| (g.as_str(), s.0)).collect();
    let cmds = new
        .iter()
        .filter(|(g, ne)| first.get(g.as_str()) != Some(ne))
        .map(|(g, ne)| format!("setfield={g}|Enumeration|{ne}"))
        .collect();
    Ok((cmds, alone))
}

/// Two floats as a sort of Python tuples compares them (no NaN reaches here: positions are finite).
fn py_cmp(a: f64, b: f64) -> std::cmp::Ordering {
    a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
}

/// `Counter.most_common(n)`: by count, highest first, ties in first-seen order.
fn most_common(c: &[(String, usize)], n: usize) -> Vec<(String, usize)> {
    let mut v = c.to_vec();
    v.sort_by_key(|x| std::cmp::Reverse(x.1));
    v.truncate(n);
    v
}

/// What stage 2 is given.
#[derive(Debug, Clone)]
pub struct Args {
    /// The OPFOR folder (`console_common.OPFOR`): its `work\` is written.
    pub root: String,
    /// The game copy (`console_common.GAME`): read only.
    pub game: String,
    /// The Onslaught level (lower case).
    pub lv: String,
    /// The PC map (lower case).
    pub pc: String,
    /// The Linux server level's `level-00.fbrb`.
    pub linux_bundle: String,
    /// The bundle this stage builds on (the ledger's `source`: stage 1's level).
    pub src_bundle: String,
    /// `--apply` (the bundle packed for the install) rather than `--plan`.
    pub write: bool,
}

/// `console_linux_base.build(lv, pc, linux_bundle, write)` after the ledger's source: the work folder's files, the
/// lines printed, the bundle to install.
#[expect(clippy::too_many_lines, reason = "console_linux_base.build, step by step")]
pub fn build(a: &Args) -> R<StageOut> {
    let (lv, pc) = (a.lv.as_str(), a.pc.as_str());
    let (ons_up, pc_up) = (lv.to_uppercase(), pc.to_uppercase());
    let b = joins(&a.root, &["work", "onslaught", &format!("build_{lv}")]);
    let first = joins(&b, &["out", "Levels", &format!("{ons_up}.dbx")]);
    if !std::path::Path::new(&first).is_file() {
        return Err(OnsError::Exit(format!(
            "{lv}: no first-stage level ({first}) - run console_onslaught.py first"
        )));
    }
    let bundle = joins(&a.game, &["Package", "levels", lv, "level-00.fbrb"]);
    let ctl = join(&b, "control");
    std::fs::create_dir_all(&ctl).map_err(|e| super::io_err(&ctl, &e))?;
    // 1. the Linux base (unpacked again when the Linux bundle changed)
    let lx = joins(&a.root, &["work", "onslaught", &format!("linux_{pc}"), "level"]);
    super::port::unpack_cached(&a.linux_bundle, &lx)?;
    let linux_level = find(&lx, &format!("Levels/{pc_up}.dbx"))?;
    let ltree = tree(&linux_level)?;
    let linst = ltree.instances();
    let lg = level_data(&linst)?;
    let ftree = tree(&first)?;
    let first_inst = ftree.instances();
    let fg = level_data(&first_inst)?;
    let prefix = format!("replaceprefix=levels/{pc}/|Levels/{ons_up}/|levels/{pc}/ui/|levels/{pc}/materials_");
    let base = join(&ctl, &format!("{ons_up}_linux.dbx"));
    let mut cmd: Vec<String> = vec!["edit".into(), linux_level.clone(), base.clone(), prefix.clone()];
    cmd.extend(AI_FIELDS.iter().map(|f| format!("copyfield={first}|{fg}|{f}|{lg}")));
    dbx(&cmd.iter().map(String::as_str).collect::<Vec<_>>())?;
    let btree = tree(&base)?;
    let binst = btree.instances();
    // 2. the Linux level's files into stage 1's bundle
    let ov = join(&b, "ctl_ov");
    rmtree(&ov)?;
    fbrb(&["unpack", &a.src_bundle, &ov])?;
    let mut man: Vec<String> = bc2_pyrt::py::splitlines(&read_text_utf8(&join(&ov, "fbrb.manifest"))?)
        .into_iter()
        .map(str::to_owned)
        .collect();
    let mut idx: HashMap<String, usize> = HashMap::new();
    for (i, ln) in man.iter().enumerate() {
        idx.insert(ln.split('|').next().unwrap_or_default().to_lowercase(), i);
    }
    let put = |man: &mut Vec<String>, idx: &mut HashMap<String, usize>, name: &str, line: String| {
        if let Some(&i) = idx.get(&name.to_lowercase()) {
            if let Some(x) = man.get_mut(i) {
                *x = line;
            }
        } else {
            man.push(line);
            idx.insert(name.to_lowercase(), man.len() - 1);
        }
    };
    let mut n = 0usize;
    let lman = read_text_utf8(&join(&lx, "fbrb.manifest"))?;
    let pre = format!("levels/{pc}/");
    for ln in bc2_pyrt::py::splitlines(&lman).into_iter().skip(1) {
        let name = ln.split('|').next().unwrap_or_default();
        let low = name.to_lowercase();
        if ln.contains("|*deleted*|") || !low.starts_with(&pre) {
            continue;
        }
        let src = joins(&lx, &name.split('/').collect::<Vec<_>>());
        if low.starts_with(&format!("levels/{pc}/materials_")) {
            let dst = joins(&ov, &name.split('/').collect::<Vec<_>>());
            write(&dst, &read(&src)?)?;
            put(&mut man, &mut idx, name, ln.to_owned());
            n += 1;
            continue;
        }
        // name[len(pre):] counts characters (the lower-case prefix has as many as the name's)
        let rest: String = name.chars().skip(pre.chars().count()).collect();
        let new = format!("Levels/{ons_up}/{rest}");
        let dst = joins(&ov, &new.split('/').collect::<Vec<_>>());
        if let Some(d) = std::path::Path::new(&dst).parent() {
            std::fs::create_dir_all(d).map_err(|e| super::io_err(&d.to_string_lossy(), &e))?;
        }
        let line = if low.ends_with(".dbx") {
            let g = bc2_formats::guid::new_guid("opfor-linux", &[&new.to_lowercase()]);
            dbx(&["edit", &src, &dst, &prefix, &format!("partguid={g}")])?;
            let after = ln
                .split_once('|')
                .map(|x| x.1)
                .ok_or_else(|| OnsError::Data("list index out of range".into()))?;
            let mid = after.rsplit_once('|').map_or(after, |x| x.0);
            format!("{new}|{mid}|{}", size(&dst)?)
        } else {
            write(&dst, &read(&src)?)?;
            let tail: String = ln.chars().skip(name.chars().count()).collect();
            format!("{new}{tail}")
        };
        put(&mut man, &mut idx, &new, line);
        n += 1;
    }
    // 3. the merge
    let bnodes: HashMap<&str, &Node> = binst.iter().map(|(g, _, i)| (g.as_str(), *i)).collect();
    let mut keep: HashSet<String> = binst
        .iter()
        .filter(|(_, t, _)| WORLD.contains(&t.as_str()))
        .map(|(g, _, _)| g.clone())
        .collect();
    let mut stack: Vec<String> = binst
        .iter()
        .filter(|(g, t, _)| keep.contains(g) && t != "LevelData")
        .map(|(g, _, _)| g.clone())
        .collect();
    while let Some(g) = stack.pop() {
        if let Some(node) = bnodes.get(g.as_str()) {
            for r in refs(node) {
                if bnodes.contains_key(r.as_str()) && !keep.contains(&r) {
                    keep.insert(r.clone());
                    stack.push(r);
                }
            }
        }
    }
    let mut cmds = vec![format!(
        "# console Onslaught gameplay ({ons_up}, first stage) on the Linux server level of {pc_up}"
    )];
    cmds.extend(
        binst
            .iter()
            .filter(|(g, _, _)| !keep.contains(g))
            .map(|(g, _, _)| format!("drop={g}")),
    );
    let ftypes: HashMap<&str, &str> = first_inst.iter().map(|(g, t, _)| (g.as_str(), t.as_str())).collect();
    let grafted: HashSet<&str> = first_inst
        .iter()
        .filter(|(g, t, _)| !WORLD.contains(&t.as_str()) && !keep.contains(g))
        .map(|(g, _, _)| g.as_str())
        .collect();
    cmds.extend(
        first_inst
            .iter()
            .filter(|(g, _, _)| grafted.contains(g.as_str()))
            .map(|(g, _, _)| format!("graft={first}|{g}")),
    );
    cmds.extend(LEVEL_FIELDS.iter().map(|f| format!("copyfield={first}|{fg}|{f}|{lg}")));
    let mut lworld: HashMap<&str, Vec<&str>> = HashMap::new();
    for (g, t, _) in &binst {
        if WORLD_ENTITIES.contains(&t.as_str()) {
            lworld.entry(t.as_str()).or_default().push(g.as_str());
        }
    }
    let fld = first_inst
        .iter()
        .find(|(g, _, _)| *g == fg)
        .map(|x| x.2)
        .ok_or_else(|| OnsError::Data("no LevelData".into()))?;
    let fobjects = objects(fld);
    let mut present: HashSet<String> = HashSet::new();
    let mut notes: Vec<String> = Vec::new();
    for r in &fobjects {
        if grafted.contains(r.as_str()) || keep.contains(r) {
            present.insert(r.clone());
            continue;
        }
        let t = ftypes.get(r.as_str()).copied();
        let is_world = t.is_some_and(|t| WORLD_ENTITIES.contains(&t));
        let cand: Vec<&str> = t.and_then(|t| lworld.get(t)).map_or_else(Vec::new, |v| {
            v.iter().copied().filter(|g| !present.contains(*g)).collect()
        });
        if is_world && !cand.is_empty() {
            let c0 = cand.first().copied().unwrap_or_default();
            cmds.push(format!("replace={}|{}", r.to_lowercase(), c0.to_lowercase()));
            present.insert(c0.to_owned());
        } else if is_world && ftypes.contains_key(r.as_str()) {
            // the Linux level has no world object of that type (Valparaiso's has no static envmap): stage 1's own
            let k = cmds
                .iter()
                .position(|c| c.starts_with("copyfield="))
                .unwrap_or(cmds.len());
            cmds.insert(k, format!("graft={first}|{r}"));
            notes.push(format!(
                "Objects item {r} ({}): no Linux one - the first stage's grafted",
                t.unwrap_or_default()
            ));
            present.insert(r.clone());
        } else {
            return Err(OnsError::Exit(format!(
                "{lv}: Objects item {r} ({}) resolves nowhere - not handled",
                t.unwrap_or("None")
            )));
        }
    }
    let first_item = fobjects.iter().find(|r| present.contains(*r)).cloned();
    for t in WORLD_ENTITIES {
        for g in lworld.get(t).cloned().unwrap_or_default() {
            if !present.contains(g) {
                let early = t == "TerrainEntityData" && first_item.as_deref().is_some_and(|f| !f.is_empty());
                let mut c = format!("additem={lg}|Objects|{}", g.to_lowercase());
                if early && let Some(f) = &first_item {
                    c.push('|');
                    c.push_str(&f.to_lowercase());
                }
                cmds.push(c);
                present.insert(g.to_owned());
            }
        }
    }
    let merge_cmd = join(&b, "merge.cmd");
    write_text_utf8(&merge_cmd, &(cmds.join("\n") + "\n"))?;
    let merged = join(&ctl, &format!("{ons_up}_merged.dbx"));
    let log = dbx(&["edit", &base, &merged, &format!("@{merge_cmd}")])?;
    write_text_utf8(&join(&b, "merge.log"), &log)?;
    let bad: Vec<String> = bc2_pyrt::py::splitlines(&log)
        .into_iter()
        .filter(|l| l.contains("NOT FOUND") || l.to_lowercase().contains("not found"))
        .map(str::to_owned)
        .collect();
    // 4. the cull slots
    let mtree = tree(&merged)?;
    let ltree2 = tree(&linux_level)?;
    let (ecmds, alone) = enum_cmds(&mtree, &ltree2)?;
    let enum_cmd = join(&b, "enum.cmd");
    write_text_utf8(
        &enum_cmd,
        &format!(
            "# cull-tree slots (Enumeration) as the Linux level's (console_linux_base.py)\n{}\n",
            ecmds.join("\n")
        ),
    )?;
    let fin = join(&ctl, &format!("{ons_up}_merged_enum.dbx"));
    if ecmds.is_empty() {
        write(&fin, &read(&merged)?)?;
    } else {
        dbx(&["edit", &merged, &fin, &format!("@{enum_cmd}")])?;
    }
    write(&joins(&ov, &["Levels", &format!("{ons_up}.dbx")]), &read(&fin)?)?;
    let lvl = format!("Levels/{ons_up}.dbx");
    put(
        &mut man,
        &mut idx,
        &lvl,
        format!("{lvl}|<non-resource>|00010000|{}", size(&fin)?),
    );
    write(&join(&ov, "fbrb.manifest"), (man.join("\n") + "\n").as_bytes())?;
    let check = bc2_tools::dbxedit::run(&super::args(&["check", &fin, "--bundle", &ov]), None);
    let mut printed = vec![format!(
        "{lv}: Linux base {pc_up} ({} world instances kept, {} dropped), {} console gameplay instances grafted, {n} \
         Linux files; {} cull slots set, {} objects outside the cull tree",
        keep.len(),
        cmds.iter().filter(|c| c.starts_with("drop=")).count(),
        grafted.len(),
        ecmds.len(),
        alone.iter().map(|x| x.1).sum::<usize>()
    )];
    for x in notes.iter().chain(&bad) {
        printed.push(format!("  {x}"));
    }
    for (bp, k) in most_common(&alone, 8) {
        printed.push(format!("  slot 0 (no Linux counterpart): {bp} x{k}"));
    }
    let out = bc2_pyrt::py::strip(&check.stdout);
    let last = bc2_pyrt::py::splitlines(out).last().copied().unwrap_or("?");
    printed.push(format!("  check: {last}"));
    let mut res = StageOut {
        printed,
        ..StageOut::default()
    };
    if check.code != 0 {
        let tail: String = {
            let c: Vec<char> = check.stdout.chars().collect();
            c[c.len().saturating_sub(2000)..].iter().collect()
        };
        res.printed.push(tail);
        res.fail = Some(format!("{lv}: DbxEdit check failed - not installed"));
        return Ok(res);
    }
    if !a.write {
        return Ok(res);
    }
    let built = join(&b, "level-00.linux.fbrb");
    pack(&ov, &built)?;
    res.writes.push((bundle, Source::File(built)));
    Ok(res)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fmt::Write as _;

    fn level(objs: &[(&str, i64, &str, f64)]) -> Tree {
        let mut x = String::from("<partition guid=\"P\" primaryInstance=\"L\">\r\n");
        for (g, e, bp, px) in objs {
            let _ = write!(
                x,
                "\t<instance guid=\"{g}\" type=\"Entity.ReferenceObjectData\">\r\n\t\t<field name=\"Enumeration\">{e}</field>\r\n\t\t<field ref=\"objects/{bp}/0123\" name=\"ReferencedObject\" />\r\n\t\t<complex name=\"Transform\">1/0/0/0/0/1/0/0/0/0/1/0/{px}/0/0/1</complex>\r\n\t</instance>\r\n"
            );
        }
        x.push_str("</partition>\r\n");
        Tree {
            root: parse_xml(&x, "t").unwrap(),
        }
    }

    #[test]
    fn cull_slots() {
        // the same GUID; the same blueprint at the same place (to 0.1 m); the nearest unused within 50 m; else 0
        let lin = level(&[
            ("a", 5, "car", 0.0),
            ("x", 6, "car", 10.04),
            ("y", 7, "car", 30.0),
            ("z", 8, "car", 31.0),
        ]);
        let m = level(&[
            ("A", 1, "car", 99.0),
            ("b", 2, "car", 10.0),
            ("c", 3, "car", 29.0),
            ("d", 4, "car", 29.5),
            ("e", 9, "house", 0.0),
        ]);
        let (cmds, alone) = enum_cmds(&m, &lin).unwrap();
        assert_eq!(
            cmds,
            [
                "setfield=A|Enumeration|5",
                "setfield=b|Enumeration|6",
                "setfield=c|Enumeration|7",
                "setfield=d|Enumeration|8",
                "setfield=e|Enumeration|0"
            ]
        );
        assert_eq!(alone, [("house".to_owned(), 1)]);
        assert_eq!(
            most_common(&[("a".into(), 1), ("b".into(), 2), ("c".into(), 1)], 2),
            [("b".to_owned(), 2), ("a".to_owned(), 1)]
        );
    }

    #[test]
    fn instances_and_objects() {
        let x = "<partition guid=\"P\" primaryInstance=\"L\">\r\n\t<instance guid=\"l1\" type=\"GameSharedResources.LevelData\">\r\n\t\t<array name=\"Objects\">\r\n\t\t\t<item ref=\"t1\" />\r\n\t\t\t<item ref=\"null\" />\r\n\t\t</array>\r\n\t</instance>\r\n\t<instance guid=\"t1\" type=\"Entity.TerrainEntityData\">\r\n\t\t<field ref=\"levels/x/terrain/ab-12\" name=\"Terrain\" />\r\n\t</instance>\r\n</partition>\r\n";
        let t = Tree {
            root: parse_xml(x, "t").unwrap(),
        };
        let inst = t.instances();
        assert_eq!(level_data(&inst).unwrap(), "L1");
        assert_eq!(objects(inst[0].2), ["T1", "NULL"]);
        assert_eq!(refs(inst[1].2), ["AB-12"]);
    }
}
