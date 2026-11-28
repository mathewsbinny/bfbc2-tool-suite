//! Round OR15: the install ledger's one implementation (`bc2_install::ledgerapi`, on `bc2_gamefs::ledger` and
//! `bc2_install::stages`) for the Python that used to carry its own - `bfbc2core.gameledger` (NavKit, stock-MP,
//! texqa) and Project OPFOR's `opforcore.stages` (its `opforcore.ledger` retired):
//! - `ledger_call(game, root, op, args, datas)`: one operation; `args` JSON, `datas` the bytes of the files an
//!   install writes (in the order of its names); the answer as JSON (`ok`, `result` or `error` / `message` /
//!   `conflicts`, and `log`: the lines the operation said).

use pyo3::prelude::*;
use pyo3::types::PyBytes;

/// One ledger operation (see `bc2_install::ledgerapi`); the answer as JSON.
#[pyfunction]
#[pyo3(signature = (game, root, op, args, datas = Vec::new()))]
pub fn ledger_call(
    py: Python<'_>,
    game: &str,
    root: Option<&str>,
    op: &str,
    args: &str,
    datas: Vec<Bound<'_, PyBytes>>,
) -> PyResult<String> {
    let a: serde_json::Value = serde_json::from_str(args)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("ledger_call: args: {e}")))?;
    let owned: Vec<Vec<u8>> = datas.iter().map(|b| b.as_bytes().to_vec()).collect();
    drop(datas);
    let out = py.detach(|| bc2_install::ledgerapi::call(game, root, op, &a, &owned));
    Ok(out.to_string())
}
