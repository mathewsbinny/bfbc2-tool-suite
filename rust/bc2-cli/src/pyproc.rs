//! A Python command line's process as the round O2 tools run in `bfbc2.exe`: what a tool prints, in the order and
//! bytes Python 3.13's stdio on Windows gives, its day log as `bfbc2core.errors.setup` keeps it, its errors as
//! `errors.guard` reports them, and the hand-off to the Python command line where the Rust core refuses.
//!
//! **The streams.** A tool prints through [`Py::print`] / [`Py::write`] (stdout) and [`Py::eprint`] (stderr). Each
//! write is encoded as Python's `TextIOWrapper` encodes it: `\n` as `\r\n`, then the ANSI code page off a console
//! (stdout strict: a character the page lacks raises `UnicodeEncodeError` at that write; `errors='replace'` after
//! `sys.stdout.reconfigure`; stderr `backslashreplace`), UTF-8 on a console. The bytes then go through Python's two
//! buffers (the text layer's 8,192-byte pending chunk and `BufferedWriter`'s 8,192 bytes), so stdout reaches the
//! pipe in the chunks and at the moments Python's would: what two streams sharing one pipe see. stderr is
//! line-buffered. `PYTHONUNBUFFERED` writes through. Everything is recorded and written at the end, so a run that
//! hands off to Python has printed nothing.
//!
//! **The log.** `errors.setup(tool)`: `%LOCALAPPDATA%\bfbc2-tool-suite\logs\<tool>-<date>.log` (the day's part, as
//! `caches.day_log`), its start line `--- <tool> started (bfbc2.exe, win32) ---` (Python's names its version), and on
//! an error the record `errors.report` writes (its traceback is Python's own; here one line names the exception).
//! The start-up pruning of the log folder (`caches.prune_logs`) is left to the Python tools (round O3).
//!
//! **Errors.** A tool returns [`Fail::Err`] with the Python exception it would raise ([`PyErr`]: its type, `str()`
//! and filename); a guarded tool reports it as `errors.report` does (`error: <cause>` / `what to do` / `details`,
//! exit code 1), an unguarded one as Python's traceback (first and last line). [`Fail::Python`] hands the whole
//! command line to the Python (the reference): `python <the tool's .py> <arguments>`, its stdio inherited, its exit
//! code returned (`BFBC2_PYTHON` names the interpreter, else `python.exe` on `PATH`; `BFBC2_CLI_SUITE` the suite
//! folder, else the one the binary sits in). `BFBC2_CLI_TRACE=<file>` appends `<tool> rust` or `<tool> python` to the file (the tests count who
//! answered).

use std::fmt::Write as _;
use std::fs::OpenOptions;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::pystdio::{self, Codec};
use crate::{clock, pyfmt, pylog};

/// A Python exception: its type's name, `str()` of it, its `filename` (an `OSError`'s) and a tool error's
/// `what_to_do`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PyErr {
    pub kind: String,
    pub msg: String,
    pub filename: Option<String>,
    pub what_to_do: Option<String>,
}

impl PyErr {
    pub fn new(kind: &str, msg: impl Into<String>) -> Self {
        Self {
            kind: kind.to_owned(),
            msg: msg.into(),
            filename: None,
            what_to_do: None,
        }
    }

    /// `FileNotFoundError(path)` as the tools raise it themselves (`str()` the path, no filename).
    pub fn file_not_found(path: &str) -> Self {
        Self::new("FileNotFoundError", path)
    }

    /// The last line of its traceback: `Kind: message`, or `Kind` alone.
    pub fn line(&self) -> String {
        if self.msg.is_empty() {
            self.kind.clone()
        } else {
            format!("{}: {}", self.kind, self.msg)
        }
    }
}

/// How a run ends other than with an exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fail {
    /// The exception the Python command line raises at this point.
    Err(PyErr),
    /// The Rust core refuses, or the Python's answer here is its own (an exception text only Python makes): the
    /// whole command line is handed to the Python.
    Python,
}

impl From<PyErr> for Fail {
    fn from(e: PyErr) -> Self {
        Self::Err(e)
    }
}

pub type PyResult<T> = Result<T, Fail>;

/// The `OSError` Python's `open()` (and the `os` calls on a file) raises for `path`: `[Errno 2] No such file or
/// directory: 'path'` and its kin; an error whose text only Python knows hands off.
pub fn os_error(path: &str, e: &io::Error) -> Fail {
    let (kind, errno, text) = match e.kind() {
        io::ErrorKind::NotFound => ("FileNotFoundError", 2, "No such file or directory"),
        io::ErrorKind::PermissionDenied => ("PermissionError", 13, "Permission denied"),
        io::ErrorKind::InvalidFilename | io::ErrorKind::InvalidInput => ("OSError", 22, "Invalid argument"),
        _ => return Fail::Python,
    };
    Fail::Err(PyErr {
        kind: kind.to_owned(),
        msg: format!("[Errno {errno}] {text}: {}", pyfmt::str_repr(path)),
        filename: Some(path.to_owned()),
        what_to_do: None,
    })
}

/// `open(path, 'rb').read()`.
pub fn read_file(path: &str) -> PyResult<Vec<u8>> {
    std::fs::read(path).map_err(|e| os_error(path, &e))
}

/// `open(path, 'wb').write(data)`.
pub fn write_file(path: &str, data: &[u8]) -> PyResult<()> {
    std::fs::write(path, data).map_err(|e| os_error(path, &e))
}

/// `errors.explain(e)`: (the cause, what to do), in words.
pub fn explain(e: &PyErr) -> (String, String) {
    let s = e.msg.clone();
    let low = s.to_lowercase();
    if let Some(w) = e.what_to_do.as_ref().filter(|w| !w.is_empty()) {
        return (s, w.clone());
    }
    if e.kind == "ModuleNotFoundError" {
        let name = e.filename.clone().unwrap_or_default();
        return (
            format!("A Python library the tool needs is not installed ({name})."),
            format!("Install it: pip install {name}"),
        );
    }
    if low.contains("opengl") || low.contains("glfw") {
        return (
            "The 3D view could not start OpenGL 3.3 (the graphics driver is too old, or Python runs on a GPU \
             without it)."
                .to_owned(),
            "Update the graphics driver; on a laptop with two GPUs set python.exe to \"High performance\" in \
             Windows Settings > System > Display > Graphics."
                .to_owned(),
        );
    }
    if e.kind == "GameCopyNotFound"
        || low.contains("no game copy")
        || (low.contains("project opfor") && low.contains("not found"))
    {
        return (
            "The tools could not find Project OPFOR BFBC2's game copy.".to_owned(),
            "Put the suite folder next to the \"project-opfor-bfbc2\" folder, or set OPFOR_ROOT to it.".to_owned(),
        );
    }
    if low.contains("not prepared") || low.contains("unknown level") {
        return (
            format!("The level is not available: {s}"),
            "Choose a level from the list, or prepare it first (Levels tab: Prepare level).".to_owned(),
        );
    }
    let file = || {
        e.filename
            .clone()
            .filter(|f| !f.is_empty())
            .unwrap_or_else(|| s.clone())
    };
    if e.kind == "PermissionError" {
        return (
            format!("A file could not be written or read: {}", file()),
            "Close the program using it (the game, an editor), or check the folder is not read-only.".to_owned(),
        );
    }
    if e.kind == "FileNotFoundError" {
        return (
            format!("A file the tool needs is missing: {}", file()),
            "Check the path; if it is a level's file, prepare the level again.".to_owned(),
        );
    }
    if e.kind == "MemoryError" {
        return (
            "The PC ran out of memory.".to_owned(),
            "Close other programs, or open a smaller level.".to_owned(),
        );
    }
    (
        format!("{}: {s}", e.kind),
        "If it happens again, send the log file to the tool's author.".to_owned(),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stream {
    Out,
    Err,
}

/// Python's chunk / buffer size (`_io.TextIOWrapper._CHUNK_SIZE`, `io.DEFAULT_BUFFER_SIZE`).
const CHUNK: usize = 8192;

/// One run of a tool: its printed bytes in the order they reach the handles, and its log records.
#[allow(clippy::struct_excessive_bools)] // Python's own stream switches, one each
pub struct Py {
    tool: &'static str,
    events: Vec<(Stream, Vec<u8>)>,
    pending: Vec<u8>,
    buffer: Vec<u8>,
    err_pending: Vec<u8>,
    codec: Codec,
    out_console: bool,
    err_console: bool,
    unbuffered: bool,
    out_replace: bool,
    log_path: Option<String>,
    log: Vec<(String, &'static str, String)>,
}

impl Py {
    fn new(tool: &'static str, log: Option<&str>) -> Self {
        let mut py = Self {
            tool,
            events: Vec::new(),
            pending: Vec::new(),
            buffer: Vec::new(),
            err_pending: Vec::new(),
            codec: pystdio::codec(),
            out_console: pystdio::is_console(&io::stdout()),
            err_console: pystdio::is_console(&io::stderr()),
            unbuffered: std::env::var("PYTHONUNBUFFERED").is_ok_and(|v| !v.is_empty()),
            out_replace: false,
            log_path: None,
            log: Vec::new(),
        };
        if let Some(name) = log {
            py.log_path = pylog::log_path(name).map(|p| p.replace('/', "\\"));
            py.log("INFO", format!("--- {name} started (bfbc2.exe, win32) ---"));
        }
        py
    }

    /// `sys.stdout.reconfigure(errors='replace')`: characters the code page lacks print as `?`.
    pub fn stdout_replace(&mut self) {
        self.flush_out();
        self.out_replace = true;
    }

    /// A log record (`logging` at `level`), stamped now.
    pub fn log(&mut self, level: &'static str, message: String) {
        if self.log_path.is_some() {
            self.log.push((clock::now().stamp(), level, message));
        }
    }

    fn encode(&self, text: &str, stream: Stream) -> Result<Vec<u8>, PyErr> {
        let console = match stream {
            Stream::Out => self.out_console,
            Stream::Err => self.err_console,
        };
        let t = text.replace('\n', "\r\n");
        if console || self.codec != Codec::Cp1252 || t.is_ascii() {
            return Ok(t.into_bytes());
        }
        if stream == Stream::Out && self.out_replace {
            return Ok(t.chars().map(|c| pystdio::cp1252(c).unwrap_or(b'?')).collect());
        }
        pystdio::encode(&t, self.codec, stream == Stream::Err)
            .map_err(|u| PyErr::new("UnicodeEncodeError", u.message()))
    }

    /// `sys.stdout.write(text)`.
    pub fn write(&mut self, text: &str) -> PyResult<()> {
        if text.is_empty() {
            return Ok(());
        }
        let b = self.encode(text, Stream::Out)?;
        if self.out_console || self.unbuffered {
            self.events.push((Stream::Out, b));
            return Ok(());
        }
        // measured on 3.13 (a recording BufferedWriter's raw writes, and the order on a real pipe): a write that takes
        // the pending bytes past the chunk goes on together with them (one buffered write of both), not after them
        self.pending.extend_from_slice(&b);
        if self.pending.len() >= CHUNK {
            self.flush_pending();
        }
        Ok(())
    }

    /// `print(text)`.
    pub fn print(&mut self, text: &str) -> PyResult<()> {
        self.write(text)?;
        self.write("\n")
    }

    /// `print(text, file=sys.stderr)` (line-buffered: it leaves at its newline).
    pub fn eprint(&mut self, text: &str) {
        for part in [text, "\n"] {
            let b = self.encode(part, Stream::Err).unwrap_or_default();
            self.err_pending.extend_from_slice(&b);
            if part.contains('\n') && !self.err_pending.is_empty() {
                let p = std::mem::take(&mut self.err_pending);
                self.events.push((Stream::Err, p));
            }
        }
    }

    /// The text layer's pending bytes into the buffered writer (`_textiowrapper_writeflush`).
    fn flush_pending(&mut self) {
        let p = std::mem::take(&mut self.pending);
        if p.is_empty() {
            return;
        }
        if p.len() <= CHUNK - self.buffer.len() {
            self.buffer.extend_from_slice(&p);
            return;
        }
        self.flush_buffer();
        if p.len() > CHUNK {
            self.events.push((Stream::Out, p));
        } else {
            self.buffer = p;
        }
    }

    fn flush_buffer(&mut self) {
        let b = std::mem::take(&mut self.buffer);
        if !b.is_empty() {
            self.events.push((Stream::Out, b));
        }
    }

    /// `sys.stdout.flush()`.
    pub fn flush_out(&mut self) {
        self.flush_pending();
        self.flush_buffer();
    }

    /// `sys.stdout.flush()` with the bytes written to the handles now (round O4: `print(..., flush=True)` of a tool
    /// whose reader follows its progress - the editor's prepare view). After it the run cannot hand off to the
    /// Python (what it printed is out): a tool calls it only once it is past every [`Fail::Python`].
    pub fn flush_now(&mut self) {
        self.flush_out();
        if !self.err_pending.is_empty() {
            let p = std::mem::take(&mut self.err_pending);
            self.events.push((Stream::Err, p));
        }
        write_events(&std::mem::take(&mut self.events));
    }

    /// `errors.report(e)`: the error logged and told on stderr.
    fn report(&mut self, e: &PyErr) {
        let (cause, fix) = explain(e);
        let tool = self.tool;
        self.log(
            "ERROR",
            format!(
                "{tool} error: {cause}\nTraceback (most recent call last):\n  (bfbc2.exe: no Python frames)\n{}\n",
                e.line()
            ),
        );
        let path = self.log_path.clone().unwrap_or_else(|| "None".to_owned());
        self.eprint(&format!("error: {cause}\n  what to do: {fix}\n  details: {path}"));
    }

    /// The end of the process: stdout flushed, everything written to the handles in order, the log appended.
    fn finish(mut self) {
        self.flush_out();
        if !self.err_pending.is_empty() {
            let p = std::mem::take(&mut self.err_pending);
            self.events.push((Stream::Err, p));
        }
        write_events(&self.events);
        if let Some(path) = &self.log_path {
            let mut text = String::new();
            for (stamp, level, message) in &self.log {
                let _ = writeln!(text, "{stamp} {level:<7} {message}");
            }
            if let Ok(mut f) = OpenOptions::new().append(true).create(true).open(path) {
                let _ = f.write_all(text.replace('\n', "\r\n").as_bytes());
            }
        }
    }
}

/// The recorded writes to the handles, in order.
fn write_events(events: &[(Stream, Vec<u8>)]) {
    let stdout = io::stdout();
    let stderr = io::stderr();
    for (s, b) in events {
        let _ = match s {
            Stream::Out => {
                let mut h = stdout.lock();
                h.write_all(b).and_then(|()| h.flush())
            }
            Stream::Err => {
                let mut h = stderr.lock();
                h.write_all(b).and_then(|()| h.flush())
            }
        };
    }
}

/// A ported Python command line: the name `bfbc2 <name>` takes, its script (relative to the suite), the log
/// `errors.setup` names (None: the script has none, nor a guard) and the port.
pub struct Tool {
    pub name: &'static str,
    pub script: &'static str,
    pub log: Option<&'static str>,
    pub run: fn(&mut Py, &[String]) -> PyResult<i32>,
}

fn trace(tool: &str, who: &str) {
    if let Some(p) = std::env::var_os("BFBC2_CLI_TRACE").filter(|p| !p.is_empty())
        && let Ok(mut f) = OpenOptions::new().append(true).create(true).open(p)
    {
        let _ = writeln!(f, "{tool} {who}");
    }
}

/// The suite whose Python answers a hand-off: `BFBC2_CLI_SUITE` (the tests' reference copy), else the first folder
/// above the binary with `suite_paths.py`, else the checkout it was built in.
pub fn suite_root() -> PathBuf {
    if let Some(p) = std::env::var_os("BFBC2_CLI_SUITE").filter(|p| !p.is_empty()) {
        return PathBuf::from(p);
    }
    if let Ok(exe) = std::env::current_exe() {
        for dir in exe.ancestors().skip(1) {
            if dir.join("suite_paths.py").is_file() {
                return dir.to_path_buf();
            }
        }
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// The Python that runs the reference: `BFBC2_PYTHON`, else the first `python.exe` on `PATH`.
fn python() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("BFBC2_PYTHON").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(if cfg!(windows) { "python.exe" } else { "python3" }))
        .find(|p| p.is_file())
}

/// The command line handed to the Python: its exit code.
fn hand_off(tool: &Tool, args: &[String]) -> i32 {
    trace(tool.name, "python");
    let script = suite_root().join(tool.script);
    let Some(py) = python() else {
        pystdio::emit(
            "",
            &format!(
                "bfbc2: {} needs Python here (BFBC2_PYTHON, or python.exe on PATH)\n",
                tool.name
            ),
        );
        return 1;
    };
    match std::process::Command::new(py).arg(&script).args(args).status() {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) => {
            pystdio::emit("", &format!("bfbc2: could not start Python for {}: {e}\n", tool.name));
            1
        }
    }
}

fn pure_python() -> bool {
    std::env::var("BFBC2_PURE_PYTHON").is_ok_and(|v| !v.is_empty() && v != "0")
}

/// Runs a ported command line: the port, or the Python where it hands off (and under `BFBC2_PURE_PYTHON`).
pub fn run(tool: &Tool, args: &[String]) -> ExitCode {
    let code = if pure_python() {
        hand_off(tool, args)
    } else {
        let mut py = Py::new(tool.log.unwrap_or(tool.name), tool.log);
        match (tool.run)(&mut py, args) {
            Ok(code) => {
                trace(tool.name, "rust");
                py.finish();
                code
            }
            Err(Fail::Err(e)) => {
                trace(tool.name, "rust");
                if tool.log.is_some() {
                    py.report(&e);
                } else {
                    // an uncaught exception: the interpreter flushes stdout (flush_io) before printing it
                    py.flush_out();
                    py.eprint(&format!("Traceback (most recent call last):\n{}", e.line()));
                }
                py.finish();
                1
            }
            Err(Fail::Python) => hand_off(tool, args),
        }
    };
    ExitCode::from(u8::try_from(code & 0xFF).unwrap_or(1))
}
