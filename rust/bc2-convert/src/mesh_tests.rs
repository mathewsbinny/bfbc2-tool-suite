#![expect(clippy::unwrap_used, reason = "tests")]

use super::*;

#[test]
fn halves_round_to_nearest_even_from_the_double() {
    for (v, want) in [
        (0.0, 0x0000),
        (-0.0, 0x8000),
        (1.0, 0x3C00),
        (-1.0, 0xBC00),
        (0.5, 0x3800),
        (1.0 / 3.0, 0x3555),
        (65504.0, 0x7BFF),
        (65519.0, 0x7BFF),
        (65520.0, 0x7C00),
        (1e9, 0x7C00),
        (2f64.powi(-14), 0x0400),
        (2f64.powi(-24), 0x0001),
        (2f64.powi(-25), 0x0000),
        (1.5 * 2f64.powi(-25), 0x0001),
        (3.0 * 2f64.powi(-25), 0x0002),
        (1e-300, 0x0000),
        (f64::INFINITY, 0x7C00),
    ] {
        assert_eq!(f64_to_f16(v), want, "{v}");
    }
    // a tie just above a half's value: rounded to the even mantissa (1 + 2^-11 is halfway between 1 and 1 + 2^-10)
    assert_eq!(f64_to_f16(1.0 + 2f64.powi(-11)), 0x3C00);
    assert_eq!(f64_to_f16(1.0 + 3.0 * 2f64.powi(-11)), 0x3C02);
    // one double rounding would make this 0x3C00 (through a float it is exactly the tie)
    assert_eq!(f64_to_f16(1.0 + 2f64.powi(-11) + 2f64.powi(-40)), 0x3C01);
}

#[test]
fn packed_normals_and_bytes() {
    let mut d = [0u8; 8];
    // x = 1023 (1.0), y = -1024 (clamped to -1), z = 0
    let v: u32 = 1023 | (1024 << 11);
    assert!(element(0x2B, 0x08, &v.to_be_bytes(), false, &mut d));
    assert_eq!(d, [0x00, 0x3C, 0x00, 0xBC, 0x00, 0x00, 0x00, 0x3C]);
    assert!(element(0x0D, 0x08, &[0, 0, 255, 255], false, &mut d));
    assert_eq!(d, [0x00, 0x3C, 0x00, 0x3C, 0x00, 0x00, 0x00, 0x00]);
}

#[test]
fn swaps_by_value_width() {
    let mut d = [0u8; 4];
    assert!(element(0x06, 0x06, &[1, 2, 3, 4], false, &mut d));
    assert_eq!(d, [2, 1, 4, 3]);
    assert!(element(0x0C, 0x0C, &[1, 2, 3, 4], false, &mut d));
    assert_eq!(d, [1, 2, 3, 4]);
    assert!(element(0x0C, 0x0C, &[1, 2, 3, 4], true, &mut d));
    assert_eq!(d, [4, 3, 2, 1]);
    assert!(element(0x02, 0x02, &[1, 2, 3, 4, 5, 6, 7, 8], false, &mut d.repeat(2)));
}

#[test]
fn refuses_what_the_reference_does_not_convert() {
    let p = |fc, fp, op| Pair { fc, oc: 0, fp, op };
    let data = [0u8; 64];
    assert!(convert_vertices(&data, 0, 4, 8, 0, 1, &[p(0x2B, 0x06, 0)], false).is_none()); // no such conversion
    assert!(convert_vertices(&data, 0, 4, 8, 0, 1, &[p(0x01, 0x01, 0)], false).is_none()); // unknown format
    assert!(convert_vertices(&data, 0, 4, 8, 0, 1, &[p(0x2B, 0x08, 4)], false).is_none()); // past the PC vertex
    assert!(convert_vertices(&data, 0, 4, 8, 0, 17, &[p(0x06, 0x06, 0)], false).is_none()); // past the data
    assert_eq!(
        convert_vertices(&data, 0, 4, 8, 0, 16, &[p(0x06, 0x06, 0)], false)
            .unwrap()
            .len(),
        128
    );
    // no vertices: nothing to refuse, as the reference's empty loop
    assert_eq!(
        convert_vertices(&data, 0, 4, 8, 0, 0, &[p(0x01, 0x01, 0)], false),
        Some(Vec::new())
    );
}

#[test]
fn vertices_in_order_with_zeroed_gaps() {
    let data: Vec<u8> = (0..32).collect();
    let pairs = [
        Pair {
            fc: 0x05,
            oc: 2,
            fp: 0x05,
            op: 4,
        },
        Pair {
            fc: 0x0C,
            oc: 4,
            fp: 0x0C,
            op: 0,
        },
    ];
    let out = convert_vertices(&data, 4, 8, 6, 1, 2, &pairs, false).unwrap();
    // vertex 1 at 12, vertex 2 at 20: the UBYTE4 at +4 kept, the half at +2 swapped
    assert_eq!(out, [16, 17, 18, 19, 15, 14, 24, 25, 26, 27, 23, 22]);
}
