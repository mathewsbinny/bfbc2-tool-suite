//! Project OPFOR's Pacific War main-menu mode (`src\tools\ww2\menu_install.py`'s movie edits, round OR14 "OPFOR in
//! Rust"): `main_nav`'s third mode (three image characters and textured quads, `gameModBackground`'s frame
//! `BFBC2143`, `gameModSelector`'s PACIFIC WAR button and its new frame, `bgBar` / `loggo_mc`, the label alias and the
//! label switches, `setGameMod`'s load-time pick) and `main_root`'s team-name suffix (`getGModStringExtensino`), on
//! the movie model of [`crate::graft`] - step for step as the Python did them on `aptgraft`'s, so the movies it
//! writes are the same bytes (`rust/parity_pacific_menu.py`).
//!
//! The Python's objects stand as the model's numbers: an instruction is its index in its code node; a `Pool` (the
//! names of a constant pool list, kept beside the list and grown with it) is an entry of [`Pools`], one per `Pool`
//! the Python made, so two pools of one list stay two (`functions()` makes a new one for each `ConstantPool` it
//! walks past). Every `AptError` the Python raised is a [`MenuError::Apt`] with its words; its other exceptions (an
//! index or attribute error on a movie of another shape) are [`MenuError::Reference`].

#![allow(
    clippy::indexing_slicing,
    reason = "each index is checked just before it, as the Python indexes"
)]
#![allow(
    clippy::many_single_char_names,
    clippy::doc_markdown,
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::similar_names,
    clippy::too_many_arguments,
    clippy::format_push_string,
    reason = "the Python's names (m, i, k, t, v ...) and its integer fields kept for review against menu_install.py"
)]

use std::collections::BTreeMap;

use bc2_formats::aptmodel::{Arg, Code, Ins, Rid, Target, Val};

use crate::error::{MenuError, MenuResult};
use crate::graft::edit::{add_label_alias, arg_text};
use crate::graft::{
    APT, CKey, FN_MAGIC, GEO, Movie, SIG, apt_err, f32_bits, fl, ins, int_arg, is_branch, latin1, latin1_encode,
    push_ins, signed, walk_ins,
};

/// DICE's label for mode 3; the DLL sends it for Pacific War.
pub const LABEL: &str = "BFBC2143";
/// Vietnam's mode label.
pub const NAM: &str = "BFBC2Nam";
/// BFBC2's mode label.
pub const BC2: &str = "BFBC2";
/// Vietnam's base background shape (0, 72) - (1282, 794).
pub const BG_SHAPE_LIKE: i64 = 415;
/// `gameModSelector`.
pub const SELECTOR: i64 = 443;
/// `gameModBackground`.
pub const BACKGROUND: i64 = 438;
/// `bgBar`.
pub const BGBAR: i64 = 466;
/// `loggo_mc`.
pub const LOGO: i64 = 477;
/// The selector's button.
pub const BUTTON_CHAR: u32 = 36;
/// Vietnam's label quad in the selector.
pub const LABEL_AT: (f64, f64) = (15.5, 65.0);
/// `loggo_mc`'s Vietnam logo shape.
pub const VN_LOGO_SHAPE: u32 = 476;
/// Where Vietnam's logo sits.
pub const LOGO_AT: (f64, f64) = (42.0, 24.0);
/// The second button's offset below the first.
pub const ROW: f64 = 30.0;
/// The PACIFIC WAR button's name.
pub const PW_BUTTON: &str = "BFBC2143_button";

/// The constant pools' names (`Pool` objects): (the pool list node, its names).
#[derive(Debug, Default)]
pub struct Pools {
    pools: Vec<(Rid, Vec<String>)>,
}

impl Pools {
    /// `Pool(movie, node)`: the names read now.
    fn new_pool(&mut self, m: &Movie, node: Rid) -> MenuResult<usize> {
        let names = m
            .vals(node)?
            .iter()
            .map(|v| m.const_text_of(*v))
            .collect::<MenuResult<Vec<_>>>()?;
        self.pools.push((node, names));
        Ok(self.pools.len() - 1)
    }

    fn names(&self, p: usize) -> &[String] {
        &self.pools[p].1
    }

    /// `Pool.idx(t)`: the name's index, added to the list and the names when missing.
    fn idx(&mut self, m: &mut Movie, p: usize, t: &str) -> MenuResult<usize> {
        if !self.pools[p].1.iter().any(|n| n == t) {
            let k = m.const_index(&CKey::S(latin1_encode(t)?))?;
            let node = self.pools[p].0;
            m.vals_mut(node)?.push(Val::Num(k as u32));
            self.pools[p].1.push(t.to_owned());
        }
        Ok(self.pools[p].1.iter().position(|n| n == t).unwrap_or(0))
    }

    /// `Pool.push(t)`.
    fn push(&mut self, m: &mut Movie, p: usize, t: &str) -> MenuResult<Ins> {
        let k = self.idx(m, p, t)?;
        Ok(ins(if k < 256 { 0xA2 } else { 0xA3 }, vec![Arg::Int(k as i64)]))
    }

    /// `Pool.op(op, t)`: an EA op with a pool-byte name.
    fn op(&mut self, m: &mut Movie, p: usize, op: u8, t: &str) -> MenuResult<Ins> {
        let k = self.idx(m, p, t)?;
        if k > 255 {
            return Err(apt_err(format!("pool index {k} for \"{t}\" does not fit a byte")));
        }
        Ok(ins(op, vec![Arg::Int(k as i64)]))
    }
}

/// One function of `functions(movie)`: its code node, the definition's instruction, its name, its body's pool.
#[derive(Debug, Clone)]
struct Func {
    node: Rid,
    ins: u32,
    name: Option<String>,
    pool: Option<usize>,
}

fn ref_arg(i: &Ins, k: usize) -> MenuResult<Rid> {
    match i.args.get(k) {
        Some(Arg::Ref(r)) => Ok(*r),
        _ => Err(MenuError::Reference),
    }
}

/// `functions(movie)`: every function of the movie (definitions with a body), depth first, each with the pool its
/// body uses.
fn functions(m: &Movie, pools: &mut Pools) -> MenuResult<Vec<Func>> {
    fn walk(
        m: &Movie,
        pools: &mut Pools,
        node: Rid,
        code: &Code,
        s: u32,
        mut pool: Option<usize>,
        out: &mut Vec<Func>,
        depth: usize,
    ) -> MenuResult<()> {
        if depth > 4 * bc2_formats::aptmodel::MAX_NESTING {
            return Err(MenuError::Reference);
        }
        let lst = code.streams.get(s as usize).cloned().ok_or(MenuError::Reference)?;
        for k in lst {
            let i = &code.ins[k as usize];
            if i.op == 0x88 {
                pool = Some(pools.new_pool(m, ref_arg(i, 1)?)?);
            }
            let Some(b) = i.body else { continue };
            let body = &code.streams[b as usize];
            let bpool = match body.first() {
                Some(&f) if code.ins[f as usize].op == 0x88 => {
                    Some(pools.new_pool(m, ref_arg(&code.ins[f as usize], 1)?)?)
                }
                _ => pool,
            };
            let name = if i.op == 0x8E {
                arg_text(m, i.args.first())?
            } else {
                Some(String::new())
            };
            out.push(Func {
                node,
                ins: k,
                name,
                pool: bpool,
            });
            walk(m, pools, node, code, b, bpool, out, depth + 1)?;
        }
        Ok(())
    }
    let mut out = Vec::new();
    for n in m.code_nodes(false)? {
        let code = m.code(n)?.clone();
        walk(m, pools, n, &code, 0, None, &mut out, 0)?;
    }
    Ok(out)
}

/// `clone(i)`: a fresh copy of an instruction (its strings and lists new nodes); `None` target, as `Ins()` makes it.
fn clone_ins(m: &mut Movie, i: &Ins) -> MenuResult<Ins> {
    if i.body.is_some() {
        return Err(MenuError::Reference); // the Python clones bodies too: never asked for here
    }
    let mut args = Vec::with_capacity(i.args.len());
    for a in &i.args {
        args.push(match *a {
            Arg::Ref(r) => match m.node(r)?.content.kind() {
                1 => Arg::Ref(m.new_str(APT, m.str_text(r)?.to_vec())),
                2 => {
                    let vals = m.vals(r)?.clone();
                    let tag = m.tag(r)?.to_owned();
                    Arg::Ref(m.new_words(APT, vals, &tag))
                }
                _ => Arg::Ref(r),
            },
            other => other,
        });
    }
    Ok(ins(i.op, args))
}

/// `getinfo(text)`: a pointer to a new string.
fn getinfo(m: &mut Movie, t: &str) -> MenuResult<Arg> {
    let b = latin1_encode(t)?;
    Ok(Arg::Ref(m.new_str(APT, b)))
}

fn getinfo_val(m: &mut Movie, t: &str) -> MenuResult<Val> {
    let b = latin1_encode(t)?;
    Ok(Val::Ref(m.new_str(APT, b)))
}

/// `push_reg(movie, r)`.
fn push_reg(m: &mut Movie, r: u32) -> MenuResult<Ins> {
    let k = m.const_index(&CKey::T(4, r))?;
    let pl = m.new_words(APT, vec![Val::Num(k as u32)], "pushlist");
    Ok(ins(0x96, vec![Arg::Int(1), Arg::Ref(pl)]))
}

/// `button_pressed(movie, pool)`: `function buttonPressed(btn)` sending changeGameMod 3 / 1 / 0 by the button's name,
/// as stream 1 of `code`; the definition's instruction (not yet in a stream).
fn button_pressed(m: &mut Movie, pools: &mut Pools, pool: usize, code: &mut Code) -> MenuResult<Ins> {
    let body_stream = code.streams.len() as u32;
    code.streams.push(Vec::new());
    let end = ins(0x3E, Vec::new());
    // (instruction, its branch: 0 none, 1 NEXT, 2 END) and the NEXT markers
    let mut body: Vec<(Ins, u8)> = Vec::new();
    let mut nexts: Vec<usize> = Vec::new();
    for (name, mode) in [(PW_BUTTON, "3"), ("BFBC2Nam_button", "1")] {
        let a = getinfo(m, "FSCommand:changeGameMod")?;
        let b = getinfo(m, mode)?;
        let go0 = ins(0x83, vec![a, b]);
        let go1 = ins(0x99, Vec::new());
        body.push((push_reg(m, 1)?, 0));
        body.push((pools.op(m, pool, 0xAF, "_name")?, 0));
        body.push((pools.push(m, pool, name)?, 0));
        body.push((ins(0x49, Vec::new()), 0));
        body.push((ins(0x12, Vec::new()), 0));
        body.push((ins(0x9D, Vec::new()), 1));
        body.push((go0, 0));
        body.push((go1, 2));
        nexts.push(body.len());
    }
    let a = getinfo(m, "FSCommand:changeGameMod")?;
    let b = getinfo(m, "0")?;
    body.push((ins(0x83, vec![a, b]), 0));
    body.push((end, 0));
    let ks: Vec<u32> = body.iter().map(|(i, _)| push_ins(code, i.clone())).collect();
    let mut next = nexts.iter();
    for (j, (_, br)) in body.iter().enumerate() {
        match br {
            1 => {
                let to = *next.next().ok_or(MenuError::Reference)?;
                code.ins[ks[j] as usize].target = Target::Ins(ks[to]);
            }
            2 => code.ins[ks[j] as usize].target = Target::End(body_stream),
            _ => {}
        }
    }
    code.streams[body_stream as usize] = ks;
    let btn = latin1_encode("btn")?;
    let s = m.new_str(APT, btn);
    let params = m.new_words(APT, vec![Val::Num(1), Val::Ref(s)], "params2");
    let name = getinfo(m, "buttonPressed")?;
    let mut def = ins(
        0x8E,
        vec![
            name,
            Arg::Int(1),
            Arg::Int((0x2A00 << 16) | 2),
            Arg::Ref(params),
            Arg::Int(i64::from(FN_MAGIC.0)),
            Arg::Int(i64::from(FN_MAGIC.1)),
        ],
    );
    def.body = Some(body_stream);
    Ok(def)
}

/// `init_button(pool, var)`: the selector's setup of one button.
fn init_button(m: &mut Movie, pools: &mut Pools, p: usize, var: &str) -> MenuResult<Vec<Ins>> {
    let e = |op: u8| ins(op, Vec::new());
    let n = |op: u8, v: i64| ins(op, vec![Arg::Int(v)]);
    let mut out = vec![
        pools.push(m, p, " ")?,
        e(0x5A),
        pools.op(m, p, 0xAE, var)?,
        pools.op(m, p, 0xB2, "setValue")?,
    ];
    out.extend([
        n(0xB5, 6),
        n(0xB5, 20),
        n(0xB5, 2),
        pools.op(m, p, 0xAE, var)?,
        pools.op(m, p, 0xB2, "offsetText")?,
    ]);
    out.extend([
        pools.push(m, p, "left")?,
        e(0x5A),
        pools.op(m, p, 0xAE, var)?,
        pools.op(m, p, 0xB2, "alignText")?,
    ]);
    out.extend([
        n(0xB7, 16_777_215),
        e(0x5A),
        pools.op(m, p, 0xAE, var)?,
        pools.op(m, p, 0xB2, "setColor")?,
    ]);
    out.extend([
        pools.push(m, p, ">")?,
        e(0x5A),
        pools.op(m, p, 0xAE, var)?,
        pools.op(m, p, 0xB2, "iconState")?,
    ]);
    out.extend([
        e(0x74),
        e(0x5A),
        pools.op(m, p, 0xAE, var)?,
        pools.op(m, p, 0xB2, "setReallyBigButton")?,
    ]);
    out.extend([e(0x59), pools.op(m, p, 0xAE, var)?, pools.op(m, p, 0xB2, "init")?]);
    Ok(out)
}

/// `selector_action(movie, buttons)`: a frame action (Stop, buttonPressed, each button's setup); the action item.
fn selector_action(m: &mut Movie, pools: &mut Pools, buttons: &[&str]) -> MenuResult<Rid> {
    let mut names: Vec<&str> = vec![
        "buttonPressed",
        "_name",
        PW_BUTTON,
        "BFBC2Nam_button",
        " ",
        "left",
        ">",
        "setValue",
        "offsetText",
        "alignText",
        "setColor",
        "iconState",
        "setReallyBigButton",
        "init",
    ];
    names.extend_from_slice(buttons);
    let mut ks = Vec::with_capacity(names.len());
    for n in &names {
        ks.push(Val::Num(m.const_index(&CKey::S(latin1_encode(n)?))? as u32));
    }
    let plist = m.new_words(APT, ks, "poollist");
    let pool = pools.new_pool(m, plist)?;
    let mut code = Code {
        ins: Vec::new(),
        streams: vec![Vec::new()],
    };
    let mut top = vec![ins(0x88, vec![Arg::Int(names.len() as i64), Arg::Ref(plist)])];
    top.push(button_pressed(m, pools, pool, &mut code)?);
    top.push(ins(0x07, Vec::new()));
    for b in buttons {
        top.extend(init_button(m, pools, pool, b)?);
    }
    top.push(ins(0, Vec::new()));
    let plist_len = m.vals(plist)?.len();
    top[0].args[0] = Arg::Int(plist_len as i64);
    let ks: Vec<u32> = top.into_iter().map(|i| push_ins(&mut code, i)).collect();
    code.streams[0] = ks;
    let c = m.new_code(code);
    Ok(m.new_words(APT, vec![Val::Num(1), Val::Ref(c)], "action"))
}

/// `quad_shape(movie, x0, y0, w, h, like)`: an image character and a shape drawing it as one textured quad.
fn quad_shape(m: &mut Movie, x0: f64, y0: f64, w: f64, h: f64, like: i64) -> MenuResult<(i64, i64)> {
    let inode = m.new_words(APT, vec![Val::Num(7), Val::Num(SIG), Val::Num(0)], "image");
    let image = m.add_char(Some(inode))?;
    let inode = m.char(image)?.ok_or(MenuError::Reference)?;
    m.set(inode, 2, Val::Num(image as u32))?;
    let (table, k) = m.geo_entry(like)?.ok_or(MenuError::Reference)?;
    let Val::Ref(parts) = m.val(table, k + 2)? else {
        return Err(MenuError::Reference);
    };
    let Val::Ref(p0) = m.val(parts, 0)? else {
        return Err(MenuError::Reference);
    };
    let src = m.vals(p0)?.clone();
    if src.first() != Some(&Val::Num(5)) {
        return Err(apt_err(format!("shape {like} is not a textured quad")));
    }
    let (x1, y1) = (x0 + w, y0 + h);
    let mut verts = Vec::with_capacity(12);
    for v in [x0, y1, x0, y0, x1, y0, x1, y0, x1, y1, x0, y1] {
        verts.push(Val::Num(f32_bits(v)?));
    }
    let mut matrix: Vec<Val> = src.get(4..src.len().min(20)).map_or_else(Vec::new, <[Val]>::to_vec);
    if matrix.len() < 14 {
        return Err(MenuError::Reference);
    }
    matrix[12] = Val::Num(f32_bits(-x0)?);
    matrix[13] = Val::Num(f32_bits(-y0)?);
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
    let sv = vec![
        Val::Num(1),
        Val::Num(SIG),
        Val::Num(f32_bits(x0)?),
        Val::Num(f32_bits(y0)?),
        Val::Num(f32_bits(x1)?),
        Val::Num(f32_bits(y1)?),
        Val::Num(0),
    ];
    let snode = m.new_words(APT, sv, "shape");
    let shape = m.add_char(Some(snode))?;
    let snode = m.char(shape)?.ok_or(MenuError::Reference)?;
    m.set(snode, 6, Val::Num(shape as u32))?;
    let parts = m.new_words(GEO, vec![Val::Ref(part)], "geo.parts");
    m.add_geometry(shape, parts)?;
    Ok((image, shape))
}

/// `labels_of(movie, owner)`: {label: frame}, a later frame's label taking the name (Python's dict, in first-insertion
/// order: the order matters only to the error words).
fn labels_of(m: &mut Movie, owner: Option<Rid>) -> MenuResult<Vec<(Option<String>, usize)>> {
    let mut out: Vec<(Option<String>, usize)> = Vec::new();
    for f in 0..m.frame_count(owner)? {
        for it in m.frame_items(f, owner)? {
            if m.tag(it)? == "label" {
                let t = m.text(m.val(it, 1)?)?;
                match out.iter_mut().find(|(k, _)| *k == t) {
                    Some(e) => e.1 = f,
                    None => out.push((t, f)),
                }
            }
        }
    }
    Ok(out)
}

fn get(lbs: &[(Option<String>, usize)], key: &str) -> Option<usize> {
    lbs.iter().find(|(k, _)| k.as_deref() == Some(key)).map(|e| e.1)
}

/// A Python dict's repr of {str: int} (`{'BFBC2': 0, ...}`; a label that is not a string as `None`).
fn dict_repr(lbs: &[(Option<String>, usize)]) -> String {
    let items: Vec<String> = lbs
        .iter()
        .map(|(k, v)| match k {
            Some(s) => format!("{}: {v}", py_str_repr(s)),
            None => format!("None: {v}"),
        })
        .collect();
    format!("{{{}}}", items.join(", "))
}

/// `repr(str)` for the texts a movie holds (Latin-1).
fn py_str_repr(s: &str) -> String {
    let q = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::new();
    out.push(q);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == q => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || (0x7F..0xA1).contains(&(c as u32)) || c as u32 == 0xAD => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push(q);
    out
}

/// `places(movie, owner, frame)`.
fn places(m: &mut Movie, owner: Option<Rid>, frame: usize) -> MenuResult<Vec<Rid>> {
    let mut out = Vec::new();
    for it in m.frame_items(frame, owner)? {
        if m.tag(it)? == "place" {
            out.push(it);
        }
    }
    Ok(out)
}

/// `place_copy(item, cid, depth, name, x, y, new)` (`name`: `Some("")` clears it).
#[allow(clippy::too_many_arguments, reason = "the Python's keyword arguments")]
fn place_copy(
    m: &mut Movie,
    item: Rid,
    cid: Option<i64>,
    depth: Option<i64>,
    name: Option<&str>,
    x: Option<f64>,
    y: Option<f64>,
    new: bool,
) -> MenuResult<Rid> {
    let mut v = m.vals(item)?.clone();
    if v.len() < 16 {
        return Err(MenuError::Reference);
    }
    let num = |v: Val| match v {
        Val::Num(n) => Ok(n),
        Val::Ref(_) => Err(MenuError::Reference),
    };
    if let Some(c) = cid {
        v[3] = Val::Num(c as u32);
    }
    if let Some(d) = depth {
        v[2] = Val::Num(d as u32);
    }
    if new {
        v[1] = Val::Num((num(v[1])? & !0x01) | 0x02);
    }
    if let Some(x) = x {
        v[8] = Val::Num(f32_bits(x)?);
    }
    if let Some(y) = y {
        v[9] = Val::Num(f32_bits(y)?);
    }
    match name {
        Some(n) => {
            v[13] = if n.is_empty() { Val::Num(0) } else { getinfo_val(m, n)? };
            let f = num(v[1])?;
            v[1] = Val::Num(if n.is_empty() { f & !0x20 } else { f | 0x20 });
        }
        None => {
            if let Val::Ref(r) = v[13] {
                let t = m.str_text(r)?.to_vec();
                v[13] = Val::Ref(m.new_str(APT, t));
            }
        }
    }
    if v[15] != Val::Num(0) {
        return Err(apt_err("a placement with clip actions is not copied"));
    }
    Ok(m.new_words(APT, v, "place"))
}

/// The field's character id when it is a number (`p.vals[3] == n`).
fn char_is(m: &Movie, it: Rid, c: u32) -> MenuResult<bool> {
    Ok(m.val(it, 3)? == Val::Num(c))
}

/// `label_alias_tests(movie, like, alias)`: every `if (v == "like")` - <v>; PushConstant like; Equals2; Not;
/// BranchIfTrue - also runs its block for v == alias. The count.
pub fn label_alias_tests(m: &mut Movie, like: &str, alias: &str) -> MenuResult<usize> {
    let mut pools = Pools::default();
    let funcs = functions(m, &mut pools)?;
    let mut done = 0;
    for f in &funcs {
        let Some(p) = f.pool else { continue };
        let mut code = m.code(f.node)?.clone();
        let b = code.ins[f.ins as usize].body.ok_or(MenuError::Reference)?;
        let mut j = 0;
        while j + 4 < code.streams[b as usize].len() {
            let body = code.streams[b as usize].clone();
            let [v, c, eq, nt, br] = [0, 1, 2, 3, 4].map(|d| code.ins[body[j + d] as usize].clone());
            let names = pools.names(p);
            let c_like = matches!(c.op, 0xA2 | 0xA3)
                && match c.args.first() {
                    Some(Arg::Int(k)) => usize::try_from(*k)
                        .ok()
                        .and_then(|k| names.get(k))
                        .is_some_and(|n| n == like),
                    _ => return Err(MenuError::Reference),
                };
            let v_ok = v.op == 0xAE || (v.op == 0x96 && matches!(v.args.first(), Some(Arg::Int(1))));
            if c_like && matches!(eq.op, 0x49 | 0x66) && nt.op == 0x12 && br.op == 0x9D && v_ok {
                let after = *body.get(j + 5).ok_or(MenuError::Reference)?;
                let first = clone_ins(m, &v)?;
                let push = pools.push(m, p, alias)?;
                let mut bif = ins(0x9D, Vec::new());
                bif.target = Target::Ins(after);
                let block = [first, push, ins(eq.op, Vec::new()), bif];
                let ks: Vec<u32> = block.into_iter().map(|x| push_ins(&mut code, x)).collect();
                let vk = body[j];
                for k in walk_ins(&code, b) {
                    let x = &mut code.ins[k as usize];
                    if is_branch(x.op) && x.target == Target::Ins(vk) {
                        x.target = Target::Ins(ks[0]);
                    }
                }
                code.streams[b as usize].splice(j..j, ks);
                done += 1;
                j += 9;
            } else {
                j += 1;
            }
        }
        if *m.code(f.node)? != code {
            *m.code_mut(f.node)? = code;
        }
    }
    Ok(done)
}

/// `edit_main_nav(movie, report, label_size, logo_size)`: (background image, label image, logo image).
pub fn edit_main_nav(
    m: &mut Movie,
    report: &mut Vec<String>,
    label_size: (f64, f64),
    logo_size: (f64, f64),
) -> MenuResult<(i64, i64, i64)> {
    for (cid, char_name) in [
        (SELECTOR, "gameModSelector"),
        (BACKGROUND, "gameModBackground"),
        (BGBAR, "bgBar"),
        (LOGO, "loggo_mc"),
    ] {
        if m.char_type(cid)? != Some(5) {
            return Err(apt_err(format!(
                "char {cid} ({char_name}) is not a sprite - not the main_nav this tool knows"
            )));
        }
    }
    let ch = |m: &Movie, c: i64| m.char(c).and_then(|r| r.ok_or(MenuError::Reference));
    let (sel, bg, bar, logo) = (ch(m, SELECTOR)?, ch(m, BACKGROUND)?, ch(m, BGBAR)?, ch(m, LOGO)?);
    if get(&labels_of(m, Some(sel))?, LABEL).is_some() {
        return Err(apt_err("main_nav has the Pacific War selector frame already"));
    }
    let (lb_bg, lb_sel) = (labels_of(m, Some(bg))?, labels_of(m, Some(sel))?);
    if get(&lb_bg, LABEL) != Some(2) || get(&lb_sel, BC2) != Some(0) || get(&lb_sel, NAM) != Some(1) {
        return Err(apt_err(format!(
            "main_nav: unexpected frames (gameModBackground {}, gameModSelector {})",
            dict_repr(&lb_bg),
            dict_repr(&lb_sel)
        )));
    }

    // the pictures' characters
    let (bg_img, bg_shape) = quad_shape(m, 0.0, 72.0, 1282.0, 722.0, BG_SHAPE_LIKE)?;
    let (lb_img, lb_shape) = quad_shape(m, LABEL_AT.0, LABEL_AT.1, label_size.0, label_size.1, 440)?;
    let (lg_img, lg_shape) = quad_shape(
        m,
        LOGO_AT.0,
        LOGO_AT.1,
        logo_size.0,
        logo_size.1,
        i64::from(VN_LOGO_SHAPE),
    )?;
    report.push(format!(
        "main_nav: background image {bg_img} / shape {bg_shape}; label image {lb_img} / shape {lb_shape}; logo image \
         {lg_img} / shape {lg_shape}"
    ));

    // gameModBackground: frame BFBC2143 places the background where frame BFBC2 places its base
    if !places(m, Some(bg), 2)?.is_empty() {
        return Err(apt_err("gameModBackground frame BFBC2143 places something already"));
    }
    let base0 = *places(m, Some(bg), 0)?.first().ok_or(MenuError::Reference)?;
    let c = place_copy(m, base0, Some(bg_shape), Some(1), None, None, None, true)?;
    m.add_item(2, c, Some(bg), None)?;
    report.push("main_nav: gameModBackground frame 2 \"BFBC2143\" places the background (depth 1, no parallax)".into());

    // gameModSelector: the second button (PACIFIC WAR) in BFBC2 and BFBC2Nam; a new frame BFBC2143
    let (p0, p1) = (places(m, Some(sel), 0)?, places(m, Some(sel), 1)?);
    let pick = |m: &Movie, ps: &[Rid], c: u32| -> MenuResult<Rid> {
        for &p in ps {
            if char_is(m, p, c)? {
                return Ok(p);
            }
        }
        Err(MenuError::Reference)
    };
    let (btn0, lab0) = (pick(m, &p0, BUTTON_CHAR)?, pick(m, &p0, 440)?);
    let (btn1, lab1) = (pick(m, &p1, BUTTON_CHAR)?, pick(m, &p1, 442)?);
    let used = m.timeline_depths(Some(sel))?;
    let top = *used.iter().max().ok_or(MenuError::Reference)?;
    let (d_btn, d_lab) = (top + 1, top + 2);
    let flv = |m: &Movie, it: Rid, k: usize| -> MenuResult<f64> { Ok(fl(m.num(it, k)?)) };
    let mut pools = Pools::default();
    let (bx, by) = (flv(m, btn0, 8)?, flv(m, btn0, 9)?);
    let c = place_copy(
        m,
        btn0,
        None,
        Some(d_btn),
        Some(PW_BUTTON),
        Some(bx),
        Some(by + ROW),
        true,
    )?;
    m.add_item(0, c, Some(sel), None)?;
    let c = place_copy(m, lab0, Some(lb_shape), Some(d_lab), None, None, Some(ROW), true)?;
    m.add_item(0, c, Some(sel), None)?;
    let a = selector_action(m, &mut pools, &[PW_BUTTON])?;
    m.add_item(0, a, Some(sel), None)?;
    let a = selector_action(m, &mut pools, &[])?;
    m.add_item(1, a, Some(sel), None)?;
    let new = m.frame_count(Some(sel))?;
    let mut all: Vec<i64> = used.iter().copied().collect();
    all.extend([d_btn, d_lab]);
    all.sort_unstable();
    all.dedup();
    for d in all {
        let r = m.new_words(APT, vec![Val::Num(4), Val::Num(d as u32)], "remove");
        m.add_item(new, r, Some(sel), None)?;
    }
    let lv = getinfo_val(m, LABEL)?;
    let l = m.new_words(APT, vec![Val::Num(2), lv, Val::Num(0), Val::Num(new as u32)], "label");
    m.add_item(new, l, Some(sel), None)?;
    let (x1, y1) = (flv(m, btn1, 8)?, flv(m, btn1, 9)?);
    let c = place_copy(m, btn1, None, Some(1), Some("BFBC2_button"), Some(x1), Some(y1), true)?;
    m.add_item(new, c, Some(sel), None)?;
    let c = place_copy(m, lab1, None, Some(2), None, None, None, true)?;
    m.add_item(new, c, Some(sel), None)?;
    let y0 = flv(m, btn0, 9)?;
    let c = place_copy(
        m,
        btn0,
        None,
        Some(d_btn),
        Some("BFBC2Nam_button"),
        None,
        Some(y0 + ROW),
        true,
    )?;
    m.add_item(new, c, Some(sel), None)?;
    let c = place_copy(m, lab0, None, Some(d_lab), None, None, Some(ROW), true)?;
    m.add_item(new, c, Some(sel), None)?;
    let a = selector_action(m, &mut pools, &["BFBC2_button", "BFBC2Nam_button"])?;
    m.add_item(new, a, Some(sel), None)?;
    report.push(format!(
        "main_nav: gameModSelector - PACIFIC WAR button (\"{PW_BUTTON}\", depth {d_btn}) and label in frames BFBC2 / \
         BFBC2Nam, {} px under the first; new frame {new} \"{LABEL}\" (BFBC2 + Vietnam buttons); buttonPressed sends \
         changeGameMod 3 / 1 / 0 by name",
        py_g(ROW)
    ));

    // bgBar / loggo_mc
    for (owner, cid, what) in [(bar, BGBAR, "bgBar"), (logo, LOGO, "loggo_mc")] {
        let lbs = labels_of(m, Some(owner))?;
        let (f3, fname) = (get(&lbs, LABEL), get(&lbs, NAM));
        let (Some(f3), Some(fname)) = (f3, fname) else {
            return Err(apt_err(format!("{what}: frame {LABEL} missing or not empty")));
        };
        if !places(m, Some(owner), f3)?.is_empty() {
            return Err(apt_err(format!("{what}: frame {LABEL} missing or not empty")));
        }
        let src = *places(m, Some(owner), fname)?.first().ok_or(MenuError::Reference)?;
        let d = signed(m.num(src, 2)?);
        let r = m.new_words(APT, vec![Val::Num(4), Val::Num(d as u32)], "remove");
        m.add_item(f3, r, Some(owner), None)?;
        if cid == LOGO {
            let sc = m.val(src, 3)?;
            if sc != Val::Num(VN_LOGO_SHAPE) {
                let shown = match sc {
                    Val::Num(n) => n.to_string(),
                    Val::Ref(_) => return Err(MenuError::Reference),
                };
                return Err(apt_err(format!(
                    "loggo_mc frame {NAM} places char {shown}, not Vietnam's logo {VN_LOGO_SHAPE}"
                )));
            }
            let c = place_copy(m, src, Some(lg_shape), None, None, None, None, true)?;
            m.add_item(f3, c, Some(owner), None)?;
            report.push(format!(
                "main_nav: {what} frame {f3} \"{LABEL}\" places the Pacific War logo (shape {lg_shape}) as frame {NAM} \
                 places Vietnam's"
            ));
            continue;
        }
        let c = place_copy(m, src, None, None, None, None, None, true)?;
        m.add_item(f3, c, Some(owner), None)?;
        let shown = match m.val(src, 3)? {
            Val::Num(n) => n.to_string(),
            Val::Ref(_) => return Err(MenuError::Reference),
        };
        report.push(format!(
            "main_nav: {what} frame {f3} \"{LABEL}\" shows Vietnam's art (char {shown}) - no Pacific War art for it"
        ));
    }

    // every other BFBC2Nam timeline: BFBC2143 on the same frame; the label switches
    let n = add_label_alias(m, NAM, LABEL)?;
    report.push(format!(
        "main_nav: label \"{LABEL}\" added beside \"{NAM}\" in {n} timelines (Vietnam's content)"
    ));
    let n = label_alias_tests(m, NAM, LABEL)?;
    report.push(format!(
        "main_nav: {n} switches on \"{NAM}\" also take \"{LABEL}\" (MainMenu.gameModChanged, ...)"
    ));

    // setGameMod: the load-time pick
    let mut pools = Pools::default();
    let fns: Vec<Func> = functions(m, &mut pools)?
        .into_iter()
        .filter(|f| f.name.as_deref() == Some("setGameMod"))
        .collect();
    if fns.len() != 1 {
        return Err(apt_err(format!("main_nav: {} setGameMod functions", fns.len())));
    }
    let f = &fns[0];
    let mut code = m.code(f.node)?.clone();
    let b = code.ins[f.ins as usize].body.ok_or(MenuError::Reference)?;
    let body = code.streams[b as usize].clone();
    let ops = |r: std::ops::Range<usize>| -> Vec<u8> {
        body.get(r.start.min(body.len())..r.end.min(body.len()))
            .unwrap_or_default()
            .iter()
            .map(|&k| code.ins[k as usize].op)
            .collect()
    };
    if ops(0..11) != [0x96, 0xA2, 0x49, 0x4C, 0x9D, 0x17, 0x96, 0x76, 0x49, 0x12, 0x9D] {
        return Err(apt_err("main_nav: setGameMod does not start with the empty-label test"));
    }
    if ops(21..28) != [0xAE, 0xAF, 0xAF, 0xAF, 0xA2, 0x74, 0x4F] {
        return Err(apt_err(
            "main_nav: setGameMod's continue-button hiding is not where expected",
        ));
    }
    let Some(p) = f.pool else {
        return Err(MenuError::Reference); // pool.op on None: AttributeError
    };
    let first = body[0];
    let mut t = Vec::with_capacity(11);
    for &k in &body[..11] {
        let x = code.ins[k as usize].clone();
        t.push(clone_ins(m, &x)?);
    }
    let mut new_ins: Vec<Ins> = Vec::new();
    let pick = [
        pools.op(m, p, 0xAE, "extern")?,
        pools.op(m, p, 0xAF, "opforGameMod")?,
        ins(0x4A, Vec::new()),
        ins(0xB5, vec![Arg::Int(3)]),
        ins(0x49, Vec::new()),
        ins(0x12, Vec::new()),
    ];
    let mut skip = ins(0x9D, Vec::new());
    skip.target = Target::Ins(first);
    let mut set = vec![
        pools.push(m, p, LABEL)?,
        ins(0x87, vec![Arg::Int(1)]),
        ins(0x17, Vec::new()),
    ];
    for &k in &body[21..28] {
        let x = code.ins[k as usize].clone();
        set.push(clone_ins(m, &x)?);
    }
    new_ins.extend(t);
    new_ins.extend(pick);
    new_ins.push(skip);
    new_ins.extend(set);
    let ks: Vec<u32> = new_ins.into_iter().map(|x| push_ins(&mut code, x)).collect();
    code.ins[ks[4] as usize].target = Target::Ins(ks[9]);
    code.ins[ks[10] as usize].target = Target::Ins(first);
    code.streams[b as usize].splice(0..0, ks);
    *m.code_mut(f.node)? = code;
    report.push(
        "main_nav: setGameMod - an empty label with extern.opforGameMod == 3 picks \"BFBC2143\" (the continue button \
         hidden, as for Vietnam)"
            .into(),
    );
    Ok((bg_img, lb_img, lg_img))
}

/// `edit_main_root(movie, report)`: `getGModStringExtensino` returns "_PW" when extern.opforGameMod == 3.
pub fn edit_main_root(m: &mut Movie, report: &mut Vec<String>) -> MenuResult<()> {
    let mut pools = Pools::default();
    let funcs = functions(m, &mut pools)?;
    for f in &funcs {
        let mut code = m.code(f.node)?.clone();
        let i = code.ins[f.ins as usize].clone();
        if i.op != 0x9B {
            continue;
        }
        let b = i.body.ok_or(MenuError::Reference)?;
        let body = code.streams[b as usize].clone();
        let mut hit = false;
        if let Some(p) = f.pool {
            for &k in &body {
                let x = &code.ins[k as usize];
                if matches!(x.op, 0xA2 | 0xA3) {
                    let names = pools.names(p);
                    let kk = int_arg(x, 0)?;
                    if usize::try_from(kk)
                        .ok()
                        .and_then(|k| names.get(k))
                        .is_some_and(|n| n == "_NAM")
                    {
                        hit = true;
                        break;
                    }
                }
            }
        }
        if !hit {
            continue;
        }
        let p = f.pool.ok_or(MenuError::Reference)?;
        let first = *body.first().ok_or(MenuError::Reference)?;
        let k = pools.idx(m, p, "opforGameMod")?;
        let ext = if pools.idx(m, p, "extern")? < 256 {
            Some(pools.op(m, p, 0xAE, "extern")?)
        } else {
            None
        };
        let Some(ext) = ext else {
            // the Python builds the rest of the list first (its push of "_PW" may add a name), then raises
            pools.push(m, p, "_PW")?;
            return Err(apt_err("main_root: \"extern\" past the pool byte range"));
        };
        let mut bif = ins(0x9D, Vec::new());
        bif.target = Target::Ins(first);
        let list = [
            ext,
            ins(if k > 255 { 0xA3 } else { 0xA2 }, vec![Arg::Int(k as i64)]),
            ins(0x4E, Vec::new()),
            ins(0x4A, Vec::new()),
            ins(0xB5, vec![Arg::Int(3)]),
            ins(0x49, Vec::new()),
            ins(0x12, Vec::new()),
            bif,
            pools.push(m, p, "_PW")?,
            ins(0x3E, Vec::new()),
        ];
        let ks: Vec<u32> = list.into_iter().map(|x| push_ins(&mut code, x)).collect();
        code.streams[b as usize].splice(0..0, ks);
        *m.code_mut(f.node)? = code;
        report.push("main_root: getGModStringExtensino returns \"_PW\" when extern.opforGameMod == 3".into());
        return Ok(());
    }
    Err(apt_err("main_root: getGModStringExtensino not found"))
}

/// Python's `f'{v:g}'` for the round numbers the report prints.
fn py_g(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e16 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// The labels of each sprite's frames by owner (used by the tests).
#[must_use]
pub fn labels_by_frame(m: &mut Movie, owner: Option<Rid>) -> BTreeMap<usize, Vec<String>> {
    let mut out = BTreeMap::new();
    let n = m.frame_count(owner).unwrap_or(0);
    for f in 0..n {
        let mut labs = Vec::new();
        for it in m.frame_items(f, owner).unwrap_or_default() {
            if m.tag(it).ok() == Some("label")
                && let Ok(Some(t)) = m.text(m.val(it, 1).unwrap_or(Val::Num(0)))
            {
                labs.push(t);
            }
        }
        out.insert(f, labs);
    }
    out
}

/// `latin1` of a constant's text, re-exported for the binding's report.
#[must_use]
pub fn text_of(b: &[u8]) -> String {
    latin1(b)
}
