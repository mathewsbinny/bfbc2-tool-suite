//! `bfbc2-editor`: the BFBC2 Tool Suite's map editor (round G1: the shell and the read-only map view; round E2:
//! editing a map project).
//!
//! ```text
//! bfbc2-editor [level | --project <file.mapedit.json>] [--opfor <folder>] [--theme dark|light] [--navmesh]
//! bfbc2-editor [level | --demo | --project <file>] --screenshot out.png [--size 1600x950] [--select <guid>]
//!              [--export [--install-plan] | --no-strip] [--new-project <level> [--era <era>] [--target-mode <mode>]]
//! bfbc2-editor --version
//! bfbc2-editor --source-hash        the Rust sources' fingerprint it was built from (bfbc2core.native)
//! ```
//!
//! Without a level the start screen offers three paths (round G4): Open project, Edit existing level (the mode
//! cards - BFBC1, BFBC2, Vietnam, WW2 - then a mode's multiplayer maps with Open and New project; `--mode` shows one
//! mode's maps at once) and Create level (from any existing level, or from a height map). `--project` opens a map
//! project on its base level (a height-map project on its page of the start screen). `--screenshot` draws the
//! window without showing it (no display needed) and saves it.
//! With `--screenshot`, `--export` shows the project's Export window (its content profiles read and its picture
//! previewed first, round E3b), `--no-strip` leaves out the campaign checklist a campaign base offers.
//! `--install-plan` (with `--export`) runs OPFOR's install plan on the project's stage and shows the Install dialog
//! (round E3d; nothing is installed). `--new-project <level>` shows the start screen's New project form on a level,
//! its Era and Mode pickers set by `--era bfbc1|bfbc2|vietnam|ww2` and `--target-mode <mode key>`.
//! `--navmesh` opens the level in the navmesh view mode: its navigation data and the Navigation panel (round GR2:
//! what the hub's "Open navigation" asks for, the Navigation Studio's successor).
#![deny(unsafe_code)]

use std::path::PathBuf;
use std::process::ExitCode;

use bc2_editor::app::{EditorApp, GpuCtx, Saved};
use bc2_editor::error::EditorError;
use bc2_editor::headless::Headless;
use bc2_editor::levels::ContentSet;
use bc2_editor::theme::ThemeId;
use eframe::egui;

const STORAGE_KEY: &str = "bfbc2-editor";

#[allow(clippy::struct_excessive_bools, reason = "the command line's switches")]
struct Args {
    level: Option<String>,
    opfor: Option<PathBuf>,
    theme: Option<ThemeId>,
    screenshot: Option<PathBuf>,
    size: [u32; 2],
    select: Option<String>,
    demo: bool,
    project: Option<PathBuf>,
    export: bool,
    no_strip: bool,
    install_plan: bool,
    new_project: Option<String>,
    era: Option<bc2_levelwrite::Profile>,
    target_mode: Option<String>,
    /// The start screen's set of maps to show (`--mode bfbc1|bfbc2|vietnam|ww2`).
    mode: Option<ContentSet>,
    /// Open the level in the navmesh view mode (`--navmesh`).
    navmesh: bool,
}

fn parse() -> Result<Args, EditorError> {
    let mut a = Args {
        level: None,
        opfor: None,
        theme: None,
        screenshot: None,
        size: [1600, 950],
        select: None,
        demo: false,
        project: None,
        export: false,
        no_strip: false,
        install_plan: false,
        new_project: None,
        era: None,
        target_mode: None,
        mode: None,
        navmesh: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = |what: &str| {
            it.next()
                .ok_or_else(|| EditorError::Input(format!("{arg} needs {what}")))
        };
        match arg.as_str() {
            "--opfor" => a.opfor = Some(PathBuf::from(value("a folder")?)),
            "--screenshot" => a.screenshot = Some(PathBuf::from(value("a file")?)),
            "--select" => a.select = Some(value("a GUID")?),
            "--demo" => a.demo = true,
            "--navmesh" => a.navmesh = true,
            "--mode" => {
                a.mode = Some(match value("bfbc1, bfbc2, vietnam or ww2")?.to_lowercase().as_str() {
                    "bfbc1" => ContentSet::Bfbc1,
                    "vietnam" => ContentSet::Vietnam,
                    "ww2" => ContentSet::Ww2,
                    _ => ContentSet::Bfbc2,
                });
            }
            "--project" => a.project = Some(PathBuf::from(value("a .mapedit.json file")?)),
            "--export" => a.export = true,
            "--no-strip" => a.no_strip = true,
            "--install-plan" => a.install_plan = true,
            "--new-project" => a.new_project = Some(value("a level")?),
            "--target-mode" => a.target_mode = Some(value("a mode")?),
            "--era" => {
                a.era = Some(match value("bfbc1, bfbc2, vietnam or ww2")?.to_lowercase().as_str() {
                    "bfbc1" => bc2_levelwrite::Profile::Bfbc1,
                    "vietnam" => bc2_levelwrite::Profile::Vietnam,
                    "ww2" => bc2_levelwrite::Profile::Ww2,
                    _ => bc2_levelwrite::Profile::Bfbc2,
                });
            }
            "--theme" => {
                a.theme = Some(match value("dark or light")?.as_str() {
                    "light" => ThemeId::Light,
                    _ => ThemeId::Dark,
                });
            }
            "--size" => {
                let v = value("WIDTHxHEIGHT")?;
                let (w, h) = v
                    .split_once('x')
                    .ok_or(EditorError::Input("--size takes WIDTHxHEIGHT".into()))?;
                a.size = [
                    w.parse()
                        .map_err(|_| EditorError::Input("--size takes WIDTHxHEIGHT".into()))?,
                    h.parse()
                        .map_err(|_| EditorError::Input("--size takes WIDTHxHEIGHT".into()))?,
                ];
            }
            "-h" | "--help" => {
                return Err(
                    EditorError::Input("bfbc2-editor [level | --project <file.mapedit.json>] [--opfor <folder>] [--theme dark|light] [--navmesh] \
                     [--screenshot out.png [--size 1600x950] [--select <guid>] [--demo] [--mode bfbc1|bfbc2|vietnam|ww2]]"
                        .into(),)
                );
            }
            _ if arg.starts_with("--") => return Err(EditorError::Input(format!("unknown option {arg}"))),
            _ => a.level = Some(arg),
        }
    }
    Ok(a)
}

fn select_guid(app: &mut EditorApp, gpu: &GpuCtx, guid: &str) {
    let g = guid.to_lowercase();
    let hit = app.view().and_then(|v| {
        v.scene()
            .entities
            .iter()
            .find(|e| e.guid.to_lowercase() == g)
            .map(|e| e.index)
    });
    app.select(gpu, hit);
}

fn screenshot(a: &Args, out: &std::path::Path) -> Result<(), EditorError> {
    let mut h = Headless::new(a.size)?;
    let mut app = EditorApp::new(a.opfor.as_deref(), None).map_err(|e| EditorError::Input(e.to_string()))?;
    if let Some(t) = a.theme {
        app.theme = t;
    }
    // the start screen's pictures first (read from the game copy, or their cache)
    app.wait_for_catalog(std::time::Duration::from_mins(2));
    if a.mode.is_some() {
        app.choose_mode(a.mode);
    }
    h.settle(&mut app, 2);
    if a.demo {
        app.open_scene(&h.gpu, "demo", bc2_editor::demo::scene());
    } else if let Some(p) = &a.project {
        let project =
            bc2_levelwrite::MapProject::load(p).map_err(|e| EditorError::Input(format!("{}: {e}", p.display())))?;
        if project.is_import() {
            // a height-map project: its page of the start screen
            app.open_project(p);
        } else {
            app.open_level_blocking_then(
                &h.gpu,
                &project.base.level,
                bc2_editor::app::Then::Open(p.clone(), None),
            )?;
        }
    } else if let Some(lv) = &a.level {
        app.open_level_blocking(&h.gpu, lv)?;
    }
    if a.no_strip || a.export {
        app.export.strip.open = false;
    }
    if a.export
        && let Some(p) = &a.project
    {
        let opfor = bc2_editor::levels::find_opfor(a.opfor.as_deref())?;
        let base = bc2_levelwrite::MapProject::load(p).map_or_else(|_| String::new(), |x| x.base.level);
        app.export.open_for(p, Some(&opfor));
        app.export.settle(&h.ctx, &base);
        if a.install_plan {
            h.settle(&mut app, 4);
            let stage = app
                .export
                .spec
                .as_ref()
                .and_then(|s| bc2_editor::export_ui::stage_of(&opfor, s));
            if let Some(st) = stage {
                app.export.install.lv = app
                    .export
                    .spec
                    .as_ref()
                    .and_then(|s| s.level_id().ok())
                    .map_or_else(String::new, |id| id.to_ascii_uppercase());
                app.export.install.plan(&opfor, &st);
                app.export.install.settle();
            }
        }
    }
    if let Some(lv) = &a.new_project {
        app.start_new_project(lv);
        app.read_era_modes();
        if a.era.is_some() || a.target_mode.is_some() {
            app.set_new_target(a.era, a.target_mode.as_deref().unwrap_or("conquest"));
        }
    }
    if a.navmesh && app.view().is_some() {
        let (gpu, ctx) = (h.gpu.clone(), h.ctx.clone());
        app.run_action(
            &gpu,
            &ctx,
            bc2_editor::actions::Action::SetViewMode(bc2_editor::workspace::ViewMode::Navmesh),
        );
        let p = app.palette();
        if let Some(v) = app.view_mut() {
            v.nav_wait(&gpu, &p);
        }
    }
    if let Some(g) = &a.select {
        let gpu = h.gpu.clone();
        select_guid(&mut app, &gpu, g);
        app.focus_selected();
    }
    h.settle(&mut app, 30);
    h.save(out)?;
    eprintln!("screenshot: {}", out.display());
    Ok(())
}

struct Window {
    app: EditorApp,
    gpu: Option<GpuCtx>,
    open: Option<String>,
    project: Option<PathBuf>,
    /// The navmesh view mode is still to be set once the level is open (`--navmesh`).
    navmesh: bool,
}

impl eframe::App for Window {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if self.gpu.is_none()
            && let Some(rs) = frame.wgpu_render_state()
        {
            let info = rs.adapter.get_info();
            self.gpu = Some(GpuCtx {
                gpu: bc2_editor::render::Gpu {
                    device: rs.device.clone(),
                    queue: rs.queue.clone(),
                },
                egui: rs.renderer.clone(),
                adapter: format!("{} ({:?})", info.name, info.backend),
            });
        }
        let Some(gpu) = self.gpu.clone() else {
            ui.label("No graphics device (wgpu).");
            return;
        };
        if let Some(lv) = self.open.take() {
            self.app.open_level(&lv);
        }
        if let Some(p) = self.project.take() {
            self.app.open_project(&p);
        }
        if self.navmesh && self.app.view().is_some() {
            self.navmesh = false;
            let ctx = ui.ctx().clone();
            self.app.run_action(
                &gpu,
                &ctx,
                bc2_editor::actions::Action::SetViewMode(bc2_editor::workspace::ViewMode::Navmesh),
            );
        }
        // round DF1 (audit AU4-DS-13): the window does not close under an install, an export or a navkit command
        // without asking
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.viewport().close_requested()) && !self.app.close_requested() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        self.app.ui(ui, &gpu);
        if self.app.closing_anyway() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, STORAGE_KEY, &self.app.saved());
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        self.app.palette().ground.to_normalized_gamma_f32()
    }
}

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--source-hash") {
        // the fingerprint of the Rust sources this binary was built from (`build_native.py --editor` passes it as
        // BC2_SOURCE_HASH; empty when built by cargo alone): `bfbc2core.native.binary` uses the binary only while
        // it is this checkout's (round QW1)
        use std::io::Write;
        let ok = writeln!(std::io::stdout(), "{}", option_env!("BC2_SOURCE_HASH").unwrap_or("")).is_ok();
        return if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE };
    }
    if std::env::args().nth(1).as_deref() == Some("--version") {
        // the suite's version (the VERSION file, round P1)
        use std::io::Write;
        let ok = writeln!(std::io::stdout(), "bfbc2-editor {}", bc2_editor::version()).is_ok();
        return if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE };
    }
    let a = match parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    if let Some(out) = &a.screenshot {
        return match screenshot(&a, out) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }
    // egui's device with the features the game-accurate view's shared frame needs, when the adapter has them
    let mut setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    let base = std::sync::Arc::clone(&setup.device_descriptor);
    setup.device_descriptor = std::sync::Arc::new(move |adapter| {
        let mut d = base(adapter);
        d.required_features |= bc2_editor::gameview::device_features(adapter);
        d
    });
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(format!("BFBC2 Map Editor {}", bc2_editor::version()))
        .with_inner_size([1600.0, 950.0])
        .with_min_inner_size([900.0, 560.0]);
    // the suite's application icon (round I1); none found, none set
    if let Some(icon) = bc2_editor::appicon::load() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            wgpu_setup: setup.into(),
            ..Default::default()
        },
        viewport,
        ..Default::default()
    };
    let (opfor, theme, open, project, navmesh) =
        (a.opfor.clone(), a.theme, a.level.clone(), a.project.clone(), a.navmesh);
    let r = eframe::run_native(
        "BFBC2 Map Editor",
        options,
        Box::new(move |cc| {
            let saved: Option<Saved> = cc.storage.and_then(|s| eframe::get_value(s, STORAGE_KEY));
            let mut app = EditorApp::new(opfor.as_deref(), saved)?;
            if let Some(t) = theme {
                app.theme = t;
            }
            Ok(Box::new(Window {
                app,
                gpu: None,
                open,
                project,
                navmesh,
            }))
        }),
    );
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("bfbc2-editor: {e}");
            ExitCode::FAILURE
        }
    }
}
