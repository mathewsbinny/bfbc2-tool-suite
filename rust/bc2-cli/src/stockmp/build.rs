//! `pipeline.build` (round O5): every level-00 bundle of the level unpacked into the work folder, its level partition
//! authored (`bc2_stockmp::author`, round E4a), the `SectorOffsets` assets made from the donor's (`sector_asset`) and the
//! AI template's partitions added (`_add_entries`), the bundles packed (`bc2_formats::fbrb::pack_many`), the path
//! data staged, `out.partial\` swapped in and `build_report.json` written - the same files, report and printed lines.
//!
//! Handed to the Python (before anything is printed): a configuration or level the authoring port refuses, a bundle
//! the in-process unpack or the packer refuses, path data the reader refuses, a value `struct.pack` refuses, a
//! manifest that is not ASCII, a donor asset of another shape.

#![allow(
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::type_complexity,
    reason = "a port of Python: indexes after their lengths are checked, the reference's names and its tuples kept"
)]

use std::fs;

use bc2_formats::dbx::edit::{EditDbx, numbers_of};
use bc2_formats::{fbrb, hpd};
use bc2_pyrt::json::{self as pyjson, Value};
use bc2_stockmp::author::{self, Config, Fields, OrderCfg, SpawnerCfg, StageCfg, Val};
use bc2_stockmp::guid::new_guid;
use bc2_stockmp::install::{isfile, join};

use super::Paths;
use super::level::{self, build_error, common, find_entry, level_entry, level_name, sha, unpacked};
use super::pyx;
use crate::pyfmt::str_repr;
use crate::pyos;
use crate::pyproc::{Fail, Py, PyResult, os_error};

const SECTOR_NS: &str = "opfor:sectoroffsets";
const MIN_AGREEMENT: f64 = 0.75;
pub const KINDS: [(&str, &str); 2] = [("Infantry", "infantry"), ("Vehicle", "vehicle")];
pub const TEMPLATE_PARTS: [&str; 3] = [
    "Objects/Soldiers/Templates/SpawnTemplate_Enemy_Grunt.dbx",
    "Objects/Soldiers/Templates/AI/Enemy_Grunt_Sense.dbx",
    "Objects/Soldiers/Templates/AI/Enemy_Grunt_Bhv.dbx",
];

// ------------------------------------------------------------------------------------------------ the configuration

fn val(v: &Value) -> Option<Val> {
    match v {
        Value::Bool(b) => Some(Val::Bool(*b)),
        Value::Int(i) => i64::try_from(*i).ok().map(Val::Int),
        Value::Float(f) => Some(Val::Float(*f)),
        Value::Str(s) => Some(Val::Str(s.clone())),
        _ => None,
    }
}

fn number(v: &Value) -> Option<f64> {
    match val(v)? {
        #[allow(clippy::cast_precision_loss, reason = "within 2^53: exact")]
        Val::Int(i) if i.unsigned_abs() <= 1 << 53 => Some(i as f64),
        Val::Float(f) if f.is_finite() => Some(f),
        _ => None,
    }
}

fn string(v: &Value) -> Option<String> {
    v.as_str().map(str::to_owned)
}

fn fields(v: Option<&Value>) -> Option<Fields> {
    let Some(v) = v else {
        return Some(Fields::default());
    };
    let Value::Dict(items) = v else { return None };
    let mut f = Fields::default();
    for (k, x) in items {
        f.0.push((k.clone(), val(x)?));
    }
    Some(f)
}

fn triple(v: &Value) -> Option<[f64; 3]> {
    let l = v.as_list()?;
    if l.len() != 3 {
        return None;
    }
    Some([number(l.first()?)?, number(l.get(1)?)?, number(l.get(2)?)?])
}

fn strings(v: &Value) -> Option<Vec<String>> {
    v.as_list()?.iter().map(string).collect()
}

#[allow(clippy::option_option, reason = "outer: the shape refused; inner: no stage")]
fn stage_of(v: Option<&Value>) -> Option<Option<i64>> {
    match v {
        None | Some(Value::Null) => Some(None),
        Some(Value::Int(i)) => Some(Some(i64::try_from(*i).ok()?)),
        _ => None,
    }
}

fn items<'a>(c: &'a Value, key: &str) -> Option<Vec<&'a Value>> {
    match c.get(key) {
        None | Some(Value::Null) => Some(Vec::new()),
        Some(Value::List(l)) => {
            if l.iter().all(|x| matches!(x, Value::Dict(_))) {
                Some(l.iter().collect())
            } else {
                None
            }
        }
        Some(_) => None,
    }
}

/// The configuration as `author.Author` reads it (`bc2-py`'s `stockmp.rs`, on the JSON); None: a shape the port
/// leaves to the Python.
#[allow(
    clippy::too_many_lines,
    reason = "the configuration, key for key as the reference reads it"
)]
pub fn config(c: &Value) -> Option<Config> {
    let lv = string(c.get("lv")?)?;
    let stock_human = c.get("stock_spawners").and_then(Value::as_str) == Some("human");
    let minimal = match c.get("layer") {
        None => false,
        Some(l) => l.as_str() == Some("minimal"),
    };
    let defaults = c.get("defaults")?;
    if !matches!(defaults, Value::Dict(_)) {
        return None;
    }
    let defaults_spawner = fields(Some(defaults.get("spawner")?))?;
    let defaults_order = fields(Some(defaults.get("order")?))?;
    let issue_default = match defaults.get("issue_on") {
        Some(x) => strings(x)?,
        None => vec!["spawn".to_owned()],
    };
    let template = val(c.get("template")?)?;
    let teams = c.get("teams")?;
    if !matches!(teams, Value::Dict(_)) {
        return None;
    }
    let mut spawners = Vec::new();
    for s in items(c, "spawners")? {
        let id = string(s.get("id")?)?;
        let team = string(s.get("team")?)?;
        let tv = teams.get(&team)?;
        let kits = tv.get("kits")?;
        if !matches!(tv, Value::Dict(_)) || !matches!(kits, Value::Dict(_)) {
            return None;
        }
        let kit = string(s.get("kit")?)?;
        spawners.push(SpawnerCfg {
            kit_ref: val(kits.get(&kit)?)?,
            template: match s.get("template") {
                Some(t) => val(t)?,
                None => template.clone(),
            },
            name: match s.get("name") {
                Some(n) => val(n)?,
                None => Val::Str(format!("OPFOR AI {id}")),
            },
            stage: stage_of(s.get("stage"))?,
            enabled: match s.get("enabled") {
                Some(e) => val(e)?,
                None => Val::Bool(true),
            },
            fields: fields(s.get("fields"))?,
            pos: triple(s.get("pos")?)?,
            yaw: match s.get("yaw") {
                Some(y) => number(y)?,
                None => 0.0,
            },
            id,
            team,
        });
    }
    let mut orders = Vec::new();
    for o in items(c, "orders")? {
        let id = string(o.get("id")?)?;
        orders.push(OrderCfg {
            soldiers: strings(o.get("soldiers")?)?,
            fields: fields(o.get("fields"))?,
            name: match o.get("name") {
                Some(n) => val(n)?,
                None => Val::Str(format!("OPFOR {id}")),
            },
            stage: stage_of(o.get("stage"))?,
            area_type: match o.get("type") {
                Some(t) => val(t)?,
                None => Val::Str("AreaMoveTo".into()),
            },
            pos: triple(o.get("pos")?)?,
            half: match o.get("half_extents") {
                Some(h) => triple(h)?,
                None => [8.0, 3.0, 8.0],
            },
            issue_on: match o.get("issue_on") {
                Some(i) => strings(i)?,
                None => issue_default.clone(),
            },
            trigger_radius: match o.get("trigger_radius") {
                Some(r) => number(r)?,
                None => 100.0,
            },
            id,
        });
    }
    let mut stages = Vec::new();
    for st in items(c, "stages")? {
        let advance = st.get("advance")?;
        if !matches!(advance, Value::Dict(_)) {
            return None;
        }
        stages.push(StageCfg {
            index: match val(st.get("index")?)? {
                Val::Int(i) => i,
                _ => return None,
            },
            source: string(advance.get("source")?)?,
            event: match advance.get("event") {
                Some(e) => string(e)?,
                None => "OnTrigger".into(),
            },
        });
    }
    Some(Config {
        lv,
        stock_human,
        minimal,
        defaults_spawner,
        defaults_order,
        spawners,
        orders,
        stages,
    })
}

// ------------------------------------------------------------------------------------------------ the pieces

/// `pipeline.sector_offsets(hpd)`: ((x, z), (agreement x, z)) from the path data's own sector ids.
pub fn sector_offsets(path: &str) -> PyResult<((i64, i64), (f64, f64))> {
    let raw = fs::read(path).map_err(|e| os_error(path, &e))?;
    let f = hpd::parse(&raw).map_err(|_| Fail::Python)?;
    let (mut cx, mut cz): (Vec<(i64, usize)>, Vec<(i64, usize)>) = (Vec::new(), Vec::new());
    let bump = |c: &mut Vec<(i64, usize)>, k: i64| match c.iter_mut().find(|(x, _)| *x == k) {
        Some(slot) => slot.1 += 1,
        None => c.push((k, 1)),
    };
    for s in &f.sectors {
        let b = s.bounds.map(f64::from);
        let x = pyx::floor((b[0] + b[1]) / 200.0)? - i64::from(s.id >> 8);
        let z = pyx::floor((b[4] + b[5]) / 200.0)? - i64::from(s.id & 0xff);
        bump(&mut cx, x);
        bump(&mut cz, z);
    }
    let n = f.sectors.len();
    if n == 0 {
        return Err(build_error(format!(
            "{path}: no sectors in the path data (an empty or failed build?)"
        )));
    }
    // most_common(1): the first of the largest counts
    let top = |c: &[(i64, usize)]| {
        let mut best = c[0];
        for &e in c {
            if e.1 > best.1 {
                best = e;
            }
        }
        best
    };
    let ((x, nx), (z, nz)) = (top(&cx), top(&cz));
    #[allow(clippy::cast_precision_loss, reason = "counts of sectors")]
    let (ax, az) = (nx as f64 / n as f64, nz as f64 / n as f64);
    if ax.min(az) < MIN_AGREEMENT {
        let shown = |c: &[(i64, usize)]| {
            let v: Vec<String> = c.iter().map(|(k, n)| format!("{k}: {n}")).collect();
            format!("{{{}}}", v.join(", "))
        };
        return Err(build_error(format!(
            "{path}: sector ids disagree ({} / {})",
            shown(&cx),
            shown(&cz)
        )));
    }
    Ok(((x, z), (ax, az)))
}

/// `pipeline.sector_asset(ref_path, level_upper, kind, lv, xz)`: (the partition's bytes).
fn sector_asset(ref_path: &str, upper: &str, kind: &str, lv: &str, xz: (i64, i64)) -> PyResult<Vec<u8>> {
    let data = fs::read(ref_path).map_err(|e| os_error(ref_path, &e))?;
    let mut d = EditDbx::parse(&data).map_err(|_| Fail::Python)?;
    let inst = d.instances();
    let one = match inst.as_slice() {
        [e] if d
            .attr(*e, b"type")
            .unwrap_or_default()
            .ends_with(b".PathdataSectorOffsets") =>
        {
            *e
        }
        _ => return Err(build_error(format!("{ref_path} is not a PathdataSectorOffsets asset"))),
    };
    let low = kind.to_lowercase();
    let part_g = new_guid(SECTOR_NS, &[lv, &low, "partition"]);
    let inst_g = new_guid(SECTOR_NS, &[lv, &low, "instance"]);
    let part = d.partition().ok_or(Fail::Python)?;
    d.set_attr(part, b"guid", part_g.as_bytes()).map_err(|_| Fail::Python)?;
    d.set_attr(part, b"primaryInstance", inst_g.as_bytes())
        .map_err(|_| Fail::Python)?;
    d.set_attr(one, b"guid", inst_g.as_bytes()).map_err(|_| Fail::Python)?;
    let Some(name) = d.child(one, b"Name") else {
        return Err(build_error("the SectorOffsets template has no Name field"));
    };
    d.set_text(
        name,
        format!("Levels/{upper}/Precomputed/{kind}/SectorOffsets").as_bytes(),
    )
    .map_err(|_| Fail::Python)?;
    for (f, v) in [("XOffset", xz.0), ("ZOffset", xz.1)] {
        let Some(c) = d.child(one, f.as_bytes()) else {
            return Err(build_error(format!("the SectorOffsets template has no {f} field")));
        };
        if d.kind(c) != bc2_formats::dbx::NUMBERS {
            return Err(Fail::Python);
        }
        let (size, old) = numbers_of(d.raw(c)).ok_or(Fail::Python)?;
        if size != 4 || old.len() != 1 {
            return Err(build_error(format!("{ref_path}: {f} is not one 4-byte number")));
        }
        let v = i32::try_from(v).map_err(|_| Fail::Python)?;
        d.set_numbers(c, &[v.to_be_bytes()]).map_err(|_| Fail::Python)?;
    }
    d.write().map_err(|_| Fail::Python)
}

/// `pipeline.loads_sp_common(paths)`.
fn loads_sp_common(paths: &Paths) -> PyResult<bool> {
    let mft = join(&join(&join(&paths.game, "Dist"), "win32"), "package.mft");
    if !isfile(&mft) {
        return Ok(false);
    }
    let data = fs::read(&mft).map_err(|e| os_error(&mft, &e))?;
    let text = String::from_utf8_lossy(&data);
    let want = format!("game:levels/{}/level-00.fbrb", paths.lv);
    let mut block = Vec::new();
    let mut cur = false;
    for line in bc2_pyrt::py::splitlines(&text) {
        if let Some(rest) = line.strip_prefix("BundleSource ") {
            cur = level::py_lower(bc2_pyrt::py::strip(rest))? == want;
        } else if cur && let Some(rest) = line.strip_prefix("BundleTarget ") {
            block.push(level::py_lower(bc2_pyrt::py::strip(rest))?);
        }
    }
    Ok(block.iter().any(|b| b == "game:levels/sp_common/level-00.fbrb"))
}

/// `pipeline.template_parts(paths, parts)`: (entry name, file, manifest line).
fn template_parts(paths: &Paths, parts: &[String]) -> PyResult<Vec<(String, String, String)>> {
    let sp = common(paths, "sp_common")?;
    let lines = level::manifest_lines(&sp)?;
    let mut out = Vec::new();
    for p in parts {
        let pl = level::py_lower(p)?;
        let hit = lines
            .iter()
            .find(|l| l.split('|').next().unwrap_or("").to_ascii_lowercase() == pl && !l.contains("|*deleted*|"));
        let name = hit.map(|l| l.split('|').next().unwrap_or("").to_owned());
        let f = name.as_deref().and_then(|n| find_entry(&sp, n));
        let (Some(name), Some(f), Some(line)) = (name, f, hit) else {
            return Err(build_error(format!("{p} is not in sp_common")));
        };
        out.push((name, f, line.clone()));
    }
    Ok(out)
}

/// `pipeline.donor(paths)`: DICE's `SP_004_B` level partition and its Infantry `SectorOffsets` asset.
fn donor(paths: &Paths) -> PyResult<(String, String)> {
    for v in ["pkg", "dist"] {
        let base = join(&join(&join(&join(&paths.opfor, "work"), "levels"), "sp_004_b"), v);
        let lvl = join(&join(&base, "levels"), "SP_004_B.dbx");
        let inf = ["levels", "sp_004_b", "Precomputed", "Infantry", "SectorOffsets.dbx"]
            .iter()
            .fold(base.clone(), |a, n| join(&a, n));
        if isfile(&lvl) && isfile(&inf) && isfile(&donor_sector_asset(&inf, "Vehicle")) {
            return Ok((lvl, inf));
        }
    }
    let bundle = join(
        &join(&join(&join(&join(&paths.game, "Dist"), "win32"), "levels"), "sp_004_B"),
        "level-00.fbrb",
    );
    let d = unpacked(&bundle, &join(&paths.cache(), "sp_004_b"))?;
    let lvl = find_entry(&d, "Levels/SP_004_B.dbx");
    let inf = find_entry(&d, "Levels/SP_004_B/Precomputed/Infantry/SectorOffsets.dbx");
    let veh = find_entry(&d, "Levels/SP_004_B/Precomputed/Vehicle/SectorOffsets.dbx");
    match (lvl, inf, veh) {
        (Some(l), Some(i), Some(_)) => Ok((l, i)),
        _ => Err(build_error(
            "the donor level SP_004_B or one of its SectorOffsets assets (Infantry, Vehicle) was not found",
        )),
    }
}

/// `pipeline.donor_sector_asset(donor_so, kind)`.
fn donor_sector_asset(donor_so: &str, kind: &str) -> String {
    let up = bc2_pyrt::py::dirname(&bc2_pyrt::py::dirname(donor_so));
    join(&join(&up, kind), "SectorOffsets.dbx")
}

/// `open(path, encoding='utf-8').read()` with universal newlines (`\r\n` and `\r` read as `\n`).
fn read_text_universal(path: &str) -> PyResult<String> {
    let data = fs::read(path).map_err(|e| os_error(path, &e))?;
    let t = String::from_utf8(data).map_err(|_| Fail::Python)?;
    Ok(t.replace("\r\n", "\n").replace('\r', "\n"))
}

/// `pipeline._add_entries(d, added)`.
fn add_entries(d: &str, added: &[(String, String, String)]) -> PyResult<()> {
    let m = join(d, "fbrb.manifest");
    let mut text = read_text_universal(&m)?;
    if !text.is_ascii() {
        return Err(Fail::Python);
    }
    let names: Vec<String> = bc2_pyrt::py::splitlines(&text)
        .into_iter()
        .filter(|x| !x.contains("|*deleted*|"))
        .map(|x| x.split('|').next().unwrap_or("").to_ascii_lowercase())
        .collect();
    for (name, f, line) in added {
        if names.contains(&level::py_lower(name)?) {
            return Err(build_error(format!("{name} is already in the bundle")));
        }
        let dst = name.split('/').fold(d.to_owned(), |a, n| join(&a, n));
        let dir = bc2_pyrt::py::dirname(&dst);
        fs::create_dir_all(&dir).map_err(|e| os_error(&dir, &e))?;
        fs::copy(f, &dst).map_err(|e| os_error(f, &e))?;
        text = format!("{}\n{line}\n", text.trim_end_matches(['\r', '\n']));
    }
    fs::write(&m, text.as_bytes()).map_err(|e| os_error(&m, &e))
}

/// `shutil.rmtree(p, ignore_errors=True)`.
fn rmtree(p: &str) {
    if bc2_pyrt::py::isdir(p) {
        pyos::rmtree_quiet(p);
    }
}

fn copy(src: &str, dst: &str) -> PyResult<()> {
    fs::copy(src, dst).map(|_| ()).map_err(|e| os_error(src, &e))
}

fn sv(x: &str) -> Value {
    Value::Str(x.to_owned())
}

fn file_rec(path: &str) -> PyResult<Value> {
    Ok(Value::dict([("path", sv(path)), ("sha256", sv(&sha(path)?))]))
}

/// `dict` assignment keeping Python's order.
fn set(items: &mut Vec<(String, Value)>, k: &str, v: Value) {
    match items.iter_mut().find(|(x, _)| x == k) {
        Some(slot) => slot.1 = v,
        None => items.push((k.to_owned(), v)),
    }
}

/// `str(dict)` of the authoring report.
pub fn report_text(r: &[(String, i64)]) -> String {
    let v: Vec<String> = r.iter().map(|(k, n)| format!("{}: {n}", str_repr(k))).collect();
    format!("{{{}}}", v.join(", "))
}

/// A variant's authoring report as JSON.
pub fn report_value(r: &[(String, i64)]) -> Value {
    Value::Dict(r.iter().map(|(k, n)| (k.clone(), Value::Int(i128::from(*n)))).collect())
}

/// What the build leaves for `main` to print: each variant's (name, authoring report).
pub type Built = Vec<(String, Vec<(String, i64)>)>;

/// `pipeline.build(paths, c)`.
#[allow(clippy::too_many_lines, reason = "pipeline.build, step for step")]
pub fn build(py: &mut Py, paths: &Paths, c: &Value, problems: &[String]) -> PyResult<Built> {
    if !problems.is_empty() {
        return Err(build_error(format!("configuration: {}", problems.join("; "))));
    }
    let cfg = config(c).ok_or(Fail::Python)?;
    let layer = match c.get("layer") {
        None => "full".to_owned(),
        Some(Value::Str(s)) => s.clone(),
        Some(_) => return Err(Fail::Python),
    };
    let mut inputs: Vec<(String, Value)> = Vec::new();
    let mut outputs: Vec<(String, Value)> = Vec::new();
    let mut variants: Vec<(String, Value)> = Vec::new();
    let mut so_rep: Vec<(String, Value)> = Vec::new();
    if paths.bundles()?.is_empty() {
        return Err(build_error(format!(
            "{}: no level-00 bundle in {}",
            paths.lv, paths.game
        )));
    }
    fs::create_dir_all(&paths.root).map_err(|e| os_error(&paths.root, &e))?;
    let report = paths.report();
    if isfile(&report) {
        fs::remove_file(&report).map_err(|e| os_error(&report, &e))?;
    }
    let stage = "out.partial";
    rmtree(&join(&paths.root, stage));
    let (donor_lvl, donor_so) = donor(paths)?;
    set(&mut inputs, "donor", file_rec(&donor_lvl)?);
    let pd = match c.get("pathdata") {
        Some(v) if v.truthy() => v.clone(),
        _ => Value::Dict(Vec::new()),
    };
    if !matches!(pd, Value::Dict(_)) {
        return Err(Fail::Python);
    }
    let mut so: Vec<(i64, i64)> = Vec::new();
    for (kind, folder) in KINDS {
        let src = match pd.get(folder) {
            None | Some(Value::Null) => None,
            Some(Value::Str(s)) => Some(s.clone()),
            Some(_) => return Err(Fail::Python),
        };
        let Some(src) = src.filter(|s| !s.is_empty() && isfile(s)) else {
            let shown = match pd.get(folder) {
                Some(Value::Str(s)) => s.clone(),
                _ => "None".to_owned(),
            };
            return Err(build_error(format!("path data for {folder} not found: {shown}")));
        };
        let (xz, agree) = sector_offsets(&src)?;
        so.push(xz);
        set(
            &mut so_rep,
            kind,
            Value::dict([
                ("x", Value::Int(i128::from(xz.0))),
                ("z", Value::Int(i128::from(xz.1))),
                (
                    "agree",
                    Value::floats(&[pyx::round(agree.0, 3)?, pyx::round(agree.1, 3)?]),
                ),
            ]),
        );
        set(&mut inputs, &format!("pathdata_{folder}"), file_rec(&src)?);
        let dst = paths.out_pathdata(folder, stage);
        let dir = bc2_pyrt::py::dirname(&dst);
        fs::create_dir_all(&dir).map_err(|e| os_error(&dir, &e))?;
        copy(&src, &dst)?;
        let rel = pyos::relpath(&paths.out_pathdata(folder, "out"), &paths.root)?;
        set(&mut outputs, &rel, sv(&sha(&dst)?));
    }
    let pack = !loads_sp_common(paths)?;
    let parts = if pack && layer != "minimal" {
        let names: Vec<String> = match c.get("template_parts") {
            Some(v) if v.truthy() => strings(v).ok_or(Fail::Python)?,
            _ => TEMPLATE_PARTS.iter().map(|s| (*s).to_owned()).collect(),
        };
        template_parts(paths, &names)?
    } else {
        Vec::new()
    };
    let template_pack = Value::List(parts.iter().map(|(p, _, _)| sv(p)).collect());
    let donor_data = fs::read(&donor_lvl).map_err(|e| os_error(&donor_lvl, &e))?;
    let mut packs: Vec<(String, String, String, Value, String, Vec<(String, i64)>)> = Vec::new();
    for (v, bundle) in paths.bundles()? {
        let src = unpacked(&bundle, &paths.src(v))?;
        set(&mut inputs, &format!("bundle_{v}"), file_rec(&bundle)?);
        let upper = level_name(&src, &paths.lv)?;
        let entry = level_entry(&src, &upper)?;
        let dst = join(&join(&paths.root, "build"), v);
        py.print(&format!(
            "{v}: the unpacked bundle to {}",
            pyos::relpath(&dst, &paths.opfor)?
        ))?;
        rmtree(&dst);
        // build_folder: unpacked in process (a bundle it refuses: the Python copies its cached unpack)
        if fbrb::unpack_path(&bundle, &dst).is_err() {
            return Err(Fail::Python);
        }
        let Some(entry) = entry else {
            return Err(build_error(format!("{}: no level partition in {src}", paths.lv)));
        };
        let Some(lvl_file) = find_entry(&dst, &entry) else {
            return Err(build_error(format!(
                "{}: the level partition {entry} is not in {dst}",
                paths.lv
            )));
        };
        let original = fs::read(&lvl_file).map_err(|e| os_error(&lvl_file, &e))?;
        let (data, counts) = author::author(&original, &donor_data, &cfg).map_err(|_| Fail::Python)?;
        let build_dir = join(&paths.root, "build");
        copy(&lvl_file, &join(&build_dir, &format!("{upper}_{v}.original.dbx")))?;
        fs::write(&lvl_file, &data).map_err(|e| os_error(&lvl_file, &e))?;
        copy(&lvl_file, &join(&build_dir, &format!("{upper}_{v}.authored.dbx")))?;
        let mut added: Vec<(String, String, String)> = Vec::new();
        for (k, (kind, _)) in KINDS.iter().enumerate() {
            let rf = donor_sector_asset(&donor_so, kind);
            let raw = sector_asset(&rf, &upper, kind, &paths.lv, so[k])?;
            let name = format!("Levels/{upper}/Precomputed/{kind}/SectorOffsets.dbx");
            let f = join(&build_dir, &format!("SectorOffsets_{kind}.dbx"));
            fs::write(&f, &raw).map_err(|e| os_error(&f, &e))?;
            let line = format!("{name}|<non-resource>|00010000|{}", raw.len());
            added.push((name, f, line));
        }
        added.extend(parts.iter().cloned());
        add_entries(&dst, &added)?;
        let out = paths.out_bundle(v, stage);
        let dir = bc2_pyrt::py::dirname(&out);
        fs::create_dir_all(&dir).map_err(|e| os_error(&dir, &e))?;
        let var = Value::dict([
            ("entry", sv(&entry)),
            ("authored", report_value(&counts.0)),
            (
                "added_entries",
                Value::List(added.iter().map(|(n, _, _)| sv(n)).collect()),
            ),
            ("level_sha256_before", sv(&pyos::sha256_hex(&original))),
            ("level_sha256_after", sv(&pyos::sha256_hex(&data))),
        ]);
        let text = report_text(&counts.0);
        packs.push((v.to_owned(), dst, out, var, text, counts.0));
    }
    let items: Vec<(&str, &str)> = packs.iter().map(|(_, d, o, ..)| (d.as_str(), o.as_str())).collect();
    for r in fbrb::pack_many(&items) {
        if r.is_err() {
            return Err(Fail::Python);
        }
    }
    let mut built = Vec::new();
    for (v, _dst, out, var, text, counts) in packs {
        set(&mut variants, &v, var);
        let rel = pyos::relpath(&paths.out_bundle(&v, "out"), &paths.root)?;
        set(&mut outputs, &rel, sv(&sha(&out)?));
        py.print(&format!(
            "{v}: {text}; packed {}",
            pyos::relpath(&paths.out_bundle(&v, "out"), &paths.opfor)?
        ))?;
        built.push((v, counts));
    }
    let rep = Value::dict([
        ("lv", sv(&paths.lv)),
        ("level", c.get("level").cloned().ok_or(Fail::Python)?),
        ("layer", sv(&layer)),
        ("inputs", Value::Dict(inputs)),
        ("outputs", Value::Dict(outputs)),
        ("variants", Value::Dict(variants)),
        ("sector_offsets", Value::Dict(so_rep)),
        ("template_pack", template_pack),
        (
            "guids",
            Value::dict([(
                "note",
                sv("deterministic: UUIDv5(NAMESPACE_URL, \"opfor:stockmp:<lv>:<role>:<id>\")"),
            )]),
        ),
    ]);
    swap_in(paths, stage)?;
    let tmp = format!("{report}.tmp");
    let text = pyjson::dumps(&rep, Some(1)).replace('\n', "\r\n");
    fs::write(&tmp, text.as_bytes()).map_err(|e| os_error(&tmp, &e))?;
    crate::fsx::replace(&tmp, &report).map_err(|e| os_error(&tmp, &e))?;
    Ok(built)
}

/// `pipeline.swap_in(paths, stage)`.
fn swap_in(paths: &Paths, stage: &str) -> PyResult<()> {
    let (fin, old) = (join(&paths.root, "out"), join(&paths.root, "out.old"));
    rmtree(&old);
    if bc2_pyrt::py::isdir(&fin) {
        fs::rename(&fin, &old).map_err(|e| os_error(&fin, &e))?;
    }
    let st = join(&paths.root, stage);
    fs::rename(&st, &fin).map_err(|e| os_error(&st, &e))?;
    rmtree(&old);
    Ok(())
}
