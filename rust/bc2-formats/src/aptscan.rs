//! The menu tools' scans of a decoded movie ([`crate::apt`]'s records), ported (round E4c): `aptcheck.Code`'s
//! functions and `findings()` (the rules of `aptcheck check`) and `aptrefs.movie_refs` (what a movie asks of the
//! engine). Both read the records where the decoder made them; no record reaches Python.
//!
//! [`check`] gives what `aptcheck check` reports (the distinct instruction offsets, the functions, the findings
//! sorted as the report sorts them); [`refs`] the dictionaries of `movie_refs` with their keys in Python's insertion
//! order. A value Python would fail to read (a push-byte argument that is no integer) refuses the movie
//! ([`Refused`]); `aptrefs.py` then runs its own code on it. [`check_many`] / [`refs_many`] decode and scan many
//! movies on all cores.
//!
//! ```
//! use bc2_formats::aptscan;
//! assert_eq!(aptscan::hex6(0x1A2B), "001A2B");
//! assert_eq!(aptscan::hex6(-5), "-00005");
//! ```

use std::collections::HashMap;

use rayon::prelude::*;

use crate::apt::{self, Instruction, MovieRef, Record};

/// The scan refused a movie (the Python code runs on it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused;

/// One finding: (kind, key, text).
pub type Finding = (&'static str, i64, String);

/// `aptcheck check`'s numbers and findings of one movie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// Distinct instruction offsets (`len(Code.by_at)`).
    pub instructions: usize,
    /// Function definitions (`len(Code.funcs)`).
    pub functions: usize,
    /// Constants.
    pub constants: usize,
    /// The findings, sorted by (kind, key).
    pub findings: Vec<Finding>,
}

/// Python's `format(v, '06X')`.
#[must_use]
pub fn hex6(v: i64) -> String {
    if v < 0 {
        format!("-{:05X}", v.unsigned_abs())
    } else {
        format!("{v:06X}")
    }
}

fn display(i: &Instruction) -> String {
    apt::display_name(i.op)
}

/// A function: (body start, body end, definition offset, name), as `Code.funcs`.
type Func = (i64, i64, i64, String);

fn funcs(recs: &[Record]) -> Vec<Func> {
    let mut out = Vec::new();
    let mut last: HashMap<i64, &Instruction> = HashMap::new();
    for r in recs {
        let ins = &r.ins;
        if let Some(f) = &ins.func {
            let prev = last.get(&r.start);
            let nm = if f.name.is_empty() {
                match prev {
                    Some(p) if matches!(p.op, 0xA1..=0xA3) && p.arg.starts_with('"') => {
                        p.arg.trim_matches('"').to_string()
                    }
                    _ => "<anon>".to_string(),
                }
            } else {
                f.name.clone()
            };
            out.push((f.body, f.body + i64::from(f.size), ins.at, nm));
        }
        last.insert(r.start, ins);
    }
    out.sort();
    out
}

fn fname(funcs: &[Func], off: i64) -> String {
    let mut best: Option<(i64, i64, &str)> = None;
    for (body, end, at, nm) in funcs {
        if *body <= off && off < *end && best.is_none_or(|b| *body >= b.0) {
            best = Some((*body, *at, nm));
        }
    }
    match best {
        Some((_, at, nm)) => format!("function {nm} ({})", hex6(at)),
        None => "a frame action".to_string(),
    }
}

/// `aptcheck.Code(movie)` and its `findings()`.
#[must_use]
#[expect(clippy::too_many_lines, reason = "aptcheck.Code and findings() in one")]
pub fn check(recs: &[Record], nconst: usize) -> Check {
    let fs = funcs(recs);
    let mut streams: HashMap<i64, std::collections::HashSet<i64>> = HashMap::new();
    for r in recs {
        streams.entry(r.start).or_default().insert(r.ins.at);
    }
    let mut out: HashMap<(&'static str, i64), String> = HashMap::new();
    let mut users: HashMap<u32, Vec<i64>> = HashMap::new();
    for r in recs {
        let ins = &r.ins;
        if let Some(p) = ins.plist {
            users.entry(p).or_default().push(ins.at);
        }
        if ins.unknown {
            out.insert(
                ("unknown-op", ins.at),
                format!(
                    "{} {}: an opcode with an argument the reader does not know",
                    hex6(ins.at),
                    display(ins)
                ),
            );
        }
        if ins.op == 0 && r.depth > 0 {
            out.insert(
                ("end-in-function", ins.at),
                format!("{} End inside {}", hex6(ins.at), fname(&fs, ins.at)),
            );
        }
        if matches!(ins.op, 0x99 | 0x9D | 0xB8)
            && let Some(t) = ins.target
        {
            let ok = streams.get(&r.start).is_some_and(|s| s.contains(&t)) || t == r.limit;
            if !ok {
                out.insert(
                    ("branch-target", ins.at),
                    format!(
                        "{} {} {}: the target is not an instruction of this stream",
                        hex6(ins.at),
                        display(ins),
                        ins.arg
                    ),
                );
            }
        }
        if matches!(ins.op, 0x96 | 0x88)
            && let Some(items) = &ins.items
            && let Some(bad) = items.iter().find(|&&g| g as usize >= nconst)
        {
            out.insert(
                ("const-index", ins.at),
                format!(
                    "{} {}: constant index {bad} past the {nconst} constants",
                    hex6(ins.at),
                    display(ins)
                ),
            );
        }
        if let Some(ix) = ins.index
            && ix as usize >= ins.pool
        {
            out.insert(
                ("const-index", ins.at),
                format!(
                    "{} {}: pool index {ix} past the current pool ({} constants)",
                    hex6(ins.at),
                    display(ins),
                    ins.pool
                ),
            );
        }
    }
    for (plist, ats) in &mut users {
        if ats.len() > 1 {
            ats.sort_unstable();
            let list: Vec<String> = ats.iter().map(|&a| hex6(a)).collect();
            out.insert(
                ("shared-list", i64::from(*plist)),
                format!(
                    "PushData list {plist:#x} used by {} PushData: {}",
                    ats.len(),
                    list.join(", ")
                ),
            );
        }
    }
    let mut ends: HashMap<(i64, i64), i64> = HashMap::new();
    for r in recs {
        if r.depth > 0 {
            let e = match &r.ins.func {
                Some(f) => f.body + i64::from(f.size),
                None => r.ins.end,
            };
            let cur = ends.get(&(r.start, r.limit)).copied().unwrap_or(r.start);
            ends.insert((r.start, r.limit), cur.max(e));
        }
    }
    for (body, end, at, nm) in &fs {
        let got = ends.get(&(*body, *end)).copied().unwrap_or(*body);
        if got != *end {
            out.insert(
                ("body-end", *at),
                format!(
                    "function {nm} at {}: its body decodes to {}, not its end {}",
                    hex6(*at),
                    hex6(got),
                    hex6(*end)
                ),
            );
        }
    }
    let mut findings: Vec<Finding> = out.into_iter().map(|((k, key), t)| (k, key, t)).collect();
    findings.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    let mut ats: Vec<i64> = recs.iter().map(|r| r.ins.at).collect();
    ats.sort_unstable();
    ats.dedup();
    Check {
        instructions: ats.len(),
        functions: fs.len(),
        constants: nconst,
        findings,
    }
}

/// One use of a name: its offset, and for an event (parameters or `None`, how it is handled).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Use {
    /// The instruction's offset.
    pub at: i64,
    /// An event's parameter count and handler.
    pub event: Option<(Option<i64>, String)>,
}

/// One of `movie_refs`' dictionaries: names in insertion order with their uses.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Names {
    /// (name, uses) in insertion order.
    pub items: Vec<(String, Vec<Use>)>,
    ix: HashMap<String, usize>,
}

impl Names {
    fn add(&mut self, name: &str, u: Use) {
        if let Some(&k) = self.ix.get(name) {
            if let Some(e) = self.items.get_mut(k) {
                e.1.push(u);
            }
        } else {
            self.ix.insert(name.to_string(), self.items.len());
            self.items.push((name.to_string(), vec![u]));
        }
    }
}

/// `movie_refs`' keys, in order.
pub const REF_KINDS: [&str; 5] = ["extern", "extern_param", "extern_write", "fscommand", "events"];

/// `aptrefs.string_arg`.
fn string_arg(i: &Instruction) -> Option<&str> {
    let a = i.arg.as_str();
    if matches!(i.op, 0xA1..=0xA3) && a.starts_with('"') && a.ends_with('"') {
        return Some(if a.len() >= 2 {
            a.get(1..a.len() - 1).unwrap_or("")
        } else {
            ""
        });
    }
    if i.op == 0x96
        && a.matches('"').count() == 2
        && a.starts_with('"')
        && !a.split("   [list").next().unwrap_or("").contains(", ")
    {
        return a.split('"').nth(1);
    }
    None
}

fn count_arg(i: &Instruction) -> Result<Option<i64>, Refused> {
    Ok(match i.op {
        0x59 => Some(0),
        0x5A => Some(1),
        0xB5 => Some(i.arg.parse::<i64>().map_err(|_| Refused)?),
        _ => None,
    })
}

/// `aptrefs.movie_refs`: the five dictionaries in [`REF_KINDS`] order.
///
/// # Errors
/// [`Refused`] where Python's `int()` would fail on a push-byte argument.
#[expect(clippy::too_many_lines, reason = "movie_refs' loop, rule by rule")]
pub fn refs(recs: &[Record]) -> Result<[Names; 5], Refused> {
    let mut order: Vec<i64> = Vec::new();
    let mut streams: HashMap<i64, Vec<&Instruction>> = HashMap::new();
    let mut by_name: HashMap<&str, i64> = HashMap::new();
    for r in recs {
        streams
            .entry(r.start)
            .or_insert_with(|| {
                order.push(r.start);
                Vec::new()
            })
            .push(&r.ins);
        if let Some(f) = &r.ins.func
            && !f.name.is_empty()
        {
            by_name.entry(f.name.as_str()).or_insert(i64::from(f.nparams));
        }
    }
    let mut out: [Names; 5] = Default::default();
    let add = |out: &mut [Names; 5], k: usize, name: &str, u: Use| {
        if let Some(n) = out.get_mut(k) {
            n.add(name, u);
        }
    };
    let handler = |ins: Option<&Instruction>| -> (Option<i64>, String) {
        let Some(ins) = ins else {
            return (None, "handler unknown".into());
        };
        if let Some(f) = &ins.func {
            return (Some(i64::from(f.nparams)), "function".into());
        }
        let n = match string_arg(ins) {
            Some(s) if !s.is_empty() => Some(s.to_string()),
            _ if matches!(ins.op, 0xAE | 0xAF) && ins.arg.starts_with('"') => {
                let a = &ins.arg;
                Some(if a.len() >= 2 {
                    a.get(1..a.len() - 1).unwrap_or("").to_string()
                } else {
                    String::new()
                })
            }
            _ => None,
        };
        match n {
            Some(n) if !n.is_empty() => match by_name.get(n.as_str()) {
                Some(&p) => (Some(p), format!("function {n}")),
                None => (None, format!("handler {n}")),
            },
            _ => (None, "handler unknown".into()),
        }
    };
    for start in order {
        let Some(mut l) = streams.remove(&start) else { continue };
        l.sort_by_key(|i| i.at);
        for (k, ins) in l.iter().enumerate() {
            let nxt = l.get(k + 1).copied();
            let s = string_arg(ins);
            if matches!(ins.op, 0xAE | 0xAF | 0xA4 | 0xA5)
                && ins.arg == "\"extern\""
                && let Some(nxt) = nxt
            {
                if matches!(nxt.op, 0xAF | 0xA5 | 0xB2 | 0xB3) && nxt.arg.starts_with('"') {
                    add(
                        &mut out,
                        0,
                        nxt.arg.trim_matches('"'),
                        Use {
                            at: nxt.at,
                            event: None,
                        },
                    );
                } else if let Some(n) = string_arg(nxt).filter(|n| !n.is_empty()) {
                    if n.contains('$') {
                        add(
                            &mut out,
                            1,
                            n.split('$').next().unwrap_or(""),
                            Use {
                                at: nxt.at,
                                event: None,
                            },
                        );
                    } else {
                        let after: Vec<u8> = l.iter().skip(k + 2).take(4).map(|x| x.op).collect();
                        let which = if after.first() == Some(&0x4E) {
                            0
                        } else if after.contains(&0x4F) {
                            2
                        } else {
                            0
                        };
                        add(
                            &mut out,
                            which,
                            n,
                            Use {
                                at: nxt.at,
                                event: None,
                            },
                        );
                    }
                }
            }
            let mut texts: Vec<&str> = Vec::new();
            if let Some(s) = s.filter(|s| !s.is_empty()) {
                texts.push(s);
            }
            if ins.op == 0x83 && ins.arg.starts_with('"') {
                texts.push(ins.arg.split('"').nth(1).unwrap_or(""));
            }
            for t in texts {
                if t.starts_with("FSCommand:") && t.chars().count() > 10 {
                    add(
                        &mut out,
                        3,
                        t.get(10..).unwrap_or(""),
                        Use {
                            at: ins.at,
                            event: None,
                        },
                    );
                }
            }
            if matches!(ins.op, 0xB0..=0xB3) {
                let callee = ins.arg.trim_matches('"').to_lowercase();
                if matches!(callee.as_str(), "fscommand" | "fscommandproxy" | "catchevent") {
                    let back = l.get(k.saturating_sub(5)..k).unwrap_or_default();
                    let skip = usize::from(matches!(ins.op, 0xB2 | 0xB3));
                    if back.len() > skip {
                        for j in (0..back.len() - skip).rev() {
                            let Some(&b) = back.get(j) else { continue };
                            if count_arg(b)?.is_some_and(|c| c >= 1) && j >= 1 {
                                if let Some(nm) = back.get(j - 1).and_then(|x| string_arg(x)).filter(|n| !n.is_empty())
                                {
                                    if callee == "catchevent" {
                                        let h = if j >= 2 { back.get(j - 2).copied() } else { None };
                                        let (params, how) = handler(h);
                                        add(
                                            &mut out,
                                            4,
                                            nm,
                                            Use {
                                                at: ins.at,
                                                event: Some((params, format!("catchEvent {how}"))),
                                            },
                                        );
                                    } else {
                                        add(
                                            &mut out,
                                            3,
                                            nm,
                                            Use {
                                                at: ins.at,
                                                event: None,
                                            },
                                        );
                                    }
                                }
                                break;
                            }
                        }
                    }
                }
            }
            if matches!(ins.op, 0xAE | 0xAF)
                && ins.arg == "\"gameEventListener\""
                && let Some(nxt) = nxt
                && let Some(nm) = string_arg(nxt).filter(|n| !n.is_empty())
                && k + 2 < l.len()
            {
                let (params, how) = handler(l.get(k + 2).copied());
                if params.is_some() {
                    add(
                        &mut out,
                        4,
                        nm,
                        Use {
                            at: nxt.at,
                            event: Some((params, format!("gameEventListener {how}"))),
                        },
                    );
                }
            }
        }
    }
    Ok(out)
}

/// [`check`] of many movies, decoded and checked on all cores; `None` for a movie the decoder refuses.
#[must_use]
pub fn check_many(movies: &[MovieRef<'_>]) -> Vec<Option<Check>> {
    movies
        .par_iter()
        .map(|m| apt::decode(*m, true).ok().map(|d| check(&d.records, m.consts.len())))
        .collect()
}

/// [`refs`] of many movies, decoded and scanned on all cores; `None` for a movie the decoder or the scan refuses.
#[must_use]
pub fn refs_many(movies: &[MovieRef<'_>]) -> Vec<Option<[Names; 5]>> {
    movies
        .par_iter()
        .map(|m| apt::decode(*m, true).ok().and_then(|d| refs(&d.records).ok()))
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn ins(at: i64, op: u8, arg: &str) -> Record {
        Record {
            ins: Instruction {
                at,
                op,
                arg: arg.into(),
                end: at + 1,
                target: None,
                plist: None,
                items: None,
                func: None,
                index: None,
                unknown: false,
                pool: 0,
            },
            start: 0,
            limit: 100,
            depth: 0,
        }
    }

    #[test]
    fn extern_reads() {
        let recs = vec![ins(0, 0xAE, "\"extern\""), ins(2, 0xAF, "\"chId\""), ins(4, 0x00, "")];
        let r = refs(&recs).unwrap();
        assert_eq!(r[0].items, vec![("chId".to_string(), vec![Use { at: 2, event: None }])]);
    }

    #[test]
    fn findings_sorted() {
        let mut a = ins(5, 0xA2, "pool[3]?");
        a.ins.index = Some(3);
        let mut b = ins(1, 0x96, "\"x\"");
        b.ins.items = Some(vec![9]);
        let c = check(&[a, b], 2);
        assert_eq!(c.findings.len(), 2);
        assert_eq!(c.findings[0].1, 1);
        assert!(
            c.findings[1]
                .2
                .contains("pool index 3 past the current pool (0 constants)")
        );
    }
}
