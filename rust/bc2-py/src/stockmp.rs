//! The stock-MP AI layer (`bc2_stockmp`, round E4a) for `bfbc2-stockmp-ai`: the editable DBX's round trip, the
//! level model, the authoring, the checks' readings and the folder readings.
//!
//! Every function returns `None` where the port refuses (`bc2_stockmp::Refuse`: an input, or a path through it, the
//! Python reference answers itself); the caller then runs its Python, which raises or answers by its own rules. A
//! configuration is read from its Python dictionary as the reference reads it; any value of a shape the port does
//! not take (a `None`, a list where a value is set, a string where a number is) refuses the whole call.

use bc2_formats::dbx::edit;
use bc2_stockmp::author::{self, Config, Fields, OrderCfg, SpawnerCfg, StageCfg, Val};
use bc2_stockmp::model::{CapturePoint, Model, Pos, Spawner, Stage};
use bc2_stockmp::{checks, files};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};

/// A string of the file as Python's `bytes.decode('utf-8', 'surrogateescape')` gives it.
fn pystr<'py>(py: Python<'py>, b: &[u8]) -> PyResult<Bound<'py, PyAny>> {
    match std::str::from_utf8(b) {
        Ok(s) => Ok(PyString::new(py, s).into_any()),
        Err(_) => PyBytes::new(py, b).call_method1("decode", ("utf-8", "surrogateescape")),
    }
}

fn opt_str<'py>(py: Python<'py>, b: Option<&[u8]>) -> PyResult<Bound<'py, PyAny>> {
    match b {
        Some(b) => pystr(py, b),
        None => Ok(py.None().into_bound(py)),
    }
}

fn pos(py: Python<'_>, p: Option<Pos>) -> PyResult<Bound<'_, PyAny>> {
    match p {
        Some((x, y, z)) => Ok(PyTuple::new(py, [x, y, z])?.into_any()),
        None => Ok(py.None().into_bound(py)),
    }
}

/// `dbxbin`'s round trip of each file on all cores: whether `write(parse(data)) == data`, `None` where the port
/// refuses the file (the Python reference then decides).
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn stockmp_dbx_round_trips(py: Python<'_>, items: Vec<Bound<'_, PyBytes>>) -> Vec<Option<bool>> {
    let slices: Vec<&[u8]> = items.iter().map(PyBytesMethods::as_bytes).collect();
    py.detach(|| edit::round_trips(&slices))
}

/// `dbxbin.new_guid(namespace, *parts)`.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn stockmp_new_guid(namespace: &str, parts: Vec<String>) -> String {
    let p: Vec<&str> = parts.iter().map(String::as_str).collect();
    bc2_stockmp::guid::new_guid(namespace, &p)
}

fn capture_point<'py>(py: Python<'py>, c: &CapturePoint) -> PyResult<Bound<'py, PyTuple>> {
    let spawners = PyList::empty(py);
    for s in &c.spawners {
        spawners.append(pystr(py, s)?)?;
    }
    let outline = PyList::empty(py);
    for p in &c.outline {
        outline.append(pos(py, Some(*p))?)?;
    }
    PyTuple::new(
        py,
        [
            pystr(py, &c.guid)?,
            pystr(py, &c.name)?,
            pystr(py, &c.label)?,
            pos(py, c.pos)?,
            opt_str(py, c.owner.as_deref())?,
            spawners.into_any(),
            outline.into_any(),
        ],
    )
}

fn spawner<'py>(py: Python<'py>, s: &Spawner) -> PyResult<Bound<'py, PyTuple>> {
    let alts = PyList::empty(py);
    for (g, p) in &s.alternates {
        alts.append(PyTuple::new(py, [opt_str(py, g.as_deref())?, pos(py, Some(*p))?])?)?;
    }
    PyTuple::new(
        py,
        [
            pystr(py, &s.guid)?,
            pystr(py, &s.name)?,
            opt_str(py, s.team.as_deref())?,
            opt_str(py, s.player_type.as_deref())?,
            pos(py, s.pos)?,
            PyBool::new(py, s.enabled).to_owned().into_any(),
            alts.into_any(),
            opt_str(py, s.flag.as_deref())?,
        ],
    )
}

fn stage<'py>(py: Python<'py>, s: &Stage) -> PyResult<Bound<'py, PyTuple>> {
    let mcoms = PyList::empty(py);
    for (g, p) in &s.mcoms {
        mcoms.append(PyTuple::new(py, [pystr(py, g)?, pos(py, *p)?])?)?;
    }
    PyTuple::new(
        py,
        [
            s.index.into_pyobject(py)?.into_any(),
            pystr(py, &s.trigger)?,
            pystr(py, &s.trigger_name)?,
            mcoms.into_any(),
        ],
    )
}

/// `levelmodel.Model` of a level partition's bytes as a dictionary of its fields (`name`, `game_mode`, `mode`,
/// `factions`, `attacker`, `has_ai`, `events`, `objects`, `capture_points`, `spawners`, `mcoms` as tuples in the
/// dataclasses' field order, and `stages`: `rush_stages()`, or `None` where the port leaves that to Python), or
/// `None` where the port refuses the file.
#[pyfunction]
pub fn stockmp_level_model<'py>(py: Python<'py>, data: &Bound<'py, PyBytes>) -> PyResult<Option<Bound<'py, PyDict>>> {
    let bytes = data.as_bytes();
    let got = py.detach(|| {
        let d = edit::EditDbx::parse(bytes).ok()?;
        let m = Model::new(&d).ok()?;
        let stages = m.rush_stages(&d).ok();
        Some((m, stages))
    });
    let Some((m, stages)) = got else {
        return Ok(None);
    };
    let out = PyDict::new(py);
    out.set_item("name", opt_str(py, m.name.as_deref())?)?;
    out.set_item("game_mode", pystr(py, &m.game_mode)?)?;
    out.set_item("mode", pystr(py, &m.mode)?)?;
    let factions = PyList::empty(py);
    for (t, f) in &m.factions {
        factions.append(PyTuple::new(py, [pystr(py, t)?, opt_str(py, f.as_deref())?])?)?;
    }
    out.set_item("factions", factions)?;
    out.set_item("attacker", opt_str(py, m.attacker.as_deref())?)?;
    out.set_item("has_ai", m.has_ai)?;
    let events = PyList::empty(py);
    for (s, se, t, te) in &m.events {
        events.append(PyTuple::new(
            py,
            [
                opt_str(py, s.as_deref())?,
                opt_str(py, se.as_deref())?,
                opt_str(py, t.as_deref())?,
                opt_str(py, te.as_deref())?,
            ],
        )?)?;
    }
    out.set_item("events", events)?;
    let objects = PyList::empty(py);
    for o in &m.objects {
        objects.append(opt_str(py, o.as_deref())?)?;
    }
    out.set_item("objects", objects)?;
    let cps = PyList::empty(py);
    for c in &m.capture_points {
        cps.append(capture_point(py, c)?)?;
    }
    out.set_item("capture_points", cps)?;
    let sps = PyList::empty(py);
    for s in &m.spawners {
        sps.append(spawner(py, s)?)?;
    }
    out.set_item("spawners", sps)?;
    let mcoms = PyList::empty(py);
    for (g, n, p) in &m.mcoms {
        mcoms.append(PyTuple::new(py, [pystr(py, g)?, pystr(py, n)?, pos(py, *p)?])?)?;
    }
    out.set_item("mcoms", mcoms)?;
    match stages {
        Some(st) => {
            let l = PyList::empty(py);
            for s in &st {
                l.append(stage(py, s)?)?;
            }
            out.set_item("stages", l)?;
        }
        None => out.set_item("stages", py.None())?,
    }
    Ok(Some(out))
}

// ------------------------------------------------------------------------------- the configuration, as read

fn val(o: &Bound<'_, PyAny>) -> Option<Val> {
    if o.is_instance_of::<PyBool>() {
        return o.extract::<bool>().ok().map(Val::Bool);
    }
    if o.is_instance_of::<PyInt>() {
        return o.extract::<i64>().ok().map(Val::Int);
    }
    if o.is_instance_of::<PyFloat>() {
        return o.extract::<f64>().ok().map(Val::Float);
    }
    if let Ok(s) = o.cast::<PyString>() {
        return s.to_str().ok().map(|s| Val::Str(s.to_owned()));
    }
    None
}

/// A number of a position, size or angle: an `int` (within 2^53, exact as a double) or a finite `float`.
fn number(o: &Bound<'_, PyAny>) -> Option<f64> {
    match val(o)? {
        #[expect(clippy::cast_precision_loss, reason = "within 2^53: exact")]
        Val::Int(i) if i.unsigned_abs() <= 1 << 53 => Some(i as f64),
        Val::Float(f) if f.is_finite() => Some(f),
        _ => None,
    }
}

fn string(o: &Bound<'_, PyAny>) -> Option<String> {
    o.cast::<PyString>().ok()?.to_str().ok().map(str::to_owned)
}

fn get<'py>(d: &Bound<'py, PyDict>, k: &str) -> Option<Bound<'py, PyAny>> {
    d.get_item(k).ok().flatten()
}

fn dict<'py>(o: &Bound<'py, PyAny>) -> Option<Bound<'py, PyDict>> {
    o.cast::<PyDict>().ok().cloned()
}

fn fields(o: Option<Bound<'_, PyAny>>) -> Option<Fields> {
    let Some(o) = o else {
        return Some(Fields::default());
    };
    let d = dict(&o)?;
    let mut f = Fields::default();
    for (k, v) in d.iter() {
        f.0.push((string(&k)?, val(&v)?));
    }
    Some(f)
}

fn triple(o: &Bound<'_, PyAny>) -> Option<[f64; 3]> {
    let items: Vec<Bound<'_, PyAny>> = if let Ok(l) = o.cast::<PyList>() {
        l.iter().collect()
    } else if let Ok(t) = o.cast::<PyTuple>() {
        t.iter().collect()
    } else {
        return None;
    };
    if items.len() != 3 {
        return None;
    }
    Some([number(items.first()?)?, number(items.get(1)?)?, number(items.get(2)?)?])
}

fn strings(o: &Bound<'_, PyAny>) -> Option<Vec<String>> {
    let l = o.cast::<PyList>().ok()?;
    l.iter().map(|x| string(&x)).collect()
}

/// `stage`: `None` or an `int` (a `bool` refused).
#[expect(clippy::option_option, reason = "outer: the shape refused; inner: no stage")]
fn stage_of(o: Option<Bound<'_, PyAny>>) -> Option<Option<i64>> {
    match o {
        None => Some(None),
        Some(x) if x.is_none() => Some(None),
        Some(x) => match val(&x)? {
            Val::Int(i) => Some(Some(i)),
            _ => None,
        },
    }
}

/// A list the reference reads as `c.get(key) or ()`: missing, `None` or empty is none.
fn items<'py>(c: &Bound<'py, PyDict>, key: &str) -> Option<Vec<Bound<'py, PyDict>>> {
    let Some(o) = get(c, key) else {
        return Some(Vec::new());
    };
    if o.is_none() {
        return Some(Vec::new());
    }
    let l = o.cast::<PyList>().ok()?;
    l.iter().map(|x| dict(&x)).collect()
}

#[expect(
    clippy::too_many_lines,
    reason = "the configuration, key for key as the reference reads it"
)]
fn config(c: &Bound<'_, PyDict>) -> Option<Config> {
    let lv = string(&get(c, "lv")?)?;
    let stock_human = get(c, "stock_spawners").and_then(|s| string(&s)).as_deref() == Some("human");
    let minimal = match get(c, "layer") {
        None => false,
        Some(l) => string(&l).as_deref() == Some("minimal"),
    };
    let defaults = dict(&get(c, "defaults")?)?;
    let defaults_spawner = fields(Some(get(&defaults, "spawner")?))?;
    let defaults_order = fields(Some(get(&defaults, "order")?))?;
    let issue_default = match get(&defaults, "issue_on") {
        Some(x) => strings(&x)?,
        None => vec!["spawn".to_owned()],
    };
    let template = val(&get(c, "template")?)?;
    let teams = dict(&get(c, "teams")?)?;
    let mut spawners = Vec::new();
    for s in items(c, "spawners")? {
        let id = string(&get(&s, "id")?)?;
        let team = string(&get(&s, "team")?)?;
        let kits = dict(&get(&dict(&get(&teams, &team)?)?, "kits")?)?;
        let kit = string(&get(&s, "kit")?)?;
        spawners.push(SpawnerCfg {
            kit_ref: val(&get(&kits, &kit)?)?,
            template: match get(&s, "template") {
                Some(t) => val(&t)?,
                None => template.clone(),
            },
            name: match get(&s, "name") {
                Some(n) => val(&n)?,
                None => Val::Str(format!("OPFOR AI {id}")),
            },
            stage: stage_of(get(&s, "stage"))?,
            enabled: match get(&s, "enabled") {
                Some(e) => val(&e)?,
                None => Val::Bool(true),
            },
            fields: fields(get(&s, "fields"))?,
            pos: triple(&get(&s, "pos")?)?,
            yaw: match get(&s, "yaw") {
                Some(y) => number(&y)?,
                None => 0.0,
            },
            id,
            team,
        });
    }
    let mut orders = Vec::new();
    for o in items(c, "orders")? {
        let id = string(&get(&o, "id")?)?;
        orders.push(OrderCfg {
            soldiers: strings(&get(&o, "soldiers")?)?,
            fields: fields(get(&o, "fields"))?,
            name: match get(&o, "name") {
                Some(n) => val(&n)?,
                None => Val::Str(format!("OPFOR {id}")),
            },
            stage: stage_of(get(&o, "stage"))?,
            area_type: match get(&o, "type") {
                Some(t) => val(&t)?,
                None => Val::Str("AreaMoveTo".into()),
            },
            pos: triple(&get(&o, "pos")?)?,
            half: match get(&o, "half_extents") {
                Some(h) => triple(&h)?,
                None => [8.0, 3.0, 8.0],
            },
            issue_on: match get(&o, "issue_on") {
                Some(i) => strings(&i)?,
                None => issue_default.clone(),
            },
            trigger_radius: match get(&o, "trigger_radius") {
                Some(r) => number(&r)?,
                None => 100.0,
            },
            id,
        });
    }
    let mut stages = Vec::new();
    for st in items(c, "stages")? {
        let advance = dict(&get(&st, "advance")?)?;
        stages.push(StageCfg {
            index: match val(&get(&st, "index")?)? {
                Val::Int(i) => i,
                _ => return None,
            },
            source: string(&get(&advance, "source")?)?,
            event: match get(&advance, "event") {
                Some(e) => string(&e)?,
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

/// `pipeline.build`'s authoring of one level partition: `(authored bytes, [(report key, count), ...])`, or `None`
/// where the port refuses (the configuration's shape, the file, or a path where the reference raises).
#[pyfunction]
pub fn stockmp_author<'py>(
    py: Python<'py>,
    original: &Bound<'py, PyBytes>,
    donor: &Bound<'py, PyBytes>,
    cfg: &Bound<'py, PyDict>,
) -> Option<Authored<'py>> {
    let c = config(cfg)?;
    let (o, d) = (original.as_bytes(), donor.as_bytes());
    let (bytes, report) = py.detach(|| author::author(o, d, &c)).ok()?;
    Some((PyBytes::new(py, &bytes), report.0))
}

/// The authored bytes and the report's `(key, count)` pairs.
type Authored<'py> = (Bound<'py, PyBytes>, Vec<(String, i64)>);

/// Why the port refused a configuration or an authoring (tests and logs): `None` when it takes them.
#[pyfunction]
pub fn stockmp_author_refusal(
    original: &Bound<'_, PyBytes>,
    donor: &Bound<'_, PyBytes>,
    cfg: &Bound<'_, PyDict>,
) -> Option<String> {
    let Some(c) = config(cfg) else {
        return Some("the configuration's shape".into());
    };
    author::author(original.as_bytes(), donor.as_bytes(), &c)
        .err()
        .map(|r| r.0.to_owned())
}

// ------------------------------------------------------------------------------- validate's readings

type PyStructure = (
    Vec<String>,
    Vec<(String, String)>,
    usize,
    usize,
    usize,
    Vec<(String, String)>,
    usize,
    String,
    Vec<String>,
);

/// `check_structure`'s reading: `(twice, local_unresolved, objects_twice, new, unlisted, event_unresolved, events,
/// has_ai, pathfinding)`, or `None`.
#[pyfunction]
pub fn stockmp_check_structure(
    py: Python<'_>,
    orig: &Bound<'_, PyBytes>,
    authored: &Bound<'_, PyBytes>,
) -> Option<PyStructure> {
    let (o, a) = (orig.as_bytes(), authored.as_bytes());
    let s = py.detach(|| checks::structure(o, a)).ok()?;
    Some((
        s.twice,
        s.local_unresolved,
        s.objects_twice,
        s.new,
        s.unlisted,
        s.event_unresolved,
        s.events,
        s.has_ai,
        s.pathfinding,
    ))
}

type PyShown = Option<checks::Shown>;

/// `check_changes`' reading: per stock instance with a change, `(guid, removed, [array changed], [(path, stock,
/// authored)])` (a leaf `(kind, value, [(name, value)])` or `None`), or `None`.
#[pyfunction]
#[expect(clippy::type_complexity, reason = "the tuple Python reads")]
pub fn stockmp_check_changes(
    py: Python<'_>,
    orig: &Bound<'_, PyBytes>,
    authored: &Bound<'_, PyBytes>,
) -> Option<Vec<(String, bool, Vec<String>, Vec<(String, PyShown, PyShown)>)>> {
    let (o, a) = (orig.as_bytes(), authored.as_bytes());
    let c = py.detach(|| checks::changes(o, a)).ok()?;
    Some(
        c.into_iter()
            .map(|c| (c.guid, c.removed, c.arrays_changed, c.leaves))
            .collect(),
    )
}

/// `check_external`'s reading: the new references into other partitions, sorted, or `None`.
#[pyfunction]
pub fn stockmp_check_external(
    py: Python<'_>,
    orig: &Bound<'_, PyBytes>,
    authored: &Bound<'_, PyBytes>,
) -> Option<Vec<String>> {
    let (o, a) = (orig.as_bytes(), authored.as_bytes());
    py.detach(|| checks::external(o, a)).ok()
}

/// `check_stages`' reading: per GUID asked, `(type, Enabled value)` of its instance or `None`; `None` for a file
/// the reader refuses.
#[pyfunction]
#[expect(clippy::type_complexity, reason = "the tuple Python reads")]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn stockmp_instance_facts(
    py: Python<'_>,
    authored: &Bound<'_, PyBytes>,
    guids: Vec<String>,
) -> Option<Vec<Option<(Option<String>, Option<String>)>>> {
    let a = authored.as_bytes();
    py.detach(|| checks::instance_facts(a, &guids)).ok()
}

/// `validate._folder_index(top)` as `[(key, path)]`, or `None`.
#[pyfunction]
pub fn stockmp_folder_index(py: Python<'_>, top: &str) -> Option<Vec<(String, String)>> {
    py.detach(|| files::folder_index(top)).ok()
}

/// `check_packed`'s reading: `(files compared, files that differ)`, or `None`.
#[pyfunction]
pub fn stockmp_packed_check(py: Python<'_>, bundle: &str, build_dir: &str) -> Option<(usize, usize)> {
    py.detach(|| files::packed_check(bundle, build_dir)).ok()
}
