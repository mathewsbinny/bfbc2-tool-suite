//! `bfbc2 aptgraft roundtrip|convert|deps|graft|onslaught-timer|mode-case|mode-alias|label-alias|crosshair-add ...`:
//! `bfbc2-menu-tools\aptgraft.py`'s command line over `bc2_menu::graft` (the movie model of round E4c and the grafts
//! of round O3), printing what the Python prints.
//!
//! Handed to the Python: a movie the Rust model refuses (the reference then decodes it with its own code and words
//! its own errors), numbers `int()` / `float()` would read in forms other than the plain ones, and any exception
//! other than an `AptError` (or the `OSError` of a file) the reference would raise.

#![allow(
    clippy::indexing_slicing,
    reason = "the arguments' count is checked before they are indexed"
)]

use std::fmt::Write as _;

use bc2_formats::aptmodel::{self, Layout};
use bc2_menu::graft::{
    GRAFT_LAYOUT, Grafter, Movie, apt_err, crosshair, edit, free_depths, load, load_members, place_item, py_float,
    py_int0,
};

use crate::pyfmt::{docstring, str_repr};
use crate::pyproc::{Fail, Py, PyErr, PyResult, write_file};

const SOURCE: &str = include_str!("../../../../bfbc2-menu-tools/aptgraft.py");
const COMMANDS: [&str; 9] = [
    "roundtrip",
    "convert",
    "deps",
    "graft",
    "onslaught-timer",
    "mode-case",
    "mode-alias",
    "label-alias",
    "crosshair-add",
];

/// `take(args, flag)`: the value after the flag's first occurrence, both taken out.
fn take(args: &mut Vec<String>, flag: &str) -> PyResult<Option<String>> {
    let Some(i) = args.iter().position(|a| a == flag) else {
        return Ok(None);
    };
    if i + 1 >= args.len() {
        return Err(apt_err(format!("{flag} needs a value")));
    }
    let v = args.remove(i + 1);
    args.remove(i);
    Ok(Some(v))
}

/// `take_flag(args, flag)`.
fn take_flag(args: &mut Vec<String>, flag: &str) -> bool {
    match args.iter().position(|a| a == flag) {
        Some(i) => {
            args.remove(i);
            true
        }
        None => false,
    }
}

/// `int(text)` (base 10) for plain digits.
fn py_int(s: &str) -> PyResult<i64> {
    let t = s.trim_matches(|c: char| c.is_ascii_whitespace());
    let d = t.strip_prefix(['-', '+']).unwrap_or(t);
    if d.is_empty() || !d.chars().all(|c| c.is_ascii_digit()) {
        return Err(Fail::Python);
    }
    t.parse::<i64>().map_err(|_| Fail::Python)
}

fn write_res(path: &str, data: &[u8]) -> PyResult<()> {
    write_file(path, data)
}

/// `{k: v}` as Python prints a dict of ints.
fn dict_ints(v: &[(i64, i64)]) -> String {
    let p: Vec<String> = v.iter().map(|(a, b)| format!("{a}: {b}")).collect();
    format!("{{{}}}", p.join(", "))
}

/// `movie_files(paths)`.
fn movie_files(paths: &[String]) -> PyResult<Vec<String>> {
    use bc2_pyrt::py::{isdir, isfile};
    let mut out = Vec::new();
    for p in paths {
        if isfile(p) || p.to_lowercase().contains(".fbrb:") {
            out.push(p.clone());
            continue;
        }
        if !isdir(p) {
            return Err(Fail::Err(PyErr::file_not_found(p)));
        }
        walk(p, &mut out, 0)?;
    }
    Ok(out)
}

/// One folder of `os.walk(p)` (top-down, folders sorted) as `movie_files` takes it.
fn walk(dp: &str, out: &mut Vec<String>, depth: usize) -> PyResult<()> {
    use bc2_pyrt::py::{isfile, join};
    if depth > 200 {
        return Err(Fail::Python);
    }
    let Ok(rd) = std::fs::read_dir(dp) else { return Ok(()) };
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for e in rd {
        let Ok(e) = e else { continue };
        let Some(name) = e.file_name().to_str().map(str::to_owned) else {
            return Err(Fail::Python);
        };
        let Ok(t) = e.file_type() else { continue };
        if t.is_symlink() {
            // os.walk follows a junction, lists a link to a folder without descending: left to the Python
            return Err(Fail::Python);
        }
        if t.is_dir() {
            dirs.push(name);
        } else {
            files.push(name);
        }
    }
    dirs.sort();
    if files.iter().any(|f| f == "fbrb.manifest") {
        let text = String::from_utf8_lossy(&bc2_pyrt::err::read_file(&join(dp, &["fbrb.manifest"]))?)
            .replace("\r\n", "\n")
            .replace('\r', "\n");
        for line in text.split_inclusive('\n') {
            let line = line.strip_suffix('\n').unwrap_or(line);
            let parts: Vec<&str> = line.split('|').collect();
            if !line.starts_with('#') && parts.len() > 1 && parts[1] == "SwfMovie" {
                let segs: Vec<&str> = parts[0].split('/').collect();
                let f = join(dp, &segs);
                if isfile(&f) {
                    out.push(f);
                }
            }
        }
        return Ok(());
    }
    files.sort();
    for f in &files {
        if f.to_lowercase().ends_with(".res") {
            let fp = join(dp, &[f]);
            let head = std::fs::File::open(&fp).ok().and_then(|mut fh| {
                let mut b = [0u8; 4];
                std::io::Read::read(&mut fh, &mut b).ok().map(|n| b[..n].to_vec())
            });
            match head {
                Some(h) if h == b"BIGF" => out.push(fp),
                Some(_) => {}
                None => return Err(Fail::Python),
            }
        }
    }
    for d in dirs {
        walk(&join(dp, &[&d]), out, depth + 1)?;
    }
    Ok(())
}

/// `roundtrip(spec, pack)` of one movie: (problems, notes); an `AptError` of its reading as the reference's.
fn roundtrip_one(spec: &str, pack: bool) -> PyResult<(Vec<String>, Vec<String>)> {
    let (label, members) = load_members(spec)?;
    let raw = if bc2_pyrt::py::isfile(spec) && !spec.to_lowercase().ends_with(".apt") {
        Some(bc2_pyrt::err::read_file(spec)?)
    } else {
        None
    };
    if let Ok(r) = aptmodel::roundtrip(&members, raw.as_deref(), pack) {
        return Ok(r);
    }
    // the reference's Movie(...) raises its AptError here (no .apt member ...), else its Python model runs
    Movie::new(members, &label)?;
    Err(Fail::Python)
}

fn cmd_roundtrip(py: &mut Py, mut args: Vec<String>) -> PyResult<i32> {
    let pack = take_flag(&mut args, "--pack");
    let quiet = take_flag(&mut args, "--quiet");
    let files = movie_files(&args)?;
    if files.is_empty() {
        return Err(apt_err("no movies found"));
    }
    let mut seen: std::collections::HashSet<Vec<u8>> = std::collections::HashSet::new();
    let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut todo = Vec::new();
    for f in &files {
        let fresh = if bc2_pyrt::py::isfile(f) {
            seen.insert(bc2_pyrt::err::read_file(f)?)
        } else {
            seen_names.insert(f.clone())
        };
        if fresh {
            todo.push(f.clone());
        }
    }
    let distinct = seen.len() + seen_names.len();
    let (mut bad, mut dead) = (0, 0);
    for f in &todo {
        let (probs, notes) = match roundtrip_one(f, pack) {
            Ok(r) => r,
            Err(Fail::Err(e)) if e.kind == "AptError" => (vec![e.msg], Vec::new()),
            Err(e) => return Err(e),
        };
        dead += usize::from(!notes.is_empty());
        bad += usize::from(!probs.is_empty());
        if !probs.is_empty() || !quiet {
            let mut line = format!("{} {f}", if probs.is_empty() { "ok  " } else { "FAIL" });
            for p in &probs {
                let _ = write!(line, "\n       {p}");
            }
            for n in notes.iter().take(4) {
                let _ = write!(line, "\n       note: {n}");
            }
            if notes.len() > 4 {
                line.push_str(" ...");
            }
            py.print(&line)?;
        }
    }
    py.print(&format!(
        "{} of {distinct} different movies ({} files) round-trip byte for byte{}{}",
        distinct - bad,
        files.len(),
        if pack { " and packed" } else { "" },
        if dead > 0 {
            format!("; {dead} hold data no structure points at (kept as it was)")
        } else {
            String::new()
        }
    ))?;
    Ok(i32::from(bad > 0))
}

fn cmd_convert(py: &mut Py, mut args: Vec<String>) -> PyResult<i32> {
    let order = if take_flag(&mut args, "--le") {
        Some(false)
    } else if take_flag(&mut args, "--be") {
        Some(true)
    } else {
        None
    };
    let layout = if take_flag(&mut args, "--pack") {
        Layout::Pack
    } else {
        Layout::Keep
    };
    if args.len() != 2 {
        return Err(apt_err("convert <movie> <out.res>"));
    }
    let m = load(&args[0])?;
    let big = order.unwrap_or(!m.big);
    let data = m.to_res(big, [layout; 3])?;
    write_res(&args[1], &data)?;
    py.print(&format!(
        "{}: {}-endian, layout {}",
        args[1],
        if big { "big" } else { "little" },
        if layout == Layout::Pack { "pack" } else { "keep" }
    ))?;
    Ok(0)
}

fn cmd_deps(py: &mut Py, args: &[String]) -> PyResult<i32> {
    if args.len() != 2 {
        return Err(apt_err("deps <movie> <char>"));
    }
    let m = load(&args[0])?;
    let d = edit::deps(&m, py_int0(&args[1])?)?;
    py.print(&format!("character {} of {}:", args[1], m.label))?;
    let chars: Vec<String> = d.chars.iter().map(|(c, t)| format!("{c} {t}")).collect();
    py.print(&format!("  characters: {}", chars.join(", ")))?;
    let opt = |s: &Option<String>| s.clone().unwrap_or_else(|| "None".to_owned());
    let imps: Vec<String> = d
        .imports
        .iter()
        .map(|(mv, n, c)| format!("{c} {}::{}", opt(mv), opt(n)))
        .collect();
    py.print(&format!(
        "  imports:    {}",
        if imps.is_empty() {
            "none".to_owned()
        } else {
            imps.join(", ")
        }
    ))?;
    let fonts: Vec<String> = d.fonts.iter().map(|(c, n)| format!("{c} \"{}\"", opt(n))).collect();
    py.print(&format!(
        "  fonts:      {}",
        if fonts.is_empty() {
            "none".to_owned()
        } else {
            fonts.join(", ")
        }
    ))?;
    let images: Vec<String> = d.images.iter().map(ToString::to_string).collect();
    py.print(&format!(
        "  images:     {}",
        if images.is_empty() {
            "none (no texture atlas entries needed)".to_owned()
        } else {
            images.join(", ")
        }
    ))?;
    let geo: Vec<String> = d.geometry.iter().map(ToString::to_string).collect();
    py.print(&format!(
        "  geometry:   {}",
        if geo.is_empty() {
            "none".to_owned()
        } else {
            geo.join(", ")
        }
    ))?;
    let consts: Vec<u32> = d.constants.iter().copied().collect();
    let mut texts = Vec::new();
    for &k in consts.iter().take(60) {
        texts.push(m.const_text(k as usize)?);
    }
    py.print(&format!(
        "  constants:  {}: {}{}",
        consts.len(),
        texts.join(", "),
        if consts.len() > 60 { " ..." } else { "" }
    ))?;
    Ok(0)
}

fn cmd_graft(py: &mut Py, mut args: Vec<String>) -> PyResult<i32> {
    let frame = take(&mut args, "--frame")?
        .map(|v| py_int(&v))
        .transpose()?
        .unwrap_or(0);
    let depth = take(&mut args, "--depth")?.map(|v| py_int(&v)).transpose()?;
    let name = take(&mut args, "--name")?;
    let x = take(&mut args, "--x")?
        .map(|v| py_float(&v))
        .transpose()?
        .unwrap_or(0.0);
    let y = take(&mut args, "--y")?
        .map(|v| py_float(&v))
        .transpose()?
        .unwrap_or(0.0);
    if args.len() != 4 {
        return Err(apt_err("graft <target> <source> <char> <out.res>"));
    }
    let mut dst = load(&args[0])?;
    let src = load(&args[1])?;
    let frames = dst.frame_count(None)? as i64;
    if !(0..frames).contains(&frame) {
        return Err(apt_err(format!(
            "--frame {frame}: the target's main timeline has frames 0-{}",
            frames - 1
        )));
    }
    let mut g = Grafter::new();
    let want = py_int0(&args[2])?;
    let cid = g.char_py(&src, &mut dst, want)?;
    let depth = match depth {
        Some(d) => d,
        None => free_depths(&mut dst, 1, 2)?[0],
    };
    let item = place_item(&mut dst, depth, cid, name.as_deref(), x, y, 0x26)?;
    dst.add_item(frame as usize, item, None, None)?;
    let data = dst.to_res(dst.big, GRAFT_LAYOUT)?;
    write_res(&args[3], &data)?;
    py.print(&format!(
        "{}: character {} -> {cid}, placed in frame {frame} at depth {depth}",
        args[3], args[2]
    ))?;
    for (n, k, s) in &g.added {
        py.print(&format!("  added {n}: {k} (source {s})"))?;
    }
    for note in &g.notes {
        py.print(&format!("  needs: {note}"))?;
    }
    Ok(0)
}

fn cmd_mode_case(py: &mut Py, mut args: Vec<String>) -> PyResult<i32> {
    let func = take(&mut args, "--function")?;
    let mode = take(&mut args, "--mode")?;
    let key = take(&mut args, "--key")?;
    let fallback = take(&mut args, "--fallback")?.unwrap_or_else(|| "Missing DESC tag!! ".to_owned());
    let (Some(func), Some(mode), Some(key)) = (func, mode, key) else {
        return Err(apt_err("mode-case <movie> <out.res> --function F --mode M --key K"));
    };
    if args.len() != 2 || func.is_empty() || mode.is_empty() || key.is_empty() {
        return Err(apt_err("mode-case <movie> <out.res> --function F --mode M --key K"));
    }
    let mut m = load(&args[0])?;
    let n = edit::add_mode_case(&mut m, &func, &mode, &key, &fallback)?;
    let data = m.to_res(m.big, GRAFT_LAYOUT)?;
    write_res(&args[1], &data)?;
    py.print(&format!("{}: {func} ({n}) returns {key} for {mode}", args[1]))?;
    Ok(0)
}

fn cmd_mode_alias(py: &mut Py, mut args: Vec<String>) -> PyResult<i32> {
    let mode = take(&mut args, "--mode")?;
    let like = take(&mut args, "--like")?;
    let funcs = take(&mut args, "--function")?;
    let usage = "mode-alias <movie> <out.res> --mode M --like L [--function F[,F...]]";
    let (Some(mode), Some(like)) = (mode, like) else {
        return Err(apt_err(usage));
    };
    if args.len() != 2 || mode.is_empty() || like.is_empty() {
        return Err(apt_err(usage));
    }
    let mut m = load(&args[0])?;
    let list: Option<Vec<String>> = funcs
        .filter(|f| !f.is_empty())
        .map(|f| f.split(',').map(str::to_owned).collect());
    let n = edit::add_mode_alias(&mut m, &mode, &like, list.as_deref())?;
    let data = m.to_res(m.big, GRAFT_LAYOUT)?;
    write_res(&args[1], &data)?;
    py.print(&format!("{}: {n} switch(es) send {mode} the way of {like}", args[1]))?;
    Ok(0)
}

fn cmd_label_alias(py: &mut Py, mut args: Vec<String>) -> PyResult<i32> {
    let like = take(&mut args, "--like")?;
    let aliases = take(&mut args, "--alias")?;
    let usage = "label-alias <movie> <out.res> --like L --alias A[,A...]";
    let (Some(like), Some(aliases)) = (like, aliases) else {
        return Err(apt_err(usage));
    };
    if args.len() != 2 || like.is_empty() || aliases.is_empty() {
        return Err(apt_err(usage));
    }
    let mut m = load(&args[0])?;
    let mut n = 0;
    for a in aliases.split(',') {
        n += edit::add_label_alias(&mut m, &like, a)?;
    }
    let data = m.to_res(m.big, GRAFT_LAYOUT)?;
    write_res(&args[1], &data)?;
    py.print(&format!(
        "{}: {n} frame label(s) added on \"{like}\"'s frames: {aliases}",
        args[1]
    ))?;
    Ok(0)
}

fn cmd_crosshair(py: &mut Py, mut args: Vec<String>) -> PyResult<i32> {
    let ids = take(&mut args, "--id")?;
    let frame = take(&mut args, "--frame")?.map(|v| py_int(&v)).transpose()?;
    let usage = "crosshair-add <target> <source> <out.res> --id ID[,ID...] [--frame N (one id)]";
    let Some(ids) = ids.filter(|i| !i.is_empty()) else {
        return Err(apt_err(usage));
    };
    if args.len() != 3 || (frame.is_some() && ids.contains(',')) {
        return Err(apt_err(usage));
    }
    let mut dst = load(&args[0])?;
    let mut src = load(&args[1])?;
    let mut reports = Vec::new();
    for cid in ids.split(',') {
        reports.push(crosshair::add_crosshair(&mut dst, &mut src, cid, frame)?);
    }
    let data = dst.to_res(dst.big, GRAFT_LAYOUT)?;
    write_res(&args[2], &data)?;
    for r in &reports {
        let values: Vec<String> = r.values.iter().map(|(k, v)| format!("{k}: {}", str_repr(v))).collect();
        py.print(&format!(
            "{}: crossHairs[\"{}\"] -> frame {} \"{}\", depths {}, {} characters, registers {}, constants for {{{}}}",
            args[2],
            r.id,
            r.frame,
            r.label,
            dict_ints(&r.depths),
            r.added.len(),
            dict_ints(&r.registers),
            values.join(", ")
        ))?;
        for note in &r.notes {
            py.print(&format!("  needs: {note}"))?;
        }
    }
    Ok(0)
}

fn cmd_timer(py: &mut Py, mut args: Vec<String>) -> PyResult<i32> {
    let lms = !take_flag(&mut args, "--no-lms");
    let place = !take_flag(&mut args, "--no-place");
    if args.len() != 3 {
        return Err(apt_err(
            "onslaught-timer <PC hudlayer_shared> <console hudlayer_shared> <out.res>",
        ));
    }
    let mut pc = load(&args[0])?;
    let mut con = load(&args[1])?;
    let report = edit::onslaught_timer(&mut pc, &mut con, lms, place)?;
    let data = pc.to_res(pc.big, GRAFT_LAYOUT)?;
    write_res(&args[2], &data)?;
    for ln in &report {
        py.print(ln)?;
    }
    py.print(&format!("{}: written", args[2]))?;
    Ok(0)
}

/// `aptgraft.main(argv)` (under `errors.setup('aptgraft')` / `errors.guard()`), then the caches' overall budget
/// when a movie was read out of a bundle (`fbrb.read`'s `_budget`; not when the line is handed to the Python, which
/// runs its own).
pub fn run(py: &mut Py, cmdline: &[String]) -> PyResult<i32> {
    let r = main(py, cmdline);
    if !matches!(r, Err(Fail::Python)) && bc2_menu::graft::BUNDLE_READ.load(std::sync::atomic::Ordering::SeqCst) {
        crate::budget::after_bundle_read();
    }
    r
}

fn main(py: &mut Py, cmdline: &[String]) -> PyResult<i32> {
    let mut args = cmdline.to_vec();
    if args.first().is_none_or(|a| !COMMANDS.contains(&a.as_str())) {
        py.print(&docstring(SOURCE))?;
        return Ok(1);
    }
    let cmd = args.remove(0);
    match cmd.as_str() {
        "roundtrip" => cmd_roundtrip(py, args),
        "convert" => cmd_convert(py, args),
        "deps" => cmd_deps(py, &args),
        "graft" => cmd_graft(py, args),
        "mode-case" => cmd_mode_case(py, args),
        "mode-alias" => cmd_mode_alias(py, args),
        "label-alias" => cmd_label_alias(py, args),
        "crosshair-add" => cmd_crosshair(py, args),
        "onslaught-timer" => cmd_timer(py, args),
        _ => Ok(1),
    }
}
