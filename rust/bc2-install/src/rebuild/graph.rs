//! The WW2 rebuild's step graph (spec XDEP-STEP-8): which step must finish before which, from the steps' records
//! (what each read, wrote, listed and tested when it last ran) and the declared graph (each step's `after` list:
//! OPFOR's `rebuild_graph.json`, derived from a full rebuild's records, and the folders a step is declared to read).
//!
//! Two steps conflict, and keep their list order, when one writes a file the other reads, edits or writes, or lists
//! a folder above a file the other writes, or tests a path the other writes. Not a conflict: the names of
//! `work\ww2` itself and the folders above it (which top-level folders exist is the declared order's business: a
//! step's first output folder is made once, from an empty `work\ww2`). A step without a record keeps only its
//! declared edges; a step neither recorded nor declared keeps the list order with every other step.
#![allow(
    clippy::indexing_slicing,
    reason = "step indexes: below the number of steps the graph was built for"
)]

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

use super::tracker::{Record, ancestors};

/// Why one step must finish before another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// The later step reads (or edits) a file the earlier one writes.
    ReadAfterWrite,
    /// Both write a file.
    WriteAfterWrite,
    /// The later step writes a file the earlier one reads: the earlier must read the version before it.
    WriteAfterRead,
    /// One lists a folder above a file the other writes.
    Listing,
    /// One tests a path the other writes.
    Probe,
    /// The declared graph says so.
    Declared,
    /// One of the two has neither a record nor a declared place: the list order.
    Unknown,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ReadAfterWrite => "read-after-write",
            Self::WriteAfterWrite => "write-after-write",
            Self::WriteAfterRead => "write-after-read",
            Self::Listing => "listing",
            Self::Probe => "probe",
            Self::Declared => "declared",
            Self::Unknown => "unknown",
        })
    }
}

/// The step graph: for each step, the earlier steps it waits for and why.
#[derive(Debug, Clone, Default)]
pub struct Graph {
    /// `preds[j]`: {i < j: the kinds}.
    pub preds: Vec<BTreeMap<usize, BTreeSet<Kind>>>,
}

/// A recorded step's sets, normalized paths.
#[derive(Debug, Default)]
struct Sets {
    writes: Vec<String>,
    reads: Vec<String>,
    lists: Vec<String>,
    probes: Vec<String>,
    write_dirs: BTreeSet<String>,
}

fn sets(r: &Record, work_and_above: &BTreeSet<String>) -> Sets {
    let writes: Vec<String> = r.outputs.keys().cloned().collect();
    let write_dirs = ancestors(writes.iter().map(String::as_str));
    Sets {
        reads: r.inputs.keys().chain(r.edits.keys()).cloned().collect(),
        lists: r
            .lists
            .keys()
            .filter(|d| !work_and_above.contains(*d))
            .cloned()
            .collect(),
        probes: r.probes.keys().cloned().collect(),
        writes,
        write_dirs,
    }
}

impl Graph {
    /// The graph of `n` steps: `recs[i]` the step's record, `declared[i]` its declared earlier steps (None: the
    /// declared graph does not know the step); `work`: `work\ww2`, normalized.
    #[must_use]
    pub fn build(recs: &[Option<&Record>], declared: &[Option<BTreeSet<usize>>], work: &str) -> Self {
        let n = recs.len();
        let mut work_and_above: BTreeSet<String> = ancestors([format!("{work}\\x").as_str()]);
        work_and_above.insert(work.to_owned());
        let s: Vec<Option<Sets>> = recs.iter().map(|r| r.map(|r| sets(r, &work_and_above))).collect();
        // path -> the steps writing it / reading it / folder -> the steps writing under it / listing it
        let mut writers: HashMap<&str, Vec<usize>> = HashMap::new();
        let mut readers: HashMap<&str, Vec<usize>> = HashMap::new();
        let mut dir_writers: HashMap<&str, Vec<usize>> = HashMap::new();
        let mut listers: HashMap<&str, Vec<usize>> = HashMap::new();
        let mut probers: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, x) in s.iter().enumerate() {
            let Some(x) = x else { continue };
            for p in &x.writes {
                writers.entry(p).or_default().push(i);
            }
            for p in &x.reads {
                readers.entry(p).or_default().push(i);
            }
            for d in &x.write_dirs {
                dir_writers.entry(d).or_default().push(i);
            }
            for d in &x.lists {
                listers.entry(d).or_default().push(i);
            }
            for p in &x.probes {
                probers.entry(p).or_default().push(i);
            }
        }
        let mut preds: Vec<BTreeMap<usize, BTreeSet<Kind>>> = vec![BTreeMap::new(); n];
        let add = |i: usize, j: usize, k: Kind, preds: &mut Vec<BTreeMap<usize, BTreeSet<Kind>>>| {
            if i < j {
                preds[j].entry(i).or_default().insert(k);
            }
        };
        for (j, x) in s.iter().enumerate() {
            let Some(x) = x else { continue };
            for p in &x.reads {
                for &i in writers.get(p.as_str()).into_iter().flatten() {
                    add(i, j, Kind::ReadAfterWrite, &mut preds);
                }
            }
            for p in &x.writes {
                for &i in writers.get(p.as_str()).into_iter().flatten() {
                    add(i, j, Kind::WriteAfterWrite, &mut preds);
                }
                for &i in readers.get(p.as_str()).into_iter().flatten() {
                    add(i, j, Kind::WriteAfterRead, &mut preds);
                }
                for &i in probers.get(p.as_str()).into_iter().flatten() {
                    add(i, j, Kind::Probe, &mut preds);
                }
            }
            for d in &x.lists {
                for &i in dir_writers.get(d.as_str()).into_iter().flatten() {
                    add(i, j, Kind::Listing, &mut preds);
                }
            }
            for d in &x.write_dirs {
                for &i in listers.get(d.as_str()).into_iter().flatten() {
                    add(i, j, Kind::Listing, &mut preds);
                }
                for &i in probers.get(d.as_str()).into_iter().flatten() {
                    add(i, j, Kind::Probe, &mut preds);
                }
            }
            for p in &x.probes {
                for &i in writers.get(p.as_str()).into_iter().flatten() {
                    add(i, j, Kind::Probe, &mut preds);
                }
                for &i in dir_writers.get(p.as_str()).into_iter().flatten() {
                    add(i, j, Kind::Probe, &mut preds);
                }
            }
        }
        for j in 0..n {
            if let Some(d) = &declared[j] {
                for &i in d {
                    add(i, j, Kind::Declared, &mut preds);
                }
            }
            for i in 0..j {
                let known_i = recs[i].is_some() || declared[i].is_some();
                let both = recs[i].is_some() && recs[j].is_some();
                if !both && (declared[j].is_none() || !known_i) {
                    add(i, j, Kind::Unknown, &mut preds);
                }
            }
        }
        Self { preds }
    }

    /// The number of steps.
    #[must_use]
    pub fn len(&self) -> usize {
        self.preds.len()
    }

    /// Whether there are no steps.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.preds.is_empty()
    }

    /// Every step each step waits for, directly or not.
    #[must_use]
    pub fn closure(&self) -> Vec<BTreeSet<usize>> {
        let mut out: Vec<BTreeSet<usize>> = Vec::with_capacity(self.len());
        for p in &self.preds {
            let mut all = BTreeSet::new();
            for &i in p.keys() {
                all.insert(i);
                all.extend(out[i].iter().copied());
            }
            out.push(all);
        }
        out
    }

    /// The direct edges not implied by a longer path (the transitive reduction): `[j] -> {i}`.
    #[must_use]
    pub fn reduced(&self) -> Vec<BTreeSet<usize>> {
        let cl = self.closure();
        (0..self.len())
            .map(|j| {
                let direct: BTreeSet<usize> = self.preds[j].keys().copied().collect();
                direct
                    .iter()
                    .copied()
                    .filter(|&i| !direct.iter().any(|&k| k != i && cl[k].contains(&i)))
                    .collect()
            })
            .collect()
    }

    /// The longest path's time ending at each step (its own time included), given each step's time.
    #[must_use]
    pub fn finish_times(&self, secs: &[f64]) -> Vec<f64> {
        let mut out: Vec<f64> = Vec::with_capacity(self.len());
        for j in 0..self.len() {
            let before = self.preds[j].keys().map(|&i| out[i]).fold(0.0, f64::max);
            out.push(before + secs[j]);
        }
        out
    }

    /// The longest path's time from each step to the end (its own time included): the order to start ready steps
    /// in.
    #[must_use]
    pub fn remaining_times(&self, secs: &[f64]) -> Vec<f64> {
        let n = self.len();
        let mut succ: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (j, p) in self.preds.iter().enumerate() {
            for &i in p.keys() {
                succ[i].push(j);
            }
        }
        let mut out = vec![0.0; n];
        for i in (0..n).rev() {
            let after = succ[i].iter().map(|&j| out[j]).fold(0.0, f64::max);
            out[i] = after + secs[i];
        }
        out
    }

    /// The edges `i -> j` of this graph that `other` does not imply (not in its closure).
    #[must_use]
    pub fn missing_from(&self, other: &Self) -> Vec<(usize, usize, BTreeSet<Kind>)> {
        let cl = other.closure();
        let mut out = Vec::new();
        for (j, p) in self.preds.iter().enumerate() {
            for (&i, k) in p {
                if j >= cl.len() || !cl[j].contains(&i) {
                    out.push((i, j, k.clone()));
                }
            }
        }
        out
    }
}
