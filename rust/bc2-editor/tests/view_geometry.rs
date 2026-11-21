//! The editor's two 3D views agree on the level's objects (round RV1), on MP_005 and SP_01 headless:
//!
//! - every blueprint the simplified view draws (its Havok collision triangles) that has a visual mesh is drawn by the
//!   game-accurate view too, wherever its blueprint lies: until RV1 the game view drew only `objects/...`, so the
//!   terrain props (`terrains/<lv>/props`: rocks, cliffs, cacti, trenches) and the `vegetation/...` sets were
//!   missing there while the simplified view showed them (the user's "unrecognisable objects": the views did not
//!   show the same things);
//! - for a sample of object kinds (bushes, trees, rocks, cacti, cliffs, buildings), the simplified view's triangles
//!   lie where the game view's mesh lies: the collision box and the visual mesh's box agree within a tenth of the object's size (at least half a metre), and
//!   for vegetation and DICE's render-mesh rocks (whose collision is the render mesh itself) the triangle counts too
//!   within 3%.
//!
//! Skipped (with a note) without the game copy, the prepared levels or Direct3D 11.
#![cfg(windows)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::too_many_lines,
    clippy::panic,
    missing_docs
)]

use std::collections::{BTreeMap, BTreeSet};

use bc2_editor::actions::Action;
use bc2_editor::app::EditorApp;
use bc2_editor::headless::Headless;
use bc2_editor::levels;

type Bounds = ([f32; 3], [f32; 3]);

fn box_of(tris: &[[[f32; 3]; 3]]) -> Bounds {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for p in tris.iter().flatten() {
        for a in 0..3 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
    }
    (lo, hi)
}

fn apart(a: Bounds, b: Bounds) -> f32 {
    (0..3)
        .map(|k| (a.0[k] - b.0[k]).abs().max((a.1[k] - b.1[k]).abs()))
        .fold(0.0, f32::max)
}

/// (blueprint, collision box, collision triangles) of the simplified view; (blueprint, mesh box, mesh triangles)
/// of the game view.
struct Views {
    simplified: BTreeMap<String, (Bounds, usize)>,
    game: BTreeMap<String, (Bounds, usize)>,
}

fn views(h: &mut Headless, lv: &str) -> Option<Views> {
    let mut app = EditorApp::new(None, None).unwrap();
    h.settle(&mut app, 2);
    let gpu = h.gpu.clone();
    if let Err(e) = app.open_level_blocking(&gpu, lv) {
        assert!(
            !crate::level_here(lv),
            "{lv} is in this game copy but does not open: {e}"
        );
        println!("{lv}: skipped, not in this game copy ({e})");
        return None;
    }
    h.settle(&mut app, 2);
    let ctx = h.ctx.clone();
    app.run_action(&gpu, &ctx, Action::ToggleGameView);
    h.settle(&mut app, 2);
    let v = app.view().unwrap();
    // the level opened: the game view not starting is a failure, not a skip (audit AU3 TS-7)
    let Some(g) = v.game.as_ref() else {
        panic!("{lv}: the game-accurate view did not start: {}", v.message);
    };
    let scene = v.scene();
    let col = scene.collision.as_ref().unwrap();
    let mut simplified = BTreeMap::new();
    for (_, bp) in &col.meshes {
        if let Some(Some(t)) = col.blueprints.get(bp)
            && !t.is_empty()
        {
            simplified
                .entry(bp.to_lowercase())
                .or_insert_with(|| (box_of(t), t.len()));
        }
    }
    let placed = bc2_editor::gameview::placed(scene);
    let w = g.world();
    let mut game = BTreeMap::new();
    for (k, o) in w.objects.iter().enumerate() {
        let bp = placed[w.object_of[k]].blueprint.to_lowercase();
        if let Some(b) = o.prepared.bounds {
            let tris = o.prepared.draws.iter().map(|d| d.index_count as usize / 3).sum();
            game.entry(bp).or_insert((b, tris));
        }
    }
    Some(Views { simplified, game })
}

#[test]
#[ignore = "game tier: opens MP_005 and SP_01 in both views; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn both_views_show_the_same_objects() {
    let _gate = crate::gate();
    if levels::find_opfor(None).is_err() {
        println!("skipped: no game copy");
        return;
    }
    let mut h = match Headless::new([800, 500]) {
        Ok(h) => h,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    // (level, blueprint, the collision is the render mesh)
    let samples: &[(&str, &str, bool)] = &[
        ("mp_005", "objects/vegetation/middlegrowth/mebush_01/mebush_01", true),
        ("mp_005", "objects/vegetation/trees/acacia_01/acacia_01", true),
        ("mp_005", "terrains/mp_005/props/mp05_rockformation01", false),
        ("mp_005", "terrains/mp_005/props/mp05_cliff01", false),
        ("mp_005", "terrains/mp_005/props/cactus/cactus_02", false),
        ("mp_005", "objects/buildings/barrack/barrack", false),
        ("sp_01", "objects/vegetation/trees/palmtree_01/palmtree_01", true),
        ("sp_01", "objects/objects/_sp01/rocks/rock_large", true),
        (
            "sp_01",
            "vegetation/jungle/middlegrowth/jungle_bush_1/jungle_bush_1",
            false,
        ),
        ("sp_01", "terrains/sp_01/props/trencharea/trench_stairs", false),
    ];
    let mut checked = 0;
    for lv in ["mp_005", "sp_01"] {
        let Some(v) = views(&mut h, lv) else { continue };
        // the simplified view's blueprints the game view leaves out, by their first folder
        let missing: BTreeSet<&String> = v.simplified.keys().filter(|b| !v.game.contains_key(*b)).collect();
        let outside: Vec<&&String> = missing.iter().filter(|b| !b.starts_with("objects/")).collect();
        println!(
            "{lv}: {} blueprints in the simplified view, {} in the game view, {} of the first not in the second ({} \
             outside objects/: {:?})",
            v.simplified.len(),
            v.game.len(),
            missing.len(),
            outside.len(),
            outside.iter().take(8).collect::<Vec<_>>()
        );
        assert!(
            outside.is_empty(),
            "{lv}: the game view draws the props and vegetation the simplified view shows: {} missing, {:?}",
            outside.len(),
            outside.iter().take(8).collect::<Vec<_>>()
        );
        for &(_, bp, render_mesh) in samples.iter().filter(|s| s.0 == lv) {
            let (sb, st) = v
                .simplified
                .get(bp)
                .unwrap_or_else(|| panic!("{lv}: {bp} not in the simplified view"));
            let (gb, gt) = v
                .game
                .get(bp)
                .unwrap_or_else(|| panic!("{lv}: {bp} not in the game view"));
            let d = apart(*sb, *gb);
            println!("{lv} {bp}: boxes {d:.2} m apart; triangles {st} (simplified) / {gt} (game, LOD 0)");
            // a collision proxy is not the render mesh: a tenth of the object's size, at least half a metre
            let size = (0..3).map(|k| gb.1[k] - gb.0[k]).fold(0f32, f32::max);
            assert!(
                d < (0.1 * size).max(0.5),
                "{lv} {bp}: the views' boxes are {d:.2} m apart: {sb:?} / {gb:?}"
            );
            if render_mesh {
                let r = *st as f64 / *gt as f64;
                assert!((0.97..=1.03).contains(&r), "{lv} {bp}: {st} triangles / {gt}");
            }
        }
        checked += 1;
    }
    if checked == 0 {
        println!("skipped: neither MP_005 nor SP_01 could be opened with the game view");
    }
}
