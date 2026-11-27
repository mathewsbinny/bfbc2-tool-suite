//! Round `OR5`: the menu art's picture pieces Project OPFOR's Pacific War menu step (`ww2\menu_install.py`) shares
//! with the Onslaught mode picture (`bc2_menu::modeart`, `bc2_menu::pil`; OPFOR's `onslaught_art.py` retired):
//! `menuart_atlas_infos(xml text)` -> `[(image id, (min u, max u, min v, max v))]`, `menuart_dxt5_decode(blocks, w, h)`
//! -> RGBA bytes (Pillow's DDS decoding), `menuart_dxt5_encode(rgba, w, h)` -> the blocks (`dxt5_encode`).

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

/// An image's rectangle in the atlas: (min u, max u, min v, max v).
type Rect = (f64, f64, f64, f64);

/// `atlas_infos(meta_xml)` of the XML's text.
#[pyfunction]
pub fn menuart_atlas_infos(xml: &str) -> PyResult<Vec<(u32, Rect)>> {
    let v = bc2_menu::modeart::atlas_infos(xml).map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(v.into_iter().map(|(id, [a, b, c, d])| (id, (a, b, c, d))).collect())
}

/// `dds_decode(data, w, h)`: the RGBA pixels, rows top to bottom.
#[pyfunction]
pub fn menuart_dxt5_decode<'py>(py: Python<'py>, data: &[u8], w: usize, h: usize) -> PyResult<Bound<'py, PyBytes>> {
    let img = py
        .detach(|| bc2_menu::pil::dxt5_decode(data, w, h))
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(PyBytes::new(py, &img.px))
}

/// `dxt5_encode(rgba)` of `w` x `h` RGBA pixels (multiples of 4): the blocks, 16 bytes each, row-major.
#[pyfunction]
pub fn menuart_dxt5_encode<'py>(py: Python<'py>, rgba: &[u8], w: usize, h: usize) -> PyResult<Bound<'py, PyBytes>> {
    if !w.is_multiple_of(4) || !h.is_multiple_of(4) || rgba.len() != w * h * 4 {
        return Err(PyValueError::new_err(format!(
            "dxt5_encode: {w} x {h} (multiples of 4) and {} bytes of RGBA",
            rgba.len()
        )));
    }
    let img = bc2_menu::pil::Rgba {
        w,
        h,
        px: rgba.to_vec(),
    };
    let blocks = py.detach(|| bc2_menu::pil::dxt5_encode(&img));
    Ok(PyBytes::new(py, &blocks.concat()))
}
