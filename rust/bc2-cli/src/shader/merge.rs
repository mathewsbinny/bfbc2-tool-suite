//! `shaderdb.py merge`: `cmd_merge` over `bc2_convert::shadercmd::cmd_merge` (round OR10 moved the merge there - the
//! `Merger` of `bc2_formats::shaderdb::merge`, its read-back check, `verify_merge` and the report - so that Project
//! OPFOR's console port stage 7 runs it in process): the same records added in the same order, the programs re-sorted
//! and the entry groups renumbered as the reference does, the same report and messages.

use bc2_convert::shadercmd;

use super::shaderdb::{cv, with_env};
use crate::pyproc::{Py, PyResult};

/// `cmd_merge`'s arguments.
pub struct MergeArgs {
    pub base: String,
    pub donors: Vec<String>,
    pub need: String,
    pub out: String,
    pub all_names: bool,
    pub dx: Option<u32>,
    pub opfor: Option<String>,
}

/// `cmd_merge(a)`.
pub fn cmd_merge(py: &mut Py, a: &MergeArgs) -> PyResult<i32> {
    let args = shadercmd::MergeArgs {
        base: &a.base,
        donors: &a.donors,
        need: &a.need,
        out: &a.out,
        all_names: a.all_names,
        dx: a.dx,
    };
    let printed = with_env(a.opfor.as_deref(), |env| shadercmd::cmd_merge(&args, env).map_err(cv))?;
    py.print(&printed.head)?;
    py.print(&printed.summary)?;
    Ok(0)
}
