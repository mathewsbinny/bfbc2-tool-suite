//! The DBX inspector against DbxEditGUI, before it was retired (round GR4): DbxEditGUI's own answers on real files of
//! the game copy (`tests/data/dbxeditgui_answers.json.gz`, recorded by the round's recorder driving the window - its
//! `MainWindow` through reflection, its prompts answered - and kept as digests and messages, no game data), replayed
//! step by step on [`DbxDoc`]: the tree (every element's label, heading, attributes and value), each Apply (the
//! file's bytes after it, the edit script's line, the output's lines, the message of a value refused), Undo, Save
//! (and the `.bak` the first save keeps), the edit script, Find (the matches and the status line), the Edit and
//! Instance menus' commands, graft and copy field from another file, the level check (alone and against the cull
//! level), Fix cull slots, Export XML, Import XML of an edited export, Save as. Every file must give DbxEditGUI's
//! bytes exactly (SHA-1 of the whole file) - the denormals among them: their bits kept, never their printed whole
//! number taken as a decimal.
//!
//! Skipped (with a note) without the game copy; a recorded file that is no longer there unchanged is skipped and
//! named, and at least half must be checked.
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::doc_markdown,
    clippy::map_unwrap_or,
    clippy::if_not_else,
    missing_docs
)]

use std::io::Read;
use std::path::{Path, PathBuf};

use bc2_editor::dbxinspect::{Applied, DbxDoc, NodePath};
use bc2_editor::levels;
use bc2_formats::dbxedit::{self as dx, EditFile};
use serde_json::Value;

fn recorded() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("dbxeditgui_answers.json.gz");
    let mut s = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(&path).unwrap())
        .read_to_string(&mut s)
        .unwrap();
    serde_json::from_str(&s).unwrap()
}

fn sha1(b: &[u8]) -> String {
    sha1_smol::Sha1::from(b).digest().to_string()
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().map(|x| x.as_str().unwrap_or("").to_owned()).collect())
        .unwrap_or_default()
}

/// Lines as the recording keeps them: their count and the SHA-1 of them joined by `\n`.
fn dl(v: &[String]) -> Value {
    serde_json::json!([v.len(), sha1(v.join("\n").as_bytes())])
}

/// A text as the recording keeps it: its SHA-1 (`null` for none).
fn ds(v: Option<&str>) -> Value {
    v.map_or(Value::Null, |s| Value::String(sha1(s.as_bytes())))
}

fn path_of(v: &Value) -> NodePath {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_u64().unwrap() as usize)
        .collect()
}

fn join(p: &[usize]) -> String {
    p.iter().map(ToString::to_string).collect::<Vec<_>>().join(",")
}

/// The window's tree as DbxEditGUI's recorder wrote it: each element's path, label, heading, attributes and value.
fn tree(d: &DbxDoc) -> (usize, String) {
    let mut lines = Vec::new();
    for p in d.all_paths() {
        let e = d.element(&p).unwrap();
        let attrs: Vec<String> = d.attributes(e).into_iter().map(|(k, v)| format!("{k}={v}")).collect();
        lines.push(format!(
            "{}\t{}\t{}\t{}\t{}",
            join(&p),
            d.label(e),
            d.heading(e),
            attrs.join("\u{1}"),
            d.value(e)
        ));
    }
    (lines.len(), sha1(lines.join("\n").as_bytes()))
}

/// The recorder's edited export (its `MutateXml`): up to 12 lines' values changed - `true` to `false`, decimals
/// halved (.NET's round-trip text), every 7th line's text value suffixed `_imp`.
fn mutate_xml(xml: &str) -> String {
    let mut lines: Vec<String> = xml.split("\r\n").map(str::to_owned).collect();
    let mut changed = 0;
    for (i, l) in lines.iter_mut().enumerate() {
        if changed >= 12 {
            break;
        }
        let (Some(gt), Some(lt)) = (l.find('>'), l.rfind("</")) else {
            continue;
        };
        if lt <= gt {
            continue;
        }
        let inner = &l[gt + 1..lt];
        let decimal = |x: &str| dx::net::try_parse_double(x);
        let repl = if inner == "true" {
            Some("false".to_owned())
        } else if inner.contains('.') && inner.split('/').all(|x| decimal(x).is_some()) {
            Some(
                inner
                    .split('/')
                    .map(|x| bc2_formats::dbx::net_r_double((decimal(x).unwrap() / 2.0).to_bits()))
                    .collect::<Vec<_>>()
                    .join("/"),
            )
        } else if i % 7 == 0
            && !inner.is_empty()
            && !inner.contains('/')
            && !inner.starts_with(|c: char| c.is_numeric() || c == '-')
        {
            Some(format!("{inner}_imp"))
        } else {
            None
        };
        let Some(r) = repl else { continue };
        *l = format!("{}{r}{}", &l[..=gt], &l[lt..]);
        changed += 1;
    }
    lines.join("\r\n")
}

struct Replay {
    dir: PathBuf,
    at: usize,
    bad: Vec<String>,
    file: String,
}

impl Replay {
    /// The output's lines since the last step, the folder named `<dir>` as the recorder wrote it.
    fn log(&mut self, d: &DbxDoc) -> Vec<String> {
        let dir = self.dir.display().to_string();
        let out: Vec<String> = d.log[self.at..].iter().map(|l| l.replace(&dir, "<dir>")).collect();
        self.at = d.log.len();
        out
    }

    /// Lines against their recorded digest (`[count, sha1 of the lines joined by \n]`).
    fn lines(&mut self, what: &str, got: &[String], want: &Value) {
        if &dl(got) != want {
            self.bad
                .push(format!("{}: {what}: got {got:?}, DbxEditGUI {want}", self.file));
        }
    }

    fn check<T: std::fmt::Debug + PartialEq>(&mut self, what: &str, got: &T, want: &T) {
        if got != want {
            self.bad
                .push(format!("{}: {what}: got {got:?}, DbxEditGUI {want:?}", self.file));
        }
    }
}

fn source(root: &Path, rel: &str) -> (String, EditFile) {
    let p = root.join(rel);
    let f = EditFile::parse(std::fs::read(&p).unwrap()).unwrap();
    (p.display().to_string(), f)
}

#[test]
#[ignore = "game tier: DbxEditGUI's recorded answers on the game copy's files (run_tests.py game)"]
fn the_inspector_gives_dbxeditguis_answers() {
    let _gate = crate::gate();
    let Ok(root) = levels::find_opfor(None) else {
        println!("skipped: no game copy");
        return;
    };
    let rec = recorded();
    let files = rec["files"].as_array().unwrap();
    let (mut checked, mut steps, mut skipped) = (0, 0, Vec::new());
    let mut bad = Vec::new();
    for f in files {
        let rel = f["path"].as_str().unwrap();
        let src = root.join(rel);
        match std::fs::read(&src) {
            Ok(b) if sha1(&b) == f["sha1"].as_str().unwrap() => {}
            _ => {
                skipped.push(rel.to_owned());
                continue;
            }
        }
        let dir = std::env::temp_dir().join(format!("bc2_gr4_parity_{}_{checked}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(Path::new(rel).file_name().unwrap());
        std::fs::copy(&src, &file).unwrap();
        let original = std::fs::read(&file).unwrap();
        let mut d = DbxDoc::open(&file).unwrap();
        let mut r = Replay {
            dir: dir.clone(),
            at: d.log.len(),
            bad: Vec::new(),
            file: rel.to_owned(),
        };
        for s in f["steps"].as_array().unwrap() {
            steps += 1;
            let bytes = |d: &DbxDoc| sha1(&d.bytes().unwrap());
            match s["step"].as_str().unwrap() {
                "tree" => {
                    let (n, h) = tree(&d);
                    r.check(
                        "tree",
                        &(n as u64, h),
                        &(s["count"].as_u64().unwrap(), s["sha1"].as_str().unwrap().to_owned()),
                    );
                }
                "apply" => {
                    let p = path_of(&s["path"]);
                    let e = d.element(&p).unwrap();
                    let attrs = if s["attrs"].is_null() {
                        d.attributes(e).into_iter().map(|a| a.1).collect()
                    } else {
                        strs(&s["attrs"])
                    };
                    // the value box holds the element's text until it is typed over
                    let value = s["value"].as_str().map_or_else(|| d.value(e), str::to_owned);
                    let before = d.script.len();
                    let res = d.apply_edit(&p, &attrs, &value);
                    let refused = match &res {
                        Err(m) => Some(m.clone()),
                        Ok(_) => None,
                    };
                    let what = format!("apply {} {value:?}", join(&p));
                    r.check(
                        &format!("{what}: refused {refused:?}"),
                        &ds(refused.as_deref()),
                        &s["refused"],
                    );
                    r.check(
                        &format!("{what}: bytes"),
                        &bytes(&d),
                        &s["sha1"].as_str().unwrap().to_owned(),
                    );
                    let line = (d.script.len() > before).then(|| d.script.last().cloned().unwrap());
                    r.check(&format!("{what}: script {line:?}"), &ds(line.as_deref()), &s["script"]);
                    let outcome = match res {
                        Ok(Applied::Changed) => "changed",
                        _ => "unchanged",
                    };
                    let want = s["outcome"].as_str().unwrap();
                    r.check(
                        &format!("{what}: outcome"),
                        &outcome,
                        &if want == "same" { "changed" } else { want },
                    );
                    let log = r.log(&d);
                    r.lines(&format!("{what}: output"), &log, &s["log"]);
                }
                "undo_all" => {
                    r.check("undo steps", &(d.undo_steps() as u64), &s["undos"].as_u64().unwrap());
                }
                "save" => {
                    d.save(&file).unwrap();
                    let bak = PathBuf::from(format!("{}.bak", file.display()));
                    r.check(
                        "saved",
                        &sha1(&std::fs::read(&file).unwrap()),
                        &s["sha1"].as_str().unwrap().to_owned(),
                    );
                    r.check(
                        "bak",
                        &std::fs::read(&bak).ok().map(|b| sha1(&b)),
                        &s["bak"].as_str().map(str::to_owned),
                    );
                    let log = r.log(&d);
                    r.lines("save output", &log, &s["log"]);
                }
                "script" => {
                    let dir_s = dir.display().to_string();
                    let root_s = root.display().to_string();
                    let lines: Vec<String> = d
                        .script
                        .iter()
                        .map(|l| l.replace(&dir_s, "<dir>").replace(&root_s, "<opfor>"))
                        .collect();
                    r.lines("script", &lines, &s["lines"]);
                }
                "undone" => {
                    while d.undo_steps() > 0 {
                        d.undo();
                    }
                    r.check("undone", &bytes(&d), &s["sha1"].as_str().unwrap().to_owned());
                    r.check("undone is the original", &bytes(&d), &sha1(&original));
                    r.check("modified after undo", &d.modified, &s["modified"].as_bool().unwrap());
                    let mut log = r.log(&d);
                    log.dedup();
                    r.lines("undo output", &log, &s["log"]);
                }
                "find" => {
                    let q = s["query"].as_str().unwrap();
                    let found = d.find(q);
                    let paths: Vec<String> = found.iter().map(|p| join(p)).collect();
                    r.check(
                        &format!("find {q}: count"),
                        &(found.len() as u64),
                        &s["count"].as_u64().unwrap(),
                    );
                    r.check(
                        &format!("find {q}: matches"),
                        &sha1(paths.join("\n").as_bytes()),
                        &s["sha1"].as_str().unwrap().to_owned(),
                    );
                    if !found.is_empty() {
                        let status = format!("match 1 of {} for '{q}'", found.len());
                        r.check(&format!("find {q}: status"), &ds(Some(&status)), &s["status"]);
                    }
                    let log = r.log(&d);
                    r.lines(&format!("find {q}: output"), &log, &s["log"]);
                }
                "command" => {
                    let name = s["name"].as_str().unwrap();
                    let a = strs(&s["answers"]);
                    let arg = |i: usize| a.get(i).cloned().unwrap_or_default();
                    let (cmd, at) = match name.split_once('@') {
                        Some((c, p)) => (c, p.split(',').map(|x| x.parse().unwrap()).collect::<Vec<usize>>()),
                        None => (name, Vec::new()),
                    };
                    match cmd {
                        "ReplaceString" => d.replace(&arg(0), &arg(1)).unwrap(),
                        "ReplacePrefix" => d.replace_prefix(&arg(0), &arg(1), &arg(2)).unwrap(),
                        "SetByteField" => d.set_byte(&arg(0), &arg(1)).unwrap(),
                        "PartitionGuid" => d.partition_guid(&arg(0)).unwrap(),
                        "DropField" => d.drop_field(&arg(0)).unwrap(),
                        "AddItem" => d.add_item(&at, &arg(0)).unwrap(),
                        "DropInstance" => d.drop_instance(&at).unwrap(),
                        other => panic!("unknown command {other}"),
                    }
                    r.check(
                        &format!("{name}: bytes"),
                        &bytes(&d),
                        &s["sha1"].as_str().unwrap().to_owned(),
                    );
                    let log = r.log(&d);
                    r.lines(&format!("{name}: output"), &log, &s["log"]);
                }
                "graft" => {
                    let (sp, sf) = source(&root, s["source"].as_str().unwrap());
                    let guid = s["guid"].as_str().unwrap();
                    let at = s["before"].as_str().and_then(|b| d.instance_path(b));
                    d.graft(&sp, &sf, guid, at.as_deref()).unwrap();
                    r.check(
                        &format!("graft {guid}: bytes"),
                        &bytes(&d),
                        &s["sha1"].as_str().unwrap().to_owned(),
                    );
                    let log = r.log(&d);
                    r.lines(&format!("graft {guid}: output"), &log, &s["log"]);
                }
                "copyfield" => {
                    let (sp, sf) = source(&root, s["source"].as_str().unwrap());
                    let at = d.instance_path(s["target"].as_str().unwrap()).unwrap();
                    let field = s["field"].as_str().unwrap();
                    d.copy_field(&at, &sp, &sf, s["guid"].as_str().unwrap(), field).unwrap();
                    r.check(
                        &format!("copyfield {field}: bytes"),
                        &bytes(&d),
                        &s["sha1"].as_str().unwrap().to_owned(),
                    );
                    let log = r.log(&d);
                    r.lines(&format!("copyfield {field}: output"), &log, &s["log"]);
                }
                "check" => {
                    d.check_level(None).unwrap();
                    let log = r.log(&d);
                    r.lines("check", &log, &s["log"]);
                }
                "check_slots" => {
                    let (sp, sf) = source(&root, s["against"].as_str().unwrap());
                    d.check_level(Some((&sp, &sf))).unwrap();
                    let log = r.log(&d);
                    r.lines("check against the cull level", &log, &s["log"]);
                }
                "fixslots" => {
                    let (sp, sf) = source(&root, s["from"].as_str().unwrap());
                    d.fix_slots(&sp, &sf).unwrap();
                    r.check("fixslots: bytes", &bytes(&d), &s["sha1"].as_str().unwrap().to_owned());
                    let log = r.log(&d);
                    r.lines("fixslots: output", &log, &s["log"]);
                }
                "export" => {
                    let xml = file.with_extension("xml");
                    d.export_xml(&xml).unwrap();
                    r.check(
                        "export",
                        &sha1(&std::fs::read(&xml).unwrap()),
                        &s["sha1"].as_str().unwrap().to_owned(),
                    );
                    r.log(&d);
                }
                "import" => {
                    let xml = file.with_extension("xml");
                    let edited = mutate_xml(&String::from_utf8(std::fs::read(&xml).unwrap()).unwrap());
                    r.check(
                        "the edited XML",
                        &sha1(edited.as_bytes()),
                        &s["xml_sha1"].as_str().unwrap().to_owned(),
                    );
                    std::fs::write(&xml, edited.as_bytes()).unwrap();
                    match d.import_xml(&xml) {
                        Ok(()) => {
                            r.check(
                                "import: bytes",
                                &Some(bytes(&d)),
                                &s["sha1"].as_str().map(str::to_owned),
                            );
                            let log = r.log(&d);
                            r.lines("import: output", &log, &s["log"]);
                        }
                        Err(e) => {
                            let e = e.replace(&dir.display().to_string(), "<dir>");
                            r.check(&format!("import: refused {e}"), &ds(Some(&e)), &s["error"]);
                        }
                    }
                }
                "save_as" => {
                    let out = dir.join("saved.dbx");
                    d.save(&out).unwrap();
                    r.check(
                        "save as",
                        &sha1(&std::fs::read(&out).unwrap()),
                        &s["sha1"].as_str().unwrap().to_owned(),
                    );
                    let log = r.log(&d);
                    r.lines("save as: output", &log, &s["log"]);
                }
                other => panic!("unknown step {other}"),
            }
        }
        bad.append(&mut r.bad);
        let _ = std::fs::remove_dir_all(&dir);
        checked += 1;
    }
    println!(
        "{checked} of {} files checked ({steps} steps); skipped: {skipped:?}",
        files.len()
    );
    assert!(
        bad.is_empty(),
        "{} difference(s):\n{}",
        bad.len(),
        bad.iter().take(40).cloned().collect::<Vec<_>>().join("\n")
    );
    assert!(
        checked * 2 >= files.len(),
        "only {checked} of {} recorded files are here unchanged",
        files.len()
    );
}

/// Every unique binary `.dbx` of the prepared levels' folders (`work\levels\*\dist`): opened in the inspector and
/// written back byte for byte, its XML export imported back (typed from the file, as Import XML does) byte for byte -
/// what DbxEditGUI's corpus tests (`DbxTests.cs --corpus`, retired with it) asked of its classes.
#[test]
#[ignore = "game tier: every prepared level's .dbx files (run_tests.py game)"]
fn every_level_file_round_trips_through_the_inspector() {
    let _gate = crate::gate();
    let Ok(root) = levels::find_opfor(None) else {
        println!("skipped: no game copy");
        return;
    };
    let mut files = Vec::new();
    let mut stack: Vec<PathBuf> = std::fs::read_dir(root.join("work").join("levels"))
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path().join("dist"))
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default();
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dbx")) {
                files.push(p);
            }
        }
    }
    if files.is_empty() {
        println!("skipped: no prepared level");
        return;
    }
    files.sort();
    let t = std::time::Instant::now();
    let seen = std::sync::Mutex::new(std::collections::HashSet::new());
    let next = std::sync::atomic::AtomicUsize::new(0);
    let counts = std::sync::Mutex::new((0usize, 0usize, Vec::<String>::new()));
    std::thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(p) = files.get(i) else { break };
                    let Ok(data) = std::fs::read(p) else { continue };
                    if !seen.lock().unwrap().insert(sha1(&data)) {
                        continue;
                    }
                    let Ok(d) = DbxDoc::from_bytes(data.clone(), bc2_editor::dbxinspect::Source::File(p.clone()), "x")
                    else {
                        continue;
                    };
                    let mut bad = None;
                    if d.bytes().unwrap() != data {
                        bad = Some("written back");
                    } else {
                        let xml = dx::convert(&data).unwrap();
                        let reference = EditFile::parse(data.clone()).unwrap();
                        match dx::import(|| Ok(xml), "x.xml", Some(&reference), &[]) {
                            Ok((back, _)) if back.serialize().unwrap() == data => {}
                            _ => bad = Some("XML imported back"),
                        }
                    }
                    let mut c = counts.lock().unwrap();
                    c.0 += 1;
                    if let Some(b) = bad {
                        c.1 += 1;
                        c.2.push(format!("{b}: {}", p.display()));
                    }
                }
            });
        }
    });
    let (n, failed, which) = counts.into_inner().unwrap();
    println!(
        "{n} unique binary .dbx files of {} in {:.0} s: {failed} differ",
        files.len(),
        t.elapsed().as_secs_f64()
    );
    assert!(n > 0);
    assert_eq!(failed, 0, "{:?}", which.iter().take(20).collect::<Vec<_>>());
}
