//! Synthetic movies for the API edits Project OPFOR's tools make (`add_icon_frame`, the label hiding,
//! `split_image_part`, `add_crosshair`'s tiles, the constant swap); no game data. Their parity with `aptgraft.py` on
//! the game's movies and on random cases: `rust\parity_menu_movies.py`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "tests"
)]

use std::collections::HashMap;

use bc2_formats::aptmodel::{Rid, Val};

use super::tiles::{Tile, split_values};
use super::{APT, CKey, GEO, Movie, SIG, consts, f32_bits, fl, icon, place_item};
use crate::error::MenuError;

fn le(v: &[u32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// `test_aptgraft.blank(name)`: an empty little-endian movie (one frame, no characters but the movie).
fn blank() -> Movie {
    let mut a = b"Apt Data:74\x1a\0\0\0\0".to_vec();
    let mv = a.len() as u32;
    a.extend(le(&[9, SIG, 1, mv + 60, 0, 1, mv + 68, 1280, 720, 33, 0, 0, 0, 0, 0]));
    a.extend(le(&[0, 0, mv]));
    let mut c = b"Apt constant file\x1a\0\0".to_vec();
    c.extend(le(&[mv, 0, 32]));
    let mut g = b"Apt PCGL geometry file\x1a\0".to_vec();
    g.extend(le(&[0, 32]));
    let members = vec![
        ("t.apt".to_owned(), a),
        ("t.const".to_owned(), c),
        ("t.geo".to_owned(), g),
    ];
    Movie::new(members, "synthetic").unwrap()
}

fn f(x: f64) -> u32 {
    f32_bits(x).unwrap()
}

fn nums(v: &[u32]) -> Vec<Val> {
    v.iter().copied().map(Val::Num).collect()
}

/// A textured-quad shape (w x h, centred) drawing image `image` through a texture matrix that moves it by (tx, ty):
/// the shape's id.
fn quad_shape(m: &mut Movie, w: f64, h: f64, image: i64, tx: f64, ty: f64) -> i64 {
    let (x0, y0, x1, y1) = (-w / 2.0, -h / 2.0, w / 2.0, h / 2.0);
    let s = m.new_words(APT, nums(&[1, SIG, f(x0), f(y0), f(x1), f(y1), 0]), "shape");
    let sid = m.add_char(Some(s)).unwrap();
    m.set(s, 6, Val::Num(sid as u32)).unwrap();
    let verts = [x0, y0, x1, y0, x1, y1, x0, y0, x1, y1, x0, y1].map(f);
    let vn = m.new_words(GEO, nums(&verts), "geo.vertices");
    let mut mat = [0u32; 16];
    mat[0] = f(1.0);
    mat[5] = f(1.0);
    mat[12] = f(tx);
    mat[13] = f(ty);
    let mut pv = vec![Val::Num(5), Val::Num(2), Val::Ref(vn), Val::Num(0xFFFF_FFFF)];
    pv.extend(nums(&mat));
    pv.extend([Val::Num(image as u32), Val::Num(0)]);
    let part = m.new_words(GEO, pv, "geo.part5");
    let parts = m.new_words(GEO, vec![Val::Ref(part)], "geo.parts");
    m.add_geometry(sid, parts).unwrap();
    sid
}

fn label(m: &mut Movie, frame: usize, text: &str, flags: u32, owner: Option<Rid>) {
    let s = m.new_str(APT, text.as_bytes().to_vec());
    let w = m.new_words(
        APT,
        vec![Val::Num(2), Val::Ref(s), Val::Num(flags), Val::Num(frame as u32)],
        "label",
    );
    m.add_item(frame, w, owner, None).unwrap();
}

/// A weaponicon-like movie: an image, a quad shape drawing it, and two sprite timelines - "icons" labels PBLB on
/// frame 0 placing the shape, "seats" labels PBLB and Boat on frame 0 placing nothing. (movie, icons, seats).
fn icon_movie() -> (Movie, i64, i64) {
    let mut m = blank();
    let img = m.new_words(APT, nums(&[7, SIG, 0]), "image");
    let image = m.add_char(Some(img)).unwrap();
    m.set(img, 2, Val::Num(image as u32)).unwrap();
    let shape = quad_shape(&mut m, 20.0, 10.0, image, 10.0, 5.0);
    let mut sprites = Vec::new();
    for _ in 0..2 {
        let s = m.new_words(APT, nums(&[5, SIG, 0, 0, 0]), "sprite");
        sprites.push((m.add_char(Some(s)).unwrap(), s));
    }
    let (icons, seats) = (sprites[0], sprites[1]);
    label(&mut m, 0, "PBLB", 0x7_0000, Some(icons.1));
    let p = place_item(&mut m, 1, shape, Some("pic"), 3.0, 4.0, 0x26).unwrap();
    m.add_item(0, p, Some(icons.1), None).unwrap();
    label(&mut m, 0, "PBLB", 0x7_0000, Some(seats.1));
    label(&mut m, 0, "Boat", 0x7_0000, Some(seats.1));
    (m, icons.0, seats.0)
}

fn reread(m: &Movie) -> Movie {
    let res = m.to_res(m.big, super::GRAFT_LAYOUT).unwrap();
    Movie::new(super::read_bigf(&res, "file").unwrap(), "back").unwrap()
}

#[test]
fn add_icon_frame_adds_an_image_a_scaled_quad_and_a_labelled_frame() {
    let (mut m, icons, seats) = icon_movie();
    // seats has "Boat": the reference refuses it, unless hidden for the call
    let err = icon::add_icon_frame(&mut m, "PBLB", "Boat", (78.0, 36.0)).unwrap_err();
    assert!(matches!(&err, MenuError::Apt(t) if t == "synthetic: a timeline has the label \"Boat\" already"));
    let (mut m, icons2, seats2) = icon_movie();
    assert_eq!((icons, seats), (icons2, seats2));
    let hidden = icon::hide_labels(&mut m, "Boat", icons, "__hidden_").unwrap();
    assert_eq!(hidden.len(), 1);
    let (image, frames) = icon::add_icon_frame(&mut m, "PBLB", "Boat", (78.0, 36.0)).unwrap();
    icon::restore_labels(&mut m, &hidden).unwrap();
    assert_eq!(frames, 1);
    let mut b = reread(&m);
    assert_eq!(b.char_type(image).unwrap(), Some(7));
    let io = b.char(icons).unwrap();
    assert_eq!(b.frame_count(io).unwrap(), 2);
    assert_eq!(b.label_frame("Boat", io).unwrap(), 1);
    let so = b.char(seats).unwrap();
    assert_eq!(b.label_frame("Boat", so).unwrap(), 0);
    let items = b.frame_items(1, io).unwrap();
    assert_eq!(b.tag(items[0]).unwrap(), "label");
    assert_eq!(b.val(items[0], 2).unwrap(), Val::Num(0x7_0000));
    assert_eq!(b.tag(items[1]).unwrap(), "place");
    let shape = i64::from(b.num(items[1], 3).unwrap());
    assert_eq!(b.char_type(shape).unwrap(), Some(1));
    assert_eq!(b.text(b.val(items[1], 13).unwrap()).unwrap().as_deref(), Some("pic"));
    let sn = b.char(shape).unwrap().unwrap();
    let bounds: Vec<f64> = (2..6).map(|k| fl(b.num(sn, k).unwrap())).collect();
    assert_eq!(bounds, vec![-39.0, -18.0, 39.0, 18.0]);
    let (t, k) = b.geo_entry(shape).unwrap().unwrap();
    let Val::Ref(parts) = b.val(t, k + 2).unwrap() else {
        panic!()
    };
    let Val::Ref(part) = b.val(parts, 0).unwrap() else {
        panic!()
    };
    let pv = b.vals(part).unwrap().clone();
    assert_eq!(pv[0], Val::Num(5));
    assert_eq!(pv[20], Val::Num(image as u32));
    assert_eq!(
        (fl(b.num(part, 16).unwrap()), fl(b.num(part, 17).unwrap())),
        (39.0, 18.0)
    );
    let Val::Ref(vn) = pv[2] else { panic!() };
    let vs: Vec<f64> = (0..4).map(|k| fl(b.num(vn, k).unwrap())).collect();
    assert_eq!(vs, vec![-39.0, -18.0, 39.0, -18.0]);
}

#[test]
fn add_icon_frame_refuses_a_like_without_a_shape_and_a_shape_that_is_not_one_quad() {
    let (mut m, _, _) = icon_movie();
    let e = icon::add_icon_frame(&mut m, "NONE", "X", (8.0, 8.0)).unwrap_err();
    assert!(matches!(&e, MenuError::Apt(t) if t == "synthetic: no frame labelled \"NONE\" places a shape"));
    let (mut m, _, _) = icon_movie();
    // a second part on the like shape
    let (t, k) = m.geo_entry(2).unwrap().unwrap();
    let Val::Ref(parts) = m.val(t, k + 2).unwrap() else {
        panic!()
    };
    let first = m.val(parts, 0).unwrap();
    m.vals_mut(parts).unwrap().push(first);
    let e = icon::add_icon_frame(&mut m, "PBLB", "X", (8.0, 8.0)).unwrap_err();
    assert!(matches!(&e, MenuError::Apt(t) if t == "synthetic: shape 2 is not one textured quad"));
}

#[test]
fn split_values_cuts_a_rectangle_onto_its_tiles() {
    // a 20 x 10 rectangle at (0, 0) drawing its image from (5, 5): pixels 5..25 x 5..15
    let verts: Vec<Val> = nums(&[0.0, 0.0, 20.0, 0.0, 20.0, 10.0, 0.0, 0.0, 20.0, 10.0, 0.0, 10.0].map(f));
    let mut v = nums(&[5, 2, 0, 0xFFFF_FFFF]);
    let mut mat = [0u32; 16];
    mat[0] = f(1.0);
    mat[5] = f(1.0);
    mat[12] = f(5.0);
    mat[13] = f(5.0);
    v.extend(nums(&mat));
    v.extend(nums(&[9, 0]));
    let t = |x: f64, image: i64, dx: f64| Tile {
        x,
        y: 0.0,
        w: 15.0,
        h: 30.0,
        image,
        dx,
        dy: 100.0,
    };
    let out = split_values(&v, &verts, &[t(0.0, 40, 200.0), t(15.0, 41, 300.0), t(60.0, 42, 0.0)]).unwrap();
    assert_eq!(out.len(), 2);
    let xs = |p: &(Vec<Val>, Vec<u32>)| -> Vec<f64> { p.1.iter().step_by(2).map(|&x| fl(x)).collect() };
    assert_eq!(xs(&out[0]), vec![0.0, 10.0, 10.0, 0.0, 10.0, 0.0]);
    assert_eq!(xs(&out[1]), vec![10.0, 20.0, 20.0, 10.0, 20.0, 10.0]);
    assert_eq!(out[0].0[20], Val::Num(40));
    assert_eq!(out[1].0[20], Val::Num(41));
    // the texture matrix's offset moved onto the tile's place: tx + dx - x
    assert_eq!(out[0].0[16], Val::Num(f(205.0)));
    assert_eq!(out[1].0[16], Val::Num(f(290.0)));
    assert_eq!(out[0].0[17], Val::Num(f(105.0)));
    // not a rectangle: a skewed matrix, three triangles' worth of vertices
    let mut skew = v.clone();
    skew[5] = Val::Num(f(0.5));
    let e = split_values(&skew, &verts, &[t(0.0, 40, 0.0)]).unwrap_err();
    assert!(
        matches!(&e, MenuError::Apt(t) if t == "a textured part that is not one axis-aligned rectangle cannot be split into tiles")
    );
    let e = split_values(&v, &verts[..10], &[t(0.0, 40, 0.0)]).unwrap_err();
    assert!(matches!(e, MenuError::Apt(_)));
    let mut bent = verts.clone();
    bent[4] = Val::Num(f(7.0));
    let e = split_values(&v, &bent, &[t(0.0, 40, 0.0)]).unwrap_err();
    assert!(
        matches!(&e, MenuError::Apt(t) if t == "a textured part whose triangles are not a rectangle in its image cannot be split")
    );
}

#[test]
fn rename_constants_swaps_texts_and_reads_back() {
    let mut m = blank();
    for t in ["MP_A", "keep", "MP_A", "MP_B"] {
        // two equal texts are two entries (const_index would share them: add them by hand)
        let s = m.new_str(super::CONST, t.as_bytes().to_vec());
        let e = m.const_entries().unwrap();
        m.vals_mut(e).unwrap().extend([Val::Num(1), Val::Ref(s)]);
    }
    let h = m.m.heads.const_head;
    m.set(h, 1, Val::Num(4)).unwrap();
    let keys: HashMap<String, String> = [("MP_A", "PW_A"), ("MP_B", "")]
        .into_iter()
        .map(|(a, b)| (a.to_owned(), b.to_owned()))
        .collect();
    let done = consts::rename_constants(&mut m, &keys).unwrap();
    assert_eq!(done, vec![("MP_A".to_owned(), 2), ("MP_B".to_owned(), 1)]);
    let b = reread(&m);
    assert_eq!(consts::const_texts(&b).unwrap(), vec!["PW_A", "keep", "PW_A", ""]);
    assert_eq!(b.constant(1).unwrap(), CKey::S(b"keep".to_vec()));
}
