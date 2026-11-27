//! Round `OR4` ("OPFOR in Rust", stage 4: the pictures): Project OPFOR's picture tools' NumPy and Pillow work
//! (`bc2_convert::pictures`) - the loadout pictures' renderer, the loading and map pictures, the weapon icons'
//! silhouettes and the minimap plane, Pillow's resize. Pictures cross as `bytes` of `uint8` (row-major, channels
//! last), the weapon parts as `bytes` of `float64` / `int64` (the caller's `np.ascontiguousarray(..).tobytes()`).
//! Errors raise what the Python raised: `SystemExit` with its message, `IndexError` / `ValueError`.

#![allow(
    clippy::type_complexity,
    clippy::manual_is_multiple_of,
    clippy::chunks_exact_to_as_chunks,
    clippy::indexing_slicing,
    reason = "the Python's shapes; chunks of a fixed size indexed inside their size"
)]

use bc2_convert::pictures::{FPic, Pic, icons, loading, resample, weapon};
use pyo3::exceptions::{PyIndexError, PySystemExit, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

fn pic(px: &[u8], w: usize, h: usize, ch: usize) -> PyResult<Pic> {
    Pic::of(w, h, ch, px.to_vec()).ok_or_else(|| {
        PyValueError::new_err(format!(
            "{} bytes are not a {w} x {h} picture of {ch} channels",
            px.len()
        ))
    })
}

fn f64s(b: &[u8]) -> PyResult<Vec<f64>> {
    if b.len() % 8 != 0 {
        return Err(PyValueError::new_err("float64 bytes of a length not a multiple of 8"));
    }
    Ok(b.chunks_exact(8)
        .map(|c| f64::from_le_bytes(c.try_into().unwrap_or_default()))
        .collect())
}

fn triples(b: &[u8]) -> PyResult<Vec<[f64; 3]>> {
    let v = f64s(b)?;
    if v.len() % 3 != 0 {
        return Err(PyValueError::new_err("rows of 3 floats expected"));
    }
    Ok(v.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect())
}

/// One part as the Python wrapper gives it: (positions, normals or None, uvs or None, (h, w, ch, image) or None,
/// triangles) - `float64` bytes, the image `float64` too, the triangles `int64` rows of 3.
type PyPart = (
    Vec<u8>,
    Option<Vec<u8>>,
    Option<Vec<u8>>,
    Option<(usize, usize, usize, Vec<u8>)>,
    Vec<u8>,
);

fn parts(items: Vec<PyPart>) -> PyResult<Vec<weapon::Part>> {
    items
        .into_iter()
        .map(|(pos, nrm, uv, img, tris)| {
            let uvs = match uv {
                Some(b) => {
                    let v = f64s(&b)?;
                    Some(v.chunks_exact(2).map(|c| [c[0], c[1]]).collect())
                }
                None => None,
            };
            let image = match img {
                Some((h, w, ch, b)) => {
                    let v = f64s(&b)?;
                    if v.len() != h * w * ch || ch < 3 {
                        return Err(PyValueError::new_err("an image of another size than its shape"));
                    }
                    Some(FPic { w, h, ch, v })
                }
                None => None,
            };
            if tris.len() % 24 != 0 {
                return Err(PyValueError::new_err("triangles: int64 rows of 3 expected"));
            }
            let t = tris
                .chunks_exact(24)
                .map(|c| {
                    std::array::from_fn(|k| {
                        i64::from_le_bytes(
                            c.get(8 * k..8 * k + 8)
                                .and_then(|s| s.try_into().ok())
                                .unwrap_or_default(),
                        )
                    })
                })
                .collect();
            Ok(weapon::Part {
                positions: triples(&pos)?,
                normals: nrm.map(|b| triples(&b)).transpose()?,
                uvs,
                image,
                tris: t,
            })
        })
        .collect()
}

fn picture_err(e: &weapon::PictureError) -> PyErr {
    if e.0.starts_with("index ") {
        PyIndexError::new_err(e.0.clone())
    } else {
        PyValueError::new_err(e.0.clone())
    }
}

/// `pictures.render(parts, w, h, box, margin, ss)`: `(h, w, 4)` bytes.
#[pyfunction]
#[expect(clippy::too_many_arguments, reason = "render's own arguments")]
pub fn pictures_weapon_render(
    py: Python<'_>,
    items: Vec<PyPart>,
    w: usize,
    h: usize,
    lo: [f64; 2],
    hi: [f64; 2],
    margin: [f64; 2],
    ss: usize,
) -> PyResult<Bound<'_, PyBytes>> {
    let ps = parts(items)?;
    let out = py
        .detach(|| weapon::render(&ps, w, h, lo, hi, margin, ss))
        .map_err(|e| picture_err(&e))?;
    Ok(PyBytes::new(py, &out.px))
}

/// `pictures.framed(parts, w, h)`: `(h, w, 4)` bytes.
#[pyfunction]
pub fn pictures_weapon_framed(py: Python<'_>, items: Vec<PyPart>, w: usize, h: usize) -> PyResult<Bound<'_, PyBytes>> {
    let ps = parts(items)?;
    let out = py.detach(|| weapon::framed(&ps, w, h)).map_err(|e| picture_err(&e))?;
    Ok(PyBytes::new(py, &out.px))
}

/// `pictures.down(img, w, h)` of an `(ih, iw, 4)` picture: `(h, w, 4)` bytes.
#[pyfunction]
pub fn pictures_down<'py>(
    py: Python<'py>,
    px: &[u8],
    iw: usize,
    ih: usize,
    w: usize,
    h: usize,
) -> PyResult<Bound<'py, PyBytes>> {
    let img = pic(px, iw, ih, 4)?;
    let out = py.detach(|| weapon::down(&img, w, h)).map_err(|e| picture_err(&e))?;
    Ok(PyBytes::new(py, &out.px))
}

/// Pillow's `Image.fromarray(a).resize((w, h), filter)` of an `(ih, iw, ch)` `uint8` picture (ch 1, 3 or 4; filter
/// `'lanczos'` or `'box'`): `(h, w, ch)` bytes.
#[pyfunction]
#[expect(clippy::too_many_arguments, reason = "the picture's shape and the new size")]
pub fn pictures_resize<'py>(
    py: Python<'py>,
    px: &[u8],
    iw: usize,
    ih: usize,
    ch: usize,
    w: usize,
    h: usize,
    filter: &str,
) -> PyResult<Bound<'py, PyBytes>> {
    let f = match filter {
        "lanczos" => resample::Filter::Lanczos,
        "box" => resample::Filter::Box,
        _ => return Err(PyValueError::new_err(format!("no filter {filter}"))),
    };
    if !matches!(ch, 1 | 3 | 4) {
        return Err(PyValueError::new_err(format!("{ch} channels: L, RGB or RGBA only")));
    }
    let img = pic(px, iw, ih, ch)?;
    let out = py.detach(|| resample::resize(&img, w, h, f));
    Ok(PyBytes::new(py, &out.px))
}

/// `ww2_loading.picture`'s pixels: BF1943's loading picture (its Xbox resource) as `(720, 1280, 3)` bytes.
#[pyfunction]
pub fn pictures_loading_picture<'py>(py: Python<'py>, xbox: &[u8], name: &str) -> PyResult<Bound<'py, PyBytes>> {
    let out = py
        .detach(|| loading::picture(xbox, name))
        .map_err(PySystemExit::new_err)?;
    Ok(PyBytes::new(py, &out.px))
}

/// `ww2_loading.level_picture(rgb)`: the match selector's `(256, 512, 4)` picture of a `(720, 1280, 3)` one.
#[pyfunction]
pub fn pictures_level_picture<'py>(py: Python<'py>, rgb: &[u8]) -> PyResult<Bound<'py, PyBytes>> {
    let img = pic(rgb, loading::W, loading::H, 3)?;
    let out = py.detach(|| loading::level_picture(&img));
    Ok(PyBytes::new(py, &out.px))
}

/// `ww2_loading.check(res, rgb)`: `(wrapped, psnr)`.
#[pyfunction]
pub fn pictures_loading_check<'py>(py: Python<'py>, res: &[u8], rgb: &[u8]) -> PyResult<(Bound<'py, PyBytes>, f64)> {
    let img = pic(rgb, loading::W, loading::H, 3)?;
    let (w, p) = py.detach(|| loading::check(res, &img)).map_err(PySystemExit::new_err)?;
    Ok((PyBytes::new(py, &w), p))
}

/// `ww2_loading.level_check(res, rgba)`: `(wrapped, psnr)`.
#[pyfunction]
pub fn pictures_level_check<'py>(py: Python<'py>, res: &[u8], rgba: &[u8]) -> PyResult<(Bound<'py, PyBytes>, f64)> {
    let img = pic(rgba, loading::LW, loading::LH, 4)?;
    let (w, p) = py
        .detach(|| loading::level_check(res, &img))
        .map_err(PySystemExit::new_err)?;
    Ok((PyBytes::new(py, &w), p))
}

/// `weapon_icons.silhouette` of a size1 loadout picture's resource (its bytes): `(h, w, rgba bytes)`.
#[pyfunction]
#[pyo3(signature = (res, width, max_h=None))]
pub fn pictures_silhouette<'py>(
    py: Python<'py>,
    res: &[u8],
    width: usize,
    max_h: Option<usize>,
) -> PyResult<(usize, usize, Bound<'py, PyBytes>)> {
    let out = py
        .detach(|| {
            let (cover, lum) = icons::picture(res)?;
            icons::silhouette(&cover, &lum, width, max_h)
        })
        .map_err(PyValueError::new_err)?;
    Ok((out.h, out.w, PyBytes::new(py, &out.px)))
}

/// `weapon_icons.fit(pic, w, h)` of an `(ph, pw, 4)` picture: `(h, w, 4)` bytes.
#[pyfunction]
pub fn pictures_fit<'py>(
    py: Python<'py>,
    px: &[u8],
    pw: usize,
    ph: usize,
    w: usize,
    h: usize,
) -> PyResult<Bound<'py, PyBytes>> {
    let img = pic(px, pw, ph, 4)?;
    let out = py.detach(|| icons::fit(&img, w, h));
    Ok(PyBytes::new(py, &out.px))
}

/// `minimap_icons.box_colours(box)` of an `(h, w, 4)` box: `(fill, outline)`.
#[pyfunction]
pub fn pictures_box_colours(px: &[u8], w: usize, h: usize) -> PyResult<([u8; 3], [u8; 3])> {
    let bx = pic(px, w, h, 4)?;
    icons::box_colours(&bx).ok_or_else(|| PyValueError::new_err("attempt to get argmin of an empty sequence"))
}

/// `minimap_icons.paint(box, plane)` of two `(h, w, 4)` pictures: `(h, w, 4)` bytes.
#[pyfunction]
pub fn pictures_paint<'py>(
    py: Python<'py>,
    bx: &[u8],
    plane: &[u8],
    w: usize,
    h: usize,
) -> PyResult<Bound<'py, PyBytes>> {
    let b = pic(bx, w, h, 4)?;
    let p = Pic::of(w, h, 4, plane.to_vec()).ok_or_else(|| PySystemExit::new_err("plane vs box: other sizes"))?;
    let out = icons::paint(&b, &p).map_err(PySystemExit::new_err)?;
    Ok(PyBytes::new(py, &out.px))
}

/// One part on a preview page: (name, triangles, texture name or None, positions `<f4`, uvs `<f4` or None, indices
/// `<u4`, (w, h, channels, pixels) or None).
type PagePartArgs = (
    String,
    usize,
    Option<String>,
    Vec<u8>,
    Option<Vec<u8>>,
    Vec<u8>,
    Option<(u32, u32, u8, Vec<u8>)>,
);

/// `meshview.page(laid, title)` (retired in round OR4): the preview page's HTML of `[(model name, offset, parts)]`.
#[pyfunction]
pub fn meshview_page(
    py: Python<'_>,
    models: Vec<(String, [f64; 3], Vec<PagePartArgs>)>,
    title: &str,
) -> PyResult<String> {
    use bc2_convert::meshview::page::{PageModel, PagePart, page};
    let ms: Vec<PageModel> = models
        .into_iter()
        .map(|(name, offset, parts)| PageModel {
            name,
            offset,
            parts: parts
                .into_iter()
                .map(|(name, tris, texname, pos, uv, idx, image)| PagePart {
                    name,
                    tris,
                    texname,
                    pos,
                    uv,
                    idx,
                    image,
                })
                .collect(),
        })
        .collect();
    py.detach(|| page(&ms, title))
        .ok_or_else(|| PyValueError::new_err("a picture whose pixels are not its shape"))
}
