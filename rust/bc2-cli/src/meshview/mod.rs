//! Round O3's `meshview` group: `bfbc2-mesh-viewer\meshview.py`'s command line. `bfbc2 meshview find <text>` runs
//! here on the index's cache ([`crate::gameindex`]); drawing meshes (the picture: NumPy, the texture decoder, PIL)
//! and building a missing index are the Python's, which the command line is handed to. Round GR3 retired the page for
//! a browser (the map editor's asset preview replaced it); the picture is what a mesh given draws now.

use std::sync::LazyLock;

use regex::Regex;

use crate::gameindex;
use crate::pyfmt::docstring;
use crate::pyproc::{Fail, Py, PyResult, Tool};

const DOC: &str = include_str!("../../../../bfbc2-mesh-viewer/meshview.py");

/// The group's tools.
pub const TOOLS: &[Tool] = &[Tool {
    name: "meshview",
    script: "bfbc2-mesh-viewer\\meshview.py",
    log: None,
    run,
}];

/// `re.sub(r'_lod\d+_data\.res$', '', n)` (`$`: the end, or before a final newline).
static LOD: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"(?s)_lod\d+_data\.res(\n?)\z").ok());

fn strip_lod(n: &str) -> String {
    let Some(c) = LOD.as_ref().and_then(|r| r.captures(n)) else {
        return n.to_owned();
    };
    let Some(m) = c.get(0) else { return n.to_owned() };
    let nl = c.get(1).map_or("", |g| g.as_str());
    format!("{}{nl}", n.get(..m.start()).unwrap_or(n))
}

/// `main(argv)` with the `__main__` guard's `except (ViewError, ShaderDbError, ConvertError)`.
fn run(py: &mut Py, argv: &[String]) -> PyResult<i32> {
    match main(py, argv) {
        Err(Fail::Err(e)) if e.kind == "shaderdb.ShaderDbError" => {
            py.eprint(&format!("error: {}", e.msg));
            Ok(1)
        }
        r => r,
    }
}

fn main(py: &mut Py, argv: &[String]) -> PyResult<i32> {
    let mut positional: Vec<&str> = Vec::new();
    let mut opts: Vec<(&str, Option<&str>)> = Vec::new();
    let mut it = argv.iter();
    while let Some(a) = it.next() {
        if a == "--png" {
            continue;
        }
        if a.starts_with("--") {
            let v = it.next().map(String::as_str);
            match opts.iter_mut().find(|(k, _)| *k == a) {
                Some(slot) => slot.1 = v,
                None => opts.push((a, v)),
            }
        } else {
            positional.push(a);
        }
    }
    let Some(&first) = positional.first() else {
        py.print(&docstring(DOC))?;
        return Ok(1);
    };
    if first != "find" {
        return Err(Fail::Python); // drawing a mesh's picture: NumPy, the texture decoder, PIL (the Python's)
    }
    let opt = |k: &str| opts.iter().find(|(n, _)| *n == k).map(|(_, v)| *v);
    if opt("--level").is_some() {
        py.eprint("note: --level is not needed (every level's shader database is indexed); ignored");
    }
    let root = gameindex::find_opfor(opt("--opfor").flatten())?;
    let ix = gameindex::load(py, &root)?;
    let text = positional.get(1..).unwrap_or(&[]).join(" ").to_lowercase();
    let mut hits: Vec<String> = ix
        .keys()
        .filter(|n| n.ends_with("_data.res") && n.contains(&text))
        .map(strip_lod)
        .collect();
    hits.sort();
    hits.dedup();
    let lines: Vec<String> = hits
        .iter()
        .map(|h| match ix.get(&format!("{h}_lod0_data.res")) {
            Some((_, name)) => {
                let n = name.chars().count();
                name.chars().take(n.saturating_sub(14)).collect()
            }
            None => h.clone(),
        })
        .collect();
    let out = lines.join("\n");
    if out.is_empty() {
        py.print(&format!("no mesh name contains \"{text}\""))?;
    } else {
        py.print(&out)?;
    }
    Ok(0)
}
