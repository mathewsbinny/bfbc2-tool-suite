//! The forward emitter (spec XSH-CF-1) on hand-built programs, without game data: an if / else whose two forward
//! jumps overlap is translated with a `_cf` guard on every block and the jumps as assignments to it; programs whose
//! jumps nest keep the nested emitter (no `_cf`); a bool-constant jump, a backward jump and a bool-constant
//! `cond_exec` are refused with the nested emitter's errors. Its results against the Xenos interpreter are
//! `bfbc2-shader-tools/tests/test_xdiff.py`'s `forward_jumps` (the same program, drawn).
#![allow(clippy::indexing_slicing, clippy::unwrap_used)]

use bc2_shader::hlsl::{self, Interface};
use bc2_shader::xenos;

/// A pixel shader container (no constant table, no literals) with one TEXCOORD0 interpolator in r0 around `code`.
fn container(code: &[u32]) -> Vec<u8> {
    let interp = 0xf050; // usage index 0, usage TEXCOORD (5), register 0, mask 0xf
    let hdr = [0, 4 * code.len() as u32, 0x0f0f, 0, 0, 1 << 5, 0, 1, interp];
    let vsize = 0x24 + 4 * hdr.len() as u32;
    let words = [0x102a_1100, vsize, 4 * code.len() as u32, 0, 0, 0, 0x24, 0, 0];
    let mut b = Vec::new();
    for w in words.iter().chain(&hdr).chain(code) {
        b.extend_from_slice(&w.to_be_bytes());
    }
    b
}

#[derive(Clone, Copy)]
enum Cf {
    /// opcode (1 `exec`, 2 `exec_end`, 3 `cond_exec` on a bool constant), first instruction, count
    Exec(u32, u32, u32),
    /// target CF index, kind (0 bool constant, 1 unconditional, 2 predicated), condition
    Jmp(u32, u32, u32),
}

/// `p0 = r0.w > 0` (scalar `setp_gt`; the vector half a nop)
const SETP: [u32; 3] = [29 << 26, 0, (2 << 24) | (7 << 29)];
/// r1 = r0 + r0
const ADD: [u32; 3] = [1 | (0xf << 16) | (50 << 26), 0, 7 << 29];
/// r1 = r0 * r0
const MUL: [u32; 3] = [1 | (0xf << 16) | (50 << 26), 0, (1 << 24) | (7 << 29)];
/// oC0 = max(r1, r1)
const OUT: [u32; 3] = [
    (1 << 15) | (0xf << 16) | (50 << 26),
    0,
    (2 << 24) | (7 << 29) | (1 << 16) | (1 << 8),
];

/// The microcode: the CFs (two per 3-word slot), then the instructions.
fn program(cfs: &[Cf], ins: &[[u32; 3]]) -> Vec<u8> {
    let slots = cfs.len().div_ceil(2) as u32;
    let mut enc: Vec<(u32, u32)> = cfs
        .iter()
        .map(|c| match *c {
            Cf::Exec(op, first, n) => ((slots + first) | (n << 12), op << 12),
            Cf::Jmp(target, kind, cond) => (
                target | (u32::from(kind == 1) << 13) | (u32::from(kind == 2) << 14),
                (11 << 12) | (cond << 10),
            ),
        })
        .collect();
    if enc.len() % 2 == 1 {
        enc.push((0, 0));
    }
    let mut code = Vec::new();
    for p in enc.chunks(2) {
        let (a, b) = (p[0], p[1]);
        code.extend([a.0, (a.1 & 0xffff) | ((b.0 & 0xffff) << 16), (b.0 >> 16) | (b.1 << 16)]);
    }
    for i in ins {
        code.extend(i);
    }
    container(&code)
}

fn translate(cfs: &[Cf]) -> Result<String, String> {
    let sh = xenos::parse(&program(cfs, &[SETP, ADD, MUL, OUT]), Some(true)).unwrap();
    hlsl::translate(&sh, &Interface::new(vec![0])).map_err(|e| e.to_string())
}

/// if (p0) goto 4; r1 = r0 + r0; goto 5; 4: r1 = r0 * r0; 5: oC0 = r1 - the jumps 1 -> 4 and 3 -> 5 overlap.
const IF_ELSE: [Cf; 6] = [
    Cf::Exec(1, 0, 1),
    Cf::Jmp(4, 2, 1),
    Cf::Exec(1, 1, 1),
    Cf::Jmp(5, 1, 0),
    Cf::Exec(1, 2, 1),
    Cf::Exec(2, 3, 1),
];

#[test]
fn overlapping_forward_jumps_take_the_forward_emitter() {
    let text = translate(&IF_ELSE).unwrap();
    let body: Vec<&str> = text
        .lines()
        .map(str::trim)
        .skip_while(|l| *l != "int _cf = 0;")
        .take_while(|l| *l != "PSOut po;")
        .collect();
    let want = [
        "int _cf = 0;",
        "if (_cf <= 0) {",
        "p0_next = (r0.w) > 0.0;",
        "float s3 = ((r0.w) > 0.0 ? 0.0 : 1.0);",
        "ps = s3;",
        "p0 = p0_next;",
        "}",
        "if (_cf <= 1 && (p0 == true)) { _cf = 4; }",
        "if (_cf <= 2) {",
        "float4 v4 = (r0.xyzw + r0.xyzw);",
        "r1.x = v4.x;",
        "r1.y = v4.y;",
        "r1.z = v4.z;",
        "r1.w = v4.w;",
        "}",
        "if (_cf <= 3 && (true)) { _cf = 5; }",
        "if (_cf <= 4) {",
        "float4 v5 = xmul(r0.xyzw, r0.xyzw);",
        "r1.x = v5.x;",
        "r1.y = v5.y;",
        "r1.z = v5.z;",
        "r1.w = v5.w;",
        "}",
        "if (_cf <= 5) {",
        "float4 v6 = max(r1.xyzw, r1.xyzw);",
        "oC0.x = v6.x;",
        "oC0.y = v6.y;",
        "oC0.z = v6.z;",
        "oC0.w = v6.w;",
        "}",
    ];
    assert_eq!(body, want, "{text}");
}

#[test]
fn nested_jumps_keep_the_nested_emitter() {
    // if (p0) skip 2 .. 3 - one jump, nested
    let cfs = [
        Cf::Exec(1, 0, 1),
        Cf::Jmp(4, 2, 1),
        Cf::Exec(1, 1, 1),
        Cf::Exec(1, 2, 1),
        Cf::Exec(2, 3, 1),
    ];
    let text = translate(&cfs).unwrap();
    assert!(!text.contains("_cf"), "{text}");
    assert!(text.contains("if (p0 != true) {"), "{text}");
}

#[test]
fn what_the_forward_emitter_does_not_take_fails_as_before() {
    // a jump on a bool constant beside the overlap: the nested emitter's error
    let mut cfs = IF_ELSE;
    cfs[3] = Cf::Jmp(5, 0, 0);
    assert_eq!(translate(&cfs).unwrap_err(), "bool-constant jump");
    // a backward jump beside the overlap: the nested emitter's error, at the first jump it cannot nest (3 -> 6)
    let cfs = [
        Cf::Exec(1, 0, 1),
        Cf::Jmp(4, 2, 1),
        Cf::Exec(1, 1, 1),
        Cf::Jmp(6, 1, 0),
        Cf::Exec(1, 2, 1),
        Cf::Jmp(0, 2, 0),
        Cf::Exec(2, 3, 1),
    ];
    assert_eq!(
        translate(&cfs).unwrap_err(),
        "jump to cf 6 is not a nested forward jump"
    );
    // a cond_exec on a bool constant inside the overlap: refused as the nested emitter refuses it (the forward
    // emitter had run it unconditionally)
    let mut cfs = IF_ELSE;
    cfs[2] = Cf::Exec(3, 1, 1);
    assert_eq!(translate(&cfs).unwrap_err(), "control flow cond_exec is not supported");
}
