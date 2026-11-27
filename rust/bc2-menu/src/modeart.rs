//! The Offline Matches panel's ONSLAUGHT mode picture (round OR5): the port of Project OPFOR's
//! `src\tools\menus\onslaught_art.py`, which `build_menus.bat` ran on the unpacked, patched main menu bundle before
//! repacking it. `bfbc2 menuart onslaught <unpacked mainmenu folder> [--xbox DIR] [--refresh] [--cache FILE]`.
//!
//! The picture, in this order: the cache (`work\import\onslaught_mode.png`, never committed or shipped); the user's own
//! Xbox 360 Update #3 package (`454108A800000003` under `--xbox`, else `OPFOR_XBOX`, else the default folder): the
//! console main menu's mode-picture sprite has a frame `onslaught`, whose shape -> geometry -> image -> atlas
//! rectangle is cut from the console atlas (Xbox 360 tiled DXT5), its light haze made transparent and fitted to
//! 413 x 226, then cached; otherwise a picture composed from pieces of the PC menu's own atlas.
//!
//! The PC panel's picture sprite (frames CONQUEST ... ANY) gets a frame `ONSLAUGHT` placing a new shape textured with
//! a new image, whose pixels go into free space in the menu atlas (4096 x 4096 DXT5) with a new entry in its index
//! (`atlas_meta.dbx`). New data is appended to the members and the tables it extends are copied (APT converts a
//! movie's offsets to pointers in place, once: nothing new may be shared with existing data; DEVELOPMENT_NOTES.md,
//! "Menu build pipeline").
//!
//! The pixels are the Python's: `crate::pil` gives Pillow's (resize, paste, composite, DXT5 decode) and the cut-out's
//! float32 arithmetic is numpy's. The cache is compared by its pixels, not its PNG bytes (spec MENU-ART-1).

#![allow(
    clippy::manual_is_multiple_of,
    clippy::manual_ignore_case_cmp,
    clippy::chunks_exact_to_as_chunks,
    clippy::single_match_else,
    reason = "written as the Python reads (len % 4, upper() ==, its if / else)"
)]
#![allow(
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::doc_markdown,
    reason = "onslaught_art.py's names kept for review against it; each index is bounded by the check before it, as the Python's"
)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    reason = "the movie's u32 fields and the Python's float arithmetic"
)]

use std::path::{Path, PathBuf};

use crate::error::{MenuError, MenuResult};
use crate::pil::{self, Rgba};

/// The character signature.
const SIG: u32 = 0x0987_6543;
/// The PC panel's mode picture size.
pub const PIC_W: usize = 413;
/// The PC panel's mode picture height.
pub const PIC_H: usize = 226;
/// The PC menu atlas' size.
const ATLAS: usize = 4096;
/// Where the Xbox 360 files are looked for when neither `--xbox` nor `OPFOR_XBOX` names a folder.
pub const XBOX_DEFAULT: &str = r"C:\Games\Battlefield Bad Company 2 Limited Edition (XBOX 360)";
/// The Update #3 package's file name.
const PACKAGE: &str = "454108A800000003";

/// Fallback composition (image id, scale, mirrored, x, y), back to front: PC main_nav atlas images 398 (Apache), 395
/// (running soldier), 401 (soldier with rifle), each placed where it overlaps the console silhouettes best.
const FALLBACK_PIECES: [(u32, f64, bool, i64, i64); 5] = [
    (398, 0.62, true, 23, 3),
    (395, 0.3, true, 355, 202),
    (395, 0.47, false, 38, 165),
    (395, 0.68, true, 92, 122),
    (401, 1.38, true, 183, -11),
];
/// The fallback's haze: no haze (the PC mode pictures are plain cut-outs; user, 2026-10-24).
const HAZE: (i64, i64, f64) = (35, 377, 0.0);

fn err(msg: impl Into<String>) -> MenuError {
    MenuError::Menu(msg.into())
}

fn u32le(b: &[u8], o: usize) -> MenuResult<u32> {
    b.get(o..o + 4)
        .and_then(|s| s.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| err(format!("a read at {o:#x} past the end ({} bytes)", b.len())))
}

fn u32be(b: &[u8], o: usize) -> MenuResult<u32> {
    b.get(o..o + 4)
        .and_then(|s| s.try_into().ok())
        .map(u32::from_be_bytes)
        .ok_or_else(|| err(format!("a read at {o:#x} past the end ({} bytes)", b.len())))
}

fn put(b: &mut [u8], o: usize, v: u32) -> MenuResult<()> {
    b.get_mut(o..o + 4)
        .ok_or_else(|| err(format!("a write at {o:#x} past the end")))?
        .copy_from_slice(&v.to_le_bytes());
    Ok(())
}

fn cstr(b: &[u8], p: usize) -> MenuResult<String> {
    let rest = b
        .get(p..)
        .ok_or_else(|| err(format!("a string at {p:#x} past the end")))?;
    let e = rest
        .iter()
        .position(|&c| c == 0)
        .ok_or_else(|| err(format!("a string at {p:#x} without its NUL")))?;
    Ok(rest[..e].iter().map(|&c| char::from(c)).collect())
}

/// `append(buf, data)`: `data` at the 4-aligned end of `buf`, its offset.
fn append(buf: &mut Vec<u8>, data: &[u8]) -> u32 {
    while buf.len() % 4 != 0 {
        buf.push(0);
    }
    let at = buf.len() as u32;
    buf.extend_from_slice(data);
    at
}

/// Python's `round()` of a float to an int (half to even).
fn py_round(x: f64) -> i64 {
    x.round_ties_even() as i64
}

/// A movie read with one byte order: `u` / `s` as `apt.Movie` / `onslaught_art.Movie` read it.
struct View<'a> {
    a: &'a [u8],
    big: bool,
    m: usize,
}

impl View<'_> {
    fn u(&self, o: usize) -> MenuResult<usize> {
        Ok((if self.big { u32be(self.a, o) } else { u32le(self.a, o) })? as usize)
    }

    fn s(&self, p: usize) -> MenuResult<String> {
        cstr(self.a, p)
    }

    /// The character table's pointers.
    fn chars(&self) -> MenuResult<Vec<usize>> {
        let (n, ptr) = (self.u(self.m + 20)?, self.u(self.m + 24)?);
        (0..n).map(|k| self.u(ptr + 4 * k)).collect()
    }

    /// Each frame's item pointers.
    fn frames(&self, cp: usize) -> MenuResult<Vec<Vec<usize>>> {
        let (n, fptr) = (self.u(cp + 8)?, self.u(cp + 12)?);
        (0..n)
            .map(|fr| {
                let (cnt, items) = (self.u(fptr + 8 * fr)?, self.u(fptr + 8 * fr + 4)?);
                (0..cnt).map(|k| self.u(items + 4 * k)).collect()
            })
            .collect()
    }

    fn labels(&self, frame: &[usize]) -> MenuResult<Vec<String>> {
        let mut out = Vec::new();
        for &it in frame {
            if self.u(it)? == 2 {
                out.push(self.s(self.u(it + 4)?)?);
            }
        }
        Ok(out)
    }
}

/// `atlas_infos(meta_xml)`: image id -> (min u, max u, min v, max v) from the atlas index's XML.
pub fn atlas_infos(xml: &str) -> MenuResult<Vec<(u32, [f64; 4])>> {
    static RE: std::sync::LazyLock<Option<regex::Regex>> = std::sync::LazyLock::new(|| {
        regex::Regex::new(
            r#"(?s)TextureFile">(\d+)<.*?MinU">([-\d.eE]+)<.*?MaxU">([-\d.eE]+)<.*?MinV">([-\d.eE]+)<.*?MaxV">([-\d.eE]+)<"#,
        )
        .ok()
    });
    let re = RE.as_ref().ok_or_else(|| err("the atlas pattern"))?;
    let mut out: Vec<(u32, [f64; 4])> = Vec::new();
    for c in re.captures_iter(xml) {
        let num = |i: usize| -> MenuResult<f64> {
            let t = c.get(i).map_or("", |m| m.as_str());
            t.parse::<f64>()
                .map_err(|_| err(format!("atlas index: {t:?} is not a number")))
        };
        let id: u32 = c
            .get(1)
            .and_then(|m| m.as_str().parse().ok())
            .ok_or_else(|| err("atlas index: an image id"))?;
        let v = [num(2)?, num(3)?, num(4)?, num(5)?];
        // a dict: a repeated id keeps its first place and takes the last value
        if let Some(e) = out.iter_mut().find(|e| e.0 == id) {
            e.1 = v;
        } else {
            out.push((id, v));
        }
    }
    Ok(out)
}

fn info(infos: &[(u32, [f64; 4])], id: u32) -> Option<[f64; 4]> {
    infos.iter().find(|e| e.0 == id).map(|e| e.1)
}

/// A file read as Python's text mode reads it: UTF-8, `\r\n` and `\r` as `\n`.
fn read_universal(path: &Path) -> MenuResult<String> {
    let b = std::fs::read(path).map_err(|e| MenuError::io(&path.to_string_lossy(), e))?;
    let t = String::from_utf8(b).map_err(|_| err(format!("{}: not UTF-8", path.display())))?;
    Ok(t.replace("\r\n", "\n").replace('\r', "\n"))
}

/// DbxEdit's command line in process; a failure is an error with its output.
fn dbxedit(args: &[&str]) -> MenuResult<()> {
    let a: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
    let o = bc2_tools::dbxedit::run(&a, None);
    if o.code != 0 {
        return Err(err(format!("DbxEdit {}: {}{}", args.join(" "), o.stdout, o.stderr)));
    }
    Ok(())
}

/// The atlas index's XML beside `meta` (`DbxEdit xml`), read in text mode; the file is left there.
fn meta_xml(meta: &Path) -> MenuResult<(PathBuf, String)> {
    dbxedit(&["xml", &meta.to_string_lossy()])?;
    let xml = meta.with_extension("xml");
    let t = read_universal(&xml)?;
    Ok((xml, t))
}

/// `geometry_image(geo, big, gid)`: the image id geometry `gid`'s first part uses (word 20 of the part).
fn geometry_image(geo: &[u8], big: bool, gid: usize) -> MenuResult<Option<usize>> {
    let v = View { a: geo, big, m: 0 };
    let (n, tab) = (v.u(0x18)?, v.u(0x1C)?);
    for k in 0..n {
        let (g, q) = (v.u(tab + 12 * k)?, v.u(tab + 12 * k + 8)?);
        if g == gid {
            let part = v.u(q)?;
            return Ok(Some(v.u(part + 80)?));
        }
    }
    Ok(None)
}

/// The log: each line indented two spaces, as the Python printed it.
pub type Log<'a> = &'a mut dyn FnMut(&str);

/// A temporary folder removed when dropped.
struct Temp(PathBuf);

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_dir() -> MenuResult<Temp> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let base = std::env::temp_dir();
    for _ in 0..100 {
        let p = base.join(format!(
            "opfor_ons_{}_{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        if std::fs::create_dir(&p).is_ok() {
            return Ok(Temp(p));
        }
    }
    Err(err("no temporary folder"))
}

/// The Update #3 package under `xbox` (the last one found, walking top-down, files before sub-folders).
fn find_package(xbox: &Path) -> Option<PathBuf> {
    fn walk(d: &Path, found: &mut Option<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        let mut dirs = Vec::new();
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                dirs.push(p);
            } else if e.file_name().to_string_lossy().to_uppercase() == PACKAGE {
                *found = Some(p);
            }
        }
        for s in dirs {
            walk(&s, found);
        }
    }
    let mut found = None;
    walk(xbox, &mut found);
    found
}

/// `from_xbox(xbox, log)`: the console menu's Onslaught mode picture from the user's Update #3 package, or `None`.
pub fn from_xbox(xbox: &Path, log: Log<'_>) -> MenuResult<Option<Rgba>> {
    let Some(pkg) = find_package(xbox) else {
        log(&format!(
            "no Xbox 360 Update #3 package ({PACKAGE}) under {}",
            xbox.display()
        ));
        return Ok(None);
    };
    let f = std::fs::File::open(&pkg).map_err(|e| MenuError::io(&pkg.to_string_lossy(), e))?;
    let mut p = bc2_formats::stfs::Package::open(f).map_err(|e| err(format!("{}: {e}", pkg.display())))?;
    let entry = p
        .entries
        .iter()
        .find(|e| e.path.to_ascii_lowercase() == b"menu/main/mainmenu-00.fbrb")
        .cloned()
        .ok_or_else(|| err("the package has no menu/main/mainmenu-00.fbrb"))?;
    let bundle = p
        .read(
            u64::from(entry.start),
            u64::from(entry.count),
            entry.consecutive,
            u64::from(entry.size),
        )
        .map_err(|e| err(format!("{}: {e}", pkg.display())))?;
    let tmp = temp_dir()?;
    let unpacked = tmp.0.join("u");
    bc2_tools::unpack_bytes(&bundle, &unpacked).map_err(|e| err(format!("the console main menu bundle: {e}")))?;
    let menu = unpacked.join("Menu");
    let res = std::fs::read(menu.join("main_nav.res")).map_err(|e| MenuError::io("main_nav.res", e))?;
    let items = crate::aptpatch::members(&res, "main_nav.res")?;
    let member = |name: &str| items.iter().find(|(n, _)| n == name).map(|(_, d)| d.as_slice());
    let (a, c) = (
        member("main_nav.apt").ok_or_else(|| err("main_nav.res: no main_nav.apt"))?,
        member("main_nav.const").ok_or_else(|| err("main_nav.res: no main_nav.const"))?,
    );
    let mv = console_view(a, c)?;
    let mut shape = None;
    for cp in mv.chars()? {
        // apt.Movie.characters(): a character with its signature; sprites only
        if cp == 0 || cp + 8 > a.len() || mv.u(cp + 4)? != SIG as usize || mv.u(cp)? != 5 {
            continue;
        }
        for frame in mv.frames(cp)? {
            if mv.labels(&frame)?.iter().any(|l| l == "onslaught") {
                let mut place = None;
                for &it in &frame {
                    if mv.u(it)? == 3 {
                        place = Some(mv.u(it + 12)?);
                        break;
                    }
                }
                shape = Some(place.ok_or_else(|| err("the onslaught frame places nothing"))?);
            }
        }
    }
    let Some(shape) = shape else {
        log("the console menu has no \"onslaught\" mode picture");
        return Ok(None);
    };
    let geo = member("main_nav.geo").ok_or_else(|| err("main_nav.res: no main_nav.geo"))?;
    let image = geometry_image(geo, true, shape)?.ok_or_else(|| err(format!("no geometry {shape}")))?;
    let tex_dir = menu.join("main_nav_textures_Xenon");
    let (_, xml) = meta_xml(&tex_dir.join("atlas_meta.dbx"))?;
    let [u0, u1, v0, v1] = info(&atlas_infos(&xml)?, image as u32)
        .ok_or_else(|| err(format!("the console atlas has no image {image}")))?;
    let tex = std::fs::read(tex_dir.join("atlas0.res")).map_err(|e| MenuError::io("atlas0.res", e))?;
    let (w, h) = (u32be(&tex, 16)? as usize, u32be(&tex, 20)? as usize);
    let body = tex
        .get(92..92 + w * h)
        .ok_or_else(|| err("the console atlas is cut short"))?;
    let atlas = pil::dxt5_decode(&pil::xbox_untile_dxt(body, w, h)?, w, h)?;
    let (wf, hf) = (w as f64, h as f64);
    // in from the edges: the rectangle's rounded edges touch neighbouring images in the console atlas
    let bx = (
        py_round(u0 * wf) + 1,
        py_round(v0 * hf) + 1,
        py_round(u1 * wf) - 4,
        py_round(v1 * hf) - 1,
    );
    log(&format!(
        "console Onslaught picture: shape {shape}, image {image}, atlas ({}, {}, {}, {}) from {}",
        bx.0,
        bx.1,
        bx.2,
        bx.3,
        pkg.display()
    ));
    Ok(Some(atlas.crop(bx.0, bx.1, bx.2, bx.3)))
}

/// The console movie as `apt.Movie` reads its byte order: big-endian when only the big-endian movie offset lies in
/// the file, or when the signature at it decides.
fn console_view<'a>(a: &'a [u8], c: &[u8]) -> MenuResult<View<'a>> {
    let (le, be) = (u32le(c, 20)? as usize, u32be(c, 20)? as usize);
    let n = a.len();
    let mut big = !(0 < le && le < n) && 0 < be && be < n;
    if 0 < be
        && be + 8 <= n
        && a[be + 4..be + 8] == [0x09, 0x87, 0x65, 0x43]
        && !(0 < le && le + 8 <= n && a[le + 4..le + 8] == [0x43, 0x65, 0x87, 0x09])
    {
        big = true;
    }
    Ok(View {
        a,
        big,
        m: if big { be } else { le },
    })
}

/// `cutout(pic)`: the light haze made transparent (a pixel at or above luminance 235 vanishes, at or below 150 keeps
/// its alpha, linear between), in numpy's float32 arithmetic.
#[must_use]
pub fn cutout(pic: &Rgba) -> Rgba {
    let (clear, solid) = (235.0f32, 150.0f32);
    let mut out = pic.clone();
    for p in out.px.chunks_exact_mut(4) {
        // numpy's float32 matmul (OpenBLAS' kernel): green first, then red and blue each fused (one rounding)
        let (r, g, b) = (f64::from(p[0]), f64::from(p[1]), f64::from(p[2]));
        let w = [f64::from(0.299f32), f64::from(0.587f32), f64::from(0.114f32)];
        let mut lum = (g * w[1]) as f32;
        lum = (f64::from(lum) + r * w[0]) as f32;
        lum = (f64::from(lum) + b * w[2]) as f32;
        let keep = ((clear - lum) / (clear - solid)).clamp(0.0, 1.0);
        p[3] = (f32::from(p[3]) * keep) as u8;
    }
    out
}

/// `fit(pic)`: scaled to the panel's picture and centred in a transparent 413 x 226 canvas.
#[must_use]
pub fn fit(pic: &Rgba) -> Rgba {
    let s = (PIC_W as f64 / pic.w as f64).min(PIC_H as f64 / pic.h as f64);
    let (w, h) = (
        py_round(pic.w as f64 * s).max(1) as usize,
        py_round(pic.h as f64 * s).max(1) as usize,
    );
    let pic = pic.resize_lanczos(w, h);
    let mut out = Rgba::filled(PIC_W, PIC_H, [0; 4]);
    out.paste(&pic, ((PIC_W - w) / 2) as i64, ((PIC_H - h) / 2) as i64);
    out
}

/// `composed(pc_atlas, infos, log)`: the fallback picture from pieces of the PC menu's atlas.
pub fn composed(atlas: &Rgba, infos: &[(u32, [f64; 4])], log: Log<'_>) -> Rgba {
    let (x0, x1, alpha) = HAZE;
    let mut out = Rgba::filled(PIC_W, PIC_H, [250, 250, 250, 0]);
    for x in 0..PIC_W {
        let xi = x as i64;
        let ramp = (((xi - x0).min(x1 - xi)) as f64 / 12.0).clamp(0.0, 1.0);
        let a = (alpha * ramp) as u8;
        for y in 0..PIC_H {
            out.px[(y * PIC_W + x) * 4 + 3] = a;
        }
    }
    let at = ATLAS as f64;
    for (image, scale, mirrored, x, y) in FALLBACK_PIECES {
        let Some([u0, u1, v0, v1]) = info(infos, image) else {
            log(&format!(
                "fallback picture: the PC atlas has no image {image} - left out"
            ));
            continue;
        };
        let mut piece = atlas.crop(
            py_round(u0 * at),
            py_round(v0 * at),
            py_round(u1 * at),
            py_round(v1 * at),
        );
        let (w, h) = (
            py_round(piece.w as f64 * scale).max(1) as usize,
            py_round(piece.h as f64 * scale).max(1) as usize,
        );
        piece = piece.resize_lanczos(w, h);
        if mirrored {
            piece = piece.flip_lr();
        }
        let mut layer = Rgba::filled(PIC_W, PIC_H, [0; 4]);
        layer.paste_masked(&piece, x, y);
        out.alpha_composite(&layer);
    }
    log("Onslaught picture composed from the PC menu atlas (no Xbox 360 files)");
    out
}

/// `edit_menu(res_path, ...)`: the ONSLAUGHT frame, shape and image added to `main_nav.res`; the new image id, or
/// `None` when the movie already has the frame.
#[expect(clippy::too_many_lines, reason = "onslaught_art.edit_menu, step by step")]
pub fn edit_menu(res_path: &Path, log: Log<'_>) -> MenuResult<Option<u32>> {
    let res = std::fs::read(res_path).map_err(|e| MenuError::io(&res_path.to_string_lossy(), e))?;
    let mut items = crate::aptpatch::members(&res, &res_path.to_string_lossy())?;
    let idx = |name: &str| {
        items
            .iter()
            .position(|(n, _)| n == name)
            .ok_or_else(|| err(format!("main_nav.res: no member {name}")))
    };
    let (ia, ic, ig) = (idx("main_nav.apt")?, idx("main_nav.const")?, idx("main_nav.geo")?);
    let mut apt = std::mem::take(&mut items[ia].1);
    let mut geo = std::mem::take(&mut items[ig].1);
    let m = u32le(&items[ic].1, 20)? as usize;
    let (nchars, cptr, cps, target, frames, label, place, remove, tmpl_shape) = {
        let mv = View { a: &apt, big: false, m };
        let cps = mv.chars()?;
        let (nchars, cptr) = (cps.len(), mv.u(m + 24)?);
        let mut target = None;
        for (cid, &cp) in cps.iter().enumerate() {
            if cp != 0 && mv.u(cp)? == 5 && mv.u(cp + 8)? != 0 {
                let labels: Vec<String> = mv
                    .frames(cp)?
                    .iter()
                    .map(|f| mv.labels(f))
                    .collect::<MenuResult<Vec<_>>>()?
                    .concat();
                if labels.iter().any(|l| l == "CONQUEST") && labels.iter().any(|l| l == "ANY") {
                    target = Some(cid);
                }
            }
        }
        let target = target.ok_or_else(|| err("main_nav: no mode-picture sprite (frames CONQUEST ... ANY)"))?;
        let frames = mv.frames(cps[target])?;
        let all: Vec<String> = frames
            .iter()
            .map(|f| mv.labels(f))
            .collect::<MenuResult<Vec<_>>>()?
            .concat();
        if all.iter().any(|l| l == "ONSLAUGHT") {
            log("main_nav already has an ONSLAUGHT mode picture - unchanged");
            return Ok(None);
        }
        let last = frames.last().ok_or_else(|| err("main_nav: the sprite has no frames"))?;
        let kinds: Vec<usize> = last.iter().map(|&it| mv.u(it)).collect::<MenuResult<_>>()?;
        let of = |k: usize| {
            kinds
                .iter()
                .position(|&x| x == k)
                .map(|i| last[i])
                .ok_or_else(|| err(format!("main_nav: the last frame has no item of kind {k}")))
        };
        let (place, label, remove) = (of(3)?, of(2)?, of(4)?);
        let first_place = |frame: &[usize]| -> MenuResult<usize> {
            for &it in frame {
                if mv.u(it)? == 3 {
                    return mv.u(it + 12);
                }
            }
            Err(err("main_nav: a frame without a placement"))
        };
        let first_mode = first_place(frames.first().ok_or_else(|| err("no frames"))?)?;
        let mode_cp = *cps.get(first_mode).ok_or_else(|| err("main_nav: no such character"))?;
        let tmpl_shape = first_place(mv.frames(mode_cp)?.first().ok_or_else(|| err("no frames"))?)?;
        (
            nchars,
            cptr,
            cps,
            target,
            frames.len(),
            label,
            place,
            remove,
            tmpl_shape,
        )
    };
    let (img_id, shape_id) = (nchars as u32, nchars as u32 + 1);
    let sp = cps[target];
    let slice = |b: &[u8], at: usize, n: usize| -> MenuResult<Vec<u8>> {
        b.get(at..at + n)
            .map(<[u8]>::to_vec)
            .ok_or_else(|| err(format!("main_nav: {n} bytes at {at:#x} past the end")))
    };
    // new image and shape characters
    let mut img = Vec::new();
    for v in [7, SIG, img_id] {
        img.extend(v.to_le_bytes());
    }
    let new_img = append(&mut apt, &img);
    let tmpl_cp = *cps.get(tmpl_shape).ok_or_else(|| err("main_nav: no template shape"))?;
    let mut shape_bytes = slice(&apt, tmpl_cp, 28)?;
    put(&mut shape_bytes, 24, shape_id)?; // geometry id = character id
    let new_shape = append(&mut apt, &shape_bytes);
    // the new frame: RemoveObject (copy), Action "Stop / End", FrameLabel "ONSLAUGHT", PlaceObject (copy, our shape)
    let code = append(&mut apt, &[0x07, 0x00]);
    let mut act = 1u32.to_le_bytes().to_vec();
    act.extend(code.to_le_bytes());
    let action = append(&mut apt, &act);
    let name = append(&mut apt, b"ONSLAUGHT\0");
    let mut lab = slice(&apt, label, 16)?;
    put(&mut lab, 4, name)?;
    put(&mut lab, 12, frames as u32)?;
    let lab_at = append(&mut apt, &lab);
    let rem = slice(&apt, remove, 8)?;
    let rem_at = append(&mut apt, &rem);
    let mut pl = slice(&apt, place, 0x40)?;
    put(&mut pl, 12, shape_id)?;
    if u32le(&pl, 0x34)? != 0 || u32le(&pl, 0x3C)? != 0 {
        return Err(err("main_nav: the template placement has a name or clip events"));
    }
    let pl_at = append(&mut apt, &pl);
    let list: Vec<u8> = [rem_at, action, lab_at, pl_at]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    let items_at = append(&mut apt, &list);
    // the sprite's frame list with the new frame; the character table with the two new characters
    let (fcount, fptr) = (u32le(&apt, sp + 8)? as usize, u32le(&apt, sp + 12)? as usize);
    let mut fl = slice(&apt, fptr, 8 * fcount)?;
    fl.extend(4u32.to_le_bytes());
    fl.extend(items_at.to_le_bytes());
    let flist = append(&mut apt, &fl);
    put(&mut apt, sp + 8, fcount as u32 + 1)?;
    put(&mut apt, sp + 12, flist)?;
    let mut ct = slice(&apt, cptr, 4 * nchars)?;
    ct.extend(new_img.to_le_bytes());
    ct.extend(new_shape.to_le_bytes());
    let ctab = append(&mut apt, &ct);
    put(&mut apt, m + 20, nchars as u32 + 2)?;
    put(&mut apt, m + 24, ctab)?;
    // geometry: a copy of the template shape's geometry (part + vertices), textured with the new image
    let (n, tab) = (u32le(&geo, 0x18)? as usize, u32le(&geo, 0x1C)? as usize);
    let mut entries = Vec::with_capacity(n);
    for k in 0..n {
        entries.push([
            u32le(&geo, tab + 12 * k)?,
            u32le(&geo, tab + 12 * k + 4)?,
            u32le(&geo, tab + 12 * k + 8)?,
        ]);
    }
    let tg = entries
        .iter()
        .find(|e| e[0] as usize == tmpl_shape)
        .ok_or_else(|| err("main_nav: the template shape has no geometry"))?;
    let part = u32le(&geo, tg[2] as usize)? as usize;
    let mut pbytes = slice(&geo, part, 88)?;
    let (ntri, verts) = (u32le(&pbytes, 4)? as usize, u32le(&pbytes, 8)? as usize);
    let vb = slice(&geo, verts, ntri * 3 * 8)?;
    let v_at = append(&mut geo, &vb);
    put(&mut pbytes, 8, v_at)?;
    put(&mut pbytes, 80, img_id)?;
    let p_at = append(&mut geo, &pbytes);
    let plist = append(&mut geo, &p_at.to_le_bytes());
    let mut gt: Vec<u8> = entries
        .iter()
        .flat_map(|e| e.iter().flat_map(|v| v.to_le_bytes()))
        .collect();
    for v in [shape_id, 1, plist] {
        gt.extend(v.to_le_bytes());
    }
    let gtab = append(&mut geo, &gt);
    put(&mut geo, 0x18, n as u32 + 1)?;
    put(&mut geo, 0x1C, gtab)?;
    items[ia].1 = apt;
    items[ig].1 = geo;
    let out = bc2_formats::aptmodel::write_bigf(&items).map_err(|e| err(format!("main_nav.res: {e}")))?;
    write(res_path, &out)?;
    log(&format!(
        "main_nav: sprite {target} frame {frames} \"ONSLAUGHT\" -> shape {shape_id} -> image {img_id} (template shape {tmpl_shape})"
    ));
    Ok(Some(img_id))
}

fn write(path: &Path, data: &[u8]) -> MenuResult<()> {
    bc2_gamefs::atomic::write(path, &[data]).map_err(|e| err(format!("{}: could not write ({e})", path.display())))
}

/// Python's `a[lo:hi]` bounds for one axis of length `n`.
fn py_range(lo: i64, hi: i64, n: usize) -> std::ops::Range<usize> {
    let n = n as i64;
    let fix = |v: i64| if v < 0 { (v + n).max(0) } else { v.min(n) };
    let (a, b) = (fix(lo), fix(hi));
    a as usize..(b.max(a)) as usize
}

/// The last `\t\t\t<item ref="..." />\n` line of the XML (`re.search` with "no `<item ref=` after it"): its end.
fn last_item_end(xml: &str) -> Option<usize> {
    let p = xml.rfind("<item ref=\"")?;
    if p < 3 || &xml[p - 3..p] != "\t\t\t" {
        return None;
    }
    let rest = &xml[p + 11..];
    let q = rest.find('"')?;
    if q == 0 {
        return None;
    }
    let tail = &rest[q..];
    tail.starts_with("\" />\n").then_some(p + 11 + q + 5)
}

/// `re.sub(r'(<field name="F">)[^<]*(<)', r'\g<1>V\g<2>', inst)`.
fn set_field(inst: &str, field: &str, value: &str) -> String {
    let open = format!("<field name=\"{field}\">");
    let mut out = String::with_capacity(inst.len());
    let mut rest = inst;
    while let Some(i) = rest.find(&open) {
        let after = &rest[i + open.len()..];
        match after.find('<') {
            Some(j) => {
                out.push_str(&rest[..i + open.len()]);
                out.push_str(value);
                rest = &after[j..];
            }
            None => {
                out.push_str(&rest[..i + open.len()]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `edit_atlas(texdir, pic, img_id, log)`: the picture's pixels into free space in the menu atlas, a new entry for
/// image `img_id` in its index.
#[expect(clippy::too_many_lines, reason = "onslaught_art.edit_atlas, step by step")]
pub fn edit_atlas(texdir: &Path, pic: &Rgba, img_id: u32, log: Log<'_>) -> MenuResult<()> {
    let res = texdir.join("atlas0.res");
    let mut tex = std::fs::read(&res).map_err(|e| MenuError::io(&res.to_string_lossy(), e))?;
    let (w, h) = (u32le(&tex, 16)? as usize, u32le(&tex, 20)? as usize);
    if (w, h) != (ATLAS, ATLAS) || tex.len() != 92 + w * h {
        return Err(err(format!("unexpected atlas texture {w}x{h}, {} bytes", tex.len())));
    }
    let meta = texdir.join("atlas_meta.dbx");
    let (xml_path, mut xml) = meta_xml(&meta)?;
    let infos = atlas_infos(&xml)?;
    // free space: no pixel and no entry's rectangle within a 4-pixel margin, block-aligned
    let mut occ = vec![false; w * h];
    let (wf, hf) = (w as f64, h as f64);
    for (_, [u0, u1, v0, v1]) in &infos {
        let ys = py_range((py_round(v0 * hf) - 4).max(0), py_round(v1 * hf) + 4, h);
        let xs = py_range((py_round(u0 * wf) - 4).max(0), py_round(u1 * wf) + 4, w);
        for y in ys {
            occ[y * w + xs.start..y * w + xs.end].fill(true);
        }
    }
    let alpha = pil::dxt5_decode(&tex[92..], w, h)?;
    for (o, p) in occ.iter_mut().zip(alpha.px.chunks_exact(4)) {
        *o |= p[3] > 0;
    }
    let (pw, ph) = ((PIC_W + 3) & !3, (PIC_H + 3) & !3);
    let stride = w + 1;
    let mut ii = vec![0i64; (h + 1) * stride];
    for y in 0..h {
        let mut row = 0i64;
        for x in 0..w {
            row += i64::from(occ[y * w + x]);
            ii[(y + 1) * stride + x + 1] = ii[y * stride + x + 1] + row;
        }
    }
    let at = |y: usize, x: usize| ii[y * stride + x];
    let mut spot = None;
    let mut y = (h - ph - 4) as i64;
    'search: while y > 3 {
        let yy = y as usize;
        let mut x = 4;
        while x < w - pw - 4 {
            if at(yy + ph, x + pw) - at(yy, x + pw) - at(yy + ph, x) + at(yy, x) == 0 {
                spot = Some((x, yy));
                break 'search;
            }
            x += 4;
        }
        y -= 4;
    }
    let (x, y) = spot.ok_or_else(|| err("no free space in the menu atlas for the Onslaught picture"))?;
    let mut canvas = Rgba::filled(pw, ph, [0; 4]);
    canvas.paste(pic, 0, 0);
    let bw = pw / 4;
    for (k, blk) in pil::dxt5_encode(&canvas).iter().enumerate() {
        let (by, bx) = (k / bw, k % bw);
        let o = 92 + (((y / 4) + by) * (w / 4) + (x / 4) + bx) * 16;
        tex[o..o + 16].copy_from_slice(blk);
    }
    write(&res, &tex)?;
    // atlas index: a copy of an existing entry with the new image id and rectangle
    let start = xml
        .find("\t<instance guid=\"")
        .into_iter()
        .chain(xml.match_indices("\t<instance guid=\"").map(|(i, _)| i))
        .find(|&i| {
            let rest = &xml[i + 17..];
            rest.find('"').is_some_and(|q| {
                q > 0
                    && rest[..q]
                        .chars()
                        .all(|c| c.is_ascii_digit() || ('A'..='F').contains(&c) || c == '-')
                    && rest[q..].starts_with("\" type=\"UI.UITextureAtlasInfo\">")
            })
        })
        .ok_or_else(|| err(format!("{}: no UITextureAtlasInfo to copy", xml_path.display())))?;
    let rel_end = xml[start..]
        .find("</instance>\n")
        .ok_or_else(|| err(format!("{}: an instance without its end", xml_path.display())))?;
    let inst_old = &xml[start..start + rel_end + "</instance>\n".len()];
    let old_guid = {
        let r = &inst_old[17..];
        &r[..r.find('"').unwrap_or(0)]
    };
    let guid = bc2_formats::guid::new_guid("opfor/main_nav/onslaught_mode", &[]);
    let mut inst = inst_old.replace(old_guid, &guid);
    let fl = |v: f64| bc2_pyrt::fmt::float_repr(v);
    for (field, value) in [
        ("TextureFile", img_id.to_string()),
        ("MinU", fl(x as f64 / wf)),
        ("MaxU", fl((x + PIC_W) as f64 / wf)),
        ("MinV", fl(y as f64 / hf)),
        ("MaxV", fl((y + PIC_H) as f64 / hf)),
    ] {
        inst = set_field(&inst, field, &value);
    }
    let end = last_item_end(&xml).ok_or_else(|| err(format!("{}: no atlas item list", xml_path.display())))?;
    xml = format!(
        "{}\t\t\t<item ref=\"{}\" />\n{}",
        &xml[..end],
        guid.to_lowercase(),
        &xml[end..]
    );
    xml = xml.replace("</partition>", &format!("{inst}</partition>"));
    // written in text mode (\r\n), as the Python wrote it
    write(&xml_path, xml.replace('\n', "\r\n").as_bytes())?;
    let (xp, mp) = (xml_path.to_string_lossy(), meta.to_string_lossy());
    dbxedit(&["import", &xp, &mp, "--like", &mp])?;
    std::fs::remove_file(&xml_path).map_err(|e| MenuError::io(&xp, e))?;
    log(&format!(
        "atlas: Onslaught picture at ({x}, {y}) {PIC_W}x{PIC_H}, index entry for image {img_id}"
    ));
    Ok(())
}

/// Where the picture comes from.
pub struct Options {
    /// The unpacked main menu bundle.
    pub unpacked: PathBuf,
    /// The folder searched for the Xbox 360 Update #3 package.
    pub xbox: PathBuf,
    /// Read the Xbox 360 files again even with a cached picture.
    pub refresh: bool,
    /// The cached picture (`<OPFOR>\work\import\onslaught_mode.png`).
    pub cache: PathBuf,
}

/// `main(argv)`: the picture found or made, the movie and atlas edited.
pub fn run(o: &Options, log: Log<'_>) -> MenuResult<()> {
    let menu = o.unpacked.join("Menu");
    let texdir = menu.join("main_nav_textures_Win32");
    let mut pic = None;
    let cache = o.cache.to_string_lossy();
    if o.cache.is_file() && !o.refresh {
        pic = Some(pil::png_read(&cache)?);
        log(&format!("Onslaught picture from the cache ({cache})"));
    } else if o.xbox.is_dir() {
        let raw = match from_xbox(&o.xbox, log) {
            Ok(r) => r,
            Err(e) => {
                log(&format!(
                    "reading the Xbox 360 files failed ({e}) - using the fallback picture"
                ));
                None
            }
        };
        if let Some(raw) = raw {
            let p = fit(&cutout(&raw));
            if let Some(d) = o.cache.parent() {
                std::fs::create_dir_all(d).map_err(|e| MenuError::io(&d.to_string_lossy(), e))?;
            }
            pil::png_write(&cache, &p)?;
            pic = Some(p);
        }
    }
    let pic = match pic {
        Some(p) => p,
        None => {
            let tex = std::fs::read(texdir.join("atlas0.res")).map_err(|e| MenuError::io("atlas0.res", e))?;
            let (xml_path, xml) = meta_xml(&texdir.join("atlas_meta.dbx"))?;
            let infos = atlas_infos(&xml)?;
            std::fs::remove_file(&xml_path).map_err(|e| MenuError::io(&xml_path.to_string_lossy(), e))?;
            let body = tex.get(92..).ok_or_else(|| err("atlas0.res is cut short"))?;
            composed(&pil::dxt5_decode(body, ATLAS, ATLAS)?, &infos, log)
        }
    };
    let Some(img) = edit_menu(&menu.join("main_nav.res"), log)? else {
        return Ok(());
    };
    edit_atlas(&texdir, &pic, img, log)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn item_list_end() {
        let x = "a\n\t\t\t<item ref=\"x/1\" />\n\t\t\t<item ref=\"x/2\" />\n</array>\n";
        let e = last_item_end(x).unwrap();
        assert_eq!(&x[e..], "</array>\n");
        assert_eq!(last_item_end("<item ref=\"x\" />\n"), None);
        assert_eq!(last_item_end("\t\t\t<item ref=\"x\"/>\n"), None);
    }

    #[test]
    fn fields_and_slices() {
        let i = "<field name=\"MinU\">0.5</field><field name=\"MinU\">1</field>";
        assert_eq!(
            set_field(i, "MinU", "0.25"),
            "<field name=\"MinU\">0.25</field><field name=\"MinU\">0.25</field>"
        );
        assert_eq!(py_range(-4, 10, 8), 4..8);
        assert_eq!(py_range(2, 20, 8), 2..8);
        assert_eq!(py_round(2.5), 2);
        assert_eq!(py_round(3.5), 4);
    }

    #[test]
    fn infos() {
        let x = "<f name=\"TextureFile\">7<x><f name=\"MinU\">0.5<f name=\"MaxU\">1E-05<f name=\"MinV\">0<f name=\"MaxV\">-1<\n\
                 <f name=\"TextureFile\">7<x><f name=\"MinU\">0.25<f name=\"MaxU\">1<f name=\"MinV\">0<f name=\"MaxV\">1<";
        let v = atlas_infos(x).unwrap();
        assert_eq!(v, vec![(7, [0.25, 1.0, 0.0, 1.0])]);
    }
}
