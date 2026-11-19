//! FbrbTool's commands in process: a port of FbrbTool.cs (`unpack`, `pack`, `list`, `big`) that gives FbrbTool's
//! output byte for byte - the same files, messages and exit codes - without starting .NET. FbrbTool.exe was retired in
//! round R4 (2026-11-18; its source is in the git history): `core/tests/fbrbtool_golden*.json` hold its answers.
//!
//! | `FbrbTool.cs` | here |
//! |---|---|
//! | `Main`, `ToolLog.Fail` / `Explain` | [`run`], [`run_many`] ([`Outcome`]) |
//! | `Read` (`File.ReadAllBytes`, `Gunzip`: .NET's `GZipStream`) | `read_bundle` ([`super::netgzip`]) |
//! | `Unpack`, `LayoutTokens`, `Safe` | `unpack`, `layout_tokens`, `safe` |
//! | `Pack` | `pack` (the bytes by [`super::pack_rows`], the packer proven in round E1b) |
//! | `List`, `BigExtract` | `list`, `big` |
//!
//! Every step fails where FbrbTool's fails, with the exception FbrbTool's would throw (.NET's message and
//! `ToolLog.Explain`'s words): the header is read with C#'s 32-bit wrapping arithmetic and array bounds, entries are
//! written in order, an entry whose data lies outside the payload leaves the empty file `File.Create` made. The
//! difference: files are written beside their name and moved over it ([`crate::atomic::replace`]), so a run cut short
//! never leaves a half-written file (FbrbTool's own writes of entries and the manifest are not atomic).
//!
//! Memory: a zipped payload is decompressed from the file as it is read (zlib; .NET's own reader runs only on a
//! stream zlib refuses), and a stored payload is not loaded at all - each entry is read from the file where it is.

use std::borrow::Cow;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rayon::prelude::*;

use super::netgzip::{self, NetGzipError};
use super::pack::{
    Layout, Manifest, ManifestLine, Padding, SourceRow, StreamError, decode_text, lines, pack_rows_stream, token,
};
use super::unpack::UnpackStats;
use crate::dbxedit::{Fault, FaultKind, net};

type Result<T, E = Fault> = std::result::Result<T, E>;

/// What a run of a command printed, its exit code, and the lines FbrbTool would have logged.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    /// The exit code: 0 done, 1 failed, 2 a usage error.
    pub code: i32,
    /// Standard output, each line ended by `\n`.
    pub stdout: String,
    /// Standard error, each line ended by `\n`.
    pub stderr: String,
    /// `(level, message)` lines for FbrbTool's log (`ERROR`).
    pub log: Vec<(String, String)>,
}

impl Outcome {
    fn out(&mut self, line: &str) {
        self.stdout.push_str(line);
        self.stdout.push('\n');
    }

    fn err(&mut self, line: &str) {
        self.stderr.push_str(line);
        self.stderr.push('\n');
    }
}

const USAGE: &str = "usage: FbrbTool unpack <file.fbrb> <outdir> | pack <dir> <file.fbrb> | list <file.fbrb>";
const TOO_LONG: &str = "The specified path, file name, or both are too long. The fully qualified file name must be less \
                        than 260 characters, and the directory name must be less than 248 characters.";
const DELETED: &str = super::DELETED;

/// FbrbTool's `ToolLog.Explain`: DbxEdit's, with a word for a path too long first.
fn explain(f: &Fault) -> (String, &'static str) {
    if f.kind == FaultKind::PathTooLong {
        return (
            "an entry's path in the output folder is longer than Windows allows (260 characters)".to_owned(),
            "Unpack into a folder with a shorter path (e.g. C:\\work\\x).",
        );
    }
    f.explain()
}

/// Runs FbrbTool's command line `args` (without the program name) in process: the same files written, the same
/// messages and exit code. `log_path` is the log file named in an unexpected error's message (FbrbTool's
/// `%LOCALAPPDATA%\bfbc2-tool-suite\logs\fbrbtool-<date>.log`).
#[must_use]
pub fn run(args: &[String], log_path: Option<&str>) -> Outcome {
    let mut o = Outcome::default();
    o.code = match dispatch(args, &mut o) {
        Ok(code) => code,
        Err(f) => {
            // ToolLog.Fail: logged and explained, exit code 1
            let (cause, todo) = explain(&f);
            o.log.push((
                "ERROR".into(),
                format!("fbrbtool error: {cause}\r\n{}: {}", f.type_name(), f.message),
            ));
            o.err(&format!("error: {cause}"));
            o.err(&format!("  what to do: {todo}"));
            o.err(&format!("  details: {}", log_path.unwrap_or("(no log file)")));
            1
        }
    };
    o
}

/// [`run`] of many command lines on all cores, in order. Runs that write the same file or folder must not be in one
/// batch.
#[must_use]
pub fn run_many(runs: &[Vec<String>], log_path: Option<&str>) -> Vec<Outcome> {
    runs.par_iter().map(|a| run(a, log_path)).collect()
}

fn dispatch(args: &[String], o: &mut Outcome) -> Result<i32> {
    match args {
        [c, a, b] if c == "big" => big(a, b, o),
        [c, a, b] if c == "unpack" => unpack(a, b, o),
        [c, a, b] if c == "pack" => pack(a, b, o),
        [c, a] if c == "list" => list(a, o),
        _ => {
            o.err(USAGE);
            Ok(2)
        }
    }
}

// ------------------------------------------------------------------------------------------- .NET's paths and files

fn argument(m: &str) -> Fault {
    Fault::new(FaultKind::Argument, m)
}

fn index() -> Fault {
    Fault::index()
}

/// `Path.CheckInvalidPathChars`: `"`, `<`, `>`, `|` and the control characters.
fn check_chars(p: &str) -> Result<()> {
    if p.chars().any(|c| matches!(c, '"' | '<' | '>' | '|') || (c as u32) < 32) {
        return Err(argument("Illegal characters in path."));
    }
    Ok(())
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// `Path.GetFullPath`: the characters checked (wildcards too), the path made full as .NET Framework's
/// `NormalizePath` makes it ([`normalize`]), 260 characters at most.
fn full_path(p: &str) -> Result<String> {
    check_chars(p)?;
    if p.trim().is_empty() {
        return Err(argument("The path is not of a legal form."));
    }
    if p.contains(['?', '*']) {
        return Err(argument("Illegal characters in path."));
    }
    let full = normalize(&absolute(p)?)?;
    if utf16_len(&full) >= 260 {
        return Err(Fault::new(FaultKind::PathTooLong, TOO_LONG));
    }
    Ok(full)
}

/// `p` on the current folder (or its drive) when it is not absolute, not yet normalized.
fn absolute(p: &str) -> Result<String> {
    let b = p.as_bytes();
    let is_sep = |c: Option<&u8>| matches!(c, Some(b'\\' | b'/'));
    if b.get(1) == Some(&b':') && is_sep(b.get(2)) || is_sep(b.first()) && is_sep(b.get(1)) {
        return Ok(p.to_owned());
    }
    let cwd = std::env::current_dir().map_err(|e| Fault::io(&e, Path::new(p)))?;
    let cwd = cwd.to_string_lossy().into_owned();
    let drive = cwd.get(..2).unwrap_or("C:").to_owned();
    if is_sep(b.first()) {
        return Ok(format!("{drive}{p}"));
    }
    if b.get(1) == Some(&b':') {
        let rest = p.get(2..).unwrap_or("");
        return Ok(if p.get(..2).is_some_and(|d| d.eq_ignore_ascii_case(&drive)) {
            format!("{cwd}\\{rest}")
        } else {
            format!("{}\\{rest}", p.get(..2).unwrap_or(""))
        });
    }
    Ok(format!("{cwd}\\{p}"))
}

/// .NET Framework's `Path.NormalizePath` of an absolute path: `/` as `\`, empty and `.` components dropped, `..`
/// (and any run of three or more dots) going up, trailing dots and spaces cut from each component, a component of
/// spaces dropped, one of dots and spaces that does not start with a dot or has a space inside refused; a path whose
/// last component was empty or spaces keeps its final separator.
fn normalize(abs: &str) -> Result<String> {
    let (root, rest) = if abs.starts_with("\\\\") || abs.starts_with("//") {
        // \\server\share\ is the root
        let body = abs.get(2..).unwrap_or("");
        let mut it = body.splitn(3, ['\\', '/']);
        let (server, share) = (it.next().unwrap_or(""), it.next().unwrap_or(""));
        (format!("\\\\{server}\\{share}\\"), it.next().unwrap_or("").to_owned())
    } else {
        (
            format!("{}\\", abs.get(..2).unwrap_or("")),
            abs.get(3..).unwrap_or("").to_owned(),
        )
    };
    let comps: Vec<&str> = rest.split(['\\', '/']).collect();
    let mut parts: Vec<String> = Vec::new();
    let mut trailing = false;
    for (i, c) in comps.iter().enumerate() {
        let last = i + 1 == comps.len();
        let trimmed = c.trim_end_matches([' ', '.']);
        if trimmed.is_empty() {
            let core = c.trim_end_matches(' ');
            if core.is_empty() {
                // empty, or spaces only
                trailing = last && i > 0;
                continue;
            }
            if !core.starts_with('.') || core.contains(' ') {
                return Err(argument("The path is not of a legal form."));
            }
            if core.len() >= 2 {
                parts.pop();
            }
            trailing = false;
            continue;
        }
        parts.push(trimmed.to_owned());
        trailing = false;
    }
    let mut out = root;
    out.push_str(&parts.join("\\"));
    if trailing && !parts.is_empty() {
        out.push('\\');
    }
    Ok(out)
}

/// `Path.Combine(a, b)` (both checked for invalid characters; `b` rooted wins).
fn combine(a: &str, b: &str) -> Result<String> {
    check_chars(a)?;
    check_chars(b)?;
    if b.is_empty() {
        return Ok(a.to_owned());
    }
    if a.is_empty() || is_rooted(b) {
        return Ok(b.to_owned());
    }
    Ok(if a.ends_with(['\\', '/', ':']) {
        format!("{a}{b}")
    } else {
        format!("{a}\\{b}")
    })
}

/// `Path.IsPathRooted` (after its character check).
fn is_rooted(p: &str) -> bool {
    p.starts_with(['\\', '/']) || p.as_bytes().get(1) == Some(&b':')
}

/// `Path.GetDirectoryName` of a full path.
fn directory_name(full: &str) -> String {
    match full.rfind(['\\', '/']) {
        Some(i) => {
            let d = full.get(..i).unwrap_or("");
            // a root keeps its separator (C:\)
            if d.len() == 2 && d.ends_with(':') {
                format!("{d}\\")
            } else {
                d.to_owned()
            }
        }
        None => String::new(),
    }
}

/// FbrbTool's `Safe(root, name)`: the full path of an entry, refused when it is empty, rooted or leads out of root.
fn safe(root: &str, archive_name: &str) -> Result<String> {
    if archive_name.is_empty() {
        return Err(Fault::invalid("empty entry name in the bundle"));
    }
    let name = archive_name.replace('/', "\\");
    check_chars(&name)?;
    if is_rooted(&name) {
        return Err(Fault::invalid(format!(
            "entry escapes the output folder: {archive_name}"
        )));
    }
    let root_full = format!("{}\\", full_path(root)?.trim_end_matches('\\'));
    let full = full_path(&combine(root, &name)?)?;
    if !net::same_ignore_case(full.get(..root_full.len()).unwrap_or(""), &root_full) {
        return Err(Fault::invalid(format!(
            "entry escapes the output folder: {archive_name}"
        )));
    }
    Ok(full)
}

/// `Directory.CreateDirectory(path)`.
fn create_directory(path: &str) -> Result<()> {
    if path.trim().is_empty() {
        return Err(argument("Path cannot be the empty string or all whitespace."));
    }
    let full = full_path(path)?;
    if utf16_len(full.trim_end_matches('\\')) >= 248 {
        return Err(Fault::new(FaultKind::PathTooLong, TOO_LONG));
    }
    // each missing folder from the top; a file where a folder must be is .NET's IOException
    let mut missing = Vec::new();
    let mut p = Path::new(&full);
    loop {
        match fs::metadata(p) {
            Ok(m) if m.is_dir() => break,
            Ok(_) => {
                let shown = p.to_string_lossy();
                if p == Path::new(&full) {
                    return Err(Fault::new(
                        FaultKind::Io,
                        format!(
                            "Cannot create \"{shown}\" because a file or directory with the same name already exists."
                        ),
                    ));
                }
                // a file part-way up: Windows says the path is not found
                return Err(Fault::new(
                    FaultKind::DirectoryNotFound,
                    format!("Could not find a part of the path '{full}'."),
                ));
            }
            Err(_) => {}
        }
        missing.push(p.to_path_buf());
        match p.parent() {
            Some(q) if q != p => p = q,
            _ => break,
        }
    }
    for d in missing.iter().rev() {
        if let Err(e) = fs::create_dir(d)
            && !d.is_dir()
        {
            return Err(Fault::io(&e, d));
        }
    }
    Ok(())
}

/// `File.ReadAllBytes`' checks before it reads: the path, the file, its length (2 GB at most).
fn open_for_read(path: &str) -> Result<(File, u64)> {
    if path.is_empty() {
        return Err(argument("Empty path name is not legal."));
    }
    let full = full_path(path)?;
    let f = open_shared(Path::new(&full)).map_err(|e| Fault::io(&e, Path::new(&full)))?;
    let len = f.metadata().map_err(|e| Fault::io(&e, Path::new(&full)))?.len();
    if len > i32::MAX as u64 {
        return Err(Fault::new(
            FaultKind::Io,
            "The file is too long. This operation is currently limited to supporting files less than 2 gigabytes in size.",
        ));
    }
    Ok((f, len))
}

/// A file opened for reading as `File.ReadAllBytes` opens it (`FileShare.Read`: refused while another program has
/// it open for writing).
fn open_shared(p: &Path) -> io::Result<File> {
    let mut o = fs::OpenOptions::new();
    o.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        o.share_mode(1); // FILE_SHARE_READ
    }
    let f = o.open(p)?;
    if f.metadata()?.is_dir() {
        return Err(io::Error::from(io::ErrorKind::PermissionDenied));
    }
    Ok(f)
}

fn read_all(path: &str) -> Result<Vec<u8>> {
    let (mut f, len) = open_for_read(path)?;
    let mut v = Vec::with_capacity(len as usize);
    f.read_to_end(&mut v).map_err(|e| Fault::io(&e, Path::new(path)))?;
    Ok(v)
}

static WRITES: AtomicU64 = AtomicU64::new(0);

/// `target` spelled as the file already there is (Windows matches names without case): `File.Create` keeps an
/// existing file's name, where a move onto another spelling would rename it (`pack`'s `File.Replace` takes the new
/// spelling: it moves onto `target` as given).
fn on_disk_name(target: &Path) -> PathBuf {
    if fs::symlink_metadata(target).is_err() {
        return target.to_path_buf();
    }
    match (fs::canonicalize(target), target.parent()) {
        (Ok(real), Some(parent)) => real
            .file_name()
            .map_or_else(|| target.to_path_buf(), |n| parent.join(n)),
        _ => target.to_path_buf(),
    }
}

/// A file written beside `target` and moved over it.
fn write_atomic(target: &str, data: &[&[u8]]) -> Result<()> {
    let n = WRITES.fetch_add(1, Ordering::Relaxed);
    let tmp = format!("{target}.tmp-{}-{n}", std::process::id());
    let written = File::create(&tmp).and_then(|mut f| {
        for d in data {
            f.write_all(d)?;
        }
        Ok(())
    });
    if let Err(e) = written {
        let _ = fs::remove_file(&tmp);
        // as File.Create(target) would have failed
        return Err(Fault::io(&e, Path::new(target)));
    }
    crate::atomic::replace(Path::new(&tmp), &on_disk_name(Path::new(target))).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        Fault::io(&e, Path::new(target))
    })
}

/// `File.Create(target)` then `FileStream.Write(data, offset, count)`: the checks of `Write` after the empty file is
/// made, the data written (atomically) when they pass.
fn create_and_write(target: &str, payload: &Payload, offset: i32, count: i32) -> Result<()> {
    if target.ends_with('\\') {
        return Err(Fault::new(
            FaultKind::DirectoryNotFound,
            format!("Could not find a part of the path '{target}'."),
        ));
    }
    let checked = payload.slice(offset, count);
    match checked {
        Ok(data) => write_atomic(target, &[&data]),
        Err(f) => {
            File::create(target).map_err(|e| Fault::io(&e, Path::new(target)))?;
            Err(f)
        }
    }
}

// ------------------------------------------------------------------------------------------- reading a bundle

/// A bundle's payload: in memory (decompressed), or where it is in the bundle file (stored).
enum Payload {
    Memory(Vec<u8>),
    File { file: File, start: u64, len: usize },
}

impl Payload {
    fn len(&self) -> usize {
        match self {
            Self::Memory(v) => v.len(),
            Self::File { len, .. } => *len,
        }
    }

    /// `count` bytes from `offset`, with `FileStream.Write`'s checks.
    fn slice(&self, offset: i32, count: i32) -> Result<Cow<'_, [u8]>> {
        let (o, c) = self.check(offset, count)?;
        self.bytes(o, c)
    }

    /// `FileStream.Write(payload, offset, count)`'s checks: the range as `usize`s.
    fn check(&self, offset: i32, count: i32) -> Result<(usize, usize)> {
        if offset < 0 {
            return Err(Fault::new(
                FaultKind::ArgumentOutOfRange,
                "Non-negative number required.\nParameter name: offset",
            ));
        }
        if count < 0 {
            return Err(Fault::new(
                FaultKind::ArgumentOutOfRange,
                "Non-negative number required.\nParameter name: count",
            ));
        }
        let (o, c) = (offset as usize, count as usize);
        if self.len() - o.min(self.len()) < c || o > self.len() {
            return Err(argument(
                "Offset and length were out of bounds for the array or count is greater than the number of elements \
                 from index to the end of the source collection.",
            ));
        }
        Ok((o, c))
    }

    fn bytes(&self, o: usize, c: usize) -> Result<Cow<'_, [u8]>> {
        match self {
            Self::Memory(v) => Ok(Cow::Borrowed(v.get(o..o + c).unwrap_or_default())),
            Self::File { file, start, .. } => {
                let mut buf = vec![0u8; c];
                read_at(file, start + o as u64, &mut buf).map_err(|e| Fault::new(FaultKind::Io, e.to_string()))?;
                Ok(Cow::Owned(buf))
            }
        }
    }
}

/// `buf.len()` bytes of `file` from `at`, without moving a shared position (entries are read on all cores).
fn read_at(file: &File, at: u64, buf: &mut [u8]) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        let mut done = 0;
        while done < buf.len() {
            let n = file.seek_read(buf.get_mut(done..).unwrap_or_default(), at + done as u64)?;
            if n == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            done += n;
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::FileExt;
        file.read_exact_at(buf, at)
    }
}

struct Entry {
    name: String,
    type_name: String,
    flags: u32,
    offset: u32,
    size: u32,
    size2: u32,
}

impl Entry {
    fn is_deleted(&self) -> bool {
        self.type_name == DELETED && self.size == 0
    }
}

struct Bundle {
    entries: Vec<Entry>,
    payload: Payload,
    zipped: bool,
    payload_field: u32,
    header_region: i32,
}

/// `ReadBE`: `b[o..o+4]` big-endian, `IndexOutOfRangeException` outside it.
fn read_be(b: &[u8], o: i32) -> Result<u32> {
    let o = usize::try_from(o).map_err(|_| index())?;
    let s: [u8; 4] = b
        .get(o..o.checked_add(4).ok_or_else(index)?)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(index)?;
    Ok(u32::from_be_bytes(s))
}

/// `Encoding.ASCII.GetString`: each byte past 0x7F as `?`.
fn ascii(b: &[u8]) -> String {
    b.iter().map(|&c| if c < 0x80 { c as char } else { '?' }).collect()
}

/// `CString(b, o)`: up to the next zero byte, `IndexOutOfRangeException` when there is none.
fn cstring(b: &[u8], o: i32) -> Result<String> {
    let o = usize::try_from(o).map_err(|_| index())?;
    let rest = b.get(o..).ok_or_else(index)?;
    let end = rest.iter().position(|&c| c == 0).ok_or_else(index)?;
    Ok(ascii(rest.get(..end).unwrap_or_default()))
}

fn gzip_fault(e: NetGzipError) -> Fault {
    match e {
        NetGzipError::InvalidData(m) => Fault::invalid(m),
        NetGzipError::IndexOutOfRange => index(),
    }
}

/// `new MemoryStream(data, index, count)`'s checks for a region of a file of `len` bytes.
fn region(len: usize, index: i32, count: i32) -> Result<()> {
    if index < 0 {
        return Err(Fault::new(
            FaultKind::ArgumentOutOfRange,
            "Non-negative number required.\nParameter name: index",
        ));
    }
    if count < 0 {
        return Err(Fault::new(
            FaultKind::ArgumentOutOfRange,
            "Non-negative number required.\nParameter name: count",
        ));
    }
    if (len as i64) - i64::from(index) < i64::from(count) {
        return Err(argument(
            "Offset and length were out of bounds for the array or count is greater than the number of elements from \
             index to the end of the source collection.",
        ));
    }
    Ok(())
}

/// FbrbTool's `Read(path, ...)`.
fn read_bundle(path: &str) -> Result<Bundle> {
    let (mut f, len) = open_for_read(path)?;
    let io = |e: io::Error| Fault::new(FaultKind::Io, e.to_string());
    let mut first = [0u8; 8];
    let got = f.read(&mut first).map_err(io)?;
    let mut got = got;
    while got < 8 && got < len as usize {
        let n = f.read(first.get_mut(got..).unwrap_or_default()).map_err(io)?;
        if n == 0 {
            break;
        }
        got += n;
    }
    if len < 8 || first.get(..4) != Some(b"FbRB".as_slice()) {
        return Err(Fault::invalid(format!("{path} is not an FbRB bundle")));
    }
    let len = len as usize;
    let [_, _, _, _, c0, c1, c2, c3] = first;
    let cut = i32::from_be_bytes([c0, c1, c2, c3]);
    region(len, 8, cut)?;
    let mut hz = vec![0u8; cut as usize];
    f.read_exact(&mut hz).map_err(io)?;
    let h = netgzip::gunzip(&hz).map_err(gzip_fault)?;
    drop(hz);

    let (entries, zipped, payload_field) = parse_header(&h)?;
    let header_region = 8i32.wrapping_add(cut);
    let rest = len - 8 - cut as usize;
    let start = header_region as u64;
    let payload = if zipped {
        read_zipped(&f, len, start, rest)?
    } else {
        Payload::File {
            file: f,
            start,
            len: rest,
        }
    };
    Ok(Bundle {
        entries,
        payload,
        zipped,
        payload_field,
        header_region,
    })
}

/// The decompressed header as FbrbTool's `Read` takes it apart: C#'s 32-bit wrapping arithmetic, its array bounds.
fn parse_header(h: &[u8]) -> Result<(Vec<Entry>, bool, u32)> {
    const BASE: i32 = 8;
    let strlen = read_be(h, 4)? as i32;
    let count = read_be(h, BASE.wrapping_add(strlen))? as i32;
    if count < 0 {
        return Err(Fault::new(
            FaultKind::ArgumentOutOfRange,
            "Non-negative number required.\nParameter name: capacity",
        ));
    }
    if count > 0x0FFF_FFFF {
        // new List<Entry>(count): an array of references past 2 GB
        return Err(Fault::new(
            FaultKind::OutOfMemory,
            "Array dimensions exceeded supported range.",
        ));
    }
    let mut entries = Vec::new();
    for i in 0..count {
        let e = BASE
            .wrapping_add(strlen)
            .wrapping_add(4)
            .wrapping_add(i.wrapping_mul(24));
        let at = |k: i32| read_be(h, e.wrapping_add(k));
        entries.push(Entry {
            name: cstring(h, BASE.wrapping_add(at(0)? as i32))?,
            flags: at(4)?,
            offset: at(8)?,
            size: at(12)?,
            size2: at(16)?,
            type_name: cstring(h, BASE.wrapping_add(at(20)? as i32))?,
        });
    }
    let tail = BASE
        .wrapping_add(strlen)
        .wrapping_add(4)
        .wrapping_add(count.wrapping_mul(24));
    let zipped = *h.get(usize::try_from(tail).map_err(|_| index())?).ok_or_else(index)? != 0;
    let payload_field = if (h.len() as i32) >= tail.wrapping_add(5) {
        read_be(h, tail.wrapping_add(1))?
    } else {
        0
    };
    Ok((entries, zipped, payload_field))
}

/// FbrbTool's `Read` of a bundle given as bytes (`label` names it in an error, as the path would).
fn bundle_from_bytes(label: &str, data: &[u8]) -> Result<Bundle> {
    if data.len() < 8 || data.get(..4) != Some(b"FbRB".as_slice()) {
        return Err(Fault::invalid(format!("{label} is not an FbRB bundle")));
    }
    if data.len() > i32::MAX as usize {
        return Err(Fault::new(
            FaultKind::Io,
            "The file is too long. This operation is currently limited to supporting files less than 2 gigabytes in size.",
        ));
    }
    let cut = data
        .get(4..8)
        .and_then(|b| <[u8; 4]>::try_from(b).ok())
        .map_or(0, i32::from_be_bytes);
    region(data.len(), 8, cut)?;
    let start = 8 + cut as usize;
    let h = netgzip::gunzip(data.get(8..start).unwrap_or_default()).map_err(gzip_fault)?;
    let (entries, zipped, payload_field) = parse_header(&h)?;
    let rest = data.get(start..).unwrap_or_default();
    let payload = if zipped {
        netgzip::gunzip(rest).map_err(gzip_fault)?
    } else {
        rest.to_vec()
    };
    Ok(Bundle {
        entries,
        payload: Payload::Memory(payload),
        zipped,
        payload_field,
        header_region: 8i32.wrapping_add(cut),
    })
}

/// A zipped payload: zlib on the file as it is read; .NET's own reader on the bytes when zlib refuses the stream.
fn read_zipped(mut f: &File, len: usize, start: u64, rest: usize) -> Result<Payload> {
    let io = |e: io::Error| Fault::new(FaultKind::Io, e.to_string());
    let mut tail4 = [0u8; 4];
    let hint = if rest >= 4 {
        f.seek(SeekFrom::Start(len as u64 - 4)).map_err(io)?;
        f.read_exact(&mut tail4).map_err(io)?;
        (u32::from_le_bytes(tail4) as usize).min(rest.saturating_mul(1100))
    } else {
        0
    };
    f.seek(SeekFrom::Start(start)).map_err(io)?;
    let mut reader = io::BufReader::with_capacity(1 << 20, f.take(rest as u64));
    if let Some(v) = netgzip::gunzip_reader(&mut reader, hint) {
        return Ok(Payload::Memory(v));
    }
    f.seek(SeekFrom::Start(start)).map_err(io)?;
    let mut all = Vec::with_capacity(rest);
    f.take(rest as u64).read_to_end(&mut all).map_err(io)?;
    Ok(Payload::Memory(netgzip::gunzip(&all).map_err(gzip_fault)?))
}

// ------------------------------------------------------------------------------------------- the commands

/// `LayoutTokens`: DICE's padded layouts of an uncompressed payload.
fn layout_tokens(b: &Bundle) -> Result<String> {
    let nd: Vec<&Entry> = b.entries.iter().filter(|e| !e.is_deleted()).collect();
    if nd.len() < 2 {
        return Ok(String::new());
    }
    let plen = b.payload.len() as i64;
    let (mut align, mut gap) = (true, true);
    for (prev, cur) in nd.iter().zip(nd.iter().skip(1)) {
        let end = i64::from(prev.offset) + i64::from(prev.size);
        let at = i64::from(cur.offset);
        let mut filled = at >= end && at <= plen;
        if filled && at > end {
            filled = all_bf(&b.payload, end as usize, (at - end) as usize)?;
        }
        align &= filled && at % 4096 == 0 && at - end < 4096;
        gap &= filled && at - end == 64;
    }
    if !align && !gap {
        return Ok(String::new());
    }
    let Some(last) = nd.last() else {
        return Ok(String::new());
    };
    let hr = b.header_region;
    let mut s = format!(
        " layout={} hdr={}",
        if align { "align4096" } else { "gap64" },
        if hr % 4096 == 0 {
            4096
        } else if hr % 4 == 0 {
            4
        } else {
            1
        }
    );
    if i64::from(b.payload_field) != plen && b.payload_field == last.offset.wrapping_add(last.size2) {
        s.push_str(" plen=size2");
    }
    if nd.iter().all(|e| e.size >= 64 && e.size2 == e.size - 64) {
        s.push_str(" size2=minus64");
    }
    Ok(s)
}

/// Whether `len` bytes of the payload from `at` are all 0xBF (read in pieces; stops at the first other byte).
fn all_bf(p: &Payload, at: usize, len: usize) -> Result<bool> {
    let mut done = 0;
    while done < len {
        let n = (len - done).min(1 << 16);
        if !p.bytes(at + done, n)?.iter().all(|&c| c == 0xBF) {
            return Ok(false);
        }
        done += n;
    }
    Ok(true)
}

/// FbrbTool's loop over the entries (`Safe`, `CreateDirectory`, `File.Create`, `Write`), with its outcome: the
/// entries are checked and their folders made in order, then written on all cores; at the first entry FbrbTool would
/// fail on, the entries before it are written, its empty file made when `File.Create` would have made it, and its
/// error returned. Names that could meet on disk (the same file twice in any case, a file where a later entry needs a
/// folder, an existing folder where a file goes) are written one by one in order instead, as FbrbTool writes them.
fn write_entries(out_dir: &str, b: &Bundle) -> Result<()> {
    let mut jobs: Vec<(String, &Entry, (usize, usize))> = Vec::new();
    let mut files: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut dirs: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut stop: Option<(Fault, Option<String>)> = None;
    let mut in_order = false;
    for e in b.entries.iter().filter(|e| !e.is_deleted()) {
        let target = match safe(out_dir, &e.name) {
            Ok(t) => t,
            Err(f) => {
                stop = Some((f, None));
                break;
            }
        };
        let dir = directory_name(&target);
        // every folder up to the output folder: a planned file there is FbrbTool's error at this entry
        let mut d = dir.as_str();
        while d.len() > 3 {
            let up = net::to_upper(d);
            if files.contains(&up) {
                in_order = true;
            }
            dirs.insert(up);
            d = d.rfind('\\').and_then(|i| d.get(..i)).unwrap_or("");
        }
        let up = net::to_upper(&target);
        if dirs.contains(&up) || Path::new(&target).is_dir() || target.ends_with('\\') {
            in_order = true;
        }
        if in_order {
            break;
        }
        if let Err(f) = create_directory(&dir) {
            stop = Some((f, None));
            break;
        }
        match b.payload.check(e.offset as i32, e.size as i32) {
            Ok(range) => jobs.push((target, e, range)),
            Err(f) => {
                stop = Some((f, Some(target)));
                break;
            }
        }
        files.insert(up);
    }
    if in_order {
        for e in b.entries.iter().filter(|e| !e.is_deleted()) {
            let target = safe(out_dir, &e.name)?;
            create_directory(&directory_name(&target))?;
            create_and_write(&target, &b.payload, e.offset as i32, e.size as i32)?;
        }
        return Ok(());
    }
    // the same file named twice (in any case) is written in order by one task
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut by_name: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (i, (t, _, _)) in jobs.iter().enumerate() {
        let g = *by_name.entry(net::to_upper(t)).or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        if let Some(v) = groups.get_mut(g) {
            v.push(i);
        }
    }
    let first_error = groups
        .par_iter()
        .filter_map(|g| {
            g.iter().find_map(|&i| {
                let (target, _, (at, len)) = jobs.get(i)?;
                let written = b
                    .payload
                    .bytes(*at, *len)
                    .and_then(|data| write_atomic(target, &[&data]));
                written.err().map(|fault| (i, fault))
            })
        })
        .min_by_key(|(i, _)| *i);
    if let Some((_, f)) = first_error {
        return Err(f);
    }
    if let Some((f, empty)) = stop {
        if let Some(t) = empty {
            File::create(&t).map_err(|e| Fault::io(&e, Path::new(&t)))?;
        }
        return Err(f);
    }
    Ok(())
}

/// FbrbTool's `Unpack` after its `Read`: the output folder, the entries (in order, then on all cores: [`write_entries`]),
/// `fbrb.manifest`.
fn unpack_bundle(b: &Bundle, out_dir: &str) -> Result<UnpackStats> {
    create_directory(out_dir)?;
    let mut manifest = format!(
        "# fbrb.manifest - entry order matters. name|type|flags(hex)|size2|size  zipped={}{}\r\n",
        u8::from(b.zipped),
        if b.zipped { String::new() } else { layout_tokens(b)? }
    );
    let mut deleted = 0;
    for e in &b.entries {
        let _ = write!(
            manifest,
            "{}|{}|{:08X}|{}|{}\r\n",
            e.name, e.type_name, e.flags, e.size2, e.size
        );
        deleted += usize::from(e.is_deleted());
    }
    write_entries(out_dir, b)?;
    write_atomic(&full_path(&combine(out_dir, "fbrb.manifest")?)?, &[manifest.as_bytes()])?;
    Ok(UnpackStats {
        entries: b.entries.len(),
        payload_len: b.payload.len() as u64,
        deleted,
    })
}

fn unpack(fbrb: &str, out_dir: &str, o: &mut Outcome) -> Result<i32> {
    let s = unpack_bundle(&read_bundle(fbrb)?, out_dir)?;
    o.out(&format!(
        "unpacked {} entries ({} bytes) to {out_dir}{}",
        s.entries,
        s.payload_len,
        if s.deleted > 0 {
            format!(
                "; {} of them *deleted* markers, listed in fbrb.manifest only",
                s.deleted
            )
        } else {
            String::new()
        }
    ));
    Ok(0)
}

/// FbrbTool's `unpack <bundle> <out_dir>` without its messages: what it unpacked, or the error FbrbTool reports.
///
/// # Errors
/// The [`Fault`] FbrbTool would report (it explains it in words: [`Fault::explain`]).
pub fn unpack_path(bundle: &str, out_dir: &str) -> Result<UnpackStats> {
    unpack_bundle(&read_bundle(bundle)?, out_dir)
}

/// [`unpack_path`] of a bundle given as bytes (`label` stands for its path in an error).
///
/// # Errors
/// As [`unpack_path`].
pub fn unpack_bytes(label: &str, data: &[u8], out_dir: &str) -> Result<UnpackStats> {
    unpack_bundle(&bundle_from_bytes(label, data)?, out_dir)
}

fn list(fbrb: &str, o: &mut Outcome) -> Result<i32> {
    let b = read_bundle(fbrb)?;
    for e in &b.entries {
        o.out(&format!("{:>10}  {:<18}  {}", e.size, e.type_name, e.name));
    }
    o.out(&format!(
        "{} entries, payload {} bytes, zipped={}",
        b.entries.len(),
        b.payload.len(),
        if b.zipped { "True" } else { "False" }
    ));
    Ok(0)
}

fn big(path: &str, out_dir: &str, o: &mut Outcome) -> Result<i32> {
    let b = read_all(path)?;
    let magic = b.get(..4).ok_or_else(|| {
        Fault::new(
            FaultKind::ArgumentOutOfRange,
            "Index and count must refer to a location within the buffer.\nParameter name: bytes",
        )
    })?;
    let magic = ascii(magic);
    if magic != "BIGF" && magic != "BIG4" {
        return Err(Fault::invalid(format!("{path} is not a BIG archive ({magic})")));
    }
    let count = read_be(&b, 8)? as i32;
    let mut p: i32 = 16;
    create_directory(out_dir)?;
    let payload = Payload::Memory(b);
    for _ in 0..count.max(0) {
        let Payload::Memory(bytes) = &payload else { break };
        let offset = read_be(bytes, p)? as i32;
        let size = read_be(bytes, p.wrapping_add(4))? as i32;
        let name = cstring(bytes, p.wrapping_add(8))?;
        p = p.wrapping_add(8).wrapping_add(name.len() as i32).wrapping_add(1);
        let target = safe(out_dir, &name)?;
        create_directory(&directory_name(&target))?;
        create_and_write(&target, &payload, offset, size)?;
        o.out(&format!("{size:>9}  {name}"));
    }
    Ok(0)
}

/// `Convert.ToUInt32(s, 16)`.
fn parse_hex_u32(s: &str) -> Result<u32> {
    let c: Vec<char> = s.chars().collect();
    if c.is_empty() {
        return Err(Fault::new(
            FaultKind::ArgumentOutOfRange,
            "Index was out of range. Must be non-negative and less than the size of the collection.\nParameter name: startIndex",
        ));
    }
    let mut i = 0;
    match c.first() {
        Some('-') => return Err(argument("String cannot contain a minus sign if the base is not 10.")),
        Some('+') => i += 1,
        _ => {}
    }
    if c.get(i) == Some(&'0') && matches!(c.get(i + 1), Some('x' | 'X')) {
        i += 2;
    }
    let start = i;
    let mut v: u64 = 0;
    while let Some(d) = c.get(i).and_then(|ch| ch.to_digit(16)) {
        v = v * 16 + u64::from(d);
        if v > u64::from(u32::MAX) {
            return Err(Fault::new(
                FaultKind::Overflow,
                "Value was either too large or too small for a UInt32.",
            ));
        }
        i += 1;
    }
    if i == start {
        return Err(Fault::new(FaultKind::Format, "Could not find any recognizable digits."));
    }
    if i < c.len() {
        return Err(Fault::new(
            FaultKind::Format,
            "Additional non-parsable characters are at the end of the string.",
        ));
    }
    Ok(v as u32)
}

/// `File.Exists`: a file (not a folder) that can be seen; never an error.
fn file_exists(p: &str) -> bool {
    !p.is_empty() && fs::metadata(p).is_ok_and(|m| m.is_file())
}

fn pack(dir: &str, fbrb: &str, o: &mut Outcome) -> Result<i32> {
    let mpath = combine(dir, "fbrb.manifest")?;
    let text = decode_text(&read_all(&mpath)?);
    let all = lines(&text);
    let first = *all.first().ok_or_else(index)?;
    let zipped = first.contains("zipped=1");
    let layout_name = if zipped { None } else { token(first, "layout") };
    let header_align = net::parse_int(token(first, "hdr").unwrap_or("1"))?;
    let layout = Layout {
        zipped,
        padding: match layout_name {
            Some("align4096") => Some(Padding::Align4096),
            Some("gap64") => Some(Padding::Gap64),
            _ => None,
        },
        header_align,
        plen_size2: token(first, "plen") == Some("size2"),
        size2_minus64: token(first, "size2") == Some("minus64"),
    };
    let mut mlines = Vec::new();
    let mut rows = Vec::new();
    for line in &all {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let p: Vec<&str> = line.split('|').collect();
        let name = p.first().copied().unwrap_or_default();
        let source = safe(dir, name)?;
        let type_name = *p.get(1).ok_or_else(index)?;
        let (src, len) = if type_name == DELETED && !file_exists(&source) {
            (None, 0usize)
        } else {
            let (_, len) = open_for_read(&source)?;
            (Some(PathBuf::from(&source)), len as usize)
        };
        let original_size2 = net::parse_uint(p.get(3).ok_or_else(index)?)?;
        let has_size = p.len() > 4;
        if !has_size && original_size2 != len as u32 {
            return Err(Fault::invalid(format!(
                "fbrb.manifest is from an older unpack (no size column) and entry '{name}' has a differing size2 - \
                 unpack the bundle again to rebuild the manifest"
            )));
        }
        let original_size = match p.get(4) {
            Some(v) => net::parse_uint(v)?,
            None => len as u32,
        };
        let marker = type_name == DELETED && len == 0;
        let flags = parse_hex_u32(p.get(2).copied().unwrap_or_default())?;
        let size = len as u32;
        let unchanged = len as i64 == i64::from(original_size as i32);
        let mut size2 = if original_size2 != size && unchanged {
            original_size2
        } else {
            size
        };
        if layout.size2_minus64 && !marker && !unchanged && size >= 64 {
            size2 = size - 64;
        }
        mlines.push(ManifestLine {
            name: name.to_owned(),
            type_name: type_name.to_owned(),
            flags,
            size2: original_size2,
            size: Some(original_size),
        });
        rows.push(SourceRow { src, len, size2 });
    }
    let manifest = Manifest { layout, lines: mlines };
    // written beside the bundle, then moved over it (FbrbTool's name for the temporary file: <bundle>.tmp-<pid>);
    // the payload streamed into it as it is made (round S1), never held
    let tmp = format!("{fbrb}.tmp-{}", std::process::id());
    check_chars(&tmp)?;
    let tmp_full = full_path(&tmp)?;
    let stats = pack_to_file(&manifest, &rows, &tmp_full)?;
    let target = full_path(fbrb)?;
    if Path::new(&target).is_dir() {
        // File.Exists is false for a folder, and File.Move onto it fails with Windows' own words
        let _ = fs::remove_file(&tmp_full);
        return Err(Fault::new(
            FaultKind::Io,
            "Cannot create a file when that file already exists.\n",
        ));
    }
    if let Err(e) = crate::atomic::replace(Path::new(&tmp_full), Path::new(&target)) {
        let _ = fs::remove_file(&tmp_full);
        return Err(Fault::io(&e, Path::new(fbrb)));
    }
    o.out(&format!(
        "packed {} entries ({} bytes) into {fbrb}",
        stats.entries, stats.payload_len
    ));
    Ok(0)
}

/// The bundle streamed into the file `tmp_full` (made, written, flushed to disk); on any error the file is deleted:
/// a packing error is reported as FbrbTool reported a failed pack, a write error with the file's name.
fn pack_to_file(manifest: &Manifest, rows: &[SourceRow], tmp_full: &str) -> Result<super::pack::PackStats> {
    let written = File::create(tmp_full).map_err(StreamError::Write).and_then(|f| {
        let mut w = io::BufWriter::with_capacity(1 << 20, f);
        let stats = pack_rows_stream(manifest, rows, &mut w)?;
        let f = w.into_inner().map_err(|e| StreamError::Write(e.into_error()))?;
        f.sync_all().map_err(StreamError::Write)?;
        Ok(stats)
    });
    written.map_err(|e| {
        let _ = fs::remove_file(tmp_full);
        match e {
            StreamError::Pack(e) => Fault::new(FaultKind::Io, e.to_string()),
            StreamError::Write(e) => Fault::io(&e, Path::new(tmp_full)),
        }
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn usage_and_numbers() {
        let o = run(&args(&["nope"]), None);
        assert_eq!((o.code, o.stderr.as_str()), (2, format!("{USAGE}\n").as_str()));
        assert_eq!(run(&args(&["list"]), None).code, 2);
        assert_eq!(parse_hex_u32("0001000a").unwrap(), 0x1000A);
        assert_eq!(parse_hex_u32("+0x1").unwrap(), 1);
        assert_eq!(parse_hex_u32("").unwrap_err().kind, FaultKind::ArgumentOutOfRange);
        assert_eq!(parse_hex_u32("-1").unwrap_err().kind, FaultKind::Argument);
        assert_eq!(parse_hex_u32("100000000").unwrap_err().kind, FaultKind::Overflow);
        assert_eq!(parse_hex_u32(" 1").unwrap_err().kind, FaultKind::Format);
        assert!(parse_hex_u32("1g").unwrap_err().message.starts_with("Additional"));
    }

    #[test]
    #[expect(clippy::many_single_char_names, reason = "a test's paths")]
    fn round_trip_and_damage() {
        let d = std::env::temp_dir().join(format!("bc2-fbrbtool-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let bundle = super::super::build(
            &[
                ("Levels/A.dbx", "<non-resource>", b"{binary}abc"),
                ("Gone", DELETED, b""),
                ("B.res", "T", b"xyz"),
            ],
            true,
        );
        let src = d.join("a.fbrb");
        fs::write(&src, &bundle).unwrap();
        let s = |p: &Path| p.to_string_lossy().into_owned();
        let u = d.join("u");
        let o = run(&args(&["unpack", &s(&src), &s(&u)]), None);
        assert_eq!(o.code, 0, "{o:?}");
        assert!(o.stdout.contains("unpacked 3 entries (14 bytes)") && o.stdout.contains("1 of them *deleted*"));
        assert_eq!(fs::read(u.join("Levels").join("A.dbx")).unwrap(), b"{binary}abc");
        assert!(!u.join("Gone").exists());
        let m = fs::read_to_string(u.join("fbrb.manifest")).unwrap();
        assert!(m.contains("Gone|*deleted*|00010000|0|0\r\n"), "{m}");
        let re = d.join("re.fbrb");
        let o = run(&args(&["pack", &s(&u), &s(&re)]), None);
        assert_eq!(o.code, 0, "{o:?}");
        let o = run(&args(&["list", &s(&re)]), None);
        assert!(
            o.stdout.ends_with("3 entries, payload 14 bytes, zipped=True\n"),
            "{o:?}"
        );
        // damage: not a bundle, a missing file
        fs::write(d.join("x.fbrb"), b"nope").unwrap();
        let o = run(&args(&["list", &s(&d.join("x.fbrb"))]), Some("LOG"));
        assert_eq!(o.code, 1);
        assert!(
            o.stderr.contains("is not an FbRB bundle") && o.stderr.contains("details: LOG"),
            "{o:?}"
        );
        assert_eq!(o.log.len(), 1);
        let o = run(&args(&["list", &s(&d.join("missing.fbrb"))]), None);
        assert!(o.stderr.contains("A file the tool needs is missing"), "{o:?}");
        let _ = fs::remove_dir_all(&d);
    }

    /// E3j's `layout_tokens(entries, payload, payload_field, header_region)` on the shared engine.
    fn tokens(es: &[Entry], p: &[u8], plen: u32, hr: i32) -> String {
        let b = Bundle {
            entries: es
                .iter()
                .map(|e| Entry {
                    name: e.name.clone(),
                    type_name: e.type_name.clone(),
                    ..*e
                })
                .collect(),
            payload: Payload::Memory(p.to_vec()),
            zipped: false,
            payload_field: plen,
            header_region: hr,
        };
        layout_tokens(&b).unwrap()
    }

    #[test]
    fn layout_tokens_of_padded_payloads() {
        let e = |offset, size, size2| Entry {
            name: "x".into(),
            type_name: "t".into(),
            flags: 0,
            offset,
            size,
            size2,
        };
        let mut p = vec![1u8; 100];
        p.extend(vec![0xBF; 4096 - 100]);
        p.extend(vec![2u8; 100]);
        let es = [e(0, 100, 36), e(4096, 100, 36)];
        assert_eq!(
            tokens(&es, &p, 4096 + 36, 4096),
            " layout=align4096 hdr=4096 plen=size2 size2=minus64"
        );
        let mut g = vec![1u8; 10];
        g.extend(vec![0xBF; 64]);
        g.extend(vec![2u8; 10]);
        assert_eq!(
            tokens(&[e(0, 10, 10), e(74, 10, 10)], &g, 84, 22),
            " layout=gap64 hdr=1"
        );
        assert_eq!(tokens(&[e(0, 10, 10), e(10, 10, 10)], &[0; 20], 20, 24), "");
    }
}
