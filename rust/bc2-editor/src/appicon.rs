//! The window's icon (round I1): the title bar, the taskbar while the editor runs and Alt+Tab. Read at start-up
//! from the suite's application icon, found as the build scripts find it for the .exe's own icon (`rust/app_icon.rs`)
//! and the hub finds it (`bfbc2core.appicon`): `BFBC2_APP_ICON`, then `design\icons\local\app.ico` (a personal
//! override, never committed), then `design\icons\app.ico` (the committed icon). No icon is no error: the window has
//! none.

use std::path::{Path, PathBuf};

use eframe::egui;

use crate::levels::suite_root;

/// The environment variable naming the icon file.
pub const ENV: &str = "BFBC2_APP_ICON";

/// The icon files tried under a suite root, in order: the user's own (gitignored), then the committed default.
#[must_use]
pub fn under(root: &Path) -> [PathBuf; 2] {
    let icons = root.join("design").join("icons");
    [icons.join("local").join("app.ico"), icons.join("app.ico")]
}

/// Every path tried, in order: `BFBC2_APP_ICON` (when set), then [`under`] the suite this editor was built in, then
/// under the folder above the editor's own (`<suite>\bin\bfbc2-editor.exe`) when that is another folder.
#[must_use]
pub fn candidates() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::env::var_os(ENV)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .into_iter()
        .collect();
    let root = suite_root();
    out.extend(under(&root));
    if let Some(up) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().and_then(Path::parent).map(Path::to_path_buf))
        .filter(|up| *up != root)
    {
        out.extend(under(&up));
    }
    out
}

/// The first icon file of [`candidates`] that exists.
#[must_use]
pub fn find() -> Option<PathBuf> {
    candidates().into_iter().find(|p| p.is_file())
}

/// The window icon: the largest picture of the found .ico, or None (no icon file, or one that does not decode).
#[must_use]
pub fn load() -> Option<egui::IconData> {
    decode_ico(&std::fs::read(find()?).ok()?)
}

/// The largest PNG-compressed picture of an .ico file as RGBA (the form current tools write every size in; an
/// .ico of only uncompressed bitmaps gives None).
#[must_use]
pub fn decode_ico(data: &[u8]) -> Option<egui::IconData> {
    let u16_at = |o: usize| Some(u16::from_le_bytes(data.get(o..o + 2)?.try_into().ok()?));
    let u32_at = |o: usize| Some(u32::from_le_bytes(data.get(o..o + 4)?.try_into().ok()?));
    if u16_at(0)? != 0 || u16_at(2)? != 1 {
        return None;
    }
    let mut best: Option<egui::IconData> = None;
    for i in 0..usize::from(u16_at(4)?) {
        let entry = 6 + 16 * i;
        let size = u32_at(entry + 8)? as usize;
        let offset = u32_at(entry + 12)? as usize;
        let Some(image) = data.get(offset..offset.checked_add(size)?) else {
            continue;
        };
        if let Some(icon) = decode_png(image)
            && best.as_ref().is_none_or(|b| icon.width > b.width)
        {
            best = Some(icon);
        }
    }
    best
}

/// A PNG as 8-bit RGBA.
fn decode_png(bytes: &[u8]) -> Option<egui::IconData> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return None;
    }
    let mut d = png::Decoder::new(std::io::Cursor::new(bytes));
    d.set_transformations(png::Transformations::normalize_to_color8() | png::Transformations::ALPHA);
    let mut r = d.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()?];
    let info = r.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::GrayscaleAlpha => buf.as_chunks::<2>().0.iter().flat_map(|&[g, a]| [g, g, g, a]).collect(),
        _ => return None,
    };
    Some(egui::IconData {
        rgba,
        width: info.width,
        height: info.height,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, reason = "tests")]
mod tests {
    use super::*;

    /// A PNG of `w` x `w` pixels, every one `px`.
    fn png_of(w: u32, px: [u8; 4]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut e = png::Encoder::new(&mut out, w, w);
            e.set_color(png::ColorType::Rgba);
            e.set_depth(png::BitDepth::Eight);
            let mut wr = e.write_header().unwrap();
            wr.write_image_data(&px.repeat((w * w) as usize)).unwrap();
        }
        out
    }

    /// An .ico of the given PNG pictures.
    fn ico_of(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
        let mut out: Vec<u8> = vec![0, 0, 1, 0];
        out.extend((images.len() as u16).to_le_bytes());
        let mut offset = 6 + 16 * images.len();
        let mut body: Vec<u8> = Vec::new();
        for (w, png) in images {
            let side = if *w >= 256 { 0 } else { *w as u8 };
            out.extend([side, side, 0, 0]);
            out.extend(1u16.to_le_bytes());
            out.extend(32u16.to_le_bytes());
            out.extend((png.len() as u32).to_le_bytes());
            out.extend((offset as u32).to_le_bytes());
            offset += png.len();
            body.extend(png);
        }
        out.extend(body);
        out
    }

    #[test]
    fn the_largest_picture_is_the_window_icon() {
        let ico = ico_of(&[
            (16, png_of(16, [1, 2, 3, 4])),
            (48, png_of(48, [9, 8, 7, 255])),
            (32, png_of(32, [0; 4])),
        ]);
        let icon = decode_ico(&ico).unwrap();
        assert_eq!((icon.width, icon.height), (48, 48));
        assert_eq!(icon.rgba.len(), 48 * 48 * 4);
        assert_eq!(&icon.rgba[..4], &[9, 8, 7, 255]);
    }

    #[test]
    fn not_an_icon_is_no_icon() {
        assert!(decode_ico(b"").is_none());
        assert!(decode_ico(b"not an icon at all").is_none());
        // a directory entry pointing past the end of the file
        let mut ico = ico_of(&[(16, png_of(16, [0; 4]))]);
        ico.truncate(30);
        assert!(decode_ico(&ico).is_none());
    }

    #[test]
    fn the_lookup_order() {
        let root = Path::new("suite");
        let [local, committed] = under(root);
        assert_eq!(local, root.join("design").join("icons").join("local").join("app.ico"));
        assert_eq!(committed, root.join("design").join("icons").join("app.ico"));
        let c = candidates();
        let at = |p: &Path| c.iter().position(|q| q == p).unwrap();
        let [local, committed] = under(&suite_root());
        assert!(at(&local) < at(&committed));
    }
}
