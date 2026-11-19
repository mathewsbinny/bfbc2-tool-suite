//! `bfbc2 graphstats <pathdata.hpd> ...`: NavKit's `tools\graphstats.py` (route-graph components per file) over
//! `bc2_formats::hpd`. A file the core refuses hands the command line to the Python.

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

use bc2_formats::hpd;

use super::pycommon::fixed;
use crate::pyproc::{Fail, Py, PyErr, PyResult, read_file};

fn index_error() -> Fail {
    Fail::Err(PyErr::new("IndexError", "list index out of range"))
}

fn find(parent: &mut HashMap<u32, u32>, mut x: u32) -> u32 {
    loop {
        let p = *parent.entry(x).or_insert(x);
        if p == x {
            return x;
        }
        let gp = *parent.entry(p).or_insert(p);
        parent.insert(x, gp);
        x = gp;
    }
}

pub fn run(py: &mut Py, args: &[String]) -> PyResult<i32> {
    for path in args {
        let data = read_file(path)?;
        let f = hpd::parse(&data).map_err(|_| Fail::Python)?;
        let mut parent: HashMap<u32, u32> = HashMap::new();
        let mut deg: HashMap<u32, usize> = HashMap::new();
        let mut ids: Vec<u32> = Vec::new();
        let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
        for s in &f.sectors {
            for e in &s.edges {
                let a = s.nodes.get(e.src as usize).ok_or_else(index_error)?.id;
                let b = s.nodes.get(e.dst as usize).ok_or_else(index_error)?.id;
                *deg.entry(a).or_insert(0) += 1;
                let ra = find(&mut parent, a);
                let rb = find(&mut parent, b);
                parent.insert(ra, rb);
            }
            for n in &s.nodes {
                if seen.insert(n.id) {
                    ids.push(n.id);
                }
                find(&mut parent, n.id);
            }
        }
        let mut comps: HashMap<u32, usize> = HashMap::new();
        for &i in &ids {
            *comps.entry(find(&mut parent, i)).or_insert(0) += 1;
        }
        let mut sizes: Vec<usize> = comps.into_values().collect();
        sizes.sort_unstable_by(|a, b| b.cmp(a));
        let isolated = ids.iter().filter(|i| deg.get(i).copied().unwrap_or(0) == 0).count();
        let largest = *sizes.first().ok_or_else(index_error)?;
        let name = if path.contains("Output") {
            let parts: Vec<&str> = path.split('/').collect();
            (*parts
                .get(parts.len().wrapping_sub(3))
                .filter(|_| parts.len() >= 3)
                .ok_or_else(index_error)?)
            .to_owned()
        } else {
            path.clone()
        };
        let next: Vec<String> = sizes.iter().skip(1).take(5).map(ToString::to_string).collect();
        py.print(&format!(
            "{name} nodes {} isolated {isolated} components {} largest {largest} ({}%) next [{}]",
            ids.len(),
            sizes.len(),
            fixed(largest as f64 / ids.len() as f64 * 100.0, 1),
            next.join(", ")
        ))?;
    }
    Ok(0)
}
