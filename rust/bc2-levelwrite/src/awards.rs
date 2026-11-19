//! The new level's pictures (round E3b, `EDITOR_ARCHITECTURE.md` 3.12): its loading screen and its map-list picture,
//! made from a picture the user gives (PNG or JPEG) with the suite's texture writer (`bc2_convert::texwrite`, round
//! F2a), in the form the game's own pictures have.
//!
//! Where the game finds them (OPFOR's `ww2_loading.py`, `research\WW2_LOADING_SCREENS_VO_RESEARCH_2026-11-14.md`):
//! - the loading screen cuts the description's `Name` loc key at `_NAME` (`MP_009_NAME` -> `mp_009`) and shows
//!   `Awards/images/loadingScreens/<that>` from the on-demand awards bundles (`async\ondemand_awards-00.fbrb`,
//!   `game\Package` first);
//! - the main menu's map list shows `Awards/images/Levels/<level id>` from the same bundles.
//!
//! **The form is read, not assumed**: each picture takes the format, size and level count of the export donor's own
//! picture (on OPFOR's game copy, `mp_009`'s loading screen: DXT1, 1280 x 720, one level; `mp_009cq`'s map picture:
//! DXT5, 512 x 256, one level, the picture opaque in its top-left 295 x 196 - the box read from its alpha - and white
//! at alpha 0 elsewhere). The user's picture is scaled to cover each (Lanczos-3) and cut at the centre.
//!
//! Each entry is kept as the on-demand bundles keep theirs ([`wrap`]: `RES\x02`, the type name, the size twice).
//! The written pictures are decoded back ([`bc2_convert::texcheck::decode`]) and compared with what was encoded:
//! the peak signal-to-noise ratio is the format's loss alone.

#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::needless_range_loop,
    reason = "the 3 x 3 solve indexes its fixed arrays; pixel sizes are far below 2^52; the picture walks go by \
              row and column"
)]

use bc2_convert::{texcheck, texwrite};

/// An RGB picture, row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgb {
    /// Width.
    pub w: usize,
    /// Height.
    pub h: usize,
    /// The pixels: `r g b` per pixel.
    pub px: Vec<u8>,
}

/// The on-demand bundles' resource header: `RES\x02`, the type name zero-padded to 56 bytes, the size twice.
pub const WRAP: usize = 64;

/// A resource in the on-demand bundles' form (`texwrite.wrapped`).
#[must_use]
pub fn wrap(res: &[u8]) -> Vec<u8> {
    let mut out = b"RES\x02ITexture".to_vec();
    out.resize(56, 0);
    let n = res.len() as u32;
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(res);
    out
}

/// The `ITexture` of an on-demand entry (its resource header off), or the bytes as they are.
#[must_use]
pub fn unwrap(data: &[u8]) -> &[u8] {
    if data.starts_with(b"RES") {
        data.get(WRAP..).unwrap_or_default()
    } else {
        data
    }
}

/// A PC `ITexture`'s header: format, flags, width, height, levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TexInfo {
    /// The format (0 DXT1, 2 DXT5).
    pub fmt: u32,
    /// The flags.
    pub flags: u32,
    /// Width.
    pub w: usize,
    /// Height.
    pub h: usize,
    /// Mip levels.
    pub mips: usize,
}

/// The header of a PC `ITexture` (unwrapped), `None` when it is not one (version 3, a 2D texture).
#[must_use]
pub fn tex_info(res: &[u8]) -> Option<TexInfo> {
    let w = |k: usize| -> Option<u32> { res.get(4 * k..4 * k + 4)?.try_into().ok().map(u32::from_le_bytes) };
    (w(0)? == 3 && w(1)? == 0).then_some(())?;
    Some(TexInfo {
        fmt: w(2)?,
        flags: w(3)?,
        w: w(4)? as usize,
        h: w(5)? as usize,
        mips: w(7)? as usize,
    })
}

/// A PC `ITexture` from its levels (`texwrite.resource`): the 92-byte header (version 3, type 0, depth 0), then the
/// levels.
#[must_use]
pub fn resource(info: &TexInfo, levels: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for v in [
        3,
        0,
        info.fmt,
        info.flags,
        info.w as u32,
        info.h as u32,
        0,
        levels.len() as u32,
    ] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for k in 0..15 {
        let n = levels.get(k).map_or(0, Vec::len) as u32;
        out.extend_from_slice(&n.to_le_bytes());
    }
    for l in levels {
        out.extend_from_slice(l);
    }
    out
}

// ------------------------------------------------------------------------------------------------ reading pictures

/// A PNG or JPEG file's picture as RGB (an alpha channel is dropped, grey made RGB).
///
/// # Errors
/// Neither a PNG nor a JPEG, or one that does not decode.
pub fn read_image(bytes: &[u8]) -> Result<Rgb, String> {
    if bytes.starts_with(b"\x89PNG") {
        read_png(bytes)
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        read_jpeg(bytes)
    } else {
        Err("not a PNG or JPEG picture".into())
    }
}

fn read_png(bytes: &[u8]) -> Result<Rgb, String> {
    let mut d = png::Decoder::new(std::io::Cursor::new(bytes));
    d.set_transformations(png::Transformations::normalize_to_color8());
    let mut r = d.read_info().map_err(|e| format!("PNG: {e}"))?;
    let mut buf = vec![0; r.output_buffer_size().ok_or("PNG: too large")?];
    let info = r.next_frame(&mut buf).map_err(|e| format!("PNG: {e}"))?;
    let (w, h) = (info.width as usize, info.height as usize);
    let ch = match info.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Err("PNG: a palette picture the decoder did not expand".into()),
    };
    let line = info.line_size;
    let mut px = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        let row = buf.get(y * line..y * line + w * ch).ok_or("PNG: short")?;
        for p in row.chunks_exact(ch) {
            match ch {
                1 | 2 => px.extend([p.first().copied().unwrap_or(0); 3]),
                _ => px.extend(p.get(..3).unwrap_or(&[0, 0, 0])),
            }
        }
    }
    Ok(Rgb { w, h, px })
}

fn read_jpeg(bytes: &[u8]) -> Result<Rgb, String> {
    use zune_core::colorspace::ColorSpace;
    use zune_core::options::DecoderOptions;
    let opts = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut d = zune_jpeg::JpegDecoder::new_with_options(std::io::Cursor::new(bytes), opts);
    let px = d.decode().map_err(|e| format!("JPEG: {e:?}"))?;
    let info = d.info().ok_or("JPEG: no header")?;
    let (w, h) = (usize::from(info.width), usize::from(info.height));
    if px.len() != w * h * 3 {
        return Err(format!("JPEG: {} bytes for {w} x {h} RGB", px.len()));
    }
    Ok(Rgb { w, h, px })
}

// ------------------------------------------------------------------------------------------------ scaling

fn lanczos3(x: f64) -> f64 {
    let x = x.abs();
    if x < 1e-12 {
        1.0
    } else if x < 3.0 {
        let p = std::f64::consts::PI * x;
        3.0 * p.sin() * (p / 3.0).sin() / (p * p)
    } else {
        0.0
    }
}

/// One axis's Lanczos-3 weights: for each output sample, (first input sample, weights).
fn weights(src: usize, dst: usize) -> Vec<(usize, Vec<f64>)> {
    let scale = src as f64 / dst as f64;
    let support = 3.0 * scale.max(1.0);
    let inv = 1.0 / scale.max(1.0);
    (0..dst)
        .map(|i| {
            let centre = (i as f64 + 0.5) * scale;
            let lo = ((centre - support).floor().max(0.0)) as usize;
            let hi = ((centre + support).ceil() as usize).min(src);
            let mut w: Vec<f64> = (lo..hi).map(|j| lanczos3((j as f64 + 0.5 - centre) * inv)).collect();
            let s: f64 = w.iter().sum();
            if s.abs() > 1e-12 {
                for v in &mut w {
                    *v /= s;
                }
            }
            (lo, w)
        })
        .collect()
}

/// The picture scaled to `w` x `h` (Lanczos-3, rows then columns).
#[must_use]
pub fn resize(src: &Rgb, w: usize, h: usize) -> Rgb {
    let wx = weights(src.w, w);
    let wy = weights(src.h, h);
    // columns: src.h rows of w
    let mut mid = vec![0f64; src.h * w * 3];
    for y in 0..src.h {
        for (x, (lo, ws)) in wx.iter().enumerate() {
            for c in 0..3 {
                let mut acc = 0.0;
                for (k, wt) in ws.iter().enumerate() {
                    acc += wt * f64::from(src.px.get(((y * src.w) + lo + k) * 3 + c).copied().unwrap_or(0));
                }
                if let Some(m) = mid.get_mut((y * w + x) * 3 + c) {
                    *m = acc;
                }
            }
        }
    }
    let mut px = vec![0u8; w * h * 3];
    for (y, (lo, ws)) in wy.iter().enumerate() {
        for x in 0..w {
            for c in 0..3 {
                let mut acc = 0.0;
                for (k, wt) in ws.iter().enumerate() {
                    acc += wt * mid.get(((lo + k) * w + x) * 3 + c).copied().unwrap_or(0.0);
                }
                if let Some(p) = px.get_mut((y * w + x) * 3 + c) {
                    *p = acc.round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
    Rgb { w, h, px }
}

/// The picture scaled to cover `w` x `h` and cut at the centre (the WW2 install's 16:9 cut).
#[must_use]
pub fn cover(src: &Rgb, w: usize, h: usize) -> Rgb {
    if src.w == 0 || src.h == 0 {
        return Rgb {
            w,
            h,
            px: vec![0; w * h * 3],
        };
    }
    let scale = (w as f64 / src.w as f64).max(h as f64 / src.h as f64);
    let sw = ((src.w as f64 * scale).round() as usize).max(w);
    let sh = ((src.h as f64 * scale).round() as usize).max(h);
    let big = resize(src, sw, sh);
    let (x0, y0) = ((sw - w) / 2, (sh - h) / 2);
    let mut px = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        px.extend_from_slice(
            big.px
                .get(((y0 + y) * sw + x0) * 3..((y0 + y) * sw + x0 + w) * 3)
                .unwrap_or_default(),
        );
    }
    Rgb { w, h, px }
}

// ------------------------------------------------------------------------------------------------ encoding

/// The eigenvector of a symmetric 3 x 3 matrix's largest eigenvalue (cyclic Jacobi; the last of equal ones, as
/// `numpy.linalg.eigh` orders them), what [`texwrite::Prepared::finish`] takes as a block's axis.
#[must_use]
pub fn principal_axis(m: &[f64; 9]) -> [f64; 3] {
    let mut a = [[m[0], m[1], m[2]], [m[3], m[4], m[5]], [m[6], m[7], m[8]]];
    let mut v = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..50 {
        let off = a[0][1].abs() + a[0][2].abs() + a[1][2].abs();
        if off < 1e-12 {
            break;
        }
        for (p, q) in [(0, 1), (0, 2), (1, 2)] {
            if a[p][q].abs() < 1e-300 {
                continue;
            }
            let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
            let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
            let t = if theta == 0.0 { 1.0 } else { t };
            let c = 1.0 / (t * t + 1.0).sqrt();
            let s = t * c;
            for k in 0..3 {
                let (akp, akq) = (a[k][p], a[k][q]);
                a[k][p] = c * akp - s * akq;
                a[k][q] = s * akp + c * akq;
            }
            for k in 0..3 {
                let (apk, aqk) = (a[p][k], a[q][k]);
                a[p][k] = c * apk - s * aqk;
                a[q][k] = s * apk + c * aqk;
            }
            for row in &mut v {
                let (vp, vq) = (row[p], row[q]);
                row[p] = c * vp - s * vq;
                row[q] = s * vp + c * vq;
            }
        }
    }
    let mut best = 0;
    for k in 1..3 {
        if a[k][k] >= a[best][best] {
            best = k;
        }
    }
    [v[0][best], v[1][best], v[2][best]]
}

/// One level encoded (`texwrite.dxt1` / `dxt5`): 3 or 4 channels for DXT1 (format 0), 4 for DXT5 (format 2).
///
/// # Errors
/// Another format, or channels the format does not take.
pub fn encode_level(img: &texwrite::Image, fmt: u32) -> Result<Vec<u8>, String> {
    let enc = match fmt {
        0 => texwrite::Encoding::Dxt1,
        2 => texwrite::Encoding::Dxt5,
        f => {
            return Err(format!(
                "texture format {f} is not one the export writes (DXT1 0, DXT5 2)"
            ));
        }
    };
    let p = texwrite::Prepared::new(img, enc).ok_or("the picture's channels do not suit the format")?;
    let axes: Vec<[f64; 3]> = p.covariances().iter().map(principal_axis).collect();
    Ok(p.finish(&axes))
}

/// A picture (3 or 4 channels) as a PC `ITexture` of `info`'s format, size and level count (the levels below the
/// first: `texwrite.levels`' 2 x 2 box filter).
///
/// # Errors
/// The picture is not `info`'s size, or the format is not written.
pub fn encode(img: &texwrite::Image, info: &TexInfo) -> Result<Vec<u8>, String> {
    if (img.w, img.h) != (info.w, info.h) {
        return Err(format!(
            "a {} x {} picture for a {} x {} texture",
            img.w, img.h, info.w, info.h
        ));
    }
    // one level (the loading screens: 1280 x 720 does not halve to 1 x 1) is the picture; more, texwrite's chain
    let chain = if info.mips <= 1 {
        vec![img.clone()]
    } else {
        texwrite::levels(img, false, img.ch).ok_or("the picture's sides do not halve")?
    };
    let levels: Vec<Vec<u8>> = chain
        .iter()
        .take(info.mips.max(1))
        .map(|l| encode_level(l, info.fmt))
        .collect::<Result<_, _>>()?;
    Ok(resource(info, &levels))
}

/// The first level of a PC `ITexture` (unwrapped) decoded: its colour, and its alpha (DXT5's; DXT1's 1-bit; 255
/// for formats without).
#[must_use]
pub fn decode(res: &[u8]) -> Option<(Rgb, Vec<u8>)> {
    let info = tex_info(res)?;
    let lv = texcheck::levels(res)?;
    let first = lv.iter().find(|l| l.0 == 0 && l.1 == 0)?;
    let bytes = res.get(first.4.clone())?;
    let (h, w, px) = texcheck::decode(bytes, i64::from(first.2), i64::from(first.3), info.fmt)?;
    let rgb = Rgb {
        w,
        h,
        px: px.iter().flat_map(|c| c.map(|v| v.clamp(0, 255) as u8)).collect(),
    };
    let alpha: Vec<u8> = match info.fmt {
        2 => texwrite::alpha5(bytes, w, h)?
            .iter()
            .map(|&a| a.clamp(0, 255) as u8)
            .collect(),
        0 => texwrite::alpha1(bytes, w, h)?
            .iter()
            .map(|&a| a.clamp(0, 255) as u8)
            .collect(),
        _ => vec![255; w * h],
    };
    Some((rgb, alpha))
}

/// The peak signal-to-noise ratio (dB) of `b` against `a` over a box (x0, y0, x1, y1; same-size pictures).
#[must_use]
pub fn psnr(a: &Rgb, b: &Rgb, area: [usize; 4]) -> f64 {
    let [x0, y0, x1, y1] = area;
    let (mut se, mut n) = (0.0, 0usize);
    for y in y0..y1.min(a.h).min(b.h) {
        for x in x0..x1.min(a.w).min(b.w) {
            for c in 0..3 {
                let p = f64::from(a.px.get((y * a.w + x) * 3 + c).copied().unwrap_or(0));
                let q = f64::from(b.px.get((y * b.w + x) * 3 + c).copied().unwrap_or(0));
                se += (p - q) * (p - q);
                n += 1;
            }
        }
    }
    let mse = se / n.max(1) as f64;
    10.0 * (255.0 * 255.0 / mse.max(1e-9)).log10()
}

/// The box (x0, y0, x1, y1) where a picture's alpha is at least half: where the picture is drawn (the whole picture
/// when nothing is clear).
#[must_use]
pub fn opaque_box(alpha: &[u8], w: usize, h: usize) -> [usize; 4] {
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if alpha.get(y * w + x).is_some_and(|&a| a >= 128) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    if x1 <= x0 || y1 <= y0 {
        [0, 0, w, h]
    } else {
        [x0, y0, x1, y1]
    }
}

/// One picture made: the entry's bytes (wrapped), its form, the box it fills, its loss.
#[derive(Debug, Clone)]
pub struct Made {
    /// The entry as the on-demand bundle keeps it.
    pub wrapped: Vec<u8>,
    /// Its form (the template's).
    pub info: TexInfo,
    /// Where the picture is (x0, y0, x1, y1); the rest white at alpha 0.
    pub area: [usize; 4],
    /// What was encoded (RGB, the texture's size).
    pub source: Rgb,
    /// The texture decoded back (what the game shows).
    pub decoded: Rgb,
    /// The decoded picture against the encoded one over `area`, dB.
    pub psnr: f64,
}

/// A picture in a template's form (`template`: a stock picture's entry, wrapped or not): scaled to cover the
/// template's opaque box, the rest white at alpha 0 when the template has alpha.
///
/// # Errors
/// The template is not a texture the export writes, or the encoding fails.
pub fn make(src: &Rgb, template: &[u8]) -> Result<Made, String> {
    let t = unwrap(template);
    let info = tex_info(t).ok_or("the template is not a PC texture")?;
    let (_, alpha) = decode(t).ok_or("the template does not decode")?;
    let area = if info.fmt == 2 {
        opaque_box(&alpha, info.w, info.h)
    } else {
        [0, 0, info.w, info.h]
    };
    let [x0, y0, x1, y1] = area;
    let pic = cover(src, x1 - x0, y1 - y0);
    let full = area == [0, 0, info.w, info.h];
    let ch = if info.fmt == 2 { 4 } else { 3 };
    let mut px = Vec::with_capacity(info.w * info.h * ch);
    let mut rgb = Vec::with_capacity(info.w * info.h * 3);
    for y in 0..info.h {
        for x in 0..info.w {
            let inside = x >= x0 && x < x1 && y >= y0 && y < y1;
            let c: [u8; 3] = if inside {
                let k = ((y - y0) * pic.w + (x - x0)) * 3;
                [
                    pic.px.get(k).copied().unwrap_or(0),
                    pic.px.get(k + 1).copied().unwrap_or(0),
                    pic.px.get(k + 2).copied().unwrap_or(0),
                ]
            } else {
                [255; 3]
            };
            px.extend_from_slice(&c);
            rgb.extend_from_slice(&c);
            if ch == 4 {
                px.push(if inside || full { 255 } else { 0 });
            }
        }
    }
    let img = texwrite::Image {
        h: info.h,
        w: info.w,
        ch,
        px,
    };
    let res = encode(&img, &info)?;
    let (decoded, _) = decode(&res).ok_or("the written texture does not decode")?;
    let source = Rgb {
        w: info.w,
        h: info.h,
        px: rgb,
    };
    let psnr = psnr(&source, &decoded, area);
    Ok(Made {
        wrapped: wrap(&res),
        info,
        area,
        source,
        decoded,
        psnr,
    })
}

/// A picture as a PNG file's bytes (previews).
///
/// # Errors
/// The encoder fails.
pub fn png_bytes(p: &Rgb) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, p.w as u32, p.h as u32);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        let mut w = e.write_header().map_err(|e| e.to_string())?;
        w.write_image_data(&p.px).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn gradient(w: usize, h: usize) -> Rgb {
        let mut px = Vec::new();
        for y in 0..h {
            for x in 0..w {
                px.extend([(x * 255 / w.max(1)) as u8, (y * 255 / h.max(1)) as u8, 128]);
            }
        }
        Rgb { w, h, px }
    }

    #[test]
    fn axes_and_wrapping() {
        // a diagonal matrix: the largest value's axis
        assert_eq!(
            principal_axis(&[1.0, 0.0, 0.0, 0.0, 5.0, 0.0, 0.0, 0.0, 2.0]),
            [0.0, 1.0, 0.0]
        );
        // equal values: the last (numpy's order)
        assert_eq!(principal_axis(&[0.0; 9]), [0.0, 0.0, 1.0]);
        // a rotated one: (1, 1, 0) / sqrt 2 has the eigenvalue 3
        let a = principal_axis(&[2.0, 1.0, 0.0, 1.0, 2.0, 0.0, 0.0, 0.0, 1.0]);
        assert!((a[0].abs() - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-9 && (a[0] - a[1]).abs() < 1e-9);
        let r = resource(
            &TexInfo {
                fmt: 0,
                flags: 1,
                w: 4,
                h: 4,
                mips: 1,
            },
            &[vec![0; 8]],
        );
        assert_eq!(r.len(), texcheck::HEADER + 8);
        assert_eq!(unwrap(&wrap(&r)), r.as_slice());
        assert_eq!(tex_info(&r).unwrap().fmt, 0);
    }

    #[test]
    fn pictures_in_a_template_form() {
        // a DXT5 template 32 x 16 opaque in its top-left 20 x 12: the box is found, the rest is white and clear
        let mut px = vec![255u8; 32 * 16 * 4];
        for y in 0..16 {
            for x in 0..32 {
                px[(y * 32 + x) * 4 + 3] = if x < 20 && y < 12 { 255 } else { 0 };
            }
        }
        let info = TexInfo {
            fmt: 2,
            flags: 1,
            w: 32,
            h: 16,
            mips: 1,
        };
        let t = encode(
            &texwrite::Image {
                h: 16,
                w: 32,
                ch: 4,
                px,
            },
            &info,
        )
        .unwrap();
        let m = make(&gradient(64, 48), &wrap(&t)).unwrap();
        assert_eq!(m.area, [0, 0, 20, 12]);
        assert_eq!(tex_info(unwrap(&m.wrapped)).unwrap(), info);
        assert!(m.psnr > 25.0, "{}", m.psnr);
        let (_, alpha) = decode(unwrap(&m.wrapped)).unwrap();
        assert!(alpha[0] == 255 && alpha[31] == 0 && alpha[15 * 32] == 0);
        // a DXT1 template: the whole picture, cut to its 2:1
        let d1 = TexInfo {
            fmt: 0,
            flags: 1,
            w: 16,
            h: 8,
            mips: 1,
        };
        let t1 = encode(
            &texwrite::Image {
                h: 8,
                w: 16,
                ch: 3,
                px: vec![0; 16 * 8 * 3],
            },
            &d1,
        )
        .unwrap();
        let m1 = make(&gradient(40, 40), &t1).unwrap();
        assert_eq!(m1.area, [0, 0, 16, 8]);
        assert!(m1.psnr > 25.0, "{}", m1.psnr);
        // PNG round trip
        let png = png_bytes(&m1.source).unwrap();
        assert_eq!(read_image(&png).unwrap(), m1.source);
        assert!(read_image(b"GIF89a").is_err());
    }
}
