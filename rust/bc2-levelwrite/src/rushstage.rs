//! Whole Rush stages added and taken out (round E4-rest, `EDITOR_ARCHITECTURE.md` 3.4, spec GAME-RUSH-11).
//!
//! What DICE's data says (30 Rush and Squad Rush levels of every set; the Onslaught install's `ons_mp_005gr` is
//! wired otherwise and is refused):
//!
//! - Every stage has one **base counter**: the `MultipleTriggerEntityData` its M-COMs reach whose `OnTrigger` takes a
//!   base from the defenders (`DestructBase` on their `LifeCounterEntityData`, BOT-GS-8). There are `BaseCount` of
//!   them.
//! - The **enable chain**: stage 1's base counter starts enabled, every later one disabled; stage k's base counter's
//!   `OnTrigger` enables stage k+1's and nothing else enables it; the last stage's enables no counter.
//! - What is **in play only during a stage** (its own: the defenders' base and its spawns, the attackers' forward base,
//!   the two combat areas, the stage's counters, markers and interactions; [`StageParts::own`]) is switched on by the
//!   previous stage's base counter (the forward base and the defenders' area directly, the defenders' base through the
//!   new base counter's `OnEnabled`, the attackers' area through its hold-back timer) and off by the stage's own base
//!   counter, or by the next stage's objects as they come into play (a combat area's `OnEnabled` disables the one
//!   before).
//!
//! So a stage is its M-COMs with their units, its own objects and what only they own and reach ([`StageParts::set`]),
//! wired to the stage before by the connections between the two stages' sets. A stage is **added** after the last as
//! a copy of the last stage's set, moved to where the user clicks, wired to the level's shared logic as the last
//! stage is, and to the last stage by the last stage's wiring to the one before shifted by one stage (each object of
//! the stage before mapped to its counterpart in the last: [`counterparts`]); the defenders' `BaseCount` counts one
//! more. A stage is **taken out** with its set: what its completion did (its base counter's events and the rest of
//! its set's events on what stays) is done by the stage before's completion, what its objects did as they came into
//! play by the next stage's counterparts; taking out stage 1 starts the round as stage 2 started (every remaining
//! object's `Enabled` as it was during stage 2); `BaseCount` counts one fewer.
//!
//! Each is one operation of the log ([`Op::Files`]: the copies or the deletes, [`Op::Connect`], the fields), and each
//! is checked as it is made: the level's stages are then played ([`crate::modelayer::rush_play`]) in an order with no
//! ambiguity, the new stage last with its copies in play as the last stage's objects were, every other stage as
//! before. Where the shifted wiring leaves an object of the last stage in play during the new one (or the reverse),
//! the last stage's base counter switches it (`Enable` / `Disable`).

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::files::FileEdit;
use crate::modelayer::{self, RushStage};
use crate::ops::{Connection, FieldSet, FieldValue, Op, Operation, Placed, Rows, SubCopy};
use crate::schema::{FieldType, Schema};
use crate::write::{LevelWriteError, LevelWriter};

type Result<T> = std::result::Result<T, LevelWriteError>;

const LIFE_COUNTER: &str = ".LifeCounterEntityData";

fn refused(why: impl Into<String>) -> LevelWriteError {
    LevelWriteError::Refused(why.into())
}

/// The guid a reference names (its last part), upper case.
fn named(r: &[u8]) -> String {
    String::from_utf8_lossy(r.rsplit(|&c| c == b'/').next().unwrap_or(r)).to_ascii_uppercase()
}

/// The level's event connections, in order (upper-case guids).
#[must_use]
pub fn connections(w: &LevelWriter) -> Vec<Connection> {
    let mut out = Vec::new();
    for a in w.connection_arrays() {
        for &c in w.d.children(a).unwrap_or_default() {
            let end = |f: &[u8]| w.d.child(c, f).and_then(|x| w.d.attr(x, b"ref")).map(named);
            let ev = |f: &[u8]| {
                w.d.child(c, f)
                    .and_then(|x| w.d.child(x, b"Name"))
                    .and_then(|x| w.d.text(x))
                    .map_or_else(String::new, |t| String::from_utf8_lossy(t).into_owned())
            };
            if let (Some(source), Some(target)) = (end(b"Source"), end(b"Target")) {
                out.push(Connection {
                    source,
                    source_event: ev(b"SourceEvent"),
                    target,
                    target_event: ev(b"TargetEvent"),
                });
            }
        }
    }
    out
}

/// One Rush stage as DICE wires it (GAME-RUSH-11).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StageParts {
    /// Its M-COMs (upper-case guids, the partition's order).
    pub mcoms: Vec<String>,
    /// Its base counter: the counter its M-COMs reach whose `OnTrigger` takes a base from the defenders.
    pub base: String,
    /// What is in play during this stage and no other (instances with an `Enabled` state; partition order).
    pub own: Vec<String>,
    /// The stage's whole set: its M-COMs with their units, its own objects, and what only these own and reach
    /// (partition order).
    pub set: Vec<String>,
}

/// A Rush level's stages as DICE wires them, in play order.
#[derive(Debug, Clone, Default)]
pub struct Stages {
    /// The stages.
    pub stages: Vec<StageParts>,
    /// The play ([`modelayer::rush_play`]): each stage's state.
    pub play: Vec<RushStage>,
    /// Each stage's state as its counters leave it ([`settled`]).
    pub states: Vec<HashMap<String, bool>>,
    /// The defenders' life counter (its `BaseCount` counts the stages).
    pub defenders: String,
}

impl Stages {
    /// The stage whose set holds an instance.
    #[must_use]
    pub fn stage_of(&self, guid: &str) -> Option<usize> {
        let g = guid.to_ascii_uppercase();
        self.stages.iter().position(|s| s.set.contains(&g))
    }
}

fn of_type(w: &LevelWriter, ty: &str) -> Vec<String> {
    w.d.of_type(ty.as_bytes())
        .into_iter()
        .map(|e| named(w.d.attr(e, b"guid").unwrap_or_default()))
        .collect()
}

fn enabled(w: &LevelWriter, g: &str) -> Option<bool> {
    w.find(g).and_then(|e| modelayer::bool_field(w, e, b"Enabled"))
}

/// The level's Rush stages with their parts (GAME-RUSH-11).
///
/// # Errors
/// The level has no Rush stages, its play order is ambiguous, it has no one defenders' life counter, a stage has no
/// one base counter, or the base counters are not chained as DICE's are.
#[allow(
    clippy::too_many_lines,
    reason = "the stages, their counters, chain, own objects and sets in one reading"
)]
pub fn stages(w: &LevelWriter) -> Result<Stages> {
    let (play, uncertain) = modelayer::rush_play(w);
    if play.is_empty() {
        return Err(refused("the level has no Rush stages"));
    }
    if uncertain {
        return Err(refused(
            "the level's Rush stages are played in no certain order: their counters are not wired as DICE's",
        ));
    }
    let conns = connections(w);
    let lifes = of_type(w, LIFE_COUNTER);
    let defenders: Vec<String> = lifes
        .iter()
        .filter(|g| w.find(g).and_then(|e| modelayer::bool_field(w, e, b"Attacker")) != Some(true))
        .cloned()
        .collect();
    let [defenders] = defenders.as_slice() else {
        return Err(refused(format!(
            "the level has {} defenders' life counters (DICE's Rush levels have one)",
            defenders.len()
        )));
    };
    let mut bases = Vec::new();
    for (k, s) in play.iter().enumerate() {
        let b: Vec<String> = crate::gameplay::stage_counters(w, &s.mcoms)
            .into_iter()
            .map(|x| x.0)
            .filter(|c| {
                conns.iter().any(|x| {
                    x.source == *c
                        && x.source_event == "OnTrigger"
                        && x.target == *defenders
                        && x.target_event == "DestructBase"
                })
            })
            .collect();
        let [b] = b.as_slice() else {
            return Err(refused(format!(
                "stage {} has {} counters taking a base from the defenders (DICE's stages have one)",
                k + 1,
                b.len()
            )));
        };
        bases.push(b.clone());
    }
    // the enable chain
    for (k, b) in bases.iter().enumerate() {
        let on = enabled(w, b) != Some(false);
        if on != (k == 0) {
            return Err(refused(format!(
                "stage {}'s base counter starts {}: DICE's first stage's starts enabled, the others disabled",
                k + 1,
                if on { "enabled" } else { "disabled" }
            )));
        }
        let by: Vec<&String> = conns
            .iter()
            .filter(|c| c.target == *b && c.target_event == "Enable")
            .map(|c| &c.source)
            .collect();
        let want: Vec<&String> = if k == 0 {
            Vec::new()
        } else {
            bases.get(k - 1).into_iter().collect()
        };
        if by != want {
            return Err(refused(format!(
                "stage {}'s base counter is not enabled by the stage before's alone",
                k + 1
            )));
        }
    }
    // what is in play during one stage only
    let n = play.len();
    let states = settled(&play, &bases, &conns);
    // in play some time during a stage: as it starts, or switched on while it is played (a timer its M-COMs start:
    // DICE's "Enable spawns in B")
    let during: Vec<HashSet<String>> = play
        .iter()
        .map(|s| {
            let started: HashSet<&String> = conns
                .iter()
                .filter(|c| s.mcoms.contains(&c.source) && c.source_event == "onDestroyed")
                .map(|c| &c.target)
                .filter(|t| !v_type(w, t).ends_with(".MultipleTriggerEntityData"))
                .collect();
            conns
                .iter()
                .filter(|c| started.contains(&c.source) && c.target_event == "Enable")
                .map(|c| c.target.clone())
                .collect()
        })
        .collect();
    let on = |k: usize, g: &str| {
        states.get(k).and_then(|s| s.get(g).copied()).unwrap_or(true) || during.get(k).is_some_and(|d| d.contains(g))
    };
    // what a stage's base counter switches on as the stage ends outlives it
    let outlives = |k: usize, g: &str| {
        conns.iter().any(|c| {
            bases.get(k).is_some_and(|b| c.source == *b)
                && c.source_event == "OnTrigger"
                && c.target == g
                && c.target_event == "Enable"
        })
    };
    let order = w.guids();
    let owns: Vec<Vec<String>> = (0..n)
        .map(|k| {
            order
                .iter()
                .filter(|g| {
                    states.get(k).is_some_and(|s| s.contains_key(*g))
                        && on(k, g)
                        && (0..n).all(|j| j == k || !on(j, g))
                        && !outlives(k, g)
                })
                .cloned()
                .collect()
        })
        .collect();
    let shared: HashSet<String> = lifes.iter().cloned().chain(of_type(w, ".LevelData")).collect();
    let ownership = w.ownership_with_events();
    // each stage's roots: its M-COMs, its counters (they stay enabled once their stage is played: not its own) and
    // its own objects
    let counters: Vec<Vec<String>> = play
        .iter()
        .map(|s| {
            crate::gameplay::stage_counters(w, &s.mcoms)
                .into_iter()
                .map(|x| x.0)
                .collect()
        })
        .collect();
    let roots: Vec<Vec<String>> = (0..n)
        .map(|k| {
            let mut r: Vec<String> = play.get(k).map_or_default(|s| s.mcoms.clone());
            for g in counters
                .get(k)
                .into_iter()
                .flatten()
                .chain(owns.get(k).into_iter().flatten())
            {
                if !r.contains(g) {
                    r.push(g.clone());
                }
            }
            r
        })
        .collect();
    let roots_all = roots.clone();
    let mut raw: Vec<HashSet<String>> = Vec::new();
    for k in 0..n {
        let roots: Vec<String> = roots.get(k).cloned().unwrap_or_default();
        let mut set: HashSet<String> = roots.iter().cloned().collect();
        for c in modelayer::clusters(w, &roots) {
            set.extend(c);
        }
        let all: Vec<String> = set.iter().cloned().collect();
        let others: HashSet<String> = roots_all
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != k)
            .flat_map(|(_, r)| r.iter().cloned())
            .collect();
        set.extend(ownership.owned_from_except(&all, &others));
        set.retain(|g| !shared.contains(g));
        raw.push(set);
    }
    // an instance in two stages' sets stays with the stage it is a root of, else with neither
    let roots: Vec<HashSet<String>> = roots.into_iter().map(|r| r.into_iter().collect()).collect();
    let mut membership: HashMap<&String, usize> = HashMap::new();
    #[allow(clippy::iter_over_hash_type, reason = "counting: the order does not matter")]
    for s in &raw {
        for g in s {
            *membership.entry(g).or_default() += 1;
        }
    }
    let mut parts = Vec::new();
    for (k, s) in play.iter().enumerate() {
        let set: Vec<String> = order
            .iter()
            .filter(|g| {
                raw.get(k).is_some_and(|r| r.contains(*g))
                    && (membership.get(g).copied().unwrap_or(0) == 1 || roots.get(k).is_some_and(|r| r.contains(*g)))
            })
            .cloned()
            .collect();
        parts.push(StageParts {
            mcoms: s.mcoms.clone(),
            base: bases.get(k).cloned().unwrap_or_default(),
            own: owns.get(k).cloned().unwrap_or_default(),
            set,
        });
    }
    Ok(Stages {
        stages: parts,
        play,
        states,
        defenders: defenders.clone(),
    })
}

/// Each stage's state as the stage before's base counter leaves it: the play ([`modelayer::rush_play`]) follows the
/// events of a stage's end in no time, so an object a delayed trigger of the stage's M-COMs enables (DICE's "Enable
/// spawns in B" timers) can come after the base counter's `Disable` of it there; the counter has the last word (its
/// `Enable` / `Disable` of an object set it).
#[must_use]
pub fn settled(play: &[RushStage], bases: &[String], conns: &[Connection]) -> Vec<HashMap<String, bool>> {
    let mut out: Vec<HashMap<String, bool>> = Vec::new();
    for (k, s) in play.iter().enumerate() {
        // what the stage before's end changed in the play, over the stage before as settled
        let mut state = match (out.last(), k.checked_sub(1).and_then(|p| play.get(p))) {
            (Some(prev), Some(raw_prev)) => {
                let mut st = prev.clone();
                #[allow(
                    clippy::iter_over_hash_type,
                    reason = "each object set once: the order does not matter"
                )]
                for (g, on) in &s.enabled {
                    if raw_prev.enabled.get(g) != Some(on) {
                        st.insert(g.clone(), *on);
                    }
                }
                st
            }
            _ => s.enabled.clone(),
        };
        if let Some(b) = k.checked_sub(1).and_then(|p| bases.get(p)) {
            for c in conns.iter().filter(|c| c.source == *b && c.source_event == "OnTrigger") {
                match c.target_event.as_str() {
                    "Enable" => {
                        state.insert(c.target.clone(), true);
                    }
                    "Disable" => {
                        state.insert(c.target.clone(), false);
                    }
                    _ => {}
                }
            }
        }
        out.push(state);
    }
    out
}

/// How an object comes into play: its type, team and enabling roles ([`signature`]).
type Signature = (String, String, BTreeSet<String>);

/// An instance's team as its data names it (`Team`, `InitialOwnerTeam`, `TeamId` or `AllowTeam`).
fn team(w: &LevelWriter, g: &str) -> String {
    let Some(e) = w.find(g) else { return String::new() };
    ["Team", "InitialOwnerTeam", "TeamId", "AllowTeam"]
        .iter()
        .find_map(|f| w.d.child(e, f.as_bytes()).and_then(|x| w.d.text(x)))
        .map_or_else(String::new, |t| String::from_utf8_lossy(t).into_owned())
}

fn short_type(w: &LevelWriter, g: &str) -> String {
    w.find(g).map_or_else(String::new, |e| {
        let t = w.type_of(e);
        t.rsplit('.').next().unwrap_or(&t).to_owned()
    })
}

/// How an object of stage `k` comes into play: its type, team and the connections that enable it, each by its
/// source's role (`B` the stage's base counter, `P` the stage before's, `S:<type>` an object of the stage's set,
/// `O:<type>` any other). Stage 1's objects that nothing of their stage enables come into play with the round, as
/// a later stage's do with the stage before's counter (`P`).
fn signature(w: &LevelWriter, st: &Stages, conns: &[Connection], k: usize, x: &str) -> Signature {
    let Some(s) = st.stages.get(k) else {
        return Default::default();
    };
    let prev = k.checked_sub(1).and_then(|p| st.stages.get(p)).map(|p| p.base.as_str());
    let mut roles: BTreeSet<String> = BTreeSet::new();
    for c in conns.iter().filter(|c| c.target == x && c.target_event == "Enable") {
        let role = if c.source == s.base {
            "B".to_owned()
        } else if Some(c.source.as_str()) == prev {
            "P".to_owned()
        } else if s.set.contains(&c.source) {
            format!("S:{}", short_type(w, &c.source))
        } else {
            format!("O:{}", short_type(w, &c.source))
        };
        roles.insert(format!("{role} {} {}", c.source_event, c.target_event));
    }
    if k == 0 && !roles.iter().any(|r| r.starts_with("B ") || r.starts_with("S:")) {
        roles.insert("P OnTrigger Enable".into());
    }
    (short_type(w, x), team(w, x), roles)
}

/// Each object of stage `from` mapped to its counterpart in stage `to` (round E4-rest): the base counter to the base
/// counter; each own object to the one own object of `to` that comes into play the same way ([`signature`]), else to
/// the one of its type and team. Objects without one counterpart are left out.
#[must_use]
pub fn counterparts(w: &LevelWriter, st: &Stages, from: usize, to: usize) -> HashMap<String, String> {
    let conns = connections(w);
    let mut out = HashMap::new();
    let (Some(a), Some(b)) = (st.stages.get(from), st.stages.get(to)) else {
        return out;
    };
    out.insert(a.base.clone(), b.base.clone());
    let sig_b: Vec<(String, Signature)> = b
        .own
        .iter()
        .filter(|g| **g != b.base)
        .map(|g| (g.clone(), signature(w, st, &conns, to, g)))
        .collect();
    let mut taken: HashSet<String> = HashSet::new();
    let mut left: Vec<(String, Signature)> = Vec::new();
    for x in a.own.iter().filter(|g| **g != a.base) {
        let sx = signature(w, st, &conns, from, x);
        let same: Vec<&String> = sig_b.iter().filter(|(_, s)| *s == sx).map(|(g, _)| g).collect();
        if let [y] = same.as_slice() {
            out.insert(x.clone(), (*y).clone());
            taken.insert((*y).clone());
        } else {
            left.push((x.clone(), sx));
        }
    }
    for (x, sx) in left {
        let same: Vec<&String> = sig_b
            .iter()
            .filter(|(g, s)| !taken.contains(g) && s.0 == sx.0 && s.1 == sx.1)
            .map(|(g, _)| g)
            .collect();
        let rivals = a
            .own
            .iter()
            .filter(|g| !out.contains_key(*g) && **g != x)
            .filter(|g| {
                let s = signature(w, st, &conns, from, g);
                s.0 == sx.0 && s.1 == sx.1
            })
            .count();
        if let [y] = same.as_slice()
            && rivals == 0
        {
            out.insert(x, (*y).clone());
            taken.insert((*y).clone());
        }
    }
    out
}

fn partition(op: Op) -> FileEdit {
    FileEdit::Partition { op: Box::new(op) }
}

/// The defenders' `BaseCount` set `by` more (or fewer).
fn base_count(w: &LevelWriter, schema: &Schema, defenders: &str, by: i64) -> Result<FieldSet> {
    let (ty, from) = w.get_field(schema, defenders, "BaseCount")?;
    let now = match &from {
        Some(FieldValue::Int(i)) => *i,
        #[allow(clippy::cast_possible_truncation, reason = "a handful of stages")]
        Some(FieldValue::Float(x)) => x.round() as i64,
        _ => return Err(refused("the defenders' BaseCount does not read")),
    };
    let n = (now + by).max(0);
    #[allow(clippy::cast_precision_loss, reason = "a handful of stages")]
    let to = match ty {
        FieldType::Float => FieldValue::Float(n as f64),
        _ => FieldValue::Int(n),
    };
    Ok(FieldSet {
        guid: defenders.to_owned(),
        field: "BaseCount".into(),
        from,
        to,
    })
}

fn v_type(w: &LevelWriter, g: &str) -> String {
    w.find(g).map_or_default(|e| w.type_of(e))
}

fn applied(w: &LevelWriter, schema: &Schema, op: &Op) -> Result<LevelWriter> {
    let mut v = w.clone();
    let o = Operation {
        n: 0,
        by: String::new(),
        when: String::new(),
        label: String::new(),
        op: op.clone(),
    };
    crate::write::apply_one(&mut v, &o, schema, &mut crate::write::Applied::default()).map_err(|e| e.error)?;
    Ok(v)
}

/// A stage added: the operation and what it made.
#[derive(Debug, Clone)]
pub struct NewStage {
    /// The operation.
    pub op: Op,
    /// The new stage's M-COMs (upper-case guids).
    pub mcoms: Vec<String>,
    /// The instances copied.
    pub copies: usize,
    /// The event connections added beside the copies' own.
    pub wires: usize,
    /// Connections between the last two stages that have no counterpart to shift to (left out).
    pub unshifted: usize,
}

/// A Rush stage added after the last (round E4-rest, GAME-RUSH-11): a copy of the last stage's set moved so its
/// M-COMs' middle stands at `at` (each copy keeping its height above the ground where `ground(x, z)` gives one),
/// wired as the module documentation says, the defenders' `BaseCount` one more. One operation; returns it with the
/// new stage's M-COMs and counts.
///
/// # Errors
/// [`stages`] refuses the level, it has one stage only (a new stage copies the wiring between the last two), or the
/// new stage does not play as the last one did.
#[allow(
    clippy::too_many_lines,
    reason = "the stage's copy, its wiring and its check in one piece"
)]
pub fn add_stage(
    w: &LevelWriter,
    schema: &Schema,
    at: [f32; 3],
    guid: &dyn Fn(&str) -> String,
    ground: &dyn Fn(f64, f64) -> Option<f64>,
) -> Result<NewStage> {
    let st = stages(w)?;
    let n = st.stages.len();
    if n < 2 {
        return Err(refused(
            "a new stage copies the wiring between the last two stages: the level has one stage",
        ));
    }
    let (Some(prev), Some(last)) = (st.stages.get(n - 2), st.stages.get(n - 1)) else {
        return Err(refused("the level has no Rush stages"));
    };
    let (dup, copy) = copy_of(w, last, at, guid, ground)?;
    let c = |g: &String| copy.get(g).cloned().unwrap_or_else(|| g.clone());
    // the wiring
    let conns = connections(w);
    let have: HashSet<&Connection> = conns.iter().collect();
    let phi = counterparts(w, &st, n - 2, n - 1);
    let in_last: HashSet<&String> = last.set.iter().collect();
    let in_prev: HashSet<&String> = prev.set.iter().collect();
    let in_other: HashSet<&String> = st.stages.iter().take(n - 2).flat_map(|s| s.set.iter()).collect();
    let mut wires: Vec<Connection> = Vec::new();
    let push = |x: Connection, wires: &mut Vec<Connection>| {
        if !have.contains(&x) && !wires.contains(&x) {
            wires.push(x);
        }
    };
    let mut unshifted = 0usize;
    for e in &conns {
        let (s_last, t_last) = (in_last.contains(&e.source), in_last.contains(&e.target));
        let (s_prev, t_prev) = (in_prev.contains(&e.source), in_prev.contains(&e.target));
        let wire = |s: String, t: String| Connection {
            source: s,
            source_event: e.source_event.clone(),
            target: t,
            target_event: e.target_event.clone(),
        };
        if s_last && t_last {
            continue; // the copy's own (the duplicate copies them)
        }
        if s_prev && t_last {
            match phi.get(&e.source) {
                Some(s) => push(wire(s.clone(), c(&e.target)), &mut wires),
                None => unshifted += 1,
            }
        } else if s_last && t_prev {
            match phi.get(&e.target) {
                Some(t) => push(wire(c(&e.source), t.clone()), &mut wires),
                None => unshifted += 1,
            }
        } else if s_last {
            push(wire(c(&e.source), e.target.clone()), &mut wires);
        } else if t_last && !in_other.contains(&e.source) && !s_prev {
            push(wire(e.source.clone(), c(&e.target)), &mut wires);
        }
    }
    let count = base_count(w, schema, &st.defenders, 1)?;
    let make = |wires: &[Connection]| -> Op {
        let mut edits = vec![partition(dup.clone())];
        if !wires.is_empty() {
            edits.push(partition(Op::Connect {
                items: wires.to_vec(),
                connected: true,
            }));
        }
        edits.push(partition(Op::SetField {
            items: vec![count.clone()],
        }));
        Op::Files { edits }
    };
    // the check: the new stage plays last, its copies as the last stage's objects; where it does not, the last
    // stage's base counter switches the object
    let want = |g: &str| -> Option<bool> {
        let state = st.states.get(n - 1)?;
        if last.own.iter().any(|o| o == g) {
            // a counter stays enabled once its stage is played (DICE's never disable one)
            let counter = v_type(w, g).ends_with(".MultipleTriggerEntityData");
            return (!counter).then_some(false);
        }
        if in_last.contains(&g.to_owned()) {
            // the rest of the last stage changes as its M-COMs go (their interactions, voice-over): left to them
            return None;
        }
        if let Some((src, _)) = copy.iter().find(|(_, v)| v.eq_ignore_ascii_case(g)) {
            return state.get(src).copied();
        }
        state.get(g).copied()
    };
    for _round in 0..4 {
        let v = applied(w, schema, &make(&wires))?;
        let (play, uncertain) = modelayer::rush_play(&v);
        if uncertain || play.len() != n + 1 {
            return Err(refused(format!(
                "the new stage would not be played last ({} stages{})",
                play.len(),
                if uncertain { ", in no certain order" } else { "" }
            )));
        }
        // the new stage as the counters leave it (as [`stages`] reads it)
        let bases_v: Vec<String> = st
            .stages
            .iter()
            .map(|s| s.base.clone())
            .chain(std::iter::once(c(&last.base)))
            .collect();
        let states = settled(&play, &bases_v, &connections(&v));
        let Some(now) = states.get(n) else { break };
        let mut wrong: Vec<(String, bool)> = now
            .iter()
            .filter_map(|(g, on)| want(g).filter(|w| w != on).map(|w| (g.clone(), w)))
            .collect();
        wrong.sort();
        // an object the last stage's counter switches already is left to it: the play follows events in no time,
        // so a delayed enable of the stage's own (DICE's "Enable spawns" timers) may come after the counter's
        // disable in it
        let fix = |g: &String, on: bool| Connection {
            source: last.base.clone(),
            source_event: "OnTrigger".into(),
            target: g.clone(),
            target_event: if on { "Enable" } else { "Disable" }.into(),
        };
        wrong.retain(|(g, on)| {
            let f = fix(g, *on);
            !have.contains(&f) && !wires.contains(&f)
        });
        if wrong.is_empty() {
            return Ok(NewStage {
                op: make(&wires),
                mcoms: last.mcoms.iter().map(&c).collect(),
                copies: copy.len(),
                wires: wires.len(),
                unshifted,
            });
        }
        for (g, on) in wrong {
            push(fix(&g, on), &mut wires);
        }
    }
    Err(refused(
        "the new stage's objects would not be in play as the last stage's are",
    ))
}

/// A copy of a stage's set (one `place` of its first M-COM with the rest as its sub-instances, the connections among
/// them copied), moved so its M-COMs' middle stands at `at` (each copy keeping its height above the ground where
/// `ground(x, z)` gives one); with each original's copy (upper-case guids).
fn copy_of(
    w: &LevelWriter,
    stage: &StageParts,
    at: [f32; 3],
    guid: &dyn Fn(&str) -> String,
    ground: &dyn Fn(f64, f64) -> Option<f64>,
) -> Result<(Op, HashMap<String, String>)> {
    let copy: HashMap<String, String> = stage
        .set
        .iter()
        .map(|g| (g.clone(), guid(g).to_ascii_uppercase()))
        .collect();
    let c = |g: &String| copy.get(g).cloned().unwrap_or_else(|| g.clone());
    // the move: the M-COMs' middle to `at`
    let pts: Vec<[f32; 3]> = stage
        .mcoms
        .iter()
        .filter_map(|m| w.transform(m))
        .map(|r| r[3])
        .collect();
    if pts.is_empty() {
        return Err(refused("the stage's M-COMs have no position"));
    }
    #[allow(clippy::cast_precision_loss, reason = "a handful of M-COMs")]
    let mid = {
        let k = pts.len() as f32;
        let s = pts
            .iter()
            .fold([0.0f32; 3], |a, p| [a[0] + p[0], a[1] + p[1], a[2] + p[2]]);
        [s[0] / k, s[1] / k, s[2] / k]
    };
    let d = [at[0] - mid[0], at[1] - mid[1], at[2] - mid[2]];
    let moved = |r: Rows| -> Rows {
        let [x, y, z] = r[3];
        let (nx, nz) = (x + d[0], z + d[2]);
        #[allow(clippy::cast_possible_truncation, reason = "transforms are singles")]
        let ny = match (ground(f64::from(x), f64::from(z)), ground(f64::from(nx), f64::from(nz))) {
            (Some(a), Some(b)) => (f64::from(y) - a + b) as f32,
            _ => y + d[1],
        };
        let mut out = r;
        out[3] = [nx, ny, nz];
        out
    };
    let Some(root) = stage.mcoms.first() else {
        return Err(refused("the stage has no M-COM"));
    };
    let root_rows = w
        .transform(root)
        .ok_or_else(|| refused("the stage's M-COM has no position"))?;
    let owned: Vec<SubCopy> = stage
        .set
        .iter()
        .filter(|g| *g != root)
        .map(|g| SubCopy {
            source: g.clone(),
            guid: c(g),
            transform: w.transform(g).map(moved),
        })
        .collect();
    // the copies with the connections among them only (a placement's copy: the stage's links to the rest of the
    // level are the wiring)
    let dup = Op::Place {
        items: vec![Placed {
            guid: c(root),
            template: root.clone(),
            blueprint: String::new(),
            ref_field: None,
            owned,
            transform: moved(root_rows),
            fields: Vec::new(),
            placement: None,
        }],
    };
    Ok((dup, copy))
}

/// A Rush stage inserted between two (round E4-rest2, GAME-RUSH-12): after stage `after` (play order, from 0) and
/// before the stage that followed it, as a copy of that next stage's set (a later stage: its objects start out of
/// play as a middle stage's do) moved so its M-COMs' middle stands at `at`. The wiring between the two stages is
/// split in two: what the stage before did to the next stage (its counter enabling the next counter, base, forward
/// base, combat area, timers) it does to the copy, and the copy does to the next stage as the stage before did (each
/// object of the stage before mapped to its counterpart in the next stage, [`counterparts`], then to that one's
/// copy); what the next stage's objects did to the stage before's as they came into play (a combat area's
/// `OnEnabled` disabling the one before) the copy's do to the stage before's, and the next stage's do to the copy's.
/// The next stage's links to the level's shared logic are copied; its links to the other stages are not. The
/// defenders' `BaseCount` counts one more. Adding after the last is [`add_stage`].
///
/// Checked as it is made: the stages are played in the order `..., after, new, next, ...`, every stage as before
/// (the objects whose state DICE's play leaves uncertain aside), the new one with its copies as the next stage's
/// objects were and the next stage's own objects waiting as they did. Where the split wiring leaves an object
/// otherwise, the counter of the stage before the one it is wrong in switches it (`Enable` / `Disable`).
///
/// # Errors
/// [`stages`] refuses the level, `after` is not a stage followed by another, or the stages would not play so.
#[allow(
    clippy::too_many_lines,
    reason = "the stage's copy, its split wiring and its check in one piece"
)]
pub fn insert_stage(
    w: &LevelWriter,
    schema: &Schema,
    after: usize,
    at: [f32; 3],
    guid: &dyn Fn(&str) -> String,
    ground: &dyn Fn(f64, f64) -> Option<f64>,
) -> Result<NewStage> {
    use std::cmp::Ordering;
    let st = stages(w)?;
    let n = st.stages.len();
    let (Some(prev), Some(next)) = (st.stages.get(after), st.stages.get(after + 1)) else {
        return Err(refused(format!(
            "a stage is inserted between two stages: the level has {n}, and stage {} is {}",
            after + 1,
            if after < n {
                "the last (a stage is added after it)"
            } else {
                "not one of them"
            }
        )));
    };
    let conns = connections(w);
    // what the next stage's counter switches on for a stage after it that is in play in no stage (MP_006's last
    // counter's forward base for a sixth stage) belongs to the stages after it: not copied
    let dormant: HashSet<String> = conns
        .iter()
        .filter(|e| e.source == next.base && e.source_event == "OnTrigger" && e.target_event == "Enable")
        .filter(|e| next.set.contains(&e.target) && st.states.iter().all(|s| s.get(&e.target) == Some(&false)))
        .flat_map(|e| modelayer::cluster(w, &e.target))
        .collect();
    let mut copied = next.clone();
    copied.set.retain(|g| !dormant.contains(g));
    let (dup, copy) = copy_of(w, &copied, at, guid, ground)?;
    let c = |g: &String| copy.get(g).cloned().unwrap_or_else(|| g.clone());
    let have: HashSet<&Connection> = conns.iter().collect();
    // each object of the stage before to its counterpart in the next stage (then to that one's copy)
    let phi = counterparts(w, &st, after, after + 1);
    let in_next: HashSet<&String> = copied.set.iter().collect();
    let in_prev: HashSet<&String> = prev.set.iter().collect();
    let in_other: HashSet<&String> = st
        .stages
        .iter()
        .enumerate()
        .filter(|(k, _)| *k != after && *k != after + 1)
        .flat_map(|(_, s)| s.set.iter())
        .chain(dormant.iter())
        .collect();
    let mut wires: Vec<Connection> = Vec::new();
    let mut gone: Vec<Connection> = Vec::new();
    let push = |x: Connection, wires: &mut Vec<Connection>| {
        if !have.contains(&x) && !wires.contains(&x) {
            wires.push(x);
        }
    };
    let mut unshifted = 0usize;
    for e in &conns {
        let (s_next, t_next) = (in_next.contains(&e.source), in_next.contains(&e.target));
        let (s_prev, t_prev) = (in_prev.contains(&e.source), in_prev.contains(&e.target));
        let wire = |s: String, t: String| Connection {
            source: s,
            source_event: e.source_event.clone(),
            target: t,
            target_event: e.target_event.clone(),
        };
        if s_next && t_next {
            continue; // the copy's own (the placement copies them)
        }
        if s_prev && t_next {
            // the stage before to the next: to the copy; the copy to the next in its place
            push(wire(e.source.clone(), c(&e.target)), &mut wires);
            match phi.get(&e.source) {
                Some(s) => {
                    push(wire(c(s), e.target.clone()), &mut wires);
                    gone.push(e.clone());
                }
                None => unshifted += 1,
            }
        } else if s_next && t_prev {
            // the next stage's objects acting on the stage before's: the copy's do; the next stage's act on the copy
            push(wire(c(&e.source), e.target.clone()), &mut wires);
            match phi.get(&e.target) {
                Some(t) => {
                    push(wire(e.source.clone(), c(t)), &mut wires);
                    gone.push(e.clone());
                }
                None => unshifted += 1,
            }
        } else if s_next && !in_other.contains(&e.target) {
            push(wire(c(&e.source), e.target.clone()), &mut wires);
        } else if t_next && !in_other.contains(&e.source) && !s_prev {
            push(wire(e.source.clone(), c(&e.target)), &mut wires);
        }
    }
    let count = base_count(w, schema, &st.defenders, 1)?;
    let make = |wires: &[Connection]| -> Op {
        let mut edits = vec![partition(dup.clone())];
        if !gone.is_empty() {
            edits.push(partition(Op::Connect {
                items: gone.clone(),
                connected: false,
            }));
        }
        if !wires.is_empty() {
            edits.push(partition(Op::Connect {
                items: wires.to_vec(),
                connected: true,
            }));
        }
        edits.push(partition(Op::SetField {
            items: vec![count.clone()],
        }));
        Op::Files { edits }
    };
    let new_mcoms: Vec<String> = next.mcoms.iter().map(&c).collect();
    let back: HashMap<String, String> = copy.iter().map(|(s, x)| (x.clone(), s.clone())).collect();
    let is_counter = |g: &str| v_type(w, g).ends_with(".MultipleTriggerEntityData");
    // a stage's state of an object where DICE's play is certain of it (the play's raw state and the counters' last
    // word agree)
    let certain = |old: usize, g: &str| -> Option<bool> {
        let on = st.states.get(old)?.get(g).copied()?;
        let raw = st.play.get(old)?.enabled.get(g).copied().unwrap_or(true);
        (raw == on).then_some(on)
    };
    // what stage `k` of the new play should have (`None`: not checked): the old stage `k` before the new one, the
    // old `k - 1` after it
    let want = |k: usize, g: &str| -> Option<bool> {
        if let Some(src) = back.get(g) {
            return match k.cmp(&(after + 1)) {
                Ordering::Less => certain(k, src),
                Ordering::Equal => certain(after + 1, src),
                // played: its own out of play (its counters stay enabled)
                Ordering::Greater => (next.own.contains(src) && !is_counter(src)).then_some(false),
            };
        }
        match k.cmp(&(after + 1)) {
            Ordering::Less => certain(k, g),
            Ordering::Equal if next.own.iter().any(|o| o == g) => certain(after, g),
            Ordering::Equal if in_next.contains(&g.to_owned()) => None,
            Ordering::Equal => certain(after + 1, g),
            Ordering::Greater => certain(k - 1, g),
        }
    };
    let order_want: Vec<Vec<String>> = st
        .play
        .iter()
        .take(after + 1)
        .map(|s| s.mcoms.clone())
        .chain(std::iter::once(new_mcoms.clone()))
        .chain(st.play.iter().skip(after + 1).map(|s| s.mcoms.clone()))
        .collect();
    let bases_v: Vec<String> = st
        .stages
        .iter()
        .take(after + 1)
        .map(|s| s.base.clone())
        .chain(std::iter::once(c(&next.base)))
        .chain(st.stages.iter().skip(after + 1).map(|s| s.base.clone()))
        .collect();
    for _round in 0..4 {
        let v = applied(w, schema, &make(&wires))?;
        let (play, uncertain) = modelayer::rush_play(&v);
        let order: Vec<Vec<String>> = play.iter().map(|s| s.mcoms.clone()).collect();
        if uncertain || order != order_want {
            return Err(refused(format!(
                "the new stage would not be played between stages {} and {} ({} stages{})",
                after + 1,
                after + 2,
                play.len(),
                if uncertain { ", in no certain order" } else { "" }
            )));
        }
        let states = settled(&play, &bases_v, &connections(&v));
        let mut wrong: Vec<(usize, String, bool)> = Vec::new();
        for (k, s) in states.iter().enumerate() {
            let mut here: Vec<(usize, String, bool)> = s
                .iter()
                .filter_map(|(g, on)| want(k, g).filter(|x| x != on).map(|x| (k, g.clone(), x)))
                .collect();
            here.sort();
            wrong.extend(here);
        }
        // the counter of the stage before switches it; an object it switches already is left to it (the play follows
        // events in no time: a delayed enable of a stage's own may come after the counter's disable)
        let mut fixes: Vec<Connection> = Vec::new();
        let mut stuck: Vec<String> = Vec::new();
        for (k, g, on) in &wrong {
            let Some(b) = k.checked_sub(1).and_then(|p| bases_v.get(p)) else {
                stuck.push(format!("{g} in stage {}", k + 1));
                continue;
            };
            let f = Connection {
                source: b.clone(),
                source_event: "OnTrigger".into(),
                target: g.clone(),
                target_event: if *on { "Enable" } else { "Disable" }.into(),
            };
            if !have.contains(&f) && !wires.contains(&f) && !fixes.contains(&f) {
                fixes.push(f);
            }
        }
        if !stuck.is_empty() {
            return Err(refused(format!(
                "the stages would not play as before: {}",
                stuck.join(", ")
            )));
        }
        if fixes.is_empty() {
            return Ok(NewStage {
                op: make(&wires),
                mcoms: new_mcoms,
                copies: copy.len(),
                wires: wires.len() + gone.len(),
                unshifted,
            });
        }
        for f in fixes {
            push(f, &mut wires);
        }
    }
    Err(refused(
        "the new stage's objects would not be in play as the next stage's are",
    ))
}

/// The connections from stage `k + 1`'s objects to stage `k`'s, each retargeted to the stage before's counterpart of
/// its target (round E4-rest2: [`remove_stage`] taking out stage `k`, `k` > 0); none for the last stage.
fn handed_back(w: &LevelWriter, st: &Stages, conns: &[Connection], k: usize) -> Vec<Connection> {
    let (Some(gone), Some(next), Some(before)) = (st.stages.get(k), st.stages.get(k + 1), k.checked_sub(1)) else {
        return Vec::new();
    };
    let dead: HashSet<&String> = gone.set.iter().collect();
    let next_set: HashSet<&String> = next.set.iter().collect();
    let back: HashMap<String, String> = counterparts(w, st, before, k)
        .into_iter()
        .map(|(p, x)| (x, p))
        .collect();
    conns
        .iter()
        .filter(|e| next_set.contains(&e.source) && dead.contains(&e.target))
        .filter_map(|e| {
            back.get(&e.target).map(|t| Connection {
                target: t.clone(),
                ..e.clone()
            })
        })
        .collect()
}

/// A Rush stage taken out (round E4-rest, GAME-RUSH-11): stage `k` (play order, from 0) deleted with its set, its
/// wiring handed on as the module documentation says, the defenders' `BaseCount` one fewer. One operation.
///
/// # Errors
/// [`stages`] refuses the level, `k` is not one of its stages, or it is the level's only stage.
#[allow(
    clippy::too_many_lines,
    reason = "the stage's removal, its hand-over and its check in one piece"
)]
pub fn remove_stage(w: &LevelWriter, schema: &Schema, k: usize) -> Result<Op> {
    let st = stages(w)?;
    let n = st.stages.len();
    let Some(gone) = st.stages.get(k) else {
        return Err(refused(format!("the level has no Rush stage {}", k + 1)));
    };
    if n < 2 {
        return Err(refused("a Rush level keeps one stage: this is its only one"));
    }
    let dead: HashSet<&String> = gone.set.iter().collect();
    let conns = connections(w);
    let have: HashSet<&Connection> = conns.iter().collect();
    let mut wires: Vec<Connection> = Vec::new();
    let mut fields: Vec<FieldSet> = Vec::new();
    if k == 0 {
        // the round starts as stage 2 started
        let Some(state) = st.states.get(1) else {
            return Err(refused("the level has no second stage"));
        };
        let mut order: Vec<(&String, &bool)> = state.iter().collect();
        order.sort();
        for (g, on) in order {
            if dead.contains(g) || w.find(g).is_none() {
                continue;
            }
            let now = enabled(w, g);
            if now.is_some() && now != Some(*on) {
                fields.push(FieldSet {
                    guid: g.clone(),
                    field: "Enabled".into(),
                    from: now.map(FieldValue::Bool),
                    to: FieldValue::Bool(*on),
                });
            }
        }
    } else {
        let before = st
            .stages
            .get(k - 1)
            .map(|s| s.base.clone())
            .ok_or_else(|| refused("no stage before"))?;
        let prev_set: HashSet<&String> = st.stages.get(k - 1).map_or_default(|s| s.set.iter().collect());
        let next = if k + 1 < n {
            counterparts(w, &st, k, k + 1)
        } else {
            HashMap::new()
        };
        let mut push = |x: Connection| {
            if x.source != x.target && !have.contains(&x) && !wires.contains(&x) {
                wires.push(x);
            }
        };
        for e in conns
            .iter()
            .filter(|e| dead.contains(&e.source) && !dead.contains(&e.target))
        {
            if e.source_event == "OnEnabled" && prev_set.contains(&e.target) {
                // a stage-k object coming into play acted on the stage before's: the next stage's counterpart does
                if let Some(s) = next.get(&e.source) {
                    push(Connection {
                        source: s.clone(),
                        ..e.clone()
                    });
                }
                continue;
            }
            // what stage k did as it began or ended is done as the stage before ends
            push(Connection {
                source: before.clone(),
                source_event: "OnTrigger".into(),
                target: e.target.clone(),
                target_event: e.target_event.clone(),
            });
        }
        // round E4-rest2: what the next stage's objects did to stage k's (a hold-back timer disabling its combat
        // area) they do to the stage before's counterparts, as they did before a stage was inserted between them
        for x in handed_back(w, &st, &conns, k) {
            push(x);
        }
    }
    fields.push(base_count(w, schema, &st.defenders, -1)?);
    let mut edits = Vec::new();
    if !wires.is_empty() {
        edits.push(partition(Op::Connect {
            items: wires,
            connected: true,
        }));
    }
    edits.push(partition(Op::Delete {
        guids: gone.set.clone(),
    }));
    edits.push(partition(Op::SetField { items: fields }));
    let op = Op::Files { edits };
    // the check: the stages left play in the same order
    let v = applied(w, schema, &op)?;
    let (play, uncertain) = modelayer::rush_play(&v);
    let left: Vec<&Vec<String>> = st
        .stages
        .iter()
        .enumerate()
        .filter(|(j, _)| *j != k)
        .map(|(_, s)| &s.mcoms)
        .collect();
    let now: Vec<&Vec<String>> = play.iter().map(|s| &s.mcoms).collect();
    if uncertain || now != left {
        return Err(refused(
            "the stages left would not be played in their order: the stage's wiring is not DICE's",
        ));
    }
    Ok(op)
}
