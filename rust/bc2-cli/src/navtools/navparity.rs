//! `bfbc2 navparity show <name> [<other>]`: NavKit's `tools\navparity.py`'s table of saved parity runs
//! (`<OPFOR>\work\navkit\parity\<name>.json`). `run` measures path data (NumPy and NavKit's measures in Python around
//! the Rust core): handed to the Python, as is a saved run whose values Python's formatting would treat in its own
//! way (a missing or non-numeric measure, a file that is not plain JSON).

#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::format_push_string,
    clippy::type_complexity,
    reason = "a port of Python: indexes after their lengths are checked, ints as Python turns them into floats, == as Python compares floats, the Python's names and its functions' shape kept"
)]

use super::pycommon::{find_opfor, fixed, number, py_str, rjust};
use crate::pyfmt::docstring;
use crate::pyjson::{self, Value};
use crate::pyos;
use crate::pyproc::{Fail, Py, PyErr, PyResult, read_file};

const SOURCE: &str = include_str!("../../../../bfbc2-auto-navkit/tools/navparity.py");

pub const LEVELS: [&str; 13] = [
    "sp_01", "sp_02", "sp_03", "sp_04", "sp_003_b", "sp_004_b", "sp_005", "sp_005_b", "sp_006", "sp_007", "sp_008",
    "sp_008_b", "sp_009",
];
pub const ONSLAUGHT: [&str; 2] = ["ons_mp_002", "ons_mp_004"];

/// (key, heading, decimals: None for `'{}'`)
const COLUMNS: [(&str, &str, Option<usize>); 23] = [
    ("ground", "ground %", Some(1)),
    ("extra_ground", "extra %", Some(1)),
    ("areas_same", "areas %", Some(1)),
    ("detailed_reference", "det.ref", None),
    ("detailed_candidate", "det.ours", None),
    ("portals", "portals %", Some(1)),
    ("nodes_same", "nodes %", Some(1)),
    ("edges_same", "edges %", Some(1)),
    ("nodes_paired", "n.pair %", Some(1)),
    ("edges_paired", "e.pair %", Some(1)),
    ("costs_paired", "c.pair %", Some(1)),
    ("islands_reference", "isl.ref", None),
    ("islands_candidate", "isl.ours", None),
    ("connectivity", "connect %", Some(1)),
    ("over_connection", "over %", Some(1)),
    ("runtime_connectivity", "runtime %", Some(1)),
    ("ai_coverage", "AI pos %", Some(1)),
    ("ai_pairwise", "pairwise %", Some(1)),
    ("ladder_ground", "ladder %", Some(1)),
    ("df_d_precision", "D prec %", Some(1)),
    ("df_d_recall", "D rec %", Some(1)),
    ("df_f_precision", "F prec %", Some(1)),
    ("df_f_recall", "F rec %", Some(1)),
];
const TOTALS: [&str; 19] = [
    "ground",
    "extra_ground",
    "areas_same",
    "portals",
    "nodes_same",
    "edges_same",
    "nodes_paired",
    "edges_paired",
    "costs_paired",
    "connectivity",
    "over_connection",
    "runtime_connectivity",
    "ai_coverage",
    "ai_pairwise",
    "ladder_ground",
    "df_d_precision",
    "df_d_recall",
    "df_f_precision",
    "df_f_recall",
];

/// `navparity.save_path(root, name)` (its folder made).
pub fn save_path(root: &str, name: &str) -> PyResult<String> {
    let d = pyos::join(root, &["work", "navkit", "parity"])?;
    pyos::makedirs(&d)?;
    pyos::join2(&d, &format!("{}.json", name.replace(' ', "_")))
}

/// A number as Python's arithmetic holds it: an int (exact) or a float.
#[derive(Clone, Copy, PartialEq)]
enum Num {
    I(i128),
    F(f64),
}

impl Num {
    fn of(v: &Value) -> PyResult<Num> {
        match v {
            Value::Int(i) => Ok(Num::I(*i)),
            Value::Float(f) => Ok(Num::F(*f)),
            _ => Err(Fail::Python),
        }
    }
    fn f(self) -> f64 {
        match self {
            Num::I(i) => i as f64,
            Num::F(f) => f,
        }
    }
    fn sub(self, o: Num) -> Num {
        match (self, o) {
            (Num::I(a), Num::I(b)) => Num::I(a - b),
            (a, b) => Num::F(a.f() - b.f()),
        }
    }
    fn pos(self) -> bool {
        match self {
            Num::I(i) => i > 0,
            Num::F(f) => f > 0.0,
        }
    }
    fn is_zero(self) -> bool {
        self.f() == 0.0
    }
}

/// `r.get(key, 0)`.
fn get_or0<'a>(r: &'a Value, key: &str) -> &'a Value {
    const ZERO: Value = Value::Int(0);
    r.get(key).unwrap_or(&ZERO)
}

/// `f.format(v)` of a column.
fn cell(v: &Value, f: Option<usize>) -> PyResult<String> {
    match f {
        Some(p) => Ok(fixed(number(v)?, p)),
        None => Ok(py_str(v)),
    }
}

fn num_str(n: Num, f: Option<usize>) -> String {
    match (n, f) {
        (Num::F(x), Some(p)) => fixed(x, p),
        (Num::F(x), None) => crate::pyfmt::float_repr(x),
        (Num::I(i), _) => i.to_string(),
    }
}

/// `{r['level']: r for r in run['levels']}`: the keys in first-seen order with their last row.
fn rows_of(run: &Value) -> PyResult<Vec<(String, &Value)>> {
    let mut out: Vec<(String, &Value)> = Vec::new();
    for r in run.get("levels").and_then(Value::as_list).ok_or(Fail::Python)? {
        let lv = r.get("level").and_then(Value::as_str).ok_or(Fail::Python)?.to_owned();
        if let Some(slot) = out.iter_mut().find(|(k, _)| *k == lv) {
            slot.1 = r;
        } else {
            out.push((lv, r));
        }
    }
    Ok(out)
}

/// `sum(r.get(k, 0) for r in rows) / len(rows)`.
fn mean<'a>(rows: impl Iterator<Item = &'a Value>, k: &str) -> PyResult<f64> {
    // sum() as CPython 3.12+ adds: ints exactly until the first float, then floats with Neumaier's compensation
    // (ints after that added plainly), the compensation added at the end
    let mut s = Num::I(0);
    let mut c = 0.0f64;
    let mut n = 0usize;
    for r in rows {
        if !matches!(r, Value::Dict(_)) {
            return Err(Fail::Python);
        }
        let x = Num::of(get_or0(r, k))?;
        s = match (s, x) {
            (Num::I(a), Num::I(b)) => Num::I(a + b),
            (Num::I(a), Num::F(b)) => Num::F(a as f64 + b),
            (Num::F(f), Num::I(b)) => Num::F(f + b as f64),
            (Num::F(f), Num::F(b)) => {
                let t = f + b;
                if f.abs() >= b.abs() {
                    c += (f - t) + b;
                } else {
                    c += (b - t) + f;
                }
                Num::F(t)
            }
        };
        n += 1;
    }
    if let Num::F(f) = s
        && c != 0.0
        && c.is_finite()
    {
        s = Num::F(f + c);
    }
    if n == 0 {
        return Err(PyErr::new(
            "ZeroDivisionError",
            if matches!(s, Num::I(_)) {
                "division by zero"
            } else {
                "float division by zero"
            },
        )
        .into());
    }
    match s {
        Num::I(i) if i.unsigned_abs() > (1u128 << 53) => Err(Fail::Python),
        _ => Ok(s.f() / n as f64),
    }
}

fn table(py: &mut Py, run: &Value, other: Option<&Value>) -> PyResult<()> {
    let rows = rows_of(run)?;
    let base = match other {
        Some(o) => rows_of(o)?,
        None => Vec::new(),
    };
    let head = format!(
        "{} {}",
        super::pycommon::ljust("level", 9),
        COLUMNS
            .iter()
            .map(|(_, h, _)| rjust(h, 10))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let name = |r: &Value, k: &str| -> PyResult<String> { r.get(k).map(py_str).ok_or(Fail::Python) };
    let mut first = format!("run '{}' ({})", name(run, "name")?, name(run, "date")?);
    if let Some(o) = other {
        first.push_str(&format!(
            " against '{}' ({}): value (change)",
            name(o, "name")?,
            name(o, "date")?
        ));
    }
    py.print(&first)?;
    py.print(&head)?;
    let mut order: Vec<&str> = LEVELS
        .iter()
        .copied()
        .filter(|l| rows.iter().any(|(k, _)| k == l))
        .collect();
    let mut rest: Vec<&str> = rows
        .iter()
        .map(|(k, _)| k.as_str())
        .filter(|k| !LEVELS.contains(k))
        .collect();
    rest.sort_unstable();
    order.extend(rest);
    for lv in order {
        let r = rows
            .iter()
            .find(|(k, _)| k == lv)
            .map(|(_, r)| *r)
            .ok_or(Fail::Python)?;
        let b = base.iter().find(|(k, _)| k == lv).map(|(_, r)| *r);
        let mut cells = Vec::new();
        for (key, _, f) in COLUMNS {
            if key == "portals" && Num::of(r.get("areas_same").ok_or(Fail::Python)?)?.is_zero() {
                cells.push(rjust("n/a", 10));
                continue;
            }
            let val = get_or0(r, key);
            let mut v = cell(val, f)?;
            if let Some(b) = b {
                if let (Value::Int(_) | Value::Float(_), Value::Dict(_)) = (val, b) {
                    let x = Num::of(val)?;
                    let bv = get_or0(b, key);
                    let differs = match bv {
                        Value::Int(_) | Value::Float(_) => {
                            let y = Num::of(bv)?;
                            match (x, y) {
                                (Num::I(p), Num::I(q)) => p != q,
                                _ => x.f() != y.f(),
                            }
                        }
                        _ => return Err(Fail::Python),
                    };
                    if differs {
                        let d = x.sub(Num::of(bv)?);
                        v.push_str(&format!("({}{})", if d.pos() { "+" } else { "" }, num_str(d, f)));
                    }
                } else if matches!(val, Value::Bool(_)) || !matches!(b, Value::Dict(_)) {
                    return Err(Fail::Python);
                }
            }
            cells.push(rjust(&v, 10));
        }
        py.print(&format!("{} {}", super::pycommon::ljust(lv, 9), cells.join(" ")))?;
    }
    let mut avg: Vec<(&str, f64)> = Vec::new();
    for k in TOTALS {
        avg.push((k, mean(rows.iter().map(|(_, r)| *r), k)?));
    }
    let mut all_zero = true;
    for (_, r) in &rows {
        if !Num::of(r.get("areas_same").ok_or(Fail::Python)?)?.is_zero() {
            all_zero = false;
        }
    }
    if all_zero {
        avg.retain(|(k, _)| *k != "portals");
    }
    let avg_of = |k: &str| avg.iter().find(|(a, _)| *a == k).map(|(_, v)| *v);
    let line: Vec<String> = COLUMNS
        .iter()
        .map(|(k, _, f)| rjust(&avg_of(k).map_or(String::new(), |v| fixed(v, f.unwrap_or(1))), 10))
        .collect();
    py.print(&format!("{} {}", super::pycommon::ljust("mean", 9), line.join(" ")))?;
    if let Some(o) = other {
        let list = o.get("levels").and_then(Value::as_list).ok_or(Fail::Python)?;
        let mut bavg: Vec<(&str, f64)> = Vec::new();
        for k in TOTALS {
            bavg.push((k, mean(list.iter(), k)?));
        }
        let line: Vec<String> = COLUMNS
            .iter()
            .map(|(k, _, f)| {
                let s = match (avg_of(k), bavg.iter().find(|(a, _)| a == k).map(|(_, v)| *v)) {
                    (Some(a), Some(b)) => {
                        format!("{}{}", if a - b > 0.0 { "+" } else { "" }, fixed(a - b, f.unwrap_or(1)))
                    }
                    _ => String::new(),
                };
                rjust(&s, 10)
            })
            .collect();
        py.print(&format!("{} {}", super::pycommon::ljust("change", 9), line.join(" ")))?;
    }
    Ok(())
}

pub fn run(py: &mut Py, argv: &[String]) -> PyResult<i32> {
    let mut args: Vec<String> = argv.to_vec();
    let mut opfor = None;
    if let Some(i) = args.iter().position(|a| a == "--opfor") {
        opfor = Some(
            args.get(i + 1)
                .cloned()
                .ok_or_else(|| Fail::Err(PyErr::new("IndexError", "list index out of range")))?,
        );
        args.drain(i..i + 2);
    }
    if args.len() < 2 || !(args[0] == "run" || args[0] == "show") {
        py.print(&docstring(SOURCE))?;
        return Ok(1);
    }
    let root = find_opfor(opfor.as_deref(), "NavKitError")?;
    if args[0] != "show" {
        return Err(Fail::Python);
    }
    let mut runs = Vec::new();
    for n in args.iter().skip(1).take(2) {
        let p = save_path(&root, n)?;
        let data = read_file(&p)?;
        runs.push(pyjson::loads_ansi(&data).ok_or(Fail::Python)?);
    }
    if runs.len() == 1 {
        table(py, &runs[0], None)?;
    } else {
        table(py, &runs[1], Some(&runs[0]))?;
    }
    Ok(0)
}
