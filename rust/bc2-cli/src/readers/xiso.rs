//! `bfbc2 xiso list|extract <image.iso> ...`: `bfbc2-console-readers\xiso\xiso.py`'s command line over
//! `bc2_formats::xiso` (round F0's reader). A root cut short (the reference's `struct.error`), a tree nested past what
//! the reader follows, and a read error are handed to the Python.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom, Write as _};

use bc2_formats::xiso;

use super::{index_error, latin1, lower, makedirs_for, open_check, target};
use crate::pyfmt::thousands;
use crate::pyproc::{Fail, Py, PyResult, os_error};

/// `raise SystemExit(message)` under `errors.guard` (which lets it through): the message on stderr, exit code 1.
fn system_exit(py: &mut Py, message: &str) -> i32 {
    py.eprint(message);
    1
}

pub fn run(py: &mut Py, args: &[String]) -> PyResult<i32> {
    let (Some(cmd), Some(iso)) = (args.first(), args.get(1)) else {
        return Err(index_error());
    };
    open_check(iso)?;
    let mut f = BufReader::new(File::open(iso).map_err(|e| os_error(iso, &e))?);
    let base = match xiso::find_partition(&mut f) {
        Ok(b) => b,
        Err(xiso::Error::NoVolume) => return Ok(system_exit(py, "no XDVDFS volume found")),
        Err(_) => return Err(Fail::Python),
    };
    let Some((root_sec, root_size)) = xiso::root(&mut f, base).map_err(|_| Fail::Python)? else {
        return Err(Fail::Python); // struct.error: the root cut short
    };
    let (files, _) = xiso::walk(&mut f, base, root_sec, root_size, b"").map_err(|_| Fail::Python)?;
    if cmd == "list" {
        let flt = lower(args.get(2).map_or("", String::as_str))?;
        py.print(&format!("partition at 0x{base:X}, {} files", files.len()))?;
        for (path, _sec, sz) in &files {
            let p = latin1(path);
            if lower(&p)?.contains(&flt) {
                py.print(&format!("{sz:12}  {p}"))?;
            }
        }
    } else if cmd == "extract" {
        let out = args.get(2).ok_or_else(index_error)?;
        let flt = lower(args.get(3).map_or("", String::as_str))?;
        let mut n = 0;
        for (path, sec, sz) in &files {
            let p = latin1(path);
            if !lower(&p)?.contains(&flt) {
                continue;
            }
            let dst = target(out, &p)?;
            makedirs_for(&dst)?;
            let mut o = File::create(&dst).map_err(|e| os_error(&dst, &e))?;
            f.seek(SeekFrom::Start(base + u64::from(*sec) * xiso::SECTOR))
                .map_err(|_| Fail::Python)?;
            let mut left = u64::from(*sz);
            let mut buf = Vec::new();
            while left > 0 {
                buf.clear();
                (&mut f)
                    .take(left.min(1 << 22))
                    .read_to_end(&mut buf)
                    .map_err(|_| Fail::Python)?;
                if buf.is_empty() {
                    drop(o);
                    let msg = format!(
                        "{p}: the image ends {} bytes before the end of the file",
                        thousands(i128::from(left))
                    );
                    return Ok(system_exit(py, &msg));
                }
                o.write_all(&buf).map_err(|_| Fail::Python)?;
                left -= buf.len() as u64;
            }
            n += 1;
        }
        py.print(&format!("extracted {n} files to {out}"))?;
    }
    Ok(0)
}
