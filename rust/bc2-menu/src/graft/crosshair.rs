//! `aptgraft.py`'s crosshairs: `crosshair_table`, `timeline_display`, `crosshair_objects` and `add_crosshair` (with
//! the `images` tiles since round OR14: [`add_crosshair_tiles`]), as the reference reads and edits `hud_crosshair`.

#![allow(
    clippy::indexing_slicing,
    reason = "each index is checked just before it, as the reference indexes"
)]

use std::collections::{BTreeMap, HashMap};

use crate::error::{MenuError, MenuResult};
use bc2_formats::aptmodel::{Arg, Ins, Rid, Val};

use super::edit::arg_text;
use super::tiles::{Tile, split_image_part};
use super::{
    APT, CKey, Grafter, Movie, apt_err, ins, int_arg, is_branch, is_one_str, is_pool_byte, latin1_encode, op_display,
    push_ins, signed,
};

const CH_FUNCTION: &str = "init";
const CH_TABLE: &str = "crossHairs";
const PRELOAD: [u32; 6] = [0x01, 0x04, 0x10, 0x40, 0x80, 0x100];

fn value_push(op: u8) -> bool {
    matches!(
        op,
        0xA1 | 0xA2 | 0xA3 | 0xB4 | 0xB5 | 0xB6 | 0xB7 | 0x59 | 0x5A | 0x5B | 0x5C | 0x73 | 0x74 | 0x75 | 0x76
    )
}

/// A function definition: its code node, its instruction and the pool its body reads.
#[derive(Debug, Clone, Copy)]
pub struct Func {
    /// The code node.
    pub code: Rid,
    /// The DefineFunction2.
    pub ins: u32,
    /// The pool node.
    pub pool: Option<Rid>,
}

/// `named_function(movie, name)`.
pub fn named_function(m: &Movie, name: &str) -> MenuResult<Func> {
    let mut found = Vec::new();
    for n in m.code_nodes(false)? {
        let code = m.code(n)?;
        nf_walk(m, n, code, 0, None, name, &mut found, 0)?;
    }
    if found.len() != 1 {
        return Err(apt_err(format!(
            "{}: {} functions named {name} (one expected)",
            m.label,
            found.len()
        )));
    }
    Ok(found[0])
}

#[allow(
    clippy::too_many_arguments,
    reason = "the walk's state, as the reference's closure holds it"
)]
fn nf_walk(
    m: &Movie,
    n: Rid,
    code: &bc2_formats::aptmodel::Code,
    s: u32,
    mut pool: Option<Rid>,
    name: &str,
    found: &mut Vec<Func>,
    depth: usize,
) -> MenuResult<()> {
    if depth > 4 * bc2_formats::aptmodel::MAX_NESTING {
        return Err(MenuError::Reference);
    }
    for &k in code.streams.get(s as usize).ok_or(MenuError::Reference)? {
        let i = &code.ins[k as usize];
        if i.op == 0x88 {
            pool = match i.args.get(1) {
                Some(Arg::Ref(r)) => Some(*r),
                _ => None,
            };
        }
        if let Some(b) = i.body {
            let mut bpool = pool;
            if let Some(&f) = code.streams[b as usize].first()
                && code.ins[f as usize].op == 0x88
                && let Some(Arg::Ref(r)) = code.ins[f as usize].args.get(1)
            {
                bpool = Some(*r);
            }
            if i.op == 0x8E && arg_text(m, i.args.first())?.as_deref() == Some(name) {
                found.push(Func {
                    code: n,
                    ins: k,
                    pool: bpool,
                });
            }
            nf_walk(m, n, code, b, bpool, name, found, depth + 1)?;
        }
    }
    Ok(())
}

/// `_pool_name(movie, pool, i)`.
fn pool_name(m: &Movie, pool: Option<Rid>, i: &Ins) -> MenuResult<Option<String>> {
    let Some(p) = pool else { return Ok(None) };
    if !(is_pool_byte(i.op) || i.op == 0xA3) {
        return Ok(None);
    }
    let k = int_arg(i, 0)?;
    let v = m.vals(p)?;
    match usize::try_from(k).ok().and_then(|k| v.get(k)) {
        Some(x) => m.const_text_of(*x).map(Some),
        None => Ok(None),
    }
}

/// `_push_regs(movie, i)`.
fn push_regs(m: &Movie, i: &Ins) -> MenuResult<Option<Vec<u32>>> {
    if i.op != 0x96 {
        return Ok(None);
    }
    let Some(Arg::Ref(r)) = i.args.get(1) else {
        return Ok(None);
    };
    let mut out = Vec::new();
    let mut all = true;
    for x in m.vals(*r)? {
        let Val::Num(k) = x else {
            return Err(MenuError::Reference);
        };
        match m.constant(*k as usize)? {
            CKey::T(4, v) => out.push(v),
            _ => all = false,
        }
    }
    Ok(if !out.is_empty() && all { Some(out) } else { None })
}

/// An entry of `crosshair_table`.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The frame label.
    pub label: Option<String>,
    /// The clip name.
    pub clip: Option<String>,
    /// The index of its first instruction in init's body.
    pub start: Option<usize>,
    /// The index after its SetMember.
    pub end: usize,
    /// The register holding the object.
    pub reg: u32,
}

/// `crosshair_table(movie)`, in table order.
pub fn crosshair_table(m: &Movie) -> MenuResult<Vec<(Option<String>, Entry)>> {
    let f = named_function(m, CH_FUNCTION)?;
    let code = m.code(f.code)?;
    let b = code.ins[f.ins as usize].body.ok_or(MenuError::Reference)?;
    let body: Vec<&Ins> = code.streams[b as usize]
        .iter()
        .map(|&k| &code.ins[k as usize])
        .collect();
    let pool = f.pool;
    let mut out: Vec<(Option<String>, Entry)> = Vec::new();
    let mut last_end = 0usize;
    for j in 0..body.len().saturating_sub(4) {
        let (a, bb, c, d, e) = (body[j], body[j + 1], body[j + 2], body[j + 3], body[j + 4]);
        let rd = push_regs(m, d)?;
        let ok = push_regs(m, a)?.is_some()
            && pool_name(m, pool, bb)?.as_deref() == Some(CH_TABLE)
            && bb.op == 0xAF
            && matches!(c.op, 0xA2 | 0xA3)
            && rd.as_ref().is_some_and(|r| r.len() == 1)
            && e.op == 0x4F;
        if !ok {
            continue;
        }
        let rd0 = rd.map_or(0, |r| r[0]);
        let key = pool_name(m, pool, c)?;
        let mut ent = Entry {
            label: None,
            clip: None,
            start: None,
            end: j + 5,
            reg: rd0,
        };
        let mut k = j as i64 - 8;
        while k >= last_end as i64 {
            let ku = k as usize;
            if ku + 9 <= body.len() {
                let w = &body[ku..ku + 9];
                let hit = matches!(w[1].op, 0xA2 | 0xA3)
                    && push_regs(m, w[2])?.is_some()
                    && w[3].op == 0xB5
                    && int_arg(w[3], 0)? == 3
                    && pool_name(m, pool, w[4])?.as_deref() == Some("dice")
                    && w[4].op == 0xAE
                    && pool_name(m, pool, w[5])?.as_deref() == Some("CrossHair")
                    && w[6].op == 0x53
                    && w[7].op == 0x87
                    && int_arg(w[7], 0)? == i64::from(rd0)
                    && w[8].op == 0x17
                    && value_push(w[0].op);
                if hit {
                    ent.label = pool_name(m, pool, w[1])?;
                    ent.clip = pool_name(m, pool, w[0])?;
                    ent.start = Some(ku);
                    break;
                }
            }
            k -= 1;
        }
        if !out.iter().any(|(x, _)| *x == key) {
            out.push((key, ent));
        }
        last_end = j + 5;
    }
    Ok(out)
}

/// `timeline_display(movie)`: each frame's {depth: the place item showing it}.
pub fn timeline_display(m: &mut Movie) -> MenuResult<Vec<BTreeMap<i64, Rid>>> {
    timeline_display_of(m, None)
}

/// `timeline_display(movie, owner)`: the same for a sprite's timeline (`None`: the movie's).
pub fn timeline_display_of(m: &mut Movie, owner: Option<Rid>) -> MenuResult<Vec<BTreeMap<i64, Rid>>> {
    let mut out = Vec::new();
    let mut disp: BTreeMap<i64, Rid> = BTreeMap::new();
    for f in 0..m.frame_count(owner)? {
        for it in m.frame_items(f, owner)? {
            match m.tag(it)? {
                "remove" => {
                    disp.remove(&signed(m.num(it, 1)?));
                }
                "place" if m.num(it, 1)? & 3 != 0 => {
                    disp.insert(signed(m.num(it, 2)?), it);
                }
                _ => {}
            }
        }
        out.push(disp.clone());
    }
    Ok(out)
}

/// `crosshair_objects(movie, label, baseline)`.
pub fn crosshair_objects(m: &mut Movie, label: &str, baseline: &str) -> MenuResult<Vec<(i64, Rid)>> {
    let disp = timeline_display(m)?;
    let f = m.label_frame(label, None)?;
    let b = m.label_frame(baseline, None)?;
    let objs: Vec<(i64, Rid)> = disp[f]
        .iter()
        .filter(|(d, it)| disp[b].get(d) != Some(it))
        .map(|(d, it)| (*d, *it))
        .collect();
    let mut moves = false;
    for (_, it) in &objs {
        moves |= m.num(*it, 1)? & 2 == 0;
    }
    if objs.is_empty() || moves {
        return Err(apt_err(format!(
            "{}: frame \"{label}\" shows no character of its own (or only moves one)",
            m.label
        )));
    }
    Ok(objs)
}

/// What `add_crosshair` reports.
#[derive(Debug, Clone)]
pub struct Added {
    /// The id.
    pub id: String,
    /// The frame label.
    pub label: String,
    /// The frame.
    pub frame: usize,
    /// src depth -> dst depth, in order.
    pub depths: Vec<(i64, i64)>,
    /// (dst id, kind, src id).
    pub added: Vec<(i64, String, i64)>,
    /// src register -> dst register, in order.
    pub registers: Vec<(i64, i64)>,
    /// register -> the push's name, in order.
    pub values: Vec<(i64, String)>,
    /// Notes.
    pub notes: Vec<String>,
}

/// `add_crosshair(dst, src, cid, frame)` (no image tiles, baseline "none").
pub fn add_crosshair(dst: &mut Movie, src: &mut Movie, cid: &str, frame: Option<i64>) -> MenuResult<Added> {
    add_crosshair_tiles(dst, src, cid, frame, &[])
}

/// `add_crosshair(dst, src, cid, frame, images)` (baseline "none"): `images` = [(src image, its tiles)] in the
/// dict's order - src's textured parts drawing a listed image are split onto its tiles (`split_image_part`) and no
/// image character is added for it.
#[allow(clippy::too_many_lines, reason = "the reference's one function, step for step")]
pub fn add_crosshair_tiles(
    dst: &mut Movie,
    src: &mut Movie,
    cid: &str,
    frame: Option<i64>,
    images: &[(i64, Vec<Tile>)],
) -> MenuResult<Added> {
    let baseline = "none";
    let table = crosshair_table(src)?;
    let Some(ent) = table
        .iter()
        .find(|(k, _)| k.as_deref() == Some(cid))
        .map(|(_, e)| e.clone())
        .filter(|e| e.start.is_some())
    else {
        return Err(apt_err(format!(
            "{}: no crossHairs entry \"{cid}\" of the usual shape",
            src.label
        )));
    };
    if crosshair_table(dst)?.iter().any(|(k, _)| k.as_deref() == Some(cid)) {
        return Err(apt_err(format!("{}: crossHairs has \"{cid}\" already", dst.label)));
    }
    let label = ent.label.clone().ok_or(MenuError::Reference)?;
    let count = dst.frame_count(None)?;
    for f in 0..count {
        for it in dst.frame_items(f, None)? {
            if dst.tag(it)? == "label" && dst.text(dst.val(it, 1)?)?.as_deref() == Some(label.as_str()) {
                return Err(apt_err(format!(
                    "{}: the main timeline has a frame \"{label}\" already",
                    dst.label
                )));
            }
        }
    }
    let objs = crosshair_objects(src, &label, baseline)?;
    let disp = timeline_display(dst)?;
    let base = disp[dst.label_frame(baseline, None)?].clone();
    let free = |dst: &mut Movie, f: i64| -> MenuResult<bool> {
        let Ok(f) = usize::try_from(f) else { return Ok(false) };
        Ok(f + 1 < count
            && dst.frame_items(f, None)?.is_empty()
            && dst.frame_items(f + 1, None)?.is_empty()
            && disp[f] == base)
    };
    let frame = match frame {
        None => {
            let mut hit = None;
            for f in 0..count.saturating_sub(1) {
                if free(dst, f as i64)? {
                    hit = Some(f);
                    break;
                }
            }
            hit.ok_or_else(|| {
                apt_err(format!(
                    "{}: no empty frame (followed by an empty one) showing the baseline frame",
                    dst.label
                ))
            })?
        }
        Some(f) => {
            if !free(dst, f)? {
                return Err(apt_err(format!(
                    "{}: frame {f} is not empty, its next frame is not, or it shows more than the \"{baseline}\" frame",
                    dst.label
                )));
            }
            f as usize
        }
    };
    let mut g = Grafter::new();
    for (img, tiles) in images {
        let Some(t0) = tiles.first() else {
            return Err(apt_err(format!("image {img}: no tiles")));
        };
        g.map(*img, t0.image);
    }
    let mut depths: Vec<(i64, i64)> = Vec::new();
    let mut taken: std::collections::HashSet<i64> = base.keys().copied().collect();
    let mut places = Vec::new();
    for (d, it) in &objs {
        let mut nd = *d;
        while taken.contains(&nd) {
            nd += 1;
        }
        taken.insert(nd);
        depths.push((*d, nd));
        let copy = g.node(src, dst, *it)?;
        dst.set(copy, 2, Val::Num(nd as u32))?;
        places.push(copy);
    }
    for scid in g.cmap_order.clone() {
        let dcid = g.cmap[&scid];
        if src.char_type(scid)? != Some(1) {
            continue;
        }
        let (Some(se), Some(de)) = (src.geo_entry(scid)?, dst.geo_entry(dcid)?) else {
            continue;
        };
        let Val::Ref(sparts) = src.val(se.0, se.1 + 2)? else {
            return Err(MenuError::Reference);
        };
        let Val::Ref(dparts) = dst.val(de.0, de.1 + 2)? else {
            return Err(MenuError::Reference);
        };
        let mut new_parts = Vec::new();
        for p in src.vals(sparts)?.clone() {
            let Val::Ref(pn) = p else {
                return Err(MenuError::Reference);
            };
            let copy = *g.memo.get(&pn).ok_or(MenuError::Reference)?;
            let tiled = if matches!(src.tag(pn)?, "geo.part4" | "geo.part5") {
                let img = src.val(pn, 20)?;
                images
                    .iter()
                    .find(|(k, _)| Val::Num(*k as u32) == img && (0..=i64::from(u32::MAX)).contains(k))
            } else {
                None
            };
            match tiled {
                Some((_, part_tiles)) => {
                    for q in split_image_part(dst, copy, part_tiles)? {
                        new_parts.push(Val::Ref(q));
                    }
                }
                None => new_parts.push(Val::Ref(copy)),
            }
        }
        let n = new_parts.len();
        *dst.vals_mut(dparts)? = new_parts;
        dst.set(de.0, de.1 + 1, Val::Num(n as u32))?;
    }
    // the table entry
    let sfn = named_function(src, CH_FUNCTION)?;
    let dfn = named_function(dst, CH_FUNCTION)?;
    let Some(dpool) = dfn.pool else {
        return Err(apt_err(format!("{}: {CH_FUNCTION} reads no constant pool", dst.label)));
    };
    let scode = src.code(sfn.code)?.clone();
    let mut dcode = dst.code(dfn.code)?.clone();
    let sflags = (int_arg(&scode.ins[sfn.ins as usize], 2)? >> 16) as u32;
    let dargs2 = int_arg(&dcode.ins[dfn.ins as usize], 2)?;
    let dflags = dargs2 >> 16;
    let sroles: HashMap<i64, u32> = PRELOAD
        .iter()
        .filter(|&&f| sflags & f != 0)
        .enumerate()
        .map(|(n, &f)| (n as i64 + 1, f))
        .collect();
    let droles: HashMap<u32, i64> = PRELOAD
        .iter()
        .filter(|&&f| (dflags as u32) & f != 0)
        .enumerate()
        .map(|(n, &f)| (f, n as i64 + 1))
        .collect();
    let sb = scode.ins[sfn.ins as usize].body.ok_or(MenuError::Reference)?;
    let db = dcode.ins[dfn.ins as usize].body.ok_or(MenuError::Reference)?;
    let sbody: Vec<Ins> = scode.streams[sb as usize]
        .iter()
        .map(|&k| scode.ins[k as usize].clone())
        .collect();
    // _value_registers
    let mut sets: Vec<(i64, Vec<usize>)> = Vec::new();
    for (k, i) in sbody.iter().enumerate() {
        if i.op == 0x87 {
            let r = int_arg(i, 0)?;
            match sets.iter_mut().find(|(x, _)| *x == r) {
                Some(e) => e.1.push(k),
                None => sets.push((r, vec![k])),
            }
        }
    }
    let mut values: HashMap<i64, Ins> = HashMap::new();
    for (r, ks) in &sets {
        let k = ks[0];
        if ks.len() == 1 && 0 < k && k + 1 < sbody.len() && value_push(sbody[k - 1].op) && sbody[k + 1].op == 0x17 {
            values.insert(*r, sbody[k - 1].clone());
        }
    }
    let mut dnames: Vec<String> = Vec::new();
    for v in dst.vals(dpool)?.clone() {
        dnames.push(dst.const_text_of(v)?);
    }
    let mut nregs = dargs2 & 0xFFFF;
    let mut fresh: Vec<(i64, i64)> = Vec::new();
    let mut used_values: Vec<(i64, String)> = Vec::new();
    let spool = sfn.pool;
    let mut ctx = Ctx {
        dnames: &mut dnames,
        dpool,
    };
    let mut block: Vec<Ins> = Vec::new();
    let (s_start, s_end) = (ent.start.unwrap_or(0), ent.end.min(sbody.len()));
    for i in &sbody[s_start.min(s_end)..s_end] {
        if i.op == 0x87 {
            let a = int_arg(i, 0)?;
            if !fresh.iter().any(|(x, _)| *x == a) {
                fresh.push((a, nregs));
                nregs += 1;
            }
            let v = fresh.iter().find(|(x, _)| *x == a).map_or(0, |e| e.1);
            block.push(ins(0x87, vec![Arg::Int(v)]));
        } else if i.op == 0xB9 {
            match read(
                src,
                dst,
                &mut ctx,
                cid,
                int_arg(i, 0)?,
                &fresh,
                &sroles,
                &droles,
                &values,
                &mut used_values,
                spool,
            )? {
                Read::Ins(x) => block.push(x),
                Read::Reg(r) => block.push(ins(0xB9, vec![Arg::Int(r)])),
            }
        } else if let (0x96, Some(Arg::Ref(lst))) = (i.op, i.args.get(1)) {
            let mut run: Vec<Val> = Vec::new();
            for k in src.vals(*lst)?.clone() {
                let Val::Num(k) = k else {
                    return Err(MenuError::Reference);
                };
                let c = src.constant(k as usize)?;
                let r = match c {
                    CKey::T(4, x) => Some(read(
                        src,
                        dst,
                        &mut ctx,
                        cid,
                        i64::from(x),
                        &fresh,
                        &sroles,
                        &droles,
                        &values,
                        &mut used_values,
                        spool,
                    )?),
                    _ => None,
                };
                match r {
                    Some(Read::Ins(x)) => {
                        if !run.is_empty() {
                            let n = run.len();
                            let pl = dst.new_words(APT, std::mem::take(&mut run), "pushlist");
                            block.push(ins(0x96, vec![Arg::Int(n as i64), Arg::Ref(pl)]));
                        }
                        block.push(x);
                    }
                    Some(Read::Reg(r)) => run.push(Val::Num(dst.const_index(&CKey::T(4, r as u32))? as u32)),
                    None => run.push(Val::Num(dst.const_index(&c)? as u32)),
                }
            }
            if !run.is_empty() {
                let n = run.len();
                let pl = dst.new_words(APT, run, "pushlist");
                block.push(ins(0x96, vec![Arg::Int(n as i64), Arg::Ref(pl)]));
            }
        } else {
            block.push(copy_ins(src, dst, &mut ctx, cid, i, spool)?);
        }
    }
    if nregs > 255 {
        return Err(apt_err(format!("{CH_FUNCTION}: more than 255 registers")));
    }
    let after = crosshair_table(dst)?
        .iter()
        .map(|(_, e)| e.end)
        .max()
        .ok_or(MenuError::Reference)?;
    dcode.ins[dfn.ins as usize].args[2] = Arg::Int(nregs | dflags << 16);
    let ks: Vec<u32> = block.into_iter().map(|b| push_ins(&mut dcode, b)).collect();
    let body = &mut dcode.streams[db as usize];
    let at = after.min(body.len());
    body.splice(at..at, ks);
    *dst.code_mut(dfn.code)? = dcode;
    // the frame's items
    let bf = dst.label_frame(baseline, None)?;
    let mut flags = None;
    for it in dst.frame_items(bf, None)? {
        if dst.tag(it)? == "label" {
            flags = Some(dst.val(it, 2)?);
            break;
        }
    }
    let flags = flags.ok_or(MenuError::Reference)?;
    let s = dst.new_str(APT, latin1_encode(&label)?);
    let w = dst.new_words(
        APT,
        vec![Val::Num(2), Val::Ref(s), flags, Val::Num(frame as u32)],
        "label",
    );
    dst.add_item(frame, w, None, None)?;
    for p in places {
        dst.add_item(frame, p, None, None)?;
    }
    let mut nds: Vec<i64> = depths.iter().map(|d| d.1).collect();
    nds.sort_unstable();
    for nd in nds {
        let r = dst.new_words(APT, vec![Val::Num(4), Val::Num(nd as u32)], "remove");
        dst.add_item(frame + 1, r, None, None)?;
    }
    Ok(Added {
        id: cid.to_owned(),
        label,
        frame,
        depths,
        added: g.added,
        registers: fresh,
        values: used_values,
        notes: g.notes,
    })
}

struct Ctx<'a> {
    dnames: &'a mut Vec<String>,
    dpool: Rid,
}

enum Read {
    Ins(Ins),
    Reg(i64),
}

fn pool_index(dst: &mut Movie, ctx: &mut Ctx<'_>, name: &str, op: u8) -> MenuResult<Ins> {
    if !ctx.dnames.iter().any(|n| n == name) {
        let k = dst.const_index(&CKey::S(latin1_encode(name)?))?;
        dst.vals_mut(ctx.dpool)?.push(Val::Num(k as u32));
        ctx.dnames.push(name.to_owned());
    }
    let k = ctx.dnames.iter().position(|n| n == name).unwrap_or(0);
    if matches!(op, 0xA2 | 0xA3) {
        return Ok(ins(if k < 256 { 0xA2 } else { 0xA3 }, vec![Arg::Int(k as i64)]));
    }
    if k >= 256 {
        return Err(apt_err("the constant pool is too long for a byte index"));
    }
    Ok(ins(op, vec![Arg::Int(k as i64)]))
}

fn copy_ins(
    src: &Movie,
    dst: &mut Movie,
    ctx: &mut Ctx<'_>,
    cid: &str,
    i: &Ins,
    spool: Option<Rid>,
) -> MenuResult<Ins> {
    if let Some(name) = pool_name(src, spool, i)? {
        return pool_index(dst, ctx, &name, i.op);
    }
    if is_one_str(i.op) || i.op == 0x83 {
        let mut args = Vec::with_capacity(i.args.len());
        for a in &i.args {
            args.push(match a {
                Arg::Ref(r) => {
                    let t = src.str_text(*r)?.to_vec();
                    Arg::Ref(dst.new_str(APT, t))
                }
                _ => *a,
            });
        }
        return Ok(ins(i.op, args));
    }
    if is_branch(i.op) || matches!(i.op, 0x8E | 0x9B | 0x88) {
        return Err(apt_err(format!(
            "{cid}: the entry's code has a {} (not copied)",
            op_display(i.op)
        )));
    }
    Ok(ins(i.op, i.args.clone()))
}

#[allow(clippy::too_many_arguments, reason = "the reference's closure state")]
fn read(
    src: &Movie,
    dst: &mut Movie,
    ctx: &mut Ctx<'_>,
    cid: &str,
    r: i64,
    fresh: &[(i64, i64)],
    sroles: &HashMap<i64, u32>,
    droles: &HashMap<u32, i64>,
    values: &HashMap<i64, Ins>,
    used: &mut Vec<(i64, String)>,
    spool: Option<Rid>,
) -> MenuResult<Read> {
    if let Some(&(_, v)) = fresh.iter().find(|(x, _)| *x == r) {
        return Ok(Read::Reg(v));
    }
    if let Some(&role) = sroles.get(&r) {
        let Some(&d) = droles.get(&role) else {
            return Err(apt_err(format!(
                "{cid}: register {r} holds what {}'s {CH_FUNCTION} does not preload (flag {role:#x})",
                dst.label
            )));
        };
        return Ok(Read::Reg(d));
    }
    if let Some(v) = values.get(&r) {
        let name = op_display(v.op);
        match used.iter_mut().find(|(x, _)| *x == r) {
            Some(e) => e.1 = name,
            None => used.push((r, name)),
        }
        return Ok(Read::Ins(copy_ins(src, dst, ctx, cid, v, spool)?));
    }
    Err(apt_err(format!(
        "{cid}: register {r} is read before the entry sets it and holds no constant"
    )))
}
