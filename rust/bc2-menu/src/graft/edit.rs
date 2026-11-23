//! The edits of `aptgraft.py`'s commands: `add_mode_case`, `add_mode_alias`, `add_label_alias`, `deps` and
//! `onslaught_timer` (with its extern checks and the PC fit), as the reference makes them.

#![allow(
    clippy::indexing_slicing,
    reason = "each index is checked just before it, as the reference indexes"
)]

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::error::{MenuError, MenuResult};
use bc2_formats::aptmodel::{Arg, Code, Content, Rid, Target, Val};

use super::{
    APT, Builder, CKey, Grafter, Movie, apt_err, char_type_name, f32_bits, free_depths, ins, int_arg, is_branch,
    is_pool_byte, latin1, latin1_encode, op_display, push_ins, signed, walk_ins,
};

/// `text(arg)` of an instruction's argument.
pub fn arg_text(m: &Movie, a: Option<&Arg>) -> MenuResult<Option<String>> {
    match a {
        Some(Arg::Ref(r)) => Ok(Some(latin1(m.str_text(*r)?))),
        Some(Arg::Int(_)) => Ok(None),
        _ => Err(MenuError::Reference),
    }
}

/// The pool node an instruction's second argument points at (`i.args[1].node`).
fn ref_arg(i: &bc2_formats::aptmodel::Ins, k: usize) -> MenuResult<Rid> {
    match i.args.get(k) {
        Some(Arg::Ref(r)) => Ok(*r),
        _ => Err(MenuError::Reference),
    }
}

fn pool_names(m: &Movie, pool: Rid) -> MenuResult<Vec<String>> {
    m.vals(pool)?.iter().map(|v| m.const_text_of(*v)).collect()
}

/// `pc(t)` of the mode switches: the pool index of `t` (added to the pool and the names when missing).
fn pc_ins(m: &mut Movie, names: &mut Vec<String>, pool: Rid, t: &str) -> MenuResult<bc2_formats::aptmodel::Ins> {
    if !names.iter().any(|n| n == t) {
        let k = m.const_index(&CKey::S(latin1_encode(t)?))?;
        m.vals_mut(pool)?.push(Val::Num(k as u32));
        names.push(t.to_owned());
    }
    let k = names.iter().position(|n| n == t).unwrap_or(0);
    Ok(ins(if k < 256 { 0xA2 } else { 0xA3 }, vec![Arg::Int(k as i64)]))
}

fn names_at(names: &[String], i: &bc2_formats::aptmodel::Ins) -> MenuResult<Option<String>> {
    let k = int_arg(i, 0)?;
    Ok(usize::try_from(k).ok().and_then(|k| names.get(k)).cloned())
}

/// `add_mode_case(movie, func, mode, key, fallback)`: the number of functions patched.
pub fn add_mode_case(m: &mut Movie, func: &str, mode: &str, key: &str, fallback: &str) -> MenuResult<usize> {
    let mut done = 0;
    for n in m.code_nodes(false)? {
        let mut code = m.code(n)?.clone();
        let top = code.streams.first().cloned().unwrap_or_default();
        let mut pool: Option<Rid> = None;
        let mut changed = false;
        for &t in &top {
            let i = code.ins[t as usize].clone();
            if i.op == 0x88 {
                pool = Some(ref_arg(&i, 1)?);
            }
            if i.op != 0x8E || arg_text(m, i.args.first())?.as_deref() != Some(func) {
                continue;
            }
            let Some(pool) = pool else { continue };
            let mut names = pool_names(m, pool)?;
            let body = i.body.ok_or(MenuError::Reference)?;
            if walk_ins(&code, body).iter().any(|&k| code.ins[k as usize].op == 0x88) {
                return Err(apt_err(format!(
                    "{func}: the function sets its own constant pool (not supported)"
                )));
            }
            let lst = code.streams[body as usize].clone();
            let mut idx = None;
            for (j, &k) in lst.iter().enumerate() {
                let x = &code.ins[k as usize];
                if matches!(x.op, 0xA2 | 0xA3) && names_at(&names, x)?.as_deref() == Some(fallback) {
                    idx = Some(j);
                    break;
                }
            }
            let Some(idx) = idx.filter(|&j| j + 5 < lst.len()) else {
                return Err(apt_err(format!("{func}: no fallback \"{fallback}\"")));
            };
            let [var, add, setr, pop, br] = [1, 2, 3, 4, 5].map(|d| code.ins[lst[idx + d] as usize].clone());
            if (var.op, add.op, setr.op, pop.op, br.op) != (0xAE, 0x47, 0x87, 0x17, 0x99) {
                return Err(apt_err(format!("{func}: the fallback is not the expected shape")));
            }
            let fb = lst[idx];
            let pm = pc_ins(m, &mut names, pool, mode)?;
            let pk = pc_ins(m, &mut names, pool, key)?;
            let mut block = vec![
                ins(0xAE, var.args.clone()),
                pm,
                ins(0x66, Vec::new()),
                ins(0x12, Vec::new()),
                ins(0x9D, Vec::new()),
                pk,
                ins(0x87, setr.args.clone()),
                ins(0x17, Vec::new()),
                ins(0x99, Vec::new()),
            ];
            block[4].target = Target::Ins(fb);
            block[8].target = br.target;
            if names.len() > 256 {
                for b in &block {
                    if is_pool_byte(b.op) && int_arg(b, 0)? >= 256 {
                        return Err(apt_err("constant pool too long for a byte index"));
                    }
                }
            }
            let ks: Vec<u32> = block.into_iter().map(|b| push_ins(&mut code, b)).collect();
            for k in walk_ins(&code, body) {
                let x = &mut code.ins[k as usize];
                if is_branch(x.op) && x.target == Target::Ins(fb) && !ks.contains(&k) {
                    x.target = Target::Ins(ks[0]);
                }
            }
            code.streams[body as usize].splice(idx..idx, ks);
            done += 1;
            changed = true;
        }
        if changed {
            *m.code_mut(n)? = code;
        }
    }
    if done == 0 {
        return Err(apt_err(format!("{}: no function {func}", m.label)));
    }
    Ok(done)
}

/// `add_mode_alias(movie, mode, like, funcs)`: the number of switches patched.
pub fn add_mode_alias(m: &mut Movie, mode: &str, like: &str, funcs: Option<&[String]>) -> MenuResult<usize> {
    let mut st = Alias {
        mode,
        like,
        funcs,
        done: 0,
        seen: 0,
        lists: Vec::new(),
    };
    for n in m.code_nodes(false)? {
        let mut code = m.code(n)?.clone();
        st.lists.clear();
        st.walk(m, &mut code, 0, None, None, 0)?;
        if *m.code(n)? != code {
            *m.code_mut(n)? = code;
        }
    }
    if st.done == 0 && st.seen == 0 {
        let within = funcs.map_or(String::new(), |f| format!(" in {}", f.join(", ")));
        return Err(apt_err(format!("{}: no \"{like}\" switch{within}", m.label)));
    }
    Ok(st.done)
}

struct Alias<'a> {
    mode: &'a str,
    like: &'a str,
    funcs: Option<&'a [String]>,
    done: usize,
    seen: usize,
    /// the name lists, by identity (two bodies can share the enclosing one)
    lists: Vec<Vec<String>>,
}

impl Alias<'_> {
    fn new_list(&mut self, m: &Movie, pool: Rid) -> MenuResult<usize> {
        self.lists.push(pool_names(m, pool)?);
        Ok(self.lists.len() - 1)
    }

    fn walk(
        &mut self,
        m: &mut Movie,
        code: &mut Code,
        s: u32,
        mut pool: Option<Rid>,
        mut names: Option<usize>,
        depth: usize,
    ) -> MenuResult<()> {
        if depth > 4 * bc2_formats::aptmodel::MAX_NESTING {
            return Err(MenuError::Reference);
        }
        let lst = code.streams.get(s as usize).cloned().ok_or(MenuError::Reference)?;
        for k in lst {
            let i = code.ins[k as usize].clone();
            if i.op == 0x88 {
                let p = ref_arg(&i, 1)?;
                pool = Some(p);
                names = Some(self.new_list(m, p)?);
            }
            let Some(b) = i.body else { continue };
            let (mut bpool, mut bnames) = (pool, names);
            if let Some(&f) = code.streams[b as usize].first()
                && code.ins[f as usize].op == 0x88
            {
                let p = ref_arg(&code.ins[f as usize].clone(), 1)?;
                bpool = Some(p);
                bnames = Some(self.new_list(m, p)?);
            }
            let name = if i.op == 0x8E {
                arg_text(m, i.args.first())?
            } else {
                Some(String::new())
            };
            if let (Some(p), Some(nl)) = (bpool, bnames)
                && self.funcs.is_none_or(|f| name.as_ref().is_some_and(|n| f.contains(n)))
            {
                self.patch(m, code, b, nl, p)?;
            }
            self.walk(m, code, b, bpool, bnames, depth + 1)?;
        }
        Ok(())
    }

    fn is_name(&self, nl: usize, i: &bc2_formats::aptmodel::Ins, want: &str) -> MenuResult<bool> {
        Ok(matches!(i.op, 0xA2 | 0xA3) && names_at(&self.lists[nl], i)?.as_deref() == Some(want))
    }

    fn patch(&mut self, m: &mut Movie, code: &mut Code, b: u32, nl: usize, pool: Rid) -> MenuResult<()> {
        let mut j = 0;
        while j + 3 < code.streams[b as usize].len() {
            let body = code.streams[b as usize].clone();
            let [v, c, eq, br] = [0, 1, 2, 3].map(|d| code.ins[body[j + d] as usize].clone());
            let vk = body[j];
            let shape = self.is_name(nl, &c, self.like)?
                && matches!(eq.op, 0x66 | 0x49)
                && br.op == 0x9D
                && (matches!(v.op, 0xAE | 0x87)
                    || (v.op == 0x96 && int_arg(&v, 0)? == 1 && matches!(v.args.get(1), Some(Arg::Ref(_)))));
            if !shape {
                j += 1;
                continue;
            }
            let after = v.op == 0x87;
            let there = if after {
                j + 5 < body.len() && self.is_name(nl, &code.ins[body[j + 5] as usize].clone(), self.mode)?
            } else {
                j >= 4 && self.is_name(nl, &code.ins[body[j - 3] as usize].clone(), self.mode)?
            };
            if there {
                self.seen += 1;
                j += 4;
                continue;
            }
            let first = if after {
                let r = int_arg(&v, 0)?;
                let k = m.const_index(&CKey::T(4, r as u32))?;
                let pl = m.new_words(APT, vec![Val::Num(k as u32)], "pushlist");
                ins(0x96, vec![Arg::Int(1), Arg::Ref(pl)])
            } else if v.op == 0xAE {
                ins(0xAE, v.args.clone())
            } else {
                let vals = m.vals(ref_arg(&v, 1)?)?.clone();
                let pl = m.new_words(APT, vals, "pushlist");
                ins(0x96, vec![Arg::Int(1), Arg::Ref(pl)])
            };
            let mut names = std::mem::take(&mut self.lists[nl]);
            let pm = pc_ins(m, &mut names, pool, self.mode);
            self.lists[nl] = names;
            let mut target = ins(0x9D, Vec::new());
            target.target = br.target;
            let block = [first, pm?, ins(eq.op, Vec::new()), target];
            if self.lists[nl].len() > 256 {
                for x in &block {
                    if is_pool_byte(x.op) && int_arg(x, 0)? >= 256 {
                        return Err(apt_err("constant pool too long for a byte index"));
                    }
                }
            }
            let ks: Vec<u32> = block.into_iter().map(|x| push_ins(code, x)).collect();
            if after {
                code.streams[b as usize].splice(j + 4..j + 4, ks);
            } else {
                for k in walk_ins(code, b) {
                    let x = &mut code.ins[k as usize];
                    if is_branch(x.op) && x.target == Target::Ins(vk) {
                        x.target = Target::Ins(ks[0]);
                    }
                }
                code.streams[b as usize].splice(j..j, ks);
            }
            self.done += 1;
            j += 8;
        }
        Ok(())
    }
}

/// The sprites' character ids and nodes: `[movie.char(c) for c in range(...) if movie.char_type(c) == 5]`.
pub fn sprite_owners(m: &Movie) -> MenuResult<Vec<Option<Rid>>> {
    let mut out = vec![None];
    for c in 0..m.char_count()? as i64 {
        if m.char_type(c)? == Some(5) {
            out.push(m.char(c)?);
        }
    }
    Ok(out)
}

/// `add_label_alias(movie, like, alias)`: the number of labels added.
pub fn add_label_alias(m: &mut Movie, like: &str, alias: &str) -> MenuResult<usize> {
    let (mut added, mut found) = (0, 0);
    for owner in sprite_owners(m)? {
        let mut labels: HashMap<Option<String>, (usize, Rid)> = HashMap::new();
        for f in 0..m.frame_count(owner)? {
            for it in m.frame_items(f, owner)? {
                if m.tag(it)? == "label" {
                    labels.entry(m.text(m.val(it, 1)?)?).or_insert((f, it));
                }
            }
        }
        let Some(&(f, it)) = labels.get(&Some(like.to_owned())) else {
            continue;
        };
        found += 1;
        if labels.contains_key(&Some(alias.to_owned())) {
            continue;
        }
        let s = m.new_str(APT, latin1_encode(alias)?);
        let (a, b) = (m.val(it, 2)?, m.val(it, 3)?);
        let w = m.new_words(APT, vec![Val::Num(2), Val::Ref(s), a, b], "label");
        m.add_item(f, w, owner, None)?;
        added += 1;
    }
    if found == 0 {
        return Err(apt_err(format!("{}: no frame label \"{like}\"", m.label)));
    }
    Ok(added)
}

/// `deps(movie, cid)`.
#[derive(Debug, Default)]
pub struct Deps {
    /// id -> type name.
    pub chars: BTreeMap<i64, String>,
    /// (movie, name, id).
    pub imports: Vec<(Option<String>, Option<String>, i64)>,
    /// (id, name).
    pub fonts: Vec<(i64, Option<String>)>,
    /// image ids.
    pub images: Vec<i64>,
    /// shape ids with geometry.
    pub geometry: Vec<i64>,
    /// constant indices.
    pub constants: BTreeSet<u32>,
}

/// `deps(movie, cid)`.
pub fn deps(m: &Movie, cid: i64) -> MenuResult<Deps> {
    let mut d = Deps::default();
    let mut imps: HashMap<u32, (Option<String>, Option<String>)> = HashMap::new();
    for (mv, n, c) in m.imports()? {
        let Val::Num(c) = c else {
            return Err(MenuError::Reference);
        };
        imps.insert(c, (mv, n));
    }
    let mut seen = HashSet::new();
    visit_char(m, cid, &mut d, &imps, &mut seen, 0)?;
    Ok(d)
}

fn visit_char(
    m: &Movie,
    c: i64,
    d: &mut Deps,
    imps: &HashMap<u32, (Option<String>, Option<String>)>,
    seen: &mut HashSet<Rid>,
    depth: usize,
) -> MenuResult<()> {
    let c = if (0..=i64::from(u32::MAX)).contains(&c) {
        signed(c as u32)
    } else {
        c
    };
    if d.chars.contains_key(&c) || d.imports.iter().any(|i| i.2 == c) {
        return Ok(());
    }
    let Some(node) = m.char(c)? else {
        if let Some((mv, n)) = u32::try_from(c).ok().and_then(|k| imps.get(&k)) {
            d.imports.push((mv.clone(), n.clone(), c));
        }
        return Ok(());
    };
    let t = m.num(node, 0)?;
    d.chars.insert(c, char_type_name(t));
    if t == 3 {
        d.fonts.push((c, m.text(m.val(node, 2)?)?));
    }
    if t == 7 {
        d.images.push(c);
    }
    if t == 1
        && let Some((tab, k)) = m.geo_entry(c)?
    {
        d.geometry.push(c);
        let Val::Ref(p) = m.val(tab, k + 2)? else {
            return Err(MenuError::Reference);
        };
        visit(m, p, d, imps, seen, depth + 1)?;
    }
    visit(m, node, d, imps, seen, depth + 1)
}

fn visit(
    m: &Movie,
    n: Rid,
    d: &mut Deps,
    imps: &HashMap<u32, (Option<String>, Option<String>)>,
    seen: &mut HashSet<Rid>,
    depth: usize,
) -> MenuResult<()> {
    // Python's recursion limit (1000 frames, two per level here)
    if depth > 400 {
        return Err(MenuError::Reference);
    }
    if !seen.insert(n) {
        return Ok(());
    }
    if let Content::Words(v) = &m.node(n)?.content {
        let num = |k: usize| -> MenuResult<i64> {
            match v.get(k) {
                Some(Val::Num(x)) => Ok(i64::from(*x)),
                _ => Err(MenuError::Reference),
            }
        };
        match m.tag(n)? {
            "place" if num(1)? & 2 != 0 => visit_char(m, num(3)?, d, imps, seen, depth + 1)?,
            "initaction" => visit_char(m, num(1)?, d, imps, seen, depth + 1)?,
            "text" => visit_char(m, num(6)?, d, imps, seen, depth + 1)?,
            "stext.records" => {
                let mut k = 0;
                while k < v.len() {
                    visit_char(m, num(k)?, d, imps, seen, depth + 1)?;
                    k += 14;
                }
            }
            "geo.part4" | "geo.part5" => visit_char(m, num(20)?, d, imps, seen, depth + 1)?,
            "pushlist" | "poollist" => {
                for x in v {
                    let Val::Num(k) = x else {
                        return Err(MenuError::Reference);
                    };
                    d.constants.insert(*k);
                }
            }
            _ => {}
        }
    }
    for r in m.refs(n)? {
        visit(m, r, d, imps, seen, depth + 1)?;
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------------ the timer

const CON_TIMER: i64 = 184;
const CON_LMS: i64 = 146;
const CON_SNIPPETS: [(&str, (i64, i64, i64)); 7] = [
    ("timer_events", (0x3E964, 0x3EC5A, 0x3ECD5)),
    ("lms_events", (0x3E964, 0x3F747, 0x3F79E)),
    ("lms_init", (0x3E964, 0x3EAA7, 0x3EAAB)),
    ("frame0_hide", (0x300DC, 0x300FD, 0x3011B)),
    ("frame0_hide_timer", (0x300DC, 0x30104, 0x3011B)),
    ("playing_vis", (0x3FAD8, 0x3FAE4, 0x3FB2B)),
    ("playing_vis_timer", (0x3FAD8, 0x3FB12, 0x3FB2B)),
];
const EXTERN_FIX: [(&str, u8, &str); 2] = [
    (
        "coopOldBestRoundTime",
        0x76,
        "undefined: \"best\" shows \"-\" until the coopOldBestRoundTime event",
    ),
    (
        "isScreenHD",
        0x73,
        "true: the full time (the PC HUD is 1280 x 720; no PC movie reads isScreenHD)",
    ),
];
const PC_LABEL: &str = "BEST";

/// `find_place(movie, frame, name, depth)`.
pub fn find_place(m: &mut Movie, frame: usize, name: Option<&str>, depth: Option<i64>) -> MenuResult<Rid> {
    for it in m.frame_items(frame, None)? {
        if m.tag(it)? != "place" {
            continue;
        }
        if let Some(n) = name
            && m.text(m.val(it, 13)?)?.as_deref() != Some(n)
        {
            continue;
        }
        if let Some(d) = depth
            && signed(m.num(it, 2)?) != d
        {
            continue;
        }
        return Ok(it);
    }
    let what = name.map_or_else(|| depth.map_or("None".to_owned(), |d| d.to_string()), str::to_owned);
    Err(apt_err(format!("{}: no placement {what} in frame {frame}", m.label)))
}

/// `extern_reads(movie, code)`: (stream, index of the push before, name).
pub fn extern_reads(m: &Movie, code: &Code) -> MenuResult<Vec<(u32, usize, String)>> {
    let mut out = Vec::new();
    ext_walk(m, code, 0, &[], &mut out, &mut 0, 0)?;
    Ok(out)
}

/// `extern_mentions(movie, code)`.
pub fn extern_mentions(m: &Movie, code: &Code) -> MenuResult<usize> {
    let mut n = 0;
    ext_walk(m, code, 0, &[], &mut Vec::new(), &mut n, 0)?;
    Ok(n)
}

fn ext_walk(
    m: &Movie,
    code: &Code,
    s: u32,
    pool: &[String],
    out: &mut Vec<(u32, usize, String)>,
    mentions: &mut usize,
    depth: usize,
) -> MenuResult<()> {
    if depth > 4 * bc2_formats::aptmodel::MAX_NESTING {
        return Err(MenuError::Reference);
    }
    let lst = code.streams.get(s as usize).ok_or(MenuError::Reference)?;
    let mut pool: Vec<String> = pool.to_vec();
    for (k, &ik) in lst.iter().enumerate() {
        let i = &code.ins[ik as usize];
        if i.op == 0x88 {
            pool = match i.args.get(1) {
                Some(Arg::Ref(r)) => pool_names(m, *r)?,
                _ => Vec::new(),
            };
        }
        let at = |x: &bc2_formats::aptmodel::Ins| -> Option<String> {
            match x.args.first() {
                Some(Arg::Int(v)) => usize::try_from(*v).ok().and_then(|v| pool.get(v)).cloned(),
                _ => None,
            }
        };
        if i.op == 0xAF && k > 0 && k + 1 < lst.len() {
            let nx = &code.ins[lst[k + 1] as usize];
            if nx.op == 0xAF && at(i).as_deref() == Some("extern") {
                if let Some(name) = at(nx) {
                    out.push((s, k - 1, name));
                } else if !matches!(nx.args.first(), Some(Arg::Int(_))) {
                    return Err(MenuError::Reference);
                }
            }
        }
        if (is_pool_byte(i.op) || i.op == 0xA3) && at(i).as_deref() == Some("extern") {
            *mentions += 1;
        }
        if let Some(b) = i.body {
            ext_walk(m, code, b, &pool, out, mentions, depth + 1)?;
        }
    }
    Ok(())
}

/// `replace_extern(lst, k, new)`.
pub fn replace_extern(m: &Movie, code: &mut Code, s: u32, k: usize, op: u8) -> MenuResult<()> {
    let lst = code.streams[s as usize].clone();
    let first = &code.ins[lst[k] as usize];
    let ok = first.op == 0x71
        || (first.op == 0x96
            && match first.args.get(1) {
                Some(Arg::Ref(r)) => m.vals(*r)?.len() == 1,
                _ => false,
            });
    if !ok {
        return Err(apt_err(format!(
            "unexpected instruction {} before an extern read",
            op_display(first.op)
        )));
    }
    let old: Vec<u32> = lst[k..(k + 3).min(lst.len())].to_vec();
    let new = push_ins(code, ins(op, Vec::new()));
    code.streams[s as usize].splice(k..(k + 3).min(lst.len()), [new]);
    for &x in &code.streams[s as usize].clone() {
        let i = &mut code.ins[x as usize];
        if is_branch(i.op)
            && let Target::Ins(t) = i.target
            && old.contains(&t)
        {
            i.target = Target::Ins(new);
        }
    }
    Ok(())
}

/// `place_named(movie, owner, name)`.
fn place_named(m: &mut Movie, owner: Option<Rid>, name: &str) -> MenuResult<Rid> {
    for f in 0..m.frame_count(owner)? {
        for it in m.frame_items(f, owner)? {
            if m.tag(it)? == "place" && m.text(m.val(it, 13)?)?.as_deref() == Some(name) {
                return Ok(it);
            }
        }
    }
    Err(apt_err(format!("no placement \"{name}\" in the character")))
}

/// `set_scale(item, sx, sy, x)`.
fn set_scale(m: &mut Movie, item: Rid, sx: Option<f64>, sy: Option<f64>, x: Option<f64>) -> MenuResult<()> {
    for (k, v) in [(4, sx), (7, sy), (8, x)] {
        if let Some(v) = v {
            let b = f32_bits(v)?;
            m.set(item, k, Val::Num(b))?;
        }
    }
    Ok(())
}

/// `set_clip_props(item, propText=text, propLocalized=localized)`.
fn set_clip_props(m: &mut Movie, item: Rid, text: &str, localized: bool) -> MenuResult<()> {
    let Val::Ref(clip) = m.val(item, 15)? else {
        return Err(apt_err("the placement has no clip events"));
    };
    let Val::Ref(ev) = m.val(clip, 1)? else {
        return Err(MenuError::Reference);
    };
    let n = m.vals(ev)?.len();
    let mut k = 0;
    while k < n {
        let Val::Ref(c) = m.val(ev, k + 2)? else {
            return Err(MenuError::Reference);
        };
        let mut code = m.code(c)?.clone();
        let lst = code.streams.first().cloned().unwrap_or_default();
        for j in 0..lst.len().saturating_sub(1) {
            let i = &code.ins[lst[j] as usize];
            if i.op != 0xA1 {
                continue;
            }
            let Some(prop) = arg_text(m, i.args.first())? else {
                continue;
            };
            let nx = lst[j + 1] as usize;
            match prop.as_str() {
                "propLocalized" if matches!(code.ins[nx].op, 0x73 | 0x74) => {
                    code.ins[nx].op = if localized { 0x73 } else { 0x74 };
                }
                "propText" if code.ins[nx].op == 0xA6 => {
                    let s = m.new_str(APT, latin1_encode(text)?);
                    code.ins[nx].args = vec![Arg::Ref(s)];
                }
                "propLocalized" | "propText" => {
                    return Err(apt_err(format!("unexpected code setting {prop}")));
                }
                _ => {}
            }
        }
        *m.code_mut(c)? = code;
        k += 3;
    }
    Ok(())
}

fn action(m: &mut Movie, code: Rid) -> Rid {
    m.new_words(APT, vec![Val::Num(1), Val::Ref(code)], "action")
}

/// `onslaught_timer(pc, con, lms, place)`: the report lines.
#[allow(clippy::too_many_lines, reason = "the reference's one function, step for step")]
pub fn onslaught_timer(pc: &mut Movie, con: &mut Movie, lms: bool, place: bool) -> MenuResult<Vec<String>> {
    let mut pc_externs = HashSet::new();
    for n in pc.code_nodes(false)? {
        for (_, _, name) in extern_reads(pc, pc.code(n)?)? {
            pc_externs.insert(name);
        }
    }
    if pc.big || !con.big {
        return Err(apt_err(
            "give the PC movie (little-endian) first, then the console one (big-endian)",
        ));
    }
    for cid in [CON_TIMER, CON_LMS] {
        if con.char_type(cid)? != Some(5) {
            return Err(apt_err(format!(
                "{}: character {cid} is not a sprite - not the expected console movie",
                con.label
            )));
        }
    }
    let mut con_place0: HashMap<Option<String>, Rid> = HashMap::new();
    for i in con.frame_items(0, None)? {
        if con.tag(i)? == "place" {
            con_place0.insert(con.text(con.val(i, 13)?)?, i);
        }
    }
    let Some(&lms_place) = con_place0.get(&Some("lastManStanding".to_owned())) else {
        return Err(apt_err(format!(
            "{}: frame 0 places no lastManStanding - not the expected console movie",
            con.label
        )));
    };
    for (_, (stream, _, _)) in CON_SNIPPETS {
        if !con.m.cache.contains_key(&(APT, stream, 3)) {
            return Err(apt_err(format!(
                "{}: no action stream at {stream:#x} - not the expected console movie",
                con.label
            )));
        }
    }
    let mut names = HashSet::new();
    for f in 0..pc.frame_count(None)? {
        for i in pc.frame_items(f, None)? {
            if pc.tag(i)? == "place" {
                names.insert(pc.text(pc.val(i, 13)?)?);
            }
        }
    }
    for inst in ["onslaught_timer", "lastManStanding"] {
        if names.contains(&Some(inst.to_owned())) {
            return Err(apt_err(format!("{} already has an instance \"{inst}\"", pc.label)));
        }
    }
    let mut g = Grafter::new();
    let mut report = Vec::new();
    let mut labels: HashMap<&str, usize> = HashMap::new();
    for lab in [
        "init",
        "playing",
        "playing_empty",
        "playing_hardcore",
        "notplaying",
        "killed",
        "menu",
    ] {
        let f = pc.label_frame(lab, None)?;
        labels.insert(lab, f);
        let cf = con.label_frame(lab, None)?;
        if cf != f {
            return Err(apt_err(format!(
                "frame label {lab}: frame {f} on PC, {cf} on the console"
            )));
        }
    }
    let d = free_depths(pc, 3, 94)?;
    let (d_play, d_kill, d_lms) = (d[0], d[1], d[2]);
    report.push(format!(
        "depths on the PC timeline: timer {d_play} (playing), {d_kill} (killed), banner {d_lms}"
    ));
    let snippet = |g: &mut Grafter, pc: &mut Movie, con: &Movie, key: &str| -> MenuResult<Rid> {
        let (stream, first, end) = CON_SNIPPETS
            .iter()
            .find(|s| s.0 == key)
            .map(|s| s.1)
            .ok_or(MenuError::Reference)?;
        let code = *con.m.cache.get(&(APT, stream, 3)).ok_or(MenuError::Reference)?;
        g.code_range(con, pc, code, first, end)
    };
    g.char(con, pc, CON_TIMER as u32)?;
    if lms {
        g.char(con, pc, CON_LMS as u32)?;
    }
    if lms && place {
        let src = g.node(con, pc, lms_place)?;
        pc.set(src, 2, Val::Num(d_lms as u32))?;
        pc.add_item(0, src, None, None)?;
    }
    let c = snippet(&mut g, pc, con, if lms { "frame0_hide" } else { "frame0_hide_timer" })?;
    let a = action(pc, c);
    pc.add_item(0, a, None, None)?;
    let mut first = None;
    for (k, it) in pc.frame_items(0, None)?.into_iter().enumerate() {
        if pc.tag(it)? == "action" {
            first = Some(k);
            break;
        }
    }
    let first = first.ok_or(MenuError::Reference)?;
    let mut inits = vec![snippet(&mut g, pc, con, "timer_events")?];
    if lms {
        inits.push(snippet(&mut g, pc, con, "lms_init")?);
        inits.push(snippet(&mut g, pc, con, "lms_events")?);
    }
    for (k, code) in inits.into_iter().enumerate() {
        let a = action(pc, code);
        pc.add_item(0, a, None, Some(first + k))?;
    }
    let f = labels["playing"];
    let p = find_place(con, f, Some("onslaught_timer"), None)?;
    let item = g.node(con, pc, p)?;
    pc.set(item, 2, Val::Num(d_play as u32))?;
    if place {
        pc.add_item(f, item, None, None)?;
    }
    let vis = if lms { "playing_vis" } else { "playing_vis_timer" };
    for lab in ["playing", "playing_empty", "playing_hardcore"] {
        let c = snippet(&mut g, pc, con, vis)?;
        let a = action(pc, c);
        pc.add_item(labels[lab], a, None, None)?;
    }
    let p = find_place(con, labels["playing_hardcore"], None, Some(95))?;
    let mv = g.node(con, pc, p)?;
    pc.set(mv, 2, Val::Num(d_play as u32))?;
    if place {
        pc.add_item(labels["playing_hardcore"], mv, None, None)?;
    }
    let r = pc.new_words(APT, vec![Val::Num(4), Val::Num(d_play as u32)], "remove");
    pc.add_item(labels["notplaying"], r, None, None)?;
    if lms {
        let r = pc.new_words(APT, vec![Val::Num(4), Val::Num(d_lms as u32)], "remove");
        pc.add_item(labels["notplaying"], r, None, None)?;
    }
    let f = labels["killed"];
    let p = find_place(con, f, Some("onslaught_timer"), None)?;
    let item = g.node(con, pc, p)?;
    pc.set(item, 2, Val::Num(d_kill as u32))?;
    if place {
        pc.add_item(f, item, None, None)?;
    }
    let mut b = Builder::new();
    b.pool(
        pc,
        &[
            "extern",
            "gameModeID",
            "onslaught",
            "onslaught_timer",
            "_visible",
            "box",
        ],
    )?;
    for (op, arg) in [
        ("EA_PushGlobalVar", None),
        ("EA_GetNamedMember", Some("extern")),
        ("EA_GetNamedMember", Some("gameModeID")),
        ("EA_PushConstant", Some("onslaught")),
        ("Equals2", None),
        ("Not", None),
        ("Not", None),
        ("BranchIfTrue", Some("ons")),
        ("EA_PushValueOfVar", Some("onslaught_timer")),
        ("EA_PushConstant", Some("_visible")),
        ("EA_PushFalse", None),
        ("SetMember", None),
        ("BranchAlways", Some("done")),
    ] {
        b.op(pc, op, arg)?;
    }
    b.label("ons");
    for (op, arg) in [
        ("EA_PushValueOfVar", Some("onslaught_timer")),
        ("EA_GetNamedMember", Some("box")),
        ("EA_PushConstant", Some("_visible")),
        ("EA_PushFalse", None),
        ("SetMember", None),
    ] {
        b.op(pc, op, arg)?;
    }
    b.label("done");
    let c = b.code(pc)?;
    let a = action(pc, c);
    pc.add_item(f, a, None, None)?;
    let r = pc.new_words(APT, vec![Val::Num(4), Val::Num(d_kill as u32)], "remove");
    pc.add_item(labels["menu"], r, None, None)?;
    let fit: [((i64, &str), (Option<f64>, Option<f64>, Option<f64>)); 4] = [
        ((182, "field"), (Some(1.0), Some(1.0), None)),
        ((184, "box"), (Some(192.0 / 166.0), None, None)),
        ((184, "best"), (Some(0.65), Some(0.65), Some(176.0 - 198.0 * 0.65))),
        ((184, "label"), (Some(0.65), Some(0.65), None)),
    ];
    for ((owner, name), (sx, sy, x)) in fit {
        let o = pc.char(*g.cmap.get(&owner).ok_or(MenuError::Reference)?)?;
        let it = place_named(pc, o, name)?;
        set_scale(pc, it, sx, sy, x)?;
    }
    let o = pc.char(*g.cmap.get(&CON_TIMER).ok_or(MenuError::Reference)?)?;
    let it = place_named(pc, o, "label")?;
    set_clip_props(pc, it, PC_LABEL, false)?;
    report.push(format!(
        "fitted to the PC font: time field uniform 1.0, box 192 px wide, best / label 0.65; label \"{PC_LABEL}\""
    ));
    for n in pc.code_nodes(true)? {
        let mut code = pc.code(n)?.clone();
        let reads = extern_reads(pc, &code)?;
        if extern_mentions(pc, &code)? > reads.len() {
            return Err(apt_err(
                "the grafted code reads extern in a form the graft does not check; refused",
            ));
        }
        for (s, k, name) in reads.into_iter().rev() {
            if let Some((_, op, what)) = EXTERN_FIX.iter().find(|e| e.0 == name) {
                replace_extern(pc, &mut code, s, k, *op)?;
                report.push(format!("extern.{name} replaced by {what}"));
            } else if !pc_externs.contains(&name) {
                return Err(apt_err(format!(
                    "the grafted code reads extern.{name}, which no code of the PC movie reads"
                )));
            }
        }
        *pc.code_mut(n)? = code;
    }
    if !place {
        report.push("variant: nothing placed (characters, events and visibility code only)".to_owned());
    }
    report.push(format!(
        "characters added: {}",
        g.added
            .iter()
            .map(|(n, k, s)| format!("{n} ({k}, console {s})"))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    report.extend(g.notes.iter().map(|n| format!("needs: {n}")));
    Ok(report)
}
