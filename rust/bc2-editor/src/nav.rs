//! The AI workspace's navigation state (round GR2: the Navigation Studio folded into the editor): the level's path
//! data read on a worker thread ([`NavState::start`]), the layers and colourings ([`crate::navview`]) drawn over the
//! view, the path test, the anomaly overlay, a reference level's AI layer (compare), and the navigation edits with
//! their side file, build, install and restore.
//!
//! **Edits** are operations of the map project's log ([`bc2_levelwrite::navedit`]): they undo, redo and save with
//! the project. The side file (`work\navkit\<level>\<level>.navtune.json`, `navtune.py`'s format, written by the
//! Rust core's [`bc2_navbuild::navtune::dumps`]) is written from the log when the edits are saved or built.
//!
//! **Build** runs NavKit's driver in process ([`start_apply`], round NK6: `bc2_navbuild::apply`, the code
//! `bfbc2.exe navkit apply` runs, on a worker thread with its lines streamed and a cancel flag). **Install, restore**
//! run `bfbc2.exe navkit install / restore` ([`navkit_command`]; `python navkit.py` without the binary). Install and restore write only the level's AI path data in the game copy
//! (`game\Output\win32\ai\levels\<lv>\infantry`): the editor shows what they will write first ([`install_plan`], [`restore_plan`]) and runs them only when that is confirmed.

use std::collections::{BTreeSet, HashMap};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use bc2_formats::terrain::Heightfield;
use bc2_gamefs::ledger::{self as gameledger, Ledger};
use bc2_levelwrite::navedit::{NavBase, NavEdit, NavRegion};
use bc2_navbuild::astar::Graph;
use bc2_navbuild::navtune::{self, Value};

use crate::error::EditorError;
use crate::navdata::{self, AiLayer, Anomalies, Difference, NavData, NavIndex, Route};
use crate::navview::ColourMode;
use crate::task::Task;

/// The two files, by their names in the panels.
pub const FILES: [&str; 2] = ["NavKit", "DICE"];

/// One path-data file read.
#[derive(Debug)]
pub struct NavFile {
    /// `NavKit` or `DICE`.
    pub name: &'static str,
    /// Its data.
    pub data: NavData,
    /// Its route graph for the path test.
    pub graph: Graph,
    /// Per soldier spawn (the level's, in scene order): its island and the distance to its route node.
    pub spawns: Vec<(Option<u32>, Option<f64>)>,
    gaps: OnceLock<Vec<f64>>,
    cells: OnceLock<HashMap<(i64, i64), Vec<usize>>>,
}

impl NavFile {
    /// Its islands' distances from the main network (the first 300), made once.
    pub fn gaps(&self) -> &[f64] {
        self.gaps.get_or_init(|| navdata::island_gaps(&self.data, 300))
    }

    /// The cell at a point (the same 0.8 m cell, the floor nearest in height within 2.5 m), and its area.
    pub fn cell_at(&self, p: [f64; 3]) -> Option<(usize, usize)> {
        let grid = self.cells.get_or_init(|| {
            let mut g: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
            for (k, c) in self.data.cell_ij.iter().enumerate() {
                g.entry((c[0], c[1])).or_default().push(k);
            }
            g
        });
        let key = (
            (p[0] / navdata::GRID).round() as i64,
            (p[2] / navdata::GRID).round() as i64,
        );
        let k = grid
            .get(&key)?
            .iter()
            .copied()
            .map(|k| (k, (self.data.cell_y.get(k).copied().unwrap_or(0.0) - p[1]).abs()))
            .filter(|(_, d)| *d <= 2.5)
            .min_by(|a, b| a.1.total_cmp(&b.1))?
            .0;
        Some((k, self.data.cell_area.get(k).copied()? as usize))
    }
}

/// What the worker read.
#[derive(Debug)]
pub struct Loaded {
    /// The files (NavKit's first when there is one).
    pub files: Vec<NavFile>,
    /// Each file's cells that the other has too (NavKit's, DICE's), with both files.
    pub same: Option<(Vec<bool>, Vec<bool>)>,
    /// The difference figures, with both files.
    pub diff: Option<Difference>,
    /// The level's own AI layer (its partition).
    pub layer: AiLayer,
    /// NavKit's anomalies against it.
    pub anomalies: Option<Anomalies>,
    /// Seconds it took.
    pub seconds: f64,
}

impl Loaded {
    /// A file by name.
    #[must_use]
    pub fn file(&self, name: &str) -> Option<&NavFile> {
        self.files.iter().find(|f| f.name == name)
    }

    /// The cells of a file the other has too.
    #[must_use]
    pub fn same_of(&self, name: &str) -> Option<&[bool]> {
        self.same
            .as_ref()
            .map(|(c, r)| if name == "NavKit" { c.as_slice() } else { r.as_slice() })
    }
}

/// What to read: the level, its files and the scene's parts the reading needs.
#[derive(Debug, Clone)]
pub struct Inputs {
    /// NavKit's build, if any.
    pub navkit: Option<PathBuf>,
    /// DICE's file, if any.
    pub dice: Option<PathBuf>,
    /// The terrain.
    pub heightfield: Option<Heightfield>,
    /// The level partition (its AI layer).
    pub partition: Option<Vec<u8>>,
    /// The soldier spawns' positions.
    pub spawns: Vec<[f64; 3]>,
}

/// Reads the files (the worker's job).
///
/// # Errors
/// A file does not read.
pub fn read(inputs: &Inputs) -> Result<Loaded, EditorError> {
    let t0 = Instant::now();
    let hf = inputs.heightfield.as_ref();
    let mut files = Vec::new();
    for (name, p) in [("NavKit", &inputs.navkit), ("DICE", &inputs.dice)] {
        if let Some(p) = p {
            let data = navdata::load(p, hf)?;
            let graph = navdata::graph(&data);
            let spawns = inputs.spawns.iter().map(|s| navdata::spawn_island(&data, *s)).collect();
            files.push(NavFile {
                name,
                data,
                graph,
                spawns,
                gaps: OnceLock::new(),
                cells: OnceLock::new(),
            });
        }
    }
    let (same, diff) = match (files.first(), files.get(1)) {
        (Some(a), Some(b)) => {
            let (c, r) = navdata::compare_cells(&a.data, &b.data);
            let d = navdata::difference(&a.data, &c, &r);
            (Some((c, r)), Some(d))
        }
        _ => (None, None),
    };
    let layer = inputs
        .partition
        .as_deref()
        .and_then(|p| navdata::read_ai_layer(p).ok())
        .unwrap_or_default();
    let anomalies = files
        .iter()
        .find(|f| f.name == "NavKit")
        .map(|f| navdata::anomalies(&f.data, &layer));
    Ok(Loaded {
        files,
        same,
        diff,
        layer,
        anomalies,
        seconds: t0.elapsed().as_secs_f64(),
    })
}

/// The path test's state.
#[derive(Debug, Clone, Default)]
pub struct PathTest {
    /// Clicks add the start, then the end.
    pub active: bool,
    /// The start and the end.
    pub points: Vec<[f64; 3]>,
    /// The route over each file (by name).
    pub routes: Vec<(&'static str, Route)>,
}

/// A navigation edit tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NavTool {
    /// A walkable region.
    Walkable,
    /// An unwalkable region.
    Unwalkable,
    /// A seed point.
    Seed,
    /// An unseed point.
    Unseed,
    /// Two areas linked.
    LinkAreas,
    /// A route edge added.
    EdgeAdd,
    /// A route edge removed.
    EdgeRemove,
    /// A traversal (ladder, jump, vault, drop): the bottom, then the top.
    Traversal,
    /// A note.
    Note,
}

impl NavTool {
    /// Every tool, in the panel's order.
    pub const ALL: [Self; 9] = [
        Self::Walkable,
        Self::Unwalkable,
        Self::Seed,
        Self::Unseed,
        Self::LinkAreas,
        Self::EdgeAdd,
        Self::EdgeRemove,
        Self::Traversal,
        Self::Note,
    ];

    /// Its name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Walkable => "Walkable region",
            Self::Unwalkable => "Unwalkable region",
            Self::Seed => "Seed",
            Self::Unseed => "Unseed",
            Self::LinkAreas => "Link two areas",
            Self::EdgeAdd => "Add a route edge",
            Self::EdgeRemove => "Remove a route edge",
            Self::Traversal => "Traversal",
            Self::Note => "Note",
        }
    }

    /// The side file's operation.
    #[must_use]
    pub fn op(self) -> &'static str {
        match self {
            Self::Walkable => "walkable",
            Self::Unwalkable => "unwalkable",
            Self::Seed => "seed",
            Self::Unseed => "unseed",
            Self::LinkAreas => "link-areas",
            Self::EdgeAdd | Self::EdgeRemove => "route-edge",
            Self::Traversal => "traversal",
            Self::Note => "note",
        }
    }

    /// Whether it needs a NavKit base (a rebuild).
    #[must_use]
    pub fn region(self) -> bool {
        navtune::REGION_OPS.contains(&self.op())
    }

    /// What to click, in words.
    #[must_use]
    pub fn how(self, polygon: bool) -> &'static str {
        match self {
            Self::Walkable | Self::Unwalkable if polygon => {
                "Click the outline's corners on the ground; Enter finishes it"
            }
            Self::Walkable | Self::Unwalkable => "Click two opposite corners of the box",
            Self::Seed | Self::Unseed => "Click a point on the ground",
            Self::Note => "Type the note, then click where it belongs",
            Self::Traversal => "Click the bottom, then the top",
            Self::LinkAreas | Self::EdgeAdd | Self::EdgeRemove => {
                "Click the first point, then the second; each joins the route node nearest it"
            }
        }
    }
}

/// How a job runs: a `navkit` child (install, restore), or NavKit's driver in process on a [`Task`] (Build with these
/// edits, round NK6) with its cancel flag.
#[derive(Debug)]
enum Runner {
    Child(Child),
    InProcess { task: Task<()>, cancel: Arc<AtomicBool> },
}

/// A running `navkit` command or in-process build: its output lines as they come.
#[derive(Debug)]
pub struct Job {
    runner: Runner,
    /// What it does (`apply`, `install`, `restore`).
    pub what: String,
    /// Its output so far.
    pub lines: Arc<Mutex<Vec<String>>>,
    /// How it ended (`Some(true)`: exit code 0 / the build done; `None`: running). Round OP1: read through
    /// [`Job::running`] and [`Job::outcome`].
    done: Option<bool>,
    /// When it started.
    pub started: Instant,
    /// The file it writes, removed when the job is dropped before it ended (round DF1, audit AU4-CON-2): a build
    /// cut short leaves no half-written file for a later Install to copy.
    pub partial: Option<PathBuf>,
}

impl Drop for Job {
    /// A command still running when its job goes (the editor closed under it) is stopped, never left orphaned; an
    /// in-process build is cancelled (it stops at its next step and removes its partial output itself).
    fn drop(&mut self) {
        if self.done.is_some() {
            return;
        }
        match &mut self.runner {
            Runner::Child(child) => {
                if matches!(child.try_wait(), Ok(None)) {
                    let _ = child.kill();
                    let _ = child.wait();
                    if let Some(p) = &self.partial {
                        let _ = std::fs::remove_file(p);
                    }
                }
            }
            Runner::InProcess { cancel, .. } => cancel.store(true, Ordering::SeqCst),
        }
    }
}

impl Job {
    /// Whether the command still runs.
    #[must_use]
    pub fn running(&self) -> bool {
        self.done.is_none()
    }

    /// How it ended (`Some(true)`: exit code 0), `None` while it runs.
    #[must_use]
    pub fn outcome(&self) -> Option<bool> {
        self.done
    }

    /// Whether it runs in the editor's process (NavKit's driver on a worker thread).
    #[must_use]
    pub fn in_process(&self) -> bool {
        matches!(self.runner, Runner::InProcess { .. })
    }

    /// A job of a child already started (tests: a stand-in for a long navkit command).
    #[doc(hidden)]
    #[must_use]
    pub fn from_child(child: Child, what: &str) -> Self {
        Self {
            runner: Runner::Child(child),
            what: what.to_owned(),
            lines: Arc::new(Mutex::new(Vec::new())),
            done: None,
            started: Instant::now(),
            partial: None,
        }
    }

    /// Asks an in-process build to stop at its next step (a child is not asked: install and restore are quick).
    pub fn cancel(&self) {
        if let Runner::InProcess { cancel, .. } = &self.runner {
            cancel.store(true, Ordering::SeqCst);
        }
    }

    /// Whether it has ended (and how), polled.
    pub fn poll(&mut self) -> Option<bool> {
        if self.done.is_none() {
            match &mut self.runner {
                Runner::Child(child) => {
                    if let Ok(Some(status)) = child.try_wait() {
                        // the reader threads finish once the pipes close; give them a moment
                        std::thread::sleep(std::time::Duration::from_millis(30));
                        self.done = Some(status.success());
                    }
                }
                Runner::InProcess { task, .. } => {
                    if let Some(r) = task.poll() {
                        self.done = Some(r.is_ok());
                    }
                }
            }
        }
        self.done
    }

    /// Waits for the end (tests, and the quick install / restore).
    pub fn wait(&mut self) -> bool {
        let ok = match &mut self.runner {
            Runner::Child(child) => {
                let ok = child.wait().is_ok_and(|s| s.success());
                std::thread::sleep(std::time::Duration::from_millis(50));
                ok
            }
            Runner::InProcess { task, .. } => task.wait().is_ok(),
        };
        self.done = Some(ok);
        ok
    }

    /// The output so far.
    #[must_use]
    pub fn output(&self) -> Vec<String> {
        self.lines.lock().map_or_else(|_| Vec::new(), |l| l.clone())
    }
}

/// The in-process build's lines (each as `bfbc2.exe navkit` prints it) and its cancel flag.
struct Lines {
    lines: Arc<Mutex<Vec<String>>>,
    cancel: Arc<AtomicBool>,
}

impl bc2_navbuild::inputs::Out for Lines {
    fn line(&mut self, text: &str) -> bc2_navbuild::ntpath::Res<()> {
        if let Ok(mut l) = self.lines.lock() {
            l.push(text.to_owned());
        }
        Ok(())
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
}

/// Build with these edits in process (round NK6): `navkit apply <lv> --navtune <side> --out <out>` as
/// `bc2_navbuild::apply` runs it - the same lines (an error as the command line's `error:` line) and the same files
/// as the `bfbc2.exe navkit apply` child - on a [`Task`], its lines streamed, cancelled through [`Job::cancel`] (or
/// by dropping the job: the build stops at its next step and removes its partial output).
#[must_use]
pub fn start_apply(root: &Path, lv: &str, side: &Path, out: &Path) -> Job {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let cancel = Arc::new(AtomicBool::new(false));
    let (root, lv) = (root.display().to_string(), lv.to_lowercase());
    let (side, out_s) = (side.display().to_string(), out.display().to_string());
    let mut sink = Lines {
        lines: Arc::clone(&lines),
        cancel: Arc::clone(&cancel),
    };
    let task = Task::spawn("NavKit's build", move || {
        let ran = bc2_navbuild::inputs::Root::new(&root).and_then(|r| {
            let run = bc2_navbuild::inputs::Run {
                clock: bc2_navbuild::inputs::local_clock(),
                no_detail: false,
            };
            bc2_navbuild::apply::apply(
                &r,
                &lv,
                (Some(&side), Some(&out_s)),
                run,
                &mut sink,
                &bc2_navbuild::inputs::quiet_tool,
            )
        });
        match ran {
            Ok(_) => Ok(()),
            Err(e) => {
                if e == bc2_navbuild::ntpath::Fail::Cancelled {
                    let _ = std::fs::remove_file(&out_s);
                }
                let msg = match &e {
                    bc2_navbuild::ntpath::Fail::Raised(r) => r.msg.clone(),
                    e => e.to_string(),
                };
                let _ = bc2_navbuild::inputs::Out::line(&mut sink, &format!("error: {msg}"));
                Err(EditorError::Tool(format!("NavKit's build: {msg}")))
            }
        }
    });
    Job {
        runner: Runner::InProcess { task, cancel },
        what: "apply".to_owned(),
        lines,
        done: None,
        started: Instant::now(),
        partial: Some(out.to_path_buf()),
    }
}

/// The suite's folder (the checkout this editor was built from).
fn suite() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// NavKit's command line: `bin\bfbc2.exe navkit` when the binary is there (round O4: it hands the steps whose core is
/// still Python, such as `build` and `apply` on a NavKit base, to `navkit.py`), else `python navkit.py`
/// (`BFBC2_CLI_PYTHON=1`: always the Python).
#[must_use]
pub fn navkit_command() -> Vec<String> {
    let exe = suite().join("bin").join("bfbc2.exe");
    if exe.is_file() && std::env::var("BFBC2_CLI_PYTHON").as_deref() != Ok("1") {
        vec![exe.display().to_string(), "navkit".to_owned()]
    } else {
        vec![
            "python".to_owned(),
            suite()
                .join("bfbc2-auto-navkit")
                .join("navkit.py")
                .display()
                .to_string(),
        ]
    }
}

/// Starts a `navkit` command (`args` after `navkit`, `--opfor root` added), its output read as it comes.
///
/// # Errors
/// It does not start.
pub fn start_navkit(root: &Path, args: &[String], what: &str) -> Result<Job, EditorError> {
    let cmd = navkit_command();
    let (prog, first) = cmd.split_first().ok_or(EditorError::Tool("no NavKit command".into()))?;
    let mut c = Command::new(prog);
    c.args(first)
        .args(args)
        .arg("--opfor")
        .arg(root)
        .env("PYTHONUNBUFFERED", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let mut child = c.spawn().map_err(|e| EditorError::Tool(format!("{prog}: {e}")))?;
    let lines = Arc::new(Mutex::new(Vec::new()));
    for out in [
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    {
        let lines = Arc::clone(&lines);
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if let Ok(mut l) = lines.lock() {
                    l.push(line.trim_end().to_owned());
                }
            }
        });
    }
    Ok(Job {
        runner: Runner::Child(child),
        what: what.to_owned(),
        lines,
        done: None,
        started: Instant::now(),
        partial: None,
    })
}

/// The level's side file: `work\navkit\<level>\<level>.navtune.json` (`navtune.default_path`).
#[must_use]
pub fn side_file(root: &Path, lv: &str) -> PathBuf {
    let lv = lv.to_lowercase();
    root.join("work")
        .join("navkit")
        .join(&lv)
        .join(format!("{lv}.navtune.json"))
}

/// Where a build with the edits goes: NavKit's own file for a NavKit base, a copy of DICE's for a DICE base.
#[must_use]
pub fn build_out(root: &Path, lv: &str, kind: &str) -> PathBuf {
    let lv = lv.to_lowercase();
    let name = if kind == "dice" {
        "pathdata_dice_edited.hpd"
    } else {
        "pathdata.hpd"
    };
    root.join("work").join("navkit").join(lv).join(name)
}

fn f3(p: [f64; 3]) -> Value {
    Value::List(p.iter().map(|v| Value::Float(*v)).collect())
}

/// An edit as the side file's JSON value (its fields in `navtune.op`'s order).
#[must_use]
pub fn to_value(e: &NavEdit) -> Value {
    let mut d: Vec<(String, Value)> = vec![("op".into(), Value::Str(e.op.clone()))];
    let mut put = |k: &str, v: Option<Value>| {
        if let Some(v) = v {
            d.push((k.into(), v));
        }
    };
    put("kind", e.kind.clone().map(Value::Str));
    put(
        "region",
        e.region.as_ref().map(|r| {
            let mut rd = Vec::new();
            if let Some(poly) = &r.polygon {
                rd.push((
                    "polygon".to_owned(),
                    Value::List(
                        poly.iter()
                            .map(|p| Value::List(vec![Value::Float(p[0]), Value::Float(p[1])]))
                            .collect(),
                    ),
                ));
            }
            if let Some(y) = r.y {
                rd.push((
                    "y".to_owned(),
                    Value::List(vec![Value::Float(y[0]), Value::Float(y[1])]),
                ));
            }
            if let Some(b) = r.cube {
                rd.push((
                    "box".to_owned(),
                    Value::List(b.iter().map(|v| Value::Float(*v)).collect()),
                ));
            }
            Value::Dict(rd)
        }),
    );
    put("value", e.value.map(Value::Bool));
    put("at", e.at.map(f3));
    put("text", e.text.clone().map(Value::Str));
    put("a", e.a.map(f3));
    put("b", e.b.map(f3));
    put("action", e.action.clone().map(Value::Str));
    put("bottom", e.bottom.map(f3));
    put("top", e.top.map(f3));
    put("two-way", e.two_way.map(Value::Bool));
    d.push(("by".into(), Value::Str(e.by.clone())));
    d.push(("when".into(), Value::Str(e.when.clone())));
    Value::Dict(d)
}

/// The problems of an edit on a base of `kind` (`navtune.validate`; empty: fine).
#[must_use]
pub fn problems(e: &NavEdit, kind: &str) -> Vec<String> {
    navtune::validate(&to_value(e), kind).unwrap_or_default()
}

/// An edit's line in the list (`navtune.summary`).
#[must_use]
pub fn summary(e: &NavEdit) -> String {
    navtune::summary(&to_value(e)).unwrap_or_else(|| e.op.clone())
}

/// The side file's text for a list of edits (`NavTune.save_text`).
#[must_use]
pub fn side_text(level: &str, base: &NavBase, edits: &[NavEdit]) -> String {
    let v = Value::Dict(vec![
        ("format".into(), Value::Str("bfbc2-navtune".into())),
        ("version".into(), Value::Int(1)),
        ("level".into(), Value::Str(level.to_owned())),
        (
            "base".into(),
            Value::Dict(vec![
                ("kind".into(), Value::Str(base.kind.clone())),
                ("sha1".into(), Value::Str(base.sha1.clone())),
                ("file".into(), Value::Str(base.file.clone())),
            ]),
        ),
        ("operations".into(), Value::List(edits.iter().map(to_value).collect())),
    ]);
    navtune::dumps(&v)
}

/// Writes the side file (whole or not at all).
///
/// # Errors
/// It cannot be written.
pub fn write_side(path: &Path, text: &str) -> Result<(), EditorError> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| EditorError::Tool(format!("{}: {e}", d.display())))?;
    }
    bc2_gamefs::atomic::write(path, &[text.as_bytes()]).map_err(|e| {
        EditorError::Tool(match e {
            bc2_gamefs::atomic::WriteError::Temp(tmp, e) => format!("{}: {e}", tmp.display()),
            bc2_gamefs::atomic::WriteError::Replace(e) => format!("{}: {e}", path.display()),
        })
    })
}

/// Reads a side file: its base and edits (`navtune.load`: the format, the version, each edit's problems).
///
/// # Errors
/// It does not read, is not a side file, or an edit is not valid.
pub fn read_side(path: &Path) -> Result<(NavBase, Vec<NavEdit>), EditorError> {
    let text = std::fs::read_to_string(path).map_err(|e| EditorError::Tool(format!("{}: {e}", path.display())))?;
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| EditorError::Tool(format!("{}: {e}", path.display())))?;
    if v.get("format").and_then(serde_json::Value::as_str) != Some("bfbc2-navtune") {
        return Err(EditorError::Tool(format!(
            "{}: not a navtune side file",
            path.display()
        )));
    }
    if v.get("version").and_then(serde_json::Value::as_u64).unwrap_or(0) > 1 {
        return Err(EditorError::Tool(format!(
            "{}: a newer version than this editor reads",
            path.display()
        )));
    }
    let base: NavBase = v
        .get("base")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| EditorError::Tool(format!("{}: base: {e}", path.display())))?
        .unwrap_or_default();
    let edits: Vec<NavEdit> = v
        .get("operations")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| EditorError::Tool(format!("{}: operations: {e}", path.display())))?
        .unwrap_or_default();
    let kind = if base.kind.is_empty() {
        "navkit"
    } else {
        base.kind.as_str()
    };
    let bad: Vec<String> = edits
        .iter()
        .enumerate()
        .flat_map(|(k, e)| {
            problems(e, kind)
                .into_iter()
                .map(move |p| format!("operation {k}: {p}"))
        })
        .take(10)
        .collect();
    if !bad.is_empty() {
        return Err(EditorError::Tool(format!("{}: {}", path.display(), bad.join("; "))));
    }
    Ok((base, edits))
}

/// A file's SHA-1 (hex).
#[must_use]
pub fn sha1_of(path: &Path) -> String {
    std::fs::read(path).map_or_else(|_| String::new(), |d| sha1_smol::Sha1::from(&d).digest().to_string())
}

/// A point rounded to millimetres (`navtune.point`).
#[must_use]
pub fn point(p: [f64; 3]) -> [f64; 3] {
    p.map(navdata::round3)
}

/// The edit a finished drawing makes: the tool, its clicks, the settings. `None` until the drawing is complete (or
/// for a polygon: until `finish`).
#[allow(clippy::too_many_arguments, reason = "the tool's settings, as the panel holds them")]
#[must_use]
pub fn edit_of(
    tool: NavTool,
    clicks: &[[f64; 3]],
    polygon: bool,
    finish: bool,
    margin: [f64; 2],
    traversal: usize,
    two_way: bool,
    text: &str,
    by: &str,
) -> Option<NavEdit> {
    let base = NavEdit {
        op: tool.op().to_owned(),
        by: by.to_owned(),
        when: bc2_levelwrite::project::now(),
        ..NavEdit::default()
    };
    let value = (tool == NavTool::Walkable).then_some(true);
    match (tool, clicks) {
        (NavTool::Seed | NavTool::Unseed, [p, ..]) => Some(NavEdit {
            at: Some(point(*p)),
            ..base
        }),
        (NavTool::Note, [p, ..]) if !text.trim().is_empty() => Some(NavEdit {
            at: Some(point(*p)),
            text: Some(text.trim().to_owned()),
            ..base
        }),
        (NavTool::Traversal, [a, b, ..]) => Some(NavEdit {
            kind: Some(navtune::TRAVERSALS.get(traversal).unwrap_or(&"ladder").to_string()),
            bottom: Some(point(*a)),
            top: Some(point(*b)),
            two_way: Some(two_way),
            ..base
        }),
        (NavTool::LinkAreas, [a, b, ..]) => Some(NavEdit {
            a: Some(point(*a)),
            b: Some(point(*b)),
            ..base
        }),
        (NavTool::EdgeAdd | NavTool::EdgeRemove, [a, b, ..]) => Some(NavEdit {
            a: Some(point(*a)),
            b: Some(point(*b)),
            action: Some(if tool == NavTool::EdgeAdd { "add" } else { "remove" }.to_owned()),
            two_way: Some(two_way),
            ..base
        }),
        (NavTool::Walkable | NavTool::Unwalkable, [a, b]) if !polygon => {
            let lo = a[1].min(b[1]) - margin[0];
            let hi = a[1].max(b[1]) + margin[1];
            let cube = [a[0].min(b[0]), lo, a[2].min(b[2]), a[0].max(b[0]), hi, a[2].max(b[2])].map(navdata::round3);
            Some(NavEdit {
                region: Some(NavRegion {
                    cube: Some(cube),
                    ..NavRegion::default()
                }),
                value,
                ..base
            })
        }
        (NavTool::Walkable | NavTool::Unwalkable, pts) if polygon && finish && pts.len() >= 3 => {
            let (lo, hi) = pts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), p| {
                (l.min(p[1]), h.max(p[1]))
            });
            Some(NavEdit {
                region: Some(NavRegion {
                    polygon: Some(
                        pts.iter()
                            .map(|p| [navdata::round3(p[0]), navdata::round3(p[2])])
                            .collect(),
                    ),
                    y: Some([navdata::round3(lo - margin[0]), navdata::round3(hi + margin[1])]),
                    cube: None,
                }),
                value,
                ..base
            })
        }
        _ => None,
    }
}

/// What installing or restoring will do, shown before it is confirmed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// `install` or `restore`.
    pub what: &'static str,
    /// The command's arguments (after `navkit`; `--opfor` is added).
    pub args: Vec<String>,
    /// What it will do, line by line.
    pub lines: Vec<String>,
    /// The game copy's files it writes, moves or removes.
    pub touches: Vec<PathBuf>,
    /// Why it cannot run (nothing built, nothing to restore), or `None`.
    pub blocked: Option<String>,
}

fn traversal_of(dir: &Path, lv: &str) -> PathBuf {
    dir.join(format!("{}.traversal.json", lv.to_lowercase()))
}

/// The game copy's install ledger, NavKit's owner name for a level, and the game-relative path of a game file.
fn ledger_of(root: &Path, lv: &str, p: &Path) -> (Ledger, String, String) {
    let game = root.join("game");
    let rel = p.strip_prefix(&game).unwrap_or(p).to_string_lossy().replace('\\', "/");
    (Ledger::new(&game), format!("navkit:{}", lv.to_lowercase()), rel)
}

/// NavKit's markers from before the install ledger beside the path data (`.opfor-orig` / `.opfor-new`), and any
/// other tool's.
fn legacy_of(dest: &Path) -> (Vec<String>, Vec<String>) {
    gameledger::legacy_markers(dest)
        .into_iter()
        .partition(|m| m.eq_ignore_ascii_case(".opfor-orig") || m.eq_ignore_ascii_case(".opfor-new"))
}

/// What `navkit install <lv> --from <built>` will do (`navkit.install`, round DF2): the file there kept once in the
/// game copy's install ledger (by its sha256; or noted as none), the built file copied in, its traversal annotations
/// beside it when they were made for it. Another tool's install there stays below NavKit's (either can be restored
/// first).
#[must_use]
pub fn install_plan(root: &Path, lv: &str, built: &Path) -> Plan {
    let dest = navdata::installed_pathdata(root, lv);
    let rel = |p: &Path| p.strip_prefix(root).unwrap_or(p).display().to_string();
    let mut plan = Plan {
        what: "install",
        args: vec![
            "install".into(),
            lv.to_uppercase(),
            "--from".into(),
            built.display().to_string(),
        ],
        lines: Vec::new(),
        touches: vec![dest.clone()],
        blocked: None,
    };
    if !built.is_file() {
        plan.blocked = Some(format!("{}: not built yet (Build with these edits first)", rel(built)));
        return plan;
    }
    let (led, owner, game_rel) = ledger_of(root, lv, &dest);
    let (own, foreign) = legacy_of(&dest);
    if !foreign.is_empty() {
        plan.blocked = Some(gameledger::legacy_refusal(&game_rel, &foreign));
        return plan;
    }
    let holders = match led.holders(&game_rel) {
        Ok(h) => h,
        Err(e) => {
            plan.blocked = Some(e.to_string());
            return plan;
        }
    };
    if !own.is_empty() {
        plan.lines.push(format!(
            "Move NavKit's markers from before the install ledger ({}) into it",
            own.join(", ")
        ));
    }
    if holders.contains(&owner) || !own.is_empty() {
        plan.lines
            .push("The file from before NavKit is already kept (an earlier install): it stays as it is".to_owned());
    } else if let Some(top) = holders.last() {
        plan.lines.push(format!(
            "Install over {top}'s file (the ledger keeps it: either install can be restored first)"
        ));
    } else if dest.is_file() {
        plan.lines
            .push("Keep DICE's file in the game copy's install ledger (game\\opfor\\ledger)".to_owned());
    } else {
        plan.lines
            .push("The level has no path data: note that (restore will remove the file)".to_owned());
    }
    plan.lines.push(format!("Copy {} to {}", rel(built), rel(&dest)));
    let (trav, trav_dest) = (
        traversal_of(built.parent().unwrap_or(root), lv),
        traversal_of(dest.parent().unwrap_or(root), lv),
    );
    let made_for = std::fs::read_to_string(&trav)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v.get("pathdata_sha1").and_then(|s| s.as_str()).map(str::to_owned));
    if made_for.is_some_and(|s| s == sha1_of(built)) {
        plan.lines
            .push(format!("Copy its traversal annotations to {}", rel(&trav_dest)));
        plan.touches.push(trav_dest);
    } else if trav_dest.is_file() {
        plan.lines
            .push(format!("Remove {} (none were made for this file)", rel(&trav_dest)));
        plan.touches.push(trav_dest);
    }
    plan
}

/// What `navkit restore <lv>` will do (`navkit.restore`, round DF2): the file from before NavKit put back from the
/// install ledger, or NavKit's layer taken out from under a later install (which then puts that file back).
#[must_use]
pub fn restore_plan(root: &Path, lv: &str) -> Plan {
    let dest = navdata::installed_pathdata(root, lv);
    let rel = |p: &Path| p.strip_prefix(root).unwrap_or(p).display().to_string();
    let mut plan = Plan {
        what: "restore",
        args: vec!["restore".into(), lv.to_uppercase()],
        lines: Vec::new(),
        touches: vec![dest.clone()],
        blocked: None,
    };
    let (led, owner, game_rel) = ledger_of(root, lv, &dest);
    let (own, foreign) = legacy_of(&dest);
    let holders = led.holders(&game_rel).unwrap_or_default();
    let at = holders.iter().position(|h| *h == owner);
    if at.is_none() && own.is_empty() {
        plan.blocked = Some("Nothing to restore: no NavKit file is installed".to_owned());
        return plan;
    }
    if !foreign.is_empty() {
        plan.blocked = Some(gameledger::legacy_refusal(&game_rel, &foreign));
        return plan;
    }
    if !own.is_empty() {
        plan.lines.push(format!(
            "Move NavKit's markers from before the install ledger ({}) into it",
            own.join(", ")
        ));
    }
    let above = at.and_then(|i| holders.get(i + 1));
    let before = led.before(&owner, &game_rel).ok().flatten();
    let had_none = before.as_ref().is_some_and(Option::is_none)
        || (own.len() == 1 && own.first().is_some_and(|m| m.eq_ignore_ascii_case(".opfor-new")));
    if let Some(o) = above {
        plan.lines.push(format!(
            "Take NavKit's file out from under {o}'s install: the file stays {o}'s, its revert then puts back what was \
             there before NavKit"
        ));
    } else if had_none {
        plan.lines
            .push(format!("Remove {} (the level had no path data before)", rel(&dest)));
    } else {
        plan.lines
            .push("Put the file from before NavKit back from the install ledger".to_owned());
    }
    let trav = traversal_of(dest.parent().unwrap_or(root), lv);
    if trav.is_file() {
        plan.lines.push(format!("Remove {}", rel(&trav)));
        plan.touches.push(trav);
    }
    plan
}

/// A reference level's AI layer drawn on this level (the compare overlay).
#[derive(Debug)]
pub struct Compare {
    /// The source as typed (`level:<lv>`, `xbox:<lv>`, a partition's path).
    pub spec: String,
    /// What it is, in words.
    pub label: String,
    /// The partition read.
    pub partition: PathBuf,
    /// Its AI layer.
    pub layer: AiLayer,
    /// Its path data's outlines (when it has path data).
    pub pathdata: Option<NavData>,
    /// Its `SectorOffsets` (`None`: the level has no such asset).
    pub offsets: Option<(i64, i64)>,
    /// The marker selected, and its nearest NavKit route node on its floor (any distance).
    pub selected: Option<(usize, Option<navdata::Near>)>,
    /// Where an export went.
    pub saved: Option<PathBuf>,
}

/// One marker of a reference layer: its kind and where it is.
#[derive(Debug, Clone, PartialEq)]
pub struct RefItem {
    /// `spawner`, `area order`, `patrol`, `objective`.
    pub kind: &'static str,
    /// Its layer (`reference AI spawners (enabled)` ...).
    pub layer: &'static str,
    /// Its name or type.
    pub name: String,
    /// Where it is.
    pub pos: [f64; 3],
}

impl Compare {
    /// The markers, in the Studio's order: spawners, area orders, patrols, objectives.
    #[must_use]
    pub fn items(&self) -> Vec<RefItem> {
        let mut out = Vec::new();
        for s in &self.layer.spawners {
            let ai = s.player_type.as_deref() == Some("SSAiPlayer");
            let layer = match (ai, s.enabled) {
                (true, true) => "reference AI spawners (enabled)",
                (true, false) => "reference AI spawners (disabled)",
                (false, true) => "reference human spawners (enabled)",
                (false, false) => "reference human spawners (disabled)",
            };
            out.push(RefItem {
                kind: "spawner",
                layer,
                name: s.name.clone(),
                pos: s.pos,
            });
        }
        for o in &self.layer.area_orders {
            out.push(RefItem {
                kind: "area order",
                layer: "reference area orders",
                name: o.name.clone(),
                pos: o.pos,
            });
        }
        for p in self.layer.patrols.iter().filter(|p| p.points.len() > 1) {
            out.push(RefItem {
                kind: "patrol",
                layer: "reference patrol orders",
                name: p.name.clone(),
                pos: p.points.first().copied().unwrap_or_default(),
            });
        }
        for o in &self.layer.objectives {
            out.push(RefItem {
                kind: "objective",
                layer: "reference objectives",
                name: o.kind.clone(),
                pos: o.pos,
            });
        }
        out
    }
}

fn find_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    let want = name.to_lowercase();
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| {
        p.file_name()
            .is_some_and(|n| n.to_string_lossy().to_lowercase() == want)
    })
}

/// A reference source: its label, its partition, its path data, its `SectorOffsets`.
pub type Source = (String, PathBuf, Option<PathBuf>, Option<(i64, i64)>);

/// A reference source's partition, path data and `SectorOffsets` (`overlay.reference_source`): `level:<lv>` (a level
/// prepared in OPFOR's `work\levels`, or the editor's own Package copy), `xbox:<lv>` (DICE's Xbox 360 Onslaught
/// levels unpacked into `work\xbox_dlc`), or a level partition's path (`.dbx`, or the `.xml` beside one).
///
/// # Errors
/// An unknown source, or no partition found.
pub fn reference_source(spec: &str, root: &Path) -> Result<Source, EditorError> {
    let spec = spec.trim();
    let as_path = Path::new(spec);
    if as_path.is_file() {
        let dbx = as_path.with_extension("dbx");
        let part = if dbx.is_file() { dbx } else { as_path.to_owned() };
        let lv = as_path
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().to_lowercase());
        let off = as_path.parent().and_then(|d| navdata::sector_offsets(&d.join(&lv)));
        return Ok((spec.to_owned(), part, None, off));
    }
    let (kind, lv) = spec.split_once(':').ok_or_else(|| {
        EditorError::Tool(format!(
            "{spec}: an unknown source (level:<level>, xbox:<level> or a level partition's path)"
        ))
    })?;
    let lv = lv.trim().to_lowercase();
    match kind.trim() {
        "level" => {
            let d = root.join("work").join("levels").join(&lv).join("dist").join("levels");
            let pkg = root
                .join("work")
                .join("editor")
                .join("levels")
                .join(&lv)
                .join(format!("00_{lv}"))
                .join("levels");
            let part = find_ci(&d, &format!("{lv}.dbx"))
                .or_else(|| find_ci(&pkg, &format!("{lv}.dbx")))
                .ok_or_else(|| {
                    EditorError::Level(format!(
                        "{lv}: not prepared (no level partition in work\\levels or the editor's copy)"
                    ))
                })?;
            let hpd = navdata::original_file(&navdata::installed_pathdata(root, &lv));
            let off = part.parent().and_then(|d| navdata::sector_offsets(&d.join(&lv)));
            Ok((format!("{lv} (OPFOR level copy)"), part, hpd, off))
        }
        "xbox" => {
            let base = root.join("work").join("xbox_dlc");
            let dirs = if lv == "ons_mp_002" {
                vec![base.join("o2").join("levels")]
            } else {
                vec![base.join("u3").join("levels").join(&lv).join("x_level").join("levels")]
            };
            let part = dirs
                .iter()
                .find_map(|d| find_ci(d, &format!("{lv}.dbx")))
                .ok_or_else(|| EditorError::Tool(format!("{lv}: no Xbox 360 level partition in work\\xbox_dlc")))?;
            let hpd = base
                .join("454108A800000002")
                .join("ai")
                .join("levels")
                .join(&lv)
                .join("infantry")
                .join("pathdata.hpd");
            let off = dirs.iter().find_map(|d| navdata::sector_offsets(&d.join(&lv)));
            Ok((
                format!("{lv} (DICE Xbox 360 DLC)"),
                part,
                hpd.is_file().then_some(hpd),
                off,
            ))
        }
        other => Err(EditorError::Tool(format!(
            "{other}: an unknown source (level:<level>, xbox:<level> or a level partition's path)"
        ))),
    }
}

/// Reads a reference source.
///
/// # Errors
/// As [`reference_source`], or the partition does not read.
pub fn open_compare(spec: &str, root: &Path) -> Result<Compare, EditorError> {
    let (label, partition, hpd, offsets) = reference_source(spec, root)?;
    let data = std::fs::read(&partition).map_err(|e| EditorError::Tool(format!("{}: {e}", partition.display())))?;
    let layer = navdata::read_ai_layer(&data)?;
    let pathdata = hpd.and_then(|p| navdata::load(&p, None).ok());
    Ok(Compare {
        spec: spec.trim().to_owned(),
        label,
        partition,
        layer,
        pathdata,
        offsets,
        selected: None,
        saved: None,
    })
}

/// A JSON number rounded as the reports keep it.
fn num(v: f64) -> serde_json::Value {
    serde_json::json!(v)
}

fn pos_json(p: [f64; 3]) -> serde_json::Value {
    serde_json::json!([num(p[0]), num(p[1]), num(p[2])])
}

fn row_json(r: &navdata::NodeRow) -> [(&'static str, serde_json::Value); 4] {
    [
        ("navkit_ground", serde_json::json!(r.navkit_ground)),
        ("nearest_node_m", serde_json::json!(r.nearest_node_m)),
        ("node_island_size", serde_json::json!(r.node_island_size)),
        ("node_on_main_island", serde_json::json!(r.node_on_main_island)),
    ]
}

/// The compare report (`overlay.reference_report`): per spawner, order, patrol waypoint and objective its fields
/// and the target's NavKit build's lookups there.
#[must_use]
pub fn compare_report(target: &str, c: &Compare, navkit: Option<&NavFile>) -> serde_json::Value {
    let idx = navkit.map(|f| NavIndex::new(&f.data));
    let row = |p: [f64; 3]| navdata::node_row(idx.as_ref(), p);
    let obj = |mut base: serde_json::Map<String, serde_json::Value>, p: [f64; 3]| {
        for (k, v) in row_json(&row(p)) {
            base.insert(k.to_owned(), v);
        }
        serde_json::Value::Object(base)
    };
    let l = &c.layer;
    let spawners: Vec<_> = l
        .spawners
        .iter()
        .map(|s| {
            let mut m = serde_json::Map::new();
            m.insert("pos".into(), pos_json(s.pos));
            m.insert("name".into(), s.name.clone().into());
            m.insert("player_type".into(), serde_json::json!(s.player_type));
            m.insert("enabled".into(), s.enabled.into());
            m.insert("team".into(), serde_json::json!(s.team));
            m.insert("squad_id".into(), serde_json::json!(s.squad_id));
            m.insert("in_squad_entity".into(), s.in_squad_entity.into());
            obj(m, s.pos)
        })
        .collect();
    let orders: Vec<_> = l
        .area_orders
        .iter()
        .map(|o| {
            let mut m = serde_json::Map::new();
            m.insert("pos".into(), pos_json(o.pos));
            m.insert("name".into(), o.name.clone().into());
            m.insert("area_type".into(), serde_json::json!(o.area_type));
            obj(m, o.pos)
        })
        .collect();
    let points: Vec<_> = l
        .patrols
        .iter()
        .flat_map(|p| p.points.iter().map(move |q| (p, *q)))
        .map(|(p, q)| {
            let mut m = serde_json::Map::new();
            m.insert("patrol".into(), p.name.clone().into());
            m.insert("pos".into(), pos_json(q));
            obj(m, q)
        })
        .collect();
    let objectives: Vec<_> = l
        .objectives
        .iter()
        .map(|o| {
            let mut m = serde_json::Map::new();
            m.insert("pos".into(), pos_json(o.pos));
            m.insert("type".into(), o.kind.clone().into());
            obj(m, o.pos)
        })
        .collect();
    serde_json::json!({
        "target": target,
        "target_navkit_build": navkit.map(|f| f.data.path.display().to_string()),
        "reference": c.label,
        "reference_partition": c.partition.display().to_string(),
        "reference_pathdata": c.pathdata.as_ref().map(|n| n.path.display().to_string()),
        "reference_has_ai": l.has_ai,
        "reference_sector_offsets": c.offsets.map(|(x, z)| [x, z]),
        "summary": {
            "spawners": l.spawners.len(),
            "ai_spawners_enabled": l.spawners.iter().filter(|s| s.enabled && s.player_type.as_deref() == Some("SSAiPlayer")).count(),
            "area_orders": l.area_orders.len(),
            "patrols": l.patrols.len(),
            "objectives": l.objectives.len(),
        },
        "spawners": spawners,
        "area_orders": orders,
        "patrol_points": points,
        "objectives": objectives,
    })
}

/// An anomaly as the export writes it (`overlay.anomalies`' rows).
#[must_use]
pub fn anomaly_json(a: &navdata::Anomaly) -> serde_json::Value {
    use navdata::Anomaly as A;
    match a {
        A::SmallIsland {
            island,
            pos,
            nodes,
            cells,
        } => {
            serde_json::json!({"island": island, "pos": pos_json(*pos), "nodes": nodes, "cells": cells})
        }
        A::SpawnerOnSmallIsland {
            pos,
            name,
            player_type,
            enabled,
            island_size,
            node_m,
        } => serde_json::json!({"pos": pos_json(*pos), "name": name, "player_type": player_type, "enabled": enabled,
            "island_size": island_size, "node_m": node_m}),
        A::OrderWithoutNode { kind, name, pos } => {
            serde_json::json!({"kind": kind, "name": name, "pos": pos_json(*pos)})
        }
        A::SliverArea {
            area,
            sector,
            cells,
            pos,
        } => serde_json::json!({"area": area, "sector": sector, "cells": cells, "pos": pos_json(*pos)}),
        A::LonelyNode { node, pos } => serde_json::json!({"node": node, "pos": pos_json(*pos)}),
    }
}

/// The anomaly export (`overlay.anomaly_report`'s form).
#[must_use]
pub fn anomaly_report(target: &str, navkit: &NavFile, an: &Anomalies) -> serde_json::Value {
    let list = |v: &[navdata::Anomaly]| v.iter().map(anomaly_json).collect::<Vec<_>>();
    serde_json::json!({
        "target": target,
        "navkit_build": navkit.data.path.display().to_string(),
        "counts": {
            "small_islands": an.small_islands.len(),
            "spawners_on_small_islands": an.spawners_on_small_islands.len(),
            "orders_without_node": an.orders_without_node.len(),
            "sliver_areas": an.sliver_areas.len(),
            "lonely_nodes": an.lonely_nodes.len(),
        },
        "small_islands": list(&an.small_islands),
        "spawners_on_small_islands": list(&an.spawners_on_small_islands),
        "orders_without_node": list(&an.orders_without_node),
        "sliver_areas": list(&an.sliver_areas),
        "lonely_nodes": list(&an.lonely_nodes),
    })
}

/// Writes an export into `work\navstudio` (the folder the Studio's exports went to); its path.
///
/// # Errors
/// It cannot be written.
pub fn save_export(root: &Path, name: &str, v: &serde_json::Value) -> Result<PathBuf, EditorError> {
    let d = root.join("work").join("navstudio");
    std::fs::create_dir_all(&d).map_err(|e| EditorError::Tool(format!("{}: {e}", d.display())))?;
    let p = d.join(name);
    let text = serde_json::to_string_pretty(v).map_err(|e| EditorError::Tool(e.to_string()))?;
    write_side(&p, &text)?;
    Ok(p)
}

/// The AI workspace's navigation state of an open level.
#[derive(Debug, Default)]
#[allow(clippy::struct_excessive_bools, reason = "the panels' switches, each its own")]
pub struct NavState {
    /// The files found (NavKit's build, DICE's file), before they are read.
    pub found: (Option<PathBuf>, Option<PathBuf>),
    /// What was read, or why not; `None` while reading (or before).
    pub loaded: Option<Result<Loaded, EditorError>>,
    rx: Option<Task<Loaded>>,
    /// The ground's colouring.
    pub colour: ColourMode,
    /// The layers hidden (by name).
    pub hidden: BTreeSet<String>,
    /// The area selected: (file, area).
    pub selected: Option<(&'static str, usize)>,
    /// A soldier spawn selected (scene index, its place among the spawns).
    pub spawn: Option<(usize, usize)>,
    /// The path test.
    pub path: PathTest,
    /// The compare overlay.
    pub compare: Option<Compare>,
    /// The compare source being typed.
    pub compare_input: String,
    /// Why the last compare did not open.
    pub compare_error: Option<String>,
    /// The Islands and spawns panel's file.
    pub islands_file: usize,
    /// The edit tool.
    pub tool: Option<NavTool>,
    /// Regions are drawn as polygons (else boxes).
    pub polygon: bool,
    /// The drawing's clicks.
    pub clicks: Vec<[f64; 3]>,
    /// A region's height range below and above the clicked points.
    pub margin: [f64; 2],
    /// The traversal kind (index into `navtune::TRAVERSALS`).
    pub traversal: usize,
    /// Route edges and traversals both ways.
    pub two_way: bool,
    /// A note's text.
    pub note: String,
    /// Who the edits are recorded as.
    pub by: String,
    /// The last edit message.
    pub message: String,
    /// The build, install or restore running or done.
    pub job: Option<Job>,
    /// The plan waiting for its confirmation.
    pub confirm: Option<Plan>,
    /// The anomaly export's path.
    pub anomaly_saved: Option<PathBuf>,
    /// The file NavKit's build is read from instead of the newest (Open the result).
    pub candidate: Option<PathBuf>,
    /// The OPFOR folder the side file, the builds and the installs go to (the editor's; tests: a scratch copy).
    pub root: Option<PathBuf>,
    /// The project revision the drawn edits were made at.
    pub drawn: Option<u64>,
    /// Cached geometry: the ground meshes (by colouring and theme) and the line layers.
    pub cache: HashMap<String, Arc<Vec<crate::navview::OverlayMesh>>>,
    /// Cached line layers.
    pub lines: HashMap<String, Arc<crate::navview::Lines>>,
    /// The hover's words (the area under the pointer).
    pub hover: Option<String>,
    /// The soldier spawns read (scene indices), in the order of [`NavFile::spawns`].
    pub spawn_list: Vec<usize>,
    /// The panels' buttons on screen by name (the tests click them).
    pub rects: Vec<(String, eframe::egui::Rect)>,
    /// A panel changed a layer or the compare: the views are drawn again after it.
    pub dirty: bool,
    /// Open the result was pressed.
    pub open_result: bool,
}

impl NavState {
    /// Default settings (the Studio's: polygons, 2 m below and 4 m above, two-way, the PC's name).
    #[must_use]
    pub fn new() -> Self {
        Self {
            polygon: true,
            margin: [2.0, 4.0],
            two_way: true,
            by: bc2_levelwrite::project::author(),
            hidden: [
                "DICE ground",
                "NavKit outlines",
                "NavKit portals",
                "NavKit border portals",
                "NavKit route graph",
                "DICE outlines",
                "DICE portals",
                "DICE border portals",
                "DICE route graph",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            ..Self::default()
        }
    }

    /// Starts reading a level's path data on a worker thread (once).
    pub fn start(&mut self, root: &Path, lv: &str, inputs: impl FnOnce(Option<PathBuf>, Option<PathBuf>) -> Inputs) {
        if self.loaded.is_some() || self.rx.is_some() {
            return;
        }
        let (mut cand, dice) = navdata::default_files(root, lv);
        if let Some(c) = self.candidate.clone() {
            cand = Some(c);
        }
        self.found = (cand.clone(), dice.clone());
        if cand.is_none() && dice.is_none() {
            self.loaded = Some(Err(EditorError::Tool(format!(
                "{} has no navigation data (no pathdata.hpd installed or built by NavKit)",
                lv.to_uppercase()
            ))));
            return;
        }
        if cand.is_none() {
            // only DICE's: its ground shown
            self.hidden.remove("DICE ground");
        }
        let inputs = inputs(cand, dice);
        self.rx = Some(Task::spawn("Reading the navigation data", move || read(&inputs)));
    }

    /// Takes the worker's answer when it is there; `true` when it arrived now.
    pub fn poll(&mut self) -> bool {
        let Some(r) = self.rx.as_ref().and_then(Task::poll) else {
            return false;
        };
        self.loaded = Some(r);
        self.rx = None;
        self.cache.clear();
        self.lines.clear();
        true
    }

    /// Waits for the worker (tests).
    pub fn wait(&mut self, limit: std::time::Duration) {
        let t0 = Instant::now();
        while self.rx.is_some() && t0.elapsed() < limit {
            if !self.poll() {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
    }

    /// Whether the worker is reading.
    #[must_use]
    pub fn reading(&self) -> bool {
        self.rx.is_some()
    }

    /// Reads again (after a build: Open the result).
    pub fn reset(&mut self) {
        self.loaded = None;
        self.rx = None;
        self.cache.clear();
        self.lines.clear();
        self.selected = None;
        self.path.routes.clear();
    }

    /// The files read, if any.
    #[must_use]
    pub fn data(&self) -> Option<&Loaded> {
        self.loaded.as_ref().and_then(|r| r.as_ref().ok())
    }

    /// Whether a layer is shown.
    #[must_use]
    pub fn shown(&self, layer: &str) -> bool {
        !self.hidden.contains(layer)
    }

    /// The file edits are made on: NavKit's build when there is one, else DICE's.
    #[must_use]
    pub fn target(&self) -> Option<&NavFile> {
        let d = self.data()?;
        d.file("NavKit").or_else(|| d.file("DICE"))
    }

    /// The base kind of new edits: `navkit` on NavKit's build (or with no file), `dice` on DICE's.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match (&self.found.0, &self.found.1) {
            (None, Some(_)) => "dice",
            _ => "navkit",
        }
    }

    /// The path test between the two points (each file's route), when there are two.
    pub fn run_path(&mut self) {
        self.path.routes.clear();
        let (Some(a), Some(b)) = (self.path.points.first().copied(), self.path.points.get(1).copied()) else {
            return;
        };
        let Some(d) = self.data() else { return };
        let routes: Vec<(&'static str, Route)> = d
            .files
            .iter()
            .map(|f| (f.name, navdata::route(&f.data, &f.graph, a, b)))
            .collect();
        self.path.routes = routes;
    }

    /// A click for the path test: the start, then the end (a third starts again).
    pub fn path_click(&mut self, p: [f64; 3]) {
        if self.path.points.len() >= 2 {
            self.path.points.clear();
        }
        self.path.points.push(p);
        self.run_path();
    }

    /// NavKit's route cost against DICE's (per cent), when both found one.
    #[must_use]
    pub fn cost_change(&self) -> Option<f64> {
        let get = |n: &str| self.path.routes.iter().find(|(f, _)| *f == n).map(|(_, r)| r);
        navdata::route_cost_change(get("NavKit"), get("DICE"))
    }

    /// The area under a point in the shown files: (file, area), NavKit's first.
    #[must_use]
    pub fn area_at(&self, p: [f64; 3]) -> Option<(&'static str, usize)> {
        let d = self.data()?;
        d.files
            .iter()
            .filter(|f| self.shown(&format!("{} ground", f.name)))
            .chain(d.files.iter())
            .find_map(|f| f.cell_at(p).map(|(_, a)| (f.name, a)))
    }

    /// The hover's words for an area.
    #[must_use]
    pub fn describe_area(&self, file: &str, k: usize) -> Option<String> {
        let a = self.data()?.file(file)?.data.areas.get(k)?;
        Some(format!(
            "{file} area {k} - sector 0x{:04X}, island {}, pitch {:.1}",
            a.sector, a.island, a.pitch
        ))
    }
}
