//! Round VW2 in the editor, headless on the game copy's prepared levels:
//!
//! - the mode view layer: on `MP_009GR` (Rush) the filters hide what the level's mode layer data says
//!   (`modelayer::layer_view` of the level's partition: the layer, each Rush stage's hidden instances), in the
//!   simplified view, the outliner and the game-accurate view (its placed objects drop by the hidden objects that
//!   draw a mesh);
//! - the weather: every state of the level's weather system listed, each one's look in the game-accurate view
//!   measured against its field values, its tint in the simplified view, the level's own (spawn-switch) state chosen
//!   at open.
//!
//! `BC2_EDITOR_SHOTS=<folder>` saves the views in Dark and Light. Skipped (with a note) without the game copy, the
//! prepared levels, a graphics adapter or Direct3D 11.
#![cfg(windows)]
#![allow(
    clippy::unwrap_used,
    clippy::many_single_char_names,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::doc_markdown,
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    clippy::panic,
    clippy::needless_range_loop,
    missing_docs
)]

use std::collections::HashSet;
use std::path::PathBuf;

use bc2_editor::actions::Action;
use bc2_editor::app::EditorApp;
use bc2_editor::camera::{Camera, Projection};
use bc2_editor::headless::Headless;
use bc2_editor::levels;
use bc2_editor::modeview::LayerFilter;
use bc2_editor::theme::ThemeId;
use bc2_levelwrite::modelayer;
use bc2_levelwrite::write::LevelWriter;

fn shots() -> Option<PathBuf> {
    std::env::var_os("BC2_EDITOR_SHOTS").map(PathBuf::from)
}

fn open(h: &mut Headless, lv: &str) -> Option<EditorApp> {
    let mut app = EditorApp::new(None, None).unwrap();
    h.settle(&mut app, 2);
    let gpu = h.gpu.clone();
    if let Err(e) = app.open_level_blocking(&gpu, lv) {
        println!("{lv}: skipped, {e}");
        return None;
    }
    h.settle(&mut app, 2);
    Some(app)
}

fn both_themes(h: &mut Headless, app: &mut EditorApp, name: &str) {
    let Some(d) = shots() else { return };
    std::fs::create_dir_all(&d).unwrap();
    for (t, s) in [(ThemeId::Dark, "dark"), (ThemeId::Light, "light")] {
        app.theme = t;
        h.settle(app, 3);
        h.save(&d.join(format!("{name}_{s}.png"))).unwrap();
    }
    app.theme = ThemeId::Dark;
    h.settle(app, 2);
}

/// The mode view layer on Atacama-style Rush (`MP_009GR`): each filter's and each stage's hidden objects are the
/// level's layer data's, in the outliner, the simplified view and the game-accurate view.
// spec: GAME-RUSH-9
#[test]
fn mode_view_layer_hides_by_mode_and_rush_stage() {
    let _gate = crate::gate();
    if levels::find_opfor(None).is_err() {
        println!("skipped: no game copy");
        return;
    }
    let mut h = match Headless::new([1600, 950]) {
        Ok(h) => h,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    let Some(mut app) = open(&mut h, "mp_009gr") else {
        return;
    };
    let gpu = h.gpu.clone();
    let ctx = h.ctx.clone();
    let (want, guids, total) = {
        let v = app.view_mut().unwrap();
        v.labels = false;
        let s = v.scene();
        let w = LevelWriter::parse(s.file(0).unwrap().1).unwrap();
        let guids: Vec<(usize, String)> = s
            .entities
            .iter()
            .map(|e| (e.index, e.guid.to_ascii_uppercase()))
            .collect();
        (modelayer::layer_view(&w), guids, s.entities.len())
    };
    let in_layer = guids.iter().filter(|(_, g)| want.layer.contains(g)).count();
    println!(
        "mp_009gr: {total} entities, the layer {} instances ({in_layer} of them entities of the scene), {} Rush stages",
        want.layer.len(),
        want.stages.len()
    );
    assert!(want.stages.len() >= 3, "MP_009GR is Rush");
    // a camera over the level's middle: the whole layer in view
    {
        let v = app.view_mut().unwrap();
        let pts: Vec<[f64; 3]> = v
            .scene()
            .entities
            .iter()
            .filter_map(bc2_scene::scene::Entity::position)
            .collect();
        let c = pts
            .iter()
            .fold([0.0; 3], |a, p| [a[0] + p[0], a[1] + p[1], a[2] + p[2]]);
        let n = pts.len().max(1) as f64;
        v.viewports.get_mut(&Projection::Perspective).unwrap().camera = Camera {
            target: [c[0] / n, c[1] / n, c[2] / n],
            yaw: 0.6,
            pitch: 1.1,
            distance: 900.0,
            fov: 60.0,
            projection: Projection::Perspective,
        };
    }
    h.settle(&mut app, 3);
    both_themes(&mut h, &mut app, "vw2_mode_all");
    let before = h.image().unwrap();
    let outliner_rows = |app: &EditorApp| app.view().unwrap().outliner.entries().count();
    let all_rows = outliner_rows(&app);
    // the layer only: everything outside the layer hidden
    app.run_action(&gpu, &ctx, Action::SetLayerFilter(LayerFilter::LayerOnly));
    h.settle(&mut app, 3);
    let hidden: HashSet<usize> = app.view().unwrap().mode_view.hidden.clone();
    assert_eq!(hidden.len(), total - in_layer, "the layer only: the rest hidden");
    assert!(guids.iter().all(|(k, g)| hidden.contains(k) != want.layer.contains(g)));
    println!(
        "layer only: {} hidden, outliner {} -> {} rows",
        hidden.len(),
        all_rows,
        outliner_rows(&app)
    );
    assert!(outliner_rows(&app) < all_rows);
    both_themes(&mut h, &mut app, "vw2_mode_layer_only");
    let layer_img = h.image().unwrap();
    let changed = |a: &[u8], b: &[u8]| a.chunks(4).zip(b.chunks(4)).filter(|(x, y)| x != y).count();
    assert!(changed(&before, &layer_img) > 1000, "the view changed");
    // without the layer: exactly the layer hidden
    app.run_action(&gpu, &ctx, Action::SetLayerFilter(LayerFilter::WithoutLayer));
    h.settle(&mut app, 3);
    assert_eq!(app.view().unwrap().mode_view.hidden.len(), in_layer);
    both_themes(&mut h, &mut app, "vw2_mode_without_layer");
    // each Rush stage: the stage's hidden instances, its M-COMs shown, the others' hidden
    app.run_action(&gpu, &ctx, Action::SetLayerFilter(LayerFilter::All));
    for (k, st) in want.stages.iter().enumerate() {
        app.run_action(&gpu, &ctx, Action::SetRushStage(Some(k)));
        h.settle(&mut app, 2);
        let v = app.view().unwrap();
        let got: HashSet<String> = v.mode_view.hidden.iter().map(|&i| guids[i].1.clone()).collect();
        let expect: HashSet<String> = guids
            .iter()
            .filter(|(_, g)| st.hidden.contains(g))
            .map(|(_, g)| g.clone())
            .collect();
        println!(
            "stage {}: {} M-COMs, {} bases, {} combat areas in play; {} hidden ({} of the layer data's {} are entities)",
            k + 1,
            st.mcoms.len(),
            st.bases.len(),
            st.combat_areas.len(),
            got.len(),
            expect.len(),
            st.hidden.len()
        );
        assert_eq!(got, expect, "stage {}", k + 1);
        for m in &st.mcoms {
            assert!(!got.contains(m), "stage {}: its own M-COM shown", k + 1);
        }
        if k == 0 {
            both_themes(&mut h, &mut app, "vw2_mode_rush_stage1");
        }
    }
    // the stepper walks the stages, then every stage again
    app.run_action(&gpu, &ctx, Action::SetRushStage(None));
    for k in 0..want.stages.len() {
        app.run_action(&gpu, &ctx, Action::StepRushStage(true));
        assert_eq!(app.view().unwrap().mode_view.stage, Some(k));
    }
    app.run_action(&gpu, &ctx, Action::StepRushStage(true));
    assert_eq!(app.view().unwrap().mode_view.stage, None);
    assert!(app.view().unwrap().mode_view.hidden.is_empty());
    // the game-accurate view leaves the hidden objects out
    app.run_action(&gpu, &ctx, Action::ToggleGameView);
    if app.view().unwrap().game.is_none() {
        println!("game view skipped: {}", app.view().unwrap().message);
        return;
    }
    h.settle(&mut app, 3);
    let objects_all = app.view().unwrap().game.as_ref().unwrap().object_count();
    app.run_action(&gpu, &ctx, Action::SetRushStage(Some(0)));
    h.settle(&mut app, 3);
    let v = app.view().unwrap();
    let g = v.game.as_ref().unwrap();
    let drawn_hidden = v
        .mode_view
        .hidden
        .iter()
        .filter(|&&k| {
            let e = &v.scene().entities[k];
            e.blueprint.is_some() && e.matrix.is_some()
        })
        .count();
    println!(
        "game view: {objects_all} objects drawn with every stage, {} with stage 1 ({} hidden placed objects)",
        g.object_count(),
        drawn_hidden
    );
    assert!(
        g.object_count() < objects_all,
        "stage 1 leaves the other stages' M-COMs out"
    );
    assert!(objects_all - g.object_count() <= drawn_hidden);
    both_themes(&mut h, &mut app, "vw2_mode_rush_stage1_game");
}

/// The mean colour (0..1) of the 3D view's middle (a box inside its rectangle, clear of the overlays' bars).
fn view_mean(h: &mut Headless, app: &EditorApp, width: usize) -> [f64; 3] {
    let img = h.image().unwrap();
    let r = app.view().unwrap().viewports[&Projection::Perspective].rect.unwrap();
    let (x0, x1) = (
        (r.left() + r.width() * 0.15) as usize,
        (r.right() - r.width() * 0.15) as usize,
    );
    let (y0, y1) = (
        (r.top() + r.height() * 0.25) as usize,
        (r.bottom() - r.height() * 0.15) as usize,
    );
    let mut s = [0.0f64; 3];
    let mut n = 0.0;
    for y in y0..y1 {
        for x in x0..x1 {
            let p = (y * width + x) * 4;
            for (c, v) in s.iter_mut().enumerate() {
                *v += f64::from(img[p + c]) / 255.0;
            }
            n += 1.0;
        }
    }
    s.map(|v| v / n)
}

fn lum(c: [f64; 3]) -> f64 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

/// Pearson's correlation of two series.
fn pearson(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len() as f64;
    let (ma, mb) = (a.iter().sum::<f64>() / n, b.iter().sum::<f64>() / n);
    let cov: f64 = a.iter().zip(b).map(|(x, y)| (x - ma) * (y - mb)).sum();
    let va: f64 = a.iter().map(|x| (x - ma).powi(2)).sum();
    let vb: f64 = b.iter().map(|y| (y - mb).powi(2)).sum();
    cov / (va * vb).sqrt()
}

/// Spearman's rank correlation of two series (ties ranked in order).
fn rank_corr(a: &[f64], b: &[f64]) -> f64 {
    let rank = |v: &[f64]| -> Vec<f64> {
        let mut idx: Vec<usize> = (0..v.len()).collect();
        idx.sort_by(|&i, &j| v[i].total_cmp(&v[j]));
        let mut r = vec![0.0; v.len()];
        for (k, &i) in idx.iter().enumerate() {
            r[i] = k as f64;
        }
        r
    };
    let (ra, rb) = (rank(a), rank(b));
    let m = (a.len() as f64 - 1.0) / 2.0;
    let cov: f64 = ra.iter().zip(&rb).map(|(x, y)| (x - m) * (y - m)).sum();
    let var: f64 = ra.iter().map(|x| (x - m).powi(2)).sum();
    cov / var
}

/// The weather picker on Laguna Presa's Squad Rush (`MP_002SR`, MP_002's weather system): its 7 states listed (the
/// Onslaught Package's night among them), the level's own state `midday` (spawn switch and ActiveState), each state
/// drawn in the game-accurate view; the frames' brightness ranks as the states' looks from their values rank, the night is
/// dark and blue; the simplified view's tint follows.
// spec: RNDR-WTHR-6, RNDR-WTHR-7
#[test]
fn weather_states_in_both_views() {
    let _gate = crate::gate();
    if levels::find_opfor(None).is_err() {
        println!("skipped: no game copy");
        return;
    }
    let mut h = match Headless::new([1600, 950]) {
        Ok(h) => h,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    let Some(mut app) = open(&mut h, "mp_002sr") else {
        return;
    };
    let gpu = h.gpu.clone();
    let ctx = h.ctx.clone();
    {
        let v = app.view_mut().unwrap();
        v.labels = false;
        let pts: Vec<[f64; 3]> = v
            .scene()
            .entities
            .iter()
            .filter(|e| e.category == "capture point")
            .filter_map(bc2_scene::scene::Entity::position)
            .collect();
        let c = pts
            .iter()
            .fold([0.0; 3], |a, p| [a[0] + p[0], a[1] + p[1], a[2] + p[2]]);
        let n = pts.len().max(1) as f64;
        v.viewports.get_mut(&Projection::Perspective).unwrap().camera = Camera {
            target: [c[0] / n, c[1] / n, c[2] / n],
            yaw: 0.8,
            pitch: 0.35,
            distance: 220.0,
            fov: 60.0,
            projection: Projection::Perspective,
        };
    }
    h.settle(&mut app, 3);
    both_themes(&mut h, &mut app, "vw2_weather_simplified_before");
    let plain = view_mean(&mut h, &app, 1600);
    app.run_action(&gpu, &ctx, Action::LevelWeather);
    let (n, own, night, rows) = {
        let v = app.view().unwrap();
        let w = v
            .weather
            .as_ref()
            .unwrap_or_else(|| panic!("the weather read: {}", v.message));
        for k in 0..w.rows.len() {
            println!("  {}", w.label(k));
        }
        let night = w.rows.iter().position(|r| r.name == "ons_mp_002_night");
        (w.rows.len(), w.level.starts_in(), night, w.rows.clone())
    };
    assert_eq!(n, 7, "MP_002's weather system has 7 states");
    let night = night.expect("ons_mp_002_night is one of the system's states");
    let midday = rows.iter().position(|r| r.name == "midday").unwrap();
    assert_eq!(own, midday, "MP_002SR starts in midday");
    app.run_action(&gpu, &ctx, Action::SetWeatherState(midday));
    // the simplified view: tinted by each state's look
    h.settle(&mut app, 3);
    let day_tinted = view_mean(&mut h, &app, 1600);
    app.run_action(&gpu, &ctx, Action::SetWeatherState(night));
    h.settle(&mut app, 3);
    both_themes(&mut h, &mut app, "vw2_weather_simplified_night");
    let night_tinted = view_mean(&mut h, &app, 1600);
    let t = rows[night].tint.unwrap();
    println!(
        "simplified view: plain {plain:.3?}, midday {day_tinted:.3?}, night {night_tinted:.3?} (tint {t:.3?}, midday's \
         {:.3?})",
        rows[midday].tint.unwrap()
    );
    assert!(
        lum(night_tinted) < 0.6 * lum(day_tinted),
        "the night darkens the simplified view"
    );
    assert!(night_tinted[2] > night_tinted[0], "and turns it blue");
    // the game-accurate view: every state drawn, measured
    app.run_action(&gpu, &ctx, Action::ToggleGameView);
    if app.view().unwrap().game.is_none() {
        println!("game view skipped: {}", app.view().unwrap().message);
        return;
    }
    h.settle(&mut app, 4);
    let mut measured = Vec::new();
    let mut predicted = Vec::new();
    // the colour's blueness (blue over red): the frame's against the state's light (key + sky, before exposure)
    let mut blue_measured = Vec::new();
    let mut blue_predicted = Vec::new();
    for k in 0..n {
        app.run_action(&gpu, &ctx, Action::SetWeatherState(k));
        h.settle(&mut app, 4);
        let g = app.view().unwrap().game.as_ref().unwrap();
        assert!(g.error.is_none(), "{:?}", g.error);
        assert_eq!(g.world().state_shown, Some(k));
        let m = view_mean(&mut h, &app, 1600);
        let l = rows[k].look.unwrap();
        let lg = l.ground.map(f64::from);
        println!(
            "state {k} {:<22} frame mean ({:.3}, {:.3}, {:.3}) lum {:.3}; look ground ({:.3}, {:.3}, {:.3}) lum {:.3}, \
             exposure {:.2}",
            rows[k].name,
            m[0],
            m[1],
            m[2],
            lum(m),
            lg[0],
            lg[1],
            lg[2],
            lum(lg),
            l.exposure
        );
        measured.push(lum(m));
        predicted.push(lum(lg));
        blue_measured.push(m[2] / m[0].max(1e-3));
        blue_predicted.push(f64::from(l.hdr[2]) / f64::from(l.hdr[0]).max(1e-3));
        if k == night || k == midday {
            both_themes(&mut h, &mut app, &format!("vw2_weather_game_{}", rows[k].name));
        }
        if k == night {
            assert!(m[2] > m[0], "the night is blue in the game-accurate view: {m:?}");
        }
    }
    let r = pearson(&measured, &predicted);
    let rho = rank_corr(&blue_measured, &blue_predicted);
    println!(
        "the frames' brightness against the looks': Pearson {r:.2}; their blueness against the light's: Spearman \
         {rho:.2} ({blue_measured:.2?} / {blue_predicted:.2?})"
    );
    assert!(r > 0.8, "the frames' brightness follows the states' values: {r}");
    assert!(rho > 0.6, "the frames' colour follows the states' light: {rho}");
    assert!(
        measured[night] < 0.5 * measured[midday],
        "the night frame is darker than midday's: {} / {}",
        measured[night],
        measured[midday]
    );
    // the level's own state, back
    app.run_action(&gpu, &ctx, Action::LevelWeather);
    assert_eq!(
        app.view().unwrap().game.as_ref().unwrap().world().state_shown,
        Some(own)
    );
}

/// The game-accurate view opens on the state players see: MP_005's spawn switch (`day_1`), not its settings'
/// `ActiveState` (`default`, dark).
// spec: RNDR-WTHR-6
#[test]
fn game_view_opens_on_the_spawn_switch() {
    let _gate = crate::gate();
    if levels::find_opfor(None).is_err() {
        println!("skipped: no game copy");
        return;
    }
    let mut h = match Headless::new([1280, 720]) {
        Ok(h) => h,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    let Some(mut app) = open(&mut h, "mp_005") else { return };
    let gpu = h.gpu.clone();
    let ctx = h.ctx.clone();
    app.run_action(&gpu, &ctx, Action::ToggleGameView);
    let Some(g) = app.view().unwrap().game.as_ref() else {
        println!("skipped: {}", app.view().unwrap().message);
        return;
    };
    let lw = g.world().level_weather.clone().unwrap();
    println!(
        "mp_005: ActiveState {} {}, spawn switch {:?} -> drawn {:?}; {:?}",
        lw.active,
        lw.name(lw.active),
        lw.spawn,
        g.world().state_shown,
        g.notes.iter().find(|n| n.contains("spawn-time"))
    );
    assert_eq!((lw.active, lw.spawn), (0, Some(2)));
    assert_eq!(g.world().state_shown, Some(2));
    assert!(
        g.world().weather.state.ends_with("day_1"),
        "{}",
        g.world().weather.state
    );
}
