//! BF1943's DBX partitions in BFBC2's schema (`bfbc2-bf1943-import/dbxport.py`'s `port`, round E4d): the template
//! choice, the alignment of each instance with its BFBC2 template, the reference rules, the converters and the
//! value passes, over trees kept here (BFBC2's template partitions and the catalogue are parsed once and stay on
//! the Rust side; the port reads the BF1943 partition from its bytes and writes the result's).
//!
//! The Python module stays the reference and the fallback: where Python's answer depends on something this port
//! does not reproduce (an exception of the reference's own other than `PortError` / `SchemaError`, a `\w` or a
//! number past what is handled), [`Fail::Unsupported`] says so and the caller ports in Python.
//!
//! - [`arena`]: the element tree (node ids standing for Python objects: shared where the reference shares).
//! - [`py`]: Python's string and number semantics the rules lean on.
//! - [`rules`]: the module's tables and small helpers.
//! - [`catalogue`]: `dbxcatalogue.Catalogue`'s tables.
//! - [`templates`]: `Templates`, `_roles`, `_inputs` and the ranking index.
//! - [`port`]: `_Port` and the value passes.

pub mod arena;
pub mod catalogue;
pub mod port;
pub mod py;
pub mod rules;
pub mod templates;

pub use arena::{Arena, Id, Node, Tn};
pub use catalogue::Catalogue;
pub use port::{Bones, CatView, Options, Ours, Ported, Record, SiKey, SiVal, port};
pub use templates::Templates;

/// Why a port did not give a partition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fail {
    /// The partition does not parse (`ValueError` with this message).
    Parse(String),
    /// The reference's `PortError` with this message.
    Port(String),
    /// The reference's `SchemaError` with these problems.
    Schema(Vec<String>),
    /// A case the reference answers otherwise than this port can: port in Python.
    Unsupported(String),
    /// The caller's `resolve` raised (the binding keeps its exception).
    Callback,
}

impl std::fmt::Display for Fail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(m) | Self::Port(m) => f.write_str(m),
            Self::Schema(p) => write!(f, "{} schema mismatch(es)", p.len()),
            Self::Unsupported(m) => write!(f, "left to the Python reference: {m}"),
            Self::Callback => f.write_str("resolve raised"),
        }
    }
}

impl std::error::Error for Fail {}
