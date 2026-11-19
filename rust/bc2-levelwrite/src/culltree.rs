//! A level's static cull trees on an export (`EDITOR_ARCHITECTURE.md` 3.5 step 4, 3.12): a placed object the edit
//! moved, or a copy that shares its original's slot, gets new leaves where it now stands - `bfbc2-culltree`'s `add`
//! (`culltree.py`: `build`, `flatten`, `encode`, `enclose`, `insert`, `make_subtree`) ported, with the object's
//! shape taken from its own leaves in the base level (moved and turned with it) rather than from other levels.
//!
//! Layout (`bfbc2-culltree/README.md`): `World.StaticCullTreeData` with `Nodes` (preorder; an internal node's left
//! child next, its right child at `i + Right`; a leaf's `Right` is `0x80000000 | leaf index`, leaves 1..L) and
//! `EnumOffset`; then 256 zero spare nodes. The exclusive tree holds the placed objects: an object's `Enumeration`
//! is `EnumOffset + its first leaf`, and it owns consecutive leaves from there. A deleted object's leaves stay in the
//! tree with nothing registered on them (the traversal skips a leaf without an object), as do a moved object's old
//! leaves.
#![allow(
    clippy::indexing_slicing,
    reason = "fixed-size vectors and rows ([f64; 3], [[f32; 3]; 4], [u16; 4]) indexed by constants or 0..3"
)]
#![allow(clippy::cast_precision_loss, reason = "sphere keys are 16-bit integers")]

use std::collections::HashMap;

use bc2_formats::dbx::NUMBERS;
use bc2_formats::dbx::edit::{EditDbx, NodeId, numbers_of, varint_bytes};

use crate::ops::Rows;
use crate::write::{LevelWriteError, LevelWriter, Result};

/// Centre keys are metres x 16, radius keys metres x 4 (`culltree.py` `CENTER_SCALE`, `RADIUS_SCALE`).
const CENTER_SCALE: f64 = 16.0;
const RADIUS_SCALE: f64 = 4.0;
/// Zero nodes after the used ones.
const SPARE: usize = 256;
const LEAF: u32 = 0x8000_0000;
const KEYS: [&[u8]; 4] = [b"SphereKeyX", b"SphereKeyY", b"SphereKeyZ", b"SphereKeyW"];

fn bad(why: impl Into<String>) -> LevelWriteError {
    LevelWriteError::Refused(format!("cull tree: {}", why.into()))
}

/// A sphere (centre, radius) in metres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sphere {
    /// The centre.
    pub c: [f64; 3],
    /// The radius.
    pub r: f64,
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

impl Sphere {
    fn contains(&self, o: &Self) -> bool {
        dist(self.c, o.c) + o.r <= self.r + 1e-6
    }
}

fn s16(v: u16) -> f64 {
    f64::from(v as i16)
}

/// `culltree.decode`: the sphere of a node's keys, as the game decodes it.
#[must_use]
pub fn decode(keys: [u16; 4]) -> Sphere {
    Sphere {
        c: [
            s16(keys[0]) / CENTER_SCALE,
            s16(keys[1]) / CENTER_SCALE,
            s16(keys[2]) / CENTER_SCALE,
        ],
        r: s16(keys[3]) / RADIUS_SCALE,
    }
}

/// `culltree.encode`: keys for a sphere, the radius rounded up so the decoded sphere holds it and `contain`.
///
/// # Errors
/// A centre outside +-2048 m or a radius over 8191 m.
pub fn encode(s: &Sphere, contain: &[Sphere]) -> Result<[u16; 4]> {
    let mut keys = [0i64; 3];
    for (k, v) in keys.iter_mut().zip(s.c) {
        *k = (v * CENTER_SCALE).round() as i64;
        if !(-32768..=32767).contains(k) {
            return Err(bad(format!("sphere centre {:?} is outside +-2048 m", s.c)));
        }
    }
    let qc = keys.map(|k| k as f64 / CENTER_SCALE);
    let need = contain
        .iter()
        .map(|o| dist(qc, o.c) + o.r)
        .fold(dist(qc, s.c) + s.r, f64::max);
    let w = (need * RADIUS_SCALE - 1e-9).ceil() as i64;
    if w > 32767 {
        return Err(bad(format!("sphere radius {need:.1} m is more than a cull tree holds")));
    }
    Ok([
        keys[0] as i16 as u16,
        keys[1] as i16 as u16,
        keys[2] as i16 as u16,
        w as u16,
    ])
}

/// `culltree.enclose`: the smallest sphere holding both.
#[must_use]
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
    Sphere {
        c: std::array::from_fn(|i| a.c[i] + (b.c[i] - a.c[i]) * t),
        r,
    }
}

#[derive(Debug, Clone, Copy)]
struct TNode {
    max: f32,
    keys: [u16; 4],
    left: usize,
    right: usize,
    leaf: Option<u32>,
}

impl TNode {
    fn sphere(&self) -> Sphere {
        decode(self.keys)
    }
}

/// A static cull tree read from its partition.
#[derive(Debug, Clone)]
pub struct CullTree {
    nodes: Vec<TNode>,
    root: usize,
    /// `EnumOffset`.
    pub enum_offset: u32,
    /// Leaf index -> node.
    leaves: HashMap<u32, usize>,
}

fn number(d: &EditDbx, n: NodeId) -> Option<(u32, Vec<u8>)> {
    let (size, vals) = numbers_of(d.raw(n))?;
    Some((size, vals.first()?.to_vec()))
}

fn tree_instance(d: &EditDbx) -> Result<NodeId> {
    d.of_type(b"World.StaticCullTreeData")
        .first()
        .copied()
        .ok_or_else(|| bad("no World.StaticCullTreeData instance"))
}

impl CullTree {
    /// The tree of a cull tree partition.
    ///
    /// # Errors
    /// Not a cull tree, or nodes that do not make one preorder tree.
    pub fn read(d: &EditDbx) -> Result<Self> {
        let inst = tree_instance(d)?;
        let array = d.child(inst, b"Nodes").ok_or_else(|| bad("no Nodes"))?;
        let eo = d.child(inst, b"EnumOffset").ok_or_else(|| bad("no EnumOffset"))?;
        let enum_offset = number(d, eo)
            .and_then(|(_, v)| <[u8; 4]>::try_from(v.as_slice()).ok())
            .map(u32::from_be_bytes)
            .ok_or_else(|| bad("EnumOffset is not a 4-byte number"))?;
        let mut flat: Vec<(i32, f32, [u16; 4])> = Vec::new();
        for &c in d.children(array).unwrap_or_default() {
            let mut right = None;
            let mut max = None;
            let mut keys = [None; 4];
            for &f in d.children(c).unwrap_or_default() {
                let name = d.attr(f, b"name").unwrap_or_default();
                let Some((_, v)) = number(d, f) else { continue };
                if name == b"Right" {
                    right = <[u8; 4]>::try_from(v.as_slice()).ok().map(i32::from_be_bytes);
                } else if name == b"Max" {
                    max = <[u8; 4]>::try_from(v.as_slice()).ok().map(f32::from_be_bytes);
                } else if let Some(k) = KEYS.iter().position(|k| *k == name) {
                    keys[k] = v
                        .get(..2)
                        .and_then(|b| <[u8; 2]>::try_from(b).ok())
                        .map(u16::from_be_bytes);
                }
            }
            let (Some(r), Some(m), [Some(a), Some(b), Some(x), Some(w)]) = (right, max, keys) else {
                return Err(bad("a node without Right, Max or a sphere key"));
            };
            flat.push((r, m, [a, b, x, w]));
        }
        let mut used = flat.len();
        while used > 0 && flat.get(used - 1).is_some_and(|n| *n == (0, 0.0, [0; 4])) {
            used -= 1;
        }
        flat.truncate(used);
        Self::build(&flat, enum_offset)
    }

    /// `culltree.build`: the preorder list as a linked tree, every `Right` checked.
    fn build(flat: &[(i32, f32, [u16; 4])], enum_offset: u32) -> Result<Self> {
        if flat.first().is_none_or(|n| n.0 == 0) {
            return Err(bad("the tree is empty"));
        }
        let mut nodes = Vec::with_capacity(flat.len());
        let mut index_of = HashMap::new();
        // (flat index, parent node, is the right child)
        let mut work: Vec<(usize, Option<(usize, bool)>)> = vec![(0, None)];
        let mut leaves = HashMap::new();
        while let Some((i, parent)) = work.pop() {
            let &(right, max, keys) = flat.get(i).ok_or_else(|| bad(format!("node {i} is past the end")))?;
            if index_of.contains_key(&i) || nodes.len() > flat.len() {
                return Err(bad("the Right offsets loop back into the tree"));
            }
            let id = nodes.len();
            index_of.insert(i, id);
            let leaf = (right < 0).then_some((right as u32) & !LEAF);
            nodes.push(TNode {
                max,
                keys,
                left: usize::MAX,
                right: usize::MAX,
                leaf,
            });
            if let Some(l) = leaf {
                if leaves.insert(l, id).is_some() {
                    return Err(bad(format!("leaf index {l} is used twice")));
                }
            } else {
                if right < 2 {
                    return Err(bad(format!("node {i}: Right {right} cannot hold a left subtree")));
                }
                work.push((i + right as usize, Some((id, true))));
                work.push((i + 1, Some((id, false))));
            }
            if let Some((p, is_right)) = parent
                && let Some(pn) = nodes.get_mut(p)
            {
                if is_right {
                    pn.right = id;
                } else {
                    pn.left = id;
                }
            }
        }
        let tree = Self {
            nodes,
            root: 0,
            enum_offset,
            leaves,
        };
        let back = tree.flatten();
        if back.len() != flat.len() || back.iter().zip(flat).any(|(a, b)| a.0 != b.0) {
            return Err(bad("the Right offsets do not describe one preorder tree"));
        }
        Ok(tree)
    }

    fn node(&self, i: usize) -> Result<&TNode> {
        self.nodes.get(i).ok_or_else(|| bad("internal: a node out of range"))
    }

    /// The leaf count (the next new leaf is this + 1).
    #[must_use]
    pub fn leaf_count(&self) -> u32 {
        self.leaves.len() as u32
    }

    /// The sphere and `Max` of a leaf.
    #[must_use]
    pub fn leaf(&self, index: u32) -> Option<(Sphere, f32)> {
        let n = self.nodes.get(*self.leaves.get(&index)?)?;
        Some((n.sphere(), n.max))
    }

    /// `culltree.flatten`: preorder `(Right, Max, keys)` with `Right` from the left subtree's size.
    #[must_use]
    pub fn flatten(&self) -> Vec<(i32, f32, [u16; 4])> {
        let mut size = vec![0usize; self.nodes.len()];
        let mut st = vec![(self.root, false)];
        while let Some((n, done)) = st.pop() {
            let Some(t) = self.nodes.get(n) else { continue };
            if t.leaf.is_some() {
                if let Some(s) = size.get_mut(n) {
                    *s = 1;
                }
            } else if done {
                let s = 1 + size.get(t.left).copied().unwrap_or(0) + size.get(t.right).copied().unwrap_or(0);
                if let Some(x) = size.get_mut(n) {
                    *x = s;
                }
            } else {
                st.push((n, true));
                st.push((t.right, false));
                st.push((t.left, false));
            }
        }
        let mut out = Vec::with_capacity(self.nodes.len());
        let mut pre = vec![self.root];
        while let Some(n) = pre.pop() {
            let Some(t) = self.nodes.get(n) else { continue };
            if let Some(l) = t.leaf {
                out.push(((LEAF | l) as i32, t.max, t.keys));
            } else {
                out.push(((1 + size.get(t.left).copied().unwrap_or(0)) as i32, t.max, t.keys));
                pre.push(t.right);
                pre.push(t.left);
            }
        }
        out
    }

    /// `culltree.make_subtree`: one object's leaves (a balanced split; two leaves under one internal node).
    fn make_subtree(&mut self, placed: &[(Sphere, f32)], first: u32) -> Result<(usize, Sphere)> {
        let mut ids = Vec::new();
        for (j, (s, mx)) in placed.iter().enumerate() {
            let keys = encode(s, &[])?;
            let leaf = first + j as u32;
            self.nodes.push(TNode {
                max: *mx,
                keys,
                left: usize::MAX,
                right: usize::MAX,
                leaf: Some(leaf),
            });
            ids.push(self.nodes.len() - 1);
            self.leaves.insert(leaf, self.nodes.len() - 1);
        }
        let top = self.join(&ids)?;
        let t = self.node(top)?;
        if t.leaf.is_none() {
            return Ok((top, t.sphere()));
        }
        let mut whole = self.node(*ids.first().ok_or_else(|| bad("no leaves"))?)?.sphere();
        for &i in ids.iter().skip(1) {
            whole = enclose(&whole, &self.node(i)?.sphere());
        }
        Ok((top, whole))
    }

    fn join(&mut self, ids: &[usize]) -> Result<usize> {
        match ids {
            [] => Err(bad("no leaves")),
            [one] => Ok(*one),
            _ => {
                let h = ids.len() / 2;
                let a = self.join(ids.get(..h).unwrap_or_default())?;
                let b = self.join(ids.get(h..).unwrap_or_default())?;
                let (na, nb) = (*self.node(a)?, *self.node(b)?);
                let s = enclose(&na.sphere(), &nb.sphere());
                let keys = encode(&s, &[na.sphere(), nb.sphere()])?;
                self.nodes.push(TNode {
                    max: na.max.max(nb.max),
                    keys,
                    left: a,
                    right: b,
                    leaf: None,
                });
                Ok(self.nodes.len() - 1)
            }
        }
    }

    /// `culltree.insert`: the subtree goes where the spheres already hold it, or under a new node in place of the
    /// child that grows least (that child keeps its side); ancestors' `Max` raised; the root's sphere grown if needed.
    fn insert(&mut self, sub: usize, sph: &Sphere) -> Result<()> {
        let sub_max = self.node(sub)?.max;
        let mut path = vec![self.root];
        let mut n = self.root;
        let (target, parent) = loop {
            let t = *self.node(n)?;
            if t.leaf.is_some() {
                return Err(bad("internal: descended into a leaf"));
            }
            let (l, r) = (*self.node(t.left)?, *self.node(t.right)?);
            let mut cands: Vec<(usize, TNode)> = [(t.left, l), (t.right, r)]
                .into_iter()
                .filter(|(_, c)| c.sphere().contains(sph))
                .collect();
            cands.sort_by(|a, b| a.1.sphere().r.total_cmp(&b.1.sphere().r));
            if let Some(&(c, cn)) = cands.first() {
                if cn.leaf.is_none() {
                    path.push(c);
                    n = c;
                    continue;
                }
                break (c, n);
            }
            let grow = |x: &TNode| enclose(&x.sphere(), sph).r - x.sphere().r;
            break (if grow(&r) < grow(&l) { t.right } else { t.left }, n);
        };
        let tn = *self.node(target)?;
        let s = enclose(&tn.sphere(), sph);
        let keys = encode(&s, &[tn.sphere(), *sph])?;
        let pn = *self.node(parent)?;
        let (left, right) = if pn.left == target {
            (target, sub)
        } else {
            (sub, target)
        };
        self.nodes.push(TNode {
            max: tn.max.max(sub_max),
            keys,
            left,
            right,
            leaf: None,
        });
        let p = self.nodes.len() - 1;
        if let Some(pm) = self.nodes.get_mut(parent) {
            if pm.left == target {
                pm.left = p;
            } else {
                pm.right = p;
            }
        }
        for a in path {
            if let Some(an) = self.nodes.get_mut(a)
                && an.max < sub_max
            {
                an.max = sub_max;
            }
        }
        let root = *self.node(self.root)?;
        if !root.sphere().contains(sph) {
            let rs = enclose(&root.sphere(), sph);
            let keys = encode(&rs, &[root.sphere(), *sph])?;
            if let Some(rn) = self.nodes.get_mut(self.root) {
                rn.keys = keys;
            }
        }
        Ok(())
    }

    /// New leaves for one object (`culltree.add`): `placed` its spheres and `Max` values; returns its new slot.
    ///
    /// # Errors
    /// A sphere the tree's keys cannot hold.
    pub fn add(&mut self, placed: &[(Sphere, f32)]) -> Result<u32> {
        let first = self.leaf_count() + 1;
        let (sub, sph) = self.make_subtree(placed, first)?;
        self.insert(sub, &sph)?;
        Ok(self.enum_offset + first)
    }

    /// The tree written into its partition (`CullTree.write`): `Nodes` rebuilt from the first node as the template,
    /// then the spare nodes; values written in the sizes they were read in.
    ///
    /// # Errors
    /// Not a cull tree partition.
    pub fn write_into(&self, d: &mut EditDbx) -> Result<()> {
        let inst = tree_instance(d)?;
        let array = d.child(inst, b"Nodes").ok_or_else(|| bad("no Nodes"))?;
        let tmpl = *d
            .children(array)
            .and_then(<[NodeId]>::first)
            .ok_or_else(|| bad("no nodes to copy"))?;
        let mut flat = self.flatten();
        flat.extend(std::iter::repeat_n((0, 0.0, [0; 4]), SPARE));
        let mut items = Vec::with_capacity(flat.len());
        for (right, max, keys) in flat {
            let c = d.clone_node(tmpl);
            for f in d.children(c).unwrap_or_default().to_vec() {
                if d.kind(f) != NUMBERS {
                    continue;
                }
                let name = d.attr(f, b"name").unwrap_or_default().to_vec();
                let size = numbers_of(d.raw(f)).map_or(4, |(s, _)| s);
                let value: Vec<u8> = if name == b"Right" {
                    right.to_be_bytes().to_vec()
                } else if name == b"Max" {
                    max.to_be_bytes().to_vec()
                } else if let Some(k) = KEYS.iter().position(|k| *k == name.as_slice()) {
                    let mut v = keys.get(k).copied().unwrap_or(0).to_be_bytes().to_vec();
                    v.resize(size as usize, 0);
                    v
                } else {
                    continue;
                };
                let mut raw = varint_bytes(1);
                raw.extend(varint_bytes(size));
                raw.extend(value);
                d.set_raw(f, raw)?;
            }
            items.push(c);
        }
        *d.children_mut(array)? = items;
        Ok(())
    }
}

fn apply_rows(rows: &Rows, local: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| {
        local[0] * f64::from(rows[0][i])
            + local[1] * f64::from(rows[1][i])
            + local[2] * f64::from(rows[2][i])
            + f64::from(rows[3][i])
    })
}

fn to_local(rows: &Rows, world: [f64; 3]) -> [f64; 3] {
    let d: [f64; 3] = std::array::from_fn(|i| world[i] - f64::from(rows[3][i]));
    // the rows are orthonormal (a placed object's rotation): the inverse is the transpose
    std::array::from_fn(|r| (0..3).map(|i| d[i] * f64::from(rows[r][i])).sum())
}

/// An object's `Enumeration` (its slot), 0 when it has none.
#[must_use]
pub fn slot_of(w: &LevelWriter, inst: NodeId) -> u32 {
    w.d.child(inst, b"Enumeration")
        .and_then(|e| number(&w.d, e))
        .and_then(|(_, v)| <[u8; 4]>::try_from(v.as_slice()).ok())
        .map_or(0, u32::from_be_bytes)
}

fn set_slot(w: &mut LevelWriter, inst: NodeId, slot: u32) -> Result<()> {
    let e =
        w.d.child(inst, b"Enumeration")
            .ok_or_else(|| bad("an object without Enumeration"))?;
    w.d.set_numbers(e, &[slot.to_be_bytes()])?;
    Ok(())
}

/// What [`reslot`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reslotted {
    /// (guid, old slot, new slot, leaves) of each object given new leaves.
    pub objects: Vec<(String, u32, u32, usize)>,
    /// Leaves before and after.
    pub leaves: (u32, u32),
}

/// Gives every placed object of `edited` that stands elsewhere than the object owning its slot stood in `base` (a
/// moved or turned object, a copy sharing its original's slot) new leaves in the exclusive tree `tree`: its shape
/// is the slot's leaves in the base (each one's centre taken into the base object's space and out into the object's
/// new place), its `Enumeration` the new slot. An object keeps at most two leaves (`README.md`, "Leaves per object").
///
/// # Errors
/// A tree that does not read, or a sphere it cannot hold.
pub fn reslot(base: &LevelWriter, edited: &mut LevelWriter, tree: &mut EditDbx) -> Result<Reslotted> {
    let mut t = CullTree::read(tree)?;
    let before = t.leaf_count();
    // the base's slotted objects: slot -> (guid, transform)
    let mut by_slot: Vec<(u32, String, Rows)> = Vec::new();
    for e in base.d.of_type(b".ReferenceObjectData") {
        let s = slot_of(base, e);
        let g = String::from_utf8_lossy(base.d.attr(e, b"guid").unwrap_or_default()).to_ascii_uppercase();
        if s > t.enum_offset
            && let Some(rows) = base.transform(&g)
        {
            by_slot.push((s, g, rows));
        }
    }
    by_slot.sort_by_key(|x| x.0);
    let next_slot: HashMap<u32, u32> = by_slot
        .windows(2)
        .filter_map(|w| Some((w.first()?.0, w.get(1)?.0)))
        .collect();
    let base_of: HashMap<u32, (String, Rows)> = by_slot.iter().map(|(s, g, r)| (*s, (g.clone(), *r))).collect();
    let mut out = Reslotted::default();
    for e in edited.d.of_type(b".ReferenceObjectData") {
        let s = slot_of(edited, e);
        let Some((owner, old)) = base_of.get(&s) else { continue };
        let g = String::from_utf8_lossy(edited.d.attr(e, b"guid").unwrap_or_default()).to_ascii_uppercase();
        let Some(new) = edited.transform(&g) else { continue };
        if *owner == g && new == *old {
            continue;
        }
        let first = s - t.enum_offset;
        let count = next_slot.get(&s).map_or(2, |n| (n - s).clamp(1, 2));
        let mut placed = Vec::new();
        for leaf in first..first + count {
            let Some((sph, mx)) = t.leaf(leaf) else { continue };
            let c = apply_rows(&new, to_local(old, sph.c));
            placed.push((Sphere { c, r: sph.r }, mx));
        }
        if placed.is_empty() {
            continue;
        }
        let slot = t.add(&placed)?;
        set_slot(edited, e, slot)?;
        out.objects.push((g, s, slot, placed.len()));
    }
    out.leaves = (before, t.leaf_count());
    if !out.objects.is_empty() {
        t.write_into(tree)?;
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip_and_contain() {
        let s = Sphere {
            c: [10.03, -4.0, 2047.0],
            r: 3.3,
        };
        let k = encode(&s, &[]).unwrap();
        let d = decode(k);
        assert!(d.contains(&s), "{d:?} {s:?}");
        assert!(
            encode(
                &Sphere {
                    c: [5000.0, 0.0, 0.0],
                    r: 1.0
                },
                &[]
            )
            .is_err()
        );
        let a = Sphere { c: [0.0; 3], r: 1.0 };
        let b = Sphere {
            c: [4.0, 0.0, 0.0],
            r: 1.0,
        };
        let e = enclose(&a, &b);
        assert!(e.contains(&a) && e.contains(&b));
        assert!((e.r - 3.0).abs() < 1e-12);
    }

    #[test]
    fn add_keeps_one_preorder_tree() {
        // a root over two leaves
        let sph = |x: f64| {
            encode(
                &Sphere {
                    c: [x, 0.0, 0.0],
                    r: 1.0,
                },
                &[],
            )
            .unwrap()
        };
        let root = encode(&Sphere { c: [0.0; 3], r: 20.0 }, &[]).unwrap();
        let flat = vec![
            (2, 1.0, root),
            ((LEAF | 1) as i32, 1.0, sph(-5.0)),
            ((LEAF | 2) as i32, 1.0, sph(5.0)),
        ];
        let mut t = CullTree::build(&flat, 100).unwrap();
        let slot = t
            .add(&[
                (
                    Sphere {
                        c: [30.0, 0.0, 0.0],
                        r: 2.0,
                    },
                    2.0,
                ),
                (
                    Sphere {
                        c: [30.0, 0.0, 0.0],
                        r: 2.0,
                    },
                    2.0,
                ),
            ])
            .unwrap();
        assert_eq!(slot, 103);
        let back = t.flatten();
        // rebuilt from its own flattening: the same tree, five leaves now... 2 + 2 new
        let again = CullTree::build(&back, 100).unwrap();
        assert_eq!(again.leaf_count(), 4);
        // every ancestor holds the new leaves' spheres; the root grew to hold them
        assert!(decode(back[0].2).contains(&Sphere {
            c: [30.0, 0.0, 0.0],
            r: 2.0
        }));
        assert!(back[0].1 >= 2.0);
    }
}
