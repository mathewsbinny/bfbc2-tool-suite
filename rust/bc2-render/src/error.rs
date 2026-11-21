//! The renderer's errors (round OP1, audit AU1 D13 / AU2 Q3): what failed, by kind, instead of a bare `String`. The
//! text of each stays the words the editor shows ([`std::fmt::Display`]); the kind lets the editor tell a device that
//! went away (load the view anew) from a level whose data does not read (say why, keep the simplified view) from a
//! shader the device refused.

use thiserror::Error;

/// Why the renderer could not do something.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum RenderError {
    /// Direct3D 11 refused a call, or no device could be made.
    #[error("{0}")]
    Device(String),
    /// The Direct3D device was removed (a driver reset, update or GPU switch): it cannot draw again; the view is
    /// loaded anew ([`crate::d3d`]'s `DEVICE_REMOVED`).
    #[error("{0}")]
    DeviceRemoved(String),
    /// A game file the level needs does not read: missing from its bundles, damaged, or of a layout not known.
    #[error("{0}")]
    Data(String),
    /// A shader or a shader database entry the draw needs is missing or refused.
    #[error("{0}")]
    Shader(String),
    /// Not possible here (another platform, a level without the data the request names).
    #[error("{0}")]
    Unsupported(String),
}

impl RenderError {
    /// Whether the device went away: the view must be loaded anew.
    #[must_use]
    pub fn is_device_lost(&self) -> bool {
        matches!(self, Self::DeviceRemoved(_))
    }
}

impl From<RenderError> for String {
    fn from(e: RenderError) -> Self {
        e.to_string()
    }
}

/// The result type of the renderer.
pub type Result<T, E = RenderError> = std::result::Result<T, E>;
