//! `bfbc2-editor`: the BFBC2 Tool Suite's map editor (round G1: the shell and the read-only map view; round E2:
//! editing a map project).
//!
//! ```text
//! bfbc2-editor [level | --project <file.mapedit.json>] [--opfor <folder>] [--theme dark|light]
//! bfbc2-editor [level | --demo | --project <file>] --screenshot out.png [--size 1600x950] [--select <guid>]
//! ```
//!
//! Without a level the start screen offers three paths (round G4): Open project, Edit existing level (the mode
//! cards - BFBC1, BFBC2, Vietnam, WW2 - then a mode's multiplayer maps with Open and New project; `--mode` shows one
//! mode's maps at once) and Create level (from any existing level, or from a height map). `--project` opens a map
//! project on its base level (a height-map project on its page of the start screen). `--screenshot` draws the
//! window without showing it (no display needed) and saves it.
#![deny(unsafe_code)]

use std::path::PathBuf;
use std::process::ExitCode;

use bc2_editor::app::{EditorApp, GpuCtx, Saved};
use bc2_editor::headless::Headless;
use bc2_editor::levels::ContentSet;
use bc2_editor::theme::ThemeId;
use eframe::egui;

const STORAGE_KEY: &str = "bfbc2-editor";

struct Args {
    level: Option<String>,
    opfor: Option<PathBuf>,
    theme: Option<ThemeId>,
    screenshot: Option<PathBuf>,
    size: [u32; 2],
    select: Option<String>,
    demo: bool,
    project: Option<PathBuf>,
    /// The start screen's set of maps to show (`--mode bfbc1|bfbc2|vietnam|ww2`).
    mode: Option<ContentSet>,
}

fn parse() -> Result<Args, String> {
    let mut a = Args {
        level: None,
        opfor: None,
        theme: None,
        screenshot: None,
        size: [1600, 950],
        select: None,
        demo: false,
        project: None,
        mode: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = |what: &str| it.next().ok_or_else(|| format!("{arg} needs {what}"));
        match arg.as_str() {
            "--opfor" => a.opfor = Some(PathBuf::from(value("a folder")?)),
            "--screenshot" => a.screenshot = Some(PathBuf::from(value("a file")?)),
            "--select" => a.select = Some(value("a GUID")?),
            "--demo" => a.demo = true,
            "--mode" => {
                a.mode = Some(match value("bfbc1, bfbc2, vietnam or ww2")?.to_lowercase().as_str() {
                    "bfbc1" => ContentSet::Bfbc1,
                    "vietnam" => ContentSet::Vietnam,
                    "ww2" => ContentSet::Ww2,
                    _ => ContentSet::Bfbc2,
                });
            }
            "--project" => a.project = Some(PathBuf::from(value("a .mapedit.json file")?)),
            "--theme" => {
                a.theme = Some(match value("dark or light")?.as_str() {
                    "light" => ThemeId::Light,
                    _ => ThemeId::Dark,
                });
            }
            "--size" => {
                let v = value("WIDTHxHEIGHT")?;
                let (w, h) = v.split_once('x').ok_or("--size takes WIDTHxHEIGHT")?;
                a.size = [
                    w.parse().map_err(|_| "--size takes WIDTHxHEIGHT")?,
                    h.parse().map_err(|_| "--size takes WIDTHxHEIGHT")?,
                ];
            }
            "-h" | "--help" => {
                return Err(
                    "bfbc2-editor [level | --project <file.mapedit.json>] [--opfor <folder>] [--theme dark|light] \
                     [--screenshot out.png [--size 1600x950] [--select <guid>] [--demo] [--mode bfbc1|bfbc2|vietnam|ww2]]"
                        .into(),
                );
            }
            _ if arg.starts_with("--") => return Err(format!("unknown option {arg}")),
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

fn screenshot(a: &Args, out: &std::path::Path) -> Result<(), String> {
    let mut h = Headless::new(a.size)?;
    let mut app = EditorApp::new(a.opfor.as_deref(), None).map_err(|e| e.to_string())?;
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
        let project = bc2_levelwrite::MapProject::load(p).map_err(|e| format!("{}: {e}", p.display()))?;
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
        self.app.ui(ui, &gpu);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, STORAGE_KEY, &self.app.saved());
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        self.app.palette().ground.to_normalized_gamma_f32()
    }
}

fn main() -> ExitCode {
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
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            wgpu_setup: setup.into(),
            ..Default::default()
        },
        viewport: egui::ViewportBuilder::default()
            .with_title("BFBC2 Map Editor")
            .with_inner_size([1600.0, 950.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };
    let (opfor, theme, open, project) = (a.opfor.clone(), a.theme, a.level.clone(), a.project.clone());
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
