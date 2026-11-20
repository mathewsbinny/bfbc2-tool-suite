//! `bfbc2 hkpack classes|dump <file> ...`: NavKit's `bfbc2-auto-navkit\tools\hkpack.py` command line over
//! `bc2_formats::havok::Packfile` (the sections, fixups, objects and class descriptions), with the reflection walk
//! of `Packfile.read_object` / `read_value` (`bc2_formats::havok::reflect`, moved there in round F4 for the Python
//! module's handle) and `short()`.
//!
//! Where the core refuses the file, or the walk would raise in Python (a read past the data, a C string without an
//! end, a parent class that is not described, a C array of a type without a size, nesting deeper than Python's
//! recursion allows), the command line is handed to the Python.

#![allow(
    clippy::indexing_slicing,
    reason = "the arguments after their count is checked; the objects' head within its length"
)]

use bc2_formats::havok::Packfile;
use bc2_formats::havok::reflect::{Index, Reader, Value};

use super::common::{arg, head_len, py_int};
use crate::pyfmt::{docstring, float_g_prec};
use crate::pyproc::{Fail, Py, PyResult, read_file};

const SOURCE: &str = include_str!("../../../../bfbc2-auto-navkit/tools/hkpack.py");

/// `short(v)`.
fn short(v: &Value) -> String {
    match v {
        Value::Dict(items) => {
            let parts: Vec<String> = items.iter().map(|(k, x)| format!("{k}: {}", short(x))).collect();
            format!("{{{}}}", parts.join(", "))
        }
        Value::List(items) if items.len() > 6 => {
            let parts: Vec<String> = items.iter().take(3).map(short).collect();
            format!("[{} items: {}, ...]", items.len(), parts.join(", "))
        }
        Value::List(items) => {
            let parts: Vec<String> = items.iter().map(short).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Ptr(target, name) => match name.as_deref() {
            Some(n) if !n.is_empty() => format!("->{n}"),
            _ => format!("->{target:#x}"),
        },
        Value::Floats(xs) => {
            let parts: Vec<String> = xs.iter().map(|x| format!("{x:.3}")).collect();
            format!("({})", parts.join(", "))
        }
        Value::Unknown(n) => format!("(?, {n})"),
        Value::Float(x) => float_g_prec(*x, 4),
        Value::Int(i) => i.to_string(),
        Value::Bool(b) => if *b { "True" } else { "False" }.to_owned(),
        Value::None => "None".to_owned(),
        Value::Str(s) => s.clone(),
    }
}

pub fn run(py: &mut Py, args: &[String]) -> PyResult<i32> {
    if args.len() < 2 {
        py.print(&docstring(SOURCE))?;
        return Ok(1);
    }
    let data = read_file(&args[1])?;
    let pf = Packfile::parse(&data).map_err(|_| Fail::Python)?;
    let ix = Index::new(&pf);
    let r = Reader::new(&data, &ix);
    match args[0].as_str() {
        "classes" => {
            py.print(&format!(
                "Havok packfile at 0x{:X}, version {}, {}, {} sections, {} class descriptions",
                pf.base,
                pf.version,
                pf.contents_version,
                pf.sections.len(),
                ix.classes().len()
            ))?;
            // Counter(...).most_common(): by count, ties in the order first seen
            let mut counts: Vec<(&str, usize)> = Vec::new();
            for (_, c) in pf.objects() {
                if let Some(e) = counts.iter_mut().find(|e| e.0 == c) {
                    e.1 += 1;
                } else {
                    counts.push((c, 1));
                }
            }
            counts.sort_by_key(|e| std::cmp::Reverse(e.1));
            for (c, n) in counts {
                py.print(&format!("{n:7}  {c}"))?;
            }
        }
        "dump" => {
            let cname = arg(args, 2)?;
            let n = match args.get(3) {
                Some(s) => py_int(s)?,
                None => 2,
            };
            let offs: Vec<u64> = pf.objects_of(cname).collect();
            for &off in &offs[..head_len(offs.len(), n)] {
                let v = r.read_object(i128::from(off), Some(cname)).ok_or(Fail::Python)?;
                py.write(&format!("@0x{off:X}"))?;
                py.write(" ")?;
                py.write(&short(&v))?;
                py.write("\n")?;
            }
        }
        _ => {
            py.print(&docstring(SOURCE))?;
            return Ok(1);
        }
    }
    Ok(0)
}
