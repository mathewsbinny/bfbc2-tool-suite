//! Round E3c's proofs on real data (`EDITOR_ARCHITECTURE.md` 3.12): each set's announcer voice-over as its stock
//! levels have it, and a WW2-profile export of Operation Aurora (`SP_01`) whose mode layer's triggers resolve to the
//! donor's own (BF1943's) banks. Reads OPFOR's game copy (found beside the suite, or `OPFOR_GAME`), writes only a
//! temporary folder; skipped where there is no game copy.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stderr
)]

use std::path::PathBuf;

use bc2_levelwrite::export::{self, ExportSpec, Game};
use bc2_levelwrite::mpconvert::{Anchor, LayerSpec};
use bc2_levelwrite::{MapProject, Profile};

fn game_dir() -> Option<PathBuf> {
    if let Some(g) = std::env::var_os("OPFOR_GAME") {
        return Some(PathBuf::from(g));
    }
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.ancestors()
        .map(|a| a.join("project-opfor-bfbc2").join("game"))
        .find(|g| g.join("Dist").join("win32").join("package.mft").is_file())
}

/// What each set's stock Conquest levels use for the announcer: BFBC2's, BFBC1's and Vietnam's levels have no bank
/// or trigger set of their own and register their `vo/<lang>` bundles on their commons' voice-over bundles; OPFOR's
/// WW2 levels carry their own banks and trigger sets and an English voice-over bundle of their own.
#[test]
fn each_sets_announcer() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let p = bc2_levelwrite::profile::scan(&game, "conquest").unwrap();
    // the voice-over bundles a donor's blocks target (DICE's MP_SP_005CQ, a BFBC2 level made from the campaign with
    // the DLC's mp2_common, targets mp2_common's as the BFBC1 levels do: the export takes whichever donor is chosen)
    let vo_common = |s: Profile, lv: &str| -> Vec<String> {
        let mut t: Vec<String> = p
            .get(s)
            .unwrap()
            .donor(lv)
            .unwrap()
            .announcer
            .blocks
            .iter()
            .flat_map(|b| b.1.clone())
            .map(|t| t.rsplit('/').nth(2).unwrap().to_ascii_lowercase())
            .collect();
        t.sort();
        t.dedup();
        t
    };
    for (s, lv, want) in [
        (Profile::Bfbc2, "mp_009cq", vec!["mp1_common"]),
        (Profile::Bfbc2, "mp_sp_005cq", vec!["mp2_common", "mp_common"]),
        (Profile::Bfbc1, "bc1_harvest_day_cq", vec!["mp2_common", "mp_common"]),
        (Profile::Vietnam, "nam_mp_002cq", vec!["mp_common", "nam_common"]),
        (Profile::Ww2, "guadal_canal", vec!["mp1_common"]),
    ] {
        assert_eq!(vo_common(s, lv), want, "{lv}");
    }
    for s in Profile::ALL {
        for d in &p.get(s).unwrap().donors {
            let a = &d.announcer;
            assert!(!a.blocks.is_empty(), "{}", d.level);
            // bundleManifest names the vo bundles of some levels only (mp_009cq's and the WW2 levels', not
            // mp_008cq's, nor any Vietnam or BFBC1 level's): the export takes what its donor has
            assert!(
                a.manifest.iter().all(|k| a.blocks.iter().any(|b| b.0 == *k)),
                "{}",
                d.level
            );
            if s == Profile::Ww2 {
                let names: Vec<String> = a.partitions.iter().map(|n| n.to_ascii_lowercase()).collect();
                assert!(
                    names.contains(&"sound/vo/vo_common.dbx".to_owned()),
                    "{}: {names:?}",
                    d.level
                );
                assert!(names.contains(&"sound/vo/vo_en.dbx".to_owned()), "{}", d.level);
                assert_eq!(a.bundles.len(), 1, "{}", d.level);
                assert_eq!(a.blocks.len(), 1, "{}", d.level);
            } else {
                // no line bank of their own (OPFOR's mp_001 holds two Squad Deathmatch trigger sets)
                assert!(
                    !a.partitions
                        .iter()
                        .any(|n| n.to_ascii_lowercase().starts_with("sound/vo/")),
                    "{}: {:?}",
                    d.level,
                    a.partitions
                );
                assert!(a.bundles.is_empty(), "{}", d.level);
                assert_eq!(a.blocks.len(), 7, "{}", d.level);
            }
        }
    }
}

/// `SP_01` exported under the WW2 profile (donor `guadal_canal`): the check passes, among its checks the announcer's
/// (the donor's voice-over registration and bundle, and every announcer partition the level names resolving as on
/// the donor: WW2's voices).
#[test]
fn a_ww2_export_takes_its_donors_announcer() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let live = game.live(&game.loaded("sp_01", "level")).unwrap();
    let base = export::read_found(&[live.get("levels/sp_01.dbx").unwrap()])
        .unwrap()
        .pop()
        .unwrap();
    let p = MapProject::new("E3c test", "sp_01", "levels/SP_01", &base, Profile::Bfbc2);
    let a = |from: &str, x: f32, z: f32| Anchor {
        from: from.into(),
        at: [x, 0.0, z],
    };
    let spec = ExportSpec {
        format: export::FORMAT.into(),
        version: export::VERSION,
        level: "MAPEDIT_E3CTEST_WW2".into(),
        title: "E3c test (WW2)".into(),
        mode: "conquest".into(),
        layer: LayerSpec {
            donor: "guadal_canal".into(),
            bases: vec![a("MP_XX_Team1", -700.0, 430.0), a("MP_XX_Team2", -560.0, -20.0)],
            flags: vec![
                a("MP_XX_American bunker", -660.0, 270.0),
                a("MP_XX_The Hill", -600.0, 120.0),
            ],
            combat_area: "E4C7B1C2-BB9F-481A-B3CC-F67E4E5CB088".into(),
        },
        profile: Profile::Ww2,
        identity: None,
    };
    let stage = std::env::temp_dir().join(format!("bc2_levelwrite_e3c_{}", std::process::id()));
    let rec = export::build(&game, &p, "test", &spec, &stage).unwrap();
    let r = export::check(&game, &stage).unwrap();
    let _ = std::fs::remove_dir_all(&stage);
    assert!(r.ok(), "{:#?}", r.checks.iter().filter(|c| !c.0).collect::<Vec<_>>());
    assert!(
        r.checks
            .iter()
            .any(|(ok, w)| *ok && w.starts_with("announcer: WW2 (Pacific War)'s voices")),
        "{:#?}",
        r.checks
    );
    assert!(rec.files.iter().any(|f| f.path == "vo/en-00.fbrb"));
    assert!(
        rec.announcer
            .iter()
            .any(|n| n.eq_ignore_ascii_case("sound/vo/vo_common.dbx"))
    );
    assert_eq!(
        rec.mft_blocks.last().unwrap(),
        &vec![
            "BundleSource game:levels/mapedit_e3ctest_ww2/vo/en-00.fbrb".to_owned(),
            "BundleTarget package:levels/mp1_common/vo/en-00.fbrb".to_owned(),
        ]
    );
}
