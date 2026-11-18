//! 32-bit x86 decoding with capstone's text and operands (the crash reader's and exeref's `capstone.Cs(CS_ARCH_X86,
//! CS_MODE_32)` with `detail = True`, capstone 5.0), on iced-x86's decoder.
//!
//! [`Disassembler::decode`] gives the first instruction of a byte slice the way capstone's `next(md.disasm(code,
//! address), None)` does: `None` where capstone decodes nothing, else the instruction's address, size, mnemonic and
//! operand text as capstone prints them, and the operands the tools read from capstone's detail (each immediate's
//! value, each memory operand's base, index and displacement; register operands are not listed: no tool reads
//! them). [`Disassembler::disasm`] is `md.disasm`: instructions one after another until one does not decode.
//!
//! The text is written here by capstone's rules (`X86IntelInstPrinter.c`: the memory operand, the immediates' signs
//! and masks, `X86ImmSize.inc`'s byte immediates; `X86Mapping.c`'s `X86_lockrep`: which lock / rep / bnd / notrack /
//! xacquire prefix it names, from the prefix bytes as `X86DisassemblerDecoder.c` reads them; `checkPrefix`: the
//! instructions a lock prefix may stand before) over iced's decoded instruction, with capstone's names where its
//! tables name an instruction or list its operands differently, and capstone's refusals (the x87 alias encodings,
//! instructions newer than capstone 5). Every rule is proved by decoding every byte offset of BFBC2Game.exe's code
//! with both (16.8 million starts: `crash-reader/tests/test_crash_native.py --game`); an encoding the exe does not
//! hold may still be named otherwise than capstone names it.

use iced_x86::{Code, Decoder, DecoderOptions, Instruction, MemorySize, Mnemonic, OpKind, Register};
use std::fmt::Write as _;

/// An operand the tools read from capstone's detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operand {
    /// An immediate (a branch target included), as capstone's `op.imm`.
    Imm(i64),
    /// A memory operand: base and index register (`Register::None` for none) and the displacement.
    Mem {
        /// The base register.
        base: Register,
        /// The index register.
        index: Register,
        /// The displacement, as capstone's `op.mem.disp`.
        disp: i64,
    },
}

/// One decoded instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Insn {
    /// Its address.
    pub address: u32,
    /// Its length in bytes.
    pub size: u32,
    /// capstone's `mnemonic` (a `lock` / `rep` prefix included).
    pub mnemonic: String,
    /// capstone's `op_str`.
    pub op_str: String,
    /// capstone's immediate and memory operands, in order.
    pub operands: Vec<Operand>,
}

/// capstone's lower-case name of a register (`reg_name`).
#[must_use]
pub fn reg_name(r: Register) -> String {
    match r {
        Register::ST0 => "st(0)".into(),
        Register::ST1 => "st(1)".into(),
        Register::ST2 => "st(2)".into(),
        Register::ST3 => "st(3)".into(),
        Register::ST4 => "st(4)".into(),
        Register::ST5 => "st(5)".into(),
        Register::ST6 => "st(6)".into(),
        Register::ST7 => "st(7)".into(),
        _ => format!("{r:?}").to_ascii_lowercase(),
    }
}

const OPTIONS: u32 = DecoderOptions::MPX;

/// iced's decode of `code` as capstone reads it: where iced refuses a lock prefix capstone takes (a lock before a
/// register destination, or before F2 / F3) or `mov cs, ...`, without iced's validity checks; an EVEX prefix with
/// the bits 32-bit mode leaves unused (R', V') as capstone ignores them.
fn iced_decode(code: &[u8], address: u32, ins: &mut Instruction) -> bool {
    iced_decode_why(code, address, ins).is_ok()
}

/// [`iced_decode`], and on failure whether the bytes ran out (`true`: capstone cannot decode either).
fn iced_decode_why(code: &[u8], address: u32, ins: &mut Instruction) -> Result<(), bool> {
    let mut dec = Decoder::with_ip(32, code, u64::from(address), OPTIONS);
    if !dec.can_decode() {
        return Err(true);
    }
    dec.decode_out(ins);
    if !ins.is_invalid() {
        return Ok(());
    }
    let short = dec.last_error() == iced_x86::DecoderError::NoMoreBytes;
    if retry_decode(code, address, ins) {
        Ok(())
    } else {
        Err(short)
    }
}

/// The re-decodes for the encodings iced refuses and capstone takes (see [`iced_decode`]).
fn retry_decode(code: &[u8], address: u32, ins: &mut Instruction) -> bool {
    let p = prefixes(code);
    let op = code.get(p.len).copied().unwrap_or(0);
    if code.get(..p.len).is_some_and(|pre| pre.contains(&0xF0)) || op == 0x8E {
        let mut dec = Decoder::with_ip(32, code, u64::from(address), OPTIONS | DecoderOptions::NO_INVALID_CHECK);
        dec.decode_out(ins);
        return !ins.is_invalid();
    }
    // mov to / from a control or debug register iced does not know (cr1, cr5-cr7 ...): decoded as cr0 / dr0, the
    // register named from the ModRM byte (control_register)
    if op == 0x0F && matches!(code.get(p.len + 1), Some(0x20..=0x23)) && code.len() > p.len + 2 {
        let mut patched = code.to_vec();
        if let Some(b) = patched.get_mut(p.len + 2) {
            *b &= !0x38;
        }
        let mut dec = Decoder::with_ip(32, &patched, u64::from(address), OPTIONS);
        dec.decode_out(ins);
        return !ins.is_invalid();
    }
    if op == 0x62 && code.len() > p.len + 3 {
        let mut patched = code.to_vec();
        if let Some(b) = patched.get_mut(p.len + 1) {
            *b |= 0x10;
        }
        if let Some(b) = patched.get_mut(p.len + 3) {
            *b |= 0x08;
        }
        let mut dec = Decoder::with_ip(32, &patched, u64::from(address), OPTIONS);
        dec.decode_out(ins);
        return !ins.is_invalid();
    }
    false
}

/// An instruction whose encoding the parity run did not prove (its iced code is not in [`VERIFIED`]): capstone, the
/// Python reference, decodes it. The address of the instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unverified(pub u32);

impl std::fmt::Display for Unverified {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the instruction at 0x{:08X} is not one the capstone parity run proved",
            self.0
        )
    }
}

impl std::error::Error for Unverified {}

/// The decode result: `Ok(None)` where capstone decodes nothing.
pub type Decoded<T> = Result<T, Unverified>;

pub use crate::x86_verified::{VERIFIED, VERIFIED_NONE};

/// The prefix bits of a proof key: 0x66, 0x67, F0, F2, F3, any segment override.
fn prefix_bits(bytes: &[u8], p: &Prefixes) -> u32 {
    let mut bits = 0;
    for &b in bytes.get(..p.len).unwrap_or_default() {
        bits |= match b {
            0x66 => 1,
            0x67 => 2,
            0xF0 => 4,
            0xF2 => 8,
            0xF3 => 16,
            _ => 32,
        };
    }
    bits
}

/// The proof key of a decoded instruction: its iced code and its prefix bits.
#[must_use]
pub fn insn_key(code: Code, bytes: &[u8]) -> u32 {
    (u32::from(code as u16) << 8) | prefix_bits(bytes, &prefixes(bytes))
}

/// The proof key of bytes iced does not decode: prefix bits, opcode map, opcode and the two bytes after it.
#[must_use]
pub fn invalid_key(bytes: &[u8]) -> u32 {
    let p = prefixes(bytes);
    let op = bytes.get(p.len..).unwrap_or_default();
    let (map, rest) = match op {
        [0x0F, 0x38, rest @ ..] => (2, rest),
        [0x0F, 0x3A, rest @ ..] => (3, rest),
        [0x0F, rest @ ..] => (1, rest),
        rest => (0, rest),
    };
    let opcode = u32::from(rest.first().copied().unwrap_or(0));
    let next = u32::from(rest.get(1).copied().unwrap_or(0));
    let third = u32::from(rest.get(2).copied().unwrap_or(0));
    prefix_bits(bytes, &p) | (map << 6) | (opcode << 8) | (next << 16) | (third << 24)
}

fn verified(code: Code, bytes: &[u8]) -> bool {
    VERIFIED.binary_search(&insn_key(code, bytes)).is_ok()
}

/// What capstone gives at `code`, as far as the proof reaches.
enum Verdict {
    Insn(Instruction),
    Nothing,
    Unproved,
}

fn verdict(code: &[u8], address: u32) -> Verdict {
    let mut ins = Instruction::default();
    match iced_decode_why(code, address, &mut ins) {
        Err(true) => Verdict::Nothing,
        Err(false) => {
            if VERIFIED_NONE.binary_search(&invalid_key(code)).is_ok() {
                Verdict::Nothing
            } else {
                Verdict::Unproved
            }
        }
        Ok(()) => {
            if accepted_size(&ins, code).is_none() {
                Verdict::Nothing
            } else if verified(ins.code(), code) {
                Verdict::Insn(ins)
            } else {
                Verdict::Unproved
            }
        }
    }
}

/// The decoder (capstone's 32-bit mode).
#[derive(Debug, Default, Clone, Copy)]
pub struct Disassembler;

impl Disassembler {
    /// A disassembler in capstone's style.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// The first instruction of `code` at `address` (capstone's `next(md.disasm(code, address), None)`).
    ///
    /// # Errors
    /// [`Unverified`] for an encoding the parity run did not prove.
    pub fn decode(&self, code: &[u8], address: u32) -> Decoded<Option<Insn>> {
        match verdict(code, address) {
            Verdict::Insn(ins) => Ok(convert(&ins, code, address)),
            Verdict::Nothing => Ok(None),
            Verdict::Unproved => Err(Unverified(address)),
        }
    }

    /// [`Self::decode`] without the proof check (the parity runs compare every decode).
    #[must_use]
    pub fn decode_unchecked(&self, code: &[u8], address: u32) -> Option<Insn> {
        let mut ins = Instruction::default();
        if !iced_decode(code, address, &mut ins) {
            return None;
        }
        convert(&ins, code, address)
    }

    /// The proof key of what is decoded at `code` (the parity tables): `Ok(key)` of an instruction ([`insn_key`]),
    /// `Err(Some(key))` of bytes iced does not decode ([`invalid_key`]), `Err(None)` where the bytes run out or a
    /// capstone rule refuses the instruction.
    ///
    /// # Errors
    /// See above.
    pub fn key_of(&self, code: &[u8], address: u32) -> Result<u32, Option<u32>> {
        let mut ins = Instruction::default();
        match iced_decode_why(code, address, &mut ins) {
            Err(true) => Err(None),
            Err(false) => Err(Some(invalid_key(code))),
            Ok(()) => match accepted_size(&ins, code) {
                Some(_) => Ok(insn_key(ins.code(), code)),
                None => Err(None),
            },
        }
    }

    /// capstone's `md.disasm(code, address)`: instructions until the bytes end or one does not decode.
    ///
    /// # Errors
    /// [`Unverified`] for an encoding the parity run did not prove.
    pub fn disasm(&self, code: &[u8], address: u32) -> Decoded<Vec<Insn>> {
        let mut out = Vec::new();
        let mut at = 0usize;
        while at < code.len() {
            let Some(i) = self.decode(code.get(at..).unwrap_or_default(), address.wrapping_add(at as u32))? else {
                break;
            };
            at += i.size as usize;
            out.push(i);
        }
        Ok(out)
    }

    /// The addresses and sizes of the instructions of `md.disasm(code, address)` (no text).
    ///
    /// # Errors
    /// [`Unverified`] for an encoding the parity run did not prove.
    pub fn boundaries(&self, code: &[u8], address: u32) -> Decoded<Vec<(u32, u32)>> {
        let mut out = Vec::new();
        let mut at = 0usize;
        while at < code.len() {
            let rest = code.get(at..).unwrap_or_default();
            let a = address.wrapping_add(at as u32);
            let Some(size) = self.size_at(rest, a)? else { break };
            out.push((a, size));
            at += size as usize;
        }
        Ok(out)
    }

    /// The first instruction's size, as [`Self::boundaries`] reads it (`None`: capstone decodes nothing).
    ///
    /// # Errors
    /// [`Unverified`] for an encoding the parity run did not prove.
    pub fn size_at(&self, code: &[u8], address: u32) -> Decoded<Option<u32>> {
        match verdict(code, address) {
            Verdict::Insn(ins) => Ok(accepted_size(&ins, code)),
            Verdict::Nothing => Ok(None),
            Verdict::Unproved => Err(Unverified(address)),
        }
    }
}

/// The legacy prefixes as capstone's `readPrefixes` reads them.
#[derive(Debug, Default, Clone, Copy)]
struct Prefixes {
    /// The number of prefix bytes.
    len: usize,
    /// The last of F0 / F2 / F3 (`prefix0`).
    group1: u8,
    /// The last segment override byte (`prefix1`).
    segment: u8,
    /// A lock prefix not followed by F2 / F3 (`hasLockPrefix`).
    lock: bool,
    /// F2 / F3 as the first byte before F0 or an xchg (`xAcquireRelease`).
    acquire_release: u8,
}

fn prefixes(bytes: &[u8]) -> Prefixes {
    let mut p = Prefixes::default();
    for (k, &b) in bytes.iter().enumerate() {
        match b {
            0xF0 => {
                p.group1 = b;
                p.lock = true;
            }
            0xF2 | 0xF3 => {
                if k == 0 {
                    let next = bytes.get(1).copied().unwrap_or(0);
                    if next == 0xF0 || (next & 0xFE) == 0x86 || (next & 0xF8) == 0x90 {
                        p.acquire_release = b;
                    }
                    if b == 0xF3 && matches!(next, 0x88 | 0x89 | 0xC6 | 0xC7) {
                        p.acquire_release = b;
                    }
                }
                p.group1 = b;
                p.lock = false;
            }
            0x2E | 0x36 | 0x3E | 0x26 | 0x64 | 0x65 => p.segment = b,
            0x66 | 0x67 => {}
            _ => {
                p.len = k;
                return p;
            }
        }
    }
    p.len = bytes.len();
    p
}

/// The size capstone gives the instruction iced decoded, or `None` where capstone decodes nothing.
fn accepted_size(ins: &Instruction, bytes: &[u8]) -> Option<u32> {
    let p = prefixes(bytes);
    let op = bytes.get(p.len..).unwrap_or_default();
    let b0 = op.first().copied().unwrap_or(0);
    let b1 = op.get(1).copied().unwrap_or(0);
    // the x87 alias encodings capstone does not decode
    match (b0, b1) {
        (0xDC, 0xD0..=0xDF) | (0xDE, 0xD0..=0xD7) | (0xDD, 0xC8..=0xCF) | (0xDF, 0xC8..=0xDF) => return None,
        _ => {}
    }
    if refused(ins) || !nop_family_decodes(bytes, &p) {
        return None;
    }
    if p.lock && !lockable(ins) {
        return None;
    }
    // ud0 / ud1: capstone reads no ModRM
    if b0 == 0x0F && (b1 == 0xFF || b1 == 0xB9) {
        return Some((p.len + 2) as u32);
    }
    Some(ins.len() as u32)
}

/// Whether a prefetch / hint-nop / MPX / fence encoding (0F 0D, 0F 18-1F, 0F AE without 66 / F2 / F3) is one capstone
/// decodes (its tables: prefetch* on memory, nop forms, bndldx / bndstx and cldemote on memory, the fences only as
/// E8 / F0 / F8).
fn nop_family_decodes(bytes: &[u8], p: &Prefixes) -> bool {
    let pre = bytes.get(..p.len).unwrap_or_default();
    if pre.iter().any(|b| matches!(b, 0x66 | 0xF2 | 0xF3)) {
        return true;
    }
    let op = bytes.get(p.len..).unwrap_or_default();
    let (Some(&0x0F), Some(&o), Some(&modrm)) = (op.first(), op.get(1), op.get(2)) else {
        return true;
    };
    let reg_form = modrm >> 6 == 3;
    let reg = (modrm >> 3) & 7;
    match o {
        0x0D => !reg_form && reg <= 2,
        0x18 => !reg_form || reg >= 4,
        0x1A | 0x1B => !reg_form && reg <= 3,
        0x1C => !reg_form && reg == 0,
        0x1D | 0x1E => !reg_form,
        0xAE if reg_form => reg >= 5 && modrm.trailing_zeros() >= 3,
        _ => true,
    }
}

/// 0F 19 with a register operand: capstone's `nop rm, reg` (two operands).
fn nop_two_operands(bytes: &[u8], p: &Prefixes) -> bool {
    let op = bytes.get(p.len..).unwrap_or_default();
    matches!((op.first(), op.get(1), op.get(2)), (Some(&0x0F), Some(&0x19), Some(&m)) if m >> 6 == 3)
}

/// Instructions capstone 5 does not know (newer than its tables) or reads otherwise.
fn refused(ins: &Instruction) -> bool {
    matches!(
        ins.mnemonic(),
        Mnemonic::Serialize | Mnemonic::Tlbsync | Mnemonic::Wrmsrns | Mnemonic::Vmulsh
    ) || matches!(
        ins.code(),
        Code::Prefetchreserved3_m8
            | Code::Prefetchreserved4_m8
            | Code::Prefetchreserved5_m8
            | Code::Prefetchreserved6_m8
            | Code::Prefetchreserved7_m8
    )
}

/// capstone's `checkPrefix`: the instructions a lock prefix may stand before.
fn lockable(ins: &Instruction) -> bool {
    use Mnemonic as M;
    let mem = (0..ins.op_count()).any(|k| ins.op_kind(k) == OpKind::Memory);
    let mem_dest = ins.op_count() > 0 && ins.op_kind(0) == OpKind::Memory;
    match ins.mnemonic() {
        M::Adc | M::Add | M::And | M::Or | M::Sub | M::Xor | M::Cmpxchg8b | M::Xchg => mem,
        M::Sbb | M::Dec | M::Inc | M::Neg | M::Not | M::Btc | M::Btr | M::Bts | M::Xadd | M::Cmpxchg => mem_dest,
        // NOOPL: 0F 1F on memory
        M::Nop | M::Reservednop => {
            mem && matches!(
                ins.code(),
                Code::Nop_rm16 | Code::Nop_rm32 | Code::Reservednop_rm16_r16_0F1F | Code::Reservednop_rm32_r32_0F1F
            )
        }
        _ => false,
    }
}

const HEX_THRESHOLD: i64 = 9;

/// capstone's `printImm(imm, positive)` in Intel syntax (`op1_size` masks a negative value printed positive).
fn print_imm(out: &mut String, imm: i64, positive: bool, op1_size: u32) {
    if positive {
        if imm < 0 {
            let v = match op1_size {
                1 => imm & 0xff,
                2 => imm & 0xffff,
                4 => imm & 0xffff_ffff,
                _ => imm,
            };
            let _ = write!(out, "0x{:x}", v as u64);
        } else if imm > HEX_THRESHOLD {
            let _ = write!(out, "0x{imm:x}");
        } else {
            let _ = write!(out, "{imm}");
        }
    } else if imm < 0 {
        if imm == i64::MIN {
            out.push_str("0x8000000000000000");
        } else if imm < -HEX_THRESHOLD {
            let _ = write!(out, "-0x{:x}", -imm);
        } else {
            let _ = write!(out, "-{}", -imm);
        }
    } else if imm > HEX_THRESHOLD {
        let _ = write!(out, "0x{imm:x}");
    } else {
        let _ = write!(out, "{imm}");
    }
}

/// The size capstone gives a register (`regsize_map`), for `op1_size`.
fn reg_size(r: Register) -> u32 {
    if r == Register::None {
        return 0;
    }
    r.size() as u32
}

/// The 16-bit register of a 32-bit general register (capstone names `arpl` / `verw` ... operands by their 16 bits).
fn reg16(r: Register) -> Register {
    match r {
        Register::EAX => Register::AX,
        Register::ECX => Register::CX,
        Register::EDX => Register::DX,
        Register::EBX => Register::BX,
        Register::ESP => Register::SP,
        Register::EBP => Register::BP,
        Register::ESI => Register::SI,
        Register::EDI => Register::DI,
        other => other,
    }
}

/// The keyword before a memory operand (`byte ptr ` ...), and the size capstone records for it.
fn mem_keyword(ins: &Instruction) -> (&'static str, u32) {
    use Mnemonic as M;
    let m = ins.mnemonic();
    match m {
        M::Lea => return ("", 4),
        M::Fbld | M::Fbstp => return ("tbyte ptr ", 10),
        M::Fldenv | M::Fnstenv => return ("", 28),
        M::Fnsave | M::Frstor => return ("dword ptr ", 4),
        M::Invlpg => return ("byte ptr ", 1),
        M::Fxsave | M::Fxrstor | M::Sgdt | M::Sidt | M::Lgdt | M::Lidt => return ("", 6),
        M::Bndldx | M::Bndstx | M::Bndmk | M::Bndcl | M::Bndcu | M::Bndcn => return ("", 16),
        _ => {}
    }
    if far16_mem(ins) {
        return ("", 6);
    }
    match ins.memory_size() {
        MemorySize::SegPtr16 | MemorySize::SegPtr32 => ("ptr ", 4),
        s => match s.size() {
            1 => ("byte ptr ", 1),
            2 => ("word ptr ", 2),
            4 => ("dword ptr ", 4),
            8 => ("qword ptr ", 8),
            10 => ("xword ptr ", 10),
            16 => ("xmmword ptr ", 16),
            32 => ("ymmword ptr ", 32),
            64 => ("zmmword ptr ", 64),
            _ => ("ptr ", 4),
        },
    }
}

const SSE_CMP: [&str; 8] = ["eq", "lt", "le", "unord", "neq", "nlt", "nle", "ord"];
const AVX_CMP: [&str; 32] = [
    "eq", "lt", "le", "unord", "neq", "nlt", "nle", "ord", "eq_uq", "nge", "ngt", "false", "neq_oq", "ge", "gt",
    "true", "eq_os", "lt_oq", "le_oq", "unord_s", "neq_us", "nlt_uq", "nle_uq", "ord_s", "eq_us", "nge_uq", "ngt_uq",
    "false_os", "neq_os", "ge_oq", "gt_oq", "true_us",
];

/// capstone's instruction name (without prefixes); the compare pseudo-ops take their predicate from the immediate.
fn mnemonic_name(ins: &Instruction, imm: Option<u8>) -> String {
    use Mnemonic as M;
    let m = ins.mnemonic();
    let direct_far = (0..ins.op_count()).any(|k| matches!(ins.op_kind(k), OpKind::FarBranch16 | OpKind::FarBranch32))
        || far16_mem(ins);
    let cmp = |stem: &str, sfx: &str| -> Option<String> {
        let v = usize::from(imm?);
        let table: &[&str] = if stem == "vcmp" { &AVX_CMP } else { &SSE_CMP };
        table.get(v).map(|p| format!("{stem}{p}{sfx}"))
    };
    let named = match m {
        M::Jmp if direct_far => Some("ljmp".to_string()),
        M::Call if direct_far => Some("lcall".to_string()),
        M::Pushad => Some("pushal".to_string()),
        M::Popad => Some("popal".to_string()),
        M::Pusha => Some("pushaw".to_string()),
        M::Popa => Some("popaw".to_string()),
        M::Fcomip => Some("fcompi".to_string()),
        M::Fucomip => Some("fucompi".to_string()),
        M::Fnsetpm => Some("fsetpm".to_string()),
        M::Fneni => Some("feni8087_nop".to_string()),
        M::Fndisi => Some("fdisi8087_nop".to_string()),
        M::Reservednop => Some("nop".to_string()),
        M::Cmpps => cmp("cmp", "ps"),
        M::Cmppd => cmp("cmp", "pd"),
        M::Cmpss => cmp("cmp", "ss"),
        M::Cmpsd if !is_string_op(ins) => cmp("cmp", "sd"),
        M::Vcmpps => cmp("vcmp", "ps"),
        M::Vcmppd => cmp("vcmp", "pd"),
        M::Vcmpss => cmp("vcmp", "ss"),
        M::Vcmpsd => cmp("vcmp", "sd"),
        _ => None,
    };
    named.unwrap_or_else(|| format!("{m:?}").to_ascii_lowercase())
}

/// A far call / jmp through a 16:16 pointer in memory: capstone's `lcall` / `ljmp` (the 16:32 one is `call` /
/// `jmp` with `ptr`).
fn far16_mem(ins: &Instruction) -> bool {
    matches!(ins.mnemonic(), Mnemonic::Call | Mnemonic::Jmp)
        && ins.memory_size() == MemorySize::SegPtr16
        && (0..ins.op_count()).any(|k| ins.op_kind(k) == OpKind::Memory)
}

fn is_string_op(ins: &Instruction) -> bool {
    (0..ins.op_count()).any(|k| {
        matches!(
            ins.op_kind(k),
            OpKind::MemorySegSI
                | OpKind::MemorySegESI
                | OpKind::MemorySegRSI
                | OpKind::MemoryESDI
                | OpKind::MemoryESEDI
                | OpKind::MemoryESRDI
        )
    })
}

/// The prefix words capstone writes before the name (`X86_lockrep`).
fn prefix_text(ins: &Instruction, p: &Prefixes) -> String {
    use Mnemonic as M;
    let m = ins.mnemonic();
    let s = is_string_op(ins);
    let mut out = String::new();
    let xchg_mem = m == M::Xchg && (0..ins.op_count()).any(|k| ins.op_kind(k) == OpKind::Memory);
    let jcc_like = ins.is_jcc_short_or_near()
        || matches!(m, M::Jcxz | M::Jecxz | M::Jmp | M::Call | M::Ret | M::Retf)
            && !(0..ins.op_count()).any(|k| matches!(ins.op_kind(k), OpKind::FarBranch16 | OpKind::FarBranch32))
            && !far16_mem(ins);
    match p.group1 {
        0xF0 => {
            out.push_str(match p.acquire_release {
                0xF2 => "xacquire lock ",
                0xF3 => "xrelease lock ",
                _ => "lock ",
            });
        }
        0xF2 => {
            let repne =
                s && matches!(
                    m,
                    M::Cmpsb
                        | M::Cmpsw
                        | M::Cmpsd
                        | M::Scasb
                        | M::Scasw
                        | M::Scasd
                        | M::Movsb
                        | M::Movsw
                        | M::Lodsb
                        | M::Lodsw
                        | M::Lodsd
                        | M::Stosb
                        | M::Stosw
                        | M::Stosd
                        | M::Insb
                        | M::Insw
                        | M::Insd
                        | M::Outsb
                        | M::Outsw
                        | M::Outsd
                ) || matches!(m, M::Movss | M::Cmpss);
            if xchg_mem && p.acquire_release != 0 {
                out.push_str("xacquire ");
            } else if repne {
                out.push_str("repne ");
            } else if jcc_like {
                out.push_str("bnd ");
            }
        }
        0xF3 => {
            let rep = s
                && matches!(
                    m,
                    M::Movsb
                        | M::Movsw
                        | M::Movsd
                        | M::Lodsb
                        | M::Lodsw
                        | M::Lodsd
                        | M::Stosb
                        | M::Stosw
                        | M::Stosd
                        | M::Insb
                        | M::Insw
                        | M::Insd
                        | M::Outsb
                        | M::Outsw
                        | M::Outsd
                );
            let repe = s && matches!(m, M::Cmpsb | M::Cmpsw | M::Cmpsd | M::Scasb | M::Scasw | M::Scasd);
            if xchg_mem && p.acquire_release != 0 {
                out.push_str("xrelease ");
            } else if rep {
                out.push_str("rep ");
            } else if repe {
                out.push_str("repe ");
            } else if m == M::Ret {
                out.push_str("repz ");
            }
        }
        _ => {}
    }
    if p.segment == 0x3E
        && matches!(m, M::Call | M::Jmp)
        && !(0..ins.op_count()).any(|k| matches!(ins.op_kind(k), OpKind::FarBranch16 | OpKind::FarBranch32))
        && !far16_mem(ins)
    {
        out.push_str("notrack ");
    }
    out
}

/// Whether capstone masks the instruction's immediate to a byte (`X86_immediate_size` 1).
fn imm_size_one(ins: &Instruction) -> bool {
    use Mnemonic as M;
    match ins.mnemonic() {
        // F6 /1 (TEST8mi_alt) is in the table, but capstone's search there misses it: its byte stays signed
        M::Test if ins.code() == Code::Test_rm8_imm8_F6r1 => false,
        M::Adc | M::Add | M::And | M::Cmp | M::Or | M::Sbb | M::Sub | M::Xor | M::Test | M::Mov => {
            first_operand_size(ins) == 1
        }
        M::Bt
        | M::Btc
        | M::Btr
        | M::Bts
        | M::Rcl
        | M::Rcr
        | M::Rol
        | M::Ror
        | M::Sal
        | M::Sar
        | M::Shl
        | M::Shr
        | M::Shld
        | M::Shrd
        | M::Aad
        | M::Aam
        | M::In => true,
        _ => false,
    }
}

/// Whether the instruction's byte immediate is capstone's `u8imm` (printed and kept unsigned): the SIMD ones.
fn u8_imm(ins: &Instruction) -> bool {
    (0..ins.op_count()).any(|k| {
        ins.op_kind(k) == OpKind::Register && {
            let r = ins.op_register(k);
            r.is_xmm() || r.is_ymm() || r.is_zmm() || r.is_mm()
        }
    })
}

/// The operand size of the first operand (for the byte immediates and the `and` / `or` / `xor` masks).
fn first_operand_size(ins: &Instruction) -> u32 {
    if ins.op_count() == 0 {
        return 0;
    }
    match ins.op_kind(0) {
        OpKind::Register => reg_size(ins.op_register(0)),
        OpKind::Memory => ins.memory_size().size() as u32,
        _ => 0,
    }
}

/// Which x87 register operand capstone leaves out of the text: 0 the first, 1 the second, else none.
fn x87_dropped(code: Code) -> Option<u32> {
    use Code as C;
    match code {
        C::Fadd_st0_sti
        | C::Fmul_st0_sti
        | C::Fcom_st0_sti
        | C::Fcomp_st0_sti
        | C::Fsub_st0_sti
        | C::Fsubr_st0_sti
        | C::Fdiv_st0_sti
        | C::Fdivr_st0_sti
        | C::Fxch_st0_sti
        | C::Fucomi_st0_sti
        | C::Fcomi_st0_sti
        | C::Fucom_st0_sti
        | C::Fucomp_st0_sti
        | C::Fucomip_st0_sti
        | C::Fcomip_st0_sti => Some(0),
        C::Faddp_sti_st0
        | C::Fmulp_sti_st0
        | C::Fsubrp_sti_st0
        | C::Fsubp_sti_st0
        | C::Fdivrp_sti_st0
        | C::Fdivp_sti_st0 => Some(1),
        _ => None,
    }
}

/// The relative branch target as capstone computes it (address + size + displacement, 32 bits, whatever the
/// operand size).
fn branch_target(ins: &Instruction, bytes: &[u8], size: u32, address: u32) -> i64 {
    let short = ins.is_jcc_short()
        || ins.is_jmp_short()
        || ins.is_loop()
        || ins.is_loopcc()
        || matches!(ins.mnemonic(), Mnemonic::Jcxz | Mnemonic::Jecxz);
    let addr16 = bytes.get(..prefixes(bytes).len).is_some_and(|pre| pre.contains(&0x67));
    let wide = ins.op_kind(0) != OpKind::NearBranch16;
    if short {
        // a byte displacement, sign-extended, the target in 32 bits even under a 0x66 prefix
        let rel = i64::from(bytes.get((size as usize).wrapping_sub(1)).copied().unwrap_or(0) as i8);
        return (i64::from(address) + i64::from(size) + rel) & 0xffff_ffff;
    }
    if wide && !addr16 {
        return i64::from(ins.near_branch32());
    }
    let end = size as usize;
    let n = if wide { 4 } else { 2 };
    let mut raw = 0u64;
    for k in 0..n {
        let b = bytes.get(end.wrapping_sub(n - k)).copied().unwrap_or(0);
        raw |= u64::from(b) << (8 * k);
    }
    // call's displacement is sign-extended by its own size; jmp's and jcc's (ENCODING_Iv) by capstone's displacement
    // size: 16 bits under a 0x67 prefix (whatever the displacement's size), else 32 (so a 16-bit one stays positive)
    let rel = if ins.mnemonic() == Mnemonic::Call {
        if wide {
            i64::from(raw as u32 as i32)
        } else {
            i64::from(raw as u16 as i16)
        }
    } else if addr16 {
        if raw & 0x8000 != 0 {
            (raw | !0xffff) as i64
        } else {
            raw as i64
        }
    } else {
        raw as i64
    };
    (i64::from(address) + i64::from(size) + rel) & 0xffff_ffff
}

fn is_moffs(code: Code) -> bool {
    use Code as C;
    matches!(
        code,
        C::Mov_AL_moffs8
            | C::Mov_AX_moffs16
            | C::Mov_EAX_moffs32
            | C::Mov_moffs8_AL
            | C::Mov_moffs16_AX
            | C::Mov_moffs32_EAX
    )
}

fn push_segment(op_str: &mut String, p: &Prefixes) {
    let seg = match p.segment {
        0x2E => "cs:",
        0x36 => "ss:",
        0x3E => "ds:",
        0x26 => "es:",
        0x64 => "fs:",
        0x65 => "gs:",
        _ => "",
    };
    op_str.push_str(seg);
}

#[allow(clippy::too_many_lines)]
fn convert(ins: &Instruction, bytes: &[u8], address: u32) -> Option<Insn> {
    use Mnemonic as M;
    let size = accepted_size(ins, bytes)?;
    let p = prefixes(bytes);
    let m = ins.mnemonic();
    let code = ins.code();
    let mut op_str = String::new();
    let mut operands = Vec::new();
    // ud0 / ud1 (no ModRM read), xlatb, and invlpga: capstone's fixed texts
    if matches!(m, M::Ud0 | M::Ud1) {
        let mnemonic = format!("{}{}", prefix_text(ins, &p), mnemonic_name(ins, None));
        return Some(Insn {
            address,
            size,
            mnemonic,
            op_str,
            operands,
        });
    }
    if m == M::Xlatb {
        let mnemonic = format!("{}xlatb", prefix_text(ins, &p));
        return Some(Insn {
            address,
            size,
            mnemonic,
            op_str,
            operands,
        });
    }
    if m == M::Invlpga {
        let mnemonic = format!("{}invlpga", prefix_text(ins, &p));
        return Some(Insn {
            address,
            size,
            mnemonic,
            op_str: "eax, ecx".into(),
            operands,
        });
    }
    // the compare pseudo-ops: the predicate in the name, the immediate gone
    let last = ins.op_count().saturating_sub(1);
    let cmp_imm = if matches!(
        m,
        M::Cmpps | M::Cmppd | M::Cmpss | M::Cmpsd | M::Vcmpps | M::Vcmppd | M::Vcmpss | M::Vcmpsd
    ) && ins.op_count() > 0
        && ins.op_kind(last) == OpKind::Immediate8
    {
        let v = ins.immediate8();
        let limit = if matches!(m, M::Vcmpps | M::Vcmppd | M::Vcmpss | M::Vcmpsd) {
            32
        } else {
            8
        };
        (v < limit).then_some(v)
    } else {
        None
    };
    let mnemonic = format!("{}{}", prefix_text(ins, &p), mnemonic_name(ins, cmp_imm));
    // direct far branches: "ljmp sel:off", "lcall sel, off"
    if (0..ins.op_count()).any(|k| matches!(ins.op_kind(k), OpKind::FarBranch16 | OpKind::FarBranch32)) {
        let sel = i64::from(ins.far_branch_selector());
        let off = if ins.op_kind(0) == OpKind::FarBranch32 {
            i64::from(ins.far_branch32())
        } else {
            i64::from(ins.far_branch16())
        };
        print_imm(&mut op_str, sel, true, 0);
        op_str.push_str(if m == M::Jmp { ":" } else { ", " });
        print_imm(&mut op_str, off, true, 0);
        operands.push(Operand::Imm(sel));
        operands.push(Operand::Imm(off));
        return Some(Insn {
            address,
            size,
            mnemonic,
            op_str,
            operands,
        });
    }
    let opsize_one = imm_size_one(ins);
    let dropped = x87_dropped(code);
    let narrow = matches!(
        code,
        Code::Arpl_r32m16_r32
            | Code::Verw_r32m16
            | Code::Verr_r32m16
            | Code::Ltr_r32m16
            | Code::Lldt_r32m16
            | Code::Lmsw_r32m16
    );
    let mut op1_size = 0u32;
    let mut first = true;
    let count = if cmp_imm.is_some() { last } else { ins.op_count() };
    for k in 0..count {
        if dropped == Some(k) {
            continue;
        }
        let kind = ins.op_kind(k);
        // implicit memory operands capstone does not print (maskmovq's [edi])
        if matches!(kind, OpKind::MemorySegDI | OpKind::MemorySegEDI | OpKind::MemorySegRDI) {
            continue;
        }
        // rcl of memory by one: capstone prints no count (its detail still has the 1)
        if matches!(code, Code::Rcl_rm8_1 | Code::Rcl_rm16_1 | Code::Rcl_rm32_1)
            && k == 1
            && ins.op_kind(0) == OpKind::Memory
        {
            operands.push(Operand::Imm(1));
            continue;
        }
        // reservednop ("nop r/m"): only its r/m operand
        if m == M::Reservednop && k > 0 && !nop_two_operands(bytes, &p) {
            continue;
        }
        if !first {
            op_str.push_str(", ");
        }
        first = false;
        match kind {
            OpKind::Register => {
                let mut r = ins.op_register(k);
                if narrow {
                    r = reg16(r);
                }
                if r.is_cr() || r.is_dr() {
                    // named from the ModRM byte (iced_decode decoded an unknown one as cr0 / dr0)
                    let n = bytes.get(p.len + 2).map_or(0, |m| (m >> 3) & 7);
                    let _ = write!(op_str, "{}{n}", if r.is_cr() { "cr" } else { "dr" });
                    op1_size = op1_size.max(4);
                    continue;
                }
                op_str.push_str(&reg_name(r));
                if op1_size == 0 {
                    op1_size = reg_size(r);
                }
                if k == 0 && ins.op_mask() != Register::None {
                    let _ = write!(op_str, " {{{}}}", reg_name(ins.op_mask()));
                    if ins.zeroing_masking() {
                        op_str.push_str(" {z}");
                    }
                }
            }
            OpKind::NearBranch16 | OpKind::NearBranch32 | OpKind::NearBranch64 => {
                let t = branch_target(ins, bytes, size, address);
                print_imm(&mut op_str, t, true, op1_size);
                operands.push(Operand::Imm(t));
            }
            OpKind::Immediate8
            | OpKind::Immediate8_2nd
            | OpKind::Immediate16
            | OpKind::Immediate32
            | OpKind::Immediate8to16
            | OpKind::Immediate8to32
            | OpKind::Immediate64
            | OpKind::Immediate8to64
            | OpKind::Immediate32to64 => {
                if ins.suppress_all_exceptions() {
                    op_str.push_str("{sae}, ");
                }
                let u8imm = kind == OpKind::Immediate8 && u8_imm(ins);
                let mut v = match kind {
                    OpKind::Immediate8 if u8imm => i64::from(ins.immediate8()),
                    OpKind::Immediate8 => i64::from(ins.immediate8() as i8),
                    OpKind::Immediate8_2nd => i64::from(ins.immediate8_2nd() as i8),
                    OpKind::Immediate16 if matches!(m, M::Enter | M::Ret | M::Retf) => {
                        i64::from(ins.immediate16() as i16)
                    }
                    OpKind::Immediate16 => i64::from(ins.immediate16()),
                    OpKind::Immediate8to16 => i64::from(ins.immediate8to16()),
                    OpKind::Immediate8to32 => i64::from(ins.immediate8to32()),
                    _ => i64::from(ins.immediate32()),
                };
                if opsize_one {
                    v &= 0xff;
                }
                if u8imm {
                    print_imm(&mut op_str, v, true, op1_size);
                    operands.push(Operand::Imm(v));
                    continue;
                }
                match m {
                    M::Mov => print_imm(&mut op_str, v, true, op1_size),
                    M::In | M::Out | M::Int => {
                        v &= 0xff;
                        print_imm(&mut op_str, v, true, op1_size);
                    }
                    M::And | M::Or | M::Xor => {
                        if !(0..=HEX_THRESHOLD).contains(&v) {
                            let sz = if opsize_one { 1 } else { first_operand_size(ins) };
                            v &= match sz {
                                1 => 0xff,
                                2 => 0xffff,
                                4 => 0xffff_ffff,
                                _ => 0,
                            };
                        }
                        print_imm(&mut op_str, v, true, op1_size);
                    }
                    M::Ret | M::Retf => {
                        if !(0..=HEX_THRESHOLD).contains(&v) {
                            v &= 0xffff;
                        }
                        print_imm(&mut op_str, v, true, op1_size);
                    }
                    _ => print_imm(&mut op_str, v, false, op1_size),
                }
                operands.push(Operand::Imm(v));
            }
            OpKind::Memory => {
                let (kw, msize) = mem_keyword(ins);
                op_str.push_str(kw);
                push_segment(&mut op_str, &p);
                op_str.push('[');
                let base = ins.memory_base();
                let index = ins.memory_index();
                let raw = ins.memory_displacement32();
                let addr16 =
                    base.is_gpr16() || index.is_gpr16() || (ins.memory_displ_size() == 2 && base == Register::None);
                let disp = if is_moffs(code) {
                    if addr16 {
                        i64::from(raw & 0xffff)
                    } else {
                        i64::from(raw as i32)
                    }
                } else if addr16 {
                    i64::from(raw as u16 as i16)
                } else {
                    i64::from(raw as i32)
                };
                let mut need_plus = false;
                if base != Register::None {
                    op_str.push_str(&reg_name(base));
                    need_plus = true;
                }
                if index != Register::None {
                    if need_plus {
                        op_str.push_str(" + ");
                    }
                    op_str.push_str(&reg_name(index));
                    let scale = ins.memory_index_scale();
                    if scale != 1 {
                        let _ = write!(op_str, "*{scale}");
                    }
                    need_plus = true;
                }
                if disp != 0 {
                    if need_plus {
                        if disp < 0 {
                            op_str.push_str(" - ");
                            print_imm(&mut op_str, -disp, true, op1_size);
                        } else {
                            op_str.push_str(" + ");
                            print_imm(&mut op_str, disp, true, op1_size);
                        }
                    } else if disp < 0 {
                        print_imm(&mut op_str, disp & 0xffff_ffff, true, op1_size);
                    } else {
                        print_imm(&mut op_str, disp, true, op1_size);
                    }
                } else if !need_plus {
                    op_str.push('0');
                }
                op_str.push(']');
                operands.push(Operand::Mem { base, index, disp });
                if op1_size == 0 {
                    op1_size = msize;
                }
            }
            OpKind::MemorySegSI | OpKind::MemorySegESI | OpKind::MemoryESDI | OpKind::MemoryESEDI => {
                let (kw, msize) = mem_keyword(ins);
                op_str.push_str(kw);
                let reg = match kind {
                    OpKind::MemorySegSI => Register::SI,
                    OpKind::MemorySegESI => Register::ESI,
                    OpKind::MemoryESDI => Register::DI,
                    _ => Register::EDI,
                };
                if matches!(kind, OpKind::MemoryESDI | OpKind::MemoryESEDI) {
                    op_str.push_str("es:");
                } else {
                    push_segment(&mut op_str, &p);
                }
                op_str.push('[');
                op_str.push_str(&reg_name(reg));
                op_str.push(']');
                operands.push(Operand::Mem {
                    base: reg,
                    index: Register::None,
                    disp: 0,
                });
                if op1_size == 0 {
                    op1_size = msize;
                }
            }
            _ => {}
        }
    }
    if code == Code::Fstpnce_sti {
        op_str.push_str(", st(0)");
    }
    Some(Insn {
        address,
        size,
        mnemonic,
        op_str,
        operands,
    })
}
