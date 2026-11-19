//! `bfbc2-culltree/culltree.py`'s model in Rust (round O3, `rust/BENCHMARKS.md` "O3"): its byte-keeping DBX codec
//! (`Doc`), the static cull tree (`CullTree`: preorder nodes, sphere keys, leaves), a level's placed objects
//! (`Level`), the blueprints' shapes from their slotted instances (`collect`, `shape_for`), the reference levels
//! (`ref_rows` / `ref_pairs`, `level_partitions`), `add` (`insert`, `make_subtree`, `auto_objects`, `misplaced`) and
//! the checks (`check_structure`, `containment`, `object_checks`) - every value computed with the reference's
//! floating-point operations in its order, so the written trees and levels and the printed numbers are its own.
//! `bfbc2.exe culltree` (bc2-cli's `cull` group) is the command line on it; the map editor's export keeps its own
//! [`crate::culltree`] (an object's shape from its own leaves).
//!
//! Errors are the Python's exceptions ([`bc2_pyrt::err::Fail`]): `CullTreeError` with its text, the `OSError` of
//! `open()`, and [`Fail::Python`] where the reference raises an exception of its own on a damaged file (an index or
//! `struct` error): the command line then runs the reference.
//!
//! The reference levels' rows are not cached here (`culltree.py` keeps them in pickles under
//! `%LOCALAPPDATA%\bfbc2-tool-suite\cache\culltree`): the levels are read again on all cores, which costs less than
//! the cache saved the Python. The rows from the cache equal those read, so the answers do not depend on it.

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

use bc2_formats::culltree as raw;
use bc2_pyrt::err::{Fail, PyErr, PyResult};
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

/// `CullTreeError(msg)`.
pub fn cte(msg: impl Into<String>) -> Fail {
    Fail::Err(PyErr::new("CullTreeError", msg))
}

const PY: Fail = Fail::Python;

/// `open(path, 'rb').read()`.
fn read(path: &str) -> PyResult<Vec<u8>> {
    std::fs::read(path).map_err(|e| bc2_pyrt::err::os_error(path, &e))
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

fn varint(data: &[u8], mut pos: usize) -> PyResult<(u64, usize)> {
    let mut v: u64 = 0;
    let mut shift = 0u32;
    loop {
        let b = *data.get(pos).ok_or(PY)?;
        pos += 1;
        let low = u64::from(b & 0x7f);
        if shift > 63 || (shift > 0 && low >> (64 - shift) != 0) {
            return Err(PY); // a big integer only Python reads
        }
        v |= low << shift;
        if b < 0x80 {
            return Ok((v, pos));
        }
        shift += 7;
    }
}

fn put_varint(mut v: u64, out: &mut Vec<u8>) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

/// `El`: an element as the reference keeps it.
#[derive(Debug, Clone)]
pub struct El {
    pub tag: u64,
    pub head: u8,
    pub attrs: Vec<(u64, u64)>,
    pub extra: u8,
    pub value: Vec<u8>,
    pub children: Option<Vec<El>>,
}

impl El {
    pub fn kind(&self) -> u8 {
        self.head >> 4
    }

    /// `attr(name)` against the document's strings (an index past them: the reference's `IndexError`).
    pub fn attr<'a>(&self, s: &'a [String], name: &str) -> PyResult<Option<&'a str>> {
        for &(k, v) in &self.attrs {
            let key = s.get(usize::try_from(k).map_err(|_| PY)?).ok_or(PY)?;
            if key == name {
                return Ok(Some(s.get(usize::try_from(v).map_err(|_| PY)?).ok_or(PY)?));
            }
        }
        Ok(None)
    }

    pub fn name<'a>(&self, s: &'a [String]) -> PyResult<Option<&'a str>> {
        self.attr(s, "name")
    }

    /// `child(name)`: the first child of that name, its index.
    pub fn child(&self, s: &[String], name: &str) -> PyResult<Option<usize>> {
        for (i, c) in self.children.iter().flatten().enumerate() {
            if c.name(s)? == Some(name) {
                return Ok(Some(i));
            }
        }
        Ok(None)
    }

    /// `numbers()`: (size, values), each value sliced as Python slices (short at the end).
    pub fn numbers(&self) -> PyResult<(u64, Vec<Vec<u8>>)> {
        let (count, p) = varint(&self.value, 0)?;
        let (size, p) = varint(&self.value, p)?;
        if count > 1 << 24 || size > 1 << 32 {
            return Err(PY); // a damaged count only the reference walks (or runs out of memory on)
        }
        let n = self.value.len();
        let mut out = Vec::new();
        for i in 0..count {
            let a = (p as u64 + i * size).min(n as u64) as usize;
            let b = (p as u64 + (i + 1) * size).min(n as u64) as usize;
            out.push(self.value[a..b.max(a)].to_vec());
        }
        Ok((size, out))
    }

    /// `set_numbers(size, values)`.
    pub fn set_numbers(&mut self, size: u64, values: &[Vec<u8>]) -> PyResult<()> {
        let mut out = Vec::new();
        put_varint(values.len() as u64, &mut out);
        put_varint(size, &mut out);
        for v in values {
            if v.len() as u64 != size {
                return Err(cte("internal: number of the wrong size"));
            }
            out.extend_from_slice(v);
        }
        self.value = out;
        Ok(())
    }
}

/// `Doc`: a binary DBX file kept byte for byte.
#[derive(Debug, Clone)]
pub struct Doc {
    pub name: String,
    head12: Vec<u8>,
    string_area: Vec<u8>,
    pub strings: Vec<String>,
    pub roots: Vec<El>,
    trailer: Vec<u8>,
}

fn be32(d: &[u8], at: usize) -> PyResult<u32> {
    let b = d.get(at..at + 4).ok_or(PY)?;
    Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

impl Doc {
    pub fn parse(data: &[u8], name: &str) -> PyResult<Self> {
        if data.len() <= 24 || !data.starts_with(MAGIC) {
            return Err(cte(format!("{name} is not a binary .dbx file")));
        }
        let head12 = data[12..16].to_vec();
        let size = be32(data, 16)? as usize;
        let count = be32(data, 20)? as usize;
        let string_area = data[24.min(data.len())..(24 + size).min(data.len())].to_vec();
        let base = 24 + 4 * count;
        let mut strings = Vec::new();
        for i in 0..count {
            let off = be32(data, 24 + 4 * i)? as usize;
            let start = base + off;
            if start > data.len() {
                return Err(PY);
            }
            let end = data[start..].iter().position(|&b| b == 0).ok_or(PY)? + start;
            strings.push(String::from_utf8_lossy(&data[start..end]).into_owned());
        }
        let pos = 24 + size;
        let read = raw::elements(data, pos).map_err(|e| cte(format!("{name}: {e}")))?;
        // the flat stream as a tree (a container's children: the elements up to its end)
        fn take(read: &raw::Elements, data: &[u8], i: &mut usize, end: usize, out: &mut Vec<El>) {
            while *i < end {
                let e = &read.elements[*i];
                *i += 1;
                let attrs = read.attrs.get(e.attrs.clone()).unwrap_or_default().to_vec();
                let mut el = El {
                    tag: e.tag,
                    head: e.head,
                    attrs,
                    extra: e.extra,
                    value: Vec::new(),
                    children: None,
                };
                if let Some(stop) = e.end {
                    let mut kids = Vec::new();
                    take(read, data, i, stop, &mut kids);
                    el.children = Some(kids);
                } else {
                    el.value = data.get(e.value.clone()).unwrap_or_default().to_vec();
                }
                out.push(el);
            }
        }
        let mut roots = Vec::new();
        let mut i = 0;
        take(&read, data, &mut i, read.elements.len(), &mut roots);
        Ok(Self {
            name: name.to_owned(),
            head12,
            string_area,
            strings,
            roots,
            trailer: data[read.end..].to_vec(),
        })
    }

    pub fn read(path: &str) -> PyResult<Self> {
        Self::parse(&read(path)?, &basename(path))
    }

    pub fn tobytes(&self) -> Vec<u8> {
        fn put(e: &El, out: &mut Vec<u8>) {
            put_varint(e.tag, out);
            out.push(e.head);
            for &(k, v) in &e.attrs {
                put_varint(k, out);
                put_varint(v, out);
            }
            if e.kind() == CONTAINER {
                out.push(e.extra);
                for c in e.children.iter().flatten() {
                    put(c, out);
                }
                out.push(0);
            } else {
                out.extend_from_slice(&e.value);
            }
        }
        let mut payload = Vec::new();
        for r in &self.roots {
            put(r, &mut payload);
        }
        payload.extend_from_slice(&self.trailer);
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&(24 + self.string_area.len() as u32).to_be_bytes());
        out.extend_from_slice(&self.head12);
        out.extend_from_slice(&(self.string_area.len() as u32).to_be_bytes());
        out.extend_from_slice(&(self.strings.len() as u32).to_be_bytes());
        out.extend_from_slice(&self.string_area);
        out.extend_from_slice(&payload);
        out
    }

    /// `write(path)`: its folder made, the file written.
    pub fn write(&self, path: &str) -> PyResult<()> {
        let dir = dirname(&abspath(path));
        std::fs::create_dir_all(&dir).map_err(|e| mkdir_error(&dir, &e))?;
        std::fs::write(path, self.tobytes()).map_err(|e| bc2_pyrt::err::os_error(path, &e))
    }

    /// `instances`: the root's children tagged `instance` (their indices).
    pub fn instances(&self) -> PyResult<Vec<usize>> {
        let root = self.roots.first().ok_or(PY)?;
        let mut out = Vec::new();
        for (i, e) in root.children.iter().flatten().enumerate() {
            let tag = self.strings.get(usize::try_from(e.tag).map_err(|_| PY)?).ok_or(PY)?;
            if tag == "instance" {
                out.push(i);
            }
        }
        Ok(out)
    }

    pub fn instance(&self, i: usize) -> &El {
        match self.roots[0].children.as_ref() {
            Some(c) => &c[i],
            None => unreachable_el(),
        }
    }

    fn instance_mut(&mut self, i: usize) -> &mut El {
        let c = self.roots[0].children.as_mut();
        match c {
            Some(c) => &mut c[i],
            None => unreachable_mut(),
        }
    }
}

#[allow(clippy::panic, reason = "instances() only gives indices of a container's children")]
fn unreachable_el() -> &'static El {
    panic!("an instance index of a root without children")
}

#[allow(clippy::panic, reason = "instances() only gives indices of a container's children")]
fn unreachable_mut() -> &'static mut El {
    panic!("an instance index of a root without children")
}

/// `os.makedirs`'s error, as the reference reports it (else the reference's own).
pub fn mkdir_error(dir: &str, e: &io::Error) -> Fail {
    if e.kind() == io::ErrorKind::AlreadyExists {
        return Fail::Err(PyErr {
            kind: "FileExistsError".to_owned(),
            msg: format!(
                "[WinError 183] Cannot create a file when that file already exists: {}",
                bc2_pyrt::fmt::str_repr(dir)
            ),
            filename: Some(dir.to_owned()),
            what_to_do: None,
        });
    }
    bc2_pyrt::err::os_error(dir, e)
}

fn int4(e: &El) -> PyResult<i64> {
    let (size, v) = e.numbers()?;
    if size != 4 || v.len() != 1 {
        return Err(cte("internal: not a single 4-byte number"));
    }
    let b: [u8; 4] = v[0].as_slice().try_into().map_err(|_| PY)?;
    Ok(i64::from(i32::from_be_bytes(b)))
}

fn floats(e: &El) -> PyResult<Vec<f64>> {
    let (size, v) = e.numbers()?;
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
fn py_round(x: f64) -> PyResult<i64> {
    if !x.is_finite() || x.abs() > 9e15 {
        return Err(PY);
    }
    Ok(x.round_ties_even() as i64)
}

/// `encode(sphere, contain)`: keys whose decoded sphere holds the sphere and every one in `contain`.
pub fn encode(s: &Sphere, contain: &[Sphere]) -> PyResult<[i64; 4]> {
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

    fn kids(&self, n: Id) -> PyResult<(Id, Id)> {
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
pub fn build(arena: &mut Arena, nodes: &[NodeData]) -> PyResult<(Id, usize)> {
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
    pub doc: Doc,
    inst: usize,
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

fn node_of(s: &[String], c: &El) -> PyResult<NodeData> {
    let mut v: HashMap<Option<String>, Vec<u8>> = HashMap::new();
    for f in c.children.iter().flatten() {
        let (_size, vals) = f.numbers()?;
        let first = vals.into_iter().next().ok_or(PY)?;
        v.insert(f.name(s)?.map(str::to_owned), first);
    }
    let get = |k: &str| -> PyResult<Vec<u8>> {
        v.get(&Some(k.to_owned()))
            .cloned()
            .ok_or_else(|| cte(format!("a cull tree node has no field '{k}'")))
    };
    let four = |b: Vec<u8>| -> PyResult<[u8; 4]> { b.as_slice().try_into().map_err(|_| PY) };
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

impl CullTree {
    pub fn read(path: &str) -> PyResult<Self> {
        let doc = Doc::read(path)?;
        let base = basename(path);
        let s = &doc.strings;
        let mut found = Vec::new();
        for i in doc.instances()? {
            if doc.instance(i).attr(s, "type")? == Some("World.StaticCullTreeData") {
                found.push(i);
            }
        }
        let Some(&inst) = found.first() else {
            return Err(cte(format!(
                "{base} has no World.StaticCullTreeData instance (not a cull tree)"
            )));
        };
        let e = doc.instance(inst);
        let array = e.child(s, "Nodes")?;
        let eo = e.child(s, "EnumOffset")?;
        let (Some(array), Some(eo)) = (array, eo) else {
            return Err(cte(format!("{base}: the cull tree has no Nodes or EnumOffset field")));
        };
        let kids = e.children.as_ref().ok_or(PY)?;
        let enum_offset = int4(&kids[eo])?;
        let mut nodes = Vec::new();
        for c in kids[array].children.iter().flatten() {
            nodes.push(node_of(s, c)?);
        }
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
    pub fn write(&mut self, path: &str) -> PyResult<()> {
        let mut flat = self.arena.flatten(self.root);
        flat.extend(std::iter::repeat_n(NodeData::ZERO, SPARE));
        let s = self.doc.strings.clone();
        let array = &self.doc.instance(self.inst).children.as_ref().ok_or(PY)?[self.array];
        let tmpl = array.children.as_ref().and_then(|c| c.first()).ok_or(PY)?.clone();
        let mut items = Vec::with_capacity(flat.len());
        for d in &flat {
            let mut c = tmpl.clone();
            for f in c.children.iter_mut().flatten() {
                let nm = f.name(&s)?;
                let (size, _) = f.numbers()?;
                match nm {
                    Some("Right") => {
                        let v = i32::try_from(d.right).map_err(|_| PY)?;
                        f.set_numbers(4, &[v.to_be_bytes().to_vec()])?;
                    }
                    Some("Max") => {
                        let x = d.max as f32;
                        if x.is_infinite() && d.max.is_finite() {
                            return Err(PY); // struct's OverflowError
                        }
                        f.set_numbers(4, &[x.to_be_bytes().to_vec()])?;
                    }
                    Some(n) if NODE_FIELDS.contains(&n) => {
                        let axis = "XYZW".find(n.chars().last().unwrap_or('X')).unwrap_or(0);
                        let k = u16::try_from(d.keys[axis]).map_err(|_| PY)?;
                        let mut v = k.to_be_bytes().to_vec();
                        v.extend(std::iter::repeat_n(0u8, (size as usize).saturating_sub(2)));
                        f.set_numbers(size, &[v])?;
                    }
                    _ => {}
                }
            }
            items.push(c);
        }
        let (inst, arr) = (self.inst, self.array);
        let mut doc = self.doc.clone();
        let node = &mut doc.instance_mut(inst).children.as_mut().ok_or(PY)?[arr];
        node.children = Some(items);
        doc.write(path)
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

    pub fn to_local(&self, w: Vec3) -> PyResult<Vec3> {
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

fn inv3(r: [[f64; 3]; 3]) -> PyResult<[[f64; 3]; 3]> {
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
    pub doc: Doc,
    pub objects: Vec<Placed>,
    pub tree_refs: HashMap<String, String>,
}

impl Level {
    pub fn read(path: &str) -> PyResult<Self> {
        let doc = Doc::read(path)?;
        let s = &doc.strings;
        let mut objects = Vec::new();
        let mut tree_refs = HashMap::new();
        for i in doc.instances()? {
            let e = doc.instance(i);
            for f in ["ExclusiveStaticCullTree", "ClientStaticCullTree"] {
                if let Some(c) = e.child(s, f)? {
                    let kid = &e.children.as_ref().ok_or(PY)?[c];
                    if let Some(r) = kid.attr(s, "ref")?
                        && !r.is_empty()
                    {
                        tree_refs.insert(f.to_owned(), r.to_owned());
                    }
                }
            }
            if e.attr(s, "type")? != Some("Entity.ReferenceObjectData") {
                continue;
            }
            let (en, tr, ro) = (
                e.child(s, "Enumeration")?,
                e.child(s, "Transform")?,
                e.child(s, "ReferencedObject")?,
            );
            let (Some(en), Some(tr)) = (en, tr) else { continue };
            let kids = e.children.as_ref().ok_or(PY)?;
            let guid = e.attr(s, "guid")?.unwrap_or("").to_owned();
            let rf = match ro {
                Some(ro) => kids[ro].attr(s, "ref")?,
                None => None,
            };
            let blueprint = match rf {
                Some(r) if r.contains('/') => r.rsplit_once('/').map_or("", |x| x.0).to_lowercase(),
                Some(r) => r.to_owned(),
                None => String::new(),
            };
            let v = floats(&kids[tr])?;
            if v.len() != 16 {
                continue;
            }
            let enumeration = int4(&kids[en])?;
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
        }
        if objects.is_empty() {
            return Err(cte(format!(
                "{} has no placed objects (Entity.ReferenceObjectData): not a level partition",
                basename(path)
            )));
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
        let g = bc2_pyrt::py::strip(guid).to_lowercase();
        self.objects.iter().position(|o| o.guid.to_lowercase() == g)
    }

    pub fn tree_path(&self) -> Option<String> {
        tree_path_of(
            &self.path,
            self.tree_refs.get("ExclusiveStaticCullTree").map(String::as_str),
        )
    }

    /// `set_enum(o, value)`.
    pub fn set_enum(&mut self, o: usize, value: i64) -> PyResult<()> {
        let v = i32::try_from(value).map_err(|_| PY)?;
        let (i, c) = self.objects[o].el;
        let el = &mut self.doc.instance_mut(i).children.as_mut().ok_or(PY)?[c];
        el.set_numbers(4, &[v.to_be_bytes().to_vec()])?;
        self.objects[o].enumeration = value;
        Ok(())
    }
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
    let eo = tree.enum_offset;
    let last = eo + tree.leaves();
    let mut slotted: Vec<usize> = (0..level.objects.len())
        .filter(|&i| eo < level.objects[i].enumeration && level.objects[i].enumeration <= last)
        .collect();
    slotted.sort_by_key(|&i| level.objects[i].enumeration);
    let mut out = Vec::new();
    for (k, &o) in slotted.iter().enumerate() {
        let en = level.objects[o].enumeration;
        let nxt = slotted.get(k + 1).map_or(last + 1, |&n| level.objects[n].enumeration);
        out.push((o, ((en - eo)..(en - eo + 1).max(nxt - eo)).collect()));
    }
    out
}

// ------------------------------------------------------------------------------------------------ shapes

/// A blueprint's leaf in its own space: (local centre, radius key, Max).
pub type ShapeLeaf = (Vec3, f64, f64);

/// `Instance`: (label, leaf count, leaves, the target level's object or None).
#[derive(Debug, Clone)]
pub struct Instance {
    pub label: String,
    pub k: usize,
    pub leaves: Vec<ShapeLeaf>,
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
pub fn collect_target(tree: &CullTree, level: &Level, label: &str, found: &mut Found) -> PyResult<()> {
    for (o, lv) in leaves_of(tree, level) {
        let obj = &level.objects[o];
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
            label: label.to_owned(),
            k: lv.len(),
            leaves: sph,
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
        if !labels.contains(&x.label.as_str()) {
            labels.push(&x.label);
        }
    }
    let mut groups: Vec<(&str, Vec<&Instance>, Vec<ShapeLeaf>)> = Vec::new();
    for label in labels {
        let group: Vec<&Instance> = inst.iter().copied().filter(|x| x.label == label).collect();
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
pub fn level_partitions(folder: &str) -> PyResult<Vec<String>> {
    let rels = raw::dbx_files(std::path::Path::new(folder));
    let mut files = Vec::with_capacity(rels.len());
    for r in rels {
        let r = r.to_str().ok_or(PY)?.to_owned(); // a name glob lists that is not Unicode
        files.push(join(folder, &[&r]));
    }
    files.sort();
    let cands: Vec<String> = files
        .into_iter()
        .filter(|p| {
            let b = basename(p).to_lowercase();
            !b.contains("staticculltree") && !b.contains("_description")
        })
        .collect();
    let hits = raw::contains(&cands, LEVEL_MARK);
    let mut out = Vec::new();
    for (p, hit) in cands.into_iter().zip(hits) {
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
    pub rows: PyResult<Found>,
}

fn same(a: &str) -> String {
    abspath(a).replace('/', "\\").to_lowercase()
}

/// What one reference level file gives: not a level, or the parsed level.
enum Read1 {
    NotLevel,
    Level(Box<Level>),
}

/// The ordered pass's events: a note, a chosen level (its index in `levels`), the exception raised there.
enum Event {
    Note(String),
    Tree(usize),
    Raise(Fail),
}

/// `ref_rows(refs, ref_dirs, log, target)` (with `reference`: `ref_pairs`, as `BFBC2_CULLTREE_REFERENCE=1` runs):
/// the reference levels in order, their notes logged. The levels and then their trees are read on all cores; the
/// notes, the choices and any exception come in the reference's order.
pub fn ref_rows(
    refs: &[String],
    ref_dirs: &[String],
    log: &mut dyn FnMut(&str) -> PyResult<()>,
    target: Option<(&str, &str)>,
    reference: bool,
) -> PyResult<Vec<RefLevel>> {
    let mut paths: Vec<String> = refs.to_vec();
    for d in ref_dirs {
        paths.extend(level_partitions(d)?);
    }
    let skip: Vec<String> = target.map_or_else(Vec::new, |(a, b)| vec![same(a), same(b)]);
    let todo: Vec<&String> = paths.iter().filter(|p| !skip.contains(&same(p))).collect();
    let parsed: Vec<PyResult<Read1>> = todo
        .par_iter()
        .map(|p| match Level::read(p) {
            Ok(l) => Ok(Read1::Level(Box::new(l))),
            Err(Fail::Err(e)) if e.kind == "CullTreeError" => Ok(Read1::NotLevel),
            Err(e) => Err(e),
        })
        .collect();
    let mut levels: Vec<(String, Level, String)> = Vec::new();
    let mut events: Vec<Event> = Vec::new();
    let mut seen: Vec<(String, u64)> = Vec::new();
    for (p, r) in todo.iter().zip(parsed) {
        let lv = match r {
            Err(e) => {
                events.push(Event::Raise(e));
                break;
            }
            Ok(Read1::NotLevel) => continue,
            Ok(Read1::Level(l)) => *l,
        };
        let Some(tp) = lv.tree_path() else {
            if refs.contains(p) {
                events.push(Event::Note(format!(
                    "note: {p}: its exclusive cull tree was not found beside it; not used"
                )));
            }
            continue;
        };
        if skip.contains(&same(&tp)) {
            continue;
        }
        let size = match std::fs::metadata(&tp) {
            Ok(m) => m.len(),
            Err(e) => {
                events.push(Event::Raise(bc2_pyrt::err::os_error(&tp, &e)));
                break;
            }
        };
        let key = (basename(p).to_lowercase(), size);
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        events.push(Event::Tree(levels.len()));
        levels.push(((*p).clone(), lv, tp));
    }
    let mut trees: Vec<Option<PyResult<CullTree>>> =
        levels.par_iter().map(|(_, _, tp)| Some(CullTree::read(tp))).collect();
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
        let (p, lv, tp) = &levels[i];
        let label = splitext(&basename(p)).0;
        let tree = match trees[i].take().ok_or(PY)? {
            Ok(t) => t,
            Err(Fail::Err(e)) if e.kind == "CullTreeError" => {
                log(&format!("note: {tp}: {}; not used", e.msg))?;
                continue;
            }
            Err(e) => return Err(e),
        };
        let mut found = Found::default();
        let rows = collect_target(&tree, lv, &label, &mut found).map(|()| {
            for v in &mut found.items {
                for x in &mut v.1 {
                    x.obj = None;
                }
            }
            found
        });
        match rows {
            Err(Fail::Err(e)) if e.kind == "CullTreeError" && !reference => {
                log(&format!("note: {tp}: {}; not used", e.msg))?;
            }
            Err(Fail::Err(e)) if e.kind == "CullTreeError" => out.push(RefLevel {
                label,
                rows: Err(Fail::Err(e)),
            }),
            Err(e) => return Err(e),
            Ok(f) => out.push(RefLevel { label, rows: Ok(f) }),
        }
    }
    Ok(out)
}

/// `collect(pairs)`: the target's rows, then each reference level's.
pub fn collect(tree: &CullTree, level: &Level, refs: &[RefLevel]) -> PyResult<Found> {
    let mut found = Found::default();
    collect_target(tree, level, "level", &mut found)?;
    for r in refs {
        let rows = r.rows.as_ref().map_err(Clone::clone)?;
        for (bp, rows) in &rows.items {
            found.entry_or(bp.clone(), Vec::new).extend(rows.iter().cloned());
        }
    }
    Ok(found)
}

// ------------------------------------------------------------------------------------------------ add

/// `insert(tree, sub, sph)`: the subtree placed where the tree's spheres already hold it; the new internal node.
pub fn insert(tree: &mut CullTree, sub: Id, sph: &Sphere) -> PyResult<Id> {
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
pub fn make_subtree(a: &mut Arena, placed: &[(Sphere, f64)], first: i64) -> PyResult<(Id, Sphere)> {
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
    fn join_nodes(a: &mut Arena, ns: &[Id]) -> PyResult<Id> {
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
    log: &mut dyn FnMut(&str) -> PyResult<()>,
) -> PyResult<Vec<Added>> {
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
