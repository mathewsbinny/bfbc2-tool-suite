//! The editor's errors (round OP1, audit AU1 D13 / AU2 Q3): what failed, by kind, instead of a bare `String`. Each
//! keeps the words the editor shows ([`std::fmt::Display`]); the kind says where to look - the project file, the
//! level, the renderer, the GPU, what the user typed, a file, an outside tool, a worker - and what the editor does
//! about it (a lost device loads the view anew; a level that does not load goes back to the start screen).
//!
//! The problems an open project has (operations that did not apply, the base changed) are [`Problem`]s: a severity,
//! a code, the operation and objects they name, and the words.

use thiserror::Error;

use bc2_render::RenderError;

/// Why the editor could not do something.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum EditorError {
    /// The map project's file: it does not read or write, is locked by another editor, or changed on disk.
    #[error("{0}")]
    Project(String),
    /// A level does not load or prepare (its partition, bundles, unpacked files, the scene).
    #[error("{0}")]
    Level(String),
    /// The game-accurate renderer.
    #[error(transparent)]
    Render(#[from] RenderError),
    /// The editor's own GPU device (the headless device, a screenshot's read back).
    #[error("{0}")]
    Gpu(String),
    /// What was typed or picked cannot be used (a name, a height map, a value).
    #[error("{0}")]
    Input(String),
    /// A file the editor reads or writes (a side file, a picture, settings).
    #[error("{0}")]
    Io(String),
    /// An outside tool did not start or failed (NavKit, OPFOR's install script, Python).
    #[error("{0}")]
    Tool(String),
    /// A worker stopped without an answer (an internal error).
    #[error("{0}")]
    Worker(String),
}

impl EditorError {
    /// Whether the game-accurate view's device went away (the view loads anew).
    #[must_use]
    pub fn is_device_lost(&self) -> bool {
        matches!(self, Self::Render(r) if r.is_device_lost())
    }

    /// Whether its words contain `s` (tests, filters).
    #[must_use]
    pub fn contains(&self, s: &str) -> bool {
        self.to_string().contains(s)
    }
}

impl From<EditorError> for String {
    fn from(e: EditorError) -> Self {
        e.to_string()
    }
}

impl From<bc2_levelwrite::project::ProjectError> for EditorError {
    fn from(e: bc2_levelwrite::project::ProjectError) -> Self {
        Self::Project(e.to_string())
    }
}

/// How bad a problem is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Worth knowing; nothing is wrong with the edits.
    Note,
    /// The edits may not be what the project was made on (the base changed).
    Warning,
    /// An operation did not apply: the level differs from what the log says.
    Error,
}

/// What a problem is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ProblemCode {
    /// The base level's partition is not the one the project was made on.
    BaseChanged,
    /// An operation names an object the base level does not have.
    MissingObject,
    /// An operation did not apply (levelwrite's reason in the words).
    NotApplied,
}

impl ProblemCode {
    /// Its rule in the export's registry (round V1, `bc2_levelwrite::rules`).
    #[must_use]
    pub fn rule(self) -> &'static str {
        match self {
            Self::BaseChanged => "XVAL-EDIT-1",
            Self::MissingObject => "XVAL-EDIT-2",
            Self::NotApplied => "XVAL-EDIT-3",
        }
    }
}

/// A problem of the open project (round OP1: typed, for the Problems panel and, later, live validation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// How bad.
    pub severity: Severity,
    /// What about.
    pub code: ProblemCode,
    /// The operation it is about (its number), when one.
    pub operation: Option<u64>,
    /// The objects it names (guids).
    pub objects: Vec<String>,
    /// In words.
    pub text: String,
}

impl Problem {
    /// A problem in words.
    #[must_use]
    pub fn new(severity: Severity, code: ProblemCode, text: impl Into<String>) -> Self {
        Self {
            severity,
            code,
            operation: None,
            objects: Vec::new(),
            text: text.into(),
        }
    }

    /// The same, about operation `n`.
    #[must_use]
    pub fn at(mut self, n: u64) -> Self {
        self.operation = Some(n);
        self
    }

    /// The same, naming these objects.
    #[must_use]
    pub fn naming(mut self, objects: Vec<String>) -> Self {
        self.objects = objects;
        self
    }

    /// Whether its words contain `s` (tests, filters).
    #[must_use]
    pub fn contains(&self, s: &str) -> bool {
        self.text.contains(s)
    }
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}
