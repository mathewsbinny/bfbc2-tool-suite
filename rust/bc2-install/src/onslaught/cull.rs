//! Stage 11 of the console Onslaught port (round OR12): the port of Project OPFOR's `console_cull.py`'s `build` and
//! `apply`'s pack - places in the level's static cull tree for the placed objects the console layout has and the
//! PC / Linux world does not (and for objects the port left on another object's slot, more than [`MISPLACED`] metres
//! from where the blueprint's other instances put them).
//!
//! The tree's leaves are built by the suite's `bfbc2 culltree add --auto` (`bc2_levelwrite::culltool`), run as the
//! command line the caller names (`console_common.cli_command('culltree')`, as the Python ran it): the cull tool's
//! library lives in bc2-levelwrite, which depends on this crate, so it cannot be called in process from here.
//!
//! Two calls, as the Python's order of work and ledger is: [`build`] (the unpack and the tool's run: `--plan`, and
//! `--apply`'s first half; its edits say whether anything is to add), then - after OPFOR's ledger check - [`pack`]
//! (the edited partitions into the unpack, the manifest's sizes, the bundle packed). The work folder's files are the
//! Python's byte for byte (rust/BENCHMARKS.md "OR12").

use super::port::{StageOut, read_text_utf8, write_text_utf8};
use super::{OnsError, R, Source, copy, fbrb, join, joins, pack as fbrb_pack, rmtree, size, write};

/// Metres: a placed object whose leaves sit farther than this from its blueprint's shape gets new leaves (a correct
/// slot's leaves are within 0.1 m: 99 % of 2,493 leaves).
pub const MISPLACED: f64 = 2.0;
/// `str(MISPLACED)` as the command line is given it.
const MISPLACED_ARG: &str = "2.0";

/// The console level -> the PC map it is built on (`console_common.PC_MAP`).
pub const PC_MAP: [(&str, &str); 4] = [
    ("ons_mp_002", "mp_002"),
    ("ons_mp_004", "mp_004"),
    ("ons_mp_005", "mp_005"),
    ("ons_mp_008", "mp_008"),
];

/// What stage 11 is given.
#[derive(Debug, Clone)]
pub struct Args {
    /// The OPFOR folder: its `work\console_cull\` is written, `work\levels\*` and the Linux level are read.
    pub root: String,
    /// The game copy: read only.
    pub game: String,
    /// The Onslaught level (lower case).
    pub lv: String,
    /// The command line that runs `bfbc2 culltree` (`console_common.cli_command('culltree')`).
    pub cli: Vec<String>,
}

struct Paths {
    bundle: String,
    work: String,
    src: String,
    tree: String,
    level: String,
    out_tree: String,
    out_level: String,
    /// The two partitions' paths inside the bundle, as the manifest names them (lower case).
    rels: [String; 2],
}

fn paths(a: &Args) -> R<Paths> {
    let lv = a.lv.as_str();
    if !PC_MAP.iter().any(|(l, _)| *l == lv) {
        return Err(OnsError::Data(format!("KeyError: '{lv}'")));
    }
    let name = lv.to_uppercase();
    let work = joins(&a.root, &["work", "console_cull"]);
    let src = joins(&work, &[lv, "pkg"]);
    Ok(Paths {
        bundle: joins(&a.game, &["Package", "levels", lv, "level-00.fbrb"]),
        tree: joins(&src, &["Levels", &name, "staticculltree_exclusive.dbx"]),
        level: joins(&src, &["Levels", &format!("{name}.dbx")]),
        out_tree: joins(&work, &[lv, "tree.dbx"]),
        out_level: joins(&work, &[lv, "level.dbx"]),
        rels: [
            format!("levels/{name}/staticculltree_exclusive.dbx").to_lowercase(),
            format!("levels/{name}.dbx").to_lowercase(),
        ],
        work,
        src,
    })
}

/// `console_common.linux_level(lv)`: the PC map's Linux server level stage 2 unpacked.
fn linux_level(root: &str, lv: &str) -> Option<String> {
    let pc = PC_MAP.iter().find(|(l, _)| *l == lv)?.1;
    Some(joins(
        root,
        &[
            "work",
            "onslaught",
            &format!("linux_{pc}"),
            "level",
            "Levels",
            &format!("{}.dbx", pc.to_uppercase()),
        ],
    ))
}

/// `sorted(glob.glob(work\levels\*))`'s folders: the unpacked stock levels (glob leaves out names starting with a
/// dot; the paths sorted as Python sorts strings).
fn ref_dirs(root: &str) -> Vec<String> {
    let base = joins(root, &["work", "levels"]);
    let Ok(rd) = std::fs::read_dir(&base) else {
        return Vec::new();
    };
    let mut out: Vec<String> = rd
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .map(|n| join(&base, &n))
        .collect();
    out.sort();
    out.retain(|d| std::path::Path::new(d).is_dir());
    out
}

/// A tool's output as Python's `text=True` reads it: universal newlines.
fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).replace("\r\n", "\n").replace('\r', "\n")
}

/// `run(*args)`: the program run, its stdout; a failure as the Python's `SystemExit`.
fn run(args: &[String]) -> R<String> {
    let (prog, rest) = args
        .split_first()
        .ok_or_else(|| OnsError::Data("IndexError: list index out of range".into()))?;
    let o = std::process::Command::new(prog)
        .args(rest)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| OnsError::Io(format!("{prog}: {e}")))?;
    let (out, err) = (text(&o.stdout), text(&o.stderr));
    if !o.status.success() {
        return Err(OnsError::Exit(format!("failed: {}\n{out}\n{err}", args.join(" "))));
    }
    Ok(out)
}

/// `console_cull.build(lv)`: the installed bundle unpacked into `work\console_cull\<lv>\pkg`, `culltree add --auto
/// --misplaced 2.0` run on its exclusive tree and level, its report kept (`add_report.txt`). The edits to make
/// (the unpack's partition, the built file), none when nothing is to add.
pub fn build(a: &Args, out: &mut StageOut) -> R<Vec<(String, String)>> {
    let p = paths(a)?;
    rmtree(&p.src)?;
    fbrb(&["unpack", &p.bundle, &p.src])?;
    let mut refs: Vec<String> = Vec::new();
    if let Some(r) = linux_level(&a.root, &a.lv).filter(|r| std::path::Path::new(r).is_file()) {
        refs.extend(["--ref".to_owned(), r]);
    }
    for d in ref_dirs(&a.root) {
        refs.extend(["--ref-dir".to_owned(), d]);
    }
    for f in [&p.out_tree, &p.out_level] {
        if std::path::Path::new(f).is_file() {
            std::fs::remove_file(f).map_err(|e| super::io_err(f, &e))?;
        }
    }
    let mut cmd = a.cli.clone();
    cmd.extend(
        ["add", &p.tree, &p.level, "--auto", "--misplaced", MISPLACED_ARG]
            .iter()
            .map(|s| (*s).to_owned()),
    );
    cmd.extend(refs);
    cmd.extend(
        ["--out-tree", &p.out_tree, "--out-level", &p.out_level]
            .iter()
            .map(|s| (*s).to_owned()),
    );
    let report = run(&cmd)?;
    let lines = bc2_pyrt::py::splitlines(bc2_pyrt::py::strip(&report));
    out.printed.push(format!(
        "{}: {} ... {}",
        a.lv,
        lines.first().copied().unwrap_or(""),
        lines.last().copied().unwrap_or("")
    ));
    write_text_utf8(&joins(&p.work, &[&a.lv, "add_report.txt"]), &report)?;
    if !std::path::Path::new(&p.out_tree).is_file() {
        return Ok(Vec::new());
    }
    Ok(vec![(p.tree, p.out_tree), (p.level, p.out_level)])
}

/// `console_cull.apply`'s second half, after the ledger's check: the built tree and level copied into the unpack,
/// their manifest sizes, the bundle packed (`work\console_cull\<lv>\level-00.fbrb`). The game copy's write.
pub fn pack(a: &Args, out: &mut StageOut) -> R<()> {
    let p = paths(a)?;
    let man = join(&p.src, "fbrb.manifest");
    let mut lines: Vec<String> = bc2_pyrt::py::splitlines(&read_text_utf8(&man)?)
        .into_iter()
        .map(str::to_owned)
        .collect();
    for ((dst, new), rel) in [(&p.tree, &p.out_tree), (&p.level, &p.out_level)]
        .into_iter()
        .zip(&p.rels)
    {
        copy(new, dst)?;
        let sz = size(dst)?;
        for line in &mut lines {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() == 5 && parts.first().is_some_and(|f| f.to_lowercase() == *rel) {
                *line = format!("{}|{sz}|{sz}", parts.get(..3).unwrap_or_default().join("|"));
            }
        }
    }
    write(&man, (lines.join("\n") + "\n").as_bytes())?;
    let built = joins(&p.work, &[&a.lv, "level-00.fbrb"]);
    fbrb_pack(&p.src, &built)?;
    out.writes.push((p.bundle, Source::File(built)));
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn levels_and_paths() {
        let a = Args {
            root: "r".into(),
            game: "g".into(),
            lv: "ons_mp_008".into(),
            cli: Vec::new(),
        };
        let p = paths(&a).unwrap();
        assert_eq!(
            p.tree,
            r"r\work\console_cull\ons_mp_008\pkg\Levels\ONS_MP_008\staticculltree_exclusive.dbx"
        );
        assert_eq!(p.rels[1], "levels/ons_mp_008.dbx");
        assert_eq!(
            linux_level("r", "ons_mp_008").unwrap(),
            r"r\work\onslaught\linux_mp_008\level\Levels\MP_008.dbx"
        );
        assert!(
            paths(&Args {
                lv: "mp_001".into(),
                ..a
            })
            .is_err()
        );
        assert_eq!(text(b"a\r\nb\rc"), "a\nb\nc");
        assert!(run(&[]).is_err());
    }
}
