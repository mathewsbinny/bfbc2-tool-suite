//! Stage 6 of the console Onslaught port (round OR10): the port of Project OPFOR's `console_soldiers.py`'s `build`
//! and `apply` up to the ledger - the console's own soldier classes back: OPFOR blueprints of every console Onslaught
//! AI class (`objects/soldiers/ons_ru_ai/*`) and player kit (`ons_us/*`) the level uses, built on the PC soldier each
//! was swapped for in stage 1 ([`super::SOLDIERS`]), and the spawners and the Onslaught team pointed at them.
//!
//! Why: the console soldier classes cannot load on PC (their `PlayerEntryComponentData` has the console's
//! `InputActionMaps`, PC's has `InputConceptDefinition` / `InputMapping`), so stage 1 swapped each for the nearest PC
//! soldier, losing what the console classes are. Rules (the user, 2026-10-30):
//!
//! | from | fields |
//! |---|---|
//! | PC (the soldier it was swapped for) | input, character physics, animation trees, death effect, every other field; the dropped weapon's settings (lifetime, ammo, slots, auto pickup) |
//! | console | the kit ([`SPOTTING`], `Weapons`, `WeaponUnlocks`), the kit class (`Category`), the spotted times, the head collision; the dropped weapon's weapons; player kits also [`PLAYER`] (DICE's co-op tuning) |
//!
//! Each blueprint is a copy of the PC soldier's partition under `Objects/Soldiers/OPFOR_ONS_RU_AI|OPFOR_ONS_US/<class>`
//! with new partition and instance GUIDs (`opfor-soldier:<new partition>:<old GUID>`: the PC soldier is loaded too).
//! Console data they reach that PC lacks is copied from the console's shared data (stage 1's `con_mp2`): data files
//! only - a partition with console resources is refused. Spawners are re-pointed per console class (the spawner's
//! console `SoldierKit`, read from the console level by its GUID), the Onslaught team's `Kits` item by item. Spawners
//! and teams name the `SoldierAsset`: a `SoldierEntityData` there gave the voice-over set-up a null `Object` (crash at
//! 0x6EA3F0, automated load tests 2026-10-30).
//!
//! The work folder's files are the Python's byte for byte (rust/BENCHMARKS.md "OR10"); the Python read the class map
//! from `console_onslaught.py`'s text, which round OR5 moved here (its read had failed since: XCONS-ONS-6).

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::{NoExpand, Regex};

use super::port::{
    StageOut, drop_xml, find, manifest_lines, set_sizes, under, write_manifest, write_text_utf8, xml_cached,
};
use super::{OnsError, R, Source, copy, dbx, fbrb, join, joins, pack, read, res_files, rmtree, size, write};

/// The console fields every class gets (`SoldierEntityData`).
pub const SPOTTING: [&str; 5] = [
    "ActiveSpottedTime",
    "PassiveSpottedTime",
    "LastKnownSpottedTime",
    "Category",
    "HeadCollision",
];
/// The console fields a player kit gets too (on every type of its blueprint).
pub const PLAYER: [&str; 6] = [
    "RegenerateHealth",
    "ImmortalTimeAfterSpawn",
    "CriticalHealthJesusModeTime",
    "TimeForCorpse",
    "LockAimToMeleeAttacker",
    "IsSelectable",
];
/// The console family -> the OPFOR family the blueprints are written under.
pub const FAMILY: [(&str, &str); 2] = [("ons_ru_ai", "OPFOR_ONS_RU_AI"), ("ons_us", "OPFOR_ONS_US")];

fn re(p: &str) -> Regex {
    #[allow(
        clippy::expect_used,
        reason = "the patterns are constants (or escaped names), each compiled by the tests"
    )]
    Regex::new(p).expect("a constant pattern")
}

static REF: LazyLock<Regex> = LazyLock::new(|| re(r#"ref="([A-Za-z0-9_!/\-]+)/[0-9A-Fa-f-]{36}""#));
static NAME: LazyLock<Regex> = LazyLock::new(|| re(r#"<field name="Name">([^<]*)</field>"#));
static PART: LazyLock<Regex> = LazyLock::new(|| re(r#"<partition guid="([^"]+)""#));
static KIT: LazyLock<Regex> =
    LazyLock::new(|| re(r#"(?i)ref="(objects/soldiers/ons_[a-z_]+/[a-z0-9_]+)/[0-9a-f-]{36}" name="SoldierKit""#));
static KITS: LazyLock<Regex> = LazyLock::new(|| re(r#"(?s)<array name="Kits">(.*?)</array>"#));
static SOLDIER_KIT: LazyLock<Regex> = LazyLock::new(|| re(r#"<field ref="[^"]+" name="SoldierKit" />"#));
static ITEM: LazyLock<Regex> = LazyLock::new(|| re(r#"<item ref="[^"]+" />"#));
static COMPLEX: LazyLock<Regex> = LazyLock::new(|| re(r"(?s)<complex>.*?</complex>"));
static SLOT: LazyLock<Regex> = LazyLock::new(|| re(r#"<field name="WeaponSlot">([^<]*)"#));

/// What stage 6 is given.
#[derive(Debug, Clone, Default)]
pub struct Args {
    /// The OPFOR folder: its `work\console_soldiers\` is written; `work\onslaught\build_<lv>\con_level` and `con_mp2`
    /// are read (DbxEdit's XML written beside their partitions where there is none); `work\console_shaders\shared`'s
    /// mp_common / sp_common level unpacks read (unpacked from the game copy when missing).
    pub root: String,
    /// The game copy: read only.
    pub game: String,
    /// The Onslaught level (lower case).
    pub lv: String,
    /// The bundle to build from (the ledger's `source`).
    pub src_bundle: String,
    /// `--apply` (the bundle packed) rather than `--plan`.
    pub write: bool,
    /// Experiments (`OPFOR_SOLDIER_KEEP_PC`): fields kept the PC's (`Pickup`: the dropped weapon).
    pub keep_pc: Vec<String>,
    /// `OPFOR_SOLDIER_ONLY`: `ai` (the AI classes only) or `kits` (the player kits only), else both.
    pub only: String,
    /// `OPFOR_SOLDIER_ORDER=end`: the new manifest lines appended rather than put ahead of the level's.
    pub order_end: bool,
    /// `OPFOR_SOLDIER_GUIDS=keep`: the PC soldier's own GUIDs kept.
    pub keep_guids: bool,
}

/// An instance of a partition's XML: (GUID upper case, the type's last name, the body's span).
struct Inst {
    guid: String,
    ty: String,
    body: std::ops::Range<usize>,
}

/// `instances(text)`.
fn instances(text: &str) -> Vec<Inst> {
    super::mode::INST
        .captures_iter(text)
        .filter_map(|c| {
            let b = c.get(4)?;
            Some(Inst {
                guid: c[2].to_uppercase(),
                ty: c[3].rsplit('.').next().unwrap_or("").to_owned(),
                body: b.range(),
            })
        })
        .collect()
}

/// `element(body, name)`: the `<field ...name="name"...>` element (or `<array name="name">...</array>`) of an instance
/// body.
fn element(body: &str, name: &str) -> Option<String> {
    let n = regex::escape(name);
    if let Some(m) = re(&format!(r#"<field (?:ref="[^"]*" )?name="{n}"(?: />|>[^<]*</field>)"#)).find(body) {
        return Some(m.as_str().to_owned());
    }
    re(&format!(r#"(?s)<array name="{n}">.*?</array>|<array name="{n}" />"#))
        .find(body)
        .map(|m| m.as_str().to_owned())
}

/// `str.replace(old, new, 1)`.
fn replace1(s: &str, old: &str, new: &str) -> String {
    s.replacen(old, new, 1)
}

/// `opfor_guid('soldier', new_part, g)`.
fn soldier_guid(new_part: &str, g: &str) -> String {
    bc2_formats::guid::new_guid("opfor-soldier", &[new_part, g])
}

/// The new GUIDs of a blueprint: (old, new), the partition's first, then its instances' in order (each once).
type Guids = Vec<(String, String)>;

/// `build_blueprint(con_text, pc_text, pc_part, new_part, new_name, player)`: the OPFOR blueprint's XML, its notes
/// and its new GUIDs.
#[expect(clippy::too_many_lines, reason = "console_soldiers.build_blueprint, step by step")]
fn build_blueprint(
    a: &Args,
    con_text: &str,
    pc_text: &str,
    pc_part: &str,
    new_part: &str,
    new_name: &str,
    player: bool,
) -> R<(String, Vec<String>, Guids)> {
    let mut t = pc_text.to_owned();
    let old_name = NAME
        .captures(&t)
        .ok_or_else(|| OnsError::Data("not found: Name".into()))?[1]
        .to_owned();
    t = replace1(
        &t,
        &format!(r#"<field name="Name">{old_name}</field>"#),
        &format!(r#"<field name="Name">{new_name}</field>"#),
    );
    t = re(&format!("(?i){}", regex::escape(&format!("{pc_part}/"))))
        .replace_all(&t, NoExpand(&format!("{new_part}/")))
        .into_owned();
    // new guids for the partition and its instances (the PC soldier is loaded too) - applied after the import
    // (rename_guids): with the PC soldier's own guids the import keeps every value's exact type and bytes
    let part_guid = PART
        .captures(&t)
        .ok_or_else(|| OnsError::Data("not found: partition guid".into()))?[1]
        .to_uppercase();
    let mut guids: Guids = Vec::new();
    for g in std::iter::once(part_guid).chain(instances(&t).into_iter().map(|i| i.guid)) {
        if !guids.iter().any(|(o, _)| *o == g) {
            let n = soldier_guid(new_part, &g);
            guids.push((g, n));
        }
    }
    // {ty: body}: the last instance of a type wins, at its type's first place
    let mut con: Vec<(String, String)> = Vec::new();
    for i in instances(con_text) {
        let body = con_text[i.body].to_owned();
        match con.iter_mut().find(|(ty, _)| *ty == i.ty) {
            Some(e) => e.1 = body,
            None => con.push((i.ty, body)),
        }
    }
    let con_of = |ty: &str| con.iter().find(|(t, _)| t == ty).map_or("", |(_, b)| b.as_str());
    let mut notes: Vec<String> = Vec::new();
    let set_fields = |t: &mut String, notes: &mut Vec<String>, ty: &str, names: &[&str]| {
        let Some(inst) = instances(t).into_iter().find(|i| i.ty == ty) else {
            return;
        };
        let mut body = t[inst.body.clone()].to_owned();
        for n in names {
            if a.keep_pc.iter().any(|k| k == n) {
                continue;
            }
            let Some(ce) = element(con_of(ty), n) else {
                continue;
            };
            let Some(pe) = element(&body, n) else {
                notes.push(format!("{ty}.{n}: not on the PC soldier - left out"));
                continue;
            };
            body = replace1(&body, &pe, &ce);
        }
        t.replace_range(inst.body, &body);
    };
    // the kit: Weapons (the loadout; for the AI the weapon clones it fires) and WeaponUnlocks (a player kit's unlock
    // list: 124 on the console assault, with the frag rounds and G3 / MK14 variants PC's kits lack)
    let mut names: Vec<&str> = SPOTTING.to_vec();
    names.extend(["Weapons", "WeaponUnlocks"]);
    if player {
        names.extend(PLAYER);
    }
    set_fields(&mut t, &mut notes, "SoldierEntityData", &names);
    if player {
        // the console blueprint's other types, in its order (the Python's set had no order: XCONS-ONS-6)
        for (ty, _) in con.iter().filter(|(ty, _)| ty != "SoldierEntityData") {
            set_fields(&mut t, &mut notes, ty, &PLAYER);
        }
    }
    // the dropped weapon (the AI's WeaponPickupEntityData): the PC pickup's own settings (lifetime, auto ammo pickup,
    // unspawn), its entries the console's - the kit's weapons with the MP-443 and the ammo box (6 on the console
    // assault, 2 on the campaign one) - with the PC entry's ammo where the PC has an entry for the same weapon slot
    let cpu = if a.keep_pc.iter().any(|k| k == "Pickup") {
        ""
    } else {
        con_of("WeaponPickupEntityData")
    };
    if let Some(carr) = element(cpu, "Weapons")
        && let Some(inst) = instances(&t).into_iter().find(|i| i.ty == "WeaponPickupEntityData")
    {
        let body = t[inst.body.clone()].to_owned();
        match element(&body, "Weapons") {
            None => notes.push("WeaponPickupEntityData.Weapons: not on the PC soldier - left out".into()),
            Some(parr) => {
                let slot = |e: &str| SLOT.captures(e).map(|c| c[1].to_owned());
                let mut pc_by_slot: HashMap<Option<String>, String> = HashMap::new();
                for e in COMPLEX.find_iter(&parr) {
                    pc_by_slot.insert(slot(e.as_str()), e.as_str().to_owned());
                }
                let mut out = carr.clone();
                for e in COMPLEX.find_iter(&carr) {
                    let e = e.as_str();
                    let mut ne = e.to_owned();
                    if let Some(pe) = pc_by_slot.get(&slot(e)).filter(|p| !p.is_empty()) {
                        for f in ["MinAmmo", "MaxAmmo"] {
                            if let (Some(ce), Some(pf)) = (element(e, f), element(pe, f))
                                && !ce.is_empty()
                                && !pf.is_empty()
                            {
                                ne = replace1(&ne, &ce, &pf);
                            }
                        }
                    }
                    out = replace1(&out, e, &ne);
                }
                let body = replace1(&body, &parr, &out);
                t.replace_range(inst.body, &body);
            }
        }
    }
    Ok((t, notes, guids))
}

/// `rename_guids(dbx, new_part, guids, text)`: the partition's and instances' new GUIDs in the imported binary
/// (DbxEdit `edit`: `partguid`, `replace` - for the spellings that occur in the partition's text: DbxEdit refuses a
/// replace that matches nothing).
fn rename_guids(dbx_path: &str, new_part: &str, guids: &Guids, text: &str) -> R<()> {
    let Some((_, part)) = guids.first() else {
        return Ok(());
    };
    let mut cmds = vec![format!("partguid={part}")];
    for (old, new) in guids.iter().skip(1) {
        for (a, b) in [
            (
                format!("{new_part}/{}", old.to_lowercase()),
                format!("{new_part}/{}", new.to_lowercase()),
            ),
            (old.clone(), new.clone()),
            (old.to_lowercase(), new.to_lowercase()),
        ] {
            if text.contains(&format!("\"{a}\"")) {
                cmds.push(format!("replace={a}|{b}"));
            }
        }
    }
    let cmdfile = format!("{dbx_path}.cmd");
    write_text_utf8(&cmdfile, &(cmds.join("\n") + "\n"))?;
    let new = format!("{dbx_path}.new");
    dbx(&["edit", dbx_path, &new, &format!("@{cmdfile}")])?;
    move_over(&new, dbx_path)?;
    std::fs::remove_file(&cmdfile).map_err(|e| super::io_err(&cmdfile, &e))
}

/// `shutil.move(src, dst)` onto a file.
fn move_over(src: &str, dst: &str) -> R<()> {
    write(dst, &read(src)?)?;
    std::fs::remove_file(src).map_err(|e| super::io_err(src, &e))
}

/// `index(*roots)`: lower-case partition path -> .dbx file under the roots (`glob('**/*.dbx')`: hidden names
/// skipped; the first root wins).
fn index(roots: &[&str]) -> (HashMap<String, String>, Vec<String>) {
    fn walk(d: &str, rel: &str, out: &mut Vec<(String, String)>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let p = join(d, &name);
            let r = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            if std::fs::metadata(&p).is_ok_and(|m| m.is_dir()) {
                walk(&p, &r, out);
            } else if name.to_lowercase().ends_with(".dbx") {
                out.push((r, p));
            }
        }
    }
    let mut out: HashMap<String, String> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for r in roots {
        let mut files = Vec::new();
        walk(r, "", &mut files);
        for (rel, p) in files {
            let k = super::stem4(&rel).to_lowercase();
            if let std::collections::hash_map::Entry::Vacant(e) = out.entry(k.clone()) {
                order.push(k);
                e.insert(p);
            }
        }
    }
    (out, order)
}

/// `closure(texts, have, con_ix)`: console partitions the new blueprints reach that the level's bundles do not have,
/// in the order found.
fn closure(texts: &[String], have: &HashSet<String>, con_ix: &HashMap<String, String>) -> R<Vec<(String, String)>> {
    let mut need: Vec<(String, String)> = Vec::new();
    let mut got: HashSet<String> = HashSet::new();
    let mut stack: Vec<String> = texts
        .iter()
        .flat_map(|t| REF.captures_iter(t).map(|c| c[1].to_lowercase()))
        .collect();
    while let Some(p) = stack.pop() {
        if have.contains(&p) || got.contains(&p) {
            continue;
        }
        let Some(f) = con_ix.get(&p) else {
            continue;
        };
        got.insert(p.clone());
        need.push((p, f.clone()));
        let x = xml_cached(f)?;
        stack.extend(REF.captures_iter(&x).map(|c| c[1].to_lowercase()));
    }
    Ok(need)
}

/// One new blueprint.
struct Made {
    class: String,
    new_name: String,
    reference: String,
    text: String,
    like: String,
    new_part: String,
    guids: Guids,
}

/// `console_soldiers.build(lv)` and, with `write`, `apply`'s pack.
#[expect(clippy::too_many_lines, reason = "console_soldiers.build, step by step")]
pub fn build(a: &Args) -> R<StageOut> {
    let lv = a.lv.as_str();
    if !super::mode::LEVELS.contains(&lv) {
        return Err(OnsError::Data(format!("KeyError: '{lv}'")));
    }
    let name = lv.to_uppercase();
    let work = joins(&a.root, &["work", "console_soldiers"]);
    let shared = joins(&a.root, &["work", "console_shaders", "shared"]);
    let b = joins(&a.root, &["work", "onslaught", &format!("build_{lv}")]);
    let con_mp2 = join(&b, "con_mp2");
    let console = join(&b, "con_level");
    let con_level = joins(&console, &["levels", &format!("{name}.dbx")]);
    let bundle = joins(&a.game, &["Package", "levels", lv, "level-00.fbrb"]);
    let src = joins(&work, &[lv, "pkg"]);
    rmtree(&src)?;
    fbrb(&["unpack", &a.src_bundle, &src])?;
    for (key, level) in [("sp_common_level", "sp_common"), ("mp_common_level", "mp_common")] {
        let d = join(&shared, key);
        if !std::path::Path::new(&d).is_dir() {
            let from = joins(&a.game, &["Dist", "win32", "levels", level, "level-00.fbrb"]);
            fbrb(&["unpack", &from, &d])?;
        }
    }
    let (pc_ix, pc_order) = index(&[
        &src,
        &join(&shared, "mp_common_level"),
        &join(&shared, "sp_common_level"),
    ]);
    let con_roots = [console.as_str(), con_mp2.as_str()];
    let (con_ix, _) = index(&con_roots);
    let swap: HashMap<&str, &str> = super::SOLDIERS.iter().copied().collect();
    let con_lv = xml_cached(&con_level)?;
    // the console classes the level uses: spawners (SoldierKit) and the Onslaught team (Kits)
    let mut spawner_class: Vec<(String, String)> = Vec::new();
    for i in instances(&con_lv) {
        if i.ty == "SoldierSpawnEntityData"
            && let Some(k) = KIT.captures(&con_lv[i.body])
        {
            let c = k[1].to_lowercase();
            match spawner_class.iter_mut().find(|(g, _)| *g == i.guid) {
                Some(e) => e.1 = c,
                None => spawner_class.push((i.guid, c)),
            }
        }
    }
    let team_dbx = pc_ix.get("objects/teams/us_ons").cloned();
    let team_con = match con_ix.get("objects/teams/us_ons") {
        Some(f) => xml_cached(f)?,
        None => String::new(),
    };
    let mut team_kits: Vec<String> = if team_con.is_empty() {
        Vec::new()
    } else {
        let k = KITS
            .captures(&team_con)
            .ok_or_else(|| OnsError::Data("not found: Kits".into()))?;
        REF.captures_iter(&k[1]).map(|c| c[1].to_lowercase()).collect()
    };
    match a.only.as_str() {
        "ai" => team_kits.clear(),
        "kits" => spawner_class.clear(),
        _ => {}
    }
    let mut classes: Vec<String> = spawner_class
        .iter()
        .map(|(_, c)| c.clone())
        .chain(team_kits.iter().cloned())
        .collect::<HashSet<String>>()
        .into_iter()
        .collect();
    classes.sort();
    let mut made: Vec<Made> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    for c in &classes {
        let parts: Vec<&str> = c.split('/').collect();
        let (Some(fam), Some(cls)) = (parts.get(2), parts.get(3)) else {
            return Err(OnsError::Data("IndexError: list index out of range".into()));
        };
        let pc_part = swap.get(c.as_str()).copied();
        let (Some(pc_part), Some(con_file)) = (pc_part.filter(|p| pc_ix.contains_key(*p)), con_ix.get(c)) else {
            return Err(OnsError::Exit(format!(
                "{lv}: {c}: no PC soldier ({}) or no console blueprint",
                pc_part.unwrap_or("None")
            )));
        };
        let cls_name = super::stem4(&bc2_pyrt::py::basename(con_file)).to_owned();
        let family = FAMILY
            .iter()
            .find(|(f, _)| f == fam)
            .map(|(_, f)| *f)
            .ok_or_else(|| OnsError::Data(format!("KeyError: '{fam}'")))?;
        let new_part = format!("objects/soldiers/{}/{cls}", family.to_lowercase());
        let new_name = format!("Objects/Soldiers/{family}/{cls_name}");
        let (t, n, guids) = build_blueprint(
            a,
            &xml_cached(con_file)?,
            &xml_cached(&pc_ix[pc_part])?,
            pc_part,
            &new_part,
            &new_name,
            *fam == "ons_us",
        )?;
        notes.extend(n.iter().map(|x| format!("{cls}: {x}")));
        let asset = instances(&t)
            .into_iter()
            .find(|i| i.ty == "SoldierAsset")
            .map(|i| i.guid)
            .ok_or_else(|| OnsError::Data("StopIteration: no SoldierAsset".into()))?;
        let asset = if a.keep_guids {
            asset
        } else {
            guids
                .iter()
                .find(|(o, _)| *o == asset)
                .map(|(_, n)| n.clone())
                .ok_or_else(|| OnsError::Data(format!("KeyError: '{asset}'")))?
        }
        .to_lowercase();
        made.push(Made {
            class: c.clone(),
            reference: format!("{new_part}/{asset}"),
            new_name,
            text: t,
            like: pc_ix[pc_part].clone(),
            new_part,
            guids,
        });
    }
    let mut have: HashSet<String> = pc_order.iter().cloned().collect();
    have.extend(made.iter().map(|m| super::partition_of(&m.reference).to_owned()));
    let texts: Vec<String> = made.iter().map(|m| m.text.clone()).collect();
    let extra = closure(&texts, &have, &con_ix)?;
    for (p, f) in &extra {
        if !res_files(f).is_empty() {
            return Err(OnsError::Exit(format!(
                "{lv}: console partition {p} has resources (console builds) - not usable on PC"
            )));
        }
    }
    // write the blueprints and the copied console data into the unpacked bundle
    let man = join(&src, "fbrb.manifest");
    let mut lines = manifest_lines(&man)?;
    let names: HashSet<String> = lines
        .iter()
        .map(|l| l.split('|').next().unwrap_or("").to_lowercase())
        .collect();
    let mut con_man: HashMap<String, String> = HashMap::new();
    for r in con_roots {
        for l in manifest_lines(&join(r, "fbrb.manifest"))? {
            con_man
                .entry(l.split('|').next().unwrap_or("").to_lowercase())
                .or_insert(l);
        }
    }
    let tmp = joins(&work, &[lv, "tmp"]);
    std::fs::create_dir_all(&tmp).map_err(|e| super::io_err(&tmp, &e))?;
    // new partitions go ahead of the level's own entry (the bundle loads in manifest order, the level last of what it
    // references), each at the level's place: what is added later (the copied console data the blueprints name) ends
    // up ahead of the blueprints; OPFOR_SOLDIER_ORDER=end appends them instead
    let level_rel = format!("levels/{lv}.dbx");
    let level_at = lines
        .iter()
        .position(|l| l.split('|').next().unwrap_or("").to_lowercase() == level_rel)
        .unwrap_or(lines.len());
    let add_line = |lines: &mut Vec<String>, line: String| {
        if a.order_end {
            lines.push(line);
        } else {
            lines.insert(level_at, line);
        }
    };
    for m in &made {
        let rel = format!("{}.dbx", m.new_name);
        let out = under(&src, &rel);
        if let Some(d) = std::path::Path::new(&out).parent() {
            std::fs::create_dir_all(d).map_err(|e| super::io_err(&d.to_string_lossy(), &e))?;
        }
        let x = join(&tmp, &format!("{}.xml", super::stem4(&bc2_pyrt::py::basename(&rel))));
        write_text_utf8(&x, &m.text)?;
        dbx(&["import", &x, &out, "--like", &m.like, "--types", &con_mp2])?;
        if !a.keep_guids {
            rename_guids(&out, &m.new_part, &m.guids, &m.text)?;
        }
        if !names.contains(&rel.to_lowercase()) {
            let sz = size(&out)?;
            add_line(&mut lines, format!("{rel}|<non-resource>|00010000|{sz}|{sz}"));
        }
    }
    for (_, f) in &extra {
        let root = con_roots
            .iter()
            .find(|r| f.starts_with(&format!("{}\\", r.trim_end_matches(['\\', '/']))))
            .ok_or_else(|| OnsError::Data("StopIteration: no console root".into()))?;
        let rel = super::rel(f, root).replace('\\', "/");
        let dst = under(&src, &rel);
        copy(f, &dst)?;
        if !names.contains(&rel.to_lowercase()) {
            let line = if let Some(l) = con_man.get(&rel.to_lowercase()) {
                l.clone()
            } else {
                let sz = size(f)?;
                format!("{rel}|<non-resource>|00010000|{sz}|{sz}")
            };
            add_line(&mut lines, line);
        }
    }
    // the level: each spawner's SoldierKit -> the OPFOR blueprint of its console class
    let level_dbx = find(&src, &format!("Levels/{name}.dbx"))?;
    let mut lt = xml_cached(&level_dbx)?;
    let mut moved = 0usize;
    for i in instances(&lt).into_iter().rev() {
        let Some((_, c)) = spawner_class.iter().find(|(g, _)| *g == i.guid) else {
            continue;
        };
        let Some(m) = made
            .iter()
            .find(|m| m.class == *c)
            .filter(|_| i.ty == "SoldierSpawnEntityData")
        else {
            continue;
        };
        let body = &lt[i.body.clone()];
        let nb = SOLDIER_KIT
            .replacen(
                body,
                1,
                NoExpand(&format!(r#"<field ref="{}" name="SoldierKit" />"#, m.reference)),
            )
            .into_owned();
        if nb != body {
            moved += 1;
            lt.replace_range(i.body, &nb);
        }
    }
    let x = join(&tmp, "level.xml");
    write_text_utf8(&x, &lt)?;
    let new = format!("{level_dbx}.new");
    dbx(&["import", &x, &new, "--like", &level_dbx])?;
    move_over(&new, &level_dbx)?;
    // the Onslaught team's kits, item by item (the console's order)
    let mut team_note = "no Onslaught team".to_owned();
    if let Some(team_dbx) = team_dbx.as_deref()
        && !team_kits.is_empty()
    {
        let tt = xml_cached(team_dbx)?;
        let arr = KITS
            .captures(&tt)
            .and_then(|c| c.get(1))
            .ok_or_else(|| OnsError::Data("not found: Kits".into()))?;
        let items: Vec<&str> = ITEM.find_iter(arr.as_str()).map(|m| m.as_str()).collect();
        if items.len() != team_kits.len() {
            return Err(OnsError::Exit(format!(
                "{lv}: the Onslaught team has {} kits, the console {}",
                items.len(),
                team_kits.len()
            )));
        }
        let mut new_arr = arr.as_str().to_owned();
        for (it, c) in items.iter().zip(&team_kits) {
            let m = made
                .iter()
                .find(|m| m.class == *c)
                .ok_or_else(|| OnsError::Data(format!("KeyError: '{c}'")))?;
            new_arr = replace1(&new_arr, it, &format!(r#"<item ref="{}" />"#, m.reference));
        }
        let x = join(&tmp, "team.xml");
        write_text_utf8(&x, &format!("{}{new_arr}{}", &tt[..arr.start()], &tt[arr.end()..]))?;
        let new = format!("{team_dbx}.new");
        dbx(&["import", &x, &new, "--like", team_dbx])?;
        move_over(&new, team_dbx)?;
        team_note = format!("Onslaught team: {} kits", items.len());
    }
    drop_xml(&src)?;
    set_sizes(&mut lines, &[level_rel.as_str(), "objects/teams/us_ons.dbx"], &|n| {
        size(&under(&src, n))
    })?;
    write_manifest(&man, &lines)?;
    let check = bc2_tools::dbxedit::run(
        &[
            "check".to_owned(),
            level_dbx.clone(),
            "--bundle".to_owned(),
            src.clone(),
        ],
        None,
    );
    let mut out = StageOut::default();
    let mut copied: Vec<&str> = extra.iter().map(|(p, _)| p.rsplit('/').next().unwrap_or("")).collect();
    copied.sort_unstable();
    out.printed.push(format!(
        "{lv}: {} OPFOR soldier classes ({}); {moved} spawners re-pointed; {team_note}; {} console data partitions \
         copied in ({})",
        made.len(),
        made.iter()
            .map(|m| m.class.rsplit('/').next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join(", "),
        extra.len(),
        copied.join(", ")
    ));
    out.printed.extend(notes.iter().map(|n| format!("  {n}")));
    let last = bc2_pyrt::py::splitlines(bc2_pyrt::py::strip(&check.stdout))
        .last()
        .copied()
        .unwrap_or("?")
        .to_owned();
    out.printed.push(format!("  check: {last}"));
    if a.write {
        let built = joins(&work, &[lv, "level-00.fbrb"]);
        if let Err(e) = pack(&src, &built) {
            out.fail = Some(e.to_string());
            return Ok(out);
        }
        out.writes.push((bundle, Source::File(built)));
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn patterns() {
        let _ = (
            &*REF,
            &*NAME,
            &*PART,
            &*KIT,
            &*KITS,
            &*SOLDIER_KIT,
            &*ITEM,
            &*COMPLEX,
            &*SLOT,
        );
        let body = r#"<field name="A">1</field><field ref="x/1" name="B" /><array name="C"><item ref="a" /></array>"#;
        assert_eq!(element(body, "A").unwrap(), r#"<field name="A">1</field>"#);
        assert_eq!(element(body, "B").unwrap(), r#"<field ref="x/1" name="B" />"#);
        assert_eq!(
            element(body, "C").unwrap(),
            r#"<array name="C"><item ref="a" /></array>"#
        );
        assert!(element(body, "D").is_none());
        // the GUID scope: opforcore.guid's 'soldier' (opfor-soldier:<new partition>:<old guid>)
        assert_eq!(
            soldier_guid("objects/soldiers/opfor_ons_us/ons_assault", "ABC"),
            bc2_formats::guid::new_guid("opfor-soldier:objects/soldiers/opfor_ons_us/ons_assault", &["ABC"])
        );
    }

    #[test]
    fn blueprint_takes_the_console_kit() {
        let inst = |g: &str, ty: &str, body: &str| format!("<instance guid=\"{g}\" type=\"{ty}\">{body}</instance>\n");
        let pc = format!(
            "<partition guid=\"p1\" primaryInstance=\"a1\">\n{}{}</partition>\n",
            inst(
                "a1",
                "Entity.SoldierAsset",
                "<field name=\"Name\">Objects/Soldiers/SP_RU/SP_RU_Assault</field>"
            ),
            inst(
                "s1",
                "Entity.SoldierEntityData",
                "<field name=\"Category\">0</field><array name=\"Weapons\"><item ref=\"objects/soldiers/sp_ru/sp_ru_assault/x\" /></array>"
            )
        );
        let con = inst(
            "c1",
            "Entity.SoldierEntityData",
            "<field name=\"Category\">3</field><array name=\"Weapons\"><item ref=\"w/g3\" /></array><field name=\"HeadCollision\">1</field>",
        );
        let a = Args::default();
        let (t, notes, guids) = build_blueprint(
            &a,
            &con,
            &pc,
            "objects/soldiers/sp_ru/sp_ru_assault",
            "objects/soldiers/opfor_ons_ru_ai/ai_ru_assault",
            "Objects/Soldiers/OPFOR_ONS_RU_AI/AI_RU_Assault",
            false,
        )
        .unwrap();
        assert!(t.contains("<field name=\"Category\">3</field>"));
        assert!(t.contains("<item ref=\"w/g3\" />"));
        assert!(t.contains("OPFOR_ONS_RU_AI/AI_RU_Assault</field>"));
        assert_eq!(
            notes,
            ["SoldierEntityData.HeadCollision: not on the PC soldier - left out"]
        );
        assert_eq!(guids.len(), 3);
        assert_eq!(guids[0].0, "P1");
    }
}
