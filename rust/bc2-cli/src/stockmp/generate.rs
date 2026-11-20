//! `config.generate`'s full layer (round O5): the AI spawners and orders placed on the path data from the level's own
//! objectives and spawns (`config.Snapper` on NavKit's `ordercheck.PathData`: its route nodes, groups and areas, the
//! Rust port of round O4), Conquest and Rush as the reference places them; and `config.check`, the configuration's
//! problems, on the JSON a configuration file holds.
//!
//! Handed to the Python: a capture point, M-COM or spawn point without a position where one is used, a level text that
//! is not UTF-8, a value `round` cannot take exactly, a key that is not finite; in `check`, any value of a shape the
//! reference would raise on or compare by Python's own rules (lists or dicts where a scalar is read, ...).

#![allow(
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::type_complexity,
    clippy::float_cmp,
    reason = "a port of Python: indexes after their lengths are checked, the reference's names and its shapes kept, == as Python compares floats"
)]

use bc2_pyrt::json::Value;
use bc2_stockmp::model::{CapturePoint, Model, Pos};

use super::pyx;
use crate::navkit::ordercheck::PathData;
use crate::pyfmt::float_repr;
use crate::pyproc::{Fail, PyErr, PyResult};

/// `config.SNAP_RADIUS`, `NODE_Y`, `SPAWN_LIFT`.
const SNAP_RADIUS: f64 = 6.0;
const NODE_Y: f64 = 0.95;
const SPAWN_LIFT: f64 = 0.3;
pub const KIT_NAMES: [&str; 4] = ["assault", "demolition", "support", "recon"];

/// A text of the level as Python holds it (UTF-8; anything else is the Python's).
pub fn text(b: &[u8]) -> PyResult<String> {
    String::from_utf8(b.to_vec()).map_err(|_| Fail::Python)
}

/// `config.Snapper`: the path data, its largest route group.
pub struct Snapper {
    pub pd: PathData,
    pub biggest: usize,
}

/// `Snapper.nearest(p)`: (distance, node, its group's size) and the node's index.
pub struct Near {
    pub dist: f64,
    pub node: [f64; 3],
    pub size: usize,
    pub index: usize,
}

impl Snapper {
    /// `Snapper(hpd)` of a file that is there (`ordercheck.PathData`; `max()` of no group raises: the Python's).
    pub fn new(hpd: &str) -> PyResult<Self> {
        let pd = PathData::new(hpd)?;
        if pd.ids.is_empty() {
            return Err(Fail::Python);
        }
        let biggest = pd.biggest();
        Ok(Self { pd, biggest })
    }

    /// `nearest(p)`: NumPy's `hypot` of the nodes' x / z (the C library's), 1e9 where the node's ground is more
    /// than 4 m above or below, the first minimum.
    pub fn nearest(&self, p: [f64; 3]) -> Near {
        let mut best = (0usize, f64::INFINITY);
        for (k, q) in self.pd.pos.iter().enumerate() {
            let d = if (q[1] - NODE_Y - p[1]).abs() <= 4.0 {
                (q[0] - p[0]).hypot(q[2] - p[2])
            } else {
                1e9
            };
            if d.is_nan() {
                best = (k, d);
                break;
            }
            if k == 0 || d < best.1 {
                best = (k, d);
            }
        }
        let k = best.0;
        let g = self.pd.group[&self.pd.ids[k]];
        Near {
            dist: best.1,
            node: self.pd.pos[k],
            size: self.pd.group_size[&g],
            index: k,
        }
    }

    /// The route group of a node index.
    pub fn group_of(&self, k: usize) -> u32 {
        self.pd.group[&self.pd.ids[k]]
    }

    /// `ground(p)`: (point, how) or (None, why).
    fn ground(&self, p: [f64; 3]) -> PyResult<(Option<[f64; 3]>, String)> {
        let n = self.nearest(p);
        #[allow(clippy::cast_precision_loss, reason = "group sizes, far below 2^52")]
        if (n.size as f64) < 0.25 * self.biggest as f64 {
            return Ok((None, format!("nearest route node on a small island ({} nodes)", n.size)));
        }
        if self.pd.on_area(p) && n.dist <= SNAP_RADIUS {
            return Ok((
                Some([p[0], p[1] + SPAWN_LIFT, p[2]]),
                format!("on an area, node {} m", pyx::fixed(n.dist, 1)?),
            ));
        }
        if n.dist <= SNAP_RADIUS {
            return Ok((
                Some([n.node[0], n.node[1] - NODE_Y + SPAWN_LIFT, n.node[2]]),
                format!("moved {} m to a route node", pyx::fixed(n.dist, 1)?),
            ));
        }
        Ok((
            None,
            format!(
                "no route node within {} m ({} m)",
                float_repr(SNAP_RADIUS),
                pyx::fixed(n.dist, 1)?
            ),
        ))
    }
}

fn arr(p: Pos) -> [f64; 3] {
    [p.0, p.1, p.2]
}

/// `_dist(a, b)`: `math.hypot` of the x / z differences.
fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    pyx::hypot(a[0] - b[0], a[2] - b[2])
}

/// `_centroid(pts)`.
fn centroid(pts: &[[f64; 3]]) -> [f64; 3] {
    #[allow(clippy::cast_precision_loss, reason = "a few points")]
    let n = pts.len() as f64;
    let mut out = [0.0; 3];
    for (i, o) in out.iter_mut().enumerate() {
        *o = pyx::sum(pts.iter().map(|p| p[i])) / n;
    }
    out
}

/// `round(v, 3)` of each.
fn round3(p: [f64; 3]) -> PyResult<[f64; 3]> {
    Ok([pyx::round(p[0], 3)?, pyx::round(p[1], 3)?, pyx::round(p[2], 3)?])
}

/// `_yaw_to(a, b)`.
fn yaw_to(a: [f64; 3], b: [f64; 3]) -> PyResult<f64> {
    pyx::round((b[0] - a[0]).atan2(b[2] - a[2]).to_degrees(), 1)
}

/// A key compared as Python's `sort` / `min` / `max` compare floats (a NaN is the Python's).
fn key(x: f64) -> PyResult<f64> {
    if x.is_nan() { Err(Fail::Python) } else { Ok(x) }
}

/// `place(snap, candidates, n, notes, label)`.
fn place(snap: &Snapper, cands: &[[f64; 3]], n: i64, notes: &mut Vec<String>, label: &str) -> PyResult<Vec<[f64; 3]>> {
    let mut good = Vec::new();
    for c in cands {
        let (g, how) = snap.ground(*c)?;
        match g {
            None => notes.push(format!(
                "{label}: candidate {} dropped - {how}",
                pyx::float_tuple(&[pyx::round(c[0], 1)?, pyx::round(c[1], 1)?, pyx::round(c[2], 1)?])
            )),
            Some(g) => good.push(g),
        }
    }
    if good.is_empty() {
        return Ok(Vec::new());
    }
    let mut out: Vec<[f64; 3]> = Vec::new();
    let mut k: i64 = 0;
    let len = i64::try_from(good.len()).map_err(|_| Fail::Python)?;
    while i64::try_from(out.len()).unwrap_or(i64::MAX) < n && k < 50 * n {
        let base = good[usize::try_from(k % len).map_err(|_| Fail::Python)?];
        let r = k / len;
        let p = if r == 0 {
            Some(base)
        } else {
            #[allow(clippy::cast_precision_loss, reason = "Python's int * float: k far below 2^53")]
            let a = (k as f64 * 2.399_963) % (2.0 * std::f64::consts::PI);
            #[allow(clippy::cast_precision_loss, reason = "the ring's index")]
            let rr = 2.5 * r as f64;
            let q = [base[0] + rr * a.cos(), base[1] - SPAWN_LIFT, base[2] + rr * a.sin()];
            snap.ground(q)?.0
        };
        if let Some(p) = p
            && out.iter().all(|o| dist(p, *o) > 1.0)
        {
            out.push(round3(p)?);
        }
        k += 1;
    }
    if i64::try_from(out.len()).unwrap_or(i64::MAX) < n {
        notes.push(format!("{label}: only {} of {n} spawn positions found", out.len()));
    }
    Ok(out)
}

/// `_order_pos(snap, p, notes, label)`.
fn order_pos(snap: &Snapper, p: [f64; 3], notes: &mut Vec<String>, label: &str) -> PyResult<[f64; 3]> {
    let n = snap.nearest(p);
    if n.dist > 15.0 {
        notes.push(format!(
            "{label}: no route node within 15 m of the objective ({} m)",
            pyx::fixed(n.dist, 1)?
        ));
    }
    round3(p)
}

fn floats(p: [f64; 3]) -> Value {
    Value::floats(&p)
}

fn s(x: &str) -> Value {
    Value::Str(x.to_owned())
}

fn pos_of(cp: &CapturePoint) -> PyResult<[f64; 3]> {
    cp.pos.map(arr).ok_or(Fail::Python)
}

/// `Model.ground_points(team=None, flag=guid)`.
fn ground_points(m: &Model, flag: &[u8]) -> Vec<[f64; 3]> {
    let mut out = Vec::new();
    for s in &m.spawners {
        if !flag.is_empty() && s.flag.as_deref() != Some(flag) {
            continue;
        }
        out.extend(s.alternates.iter().map(|(_, p)| arr(*p)));
    }
    out
}

/// A list sorted by a float key as Python's stable `sort` sorts it.
fn sort_by_key<T: Clone>(items: &[T], key_of: impl Fn(&T) -> PyResult<f64>) -> PyResult<Vec<T>> {
    let mut keyed = Vec::with_capacity(items.len());
    for it in items {
        keyed.push((key(key_of(it)?)?, it.clone()));
    }
    keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok(keyed.into_iter().map(|(_, it)| it).collect())
}

/// `min(items, key=...)`: the first smallest.
fn min_by<T>(items: &[T], key_of: impl Fn(&T) -> PyResult<f64>) -> PyResult<Option<&T>> {
    let mut best: Option<(f64, &T)> = None;
    for it in items {
        let k = key(key_of(it)?)?;
        if best.is_none_or(|(b, _)| k < b) {
            best = Some((k, it));
        }
    }
    Ok(best.map(|(_, it)| it))
}

/// `max(items, key=...)`: the first largest.
fn max_by<T>(items: &[T], key_of: impl Fn(&T) -> PyResult<f64>) -> PyResult<Option<&T>> {
    let mut best: Option<(f64, &T)> = None;
    for it in items {
        let k = key(key_of(it)?)?;
        if best.is_none_or(|(b, _)| k > b) {
            best = Some((k, it));
        }
    }
    Ok(best.map(|(_, it)| it))
}

fn value_error(msg: impl Into<String>) -> Fail {
    Fail::Err(PyErr::new("ValueError", msg))
}

/// The settings of a full layer (`config.generate`'s arguments).
pub struct Layer {
    pub bots: i64,
}

/// `config._conquest(c, m, snap, n, half, radius, notes)`: (spawners, orders) added.
#[allow(clippy::too_many_lines, reason = "config._conquest, step for step")]
fn conquest(m: &Model, snap: &Snapper, n: i64, notes: &mut Vec<String>) -> PyResult<(Vec<Value>, Vec<Value>)> {
    let teams: [&[u8]; 2] = [b"Team1", b"Team2"];
    let flags: Vec<&CapturePoint> = m
        .capture_points
        .iter()
        .filter(|cp| !cp.owner.as_deref().is_some_and(|o| teams.contains(&o)))
        .collect();
    if flags.is_empty() {
        return Err(value_error("no neutral capture points (Conquest flags) found"));
    }
    let (mut spawners, mut orders) = (Vec::new(), Vec::new());
    for t in ["Team1", "Team2"] {
        let bases: Vec<&CapturePoint> = m
            .capture_points
            .iter()
            .filter(|cp| cp.owner.as_deref() == Some(t.as_bytes()))
            .collect();
        if bases.is_empty() {
            return Err(value_error(format!(
                "{t}: no capture point owned at the start (its base)"
            )));
        }
        let mut cands = Vec::new();
        for b in &bases {
            cands.extend(ground_points(m, &b.guid));
        }
        let base_pos: Vec<[f64; 3]> = bases.iter().map(|b| pos_of(b)).collect::<PyResult<_>>()?;
        if cands.is_empty() {
            cands.clone_from(&base_pos);
            notes.push(format!(
                "{t}: the base has no ground spawn points; using the base flag position"
            ));
        }
        let home = centroid(&base_pos);
        let cands = sort_by_key(&cands, |p| Ok(dist(*p, home)))?;
        let pts = place(snap, &cands, n, notes, &format!("{t} base"))?;
        let tag = if t == "Team1" { "t1" } else { "t2" };
        let order_flags = sort_by_key(&flags, |f| Ok(dist(pos_of(f)?, home)))?;
        let mut assign: Vec<Vec<String>> = vec![Vec::new(); order_flags.len()];
        for (i, p) in pts.iter().enumerate() {
            let sid = format!("{tag}_{i:02}");
            let fi = i % order_flags.len();
            let f = order_flags[fi];
            spawners.push(Value::dict([
                ("id", s(&sid)),
                ("team", s(t)),
                ("pos", floats(*p)),
                ("yaw", Value::Float(yaw_to(*p, pos_of(f)?)?)),
                ("kit", s(KIT_NAMES[i % KIT_NAMES.len()])),
                ("stage", Value::Null),
            ]));
            // assign[f.guid]: flags of one guid share their list
            let first = order_flags.iter().position(|g| g.guid == f.guid).unwrap_or(fi);
            assign[first].push(sid);
        }
        for (fi, f) in order_flags.iter().enumerate() {
            let first = order_flags.iter().position(|g| g.guid == f.guid).unwrap_or(fi);
            if assign[first].is_empty() {
                continue;
            }
            let label = if f.label.is_empty() { &f.name } else { &f.label };
            let label = text(label)?;
            let name = text(&f.name)?;
            orders.push(Value::dict([
                ("id", s(&format!("{tag}_flag_{label}"))),
                ("team", s(t)),
                ("type", s("AreaMoveTo")),
                ("objective", s(&label)),
                ("pos", floats(order_pos(snap, pos_of(f)?, notes, &name)?)),
                ("half_extents", Value::floats(&[8.0, 3.0, 8.0])),
                ("soldiers", Value::List(assign[first].iter().map(|x| s(x)).collect())),
                ("stage", Value::Null),
                ("trigger_radius", Value::Float(100.0)),
            ]));
        }
    }
    Ok((spawners, orders))
}

/// `config._rush(c, m, snap, bots, half, radius, notes)`: (spawners, orders, stage switches).
#[allow(clippy::too_many_lines, reason = "config._rush, step for step")]
fn rush(
    m: &Model,
    dbx: &bc2_formats::dbx::edit::EditDbx,
    snap: &Snapper,
    notes: &mut Vec<String>,
) -> PyResult<(Vec<Value>, Vec<Value>, Vec<Value>)> {
    let stages = m.rush_stages(dbx).map_err(|_| Fail::Python)?;
    if stages.is_empty() {
        return Err(value_error("no Rush stages found (M-COM triggers)"));
    }
    let att: &str = match m.attacker.as_deref() {
        Some(b"Team2") => "Team2",
        _ => "Team1",
    };
    let dfn = if att == "Team1" { "Team2" } else { "Team1" };
    let mut centroids = Vec::new();
    for st in &stages {
        let pts: Vec<[f64; 3]> = st
            .mcoms
            .iter()
            .map(|(_, p)| p.map(arr).ok_or(Fail::Python))
            .collect::<PyResult<_>>()?;
        if pts.is_empty() {
            return Err(Fail::Python); // ZeroDivisionError
        }
        centroids.push(centroid(&pts));
    }
    let att_cps: Vec<&CapturePoint> = m
        .capture_points
        .iter()
        .filter(|cp| cp.owner.as_deref() == Some(att.as_bytes()))
        .collect();
    let def_cps: Vec<&CapturePoint> = m
        .capture_points
        .iter()
        .filter(|cp| cp.owner.as_deref() == Some(dfn.as_bytes()))
        .collect();
    let att_base = max_by(&att_cps, |cp| {
        let p = pos_of(cp)?;
        let mut best: Option<f64> = None;
        for x in &centroids {
            let d = key(dist(p, *x))?;
            if best.is_none_or(|b| d < b) {
                best = Some(d);
            }
        }
        best.ok_or(Fail::Python)
    })?
    .copied();
    let (mut spawners, mut orders, mut switches) = (Vec::new(), Vec::new(), Vec::new());
    for (st, cen) in stages.iter().zip(&centroids) {
        let k = st.index;
        let dcp = min_by(&def_cps, |cp| Ok(dist(pos_of(cp)?, *cen)))?.copied();
        let acp = if k == 0 || att_cps.is_empty() {
            att_base
        } else {
            min_by(&att_cps, |cp| Ok(dist(pos_of(cp)?, centroids[k - 1])))?.copied()
        };
        for (team, cp, n, kind, tag) in [
            (dfn, dcp, 4i64, "AreaHoldGround", "d"),
            (att, acp, 4i64, "AreaMoveTo", "a"),
        ] {
            let Some(cp) = cp else {
                notes.push(format!("stage {k}: no {team} capture point to spawn at"));
                continue;
            };
            let mut cands = sort_by_key(&ground_points(m, &cp.guid), |p| Ok(dist(*p, *cen)))?;
            if cands.is_empty() {
                cands = vec![pos_of(cp)?];
            }
            let pts = place(
                snap,
                &cands,
                n,
                notes,
                &format!("stage {k} {team} ({})", text(&cp.name)?),
            )?;
            let mut ids = Vec::new();
            for (i, p) in pts.iter().enumerate() {
                let sid = format!("s{k}{tag}_{i:02}");
                ids.push(sid.clone());
                spawners.push(Value::dict([
                    ("id", s(&sid)),
                    ("team", s(team)),
                    ("pos", floats(*p)),
                    ("yaw", Value::Float(yaw_to(*p, *cen)?)),
                    ("kit", s(KIT_NAMES[i % KIT_NAMES.len()])),
                    ("stage", Value::Int(i128::try_from(k).map_err(|_| Fail::Python)?)),
                ]));
            }
            let nm = st.mcoms.len();
            for (j, (g, mp)) in st.mcoms.iter().enumerate() {
                let sold: Vec<Value> = ids.iter().skip(j).step_by(nm).map(|x| s(x)).collect();
                if sold.is_empty() {
                    continue;
                }
                let mp = mp.map(arr).ok_or(Fail::Python)?;
                orders.push(Value::dict([
                    ("id", s(&format!("s{k}{tag}_mcom{j}"))),
                    ("team", s(team)),
                    ("type", s(kind)),
                    ("objective", s(&text(g)?)),
                    (
                        "pos",
                        floats(order_pos(snap, mp, notes, &format!("stage {k} M-COM {j}"))?),
                    ),
                    ("half_extents", Value::floats(&[8.0, 3.0, 8.0])),
                    ("soldiers", Value::List(sold)),
                    ("stage", Value::Int(i128::try_from(k).map_err(|_| Fail::Python)?)),
                    ("trigger_radius", Value::Float(100.0)),
                ]));
            }
        }
        if k + 1 < stages.len() {
            switches.push(Value::dict([
                ("index", Value::Int(i128::try_from(k).map_err(|_| Fail::Python)?)),
                ("name", s(&text(&st.trigger_name)?)),
                (
                    "advance",
                    Value::dict([("source", s(&text(&st.trigger)?)), ("event", s("OnTrigger"))]),
                ),
            ]));
        }
    }
    Ok((spawners, orders, switches))
}

/// `config.generate`'s full layer: (spawners, orders, stages, notes).
pub fn layer(
    m: &Model,
    dbx: &bc2_formats::dbx::edit::EditDbx,
    snap: &Snapper,
    l: &Layer,
) -> PyResult<(Vec<Value>, Vec<Value>, Vec<Value>, Vec<String>)> {
    let mut notes = Vec::new();
    match m.mode.as_slice() {
        b"conquest" => {
            let (sp, or) = conquest(m, snap, l.bots, &mut notes)?;
            Ok((sp, or, Vec::new(), notes))
        }
        b"rush" => {
            let (sp, or, st) = rush(m, dbx, snap, &mut notes)?;
            Ok((sp, or, st, notes))
        }
        other => {
            let name = match &m.name {
                Some(n) => text(n)?,
                None => "None".to_owned(),
            };
            Err(value_error(format!(
                "{name}: mode {} - only conquest and rush are generated so far",
                text(other)?
            )))
        }
    }
}

// ------------------------------------------------------------------------------------------------ config.check

/// A hashable JSON scalar as a Python set or dict key (`1 == 1.0 == True`).
#[derive(Clone, PartialEq)]
enum Key {
    None,
    Num(f64),
    Str(String),
}

fn key_of(v: Option<&Value>) -> PyResult<Key> {
    Ok(match v {
        None | Some(Value::Null) => Key::None,
        Some(Value::Bool(b)) => Key::Num(f64::from(u8::from(*b))),
        #[allow(clippy::cast_precision_loss, reason = "ints of a configuration: small")]
        Some(Value::Int(i)) if i.unsigned_abs() < (1u128 << 53) => Key::Num(*i as f64),
        Some(Value::Float(f)) if !f.is_nan() => Key::Num(*f),
        Some(Value::Str(s)) => Key::Str(s.clone()),
        _ => return Err(Fail::Python),
    })
}

/// `str(v)` of a scalar (a list or dict: the Python's).
pub fn py_str(v: Option<&Value>) -> PyResult<String> {
    Ok(match v {
        None | Some(Value::Null) => "None".to_owned(),
        Some(Value::Bool(true)) => "True".to_owned(),
        Some(Value::Bool(false)) => "False".to_owned(),
        Some(Value::Int(i)) => i.to_string(),
        Some(Value::Float(f)) => float_repr(*f),
        Some(Value::Str(s)) => s.clone(),
        _ => return Err(Fail::Python),
    })
}

fn dict_items(v: &Value) -> PyResult<&[(String, Value)]> {
    match v {
        Value::Dict(items) => Ok(items),
        _ => Err(Fail::Python),
    }
}

fn list_items(v: &Value) -> PyResult<&[Value]> {
    match v {
        Value::List(items) => Ok(items),
        _ => Err(Fail::Python),
    }
}

/// `x in some_dict` of a JSON dict (string keys).
fn dict_has(d: &[(String, Value)], k: &Key) -> bool {
    matches!(k, Key::Str(s) if d.iter().any(|(x, _)| x == s))
}

/// `config.check(c)`.
#[allow(clippy::too_many_lines, reason = "config.check, step for step")]
pub fn check(c: &Value) -> PyResult<Vec<String>> {
    let top = dict_items(c)?;
    let mut p = Vec::new();
    let version_ok = matches!(c.get("version").and_then(Value::as_f64), Some(v) if v == 1.0)
        && !matches!(c.get("version"), Some(Value::Str(_)));
    if c.get("format").and_then(Value::as_str) != Some("bfbc2-stockmp-ai") || !version_ok {
        p.push("not a bfbc2-stockmp-ai v1 configuration".to_owned());
        return Ok(p);
    }
    for k in [
        "lv", "level", "mode", "teams", "defaults", "spawners", "orders", "template",
    ] {
        if !top.iter().any(|(x, _)| x == k) {
            p.push(format!("missing \"{k}\""));
        }
    }
    if !p.is_empty() {
        return Ok(p);
    }
    let teams = c.get("teams").map_or(Ok(&[][..]), dict_items)?;
    let spawners = list_items(c.get("spawners").ok_or(Fail::Python)?)?;
    let orders = list_items(c.get("orders").ok_or(Fail::Python)?)?;
    let mut ids: Vec<Key> = Vec::new();
    for sp in spawners {
        dict_items(sp)?;
        let id = key_of(sp.get("id"))?;
        let shown = py_str(sp.get("id"))?;
        if ids.contains(&id) {
            p.push(format!("spawner id {shown} twice"));
        }
        if !ids.contains(&id) {
            ids.push(id);
        }
        let team = key_of(sp.get("team"))?;
        if dict_has(teams, &team) {
            let Key::Str(t) = &team else { return Err(Fail::Python) };
            let tv = c.get("teams").and_then(|x| x.get(t)).ok_or(Fail::Python)?;
            dict_items(tv)?;
            let kits = dict_items(tv.get("kits").ok_or(Fail::Python)?)?;
            if !dict_has(kits, &key_of(sp.get("kit"))?) {
                p.push(format!(
                    "spawner {shown}: kit {} not in the team's kits",
                    py_str(sp.get("kit"))?
                ));
            }
        } else {
            p.push(format!(
                "spawner {shown}: team {} not in teams",
                py_str(sp.get("team"))?
            ));
        }
        if !matches!(sp.get("pos"), Some(Value::List(v)) if v.len() == 3) {
            p.push(format!("spawner {shown}: pos must be [x, y, z]"));
        }
    }
    // teams_of / stage_of: {s['id']: ...} (a spawner without an id: KeyError)
    let mut teams_of: Vec<(Key, Option<&Value>, Option<&Value>)> = Vec::new();
    for sp in spawners {
        let id = key_of(Some(sp.get("id").ok_or(Fail::Python)?))?;
        let (t, st) = (sp.get("team"), sp.get("stage"));
        match teams_of.iter_mut().find(|(k, _, _)| *k == id) {
            Some(slot) => {
                slot.1 = t;
                slot.2 = st;
            }
            None => teams_of.push((id, t, st)),
        }
    }
    for o in orders {
        dict_items(o)?;
        let id = key_of(o.get("id"))?;
        let shown = py_str(o.get("id"))?;
        if ids.contains(&id) {
            p.push(format!("order id {shown} used twice"));
        }
        if !ids.contains(&id) {
            ids.push(id);
        }
        if !o.get("soldiers").is_some_and(Value::truthy) {
            p.push(format!("order {shown}: no soldiers"));
        }
        let soldiers: &[Value] = match o.get("soldiers") {
            None => &[],
            Some(v) => list_items(v)?,
        };
        for x in soldiers {
            let xk = key_of(Some(x))?;
            match teams_of.iter().find(|(k, _, _)| *k == xk) {
                None => p.push(format!("order {shown}: unknown soldier {}", py_str(Some(x))?)),
                Some((_, t, st)) => {
                    if key_of(*t)? != key_of(o.get("team"))? {
                        p.push(format!(
                            "order {shown}: soldier {} is on another team",
                            py_str(Some(x))?
                        ));
                    } else if key_of(*st)? != key_of(o.get("stage"))? {
                        p.push(format!(
                            "order {shown}: soldier {} belongs to another stage",
                            py_str(Some(x))?
                        ));
                    }
                }
            }
        }
        let typ = o.get("type");
        let typ_ok = match typ {
            None => true,
            Some(Value::Str(t)) => t == "AreaMoveTo" || t == "AreaHoldGround",
            Some(v) => {
                key_of(Some(v))?;
                false
            }
        };
        if !typ_ok {
            p.push(format!("order {shown}: type {}", py_str(typ)?));
        }
    }
    // stages = {s.get('stage') for s in spawners} - {None}: ints only here
    let mut stages: Vec<i128> = Vec::new();
    for sp in spawners {
        match sp.get("stage") {
            None | Some(Value::Null) => {}
            Some(Value::Int(i)) => {
                if !stages.contains(i) {
                    stages.push(*i);
                }
            }
            _ => return Err(Fail::Python),
        }
    }
    stages.sort_unstable();
    if !stages.is_empty()
        && stages
            .iter()
            .enumerate()
            .any(|(k, v)| i128::try_from(k).ok() != Some(*v))
    {
        let shown: Vec<String> = stages.iter().map(ToString::to_string).collect();
        p.push(format!("stages must be 0..n-1, have [{}]", shown.join(", ")));
    }
    let st_list: &[Value] = match c.get("stages") {
        Some(v) if v.truthy() => list_items(v)?,
        _ => &[],
    };
    for st in st_list {
        dict_items(st)?;
        let ok = match st.get("advance") {
            None => false,
            Some(a) => dict_items(a)?.iter().any(|(k, _)| k == "source"),
        };
        if !ok {
            p.push(format!("stage {}: no advance source", py_str(st.get("index"))?));
        }
    }
    Ok(p)
}
