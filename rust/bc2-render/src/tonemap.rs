//! BFBC2's post-process as the render harness applies it (`renderqa.tonemap`): exposure = middle grey / the drawn
//! pixels' log-average luminance, clamped to the state's range; the filmic curve (its gamma included); saturation,
//! contrast, brightness. No bloom, lens flare or film grain.
//!
//! **An approximation:** the game runs DICE's own post-process shaders (`Systems/PostProcess/ShaderContainer/
//! psFilmicTonemap*_d3d11` and `psExposure_d3d11` in `async/startup-00.fbrb`); running those unchanged is phase 2 of
//! `EDITOR_ARCHITECTURE.md` 3.11.

use rayon::prelude::*;

use crate::weather::Post;

const LUM: [f32; 3] = [0.3, 0.59, 0.11];

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The exposure of an HDR picture (RGBA `f32`, `mask` true where something was drawn).
#[must_use]
pub fn exposure(hdr: &[f32], mask: &[bool], post: &Post) -> f32 {
    let (sum, n) = hdr
        .par_chunks_exact(4)
        .zip(mask.par_iter())
        .filter_map(|(px, &m)| match *px {
            [r, g, b, _] if m && [r, g, b].iter().all(|v| v.is_finite()) => {
                Some(f64::from(dot([r, g, b], LUM).max(1e-5).ln()))
            }
            _ => None,
        })
        .fold(|| (0.0f64, 0usize), |(s, n), v| (s + v, n + 1))
        .reduce(|| (0.0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
    let avg = if n == 0 { 1.0 } else { (sum / n as f64).exp() as f32 };
    (post.middle_gray / (avg + 0.001)).clamp(post.min_exposure, post.max_exposure.max(post.min_exposure))
}

/// One HDR colour to the screen (0..1).
#[must_use]
pub fn map(colour: [f32; 3], exposure: f32, post: &Post) -> [f32; 3] {
    let c = colour;
    if !c.iter().all(|v| v.is_finite()) {
        return [1.0; 3]; // NaN / inf: the game shows these white
    }
    let curve = |v: f32| {
        let x = (v * exposure - 0.004).max(0.0);
        (x * (6.2 * x + 0.5) / (x * (6.2 * x + 1.7) + 0.06)).min(1.0)
    };
    let y = c.map(curve);
    let grey = dot(y, LUM);
    let mut out = [0.0; 3];
    for (k, o) in out.iter_mut().enumerate() {
        let (v, s, ct, b) = (
            y.get(k).copied().unwrap_or(0.0),
            post.saturation.get(k).copied().unwrap_or(1.0),
            post.contrast.get(k).copied().unwrap_or(1.0),
            post.brightness.get(k).copied().unwrap_or(1.0),
        );
        let v = grey + (v - grey) * s;
        *o = (((v - 0.5) * ct + 0.5) * b).clamp(0.0, 1.0);
    }
    out
}

/// The picture as RGBA8 (`background` where nothing was drawn); the exposure used.
#[must_use]
pub fn tonemap(hdr: &[f32], mask: &[bool], post: &Post, background: [u8; 3]) -> (Vec<u8>, f32) {
    let e = exposure(hdr, mask, post);
    let mut out = vec![255u8; mask.len() * 4];
    out.par_chunks_exact_mut(4)
        .zip(hdr.par_chunks_exact(4))
        .zip(mask.par_iter())
        .for_each(|((o, px), &m)| {
            let c = match *px {
                [r, g, b, _] if m => map([r, g, b], e, post).map(|v| (v * 255.0).round() as u8),
                _ => background,
            };
            for (d, s) in o.iter_mut().zip(c) {
                *d = s;
            }
        });
    (out, e)
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::chunks_exact_to_as_chunks,
    clippy::trivially_copy_pass_by_ref,
    clippy::float_cmp,
    clippy::unwrap_used,
    clippy::expect_used
)]
mod tests {
    use super::*;

    #[test]
    fn the_curve_is_the_harness_one() {
        let p = Post::default();
        assert_eq!(map([0.0; 3], 1.0, &p), [0.0; 3]);
        // x = 1 - 0.004: x (6.2x + 0.5) / (x (6.2x + 1.7) + 0.06)
        let x = 0.996f32;
        let want = x * (6.2 * x + 0.5) / (x * (6.2 * x + 1.7) + 0.06);
        let got = map([1.0; 3], 1.0, &p);
        assert!((got[0] - want).abs() < 1e-6);
        assert_eq!(map([f32::NAN, 0.0, 0.0], 1.0, &p), [1.0; 3]);
    }

    #[test]
    fn exposure_clamps_to_the_state() {
        let p = Post {
            middle_gray: 0.31,
            min_exposure: 0.5,
            max_exposure: 12.0,
            ..Post::default()
        };
        let hdr = [0.31f32, 0.31, 0.31, 1.0];
        let e = exposure(&hdr, &[true], &p);
        assert!((e - 0.31 / (0.31 + 0.001)).abs() < 1e-4);
        assert!((exposure(&[100.0, 100.0, 100.0, 1.0], &[true], &p) - 0.5).abs() < 1e-6);
    }
}
