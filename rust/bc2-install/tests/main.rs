//! The installer on a fake game copy (round OR1): the plan's tail changes nothing, an apply writes the levels, the
//! shared files through the ledger (records in OPFOR's order) and the registration, the opt-out gives every file back,
//! and the ledger's stage operations (an install keeping the owner's other files, a restate). Project OPFOR's
//! `src\tools\tests\test_ww2_installer.py` runs the same through `bfbc2 install` with OPFOR's own plan, a kill at
//! every write step included.

#![allow(
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test assertions on known fixtures"
)]

use std::fs;
use std::path::{Path, PathBuf};

use bc2_gamefs::ledger::Ledger;
use bc2_install::plan::Plan;
use bc2_install::run::Run;
use serde_json::{Value, json};

const MFT: &str = "Signature 0123\r\n\r\nBundleSource game:levels/mp_001/level-00.fbrb\r\n\
                   BundleTarget package:levels/mp_001/level-00.fbrb\r\n";
const BM: &str = "levels/mp_001/level\t00000000-0000-0000-0000-000000000000\t1\n\
                  levels/ons_mp_005gr/terrain\t00000000-0000-0000-0000-000000000000\n";
const INI: &str = "[dev]\r\nlevel=Levels/MP_005GR\r\n[debug]\r\nprobes=0\r\n";
const KINDS: [&str; 5] = ["level", "terrain", "ingame", "loader", "vo/en"];

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("bc2-install-test-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).ok();
    d
}

fn put(p: &Path, data: &[u8]) -> String {
    if let Some(d) = p.parent() {
        fs::create_dir_all(d).ok();
    }
    fs::write(p, data).ok();
    p.to_string_lossy().into_owned()
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// A fake game copy and staging under d, and the plan OPFOR would write for Wake alone.
fn fake(d: &Path, ok: bool) -> Plan {
    let g = d.join("game");
    let w = d.join("work");
    put(&g.join("Dist/win32/package.mft"), MFT.as_bytes());
    put(&g.join("Dist/win32/bundleManifest"), BM.as_bytes());
    put(&g.join("opfor.ini"), INI.as_bytes());
    put(&g.join("maps.ini"), b"[MP_001]\r\nname=Panama Canal\r\n");
    for (f, b) in [
        ("Package/async/startup-00.fbrb", "startup"),
        ("Dist/win32/overlay-00.fbrb", "overlay"),
        ("Package/async/ondemand_awards-00.fbrb", "ondemand"),
    ] {
        put(&g.join(f), b.as_bytes());
    }
    let built: Vec<Value> = KINDS
        .iter()
        .map(|k| {
            json!([
                k,
                put(&w.join(format!("install/{k}-00.fbrb")), format!("built {k}").as_bytes())
            ])
        })
        .collect();
    let blocks: Vec<String> = KINDS
        .iter()
        .map(|k| format!("BundleSource game:levels/wake_island_s/{k}-00.fbrb"))
        .collect();
    let shared: Vec<Value> = [
        "Package/async/startup-00.fbrb",
        "Dist/win32/overlay-00.fbrb",
        "Package/async/ondemand_awards-00.fbrb",
    ]
    .iter()
    .map(|f| json!({ "file": s(&g.join(f)), "built": put(&w.join(format!("patched/{f}")), b"patched") }))
    .collect();
    let pkg = g.join("Package/levels/wake_island_s");
    let common = g.join("Package/levels/ww2_common");
    let names = ["wake_island_s", "guadal_canal", "iwo_jima_s", "coral_sea"].join("|");
    let plan = json!({
        "format": "bfbc2-install-plan/1", "game": s(&g), "opfor": s(d), "owner": "wake", "fix_owner": "manifestfix",
        "full": false,
        "levels": [{ "lv": "wake_island_s", "name": "WAKE_ISLAND_S", "pkg": s(&pkg),
                     "record": s(&pkg.join("opfor-wake-installed.json")), "work": s(&w.join("install")),
                     "kinds": KINDS, "built": built, "mft_blocks": blocks, "report": ["model groups: ok"], "ok": ok }],
        "registration": [{ "lv": "wake_island_s", "title": "Wake Island", "kinds": KINDS, "mft_blocks": blocks }],
        "registration_all": s(&w.join("install/registration_all.txt")),
        "common": { "folder": s(&common), "record": s(&common.join("opfor-ww2common-installed.json")),
                    "built": [["level", put(&w.join("install/ww2_common/level-00.fbrb"), b"common")]] },
        "markers": [], "shared": shared,
        "shared_files": [s(&g.join("Package/async/startup-00.fbrb")), s(&g.join("Dist/win32/overlay-00.fbrb")),
                         s(&g.join("Package/async/ondemand_awards-00.fbrb"))],
        "shared_report": [],
        "ini": { "file": s(&g.join("opfor.ini")), "dev_level": "Levels/WAKE_ISLAND_S",
                 "pacific_levels": "Levels/WAKE_ISLAND_S" },
        "maps": { "file": s(&g.join("maps.ini")), "mark": "; --- Pacific War ---", "strip": ["wake_island_s"],
                  "levels": [{ "name": "WAKE_ISLAND_S", "title": "Wake Island", "browser_mode": "Conquest" }] },
        "mft": s(&g.join("Dist/win32/package.mft")), "bm": s(&g.join("Dist/win32/bundleManifest")),
        "marks": { "entry_re": format!("(?i)^(levels/({names})_description\\.dbx$)"),
                   "text_re": format!("(?im)^\\s*(bundlesource\\s+game:)?levels/({names})/|^\\s*level\\s*=\\s*levels/({names})\\s*$"),
                   "ui_prefix": "systems/ui/shaders/", "ui_mark": "PacificMinimap", "own_root": s(&w.join("names")),
                   "list_entry": "LevelListQuery.dbx", "list_re": format!("(?i)({names})(_description)?\\.dbx") },
        "verify": { "startup": s(&g.join("Package/async/startup-00.fbrb")),
                    "ondemand": s(&g.join("Package/async/ondemand_awards-00.fbrb")),
                    "overlay": s(&g.join("Dist/win32/overlay-00.fbrb")), "pictures": "Awards/images/weapons/WW2",
                    "level_list": "LevelListQuery.dbx", "description_fields": [["Licenses", "nam"]] },
        "staged": { "path": s(&w.join("staged.json")), "w": s(&w), "not_input": "^(install.*|_rebuild)$", "code": [] },
        "others": [], "lock": s(&w.join("_rebuild/rebuild.lock")),
    });
    serde_json::from_value(plan).unwrap_or_else(|e| panic!("plan: {e}"))
}

fn read(p: &str) -> Vec<u8> {
    fs::read(p).unwrap_or_default()
}

fn shared(plan: &Plan) -> Vec<Vec<u8>> {
    [&plan.mft, &plan.bm, &plan.ini.file, &plan.maps.file]
        .into_iter()
        .chain(&plan.shared_files)
        .map(|p| read(p))
        .collect()
}

#[test]
fn plan_apply_revert_on_a_fake_game() {
    let d = tmp("flow");
    let plan = fake(&d, true);
    let before = shared(&plan);
    let mut lines = Vec::new();
    let mut log = |l: &str| lines.push(l.to_owned());
    let code = Run::new(&plan, &mut log).and_then(|mut r| r.install(false, false));
    assert_eq!(code, Ok(0), "{lines:?}");
    assert_eq!(shared(&plan), before);
    assert!(lines.contains(&"  package.mft: the level's blocks written (0 there before, 5 now)".to_owned()));
    assert!(
        fs::read_to_string(&plan.registration_all)
            .unwrap_or_default()
            .contains("levels/wake_island_s/vo/en\t00000000-0000-0000-0000-000000000000\t1")
    );
    // apply: the fake bundles do not open, so the verification fails (exit 1) after everything was written
    let mut lines = Vec::new();
    let mut log = |l: &str| lines.push(l.to_owned());
    let code = Run::new(&plan, &mut log).and_then(|mut r| r.install(true, false));
    assert_eq!(code, Ok(1), "{lines:?}");
    assert!(lines.contains(&"WAKE_ISLAND_S: FAILED".to_owned()), "{lines:?}");
    let pkg = Path::new(&plan.levels[0].pkg);
    assert_eq!(read(&s(&pkg.join("vo/en-00.fbrb"))), b"built vo/en");
    let rec = fs::read_to_string(&plan.levels[0].record).unwrap_or_default();
    assert!(
        rec.starts_with("{\n \"level\": \"") && rec.contains("\"vo/en\""),
        "{rec}"
    );
    let mft = String::from_utf8(read(&plan.mft)).unwrap_or_default();
    assert_eq!(mft.matches("BundleSource game:levels/wake_island_s/").count(), 5);
    assert!(String::from_utf8_lossy(&read(&plan.ini.file)).contains("pacific_levels=Levels/WAKE_ISLAND_S"));
    let led = Ledger::new(Path::new(&plan.game));
    let files = led.owner_files("wake").unwrap_or_default();
    assert_eq!(
        files,
        [
            "Package/async/startup-00.fbrb",
            "Dist/win32/overlay-00.fbrb",
            "Package/async/ondemand_awards-00.fbrb",
            "opfor.ini",
            "maps.ini",
            "Dist/win32/package.mft",
            "Dist/win32/bundleManifest"
        ]
    );
    // the opt-out: every shared file back (the bundleManifest repair kept as its own install), the folders gone
    let mut lines = Vec::new();
    let mut log = |l: &str| lines.push(l.to_owned());
    let code = Run::new(&plan, &mut log).and_then(|mut r| r.revert());
    assert_eq!(code, Ok(0), "{lines:?}");
    let mut now = shared(&plan);
    let mut was = before.clone();
    assert_ne!(now.remove(1), was.remove(1));
    assert_eq!(now, was);
    assert!(!pkg.exists() && !Path::new(&plan.common.folder).exists());
    assert_eq!(led.owner_files("wake").unwrap_or_default(), Vec::<String>::new());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_failed_gate_writes_nothing() {
    let d = tmp("gate");
    let plan = fake(&d, false);
    let before = shared(&plan);
    let mut log = |_: &str| {};
    let code = Run::new(&plan, &mut log).and_then(|mut r| r.install(true, false));
    assert_eq!(code, Ok(1));
    assert_eq!(shared(&plan), before);
    assert!(!Path::new(&plan.levels[0].pkg).exists());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn install_keep_orders_and_restate() {
    let dir = tmp("keep");
    let game = dir.join("game");
    let (fa, fb) = ("a.ini", "b.ini");
    put(&game.join(fa), b"A");
    put(&game.join(fb), b"B");
    let mut led = Ledger::new(&game);
    assert!(
        led.install_keep("wake", &[(fa.to_owned(), b"A1".to_vec())], true, "", false)
            .is_ok()
    );
    assert!(
        led.install_keep("wake", &[(fb.to_owned(), b"B1".to_vec())], true, "", false)
            .is_ok()
    );
    // the owner's other file kept; a changed one goes after the untouched ones (OPFOR's record order)
    assert!(
        led.install_keep("wake", &[(fa.to_owned(), b"A2".to_vec())], true, "", false)
            .is_ok()
    );
    assert_eq!(led.owner_files("wake").unwrap_or_default(), [fb, fa]);
    assert_eq!(read(&s(&game.join(fa))), b"A2");
    // unchanged: nothing written
    let plan = led.install_keep("wake", &[(fb.to_owned(), b"B1".to_vec())], true, "", false);
    assert!(plan.is_ok_and(|p| p.notes.iter().any(|n| n.contains("unchanged"))));
    // restate: the file edited outside, taken as the owner's with a new prior; a revert gives that prior back
    put(&game.join(fa), b"A2 edited");
    assert!(led.restate("wake", fa, false, Some(b"A edited"), "test").is_ok());
    let layers = led.file_layers(fa).unwrap_or_default();
    assert_eq!(layers[0].1["extra"]["restated"], "test");
    assert!(led.uninstall("wake", false).is_ok());
    assert_eq!(read(&s(&game.join(fa))), b"A edited");
    assert_eq!(read(&s(&game.join(fb))), b"B");
    let _ = fs::remove_dir_all(&dir);
}
