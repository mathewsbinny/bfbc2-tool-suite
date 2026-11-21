//! The simplified view draws the objects' render meshes (round SV1), on MP_005 and SP_01 headless:
//!
//! - for sampled kinds (bush, acacia, palm, rocks, cliff, buildings) what the view draws is the render mesh the
//!   game-accurate view draws (`bc2_render::world::World`, the same reading): the same indices, less only the blended
//!   parts (glass, decals) and, on a destructible object, the parts its intact state hides;
//! - a leaf-card blueprint renders with holes: its alpha-tested leaves cut by their texture, fewer pixels than the
//!   same cards drawn solid (its collision is those cards);
//! - the invisible walls are hidden at the start, outlined or solid when asked;
//! - each "Show as" (the view's and an object's) draws what it says (index counts: render mesh against collision),
//!   an override survives the editor's settings saved and read again and "Reset view overrides" clears it, the LOD
//!   picker draws the coarser levels, a destructible object's destroyed state other parts;
//! - picking at sampled objects' centres selects what it selected before (collision drawn), the differences listed;
//! - the view modes still draw (lit, unlit, wireframe, collision, navmesh, cull tree).
//!
//! Frame times (the view drawn and waited for, the camera turning) are printed for collision and render meshes.
//! `BC2_EDITOR_SHOTS=<folder>` saves the before / after pictures (MP_005's rock formation and bushes, SP_01's palms
//! and invisible walls, an object shown as visual, collision and both) in Dark and Light.
//!
//! Skipped (with a note) without the game copy, the prepared levels or a graphics adapter.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::doc_markdown,
    clippy::too_many_lines,
    clippy::panic,
    clippy::many_single_char_names,
    clippy::similar_names,
    missing_docs
)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use bc2_editor::actions::Action;
use bc2_editor::app::{EditorApp, Saved};
use bc2_editor::camera::{Camera, Projection};
use bc2_editor::display::{Destruction, InvisibleAs, ShowAs};
use bc2_editor::headless::Headless;
use bc2_editor::levels;
use bc2_editor::render::{self, Drawn, Renderer, SceneGpu};
use bc2_editor::theme::{Palette, ThemeId, Tokens};
use bc2_editor::workspace::ViewMode;
use bc2_render::simple::{Shape, State, part_shown};
use bc2_scene::scene::Scene;

fn shots() -> Option<PathBuf> {
    std::env::var_os("BC2_EDITOR_SHOTS").map(PathBuf::from)
}

fn open(h: &mut Headless, lv: &str, saved: Option<Saved>) -> Option<EditorApp> {
    let mut app = EditorApp::new(None, saved).unwrap();
    h.settle(&mut app, 2);
    let gpu = h.gpu.clone();
    if let Err(e) = app.open_level_blocking(&gpu, lv) {
        println!("{lv}: skipped, {e}");
        return None;
    }
    let v = app.view_mut().unwrap();
    v.labels = false;
    if v.gpu_scene().visuals().is_none() {
        println!("{lv}: skipped, the render meshes were not read: {:?}", v.notes.last());
        return None;
    }
    h.settle(&mut app, 2);
    Some(app)
}

fn palette(app: &EditorApp) -> Palette {
    Palette::new(Tokens::embedded().unwrap(), app.theme)
}

/// The first entity whose blueprint ends with `bp`.
fn entity(scene: &Scene, bp: &str) -> usize {
    scene
        .entities
        .iter()
        .find(|e| e.blueprint.as_deref().is_some_and(|b| b.ends_with(bp)) && e.matrix.is_some())
        .unwrap_or_else(|| panic!("no {bp}"))
        .index
}

/// A camera looking at an entity from `distance` metres, a little above.
fn look_at(scene: &Scene, k: usize, distance: f64, yaw: f64) -> Camera {
    let p = scene.entities[k].position().unwrap();
    Camera {
        target: [p[0], p[1] + distance * 0.15, p[2]],
        yaw,
        pitch: 0.3,
        distance,
        fov: 60.0,
        projection: Projection::Perspective,
    }
}

fn set_camera(app: &mut EditorApp, c: Camera) {
    app.view_mut()
        .unwrap()
        .viewports
        .get_mut(&Projection::Perspective)
        .unwrap()
        .camera = c;
}

fn solid_render(d: &[Drawn]) -> Option<u32> {
    d.iter().find(|x| x.render_mesh && !x.wire).map(|x| x.count)
}

/// The game view's indices for an entity (every draw of its prepared mesh, LOD 0), from `bc2_render`'s world.
fn game_indices(world: &bc2_render::world::World, entity_of: &[usize], k: usize) -> Option<usize> {
    let at = world.object_of.iter().position(|&n| entity_of[n] == k)?;
    Some(
        world.objects[at]
            .prepared
            .draws
            .iter()
            .map(|d| d.index_count as usize)
            .sum(),
    )
}

#[test]
fn the_simplified_view_draws_render_meshes() {
    let _gate = crate::gate();
    let Ok(opfor) = levels::find_opfor(None) else {
        println!("skipped: no game copy");
        return;
    };
    let mut h = match Headless::new([1600, 950]) {
        Ok(h) => h,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    let gpu = h.gpu.clone();
    let ctx = h.ctx.clone();
    let tokens = Tokens::embedded().unwrap();
    // (level, blueprint ending)
    let samples: &[(&str, &str)] = &[
        ("mp_005", "mebush_01/mebush_01"),
        ("mp_005", "acacia_01/acacia_01"),
        ("mp_005", "props/mp05_rockformation01"),
        ("mp_005", "props/mp05_cliff01"),
        ("mp_005", "buildings/barrack/barrack"),
        ("sp_01", "palmtree_01/palmtree_01"),
        ("sp_01", "_sp01/rocks/rock_large"),
        ("sp_01", "jungle_bush_1/jungle_bush_1"),
        ("sp_01", "buildings/guardtower/guardtower"),
    ];
    for lv in ["mp_005", "sp_01"] {
        let Some(mut app) = open(&mut h, lv, None) else {
            continue;
        };
        let pal = palette(&app);
        let scene = app.view().unwrap().scene().clone();
        let read = Arc::clone(app.view().unwrap().gpu_scene().visuals().unwrap());
        println!(
            "{lv}: {} render meshes read in {:.2} s ({} with coarser levels of detail, {} destructible blueprints)",
            read.meshes.len(),
            read.seconds,
            read.meshes.iter().filter(|m| !m.lods.is_empty()).count(),
            read.components.values().filter(|c| !c.is_empty()).count()
        );
        // ---- the render meshes, against the game view's reading of the same meshes
        let (placed, entity_of) = bc2_editor::gameview::placed_entities(&scene);
        let world = bc2_render::world::World::load(&opfor, lv, &scene.name, &placed, None).unwrap();
        for &(_, bp) in samples.iter().filter(|s| s.0 == lv) {
            let k = entity(&scene, bp);
            let e = &scene.entities[k];
            let Some(Shape::Mesh(m)) = render::shape_of(&read, e) else {
                panic!("{lv} {bp}: no render mesh read")
            };
            let mesh = &read.meshes[m];
            let game = game_indices(&world, &entity_of, k).unwrap();
            assert_eq!(mesh.index_count(), game, "{lv} {bp}: the same indices as the game view");
            let parts = read.components_of(e.blueprint.as_deref().unwrap());
            let expect: u32 = mesh
                .drawn_parts()
                .filter(|p| part_shown(parts, &p.name, State::Intact))
                .map(|p| p.count)
                .sum();
            let drawn = app.view().unwrap().gpu_scene().drawn(k);
            let got = solid_render(&drawn).unwrap_or_else(|| panic!("{lv} {bp}: not drawn as its mesh: {drawn:?}"));
            assert_eq!(got, expect, "{lv} {bp}");
            let collision: u32 = drawn.iter().filter(|d| !d.render_mesh).map(|d| d.count).sum();
            println!(
                "{lv} {bp}: render mesh {got} indices drawn of the game view's {game} ({} parts, {} alpha-tested, {} \
                 blended left out{}); {} levels of detail",
                mesh.parts.len(),
                mesh.parts.iter().filter(|p| p.tested).count(),
                mesh.parts.iter().filter(|p| p.blended).count(),
                if parts.is_empty() {
                    String::new()
                } else {
                    format!(", {} destructible parts: intact state", parts.len())
                },
                mesh.lods.len() + 1
            );
            assert_eq!(collision, 0, "{lv} {bp}: collision is not drawn as well");
        }
        // ---- a leaf-card blueprint renders with holes
        let leaf = if lv == "mp_005" {
            "acacia_01/acacia_01"
        } else {
            "palmtree_01/palmtree_01"
        };
        let k = entity(&scene, leaf);
        let size = [640u32, 480];
        let r = Renderer::new(&gpu.gpu);
        let target = r.target(&gpu.gpu, size);
        let cam = look_at(&scene, k, 18.0, 0.7);
        let view = cam.view([f64::from(size[0]), f64::from(size[1])]);
        let only = |k2: usize| k2 != k;
        let mut sg = SceneGpu::new(&gpu.gpu, &scene, &pal);
        sg.set_visuals(&gpu.gpu, Arc::clone(&read));
        let count = |sg: &SceneGpu| {
            r.render(&gpu.gpu, sg, &target, &view.matrix(), &pal);
            let id = render::entity_id(k);
            r.read_ids(&gpu.gpu, &target)
                .unwrap()
                .iter()
                .filter(|&&i| i == id)
                .count()
        };
        sg.refresh(&gpu.gpu, &scene, &pal, &only);
        let cut = count(&sg);
        sg.render_meshes = false;
        sg.refresh(&gpu.gpu, &scene, &pal, &only);
        let solid = count(&sg);
        println!("{lv} {leaf}: {cut} pixels with the leaves cut, {solid} drawn solid (its collision: the same cards)");
        assert!(
            cut > 0 && (cut as f64) < 0.85 * solid as f64,
            "{lv} {leaf}: holes {cut} / {solid}"
        );
        // ---- the view's and an object's Show as, by index counts
        let b = entity(
            &scene,
            if lv == "mp_005" {
                "barrack/barrack"
            } else {
                "guardtower/guardtower"
            },
        );
        let show = |app: &mut EditorApp, s: ShowAs| {
            app.view_mut().unwrap().set_display(&gpu, &pal, |d| d.show = s);
            app.view().unwrap().gpu_scene().drawn(b)
        };
        let visual = show(&mut app, ShowAs::Visual);
        let collision = show(&mut app, ShowAs::Collision);
        let both = show(&mut app, ShowAs::Both);
        let wire = show(&mut app, ShowAs::Wireframe);
        let bounds = show(&mut app, ShowAs::Bounds);
        show(&mut app, ShowAs::Visual);
        let col_count: u32 = collision.iter().map(|d| d.count).sum();
        println!(
            "{lv} show as: visual {visual:?}, collision {collision:?}, both {both:?}, wireframe {wire:?}, bounds \
             {bounds:?}"
        );
        assert!(visual.len() == 1 && visual[0].render_mesh && !visual[0].wire);
        assert!(!collision.is_empty() && collision.iter().all(|d| !d.render_mesh && !d.wire));
        assert_ne!(visual[0].count, col_count, "the mesh and its collision differ");
        assert_eq!(solid_render(&both), Some(visual[0].count));
        assert_eq!(
            both.iter()
                .filter(|d| d.wire && !d.render_mesh)
                .map(|d| d.count)
                .sum::<u32>(),
            col_count
        );
        assert!(wire.len() == 1 && wire[0].render_mesh && wire[0].wire && wire[0].count == visual[0].count);
        assert!(
            bounds.len() == 1 && bounds[0].wire && bounds[0].count == 36,
            "the box: {bounds:?}"
        );
        // an object's override, its blueprint's, and the view's collision mode
        app.select(&gpu, Some(b));
        app.run_action(&gpu, &ctx, Action::ShowObjectsAs(ShowAs::Hidden));
        assert!(app.view().unwrap().gpu_scene().drawn(b).is_empty(), "hidden");
        app.run_action(&gpu, &ctx, Action::ShowObjectsAs(ShowAs::Collision));
        assert_eq!(
            app.view()
                .unwrap()
                .gpu_scene()
                .drawn(b)
                .iter()
                .map(|d| d.count)
                .sum::<u32>(),
            col_count
        );
        let other = scene
            .entities
            .iter()
            .find(|e| e.index != b && e.blueprint == scene.entities[b].blueprint)
            .map(|e| e.index);
        app.run_action(&gpu, &ctx, Action::ShowBlueprintAs(ShowAs::Bounds));
        if let Some(o) = other {
            let d = app.view().unwrap().gpu_scene().drawn(o);
            assert!(
                d.len() == 1 && d[0].count == 36,
                "the blueprint's others as bounds: {d:?}"
            );
        }
        // the object's own override wins over its blueprint's
        assert!(app.view().unwrap().gpu_scene().drawn(b).iter().all(|d| !d.render_mesh));
        app.run_action(&gpu, &ctx, Action::SetViewMode(ViewMode::Collision));
        assert!(
            app.view()
                .unwrap()
                .gpu_scene()
                .drawn(b)
                .iter()
                .all(|d| !d.render_mesh && !d.wire)
        );
        app.run_action(&gpu, &ctx, Action::SetViewMode(ViewMode::Lit));
        // ---- the LOD picker and the destruction states
        let e = scene.entities[b].clone();
        let Some(Shape::Mesh(m)) = render::shape_of(&read, &e) else {
            panic!()
        };
        let mesh = &read.meshes[m];
        let guid = e.guid.to_lowercase();
        app.run_action(&gpu, &ctx, Action::ResetViewOverrides);
        let lod0 = solid_render(&app.view().unwrap().gpu_scene().drawn(b)).unwrap();
        assert!(!mesh.lods.is_empty(), "{lv}: the sample has coarser levels of detail");
        let g = guid.clone();
        app.view_mut().unwrap().set_display(&gpu, &pal, |d| {
            d.lods.insert(g, 1);
        });
        let lod1 = solid_render(&app.view().unwrap().gpu_scene().drawn(b)).unwrap();
        println!(
            "{lv} {}: LOD 0 {lod0} indices, LOD 1 {lod1} (the mesh set's levels: {:?})",
            e.blueprint.as_deref().unwrap(),
            std::iter::once(mesh.index_count())
                .chain(mesh.lods.iter().map(|l| l.index_count()))
                .collect::<Vec<_>>()
        );
        assert!(lod1 < lod0, "LOD 1 is coarser");
        let parts = read.components_of(e.blueprint.as_deref().unwrap()).to_vec();
        if !parts.is_empty() {
            let mut counts = Vec::new();
            for s in Destruction::ALL {
                let g = guid.clone();
                app.view_mut().unwrap().set_display(&gpu, &pal, |d| {
                    d.lods.clear();
                    d.states.insert(g, s);
                });
                counts.push(solid_render(&app.view().unwrap().gpu_scene().drawn(b)).unwrap_or(0));
            }
            println!(
                "{lv} {}: {} destructible parts {:?}; indices intact / destroyed / all parts {counts:?}",
                e.blueprint.as_deref().unwrap(),
                parts.len(),
                parts.iter().map(|c| (&c.name, &c.states)).collect::<Vec<_>>()
            );
            assert!(counts[2] >= counts[0].max(counts[1]), "{counts:?}");
            // SP_01's guard tower: its mesh holds its parts' intact and destroyed pieces (MP_005's barrack holds only
            // its ruin and windows: its parts' pieces are another mesh's)
            if lv == "sp_01" {
                assert_ne!(counts[0], counts[1], "intact and destroyed differ");
            }
        }
        // ---- an override survives the editor's settings saved and read again; Reset clears it
        app.select(&gpu, Some(b));
        app.run_action(&gpu, &ctx, Action::ShowObjectsAs(ShowAs::Both));
        let text = serde_json::to_string(&app.saved()).unwrap();
        let saved: Saved = serde_json::from_str(&text).unwrap();
        drop(app);
        let mut app = open(&mut h, lv, Some(saved)).unwrap();
        assert_eq!(app.view().unwrap().display().objects.get(&guid), Some(&ShowAs::Both));
        assert!(app.view().unwrap().gpu_scene().drawn(b).iter().any(|d| d.wire));
        app.run_action(&gpu, &ctx, Action::ResetViewOverrides);
        assert!(!app.view().unwrap().display().has_overrides());
        assert_eq!(app.view().unwrap().gpu_scene().drawn(b).len(), 1);
        // ---- the invisible walls (SP_01's 17 slabs)
        if lv == "sp_01" {
            let walls: Vec<usize> = scene
                .entities
                .iter()
                .filter(|e| e.blueprint.as_deref().is_some_and(|b| b.contains("invisiblewall")))
                .map(|e| e.index)
                .collect();
            let drawn = |app: &EditorApp| -> Vec<Vec<Drawn>> {
                walls
                    .iter()
                    .map(|&w| app.view().unwrap().gpu_scene().drawn(w))
                    .collect()
            };
            assert_ne!(walls, [] as [usize; 0]);
            assert!(drawn(&app).iter().all(Vec::is_empty), "hidden at the start");
            app.run_action(&gpu, &ctx, Action::SetInvisibleAs(InvisibleAs::Outline));
            assert!(drawn(&app).iter().all(|d| !d.is_empty() && d.iter().all(|x| x.wire)));
            app.run_action(&gpu, &ctx, Action::SetInvisibleAs(InvisibleAs::Solid));
            assert!(drawn(&app).iter().all(|d| !d.is_empty() && d.iter().all(|x| !x.wire)));
            app.run_action(&gpu, &ctx, Action::SetInvisibleAs(InvisibleAs::Hidden));
            println!(
                "sp_01: {} invisible walls hidden, outlined and solid as asked",
                walls.len()
            );
        }
        // ---- picking: at sampled objects' centres, as before (collision drawn) and now
        let mut before = SceneGpu::new(&gpu.gpu, &scene, &pal);
        before.refresh(&gpu.gpu, &scene, &pal, &|_| false);
        let mut after = SceneGpu::new(&gpu.gpu, &scene, &pal);
        after.set_visuals(&gpu.gpu, Arc::clone(&read));
        after.refresh(&gpu.gpu, &scene, &pal, &|_| false);
        let picked: Vec<usize> = scene
            .entities
            .iter()
            .filter(|e| e.matrix.is_some() && after.drawn(e.index).first().is_some_and(|d| d.render_mesh))
            .map(|e| e.index)
            .step_by(37)
            .take(60)
            .collect();
        let (mut same, mut hit_before, mut hit_now, mut lost, mut gained) = (0, 0, 0, Vec::new(), Vec::new());
        let (mut visible, mut selectable) = (0, 0);
        for &k in &picked {
            let s = after.sizes.get(k).copied().unwrap_or(4.0).clamp(2.0, 80.0);
            let cam = look_at(&scene, k, s * 2.5, 1.1);
            let view = cam.view([f64::from(size[0]), f64::from(size[1])]);
            let p = scene.entities[k].position().unwrap();
            let Some(px) = view.project([p[0], p[1] + s * 0.25, p[2]]) else {
                continue;
            };
            let at = [px[0] as u32, px[1] as u32];
            let pick = |sg: &SceneGpu| {
                r.render(&gpu.gpu, sg, &target, &view.matrix(), &pal);
                render::id_entity(r.pick(&gpu.gpu, &target, at, 4))
            };
            let (a, b2) = (pick(&before), pick(&after));
            let bp = scene.entities[k].blueprint.clone().unwrap_or_default();
            same += usize::from(a == b2);
            hit_before += usize::from(a == Some(k));
            hit_now += usize::from(b2 == Some(k));
            if a == Some(k) && b2 != Some(k) {
                lost.push(bp);
            } else if a != Some(k) && b2 == Some(k) {
                gained.push(bp);
            }
            // where the object is drawn now, a click picks it: its drawn pixel nearest the centre
            r.render(&gpu.gpu, &after, &target, &view.matrix(), &pal);
            let ids = r.read_ids(&gpu.gpu, &target).unwrap();
            let id = render::entity_id(k);
            let near = ids
                .iter()
                .enumerate()
                .filter(|(_, i)| **i == id)
                .map(|(n, _)| [(n as u32) % size[0], (n as u32) / size[0]])
                .min_by_key(|q| q[0].abs_diff(at[0]).pow(2) + q[1].abs_diff(at[1]).pow(2));
            if let Some(q) = near {
                visible += 1;
                selectable += usize::from(r.pick(&gpu.gpu, &target, q, 4) == id);
            }
        }
        println!(
            "{lv}: a click at {} objects' centres: the same object picked as before {same} times; the object itself \
             {hit_before} times before, {hit_now} now; no longer picked there (its mesh is not where its collision \
             is): {lost:?}; picked there now: {gained:?}; a click where each is drawn picks it: {selectable} of \
             {visible}",
            picked.len()
        );
        // the click picks what is drawn: an object whose mesh is thinner than its collision can be missed at its
        // centre, one whose collision was thinner found there; every object drawn is picked where it is drawn
        assert_eq!(selectable, visible, "{lv}: every drawn object picked where it is drawn");
        assert!(
            hit_now + 1 >= hit_before,
            "{lv}: at the centres {hit_now} now, {hit_before} before"
        );
        // ---- the view modes still draw
        for m in ViewMode::ALL {
            app.run_action(&gpu, &ctx, Action::SetViewMode(m));
            h.settle(&mut app, 2);
            assert!(h.image().is_some(), "{m:?}");
        }
        app.run_action(&gpu, &ctx, Action::SetViewMode(ViewMode::Lit));
        // ---- frame times: the view drawn and waited for, the camera turning, collision against render meshes
        let overview = Camera {
            yaw: 0.0,
            ..app.view().unwrap().viewports[&Projection::Perspective].camera
        };
        for (label, on) in [("collision", false), ("render meshes", true)] {
            after.render_meshes = on;
            after.refresh(&gpu.gpu, &scene, &pal, &|_| false);
            let big = r.target(&gpu.gpu, [1600, 950]);
            for (name, cam) in [("overview", overview), ("near", look_at(&scene, b, 60.0, 0.4))] {
                let mut times = Vec::new();
                for f in 0..12 {
                    let c = Camera {
                        yaw: cam.yaw + 0.01 * f64::from(f),
                        ..cam
                    };
                    let view = c.view([1600.0, 950.0]);
                    let t = Instant::now();
                    r.render(&gpu.gpu, &after, &big, &view.matrix(), &pal);
                    let _ = gpu.gpu.device.poll(eframe::egui_wgpu::wgpu::PollType::Wait {
                        submission_index: None,
                        timeout: None,
                    });
                    times.push(t.elapsed().as_secs_f64() * 1e3);
                }
                times.drain(..2);
                times.sort_by(f64::total_cmp);
                println!(
                    "{lv} frame, {label}, {name}: median {:.2} ms (1600 x 950, {} triangles listed)",
                    times[times.len() / 2],
                    after.triangles
                );
            }
        }
        // ---- the pictures
        if let Some(dir) = shots() {
            let views: Vec<(&str, Camera)> = if lv == "mp_005" {
                vec![
                    (
                        "rocks",
                        look_at(&scene, entity(&scene, "props/mp05_rockformation01"), 90.0, 2.2),
                    ),
                    (
                        "bushes",
                        look_at(&scene, entity(&scene, "mebush_01/mebush_01"), 25.0, 0.6),
                    ),
                    ("barrack", look_at(&scene, entity(&scene, "barrack/barrack"), 45.0, 0.8)),
                ]
            } else {
                let wall = entity(&scene, "invisiblewall/invwall");
                vec![
                    (
                        "palms",
                        look_at(&scene, entity(&scene, "palmtree_01/palmtree_01"), 30.0, 0.6),
                    ),
                    ("walls", look_at(&scene, wall, 120.0, 0.9)),
                    (
                        "guardtower",
                        look_at(&scene, entity(&scene, "guardtower/guardtower"), 30.0, 0.8),
                    ),
                ]
            };
            for theme in ThemeId::ALL {
                app.theme = theme;
                h.settle(&mut app, 3);
                let pal = Palette::new(tokens, theme);
                let t = if theme == ThemeId::Dark { "dark" } else { "light" };
                for (name, cam) in &views {
                    set_camera(&mut app, *cam);
                    app.view_mut().unwrap().set_render_meshes(&gpu, &pal, false);
                    h.settle(&mut app, 3);
                    h.save(&dir.join(format!("sv1_{lv}_{name}_before_{t}.png"))).unwrap();
                    app.view_mut().unwrap().set_render_meshes(&gpu, &pal, true);
                    h.settle(&mut app, 3);
                    h.save(&dir.join(format!("sv1_{lv}_{name}_after_{t}.png"))).unwrap();
                    if *name == "walls" {
                        app.run_action(&gpu, &ctx, Action::SetInvisibleAs(InvisibleAs::Outline));
                        h.settle(&mut app, 3);
                        h.save(&dir.join(format!("sv1_{lv}_{name}_outlined_{t}.png"))).unwrap();
                        app.run_action(&gpu, &ctx, Action::SetInvisibleAs(InvisibleAs::Hidden));
                    }
                    if matches!(*name, "barrack" | "guardtower") {
                        for s in [ShowAs::Collision, ShowAs::Both] {
                            app.view_mut().unwrap().set_display(&gpu, &pal, |d| d.show = s);
                            h.settle(&mut app, 3);
                            let n = if s == ShowAs::Both { "both" } else { "collision" };
                            h.save(&dir.join(format!("sv1_{lv}_{name}_as_{n}_{t}.png"))).unwrap();
                        }
                        app.view_mut()
                            .unwrap()
                            .set_display(&gpu, &pal, |d| d.show = ShowAs::Visual);
                    }
                }
            }
            app.theme = ThemeId::Dark;
            println!("{lv}: pictures saved in {}", dir.display());
        }
    }
}
