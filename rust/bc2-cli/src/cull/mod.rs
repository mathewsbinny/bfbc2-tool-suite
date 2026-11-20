//! Round O3's `cull` group: `bfbc2 culltree info|verify|add ...`, the command line of `bfbc2-culltree\culltree.py`
//! over `bc2_levelwrite::culltool` (its model, the reference levels and `add` in Rust): the same arguments, printed
//! lines (each flushed as it is printed), written trees, levels and reports, errors and exit codes.
//!
//! The reference levels' rows are kept in `culltree.py`'s own row cache (round CT1: `bc2_levelwrite::cullcache`,
//! the pickles under the culltree cache folder, shared with the Python; `BFBC2_CULLTREE_CACHE`). The answers do not
//! depend on that cache.
//!
//! Handed to the Python: the reference's own exceptions on damaged files (an index, `struct` or big-integer error
//! where a `CullTreeError` is not raised), a sphere key or value `struct` cannot pack, an objects file that is not
//! UTF-8.

#![allow(
    clippy::indexing_slicing,
    clippy::similar_names,
    reason = "the arguments checked by count and the level's objects by the indices the model gave; the reference's names"
)]

use bc2_levelwrite::cullcache::{self, PickleStore};
use bc2_levelwrite::culltool::{
    self as ct, CullTree, Level, SPARE, auto_objects, check_structure, collect, containment, cte, fixed, fmt3,
    leaves_of, misplaced, object_checks, ref_rows,
};
use bc2_pyrt::py::{abspath, basename, dirname, strip};

use crate::pyfmt::{docstring, str_repr};
use crate::pyproc::{Fail, Py, PyErr, PyResult, Tool};

/// The group's tools.
pub const TOOLS: &[Tool] = &[Tool {
    name: "culltree",
    script: "bfbc2-culltree\\culltree.py",
    log: Some("culltree"),
    run,
}];

const SOURCE: &str = include_str!("../../../../bfbc2-culltree/culltree.py");

/// `Out`: each line printed and flushed, and kept for `--report`.
struct Out<'a> {
    py: &'a mut Py,
    lines: Vec<String>,
}

impl Out<'_> {
    fn log(&mut self, s: &str) -> PyResult<()> {
        self.py.print(s)?;
        self.py.flush_out();
        self.lines.push(s.to_owned());
        Ok(())
    }

    /// `save(path)`: the lines as a UTF-8 text file (`\r\n`), its folder made.
    fn save(&self, path: Option<&str>) -> PyResult<()> {
        let Some(path) = path.filter(|p| !p.is_empty()) else {
            return Ok(());
        };
        let dir = dirname(&abspath(path));
        std::fs::create_dir_all(&dir).map_err(|e| ct::mkdir_error(&dir, &e))?;
        let text = format!("{}\n", self.lines.join("\n")).replace('\n', "\r\n");
        std::fs::write(path, text).map_err(|e| crate::pyproc::os_error(path, &e))
    }
}

/// `_arg(args, name, many=True)`: every value given (each taken out of args).
fn arg_all(args: &mut Vec<String>, name: &str) -> PyResult<Vec<String>> {
    let mut vals = Vec::new();
    while let Some(i) = args.iter().position(|a| a == name) {
        if i + 1 >= args.len() {
            return Err(cte(format!("{name} needs a value")));
        }
        vals.push(args.remove(i + 1));
        args.remove(i);
    }
    Ok(vals)
}

/// `_arg(args, name)`: the last value given.
fn arg(args: &mut Vec<String>, name: &str) -> PyResult<Option<String>> {
    Ok(arg_all(args, name)?.pop())
}

/// `_flag(args, name)`.
fn flag(args: &mut Vec<String>, name: &str) -> bool {
    match args.iter().position(|a| a == name) {
        Some(i) => {
            args.remove(i);
            true
        }
        None => false,
    }
}

/// `_report(res, log)`: the failed count.
fn report(res: &[(bool, String)], out: &mut Out) -> PyResult<usize> {
    let mut bad = 0;
    for (ok, text) in res {
        bad += usize::from(!ok);
        out.log(&format!("{} {text}", if *ok { "ok  " } else { "FAIL" }))?;
    }
    Ok(bad)
}

/// `BFBC2_CULLTREE_REFERENCE=1`: `ref_pairs` instead of `ref_rows`.
fn reference() -> bool {
    std::env::var("BFBC2_CULLTREE_REFERENCE").is_ok_and(|v| !v.is_empty() && v != "0")
}

fn last_part(bp: &str) -> &str {
    bp.rsplit('/').next().unwrap_or("")
}

/// A list's repr: `[1, 2]`.
fn list(ids: &[i64]) -> String {
    format!(
        "[{}]",
        ids.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
    )
}

/// `culltree.cache_folder()`: `BFBC2_CULLTREE_CACHE` (a folder; 0: none), else
/// `%LOCALAPPDATA%\bfbc2-tool-suite\cache\culltree`.
fn cache_folder() -> Option<String> {
    let v = std::env::var("BFBC2_CULLTREE_CACHE").unwrap_or_default();
    if v == "0" {
        return None;
    }
    if !v.is_empty() {
        return Some(v);
    }
    let base = std::env::var("LOCALAPPDATA")
        .ok()
        .filter(|b| !b.is_empty())
        .unwrap_or_else(crate::pylog::home_dir);
    Some(bc2_pyrt::py::join(&base, &["bfbc2-tool-suite", "cache", "culltree"]))
}

/// `caches.touch(path)`: the entry marked used now, unless it was in the last minute.
fn touch(path: &str) {
    if let Ok(st) = crate::fsx::stat(path)
        && crate::fsx::now() - st.mtime >= 60.0
    {
        let _ = crate::fsx::utime_now(path);
    }
}

/// `ref_rows(refs, dirs, log, target=(tree, level))`, its notes printed as the command's own lines; the rows kept
/// in `culltree.py`'s row cache (round CT1), its folder pruned to its cap after a run that wrote to it.
fn ref_levels(out: &mut Out, refs: &[String], dirs: &[String], target: (&str, &str)) -> PyResult<Vec<ct::RefLevel>> {
    let store = cache_folder()
        .zip(cullcache::code_id(SOURCE))
        .map(|(folder, code)| PickleStore::new(folder, code, touch));
    let r = ref_rows(
        refs,
        dirs,
        &mut |s| out.log(s),
        Some(target),
        reference(),
        store.as_ref().map(|s| s as &dyn ct::RowStore),
    );
    if let Some(s) = &store
        && s.wrote.load(std::sync::atomic::Ordering::Relaxed)
    {
        crate::caches::maybe_prune("culltree", &s.folder);
    }
    r
}

/// `{k: c, ...}` of the leaves per object, sorted.
fn per_object(lo: &[(usize, Vec<i64>)]) -> Vec<(usize, usize)> {
    let mut ks: Vec<(usize, usize)> = Vec::new();
    for (_, v) in lo {
        match ks.iter_mut().find(|c| c.0 == v.len()) {
            Some(c) => c.1 += 1,
            None => ks.push((v.len(), 1)),
        }
    }
    ks.sort_unstable();
    ks
}

fn cmd_info(args: &[String], out: &mut Out) -> PyResult<i32> {
    let t = CullTree::read_only(&args[0])?;
    let (depth, most) = t.depth_and_stack();
    let r = t.arena.sphere(t.root);
    let leaves = t.leaves();
    out.log(&args[0])?;
    out.log(&format!(
        "  nodes {}: {} used ({leaves} leaves, {} internal), {} spare",
        t.nodes.len(),
        t.used,
        t.used as i64 - leaves,
        t.spare
    ))?;
    out.log(&format!(
        "  EnumOffset {}: slots {}..{}",
        t.enum_offset,
        t.enum_offset + 1,
        t.enum_offset + leaves
    ))?;
    out.log(&format!(
        "  root sphere centre {} radius {} m, Max {}; deepest leaf {depth}, stack {most}",
        fmt3(r.c),
        fixed(r.r, 2),
        fixed(t.arena.max(t.root), 3)
    ))?;
    if let Some(level) = args.get(1) {
        let lv = Level::read(level)?;
        let found = collect(&t, &lv, &[])?;
        let lo = leaves_of(&t, &lv);
        let dict = per_object(&lo)
            .iter()
            .map(|(k, c)| format!("{k}: {c}"))
            .collect::<Vec<_>>()
            .join(", ");
        let zero = lv.objects.iter().filter(|o| o.enumeration == 0).count();
        out.log(&format!(
            "{level}: {} placed objects, {} with a slot in this tree (leaves per object: {{{dict}}}), {zero} with slot 0",
            lv.objects.len(),
            lo.len()
        ))?;
        for o in auto_objects(&lv, &found) {
            let o = &lv.objects[o];
            out.log(&format!(
                "  slot 0, blueprint in this tree: {} {} at {}",
                o.guid,
                o.blueprint,
                fmt3(o.pos())
            ))?;
        }
    }
    Ok(0)
}

fn cmd_verify(mut args: Vec<String>, out: &mut Out) -> PyResult<i32> {
    let refs = arg_all(&mut args, "--ref")?;
    let dirs = arg_all(&mut args, "--ref-dir")?;
    let client_p = arg(&mut args, "--client")?;
    let added = arg(&mut args, "--added")?;
    let report_p = arg(&mut args, "--report")?;
    if args.len() != 2 {
        return Err(cte("verify needs <tree.dbx> <level.dbx>"));
    }
    let mut t = CullTree::read_only(&args[0])?;
    let lv = Level::read(&args[1])?;
    let client = match client_p.as_deref().filter(|c| !c.is_empty()) {
        Some(c) => Some(CullTree::read_only(c)?),
        None => None,
    };
    out.log(&format!("tree  {}\nlevel {}", args[0], args[1]))?;
    let rr = ref_levels(out, &refs, &dirs, (&args[0], &args[1]))?;
    let found = collect(&t, &lv, &rr)?;
    let mut bad = report(&check_structure(&mut t, client.as_ref()), out)?;
    let (res, detail) = object_checks(&t, &lv, &found);
    bad += report(&res, out)?;
    for d in detail {
        out.log(&d)?;
    }
    if let Some(added) = added.filter(|a| !a.is_empty()) {
        for g in added.split(',').filter(|g| !strip(g).is_empty()) {
            let Some(o) = lv.by_guid(g).filter(|&o| lv.objects[o].enumeration != 0) else {
                out.log(&format!("FAIL added object {g}: not in the level or slot 0"))?;
                bad += 1;
                continue;
            };
            let ids = leaves_of(&t, &lv)
                .into_iter()
                .find(|x| x.0 == o)
                .map_or_else(Vec::new, |x| x.1);
            let c = containment(&t, &ids);
            let miss: usize = c.iter().map(|x| x.1).sum();
            let ok = !c.is_empty() && miss == 0;
            bad += usize::from(!ok);
            let obj = &lv.objects[o];
            out.log(&format!(
                "{} added {} {} slot {}: leaves {}, every ancestor (root excepted) contains their spheres (path {} \
                 nodes, {miss} not containing)",
                if ok { "ok  " } else { "FAIL" },
                obj.guid,
                last_part(&obj.blueprint),
                obj.enumeration,
                list(&ids),
                c.iter().map(|x| x.2).max().unwrap_or(0)
            ))?;
        }
    }
    out.log(&if bad == 0 {
        "all checks passed".to_owned()
    } else {
        format!("{bad} check(s) failed")
    })?;
    out.save(report_p.as_deref())?;
    Ok(i32::from(bad != 0))
}

/// `float(text)`, or the `ValueError` the reference raises.
fn py_float(text: &str) -> PyResult<f64> {
    crate::pylog::py_float(text).ok_or_else(|| {
        Fail::Err(PyErr::new(
            "ValueError",
            format!("could not convert string to float: {}", str_repr(text)),
        ))
    })
}

/// `--objects`'s guids: as given, or `@file`'s lines (comments after `#` and blank lines left out), comma-joined.
fn object_guids(objs: &str) -> PyResult<String> {
    let Some(file) = objs.strip_prefix('@') else {
        return Ok(objs.to_owned());
    };
    let data = crate::pyproc::read_file(file)?;
    let text = String::from_utf8(data).map_err(|_| Fail::Python)?;
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let parts: Vec<&str> = text
        .split_inclusive('\n')
        .map(|l| strip(l.split('#').next().unwrap_or("")))
        .filter(|s| !s.is_empty())
        .collect();
    Ok(parts.join(","))
}

#[allow(
    clippy::too_many_lines,
    reason = "cmd_add's steps in the reference's order, kept in one piece"
)]
fn cmd_add(mut args: Vec<String>, out: &mut Out) -> PyResult<i32> {
    let refs = arg_all(&mut args, "--ref")?;
    let dirs = arg_all(&mut args, "--ref-dir")?;
    let objs_arg = arg(&mut args, "--objects")?.filter(|s| !s.is_empty());
    let auto = flag(&mut args, "--auto");
    let away = arg(&mut args, "--misplaced")?;
    let out_tree = arg(&mut args, "--out-tree")?.filter(|s| !s.is_empty());
    let out_level = arg(&mut args, "--out-level")?.filter(|s| !s.is_empty());
    let client_p = arg(&mut args, "--client")?;
    let report_p = arg(&mut args, "--report")?;
    let (Some(out_tree), Some(out_level)) = (out_tree, out_level) else {
        return Err(cte(
            "add needs <tree.dbx> <level.dbx>, --objects or --auto, --out-tree and --out-level",
        ));
    };
    if args.len() != 2 || !(objs_arg.is_some() || auto) {
        return Err(cte(
            "add needs <tree.dbx> <level.dbx>, --objects or --auto, --out-tree and --out-level",
        ));
    }
    let low = |p: &str| abspath(p).to_lowercase();
    for path in [&out_tree, &out_level] {
        if low(path) == low(&args[0]) || low(path) == low(&args[1]) {
            return Err(cte(
                "write the results to new files (--out-tree / --out-level), not over the inputs",
            ));
        }
    }
    let mut t = CullTree::read(&args[0])?;
    let mut lv = Level::read(&args[1])?;
    let client = match client_p.as_deref().filter(|c| !c.is_empty()) {
        Some(c) => Some(CullTree::read_only(c)?),
        None => None,
    };
    let rr = ref_levels(out, &refs, &dirs, (&args[0], &args[1]))?;
    let mut found = collect(&t, &lv, &rr)?;
    if let Some(away) = away {
        let metres = py_float(&away)?;
        let moved = misplaced(&t, &lv, &found, metres);
        for (o, off) in &moved {
            let obj = &lv.objects[*o];
            out.log(&format!(
                "  misplaced: {} {} at {} on slot {}, its leaves {} m from the blueprint's shape: slot 0, new leaves",
                obj.guid,
                last_part(&obj.blueprint),
                fmt3(obj.pos()),
                obj.enumeration,
                fixed(*off, 2)
            ))?;
            lv.set_enum(*o, 0)?;
        }
        if !moved.is_empty() {
            found = collect(&t, &lv, &rr)?;
        }
    }
    let objs: Vec<usize> = if auto {
        let objs = auto_objects(&lv, &found);
        let far: Vec<usize> = objs
            .iter()
            .copied()
            .filter(|&o| lv.objects[o].pos().iter().any(|c| c.abs() > 2040.0))
            .collect();
        for &o in &far {
            let obj = &lv.objects[o];
            out.log(&format!(
                "  beyond the tree's range, left at slot 0: {} {} at {}",
                obj.guid,
                last_part(&obj.blueprint),
                fmt3(obj.pos())
            ))?;
        }
        objs.into_iter().filter(|o| !far.contains(o)).collect()
    } else {
        let guids = object_guids(&objs_arg.unwrap_or_default())?;
        let mut objs = Vec::new();
        for g in guids.split(',') {
            if strip(g).is_empty() {
                continue;
            }
            let Some(o) = lv.by_guid(g) else {
                return Err(cte(format!("object {g} is not in {}", basename(&args[1]))));
            };
            objs.push(o);
        }
        objs
    };
    if objs.is_empty() {
        out.log("nothing to add")?;
        return Ok(0);
    }
    let before = (t.used, t.leaves());
    out.log(&format!(
        "adding {} object(s) to {} (EnumOffset {}, {} leaves):",
        objs.len(),
        args[0],
        t.enum_offset,
        t.leaves()
    ))?;
    let done = ct::add(&mut t, &mut lv, &objs, &found, &mut |s| out.log(s))?;
    t.write(&out_tree)?;
    lv.doc.write(&out_level)?;
    // the documents are not used once written (round CT1: their memory freed before the files are read back)
    t.doc.release();
    lv.doc.release();
    let used = t.arena.flatten(t.root).len();
    out.log(&format!(
        "tree: {} -> {used} used nodes, {} -> {} leaves, {SPARE} spare; written {out_tree}",
        before.0,
        before.1,
        t.leaves()
    ))?;
    out.log(&format!(
        "level: {} Enumeration(s) set; written {out_level}",
        done.len()
    ))?;
    let mut t2 = CullTree::read_only(&out_tree)?;
    let lv2 = ct::scan_level(&out_level)?; // its objects (Level(out_level)'s, streamed)
    let mut bad = report(&check_structure(&mut t2, client.as_ref()), out)?;
    for (o, slot, ids, _) in &done {
        let c = containment(&t2, ids);
        let miss: usize = c.iter().map(|x| x.1).sum();
        let guid = &lv.objects[*o].guid;
        let o2 = lv2.by_guid(guid).ok_or(Fail::Python)?;
        let en = lv2.objects[o2].enumeration;
        let ok = c.len() == ids.len() && miss == 0 && en == *slot;
        bad += usize::from(!ok);
        out.log(&format!(
            "{} {guid}: slot {en}, leaves {} reached through containing spheres",
            if ok { "ok  " } else { "FAIL" },
            list(ids)
        ))?;
    }
    out.log(&if bad == 0 {
        "written and checked".to_owned()
    } else {
        format!("{bad} check(s) failed")
    })?;
    out.save(report_p.as_deref())?;
    Ok(i32::from(bad != 0))
}

/// `main(argv)`.
pub fn run(py: &mut Py, argv: &[String]) -> PyResult<i32> {
    let Some((cmd, rest)) = argv.split_first() else {
        py.print(&docstring(SOURCE))?;
        return Ok(0);
    };
    if matches!(cmd.as_str(), "-h" | "--help" | "help") {
        py.print(&docstring(SOURCE))?;
        return Ok(0);
    }
    let mut out = Out { py, lines: Vec::new() };
    match cmd.as_str() {
        "info" => {
            if rest.is_empty() {
                return Err(cte("info needs <tree.dbx> [<level.dbx>]"));
            }
            cmd_info(rest, &mut out)
        }
        "verify" => cmd_verify(rest.to_vec(), &mut out),
        "add" => cmd_add(rest.to_vec(), &mut out),
        _ => Err(cte(format!(
            "unknown command {}: use info, verify or add",
            str_repr(cmd)
        ))),
    }
}
