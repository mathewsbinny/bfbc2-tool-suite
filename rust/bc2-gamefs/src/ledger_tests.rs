//! The install ledger's hazard sequences (round DF2): AU4-DS-1's sequence, every order of two tools, an interruption
//! at every write step, legacy adoption and other owners' records kept.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::assert_is_empty,
    reason = "tests"
)]

use std::cell::Cell;
use std::rc::Rc;

use super::*;

const HPD: &str = "Output/win32/ai/levels/mp_005/infantry/pathdata.hpd";
const DICE: &[u8] = b"DICE's own path data";
const NAV: &[u8] = b"NavKit's build";
const SMP: &[u8] = b"stock-MP's build";

struct Game(PathBuf);

impl Game {
    fn new(tag: &str) -> Self {
        let d = std::env::temp_dir().join(format!("bc2_gameledger_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let g = Self(d);
        g.put(HPD, Some(DICE));
        g
    }
    fn led(&self) -> Ledger {
        Ledger::new(&self.0)
    }
    fn path(&self, rel: &str) -> PathBuf {
        rel.split('/').fold(self.0.clone(), |p, x| p.join(x))
    }
    fn put(&self, rel: &str, data: Option<&[u8]>) {
        let p = self.path(rel);
        match data {
            Some(d) => {
                fs::create_dir_all(p.parent().unwrap()).unwrap();
                fs::write(p, d).unwrap();
            }
            None => {
                let _ = fs::remove_file(p);
            }
        }
    }
    fn get(&self, rel: &str) -> Option<Vec<u8>> {
        fs::read(self.path(rel)).ok()
    }
    /// Every file of the game copy outside the ledger store, with its bytes.
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
    /// The ledger store's records and blobs, with their bytes (the lock and journal must be gone).
    fn store(&self) -> BTreeMap<String, Vec<u8>> {
        let root = default_root(&self.0);
        let mut out = BTreeMap::new();
        for sub in [root.clone(), root.join("blobs")] {
            if let Ok(rd) = fs::read_dir(&sub) {
                for e in rd.flatten() {
                    if e.path().is_file() {
                        out.insert(
                            e.file_name().to_string_lossy().into_owned(),
                            fs::read(e.path()).unwrap(),
                        );
                    }
                }
            }
        }
        out
    }
    /// The lock left by a "killed" process names a pid that is gone.
    fn kill(&self) {
        let p = default_root(&self.0).join(".lock");
        assert!(p.is_file(), "a crash leaves the lock");
        fs::write(p, br#"{"pid": 4000000000, "started": "2026-11-21T00:00:00Z"}"#).unwrap();
    }
}

impl Drop for Game {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn nav(g: &Game) -> Result<Plan, LedgerError> {
    g.led()
        .install("navkit:mp_005", &[(HPD.to_owned(), NAV.to_vec())], false, "", false)
}

fn smp(g: &Game) -> Result<Plan, LedgerError> {
    g.led()
        .install("stockmp:mp_005", &[(HPD.to_owned(), SMP.to_vec())], false, "", false)
}

fn original(g: &Game) -> Option<Vec<u8>> {
    g.led().original(HPD).unwrap().map(|p| fs::read(p).unwrap())
}

#[test]
fn ds1_sequence_keeps_dices_file() {
    let g = Game::new("ds1");
    nav(&g).unwrap();
    assert_eq!(g.get(HPD).as_deref(), Some(NAV));
    smp(&g).unwrap();
    assert_eq!(g.get(HPD).as_deref(), Some(SMP));
    assert_eq!(original(&g).as_deref(), Some(DICE));
    // NavKit's restore with stock-MP on top: spliced out, the file stays stock-MP's
    let p = g.led().uninstall("navkit:mp_005", false).unwrap();
    assert!(
        p.notes
            .iter()
            .any(|n| n.contains("taken out from under stockmp:mp_005")),
        "{p:?}"
    );
    assert_eq!(g.get(HPD).as_deref(), Some(SMP));
    assert_eq!(original(&g).as_deref(), Some(DICE));
    // stock-MP's revert puts DICE's file back (NavKit's build is nowhere)
    g.led().uninstall("stockmp:mp_005", false).unwrap();
    assert_eq!(g.get(HPD).as_deref(), Some(DICE));
    assert_eq!(original(&g).as_deref(), Some(DICE));
    assert!(g.led().records().unwrap().is_empty());
    assert_eq!(
        g.store().keys().filter(|k| k.ends_with(".bin")).count(),
        0,
        "unused blobs deleted"
    );
}

#[test]
fn every_order_of_two_tools() {
    type Op = (&'static str, bool);
    let installs: [[Op; 2]; 2] = [
        [("navkit:mp_005", true), ("stockmp:mp_005", true)],
        [("stockmp:mp_005", true), ("navkit:mp_005", true)],
    ];
    for ins in installs {
        for out in [[ins[0].0, ins[1].0], [ins[1].0, ins[0].0]] {
            for no_original in [false, true] {
                let g = Game::new("orders");
                if no_original {
                    g.put(HPD, None);
                }
                let base = g.get(HPD);
                let bytes = |o: &str| if o.starts_with("navkit") { NAV } else { SMP };
                let mut stack: Vec<&str> = Vec::new();
                for (o, _) in ins {
                    g.led()
                        .install(o, &[(HPD.to_owned(), bytes(o).to_vec())], false, "", false)
                        .unwrap();
                    stack.push(o);
                    assert_eq!(g.get(HPD).as_deref(), Some(bytes(o)));
                    assert_eq!(original(&g), base, "{ins:?}");
                }
                for o in out {
                    g.led().uninstall(o, false).unwrap();
                    stack.retain(|x| *x != o);
                    let want = stack.last().map(|t| bytes(t).to_vec()).or_else(|| base.clone());
                    assert_eq!(g.get(HPD), want, "{ins:?} then out {out:?}");
                    assert_eq!(original(&g), base);
                }
                assert!(g.led().records().unwrap().is_empty());
            }
        }
    }
}

#[test]
fn a_redo_keeps_its_prior_and_a_buried_redo_is_refused() {
    let g = Game::new("redo");
    nav(&g).unwrap();
    g.led()
        .install(
            "navkit:mp_005",
            &[(HPD.to_owned(), b"NavKit v2".to_vec())],
            false,
            "",
            false,
        )
        .unwrap();
    assert_eq!(original(&g).as_deref(), Some(DICE));
    smp(&g).unwrap();
    let e = g
        .led()
        .install(
            "navkit:mp_005",
            &[(HPD.to_owned(), b"NavKit v3".to_vec())],
            false,
            "",
            false,
        )
        .unwrap_err();
    assert!(
        e.to_string()
            .contains("stockmp:mp_005 was installed after navkit:mp_005: revert stockmp:mp_005 first"),
        "{e}"
    );
    assert_eq!(g.get(HPD).as_deref(), Some(SMP));
}

#[test]
fn a_derived_layer_refuses_its_base_leaving() {
    let g = Game::new("derived");
    nav(&g).unwrap();
    g.led()
        .install(
            "cull:mp_005",
            &[(HPD.to_owned(), b"cull on navkit".to_vec())],
            true,
            "",
            false,
        )
        .unwrap();
    let e = g.led().uninstall("navkit:mp_005", false).unwrap_err();
    assert!(
        e.to_string()
            .contains("cull:mp_005 was built on navkit:mp_005's file: revert cull:mp_005 first"),
        "{e}"
    );
    g.led().uninstall("cull:mp_005", false).unwrap();
    g.led().uninstall("navkit:mp_005", false).unwrap();
    assert_eq!(g.get(HPD).as_deref(), Some(DICE));
}

#[test]
fn a_file_changed_outside_is_refused() {
    let g = Game::new("outside");
    nav(&g).unwrap();
    g.put(HPD, Some(b"someone else"));
    let e = smp(&g).unwrap_err();
    assert!(
        e.to_string()
            .contains("changed outside the ledger since navkit:mp_005 installed it"),
        "{e}"
    );
    let e = g.led().uninstall("navkit:mp_005", false).unwrap_err();
    assert!(e.to_string().contains("not reverted"), "{e}");
    assert_eq!(g.get(HPD).as_deref(), Some(&b"someone else"[..]));
}

/// The operations of a sequence, each run with an interruption at its k-th write step: a failed write rolls back at
/// once; a "kill" leaves the journal and the lock, and the next operation rolls it back first. Either way the game
/// copy and the store are as before the interrupted operation, and running it again completes it.
#[test]
fn an_interruption_at_every_write_step() {
    type Step = fn(&Game) -> Result<Plan, LedgerError>;
    let seq: [Step; 4] = [
        nav,
        smp,
        |g| g.led().uninstall("navkit:mp_005", false),
        |g| g.led().uninstall("stockmp:mp_005", false),
    ];
    for crash in [false, true] {
        for i in 0..seq.len() {
            for k in 1..=8u32 {
                let g = Game::new("interrupt");
                g.put("Dist/other.fbrb", Some(b"untouched"));
                for s in seq.iter().take(i) {
                    s(&g).unwrap();
                }
                let (files, store) = (g.files(), g.store());
                let n = Rc::new(Cell::new(0u32));
                let hit = Rc::new(Cell::new(false));
                let mut led = g.led();
                {
                    let (n, hit) = (n.clone(), hit.clone());
                    led.set_hook(Some(Box::new(move |_label| {
                        n.set(n.get() + 1);
                        if n.get() == k {
                            hit.set(true);
                            return Err(if crash {
                                Interrupt::Crash
                            } else {
                                Interrupt::Fail(io::Error::other("injected"))
                            });
                        }
                        Ok(())
                    })));
                }
                // run the i-th operation through the hooked ledger
                let r = match i {
                    0 => led.install("navkit:mp_005", &[(HPD.to_owned(), NAV.to_vec())], false, "", false),
                    1 => led.install("stockmp:mp_005", &[(HPD.to_owned(), SMP.to_vec())], false, "", false),
                    2 => led.uninstall("navkit:mp_005", false),
                    _ => led.uninstall("stockmp:mp_005", false),
                };
                if !hit.get() {
                    r.unwrap();
                    continue; // fewer write steps than k
                }
                assert!(r.is_err());
                if crash {
                    g.kill();
                    let journal = default_root(&g.0).join(".txn.json").is_file();
                    // the next operation (here a recover alone) rolls the interrupted commit back
                    let notes = g.led().recover(false).unwrap();
                    assert!(notes.iter().any(|n| n.contains("stale lock")), "{notes:?}");
                    assert_eq!(journal, notes.iter().any(|n| n.contains("rolled back")), "{notes:?}");
                }
                assert_eq!(g.files(), files, "op {i} step {k} crash {crash}");
                assert_eq!(
                    g.store()
                        .into_iter()
                        .filter(|(f, _)| !f.ends_with(".bin"))
                        .collect::<Vec<_>>(),
                    store
                        .into_iter()
                        .filter(|(f, _)| !f.ends_with(".bin"))
                        .collect::<Vec<_>>()
                );
                seq[i](&g).unwrap();
                for s in seq.iter().skip(i + 1) {
                    s(&g).unwrap();
                }
                assert_eq!(g.get(HPD).as_deref(), Some(DICE), "op {i} step {k} crash {crash}");
                assert_eq!(g.get("Dist/other.fbrb").as_deref(), Some(&b"untouched"[..]));
            }
        }
    }
}

#[test]
fn a_held_lock_refuses_and_a_stale_one_is_cleared() {
    let g = Game::new("lock");
    let root = default_root(&g.0);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join(".lock"), format!("{{\"pid\": {}}}", std::process::id())).unwrap();
    assert!(matches!(nav(&g), Err(LedgerError::Locked(_))));
    fs::write(root.join(".lock"), br#"{"pid": 4000000000}"#).unwrap();
    let p = nav(&g).unwrap();
    assert!(p.notes.iter().any(|n| n.contains("stale lock")), "{p:?}");
    assert!(!root.join(".lock").exists());
}

#[test]
fn legacy_markers_are_adopted() {
    let g = Game::new("legacy");
    g.put(HPD, Some(NAV));
    g.put(&format!("{HPD}.opfor-orig"), Some(DICE));
    assert_eq!(legacy_markers(&g.path(HPD)), vec![".opfor-orig".to_owned()]);
    let p = g
        .led()
        .adopt(
            "navkit:mp_005",
            &[Adopt {
                rel: HPD.to_owned(),
                prior: Some(DICE.to_vec()),
            }],
            &[format!("{HPD}.opfor-orig")],
        )
        .unwrap();
    assert!(p.applied);
    assert!(legacy_markers(&g.path(HPD)).is_empty());
    assert_eq!(original(&g).as_deref(), Some(DICE));
    g.led().uninstall("navkit:mp_005", false).unwrap();
    assert_eq!(g.get(HPD).as_deref(), Some(DICE));
}

/// A record Project OPFOR's ledger wrote (other kinds, its own blob, Python's layout) is kept as it is; a whole-file
/// install over a file it holds slots in is refused.
#[test]
fn opfors_records_are_kept() {
    let g = Game::new("opfor");
    g.put("Dist/win32/package.mft", Some(b"head\r\n"));
    let root = default_root(&g.0);
    fs::create_dir_all(root.join("blobs")).unwrap();
    let blob = sha256::hex(b"old entry");
    fs::write(root.join("blobs").join(format!("{blob}.bin")), b"old entry").unwrap();
    let rec = format!(
        "{{\n \"format\": \"opfor-ledger/1\",\n \"owner\": \"pacific\",\n \"txn\": \"t\",\n \"installed\": \"i\",\n \
         \"note\": \"\",\n \"contributions\": [\n  {{\n   \"kind\": \"mft_block\",\n   \"file\": \
         \"Dist/win32/package.mft\",\n   \"slot\": \"levels/x\",\n   \"value\": \"b\",\n   \"prior\": null,\n   \
         \"below\": null,\n   \"stack\": true,\n   \"extra\": {{}}\n  }},\n  {{\n   \"kind\": \"bundle_entry\",\n   \
         \"file\": \"Package/async/startup-00.fbrb\",\n   \"slot\": \"e\",\n   \"value\": {{\"sha\": \"x\"}},\n   \
         \"prior\": {{\"sha\": \"{blob}\", \"blob\": \"{blob}\"}},\n   \"below\": null,\n   \"stack\": false,\n   \
         \"extra\": {{\"name\": \"E\"}}\n  }}\n ]\n}}\n"
    );
    fs::write(root.join("pacific.json"), &rec).unwrap();
    nav(&g).unwrap();
    g.led().uninstall("navkit:mp_005", false).unwrap();
    assert_eq!(fs::read_to_string(root.join("pacific.json")).unwrap(), rec, "untouched");
    assert!(
        root.join("blobs").join(format!("{blob}.bin")).is_file(),
        "its blob kept by the GC"
    );
    let e = g
        .led()
        .install(
            "navkit:x",
            &[("Dist/win32/package.mft".to_owned(), b"whole".to_vec())],
            false,
            "",
            false,
        )
        .unwrap_err();
    assert!(e.to_string().contains("pacific holds parts of it"), "{e}");
}

#[test]
fn a_spliced_record_keeps_its_other_fields() {
    let g = Game::new("splice");
    nav(&g).unwrap();
    smp(&g).unwrap();
    let root = default_root(&g.0);
    let p = root.join("stockmp~mp_005.json");
    let mut v: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
    v["note"] = json!("kept note");
    v["contributions"][0]["extra"]["mine"] = json!(7);
    fs::write(&p, serde_json::to_vec(&v).unwrap()).unwrap();
    g.led().uninstall("navkit:mp_005", false).unwrap();
    let w: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
    assert_eq!(w["note"], json!("kept note"));
    assert_eq!(w["contributions"][0]["extra"]["mine"], json!(7));
    assert_eq!(w["contributions"][0]["below"], Value::Null);
    assert_eq!(w["contributions"][0]["prior"]["sha"], json!(sha256::hex(DICE)));
}

#[test]
fn paths_and_owners() {
    assert_eq!(norm_rel("a\\b. /c").unwrap(), "a/b/c");
    assert!(norm_rel("..\\x").is_err());
    assert!(norm_rel("C:/x").is_err());
    assert!(valid_owner("navkit:mp_005"));
    assert!(!valid_owner("NavKit"));
    assert_eq!(owner_file("navkit:mp_005"), "navkit~mp_005.json");
}

#[test]
fn a_dry_run_writes_nothing() {
    let g = Game::new("dry");
    let p = g
        .led()
        .install("navkit:mp_005", &[(HPD.to_owned(), NAV.to_vec())], false, "", true)
        .unwrap();
    assert_eq!(p.writes, vec![format!("write {HPD}")]);
    assert!(!p.applied);
    assert_eq!(g.get(HPD).as_deref(), Some(DICE));
    assert!(!default_root(&g.0).exists());
}
