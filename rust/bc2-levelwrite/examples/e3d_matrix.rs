//! Round E3d's offline proof (`EDITOR_ARCHITECTURE.md` 3.12, "Round E3d"): any base, any era, any mode. Each case
//! makes a project the way the editor does - a new project on a stock level, its target (era and mode) chosen as
//! the first operation of the log, which brings the donor's mode layer in and takes the base's out - then a small
//! scripted edit, exports it into `<stages>\<level>\stage` and runs `mapexport check` on it. Nothing in the game copy
//! is written.
//!
//! `cargo run -p bc2-levelwrite --example e3d_matrix -- <game folder> <projects folder> <stages folder> [case ...]`
//!
//! The canaries (the user's, 2026-11-19):
//! 1. `aurora_ww2` - Operation Aurora (`SP_01`) as a WW2 Conquest level, a WW2 prop (the radar bunker) and a WW2
//!    vehicle spawner (the Kurogane) placed from `guadal_canal`'s own bundle, the bases and flags moved to E3's
//!    places by "Use selection";
//! 2. `rush_cq` - Rush to Conquest: Atacama Desert's Rush (`MP_005GR`) as a BFBC2 Conquest level;
//! 3. `wake_bc2` - Wake Island (`wake_island_s`, WW2) as a BFBC2 level: the same map, the era swapped;
//! 4. `nam_ww2` - a Vietnam level (`NAM_MP_003CQ`) as a WW2 level;
//! 5. `air_superiority` - a BFBC2 level as WW2 Air Superiority: PENDING (the mode is queued in OPFOR's Pacific War
//!    work), refused with its reason;
//! 6. `arica_nam_sdm` - a squad mode: Arica Harbor (`MP_002`, BFBC2 Rush) as a Vietnam Squad Deathmatch level;
//! 7. `guadal_rush` - Guadalcanal (WW2 Conquest) as a BFBC2 Rush level.
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::panic
)]

use std::path::{Path, PathBuf};

use bc2_levelwrite::export::{self, ExportSpec, Game, Identity};
use bc2_levelwrite::mpconvert::LayerSpec;
use bc2_levelwrite::ops::{Moved, Op, Placed};
use bc2_levelwrite::{BaseKind, MapProject, Profile};

struct Case {
    id: &'static str,
    base: &'static str,
    profile: Profile,
    mode: &'static str,
    level: &'static str,
    title: &'static str,
}

const CASES: [Case; 7] = [
    Case {
        id: "aurora_ww2",
        base: "sp_01",
        profile: Profile::Ww2,
        mode: "conquest",
        level: "MAPEDIT_E3D_AURORA_WW2_CQ",
        title: "Operation Aurora 1943",
    },
    Case {
        id: "rush_cq",
        base: "mp_005gr",
        profile: Profile::Bfbc2,
        mode: "conquest",
        level: "MAPEDIT_E3D_ATACAMA_RUSH_CQ",
        title: "Atacama Rush as Conquest",
    },
    Case {
        id: "wake_bc2",
        base: "wake_island_s",
        profile: Profile::Bfbc2,
        mode: "conquest",
        level: "MAPEDIT_E3D_WAKE_BC2_CQ",
        title: "Wake Island 2010",
    },
    Case {
        id: "nam_ww2",
        base: "nam_mp_003cq",
        profile: Profile::Ww2,
        mode: "conquest",
        level: "MAPEDIT_E3D_NAM3_WW2_CQ",
        title: "Vietnam 1943",
    },
    Case {
        id: "air_superiority",
        base: "mp_005",
        profile: Profile::Ww2,
        mode: "airsuperiority",
        level: "MAPEDIT_E3D_ATACAMA_WW2_AS",
        title: "Atacama Air Superiority",
    },
    Case {
        id: "arica_nam_sdm",
        base: "mp_002",
        profile: Profile::Vietnam,
        mode: "teamdeathmatch",
        level: "MAPEDIT_E3D_ARICA_NAM_SDM",
        title: "Arica Harbor 1968 Squad DM",
    },
    Case {
        id: "guadal_rush",
        base: "guadal_canal",
        profile: Profile::Bfbc2,
        mode: "goldrush",
        level: "MAPEDIT_E3D_GUADAL_BC2_RUSH",
        title: "Guadalcanal Rush",
    },
];

/// E3's places on Aurora: the two bases (team 1, team 2) and the flags, between the beach and the inland end.
const AURORA_PLACES: [[f32; 2]; 4] = [[-700.0, 430.0], [-560.0, -20.0], [-660.0, 270.0], [-600.0, 120.0]];

fn kind_of(game: &Game, lv: &str) -> BaseKind {
    let d = export::description_key(game, lv).ok();
    // a campaign level's description names a campaign game mode; the multiplayer levels' are listed by profile
    let mp = bc2_levelwrite::profile::modes(game)
        .unwrap()
        .values()
        .any(|m| m.values().any(|l| l.iter().any(|x| x == lv)));
    let _ = d;
    if mp { BaseKind::Multiplayer } else { BaseKind::Campaign }
}

fn run(game: &Game, c: &Case, projects: &Path, stages: &Path) -> bool {
    println!("==== {} - {} as {} {}", c.id, c.base, c.profile.word(), c.mode);
    let t = std::time::Instant::now();
    let base = export::level_partition(game, c.base).unwrap();
    let base_profile = {
        let targets = game.targets(&format!("levels/{}/level-00.fbrb", c.base));
        let package = game
            .bundle(&format!("package:levels/{}/level-00.fbrb", c.base))
            .is_file();
        Profile::of_stack(c.base, &targets, package)
    };
    let mut p = MapProject::new(
        c.title,
        c.base,
        &format!("levels/{}", c.base.to_ascii_uppercase()),
        &base,
        base_profile,
    );
    p.base.kind = Some(kind_of(game, c.base));
    // 1. the target, as the New project dialog sets it: the era's mode layer in, the base's out
    let choice = match export::choose_mode(game, &p, c.profile, c.mode, None) {
        Ok(x) => x,
        Err(e) if bc2_levelwrite::modelayer::not_offered(c.mode).is_some() => {
            println!("  PENDING canary (not a failure): {e}");
            return true;
        }
        Err(e) => panic!("{e}"),
    };
    println!(
        "  target: {} (fit {:?} -> {:?}, scale {:.2})",
        choice.describe(),
        choice.fit.from,
        choice.fit.to,
        choice.fit.scale
    );
    p.push(Op::SetMode { to: Box::new(choice) }, "New project: era and mode");
    // 2. the scripted edit: one placed object of the level (as the target left it) moved 8 m east
    let (now, _) = export::current(game, &p).unwrap();
    let first = now
        .d
        .of_type(b".ReferenceObjectData")
        .into_iter()
        .filter(|&e| bc2_levelwrite::culltree::slot_of(&now, e) > 0)
        .find_map(|e| {
            let g = String::from_utf8_lossy(now.d.attr(e, b"guid")?).to_ascii_uppercase();
            now.transform(&g).map(|r| (g, r))
        });
    if let Some((g, from)) = first {
        let mut to = from;
        to[3][0] += 8.0;
        p.push(
            Op::Move {
                items: vec![Moved {
                    guid: g,
                    from,
                    to,
                    placement: None,
                }],
            },
            "E3d proof: an object 8 m east",
        );
    }
    let (cur, _) = export::current(game, &p).unwrap();
    let anchors = bc2_levelwrite::modelayer::anchors(&cur);
    // 3. Aurora: WW2 objects from WW2 bundles, and the layer's bases and flags at E3's places ("Use selection")
    if c.id == "aurora_ww2" {
        let gc = game.level_bundles("guadal_canal", "level");
        let bunker =
            export::blueprint_ref(game, &gc, "objects/architecture/beach/radarbunker_t01/radarbunker_t01").unwrap();
        let kurogane = export::blueprint_ref(game, &gc, "Objects/Vehicles/Beach/kurogane/kurogane").unwrap();
        let prop_t = cur.template_for(&bunker).unwrap();
        let spawner_t = cur
            .d
            .of_type(b".VehicleSpawnEntityData")
            .first()
            .map(|&e| String::from_utf8_lossy(cur.d.attr(e, b"guid").unwrap()).to_ascii_uppercase())
            .expect("a vehicle spawner to copy");
        let at = |x: f32, z: f32, y: f32| bc2_levelwrite::rows_at([x, y, z]);
        let n = p.next_n();
        let items = vec![
            Placed {
                guid: bc2_levelwrite::new_guid(&p.id, n, &bunker),
                template: prop_t,
                blueprint: bunker.clone(),
                ref_field: None,
                owned: Vec::new(),
                transform: at(-640.0, 330.0, 6.0),
                fields: Vec::new(),
                placement: None,
            },
            Placed {
                guid: bc2_levelwrite::new_guid(&p.id, n, &kurogane),
                template: spawner_t,
                blueprint: kurogane.clone(),
                ref_field: Some("Vehicle".into()),
                owned: Vec::new(),
                transform: at(-690.0, 410.0, 6.0),
                fields: Vec::new(),
                placement: None,
            },
        ];
        println!("  placed {bunker} and a spawner of {kurogane} (guadal_canal's own bundle)");
        p.push(Op::Place { items }, "E3d proof: WW2 objects from WW2 bundles");
        let cps: Vec<&(String, String, [f32; 3])> = anchors.iter().filter(|a| !a.1.starts_with("Stage")).collect();
        let mut moves: Vec<Moved> = Vec::new();
        for (a, to) in cps.iter().zip(AURORA_PLACES) {
            let m = export::move_anchor(game, &p, &a.0, to).unwrap();
            println!("  use selection: {:?} ({} instances) -> {to:?}", a.1, m.len());
            moves.extend(m);
        }
        p.push(Op::Move { items: moves }, "E3d proof: bases and flags placed");
    } else if let Some(a) = anchors
        .iter()
        .find(|a| a.1.starts_with("Stage 1") || a.1.contains("Base"))
    {
        // the other cases: the first M-COM or base nudged 12 m north by "Use selection"
        let m = export::move_anchor(game, &p, &a.0, [a.2[0], a.2[2] + 12.0]).unwrap();
        println!("  use selection: {:?} ({} instances) 12 m north", a.1, m.len());
        p.push(Op::Move { items: m }, "E3d proof: an anchor moved");
    }
    let path = projects.join(p.file_name());
    p.save(&path).unwrap();
    let spec = ExportSpec {
        format: export::FORMAT.into(),
        version: export::VERSION,
        level: c.level.into(),
        folder: String::new(),
        title: c.title.into(),
        mode: bc2_levelwrite::modelayer::mode_name(c.mode),
        layer: LayerSpec {
            donor: p.mode().unwrap().donor.clone(),
            bases: Vec::new(),
            flags: Vec::new(),
            combat_area: String::new(),
        },
        profile: c.profile,
        identity: Some(Identity {
            description: format!(
                "{} exported as a {} level (round E3d)",
                c.base.to_ascii_uppercase(),
                c.profile.word()
            ),
            picture: String::new(),
            ..Identity::default()
        }),
    };
    std::fs::write(
        export::spec_path_of(&path),
        serde_json::to_string_pretty(&spec).unwrap() + "\n",
    )
    .unwrap();
    let stage = stages.join(c.level.to_ascii_lowercase()).join("stage");
    let rec = match export::build(game, &p, &path.display().to_string(), &spec, &stage) {
        Ok(r) => r,
        Err(e) => {
            println!("  BUILD FAILED: {e}");
            return false;
        }
    };
    for l in &rec.report {
        println!("  | {l}");
    }
    let built = t.elapsed().as_secs_f64();
    let r = export::check(game, &stage).unwrap();
    println!(
        "  built in {built:.1} s, checked in {:.1} s",
        t.elapsed().as_secs_f64() - built
    );
    for (ok, w) in &r.checks {
        println!("  {} {w}", if *ok { "ok    " } else { "FAILED" });
    }
    println!(
        "  {} in {:.0} s: {}",
        c.level,
        t.elapsed().as_secs_f64(),
        if r.ok() { "every check passed" } else { "A CHECK FAILED" }
    );
    r.ok()
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let game = Game::open(&PathBuf::from(&a[1])).expect("game");
    let projects = PathBuf::from(&a[2]);
    let stages = PathBuf::from(&a[3]);
    std::fs::create_dir_all(&projects).unwrap();
    let only: Vec<&str> = a.iter().skip(4).map(String::as_str).collect();
    let mut failed = Vec::new();
    for c in &CASES {
        if !only.is_empty() && !only.contains(&c.id) {
            continue;
        }
        if !run(&game, c, &projects, &stages) {
            failed.push(c.id);
        }
    }
    println!("failed: {failed:?}");
    if !failed.is_empty() {
        std::process::exit(1);
    }
}
