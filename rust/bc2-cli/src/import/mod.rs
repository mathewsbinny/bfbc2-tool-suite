//! Round O5's `import` group: `bfbc2 bf1943import list|import|check ...`, `bfbc2-bf1943-import\bf1943import.py`'s
//! command line. The help, the options, the source (`Source`: `crate::bf1943src`) and `list` answer here.
//! `import` and `check` are handed to the Python after the source opens: their `Importer` (the BFBC2 soldier
//! skeleton from the game copy's index, Granny trees compared as Python objects, the undeploys' added tracks) is
//! not ported.

use crate::bf1943src::{Source, default_source, import_error};
use crate::pyfmt::docstring;
use crate::pyproc::{Fail, Py, PyResult, Tool};

const SOURCE: &str = include_str!("../../../../bfbc2-bf1943-import/bf1943import.py");

/// The group's tools.
pub const TOOLS: &[Tool] = &[Tool {
    name: "bf1943import",
    script: "bfbc2-bf1943-import\\bf1943import.py",
    log: Some("bf1943import"),
    run,
}];

/// `NOT_YET`
fn not_yet(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "MeshData" => "mesh data: needs PC vertex layouts from a shader database",
        "RigidMeshSet" | "SkinnedMeshSet" | "CompositeMeshSet" => "mesh set: needs its meshes converted first",
        "HavokPhysicsData" => "physics: BF1943's Havok differs from BFBC2's",
        "AnimTreeInfo" => "animation tree: format not decoded",
        "<non-resource>" => "DBX partition: another game's data",
        _ => return None,
    })
}

/// `plan(kind)`.
pub fn plan(kind: &str) -> String {
    match kind {
        "GrannyAnimation" | "DeltaAnimation" => {
            "animation to PC (bones renumbered onto BFBC2's soldier when it animates the soldier)".to_owned()
        }
        "GrannyModel" => "skeleton to PC".to_owned(),
        "AimAnimation" => "aim animation to PC (bones renumbered onto BFBC2's soldier)".to_owned(),
        "XenonTexture" => "texture to PC".to_owned(),
        _ => format!(
            "not converted: {}",
            not_yet(kind).map_or_else(|| format!("{kind}: no converter"), str::to_owned)
        ),
    }
}

/// Python's `f'{s:20s}'`: padded with spaces to 20 characters.
fn pad(s: &str, n: usize) -> String {
    let k = s.chars().count();
    if k >= n {
        s.to_owned()
    } else {
        format!("{s}{}", " ".repeat(n - k))
    }
}

pub fn run(py: &mut Py, argv: &[String]) -> PyResult<i32> {
    if argv.iter().any(|a| a == "-h" || a == "--help" || a == "/?") {
        py.print(&docstring(SOURCE))?;
        return Ok(0);
    }
    let mut positional: Vec<&str> = Vec::new();
    let mut opts: Vec<(&str, &str)> = Vec::new();
    let mut it = argv.iter();
    while let Some(a) = it.next() {
        if a.starts_with("--") {
            let Some(v) = it.next() else {
                return Err(import_error(format!("{a} needs a value")));
            };
            opts.retain(|(k, _)| *k != a.as_str());
            opts.push((a.as_str(), v.as_str()));
        } else {
            positional.push(a.as_str());
        }
    }
    let opt = |k: &str| opts.iter().find(|(n, _)| *n == k).map(|(_, v)| *v);
    let Some(&cmd) = positional
        .first()
        .filter(|c| matches!(**c, "list" | "import" | "check"))
    else {
        py.print(&docstring(SOURCE))?;
        return Ok(i32::from(!positional.is_empty()));
    };
    let suite = crate::pyproc::suite_root().to_string_lossy().into_owned();
    let folder = opt("--source")
        .filter(|s| !s.is_empty())
        .map_or_else(|| default_source(&suite), str::to_owned);
    let source = Source::open(&folder, &suite)?;
    if cmd != "list" {
        return Err(Fail::Python);
    }
    let filters: Vec<String> = positional.iter().skip(1).map(|s| (*s).to_owned()).collect();
    if filters.iter().any(|f| f.chars().any(|c| u32::from(c) > 0xFF)) {
        return Err(Fail::Python); // str.lower() beyond Latin-1
    }
    for k in source.find(&filters) {
        let Some((rel, e)) = source.entry(&k) else { continue };
        py.print(&format!(
            "{:10}  {}  {}  [{rel}]  -> {}",
            e.size,
            pad(&e.kind, 20),
            e.name,
            plan(&e.kind)
        ))?;
    }
    Ok(0)
}
