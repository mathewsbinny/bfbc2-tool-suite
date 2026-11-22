//! The AI specification's data rules (`spec/engine/bot_ai.md`, `spec/model/ai_orders.md`) checked on DICE's AI
//! partitions and campaign levels in Project OPFOR BFBC2's game copy (run by `run_tests.py game`; ignored by a plain
//! `cargo test`):
//!
//!   cargo test --release -p bc2-formats --test ai_corpus -- --ignored --nocapture
//!
//! The game copy is `OPFOR_ROOT`'s `game`, else `project-opfor-bfbc2\game` beside the suite. A test passes with a note
//! when it is missing. The measurements behind the observed rules are `examples/ai_probe.rs`.
#![allow(
    // names are lower-cased before the extension test
    clippy::case_sensitive_file_extension_comparisons,
    clippy::map_unwrap_or,
    clippy::collapsible_if,
    clippy::indexing_slicing,
    clippy::doc_markdown,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    missing_docs
)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use bc2_formats::{dbx, fbrb};

fn game() -> Option<PathBuf> {
    let suite = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let root = std::env::var_os("OPFOR_ROOT")
        .filter(|v| !v.is_empty())
        .map_or_else(|| suite.join("..").join("project-opfor-bfbc2"), PathBuf::from);
    let g = root.join("game");
    if g.join("Dist").join("win32").join("levels").is_dir() {
        Some(g)
    } else {
        println!("skipped: no game copy (OPFOR_ROOT or beside the suite)");
        None
    }
}

fn bundles(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
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

/// Every entry of the common bundle folder `common` whose lower-case name starts with `prefix`, by name.
fn common_entries(game: &Path, common: &str, prefix: &str) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    for b in bundles(&game.join("Dist").join("win32").join("levels").join(common)) {
        let (h, data) = fbrb::extract_path_uncached(&b).expect("a common bundle");
        for (e, d) in h.entries.iter().zip(data) {
            let n = e.name.to_ascii_lowercase();
            if !e.is_deleted() && n.starts_with(prefix) && n.ends_with(".dbx") {
                out.insert(n, d);
            }
        }
    }
    out
}

/// A level's partition, from Package when that copy carries it, else from Dist.
fn level_partition(game: &Path, level: &str) -> Option<Vec<u8>> {
    let want = format!("levels/{}.dbx", level.to_ascii_lowercase());
    for dir in [
        game.join("Package").join("levels").join(level),
        game.join("Dist").join("win32").join("levels").join(level),
    ] {
        for b in bundles(&dir) {
            let Ok((h, data)) = fbrb::extract_path_uncached(&b) else {
                continue;
            };
            for (e, d) in h.entries.iter().zip(data) {
                if !e.is_deleted() && e.name.to_ascii_lowercase() == want {
                    return Some(d);
                }
            }
        }
    }
    None
}

fn attr<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let s = line.find(&key)? + key.len();
    let e = line[s..].find('"')? + s;
    Some(&line[s..e])
}

/// A converted partition's instances: guid -> (type, [(field path, text value or ref)], [(array path, item refs)]).
type Inst = (String, Vec<(String, String)>, Vec<(String, Vec<String>)>);

fn instances(xml: &str) -> BTreeMap<String, Inst> {
    let mut out: BTreeMap<String, Inst> = BTreeMap::new();
    let mut cur = String::new();
    let mut stack: Vec<(bool, String)> = Vec::new();
    for raw in xml.lines() {
        let l = raw.trim();
        if l.starts_with("<instance ") {
            cur = attr(l, "guid").unwrap_or_default().to_ascii_uppercase();
            let t = attr(l, "type")
                .unwrap_or_default()
                .rsplit('.')
                .next()
                .unwrap_or_default()
                .to_owned();
            out.insert(cur.clone(), (t, Vec::new(), Vec::new()));
            stack.clear();
            continue;
        }
        let Some(inst) = out.get_mut(&cur) else { continue };
        let path = |stack: &[(bool, String)], name: &str| {
            let mut p: Vec<&str> = stack.iter().map(|s| s.1.as_str()).collect();
            p.push(name);
            p.join(".")
        };
        if l.starts_with("<complex") || l.starts_with("<array") {
            let is_array = l.starts_with("<array");
            let name = attr(l, "name").unwrap_or("item").to_owned();
            if is_array {
                inst.2.push((path(&stack, &name), Vec::new()));
            }
            if !l.ends_with("/>") && !l.contains("</complex>") && !l.contains("</array>") {
                stack.push((is_array, name));
            }
        } else if l.starts_with("</complex") || l.starts_with("</array") {
            stack.pop();
        } else if l.starts_with("<item") {
            if let (Some(r), Some((true, _))) = (attr(l, "ref"), stack.last()) {
                if let Some(a) = inst.2.last_mut() {
                    a.1.push(r.to_ascii_uppercase());
                }
            }
        } else if l.starts_with("<field") {
            let name = attr(l, "name").unwrap_or_default();
            let v = attr(l, "ref").map(str::to_owned).or_else(|| {
                let s = l.find('>')? + 1;
                let e = l[s..].find("</field>")? + s;
                Some(l[s..e].to_owned())
            });
            inst.1.push((path(&stack, name), v.unwrap_or_default()));
        }
    }
    out
}

fn field<'a>(i: &'a Inst, name: &str) -> Option<&'a str> {
    i.1.iter().find(|(p, _)| p == name).map(|(_, v)| v.as_str())
}

fn campaign_levels(game: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(game.join("Dist").join("win32").join("levels"))
        .map(|r| {
            r.filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| {
                    let l = n.to_ascii_lowercase();
                    l.starts_with("sp_") && l != "sp_common"
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by_key(|s| s.to_ascii_lowercase());
    v
}

// spec: BOT-DIF-1
#[test]
#[ignore = "game tier: needs the game copy"]
fn ai_partitions_are_shared_by_both_commons() {
    let Some(g) = game() else { return };
    let sp = common_entries(&g, "sp_common", "ai/");
    let mp = common_entries(&g, "mp_common", "ai/");
    let names = |m: &BTreeMap<String, Vec<u8>>| -> BTreeSet<String> {
        m.keys().filter(|k| k.matches('/').count() == 1).cloned().collect()
    };
    let (spn, mpn) = (names(&sp), names(&mp));
    println!("ai/* partitions: sp_common {}, mp_common {}", spn.len(), mpn.len());
    assert_eq!(spn.len(), 9, "nine AI/* partitions in sp_common");
    assert_eq!(spn, mpn, "the same AI/* partitions in both commons");
    for n in &spn {
        assert_eq!(sp[n], mp[n], "{n}: byte-identical in both commons");
    }
    // mp_common carries only the two default templates AI/Settings names, the same as sp_common's, and no spawn
    // template (SpawnTemplate_*: sp_common only)
    let mp_t = common_entries(&g, "mp_common", "objects/soldiers/templates/");
    let sp_t = common_entries(&g, "sp_common", "objects/soldiers/templates/");
    let mp_names: Vec<&str> = mp_t.keys().map(String::as_str).collect();
    assert_eq!(
        mp_names,
        [
            "objects/soldiers/templates/ai/defaultsoldierbehaviour.dbx",
            "objects/soldiers/templates/ai/defaultsoldiersensing.dbx"
        ]
    );
    for (n, d) in &mp_t {
        assert_eq!(sp_t.get(n), Some(d), "{n}: byte-identical in both commons");
    }
    let spawn = sp_t.keys().filter(|k| k.contains("/spawntemplate_")).count();
    println!("sp_common spawn templates: {spawn}");
    assert!(spawn > 0);
}

// spec: BOT-DIF-2
#[test]
#[ignore = "game tier: needs the game copy"]
fn difficulty_has_three_modes() {
    let Some(g) = game() else { return };
    let sp = common_entries(&g, "sp_common", "ai/difficulty");
    let data = sp.get("ai/difficulty.dbx").expect("AI/Difficulty");
    let inst = instances(&dbx::to_xml(data).unwrap());
    let modes: Vec<&Inst> = inst.values().filter(|i| i.0 == "DifficultyData").collect();
    assert_eq!(modes.len(), 3);
    let holder = inst
        .values()
        .find(|i| i.0 == "DifficultyModesData")
        .expect("the holder");
    let list = &holder
        .2
        .iter()
        .find(|a| a.0 == "DifficultyModes")
        .expect("DifficultyModes")
        .1;
    let names: Vec<&str> = list
        .iter()
        .map(|r| field(&inst[r], "Name").unwrap_or_default())
        .collect();
    assert_eq!(
        names,
        ["Casual", "Normal", "Hard"],
        "index 0, 1, 2 of the AI init's difficulty"
    );
    for m in modes {
        assert_eq!(
            m.1.iter().filter(|(p, _)| !p.contains('.') && p != "Name").count(),
            16,
            "16 scalar knobs per mode (plus the two box vectors)"
        );
    }
}

// spec: AIO-RDY-1
#[test]
#[ignore = "game tier: needs the game copy"]
fn order_readiness_per_order_type() {
    let Some(g) = game() else { return };
    let sp = common_entries(&g, "sp_common", "ai/orders");
    let inst = instances(&dbx::to_xml(&sp["ai/orders.dbx"]).unwrap());
    let rd: Vec<&Inst> = inst.values().filter(|i| i.0 == "OrderReadiness").collect();
    let types: BTreeSet<&str> = rd.iter().filter_map(|i| field(i, "OrderType")).collect();
    assert_eq!(rd.len(), 11);
    assert_eq!(types.len(), 11, "one OrderReadiness per order type");
    let levels = ["RSPatrol", "RSCombat"];
    for i in &rd {
        let (a, b) = (
            field(i, "Initial").unwrap_or_default(),
            field(i, "Minimum").unwrap_or_default(),
        );
        assert!(
            levels.contains(&a) && a == b,
            "initial = minimum readiness, patrol or combat"
        );
        assert!(
            i.1.iter().any(|(p, _)| p == "UrgencyModifiers.item.Name"),
            "every type adds an urgency"
        );
    }
}

// spec: AIO-FLD-2, AIO-EVT-1
#[test]
#[ignore = "game tier: needs the game copy"]
fn campaign_order_shapes_and_events() {
    let Some(g) = game() else { return };
    let levels = campaign_levels(&g);
    assert_eq!(levels.len(), 13, "the 13 campaign levels");
    let mut into_orders: BTreeMap<String, u64> = BTreeMap::new();
    let (mut area, mut patrol) = (0u64, 0u64);
    for lv in &levels {
        let data = level_partition(&g, lv).expect("a campaign level partition");
        let inst = instances(&dbx::to_xml(&data).unwrap());
        for i in inst.values() {
            let arr = |n: &str| i.2.iter().find(|a| a.0 == n).map_or(usize::MAX, |a| a.1.len());
            if i.0 == "AreaOrderEntityData" {
                area += 1;
                assert_eq!(arr("Area"), 1, "{lv}: an area order has one area");
            }
            if i.0 == "PatrolOrderEntityData" {
                patrol += 1;
                assert_eq!(arr("RouteWaypoints"), 1, "{lv}: a patrol order has one route");
            }
        }
        let level = inst.values().find(|i| i.0 == "LevelData").expect("LevelData");
        let mut target = String::new();
        for (p, v) in &level.1 {
            if p.ends_with("EventConnections.item.Target") {
                target = v.to_ascii_uppercase();
            }
            if p.ends_with("EventConnections.item.TargetEvent.Name") {
                if let Some(t) = inst.get(&target) {
                    if t.0.ends_with("OrderEntityData") {
                        *into_orders.entry(v.clone()).or_default() += 1;
                    }
                }
            }
        }
    }
    println!("area orders {area}, patrol orders {patrol}, events into orders {into_orders:?}");
    assert!(area > 0 && patrol > 0);
    let allowed: BTreeSet<&str> = ["IssueOrder", "Enable", "Disable"].into_iter().collect();
    for k in into_orders.keys() {
        assert!(
            allowed.contains(k.as_str()),
            "an order takes IssueOrder, Enable or Disable, not {k}"
        );
    }
    assert!(into_orders["IssueOrder"] > into_orders.get("Disable").copied().unwrap_or(0) * 10);
}
