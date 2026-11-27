//! `aptgraft.py`'s `add_icon_frame` (round OR14: Project OPFOR's `ww2\type95_icon.py` and
//! `onslaught\console_icons.py` call it through `bc2_native.AptMovie`), step for step, and the label hiding
//! `console_icons.add_icon` wraps it in.

#![allow(
    clippy::indexing_slicing,
    reason = "each index is checked just before it, as the reference indexes"
)]

use crate::error::{MenuError, MenuResult};
use bc2_formats::aptmodel::{Rid, Val};

use super::edit::sprite_owners;
use super::{APT, GEO, Movie, SIG, apt_err, f32_bits, fl, latin1_encode};

/// `Movie.label_frame(label, owner)` as `add_icon_frame`'s `frame_of`: `None` where the label is missing.
fn frame_of(m: &mut Movie, label: &str, owner: Option<Rid>) -> MenuResult<Option<usize>> {
    match m.label_frame(label, owner) {
        Ok(f) => Ok(Some(f)),
        Err(MenuError::Apt(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

/// The places of a frame that show a shape: `it.tag == 'place' and it.vals[1] & 0x02 and not isinstance(it.vals[3],
/// Ref) and movie.char_type(it.vals[3]) == 1`.
pub fn shape_places(m: &mut Movie, frame: usize, owner: Option<Rid>) -> MenuResult<Vec<Rid>> {
    let mut out = Vec::new();
    for it in m.frame_items(frame, owner)? {
        if m.tag(it)? != "place" {
            continue;
        }
        // it.vals[1] & 0x02: a pointer there is the reference's TypeError
        if m.num(it, 1)? & 0x02 == 0 {
            continue;
        }
        if let Val::Num(c) = m.val(it, 3)?
            && m.char_type(i64::from(c))? == Some(1)
        {
            out.push(it);
        }
    }
    Ok(out)
}

/// A field as the reference's `fl` reads it (`struct.pack('<I', v & 0xFFFFFFFF)`: a pointer is its TypeError).
fn flv(v: Val) -> MenuResult<f64> {
    match v {
        Val::Num(x) => Ok(fl(x)),
        Val::Ref(_) => Err(MenuError::Reference),
    }
}

/// A field's truth (`if v[15]`): a number other than 0, or a pointer.
fn truthy(v: Val) -> bool {
    !matches!(v, Val::Num(0))
}

/// `add_icon_frame(movie, like, label, size)`: (the image character's id, the timelines given a frame).
///
/// A new image character (texture id = its id), a shape of `size` (width, height) drawing it - `like`'s shape's one
/// textured quad scaled, its texture matrix centred on the new size - and, in every timeline whose `like` frame
/// places a shape, a frame at its end labelled `label` placing the new shape as `like`'s frame places its own.
pub fn add_icon_frame(m: &mut Movie, like: &str, label: &str, size: (f64, f64)) -> MenuResult<(i64, usize)> {
    add_icon_frame_neg(m, like, label, size, (-size.0, -size.1))
}

/// [`add_icon_frame`] with the shape's lower bounds' numerators given: `-w`, `-h` as the reference computes them
/// (a Python int 0 negates to 0, so `-w / 2` is +0.0 where a float's is -0.0: `bc2_native.AptMovie` passes them).
#[allow(clippy::too_many_lines, reason = "the reference's one function, step for step")]
pub fn add_icon_frame_neg(
    m: &mut Movie,
    like: &str,
    label: &str,
    size: (f64, f64),
    neg: (f64, f64),
) -> MenuResult<(i64, usize)> {
    let mut found: Vec<(Option<Rid>, usize, Rid)> = Vec::new();
    for owner in sprite_owners(m)? {
        let Some(f) = frame_of(m, like, owner)? else {
            continue;
        };
        if frame_of(m, label, owner)?.is_some() {
            return Err(apt_err(format!(
                "{}: a timeline has the label \"{label}\" already",
                m.label
            )));
        }
        if let Some(&p) = shape_places(m, f, owner)?.first() {
            found.push((owner, f, p));
        }
    }
    let Some(&(_, _, first)) = found.first() else {
        return Err(apt_err(format!(
            "{}: no frame labelled \"{like}\" places a shape",
            m.label
        )));
    };
    let like_shape = i64::from(m.num(first, 3)?);
    let Some((table, k)) = m.geo_entry(like_shape)? else {
        return Err(apt_err(format!("{}: shape {like_shape} has no geometry", m.label)));
    };
    let Val::Ref(parts_node) = m.val(table, k + 2)? else {
        return Err(MenuError::Reference);
    };
    let parts = m.vals(parts_node)?.clone();
    let is_quad = match parts.as_slice() {
        [Val::Ref(p)] => m.val(*p, 0)? == Val::Num(5),
        [Val::Num(_)] => return Err(MenuError::Reference), // parts[0].node: AttributeError
        _ => false,
    };
    if !is_quad {
        return Err(apt_err(format!(
            "{}: shape {like_shape} is not one textured quad",
            m.label
        )));
    }
    let Val::Ref(quad_part) = parts[0] else {
        return Err(MenuError::Reference);
    };
    let src = m.vals(quad_part)?.clone();
    let shape_node = m.char(like_shape)?.ok_or(MenuError::Reference)?;
    let sv = m.vals(shape_node)?.clone();
    let sf = |k: usize| -> MenuResult<f64> { flv(*sv.get(k).ok_or(MenuError::Reference)?) };
    let (w0, h0) = (sf(4)? - sf(2)?, sf(5)? - sf(3)?);
    let (w, h) = size;
    let image_node = m.new_words(APT, vec![Val::Num(7), Val::Num(SIG), Val::Num(0)], "image");
    let image = m.add_char(Some(image_node))?;
    m.set(image_node, 2, Val::Num(image as u32))?;
    // verts: each vertex field scaled (x by w / w0, y by h / h0; a zero span: ZeroDivisionError)
    let Val::Ref(vnode) = *src.get(2).ok_or(MenuError::Reference)? else {
        return Err(MenuError::Reference);
    };
    let mut verts = Vec::new();
    for (i, v) in m.vals(vnode)?.clone().into_iter().enumerate() {
        let x = flv(v)?;
        let (num, den) = if i % 2 == 0 { (w, w0) } else { (h, h0) };
        if den == 0.0 {
            return Err(MenuError::Reference);
        }
        verts.push(Val::Num(f32_bits(x * (num / den))?));
    }
    let mut matrix: Vec<Val> = src.get(4..src.len().min(20)).map_or_else(Vec::new, <[Val]>::to_vec);
    if matrix.len() < 14 {
        return Err(MenuError::Reference); // matrix[12], matrix[13] = ...: IndexError
    }
    matrix[12] = Val::Num(f32_bits(w / 2.0)?);
    matrix[13] = Val::Num(f32_bits(h / 2.0)?);
    let vn = m.new_words(GEO, verts, "geo.vertices");
    let mut pv = vec![
        Val::Num(5),
        *src.get(1).ok_or(MenuError::Reference)?,
        Val::Ref(vn),
        *src.get(3).ok_or(MenuError::Reference)?,
    ];
    pv.extend(matrix);
    pv.extend([Val::Num(image as u32), Val::Num(0)]);
    let part = m.new_words(GEO, pv, "geo.part5");
    let shape_vals = vec![
        Val::Num(1),
        Val::Num(SIG),
        Val::Num(f32_bits(neg.0 / 2.0)?),
        Val::Num(f32_bits(neg.1 / 2.0)?),
        Val::Num(f32_bits(w / 2.0)?),
        Val::Num(f32_bits(h / 2.0)?),
        Val::Num(0),
    ];
    let shape_new = m.new_words(APT, shape_vals, "shape");
    let shape = m.add_char(Some(shape_new))?;
    m.set(shape_new, 6, Val::Num(shape as u32))?;
    let parts_new = m.new_words(GEO, vec![Val::Ref(part)], "geo.parts");
    m.add_geometry(shape, parts_new)?;
    let count = found.len();
    for (owner, f, place) in found {
        let mut v = m.vals(place)?.clone();
        if v.len() < 16 {
            return Err(MenuError::Reference);
        }
        v[3] = Val::Num(shape as u32);
        if let Val::Ref(r) = v[13] {
            let t = m.str_text(r)?.to_vec();
            v[13] = Val::Ref(m.new_str(APT, t));
        }
        if truthy(v[15]) {
            return Err(apt_err(format!(
                "{}: \"{like}\" places its shape with clip actions - not copied",
                m.label
            )));
        }
        let new = m.frame_count(owner)?;
        // the label takes like's label flags (DICE's frame labels carry 0x70000; frames labelled with 0 drew as
        // white boxes on Wake's deploy grid)
        let mut flags = Val::Num(0);
        for it in m.frame_items(f, owner)? {
            if m.tag(it)? == "label" && m.text(m.val(it, 1)?)?.as_deref() == Some(like) {
                flags = m.val(it, 2)?;
                break;
            }
        }
        let s = m.new_str(APT, latin1_encode(label)?);
        let lw = m.new_words(
            APT,
            vec![Val::Num(2), Val::Ref(s), flags, Val::Num(new as u32)],
            "label",
        );
        m.add_item(new, lw, owner, None)?;
        let pw = m.new_words(APT, v, "place");
        m.add_item(new, pw, owner, None)?;
    }
    Ok((image, count))
}

/// `console_icons.add_icon`'s hiding: in every sprite timeline but character `keep`'s, each frame label `name` is
/// renamed `prefix + name` (a new string). Returns what [`restore_labels`] puts back, in the order hidden.
pub fn hide_labels(m: &mut Movie, name: &str, keep: i64, prefix: &str) -> MenuResult<Vec<(Rid, Val)>> {
    let mut hidden = Vec::new();
    for c in 0..m.char_count()? as i64 {
        if c == keep || m.char_type(c)? != Some(5) {
            continue;
        }
        let o = m.char(c)?;
        for f in 0..m.frame_count(o)? {
            for it in m.frame_items(f, o)? {
                if m.tag(it)? == "label" && m.text(m.val(it, 1)?)?.as_deref() == Some(name) {
                    let old = m.val(it, 1)?;
                    hidden.push((it, old));
                    let mut t = latin1_encode(prefix)?;
                    t.extend(latin1_encode(name)?);
                    let s = m.new_str(APT, t);
                    m.set(it, 1, Val::Ref(s))?;
                }
            }
        }
    }
    Ok(hidden)
}

/// Puts [`hide_labels`]'s labels back (`for it, v in hidden: it.vals[1] = v`).
pub fn restore_labels(m: &mut Movie, hidden: &[(Rid, Val)]) -> MenuResult<()> {
    for &(it, v) in hidden {
        m.set(it, 1, v)?;
    }
    Ok(())
}
