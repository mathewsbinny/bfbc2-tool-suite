//! `bfbc2-auto-navkit\tools\navcompare.py`: two path-data files of a level compared on its AI order positions
//! (`compare`, `report`), and its command line.

use super::ordercheck::{PathData, Point, order_points};
use crate::pyfmt::{self, float_repr, round_to};
use crate::pyjson::{self, Value};
use crate::pyproc::{Py, PyResult};

fn main_network(d: &PathData, points: &[Point]) -> usize {
    let largest = d.biggest();
    points
        .iter()
        .filter(|(_, _, p)| {
            d.nearest_group(*p)
                .is_some_and(|(_, s)| s > 0 && s as f64 >= 0.25 * largest as f64)
        })
        .count()
}

fn int(n: usize) -> Value {
    Value::Int(n as i128)
}

/// `compare(level_xml, reference, candidate)`.
pub fn compare(level_xml: &str, reference: &str, candidate: &str) -> PyResult<Value> {
    let points = order_points(level_xml)?;
    let rf = PathData::new(reference)?;
    let cd = PathData::new(candidate)?;
    let on_ref: Vec<bool> = points.iter().map(|(_, _, p)| rf.on_area(*p)).collect();
    let on_cand: Vec<bool> = points.iter().map(|(_, _, p)| cd.on_area(*p)).collect();
    let group = |d: &PathData, on: &[bool]| -> Vec<Option<u32>> {
        points
            .iter()
            .enumerate()
            .map(|(i, (_, _, p))| if on[i] { d.nearest_group(*p).map(|g| g.0) } else { None })
            .collect()
    };
    let (gr, gc) = (group(&rf, &on_ref), group(&cd, &on_cand));
    let both = on_ref.iter().zip(&on_cand).filter(|(a, b)| **a && **b).count();
    let only = on_ref.iter().zip(&on_cand).filter(|(a, b)| **b && !**a).count();
    let idx: Vec<usize> = (0..points.len()).filter(|&i| gr[i].is_some()).collect();
    let (mut pairs, mut connected) = (0usize, 0usize);
    for a in 0..idx.len() {
        for b in a + 1..idx.len() {
            let (x, y) = (idx[a], idx[b]);
            if gr[x] == gr[y] {
                pairs += 1;
                if gc[x].is_some() && gc[x] == gc[y] {
                    connected += 1;
                }
            }
        }
    }
    let covers = on_ref.iter().filter(|x| **x).count();
    let file = |path: &str, d: &PathData| {
        Value::dict([
            ("file", Value::Str(path.to_owned())),
            ("sectors", int(d.sectors)),
            ("nodes", int(d.nodes)),
            ("edges", int(d.edges)),
        ])
    };
    Ok(Value::dict([
        ("positions", int(points.len())),
        ("reference_covers", int(covers)),
        ("candidate_covers_of_those", int(both)),
        (
            "coverage_percent",
            Value::Float(round_to(both as f64 / covers.max(1) as f64 * 100.0, 1)),
        ),
        ("candidate_only", int(only)),
        (
            "main_network",
            Value::dict([
                ("reference", int(main_network(&rf, &points))),
                ("candidate", int(main_network(&cd, &points))),
            ]),
        ),
        (
            "pairwise",
            Value::dict([
                ("connected", int(connected)),
                ("pairs", int(pairs)),
                (
                    "percent",
                    Value::Float(round_to(connected as f64 / pairs.max(1) as f64 * 100.0, 1)),
                ),
            ]),
        ),
        (
            "files",
            Value::dict([("reference", file(reference, &rf)), ("candidate", file(candidate, &cd))]),
        ),
    ]))
}

fn n(v: &Value, path: &[&str]) -> String {
    let mut cur = v;
    for k in path {
        match cur.get(k) {
            Some(x) => cur = x,
            None => return String::new(),
        }
    }
    match cur {
        Value::Int(i) => i.to_string(),
        Value::Float(f) => float_repr(*f),
        _ => String::new(),
    }
}

/// `report(r, log)`: its three lines.
pub fn report(r: &Value) -> Vec<String> {
    vec![
        format!(
            "{} order positions; the reference covers {}, the candidate {} of those ({} %) + {} more",
            n(r, &["positions"]),
            n(r, &["reference_covers"]),
            n(r, &["candidate_covers_of_those"]),
            n(r, &["coverage_percent"]),
            n(r, &["candidate_only"])
        ),
        format!(
            "main route network: reference {}, candidate {}; pairwise {}/{} ({} %)",
            n(r, &["main_network", "reference"]),
            n(r, &["main_network", "candidate"]),
            n(r, &["pairwise", "connected"]),
            n(r, &["pairwise", "pairs"]),
            n(r, &["pairwise", "percent"])
        ),
        format!(
            "sectors {} / {}; nodes {} / {}; edges {} / {}",
            n(r, &["files", "reference", "sectors"]),
            n(r, &["files", "candidate", "sectors"]),
            n(r, &["files", "reference", "nodes"]),
            n(r, &["files", "candidate", "nodes"]),
            n(r, &["files", "reference", "edges"]),
            n(r, &["files", "candidate", "edges"])
        ),
    ]
}

/// `navcompare.py <level.xml> <reference.hpd> <candidate.hpd> [--json]`.
pub fn run(py: &mut Py, args: &[String]) -> PyResult<i32> {
    let pos: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    if pos.len() != 3 {
        py.print(&pyfmt::docstring(include_str!(
            "../../../../bfbc2-auto-navkit/tools/navcompare.py"
        )))?;
        return Ok(1);
    }
    let r = compare(pos[0], pos[1], pos[2])?;
    if args.iter().any(|a| a == "--json") {
        py.print(&pyjson::dumps(&r, Some(1)))?;
    } else {
        for l in report(&r) {
            py.print(&l)?;
        }
    }
    Ok(0)
}
