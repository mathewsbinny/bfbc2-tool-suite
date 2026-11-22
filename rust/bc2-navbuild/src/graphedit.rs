//! `navkit apply`'s edit stage (round NK1): the links and route edges of a navigation side file written into a
//! path-data file's route graph (`navlinks.py`), and its traversal annotations as `<level>.traversal.json`
//! (`navtraversal.py`) - the Python of both retired in round NK1 against their recorded answers
//! (`core/tests/cli_o4_navkit.py`'s apply cases, `rust/BENCHMARKS.md` "NK1").
//!
//! - [`graph_edits`]: `navlinks.apply` on a file's bytes - the edited file's bytes, one line per operation and the
//!   count line `navkit.py`'s apply prints after them.
//! - [`traversal`]: `navtraversal.write` - the annotations file's text and the lines it prints.
//!
//! **The same answers.** A point joins the route node nearest to it (NumPy's `hypot` and `argmin`: the first
//! minimum, or the first NaN), within 15 m horizontally and 4 m in height; costs are `0x80000000 | floor(dist * 16)`
//! with CPython's `math.dist` ([`crate::pymath::dist3`]); Python's `round` and its float formatting
//! ([`crate::pymath::round_to`]); the annotations file is `json.dump(doc, indent=1)` and a newline ([`crate::pyjson`]). The side
//! file's operations arrive as JSON values (`json.loads` of the side file, or of `json.dumps` of its operations).
//!
//! **Errors.** Where the reference raised, its exception: the path-data reader's `ValueError` / `struct.error`
//! ([`hpd::parse_exact`]), `IndexError` for an edge whose node index is past the sector's nodes. A file the port does
//! not take - a NaN or an infinity in a node or a sector's bounds, a position no float holds exactly, a cost that does
//! not fit the file's 32 bits - is [`EditError::Refused`] (the reference's answer there was never recorded).

use std::collections::{HashMap, HashSet};

use crate::pyjson::{self as json, Value};
use crate::pymath::round_to;
use bc2_formats::hpd::{self, Edge, HpdFile};

use crate::pymath::{dist3, round3};

/// How far a point may be from the route node it joins: horizontally, in height (`navlinks.JOIN_XZ`, `JOIN_Y`).
pub const JOIN_XZ: f64 = 15.0;
/// See [`JOIN_XZ`].
pub const JOIN_Y: f64 = 4.0;
/// Route nodes stand this far above the ground (`navlinks.Y_OFFSET`).
pub const Y_OFFSET: f64 = 0.95;
/// The side file's operations that edit the route graph (`navlinks.GRAPH_OPS`).
pub const GRAPH_OPS: [&str; 2] = ["link-areas", "route-edge"];

/// Why an edit stage stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    /// The reference's exception: its type's name (`ValueError`, `error` of `struct`, `IndexError`) and `str()`.
    Raised {
        /// The exception type's `__name__` (`struct.error` is `error`).
        kind: &'static str,
        /// Its text.
        msg: String,
    },
    /// An input the port does not take (what it is).
    Refused(String),
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Raised { kind, msg } => write!(f, "{kind}: {msg}"),
            Self::Refused(what) => write!(f, "refused: {what}"),
        }
    }
}

impl std::error::Error for EditError {}

fn index_error() -> EditError {
    EditError::Raised {
        kind: "IndexError",
        msg: "list index out of range".into(),
    }
}

fn refused(what: impl Into<String>) -> EditError {
    EditError::Refused(what.into())
}

/// `hpdtool.parse(data)`, with the values the port takes (every node position and value and every sector bound
/// finite).
fn parse(data: &[u8]) -> Result<HpdFile<'_>, EditError> {
    let f = hpd::parse_exact(data).map_err(|e| EditError::Raised {
        kind: if e.is_struct() { "error" } else { "ValueError" },
        msg: e.to_string(),
    })?;
    for s in &f.sectors {
        let finite = s.bounds.iter().all(|v| v.is_finite())
            && s.nodes
                .iter()
                .all(|n| n.value.is_finite() && n.pos.iter().all(|v| v.is_finite()));
        if !finite {
            return Err(refused(format!(
                "sector 0x{:04X} holds a NaN or an infinity (a path-data file the apply does not take)",
                s.id
            )));
        }
    }
    Ok(f)
}

/// `_apply`'s read-back: whether `hpdtool.write(hpdtool.parse(data)) == data`.
///
/// # Errors
/// Where the reference's reader raised, or a file the port does not take ([`EditError`]).
pub fn read_back(data: &[u8]) -> Result<bool, EditError> {
    let f = parse(data)?;
    let again = hpd::write(&f).map_err(|e| refused(format!("the file cannot be written again: {e}")))?;
    Ok(again == data)
}

/// A number of the side file as Python's float arithmetic takes it (`float(x)`; a bool is 0 or 1). Ints past 2**53
/// are refused (NumPy and `math` would round them).
fn num(v: &Value) -> Result<f64, EditError> {
    match v {
        Value::Bool(b) => Ok(f64::from(u8::from(*b))),
        Value::Float(f) => Ok(*f),
        Value::Int(i) if i.unsigned_abs() <= 1 << 53 => Ok(*i as f64),
        _ => Err(refused(
            "a position the apply does not take (not a number, or an int past 2**53)",
        )),
    }
    .and_then(|x| {
        if x.is_finite() {
            Ok(x)
        } else {
            Err(refused("a position that is a NaN or an infinity"))
        }
    })
}

/// An operation's point `[x, y, z]` (the side file's checks made sure it has three numbers).
fn point(o: &Value, key: &str) -> Result<[f64; 3], EditError> {
    let Some(items) = o.get(key).and_then(Value::as_list) else {
        return Err(refused(format!("{key}: not a point")));
    };
    let [x, y, z] = items else {
        return Err(refused(format!("{key}: not three numbers")));
    };
    Ok([num(x)?, num(y)?, num(z)?])
}

fn pos64(p: &[f32; 3]) -> [f64; 3] {
    [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])]
}

/// The node nearest `p` among `pos` within the join limits, as NumPy finds it: `(index, horizontal distance)`.
fn nearest_of(pos: impl Iterator<Item = [f64; 3]>, p: [f64; 3]) -> Option<(usize, f64)> {
    // d = hypot(x - px, z - pz), inf where the height is too far; argmin: the first minimum, or the first NaN
    let mut best: Option<(usize, f64)> = None;
    for (k, q) in pos.enumerate() {
        let d = if ((q[1] - Y_OFFSET) - p[1]).abs() <= JOIN_Y {
            (q[0] - p[0]).hypot(q[2] - p[2])
        } else {
            f64::INFINITY
        };
        if d.is_nan() {
            best = Some((k, d));
            break;
        }
        if best.is_none_or(|(_, b)| d < b) {
            best = Some((k, d));
        }
    }
    // d[k] <= JOIN_XZ (False for a NaN)
    best.filter(|&(_, d)| d <= JOIN_XZ)
}

/// `navlinks.find_pair(f, a, b)`: (sector index, local node a, local node b) of the closest pair of nodes in one
/// sector.
fn find_pair(f: &HpdFile<'_>, a: [f64; 3], b: [f64; 3]) -> Option<(usize, usize, usize)> {
    let mut best: Option<(f64, usize, usize, usize)> = None;
    let reach = JOIN_XZ + 1.0;
    for (si, s) in f.sectors.iter().enumerate() {
        let [x0, x1, _, _, z0, z1] = s.bounds.map(f64::from);
        if !(x0 - reach <= a[0] && a[0] <= x1 + reach && z0 - reach <= a[2] && a[2] <= z1 + reach) {
            continue;
        }
        if s.nodes.is_empty() {
            continue;
        }
        let na = nearest_of(s.nodes.iter().map(|n| pos64(&n.pos)), a);
        let nb = nearest_of(s.nodes.iter().map(|n| pos64(&n.pos)), b);
        if let (Some(na), Some(nb)) = (na, nb)
            && na.0 != nb.0
        {
            let score = na.1 + nb.1;
            if best.is_none_or(|(bs, ..)| score < bs) {
                best = Some((score, si, na.0, nb.0));
            }
        }
    }
    best.map(|(_, si, ia, ib)| (si, ia, ib))
}

/// `navlinks.cost_tag(p, q)`: `0x80000000 | floor(math.dist(p, q) * 16)`.
fn cost_tag(p: [f64; 3], q: [f64; 3]) -> Result<u32, EditError> {
    let c = (dist3((p[0], p[1], p[2]), (q[0], q[1], q[2])) * 16.0).floor();
    if !(0.0..=f64::from(u32::MAX)).contains(&c) {
        return Err(refused("a route edge whose cost does not fit the file's 32 bits"));
    }
    Ok(0x8000_0000 | c as u32)
}

/// `navlinks.recompute_first_edges(sector)`: edges sorted by source (stable), each node's `a` its first outgoing
/// edge + 1 (0 without one).
fn recompute_first_edges(s: &mut hpd::Sector<'_>) {
    s.edges.sort_by_key(|e| e.src);
    let mut first: HashMap<u32, u32> = HashMap::new();
    for (k, e) in s.edges.iter().enumerate() {
        first.entry(e.src).or_insert(k as u32);
    }
    for (n, node) in s.nodes.iter_mut().enumerate() {
        node.a = first.get(&(n as u32)).map_or(0, |k| k + 1);
    }
}

/// `{x:.1f}` / `{x:.0f}` of a Python float.
fn fixed(x: f64, places: usize) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf" } else { "-inf" }.into();
    }
    format!("{x:.places$}")
}

fn op_name(o: &Value) -> &str {
    o.get("op").and_then(Value::as_str).unwrap_or("")
}

/// `o.get('two-way', True)`.
fn two_way(o: &Value) -> Value {
    o.get("two-way").cloned().unwrap_or(Value::Bool(true))
}

/// What [`graph_edits`] made.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphEdits {
    /// The edited file (`hpdtool.write(f)`).
    pub data: Vec<u8>,
    /// Per graph operation: whether it was applied, and its message.
    pub results: Vec<(bool, String)>,
    /// The lines `navlinks.apply` prints (one per result: `'  '` or `'  NOT APPLIED: '` and the message).
    pub lines: Vec<String>,
    /// The line `navkit.py`'s apply prints once the file is written: `'  <n> of <m> link / route-edge edit(s)
    /// applied'`.
    pub summary: String,
}

/// `navlinks.apply(f, graph, log)` on `data` (the file `_apply` reads) for the graph operations among `ops`, then
/// `hpdtool.write(f)`.
///
/// # Errors
/// Where the reference raised, or an input the port does not take ([`EditError`]).
#[expect(clippy::too_many_lines, reason = "navlinks.apply's one function, step for step")]
pub fn graph_edits(data: &[u8], ops: &[Value]) -> Result<GraphEdits, EditError> {
    let mut f = parse(data)?;
    let graph: Vec<&Value> = ops.iter().filter(|o| GRAPH_OPS.contains(&op_name(o))).collect();
    let mut results: Vec<(bool, String)> = Vec::new();
    let mut touched: HashSet<usize> = HashSet::new();
    for o in &graph {
        let name = op_name(o);
        let (a, b) = (point(o, "a")?, point(o, "b")?);
        let Some((si, ia, ib)) = find_pair(&f, a, b) else {
            results.push((
                false,
                format!(
                    "{name}: no sector has route nodes near both points (within {} m) - split it at the sector \
                     border, or put the points nearer the route graph",
                    fixed(JOIN_XZ, 0)
                ),
            ));
            continue;
        };
        let two = two_way(o).truthy();
        let s = f.sectors.get(si).ok_or_else(index_error)?;
        let pos = |k: usize| s.nodes.get(k).map(|n| pos64(&n.pos)).ok_or_else(index_error);
        let (pa, pb) = (pos(ia)?, pos(ib)?);
        let dirs: Vec<(usize, usize)> = if two { vec![(ia, ib), (ib, ia)] } else { vec![(ia, ib)] };
        let length = dist3((pa[0], pa[1], pa[2]), (pb[0], pb[1], pb[2]));
        let limit = s
            .nodes
            .iter()
            .map(|n| f64::from(n.value))
            .find(|v| *v != 0.0)
            .unwrap_or(15.0);
        let warn = if length > limit {
            format!(
                " - longer than the file's DistEdgeMax ({} m): the game may not use it",
                fixed(limit, 0)
            )
        } else {
            String::new()
        };
        let add = name == "link-areas" || o.get("action").and_then(Value::as_str) == Some("add");
        if add {
            let have: HashSet<(u32, u32)> = s.edges.iter().map(|e| (e.src, e.dst)).collect();
            let mut new = Vec::new();
            for &(src, dst) in &dirs {
                if !have.contains(&(src as u32, dst as u32)) {
                    new.push(Edge {
                        tag: cost_tag(pos(src)?, pos(dst)?)?,
                        src: src as u32,
                        dst: dst as u32,
                    });
                }
            }
            let added = new.len();
            let sid = s.id;
            if let Some(s) = f.sectors.get_mut(si) {
                s.edges.extend(new);
            }
            touched.insert(si);
            results.push((
                true,
                format!(
                    "{name}: {added} edge(s) added in sector 0x{sid:04X}, {} m{}{warn}",
                    fixed(length, 1),
                    if added == 0 { " (already there)" } else { "" }
                ),
            ));
        } else {
            // an edge between two border nodes is in every sector that shares both (same node ids): removed in all
            let id = |k: usize| s.nodes.get(k).map(|n| n.id).ok_or_else(index_error);
            let (ida, idb) = (id(ia)?, id(ib)?);
            let sid = s.id;
            let want: Vec<(u32, u32)> = if two {
                vec![(ida, idb), (idb, ida)]
            } else {
                vec![(ida, idb)]
            };
            let shared = ida & 0x8000_0000 != 0 && idb & 0x8000_0000 != 0;
            let mut removed = 0usize;
            let mut at: Vec<String> = Vec::new();
            for (sj, t) in f.sectors.iter_mut().enumerate() {
                let ids: Vec<u32> = t.nodes.iter().map(|n| n.id).collect();
                if sj != si && !(shared && ids.contains(&ida) && ids.contains(&idb)) {
                    continue;
                }
                let before = t.edges.len();
                let mut kept = Vec::with_capacity(before);
                for e in &t.edges {
                    let src = *ids.get(e.src as usize).ok_or_else(index_error)?;
                    let dst = *ids.get(e.dst as usize).ok_or_else(index_error)?;
                    if !want.contains(&(src, dst)) {
                        kept.push(*e);
                    }
                }
                t.edges = kept;
                if t.edges.len() != before {
                    removed += before - t.edges.len();
                    at.push(format!("0x{:04X}", t.id));
                    touched.insert(sj);
                }
            }
            results.push(if removed > 0 {
                (
                    true,
                    format!(
                        "route-edge remove: {removed} edge(s) removed in sector(s) {}",
                        at.join(", ")
                    ),
                )
            } else {
                (
                    false,
                    format!("route-edge remove: no edge between those nodes in sector 0x{sid:04X}"),
                )
            });
        }
    }
    #[allow(clippy::iter_over_hash_type, reason = "each touched sector recomputed on its own")]
    for &si in &touched {
        if let Some(s) = f.sectors.get_mut(si) {
            recompute_first_edges(s);
        }
    }
    let lines = results
        .iter()
        .map(|(ok, msg)| format!("{}{msg}", if *ok { "  " } else { "  NOT APPLIED: " }))
        .collect();
    let data = hpd::write(&f).map_err(|e| refused(format!("the edited file cannot be written: {e}")))?;
    let summary = format!(
        "  {} of {} link / route-edge edit(s) applied",
        results.iter().filter(|r| r.0).count(),
        graph.len()
    );
    Ok(GraphEdits {
        data,
        results,
        lines,
        summary,
    })
}

/// `navtraversal.graph(f)`: the unique nodes (by id, first met) with their sector, position and island (0: the
/// largest; equal sizes in the order first met, as `Counter.most_common`).
struct Graph {
    ids: Vec<u32>,
    secs: Vec<u32>,
    pos: Vec<[f64; 3]>,
    island: Vec<usize>,
}

/// The union-find's root of `a`, halving the path (`navtraversal.graph`'s `find`).
#[expect(clippy::indexing_slicing, reason = "every index is a node's, below the length")]
fn find(parent: &mut [usize], mut a: usize) -> usize {
    while parent[a] != a {
        parent[a] = parent[parent[a]];
        a = parent[a];
    }
    a
}

fn graph(f: &HpdFile<'_>) -> Result<Graph, EditError> {
    let mut index: HashMap<u32, usize> = HashMap::new();
    let (mut ids, mut secs, mut pos) = (Vec::new(), Vec::new(), Vec::new());
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for s in &f.sectors {
        let mut local = Vec::with_capacity(s.nodes.len());
        for n in &s.nodes {
            let k = *index.entry(n.id).or_insert_with(|| {
                ids.push(n.id);
                secs.push(s.id);
                pos.push(pos64(&n.pos));
                ids.len() - 1
            });
            local.push(k);
        }
        for e in &s.edges {
            let a = *local.get(e.src as usize).ok_or_else(index_error)?;
            let b = *local.get(e.dst as usize).ok_or_else(index_error)?;
            edges.push((a, b));
        }
    }
    let mut parent: Vec<usize> = (0..ids.len()).collect();
    for (a, b) in edges {
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        if ra != rb
            && let Some(p) = parent.get_mut(ra)
        {
            *p = rb;
        }
    }
    let roots: Vec<usize> = (0..ids.len()).map(|k| find(&mut parent, k)).collect();
    // Counter(roots).most_common(): by count, equal counts in the order first met
    let mut order: Vec<usize> = Vec::new();
    let mut count: HashMap<usize, usize> = HashMap::new();
    for &r in &roots {
        let c = count.entry(r).or_insert(0);
        if *c == 0 {
            order.push(r);
        }
        *c += 1;
    }
    order.sort_by_key(|r| std::cmp::Reverse(count.get(r).copied().unwrap_or(0)));
    let rank: HashMap<usize, usize> = order.iter().enumerate().map(|(k, &r)| (r, k)).collect();
    let island = roots.iter().map(|r| rank.get(r).copied().unwrap_or(0)).collect();
    Ok(Graph { ids, secs, pos, island })
}

/// `navtraversal.nearest(g, p)`: the route node an annotation's end joins, as the file's dict.
fn nearest(g: &Graph, p: [f64; 3]) -> Result<Value, EditError> {
    let Some((k, d)) = nearest_of(g.pos.iter().copied(), p) else {
        return Ok(Value::Null);
    };
    let (Some(at), Some(sec), Some(id), Some(island)) = (g.pos.get(k), g.secs.get(k), g.ids.get(k), g.island.get(k))
    else {
        return Err(index_error());
    };
    let mut pos = Vec::with_capacity(3);
    for &v in at {
        pos.push(Value::Float(
            round3(v).ok_or_else(|| refused("a node position round() does not take"))?,
        ));
    }
    Ok(Value::dict([
        ("sector", Value::Int(i128::from(*sec))),
        ("id", Value::Int(i128::from(*id))),
        ("pos", Value::List(pos)),
        ("island", Value::Int(*island as i128)),
        ("distance", Value::Float(round_to(d, 2))),
    ]))
}

/// `round(t[1] - b[1], 2)`: an int when both are ints (bools count), else a float.
fn rise(b: &Value, t: &Value) -> Result<Value, EditError> {
    let (Some(b1), Some(t1)) = (b.as_list().and_then(|l| l.get(1)), t.as_list().and_then(|l| l.get(1))) else {
        return Err(refused("a traversal end that is not a point"));
    };
    if let (Some(x), Some(y)) = (b1.as_int(), t1.as_int())
        && !matches!(b1, Value::Float(_))
        && !matches!(t1, Value::Float(_))
    {
        return Ok(Value::Int(y - x));
    }
    Ok(Value::Float(round_to(num(t1)? - num(b1)?, 2)))
}

/// What [`traversal`] made.
#[derive(Debug, Clone, PartialEq)]
pub struct Traversal {
    /// The annotations file's text (`json.dump(doc, indent=1)` and a newline).
    pub text: String,
    /// The lines `navtraversal.write` prints, one per annotation.
    pub lines: Vec<String>,
    /// The annotations written.
    pub links: usize,
}

/// `navtraversal.write(f, pathdata_path, operations, level, out_path)` for the path data `data` (the file as
/// written: its SHA-1 goes into the annotations), named `pathdata_name` (`os.path.basename(pathdata_path)`): the
/// text to write to `out_path` and the lines printed.
///
/// # Errors
/// Where the reference raised, or an input the port does not take ([`EditError`]).
#[expect(
    clippy::many_single_char_names,
    clippy::similar_names,
    reason = "the reference's names (b, t, nb, nt) kept for side-by-side reading"
)]
pub fn traversal(data: &[u8], pathdata_name: &str, ops: &[Value], level: &Value) -> Result<Traversal, EditError> {
    let f = parse(data)?;
    let g = graph(&f)?;
    let mut links = Vec::new();
    let mut lines = Vec::new();
    for o in ops.iter().filter(|o| op_name(o) == "traversal") {
        let (Some(b), Some(t)) = (o.get("bottom"), o.get("top")) else {
            return Err(refused("a traversal without its ends"));
        };
        let (pb, pt) = (point(o, "bottom")?, point(o, "top")?);
        let nb = nearest(&g, pb)?;
        let nt = nearest(&g, pt)?;
        let length = round_to(dist3((pb[0], pb[1], pb[2]), (pt[0], pt[1], pt[2])), 2);
        let isl = |n: &Value| n.get("island").and_then(Value::as_int);
        let joins = nb != Value::Null && nt != Value::Null && isl(&nb) != isl(&nt);
        let kind = o.get("kind").cloned().unwrap_or(Value::Null);
        let tw = two_way(o);
        let ends: Vec<String> = [("bottom", &nb), ("top", &nt)]
            .iter()
            .map(|(name, n)| match isl(n) {
                Some(i) if **n != Value::Null => format!("{name} island {i}"),
                _ => format!("{name} no route node within 15 m"),
            })
            .collect();
        let Some(kind_s) = kind.as_str() else {
            return Err(refused("a traversal kind that is not a str"));
        };
        lines.push(format!(
            "  traversal {kind_s}{}: {} m, {}{}",
            if tw.truthy() { " (two-way)" } else { "" },
            fixed(length, 1),
            ends.join(" and "),
            if joins { " - joins two islands" } else { "" }
        ));
        links.push(Value::dict([
            ("kind", kind),
            ("two-way", tw),
            ("bottom", b.clone()),
            ("top", t.clone()),
            ("rise", rise(b, t)?),
            ("length", Value::Float(length)),
            ("bottom_node", nb),
            ("top_node", nt),
            ("joins_islands", Value::Bool(joins)),
            ("by", o.get("by").cloned().unwrap_or(Value::Null)),
            ("when", o.get("when").cloned().unwrap_or(Value::Null)),
        ]));
    }
    let n = links.len();
    let doc = Value::dict([
        ("format", Value::Str("bfbc2-traversal".into())),
        ("version", Value::Int(1)),
        ("level", level.clone()),
        ("pathdata", Value::Str(pathdata_name.into())),
        (
            "pathdata_sha1",
            Value::Str(sha1_smol::Sha1::from(data).digest().to_string()),
        ),
        ("links", Value::List(links)),
    ]);
    let mut text = json::dumps(&doc, Some(1));
    text.push('\n');
    Ok(Traversal { text, lines, links: n })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use bc2_formats::hpd::{Node, Sector};

    fn node(id: u32, x: f32, z: f32) -> Node {
        Node {
            id,
            pos: [x, 0.95, z],
            flags: 0x8000_0000,
            value: if id == 1 { 15.0 } else { 0.0 },
            a: 0,
            b: 0,
            c: 0,
        }
    }

    fn file() -> Vec<u8> {
        let mesh: &[u8] = b"KynogonRuntimeMesh\0\0";
        let s = |id: u32, x0: f32, nodes: Vec<Node>, edges: Vec<Edge>| Sector {
            id,
            bounds: [x0, x0 + 100.0, 0.0, 2.0, 0.0, 100.0],
            reserved: 0,
            date: 0,
            time: 0,
            nodes,
            edges,
            mesh,
        };
        let e = |src, dst| Edge {
            tag: 0x8000_0010,
            src,
            dst,
        };
        let f = HpdFile {
            header: [0; hpd::HEADER_WORDS],
            sectors: vec![
                s(
                    0x0101,
                    0.0,
                    vec![node(1, 10.0, 10.0), node(2, 20.0, 10.0), node(0x8000_0003, 100.0, 50.0)],
                    vec![e(0, 1), e(1, 0)],
                ),
                s(
                    0x0201,
                    100.0,
                    vec![node(0x8000_0003, 100.0, 50.0), node(4, 150.0, 50.0)],
                    vec![],
                ),
            ],
        };
        hpd::write(&f).unwrap()
    }

    fn op(text: &str) -> Value {
        json::loads(text).unwrap()
    }

    #[test]
    fn links_and_removals() {
        let data = file();
        let ops = vec![
            op(r#"{"op": "link-areas", "a": [100, 0, 50], "b": [150.5, 0.0, 50]}"#),
            op(r#"{"op": "route-edge", "a": [10, 0, 10], "b": [20, 0, 10], "action": "remove"}"#),
            op(r#"{"op": "route-edge", "a": [10, 0, 10], "b": [20, 0, 10], "action": "remove", "two-way": false}"#),
            op(r#"{"op": "link-areas", "a": [500, 0, 500], "b": [10, 0, 10]}"#),
            op(r#"{"op": "note", "at": [0, 0, 0], "text": "x"}"#),
        ];
        let got = graph_edits(&data, &ops).unwrap();
        assert_eq!(got.lines.len(), 4);
        assert_eq!(
            got.lines[0],
            "  link-areas: 2 edge(s) added in sector 0x0201, 50.0 m - longer than the file's DistEdgeMax (15 m): the game may not use it"
        );
        assert_eq!(
            got.lines[1],
            "  route-edge remove: 2 edge(s) removed in sector(s) 0x0101"
        );
        assert!(got.lines[2].starts_with("  NOT APPLIED: route-edge remove: no edge"));
        assert!(got.lines[3].starts_with("  NOT APPLIED: link-areas: no sector"));
        assert_eq!(got.summary, "  2 of 4 link / route-edge edit(s) applied");
        let f = hpd::parse(&got.data).unwrap();
        assert_eq!(f.sectors[0].edges, vec![]);
        assert_eq!(f.sectors[1].edges.len(), 2);
        assert_eq!(f.sectors[1].edges[0].tag, 0x8000_0000 | 0x320);
        assert_eq!(f.sectors[1].nodes[0].a, 1);
        assert_eq!(f.sectors[1].nodes[1].a, 2);
        // the same edit again: already there
        let again = graph_edits(&got.data, &ops[..1]).unwrap();
        assert!(again.lines[0].contains("50.0 m (already there)"));
        assert_eq!(again.data, got.data);
    }

    #[test]
    fn traversal_file() {
        let data = file();
        let ops = vec![op(
            r#"{"op": "traversal", "kind": "ladder", "bottom": [10, 0, 10], "top": [150, 3, 50], "by": "PC é"}"#,
        )];
        let got = traversal(&data, "x.hpd", &ops, &Value::Str("Levels/SP".into())).unwrap();
        assert_eq!(got.links, 1);
        assert_eq!(
            got.lines[0],
            "  traversal ladder (two-way): 145.6 m, bottom island 0 and top island 2 - joins two islands"
        );
        assert!(got.text.contains("\"rise\": 3,\n"));
        assert!(got.text.contains("\"by\": \"PC \\u00e9\""));
        assert!(got.text.ends_with("}\n"));
    }

    #[test]
    fn bad_edges_raise_index_error() {
        let data0 = file();
        let mut f = hpd::parse(&data0).unwrap();
        f.sectors[0].edges.push(Edge { tag: 0, src: 9, dst: 0 });
        let data = hpd::write(&f).unwrap();
        let ops = vec![op(
            r#"{"op": "route-edge", "a": [10, 0, 10], "b": [20, 0, 10], "action": "remove"}"#,
        )];
        assert_eq!(graph_edits(&data, &ops).unwrap_err(), index_error());
        assert_eq!(traversal(&data, "x", &[], &Value::Null).unwrap_err(), index_error());
    }
}
