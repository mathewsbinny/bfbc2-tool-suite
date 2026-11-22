//! Round GR3's proofs: the mesh viewer folded into the asset browser's preview.
//!
//! - **Parity** (the game copy; skipped without it): every mesh and level of detail the mesh viewer's answers were
//!   recorded for (`tests/data/meshview_parity.json.gz`, written by `bfbc2-mesh-viewer/tests/record_preview_parity.py`
//!   from `meshview.load_mesh` / `stack_models` / `render`: digests and counts only) is read by the preview's reader
//!   ([`bc2_editor::preview::read_model`]) and drawn by its renderer: each part's name, surface, colour texture,
//!   vertex and triangle counts, positions, normals, texture coordinates, indices and decoded texture, the stacked
//!   layout's offsets and the side, top and front pictures must be the Python's, byte for byte (quick tier: about
//!   3 s). `BC2_EDITOR_SHOTS=<folder>` saves the Rust pictures (`gr3_parity\`).
//! - **The panel** (no game copy needed): the demo level open, a mesh put into the preview, the Asset preview panel
//!   draws it, a click on it selects its part (drawn in `view-selection`), hiding the part empties the picture.
//! - **Screenshots** (`BC2_EDITOR_SHOTS`, the game copy): the preview of `MP_005`'s guard tower and a rifle from the
//!   Meshes mode, Dark and Light.
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::too_many_lines,
    missing_docs
)]

use std::io::Read as _;
use std::path::PathBuf;

use bc2_convert::meshview::render::{View, render, stack_models};
use bc2_convert::meshview::{Image, Index, Part};
use bc2_editor::actions::{Action, PanelKind};
use bc2_editor::app::{EditorApp, Tab};
use bc2_editor::headless::Headless;
use bc2_editor::preview::{self, Camera, Subject};
use bc2_editor::theme::ThemeId;
use serde_json::Value;

fn sha<T: le::Bytes>(v: &[T]) -> String {
    let mut b = Vec::new();
    for x in v {
        x.put(&mut b);
    }
    sha1_smol::Sha1::from(&b).digest().to_string()
}

/// Little-endian bytes of the values the recorder digests (`astype('<f4' / '<u4').tobytes()`).
mod le {
    pub trait Bytes {
        fn put(&self, out: &mut Vec<u8>);
    }
    impl Bytes for [f32; 3] {
        fn put(&self, out: &mut Vec<u8>) {
            for v in self {
                out.extend(v.to_le_bytes());
            }
        }
    }
    impl Bytes for [f32; 2] {
        fn put(&self, out: &mut Vec<u8>) {
            for v in self {
                out.extend(v.to_le_bytes());
            }
        }
    }
    impl Bytes for [u32; 3] {
        fn put(&self, out: &mut Vec<u8>) {
            for v in self {
                out.extend(v.to_le_bytes());
            }
        }
    }
}

fn recorded() -> Value {
    let f = std::fs::File::open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("data")
            .join("meshview_parity.json.gz"),
    )
    .unwrap();
    let mut s = String::new();
    flate2::read::GzDecoder::new(f).read_to_string(&mut s).unwrap();
    serde_json::from_str(&s).unwrap()
}

fn picture(img: &Image) -> Value {
    serde_json::json!([img.w, img.h, sha1_smol::Sha1::from(&img.rgb).digest().to_string()])
}

fn part_digest(p: &Part) -> Value {
    serde_json::json!({
        "name": p.name,
        "surface": p.surface,
        "texture": p.texture,
        "vertices": p.positions.len(),
        "triangles": p.indices.len(),
        "positions": sha(&p.positions),
        "normals": p.normals.as_ref().map(|n| sha(n)),
        "uvs": p.uvs.as_ref().map(|n| sha(n)),
        "indices": sha(&p.indices),
        "image": p.image.as_ref().map(|i| picture(i)),
    })
}

fn shots() -> Option<PathBuf> {
    std::env::var_os("BC2_EDITOR_SHOTS").map(PathBuf::from)
}

/// The recorded meshes read and drawn in Rust, compared.
fn parity() {
    let Ok(root) = bc2_editor::levels::find_opfor(None) else {
        println!("skipped: no game copy");
        return;
    };
    let t = std::time::Instant::now();
    let ix: std::sync::Arc<Index> = preview::index(&root).unwrap();
    println!("index: {} entries in {:.1} s", ix.len(), t.elapsed().as_secs_f64());
    let rec = recorded();
    let size = rec["size"].as_u64().unwrap() as usize;
    let dir = shots().map(|d| d.join("gr3_parity"));
    let (mut parts, mut pictures, mut rows) = (0, 0, 0);
    let mut differ = Vec::new();
    for row in rec["meshes"].as_array().unwrap() {
        let name = row["mesh"].as_str().unwrap();
        let lod = row["lod"].as_u64().unwrap() as usize;
        let t = std::time::Instant::now();
        let m = preview::read_model(&ix, &Subject::Mesh(name.to_owned()), Some(lod)).unwrap();
        assert_eq!(m.read.lod, lod, "{name}");
        let got: Vec<Value> = m.read.parts.iter().map(part_digest).collect();
        let want = row["parts"].as_array().unwrap();
        assert_eq!(got.len(), want.len(), "{name} LOD {lod}: part count");
        for (g, w) in got.iter().zip(want) {
            assert_eq!(g, w, "{name} LOD {lod}: a part differs from the mesh viewer's");
            parts += 1;
        }
        let refs: Vec<&Part> = m.read.parts.iter().collect();
        let off = stack_models(&[m.read.parts.as_slice()])[0];
        let want_off: Vec<f32> = row["offset"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap() as f32)
            .collect();
        assert_eq!(off.to_vec(), want_off, "{name} LOD {lod}: the layout's offset");
        let stem = format!("{}_lod{lod}", name.rsplit('/').next().unwrap());
        for v in View::ALL {
            let img = render(&[(refs.clone(), off)], size, v);
            if let Some(d) = &dir {
                preview::write_png(&d.join(format!("{stem}_{}.png", v.name())), &img).unwrap();
            }
            if picture(&img) == row["pictures"][v.name()] {
                pictures += 1;
            } else {
                differ.push(format!("{stem} {}", v.name()));
            }
        }
        rows += 1;
        println!(
            "{stem}: {} parts, {} tris, identical; read and drawn in {:.2} s",
            got.len(),
            m.triangles(),
            t.elapsed().as_secs_f64()
        );
    }
    {
        // two rifles stacked (stack_models of several)
        let st = &rec["stacked"];
        let models: Vec<preview::Model> = st["meshes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| preview::read_model(&ix, &Subject::Mesh(n.as_str().unwrap().to_owned()), Some(0)).unwrap())
            .collect();
        let slices: Vec<&[Part]> = models.iter().map(|m| m.read.parts.as_slice()).collect();
        let offs = stack_models(&slices);
        let want: Vec<Vec<f32>> = st["offsets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| {
                o.as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_f64().unwrap() as f32)
                    .collect()
            })
            .collect();
        assert_eq!(
            offs.iter().map(|o| o.to_vec()).collect::<Vec<_>>(),
            want,
            "the stacked offsets"
        );
        let laid: Vec<_> = models
            .iter()
            .zip(&offs)
            .map(|(m, o)| (m.read.parts.iter().collect::<Vec<_>>(), *o))
            .collect();
        let img = render(&laid, size, View::Side);
        if let Some(d) = &dir {
            preview::write_png(&d.join("stacked_side.png"), &img).unwrap();
        }
        if picture(&img) == st["side"] {
            pictures += 1;
        } else {
            differ.push("stacked side".into());
        }
    }
    println!(
        "gr3 parity: {rows} mesh LODs, {parts} parts identical; {pictures} pictures identical; differ: {differ:?}"
    );
    assert!(differ.is_empty(), "pictures differ from the mesh viewer's: {differ:?}");
}

/// About 3 s: the index from its cache, 18 levels of detail read (each bundle partly inflated) and drawn three ways.
#[test]
fn every_recorded_mesh_as_the_mesh_viewer_reads_and_draws_it() {
    let _gate = crate::gate();
    parity();
}

/// A box with one part, for the panel test.
fn boxed() -> preview::Model {
    let positions = (0..8)
        .map(|i: u32| std::array::from_fn(|a| if i >> a & 1 == 0 { -0.5 } else { 0.5 }))
        .collect();
    let quads = [
        [0, 1, 3, 2],
        [4, 6, 7, 5],
        [0, 4, 5, 1],
        [2, 3, 7, 6],
        [0, 2, 6, 4],
        [1, 5, 7, 3],
    ];
    let indices = quads
        .iter()
        .flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])
        .collect();
    let part = Part {
        name: "lod0|box_Wood".into(),
        surface: None,
        shader: None,
        positions,
        normals: Some(vec![[0.0, 1.0, 0.0]; 8]),
        uvs: None,
        indices,
        texture: None,
        image: None,
    };
    preview::Model {
        subject: Subject::Mesh("demo/box_mesh".into()),
        mesh: "demo/box_mesh".into(),
        lods: vec![0],
        read: bc2_convert::meshview::Mesh {
            name: "demo/box_mesh".into(),
            lod: 0,
            parts: vec![part],
            notes: vec![],
        },
        components: vec![],
        materials: vec![],
    }
}

#[test]
fn the_asset_preview_panel_draws_picks_and_hides() {
    let _gate = crate::gate();
    let Ok(mut h) = Headless::new([1600, 1000]) else {
        println!("skipped: no graphics adapter");
        return;
    };
    let dir = std::env::temp_dir().join(format!("bc2_editor_gr3_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let scene = bc2_editor::demo::level(&dir.join("level")).unwrap();
    let mut app = EditorApp::new(None, None).unwrap();
    h.settle(&mut app, 2);
    let gpu = h.gpu.clone();
    let ctx = h.ctx.clone();
    app.open_scene(&gpu, "demo", scene);
    h.settle(&mut app, 3);
    assert!(app.tabs().contains(&Tab::Preview), "{:?}", app.tabs());
    {
        let v = app.view_mut().unwrap();
        v.preview.subjects = vec![Subject::Mesh("demo/box_mesh".into())];
        v.preview.models = vec![boxed()];
        v.preview.revision += 1;
        v.preview.camera = Camera::Preset(View::Side);
    }
    app.run_action(&gpu, &ctx, Action::ShowPanel(PanelKind::Preview));
    h.settle(&mut app, 4);
    let rect = app
        .view()
        .unwrap()
        .preview_shown
        .rect
        .expect("the preview drew its picture");
    // the box fills the middle of the picture: a click there selects its part
    h.click(&mut app, rect.center());
    h.settle(&mut app, 3);
    println!(
        "picture at {rect:?}, now {:?}; tab {:?}",
        app.view().unwrap().preview_shown.rect,
        app.tabs()
    );
    assert_eq!(
        app.view().unwrap().preview.selected,
        Some((0, 0)),
        "the click picks the part"
    );
    if let Some(d) = shots() {
        h.save(&d.join("gr3_panel_demo_dark.png")).unwrap();
    }
    // the picture the panel shows, drawn again: the selected part's edges in the theme's view-selection (the panel
    // scales the picture into its rectangle, so its 1-pixel lines are not counted on screen)
    let sel = app.palette().view_selection;
    let colours = bc2_editor::app::preview_colours(&app.palette());
    assert_eq!(colours.selection, [sel.r(), sel.g(), sel.b()]);
    let pv = &mut app.view_mut().unwrap().preview;
    pv.camera = Camera::orbit();
    let (img, owner) = pv.draw(400, 300, colours);
    let orange = img
        .rgb
        .as_chunks::<3>()
        .0
        .iter()
        .filter(|c| **c == colours.selection)
        .count();
    assert!(
        orange > 50,
        "the selected part's edges in view-selection: {orange} pixels"
    );
    assert!(owner.iter().filter(|&&o| o != u32::MAX).count() > 1000);
    h.settle(&mut app, 2);
    // hidden, nothing of it is drawn: the bounds are gone
    app.view_mut()
        .unwrap()
        .preview
        .hidden
        .insert((0, "lod0|box_Wood".into()));
    h.settle(&mut app, 2);
    assert!(app.view().unwrap().preview.bounds_box().is_none());
    println!("gr3 panel: drawn in {rect:?}, part picked, {orange} selection pixels, hidden");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The asset preview on the game copy in Dark and Light (`BC2_EDITOR_SHOTS`): `MP_005`'s guard tower from Place
/// Objects with its edges and bounds, its destroyed state, and two rifles from the Meshes mode stacked, Side.
#[test]
fn asset_preview_screenshots() {
    let _gate = crate::gate();
    let Some(shots) = shots() else {
        println!("skipped: BC2_EDITOR_SHOTS is not set");
        return;
    };
    let Ok(root) = bc2_editor::levels::find_opfor(None) else {
        println!("skipped: no game copy");
        return;
    };
    if !crate::level_here("mp_005") {
        println!("skipped: MP_005 is not in this game copy");
        return;
    }
    let Ok(mut h) = Headless::new([1600, 1000]) else {
        println!("skipped: no graphics adapter");
        return;
    };
    let tmp = std::env::temp_dir().join(format!("bc2_gr3_shots_{}", std::process::id()));
    let mut app = EditorApp::new(None, None).unwrap();
    h.settle(&mut app, 2);
    let gpu = h.gpu.clone();
    let ctx = h.ctx.clone();
    app.open_level_blocking_then(
        &gpu,
        "mp_005",
        bc2_editor::app::Then::Create("Preview".into(), tmp.clone(), None),
    )
    .unwrap();
    // MP_005 offers the campaign strip checklist on a new project: closed for the pictures
    app.export.strip.open = false;
    app.run_action(&gpu, &ctx, Action::ShowPanel(PanelKind::Content));
    let save = |h: &mut Headless, name: &str| {
        let p = shots.join(name);
        h.save(&p).unwrap();
        println!("saved {}", p.display());
    };
    // a blueprint from Place Objects
    {
        let v = app.view_mut().unwrap();
        v.preview.show(
            &root,
            Subject::Blueprint("objects/buildings/guardtower/guardtower".into()),
        );
        v.preview.wait(std::time::Duration::from_mins(10));
        assert!(!v.preview.models.is_empty(), "{}", v.preview.status);
        v.preview.bounds = true;
        v.preview.wireframe = true;
        v.preview.selected = Some((0, 0));
        v.browser.content_pick = Some("objects/buildings/guardtower/guardtower".into());
    }
    app.run_action(&gpu, &ctx, Action::ShowPanel(PanelKind::Preview));
    for (theme, tag) in [(ThemeId::Dark, "dark"), (ThemeId::Light, "light")] {
        app.theme = theme;
        h.settle(&mut app, 5);
        save(&mut h, &format!("gr3_preview_guardtower_{tag}.png"));
    }
    {
        let v = app.view_mut().unwrap();
        v.preview.state = bc2_render::simple::State::Destroyed;
        v.preview.wireframe = false;
        v.preview.selected = None;
    }
    for (theme, tag) in [(ThemeId::Dark, "dark"), (ThemeId::Light, "light")] {
        app.theme = theme;
        h.settle(&mut app, 5);
        save(&mut h, &format!("gr3_preview_guardtower_destroyed_{tag}.png"));
    }
    // the Meshes mode: two rifles stacked, the mesh viewer's Side framing, the normal maps
    {
        let v = app.view_mut().unwrap();
        v.browser.browser_mode = bc2_editor::gameplay::BrowserMode::Meshes;
        v.preview_shown.mesh_query = "m1garand".into();
        v.preview.show(
            &root,
            Subject::Mesh("Objects/Weapons/Handheld/Beach/M1Garand/M1Garand_Mesh".into()),
        );
        v.preview.add(
            &root,
            Subject::Mesh("Objects/Weapons/Handheld/Beach/Jap_Type5/Jap_Type5_Mesh".into()),
        );
        v.preview.wait(std::time::Duration::from_mins(10));
        v.preview.camera = Camera::Preset(View::Side);
        v.preview.bounds = false;
    }
    for (theme, tag) in [(ThemeId::Dark, "dark"), (ThemeId::Light, "light")] {
        app.theme = theme;
        h.settle(&mut app, 5);
        save(&mut h, &format!("gr3_preview_meshes_rifles_{tag}.png"));
    }
    app.view_mut().unwrap().preview.channel = Some(bc2_convert::meshview::Channel::Normal);
    app.theme = ThemeId::Dark;
    h.settle(&mut app, 5);
    save(&mut h, "gr3_preview_rifles_normal_channel_dark.png");
    // Save picture of the Side preset in the Colour channel: the command line's picture (`meshview.py --png --size
    // 1200`), drawn by the renderer the parity test holds to the Python's pixels
    let pv = &mut app.view_mut().unwrap().preview;
    pv.channel = None;
    let file = shots.join("gr3_preview_save_picture_side.png");
    let (w, hh) = pv.save_picture(&file, 1200, preview::Colours::default()).unwrap();
    let slices: Vec<&[Part]> = pv.models.iter().map(|m| m.read.parts.as_slice()).collect();
    let laid: Vec<_> = pv
        .models
        .iter()
        .zip(stack_models(&slices))
        .map(|(m, o)| (m.read.parts.iter().collect::<Vec<_>>(), o))
        .collect();
    let want = render(&laid, 1200, View::Side);
    let (saved, _) = pv.draw(1200, 900, preview::Colours::default());
    assert_eq!((saved.w, saved.h), (w, hh));
    assert!(saved.rgb == want.rgb, "Save picture is the command line's picture");
    println!("saved {} ({w} x {hh}), the command line's picture", file.display());
    let _ = std::fs::remove_dir_all(&tmp);
}
