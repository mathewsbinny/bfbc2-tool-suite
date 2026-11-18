//! EA "APT" UI movies (the menus and HUDs, PC and Xbox 360): the dump and the instruction records of
//! `bfbc2-console-readers/apt/apt.py`, ported (round E3n).
//!
//! The layout is in `apt.py`'s module text. This port gives `Movie.dump()`'s lines and `Movie.instructions()`'
//! records (each instruction the dump decodes, once, in dump order, with its stream's start, limit and depth) for a
//! movie whose byte order, movie offset and constants `apt.Movie.__init__` found (or [`constants`] gives).
//!
//! It refuses ([`AptError`]) every input where the Python reader would do anything but decode in bounds: a read
//! past the end or at a negative offset (Python's `unpack_from` and indexing count those from the end, or raise
//! `struct.error` / `IndexError`, which the dump writes into its text for frame items), function nesting deeper
//! than [`MAX_NESTING`] (Python's recursion limit). `apt.py` then runs its own code on that movie, so the answer
//! is always the reference's.
//!
//! | Python (`apt.py`) | here |
//! |---|---|
//! | `Movie.__init__`'s constant list | [`constants`] |
//! | `Movie.dump()` | [`decode`] with `records: false` |
//! | `Movie.instructions()` (and `dump_lines`) | [`decode`] with `records: true` |
//! | the same for many movies | [`decode_many`] (all cores) |
//!
//! ```
//! use bc2_formats::apt;
//!
//! // "Apt constant file", the movie at 0x10, 1 constant: the string at 0x28
//! let mut c = b"Apt constant file\x1a\0\0".to_vec();
//! for v in [0x10u32, 1, 0x20, 1, 0x28] {
//!     c.extend(v.to_le_bytes());
//! }
//! c.extend(b"hello\0");
//! assert_eq!(apt::constants(&c, false)?, vec!["\"hello\"".to_string()]);
//! assert_eq!(apt::format_g(0.1f32.into()), "0.1");
//! assert_eq!(apt::format_g(1_234_565.0), "1.23456e+06");
//! # Ok::<(), apt::AptError>(())
//! ```

use std::collections::HashSet;
use std::fmt::Write as _;
use std::rc::Rc;
use std::sync::LazyLock;

use rayon::prelude::*;
use thiserror::Error;

/// The character signature after a character's type.
pub const SIG: u32 = 0x0987_6543;

/// The deepest function nesting decoded here; deeper movies go to the Python reader (whose recursion limit decides).
pub const MAX_NESTING: usize = 64;

/// Why this port hands a movie to the Python reader.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AptError {
    /// A read outside the file (Python would raise, or read from the end for a negative offset).
    #[error("a read at {at} of {size} byte(s) outside the {len}-byte file")]
    Outside {
        /// The offset.
        at: i64,
        /// The read's size.
        size: usize,
        /// The file's size.
        len: usize,
    },
    /// Function bodies nested deeper than [`MAX_NESTING`].
    #[error("function bodies nested deeper than {MAX_NESTING}")]
    TooDeep,
}

type Result<T> = std::result::Result<T, AptError>;

/// An opcode's name (`apt.NAMES`), or `None` for an opcode the reader does not know.
#[must_use]
#[expect(clippy::too_many_lines, reason = "one arm per opcode, as apt.NAMES lists them")]
pub fn op_name(op: u8) -> Option<&'static str> {
    Some(match op {
        0x00 => "End",
        0x04 => "NextFrame",
        0x05 => "PrevFrame",
        0x06 => "Play",
        0x07 => "Stop",
        0x0A => "Add",
        0x0B => "Subtract",
        0x0C => "Multiply",
        0x0D => "Divide",
        0x0E => "Equals",
        0x0F => "Less",
        0x10 => "And",
        0x11 => "Or",
        0x12 => "Not",
        0x13 => "StringEquals",
        0x14 => "StringLength",
        0x15 => "StringExtract",
        0x17 => "Pop",
        0x18 => "ToInteger",
        0x1C => "GetVariable",
        0x1D => "SetVariable",
        0x20 => "SetTarget2",
        0x21 => "StringAdd",
        0x22 => "GetProperty",
        0x23 => "SetProperty",
        0x24 => "CloneSprite",
        0x25 => "RemoveSprite",
        0x26 => "Trace",
        0x27 => "StartDrag",
        0x28 => "EndDrag",
        0x29 => "StringLess",
        0x2A => "Throw",
        0x2B => "CastOp",
        0x2C => "ImplementsOp",
        0x30 => "RandomNumber",
        0x34 => "GetTime",
        0x3A => "Delete",
        0x3B => "Delete2",
        0x3C => "DefineLocal",
        0x3D => "CallFunction",
        0x3E => "Return",
        0x3F => "Modulo",
        0x40 => "NewObject",
        0x41 => "DefineLocal2",
        0x42 => "InitArray",
        0x43 => "InitObject",
        0x44 => "TypeOf",
        0x45 => "TargetPath",
        0x46 => "Enumerate",
        0x47 => "Add2",
        0x48 => "Less2",
        0x49 => "Equals2",
        0x4A => "ToNumber",
        0x4B => "ToString",
        0x4C => "PushDuplicate",
        0x4D => "StackSwap",
        0x4E => "GetMember",
        0x4F => "SetMember",
        0x50 => "Increment",
        0x51 => "Decrement",
        0x52 => "CallMethod",
        0x53 => "NewMethod",
        0x54 => "InstanceOf",
        0x55 => "Enumerate2",
        0x56 => "EA_PushThis",
        0x57 => "EA_PushGlobal",
        0x58 => "EA_ZeroVar",
        0x59 => "EA_PushZero",
        0x5A => "EA_PushOne",
        0x5B => "EA_PushNull",
        0x5C => "EA_PushUndefined",
        0x5D => "EA_CallMethodPop",
        0x60 => "BitAnd",
        0x61 => "BitOr",
        0x62 => "BitXor",
        0x63 => "BitLShift",
        0x64 => "BitRShift",
        0x65 => "BitURShift",
        0x66 => "StrictEquals",
        0x67 => "Greater",
        0x68 => "StringGreater",
        0x69 => "Extends",
        0x70 => "EA_PushThisVar",
        0x71 => "EA_PushGlobalVar",
        0x72 => "EA_ZeroVar2",
        0x73 => "EA_PushTrue",
        0x74 => "EA_PushFalse",
        0x75 => "EA_PushNull2",
        0x76 => "EA_PushUndefined2",
        0x81 => "GotoFrame",
        0x83 => "GetURL",
        0x87 => "SetRegister",
        0x88 => "ConstantPool",
        0x8A => "WaitForFrame",
        0x8B => "SetTarget",
        0x8C => "GotoLabel",
        0x8D => "WaitForFrame2",
        0x8E => "DefineFunction2",
        0x8F => "Try",
        0x94 => "With",
        0x96 => "PushData",
        0x99 => "BranchAlways",
        0x9A => "GetURL2",
        0x9B => "DefineFunction",
        0x9D => "BranchIfTrue",
        0x9E => "CallFrame",
        0x9F => "GotoFrame2",
        0xA1 => "EA_PushString",
        0xA2 => "EA_PushConstant",
        0xA3 => "EA_PushWordConstant",
        0xA4 => "EA_GetStringVar",
        0xA5 => "EA_GetStringMember",
        0xA6 => "EA_SetStringVar",
        0xA7 => "EA_SetStringMember",
        0xAE => "EA_PushValueOfVar",
        0xAF => "EA_GetNamedMember",
        0xB0 => "EA_CallNamedFuncPop",
        0xB1 => "EA_CallNamedFunc",
        0xB2 => "EA_CallNamedMethodPop",
        0xB3 => "EA_CallNamedMethod",
        0xB4 => "EA_PushFloat",
        0xB5 => "EA_PushByte",
        0xB6 => "EA_PushShort",
        0xB7 => "EA_PushLong",
        0xB8 => "EA_BranchIfFalse",
        0xB9 => "EA_PushRegister",
        _ => return None,
    })
}

/// The name the dump shows: [`op_name`], or `op_XX`.
#[must_use]
pub fn display_name(op: u8) -> String {
    op_name(op).map_or_else(|| format!("op_{op:02X}"), str::to_string)
}

/// [`display_name`] of every opcode, made once.
static NAMES: LazyLock<Vec<String>> = LazyLock::new(|| (0..=255u8).map(display_name).collect());

fn name_of(op: u8) -> &'static str {
    NAMES.get(usize::from(op)).map_or("", String::as_str)
}

/// A character type's name (`apt.CHAR_TYPES`).
fn char_type(t: u32) -> Option<&'static str> {
    Some(match t {
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
        _ => return None,
    })
}

/// Python's `format(x, 'g')`: six significant digits, trailing zeros dropped, scientific notation (two exponent
/// digits at least) below 1e-4 or from 1e6.
#[must_use]
pub fn format_g(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x < 0.0 { "-inf".into() } else { "inf".into() };
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0".into() } else { "0".into() };
    }
    // exactly rounded to 6 significant digits (ties to even, as Python's dtoa): "d.ddddde<exp>"
    let s = format!("{:.5e}", x.abs());
    let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let digits: Vec<u8> = mant.bytes().filter(u8::is_ascii_digit).collect();
    let mut out = String::new();
    if x < 0.0 {
        out.push('-');
    }
    if (-4..6).contains(&exp) {
        // fixed: the point after digit exp + 1 (zeros before when exp < 0)
        let mut text = String::new();
        if exp < 0 {
            text.push_str("0.");
            for _ in 0..(-exp - 1) {
                text.push('0');
            }
            text.extend(digits.iter().map(|&d| char::from(d)));
        } else {
            let point = exp as usize + 1;
            for (k, &d) in digits.iter().enumerate() {
                if k == point {
                    text.push('.');
                }
                text.push(char::from(d));
            }
        }
        if text.contains('.') {
            let t = text.trim_end_matches('0').trim_end_matches('.');
            out.push_str(t);
        } else {
            out.push_str(&text);
        }
    } else {
        let mut m: String = String::new();
        for (k, &d) in digits.iter().enumerate() {
            if k == 1 {
                m.push('.');
            }
            m.push(char::from(d));
        }
        let m = m.trim_end_matches('0').trim_end_matches('.');
        out.push_str(m);
        let _ = write!(out, "e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.unsigned_abs());
    }
    out
}

/// A function definition (`apt.Func`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    /// The name (`"null"` for a null pointer, empty for an empty string).
    pub name: String,
    /// The parameter count.
    pub nparams: i32,
    /// `" regs=N"` for `DefineFunction2`, else empty.
    pub extra: String,
    /// The body's size.
    pub size: i32,
    /// The body's start.
    pub body: i64,
}

/// A decoded instruction (`apt.Ins`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    /// Its offset in the `.apt`.
    pub at: i64,
    /// The opcode.
    pub op: u8,
    /// The argument as the dump shows it.
    pub arg: String,
    /// The next instruction (a function definition's body start).
    pub end: i64,
    /// A branch's target.
    pub target: Option<i64>,
    /// `PushData`'s list.
    pub plist: Option<u32>,
    /// `PushData`'s / `ConstantPool`'s constant indices.
    pub items: Option<Vec<u32>>,
    /// A function definition.
    pub func: Option<Function>,
    /// The constant-pool index of A2 / A3 / AE-B3.
    pub index: Option<u32>,
    /// An opcode `>= 0x80` the reader does not know.
    pub unknown: bool,
    /// The pool's length after the instruction.
    pub pool: usize,
}

/// One record of `Movie.instructions()`: the instruction, its stream's start and limit, its depth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// The instruction.
    pub ins: Instruction,
    /// The start of its stream.
    pub start: i64,
    /// The stream's limit (the file's end for a frame action).
    pub limit: i64,
    /// 0 for a frame action's stream, a function body's: its definition's depth + 1.
    pub depth: u32,
}

/// A movie's dump and (when asked) records.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Decoded {
    /// `Movie.dump()`'s lines.
    pub lines: Vec<String>,
    /// `Movie.instructions()`' records (empty unless asked).
    pub records: Vec<Record>,
}

/// One movie to decode: what `apt.Movie.__init__` found.
#[derive(Debug, Clone, Copy)]
pub struct MovieRef<'a> {
    /// The movie's name (the dump's first line).
    pub name: &'a str,
    /// The `.apt` bytes.
    pub apt: &'a [u8],
    /// Big-endian (Xbox 360).
    pub big: bool,
    /// The movie character's offset.
    pub movie: i64,
    /// The constants (`Movie.const`).
    pub consts: &'a [String],
}

fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| char::from(c)).collect()
}

/// `Movie.cstr`: the C string at `p` (latin-1), `"null"` for 0 or past the end.
fn cstr(buf: &[u8], p: i64) -> String {
    if p <= 0 || p as u64 >= buf.len() as u64 {
        return "null".into();
    }
    let rest = buf.get(p as usize..).unwrap_or_default();
    let e = rest.iter().position(|&c| c == 0).unwrap_or(rest.len());
    latin1(rest.get(..e).unwrap_or_default())
}

/// The word at `o` of `buf` in the byte order, or [`AptError::Outside`].
fn word(buf: &[u8], o: i64, big: bool) -> Result<[u8; 4]> {
    let w = usize::try_from(o)
        .ok()
        .and_then(|o| buf.get(o..)?.first_chunk::<4>().copied())
        .ok_or(AptError::Outside {
            at: o,
            size: 4,
            len: buf.len(),
        })?;
    Ok(if big { [w[3], w[2], w[1], w[0]] } else { w })
}

/// `Movie.__init__`'s constants of a `.const` file: `"text"`, `regN` or `typeT:V`.
///
/// # Errors
/// [`AptError::Outside`] where Python raises `struct.error`.
pub fn constants(c: &[u8], big: bool) -> Result<Vec<String>> {
    let cu = |o: i64| word(c, o, big).map(u32::from_le_bytes);
    let (count, entries) = (cu(24)?, i64::from(cu(28)?));
    // every entry must be readable (Python raises at the first that is not)
    let last = entries + 8 * i64::from(count);
    if count > 0 && (last > c.len() as i64) {
        return Err(AptError::Outside {
            at: last - 4,
            size: 4,
            len: c.len(),
        });
    }
    let mut out = Vec::with_capacity(count as usize);
    for i in 0..i64::from(count) {
        let (t, v) = (cu(entries + 8 * i)?, cu(entries + 8 * i + 4)?);
        out.push(match t {
            1 => format!("\"{}\"", cstr(c, i64::from(v))),
            4 => format!("reg{v}"),
            _ => format!("type{t}:{v}"),
        });
    }
    Ok(out)
}

/// The constant pool: the constant indices of the last `ConstantPool` (shared, cheap to copy).
type Pool = Rc<Vec<u32>>;

/// A record before its depth is known: the instruction, its stream's start and limit.
type Raw = (Instruction, i64, i64);

struct Dumper<'a> {
    m: MovieRef<'a>,
    a: &'a [u8],
    len: i64,
    out: Vec<String>,
    records: Option<(HashSet<i64>, Vec<Raw>)>,
    nesting: usize,
}

impl Dumper<'_> {
    fn u(&self, o: i64) -> Result<u32> {
        word(self.a, o, self.m.big).map(u32::from_le_bytes)
    }
    fn i(&self, o: i64) -> Result<i32> {
        word(self.a, o, self.m.big).map(i32::from_le_bytes)
    }
    fn f(&self, o: i64) -> Result<f64> {
        word(self.a, o, self.m.big).map(|w| f64::from(f32::from_le_bytes(w)))
    }
    fn byte(&self, o: i64) -> Result<u8> {
        usize::try_from(o)
            .ok()
            .and_then(|o| self.a.get(o).copied())
            .ok_or(AptError::Outside {
                at: o,
                size: 1,
                len: self.a.len(),
            })
    }
    fn half(&self, o: i64) -> Result<u16> {
        let b = usize::try_from(o)
            .ok()
            .and_then(|o| self.a.get(o..)?.first_chunk::<2>().copied())
            .ok_or(AptError::Outside {
                at: o,
                size: 2,
                len: self.a.len(),
            })?;
        Ok(if self.m.big {
            u16::from_be_bytes(b)
        } else {
            u16::from_le_bytes(b)
        })
    }
    fn s(&self, p: i64) -> String {
        cstr(self.a, p)
    }
    fn konst(&self, g: u32, missing: &str) -> String {
        self.m
            .consts
            .get(g as usize)
            .cloned()
            .unwrap_or_else(|| format!("{missing}{g}"))
    }

    /// `Movie.decode`: the instruction at `p` and the pool after it.
    #[allow(clippy::too_many_lines)]
    fn decode(&self, p: i64, pool: &Pool) -> Result<(Instruction, Pool)> {
        let at = p;
        let op = self.byte(p)?;
        let mut p = p + 1;
        let mut ins = Instruction {
            at,
            op,
            arg: String::new(),
            end: p,
            target: None,
            plist: None,
            items: None,
            func: None,
            index: None,
            unknown: false,
            pool: pool.len(),
        };
        if op < 0x80 {
            return Ok((ins, pool.clone()));
        }
        let al = |p: i64| (p + 3) & !3;
        let mut pool = pool.clone();
        let pooled = |pool: &Pool, index: u32| {
            pool.get(index as usize)
                .map_or_else(|| format!("pool[{index}]?"), |&g| self.konst(g, "?"))
        };
        match op {
            0xB7 => {
                ins.arg = self.i(p)?.to_string();
                p += 4;
            }
            0x81 | 0x87 | 0x99 | 0x9D | 0xB8 | 0x94 | 0x9F => {
                p = al(p);
                let v = self.i(p)?;
                p += 4;
                if matches!(op, 0x99 | 0x9D | 0xB8) {
                    let t = p + i64::from(v);
                    ins.target = Some(t);
                    ins.arg = format!("{v:+} -> {}", hex(t));
                } else {
                    ins.arg = v.to_string();
                }
            }
            0x83 => {
                p = al(p);
                ins.arg = format!(
                    "\"{}\", \"{}\"",
                    self.s(i64::from(self.u(p)?)),
                    self.s(i64::from(self.u(p + 4)?))
                );
                p += 8;
            }
            0x88 => {
                p = al(p);
                let (n, ptr) = (self.u(p)?, i64::from(self.u(p + 4)?));
                p += 8;
                if n > 0 {
                    self.u(ptr + 4 * (i64::from(n) - 1))?;
                }
                let items = (0..i64::from(n))
                    .map(|k| self.u(ptr + 4 * k))
                    .collect::<Result<Vec<_>>>()?;
                pool = Rc::new(items.clone());
                ins.items = Some(items);
                ins.arg = format!("{n} constants");
            }
            0x8A => {
                p = al(p);
                ins.arg = format!("frame {} skip {}", self.i(p)?, self.i(p + 4)?);
                p += 8;
            }
            0x8B | 0x8C | 0xA1 | 0xA4 | 0xA5 | 0xA6 | 0xA7 => {
                p = al(p);
                ins.arg = format!("\"{}\"", self.s(i64::from(self.u(p)?)));
                p += 4;
            }
            0x96 => {
                p = al(p);
                let (n, plist) = (self.u(p)?, self.u(p + 4)?);
                p += 8;
                let base = i64::from(plist);
                let mut items = (0..i64::from(n.min(32)))
                    .map(|k| self.u(base + 4 * k))
                    .collect::<Result<Vec<_>>>()?;
                for k in 32..i64::from(n.min(4096)) {
                    if base + 4 * k + 4 <= self.len {
                        items.push(self.u(base + 4 * k)?);
                    }
                }
                let mut arg = String::new();
                for (k, &g) in items.iter().take(32).enumerate() {
                    if k > 0 {
                        arg.push_str(", ");
                    }
                    arg.push_str(&self.konst(g, "#"));
                }
                let _ = write!(arg, "   [list {}]", hex(i64::from(plist)));
                ins.arg = arg;
                ins.plist = Some(plist);
                ins.items = Some(items);
            }
            0x8D => {
                ins.arg = self.byte(p)?.to_string();
                p += 1;
            }
            0xA2 | 0xAE | 0xAF | 0xB0 | 0xB1 | 0xB2 | 0xB3 => {
                let index = u32::from(self.byte(p)?);
                ins.index = Some(index);
                ins.arg = pooled(&pool, index);
                p += 1;
            }
            0xA3 => {
                let index = u32::from(self.half(p)?);
                ins.index = Some(index);
                ins.arg = pooled(&pool, index);
                p += 2;
            }
            0xB4 => {
                ins.arg = format_g(self.f(p)?);
                p += 4;
            }
            0xB5 => {
                ins.arg = (self.byte(p)? as i8).to_string();
                p += 1;
            }
            0xB6 => {
                ins.arg = (self.half(p)? as i16).to_string();
                p += 2;
            }
            0xB9 => {
                ins.arg = format!("r{}", self.byte(p)?);
                p += 1;
            }
            0x9B | 0x8E => {
                p = al(p);
                let name = self.s(i64::from(self.u(p)?));
                let nparams = self.i(p + 4)?;
                let (extra, size, body) = if op == 0x8E {
                    let regs = self.byte(if self.m.big { p + 9 } else { p + 8 })?;
                    (format!(" regs={regs}"), self.i(p + 16)?, p + 28)
                } else {
                    (String::new(), self.i(p + 12)?, p + 24)
                };
                ins.func = Some(Function {
                    name,
                    nparams,
                    extra,
                    size,
                    body,
                });
                p = body;
            }
            _ => {}
        }
        ins.end = p;
        ins.unknown = op_name(op).is_none();
        ins.pool = pool.len();
        Ok((ins, pool))
    }

    /// `Movie.disasm`.
    fn disasm(&mut self, start: i64, indent: &str, limit: Option<i64>, pool: &Pool) -> Result<()> {
        let mut pool = pool.clone();
        let mut p = start;
        let limit = limit.unwrap_or(self.len);
        let mut guard = 0;
        while p < self.len && p < limit && guard < 200_000 {
            guard += 1;
            let (ins, next) = self.decode(p, &pool)?;
            pool = next;
            let name = name_of(ins.op);
            if let Some(f) = &ins.func {
                let (body, end) = (f.body, f.body + i64::from(f.size));
                self.out.push(format!(
                    "{indent}{:06X}  {name} {}({} params){} size={}",
                    ins.at,
                    if f.name.is_empty() { "<anon>" } else { &f.name },
                    f.nparams,
                    f.extra,
                    f.size
                ));
                self.record(ins, start, limit);
                self.nesting += 1;
                if self.nesting > MAX_NESTING {
                    return Err(AptError::TooDeep);
                }
                self.disasm(body, &format!("{indent}    "), Some(end), &pool)?;
                self.nesting -= 1;
                p = end;
                continue;
            }
            if ins.unknown {
                self.out.push(format!(
                    "{indent}{:06X}  {name}  ** unknown opcode with argument - stopping",
                    ins.at
                ));
                self.record(ins, start, limit);
                return Ok(());
            }
            if ins.arg.is_empty() {
                self.out.push(format!("{indent}{:06X}  {name}", ins.at));
            } else {
                self.out.push(format!("{indent}{:06X}  {name} {}", ins.at, ins.arg));
            }
            p = ins.end;
            let stop = ins.op == 0 && limit == self.len;
            self.record(ins, start, limit);
            if stop {
                return Ok(());
            }
        }
        Ok(())
    }

    fn record(&mut self, ins: Instruction, start: i64, limit: i64) {
        if let Some((seen, recs)) = &mut self.records
            && seen.insert(ins.at)
        {
            recs.push((ins, start, limit));
        }
    }

    fn frames(&mut self, ptr: i64, count: u32, indent: &str, owner: &str) -> Result<()> {
        for fr in 0..i64::from(count) {
            let (n, items) = (self.u(ptr + 8 * fr)?, self.u(ptr + 8 * fr + 4)?);
            self.out.push(format!("{indent}{owner} frame {fr} ({n} items)"));
            let ind = format!("{indent}  ");
            for k in 0..i64::from(n) {
                self.item(i64::from(items), k, &ind)?;
            }
        }
        Ok(())
    }

    fn item(&mut self, items: i64, k: i64, ind: &str) -> Result<()> {
        let it = i64::from(self.u(items + 4 * k)?);
        let t = self.u(it)?;
        let empty: Pool = Rc::new(Vec::new());
        match t {
            1 => {
                let code = self.u(it + 4)?;
                self.out.push(format!("{ind}Action @{code:X}"));
                self.disasm(i64::from(code), &format!("{ind}    "), None, &empty)?;
            }
            2 => {
                let line = format!(
                    "{ind}FrameLabel \"{}\" frame {}",
                    self.s(i64::from(self.u(it + 4)?)),
                    self.i(it + 12)?
                );
                self.out.push(line);
            }
            3 => {
                let nm = self.s(i64::from(self.u(it + 0x34)?));
                let mut line = format!(
                    "{ind}PlaceObject depth={} char={} flags={:#x}",
                    self.i(it + 8)?,
                    self.i(it + 12)?,
                    self.u(it + 4)?
                );
                if nm != "null" {
                    let _ = write!(line, " name=\"{nm}\"");
                }
                let _ = write!(
                    line,
                    " @{it:X} x={} y={}",
                    format_g(self.f(it + 0x20)?),
                    format_g(self.f(it + 0x24)?)
                );
                self.out.push(line);
                let clip = i64::from(self.u(it + 0x3C)?);
                if clip != 0 && clip < self.len {
                    let (cn, cpp) = (self.u(clip)?, i64::from(self.u(clip + 4)?));
                    for c in 0..i64::from(cn.min(64)) {
                        let ev = cpp + 12 * c;
                        let code = self.u(ev + 8)?;
                        let line = format!(
                            "{ind}    ClipEvent flags={:#x} key={} @{code:X}",
                            self.u(ev)?,
                            self.u(ev + 4)?
                        );
                        self.out.push(line);
                        self.disasm(i64::from(code), &format!("{ind}        "), None, &empty)?;
                    }
                }
            }
            4 => {
                let line = format!("{ind}RemoveObject depth={}", self.i(it + 4)?);
                self.out.push(line);
            }
            5 => self.out.push(format!("{ind}BackgroundColor")),
            8 => {
                let (sprite, code) = (self.i(it + 4)?, self.u(it + 8)?);
                self.out.push(format!("{ind}InitAction sprite={sprite} @{code:X}"));
                self.disasm(i64::from(code), &format!("{ind}    "), None, &empty)?;
            }
            _ => self.out.push(format!("{ind}item type {t} @{it:X}")),
        }
        Ok(())
    }

    fn dump(&mut self) -> Result<()> {
        let m = self.m.movie;
        let (frames, fptr) = (self.u(m + 8)?, i64::from(self.u(m + 12)?));
        let head = format!(
            "Movie {} {}x{} frames={frames} characters={} ({})",
            self.m.name,
            self.i(m + 28)?,
            self.i(m + 32)?,
            self.u(m + 20)?,
            if self.m.big {
                "big-endian, Xbox 360"
            } else {
                "little-endian, PC"
            }
        );
        self.out.push(head);
        for k in 0..i64::from(self.u(m + 40)?) {
            let e = i64::from(self.u(m + 44)?) + 16 * k;
            let line = format!(
                "  import \"{}\" :: \"{}\" -> char {}",
                self.s(i64::from(self.u(e)?)),
                self.s(i64::from(self.u(e + 4)?)),
                self.u(e + 8)?
            );
            self.out.push(line);
        }
        for k in 0..i64::from(self.u(m + 48)?) {
            let e = i64::from(self.u(m + 52)?) + 8 * k;
            let line = format!(
                "  export \"{}\" = char {}",
                self.s(i64::from(self.u(e)?)),
                self.u(e + 4)?
            );
            self.out.push(line);
        }
        self.frames(fptr, frames, "", "Movie")?;
        // characters(): the whole table first
        let (count, ptr) = (self.u(m + 20)?, i64::from(self.u(m + 24)?));
        let mut chars = Vec::with_capacity(count as usize);
        for c in 0..i64::from(count) {
            let cp = i64::from(self.u(ptr + 4 * c)?);
            let t = if cp == 0 || cp + 8 > self.len || self.u(cp + 4)? != SIG {
                None
            } else {
                Some(self.u(cp)?)
            };
            chars.push((c, cp, t));
        }
        for (c, cp, t) in chars {
            let Some(t) = t else { continue };
            let line = match t {
                5 => format!("char {c}: sprite @{} frames={}", hex(cp), self.u(cp + 8)?),
                7 => format!("char {c}: image @{} texture={}", hex(cp), self.u(cp + 8)?),
                1 => {
                    let b = [self.f(cp + 8)?, self.f(cp + 12)?, self.f(cp + 16)?, self.f(cp + 20)?].map(format_g);
                    format!(
                        "char {c}: shape @{} bounds=({},{})-({},{}) geometry={}",
                        hex(cp),
                        b[0],
                        b[1],
                        b[2],
                        b[3],
                        self.u(cp + 24)?
                    )
                }
                9 => format!("char {c}: movie @{} frames={}", hex(cp), self.u(cp + 8)?),
                _ => {
                    let name = char_type(t).map_or_else(|| format!("type{t}"), str::to_string);
                    format!("char {c}: {name} @{}", hex(cp))
                }
            };
            self.out.push(line);
            if t == 5 {
                let (fp, n) = (i64::from(self.u(cp + 12)?), self.u(cp + 8)?);
                self.frames(fp, n, "  ", &format!("Sprite {c}"))?;
            }
        }
        Ok(())
    }
}

/// Python's `f'{v:#x}'` (a minus sign before the prefix).
fn hex(v: i64) -> String {
    if v < 0 {
        format!("-{:#x}", v.unsigned_abs())
    } else {
        format!("{v:#x}")
    }
}

/// `Movie.dump()`'s lines and, with `records`, `Movie.instructions()`' records.
///
/// # Errors
/// [`AptError`] for a movie this port hands to the Python reader (module text).
pub fn decode(m: MovieRef<'_>, records: bool) -> Result<Decoded> {
    let mut d = Dumper {
        m,
        a: m.apt,
        len: m.apt.len() as i64,
        out: Vec::new(),
        records: records.then(|| (HashSet::new(), Vec::new())),
        nesting: 0,
    };
    d.dump()?;
    let recs = d.records.take().map_or_else(Vec::new, |(_, r)| r);
    // depth: 0 for streams no function contains; a body's = its definition's stream's + 1 (in record order)
    let mut depth: std::collections::HashMap<i64, u32> = std::collections::HashMap::new();
    for (ins, start, _) in &recs {
        let ds = *depth.entry(*start).or_insert(0);
        if let Some(f) = &ins.func {
            depth.insert(f.body, ds + 1);
        }
    }
    let records = recs
        .into_iter()
        .map(|(ins, start, limit)| {
            let depth = depth.get(&start).copied().unwrap_or(0);
            Record {
                ins,
                start,
                limit,
                depth,
            }
        })
        .collect();
    Ok(Decoded { lines: d.out, records })
}

/// [`decode`] for many movies on all cores, in order.
#[must_use]
pub fn decode_many(movies: &[MovieRef<'_>], records: bool) -> Vec<Result<Decoded>> {
    movies.par_iter().map(|m| decode(*m, records)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g_format() {
        // Python's format(x, 'g') for these values
        let cases: [(f64, &str); 18] = [
            (0.0, "0"),
            (-0.0, "-0"),
            (1.0, "1"),
            (-2.5, "-2.5"),
            (0.1, "0.1"),
            (100_000.0, "100000"),
            (999_999.0, "999999"),
            (999_999.5, "1e+06"),
            (1_000_000.0, "1e+06"),
            (1_234_565.0, "1.23456e+06"),
            (1_234_575.0, "1.23458e+06"),
            (0.0001, "0.0001"),
            (0.000_012_5, "1.25e-05"),
            (123.456_789, "123.457"),
            (1e300, "1e+300"),
            (f64::INFINITY, "inf"),
            (f64::NEG_INFINITY, "-inf"),
            (f64::NAN, "nan"),
        ];
        for (x, want) in cases {
            assert_eq!(format_g(x), want, "{x}");
        }
        assert_eq!(format_g(f64::from(0.1f32)), "0.1");
        assert_eq!(format_g(f64::from(1280.5f32)), "1280.5");
    }

    #[test]
    fn hex_like_python() {
        assert_eq!(hex(0), "0x0");
        assert_eq!(hex(255), "0xff");
        assert_eq!(hex(-16), "-0x10");
    }

    fn movie_bytes() -> Vec<u8> {
        // header, the movie at 0x10 with one frame holding one action, no characters but itself
        let mut a = b"Apt Data:74\x1a\0\0\0\0".to_vec();
        let mv = a.len() as u32; // 0x10
        let frames = mv + 60;
        let items = frames + 8;
        let action = items + 4;
        let code = action + 8;
        let table = code + 12;
        for v in [9, SIG, 1, frames, 0, 1, table, 1280, 720, 33, 0, 0, 0, 0, 0] {
            a.extend(u32::to_le_bytes(v));
        }
        for v in [1, items, action, 1, code] {
            a.extend(u32::to_le_bytes(v));
        }
        // Play; EA_PushByte -3; BranchAlways +0 (aligned); End
        a.extend([0x06, 0xB5, 0xFD, 0x99]);
        a.extend(0i32.to_le_bytes());
        a.extend([0x00, 0, 0, 0]);
        a.extend(mv.to_le_bytes());
        a
    }

    #[test]
    fn dump_and_records() -> Result<()> {
        let a = movie_bytes();
        let consts = Vec::new();
        let m = MovieRef {
            name: "t",
            apt: &a,
            big: false,
            movie: 0x10,
            consts: &consts,
        };
        let d = decode(m, true)?;
        let want = [
            "Movie t 1280x720 frames=1 characters=1 (little-endian, PC)",
            "Movie frame 0 (1 items)",
            "  Action @60",
            "      000060  Play",
            "      000061  EA_PushByte -3",
            "      000063  BranchAlways +0 -> 0x68",
            "      000068  End",
            "char 0: movie @0x10 frames=1",
        ];
        assert_eq!(d.lines, want);
        assert_eq!(d.records.len(), 4);
        assert_eq!(d.records.get(2).and_then(|r| r.ins.target), Some(0x68));
        assert!(d.records.iter().all(|r| r.depth == 0 && r.start == 0x60));
        Ok(())
    }

    #[test]
    fn refuses_reads_outside() {
        let a = movie_bytes();
        let consts = Vec::new();
        let short = a.get(..0x50).unwrap_or_default();
        let m = MovieRef {
            name: "t",
            apt: short,
            big: false,
            movie: 0x10,
            consts: &consts,
        };
        assert!(decode(m, false).is_err());
    }
}
