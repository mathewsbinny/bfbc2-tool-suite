//! `bfbc2 hpdcheck <pathdata.hpd> ...`: NavKit's `tools\hpdcheck.py` (the structural checks DICE's files satisfy)
//! over `bc2_formats::hpd` / `kymesh`. A file or mesh the core refuses, or a NaN where the checks key points by their
//! rounded coordinates, hands the command line to the Python.

#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::format_push_string,
    clippy::type_complexity,
    reason = "a port of Python: indexes after their lengths are checked, ints as Python turns them into floats, == as Python compares floats, the Python's names and its functions' shape kept"
)]

use std::collections::HashMap;

use bc2_formats::{hpd, kymesh};

use crate::pyfmt::{docstring, float_repr, round_to};
use crate::pyos;
use crate::pyproc::{Fail, Py, PyResult, read_file};

const SOURCE: &str = include_str!("../../../../bfbc2-auto-navkit/tools/hpdcheck.py");
const BORDER: u32 = 0x8000_0000;

fn tuple(v: &[f64]) -> String {
    let parts: Vec<String> = v.iter().map(|x| float_repr(*x)).collect();
    format!("({})", parts.join(", "))
}

fn f3(v: [f32; 3]) -> [f64; 3] {
    [f64::from(v[0]), f64::from(v[1]), f64::from(v[2])]
}

/// The bits a rounded coordinate is keyed by (0.0 and -0.0 are one key, as in a Python dict).
fn key_bits(x: f64) -> u64 {
    if x == 0.0 { 0 } else { x.to_bits() }
}

fn detail_problem(a: &kymesh::Area) -> Option<String> {
    if a.d_lines.iter().any(|l| l.len() < 2) {
        return Some("a D line with fewer than 2 points".to_owned());
    }
    for c in &a.cells {
        let [dx, dy, dz] = f3(c.direction);
        if dy.abs() > 1e-6 || (dx * dx + dz * dz - 1.0).abs() > 1e-4 {
            return Some(format!("an F record with direction {}", tuple(&[dx, dy, dz])));
        }
        let closed = match (c.points.first(), c.points.last()) {
            (Some(p), Some(q)) => f3(*p) == f3(*q),
            _ => false,
        };
        if c.points.len() < 4 || !closed {
            return Some("an F polygon not closed".to_owned());
        }
    }
    None
}

const CHECKS: [&str; 5] = [
    "closed boundaries",
    "edges",
    "node order",
    "zero-length edges",
    "detail features",
];

fn check(path: &str) -> PyResult<Vec<Vec<String>>> {
    let data = read_file(path)?;
    let f = hpd::parse(&data).map_err(|_| Fail::Python)?;
    let mut out: Vec<Vec<String>> = vec![Vec::new(); 5];
    for s in &f.sectors {
        let m = kymesh::parse(s.mesh).map_err(|_| Fail::Python)?;
        for (ai, a) in m.areas.iter().enumerate() {
            let mut segs: Vec<([f32; 3], [f32; 3])> = Vec::new();
            for l in &a.outline {
                for k in 0..l.len().saturating_sub(1) {
                    segs.push((l[k], l[k + 1]));
                }
            }
            segs.extend(a.portals.iter().map(|b| (b.p0, b.p1)));
            segs.extend(a.border_portals.iter().map(|c| (c.p0, c.p1)));
            let mut index: HashMap<(u64, u64), usize> = HashMap::new();
            let mut deg: Vec<((f64, f64), usize)> = Vec::new();
            for (p, q) in segs {
                for pt in [p, q] {
                    let x = round_to(f64::from(pt[0]), 2);
                    let z = round_to(f64::from(pt[2]), 2);
                    if x.is_nan() || z.is_nan() {
                        return Err(Fail::Python);
                    }
                    let k = (key_bits(x), key_bits(z));
                    let i = *index.entry(k).or_insert_with(|| {
                        deg.push(((x, z), 0));
                        deg.len() - 1
                    });
                    deg[i].1 += 1;
                }
            }
            let odd: Vec<&(f64, f64)> = deg.iter().filter(|(_, v)| v % 2 == 1).map(|(k, _)| k).collect();
            if let Some(first) = odd.first() {
                out[0].push(format!(
                    "sector {:04x} area {ai}: {} open end(s), e.g. {}",
                    s.id,
                    odd.len(),
                    tuple(&[first.0, first.1])
                ));
            }
            if let Some(bad) = detail_problem(a) {
                out[4].push(format!("sector {:04x} area {ai}: {bad}", s.id));
            }
        }
        let n = s.nodes.len();
        for e in &s.edges {
            let (src, dst) = (e.src as usize, e.dst as usize);
            if src >= n || dst >= n {
                out[1].push(format!("sector {:04x}: edge {} -> {} of {n} nodes", s.id, e.src, e.dst));
                continue;
            }
            let (na, nb) = (&s.nodes[src], &s.nodes[dst]);
            if na.id != nb.id && f3(na.pos) == f3(nb.pos) {
                let r: Vec<f64> = f3(na.pos).iter().map(|v| round_to(*v, 2)).collect();
                out[3].push(format!(
                    "sector {:04x}: nodes {:08x} and {:08x} at {}",
                    s.id,
                    na.id,
                    nb.id,
                    tuple(&r)
                ));
            }
        }
        let key: Vec<(bool, u32)> = s.nodes.iter().map(|nd| (nd.id & BORDER != 0, nd.id)).collect();
        if key.windows(2).any(|w| w[0] > w[1]) {
            out[2].push(format!(
                "sector {:04x}: nodes not sorted by id with border nodes last",
                s.id
            ));
        }
    }
    Ok(out)
}

pub fn run(py: &mut Py, args: &[String]) -> PyResult<i32> {
    if args.is_empty() {
        py.print(&docstring(SOURCE))?;
        return Ok(1);
    }
    let mut bad = 0;
    for path in args {
        let res = check(path)?;
        let fails: Vec<(&str, &Vec<String>)> = CHECKS
            .iter()
            .copied()
            .zip(res.iter())
            .filter(|(_, v)| !v.is_empty())
            .collect();
        if !fails.is_empty() {
            bad += 1;
        }
        let mut line = format!(
            "{} {}",
            if fails.is_empty() { "ok  " } else { "FAIL" },
            pyos::basename(path)?
        );
        for (k, v) in fails {
            line.push_str(&format!("\n       {k}: {} - {}", v.len(), v[0]));
        }
        py.print(&line)?;
    }
    Ok(i32::from(bad > 0))
}
