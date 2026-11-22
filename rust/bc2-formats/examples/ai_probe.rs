//! Round AIS's measurements of DICE's AI data (`spec/engine/bot_ai.md`, `spec/model/ai_orders.md`): read-only,
//! structure, counts, enumerator names and numeric ranges only; no names, positions or other content printed.
//!
//!   cargo run --release -p bc2-formats --example ai_probe [-- <game folder>]
//!
//! The folder defaults to Project OPFOR BFBC2's `game` (`OPFOR_ROOT`, else `project-opfor-bfbc2` beside the suite).
//! It reads:
//! - the shared AI partitions: every `ai/*` and `objects/soldiers/templates/*` DBX of `sp_common` and `mp_common`
//!   (`Dist\win32\levels\<common>\*.fbrb`);
//! - each level partition `levels/<level>.dbx` of the campaign levels (`sp_*`, DICE's) and of the Onslaught levels in
//!   the game copy (`ons_*`, installed by OPFOR from Napisal's and the console's data), from `Package\levels\<level>`
//!   when that bundle carries it, else from `Dist\win32\levels\<level>`.
//!
//! Each section names the rules its numbers back.
#![allow(
    // names are lower-cased before the extension test
    clippy::case_sensitive_file_extension_comparisons,
    clippy::map_unwrap_or,
    clippy::collapsible_if,
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::indexing_slicing,
    clippy::doc_markdown,
    clippy::cast_precision_loss,
    clippy::float_cmp,
    missing_docs
)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use bc2_formats::{dbx, fbrb};

fn game_dir() -> PathBuf {
    if let Some(a) = std::env::args().nth(1) {
        return PathBuf::from(a);
    }
    let suite = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let root = std::env::var_os("OPFOR_ROOT")
        .filter(|v| !v.is_empty())
        .map_or_else(|| suite.join("..").join("project-opfor-bfbc2"), PathBuf::from);
    root.join("game")
}

/// One value of one field: a number, or a token (an enumerator, `true` / `false`); anything else is not kept.
enum Val {
    Num(f64),
    Tok(String),
}

/// One `<field>` of an instance: the instance's guid and type, the field's path (complex names joined by `.`),
/// its value or reference, and, for an array, the number of items.
struct Field {
    guid: String,
    typ: String,
    path: String,
    val: Option<Val>,
    reference: Option<String>,
}

/// An instance's arrays: (guid, type, array path, item count, item refs).
struct Array {
    guid: String,
    typ: String,
    path: String,
    refs: Vec<String>,
}

fn attr<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let s = line.find(&key)? + key.len();
    let e = line[s..].find('"')? + s;
    Some(&line[s..e])
}

fn token(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 48
        && v.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The fields and arrays of a converted partition, read line by line (DbxEdit's XML has one element per line).
fn read_xml(xml: &str) -> (Vec<Field>, Vec<Array>, BTreeMap<String, String>) {
    let mut fields = Vec::new();
    let mut arrays: Vec<Array> = Vec::new();
    let mut types = BTreeMap::new();
    let (mut guid, mut typ) = (String::new(), String::new());
    // the open complexes / arrays below the instance: (is_array, name, index into `arrays` for an array)
    let mut stack: Vec<(bool, String, usize)> = Vec::new();
    for raw in xml.lines() {
        let l = raw.trim();
        if l.starts_with("<instance ") {
            guid = attr(l, "guid").unwrap_or_default().to_ascii_uppercase();
            attr(l, "type")
                .unwrap_or_default()
                .rsplit('.')
                .next()
                .unwrap_or_default()
                .clone_into(&mut typ);
            types.insert(guid.clone(), typ.clone());
            stack.clear();
            continue;
        }
        let path_of = |stack: &[(bool, String, usize)], name: &str| {
            let mut p: Vec<&str> = stack.iter().map(|s| s.1.as_str()).collect();
            p.push(name);
            p.join(".")
        };
        if l.starts_with("<complex") {
            let name = attr(l, "name").unwrap_or("item").to_owned();
            if !l.ends_with("/>") && !l.contains("</complex>") {
                stack.push((false, name, 0));
            }
            continue;
        }
        if l.starts_with("<array") {
            let name = attr(l, "name").unwrap_or_default().to_owned();
            let path = path_of(&stack, &name);
            arrays.push(Array {
                guid: guid.clone(),
                typ: typ.clone(),
                path,
                refs: Vec::new(),
            });
            if !l.ends_with("/>") && !l.contains("</array>") {
                stack.push((true, name, arrays.len() - 1));
            }
            continue;
        }
        if l.starts_with("</complex") || l.starts_with("</array") {
            stack.pop();
            continue;
        }
        if l.starts_with("<item") {
            if let (Some(r), Some((true, _, i))) = (attr(l, "ref"), stack.last()) {
                arrays[*i].refs.push(r.to_ascii_uppercase());
            }
            continue;
        }
        if l.starts_with("<field") {
            let name = attr(l, "name").unwrap_or_default();
            let path = path_of(&stack, name);
            let reference = attr(l, "ref").map(str::to_owned);
            let val = l.find('>').and_then(|s| {
                let rest = &l[s + 1..];
                let e = rest.find("</field>")?;
                let v = &rest[..e];
                v.parse::<f64>()
                    .ok()
                    .map(Val::Num)
                    .or_else(|| token(v).then(|| Val::Tok(v.to_owned())))
            });
            fields.push(Field {
                guid: guid.clone(),
                typ: typ.clone(),
                path,
                val,
                reference,
            });
        }
    }
    (fields, arrays, types)
}

/// Per type and field: numeric (min, max, count) and token tallies.
#[derive(Default)]
struct Stats {
    num: BTreeMap<(String, String), (f64, f64, u64)>,
    tok: BTreeMap<(String, String), BTreeMap<String, u64>>,
    inst: BTreeMap<String, BTreeSet<String>>,
}

impl Stats {
    fn add(&mut self, f: &Field) {
        self.inst.entry(f.typ.clone()).or_default().insert(f.guid.clone());
        if f.path == "Name" || f.path.ends_with("DebugText") {
            return;
        }
        match &f.val {
            Some(Val::Num(x)) => {
                let e = self.num.entry((f.typ.clone(), f.path.clone())).or_insert((*x, *x, 0));
                e.0 = e.0.min(*x);
                e.1 = e.1.max(*x);
                e.2 += 1;
            }
            Some(Val::Tok(t)) => {
                *self
                    .tok
                    .entry((f.typ.clone(), f.path.clone()))
                    .or_default()
                    .entry(t.clone())
                    .or_default() += 1;
            }
            None => {}
        }
    }

    fn print(&self, typ_filter: impl Fn(&str) -> bool) {
        for (t, g) in &self.inst {
            if !typ_filter(t) {
                continue;
            }
            println!("  {t}: {} instances", g.len());
            for ((tt, p), (lo, hi, n)) in &self.num {
                if tt == t {
                    println!("    {p}: {lo} .. {hi} (n {n})");
                }
            }
            for ((tt, p), m) in &self.tok {
                if tt == t {
                    let v: Vec<String> = m.iter().map(|(k, c)| format!("{k} {c}")).collect();
                    println!("    {p}: {}", v.join(", "));
                }
            }
        }
    }
}

fn bundles(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .map(|r| {
            r.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("fbrb")))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn level_partition(game: &Path, level: &str) -> Option<(String, Vec<u8>)> {
    let want = format!("levels/{}.dbx", level.to_ascii_lowercase());
    for dir in [
        game.join("Package").join("levels").join(level),
        game.join("Dist").join("win32").join("levels").join(level),
    ] {
        for b in bundles(&dir) {
            let Ok((h, data)) = fbrb::extract_path(&b) else {
                continue;
            };
            for (e, d) in h.entries.iter().zip(data) {
                if !e.is_deleted() && e.name.to_ascii_lowercase() == want {
                    let from = if dir.starts_with(game.join("Package")) {
                        "Package"
                    } else {
                        "Dist"
                    };
                    return Some((from.to_owned(), d));
                }
            }
        }
    }
    None
}

fn is_ai_type(t: &str) -> bool {
    t.ends_with("OrderEntityData")
        || t == "SoldierSpawnEntityData"
        || t == "SquadEntityData"
        || t == "CoverEntityData"
        || t == "AreaTriggerEntityData"
}

fn main() {
    let game = game_dir();
    println!("game folder: {}", game.display());

    // ---- 1. the shared AI partitions
    println!("\n== shared AI partitions (BOT-RT-*, BOT-PER-*, BOT-CMB-*, BOT-MOV-*, BOT-SQD-*, BOT-DIF-*, AIO-RDY-*)");
    let mut digests: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut shared = Stats::default();
    for common in ["sp_common", "mp_common"] {
        let dir = game.join("Dist").join("win32").join("levels").join(common);
        let mut n = 0usize;
        for b in bundles(&dir) {
            let Ok((h, data)) = fbrb::extract_path(&b) else {
                continue;
            };
            for (e, d) in h.entries.iter().zip(data) {
                let name = e.name.to_ascii_lowercase();
                if e.is_deleted() || !name.ends_with(".dbx") {
                    continue;
                }
                if !(name.starts_with("ai/") || name.starts_with("objects/soldiers/templates/")) {
                    continue;
                }
                let Ok(xml) = dbx::to_xml(&d) else { continue };
                n += 1;
                digests
                    .entry(name.clone())
                    .or_default()
                    .insert(common.to_owned(), d.len());
                if common == "sp_common" {
                    let (fields, _, _) = read_xml(&xml);
                    for f in &fields {
                        shared.add(f);
                    }
                }
            }
        }
        println!("{common}: {n} AI / template partitions");
    }
    let both = digests.values().filter(|m| m.len() == 2).count();
    let same = digests
        .values()
        .filter(|m| m.len() == 2 && m.values().collect::<BTreeSet<_>>().len() == 1)
        .count();
    println!("in both commons: {both} (same size: {same}) (BOT-DIF-1)");
    shared.print(|t| {
        matches!(
            t,
            "SensingTemplateData"
                | "BehaviourTemplateData"
                | "SoldierSpawnTemplateData"
                | "DifficultyData"
                | "OrderReadiness"
                | "SquadTypeData"
                | "FormationData"
                | "BotPriorityConfigData"
                | "AIVehicleBehaviourData"
                | "BehaviourData"
                | "WeaponData"
                | "AIAimingConstraintsData"
                | "TurretData"
                | "MobilityData"
                | "ArmamentData"
        )
    });

    // ---- 2. the levels
    let mut levels: Vec<String> = fs::read_dir(game.join("Dist").join("win32").join("levels"))
        .map(|r| {
            r.filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| {
                    let l = n.to_ascii_lowercase();
                    (l.starts_with("sp_") && l != "sp_common") || l.starts_with("ons_")
                })
                .collect()
        })
        .unwrap_or_default();
    levels.sort_by_key(|s| s.to_ascii_lowercase());
    for group in ["sp_", "ons_"] {
        println!(
            "\n== levels {group}* ({}) (AIO-*, BOT-ORD-*, BOT-SQD-*)",
            if group == "sp_" {
                "DICE's campaign"
            } else {
                "Onslaught in the game copy, installed by OPFOR"
            }
        );
        let mut st = Stats::default();
        let mut per_level: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
        let mut target_ev: BTreeMap<(String, String), u64> = BTreeMap::new();
        let mut source_ev: BTreeMap<(String, String), u64> = BTreeMap::new();
        let mut issuers: BTreeMap<String, u64> = BTreeMap::new();
        let mut arr_len: BTreeMap<(String, String), (usize, usize, u64, u64)> = BTreeMap::new();
        let mut area_types: BTreeMap<String, u64> = BTreeMap::new();
        let mut templates: BTreeMap<String, u64> = BTreeMap::new();
        let mut kits: BTreeMap<String, u64> = BTreeMap::new();
        let mut named_spawners: (u64, u64) = (0, 0);
        for lv in levels.iter().filter(|l| l.to_ascii_lowercase().starts_with(group)) {
            let Some((from, data)) = level_partition(&game, lv) else {
                println!("{lv}: no level partition");
                continue;
            };
            let Ok(xml) = dbx::to_xml(&data) else {
                println!("{lv}: not readable");
                continue;
            };
            let (fields, arrays, types) = read_xml(&xml);
            let mut counts: BTreeMap<String, usize> = BTreeMap::new();
            for t in types.values().filter(|t| is_ai_type(t)) {
                *counts.entry(t.clone()).or_default() += 1;
            }
            println!("{lv} ({from}): {counts:?}");
            for (t, c) in &counts {
                *per_level.entry(t.clone()).or_default().entry(lv.clone()).or_default() += c;
            }
            for f in fields.iter().filter(|f| is_ai_type(&f.typ)) {
                st.add(f);
                if f.typ == "SoldierSpawnEntityData" {
                    let part = |r: &str| r.rsplit_once('/').map_or_else(String::new, |(p, _)| p.to_owned());
                    if f.path == "Template" {
                        if let Some(r) = &f.reference {
                            *templates.entry(part(r)).or_default() += 1;
                        }
                    }
                    if f.path == "SoldierKit" {
                        if let Some(r) = &f.reference {
                            *kits.entry(part(r)).or_default() += 1;
                        }
                    }
                }
            }
            // soldiers named by orders, directly or through squads
            let mut ordered: BTreeSet<String> = BTreeSet::new();
            let squad_members: BTreeMap<String, Vec<String>> = arrays
                .iter()
                .filter(|a| a.typ == "SquadEntityData" && a.path == "Soldiers")
                .map(|a| (a.guid.clone(), a.refs.clone()))
                .collect();
            for a in &arrays {
                if !is_ai_type(&a.typ) {
                    continue;
                }
                let e = arr_len
                    .entry((a.typ.clone(), a.path.clone()))
                    .or_insert((usize::MAX, 0, 0, 0));
                e.0 = e.0.min(a.refs.len());
                e.1 = e.1.max(a.refs.len());
                e.2 += 1;
                e.3 += a.refs.len() as u64;
                if a.typ.ends_with("OrderEntityData") {
                    if a.path == "Soldiers" {
                        ordered.extend(a.refs.iter().cloned());
                    }
                    if a.path == "Squads" {
                        for s in &a.refs {
                            ordered.extend(squad_members.get(s).into_iter().flatten().cloned());
                        }
                    }
                    if a.path == "Area" {
                        for r in &a.refs {
                            *area_types
                                .entry(types.get(r).cloned().unwrap_or_else(|| "(other partition)".into()))
                                .or_default() += 1;
                        }
                    }
                }
            }
            // enabled SSAiPlayer spawners and how many an order names
            let mut ai_spawners: BTreeSet<String> = fields
                .iter()
                .filter(|f| f.typ == "SoldierSpawnEntityData" && f.path == "PlayerType")
                .filter(|f| matches!(&f.val, Some(Val::Tok(t)) if t == "SSAiPlayer"))
                .map(|f| f.guid.clone())
                .collect();
            let disabled: BTreeSet<String> = fields
                .iter()
                .filter(|f| f.typ == "SoldierSpawnEntityData" && f.path == "Enabled")
                .filter(|f| matches!(&f.val, Some(Val::Tok(t)) if t == "false"))
                .map(|f| f.guid.clone())
                .collect();
            ai_spawners.retain(|g| !disabled.contains(g));
            named_spawners.0 += ai_spawners.len() as u64;
            named_spawners.1 += ai_spawners.iter().filter(|g| ordered.contains(*g)).count() as u64;
            // event connections: Source / Target refs and event names, in the order of the complex
            let mut cur: (Option<String>, Option<String>, Option<String>, Option<String>) = (None, None, None, None);
            for f in fields
                .iter()
                .filter(|f| f.typ == "LevelData" && f.path.starts_with("EventConnections."))
            {
                let tail = &f.path["EventConnections.".len()..];
                match tail {
                    "item.Source" | "Source" => cur.0 = f.reference.clone().map(|r| r.to_ascii_uppercase()),
                    "item.Target" | "Target" => cur.1 = f.reference.clone().map(|r| r.to_ascii_uppercase()),
                    "item.SourceEvent.Name" | "SourceEvent.Name" => {
                        cur.2 = f
                            .val
                            .as_ref()
                            .and_then(|v| if let Val::Tok(t) = v { Some(t.clone()) } else { None });
                    }
                    "item.TargetEvent.Name" | "TargetEvent.Name" => {
                        cur.3 = f
                            .val
                            .as_ref()
                            .and_then(|v| if let Val::Tok(t) = v { Some(t.clone()) } else { None });
                        let st_ = cur.0.as_ref().and_then(|g| types.get(g)).cloned().unwrap_or_default();
                        let tt = cur.1.as_ref().and_then(|g| types.get(g)).cloned().unwrap_or_default();
                        let se = cur.2.clone().unwrap_or_default();
                        let te = cur.3.clone().unwrap_or_default();
                        if is_ai_type(&tt) {
                            *target_ev.entry((tt.clone(), te.clone())).or_default() += 1;
                            if tt.ends_with("OrderEntityData") && te == "IssueOrder" {
                                let src = if st_.is_empty() {
                                    "(other partition)".to_owned()
                                } else {
                                    st_.clone()
                                };
                                *issuers.entry(format!("{src}.{se}")).or_default() += 1;
                            }
                        }
                        if is_ai_type(&st_) {
                            *source_ev.entry((st_, se)).or_default() += 1;
                        }
                        cur = (None, None, None, None);
                    }
                    _ => {}
                }
            }
        }
        println!("-- instances per type and level");
        for (t, m) in &per_level {
            let total: usize = m.values().sum();
            println!("  {t}: {total} on {} levels", m.len());
        }
        println!(
            "-- enabled SSAiPlayer spawners {} , named by an order (directly or via a squad) {}",
            named_spawners.0, named_spawners.1
        );
        println!("-- spawner templates (partition): {templates:?}");
        println!(
            "-- spawner kits (partition): {} distinct, {} uses",
            kits.len(),
            kits.values().sum::<u64>()
        );
        println!("-- order Area targets by type: {area_types:?}");
        println!("-- arrays (type.path: min .. max, instances, items)");
        for ((t, p), (lo, hi, n, items)) in &arr_len {
            println!("  {t}.{p}: {lo} .. {hi}, {n}, {items}");
        }
        println!("-- events into AI entities (target type, event)");
        for ((t, e), n) in &target_ev {
            println!("  {t} <- {e}: {n}");
        }
        println!("-- events out of AI entities (source type, event)");
        for ((t, e), n) in &source_ev {
            println!("  {t} -> {e}: {n}");
        }
        println!("-- IssueOrder sources (type.event)");
        let mut iss: Vec<_> = issuers.into_iter().collect();
        iss.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        for (k, n) in iss {
            println!("  {k}: {n}");
        }
        println!("-- fields");
        st.print(|_| true);
    }
}
