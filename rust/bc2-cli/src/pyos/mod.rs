//! `os`, `os.path` (`ntpath`), `glob`, `shutil`, `hashlib` and `bfbc2core.atomicio` as the round O4 tools use them:
//! the same paths spelled the same way, the same listings in the same order, the same bytes on disk.
//!
//! Paths are Python strs; a path the port cannot spell as Python would (a UNC path, a name that is not Unicode, a
//! pattern with `[`) gives [`Fail::Python`] so the command line is handed to the Python.

// path parts and SHA-256's words indexed within the lengths just checked; SHA-256's own one-letter names; Python's
// float of a nanosecond count; ntpath's assignments kept as it writes them
#![allow(
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::cast_precision_loss,
    clippy::assigning_clones,
    clippy::unnecessary_wraps,
    clippy::chunks_exact_to_as_chunks
)]

// round O5: round O3's fsx (stat, utime, the shared lock, replace's tries, scandir) folded in
pub mod files;

use std::fmt::Write as _;
use std::fs;
use std::io::Read as _;
use std::path::Path;

use crate::pyproc::{Fail, PyErr, PyResult, os_error};

/// `\` or `/`.
#[must_use]
pub fn is_sep(c: char) -> bool {
    c == '\\' || c == '/'
}

/// `ntpath.splitdrive(p)` for a drive letter (`C:`); a UNC path (`\\server\share`) is left to Python.
pub fn splitdrive(p: &str) -> PyResult<(&str, &str)> {
    match p.as_bytes() {
        [d, b':', ..] if d.is_ascii_alphabetic() => Ok(p.split_at(2)),
        [a, b, ..] if is_sep(char::from(*a)) && is_sep(char::from(*b)) => Err(Fail::Python),
        _ => Ok(("", p)),
    }
}

/// `ntpath.isabs(p)`.
pub fn isabs(p: &str) -> PyResult<bool> {
    let (_, rest) = splitdrive(p)?;
    Ok(rest.starts_with(is_sep))
}

/// `ntpath.join(a, *parts)`.
pub fn join(a: &str, parts: &[&str]) -> PyResult<String> {
    let (mut drive, mut path) = {
        let (d, r) = splitdrive(a)?;
        (d.to_owned(), r.to_owned())
    };
    for p in parts {
        let (pd, pp) = splitdrive(p)?;
        if pp.starts_with(is_sep) {
            // the second path is absolute
            if !pd.is_empty() || drive.is_empty() {
                drive = pd.to_owned();
            }
            path = pp.to_owned();
            continue;
        } else if !pd.is_empty() && !pd.eq_ignore_ascii_case(&drive) {
            // a different drive: ignore the first path entirely
            drive = pd.to_owned();
            path = pp.to_owned();
            continue;
        } else if !pd.is_empty() {
            drive = pd.to_owned();
        }
        if !path.is_empty() && !path.ends_with(is_sep) {
            path.push('\\');
        }
        path.push_str(pp);
    }
    // the `C:` + `x` case: a drive-relative path gets no separator
    if !path.is_empty() && !path.starts_with(is_sep) && !drive.is_empty() && !drive.ends_with(':') {
        return Ok(format!("{drive}\\{path}"));
    }
    Ok(format!("{drive}{path}"))
}

/// `os.path.join(a, b)`.
pub fn join2(a: &str, b: &str) -> PyResult<String> {
    join(a, &[b])
}

/// `ntpath.split(p)`: (head, tail).
pub fn split(p: &str) -> PyResult<(String, String)> {
    let (d, rest) = splitdrive(p)?;
    let i = rest.rfind(is_sep).map_or(0, |k| k + 1);
    let (head, tail) = rest.split_at(i);
    let trimmed = head.trim_end_matches(is_sep);
    let head = if trimmed.is_empty() { head } else { trimmed };
    Ok((format!("{d}{head}"), tail.to_owned()))
}

/// `os.path.dirname(p)`.
pub fn dirname(p: &str) -> PyResult<String> {
    split(p).map(|s| s.0)
}

/// `os.path.basename(p)`.
pub fn basename(p: &str) -> PyResult<String> {
    split(p).map(|s| s.1)
}

/// `os.path.splitext(p)`: (root, ext) - the last dot of the last component that is not all leading dots.
#[must_use]
pub fn splitext(p: &str) -> (String, String) {
    let sep = p.rfind(is_sep).map_or(0, |k| k + 1);
    let name = &p[sep..];
    if let Some(dot) = name.rfind('.') {
        let lead = name.len() - name.trim_start_matches('.').len();
        if dot >= lead {
            let at = sep + dot;
            return (p[..at].to_owned(), p[at..].to_owned());
        }
    }
    (p.to_owned(), String::new())
}

/// `ntpath.normpath(p)`.
pub fn normpath(p: &str) -> PyResult<String> {
    let p = p.replace('/', "\\");
    let (drive, rest) = splitdrive(&p)?;
    let root = if rest.starts_with('\\') { "\\" } else { "" };
    let mut comps: Vec<&str> = Vec::new();
    for c in rest.split('\\') {
        match c {
            "" | "." => {}
            ".." => {
                if comps.last().is_some_and(|l| *l != "..") {
                    comps.pop();
                } else if root.is_empty() {
                    comps.push("..");
                }
            }
            c => comps.push(c),
        }
    }
    let body = comps.join("\\");
    let out = format!("{drive}{root}{body}");
    Ok(if out.is_empty() { ".".to_owned() } else { out })
}

/// `os.getcwd()`.
pub fn getcwd() -> PyResult<String> {
    std::env::current_dir()
        .ok()
        .and_then(|d| d.into_os_string().into_string().ok())
        .ok_or(Fail::Python)
}

/// `os.path.abspath(p)` (`GetFullPathNameW` on a plain path: the working folder joined, then normalized).
pub fn abspath(p: &str) -> PyResult<String> {
    if p.is_empty() {
        return normpath(&getcwd()?);
    }
    let (d, rest) = splitdrive(p)?;
    if !rest.starts_with(is_sep) {
        let cwd = getcwd()?;
        if !d.is_empty() && !cwd.to_lowercase().starts_with(&d.to_lowercase()) {
            return Err(Fail::Python); // another drive's working folder
        }
        return normpath(&join2(&cwd, rest)?);
    }
    if d.is_empty() {
        let cwd = getcwd()?;
        let (cd, _) = splitdrive(&cwd)?;
        return normpath(&format!("{cd}{rest}"));
    }
    normpath(p)
}

/// `os.path.normcase(p)` for the paths the tools meet (lower case, `/` as `\`).
#[must_use]
pub fn normcase(p: &str) -> String {
    p.replace('/', "\\").to_lowercase()
}

/// `os.path.relpath(path, start)`.
pub fn relpath(path: &str, start: &str) -> PyResult<String> {
    if path.is_empty() {
        return Err(PyErr::new("ValueError", "no path specified").into());
    }
    let a = abspath(path)?;
    let b = abspath(start)?;
    let (ad, ar) = splitdrive(&a)?;
    let (bd, br) = splitdrive(&b)?;
    if normcase(ad) != normcase(bd) {
        return Err(PyErr::new("ValueError", format!("path is on mount '{bd}', start on mount '{ad}'")).into());
    }
    let sl: Vec<&str> = br.split('\\').filter(|x| !x.is_empty()).collect();
    let pl: Vec<&str> = ar.split('\\').filter(|x| !x.is_empty()).collect();
    let mut i = 0;
    while i < sl.len() && i < pl.len() && normcase(sl[i]) == normcase(pl[i]) {
        i += 1;
    }
    let mut rel: Vec<&str> = vec![".."; sl.len() - i];
    rel.extend_from_slice(&pl[i..]);
    if rel.is_empty() {
        return Ok(".".to_owned());
    }
    Ok(rel.join("\\"))
}

/// `os.path.isdir(p)`.
#[must_use]
pub fn isdir(p: &str) -> bool {
    !p.is_empty() && Path::new(p).is_dir()
}

/// `os.path.isfile(p)`.
#[must_use]
pub fn isfile(p: &str) -> bool {
    !p.is_empty() && Path::new(p).is_file()
}

/// `os.path.exists(p)`.
#[must_use]
pub fn exists(p: &str) -> bool {
    !p.is_empty() && Path::new(p).exists()
}

/// `os.path.lexists(p)`.
#[must_use]
pub fn lexists(p: &str) -> bool {
    !p.is_empty() && fs::symlink_metadata(p).is_ok()
}

/// `os.path.getsize(p)`.
pub fn getsize(p: &str) -> PyResult<u64> {
    fs::metadata(p).map(|m| m.len()).map_err(|e| os_error(p, &e))
}

/// `os.stat(p).st_mtime_ns`.
pub fn mtime_ns(p: &str) -> PyResult<i128> {
    let m = fs::metadata(p).map_err(|e| os_error(p, &e))?;
    let t = m.modified().map_err(|_| Fail::Python)?;
    Ok(match t.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_nanos() as i128,
        Err(e) => -(e.duration().as_nanos() as i128),
    })
}

/// `os.path.getmtime(p)` (`st_mtime`: the nanoseconds as a float, as Python makes it).
pub fn getmtime(p: &str) -> PyResult<f64> {
    let ns = mtime_ns(p)?;
    // Python: _PyTime_AsSecondsDouble of the ns count - seconds and remainder added as doubles
    let secs = ns.div_euclid(1_000_000_000);
    let rem = ns.rem_euclid(1_000_000_000);
    Ok(secs as f64 + rem as f64 * 1e-9)
}

/// The names in a folder in `os.scandir`'s order (`FindFirstFile`'s), with whether each is a folder (following
/// links); an unreadable folder or a name that is not Unicode gives the error / a hand-off.
pub fn scandir(dir: &str) -> PyResult<Vec<(String, bool)>> {
    let rd = fs::read_dir(if dir.is_empty() { "." } else { dir }).map_err(|e| os_error(dir, &e))?;
    let mut out = Vec::new();
    for e in rd {
        let e = e.map_err(|_| Fail::Python)?;
        let name = e.file_name().into_string().map_err(|_| Fail::Python)?;
        let is_dir = fs::metadata(e.path()).is_ok_and(|m| m.is_dir());
        out.push((name, is_dir));
    }
    Ok(out)
}

/// `os.listdir(dir)`.
pub fn listdir(dir: &str) -> PyResult<Vec<String>> {
    Ok(scandir(dir)?.into_iter().map(|(n, _)| n).collect())
}

/// One folder of `os.walk(top)`: (path, folders, files).
pub type WalkRow = (String, Vec<String>, Vec<String>);

/// `os.walk(top)` top-down: each folder's (path, folders, files) in the listing's order; a folder that cannot be
/// listed is skipped; Windows symbolic links to folders are listed but not entered (junctions are entered:
/// `os.path.islink` is false for them).
pub fn walk(top: &str) -> PyResult<Vec<WalkRow>> {
    let mut out = Vec::new();
    walk_into(top, &mut out)?;
    Ok(out)
}

fn walk_into(top: &str, out: &mut Vec<WalkRow>) -> PyResult<()> {
    let Ok(entries) = scandir(top) else {
        return Ok(());
    };
    let (mut dirs, mut files) = (Vec::new(), Vec::new());
    for (name, is_dir) in entries {
        if is_dir {
            dirs.push(name);
        } else {
            files.push(name);
        }
    }
    out.push((top.to_owned(), dirs.clone(), files));
    for d in dirs {
        let p = join2(top, &d)?;
        if !is_symlink(&p) {
            walk_into(&p, out)?;
        }
    }
    Ok(())
}

/// `os.path.islink(p)`: a symbolic link (not a junction).
#[must_use]
pub fn is_symlink(p: &str) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileTypeExt as _;
        fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink_dir() || m.file_type().is_symlink_file())
            && !is_junction(p)
    }
    #[cfg(not(windows))]
    {
        fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink())
    }
}

#[cfg(windows)]
fn is_junction(p: &str) -> bool {
    // a junction's target as read_link reads it is \??\-prefixed; a symbolic link's is not (the tools meet no others)
    fs::read_link(p).is_ok_and(|t| t.to_string_lossy().starts_with("\\??\\"))
}

fn has_magic(s: &str) -> bool {
    s.contains(['*', '?', '['])
}

/// `fnmatch.fnmatch(name, pattern)` on Windows (both case-folded) for `*` and `?` patterns.
fn fnmatch(name: &str, pattern: &str) -> bool {
    let n: Vec<char> = normcase(name).chars().collect();
    let p: Vec<char> = normcase(pattern).chars().collect();
    let (mut i, mut j) = (0usize, 0usize);
    let (mut star, mut mark) = (None::<usize>, 0usize);
    while i < n.len() {
        if j < p.len() && (p[j] == '?' || p[j] == n[i]) {
            i += 1;
            j += 1;
        } else if j < p.len() && p[j] == '*' {
            star = Some(j);
            mark = i;
            j += 1;
        } else if let Some(s) = star {
            j = s + 1;
            mark += 1;
            i = mark;
        } else {
            return false;
        }
    }
    while j < p.len() && p[j] == '*' {
        j += 1;
    }
    j == p.len()
}

/// `glob.glob(pattern)` (not recursive) for `*` / `?` patterns: names starting with `.` match only a pattern
/// starting with `.`; `[` hands off.
pub fn glob(pattern: &str) -> PyResult<Vec<String>> {
    if pattern.contains('[') {
        return Err(Fail::Python);
    }
    glob_in(pattern, false)
}

fn glob_in(pattern: &str, dironly: bool) -> PyResult<Vec<String>> {
    let (dir, base) = split(pattern)?;
    if !has_magic(pattern) {
        let ok = if base.is_empty() { isdir(&dir) } else { lexists(pattern) };
        return Ok(if ok { vec![pattern.to_owned()] } else { Vec::new() });
    }
    if dir.is_empty() {
        return glob1("", &base, dironly);
    }
    let dirs = if dir != pattern && has_magic(&dir) {
        glob_in(&dir, true)?
    } else {
        vec![dir]
    };
    let mut out = Vec::new();
    for d in dirs {
        let names = if has_magic(&base) {
            glob1(&d, &base, dironly)?
        } else if base.is_empty() {
            if isdir(&d) { vec![base.clone()] } else { Vec::new() }
        } else if lexists(&join2(&d, &base)?) {
            vec![base.clone()]
        } else {
            Vec::new()
        };
        for n in names {
            out.push(join2(&d, &n)?);
        }
    }
    Ok(out)
}

fn glob1(dir: &str, pattern: &str, dironly: bool) -> PyResult<Vec<String>> {
    let Ok(entries) = scandir(dir) else {
        return Ok(Vec::new());
    };
    let hidden = pattern.starts_with('.');
    Ok(entries
        .into_iter()
        .filter(|(n, d)| (!dironly || *d) && (hidden || !n.starts_with('.')) && fnmatch(n, pattern))
        .map(|(n, _)| n)
        .collect())
}

/// `os.makedirs(p, exist_ok=True)`.
pub fn makedirs(p: &str) -> PyResult<()> {
    if isdir(p) {
        return Ok(());
    }
    fs::create_dir_all(p).map_err(|e| os_error(p, &e))
}

/// `shutil.rmtree(p, ignore_errors=True)`.
pub fn rmtree_quiet(p: &str) {
    let _ = fs::remove_dir_all(p);
}

/// `open(path, 'rb').read()`.
pub fn read(path: &str) -> PyResult<Vec<u8>> {
    fs::read(path).map_err(|e| os_error(path, &e))
}

/// `open(path, 'w').write(text)` (the ANSI code page; `\n` as `\r\n`): ASCII text only, else a hand-off.
pub fn write_text_plain(path: &str, text: &str) -> PyResult<()> {
    if !text.is_ascii() {
        return Err(Fail::Python);
    }
    fs::write(path, text.replace('\n', "\r\n")).map_err(|e| os_error(path, &e))
}

/// `open(path, 'w', encoding='utf-8').write(text)`.
pub fn write_text_utf8(path: &str, text: &str) -> PyResult<()> {
    fs::write(path, text.replace('\n', "\r\n")).map_err(|e| os_error(path, &e))
}

/// `atomicio.write_bytes(path, data)` ([`files::write_atomic`]): `<path>.tmp-<pid>-<n>` written and synced, then
/// moved over the target (tried again while another process holds it for a moment).
pub fn atomic_write(path: &str, data: &[u8]) -> PyResult<()> {
    files::write_atomic(path, data).map_err(|(at, e)| os_error(&at, &e))
}

/// `atomicio.write_text(path, text)` (UTF-8, `\n` as `\r\n`).
pub fn atomic_write_text(path: &str, text: &str) -> PyResult<()> {
    atomic_write(path, text.replace('\n', "\r\n").as_bytes())
}

/// `atomicio.copy_file(src, dst)`.
pub fn atomic_copy(src: &str, dst: &str) -> PyResult<()> {
    let data = read(src)?;
    atomic_write(dst, &data)
}

/// `hashlib.sha256(open(path, 'rb').read()).hexdigest()`, read in 1 MB blocks.
pub fn sha256_file(path: &str) -> PyResult<String> {
    let mut f = fs::File::open(path).map_err(|e| os_error(path, &e))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).map_err(|e| os_error(path, &e))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.hex())
}

/// SHA-256 (FIPS 180-4).
pub struct Sha256 {
    state: [u32; 8],
    block: Vec<u8>,
    len: u64,
}

const K: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256 {
    /// A fresh hash.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: [
                0x6a09_e667,
                0xbb67_ae85,
                0x3c6e_f372,
                0xa54f_f53a,
                0x510e_527f,
                0x9b05_688c,
                0x1f83_d9ab,
                0x5be0_cd19,
            ],
            block: Vec::with_capacity(64),
            len: 0,
        }
    }

    fn compress(state: &mut [u32; 8], chunk: &[u8]) {
        let mut w = [0u32; 64];
        for (i, c) in chunk.chunks_exact(4).enumerate().take(16) {
            w[i] = u32::from_be_bytes([c[0], c[1], c[2], c[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (s, v) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *s = s.wrapping_add(v);
        }
    }

    /// More bytes.
    pub fn update(&mut self, mut data: &[u8]) {
        self.len += data.len() as u64;
        if !self.block.is_empty() {
            let take = (64 - self.block.len()).min(data.len());
            self.block.extend_from_slice(&data[..take]);
            data = &data[take..];
            if self.block.len() == 64 {
                let b = std::mem::take(&mut self.block);
                Self::compress(&mut self.state, &b);
            }
        }
        let whole = data.len() / 64 * 64;
        let (blocks, rest) = data.split_at(whole);
        if !blocks.is_empty() && !shani::compress(&mut self.state, blocks) {
            for c in blocks.chunks_exact(64) {
                Self::compress(&mut self.state, c);
            }
        }
        self.block.extend_from_slice(rest);
    }

    /// The digest as lower-case hex.
    #[must_use]
    pub fn hex(mut self) -> String {
        let bits = self.len.wrapping_mul(8);
        let mut tail = std::mem::take(&mut self.block);
        tail.push(0x80);
        while tail.len() % 64 != 56 {
            tail.push(0);
        }
        tail.extend_from_slice(&bits.to_be_bytes());
        for c in tail.chunks_exact(64) {
            Self::compress(&mut self.state, c);
        }
        let mut out = String::with_capacity(64);
        for v in self.state {
            let _ = write!(out, "{v:08x}");
        }
        out
    }
}

/// SHA-256's block function on the CPU's SHA extensions (as `OpenSSL`, which `hashlib` uses, runs it): about four
/// times the portable rounds on the bundles NavKit stamps (hundreds of MB). Detected at run time; `compress`
/// returns false where the CPU has none (the portable rounds then run). The instruction sequence is the one Intel
/// documents (and the `sha2` crate uses); the digests are checked against the portable rounds in the tests.
#[allow(unsafe_code)]
mod shani {
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::{
        __m128i, _mm_add_epi32, _mm_alignr_epi8, _mm_blend_epi16, _mm_loadu_si128, _mm_set_epi32, _mm_set_epi64x,
        _mm_sha256msg1_epu32, _mm_sha256msg2_epu32, _mm_sha256rnds2_epu32, _mm_shuffle_epi8, _mm_shuffle_epi32,
        _mm_storeu_si128,
    };

    /// The blocks (a multiple of 64 bytes) into `state`; false: no SHA extensions here.
    pub fn compress(state: &mut [u32; 8], blocks: &[u8]) -> bool {
        #[cfg(target_arch = "x86_64")]
        {
            if std::arch::is_x86_feature_detected!("sha")
                && std::arch::is_x86_feature_detected!("sse4.1")
                && std::arch::is_x86_feature_detected!("ssse3")
            {
                // SAFETY: the features the function is compiled for were detected just above
                unsafe { blocks_ni(state, blocks) };
                return true;
            }
        }
        let _ = (state, blocks);
        false
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "sha,sse2,ssse3,sse4.1")]
    fn schedule(v0: __m128i, v1: __m128i, v2: __m128i, v3: __m128i) -> __m128i {
        let t1 = _mm_sha256msg1_epu32(v0, v1);
        let t2 = _mm_alignr_epi8(v3, v2, 4);
        let t3 = _mm_add_epi32(t1, t2);
        _mm_sha256msg2_epu32(t3, v3)
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "sha,sse2,ssse3,sse4.1")]
    fn rounds4(abef: &mut __m128i, cdgh: &mut __m128i, w: __m128i, i: usize) {
        let k = &super::K[4 * i..4 * i + 4];
        let kv = _mm_set_epi32(k[3] as i32, k[2] as i32, k[1] as i32, k[0] as i32);
        let t1 = _mm_add_epi32(w, kv);
        *cdgh = _mm_sha256rnds2_epu32(*cdgh, *abef, t1);
        let t2 = _mm_shuffle_epi32(t1, 0x0E);
        *abef = _mm_sha256rnds2_epu32(*abef, *cdgh, t2);
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "sha,sse2,ssse3,sse4.1")]
    #[allow(clippy::cast_ptr_alignment)] // only the unaligned loads and stores (loadu / storeu) use the pointers
    unsafe fn blocks_ni(state: &mut [u32; 8], blocks: &[u8]) {
        let mask = _mm_set_epi64x(0x0C0D_0E0F_0809_0A0B_u64 as i64, 0x0405_0607_0001_0203_u64 as i64);
        let sp = state.as_ptr().cast::<__m128i>();
        // SAFETY: state is 32 bytes: two unaligned 16-byte loads
        let (dcba, efgh) = unsafe { (_mm_loadu_si128(sp), _mm_loadu_si128(sp.add(1))) };
        let cdab = _mm_shuffle_epi32(dcba, 0xB1);
        let efgh = _mm_shuffle_epi32(efgh, 0x1B);
        let mut abef = _mm_alignr_epi8(cdab, efgh, 8);
        let mut cdgh = _mm_blend_epi16(efgh, cdab, 0xF0);
        for block in blocks.chunks_exact(64) {
            let (abef_save, cdgh_save) = (abef, cdgh);
            let dp = block.as_ptr().cast::<__m128i>();
            // SAFETY: the block is 64 bytes: four unaligned 16-byte loads
            let mut w = unsafe {
                [
                    _mm_shuffle_epi8(_mm_loadu_si128(dp), mask),
                    _mm_shuffle_epi8(_mm_loadu_si128(dp.add(1)), mask),
                    _mm_shuffle_epi8(_mm_loadu_si128(dp.add(2)), mask),
                    _mm_shuffle_epi8(_mm_loadu_si128(dp.add(3)), mask),
                ]
            };
            for (i, wi) in w.iter().enumerate() {
                rounds4(&mut abef, &mut cdgh, *wi, i);
            }
            for i in 4..16 {
                let next = schedule(w[0], w[1], w[2], w[3]);
                w = [w[1], w[2], w[3], next];
                rounds4(&mut abef, &mut cdgh, next, i);
            }
            abef = _mm_add_epi32(abef, abef_save);
            cdgh = _mm_add_epi32(cdgh, cdgh_save);
        }
        let feba = _mm_shuffle_epi32(abef, 0x1B);
        let dchg = _mm_shuffle_epi32(cdgh, 0xB1);
        let dcba = _mm_blend_epi16(feba, dchg, 0xF0);
        let hgef = _mm_alignr_epi8(dchg, feba, 8);
        let mp = state.as_mut_ptr().cast::<__m128i>();
        // SAFETY: state is 32 bytes: two unaligned 16-byte stores
        unsafe {
            _mm_storeu_si128(mp, dcba);
            _mm_storeu_si128(mp.add(1), hgef);
        }
    }
}

/// `hashlib.sha256(data).hexdigest()`.
#[must_use]
pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    h.hex()
}

/// `hashlib.sha1(data).hexdigest()`.
#[must_use]
pub fn sha1_hex(data: &[u8]) -> String {
    sha1_smol::Sha1::from(data).digest().to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn sha256_extensions_as_the_portable_rounds() {
        let mut x: u32 = 12345;
        let data: Vec<u8> = (0..64 * 97)
            .map(|_| {
                x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
                (x >> 16) as u8
            })
            .collect();
        let mut a = [
            0x6a09_e667,
            0xbb67_ae85,
            0x3c6e_f372,
            0xa54f_f53a,
            0x510e_527f,
            0x9b05_688c,
            0x1f83_d9ab,
            0x5be0_cd19,
        ];
        let mut b = a;
        for c in data.chunks_exact(64) {
            Sha256::compress(&mut a, c);
        }
        if shani::compress(&mut b, &data) {
            assert_eq!(a, b);
        }
    }

    #[test]
    fn sha256_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let long = vec![b'a'; 1_000_003];
        let mut h = Sha256::new();
        for c in long.chunks(777) {
            h.update(c);
        }
        assert_eq!(h.hex(), sha256_hex(&long));
    }

    #[test]
    fn paths_as_ntpath() {
        assert_eq!(join("C:\\a", &["b", "c.txt"]).unwrap(), "C:\\a\\b\\c.txt");
        assert_eq!(join("C:\\a\\", &["b"]).unwrap(), "C:\\a\\b");
        assert_eq!(join("a", &["C:\\b"]).unwrap(), "C:\\b");
        assert_eq!(join("C:", &["b"]).unwrap(), "C:b");
        assert_eq!(normpath("C:/a/./b/../c/").unwrap(), "C:\\a\\c");
        assert_eq!(
            relpath("C:\\x\\work\\levels\\sp_01", "C:\\x").unwrap(),
            "work\\levels\\sp_01"
        );
        assert_eq!(relpath("C:\\x\\a", "C:\\x\\b\\c").unwrap(), "..\\..\\a");
        assert_eq!(relpath("C:\\X", "c:\\x").unwrap(), ".");
        assert_eq!(splitext("a\\b.c\\d.xml"), ("a\\b.c\\d".to_owned(), ".xml".to_owned()));
        assert_eq!(splitext("a\\.hidden"), ("a\\.hidden".to_owned(), String::new()));
        assert!(fnmatch("SP_01.XML", "*.xml"));
        assert!(fnmatch("a_HavokAsset.xml", "*_havokasset.xml"));
        assert!(!fnmatch("a.xm", "*.xml"));
    }
}
