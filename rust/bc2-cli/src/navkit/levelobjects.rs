//! `bfbc2-auto-navkit\tools\levelobjects.py`: a prepared level's placed objects, group instances, spawn and order
//! points (JSON) and the collision triangles NavKit's build reads (`.npy`), from its unpacked and converted bundles.
//!
//! The arithmetic is NumPy's: each triangle corner in world space is `t @ m[:3, :3] + m[3, :3]` in float64 (the
//! float32 corners widened), then rounded to float32; the barrier-arm and footprint tests compare in float32 (NumPy
//! 2's weak Python scalars: `0.7`, `base + 0.5` become float32 first).

use std::collections::{BTreeSet, HashMap};

use bc2_navbuild::levelscan;
use bc2_scene::collision;
use regex::Regex;

use super::{floats_of, read_text_replace, read_text_utf8, str_list_repr};
use crate::pyjson::{self, Value};
use crate::pyos::{self, join, join2};
use crate::pyproc::{Fail, Py, PyErr, PyResult};

/// Blueprint flags: (`ForbidPathfinding`, `ExcludeFromPathfinding`, `AllowPathdataSeeds`, dynamic, `DisablePathfinding`).
pub type Flags = (bool, bool, bool, bool, String);

/// A placed object or group instance: blueprint, flags, its 16 matrix values (as many as the file has).
pub struct Obj {
    pub group: Option<String>,
    pub bp: String,
    pub flags: Option<Flags>,
    pub m: Vec<f64>,
}

impl Obj {
    fn json(&self) -> Value {
        let mut items = Vec::new();
        if let Some(g) = &self.group {
            items.push(("group".to_owned(), Value::Str(g.clone())));
        }
        items.push(("bp".to_owned(), Value::Str(self.bp.clone())));
        items.push((
            "flags".to_owned(),
            match &self.flags {
                None => Value::Null,
                Some((a, b, c, d, e)) => Value::List(vec![
                    Value::Bool(*a),
                    Value::Bool(*b),
                    Value::Bool(*c),
                    Value::Bool(*d),
                    Value::Str(e.clone()),
                ]),
            },
        ));
        items.push(("m".to_owned(), Value::floats(&self.m)));
        Value::Dict(items)
    }
}

fn re(p: &str) -> PyResult<Regex> {
    Regex::new(p).map_err(|_| Fail::Python)
}

/// `xmlinst.instances(text, suffix)` (the core's, else the reference's patterns).
pub fn instances<'t>(text: &'t str, suffix: Option<&str>) -> PyResult<Vec<(String, String, &'t str)>> {
    if let Some(v) = bc2_formats::xmlinst::instances(text, suffix) {
        return Ok(v.into_iter().map(|(t, g, b)| (t.to_owned(), g.to_owned(), b)).collect());
    }
    let inst = re(r"(?s)<instance\b([^>]*)>(.*?)</instance>")?;
    let mut out = Vec::new();
    for c in inst.captures_iter(text) {
        let a = attrs_ref(c.get(1).map_or("", |m| m.as_str()))?;
        let get = |k: &str| {
            a.iter()
                .rev()
                .find(|(n, _)| n == k)
                .map_or(String::new(), |x| x.1.clone())
        };
        let typ = get("type");
        if suffix.is_none_or(|s| typ.ends_with(s)) {
            out.push((typ, get("guid"), c.get(2).map_or("", |m| m.as_str())));
        }
    }
    Ok(out)
}

fn attrs_ref(t: &str) -> PyResult<Vec<(String, String)>> {
    let a = re(r#"(\w+)="([^"]*)""#)?;
    Ok(a.captures_iter(t)
        .map(|c| {
            (
                c.get(1).map_or("", |m| m.as_str()).to_owned(),
                c.get(2).map_or("", |m| m.as_str()).to_owned(),
            )
        })
        .collect())
}

/// `xmlinst.field_ref(body, name)`.
pub fn field_ref(body: &str, name: &str) -> PyResult<Option<String>> {
    if let Some(r) = bc2_formats::xmlinst::field_ref(body, name) {
        return Ok(r.map(str::to_owned));
    }
    let f = re(r"<field\b([^>]*?)/?>")?;
    for c in f.captures_iter(body) {
        let a = attrs_ref(c.get(1).map_or("", |m| m.as_str()))?;
        let get = |k: &str| a.iter().rev().find(|(n, _)| n == k).map(|x| x.1.clone());
        if get("name").as_deref() == Some(name)
            && let Some(r) = get("ref")
        {
            return Ok(Some(r));
        }
    }
    Ok(None)
}

/// `re.search(r'<complex name="Transform">([^<]*)</complex>', body)`'s values.
pub fn transform(body: &str) -> PyResult<Option<Vec<f64>>> {
    const OPEN: &str = "<complex name=\"Transform\">";
    let mut from = 0;
    while let Some(p) = body.get(from..).and_then(|r| r.find(OPEN)) {
        let s = from + p + OPEN.len();
        let rest = body.get(s..).unwrap_or("");
        let end = rest.find('<').unwrap_or(rest.len());
        if rest.get(end..).is_some_and(|r| r.starts_with("</complex>")) {
            return floats_of(&rest[..end]).map(Some);
        }
        from = from + p + 1;
    }
    Ok(None)
}

fn far(v: &[f64]) -> bool {
    v.get(12..15.min(v.len()))
        .unwrap_or_default()
        .iter()
        .any(|c| c.abs() > 1e-3)
}

/// `order_points(level_xml)` / `area_orders(level_xml)`: the Transform translations of the instances of a type.
pub fn points_of(x: &str, suffix: &str) -> PyResult<Vec<Vec<f64>>> {
    let mut pts = Vec::new();
    for (_t, _g, body) in instances(x, Some(suffix))? {
        if let Some(v) = transform(body)?
            && v.len() >= 15
            && far(&v)
        {
            pts.push(v[12..15].to_vec());
        }
    }
    Ok(pts)
}

fn points_json(p: &[Vec<f64>]) -> Value {
    Value::List(p.iter().map(|v| Value::floats(v)).collect())
}

/// `collision.index(bases)` with the cache `collision._default_cache()` names.
fn collision_cache() -> Option<String> {
    match std::env::var("BFBC2_COLLISION_CACHE").ok().filter(|v| !v.is_empty()) {
        Some(v) if v == "0" => None,
        Some(v) => Some(v),
        None => {
            let base = std::env::var("LOCALAPPDATA")
                .ok()
                .filter(|v| !v.is_empty())
                .or_else(|| std::env::var("USERPROFILE").ok())?;
            join(&base, &["bfbc2-tool-suite", "cache", "collision"]).ok()
        }
    }
}

/// An `io::Error` from the core as the binding raises it (`OSError` and its subclasses, Rust's message).
pub fn io_err(e: &std::io::Error) -> Fail {
    let kind = match e.kind() {
        std::io::ErrorKind::NotFound => "FileNotFoundError",
        std::io::ErrorKind::PermissionDenied => "PermissionError",
        _ => "OSError",
    };
    Fail::Err(PyErr::new(kind, e.to_string()))
}

/// `collision.triangles(path)`: (n, 3, 3) float32 local triangles, or None.
pub fn triangles(path: &str) -> PyResult<Option<Vec<[[f32; 3]; 3]>>> {
    let data = std::fs::read(path).map_err(|e| io_err(&e))?;
    match bc2_formats::havok::triangles(&data) {
        Ok(t) => Ok(t.map(|v| v.into_iter().collect())),
        Err(e) => Err(Fail::Err(PyErr::new("ValueError", e.to_string()))),
    }
}

/// `np.save(path, a)` of an (n, 3, 3) array: float32 values, or `np.zeros((0, 3, 3))` (float64) for None.
pub fn save_npy(path: &str, tris: Option<&[[[f32; 3]; 3]]>) -> PyResult<()> {
    let (descr, n) = match tris {
        Some(t) => ("<f4", t.len()),
        None => ("<f8", 0),
    };
    let mut h = format!("{{'descr': '{descr}', 'fortran_order': False, 'shape': ({n}, 3, 3), }}");
    h.push_str(&" ".repeat(21usize.saturating_sub(n.to_string().len())));
    let hlen = h.len() + 1;
    let pad = 64 - ((10 + hlen) % 64);
    h.push_str(&" ".repeat(pad));
    h.push('\n');
    let mut out = Vec::with_capacity(10 + h.len() + n * 36);
    out.extend_from_slice(b"\x93NUMPY\x01\x00");
    out.extend_from_slice(&u16::try_from(h.len()).map_err(|_| Fail::Python)?.to_le_bytes());
    out.extend_from_slice(h.as_bytes());
    for tri in tris.unwrap_or_default() {
        for p in tri {
            for v in p {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
    }
    std::fs::write(path, out).map_err(|e| pyproc_os(path, &e))
}

fn pyproc_os(path: &str, e: &std::io::Error) -> Fail {
    crate::pyproc::os_error(path, e)
}

/// `json.dump(v, open(path, 'w'))` (no newlines: what is written is the text).
pub fn dump_json(path: &str, v: &Value) -> PyResult<()> {
    std::fs::write(path, pyjson::dumps(v, None)).map_err(|e| pyproc_os(path, &e))
}

/// One object in world space: `(t @ m[:3, :3] + m[3, :3]).astype(np.float32)`.
fn world(t: &[[[f32; 3]; 3]], m: &[f64]) -> PyResult<Vec<[[f32; 3]; 3]>> {
    if m.len() != 16 {
        return Err(Fail::Err(PyErr::new(
            "ValueError",
            format!("cannot reshape array of size {} into shape (4,4)", m.len()),
        )));
    }
    Ok(t.iter()
        .map(|tri| {
            tri.map(|p| {
                let (a, b, c) = (f64::from(p[0]), f64::from(p[1]), f64::from(p[2]));
                [0usize, 1, 2].map(|j| (a * m[j] + b * m[4 + j] + c * m[8 + j] + m[12 + j]) as f32)
            })
        })
        .collect())
}

/// The run's state: each blueprint's triangles read once (`cache`), those with none (`missing`).
struct Tris {
    havok: collision::FirstWins<std::path::PathBuf>,
    cache: HashMap<String, Option<Vec<[[f32; 3]; 3]>>>,
    missing: BTreeSet<String>,
}

impl Tris {
    fn local(&mut self, bp: &str) -> PyResult<Option<&Vec<[[f32; 3]; 3]>>> {
        if !self.cache.contains_key(bp) {
            let t = match self.havok.get(bp) {
                Some(p) => triangles(&p.to_string_lossy())?,
                None => None,
            };
            self.cache.insert(bp.to_owned(), t);
        }
        Ok(self.cache.get(bp).and_then(Option::as_ref))
    }

    /// `tris_of(o)`.
    fn of(&mut self, o: &Obj) -> PyResult<Option<Vec<[[f32; 3]; 3]>>> {
        if let Some(t) = self.local(&o.bp)? {
            let t = t.clone();
            return world(&t, &o.m).map(Some);
        }
        self.missing.insert(o.bp.clone());
        Ok(None)
    }

    /// `barrier_arm(o)`.
    fn barrier_arm(&mut self, o: &Obj) -> PyResult<bool> {
        if !self.havok.contains(&o.bp) {
            return Ok(false);
        }
        let Some(t) = self.local(&o.bp)? else {
            return Ok(false);
        };
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        let mut nan = false;
        for p in t.iter().flatten() {
            for k in 0..3 {
                nan |= p[k].is_nan();
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        if t.is_empty() {
            // v.min(0) of an empty array raises
            return Err(Fail::Err(PyErr::new(
                "ValueError",
                "zero-size array to reduction operation minimum which has no identity",
            )));
        }
        if nan {
            return Err(Fail::Python);
        }
        Ok(lo[1] >= 0.7f32 && hi[1] - lo[1] <= 0.5f32 && (hi[0] - lo[0]).max(hi[2] - lo[2]) >= 2.0f32)
    }
}

fn count(v: &[Vec<[[f32; 3]; 3]>]) -> usize {
    v.iter().map(Vec::len).sum()
}

fn concat(v: &[Vec<[[f32; 3]; 3]>]) -> Option<Vec<[[f32; 3]; 3]>> {
    (!v.is_empty()).then(|| v.concat())
}

/// `levelobjects.main(['levelobjects.py', level_dir, level, *extra])`.
pub fn main(py: &mut Py, level_dir: &str, level: &str, extra: &[String]) -> PyResult<i32> {
    let mut bases: Vec<String> = vec![join2(level_dir, "pkg")?, join2(level_dir, "dist")?];
    bases.extend(extra.iter().cloned());
    let paths: Vec<std::path::PathBuf> = bases.iter().map(std::path::PathBuf::from).collect();
    // blueprint_flags(bases, scan(bases))
    let scanned = levelscan::scan(&paths).ok_or(Fail::Python)?;
    let mut flags: Vec<(String, Flags)> = Vec::new();
    let mut flag_at: HashMap<String, usize> = HashMap::new();
    for (records, _havok) in scanned {
        for r in records {
            if let Some(fl) = r.flags {
                let k: String = r.rel.replace('\\', "/").to_lowercase();
                let n = k.chars().count();
                let k: String = k.chars().take(n.saturating_sub(4)).collect();
                if !flag_at.contains_key(&k) {
                    flag_at.insert(k.clone(), flags.len());
                    flags.push((k, fl));
                }
            }
        }
    }
    let flag = |bp: &str| flag_at.get(bp).and_then(|&i| flags.get(i)).map(|x| x.1.clone());
    let x = read_text_replace(&join(level_dir, &["dist", "levels", &format!("{level}.xml")])?)?;
    let mut objs = Vec::new();
    for (_t, _g, body) in instances(&x, Some("Entity.ReferenceObjectData"))? {
        let t = transform(body)?;
        let r = field_ref(body, "ReferencedObject")?;
        if let (Some(m), Some(r)) = (t, r) {
            let bp = r.rsplit_once('/').map_or(r.as_str(), |x| x.0).to_lowercase();
            objs.push(Obj {
                group: None,
                flags: flag(&bp),
                bp,
                m,
            });
        }
    }
    let mut groups = Vec::new();
    let trans = re(r#"<array name="Transforms">([^<]*)<"#)?;
    let item = re(r#"<item ref="([^"]*)" />"#)?;
    for f in pyos::glob(&join(level_dir, &["dist", "levels", level, "*_HavokAsset.xml"])?)? {
        let text = read_text_utf8(&f)?;
        let Some(trm) = trans.captures(&text) else {
            continue;
        };
        let tr: Vec<&str> = trm.get(1).map_or("", |m| m.as_str()).split('/').collect();
        let Some((_, after)) = text.split_once("<array name=\"Assets\">") else {
            return Err(Fail::Err(PyErr::new("IndexError", "list index out of range")));
        };
        let assets_text = after.split("</array>").next().unwrap_or("");
        for (i, c) in item.captures_iter(assets_text).enumerate() {
            let a = c.get(1).map_or("", |m| m.as_str());
            let low = a.rsplit_once('/').map_or(a, |x| x.0).to_lowercase();
            let bp = low.strip_suffix("_havok").unwrap_or(&low).to_owned();
            let m = tr
                .get((i * 16).min(tr.len())..((i + 1) * 16).min(tr.len()))
                .unwrap_or_default()
                .iter()
                .map(|v| super::py_float(v))
                .collect::<PyResult<Vec<f64>>>()?;
            groups.push(Obj {
                group: Some(pyos::basename(&f)?),
                flags: flag(&bp),
                bp,
                m,
            });
        }
    }
    let mut spawns: Vec<Vec<f64>> = Vec::new();
    for (_t, _g, body) in instances(&x, Some(".SoldierSpawnEntityData"))? {
        if let Some(v) = transform(body)?
            && far(&v)
        {
            spawns.push(v.get(12..15.min(v.len())).unwrap_or_default().to_vec());
        }
    }
    dump_json(&join2(level_dir, "soldier_spawns.json")?, &points_json(&spawns))?;
    dump_json(
        &join2(level_dir, "order_points.json")?,
        &points_json(&points_of(&x, "Entity.AreaGeometryEntityData")?),
    )?;
    dump_json(
        &join2(level_dir, "area_orders.json")?,
        &points_json(&points_of(&x, ".AreaOrderEntityData")?),
    )?;
    dump_json(
        &join2(level_dir, "objects.json")?,
        &Value::List(objs.iter().map(Obj::json).collect()),
    )?;
    dump_json(
        &join2(level_dir, "group_instances.json")?,
        &Value::List(groups.iter().map(Obj::json).collect()),
    )?;

    let index = collision::index_cached(&paths, collision_cache().as_deref().map(std::path::Path::new));
    let havok = index.resolved().map_err(|e| io_err(&e))?;
    let mut tris = Tris {
        havok,
        cache: HashMap::new(),
        missing: BTreeSet::new(),
    };
    let all: Vec<&Obj> = objs.iter().chain(groups.iter()).collect();
    let mut dynamic = Vec::new();
    for o in &all {
        let Some(fl) = &o.flags else { continue };
        if (fl.3 || fl.4 == "PdtWhenAlive" || (fl.0 && tris.barrier_arm(o)?))
            && let Some(w) = tris.of(o)?
        {
            dynamic.push(w);
        }
    }
    let stair_like = re(r"(?i)stair|ramp|steps")?;
    let mut stairs = Vec::new();
    for o in &all {
        if stair_like.is_match(o.bp.rsplit('/').next().unwrap_or(""))
            && let Some(w) = tris.of(o)?
        {
            stairs.push(w);
        }
    }
    save_npy(&join2(level_dir, "stair_tris.npy")?, concat(&stairs).as_deref())?;
    py.print(&format!(
        "{} stair triangles from {} staircases / ramps",
        count(&stairs),
        stairs.len()
    ))?;
    let mut arms = BTreeSet::new();
    for o in &all {
        if o.flags.as_ref().is_some_and(|f| f.0) && tris.barrier_arm(o)? {
            arms.insert(o.bp.rsplit('/').next().unwrap_or("").to_owned());
        }
    }
    py.print(&format!(
        "barrier arms: {}",
        str_list_repr(&arms.into_iter().collect::<Vec<_>>())
    ))?;
    save_npy(&join2(level_dir, "dynamic_tris.npy")?, concat(&dynamic).as_deref())?;
    py.print(&format!(
        "{} dynamic-object triangles from {} instances",
        count(&dynamic),
        dynamic.len()
    ))?;
    let (mut ex_all, mut ex_foot) = (Vec::new(), Vec::new());
    for o in &all {
        if !o.flags.as_ref().is_some_and(|f| f.1) {
            continue;
        }
        let Some(w) = tris.of(o)? else { continue };
        let base =
            *o.m.get(13)
                .ok_or_else(|| Fail::Err(PyErr::new("IndexError", "list index out of range")))?;
        let limit = (base + 0.5) as f32;
        let foot: Vec<[[f32; 3]; 3]> = w
            .iter()
            .filter(|t| t[0][1].min(t[1][1]).min(t[2][1]) <= limit)
            .copied()
            .collect();
        if w.iter().any(|t| t.iter().any(|p| p[1].is_nan())) {
            return Err(Fail::Python);
        }
        ex_all.push(w);
        ex_foot.push(foot);
    }
    save_npy(&join2(level_dir, "exclude_tris.npy")?, concat(&ex_all).as_deref())?;
    save_npy(
        &join2(level_dir, "exclude_footprint_tris.npy")?,
        concat(&ex_foot).as_deref(),
    )?;
    py.print(&format!(
        "{} excluded objects: {} triangles, footprint {}",
        ex_all.len(),
        count(&ex_all),
        count(&ex_foot)
    ))?;
    let mut forbid = Vec::new();
    for o in &all {
        let Some(fl) = &o.flags else { continue };
        if !fl.0 || fl.1 {
            continue;
        }
        if let Some(w) = tris.of(o)? {
            forbid.push(w);
        }
    }
    save_npy(&join2(level_dir, "forbid_tris.npy")?, concat(&forbid).as_deref())?;
    let missing: Vec<String> = tris.missing.iter().cloned().collect();
    py.print(&format!(
        "{} placed objects, {} group instances, {} blueprints with flags; {} forbid triangles from {} instances; no \
         collision: {}",
        objs.len(),
        groups.len(),
        flags.len(),
        count(&forbid),
        forbid.len(),
        str_list_repr(&missing)
    ))?;
    Ok(0)
}

/// `levelobjects.py <level dir> <level name> [extra blueprint dirs ...]`.
pub fn run(py: &mut Py, args: &[String]) -> PyResult<i32> {
    if args.len() < 2 {
        py.print(&crate::pyfmt::docstring(include_str!(
            "../../../../bfbc2-auto-navkit/tools/levelobjects.py"
        )))?;
        return Ok(1);
    }
    main(py, &args[0], &args[1], &args[2..])
}
