//! The WW2 levels' loading and map pictures (Project OPFOR's `ww2_loading.py`, round OR4): BF1943's
//! `load_image_*` (an Xbox BGRA8 texture) scaled to 720 rows with Pillow's Lanczos filter and cut to DICE's
//! 1280 x 720 at the centre, the match selector's picture cut from it (295 x 196, opaque, in the top-left of a
//! 512 x 256 picture white at alpha 0 elsewhere), and the written resources read back against DICE's form.

use super::resample::{Filter, crop, resize};
use super::{Pic, psnr};
use crate::pyfmt::fixed;
use crate::{texcheck, texture, texwrite};

/// DICE's loading pictures: 1280 x 720.
pub const W: usize = 1280;
/// See [`W`].
pub const H: usize = 720;
/// DICE's wrapped size of a loading picture (DXT1, one level).
pub const SIZE: usize = 460_956;
/// The match selector's picture: 512 x 256 ...
pub const LW: usize = 512;
/// See [`LW`].
pub const LH: usize = 256;
/// ... the map in its top-left 295 x 196.
pub const PW: usize = 295;
/// See [`PW`].
pub const PH: usize = 196;
/// DICE's wrapped size of a map picture (DXT5, one level).
pub const LEVEL_SIZE: usize = 131_228;

const HEADER: usize = texture::HEADER;
const BGRA8: u32 = 9;

fn u32_at(b: &[u8], i: usize) -> u32 {
    b.get(4 * i..4 * i + 4)
        .and_then(|s| s.try_into().ok())
        .map_or(0, u32::from_le_bytes)
}

/// Python's `round` of a float (half to even) as a whole number.
fn py_round(v: f64) -> usize {
    v.round_ties_even() as usize
}

/// `picture(src, level)`: BF1943's loading picture (the Xbox resource's bytes) as `(720, 1280, 3)` at 16:9.
///
/// # Errors
/// (Python's `SystemExit`) the converted picture is not BGRA8, or narrower than 16:9; the texture does not convert.
pub fn picture(xbox: &[u8], name: &str) -> Result<Pic, String> {
    let pc = texture::convert(xbox).map_err(|e| format!("{name}: {e}"))?;
    let (fmt, w, h) = (u32_at(&pc, 2), u32_at(&pc, 4) as usize, u32_at(&pc, 5) as usize);
    if fmt != BGRA8 {
        return Err(format!("{name}: PC format {fmt}, expected 9 (B8G8R8A8)"));
    }
    let bgra = pc
        .get(HEADER..HEADER + w * h * 4)
        .ok_or_else(|| format!("{name}: {w} x {h} cut short"))?;
    let rgb: Vec<u8> = bgra.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0]]).collect();
    let img = Pic::of(w, h, 3, rgb).ok_or_else(|| format!("{name}: {w} x {h} cut short"))?;
    let sw = py_round(w as f64 * H as f64 / h as f64);
    if sw < W {
        return Err(format!("{name}: {w} x {h} is narrower than 16:9"));
    }
    let img = resize(&img, sw, H, Filter::Lanczos);
    let x0 = (sw - W) / 2;
    Ok(crop(&img, x0, 0, x0 + W, H))
}

/// `level_picture(rgb)`: the loading picture as DICE's match-selector picture - `(256, 512, 4)`, the picture cut at
/// the centre to 295:196 and scaled (Lanczos) into the top-left 295 x 196 (opaque), the rest white at alpha 0.
#[must_use]
pub fn level_picture(rgb: &Pic) -> Pic {
    let cw = py_round(H as f64 * PW as f64 / PH as f64);
    let x0 = (W - cw) / 2;
    let small = resize(&crop(rgb, x0, 0, x0 + cw, H), PW, PH, Filter::Lanczos);
    let mut out = Pic::new(LW, LH, 4);
    for (i, p) in out.px.chunks_exact_mut(4).enumerate() {
        let (y, x) = (i / LW, i % LW);
        if y < PH && x < PW {
            let s = (y * PW + x) * 3;
            p.copy_from_slice(&[small.px[s], small.px[s + 1], small.px[s + 2], 255]);
        } else {
            p.copy_from_slice(&[255, 255, 255, 0]);
        }
    }
    out
}

fn header(res: &[u8]) -> (u32, u32, u32, u32, u32, u32, u32) {
    (
        u32_at(res, 0),
        u32_at(res, 1),
        u32_at(res, 2),
        u32_at(res, 3),
        u32_at(res, 4),
        u32_at(res, 5),
        u32_at(res, 7),
    )
}

fn hdr_text(h: (u32, u32, u32, u32, u32, u32, u32)) -> String {
    format!("({}, {}, {}, {}, {}, {}, {})", h.0, h.1, h.2, h.3, h.4, h.5, h.6)
}

fn decode_rgb(res: &[u8], w: usize, h: usize, fmt: u32) -> Result<Vec<[i32; 3]>, String> {
    texcheck::decode(res.get(HEADER..).unwrap_or_default(), w as i64, h as i64, fmt)
        .map(|(_, _, px)| px)
        .ok_or_else(|| format!("decode: a {w} x {h} level of format {fmt} the decoder does not read"))
}

/// `check(res, rgb)`: the written loading picture read back - DICE's header (version 3, DXT1, flags 1, 1280 x 720,
/// one level) and wrapped size, and at least 30 dB against the picture it was made from. Returns (the wrapped
/// resource, the PSNR).
///
/// # Errors
/// (Python's `SystemExit`) any of those not so.
pub fn check(res: &[u8], rgb: &Pic) -> Result<(Vec<u8>, f64), String> {
    let hd = header(res);
    if hd != (3, 0, 0, 1, W as u32, H as u32, 1) {
        return Err(format!("loading picture header {}, not DICE's", hdr_text(hd)));
    }
    let wrapped = texwrite::wrapped(res, b"ITexture").ok_or("a resource of 4 GB or more cannot be wrapped")?;
    if wrapped.len() != SIZE {
        return Err(format!(
            "loading picture {} bytes wrapped, DICE's are {SIZE}",
            wrapped.len()
        ));
    }
    let back = decode_rgb(res, W, H, 0)?;
    let a: Vec<f64> = back.iter().flat_map(|p| p.map(f64::from)).collect();
    let b: Vec<f64> = rgb.px.iter().map(|&v| f64::from(v)).collect();
    let p = psnr(&a, &b);
    if p < 30.0 {
        return Err(format!(
            "loading picture: {} dB against its source after DXT1, expected 30+",
            fixed(p, 1)
        ));
    }
    Ok((wrapped, p))
}

/// `level_check(res, rgba)`: the written map picture read back - DICE's header (DXT5, flags 1, 512 x 256, one level)
/// and wrapped size, its visible part at least 28 dB against the source, the alpha opaque over the picture (250 or
/// more) and clear outside it (from 4 texels past it: the DXT5 blocks). Returns (the wrapped resource, the PSNR).
///
/// # Errors
/// (Python's `SystemExit`) any of those not so.
pub fn level_check(res: &[u8], rgba: &Pic) -> Result<(Vec<u8>, f64), String> {
    let hd = header(res);
    if hd != (3, 0, 2, 1, LW as u32, LH as u32, 1) {
        return Err(format!("map picture header {}, not DICE's", hdr_text(hd)));
    }
    let wrapped = texwrite::wrapped(res, b"ITexture").ok_or("a resource of 4 GB or more cannot be wrapped")?;
    if wrapped.len() != LEVEL_SIZE {
        return Err(format!(
            "map picture {} bytes wrapped, DICE's are {LEVEL_SIZE}",
            wrapped.len()
        ));
    }
    let back = decode_rgb(res, LW, LH, 2)?;
    let mut a = Vec::with_capacity(PW * PH * 3);
    let mut b = Vec::with_capacity(PW * PH * 3);
    for y in 0..PH {
        for x in 0..PW {
            a.extend(back[y * LW + x].map(f64::from));
            let s = (y * LW + x) * 4;
            b.extend(rgba.px[s..s + 3].iter().map(|&v| f64::from(v)));
        }
    }
    let p = psnr(&a, &b);
    let alpha: Vec<i32> = decode_rgb(res, LW, LH, 19)?.iter().map(|v| v[0]).collect();
    let inside = (0..PH)
        .flat_map(|y| (0..PW).map(move |x| (y, x)))
        .map(|(y, x)| alpha[y * LW + x])
        .min()
        .unwrap_or(0);
    let below = alpha[(PH + 4) * LW..].iter().copied().max().unwrap_or(0);
    let right = (0..LH)
        .flat_map(|y| (PW + 4..LW).map(move |x| (y, x)))
        .map(|(y, x)| alpha[y * LW + x])
        .max()
        .unwrap_or(0);
    if p < 28.0 || inside < 250 || below > 0 || right > 0 {
        return Err(format!(
            "map picture: {} dB, alpha {inside} inside, {} outside",
            fixed(p, 1),
            below.max(right)
        ));
    }
    Ok((wrapped, p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_map_picture_has_dices_shape() {
        let mut rgb = Pic::new(W, H, 3);
        for (i, v) in rgb.px.iter_mut().enumerate() {
            *v = (i % 251) as u8;
        }
        let m = level_picture(&rgb);
        assert_eq!((m.w, m.h, m.ch), (LW, LH, 4));
        assert_eq!(m.px[3], 255);
        assert_eq!(m.px[(PH * LW) * 4..(PH * LW) * 4 + 4], [255, 255, 255, 0]);
        assert_eq!(m.px[PW * 4..PW * 4 + 4], [255, 255, 255, 0]);
    }

    #[test]
    fn a_wrong_header_is_refused() {
        let res = vec![0u8; 200];
        let rgb = Pic::new(W, H, 3);
        assert!(check(&res, &rgb).is_err_and(|e| e.starts_with("loading picture header (0, 0, 0, 0, 0, 0, 0)")));
    }
}
