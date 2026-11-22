//! LevelDiff's command line (`LevelDiff.exe`, retired in round EX1) in process: `bfbc2.exe leveldiff`'s arguments,
//! text, JSON file and exit codes (0 no differences, 1 differences, 2 bad input, 3 an unexpected error). The
//! comparison is `bc2_formats::dbxedit::leveldiff`'s (the map editor's Diff panel's too); moved here from there in
//! round PR1b-2-rest (audit AU5 ARC-2: a command line is a tool's, the formats crate writes no file). The JSON file
//! is written through `bc2_gamefs::atomic`.

#![allow(
    clippy::many_single_char_names,
    reason = "LevelDiff's names: a / b (the files), o (the options), r (the run)"
)]

use bc2_formats::dbxedit::leveldiff::{Options, diff, json, text};
use bc2_formats::dbxedit::{EditDbx, Fault, Result, net, read};

/// The usage line (standard error, exit code 2).
pub const USAGE: &str = "usage: LevelDiff <a.dbx> <b.dbx> [--match-objects] [--type <text>] [--ignore <field>] [--summary] \
                         [--all] [--json <file>]";

/// What a run printed and its exit code: 0 no differences, 1 differences, 2 bad input, 3 an unexpected error.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Run {
    /// The exit code.
    pub code: i32,
    /// Standard output, each line ended by `\n`.
    pub stdout: String,
    /// Standard error, each line ended by `\n`.
    pub stderr: String,
    /// `(level, message)` lines for the log.
    pub log: Vec<(String, String)>,
}

/// The command line parsed: options and file names, or `None` for a usage error.
#[must_use]
pub fn parse_args(args: &[String]) -> Option<(Options, Vec<String>)> {
    let mut o = Options::default();
    let mut files = Vec::new();
    let mut missing = false;
    let mut i = 0;
    while let Some(a) = args.get(i) {
        let mut next = || {
            i += 1;
            args.get(i).cloned()
        };
        match a.as_str() {
            "--match-objects" => o.match_objects = true,
            "--summary" => o.summary = true,
            "--all" => o.all = true,
            "--type" => {
                o.type_filter = next();
                missing |= o.type_filter.is_none();
            }
            "--ignore" => match next() {
                Some(v) => {
                    o.ignore.insert(v);
                }
                None => missing = true,
            },
            "--json" => {
                o.json = next();
                missing |= o.json.is_none();
            }
            _ => files.push(a.clone()),
        }
        i += 1;
    }
    (files.len() == 2 && !missing).then_some((o, files))
}

/// LevelDiff's command line (`args` without the program name) in process: the same text, JSON file and exit code.
/// `log_path` is the log named in the error messages (`%LOCALAPPDATA%\bfbc2-tool-suite\logs\leveldiff-<date>.log`).
#[must_use]
pub fn run(args: &[String], log_path: Option<&str>) -> Run {
    let mut r = Run::default();
    let log = log_path.unwrap_or("no log");
    let fail_log = log_path.unwrap_or("(no log file)");
    let Some((o, files)) = parse_args(args) else {
        r.stderr = format!("{USAGE}\n");
        r.code = 2;
        return r;
    };
    let (Some(fa), Some(fb)) = (files.first(), files.get(1)) else {
        r.code = 2;
        return r;
    };
    let load = |p: &str| -> Result<EditDbx> { read(&net::read_bytes(p)?) };
    let (a, b) = match load(fa).and_then(|a| Ok((a, load(fb)?))) {
        Ok(x) => x,
        Err(f) => {
            let (cause, todo) = f.explain();
            r.log.push((
                "ERROR".into(),
                format!(
                    "leveldiff error in reading the files: {cause}\r\n{}: {}",
                    f.type_name(),
                    f.message
                ),
            ));
            r.stderr = format!("{}\n  what to do: {todo}  (details: {log})\n", f.message);
            r.code = 2;
            return r;
        }
    };
    if !a.is_binary() || !b.is_binary() {
        r.stderr = "both files must be binary .dbx\n".into();
        r.code = 2;
        return r;
    }
    let result = diff(&a, &b, &o).and_then(|d| {
        r.stdout = text(&d, fa, fb, &o);
        if let Some(j) = &o.json {
            bc2_gamefs::atomic::write(std::path::Path::new(j), &[json(&d, fa, fb).as_bytes()])
                .map_err(|e| Fault::io(e.io(), std::path::Path::new(j)))?;
        }
        Ok(i32::from(!d.is_empty()))
    });
    match result {
        Ok(code) => r.code = code,
        Err(f) => {
            let (cause, todo) = f.explain();
            r.log.push((
                "ERROR".into(),
                format!("leveldiff error: {cause}\r\n{}: {}", f.type_name(), f.message),
            ));
            r.stderr = format!("error: {cause}\n  what to do: {todo}\n  details: {fail_log}\n");
            r.code = 3;
        }
    }
    r
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn arguments_as_leveldiff_reads_them() {
        let a = |s: &[&str]| s.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>();
        let (o, f) = parse_args(&a(&[
            "x", "--type", "Spawn", "y", "--ignore", "A", "--ignore", "B", "--all",
        ]))
        .unwrap();
        assert_eq!(f, ["x", "y"]);
        assert_eq!(o.type_filter.as_deref(), Some("Spawn"));
        assert!(o.all && o.ignore.contains("A") && o.ignore.contains("B"));
        assert!(parse_args(&a(&["x", "y", "--type"])).is_none());
        assert!(parse_args(&a(&["x"])).is_none());
        assert!(parse_args(&a(&["x", "y", "z"])).is_none());
        // a switch's value is the next argument, whatever it is
        let (o, f) = parse_args(&a(&["--json", "--all", "x", "y"])).unwrap();
        assert_eq!(o.json.as_deref(), Some("--all"));
        assert!(!o.all && f.len() == 2);
    }
}
