//! Stage 12 of the console Onslaught port (round OR12): the port of Project OPFOR's `console_loading.py`'s
//! `console_setup` and `build` up to the ledger - the console's own Onslaught loading video instead of the PC Rush
//! map's, and the description's game mode:
//!
//! - the console level's `LoadingMovie` partition (e.g. `Levels/MP_008/UI/MP08_ONS`, a `Render.BinkMemoryAsset`) and
//!   its Bink resource copied from the user's own Xbox loader bundle into the level's Package `level-00` (the
//!   partition), `loader-00` (partition and resource) and the shared `startup-00` (the partition, beside the
//!   description that names it, as every stock description's movie is there);
//! - `LevelDescription.LoadingMovie` in the level and in its description pointed at it;
//! - OPFOR's `GameModes/Onslaught` (stage 3) added to `startup-00` and the description's `GameMode` pointed at it
//!   (the loading screen shows the mode of the description's game mode).
//!
//! Only the bundles that change are packed; the ledger's commit (owner `loading:<level>`, `startup-00` one layer per
//! level) and the reverts stay in OPFOR's `console_loading.py`. The work folder's files are the Python's byte for byte
//! (rust/BENCHMARKS.md "OR12").

use std::sync::LazyLock;

use regex::Regex;

use super::port::{StageOut, find, read_text_utf8, write_text_utf8};
use super::{OnsError, R, Source, copy, dbx, fbrb, join, joins, pack, read, rmtree, stem4, write};

/// The console levels (`console_common.LEVELS`).
pub const LEVELS: [&str; 4] = ["ons_mp_002", "ons_mp_004", "ons_mp_005", "ons_mp_008"];
/// OPFOR's Onslaught game mode partition (stage 3), in the level bundle.
pub const GAMEMODE: &str = "GameModes/Onslaught";
/// The three bundles a level's loading edits go into, in the order they are packed.
const BUNDLES: [&str; 3] = ["level", "loader", "startup"];

fn re(p: &str) -> Regex {
    #[allow(
        clippy::expect_used,
        reason = "the patterns are constants, each compiled by the tests"
    )]
    Regex::new(p).expect("a constant pattern")
}

/// `console_common.HEAD`.
static HEAD: LazyLock<Regex> = LazyLock::new(|| re(r#"<partition guid="([^"]+)" primaryInstance="([^"]+)""#));
/// The console level's `LoadingMovie` reference (its partition path).
static MOVIE_REF: LazyLock<Regex> = LazyLock::new(|| re(r#"ref="([^"]+)/[0-9a-fA-F-]{36}" name="LoadingMovie""#));

/// What stage 12 is given.
#[derive(Debug, Clone)]
pub struct Args {
    /// The OPFOR folder: `work\console_loading\` written; the console level and loaders read (the console level's XML
    /// written beside it when it has none).
    pub root: String,
    /// The game copy: read only.
    pub game: String,
    /// The Onslaught level (lower case).
    pub lv: String,
    /// `--apply` (the changed bundles packed) rather than `--plan`.
    pub write: bool,
}

/// `console_setup(lv)`: the console level's loading video and the console loader bundle that carries it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setup {
    /// The level's name (upper case).
    pub name: String,
    /// The movie's partition path as the console bundles spell it (`Levels/MP_008/UI/MP08_ONS`).
    pub movie: String,
    /// The console loader bundle (stage 1's extract of the user's Xbox package).
    pub console_loader: String,
}

/// `xml_of(dbx)`: DbxEdit's XML of a .dbx (written beside it), read as text.
fn xml_of(path: &str) -> R<String> {
    dbx(&["xml", path])?;
    read_text_utf8(&format!("{}.xml", stem4(path)))
}

fn need<'t>(c: Option<regex::Captures<'t>>, what: &str) -> R<regex::Captures<'t>> {
    c.ok_or_else(|| OnsError::Data(format!("not found: {what}")))
}

/// `str.capitalize()`: the first character upper case, the rest lower.
fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| {
        f.to_uppercase().chain(c.flat_map(char::to_lowercase)).collect()
    })
}

/// `console_setup(lv)`.
pub fn console_setup(root: &str, lv: &str) -> R<Setup> {
    let con = joins(root, &["work", "onslaught", &format!("build_{lv}"), "con_level"]);
    let xml = joins(&con, &["levels", &format!("{}.xml", lv.to_uppercase())]);
    if !std::path::Path::new(&xml).is_file() {
        xml_of(&format!("{}.dbx", stem4(&xml)))?;
    }
    let text = read_text_utf8(&xml)?;
    let m = need(MOVIE_REF.captures(&text), "LoadingMovie")?;
    let parts: Vec<&str> = m[1].split('/').collect();
    let (Some(first), Some(second)) = (parts.first(), parts.get(1)) else {
        return Err(OnsError::Data("IndexError: list index out of range".into()));
    };
    let mut movie = vec![capitalize(first), second.to_uppercase()];
    movie.extend(parts.iter().skip(2).map(|p| p.to_uppercase()));
    let mut loader = joins(
        root,
        &[
            "work",
            "onslaught",
            &format!("build_{lv}"),
            "xbox",
            "levels",
            lv,
            "loader-00.fbrb",
        ],
    );
    if !std::path::Path::new(&loader).is_file() {
        // Nelson Bay was built from the earlier u3 extract
        loader = joins(root, &["work", "xbox_dlc", "u3", "levels", lv, "loader-00.fbrb"]);
    }
    Ok(Setup {
        name: lv.to_uppercase(),
        movie: movie.join("/"),
        console_loader: loader,
    })
}

fn manifest(folder: &str) -> R<Vec<String>> {
    Ok(
        bc2_pyrt::py::splitlines(&read_text_utf8(&join(folder, "fbrb.manifest"))?)
            .into_iter()
            .map(str::to_owned)
            .collect(),
    )
}

fn first_field(line: &str) -> String {
    line.split('|').next().unwrap_or("").to_lowercase()
}

/// `manifest_line(folder, rel)`: the entry's manifest line.
fn manifest_line(folder: &str, rel: &str) -> R<String> {
    let want = rel.to_lowercase();
    manifest(folder)?
        .into_iter()
        .find(|l| first_field(l) == want)
        .ok_or_else(|| OnsError::Exit(format!("{rel} not in {folder}\\fbrb.manifest")))
}

/// `add_entry(src, dst, rel)`: entry `rel` (file and manifest line) copied from unpacked bundle `src` into unpacked
/// bundle `dst`, if absent there; true when added.
fn add_entry(src: &str, dst: &str, rel: &str) -> R<bool> {
    let mut lines = manifest(dst)?;
    let want = rel.to_lowercase();
    if lines.iter().any(|l| first_field(l) == want) {
        return Ok(false);
    }
    let path = join(dst, &rel.replace('/', "\\"));
    copy(&find(src, rel)?, &path)?;
    lines.push(manifest_line(src, rel)?);
    write(&join(dst, "fbrb.manifest"), (lines.join("\n") + "\n").as_bytes())?;
    Ok(true)
}

/// `retarget(dbx, new_ref, field)`: reference field `field` of a level or description partition pointed at
/// `new_ref` (imported back through an XML beside it); the old reference.
fn retarget(path: &str, new_ref: &str, field: &str) -> R<String> {
    let text = xml_of(path)?;
    let r = re(&format!(r#"ref="([^"]+)" name="{}""#, regex::escape(field)));
    let old = need(r.captures(&text), field)?[1].to_owned();
    if old.to_lowercase() != new_ref.to_lowercase() {
        let stem = stem4(path);
        let x = format!("{stem}.new.xml");
        write_text_utf8(
            &x,
            &text.replace(
                &format!(r#"ref="{old}" name="{field}""#),
                &format!(r#"ref="{new_ref}" name="{field}""#),
            ),
        )?;
        let out = format!("{stem}.new.dbx");
        dbx(&["import", &x, &out, "--like", path])?;
        write(path, &read(&out)?)?;
        for f in [x, out, format!("{stem}.xml")] {
            if std::path::Path::new(&f).is_file() {
                std::fs::remove_file(&f).map_err(|e| super::io_err(&f, &e))?;
            }
        }
    }
    Ok(old)
}

/// `bundles(lv)`: the level's three bundles (level, loader, the shared startup).
fn bundle_paths(game: &str, lv: &str) -> [String; 3] {
    let d = joins(game, &["Package", "levels", lv]);
    [
        join(&d, "level-00.fbrb"),
        join(&d, "loader-00.fbrb"),
        joins(game, &["Package", "async", "startup-00.fbrb"]),
    ]
}

/// The movie's partition into the level, partition and Bink resource into the loader, and the partition into the
/// startup bundle beside the description that names it - as every stock description's `LoadingMovie` partition is
/// there (MP05_CQ, MP08_RUSH ...; the Bink resource only in the loader): without it the four Onslaught descriptions'
/// `LoadingMovie` dangled in startup-00 (texture / reference QA 2026-11-09). Which of the three changed.
fn add_movie(con: &str, [level, loader, startup]: [&str; 3], movie: &str) -> R<[bool; 3]> {
    let dbx = format!("{movie}.dbx");
    let l = add_entry(con, level, &dbx)?;
    // both added, as the Python's `|` evaluates both
    let a1 = add_entry(con, loader, &dbx)?;
    let a2 = add_entry(con, loader, &format!("{movie}.res"))?;
    let s = add_entry(con, startup, &dbx)?;
    Ok([l, a1 | a2, s])
}

/// `console_loading.build(lv, write)`: the lines printed go to `out` as they come; with `write`, the changed bundles
/// packed and their writes (level, loader, startup order), or "already applied".
pub fn build(a: &Args, out: &mut StageOut) -> R<()> {
    let lv = a.lv.as_str();
    if !LEVELS.contains(&lv) {
        return Err(OnsError::Data(format!("KeyError: '{lv}'")));
    }
    let cfg = console_setup(&a.root, lv)?;
    let root = joins(&a.root, &["work", "console_loading", lv]);
    rmtree(&root)?;
    let con = join(&root, "console_loader");
    fbrb(&["unpack", &cfg.console_loader, &con])?;
    let movie_dbx = format!("{}.dbx", cfg.movie);
    let head = xml_of(&find(&con, &movie_dbx)?)?;
    let new_ref = format!(
        "{}/{}",
        cfg.movie.to_lowercase(),
        need(HEAD.captures(&head), "partition head")?[2].to_lowercase()
    );
    let bundles = bundle_paths(&a.game, lv);
    let unpacked: Vec<String> = BUNDLES.iter().map(|k| join(&root, k)).collect();
    for (b, u) in bundles.iter().zip(&unpacked) {
        fbrb(&["unpack", b, u])?;
    }
    let [level, loader, startup] = [&unpacked[0], &unpacked[1], &unpacked[2]];
    let mut changed = add_movie(&con, [level, loader, startup], &cfg.movie)?;
    let old_level = retarget(
        &find(level, &format!("Levels/{}.dbx", cfg.name))?,
        &new_ref,
        "LoadingMovie",
    )?;
    let desc = find(startup, &format!("Levels/{}_description.dbx", cfg.name))?;
    let old_desc = retarget(&desc, &new_ref, "LoadingMovie")?;
    changed[0] |= old_level.to_lowercase() != new_ref.to_lowercase();
    changed[2] |= old_desc.to_lowercase() != new_ref.to_lowercase();
    out.printed.push(format!(
        "{lv}: loading video {}; level {old_level} -> {new_ref}; description {old_desc} -> {new_ref}",
        cfg.movie
    ));
    let gm = find(level, &format!("{GAMEMODE}.dbx"))?;
    let gm_ref = format!(
        "{}/{}",
        GAMEMODE.to_lowercase(),
        need(HEAD.captures(&xml_of(&gm)?), "partition head")?[2].to_lowercase()
    );
    changed[2] |= add_entry(level, startup, &format!("{GAMEMODE}.dbx"))?;
    let old_gm = retarget(&desc, &gm_ref, "GameMode")?;
    changed[2] |= old_gm.to_lowercase() != gm_ref.to_lowercase();
    out.printed
        .push(format!("{lv}: description game mode {old_gm} -> {gm_ref}"));
    if !a.write {
        return Ok(());
    }
    if !changed.contains(&true) {
        out.printed.push(format!("{lv}: already applied"));
        return Ok(());
    }
    for (((k, b), u), c) in BUNDLES.iter().zip(&bundles).zip(&unpacked).zip(changed) {
        if c {
            let built = join(&root, &format!("{k}.fbrb"));
            pack(u, &built)?;
            out.writes.push((b.clone(), Source::File(built)));
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        let _ = (&*HEAD, &*MOVIE_REF);
        assert_eq!(capitalize("levels"), "Levels");
        assert_eq!(capitalize("LEVELS"), "Levels");
        assert_eq!(capitalize(""), "");
        let c = MOVIE_REF
            .captures(
                r#"<field ref="levels/mp_008/ui/mp08_ons/0B2B8D49-CE13-4B4F-9E7B-7E6E1A6A63C7" name="LoadingMovie" />"#,
            )
            .unwrap();
        assert_eq!(&c[1], "levels/mp_008/ui/mp08_ons");
        assert_eq!(first_field("Levels/A.dbx|x|1"), "levels/a.dbx");
    }

    #[test]
    fn entries() {
        let d = std::env::temp_dir().join(format!("bc2-loading-test-{}", std::process::id()));
        let d = d.to_string_lossy().into_owned();
        let (src, dst) = (join(&d, "src"), join(&d, "dst"));
        write(&joins(&src, &["Levels", "M.dbx"]), b"part").unwrap();
        write(
            &join(&src, "fbrb.manifest"),
            b"Levels/M.dbx|<non-resource>|00010000|4|4\n",
        )
        .unwrap();
        write(&join(&dst, "fbrb.manifest"), b"first\n").unwrap();
        assert!(add_entry(&src, &dst, "levels/m.dbx").unwrap());
        assert!(!add_entry(&src, &dst, "Levels/M.dbx").unwrap());
        assert_eq!(
            read(&join(&dst, "fbrb.manifest")).unwrap(),
            b"first\nLevels/M.dbx|<non-resource>|00010000|4|4\n"
        );
        assert!(add_entry(&src, &dst, "Levels/N.dbx").is_err());
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// The movie's partition goes into startup-00 beside the description (the Bink resource only into the loader);
    /// a second run adds nothing.
    #[test]
    fn movie_in_startup() {
        let d = std::env::temp_dir().join(format!("bc2-loading-movie-{}", std::process::id()));
        let d = d.to_string_lossy().into_owned();
        let movie = "Levels/MP_008/UI/MP08_ONS";
        let bundle = |name: &str, files: &[&str]| {
            let root = join(&d, name);
            let mut man = String::new();
            for rel in files {
                write(&join(&root, &rel.replace('/', "\\")), b"<partition />").unwrap();
                man.push_str(&format!("{rel}|x|1\n"));
            }
            write(&join(&root, "fbrb.manifest"), man.as_bytes()).unwrap();
            root
        };
        let con = bundle("con", &[&format!("{movie}.dbx"), &format!("{movie}.res")]);
        let level = bundle("level", &["Levels/ONS_MP_008.dbx"]);
        let loader = bundle("loader", &["Levels/MP_008/UI/MP08_RUSH.res"]);
        let startup = bundle("startup", &["Levels/ONS_MP_008_description.dbx"]);
        let ch = add_movie(&con, [&level, &loader, &startup], movie).unwrap();
        assert_eq!(ch, [true, true, true]);
        let man = |f: &str| String::from_utf8(read(&join(f, "fbrb.manifest")).unwrap()).unwrap().to_lowercase();
        let m = movie.to_lowercase();
        assert!(man(&startup).contains(&format!("{m}.dbx")) && !man(&startup).contains(&format!("{m}.res")));
        assert!(man(&level).contains(&format!("{m}.dbx")) && man(&loader).contains(&format!("{m}.res")));
        assert_eq!(add_movie(&con, [&level, &loader, &startup], movie).unwrap(), [false; 3]);
        std::fs::remove_dir_all(&d).unwrap();
    }
}
