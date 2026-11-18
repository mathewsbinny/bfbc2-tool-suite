//! The crash reader's and exeref's exe (`crashreader.Image`, `exeref.Exe`, `exeref.window` / `annotate`,
//! `crashreader.name_of`): function starts from the int3 padding, the code around an address, the call before a
//! return address, C strings, the instructions that use an address (as an operand, or as the target of a call / jmp
//! rel32) and the data that holds it, and the annotated disassembly window - over the mapped image ([`Image`]) with
//! capstone's text ([`crate::x86`]).
//!
//! Every function answers as its Python reference or returns [`Fallback`]: an instruction the capstone parity run did
//! not prove ([`crate::x86::Unverified`]), or an input where the reference raises or indexes in a way not
//! reproduced here (an address before the image, an index past its end). The caller then runs the reference.

use std::collections::BTreeMap;

use crate::pe::Image;
use crate::x86::{Disassembler, Insn, Operand, Unverified};

/// The reference decides (see the module's text).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fallback {
    /// An instruction outside the capstone parity proof.
    Unverified(u32),
    /// An input the port does not take.
    Input(&'static str),
}

impl From<Unverified> for Fallback {
    fn from(u: Unverified) -> Self {
        Self::Unverified(u.0)
    }
}

impl std::fmt::Display for Fallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unverified(a) => write!(
                f,
                "the instruction at 0x{a:08X} is not one the capstone parity run proved"
            ),
            Self::Input(why) => write!(f, "not taken here: {why}"),
        }
    }
}

impl std::error::Error for Fallback {}

/// A result of this module.
pub type R<T> = Result<T, Fallback>;

/// The symbols (`crashreader.read_symbols`): address -> (name, hint).
pub type Syms = BTreeMap<u32, (String, String)>;

/// How an address is used (`exeref.Exe.refs`' kinds).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    /// An immediate or a displacement.
    Operand,
    /// A call rel32's target.
    Call,
    /// A jmp rel32's target.
    Jmp,
}

impl RefKind {
    /// exeref's name of the kind.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Operand => "operand",
            Self::Call => "call",
            Self::Jmp => "jmp",
        }
    }
}

/// The exe, mapped.
#[derive(Debug, Clone)]
pub struct Exe {
    /// The mapped image.
    pub img: Image,
    code: Vec<(u32, u32)>,
    data_ranges: Vec<(u32, u32)>,
    dis: Disassembler,
    strict: bool,
}

impl Exe {
    /// The exe of a mapped image. `strict`: refuse instructions outside the parity proof (false: answer them as
    /// decoded, for a machine without capstone).
    #[must_use]
    pub fn new(img: Image, strict: bool) -> Self {
        let code = img.code_ranges();
        let data_ranges = img.data_ranges();
        Self {
            img,
            code,
            data_ranges,
            dis: Disassembler::new(),
            strict,
        }
    }

    /// The image base.
    #[must_use]
    pub fn base(&self) -> u32 {
        self.img.base
    }

    /// The end of the image (absolute).
    #[must_use]
    pub fn end(&self) -> i64 {
        self.img.end() as i64
    }

    /// The code ranges (absolute, end exclusive).
    #[must_use]
    pub fn code_ranges(&self) -> &[(u32, u32)] {
        &self.code
    }

    fn decode(&self, code: &[u8], address: u32) -> R<Option<Insn>> {
        if self.strict {
            Ok(self.dis.decode(code, address)?)
        } else {
            Ok(self.dis.decode_unchecked(code, address))
        }
    }

    fn disasm_bytes(&self, code: &[u8], address: u32) -> R<Vec<Insn>> {
        if self.strict {
            return Ok(self.dis.disasm(code, address)?);
        }
        let mut out = Vec::new();
        let mut at = 0usize;
        while at < code.len() {
            let a = address.wrapping_add(at as u32);
            let Some(i) = self.dis.decode_unchecked(code.get(at..).unwrap_or_default(), a) else {
                break;
            };
            at += i.size as usize;
            out.push(i);
        }
        Ok(out)
    }

    /// `is_code(a)`: inside a section with execute.
    #[must_use]
    pub fn is_code(&self, a: i64) -> bool {
        self.code
            .iter()
            .any(|&(lo, hi)| i64::from(lo) <= a && a < i64::from(hi))
    }

    /// `bytes_at(a, n)`: `data[a - base : a - base + n]` (Python's slice; an address before the base is refused).
    ///
    /// # Errors
    /// [`Fallback::Input`] for an address before the image.
    pub fn bytes_at(&self, a: i64, n: i64) -> R<&[u8]> {
        let o = a - i64::from(self.img.base);
        if o < 0 {
            return Err(Fallback::Input("an address before the image"));
        }
        let len = self.img.data.len() as i64;
        let lo = o.min(len) as usize;
        let hi = (o + n).clamp(0, len) as usize;
        Ok(if hi > lo {
            self.img.data.get(lo..hi).unwrap_or_default()
        } else {
            &[]
        })
    }

    fn byte(&self, o: i64) -> R<u8> {
        // Python's data[o]: a negative index counts from the end, past the end raises
        let len = self.img.data.len() as i64;
        let k = if o < 0 { o + len } else { o };
        if k < 0 || k >= len {
            return Err(Fallback::Input(
                "an index past the image (the reference raises IndexError)",
            ));
        }
        Ok(self.img.data.get(k as usize).copied().unwrap_or(0))
    }

    /// `function_start(a, limit)`: the start after the last int3 pad before `a` (MSVC pads functions with 0xCC), or
    /// `None`.
    ///
    /// # Errors
    /// [`Fallback::Input`] where the reference indexes past the image.
    pub fn function_start(&self, a: i64, limit: i64) -> R<Option<i64>> {
        let base = i64::from(self.img.base);
        let stop = (a - limit).max(base);
        let mut p = a;
        while p > stop {
            let o = p - base;
            if self.byte(o - 1)? == 0xCC && self.byte(o)? != 0xCC && (p % 16 == 0 || self.byte(o - 2)? == 0xCC) {
                return Ok(Some(p));
            }
            p -= 1;
        }
        Ok(None)
    }

    /// `disasm(start, stop)`: `md.disasm(bytes_at(start, stop - start + 16), start)`.
    ///
    /// # Errors
    /// See [`Fallback`].
    pub fn disasm(&self, start: i64, stop: i64) -> R<Vec<Insn>> {
        let bytes = self.bytes_at(start, stop - start + 16)?;
        self.disasm_bytes(bytes, start as u32)
    }

    /// `around(eip, after)`: (function start or eip - 0x40, the instructions up to eip + after, the index of the one
    /// at eip).
    ///
    /// # Errors
    /// See [`Fallback`].
    pub fn around(&self, eip: i64, after: i64) -> R<(i64, Vec<Insn>, Option<usize>)> {
        let start = self.function_start(eip, 0x4000)?;
        let mut tries: Vec<i64> = Vec::new();
        if let Some(s) = start
            && eip - s <= 0x2000
        {
            tries.push(s);
        }
        tries.extend((eip - 0x60).max(i64::from(self.img.base))..=eip);
        let mut ins: Vec<Insn> = Vec::new();
        for s in tries {
            ins = self
                .disasm(s, eip + after)?
                .into_iter()
                .filter(|x| i64::from(x.address) <= eip + after)
                .collect();
            if ins.iter().any(|x| i64::from(x.address) == eip) {
                break;
            }
        }
        let at = ins.iter().position(|x| i64::from(x.address) == eip);
        Ok((start.unwrap_or(eip - 0x40), ins, at))
    }

    /// `call_before(ret)`: the call instruction that ends exactly at a return address.
    ///
    /// # Errors
    /// See [`Fallback`].
    pub fn call_before(&self, ret: i64) -> R<Option<Insn>> {
        for length in [5i64, 6, 2, 3, 7, 4] {
            let bytes = self.bytes_at(ret - length, length)?;
            if let Some(ins) = self.decode(bytes, (ret - length) as u32)?
                && i64::from(ins.address) + i64::from(ins.size) == ret
                && ins.mnemonic == "call"
            {
                return Ok(Some(ins));
            }
        }
        Ok(None)
    }

    /// `name_of(a, syms, image)`: the function name of an address and its hint.
    ///
    /// # Errors
    /// See [`Fallback`].
    pub fn name_of(&self, a: i64, syms: &Syms) -> R<(String, String)> {
        name_of(a, syms, Some(self))
    }

    /// exeref's `cstr(va, limit)`: the printable C string at an address.
    #[must_use]
    pub fn cstr(&self, va: i64, limit: i64) -> Option<String> {
        if !(i64::from(self.img.base) <= va && va < self.end()) {
            return None;
        }
        let o = (va - i64::from(self.img.base)) as usize;
        let hi = (o + limit.max(0) as usize + 1).min(self.img.data.len());
        let window = self.img.data.get(o..hi)?;
        let e = memchr::memchr(0, window)?;
        if e == 0 {
            return None;
        }
        let s = window.get(..e)?;
        s.iter()
            .all(|&c| (32..127).contains(&c) || c == 9 || c == 10)
            .then(|| s.iter().map(|&c| char::from(c)).collect())
    }

    /// exeref's `find_strings(text, exact)`: the C strings of the data sections that contain `text` (ASCII case
    /// ignored, the string all printable) or equal it (`exact`, case kept), as (address, Latin-1 string).
    #[must_use]
    pub fn find_strings(&self, text: &[u8], exact: bool) -> Vec<(u32, String)> {
        let mut out = Vec::new();
        let base = i64::from(self.img.base);
        let data = &self.img.data;
        for &(lo, hi) in &self.data_ranges {
            let pos = (i64::from(lo) - base).clamp(0, data.len() as i64) as usize;
            let endpos = (i64::from(hi) - base).clamp(0, data.len() as i64) as usize;
            let mut s = pos;
            while s < endpos {
                // a match starts just after a NUL (the lookbehind sees before pos) and ends just before one inside
                // endpos (the lookahead does not see past it)
                let next_nul = |from: usize| {
                    data.get(from..endpos)
                        .and_then(|w| memchr::memchr(0, w))
                        .map(|k| from + k)
                };
                if s == 0 || data.get(s - 1) != Some(&0) {
                    match next_nul(s) {
                        Some(n) => {
                            s = n + 1;
                            continue;
                        }
                        None => break,
                    }
                }
                let Some(nul) = next_nul(s) else { break };
                let seg = data.get(s..nul).unwrap_or_default();
                let hit = if exact {
                    seg == text
                } else {
                    contains_printable(seg, text)
                };
                if hit {
                    out.push(((base + s as i64) as u32, seg.iter().map(|&c| char::from(c)).collect()));
                    s = if nul == s { s + 1 } else { nul };
                } else {
                    s = nul + 1;
                }
            }
        }
        out
    }

    fn instruction_at(&self, start: i64, value: u32) -> R<Option<Insn>> {
        let bytes = self.bytes_at(start, 16)?;
        let Some(ins) = self.decode(bytes, start as u32)? else {
            return Ok(None);
        };
        let hit = ins.operands.iter().any(|op| match op {
            Operand::Imm(v) => (*v as u64 & 0xFFFF_FFFF) == u64::from(value),
            Operand::Mem { disp, .. } => (*disp as u64 & 0xFFFF_FFFF) == u64::from(value),
        });
        Ok(hit.then_some(ins))
    }

    /// exeref's `on_boundary(va)`: whether a linear decode from the function start reaches `va`.
    ///
    /// # Errors
    /// See [`Fallback`].
    pub fn on_boundary(&self, va: i64) -> R<bool> {
        let Some(start) = self.function_start(va, 0x4000)? else {
            return Ok(true);
        };
        if va - start > 0x8000 {
            return Ok(true);
        }
        let bytes = self.bytes_at(start, va - start + 16)?;
        let mut at = 0usize;
        while at < bytes.len() {
            let a = start + at as i64;
            let rest = bytes.get(at..).unwrap_or_default();
            let size = if self.strict {
                self.dis.size_at(rest, a as u32)?
            } else {
                self.dis.decode_unchecked(rest, a as u32).map(|i| i.size)
            };
            let Some(size) = size else { break };
            if a >= va {
                return Ok(a == va);
            }
            at += size as usize;
        }
        Ok(true)
    }

    /// exeref's `refs(value, limit)`: the instructions that use `value` (as an operand found by its bytes, or as the
    /// target of a call / jmp rel32), by address.
    ///
    /// # Errors
    /// See [`Fallback`].
    pub fn refs(&self, value: u32, limit: usize) -> R<Vec<(u32, Insn, RefKind)>> {
        let mut out: Vec<(u32, Insn, RefKind)> = Vec::new();
        let pat = value.to_le_bytes();
        let finder = memchr::memmem::Finder::new(&pat);
        let base = i64::from(self.img.base);
        let data = &self.img.data;
        for &(lo, hi) in &self.code {
            let o = (i64::from(lo) - base) as usize;
            let end = (i64::from(hi) - base) as usize;
            if end > data.len() {
                return Err(Fallback::Input(
                    "a code section past the image (the reference's buffer is too short)",
                ));
            }
            let window = data.get(o..end).unwrap_or_default();
            for k in finder.find_iter(window) {
                if out.len() >= limit {
                    break;
                }
                let j = (o + k) as i64;
                for back in 1..8 {
                    if let Some(ins) = self.instruction_at(base + j - back, value)?
                        && i64::from(ins.address) + i64::from(ins.size) >= base + j + 4
                        && self.on_boundary(i64::from(ins.address))?
                    {
                        out.push((ins.address, ins, RefKind::Operand));
                        break;
                    }
                }
            }
            // call / jmp rel32 to value: the opcode at every position but the last five
            let scan = window.get(..window.len().saturating_sub(5)).unwrap_or_default();
            for (opc, kind) in [(0xE8u8, RefKind::Call), (0xE9u8, RefKind::Jmp)] {
                let mut hits = 0usize;
                for p in memchr::memchr_iter(opc, scan) {
                    let Some(r) = window.get(p + 1..p + 5) else { continue };
                    let Ok(r4) = <[u8; 4]>::try_from(r) else { continue };
                    let rel = i64::from(i32::from_le_bytes(r4));
                    if i64::from(lo) + p as i64 + 5 + rel != i64::from(value) {
                        continue;
                    }
                    if hits >= limit {
                        break;
                    }
                    hits += 1;
                    let va = i64::from(lo) + p as i64;
                    if self.on_boundary(va)?
                        && let Some(ins) = self.decode(self.bytes_at(va, 5)?, va as u32)?
                    {
                        out.push((va as u32, ins, kind));
                    }
                }
            }
        }
        out.sort_by_key(|x| x.0);
        out.truncate(limit);
        Ok(out)
    }

    /// exeref's `data_refs(value, limit)`: 4-byte aligned places in the data sections that hold `value`.
    #[must_use]
    pub fn data_refs(&self, value: u32, limit: usize) -> Vec<u32> {
        let mut out = Vec::new();
        let pat = value.to_le_bytes();
        let base = i64::from(self.img.base);
        let finder = memchr::memmem::Finder::new(&pat);
        for &(lo, hi) in &self.data_ranges {
            let o = ((i64::from(lo) - base).max(0) as usize).min(self.img.data.len());
            let end = ((i64::from(hi) - base).max(0) as usize).min(self.img.data.len());
            let Some(window) = self.img.data.get(o..end) else {
                continue;
            };
            for k in finder.find_iter(window) {
                if out.len() >= limit {
                    break;
                }
                if (o + k).is_multiple_of(4) {
                    out.push((base + (o + k) as i64) as u32);
                }
            }
        }
        out
    }

    /// exeref's `window(exe, va, before, after, syms)`: the lines of a short disassembly around `va`.
    ///
    /// # Errors
    /// See [`Fallback`].
    pub fn window(&self, va: i64, before: i64, after: i64, syms: &Syms) -> R<Vec<String>> {
        let (before, after) = (before.min(0x100), after.min(0x100));
        let mut insns: Vec<Insn> = Vec::new();
        if let Some(start) = self.function_start(va, 0x4000)?
            && va - start <= 0x2000
        {
            insns = self.disasm_bytes(self.bytes_at(start, va - start + after + 16)?, start as u32)?;
        }
        if !insns.iter().any(|i| i64::from(i.address) == va) {
            for s in (va - before)..=va {
                insns = self.disasm_bytes(self.bytes_at(s, va - s + after + 16)?, s as u32)?;
                if insns.iter().any(|i| i64::from(i.address) == va) {
                    break;
                }
            }
        }
        let mut lines = Vec::new();
        for ins in &insns {
            let a = i64::from(ins.address);
            if a < va - before {
                continue;
            }
            if a > va + after {
                break;
            }
            let note = self.annotate(ins, syms)?;
            let mark = if a <= va && va < a + i64::from(ins.size) {
                '>'
            } else {
                ' '
            };
            lines.push(format!(
                "  {mark} {:08X}  {:<6} {}{note}",
                ins.address, ins.mnemonic, ins.op_str
            ));
        }
        Ok(lines)
    }

    /// exeref's `annotate(exe, ins, syms)`: the strings and known functions an instruction names.
    ///
    /// # Errors
    /// See [`Fallback`].
    pub fn annotate(&self, ins: &Insn, syms: &Syms) -> R<String> {
        let mut notes: Vec<String> = Vec::new();
        for tok in hex_tokens(&ins.op_str) {
            let Some(v) = parse_hex(tok) else {
                return Err(Fallback::Input("a number past 63 bits"));
            };
            if let Some(s) = self.cstr(v, 120).filter(|s| s.chars().count() >= 2) {
                let cut: String = s.chars().take(60).collect();
                notes.push(format!("\"{cut}\""));
            } else if let Some((name, _)) = u32::try_from(v).ok().and_then(|k| syms.get(&k)) {
                notes.push(name.clone());
            }
        }
        if ins.mnemonic == "call" && notes.is_empty() {
            let op = ins.op_str.as_str();
            if let Some(hex) = op.strip_prefix("0x")
                && !hex.is_empty()
                && hex.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            {
                let Some(v) = parse_hex(op) else {
                    return Err(Fallback::Input("a number past 63 bits"));
                };
                let (n, _) = self.name_of(v, syms)?;
                if !n.is_empty() {
                    notes.push(n.strip_suffix(" +0x0").map_or_else(|| n.clone(), str::to_string));
                }
            }
        }
        Ok(if notes.is_empty() {
            String::new()
        } else {
            format!("   ; {}", notes.join(", "))
        })
    }
}

/// `[\x20-\x7e]* text [\x20-\x7e]*` over a whole NUL-free segment, `text` ignoring ASCII case.
fn contains_printable(seg: &[u8], text: &[u8]) -> bool {
    let printable = |c: &u8| (0x20..=0x7E).contains(c);
    if seg.len() < text.len() {
        return false;
    }
    // every character outside the match must be printable
    let first_bad = seg.iter().position(|c| !printable(c));
    let last_bad = seg.iter().rposition(|c| !printable(c));
    (0..=seg.len() - text.len()).any(|k| {
        seg.get(k..k + text.len()).is_some_and(|w| w.eq_ignore_ascii_case(text))
            && first_bad.is_none_or(|f| f >= k)
            && last_bad.is_none_or(|l| l < k + text.len())
    })
}

/// The `0x[0-9a-f]+` tokens of a text (leftmost, non-overlapping, greedy).
fn hex_tokens(s: &str) -> Vec<&str> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut k = 0;
    while k < b.len() {
        if b.get(k) == Some(&b'0') && b.get(k + 1) == Some(&b'x') {
            let mut e = k + 2;
            while b
                .get(e)
                .is_some_and(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(c))
            {
                e += 1;
            }
            if e > k + 2 {
                out.push(s.get(k..e).unwrap_or_default());
                k = e;
                continue;
            }
        }
        k += 1;
    }
    out
}

fn parse_hex(tok: &str) -> Option<i64> {
    let digits = tok.strip_prefix("0x")?;
    let v = u64::from_str_radix(digits, 16).ok()?;
    i64::try_from(v).ok()
}

/// `crashreader.name_of(a, syms, image)` (`exe` None: without the exe).
///
/// # Errors
/// See [`Fallback`].
pub fn name_of(a: i64, syms: &Syms, exe: Option<&Exe>) -> R<(String, String)> {
    if let Some((n, h)) = u32::try_from(a).ok().and_then(|k| syms.get(&k)) {
        return Ok((n.clone(), h.clone()));
    }
    let start = match exe {
        Some(e) if e.is_code(a) => e.function_start(a, 0x4000)?,
        _ => None,
    };
    if let Some(s) = start
        && let Some((n, h)) = u32::try_from(s).ok().and_then(|k| syms.get(&k))
    {
        return Ok((format!("{n} +0x{:X}", a - s), h.clone()));
    }
    // the nearest symbol at or below a, within 0x800
    let near = if a < 0 {
        None
    } else {
        syms.range(..=u32::try_from(a).unwrap_or(u32::MAX))
            .next_back()
            .map(|(&k, v)| (i64::from(k), v))
            .filter(|(k, _)| a - *k < 0x800)
    };
    if let (Some((k, (n, h))), Some(s)) = (near, start)
        && k >= s
    {
        return Ok((format!("{n} +0x{:X}", a - k), h.clone()));
    }
    if let Some(s) = start {
        return Ok((format!("FUN_{s:08x} +0x{:X}", a - s), String::new()));
    }
    Ok((String::new(), String::new()))
}
