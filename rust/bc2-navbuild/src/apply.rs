//! `navkit apply` as a library (round NK6; the command line's since rounds NK1 and NK5): a navigation side file's
//! edits on a level's path data, the same lines and bytes whether `bfbc2.exe navkit apply` or the editor's AI
//! workspace (in process) runs it.
//!
//! - A **DICE base**: the side file's links, route edges and traversal annotations on a copy of DICE's file
//!   ([`crate::graphedit`]).
//! - A **NavKit base**: the level built again with the side file's walkability edits ([`crate::inputs`],
//!   [`crate::build`]), then the graph edits, the read-back and the annotations as on a DICE base.
//!
//! The report is kept as `<output>.report.txt`. A side file the port does not read (not JSON, a value of a type the
//! checks refuse) and a path-data file `graphedit` refuses hand off ([`Fail::Handoff`]) before anything is printed;
//! past the first line a refusal is a `NavKitError` in words.

#![allow(clippy::missing_errors_doc, clippy::too_many_lines)]

use std::time::Instant;

use bc2_shader::pyrepr::float_repr;

use crate::graphedit::{self, EditError};
use crate::inputs::{self, Out, Root, Run, ToolRun};
use crate::navtune;
use crate::ntpath::{self as os, Fail, Raised, Res, join, join2, navkit_error, os_error, raise};
use crate::pyjson::{self, Value};

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

/// `navtune.load(path)` with `_apply`'s `except ValueError as e: raise NavKitError(str(e))`.
fn load(path: &str) -> Res<Tune> {
    let data = os::read(path)?;
    let Some(d @ Value::Dict(_)) = pyjson::loads_utf8(&data) else {
        return Err(Fail::Handoff); // json's error text, a BOM, a document that is not a dict: the Python's
    };
    if d.get("format").and_then(Value::as_str) != Some("bfbc2-navtune") {
        return Err(navkit_error(format!("{path}: not a navtune side file")));
    }
    let newer = match d.get("version") {
        None | Some(Value::Bool(_)) => false,
        Some(Value::Int(i)) => *i > 1,
        Some(Value::Float(f)) => *f > 1.0,
        Some(_) => return Err(Fail::Handoff), // a comparison Python raises on
    };
    if newer {
        let v = match d.get("version") {
            Some(Value::Int(i)) => i.to_string(),
            Some(Value::Float(f)) => float_repr(*f),
            _ => return Err(Fail::Handoff),
        };
        return Err(navkit_error(format!(
            "{path}: version {v} is newer than this tool reads (1)"
        )));
    }
    let level = d.get("level").cloned().unwrap_or(Value::Str(String::new()));
    let base = d.get("base").cloned().unwrap_or(Value::Dict(Vec::new()));
    let Some(Value::List(ops)) = d.get("operations").cloned().or(Some(Value::List(Vec::new()))) else {
        return Err(Fail::Handoff); // list() of another type
    };
    if !matches!(base, Value::Dict(_)) {
        return Err(Fail::Handoff); // base.get raises
    }
    let kind = match base.get("kind") {
        None => "navkit".to_owned(),
        Some(Value::Str(s)) => s.clone(),
        Some(_) => return Err(Fail::Handoff), // navtune.validate refuses it
    };
    let mut problems = Vec::new();
    for (k, o) in ops.iter().enumerate() {
        let o = tune_value(o).ok_or(Fail::Handoff)?;
        for p in navtune::validate(&o, &kind).ok_or(Fail::Handoff)? {
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
        EditError::Raised { kind, msg } => raise(kind, msg),
        EditError::Refused(_) => Fail::Handoff,
    }
}

/// What the edit stage gives, worked out before anything is printed: the graph edits, the read-back, the annotations.
struct Plan {
    graph: Option<graphedit::GraphEdits>,
    /// `Err`: what the read-back raised.
    exact: Result<bool, EditError>,
    traversal: Option<Result<graphedit::Traversal, EditError>>,
}

/// The stage (pure): `Err(Handoff)` for a refusal, `Ok(Err(e))` for the reference's exception.
fn plan(src: &[u8], tune: &Tune, out_name: &str, graph: bool, later: bool) -> Res<Result<Plan, EditError>> {
    let g = if graph {
        match graphedit::graph_edits(src, &tune.ops) {
            Ok(g) => Some(g),
            Err(EditError::Refused(_)) => return Err(Fail::Handoff),
            Err(e) => return Ok(Err(e)),
        }
    } else {
        None
    };
    let data = g.as_ref().map_or(src, |g| g.data.as_slice());
    let exact = graphedit::read_back(data);
    if let Err(EditError::Refused(_)) = exact {
        return Err(Fail::Handoff);
    }
    let traversal = match (&exact, later) {
        (Ok(true), true) => {
            let t = graphedit::traversal(data, out_name, &tune.ops, &tune.level);
            if let Err(EditError::Refused(_)) = t {
                return Err(Fail::Handoff);
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

/// The sink, each line kept for the report.
struct Say<'a> {
    out: &'a mut dyn Out,
    lines: Vec<String>,
}

impl Say<'_> {
    fn say(&mut self, msg: String) -> Res<()> {
        self.out.line(&msg)?;
        self.lines.push(msg);
        Ok(())
    }

    /// `<out>.report.txt`: the lines (and the error's) as `open(..., 'w', encoding='utf-8')` writes them.
    fn report(&self, out: &str) -> Res<()> {
        os::write_text_utf8(&format!("{out}.report.txt"), &format!("{}\n", self.lines.join("\n")))
    }
}

/// The message past the first line where the port refused an input the Python would read.
const REFUSED: &str = "the Rust port of navkit apply refused an input the Python would read (bfbc2.exe)";

/// `navkit.apply(pr, lv, navtune_path, out)`: the side file's edits applied; the output's path returned (the caller
/// installs it when asked).
pub fn apply(
    r: &Root,
    lv: &str,
    (navtune_path, out): (Option<&str>, Option<&str>),
    run: Run,
    sink: &mut dyn Out,
    tool: ToolRun<'_>,
) -> Res<String> {
    let path = match navtune_path.filter(|p| !p.is_empty()) {
        Some(p) => p.to_owned(),
        None => join(&r.root, &["work", "navkit", lv, &format!("{lv}.navtune.json")])?,
    };
    if !os::isfile(&path) {
        return Err(navkit_error(format!(
            "{lv}: no side file {path} (make edits in the map editor's AI workspace first)"
        )));
    }
    let tune = load(&path)?;
    let t0 = Instant::now();
    if tune.kind == "navkit" {
        return navkit_base(r, lv, &tune, (&path, out), run, sink, tool, t0);
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
    let Some(src) = r.dice_pathdata(lv)? else {
        return Err(navkit_error(format!(
            "{lv}: the side file's base is DICE's data, but the level has none"
        )));
    };
    let out = match out.filter(|o| !o.is_empty()) {
        Some(o) => o.to_owned(),
        None => join2(&r.out_dir(lv)?, "pathdata_dice_edited.hpd")?,
    };
    // the stage worked out first (pure): a refusal hands off while nothing is printed yet; a source the copy cannot
    // read is the copy's error, raised where the Python meets it
    let src_data = std::fs::read(&src);
    let planned = match &src_data {
        Ok(d) => Some(plan(d, &tune, &os::basename(&out)?, graph, later > 0)?),
        Err(_) => None,
    };
    let mut say = Say {
        out: sink,
        lines: Vec::new(),
    };
    let ran = dice_run(
        r,
        lv,
        &tune,
        &src,
        &out,
        src_data,
        planned,
        &mut say,
        (later, notes, t0),
    );
    finish_report(ran, &say, &out).map(|()| out)
}

/// The report after a run that printed its first line; a refusal past it said in words.
fn finish_report(ran: Res<()>, say: &Say<'_>, out: &str) -> Res<()> {
    match ran {
        Ok(()) => say.report(out),
        Err(Fail::Sink) => Err(Fail::Sink),
        Err(Fail::Cancelled) => Err(Fail::Cancelled),
        Err(e) => {
            let e = match e {
                Fail::Raised(e) => e,
                _ => Raised {
                    kind: "NavKitError".to_owned(),
                    msg: REFUSED.to_owned(),
                    filename: None,
                },
            };
            let mut lines = say.lines.clone();
            lines.push(format!("error: {}", e.msg));
            os::write_text_utf8(&format!("{out}.report.txt"), &format!("{}\n", lines.join("\n")))?;
            Err(Fail::Raised(e))
        }
    }
}

/// `_apply`'s DICE branch from its first line.
#[allow(clippy::too_many_arguments)]
fn dice_run(
    r: &Root,
    lv: &str,
    tune: &Tune,
    src: &str,
    out: &str,
    src_data: std::io::Result<Vec<u8>>,
    planned: Option<Result<Plan, EditError>>,
    say: &mut Say<'_>,
    (later, notes, t0): (usize, usize, Instant),
) -> Res<()> {
    say.say(format!(
        "== apply {lv}: {} edit(s) on a copy of DICE's file -> {} (for this PC only)",
        tune.ops.len(),
        os::relpath(out, &r.root)?
    ))?;
    let sha = tune.base.get("sha1");
    if sha.is_some_and(Value::truthy) {
        let data = src_data.as_ref().map_err(|e| os_error(src, e))?;
        let now = sha1_smol::Sha1::from(data).digest().to_string();
        if sha != Some(&Value::Str(now)) {
            say.say(
                "  note: DICE's file is not the one the edits were made on (another game version?); the edits are by \
                 position and are applied, but check them"
                    .to_owned(),
            )?;
        }
    }
    // shutil.copyfile(src, out)
    if same_file(src, out) {
        return Err(raise(
            "SameFileError",
            format!("{} and {} are the same file", os::str_repr(src), os::str_repr(out)),
        ));
    }
    let data = src_data.map_err(|e| os_error(src, &e))?;
    std::fs::write(out, &data).map_err(|e| os_error(out, &e))?;
    let plan = match planned {
        Some(Ok(p)) => p,
        Some(Err(e)) => return Err(raised(e)),
        None => return Err(Fail::Handoff),
    };
    finish(r, lv, out, plan, say, (later, notes, t0))
}

/// `_apply` after the base is in `out`: the graph edits written, the read-back, the annotations, the last lines.
fn finish(
    r: &Root,
    lv: &str,
    out: &str,
    plan: Plan,
    say: &mut Say<'_>,
    (later, notes, t0): (usize, usize, Instant),
) -> Res<()> {
    if let Some(g) = &plan.graph {
        for l in &g.lines {
            say.say(l.clone())?;
        }
        std::fs::write(out, &g.data).map_err(|e| os_error(out, &e))?;
        say.say(g.summary.clone())?;
    }
    match plan.exact {
        Ok(true) => {}
        Ok(false) => {
            return Err(navkit_error(format!("{out}: does not read back exactly - not usable")));
        }
        Err(e) => return Err(raised(e)),
    }
    let trav = join2(&os::dirname(out)?, &format!("{lv}.traversal.json"))?;
    if let Some(t) = plan.traversal {
        let t = t.map_err(raised)?;
        for l in &t.lines {
            say.say(l.clone())?;
        }
        os::atomic_write(&trav, t.text.as_bytes())?;
        say.say(format!(
            "  {later} traversal annotation(s) written to {} (for OPFOR's bot layer; not route edges - the game's AI \
             cannot climb or jump them)",
            os::relpath(&trav, &r.root)?
        ))?;
    } else if os::isfile(&trav) {
        std::fs::remove_file(&trav).map_err(|e| os_error(&trav, &e))?;
    }
    if notes > 0 {
        say.say(format!("  {notes} note(s): kept in the side file only"))?;
    }
    say.say(format!(
        "  read back exactly ({} s in all)",
        pyfixed0(t0.elapsed().as_secs_f64())
    ))
}

/// `_apply`'s NavKit branch: the level built again with the side file's walkability edits, then the graph edits,
/// read-back and annotations as on a DICE base.
#[allow(clippy::too_many_arguments)]
fn navkit_base(
    r: &Root,
    lv: &str,
    tune: &Tune,
    (path, out): (&str, Option<&str>),
    run: Run,
    sink: &mut dyn Out,
    tool: ToolRun<'_>,
    t0: Instant,
) -> Res<String> {
    let out = match out.filter(|o| !o.is_empty()) {
        Some(o) => o.to_owned(),
        None => join2(&r.out_dir(lv)?, "pathdata.hpd")?,
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
    // the inputs and the edits read first: what the port does not read hands off before anything is printed
    let gathered = inputs::build_inputs(r, lv, tool);
    if matches!(gathered, Err(Fail::Handoff)) {
        return Err(Fail::Handoff);
    }
    let edits = inputs::edits_of(&walk)?;
    let mut say = Say {
        out: sink,
        lines: Vec::new(),
    };
    let ran = (|| -> Res<()> {
        say.say(format!(
            "== apply {lv}: {} edit(s) from {} -> {}",
            tune.ops.len(),
            os::relpath(path, &r.root)?,
            os::relpath(&out, &r.root)?
        ))?;
        let mut g = gathered?;
        for l in &g.lines {
            say.say(l.clone())?;
        }
        g.inputs.edits = (!walk.is_empty()).then_some(edits);
        let mut kept = Vec::new();
        let built = inputs::run_build(&g.inputs, &out, run, say.out, Some(&mut kept));
        say.lines.extend(kept);
        built?;
        say.say(format!(
            "  built in {} s; {} walkability edit(s) applied",
            pyfixed0(t0.elapsed().as_secs_f64()),
            walk.len()
        ))?;
        let data = std::fs::read(&out).map_err(|e| os_error(&out, &e))?;
        let planned = match plan(&data, tune, &os::basename(&out)?, graph, later > 0) {
            Ok(Ok(p)) => p,
            Ok(Err(e)) => return Err(raised(e)),
            Err(_) => return Err(navkit_error(REFUSED)),
        };
        finish(r, lv, &out, planned, &mut say, (later, notes, t0))
    })();
    match ran {
        // the NavKit branch hands a refusal off wherever it comes (the command line's answer since round NK5)
        Err(Fail::Handoff) => Err(Fail::Handoff),
        ran => finish_report(ran, &say, &out).map(|()| out),
    }
}

/// `f'{x:.0f}'`.
fn pyfixed0(x: f64) -> String {
    format!("{x:.0}")
}

/// `shutil._samefile(src, dst)`: the same file (when the target exists).
fn same_file(a: &str, b: &str) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}
