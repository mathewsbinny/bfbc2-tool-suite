//! Preparing a level from the editor (`EDITOR_ARCHITECTURE.md` 3.10, "Decisions on G1's first version"): a level
//! NavKit has not prepared, or whose terrain unpack is missing or stale, is prepared when it is opened - open, then
//! "Preparing `MP_001` (step 2 of 4)", then the map - without sending the user to another tool.
//!
//! The work is the suite's own, run as a child process: `bfbc2.exe prepare_level` (round O4's Rust port, the
//! suite's `bin\bfbc2.exe` - [`binary`]; the Python script it ported, `prepare_level.py`, was retired in round RT1):
//! NavKit's prepare and the Map Inspector's terrain unpack. It reports steps and log lines on its output ([`parse`]); the
//! editor shows them ([`Job::poll`]) and can cancel ([`Job::cancel`]). The child writes only where those steps write:
//! Project OPFOR BFBC2's `work\levels\<level>` and `work\navkit`, never the game copy.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::EditorError;
use crate::{levels, package};

/// The environment variable naming the Python to run (default `python`, then `py -3`).
pub const PYTHON_ENV: &str = "BFBC2_PYTHON";

/// What a level needs before it opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    /// Ready: open it.
    Ready,
    /// NavKit has not prepared it: every step.
    Full,
    /// Prepared, but its terrain unpack is missing or older than its bundle: the terrain step.
    Terrain,
    /// A level of `game\Package` only whose editor's copy is missing or not current: read from its bundles, in the
    /// editor ([`crate::package::prepare`], on a thread; round G3).
    Package,
}

/// What `lv` needs before the editor opens it.
#[must_use]
pub fn plan(opfor: &Path, lv: &str) -> Plan {
    let lv = lv.to_lowercase();
    if let Some(p) = package::find(opfor, &lv) {
        return if package::ready(opfor, &p).is_ok() {
            Plan::Ready
        } else {
            Plan::Package
        };
    }
    if !opfor.join("work").join("levels").join(&lv).join("dist").is_dir() {
        Plan::Full
    } else if levels::terrain_bundle(opfor, &lv).is_some() && levels::terrain_unpack(opfor, &lv).is_err() {
        Plan::Terrain
    } else {
        Plan::Ready
    }
}

/// `<suite>\bin\bfbc2.exe` of the suite this editor was built in.
fn bin_exe() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("bin")
        .join("bfbc2.exe")
}

/// The suite's command-line binary when it can prepare (round O4: `bfbc2.exe prepare_level`, NavKit's prepare and
/// the terrain unpack; its Python twin `prepare_level.py` was retired in round RT1): [`bin_exe`], asked once whether
/// it knows the command (an older binary prints its usage instead). None when it cannot.
#[must_use]
pub fn binary() -> Option<PathBuf> {
    static FOUND: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    FOUND
        .get_or_init(|| {
            let exe = bin_exe();
            let mut cmd = Command::new(&exe);
            cmd.arg("prepare_level").stdin(Stdio::null());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x0800_0000);
            }
            // with no level it answers in the script's words
            let out = cmd.output().ok()?;
            String::from_utf8_lossy(&out.stdout)
                .starts_with("@error Which level?")
                .then_some(exe)
        })
        .clone()
}

/// One line of the script's output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// Step `n` of `of` starts.
    Step {
        /// Its number, from 1.
        n: usize,
        /// How many there are.
        of: usize,
        /// What it does.
        what: String,
    },
    /// A log line.
    Log(String),
    /// Why it stopped.
    Error(String),
    /// Finished.
    Done,
}

/// A line of the script's output, read (anything not in its form is a log line: the tools' own prints).
#[must_use]
pub fn parse(line: &str) -> Line {
    let line = line.trim_end();
    if let Some(rest) = line.strip_prefix("@step ") {
        let mut it = rest.splitn(3, ' ');
        if let (Some(Ok(n)), Some(Ok(of)), Some(what)) =
            (it.next().map(str::parse), it.next().map(str::parse), it.next())
        {
            return Line::Step {
                n,
                of,
                what: what.to_owned(),
            };
        }
    }
    if let Some(rest) = line.strip_prefix("@log") {
        return Line::Log(rest.trim().to_owned());
    }
    if let Some(rest) = line.strip_prefix("@error") {
        return Line::Error(rest.trim().to_owned());
    }
    if line == "@done" {
        return Line::Done;
    }
    Line::Log(line.trim().to_owned())
}

/// Where a preparation is.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Progress {
    /// The current step (from 1; 0 before the first).
    pub step: usize,
    /// How many steps (0 before the first is announced).
    pub steps: usize,
    /// What the current step does.
    pub what: String,
    /// The last log line.
    pub last: String,
    /// Every log line so far.
    pub log: Vec<String>,
}

impl Progress {
    /// The share done, 0 - 1: finished steps, and half of the current one.
    #[must_use]
    pub fn fraction(&self) -> f32 {
        if self.steps == 0 {
            0.0
        } else {
            ((self.step.saturating_sub(1)) as f32 + 0.5) / self.steps as f32
        }
    }
}

/// How a preparation ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Still running.
    Running,
    /// Finished: the level opens.
    Done,
    /// Stopped, with why in plain words.
    Failed(String),
}

enum Msg {
    Line(String),
    Exit,
}

/// A preparation running in a child process.
pub struct Job {
    /// The level.
    pub lv: String,
    /// Where it is.
    pub progress: Progress,
    /// When it started.
    pub since: Instant,
    child: Option<Child>,
    /// An in-process preparation (a Package level): set to stop it.
    stop: Option<Arc<AtomicBool>>,
    /// The in-process preparation's job (round E4-rest2: on [`crate::task::Task`]; a panic is its answer).
    task: Option<crate::task::Task<()>>,
    rx: Receiver<Msg>,
    error: Option<String>,
    stderr_tail: Option<String>,
    exits: u8,
    done: bool,
    outcome: Outcome,
}

impl std::fmt::Debug for Job {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Job")
            .field("lv", &self.lv)
            .field("progress", &self.progress)
            .field("outcome", &self.outcome)
            .finish_non_exhaustive()
    }
}

/// The Pythons to try: `BFBC2_PYTHON`, else `python`, else the launcher `py -3`.
fn pythons() -> Vec<Vec<String>> {
    if let Some(p) = std::env::var_os(PYTHON_ENV).filter(|v| !v.is_empty()) {
        return vec![vec![p.to_string_lossy().into_owned()]];
    }
    vec![vec!["python".into()], vec!["py".into(), "-3".into()]]
}

impl Job {
    /// Starts preparing `lv` of the game copy at `opfor` as `plan` says, with the suite's `bfbc2.exe prepare_level`.
    ///
    /// # Errors
    ///
    /// The binary is not built (or too old to prepare), or could not be started, in plain words.
    pub fn start(opfor: &Path, lv: &str, plan: Plan) -> Result<Self, EditorError> {
        if plan == Plan::Package {
            return Self::start_package(opfor, lv);
        }
        let Some(exe) = binary() else {
            return Err(EditorError::Tool(format!(
                "Preparing a level needs the suite's command-line binary {} (its Python twin, prepare_level.py, was \
                 retired in round RT1), and it is not built or cannot prepare. In the suite's folder run \"python \
                 rust\\build_native.py\" (needs Rust and maturin) or \"python rust\\build_native.py --download\".",
                bin_exe().display()
            )));
        };
        let mut cmd = Command::new(&exe);
        cmd.arg("prepare_level");
        Self::spawn(cmd, opfor, lv, plan)
            .map_err(|e| EditorError::Tool(format!("{} could not be started: {e}", exe.display())))
    }

    /// Starts preparing a Package level in the editor ([`package::prepare`] on a thread), its steps and log reported
    /// as the script's are.
    ///
    /// # Errors
    ///
    /// It is not an installed Package level.
    pub fn start_package(opfor: &Path, lv: &str) -> Result<Self, EditorError> {
        let level = package::find(opfor, lv).ok_or_else(|| {
            EditorError::Tool(format!(
                "{} is not an installed level of game\\Package (package.mft names it with every bundle it loads).",
                lv.to_uppercase()
            ))
        })?;
        let (tx, rx) = std::sync::mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let root = opfor.to_path_buf();
        // round E4-rest2: on Task (its thread named, a panic its answer); its lines stream on the job's channel
        let task = crate::task::Task::spawn_value(&format!("Preparing {}", lv.to_uppercase()), move || {
            let send = |l: String| {
                let _ = tx.send(Msg::Line(l));
            };
            let r = package::prepare(
                &root,
                &level,
                &mut |e| match e {
                    package::Event::Step { n, of, what } => send(format!("@step {n} {of} {what}")),
                    package::Event::Log(t) => send(format!("@log {t}")),
                },
                &flag,
            );
            match r {
                Ok(_) => send("@done".into()),
                Err(e) => send(format!("@error {}", e.to_string().replace(['\r', '\n'], " "))),
            }
            // the end, as a child's two readers report it
            let _ = tx.send(Msg::Exit);
            let _ = tx.send(Msg::Exit);
        });
        Ok(Self {
            lv: lv.to_lowercase(),
            progress: Progress::default(),
            since: Instant::now(),
            child: None,
            stop: Some(stop),
            task: Some(task),
            rx,
            error: None,
            stderr_tail: None,
            exits: 0,
            done: false,
            outcome: Outcome::Running,
        })
    }

    /// A preparing script under a Python, speaking `bfbc2.exe prepare_level`'s output protocol (the tests'
    /// stand-in).
    ///
    /// # Errors
    ///
    /// No Python could be started, in plain words.
    pub fn start_with(script: &Path, opfor: &Path, lv: &str, plan: Plan) -> Result<Self, EditorError> {
        let mut last_err = String::new();
        for py in pythons() {
            let Some((exe, pre)) = py.split_first() else { continue };
            let mut cmd = Command::new(exe);
            cmd.args(pre).arg("-u").arg(script);
            match Self::spawn(cmd, opfor, lv, plan) {
                Ok(job) => return Ok(job),
                Err(e) => last_err = format!("{exe}: {e}"),
            }
        }
        Err(EditorError::Tool(format!(
            "Preparing a level needs Python 3.12 or later with the suite's tools, and none could be started \
             ({last_err}). Install Python, or set {PYTHON_ENV} to the python.exe to use."
        )))
    }

    /// The preparing command (the script under a Python, or the binary's `prepare_level`) started for `lv`, its two
    /// output streams read on their own threads.
    fn spawn(mut cmd: Command, opfor: &Path, lv: &str, plan: Plan) -> std::io::Result<Self> {
        cmd.arg(lv)
            .arg("--opfor")
            .arg(opfor)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("PYTHONIOENCODING", "utf-8");
        if plan == Plan::Terrain {
            cmd.arg("--terrain-only");
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd.spawn()?;
        let (tx, rx) = std::sync::mpsc::channel();
        let out = child.stdout.take();
        let err = child.stderr.take();
        let tx_err = tx.clone();
        // stderr: a traceback's last line becomes the error when the script did not say one
        std::thread::spawn(move || {
            if let Some(e) = err {
                for line in BufReader::new(e).lines().map_while(Result::ok) {
                    let _ = tx_err.send(Msg::Line(format!("@stderr {line}")));
                }
            }
            let _ = tx_err.send(Msg::Exit);
        });
        std::thread::spawn(move || {
            if let Some(o) = out {
                for line in BufReader::new(o).lines().map_while(Result::ok) {
                    if tx.send(Msg::Line(line)).is_err() {
                        return;
                    }
                }
            }
            let _ = tx.send(Msg::Exit);
        });
        Ok(Self {
            lv: lv.to_lowercase(),
            progress: Progress::default(),
            since: Instant::now(),
            child: Some(child),
            stop: None,
            task: None,
            rx,
            error: None,
            stderr_tail: None,
            exits: 0,
            done: false,
            outcome: Outcome::Running,
        })
    }

    /// Reads what the child said since the last call; the outcome once it has exited.
    pub fn poll(&mut self) -> Outcome {
        if self.outcome != Outcome::Running {
            return self.outcome.clone();
        }
        loop {
            match self.rx.try_recv() {
                Ok(Msg::Line(l)) => {
                    if let Some(e) = l.strip_prefix("@stderr ") {
                        if !e.trim().is_empty() {
                            self.stderr_tail = Some(e.trim().to_owned());
                            self.progress.log.push(e.trim().to_owned());
                        }
                        continue;
                    }
                    match parse(&l) {
                        Line::Step { n, of, what } => {
                            self.progress.step = n;
                            self.progress.steps = of;
                            self.progress.what = what;
                        }
                        Line::Log(t) => {
                            if !t.is_empty() {
                                self.progress.last.clone_from(&t);
                                self.progress.log.push(t);
                            }
                        }
                        Line::Error(e) => self.error = Some(e),
                        Line::Done => self.done = true,
                    }
                }
                Ok(Msg::Exit) if self.exits == 0 => self.exits = 1,
                Ok(Msg::Exit) | Err(TryRecvError::Disconnected) => {
                    let ok = match self.child.as_mut() {
                        Some(c) => c.wait().is_ok_and(|s| s.success()),
                        None => self.stop.is_some(), // in process: done says it
                    };
                    self.child = None;
                    // an in-process preparation that panicked: its job's answer says so
                    if let Some(Some(Err(e))) = self.task.take().map(|t| t.wait_for(Duration::from_secs(5))) {
                        self.error.get_or_insert_with(|| e.to_string());
                    }
                    self.outcome = if self.done && ok {
                        Outcome::Done
                    } else {
                        Outcome::Failed(self.error.clone().unwrap_or_else(|| {
                            let tail = self
                                .stderr_tail
                                .clone()
                                .or_else(|| self.progress.log.last().cloned())
                                .unwrap_or_else(|| "it stopped without saying why".into());
                            format!("Preparing {} stopped: {tail}", self.lv.to_uppercase())
                        }))
                    };
                    return self.outcome.clone();
                }
                Err(TryRecvError::Empty) => return Outcome::Running,
            }
        }
    }

    /// Stops the child (the level stays unprepared; NavKit's unpacks are redone or reused on the next try).
    pub fn cancel(&mut self) {
        if let Some(s) = &self.stop {
            s.store(true, Ordering::Relaxed);
        }
        if let Some(c) = self.child.as_mut() {
            let _ = c.kill();
            let _ = c.wait();
        }
        self.child = None;
        self.outcome = Outcome::Failed(format!("Preparing {} was cancelled.", self.lv.to_uppercase()));
    }

    /// Waits for the end (tests, headless runs), polling every 50 ms, at most `limit`.
    pub fn wait(&mut self, limit: Duration) -> Outcome {
        let start = Instant::now();
        loop {
            let o = self.poll();
            if o != Outcome::Running {
                return o;
            }
            if start.elapsed() > limit {
                self.cancel();
                return self.outcome.clone();
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        if let Some(s) = &self.stop {
            s.store(true, Ordering::Relaxed);
        }
        if let Some(c) = self.child.as_mut() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn lines() {
        assert_eq!(
            parse("@step 2 4 Unpacking the level"),
            Line::Step {
                n: 2,
                of: 4,
                what: "Unpacking the level".into()
            }
        );
        assert_eq!(parse("@log  box 1,2"), Line::Log("box 1,2".into()));
        assert_eq!(parse("@error no game copy\r"), Line::Error("no game copy".into()));
        assert_eq!(parse("@done"), Line::Done);
        assert_eq!(parse("plain print"), Line::Log("plain print".into()));
        assert_eq!(parse("@step x"), Line::Log("@step x".into()));
        let mut p = Progress::default();
        assert!(p.fraction().abs() < f32::EPSILON);
        p.step = 2;
        p.steps = 4;
        assert!((p.fraction() - 0.375).abs() < 1e-6);
    }

    #[test]
    fn plans() {
        let root = std::env::temp_dir().join(format!("bc2_editor_plan_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let lv = root
            .join("game")
            .join("Dist")
            .join("win32")
            .join("levels")
            .join("MP_005");
        std::fs::create_dir_all(&lv).unwrap();
        assert_eq!(plan(&root, "mp_005"), Plan::Full);
        std::fs::create_dir_all(root.join("work").join("levels").join("mp_005").join("dist")).unwrap();
        // no terrain bundle: nothing to unpack
        assert_eq!(plan(&root, "MP_005"), Plan::Ready);
        std::fs::write(lv.join("terrain-00.fbrb"), b"t").unwrap();
        assert_eq!(plan(&root, "mp_005"), Plan::Terrain);
        let _ = std::fs::remove_dir_all(&root);
    }
}
