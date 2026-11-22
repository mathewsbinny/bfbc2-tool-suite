//! `bfbc2 mods` as a process (round MM1): on a fake game copy, the command lines' exit codes and lines, and a real
//! kill (`BC2_MODS_CRASH_AT=<n>`: the process ends before its n-th write step) at every step of a disable and of an
//! enable, each finished by `bfbc2 mods resume`, every game file compared byte for byte.
#![cfg(feature = "mods")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    missing_docs
)]

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

use bc2_gamefs::ledger::Ledger;

const EXE: &str = env!("CARGO_BIN_EXE_bfbc2");
const MFT: &str = "Dist/win32/package.mft";
const MENU: &str = "Package/menu/main/mainmenu-00.fbrb";
const WAKE: &str = "Package/levels/wake_island_s";

struct Game(PathBuf);

impl Drop for Game {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl Game {
    fn path(&self, rel: &str) -> PathBuf {
        rel.split('/').fold(self.0.clone(), |p, x| p.join(x))
    }
    fn put(&self, rel: &str, data: &[u8]) {
        let p = self.path(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, data).unwrap();
    }
    fn files(&self) -> BTreeMap<String, Vec<u8>> {
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
                    out.insert(rel, fs::read(&p).unwrap());
                }
            }
        }
        out
    }
    fn run(&self, args: &[&str], crash: Option<u32>) -> Output {
        let mut c = Command::new(EXE);
        c.arg("mods").args(args).arg("--game").arg(&self.0);
        c.env_remove("BC2_MODS_CRASH_AT").env_remove("BC2_LEDGER_CRASH_AT");
        if let Some(n) = crash {
            c.env("BC2_MODS_CRASH_AT", n.to_string());
        }
        c.output().unwrap()
    }
}

fn game(tag: &str) -> Game {
    let d = std::env::temp_dir().join(format!("bc2_mods_cli_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    let g = Game(d);
    g.put(MFT, b"DICE mft");
    g.put(MENU, b"DICE menu");
    g.put(
        "opfor.ini",
        b"[dev]\r\nlevel=Levels/MP_005GR\r\n[patches]\r\nvietnam=1\r\n",
    );
    g.put(&format!("{WAKE}/level-00.fbrb"), b"wake level");
    g.put(&format!("{WAKE}/opfor-wake-installed.json"), b"{}");
    let led = |o: &str, f: &str, d: &[u8]| {
        Ledger::new(&g.0)
            .install_keep(o, &[(f.to_owned(), d.to_vec())], true, "", false)
            .unwrap();
    };
    led("import", MFT, b"mft + import");
    led("wake", MFT, b"mft + import + wake");
    led("pacificmenu", MENU, b"pacific menu");
    g
}

fn text(o: &Output) -> String {
    format!(
        "exit {:?}\n{}{}",
        o.status.code(),
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

#[test]
fn the_command_lines_and_their_exit_codes() {
    let g = game("lines");
    let o = g.run(&["list"], None);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let t = text(&o);
    assert!(
        t.contains("ww2") && t.contains("enabled") && t.contains("vietnam"),
        "{t}"
    );
    // a confirmation: exit 2, nothing written
    let before = g.files();
    let o = g.run(&["disable", "ww2"], None);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o));
    assert!(text(&o).contains("--yes"), "{}", text(&o));
    assert_eq!(g.files(), before);
    let o = g.run(&["disable", "ww2", "--yes"], None);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert_eq!(fs::read(g.path(MFT)).unwrap(), b"mft + import");
    let o = g.run(&["list", "--json"], None);
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert!(
        v["mods"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == "ww2" && m["state"] == "disabled")
    );
    let o = g.run(&["install", "ww2"], None);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert_eq!(g.files(), before);
    let o = g.run(&["verify"], None);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let o = g.run(&["disable", "nosuchmod"], None);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    let o = g.run(&["migrate"], None);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(text(&o).contains("0 file(s) with old markers"), "{}", text(&o));
}

/// `args` killed before each write step in turn, then `bfbc2 mods resume` and the command again: the files end as an
/// uninterrupted run leaves them.
fn killed(tag: &str, prepare: &[&[&str]], args: &[&str]) {
    let g = game(tag);
    for p in prepare {
        assert_eq!(g.run(p, None).status.code(), Some(0));
    }
    assert_eq!(g.run(args, None).status.code(), Some(0));
    let want = g.files();
    drop(g);
    for k in 1.. {
        let g = game(tag);
        for p in prepare {
            assert_eq!(g.run(p, None).status.code(), Some(0));
        }
        let o = g.run(args, Some(k));
        if o.status.code() == Some(0) {
            assert!(k > 3, "{tag}: {k} steps");
            return;
        }
        assert_eq!(o.status.code(), Some(9), "{tag} step {k}: {}", text(&o));
        let r = g.run(&["resume"], None);
        assert_eq!(r.status.code(), Some(0), "{tag} step {k}: {}", text(&r));
        let again = g.run(args, None);
        assert_eq!(again.status.code(), Some(0), "{tag} step {k}: {}", text(&again));
        assert_eq!(g.files(), want, "{tag}: killed before step {k}");
        assert!(!g.0.join("opfor/mods/.lock").exists() && !g.0.join("opfor/ledger/.lock").exists());
    }
}

#[test]
fn a_real_kill_at_every_step_of_a_disable() {
    killed("kill-dis", &[], &["disable", "ww2", "--yes"]);
}

#[test]
fn a_real_kill_at_every_step_of_an_enable() {
    killed("kill-en", &[&["disable", "ww2", "--yes"]], &["enable", "ww2"]);
}
