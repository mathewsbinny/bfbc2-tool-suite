//! Stage 13 of the console Onslaught port (round OR12): the port of Project OPFOR's `console_hud.py`'s `build`,
//! `apply`'s packs, `legacy_record`, `unpatched` and `patch_movie` - the console HUD's Onslaught stopwatch in the PC
//! HUD movie and Onslaught's texts in the PC menus that switch on the game mode ([`patches`]):
//!
//! | bundle | movie | edits |
//! |---|---|---|
//! | `ingame-00` | `Menu/hudlayer_shared.res` | the console's timer element, its visibility and its two events (`onslaught-timer`) |
//! | `ingame-00` | `Menu/mp_objectives.res` | `getMapDesc` -> `MP_LOADING_OBJECTIVE_ONSLAUGHT` |
//! | `ingame-00` | `Menu/hud_root.res` | onslaught switched as conquest (the spawn screen's team names by faction) |
//! | `loader-00` | `Menu/loadingscreen_mp.res` | the headline `MAINMENU_COOP`, the objective text, goldrush's team name, tdm's mode panel |
//!
//! The edits are `bc2_menu::graft`'s (the movie model OPFOR's tools use through `bc2_native.AptMovie` since round
//! OR14), run in process where the Python ran `bfbc2 aptgraft`'s commands; each step's printed line is the one that
//! command printed last. The movies come from the level's own bundles and the console movie from the user's own Xbox
//! package extract; nothing is shipped.
//!
//! The ledger's parts stay in OPFOR's `console_hud.py`: the check before the builds, each bundle's source and the
//! file before this stage (passed in), the install, and after it the kept originals and the entry records beside the
//! bundles ([`Record`]). The work folder's files are the Python's byte for byte (rust/BENCHMARKS.md "OR12").

use bc2_menu::graft::{GRAFT_LAYOUT, edit, load};

use super::port::{StageOut, find, read_text_utf8};
use super::{OnsError, R, Source, copy, fbrb, join, joins, pack, rmtree, size, write};

/// The mode the edits add.
pub const MODE: &str = "onslaught";
/// The levels the stage runs on without `--level` (`console_hud.LEVELS`).
pub const LEVELS: [&str; 1] = ["ons_mp_008"];
/// The entry record beside a bundle: `{entry: {"orig": sha, "patched": sha}}` of the last apply.
pub const ENTRIES: &str = ".opfor-hud-entries.json";

/// One `aptgraft` step on a movie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// `onslaught-timer <movie> <console hudlayer_shared> <out>`.
    Timer,
    /// `mode-case --function F --mode onslaught --key K [--fallback T]`.
    Case {
        /// The function.
        function: &'static str,
        /// The text key returned for the mode.
        key: &'static str,
        /// The text the function returns for an unknown mode.
        fallback: &'static str,
    },
    /// `mode-alias --mode onslaught --like L [--function F]`.
    Alias {
        /// The mode the switches send onslaught the way of.
        like: &'static str,
        /// The function (every switch when none).
        function: Option<&'static str>,
    },
}

/// `aptgraft mode-case`'s default fallback.
const DESC_FALLBACK: &str = "Missing DESC tag!! ";

/// The loading screen's steps (`LOADING_SCREEN`; also the main menu's copy, `--loading-screen`).
pub const LOADING_SCREEN: [Step; 4] = [
    Step::Case {
        function: "getHeadline",
        key: "MAINMENU_COOP",
        fallback: "Missing GM tag: ",
    },
    Step::Case {
        function: "getMapDesc",
        key: "MP_LOADING_OBJECTIVE_ONSLAUGHT",
        fallback: DESC_FALLBACK,
    },
    Step::Alias {
        like: "goldrush",
        function: Some("getTeamName"),
    },
    Step::Alias {
        like: "tdm",
        function: Some("showGameModeComponents"),
    },
];

/// A movie of a bundle (its entry) and its steps.
pub type Patch = (&'static str, Vec<Step>);

/// `PATCHES`: bundle (`Package\levels\<lv>\<name>.fbrb`) -> entry -> steps, in order.
#[must_use]
pub fn patches() -> Vec<(&'static str, Vec<Patch>)> {
    vec![
        (
            "ingame-00",
            vec![
                ("Menu/hudlayer_shared.res", vec![Step::Timer]),
                (
                    "Menu/mp_objectives.res",
                    vec![Step::Case {
                        function: "getMapDesc",
                        key: "MP_LOADING_OBJECTIVE_ONSLAUGHT",
                        fallback: DESC_FALLBACK,
                    }],
                ),
                (
                    "Menu/hud_root.res",
                    vec![Step::Alias {
                        like: "conquest",
                        function: None,
                    }],
                ),
            ],
        ),
        (
            "loader-00",
            vec![("Menu/loadingscreen_mp.res", LOADING_SCREEN.to_vec())],
        ),
    ]
}

impl Step {
    /// The command's name and options as `aptgraft`'s command line takes them (for a failure's message).
    fn command(self, cur: &str, out: &str, console: &str) -> String {
        match self {
            Self::Timer => format!("aptgraft onslaught-timer {cur} {console} {out}"),
            Self::Case {
                function,
                key,
                fallback,
            } => {
                let fb = if fallback == DESC_FALLBACK {
                    String::new()
                } else {
                    format!(" --fallback {fallback}")
                };
                format!("aptgraft mode-case {cur} {out} --function {function} --mode {MODE} --key {key}{fb}")
            }
            Self::Alias { like, function } => format!(
                "aptgraft mode-alias {cur} {out} --mode {MODE} --like {like}{}",
                function.map_or_else(String::new, |f| format!(" --function {f}"))
            ),
        }
    }

    /// The step on movie `cur`, written to `out`: the line `aptgraft` printed last.
    fn run(self, cur: &str, out: &str, console: &str) -> R<String> {
        let fail =
            |e: bc2_menu::error::MenuError| OnsError::Exit(format!("failed: {}\n{e}", self.command(cur, out, console)));
        let mut m = load(cur).map_err(fail)?;
        let line = match self {
            Self::Timer => {
                let mut con = load(console).map_err(fail)?;
                edit::onslaught_timer(&mut m, &mut con, true, true).map_err(fail)?;
                format!("{out}: written")
            }
            Self::Case {
                function,
                key,
                fallback,
            } => {
                let n = edit::add_mode_case(&mut m, function, MODE, key, fallback).map_err(fail)?;
                format!("{out}: {function} ({n}) returns {key} for {MODE}")
            }
            Self::Alias { like, function } => {
                let funcs = function.map(|f| vec![f.to_owned()]);
                let n = edit::add_mode_alias(&mut m, MODE, like, funcs.as_deref()).map_err(fail)?;
                format!("{out}: {n} switch(es) send {MODE} the way of {like}")
            }
        };
        let data = m.to_res(m.big, GRAFT_LAYOUT).map_err(fail)?;
        write(out, &data)?;
        Ok(line)
    }
}

/// What stage 13 is given.
#[derive(Debug, Clone)]
pub struct Args {
    /// The OPFOR folder: `work\console_hud\` written, the console movie read.
    pub root: String,
    /// The game copy: read only (the entry records beside the bundles read).
    pub game: String,
    /// The Onslaught level (lower case).
    pub lv: String,
    /// Per bundle of [`patches`], in order: the file the stage builds from (`cc.source_bundle`: the bundle, or the
    /// ledger's copy of it before this stage's last write when it redoes that write) and the file before this stage
    /// (`cc.before`; `None` when the stage did not install it).
    pub sources: Vec<(String, Option<String>)>,
    /// `--apply` (the bundles packed) rather than `--plan`.
    pub write: bool,
}

/// One bundle's entry record, written beside it once the ledger's commit is done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// The bundle in the game copy.
    pub bundle: String,
    /// `json.dumps(record, indent=1)`.
    pub json: String,
    /// The kept originals: each `<keep>.new` becomes `<keep>` (`os.replace`).
    pub keeps: Vec<String>,
}

/// `{entry: (orig, patched)}`, in the order the entries were patched.
type Rec = Vec<(String, (String, String))>;

fn sha(p: &str) -> R<String> {
    bc2_gamefs::sha256::file(p).map_err(|e| super::io_err(p, &e))
}

fn bundle_path(game: &str, lv: &str, name: &str) -> String {
    joins(game, &["Package", "levels", lv, &format!("{name}.fbrb")])
}

/// `orig_copy(lv, name, entry)`: where the movie as it was before this stage patched it is kept.
#[must_use]
pub fn orig_copy(root: &str, lv: &str, name: &str, entry: &str) -> String {
    let mut parts = vec![lv.to_owned(), format!("{name}.orig")];
    parts.extend(entry.split('/').map(str::to_owned));
    let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
    joins(&work(root), &parts)
}

fn work(root: &str) -> String {
    joins(root, &["work", "console_hud"])
}

/// `entry_record(bundle)`: the record beside the bundle (`{}` without one).
fn entry_record(bundle: &str) -> R<Rec> {
    let p = format!("{bundle}{ENTRIES}");
    if !std::path::Path::new(&p).is_file() {
        return Ok(Vec::new());
    }
    let v: serde_json::Value =
        serde_json::from_str(&read_text_utf8(&p)?).map_err(|e| OnsError::Data(format!("{p}: {e}")))?;
    let obj = v
        .as_object()
        .ok_or_else(|| OnsError::Data(format!("{p}: not a JSON object")))?;
    let mut out = Vec::new();
    for (k, r) in obj {
        let get = |f: &str| {
            r.get(f)
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| OnsError::Data(format!("KeyError: '{f}'")))
        };
        out.push((k.clone(), (get("orig")?, get("patched")?)));
    }
    Ok(out)
}

fn rec_get<'r>(rec: &'r Rec, entry: &str) -> Option<&'r (String, String)> {
    let k = entry.to_lowercase();
    rec.iter().rev().find(|(e, _)| *e == k).map(|(_, v)| v)
}

/// `json.dumps(rec, indent=1)` (the keys and digests are ASCII).
fn dumps(rec: &Rec) -> String {
    if rec.is_empty() {
        return "{}".to_owned();
    }
    let items: Vec<String> = rec
        .iter()
        .map(|(k, (o, p))| format!(" \"{k}\": {{\n  \"orig\": \"{o}\",\n  \"patched\": \"{p}\"\n }}"))
        .collect();
    format!("{{\n{}\n}}", items.join(",\n"))
}

/// `unpatched(...)`: the movie at `path` made ready to patch - this stage's own patched result put back to the
/// original kept at the last apply (the patches never apply twice); anything else patched as it is.
fn unpatched(root: &str, lv: &str, name: &str, entry: &str, path: &str, rec: &Rec) -> R<()> {
    if let Some((orig, patched)) = rec_get(rec, entry)
        && sha(path)? == *patched
    {
        let keep = orig_copy(root, lv, name, entry);
        if !std::path::Path::new(&keep).is_file() || sha(&keep)? != *orig {
            return Err(OnsError::Exit(format!(
                "{lv} {name} {entry}: the bundle holds this stage's patched movie, and the original kept at the last \
                 --apply ({keep}) is missing or changed - not patched twice"
            )));
        }
        copy(&keep, path)?;
    }
    Ok(())
}

/// `legacy_record(lv, name, bundle, backup)`: the record of an install made before the record existed (that version
/// patched the backup's movies: an entry that differs from the backup's is its patch, the backup's the original).
fn legacy_record(a: &Args, name: &str, entries: &[&str], bundle: &str, backup: &str) -> R<Rec> {
    let w = joins(&work(&a.root), &[&a.lv]);
    let tmp = join(&w, &format!("{name}.legacy"));
    let cur = join(&w, &format!("{name}.current"));
    rmtree(&tmp)?;
    rmtree(&cur)?;
    fbrb(&["unpack", backup, &tmp])?;
    fbrb(&["unpack", bundle, &cur])?;
    let mut rec = Vec::new();
    for entry in entries {
        let (o, p) = (find(&tmp, entry)?, find(&cur, entry)?);
        let (so, sp) = (sha(&o)?, sha(&p)?);
        if so != sp {
            copy(&o, &orig_copy(&a.root, &a.lv, name, entry))?;
            rec.push((entry.to_lowercase(), (so, sp)));
        }
    }
    rmtree(&tmp)?;
    rmtree(&cur)?;
    Ok(rec)
}

/// `os.path.splitext(os.path.basename(entry))[0]`.
fn stem(entry: &str) -> &str {
    let b = entry.rsplit(['/', '\\']).next().unwrap_or(entry);
    match b.rfind('.') {
        Some(i) if b[..i].chars().any(|c| c != '.') => &b[..i],
        _ => b,
    }
}

/// `build(lv, name)`: the bundle this stage builds on unpacked into `work\console_hud\<lv>\<name>`, its movies
/// patched; the unpacked folder and the entry record.
fn build_one(
    a: &Args,
    name: &str,
    entries: &[Patch],
    src_bundle: &str,
    prior: Option<&str>,
    out: &mut StageOut,
) -> R<(String, Rec)> {
    let bundle = bundle_path(&a.game, &a.lv, name);
    let mut rec = if src_bundle == bundle {
        entry_record(&bundle)?
    } else {
        Vec::new()
    };
    if src_bundle == bundle
        && rec.is_empty()
        && let Some(prior) = prior.filter(|p| !p.is_empty() && *p != bundle)
    {
        let names: Vec<&str> = entries.iter().map(|e| e.0).collect();
        rec = legacy_record(a, name, &names, &bundle, prior)?;
    }
    let w = joins(&work(&a.root), &[&a.lv]);
    let src = join(&w, name);
    rmtree(&src)?;
    fbrb(&["unpack", src_bundle, &src])?;
    let man = join(&src, "fbrb.manifest");
    let mut lines: Vec<String> = bc2_pyrt::py::splitlines(&read_text_utf8(&man)?)
        .into_iter()
        .map(str::to_owned)
        .collect();
    let console = joins(
        &a.root,
        &["work", "xbox_dlc", "hud", "con_ingame", "Menu", "hudlayer_shared.res"],
    );
    let mut new_rec = Vec::new();
    for (entry, steps) in entries {
        let dst = find(&src, entry)?;
        unpatched(&a.root, &a.lv, name, entry, &dst, &rec)?;
        let keep = orig_copy(&a.root, &a.lv, name, entry);
        let keep_new = format!("{keep}.new");
        copy(&dst, &keep_new)?;
        let mut cur = dst.clone();
        for (k, step) in steps.iter().enumerate() {
            let o = join(&w, &format!("{}.{k}.res", stem(entry)));
            let line = step.run(&cur, &o, &console)?;
            out.printed.push(format!("{} {entry}: {line}", a.lv));
            cur = o;
        }
        new_rec.push((entry.to_lowercase(), (sha(&keep_new)?, sha(&cur)?)));
        copy(&cur, &dst)?;
        let rel = super::rel(&dst, &src).replace('\\', "/").to_lowercase();
        let sz = size(&dst)?;
        for line in &mut lines {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() == 5 && parts.first().is_some_and(|f| f.to_lowercase() == rel) {
                *line = format!("{}|{sz}|{sz}", parts.get(..3).unwrap_or_default().join("|"));
            }
        }
    }
    write(&man, (lines.join("\n") + "\n").as_bytes())?;
    Ok((src, new_rec))
}

/// `apply(lv, write)` after the ledger's check: each bundle built (and with `write` packed); the game copy's writes
/// in order and, for after the commit, each bundle's record.
pub fn build(a: &Args, out: &mut StageOut) -> R<Vec<Record>> {
    let p = patches();
    if a.sources.len() != p.len() {
        return Err(OnsError::Data(format!(
            "{} bundle sources given, {} bundles patched",
            a.sources.len(),
            p.len()
        )));
    }
    let mut records = Vec::new();
    for ((name, entries), (src_bundle, prior)) in p.iter().zip(&a.sources) {
        let (src, rec) = build_one(a, name, entries, src_bundle, prior.as_deref(), out)?;
        if !a.write {
            continue;
        }
        let built = joins(&work(&a.root), &[&a.lv, &format!("{name}.fbrb")]);
        pack(&src, &built)?;
        let bundle = bundle_path(&a.game, &a.lv, name);
        out.writes.push((bundle.clone(), Source::File(built)));
        records.push(Record {
            bundle,
            json: dumps(&rec),
            keeps: entries
                .iter()
                .map(|(e, _)| orig_copy(&a.root, &a.lv, name, e))
                .collect(),
        });
    }
    Ok(records)
}

/// `patch_movie(movie, LOADING_SCREEN, tag)` (`--loading-screen <movie>`, build_menus.bat's main menu copy): the
/// steps on the movie in place, through `<movie>.<k>.tmp`.
pub fn loading_screen(movie: &str, tag: &str, out: &mut StageOut) -> R<()> {
    let mut cur = movie.to_owned();
    for (k, step) in LOADING_SCREEN.iter().enumerate() {
        let o = format!("{movie}.{k}.tmp");
        let line = step.run(&cur, &o, "")?;
        out.printed.push(format!("{tag}: {line}"));
        if cur != movie {
            std::fs::remove_file(&cur).map_err(|e| super::io_err(&cur, &e))?;
        }
        cur = o;
    }
    // shutil.move: a rename over the movie
    std::fs::rename(&cur, movie).map_err(|e| super::io_err(movie, &e))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// The build starts from the current movie and never patches twice: its own patched result goes back to the kept
    /// original, another stage's change is kept, a lost original refuses.
    #[test]
    fn never_patches_twice() {
        let d = std::env::temp_dir().join(format!("bc2-hud-test-{}", std::process::id()));
        let root = d.to_string_lossy().into_owned();
        let entry = "Menu/hud_root.res";
        let movie = joins(&root, &["unpacked", "Menu", "hud_root.res"]);
        write(&movie, b"patched by hud").unwrap();
        let keep = orig_copy(&root, "ons_mp_008", "ingame-00", entry);
        write(&keep, b"pc movie").unwrap();
        let rec: Rec = vec![(entry.to_lowercase(), (sha(&keep).unwrap(), sha(&movie).unwrap()))];
        unpatched(&root, "ons_mp_008", "ingame-00", entry, &movie, &rec).unwrap();
        assert_eq!(super::super::read(&movie).unwrap(), b"pc movie");
        let other = joins(&root, &["unpacked", "Menu", "mp_objectives.res"]);
        write(&other, b"changed by a later stage").unwrap();
        unpatched(&root, "ons_mp_008", "ingame-00", "Menu/mp_objectives.res", &other, &rec).unwrap();
        assert_eq!(super::super::read(&other).unwrap(), b"changed by a later stage");
        write(&keep, b"lost").unwrap();
        write(&movie, b"patched by hud").unwrap();
        let e = unpatched(&root, "ons_mp_008", "ingame-00", entry, &movie, &rec).unwrap_err();
        assert!(e.to_string().contains("not patched twice"), "{e}");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn records_and_names() {
        assert_eq!(dumps(&Vec::new()), "{}");
        let r = vec![
            ("menu/a.res".to_owned(), ("1".to_owned(), "2".to_owned())),
            ("menu/b.res".to_owned(), ("3".to_owned(), "4".to_owned())),
        ];
        assert_eq!(
            dumps(&r),
            "{\n \"menu/a.res\": {\n  \"orig\": \"1\",\n  \"patched\": \"2\"\n },\n \"menu/b.res\": {\n  \"orig\": \"3\",\n  \"patched\": \"4\"\n }\n}"
        );
        assert_eq!(rec_get(&r, "Menu/B.res").unwrap().1, "4");
        assert_eq!(stem("Menu/hud_root.res"), "hud_root");
        assert_eq!(stem("Menu/.res"), ".res");
        assert_eq!(
            orig_copy("r", "l", "ingame-00", "Menu/x.res"),
            r"r\work\console_hud\l\ingame-00.orig\Menu\x.res"
        );
        assert_eq!(patches().iter().map(|p| p.1.len()).sum::<usize>(), 4);
        assert!(
            Step::Case {
                function: "f",
                key: "k",
                fallback: "x"
            }
            .command("a", "b", "")
            .ends_with("--key k --fallback x")
        );
    }
}
