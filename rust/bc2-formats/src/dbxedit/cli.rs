//! `Program` of `DbxEdit.cs`: the command line, its messages (stdout and stderr, line by line as DbxEdit prints
//! them) and exit codes, run in process. What DbxEdit's `ToolLog` writes to its log is returned with the outcome, for
//! the caller to write.

use std::collections::HashMap;
use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use rayon::prelude::*;

use super::check::{fix_slots, level_check};
use super::editor;
use super::export::convert;
use super::fault::{Fault, FaultKind, Result};
use super::file::EditFile;
use super::import::import;
use super::net;

/// What a run of a command printed, its exit code, and the lines DbxEdit would have logged.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    /// The exit code: 0 done, 1 failed (or `check` found errors, `roundtrip` differences), 2 a usage error, 3 an
    /// import written with guessed types.
    pub code: i32,
    /// Standard output, each line ended by `\n`.
    pub stdout: String,
    /// Standard error, each line ended by `\n`.
    pub stderr: String,
    /// `(level, message)` lines for DbxEdit's log (`WARNING`, `ERROR`).
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

    fn warn(&mut self, message: String) {
        self.log.push(("WARNING".into(), message));
    }

    /// `ToolLog.Error(e, where)`.
    fn error(&mut self, f: &Fault, place: Option<&str>) {
        let (cause, _) = f.explain();
        let at = place.map_or(String::new(), |p| format!(" in {p}"));
        self.log.push((
            "ERROR".into(),
            format!("dbxedit error{at}: {cause}\r\n{}: {}", f.type_name(), f.message),
        ));
    }
}

static WRITES: AtomicU64 = AtomicU64::new(0);

/// `AtomicFile.WriteAllBytes`: written beside the target (`<path>.tmp-<process>-<n>`), flushed, then moved over it
/// with [`crate::atomic::replace`] (which waits out a moment's lock on the target).
///
/// # Errors
/// The I/O error as .NET words it.
pub fn write_atomic(path: &str, data: &[u8]) -> Result<()> {
    let n = WRITES.fetch_add(1, Ordering::Relaxed);
    let tmp = format!("{path}.tmp-{}-{n}", std::process::id());
    let written = fs::File::create(&tmp).and_then(|mut f| {
        f.write_all(data)?;
        f.sync_all()
    });
    if let Err(e) = written {
        let _ = fs::remove_file(&tmp);
        return Err(Fault::io(&e, Path::new(&tmp)));
    }
    crate::atomic::replace(Path::new(&tmp), Path::new(path)).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        Fault::io(&e, Path::new(path))
    })
}

fn read_file(path: &str) -> Result<EditFile> {
    EditFile::parse(net::read_bytes(path)?)
}

fn deleted_note(n: usize) -> String {
    if n == 0 {
        String::new()
    } else {
        format!(", skipped {n} empty file(s) (*deleted* bundle entries)")
    }
}

const USAGE: [&str; 5] = [
    "usage: DbxEdit xml <file|folder> ... | roundtrip <file|folder> | edit <in.dbx> <out.dbx> <command|@file> ...",
    "       DbxEdit import <in.xml> <out.dbx> [--like <original.dbx>] [--types <file.dbx|folder>] ...",
    "       DbxEdit check <level.dbx> [--slots-from <level.dbx>] [--bundle <folder>] ...",
    "       DbxEdit fixslots <in.dbx> <out.dbx> --from <level.dbx>",
    "  replace=<old>|<new>  drop=<guid>  dropfield=<name>  graft=<source.dbx>|<guid>",
];

/// Runs DbxEdit's command line `args` (without the program name) in process: the same files written, the same
/// messages and exit code. `log_path` is the log file named in an unexpected error's message (DbxEdit's
/// `%LOCALAPPDATA%\bfbc2-tool-suite\logs\dbxedit-<date>.log`).
#[must_use]
pub fn run(args: &[String], log_path: Option<&str>) -> Outcome {
    let mut o = Outcome::default();
    o.code = match dispatch(args, &mut o) {
        Ok(code) => code,
        Err(f) => {
            // ToolLog.Fail: logged and explained, exit code 1
            o.error(&f, None);
            let (cause, todo) = f.explain();
            o.err(&format!("error: {cause}"));
            o.err(&format!("  what to do: {todo}"));
            o.err(&format!("  details: {}", log_path.unwrap_or("(no log file)")));
            1
        }
    };
    o
}

/// [`run`] of many command lines on all cores, in order. Runs that write the same file must not be in one batch.
#[must_use]
pub fn run_many(runs: &[Vec<String>], log_path: Option<&str>) -> Vec<Outcome> {
    runs.par_iter().map(|a| run(a, log_path)).collect()
}

fn dispatch(args: &[String], o: &mut Outcome) -> Result<i32> {
    let a0 = args.first().map_or("", String::as_str);
    let n = args.len();
    match a0 {
        "xml" if n >= 2 => xml(args.get(1..).unwrap_or_default(), o),
        "roundtrip" if n >= 2 => roundtrip(args.get(1).map_or("", String::as_str), o),
        "edit" if n >= 3 => edit(args, o),
        "import" if n >= 3 => import_cmd(args, o),
        "check" if n >= 2 => check(args, o),
        "fixslots" if n >= 3 => fixslots(args, o),
        _ => {
            for l in USAGE {
                o.err(l);
            }
            Ok(2)
        }
    }
}

fn xml(paths: &[String], o: &mut Outcome) -> Result<i32> {
    let (mut converted, mut deleted) = (0, 0);
    for p in paths {
        for f in net::dbx_files(p)? {
            let one = (|| -> Result<bool> {
                if net::file_len(&f)? == 0 {
                    return Ok(false);
                }
                let data = net::read_bytes(&f)?;
                let magic = data.get(..8).ok_or_else(|| {
                    Fault::new(
                        FaultKind::ArgumentOutOfRange,
                        "Index and count must refer to a location within the buffer.\nParameter name: bytes",
                    )
                })?;
                if magic == b"{binary}" {
                    write_atomic(&net::change_extension(&f, ".xml"), convert(&data)?.as_bytes())?;
                }
                Ok(true)
            })();
            match one {
                Ok(true) => converted += 1,
                Ok(false) => deleted += 1,
                Err(e) => {
                    o.err(&format!("{f}: {}", e.message));
                    o.warn(format!("xml: {f}: {}", e.message));
                }
            }
        }
    }
    o.out(&format!("converted {converted} file(s){}", deleted_note(deleted)));
    Ok(0)
}

fn roundtrip(path: &str, o: &mut Outcome) -> Result<i32> {
    let (mut same, mut different, mut skipped, mut deleted) = (0, 0, 0, 0);
    for f in net::dbx_files(path)? {
        if net::file_len(&f)? == 0 {
            deleted += 1;
            continue;
        }
        let d = match read_file(&f) {
            Ok(d) => d,
            Err(e) => {
                o.out(&format!("ERROR {f}: {}", e.message));
                o.warn(format!("roundtrip: {f}: {}", e.message));
                different += 1;
                continue;
            }
        };
        if !d.is_binary {
            skipped += 1;
            continue;
        }
        let written = d.serialize()?;
        if written == d.original {
            same += 1;
        } else {
            different += 1;
            if different <= 10 {
                o.out(&format!("DIFF {f} ({} -> {})", d.original.len(), written.len()));
            }
        }
    }
    o.out(&format!(
        "roundtrip: {same} identical, {different} different, {skipped} not binary{}",
        deleted_note(deleted)
    ));
    Ok(i32::from(different != 0))
}

/// The commands of an `edit` command line: each argument, or the lines of an `@file` (trimmed, but a `setfield`
/// line keeps the spaces of its value; empty lines and `#` comments left out).
///
/// # Errors
/// An `@file` that cannot be read.
pub fn expand_commands(args: &[String]) -> Result<Vec<String>> {
    let mut commands = Vec::new();
    for a in args {
        if let Some(file) = a.strip_prefix('@') {
            for l in net::read_lines(file)? {
                let ts = l.trim_start();
                let l = if ts.starts_with("setfield=") {
                    ts.to_owned()
                } else {
                    l.trim().to_owned()
                };
                if !l.trim().is_empty() && !l.starts_with('#') {
                    commands.push(l);
                }
            }
        } else {
            commands.push(a.clone());
        }
    }
    Ok(commands)
}

fn part<'a>(p: &[&'a str], i: usize) -> Result<&'a str> {
    p.get(i).copied().ok_or_else(Fault::index)
}

fn source<'c>(cache: &'c mut HashMap<String, EditFile>, path: &str) -> Result<&'c EditFile> {
    if !cache.contains_key(path) {
        let f = read_file(path)?;
        cache.insert(path.to_owned(), f);
    }
    cache.get(path).ok_or_else(Fault::null)
}

/// One `edit` command applied: `Some(exit code)` when it stops the edit (nothing is written then).
#[expect(clippy::too_many_lines, reason = "one arm per command, as DbxEdit's Program.Edit")]
fn apply(
    file: &mut EditFile,
    sources: &mut HashMap<String, EditFile>,
    fields_set: &mut usize,
    command: &str,
    o: &mut Outcome,
) -> Result<Option<i32>> {
    let eq = command.find('=');
    let verb = eq.map_or(command, |e| command.get(..=e).unwrap_or(""));
    let rest = eq.map_or("", |e| command.get(e + 1..).unwrap_or(""));
    let parts: Vec<&str> = match eq {
        None => Vec::new(),
        Some(_) if verb == "setfield=" => rest.splitn(3, '|').collect(),
        Some(_) => rest.split('|').collect(),
    };
    match verb {
        "replace=" => {
            let (from, to) = (part(&parts, 0)?, part(&parts, 1)?);
            let hits = editor::replace(file, from, to);
            o.out(&format!("replace {from} -> {to}: {hits} string(s)"));
            if hits == 0 {
                o.err(&format!("not found: {from}"));
                return Ok(Some(1));
            }
        }
        "drop=" => {
            let (removed, references) = editor::drop(file, rest)?;
            o.out(&format!(
                "drop {rest}: {removed} instance(s), {references} reference(s)"
            ));
            if removed == 0 {
                o.err(&format!("no instance {rest}"));
                return Ok(Some(1));
            }
        }
        "replaceprefix=" => {
            let (from, to) = (part(&parts, 0)?, part(&parts, 1)?);
            let except: Vec<String> = parts.iter().skip(2).map(|s| (*s).to_owned()).collect();
            let count = editor::replace_prefix(file, from, to, &except);
            o.out(&format!("replaceprefix {from} -> {to}: {count} string(s)"));
        }
        "partguid=" => {
            editor::set_partition_guid(file, rest)?;
            o.out(&format!("partguid {rest}"));
        }
        "copyfield=" => {
            let src = source(sources, part(&parts, 0)?)?;
            let (sg, field, guid) = (part(&parts, 1)?, part(&parts, 2)?, part(&parts, 3)?);
            let ok = editor::copy_field(file, src, sg, field, guid)?;
            o.out(&format!(
                "copyfield {field}: {}",
                if ok { "copied" } else { "NOT FOUND" }
            ));
            if !ok {
                return Ok(Some(1));
            }
        }
        "set=" => {
            let name = part(&parts, 0)?;
            let text = part(&parts, 1)?;
            let v = match text {
                "true" => 1,
                "false" => 0,
                _ => net::parse_byte(text)?,
            };
            let count = editor::set_byte(file, name, v)?;
            o.out(&format!("set {name} = {text}: {count} field(s)"));
            if count == 0 {
                o.err(&format!("no bool/byte field {name}"));
                return Ok(Some(1));
            }
        }
        "setfield=" => {
            let (guid, field, value) = (part(&parts, 0)?, part(&parts, 1)?, part(&parts, 2)?);
            if !editor::set_field(file, guid, field, value)? {
                o.err(&format!("setfield: no settable field {field} in {guid}"));
                return Ok(Some(1));
            }
            *fields_set += 1;
        }
        "additem=" => {
            let (guid, array, reference) = (part(&parts, 0)?, part(&parts, 1)?, part(&parts, 2)?);
            let ok = editor::add_item(file, guid, array, reference, parts.get(3).copied())?;
            o.out(&format!(
                "additem {reference} -> {array}: {}",
                if ok { "added" } else { "NOT FOUND" }
            ));
            if !ok {
                return Ok(Some(1));
            }
        }
        "dropfield=" => {
            let count = editor::drop_field(file, rest)?;
            o.out(&format!("dropfield {rest}: {count} element(s)"));
        }
        "graft=" => {
            let src_path = part(&parts, 0)?;
            let src = source(sources, src_path)?;
            let guid = part(&parts, 1)?;
            let count = editor::graft(file, src, guid, parts.get(2).copied())?;
            o.out(&format!(
                "graft {guid} from {}: {count} instance(s)",
                net::file_name(src_path)
            ));
            if count == 0 && file.instance(guid)?.is_none() {
                o.err(&format!("nothing grafted for {guid}"));
                return Ok(Some(1));
            }
        }
        _ => {
            o.err(&format!("unknown command {command}"));
            return Ok(Some(2));
        }
    }
    Ok(None)
}

fn edit(args: &[String], o: &mut Outcome) -> Result<i32> {
    let input = args.get(1).map_or("", String::as_str);
    let output = args.get(2).map_or("", String::as_str);
    let mut file = read_file(input)?;
    if !file.is_binary {
        o.err(&format!("not a binary dbx: {input}"));
        return Ok(2);
    }
    let commands = expand_commands(args.get(3..).unwrap_or_default())?;
    let mut sources: HashMap<String, EditFile> = HashMap::new();
    let mut fields_set = 0;
    for c in &commands {
        if let Some(code) = apply(&mut file, &mut sources, &mut fields_set, c, o)? {
            return Ok(code);
        }
    }
    if fields_set > 0 {
        o.out(&format!("setfield: {fields_set} field(s)"));
    }
    write_atomic(output, &file.serialize()?)?;
    Ok(0)
}

fn import_cmd(args: &[String], o: &mut Outcome) -> Result<i32> {
    let xml_path = args.get(1).map_or("", String::as_str);
    let output = args.get(2).map_or("", String::as_str);
    let mut like: Option<String> = None;
    let mut type_paths = Vec::new();
    let mut i = 3;
    while i < args.len() {
        let a = args.get(i).map_or("", String::as_str);
        let next = args.get(i + 1);
        match (a, next) {
            ("--like", Some(v)) => {
                like = Some(v.clone());
                i += 1;
            }
            ("--types", Some(v)) => {
                type_paths.push(v.clone());
                i += 1;
            }
            _ => {
                o.err(&format!("import: unknown option {a}"));
                return Ok(2);
            }
        }
        i += 1;
    }
    if like.is_none() {
        let beside = net::change_extension(xml_path, ".dbx");
        if net::is_file(&beside) {
            like = Some(beside);
        }
    }
    let attempt = (|| -> Result<i32> {
        let reference = match &like {
            Some(l) => {
                let r = read_file(l)?;
                if !r.is_binary {
                    o.err(&format!("import: not a binary dbx: {l}"));
                    return Ok(1);
                }
                Some(r)
            }
            None => None,
        };
        let mut type_files = Vec::new();
        for tp in &type_paths {
            for p in net::dbx_files(tp)? {
                let d = read_file(&p)?;
                if d.is_binary {
                    type_files.push(d);
                }
            }
        }
        let (result, report) = import(
            || Ok(net::decode_text(&net::read_bytes(xml_path)?)),
            xml_path,
            reference.as_ref(),
            &type_files,
        )?;
        write_atomic(output, &result.serialize()?)?;
        let types_from = like.as_ref().map_or(String::new(), |l| format!(" (types from {l})"));
        let rounded = if report.rounded > 0 {
            format!("; {} decimal(s) rounded to the nearest 4-byte value", report.rounded)
        } else {
            String::new()
        };
        o.out(&format!(
            "import {xml_path} -> {output}{types_from}: {} unchanged, {} changed, {} new, {} removed{rounded}",
            report.kept, report.changed, report.added, report.removed
        ));
        if report.guesses.is_empty() {
            return Ok(0);
        }
        for g in report.guesses.iter().take(50) {
            o.err(&format!("guess: {g}"));
        }
        o.err(&format!(
            "{} value(s) typed by guess - check them, or give the original file (--like) or more files (--types)",
            report.guesses.len()
        ));
        Ok(3)
    })();
    match attempt {
        Err(e) if e.caught_by_import() => {
            o.err(&format!("import failed: {}", e.message));
            o.error(&e, Some("import"));
            Ok(1)
        }
        other => other,
    }
}

fn check(args: &[String], o: &mut Outcome) -> Result<i32> {
    let mut slots_from = None;
    let mut bundles = Vec::new();
    let mut i = 2;
    while i < args.len() {
        let a = args.get(i).map_or("", String::as_str);
        match (a, args.get(i + 1)) {
            ("--slots-from", Some(v)) => {
                slots_from = Some(read_file(v)?);
                i += 1;
            }
            ("--bundle", Some(v)) => {
                bundles.push(v.clone());
                i += 1;
            }
            _ => {
                o.err(&format!("check: unknown option {a}"));
                return Ok(2);
            }
        }
        i += 1;
    }
    let path = args.get(1).map_or("", String::as_str);
    let f = read_file(path)?;
    if !f.is_binary {
        o.err(&format!("not a binary dbx: {path}"));
        return Ok(2);
    }
    let found = level_check(&f, slots_from.as_ref(), &bundles)?;
    for x in found
        .iter()
        .filter(|x| x.error)
        .chain(found.iter().filter(|x| !x.error))
    {
        o.out(&x.to_string());
    }
    let errors = found.iter().filter(|x| x.error).count();
    o.out(&format!(
        "check {path}: {errors} error(s), {} warning(s)",
        found.len() - errors
    ));
    Ok(i32::from(errors > 0))
}

fn fixslots(args: &[String], o: &mut Outcome) -> Result<i32> {
    if args.len() < 5 || args.get(3).map(String::as_str) != Some("--from") {
        o.err("usage: DbxEdit fixslots <in.dbx> <out.dbx> --from <level.dbx>");
        return Ok(2);
    }
    let mut f = read_file(args.get(1).map_or("", String::as_str))?;
    let from = read_file(args.get(4).map_or("", String::as_str))?;
    if !f.is_binary || !from.is_binary {
        o.err("not a binary dbx");
        return Ok(2);
    }
    let r = fix_slots(&mut f, &from)?;
    o.out(&format!(
        "fixslots: {} slot(s) changed; matched by guid {}, by blueprint and position {}, nearest within 50 m {}, no counterpart (slot 0) {}",
        r.changed, r.by_guid, r.by_position, r.by_nearest, r.no_counterpart
    ));
    for u in r.unmatched.iter().take(30) {
        o.out(&format!("  no counterpart: {u}"));
    }
    write_atomic(args.get(2).map_or("", String::as_str), &f.serialize()?)?;
    Ok(0)
}
