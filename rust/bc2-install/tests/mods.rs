//! The mod manager on a fake game copy (spec `transforms/install.md` section 5, XINST-MOD-*): OPFOR's import, an
//! Onslaught console port's stages, the Pacific War (owner `wake`, its level folders, the DLL's marker), the Pacific
//! menus, NavKit and stock-MP on one level, the Vietnam setting - installed through the ledger as the tools install
//! them - then disabled, enabled, uninstalled and restored to DICE's files in every order the tests name, with every
//! game file (bytes and write time) compared, and each operation interrupted at every write step.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test assertions on known fixtures"
)]

use std::cell::Cell;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, UNIX_EPOCH};

use bc2_gamefs::ledger::{Interrupt, Ledger};
use bc2_install::mods::{Manager, ModError, mtime_ns};

const MFT: &str = "Dist/win32/package.mft";
const BM: &str = "Dist/win32/bundleManifest";
const STARTUP: &str = "Package/async/startup-00.fbrb";
const LEVEL: &str = "Package/levels/ons_mp_002/level-00.fbrb";
const MENU: &str = "Package/menu/main/mainmenu-00.fbrb";
const HPD: &str = "Output/win32/ai/levels/mp_005/infantry/pathdata.hpd";
const INI: &str = "opfor.ini";
const MAPS: &str = "maps.ini";
const WAKE: &str = "Package/levels/wake_island_s";
const COMMON: &str = "Package/levels/ww2_common";

struct Game(PathBuf);

impl Drop for Game {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// (bytes, write time) of every game file outside `opfor\`.
type Files = BTreeMap<String, (Vec<u8>, u64)>;

impl Game {
    fn path(&self, rel: &str) -> PathBuf {
        rel.split('/').fold(self.0.clone(), |p, x| p.join(x))
    }
    fn put(&self, rel: &str, data: &[u8]) {
        let p = self.path(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, data).unwrap();
        // an old write time, as the game's own files have
        let f = fs::OpenOptions::new().write(true).open(&p).unwrap();
        f.set_modified(UNIX_EPOCH + Duration::from_secs(1_262_304_000 + rel.len() as u64))
            .unwrap();
    }
    fn get(&self, rel: &str) -> Vec<u8> {
        fs::read(self.path(rel)).unwrap()
    }
    fn led(&self) -> Ledger {
        Ledger::new(&self.0)
    }
    fn mgr(&self) -> Manager {
        Manager::new(&self.0)
    }
    fn files(&self) -> Files {
        let mut out = BTreeMap::new();
        let mut stack = vec![self.0.clone()];
        while let Some(d) = stack.pop() {
            for e in fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    if p != self.0.join("opfor") {
                        stack.push(p);
                    }
                } else {
                    let rel = p.strip_prefix(&self.0).unwrap().to_string_lossy().replace('\\', "/");
                    out.insert(rel, (fs::read(&p).unwrap(), mtime_ns(&p).unwrap()));
                }
            }
        }
        out
    }
    /// Both locks a killed process leaves name a pid that is gone.
    fn kill(&self) {
        for p in [
            self.0.join("opfor").join("mods").join(".lock"),
            self.0.join("opfor").join("ledger").join(".lock"),
        ] {
            if p.is_file() {
                fs::write(p, br#"{"pid": 4000000000, "started": "0"}"#).unwrap();
            }
        }
    }
    /// Installs `files` as `owner` (a stage: built on what it found); the files get fixed write times, so two
    /// fixtures are alike to the byte and the write time.
    fn stage(&self, owner: &str, files: &[(&str, &[u8])]) {
        let items: Vec<(String, Vec<u8>)> = files.iter().map(|(r, d)| ((*r).to_owned(), d.to_vec())).collect();
        self.led().install_keep(owner, &items, true, "", false).unwrap();
        self.fix_times(owner, files.iter().map(|(r, _)| *r));
    }
    fn fix_times<'a>(&self, owner: &str, rels: impl Iterator<Item = &'a str>) {
        for r in rels {
            let f = fs::OpenOptions::new().write(true).open(self.path(r)).unwrap();
            let s = 1_700_000_000 + (owner.len() * 1000 + r.len()) as u64;
            f.set_modified(UNIX_EPOCH + Duration::from_secs(s)).unwrap();
        }
    }
}

const GUID: &str = "00000000-0000-0000-0000-000000000000";
const DICE_INI: &str = "[dev]\r\nlevel=Levels/MP_005GR\r\n[patches]\r\nvietnam=1\r\n[input]\r\nprompts=pc\r\n";

/// The fake game copy with every mod installed (the order the tools install them in).
fn game(tag: &str) -> Game {
    let d = std::env::temp_dir().join(format!("bc2-mods-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    let g = Game(d);
    // DICE's files (and OPFOR's own settings file and Napisal's imported level, not installs)
    g.put(
        MFT,
        b"Signature 01\r\n\r\nBundleSource game:levels/mp_001/level-00.fbrb\r\n",
    );
    g.put(
        BM,
        format!("levels/mp_001/level\t{GUID}\t1\nlevels/ons_mp_005gr/terrain\t{GUID}\n").as_bytes(),
    );
    g.put(STARTUP, b"DICE startup bundle");
    g.put(LEVEL, b"Napisal's Valparaiso level");
    g.put(MENU, b"DICE main menu");
    g.put(HPD, b"DICE path data");
    g.put(INI, DICE_INI.as_bytes());
    g.put(MAPS, b"[MP_001]\r\nname=Panama Canal\r\n");
    g.put("Package/levels/mp_001/level-00.fbrb", b"an untouched DICE level");
    // OPFOR's import, the Onslaught port of ons_mp_002
    g.stage("import", &[(MFT, b"mft + import"), (STARTUP, b"startup + import")]);
    g.stage("linux:ons_mp_002", &[(LEVEL, b"linux server level")]);
    g.stage("cull:ons_mp_002", &[(LEVEL, b"linux + cull")]);
    g.stage("loadset:ons_mp_002", &[(MFT, b"mft + import + loadset")]);
    g.stage("loading:ons_mp_002", &[(STARTUP, b"startup + import + loading")]);
    // the Pacific War: its level folders (outside the ledger), the shared files (owner wake)
    g.put(&format!("{WAKE}/level-00.fbrb"), b"wake level");
    g.put(&format!("{WAKE}/vo/en-00.fbrb"), b"wake vo");
    let rec = format!("{{\n \"level\": \"{}\"\n}}", bc2_gamefs::sha256::hex(b"wake level"));
    g.put(&format!("{WAKE}/opfor-wake-installed.json"), rec.as_bytes());
    g.put(&format!("{COMMON}/loader-00.fbrb"), b"ww2 loader");
    g.put(&format!("{COMMON}/loader-00.fbrb.opfor-tips"), b"10 1234");
    g.put(&format!("{COMMON}/opfor-ww2common-installed.json"), b"{}");
    let ini_wake =
        DICE_INI.replace("Levels/MP_005GR", "Levels/WAKE_ISLAND_S") + "[gamemode]\r\npacific_levels=wake_island_s\r\n";
    g.stage(
        "wake",
        &[
            (MFT, b"mft + import + loadset + wake"),
            (STARTUP, b"startup + import + loading + wake"),
            (INI, ini_wake.as_bytes()),
            (
                MAPS,
                b"[MP_001]\r\nname=Panama Canal\r\n[WAKE_ISLAND_S]\r\nname=Wake Island\r\n",
            ),
        ],
    );
    // the launcher edits opfor.ini; the WW2 install's refresh restates it
    let launcher = ini_wake.replace("prompts=pc", "prompts=auto");
    fs::write(g.path(INI), launcher).unwrap();
    g.fix_times("launcher", [INI].into_iter());
    g.led()
        .restate("wake", INI, true, None, "opfor.ini edited since the WW2 install")
        .unwrap();
    // the Pacific menus and the DLL's marker beside the bundle (its size and write time as a FILETIME)
    g.stage("pacificmenu", &[(MENU, b"pacific main menu")]);
    let m = mtime_ns(&g.path(MENU)).unwrap() / 100 + 116_444_736_000_000_000;
    g.put(&format!("{MENU}.opfor-pacific-music"), format!("17 {m}").as_bytes());
    // NavKit, stock-MP on top (independent whole files)
    g.led()
        .install(
            "navkit:mp_005",
            &[(HPD.to_owned(), b"navkit build".to_vec())],
            false,
            "",
            false,
        )
        .unwrap();
    g.led()
        .install(
            "stockmp:mp_005",
            &[(HPD.to_owned(), b"stock-mp build".to_vec())],
            false,
            "",
            false,
        )
        .unwrap();
    g.fix_times("stockmp:mp_005", [HPD].into_iter());
    g
}

fn names(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

fn state(g: &Game, id: &str) -> String {
    let l = g.mgr().list().unwrap();
    l.iter()
        .find(|c| c.id == id)
        .map_or_else(|| "absent".to_owned(), |c| format!("{:?}", c.state).to_lowercase())
}

/// The differences of the bytes alone.
fn bytes_only(d: Vec<String>) -> Vec<String> {
    d.into_iter().filter(|l| !l.ends_with(": write time differs")).collect()
}

/// The differences less the write times of `files` (a setting rewrote them).
fn loose(d: Vec<String>, files: &[&str]) -> Vec<String> {
    d.into_iter()
        .filter(|l| !files.iter().any(|f| *l == format!("{f}: write time differs")))
        .collect()
}

fn diff(a: &Files, b: &Files) -> Vec<String> {
    let mut out = Vec::new();
    for (k, v) in a {
        match b.get(k) {
            None => out.push(format!("{k}: gone")),
            Some(w) if w.0 != v.0 => out.push(format!("{k}: bytes differ")),
            Some(w) if w.1 != v.1 => out.push(format!("{k}: write time differs")),
            _ => {}
        }
    }
    for k in b.keys() {
        if !a.contains_key(k) {
            out.push(format!("{k}: new"));
        }
    }
    out
}

#[test]
fn the_list_names_every_mod() {
    let g = game("list");
    let l = g.mgr().list().unwrap();
    let ids: Vec<&str> = l.iter().map(|c| c.id.as_str()).collect();
    for want in [
        "base",
        "navkit:mp_005",
        "onslaught:ons_mp_002",
        "pacific-menus",
        "stockmp:mp_005",
        "vietnam",
        "ww2",
    ] {
        assert!(ids.contains(&want), "{want} in {ids:?}");
    }
    assert!(l.iter().all(|c| format!("{:?}", c.state) == "Enabled"), "{l:?}");
    let ww2 = l.iter().find(|c| c.id == "ww2").unwrap();
    assert!(ww2.installed.iter().any(|u| u.contains(WAKE)), "{ww2:?}");
}

#[test]
fn disable_and_enable_ww2_round_trip_byte_for_byte() {
    let g = game("ww2");
    let before = g.files();
    // the Pacific menus need the Pacific War: a confirmation first, nothing written
    let e = g.mgr().disable(&names(&["ww2"]), false).unwrap_err();
    assert!(matches!(e, ModError::Confirm(_)), "{e}");
    assert!(e.to_string().contains("pacific-menus"), "{e}");
    assert_eq!(diff(&before, &g.files()), Vec::<String>::new());
    let lines = g.mgr().disable(&names(&["ww2"]), true).unwrap();
    assert!(
        lines.iter().any(|l| l.contains("disabled: pacific-menus, ww2")),
        "{lines:?}"
    );
    assert_eq!(state(&g, "ww2"), "disabled");
    assert_eq!(state(&g, "pacific-menus"), "disabled");
    // each shared file is what the layer under wake wrote; the folders are gone; the menu is DICE's
    assert_eq!(g.get(MFT), b"mft + import + loadset");
    assert_eq!(g.get(STARTUP), b"startup + import + loading");
    assert_eq!(g.get(MENU), b"DICE main menu");
    assert!(!g.path(WAKE).exists() && !g.path(COMMON).exists());
    // opfor.ini: wake's own keys back, the launcher's edit made after the install kept
    let ini = String::from_utf8(g.get(INI)).unwrap();
    assert!(
        ini.contains("level=Levels/MP_005GR") && !ini.contains("pacific_levels"),
        "{ini}"
    );
    assert!(ini.contains("prompts=auto"), "{ini}");
    // enable brings back the menus disabled with it, every file and write time as before
    let lines = g.mgr().enable(&names(&["ww2"]), false, false).unwrap();
    assert!(
        lines.iter().any(|l| l.contains("enabled: pacific-menus, ww2")),
        "{lines:?}"
    );
    assert_eq!(diff(&before, &g.files()), Vec::<String>::new());
    assert_eq!(state(&g, "ww2"), "enabled");
    // and again: disable / enable / disable / enable
    g.mgr().disable(&names(&["ww2"]), true).unwrap();
    let off = g.files();
    g.mgr().enable(&names(&["ww2"]), false, false).unwrap();
    g.mgr().disable(&names(&["ww2"]), true).unwrap();
    // the files put back are written anew (the ledger keeps no write time of a prior): their bytes are the same
    assert_eq!(bytes_only(diff(&off, &g.files())), Vec::<String>::new());
    g.mgr().enable(&names(&["ww2"]), false, false).unwrap();
    assert_eq!(diff(&before, &g.files()), Vec::<String>::new());
    let v = g.mgr().verify().unwrap();
    assert_eq!(v.problems, 0, "{:#?}", v.lines);
}

#[test]
fn disabling_onslaught_takes_what_was_built_on_it_first() {
    let g = game("ons");
    let before = g.files();
    let e = g.mgr().disable(&names(&["onslaught:ons_mp_002"]), false).unwrap_err();
    let t = e.to_string();
    assert!(t.contains("ww2") && t.contains("pacific-menus"), "{t}");
    g.mgr().disable(&names(&["onslaught:ons_mp_002"]), true).unwrap();
    assert_eq!(g.get(MFT), b"mft + import");
    assert_eq!(g.get(STARTUP), b"startup + import");
    assert_eq!(g.get(LEVEL), b"Napisal's Valparaiso level");
    // the Pacific War cannot come back alone: its files were built on the Onslaught port's
    let e = g.mgr().enable(&names(&["ww2"]), true, false).unwrap_err();
    assert!(matches!(e, ModError::Refused(_)), "{e}");
    let off = g.files();
    assert_eq!(diff(&off, &g.files()), Vec::<String>::new());
    // enabling the port brings back everything disabled with it
    g.mgr().enable(&names(&["onslaught"]), false, false).unwrap();
    assert_eq!(diff(&before, &g.files()), Vec::<String>::new());
}

#[test]
fn restore_original_all_gives_dices_files_and_enable_gives_the_mods_back() {
    let g = game("orig");
    let before = g.files();
    let e = g.mgr().restore_original(&[], true, false).unwrap_err();
    assert!(matches!(e, ModError::Confirm(_)), "{e}");
    g.mgr().restore_original(&[], true, true).unwrap();
    // every ledger file is DICE's (the bottom layer's prior), byte for byte
    assert_eq!(
        g.get(MFT),
        b"Signature 01\r\n\r\nBundleSource game:levels/mp_001/level-00.fbrb\r\n"
    );
    assert_eq!(g.get(STARTUP), b"DICE startup bundle");
    assert_eq!(g.get(LEVEL), b"Napisal's Valparaiso level");
    assert_eq!(g.get(MENU), b"DICE main menu");
    assert_eq!(g.get(HPD), b"DICE path data");
    assert_eq!(g.get(MAPS), b"[MP_001]\r\nname=Panama Canal\r\n");
    assert!(g.led().records().unwrap().is_empty());
    // the Vietnam setting is OPFOR's, not an install: untouched
    assert!(String::from_utf8(g.get(INI)).unwrap().contains("vietnam=1"));
    for m in ["base", "onslaught", "ww2", "navkit:mp_005", "stockmp:mp_005"] {
        g.mgr().enable(&names(&[m]), false, true).unwrap();
    }
    assert_eq!(diff(&before, &g.files()), Vec::<String>::new());
}

#[test]
fn restore_original_of_one_file_keeps_the_owners_other_files() {
    let g = game("one");
    let before = g.files();
    g.mgr().restore_original(&names(&[MFT]), false, true).unwrap();
    assert_eq!(
        g.get(MFT),
        b"Signature 01\r\n\r\nBundleSource game:levels/mp_001/level-00.fbrb\r\n"
    );
    assert_eq!(g.get(STARTUP), b"startup + import + loading + wake");
    assert_eq!(state(&g, "ww2"), "partly");
    assert_eq!(state(&g, "base"), "partly");
    // wake's package.mft cannot come back before the layers it was built on
    assert!(g.mgr().enable(&names(&["ww2"]), true, true).is_err());
    g.mgr().enable(&names(&["base"]), true, true).unwrap();
    g.mgr().enable(&names(&["onslaught"]), true, true).unwrap();
    g.mgr().enable(&names(&["ww2"]), true, true).unwrap();
    assert_eq!(diff(&before, &g.files()), Vec::<String>::new());
}

#[test]
fn vietnam_is_set_through_every_layer_and_kept_copy() {
    let g = game("nam");
    let before = g.files();
    g.mgr().disable(&names(&["vietnam"]), true).unwrap();
    assert!(String::from_utf8(g.get(INI)).unwrap().contains("vietnam=0"));
    assert_eq!(state(&g, "vietnam"), "disabled");
    assert_eq!(state(&g, "pacific-menus"), "disabled");
    // wake's revert does not bring vietnam=1 back: every state the ledger keeps has the setting
    g.mgr().disable(&names(&["ww2"]), true).unwrap();
    assert!(String::from_utf8(g.get(INI)).unwrap().contains("vietnam=0"));
    g.mgr().enable(&names(&["vietnam"]), true, false).unwrap();
    g.mgr().enable(&names(&["ww2"]), false, true).unwrap();
    g.mgr().enable(&names(&["pacific-menus"]), false, true).unwrap();
    assert_eq!(state(&g, "pacific-menus"), "enabled");
    // every byte as before; opfor.ini was written by the setting, so its write time is new
    assert_eq!(loose(diff(&before, &g.files()), &[INI]), Vec::<String>::new());
    // a missing key is on (the DLL's default)
    let t = String::from_utf8(g.get(INI)).unwrap().replace("vietnam=1\r\n", "");
    fs::write(g.path(INI), t).unwrap();
    assert_eq!(state(&g, "vietnam"), "enabled");
}

#[test]
fn a_settings_edit_made_while_a_mod_is_out_is_carried_onto_it() {
    let g = game("rebase");
    g.mgr().disable(&names(&["ww2"]), true).unwrap();
    let t = String::from_utf8(g.get(INI))
        .unwrap()
        .replace("prompts=auto", "prompts=pad");
    fs::write(g.path(INI), t).unwrap();
    g.mgr().enable(&names(&["ww2"]), false, false).unwrap();
    let ini = String::from_utf8(g.get(INI)).unwrap();
    assert!(
        ini.contains("level=Levels/WAKE_ISLAND_S") && ini.contains("prompts=pad"),
        "{ini}"
    );
    // and a bundle changed while it was out is refused in words, nothing written
    g.mgr().disable(&names(&["ww2"]), true).unwrap();
    fs::write(g.path(STARTUP), b"someone else's startup").unwrap();
    let files = g.files();
    let e = g.mgr().enable(&names(&["ww2"]), false, false).unwrap_err();
    assert!(e.to_string().contains("startup-00.fbrb"), "{e}");
    assert_eq!(diff(&files, &g.files()), Vec::<String>::new());
}

#[test]
fn uninstall_drops_the_kept_copy_and_keeps_the_manifest_repair() {
    let g = game("uninst");
    let e = g.mgr().uninstall(&names(&["ww2"]), false).unwrap_err();
    assert!(matches!(e, ModError::Confirm(_)), "{e}");
    g.mgr().uninstall(&names(&["ww2"]), true).unwrap();
    assert_eq!(state(&g, "ww2"), "absent");
    assert_eq!(state(&g, "pacific-menus"), "disabled");
    assert!(!g.path(WAKE).exists());
    assert!(g.led().holders(BM).unwrap().contains(&"manifestfix".to_owned()));
    assert!(
        String::from_utf8(g.get(BM))
            .unwrap()
            .contains(&format!("levels/ons_mp_005gr/terrain\t{GUID}\t1"))
    );
    let v = g.mgr().verify().unwrap();
    assert_eq!(v.problems, 0, "{:#?}", v.lines);
    assert!(!g.0.join("opfor/mods/folders").join("Package").exists());
}

#[test]
fn verify_finds_a_changed_file_and_a_damaged_kept_copy() {
    let g = game("verify");
    assert_eq!(g.mgr().verify().unwrap().problems, 0);
    g.mgr().disable(&names(&["navkit:mp_005"]), true).unwrap();
    let b =
        g.0.join("opfor/mods/blobs")
            .join(format!("{}.bin", bc2_gamefs::sha256::hex(b"navkit build")));
    fs::remove_file(&b).unwrap();
    fs::write(&b, b"damaged").unwrap();
    fs::write(g.path(LEVEL), b"changed outside").unwrap();
    let v = g.mgr().verify().unwrap();
    assert_eq!(v.problems, 2, "{:#?}", v.lines);
    assert!(
        v.lines
            .iter()
            .any(|l| l.starts_with("FAILED") && l.contains("level-00.fbrb"))
    );
    assert!(
        v.lines
            .iter()
            .any(|l| l.starts_with("FAILED") && l.contains("kept navkit:mp_005"))
    );
}

/// One operation interrupted at its k-th write step (a "kill": the journal and both locks left), then the next
/// command finishes it: the files end as an uninterrupted run leaves them, and enabling gives the start back.
fn killed_at_every_step(
    tag: &str,
    op: &dyn Fn(&Manager) -> Result<Vec<String>, ModError>,
    back: &dyn Fn(&Manager),
    rewritten: &[&str],
) {
    let g = game(tag);
    let before = g.files();
    op(&g.mgr()).unwrap();
    let after = g.files();
    back(&g.mgr());
    assert_eq!(
        loose(diff(&before, &g.files()), rewritten),
        Vec::<String>::new(),
        "{tag}: the uninterrupted round trip"
    );
    let mut k = 1u32;
    loop {
        let g = game(tag);
        let mut m = g.mgr();
        let n = Rc::new(Cell::new(0u32));
        let hit = Rc::new(Cell::new(false));
        {
            let (n, hit) = (n.clone(), hit.clone());
            m.set_hook(Some(Box::new(move |_label: &str| {
                n.set(n.get() + 1);
                if n.get() == k {
                    hit.set(true);
                    return Err(Interrupt::Crash);
                }
                Ok(())
            })));
        }
        let r = op(&m);
        if !hit.get() {
            r.unwrap();
            assert!(k > 3, "{tag}: too few steps ({k})");
            break;
        }
        assert!(matches!(r, Err(ModError::Crashed)), "{tag} step {k}: {r:?}");
        g.kill();
        // the next command: finishes the interrupted operation (or finds none was begun), then runs again
        g.mgr().resume().unwrap();
        let _ = op(&g.mgr()).unwrap();
        // the files the operation wrote have this run's write times: their bytes are compared
        assert_eq!(
            bytes_only(diff(&after, &g.files())),
            Vec::<String>::new(),
            "{tag}: killed at step {k}"
        );
        back(&g.mgr());
        assert_eq!(
            loose(diff(&before, &g.files()), rewritten),
            Vec::<String>::new(),
            "{tag}: killed at step {k}, back"
        );
        assert!(!g.0.join("opfor/mods/.lock").exists() && !g.0.join("opfor/ledger/.lock").exists());
        k += 1;
    }
}

#[test]
fn a_kill_at_every_step_of_a_disable_is_finished() {
    killed_at_every_step(
        "kill-dis",
        &|m| m.disable(&names(&["ww2"]), true),
        &|m| {
            m.enable(&names(&["ww2"]), false, true).unwrap();
        },
        &[],
    );
}

#[test]
fn a_kill_at_every_step_of_an_enable_is_finished() {
    // the disable and the enable after it, a kill anywhere in either
    killed_at_every_step(
        "kill-en",
        &|m| {
            if m.list()?
                .iter()
                .any(|c| c.id == "ww2" && format!("{:?}", c.state) == "Enabled")
            {
                m.disable(&names(&["onslaught"]), true)?;
            }
            m.enable(&names(&["onslaught"]), false, true)
        },
        &|_| {},
        &[],
    );
}

#[test]
fn a_kill_at_every_step_of_restore_original_is_finished() {
    killed_at_every_step(
        "kill-orig",
        &|m| m.restore_original(&[], true, true),
        &|m| {
            for x in ["base", "onslaught", "ww2", "navkit:mp_005", "stockmp:mp_005"] {
                m.enable(&names(&[x]), false, true).unwrap();
            }
        },
        &[],
    );
}

#[test]
fn a_kill_at_every_step_of_a_setting_is_finished() {
    killed_at_every_step(
        "kill-nam",
        &|m| m.disable(&names(&["vietnam", "ww2"]), true),
        &|m| {
            m.enable(&names(&["vietnam", "ww2"]), false, true).unwrap();
        },
        &[INI],
    );
}

#[test]
fn a_held_lock_refuses_and_a_dead_ones_is_cleared() {
    let g = game("lock");
    let root = g.0.join("opfor/mods");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join(".lock"), format!("{{\"pid\": {}}}", std::process::id())).unwrap();
    assert!(matches!(
        g.mgr().disable(&names(&["ww2"]), true),
        Err(ModError::Locked(_))
    ));
    fs::write(root.join(".lock"), br#"{"pid": 4000000000}"#).unwrap();
    g.mgr().disable(&names(&["ww2"]), true).unwrap();
}

// ------------------------------------------------------------------------------------------------- migration

/// A fake game copy with the markers of before the ledger: package.mft imported, then the Onslaught loadset, then
/// the WW2 install (`.opfor-last` naming wake), NavKit's `.opfor-new` on path data that had none.
fn legacy(tag: &str) -> Game {
    let d = std::env::temp_dir().join(format!("bc2-mods-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    let g = Game(d);
    let t = |s: u64| UNIX_EPOCH + Duration::from_secs(1_700_000_000 + s);
    let put_at = |rel: &str, data: &[u8], at: u64| {
        let p = g.path(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, data).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&p)
            .unwrap()
            .set_modified(t(at))
            .unwrap();
    };
    put_at(&format!("{MFT}.opfor-orig"), b"DICE mft", 1);
    put_at(&format!("{MFT}.opfor-loadset-orig"), b"mft + import", 2);
    put_at(&format!("{MFT}.opfor-wake-orig"), b"mft + import + loadset", 3);
    put_at(MFT, b"mft + import + loadset + wake", 4);
    let sha = bc2_gamefs::sha256::hex(b"mft + import + loadset + wake");
    put_at(&format!("{MFT}.opfor-last"), format!("wake {sha}").as_bytes(), 4);
    put_at(&format!("{MFT}.opfor-loadset-applied"), b"whatever", 2);
    put_at(HPD, b"navkit build", 5);
    put_at(&format!("{HPD}.opfor-new"), b"", 5);
    put_at(&format!("{STARTUP}.opfor-loading-orig"), b"startup before", 6);
    put_at(STARTUP, b"startup", 7);
    put_at(&format!("{STARTUP}.opfor-last"), b"loading 00", 7);
    g
}

#[test]
fn migrate_plans_applies_resumes_and_reverts_to_the_backups() {
    let g = legacy("migrate");
    let fps = bc2_install::mods::migrate::plan(&g.0).unwrap();
    let lines = bc2_install::mods::migrate::show(&fps);
    assert!(
        lines
            .iter()
            .any(|l| l.contains("migrate  Dist/win32/package.mft") && l.contains("import -> loadset -> wake")),
        "{lines:#?}"
    );
    // the startup bundle's .opfor-last names a sha the file is not: refused, left as it is
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("REFUSED Package/async/startup-00.fbrb")),
        "{lines:#?}"
    );
    assert!(g.led().records().unwrap().is_empty(), "the plan writes nothing");
    let (moved, refused, _) = bc2_install::mods::migrate::apply(&g.mgr()).unwrap();
    assert_eq!((moved, refused), (2, 1));
    assert!(
        g.path(&format!("{STARTUP}.opfor-last")).is_file(),
        "a refused file keeps its markers"
    );
    assert!(!g.path(&format!("{MFT}.opfor-orig")).exists());
    assert!(
        g.0.join("opfor/legacy-markers/Dist/win32/package.mft.opfor-wake-orig")
            .is_file()
    );
    assert_eq!(g.led().holders(MFT).unwrap(), names(&["import", "loadset", "wake"]));
    assert_eq!(g.led().holders(HPD).unwrap(), names(&["navkit:mp_005"]));
    // again: nothing more to do
    let (moved2, _, _) = bc2_install::mods::migrate::apply(&g.mgr()).unwrap();
    assert_eq!(moved2, 0);
    // the owners reverted top-down give back each old backup
    g.led().uninstall("wake", false).unwrap();
    assert_eq!(g.get(MFT), b"mft + import + loadset");
    g.led().uninstall("loadset", false).unwrap();
    assert_eq!(g.get(MFT), b"mft + import");
    g.led().uninstall("import", false).unwrap();
    assert_eq!(g.get(MFT), b"DICE mft");
    g.led().uninstall("navkit:mp_005", false).unwrap();
    assert!(!g.path(HPD).exists());
}

#[test]
fn a_kill_at_every_step_of_the_migration_is_finished() {
    let mut k = 1u32;
    loop {
        let g = legacy("migrate-kill");
        let mut m = g.mgr();
        let n = Rc::new(Cell::new(0u32));
        let hit = Rc::new(Cell::new(false));
        {
            let (n, hit) = (n.clone(), hit.clone());
            m.set_hook(Some(Box::new(move |_label: &str| {
                n.set(n.get() + 1);
                if n.get() == k {
                    hit.set(true);
                    return Err(Interrupt::Crash);
                }
                Ok(())
            })));
        }
        let r = bc2_install::mods::migrate::apply(&m);
        if !hit.get() {
            r.unwrap();
            assert!(k > 3);
            break;
        }
        g.kill();
        bc2_install::mods::migrate::apply(&g.mgr()).unwrap();
        assert_eq!(
            g.led().holders(MFT).unwrap(),
            names(&["import", "loadset", "wake"]),
            "step {k}"
        );
        assert_eq!(
            bc2_install::mods::migrate::scan(&g.0).len(),
            1,
            "step {k}: only the refused file"
        );
        for o in ["wake", "loadset", "import"] {
            g.led().uninstall(o, false).unwrap();
        }
        assert_eq!(g.get(MFT), b"DICE mft", "step {k}");
        k += 1;
    }
}
