//! The install ledger as one call (round OR15, audit AU5-ARC-3): what the two retired Python implementations did,
//! answered by [`bc2_gamefs::ledger`] and [`crate::stages`] - `bc2_native.ledger_call` for Python
//! (`bfbc2core.gameledger`, Project OPFOR's `opforcore.stages`); `bfbc2 gameledger` is the command line.
//!
//! [`call`] takes the game copy, the ledger folder (None: `<game>\opfor\ledger`), an operation, its arguments as JSON
//! and the bytes of the files it installs, and answers JSON:
//!
//! ```text
//! {"ok": true,  "result": ..., "log": [lines]}
//! {"ok": false, "error": "refused" | "locked" | "damaged" | "io" | "crashed" | "stop", "message": "...",
//!  "conflicts": [...], "log": [lines]}
//! ```
//!
//! Operations of a stage (`opforcore.stages.Stages`: an interrupted commit rolled back first, old markers refused):
//! `resume`, `rel`, `owners`, `is_top`, `before`, `original`, `before_path`, `restated`, `installed`, `source`,
//! `check_install`, `install`, `check_revert`, `revert`, `restate`, `forget`, `forget_under`, `status`,
//! `legacy_markers`; the one-time migration of the markers from before the ledger (`opforcore.migrate`, the mod
//! manager's `bc2_install::mods::migrate`): `migrate_plan`, `migrate_apply`. Operations of the ledger itself (`bfbc2core.gameledger.Ledger`): `ledger_install`,
//! `ledger_uninstall`, `ledger_holders`, `ledger_owner_files`, `ledger_original`, `ledger_before`, `ledger_status`,
//! `ledger_entries`, `ledger_recover`, `ledger_adopt_legacy`, `ledger_legacy_markers`, `ledger_check`,
//! `ledger_forget`, `ledger_records`.

use std::path::Path;

use bc2_gamefs::ledger::{Ledger, LedgerError, Legacy, Plan};
use serde_json::{Value, json};

use crate::stages::{Stages, Steps, Stop};

/// A failed operation: its kind and message, and the conflicts of a refusal.
struct Fail(&'static str, String, Vec<String>);

impl From<LedgerError> for Fail {
    fn from(e: LedgerError) -> Self {
        match e {
            LedgerError::Refused(c) => Self("refused", c.join("; "), c),
            LedgerError::Locked(m) => Self("locked", m, Vec::new()),
            LedgerError::Damaged(m) => Self("damaged", m, Vec::new()),
            LedgerError::Io(m) => Self("io", m, Vec::new()),
            LedgerError::Crashed => Self("crashed", "stopped by the test hook".to_owned(), Vec::new()),
        }
    }
}

fn stop(m: Stop) -> Fail {
    Fail("stop", m, Vec::new())
}

fn plan_json(p: &Plan) -> Value {
    json!({ "owner": p.owner, "action": p.action, "writes": p.writes, "notes": p.notes, "applied": p.applied })
}

fn arg<'a>(a: &'a Value, k: &str) -> Result<&'a str, Fail> {
    a.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| Fail("stop", format!("ledger call: {k:?} is needed"), Vec::new()))
}

fn flag(a: &Value, k: &str, default: bool) -> bool {
    a.get(k).and_then(Value::as_bool).unwrap_or(default)
}

fn strings(a: &Value, k: &str) -> Option<Vec<String>> {
    a.get(k)
        .and_then(Value::as_array)
        .map(|v| v.iter().filter_map(Value::as_str).map(str::to_owned).collect())
}

fn files(a: &Value, k: &str, datas: &[Vec<u8>]) -> Result<Vec<(String, Vec<u8>)>, Fail> {
    let names = strings(a, k).unwrap_or_default();
    if names.len() != datas.len() {
        return Err(stop(format!(
            "ledger call: {} file names for {} contents",
            names.len(),
            datas.len()
        )));
    }
    Ok(names.into_iter().zip(datas.iter().cloned()).collect())
}

/// Runs one operation; the answer as JSON (see the module's notes).
#[must_use]
pub fn call(game: &str, root: Option<&str>, op: &str, args: &Value, datas: &[Vec<u8>]) -> Value {
    let mut log: Vec<String> = Vec::new();
    let r = {
        let mut say = |l: &str| log.push(l.to_owned());
        run(game, root, op, args, datas, &mut say)
    };
    match r {
        Ok(v) => json!({ "ok": true, "result": v, "log": log }),
        Err(Fail(kind, message, conflicts)) => {
            json!({ "ok": false, "error": kind, "message": message, "conflicts": conflicts, "log": log })
        }
    }
}

fn ledger_of(game: &str, root: Option<&str>) -> Ledger {
    match root {
        Some(r) => Ledger::with_root(Path::new(game), Path::new(&crate::stages::abspath(r))),
        None => Ledger::new(Path::new(game)),
    }
}

fn opt_path(p: Option<std::path::PathBuf>) -> Value {
    p.map_or(Value::Null, |p| json!(p.to_string_lossy()))
}

#[allow(clippy::too_many_lines, reason = "one arm per operation")]
fn run(
    game: &str,
    root: Option<&str>,
    op: &str,
    a: &Value,
    datas: &[Vec<u8>],
    log: &mut dyn FnMut(&str),
) -> Result<Value, Fail> {
    if let Some(lop) = op.strip_prefix("ledger_") {
        return run_ledger(game, root, lop, a, datas);
    }
    if op == "before_path" {
        return Ok(json!(Stages::before_path(arg(a, "path")?, arg(a, "owner")?)));
    }
    if op == "migrate_plan" {
        let fps = crate::mods::migrate::plan(Path::new(game)).map_err(|e| stop(e.to_string()))?;
        let refused = fps.iter().filter(|f| !f.refused.is_empty()).count();
        return Ok(json!({ "lines": crate::mods::migrate::show(&fps), "files": fps.len(), "refused": refused }));
    }
    if op == "migrate_apply" {
        let mgr = crate::mods::Manager::new(Path::new(game));
        let (moved, refused, lines) = crate::mods::migrate::apply(&mgr).map_err(|e| stop(e.to_string()))?;
        return Ok(json!({ "moved": moved, "refused": refused, "lines": lines }));
    }
    if op == "legacy_markers" {
        return Ok(json!(crate::run::legacy_markers(Path::new(arg(a, "path")?))));
    }
    let mut st = match root {
        Some(r) => Stages::with_root(game, r, Steps::from_env()),
        None => Stages::new(game, Steps::from_env()),
    };
    let paths = strings(a, "paths");
    let refs: Option<Vec<&str>> = paths.as_ref().map(|p| p.iter().map(String::as_str).collect());
    match op {
        "resume" => Ok(json!(st.resume(log).map_err(stop)?)),
        "rel" => Ok(json!(st.rel(arg(a, "path")?).map_err(stop)?)),
        "owners" => Ok(json!(st.owners(arg(a, "path")?).map_err(stop)?)),
        "is_top" => Ok(json!(st.is_top(arg(a, "owner")?, arg(a, "path")?).map_err(stop)?)),
        "before" => Ok(json!(st.before(arg(a, "owner")?, arg(a, "path")?).map_err(stop)?)),
        "original" => Ok(json!(st.original(arg(a, "path")?).map_err(stop)?)),
        "restated" => Ok(json!(st.restated(arg(a, "owner")?, arg(a, "path")?).map_err(stop)?)),
        "installed" => Ok(json!(st.installed(arg(a, "owner")?).map_err(stop)?)),
        "source" => Ok(json!(st.source(arg(a, "owner")?, arg(a, "path")?, log).map_err(stop)?)),
        "check_install" => {
            let r = refs.unwrap_or_default();
            Ok(json!(st.check_install(arg(a, "owner")?, &r, log).map_err(stop)?))
        }
        "install" => {
            let fs_ = files(a, "paths", datas)?;
            let p = st
                .install_with(
                    arg(a, "owner")?,
                    &fs_,
                    flag(a, "derived", true),
                    flag(a, "keep_others", true),
                    a.get("note").and_then(Value::as_str).unwrap_or(""),
                    flag(a, "dry_run", false),
                    log,
                )
                .map_err(stop)?;
            Ok(plan_json(&p))
        }
        "check_revert" => Ok(json!(st.check_revert(arg(a, "owner")?, refs.as_deref(), log))),
        "revert" => {
            let owner = arg(a, "owner")?;
            let mut plan = Plan {
                owner: owner.to_owned(),
                action: "uninstall".to_owned(),
                ..Plan::default()
            };
            let held = st.installed(owner).map_err(stop)?;
            let mine: Vec<String> = refs
                .as_ref()
                .map_or_else(|| held.clone(), |r| r.iter().filter_map(|p| st.rel(p).ok()).collect());
            if held.is_empty() || !mine.iter().any(|m| held.iter().any(|h| h.eq_ignore_ascii_case(m))) {
                plan.notes.push("not installed".to_owned());
                // the markers and an interrupted commit are still looked at, as OPFOR's revert does
                let check: Vec<&str> = refs.clone().unwrap_or_default();
                st.ready(&check, log).map_err(stop)?;
                return Ok(plan_json(&plan));
            }
            if flag(a, "dry_run", false) {
                let c = st.check_revert(owner, refs.as_deref(), log);
                if !c.is_empty() {
                    return Err(Fail("stop", c.join("; "), c));
                }
            } else {
                st.revert(owner, refs.as_deref(), log).map_err(stop)?;
                plan.applied = true;
            }
            plan.writes = mine;
            Ok(plan_json(&plan))
        }
        "restate" => {
            let prior = if flag(a, "keep_prior", true) {
                None
            } else if flag(a, "prior_none", false) {
                Some(None)
            } else {
                Some(datas.first().map(Vec::as_slice))
            };
            st.restate(
                arg(a, "owner")?,
                arg(a, "path")?,
                prior,
                a.get("why").and_then(Value::as_str).unwrap_or(""),
                log,
            )
            .map_err(stop)?;
            Ok(Value::Null)
        }
        "forget" => Ok(plan_json(&st.forget(arg(a, "owner")?, log).map_err(stop)?)),
        "forget_under" => {
            let f = strings(a, "folders").unwrap_or_default();
            let refs: Vec<&str> = f.iter().map(String::as_str).collect();
            Ok(json!(
                st.forget_under(&refs, flag(a, "dry_run", false), log).map_err(stop)?
            ))
        }
        "status" => Ok(json!(st.status(&refs.unwrap_or_default()).map_err(stop)?)),
        _ => Err(stop(format!("ledger call: unknown operation {op:?}"))),
    }
}

fn run_ledger(game: &str, root: Option<&str>, op: &str, a: &Value, datas: &[Vec<u8>]) -> Result<Value, Fail> {
    let mut led = ledger_of(game, root);
    match op {
        "install" => {
            let fs_ = files(a, "rels", datas)?;
            let p = led.install(
                arg(a, "owner")?,
                &fs_,
                flag(a, "derived", false),
                a.get("note").and_then(Value::as_str).unwrap_or(""),
                flag(a, "dry_run", false),
            )?;
            Ok(plan_json(&p))
        }
        "uninstall" => Ok(plan_json(&led.uninstall(arg(a, "owner")?, flag(a, "dry_run", false))?)),
        "holders" => Ok(json!(led.holders(arg(a, "rel")?)?)),
        "owner_files" => Ok(json!(led.owner_files(arg(a, "owner")?)?)),
        "original" => Ok(opt_path(led.original(arg(a, "rel")?)?)),
        "before" => Ok(match led.before(arg(a, "owner")?, arg(a, "rel")?)? {
            None => json!([false, null]),
            Some(p) => json!([true, opt_path(p)]),
        }),
        "status" => Ok(json!(led.status()?)),
        "entries" => Ok(Value::Array(
            led.status_entries()?
                .into_iter()
                .map(|e| {
                    json!({ "owner": e.owner, "kind": e.kind, "file": e.file, "slot": e.slot, "state": e.state,
                            "detail": e.detail })
                })
                .collect(),
        )),
        "recover" => Ok(json!(led.recover(flag(a, "force", false))?)),
        "adopt_legacy" => {
            let family = match arg(a, "family")? {
                "navkit" => Legacy::NavKit,
                "stockmp" => Legacy::StockMp,
                f => return Err(stop(format!("ledger call: unknown legacy family {f:?}"))),
            };
            let rels = strings(a, "rels").unwrap_or_default();
            Ok(led
                .adopt_legacy(arg(a, "owner")?, &rels, family)?
                .map_or(Value::Null, |p| plan_json(&p)))
        }
        "legacy_markers" => Ok(json!(bc2_gamefs::ledger::legacy_markers(Path::new(arg(a, "path")?)))),
        "check" => {
            let r = led.check()?;
            Ok(
                json!({ "healthy": r.healthy(), "lines": r.lines(), "owners": r.owners, "kinds": r.kinds,
                       "damaged": r.damaged, "blobs": r.blobs, "interrupted": r.interrupted, "lock": r.lock,
                       "kept": r.kept }),
            )
        }
        "forget" => Ok(plan_json(&led.forget(
            arg(a, "owner")?,
            flag(a, "only_missing", false),
            flag(a, "dry_run", false),
        )?)),
        "records" => Ok(Value::Object(led.records()?.into_iter().collect())),
        _ => Err(stop(format!("ledger call: unknown operation ledger_{op}"))),
    }
}
