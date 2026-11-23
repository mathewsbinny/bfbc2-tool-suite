//! `bfbc2-culltree/culltree.py`'s model in Rust (round O3, `rust/BENCHMARKS.md` "O3"): its byte-keeping DBX codec
//! (`Doc`), the static cull tree (`CullTree`: preorder nodes, sphere keys, leaves), a level's placed objects
//! (`Level`), the blueprints' shapes from their slotted instances (`collect`, `shape_for`), the reference levels
//! (`ref_rows` / `ref_pairs`, `level_partitions`), `add` (`insert`, `make_subtree`, `auto_objects`, `misplaced`) and
//! the checks (`check_structure`, `containment`, `object_checks`) - every value computed with the reference's
//! floating-point operations in its order, so the written trees and levels and the printed numbers are its own.
//! `bfbc2.exe culltree` (bc2-cli's `cull` group) is the command line on it; the map editor's export keeps its own
//! [`crate::culltree`] (an object's shape from its own leaves).
//!
//! Errors are typed ([`CullError`], round PR1b-2-rest): the reference's `CullTreeError` with its text, a file's I/O
//! error with its path, and [`CullError::Damaged`] where the reference raises an exception of its own on a damaged
//! file (an index or `struct` error). `bfbc2.exe culltree` turns them into the Python's exceptions (the `From` impl
//! below: the `OSError` of `open()`, and the hand-off to the reference for a damaged file). Before, they were the
//! Python emulation's `Fail` itself.
//!
//! Round CT1: the reference levels are streamed ([`scan_level`]: one instance at a time, not the whole document)
//! and only their rows are kept; the rows are kept between runs in `culltree.py`'s own row cache ([`RowStore`],
//! the pickles of [`crate::cullcache`] under `%LOCALAPPDATA%\bfbc2-tool-suite\cache\culltree`), read and written
//! by both. The rows from the cache equal those read, so the answers do not depend on it.

#![allow(
    clippy::indexing_slicing,
    reason = "indexes bounded by the checks before them, as the reference's (the arena's nodes, fixed-size keys)"
)]
#![allow(
    clippy::cast_precision_loss,
    clippy::float_cmp,
    reason = "the reference's own float arithmetic and comparisons, kept as it is"
)]
#![allow(
    clippy::items_after_statements,
    clippy::manual_midpoint,
    reason = "the reference's helpers next to their use; its (a + b) / 2 median"
)]
#![allow(missing_docs, reason = "named as the Python they port, documented there")]
#![allow(
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    reason = "every error is the Python exception the reference raises at that point"
)]

use std::collections::HashMap;
use std::io;
use std::sync::Arc;

use bc2_formats::culltree as raw;
use bc2_formats::dbx::edit::{EditDbx, NodeId, Parts};
use bc2_pyrt::err::{Fail, PyErr};
use bc2_pyrt::fmt::float_repr;
use bc2_pyrt::py::{OMap, abspath, basename, dirname, isdir, isfile, join, splitext};
use rayon::prelude::*;

pub const MAGIC: &[u8] = b"{binary}";
pub const CONTAINER: u8 = 0xA;
pub const LEAF: i64 = 0x8000_0000;
pub const SPARE: usize = 256;
pub const CENTER_SCALE: f64 = 16.0;
pub const RADIUS_SCALE: f64 = 4.0;
pub const STACK_SLOTS: usize = 128;
pub const LEVEL_MARK: &[u8] = b"ExclusiveStaticCullTree";

/// What went wrong in the model (see the module): typed, so the library's callers need no Python emulation.
#[derive(Debug, thiserror::Error)]
pub enum CullError {
    /// The reference's `CullTreeError`: its text.
    #[error("{0}")]
    CullTree(String),
    /// A file could not be read, written or looked at (`open()`'s or `os.stat`'s `OSError`).
    #[error("{path}: {source}")]
    Io {
        /// The file.
        path: String,
        /// The error.
        source: io::Error,
    },
    /// A folder could not be made (`os.makedirs`'s error).
    #[error("{dir}: {source}")]
    Mkdir {
        /// The folder.
        dir: String,
        /// The error.
        source: io::Error,
    },
    /// A damaged file the reference fails on with an exception of its own (an index or `struct` error).
    #[error("a damaged file (the reference fails on it with an exception of its own)")]
    Damaged,
    /// The caller's `log` failed (the caller holds why).
    #[error("the log's sink failed")]
    Sink,
}

impl CullError {
    /// A file's I/O error.
    pub fn io(path: &str, source: io::Error) -> Self {
        Self::Io {
            path: path.to_owned(),
            source,
        }
    }

    /// A `CullTreeError`.
    pub fn is_cull_tree(&self) -> bool {
        matches!(self, Self::CullTree(_))
    }
}

/// The Python's exception for it (`bfbc2.exe culltree`): `CullTreeError`, the `OSError` (`FileExistsError` for a
/// folder in the way, as `os.makedirs` says it), or the hand-off to the reference.
impl From<CullError> for Fail {
    fn from(e: CullError) -> Self {
        match e {
            CullError::CullTree(m) => Fail::Err(PyErr::new("CullTreeError", m)),
            CullError::Io { path, source } => bc2_pyrt::err::os_error(&path, &source),
            CullError::Mkdir { dir, source } if source.kind() == io::ErrorKind::AlreadyExists => Fail::Err(PyErr {
                kind: "FileExistsError".to_owned(),
                msg: format!(
                    "[WinError 183] Cannot create a file when that file already exists: {}",
                    bc2_pyrt::fmt::str_repr(&dir)
                ),
                filename: Some(dir),
                what_to_do: None,
            }),
            CullError::Mkdir { dir, source } => bc2_pyrt::err::os_error(&dir, &source),
            CullError::Damaged | CullError::Sink => Fail::Python,
        }
    }
}

/// The model's results.
pub type CullResult<T> = Result<T, CullError>;

/// `CullTreeError(msg)`.
pub fn cte(msg: impl Into<String>) -> CullError {
    CullError::CullTree(msg.into())
}

const PY: CullError = CullError::Damaged;

/// `open(path, 'rb').read()`.
fn read(path: &str) -> CullResult<Vec<u8>> {
    std::fs::read(path).map_err(|e| CullError::io(path, e))
}

/// Python's `max(a, b, ...)`: the first of the largest (an item replaces it only when greater).
pub fn py_max(v: &[f64]) -> f64 {
    let mut m = v[0];
    for &x in &v[1..] {
        if x > m {
            m = x;
        }
    }
    m
}

// ------------------------------------------------------------------------------------------------ binary DBX

// Round PR1b-2-rest2 (merge step 3): `culltree.py`'s `Doc` is the suite's one document, `EditDbx`, read under
// `Doc`'s rules by `bc2_formats::culltree::document` / `stream` (string indexes checked where they are read, as here;
// every value's bytes as stored; written back byte for byte). The reference's `El` methods are the functions below.

/// A varint ([`bc2_formats::varint::read_u64`]); one past 64 bits is a big integer only Python reads.
fn varint(data: &[u8], pos: usize) -> CullResult<(u64, usize)> {
    bc2_formats::varint::read_u64(data, pos).map_err(|_| PY)
}

fn put_varint(v: u64, out: &mut Vec<u8>) {
    bc2_formats::varint::put(out, v);
}

/// String `id` of the document (an index past the table: the reference's `IndexError`).
fn st(d: &EditDbx, id: u32) -> CullResult<&str> {
    // the table was decoded as the reference decodes it (UTF-8, a bad sequence replaced) before it was kept
    std::str::from_utf8(d.string(id).ok_or(PY)?).map_err(|_| PY)
}

/// `El.attr(name)`.
pub fn attr<'a>(d: &'a EditDbx, n: NodeId, name: &str) -> CullResult<Option<&'a str>> {
    for &(k, v) in d.attr_ids(n) {
        if st(d, k)? == name {
            return Ok(Some(st(d, v)?));
        }
    }
    Ok(None)
}

/// `El.name()`.
pub fn name_of(d: &EditDbx, n: NodeId) -> CullResult<Option<&str>> {
    attr(d, n, "name")
}

/// `El.child(name)`: the first child of that name, its index among the children.
pub fn child_index(d: &EditDbx, n: NodeId, name: &str) -> CullResult<Option<usize>> {
    for (i, &c) in d.children(n).unwrap_or_default().iter().enumerate() {
        if name_of(d, c)? == Some(name) {
            return Ok(Some(i));
        }
    }
    Ok(None)
}

/// Child `i` of `n`.
fn kid(d: &EditDbx, n: NodeId, i: usize) -> CullResult<NodeId> {
    d.children(n).and_then(|c| c.get(i)).copied().ok_or(PY)
}

/// `El.numbers()`: (size, values), each value sliced as Python slices (short at the end).
pub fn numbers(d: &EditDbx, n: NodeId) -> CullResult<(u64, Vec<Vec<u8>>)> {
    let value = d.payload(n);
    let (count, p) = varint(value, 0)?;
    let (size, p) = varint(value, p)?;
    if count > 1 << 24 || size > 1 << 32 {
        return Err(PY); // a damaged count only the reference walks (or runs out of memory on)
    }
    let len = value.len();
    let mut out = Vec::new();
    for i in 0..count {
        let a = (p as u64 + i * size).min(len as u64) as usize;
        let b = (p as u64 + (i + 1) * size).min(len as u64) as usize;
        out.push(value[a..b.max(a)].to_vec());
    }
    Ok((size, out))
}

/// `El.set_numbers(size, values)`.
pub fn set_numbers(d: &mut EditDbx, n: NodeId, size: u64, values: &[Vec<u8>]) -> CullResult<()> {
    let mut out = Vec::new();
    put_varint(values.len() as u64, &mut out);
    put_varint(size, &mut out);
    for v in values {
        if v.len() as u64 != size {
            return Err(cte("internal: number of the wrong size"));
        }
        out.extend_from_slice(v);
    }
    d.set_raw(n, out).map_err(|_| PY)
}

/// `El.clone()`: a deep copy with every string index and value as stored (not attached anywhere yet).
fn clone_stored(d: &mut EditDbx, n: NodeId) -> NodeId {
    let children = d
        .children(n)
        .map(<[NodeId]>::to_vec)
        .map(|kids| kids.into_iter().map(|c| clone_stored(d, c)).collect());
    d.push(Parts {
        tag: d.tag_id(n),
        kind: d.kind(n),
        attrs: d.attr_ids(n).to_vec(),
        cbyte: d.container_byte(n),
        payload: if children.is_some() {
            Vec::new()
        } else {
            d.payload(n).to_vec()
        },
        children,
    })
}

fn be32(d: &[u8], at: usize) -> CullResult<u32> {
    let b = d.get(at..at + 4).ok_or(PY)?;
    Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// The header and string table of a binary DBX file: (header word, strings, where the elements start).
fn header(data: &[u8], name: &str) -> CullResult<(u32, Vec<Vec<u8>>, usize)> {
    if data.len() <= 24 || !data.starts_with(MAGIC) {
        return Err(cte(format!("{name} is not a binary .dbx file")));
    }
    let size = be32(data, 16)? as usize;
    let count = be32(data, 20)? as usize;
    let base = 24 + 4 * count;
    let mut strings = Vec::new();
    for i in 0..count {
        let off = be32(data, 24 + 4 * i)? as usize;
        let start = base + off;
        if start > data.len() {
            return Err(PY);
        }
        let end = data[start..].iter().position(|&b| b == 0).ok_or(PY)? + start;
        strings.push(String::from_utf8_lossy(&data[start..end]).into_owned().into_bytes());
    }
    Ok((be32(data, 12)?, strings, 24 + size))
}

/// The element reader's error as the reference raises it: `CullTreeError` with its text; a string index past 32
/// bits is the reference's own exception where it reads it.
fn element_error(name: &str, e: &raw::ElementError) -> CullError {
    match e {
        raw::ElementError::Wide(_) => PY,
        e => cte(format!("{name}: {e}")),
    }
}

/// `Doc(data, name)`: the file as the one document.
pub fn parse_doc(data: &[u8], name: &str) -> CullResult<EditDbx> {
    let (head, strings, pos) = header(data, name)?;
    raw::document(data, head, strings, pos).map_err(|e| element_error(name, &e))
}

/// `Doc.read(path)`.
pub fn read_doc(path: &str) -> CullResult<EditDbx> {
    parse_doc(&read(path)?, &basename(path))
}

/// `Doc.write(path)`: its folder made, the file written.
pub fn write_doc(d: &EditDbx, path: &str) -> CullResult<()> {
    let dir = dirname(&abspath(path));
    std::fs::create_dir_all(&dir).map_err(|e| mkdir_error(&dir, e))?;
    let bytes = d.write().map_err(|_| PY)?;
    std::fs::write(path, bytes).map_err(|e| CullError::io(path, e))
}

/// `Doc.instances`: the root's children tagged `instance` (their indices among the root's children).
pub fn instances(d: &EditDbx) -> CullResult<Vec<usize>> {
    let root = *d.roots().first().ok_or(PY)?;
    let mut out = Vec::new();
    for (i, &e) in d.children(root).unwrap_or_default().iter().enumerate() {
        if st(d, d.tag_id(e))? == "instance" {
            out.push(i);
        }
    }
    Ok(out)
}

/// The root's child `i` (an index [`instances`] gave).
fn instance(d: &EditDbx, i: usize) -> CullResult<NodeId> {
    kid(d, *d.roots().first().ok_or(PY)?, i)
}

/// `os.makedirs`'s error (reported as the reference reports it: the `From` impl).
pub fn mkdir_error(dir: &str, e: io::Error) -> CullError {
    CullError::Mkdir {
        dir: dir.to_owned(),
        source: e,
    }
}

fn int4(d: &EditDbx, e: NodeId) -> CullResult<i64> {
    let (size, v) = numbers(d, e)?;
    if size != 4 || v.len() != 1 {
        return Err(cte("internal: not a single 4-byte number"));
    }
    let b: [u8; 4] = v[0].as_slice().try_into().map_err(|_| PY)?;
    Ok(i64::from(i32::from_be_bytes(b)))
}

fn floats(d: &EditDbx, e: NodeId) -> CullResult<Vec<f64>> {
    let (size, v) = numbers(d, e)?;
    if size != 4 {
        return Ok(Vec::new());
    }
    v.iter()
        .map(|x| {
            let b: [u8; 4] = x.as_slice().try_into().map_err(|_| PY)?;
            Ok(f64::from(f32::from_be_bytes(b)))
        })
        .collect()
}

// ------------------------------------------------------------------------------------------------ spheres

pub type Vec3 = [f64; 3];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sphere {
    pub c: Vec3,
    pub r: f64,
}

/// `_dist`: `math.sqrt(sum((x - y) ** 2 ...))`.
pub fn dist(a: Vec3, b: Vec3) -> f64 {
    let mut s = 0.0;
    for i in 0..3 {
        let d = a[i] - b[i];
        s += d * d;
    }
    s.sqrt()
}

impl Sphere {
    pub fn contains(&self, o: &Self) -> bool {
        dist(self.c, o.c) + o.r <= self.r + 1e-6
    }
}

fn s16(v: i64) -> f64 {
    if v >= 32768 { (v - 65536) as f64 } else { v as f64 }
}

/// A node as stored: (Right, Max, kx, ky, kz, kw).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeData {
    pub right: i64,
    pub max: f64,
    pub keys: [i64; 4],
}

impl NodeData {
    const ZERO: Self = Self {
        right: 0,
        max: 0.0,
        keys: [0; 4],
    };
}

pub fn decode(d: &NodeData) -> Sphere {
    let [x, y, z, w] = d.keys;
    Sphere {
        c: [s16(x) / CENTER_SCALE, s16(y) / CENTER_SCALE, s16(z) / CENTER_SCALE],
        r: s16(w) / RADIUS_SCALE,
    }
}

fn tuple_repr(c: Vec3) -> String {
    format!("({}, {}, {})", float_repr(c[0]), float_repr(c[1]), float_repr(c[2]))
}

/// `int(round(x))` (half to even; NaN and infinities raise in the reference).
fn py_round(x: f64) -> CullResult<i64> {
    if !x.is_finite() || x.abs() > 9e15 {
        return Err(PY);
    }
    Ok(x.round_ties_even() as i64)
}

/// `encode(sphere, contain)`: keys whose decoded sphere holds the sphere and every one in `contain`.
pub fn encode(s: &Sphere, contain: &[Sphere]) -> CullResult<[i64; 4]> {
    let mut keys = [0i64; 3];
    for i in 0..3 {
        keys[i] = py_round(s.c[i] * CENTER_SCALE)?;
    }
    if keys.iter().any(|k| !(-32768..=32767).contains(k)) {
        return Err(cte(format!(
            "sphere centre {} is outside the range a cull tree can hold (+-2048 m)",
            tuple_repr(s.c)
        )));
    }
    let qc = [
        keys[0] as f64 / CENTER_SCALE,
        keys[1] as f64 / CENTER_SCALE,
        keys[2] as f64 / CENTER_SCALE,
    ];
    let mut needs = vec![dist(qc, s.c) + s.r];
    needs.extend(contain.iter().map(|o| dist(qc, o.c) + o.r));
    let need = py_max(&needs);
    let c = (need * RADIUS_SCALE - 1e-9).ceil();
    if !c.is_finite() {
        return Err(PY);
    }
    let w = c as i64;
    if w > 32767 {
        return Err(cte(format!(
            "sphere radius {} m is more than a cull tree can hold (8191 m)",
            fixed(need, 1)
        )));
    }
    Ok([keys[0] & 0xFFFF, keys[1] & 0xFFFF, keys[2] & 0xFFFF, w])
}

/// `f'{x:.{p}f}'`.
pub fn fixed(x: f64, p: usize) -> String {
    if x.is_nan() {
        return "nan".to_owned();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf".to_owned() } else { "-inf".to_owned() };
    }
    format!("{x:.p$}")
}

/// `enclose(a, b)`.
pub fn enclose(a: &Sphere, b: &Sphere) -> Sphere {
    let d = dist(a.c, b.c);
    if d + b.r <= a.r {
        return *a;
    }
    if d + a.r <= b.r {
        return *b;
    }
    let r = (d + a.r + b.r) / 2.0;
    let t = (r - a.r) / d;
    let mut c = [0.0; 3];
    for i in 0..3 {
        c[i] = a.c[i] + (b.c[i] - a.c[i]) * t;
    }
    Sphere { c, r }
}

// ------------------------------------------------------------------------------------------------ the tree

pub type Id = usize;

/// `Node`: a leaf (its index) or an internal node (left, right), in an arena.
#[derive(Debug, Clone)]
pub struct Node {
    pub data: NodeData,
    pub left: Option<Id>,
    pub right: Option<Id>,
    pub leaf: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct Arena {
    pub nodes: Vec<Node>,
}

impl Arena {
    fn push(&mut self, data: NodeData, left: Option<Id>, right: Option<Id>, leaf: Option<i64>) -> Id {
        self.nodes.push(Node {
            data,
            left,
            right,
            leaf,
        });
        self.nodes.len() - 1
    }

    pub fn sphere(&self, n: Id) -> Sphere {
        decode(&self.nodes[n].data)
    }

    pub fn max(&self, n: Id) -> f64 {
        self.nodes[n].data.max
    }

    fn kids(&self, n: Id) -> CullResult<(Id, Id)> {
        match (self.nodes[n].left, self.nodes[n].right) {
            (Some(l), Some(r)) => Ok((l, r)),
            _ => Err(PY),
        }
    }

    /// `flatten(root)`: preorder node data with Right recomputed (and set) from the left subtree's size.
    pub fn flatten(&mut self, root: Id) -> Vec<NodeData> {
        let mut sizes: HashMap<Id, i64> = HashMap::new();
        let mut st = vec![(root, false)];
        while let Some((n, done)) = st.pop() {
            if self.nodes[n].leaf.is_some() {
                sizes.insert(n, 1);
            } else if done {
                let (l, r) = (self.nodes[n].left.unwrap_or(n), self.nodes[n].right.unwrap_or(n));
                let s = 1 + sizes.get(&l).copied().unwrap_or(0) + sizes.get(&r).copied().unwrap_or(0);
                sizes.insert(n, s);
            } else {
                st.push((n, true));
                if let (Some(l), Some(r)) = (self.nodes[n].left, self.nodes[n].right) {
                    st.push((r, false));
                    st.push((l, false));
                }
            }
        }
        let mut out = Vec::new();
        let mut pre = vec![root];
        while let Some(n) = pre.pop() {
            if self.nodes[n].leaf.is_some() {
                out.push(self.nodes[n].data);
            } else if let (Some(l), Some(r)) = (self.nodes[n].left, self.nodes[n].right) {
                self.nodes[n].data.right = 1 + sizes.get(&l).copied().unwrap_or(0);
                out.push(self.nodes[n].data);
                pre.push(r);
                pre.push(l);
            }
        }
        out
    }

    /// `walk(root)`: (node, ancestors) for every node, preorder.
    pub fn walk(&self, root: Id) -> Vec<(Id, Vec<Id>)> {
        let mut out = Vec::new();
        let mut st: Vec<(Id, Vec<Id>)> = vec![(root, Vec::new())];
        while let Some((n, anc)) = st.pop() {
            if self.nodes[n].leaf.is_none()
                && let (Some(l), Some(r)) = (self.nodes[n].left, self.nodes[n].right)
            {
                let mut a = anc.clone();
                a.push(n);
                st.push((r, a.clone()));
                st.push((l, a));
            }
            out.push((n, anc));
        }
        out
    }
}

/// `build(nodes)`: the preorder list as a linked tree, every Right offset checked: (root, used node count).
pub fn build(arena: &mut Arena, nodes: &[NodeData]) -> CullResult<(Id, usize)> {
    if nodes.is_empty() || nodes[0].right == 0 {
        return Err(cte("the tree is empty"));
    }
    let mut root: Option<Id> = None;
    let mut work: Vec<(i64, Option<(Id, bool)>)> = vec![(0, None)];
    let mut order: Vec<i64> = Vec::new();
    while let Some((i, parent)) = work.pop() {
        if i < 0 || i as usize >= nodes.len() {
            return Err(cte(format!("node {i} is past the end of the tree")));
        }
        let d = nodes[i as usize];
        let n = if d.right < 0 {
            arena.push(d, None, None, Some(d.right & 0x7FFF_FFFF))
        } else {
            if d.right < 2 {
                return Err(cte(format!("node {i}: Right {} cannot hold a left subtree", d.right)));
            }
            let n = arena.push(d, None, None, None);
            work.push((i + d.right, Some((n, true))));
            work.push((i + 1, Some((n, false))));
            n
        };
        order.push(i);
        if order.len() > nodes.len() {
            return Err(cte("the Right offsets loop back into the tree"));
        }
        match parent {
            None => root = Some(n),
            Some((p, true)) => arena.nodes[p].right = Some(n),
            Some((p, false)) => arena.nodes[p].left = Some(n),
        }
    }
    let used = order.len();
    let root = root.ok_or(PY)?;
    let flat = arena.flatten(root);
    if flat.len() != used || flat.iter().zip(nodes).any(|(f, n)| f.right != n.right) {
        return Err(cte(
            "the Right offsets do not describe one preorder tree (subtrees overlap or leave gaps)",
        ));
    }
    Ok((root, used))
}

/// `CullTree`.
#[derive(Debug, Clone)]
pub struct CullTree {
    pub path: String,
    /// The tree's document (empty for a tree only read: [`CullTree::read_only`]).
    pub doc: EditDbx,
    /// The cull tree instance: its index among the root's children.
    inst: usize,
    /// Its `Nodes` field: the index among the instance's children.
    array: usize,
    pub enum_offset: i64,
    pub nodes: Vec<NodeData>,
    pub arena: Arena,
    pub root: Id,
    pub used: usize,
    pub spare: usize,
    /// leaf index -> its node (None: held by add until it is linked)
    pub leaf_node: HashMap<i64, Option<Id>>,
}

const NODE_FIELDS: [&str; 6] = ["Right", "Max", "SphereKeyX", "SphereKeyY", "SphereKeyZ", "SphereKeyW"];

/// A streamed `Nodes` array of a root child ([`CullTree::read_only`]): (its index among the child's children, the
/// nodes up to the first that fails, that failure).
type Streamed = (usize, Vec<NodeData>, Option<CullError>);

/// The cull tree instance's `Nodes` and `EnumOffset` fields: (the array's index among its children, `EnumOffset`).
fn fields(d: &EditDbx, e: NodeId, base: &str) -> CullResult<(usize, i64)> {
    let array = child_index(d, e, "Nodes")?;
    let eo = child_index(d, e, "EnumOffset")?;
    let (Some(array), Some(eo)) = (array, eo) else {
        return Err(cte(format!("{base}: the cull tree has no Nodes or EnumOffset field")));
    };
    Ok((array, int4(d, kid(d, e, eo)?)?))
}

fn node_of(d: &EditDbx, c: NodeId) -> CullResult<NodeData> {
    let mut v: HashMap<Option<String>, Vec<u8>> = HashMap::new();
    for &f in d.children(c).unwrap_or_default() {
        let (_size, vals) = numbers(d, f)?;
        let first = vals.into_iter().next().ok_or(PY)?;
        v.insert(name_of(d, f)?.map(str::to_owned), first);
    }
    let get = |k: &str| -> CullResult<Vec<u8>> {
        v.get(&Some(k.to_owned()))
            .cloned()
            .ok_or_else(|| cte(format!("a cull tree node has no field '{k}'")))
    };
    let four = |b: Vec<u8>| -> CullResult<[u8; 4]> { b.as_slice().try_into().map_err(|_| PY) };
    let right = i64::from(i32::from_be_bytes(four(get("Right")?)?));
    let max = f64::from(f32::from_be_bytes(four(get("Max")?)?));
    let mut keys = [0i64; 4];
    for (i, a) in ["X", "Y", "Z", "W"].iter().enumerate() {
        let b = get(&format!("SphereKey{a}"))?;
        let two: [u8; 2] = b.get(..2).ok_or(PY)?.try_into().map_err(|_| PY)?;
        keys[i] = i64::from(u16::from_be_bytes(two));
    }
    Ok(NodeData { right, max, keys })
}

/// The nodes of a `Nodes` array element.
fn nodes_of(d: &EditDbx, array: NodeId) -> CullResult<Vec<NodeData>> {
    d.children(array)
        .unwrap_or_default()
        .iter()
        .map(|&c| node_of(d, c))
        .collect()
}

/// The chosen cull tree instance's fields: (its `Nodes` array's index among its children, `EnumOffset`, the array's
/// nodes as read from the instance itself).
type Chosen = (usize, i64, CullResult<Vec<NodeData>>);

/// What [`CullTree::read_only`] keeps of the streamed root children (its sink).
struct TreeSink {
    /// the file's name (for the fields' error)
    base: String,
    /// a tag or type that is the reference's own exception
    bad: bool,
    /// the first cull tree instance: its index, then its fields and (in case its `Nodes` were not streamed) its
    /// array's nodes, as read from the instance itself
    chosen: Option<(usize, CullResult<Chosen>)>,
    /// the streamed `Nodes` items of the root child being read
    items: Vec<Streamed>,
    nodes_of_chosen: Vec<Streamed>,
}

impl raw::Sink for TreeSink {
    fn child(&mut self, i: usize, d: &EditDbx, e: NodeId) {
        let mine = std::mem::take(&mut self.items);
        match st(d, d.tag_id(e)) {
            Err(_) => self.bad = true,
            Ok("instance") => match attr(d, e, "type") {
                Err(_) => self.bad = true,
                Ok(Some("World.StaticCullTreeData")) if self.chosen.is_none() => {
                    let read = fields(d, e, &self.base).map(|(array, eo)| {
                        let nodes = kid(d, e, array).and_then(|a| nodes_of(d, a));
                        (array, eo, nodes)
                    });
                    self.chosen = Some((i, read));
                    self.nodes_of_chosen = mine;
                }
                Ok(_) => {}
            },
            Ok(_) => {}
        }
    }

    fn streamed(&mut self, d: &EditDbx, e: NodeId) -> bool {
        matches!(name_of(d, e), Ok(Some("Nodes")))
    }

    fn item(&mut self, _: usize, slot: usize, d: &EditDbx, e: NodeId) {
        if self.items.last().is_none_or(|x| x.0 != slot) {
            self.items.push((slot, Vec::new(), None));
        }
        if let Some(x) = self.items.last_mut()
            && x.2.is_none()
        {
            match node_of(d, e) {
                Ok(n) => x.1.push(n),
                Err(e) => x.2 = Some(e),
            }
        }
    }
}

impl CullTree {
    pub fn read(path: &str) -> CullResult<Self> {
        let doc = read_doc(path)?;
        let base = basename(path);
        let mut found = Vec::new();
        for i in instances(&doc)? {
            if attr(&doc, instance(&doc, i)?, "type")? == Some("World.StaticCullTreeData") {
                found.push(i);
            }
        }
        let Some(&inst) = found.first() else {
            return Err(cte(format!(
                "{base} has no World.StaticCullTreeData instance (not a cull tree)"
            )));
        };
        let e = instance(&doc, inst)?;
        let (array, enum_offset) = fields(&doc, e, &base)?;
        let nodes = nodes_of(&doc, kid(&doc, e, array)?)?;
        Self::with_nodes(path, doc, inst, array, enum_offset, nodes)
    }

    /// `CullTree(path)` for a tree that is only read (round CT1: the reference levels' trees, a written tree read
    /// back): the same nodes, values and errors as [`CullTree::read`], the file streamed - its nodes taken as they
    /// are read instead of the whole document kept. Its document is empty: it cannot be written.
    pub fn read_only(path: &str) -> CullResult<Self> {
        let data = read(path)?;
        let base = basename(path);
        let (head, strings, pos) = header(&data, &base)?;
        // CullTree.__init__'s order: the document (its element errors first), `instances` (every root child's tag),
        // every instance's type, then the first cull tree's fields and nodes
        let mut sink = TreeSink {
            base: base.clone(),
            bad: false,
            chosen: None,
            items: Vec::new(),
            nodes_of_chosen: Vec::new(),
        };
        let walked = raw::stream(&data, head, strings, pos, &mut sink);
        let has_root = walked.map_err(|e| element_error(&base, &e))?;
        if !has_root || sink.bad {
            return Err(PY);
        }
        let Some((inst, read)) = sink.chosen else {
            return Err(cte(format!(
                "{base} has no World.StaticCullTreeData instance (not a cull tree)"
            )));
        };
        let (array, enum_offset, kept) = read?;
        let nodes = match sink.nodes_of_chosen.into_iter().find(|x| x.0 == array) {
            Some((_, _, Some(fail))) => return Err(fail),
            Some((_, nodes, None)) => nodes,
            // not streamed: a Nodes field that is not an array (no items), or one whose items are kept
            None => kept?,
        };
        Self::with_nodes(path, EditDbx::new(), inst, array, enum_offset, nodes)
    }

    /// The tree from its nodes: the used ones linked and checked, the spare ones counted, the leaves indexed.
    fn with_nodes(
        path: &str,
        doc: EditDbx,
        inst: usize,
        array: usize,
        enum_offset: i64,
        nodes: Vec<NodeData>,
    ) -> CullResult<Self> {
        let base = basename(path);
        let mut used = nodes.len();
        while used > 0 && nodes[used - 1] == NodeData::ZERO {
            used -= 1;
        }
        let mut arena = Arena::default();
        let (root, reached) = build(&mut arena, &nodes[..used])?;
        if reached != used {
            return Err(cte(format!(
                "{base}: {} nodes before the spare ones are not reached from the root",
                used - reached
            )));
        }
        let spare = nodes.len() - used;
        let mut leaf_node = HashMap::new();
        for (n, _) in arena.walk(root) {
            if let Some(l) = arena.nodes[n].leaf {
                if leaf_node.contains_key(&l) {
                    return Err(cte(format!("{base}: leaf index {l} is used twice")));
                }
                leaf_node.insert(l, Some(n));
            }
        }
        Ok(Self {
            path: path.to_owned(),
            doc,
            inst,
            array,
            enum_offset,
            nodes,
            arena,
            root,
            used,
            spare,
            leaf_node,
        })
    }

    pub fn leaves(&self) -> i64 {
        self.leaf_node.len() as i64
    }

    /// A leaf's linked node.
    pub fn leaf(&self, i: i64) -> Option<Id> {
        self.leaf_node.get(&i).copied().flatten()
    }

    pub fn leaf_range_ok(&self) -> bool {
        let mut k: Vec<i64> = self.leaf_node.keys().copied().collect();
        k.sort_unstable();
        k.iter().enumerate().all(|(i, &v)| v == i as i64 + 1)
    }

    pub fn depth_and_stack(&self) -> (usize, usize) {
        let (mut depth, mut most) = (0, 0);
        let mut stack = vec![(self.root, 0usize)];
        while !stack.is_empty() {
            most = most.max(stack.len());
            let Some((n, d)) = stack.pop() else { break };
            depth = depth.max(d);
            if self.arena.nodes[n].leaf.is_none()
                && let (Some(l), Some(r)) = (self.arena.nodes[n].left, self.arena.nodes[n].right)
            {
                stack.push((r, d + 1));
                stack.push((l, d + 1));
            }
        }
        (depth, most)
    }

    /// `write(path)`: the nodes from the first one's fields (values replaced, big-endian, same sizes), 256 spare.
    pub fn write(&mut self, path: &str) -> CullResult<()> {
        let mut flat = self.arena.flatten(self.root);
        flat.extend(std::iter::repeat_n(NodeData::ZERO, SPARE));
        let mut doc = self.doc.clone();
        let array = kid(&doc, instance(&doc, self.inst)?, self.array)?;
        let tmpl = *doc.children(array).and_then(<[NodeId]>::first).ok_or(PY)?;
        let mut items = Vec::with_capacity(flat.len());
        for d in &flat {
            let c = clone_stored(&mut doc, tmpl);
            for f in doc.children(c).unwrap_or_default().to_vec() {
                let nm = name_of(&doc, f)?.map(str::to_owned);
                let (size, _) = numbers(&doc, f)?;
                match nm.as_deref() {
                    Some("Right") => {
                        let v = i32::try_from(d.right).map_err(|_| PY)?;
                        set_numbers(&mut doc, f, 4, &[v.to_be_bytes().to_vec()])?;
                    }
                    Some("Max") => {
                        let x = d.max as f32;
                        if x.is_infinite() && d.max.is_finite() {
                            return Err(PY); // struct's OverflowError
                        }
                        set_numbers(&mut doc, f, 4, &[x.to_be_bytes().to_vec()])?;
                    }
                    Some(n) if NODE_FIELDS.contains(&n) => {
                        let axis = "XYZW".find(n.chars().last().unwrap_or('X')).unwrap_or(0);
                        let k = u16::try_from(d.keys[axis]).map_err(|_| PY)?;
                        let mut v = k.to_be_bytes().to_vec();
                        v.extend(std::iter::repeat_n(0u8, (size as usize).saturating_sub(2)));
                        set_numbers(&mut doc, f, size, &[v])?;
                    }
                    _ => {}
                }
            }
            items.push(c);
        }
        *doc.children_mut(array).map_err(|_| PY)? = items;
        write_doc(&doc, path)
    }
}

// ------------------------------------------------------------------------------------------------ levels

/// `Placed`: a placed object.
#[derive(Debug, Clone)]
pub struct Placed {
    pub guid: String,
    pub blueprint: String,
    pub m: [[f64; 4]; 4],
    pub enumeration: i64,
    /// (instance index, child index) of its Enumeration field
    el: (usize, usize),
}

impl Placed {
    pub fn pos(&self) -> Vec3 {
        [self.m[3][0], self.m[3][1], self.m[3][2]]
    }

    pub fn to_world(&self, l: Vec3) -> Vec3 {
        let m = &self.m;
        let mut out = [0.0; 3];
        for i in 0..3 {
            out[i] = l[0] * m[0][i] + l[1] * m[1][i] + l[2] * m[2][i] + m[3][i];
        }
        out
    }

    pub fn to_local(&self, w: Vec3) -> CullResult<Vec3> {
        let m = &self.m;
        let inv = inv3([
            [m[0][0], m[0][1], m[0][2]],
            [m[1][0], m[1][1], m[1][2]],
            [m[2][0], m[2][1], m[2][2]],
        ])?;
        let d = [w[0] - m[3][0], w[1] - m[3][1], w[2] - m[3][2]];
        let mut out = [0.0; 3];
        for i in 0..3 {
            out[i] = d[0] * inv[0][i] + d[1] * inv[1][i] + d[2] * inv[2][i];
        }
        Ok(out)
    }

    pub fn scale(&self) -> f64 {
        let n =
            |i: usize| (self.m[i][0] * self.m[i][0] + self.m[i][1] * self.m[i][1] + self.m[i][2] * self.m[i][2]).sqrt();
        py_max(&[n(0), n(1), n(2)])
    }
}

fn inv3(r: [[f64; 3]; 3]) -> CullResult<[[f64; 3]; 3]> {
    let [a, b, c] = r;
    let det =
        a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0]);
    if det.abs() < 1e-12 {
        return Err(cte("an object has a transform that cannot be inverted (zero scale)"));
    }
    let m = [
        [
            b[1] * c[2] - b[2] * c[1],
            a[2] * c[1] - a[1] * c[2],
            a[1] * b[2] - a[2] * b[1],
        ],
        [
            b[2] * c[0] - b[0] * c[2],
            a[0] * c[2] - a[2] * c[0],
            a[2] * b[0] - a[0] * b[2],
        ],
        [
            b[0] * c[1] - b[1] * c[0],
            a[1] * c[0] - a[0] * c[1],
            a[0] * b[1] - a[1] * b[0],
        ],
    ];
    Ok(m.map(|row| row.map(|x| x / det)))
}

/// `Level`.
#[derive(Debug, Clone)]
pub struct Level {
    pub path: String,
    pub doc: EditDbx,
    pub objects: Vec<Placed>,
    pub tree_refs: HashMap<String, String>,
}

/// `Level.__init__`'s loop body for one instance (root child `i`): its tree references and its placed object.
fn scan_instance(
    d: &EditDbx,
    e: NodeId,
    i: usize,
    objects: &mut Vec<Placed>,
    tree_refs: &mut HashMap<String, String>,
) -> CullResult<()> {
    for f in ["ExclusiveStaticCullTree", "ClientStaticCullTree"] {
        if let Some(c) = child_index(d, e, f)?
            && let Some(r) = attr(d, kid(d, e, c)?, "ref")?
            && !r.is_empty()
        {
            tree_refs.insert(f.to_owned(), r.to_owned());
        }
    }
    if attr(d, e, "type")? != Some("Entity.ReferenceObjectData") {
        return Ok(());
    }
    let (en, tr, ro) = (
        child_index(d, e, "Enumeration")?,
        child_index(d, e, "Transform")?,
        child_index(d, e, "ReferencedObject")?,
    );
    let (Some(en), Some(tr)) = (en, tr) else {
        return Ok(());
    };
    let guid = attr(d, e, "guid")?.unwrap_or("").to_owned();
    let rf = match ro {
        Some(ro) => attr(d, kid(d, e, ro)?, "ref")?,
        None => None,
    };
    let blueprint = match rf {
        Some(r) if r.contains('/') => r.rsplit_once('/').map_or("", |x| x.0).to_lowercase(),
        Some(r) => r.to_owned(),
        None => String::new(),
    };
    let v = floats(d, kid(d, e, tr)?)?;
    if v.len() != 16 {
        return Ok(());
    }
    let enumeration = int4(d, kid(d, e, en)?)?;
    let mut m = [[0.0; 4]; 4];
    for (k, x) in v.iter().enumerate() {
        m[k / 4][k % 4] = *x;
    }
    objects.push(Placed {
        guid,
        blueprint,
        m,
        enumeration,
        el: (i, en),
    });
    Ok(())
}

fn no_objects(path: &str) -> CullError {
    cte(format!(
        "{} has no placed objects (Entity.ReferenceObjectData): not a level partition",
        basename(path)
    ))
}

impl Level {
    pub fn read(path: &str) -> CullResult<Self> {
        let doc = read_doc(path)?;
        let mut objects = Vec::new();
        let mut tree_refs = HashMap::new();
        for i in instances(&doc)? {
            scan_instance(&doc, instance(&doc, i)?, i, &mut objects, &mut tree_refs)?;
        }
        if objects.is_empty() {
            return Err(no_objects(path));
        }
        Ok(Self {
            path: path.to_owned(),
            doc,
            objects,
            tree_refs,
        })
    }

    /// `by_guid(guid)`: the object's index.
    pub fn by_guid(&self, guid: &str) -> Option<usize> {
        by_guid(&self.objects, guid)
    }

    pub fn tree_path(&self) -> Option<String> {
        tree_path_of(
            &self.path,
            self.tree_refs.get("ExclusiveStaticCullTree").map(String::as_str),
        )
    }

    /// `set_enum(o, value)`.
    pub fn set_enum(&mut self, o: usize, value: i64) -> CullResult<()> {
        let v = i32::try_from(value).map_err(|_| PY)?;
        let (i, c) = self.objects[o].el;
        let el = kid(&self.doc, instance(&self.doc, i)?, c)?;
        set_numbers(&mut self.doc, el, 4, &[v.to_be_bytes().to_vec()])?;
        self.objects[o].enumeration = value;
        Ok(())
    }
}

/// `by_guid(guid)`: the object's index.
fn by_guid(objects: &[Placed], guid: &str) -> Option<usize> {
    let g = bc2_pyrt::py::strip(guid).to_lowercase();
    objects.iter().position(|o| o.guid.to_lowercase() == g)
}

/// A level's placed objects and tree references without its document (round CT1): what a reference level gives
/// `collect`, read by [`scan_level`] with the memory of one instance at a time.
#[derive(Debug, Clone)]
pub struct Objects {
    pub path: String,
    pub objects: Vec<Placed>,
    pub tree_refs: HashMap<String, String>,
}

impl Objects {
    /// [`Level::by_guid`].
    pub fn by_guid(&self, guid: &str) -> Option<usize> {
        by_guid(&self.objects, guid)
    }
}

/// What [`scan_level`] keeps of the streamed root children (its sink): `Level.__init__`'s loop.
struct LevelSink {
    objects: Vec<Placed>,
    tree_refs: HashMap<String, String>,
    bad_tag: bool,
    first: Option<CullError>,
}

impl raw::Sink for LevelSink {
    fn child(&mut self, i: usize, d: &EditDbx, e: NodeId) {
        match st(d, d.tag_id(e)) {
            Err(_) => self.bad_tag = true,
            Ok("instance") if !self.bad_tag && self.first.is_none() => {
                if let Err(err) = scan_instance(d, e, i, &mut self.objects, &mut self.tree_refs) {
                    self.first = Some(err);
                }
            }
            Ok(_) => {}
        }
    }

    fn streamed(&mut self, _: &EditDbx, _: NodeId) -> bool {
        false
    }

    fn item(&mut self, _: usize, _: usize, _: &EditDbx, _: NodeId) {}
}

/// `Level(path)`'s objects and tree references (the same values and the same errors as [`Level::read`]), the file
/// streamed: the first root's children are read and dropped one by one instead of the whole document kept.
pub fn scan_level(path: &str) -> CullResult<Objects> {
    let data = read(path)?;
    let name = basename(path);
    let (head, strings, pos) = header(&data, &name)?;
    // Level.__init__'s order: the document read whole (its element errors first), `instances` (every root child's
    // tag looked up), then each instance in turn, then the check for objects
    let mut sink = LevelSink {
        objects: Vec::new(),
        tree_refs: HashMap::new(),
        bad_tag: false,
        first: None,
    };
    let walked = raw::stream(&data, head, strings, pos, &mut sink);
    let has_root = walked.map_err(|e| element_error(&name, &e))?;
    if !has_root || sink.bad_tag {
        return Err(PY);
    }
    if let Some(e) = sink.first {
        return Err(e);
    }
    if sink.objects.is_empty() {
        return Err(no_objects(path));
    }
    Ok(Objects {
        path: path.to_owned(),
        objects: sink.objects,
        tree_refs: sink.tree_refs,
    })
}

/// `tree_path_of(path, ref)`: levels/<x>/staticculltree_* beside the level's own folder, matched without case.
pub fn tree_path_of(path: &str, rf: Option<&str>) -> Option<String> {
    let rf = rf.filter(|r| !r.is_empty())?;
    let all: Vec<&str> = rf.split('/').collect();
    let mut parts = &all[..all.len() - 1];
    let base = dirname(&abspath(path));
    while let Some(p) = parts.first() {
        if p.to_lowercase() == "levels" {
            break;
        }
        parts = &parts[1..];
    }
    if parts.is_empty() {
        return None;
    }
    let rest = &parts[1..];
    let mut p = base;
    for (i, q) in rest.iter().enumerate() {
        let last = i == rest.len() - 1;
        let want = if last {
            format!("{}.dbx", q.to_lowercase())
        } else {
            q.to_lowercase()
        };
        let names: Vec<String> = std::fs::read_dir(&p)
            .ok()?
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        let mut m: Vec<String> = names
            .into_iter()
            .filter(|f| {
                f.to_lowercase() == want && {
                    let full = join(&p, &[f]);
                    if last { isfile(&full) } else { isdir(&full) }
                }
            })
            .collect();
        if m.is_empty() {
            return None;
        }
        m.sort();
        p = join(&p, &[&m[0]]);
    }
    Some(p)
}

/// `leaves_of(tree, level)`: (object, its leaf indices) for the level's slotted objects, by slot.
pub fn leaves_of(tree: &CullTree, level: &Level) -> Vec<(usize, Vec<i64>)> {
    leaves_in(tree, &level.objects)
}

/// [`leaves_of`] over a level's objects.
pub fn leaves_in(tree: &CullTree, objects: &[Placed]) -> Vec<(usize, Vec<i64>)> {
    let eo = tree.enum_offset;
    let last = eo + tree.leaves();
    let mut slotted: Vec<usize> = (0..objects.len())
        .filter(|&i| eo < objects[i].enumeration && objects[i].enumeration <= last)
        .collect();
    slotted.sort_by_key(|&i| objects[i].enumeration);
    let mut out = Vec::new();
    for (k, &o) in slotted.iter().enumerate() {
        let en = objects[o].enumeration;
        let nxt = slotted.get(k + 1).map_or(last + 1, |&n| objects[n].enumeration);
        out.push((o, ((en - eo)..(en - eo + 1).max(nxt - eo)).collect()));
    }
    out
}

// ------------------------------------------------------------------------------------------------ shapes

/// A blueprint's leaf in its own space: (local centre, radius key, Max).
pub type ShapeLeaf = (Vec3, f64, f64);

/// `Instance`: (label, leaf count, leaves, the target level's object or None). The label and leaves are shared
/// (round CT1): `collect` copies every reference row into its result, and does it again after `--misplaced`.
#[derive(Debug, Clone)]
pub struct Instance {
    pub label: Arc<str>,
    pub k: usize,
    pub leaves: Arc<[ShapeLeaf]>,
    pub obj: Option<usize>,
}

pub type Found = OMap<String, Vec<Instance>>;

/// `Shape`.
#[derive(Debug, Clone)]
pub struct Shape {
    pub leaves: Vec<ShapeLeaf>,
    pub source: String,
    pub count: usize,
    pub confirmed: usize,
}

impl Shape {
    /// `place(o)`: (world sphere, Max) per leaf.
    pub fn place(&self, o: &Placed) -> Vec<(Sphere, f64)> {
        let s = o.scale();
        self.leaves
            .iter()
            .map(|&(c, w, mx)| {
                (
                    Sphere {
                        c: o.to_world(c),
                        r: (w + 1.0) * s / RADIUS_SCALE,
                    },
                    mx * s,
                )
            })
            .collect()
    }
}

fn median(vals: &mut [f64]) -> f64 {
    vals.sort_by(f64::total_cmp);
    let n = vals.len();
    if n % 2 == 1 {
        vals[n / 2]
    } else {
        (vals[n / 2 - 1] + vals[n / 2]) / 2.0
    }
}

/// The target pair's rows (`collect([(tree, level, label)])`).
pub fn collect_target(tree: &CullTree, level: &Level, label: &str, found: &mut Found) -> CullResult<()> {
    collect_in(tree, &level.objects, label, found)
}

/// [`collect_target`] over a level's objects.
pub fn collect_in(tree: &CullTree, objects: &[Placed], label: &str, found: &mut Found) -> CullResult<()> {
    let label: Arc<str> = Arc::from(label);
    for (o, lv) in leaves_in(tree, objects) {
        let obj = &objects[o];
        let s = {
            let s = obj.scale();
            if s == 0.0 { 1.0 } else { s }
        };
        let mut sph = Vec::new();
        for li in &lv {
            let Some(n) = tree.leaf(*li) else { break };
            let d = tree.arena.sphere(n);
            let data = tree.arena.nodes[n].data;
            sph.push((obj.to_local(d.c)?, data.keys[3] as f64 / s, data.max / s));
        }
        found.entry_or(obj.blueprint.clone(), Vec::new).push(Instance {
            label: label.clone(),
            k: lv.len(),
            leaves: Arc::from(sph),
            obj: Some(o),
        });
    }
    Ok(())
}

fn consensus(group: &[&Instance], k: usize, tol: f64) -> Vec<usize> {
    if group.is_empty() {
        return Vec::new();
    }
    let med: Vec<Vec3> = (0..k)
        .map(|j| {
            let mut c = [0.0; 3];
            for a in 0..3 {
                let mut v: Vec<f64> = group.iter().map(|g| g.leaves[j].0[a]).collect();
                c[a] = median(&mut v);
            }
            c
        })
        .collect();
    let good: Vec<usize> = (0..group.len())
        .filter(|&i| (0..k).all(|j| dist(group[i].leaves[j].0, med[j]) <= tol))
        .collect();
    if good.len() * 2 > group.len() { good } else { Vec::new() }
}

fn shape_leaves(good: &[&Instance], k: usize) -> Vec<ShapeLeaf> {
    (0..k)
        .map(|j| {
            let mut c = [0.0; 3];
            for a in 0..3 {
                let mut v: Vec<f64> = good.iter().map(|g| g.leaves[j].0[a]).collect();
                c[a] = median(&mut v);
            }
            let ws: Vec<f64> = good.iter().map(|g| g.leaves[j].1).collect();
            let mut mx: Vec<f64> = good.iter().map(|g| g.leaves[j].2).collect();
            (c, py_max(&ws), median(&mut mx))
        })
        .collect()
}

/// `shape_for(blueprint, found)` over the given instances (tolerance 0.5 m).
pub fn shape_for(blueprint: &str, inst: &[&Instance]) -> Option<Shape> {
    let tol = 0.5;
    if inst.is_empty() {
        return None;
    }
    let _ = blueprint;
    // Counter.most_common(1): the first of the most frequent leaf counts
    let mut counts: Vec<(usize, usize)> = Vec::new();
    for x in inst {
        match counts.iter_mut().find(|c| c.0 == x.k) {
            Some(c) => c.1 += 1,
            None => counts.push((x.k, 1)),
        }
    }
    let mut k = counts[0];
    for c in &counts[1..] {
        if c.1 > k.1 {
            k = *c;
        }
    }
    let k = k.0;
    let inst: Vec<&Instance> = inst
        .iter()
        .copied()
        .filter(|x| x.k == k && x.leaves.len() == k)
        .collect();
    let mut labels: Vec<&str> = Vec::new();
    for x in &inst {
        if !labels.contains(&&*x.label) {
            labels.push(&x.label);
        }
    }
    let mut groups: Vec<(&str, Vec<&Instance>, Vec<ShapeLeaf>)> = Vec::new();
    for label in labels {
        let group: Vec<&Instance> = inst.iter().copied().filter(|x| *x.label == *label).collect();
        let good: Vec<&Instance> = consensus(&group, k, tol).into_iter().map(|i| group[i]).collect();
        if !good.is_empty() {
            let leaves = shape_leaves(&good, k);
            groups.push((label, good, leaves));
        }
    }
    if groups.is_empty() {
        return None;
    }
    let agree = |a: &[ShapeLeaf], b: &[ShapeLeaf]| {
        a.iter()
            .zip(b)
            .all(|(x, y)| dist(x.0, y.0) <= tol && (x.2 - y.2).abs() <= 0.01 * py_max(&[x.2, y.2, 1e-6]))
    };
    for (label, good, leaves) in &groups {
        let others: usize = groups
            .iter()
            .filter(|(lb, _, lv)| lb != label && agree(leaves, lv))
            .map(|g| g.1.len())
            .sum();
        if good.len() >= 3 || others > 0 {
            return Some(Shape {
                leaves: leaves.clone(),
                source: (*label).to_owned(),
                count: good.len(),
                confirmed: others,
            });
        }
    }
    let mut best = &groups[0];
    for g in &groups[1..] {
        if g.1.len() > best.1.len() {
            best = g;
        }
    }
    Some(Shape {
        leaves: best.2.clone(),
        source: best.0.to_owned(),
        count: best.1.len(),
        confirmed: 0,
    })
}

/// `shape_for(blueprint, found)`.
pub fn shape_in(blueprint: &str, found: &Found) -> Option<Shape> {
    let v: Vec<&Instance> = found
        .get(&blueprint.to_lowercase())
        .map_or_else(Vec::new, |v| v.iter().collect());
    shape_for(blueprint, &v)
}

// ------------------------------------------------------------------------------------------------ references

/// `level_partitions(folder)`: every .dbx under the folder (sorted) that is not a tree or a description and holds
/// `ExclusiveStaticCullTree`.
pub fn level_partitions(folder: &str) -> CullResult<Vec<String>> {
    level_partitions_in(folder, None)
}

/// A folder's searched files as the listing cache keeps them: (path below the folder, size, modification time,
/// holds the mark).
pub type Listed = (String, u64, i128, bool);

/// [`level_partitions`], the search's answers kept in `store` (round CT1): a file whose size and modification time
/// in the folder listing are those it had when it was searched is not read again. A file that cannot be read is
/// never kept (it is read, and raises, every time).
pub fn level_partitions_in(folder: &str, store: Option<&dyn RowStore>) -> CullResult<Vec<String>> {
    let rels = raw::dbx_files_stat(std::path::Path::new(folder));
    // (full path, path below the folder, listed stat)
    let mut files = Vec::with_capacity(rels.len());
    for (r, st) in rels {
        let r = r.to_str().ok_or(PY)?.to_owned(); // a name glob lists that is not Unicode
        files.push((join(folder, &[&r]), r, st));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let cands: Vec<(String, String, Option<raw::ListedStat>)> = files
        .into_iter()
        .filter(|(p, _, _)| {
            let b = basename(p).to_lowercase();
            !b.contains("staticculltree") && !b.contains("_description")
        })
        .collect();
    let kept: HashMap<String, (u64, i128, bool)> =
        store.and_then(|s| s.get_list(folder)).map_or_else(HashMap::new, |l| {
            l.into_iter().map(|(r, a, b, h)| (r, (a, b, h))).collect()
        });
    let mut hits: Vec<Option<bool>> = cands
        .iter()
        .map(|(_, r, st)| {
            let (sz, ns) = (*st)?;
            kept.get(r).filter(|k| k.0 == sz && k.1 == ns).map(|k| k.2)
        })
        .collect();
    let todo: Vec<usize> = (0..cands.len()).filter(|&i| hits[i].is_none()).collect();
    let read_now: Vec<&str> = todo.iter().map(|&i| cands[i].0.as_str()).collect();
    for (i, h) in todo.iter().zip(raw::contains(&read_now, LEVEL_MARK)) {
        hits[*i] = h;
    }
    if let Some(s) = store {
        let list: Vec<Listed> = cands
            .iter()
            .zip(&hits)
            .filter_map(|((_, r, st), h)| {
                let (sz, ns) = (*st)?;
                Some((r.clone(), sz, ns, (*h)?))
            })
            .collect();
        if !todo.is_empty() || list.len() != kept.len() {
            s.put_list(folder, &list);
        }
    }
    let mut out = Vec::new();
    for ((p, _, _), hit) in cands.into_iter().zip(hits) {
        let hit = if let Some(h) = hit {
            h
        } else {
            let d = read(&p)?;
            d.windows(LEVEL_MARK.len()).any(|w| w == LEVEL_MARK)
        };
        if hit {
            out.push(p);
        }
    }
    Ok(out)
}

/// A reference level's rows (`RefRows`), or (reference mode) the error its `collect()` raises later.
pub struct RefLevel {
    pub label: String,
    /// The rows, or the `CullTreeError` text of the tree's `collect`.
    pub rows: Result<Found, String>,
}

fn same(a: &str) -> String {
    abspath(a).replace('/', "\\").to_lowercase()
}

/// A file's content key (`culltree._stat`, round PR1b-1c): its size and the first 128 bits of its SHA-256 (little-endian,
/// signed: a Python int in the cache's pickle). Rows are reused while the level file and its tree hold the same bytes,
/// whatever their times say (a copy, a restore, a touch); until PR1b-1c `(st_size, st_mtime_ns)`.
pub type Stat = (u64, i128);

/// [`Stat`] of the file at `path`: read whole and hashed (SHA-256 on the CPU's SHA extensions, as Python's
/// `hashlib` is).
///
/// # Errors
/// The file cannot be read.
pub fn content_stat(path: &str) -> io::Result<Stat> {
    let size = std::fs::metadata(path)?.len();
    let hex = bc2_gamefs::sha256::file(path)?;
    let mut b = [0u8; 16];
    for (i, x) in b.iter_mut().enumerate() {
        *x = u8::from_str_radix(hex.get(2 * i..2 * i + 2).unwrap_or("00"), 16).unwrap_or(0);
    }
    Ok((size, i128::from_le_bytes(b)))
}

/// One reference tree in a cache entry: its stat, and its rows or the `CullTreeError` text its read or `collect`
/// raised (`{'stat': (size, mtime), 'rows' or 'error'}`).
#[derive(Debug, Clone)]
pub struct TreeEntry {
    pub stat: Stat,
    pub rows: Result<Found, String>,
}

/// `ref_rows`' cache entry for one reference level file (`culltree._RowCache`): the level's stat; not a level, or
/// its exclusive tree reference; the rows of each tree it was read with (keyed by the tree's `normcase(abspath)`).
#[derive(Debug, Clone, Default)]
pub struct Entry {
    pub level: Stat,
    pub notlevel: bool,
    pub tree_ref: Option<String>,
    pub trees: Vec<(String, TreeEntry)>,
}

impl Entry {
    /// `e['trees'][key] = t`: replaced where it is, else added at the end.
    pub fn set_tree(&mut self, key: String, t: TreeEntry) {
        match self.trees.iter_mut().find(|x| x.0 == key) {
            Some(x) => x.1 = t,
            None => self.trees.push((key, t)),
        }
    }
}

/// Where `ref_rows` keeps the reference levels' rows between runs (round CT1; `culltree._RowCache`). Shared by the
/// threads that read the reference levels.
pub trait RowStore: Sync {
    /// The entry this code made for the level file while it had this stat, if it is kept.
    fn get(&self, path: &str, st: Stat) -> Option<Entry>;
    /// Keeps the entry (a store that cannot be written is not used).
    fn put(&self, path: &str, e: &Entry);
    /// The listing cache's record of a `--ref-dir` folder ([`level_partitions_in`]), if kept.
    fn get_list(&self, _folder: &str) -> Option<Vec<Listed>> {
        None
    }
    /// Keeps the folder's record.
    fn put_list(&self, _folder: &str, _list: &[Listed]) {}
}

/// What one chosen reference tree gives.
enum TreeOut {
    Rows(Found),
    /// a `CullTreeError` of the tree's read (true) or of `collect` (false)
    Note(String, bool),
    /// any other exception: raised
    Raise(CullError),
}

impl TreeOut {
    /// The cache's record of it (None: not kept, the exception is raised again next time).
    fn kept(&self, stat: Stat) -> Option<TreeEntry> {
        match self {
            Self::Rows(f) => Some(TreeEntry {
                stat,
                rows: Ok(f.clone()),
            }),
            Self::Note(e, _) => Some(TreeEntry {
                stat,
                rows: Err(e.clone()),
            }),
            Self::Raise(_) => None,
        }
    }

    fn of(t: TreeEntry) -> Self {
        match t.rows {
            Ok(f) => Self::Rows(f),
            Err(m) => Self::Note(m, true),
        }
    }
}

fn is_cte(e: &CullError) -> bool {
    e.is_cull_tree()
}

/// The tree read and the level's rows collected in it, the objects left out (`collect([(tree, lv, label)])`).
fn tree_rows(objects: &[Placed], tp: &str, label: &str) -> TreeOut {
    let tree = match CullTree::read_only(tp) {
        Ok(t) => t,
        Err(CullError::CullTree(e)) => return TreeOut::Note(e, true),
        Err(e) => return TreeOut::Raise(e),
    };
    let mut found = Found::default();
    match collect_in(&tree, objects, label, &mut found) {
        Ok(()) => {
            for v in &mut found.items {
                for x in &mut v.1 {
                    x.obj = None;
                }
            }
            TreeOut::Rows(found)
        }
        Err(CullError::CullTree(e)) => TreeOut::Note(e, false),
        Err(e) => TreeOut::Raise(e),
    }
}

/// One reference level file read (or found in the cache), before the ordered pass chooses it.
enum Pre {
    Raise(CullError),
    NotLevel,
    /// its exclusive tree is not found beside it
    NoTree,
    /// its tree is the target's
    Skipped,
    Tree {
        st: Stat,
        tp: String,
        size: CullResult<u64>,
        /// the tree's rows: from the cache, or read with the level (None: to be read if chosen)
        out: Option<TreeOut>,
    },
}

/// The level file's part of `ref_rows`' loop that does not depend on the files before it: its stat, the cache
/// entry or the level read (streamed), its tree's path and size, and - when the level was read now or the cache
/// has them - the tree's rows. The entry is written back when it is new or gained a tree.
fn pre(p: &str, store: Option<&dyn RowStore>, skip: &[String]) -> Pre {
    let label = splitext(&basename(p)).0;
    let st = match content_stat(p) {
        Ok(s) => s,
        Err(e) => {
            // `Level(p)` raises as ref_pairs' read does, else the stat's own error
            return Pre::Raise(match scan_level(p) {
                Err(f) => f,
                Ok(_) => CullError::io(p, e),
            });
        }
    };
    let cached = store.and_then(|s| s.get(p, st));
    let mut dirty = cached.is_none();
    let (mut entry, objs) = if let Some(e) = cached {
        (e, None)
    } else {
        let mut e = Entry {
            level: st,
            ..Entry::default()
        };
        match scan_level(p) {
            Ok(o) => {
                e.tree_ref = o.tree_refs.get("ExclusiveStaticCullTree").cloned();
                (e, Some(o))
            }
            Err(f) if is_cte(&f) => {
                e.notlevel = true;
                (e, None)
            }
            Err(f) => return Pre::Raise(f),
        }
    };
    let put = |entry: &Entry, dirty: bool| {
        if dirty && let Some(s) = store {
            s.put(p, entry);
        }
    };
    if entry.notlevel {
        put(&entry, dirty);
        return Pre::NotLevel;
    }
    let Some(tp) = tree_path_of(p, entry.tree_ref.as_deref()) else {
        put(&entry, dirty);
        return Pre::NoTree;
    };
    let key = same(&tp);
    if skip.contains(&key) {
        put(&entry, dirty);
        return Pre::Skipped;
    }
    let size = std::fs::metadata(&tp)
        .map(|m| m.len())
        .map_err(|e| CullError::io(&tp, e));
    let mut out = None;
    if size.is_ok() {
        match content_stat(&tp) {
            Err(e) => out = Some(TreeOut::Raise(CullError::io(&tp, e))),
            Ok(tst) => {
                let hit = entry.trees.iter().position(|(k, t)| *k == key && t.stat == tst);
                if let Some(i) = hit {
                    out = Some(TreeOut::of(entry.trees.swap_remove(i).1));
                    dirty = false; // the entry is not written with that tree taken out
                } else if let Some(o) = &objs {
                    let r = tree_rows(&o.objects, &tp, &label);
                    if let Some(t) = r.kept(tst) {
                        entry.set_tree(key, t);
                        dirty = true;
                    }
                    out = Some(r);
                }
            }
        }
    }
    put(&entry, dirty);
    Pre::Tree { st, tp, size, out }
}

/// How many reference levels [`ref_rows`] reads at once.
const PRE_THREADS: usize = 8;

/// The ordered pass's events: a note, a chosen level (its index in `chosen`), the exception raised there.
enum Event {
    Note(String),
    Tree(usize),
    Raise(CullError),
}

/// `ref_rows(refs, ref_dirs, log, target)` (with `reference`: `ref_pairs`, as `BFBC2_CULLTREE_REFERENCE=1` runs,
/// and no store): the reference levels in order, their notes logged.
///
/// Round CT1: each level file is streamed (one instance at a time, [`scan_level`]) on all cores and only its
/// objects' rows in its tree are kept; the rows come from `store` (`culltree.py`'s row cache) while the level and
/// its tree are unchanged. A tree the cache has no rows for is read with its level when that is read anyway (the
/// rows of levels not chosen are kept for the next run), else - a level from the cache whose tree changed - the
/// level is read again for it once chosen. The `--ref-dir` folders are listed together, the files the store's
/// listing entries know unchanged not searched again ([`level_partitions_in`]). The notes, the choices and any
/// exception come in the reference's order.
pub fn ref_rows(
    refs: &[String],
    ref_dirs: &[String],
    log: &mut dyn FnMut(&str) -> CullResult<()>,
    target: Option<(&str, &str)>,
    reference: bool,
    store: Option<&dyn RowStore>,
) -> CullResult<Vec<RefLevel>> {
    let store = if reference { None } else { store };
    let mut paths: Vec<String> = refs.to_vec();
    // the folders listed together (each one's files are searched on all cores too); the first error in their order
    let lists: Vec<CullResult<Vec<String>>> = ref_dirs.par_iter().map(|d| level_partitions_in(d, store)).collect();
    for l in lists {
        paths.extend(l?);
    }
    let skip: Vec<String> = target.map_or_else(Vec::new, |(a, b)| vec![same(a), same(b)]);
    let todo: Vec<&String> = paths.iter().filter(|p| !skip.contains(&same(p))).collect();
    // at most 8 levels at once (round CT1): each holds its file and tree while it is read; more threads gain
    // little time and cost memory (16 threads: about 30 MB more on the Onslaught runs)
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(rayon::current_num_threads().min(PRE_THREADS))
        .build()
        .map_err(|_| PY)?;
    let pres: Vec<Pre> = pool.install(|| todo.par_iter().map(|p| pre(p, store, &skip)).collect());
    // (level path, its stat, tree path, rows)
    let mut chosen: Vec<(&str, Stat, String, Option<TreeOut>)> = Vec::new();
    let mut events: Vec<Event> = Vec::new();
    let mut seen: Vec<(String, u64)> = Vec::new();
    for (p, r) in todo.iter().zip(pres) {
        match r {
            Pre::Raise(e) => {
                events.push(Event::Raise(e));
                break;
            }
            Pre::NotLevel | Pre::Skipped => {}
            Pre::NoTree => {
                if refs.contains(p) {
                    events.push(Event::Note(format!(
                        "note: {p}: its exclusive cull tree was not found beside it; not used"
                    )));
                }
            }
            Pre::Tree { st, tp, size, out } => {
                let size = match size {
                    Ok(s) => s,
                    Err(e) => {
                        events.push(Event::Raise(e));
                        break;
                    }
                };
                let key = (basename(p).to_lowercase(), size);
                if seen.contains(&key) {
                    continue;
                }
                seen.push(key);
                events.push(Event::Tree(chosen.len()));
                chosen.push((p.as_str(), st, tp, out));
            }
        }
    }
    // the chosen levels from the cache whose tree has no rows there: the level read again (`Level(p)`, its
    // exceptions raised), the rows kept in its entry
    chosen.par_iter_mut().for_each(|(p, st, tp, out)| {
        if out.is_some() {
            return;
        }
        let objs = match scan_level(p) {
            Ok(o) => o,
            Err(e) => {
                *out = Some(TreeOut::Raise(e));
                return;
            }
        };
        let label = splitext(&basename(p)).0;
        let r = tree_rows(&objs.objects, tp, &label);
        if let Some(s) = store
            && let Ok(tst) = content_stat(tp)
            && let Some(t) = r.kept(tst)
        {
            let mut e = s.get(p, *st).unwrap_or_else(|| Entry {
                level: *st,
                tree_ref: objs.tree_refs.get("ExclusiveStaticCullTree").cloned(),
                ..Entry::default()
            });
            e.set_tree(same(tp), t);
            s.put(p, &e);
        }
        *out = Some(r);
    });
    let mut out = Vec::new();
    for ev in events {
        let i = match ev {
            Event::Note(n) => {
                log(&n)?;
                continue;
            }
            Event::Raise(e) => return Err(e),
            Event::Tree(i) => i,
        };
        let (p, _, tp, rows) = &mut chosen[i];
        let label = splitext(&basename(p)).0;
        match rows.take().ok_or(PY)? {
            TreeOut::Rows(f) => out.push(RefLevel { label, rows: Ok(f) }),
            TreeOut::Note(e, false) if reference => out.push(RefLevel { label, rows: Err(e) }),
            TreeOut::Note(e, _) => log(&format!("note: {tp}: {e}; not used"))?,
            TreeOut::Raise(e) => return Err(e),
        }
    }
    Ok(out)
}

/// `collect(pairs)`: the target's rows, then each reference level's.
pub fn collect(tree: &CullTree, level: &Level, refs: &[RefLevel]) -> CullResult<Found> {
    let mut found = Found::default();
    collect_target(tree, level, "level", &mut found)?;
    for r in refs {
        let rows = r.rows.as_ref().map_err(|m| cte(m.clone()))?;
        for (bp, rows) in &rows.items {
            found.entry_or(bp.clone(), Vec::new).extend(rows.iter().cloned());
        }
    }
    Ok(found)
}

// ------------------------------------------------------------------------------------------------ add

/// `insert(tree, sub, sph)`: the subtree placed where the tree's spheres already hold it; the new internal node.
pub fn insert(tree: &mut CullTree, sub: Id, sph: &Sphere) -> CullResult<Id> {
    let a = &mut tree.arena;
    let root = tree.root;
    let mut path = vec![root];
    let mut n = root;
    let (target, parent) = loop {
        if a.nodes[n].leaf.is_some() {
            return Err(cte("internal: descended into a leaf"));
        }
        let (l, r) = a.kids(n)?;
        let cands: Vec<Id> = [l, r].into_iter().filter(|&c| a.sphere(c).contains(sph)).collect();
        if !cands.is_empty() {
            let mut c = cands[0];
            for &x in &cands[1..] {
                if a.sphere(x).r < a.sphere(c).r {
                    c = x;
                }
            }
            if a.nodes[c].leaf.is_none() {
                path.push(c);
                n = c;
                continue;
            }
            break (c, n);
        }
        let grow = |x: Id| enclose(&a.sphere(x), sph).r - a.sphere(x).r;
        let t = if grow(r) < grow(l) { r } else { l };
        break (t, n);
    };
    let s = enclose(&a.sphere(target), sph);
    let keys = encode(&s, &[a.sphere(target), *sph])?;
    let mx = if a.max(sub) > a.max(target) {
        a.max(sub)
    } else {
        a.max(target)
    };
    let data = NodeData {
        right: 0,
        max: mx,
        keys,
    };
    let p = if a.nodes[parent].left == Some(target) {
        let p = a.push(data, Some(target), Some(sub), None);
        a.nodes[parent].left = Some(p);
        p
    } else {
        let p = a.push(data, Some(sub), Some(target), None);
        a.nodes[parent].right = Some(p);
        p
    };
    let smax = a.max(sub);
    for &x in &path {
        if a.nodes[x].data.max < smax {
            a.nodes[x].data.max = smax;
        }
    }
    let rs0 = a.sphere(root);
    if !rs0.contains(sph) {
        let rs = enclose(&rs0, sph);
        a.nodes[root].data.keys = encode(&rs, &[rs0, *sph])?;
    }
    Ok(p)
}

/// `make_subtree(placed, first_leaf)`: (its top node, its sphere).
pub fn make_subtree(a: &mut Arena, placed: &[(Sphere, f64)], first: i64) -> CullResult<(Id, Sphere)> {
    let mut leaves = Vec::new();
    for (j, (s, mx)) in placed.iter().enumerate() {
        let keys = encode(s, &[])?;
        let idx = first + j as i64;
        let data = NodeData {
            right: (LEAF | idx) - (1 << 32),
            max: *mx,
            keys,
        };
        leaves.push(a.push(data, None, None, Some(idx)));
    }
    fn join_nodes(a: &mut Arena, ns: &[Id]) -> CullResult<Id> {
        if ns.len() == 1 {
            return Ok(ns[0]);
        }
        let h = ns.len() / 2;
        let x = join_nodes(a, &ns[..h])?;
        let y = join_nodes(a, &ns[h..])?;
        let (sx, sy) = (a.sphere(x), a.sphere(y));
        let s = enclose(&sx, &sy);
        let mx = if a.max(y) > a.max(x) { a.max(y) } else { a.max(x) };
        let keys = encode(&s, &[sx, sy])?;
        Ok(a.push(
            NodeData {
                right: 0,
                max: mx,
                keys,
            },
            Some(x),
            Some(y),
            None,
        ))
    }
    let top = join_nodes(a, &leaves)?;
    let mut whole = a.sphere(leaves[0]);
    for &x in &leaves[1..] {
        whole = enclose(&whole, &a.sphere(x));
    }
    Ok((
        top,
        if a.nodes[top].leaf.is_none() {
            a.sphere(top)
        } else {
            whole
        },
    ))
}

/// What `add` did for one object: (object, slot, leaves, shape).
pub type Added = (usize, i64, Vec<i64>, Shape);

/// `add(tree, level, objects, found, log)`.
pub fn add(
    tree: &mut CullTree,
    level: &mut Level,
    objects: &[usize],
    found: &Found,
    log: &mut dyn FnMut(&str) -> CullResult<()>,
) -> CullResult<Vec<Added>> {
    let mut done = Vec::new();
    for &o in objects {
        let obj = level.objects[o].clone();
        if obj.enumeration != 0 {
            return Err(cte(format!(
                "object {} already has slot {}; add only takes slot-0 objects",
                obj.guid, obj.enumeration
            )));
        }
        let Some(shape) = shape_in(&obj.blueprint, found) else {
            return Err(cte(format!(
                "object {}: no slotted instance of {} in the tree or the --ref levels to take its shape from; add a \
                 --ref level that has one",
                obj.guid, obj.blueprint
            )));
        };
        let placed = shape.place(&obj);
        let first = tree.leaves() + 1;
        let (sub, sph) = make_subtree(&mut tree.arena, &placed, first)?;
        insert(tree, sub, &sph)?;
        for j in 0..placed.len() as i64 {
            tree.leaf_node.insert(first + j, None);
        }
        for (n, _) in tree.arena.walk(sub) {
            if let Some(l) = tree.arena.nodes[n].leaf {
                tree.leaf_node.insert(l, Some(n));
            }
        }
        let slot = tree.enum_offset + first;
        level.set_enum(o, slot)?;
        let name = obj.blueprint.rsplit('/').next().unwrap_or("");
        let r0 = placed.first().map_or(0.0, |p| p.0.r);
        log(&format!(
            "  {} {name:<32} at {}: slot {slot}, {} leaves (shape from {}: {} instance(s), {} agreeing elsewhere), \
             radius {} m",
            obj.guid,
            fmt3(obj.pos()),
            placed.len(),
            shape.source,
            shape.count,
            shape.confirmed,
            fixed(r0, 2)
        ))?;
        if shape.count < 3 && shape.confirmed == 0 {
            log(&format!(
                "  warning: the shape of {} rests on {} unconfirmed instance(s); add a --ref level with more of them \
                 to check it",
                obj.blueprint, shape.count
            ))?;
        }
        done.push((o, slot, (first..first + placed.len() as i64).collect(), shape));
    }
    Ok(done)
}

/// `_fmt(v)`: `(x.x, y.y, z.z)`.
pub fn fmt3(v: Vec3) -> String {
    format!("({}, {}, {})", fixed(v[0], 1), fixed(v[1], 1), fixed(v[2], 1))
}

/// `auto_objects(level, found)`.
pub fn auto_objects(level: &Level, found: &Found) -> Vec<usize> {
    (0..level.objects.len())
        .filter(|&i| {
            let o = &level.objects[i];
            o.enumeration == 0 && !o.blueprint.is_empty() && found.contains(&o.blueprint)
        })
        .collect()
}

/// `misplaced(tree, level, found, metres)`.
pub fn misplaced(tree: &CullTree, level: &Level, found: &Found, metres: f64) -> Vec<(usize, f64)> {
    let mut out = Vec::new();
    for (o, ids) in leaves_of(tree, level) {
        let obj = &level.objects[o];
        // shape_for looks the blueprint up lower-cased in {blueprint as it is: instances}
        if obj.blueprint.to_lowercase() != obj.blueprint {
            continue;
        }
        let rest: Vec<&Instance> = found
            .get(&obj.blueprint)
            .map_or_else(Vec::new, |v| v.iter().filter(|x| x.obj != Some(o)).collect());
        let Some(sh) = shape_for(&obj.blueprint, &rest) else {
            continue;
        };
        let k = sh.leaves.len().min(ids.len());
        let off: Vec<f64> = (0..k)
            .filter_map(|j| {
                tree.leaf(ids[j])
                    .map(|n| dist(obj.to_world(sh.leaves[j].0), tree.arena.sphere(n).c))
            })
            .collect();
        if !off.is_empty() {
            let m = py_max(&off);
            if m > metres {
                out.push((o, m));
            }
        }
    }
    out
}

// ------------------------------------------------------------------------------------------------ checks

/// `check_structure(tree, client)`.
pub fn check_structure(tree: &mut CullTree, client: Option<&CullTree>) -> Vec<(bool, String)> {
    let mut res = Vec::new();
    let flat = tree.arena.flatten(tree.root);
    res.push((
        flat == tree.nodes[..tree.used],
        format!(
            "preorder layout and Right offsets: {} used nodes, every Right = 1 + left subtree size",
            tree.used
        ),
    ));
    res.push((
        tree.spare == SPARE,
        format!(
            "{} all-zero spare nodes after the used ones (stock trees: {SPARE})",
            tree.spare
        ),
    ));
    res.push((
        tree.leaf_range_ok(),
        format!("leaf indices 1..{}, each once", tree.leaves()),
    ));
    res.push((
        tree.used as i64 == 2 * tree.leaves() - 1,
        format!(
            "{} used nodes = 2 x {} leaves - 1 (binary tree)",
            tree.used,
            tree.leaves()
        ),
    ));
    let a = &tree.arena;
    let bad_max = a
        .walk(tree.root)
        .iter()
        .filter(|(n, _)| {
            let x = &a.nodes[*n];
            match (x.leaf, x.left, x.right) {
                (None, Some(l), Some(r)) => {
                    let (lm, rm) = (a.max(l), a.max(r));
                    x.data.max != if rm > lm { rm } else { lm }
                }
                _ => false,
            }
        })
        .count();
    res.push((
        bad_max == 0,
        format!("every internal node's Max is the larger of its children's ({bad_max} not)"),
    ));
    let (depth, most) = tree.depth_and_stack();
    res.push((
        most <= STACK_SLOTS,
        format!("deepest leaf {depth} levels; traversal stack up to {most} of {STACK_SLOTS} entries"),
    ));
    if let Some(c) = client {
        res.push((
            tree.enum_offset == c.leaves() && c.enum_offset == 0,
            format!(
                "EnumOffset {} = client tree leaves {} (client EnumOffset {})",
                tree.enum_offset,
                c.leaves(),
                c.enum_offset
            ),
        ));
    }
    res
}

/// `containment(tree, leaf_ids)`: (leaf, ancestors not containing it, path length), preorder.
pub fn containment(tree: &CullTree, ids: &[i64]) -> Vec<(i64, usize, usize)> {
    let a = &tree.arena;
    let mut out = Vec::new();
    for (n, anc) in a.walk(tree.root) {
        if let Some(l) = a.nodes[n].leaf
            && ids.contains(&l)
        {
            let s = a.sphere(n);
            let miss = anc.iter().skip(1).filter(|x| !a.sphere(**x).contains(&s)).count();
            out.push((l, miss, anc.len()));
        }
    }
    out
}

/// `object_checks(tree, level, found)`: ([(ok, text)], details).
#[allow(
    clippy::too_many_lines,
    reason = "the reference's checks in its order, kept in one piece"
)]
pub fn object_checks(tree: &CullTree, level: &Level, found: &Found) -> (Vec<(bool, String)>, Vec<String>) {
    let mut res = Vec::new();
    let mut detail = Vec::new();
    let eo = tree.enum_offset;
    let last = eo + tree.leaves();
    let mut cnt: Vec<(i64, usize)> = Vec::new();
    for o in &level.objects {
        if o.enumeration != 0 {
            match cnt.iter_mut().find(|c| c.0 == o.enumeration) {
                Some(c) => c.1 += 1,
                None => cnt.push((o.enumeration, 1)),
            }
        }
    }
    let shared = cnt.iter().filter(|c| c.1 > 1).count();
    res.push((shared == 0, format!("no slot shared by two objects ({shared} shared)")));
    let out_of = level
        .objects
        .iter()
        .filter(|o| o.enumeration != 0 && !(eo < o.enumeration && o.enumeration <= last))
        .count();
    res.push((
        out_of == 0,
        format!(
            "every slot is a leaf of this tree: {}..{last} ({out_of} outside)",
            eo + 1
        ),
    ));
    let lv = leaves_of(tree, level);
    let mut ks: Vec<(usize, usize)> = Vec::new();
    for (_, v) in &lv {
        match ks.iter_mut().find(|c| c.0 == v.len()) {
            Some(c) => c.1 += 1,
            None => ks.push((v.len(), 1)),
        }
    }
    ks.sort_unstable();
    res.push((
        true,
        format!(
            "leaves per object: {}",
            ks.iter()
                .map(|(k, c)| format!("{k}: {c}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    ));
    let used: usize = lv.iter().map(|(_, v)| v.len()).sum();
    res.push((
        true,
        format!("{used} of {} leaves start at or follow an object's slot", tree.leaves()),
    ));
    let mut errs: Vec<f64> = Vec::new();
    let mut worst: Vec<(f64, usize, i64)> = Vec::new();
    let (mut rad_short, mut n_cmp) = (0, 0);
    for (o, ids) in &lv {
        let obj = &level.objects[*o];
        let inst: Vec<&Instance> = found
            .get(&obj.blueprint)
            .map_or_else(Vec::new, |v| v.iter().filter(|x| x.obj != Some(*o)).collect());
        if inst.len() < 2 || obj.blueprint.to_lowercase() != obj.blueprint {
            continue;
        }
        let Some(sh) = shape_for(&obj.blueprint, &inst) else {
            continue;
        };
        if sh.leaves.len() != ids.len() {
            continue;
        }
        for (j, li) in ids.iter().enumerate() {
            let Some(n) = tree.leaf(*li) else { continue };
            let d = tree.arena.sphere(n);
            let pc = obj.to_world(sh.leaves[j].0);
            let e = dist(pc, d.c);
            errs.push(e);
            n_cmp += 1;
            if d.r + 1e-6 < sh.leaves[j].2 * obj.scale() {
                rad_short += 1;
            }
            worst.push((e, *o, *li));
        }
    }
    if !errs.is_empty() {
        errs.sort_by(f64::total_cmp);
        let p99 = errs[errs.len() * 99 / 100];
        res.push((
            p99 < 1.0,
            format!(
                "decoded leaf centres vs the blueprint's other instances (leave-one-out): {n_cmp} leaves, median {} m, \
                 99% within {} m, largest {} m",
                fixed(errs[errs.len() / 2], 3),
                fixed(p99, 3),
                fixed(errs[errs.len() - 1], 2)
            ),
        ));
        res.push((
            rad_short == 0,
            format!(
                "decoded radius at least the leaf's Max (unquantized radius): {} of {n_cmp}",
                n_cmp - rad_short
            ),
        ));
        worst.sort_by(|a, b| (-a.0).total_cmp(&-b.0));
        for (e, o, li) in worst.iter().take(5) {
            if *e > 1.0 {
                let obj = &level.objects[*o];
                detail.push(format!(
                    "  leaf {li} of {} ({}) is {} m from where the blueprint's other instances put it",
                    obj.guid,
                    obj.blueprint.rsplit('/').next().unwrap_or(""),
                    fixed(*e, 2)
                ));
            }
        }
    }
    let inside = lv
        .iter()
        .filter(|(o, ids)| {
            tree.leaf(ids[0]).is_some_and(|f| {
                tree.arena.sphere(f).contains(&Sphere {
                    c: level.objects[*o].pos(),
                    r: 0.0,
                })
            })
        })
        .count();
    res.push((
        true,
        format!(
            "object origin inside its first leaf's sphere: {inside} of {} (models built away from their origin, e.g. \
             distant scenery, are not)",
            lv.len()
        ),
    ));
    let zero = level.objects.iter().filter(|o| o.enumeration == 0).count();
    let cand = auto_objects(level, found);
    res.push((
        cand.is_empty(),
        format!(
            "{zero} objects with slot 0; {} of them have a blueprint that is in a cull tree (see-through / \
             indestructible in game: candidates for add)",
            cand.len()
        ),
    ));
    for o in cand {
        let obj = &level.objects[o];
        detail.push(format!(
            "  slot 0: {} {} at {}",
            obj.guid,
            obj.blueprint,
            fmt3(obj.pos())
        ));
    }
    (res, detail)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_formats() {
        let s = Sphere {
            c: [1.03125, -0.03125, 10.0],
            r: 0.5,
        };
        // Python's round: half to even (16.5 -> 16, -0.5 -> 0)
        assert_eq!(encode(&s, &[]).unwrap()[..3], [16, 0, 160]);
        let far = Sphere {
            c: [0.0, 0.0, 2047.96875], // 32767.5 -> 32768: out of range
            r: 0.5,
        };
        assert!(encode(&far, &[]).is_err());
        assert!(
            encode(
                &Sphere {
                    c: [3000.0, 0.0, 0.0],
                    r: 1.0
                },
                &[]
            )
            .is_err()
        );
        assert_eq!(fixed(0.125, 2), "0.12");
        assert_eq!(fixed(0.25, 1), "0.2");
        assert_eq!(fixed(-0.04, 1), "-0.0");
        assert_eq!(fixed(f64::NAN, 1), "nan");
        assert_eq!(py_max(&[1.0, 3.0, 3.0, 2.0]), 3.0);
        let d = decode(&NodeData {
            right: 0,
            max: 1.0,
            keys: [65536 - 3499, 9432, 65536 - 11703, 10661],
        });
        assert!((d.c[0] + 218.6875).abs() < 1e-9 && (d.r - 2665.25).abs() < 1e-9);
    }
}
