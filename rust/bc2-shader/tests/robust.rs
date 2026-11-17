//! The decoder and the translator never panic: any bytes, and valid containers around random microcode, are decoded
//! or refused, and whatever decodes is translated or refused (parity with the Python is
//! `bfbc2-shader-tools/tests/test_xenos_native.py`'s).
#![allow(clippy::indexing_slicing, clippy::unwrap_used)]

use bc2_shader::hlsl::{self, Interface};
use bc2_shader::xenos;
use proptest::prelude::*;

/// A pixel or vertex shader container with an empty header around `code` (whole 3-word slots).
fn container(pixel: bool, interps: &[u32], code: &[u32]) -> Vec<u8> {
    let mut b = Vec::new();
    let hdr: Vec<u32> = if pixel {
        let mut h = vec![
            0,
            4 * code.len() as u32,
            0x0f0f,
            0,
            0,
            (interps.len() as u32) << 5,
            0,
            1,
        ];
        h.extend(interps);
        h
    } else {
        let mut h = vec![
            0,
            4 * code.len() as u32,
            0x0f0f,
            0,
            0,
            (interps.len() as u32) << 5,
            1,
            0,
            0,
            0x290,
        ];
        h.extend(interps);
        h
    };
    let vsize = 0x24 + 4 * hdr.len() as u32;
    let words = [
        0x102a_1100 | u32::from(!pixel),
        vsize,
        4 * code.len() as u32,
        0,
        0,
        0,
        0x24,
        0,
        0,
    ];
    for w in words.iter().chain(&hdr).chain(code) {
        b.extend_from_slice(&w.to_be_bytes());
    }
    b
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn any_bytes(data in proptest::collection::vec(any::<u8>(), 0..400), pixel in any::<Option<bool>>()) {
        if let Ok(sh) = xenos::parse(&data, pixel) {
            let _ = hlsl::translate(&sh, &Interface::new(vec![0, 1, 2]));
        }
    }

    #[test]
    fn random_microcode(
        pixel in any::<bool>(),
        slots in 1usize..24,
        seed in proptest::collection::vec(any::<u32>(), 72),
        interps in proptest::collection::vec(0u32..0x10000, 0..4),
    ) {
        let code: Vec<u32> = seed.iter().copied().take(3 * slots).collect();
        let blob = container(pixel, &interps, &code);
        if let Ok(sh) = xenos::parse(&blob, None) {
            let mut iface = Interface::new((0..16).collect());
            iface.instanced = seed[0] & 1 != 0;
            iface.pc_shadow_atlas = true;
            let _ = hlsl::translate(&sh, &iface);
        }
    }
}

#[test]
fn a_blob_cut_anywhere_is_refused_not_a_panic() {
    let code = [0x0000_1001, 0x2000, 0, 0x0123_4567, 0x89ab_cdef, 0x0102_0304];
    let blob = container(true, &[0x5f50], &code);
    assert!(xenos::parse(&blob, None).is_ok());
    for n in 0..blob.len() {
        assert!(xenos::parse(&blob[..n], None).is_err());
    }
}
