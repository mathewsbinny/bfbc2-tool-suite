//! The Terrain workspace's brushes in the running editor (round E5), headless, on the demo level given a synthetic
//! height field (256 samples of 1 m, 0 - 100 m) and material map (512 cells of 0.5 m; no game copy; skipped without
//! a graphics adapter), in both themes:
//!
//! - the Raise brush dragged over the ground is one `stroke` operation on the height field: the scene's heights rise
//!   under the drag (and its terrain mesh with them), nothing far from it moves; Ctrl+Z puts the base back, Ctrl+Y the
//!   stroke; the log replayed from its tiles beside the project (a rebuild) gives the same heights;
//! - Flatten, Smooth and Noise are strokes of their own; the Ramp takes two clicks and leaves a slope between them;
//! - the Material brush paints the material map (a `stroke` on the material target, project format 11) and the
//!   toolbar shows the map's materials;
//! - round E5-rest, on a synthetic visual terrain, atlas, colour / occlusion map and ring: the Paint layer brush lays
//!   a visual layer (a `stroke` on its paint layer), with its paired material in the same operation; the Shade brush
//!   darkens the colour / occlusion map; a Raise at the main block's edge reaches the ring in the same operation; the
//!   re-shade after the sculpt is one stroke on the colour map; the range widened keeps the ground where it was and
//!   its undo puts the old range back (project format 12);
//! - leaving the workspace gives the Select tool back.
//!
//! `BC2_EDITOR_SHOTS=<folder>` saves the screenshots (`terrain_raise_<theme>.png`, `terrain_material_<theme>.png`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::float_cmp,
    clippy::too_many_lines,
    clippy::panic,
    clippy::many_single_char_names,
    clippy::doc_markdown,
    missing_docs
)]

use std::sync::Arc;

use bc2_editor::actions::{Action, Brush};
use bc2_editor::app::{EditorApp, Then};
use bc2_editor::camera::Projection;
use bc2_editor::demo;
use bc2_editor::gizmo::Tool;
use bc2_editor::headless::Headless;
use bc2_editor::sculpt::{Falloff, Ground};
use bc2_editor::theme::ThemeId;
use bc2_editor::workspace::Workspace;
use bc2_formats::terrain_codec::{MainBlock, MaterialMap};
use bc2_levelwrite::Op;
use bc2_levelwrite::delta::Raster;
use bc2_levelwrite::files::Target;
use eframe::egui::{Key, Modifiers, Pos2, Vec2};

const CTRL: Modifiers = Modifiers {
    alt: false,
    ctrl: true,
    shift: false,
    mac_cmd: false,
    command: true,
};

fn shot(h: &mut Headless, app: &mut EditorApp, name: &str) {
    if let Some(dir) = std::env::var_os("BC2_EDITOR_SHOTS") {
        h.settle(app, 3);
        let p = std::path::Path::new(&dir).join(name);
        h.save(&p).unwrap();
        println!("saved {}", p.display());
    }
}

/// The demo level with a synthetic 256 m terrain (a gentle slope) and material map.
fn scene(dir: &std::path::Path) -> bc2_scene::scene::Scene {
    let mut s = demo::level(dir).unwrap();
    let n = 256usize;
    let h: Vec<u16> = (0..n * n)
        .map(|i| (6000 + (i % n) * 20 + (i / n) * 10) as u16)
        .collect();
    let bytes = MainBlock::new([-128.0, 0.0, -128.0, 128.0, 100.0, 128.0], h, None)
        .unwrap()
        .encode();
    let hf = bc2_formats::terrain::parse(&bytes).unwrap();
    s.terrain = Some(Arc::new(hf.mesh(2)));
    s.heightfield = Some(Arc::new(hf));
    s.material_map = Some(Arc::new(
        MaterialMap::new(512, vec!["defaultmaterial".into(), "grass".into(), "tarmac".into()]).unwrap(),
    ));
    s.terrain_extra = Some(Arc::new(extra()));
    s
}

/// Round E5-rest: a visual terrain of 16 x 16 cells (every cell the rule-only `Base_Destr`), an empty mask atlas, a
/// grey DXT1 colour / occlusion map of 256 texels over 2,048 m and DICE's ring of 12 blocks around the 256 m terrain.
fn extra() -> bc2_scene::scene::TerrainExtra {
    use bc2_formats::visual_terrain::{PatchMap, PatchShader, VisualTerrain};
    let shader = |name: &str, layers: u32, tiled: u32| PatchShader {
        name: format!("Terrains/demo/demo/PatchMaskShaders/{name}"),
        undergrowth: String::new(),
        layers,
        tiled,
        groups: Vec::new(),
    };
    let vt = VisualTerrain {
        roads: vec![VisualTerrain::EMPTY_ROADS.to_vec(); 2],
        shaders: vec![
            shader("Base_Destr", 2, 0),
            shader("Base_Grass_Destr", 3, 1),
            shader("Base_Rock_Destr", 3, 1),
            shader("Base_Grass_Rock_Destr", 4, 2),
        ],
        maps: vec![PatchMap::rule_only(
            [-128.0, 0.0, -128.0, 128.0, 100.0, 128.0],
            [66, 1, 15, 7],
            16,
            0,
            "Terrains/demo/demo.Heightfield-000",
            "Terrains/demo/demo.MaskTileAtlas",
        )],
    };
    let pic = |w: usize, h: usize, c: [u8; 3]| bc2_formats::itexture::Picture {
        w,
        h,
        rgb: vec![c; w * h],
    };
    // DICE's ring order (TERR-HF-12), in half main sides from the main block's min corner
    let places: [(i32, i32); 12] = [
        (-2, -2),
        (-1, -2),
        (-2, -1),
        (0, -2),
        (1, -2),
        (1, -1),
        (-2, 0),
        (-2, 1),
        (-1, 1),
        (1, 0),
        (0, 1),
        (1, 1),
    ];
    let ring = places
        .iter()
        .map(|&(px, pz)| {
            let (x0, z0) = (f32::from(px as i16) * 128.0, f32::from(pz as i16) * 128.0);
            bc2_formats::terrain_codec::SubBlock::new([x0, 0.0, z0, x0 + 128.0, 100.0, z0 + 128.0], vec![6000; 64 * 64])
                .unwrap()
                .encode()
        })
        .collect();
    bc2_scene::scene::TerrainExtra {
        visual: Some(vt.encode()),
        atlas: Some(bc2_formats::itexture::encode(
            &pic(1024, 512, [0; 3]),
            bc2_formats::itexture::Format::L8,
            0,
            false,
        )),
        ring,
        colour: Some((
            "terrains/demo/textures/demo_occlusion.res".into(),
            bc2_formats::itexture::encode(
                &pic(256, 256, [128, 128, 128]),
                bc2_formats::itexture::Format::Dxt1,
                2,
                true,
            ),
        )),
    }
}

fn ops(app: &EditorApp) -> Vec<Op> {
    let d = app.view().unwrap().doc.as_ref().unwrap();
    d.project.operations.iter().map(|o| o.op.clone()).collect()
}

/// The scene's height in metres at world (x, z) (the nearest sample).
fn height(app: &EditorApp, x: f64, z: f64) -> f64 {
    let hf = app.view().unwrap().scene().heightfield.clone().unwrap();
    let g = Ground::of(&hf);
    let r = Raster::from_samples(hf.size, hf.size, &hf.raw);
    let (i, j) = g.grid(x, z);
    g.metres(r.get(i.round() as u32, j.round() as u32))
}

/// A screen point of the 3D view on the ground at world (x, z).
fn ground(app: &EditorApp, x: f64, z: f64) -> Pos2 {
    let v = app.view().unwrap();
    let vp = &v.viewports[&Projection::Perspective];
    let rect = vp.rect.unwrap();
    let view = vp.camera.view([f64::from(rect.width()), f64::from(rect.height())]);
    let s = view.project([x, height(app, x, z), z]).unwrap();
    rect.min + Vec2::new(s[0] as f32, s[1] as f32)
}

fn brush(h: &mut Headless, app: &mut EditorApp, b: Brush) {
    let (gpu, ctx) = (h.gpu.clone(), h.ctx.clone());
    app.run(&gpu, &ctx, Action::Brush(b));
    h.settle(app, 2);
}

#[test]
// spec: TEDIT-HF-3, TEDIT-MM-1
fn terrain_brushes_through_the_interface() {
    let _gate = crate::gate();
    let mut h = match Headless::new([1500, 900]) {
        Ok(h) => h,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    for theme in ThemeId::ALL {
        let dir = std::env::temp_dir().join(format!("bc2_editor_terrain_{}_{}", theme.key(), std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let s = scene(&dir.join("level"));
        let mut app = EditorApp::new(None, None).unwrap();
        app.theme = theme;
        h.settle(&mut app, 2);
        let gpu = h.gpu.clone();
        let ctx = h.ctx.clone();
        app.open_scene_then(&gpu, "demo", s, Then::Create("Terrain demo".into(), dir.clone(), None));
        h.settle(&mut app, 6);
        app.run(&gpu, &ctx, Action::SetWorkspace(Workspace::Terrain));
        h.settle(&mut app, 2);

        // ------------------------------------------------------------------ raise: one stroke, undone, redone
        brush(&mut h, &mut app, Brush::Raise);
        assert_eq!(app.view().unwrap().tool, Tool::Terrain);
        {
            let t = &mut app.view_mut().unwrap().tools.terrain;
            t.settings.radius = 10.0;
            t.settings.strength = 1.0;
            t.settings.falloff = Falloff::Smooth;
        }
        let (before, far) = (height(&app, 10.0, 10.0), height(&app, -80.0, -80.0));
        let (a, b) = (ground(&app, 5.0, 10.0), ground(&app, 15.0, 10.0));
        h.drag(&mut app, a, b, 12, Modifiers::NONE);
        h.settle(&mut app, 2);
        let log = ops(&app);
        assert_eq!(log.len(), 1, "{} {log:?}", app.view().unwrap().message);
        let Op::Stroke {
            target, size, tiles, ..
        } = &log[0]
        else {
            panic!("{log:?}")
        };
        assert!(matches!(target, Target::Terrain { .. }), "{target:?}");
        assert_eq!(*size, [256, 256]);
        assert!(!tiles.is_empty() && tiles.len() <= 4, "{} tiles", tiles.len());
        let raised = height(&app, 10.0, 10.0);
        assert!(raised > before + 1.0, "{before} -> {raised}");
        assert_eq!(height(&app, -80.0, -80.0), far, "far from the stroke");
        // the mesh follows the heights
        let mesh_y = {
            let v = app.view().unwrap();
            let (verts, _) = v.scene().terrain.as_deref().unwrap();
            verts
                .iter()
                .find(|p| (f64::from(p[0]) - 10.0).abs() < 0.5 && (f64::from(p[2]) - 10.0).abs() < 0.5)
                .unwrap()[1]
        };
        assert!((f64::from(mesh_y) - raised).abs() < 0.01, "{mesh_y} vs {raised}");
        shot(&mut h, &mut app, &format!("terrain_raise_{}.png", theme.key()));
        h.key(&mut app, Key::Z, CTRL);
        assert_eq!(ops(&app).len(), 0, "undone");
        assert_eq!(height(&app, 10.0, 10.0), before);
        h.key(&mut app, Key::Y, CTRL);
        assert_eq!(ops(&app).len(), 1, "redone");
        assert_eq!(height(&app, 10.0, 10.0), raised);
        // the log replayed from its tiles beside the project gives the same heights
        app.view_mut().unwrap().doc.as_mut().unwrap().rebuild();
        assert_eq!(height(&app, 10.0, 10.0), raised);

        // ------------------------------------------------------------------ flatten, smooth, noise: a stroke each
        for (k, b) in [Brush::Flatten, Brush::Smooth, Brush::Noise].into_iter().enumerate() {
            brush(&mut h, &mut app, b);
            let (a, c) = (ground(&app, -40.0, 30.0), ground(&app, -20.0, 40.0));
            h.drag(&mut app, a, c, 8, Modifiers::NONE);
            assert_eq!(ops(&app).len(), 2 + k, "{b:?}: {}", app.view().unwrap().message);
        }

        // ------------------------------------------------------------------ a ramp: two clicks
        brush(&mut h, &mut app, Brush::Ramp);
        let (ra, rb) = (ground(&app, -60.0, -40.0), ground(&app, -20.0, -40.0));
        let (ha, hb) = (height(&app, -60.0, -40.0), height(&app, -20.0, -40.0));
        h.click(&mut app, ra);
        assert!(app.view().unwrap().tools.terrain.ramp_from.is_some());
        h.click(&mut app, rb);
        assert_eq!(ops(&app).len(), 5, "{}", app.view().unwrap().message);
        let mid = height(&app, -40.0, -40.0);
        assert!((mid - f64::midpoint(ha, hb)).abs() < 1.0, "{ha} {mid} {hb}");

        // ------------------------------------------------------------------ the material brush
        brush(&mut h, &mut app, Brush::Material);
        app.view_mut().unwrap().tools.terrain.settings.material = 2;
        let (a, c) = (ground(&app, 30.0, -30.0), ground(&app, 40.0, -30.0));
        h.drag(&mut app, a, c, 6, Modifiers::NONE);
        let log = ops(&app);
        assert_eq!(log.len(), 6, "{}", app.view().unwrap().message);
        let Op::Stroke { target, size, .. } = &log[5] else {
            panic!("{log:?}")
        };
        assert!(matches!(target, Target::Material { .. }), "{target:?}");
        assert_eq!(*size, [512, 512]);
        let d = app.view().unwrap().doc.as_ref().unwrap();
        assert_eq!(d.project.reader_needed(), 11);
        let cells = d.raster(target).unwrap();
        let g = Ground::of(d.scene.heightfield.as_deref().unwrap());
        let cell = |x: f64, z: f64| cells.get(((x - g.x0) / 0.5) as u32, ((z - g.z0) / 0.5) as u32);
        assert_eq!(cell(35.0, -30.0), 2, "painted tarmac");
        assert_eq!(cell(-60.0, 60.0), 0, "elsewhere the base's");
        assert_eq!(d.material_names(), ["defaultmaterial", "grass", "tarmac"]);
        shot(&mut h, &mut app, &format!("terrain_material_{}.png", theme.key()));

        // ------------------------------------------------------------------ round E5-rest: the visual paint
        brush(&mut h, &mut app, Brush::Paint);
        assert_eq!(app.view().unwrap().tools.terrain.brush, Some(Brush::Paint));
        assert_eq!(
            app.view().unwrap().tools.terrain.settings.layer,
            "Grass",
            "the first paintable layer"
        );
        let (a, c) = (ground(&app, -10.0, 60.0), ground(&app, 10.0, 60.0));
        h.drag(&mut app, a, c, 6, Modifiers::NONE);
        let log = ops(&app);
        assert_eq!(log.len(), 7, "{}", app.view().unwrap().message);
        let Op::Stroke { target, size, also, .. } = &log[6] else {
            panic!("{log:?}")
        };
        assert_eq!(*target, Target::Paint { layer: "Grass".into() });
        assert_eq!((*size, also.len()), ([256, 256], 0));
        // with its paired material: one operation, two rasters
        app.view_mut().unwrap().tools.terrain.settings.pair = Some(1);
        let (a, c) = (ground(&app, -10.0, 80.0), ground(&app, 10.0, 80.0));
        h.drag(&mut app, a, c, 6, Modifiers::NONE);
        let log = ops(&app);
        let Op::Stroke { target, also, .. } = &log[7] else {
            panic!("{log:?}")
        };
        assert!(matches!(target, Target::Paint { .. }));
        assert!(
            matches!(also.as_slice(), [p] if matches!(p.target, Target::Material { .. })),
            "{also:?}"
        );
        shot(&mut h, &mut app, &format!("terrain_paint_{}.png", theme.key()));

        // ------------------------------------------------------------------ the shade brush
        brush(&mut h, &mut app, Brush::Shade);
        let (a, c) = (ground(&app, 40.0, 40.0), ground(&app, 60.0, 40.0));
        h.drag(&mut app, a, c, 6, Modifiers::NONE);
        let log = ops(&app);
        assert_eq!(log.len(), 9, "{}", app.view().unwrap().message);
        assert!(
            matches!(
                &log[8],
                Op::Stroke {
                    target: Target::Colour { .. },
                    ..
                }
            ),
            "{:?}",
            log[8]
        );

        // ------------------------------------------------------------------ a raise at the edge reaches the ring
        brush(&mut h, &mut app, Brush::Raise);
        let (a, c) = (ground(&app, 118.0, 0.0), ground(&app, 124.0, 10.0));
        h.drag(&mut app, a, c, 6, Modifiers::NONE);
        let log = ops(&app);
        assert_eq!(log.len(), 10, "{}", app.view().unwrap().message);
        let Op::Stroke { target, also, .. } = &log[9] else {
            panic!("{log:?}")
        };
        assert!(matches!(target, Target::Terrain { .. }));
        assert!(
            matches!(also.as_slice(), [p] if matches!(p.target, Target::Ring { .. })),
            "{also:?}"
        );

        // ------------------------------------------------------------------ the re-shade and the range
        let (here, top) = (height(&app, 10.0, 10.0), height(&app, 120.0, 5.0));
        {
            let d = app.view_mut().unwrap().doc.as_mut().unwrap();
            // a 30 m pit dug 40 m across: the sky the pit's floor sees shrinks
            let t = d.terrain_target().unwrap();
            let mut r = d.raster(&t).unwrap();
            for z in 20..60 {
                for x in 160..200 {
                    r.set(x, z, r.get(x, z).saturating_sub(19_660));
                }
            }
            d.stroke(&t, &r, "Pit").unwrap().unwrap();
            let (_, done) = d.reshade().unwrap().unwrap();
            assert!(done.texels > 0, "{done:?}");
            assert_eq!(d.terrain_range(), Some((100.0, 100.0)));
            d.widen_range(150.0).unwrap().unwrap();
            assert_eq!(d.terrain_range(), Some((100.0, 150.0)));
            assert_eq!(d.project.reader_needed(), 12);
            assert!(d.add_material("Mud").is_err(), "no grid to take it from");
        }
        assert!((height(&app, 10.0, 10.0) - here).abs() < 0.01, "the ground kept");
        assert!((height(&app, 120.0, 5.0) - top).abs() < 0.01);
        h.key(&mut app, Key::Z, CTRL);
        let d = app.view().unwrap().doc.as_ref().unwrap();
        assert_eq!(d.terrain_range(), Some((100.0, 100.0)), "undone");
        assert_eq!(d.scene.heightfield.as_ref().unwrap().max[1], 100.0);
        assert!((height(&app, 10.0, 10.0) - here).abs() < 0.01);

        // ------------------------------------------------------------------ leaving the workspace
        app.run(&gpu, &ctx, Action::SetWorkspace(Workspace::Objects));
        h.settle(&mut app, 2);
        assert_eq!(app.view().unwrap().tool, Tool::Select);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Sculpt and paint on MP_005 and SP_01 as loaded (game tier): a raise and a material dab committed as strokes, the
/// scene's heights following, the material target beside the height field, undo back to DICE's heights.
#[test]
#[ignore = "game tier: MP_005's and SP_01's terrains; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn terrain_strokes_on_real_levels() {
    use bc2_editor::edit::Doc;
    use bc2_editor::sculpt::{BrushSettings, paint_dab, sculpt_dab};
    let _gate = crate::gate();
    let Ok(root) = bc2_editor::levels::find_opfor(None) else {
        println!("skipped: no game copy");
        return;
    };
    for lv in ["mp_005", "sp_01"] {
        let found = match bc2_editor::levels::load_options(&root, lv) {
            Ok(f) => f,
            Err(e) => {
                println!("skipped {lv}: {e}");
                continue;
            }
        };
        let s = bc2_scene::scene::load(&found.options).unwrap();
        if s.heightfield.is_none() || s.material_map.is_none() {
            println!("skipped {lv}: its terrain is not unpacked");
            continue;
        }
        let dir = std::env::temp_dir().join(format!("bc2_editor_terrain_real_{lv}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut d = Doc::create("Sculpted", &dir, lv, s).unwrap();
        let ht = d.terrain_target().unwrap();
        let mt = d.material_target().unwrap();
        assert!(
            matches!(&mt, Target::Material { file } if file.ends_with("-materialmap.res")),
            "{mt:?}"
        );
        let base = d.scene.heightfield.clone().unwrap();
        let g = Ground::of(&base);
        let s = BrushSettings {
            radius: 20.0,
            strength: 1.0,
            material: 1,
            ..BrushSettings::default()
        };
        // the middle of the terrain
        let (x, z) = (
            g.x0 + g.cell[0] * f64::from(g.n) / 2.0,
            g.z0 + g.cell[1] * f64::from(g.n) / 2.0,
        );
        let mut r = d.raster(&ht).unwrap();
        let t = std::time::Instant::now();
        let dab = sculpt_dab(&mut r, &g, Brush::Raise, (x, z), &s, 0.0);
        d.stroke(&ht, &r, "Raise").unwrap().unwrap();
        println!(
            "{lv}: n {}, a raise dab and its stroke in {:.3} s",
            g.n,
            t.elapsed().as_secs_f64()
        );
        let (i, j) = g.grid(x, z);
        let at =
            |hf: &bc2_formats::terrain::Heightfield| hf.raw[j.round() as usize * hf.size as usize + i.round() as usize];
        let now = d.scene.heightfield.clone().unwrap();
        assert!(at(&now) > at(&base) || dab.clamped, "{lv}: raised");
        let mut cells = d.raster(&mt).unwrap();
        // a material other than the one under the brush's middle
        let size = bc2_editor::sculpt::cell_size(&g, cells.width());
        let under = cells.get(((x - g.x0) / size) as u32, ((z - g.z0) / size) as u32);
        let names = d.material_names().len() as u16;
        let s = BrushSettings {
            material: ((under + 1) % names) as u8,
            ..s
        };
        let _ = paint_dab(&mut cells, &g, (x, z), &s);
        let t = std::time::Instant::now();
        d.stroke(&mt, &cells, "Material").unwrap().unwrap();
        println!(
            "{lv}: material map {} cells, a paint stroke in {:.3} s",
            cells.width(),
            t.elapsed().as_secs_f64()
        );
        assert_eq!(d.project.reader_needed(), 11);
        assert!(d.undo() && d.undo());
        assert!(
            d.scene.heightfield.as_ref().unwrap().raw == base.raw,
            "{lv}: undone to DICE's heights"
        );
        assert!(d.redo());
        assert!(d.scene.heightfield.as_ref().unwrap().raw == now.raw, "{lv}: redone");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
