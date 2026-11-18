//! bc2-harness on small inputs: capstone's text for encodings where capstone and iced differ, the opfor.log readers,
//! the pair bits and the cell index. (The full parity runs are Python's: crash-reader/tests/test_crash_native.py.)

#![allow(
    clippy::assert_is_empty,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    missing_docs
)]

use bc2_harness::graph::{CellIndex, pair_bits};
use bc2_harness::oplog::{log_before_crash, sessions};
use bc2_harness::x86::{Disassembler, Operand};

fn text(bytes: &[u8], at: u32) -> Option<(u32, String)> {
    let mut code = bytes.to_vec();
    code.resize(16, 0x90);
    Disassembler::new()
        .decode(&code, at)
        .expect("a proved encoding")
        .map(|i| (i.size, format!("{} {}", i.mnemonic, i.op_str).trim_end().to_string()))
}

#[test]
fn capstone_text() {
    // each as capstone 5.0 decodes it in BFBC2Game.exe
    assert_eq!(
        text(&[0x8b, 0x44, 0x24, 0x08], 0x40_1010),
        Some((4, "mov eax, dword ptr [esp + 8]".into()))
    );
    assert_eq!(text(&[0xf6, 0xc8, 0xf5], 0x5b_23c6), Some((3, "test al, -0xb".into())));
    assert_eq!(
        text(&[0x05, 0x00, 0x00, 0x85, 0xc0], 0x40_1b68),
        Some((5, "add eax, 0xc0850000".into()))
    );
    assert_eq!(
        text(&[0x66, 0xe9, 0xff, 0xff], 0x57_aa26),
        Some((4, "jmp 0x58aa29".into()))
    );
    assert_eq!(
        text(&[0x66, 0xe8, 0xe6, 0x8b], 0x58_f004),
        Some((4, "call 0x587bee".into()))
    );
    assert_eq!(text(&[0xd9, 0xc9], 0x40_283e), Some((2, "fxch st(1)".into())));
    assert_eq!(text(&[0xde, 0xc1], 0x40_29b2), Some((2, "faddp st(1)".into())));
    assert_eq!(text(&[0xdc, 0xdd], 0x40_55dc), None);
    assert_eq!(text(&[0x0f, 0x22, 0x75], 0x4f_8784), Some((3, "mov cr6, ebp".into())));
    assert_eq!(text(&[0x0f, 0xff, 0x24, 0x85], 0x40_f787), Some((2, "ud0".into())));
    assert_eq!(text(&[0x60], 0x40_1c1a), Some((1, "pushal".into())));
    assert_eq!(
        text(&[0xf3, 0xa5], 0x40_17e7),
        Some((2, "rep movsd dword ptr es:[edi], dword ptr [esi]".into()))
    );
    let i = Disassembler::new()
        .decode(
            &[0x68, 0x70, 0xb6, 0x40, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            0x95_b7d3,
        )
        .unwrap()
        .unwrap();
    assert_eq!(i.operands, vec![Operand::Imm(0x140_b670)]);
}

const LOG: &str = "[10:00:00.000] ---- opfor.dll loaded into PID 100 ----\n\
[10:00:01.000] dev trigger: level='Levels/ONS_MP_002' autostart=1\r\n\
probe Session::updateState (arg1 = state) ecx=180AD880 arg1=00000003 (3)\n\
probe AI: init\x0bplain\u{2028}probe AI: sector offsets load handle 0 0 0; object 0  X(+1Ch)=-3 Z(+18h)=-7\n\
unit counters (interval): 21 pathfinders\n\
the game is writing crashreport.xml\n";

#[test]
fn log_sessions() {
    let s = sessions(LOG).unwrap();
    assert_eq!(s.len(), 1);
    let s = &s[0];
    assert_eq!(
        (s.pid.as_str(), s.start.as_str(), s.level.as_deref()),
        ("100", "10:00:00", Some("Levels/ONS_MP_002"))
    );
    assert_eq!((s.states.clone(), s.events.clone()), (vec![3], vec!["AI init"]));
    assert_eq!(
        (s.offset_loads.clone(), s.pathfinders.clone()),
        (vec![(-3, -7)], vec![21])
    );
    assert!(s.crashed && !s.exited);
    assert_eq!(s.lines, 7);
    assert!(sessions("---- opfor.dll loaded into PID 1 ----\nunit counters (exit): \u{663} pathfinders\n").is_err());
}

#[test]
fn crash_window() {
    let got = log_before_crash(LOG.as_bytes(), 1, None).unwrap();
    assert_eq!(
        got,
        vec![
            "unit counters (interval): 21 pathfinders",
            "the game is writing crashreport.xml"
        ]
    );
    assert!(log_before_crash(b"nothing\n", 18, None).unwrap().is_empty());
    assert!(
        log_before_crash(b"[23:59:00.0] crashreport.xml\n", 18, Some(12 * 3600))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        log_before_crash(b"[23:59:00.0] crashreport.xml\n", 18, Some(30))
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn bits_and_cells() {
    // 0 -> 1 -> 2, 3 alone
    let start = [0, 1, 2, 2, 2];
    let nbr = [1, 2];
    assert_eq!(
        pair_bits(&start, &nbr, &[Some(0), None, Some(2)], &[Some(2), Some(3)]).unwrap(),
        "10--10"
    );
    assert!(pair_bits(&start, &nbr, &[Some(9)], &[Some(0)]).is_err());
    let idx = CellIndex::new(
        vec![[0.0, 0.0], [2.0, 0.0], [2.4, 0.0], [-2.0, 2.0]],
        vec![1.0, 2.0, 3.0, 4.0],
        4.0,
    );
    assert_eq!(idx.cells(0.0, 0.0, 2.0).unwrap(), vec![1.0, 2.0, 4.0]);
    assert!(idx.cells(f64::NAN, 0.0, 2.0).is_none());
}
