//! The map export's install (round OR6, feature `mapedit`) on a synthetic game copy and synthetic stages: the text
//! patches add and take out exactly, an install and its revert give every file back byte for byte through the ledger,
//! a second export beside the first and one taken out of two, the shared patches built from each install's kept copy
//! and never from the editor's live stage (audit AU4-DS-12), and a kill at every write step of an install and a revert
//! finished by the next run. Project OPFOR's `src\tools\tests\test_mapedit_install.py` runs the same command line on
//! real exports.

#![cfg(feature = "mapedit")]
#![allow(
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::type_complexity,
    reason = "test assertions on known fixtures"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use bc2_formats::dbx::{self, DbxFile, Element};
use bc2_formats::fbrb;
use bc2_install::mapedit::{self, Game, Installer, KEPT, RECORD};
use bc2_install::stages::{CRASHED, CrashAt, Steps};
use bc2_pyrt::json::{self, Value};

/// A ledger record's transaction id and time (masked: they differ from run to run).
static TXN: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r#""(txn|installed)": "[^"]*""#).unwrap());

const G1: &str = "11111111-2222-3333-4444-555555555555";
const G2: &str = "66666666-7777-8888-9999-AAAAAAAAAAAA";

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("bc2-install-mapedit-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn put(p: &Path, data: &[u8]) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, data).unwrap();
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// The menus' level list partition with `items`.
fn level_list(items: &[&str]) -> Vec<u8> {
    let mut f = DbxFile::default();
    let kids = items.iter().map(|i| Element::text(&mut f, "item", &[], i)).collect();
    let arr = Element::container(&mut f, "array", &[("name", "Results")], kids);
    let inst = Element::container(
        &mut f,
        "instance",
        &[("guid", G1), ("type", "Core.FileQueryData")],
        vec![arr],
    );
    let part = Element::container(
        &mut f,
        "partition",
        &[("guid", G2), ("primaryInstance", G1)],
        vec![inst],
    );
    f.roots.push(part);
    dbx::write(&f).unwrap()
}

/// A synthetic game copy: the startup bundle (a level list and a stock description), the on-demand awards bundle,
/// maps.ini, package.mft and bundleManifest.
fn game(d: &Path) -> PathBuf {
    let g = d.join("game");
    let list = level_list(&["MP_001.dbx", "MP_001_description.dbx"]);
    put(
        &g.join(r"Package\async\startup-00.fbrb"),
        &fbrb::build(
            &[
                ("Levels/MP_001_description.dbx", "<non-resource>", b"stock description"),
                ("LevelListQuery.dbx", "<non-resource>", &list),
            ],
            true,
        ),
    );
    put(
        &g.join(r"Package\async\ondemand_awards-00.fbrb"),
        &fbrb::build(
            &[("Awards/images/Levels/mp_001.res", "ITexture", b"stock picture")],
            true,
        ),
    );
    put(
        &g.join("maps.ini"),
        b"; maps\r\n[MP_001]\r\nname=Panama Canal\r\nConquest=Levels/MP_001\r\n",
    );
    put(
        &g.join(r"Dist\win32\package.mft"),
        b"Signature 0123\r\n\r\nBundleSource game:levels/mp_001/level-00.fbrb\r\nBundleTarget package:levels/mp_001/level-00.fbrb\r\n",
    );
    put(
        &g.join(r"Dist\win32\bundleManifest"),
        b"levels/mp_001/level\t00000000-0000-0000-0000-000000000000\t1\n",
    );
    fs::create_dir_all(g.join(r"Package\levels")).unwrap();
    g
}

/// A synthetic stage of level `lv` (its own pictures when `named`), its description `desc`.
fn stage(d: &Path, lv: &str, named: bool, desc: &[u8]) -> PathBuf {
    let st = d.join("stages").join(lv.to_lowercase()).join("stage");
    let low = lv.to_lowercase();
    let mut files = Vec::new();
    for k in ["level", "terrain", "ingame", "loader"] {
        let data = format!("{lv} {k} bundle").into_bytes();
        put(&st.join(format!("{k}-00.fbrb")), &data);
        files.push(Value::dict([
            ("path", Value::Str(format!("{k}-00.fbrb"))),
            ("install", Value::Str(format!("Package/levels/{low}/{k}-00.fbrb"))),
            ("sha1", Value::Str(String::new())),
            ("size", Value::Int(data.len() as i128)),
            ("entries", Value::Int(1)),
        ]));
    }
    put(&st.join(format!("{lv}_description.dbx")), desc);
    let mut awards = Vec::new();
    if named {
        for (dir, name) in [("loadingscreens", "loadingScreens"), ("levels", "Levels")] {
            let data = format!("{lv} {dir} picture").into_bytes();
            put(&st.join("awards").join(dir).join(format!("{low}.res")), &data);
            awards.push(Value::dict([
                ("path", Value::Str(format!("awards/{dir}/{low}.res"))),
                ("install", Value::Str(format!("Awards/images/{name}/{low}.res"))),
                ("sha1", Value::Str(String::new())),
                ("size", Value::Int(data.len() as i128)),
                ("entries", Value::Int(1)),
            ]));
        }
    }
    let rec = Value::dict([
        ("format", Value::Str(mapedit::STAGE_FORMAT.into())),
        ("level", Value::Str(lv.into())),
        ("title", Value::Str(format!("{lv} title"))),
        ("mode", Value::Str("Conquest".into())),
        ("base", Value::Str("mp_001".into())),
        ("base_sha1", Value::Str(String::new())),
        ("project", Value::Str("test".into())),
        ("files", Value::List(files)),
        (
            "description",
            Value::List(vec![
                Value::Str(format!("{lv}_description.dbx")),
                Value::Str(format!("Levels/{lv}_description.dbx")),
            ]),
        ),
        (
            "mft_blocks",
            Value::List(
                ["level", "terrain", "ingame", "loader"]
                    .iter()
                    .map(|k| {
                        Value::List(vec![
                            Value::Str(format!("BundleSource game:levels/{low}/{k}-00.fbrb")),
                            Value::Str(format!("BundleTarget package:levels/{low}/{k}-00.fbrb")),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "bundle_manifest",
            Value::List(
                ["level", "terrain", "ingame", "loader"]
                    .iter()
                    .map(|k| Value::Str(format!("levels/{low}/{k}\t{}\t1", mapedit::ZERO)))
                    .collect(),
            ),
        ),
        (
            "level_list",
            Value::List(vec![
                Value::Str(format!("{low}.dbx")),
                Value::Str(format!("{low}_description.dbx")),
            ]),
        ),
        ("report", Value::List(Vec::new())),
        (
            "converted",
            Value::dict([
                ("removed", Value::List(Vec::new())),
                ("connections_removed", Value::Int(0)),
                ("references_cut", Value::Int(0)),
                ("grafted", Value::List(Vec::new())),
                ("connections_grafted", Value::Int(0)),
                ("not_followed", Value::List(Vec::new())),
                ("placed", Value::List(Vec::new())),
                ("fields", Value::List(Vec::new())),
            ]),
        ),
        ("strings", Value::List(Vec::new())),
        ("awards", Value::List(awards)),
    ]);
    put(&st.join("stage.json"), json::dumps(&rec, Some(2)).as_bytes());
    st
}

/// Every file under the game copy (the ledger's records with their transaction id and time masked).
fn files(g: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut todo = vec![g.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                todo.push(p);
                continue;
            }
            let rel = p.strip_prefix(g).unwrap().to_string_lossy().replace('\\', "/");
            let mut data = fs::read(&p).unwrap();
            if rel.starts_with("opfor/ledger/") && rel.ends_with(".json") {
                let t = String::from_utf8(data).unwrap();
                let t = TXN.replace_all(&t, "\"$1\": \"*\"").into_owned();
                data = t.into_bytes();
            }
            out.insert(rel, data);
        }
    }
    out
}

/// The game copy's files less the ledger (what DICE's and the stock files look like to the game).
fn game_files(g: &Path) -> BTreeMap<String, Vec<u8>> {
    files(g).into_iter().filter(|(k, _)| !k.starts_with("opfor/")).collect()
}

fn installer<'a>(g: &Path, work: &Path, log: &'a mut dyn FnMut(&str), at: Option<CrashAt>) -> Installer<'a> {
    Installer::with_steps(Game::new(&s(g), None), &s(work), Steps::new(at), log).without_check()
}

fn apply(g: &Path, work: &Path, st: &Path) -> Vec<String> {
    let mut lines = Vec::new();
    let mut log = |l: &str| lines.push(l.to_owned());
    installer(g, work, &mut log, None).apply(&s(st)).unwrap();
    lines
}

fn revert(g: &Path, work: &Path, lv: &[&str]) {
    let mut log = |_: &str| {};
    let names: Vec<String> = lv.iter().map(|x| (*x).to_owned()).collect();
    installer(g, work, &mut log, None).revert(&names).unwrap();
}

fn entries(bundle: &Path) -> BTreeMap<String, Vec<u8>> {
    let (h, data) = fbrb::extract_path_uncached(bundle).unwrap();
    h.entries
        .iter()
        .zip(data)
        .filter(|(e, _)| !e.is_deleted())
        .map(|(e, d)| (e.name.clone(), d))
        .collect()
}

#[test]
fn text_patches_add_and_take_out() {
    let d = tmp("text");
    let st = stage(&d, "MAPEDIT_X_CQ", false, b"x");
    let rec = mapedit::read_stage(&s(&st)).unwrap();
    let drop: BTreeSet<String> = ["mapedit_x_cq".to_owned()].into();
    let mft = "Signature x\r\nAuthorative\r\n\r\nBundleSource game:levels/mp_005/level-00.fbrb\r\nBundleTarget a\r\n";
    let once = mapedit::mft_with(mft, std::slice::from_ref(&rec), &drop);
    assert_eq!(once.matches("BundleSource game:levels/mapedit_x_cq/").count(), 4);
    assert_eq!(mapedit::mft_with(&once, std::slice::from_ref(&rec), &drop), once);
    assert_eq!(mapedit::mft_with(&once, &[], &drop), mft);
    // DICE's file ends with a blank line: kept (XINST-MAP-7), so only the level's blocks change
    let dice =
        "Signature x\r\nAuthorative\r\n\r\nBundleSource game:levels/sp_04/vo/ru-00.fbrb\r\nBundleTarget b\r\n\r\n";
    let with = mapedit::mft_with(dice, std::slice::from_ref(&rec), &drop);
    assert!(
        with.ends_with("BundleTarget package:levels/mapedit_x_cq/loader-00.fbrb\r\n\r\n"),
        "{with:?}"
    );
    assert_eq!(mapedit::mft_with(&with, &[], &drop), dice);
    let bm = format!("levels/mp_005/level\t{}\t1\n", mapedit::ZERO);
    let with = mapedit::bm_with(&bm, std::slice::from_ref(&rec), &drop);
    assert_eq!(with.lines().count(), 5);
    assert_eq!(mapedit::bm_with(&with, &[], &drop), bm);
    let ini = "; head\n[MP_005]\nname=Atacama Desert\nConquest=Levels/MP_005\n";
    let with = mapedit::maps_with(ini, std::slice::from_ref(&rec), &drop);
    assert!(with.contains("[MAPEDIT_X_CQ]\nname=MAPEDIT_X_CQ title\nConquest=Levels/MAPEDIT_X_CQ\n"));
    assert!(with.contains(mapedit::MAPS_MARK));
    assert_eq!(mapedit::maps_with(&with, &[], &drop), ini);
    assert_eq!(mapedit::browser_row(&rec), "OPFOR - MAPEDIT_X_CQ title (Conquest)");
    assert_eq!(mapedit::ai_owner("MAPEDIT_X_CQ"), "mapedit-ai:mapedit_x_cq");
    assert_eq!(mapedit::ascii_repr("Conqu\u{ea}te \u{2019}"), "'Conqu\\xeate \\u2019'");
    // the files a set needs: the pictures only while a level has its own
    let gm = Game::new("g", None);
    assert_eq!(gm.needed(&[]), Vec::<String>::new());
    assert!(!gm.needed(std::slice::from_ref(&rec)).contains(&gm.awards()));
    let named = mapedit::read_stage(&s(&stage(&d, "MAPEDIT_Y_CQ", true, b"y"))).unwrap();
    assert!(gm.needed(&[rec, named]).contains(&gm.awards()));
    assert_eq!(gm.shared()[4..], [gm.mft(), gm.bm()], "the registration last");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn install_beside_and_revert_give_every_file_back() {
    let d = tmp("apply");
    let g = game(&d);
    let work = d.join("work");
    let start = game_files(&g);
    let a = stage(&d, "MAPEDIT_A_CQ", true, b"description of A");
    let b = stage(&d, "MAPEDIT_B_CQ", false, b"description of B");
    let lines = apply(&g, &work, &a);
    assert!(lines.iter().any(|l| l == "MAPEDIT_A_CQ: installed"), "{lines:?}");
    assert!(
        lines
            .iter()
            .any(|l| l.contains("write Package\\async\\ondemand_awards-00.fbrb")),
        "{lines:?}"
    );
    let folder = g.join(r"Package\levels\mapedit_a_cq");
    assert!(folder.join(RECORD).is_file() && folder.join(KEPT).join("stage.json").is_file());
    let startup = entries(&g.join(r"Package\async\startup-00.fbrb"));
    assert_eq!(startup["Levels/MAPEDIT_A_CQ_description.dbx"], b"description of A");
    let awards = entries(&g.join(r"Package\async\ondemand_awards-00.fbrb"));
    assert_eq!(
        awards["Awards/images/Levels/mapedit_a_cq.res"],
        b"MAPEDIT_A_CQ levels picture"
    );
    assert!(
        !work.join("mapedit_a_cq").join("shared").exists(),
        "the scratch is removed"
    );
    // the record: the stage's fields in order, then the stage and the bundles' sha256
    let rec = json::loads(&fs::read_to_string(folder.join(RECORD)).unwrap()).unwrap();
    let Value::Dict(items) = &rec else { panic!() };
    let keys: Vec<&str> = items.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys.last(), Some(&"installed"));
    assert_eq!(keys[keys.len() - 2], "stage");
    revert(&g, &work, &["MAPEDIT_A_CQ"]);
    assert_eq!(game_files(&g), start, "an install and its revert give every file back");
    // a second export beside the first, one taken out of two, then all
    apply(&g, &work, &a);
    apply(&g, &work, &a); // a redo on top: built from the ledger's copy, the same files
    let one = files(&g);
    apply(&g, &work, &b);
    let mft = fs::read_to_string(g.join(r"Dist\win32\package.mft")).unwrap();
    assert_eq!(mft.matches("BundleSource game:levels/mapedit_a_cq/").count(), 4);
    assert_eq!(mft.matches("BundleSource game:levels/mapedit_b_cq/").count(), 4);
    let mut st = Vec::new();
    let mut log = |l: &str| st.push(l.to_owned());
    installer(&g, &work, &mut log, None).status().unwrap();
    assert_eq!(
        st,
        [
            "MAPEDIT_A_CQ: MAPEDIT_A_CQ title (Conquest, from mp_001)",
            "MAPEDIT_B_CQ: MAPEDIT_B_CQ title (Conquest, from mp_001)"
        ]
    );
    // A out: B has no pictures, so the awards bundle goes back as it was before the installer
    revert(&g, &work, &["MAPEDIT_A_CQ"]);
    assert_eq!(
        fs::read(g.join(r"Package\async\ondemand_awards-00.fbrb")).unwrap(),
        start[r"Package/async/ondemand_awards-00.fbrb"]
    );
    assert!(!g.join(r"Package\levels\mapedit_a_cq").exists());
    apply(&g, &work, &a);
    revert(&g, &work, &["MAPEDIT_B_CQ"]);
    // the files as they were (the owner's ledger record lists its contributions in the order they were last written)
    let now = files(&g);
    let differ: Vec<&String> = one
        .keys()
        .chain(now.keys())
        .filter(|k| one.get(*k) != now.get(*k))
        .collect();
    assert_eq!(
        differ, ["opfor/ledger/mapedit.json"; 2],
        "B in and out again: A's install as it was"
    );
    revert(&g, &work, &[]);
    assert_eq!(game_files(&g), start);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn the_patches_come_from_each_installs_kept_copy() {
    let d = tmp("kept");
    let g = game(&d);
    let work = d.join("work");
    let a = stage(&d, "MAPEDIT_A_CQ", true, b"description of A");
    let b = stage(&d, "MAPEDIT_B_CQ", false, b"description of B");
    apply(&g, &work, &a);
    // the editor exports A again (not installed): its live stage changes
    fs::write(a.join("MAPEDIT_A_CQ_description.dbx"), b"A exported again").unwrap();
    fs::write(a.join(r"awards\levels\mapedit_a_cq.res"), b"a new picture").unwrap();
    apply(&g, &work, &b);
    revert(&g, &work, &["MAPEDIT_B_CQ"]);
    let startup = entries(&g.join(r"Package\async\startup-00.fbrb"));
    assert_eq!(
        startup["Levels/MAPEDIT_A_CQ_description.dbx"], b"description of A",
        "A's patch is the installed A's, not the stage exported since (AU4-DS-12)"
    );
    let awards = entries(&g.join(r"Package\async\ondemand_awards-00.fbrb"));
    assert_eq!(
        awards["Awards/images/Levels/mapedit_a_cq.res"],
        b"MAPEDIT_A_CQ levels picture"
    );
    // an install from before the kept copies reads its stage folder, and says so
    fs::remove_dir_all(g.join(r"Package\levels\mapedit_a_cq").join(KEPT)).unwrap();
    let lines = apply(&g, &work, &b);
    assert!(
        lines
            .iter()
            .any(|l| l.contains("MAPEDIT_A_CQ: installed without a kept copy of its stage")),
        "{lines:?}"
    );
    // a stock level's folder is never replaced
    let mut stock = stage(&d, "MP_001", false, b"x");
    put(&g.join(r"Package\levels\mp_001\level-00.fbrb"), b"stock");
    let mut log = |_: &str| {};
    let e = installer(&g, &work, &mut log, None).apply(&s(&stock)).unwrap_err();
    assert!(e.contains("is not an exported level"), "{e}");
    stock.push("nothing");
    assert!(installer(&g, &work, &mut log, None).apply(&s(&stock)).is_err());
    let _ = fs::remove_dir_all(&d);
}

/// The files less the ledger's blobs (a rolled-back commit may leave a blob no record names; they are content
/// addressed and pruned by `bfbc2 mods`).
fn no_blobs(f: BTreeMap<String, Vec<u8>>) -> BTreeMap<String, Vec<u8>> {
    f.into_iter()
        .filter(|(k, _)| !k.starts_with("opfor/ledger/blobs/"))
        .collect()
}

/// The lock a killed process leaves, given a process id that is gone.
fn kill(g: &Path) {
    let p = g.join(r"opfor\ledger\.lock");
    if p.is_file() {
        fs::write(p, br#"{"pid": 4000000000, "started": "2026-11-27T00:00:00Z"}"#).unwrap();
    }
}

#[test]
fn a_kill_at_every_write_step_is_finished_by_the_next_run() {
    let d = tmp("kill");
    let a = stage(&d, "MAPEDIT_A_CQ", true, b"description of A");
    let b = stage(&d, "MAPEDIT_B_CQ", true, b"description of B");
    let ops: [(&str, &dyn Fn(&mut Installer<'_>) -> Result<(), String>); 3] = [
        ("apply A", &|i| i.apply(&s(&a))),
        ("apply B beside A", &|i| i.apply(&s(&b))),
        ("revert A", &|i| i.revert(&["MAPEDIT_A_CQ".to_owned()])),
    ];
    // the uninterrupted run's files after each operation
    let clean = d.join("clean");
    let g0 = game(&clean);
    let w0 = clean.join("work");
    let mut want = Vec::new();
    let mut counts = Vec::new();
    for (_, op) in &ops {
        let mut log = |_: &str| {};
        let mut inst = installer(&g0, &w0, &mut log, None);
        op(&mut inst).unwrap();
        counts.push(inst.steps());
        want.push(files(&g0));
    }
    let mut kills = 0;
    for (i, (what, op)) in ops.iter().enumerate() {
        assert!(counts[i] >= 3, "{what}: {} write steps", counts[i]);
        for k in 1..=counts[i] {
            let run = d.join(format!("run-{i}-{k}"));
            let g = game(&run);
            let w = run.join("work");
            let mut log = |_: &str| {};
            for (_, before) in ops.iter().take(i) {
                before(&mut installer(&g, &w, &mut log, None)).unwrap();
            }
            let r = op(&mut installer(&g, &w, &mut log, Some(CrashAt::Return(k))));
            assert_eq!(r.err().as_deref(), Some(CRASHED), "{what}: step {k}");
            kill(&g);
            kills += 1;
            // the same command again: finished, as if never interrupted
            op(&mut installer(&g, &w, &mut log, None)).unwrap_or_else(|e| panic!("{what}: step {k}: {e}"));
            assert_eq!(
                no_blobs(files(&g)),
                no_blobs(want[i].clone()),
                "{what}: killed at step {k} of {}",
                counts[i]
            );
            let _ = fs::remove_dir_all(&run);
        }
    }
    assert!(kills >= 25, "{kills} kills");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn path_data_goes_through_the_ledger_under_its_own_owner() {
    let d = tmp("ai");
    let g = game(&d);
    let work = d.join("work");
    let start = game_files(&g);
    let st = stage(&d, "MAPEDIT_T_CQ", false, b"t");
    put(&st.join(r"ai\infantry\pathdata.hpd"), b"path data of a test level");
    // stage.json's path_data: the staged file and its install path
    let p = st.join("stage.json");
    let Value::Dict(mut items) = json::loads(&fs::read_to_string(&p).unwrap()).unwrap() else {
        panic!()
    };
    let file = Value::dict([
        ("path", Value::Str("ai/infantry/pathdata.hpd".into())),
        (
            "install",
            Value::Str("Output/win32/ai/levels/mapedit_t_cq/infantry/pathdata.hpd".into()),
        ),
        ("sha1", Value::Str(String::new())),
        ("size", Value::Int(25)),
        ("entries", Value::Int(0)),
    ]);
    items.push((
        "path_data".into(),
        Value::dict([
            ("files", Value::List(vec![file])),
            ("source", Value::Str("a test".into())),
            ("base", Value::Str("mp_001".into())),
        ]),
    ));
    fs::write(&p, json::dumps(&Value::Dict(items), Some(2))).unwrap();
    let lines = apply(&g, &work, &st);
    assert!(
        lines.iter().any(|l| l
            == r"  new   Output\win32\ai\levels\mapedit_t_cq\infantry\pathdata.hpd (the level's path data, through the install ledger)"),
        "{lines:?}"
    );
    let dst = g.join(r"Output\win32\ai\levels\mapedit_t_cq\infantry\pathdata.hpd");
    assert_eq!(fs::read(&dst).unwrap(), b"path data of a test level");
    let held = || {
        bc2_gamefs::ledger::Ledger::new(&g)
            .owner_files("mapedit-ai:mapedit_t_cq")
            .unwrap()
    };
    assert_eq!(held(), ["Output/win32/ai/levels/mapedit_t_cq/infantry/pathdata.hpd"]);
    revert(&g, &work, &["MAPEDIT_T_CQ"]);
    assert!(!dst.exists() && held().is_empty());
    assert_eq!(
        game_files(&g)
            .into_iter()
            .filter(|(k, _)| !k.starts_with("Output/"))
            .collect::<BTreeMap<_, _>>(),
        start
    );
    let _ = fs::remove_dir_all(&d);
}
