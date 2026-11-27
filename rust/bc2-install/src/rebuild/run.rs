//! Running the plan (spec XDEP-STEP-9): every step as soon as the steps it waits for ([`super::graph`]) are done, side
//! by side within a memory budget, at most `jobs` at a time; with `changed`, a step whose code, inputs, listings,
//! probes and outputs are as recorded is skipped (its recorded outputs stand for it), so a step whose rerun gives the
//! same files does not rerun the steps after it.
//!
//! What the step list's order decided before round OR7 holds:
//! - a step decides only when every step it waits for is done, against the files as they left them - the same
//!   decision the list order gave;
//! - a step about to run that reads (or edits in place) a file an earlier, skipped step wrote but a later step
//!   rewrote sends the run back to that writer first (XDEP-STEP-7): found before anything starts, by a dry pass over
//!   the list order that marks what would run;
//! - two steps whose scanned folders overlap never run at once (a scan around one would take the other's writes for
//!   its own);
//! - the run stops starting steps at the first that fails; the running ones finish.
//!
//! Afterwards the new records are checked against what ran side by side: two steps the records now order that ran at
//! the same time are reported (the declared graph missed an edge: the outputs of that run are not to be trusted).
#![allow(clippy::indexing_slicing, reason = "step indexes: below the plan's number of steps")]
#![allow(clippy::cast_precision_loss, reason = "bytes as MiB for the budget and the report")]

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::File;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::graph::Graph;
use super::sys::{Job, peak_working_set, physical};
use super::tracker::{self, FileState, Record, Tracker, ancestors};
use super::{Plan, Step, estimates, graph_of};

const MB: f64 = 1_048_576.0;

/// How a run ended.
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    /// 0, or 1 when a step failed (or the side-by-side check found two conflicting steps run at once).
    pub rc: i32,
    /// The steps that ran (or would run, dry), in the order they finished.
    pub ran: Vec<String>,
    /// The run's wall time, seconds.
    pub seconds: f64,
    /// The most steps running at once.
    pub most_at_once: usize,
    /// The sum of the measured peaks of the steps running at once, at most, MiB.
    pub peak_steps_mb: f64,
    /// Physical memory in use at the start and at most during the run, MiB.
    pub used_mb: Option<(f64, f64)>,
    /// Conflicting steps that ran at once.
    pub overlaps: Vec<String>,
    /// Edges of the records the declared graph does not imply.
    pub undeclared: Vec<String>,
}

impl Outcome {
    /// As JSON.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "rc": self.rc, "ran": self.ran, "seconds": self.seconds, "most_at_once": self.most_at_once,
            "peak_steps_mb": self.peak_steps_mb, "used_mb": self.used_mb, "overlaps": self.overlaps,
            "undeclared": self.undeclared,
        })
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn last_writers(recs: &[Option<Record>]) -> HashMap<String, Option<String>> {
    let mut last = HashMap::new();
    for r in recs.iter().flatten() {
        for (p, sha) in &r.outputs {
            last.insert(p.clone(), sha.clone());
        }
    }
    last
}

fn later_dirs(recs: &[Option<Record>], i: usize) -> BTreeSet<String> {
    ancestors(
        recs[i + 1..]
            .iter()
            .flatten()
            .flat_map(|r| r.outputs.keys().map(String::as_str)),
    )
}

/// The list-order pass of `--changed --dry` (`rebuild_all.run_changed` with `dry`): each step's reason to run (None:
/// unchanged), a step that would run marking its outputs as changed for the steps after it; and for each step that
/// would run, the earlier steps whose recorded outputs it reads (or edits) but the disk no longer holds. Those are
/// taken whether or not the pass expects them to run: a step the pass marks as running may be skipped once the steps
/// before it rerun and give the same files, and the reader would then meet a later step's version.
fn simulate(
    tr: &mut Tracker,
    plan: &Plan,
    recs: &[Option<Record>],
    forced: &BTreeSet<usize>,
) -> (Vec<Option<String>>, BTreeMap<usize, BTreeSet<usize>>) {
    let last = last_writers(recs);
    let mut state: HashMap<String, FileState> = HashMap::new();
    let mut recorded: HashMap<String, FileState> = HashMap::new();
    let mut why = Vec::with_capacity(recs.len());
    let mut back_to = BTreeMap::new();
    for (i, rec) in recs.iter().enumerate() {
        let w = if forced.contains(&i) {
            Some("sent back to by a later step".to_owned())
        } else {
            tr.why(rec.as_ref(), &plan.python_version, &state, &last, &later_dirs(recs, i))
        };
        if w.is_some() {
            let back = tr.stale_inputs(rec.as_ref(), &recorded);
            if !back.is_empty() {
                back_to.insert(i, back);
            }
        }
        for (p, sha) in rec.iter().flat_map(|r| r.outputs.iter()) {
            recorded.insert(
                p.clone(),
                FileState {
                    sha: sha.clone(),
                    step: i,
                    ran: false,
                },
            );
            let sha = if w.is_some() {
                Some(tracker::PENDING.to_owned())
            } else {
                sha.clone()
            };
            state.insert(
                p.clone(),
                FileState {
                    sha,
                    step: i,
                    ran: w.is_some(),
                },
            );
        }
        why.push(w);
    }
    (why, back_to)
}

/// The steps a `--changed` run must send back to (XDEP-STEP-7), to a fixed point; the lines saying so.
fn sent_back(tr: &mut Tracker, plan: &Plan, recs: &[Option<Record>]) -> (BTreeSet<usize>, Vec<String>) {
    let mut forced = BTreeSet::new();
    let mut lines = Vec::new();
    loop {
        let (_, stale) = simulate(tr, plan, recs, &forced);
        let mut new = false;
        for (i, back) in stale {
            let fresh: BTreeSet<usize> = back.difference(&forced).copied().collect();
            if let Some(&j) = fresh.iter().next() {
                lines.push(format!(
                    "   {}: its inputs from {} were rewritten by a later step - back to {j}",
                    plan.steps[i].name, plan.steps[j].name
                ));
                forced.extend(fresh);
                new = true;
            }
        }
        if !new {
            return (forced, lines);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum St {
    /// Waiting for its earlier steps.
    Wait,
    /// Decided to run, waiting for room.
    Ready,
    Running,
    /// Done: ran or skipped.
    Done(bool),
    Failed,
}

struct Shared {
    st: Vec<St>,
    files: HashMap<String, FileState>,
    recs: Vec<Option<Record>>,
    running_mb: f64,
    running: BTreeSet<usize>,
    times: Vec<Option<(f64, f64)>>,
    peaks: Vec<Option<f64>>,
    ran: Vec<usize>,
    stopped: Vec<String>,
    most: usize,
}

fn overlap(a: Option<&Vec<String>>, b: Option<&Vec<String>>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.iter().any(|x| b.iter().any(|y| x.eq_ignore_ascii_case(y))),
        _ => true,
    }
}

fn free_mb() -> Option<f64> {
    physical().map(|(_, avail)| avail as f64 / MB)
}

fn used_mb() -> Option<f64> {
    physical().map(|(total, avail)| total.saturating_sub(avail) as f64 / MB)
}

/// Free physical memory a step leaves when it starts beside others, MiB.
pub const RESERVE_MB: f64 = 1024.0;

/// The memory budget: the plan's, else 70% of physical memory but no more than what is free at the start less
/// [`RESERVE_MB`] (the PC is shared: what other programs use is not the rebuild's), at least 2 GiB; 8 GiB when
/// unknown. MiB.
#[must_use]
pub fn budget(plan: &Plan) -> f64 {
    plan.budget_mb.unwrap_or_else(|| {
        physical().map_or(8192.0, |(total, avail)| {
            (total as f64 / MB * 0.7)
                .min(avail as f64 / MB - RESERVE_MB)
                .max(2048.0)
        })
    })
}

/// One step run: its exit code, wall time, peak commit of its processes and its process's peak working set; its
/// new record when it succeeded.
fn execute(
    plan: &Plan,
    step: &Step,
    tr: &Mutex<Tracker>,
) -> (i32, f64, Option<f64>, Option<f64>, Option<Record>, Option<String>) {
    let t0 = Instant::now();
    let roots = step.roots.as_deref();
    let begun = lock(tr).begin(&step.name, roots);
    let (scratch, before) = match begun {
        Ok(b) => b,
        Err(e) => {
            return (
                1,
                0.0,
                None,
                None,
                None,
                Some(format!("{}: its record folder: {e}", step.name)),
            );
        }
    };
    let log = match File::create(&step.log) {
        Ok(f) => f,
        Err(e) => return (1, 0.0, None, None, None, Some(format!("{}: {e}", step.log.display()))),
    };
    let err = log.try_clone().map_or_else(|_| Stdio::null(), Stdio::from);
    let child = Command::new(&plan.python)
        .args(&step.argv)
        .current_dir(&plan.cwd)
        .env_clear()
        .envs(&plan.env)
        .env("OPFOR_REBUILD_TRACK", &scratch)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(err)
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            return (
                1,
                0.0,
                None,
                None,
                None,
                Some(format!("{}: {e}", plan.python.display())),
            );
        }
    };
    let job = Job::of(&child);
    let rc = child.wait().ok().and_then(|s| s.code()).unwrap_or(1);
    let secs = t0.elapsed().as_secs_f64();
    let commit = job.as_ref().and_then(Job::peak).map(|b| b as f64 / MB);
    let ws = peak_working_set(&child).map(|b| b as f64 / MB);
    drop(job);
    if rc != 0 {
        return (rc, secs, commit, ws, None, None);
    }
    let rounded = (commit.map(f64::round), Some((secs * 10.0).round() / 10.0));
    let fin = lock(tr).finish(&step.name, &plan.python_version, &scratch, &before, roots, rounded);
    match fin {
        Ok(r) => (0, secs, commit, ws, Some(r), None),
        Err(e) => (
            1,
            secs,
            commit,
            ws,
            None,
            Some(format!("{}: its record: {e}", step.name)),
        ),
    }
}

/// The step line `rebuild_all` printed: number, name, exit code, time, peaks.
fn step_line(n: usize, name: &str, rc: i32, secs: f64, ws: Option<f64>, commit: Option<f64>) -> String {
    let peak = match (ws, commit) {
        (Some(w), Some(c)) => format!("  peak {w:5.0} MB / {c:5.0} MB"),
        (None, Some(c)) => format!("  peak {c:5.0} MB"),
        _ => String::new(),
    };
    format!("{n:2} {name:45} exit {rc}  {secs:7.0} s{peak}")
}

/// Run the plan; `emit` gets each line to print (from any thread, one at a time).
///
/// # Errors
/// When the record folder cannot be made or the plan's declarations are inconsistent.
#[allow(clippy::too_many_lines, reason = "the scheduler's one loop")]
pub fn run(plan: &Plan, emit: &(dyn Fn(&str) + Sync)) -> Result<Outcome, String> {
    let began = Instant::now();
    let skip: BTreeSet<String> = plan.skip.iter().cloned().collect();
    let mut tr = Tracker::new(&plan.track, &plan.work, skip).map_err(|e| format!("{}: {e}", plan.track.display()))?;
    let n = plan.steps.len();
    let recs: Vec<Option<Record>> = plan.steps.iter().map(|s| tr.record(&s.name)).collect();
    let g = graph_of(plan, &recs)?;
    let est = estimates(plan, &recs);
    let changed = plan.mode == "changed";
    let mut out = Outcome::default();
    if changed && plan.dry {
        let (forced, lines) = sent_back(&mut tr, plan, &recs);
        for l in &lines {
            emit(l);
        }
        let (why, _) = simulate(&mut tr, plan, &recs, &forced);
        for (i, w) in why.iter().enumerate() {
            match w {
                None => emit(&format!("{i:2} {:45} unchanged", plan.steps[i].name)),
                Some(w) => {
                    emit(&format!("{i:2} {:45} would run: {w}", plan.steps[i].name));
                    out.ran.push(plan.steps[i].name.clone());
                }
            }
        }
        let _ = tr.hashes.save();
        out.seconds = began.elapsed().as_secs_f64();
        return Ok(out);
    }
    let forced = if changed {
        let (forced, lines) = sent_back(&mut tr, plan, &recs);
        for l in &lines {
            emit(l);
        }
        forced
    } else {
        BTreeSet::new()
    };
    let secs: Vec<f64> = est.iter().map(|e| e.0).collect();
    let prio = g.remaining_times(&secs);
    let budget = budget(plan);
    let jobs = plan.jobs.max(1);
    let base = used_mb();
    let mut files = HashMap::new();
    let mut st = vec![St::Wait; n];
    for (i, s) in st.iter_mut().enumerate().take(plan.start.min(n)) {
        if !changed {
            *s = St::Done(false);
            for (p, sha) in recs[i].iter().flat_map(|r| r.outputs.iter()) {
                files.insert(
                    p.clone(),
                    FileState {
                        sha: sha.clone(),
                        step: i,
                        ran: false,
                    },
                );
            }
        }
    }
    let shared = Mutex::new(Shared {
        st,
        files,
        recs,
        running_mb: 0.0,
        running: BTreeSet::new(),
        times: vec![None; n],
        peaks: vec![None; n],
        ran: Vec::new(),
        stopped: Vec::new(),
        most: 0,
    });
    let tr = Mutex::new(tr);
    let wake = Condvar::new();
    let done = AtomicBool::new(false);
    let peak_used = Mutex::new(base);
    let say = Mutex::new(());
    let emit_one = |l: &str| {
        let _g = lock(&say);
        emit(l);
    };
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !done.load(Ordering::Relaxed) {
                if let Some(u) = used_mb() {
                    let mut p = lock(&peak_used);
                    *p = Some(p.map_or(u, |x: f64| x.max(u)));
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        });
        let mut sh = lock(&shared);
        loop {
            // decide every step whose earlier steps are done
            for i in 0..n {
                if sh.st[i] != St::Wait || !g.preds[i].keys().all(|&k| matches!(sh.st[k], St::Done(_))) {
                    continue;
                }
                if !sh.stopped.is_empty() {
                    continue;
                }
                let name = &plan.steps[i].name;
                if !changed {
                    sh.st[i] = St::Ready;
                    continue;
                }
                let why = if forced.contains(&i) {
                    Some("sent back to by a later step".to_owned())
                } else {
                    let last = last_writers(&sh.recs);
                    let later = later_dirs(&sh.recs, i);
                    lock(&tr).why(sh.recs[i].as_ref(), &plan.python_version, &sh.files, &last, &later)
                };
                match why {
                    None => {
                        emit_one(&format!("{i:2} {name:45} unchanged"));
                        let outs: Vec<(String, Option<String>)> = sh.recs[i]
                            .iter()
                            .flat_map(|r| r.outputs.iter().map(|(p, s)| (p.clone(), s.clone())))
                            .collect();
                        for (p, sha) in outs {
                            sh.files.insert(
                                p,
                                FileState {
                                    sha,
                                    step: i,
                                    ran: false,
                                },
                            );
                        }
                        sh.st[i] = St::Done(false);
                    }
                    Some(w) => {
                        let back = lock(&tr).stale_inputs(sh.recs[i].as_ref(), &sh.files);
                        let missed: Vec<&str> = back
                            .iter()
                            .filter(|j| !forced.contains(j))
                            .map(|&j| plan.steps[j].name.as_str())
                            .collect();
                        if !missed.is_empty() {
                            emit_one(&format!(
                                "   {name}: its inputs from {} were rewritten by a later step, which the pass before \
                                 the run did not foresee: stopped (run --changed again)",
                                missed.join(", ")
                            ));
                            sh.stopped.push(plan.steps[i].log.display().to_string());
                            sh.st[i] = St::Failed;
                            continue;
                        }
                        emit_one(&format!("   {name}: {w}"));
                        sh.st[i] = St::Ready;
                    }
                }
            }
            // start what fits, the longest remaining path first
            let mut ready: Vec<usize> = (0..n).filter(|&i| sh.st[i] == St::Ready).collect();
            ready.sort_by(|&a, &b| prio[b].total_cmp(&prio[a]).then(a.cmp(&b)));
            let mut reserved = 0.0;
            if sh.stopped.is_empty() {
                for i in ready {
                    if sh.running.len() >= jobs {
                        break;
                    }
                    let roots = plan.steps[i].roots.as_ref();
                    if sh.running.iter().any(|&r| overlap(plan.steps[r].roots.as_ref(), roots)) {
                        continue;
                    }
                    let need = est[i].1;
                    let fits = sh.running.is_empty()
                        || (sh.running_mb + need + reserved <= budget
                            && free_mb().is_none_or(|f| f >= need + RESERVE_MB));
                    if !fits {
                        if reserved == 0.0 {
                            reserved = need;
                        }
                        continue;
                    }
                    sh.st[i] = St::Running;
                    sh.running.insert(i);
                    sh.running_mb += need;
                    sh.most = sh.most.max(sh.running.len());
                    let (shared, tr, wake, emit_one, began) = (&shared, &tr, &wake, &emit_one, &began);
                    let step = &plan.steps[i];
                    scope.spawn(move || {
                        let t0 = began.elapsed().as_secs_f64();
                        let (rc, secs, commit, ws, rec, err) = execute(plan, step, tr);
                        if let Some(e) = &err {
                            emit_one(e);
                        }
                        emit_one(&step_line(i, &step.name, rc, secs, ws, commit));
                        let mut sh = lock(shared);
                        sh.running.remove(&i);
                        sh.running_mb -= need;
                        sh.times[i] = Some((t0, began.elapsed().as_secs_f64()));
                        sh.peaks[i] = commit;
                        if rc == 0 {
                            if let Some(r) = rec {
                                for (p, sha) in &r.outputs {
                                    sh.files.insert(
                                        p.clone(),
                                        FileState {
                                            sha: sha.clone(),
                                            step: i,
                                            ran: true,
                                        },
                                    );
                                }
                                sh.recs[i] = Some(r);
                            }
                            sh.st[i] = St::Done(true);
                            sh.ran.push(i);
                        } else {
                            sh.st[i] = St::Failed;
                            sh.stopped.push(step.log.display().to_string());
                        }
                        drop(sh);
                        wake.notify_all();
                    });
                }
            }
            let finished = sh.st.iter().all(|s| matches!(s, St::Done(_)));
            if finished || (!sh.stopped.is_empty() && sh.running.is_empty()) {
                break;
            }
            sh = wake
                .wait_timeout(sh, Duration::from_secs(1))
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        drop(sh);
        done.store(true, Ordering::Relaxed);
    });
    let sh = shared.into_inner().unwrap_or_else(PoisonError::into_inner);
    let mut tr = tr.into_inner().unwrap_or_else(PoisonError::into_inner);
    let _ = tr.hashes.save();
    for log in &sh.stopped {
        emit(&format!("stopped: {log}"));
    }
    out.rc = i32::from(!sh.stopped.is_empty());
    out.ran = sh.ran.iter().map(|&i| plan.steps[i].name.clone()).collect();
    out.seconds = began.elapsed().as_secs_f64();
    out.most_at_once = sh.most;
    out.peak_steps_mb = sh
        .times
        .iter()
        .flatten()
        .map(|&(s, _)| {
            sh.times
                .iter()
                .zip(&sh.peaks)
                .filter_map(|(t, p)| t.filter(|&(a, b)| a <= s && s < b).and(*p))
                .sum::<f64>()
        })
        .fold(0.0, f64::max);
    out.used_mb = base.zip(*lock(&peak_used));
    check(plan, &tr, &sh.times, &mut out)?;
    Ok(out)
}

/// The new records against what ran at once and against the declared graph.
fn check(plan: &Plan, tr: &Tracker, times: &[Option<(f64, f64)>], out: &mut Outcome) -> Result<(), String> {
    let recs: Vec<Option<Record>> = plan.steps.iter().map(|s| tr.record(&s.name)).collect();
    let rec_refs: Vec<Option<&Record>> = recs.iter().map(Option::as_ref).collect();
    let none: Vec<Option<BTreeSet<usize>>> = vec![Some(BTreeSet::new()); recs.len()];
    let recorded = Graph::build(&rec_refs, &none, &tracker::norm(&plan.work));
    for (j, p) in recorded.preds.iter().enumerate() {
        for (&i, kinds) in p {
            if let (Some((a0, a1)), Some((b0, _))) = (times[i], times[j])
                && b0 < a1
                && a0 <= b0
            {
                let k: Vec<String> = kinds.iter().map(ToString::to_string).collect();
                out.overlaps.push(format!(
                    "{} and {} ran at once, but {} ({})",
                    plan.steps[i].name,
                    plan.steps[j].name,
                    "the records order them",
                    k.join(", ")
                ));
            }
        }
    }
    if !out.overlaps.is_empty() {
        out.rc = 1;
    }
    let declared = plan.declared()?;
    if declared.iter().any(Option::is_some) {
        let nobody: Vec<Option<&Record>> = vec![None; recs.len()];
        let dg = Graph::build(&nobody, &declared, &tracker::norm(&plan.work));
        for (i, j, kinds) in recorded.missing_from(&dg) {
            let k: Vec<String> = kinds.iter().map(ToString::to_string).collect();
            out.undeclared.push(format!(
                "{} -> {} ({})",
                plan.steps[i].name,
                plan.steps[j].name,
                k.join(", ")
            ));
        }
    }
    Ok(())
}
