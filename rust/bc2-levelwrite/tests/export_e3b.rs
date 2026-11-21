//! Round E3b's proofs on real data (`EDITOR_ARCHITECTURE.md` 3.12): the campaign checklist, a Vietnam-profile export
//! of Operation Aurora (`SP_01`) with its own name and picture, and the two shared bundles an install patches (the
//! localization overlay, the on-demand awards). Reads OPFOR's game copy (found beside the suite, or `OPFOR_GAME`),
//! writes only temporary folders; skipped where there is no game copy.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stderr,
    clippy::too_many_lines
)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use bc2_formats::fbrb;
use bc2_levelwrite::export::{self, ExportSpec, Game, Identity};
use bc2_levelwrite::mpconvert::{self, Anchor, CAMPAIGN_VEHICLES, Converted, LayerSpec, Strip};
use bc2_levelwrite::{LevelWriter, MapProject, Op, Profile};

fn game_dir() -> Option<PathBuf> {
    if let Some(g) = std::env::var_os("OPFOR_GAME") {
        return Some(PathBuf::from(g));
    }
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.ancestors()
        .map(|a| a.join("project-opfor-bfbc2").join("game"))
        .find(|g| g.join("Dist").join("win32").join("package.mft").is_file())
}

fn sp01(game: &Game) -> Vec<u8> {
    let live = game.live(&game.loaded("sp_01", "level")).unwrap();
    export::read_found(&[live.get("levels/sp_01.dbx").unwrap()])
        .unwrap()
        .pop()
        .unwrap()
}

fn types(w: &LevelWriter) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for e in w.d.instances() {
        let t = String::from_utf8_lossy(w.d.attr(e, b"type").unwrap_or_default()).into_owned();
        *out.entry(t.rsplit('.').next().unwrap().to_owned()).or_default() += 1;
    }
    out
}

/// The default choice removes what E3 removed; keeping the campaign vehicles keeps `SP_01`'s 53 vehicle spawns and
/// nothing else; the choice is an undoable entry of the project's log.
#[test]
fn the_campaign_checklist() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let base = sp01(&game);
    let area: HashSet<String> = ["E4C7B1C2-BB9F-481A-B3CC-F67E4E5CB088".to_owned()].into();
    let run = |choice: &Strip| {
        let mut w = LevelWriter::parse(&base).unwrap();
        let mut c = Converted::default();
        mpconvert::strip(&mut w, &area, choice, &mut c).unwrap();
        (types(&w), w.guids().into_iter().collect::<HashSet<String>>(), c)
    };
    let (dt, dg, dc) = run(&Strip::default());
    assert_eq!(dc.removed.iter().map(|x| x.1).sum::<usize>(), 1785, "E3's count");
    let (vt, vg, _) = run(&Strip::default().with_vehicles(true));
    let extra: Vec<&String> = vg.difference(&dg).collect();
    assert_eq!(extra.len(), 53);
    assert!(dg.is_subset(&vg));
    let mut diff: Vec<(String, usize)> = vt
        .iter()
        .filter(|(t, n)| dt.get(*t) != Some(n))
        .map(|(t, n)| (t.clone(), n - dt.get(t).copied().unwrap_or(0)))
        .collect();
    diff.sort();
    assert_eq!(diff, [(CAMPAIGN_VEHICLES.to_owned(), 53)]);
    // the log: chosen, undone, redone
    let mut p = MapProject::new("E3b", "sp_01", "levels/SP_01", &base, Profile::Bfbc2);
    assert!(p.strip_chosen().is_none());
    let to = Strip::default().with_vehicles(true);
    p.push(
        Op::SetStrip {
            from: Strip::default(),
            to: to.clone(),
        },
        "",
    );
    assert_eq!(p.strip(), to);
    assert_eq!(p.operations[0].name(), "Campaign content: campaign vehicles kept");
    p.undo();
    assert_eq!(p.strip(), Strip::default());
    p.redo();
    assert_eq!(p.strip(), to);
    let back = MapProject::from_json(&p.to_json().unwrap()).unwrap();
    assert_eq!(back.strip(), to);
}

/// `SP_01` exported under the Vietnam profile with its own name and a picture: the check passes (the profile's
/// commons, kits and fields, the names, the pictures); the overlay and awards patches built from the game copy's
/// bundles hold the texts and pictures, and every other entry and text is as it was.
#[test]
#[ignore = "game tier: a whole Vietnam export; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn a_vietnam_export_with_its_own_name_and_picture() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let base = sp01(&game);
    let tmp = std::env::temp_dir().join(format!("bc2_levelwrite_e3b_{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    // a picture of our own (no game content): a gradient
    let (pw, ph) = (640usize, 400usize);
    let mut px = Vec::new();
    for y in 0..ph {
        for x in 0..pw {
            px.extend([(x * 255 / pw) as u8, (y * 255 / ph) as u8, ((x + y) % 256) as u8]);
        }
    }
    let png = bc2_levelwrite::awards::png_bytes(&bc2_levelwrite::awards::Rgb { w: pw, h: ph, px }).unwrap();
    let picture = tmp.join("picture.png");
    std::fs::write(&picture, png).unwrap();
    let p = MapProject::new("E3b test", "sp_01", "levels/SP_01", &base, Profile::Bfbc2);
    let a = |from: &str, x: f32, z: f32| Anchor {
        from: from.into(),
        at: [x, 0.0, z],
    };
    let spec = ExportSpec {
        format: export::FORMAT.into(),
        version: export::VERSION,
        level: "MAPEDIT_E3BTEST_NAM".into(),
        folder: String::new(),
        title: "E3b test (Vietnam)".into(),
        mode: "conquest".into(),
        layer: LayerSpec {
            donor: "nam_mp_002cq".into(),
            bases: vec![a("US Base", -700.0, 430.0), a("VC Base", -560.0, -20.0)],
            flags: vec![a("A: Village", -660.0, 270.0), a("B: Camp", -600.0, 120.0)],
            combat_area: "E4C7B1C2-BB9F-481A-B3CC-F67E4E5CB088".into(),
        },
        profile: Profile::Vietnam,
        identity: Some(Identity {
            description: "A test of the export's own name.".into(),
            picture: picture.display().to_string(),
            ..Identity::default()
        }),
    };
    let stage = tmp.join("stage");
    let rec = export::build(&game, &p, "test", &spec, &stage).unwrap();
    let r = export::check(&game, &stage).unwrap();
    assert!(r.ok(), "{:#?}", r.checks.iter().filter(|c| !c.0).collect::<Vec<_>>());
    assert_eq!(rec.profile, Profile::Vietnam);
    assert_eq!(
        rec.strings[0],
        ("MAPEDIT_E3BTEST_NAM_NAME".into(), "E3b test (Vietnam)".into())
    );
    assert!(rec.mft_blocks[0].iter().any(|l| l.contains("nam_common")));
    for l in rec.report.iter().filter(|l| l.starts_with("picture ")) {
        let db: f64 = l
            .rsplit(", ")
            .next()
            .unwrap()
            .split(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(db > 30.0, "{l}");
    }
    // the overlay: the texts in every language, every other text as it was
    let overlay = game.bundle("game:overlay-00.fbrb");
    let out = tmp.join("overlay-00.fbrb");
    let langs = export::patch_overlay(&overlay, &rec.strings, &[], &tmp.join("w"), &out).unwrap();
    assert!(langs.contains(&"english".to_owned()), "{langs:?}");
    let (h0, d0) = fbrb::extract_path_uncached(&overlay).unwrap();
    let (h1, d1) = fbrb::extract_path_uncached(&out).unwrap();
    assert_eq!(h0.entries.len(), h1.entries.len());
    let texts = |data: &[u8]| -> HashMap<u32, Vec<u8>> {
        let f = bc2_formats::loc::parse(data).unwrap();
        f.entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.0, f.raw(data, i).to_vec()))
            .collect()
    };
    let mut checked = 0;
    for ((e0, a0), (e1, a1)) in h0.entries.iter().zip(&d0).zip(h1.entries.iter().zip(&d1)) {
        assert_eq!(e0.name, e1.name);
        let low = e0.name.to_ascii_lowercase();
        if low.starts_with("localization/bin/language_") && low.ends_with("_normal.bin") {
            let (t0, t1) = (texts(a0), texts(a1));
            assert_eq!(t1.len(), t0.len() + 2, "{}", e0.name);
            for (h, t) in &t0 {
                assert_eq!(t1.get(h), Some(t), "{}: a text changed", e0.name);
            }
            for (k, t) in &rec.strings {
                assert_eq!(t1.get(&bc2_formats::loc::khash_str(k)), Some(&t.as_bytes().to_vec()));
            }
            checked += 1;
        } else {
            assert_eq!(a0, a1, "{}", e0.name);
        }
    }
    assert_eq!(checked, langs.len());
    // the awards: the pictures added, every stock and WW2 entry as it was
    let awards = game.bundle("package:async/ondemand_awards-00.fbrb");
    let pics: Vec<(String, Vec<u8>)> = rec
        .awards
        .iter()
        .map(|f| (f.install.clone(), std::fs::read(stage.join(&f.path)).unwrap()))
        .collect();
    let out = tmp.join("ondemand_awards-00.fbrb");
    export::patch_awards(&awards, &pics, &[], &tmp.join("w"), &out).unwrap();
    let (h0, d0) = fbrb::extract_path_uncached(&awards).unwrap();
    let (h1, d1) = fbrb::extract_path_uncached(&out).unwrap();
    assert_eq!(h1.entries.len(), h0.entries.len() + 2);
    for (k, (e0, a0)) in h0.entries.iter().zip(&d0).enumerate() {
        assert_eq!(e0.name, h1.entries[k].name);
        assert_eq!(a0, &d1[k], "{}", e0.name);
    }
    for (name, bytes) in &pics {
        let k = h1.entries.iter().position(|e| e.name == *name).unwrap();
        assert_eq!(&d1[k], bytes);
        assert!(bc2_levelwrite::awards::decode(bc2_levelwrite::awards::unwrap(&d1[k])).is_some());
    }
    let _ = std::fs::remove_dir_all(&tmp);
}
