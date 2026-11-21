//! Round EX1's proofs on real data (`EDITOR_ARCHITECTURE.md` 3.12, "The level's names"): every name of an export
//! edited and checked together, and landing consistently.
//!
//! - The rules against the game copy: a stock level's id, another project's install, an id holding `_NAME` (the black
//!   loading screen), a localisation key or loading picture the game already has, a folder that is not the id, a
//!   server-browser name another map has, a letter a language cannot show - each refused by the build in words.
//! - An E3d canary (Atacama's Rush made a BFBC2 Conquest level, `examples/e3d_matrix.rs`'s `atacama_rush`) built under
//!   new names: the id in its partition, description, the menus' list items, every `package.mft` block and
//!   `bundleManifest` line, its folder; the server-browser name in `stage.json` (maps.ini's row and the browser's
//!   `OPFOR - <name> (<mode>)`); the on-screen name in every language of a patched copy of the overlay, French and
//!   Russian with their own letters; `mapexport check` passing its names lines.
//!
//! Reads OPFOR's game copy (beside the suite, or `OPFOR_GAME`); writes only into a temporary folder; skipped without a
//! game copy.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stderr,
    clippy::print_stdout,
    clippy::too_many_lines,
    clippy::many_single_char_names
)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use bc2_formats::fbrb;
use bc2_levelwrite::export::{self, ExportSpec, Game, Identity};
use bc2_levelwrite::mpconvert::LayerSpec;
use bc2_levelwrite::names::{self, LangText, Languages, Taken};
use bc2_levelwrite::{MapProject, Op, Profile};

fn game_dir() -> Option<PathBuf> {
    if let Some(g) = std::env::var_os("OPFOR_GAME") {
        return Some(PathBuf::from(g));
    }
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.ancestors()
        .map(|a| a.join("project-opfor-bfbc2").join("game"))
        .find(|g| g.join("Dist").join("win32").join("package.mft").is_file())
}

/// The canary project: `MP_005GR` with the BFBC2 Conquest target.
fn canary(game: &Game) -> MapProject {
    let base = export::level_partition(game, "mp_005gr").unwrap();
    let mut p = MapProject::new("EX1 names", "mp_005gr", "levels/MP_005GR", &base, Profile::Bfbc2);
    let c = export::choose_mode(game, &p, Profile::Bfbc2, "Conquest", None).unwrap();
    p.push(Op::SetMode { to: Box::new(c) }, "target");
    p
}

fn spec_of(p: &MapProject, id: &str, browser: &str) -> ExportSpec {
    let mut languages = BTreeMap::new();
    languages.insert(
        "french".to_owned(),
        LangText {
            name: "D\u{c9}SERT RENOMM\u{c9}".into(),
            description: String::new(),
        },
    );
    languages.insert(
        "russian".to_owned(),
        LangText {
            name: "\u{41f}\u{423}\u{421}\u{422}\u{42b}\u{41d}\u{42f}".into(),
            description: "\u{41d}\u{41e}\u{412}\u{410}\u{42f}".into(),
        },
    );
    ExportSpec {
        format: export::FORMAT.into(),
        version: export::VERSION,
        level: id.into(),
        folder: String::new(),
        title: browser.into(),
        mode: "conquest".into(),
        layer: LayerSpec {
            donor: p.mode().unwrap().donor.clone(),
            bases: Vec::new(),
            flags: Vec::new(),
            combat_area: String::new(),
        },
        profile: Profile::Bfbc2,
        identity: Some(Identity {
            name: "ATACAMA RENAMED".into(),
            description: "Round EX1: every name of the level".into(),
            picture: String::new(),
            languages,
        }),
    }
}

/// The rules, against the game copy (nothing is built).
#[test]
#[ignore = "game tier: every level's names; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn names_are_checked_against_the_game_copy() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let taken = Taken::read(&game);
    let langs = Languages::read(&game);
    assert!(taken.levels.contains_key("mp_005") && taken.levels.contains_key("sp_01"));
    assert!(
        taken.browser.contains_key("atacama desert"),
        "{:?}",
        taken.browser.keys().take(5).collect::<Vec<_>>()
    );
    assert_eq!(
        langs.names(),
        [
            "english", "french", "german", "italian", "japanese", "polish", "russian", "spanish"
        ]
    );
    assert_eq!(langs.text_of("MP_005_NAME"), Some("ATACAMA DESERT"));
    let p = canary(&game);
    let check = |s: &ExportSpec| {
        names::check(&s.names(), Some(&p.id), "Conquest", Some(&taken), Some(&langs))
            .into_iter()
            .map(|x| x.message)
            .collect::<Vec<_>>()
    };
    let good = spec_of(&p, "MAPEDIT_EX1_RENAMED", "Atacama renamed (EX1)");
    assert_eq!(check(&good), Vec::<String>::new());
    let with = |id: &str, browser: &str| check(&spec_of(&p, id, browser));
    let has = |v: &[String], part: &str| v.iter().any(|m| m.contains(part));
    // a stock level, the black loading screen, a stock text and picture
    assert!(has(&with("MP_005", "x y"), "a level of the game copy"));
    assert!(has(&with("MAPEDIT_NAME_X", "x y"), "black loading screen"));
    let heavy = with("SP_05_A", "x y");
    assert!(
        has(&heavy, "SP_05_A_NAME is already a text of the game (\"HEAVY METAL\")"),
        "{heavy:?}"
    );
    assert!(has(&heavy, "loading picture called sp_05_a"), "{heavy:?}");
    // the server browser: a stock map's name, a long one
    assert!(has(&with("MAPEDIT_X", "Atacama Desert"), "already lists a map called"));
    assert!(has(&with("MAPEDIT_X", &"x".repeat(160)), "cut at 159"));
    // the folder
    let mut s = good.clone();
    s.folder = "elsewhere".into();
    assert!(has(&check(&s), "the level id in lower case (mapedit_ex1_renamed)"));
    // a letter the language has none for (Japanese has no Cyrillic in its table: a Russian default name)
    let mut s = good.clone();
    s.identity.as_mut().unwrap().name = "\u{416}\u{416}".into();
    let v = check(&s);
    assert!(has(&v, "which the game's English texts have no letter for"), "{v:?}");
    // the build refuses what the check refuses, in words
    let tmp = std::env::temp_dir().join(format!("bc2_ex1_refused_{}", std::process::id()));
    let e = export::build(&game, &p, "test", &spec_of(&p, "MAPEDIT_NAME_X", "x y"), &tmp).unwrap_err();
    assert!(e.to_string().contains("black loading screen"), "{e}");
    assert!(!tmp.exists(), "nothing staged");
}

/// The canary exported under new names: every name lands where the game and OPFOR's install read it.
#[test]
#[ignore = "game tier: a whole staged export; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn every_name_lands_in_a_staged_canary() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let p = canary(&game);
    let id = "MAPEDIT_EX1_RENAMED";
    let browser = "Atacama renamed (EX1)";
    let spec = spec_of(&p, id, browser);
    let tmp = std::env::temp_dir().join(format!("bc2_ex1_names_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    // BC2_EX1_STAGE=<folder>: the stage is kept there (OPFOR's install round trip takes it; round EX1's proof)
    let keep = std::env::var_os("BC2_EX1_STAGE").map(PathBuf::from);
    let stage = keep.clone().unwrap_or_else(|| tmp.join("stage"));
    let t = std::time::Instant::now();
    let rec = export::build(&game, &p, "test", &spec, &stage).unwrap();
    println!("built in {:.1} s", t.elapsed().as_secs_f64());
    let low = id.to_ascii_lowercase();
    // the id and its folder: the partition, the description, the menus' list, the registration
    assert_eq!(rec.level, id);
    assert_eq!(rec.description.1, format!("Levels/{id}_description.dbx"));
    assert_eq!(rec.level_list, [format!("{low}.dbx"), format!("{low}_description.dbx")]);
    for b in &rec.mft_blocks {
        assert!(b[0].starts_with(&format!("BundleSource game:levels/{low}/")), "{b:?}");
    }
    assert!(rec.mft_blocks.iter().any(|b| {
        b.iter()
            .any(|l| l == &format!("BundleTarget package:levels/{low}/level-00.fbrb"))
    }));
    for l in &rec.bundle_manifest {
        assert!(l.starts_with(&format!("levels/{low}/")), "{l}");
    }
    let level = fbrb::read_path(stage.join("level-00.fbrb"), &[&format!("Levels/{id}.dbx")]).unwrap();
    let xml = bc2_formats::dbx::to_xml(&level[0]).unwrap();
    assert!(xml.contains(&format!("<field name=\"Name\">Levels/{id}</field>")));
    assert!(xml.contains(&format!("{id}_NAME")) && xml.contains(&format!("{id}_DESC")));
    // the server browser's name: maps.ini's row (OPFOR's install writes [<id>] name=<title>) and the browser's
    assert_eq!(rec.title, browser);
    assert_eq!(rec.mode, "Conquest");
    let row = format!("OPFOR - {} ({})", rec.title, rec.mode);
    assert!(row.len() <= names::BROWSER_ROW);
    // the on-screen names: the default and each language's own
    assert_eq!(
        rec.strings,
        [
            (format!("{id}_NAME"), "ATACAMA RENAMED".to_owned()),
            (format!("{id}_DESC"), "Round EX1: every name of the level".to_owned())
        ]
    );
    assert_eq!(rec.strings_by_language.len(), 2);
    assert_eq!(rec.awards[0].install, format!("Awards/images/loadingScreens/{low}.res"));
    // mapexport check: every line, the names' among them
    let r = export::check(&game, &stage).unwrap();
    assert!(r.ok(), "{:#?}", r.checks.iter().filter(|c| !c.0).collect::<Vec<_>>());
    let lines: Vec<&str> = r.checks.iter().map(|c| c.1.as_str()).collect();
    assert!(
        lines.iter().any(|l| l.starts_with("names: the id MAPEDIT_EX1_RENAMED")),
        "{lines:#?}"
    );
    assert!(lines.iter().any(|l| l.starts_with("names agree:")), "{lines:#?}");
    assert!(
        lines.iter().any(|l| l.contains("(french, russian with own texts)")),
        "{lines:#?}"
    );
    // the checksum crash's causes refused: a bundle registered under another folder, the menus' list missing the
    // level (stage.json tampered with, then put back)
    let rec_path = stage.join("stage.json");
    let good = std::fs::read(&rec_path).unwrap();
    let mut bad = rec.clone();
    bad.bundle_manifest[0] = bad.bundle_manifest[0].replacen(&low, "mapedit_elsewhere", 1);
    bad.level_list.pop();
    std::fs::write(&rec_path, serde_json::to_vec_pretty(&bad).unwrap()).unwrap();
    let r2 = export::check(&game, &stage).unwrap();
    std::fs::write(&rec_path, &good).unwrap();
    let agree = r2.checks.iter().find(|c| c.1.starts_with("names agree")).unwrap();
    assert!(
        !agree.0 && agree.1.contains("bundleManifest line levels/mapedit_elsewhere/"),
        "{agree:?}"
    );
    assert!(agree.1.contains("the menus' level list items are"), "{agree:?}");
    // the overlay an install builds (from the game's, written to the temporary folder): each language's text
    let overlay = game.bundle("game:overlay-00.fbrb");
    let out = tmp.join("overlay-00.fbrb");
    let langs = export::patch_overlay_langs(
        &overlay,
        &rec.strings,
        &rec.strings_by_language,
        &[],
        &tmp.join("w"),
        &out,
    )
    .unwrap();
    assert_eq!(langs.len(), 8);
    let lang_of = Languages::read_bundle(&out);
    let text = |lang: &str, key: &str| {
        let names = [
            format!("Localization/bin/language_{lang}_normal.bin"),
            format!("Localization/bin/language_{lang}_histogram.bin"),
        ];
        let live: Vec<String> = fbrb::entries_path(&out)
            .unwrap()
            .entries
            .iter()
            .map(|e| e.name.clone())
            .filter(|n| names.iter().any(|x| x.eq_ignore_ascii_case(n)))
            .collect();
        let refs: Vec<&str> = live.iter().map(String::as_str).collect();
        let data = fbrb::read_path(&out, &refs).unwrap();
        let (mut normal, mut hist) = (Vec::new(), Vec::new());
        for (n, d) in live.iter().zip(data) {
            if n.to_ascii_lowercase().ends_with("_normal.bin") {
                normal = d;
            } else {
                hist = d;
            }
        }
        let f = bc2_formats::loc::parse(&normal).unwrap();
        let h = bc2_formats::loc::khash_str(key);
        let i = f.entries.iter().position(|e| e.0 == h).unwrap();
        let table = bc2_formats::loc::histogram(&hist).unwrap();
        bc2_formats::loc::decode(f.raw(&normal, i), Some(&table)).unwrap()
    };
    let nk = format!("{id}_NAME");
    assert_eq!(text("english", &nk), "ATACAMA RENAMED");
    assert_eq!(text("german", &nk), "ATACAMA RENAMED");
    assert_eq!(text("french", &nk), "D\u{c9}SERT RENOMM\u{c9}");
    assert_eq!(
        text("russian", &nk),
        "\u{41f}\u{423}\u{421}\u{422}\u{42b}\u{41d}\u{42f}"
    );
    assert_eq!(
        text("russian", &format!("{id}_DESC")),
        "\u{41d}\u{41e}\u{412}\u{410}\u{42f}"
    );
    assert_eq!(
        text("french", &format!("{id}_DESC")),
        "Round EX1: every name of the level"
    );
    assert_eq!(lang_of.text_of(&nk), Some("ATACAMA RENAMED"));
    println!(
        "ex1 names: {id} staged with {} blocks, {} manifest lines; texts in {} languages; {} check lines",
        rec.mft_blocks.len(),
        rec.bundle_manifest.len(),
        langs.len(),
        r.checks.len()
    );
    let _ = std::fs::remove_dir_all(&tmp);
}
