//! `bfbc2-auto-navkit\tools\ordercheck.py`: a level's AI order positions against path-data files (on an area, the
//! nearest route node's group), on the core's `measures::order_areas` / `on_area` (round E4e).

use std::collections::HashMap;
use std::fmt::Write as _;

use bc2_formats::hpd;
use bc2_navbuild::measures;
use regex::Regex;

use super::levelobjects::{field_ref, instances, transform};
use super::read_text_replace;
use crate::pyfmt::{self, round_to};
use crate::pyos;
use crate::pyproc::{Fail, Py, PyErr, PyResult};

pub const Y_OFFSET: f64 = 0.95;

/// A position: (kind, name, (x, y, z)).
pub type Point = (String, String, [f64; 3]);

fn pos(body: &str) -> PyResult<Option<[f64; 3]>> {
    let Some(v) = transform(body)? else {
        return Ok(None);
    };
    let far = v
        .get(12..15.min(v.len()))
        .unwrap_or_default()
        .iter()
        .any(|c| c.abs() > 1e-3);
    if !far {
        return Ok(None);
    }
    if v.len() < 15 {
        return Err(Fail::Err(PyErr::new("IndexError", "list index out of range")));
    }
    Ok(Some([v[12], v[13], v[14]]))
}

fn is_guid(s: &str) -> bool {
    s.len() == 36
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b) || b == b'-')
}

/// `order_points(xml_path)`.
pub fn order_points(xml_path: &str) -> PyResult<Vec<Point>> {
    let x = read_text_replace(xml_path)?;
    let mut order: Vec<String> = Vec::new();
    let mut inst: HashMap<String, (String, &str)> = HashMap::new();
    for (typ, guid, body) in instances(&x, None)? {
        let g = guid.to_lowercase();
        if !inst.contains_key(&g) {
            order.push(g.clone());
        }
        inst.insert(g, (typ, body));
    }
    let name_re = Regex::new(r#"<field name="Name">([^<]*)</field>"#).map_err(|_| Fail::Python)?;
    let ref_re = Regex::new(r#"ref="([0-9a-f-]{36})""#).map_err(|_| Fail::Python)?;
    let mut pts: Vec<Point> = Vec::new();
    for g0 in &order {
        let (typ, body) = &inst[g0];
        let short = typ.rsplit('.').next().unwrap_or("");
        if !matches!(
            short,
            "AreaOrderEntityData" | "PatrolOrderEntityData" | "InterludeOrderEntityData" | "FireAtAreaOrderEntityData"
        ) {
            continue;
        }
        let name = name_re
            .captures(body)
            .and_then(|c| c.get(1))
            .map_or(String::new(), |m| m.as_str().to_owned());
        if let Some(p) = pos(body)? {
            pts.push((short.replace("EntityData", ""), name.clone(), p));
        }
        if short == "PatrolOrderEntityData" {
            for c in ref_re.captures_iter(body) {
                let mut seen: Vec<String> = Vec::new();
                let mut g = c.get(1).map_or("", |m| m.as_str()).to_lowercase();
                while let Some((t, b)) = inst.get(&g) {
                    if seen.contains(&g) || !t.ends_with("AreaGeometryEntityData") {
                        break;
                    }
                    seen.push(g.clone());
                    if let Some(q) = pos(b)? {
                        pts.push(("PatrolWaypoint".to_owned(), name.clone(), q));
                    }
                    let nxt = field_ref(b, "Next")?;
                    g = match nxt {
                        Some(n) if !n.is_empty() && is_guid(&n) => n.to_lowercase(),
                        _ => String::new(),
                    };
                }
            }
        }
    }
    let mut out: Vec<Point> = Vec::new();
    let mut seen: Vec<(String, u64, u64)> = Vec::new();
    for (k, n, p) in pts {
        if p.iter().any(|v| !v.is_finite()) {
            return Err(Fail::Python);
        }
        let key = (k.clone(), round_to(p[0], 1).to_bits(), round_to(p[2], 1).to_bits());
        // -0.0 and 0.0 are one key in Python
        let key = (key.0, norm0(key.1), norm0(key.2));
        if !seen.contains(&key) {
            seen.push(key);
            out.push((k, n, p));
        }
    }
    Ok(out)
}

fn norm0(bits: u64) -> u64 {
    if f64::from_bits(bits) == 0.0 { 0 } else { bits }
}

/// `ordercheck.PathData(path)`.
pub struct PathData {
    pub ids: Vec<u32>,
    pub pos: Vec<[f64; 3]>,
    pub group: HashMap<u32, u32>,
    pub group_size: HashMap<u32, usize>,
    group_order: Vec<u32>,
    pub areas: Vec<measures::OrderArea>,
    pub sectors: usize,
    pub nodes: usize,
    pub edges: usize,
}

impl PathData {
    pub fn new(path: &str) -> PyResult<Self> {
        let data = pyos::read(path)?;
        let f = hpd::parse(&data).map_err(|_| Fail::Python)?;
        let mut index: HashMap<u32, usize> = HashMap::new();
        let mut ids: Vec<u32> = Vec::new();
        let mut parent: Vec<usize> = Vec::new();
        let mut at = |id: u32, ids: &mut Vec<u32>, parent: &mut Vec<usize>| -> usize {
            *index.entry(id).or_insert_with(|| {
                ids.push(id);
                parent.push(parent.len());
                parent.len() - 1
            })
        };
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        let (mut nodes, mut pos) = (Vec::new(), Vec::new());
        let mut n_edges = 0;
        for s in &f.sectors {
            for e in &s.edges {
                let (Some(na), Some(nb)) = (s.nodes.get(e.src as usize), s.nodes.get(e.dst as usize)) else {
                    return Err(Fail::Python);
                };
                let a = at(na.id, &mut ids, &mut parent);
                let b = at(nb.id, &mut ids, &mut parent);
                pairs.push((a, b));
            }
            n_edges += s.edges.len();
            for n in &s.nodes {
                nodes.push(n.id);
                pos.push(n.pos.map(f64::from));
                at(n.id, &mut ids, &mut parent);
            }
        }
        let find = |parent: &mut Vec<usize>, mut a: usize| {
            while parent[a] != a {
                parent[a] = parent[parent[a]];
                a = parent[a];
            }
            a
        };
        for (a, b) in pairs {
            let ra = find(&mut parent, a);
            let rb = find(&mut parent, b);
            parent[ra] = rb;
        }
        let mut group = HashMap::new();
        let mut group_size: HashMap<u32, usize> = HashMap::new();
        let mut group_order = Vec::new();
        for &nid in &nodes {
            if group.contains_key(&nid) {
                continue; // {nid: root}: a border node met again is one entry
            }
            let r = find(&mut parent, index[&nid]);
            let g = ids[r];
            group.insert(nid, g);
            let c = group_size.entry(g).or_insert(0);
            if *c == 0 {
                group_order.push(g);
            }
            *c += 1;
        }
        let meshes: Vec<&[u8]> = f.sectors.iter().map(|s| s.mesh).collect();
        let areas = measures::order_areas(&meshes, Y_OFFSET).ok_or(Fail::Python)?;
        Ok(Self {
            sectors: f.sectors.len(),
            nodes: nodes.len(),
            edges: n_edges,
            ids: nodes,
            pos,
            group,
            group_size,
            group_order,
            areas,
        })
    }

    /// `on_area(p)`.
    pub fn on_area(&self, p: [f64; 3]) -> bool {
        measures::on_area(&self.areas, (p[0], p[1], p[2]), 2.5)
    }

    /// `nearest_group(p)`: (group, its size) or None.
    pub fn nearest_group(&self, p: [f64; 3]) -> Option<(u32, usize)> {
        if self.ids.is_empty() {
            return None;
        }
        // np.argmin: the first minimum, or the first NaN
        let mut best = (0usize, f64::INFINITY);
        for (k, q) in self.pos.iter().enumerate() {
            let near = (q[1] - Y_OFFSET - p[1]).abs() <= 4.0;
            let d = if near { (q[0] - p[0]).hypot(q[2] - p[2]) } else { 1e9 };
            if d.is_nan() {
                best = (k, d);
                break;
            }
            if k == 0 || d < best.1 {
                best = (k, d);
            }
        }
        if best.1 > 15.0 {
            return None;
        }
        let g = self.group[&self.ids[best.0]];
        Some((g, self.group_size[&g]))
    }

    /// `max(group_size.values(), default=0)`.
    pub fn biggest(&self) -> usize {
        self.group_order.iter().map(|g| self.group_size[g]).max().unwrap_or(0)
    }
}

/// `{x:w.pf}` as Python formats it.
fn f(x: f64, w: usize, p: usize) -> String {
    format!("{x:w$.p$}")
}

/// `ordercheck.py <level.xml> <a.hpd> [<b.hpd> ...]`.
pub fn run(py: &mut Py, args: &[String]) -> PyResult<i32> {
    if args.len() < 2 {
        py.print(&pyfmt::docstring(include_str!(
            "../../../../bfbc2-auto-navkit/tools/ordercheck.py"
        )))?;
        return Ok(1);
    }
    let pts = order_points(&args[0])?;
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for (k, _, _) in &pts {
        match counts.iter_mut().find(|(x, _)| x == k) {
            Some(c) => c.1 += 1,
            None => counts.push((k, 1)),
        }
    }
    let mut d = String::from("{");
    for (i, (k, n)) in counts.iter().enumerate() {
        let _ = write!(d, "{}{}: {n}", if i > 0 { ", " } else { "" }, pyfmt::str_repr(k));
    }
    d.push('}');
    py.print(&format!("{} order / waypoint positions: {d}", pts.len()))?;
    let files = &args[1..];
    // results: a dict by path (a path given twice is one entry, its last run's)
    let mut results: Vec<(String, Vec<(bool, Option<usize>)>)> = Vec::new();
    for path in files {
        let pd = PathData::new(path)?;
        let biggest = pd.biggest();
        let res: Vec<(bool, Option<usize>)> = pts
            .iter()
            .map(|(_, _, p)| (pd.on_area(*p), pd.nearest_group(*p).map(|g| g.1)))
            .collect();
        let on = res.iter().filter(|r| r.0).count();
        let near = res.iter().filter(|r| r.1.is_some_and(|s| s > 0)).count();
        let big = res
            .iter()
            .filter(|r| r.1.is_some_and(|s| s > 0 && s as f64 >= 0.25 * biggest as f64))
            .count();
        let up = pyos::basename(&pyos::dirname(&pyos::dirname(path)?)?)?;
        py.print(&format!(
            "{}: {}  on an area: {on}/{}  node within 15 m: {near}  in a large route group (>= 25% of largest): {big}",
            if up.is_empty() { path.clone() } else { up },
            pyos::basename(path)?,
            pts.len()
        ))?;
        match results.iter_mut().find(|(p, _)| p == path) {
            Some(slot) => slot.1 = res,
            None => results.push((path.clone(), res)),
        }
    }
    let mut groups: Vec<(String, Vec<Option<u32>>)> = Vec::new();
    for path in files {
        let pd = PathData::new(path)?;
        let g: Vec<Option<u32>> = pts
            .iter()
            .map(|(_, _, p)| {
                if pd.on_area(*p) {
                    pd.nearest_group(*p).map(|x| x.0)
                } else {
                    None
                }
            })
            .collect();
        match groups.iter_mut().find(|(p, _)| p == path) {
            Some(slot) => slot.1 = g,
            None => groups.push((path.clone(), g)),
        }
    }
    let get = |p: &str| {
        groups
            .iter()
            .find(|(x, _)| x == p)
            .map_or_else(Vec::new, |x| x.1.clone())
    };
    if files.len() == 1 {
        let g = get(&files[0]);
        let idx: Vec<usize> = (0..pts.len()).filter(|&i| g[i].is_some()).collect();
        let total = idx.len() * idx.len().saturating_sub(1) / 2;
        let mut conn = 0usize;
        for a in 0..idx.len() {
            for b in a + 1..idx.len() {
                conn += usize::from(g[idx[a]] == g[idx[b]]);
            }
        }
        py.print(&format!(
            "self-check: {conn}/{total} point pairs on areas are route-connected ({}%)",
            f(conn as f64 / total.max(1) as f64 * 100.0, 0, 1)
        ))?;
    }
    if files.len() == 2 {
        let (gd, go) = (get(&files[0]), get(&files[1]));
        let idx: Vec<usize> = (0..pts.len()).filter(|&i| gd[i].is_some()).collect();
        let (mut rf, mut both, mut extra) = (0usize, 0usize, 0usize);
        for a in 0..idx.len() {
            for b in a + 1..idx.len() {
                let (x, y) = (idx[a], idx[b]);
                let dc = gd[x] == gd[y];
                let oc = go[x].is_some() && go[x] == go[y];
                rf += usize::from(dc);
                both += usize::from(dc && oc);
                extra += usize::from(oc && !dc);
            }
        }
        py.print(&format!(
            "pairwise route check: {both}/{rf} reference-connected pairs also connected ({}%); {extra} pairs \
             connected only in the second file",
            f(both as f64 / rf.max(1) as f64 * 100.0, 0, 1)
        ))?;
    }
    if results.len() == 2 {
        let (pa, ra) = &results[0];
        let (pb, rb) = &results[1];
        let diff: Vec<usize> = (0..pts.len())
            .filter(|&i| ra[i].0 != rb[i].0 || ra[i].1.is_some_and(|s| s > 0) != rb[i].1.is_some_and(|s| s > 0))
            .collect();
        py.print(&format!(
            "\npoints that differ ({}): first = {}, second = {}",
            diff.len(),
            pyos::basename(pa)?,
            pyos::basename(pb)?
        ))?;
        let size = |s: Option<usize>| s.map_or("None".to_owned(), |v| v.to_string());
        let tf = |b: bool| if b { "True" } else { "False" };
        for &i in diff.iter().take(60) {
            let (k, nm, p) = &pts[i];
            let nm: String = nm.chars().take(34).collect();
            py.print(&format!(
                "  {k:<22} {nm:<34} ({} {} {})  on-area {:<5} / {:<5}   group size {} / {}",
                f(p[0], 8, 1),
                f(p[1], 6, 1),
                f(p[2], 8, 1),
                tf(ra[i].0),
                tf(rb[i].0),
                size(ra[i].1),
                size(rb[i].1)
            ))?;
        }
    }
    Ok(0)
}
