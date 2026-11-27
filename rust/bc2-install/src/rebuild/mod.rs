//! Project OPFOR's WW2 rebuild as a step graph (round OR7, "OPFOR in Rust": `rebuild_all.py`'s runner and
//! `stepstamps.py`'s records and decisions). OPFOR's `rebuild_all.py` keeps the steps (their command lines in the
//! documented order, the folders each writes, the inputs no step writes) and hands them over as a [`Plan`]; this
//! module runs them:
//!
//! - [`graph`]: which step waits for which - from the steps' records and the declared graph (spec XDEP-STEP-8);
//! - [`tracker`]: the records and the decision whether a recorded step must run again (XDEP-STEP-1, -7);
//! - [`run`]: the steps side by side within a memory budget, each as soon as the steps it waits for are done, a step
//!   whose inputs did not change skipped (XDEP-STEP-9);
//! - [`sys`]: the machine's free memory and a step's peak (a job object around its processes).
//!
//! | Project OPFOR (Python, until OR7) | here |
//! |---|---|
//! | `rebuild_all.run` / `run_changed` / `run_chains` / `run_step`, `JOBS`, `MIN_FREE`, `peak_memory` | [`run::run`] |
//! | `stepstamps.Tracker.begin` / `finish` / `why` / `stale_inputs`, `Hashes`, `scan` | [`tracker`] |
//! | (none: the fixed list) | [`graph`], [`derive`] |
#![allow(clippy::indexing_slicing, reason = "step indexes: below the plan's number of steps")]

pub mod graph;
pub mod run;
pub mod sys;
pub mod tracker;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{Value, json};

pub use graph::{Graph, Kind};
pub use tracker::{Record, Tracker};

/// A step memory estimate when nothing is known (no record, no declared figure), MiB.
pub const DEFAULT_PEAK_MB: f64 = 1536.0;
/// A step time estimate when nothing is known, seconds.
pub const DEFAULT_SECONDS: f64 = 10.0;

/// One step of the plan.
#[derive(Debug, Clone, Deserialize)]
pub struct Step {
    /// Its name: the script and its arguments, space-separated (the record's key).
    pub name: String,
    /// The command line after the Python interpreter.
    pub argv: Vec<String>,
    /// Its log file.
    pub log: PathBuf,
    /// The top-level folders under `work\ww2` scanned around it (None: all of them).
    pub roots: Option<Vec<String>>,
    /// The declared earlier steps it waits for, by name (None: the declared graph does not know the step).
    pub after: Option<Vec<String>>,
    /// The declared peak memory, MiB (the record's measured one wins).
    pub peak_mb: Option<f64>,
    /// The declared time, seconds (the record's measured one wins).
    pub seconds: Option<f64>,
}

/// What `rebuild_all.py` hands over.
#[derive(Debug, Clone, Deserialize)]
pub struct Plan {
    /// `work\ww2`.
    pub work: PathBuf,
    /// The record folder (`work\ww2\_rebuild\track`).
    pub track: PathBuf,
    /// The steps' working folder (OPFOR's `src\tools\ww2`).
    pub cwd: PathBuf,
    /// The Python interpreter.
    pub python: PathBuf,
    /// Its `sys.version` (a record of another Python reruns its step).
    pub python_version: String,
    /// The steps' environment (the hook on `PYTHONPATH`); `OPFOR_REBUILD_TRACK` is added per step.
    pub env: BTreeMap<String, String>,
    /// The top-level folders of `work\ww2` never scanned.
    pub skip: Vec<String>,
    /// The steps in the documented order.
    pub steps: Vec<Step>,
    /// `all`: every step from `start`; `changed`: only the steps whose code, inputs or outputs changed.
    pub mode: String,
    /// Only say what would run.
    #[serde(default)]
    pub dry: bool,
    /// The first step a full run runs (the earlier ones' outputs as they are).
    #[serde(default)]
    pub start: usize,
    /// Steps at a time at most.
    #[serde(default = "default_jobs")]
    pub jobs: usize,
    /// The memory the running steps' estimates may add up to, MiB (None: 70% of physical memory).
    pub budget_mb: Option<f64>,
    /// Whether a step waits for free physical memory (its estimate and [`run::RESERVE_MB`]) besides the budget
    /// (default true). A test plan of tiny steps turns it off (round OR14: OPFOR's `test_rebuild_tracker` failed
    /// on a busy PC, the two side-by-side steps run one at a time); a real rebuild never does.
    #[serde(default = "default_true")]
    pub free_gate: bool,
}

fn default_true() -> bool {
    true
}

fn default_jobs() -> usize {
    4
}

impl Plan {
    /// From its JSON.
    ///
    /// # Errors
    /// When the JSON is not a plan, a step is named twice or a declared step is not an earlier one.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let p: Self = serde_json::from_str(text).map_err(|e| format!("not a rebuild plan: {e}"))?;
        p.declared()?;
        Ok(p)
    }

    /// The index of each step's name.
    #[must_use]
    pub fn index(&self) -> BTreeMap<&str, usize> {
        self.steps
            .iter()
            .enumerate()
            .map(|(i, s)| (s.name.as_str(), i))
            .collect()
    }

    /// Each step's declared earlier steps (None: not declared).
    ///
    /// # Errors
    /// When a name is given twice or a declared step is unknown or not earlier.
    pub fn declared(&self) -> Result<Vec<Option<BTreeSet<usize>>>, String> {
        let idx = self.index();
        if idx.len() != self.steps.len() {
            return Err("a step is named twice".to_owned());
        }
        self.steps
            .iter()
            .enumerate()
            .map(|(j, s)| {
                s.after
                    .as_ref()
                    .map(|a| {
                        a.iter()
                            .map(|n| match idx.get(n.as_str()) {
                                Some(&i) if i < j => Ok(i),
                                Some(_) => Err(format!("{}: declared after {n}, a later step", s.name)),
                                None => Err(format!("{}: declared after {n}, no such step", s.name)),
                            })
                            .collect()
                    })
                    .transpose()
            })
            .collect()
    }
}

/// The step graph of the plan on the records at hand.
///
/// # Errors
/// When the plan's declarations are inconsistent.
pub fn graph_of(plan: &Plan, recs: &[Option<Record>]) -> Result<Graph, String> {
    let declared = plan.declared()?;
    let rec_refs: Vec<Option<&Record>> = recs.iter().map(Option::as_ref).collect();
    Ok(Graph::build(&rec_refs, &declared, &tracker::norm(&plan.work)))
}

/// Each step's time and memory estimates: the record's measured figures, else the declared, else the defaults.
#[must_use]
pub fn estimates(plan: &Plan, recs: &[Option<Record>]) -> Vec<(f64, f64)> {
    plan.steps
        .iter()
        .zip(recs)
        .map(|(s, r)| {
            let secs = r
                .as_ref()
                .and_then(|r| r.seconds)
                .or(s.seconds)
                .unwrap_or(DEFAULT_SECONDS);
            let mb = r
                .as_ref()
                .and_then(|r| r.peak_mb)
                .or(s.peak_mb)
                .unwrap_or(DEFAULT_PEAK_MB);
            (secs, mb)
        })
        .collect()
}

/// The declared graph from the records at hand (OPFOR's `rebuild_graph.json`): each step's direct earlier steps
/// (the transitive reduction of the recorded and declared edges), its measured peak and time. A step with no record
/// keeps its declared entry.
///
/// # Errors
/// When the plan's declarations are inconsistent.
pub fn derive(plan: &Plan, recs: &[Option<Record>]) -> Result<Value, String> {
    let g = graph_of(plan, recs)?;
    let red = g.reduced();
    let est = estimates(plan, recs);
    let steps: Vec<Value> = plan
        .steps
        .iter()
        .enumerate()
        .map(|(j, s)| {
            let after: Vec<&str> = red[j].iter().map(|&i| plan.steps[i].name.as_str()).collect();
            json!({"step": s.name, "after": after, "seconds": (est[j].0 * 10.0).round() / 10.0,
                   "peak_mb": est[j].1.round()})
        })
        .collect();
    Ok(json!({"version": 1, "steps": steps}))
}

/// A description of the graph: each step's direct earlier steps and why, the longest path and what a run side by
/// side would take with the estimates (`rebuild_all.py --graph`).
///
/// # Errors
/// When the plan's declarations are inconsistent.
pub fn describe(plan: &Plan, recs: &[Option<Record>]) -> Result<Vec<String>, String> {
    let g = graph_of(plan, recs)?;
    let red = g.reduced();
    let est = estimates(plan, recs);
    let secs: Vec<f64> = est.iter().map(|e| e.0).collect();
    let fin = g.finish_times(&secs);
    let mut out = Vec::new();
    for (j, s) in plan.steps.iter().enumerate() {
        let after: Vec<String> = red[j]
            .iter()
            .map(|i| {
                let kinds: Vec<String> = g.preds[j][i].iter().map(ToString::to_string).collect();
                format!("{i} ({})", kinds.join(", "))
            })
            .collect();
        out.push(format!(
            "{j:2} {:45} {:6.0} s {:6.0} MB  ends {:5.0} s  after {}{}",
            s.name,
            est[j].0,
            est[j].1,
            fin[j],
            if after.is_empty() {
                "nothing".to_owned()
            } else {
                after.join(", ")
            },
            if recs[j].is_none() { "  (no record)" } else { "" },
        ));
    }
    let total: f64 = secs.iter().sum();
    let longest = fin.iter().copied().fold(0.0, f64::max);
    let edges: usize = g.preds.iter().map(BTreeMap::len).sum();
    out.push(format!(
        "{} steps, {edges} edges ({} after the reduction); one after another {:.1} min, the longest path {:.1} min",
        plan.steps.len(),
        red.iter().map(BTreeSet::len).sum::<usize>(),
        total / 60.0,
        longest / 60.0
    ));
    Ok(out)
}
