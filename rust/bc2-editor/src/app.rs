//! The map editor's window: the top bar (workspace switcher, the level, the content profile badge, the theme and
//! the layout), the start screen ("View a map"), the docked panels around the 3D view (Outliner left, Properties
//! right, Problems below; drag, dock, tab and float them, Reset layout returns to this), and the status bar.
//!
//! Round G1 is read-only: only the View workspace is active; Objects, Gameplay and AI are shown, disabled, with
//! what they wait for. The same [`EditorApp`] runs in the window (`main.rs`, eframe) and headless
//! (`headless.rs`: the tests and screenshots), so what the tests see is what the window shows.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Instant;

use bc2_scene::scene::{self, Scene};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Key, Layout, Margin, PointerButton, Pos2, Rect, RichText,
    Sense, Stroke, TextStyle, Vec2,
};
use eframe::egui_wgpu;
use eframe::epaint::mutex::RwLock;
use egui_dock::{DockArea, DockState, NodeIndex, TabViewer};

use crate::actions::{self, Action};
use crate::camera::{self, Camera, Projection};
use crate::factions::{Profile, Teams};
use crate::help::{self, HelpState};
use crate::levels::{self, LevelInfo};
use crate::model::{self, Kind, Outliner};
use crate::prepare::{self, Job, Outcome, Plan};
use crate::query;
use crate::render::{self, Gpu, SceneGpu, Target};
use crate::theme::{self, Metrics, Palette, ThemeId, Tokens};

/// The GPU and egui's renderer on it (the window's, or the headless runner's).
#[derive(Clone)]
pub struct GpuCtx {
    /// The device and queue.
    pub gpu: Gpu,
    /// egui's renderer, for the 3D view's texture.
    pub egui: Arc<RwLock<egui_wgpu::Renderer>>,
    /// The adapter, in words (status bar).
    pub adapter: String,
}

/// A docked panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Tab {
    /// A view of the map: the 3D view, or a top, front or side view; each has its own camera.
    Viewport(Projection),
    /// The map's contents by kind.
    Outliner,
    /// The selection's fields.
    Properties,
    /// Live validation (empty in G1; the level's load notes as information).
    Problems,
}

impl Tab {
    fn title(self) -> &'static str {
        match self {
            Self::Viewport(p) => p.name(),
            Self::Outliner => "Outliner",
            Self::Properties => "Properties",
            Self::Problems => "Problems",
        }
    }
}

/// The default layout: the 3D view in the middle, the outliner left, properties right, problems below the view.
#[must_use]
pub fn default_layout() -> DockState<Tab> {
    let mut d = DockState::new(vec![Tab::Viewport(Projection::Perspective)]);
    let s = d.main_surface_mut();
    let [centre, _] = s.split_left(NodeIndex::root(), 0.2, vec![Tab::Outliner]);
    let [centre, _] = s.split_right(centre, 0.72, vec![Tab::Properties]);
    let [_, _] = s.split_below(centre, 0.8, vec![Tab::Problems]);
    d
}

/// What the editor keeps between runs (eframe's storage): the theme, the layout, camera bookmarks per level.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Saved {
    /// Dark or Light.
    pub theme: ThemeId,
    /// The panels.
    pub dock: DockState<Tab>,
    /// Bookmarks 1-9 by level.
    pub bookmarks: HashMap<String, Vec<Option<Camera>>>,
}

/// An open level.
pub struct ViewState {
    /// The level (`mp_005`).
    pub lv: String,
    /// The scene.
    pub scene: Scene,
    gpu_scene: SceneGpu,
    /// The outliner's model.
    pub outliner: Outliner,
    /// The selected entity.
    pub selected: Option<usize>,
    /// The views of the map, by projection (each opened once, each with its own camera).
    pub viewports: HashMap<Projection, Viewport>,
    search: String,
    /// Notes from loading (no terrain unpack ...).
    pub notes: Vec<String>,
    load_seconds: f64,
    /// The selection changed in the view: the outliner shows its row.
    reveal: bool,
    /// Labels at spawns, flags and objectives.
    pub labels: bool,
    /// The surface point under the last click (the status bar's read-out).
    pub last_hit: Option<query::Hit>,
    /// Which faction each team is (the labels), from the level's data.
    pub teams: Teams,
}

/// One view of the map: its camera, its render target and the texture egui shows it through.
pub struct Viewport {
    /// Its camera.
    pub camera: Camera,
    target: Option<Target>,
    texture: Option<egui::TextureId>,
    dirty: bool,
    /// Its rectangle on screen, last frame.
    pub rect: Option<Rect>,
    fly: bool,
}

impl Viewport {
    fn new(camera: Camera) -> Self {
        Self {
            camera,
            target: None,
            texture: None,
            dirty: true,
            rect: None,
            fly: false,
        }
    }
}

type Loaded = Result<(Scene, Vec<String>, f64), String>;

/// What the top bar asks for: an action, or a page of the Help window.
enum TopBar {
    Run(Action),
    Help(help::Page),
}

enum Mode {
    Start,
    /// The level is being prepared (NavKit's prepare, the terrain unpack); it opens when that is done.
    Preparing(Box<Job>),
    Loading {
        lv: String,
        rx: Receiver<Loaded>,
        since: Instant,
    },
    View(Box<ViewState>),
}

/// The editor.
pub struct EditorApp {
    tokens: &'static Tokens,
    /// The current theme.
    pub theme: ThemeId,
    applied: Option<ThemeId>,
    opfor: Result<PathBuf, String>,
    levels: Vec<LevelInfo>,
    level_filter: String,
    dock: DockState<Tab>,
    mode: Mode,
    error: Option<String>,
    bookmarks: HashMap<String, Vec<Option<Camera>>>,
    renderer: Option<render::Renderer>,
    /// The command palette: open, its query, the highlighted row.
    palette_open: bool,
    palette_query: String,
    palette_row: usize,
    /// The Help window.
    pub help: HelpState,
}

impl EditorApp {
    /// The editor, its game copy found from `opfor` (or as the suite finds it), its saved state applied.
    ///
    /// # Errors
    ///
    /// tokens.json does not read (the build embeds it; the tests check it).
    pub fn new(opfor: Option<&std::path::Path>, saved: Option<Saved>) -> Result<Self, theme::TokenError> {
        let tokens = Tokens::embedded()?;
        let opfor = levels::find_opfor(opfor);
        let levels = opfor.as_ref().map_or_else(|_| Vec::new(), |r| levels::list_levels(r));
        let (theme, dock, bookmarks) = match saved {
            Some(s) => (s.theme, s.dock, s.bookmarks),
            None => (ThemeId::Dark, default_layout(), HashMap::new()),
        };
        Ok(Self {
            tokens,
            theme,
            applied: None,
            opfor,
            levels,
            level_filter: String::new(),
            dock,
            mode: Mode::Start,
            error: None,
            bookmarks,
            renderer: None,
            palette_open: false,
            palette_query: String::new(),
            palette_row: 0,
            help: HelpState::default(),
        })
    }

    /// Why an action cannot run now (for its disabled control's tooltip), or `None` when it can.
    #[must_use]
    pub fn why_not(&self, action: Action) -> Option<&'static str> {
        let open = matches!(self.mode, Mode::View(_));
        let selected = self.view().is_some_and(|v| v.selected.is_some());
        match action {
            Action::FocusSelection | Action::CopyGuid | Action::ClearSelection if !selected => {
                Some("Select an object first")
            }
            Action::OpenViewport(_) | Action::ToggleLabels | Action::CloseMap if !open => Some("Open a map first"),
            _ => None,
        }
    }

    /// Runs an action (the top bar, the keyboard and the command palette all come here).
    pub fn run(&mut self, gpu: &GpuCtx, ctx: &egui::Context, action: Action) {
        if self.why_not(action).is_some() {
            return;
        }
        match action {
            Action::CommandPalette => {
                self.palette_open = true;
                self.palette_query.clear();
                self.palette_row = 0;
            }
            Action::ResetLayout => self.dock = default_layout(),
            Action::ToggleTheme => {
                self.theme = match self.theme {
                    ThemeId::Dark => ThemeId::Light,
                    ThemeId::Light => ThemeId::Dark,
                };
            }
            Action::CloseMap => self.mode = Mode::Start,
            Action::OpenViewport(p) => self.open_viewport(p),
            Action::FocusSelection => self.focus_selected(),
            Action::ClearSelection => self.select(gpu, None),
            Action::CopyGuid => {
                if let Some(e) = self
                    .view()
                    .and_then(|v| v.selected.and_then(|k| v.scene.entities.get(k)))
                {
                    ctx.copy_text(e.guid.clone());
                }
            }
            Action::ToggleLabels => {
                if let Mode::View(v) = &mut self.mode {
                    v.labels = !v.labels;
                }
            }
            Action::Help => {
                let page = self.help.page;
                self.help.show(page);
            }
        }
    }

    /// Shows a viewport: its tab is made active when it is docked, else docked beside the 3D view.
    fn open_viewport(&mut self, p: Projection) {
        let tab = Tab::Viewport(p);
        if let Some(path) = self.dock.find_tab(&tab) {
            let _ = self.dock.set_active_tab(path);
            return;
        }
        match self.dock.find_tab(&Tab::Viewport(Projection::Perspective)) {
            Some(path) if path.surface == egui_dock::SurfaceIndex::main() => {
                let _ = self.dock.main_surface_mut().split_right(path.node, 0.5, vec![tab]);
            }
            _ => self.dock.push_to_focused_leaf(tab),
        }
    }

    /// The tabs docked now (tests).
    #[must_use]
    pub fn tabs(&self) -> Vec<Tab> {
        self.dock.iter_all_tabs().map(|(_, t)| *t).collect()
    }

    /// What to save.
    #[must_use]
    pub fn saved(&self) -> Saved {
        Saved {
            theme: self.theme,
            dock: self.dock.clone(),
            bookmarks: self.bookmarks.clone(),
        }
    }

    /// The levels of the game copy.
    #[must_use]
    pub fn levels(&self) -> &[LevelInfo] {
        &self.levels
    }

    /// The open level.
    #[must_use]
    pub fn view(&self) -> Option<&ViewState> {
        match &self.mode {
            Mode::View(v) => Some(v),
            _ => None,
        }
    }

    /// The open level, to change (tests: camera, selection).
    pub fn view_mut(&mut self) -> Option<&mut ViewState> {
        match &mut self.mode {
            Mode::View(v) => Some(v),
            _ => None,
        }
    }

    /// The error shown on the start screen, if any.
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// The palette of the current theme.
    #[must_use]
    pub fn palette(&self) -> Palette {
        Palette::new(self.tokens, self.theme)
    }

    /// Opens a level: prepares it first when it needs it (NavKit's prepare, the terrain unpack: a child process, with
    /// its progress shown), then loads it on a worker thread (the window stays usable).
    pub fn open_level(&mut self, lv: &str) {
        let Ok(root) = self.opfor.clone() else { return };
        self.error = None;
        match prepare::plan(&root, lv) {
            Plan::Ready => self.load_level(lv),
            plan => self.start_preparing(Job::start(&root, lv, plan)),
        }
    }

    /// [`EditorApp::open_level`] with another preparing script, whatever the level needs (the tests' stand-in).
    pub fn open_level_preparing_with(&mut self, script: &std::path::Path, lv: &str) {
        let Ok(root) = self.opfor.clone() else { return };
        self.error = None;
        self.start_preparing(Job::start_with(script, &root, lv, Plan::Full));
    }

    fn start_preparing(&mut self, job: Result<Job, String>) {
        match job {
            Ok(job) => self.mode = Mode::Preparing(Box::new(job)),
            Err(e) => {
                self.error = Some(e);
                self.mode = Mode::Start;
            }
        }
    }

    /// The preparation running now, if any (tests: its progress).
    #[must_use]
    pub fn preparing(&self) -> Option<&prepare::Progress> {
        match &self.mode {
            Mode::Preparing(job) => Some(&job.progress),
            _ => None,
        }
    }

    /// Whether a level is being loaded (after its preparation).
    #[must_use]
    pub fn is_loading(&self) -> bool {
        matches!(self.mode, Mode::Loading { .. })
    }

    fn load_level(&mut self, lv: &str) {
        let Ok(root) = self.opfor.clone() else { return };
        let (tx, rx) = std::sync::mpsc::channel();
        let name = lv.to_owned();
        std::thread::spawn(move || {
            let _ = tx.send(load(&root, &name));
        });
        self.error = None;
        self.mode = Mode::Loading {
            lv: lv.to_owned(),
            rx,
            since: Instant::now(),
        };
    }

    /// Loads a level and waits for it (headless runs).
    ///
    /// # Errors
    ///
    /// No game copy, or the level does not load.
    pub fn open_level_blocking(&mut self, gpu: &GpuCtx, lv: &str) -> Result<(), String> {
        let root = self.opfor.clone()?;
        let plan = prepare::plan(&root, lv);
        if plan != Plan::Ready {
            let mut job = Job::start(&root, lv, plan)?;
            if let Outcome::Failed(e) = job.wait(std::time::Duration::from_mins(15)) {
                return Err(e);
            }
        }
        let loaded = load(&root, lv)?;
        self.show_scene(gpu, lv, loaded);
        Ok(())
    }

    /// Shows a scene that is already loaded (the demo scene, tests).
    pub fn open_scene(&mut self, gpu: &GpuCtx, lv: &str, scene: Scene) {
        self.show_scene(gpu, lv, (scene, Vec::new(), 0.0));
    }

    fn show_scene(&mut self, gpu: &GpuCtx, lv: &str, (scene, notes, secs): (Scene, Vec<String>, f64)) {
        let palette = self.palette();
        let mut gpu_scene = SceneGpu::new(&gpu.gpu, &scene, &palette);
        for g in model::HIDDEN_AT_START {
            gpu_scene.set_hidden(&gpu.gpu, g, true);
        }
        let mut camera = Camera::default();
        if let Some((lo, hi)) = gpu_scene.bounds {
            camera.frame(lo, hi);
        }
        let teams = match scene.file(0) {
            Some((_, data)) => Teams::from_partition(data, Profile::of(lv)),
            None if lv == "demo" => crate::demo::teams(),
            None => Teams::unknown("The scene has no level partition; teams are shown as T1 and T2."),
        };
        self.mode = Mode::View(Box::new(ViewState {
            teams,
            lv: lv.to_owned(),
            outliner: Outliner::new(&scene),
            scene,
            gpu_scene,
            selected: None,
            viewports: HashMap::from([(Projection::Perspective, Viewport::new(camera))]),
            search: String::new(),
            notes,
            load_seconds: secs,
            reveal: false,
            labels: true,
            last_hit: None,
        }));
    }

    /// Selects an entity (or none), as a click would.
    pub fn select(&mut self, gpu: &GpuCtx, entity: Option<usize>) {
        if let Mode::View(v) = &mut self.mode {
            v.select(gpu, entity, true);
        }
    }

    /// Moves the camera to the selection, as F does.
    pub fn focus_selected(&mut self) {
        if let Mode::View(v) = &mut self.mode {
            v.focus_selected();
        }
    }

    /// Where an entity's position is on screen in the 3D view (points), with the last frame's view.
    #[must_use]
    pub fn project_entity(&self, entity: usize) -> Option<Pos2> {
        self.project_entity_in(Projection::Perspective, entity)
    }

    /// Where an entity's position is on screen in a viewport (points), with the last frame's view.
    #[must_use]
    pub fn project_entity_in(&self, projection: Projection, entity: usize) -> Option<Pos2> {
        let v = self.view()?;
        let vp = v.viewports.get(&projection)?;
        let rect = vp.rect?;
        let p = v.scene.entities.get(entity)?.position()?;
        let size = v.marker_offset(entity);
        let view = vp.camera.view([f64::from(rect.width()), f64::from(rect.height())]);
        let px = view.project([p[0], p[1] + size, p[2]])?;
        Some(rect.min + Vec2::new(px[0] as f32, px[1] as f32))
    }

    /// The window's contents for one frame.
    pub fn ui(&mut self, ui: &mut egui::Ui, gpu: &GpuCtx) {
        let ctx = ui.ctx().clone();
        if self.applied.is_none() {
            // fonts take effect from the next frame: this one only installs the theme
            theme::apply(&ctx, self.tokens, self.theme);
            self.applied = Some(self.theme);
            ctx.request_repaint();
            return;
        }
        if self.applied != Some(self.theme) {
            theme::apply(&ctx, self.tokens, self.theme);
            // this frame's Ui took its style before the theme was applied
            ui.set_style(self.tokens.style(self.theme));
            self.applied = Some(self.theme);
            if let Mode::View(v) = &mut self.mode {
                // the scene's colours follow the theme
                let palette = Palette::new(self.tokens, self.theme);
                let hidden: Vec<String> = v
                    .gpu_scene
                    .groups
                    .iter()
                    .filter(|g| v.gpu_scene.is_hidden(g))
                    .cloned()
                    .collect();
                v.gpu_scene = SceneGpu::new(&gpu.gpu, &v.scene, &palette);
                for g in &hidden {
                    v.gpu_scene.set_hidden(&gpu.gpu, g, true);
                }
                v.gpu_scene.set_selection(&gpu.gpu, v.selected);
                v.redraw();
            }
        }
        if self.renderer.is_none() {
            self.renderer = Some(render::Renderer::new(&gpu.gpu));
        }
        self.poll_loading(gpu, &ctx);
        self.shortcuts(gpu, &ctx);
        let palette = self.palette();
        if self.palette_open {
            self.command_palette(gpu, &ctx, &palette);
        }
        let metrics = Metrics::new(self.tokens);
        for asked in self.top_bar(ui, &palette, &metrics) {
            match asked {
                TopBar::Run(action) => self.run(gpu, &ctx, action),
                TopBar::Help(page) => self.help.show(page),
            }
        }
        self.status_bar(ui, &palette, &metrics, gpu);
        help::window(&ctx, &mut self.help, &palette, &metrics);
        let mut cancel = false;
        let Self {
            mode,
            dock,
            renderer,
            bookmarks,
            tokens,
            ..
        } = self;
        match mode {
            Mode::View(v) => {
                let Some(renderer) = renderer.as_ref() else { return };
                let style = theme::dock_style(tokens, self.theme, ui.style());
                let mut tabs = Tabs {
                    view: v,
                    gpu,
                    renderer,
                    palette,
                    metrics,
                    tokens,
                    bookmarks,
                };
                egui::CentralPanel::no_frame().show(ui, |ui| {
                    DockArea::new(dock)
                        .style(style)
                        .show_add_buttons(false)
                        .show_close_buttons(true)
                        .show_leaf_close_all_buttons(false)
                        .show_leaf_collapse_buttons(false)
                        .show_inside(ui, &mut tabs);
                });
            }
            Mode::Loading { lv, since, .. } => {
                let (lv, secs) = (lv.clone(), since.elapsed().as_secs_f64());
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(palette.ground))
                    .show(ui, |ui| {
                        ui.centered_and_justified(|ui| {
                            ui.label(
                                RichText::new(format!(
                                    "Opening {} - reading the level, its collision and terrain ({secs:.0} s)",
                                    lv.to_uppercase()
                                ))
                                .color(palette.ink_muted),
                            );
                        });
                    });
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
            Mode::Preparing(job) => {
                cancel = preparing_view(ui, job, &palette, &metrics);
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
            Mode::Start => self.start_screen(ui, &palette, &metrics),
        }
        if cancel && let Mode::Preparing(job) = &mut self.mode {
            job.cancel();
            self.mode = Mode::Start;
        }
    }

    fn poll_loading(&mut self, gpu: &GpuCtx, ctx: &egui::Context) {
        if let Mode::Preparing(job) = &mut self.mode {
            match job.poll() {
                Outcome::Running => {}
                Outcome::Done => {
                    let lv = job.lv.clone();
                    if let Ok(root) = &self.opfor {
                        // listed as prepared from here on
                        self.levels = levels::list_levels(root);
                    }
                    self.load_level(&lv);
                    ctx.request_repaint();
                }
                Outcome::Failed(e) => {
                    self.error = Some(e);
                    self.mode = Mode::Start;
                    // the start screen's rows say again which levels are prepared
                    if let Ok(root) = &self.opfor {
                        self.levels = levels::list_levels(root);
                    }
                }
            }
            return;
        }
        let Mode::Loading { lv, rx, .. } = &self.mode else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(loaded)) => {
                let lv = lv.clone();
                self.show_scene(gpu, &lv, loaded);
                if let Ok(root) = &self.opfor {
                    // a level prepared just now is listed as ready from here on
                    self.levels = levels::list_levels(root);
                }
                ctx.request_repaint();
            }
            Ok(Err(e)) => {
                self.error = Some(e);
                self.mode = Mode::Start;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.error = Some("The level's loader stopped unexpectedly.".into());
                self.mode = Mode::Start;
            }
        }
    }

    /// The registry's shortcuts, while no text field has the keyboard (the palette's keys are its own).
    fn shortcuts(&mut self, gpu: &GpuCtx, ctx: &egui::Context) {
        if self.palette_open || ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        for e in actions::registry() {
            let Some(sc) = e.shortcut else { continue };
            if self.why_not(e.action).is_none() && ctx.input_mut(|i| i.consume_shortcut(&sc)) {
                self.run(gpu, ctx, e.action);
            }
        }
    }

    /// The command palette: type to find an action, Up / Down to choose, Enter to run, Esc to close.
    fn command_palette(&mut self, gpu: &GpuCtx, ctx: &egui::Context, p: &Palette) {
        let hits = actions::search(&self.palette_query);
        let (up, down, enter, escape) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, Key::Enter),
                i.consume_key(egui::Modifiers::NONE, Key::Escape),
            )
        });
        if down {
            self.palette_row = (self.palette_row + 1).min(hits.len().saturating_sub(1));
        }
        if up {
            self.palette_row = self.palette_row.saturating_sub(1);
        }
        self.palette_row = self.palette_row.min(hits.len().saturating_sub(1));
        let mut chosen = if enter {
            hits.get(self.palette_row).map(|e| e.action)
        } else {
            None
        };
        let width = 520.0;
        let modal = egui::Modal::new(egui::Id::new("command palette"))
            .frame(
                egui::Frame::new()
                    .fill(p.surface_2)
                    .stroke(Stroke::new(1.0, p.line))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::same(8)),
            )
            .show(ctx, |ui| {
                ui.set_width(width);
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut self.palette_query)
                        .hint_text(RichText::new("Type a command").color(p.ink_faint))
                        .desired_width(f32::INFINITY),
                );
                edit.request_focus();
                if edit.changed() {
                    self.palette_row = 0;
                }
                ui.add_space(4.0);
                for (k, e) in hits.iter().enumerate() {
                    let why = self.why_not(e.action);
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 26.0), Sense::click());
                    if k == self.palette_row {
                        ui.painter().rect_filled(rect, CornerRadius::same(2), p.accent_subtle);
                    } else if resp.hovered() {
                        ui.painter().rect_filled(rect, CornerRadius::same(2), p.surface_3);
                    }
                    let ink = if why.is_some() { p.ink_faint } else { p.ink };
                    ui.painter().text(
                        rect.left_center() + Vec2::new(8.0, 0.0),
                        Align2::LEFT_CENTER,
                        e.name,
                        FontId::proportional(13.0),
                        ink,
                    );
                    let right = match (why, e.shortcut) {
                        (Some(w), _) => w.to_owned(),
                        (None, Some(sc)) => ctx.format_shortcut(&sc),
                        (None, None) => String::new(),
                    };
                    ui.painter().text(
                        rect.right_center() - Vec2::new(8.0, 0.0),
                        Align2::RIGHT_CENTER,
                        right,
                        FontId::monospace(12.0),
                        p.ink_muted,
                    );
                    if resp.on_hover_text(e.help).clicked() {
                        chosen = Some(e.action);
                    }
                }
                if hits.is_empty() {
                    ui.label(RichText::new("No command matches.").color(p.ink_muted));
                }
            });
        if escape || modal.should_close() {
            self.palette_open = false;
        }
        if let Some(a) = chosen {
            self.palette_open = false;
            if a != Action::CommandPalette {
                self.run(gpu, ctx, a);
            }
        }
    }

    /// Opens the command palette with a query (tests).
    pub fn open_palette(&mut self, query: &str) {
        self.palette_open = true;
        query.clone_into(&mut self.palette_query);
        self.palette_row = 0;
    }

    /// Whether the command palette is open.
    #[must_use]
    pub fn palette_is_open(&self) -> bool {
        self.palette_open
    }

    /// The top bar; what its buttons and its Help menu ask for.
    fn top_bar(&self, ui: &mut egui::Ui, p: &Palette, m: &Metrics) -> Vec<TopBar> {
        let mut run = Vec::new();
        let frame = egui::Frame::new()
            .fill(p.surface_2)
            .stroke(Stroke::new(1.0, p.line))
            .inner_margin(Margin::symmetric(m.space_3 as i8, 0));
        egui::Panel::top("top bar")
            .exact_size(m.toolbar)
            .frame(frame)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = m.space_2;
                    ui.label(
                        RichText::new("BFBC2 Map Editor")
                            .text_style(style("title"))
                            .color(p.ink),
                    );
                    separator(ui, p, m);
                    // the workspace switcher (help::WORKSPACES, as the Help window describes them): only View in
                    // this round
                    for w in help::WORKSPACES {
                        let why = if w.available {
                            String::new()
                        } else {
                            format!("{} {}", w.what, w.when)
                        };
                        segment(ui, p, m, w.name, w.available, &why);
                    }
                    separator(ui, p, m);
                    if let Mode::View(v) = &self.mode {
                        ui.label(
                            RichText::new(v.scene.name.clone())
                                .text_style(style("body-strong"))
                                .color(p.ink),
                        );
                        ui.label(RichText::new("read-only").text_style(style("small")).color(p.ink_muted));
                        badge(ui, p, m, profile_of(&v.lv));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // right to left: Help last on screen, then the theme
                        let help_entry = actions::entry(Action::Help);
                        ui.menu_button(RichText::new("Help").color(p.ink_muted), |ui| {
                            for page in help::Page::ALL {
                                if ui.button(page.name()).clicked() {
                                    run.push(TopBar::Help(page));
                                    ui.close();
                                }
                            }
                            ui.separator();
                            if ui.button("Command palette").clicked() {
                                run.push(TopBar::Run(Action::CommandPalette));
                                ui.close();
                            }
                        })
                        .response
                        .on_hover_text(help_entry.map_or("", |e| e.help).to_owned() + " (F1)");
                        let theme_label = match self.theme {
                            ThemeId::Dark => "Light theme",
                            ThemeId::Light => "Dark theme",
                        };
                        let mut buttons: Vec<(Action, &str)> = vec![
                            (Action::ToggleTheme, theme_label),
                            (Action::ResetLayout, "Reset layout"),
                            (Action::CommandPalette, "Commands"),
                        ];
                        if matches!(self.mode, Mode::View(_)) {
                            buttons.push((Action::CloseMap, "Close map"));
                            for (proj, name) in [
                                (Projection::Side, "Side"),
                                (Projection::Front, "Front"),
                                (Projection::Top, "Top"),
                            ] {
                                buttons.push((Action::OpenViewport(proj), name));
                            }
                        }
                        for (action, label) in buttons {
                            if action_button(ui, p, action, label, self.why_not(action)).clicked() {
                                run.push(TopBar::Run(action));
                            }
                        }
                    });
                });
            });
        run
    }

    fn status_bar(&self, ui: &mut egui::Ui, p: &Palette, m: &Metrics, gpu: &GpuCtx) {
        let frame = egui::Frame::new()
            .fill(p.surface_2)
            .stroke(Stroke::new(1.0, p.line))
            .inner_margin(Margin::symmetric(m.space_3 as i8, 0));
        egui::Panel::bottom("status bar")
            .exact_size(m.row)
            .frame(frame)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    let small = |t: String| RichText::new(t).text_style(style("small")).color(p.ink_muted);
                    match &self.mode {
                        Mode::View(v) => {
                            ui.label(small(format!(
                                "{}  -  {} entities  -  {} triangles  -  loaded in {:.1} s",
                                v.scene.name,
                                group_digits(v.scene.entities.len() as u64),
                                group_digits(v.gpu_scene.triangles),
                                v.load_seconds
                            )));
                            let fly = v.viewports.values().any(|vp| vp.fly);
                            ui.label(small(format!(
                                "camera: {}",
                                if fly { "fly (right button held)" } else { "orbit" }
                            )));
                            if let Some(h) = &v.last_hit {
                                let what = match h.entity.and_then(|k| v.scene.entities.get(k)) {
                                    Some(e) => model::display_name(e),
                                    None => "terrain".into(),
                                };
                                ui.label(small(format!(
                                    "surface: {:.2}, {:.2}, {:.2} m ({what})",
                                    h.point[0], h.point[1], h.point[2]
                                )));
                            }
                        }
                        Mode::Loading { lv, .. } => {
                            ui.label(small(format!("opening {lv}")));
                        }
                        Mode::Preparing(job) => {
                            ui.label(small(format!("preparing {} for the editor", job.lv)));
                        }
                        Mode::Start => {
                            let where_ = match &self.opfor {
                                Ok(r) => format!("game copy: {}", r.display()),
                                Err(_) => "no game copy found".into(),
                            };
                            ui.label(small(where_));
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(small(gpu.adapter.clone()));
                    });
                });
            });
    }

    fn start_screen(&mut self, ui: &mut egui::Ui, p: &Palette, m: &Metrics) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(p.ground)
                    .inner_margin(Margin::same(m.space_8 as i8)),
            )
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.set_max_width(720.0);
                    ui.label(
                        RichText::new("BFBC2 Map Editor")
                            .text_style(style("display"))
                            .color(p.ink),
                    );
                    ui.add_space(m.space_1);
                    ui.label(
                        RichText::new("View any map of your game copy read-only. Editing comes in later rounds.")
                            .color(p.ink_muted),
                    );
                    ui.add_space(m.space_6);
                    ui.horizontal(|ui| {
                        for (name, why) in [
                            ("New map from an existing map", "Comes with map projects (round E2)."),
                            ("Open a project", "Comes with map projects (round E2)."),
                        ] {
                            ui.add_enabled(false, egui::Button::new(name).min_size(Vec2::new(0.0, m.control)))
                                .on_disabled_hover_text(why);
                        }
                    });
                    ui.add_space(m.space_6);
                    ui.label(RichText::new("View a map").text_style(style("section")).color(p.ink));
                    ui.add_space(m.space_2);
                    if let Some(e) = &self.error {
                        status_line(ui, p, "Error", p.status_error, e);
                        ui.add_space(m.space_2);
                    }
                    if let Err(e) = &self.opfor {
                        status_line(ui, p, "Error", p.status_error, e);
                    } else {
                        {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.level_filter)
                                    .hint_text(RichText::new("Filter levels (mp_005, sp_01 ...)").color(p.ink_faint))
                                    .desired_width(320.0),
                            );
                            ui.add_space(m.space_2);
                            let mut open = None;
                            let f = self.level_filter.trim().to_lowercase();
                            egui::Frame::new()
                                .fill(p.surface_1)
                                .stroke(Stroke::new(1.0, p.line))
                                .corner_radius(CornerRadius::same(m.radius_md as u8))
                                .show(ui, |ui| {
                                    egui::ScrollArea::vertical()
                                        .max_height(ui.available_height() - m.space_4)
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            for l in self.levels.iter().filter(|l| f.is_empty() || l.lv.contains(&f)) {
                                                if level_row(ui, p, m, l).clicked() {
                                                    open = Some(l.lv.clone());
                                                }
                                            }
                                        });
                                });
                            if let Some(lv) = open {
                                self.open_level(&lv);
                            }
                        }
                    }
                });
            });
    }
}

fn load(root: &std::path::Path, lv: &str) -> Loaded {
    let t = Instant::now();
    let found = levels::load_options(root, lv)?;
    let s = scene::load(&found.options).map_err(|e| format!("{lv} could not be read: {e}"))?;
    let mut notes = found.notes;
    notes.extend(s.notes.iter().cloned());
    Ok((s, notes, t.elapsed().as_secs_f64()))
}

/// The content profile of a level, for the badge.
#[must_use]
pub fn profile_of(lv: &str) -> &'static str {
    Profile::of(lv).badge()
}

fn style(name: &str) -> TextStyle {
    TextStyle::Name(name.into())
}

fn group_digits(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (k, c) in s.chars().enumerate() {
        if k > 0 && (s.len() - k).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A `label` caption: capitals, 11px semibold, its letter spacing.
fn caption(text: &str, color: Color32) -> egui::text::LayoutJob {
    let t = Tokens::embedded().map(|t| t.text("label")).ok();
    let (size, spacing) = t.as_ref().map_or((11.0, 0.0), |t| (t.size, t.letter_spacing * t.size));
    egui::text::LayoutJob::single_section(
        text.to_uppercase(),
        egui::TextFormat {
            font_id: t.map_or_else(|| FontId::proportional(size), |t| t.font_id()),
            color,
            extra_letter_spacing: spacing,
            ..Default::default()
        },
    )
}

fn separator(ui: &mut egui::Ui, p: &Palette, m: &Metrics) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(1.0, m.control), Sense::hover());
    ui.painter().rect_filled(r, CornerRadius::ZERO, p.line);
}

/// A segment of the workspace switcher: active `accent-subtle` with an `accent` underline; disabled says why.
fn segment(ui: &mut egui::Ui, p: &Palette, m: &Metrics, name: &str, active: bool, why: &str) {
    let galley = ui.painter().layout_no_wrap(
        name.to_owned(),
        FontId::new(13.0, egui::FontFamily::Name(crate::fonts::SANS_SEMIBOLD.into())),
        if active { p.ink } else { p.ink_faint },
    );
    let size = Vec2::new(galley.size().x + 2.0 * m.space_3, m.control);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    if active {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(m.radius_md as u8), p.accent_subtle);
        let u = Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - 2.0), rect.right_bottom());
        ui.painter().rect_filled(u, CornerRadius::ZERO, p.accent);
    }
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, p.ink);
    if !why.is_empty() {
        resp.on_hover_text(why);
    }
}

/// A badge: `accent` outline and text (the content profile).
fn badge(ui: &mut egui::Ui, p: &Palette, m: &Metrics, text: &str) {
    let galley = ui.painter().layout_job(caption(text, p.accent_text));
    let size = galley.size() + Vec2::new(2.0 * m.space_2, m.space_1);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(m.radius_sm as u8),
        Stroke::new(1.0, p.accent_text),
        egui::StrokeKind::Inside,
    );
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, p.accent_text);
}

/// A ghost button (no fill, `ink-muted` text) running a registry action: its tooltip says what it does and its
/// shortcut, or why it cannot run.
fn action_button(ui: &mut egui::Ui, p: &Palette, action: Action, label: &str, why_not: Option<&str>) -> egui::Response {
    let entry = actions::entry(action);
    let tip = match (why_not, entry) {
        (Some(w), _) => w.to_owned(),
        (None, Some(e)) => match e.shortcut {
            Some(sc) => format!("{} ({})", e.help, ui.ctx().format_shortcut(&sc)),
            None => e.help.to_owned(),
        },
        (None, None) => String::new(),
    };
    let resp = ui.add_enabled(
        why_not.is_none(),
        egui::Button::new(RichText::new(label).color(p.ink_muted)).frame(false),
    );
    if why_not.is_some() {
        resp.on_disabled_hover_text(tip)
    } else {
        resp.on_hover_text(tip)
    }
}

/// The preparing view: the level, its step of how many, a bar (`accent` on `surface-2`; never a spinner alone),
/// the time so far and the last thing the tools said, on a dialog card. Whether Cancel was pressed.
fn preparing_view(ui: &mut egui::Ui, job: &Job, p: &Palette, m: &Metrics) -> bool {
    let mut cancel = false;
    let pr = &job.progress;
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(p.ground))
        .show(ui, |ui| {
            let width = 520.0_f32.min(ui.available_width() - 2.0 * m.space_6);
            let card_h = 236.0;
            let top = ui.max_rect().center().y - card_h / 2.0;
            let rect = Rect::from_min_size(
                Pos2::new(ui.max_rect().center().x - width / 2.0, top),
                Vec2::new(width, card_h),
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                egui::Frame::new()
                    .fill(p.surface_1)
                    .stroke(Stroke::new(1.0, p.line))
                    .corner_radius(CornerRadius::same(m.radius_lg as u8))
                    .inner_margin(Margin::same(m.space_6 as i8))
                    .show(ui, |ui| {
                        ui.set_width(width - 2.0 * m.space_6);
                        ui.label(
                            RichText::new(format!("Preparing {}", job.lv.to_uppercase()))
                                .text_style(style("section"))
                                .color(p.ink),
                        );
                        ui.add_space(m.space_1);
                        let step = if pr.steps == 0 {
                            "Starting the suite's tools".to_owned()
                        } else {
                            format!("Step {} of {}: {}", pr.step, pr.steps, pr.what)
                        };
                        ui.label(RichText::new(step).color(p.ink));
                        ui.add_space(m.space_3);
                        let (bar, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 6.0), Sense::hover());
                        ui.painter()
                            .rect_filled(bar, CornerRadius::same(m.radius_sm as u8), p.surface_2);
                        let mut done = bar;
                        done.set_width(bar.width() * pr.fraction());
                        ui.painter()
                            .rect_filled(done, CornerRadius::same(m.radius_sm as u8), p.accent);
                        ui.add_space(m.space_2);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{} s", job.since.elapsed().as_secs()))
                                    .text_style(style("value"))
                                    .color(p.ink_muted),
                            );
                            ui.add(
                                egui::Label::new(RichText::new(&pr.last).text_style(style("log")).color(p.ink_muted))
                                    .truncate(),
                            );
                        });
                        ui.add_space(m.space_3);
                        ui.add(
                            egui::Label::new(
                                RichText::new(
                                    "Once per level: its bundles are unpacked and its objects, collision and terrain \
                                     read into Project OPFOR BFBC2's work folder. The game copy is not changed. The \
                                     map opens when this is done.",
                                )
                                .text_style(style("small"))
                                .color(p.ink_muted),
                            )
                            .wrap(),
                        );
                        ui.add_space(m.space_3);
                        if ui
                            .add(egui::Button::new("Cancel").min_size(Vec2::new(0.0, m.control)))
                            .on_hover_text("Stops preparing; the level stays unprepared")
                            .clicked()
                        {
                            cancel = true;
                        }
                    });
            });
        });
    cancel
}

/// A severity with its word and colour, then the text.
fn status_line(ui: &mut egui::Ui, p: &Palette, word: &str, colour: Color32, text: &str) {
    ui.horizontal_wrapped(|ui| {
        // the status shape, drawn (IBM Plex has no octagon glyph: G1's showed a '?'), then the word
        let (r, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
        status_shape(ui.painter(), r.center(), 5.5, word, colour);
        ui.label(RichText::new(word).text_style(style("body-strong")).color(colour));
        ui.label(RichText::new(text).color(p.ink));
    });
}

/// A status shape (the design's fixed set): octagon for an error, triangle for a warning, circle otherwise.
fn status_shape(painter: &egui::Painter, c: Pos2, r: f32, word: &str, colour: Color32) {
    match word {
        "Error" => {
            let pts = (0..8)
                .map(|k| {
                    let a = (k as f32 + 0.5) * std::f32::consts::TAU / 8.0;
                    c + Vec2::new(a.cos(), a.sin()) * r
                })
                .collect();
            painter.add(egui::Shape::convex_polygon(pts, colour, Stroke::NONE));
        }
        "Warning" => {
            let pts = vec![
                c + Vec2::new(0.0, -r),
                c + Vec2::new(r, r * 0.8),
                c + Vec2::new(-r, r * 0.8),
            ];
            painter.add(egui::Shape::convex_polygon(pts, colour, Stroke::NONE));
        }
        _ => {
            painter.circle_filled(c, r * 0.85, colour);
        }
    }
}

fn level_row(ui: &mut egui::Ui, p: &Palette, m: &Metrics, l: &LevelInfo) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), m.row + m.space_1), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::ZERO, p.surface_2);
    }
    let x = rect.left() + m.space_3;
    ui.painter().text(
        Pos2::new(x, rect.center().y),
        Align2::LEFT_CENTER,
        l.lv.to_uppercase(),
        FontId::monospace(12.0),
        p.ink,
    );
    let state = if !l.prepared {
        "Not prepared yet - opening it prepares it first (up to a minute)"
    } else if !l.terrain_current {
        "Ready - opening it unpacks its terrain first"
    } else {
        "Ready"
    };
    ui.painter().text(
        Pos2::new(x + 140.0, rect.center().y),
        Align2::LEFT_CENTER,
        state,
        FontId::proportional(12.0),
        p.ink_muted,
    );
    ui.painter().text(
        Pos2::new(rect.right() - m.space_3, rect.center().y),
        Align2::RIGHT_CENTER,
        profile_of(&l.lv),
        FontId::proportional(11.0),
        p.ink_muted,
    );
    ui.painter()
        .hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, p.line));
    resp.on_hover_text(if l.prepared {
        "Open read-only"
    } else {
        "Prepare it, then open it read-only"
    })
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

impl ViewState {
    fn select(&mut self, gpu: &GpuCtx, entity: Option<usize>, reveal: bool) {
        if self.selected != entity {
            self.selected = entity;
            self.gpu_scene.set_selection(&gpu.gpu, entity);
            self.redraw();
            self.reveal = reveal;
        }
    }

    /// Every viewport draws again.
    fn redraw(&mut self) {
        for vp in self.viewports.values_mut() {
            vp.dirty = true;
        }
    }

    /// Every viewport looks at the selection (the 3D view from a distance that fits it, the orthographic views
    /// centred on it at a zoom that fits it).
    fn focus_selected(&mut self) {
        let Some(e) = self.selected.and_then(|k| self.scene.entities.get(k)) else {
            return;
        };
        if let Some(p) = e.position() {
            let size = self.gpu_scene.sizes.get(e.index).copied().unwrap_or(2.0);
            for vp in self.viewports.values_mut() {
                vp.camera.focus(p, Some(camera::focus_distance(size)));
                vp.dirty = true;
            }
        }
    }

    /// The 3D view's rectangle on screen, last frame.
    #[must_use]
    pub fn view_rect(&self) -> Option<Rect> {
        self.viewports.get(&Projection::Perspective).and_then(|vp| vp.rect)
    }

    /// How far above its position an entity's marker's middle is (markers stand on their position).
    fn marker_offset(&self, entity: usize) -> f64 {
        let Some(e) = self.scene.entities.get(entity) else {
            return 0.0;
        };
        let has_mesh = self
            .scene
            .collision
            .as_ref()
            .is_some_and(|c| c.meshes.iter().any(|(k, _)| *k == entity));
        if has_mesh {
            0.0
        } else {
            render::marker_size(&e.category).map_or(0.0, |s| f64::from(s) / 2.0)
        }
    }
}

struct Tabs<'a> {
    view: &'a mut ViewState,
    gpu: &'a GpuCtx,
    renderer: &'a render::Renderer,
    palette: Palette,
    metrics: Metrics,
    tokens: &'a Tokens,
    bookmarks: &'a mut HashMap<String, Vec<Option<Camera>>>,
}

impl TabViewer for Tabs<'_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(("editor tab", *tab))
    }

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        caption(tab.title(), self.palette.ink).into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        match tab {
            Tab::Viewport(p) => self.viewport(ui, *p),
            Tab::Outliner => self.outliner(ui),
            Tab::Properties => self.properties(ui),
            Tab::Problems => self.problems(ui),
        }
    }

    /// The orthographic views can be closed (the actions open them again); the rest stay.
    fn is_closeable(&self, tab: &Tab) -> bool {
        matches!(tab, Tab::Viewport(p) if *p != Projection::Perspective)
    }

    fn scroll_bars(&self, _tab: &Tab) -> [bool; 2] {
        [false, false]
    }
}

impl Tabs<'_> {
    fn viewport(&mut self, ui: &mut egui::Ui, projection: Projection) {
        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        // a viewport opened for the first time looks where the 3D view looks
        let mut vp = self.view.viewports.remove(&projection).unwrap_or_else(|| {
            let base = self
                .view
                .viewports
                .get(&Projection::Perspective)
                .map_or_else(Camera::default, |p| p.camera);
            Viewport::new(base.with_projection(projection))
        });
        vp.rect = Some(rect);
        let ppp = ui.ctx().pixels_per_point();
        let px = [
            (rect.width() * ppp).round().max(1.0) as u32,
            (rect.height() * ppp).round().max(1.0) as u32,
        ];
        let size = [f64::from(px[0]), f64::from(px[1])];
        let ortho = projection != Projection::Perspective;
        // input: orbit (left drag; pans in the orthographic views), pan (middle drag), zoom (wheel),
        // fly (right button held: look, W A S D, Q E; the 3D view only)
        let typing = ui.ctx().memory(|mem| mem.focused().is_some());
        let (delta, scroll, keys, secondary, dt, shift) = ui.input(|i| {
            let k = |key: Key| if i.key_down(key) { 1.0 } else { 0.0 };
            (
                i.pointer.delta(),
                i.smooth_scroll_delta.y,
                [k(Key::W) - k(Key::S), k(Key::D) - k(Key::A), k(Key::E) - k(Key::Q)],
                i.pointer.button_down(PointerButton::Secondary),
                f64::from(i.stable_dt.min(0.1)),
                i.modifiers.shift,
            )
        });
        let h = f64::from(rect.height());
        if resp.dragged_by(PointerButton::Primary) {
            let d = resp.drag_delta();
            if ortho {
                vp.camera.pan(f64::from(d.x), f64::from(d.y), h);
            } else {
                vp.camera.orbit(f64::from(d.x), f64::from(d.y));
            }
            vp.dirty = true;
        }
        if resp.dragged_by(PointerButton::Middle) {
            let d = resp.drag_delta();
            vp.camera.pan(f64::from(d.x), f64::from(d.y), h);
            vp.dirty = true;
        }
        vp.fly = !ortho && secondary && (resp.dragged_by(PointerButton::Secondary) || resp.hovered());
        if vp.fly {
            if delta != Vec2::ZERO {
                vp.camera.look(f64::from(delta.x), f64::from(delta.y));
                vp.dirty = true;
            }
            if !typing && keys.iter().any(|k| *k != 0.0) {
                let speed = (vp.camera.distance * 0.6).clamp(10.0, 400.0) * if shift { 4.0 } else { 1.0 } * dt;
                vp.camera.fly(keys[0] * speed, keys[1] * speed, keys[2] * speed);
                vp.dirty = true;
            }
            ui.ctx().request_repaint();
        }
        if resp.hovered() && scroll != 0.0 {
            vp.camera.zoom(f64::from(scroll) / 50.0);
            vp.dirty = true;
        }
        // camera bookmarks: Ctrl+1-9 keeps this view's camera, 1-9 recalls it (per level and projection)
        if !typing && resp.hovered() {
            let (digits, ctrl) = ui.input(|i| {
                let digits: Vec<usize> = if i.modifiers.alt || i.modifiers.shift {
                    Vec::new()
                } else {
                    [
                        Key::Num1,
                        Key::Num2,
                        Key::Num3,
                        Key::Num4,
                        Key::Num5,
                        Key::Num6,
                        Key::Num7,
                        Key::Num8,
                        Key::Num9,
                    ]
                    .iter()
                    .enumerate()
                    .filter(|(_, k)| i.key_pressed(**k))
                    .map(|(n, _)| n)
                    .collect()
                };
                (digits, i.modifiers.command)
            });
            for n in digits {
                let key = format!("{}/{}", self.view.lv, projection.name());
                let marks = self.bookmarks.entry(key).or_insert_with(|| vec![None; 9]);
                marks.resize(9, None);
                if ctrl {
                    if let Some(m) = marks.get_mut(n) {
                        *m = Some(vp.camera);
                    }
                } else if let Some(Some(c)) = marks.get(n) {
                    vp.camera = *c;
                    vp.dirty = true;
                }
            }
        }
        // the target and its texture follow the panel's size
        let gpu = &self.gpu.gpu;
        if vp.target.as_ref().is_none_or(|t| t.size != px) {
            let t = self.renderer.target(gpu, px);
            let mut r = self.gpu.egui.write();
            match vp.texture {
                Some(id) => r.update_egui_texture_from_wgpu_texture(
                    &gpu.device,
                    &t.output_view,
                    egui_wgpu::wgpu::FilterMode::Nearest,
                    id,
                ),
                None => {
                    vp.texture = Some(r.register_native_texture(
                        &gpu.device,
                        &t.output_view,
                        egui_wgpu::wgpu::FilterMode::Nearest,
                    ));
                }
            }
            vp.target = Some(t);
            vp.dirty = true;
        }
        // a click picks what is drawn under the pointer (a double click also focuses it); the surface point under
        // it goes to the status bar
        if (resp.clicked() || resp.double_clicked())
            && let (Some(pos), Some(t)) = (resp.interact_pointer_pos(), vp.target.as_ref())
        {
            let view = vp.camera.view(size);
            if vp.dirty {
                self.renderer
                    .render(gpu, &self.view.gpu_scene, t, &view.matrix(), &self.palette);
                vp.dirty = false;
            }
            let at = (pos - rect.min) * ppp;
            let id = self
                .renderer
                .pick(gpu, t, [at.x.max(0.0) as u32, at.y.max(0.0) as u32], 4);
            self.view.select(self.gpu, render::id_entity(id), true);
            self.view.last_hit = if id == render::ID_NONE {
                None
            } else {
                let (o, d) = view.ray([f64::from(at.x), f64::from(at.y)]);
                query::ray_hit(&self.view.scene, o, d)
            };
            if resp.double_clicked() {
                self.view.focus_selected();
                if let Some(e) = self.view.selected.and_then(|k| self.view.scene.entities.get(k))
                    && let Some(p) = e.position()
                {
                    let s = self.view.gpu_scene.sizes.get(e.index).copied().unwrap_or(2.0);
                    vp.camera.focus(p, Some(camera::focus_distance(s)));
                }
            }
            // the selection changed: this view draws again too (it is out of the map while it is shown)
            vp.dirty = true;
        }
        if vp.dirty
            && let Some(t) = vp.target.as_ref()
        {
            self.renderer.render(
                gpu,
                &self.view.gpu_scene,
                t,
                &vp.camera.view(size).matrix(),
                &self.palette,
            );
            vp.dirty = false;
        }
        if let Some(tex) = vp.texture {
            ui.painter().image(
                tex,
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        if self.view.labels {
            self.labels(ui, rect, &vp.camera);
        }
        // the controls, in the corner (from Help's list, so the two never differ), on the view's label plate
        let painter = ui.painter().with_clip_rect(rect);
        let m = &self.metrics;
        let hint = painter.layout_no_wrap(help::corner_hint(ortho), FontId::proportional(11.0), self.palette.ink);
        let at = rect.left_bottom() + Vec2::new(m.space_2, -m.space_2 - hint.size().y);
        let plate = Rect::from_min_size(at, hint.size()).expand2(Vec2::new(m.space_1, 2.0));
        painter.rect_filled(plate, CornerRadius::same(m.radius_sm as u8), self.palette.scene_label);
        painter.galley(at, hint, self.palette.ink);
        // what the level opened without (no terrain ...): a note in the corner, the details in Problems
        if let Some(first) = self.view.notes.first() {
            let more = self.view.notes.len() - 1;
            let text = if more > 0 {
                format!("{first} (+{more} in Problems)")
            } else {
                first.clone()
            };
            let g = painter.layout(text, FontId::proportional(12.0), self.palette.ink, rect.width() * 0.6);
            let at = rect.left_top() + Vec2::new(m.space_2 + m.space_1 + 14.0, m.space_2 + 2.0);
            let plate = Rect::from_min_size(at - Vec2::new(14.0, 0.0), g.size() + Vec2::new(14.0, 0.0))
                .expand2(Vec2::new(m.space_1, 2.0));
            painter.rect_filled(plate, CornerRadius::same(m.radius_sm as u8), self.palette.scene_label);
            // the information circle, in the panel colour: it sits on the plate
            status_shape(
                &painter,
                Pos2::new(at.x - 8.0, at.y + 8.0),
                5.0,
                "Information",
                self.palette.status_info,
            );
            painter.galley(at, g, self.palette.ink);
        }
        self.view.viewports.insert(projection, vp);
    }

    /// Team and name labels at gameplay entities (spawns, flags, objectives, vehicle spawns), nearest first; the
    /// selection's always.
    fn labels(&self, ui: &egui::Ui, rect: Rect, cam: &Camera) {
        let v = &*self.view;
        let view = cam.view([f64::from(rect.width()), f64::from(rect.height())]);
        // the orthographic views have no near and far: distance from what they look at
        let from = if view.ortho.is_some() { cam.target } else { view.eye };
        let mut items: Vec<(f64, usize, Pos2)> = Vec::new();
        for e in &v.scene.entities {
            let shown = matches!(
                e.category.as_str(),
                "soldier spawn" | "vehicle spawn" | "capture point" | "objective"
            ) && !v.gpu_scene.is_hidden(&e.category);
            if !(shown || v.selected == Some(e.index)) {
                continue;
            }
            let Some(p) = e.position() else { continue };
            let top = p[1] + v.marker_offset(e.index) * 2.0 + 0.5;
            let Some(s) = view.project([p[0], top, p[2]]) else {
                continue;
            };
            let d = ((p[0] - from[0]).powi(2) + (p[1] - from[1]).powi(2) + (p[2] - from[2]).powi(2)).sqrt();
            items.push((d, e.index, rect.min + Vec2::new(s[0] as f32, s[1] as f32)));
        }
        items.sort_by(|a, b| a.0.total_cmp(&b.0));
        let painter = ui.painter().with_clip_rect(rect);
        let p = &self.palette;
        let m = &self.metrics;
        let mut placed: Vec<Rect> = Vec::new();
        // the selection's label first, so it is never crowded out
        items.sort_by_key(|(_, index, _)| v.selected != Some(*index));
        for (k, (d, index, at)) in items.into_iter().enumerate() {
            let selected = v.selected == Some(index);
            if !selected && (k >= 80 || d > 1500.0) {
                continue;
            }
            let Some(e) = v.scene.entities.get(index) else { continue };
            // the team's faction (US, RU, NVA, JP ...; T1 / T2 where the level says none), beside its colour
            let team = e.team.map_or_else(String::new, |t| v.teams.label(t).to_owned());
            let square = match e.team {
                Some(t) => Some(p.team(t)),
                None if render::GAMEPLAY.contains(&e.category.as_str()) => Some(p.unowned),
                None => None,
            };
            let text = model::display_name(e);
            let font = FontId::proportional(11.0);
            let g = painter.layout_no_wrap(text, font.clone(), p.ink);
            let tg = painter.layout_no_wrap(
                team,
                FontId::new(11.0, egui::FontFamily::Name(crate::fonts::SANS_SEMIBOLD.into())),
                p.ink_muted,
            );
            let sq = if square.is_some() { 8.0 } else { 0.0 };
            let w = sq
                + if sq > 0.0 { m.space_1 } else { 0.0 }
                + g.size().x
                + if tg.size().x > 0.0 {
                    m.space_1 + tg.size().x
                } else {
                    0.0
                };
            let r = Rect::from_center_size(at - Vec2::new(0.0, 10.0), Vec2::new(w + 2.0 * m.space_1, 16.0));
            // nearer labels win; one that would cover another is left out (the selection's never)
            if !selected && placed.iter().any(|p: &Rect| p.intersects(r)) {
                continue;
            }
            placed.push(r);
            painter.rect_filled(r, CornerRadius::same(m.radius_sm as u8), p.scene_label);
            if selected {
                painter.rect_stroke(
                    r,
                    CornerRadius::same(m.radius_sm as u8),
                    Stroke::new(1.0, p.selection),
                    egui::StrokeKind::Outside,
                );
            }
            let mut x = r.left() + m.space_1;
            if let Some(c) = square {
                painter.rect_filled(
                    Rect::from_center_size(Pos2::new(x + 4.0, r.center().y), Vec2::splat(8.0)),
                    CornerRadius::same(1),
                    c,
                );
                x += sq + m.space_1;
            }
            let gw = g.size().x;
            painter.galley(Pos2::new(x, r.center().y - g.size().y / 2.0), g, p.ink);
            x += gw + m.space_1;
            painter.galley(Pos2::new(x, r.center().y - tg.size().y / 2.0), tg, p.ink_muted);
        }
    }

    fn header(&self, ui: &mut egui::Ui, left: &str, right: &str) {
        let p = &self.palette;
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), self.metrics.panel_header),
            Sense::hover(),
        );
        ui.painter().rect_filled(rect, CornerRadius::ZERO, p.surface_3);
        let g = ui.painter().layout_job(caption(left, p.ink_muted));
        ui.painter().galley(
            Pos2::new(rect.left() + self.metrics.space_3, rect.center().y - g.size().y / 2.0),
            g,
            p.ink_muted,
        );
        ui.painter().text(
            Pos2::new(rect.right() - self.metrics.space_3, rect.center().y),
            Align2::RIGHT_CENTER,
            right,
            FontId::proportional(12.0),
            p.ink_muted,
        );
    }

    fn outliner(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let m = self.metrics;
        let total = self.view.scene.entities.len();
        self.header(ui, "Contents", &format!("{} entities", group_digits(total as u64)));
        egui::Frame::new()
            .inner_margin(Margin::same(m.space_2 as i8))
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.view.search)
                        .hint_text(RichText::new("Search name, type, GUID or blueprint").color(p.ink_faint))
                        .desired_width(f32::INFINITY),
                );
            });
        let reveal = std::mem::take(&mut self.view.reveal)
            .then_some(self.view.selected)
            .flatten();
        let mut clicked: Option<(usize, bool)> = None;
        let mut toggles: Vec<(String, bool)> = Vec::new();
        let query = self.view.search.clone();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let v = &*self.view;
            for kind in Kind::ALL {
                let groups: Vec<&model::Group> = v.outliner.groups.iter().filter(|g| g.kind == kind).collect();
                if groups.is_empty() {
                    continue;
                }
                let g = ui.painter().layout_job(caption(kind.name(), p.ink_muted));
                let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), m.row), Sense::hover());
                ui.painter().galley(
                    Pos2::new(r.left() + m.space_2, r.center().y - g.size().y / 2.0),
                    g,
                    p.ink_muted,
                );
                for grp in groups {
                    let rows = Outliner::matching(grp, &query);
                    if rows.is_empty() && !query.is_empty() {
                        continue;
                    }
                    let hidden = v.gpu_scene.is_hidden(&grp.category);
                    let id = ui.make_persistent_id(("outliner group", &grp.category));
                    let mut state =
                        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
                    if reveal.is_some_and(|s| grp.entries.iter().any(|e| e.entity == s))
                        || (!query.is_empty() && rows.len() <= 50)
                    {
                        state.set_open(true);
                    }
                    let header = state.show_header(ui, |ui| {
                        ui.label(RichText::new(grp.name).color(if hidden { p.ink_faint } else { p.ink }));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let eye = if hidden { "Show" } else { "Hide" };
                            if ui
                                .add(
                                    egui::Button::new(RichText::new(eye).text_style(style("small")).color(p.ink_muted))
                                        .frame(false),
                                )
                                .on_hover_text(format!("{eye} {} in the 3D view", grp.name.to_lowercase()))
                                .clicked()
                            {
                                toggles.push((grp.category.clone(), !hidden));
                            }
                            ui.label(
                                RichText::new(group_digits(rows.len() as u64))
                                    .text_style(style("value"))
                                    .color(p.ink_muted),
                            );
                        });
                    });
                    header.body(|ui| {
                        let row_h = m.row;
                        let show = |ui: &mut egui::Ui,
                                    range: std::ops::Range<usize>,
                                    clicked: &mut Option<(usize, bool)>| {
                            for e in rows.get(range).unwrap_or_default() {
                                let resp = outliner_row(ui, &p, &m, e, &v.teams, v.selected == Some(e.entity), hidden);
                                if reveal == Some(e.entity) {
                                    resp.scroll_to_me(Some(Align::Center));
                                }
                                if resp.double_clicked() {
                                    *clicked = Some((e.entity, true));
                                } else if resp.clicked() {
                                    *clicked = Some((e.entity, false));
                                }
                            }
                        };
                        if rows.len() > 300 {
                            // long groups (static group members) scroll on their own, drawing only the rows in view
                            egui::ScrollArea::vertical()
                                .id_salt(("rows", &grp.category))
                                .max_height(row_h * 16.0)
                                .show_rows(ui, row_h, rows.len(), |ui, range| show(ui, range, &mut clicked));
                        } else {
                            show(ui, 0..rows.len(), &mut clicked);
                        }
                    });
                }
            }
        });
        for (g, hide) in toggles {
            self.view.gpu_scene.set_hidden(&self.gpu.gpu, &g, hide);
            self.view.redraw();
        }
        if let Some((entity, focus)) = clicked {
            self.view.select(self.gpu, Some(entity), false);
            if focus {
                self.view.focus_selected();
            }
        }
    }

    fn properties(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let m = self.metrics;
        let Some(e) = self
            .view
            .selected
            .and_then(|k| self.view.scene.entities.get(k))
            .cloned()
        else {
            self.header(ui, "Properties", "");
            let t = &self.view.teams;
            egui::Frame::new()
                .inner_margin(Margin::same(m.space_3 as i8))
                .show(ui, |ui| {
                    ui.label(RichText::new("Select an object in the 3D view or the outliner.").color(p.ink_muted));
                    // the level: its profile and its teams, with where the labels come from
                    ui.add_space(m.space_4);
                    ui.label(caption("Level", p.ink_muted));
                    ui.add_space(m.space_1);
                    egui::Grid::new("level teams")
                        .num_columns(2)
                        .spacing([m.space_3, m.space_1])
                        .show(ui, |ui| {
                            ui.label(RichText::new("Content").color(p.ink_muted));
                            ui.label(RichText::new(Profile::of(&self.view.lv).word()).color(p.ink));
                            ui.end_row();
                            for team in [1u8, 2] {
                                ui.label(RichText::new(format!("Team {team}")).color(p.ink_muted));
                                ui.horizontal_wrapped(|ui| {
                                    let (r, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                                    ui.painter().rect_filled(r, CornerRadius::same(2), p.team(team));
                                    ui.add(egui::Label::new(RichText::new(t.describe(team)).color(p.ink)).wrap());
                                });
                                ui.end_row();
                            }
                            ui.label(RichText::new("Team labels").color(p.ink_muted));
                            ui.add(egui::Label::new(RichText::new(&t.source).color(p.ink)).wrap());
                            ui.end_row();
                        });
                });
            return;
        };
        self.header(ui, "Properties", &e.short_type);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(Margin::same(m.space_3 as i8))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(model::display_name(&e))
                            .text_style(style("title"))
                            .color(p.ink),
                    );
                    ui.add_space(m.space_2);
                    egui::Grid::new("properties")
                        .num_columns(2)
                        .spacing([m.space_3, m.space_1])
                        .show(ui, |ui| {
                            for (label, value, data) in model::properties(&e, &self.view.teams) {
                                ui.label(RichText::new(label).color(p.ink_muted));
                                let text = if data {
                                    RichText::new(value).text_style(style("value")).color(p.ink)
                                } else {
                                    RichText::new(value).color(p.ink)
                                };
                                if label == "Team" {
                                    ui.horizontal_wrapped(|ui| {
                                        let (r, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                                        let c = e.team.map_or(p.unowned, |t| p.team(t));
                                        ui.painter().rect_filled(r, CornerRadius::same(2), c);
                                        ui.add(egui::Label::new(text).wrap());
                                    });
                                } else {
                                    ui.add(egui::Label::new(text).wrap());
                                }
                                ui.end_row();
                            }
                        });
                    ui.add_space(m.space_3);
                    let g = ui.painter().layout_job(caption("Transform", p.ink_muted));
                    let (r, _) = ui.allocate_exact_size(g.size(), Sense::hover());
                    ui.painter().galley(r.min, g, p.ink_muted);
                    ui.add_space(m.space_1);
                    egui::Grid::new("transform")
                        .num_columns(4)
                        .spacing([m.space_2, m.space_1])
                        .show(ui, |ui| {
                            for (label, v) in model::transform_rows(&e) {
                                ui.label(RichText::new(label).color(p.ink_muted));
                                for (axis, (c, value)) in [(p.axis_x, v[0]), (p.axis_y, v[1]), (p.axis_z, v[2])]
                                    .into_iter()
                                    .enumerate()
                                {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(["X", "Y", "Z"].get(axis).copied().unwrap_or_default())
                                                .text_style(style("body-strong"))
                                                .color(c),
                                        );
                                        let unit = if label == "Position" { " m" } else { "" };
                                        ui.label(
                                            RichText::new(format!("{value:.3}{unit}"))
                                                .text_style(style("value"))
                                                .color(p.ink),
                                        );
                                    });
                                }
                                ui.end_row();
                            }
                        });
                    ui.add_space(m.space_3);
                    ui.horizontal(|ui| {
                        if ui.button("Focus").on_hover_text("Moves the camera to it (F)").clicked() {
                            self.view.focus_selected();
                        }
                        if ui.button("Copy GUID").clicked() {
                            ui.ctx().copy_text(e.guid.clone());
                        }
                    });
                });
        });
    }

    fn problems(&self, ui: &mut egui::Ui) {
        let p = &self.palette;
        let m = &self.metrics;
        self.header(ui, "Problems", "");
        egui::Frame::new()
            .inner_margin(Margin::same(m.space_3 as i8))
            .show(ui, |ui| {
                if self.view.notes.is_empty() {
                    ui.label(
                        RichText::new(
                            "No problems. Live validation comes with editing; a read-only map has nothing to check.",
                        )
                        .color(p.ink_muted),
                    );
                }
                for n in &self.view.notes {
                    status_line(ui, p, "Information", p.status_info, n);
                }
            });
        let _ = self.tokens;
    }
}

fn outliner_row(
    ui: &mut egui::Ui,
    p: &Palette,
    m: &Metrics,
    e: &model::Entry,
    teams: &Teams,
    selected: bool,
    hidden: bool,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), m.row), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, CornerRadius::same(m.radius_sm as u8), p.accent_subtle);
        painter.rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(2.0, rect.height())),
            CornerRadius::ZERO,
            p.accent,
        );
    } else if resp.hovered() {
        painter.rect_filled(rect, CornerRadius::ZERO, p.surface_2);
    }
    let mut x = rect.left() + m.space_2;
    if let Some(t) = e.team {
        painter.rect_filled(
            Rect::from_center_size(Pos2::new(x + 4.0, rect.center().y), Vec2::splat(8.0)),
            CornerRadius::same(1),
            p.team(t),
        );
        x += 8.0 + m.space_1;
        painter.text(
            Pos2::new(rect.right() - m.space_2, rect.center().y),
            Align2::RIGHT_CENTER,
            teams.label(t),
            FontId::proportional(11.0),
            p.ink_muted,
        );
    }
    painter.text(
        Pos2::new(x, rect.center().y),
        Align2::LEFT_CENTER,
        &e.name,
        FontId::proportional(13.0),
        if hidden { p.ink_faint } else { p.ink },
    );
    painter.hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, p.line));
    resp
}
