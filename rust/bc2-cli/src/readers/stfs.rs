//! `bfbc2 stfs info|list|extract <package> ...`: `bfbc2-console-readers\stfs\stfs.py`'s command line over
//! `bc2_formats::stfs` (round F0's reader). A package the reader leaves to the reference (a header or a file table
//! cut short, a folder that is its own ancestor) and a read the reader cannot make are handed to the Python.

#![allow(
    clippy::indexing_slicing,
    reason = "the arguments are indexed after their count is checked, as the Python does"
)]

use std::fmt::Write as _;
use std::fs::File;

use bc2_formats::stfs::Package;

use super::{latin1, lower, makedirs_for, open_check, target};
use crate::pyfmt::docstring;
use crate::pyproc::{Fail, Py, PyResult, write_file};

const SOURCE: &str = include_str!("../../../../bfbc2-console-readers/stfs/stfs.py");

/// `repr(bytes)` of the magic.
fn bytes_repr(b: &[u8]) -> String {
    let q = if b.contains(&b'\'') && !b.contains(&b'"') {
        '"'
    } else {
        '\''
    };
    let mut out = format!("b{q}");
    for &c in b {
        match c {
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            c if char::from(c) == q => {
                out.push('\\');
                out.push(q);
            }
            0x20..=0x7e => out.push(char::from(c)),
            c => {
                let _ = write!(out, "\\x{c:02x}");
            }
        }
    }
    out.push(q);
    out
}

pub fn run(py: &mut Py, args: &[String]) -> PyResult<i32> {
    let cmd = args.first().map(String::as_str);
    let known = matches!(cmd, Some("info" | "list" | "extract"));
    if args.len() < 2 || !known || (cmd == Some("extract") && args.len() < 3) {
        py.print(&docstring(SOURCE))?;
        return Ok(1);
    }
    let path = &args[1];
    open_check(path)?;
    let f = File::open(path).map_err(|e| crate::pyproc::os_error(path, &e))?;
    let mut pkg = Package::open(f).map_err(|_| Fail::Python)?;
    let h = pkg.header.clone();
    if cmd == Some("info") {
        py.print(&format!(
            "{} \"{}\" header {:#x} separation {} file table {} blocks at {}; {} entries",
            bytes_repr(&h.magic),
            h.name,
            h.header_size,
            h.separation,
            h.ft_blocks,
            h.ft_start,
            pkg.entries.len()
        ))?;
        return Ok(0);
    }
    let flt = if cmd == Some("list") { args.get(2) } else { args.get(3) };
    let flt = lower(flt.map_or("", String::as_str))?;
    let entries = pkg.entries.clone();
    for e in &entries {
        let p = latin1(&e.path);
        if e.dir || !lower(&p)?.contains(&flt) {
            continue;
        }
        if cmd == Some("list") {
            py.print(&format!("{:>12}  {p}", e.size))?;
        } else {
            let dst = target(&args[2], &p)?;
            makedirs_for(&dst)?;
            let data = pkg
                .read(u64::from(e.start), u64::from(e.count), e.consecutive, u64::from(e.size))
                .map_err(|_| Fail::Python)?;
            write_file(&dst, &data)?;
            py.print(&format!("{:>12}  {dst}", e.size))?;
        }
    }
    Ok(0)
}
