//! The laid-out models as a page for a browser (three.js): `bfbc2-mesh-viewer/meshview.py`'s `page()`, which Project
//! OPFOR's WW2 tools write their preview pages with (round OR4 retired the Python). The page is the Python's but for
//! the textures' PNG files: the same pixels, encoded by the `png` crate instead of Pillow (spec rule XWW2-PIC-4).

#![allow(
    clippy::indexing_slicing,
    clippy::needless_raw_string_hashes,
    reason = "a chunk of three or fewer bytes indexed at 0 and a 6-bit table index; the page's template kept as the Python's text"
)]

use std::fmt::Write as _;

/// A part of a model on the page: its name, triangle count, colour texture's name, the arrays three.js reads (as
/// the caller made them: `float32` positions and uvs, `uint32` indices) and its colour image.
#[derive(Debug, Clone, Default)]
pub struct PagePart {
    /// The part's name.
    pub name: String,
    /// Its triangle count (`len(indices)`).
    pub tris: usize,
    /// Its colour texture's name, if any.
    pub texname: Option<String>,
    /// Positions (`<f4` bytes).
    pub pos: Vec<u8>,
    /// Uvs (`<f4` bytes), if any.
    pub uv: Option<Vec<u8>>,
    /// Indices (`<u4` bytes).
    pub idx: Vec<u8>,
    /// The colour image: (width, height, channels 1 - 4, `uint8` pixels), if any.
    pub image: Option<(u32, u32, u8, Vec<u8>)>,
}

/// A model on the page: its name (the last part of its path is shown), its offset, its parts.
#[derive(Debug, Clone, Default)]
pub struct PageModel {
    /// The model's name.
    pub name: String,
    /// Where it is placed.
    pub offset: [f64; 3],
    /// Its parts.
    pub parts: Vec<PagePart>,
}

const HEAD: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>"#;
const STYLE: &str = r#"</title>
<style>
:root { --bg: #1d1f22; --panel: #2a2d31; --text: #e6e6e6; --muted: #9aa0a6; }
html, body { margin: 0; height: 100%; background: var(--bg); color: var(--text); font: 13px system-ui, sans-serif; }
#panel { position: absolute; top: 8px; left: 8px; max-height: calc(100% - 16px); overflow: auto; background: var(--panel);
         padding: 8px 10px; border-radius: 6px; max-width: 420px; }
#panel h2 { font-size: 13px; margin: 8px 0 4px; } #panel label { display: block; white-space: nowrap; }
#panel small { color: var(--muted); } canvas { display: block; }
</style>
<script type="importmap">{"imports": {"three": "https://cdn.jsdelivr.net/npm/three@0.160.0/build/three.module.js",
"three/addons/": "https://cdn.jsdelivr.net/npm/three@0.160.0/examples/jsm/"}}</script>
</head><body><div id="panel"><b>"#;
const PANEL: &str = r#"</b><br><small>drag: orbit - right drag: pan - wheel: zoom</small>
<label><input type="checkbox" id="wire"> wireframe</label><div id="parts"></div></div>
<script type="module">
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
const MODELS = "#;
const TAIL: &str = r#";
const b64 = s => Uint8Array.from(atob(s), c => c.charCodeAt(0)).buffer;
const renderer = new THREE.WebGLRenderer({ antialias: true });
renderer.setPixelRatio(devicePixelRatio); renderer.setSize(innerWidth, innerHeight);
document.body.appendChild(renderer.domElement);
const scene = new THREE.Scene(); scene.background = new THREE.Color(0x1d1f22);
scene.add(new THREE.HemisphereLight(0xffffff, 0x404040, 1.2));
const sun = new THREE.DirectionalLight(0xffffff, 1.6); sun.position.set(2, 3, 2); scene.add(sun);
const camera = new THREE.PerspectiveCamera(40, innerWidth / innerHeight, 0.001, 1000);
const controls = new OrbitControls(camera, renderer.domElement);
const box = new THREE.Box3(), mats = [], list = document.getElementById('parts');
for (const m of MODELS) {
  const h = document.createElement('h2'); h.textContent = m.name; list.appendChild(h);
  for (const p of m.parts) {
    const g = new THREE.BufferGeometry();
    g.setAttribute('position', new THREE.BufferAttribute(new Float32Array(b64(p.pos)), 3));
    if (p.uv) g.setAttribute('uv', new THREE.BufferAttribute(new Float32Array(b64(p.uv)), 2));
    g.setIndex(new THREE.BufferAttribute(new Uint32Array(b64(p.idx)), 1));
    g.computeVertexNormals();
    const mat = new THREE.MeshStandardMaterial({ color: p.tex ? 0xffffff : 0xaaaaaa, roughness: 0.7, side: THREE.DoubleSide });
    if (p.tex) { const t = new THREE.TextureLoader().load('data:image/png;base64,' + p.tex); t.flipY = false;
                 t.wrapS = t.wrapT = THREE.RepeatWrapping; t.colorSpace = THREE.SRGBColorSpace; mat.map = t; }
    mats.push(mat);
    const mesh = new THREE.Mesh(g, mat); mesh.position.set(...m.offset); scene.add(mesh); box.expandByObject(mesh);
    const l = document.createElement('label'), c = document.createElement('input');
    c.type = 'checkbox'; c.checked = true; c.onchange = () => { mesh.visible = c.checked; };
    l.append(c, ' ' + p.name + ' '); const s = document.createElement('small');
    s.textContent = `${p.tris} tris` + (p.texname ? ' - ' + p.texname.split('/').pop() : ''); l.append(s); list.appendChild(l);
  }
}
const c = box.getCenter(new THREE.Vector3()), r = box.getSize(new THREE.Vector3()).length();
camera.position.set(c.x, c.y + r * 0.2, c.z + r * 1.1); controls.target.copy(c); controls.update();
document.getElementById('wire').onchange = e => mats.forEach(m => { m.wireframe = e.target.checked; });
addEventListener('resize', () => { camera.aspect = innerWidth / innerHeight; camera.updateProjectionMatrix();
  renderer.setSize(innerWidth, innerHeight); });
renderer.setAnimationLoop(() => { controls.update(); renderer.render(scene, camera); });
</script></body></html>
"#;

/// Base64 (standard alphabet, padded), as `base64.b64encode`.
#[must_use]
pub fn b64(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(c.get(1).copied().unwrap_or(0)) << 8)
            | u32::from(c.get(2).copied().unwrap_or(0));
        let k = |s: u32| char::from(A[((n >> s) & 63) as usize]);
        out.push(k(18));
        out.push(k(12));
        out.push(if c.len() > 1 { k(6) } else { '=' });
        out.push(if c.len() > 2 { k(0) } else { '=' });
    }
    out
}

/// `html.escape(s)`.
#[must_use]
pub fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            c => out.push(c),
        }
    }
    out
}

/// A JSON string as `json.dumps` writes it (`ensure_ascii`: every character past ASCII as `\uXXXX`).
fn json_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 || (c as u32) > 0x7e => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    let _ = write!(out, "\\u{u:04x}");
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Python's `repr` of a float (`json.dumps`' numbers): the shortest digits that read back, `1.0` for a whole
/// number, exponents from 1e16 and under 1e-4 (`1e-05`, `1.5e+16`).
#[must_use]
pub fn py_float(v: f64) -> String {
    if v.is_nan() {
        return "NaN".to_owned();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }
    if v == 0.0 {
        return if v.is_sign_negative() { "-0.0" } else { "0.0" }.to_owned();
    }
    let e = format!("{v:e}");
    let (mant, exp) = e.split_once('e').unwrap_or((&e, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let neg = mant.starts_with('-');
    let digits: String = mant.chars().filter(char::is_ascii_digit).collect();
    let sign = if neg { "-" } else { "" };
    if (-4..16).contains(&exp) {
        let n = digits.len() as i32;
        let body = if exp >= n - 1 {
            format!("{digits}{}.0", "0".repeat((exp - (n - 1)) as usize))
        } else if exp >= 0 {
            let (a, b) = digits.split_at((exp + 1) as usize);
            format!("{a}.{b}")
        } else {
            format!("0.{}{digits}", "0".repeat((-exp - 1) as usize))
        };
        format!("{sign}{body}")
    } else {
        let (a, b) = digits.split_at(1);
        let m = if b.is_empty() { a.to_owned() } else { format!("{a}.{b}") };
        format!("{sign}{m}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs())
    }
}

/// A picture as a PNG file (8 bits; grey, grey with alpha, RGB or RGBA by its channels).
fn png_of(w: u32, h: u32, ch: u8, px: &[u8]) -> Option<Vec<u8>> {
    let colour = match ch {
        1 => png::ColorType::Grayscale,
        2 => png::ColorType::GrayscaleAlpha,
        3 => png::ColorType::Rgb,
        4 => png::ColorType::Rgba,
        _ => return None,
    };
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(colour);
        e.set_depth(png::BitDepth::Eight);
        e.set_compression(png::Compression::High);
        let mut wr = e.write_header().ok()?;
        wr.write_image_data(px).ok()?;
        wr.finish().ok()?;
    }
    Some(out)
}

/// `page(laid, title)`: the page's HTML. `None` when an image's pixels are not its shape.
#[must_use]
pub fn page(models: &[PageModel], title: &str) -> Option<String> {
    let mut js = String::from("[");
    for (mi, m) in models.iter().enumerate() {
        if mi > 0 {
            js.push_str(", ");
        }
        js.push_str("{\"name\": ");
        json_str(m.name.rsplit('/').next().unwrap_or(&m.name), &mut js);
        let off: Vec<String> = m.offset.iter().map(|v| py_float(*v)).collect();
        let _ = write!(js, ", \"offset\": [{}], \"parts\": [", off.join(", "));
        for (pi, p) in m.parts.iter().enumerate() {
            if pi > 0 {
                js.push_str(", ");
            }
            js.push_str("{\"name\": ");
            json_str(&p.name, &mut js);
            let _ = write!(js, ", \"tris\": {}, \"texname\": ", p.tris);
            match &p.texname {
                Some(t) => json_str(t, &mut js),
                None => js.push_str("null"),
            }
            let _ = write!(js, ", \"pos\": \"{}\", \"uv\": ", b64(&p.pos));
            match &p.uv {
                Some(u) => {
                    let _ = write!(js, "\"{}\"", b64(u));
                }
                None => js.push_str("null"),
            }
            let _ = write!(js, ", \"idx\": \"{}\", \"tex\": ", b64(&p.idx));
            match &p.image {
                Some((w, h, ch, px)) => {
                    if px.len() != (*w as usize) * (*h as usize) * usize::from(*ch) {
                        return None;
                    }
                    let _ = write!(js, "\"{}\"", b64(&png_of(*w, *h, *ch, px)?));
                }
                None => js.push_str("null"),
            }
            js.push('}');
        }
        js.push_str("]}");
    }
    js.push(']');
    let js = js.replace("</", "<\\/");
    let t = html_escape(title);
    Some(format!("{HEAD}{t}{STYLE}{t}{PANEL}{js}{TAIL}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_and_strings_as_python() {
        assert_eq!(py_float(1.0), "1.0");
        assert_eq!(py_float(-0.5), "-0.5");
        assert_eq!(py_float(0.0001), "0.0001");
        assert_eq!(py_float(0.00001), "1e-05");
        assert_eq!(py_float(1.5e16), "1.5e+16");
        assert_eq!(py_float(123_456.789), "123456.789");
        assert_eq!(py_float(f64::from(0.1f32)), "0.10000000149011612");
        assert_eq!(b64(b"ab"), "YWI=");
        assert_eq!(html_escape("a<b>&\"'"), "a&lt;b&gt;&amp;&quot;&#x27;");
        let mut s = String::new();
        json_str("é\"/", &mut s);
        assert_eq!(s, "\"\\u00e9\\\"/\"");
    }

    #[test]
    fn a_page_holds_its_models() {
        let m = PageModel {
            name: "objects/a/thing".into(),
            offset: [0.0, 1.5, -2.0],
            parts: vec![PagePart {
                name: "main".into(),
                tris: 1,
                pos: vec![0; 36],
                idx: vec![0; 12],
                image: Some((1, 1, 3, vec![1, 2, 3])),
                ..PagePart::default()
            }],
        };
        let p = page(&[m], "A <test>").unwrap_or_default();
        assert!(p.contains("<title>A &lt;test&gt;</title>"));
        assert!(p.contains("const MODELS = [{\"name\": \"thing\", \"offset\": [0.0, 1.5, -2.0], \"parts\": [{\"name\": \"main\", \"tris\": 1, \"texname\": null"));
        assert!(p.contains("\"tex\": \"iVBOR"));
    }
}
