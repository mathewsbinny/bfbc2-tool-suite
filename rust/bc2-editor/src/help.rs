//! The Help window (the top bar's Help menu, F1; `EDITOR_ARCHITECTURE.md` 3.10, "Decisions on G1's first version"):
//! controls and shortcuts, a getting-started guide for new users, and what each workspace is for; searchable.
//!
//! Nothing here is written twice. The shortcuts come from the action registry ([`crate::actions::registry`]: its
//! names, shortcuts and help lines), the mouse and keyboard controls of the views from [`VIEW_CONTROLS`] (which the
//! views' corner hint is made from too: [`corner_hint`]), and the workspaces from [`WORKSPACES`] (which the top bar's
//! switcher is made from). A new action is in Help as soon as it is in the registry; one without a description fails
//! the tests.

use eframe::egui::{self, Align, Color32, CornerRadius, Layout, Margin, ModifierNames, RichText, Sense, Stroke, Vec2};

use crate::actions;
use crate::theme::{Metrics, Palette};

/// A page of the Help window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Page {
    /// Controls and shortcuts.
    #[default]
    Controls,
    /// The getting-started guide.
    GettingStarted,
    /// What each workspace is for.
    Workspaces,
}

impl Page {
    /// Every page, in the window's order.
    pub const ALL: [Self; 3] = [Self::GettingStarted, Self::Controls, Self::Workspaces];

    /// Its name (the Help menu's items, the window's tabs).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Controls => "Controls and shortcuts",
            Self::GettingStarted => "Getting started",
            Self::Workspaces => "Workspaces",
        }
    }
}

/// Which views a control works in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Views {
    /// Every view.
    All,
    /// The 3D (perspective) view.
    Perspective,
    /// The top, front and side views.
    Orthographic,
}

/// A mouse or keyboard control of the views (not an action of the registry: these act where the pointer is).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Control {
    /// What it does.
    pub name: &'static str,
    /// The keys or buttons.
    pub keys: &'static str,
    /// Which views.
    pub views: Views,
    /// A line of help.
    pub help: &'static str,
}

/// The views' controls, as `app.rs`'s viewport handles them.
pub const VIEW_CONTROLS: [Control; 17] = [
    Control {
        name: "Orbit",
        keys: "Left drag",
        views: Views::Perspective,
        help: "Turns the camera around the point it looks at",
    },
    Control {
        name: "Pan",
        keys: "Middle drag",
        views: Views::Perspective,
        help: "Slides the camera sideways and up or down",
    },
    Control {
        name: "Pan",
        keys: "Left or middle drag",
        views: Views::Orthographic,
        help: "Slides the top, front or side view",
    },
    Control {
        name: "Zoom",
        keys: "Wheel",
        views: Views::All,
        help: "Moves the camera nearer or further",
    },
    Control {
        name: "Fly",
        keys: "Hold right, W A S D, Q E",
        views: Views::Perspective,
        help: "Looks around with the mouse while the right button is held; W A S D move, Q E go down and up, Shift \
               goes faster",
    },
    Control {
        name: "Walk (player's eye camera)",
        keys: "Alt+W, then W A S D, drag",
        views: Views::Perspective,
        help: "A soldier's eye on the ground (the game's eye height and field of view): W A S D walk, a drag looks, \
               Shift sprints, C crouches, Esc returns to the camera before",
    },
    Control {
        name: "Select",
        keys: "Click",
        views: Views::All,
        help: "Selects what is under the pointer and outlines it in orange, through walls; a click on the sky \
               selects nothing",
    },
    Control {
        name: "Select and focus",
        keys: "Double click",
        views: Views::All,
        help: "Selects what is under the pointer and moves the camera to it",
    },
    Control {
        name: "Keep a camera bookmark",
        keys: "Ctrl+1 - Ctrl+9",
        views: Views::All,
        help: "Keeps where this view looks, per level and view (the pointer over the view)",
    },
    Control {
        name: "Go to a camera bookmark",
        keys: "1 - 9",
        views: Views::All,
        help: "Moves this view's camera to a bookmark kept with Ctrl",
    },
    Control {
        name: "Context menu",
        keys: "Right click",
        views: Views::All,
        help: "A menu of what can be done to the object under the pointer (selected first) or to the ground there; its entries follow the workspace, and greyed ones say why",
    },
    Control {
        name: "Sink into the surface",
        keys: "Alt+wheel",
        views: Views::All,
        help: "Sinks the selection (or the object being placed) 0.1 m a notch into the surface below it, 0.01 m with Shift; a popup shows the depth and its mode, and each burst is one undo step",
    },
    Control {
        name: "Place from the asset browser",
        keys: "Drag an entry into a view",
        views: Views::All,
        help: "The object follows the pointer on the surface; let go to place it (one operation)",
    },
    Control {
        name: "Preview an asset",
        keys: "Click a tile (Shift+click: compare)",
        views: Views::All,
        help: "The Asset preview panel shows the blueprint's mesh, or a mesh picked in the browser's Meshes mode: drag to turn, right-drag to move, wheel to zoom; Side, Top and Front are the mesh viewer's pictures; click a part to select it",
    },
    Control {
        name: "Place the object being placed",
        keys: "Click",
        views: Views::All,
        help: "After Place object here: the object follows the pointer; a click places it, Esc cancels",
    },
    Control {
        name: "Turn the object being placed",
        keys: "Shift+wheel",
        views: Views::All,
        help: "15 degrees a notch about its up axis; 1 degree with Ctrl held too",
    },
    Control {
        name: "Polygon corner",
        keys: "Click (Polygon select)",
        views: Views::All,
        help: "Adds a corner; Enter or a double click selects what is inside, Esc cancels",
    },
];

/// A workspace of the top bar's switcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Workspace {
    /// Its name.
    pub name: &'static str,
    /// Whether it can be used in this version.
    pub available: bool,
    /// What it is for.
    pub what: &'static str,
    /// When it comes (empty when it is here).
    pub when: &'static str,
}

/// The workspaces, in the switcher's order (`EDITOR_ARCHITECTURE.md` 3.9).
pub const WORKSPACES: [Workspace; 6] = [
    Workspace {
        name: "View",
        available: true,
        what: "Look at any map of your game copy, read-only: fly around it, find objects in the outliner, read their properties. Nothing is changed.",
        when: "",
    },
    Workspace {
        name: "Objects",
        available: true,
        what: "Edit a map project's buildings, props, vehicles, effects and sounds, all edited alike: place them from the asset browser (Place Objects: every placeable object of the content set), move, rotate, sink into the ground (Alt+wheel), duplicate (deep duplicate copies what an object owns), hide, lock and delete; a category filter limits what can be selected; snapping, align, distribute, the ruler, layers, folders and groups; every edit undoable in History.",
        when: "",
    },
    Workspace {
        name: "Terrain",
        available: true,
        what: "Sculpt the terrain with brushes - raise, lower, smooth, flatten to the height where the stroke began, noise, and a ramp between two clicked points - each with a radius, a strength and a falloff, and paint its physics material (footsteps, impacts, dust) with one of the terrain's own materials. Hold the left button on the ground to paint; letting go keeps the stroke as one step of History (Ctrl+Z undoes it). The game-accurate view follows, also while a stroke is made. Paint layer lays one of the terrain's visual layers (with its paired material when chosen), Shade darkens or lightens its colour / occlusion map, Re-shade sculpted area scales that map by the change in sky visibility, Widen range raises the height range with the ground kept, Add material adds a material of the level's grid; a sculpt at the edge reaches the sub-terrain ring. The export writes the sculpted height field (its min / max tree and lower envelope rebuilt), the ring stitched to it, the painted material map, visual terrain, mask atlas and colour map.",
        when: "",
    },
    Workspace {
        name: "Vegetation",
        available: false,
        what: "Scatter and erase vegetation that passes the safe-to-place rules. The workspace and its actions are here; its brushes are greyed until then.",
        when: "Its brushes come in a later round.",
    },
    Workspace {
        name: "Gameplay",
        available: true,
        what: "The game mode's flags, spawns, vehicle spawns, objectives and areas: only they can be selected; move them, give them a team from the context menu, deep duplicate a flag with its capture area; place new ones from the asset browser's Spawners and Objectives modes (with the team picker's team and the sub-objects they own). The Gameplay tool (chosen in the Gameplay panel) adds a flag, a spawn point, a soldier spawner (to the selected or nearest flag or base) or an M-COM where you click, wired as the level's own, adds a whole Rush stage after the last (a copy of the last stage with its bases, spawns and combat areas where you click) or between two (the panel's Insert after: a copy of the next stage, played between them), or carries one with what belongs to it; Delete takes a flag, spawner or M-COM with its unit. The Gameplay panel lists the capture points and Rush stages, renames the selected flag (DICE's form, \"E: Harbor\": the export makes its deploy-screen text), takes a Rush stage out, sets teams and edits the round's rules (tickets, capture time, lives, kills) with DICE's values; the Problems panel is the mode's checklist (what the layer still lacks).",
        when: "",
    },
    Workspace {
        name: "AI",
        available: true,
        what: "The bots' orders, AI spawns and area points: only they can be selected. The level's navigation is drawn here: NavKit's build and DICE's file side by side, the ground coloured by island, sector, pitch, difference or anomaly, each layer switchable in the Navigation panel; Islands and spawns lists the islands and the spawns off the main network to fly to; the path test (Shift+P) finds each file's route between two clicks; a reference level's AI layer can be drawn over the map. Navigation edits (regions, seeds, links, route edges, traversals, notes) are drawn with the Navigation edits panel's tools and kept in the project; Build with these edits runs NavKit, Install and Restore show what they write in the game copy before they run. Placing spawners and orders, routes and the cross-domain links are planned.",
        when: "",
    },
];

/// The getting-started guide: (title, text) steps.
pub const GETTING_STARTED: [(&str, &str); 15] = [
    (
        "Start: three paths",
        "The start screen offers three buttons. Open project opens a map project you saved (a .mapedit.json file; \
         your recent projects are listed). Edit existing level shows the four modes - BFBC1, BFBC2, Vietnam and \
         WW2 - and then that mode's multiplayer maps. Create level makes a new level, from any existing level or from \
         a height map. Back (or Start) goes up a step. Nothing in your game copy is ever changed: a project holds \
         only your edits.",
    ),
    (
        "Edit an existing level",
        "Choose a mode card, then a map: Open views it read-only, New project starts a project on it (a name and a \
         folder). Only multiplayer maps are listed here - each level's description says its game mode; campaign \
         levels are under Create level. A level opened for the first time is prepared first - its objects and terrain \
         read - which takes up to a minute; the progress shows each step.",
    ),
    (
        "Create a level",
        "From existing levels lists every level of every mode, multiplayer and campaign, with a search: choose one, \
         give the new level a name and a folder, and edit it; when it is exported, a campaign base's campaign parts \
         are stripped (the export's checklist). From a height map takes a 16-bit PNG or RAW height map (1024 or 2048 \
         samples a side, or 1025 / 2049), the map's size, height range and water level, an optional colour map and \
         the mode its content comes from; the project records them, and the terrain is generated in a later round.",
    ),
    (
        "Look around",
        "Drag with the left button to orbit, with the middle button to pan, and turn the wheel to zoom. Hold the \
         right button to fly: move the mouse to look, W A S D to move, Q and E to go down and up.",
    ),
    (
        "Find things",
        "Click an object in the 3D view, or a row in the Outliner on the left, to select it; Properties on the right \
         shows what it is. F moves the camera to the selection. Search the Outliner by name, type, GUID or \
         blueprint, and hide whole groups you do not need.",
    ),
    (
        "Read the colours",
        "Each team's spawns, flags and vehicles are in its colour with its faction's label (US and RU, US and NVA \
         in Vietnam, US and JP in the Pacific War). Flags no team holds yet are light grey with NEU; objectives and \
         combat areas no team owns are violet. The selection is outlined in orange.",
    ),
    (
        "Use more views",
        "Alt+2, Alt+3 and Alt+4 open the top, front and side views beside the 3D view, each with its own camera. \
         Ctrl+1 to Ctrl+9 keep a camera bookmark, 1 to 9 go back to it.",
    ),
    (
        "Show what the data holds",
        "The View menu's Overlays switch on the level's effects, lights and sounds with their reach (a light's radius \
         and colour, a sound's minimum distance, an effect's spawn volume), its combat areas (the out-of-bounds \
         edge as a wall), the flags' capture areas and the spawns' enemy-check radius, each with labels. Player's \
         eye camera (Alt+W) walks the map at a soldier's eye with the game's field of view. A destructible \
         object's State in Properties (Page Up / Page Down) steps through its health states, its collision with \
         them where its files split it per part.",
    ),
    (
        "Show one game mode or Rush stage",
        "The Mode switch over the 3D view shows every object, the level's game mode layer only (its flags, bases, \
         M-COMs, spawns, combat areas and their logic) or the level without it, and a Rush level one stage at a time \
         as it is played: the other stages' M-COMs and the bases, combat areas and spawns not in play are hidden, in \
         every view and the outliner. It is a view setting: the project and the export keep every object.",
    ),
    (
        "See the weather",
        "The Weather switch over the 3D view lists every state of the level's weather system (night, dawn, fog, rain \
         and the cutscene states are marked, with the spawn-time switch - what players see - and the settings' \
         ActiveState). The game-accurate view draws the state's light, exposure and sky and opens on the level's own \
         state; the simplified view is tinted by it. The Export window's Weather section makes the exported level \
         start in one of them, in another level's state (keeping this level's sky, or with that level's whole sky) \
         or at night. Night and rain are not yet seen in game: they build only with Build it anyway.",
    ),
    (
        "Arrange the window",
        "Drag a panel's tab to dock it elsewhere, put it beside another as a tab, or float it. Reset layout puts \
         everything back. The theme button switches between Dark and Light.",
    ),
    (
        "Find any command",
        "Ctrl+Shift+P opens the command palette: type a few letters of what you want to do and press Enter. Every \
         button of the top bar is there too, with its shortcut.",
    ),
    (
        "Export and install",
        "With a project open, Export as a multiplayer level (command palette) opens the Export window: the era and \
         game mode, the map's name and level id. Its checks run as you type; Export stays greyed, with the reason, \
         until they pass. Export builds the new level into a stage folder in Project OPFOR's work folder and checks \
         it. Install... shows what an install changes in the game copy, Install now (with the game closed) adds the \
         level under its new name, and Uninstall... takes it out again. The game's own levels are never \
         overwritten.",
    ),
    (
        "Name the level",
        "The Export window's Names: the level id (what the game loads), its folder (the id in lower case), the name \
         the launcher and the server browser show, and the on-screen name and description - one for every language, \
         or each language its own (Per language). They are checked together as you type, with the reason: an id of \
         the game's own levels or another project's export, an id holding _NAME (a black loading screen), a name \
         another map has, a letter a language cannot show. Keep in project puts them in the project (Export does \
         too).",
    ),
    (
        "Compare and preview",
        "Compare with base (command palette, or the Diff panel) lists what the project changed: objects added, \
         removed, moved and changed, filtered by kind, group and text; click one to fly there. The 3D view tints them \
         and marks with a box where an object of the other level stands. Compare with level does the same against \
         any level. After an export, Preview export opens it read only beside the original level with the same \
         camera: T switches between them, Close preview goes back to the project.",
    ),
];

/// One entry of the Help window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Topic {
    /// Its page.
    pub page: Page,
    /// The heading it sits under.
    pub section: &'static str,
    /// Its title.
    pub title: String,
    /// Its keys (controls) or state (workspaces), empty for guide steps.
    pub keys: String,
    /// Its text.
    pub body: String,
}

/// A shortcut in words, as the window and the tests write it (`Ctrl+Shift+P`, `F1`).
#[must_use]
pub fn shortcut_text(sc: &egui::KeyboardShortcut) -> String {
    sc.format(&ModifierNames::NAMES, false)
}

/// Every entry of the Help window, in page order: the controls of the views, then every action of the registry
/// (with its shortcut, or where to find it), then the guide's steps, then the workspaces.
#[must_use]
pub fn topics() -> Vec<Topic> {
    let mut out = Vec::new();
    // grouped by the views they work in: the 3D view, the orthographic views, every view
    let mut controls = VIEW_CONTROLS.to_vec();
    controls.sort_by_key(|c| match c.views {
        Views::Perspective => 0,
        Views::Orthographic => 1,
        Views::All => 2,
    });
    for c in controls {
        let section = match c.views {
            Views::All => "In every view",
            Views::Perspective => "In the 3D view",
            Views::Orthographic => "In the top, front and side views",
        };
        out.push(Topic {
            page: Page::Controls,
            section,
            title: c.name.into(),
            keys: c.keys.into(),
            body: c.help.into(),
        });
    }
    for e in actions::registry() {
        out.push(Topic {
            page: Page::Controls,
            section: "Commands",
            title: e.name.into(),
            keys: e
                .shortcut
                .as_ref()
                .map_or_else(|| "Command palette".to_owned(), shortcut_text),
            body: e.help.into(),
        });
    }
    for (k, (title, text)) in GETTING_STARTED.iter().enumerate() {
        out.push(Topic {
            page: Page::GettingStarted,
            section: "A first look at a map",
            title: format!("{}. {title}", k + 1),
            keys: String::new(),
            body: (*text).into(),
        });
    }
    for w in WORKSPACES {
        out.push(Topic {
            page: Page::Workspaces,
            section: "Workspaces",
            title: w.name.into(),
            keys: if w.available { "Available".into() } else { w.when.into() },
            body: w.what.into(),
        });
    }
    out
}

/// The topics a search finds: every word of the query in the title, keys, text or heading, ignoring case.
#[must_use]
pub fn search<'a>(topics: &'a [Topic], query: &str) -> Vec<&'a Topic> {
    let q = query.trim().to_lowercase();
    topics
        .iter()
        .filter(|t| {
            let hay = format!("{} {} {} {}", t.title, t.keys, t.body, t.section).to_lowercase();
            q.split_whitespace().all(|w| hay.contains(w))
        })
        .collect()
}

/// The hint in a view's corner: its controls and the two commands a newcomer needs (focus, the palette), from
/// [`VIEW_CONTROLS`] and the registry.
#[must_use]
pub fn corner_hint(orthographic: bool) -> String {
    let mut parts: Vec<String> = VIEW_CONTROLS
        .iter()
        .filter(|c| match c.views {
            Views::All => {
                !c.name.contains("bookmark")
                    && c.name != "Select"
                    && c.name != "Select and focus"
                    && c.name != "Preview an asset"
            }
            Views::Perspective => !orthographic,
            Views::Orthographic => orthographic,
        })
        .map(|c| {
            format!(
                "{}: {}",
                c.name,
                c.keys.to_lowercase().replace("w a s d, q e", "W A S D, Q E")
            )
        })
        .collect();
    for a in [actions::Action::FocusSelection, actions::Action::Help] {
        if let Some(e) = actions::entry(a)
            && let Some(sc) = e.shortcut
        {
            parts.push(format!("{}: {}", e.name, shortcut_text(&sc)));
        }
    }
    parts.join("   ")
}

/// The Help window's state.
#[derive(Debug, Clone, Default)]
pub struct HelpState {
    /// Shown.
    pub open: bool,
    /// The page shown when there is no search.
    pub page: Page,
    /// The search.
    pub query: String,
    /// The open project's target in words (round E3d: the era and mode its panels follow), shown above the pages.
    pub target: Option<String>,
}

impl HelpState {
    /// Opens the window at `page`.
    pub fn show(&mut self, page: Page) {
        self.open = true;
        self.page = page;
    }
}

fn caption(text: &str, color: Color32) -> RichText {
    RichText::new(text.to_uppercase())
        .text_style(egui::TextStyle::Name("label".into()))
        .color(color)
}

/// The Help window: a floating panel (the design's dialog: `surface-1`, `radius-lg`, `shadow-popover`) with the
/// search above the page tabs.
pub fn window(ctx: &egui::Context, state: &mut HelpState, p: &Palette, m: &Metrics) {
    if !state.open {
        return;
    }
    let mut open = true;
    let frame = egui::Frame::new()
        .fill(p.surface_1)
        .stroke(Stroke::new(1.0, p.line))
        .corner_radius(CornerRadius::same(m.radius_lg as u8))
        .shadow(ctx.global_style().visuals.window_shadow)
        .inner_margin(Margin::same(m.space_4 as i8));
    egui::Window::new(RichText::new("Help").text_style(egui::TextStyle::Name("section".into())))
        .id(egui::Id::new("help window"))
        .open(&mut open)
        .frame(frame)
        .default_size(Vec2::new(680.0, 540.0))
        .min_size(Vec2::new(420.0, 300.0))
        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
        .collapsible(false)
        .resizable(true)
        .show(ctx, |ui| {
            if let Some(t) = &state.target {
                ui.label(RichText::new(t).color(p.ink_muted));
                ui.add_space(m.space_2);
            }
            ui.add(
                egui::TextEdit::singleline(&mut state.query)
                    .hint_text(RichText::new("Search help (fly, bookmark, faction ...)").color(p.ink_faint))
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(m.space_2);
            let searching = !state.query.trim().is_empty();
            ui.horizontal(|ui| {
                for page in Page::ALL {
                    let active = !searching && state.page == page;
                    let (rect, resp) = ui.allocate_exact_size(
                        Vec2::new(
                            ui.painter()
                                .layout_no_wrap(page.name().into(), egui::FontId::proportional(13.0), p.ink)
                                .size()
                                .x
                                + 2.0 * m.space_3,
                            m.control,
                        ),
                        Sense::click(),
                    );
                    if active {
                        ui.painter()
                            .rect_filled(rect, CornerRadius::same(m.radius_md as u8), p.accent_subtle);
                        ui.painter().rect_filled(
                            egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - 2.0), rect.right_bottom()),
                            CornerRadius::ZERO,
                            p.accent,
                        );
                    } else if resp.hovered() {
                        ui.painter()
                            .rect_filled(rect, CornerRadius::same(m.radius_md as u8), p.surface_2);
                    }
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        page.name(),
                        egui::FontId::new(13.0, egui::FontFamily::Name(crate::fonts::SANS_SEMIBOLD.into())),
                        if active { p.ink } else { p.ink_muted },
                    );
                    if resp.clicked() {
                        state.page = page;
                        state.query.clear();
                    }
                }
            });
            ui.painter()
                .hline(ui.min_rect().x_range(), ui.cursor().top(), Stroke::new(1.0, p.line));
            ui.add_space(m.space_3);
            let all = topics();
            let shown: Vec<&Topic> = if searching {
                search(&all, &state.query)
            } else {
                all.iter().filter(|t| t.page == state.page).collect()
            };
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                if searching {
                    ui.label(
                        RichText::new(format!(
                            "{} {} for \"{}\"",
                            shown.len(),
                            if shown.len() == 1 { "entry" } else { "entries" },
                            state.query.trim()
                        ))
                        .color(p.ink_muted),
                    );
                    ui.add_space(m.space_2);
                }
                let mut section = "";
                for t in shown {
                    let heading = if searching { t.page.name() } else { t.section };
                    if heading != section {
                        section = heading;
                        ui.add_space(m.space_2);
                        ui.label(caption(heading, p.ink_muted));
                        ui.add_space(m.space_1);
                    }
                    topic_row(ui, p, m, t);
                }
                if searching && search(&all, &state.query).is_empty() {
                    ui.label(RichText::new("Nothing in Help matches. Try fewer words.").color(p.ink_muted));
                }
                // About: the suite's version (the VERSION file, round P1)
                ui.add_space(m.space_4);
                ui.label(
                    RichText::new(format!("BFBC2 Tool Suite {} - map editor", crate::version())).color(p.ink_faint),
                );
            });
        });
    if !open {
        state.open = false;
    }
}

fn topic_row(ui: &mut egui::Ui, p: &Palette, m: &Metrics, t: &Topic) {
    egui::Frame::new()
        .inner_margin(Margin::symmetric(0, m.space_1 as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(&t.title)
                        .text_style(egui::TextStyle::Name("body-strong".into()))
                        .color(p.ink),
                );
                if !t.keys.is_empty() {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let colour = match (t.page, t.keys.as_str()) {
                            (Page::Workspaces, "Available") => p.status_ok,
                            (Page::Workspaces, _) => p.ink_muted,
                            _ => p.ink,
                        };
                        egui::Frame::new()
                            .fill(p.surface_2)
                            .stroke(Stroke::new(1.0, p.line))
                            .corner_radius(CornerRadius::same(m.radius_sm as u8))
                            .inner_margin(Margin::symmetric(m.space_1 as i8 + 2, 1))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(&t.keys)
                                        .text_style(egui::TextStyle::Name("value".into()))
                                        .color(colour),
                                );
                            });
                    });
                }
            });
            ui.add(egui::Label::new(RichText::new(&t.body).color(p.ink_muted)).wrap());
        });
    ui.painter().hline(
        ui.min_rect().x_range(),
        ui.cursor().top() - 1.0,
        Stroke::new(1.0, p.line),
    );
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Help is generated from the registry: every action is there once, under its own name, with its shortcut (or
    /// the palette) and its description; an action without a description fails here.
    #[test]
    fn every_action_is_in_help() {
        let t = topics();
        for e in actions::registry() {
            assert!(
                e.help.trim().len() >= 12 && e.help.chars().next().is_some_and(char::is_uppercase),
                "{:?} has no real description: {:?}",
                e.action,
                e.help
            );
            assert!(!e.help.ends_with('.'), "{:?}: a help line has no full stop", e.action);
            let hits: Vec<&Topic> = t
                .iter()
                .filter(|x| x.section == "Commands" && x.title == e.name)
                .collect();
            assert_eq!(hits.len(), 1, "{} in Help", e.name);
            let h = hits.first().unwrap();
            assert_eq!(h.body, e.help);
            match e.shortcut {
                Some(sc) => assert_eq!(h.keys, shortcut_text(&sc)),
                None => assert_eq!(h.keys, "Command palette"),
            }
        }
        let commands = t.iter().filter(|x| x.section == "Commands").count();
        assert_eq!(commands, actions::registry().len());
        // the views' controls and the workspaces are there, each with a description
        for c in VIEW_CONTROLS {
            assert!(c.help.len() >= 12, "{}", c.name);
            assert!(t.iter().any(|x| x.title == c.name && x.keys == c.keys));
        }
        for w in WORKSPACES {
            assert!(w.available == w.when.is_empty(), "{}", w.name);
            assert!(t.iter().any(|x| x.page == Page::Workspaces && x.title == w.name));
        }
        assert_eq!(
            t.iter().filter(|x| x.page == Page::GettingStarted).count(),
            GETTING_STARTED.len()
        );
    }

    #[test]
    fn search_and_hint() {
        let t = topics();
        let fly = search(&t, "FLY");
        assert!(fly.iter().any(|x| x.title == "Fly"));
        assert!(search(&t, "f1").iter().any(|x| x.title == "Help" && x.keys == "F1"));
        assert!(search(&t, "ctrl+shift+p").iter().any(|x| x.title == "Command palette"));
        assert!(search(&t, "faction").iter().any(|x| x.page == Page::GettingStarted));
        assert!(search(&t, "gameplay checklist").iter().any(|x| x.title == "Gameplay"));
        assert!(
            search(&t, "alt+wheel")
                .iter()
                .any(|x| x.title == "Sink into the surface")
        );
        assert!(search(&t, "right click").iter().any(|x| x.title == "Context menu"));
        assert_eq!(search(&t, "no such words anywhere"), Vec::<&Topic>::new());
        assert_eq!(search(&t, "").len(), t.len());
        let h = corner_hint(false);
        assert!(
            h.contains("Orbit: left drag") && h.contains("Fly: hold right, W A S D, Q E"),
            "{h}"
        );
        assert!(h.contains("Focus selection: F") && h.contains("Help: F1"), "{h}");
        let o = corner_hint(true);
        assert!(
            o.contains("Pan: left or middle drag") && !o.contains("Fly") && !o.contains("Orbit"),
            "{o}"
        );
    }
}
