//! `audio`'s unit and property tests. The search is checked against a plain transliteration of the reference's
//! (every candidate tried to the end, no shortcuts); the bytes against `audio.py` itself in
//! `bfbc2-console-convert/tests/test_audio_native.py`.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::float_cmp,
    clippy::many_single_char_names, // the reference's names
    clippy::cast_precision_loss     // NumPy's float steps, of integers below 2^53
)]

use super::*;
use proptest::prelude::*;

/// `xas_encode_frames`' search for one group, line by line (float steps as NumPy takes them).
fn reference_group(g: &[i64; 32]) -> (u16, u16, [i64; 30]) {
    let q0 = (((g[0] + 8) >> 4) << 4).clamp(-32768, 32752);
    let q1 = (((g[1] + 8) >> 4) << 4).clamp(-32768, 32752);
    let mut best_err = f64::INFINITY;
    let mut best = (0i64, 0i64, [0i64; 30]);
    for c in 0..4usize {
        let (c0, c1) = (COEFS[c], COEFS[c + 4]);
        let mut r = 0i64;
        for t in 0..30 {
            let p1 = if t == 0 { q1 } else { g[t + 1] };
            let p2 = match t {
                0 => q0,
                1 => q1,
                _ => g[t],
            };
            r = r.max((g[t + 2] * 256 - (p1 * c0 + p2 * c1)).abs());
        }
        let need = ((r.max(1) as f64) / 7.0).log2().ceil() as i64;
        for d in [-1i64, 0, 1] {
            let k = (20 - need - d).clamp(0, 15);
            let step = 1i64 << (20 - k);
            let (mut p2, mut p1) = (q0, q1);
            let mut err = 0f64;
            let mut nib = [0i64; 30];
            for t in 0..30 {
                let pred = p1 * c0 + p2 * c1;
                let v = (((g[t + 2] * 256 - pred) as f64) / step as f64)
                    .round_ties_even()
                    .clamp(-8.0, 7.0) as i64;
                let s = ((v * step + pred + 128) >> 8).clamp(-32768, 32767);
                err += ((s - g[t + 2]) * (s - g[t + 2])) as f64;
                nib[t] = v;
                p2 = p1;
                p1 = s;
            }
            if err < best_err {
                best_err = err;
                best = (c as i64, k, nib);
            }
        }
    }
    let (c, k, nib) = best;
    (
        (((q0 & 0xFFF0) | c) & 0xFFFF) as u16,
        (((q1 & 0xFFF0) | k) & 0xFFFF) as u16,
        nib,
    )
}

/// `xas_decode_frames` for one frame, line by line.
fn reference_decode(f: &[u8]) -> Vec<i16> {
    let mut out = vec![0i16; 128];
    for gi in 0..4 {
        let h = u32::from_le_bytes(f[gi * 4..gi * 4 + 4].try_into().unwrap());
        let (lo, hi) = (i64::from(h & 0xFFFF), i64::from(h >> 16));
        let s0 = ((lo & 0xFFF0) ^ 0x8000) - 0x8000;
        let s1 = ((hi & 0xFFF0) ^ 0x8000) - 0x8000;
        let (c0, c1) = (COEFS[(lo & 15) as usize], COEFS[(lo & 15) as usize + 4]);
        let step = 1i64 << (20 - (hi & 15));
        out[gi * 32] = s0 as i16;
        out[gi * 32 + 1] = s1 as i16;
        let (mut p2, mut p1) = (s0, s1);
        for t in 0..30 {
            let b = i64::from(f[16 + (t / 2) * 4 + gi]);
            let n = if t % 2 == 0 { b >> 4 } else { b & 15 };
            let n = (n ^ 8) - 8;
            let s = ((n * step + p1 * c0 + p2 * c1 + 128) >> 8).clamp(-32768, 32767);
            out[gi * 32 + t + 2] = s as i16;
            p2 = p1;
            p1 = s;
        }
    }
    out
}

fn group_of(v: &[i16]) -> [i64; 32] {
    std::array::from_fn(|j| i64::from(v[j]))
}

#[test]
fn need_is_the_float_formula() {
    for r in (0..1 << 16)
        .chain((1 << 16..1 << 26).step_by(97))
        .chain((0..26).flat_map(|m| {
            let p = 7i64 << m;
            [p - 1, p, p + 1]
        }))
    {
        assert_eq!(need(r), ((r.max(1) as f64) / 7.0).log2().ceil() as i64, "r {r}");
    }
}

#[test]
fn rint_is_half_to_even() {
    for s in 1..21u32 {
        for a in -5000i64..5000 {
            let want = ((a as f64) / f64::from(1u32 << s)).round_ties_even() as i64;
            assert_eq!(rint_shift(a, s), want, "{a} >> {s}");
        }
    }
}

#[test]
fn groups_as_the_reference_on_extremes() {
    let cases: Vec<[i64; 32]> = vec![
        [0; 32],
        [32767; 32],
        [-32768; 32],
        std::array::from_fn(|j| if j % 2 == 0 { 32767 } else { -32768 }),
        std::array::from_fn(|j| (j as i64 * 2000) - 32000),
        std::array::from_fn(|j| if j < 2 { 32767 } else { -32768 }),
        std::array::from_fn(|j| if j < 2 { -32768 } else { 32767 }),
        std::array::from_fn(|j| [1, -1, 0, 7, -8][j % 5]),
    ];
    for g in &cases {
        assert_eq!(encode_group(g), reference_group(g), "{g:?}");
    }
}

#[test]
fn layout_of_frames() {
    // 3 channels, 300 samples: 3 frames of each channel, frame-major
    let pcm: Vec<i16> = (0..900).map(|i| ((i * 37) % 2001 - 1000) as i16).collect();
    let out = encode(&pcm, 3).unwrap();
    assert_eq!(out.len(), 3 * 3 * FRAME);
    for f in 0..3 {
        for c in 0..3 {
            let u = &out[(f * 3 + c) * FRAME..][..FRAME];
            for gi in 0..4 {
                let src: Vec<i16> = (0..32)
                    .map(|j| {
                        let i = f * 128 + gi * 32 + j;
                        if i < 300 { pcm[i * 3 + c] } else { 0 }
                    })
                    .collect();
                let (lo, hi, nib) = reference_group(&group_of(&src));
                assert_eq!(
                    u32::from_le_bytes(u[gi * 4..gi * 4 + 4].try_into().unwrap()),
                    u32::from(lo) | u32::from(hi) << 16
                );
                for r in 0..15 {
                    assert_eq!(
                        i64::from(u[16 + r * 4 + gi]),
                        (nib[2 * r] & 15) << 4 | (nib[2 * r + 1] & 15)
                    );
                }
            }
        }
    }
}

#[test]
fn empty_and_errors() {
    assert_eq!(encode(&[], 2).unwrap(), Vec::<u8>::new());
    assert_eq!(encode(&[1, 2, 3], 0).unwrap(), Vec::<u8>::new());
    assert_eq!(
        encode(&[1, 2, 3], 2),
        Err(AudioError::NotWhole {
            samples: 3,
            channels: 2
        })
    );
    assert_eq!(encode(&[5], 1).unwrap().len(), FRAME);
    assert_eq!(decode_frames(&[]).unwrap(), Vec::<i16>::new());
    assert_eq!(decode_frames(&[0; 75]), Err(AudioError::NotFrames(75)));
    assert_eq!(
        AudioError::NotFrames(75).to_string(),
        "EA-XAS: 75 bytes, not whole 76-byte frames"
    );
}

#[test]
fn batch_as_one_by_one() {
    let a: Vec<i16> = (0..1000).map(|i| (i * 13 % 600) as i16).collect();
    let b: Vec<i16> = (0..777 * 2).map(|i| ((i * 7919) % 65536 - 32768) as i16).collect();
    let items: Vec<(&[i16], usize)> = vec![(&a, 1), (&b, 2), (&b, 3), (&[], 1)];
    let all = encode_all(&items);
    for ((pcm, ch), got) in items.iter().zip(all) {
        assert_eq!(got, encode(pcm, *ch));
    }
}

#[test]
fn decode_inverts_closely() {
    let pcm: Vec<i16> = (0..4096)
        .map(|i| ((f64::from(i) * 0.05).sin() * 12000.0) as i16)
        .collect();
    let back = decode_frames(&encode(&pcm, 1).unwrap()).unwrap();
    let err: f64 = pcm.iter().zip(&back).map(|(a, b)| f64::from(a - b).powi(2)).sum();
    let sig: f64 = pcm.iter().map(|a| f64::from(*a).powi(2)).sum();
    assert!(10.0 * (sig / err).log10() > 40.0);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(3000))]

    #[test]
    fn groups_as_the_reference(v in prop::collection::vec(any::<i16>(), 32), scale in 0u32..16) {
        // the full range and quieter signals (smaller shifts)
        let g: [i64; 32] = std::array::from_fn(|j| i64::from(v[j]) >> scale);
        prop_assert_eq!(encode_group(&g), reference_group(&g));
    }

    #[test]
    fn smooth_groups_as_the_reference(start in any::<i16>(), steps in prop::collection::vec(-300i64..300, 32)) {
        let mut x = i64::from(start);
        let g: [i64; 32] = std::array::from_fn(|j| { x = (x + steps[j]).clamp(-32768, 32767); x });
        prop_assert_eq!(encode_group(&g), reference_group(&g));
    }

    #[test]
    fn tied_groups_as_the_reference(base in any::<i16>(), v in prop::collection::vec(-2i64..3, 32), mul in 0i64..64) {
        // near-constant groups: many candidates with the same error, the reference's first must win
        let g: [i64; 32] = std::array::from_fn(|j| (i64::from(base) + v[j] * mul).clamp(-32768, 32767));
        prop_assert_eq!(encode_group(&g), reference_group(&g));
    }

    #[test]
    fn frames_decode_as_the_reference(f in prop::collection::vec(any::<u8>(), 76)) {
        prop_assert_eq!(decode_frames(&f).unwrap(), reference_decode(&f));
    }
}
