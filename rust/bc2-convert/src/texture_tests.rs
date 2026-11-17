//! `texture`'s unit and property tests; the expected bytes are `texture.convert_python`'s on the same inputs.
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use super::*;
use proptest::prelude::*;

/// A console texture: the header and the layout's bytes plus one, in the pattern (i * 31 + 7) % 251.
fn built(fmt: u32, w: usize, h: usize, mips: usize, typ: u32) -> Vec<u8> {
    let (_, size) = layout(w, h, mips, fmt, if typ == CUBE { 6 } else { 1 }).unwrap();
    let mut b = Vec::new();
    for v in [3, typ, fmt, 3, w as u32, h as u32, 0, mips as u32] {
        b.extend(v.to_be_bytes());
    }
    b.extend([0u8; 60]);
    b.extend((0..=size).map(|i| ((i * 31 + 7) % 251) as u8));
    b
}

fn sha1(b: &[u8]) -> String {
    sha1_smol::Sha1::from(b).digest().to_string()
}

fn header_only(w: [u32; 8]) -> Vec<u8> {
    let mut b: Vec<u8> = w.iter().flat_map(|v| v.to_be_bytes()).collect();
    b.extend([0u8; 60]);
    b
}

#[test]
fn tiling_as_the_reference() {
    let got: Vec<usize> = [
        (0, 0, 32, 3),
        (5, 9, 32, 3),
        (40, 17, 64, 4),
        (3, 31, 32, 0),
        (100, 77, 128, 2),
        (7, 8, 32, 1),
    ]
    .iter()
    .map(|&(x, y, p, l)| tiled(x, y, p, l))
    .collect();
    assert_eq!(got, [0, 563, 1157, 2491, 11692, 327]);
    assert_eq!(packed(1024, 64, 3, 4), Some((0, 2)));
    assert_eq!(packed(64, 1024, 6, 4), Some((0, 16)));
    assert_eq!(packed(64, 1024, 0, 4), None);
    assert_eq!(packed(8, 8, 0, 1), Some((16, 0)));
}

#[test]
fn layouts_as_the_reference() {
    let (w, size) = layout(64, 32, 7, 2, 1).unwrap();
    assert_eq!(size, 32768);
    let got: Vec<_> = w[0].iter().map(|p| (p.start, p.pitch, p.x, p.y)).collect();
    assert_eq!(
        got,
        [
            (0, 32, 0, 0),
            (16384, 32, 0, 4),
            (16384, 32, 0, 2),
            (16384, 32, 0, 1),
            (16384, 32, 4, 0),
            (16384, 32, 2, 0),
            (16384, 32, 1, 0)
        ]
    );
    let (w, size) = layout(16, 16, 5, 0, 6).unwrap();
    assert_eq!(size, 49152);
    assert_eq!(w.len(), 6);
    assert_eq!((w[5][0].start, w[5][4].x, w[5][4].y), (40960, 0, 1));
    assert!(layout(4, 4, 1, 77, 1).is_none());
}

/// (format, width, height, mips, type) of a built texture.
type Args = (u32, usize, usize, usize, u32);

#[test]
fn conversions_as_the_reference() {
    // (format, width, height, mips, type) -> (length, sha1) of texture.convert_python on the same bytes
    let cases: [(Args, usize, &str); 10] = [
        ((0, 64, 64, 7, 0), 2836, "7317e323fb88d6e754e415e66bc1fe3fcf38b103"),
        ((3, 32, 16, 6, 0), 780, "673292209fd7462534e4f5650de1b20378f35d25"),
        ((3, 6, 10, 4, 0), 204, "65d01ad502298238550177c57a8dad18e51d1a0c"),
        ((5, 16, 8, 5, 0), 776, "c5b0be5b9341b816d6eeeeae40c01fb19e44ebdb"),
        ((9, 8, 8, 4, 0), 432, "fa781a177362247d200a7ef85b20898ed040d904"),
        ((10, 16, 8, 5, 0), 268, "446e7828feb5f772ecad35a99d9e28346f87486e"),
        ((10, 6, 3, 2, 0), 120, "e7bbbc88f824ecc310493470d77ff8b6bf2f6181"),
        ((2, 16, 16, 5, 1), 2300, "26e41a657d7366ddfa70d29925d072cc95572d52"),
        ((13, 4, 4, 3, 0), 260, "108a68b097deb6117dfab194de564e5b67dad5e3"),
        ((18, 2048, 32, 12, 0), 44132, "cf09f2db0f6a45e48bf342f8afe5882b531ba1b1"),
    ];
    let inputs: Vec<Vec<u8>> = cases
        .iter()
        .map(|&((f, w, h, m, t), _, _)| built(f, w, h, m, t))
        .collect();
    let refs: Vec<&[u8]> = inputs.iter().map(Vec::as_slice).collect();
    for ((args, len, hash), out) in cases.iter().zip(convert_all(&refs)) {
        let out = out.unwrap();
        assert_eq!((out.len(), sha1(&out).as_str()), (*len, *hash), "{args:?}");
    }
}

#[test]
fn refusals_carry_the_reference_messages() {
    let msg = |b: &[u8]| convert(b).unwrap_err().to_string();
    assert_eq!(msg(&[0u8; 10]), "shorter than a texture header");
    assert_eq!(
        msg(&header_only([3, 0, 0, 1, 0, 4, 0, 1])),
        "invalid texture dimensions 0 x 4 / 1 mip levels"
    );
    assert_eq!(
        msg(&header_only([3, 0, 0, 1, 4, 20000, 0, 1])),
        "invalid texture dimensions 4 x 20000 / 1 mip levels"
    );
    assert_eq!(
        msg(&header_only([3, 0, 0, 1, 4, 4, 0, 0])),
        "invalid texture dimensions 4 x 4 / 0 mip levels"
    );
    assert_eq!(
        msg(&header_only([3, 0, 0, 1, 4, 4, 0, 4])),
        "4 mip levels exceed the 4 x 4 chain"
    );
    assert_eq!(
        msg(&header_only([3, 2, 0, 1, 4, 4, 0, 1])),
        "texture type 2 / depth 0 is not converted (2D and cube maps only)"
    );
    assert_eq!(
        msg(&header_only([3, 0, 0, 1, 4, 4, 2, 1])),
        "texture type 0 / depth 2 is not converted (2D and cube maps only)"
    );
    assert_eq!(
        msg(&header_only([3, 0, 7, 1, 4, 4, 0, 1])),
        "texture format 7 is not converted (FORMAT.md 2.4)"
    );
    for f in [3, 5, 9] {
        assert_eq!(
            msg(&header_only([3, 1, f, 1, 4, 4, 0, 1])),
            format!("texture type 1 in console format {f} is not converted (2D only)")
        );
    }
    for (f, need) in [(0, 8192), (3, 8192), (5, 4096)] {
        assert_eq!(
            msg(&header_only([3, 0, f, 1, 4, 4, 0, 1])),
            format!("0 bytes of texture data, the layout needs {need}")
        );
    }
    let mut short = header_only([3, 0, 9, 1, 4, 4, 0, 1]);
    short.extend([0u8; 100]);
    assert_eq!(msg(&short), "100 bytes of texture data, the layout needs 4096");
    assert_eq!(
        TextureError::OutOfRange.to_string(),
        "the layout reads past the end of the texture data"
    );
    assert!(format(42).is_none());
}

#[test]
fn helpers() {
    assert_eq!(rows4(&[1, 2, 3, 4, 5, 6], 3, 2), [1, 2, 3, 0, 4, 5, 6, 0]);
    assert_eq!(rows4(&[1, 2, 3, 4], 4, 1), [1, 2, 3, 4]);
    let mut d = [1, 2, 3];
    swap16(&mut d);
    assert_eq!(d, [2, 1, 3]);
    assert_eq!(be32(&[0, 0, 1], 0), 0);
    assert_eq!(slice(&[1, 2], 1, 2), Err(TextureError::OutOfRange));
    assert_eq!(array::<2>(&[1, 2], 1), Err(TextureError::OutOfRange));
    // 6-value mode (a0 <= a1): indices 6 and 7 are 0 and 255
    let px = dxt5a_block([10, 20, 0b1111_1110, 0xFF, 0, 0, 0, 0]);
    assert_eq!(&px[..4], &[0, 255, 255, 255]);
}

#[test]
fn guards() {
    // 2 or 4 bytes per block reach the block copier only through a bug: it refuses them
    let t = Texture {
        b: &[],
        ver: 0,
        typ: 0,
        fmt: 5,
        flags: 0,
        w: 4,
        h: 4,
        depth: 0,
        mips: 1,
    };
    assert_eq!(t.blocks(), Err(TextureError::OutOfRange));
    let t = Texture {
        b: &[],
        ver: 0,
        typ: 0,
        fmt: 77,
        flags: 0,
        w: 4,
        h: 4,
        depth: 0,
        mips: 1,
    };
    assert_eq!(t.layout(77, 1), Err(TextureError::OutOfRange));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]
    /// Any bytes under a plausible header: a texture or an error, never a panic.
    #[test]
    fn any_bytes(mut b in proptest::collection::vec(any::<u8>(), 0..400),
                 fmt in prop::sample::select(vec![0u32, 2, 3, 5, 9, 10, 13, 19]),
                 w in 1u32..40, h in 1u32..40, mips in 1u32..4) {
        if b.len() >= 32 {
            for (i, v) in [3, 0, fmt, 1, w, h, 0, mips].iter().enumerate() {
                b[4 * i..4 * i + 4].copy_from_slice(&v.to_be_bytes());
            }
        }
        if let Ok(pc) = convert(&b) {
            prop_assert!(pc.len() >= HEADER);
        }
    }
}
