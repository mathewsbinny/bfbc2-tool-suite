//! The movie model of the menu tools' `aptgraft.py` (bfbc2-menu-tools), ported (round E4c): every structure of a
//! movie's `.apt`, `.const` and `.geo` a node (raw header, C string, 32-bit words, an action stream as
//! instructions), parsed in the order and with the sharing `aptgraft.Movie._parse` gives, then laid out (layouts
//! `keep` and `pack`) and written in either byte order, byte for byte as `Movie.write`. `leftovers` and
//! `roundtrip` (with apt.py's dump compared, [`dump_text`]) are here too.
//!
//! The Python model stays the reference: wherever its code would raise (an `AptError` it words, a `struct.error`,
//! an assertion, a value out of range for a field) or depend on an object's identity in a way not modelled here,
//! this port refuses ([`Refused`]) and `aptgraft.py` runs its own code on the same data, so every answer, error
//! included, is the reference's. Nodes are numbered ([`Rid`]); `aptgraft.NativeMovie` gives Python objects for
//! them on demand and sends edited or new ones back ([`Model::set_node`]) before a write.
//!
//! ```
//! use bc2_formats::aptmodel::{self, Layout};
//!
//! // a .const without a movie it points at: refused (Python raises an AptError for it)
//! let c = b"Apt constant file\x1a\0\0\x10\0\0\0\0\0\0\0\0\0\0\0".to_vec();
//! assert!(aptmodel::Model::parse(b"Apt Data:74\x1a", &c, None, false).is_err());
//! assert_eq!(Layout::Keep as u8, 0);
//! ```

#![expect(clippy::many_single_char_names, reason = "the Python names: a, c, f, n, p, s, ...")]

use std::collections::{HashMap, HashSet};

use crate::apt;

/// A node's number in its [`Model`].
pub type Rid = u32;

/// The members, in `Movie.roots` order.
pub const FILES: [&str; 3] = ["apt", "const", "geo"];

/// The character signature after a character's type.
pub const SIG: u32 = 0x0987_6543;

/// Function bodies nested deeper are refused (Python's recursion limit decides there).
pub const MAX_NESTING: usize = 200;

/// Why the port hands the work to `aptgraft.py`'s Python (the reason is for debugging only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused(pub String);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Refused {}

/// The prefix of a refusal whose reason is the `AptError` `aptgraft.py`'s own parse raises there: its words after
/// `<label>: ` (round OR14: `bc2_menu::graft::Movie::new` raises them, as the Python does for a movie cut short).
pub const APT_ERROR: &str = "AptError: ";

type R<T> = Result<T, Refused>;

fn refuse<T>(why: impl Into<String>) -> R<T> {
    Err(Refused(why.into()))
}

/// A field of a `Words` node: a number or a pointer to a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Val {
    /// A 32-bit value.
    Num(u32),
    /// A pointer.
    Ref(Rid),
}

/// An instruction's argument: an integer, a pointer, or anything else Python holds there (refused when used).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arg {
    /// An integer as Python holds it.
    Int(i64),
    /// A pointer (`Ref`).
    Ref(Rid),
    /// Another Python value.
    Other,
}

/// A branch's target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Not a branch (or no target set).
    Unset,
    /// An instruction of the same code node (an index into [`Code::ins`]).
    Ins(u32),
    /// The end of a stream of the code node (an index into [`Code::streams`]).
    End(u32),
    /// Something the code node does not place (the write raises in Python).
    Missing,
}

/// One instruction (`aptgraft.Ins`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ins {
    /// The opcode.
    pub op: u8,
    /// The arguments.
    pub args: Vec<Arg>,
    /// A function body (an index into [`Code::streams`]).
    pub body: Option<u32>,
    /// A branch's target.
    pub target: Target,
    /// Where it was read.
    pub at: Option<i64>,
}

/// An action stream: its instructions and streams (0 the top level; function bodies after).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Code {
    /// Every instruction.
    pub ins: Vec<Ins>,
    /// Each stream's instructions, in order.
    pub streams: Vec<Vec<u32>>,
}

/// What a node holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    /// A header's bytes.
    Raw(Vec<u8>),
    /// A C string's text (without its NUL).
    Str(Vec<u8>),
    /// 32-bit fields.
    Words(Vec<Val>),
    /// An action stream.
    Code(Code),
}

impl Content {
    /// 0 raw, 1 string, 2 words, 3 code (the order of `aptgraft`'s classes).
    #[must_use]
    pub fn kind(&self) -> u8 {
        match self {
            Content::Raw(_) => 0,
            Content::Str(_) => 1,
            Content::Words(_) => 2,
            Content::Code(_) => 3,
        }
    }
}

/// A node (`aptgraft.Node`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// 0 `.apt`, 1 `.const`, 2 `.geo`.
    pub file: u8,
    /// Where it was read (`None`: new).
    pub at: Option<i64>,
    /// Its size as read.
    pub size0: Option<i64>,
    /// Its tag (an index into [`Model::tags`]).
    pub tag: u16,
    /// What it holds.
    pub content: Content,
}

/// A cache key: (file, offset, kind: 0 words, 1 empty words, 2 string, 3 code).
pub type Key = (u8, i64, u8);

/// The headers and the movie (`Movie.apt_head` ...).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Heads {
    /// `.apt`'s signature.
    pub apt_head: Rid,
    /// `.const`'s signature.
    pub const_magic: Rid,
    /// `.const`'s header words.
    pub const_head: Rid,
    /// The movie character.
    pub movie: Rid,
    /// `.geo`'s signature.
    pub geo_magic: Option<Rid>,
    /// `.geo`'s header words.
    pub geo_head: Option<Rid>,
}

/// A layout (module text of `aptgraft.py`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Unchanged nodes stay; new and grown ones after the end.
    Keep = 0,
    /// Every node in its old order, shifted where something before it grew.
    Pack = 1,
}

/// A parsed movie.
#[derive(Debug, Clone, Default)]
pub struct Model {
    /// Every node.
    pub nodes: Vec<Node>,
    /// The tags.
    pub tags: Vec<String>,
    tag_ix: HashMap<String, u16>,
    /// `Movie.cache`: the nodes read, by key.
    pub cache: HashMap<Key, Rid>,
    /// The keys in the order they were read.
    pub cache_keys: Vec<Key>,
    /// The members' bytes (`.geo` empty without one).
    pub data: [Vec<u8>; 3],
    /// A `.geo` member.
    pub has_geo: bool,
    /// Big-endian.
    pub big: bool,
    /// The headers and the movie.
    pub heads: Heads,
}

const W: u8 = 0;
const EMPTY: u8 = 1;
const S: u8 = 2;
const C: u8 = 3;
const APT: u8 = 0;
const CONST: u8 = 1;
const GEO: u8 = 2;

/// The operand layout of each opcode (`aptgraft._KIND`).
#[must_use]
pub fn kind(op: u8) -> i8 {
    match op {
        0xB7 | 0xB4 => 4,
        0x81 | 0x87 | 0x94 | 0x9F | 0x99 | 0x9D | 0xB8 | 0x8B | 0x8C | 0xA1 | 0xA4 | 0xA5 | 0xA6 | 0xA7 => -1,
        0x83 | 0x88 | 0x96 | 0x8A => -2,
        0x8D | 0xA2 | 0xAE | 0xAF | 0xB0 | 0xB1 | 0xB2 | 0xB3 | 0xB5 | 0xB9 => 1,
        0xA3 | 0xB6 => 2,
        0x9B => -3,
        0x8E => -4,
        _ if op >= 0x80 && apt::op_name(op).is_none() => -5,
        _ => 0,
    }
}

fn is_branch(op: u8) -> bool {
    matches!(op, 0x99 | 0x9D | 0xB8)
}

fn is_one_str(op: u8) -> bool {
    matches!(op, 0x8B | 0x8C | 0xA1 | 0xA4 | 0xA5 | 0xA6 | 0xA7)
}

fn al(p: i64) -> i64 {
    (p + 3) & !3
}

/// `struct.unpack_from(e + 'I', buf, o)`.
fn rd32(buf: &[u8], o: i64, big: bool) -> R<u32> {
    let w = usize::try_from(o)
        .ok()
        .and_then(|o| buf.get(o..)?.first_chunk::<4>().copied())
        .ok_or_else(|| Refused(format!("read of 4 bytes at {o} past the end")))?;
    Ok(if big {
        u32::from_be_bytes(w)
    } else {
        u32::from_le_bytes(w)
    })
}

fn rd16(buf: &[u8], o: i64, big: bool) -> R<u16> {
    let w = usize::try_from(o)
        .ok()
        .and_then(|o| buf.get(o..)?.first_chunk::<2>().copied())
        .ok_or_else(|| Refused(format!("read of 2 bytes at {o} past the end")))?;
    Ok(if big {
        u16::from_be_bytes(w)
    } else {
        u16::from_le_bytes(w)
    })
}

/// Python's `buf[a:b] = src` for 0 <= a (a slice past the end clipped, the bytearray grown or shrunk).
fn splice(buf: &mut Vec<u8>, a: i64, b: i64, src: &[u8]) -> R<()> {
    if a < 0 || b < 0 {
        return refuse("a negative offset");
    }
    let n = buf.len();
    let a = (a as usize).min(n);
    let b = (b as usize).min(n).max(a);
    if b - a == src.len() {
        if let Some(d) = buf.get_mut(a..b) {
            d.copy_from_slice(src);
        }
    } else {
        buf.splice(a..b, src.iter().copied());
    }
    Ok(())
}

fn put32(out: &mut Vec<u8>, v: i64, big: bool) -> R<()> {
    let v = u32::try_from(v).map_err(|_| Refused(format!("{v} out of range for a 32-bit field")))?;
    out.extend(if big { v.to_be_bytes() } else { v.to_le_bytes() });
    Ok(())
}

fn put_i32(out: &mut Vec<u8>, v: i64, big: bool) -> R<()> {
    let v = i32::try_from(v).map_err(|_| Refused(format!("{v} out of range for a branch")))?;
    out.extend(if big { v.to_be_bytes() } else { v.to_le_bytes() });
    Ok(())
}

fn put16(out: &mut Vec<u8>, v: i64, big: bool) -> R<()> {
    let v = u16::try_from(v).map_err(|_| Refused(format!("{v} out of range for a 16-bit field")))?;
    out.extend(if big { v.to_be_bytes() } else { v.to_le_bytes() });
    Ok(())
}

/// `aptgraft.Movie.__init__`'s byte order: `None` where it raises (no constant file signature, a short header).
#[must_use]
pub fn movie_big(apt: &[u8], c: &[u8]) -> Option<bool> {
    if !c.starts_with(b"Apt constant file") {
        return None;
    }
    let le = rd32(c, 20, false).ok()?;
    let be = rd32(c, 20, true).ok()?;
    let is_movie = |o: u32, big: bool| {
        let o = i64::from(o);
        0 < o && o <= apt.len() as i64 - 8 && rd32(apt, o + 4, big).ok() == Some(SIG)
    };
    Some(!is_movie(le, false) && is_movie(be, true))
}

struct Parser<'a> {
    m: Model,
    bufs: [&'a [u8]; 3],
    big: bool,
    depth: usize,
}

impl Parser<'_> {
    fn buf(&self, f: u8) -> &[u8] {
        self.bufs.get(usize::from(f)).copied().unwrap_or_default()
    }

    fn u(&self, f: u8, o: i64) -> R<u32> {
        rd32(self.buf(f), o, self.big)
    }

    fn add(&mut self, node: Node) -> Rid {
        self.m.nodes.push(node);
        (self.m.nodes.len() - 1) as Rid
    }

    fn remember(&mut self, key: Key, rid: Rid) {
        self.m.cache.insert(key, rid);
        self.m.cache_keys.push(key);
    }

    fn vals(&self, rid: Rid) -> R<&Vec<Val>> {
        match self.m.nodes.get(rid as usize).map(|n| &n.content) {
            Some(Content::Words(v)) => Ok(v),
            _ => refuse("not a words node"),
        }
    }

    fn vals_mut(&mut self, rid: Rid) -> R<&mut Vec<Val>> {
        match self.m.nodes.get_mut(rid as usize).map(|n| &mut n.content) {
            Some(Content::Words(v)) => Ok(v),
            _ => refuse("not a words node"),
        }
    }

    /// A field as a number (Python would raise using a `Ref` as one).
    fn num(&self, rid: Rid, k: usize) -> R<i64> {
        match self.vals(rid)?.get(k) {
            Some(Val::Num(v)) => Ok(i64::from(*v)),
            Some(Val::Ref(_)) => refuse("a pointer used as a number"),
            None => refuse("a field past the node"),
        }
    }

    fn set(&mut self, rid: Rid, k: usize, v: Val) -> R<()> {
        match self.vals_mut(rid)?.get_mut(k) {
            Some(x) => {
                *x = v;
                Ok(())
            }
            None => refuse("a field past the node"),
        }
    }

    /// `Movie.words`.
    fn words(&mut self, f: u8, at: i64, count: i64, tag: &str) -> R<Rid> {
        let key = (f, at, if count != 0 { W } else { EMPTY });
        if let Some(&n) = self.m.cache.get(&key) {
            if self.vals(n)?.len() as i64 != count {
                return refuse("read with two sizes");
            }
            return Ok(n);
        }
        let b = self.buf(f);
        if at < 0 || count < 0 {
            return refuse("words past the end of the file");
        }
        if at + 4 * count > b.len() as i64 {
            let file = FILES.get(usize::from(f)).copied().unwrap_or("?");
            return refuse(format!(
                "{APT_ERROR}{tag} at {file} {at:#x} runs past the end of the file"
            ));
        }
        let mut v = Vec::with_capacity(count as usize);
        for k in 0..count {
            v.push(Val::Num(rd32(b, at + 4 * k, self.big)?));
        }
        let tag = self.m.tag(tag);
        let n = self.add(Node {
            file: f,
            at: Some(at),
            size0: Some(4 * count),
            tag,
            content: Content::Words(v),
        });
        self.remember(key, n);
        Ok(n)
    }

    /// `Movie.string`.
    fn string(&mut self, f: u8, at: i64) -> R<Rid> {
        if let Some(&n) = self.m.cache.get(&(f, at, S)) {
            return Ok(n);
        }
        let b = self.buf(f);
        if !(0 < at && at < b.len() as i64) {
            return refuse("string pointer outside the file");
        }
        let rest = b.get(at as usize..).unwrap_or_default();
        let Some(e) = rest.iter().position(|&c| c == 0) else {
            return refuse("unterminated string");
        };
        let text = rest.get(..e).unwrap_or_default().to_vec();
        let tag = self.m.tag("str");
        let n = self.add(Node {
            file: f,
            at: Some(at),
            size0: Some(e as i64 + 1),
            tag,
            content: Content::Str(text),
        });
        self.remember((f, at, S), n);
        Ok(n)
    }

    fn sref(&mut self, f: u8, p: i64) -> R<Val> {
        Ok(if p != 0 {
            Val::Ref(self.string(f, p)?)
        } else {
            Val::Num(0)
        })
    }

    /// `Movie.wref`: the fill runs once, the first time the node is read.
    fn wref(&mut self, f: u8, p: i64, count: i64, tag: &str, fill: Option<Fill>) -> R<Val> {
        if p == 0 {
            return Ok(Val::Num(0));
        }
        let new = !self.m.cache.contains_key(&(f, p, if count != 0 { W } else { EMPTY }));
        let n = self.words(f, p, count, tag)?;
        if new && let Some(fill) = fill {
            self.fill(fill, n)?;
        }
        Ok(Val::Ref(n))
    }

    #[expect(clippy::too_many_lines, reason = "one arm per fill callback, as Movie._fill_*")]
    fn fill(&mut self, fill: Fill, n: Rid) -> R<()> {
        let len = self.vals(n)?.len();
        match fill {
            Fill::Consts => {
                for k in (0..len).step_by(2) {
                    if self.vals(n)?.get(k) == Some(&Val::Num(1)) {
                        let v = self.sref(CONST, self.num(n, k + 1)?)?;
                        self.set(n, k + 1, v)?;
                    }
                }
            }
            Fill::Geo => {
                for k in (0..len).step_by(3) {
                    let (p, c) = (self.num(n, k + 2)?, self.num(n, k + 1)?);
                    let v = self.wref(GEO, p, c, "geo.parts", Some(Fill::Parts))?;
                    self.set(n, k + 2, v)?;
                }
            }
            Fill::Parts => {
                for k in 0..len {
                    let p = match self.vals(n)?.get(k) {
                        Some(Val::Num(0)) => continue,
                        Some(Val::Num(p)) => i64::from(*p),
                        _ => return refuse("a part pointer that is not a number"),
                    };
                    let kind = self.u(GEO, p)?;
                    let (part, per) = match kind {
                        1 | 6 => (
                            self.words(GEO, p, 4, &format!("geo.part{kind}"))?,
                            if kind == 1 { 6 } else { 4 },
                        ),
                        4 | 5 => (self.words(GEO, p, 22, &format!("geo.part{kind}"))?, 6),
                        _ => return refuse("unknown geometry part"),
                    };
                    if !matches!(self.vals(part)?.get(2), Some(Val::Ref(_))) {
                        let (q, c) = (self.num(part, 2)?, self.num(part, 1)?);
                        let v = self.wref(GEO, q, per * c, "geo.vertices", None)?;
                        self.set(part, 2, v)?;
                    }
                    self.set(n, k, Val::Ref(part))?;
                }
            }
            Fill::Chartable => {
                for k in 0..len {
                    let p = self.num(n, k)?;
                    let v = if p != 0 {
                        Val::Ref(self.character(p)?)
                    } else {
                        Val::Num(0)
                    };
                    self.set(n, k, v)?;
                }
            }
            Fill::Imports => {
                for k in (0..len).step_by(4) {
                    let v = self.sref(APT, self.num(n, k)?)?;
                    self.set(n, k, v)?;
                    let v = self.sref(APT, self.num(n, k + 1)?)?;
                    self.set(n, k + 1, v)?;
                }
            }
            Fill::Exports => {
                for k in (0..len).step_by(2) {
                    let v = self.sref(APT, self.num(n, k)?)?;
                    self.set(n, k, v)?;
                }
            }
            Fill::Records => {
                for k in (0..len).step_by(14) {
                    let (p, c) = (self.num(n, k + 13)?, self.num(n, k + 12)?);
                    let v = self.wref(APT, p, c, "glyphs", None)?;
                    self.set(n, k + 13, v)?;
                }
            }
            Fill::Frames => {
                for k in (0..len).step_by(2) {
                    let (p, c) = (self.num(n, k + 1)?, self.num(n, k)?);
                    let v = self.wref(APT, p, c, "items", Some(Fill::Items))?;
                    self.set(n, k + 1, v)?;
                }
            }
            Fill::Items => {
                for k in 0..len {
                    let p = self.num(n, k)?;
                    let v = Val::Ref(self.item(p)?);
                    self.set(n, k, v)?;
                }
            }
            Fill::Clip => {
                let (p, c) = (self.num(n, 1)?, self.num(n, 0)?);
                let v = self.wref(APT, p, 3 * c, "clipevents", Some(Fill::Events))?;
                self.set(n, 1, v)?;
            }
            Fill::Events => {
                for k in (0..len).step_by(3) {
                    let p = self.num(n, k + 2)?;
                    let v = Val::Ref(self.code(p)?);
                    self.set(n, k + 2, v)?;
                }
            }
            Fill::F1 => {
                for k in 0..len {
                    let v = self.sref(APT, self.num(n, k)?)?;
                    self.set(n, k, v)?;
                }
            }
            Fill::F2 => {
                for k in (1..len).step_by(2) {
                    let v = self.sref(APT, self.num(n, k)?)?;
                    self.set(n, k, v)?;
                }
            }
        }
        Ok(())
    }

    /// `Movie.character`.
    fn character(&mut self, p: i64) -> R<Rid> {
        let a = self.buf(APT);
        if p + 8 > a.len() as i64 || self.u(APT, p + 4)? != SIG {
            return refuse("no character");
        }
        let t = self.u(APT, p)?;
        let (size, name) = match t {
            1 => (7, "shape"),
            2 => (15, "text"),
            3 => (5, "font"),
            5 => (5, "sprite"),
            6 => (3, "sound"),
            7 => (3, "image"),
            9 => (15, "movie"),
            10 => (14, "statictext"),
            _ => return refuse("character type not supported"),
        };
        let known = self.m.cache.contains_key(&(APT, p, W));
        let n = self.words(APT, p, size, name)?;
        if known {
            return Ok(n);
        }
        if t == 9 || t == 5 {
            let (q, c) = (self.num(n, 3)?, self.num(n, 2)?);
            let v = self.wref(APT, q, 2 * c, "frames", Some(Fill::Frames))?;
            self.set(n, 3, v)?;
        }
        if t == 9 {
            let (q, c) = (self.num(n, 6)?, self.num(n, 5)?);
            let v = self.wref(APT, q, c, "chartable", Some(Fill::Chartable))?;
            self.set(n, 6, v)?;
            let (q, c) = (self.num(n, 11)?, self.num(n, 10)?);
            let v = self.wref(APT, q, 4 * c, "imports", Some(Fill::Imports))?;
            self.set(n, 11, v)?;
            let (q, c) = (self.num(n, 13)?, self.num(n, 12)?);
            let v = self.wref(APT, q, 2 * c, "exports", Some(Fill::Exports))?;
            self.set(n, 13, v)?;
        } else if t == 2 {
            let (x, y) = (self.num(n, 13)?, self.num(n, 14)?);
            let a = self.sref(APT, x)?;
            let b = self.sref(APT, y)?;
            self.set(n, 13, a)?;
            self.set(n, 14, b)?;
        } else if t == 3 {
            let v = self.sref(APT, self.num(n, 2)?)?;
            self.set(n, 2, v)?;
            let (q, c) = (self.num(n, 4)?, self.num(n, 3)?);
            let v = self.wref(APT, q, c, "glyphs", None)?;
            self.set(n, 4, v)?;
        } else if t == 10 {
            let (q, c) = (self.num(n, 13)?, self.num(n, 12)?);
            let v = self.wref(APT, q, 14 * c, "stext.records", Some(Fill::Records))?;
            self.set(n, 13, v)?;
        }
        Ok(n)
    }

    /// `Movie.item`.
    fn item(&mut self, p: i64) -> R<Rid> {
        let t = self.u(APT, p)?;
        let (size, name) = match t {
            1 => (2, "action"),
            2 => (4, "label"),
            3 => (16, "place"),
            4 => (2, "remove"),
            5 => (2, "background"),
            8 => (3, "initaction"),
            _ => return refuse("frame item type not supported"),
        };
        let known = self.m.cache.contains_key(&(APT, p, W));
        let n = self.words(APT, p, size, name)?;
        if known {
            return Ok(n);
        }
        match t {
            1 => {
                let v = Val::Ref(self.code(self.num(n, 1)?)?);
                self.set(n, 1, v)?;
            }
            8 => {
                let v = Val::Ref(self.code(self.num(n, 2)?)?);
                self.set(n, 2, v)?;
            }
            2 => {
                let v = self.sref(APT, self.num(n, 1)?)?;
                self.set(n, 1, v)?;
            }
            3 => {
                let v = self.sref(APT, self.num(n, 13)?)?;
                self.set(n, 13, v)?;
                let v = self.wref(APT, self.num(n, 15)?, 2, "clipactions", Some(Fill::Clip))?;
                self.set(n, 15, v)?;
            }
            _ => {}
        }
        Ok(n)
    }

    /// `Movie.code`.
    fn code(&mut self, p: i64) -> R<Rid> {
        if let Some(&n) = self.m.cache.get(&(APT, p, C)) {
            return Ok(n);
        }
        let mut code = Code::default();
        code.streams.push(Vec::new());
        let end = self.stream(&mut code, 0, p, None)?;
        let tag = self.m.tag("code");
        let n = self.add(Node {
            file: APT,
            at: Some(p),
            size0: Some(end - p),
            tag,
            content: Content::Code(code),
        });
        self.remember((APT, p, C), n);
        Ok(n)
    }

    /// `Movie._stream`: instructions from p into stream `s` of `code`, to the top-level End (end `None`) or to end
    /// exactly; returns where it stopped.
    #[expect(clippy::too_many_lines, reason = "one arm per operand layout, as Movie._stream")]
    fn stream(&mut self, code: &mut Code, s: u32, mut p: i64, end: Option<i64>) -> R<i64> {
        let start = p;
        self.depth += 1;
        if self.depth > MAX_NESTING {
            return refuse("function bodies nested too deep");
        }
        let n_a = self.buf(APT).len() as i64;
        let mut mine: Vec<u32> = Vec::new();
        let mut targets: Vec<(u32, i64)> = Vec::new();
        loop {
            if end.is_none()
                && mine
                    .last()
                    .and_then(|&k| code.ins.get(k as usize))
                    .is_some_and(|i| i.op == 0)
            {
                break;
            }
            if let Some(e) = end
                && p >= e
            {
                if p != e {
                    return refuse("a function body does not decode to its end");
                }
                break;
            }
            if p < 0 {
                return refuse("code runs past the end of the file");
            }
            if p >= n_a {
                return refuse(format!("{APT_ERROR}code from {start:#x} runs past the end of the file"));
            }
            let a = self.buf(APT);
            let op = a.get(p as usize).copied().unwrap_or_default();
            let at = p;
            p += 1;
            let mut ins = Ins {
                op,
                args: Vec::new(),
                body: None,
                target: Target::Unset,
                at: Some(at),
            };
            match kind(op) {
                0 => {}
                4 => {
                    ins.args.push(Arg::Int(i64::from(self.u(APT, p)?)));
                    p += 4;
                }
                1 => {
                    let Some(&b) = a.get(p as usize) else {
                        return refuse("an operand past the end");
                    };
                    ins.args.push(Arg::Int(i64::from(b)));
                    p += 1;
                }
                2 => {
                    ins.args.push(Arg::Int(i64::from(rd16(a, p, self.big)?)));
                    p += 2;
                }
                -1 => {
                    p = al(p);
                    if is_branch(op) {
                        let v = i64::from(self.u(APT, p)? as i32);
                        p += 4;
                        targets.push((code.ins.len() as u32, p + v));
                    } else if is_one_str(op) {
                        let v = self.sref(APT, i64::from(self.u(APT, p)?))?;
                        ins.args.push(val_arg(v));
                        p += 4;
                    } else {
                        ins.args.push(Arg::Int(i64::from(self.u(APT, p)?)));
                        p += 4;
                    }
                }
                -2 => {
                    p = al(p);
                    if op == 0x83 {
                        let x = self.u(APT, p)?;
                        let x = self.sref(APT, i64::from(x))?;
                        let y = self.u(APT, p + 4)?;
                        let y = self.sref(APT, i64::from(y))?;
                        ins.args = vec![val_arg(x), val_arg(y)];
                    } else if op == 0x8A {
                        ins.args = vec![
                            Arg::Int(i64::from(self.u(APT, p)?)),
                            Arg::Int(i64::from(self.u(APT, p + 4)?)),
                        ];
                    } else {
                        let cnt = i64::from(self.u(APT, p)?);
                        let q = i64::from(self.u(APT, p + 4)?);
                        let tag = if op == 0x88 { "poollist" } else { "pushlist" };
                        let v = self.wref(APT, q, cnt, tag, None)?;
                        ins.args = vec![Arg::Int(cnt), val_arg(v)];
                    }
                    p += 8;
                }
                -3 | -4 => {
                    p = al(p);
                    let two = op == 0x8E;
                    let need = if two { 28 } else { 24 };
                    if p < 0 || p + need > n_a {
                        return refuse("a function definition past the end");
                    }
                    let name = i64::from(self.u(APT, p)?);
                    let npar = i64::from(self.u(APT, p + 4)?);
                    let (regs, params, size, m1, m2);
                    if two {
                        let nregs = i64::from(rd16(a, p + 8, self.big)?);
                        let flags = i64::from(rd16(a, p + 10, self.big)?);
                        regs = Some(nregs | flags << 16);
                        params = i64::from(self.u(APT, p + 12)?);
                        size = i64::from(self.u(APT, p + 16)?);
                        m1 = i64::from(self.u(APT, p + 20)?);
                        m2 = i64::from(self.u(APT, p + 24)?);
                    } else {
                        regs = None;
                        params = i64::from(self.u(APT, p + 8)?);
                        size = i64::from(self.u(APT, p + 12)?);
                        m1 = i64::from(self.u(APT, p + 16)?);
                        m2 = i64::from(self.u(APT, p + 20)?);
                    }
                    let name = val_arg(self.sref(APT, name)?);
                    let pv = if two {
                        self.wref(APT, params, 2 * npar, "f2params", Some(Fill::F2))?
                    } else {
                        self.wref(APT, params, npar, "f1params", Some(Fill::F1))?
                    };
                    ins.args = match regs {
                        Some(r) => vec![
                            name,
                            Arg::Int(npar),
                            Arg::Int(r),
                            val_arg(pv),
                            Arg::Int(m1),
                            Arg::Int(m2),
                        ],
                        None => vec![name, Arg::Int(npar), val_arg(pv), Arg::Int(m1), Arg::Int(m2)],
                    };
                    let body = p + need;
                    let bs = code.streams.len() as u32;
                    code.streams.push(Vec::new());
                    ins.body = Some(bs);
                    // the definition is placed before its body's instructions (index order = preorder)
                    let k = code.ins.len() as u32;
                    code.ins.push(ins);
                    mine.push(k);
                    self.stream(code, bs, body, Some(body + size))?;
                    p = body + size;
                    continue;
                }
                _ => return refuse("an opcode with an argument this tool does not know"),
            }
            let k = code.ins.len() as u32;
            code.ins.push(ins);
            mine.push(k);
        }
        // branches: to the stream's end or an instruction of the stream
        let by_at: HashMap<i64, u32> = mine
            .iter()
            .filter_map(|&k| code.ins.get(k as usize).and_then(|i| i.at).map(|at| (at, k)))
            .collect();
        for (k, t) in targets {
            let tgt = if t == p {
                Target::End(s)
            } else if let Some(&j) = by_at.get(&t) {
                Target::Ins(j)
            } else {
                return refuse("a branch that does not land on an instruction of its stream");
            };
            if let Some(i) = code.ins.get_mut(k as usize) {
                i.target = tgt;
            }
        }
        if let Some(st) = code.streams.get_mut(s as usize) {
            *st = mine;
        }
        self.depth -= 1;
        Ok(p)
    }
}

fn val_arg(v: Val) -> Arg {
    match v {
        Val::Num(n) => Arg::Int(i64::from(n)),
        Val::Ref(r) => Arg::Ref(r),
    }
}

#[derive(Debug, Clone, Copy)]
enum Fill {
    Consts,
    Geo,
    Parts,
    Chartable,
    Imports,
    Exports,
    Records,
    Frames,
    Items,
    Clip,
    Events,
    F1,
    F2,
}

/// A written member set: `.apt`, `.const`, `.geo` (`None` where the movie has none).
pub type Written = [Option<Vec<u8>>; 3];

/// Placement of one code node: each instruction's position, each stream's end (-1: not placed).
struct Placed {
    ins: Vec<i64>,
    ends: Vec<i64>,
}

impl Model {
    /// Parses a movie as `aptgraft.Movie._parse` does (big: the byte order `Movie.__init__` chose).
    ///
    /// # Errors
    /// [`Refused`] wherever the Python parse raises or might differ.
    pub fn parse(apt: &[u8], c: &[u8], geo: Option<&[u8]>, big: bool) -> R<Model> {
        let mut p = Parser {
            m: Model {
                big,
                has_geo: geo.is_some(),
                ..Model::default()
            },
            bufs: [apt, c, geo.unwrap_or_default()],
            big,
            depth: 0,
        };
        let Some(end) = apt.iter().position(|&b| b == 0x1A) else {
            return refuse("not an APT data file");
        };
        if !apt.starts_with(b"Apt Data") {
            return refuse("not an APT data file");
        }
        let t = p.m.tag("header");
        let apt_head = p.add(Node {
            file: APT,
            at: Some(0),
            size0: Some(end as i64 + 1),
            tag: t,
            content: Content::Raw(apt.get(..=end).unwrap_or_default().to_vec()),
        });
        let const_magic = p.add(Node {
            file: CONST,
            at: Some(0),
            size0: Some(20),
            tag: t,
            content: Content::Raw(c.get(..20.min(c.len())).unwrap_or_default().to_vec()),
        });
        let h = p.words(CONST, 20, 3, "const.header")?;
        let mv = p.num(h, 0)?;
        let movie = p.character(mv)?;
        if p.num(movie, 0)? != 9 {
            return refuse("the .const header does not point at the movie");
        }
        p.set(h, 0, Val::Ref(movie))?;
        let (q, n) = (p.num(h, 2)?, p.num(h, 1)?);
        let v = p.wref(CONST, q, 2 * n, "const.entries", Some(Fill::Consts))?;
        p.set(h, 2, v)?;
        p.m.heads = Heads {
            apt_head,
            const_magic,
            const_head: h,
            movie,
            geo_magic: None,
            geo_head: None,
        };
        if let Some(g) = geo {
            if !g.starts_with(b"Apt PCGL geometry file") {
                return refuse("unknown geometry file");
            }
            let t = p.m.tag("header");
            let gm = p.add(Node {
                file: GEO,
                at: Some(0),
                size0: Some(0x18),
                tag: t,
                content: Content::Raw(g.get(..0x18.min(g.len())).unwrap_or_default().to_vec()),
            });
            let gh = p.words(GEO, 0x18, 2, "geo.header")?;
            let (q, n) = (p.num(gh, 1)?, p.num(gh, 0)?);
            let v = p.wref(GEO, q, 3 * n, "geo.table", Some(Fill::Geo))?;
            p.set(gh, 1, v)?;
            p.m.heads.geo_magic = Some(gm);
            p.m.heads.geo_head = Some(gh);
        }
        p.m.data = [apt.to_vec(), c.to_vec(), geo.unwrap_or_default().to_vec()];
        Ok(p.m)
    }

    /// A tag's index (added when new).
    pub fn tag(&mut self, t: &str) -> u16 {
        if let Some(&k) = self.tag_ix.get(t) {
            return k;
        }
        self.tags.push(t.to_string());
        let k = (self.tags.len() - 1) as u16;
        self.tag_ix.insert(t.to_string(), k);
        k
    }

    /// `Movie.roots`: (file, nodes) in order.
    #[must_use]
    pub fn roots(&self) -> Vec<(u8, Vec<Rid>)> {
        let h = &self.heads;
        let mut out = vec![
            (APT, vec![h.apt_head, h.movie]),
            (CONST, vec![h.const_magic, h.const_head]),
        ];
        if let (Some(m), Some(g)) = (h.geo_magic, h.geo_head) {
            out.push((GEO, vec![m, g]));
        }
        out
    }

    /// A node.
    #[must_use]
    pub fn node(&self, rid: Rid) -> Option<&Node> {
        self.nodes.get(rid as usize)
    }

    /// A new node (its content set later by [`Model::set_node`]): its number.
    pub fn new_node(&mut self) -> Rid {
        self.nodes.push(Node {
            file: APT,
            at: None,
            size0: None,
            tag: 0,
            content: Content::Raw(Vec::new()),
        });
        (self.nodes.len() - 1) as Rid
    }

    /// Replaces a node (the Python object's state sent back).
    ///
    /// # Errors
    /// [`Refused`] for a number that is no node or a pointer to none.
    pub fn set_node(&mut self, rid: Rid, node: Node) -> R<()> {
        let n = self.nodes.len() as u32;
        let ok = |r: &Rid| *r < n;
        let refs_ok = match &node.content {
            Content::Words(v) => v.iter().all(|x| match x {
                Val::Ref(r) => ok(r),
                Val::Num(_) => true,
            }),
            Content::Code(c) => {
                c.ins.iter().all(|i| {
                    i.args.iter().all(|a| match a {
                        Arg::Ref(r) => ok(r),
                        _ => true,
                    }) && i.body.is_none_or(|b| (b as usize) < c.streams.len())
                }) && c.streams.iter().flatten().all(|&k| (k as usize) < c.ins.len())
            }
            _ => true,
        };
        if !refs_ok || node.file > 2 {
            return refuse("a node pointing at nothing");
        }
        match self.nodes.get_mut(rid as usize) {
            Some(slot) => {
                *slot = node;
                Ok(())
            }
            None => refuse("no such node"),
        }
    }

    fn refs(&self, rid: Rid, out: &mut Vec<Rid>) {
        match self.nodes.get(rid as usize).map(|n| &n.content) {
            Some(Content::Words(v)) => out.extend(v.iter().filter_map(|x| match x {
                Val::Ref(r) => Some(*r),
                Val::Num(_) => None,
            })),
            Some(Content::Code(c)) => code_refs(c, 0, out, 0),
            _ => {}
        }
    }

    /// `Movie.reachable()`: the nodes reachable from the roots, each once, per file, in the Python walk's order.
    #[must_use]
    pub fn reachable(&self) -> Vec<(u8, Vec<Rid>)> {
        let roots = self.roots();
        let mut out: Vec<(u8, Vec<Rid>)> = roots.iter().map(|(f, _)| (*f, Vec::new())).collect();
        let mut seen = vec![false; self.nodes.len()];
        let mut stack: Vec<Rid> = roots.iter().flat_map(|(_, r)| r.iter().copied()).collect();
        let mut refs = Vec::new();
        while let Some(n) = stack.pop() {
            match seen.get_mut(n as usize) {
                Some(s) if !*s => *s = true,
                _ => continue,
            }
            let f = self.nodes.get(n as usize).map_or(APT, |x| x.file);
            match out.iter_mut().find(|(g, _)| *g == f) {
                Some((_, l)) => l.push(n),
                None => out.push((f, vec![n])),
            }
            refs.clear();
            self.refs(n, &mut refs);
            stack.extend_from_slice(&refs);
        }
        out
    }

    fn size(&self, rid: Rid, phase: i64) -> R<i64> {
        match self.nodes.get(rid as usize).map(|n| &n.content) {
            Some(Content::Raw(d)) => Ok(d.len() as i64),
            Some(Content::Str(t)) => Ok(t.len() as i64 + 1),
            Some(Content::Words(v)) => Ok(4 * v.len() as i64),
            Some(Content::Code(c)) => {
                let mut pl = Placed::new(c);
                Ok(place(c, 0, phase, &mut pl, 0)? - phase)
            }
            None => refuse("no such node"),
        }
    }

    /// `Movie._layout`: each node's offset in `new_at`; the member's size.
    fn layout(&self, nodes: &[Rid], f: u8, layout: Layout, gap: i64, new_at: &mut [Option<i64>]) -> R<i64> {
        let mut old: Vec<Rid> = Vec::new();
        let mut new: Vec<Rid> = Vec::new();
        for &n in nodes {
            if self.nodes.get(n as usize).and_then(|x| x.at).is_some() {
                old.push(n);
            } else {
                new.push(n);
            }
        }
        let key = |n: &Rid| {
            let x = self.nodes.get(*n as usize);
            (x.and_then(|x| x.at).unwrap_or(0), x.and_then(|x| x.size0) != Some(0))
        };
        old.sort_by_key(key);
        let end0 = self.member(f)?.len() as i64;
        let set = |new_at: &mut [Option<i64>], n: Rid, at: i64| {
            if let Some(s) = new_at.get_mut(n as usize) {
                *s = Some(at);
            }
        };
        let at_of = |n: Rid| self.nodes.get(n as usize).and_then(|x| x.at).unwrap_or(0);
        if layout == Layout::Keep {
            let mut moved = Vec::new();
            for &n in &old {
                let at = at_of(n);
                if Some(self.size(n, at.rem_euclid(4))?) == self.nodes.get(n as usize).and_then(|x| x.size0) {
                    set(new_at, n, at);
                } else {
                    moved.push(n);
                }
            }
            let mut end = end0;
            for n in moved.into_iter().chain(new) {
                end = self.append(n, end, new_at)?;
            }
            return Ok(end);
        }
        let (mut shift, mut prev_end) = (0i64, 0i64);
        let fixed = |n: Rid| {
            let t = self.nodes.get(n as usize).map_or(0, |x| x.tag);
            matches!(
                self.tags.get(usize::from(t)).map(String::as_str),
                Some("header" | "const.header" | "geo.header")
            )
        };
        for (k, &n) in old.iter().enumerate() {
            let at0 = at_of(n);
            let at = if fixed(n) {
                at0
            } else {
                let mut at = at0 + shift;
                if at < prev_end {
                    shift += al(prev_end - at);
                    at = at0 + shift;
                }
                at
            };
            set(new_at, n, at);
            if fixed(n) && old.get(k + 1).is_none_or(|&x| !fixed(x)) {
                shift += gap;
            }
            prev_end = prev_end.max(at + self.size(n, at.rem_euclid(4))?);
        }
        let mut end = prev_end.max(end0 + shift);
        for n in new {
            end = self.append(n, end, new_at)?;
        }
        Ok(end)
    }

    fn append(&self, n: Rid, end: i64, new_at: &mut [Option<i64>]) -> R<i64> {
        let node = self
            .nodes
            .get(n as usize)
            .ok_or_else(|| Refused("no such node".into()))?;
        let set = |new_at: &mut [Option<i64>], at: i64| {
            if let Some(s) = new_at.get_mut(n as usize) {
                *s = Some(at);
            }
        };
        if let Content::Str(t) = &node.content {
            set(new_at, end);
            return Ok(end + t.len() as i64 + 1);
        }
        let mut at = al(end);
        if let (Content::Code(_), Some(a)) = (&node.content, node.at) {
            at += a.rem_euclid(4);
        }
        set(new_at, at);
        Ok(at + self.size(n, at.rem_euclid(4))?)
    }

    fn member(&self, f: u8) -> R<&Vec<u8>> {
        if f == GEO && !self.has_geo {
            return refuse("no .geo member");
        }
        self.data
            .get(usize::from(f))
            .ok_or_else(|| Refused("no such member".into()))
    }

    /// `Movie.write(order, layout, gap, dead)`: the members' bytes (the member order is the caller's).
    ///
    /// # Errors
    /// [`Refused`] wherever the Python writer raises.
    pub fn write(&self, big: bool, layouts: [Layout; 3], gap: i64, dead: bool) -> R<Written> {
        self.write_placed(big, layouts, gap, dead).map(|(w, _)| w)
    }

    /// [`Model::write`] and each node's offset in it (`None`: not placed), as `Node.new_at` after a Python write.
    ///
    /// # Errors
    /// [`Refused`] wherever the Python writer raises.
    pub fn write_placed(
        &self,
        big: bool,
        layouts: [Layout; 3],
        gap: i64,
        dead: bool,
    ) -> R<(Written, Vec<Option<i64>>)> {
        let nodes = self.reachable();
        let mut new_at: Vec<Option<i64>> = vec![None; self.nodes.len()];
        let mut sizes: Vec<(u8, i64)> = Vec::new();
        for f in [APT, CONST, GEO] {
            if let Some((_, l)) = nodes.iter().find(|(g, _)| *g == f) {
                let lay = layouts.get(usize::from(f)).copied().unwrap_or(Layout::Keep);
                sizes.push((f, self.layout(l, f, lay, gap, &mut new_at)?));
            }
        }
        if nodes.iter().any(|(f, _)| *f > GEO) {
            return refuse("a node of another file");
        }
        let resolve = |r: Rid| -> R<i64> {
            new_at
                .get(r as usize)
                .copied()
                .flatten()
                .ok_or_else(|| Refused("a pointer to a node not placed".into()))
        };
        let mut out: Written = [None, None, None];
        for (f, fsize) in sizes {
            let lay = layouts.get(usize::from(f)).copied().unwrap_or(Layout::Keep);
            let mut buf;
            if dead && lay == Layout::Keep && big == self.big {
                buf = vec![0u8; usize::try_from(fsize).map_err(|_| Refused("size".into()))?];
                let d = self.member(f)?;
                splice(&mut buf, 0, d.len() as i64, d)?;
                for key in &self.cache_keys {
                    let Some(&c) = self.cache.get(key) else { continue };
                    let Some(n) = self.nodes.get(c as usize) else { continue };
                    if n.file == f
                        && let Some(s0) = n.size0
                        && s0 != 0
                    {
                        let Some(at) = n.at else {
                            return refuse("a node read from the file without an offset");
                        };
                        let z = vec![0u8; usize::try_from(s0).map_err(|_| Refused("size".into()))?];
                        splice(&mut buf, at, at + s0, &z)?;
                    }
                }
                let h = &self.heads;
                for r in [Some(h.apt_head), Some(h.const_magic), h.geo_magic]
                    .into_iter()
                    .flatten()
                {
                    let Some(n) = self.nodes.get(r as usize) else { continue };
                    if n.file == f {
                        let (Some(at), Some(s0)) = (n.at, n.size0) else {
                            return refuse("a header without its offset");
                        };
                        let z = vec![0u8; usize::try_from(s0).map_err(|_| Refused("size".into()))?];
                        splice(&mut buf, at, at + s0, &z)?;
                    }
                }
            } else {
                buf = vec![0u8; usize::try_from(fsize).map_err(|_| Refused("size".into()))?];
            }
            let list = nodes
                .iter()
                .find(|(g, _)| *g == f)
                .map_or(&[][..], |(_, l)| l.as_slice());
            let mut b = Vec::new();
            for &r in list {
                let at = resolve(r)?;
                let n = self
                    .nodes
                    .get(r as usize)
                    .ok_or_else(|| Refused("no such node".into()))?;
                b.clear();
                match &n.content {
                    Content::Raw(d) => b.extend_from_slice(d),
                    Content::Str(t) => {
                        b.extend_from_slice(t);
                        b.push(0);
                    }
                    Content::Words(v) => {
                        for x in v {
                            let w = match x {
                                Val::Num(w) => i64::from(*w),
                                Val::Ref(r) => resolve(*r)?,
                            };
                            put32(&mut b, w, big)?;
                        }
                    }
                    Content::Code(c) => {
                        let mut pl = Placed::new(c);
                        place(c, 0, at, &mut pl, 0)?;
                        emit(c, 0, at, big, &resolve, &pl, &mut b, self)?;
                    }
                }
                splice(&mut buf, at, at + b.len() as i64, &b)?;
            }
            if let Some(slot) = out.get_mut(usize::from(f)) {
                *slot = Some(buf);
            }
        }
        Ok((out, new_at))
    }

    /// `Movie.leftovers()`: (file, text) of bytes no node accounts for and of overlapping nodes.
    ///
    /// # Errors
    /// [`Refused`] where Python would compare two nodes (the same offset and end) or index a missing member.
    pub fn leftovers(&self) -> R<Vec<(u8, String)>> {
        let mut out = Vec::new();
        for (f, lst) in self.reachable() {
            let b = self.member(f)?;
            let mut spans: Vec<(i64, i64, Rid)> = lst
                .iter()
                .filter_map(|&r| {
                    let n = self.nodes.get(r as usize)?;
                    match (n.at, n.size0) {
                        (Some(a), Some(s)) if s != 0 => Some((a, a + s, r)),
                        _ => None,
                    }
                })
                .collect();
            spans.sort_by_key(|&(s, e, _)| (s, e));
            if spans
                .windows(2)
                .any(|w| matches!(w, [x, y] if (x.0, x.1) == (y.0, y.1)))
            {
                return refuse("two nodes with the same span (Python compares the nodes)");
            }
            if spans.iter().any(|&(s, e, _)| s < 0 || e < 0) {
                return refuse("a negative offset");
            }
            let any = |a: i64, z: i64| {
                let a = (a.max(0) as usize).min(b.len());
                let z = (z.max(0) as usize).min(b.len()).max(a);
                b.get(a..z).is_some_and(|s| s.iter().any(|&x| x != 0))
            };
            let mut pos = 0i64;
            for (s, e, n) in spans {
                if s < pos {
                    let tag = self
                        .nodes
                        .get(n as usize)
                        .and_then(|x| self.tags.get(usize::from(x.tag)))
                        .map_or("", String::as_str);
                    out.push((
                        f,
                        format!("{tag} at {s:#x} overlaps the structure before it (ending {pos:#x})"),
                    ));
                } else if any(pos, s) {
                    let lo = (pos as usize).min(b.len());
                    let hi = (s.min(pos + 16) as usize).min(b.len()).max(lo);
                    let hex: Vec<String> = b
                        .get(lo..hi)
                        .unwrap_or_default()
                        .iter()
                        .map(|x| format!("{x:02x}"))
                        .collect();
                    out.push((f, format!("unread bytes {pos:#x}-{s:#x}: {}", hex.join(" "))));
                }
                pos = pos.max(e);
            }
            if any(pos, b.len() as i64) {
                out.push((f, format!("unread bytes at the end {pos:#x}-{:#x}", b.len())));
            }
        }
        Ok(out)
    }
}

fn code_refs(c: &Code, s: u32, out: &mut Vec<Rid>, depth: usize) {
    if depth > MAX_NESTING * 4 {
        return;
    }
    for &k in c.streams.get(s as usize).map_or(&[][..], Vec::as_slice) {
        let Some(i) = c.ins.get(k as usize) else { continue };
        out.extend(i.args.iter().filter_map(|a| match a {
            Arg::Ref(r) => Some(*r),
            _ => None,
        }));
        if let Some(b) = i.body {
            code_refs(c, b, out, depth + 1);
        }
    }
}

impl Placed {
    fn new(c: &Code) -> Placed {
        Placed {
            ins: vec![-1; c.ins.len()],
            ends: vec![-1; c.streams.len()],
        }
    }
}

/// `aptgraft._place`: each instruction's position and each stream's end; returns the end.
fn place(c: &Code, s: u32, mut pos: i64, pl: &mut Placed, depth: usize) -> R<i64> {
    if depth > MAX_NESTING {
        return refuse("bodies nested too deep");
    }
    for &k in c.streams.get(s as usize).map_or(&[][..], Vec::as_slice) {
        let i = c
            .ins
            .get(k as usize)
            .ok_or_else(|| Refused("no such instruction".into()))?;
        if let Some(x) = pl.ins.get_mut(k as usize) {
            *x = pos;
        }
        match kind(i.op) {
            k if k >= 0 => pos += 1 + i64::from(k),
            -1 => pos = ((pos + 4) & !3) + 4,
            -2 => pos = ((pos + 4) & !3) + 8,
            -5 => return refuse("an opcode with an argument this tool does not know"),
            k => {
                let Some(b) = i.body else {
                    return refuse("a function definition without its body");
                };
                pos = place(c, b, ((pos + 4) & !3) + if k == -3 { 24 } else { 28 }, pl, depth + 1)?;
            }
        }
    }
    if let Some(x) = pl.ends.get_mut(s as usize) {
        *x = pos;
    }
    Ok(pos)
}

fn arg(i: &Ins, k: usize) -> R<Arg> {
    i.args
        .get(k)
        .copied()
        .ok_or_else(|| Refused("an argument missing".into()))
}

fn int(a: Arg) -> R<i64> {
    match a {
        Arg::Int(v) => Ok(v),
        _ => refuse("not an integer"),
    }
}

/// `aptgraft._emit`.
#[expect(
    clippy::too_many_arguments,
    reason = "the placement, the resolver and the model, as _emit takes them"
)]
fn emit(
    c: &Code,
    s: u32,
    mut pos: i64,
    big: bool,
    resolve: &dyn Fn(Rid) -> R<i64>,
    pl: &Placed,
    out: &mut Vec<u8>,
    m: &Model,
) -> R<i64> {
    let ptr = |a: Arg| -> R<i64> {
        match a {
            Arg::Ref(r) => resolve(r),
            Arg::Int(v) => Ok(v),
            Arg::Other => refuse("not a pointer or an integer"),
        }
    };
    for &k in c.streams.get(s as usize).map_or(&[][..], Vec::as_slice) {
        let i = c
            .ins
            .get(k as usize)
            .ok_or_else(|| Refused("no such instruction".into()))?;
        let op = i.op;
        out.push(op);
        let mut p = pos + 1;
        let kd = kind(op);
        if kd > 0 {
            let v = int(arg(i, 0)?)?;
            match kd {
                4 => put32(out, v, big)?,
                1 => out.push((v & 0xFF) as u8),
                _ => put16(out, v & 0xFFFF, big)?,
            }
            p += i64::from(kd);
        } else if kd < 0 && kd != -5 {
            let q = (p + 3) & !3;
            out.extend(std::iter::repeat_n(0u8, (q - p) as usize));
            p = q;
            if kd == -1 {
                if is_branch(op) {
                    p += 4;
                    let tgt = match i.target {
                        Target::Ins(j) => pl.ins.get(j as usize).copied().unwrap_or(-1),
                        Target::End(e) => pl.ends.get(e as usize).copied().unwrap_or(-1),
                        _ => -1,
                    };
                    if tgt < 0 {
                        return refuse("a branch with no target in its stream");
                    }
                    put_i32(out, tgt - p, big)?;
                } else {
                    let a = arg(i, 0)?;
                    put32(out, if is_one_str(op) { ptr(a)? } else { int(a)? }, big)?;
                    p += 4;
                }
            } else if kd == -2 {
                if op == 0x88 || op == 0x96 {
                    let a1 = arg(i, 1)?;
                    let n = match a1 {
                        Arg::Ref(r) => match m.nodes.get(r as usize).map(|x| &x.content) {
                            Some(Content::Words(v)) => v.len() as i64,
                            _ => return refuse("a list that is not a words node"),
                        },
                        _ => int(arg(i, 0)?)?,
                    };
                    put32(out, n, big)?;
                    put32(out, ptr(a1)?, big)?;
                } else {
                    put32(out, ptr(arg(i, 0)?)?, big)?;
                    put32(out, ptr(arg(i, 1)?)?, big)?;
                }
                p += 8;
            } else {
                let Some(b) = i.body else {
                    return refuse("a function definition without its body");
                };
                let body = p + if kd == -3 { 24 } else { 28 };
                let size = pl.ends.get(b as usize).copied().unwrap_or(-1) - body;
                if kd == -3 {
                    put32(out, ptr(arg(i, 0)?)?, big)?;
                    put32(out, int(arg(i, 1)?)?, big)?;
                    put32(out, ptr(arg(i, 2)?)?, big)?;
                    put32(out, size, big)?;
                    put32(out, int(arg(i, 3)?)?, big)?;
                    put32(out, int(arg(i, 4)?)?, big)?;
                } else {
                    let r = int(arg(i, 2)?)?;
                    put32(out, ptr(arg(i, 0)?)?, big)?;
                    put32(out, int(arg(i, 1)?)?, big)?;
                    put16(out, r & 0xFFFF, big)?;
                    put16(out, r >> 16, big)?;
                    put32(out, ptr(arg(i, 3)?)?, big)?;
                    put32(out, size, big)?;
                    put32(out, int(arg(i, 4)?)?, big)?;
                    put32(out, int(arg(i, 5)?)?, big)?;
                }
                emit(c, b, body, big, resolve, pl, out, m)?;
                p = body + size;
            }
        }
        pos = p;
    }
    Ok(pos)
}

// ------------------------------------------------------------------------------------------------ roundtrip

/// A movie's BIGF members: (name, bytes) in index order.
pub type Members = Vec<(String, Vec<u8>)>;

fn latin1_bytes(s: &str) -> R<Vec<u8>> {
    s.chars()
        .map(|c| u8::try_from(u32::from(c)).map_err(|_| Refused("a member name outside latin-1".into())))
        .collect()
}

/// `aptgraft.write_bigf`.
///
/// # Errors
/// [`Refused`] for a member name Python cannot encode as latin-1.
pub fn write_bigf(members: &[(String, Vec<u8>)]) -> R<Vec<u8>> {
    let names: Vec<Vec<u8>> = members.iter().map(|(n, _)| latin1_bytes(n)).collect::<R<_>>()?;
    let index: usize = names.iter().map(|n| 8 + n.len() + 1).sum();
    let mut pos = 16 + index;
    let mut offs = Vec::with_capacity(members.len());
    for (_, d) in members {
        pos = (pos + 3) & !3;
        offs.push(pos);
        pos += d.len();
    }
    let mut out = vec![0u8; pos];
    let w32 = |o: &mut Vec<u8>, at: usize, v: u32, big: bool| {
        if let Some(s) = o.get_mut(at..at + 4) {
            s.copy_from_slice(&if big { v.to_be_bytes() } else { v.to_le_bytes() });
        }
    };
    if let Some(s) = out.get_mut(0..4) {
        s.copy_from_slice(b"BIGF");
    }
    w32(&mut out, 4, pos as u32, false);
    w32(&mut out, 8, members.len() as u32, true);
    w32(&mut out, 12, (16 + index) as u32, true);
    let mut ip = 16;
    for (((_, d), n), &o) in members.iter().zip(&names).zip(&offs) {
        w32(&mut out, ip, o as u32, true);
        w32(&mut out, ip + 4, d.len() as u32, true);
        if let Some(s) = out.get_mut(ip + 8..ip + 8 + n.len()) {
            s.copy_from_slice(n);
        }
        ip += 8 + n.len() + 1;
        if let Some(s) = out.get_mut(o..o + d.len()) {
            s.copy_from_slice(d);
        }
    }
    Ok(out)
}

/// The members' files (`Movie.names`): for each member 0 / 1 / 2 (`.apt`, `.const`, `.geo`) or -1, as
/// `Movie.__init__` finds them; `None` where it raises.
#[must_use]
pub fn member_files(members: &[(String, Vec<u8>)]) -> Option<Vec<i8>> {
    let mut seen = HashSet::new();
    if !members.iter().all(|(n, _)| seen.insert(n.as_str())) {
        return None;
    }
    let apt = members.iter().position(|(n, _)| n.to_lowercase().ends_with(".apt"))?;
    let name = members.get(apt)?.0.chars().collect::<Vec<char>>();
    let base: String = name.get(..name.len().checked_sub(4)?)?.iter().collect();
    let find = |ext: &str| {
        let want = format!("{base}.{ext}").to_lowercase();
        members.iter().position(|(n, _)| n.to_lowercase() == want)
    };
    let c = find("const")?;
    let g = find("geo");
    Some(
        (0..members.len())
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
            .collect(),
    )
}

/// `Movie(members)` for a roundtrip: the model and each member's file.
fn load(members: &[(String, Vec<u8>)]) -> R<(Model, Vec<i8>)> {
    let files = member_files(members).ok_or_else(|| Refused("the members".into()))?;
    let get = |f: i8| {
        files
            .iter()
            .position(|&x| x == f)
            .and_then(|k| members.get(k))
            .map(|(_, d)| d.as_slice())
    };
    let (Some(a), Some(c)) = (get(0), get(1)) else {
        return refuse("the members");
    };
    let big = movie_big(a, c).ok_or_else(|| Refused("not an APT constant file".into()))?;
    Ok((Model::parse(a, c, get(2), big)?, files))
}

/// `Movie.write`'s member list: each member's new bytes (an extra member as it was).
fn compose(members: &[(String, Vec<u8>)], files: &[i8], w: &Written) -> R<Members> {
    members
        .iter()
        .zip(files)
        .map(|((n, d), &f)| {
            if f < 0 {
                Ok((n.clone(), d.clone()))
            } else {
                match w.get(f as usize) {
                    Some(Some(b)) => Ok((n.clone(), b.clone())),
                    _ => refuse("a member not written"),
                }
            }
        })
        .collect()
}

/// Python's `str.strip()` on a dump line (latin-1 text).
fn py_strip(s: &str) -> &str {
    let ws = |c: char| {
        matches!(
            c,
            '\t' | '\n' | '\x0b' | '\x0c' | '\r' | '\x1c'..='\x1f' | ' ' | '\u{85}' | '\u{a0}'
        ) || (c > '\u{ff}' && c.is_whitespace())
    };
    s.trim_matches(ws)
}

/// `aptgraft.dump_text`'s pattern removed from one stripped line (`re.sub`, alternatives in order).
fn strip_offsets(s: &str) -> String {
    let b = s.as_bytes();
    let lower = |c: u8| c.is_ascii_digit() || (b'a'..=b'f').contains(&c);
    let anyhex = |c: u8| c.is_ascii_hexdigit();
    let upper = |c: u8| c.is_ascii_digit() || (b'A'..=b'F').contains(&c);
    let run =
        |from: usize, f: &dyn Fn(u8) -> bool| b.get(from..).map_or(0, |r| r.iter().take_while(|&&c| f(c)).count());
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let rest = b.get(i..).unwrap_or_default();
        let m = if rest.starts_with(b"@0x") && run(i + 3, &lower) > 0 {
            3 + run(i + 3, &lower)
        } else if rest.starts_with(b"@") && run(i + 1, &anyhex) > 0 {
            1 + run(i + 1, &anyhex)
        } else if rest.starts_with(b"[list 0x")
            && run(i + 8, &lower) > 0
            && b.get(i + 8 + run(i + 8, &lower)) == Some(&b']')
        {
            8 + run(i + 8, &lower) + 1
        } else if rest.starts_with(b"-> 0x") && run(i + 5, &lower) > 0 {
            5 + run(i + 5, &lower)
        } else if i == 0 && rest.len() >= 8 && rest.iter().take(6).all(|&c| upper(c)) && rest.get(6..8) == Some(b"  ") {
            8
        } else {
            0
        };
        if m > 0 {
            i += m;
        } else {
            out.push(b.get(i).copied().unwrap_or_default());
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_default()
}

/// `aptgraft.dump_text`: apt.py's dump of written members, offsets taken out.
///
/// # Errors
/// [`Refused`] where `apt.py` would raise or decode the movie itself (the decoder refuses it).
pub fn dump_text(members: &[(String, Vec<u8>)]) -> R<Vec<String>> {
    // d = dict(members): the last of equal names
    let mut d: Vec<(&str, &[u8])> = Vec::new();
    for (n, b) in members {
        match d.iter_mut().find(|(x, _)| *x == n.as_str()) {
            Some(e) => e.1 = b,
            None => d.push((n, b)),
        }
    }
    let Some(&(aptname, a)) = d.iter().find(|(n, _)| n.to_lowercase().ends_with(".apt")) else {
        return refuse("no .apt");
    };
    let chars: Vec<char> = aptname.chars().collect();
    let name: String = chars
        .get(..chars.len().saturating_sub(4))
        .unwrap_or_default()
        .iter()
        .collect();
    let cname = format!("{name}.const");
    let Some(&(_, c)) = d.iter().find(|(n, _)| *n == cname) else {
        return refuse("no .const of that exact name");
    };
    if !c.starts_with(b"Apt constant file") {
        return refuse("not an APT constant file");
    }
    let le = i64::from(rd32(c, 20, false)?);
    let be = i64::from(rd32(c, 20, true)?);
    let n = a.len() as i64;
    let mut big = !(0 < le && le < n) && 0 < be && be < n;
    let sig_at = |o: i64, s: [u8; 4]| 0 < o && o <= n - 8 && a.get(o as usize + 4..o as usize + 8) == Some(&s[..]);
    if sig_at(be, [0x09, 0x87, 0x65, 0x43]) && !sig_at(le, [0x43, 0x65, 0x87, 0x09]) {
        big = true;
    }
    let movie = if big { be } else { le };
    let consts = apt::constants(c, big).map_err(|e| Refused(e.to_string()))?;
    let dec = apt::decode(
        apt::MovieRef {
            name: &name,
            apt: a,
            big,
            movie,
            consts: &consts,
        },
        false,
    )
    .map_err(|e| Refused(e.to_string()))?;
    Ok(dec.lines.iter().map(|l| strip_offsets(py_strip(l))).collect())
}

/// `aptgraft.roundtrip`'s (problems, notes) of a movie read as `members` (raw: the container's bytes when it is
/// compared, a `.res` file).
///
/// # Errors
/// [`Refused`] wherever the Python would raise or decode with its own code: `aptgraft.py` then runs it.
pub fn roundtrip(members: &[(String, Vec<u8>)], raw: Option<&[u8]>, pack: bool) -> R<(Vec<String>, Vec<String>)> {
    let orig = write_bigf(members)?;
    let mut problems = Vec::new();
    if raw.is_some_and(|r| r != orig.as_slice()) {
        problems.push("the BIGF container is laid out differently from this writer's".to_string());
    }
    let (m, files) = load(members)?;
    let notes: Vec<String> = m
        .leftovers()?
        .into_iter()
        .map(|(f, t)| format!("{}: {t}", FILES.get(usize::from(f)).copied().unwrap_or("?")))
        .collect();
    problems.extend(notes.iter().filter(|n| n.contains("overlaps")).cloned());
    let keep = [Layout::Keep; 3];
    let out = compose(members, &files, &m.write(m.big, keep, 0, true)?)?;
    for ((n, a), (_, b)) in members.iter().zip(&out) {
        if a != b {
            let k = a
                .iter()
                .zip(b.iter())
                .position(|(x, y)| x != y)
                .unwrap_or(a.len().min(b.len()));
            problems.push(format!(
                "{n}: written differently from offset {k:#x} (sizes {} / {})",
                a.len(),
                b.len()
            ));
        }
    }
    if pack && problems.is_empty() {
        let packed = compose(members, &files, &m.write(m.big, [Layout::Pack; 3], 16, true)?)?;
        if members
            .iter()
            .zip(&packed)
            .zip(&files)
            .any(|(((_, a), (_, b)), &f)| f >= 0 && b.len() <= a.len())
        {
            problems.push("the packed layout did not move the data".into());
        }
        if dump_text(&packed)? != dump_text(members)? {
            problems.push("packed layout: apt.py reads a different movie".into());
        }
        let (m2, f2) = load(&packed)?;
        if !m2.leftovers()?.is_empty() || compose(&packed, &f2, &m2.write(m2.big, keep, 0, true)?)? != packed {
            problems.push("packed layout: not read back completely".into());
        }
        let conv_members = compose(members, &files, &m.write(!m.big, keep, 0, true)?)?;
        let (conv, f3) = load(&conv_members)?;
        if compose(&conv_members, &f3, &conv.write(m.big, keep, 0, true)?)?
            != compose(members, &files, &m.write(m.big, keep, 0, false)?)?
        {
            problems.push("converting to the other byte order and back changes the movie".into());
        }
    }
    Ok((problems, notes))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn strip_like_python() {
        assert_eq!(strip_offsets("00012A  Push @0x1f -> 0x20 [list 0xab] @AB"), "Push    ");
        assert_eq!(strip_offsets("@0x"), "x");
        assert_eq!(py_strip("\x1c a \u{a0}"), "a");
    }

    /// A movie: header, a movie character with one frame holding one action (`PushString "hi"`, `End`), a constant.
    fn movie(big: bool) -> (Vec<u8>, Vec<u8>) {
        let w = |v: u32| if big { v.to_be_bytes() } else { v.to_le_bytes() };
        let mut a = b"Apt Data:74\x1a".to_vec();
        while a.len() < 16 {
            a.push(0);
        }
        // movie at 16: 15 words
        let frames = 16 + 60;
        let items = frames + 8;
        let item = items + 4;
        let code = item + 8;
        let s = code + 12;
        for v in [9, SIG, 1, frames, 0, 0, 0, 640, 480, 33, 0, 0, 0, 0, 0] {
            a.extend(w(v));
        }
        a.extend(w(1));
        a.extend(w(items));
        a.extend(w(item));
        a.extend(w(1));
        a.extend(w(code));
        // code: A1 pad pad pad <ptr> 00 ...
        a.extend([0xA1, 0, 0, 0]);
        a.extend(w(s));
        a.extend([0, 0, 0, 0]);
        a.extend(b"hi\0");
        let mut c = b"Apt constant file\x1a\0\0".to_vec();
        for v in [16, 1, 32, 1, 40] {
            c.extend(w(v));
        }
        c.extend(b"k\0");
        (a, c)
    }

    #[test]
    fn parse_and_write_back() {
        for big in [false, true] {
            let (a, c) = movie(big);
            assert_eq!(movie_big(&a, &c), Some(big));
            let m = Model::parse(&a, &c, None, big).unwrap();
            assert_eq!(m.leftovers().unwrap(), vec![]);
            let k = [Layout::Keep; 3];
            let out = m.write(big, k, 0, true).unwrap();
            assert_eq!(out[0].as_deref(), Some(a.as_slice()));
            assert_eq!(out[1].as_deref(), Some(c.as_slice()));
            // the other byte order and back
            let other = m.write(!big, k, 0, true).unwrap();
            let m2 = Model::parse(other[0].as_ref().unwrap(), other[1].as_ref().unwrap(), None, !big).unwrap();
            assert_eq!(m2.write(big, k, 0, true).unwrap(), out);
            // packed with a gap: longer, and read back the same
            let p = m.write(big, [Layout::Pack; 3], 16, true).unwrap();
            assert!(p[0].as_ref().unwrap().len() > a.len());
            let m3 = Model::parse(p[0].as_ref().unwrap(), p[1].as_ref().unwrap(), None, big).unwrap();
            assert_eq!(m3.leftovers().unwrap(), vec![]);
        }
    }

    #[test]
    fn refuses_what_python_raises() {
        let (a, c) = movie(false);
        assert!(Model::parse(&a[..a.len() - 6], &c, None, false).is_err());
        let mut bad = a.clone();
        bad[96] = 0xC3; // an opcode with an argument nobody knows
        assert!(Model::parse(&bad, &c, None, false).is_err());
    }

    #[test]
    fn grown_node_moves_in_keep() {
        let (a, c) = movie(false);
        let mut m = Model::parse(&a, &c, None, false).unwrap();
        let s = *m.cache.get(&(APT, 108, S)).unwrap();
        let mut n = m.nodes[s as usize].clone();
        n.content = Content::Str(b"hello".to_vec());
        m.set_node(s, n).unwrap();
        let out = m.write(false, [Layout::Keep; 3], 0, true).unwrap();
        let o = out[0].as_ref().unwrap();
        assert_eq!(&o[o.len() - 6..], b"hello\0");
        assert_eq!(&o[108..111], b"\0\0\0");
    }
}
