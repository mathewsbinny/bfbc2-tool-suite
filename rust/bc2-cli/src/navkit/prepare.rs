//! `navkit.prepare` (the shared and the level's bundles unpacked and converted, objects / collision / spawns
//! extracted, ladders, the sampling box and sector offsets into `navkit.json`) and the terrain step the editor's
//! prepare adds (`bfbc2core.level.heightfield_of`: the level's `terrain-00.fbrb` unpacked into
//! `work\levels\<lv>\terrain` once, stamped).

use regex::Regex;

use super::project::{Log, Project};
use super::{ladders, levelobjects, navkit_error, read_text_utf8};
use crate::pyfmt::float_g;
use crate::pyjson::{self, Value};
use crate::pyos::{self, join, join2};
use crate::pyproc::{Fail, Py, PyErr, PyResult};

/// `fmt(v)`: `','.join(f'{a:g}' for a in v)`.
pub fn fmt(v: &[Num]) -> String {
    v.iter().map(Num::g).collect::<Vec<_>>().join(",")
}

/// A Python number of the box: an int (`math.floor` / `ceil`'s) or a float (`--box`'s).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Num {
    I(i64),
    F(f64),
}

impl Num {
    fn g(&self) -> String {
        match self {
            Num::I(i) => float_g(*i as f64),
            Num::F(f) => float_g(*f),
        }
    }

    pub fn f(&self) -> f64 {
        match self {
            Num::I(i) => *i as f64,
            Num::F(f) => *f,
        }
    }

    fn json(&self) -> Value {
        match self {
            Num::I(i) => Value::Int(i128::from(*i)),
            Num::F(f) => Value::Float(*f),
        }
    }
}

/// `level_partition_name(L, lv)`.
pub fn level_partition_name(l: &str, lv: &str) -> PyResult<String> {
    for f in pyos::glob(&join(l, &["dist", "levels", "*.xml"])?)? {
        let b = pyos::splitext(&pyos::basename(&f)?).0;
        if b.to_lowercase() == lv {
            return Ok(b);
        }
    }
    Err(navkit_error(format!(
        "{lv}: level partition levels/{lv}.xml not found in the bundle"
    )))
}

/// `sector_offsets_of(xml_path)`.
pub fn sector_offsets_of(xml_path: &str) -> PyResult<(i64, i64)> {
    let t = read_text_utf8(xml_path)?;
    let get = |name: &str| -> PyResult<Option<i64>> {
        let re = Regex::new(&format!(r#"name="{name}">(-?\d+)<"#)).map_err(|_| Fail::Python)?;
        match re.captures(&t) {
            None => Ok(None),
            Some(c) => {
                let s = c.get(1).map_or("", |m| m.as_str());
                // \d is Unicode in Python and int() reads those digits: left to Python
                if !s.trim_start_matches('-').bytes().all(|b| b.is_ascii_digit()) {
                    return Err(Fail::Python);
                }
                s.parse::<i64>().map(Some).map_err(|_| Fail::Python)
            }
        }
    };
    match (get("XOffset")?, get("ZOffset")?) {
        (Some(x), Some(z)) => Ok((x, z)),
        _ => Err(navkit_error(format!(
            "{xml_path}: no XOffset / ZOffset value (re-run navkit.py prepare)"
        ))),
    }
}

/// The area outlines' points of every sector of a path-data file (and, `portals`, the portals' ends).
fn outline_points(path: &str, portals: bool) -> PyResult<Vec<[f64; 3]>> {
    bc2_navbuild::inputs::outline_points(path, portals).map_err(super::build::fail)
}

/// `navregress.reference_bounds(root, lv)`.
pub fn reference_bounds(pr: &Project, lv: &str) -> PyResult<Option<[f64; 4]>> {
    bc2_navbuild::inputs::reference_bounds(&super::build::root(pr), lv).map_err(super::build::fail)
}

fn floor10(v: f64) -> PyResult<i64> {
    let f = (v / 10.0).floor();
    if !f.is_finite() {
        return Err(Fail::Python);
    }
    Ok(f as i64 * 10)
}

fn ceil10(v: f64) -> PyResult<i64> {
    let f = (v / 10.0).ceil();
    if !f.is_finite() {
        return Err(Fail::Python);
    }
    Ok(f as i64 * 10)
}

fn load_list(path: &str) -> PyResult<Vec<Value>> {
    let data = pyos::read(path)?;
    let v = pyjson::loads_ansi(&data).ok_or(Fail::Python)?;
    v.as_list().map(<[Value]>::to_vec).ok_or(Fail::Python)
}

/// `sampling_box(pr, lv, L)`: (box, y, source).
pub fn sampling_box(pr: &Project, lv: &str, l: &str) -> PyResult<(Vec<Num>, Vec<Num>, &'static str)> {
    let mut pts: Vec<[f64; 3]> = Vec::new();
    let mut margin = 0.0;
    let mut source = "";
    if let Some(r) = pr.dice_pathdata(lv)? {
        pts = outline_points(&r, false)?;
        margin = 20.0;
        source = "extent of the level's path data + 20 m";
    }
    if pts.is_empty() {
        for name in ["objects.json", "group_instances.json"] {
            for o in load_list(&join2(l, name)?)? {
                let m = o.get("m").and_then(Value::as_list).ok_or(Fail::Python)?;
                let g = |k: usize| m.get(k).and_then(Value::as_f64).ok_or(Fail::Python);
                pts.push([g(12)?, g(13)?, g(14)?]);
            }
        }
        for p in load_list(&join2(l, "soldier_spawns.json")?)? {
            let p = p.as_list().ok_or(Fail::Python)?;
            if p.len() != 3 {
                return Err(Fail::Python);
            }
            let g = |k: usize| p.get(k).and_then(Value::as_f64).ok_or(Fail::Python);
            pts.push([g(0)?, g(1)?, g(2)?]);
        }
        margin = 50.0;
        source = "extent of the level's objects + 50 m";
    }
    if pts.is_empty() {
        return Err(navkit_error(format!(
            "{lv}: no objects to size the sampling box - pass --box and --y"
        )));
    }
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in &pts {
        for k in 0..3 {
            if p[k].is_nan() {
                return Err(Fail::Python);
            }
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let bx = vec![
        Num::I(floor10(lo[0] - margin)?),
        Num::I(floor10(lo[2] - margin)?),
        Num::I(ceil10(hi[0] + margin)?),
        Num::I(ceil10(hi[2] + margin)?),
    ];
    let yr = vec![Num::I(floor10(lo[1] - 30.0)?), Num::I(ceil10(hi[1] + 150.0)?)];
    Ok((bx, yr, source))
}

/// `prepare(pr, lv, log, box, y)`.
pub fn prepare(
    py: &mut Py,
    pr: &Project,
    lv: &str,
    log: &mut dyn Log,
    bx: Option<&[f64]>,
    y: Option<&[f64]>,
) -> PyResult<()> {
    log.log(py, &format!("== prepare {lv}"))?;
    let mut common = Vec::new();
    for c in ["sp_common", "mp_common"] {
        let dest = join(&pr.work, &["navkit", "common", c])?;
        pr.unpack(
            py,
            &join(&pr.game, &["Dist", "win32", "levels", c, "level-00.fbrb"])?,
            &dest,
            log,
        )?;
        common.push(dest);
    }
    let l = pr.level_work(lv)?;
    let ldir = pr.level_dir(lv)?;
    pr.unpack(py, &join2(&ldir, "level-00.fbrb")?, &join2(&l, "dist")?, log)?;
    let pkg = join(
        &pr.game,
        &["Package", "levels", &pyos::basename(&ldir)?, "level-00.fbrb"],
    )?;
    if pyos::isfile(&pkg) {
        pr.unpack(py, &pkg, &join2(&l, "pkg")?, log)?;
    } else {
        pyos::makedirs(&join2(&l, "pkg")?)?;
    }
    let lvname = level_partition_name(&l, lv)?;
    log.log(py, "  extracting objects, collision and spawn points ...")?;
    levelobjects::main(py, &l, &lvname, &common)?;
    let lad = ladders::extract(&l, &common)?;
    pyos::atomic_write_text(&join2(&l, "ladders.json")?, &pyjson::dumps(&lad, Some(1)))?;
    log.log(
        py,
        &format!(
            "  ladders: {} (ladder links: their far ends seeded and linked in the build)",
            lad.as_list().map_or(0, <[Value]>::len)
        ),
    )?;

    let so = pyos::glob(&join(
        &l,
        &["dist", "levels", lv, "Precomputed", "Infantry", "SectorOffsets.xml"],
    )?)?;
    let mut offsets = match so.first() {
        Some(p) => Some(sector_offsets_of(p)?),
        None => None,
    };
    let given_box = bx.map(|v| v.iter().map(|x| Num::F(*x)).collect::<Vec<_>>());
    let given_y = y.map(|v| v.iter().map(|x| Num::F(*x)).collect::<Vec<_>>());
    let (auto_box, auto_y, source) = if bx.is_none() || y.is_none() {
        match sampling_box(pr, lv, &l) {
            Ok((a, b, s)) => (Some(a), Some(b), s),
            Err(Fail::Err(e)) if e.kind == "NavKitError" || is_os_error(&e) => {
                if bx.is_none() == y.is_none() {
                    return Err(Fail::Err(e));
                }
                return Err(navkit_error(format!(
                    "{lv}: no objects to size the sampling {}",
                    if y.is_none() {
                        "height range - pass --y as well"
                    } else {
                        "box - pass --box as well"
                    }
                )));
            }
            Err(e) => return Err(e),
        }
    } else {
        (None, None, "given")
    };
    // `box or auto_box`: an empty given list is falsy
    let the_box = given_box
        .clone()
        .filter(|v| !v.is_empty())
        .or(auto_box)
        .unwrap_or_default();
    let the_y = given_y.filter(|v| !v.is_empty()).or(auto_y).unwrap_or_default();
    let box_source = if given_box.as_ref().is_some_and(|v| !v.is_empty()) {
        "given"
    } else {
        source
    };
    let offsets_source;
    if offsets.is_none() {
        let ext = reference_bounds(pr, lv)?;
        let lo = match ext {
            Some(e) => [e[0], e[1]],
            None => [
                the_box.first().map(Num::f).ok_or(Fail::Python)?,
                the_box.get(1).map(Num::f).ok_or(Fail::Python)?,
            ],
        };
        offsets = Some(((lo[0] / 100.0).floor() as i64 - 1, (lo[1] / 100.0).floor() as i64 - 1));
        offsets_source = if ext.is_some() {
            "rule: path data extent"
        } else {
            "rule: box"
        };
    } else {
        offsets_source = "SectorOffsets";
    }
    let (xoff, zoff) = offsets.unwrap_or_default();
    let s = Value::dict([
        ("level", Value::Str(lvname)),
        ("box", Value::List(the_box.iter().map(Num::json).collect())),
        ("y", Value::List(the_y.iter().map(Num::json).collect())),
        ("box_source", Value::Str(box_source.to_owned())),
        ("offsets_source", Value::Str(offsets_source.to_owned())),
        ("xoff", Value::Int(i128::from(xoff))),
        ("zoff", Value::Int(i128::from(zoff))),
    ]);
    let sp = pr.settings_path(lv)?;
    pyos::atomic_write_text(&sp, &pyjson::dumps(&s, Some(1)))?;
    let b = |k: usize| the_box.get(k).map(Num::f).ok_or(Fail::Python);
    let cols = (b(2)? - b(0)?) / 0.8 * (b(3)? - b(1)?) / 0.8;
    log.log(
        py,
        &format!(
            "  box {} ({box_source}), y {}, sector offsets {xoff}, {zoff} ({offsets_source}); {:.1} M columns per \
             sampling run",
            fmt(&the_box),
            fmt(&the_y),
            cols / 1e6
        ),
    )?;
    log.log(py, &format!("  settings: {}", pyos::relpath(&sp, &pr.root)?))
}

fn is_os_error(e: &PyErr) -> bool {
    matches!(e.kind.as_str(), "OSError" | "FileNotFoundError" | "PermissionError")
}

/// `level.heightfield_of(root, lv, log)` (its terrain name returned, for `water_of`): the terrain unpacked when its
/// unpack is missing or from another bundle, then read (its error raised) - `bc2_navbuild::inputs::heightfield_of`.
pub fn heightfield_of(py: &mut Py, root: &str, lv: &str, log: &mut dyn Log) -> PyResult<String> {
    let mut kept: Option<Fail> = None;
    let r = bc2_navbuild::inputs::heightfield_of(
        root,
        lv,
        &mut |line| {
            log.log(py, line).map_err(|e| {
                kept.get_or_insert(e);
                bc2_navbuild::ntpath::Fail::Sink
            })
        },
        &super::build::tool,
    );
    match r {
        Ok(v) => Ok(v),
        Err(bc2_navbuild::ntpath::Fail::Sink) => Err(kept.unwrap_or(Fail::Python)),
        Err(e) => Err(super::build::fail(e)),
    }
}
