//! The console Onslaught port's driver (round OR13): the port of Project OPFOR's `console_port.py` - which stages a
//! level is built with, in which order, with which command lines; which stages a restart reverts first; and the
//! earlier port's registration the first stage replaces.
//!
//! The stages are OPFOR's scripts (each a wrapper over the suite's Rust core since rounds OR5 and OR9 - OR13): stage 1
//! `console_onslaught.py` (the level from the user's Xbox package and the PC map), 2 `console_linux_base.py` (on the
//! Linux server level), 3 - 14 `console_<name>.py --apply --level <lv>`; stage 8 (the Mark V) runs for Valparaiso
//! (`ons_mp_002`) only, stage 14 (the icons) for the levels [`super::icons::ICONS`] names. A restart at stage N
//! (`--from N`) first reverts the level's stages N - 14, the last first: each stage is a layer of the install ledger
//! built on the one below. OPFOR's script keeps the ledger's calls, the backup check and the runs.

use super::{OnsError, R, join, joins};

/// (stage number, the install ledger's stage name, the script).
pub const STAGES: [(u32, &str, &str); 14] = [
    (1, "onslaught", "console_onslaught.py"),
    (2, "linux", "console_linux_base.py"),
    (3, "mode", "console_mode.py"),
    (4, "loadset", "console_loadset.py"),
    (5, "vehicles", "console_vehicles.py"),
    (6, "soldiers", "console_soldiers.py"),
    (7, "shaders", "console_shaders.py"),
    (8, "markv", "console_markv.py"),
    (9, "input", "console_input.py"),
    (10, "deps", "console_deps.py"),
    (11, "cull", "console_cull.py"),
    (12, "loading", "console_loading.py"),
    (13, "hud", "console_hud.py"),
    (14, "icons", "console_icons.py"),
];

/// One stage to run: its number, and its command line (the script first), or the option it needs that was not
/// given (`opts['--xbox']`'s `KeyError` when the run reaches it).
pub type Run = (u32, Result<Vec<String>, String>);

/// What a run of the driver does, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The ledger stage names reverted first (when installed), the last stage first.
    pub reverts: Vec<String>,
    /// Stage 1 runs: the backup of Napisal's ports is checked and his registration of the level dropped first.
    pub first: bool,
    /// The stages run.
    pub runs: Vec<Run>,
}

/// `int(text)` as the Python's `--from` was read.
fn py_int(text: &str) -> R<i64> {
    let t = bc2_pyrt::py::strip(text).replace('_', "");
    let ok = !t.is_empty()
        && !text.contains("__")
        && !bc2_pyrt::py::strip(text).starts_with('_')
        && !bc2_pyrt::py::strip(text).ends_with('_');
    t.parse::<i64>().ok().filter(|_| ok).ok_or_else(|| {
        OnsError::Data(format!(
            "invalid literal for int() with base 10: {}",
            bc2_pyrt::fmt::str_repr(text)
        ))
    })
}

/// `main(argv)`'s plan: `lv` the level as given, `from` the `--from` text (1 when absent), the `--xbox` and `--linux`
/// options, `opfor` the OPFOR folder (stage 1's first argument).
pub fn plan(lv: &str, from: Option<&str>, xbox: Option<&str>, linux: Option<&str>, opfor: &str) -> R<Plan> {
    let lv = lv.to_lowercase();
    let Some((_, pc)) = super::shaders::PC_MAP.iter().find(|(l, _)| *l == lv) else {
        let all: Vec<&str> = super::shaders::PC_MAP.iter().map(|(l, _)| *l).collect();
        return Err(OnsError::Exit(format!(
            "{lv}: not a console Onslaught level ({})",
            all.join(", ")
        )));
    };
    let start = from.map_or(Ok(1), py_int)?;
    let reverts = STAGES
        .iter()
        .rev()
        .filter(|(n, _, _)| i64::from(*n) >= start)
        .map(|(_, s, _)| (*s).to_owned())
        .collect();
    let need = |o: Option<&str>, name: &str| o.map(str::to_owned).ok_or_else(|| name.to_owned());
    let mut runs: Vec<Run> = Vec::new();
    if start <= 1 {
        runs.push((
            1,
            need(xbox, "--xbox")
                .map(|x| vec!["console_onslaught.py".into(), opfor.into(), x, lv.clone(), (*pc).into()]),
        ));
    }
    if start <= 2 {
        runs.push((
            2,
            need(linux, "--linux").map(|l| {
                vec![
                    "console_linux_base.py".into(),
                    "--apply".into(),
                    lv.clone(),
                    (*pc).into(),
                    joins(&join(&l, pc), &["level-00.fbrb"]),
                ]
            }),
        ));
    }
    for (n, _, tool) in &STAGES[2..] {
        if start > i64::from(*n) {
            continue;
        }
        let args: Vec<String> = match *tool {
            // the Mark V is Valparaiso's (the tool knows its level)
            "console_markv.py" if lv == "ons_mp_002" => vec![(*tool).into(), "--apply".into()],
            "console_markv.py" => continue,
            "console_icons.py" if super::icons::ICONS.iter().all(|(l, _)| *l != lv) => continue,
            _ => vec![(*tool).into(), "--apply".into(), "--level".into(), lv.clone()],
        };
        runs.push((*n, Ok(args)));
    }
    Ok(Plan {
        reverts,
        first: start <= 1,
        runs,
    })
}

/// `package.mft` without the level's earlier registration (Napisal's port of the same name): its text (latin-1)
/// split at its blank lines, every block that starts `BundleSource game:levels/<lv>/<level|ingame|loader|terrain>-00
/// .fbrb` left out; (the text, the blocks left out).
#[must_use]
pub fn without_blocks(text: &str, lv: &str) -> (String, usize) {
    let blocks: Vec<&str> = text.split("\r\n\r\n").collect();
    let keep: Vec<&str> = blocks
        .iter()
        .copied()
        .filter(|b| {
            let s = bc2_pyrt::py::strip(b);
            !["level", "ingame", "loader", "terrain"]
                .iter()
                .any(|k| s.starts_with(&format!("BundleSource game:levels/{lv}/{k}-00.fbrb")))
        })
        .collect();
    (keep.join("\r\n\r\n"), blocks.len() - keep.len())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn stages_per_level_and_restart() {
        let p = plan("ONS_MP_002", None, Some("X"), Some(r"L\levels"), r"C:\o").unwrap();
        assert!(p.first);
        assert_eq!(p.reverts.len(), 14);
        assert_eq!(p.reverts[0], "icons");
        let ns: Vec<u32> = p.runs.iter().map(|r| r.0).collect();
        assert_eq!(ns, (1..=14).collect::<Vec<_>>());
        assert_eq!(p.runs[1].1.as_ref().unwrap()[4], r"L\levels\mp_002\level-00.fbrb");
        let p = plan("ons_mp_004", Some("7"), None, None, "o").unwrap();
        assert!(!p.first);
        assert_eq!(
            p.reverts,
            ["icons", "hud", "loading", "cull", "deps", "input", "markv", "shaders"]
        );
        let ns: Vec<u32> = p.runs.iter().map(|r| r.0).collect();
        assert_eq!(ns, [7, 9, 10, 11, 12, 13], "no Mark V, no icons on Atacama");
        let p = plan("ons_mp_005", None, None, Some("L"), "o").unwrap();
        assert_eq!(p.runs[0].1, Err("--xbox".to_owned()));
        assert!(plan("mp_001", None, None, None, "o").is_err());
        assert!(plan("ons_mp_002", Some("x"), None, None, "o").is_err());
        assert_eq!(
            plan("ons_mp_002", Some(" 1_4 "), None, None, "o").unwrap().runs.len(),
            1
        );
    }

    #[test]
    fn napisals_blocks_go() {
        let t = "BundleSource game:levels/ons_mp_002/level-00.fbrb\r\nBundleTarget x\r\n\r\nBundleSource \
                 game:levels/mp_002/level-00.fbrb\r\n\r\nBundleSource game:levels/ons_mp_002/terrain-00.fbrb\r\n";
        let (out, n) = without_blocks(t, "ons_mp_002");
        assert_eq!(n, 2);
        assert_eq!(out, "BundleSource game:levels/mp_002/level-00.fbrb");
        assert_eq!(without_blocks("a\r\n\r\nb", "ons_mp_004"), ("a\r\n\r\nb".to_owned(), 0));
    }
}
