//! `bfbc2-menu-tools\aptgraft.py`'s movie API and grafts in Rust (round O3), over the movie model of
//! `bc2_formats::aptmodel` (round E4c): [`Movie`] is `aptgraft.Movie` (its members, label and constant index around a
//! [`Model`]), [`Grafter`] / [`Builder`] / `code_range` copy characters and code between movies or build code, and
//! [`edit`] / [`crosshair`] hold the commands' edits (`mode-case`, `mode-alias`, `label-alias`, `deps`,
//! `onslaught-timer`, `crosshair-add`).
//!
//! The model's node numbers stand for the Python objects: an instruction is its index in its code node, a list of
//! instructions a stream index ([`Code::streams`]), so `is` / `id()` comparisons of the reference are comparisons of
//! numbers here. Every `AptError` the reference raises is a [`Fail::Err`] with its words; wherever the reference
//! would raise another exception (an index or attribute error, `struct.error`, a value its writer cannot pack) or the
//! model refuses, [`Fail::Python`] hands the whole command line to `aptgraft.py`.

#![allow(
    clippy::indexing_slicing,
    reason = "each index is checked just before it, as the reference indexes"
)]
#![allow(
    clippy::many_single_char_names,
    clippy::type_complexity,
    clippy::single_match_else,
    clippy::doc_markdown,
    reason = "the reference's names (m, g, c, k, i, n ...), its tuples and its words (opcode names) kept for review \
              against aptgraft.py"
)]

pub mod crosshair;
pub mod edit;

use std::collections::HashMap;

use bc2_formats::aptmodel::{self, Arg, Code, Content, Ins, Layout, Model, Node, Rid, Target, Val};
use bc2_pyrt::err::{Fail, PyErr, PyResult};

/// `aptgraft.SIG`.
pub const SIG: u32 = 0x0987_6543;
/// `FN_MAGIC`.
pub const FN_MAGIC: (u32, u32) = (0x9876_5432, 0x1234_5678);
/// The files.
pub const APT: u8 = 0;
/// `.const`.
pub const CONST: u8 = 1;
/// `.geo`.
pub const GEO: u8 = 2;

/// `BRANCH`.
pub fn is_branch(op: u8) -> bool {
    matches!(op, 0x99 | 0x9D | 0xB8)
}

/// `ONE_STR`.
pub fn is_one_str(op: u8) -> bool {
    matches!(op, 0x8B | 0x8C | 0xA1 | 0xA4 | 0xA5 | 0xA6 | 0xA7)
}

/// `POOL_BYTE`.
pub fn is_pool_byte(op: u8) -> bool {
    matches!(op, 0xA2 | 0xAE | 0xAF | 0xB0 | 0xB1 | 0xB2 | 0xB3)
}

/// `AptError(msg)`.
pub fn apt_err(msg: impl Into<String>) -> Fail {
    Fail::Err(PyErr::new("AptError", msg))
}

/// `apt.CHAR_TYPES.get(t, str(t))`.
pub fn char_type_name(t: u32) -> String {
    match t {
        1 => "shape",
        2 => "text",
        3 => "font",
        4 => "button",
        5 => "sprite",
        6 => "sound",
        7 => "image",
        8 => "morph",
        9 => "movie",
        10 => "statictext",
        11 => "none",
        12 => "video",
        _ => return t.to_string(),
    }
    .to_owned()
}

/// `Ins.name`: `apt.NAMES.get(op, f'op_{op:02X}')`.
pub fn op_display(op: u8) -> String {
    bc2_formats::apt::display_name(op)
}

/// `OPS[name]`.
pub fn op_named(name: &str) -> Option<u8> {
    (0..=255u8).find(|&op| bc2_formats::apt::op_name(op) == Some(name))
}

/// `signed(v)`.
pub fn signed(v: u32) -> i64 {
    i64::from(v as i32)
}

/// Text as `bytes.decode('latin1')`.
pub fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| char::from(c)).collect()
}

/// `s.encode('latin1')`, its `UnicodeEncodeError` as Python words it.
pub fn latin1_encode(s: &str) -> PyResult<Vec<u8>> {
    let chars: Vec<char> = s.chars().collect();
    if let Some(i) = chars.iter().position(|&c| u32::from(c) > 0xFF) {
        let mut j = i;
        while j < chars.len() && u32::from(chars[j]) > 0xFF {
            j += 1;
        }
        let esc = |c: char| {
            let v = u32::from(c);
            if v > 0xFFFF {
                format!("\\U{v:08x}")
            } else {
                format!("\\u{v:04x}")
            }
        };
        let msg = if j - i == 1 {
            format!(
                "'latin-1' codec can't encode character '{}' in position {i}: ordinal not in range(256)",
                esc(chars[i])
            )
        } else {
            format!(
                "'latin-1' codec can't encode characters in position {i}-{}: ordinal not in range(256)",
                j - 1
            )
        };
        return Err(Fail::Err(PyErr::new("UnicodeEncodeError", msg)));
    }
    Ok(chars.iter().map(|&c| c as u8).collect())
}

/// `f32(x)`: the bits of `struct.pack('<f', x)` (its `OverflowError`: handed to the Python).
pub fn f32_bits(x: f64) -> PyResult<u32> {
    let f = x as f32;
    if x.is_finite() && !f.is_finite() {
        return Err(Fail::Python);
    }
    Ok(f.to_bits())
}

/// `_fl(v)`: a field's float.
pub fn fl(v: u32) -> f64 {
    f64::from(f32::from_bits(v))
}

/// A constant of the `.const` table as `Movie.constant` keys it: `('s', text)` or `(type, value)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CKey {
    /// A string.
    S(Vec<u8>),
    /// Another entry: its type and value.
    T(u32, u32),
}

/// A movie's BIGF members, `[(name, bytes)]` in index order.
pub type Members = Vec<(String, Vec<u8>)>;

/// `read_bigf_bytes(b, where)`.
pub fn read_bigf(b: &[u8], place: &str) -> PyResult<Members> {
    if b.get(..4) != Some(b"BIGF".as_slice()) {
        return Err(apt_err(format!("{place} is not a menu movie (.res, a BIGF archive)")));
    }
    let word = |at: usize| -> PyResult<usize> {
        b.get(at..at + 4)
            .and_then(|s| s.try_into().ok())
            .map(|s: [u8; 4]| u32::from_be_bytes(s) as usize)
            .ok_or(Fail::Python)
    };
    let n = word(8)?;
    let mut p = 16;
    let mut out = Vec::new();
    for _ in 0..n {
        let (off, size) = (word(p)?, word(p + 4)?);
        let rest = b.get(p + 8..).ok_or(Fail::Python)?;
        let e = rest.iter().position(|&c| c == 0).ok_or(Fail::Python)?;
        let a = off.min(b.len());
        let z = off.saturating_add(size).min(b.len()).max(a);
        out.push((latin1(&rest[..e]), b[a..z].to_vec()));
        p += 8 + e + 1;
    }
    Ok(out)
}

/// `write_bigf(members)`.
pub fn write_bigf(members: &Members) -> PyResult<Vec<u8>> {
    aptmodel::write_bigf(members).map_err(|_| Fail::Python)
}

/// A layout per member file: `'keep'`, `'pack'`, or `GRAFT_LAYOUT` (`.apt` / `.geo` keep, `.const` pack).
pub const GRAFT_LAYOUT: [Layout; 3] = [Layout::Keep, Layout::Pack, Layout::Keep];

/// `aptgraft.Movie`.
#[derive(Debug, Clone)]
pub struct Movie {
    /// The model.
    pub m: Model,
    /// `Movie.label`.
    pub label: String,
    members: Members,
    files: Vec<i8>,
    /// `Movie.big`.
    pub big: bool,
    cidx: Option<HashMap<CKey, usize>>,
}

/// A frame item's or a node's shape where the reference would raise its own exception.
fn py<T>(v: Option<T>) -> PyResult<T> {
    v.ok_or(Fail::Python)
}

impl Movie {
    /// `Movie(members, label)`.
    pub fn new(members: Members, label: &str) -> PyResult<Self> {
        // dict(members): a repeated name takes its last bytes - left to the Python
        let mut seen = std::collections::HashSet::new();
        if !members.iter().all(|(n, _)| seen.insert(n.clone())) {
            return Err(Fail::Python);
        }
        let Some(apt) = members.iter().position(|(n, _)| n.to_lowercase().ends_with(".apt")) else {
            return Err(apt_err(format!("{label}: no .apt member")));
        };
        let an: Vec<char> = members[apt].0.chars().collect();
        let name: String = an[..an.len() - 4].iter().collect();
        let find = |ext: &str| {
            let want = format!("{name}.{ext}").to_lowercase();
            members.iter().position(|(n, _)| n.to_lowercase() == want)
        };
        let Some(c) = find("const") else {
            return Err(apt_err(format!("{label}: no {name}.const member")));
        };
        let g = find("geo");
        let files: Vec<i8> = (0..members.len())
            .map(|k| {
                if k == apt {
                    0
                } else if k == c {
                    1
                } else if Some(k) == g {
                    2
                } else {
                    -1
                }
            })
            .collect();
        let cb = &members[c].1;
        if !cb.starts_with(b"Apt constant file") {
            return Err(apt_err(format!("{label}: not an APT constant file")));
        }
        if cb.len() < 24 {
            return Err(Fail::Python); // struct.error
        }
        let a = &members[apt].1;
        let big = aptmodel::movie_big(a, cb).ok_or(Fail::Python)?;
        let m = Model::parse(a, cb, g.map(|k| members[k].1.as_slice()), big).map_err(|_| Fail::Python)?;
        Ok(Self {
            m,
            label: label.to_owned(),
            members,
            files,
            big,
            cidx: None,
        })
    }

    // ------------------------------------------------------------------------------------------- nodes

    /// A node.
    pub fn node(&self, rid: Rid) -> PyResult<&Node> {
        py(self.m.nodes.get(rid as usize))
    }

    /// A Words node's fields.
    pub fn vals(&self, rid: Rid) -> PyResult<&Vec<Val>> {
        match &self.node(rid)?.content {
            Content::Words(v) => Ok(v),
            _ => Err(Fail::Python),
        }
    }

    /// A Words node's fields, to change.
    pub fn vals_mut(&mut self, rid: Rid) -> PyResult<&mut Vec<Val>> {
        match self.m.nodes.get_mut(rid as usize).map(|n| &mut n.content) {
            Some(Content::Words(v)) => Ok(v),
            _ => Err(Fail::Python),
        }
    }

    /// Field `k` of a Words node.
    pub fn val(&self, rid: Rid, k: usize) -> PyResult<Val> {
        py(self.vals(rid)?.get(k).copied())
    }

    /// Field `k` as a number (a pointer there: the reference's own exception).
    pub fn num(&self, rid: Rid, k: usize) -> PyResult<u32> {
        match self.val(rid, k)? {
            Val::Num(v) => Ok(v),
            Val::Ref(_) => Err(Fail::Python),
        }
    }

    /// Sets field `k`.
    pub fn set(&mut self, rid: Rid, k: usize, v: Val) -> PyResult<()> {
        *py(self.vals_mut(rid)?.get_mut(k))? = v;
        Ok(())
    }

    /// A Str node's text.
    pub fn str_text(&self, rid: Rid) -> PyResult<&[u8]> {
        match &self.node(rid)?.content {
            Content::Str(t) => Ok(t),
            _ => Err(Fail::Python),
        }
    }

    /// `text(v)`: the text a field points at, `None` for a number.
    pub fn text(&self, v: Val) -> PyResult<Option<String>> {
        match v {
            Val::Ref(r) => Ok(Some(latin1(self.str_text(r)?))),
            Val::Num(_) => Ok(None),
        }
    }

    /// A node's tag.
    pub fn tag(&self, rid: Rid) -> PyResult<&str> {
        let t = self.node(rid)?.tag;
        py(self.m.tags.get(usize::from(t)).map(String::as_str))
    }

    /// A Code node.
    pub fn code(&self, rid: Rid) -> PyResult<&Code> {
        match &self.node(rid)?.content {
            Content::Code(c) => Ok(c),
            _ => Err(Fail::Python),
        }
    }

    /// A Code node, to change.
    pub fn code_mut(&mut self, rid: Rid) -> PyResult<&mut Code> {
        match self.m.nodes.get_mut(rid as usize).map(|n| &mut n.content) {
            Some(Content::Code(c)) => Ok(c),
            _ => Err(Fail::Python),
        }
    }

    /// A new node (`Words(file, None, vals, tag)` ...).
    pub fn add_node(&mut self, file: u8, tag: &str, content: Content) -> Rid {
        let tag = self.m.tag(tag);
        let rid = self.m.new_node();
        self.m.nodes[rid as usize] = Node {
            file,
            at: None,
            size0: None,
            tag,
            content,
        };
        rid
    }

    /// `Words(file, None, vals, tag)`.
    pub fn new_words(&mut self, file: u8, vals: Vec<Val>, tag: &str) -> Rid {
        self.add_node(file, tag, Content::Words(vals))
    }

    /// `Str(file, None, text)`.
    pub fn new_str(&mut self, file: u8, text: Vec<u8>) -> Rid {
        self.add_node(file, "str", Content::Str(text))
    }

    /// `Code('apt', None, ins)`.
    pub fn new_code(&mut self, code: Code) -> Rid {
        self.add_node(APT, "code", Content::Code(code))
    }

    // ------------------------------------------------------------------------------------------- structure

    /// `Movie.movie`.
    pub fn movie(&self) -> Rid {
        self.m.heads.movie
    }

    fn ref_at(&self, rid: Rid, k: usize) -> PyResult<Rid> {
        match self.val(rid, k)? {
            Val::Ref(r) => Ok(r),
            Val::Num(_) => Err(Fail::Python),
        }
    }

    /// `Movie.chartable`.
    pub fn chartable(&self) -> PyResult<Rid> {
        self.ref_at(self.movie(), 6)
    }

    /// `len(chartable.vals)`.
    pub fn char_count(&self) -> PyResult<usize> {
        Ok(self.vals(self.chartable()?)?.len())
    }

    /// `Movie.char(cid)`.
    pub fn char(&self, cid: i64) -> PyResult<Option<Rid>> {
        let t = self.vals(self.chartable()?)?;
        Ok(match usize::try_from(cid).ok().and_then(|c| t.get(c)) {
            Some(Val::Ref(r)) => Some(*r),
            _ => None,
        })
    }

    /// `Movie.char_type(cid)`.
    pub fn char_type(&self, cid: i64) -> PyResult<Option<u32>> {
        match self.char(cid)? {
            Some(n) => self.num(n, 0).map(Some),
            None => Ok(None),
        }
    }

    /// `Movie.add_char(node)`.
    pub fn add_char(&mut self, node: Option<Rid>) -> PyResult<i64> {
        let t = self.chartable()?;
        let v = self.vals_mut(t)?;
        v.push(node.map_or(Val::Num(0), Val::Ref));
        let n = v.len();
        let mv = self.movie();
        self.set(mv, 5, Val::Num(n as u32))?;
        Ok(n as i64 - 1)
    }

    /// `Movie.imports()`: (movie, name, character).
    pub fn imports(&self) -> PyResult<Vec<(Option<String>, Option<String>, Val)>> {
        let Val::Ref(r) = self.val(self.movie(), 11)? else {
            return Ok(Vec::new());
        };
        let v = self.vals(r)?;
        let mut out = Vec::new();
        let mut k = 0;
        while k < v.len() {
            out.push((self.text(v[k])?, self.text(*py(v.get(k + 1))?)?, *py(v.get(k + 2))?));
            k += 4;
        }
        Ok(out)
    }

    /// `Movie.add_import(movie, name)`.
    pub fn add_import(&mut self, movie: &str, name: &str) -> PyResult<i64> {
        let cid = self.add_char(None)?;
        let mv = self.movie();
        let node = match self.val(mv, 11)? {
            Val::Ref(r) => r,
            Val::Num(_) => {
                let n = self.new_words(APT, Vec::new(), "imports");
                self.set(mv, 11, Val::Ref(n))?;
                n
            }
        };
        let (a, b) = (latin1_encode(movie)?, latin1_encode(name)?);
        let sa = self.new_str(APT, a);
        let sb = self.new_str(APT, b);
        let v = self.vals_mut(node)?;
        v.extend([Val::Ref(sa), Val::Ref(sb), Val::Num(cid as u32), Val::Num(0)]);
        let n = v.len() / 4;
        self.set(mv, 10, Val::Num(n as u32))?;
        Ok(cid)
    }

    /// `Movie.font_name(cid)`.
    pub fn font_name(&self, cid: i64) -> PyResult<Option<String>> {
        match self.char(cid)? {
            Some(n) if self.val(n, 0)? == Val::Num(3) => self.text(self.val(n, 2)?),
            _ => Ok(None),
        }
    }

    fn owner(&self, owner: Option<Rid>) -> Rid {
        owner.unwrap_or_else(|| self.movie())
    }

    /// `Movie.frames(owner, create)`: `None` for the detached empty node the reference gives without `create`.
    pub fn frames(&mut self, owner: Option<Rid>, create: bool) -> PyResult<Option<Rid>> {
        let o = self.owner(owner);
        match self.val(o, 3)? {
            Val::Ref(r) => Ok(Some(r)),
            Val::Num(_) if !create => Ok(None),
            Val::Num(_) => {
                let n = self.new_words(APT, Vec::new(), "frames");
                self.set(o, 3, Val::Ref(n))?;
                Ok(Some(n))
            }
        }
    }

    /// `len(frames(owner).vals) // 2`.
    pub fn frame_count(&self, owner: Option<Rid>) -> PyResult<usize> {
        match self.val(self.owner(owner), 3)? {
            Val::Ref(r) => Ok(self.vals(r)?.len() / 2),
            Val::Num(_) => Ok(0),
        }
    }

    /// `Movie.items(frame, owner, create)`.
    pub fn items(&mut self, frame: usize, owner: Option<Rid>, create: bool) -> PyResult<Option<Rid>> {
        let fr = self.frames(owner, create)?;
        let len = match fr {
            Some(f) => self.vals(f)?.len(),
            None => 0,
        };
        if 2 * frame + 1 >= len {
            if !create {
                return Ok(None);
            }
            let f = py(fr)?;
            let v = self.vals_mut(f)?;
            let add = frame + 1 - v.len() / 2;
            for _ in 0..add {
                v.extend([Val::Num(0), Val::Num(0)]);
            }
            let n = v.len() / 2;
            let o = self.owner(owner);
            self.set(o, 2, Val::Num(n as u32))?;
        }
        let f = py(fr)?;
        let v = self.val(f, 2 * frame + 1)?;
        let fresh = match v {
            Val::Num(_) => true,
            Val::Ref(r) => create && self.vals(r)?.is_empty() && self.node(r)?.at.is_some(),
        };
        if fresh {
            if !create {
                return Ok(None);
            }
            let n = self.new_words(APT, Vec::new(), "items");
            self.set(f, 2 * frame + 1, Val::Ref(n))?;
            return Ok(Some(n));
        }
        match v {
            Val::Ref(r) => Ok(Some(r)),
            Val::Num(_) => Err(Fail::Python),
        }
    }

    /// `Movie.add_item(frame, item, owner, index)`.
    pub fn add_item(&mut self, frame: usize, item: Rid, owner: Option<Rid>, index: Option<usize>) -> PyResult<()> {
        let lst = py(self.items(frame, owner, true)?)?;
        let v = self.vals_mut(lst)?;
        let at = index.map_or(v.len(), |i| i.min(v.len()));
        v.insert(at, Val::Ref(item));
        let n = v.len();
        let f = py(self.frames(owner, false)?)?;
        self.set(f, 2 * frame, Val::Num(n as u32))
    }

    /// `Movie.frame_items(frame, owner)`.
    pub fn frame_items(&mut self, frame: usize, owner: Option<Rid>) -> PyResult<Vec<Rid>> {
        match self.items(frame, owner, false)? {
            Some(l) => self
                .vals(l)?
                .iter()
                .map(|v| match v {
                    Val::Ref(r) => Ok(*r),
                    Val::Num(_) => Err(Fail::Python),
                })
                .collect(),
            None => Ok(Vec::new()),
        }
    }

    /// `Movie.timeline_depths(owner)`.
    pub fn timeline_depths(&mut self, owner: Option<Rid>) -> PyResult<std::collections::HashSet<i64>> {
        let mut out = std::collections::HashSet::new();
        for f in 0..self.frame_count(owner)? {
            for it in self.frame_items(f, owner)? {
                match self.tag(it)? {
                    "place" => {
                        out.insert(signed(self.num(it, 2)?));
                    }
                    "remove" => {
                        out.insert(signed(self.num(it, 1)?));
                    }
                    _ => {}
                }
            }
        }
        Ok(out)
    }

    /// `Movie.label_frame(label, owner)`.
    pub fn label_frame(&mut self, label: &str, owner: Option<Rid>) -> PyResult<usize> {
        for f in 0..self.frame_count(owner)? {
            for it in self.frame_items(f, owner)? {
                if self.tag(it)? == "label" && self.text(self.val(it, 1)?)?.as_deref() == Some(label) {
                    return Ok(f);
                }
            }
        }
        Err(apt_err(format!("{}: no frame label \"{label}\"", self.label)))
    }

    // ------------------------------------------------------------------------------------------- constants

    /// `Movie.const_entries`.
    pub fn const_entries(&self) -> PyResult<Rid> {
        self.ref_at(self.m.heads.const_head, 2)
    }

    /// `Movie.constant(k)`.
    pub fn constant(&self, k: usize) -> PyResult<CKey> {
        let v = self.vals(self.const_entries()?)?;
        let (t, x) = (*py(v.get(2 * k))?, *py(v.get(2 * k + 1))?);
        match (t, x) {
            (Val::Num(1), Val::Ref(r)) => Ok(CKey::S(self.str_text(r)?.to_vec())),
            (Val::Num(t), Val::Num(x)) => Ok(CKey::T(t, x)),
            _ => Err(Fail::Python),
        }
    }

    /// `Movie.const_index(key)` (added at the end when missing).
    pub fn const_index(&mut self, key: &CKey) -> PyResult<usize> {
        if self.cidx.is_none() {
            let n = self.vals(self.const_entries()?)?.len() / 2;
            let mut idx = HashMap::new();
            for k in 0..n {
                idx.entry(self.constant(k)?).or_insert(k);
            }
            self.cidx = Some(idx);
        }
        if let Some(&k) = self.cidx.as_ref().and_then(|i| i.get(key)) {
            return Ok(k);
        }
        let e = self.const_entries()?;
        let add = match key {
            CKey::S(t) => {
                let s = self.new_str(CONST, t.clone());
                [Val::Num(1), Val::Ref(s)]
            }
            CKey::T(t, x) => [Val::Num(*t), Val::Num(*x)],
        };
        let v = self.vals_mut(e)?;
        v.extend(add);
        let k = v.len() / 2 - 1;
        let h = self.m.heads.const_head;
        self.set(h, 1, Val::Num(k as u32 + 1))?;
        if let Some(i) = self.cidx.as_mut() {
            i.insert(key.clone(), k);
        }
        Ok(k)
    }

    /// `Movie.const_text(k)`.
    pub fn const_text(&self, k: usize) -> PyResult<String> {
        Ok(match self.constant(k)? {
            CKey::S(t) => latin1(&t),
            CKey::T(4, x) => format!("reg{x}"),
            CKey::T(t, x) => format!("type{t}:{x}"),
        })
    }

    /// `const_text` of a field (a pool list's entry: a number).
    pub fn const_text_of(&self, v: Val) -> PyResult<String> {
        match v {
            Val::Num(k) => self.const_text(k as usize),
            Val::Ref(_) => Err(Fail::Python),
        }
    }

    // ------------------------------------------------------------------------------------------- geometry

    /// `Movie.geo_entry(gid)`: (table node, field index).
    pub fn geo_entry(&self, gid: i64) -> PyResult<Option<(Rid, usize)>> {
        let Some(gh) = self.m.heads.geo_head else {
            return Ok(None);
        };
        let Val::Ref(t) = self.val(gh, 1)? else {
            return Ok(None);
        };
        let v = self.vals(t)?;
        let mut k = 0;
        while k < v.len() {
            if v[k] == Val::Num(gid as u32) && gid >= 0 && gid <= i64::from(u32::MAX) {
                return Ok(Some((t, k)));
            }
            k += 3;
        }
        Ok(None)
    }

    /// `Movie.add_geometry(gid, parts_node)`.
    pub fn add_geometry(&mut self, gid: i64, parts: Rid) -> PyResult<()> {
        let Some(gh) = self.m.heads.geo_head else {
            return Err(apt_err(format!("{}: no .geo member to add geometry to", self.label)));
        };
        let t = match self.val(gh, 1)? {
            Val::Ref(r) => r,
            Val::Num(_) => {
                let n = self.new_words(GEO, Vec::new(), "geo.table");
                self.set(gh, 1, Val::Ref(n))?;
                n
            }
        };
        let np = self.vals(parts)?.len();
        let v = self.vals_mut(t)?;
        v.extend([Val::Num(gid as u32), Val::Num(np as u32), Val::Ref(parts)]);
        let n = v.len() / 3;
        self.set(gh, 0, Val::Num(n as u32))
    }

    /// `code_nodes(movie, new_only)`: the reachable `.apt` Code nodes.
    pub fn code_nodes(&self, new_only: bool) -> PyResult<Vec<Rid>> {
        let mut out = Vec::new();
        for (f, lst) in self.m.reachable() {
            if f != APT {
                continue;
            }
            for r in lst {
                let n = self.node(r)?;
                if matches!(n.content, Content::Code(_)) && (n.at.is_none() || !new_only) {
                    out.push(r);
                }
            }
        }
        Ok(out)
    }

    /// `Node.refs()`.
    pub fn refs(&self, rid: Rid) -> PyResult<Vec<Rid>> {
        Ok(match &self.node(rid)?.content {
            Content::Words(v) => v
                .iter()
                .filter_map(|x| match x {
                    Val::Ref(r) => Some(*r),
                    Val::Num(_) => None,
                })
                .collect(),
            Content::Code(c) => walk_ins(c, 0)
                .into_iter()
                .flat_map(|k| {
                    c.ins[k as usize].args.iter().filter_map(|a| match a {
                        Arg::Ref(r) => Some(*r),
                        _ => None,
                    })
                })
                .collect(),
            _ => Vec::new(),
        })
    }

    // ------------------------------------------------------------------------------------------- writing

    /// `Movie.write(order, layout)`: the members in their order (`big`: the byte order).
    pub fn write(&self, big: bool, layouts: [Layout; 3], dead: bool) -> PyResult<Members> {
        let w = self.m.write(big, layouts, 0, dead).map_err(|_| Fail::Python)?;
        self.members
            .iter()
            .zip(&self.files)
            .map(|((n, d), &f)| {
                if f < 0 {
                    Ok((n.clone(), d.clone()))
                } else {
                    match w.get(f as usize) {
                        Some(Some(b)) => Ok((n.clone(), b.clone())),
                        _ => Err(Fail::Python),
                    }
                }
            })
            .collect()
    }

    /// `Movie.to_res(order, layout)`.
    pub fn to_res(&self, big: bool, layouts: [Layout; 3]) -> PyResult<Vec<u8>> {
        write_bigf(&self.write(big, layouts, true)?)
    }
}

/// `walk_ins(lst)`: every instruction of a stream, function bodies included (depth first).
pub fn walk_ins(c: &Code, s: u32) -> Vec<u32> {
    let mut out = Vec::new();
    walk_into(c, s, &mut out, 0);
    out
}

fn walk_into(c: &Code, s: u32, out: &mut Vec<u32>, depth: usize) {
    if depth > 4 * aptmodel::MAX_NESTING {
        return;
    }
    for &k in c.streams.get(s as usize).map_or(&[][..], Vec::as_slice) {
        out.push(k);
        if let Some(b) = c.ins.get(k as usize).and_then(|i| i.body) {
            walk_into(c, b, out, depth + 1);
        }
    }
}

/// A new instruction (`Ins(op, args)`).
pub fn ins(op: u8, args: Vec<Arg>) -> Ins {
    Ins {
        op,
        args,
        body: None,
        target: Target::Unset,
        at: None,
    }
}

/// Adds an instruction to a code node (not to any stream): its index.
pub fn push_ins(c: &mut Code, i: Ins) -> u32 {
    c.ins.push(i);
    (c.ins.len() - 1) as u32
}

/// An instruction's integer argument `k` (anything else: the reference's own exception).
pub fn int_arg(i: &Ins, k: usize) -> PyResult<i64> {
    match i.args.get(k) {
        Some(Arg::Int(v)) => Ok(*v),
        _ => Err(Fail::Python),
    }
}

/// `int(text, 0)` for the forms the command lines take (anything else: the Python answers).
pub fn py_int0(s: &str) -> PyResult<i64> {
    let t = s.trim_matches(|c: char| c.is_ascii_whitespace());
    let (neg, body) = match t.strip_prefix('-') {
        Some(b) => (true, b),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let lower = body.to_ascii_lowercase();
    let (radix, digits) = if let Some(d) = lower.strip_prefix("0x") {
        (16, d)
    } else if let Some(d) = lower.strip_prefix("0o") {
        (8, d)
    } else if let Some(d) = lower.strip_prefix("0b") {
        (2, d)
    } else {
        if lower.len() > 1 && lower.starts_with('0') && !lower.chars().all(|c| c == '0') {
            return Err(Fail::Python);
        }
        (10, lower.as_str())
    };
    if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
        return Err(Fail::Python);
    }
    let v = i64::from_str_radix(digits, radix).map_err(|_| Fail::Python)?;
    Ok(if neg { -v } else { v })
}

/// `float(text)` for the plain forms (anything else: the Python answers).
pub fn py_float(s: &str) -> PyResult<f64> {
    let t = s.trim_matches(|c: char| c.is_ascii_whitespace());
    if t.is_empty() || t.contains('_') {
        return Err(Fail::Python);
    }
    t.parse::<f64>().map_err(|_| Fail::Python)
}

// ------------------------------------------------------------------------------------------------ grafting

/// `aptgraft.Grafter`: copies characters, items and code from a source movie into a target.
#[derive(Debug, Default, Clone)]
pub struct Grafter {
    /// src character id -> dst id.
    pub cmap: HashMap<i64, i64>,
    /// The order the ids were mapped in (a dict's).
    pub cmap_order: Vec<i64>,
    /// src node -> its copy.
    pub memo: HashMap<Rid, Rid>,
    /// What the copy needs that it could not bring.
    pub notes: Vec<String>,
    /// (dst id, kind, src id).
    pub added: Vec<(i64, String, i64)>,
    cmap_const: HashMap<usize, usize>,
}

impl Grafter {
    /// A new grafter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Maps a character id (`self.cmap[cid] = new`, kept in insertion order).
    pub fn map(&mut self, cid: i64, new: i64) {
        if self.cmap.insert(cid, new).is_none() {
            self.cmap_order.push(cid);
        }
    }

    /// `Grafter.const(k)`.
    pub fn konst(&mut self, src: &Movie, dst: &mut Movie, k: usize) -> PyResult<usize> {
        if let Some(&v) = self.cmap_const.get(&k) {
            return Ok(v);
        }
        if k >= src.vals(src.const_entries()?)?.len() / 2 {
            return Err(apt_err(format!(
                "{}: constant index {k} past the constant table",
                src.label
            )));
        }
        let v = dst.const_index(&src.constant(k)?)?;
        self.cmap_const.insert(k, v);
        Ok(v)
    }

    /// `Grafter.char(cid)` of a Python int (`signed()` of it as the reference takes it).
    pub fn char_py(&mut self, src: &Movie, dst: &mut Movie, cid: i64) -> PyResult<i64> {
        let c = if (cid >> 31) & 1 != 0 { cid - (1 << 32) } else { cid };
        self.char_signed(src, dst, c)
    }

    /// `Grafter.char(cid)`.
    pub fn char(&mut self, src: &Movie, dst: &mut Movie, cid: u32) -> PyResult<i64> {
        self.char_signed(src, dst, signed(cid))
    }

    fn char_signed(&mut self, src: &Movie, dst: &mut Movie, cid: i64) -> PyResult<i64> {
        if let Some(&v) = self.cmap.get(&cid) {
            return Ok(v);
        }
        let Some(node) = src.char(cid)? else {
            let imp = src
                .imports()?
                .into_iter()
                .find(|(_, _, c)| *c == Val::Num(cid as u32) && (0..=i64::from(u32::MAX)).contains(&cid));
            let Some((m, n, _)) = imp else {
                return Err(apt_err(format!(
                    "{}: character {cid} is neither defined nor imported",
                    src.label
                )));
            };
            let (m, n) = (py(m)?, py(n)?);
            let mut new = None;
            for (dm, dn, c) in dst.imports()? {
                // m.lower() of an import without a movie name: the reference's AttributeError
                if py(dm)?.to_lowercase() == m.to_lowercase() && dn.as_deref() == Some(n.as_str()) {
                    new = Some(match c {
                        Val::Num(v) => i64::from(v),
                        Val::Ref(_) => return Err(Fail::Python),
                    });
                    break;
                }
            }
            let new = match new {
                Some(v) => v,
                None => {
                    let v = dst.add_import(&m, &n)?;
                    self.added.push((v, format!("import {m}::{n}"), cid));
                    v
                }
            };
            self.map(cid, new);
            return Ok(new);
        };
        let t = src.num(node, 0)?;
        if t == 3 {
            let name = src.text(src.val(node, 2)?)?;
            for c in 0..dst.char_count()? as i64 {
                if dst.font_name(c)? == name {
                    self.map(cid, c);
                    return Ok(c);
                }
            }
        }
        if t == 9 {
            return Err(apt_err("a movie cannot be grafted as a character"));
        }
        let new = dst.add_char(None)?;
        self.map(cid, new);
        let copy = self.node(src, dst, node)?;
        let tab = dst.chartable()?;
        dst.set(tab, new as usize, Val::Ref(copy))?;
        self.added.push((new, char_type_name(t), cid));
        if t == 1 {
            dst.set(copy, 6, Val::Num(new as u32))?;
            let Some((tab, k)) = src.geo_entry(cid)? else {
                return Err(apt_err(format!("{}: shape {cid} has no geometry", src.label)));
            };
            let Val::Ref(parts) = src.val(tab, k + 2)? else {
                return Err(Fail::Python);
            };
            let p = self.node(src, dst, parts)?;
            dst.add_geometry(new, p)?;
        } else if t == 7 {
            dst.set(copy, 2, Val::Num(new as u32))?;
            self.notes.push(format!(
                "image {cid} -> {new}: its pixels are an entry of the source movie's texture atlas (atlas_meta \
                 TextureFile {cid}); the target atlas needs an entry {new}"
            ));
        } else if t == 3 {
            let name = src.text(src.val(node, 2)?)?;
            self.notes.push(format!(
                "font \"{}\" (char {cid}) is not in the target; copied as char {new}",
                name.unwrap_or_else(|| "None".to_owned())
            ));
        }
        Ok(new)
    }

    /// `Grafter.node(n)`: a copy of a source node for the target (memoised; ids, constants and fonts re-mapped).
    pub fn node(&mut self, src: &Movie, dst: &mut Movie, n: Rid) -> PyResult<Rid> {
        if let Some(&c) = self.memo.get(&n) {
            return Ok(c);
        }
        let node = src.node(n)?.clone();
        if node.file == CONST {
            return Err(apt_err("constant-file data is copied by index, not as nodes"));
        }
        let tag = src.tag(n)?.to_owned();
        match &node.content {
            Content::Str(_) | Content::Raw(_) => {
                let c = dst.add_node(node.file, &tag, node.content.clone());
                self.memo.insert(n, c);
                Ok(c)
            }
            Content::Code(code) => {
                let c = dst.add_node(node.file, &tag, Content::Code(Code::default()));
                self.memo.insert(n, c);
                let mut out = Code::default();
                self.stream(src, dst, code, 0, &mut out, &mut Imap::default())?;
                *dst.code_mut(c)? = out;
                Ok(c)
            }
            Content::Words(vals) => {
                let c = dst.add_node(node.file, &tag, Content::Words(Vec::new()));
                self.memo.insert(n, c);
                let mut v = Vec::with_capacity(vals.len());
                for x in vals {
                    v.push(match x {
                        Val::Ref(r) => Val::Ref(self.node(src, dst, *r)?),
                        Val::Num(_) => *x,
                    });
                }
                *dst.vals_mut(c)? = v;
                self.fix(src, dst, c, &tag)?;
                Ok(c)
            }
        }
    }

    fn fix(&mut self, src: &Movie, dst: &mut Movie, c: Rid, tag: &str) -> PyResult<()> {
        let remap = |g: &mut Self, dst: &mut Movie, k: usize| -> PyResult<()> {
            let id = dst.num(c, k)?;
            let v = g.char(src, dst, id)?;
            dst.set(c, k, Val::Num(v as u32))
        };
        match tag {
            "place" if dst.num(c, 1)? & 2 != 0 => remap(self, dst, 3),
            "initaction" => remap(self, dst, 1),
            "text" => remap(self, dst, 6),
            "stext.records" => {
                let n = dst.vals(c)?.len();
                let mut k = 0;
                while k < n {
                    remap(self, dst, k)?;
                    k += 14;
                }
                Ok(())
            }
            "pushlist" | "poollist" => {
                let v = dst.vals(c)?.clone();
                let mut out = Vec::with_capacity(v.len());
                for x in v {
                    let Val::Num(k) = x else { return Err(Fail::Python) };
                    out.push(Val::Num(self.konst(src, dst, k as usize)? as u32));
                }
                *dst.vals_mut(c)? = out;
                Ok(())
            }
            "geo.part4" | "geo.part5" => remap(self, dst, 20),
            _ => Ok(()),
        }
    }

    /// `Grafter.stream(lst, imap)`: a copy of source stream `s` of `code` into `out` (bodies too, branch targets kept
    /// inside); the new stream's index.
    pub fn stream(
        &mut self,
        src: &Movie,
        dst: &mut Movie,
        code: &Code,
        s: u32,
        out: &mut Code,
        imap: &mut Imap,
    ) -> PyResult<u32> {
        let lst = py(code.streams.get(s as usize))?.clone();
        let ns = out.streams.len() as u32;
        out.streams.push(Vec::new());
        imap.streams.insert(s, ns);
        let mut local = Vec::with_capacity(lst.len());
        for &k in &lst {
            let i = py(code.ins.get(k as usize))?;
            let mut args = Vec::with_capacity(i.args.len());
            for a in &i.args {
                args.push(match a {
                    Arg::Ref(r) => Arg::Ref(self.node(src, dst, *r)?),
                    _ => *a,
                });
            }
            let j = push_ins(out, ins(i.op, args));
            if let Some(b) = i.body {
                let nb = self.stream(src, dst, code, b, out, imap)?;
                out.ins[j as usize].body = Some(nb);
            }
            imap.ins.insert(k, j);
            local.push(j);
        }
        for (&k, &j) in lst.iter().zip(&local) {
            let i = &code.ins[k as usize];
            if is_branch(i.op) {
                out.ins[j as usize].target = match i.target {
                    Target::Ins(t) => Target::Ins(*py(imap.ins.get(&t))?),
                    Target::End(t) => Target::End(*py(imap.streams.get(&t))?),
                    _ => return Err(Fail::Python),
                };
            }
        }
        out.streams[ns as usize] = local;
        Ok(ns)
    }

    /// `Grafter.code_range(code, first, end)`: a new Code node for the target - the top-level instructions of
    /// source Code node `code` from offset `first` up to `end`, after a ConstantPool of the pool constants they
    /// use, then End.
    #[allow(clippy::too_many_lines, reason = "the reference's one function, kept in one piece")]
    pub fn code_range(&mut self, src: &Movie, dst: &mut Movie, code_rid: Rid, first: i64, end: i64) -> PyResult<Rid> {
        let code = src.code(code_rid)?.clone();
        let top = py(code.streams.first())?.clone();
        let mut idx: HashMap<i64, usize> = HashMap::new();
        for (k, &i) in top.iter().enumerate() {
            if let Some(at) = code.ins[i as usize].at {
                idx.insert(at, k);
            } else {
                return Err(Fail::Python);
            }
        }
        let last_at = py(top.last().and_then(|&i| code.ins[i as usize].at))?;
        if !idx.contains_key(&first) || (!idx.contains_key(&end) && end != last_at + 1) {
            let at = src.node(code_rid)?.at.map_or("None".to_owned(), |a| format!("{a:#x}"));
            return Err(apt_err(format!(
                "{} / {} are not instruction offsets of the stream at {at}",
                hexs(first),
                hexs(end)
            )));
        }
        let (a, b) = (idx[&first], idx.get(&end).copied().unwrap_or(top.len()));
        let mut pool: Option<Vec<Val>> = None;
        for &i in &top[..a] {
            let i = &code.ins[i as usize];
            if i.op == 0x88 {
                pool = Some(match i.args.get(1) {
                    Some(Arg::Ref(r)) => src.vals(*r)?.clone(),
                    _ => Vec::new(),
                });
            }
        }
        let part: Vec<u32> = top[a..b.max(a)].to_vec();
        let inside: std::collections::HashSet<u32> = part.iter().copied().collect();
        let mut walk = Vec::new();
        for &k in &part {
            walk.push(k);
            if let Some(bd) = code.ins[k as usize].body {
                walk_into(&code, bd, &mut walk, 1);
            }
        }
        let bodies: std::collections::HashSet<u32> = walk.iter().filter_map(|&k| code.ins[k as usize].body).collect();
        let mut used: Vec<i64> = Vec::new();
        for &k in &walk {
            let i = &code.ins[k as usize];
            if i.op == 0x88 {
                return Err(apt_err("the range sets its own constant pool"));
            }
            if is_branch(i.op) {
                let ok = match i.target {
                    Target::Ins(t) => inside.contains(&t),
                    Target::End(s) => bodies.contains(&s),
                    _ => false,
                };
                if !ok {
                    let at = i.at.map_or("None".to_owned(), |a| format!("{a:#x}"));
                    return Err(apt_err(format!("the branch at {at} leaves the range")));
                }
            }
            if is_pool_byte(i.op) || i.op == 0xA3 {
                let x = int_arg(i, 0)?;
                if pool.as_ref().is_none_or(|p| x < 0 || x as usize >= p.len()) {
                    let at = i.at.map_or("None".to_owned(), |a| format!("{a:#x}"));
                    return Err(apt_err(format!(
                        "the instruction at {at} uses a constant-pool entry the stream has not set"
                    )));
                }
                if !used.contains(&x) {
                    used.push(x);
                }
            }
        }
        used.sort_unstable();
        let remap: HashMap<i64, i64> = used.iter().enumerate().map(|(n, &k)| (k, n as i64)).collect();
        // self.stream(part): the slice is a new list of its own
        let mut sliced = code.clone();
        sliced.streams.push(part.clone());
        let ps = (sliced.streams.len() - 1) as u32;
        let mut body = Code::default();
        self.stream(src, dst, &sliced, ps, &mut body, &mut Imap::default())?;
        for k in walk_ins(&body, 0) {
            let i = &mut body.ins[k as usize];
            if is_pool_byte(i.op) || i.op == 0xA3 {
                let x = int_arg(i, 0)?;
                i.args[0] = Arg::Int(remap[&x]);
            }
        }
        let pool = pool.unwrap_or_default();
        let mut consts = Vec::with_capacity(used.len());
        for &k in &used {
            let Val::Num(c) = pool[k as usize] else {
                return Err(Fail::Python);
            };
            consts.push(Val::Num(self.konst(src, dst, c as usize)? as u32));
        }
        let pl = dst.new_words(APT, consts, "poollist");
        if used.len() > 255
            && walk_ins(&body, 0)
                .iter()
                .any(|&k| is_pool_byte(body.ins[k as usize].op))
        {
            return Err(apt_err("more than 255 pool constants"));
        }
        let mut out = body;
        let head = push_ins(&mut out, ins(0x88, vec![Arg::Int(used.len() as i64), Arg::Ref(pl)]));
        let endi = push_ins(&mut out, ins(0, Vec::new()));
        let top0 = std::mem::take(&mut out.streams[0]);
        out.streams[0] = std::iter::once(head).chain(top0).chain(std::iter::once(endi)).collect();
        Ok(dst.new_code(out))
    }
}

/// `f'{v:#x}'` of a Python int.
pub fn hexs(v: i64) -> String {
    if v < 0 {
        format!("-{:#x}", v.unsigned_abs())
    } else {
        format!("{v:#x}")
    }
}

/// `Grafter.stream`'s `imap`: source instruction / stream -> its copy.
#[derive(Debug, Default)]
pub struct Imap {
    ins: HashMap<u32, u32>,
    streams: HashMap<u32, u32>,
}

/// `aptgraft.Builder`: a new action stream for a movie.
#[derive(Debug, Default)]
pub struct Builder {
    code: Code,
    labels: HashMap<String, usize>,
    pending: Vec<(u32, String)>,
    names: Vec<String>,
}

impl Builder {
    /// `Builder(movie)`.
    pub fn new() -> Self {
        let mut b = Self::default();
        b.code.streams.push(Vec::new());
        b
    }

    fn add(&mut self, i: Ins) -> u32 {
        let k = push_ins(&mut self.code, i);
        self.code.streams[0].push(k);
        k
    }

    /// `b.pool(*names)`.
    pub fn pool(&mut self, m: &mut Movie, names: &[&str]) -> PyResult<()> {
        self.names = names.iter().map(|s| (*s).to_owned()).collect();
        let mut idx = Vec::new();
        for n in names {
            idx.push(Val::Num(m.const_index(&CKey::S(latin1_encode(n)?))? as u32));
        }
        let n = idx.len();
        let pl = m.new_words(APT, idx, "poollist");
        self.add(ins(0x88, vec![Arg::Int(n as i64), Arg::Ref(pl)]));
        Ok(())
    }

    /// `b.label(name)`.
    pub fn label(&mut self, name: &str) {
        self.labels.insert(name.to_owned(), self.code.streams[0].len());
    }

    /// `b.op(name, *args)`.
    pub fn op(&mut self, m: &mut Movie, name: &str, arg: Option<&str>) -> PyResult<&mut Self> {
        let op = py(op_named(name))?;
        let mut i = ins(op, Vec::new());
        let mut branch = None;
        if is_pool_byte(op) {
            let a = py(arg)?;
            let k = py(self.names.iter().position(|n| n == a))?;
            i.args = vec![Arg::Int(k as i64)];
        } else if is_branch(op) {
            branch = Some(py(arg)?.to_owned());
        } else if is_one_str(op) {
            let s = m.new_str(APT, latin1_encode(py(arg)?)?);
            i.args = vec![Arg::Ref(s)];
        } else if op >= 0x80 {
            return Err(apt_err(format!("Builder does not write {name}")));
        }
        let k = self.add(i);
        if let Some(l) = branch {
            self.pending.push((k, l));
        }
        Ok(self)
    }

    /// `b.code()`: the Code node (End added).
    pub fn code(mut self, m: &mut Movie) -> PyResult<Rid> {
        self.add(ins(0, Vec::new()));
        for (k, name) in &self.pending {
            let at = *py(self.labels.get(name))?;
            let t = self.code.streams[0][at];
            self.code.ins[*k as usize].target = Target::Ins(t);
        }
        Ok(m.new_code(self.code))
    }
}

/// `place_item(movie, depth, cid, name, x, y, flags)`.
pub fn place_item(
    m: &mut Movie,
    depth: i64,
    cid: i64,
    name: Option<&str>,
    x: f64,
    y: f64,
    flags: u32,
) -> PyResult<Rid> {
    let one = f32_bits(1.0)?;
    let mut v: Vec<Val> = [3, flags, 0, 0, one, 0, 0, one, 0, 0, 0xFFFF_FFFF, 0, 0, 0, 0, 0]
        .into_iter()
        .map(Val::Num)
        .collect();
    v[2] = Val::Num(depth as u32);
    v[3] = Val::Num(cid as u32);
    v[8] = Val::Num(f32_bits(x)?);
    v[9] = Val::Num(f32_bits(y)?);
    match name.filter(|n| !n.is_empty()) {
        Some(n) => {
            let s = m.new_str(APT, latin1_encode(n)?);
            v[13] = Val::Ref(s);
        }
        None => v[1] = Val::Num(flags & !0x20),
    }
    Ok(m.new_words(APT, v, "place"))
}

/// `free_depths(movie, count, start)`.
pub fn free_depths(m: &mut Movie, count: usize, start: i64) -> PyResult<Vec<i64>> {
    let used = m.timeline_depths(None)?;
    let mut out = Vec::new();
    let mut d = start;
    while out.len() < count {
        if !used.contains(&d) {
            out.push(d);
        }
        d += 1;
    }
    Ok(out)
}

/// `Movie.load(spec)`'s members: a `.res`, a `.apt` with its `.const` / `.geo` beside it, or `<bundle.fbrb>:<name>`
/// (read through the bundle reader). (label, members).
pub fn load_members(spec: &str) -> PyResult<(String, Members)> {
    use bc2_pyrt::py::{basename, isfile, splitext};
    if isfile(spec) {
        if spec.to_lowercase().ends_with(".apt") {
            let base = splitext(spec).0;
            let name = basename(&base);
            let mut out = vec![(format!("{name}.apt"), bc2_pyrt::err::read_file(spec)?)];
            for ext in [".const", ".geo"] {
                let p = format!("{base}{ext}");
                if isfile(&p) {
                    out.push((format!("{name}{ext}"), bc2_pyrt::err::read_file(&p)?));
                }
            }
            return Ok((spec.to_owned(), out));
        }
        return Ok((spec.to_owned(), read_bigf(&bc2_pyrt::err::read_file(spec)?, spec)?));
    }
    let low = spec.to_lowercase();
    if low.contains(".fbrb:") || bundle_spec(spec) {
        let Some((bundle, name)) = spec.rsplit_once(':') else {
            return Err(Fail::Python);
        };
        if !isfile(bundle) {
            return Err(Fail::Err(PyErr::file_not_found(bundle)));
        }
        let entries = bc2_formats::fbrb::entries_path(bundle)
            .map_err(|_| Fail::Python)?
            .entries;
        // fbrb.names: {lower name: entry}, the last of a repeated name
        let mut names: HashMap<String, String> = HashMap::new();
        for e in &entries {
            names.insert(e.name.to_lowercase(), e.name.clone());
        }
        let entry = if names.contains_key(&name.to_lowercase()) {
            name.to_owned()
        } else {
            format!("Menu/{name}.res")
        };
        let Some(real) = names.get(&entry.to_lowercase()) else {
            return Err(apt_err(format!("no {entry} in {bundle}")));
        };
        let data = bc2_gamefs::bundles::read_path(bundle, &[real.to_lowercase()])
            .map_err(|_| Fail::Python)?
            .into_iter()
            .next()
            .ok_or(Fail::Python)?;
        BUNDLE_READ.store(true, std::sync::atomic::Ordering::SeqCst);
        return Ok((spec.to_owned(), read_bigf(&data, spec)?));
    }
    Err(Fail::Err(PyErr::file_not_found(spec)))
}

/// Set once a movie was read out of a bundle (`fbrb.read`): the Python then runs the caches' overall budget
/// (`fbrb._budget`), which the command line runs too (`bfbc2.exe`'s `budget.rs`).
pub static BUNDLE_READ: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// `BUNDLE_SPEC.search(spec)`: `\.fbrb(?:\.[^\\/:]+)?:[^\\/:]+$`, ignoring case.
#[allow(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "compared lower-cased, as IGNORECASE"
)]
fn bundle_spec(spec: &str) -> bool {
    let low = spec.to_lowercase();
    let Some((head, tail)) = low.rsplit_once(':') else {
        return false;
    };
    if tail.is_empty() || tail.contains(['\\', '/']) {
        return false;
    }
    let bad = |s: &str| s.is_empty() || s.contains(['\\', '/', ':']);
    if head.ends_with(".fbrb") {
        return true;
    }
    match head.rfind(".fbrb.") {
        Some(i) => !bad(&head[i + 6..]),
        None => false,
    }
}

/// `Movie.load(spec)`.
pub fn load(spec: &str) -> PyResult<Movie> {
    let (label, members) = load_members(spec)?;
    Movie::new(members, &label)
}
