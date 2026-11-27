//! The step graph, the records and the runner on synthetic steps (no OPFOR, no game copy): records built by hand
//! for the graph's edges, hook records written by hand for the tracker, and on Windows small `cmd.exe` steps run for
//! real for the scheduler (side by side, the memory budget, a failure, a rerun that skips everything, a touched
//! output).

#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test assertions"
)]

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use super::graph::{Graph, Kind};
use super::tracker::{self, FileState, Record, Tracker, key, listing};
use super::{Plan, Step, derive, graph_of};

static N: AtomicU32 = AtomicU32::new(0);

/// A fresh folder of this test run's own under the system's temporary folder (removed when dropped).
struct Temp(PathBuf);

impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "bc2-rebuild-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn record_names_are_stepstamps_keys() {
    assert_eq!(
        key("wake_translate.py Objects/ -Objects/Weapons"),
        "wake_translate_py_Objects___Objects_Weapons_0b82104ed4.json"
    );
    assert_eq!(
        key("ww2_level_build.py guadal_canal level shaders clone billboards translate grid"),
        "ww2_level_build_py_guadal_canal_level_shaders_clone_billboar_efb97275af.json"
    );
}

#[test]
fn a_listing_digest_is_the_hooks() {
    let t = Temp::new();
    for n in ["a b", "b.txt", "it's"] {
        fs::write(t.0.join(n), b"").unwrap();
    }
    // Python: hashlib.sha1(repr(sorted(os.listdir(p))).encode()).hexdigest()
    assert_eq!(listing(&t.0), "ed83e967902291f988f9b9b4019c5082dfedaafd");
    assert_eq!(listing(&t.0.join("none")), "6eef6648406c333a4035cd5e60d0bf2ecf2606d7");
}

fn rec(name: &str, reads: &[&str], writes: &[&str], lists: &[&str]) -> Record {
    Record {
        step: name.to_owned(),
        tracker: tracker::VERSION,
        inputs: reads.iter().map(|p| ((*p).to_owned(), Some("r".to_owned()))).collect(),
        outputs: writes.iter().map(|p| ((*p).to_owned(), Some("w".to_owned()))).collect(),
        lists: lists.iter().map(|p| ((*p).to_owned(), "d".to_owned())).collect(),
        ..Record::default()
    }
}

#[test]
fn the_graph_keeps_every_conflict_in_list_order() {
    let w = r"c:\w";
    let recs = [
        Some(rec("s0", &[], &[r"c:\w\a\f.txt"], &[])),
        Some(rec("s1", &[r"c:\w\a\f.txt"], &[r"c:\w\b\g.txt"], &[])),
        Some(rec("s2", &[], &[r"c:\w\a\f.txt"], &[])),
        // the names of work and the folders above it are no edge
        Some(rec("s3", &[], &[r"c:\w\c\h.txt"], &[r"c:\w", r"c:\"])),
        Some(rec("s4", &[], &[r"c:\w\d\i.txt"], &[r"c:\w\a"])),
        None,
        None,
        Some(rec("s7", &[r"c:\w\c\h.txt"], &[], &[])),
    ];
    let rec_refs: Vec<Option<&Record>> = recs.iter().map(Option::as_ref).collect();
    let mut declared: Vec<Option<BTreeSet<usize>>> = vec![Some(BTreeSet::new()); recs.len()];
    declared[5] = Some([1].into());
    declared[6] = None;
    let g = Graph::build(&rec_refs, &declared, w);
    let kinds = |i: usize, j: usize| g.preds[j].get(&i).cloned().unwrap_or_default();
    assert_eq!(kinds(0, 1), [Kind::ReadAfterWrite].into());
    assert_eq!(kinds(0, 2), [Kind::WriteAfterWrite].into());
    assert_eq!(kinds(1, 2), [Kind::WriteAfterRead].into());
    assert!(g.preds[3].is_empty(), "{:?}", g.preds[3]);
    assert_eq!(kinds(0, 4), [Kind::Listing].into());
    assert_eq!(kinds(2, 4), [Kind::Listing].into());
    // declared without a record: its declared edges, and the list order with the step nobody knows
    assert_eq!(g.preds[5].keys().copied().collect::<Vec<_>>(), [1]);
    assert_eq!(g.preds[6].keys().copied().collect::<Vec<_>>(), [0, 1, 2, 3, 4, 5]);
    assert_eq!(kinds(6, 7), [Kind::Unknown].into());
    assert_eq!(kinds(3, 7), [Kind::ReadAfterWrite].into());
    assert!(!g.preds[7].contains_key(&5), "s5 is declared: s7 does not wait for it");
    // the reduction drops 0 -> 2 (through 1); the longest paths: 0 -> 1 -> 2 -> 4 and through the unknown step 6
    let red = g.reduced();
    assert_eq!(red[2], [1].into());
    let fin = g.finish_times(&[1.0; 8]);
    assert!((fin[4] - 4.0).abs() < 1e-9 && (fin[7] - 6.0).abs() < 1e-9, "{fin:?}");
}

fn write_json(p: &Path, v: &serde_json::Value) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, serde_json::to_vec(v).unwrap()).unwrap();
}

#[test]
fn a_record_joins_the_hooks_records_and_decides() {
    let t = Temp::new();
    let work = t.0.join("work");
    let src = t.0.join("src");
    fs::create_dir_all(work.join("a")).unwrap();
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("in.txt"), b"in").unwrap();
    fs::write(src.join("code.py"), b"code").unwrap();
    let mut tr = Tracker::new(&work.join("_rebuild").join("track"), &work, BTreeSet::new()).unwrap();
    let (scratch, before) = tr.begin("s.py", Some(&["a".to_owned()])).unwrap();
    // the step: reads in.txt, writes a\out.txt (seen by the hook) and a\x.txt (seen by the scan only)
    fs::write(work.join("a").join("out.txt"), b"out").unwrap();
    fs::write(work.join("a").join("x.txt"), b"x").unwrap();
    let n = |p: &Path| tracker::norm(p);
    let st = tracker::stamp(&src.join("in.txt")).unwrap();
    write_json(
        &scratch.join("100.json"),
        &serde_json::json!({
            "reads": {n(&src.join("in.txt")): [st.0, i64::try_from(st.1).unwrap()]},
            "writes": [n(&work.join("a").join("out.txt"))], "rmtrees": [], "lists": {}, "probes": {},
            "progs": {}, "modules": [n(&src.join("code.py"))],
        }),
    );
    let r = tr
        .finish(
            "s.py",
            "3.12",
            &scratch,
            &before,
            Some(&["a".to_owned()]),
            (Some(100.0), Some(1.5)),
        )
        .unwrap();
    assert_eq!(r.inputs.keys().cloned().collect::<Vec<_>>(), [n(&src.join("in.txt"))]);
    assert_eq!(r.outputs.len(), 2);
    assert!(r.code.contains_key(&n(&src.join("code.py"))));
    assert_eq!(tr.record("s.py").unwrap().peak_mb, Some(100.0));
    let none = HashMap::new();
    let last = HashMap::new();
    let later = BTreeSet::new();
    let rec = tr.record("s.py");
    assert_eq!(tr.why(rec.as_ref(), "3.12", &none, &last, &later), None);
    assert_eq!(
        tr.why(rec.as_ref(), "3.13", &none, &last, &later).as_deref(),
        Some("Python changed")
    );
    // an earlier step that will run first rewrites its input (dry)
    let pending: HashMap<String, FileState> = [(
        n(&src.join("in.txt")),
        FileState {
            sha: Some(tracker::PENDING.to_owned()),
            step: 0,
            ran: true,
        },
    )]
    .into();
    assert!(
        tr.why(rec.as_ref(), "3.12", &pending, &last, &later)
            .unwrap()
            .starts_with("reads a file a step before")
    );
    fs::write(work.join("a").join("x.txt"), b"edited").unwrap();
    assert!(
        tr.why(rec.as_ref(), "3.12", &none, &last, &later)
            .unwrap()
            .starts_with("output changed on disk")
    );
    fs::write(work.join("a").join("x.txt"), b"x").unwrap();
    fs::write(src.join("in.txt"), b"other").unwrap();
    assert!(
        tr.why(rec.as_ref(), "3.12", &none, &last, &later)
            .unwrap()
            .starts_with("input changed")
    );
    fs::write(src.join("code.py"), b"other").unwrap();
    assert!(
        tr.why(rec.as_ref(), "3.12", &none, &last, &later)
            .unwrap()
            .starts_with("code changed")
    );
}

/// A plan of `cmd.exe` steps over `work` (no hook: the scans see what they write).
#[cfg(windows)]
fn plan(t: &Path, steps: &[(&str, &str, &[&str])], mode: &str, budget_mb: Option<f64>) -> Plan {
    let comspec = std::env::var("ComSpec").unwrap_or_else(|_| r"C:\Windows\System32\cmd.exe".to_owned());
    let work = t.join("work");
    fs::create_dir_all(work.join("_rebuild")).unwrap();
    let env: BTreeMap<String, String> = std::env::vars().collect();
    Plan {
        work: work.clone(),
        track: work.join("_rebuild").join("track"),
        cwd: t.to_path_buf(),
        python: PathBuf::from(comspec),
        python_version: "cmd".to_owned(),
        env,
        skip: vec!["_rebuild".to_owned()],
        steps: steps
            .iter()
            .map(|(name, cmd, roots)| Step {
                name: (*name).to_owned(),
                argv: vec!["/d".to_owned(), "/c".to_owned(), (*cmd).to_owned()],
                log: work.join("_rebuild").join(format!("{name}.log")),
                roots: Some(roots.iter().map(|r| (*r).to_owned()).collect()),
                after: Some(Vec::new()),
                peak_mb: Some(100.0),
                seconds: Some(1.0),
            })
            .collect(),
        mode: mode.to_owned(),
        dry: false,
        start: 0,
        jobs: 4,
        budget_mb,
        free_gate: true,
    }
}

#[cfg(windows)]
fn go(p: &Plan) -> (super::run::Outcome, String) {
    let lines = Mutex::new(Vec::new());
    let out = super::run::run(p, &|l: &str| lines.lock().unwrap().push(l.to_owned())).unwrap();
    let text = lines.into_inner().unwrap().join("\n");
    (out, text)
}

#[cfg(windows)]
#[test]
fn independent_steps_run_side_by_side_within_the_budget_and_a_rerun_skips_them() {
    let t = Temp::new();
    // each step waits about a second (ping's interval), then writes its folder's file
    let steps: [(&str, &str, &[&str]); 3] = [
        (
            "a",
            r"ping -n 2 127.0.0.1 >nul & mkdir work\a & echo a> work\a\f.txt",
            &["a"],
        ),
        (
            "b",
            r"ping -n 2 127.0.0.1 >nul & mkdir work\b & echo b> work\b\f.txt",
            &["b"],
        ),
        (
            "c",
            r"ping -n 2 127.0.0.1 >nul & mkdir work\c & echo c> work\c\f.txt",
            &["c"],
        ),
    ];
    // no records yet: a budget of 150 MB for steps declared at 100 MB each runs them one at a time
    let p1 = plan(&t.0, &steps, "all", Some(150.0));
    let (out, log) = go(&p1);
    assert_eq!((out.rc, out.most_at_once), (0, 1), "{log}");
    // the records' measured peaks (a few MB each) count from now on; without a budget all three at once
    let p = plan(&t.0, &steps, "all", None);
    let (out, log) = go(&p);
    assert_eq!(out.rc, 0, "{log}");
    assert_eq!(out.most_at_once, 3, "{log}");
    assert_eq!(out.ran.len(), 3);
    let tr = Tracker::new(&p.track, &p.work, BTreeSet::new()).unwrap();
    assert!(tr.record("a").unwrap().peak_mb.is_some_and(|m| m > 0.0 && m < 100.0));
    // nothing changed: every step skipped
    let pc = plan(&t.0, &steps, "changed", None);
    let (out, log) = go(&pc);
    assert!(out.ran.is_empty() && log.matches("unchanged").count() == 3, "{log}");
    // an output touched: only its step reruns
    fs::write(t.0.join("work").join("b").join("f.txt"), b"edited").unwrap();
    let (out, log) = go(&pc);
    assert_eq!(out.ran, ["b"], "{log}");
    assert!(log.contains("output changed on disk"), "{log}");
}

#[cfg(windows)]
#[test]
fn a_failed_step_stops_the_run_and_keeps_no_record() {
    let t = Temp::new();
    let steps: [(&str, &str, &[&str]); 3] = [
        ("a", r"mkdir work\a & echo a> work\a\f.txt", &["a"]),
        ("bad", "exit 3", &["bad"]),
        ("c", r"mkdir work\c & echo c> work\c\f.txt", &["c"]),
    ];
    let mut p = plan(&t.0, &steps, "all", None);
    // c waits for bad: it never starts
    p.steps[2].after = Some(vec!["bad".to_owned()]);
    let (out, log) = go(&p);
    assert_eq!(out.rc, 1, "{log}");
    assert!(
        log.contains("exit 3") && log.contains("stopped:") && log.contains("bad.log"),
        "{log}"
    );
    assert!(!t.0.join("work").join("c").exists());
    let tr = Tracker::new(&p.track, &p.work, BTreeSet::new()).unwrap();
    assert!(tr.record("bad").is_none() && tr.record("a").is_some());
}

#[cfg(windows)]
#[test]
fn the_derived_graph_orders_a_reader_after_its_writer() {
    let t = Temp::new();
    // b's record cannot see that it reads a's file (no hook): it is declared, and the derived graph keeps it
    let steps: [(&str, &str, &[&str]); 2] = [
        ("a", r"mkdir work\a & echo a> work\a\f.txt", &["a"]),
        ("b", r"mkdir work\b & copy work\a\f.txt work\b\f.txt >nul", &["b"]),
    ];
    let mut p = plan(&t.0, &steps, "all", None);
    p.steps[1].after = Some(vec!["a".to_owned()]);
    let (out, log) = go(&p);
    assert_eq!((out.rc, out.most_at_once), (0, 1), "{log}");
    assert_eq!(
        fs::read_to_string(t.0.join("work").join("b").join("f.txt"))
            .unwrap()
            .trim(),
        "a"
    );
    let tr = Tracker::new(&p.track, &p.work, BTreeSet::new()).unwrap();
    let recs: Vec<Option<Record>> = p.steps.iter().map(|s| tr.record(&s.name)).collect();
    let d = derive(&p, &recs).unwrap();
    assert_eq!(d["steps"][1]["after"], serde_json::json!(["a"]));
    assert!(graph_of(&p, &recs).unwrap().preds[1].contains_key(&0));
}

#[test]
fn a_plan_refuses_a_declared_later_step() {
    let text = serde_json::json!({
        "work": "w", "track": "w/t", "cwd": ".", "python": "py", "python_version": "3", "env": {}, "skip": [],
        "mode": "all", "budget_mb": null,
        "steps": [
            {"name": "a", "argv": [], "log": "a.log", "roots": null, "after": ["b"], "peak_mb": null, "seconds": null},
            {"name": "b", "argv": [], "log": "b.log", "roots": null, "after": [], "peak_mb": null, "seconds": null},
        ],
    })
    .to_string();
    assert!(Plan::from_json(&text).unwrap_err().contains("a later step"));
}
