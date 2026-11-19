//! texqa's decoding (`read_pc`, `read_xbox`, `decode_level`): every level of a PC `ITexture` or an Xbox 360
//! `XenonTexture` to RGBA bytes, with NumPy's integer arithmetic (`//` floors) and RGBA16F's float32 steps.
//!
//! What NumPy would raise (a reshape or an index out of range) is a [`Fail::Python`]: texqa's row of such a texture
//! names NumPy's own message.

use crate::pyproc::Fail;

/// `texture.HEADER`.
pub const HEADER: usize = 92;
/// `texture.CUBE`.
pub const CUBE: u32 = 1;

/// Bytes per 4 x 4 block of the block formats (`BLOCK`).
pub fn block(fmt: u32) -> Option<usize> {
    Some(match fmt {
        0 | 18 | 3 => 8,
        1 | 2 | 19 | 20 => 16,
        _ => return None,
    })
}

/// Bytes per texel of the texel formats (`TEXEL`).
pub fn texel(fmt: u32) -> Option<usize> {
    Some(match fmt {
        10 => 1,
        5 => 2,
        9 => 4,
        13 => 8,
        _ => return None,
    })
}

/// The channels a format holds (`CHANNELS.get(fmt, 'rgba')`).
pub fn channels_of(fmt: u32) -> &'static str {
    match fmt {
        0 | 18 | 5 => "rgb",
        19 | 20 => "rg",
        3 | 10 => "r",
        _ => "rgba",
    }
}

/// `NAMES.get(fmt)`.
pub fn name_of(fmt: i128) -> Option<&'static str> {
    Some(match fmt {
        0 => "DXT1",
        18 => "DXT1 normal",
        1 => "DXT3",
        2 => "DXT5",
        19 => "BC5",
        20 => "BC5 (20)",
        3 => "DXT5A",
        10 => "L8",
        5 => "RGB565",
        9 => "BGRA8",
        13 => "RGBA16F",
        _ => return None,
    })
}

/// A texture decode error texqa words itself (`TexQaError`).
#[derive(Debug, Clone)]
pub struct QaErr(pub String);

/// What decoding gives: texqa's own error, or a hand-off.
#[derive(Debug, Clone)]
pub enum DErr {
    Qa(QaErr),
    Py,
}

impl From<Fail> for DErr {
    fn from(_: Fail) -> Self {
        DErr::Py
    }
}

pub type DResult<T> = Result<T, DErr>;

fn qa<T>(msg: String) -> DResult<T> {
    Err(DErr::Qa(QaErr(msg)))
}

/// One level as (h, w, 4) RGBA bytes.
#[derive(Debug, Clone)]
pub struct Img {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

impl Img {
    pub fn at(&self, y: usize, x: usize, c: usize) -> u8 {
        self.px[(y * self.w + x) * 4 + c]
    }
}

/// `Tex`.
#[derive(Debug, Clone)]
pub struct Tex {
    pub fmt: u32,
    pub flags: u32,
    pub typ: u32,
    pub w: u32,
    pub h: u32,
    pub mips: u32,
    pub levels: Vec<(usize, usize, Img)>,
    pub floats: Vec<Vec<f32>>,
    pub size_errors: Vec<String>,
    pub zero_blocks: f64,
}

impl Tex {
    pub fn level(&self, m: usize, s: usize) -> Option<&Img> {
        self.levels
            .iter()
            .find(|(sl, lv, _)| *sl == s && *lv == m)
            .map(|x| &x.2)
    }

    /// `Tex.channels`.
    pub fn channels(&self) -> String {
        let mut c = channels_of(self.fmt).to_owned();
        if matches!(self.fmt, 0 | 18)
            && let Some((_, _, img)) = self.levels.first()
            && img.px.chunks_exact(4).map(|p| p[3]).min().unwrap_or(255) < 255
        {
            c.push('a');
        }
        c
    }
}

fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn u32be(b: &[u8], o: usize) -> u32 {
    u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn rgb565(v: i64) -> [i64; 3] {
    [
        ((v >> 11) & 31) * 255 / 31,
        ((v >> 5) & 63) * 255 / 63,
        (v & 31) * 255 / 31,
    ]
}

/// `_color`: 16 texels' RGB and alpha of one 8-byte DXT colour half.
fn color(b: &[u8], force4: bool) -> ([[i64; 3]; 16], [i64; 16]) {
    let c0 = i64::from(b[0]) | (i64::from(b[1]) << 8);
    let c1 = i64::from(b[2]) | (i64::from(b[3]) << 8);
    let (p0, p1) = (rgb565(c0), rgb565(c1));
    let four = c0 > c1 || force4;
    let mut pal = [[0i64; 3]; 4];
    pal[0] = p0;
    pal[1] = p1;
    for k in 0..3 {
        pal[2][k] = if four {
            (2 * p0[k] + p1[k]).div_euclid(3)
        } else {
            (p0[k] + p1[k]).div_euclid(2)
        };
        pal[3][k] = if four { (p0[k] + 2 * p1[k]).div_euclid(3) } else { 0 };
    }
    let bits = u32le(b, 4);
    let mut out = [[0i64; 3]; 16];
    let mut alpha = [255i64; 16];
    for k in 0..16 {
        let idx = ((bits >> (2 * k)) & 3) as usize;
        out[k] = pal[idx];
        if idx == 3 && !four {
            alpha[k] = 0;
        }
    }
    (out, alpha)
}

/// `_alpha`: 16 values of one 8-byte DXT5 alpha half (BC5's channels, DXT5A).
fn alpha(b: &[u8]) -> [i64; 16] {
    let (a0, a1) = (i64::from(b[0]), i64::from(b[1]));
    let big = a0 > a1;
    let mut pal = [0i64; 8];
    pal[0] = a0;
    pal[1] = a1;
    for k in 1..7 {
        pal[k + 1] = ((7 - k as i64) * a0 + k as i64 * a1).div_euclid(7);
    }
    for k in 1..5 {
        if !big {
            pal[k + 1] = ((5 - k as i64) * a0 + k as i64 * a1).div_euclid(5);
        }
    }
    if !big {
        pal[6] = 0;
        pal[7] = 255;
    }
    let mut bits = 0u64;
    for k in 0..6 {
        bits |= u64::from(b[2 + k]) << (8 * k);
    }
    let mut out = [0i64; 16];
    for (k, o) in out.iter_mut().enumerate() {
        *o = pal[((bits >> (3 * k)) & 7) as usize];
    }
    out
}

/// IEEE half to float32 (NumPy's `astype(float32)` of `<f2`: exact).
pub fn half(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0f32 } else { 1.0 };
    let e = i32::from((h >> 10) & 31);
    let m = f32::from(h & 1023);
    if e == 0 {
        sign * m * (2.0f32).powi(-24)
    } else if e == 31 {
        if m == 0.0 { sign * f32::INFINITY } else { f32::NAN }
    } else {
        sign * (1.0 + m / 1024.0) * (2.0f32).powi(e - 15)
    }
}

/// `decode_level`: (RGBA, float32 RGBA for RGBA16F).
pub fn decode_level(
    data: &[u8],
    w: usize,
    h: usize,
    fmt: u32,
    pitch: Option<usize>,
) -> DResult<(Img, Option<Vec<f32>>)> {
    let (w, h) = (w.max(1), h.max(1));
    let mut out = vec![0u8; w * h * 4];
    for p in out.chunks_exact_mut(4) {
        p[3] = 255;
    }
    if let Some(bpb) = block(fmt) {
        let (bw, bh) = (w.div_ceil(4).max(1), h.div_ceil(4).max(1));
        let need = bw * bh * bpb;
        if data.len() < need {
            return qa(format!(
                "a {w} x {h} level of format {fmt} needs {need} bytes, has {}",
                data.len()
            ));
        }
        for by in 0..bh {
            for bx in 0..bw {
                let blk = &data[(by * bw + bx) * bpb..(by * bw + bx + 1) * bpb];
                let mut px = [[0i64; 4]; 16];
                match fmt {
                    0 | 18 => {
                        let (rgb, a) = color(blk, false);
                        for k in 0..16 {
                            px[k] = [rgb[k][0], rgb[k][1], rgb[k][2], if fmt == 0 { a[k] } else { 255 }];
                        }
                    }
                    1 => {
                        let (rgb, _) = color(&blk[8..], true);
                        for k in 0..16 {
                            let a = ((i64::from(blk[k / 2]) >> (4 * (k % 2))) & 15) * 17;
                            px[k] = [rgb[k][0], rgb[k][1], rgb[k][2], a];
                        }
                    }
                    2 => {
                        let (rgb, _) = color(&blk[8..], true);
                        let a = alpha(&blk[..8]);
                        for k in 0..16 {
                            px[k] = [rgb[k][0], rgb[k][1], rgb[k][2], a[k]];
                        }
                    }
                    19 | 20 => {
                        let (r, g) = (alpha(&blk[..8]), alpha(&blk[8..]));
                        for k in 0..16 {
                            px[k] = [r[k], g[k], 0, 255];
                        }
                    }
                    _ => {
                        let v = alpha(blk);
                        for k in 0..16 {
                            px[k] = [v[k], v[k], v[k], 255];
                        }
                    }
                }
                for k in 0..16 {
                    let (y, x) = (by * 4 + k / 4, bx * 4 + k % 4);
                    if y < h && x < w {
                        for c in 0..4 {
                            out[(y * w + x) * 4 + c] = px[k][c] as u8;
                        }
                    }
                }
            }
        }
        return Ok((Img { w, h, px: out }, None));
    }
    let Some(bpt) = texel(fmt) else {
        return qa(format!("texture format {fmt} is not decoded"));
    };
    let pitch = pitch.unwrap_or_else(|| (w * bpt).max(data.len() / h));
    if data.len() < pitch * (h - 1) + w * bpt {
        return qa(format!(
            "a {w} x {h} level of format {fmt} needs {} bytes, has {}",
            w * h * bpt,
            data.len()
        ));
    }
    let mut floats = None;
    let mut fl = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let t = &data[y * pitch + x * bpt..y * pitch + (x + 1) * bpt];
            let o = &mut out[(y * w + x) * 4..(y * w + x) * 4 + 4];
            match fmt {
                10 => {
                    o[0] = t[0];
                    o[1] = t[0];
                    o[2] = t[0];
                }
                9 => {
                    o[0] = t[2];
                    o[1] = t[1];
                    o[2] = t[0];
                    o[3] = t[3];
                }
                5 => {
                    let v = u32::from(t[0]) | (u32::from(t[1]) << 8);
                    let (r5, g6, b5) = ((v >> 11) & 31, (v >> 5) & 63, v & 31);
                    o[0] = ((r5 << 3) | (r5 >> 2)) as u8;
                    o[1] = ((g6 << 2) | (g6 >> 4)) as u8;
                    o[2] = ((b5 << 3) | (b5 >> 2)) as u8;
                }
                _ => {
                    for c in 0..4 {
                        let f = half(u16::from_le_bytes([t[2 * c], t[2 * c + 1]]));
                        fl.push(f);
                        let g = if f.is_nan() {
                            0.0
                        } else if f == f32::INFINITY {
                            1.0
                        } else if f == f32::NEG_INFINITY {
                            0.0
                        } else {
                            f
                        };
                        let v = (g * 255.0f32 + 0.5f32).clamp(0.0, 255.0);
                        o[c] = v as u8;
                    }
                }
            }
        }
    }
    if fmt == 13 {
        floats = Some(fl);
    }
    Ok((Img { w, h, px: out }, floats))
}

/// `level_size`.
pub fn level_size(fmt: u32, w: usize, h: usize) -> usize {
    if let Some(b) = block(fmt) {
        return w.div_ceil(4).max(1) * h.div_ceil(4).max(1) * b;
    }
    let bpt = texel(fmt).unwrap_or(0);
    (if fmt == 10 { (w * bpt).div_ceil(4) * 4 } else { w * bpt }) * h
}

/// `unwrap`.
pub fn unwrap(res: &[u8]) -> &[u8] {
    if res.len() >= 64 && &res[..4] == b"RES\x02" {
        let n = u32le(res, 56) as usize;
        return res.get(64..(64 + n).min(res.len())).unwrap_or(&[]);
    }
    res
}

/// `_sized_texture` / `is_texture`.
pub fn sized_texture(head: &[u8], size: u64) -> bool {
    if head.len() < HEADER {
        return false;
    }
    let (ver, typ, w, h, mips) = (
        u32le(head, 0),
        u32le(head, 4),
        u32le(head, 16),
        u32le(head, 20),
        u32le(head, 28),
    );
    if ver != 3 || typ > 1 || !(1..=15).contains(&mips) || !(1..=16384).contains(&w) || !(1..=16384).contains(&h) {
        return false;
    }
    let sum: u64 = (0..mips as usize).map(|m| u64::from(u32le(head, 32 + 4 * m))).sum();
    HEADER as u64 + (if typ == CUBE { 6 } else { 1 }) * sum == size
}

/// `read_pc`.
pub fn read_pc(res: &[u8]) -> DResult<Tex> {
    let res = unwrap(res);
    if res.len() < HEADER {
        return qa("shorter than a texture header".to_owned());
    }
    let hd = |i: usize| u32le(res, 4 * i);
    let (typ, fmt, flags, w, h, mips) = (hd(1), hd(2), hd(3), hd(4), hd(5), hd(7));
    let sizes: Vec<usize> = (0..15).map(|i| u32le(res, 32 + 4 * i) as usize).collect();
    if mips > 15 {
        return Err(DErr::Py); // sizes[m] out of range: IndexError
    }
    let mut t = Tex {
        fmt,
        flags,
        typ,
        w,
        h,
        mips,
        levels: Vec::new(),
        floats: Vec::new(),
        size_errors: Vec::new(),
        zero_blocks: 0.0,
    };
    let mut pos = HEADER;
    for s in 0..(if typ == CUBE { 6 } else { 1 }) {
        for m in 0..mips as usize {
            let (lw, lh) = ((w as usize >> m.min(63)).max(1), (h as usize >> m.min(63)).max(1));
            let data = res
                .get(pos.min(res.len())..(pos + sizes[m]).min(res.len()))
                .unwrap_or(&[]);
            pos += sizes[m];
            if s == 0
                && (block(fmt).is_some() || texel(fmt).is_some())
                && sizes[m] != level_size(fmt, lw, lh)
                && !(fmt == 10 && sizes[m] == lw * lh)
            {
                t.size_errors.push(format!(
                    "level {m} ({lw} x {lh}): {} bytes, format {fmt} needs {}",
                    sizes[m],
                    level_size(fmt, lw, lh)
                ));
            }
            let (img, f) = decode_level(data, lw, lh, fmt, None)?;
            t.levels.push((s, m, img));
            if let Some(f) = f {
                t.floats.push(f);
            }
            if s == 0
                && m == 0
                && let Some(b) = block(fmt)
            {
                t.zero_blocks = zero_share(data, b)?;
            }
        }
    }
    Ok(t)
}

fn zero_share(data: &[u8], b: usize) -> DResult<f64> {
    if !data.len().is_multiple_of(b) {
        return Err(DErr::Py); // NumPy's reshape error
    }
    let n = data.len() / b;
    if n == 0 {
        return Ok(0.0);
    }
    let z = data.chunks_exact(b).filter(|c| c.iter().all(|&x| x == 0)).count();
    Ok((0.0 + z as f64) / n as f64)
}

/// `read_xbox`.
pub fn read_xbox(res: &[u8]) -> DResult<Tex> {
    if res.len() < HEADER {
        return qa("shorter than a texture header".to_owned());
    }
    let hd = |i: usize| u32be(res, 4 * i);
    let (typ, fmt, flags, w, h, mips) = (hd(1), hd(2), hd(3), hd(4), hd(5), hd(7));
    let Some((bpb, edge)) = bc2_convert::texture::format(fmt) else {
        return qa(format!("console texture format {fmt} is not decoded"));
    };
    if typ > 1 {
        return qa(format!("console texture type {typ} is not decoded"));
    }
    let log_bpp = match bpb {
        1 => 0,
        2 => 1,
        8 => 3,
        16 => 4,
        _ => return Err(DErr::Qa(QaErr(bpb.to_string()))), // KeyError(bpb): str() is the key
    };
    if w == 0 || h == 0 || mips == 0 || mips > 15 {
        return Err(DErr::Py);
    }
    let slices = if typ == CUBE { 6 } else { 1 };
    let (w, h) = (w as usize, h as usize);
    let Some((place, size)) = bc2_convert::texture::layout(w, h, mips as usize, fmt, slices) else {
        return Err(DErr::Py);
    };
    let raw = &res[HEADER..];
    if raw.len() < size {
        return qa(format!(
            "{} bytes of console texture data, the layout needs {size}",
            raw.len()
        ));
    }
    let data: Vec<u8> = if bpb > 1 {
        let mut d = raw[..raw.len() / 2 * 2].to_vec();
        for p in d.chunks_exact_mut(2) {
            p.swap(0, 1);
        }
        d
    } else {
        raw.to_vec()
    };
    let mut t = Tex {
        fmt,
        flags,
        typ,
        w: w as u32,
        h: h as u32,
        mips,
        levels: Vec::new(),
        floats: Vec::new(),
        size_errors: Vec::new(),
        zero_blocks: 0.0,
    };
    for s in 0..slices {
        for m in 0..mips as usize {
            let pl = place[s][m];
            let (lw, lh) = ((w >> m.min(63)).max(1), (h >> m.min(63)).max(1));
            let (lbw, lbh) = (lw.div_ceil(edge).max(1), lh.div_ceil(edge).max(1));
            let mut lev = Vec::with_capacity(lbw * lbh * bpb);
            for y in 0..lbh {
                for x in 0..lbw {
                    let idx = bc2_convert::texture::tiled(x + pl.x, y + pl.y, pl.pitch, log_bpp);
                    let off = pl.start + idx * bpb;
                    let Some(b) = data.get(off..off + bpb) else {
                        return Err(DErr::Py); // NumPy's IndexError
                    };
                    lev.extend_from_slice(b);
                }
            }
            let (img, f) = decode_level(&lev, lw, lh, fmt, if edge == 1 { Some(lbw * bpb) } else { None })?;
            t.levels.push((s, m, img));
            if s == 0 && m == 0 && block(fmt).is_some() {
                t.zero_blocks = zero_share(&lev, bpb)?;
            }
            if let Some(f) = f {
                t.floats.push(f);
            }
        }
    }
    Ok(t)
}
