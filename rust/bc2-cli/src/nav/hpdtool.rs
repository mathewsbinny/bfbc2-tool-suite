//! `bfbc2 hpdtool info|sector|roundtrip|coverage|meshtrip ...`: NavKit's `bfbc2-auto-navkit\tools\hpdtool.py`
//! command line over `bc2_formats::hpd` (parse / write), `hpdlayout::coverage` and `kymesh` (parse / write).
//!
//! Where the core refuses a file or a mesh (the reference raises, and prints or reports its own exception text, or a
//! value is a NaN) the command line is handed to the Python; `coverage`'s errors of a mesh's walk are printed here as
//! the Python prints them (round RT2: `hpdlayout` gives the reference's messages).

#![allow(
    clippy::indexing_slicing,
    reason = "the arguments after their count is checked; mesh slices clamped to the mesh; indexes below a common length"
)]

use bc2_formats::{hpd, hpdlayout, kymesh};

use super::common::{arg, basename, dirname, hex_spaced, index, isdir, join, py_int, walk};
use crate::pyfmt::{docstring, float_g, float_repr, round_to, str_repr};
use crate::pyproc::{Fail, Py, PyErr, PyResult, read_file};

const SOURCE: &str = include_str!("../../../../bfbc2-auto-navkit/tools/hpdtool.py");

fn usage(py: &mut Py) -> PyResult<i32> {
    py.print(&docstring(SOURCE))?;
    Ok(1)
}

fn parse(data: &[u8]) -> PyResult<hpd::HpdFile<'_>> {
    hpd::parse(data).map_err(|_| Fail::Python)
}

/// `_files(args)`: each argument, or the `pathdata.hpd` files (any case) under a folder.
fn files(args: &[String]) -> PyResult<Vec<String>> {
    let mut out = Vec::new();
    for a in args {
        if isdir(a) {
            let mut tree = Vec::new();
            walk(a, &mut tree)?;
            for (dp, _dirs, fs) in tree {
                for f in fs {
                    if f.to_lowercase() == "pathdata.hpd" {
                        out.push(join(&dp, &f).ok_or(Fail::Python)?);
                    }
                }
            }
        } else {
            out.push(a.clone());
        }
    }
    Ok(out)
}

fn u32_at(m: &[u8], at: usize) -> PyResult<u32> {
    m.get(at..at + 4)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or(Fail::Python)
}

fn info(py: &mut Py, path: &str) -> PyResult<i32> {
    let data = read_file(path)?;
    let f = parse(&data)?;
    let nodes: usize = f.sectors.iter().map(|s| s.nodes.len()).sum();
    let edges: usize = f.sectors.iter().map(|s| s.edges.len()).sum();
    let mesh: usize = f.sectors.iter().map(|s| s.mesh.len()).sum();
    let mut dates: Vec<u32> = f.sectors.iter().map(|s| s.date).collect();
    dates.sort_unstable();
    dates.dedup();
    let (Some(first), Some(last)) = (dates.first(), dates.last()) else {
        return Err(PyErr::new("IndexError", "list index out of range").into());
    };
    py.print(&format!(
        "{} sectors, {nodes} graph nodes, {edges} edges, {mesh} bytes of runtime mesh; build dates {first}..{last}",
        f.sectors.len()
    ))?;
    Ok(0)
}

fn sector(py: &mut Py, args: &[String]) -> PyResult<i32> {
    let data = read_file(&args[1])?;
    let f = parse(&data)?;
    let i = py_int(arg(args, 2)?)?;
    let s = index(&f.sectors, i)?;
    let bounds: Vec<String> = s
        .bounds
        .iter()
        .map(|&b| float_repr(round_to(f64::from(b), 2)))
        .collect();
    py.print(&format!(
        "sector {:#06x} bounds [{}] built {} {:08}",
        s.id,
        bounds.join(", "),
        s.date,
        s.time
    ))?;
    for n in &s.nodes {
        py.print(&format!(
            "  node {:#010x} pos ({:.2}, {:.2}, {:.2}) flags {:#x} value {} a {} b {},{} c {}",
            n.id,
            f64::from(n.pos[0]),
            f64::from(n.pos[1]),
            f64::from(n.pos[2]),
            n.flags,
            float_g(f64::from(n.value)),
            n.a,
            n.b & 0xFFFF,
            n.b >> 16,
            n.c
        ))?;
    }
    for e in &s.edges {
        py.print(&format!("  edge {:#010x} {} -> {}", e.tag, e.src, e.dst))?;
    }
    let head = &s.mesh[..s.mesh.len().min(48)];
    py.print(&format!("  mesh {} bytes: {}", s.mesh.len(), hex_spaced(head)))?;
    Ok(0)
}

fn coverage(py: &mut Py, args: &[String]) -> PyResult<i32> {
    let (mut total, mut clean) = (0usize, 0usize);
    let mut kinds: Vec<((Vec<i64>, usize), usize)> = Vec::new();
    for p in files(&args[1..])? {
        let data = read_file(&p)?;
        let f = parse(&data)?;
        for s in &f.sectors {
            total += 1;
            let (gaps, overlaps) = match hpdlayout::coverage(s.mesh) {
                Ok(c) => c,
                Err(e) => {
                    // the reference's ValueError / struct.error, caught and printed (round RT2: in Rust)
                    py.print(&format!("ERROR {p} sector {:#x}: {e}", s.id))?;
                    continue;
                }
            };
            if gaps.is_empty() && overlaps.is_empty() {
                clean += 1;
                continue;
            }
            let key = (
                gaps.iter().take(3).map(|(b, e)| e - b).collect::<Vec<_>>(),
                overlaps.len(),
            );
            let n = if let Some(k) = kinds.iter_mut().find(|k| k.0 == key) {
                k.1 += 1;
                k.1
            } else {
                kinds.push((key, 1));
                1
            };
            if n <= 2 {
                let mut desc = Vec::new();
                for &(b, e) in gaps.iter().take(4) {
                    let (bu, eu) = (
                        usize::try_from(b).map_err(|_| Fail::Python)?,
                        usize::try_from(e).map_err(|_| Fail::Python)?,
                    );
                    let bytes = s.mesh.get(bu..eu.min(s.mesh.len()).max(bu)).unwrap_or_default();
                    desc.push(str_repr(&format!("{b:#x}:{}", hex_spaced(bytes))));
                }
                let m = s.mesh;
                let at = 0x50usize
                    .checked_add(
                        (u64::from(u32_at(m, 0x38)?) * u64::from(u32_at(m, 0x3C)?) * 4)
                            .try_into()
                            .map_err(|_| Fail::Python)?,
                    )
                    .ok_or(Fail::Python)?;
                let last = u32_at(m, at)?;
                let ov: Vec<String> = overlaps
                    .iter()
                    .take(2)
                    .map(|(a, b, l)| format!("({a}, {b}, {})", str_repr(l)))
                    .collect();
                let up = dirname(&p).ok_or(Fail::Python)?;
                let level = basename(&dirname(&up).ok_or(Fail::Python)?).ok_or(Fail::Python)?;
                let sub = basename(&up).ok_or(Fail::Python)?;
                py.print(&format!(
                    "{level}/{sub} sector {:#x} len {:#x} last-tile-entry {last:#x}: gaps [{}] overlaps [{}]",
                    s.id,
                    m.len(),
                    desc.join(", "),
                    ov.join(", ")
                ))?;
            }
        }
    }
    py.print(&format!(
        "coverage: {clean}/{total} meshes fully explained; gap patterns {}",
        kinds.len()
    ))?;
    Ok(0)
}

fn meshtrip(py: &mut Py, args: &[String]) -> PyResult<i32> {
    let (mut total, mut same) = (0usize, 0usize);
    let mut reasons: Vec<(String, usize)> = Vec::new();
    for p in files(&args[1..])? {
        let data = read_file(&p)?;
        let f = parse(&data)?;
        for s in &f.sectors {
            total += 1;
            let mesh = kymesh::parse(s.mesh).map_err(|_| Fail::Python)?;
            let out = kymesh::write(&mesh).map_err(|_| Fail::Python)?;
            if out == s.mesh {
                same += 1;
                continue;
            }
            let common = out.len().min(s.mesh.len());
            let first = (0..common).find(|&i| out[i] != s.mesh[i]).unwrap_or(common);
            let w = first / 4 * 4;
            let delta = i128::try_from(out.len()).unwrap_or(0) - i128::try_from(s.mesh.len()).unwrap_or(0);
            let diff = format!("len {delta:+}, first diff at word {w:#x}");
            if reasons.is_empty() {
                let orig = &s.mesh[w.min(s.mesh.len())..(w + 16).min(s.mesh.len())];
                let new = &out[w.min(out.len())..(w + 16).min(out.len())];
                py.print(&format!(
                    "first mismatch: {p} sector {:#x}: {diff}; orig {} new {}",
                    s.id,
                    hex_spaced(orig),
                    hex_spaced(new)
                ))?;
            }
            if let Some(r) = reasons.iter_mut().find(|r| r.0 == diff) {
                r.1 += 1;
            } else {
                reasons.push((diff, 1));
            }
        }
    }
    py.print(&format!("mesh round trip: {same}/{total} identical"))?;
    let mut sorted = reasons;
    sorted.sort_by_key(|e| std::cmp::Reverse(e.1));
    for (k, v) in sorted.iter().take(8) {
        py.print(&format!("  {v:6} x {k}"))?;
    }
    Ok(if same == total { 0 } else { 2 })
}

fn roundtrip(py: &mut Py, args: &[String]) -> PyResult<i32> {
    let (mut ok, mut bad) = (0usize, 0usize);
    for p in files(&args[1..])? {
        let data = read_file(&p)?;
        let f = parse(&data)?;
        let same = hpd::write(&f).map_err(|_| Fail::Python)? == data;
        if same {
            ok += 1;
        } else {
            bad += 1;
            py.print(&format!("MISMATCH {p}"))?;
        }
    }
    py.print(&format!("round trip: {ok} identical, {bad} different"))?;
    Ok(if bad == 0 { 0 } else { 2 })
}

pub fn run(py: &mut Py, args: &[String]) -> PyResult<i32> {
    if args.len() < 2 {
        return usage(py);
    }
    match args[0].as_str() {
        "info" => info(py, &args[1]),
        "sector" => sector(py, args),
        "coverage" => coverage(py, args),
        "meshtrip" => meshtrip(py, args),
        "roundtrip" => roundtrip(py, args),
        _ => usage(py),
    }
}
