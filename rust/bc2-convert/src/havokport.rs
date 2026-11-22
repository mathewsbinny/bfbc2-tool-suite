//! Battlefield 1943's physics (Havok 4.6.1, Xbox 360) towards BFBC2's PC format (`bfbc2-bf1943-import/havokport.py`,
//! round F2b): the 4.6.1 packfile restructured as a big-endian 5.5.0 one ([`restructure`]), the wrapper's material
//! table put in BFBC2's byte order ([`run_table_to_pc`]), the shape subparts in 5.5.0's field order with the w
//! lanes BFBC2's exporter writes ([`shape_subparts_to_550`], [`aabb_w`]), and the physics materials' readers and
//! writers ([`part_materials`], [`with_part_materials`], [`havok_materials`], [`with_havok_materials`],
//! [`bodies`], [`with_face_materials`]). Between the first two steps the caller runs the console converter
//! (`havok.convert`, whose packfile walk is [`crate::havok`]).
//!
//! Each function is the reference step for step. [`Port::Error`] / [`Port::Material`] carry the reference's
//! `PortError` / `MaterialError` messages; [`Port::Refused`] is returned wherever the reference would raise
//! anything else (`struct.error`, `IndexError`, `KeyError`, `ValueError` from a search) or read a short slice whose
//! effect is not reproduced here - the caller then runs the reference, which raises its own error.

#![allow(clippy::many_single_char_names)] // the reference's names (b, p, o, k ...), kept to read side by side

use std::collections::{HashMap, HashSet};

use crate::pyfmt::{OrderedMap, round_to};

/// The packfile magic (the same bytes on both platforms).
pub const MAGIC: [u8; 8] = [0x57, 0xe0, 0xe0, 0x57, 0x10, 0xc0, 0xc0, 0x10];

/// A port's outcome other than its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Port {
    /// The reference's `PortError` with its message.
    Error(String),
    /// The reference's `MaterialError` with its message.
    Material(String),
    /// Not done here: the reference raises another exception (or decides on a value this port does not).
    Refused,
}

/// A result of this module.
pub type Result<T> = std::result::Result<T, Port>;

fn err<T>(m: impl Into<String>) -> Result<T> {
    Err(Port::Error(m.into()))
}

fn merr<T>(m: impl Into<String>) -> Result<T> {
    Err(Port::Material(m.into()))
}

/// `b.find(MAGIC)`.
#[must_use]
pub fn find_magic(b: &[u8]) -> Option<usize> {
    b.windows(MAGIC.len()).position(|w| w == MAGIC)
}

fn slice(b: &[u8], a: usize, n: usize) -> Result<&[u8]> {
    b.get(a..a.checked_add(n).ok_or(Port::Refused)?).ok_or(Port::Refused)
}

fn u32_at(b: &[u8], o: usize, big: bool) -> Result<u32> {
    let s: [u8; 4] = slice(b, o, 4)?.try_into().map_err(|_| Port::Refused)?;
    Ok(if big {
        u32::from_be_bytes(s)
    } else {
        u32::from_le_bytes(s)
    })
}

fn u16_at(b: &[u8], o: usize, big: bool) -> Result<u16> {
    let s: [u8; 2] = slice(b, o, 2)?.try_into().map_err(|_| Port::Refused)?;
    Ok(if big {
        u16::from_be_bytes(s)
    } else {
        u16::from_le_bytes(s)
    })
}

fn f32_at(b: &[u8], o: usize, big: bool) -> Result<f32> {
    Ok(f32::from_bits(u32_at(b, o, big)?))
}

fn put(b: &mut [u8], o: usize, v: &[u8]) -> Result<()> {
    let end = o.checked_add(v.len()).ok_or(Port::Refused)?;
    b.get_mut(o..end).ok_or(Port::Refused)?.copy_from_slice(v);
    Ok(())
}

fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| char::from(c)).collect()
}

/// `bytes.index(0, start)`.
fn nul_from(b: &[u8], start: usize) -> Result<usize> {
    b.get(start..)
        .and_then(|r| r.iter().position(|&c| c == 0))
        .map(|p| p + start)
        .ok_or(Port::Refused)
}

/// A section: its tag (as the reference splits it) and its seven offsets.
type Sec = (String, [u32; 7]);

/// `_sections(d, p, e)`.
fn sections(d: &[u8], p: usize, big: bool) -> Result<Vec<Sec>> {
    let n = u32_at(d, p + 20, big)? as usize;
    let mut out = Vec::new();
    for i in 0..n {
        let h = p + 64 + 48 * i;
        let tag = slice(d, h, 20)?;
        let name = latin1(tag.split(|&c| c == 0).next().unwrap_or_default());
        let mut v = [0u32; 7];
        for (k, x) in v.iter_mut().enumerate() {
            *x = u32_at(d, h + 20 + 4 * k, big)?;
        }
        out.push((name, v));
    }
    Ok(out)
}

/// `_table(d, start, end, size, e)`: rows until an 0xFFFFFFFF.
fn read_table(d: &[u8], start: usize, end: usize, size: usize, big: bool) -> Result<Vec<Vec<u32>>> {
    let mut out = Vec::new();
    let mut o = start;
    while o + size <= end && u32_at(d, o, big)? != 0xFFFF_FFFF {
        let mut row = Vec::with_capacity(size / 4);
        for k in 0..size / 4 {
            row.push(u32_at(d, o + 4 * k, big)?);
        }
        out.push(row);
        o += size;
    }
    Ok(out)
}

/// `_classnames(d, start, e)`: (offset of the name in the section, name, signature).
fn classnames(d: &[u8], start: usize, big: bool) -> Result<Vec<(usize, String, u32)>> {
    let mut out = Vec::new();
    let mut o = start;
    while *d.get(o + 4).ok_or(Port::Refused)? == 0x09 {
        let q = nul_from(d, o + 5)?;
        out.push((
            o + 5 - start,
            latin1(d.get(o + 5..q).unwrap_or_default()),
            u32_at(d, o, big)?,
        ));
        o = q + 1;
    }
    Ok(out)
}

/// The 5.5.0 class layouts restructure needs: the class names, and per 5.5.0 class of `AABB_W` its `aabbCenter`
/// offset (`None` where `_member_offset` would raise).
#[derive(Debug, Clone, Default)]
pub struct Layouts {
    /// The names of the game's classes.
    pub names: HashSet<String>,
    /// `_member_offset(classes, cls, 'aabbCenter')` of the bounding-box shapes.
    pub aabb_center: HashMap<String, Option<u32>>,
}

impl Layouts {
    /// `_rename(name, classes)`.
    #[must_use]
    pub fn rename(&self, name: &str) -> Option<String> {
        if self.names.contains(name) || name == "HavokMaterial" || name == "HavokMaterials" {
            return Some(name.to_owned());
        }
        if let Some(rest) = name.strip_prefix("hk") {
            let p = format!("hkp{rest}");
            if self.names.contains(&p) {
                return Some(p);
            }
        }
        None
    }
}

/// The 4.6.1-only deactivators, left out.
pub const DROPPED: [&str; 2] = ["hkSpatialRigidBodyDeactivator", "hkRigidBodyDeactivator"];
/// hkRigidBody: (4.6.1 offset, 5.5.0 offset, bytes) copied.
pub const RIGID_COPY: [(usize, usize, usize); 9] = [
    (0, 0, 48),
    (48, 88, 52),
    (100, 140, 44),
    (148, 184, 12),
    (160, 208, 16),
    (176, 224, 160),
    (336, 384, 4),
    (348, 388, 8),
    (368, 400, 96),
];
/// hkRigidBody's 5.5.0 constants.
pub const RIGID_CONST: [(usize, &[u8]); 2] = [(48, b"\0\0\0\x01"), (202, b"\x03\x01")];
/// hkMoppBvTreeShape: header; code pointer; child shape pointer.
pub const MOPP_TREE_COPY: [(usize, usize, usize); 3] = [(0, 0, 16), (24, 16, 4), (20, 52, 4)];
/// The 4.6.1 classes whose bounding box centre's w becomes 1.0.
pub const AABB_W: [&str; 3] = ["hkListShape", "hkStorageExtendedMeshShape", "hkExtendedMeshShape"];

/// `ufloat8(v, table)`: the index of the nearest value (the first of equals; `min`'s rule).
#[must_use]
pub fn ufloat8(v: f64, table: &[f64]) -> usize {
    let mut best = 0usize;
    let mut bk = table.first().map_or(f64::NAN, |t| (t - v).abs());
    for (i, t) in table.iter().enumerate().skip(1) {
        let k = (t - v).abs();
        if k < bk {
            best = i;
            bk = k;
        }
    }
    best
}

/// `ufloat8_table(exe)`: Havok's hkUFloat8 decoding table found in the executable, or `None`.
#[must_use]
pub fn ufloat8_table(exe: &[u8]) -> Option<Vec<f64>> {
    let target = 0.01f32.to_bits();
    for off in 0..4usize {
        let n = (exe.len().saturating_sub(off)) / 4;
        let word = |i: usize| -> Option<f32> {
            let s: [u8; 4] = exe.get(off + 4 * i..off + 4 * i + 4)?.try_into().ok()?;
            Some(f32::from_le_bytes(s))
        };
        for i in 0..n {
            // np.where(a == float32(0.01)): equal values (0.01's bits, and no other float equals it)
            if word(i).map(f32::to_bits) != Some(target) {
                continue;
            }
            if i < 1 || i - 1 + 256 > n {
                continue;
            }
            let t: Vec<f64> = (i - 1..i + 255).filter_map(word).map(f64::from).collect();
            let (Some(&first), Some(&last)) = (t.first(), t.last()) else {
                continue;
            };
            if t.len() == 256
                && first == 0.0
                && 9e5 < last
                && last < 1.1e6
                && t.windows(2)
                    .all(|w| w.get(1).zip(w.first()).is_some_and(|(b, a)| b - a > 0.0))
            {
                return Some(t);
            }
        }
    }
    None
}

/// `restructure(b, template, classes, table)`: the 4.6.1 resource as a big-endian 5.5.0 one.
///
/// # Errors
/// The reference's `PortError`s; [`Port::Refused`] where it raises anything else.
#[expect(clippy::too_many_lines, reason = "the reference's one function, step for step")]
pub fn restructure(b: &[u8], template: &[u8], classes: &Layouts, table: Option<&[f64]>) -> Result<Vec<u8>> {
    let Some(p) = find_magic(b) else {
        return err("not a Havok 4.6.1 packfile");
    };
    let ver = b.get(p + 40..(p + 56).min(b.len())).unwrap_or_default();
    if !ver.windows(11).any(|w| w == b"Havok-4.6.1") {
        return err("not a Havok 4.6.1 packfile");
    }
    let secs = sections(b, p, true)?;
    let tags: Vec<&str> = secs.iter().map(|(t, _)| t.as_str()).collect();
    let cn = tags.iter().position(|&t| t == "__classnames__").ok_or(Port::Refused)?;
    let data = tags.iter().position(|&t| t == "__data__").ok_or(Port::Refused)?;
    let cn_off = secs.get(cn).ok_or(Port::Refused)?.1[0] as usize;
    let mut names: HashMap<usize, String> = HashMap::new();
    for (off, n, _) in classnames(b, p + cn_off, true)? {
        names.insert(off, n);
    }
    // the template's class names (PC: little-endian), written big-endian
    let tp = find_magic(template).ok_or(Port::Refused)?;
    let tsecs = sections(template, tp, false)?;
    let tcn = tsecs
        .iter()
        .find(|(t, _)| t == "__classnames__")
        .ok_or(Port::Refused)?
        .1;
    let tnames = classnames(template, tp + tcn[0] as usize, false)?;
    let mut new_cn: Vec<u8> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    for (_, n, sig) in &tnames {
        at.insert(n.clone(), new_cn.len() + 5);
        new_cn.extend_from_slice(&sig.to_be_bytes());
        new_cn.push(0x09);
        new_cn.extend(n.chars().map(|c| c as u8));
        new_cn.push(0);
    }
    while !new_cn.len().is_multiple_of(16) {
        new_cn.push(0xFF);
    }
    // the data section: content, fixups
    let [start, loc, glob, virt, exp, _imp, _end] = secs.get(data).ok_or(Port::Refused)?.1;
    let (loc, glob, virt, exp) = (loc as usize, glob as usize, virt as usize, exp as usize);
    let base = p + start as usize;
    let mut content: Vec<u8> = slice(b, base, loc)?.to_vec();
    let mut local: OrderedMap<u32, u32> = OrderedMap::new();
    for r in read_table(b, base + loc, base + glob, 8, true)? {
        local.insert(col(&r, 0), col(&r, 1));
    }
    let gfix = read_table(b, base + glob, base + virt, 12, true)?;
    let vfix = read_table(b, base + virt, base + exp, 12, true)?;
    let mut objects: Vec<(u32, String)> = Vec::new();
    for r in &vfix {
        let (o, sec, noff) = (col(r, 0), col(r, 1) as usize, col(r, 2) as usize);
        match names.get(&noff) {
            Some(n) if sec == cn => objects.push((o, n.clone())),
            _ => {
                return err(format!(
                    "object at {o:#x}: its class name is not in the class-name section"
                ));
            }
        }
    }
    let dropped: Vec<&(u32, String)> = objects.iter().filter(|(_, n)| DROPPED.contains(&n.as_str())).collect();
    let kept: Vec<&(u32, String)> = objects.iter().filter(|(_, n)| !DROPPED.contains(&n.as_str())).collect();
    let mut unknown: Vec<&str> = kept
        .iter()
        .filter(|(_, n)| classes.rename(n).is_none())
        .map(|(_, n)| n.as_str())
        .collect();
    unknown.sort_unstable();
    unknown.dedup();
    if !unknown.is_empty() {
        return err(format!("classes with no 5.5.0 counterpart: {}", unknown.join(", ")));
    }
    let mut missing: Vec<String> = kept
        .iter()
        .filter_map(|(_, n)| classes.rename(n))
        .filter(|r| !at.contains_key(r))
        .collect();
    missing.sort_unstable();
    missing.dedup();
    if !missing.is_empty() {
        return err(format!(
            "the template's class names lack {}: give a BFBC2 file that has them",
            missing.join(", ")
        ));
    }
    let mut starts: Vec<u32> = objects.iter().map(|(o, _)| *o).collect();
    starts.sort_unstable();
    let span = |o: u32| -> i64 {
        let next = starts.iter().find(|&&s| s > o).map_or(loc as i64, |&s| i64::from(s));
        next - i64::from(o)
    };
    for (o, _) in &dropped {
        let end = i64::from(*o) + span(*o);
        let gone: Vec<u32> = local
            .keys()
            .filter(|&&s| *o <= s && i64::from(s) < end)
            .copied()
            .collect();
        for s in gone {
            local.remove(&s);
        }
    }
    // old slot -> new slot (None: dropped), in rewritten objects
    let mut slot_map: HashMap<u32, Option<u32>> = HashMap::new();
    for (o, n) in &kept {
        let ou = *o as usize;
        if n == "hkRigidBody" {
            let old = slice(&content, ou, 512)?.to_vec();
            let mut new = vec![0u8; 512];
            for &(src, dst, size) in &RIGID_COPY {
                put(&mut new, dst, slice(&old, src, size)?)?;
                for k in (0..size).step_by(4) {
                    slot_map.insert(o + (src + k) as u32, Some(o + (dst + k) as u32));
                }
            }
            for &(dst, val) in &RIGID_CONST {
                put(&mut new, dst, val)?;
            }
            for (k, src) in [(396usize, 340usize), (397, 344)] {
                let v = round_to(f64::from(f32_at(&old, src, true)?), 3);
                let byte = if let Some(t) = table {
                    ufloat8(v, t)
                } else if v.to_bits() == 200.0f64.to_bits() {
                    0x7F
                } else {
                    // the reference's message prints the value as Python does: left to it
                    return Err(Port::Refused);
                };
                *new.get_mut(k).ok_or(Port::Refused)? = u8::try_from(byte).map_err(|_| Port::Refused)?;
            }
            *new.get_mut(398).ok_or(Port::Refused)? = *old.get(357).ok_or(Port::Refused)?;
            slot_map.insert(o + 144, None);
            put(&mut content, ou, &new)?;
        } else if n == "hkMoppBvTreeShape" {
            let old = slice(&content, ou, 64)?.to_vec();
            let mut new = vec![0u8; 64];
            for &(src, dst, size) in &MOPP_TREE_COPY {
                put(&mut new, dst, slice(&old, src, size)?)?;
                for k in (0..size).step_by(4) {
                    slot_map.insert(o + (src + k) as u32, Some(o + (dst + k) as u32));
                }
            }
            put(&mut content, ou, &new)?;
        } else if n == "hkMoppCode" {
            *content.get_mut(ou + 44).ok_or(Port::Refused)? = 1;
        }
        if AABB_W.contains(&n.as_str()) {
            let r = classes.rename(n).ok_or(Port::Refused)?;
            let off = classes.aabb_center.get(&r).copied().flatten().ok_or(Port::Refused)? as usize;
            let w = ou + off + 12;
            if content.get(w..w + 4) == Some(&[0, 0, 0, 0][..]) {
                put(&mut content, w, &1.0f32.to_be_bytes())?;
            }
        }
    }
    let gone: HashSet<u32> = dropped.iter().map(|(o, _)| *o).collect();
    let mut new_local: OrderedMap<u32, u32> = OrderedMap::new();
    for (&s, &t) in local.iter() {
        let s2 = match slot_map.get(&s) {
            Some(None) => continue,
            Some(Some(x)) => *x,
            None => s,
        };
        if gone.contains(&t) {
            continue;
        }
        new_local.insert(s2, t);
    }
    // class-name strings in the data (a root container's variants name their class): renamed, appended
    let mut renames: HashMap<&str, String> = HashMap::new();
    #[allow(clippy::iter_over_hash_type, reason = "collected into a map keyed by name")]
    for n in names.values() {
        if let Some(r) = classes.rename(n)
            && r != *n
        {
            renames.insert(n.as_str(), r);
        }
    }
    let mut appended: HashMap<String, u32> = HashMap::new();
    let items: Vec<(u32, u32)> = new_local.iter().map(|(&s, &t)| (s, t)).collect();
    for (s, t) in items {
        let tu = t as usize;
        if tu < loc && content.get(tu..tu + 2) == Some(&b"hk"[..]) {
            // content.find(b'\0', t): -1 when there is none, and content[t:-1] then
            let q = content
                .get(tu..)
                .and_then(|r| r.iter().position(|&c| c == 0))
                .map_or(content.len().saturating_sub(1), |x| x + tu);
            let text = latin1(content.get(tu..q.max(tu)).unwrap_or_default());
            if let Some(r) = renames.get(text.as_str()) {
                if !appended.contains_key(&text) {
                    while !content.len().is_multiple_of(16) {
                        content.push(0);
                    }
                    appended.insert(text.clone(), u32::try_from(content.len()).map_err(|_| Port::Refused)?);
                    content.extend(r.chars().map(|c| c as u8));
                    content.push(0);
                }
                if let Some(&a) = appended.get(&text) {
                    new_local.insert(s, a);
                }
            }
        }
    }
    while !content.len().is_multiple_of(16) {
        content.push(0);
    }
    // the global fixups: class-name targets to the template's offsets
    let mut new_glob: Vec<[u32; 3]> = Vec::new();
    for r in &gfix {
        let (s, sec, off) = (col(r, 0), col(r, 1) as usize, col(r, 2));
        let s2 = match slot_map.get(&s) {
            Some(None) => continue,
            Some(Some(x)) => *x,
            None => s,
        };
        if sec == cn {
            let n = names.get(&(off as usize)).ok_or(Port::Refused)?;
            let rn = classes.rename(n).ok_or(Port::Refused)?;
            let a = *at.get(&rn).ok_or(Port::Refused)?;
            new_glob.push([s2, 0, u32::try_from(a).map_err(|_| Port::Refused)?]);
        } else if sec == data {
            new_glob.push([s2, 2, off]);
        } else {
            let tag = tags.get(sec).ok_or(Port::Refused)?;
            return err(format!("a global fixup into section {tag}"));
        }
    }
    let mut new_virt: Vec<[u32; 3]> = Vec::new();
    for (o, n) in &kept {
        let rn = classes.rename(n).ok_or(Port::Refused)?;
        let a = *at.get(&rn).ok_or(Port::Refused)?;
        new_virt.push([*o, 0, u32::try_from(a).map_err(|_| Port::Refused)?]);
    }
    let pack_table = |rows: &[Vec<u32>]| -> Vec<u8> {
        let mut out = Vec::new();
        for r in rows {
            for v in r {
                out.extend_from_slice(&v.to_be_bytes());
            }
        }
        while !out.len().is_multiple_of(16) {
            out.push(0xFF);
        }
        out
    };
    let mut lrows: Vec<(u32, u32)> = new_local.iter().map(|(&s, &t)| (s, t)).collect();
    lrows.sort_unstable();
    new_glob.sort_unstable();
    new_virt.sort_unstable();
    let lt = pack_table(&lrows.iter().map(|&(s, t)| vec![s, t]).collect::<Vec<_>>());
    let gt = pack_table(&new_glob.iter().map(|r| r.to_vec()).collect::<Vec<_>>());
    let vt = pack_table(&new_virt.iter().map(|r| r.to_vec()).collect::<Vec<_>>());
    let d_loc = content.len();
    let d_glob = d_loc + lt.len();
    let d_virt = d_glob + gt.len();
    let d_end = d_virt + vt.len();
    // header and section table
    let mut head = slice(b, p, 64)?.to_vec();
    put(&mut head, 12, &5u32.to_be_bytes())?;
    let root_name = names.get(&(u32_at(b, p + 36, true)? as usize)).ok_or(Port::Refused)?;
    let root = classes.rename(root_name).ok_or(Port::Refused)?;
    let root_at = u32::try_from(*at.get(&root).ok_or(Port::Refused)?).map_err(|_| Port::Refused)?;
    for (k, v) in [3u32, 2, 0, 0, root_at].iter().enumerate() {
        put(&mut head, 20 + 4 * k, &v.to_be_bytes())?;
    }
    put(&mut head, 40, b"Havok-5.5.0-r1\0\xff")?;
    let cn_start = 64 + 48 * 3;
    let data_start = cn_start + new_cn.len();
    let u = |x: usize| u32::try_from(x).map_err(|_| Port::Refused);
    let ncn = u(new_cn.len())?;
    let rows: [(&[u8], [u32; 7]); 3] = [
        (b"__classnames__", [u(cn_start)?, ncn, ncn, ncn, ncn, ncn, ncn]),
        (b"__types__", [u(data_start)?, 0, 0, 0, 0, 0, 0]),
        (
            b"__data__",
            [
                u(data_start)?,
                u(d_loc)?,
                u(d_glob)?,
                u(d_virt)?,
                u(d_end)?,
                u(d_end)?,
                u(d_end)?,
            ],
        ),
    ];
    let mut out = Vec::with_capacity(p + 64 + 144 + new_cn.len() + d_end);
    out.extend_from_slice(b.get(..p).unwrap_or_default());
    out.extend_from_slice(&head);
    for (tag, rel) in rows {
        out.extend_from_slice(tag);
        out.resize(out.len() + 19 - tag.len(), 0);
        out.push(0xFF);
        for v in rel {
            out.extend_from_slice(&v.to_be_bytes());
        }
    }
    out.extend_from_slice(&new_cn);
    out.extend_from_slice(&content);
    out.extend_from_slice(&lt);
    out.extend_from_slice(&gt);
    out.extend_from_slice(&vt);
    Ok(out)
}

/// `wrapper_run_table(b)`: (offset, M, C, big-endian) of the wrapper's material table, or `None`.
///
/// # Errors
/// [`Port::Refused`] where the reference's `struct.unpack_from` raises.
pub fn wrapper_run_table(b: &[u8]) -> Result<Option<(usize, usize, usize, bool)>> {
    let o = find_magic(b).map_or(-1i64, |x| x as i64);
    let n = i64::from(u32_at(b, 0, false)?);
    let mut p = 8 + 4 * n;
    if p + 2 > o {
        return Ok(None);
    }
    let a = i64::from(u16_at(b, p as usize, false)?);
    p += 2 + 12 * a;
    if p + 2 > o {
        return Ok(None);
    }
    let c = i64::from(u16_at(b, p as usize, false)?);
    p += 2 + 24 * c;
    if p + 4 > o {
        return Ok(None);
    }
    let pu = p as usize;
    if u16_at(b, pu, false)? == 0 {
        return Ok(Some((pu, 0, 0, false)));
    }
    for big in [false, true] {
        let m = usize::from(u16_at(b, pu, big)?);
        let k = usize::from(u16_at(b, pu + 2, big)?);
        let end = p + 4 + 4 * k as i64 + 2 * m as i64;
        if end > o - 8 {
            continue;
        }
        let mut ends = Vec::with_capacity(m);
        for i in 0..m {
            ends.push(usize::from(u16_at(b, pu + 4 + 4 * k + 2 * i, big)?));
        }
        let ok = (m == 0 && k == 0) || (ends.last() == Some(&k) && ends.is_sorted());
        if ok {
            return Ok(Some((pu, m, k, big)));
        }
    }
    Ok(None)
}

/// `_run_table_to_pc(b)`: the wrapper's material table in BFBC2's byte order (little-endian).
///
/// # Errors
/// The reference's `PortError`; [`Port::Refused`] where it raises anything else.
pub fn run_table_to_pc(b: &[u8]) -> Result<Vec<u8>> {
    let Some((p, m, k, big)) = wrapper_run_table(b)? else {
        if find_magic(b).is_some() && u32_at(b, 0, false)? != u32_at(b, 4, false)? {
            return Ok(b.to_vec());
        }
        return err("the wrapper's material table (the byte run's start) is in neither byte order");
    };
    if !big {
        return Ok(b.to_vec());
    }
    let mut out = b.to_vec();
    let mut q = p;
    for _ in 0..2 {
        let v = u16_at(b, q, true)?;
        put(&mut out, q, &v.to_le_bytes())?;
        q += 2;
    }
    for _ in 0..k {
        let v = u32_at(b, q, true)?;
        put(&mut out, q, &v.to_le_bytes())?;
        q += 4;
    }
    for _ in 0..m {
        let v = u16_at(b, q, true)?;
        put(&mut out, q, &v.to_le_bytes())?;
        q += 2;
    }
    Ok(out)
}

/// The extended mesh shapes' classes.
pub const SHAPE_MESHES: [&str; 2] = ["hkpExtendedMeshShape", "hkpStorageExtendedMeshShape"];
/// A shape subpart's size; `hkpExtendedMeshShape::shapesSubparts` (pointer, count).
pub const SUBPART_SIZE: usize = 64;
/// `hkpExtendedMeshShape::shapesSubparts`.
pub const SUBPARTS_AT: usize = 88;
/// `hkpExtendedMeshShape::trianglesSubparts`.
pub const TRIANGLE_SUBPARTS_AT: usize = 80;
/// `hkpExtendedMeshShape::aabbHalfExtents`.
pub const HALF_EXTENTS_AT: usize = 48;
/// `hkpExtendedMeshShape::aabbCenter`.
pub const CENTER_AT: usize = 64;
/// `ShapesSubpart::childShapes`.
pub const CHILDREN_AT: usize = 16;
/// `hkpConvexShape::radius`.
pub const CONVEX_RADIUS_AT: usize = 16;

/// A convex child's local box: (half extents at, centre at - `None`: 0).
fn convex_box(cls: &str) -> Option<(usize, Option<usize>)> {
    match cls {
        "hkpConvexVerticesShape" => Some((32, Some(48))),
        "hkpBoxShape" => Some((32, None)),
        _ => None,
    }
}

/// `subpart_w_lanes(q)` in float32.
#[must_use]
pub fn subpart_w_lanes(q: [f32; 4]) -> [f32; 3] {
    let [x, y, z, w] = q;
    let two = 2.0f32;
    [two * (y * z - x * w), two * (y * w + x * z), 0.0]
}

/// A shape subpart's children: (centre, half extents, radius).
pub type Child = ([f32; 3], [f32; 3], f32);

/// `numpy`'s `min(a, b)` on float32 scalars as Python's `min` takes it: the first unless the second is smaller.
fn mn(a: f32, b: f32) -> f32 {
    if b < a { b } else { a }
}

fn mx(a: f32, b: f32) -> f32 {
    if b > a { b } else { a }
}

/// `aabb_w(parts)`: (aabbCenter.w, aabbHalfExtents.w) in float32, or `None` when no subpart has a child.
#[must_use]
pub fn aabb_w(parts: &[([f32; 4], Vec<Child>)]) -> Option<(f32, f32)> {
    let mut acc: Option<(f32, f32)> = None;
    let (one, h5) = (1.0f32, 0.5f32);
    for (q, children) in parts {
        if children.is_empty() {
            continue;
        }
        let cw = subpart_w_lanes(*q);
        let mut lohi: Option<(f32, f32)> = None;
        for (c, h, r) in children {
            // np.dot of two float32 3-vectors (the third lane 0) and np.abs(...).sum(): left to right in float32
            let dot = (cw[0] * c[0] + cw[1] * c[1]) + cw[2] * c[2];
            let cc = one + dot;
            let he = (((cw[0] * h[0]).abs() + (cw[1] * h[1]).abs()) + (cw[2] * h[2]).abs()) + *r;
            lohi = Some(match lohi {
                None => (cc - he, cc + he),
                Some((lo, hi)) => (mn(lo, cc - he), mx(hi, cc + he)),
            });
        }
        let (mut lo, mut hi) = lohi?;
        if let Some((cen, half)) = acc {
            lo = mn(cen - half, lo);
            hi = mx(cen + half, hi);
        }
        acc = Some(((hi + lo) * h5, (hi - lo) * h5));
    }
    acc
}

/// The data section of a PC packfile as `_data` reads it: (start, local fixups, global fixups, objects {offset:
/// class} in table order).
struct Data {
    base: usize,
    local: HashMap<u32, u32>,
    glob: HashMap<u32, u32>,
    kinds: OrderedMap<u32, String>,
}

/// The section table of a PC packfile (`{tag: 7 offsets}`, a later tag replacing an earlier one).
fn pc_sections(b: &[u8], p: usize) -> Result<HashMap<String, [u32; 7]>> {
    let n = u32_at(b, p + 20, false)? as usize;
    let mut secs = HashMap::new();
    for i in 0..n {
        let h = p + 64 + 48 * i;
        let tag = slice(b, h, 19)?;
        let name = latin1(tag.split(|&c| c == 0).next().unwrap_or_default());
        let mut v = [0u32; 7];
        for (k, x) in v.iter_mut().enumerate() {
            *x = u32_at(b, h + 20 + 4 * k, false)?;
        }
        secs.insert(name, v);
    }
    Ok(secs)
}

/// The class names of a PC packfile's class-name section: {offset in the section: name}.
fn pc_names(b: &[u8], p: usize, cstart: usize, cloc: usize) -> Result<HashMap<u32, String>> {
    let mut names = HashMap::new();
    let mut o = p + cstart;
    while o + 5 < p + cstart + cloc {
        if u32_at(b, o, false)? == 0xFFFF_FFFF {
            break;
        }
        let q = nul_from(b, o + 5)?;
        names.insert(
            (o + 5 - (p + cstart)) as u32,
            latin1(b.get(o + 5..q).unwrap_or_default()),
        );
        o = q + 1;
    }
    Ok(names)
}

/// The fixup tables' rows, every row but the 0xFFFFFFFF ones (`range(start, end - size + 1, size)`).
fn rows(b: &[u8], start: usize, end: usize, size: usize) -> Result<Vec<Vec<u32>>> {
    let mut out = Vec::new();
    let mut o = start;
    while o + size <= end {
        let mut r = Vec::with_capacity(size / 4);
        for k in 0..size / 4 {
            r.push(u32_at(b, o + 4 * k, false)?);
        }
        if r.first() != Some(&0xFFFF_FFFF) {
            out.push(r);
        }
        o += size;
    }
    Ok(out)
}

/// `a + b` as the reference's unbounded integers give it, `None` past u32 (no key of a table is that large).
fn add(a: u32, b: usize) -> Option<u32> {
    u32::try_from(u64::from(a) + b as u64).ok()
}

fn col(r: &[u32], i: usize) -> u32 {
    r.get(i).copied().unwrap_or(0)
}

/// `_data(b)` (and the reading `shape_subparts_to_550` does the same way).
fn data_of(b: &[u8], p: usize, secs: &HashMap<String, [u32; 7]>) -> Result<Data> {
    let cs = secs.get("__classnames__").ok_or(Port::Refused)?;
    let names = pc_names(b, p, cs[0] as usize, cs[1] as usize)?;
    let ds = secs.get("__data__").ok_or(Port::Refused)?;
    let (start, loc, glob, virt, exp) = (
        ds[0] as usize,
        ds[1] as usize,
        ds[2] as usize,
        ds[3] as usize,
        ds[4] as usize,
    );
    let base = p + start;
    let mut local = HashMap::new();
    for r in rows(b, base + loc, base + glob, 8)? {
        local.insert(col(&r, 0), col(&r, 1));
    }
    let mut glob_ = HashMap::new();
    for r in rows(b, base + glob, base + virt, 12)? {
        glob_.insert(col(&r, 0), col(&r, 2));
    }
    let mut kinds = OrderedMap::new();
    for r in rows(b, base + virt, base + exp, 12)? {
        kinds.insert(col(&r, 0), names.get(&col(&r, 2)).cloned().unwrap_or_default());
    }
    Ok(Data {
        base,
        local,
        glob: glob_,
        kinds,
    })
}

/// `shape_subparts_to_550(pc)`: (bytes, subparts swapped).
///
/// # Errors
/// [`Port::Refused`] where the reference raises (or reads a short slice).
pub fn shape_subparts_to_550(pc: &[u8]) -> Result<(Vec<u8>, usize)> {
    let mut b = pc.to_vec();
    let Some(p) = find_magic(pc) else {
        return Ok((b, 0));
    };
    let secs = pc_sections(pc, p)?;
    if !secs.contains_key("__data__") || !secs.contains_key("__classnames__") {
        return Ok((b, 0));
    }
    let d = data_of(pc, p, &secs)?;
    let base = d.base;
    let mut swapped = 0usize;
    for (&off, cls) in d.kinds.iter() {
        if !SHAPE_MESHES.contains(&cls.as_str()) {
            continue;
        }
        let ou = off as usize;
        let arr = add(off, SUBPARTS_AT).and_then(|k| d.local.get(&k)).copied();
        let cnt = u32_at(&b, base + ou + SUBPARTS_AT + 4, false)? as usize;
        let Some(arr) = arr else { continue };
        let mut parts: Vec<([f32; 4], Vec<Child>)> = Vec::new();
        let mut known = true;
        for k in 0..cnt {
            let r = base + arr as usize + SUBPART_SIZE * k;
            let rot = slice(&b, r + 32, 16)?.to_vec();
            let tra = slice(&b, r + 48, 16)?.to_vec();
            let norm = |v: &[u8]| -> Result<f64> {
                let mut s = 0.0f64;
                for i in 0..4 {
                    let x = f64::from(f32_at(v, 4 * i, false)?);
                    s += x * x;
                }
                Ok(s.sqrt())
            };
            let (nr, nt) = (norm(&rot)?, norm(&tra)?);
            // numpy's norm is float32: a value within rounding of the 1e-2 bound is left to the reference
            for v in [nr, nt] {
                if ((v - 1.0).abs() - 1e-2).abs() < 1e-5 || !v.is_finite() {
                    return Err(Port::Refused);
                }
            }
            if (nr - 1.0).abs() > 1e-2 && (nt - 1.0).abs() < 1e-2 {
                put(&mut b, r + 32, &tra)?;
                put(&mut b, r + 48, &rot)?;
                swapped += 1;
            }
            let nch = u32_at(&b, r + CHILDREN_AT + 4, false)? as usize;
            if nch == 0 {
                continue;
            }
            put(&mut b, r + 60, &1.0f32.to_le_bytes())?;
            let mut quat = [0f32; 4];
            for (i, x) in quat.iter_mut().enumerate() {
                *x = f32_at(&b, r + 32 + 4 * i, false)?;
            }
            let mut children = Vec::new();
            let carr = add(arr, SUBPART_SIZE * k + CHILDREN_AT)
                .and_then(|x| d.local.get(&x))
                .copied();
            if carr.is_none() {
                known = false;
            }
            if let Some(carr) = carr {
                for i in 0..nch {
                    let c = add(carr, 4 * i).and_then(|x| d.glob.get(&x)).copied();
                    let bx = c.and_then(|c| d.kinds.get(&c)).and_then(|k| convex_box(k));
                    let (Some(c), Some((h_at, c_at))) = (c, bx) else {
                        known = false;
                        break;
                    };
                    let cb = base + c as usize;
                    let half = [
                        f32_at(&b, cb + h_at, false)?,
                        f32_at(&b, cb + h_at + 4, false)?,
                        f32_at(&b, cb + h_at + 8, false)?,
                    ];
                    let cen = match c_at {
                        Some(a) => [
                            f32_at(&b, cb + a, false)?,
                            f32_at(&b, cb + a + 4, false)?,
                            f32_at(&b, cb + a + 8, false)?,
                        ],
                        None => [0.0; 3],
                    };
                    children.push((cen, half, f32_at(&b, cb + CONVEX_RADIUS_AT, false)?));
                }
            }
            parts.push((quat, children));
        }
        let has_triangles = u32_at(&b, base + ou + TRIANGLE_SUBPARTS_AT + 4, false)? > 0;
        if known
            && !has_triangles
            && let Some((cen, half)) = aabb_w(&parts)
        {
            put(&mut b, base + ou + CENTER_AT + 12, &cen.to_le_bytes())?;
            put(&mut b, base + ou + HALF_EXTENTS_AT + 12, &half.to_le_bytes())?;
        }
    }
    Ok((b, swapped))
}

/// `convert`'s steps after the console converter: the material table to the PC's order, then the shape subparts.
///
/// # Errors
/// As [`run_table_to_pc`] and [`shape_subparts_to_550`].
pub fn finish(pc: &[u8]) -> Result<Vec<u8>> {
    Ok(shape_subparts_to_550(&run_table_to_pc(pc)?)?.0)
}

// ------------------------------------------------------------------------------------------------ physics materials

/// `_wrapper(b)`: (parts n, the part run's start, the material list's start, its hashes, the packfile's start).
///
/// # Errors
/// The reference's `MaterialError`s; [`Port::Refused`] for its other exceptions.
pub fn wrapper(b: &[u8]) -> Result<(usize, usize, usize, Vec<u32>, usize)> {
    let o = find_magic(b);
    let Some(o) = o.filter(|_| b.len() >= 8) else {
        return merr("no Havok packfile");
    };
    let (n, n2) = (u32_at(b, 0, false)?, u32_at(b, 4, false)?);
    if n != n2 {
        return merr("not a part-list wrapper (its counts differ)");
    }
    let n = n as usize;
    let mut p = 8 + 4 * n;
    p += 2 + 12 * usize::from(u16_at(b, p, false)?);
    p += 2 + 24 * usize::from(u16_at(b, p, false)?);
    let span = o as i64 - p as i64;
    let kmax = if span >= 0 { span / 4 } else { (span - 3) / 4 };
    for k in 0..kmax.max(0) as usize {
        let q = o as i64 - 4 * k as i64 - 8;
        if q < p as i64 {
            break;
        }
        let q = q as usize;
        let (one, kk) = (u32_at(b, q, false)?, u32_at(b, q + 4, false)?);
        if one == 1 && kk as usize == k {
            let mut hashes = Vec::with_capacity(k);
            for i in 0..k {
                hashes.push(u32_at(b, q + 8 + 4 * i, false)?);
            }
            return Ok((n, p, q, hashes, o));
        }
    }
    merr("no material list (1, k, k hashes) before the packfile")
}

/// Python's `bytes.hex(" ")`.
fn hex_spaced(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")
}

/// `part_materials(b)`: (the material list's hashes, each part's index into it).
///
/// # Errors
/// As [`wrapper`].
pub fn part_materials(b: &[u8]) -> Result<(Vec<u32>, Vec<u8>)> {
    let (n, p, q, hashes, _) = wrapper(b)?;
    let run = b.get(p..q.max(p)).ok_or(Port::Refused)?;
    if n == 0 {
        return Ok((hashes, Vec::new()));
    }
    if run.len() < 4 || run.get(..2) != Some(&[0u8, 0][..]) {
        return merr(format!(
            "the part run opens with a material table ({}): not read here",
            hex_spaced(run.get(..4.min(run.len())).unwrap_or_default())
        ));
    }
    let (distinct, top) = (run.get(2).copied().unwrap_or(0), run.get(3).copied().unwrap_or(0));
    let idx: Vec<u8> = if distinct == 1 {
        vec![top; n]
    } else {
        run.get(4..(4 + n).min(run.len())).unwrap_or_default().to_vec()
    };
    let want = if distinct == 1 { 4 } else { 4 + n };
    let set: HashSet<u8> = idx.iter().copied().collect();
    // the reference's `or` stops at the first true test: max(idx) is reached only with idx non-empty
    let bad = run.len() != want
        || set.len() != usize::from(distinct)
        || idx.iter().max() != Some(&top)
        || usize::from(top) >= hashes.len();
    if bad {
        return merr(format!(
            "the part run {} is not {n} parts' indices into {} materials",
            hex_spaced(run),
            hashes.len()
        ));
    }
    Ok((hashes, idx))
}

/// `part_run(indices)`.
///
/// # Errors
/// The reference's `MaterialError`s.
pub fn part_run(indices: &[i64]) -> Result<Vec<u8>> {
    let (Some(&lo), Some(&hi)) = (indices.iter().min(), indices.iter().max()) else {
        return merr("no parts");
    };
    if hi > 255 || lo < 0 {
        return merr("an index outside 0 - 255");
    }
    let distinct = indices.iter().collect::<HashSet<_>>().len();
    if distinct > 255 {
        return Err(Port::Refused); // bytes([256, ...]) raises ValueError in the reference
    }
    let mut head = vec![0u8, 0, distinct as u8, hi as u8];
    if distinct != 1 {
        head.extend(indices.iter().map(|&i| i as u8));
    }
    Ok(head)
}

/// `with_part_materials(b, indices, hashes)`.
///
/// # Errors
/// As [`part_materials`] and [`part_run`].
pub fn with_part_materials(b: &[u8], indices: &[i64], hashes: &[u32]) -> Result<Vec<u8>> {
    let (n, p, _, old, o) = wrapper(b)?;
    part_materials(b)?;
    if indices.len() != n {
        return merr(format!("{} part materials for {n} parts", indices.len()));
    }
    if hashes.get(..old.len().min(hashes.len())) != Some(&old[..]) {
        return merr("the material list may only grow at its end");
    }
    let Some(&top) = indices.iter().max() else {
        return Err(Port::Refused);
    };
    if top >= hashes.len() as i64 {
        return merr("a part index past the material list");
    }
    let mut out = b.get(..p).ok_or(Port::Refused)?.to_vec();
    out.extend(part_run(indices)?);
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&u32::try_from(hashes.len()).map_err(|_| Port::Refused)?.to_le_bytes());
    for h in hashes {
        out.extend_from_slice(&h.to_le_bytes());
    }
    out.extend_from_slice(b.get(o..).unwrap_or_default());
    Ok(out)
}

fn cstr(b: &[u8], at: usize) -> Result<String> {
    let e = nul_from(b, at)?;
    Ok(latin1(b.get(at..e).unwrap_or_default()))
}

/// `havok_materials(b)`: the packfile's `HavokMaterials` list.
///
/// # Errors
/// [`Port::Refused`] where the reference raises.
pub fn havok_materials(b: &[u8]) -> Result<Vec<String>> {
    let p = find_magic(b).ok_or(Port::Refused)?;
    let secs = pc_sections(b, p)?;
    let d = data_of(b, p, &secs)?;
    let mut lists: Vec<u32> = d
        .kinds
        .iter()
        .filter(|(_, c)| *c == "HavokMaterials")
        .map(|(o, _)| *o)
        .collect();
    lists.sort_unstable();
    let Some(&first) = lists.first() else {
        return Ok(Vec::new());
    };
    let arr = d.local.get(&first).copied();
    let cnt = u32_at(b, d.base + first as usize + 4, false)?;
    let mut out = Vec::new();
    for i in 0..cnt {
        let arr = arr.ok_or(Port::Refused)?;
        let g = add(arr, 4 * i as usize)
            .and_then(|x| d.glob.get(&x))
            .ok_or(Port::Refused)?;
        let l = d.local.get(g).ok_or(Port::Refused)?;
        out.push(cstr(b, d.base + *l as usize)?);
    }
    Ok(out)
}

/// `_fixups(b, start, end, size)`.
fn fixups(b: &[u8], start: usize, end: usize, size: usize) -> Result<Vec<Vec<u32>>> {
    rows(b, start, end, size)
}

fn table_bytes(rows: &[Vec<u32>]) -> Vec<u8> {
    let mut t = Vec::new();
    for r in rows {
        for v in r {
            t.extend_from_slice(&v.to_le_bytes());
        }
    }
    while !t.len().is_multiple_of(16) {
        t.push(0xFF);
    }
    t
}

/// Python's `str.lower()` for Latin-1 text.
fn lower(s: &str) -> String {
    s.to_lowercase()
}

/// `with_havok_materials(b, names)`.
///
/// # Errors
/// The reference's `MaterialError`s; [`Port::Refused`] for its other exceptions.
#[expect(clippy::too_many_lines, reason = "the reference's one function, step for step")]
pub fn with_havok_materials(b: &[u8], names: &[String]) -> Result<Vec<u8>> {
    let p = find_magic(b).ok_or(Port::Refused)?;
    let old = havok_materials(b)?;
    let head_names: Vec<String> = names.iter().take(old.len()).map(|n| lower(n)).collect();
    if head_names != old.iter().map(|n| lower(n)).collect::<Vec<_>>() {
        return merr("the HavokMaterials list may only grow at its end");
    }
    if names.len() == old.len() {
        return Ok(b.to_vec());
    }
    let secs = pc_sections(b, p)?;
    let d = data_of(b, p, &secs)?;
    let lists: Vec<u32> = d
        .kinds
        .iter()
        .filter(|(_, c)| *c == "HavokMaterials")
        .map(|(o, _)| *o)
        .collect();
    if lists.len() != 1 {
        return merr(format!("{} HavokMaterials lists", lists.len()));
    }
    let hm = *lists.first().ok_or(Port::Refused)?;
    let arr = *d.local.get(&hm).ok_or(Port::Refused)?;
    if d.kinds.iter().any(|(o, c)| *o > hm && c != "HavokMaterial") || arr <= hm {
        return merr("the HavokMaterials list does not end the data section: not written here");
    }
    let h = p + 64 + 48 * 2;
    if b.get(h..h + 8) != Some(&b"__data__"[..]) {
        return merr("the data section is not the third");
    }
    let mut v = [0u32; 7];
    for (k, x) in v.iter_mut().enumerate() {
        *x = u32_at(b, h + 20 + 4 * k, false)?;
    }
    let [start, loc, glob, virt, exp, imp, end] = v;
    let base = d.base;
    if base + end as usize != b.len() || !(exp == imp && imp == end) {
        return merr("the data section does not end the file, or has exports / imports");
    }
    let (loc, glob, virt, exp) = (loc as usize, glob as usize, virt as usize, exp as usize);
    let lo = fixups(b, base + loc, base + glob, 8)?;
    let gl = fixups(b, base + glob, base + virt, 12)?;
    let vi = fixups(b, base + virt, base + exp, 12)?;
    let mut mats: Vec<u32> = d
        .kinds
        .iter()
        .filter(|(_, c)| *c == "HavokMaterial")
        .map(|(o, _)| *o)
        .collect();
    mats.sort_unstable();
    let sec = gl
        .iter()
        .find(|r| col(r, 0) == arr)
        .map(|r| col(r, 1))
        .ok_or(Port::Refused)?;
    let m0 = *mats.first().ok_or(Port::Refused)?;
    let vrow = vi.iter().find(|r| col(r, 0) == m0).ok_or(Port::Refused)?.clone();
    let mut old_objs = Vec::new();
    for i in 0..old.len() {
        let o = *add(arr, 4 * i).and_then(|x| d.glob.get(&x)).ok_or(Port::Refused)? as usize;
        // b[base + o:base + o + 16]: a short slice would change the layout - left to the reference
        old_objs.push(slice(b, base + o, 16)?.to_vec());
    }
    let n = names.len();
    let mut region: Vec<u8> = vec![0; 4 * n];
    region.resize(region.len() + (16 - (4 * n) % 16) % 16, 0);
    let (mut objs, mut strs) = (Vec::new(), Vec::new());
    for (i, name) in names.iter().enumerate() {
        let o = arr as usize + region.len();
        match old_objs.get(i) {
            Some(x) => region.extend_from_slice(x),
            None => region.extend_from_slice(&[0; 16]),
        }
        let s = arr as usize + region.len();
        let mut raw: Vec<u8> = Vec::with_capacity(name.len() + 1);
        for c in name.chars() {
            raw.push(u8::try_from(u32::from(c)).map_err(|_| Port::Refused)?);
        }
        raw.push(0);
        let pad = (16 - raw.len() % 16) % 16;
        region.extend_from_slice(&raw);
        region.resize(region.len() + pad, 0);
        objs.push(u32::try_from(o).map_err(|_| Port::Refused)?);
        strs.push(u32::try_from(s).map_err(|_| Port::Refused)?);
    }
    let mut data = slice(b, base, arr as usize)?.to_vec();
    data.extend_from_slice(&region);
    put(
        &mut data,
        hm as usize + 4,
        &u32::try_from(n).map_err(|_| Port::Refused)?.to_le_bytes(),
    )?;
    let mut lo: Vec<Vec<u32>> = lo.into_iter().filter(|r| col(r, 0) < arr).collect();
    lo.extend(objs.iter().zip(&strs).map(|(&o, &s)| vec![o, s]));
    let mut gl: Vec<Vec<u32>> = gl.into_iter().filter(|r| col(r, 0) < arr).collect();
    for (i, &o) in objs.iter().enumerate() {
        gl.push(vec![add(arr, 4 * i).ok_or(Port::Refused)?, sec, o]);
    }
    let mut vi: Vec<Vec<u32>> = vi.into_iter().filter(|r| col(r, 0) < arr).collect();
    vi.extend(objs.iter().map(|&o| vec![o, col(&vrow, 1), col(&vrow, 2)]));
    let (tl, tg, tv) = (table_bytes(&lo), table_bytes(&gl), table_bytes(&vi));
    let loc2 = data.len();
    let glob2 = loc2 + tl.len();
    let virt2 = glob2 + tg.len();
    let end2 = virt2 + tv.len();
    let mut head = b.get(..base).ok_or(Port::Refused)?.to_vec();
    let u = |x: usize| u32::try_from(x).map_err(|_| Port::Refused);
    for (k, x) in [start, u(loc2)?, u(glob2)?, u(virt2)?, u(end2)?, u(end2)?, u(end2)?]
        .iter()
        .enumerate()
    {
        put(&mut head, h + 20 + 4 * k, &x.to_le_bytes())?;
    }
    let mut out = head;
    out.extend_from_slice(&data);
    out.extend_from_slice(&tl);
    out.extend_from_slice(&tg);
    out.extend_from_slice(&tv);
    Ok(out)
}

/// `hkpStorageExtendedMeshShapeMeshSubpartStorage::materials`.
pub const STORAGE_MATERIALS_AT: u32 = 56;
/// `hkpWorldObject::name`.
pub const BODY_NAME_AT: u32 = 112;
/// `collidable.shape`.
pub const BODY_SHAPE_AT: u32 = 16;
/// `hkpMoppBvTreeShape` child.childShape.
pub const MOPP_CHILD_AT: u32 = 52;
/// `hkpStorageExtendedMeshShape::meshstorage`.
pub const MESH_STORAGE_AT: u32 = 192;

/// `_storages(...)`: [(the materials array's offset or `None`, count)] of a body's storage mesh, `None` when its
/// shape is no storage mesh.
/// A subpart storage's materials: (the array's offset, count).
type Storage = (Option<u32>, usize);

fn storages(b: &[u8], d: &Data, body: u32) -> Result<Option<Vec<Storage>>> {
    let mut shape = add(body, BODY_SHAPE_AT as usize).and_then(|x| d.glob.get(&x)).copied();
    if let Some(s) = shape
        && d.kinds.get(&s).map(String::as_str) == Some("hkpMoppBvTreeShape")
    {
        shape = add(s, MOPP_CHILD_AT as usize).and_then(|x| d.glob.get(&x)).copied();
    }
    let Some(shape) = shape.filter(|s| d.kinds.get(s).map(String::as_str) == Some("hkpStorageExtendedMeshShape"))
    else {
        return Ok(None);
    };
    let arr = add(shape, MESH_STORAGE_AT as usize)
        .and_then(|x| d.local.get(&x))
        .copied();
    let cnt = u32_at(b, d.base + shape as usize + MESH_STORAGE_AT as usize + 4, false)?;
    let mut out = Vec::new();
    for i in 0..cnt {
        let arr = arr.ok_or(Port::Refused)?;
        let st = *add(arr, 4 * i as usize)
            .and_then(|x| d.glob.get(&x))
            .ok_or(Port::Refused)?;
        let k = u32_at(b, d.base + st as usize + STORAGE_MATERIALS_AT as usize + 4, false)? as usize;
        out.push((
            add(st, STORAGE_MATERIALS_AT as usize)
                .and_then(|x| d.local.get(&x))
                .copied(),
            k,
        ));
    }
    Ok(Some(out))
}

/// A body as `bodies` lists it: its name and its subpart storages' face materials (`None`: no storage mesh).
pub type Body = (String, Option<Vec<Vec<u32>>>);

/// `bodies(b)`.
///
/// # Errors
/// [`Port::Refused`] where the reference raises.
pub fn bodies(b: &[u8]) -> Result<Vec<Body>> {
    let p = find_magic(b).ok_or(Port::Refused)?;
    let secs = pc_sections(b, p)?;
    let d = data_of(b, p, &secs)?;
    let mut order: Vec<u32> = d
        .kinds
        .iter()
        .filter(|(_, c)| *c == "hkpRigidBody")
        .map(|(o, _)| *o)
        .collect();
    order.sort_unstable();
    let mut out = Vec::new();
    for o in order {
        let name = match add(o, BODY_NAME_AT as usize).and_then(|x| d.local.get(&x)) {
            Some(&l) => cstr(b, d.base + l as usize)?,
            None => String::new(),
        };
        let st = storages(b, &d, o)?;
        let mats = match st {
            None => None,
            Some(st) => {
                let mut v = Vec::new();
                for (m, k) in st {
                    if k == 0 {
                        v.push(Vec::new());
                        continue;
                    }
                    let m = m.ok_or(Port::Refused)? as usize;
                    let mut row = Vec::with_capacity(k);
                    for i in 0..k {
                        row.push(u32_at(b, d.base + m + 4 * i, false)?);
                    }
                    v.push(row);
                }
                Some(v)
            }
        };
        out.push((name, mats));
    }
    Ok(out)
}

/// `with_face_materials(b, changes)`: changes in the dict's order.
///
/// # Errors
/// The reference's `MaterialError`s; [`Port::Refused`] for its other exceptions.
pub fn with_face_materials(b: &[u8], changes: &[(i64, Vec<Vec<i64>>)]) -> Result<Vec<u8>> {
    let p = find_magic(b).ok_or(Port::Refused)?;
    let secs = pc_sections(b, p)?;
    let d = data_of(b, p, &secs)?;
    let mut out = b.to_vec();
    let n = havok_materials(b)?.len() as i64;
    let mut order: Vec<u32> = d
        .kinds
        .iter()
        .filter(|(_, c)| *c == "hkpRigidBody")
        .map(|(o, _)| *o)
        .collect();
    order.sort_unstable();
    for (bi, new) in changes {
        // order[bi]: a negative index counts from the end in the reference
        let idx = if *bi < 0 { order.len() as i64 + bi } else { *bi };
        let body = *usize::try_from(idx)
            .ok()
            .and_then(|i| order.get(i))
            .ok_or(Port::Refused)?;
        let Some(st) = storages(b, &d, body)? else {
            return merr(format!("body {bi}: not a storage mesh"));
        };
        if new.len() != st.len() {
            return merr(format!(
                "body {bi}: {} subpart lists for {} subpart storages",
                new.len(),
                st.len()
            ));
        }
        for (i, ((m, k), vals)) in st.iter().zip(new).enumerate() {
            if vals.len() != *k {
                return merr(format!("body {bi} subpart {i}: {} materials for {k}", vals.len()));
            }
            if vals.iter().any(|&x| !(0 <= x && x < n)) {
                return merr(format!("body {bi} subpart {i}: an index past the {n} HavokMaterials"));
            }
            if *k > 0 {
                let m = m.ok_or(Port::Refused)? as usize;
                for (j, &x) in vals.iter().enumerate() {
                    put(&mut out, d.base + m + 4 * j, &(x as u32).to_le_bytes())?;
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_runs() {
        assert_eq!(part_run(&[2, 2, 2]), Ok(vec![0, 0, 1, 2]));
        assert_eq!(part_run(&[0, 1, 0]), Ok(vec![0, 0, 2, 1, 0, 1, 0]));
        assert_eq!(part_run(&[]), Err(Port::Material("no parts".into())));
        assert_eq!(part_run(&[256]), Err(Port::Material("an index outside 0 - 255".into())));
    }

    #[test]
    fn nearest_byte() {
        let t: Vec<f64> = (0..256).map(f64::from).collect();
        assert_eq!(ufloat8(200.4, &t), 200);
        assert_eq!(ufloat8(200.5, &t), 200);
        assert_eq!(ufloat8(f64::NAN, &t), 0);
    }

    #[test]
    fn w_lanes() {
        assert_eq!(subpart_w_lanes([0.0, 0.0, 0.0, 1.0]), [0.0, 0.0, 0.0]);
        let parts = vec![([0.0, 0.0, 0.0, 1.0], vec![([1.0, 2.0, 3.0], [1.0, 1.0, 1.0], 0.5)])];
        assert_eq!(aabb_w(&parts), Some((1.0, 0.5)));
        assert_eq!(aabb_w(&[]), None);
    }

    #[test]
    fn refusals() {
        assert_eq!(
            restructure(b"nothing", b"", &Layouts::default(), None),
            Err(Port::Error("not a Havok 4.6.1 packfile".into()))
        );
        assert_eq!(wrapper(b"x"), Err(Port::Material("no Havok packfile".into())));
    }
}
