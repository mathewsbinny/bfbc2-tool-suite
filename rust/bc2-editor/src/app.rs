//! The map editor's window: the top bar (workspace switcher, the level or project, the content profile badge, the
//! theme and the layout), the start screen (round G4: Open project, Edit existing level, Create level), the
//! docked panels around the 3D view (Outliner and Layers left, Properties right, Problems and History below; drag,
//! dock, tab and float them, Reset layout returns to this), and the status bar.
//!
//! Round G1 made the read-only view (the View workspace); round E2 edits a map project in the Objects workspace:
//! select (click, box), move and rotate with the gizmo in world or local axes, snapping (grid, angle, vertex and
//! edge, surface with a sink depth), align and distribute, the ruler, delete and duplicate, layers, folders and
//! groups, each edit an undoable operation in the History panel, autosave and crash recovery. The same
//! [`EditorApp`] runs in the window (`main.rs`, eframe) and headless (`headless.rs`: the tests and screenshots), so
//! what the tests see is what the window shows.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Instant;

use bc2_levelwrite::ops::{FieldSet, LAYER_COLOURS, Moved};
use bc2_levelwrite::{FieldType, FieldValue, MapProject, Op, Placement, SinkUnit};
use bc2_scene::scene::{self, Scene};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Key, Layout, Margin, PointerButton, Pos2, Rect, RichText,
    Sense, Stroke, TextStyle, Vec2,
};
use eframe::egui_wgpu;
use eframe::epaint::mutex::RwLock;
use egui_dock::{DockArea, DockState, NodeIndex, TabViewer};

use crate::actions::{self, Action, PanelKind};
use crate::camera::{self, Camera, Projection};
use crate::catalog::{self, Catalog};
use crate::edit::Doc;
use crate::factions::{Profile, Teams};
use crate::gizmo::{self, Frame, Handle, Space, Tool};
use crate::help::{self, HelpState};
use crate::levels::{self, ContentSet, LevelInfo};
use crate::math::{self, M, V3};
use crate::model::{self, Kind, Outliner};
use crate::prepare::{self, Job, Outcome, Plan};
use crate::query;
use crate::render::{self, Gpu, SceneGpu, Target};
use crate::theme::{self, Metrics, Palette, ThemeId, Tokens};

mod create;
mod editui;
mod inspectui;
mod modes;
mod tools;

pub use crate::workspace::Workspace;
pub use create::HeightmapForm;
pub use modes::StartPage;
pub use tools::{ContextTarget, MenuItem, MenuKind, Placing, SinkPopup, menu_items};

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
    /// Live validation (the level's load notes, edits that did not apply, the base-changed report).
    Problems,
    /// The project's operations: undo and redo to any point.
    History,
    /// The project's layers: hide, isolate, lock, colour.
    Layers,
    /// The content browser: blueprints to place.
    Content,
    /// The selection's links: references and event connections, both ways (round GR1).
    Links,
}

impl Tab {
    fn title(self) -> &'static str {
        match self {
            Self::Viewport(p) => p.name(),
            Self::Outliner => "Outliner",
            Self::Properties => "Properties",
            Self::Problems => "Problems",
            Self::History => "History",
            Self::Layers => "Layers",
            Self::Content => "Place objects",
            Self::Links => "Links",
        }
    }
}

/// The default layout: the 3D view in the middle, the outliner (and layers) left, properties right with the links
/// below them, problems (and history) below the view.
#[must_use]
pub fn default_layout() -> DockState<Tab> {
    let mut d = DockState::new(vec![Tab::Viewport(Projection::Perspective)]);
    let s = d.main_surface_mut();
    let [centre, _] = s.split_left(NodeIndex::root(), 0.2, vec![Tab::Outliner, Tab::Layers, Tab::Content]);
    let [centre, right] = s.split_right(centre, 0.72, vec![Tab::Properties]);
    let [_, _] = s.split_below(right, 0.6, vec![Tab::Links]);
    let [_, _] = s.split_below(centre, 0.78, vec![Tab::Problems, Tab::History]);
    d
}

/// What the editor keeps between runs (eframe's storage): the theme, the layout, camera bookmarks per level, the
/// recent projects and the last project folder.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Saved {
    /// Dark or Light.
    pub theme: ThemeId,
    /// The panels.
    pub dock: DockState<Tab>,
    /// Bookmarks 1-9 by level.
    pub bookmarks: HashMap<String, Vec<Option<Camera>>>,
    /// Projects opened or made, the latest first.
    #[serde(default)]
    pub recent: Vec<PathBuf>,
    /// Where new projects were last made.
    #[serde(default)]
    pub project_folder: Option<PathBuf>,
}

/// The snapping and placement settings (the gizmo toolbar).
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(clippy::struct_excessive_bools, reason = "the toolbar's switches, one field each")]
pub struct Snap {
    /// Moves snap to the grid.
    pub grid: bool,
    /// The grid, metres.
    pub grid_size: f64,
    /// Turns snap to the angle step.
    pub angle: bool,
    /// The angle step, degrees.
    pub angle_step: f64,
    /// Moves snap to the corner (or, with Shift held, the edge) of the surface under the pointer.
    pub vertex: bool,
    /// Moves put the objects on the surface below them (terrain or a mesh), sunk by `placement`.
    pub surface: bool,
    /// How objects sit on the surface (the sink depth, along the normal or straight down, turned to the normal).
    pub placement: Placement,
}

impl Default for Snap {
    fn default() -> Self {
        Self {
            grid: false,
            grid_size: 1.0,
            angle: false,
            angle_step: 15.0,
            vertex: false,
            surface: false,
            placement: Placement::default(),
        }
    }
}

/// A gizmo drag under way.
#[derive(Debug, Clone)]
struct Drag {
    projection: Projection,
    handle: Handle,
    frame: Frame,
    start: gizmo::Start,
    /// The objects and their transforms when the drag began.
    objects: Vec<(usize, M)>,
    /// Their transforms now.
    now: Vec<(usize, M)>,
    /// Placed on the surface.
    placed: bool,
}

/// An open level.
#[allow(clippy::struct_excessive_bools, reason = "the view's switches, one field each")]
pub struct ViewState {
    /// The level (`mp_005`).
    pub lv: String,
    /// The scene as loaded (moved into the project's [`Doc`] when one is open).
    base: Scene,
    gpu_scene: SceneGpu,
    /// The outliner's model.
    pub outliner: Outliner,
    /// The selected entity (the primary one: the gizmo's and the properties').
    pub selected: Option<usize>,
    /// Every selected entity (the primary last).
    pub selection: Vec<usize>,
    /// The project being edited.
    pub doc: Option<Doc>,
    /// The workspace.
    pub workspace: Workspace,
    /// The editing tool.
    pub tool: Tool,
    /// World or local axes.
    pub space: Space,
    /// Snapping.
    pub snap: Snap,
    drag: Option<Drag>,
    box_from: Option<(Projection, Pos2)>,
    /// The ruler's points (up to two).
    pub ruler: Vec<V3>,
    /// The last edit or action, in words (the status bar).
    pub message: String,
    /// The handle under the pointer.
    hover: Option<Handle>,
    folder_input: String,
    /// A typed transform edit under way in the properties: the entity and its transform before it.
    prop_from: Option<(usize, M)>,
    /// The properties' surface placement for the primary object.
    prop_place: Option<(usize, Placement)>,
    /// A field being typed in the properties: the entity, the field, the text.
    field_edit: Option<(usize, String, String)>,
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
    /// The 3D view draws the level as the game does (round R5: [`crate::gameview`]).
    pub game_accurate: bool,
    /// The game-accurate view's level, loaded when the mode is first turned on.
    pub game: Option<crate::gameview::GameView>,
    // ---- round E2b
    /// The Objects workspace's category filter (also the content browser's).
    pub filter: Option<crate::workspace::Category>,
    /// The editor view's display mode.
    pub view_mode: crate::workspace::ViewMode,
    /// What the display mode shows, in words (where its data came from).
    pub mode_note: String,
    /// The navmesh mode's outlines (loaded once): the file and its segments, or why there are none.
    nav: Option<Result<(String, Vec<crate::viewmodes::Segment>), String>>,
    /// The cull-tree mode's data (loaded once).
    cull: Option<crate::viewmodes::CullView>,
    /// The level's folders (its own, then its shared bundles').
    pub bases: Vec<PathBuf>,
    /// Project OPFOR BFBC2's folder.
    root: Option<PathBuf>,
    /// The content browser's catalogue (a project open).
    pub content: Option<crate::content::Catalogue>,
    content_query: String,
    /// The content browser's chosen entry (its blueprint path).
    pub content_pick: Option<String>,
    /// An object being placed.
    pub placing: Option<Placing>,
    /// The sink popup.
    pub sink: Option<SinkPopup>,
    /// The "Place object here..." picker.
    pub picker: Option<tools::Picker>,
    /// The last right-click's target.
    pub context: Option<ContextTarget>,
    /// The point a context menu entry acts at (taken once).
    menu_point: Option<V3>,
    /// The ray under the pointer in the view it is over (last frame).
    pointer_ray: Option<(V3, V3)>,
    /// The polygon selection's corners (screen) and the view they are drawn in.
    pub polygon: Vec<Pos2>,
    polygon_view: Option<Projection>,
    /// Enter was pressed: the polygon closes.
    polygon_close: bool,
    /// The objects Copy remembered (guids).
    pub clipboard: Vec<String>,
    /// Actions asked for by menus and panels, run by the app after the frame's panels.
    pub pending: Vec<Action>,
    /// The last context menu's entries on screen: (label, rectangle, enabled) - the tests click them.
    pub menu_rects: Vec<(String, Rect, bool)>,
    /// Which menu that was.
    pub menu_kind: Option<MenuKind>,
    /// The content browser's tiles on screen: (blueprint path, rectangle) - the tests drag them.
    pub content_rects: Vec<(String, Rect)>,
    /// An entry that could not be placed (not read again while it is dragged).
    place_failed: Option<String>,
    /// Rendered thumbnails loaded from the cache, by blueprint path (`None`: none there).
    thumbs: HashMap<String, Option<egui::TextureHandle>>,
    /// The asset browser's mode: Place Objects, Spawners, Objectives.
    pub browser_mode: crate::gameplay::BrowserMode,
    /// The team placed spawns, flags and vehicle spawners take (1, 2; 0 neutral).
    pub place_team: u8,
    /// The Spawners and Objectives tiles, with what they were made from (the log's and the catalogue's sizes).
    tiles: Option<((usize, usize), Vec<crate::gameplay::Tile>)>,
    // ---- round GR1: the Map Inspector's views
    /// The links are drawn in the 3D view (the selection's references and event connections).
    pub show_links: bool,
    /// The partitions read for the property tree and the links, and the project revision they were read at.
    inspection: Option<(u64, crate::inspect::Inspection)>,
    /// The selected entity's property tree (entity, revision, the tree or why it could not be read).
    fields_tree: Option<(usize, u64, Result<Vec<crate::inspect::PropNode>, String>)>,
    /// The objects a link was followed from, the latest last (Links' Back).
    pub link_back: Vec<usize>,
    /// The Links panel's rows on screen: (the entity a row leads to, its rectangle) - the tests click them.
    pub link_rects: Vec<(usize, Rect)>,
    /// All fields' container headers on screen: (field path by name, rectangle) - the tests open them.
    pub field_rects: Vec<(String, Rect)>,
    /// Measuring (M): clicks add points to [`ViewState::measure`] instead of selecting.
    pub measuring: bool,
    /// The measured path's points.
    pub measure: Vec<V3>,
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

type Loaded = Result<(Scene, Vec<String>, f64, Vec<PathBuf>), String>;

/// What to do with a level once it is loaded.
#[derive(Debug, Clone)]
pub enum Then {
    /// Show it read-only.
    View,
    /// Make a new project on it: its name, its folder.
    Create(String, PathBuf),
    /// Open this project on it (with the autosave recovered, when the user chose that).
    Open(PathBuf, Option<Box<MapProject>>),
}

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
        then: Then,
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
    /// "New map from an existing map": the base level, the name, the folder.
    new_level: Option<String>,
    new_name: String,
    new_folder: String,
    /// The new project form's target (round E3d): the era (`None`: the base's own, its layer kept) and the mode.
    new_target: (Option<bc2_levelwrite::Profile>, String),
    /// The game copy's multiplayer modes per era, read once (round E3d: the target pickers list them).
    era_modes: Option<Result<EraModes, String>>,
    /// The target the next created project takes (set by the form's button).
    pending_target: Option<(bc2_levelwrite::Profile, String)>,
    /// "Open a project": the path typed.
    open_path: String,
    recent: Vec<PathBuf>,
    /// An autosave to offer before a project opens: the project file and the autosave.
    recover: Option<(PathBuf, Box<MapProject>)>,
    /// What to do with a level being prepared once it is loaded.
    after_prepare: Then,
    /// The start screen's page (round G4; never remembered: every start opens on the three paths).
    start: StartPage,
    /// "Create level - from a height map": the form.
    pub heightmap: HeightmapForm,
    /// A height-map project shown on the start screen (it has no level to open until E7): its file and project.
    import_shown: Option<(PathBuf, Box<MapProject>)>,
    /// The start screen's buttons drawn this frame, by label (the tests click them).
    start_buttons: Vec<(String, Rect)>,
    /// The top bar's buttons' width as last drawn (the room left for the map's name).
    top_right: std::cell::Cell<f32>,
    /// The top bar as last drawn: the right edge of the map's badge and name, the left edge of the buttons.
    top_spans: std::cell::Cell<[f32; 2]>,
    /// "Open project": the project files found in the projects folder (read when the page opens).
    found_projects: Vec<PathBuf>,
    /// The start screen's pictures, thumbnails and modes from the game copy, and the thread reading them.
    catalog: Option<Catalog>,
    catalog_rx: Option<Receiver<Catalog>>,
    /// The catalog's pictures made into textures, by key.
    pictures: HashMap<String, egui::TextureHandle>,
    /// The Export window (round E3: `export_ui`).
    pub export: crate::export_ui::ExportWindow,
}

/// The folder new projects go to when none was chosen before: `Documents\BFBC2 Map Projects`.
#[must_use]
pub fn default_project_folder() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join("Documents")
        .join("BFBC2 Map Projects")
}

impl EditorApp {
    /// The editor, its game copy found from `opfor` (or as the suite finds it), its saved state applied.
    ///
    /// # Errors
    ///
    /// tokens.json does not read (the build embeds it; the tests check it).
    pub fn new(opfor: Option<&Path>, saved: Option<Saved>) -> Result<Self, theme::TokenError> {
        let tokens = Tokens::embedded()?;
        let opfor = levels::find_opfor(opfor);
        let levels = opfor.as_ref().map_or_else(|_| Vec::new(), |r| levels::list_levels(r));
        // the start screen's pictures, thumbnails and modes, read (or taken from their cache) on a thread
        let catalog_rx = opfor.as_ref().ok().map(|root| {
            let (tx, rx) = std::sync::mpsc::channel();
            let (root, names): (PathBuf, Vec<String>) = (root.clone(), levels.iter().map(|l| l.lv.clone()).collect());
            std::thread::spawn(move || {
                let _ = tx.send(catalog::load(&root, &names));
            });
            rx
        });
        let (theme, dock, bookmarks, recent, folder) = match saved {
            Some(s) => (s.theme, s.dock, s.bookmarks, s.recent, s.project_folder),
            None => (ThemeId::Dark, default_layout(), HashMap::new(), Vec::new(), None),
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
            new_level: None,
            new_name: String::new(),
            new_folder: folder.unwrap_or_else(default_project_folder).display().to_string(),
            new_target: (None, "conquest".into()),
            era_modes: None,
            pending_target: None,
            open_path: String::new(),
            recent,
            recover: None,
            after_prepare: Then::View,
            start: StartPage::Home,
            heightmap: HeightmapForm::default(),
            import_shown: None,
            start_buttons: Vec::new(),
            top_right: std::cell::Cell::new(640.0),
            top_spans: std::cell::Cell::new([0.0, 0.0]),
            found_projects: Vec::new(),
            catalog: None,
            catalog_rx,
            pictures: HashMap::new(),
            export: crate::export_ui::ExportWindow::default(),
        })
    }

    /// The start screen's catalog (pictures, thumbnails, modes) once read.
    #[must_use]
    pub fn catalog(&mut self) -> Option<&Catalog> {
        self.poll_catalog();
        self.catalog.as_ref()
    }

    /// Waits for the catalog (headless runs, screenshots), at most `limit`.
    pub fn wait_for_catalog(&mut self, limit: std::time::Duration) {
        let start = Instant::now();
        while self.catalog_rx.is_some() && start.elapsed() < limit {
            self.poll_catalog();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    fn poll_catalog(&mut self) {
        if let Some(rx) = &self.catalog_rx {
            match rx.try_recv() {
                Ok(c) => {
                    self.catalog = Some(c);
                    self.catalog_rx = None;
                    self.pictures.clear();
                }
                Err(TryRecvError::Disconnected) => self.catalog_rx = None,
                Err(TryRecvError::Empty) => {}
            }
        }
    }

    /// Why an action cannot run now (for its disabled control's tooltip), or `None` when it can.
    #[must_use]
    pub fn why_not(&self, action: Action) -> Option<&'static str> {
        why_not_in(self.view(), action)
    }
}

/// Why an action cannot run now in the open level (`None`: no level open), or `None` when it can.
#[must_use]
#[allow(
    clippy::match_same_arms,
    reason = "the arms' guards differ and their order matters: the first reason that holds is the one shown"
)]
pub fn why_not_in(v: Option<&ViewState>, action: Action) -> Option<&'static str> {
    {
        let open = v.is_some();
        let selected = v.is_some_and(|v| v.selected.is_some());
        let doc = v.and_then(|v| v.doc.as_ref());
        let editing = v.is_some_and(|v| v.doc.is_some() && v.workspace.moves_objects());
        let editable: Vec<usize> = v.map_or_else(Vec::new, ViewState::editable_selection);
        let objects = v.is_some_and(|v| v.doc.is_some() && v.workspace == Workspace::Objects);
        match action {
            Action::Brush(b) => b.workspace().coming(),
            Action::PathTest => Some("Comes with the AI workspace's tools (round E5a)"),
            Action::SetWorkspace(_) | Action::SetViewMode(_) | Action::FocusHere if !open => Some("Open a map first"),
            Action::SetWorkspace(w) if w.edits() && doc.is_none() => {
                Some("Open a project first (New map from an existing map, or Open a project)")
            }
            Action::SelectAllOfType if !(selected || v.is_some_and(|v| v.content_pick.is_some())) => {
                Some("Select an object or a content browser entry first")
            }
            Action::ShowPanel(PanelKind::Content) if doc.is_none() => {
                Some("Open a project first (New map from an existing map, or Open a project)")
            }
            Action::PlaceObject if !objects => Some("Edit a project in the Objects workspace first"),
            Action::PlaceObject if !doc.is_some_and(Doc::has_partition) => {
                Some("This scene has no level partition to add objects to")
            }
            Action::Paste if !editing => Some("Edit a project in the Objects workspace first"),
            Action::Paste if v.is_some_and(|v| v.clipboard.is_empty()) => Some("Copy objects first"),
            Action::ShowHidden if doc.is_none_or(|d| d.org.hidden.is_empty()) => Some("Nothing is hidden"),
            Action::UnlockAll if doc.is_none_or(|d| d.org.locked.is_empty()) => Some("Nothing is locked"),
            Action::DeepDuplicate
            | Action::Hide
            | Action::Lock
            | Action::Copy
            | Action::SinkPopup
            | Action::SetTeam(_)
                if !editing =>
            {
                Some("Edit a project in the Objects, Gameplay or AI workspace first")
            }
            Action::DeepDuplicate | Action::Hide | Action::Lock | Action::Copy | Action::SinkPopup
                if editable.is_empty() =>
            {
                Some("Select an object that can be edited first")
            }
            Action::SetTeam(_)
                if !v.is_some_and(|v| {
                    editable
                        .iter()
                        .any(|&k| v.scene().entities.get(k).is_some_and(|e| e.team.is_some()))
                }) =>
            {
                Some("Select spawns, flags or vehicle spawns first")
            }
            Action::FocusSelection | Action::CopyGuid | Action::ClearSelection if !selected => {
                Some("Select an object first")
            }
            Action::OpenViewport(_)
            | Action::ToggleLabels
            | Action::ToggleLinks
            | Action::ToggleMeasure
            | Action::ToggleGameView
            | Action::CloseMap
                if !open =>
            {
                Some("Open a map first")
            }
            Action::Undo
            | Action::Redo
            | Action::Save
            | Action::CheckEdits
            | Action::Export
            | Action::ShowPanel(PanelKind::History | PanelKind::Layers)
                if doc.is_none() =>
            {
                Some("Open a project first (New map from an existing map, or Open a project)")
            }
            Action::Undo if doc.is_some_and(|d| d.project.operations.is_empty()) => Some("Nothing to undo"),
            Action::Redo if doc.is_some_and(|d| d.project.redo.is_empty()) => Some("Nothing to redo"),
            Action::Delete
            | Action::Duplicate
            | Action::PlaceOnSurface
            | Action::Align(_)
            | Action::Distribute(_)
            | Action::Group
            | Action::Ungroup
            | Action::NewLayer
            | Action::UseTool(_)
            | Action::ToggleSpace
            | Action::ToggleGridSnap
            | Action::ToggleAngleSnap
            | Action::ToggleVertexSnap
            | Action::ToggleSurfaceSnap
                if !editing =>
            {
                Some("Edit a project in the Objects, Gameplay or AI workspace first")
            }
            Action::Delete | Action::Duplicate | Action::PlaceOnSurface | Action::Group | Action::NewLayer
                if editable.is_empty() =>
            {
                Some("Select an object that can be edited first")
            }
            Action::Align(_) if editable.len() < 2 => Some("Select two objects or more"),
            Action::Distribute(_) if editable.len() < 3 => Some("Select three objects or more"),
            Action::Ungroup
                if !v.is_some_and(|v| {
                    v.doc.as_ref().is_some_and(|d| {
                        v.selection
                            .iter()
                            .any(|&k| d.guid(k).is_some_and(|g| d.org.group_of(g).is_some()))
                    })
                }) =>
            {
                Some("Select a grouped object first")
            }
            _ => None,
        }
    }
}

impl EditorApp {
    /// Runs an action (the top bar, the keyboard and the command palette all come here).
    pub fn run(&mut self, gpu: &GpuCtx, ctx: &egui::Context, action: Action) {
        if self.why_not(action).is_some() {
            return;
        }
        let palette = self.palette();
        if let Mode::View(v) = &mut self.mode
            && (v.tool_action(gpu, &palette, action) || v.edit_action(gpu, &palette, action))
        {
            return;
        }
        match action {
            Action::ShowPanel(p) => {
                let tab = match p {
                    PanelKind::History => Tab::History,
                    PanelKind::Layers => Tab::Layers,
                    PanelKind::Problems => Tab::Problems,
                    PanelKind::Properties => Tab::Properties,
                    PanelKind::Content => Tab::Content,
                    PanelKind::Links => Tab::Links,
                };
                if let Some(path) = self.dock.find_tab(&tab) {
                    let _ = self.dock.set_active_tab(path);
                } else {
                    self.dock.push_to_focused_leaf(tab);
                }
            }
            Action::Save => {
                if let Mode::View(v) = &mut self.mode
                    && let Some(d) = v.doc.as_mut()
                {
                    v.message = match d.save() {
                        Ok(()) => format!("Saved {}", d.path.display()),
                        Err(e) => format!("Not saved: {e}"),
                    };
                }
            }
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
                    .and_then(|v| v.selected.and_then(|k| v.scene().entities.get(k)))
                {
                    ctx.copy_text(e.guid.clone());
                }
            }
            Action::ToggleLabels => {
                if let Mode::View(v) = &mut self.mode {
                    v.labels = !v.labels;
                }
            }
            Action::ToggleLinks => {
                if let Mode::View(v) = &mut self.mode {
                    v.show_links = !v.show_links;
                }
            }
            Action::ToggleMeasure => {
                if let Mode::View(v) = &mut self.mode {
                    v.measuring = !v.measuring;
                    v.measure.clear();
                    v.message = if v.measuring {
                        "Measuring: click points on the terrain or objects (M ends it)".to_owned()
                    } else {
                        String::new()
                    };
                }
            }
            Action::ToggleGameView => {
                if let (Mode::View(v), Ok(root)) = (&mut self.mode, &self.opfor) {
                    v.game_accurate = !v.game_accurate;
                    if v.game_accurate && v.game.is_none() {
                        match crate::gameview::GameView::load(root, &v.lv, v.scene()) {
                            Ok(g) => {
                                v.message = format!("Game-accurate view: loaded in {:.1} s", g.load_seconds);
                                v.game = Some(g);
                            }
                            Err(e) => {
                                v.message = format!("Game-accurate view: {e}");
                                v.game_accurate = false;
                            }
                        }
                    }
                }
            }
            Action::Help => {
                let page = self.help.page;
                self.help.show(page);
            }
            Action::Export => {
                if let Some(path) = self.view().and_then(|v| v.doc.as_ref()).map(|d| d.path.clone()) {
                    self.export
                        .open_for(&path, self.opfor.as_ref().ok().map(PathBuf::as_path));
                }
            }
            // the editing actions ran above
            _ => {}
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
            recent: self.recent.clone(),
            project_folder: Some(PathBuf::from(&self.new_folder)),
        }
    }

    /// The top bar as last drawn with a map open: the right edge of its badge and name, the left edge of its buttons
    /// (round G4: the first is never past the second).
    #[must_use]
    pub fn top_bar_spans(&self) -> [f32; 2] {
        self.top_spans.get()
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

    /// Opens a level read-only: prepares it first when it needs it (NavKit's prepare, the terrain unpack: a child
    /// process, with its progress shown), then loads it on a worker thread (the window stays usable).
    pub fn open_level(&mut self, lv: &str) {
        self.open_level_then(lv, Then::View);
    }

    /// Starts loading a level on a worker thread, then does `then` with it (view it, make or open a project).
    pub fn open_level_then(&mut self, lv: &str, then: Then) {
        let Ok(root) = self.opfor.clone() else { return };
        self.error = None;
        match prepare::plan(&root, lv) {
            Plan::Ready => self.load_level(lv, then),
            plan => {
                // what to do once prepared and loaded
                self.after_prepare = then;
                self.start_preparing(Job::start(&root, lv, plan));
            }
        }
    }

    /// [`EditorApp::open_level`] with another preparing script, whatever the level needs (the tests' stand-in).
    pub fn open_level_preparing_with(&mut self, script: &Path, lv: &str) {
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

    fn load_level(&mut self, lv: &str, then: Then) {
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
            then,
        };
    }

    /// Loads a level and waits for it (headless runs).
    ///
    /// # Errors
    ///
    /// No game copy, or the level does not load.
    pub fn open_level_blocking(&mut self, gpu: &GpuCtx, lv: &str) -> Result<(), String> {
        self.open_level_blocking_then(gpu, lv, Then::View)
    }

    /// Loads a level, waits for it and does `then` with it (headless runs: a new project, an opened one).
    ///
    /// # Errors
    ///
    /// No game copy, the level does not load, or the project cannot be made or opened.
    pub fn open_level_blocking_then(&mut self, gpu: &GpuCtx, lv: &str, then: Then) -> Result<(), String> {
        let root = self.opfor.clone()?;
        let plan = prepare::plan(&root, lv);
        if plan != Plan::Ready {
            let mut job = Job::start(&root, lv, plan)?;
            if let Outcome::Failed(e) = job.wait(std::time::Duration::from_mins(15)) {
                return Err(e);
            }
        }
        let loaded = load(&root, lv)?;
        self.show_scene(gpu, lv, loaded, then);
        self.error.clone().map_or(Ok(()), Err)
    }

    /// Opens a project file: its base level is loaded first (in the background), then the project on it; an
    /// autosave that differs from the file is offered first (crash recovery).
    pub fn open_project(&mut self, path: &Path) {
        match MapProject::load(path) {
            Ok(p) if p.is_import() => {
                // a new level from a height map: no level to open until its terrain is generated (round E7)
                self.error = None;
                self.remember(path);
                self.import_shown = Some((path.to_owned(), Box::new(p)));
                self.start = StartPage::ImportProject;
            }
            Ok(p) => {
                if let Some(auto) = MapProject::recover(path) {
                    self.recover = Some((path.to_owned(), Box::new(auto)));
                    return;
                }
                self.open_level_then(&p.base.level, Then::Open(path.to_owned(), None));
            }
            Err(e) => self.error = Some(format!("{} does not open: {e}", path.display())),
        }
    }

    /// The crash-recovery offer waiting for an answer: the project file.
    #[must_use]
    pub fn recovery_offer(&self) -> Option<&Path> {
        self.recover.as_ref().map(|(p, _)| p.as_path())
    }

    /// Answers the crash-recovery offer: open the autosave (`true`) or the saved project (the autosave removed).
    pub fn answer_recovery(&mut self, recover: bool) {
        let Some((path, auto)) = self.recover.take() else {
            return;
        };
        let level = auto.base.level.clone();
        if recover {
            self.open_level_then(&level, Then::Open(path, Some(auto)));
        } else {
            MapProject::discard_autosave(&path);
            self.open_level_then(&level, Then::Open(path, None));
        }
    }

    /// Shows a scene that is already loaded (the demo scene, tests).
    pub fn open_scene(&mut self, gpu: &GpuCtx, lv: &str, scene: Scene) {
        self.show_scene(gpu, lv, (scene, Vec::new(), 0.0, Vec::new()), Then::View);
    }

    /// Shows a scene that is already loaded and does `then` with it (the demo scene as a project, tests).
    pub fn open_scene_then(&mut self, gpu: &GpuCtx, lv: &str, scene: Scene, then: Then) {
        self.show_scene(gpu, lv, (scene, Vec::new(), 0.0, Vec::new()), then);
    }

    fn remember(&mut self, path: &Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_owned());
        self.recent.truncate(10);
    }

    fn show_scene(
        &mut self,
        gpu: &GpuCtx,
        lv: &str,
        (scene, notes, secs, bases): (Scene, Vec<String>, f64, Vec<PathBuf>),
        then: Then,
    ) {
        let palette = self.palette();
        let mut gpu_scene = SceneGpu::new(&gpu.gpu, &scene, &palette);
        for g in model::HIDDEN_AT_START {
            gpu_scene.set_hidden(&gpu.gpu, g, true);
        }
        let mut camera = Camera::default();
        if let Some((lo, hi)) = gpu_scene.bounds {
            camera.frame(lo, hi);
        }
        let mut doc = match then {
            Then::View => None,
            Then::Create(name, folder) => match Doc::create_kind(&name, &folder, lv, scene.clone(), self.base_kind(lv))
            {
                Ok(d) => {
                    self.remember(&d.path);
                    Some(d)
                }
                Err(e) => {
                    self.error = Some(e);
                    self.mode = Mode::Start;
                    return;
                }
            },
            Then::Open(path, recovered) => {
                let project = match recovered {
                    Some(p) => Ok(*p),
                    None => MapProject::load(&path).map_err(|e| e.to_string()),
                };
                match project {
                    Ok(p) => {
                        self.remember(&path);
                        let mut d = Doc::with(p, path, scene.clone());
                        if !d.saved || MapProject::recover(&d.path).is_some() {
                            // a recovered autosave: unsaved until saved
                            d.saved = false;
                        }
                        Some(d)
                    }
                    Err(e) => {
                        self.error = Some(e);
                        self.mode = Mode::Start;
                        return;
                    }
                }
            }
        };
        // round E3d: the doc reads a mode choice's donor from the game copy; a new project takes its target as the
        // first operation of its log (the donor's mode layer in, the base's out)
        if let Some(d) = doc.as_mut() {
            let game = self
                .opfor
                .as_ref()
                .ok()
                .and_then(|r| bc2_levelwrite::export::Game::open(&r.join("game")).ok());
            d.set_game(game.clone());
            if let (Some((profile, mode)), Some(g)) = (self.pending_target.take(), game.as_ref()) {
                match bc2_levelwrite::export::choose_mode(g, &d.project, profile, &mode, None) {
                    Ok(c) => {
                        d.commit(Op::SetMode { to: Box::new(c) }, "New project: era and mode");
                        if let Err(e) = d.save() {
                            self.error = Some(e);
                        }
                    }
                    Err(e) => self.error = Some(format!("The project's era and mode were not set: {e}")),
                }
            }
        }
        let editing = doc.is_some();
        if let Some(d) = &doc {
            // a campaign level as the base: the export's campaign checklist (round E3b)
            self.export.offer_strip(d);
        }
        // the asset browser: the level's placements, its folders', its content set's bundles' (round E2b)
        let target_set = doc.as_ref().and_then(|d| d.project.mode()).map(|m| set_of(m.profile));
        let game = self.opfor.as_ref().ok().and_then(|root| {
            let set = match target_set {
                Some(s) => s,
                None => self.levels.iter().find(|l| l.lv.eq_ignore_ascii_case(lv))?.set,
            };
            Some(crate::content::GameSource {
                opfor: root.clone(),
                set,
                levels: self
                    .levels
                    .iter()
                    .filter(|l| l.set == set)
                    .map(|l| l.lv.clone())
                    .collect(),
                own: lv.to_owned(),
            })
        });
        let content = doc
            .as_ref()
            .map(|d| crate::content::Catalogue::start(&d.scene, bases.clone(), game));
        let outliner = match &doc {
            Some(d) => Outliner::new_without(&d.scene, &d.deleted),
            None => Outliner::new(&scene),
        };
        // the teams follow the project's target era (round E3d): its layer's factions read as that era names them
        let target_profile = doc
            .as_ref()
            .and_then(|d| d.project.mode())
            .map(|m| faction_profile(m.profile));
        let edited = doc.as_ref().and_then(Doc::partition);
        let teams = match edited.as_deref().or_else(|| scene.file(0).map(|(_, d)| d)) {
            Some(data) => Teams::from_partition(data, target_profile.unwrap_or_else(|| Profile::of(lv))),
            None if lv == "demo" => crate::demo::teams(),
            None => Teams::unknown("The scene has no level partition; teams are shown as T1 and T2."),
        };
        let mut v = ViewState {
            teams,
            lv: lv.to_owned(),
            outliner,
            base: if editing { Scene::default() } else { scene },
            gpu_scene,
            selected: None,
            selection: Vec::new(),
            doc,
            workspace: if editing { Workspace::Objects } else { Workspace::View },
            tool: Tool::Move,
            space: Space::World,
            snap: Snap::default(),
            drag: None,
            box_from: None,
            ruler: Vec::new(),
            message: String::new(),
            hover: None,
            folder_input: String::new(),
            prop_from: None,
            prop_place: None,
            field_edit: None,
            viewports: HashMap::from([(Projection::Perspective, Viewport::new(camera))]),
            search: String::new(),
            notes,
            load_seconds: secs,
            reveal: false,
            labels: true,
            game_accurate: false,
            game: None,
            last_hit: None,
            filter: None,
            view_mode: crate::workspace::ViewMode::Lit,
            mode_note: String::new(),
            nav: None,
            cull: None,
            bases,
            root: self.opfor.clone().ok(),
            content,
            content_query: String::new(),
            content_pick: None,
            placing: None,
            sink: None,
            picker: None,
            context: None,
            menu_point: None,
            pointer_ray: None,
            polygon: Vec::new(),
            polygon_view: None,
            polygon_close: false,
            clipboard: Vec::new(),
            pending: Vec::new(),
            menu_rects: Vec::new(),
            menu_kind: None,
            content_rects: Vec::new(),
            place_failed: None,
            thumbs: HashMap::new(),
            browser_mode: crate::gameplay::BrowserMode::PlaceObjects,
            place_team: 1,
            tiles: None,
            show_links: true,
            inspection: None,
            fields_tree: None,
            link_back: Vec::new(),
            link_rects: Vec::new(),
            field_rects: Vec::new(),
            measuring: false,
            measure: Vec::new(),
        };
        if editing {
            v.refresh(gpu, &palette);
        }
        self.mode = Mode::View(Box::new(v));
    }

    /// Selects an entity (or none), as a click would.
    pub fn select(&mut self, gpu: &GpuCtx, entity: Option<usize>) {
        if let Mode::View(v) = &mut self.mode {
            v.select(gpu, entity, true);
        }
    }

    /// Selects entities (the last the primary), as a box selection would.
    pub fn select_many(&mut self, gpu: &GpuCtx, entities: &[usize]) {
        if let Mode::View(v) = &mut self.mode {
            v.set_selection(gpu, entities.to_vec(), true);
        }
    }

    /// Runs an action as the keyboard or palette would (tests).
    pub fn run_action(&mut self, gpu: &GpuCtx, ctx: &egui::Context, action: Action) {
        self.run(gpu, ctx, action);
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
        let p = v.scene().entities.get(entity)?.position()?;
        let size = v.marker_offset(entity);
        let view = vp.camera.view([f64::from(rect.width()), f64::from(rect.height())]);
        let px = view.project([p[0], p[1] + size, p[2]])?;
        Some(rect.min + Vec2::new(px[0] as f32, px[1] as f32))
    }

    /// Where a gizmo handle is drawn in a viewport (points): its outline's first and last points (an arm: the
    /// origin and the tip), with the last frame's view.
    #[must_use]
    pub fn gizmo_handle(&self, projection: Projection, handle: Handle) -> Option<(Pos2, Pos2)> {
        let v = self.view()?;
        let vp = v.viewports.get(&projection)?;
        let rect = vp.rect?;
        let view = vp.camera.view([f64::from(rect.width()), f64::from(rect.height())]);
        let f = v.gizmo_frame()?;
        let (_, pts) = gizmo::handles(&view, &f, v.tool)
            .into_iter()
            .find(|(h, _)| *h == handle)?;
        let at = |s: &[f64; 2]| rect.min + Vec2::new(s[0] as f32, s[1] as f32);
        Some((at(pts.first()?), at(pts.last()?)))
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
                v.apply_look(&palette);
                v.refresh(gpu, &palette);
            }
        }
        if self.renderer.is_none() {
            self.renderer = Some(render::Renderer::new(&gpu.gpu));
        }
        self.poll_loading(gpu, &ctx);
        self.poll_catalog();
        if self.catalog_rx.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        self.shortcuts(gpu, &ctx);
        let palette = self.palette();
        if self.palette_open {
            self.command_palette(gpu, &ctx, &palette);
        }
        if self.recover.is_some() {
            self.recovery_modal(&ctx, &palette);
        }
        let metrics = Metrics::new(self.tokens);
        for asked in self.top_bar(ui, &palette, &metrics) {
            match asked {
                TopBar::Run(action) => self.run(gpu, &ctx, action),
                TopBar::Help(page) => self.help.show(page),
            }
        }
        self.status_bar(ui, &palette, &metrics, gpu);
        self.help.target = match &self.mode {
            Mode::View(v) => v.doc.as_ref().and_then(|d| d.project.mode()).map(|c| {
                format!(
                    "This project is made for {} {} (the mode layer from {}): the asset browser lists {} objects, the                      Spawners and Objectives are the mode's, and the team labels are the era's. Change it in the                      Export window (Set target).",
                    c.profile.word(),
                    bc2_levelwrite::modelayer::mode_name(&c.mode),
                    c.donor.to_ascii_uppercase(),
                    c.profile.word()
                )
            }),
            _ => None,
        };
        help::window(&ctx, &mut self.help, &palette, &metrics);
        let (doc, selected) = match &mut self.mode {
            Mode::View(v) => (v.doc.as_mut(), v.selected),
            _ => (None, None),
        };
        if crate::export_ui::window(&ctx, &mut self.export, doc, selected)
            && let Mode::View(v) = &mut self.mode
        {
            // the target or a placement changed the project (round E3d): the views follow
            v.refresh(gpu, &palette);
        }
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
        let pending = match &mut self.mode {
            Mode::View(v) => std::mem::take(&mut v.pending),
            _ => Vec::new(),
        };
        for a in pending {
            self.run(gpu, &ctx, a);
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
                    let then = std::mem::replace(&mut self.after_prepare, Then::View);
                    self.load_level(&lv, then);
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
        let Mode::Loading { lv, rx, then, .. } = &self.mode else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(loaded)) => {
                let (lv, then) = (lv.clone(), then.clone());
                self.show_scene(gpu, &lv, loaded, then);
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
        // no shortcuts while typing, or while flying (W A S D Q E fly with the right button held)
        if self.palette_open
            || self.recover.is_some()
            || ctx.memory(|m| m.focused().is_some())
            || ctx.input(|i| i.pointer.secondary_down())
        {
            return;
        }
        if let Mode::View(v) = &mut self.mode {
            if v.has_transient() && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
                v.cancel_transient();
                v.redraw();
            }
            if v.polygon.len() >= 3 && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter)) {
                v.polygon_close = true;
            }
        }
        let redo2 = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT), Key::Z);
        if self.why_not(Action::Redo).is_none() && ctx.input_mut(|i| i.consume_shortcut(&redo2)) {
            self.run(gpu, ctx, Action::Redo);
        }
        // longer chords first: Ctrl+Shift+G before Ctrl+G before G
        let mut entries = actions::registry();
        entries.sort_by_key(|e| {
            std::cmp::Reverse(e.shortcut.map_or(0, |s| {
                let m = s.modifiers;
                usize::from(m.alt) + usize::from(m.ctrl || m.command || m.mac_cmd) + usize::from(m.shift)
            }))
        });
        for e in entries {
            let Some(sc) = e.shortcut else { continue };
            if self.why_not(e.action).is_none() && ctx.input_mut(|i| i.consume_shortcut(&sc)) {
                self.run(gpu, ctx, e.action);
            }
        }
    }

    /// The crash-recovery offer: the project's autosave differs from its file (the editor stopped before a save).
    fn recovery_modal(&mut self, ctx: &egui::Context, p: &Palette) {
        let Some((path, auto)) = &self.recover else { return };
        let text = format!(
            "{} has unsaved changes from the last session: its autosave holds {} operation(s) the saved project does \
             not. Recover them, or open the project as it was last saved (the autosave is then removed).",
            path.display(),
            auto.operations.len()
        );
        let mut answer = None;
        egui::Modal::new(egui::Id::new("recover"))
            .frame(
                egui::Frame::new()
                    .fill(p.surface_2)
                    .stroke(Stroke::new(1.0, p.line))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::same(16)),
            )
            .show(ctx, |ui| {
                ui.set_max_width(520.0);
                ui.label(
                    RichText::new("Recover unsaved changes")
                        .text_style(style("title"))
                        .color(p.ink),
                );
                ui.add_space(8.0);
                ui.label(RichText::new(text).color(p.ink));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(egui::Button::new(RichText::new("Recover changes").color(p.on_accent)).fill(p.accent))
                        .clicked()
                    {
                        answer = Some(true);
                    }
                    if ui.button("Open the saved project").clicked() {
                        answer = Some(false);
                    }
                });
            });
        if let Some(a) = answer {
            self.answer_recovery(a);
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
                    // the workspace switcher (help::WORKSPACES, as the Help window describes them): View, and Objects
                    // with a project open
                    let ws = match &self.mode {
                        Mode::View(v) => Some((v.workspace, v.doc.is_some())),
                        _ => None,
                    };
                    for w in Workspace::ALL {
                        let active = ws.map_or(w == Workspace::View, |(x, _)| x == w);
                        let enabled = ws.is_some_and(|(_, d)| d || !w.edits());
                        let what = help::WORKSPACES
                            .iter()
                            .find(|h| h.name == w.name())
                            .map_or("", |h| h.what);
                        let why = if enabled {
                            what.to_owned()
                        } else if ws.is_none() {
                            "Open a map first.".to_owned()
                        } else {
                            "Editing needs a map project: New map from an existing map, or Open a project.".to_owned()
                        };
                        if segment(ui, p, m, w.name(), active, enabled, &why) && !active {
                            run.push(TopBar::Run(Action::SetWorkspace(w)));
                        }
                    }
                    separator(ui, p, m);
                    if let Mode::View(v) = &self.mode {
                        // the badge, the map's name and its level in the room the buttons at the right leave (their
                        // width as last drawn): the name is cut first, never drawn under the buttons (round G4)
                        let (tag, name, sub) = match &v.doc {
                            Some(d) => (
                                d.project.target_profile().badge(),
                                d.project.name.clone(),
                                format!("{}{}", v.scene().name, if d.saved { "" } else { "  -  not saved" }),
                            ),
                            None => (profile_of(&v.lv), v.scene().name.clone(), "read-only".to_owned()),
                        };
                        let room = (ui.available_width() - self.top_right.get() - m.space_4).max(0.0);
                        let h = ui.available_height();
                        let info = ui.allocate_ui_with_layout(
                            Vec2::new(room, h),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                ui.set_max_width(room);
                                ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                                badge(ui, p, m, tag);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&name).text_style(style("body-strong")).color(p.ink),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(format!("{name}  -  {sub}"));
                                ui.add(
                                    egui::Label::new(RichText::new(sub).text_style(style("small")).color(p.ink_muted))
                                        .truncate(),
                                );
                            },
                        );
                        let s = self.top_spans.get();
                        self.top_spans.set([info.response.rect.right(), s[1]]);
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
                            ThemeId::Dark => "Light",
                            ThemeId::Light => "Dark",
                        };
                        let mut buttons: Vec<(Action, &str)> = vec![
                            (Action::ToggleTheme, theme_label),
                            (Action::ResetLayout, "Reset layout"),
                            (Action::CommandPalette, "Commands"),
                        ];
                        if let Mode::View(v) = &self.mode {
                            buttons.push((Action::CloseMap, "Close map"));
                            buttons.push((
                                Action::ToggleGameView,
                                if v.game_accurate {
                                    "Editor view"
                                } else {
                                    "Game-accurate"
                                },
                            ));
                        }
                        for (action, label) in buttons {
                            if action_button(ui, p, action, label, self.why_not(action)).clicked() {
                                run.push(TopBar::Run(action));
                            }
                        }
                        if let Mode::View(v) = &self.mode {
                            // the orthographic views in one menu (round G4: a compact top bar)
                            ui.menu_button(RichText::new("Views").color(p.ink_muted), |ui| {
                                for (proj, name) in [
                                    (Projection::Top, "Top view"),
                                    (Projection::Front, "Front view"),
                                    (Projection::Side, "Side view"),
                                ] {
                                    let action = Action::OpenViewport(proj);
                                    let tip = actions::entry(action).map_or("", |e| e.help);
                                    if ui.button(name).on_hover_text(tip).clicked() {
                                        run.push(TopBar::Run(action));
                                        ui.close();
                                    }
                                }
                            })
                            .response
                            .on_hover_text("Open the top, front or side view beside the 3D view (Alt+2, Alt+3, Alt+4)");
                            if v.doc.is_some() {
                                for (action, label) in
                                    [(Action::Redo, "Redo"), (Action::Undo, "Undo"), (Action::Save, "Save")]
                                {
                                    if action_button(ui, p, action, label, self.why_not(action)).clicked() {
                                        run.push(TopBar::Run(action));
                                    }
                                }
                            }
                        }
                        self.top_right.set(ui.min_rect().width());
                        let s = self.top_spans.get();
                        self.top_spans.set([s[0], ui.min_rect().left()]);
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
                            let live = v.scene().entities.len() - v.doc.as_ref().map_or(0, |d| d.deleted.len());
                            ui.label(small(format!(
                                "{}  -  {} entities  -  {} triangles  -  loaded in {:.1} s",
                                v.scene().name,
                                group_digits(live as u64),
                                group_digits(v.gpu_scene.triangles),
                                v.load_seconds
                            )));
                            let fly = v.viewports.values().any(|vp| vp.fly);
                            ui.label(small(format!(
                                "camera: {}",
                                if fly { "fly (right button held)" } else { "orbit" }
                            )));
                            if v.selection.len() > 1 {
                                ui.label(small(format!("{} selected", v.selection.len())));
                            }
                            if let Some(h) = &v.last_hit {
                                let what = match h.entity.and_then(|k| v.scene().entities.get(k)) {
                                    Some(e) => model::display_name(e),
                                    None => "terrain".into(),
                                };
                                ui.label(small(format!(
                                    "surface: {:.2}, {:.2}, {:.2} m ({what})",
                                    h.point[0], h.point[1], h.point[2]
                                )));
                            }
                            if !v.message.is_empty() {
                                ui.label(RichText::new(v.message.clone()).text_style(style("small")).color(p.ink));
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
}

fn load(root: &Path, lv: &str) -> Loaded {
    let t = Instant::now();
    let found = levels::load_options(root, lv)?;
    let s = scene::load(&found.options).map_err(|e| format!("{lv} could not be read: {e}"))?;
    let mut notes = found.notes;
    notes.extend(s.notes.iter().cloned());
    Ok((s, notes, t.elapsed().as_secs_f64(), found.options.bases))
}

/// The content profile of a level, for the badge.
#[must_use]
pub fn profile_of(lv: &str) -> &'static str {
    // the set's badge: the content profile's, and BFBC1 for the Bad Company 1 maps (round G3)
    ContentSet::of(lv, Profile::of(lv), true).badge()
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
/// Whether it was clicked (enabled ones only).
#[allow(
    clippy::too_many_arguments,
    reason = "a widget's look and state, as the design names them"
)]
fn segment(ui: &mut egui::Ui, p: &Palette, m: &Metrics, name: &str, active: bool, enabled: bool, why: &str) -> bool {
    let galley = ui.painter().layout_no_wrap(
        name.to_owned(),
        FontId::new(13.0, egui::FontFamily::Name(crate::fonts::SANS_SEMIBOLD.into())),
        if active || enabled { p.ink } else { p.ink_faint },
    );
    let size = Vec2::new(galley.size().x + 2.0 * m.space_3, m.control);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    if active {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(m.radius_md as u8), p.accent_subtle);
        let u = Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - 2.0), rect.right_bottom());
        ui.painter().rect_filled(u, CornerRadius::ZERO, p.accent);
    } else if enabled && resp.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(m.radius_md as u8), p.surface_3);
    }
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, p.ink);
    let clicked = enabled && resp.clicked();
    if !why.is_empty() {
        resp.on_hover_text(why);
    }
    clicked
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

/// `text` on one line, cut to `width` with an ellipsis when it is longer (the galley's `elided` says so).
fn elided(ui: &egui::Ui, text: &str, font: FontId, color: Color32, width: f32) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat {
            font_id: font,
            color,
            ..Default::default()
        },
    );
    job.wrap = egui::text::TextWrapping {
        max_width: width.max(1.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('\u{2026}'),
    };
    ui.painter().layout_job(job)
}

impl ViewState {
    /// The scene the views show: the project's (its log applied), or the level as loaded.
    #[must_use]
    pub fn scene(&self) -> &Scene {
        self.doc.as_ref().map_or(&self.base, |d| &d.scene)
    }

    /// Selects an entity (with its group, when editing), or none.
    fn select(&mut self, gpu: &GpuCtx, entity: Option<usize>, reveal: bool) {
        let list = match (entity, &self.doc) {
            (Some(k), Some(d)) => {
                let mut g = d.with_group(k);
                g.retain(|&x| x != k);
                g.push(k);
                g
            }
            (Some(k), None) => vec![k],
            (None, _) => Vec::new(),
        };
        self.set_selection(gpu, list, reveal);
    }

    /// Sets the selection (the last entity the primary).
    pub(crate) fn set_selection(&mut self, gpu: &GpuCtx, list: Vec<usize>, reveal: bool) {
        let primary = list.last().copied();
        if self.selection != list {
            self.selection = list;
            self.selected = primary;
            self.gpu_scene.set_selection(&gpu.gpu, &self.selection);
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
        let Some(e) = self.selected.and_then(|k| self.scene().entities.get(k)) else {
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
        let Some(e) = self.scene().entities.get(entity) else {
            return 0.0;
        };
        let has_mesh = self
            .scene()
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
            Tab::History => self.history(ui),
            Tab::Layers => self.layers(ui),
            Tab::Content => self.content(ui),
            Tab::Links => self.links(ui),
        }
    }

    /// The orthographic views, History and Layers can be closed (the actions open them again); the rest stay.
    fn is_closeable(&self, tab: &Tab) -> bool {
        matches!(tab, Tab::Viewport(p) if *p != Projection::Perspective)
            || matches!(tab, Tab::History | Tab::Layers | Tab::Content | Tab::Links)
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
        // editing: the left button drags the gizmo or a selection box (Alt+left orbits or pans)
        let editing = self.view.doc.is_some() && self.view.workspace.moves_objects();
        let (wheel_taken, tools_left) = self.tools_input(ui, &resp, rect, projection, &vp);
        let consumed = tools_left || (editing && self.edit_input(ui, &resp, rect, projection, &mut vp));
        if resp.dragged_by(PointerButton::Primary) && !consumed {
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
        if resp.hovered() && scroll != 0.0 && !wheel_taken {
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
            && !tools_left
            && let (Some(pos), Some(t)) = (resp.interact_pointer_pos(), vp.target.as_ref())
        {
            let view = vp.camera.view(size);
            if vp.dirty {
                self.renderer
                    .render(gpu, &self.view.gpu_scene, t, &view.matrix(), &self.palette);
                vp.dirty = false;
            }
            let at = (pos - rect.min) * ppp;
            let mut id = self
                .renderer
                .pick(gpu, t, [at.x.max(0.0) as u32, at.y.max(0.0) as u32], 4);
            // the game-accurate view picks what its own frame shows (round R5c)
            if projection == Projection::Perspective
                && self.view.game_accurate
                && let Some(g) = self.view.game.as_mut()
                && let Ok(e) = g.pick([at.x.max(0.0) as u32, at.y.max(0.0) as u32], px)
            {
                id = e.map_or(render::ID_NONE, render::entity_id);
            }
            let hit = if id == render::ID_NONE {
                None
            } else {
                let (o, d) = view.ray([f64::from(at.x), f64::from(at.y)]);
                let skip = |k: usize| self.view.doc.as_ref().is_some_and(|d| !d.shown(k));
                query::ray_hit_except(self.view.scene(), o, d, &skip)
            };
            if self.view.measuring {
                // measuring: the click adds the surface point under it, the selection stays
                if let Some(h) = hit {
                    self.view.measure.push(h.point);
                }
            } else if editing {
                let ctrl = ui.input(|i| i.modifiers.command || i.modifiers.shift);
                self.edit_click(render::id_entity(id), ctrl, hit);
            } else {
                self.view.select(self.gpu, render::id_entity(id), true);
            }
            self.view.last_hit = hit;
            if resp.double_clicked() {
                self.view.focus_selected();
                if let Some(e) = self.view.selected.and_then(|k| self.view.scene().entities.get(k))
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
        let v = &mut *self.view;
        let game = v
            .game
            .as_mut()
            .filter(|_| projection == Projection::Perspective && v.game_accurate);
        if let Some(g) = game {
            // the game frame follows the scene's edits and outlines the selection (round R5c)
            let scene = v.doc.as_ref().map_or(&v.base, |d| &d.scene);
            g.paint(ui, Some(self.gpu), rect, &vp.camera, px, scene, &v.selection);
        } else if let Some(tex) = vp.texture {
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
        if self.view.show_links {
            self.links_overlay(ui, rect, &vp.camera);
        }
        self.measure_overlay(ui, rect, &vp.camera);
        if editing {
            self.edit_overlay(ui, rect, projection, &vp.camera);
        }
        self.tools_overlay(ui, rect, projection, &vp.camera);
        self.view_menu(&resp);
        // the controls, in the corner (from Help's list, so the two never differ; editing adds its own), on the
        // view's label plate
        let painter = ui.painter().with_clip_rect(rect);
        let m = &self.metrics;
        let hint_text = if editing {
            editui::EDITING_HINT.to_owned()
        } else {
            help::corner_hint(ortho)
        };
        let hint = painter.layout_no_wrap(hint_text, FontId::proportional(11.0), self.palette.ink);
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
        for e in &v.scene().entities {
            if v.doc.as_ref().is_some_and(|d| !d.shown(e.index)) {
                continue;
            }
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
            let Some(e) = v.scene().entities.get(index) else {
                continue;
            };
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
        let total = self.view.scene().entities.len() - self.view.doc.as_ref().map_or(0, |d| d.deleted.len());
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
        let mut menus: Vec<(egui::Response, usize)> = Vec::new();
        let query = self.view.search.clone();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let v = &*self.view;
            if v.doc.is_some() {
                editui::folders_section(ui, v, &p, &m, &query, &mut clicked);
            }
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
                                    clicked: &mut Option<(usize, bool)>,
                                    menus: &mut Vec<(egui::Response, usize)>| {
                            for e in rows.get(range).unwrap_or_default() {
                                let layer = v.doc.as_ref().and_then(|d| {
                                    d.guid(e.entity).and_then(|g| d.org.layer(g)).map(|l| l.state.colour)
                                });
                                let resp = outliner_row(
                                    ui,
                                    &p,
                                    &m,
                                    e,
                                    &v.teams,
                                    v.selection.contains(&e.entity),
                                    hidden || v.doc.as_ref().is_some_and(|d| !d.shown(e.entity)),
                                    layer,
                                );
                                if reveal == Some(e.entity) {
                                    resp.scroll_to_me(Some(Align::Center));
                                }
                                if resp.secondary_clicked() || resp.context_menu_opened() {
                                    menus.push((resp.clone(), e.entity));
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
                                .show_rows(ui, row_h, rows.len(), |ui, range| {
                                    show(ui, range, &mut clicked, &mut menus);
                                });
                        } else {
                            show(ui, 0..rows.len(), &mut clicked, &mut menus);
                        }
                    });
                }
            }
        });
        for (g, hide) in toggles {
            self.view.gpu_scene.set_hidden(&self.gpu.gpu, &g, hide);
            self.view.redraw();
        }
        for (resp, entity) in menus {
            self.row_menu(&resp, entity);
        }
        if let Some((entity, focus)) = clicked {
            let add = ui.input(|i| i.modifiers.command || i.modifiers.shift);
            if add && self.view.doc.is_some() {
                let mut list = self.view.selection.clone();
                if let Some(at) = list.iter().position(|&k| k == entity) {
                    list.remove(at);
                } else {
                    list.push(entity);
                }
                self.view.set_selection(self.gpu, list, false);
            } else {
                self.view.select(self.gpu, Some(entity), false);
            }
            if focus {
                self.view.focus_selected();
            }
        }
    }

    fn properties(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let m = self.metrics;
        if self.view.doc.is_some() && self.view.editable_selection().len() > 1 {
            self.multi_properties(ui);
            return;
        }
        let Some(e) = self
            .view
            .selected
            .and_then(|k| self.view.scene().entities.get(k))
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
                            // the project's target era (round E3d), else the level's
                            let era = self.view.doc.as_ref().and_then(|d| d.project.mode()).map_or_else(
                                || Profile::of(&self.view.lv).word().to_owned(),
                                |m| {
                                    format!(
                                        "{} (target; the base is {})",
                                        m.profile.word(),
                                        Profile::of(&self.view.lv).word()
                                    )
                                },
                            );
                            ui.label(RichText::new(era).color(p.ink));
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
                    if self.view.doc.is_some() {
                        self.edit_properties(ui, &e);
                    }
                    ui.add_space(m.space_3);
                    ui.horizontal(|ui| {
                        if ui.button("Focus").on_hover_text("Moves the camera to it (F)").clicked() {
                            self.view.focus_selected();
                        }
                        if ui.button("Copy GUID").clicked() {
                            ui.ctx().copy_text(e.guid.clone());
                        }
                    });
                    self.all_fields(ui, e.index);
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
                let doc_problems: &[String] = self.view.doc.as_ref().map_or(&[], |d| d.problems.as_slice());
                let autosave = self.view.doc.as_ref().and_then(|d| d.autosave_error.clone());
                if self.view.notes.is_empty() && doc_problems.is_empty() && autosave.is_none() {
                    ui.label(
                        RichText::new(if self.view.doc.is_some() {
                            "No problems: every edit applies to the level partition."
                        } else {
                            "No problems. A read-only map has nothing to check."
                        })
                        .color(p.ink_muted),
                    );
                }
                for n in doc_problems {
                    status_line(ui, p, "Error", p.status_error, n);
                }
                if let Some(a) = autosave {
                    status_line(
                        ui,
                        p,
                        "Warning",
                        p.status_warn,
                        &format!("The autosave could not be written ({a}). Save the project to keep your changes."),
                    );
                }
                for n in &self.view.notes {
                    status_line(ui, p, "Information", p.status_info, n);
                }
            });
        let _ = self.tokens;
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "a row's look and state, as the design names them"
)]
fn outliner_row(
    ui: &mut egui::Ui,
    p: &Palette,
    m: &Metrics,
    e: &model::Entry,
    teams: &Teams,
    selected: bool,
    hidden: bool,
    layer: Option<[u8; 3]>,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), m.row), Sense::click());
    let painter = ui.painter();
    if let Some([r, g, b]) = layer {
        // the layer's colour: a bar at the row's right edge
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(rect.right() - 3.0, rect.top() + 3.0),
                Vec2::new(3.0, rect.height() - 6.0),
            ),
            CornerRadius::ZERO,
            Color32::from_rgb(r, g, b),
        );
    }
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

/// The editor's content set of an era (round E3d: the asset browser lists the target era's objects).
#[must_use]
pub fn set_of(p: bc2_levelwrite::Profile) -> ContentSet {
    match p {
        bc2_levelwrite::Profile::Bfbc2 => ContentSet::Bfbc2,
        bc2_levelwrite::Profile::Bfbc1 => ContentSet::Bfbc1,
        bc2_levelwrite::Profile::Vietnam => ContentSet::Vietnam,
        bc2_levelwrite::Profile::Ww2 => ContentSet::Ww2,
    }
}

/// The faction profile of an era (the team labels: `FactionRUS` is RU, NVA or JP).
#[must_use]
pub fn faction_profile(p: bc2_levelwrite::Profile) -> Profile {
    match p {
        bc2_levelwrite::Profile::Vietnam => Profile::Vietnam,
        bc2_levelwrite::Profile::Ww2 => Profile::Ww2,
        bc2_levelwrite::Profile::Bfbc2 | bc2_levelwrite::Profile::Bfbc1 => Profile::Bfbc2,
    }
}

/// The multiplayer modes of each era, from the game copy's stock levels (`bc2_levelwrite::profile::modes`).
pub type EraModes =
    std::collections::BTreeMap<bc2_levelwrite::Profile, std::collections::BTreeMap<String, Vec<String>>>;
