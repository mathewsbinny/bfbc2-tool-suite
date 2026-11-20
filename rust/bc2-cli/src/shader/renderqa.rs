//! `bfbc2 renderqa render|compare|validate|wake|strips|patches ...`: `bfbc2-shader-tools\renderqa.py`'s command line up to
//! its drawing - the `argparse` parser, the game copy looked for, the staged folders checked, the output folder made,
//! the mesh list read, and the errors the Python raises before it draws (`RenderQaError`: no game copy for validate /
//! wake, a staged folder that is not there, no `--db`, no mesh named).
//!
//! Every run that draws is handed to the Python: the pictures are Direct3D 11 draws (`shaderview.exe` on the GPU, or
//! WARP), measured with NumPy and laid out as contact sheets with Pillow; byte-identical PNG sheets and the GPU's own
//! pixels are the Python's (round R5's `bc2-render` draws the same programs in the editor, not these reports).

use super::argp::{self, Kind, Nargs, Opt, Parsed, Parser, Pos, Ty};
use super::shaderdb;
use crate::pyos;
use crate::pyproc::{Fail, Py, PyErr, PyResult, os_error};

const USAGE: &str = "usage: renderqa.py [-h] [--db DB] [--truth TRUTH] [--stage STAGE]
                   [--list LIST] [--find] [--only ONLY] [--terrain TERRAIN]
                   [--height HEIGHT] [--limit LIMIT] [--size SIZE]
                   [--weather WEATHER] [--sun {noon,weather}]
                   [--envmap ENVMAP] [--warp] [--opaque] [--together]
                   [--coverage] [--opfor OPFOR] [-o OUT]
                   {render,compare,validate,wake,strips,patches} [meshes ...]
";

const HELP_BODY: &str = "
renderqa.py - draw meshes offline with the game's own PC shader programs,
textures and vertex data (Direct3D 11,

positional arguments:
  {render,compare,validate,wake,strips,patches}
  meshes                mesh names (or with --find, parts of names)

options:
  -h, --help            show this help message and exit
  --db DB               the database to draw with (render, compare)
  --truth TRUTH         compare: DICE's database (default auto: a BFBC2 level
                        that draws the mesh)
  --stage STAGE         unpacked bundles read before the game copy
  --list LIST           a file of mesh names, one per line
  --find                the mesh arguments are parts of names
  --only ONLY           wake: only meshes whose name contains this; strips:
                        only strips whose shader does; patches: only patch
                        entries whose name does (repeatable)
  --terrain TERRAIN     strips: the VisualTerrain (a file or a resource name;
                        default for level:<name>
                        terrains/<name>/<name>.visual.res)
  --height HEIGHT       patches: the drawn patch's world height in metres
                        (default 50.5; sea-floor layers shade by height)
  --limit LIMIT         at most this many meshes
  --size SIZE
  --weather WEATHER     weather folder of the light (default weather/r_mp005)
  --sun {noon,weather}  key light direction: a fixed high sun (default) or the
                        weather state's theta / phi
  --envmap ENVMAP       the cube the engine binds as
                        outdoorLightReflectionEnvmap
  --warp                the software rasterizer instead of the GPU
  --opaque              draw every part opaque with no culling, in the mesh's
                        order (as before render states)
  --together            render: draw the meshes named into one picture (a
                        building and its material-mask mesh)
  --coverage            render / compare / wake: also write
                        <mesh>.coverage.png - per view the kind of draw last
                        to cover each pixel (opaque, alpha-tested, blended)
                        and the render target's alpha
  --opfor OPFOR         Project OPFOR BFBC2 folder (the game copy)
  -o, --out OUT         output folder
";

const fn o(strings: &'static [&'static str], kind: Kind, ty: Ty) -> Opt {
    Opt {
        strings,
        kind,
        ty,
        choices: &[],
        required: false,
    }
}

const OPTS: &[Opt] = &[
    o(&["--db"], Kind::Store, Ty::Str),
    o(&["--truth"], Kind::Store, Ty::Str),
    o(&["--stage"], Kind::Append, Ty::Str),
    o(&["--list"], Kind::Store, Ty::Str),
    o(&["--find"], Kind::Flag, Ty::Str),
    o(&["--only"], Kind::Append, Ty::Str),
    o(&["--terrain"], Kind::Store, Ty::Str),
    o(&["--height"], Kind::Store, Ty::Float),
    o(&["--limit"], Kind::Store, Ty::Int),
    o(&["--size"], Kind::Store, Ty::Int),
    o(&["--weather"], Kind::Store, Ty::Str),
    Opt {
        strings: &["--sun"],
        kind: Kind::Store,
        ty: Ty::Str,
        choices: &["noon", "weather"],
        required: false,
    },
    o(&["--envmap"], Kind::Store, Ty::Str),
    o(&["--warp"], Kind::Flag, Ty::Str),
    o(&["--opaque"], Kind::Flag, Ty::Str),
    o(&["--together"], Kind::Flag, Ty::Str),
    o(&["--coverage"], Kind::Flag, Ty::Str),
    o(&["--opfor"], Kind::Store, Ty::Str),
    o(&["-o", "--out"], Kind::Store, Ty::Str),
];

const COMMANDS: &[&str] = &["render", "compare", "validate", "wake", "strips", "patches"];

fn rqa_error(msg: impl Into<String>) -> Fail {
    let mut e = PyErr::new("RenderQaError", msg);
    e.what_to_do = Some("Check the names and paths given (python renderqa.py -h).".into());
    Fail::Err(e)
}

/// `tempfile.gettempdir()`: `TMPDIR`, `TEMP`, `TMP` (the first that is a folder), made absolute.
fn gettempdir() -> PyResult<String> {
    for v in ["TMPDIR", "TEMP", "TMP"] {
        if let Ok(d) = std::env::var(v)
            && !d.is_empty()
            && pyos::isdir(&d)
        {
            return pyos::abspath(&d);
        }
    }
    Err(Fail::Python)
}

/// `Files(root, stages)`'s checks: each staged folder there, its manifests readable (their lines are read only when
/// drawing).
fn check_stage(folder: &str) -> PyResult<()> {
    if !pyos::isdir(folder) {
        return Err(rqa_error(format!("--stage {folder}: no such folder")));
    }
    let man = pyos::join2(folder, "fbrb.manifest")?;
    let tops = if pyos::isfile(&man) {
        vec![folder.to_owned()]
    } else {
        let mut l = pyos::listdir(folder)?;
        l.sort();
        l.iter().map(|d| pyos::join2(folder, d)).collect::<PyResult<Vec<_>>>()?
    };
    for top in tops {
        let man = pyos::join2(&top, "fbrb.manifest")?;
        if pyos::isfile(&man) {
            let data = std::fs::read(&man).map_err(|_| Fail::Python)?;
            if std::str::from_utf8(&data).is_err() {
                return Err(Fail::Python); // the Python's UnicodeDecodeError
            }
        }
    }
    Ok(())
}

/// `renderqa.main(argv)` under `errors.setup('renderqa')` / `errors.guard()` (stdout line-buffered, `errors='replace'`).
pub fn run(py: &mut Py, argv: &[String]) -> PyResult<i32> {
    py.stdout_replace();
    let help: &'static str = Box::leak(format!("{USAGE}{HELP_BODY}").into_boxed_str());
    let p = Parser {
        prog: "renderqa.py",
        usage: USAGE,
        help,
        opts: OPTS,
        pos: &[
            Pos {
                name: "command",
                nargs: Nargs::One,
                choices: COMMANDS,
            },
            Pos {
                name: "meshes",
                nargs: Nargs::Star,
                choices: &[],
            },
        ],
        order: &[
            Ok(0),
            Ok(1),
            Err(0),
            Err(1),
            Err(2),
            Err(3),
            Err(4),
            Err(5),
            Err(6),
            Err(7),
            Err(8),
            Err(9),
            Err(10),
            Err(11),
            Err(12),
            Err(13),
            Err(14),
            Err(15),
            Err(16),
            Err(17),
            Err(18),
        ],
    };
    let a = match argp::parse(py, &p, argv, false, &mut Vec::new())? {
        Parsed::Args(a) => a,
        Parsed::Exit(c) => return Ok(c),
    };
    let a = match argp::finish(py, &p, a) {
        Parsed::Args(a) => a,
        Parsed::Exit(c) => return Ok(c),
    };
    let command = a.one("command").ok_or(Fail::Python)?.to_owned();
    let root = match shaderdb::find_opfor(a.one("--opfor")) {
        Ok(r) => Some(r),
        Err(Fail::Err(e)) if e.kind == "ShaderDbError" => None,
        Err(e) => return Err(e),
    };
    if root.is_none() && (command == "validate" || command == "wake") {
        return Err(rqa_error("Project OPFOR BFBC2 (the game copy) not found: give --opfor"));
    }
    for st in a.all("--stage") {
        check_stage(st)?;
    }
    let out = match a.one("--out") {
        Some(o) => o.to_owned(),
        None => pyos::join(&gettempdir()?, &["renderqa", &command])?,
    };
    if out.is_empty() {
        return Err(Fail::Python); // os.makedirs('') raises its own error
    }
    pyos::makedirs(&out).map_err(|_| Fail::Python)?;
    let mut meshes: Vec<String> = a.all("meshes").to_vec();
    if let Some(list) = a.one("--list") {
        let data = std::fs::read(list).map_err(|e| os_error(list, &e))?;
        let text = String::from_utf8(data).map_err(|_| Fail::Python)?;
        for x in text.split_inclusive(['\n', '\r']) {
            let s = bc2_pyrt::py::strip(x);
            if !s.is_empty() && !x.starts_with('#') {
                meshes.push(s.to_owned());
            }
        }
    }
    if a.flag("--find") || command == "wake" {
        return Err(Fail::Python); // the game copy's index, then drawing
    }
    if command == "patches" || command == "strips" {
        if a.one("--db").is_none() {
            return Err(rqa_error(format!("{command}: --db is needed")));
        }
        return Err(Fail::Python);
    }
    if let Some(l) = a.one("--limit") {
        let n: i64 = l.parse().map_err(|_| Fail::Python)?;
        if n != 0 {
            // meshes[:limit]: a negative limit counts from the end
            let len = i64::try_from(meshes.len()).map_err(|_| Fail::Python)?;
            let keep = if n >= 0 { n.min(len) } else { (len + n).max(0) };
            meshes.truncate(usize::try_from(keep).map_err(|_| Fail::Python)?);
        }
    }
    if meshes.is_empty() {
        return Err(rqa_error("no mesh named (give names, --list or --find)"));
    }
    if command == "validate" {
        return Err(Fail::Python);
    }
    if a.one("--db").is_none() {
        return Err(rqa_error(format!("{command}: --db is needed")));
    }
    Err(Fail::Python)
}
