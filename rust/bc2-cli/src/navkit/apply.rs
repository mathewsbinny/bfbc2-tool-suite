//! `navkit.py apply` on a DICE base (round NK1): the side file's links, route edges and traversal annotations on a
//! copy of DICE's path data (`navkit.apply` / `_apply` with `navlinks.apply` and `navtraversal.write`, whose Python
//! was retired: `bc2_navbuild::graphedit`). A NavKit base rebuilds the level with the walkability edits (round NK5:
//! `bc2_navbuild::build`, [`super::build`]). A side file the port does not read (not JSON, a value of a type the
//! checks refuse) and a path-data file `graphedit` refuses are handed off before anything is printed.

use bc2_navbuild::graphedit::{self, EditError};
use bc2_navbuild::navtune;
use std::time::Instant;

use super::navkit_error;
use super::project::Project;
use crate::pyfmt::float_repr;
use crate::pyjson::{self, Value};
use crate::pyos::{self, join, join2};
use crate::pyproc::{Fail, Py, PyErr, PyResult};

/// The side file as `navtune.load` reads it.
struct Tune {
    level: Value,
    base: Value,
    ops: Vec<Value>,
    kind: String,
}

/// A side-file value as `bfbc2core.navtune`'s checks take it (`None`: an int past 64 bits).
fn tune_value(v: &Value) -> Option<navtune::Value> {
    Some(match v {
        Value::Null => navtune::Value::Null,
        Value::Bool(b) => navtune::Value::Bool(*b),
        Value::Int(i) => navtune::Value::Int(i64::try_from(*i).ok()?),
        Value::Float(f) => navtune::Value::Float(*f),
        Value::Str(s) => navtune::Value::Str(s.clone()),
        Value::List(l) => navtune::Value::List(l.iter().map(tune_value).collect::<Option<_>>()?),
        Value::Dict(d) => navtune::Value::Dict(
            d.iter()
                .map(|(k, x)| Some((k.clone(), tune_value(x)?)))
                .collect::<Option<_>>()?,
        ),
    })
}

/// `navtune.load(path)` with `_apply`'s `except ValueError as e: raise NavKitError(str(e))`; what the port does not
/// read is handed to the Python.
fn load(path: &str) -> PyResult<Tune> {
    let data = pyos::read(path)?;
    let Some(d @ Value::Dict(_)) = pyjson::loads_utf8(&data) else {
        return Err(Fail::Python); // json's error text, a BOM, a document that is not a dict: the Python's
    };
    if d.get("format").and_then(Value::as_str) != Some("bfbc2-navtune") {
        return Err(navkit_error(format!("{path}: not a navtune side file")));
    }
    let newer = match d.get("version") {
        None | Some(Value::Bool(_)) => false,
        Some(Value::Int(i)) => *i > 1,
        Some(Value::Float(f)) => *f > 1.0,
        Some(_) => return Err(Fail::Python), // a comparison Python raises on
    };
    if newer {
        let v = match d.get("version") {
            Some(Value::Int(i)) => i.to_string(),
            Some(Value::Float(f)) => float_repr(*f),
            _ => return Err(Fail::Python),
        };
        return Err(navkit_error(format!(
            "{path}: version {v} is newer than this tool reads (1)"
        )));
    }
    let level = d.get("level").cloned().unwrap_or(Value::Str(String::new()));
    let base = d.get("base").cloned().unwrap_or(Value::Dict(Vec::new()));
    let Some(Value::List(ops)) = d.get("operations").cloned().or(Some(Value::List(Vec::new()))) else {
        return Err(Fail::Python); // list() of another type
    };
    if !matches!(base, Value::Dict(_)) {
        return Err(Fail::Python); // base.get raises
    }
    let kind = match base.get("kind") {
        None => "navkit".to_owned(),
        Some(Value::Str(s)) => s.clone(),
        Some(_) => return Err(Fail::Python), // navtune.validate refuses it
    };
    let mut problems = Vec::new();
    for (k, o) in ops.iter().enumerate() {
        let o = tune_value(o).ok_or(Fail::Python)?;
        for p in navtune::validate(&o, &kind).ok_or(Fail::Python)? {
            problems.push(format!("operation {k}: {p}"));
        }
    }
    if !problems.is_empty() {
        problems.truncate(10);
        return Err(navkit_error(format!("{path}: {}", problems.join("; "))));
    }
    Ok(Tune { level, base, ops, kind })
}

fn op_is(o: &Value, names: &[&str]) -> bool {
    o.get("op").and_then(Value::as_str).is_some_and(|n| names.contains(&n))
}

/// The reference's exception for a stage error.
fn raised(e: EditError) -> Fail {
    match e {
        EditError::Raised { kind, msg } => Fail::Err(PyErr::new(kind, msg)),
        EditError::Refused(_) => Fail::Python,
    }
}

/// What the edit stage gives for DICE's file, worked out before anything is printed (a refusal still hands the line
/// to the Python): the graph edits, the read-back, the annotations.
struct Plan {
    graph: Option<graphedit::GraphEdits>,
    /// `Err`: what the read-back raised (the file as `_apply` reads it back).
    exact: Result<bool, EditError>,
    traversal: Option<Result<graphedit::Traversal, EditError>>,
}

fn plan(src: &[u8], tune: &Tune, out_name: &str, graph: bool, later: bool) -> PyResult<Result<Plan, EditError>> {
    let g = if graph {
        match graphedit::graph_edits(src, &tune.ops) {
            Ok(g) => Some(g),
            Err(EditError::Refused(_)) => return Err(Fail::Python),
            Err(e) => return Ok(Err(e)),
        }
    } else {
        None
    };
    let data = g.as_ref().map_or(src, |g| g.data.as_slice());
    let exact = graphedit::read_back(data);
    if let Err(EditError::Refused(_)) = exact {
        return Err(Fail::Python);
    }
    let traversal = match (&exact, later) {
        (Ok(true), true) => {
            let t = graphedit::traversal(data, out_name, &tune.ops, &tune.level);
            if let Err(EditError::Refused(_)) = t {
                return Err(Fail::Python);
            }
            Some(t)
        }
        _ => None,
    };
    Ok(Ok(Plan {
        graph: g,
        exact,
        traversal,
    }))
}

/// `navkit.apply(pr, lv, navtune_path, out, install_after=...)`: on a DICE base answered here; a NavKit base handed to
/// the Python before anything is printed.
pub(super) fn apply(
    py: &mut Py,
    pr: &Project,
    lv: &str,
    (navtune_path, out): (Option<&str>, Option<&str>),
    install_after: bool,
    no_detail: bool,
) -> PyResult<()> {
    let path = match navtune_path.filter(|p| !p.is_empty()) {
        Some(p) => p.to_owned(),
        None => join(&pr.root, &["work", "navkit", lv, &format!("{lv}.navtune.json")])?,
    };
    if !pyos::isfile(&path) {
        return Err(navkit_error(format!(
            "{lv}: no side file {path} (make edits in the map editor's AI workspace first)"
        )));
    }
    let tune = load(&path)?;
    let t0 = Instant::now();
    if tune.kind == "navkit" {
        return navkit_base(py, pr, lv, &tune, (&path, out), install_after, no_detail, t0);
    }
    let walk = tune
        .ops
        .iter()
        .any(|o| op_is(o, &["walkable", "unwalkable", "seed", "unseed"]));
    let graph = tune.ops.iter().any(|o| op_is(o, &graphedit::GRAPH_OPS));
    let later = tune.ops.iter().filter(|o| op_is(o, &["traversal"])).count();
    let notes = tune.ops.iter().filter(|o| op_is(o, &["note"])).count();
    if walk {
        return Err(navkit_error(format!(
            "{lv}: region edits need a NavKit base (DESIGN.md 5.1)"
        )));
    }
    let Some(src) = pr.dice_pathdata(lv)? else {
        return Err(navkit_error(format!(
            "{lv}: the side file's base is DICE's data, but the level has none"
        )));
    };
    let out = match out.filter(|o| !o.is_empty()) {
        Some(o) => o.to_owned(),
        None => join2(&pr.out_dir(lv)?, "pathdata_dice_edited.hpd")?,
    };
    // the stage worked out first (pure): a refusal hands the line to the Python while nothing is printed yet; a
    // source the copy cannot read is the copy's error, raised where the Python meets it
    let src_data = std::fs::read(&src);
    let planned = match &src_data {
        Ok(d) => Some(plan(d, &tune, &pyos::basename(&out)?, graph, later > 0)?),
        Err(_) => None,
    };
    let mut lines: Vec<String> = Vec::new();
    let r = run(
        py,
        pr,
        lv,
        &tune,
        &src,
        &out,
        src_data,
        planned,
        &mut lines,
        (later, notes, t0),
    );
    match r {
        Ok(()) => {
            pyos::write_text_utf8(&format!("{out}.report.txt"), &format!("{}\n", lines.join("\n")))?;
            if install_after {
                super::driver::install(py, pr, lv, Some(&out))?;
            }
            Ok(())
        }
        Err(e) => {
            let e = match e {
                Fail::Err(e) => e,
                // past the first line printed the Python cannot take the run over: said in words
                Fail::Python => PyErr::new(
                    "NavKitError",
                    "the Rust port of navkit apply refused an input the Python would read (bfbc2.exe)",
                ),
            };
            lines.push(format!("error: {}", e.msg));
            pyos::write_text_utf8(&format!("{out}.report.txt"), &format!("{}\n", lines.join("\n")))?;
            Err(Fail::Err(e))
        }
    }
}

/// `_apply`'s DICE branch from its first line, each line printed and kept for the report.
#[allow(clippy::too_many_arguments)]
fn run(
    py: &mut Py,
    pr: &Project,
    lv: &str,
    tune: &Tune,
    src: &str,
    out: &str,
    src_data: std::io::Result<Vec<u8>>,
    planned: Option<Result<Plan, EditError>>,
    lines: &mut Vec<String>,
    (later, notes, t0): (usize, usize, Instant),
) -> PyResult<()> {
    let mut say = |py: &mut Py, msg: String| -> PyResult<()> {
        py.print(&msg)?;
        lines.push(msg);
        Ok(())
    };
    say(
        py,
        format!(
            "== apply {lv}: {} edit(s) on a copy of DICE's file -> {} (for this PC only)",
            tune.ops.len(),
            pyos::relpath(out, &pr.root)?
        ),
    )?;
    let sha = tune.base.get("sha1");
    if sha.is_some_and(Value::truthy) {
        let data = src_data.as_ref().map_err(|e| crate::pyproc::os_error(src, e))?;
        let now = pyos::sha1_hex(data);
        if sha != Some(&Value::Str(now)) {
            say(
                py,
                "  note: DICE's file is not the one the edits were made on (another game version?); the edits are by \
                 position and are applied, but check them"
                    .to_owned(),
            )?;
        }
    }
    // shutil.copyfile(src, out)
    if same_file(src, out) {
        return Err(Fail::Err(PyErr::new(
            "SameFileError",
            format!(
                "{} and {} are the same file",
                crate::pyfmt::str_repr(src),
                crate::pyfmt::str_repr(out)
            ),
        )));
    }
    let data = src_data.map_err(|e| crate::pyproc::os_error(src, &e))?;
    std::fs::write(out, &data).map_err(|e| crate::pyproc::os_error(out, &e))?;
    let plan = match planned {
        Some(Ok(p)) => p,
        Some(Err(e)) => return Err(raised(e)),
        None => return Err(Fail::Python),
    };
    finish(py, pr, lv, out, plan, lines, (later, notes, t0))
}

/// `_apply` after the base is in `out`: the graph edits written, the read-back, the annotations, the last lines.
fn finish(
    py: &mut Py,
    pr: &Project,
    lv: &str,
    out: &str,
    plan: Plan,
    lines: &mut Vec<String>,
    (later, notes, t0): (usize, usize, Instant),
) -> PyResult<()> {
    let mut say = |py: &mut Py, msg: String| -> PyResult<()> {
        py.print(&msg)?;
        lines.push(msg);
        Ok(())
    };
    if let Some(g) = &plan.graph {
        for l in &g.lines {
            say(py, l.clone())?;
        }
        std::fs::write(out, &g.data).map_err(|e| crate::pyproc::os_error(out, &e))?;
        say(py, g.summary.clone())?;
    }
    match plan.exact {
        Ok(true) => {}
        Ok(false) => {
            return Err(navkit_error(format!("{out}: does not read back exactly - not usable")));
        }
        Err(e) => return Err(raised(e)),
    }
    let trav = join2(&pyos::dirname(out)?, &format!("{lv}.traversal.json"))?;
    if let Some(t) = plan.traversal {
        let t = t.map_err(raised)?;
        for l in &t.lines {
            say(py, l.clone())?;
        }
        pyos::files::write_atomic(&trav, t.text.as_bytes()).map_err(|(at, e)| crate::pyproc::os_error(&at, &e))?;
        say(
            py,
            format!(
                "  {later} traversal annotation(s) written to {} (for OPFOR's bot layer; not route edges - the game's \
                 AI cannot climb or jump them)",
                pyos::relpath(&trav, &pr.root)?
            ),
        )?;
    } else if pyos::isfile(&trav) {
        std::fs::remove_file(&trav).map_err(|e| crate::pyproc::os_error(&trav, &e))?;
    }
    if notes > 0 {
        say(py, format!("  {notes} note(s): kept in the side file only"))?;
    }
    say(
        py,
        format!(
            "  read back exactly ({} s in all)",
            pyfixed0(t0.elapsed().as_secs_f64())
        ),
    )
}

/// `_apply`'s NavKit branch (round NK5, in Rust): the level built again with the side file's walkability edits
/// (`build_inputs`, `navbuild.build(..., edits=walk)`), then the graph edits, read-back and annotations as on a DICE
/// base; the report kept as `<output>.report.txt`.
#[allow(clippy::too_many_arguments)]
fn navkit_base(
    py: &mut Py,
    pr: &Project,
    lv: &str,
    tune: &Tune,
    (path, out): (&str, Option<&str>),
    install_after: bool,
    no_detail: bool,
    t0: Instant,
) -> PyResult<()> {
    let out = match out.filter(|o| !o.is_empty()) {
        Some(o) => o.to_owned(),
        None => join2(&pr.out_dir(lv)?, "pathdata.hpd")?,
    };
    let walk: Vec<Value> = tune
        .ops
        .iter()
        .filter(|o| op_is(o, &["walkable", "unwalkable", "seed", "unseed"]))
        .cloned()
        .collect();
    let graph = tune.ops.iter().any(|o| op_is(o, &graphedit::GRAPH_OPS));
    let later = tune.ops.iter().filter(|o| op_is(o, &["traversal"])).count();
    let notes = tune.ops.iter().filter(|o| op_is(o, &["note"])).count();
    // the inputs and the edits read first: what the port does not read hands the line over before anything is printed
    let gathered = super::build::build_inputs(py, pr, lv);
    if matches!(gathered, Err(Fail::Python)) {
        return Err(Fail::Python);
    }
    let edits = super::build::edits_of(&walk)?;
    let mut lines: Vec<String> = Vec::new();
    let r = (|| -> PyResult<()> {
        let head = format!(
            "== apply {lv}: {} edit(s) from {} -> {}",
            tune.ops.len(),
            pyos::relpath(path, &pr.root)?,
            pyos::relpath(&out, &pr.root)?
        );
        py.print(&head)?;
        lines.push(head);
        let mut g = gathered?;
        for l in &g.lines {
            py.print(l)?;
            lines.push(l.clone());
        }
        g.inputs.edits = (!walk.is_empty()).then_some(edits);
        super::build::run_build(py, &g.inputs, &out, no_detail, Some(&mut lines))?;
        let built = format!(
            "  built in {} s; {} walkability edit(s) applied",
            pyfixed0(t0.elapsed().as_secs_f64()),
            walk.len()
        );
        py.print(&built)?;
        lines.push(built);
        let data = std::fs::read(&out).map_err(|e| crate::pyproc::os_error(&out, &e))?;
        let planned = match plan(&data, tune, &pyos::basename(&out)?, graph, later > 0) {
            Ok(Ok(p)) => p,
            Ok(Err(e)) => return Err(raised(e)),
            Err(_) => {
                return Err(Fail::Err(PyErr::new(
                    "NavKitError",
                    "the Rust port of navkit apply refused an input the Python would read (bfbc2.exe)",
                )));
            }
        };
        finish(py, pr, lv, &out, planned, &mut lines, (later, notes, t0))
    })();
    match r {
        Ok(()) => {
            pyos::write_text_utf8(&format!("{out}.report.txt"), &format!("{}
", lines.join("
")))?;
            if install_after {
                super::driver::install(py, pr, lv, Some(&out))?;
            }
            Ok(())
        }
        Err(Fail::Err(e)) => {
            lines.push(format!("error: {}", e.msg));
            pyos::write_text_utf8(&format!("{out}.report.txt"), &format!("{}
", lines.join("
")))?;
            Err(Fail::Err(e))
        }
        Err(Fail::Python) => Err(Fail::Python),
    }
}

/// `f'{x:.0f}'`.
fn pyfixed0(x: f64) -> String {
    format!("{x:.0}")
}

/// `shutil._samefile(src, dst)`: the same file (`os.path.samefile`, when the target exists).
fn same_file(a: &str, b: &str) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}
