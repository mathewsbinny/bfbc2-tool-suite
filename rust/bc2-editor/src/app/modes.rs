//! The start screen (round G4, the user's design): three paths.
//!
//! - **Open project:** a saved `.mapedit.json` - its path, the recent projects, the projects in the projects folder;
//!   an autosave newer than the file is offered first (crash recovery).
//! - **Edit existing level:** the four mode cards of round G3 (BFBC1, BFBC2, Vietnam, WW2: each with its picture from
//!   the game copy, [`crate::catalog`]), then that mode's **multiplayer** maps (thumbnail, name, game mode, prepared or
//!   not; Open views a map, New project starts a project on it). WW2 lists OPFOR's Pacific War maps.
//! - **Create level:** from existing levels - every level of every mode, multiplayer and campaign, grouped by mode,
//!   with a search; choosing one starts a new project on it (a name and a folder) - or from a height map
//!   (`create.rs`).
//!
//! Multiplayer or campaign is decided from each level's description: its `GameMode` (a campaign mode,
//! `gamemodes/singelplayer...`, or any other: [`crate::catalog::LevelKind`]). A level whose description was not read
//! is listed under Create level only. Every start opens on the three paths (nothing remembered); `--mode` opens a
//! mode's map list at once.

use bc2_levelwrite::BaseKind;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Stroke, TextureHandle, TextureOptions,
    Vec2,
};

use super::editui::{self, FormText, StartChoice, StartForm};
use super::{EditorApp, Then, elided, status_line, style};
use crate::catalog::{LevelKind, Picture};
use crate::error::EditorError;
use crate::levels::{ContentSet, LevelInfo};
use crate::theme::{Metrics, Palette};

/// The cards' order on the screen.
pub const CARD_ORDER: [ContentSet; 4] = [
    ContentSet::Bfbc1,
    ContentSet::Bfbc2,
    ContentSet::Vietnam,
    ContentSet::Ww2,
];

/// The start screen's pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartPage {
    /// The three paths.
    Home,
    /// Open project.
    OpenProject,
    /// Edit existing level: the mode cards.
    EditModes,
    /// Edit existing level: a mode's multiplayer maps.
    EditMaps(ContentSet),
    /// Create level: from existing levels, or from a height map.
    CreateChoice,
    /// Create level - from existing levels: every level, grouped by mode.
    CreateFromExisting,
    /// Create level - from a height map: the form.
    CreateFromHeightmap,
    /// A height-map project, opened or just made (its terrain is generated in a later round).
    ImportProject,
}

impl StartPage {
    /// The page Back goes to.
    #[must_use]
    pub fn parent(self) -> Option<Self> {
        match self {
            Self::Home => None,
            Self::OpenProject | Self::EditModes | Self::CreateChoice | Self::ImportProject => Some(Self::Home),
            Self::EditMaps(_) => Some(Self::EditModes),
            Self::CreateFromExisting | Self::CreateFromHeightmap => Some(Self::CreateChoice),
        }
    }

    /// Its name in the trail at the top.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::Home => "Start",
            Self::OpenProject => "Open project",
            Self::EditModes => "Edit existing level",
            Self::EditMaps(set) => set.word(),
            Self::CreateChoice => "Create level",
            Self::CreateFromExisting => "From existing levels",
            Self::CreateFromHeightmap => "From a height map",
            Self::ImportProject => "Height-map project",
        }
    }

    /// The pages from the start to this one.
    #[must_use]
    pub fn trail(self) -> Vec<Self> {
        let mut out = vec![self];
        while let Some(up) = out.last().and_then(|p| p.parent()) {
            out.push(up);
        }
        out.reverse();
        out
    }
}

/// What a map list row asked for.
enum RowChoice {
    Open(String),
    New(String),
}

/// A row's buttons: Open and New project (Edit existing level), or New project only (Create level).
#[derive(Clone, Copy, PartialEq, Eq)]
enum RowButtons {
    OpenAndNew,
    NewOnly,
}

/// The three paths' cards: label, text.
const PATHS: [(&str, &str); 3] = [
    (
        "Open project",
        "Open a map project you saved (.mapedit.json). Your recent projects are listed.",
    ),
    (
        "Edit existing level",
        "Choose a mode - BFBC1, BFBC2, Vietnam or WW2 - then one of its multiplayer maps: view it, or start a project \
         on it.",
    ),
    (
        "Create level",
        "A new level from any existing level, multiplayer or campaign, or from a height map of your own.",
    ),
];

impl EditorApp {
    /// The texture of a picture of the catalog (made once, kept by `key`).
    fn picture(&mut self, ctx: &egui::Context, key: &str, pic: Option<&Picture>) -> Option<TextureHandle> {
        if let Some(t) = self.catalog_state.pictures.get(key) {
            return Some(t.clone());
        }
        let pic = pic?;
        let img = egui::ColorImage::from_rgba_unmultiplied(pic.size, &pic.rgba);
        let t = ctx.load_texture(key, img, TextureOptions::LINEAR);
        self.catalog_state.pictures.insert(key.to_owned(), t.clone());
        Some(t)
    }

    /// The start screen: the page shown, under the trail back to the start.
    pub(super) fn start_screen(&mut self, ui: &mut egui::Ui, p: &Palette, m: &Metrics) {
        self.start_ui.start_buttons.clear();
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(p.ground)
                    .inner_margin(egui::Margin::same(m.space_8 as i8)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    // at most 1200 wide, never wider than the window
                    let column = ui.available_width().min(1200.0);
                    ui.set_max_width(column);
                    if let Some(e) = self.error.clone() {
                        status_line(ui, p, "Error", p.status_error, &e);
                        ui.add_space(m.space_2);
                    }
                    if let Err(e) = &self.opfor {
                        let e = e.clone();
                        status_line(ui, p, "Error", p.status_error, &e.to_string());
                        if !matches!(self.start_ui.start, StartPage::OpenProject | StartPage::ImportProject) {
                            // without a game copy, only projects (a height-map project needs none to be shown)
                            self.start_ui.start = StartPage::OpenProject;
                        }
                        // the DBX inspector needs no game copy either (the home page that offers it is not shown)
                        ui.add_space(m.space_2);
                        self.dbx_link(ui, p);
                        ui.add_space(m.space_2);
                    }
                    if self.start_ui.start != StartPage::Home {
                        self.trail(ui, p, m);
                        ui.add_space(m.space_4);
                    }
                    match self.start_ui.start {
                        StartPage::Home => self.home(ui, p, m, column),
                        StartPage::OpenProject => self.open_page(ui, p, m),
                        StartPage::EditModes => self.mode_cards(ui, p, m, column),
                        StartPage::EditMaps(set) => self.map_list(ui, p, m, set, column),
                        StartPage::CreateChoice => self.create_choice(ui, p, m, column),
                        StartPage::CreateFromExisting => self.all_levels(ui, p, m, column),
                        StartPage::CreateFromHeightmap => self.heightmap_page(ui, p, m, column),
                        StartPage::ImportProject => self.import_page(ui, p, m, column),
                    }
                });
            });
        self.new_project_modal(ui.ctx(), p, m);
    }

    /// Back, and the trail of pages from the start ("Start / Edit existing level / BFBC2"), each a link back.
    fn trail(&mut self, ui: &mut egui::Ui, p: &Palette, m: &Metrics) {
        let mut go = None;
        ui.horizontal(|ui| {
            let b = ui
                .add(egui::Button::new("\u{2190} Back").min_size(Vec2::new(0.0, m.control)))
                .on_hover_text("Up one step");
            self.start_ui.start_buttons.push(("Back".into(), b.rect));
            if b.clicked() {
                go = self.start_ui.start.parent();
            }
            ui.add_space(m.space_3);
            let trail = self.start_ui.start.trail();
            for (k, page) in trail.iter().enumerate() {
                if k > 0 {
                    ui.label(RichText::new("/").color(p.ink_faint));
                }
                if k + 1 == trail.len() {
                    ui.label(
                        RichText::new(page.title())
                            .text_style(style("body-strong"))
                            .color(p.ink),
                    );
                } else {
                    let l = ui.add(egui::Button::new(RichText::new(page.title()).color(p.accent_text)).frame(false));
                    self.start_ui
                        .start_buttons
                        .push((format!("Trail:{}", page.title()), l.rect));
                    if l.clicked() {
                        go = Some(*page);
                    }
                }
            }
        });
        if let Some(page) = go {
            self.go(page);
        }
    }

    /// The three paths.
    fn home(&mut self, ui: &mut egui::Ui, p: &Palette, m: &Metrics, column: f32) {
        ui.label(
            RichText::new("BFBC2 Map Editor")
                .text_style(style("display"))
                .color(p.ink),
        );
        ui.add_space(m.space_1);
        ui.label(
            RichText::new(
                "Open a project, edit an existing level, or create a new one. Game files are never changed: \
                           a project holds only your edits.",
            )
            .color(p.ink_muted),
        );
        ui.add_space(m.space_6);
        let gap = m.space_4;
        let cols = if column >= 3.0 * 260.0 + 2.0 * gap { 3 } else { 1 };
        let card_w = ((column - gap * (cols as f32 - 1.0)) / cols as f32).floor();
        let card_h = 132.0;
        let mut chosen = None;
        for row in PATHS.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (label, text) in row {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(card_w, card_h), Sense::click());
                    path_card(ui, p, m, rect, label, text, resp.hovered());
                    self.start_ui.start_buttons.push(((*label).to_owned(), rect));
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        chosen = Some(*label);
                    }
                }
            });
            ui.add_space(gap);
        }
        match chosen {
            Some("Open project") => self.go(StartPage::OpenProject),
            Some("Edit existing level") => self.go(StartPage::EditModes),
            Some("Create level") => self.go(StartPage::CreateChoice),
            _ => {}
        }
        if let Some(r) = self.start_ui.recent.first().cloned() {
            ui.add_space(m.space_2);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Last project:").color(p.ink_muted));
                let name = r
                    .file_name()
                    .map_or_else(|| r.display().to_string(), |n| n.to_string_lossy().into_owned());
                let b = ui
                    .add_enabled(
                        r.is_file(),
                        egui::Button::new(RichText::new(name).color(p.accent_text)).frame(false),
                    )
                    .on_hover_text(r.display().to_string());
                self.start_ui.start_buttons.push(("Last project".into(), b.rect));
                if b.clicked() {
                    self.open_project(&r);
                }
            });
        }
        // round GR4: the DBX inspector (DbxEditGUI folded in) needs no level
        ui.add_space(m.space_2);
        self.dbx_link(ui, p);
    }

    /// The link that opens the DBX inspector (on the home page, or under the error when there is no game copy).
    fn dbx_link(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let b = ui
            .add(egui::Button::new(RichText::new("Inspect a DBX file\u{2026}").color(p.accent_text)).frame(false))
            .on_hover_text("Any binary .dbx - a file or a bundle's entry: its tree, edits, checks, XML");
        self.start_ui.start_buttons.push(("Inspect a DBX file".into(), b.rect));
        if b.clicked() {
            self.dbx.open = true;
        }
    }

    /// Open project.
    fn open_page(&mut self, ui: &mut egui::Ui, p: &Palette, m: &Metrics) {
        let found = self.start_ui.found_projects.clone();
        let folder = self.new_project.new_folder.clone();
        let choice = editui::open_card(
            ui,
            p,
            m,
            &mut StartForm {
                new_level: &mut self.new_project.new_level,
                new_name: &mut self.new_project.new_name,
                new_folder: &mut self.new_project.new_folder,
                open_path: &mut self.start_ui.open_path,
                recent: &self.start_ui.recent,
                buttons: &mut self.start_ui.start_buttons,
                target: None,
                modes: None,
            },
            &found,
            &folder,
        );
        if let Some(StartChoice::Open(path)) = choice {
            self.open_project(&path);
        }
    }

    /// Edit existing level: the mode cards, each with its multiplayer map count.
    fn mode_cards(&mut self, ui: &mut egui::Ui, p: &Palette, m: &Metrics, column: f32) {
        ui.label(RichText::new("Choose a mode").text_style(style("display")).color(p.ink));
        ui.add_space(m.space_1);
        ui.label(
            RichText::new(
                "Then one of its multiplayer maps, to view it or to start a project on it. Campaign levels \
                           are under Create level.",
            )
            .color(p.ink_muted),
        );
        ui.add_space(m.space_6);
        // four in a row, two by two when narrow; the cards scale with the window
        let gap = m.space_4;
        let cols = if column >= 4.0 * 230.0 + 3.0 * gap { 4 } else { 2 };
        let card_w = ((column - gap * (cols as f32 - 1.0)) / cols as f32).floor();
        let pic_h = (card_w * 9.0 / 16.0).floor();
        let card_h = pic_h + 64.0;
        let ctx = ui.ctx().clone();
        let known = self.catalog_state.catalog.is_some();
        let mut chosen = None;
        for row in CARD_ORDER.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for set in row {
                    let installed = self.levels.iter().filter(|l| l.set == *set).count();
                    let n = if known { self.edit_levels(*set).len() } else { installed };
                    let art = self
                        .catalog_state
                        .catalog
                        .as_ref()
                        .and_then(|c| c.cards.get(set))
                        .cloned();
                    let tex = self.picture(&ctx, &format!("card:{}", set.word()), art.as_ref());
                    let sense = if n > 0 { Sense::click() } else { Sense::hover() };
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(card_w, card_h), sense);
                    let count = match (n, known) {
                        (0, _) if installed == 0 => "Not installed".to_owned(),
                        (0, _) => "No multiplayer map".to_owned(),
                        (n, true) => format!("{n} multiplayer map{}", if n == 1 { "" } else { "s" }),
                        (n, false) => format!("{n} maps installed"),
                    };
                    mode_card(ui, p, m, rect, *set, n, &count, tex.as_ref(), resp.hovered() && n > 0);
                    self.start_ui.start_buttons.push((format!("Mode:{}", set.word()), rect));
                    let resp = if n > 0 {
                        resp.on_hover_text(format!("Show the {} multiplayer maps", set.word()))
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                    } else {
                        resp.on_hover_text(format!(
                            "No {} multiplayer map is installed in this game copy",
                            set.word()
                        ))
                    };
                    if resp.clicked() {
                        chosen = Some(*set);
                    }
                }
            });
            ui.add_space(gap);
        }
        if let Some(set) = chosen {
            self.choose_mode(Some(set));
        }
    }

    /// A search box `width` wide.
    fn search_box(&mut self, ui: &mut egui::Ui, p: &Palette, hint: &str, width: f32) {
        ui.add(
            egui::TextEdit::singleline(&mut self.start_ui.level_filter)
                .hint_text(RichText::new(hint).color(p.ink_faint))
                .desired_width(width),
        );
    }

    /// "Reading each map's game mode": the lists wait for the catalog (it says multiplayer or campaign).
    fn waiting_for_modes(&self, ui: &mut egui::Ui, p: &Palette, m: &Metrics) -> bool {
        if self.catalog_state.catalog.is_some() {
            return false;
        }
        ui.add_space(m.space_3);
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(RichText::new("Reading each map's game mode from the game copy...").color(p.ink_muted));
        });
        true
    }

    /// Edit existing level: a mode's multiplayer maps.
    fn map_list(&mut self, ui: &mut egui::Ui, p: &Palette, m: &Metrics, set: ContentSet, column: f32) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(set.word()).text_style(style("display")).color(p.ink));
            ui.add_space(m.space_3);
            self.search_box(ui, p, "Search maps", (column * 0.3).clamp(140.0, 320.0));
        });
        ui.label(
            RichText::new(
                "Multiplayer maps (each level's description names its game mode). Open views a map; New \
                           project starts a project on it.",
            )
            .text_style(style("small"))
            .color(p.ink_muted),
        );
        ui.add_space(m.space_4);
        if self.waiting_for_modes(ui, p, m) {
            return;
        }
        let f = self.start_ui.level_filter.trim().to_lowercase();
        let rows: Vec<LevelInfo> = self
            .edit_levels(set)
            .into_iter()
            .filter(|l| f.is_empty() || self.matches(l, &f))
            .collect();
        let asked = self.level_rows(ui, p, m, &rows, RowButtons::OpenAndNew, column);
        match asked {
            Some(RowChoice::Open(lv)) => self.open_level(&lv),
            Some(RowChoice::New(lv)) => self.new_project.new_level = Some(lv),
            None => {}
        }
    }

    /// Create level: the two ways.
    fn create_choice(&mut self, ui: &mut egui::Ui, p: &Palette, m: &Metrics, column: f32) {
        ui.label(RichText::new("Create level").text_style(style("display")).color(p.ink));
        ui.add_space(m.space_1);
        ui.label(
            RichText::new(
                "Start from a level of the game, or from a height map of your own. Either way you then edit \
                           the new level and export it.",
            )
            .color(p.ink_muted),
        );
        ui.add_space(m.space_6);
        let gap = m.space_4;
        let cols = if column >= 2.0 * 300.0 + gap { 2 } else { 1 };
        let card_w = ((column - gap * (cols as f32 - 1.0)) / cols as f32).floor();
        let ways = [
            (
                "From existing levels",
                "Every level of every mode - multiplayer and campaign - to start from. A campaign level's campaign \
                 parts are stripped when it is exported.",
            ),
            (
                "From a height map",
                "A 16-bit PNG or RAW height map, or a terrain generated from a seed (Generate), on a base level \
                 whose mode layer and look it takes. Export writes the new terrain.",
            ),
        ];
        let mut chosen = None;
        for row in ways.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (label, text) in row {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(card_w, 132.0), Sense::click());
                    path_card(ui, p, m, rect, label, text, resp.hovered());
                    self.start_ui.start_buttons.push(((*label).to_owned(), rect));
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        chosen = Some(*label);
                    }
                }
            });
            ui.add_space(gap);
        }
        match chosen {
            Some("From existing levels") => self.go(StartPage::CreateFromExisting),
            Some("From a height map") => self.go(StartPage::CreateFromHeightmap),
            _ => {}
        }
    }

    /// Create level - from existing levels: every level, grouped by mode, with the search.
    fn all_levels(&mut self, ui: &mut egui::Ui, p: &Palette, m: &Metrics, column: f32) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("From existing levels")
                    .text_style(style("display"))
                    .color(p.ink),
            );
            ui.add_space(m.space_3);
            self.search_box(ui, p, "Search levels", (column * 0.3).clamp(140.0, 320.0));
        });
        ui.label(
            RichText::new(
                "Every level of every mode, multiplayer and campaign. Choose one to start a new level from \
                           it.",
            )
            .text_style(style("small"))
            .color(p.ink_muted),
        );
        ui.add_space(m.space_4);
        if self.waiting_for_modes(ui, p, m) {
            return;
        }
        let f = self.start_ui.level_filter.trim().to_lowercase();
        let mut asked = None;
        let mut shown = 0;
        for set in CARD_ORDER {
            let rows: Vec<LevelInfo> = self
                .create_levels(set)
                .into_iter()
                .filter(|l| f.is_empty() || self.matches(l, &f))
                .collect();
            if rows.is_empty() {
                continue;
            }
            shown += rows.len();
            let (mp, sp) = rows.iter().fold((0, 0), |(a, b), l| match self.level_kind(&l.lv) {
                LevelKind::Multiplayer => (a + 1, b),
                LevelKind::Campaign => (a, b + 1),
                LevelKind::Unknown => (a, b),
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new(set.word()).text_style(style("title")).color(p.ink));
                ui.label(
                    RichText::new(format!(
                        "{} level{}  -  {mp} multiplayer, {sp} campaign",
                        rows.len(),
                        if rows.len() == 1 { "" } else { "s" }
                    ))
                    .text_style(style("small"))
                    .color(p.ink_muted),
                );
            });
            ui.add_space(m.space_2);
            if let Some(c) = self.level_rows(ui, p, m, &rows, RowButtons::NewOnly, column) {
                asked = Some(c);
            }
            ui.add_space(m.space_6);
        }
        if shown == 0 {
            ui.label(RichText::new("No level matches the search.").color(p.ink_muted));
        }
        if let Some(RowChoice::New(lv) | RowChoice::Open(lv)) = asked {
            self.new_project.new_level = Some(lv);
        }
    }

    /// A framed list of level rows; what was clicked.
    fn level_rows(
        &mut self,
        ui: &mut egui::Ui,
        p: &Palette,
        m: &Metrics,
        rows: &[LevelInfo],
        buttons: RowButtons,
        column: f32,
    ) -> Option<RowChoice> {
        let ctx = ui.ctx().clone();
        let mut asked = None;
        let mut rects = Vec::new();
        let shown: Vec<(LevelInfo, Option<TextureHandle>, String, LevelKind)> = rows
            .iter()
            .map(|l| {
                let thumb = self
                    .catalog_state
                    .catalog
                    .as_ref()
                    .and_then(|c| c.thumbs.get(&l.lv))
                    .cloned();
                let tex = self.picture(&ctx, &format!("thumb:{}", l.lv), thumb.as_ref());
                let mode = self
                    .catalog_state
                    .catalog
                    .as_ref()
                    .and_then(|c| c.modes.get(&l.lv))
                    .cloned()
                    .unwrap_or_default();
                (l.clone(), tex, mode, self.level_kind(&l.lv))
            })
            .collect();
        egui::Frame::new()
            .fill(p.surface_1)
            .stroke(Stroke::new(1.0, p.line))
            .corner_radius(CornerRadius::same(m.radius_md as u8))
            .show(ui, |ui| {
                ui.set_width(column - 2.0);
                if shown.is_empty() {
                    ui.add_space(m.space_3);
                    ui.label(RichText::new("   No map matches the search.").color(p.ink_muted));
                    ui.add_space(m.space_3);
                }
                for (l, tex, mode, kind) in &shown {
                    if let Some(c) = map_row(ui, p, m, l, mode, *kind, tex.as_ref(), buttons, &mut rects) {
                        asked = Some(c);
                    }
                }
            });
        self.start_ui.start_buttons.extend(rects);
        asked
    }

    /// The new project form, over the list it was asked from: Edit existing level's "Start project", Create level's
    /// "Create level" (with a campaign base's note).
    fn new_project_modal(&mut self, ctx: &egui::Context, p: &Palette, m: &Metrics) {
        let Some(lv) = self.new_project.new_level.clone() else {
            return;
        };
        self.read_era_modes();
        let creating = self.start_ui.start == StartPage::CreateFromExisting;
        let campaign = self.level_kind(&lv) == LevelKind::Campaign;
        let text = FormText {
            title: if creating {
                format!("New level from {}", lv.to_uppercase())
            } else {
                format!("New project on {}", lv.to_uppercase())
            },
            button: if creating { "Create level" } else { "Start project" },
            note: campaign.then_some(
                "A campaign level: when the new level is exported as a multiplayer level, the export strips its \
                 campaign parts (its checklist lists each).",
            ),
        };
        let mut choice = None;
        let modal = egui::Modal::new(egui::Id::new("new project"))
            .frame(
                egui::Frame::new()
                    .fill(p.surface_2)
                    .stroke(Stroke::new(1.0, p.line))
                    .corner_radius(CornerRadius::same(m.radius_lg as u8))
                    .inner_margin(egui::Margin::same(m.space_4 as i8)),
            )
            .show(ctx, |ui| {
                ui.set_width(520.0);
                choice = editui::new_map_form(
                    ui,
                    p,
                    m,
                    &mut StartForm {
                        new_level: &mut self.new_project.new_level,
                        new_name: &mut self.new_project.new_name,
                        new_folder: &mut self.new_project.new_folder,
                        open_path: &mut self.start_ui.open_path,
                        recent: &self.start_ui.recent,
                        buttons: &mut self.start_ui.start_buttons,
                        target: Some(&mut self.new_project.new_target),
                        modes: self.new_project.era_modes.as_ref().and_then(|r| r.as_ref().ok()),
                    },
                    &text,
                );
            });
        if modal.should_close() {
            self.new_project.new_level = None;
        }
        if let Some(StartChoice::Create(lv, name, folder)) = choice {
            self.new_project.new_level = None;
            let target = self
                .new_project
                .new_target
                .0
                .map(|e| (e, self.new_project.new_target.1.clone()));
            self.open_level_then(&lv, Then::Create(name, folder, target));
        }
    }

    // ------------------------------------------------------------------------------------------ the flow's state

    /// Shows a page of the start screen; the search and a half-made new project are cleared.
    pub fn go(&mut self, page: StartPage) {
        self.start_ui.start = page;
        self.start_ui.level_filter.clear();
        self.new_project.new_level = None;
        if page == StartPage::OpenProject {
            self.start_ui.found_projects = project_files(std::path::Path::new(self.new_project.new_folder.trim()));
        }
        if page != StartPage::ImportProject {
            self.start_ui.import_shown = None;
        }
    }

    /// The start screen's page.
    #[must_use]
    pub fn start_page(&self) -> StartPage {
        self.start_ui.start
    }

    /// Shows a mode's multiplayer maps (`Some`) or the mode cards (`None`) of "Edit existing level".
    pub fn choose_mode(&mut self, set: Option<ContentSet>) {
        self.go(set.map_or(StartPage::EditModes, StartPage::EditMaps));
    }

    /// The mode whose maps "Edit existing level" shows (`None`: another page).
    #[must_use]
    pub fn chosen_mode(&self) -> Option<ContentSet> {
        match self.start_ui.start {
            StartPage::EditMaps(set) => Some(set),
            _ => None,
        }
    }

    /// Where a start screen button was drawn in the last frame, by its label (`Open project`, `Mode:WW2`,
    /// `Open:mp_005`, `New project:sp_01`, `Start project`, `Back` ...).
    #[must_use]
    pub fn start_button(&self, label: &str) -> Option<Rect> {
        self.start_ui
            .start_buttons
            .iter()
            .find(|(l, _)| l == label)
            .map(|(_, r)| *r)
    }

    /// The labels of the start screen's buttons drawn in the last frame.
    #[must_use]
    pub fn start_button_labels(&self) -> Vec<String> {
        self.start_ui.start_buttons.iter().map(|(l, _)| l.clone()).collect()
    }

    /// A level's kind from its description (`Unknown` until the catalog is read).
    #[must_use]
    pub fn level_kind(&self, lv: &str) -> LevelKind {
        self.catalog_state
            .catalog
            .as_ref()
            .map_or(LevelKind::Unknown, |c| c.kind(lv))
    }

    /// The kind a new project on `lv` records.
    #[must_use]
    pub fn base_kind(&self, lv: &str) -> Option<BaseKind> {
        match self.level_kind(lv) {
            LevelKind::Multiplayer => Some(BaseKind::Multiplayer),
            LevelKind::Campaign => Some(BaseKind::Campaign),
            LevelKind::Unknown => None,
        }
    }

    /// "Edit existing level": a mode's multiplayer levels.
    #[must_use]
    pub fn edit_levels(&self, set: ContentSet) -> Vec<LevelInfo> {
        self.levels
            .iter()
            .filter(|l| l.set == set && self.level_kind(&l.lv) == LevelKind::Multiplayer)
            .cloned()
            .collect()
    }

    /// "Create level - from existing levels": every level of a mode.
    #[must_use]
    pub fn create_levels(&self, set: ContentSet) -> Vec<LevelInfo> {
        self.levels.iter().filter(|l| l.set == set).cloned().collect()
    }

    /// Whether a level matches a search (lower case): its name, its game mode, multiplayer / campaign.
    fn matches(&self, l: &LevelInfo, f: &str) -> bool {
        let mode = self
            .catalog_state
            .catalog
            .as_ref()
            .and_then(|c| c.modes.get(&l.lv))
            .map_or_else(String::new, |s| s.to_lowercase());
        l.lv.contains(f) || mode.contains(f) || self.level_kind(&l.lv).word().to_lowercase().contains(f)
    }

    /// Fills the new project form (the tests: what a user types).
    pub fn set_new_project(&mut self, name: &str, folder: &std::path::Path) {
        name.clone_into(&mut self.new_project.new_name);
        self.new_project.new_folder = folder.display().to_string();
    }

    /// Opens the new project form for a level (what its row's New project button does): its target the base's own
    /// on a multiplayer level, Conquest of the base's era on a campaign level (round E3d).
    pub fn start_new_project(&mut self, lv: &str) {
        self.new_project.new_level = Some(lv.to_lowercase());
        self.new_project.new_target = if self.level_kind(lv) == LevelKind::Campaign {
            let set = self
                .levels
                .iter()
                .find(|l| l.lv.eq_ignore_ascii_case(lv))
                .map(|l| l.set);
            let era = match set {
                Some(ContentSet::Vietnam) => bc2_levelwrite::Profile::Vietnam,
                Some(ContentSet::Ww2) => bc2_levelwrite::Profile::Ww2,
                Some(ContentSet::Bfbc1) => bc2_levelwrite::Profile::Bfbc1,
                _ => bc2_levelwrite::Profile::Bfbc2,
            };
            (Some(era), "conquest".into())
        } else {
            (None, "conquest".into())
        };
    }

    /// Sets the new project form's target (the tests: what a user picks).
    pub fn set_new_target(&mut self, era: Option<bc2_levelwrite::Profile>, mode: &str) {
        self.new_project.new_target = (era, mode.to_owned());
    }

    /// Reads the game copy's modes per era once (the target pickers).
    pub fn read_era_modes(&mut self) {
        if self.new_project.era_modes.is_some() {
            return;
        }
        self.new_project.era_modes = Some(match &self.opfor {
            Ok(root) => bc2_levelwrite::export::Game::open(&root.join("game"))
                .and_then(|g| bc2_levelwrite::profile::modes(&g))
                .map_err(|e| EditorError::Level(e.to_string())),
            Err(e) => Err(e.clone()),
        });
    }

    /// The level the new project form is open for.
    #[must_use]
    pub fn new_project_level(&self) -> Option<&str> {
        self.new_project.new_level.as_deref()
    }

    /// The height-map project shown on the start screen.
    #[must_use]
    pub fn import_project(&self) -> Option<&bc2_levelwrite::MapProject> {
        self.start_ui.import_shown.as_ref().map(|(_, p)| p.as_ref())
    }
}

/// The project files (`*.mapedit.json`) of a folder, newest first.
#[must_use]
pub fn project_files(folder: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(rd) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut out: Vec<(std::time::SystemTime, std::path::PathBuf)> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name().is_some_and(|n| {
                    n.to_string_lossy()
                        .to_lowercase()
                        .ends_with(bc2_levelwrite::project::EXTENSION)
                })
        })
        .map(|p| {
            let t = std::fs::metadata(&p)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            (t, p)
        })
        .collect();
    out.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    out.into_iter().map(|(_, p)| p).collect()
}

/// A path card (the start's three, Create level's two): its icon, its name, its text.
fn path_card(ui: &egui::Ui, p: &Palette, m: &Metrics, rect: Rect, label: &str, text: &str, hovered: bool) {
    let painter = ui.painter();
    let radius = CornerRadius::same(m.radius_lg as u8);
    painter.rect_filled(rect, radius, if hovered { p.surface_2 } else { p.surface_1 });
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(1.0, if hovered { p.accent } else { p.line }),
        egui::StrokeKind::Inside,
    );
    let icon = Rect::from_min_size(rect.min + Vec2::splat(m.space_4), Vec2::splat(40.0));
    painter.rect_filled(icon, CornerRadius::same(m.radius_md as u8), p.accent_subtle);
    draw_icon(painter, icon.shrink(10.0), label, p.accent_text);
    painter.text(
        Pos2::new(icon.right() + m.space_3, icon.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::new(18.0, egui::FontFamily::Name(crate::fonts::SANS_SEMIBOLD.into())),
        p.ink,
    );
    let body = ui.painter().layout(
        text.to_owned(),
        FontId::proportional(13.0),
        p.ink_muted,
        rect.width() - 2.0 * m.space_4,
    );
    painter.galley(
        Pos2::new(rect.left() + m.space_4, icon.bottom() + m.space_3),
        body,
        p.ink_muted,
    );
}

/// The small drawn icon of a path card (IBM Plex has no such glyphs): a folder, a pencil, a plus, a picture, a
/// height profile.
fn draw_icon(painter: &egui::Painter, r: Rect, label: &str, c: Color32) {
    let s = Stroke::new(2.0, c);
    let at = |x: f32, y: f32| Pos2::new(r.left() + x * r.width(), r.top() + y * r.height());
    match label {
        "Open project" => {
            painter.line_segment([at(0.0, 0.15), at(0.4, 0.15)], s);
            painter.line_segment([at(0.4, 0.15), at(0.5, 0.3)], s);
            painter.rect_stroke(
                Rect::from_min_max(at(0.0, 0.3), at(1.0, 0.95)),
                CornerRadius::same(2),
                s,
                egui::StrokeKind::Middle,
            );
        }
        "Edit existing level" => {
            painter.line_segment([at(0.1, 0.9), at(0.85, 0.15)], Stroke::new(4.0, c));
            painter.line_segment([at(0.0, 1.0), at(0.15, 0.95)], s);
            painter.line_segment([at(0.55, 1.0), at(1.0, 1.0)], s);
        }
        "From a height map" => {
            painter.add(egui::Shape::line(
                vec![at(0.0, 0.9), at(0.25, 0.45), at(0.45, 0.65), at(0.7, 0.1), at(1.0, 0.9)],
                s,
            ));
            painter.line_segment([at(0.0, 0.95), at(1.0, 0.95)], s);
        }
        "From existing levels" => {
            for (y0, y1) in [(0.0, 0.4), (0.55, 0.95)] {
                painter.rect_stroke(
                    Rect::from_min_max(at(0.0, y0), at(1.0, y1)),
                    CornerRadius::same(2),
                    s,
                    egui::StrokeKind::Middle,
                );
            }
        }
        _ => {
            painter.line_segment([at(0.5, 0.0), at(0.5, 1.0)], Stroke::new(3.0, c));
            painter.line_segment([at(0.0, 0.5), at(1.0, 0.5)], Stroke::new(3.0, c));
        }
    }
}

/// A mode card: its picture (or the plain tile), its name and map count; greyed without maps.
#[allow(clippy::too_many_arguments, reason = "a card's parts")]
fn mode_card(
    ui: &egui::Ui,
    p: &Palette,
    m: &Metrics,
    rect: Rect,
    set: ContentSet,
    n: usize,
    count: &str,
    tex: Option<&TextureHandle>,
    hovered: bool,
) {
    let painter = ui.painter();
    let radius = CornerRadius::same(m.radius_lg as u8);
    painter.rect_filled(rect, radius, if hovered { p.surface_2 } else { p.surface_1 });
    let pic = Rect::from_min_size(rect.min, Vec2::new(rect.width(), rect.height() - 64.0)).shrink(1.0);
    let top = CornerRadius {
        nw: radius.nw,
        ne: radius.ne,
        sw: 0,
        se: 0,
    };
    if let Some(t) = tex {
        // cover: the picture's middle, cropped to the card's shape
        let [tw, th] = t.size().map(|v| v as f32);
        let (want, have) = (pic.width() / pic.height(), tw / th.max(1.0));
        let uv = if have > want {
            let w = want / have;
            Rect::from_min_max(Pos2::new((1.0 - w) * 0.5, 0.0), Pos2::new(f32::midpoint(1.0, w), 1.0))
        } else {
            let h = have / want;
            Rect::from_min_max(Pos2::new(0.0, (1.0 - h) * 0.5), Pos2::new(1.0, f32::midpoint(1.0, h)))
        };
        let tint = if n > 0 { Color32::WHITE } else { Color32::from_gray(90) };
        painter.add(egui::epaint::RectShape::filled(pic, top, tint).with_texture(t.id(), uv));
    } else {
        // the plain tile: the accent's subtle fill and the set's badge
        painter.rect_filled(pic, top, if n > 0 { p.accent_subtle } else { p.surface_2 });
        painter.text(
            pic.center(),
            Align2::CENTER_CENTER,
            set.badge(),
            FontId::proportional((pic.height() / 6.0).clamp(14.0, 40.0)),
            if n > 0 { p.accent_text } else { p.ink_faint },
        );
    }
    let x = rect.left() + m.space_3;
    let y = pic.bottom() + m.space_3;
    painter.text(
        Pos2::new(x, y),
        Align2::LEFT_TOP,
        set.word(),
        FontId::new(17.0, egui::FontFamily::Name(crate::fonts::SANS_SEMIBOLD.into())),
        if n > 0 { p.ink } else { p.ink_faint },
    );
    painter.text(
        Pos2::new(x, y + 24.0),
        Align2::LEFT_TOP,
        count,
        FontId::proportional(12.0),
        if n > 0 { p.ink_muted } else { p.ink_faint },
    );
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(1.0, if hovered { p.accent } else { p.line }),
        egui::StrokeKind::Inside,
    );
}

/// One level of a list: thumbnail, name, game mode, multiplayer or campaign (Create level), prepared or not, its
/// buttons; what was clicked. The buttons' rects go to `rects` ("Open:<lv>", "New project:<lv>").
#[allow(clippy::too_many_arguments, reason = "a row's parts")]
fn map_row(
    ui: &mut egui::Ui,
    p: &Palette,
    m: &Metrics,
    l: &LevelInfo,
    mode: &str,
    kind: LevelKind,
    tex: Option<&TextureHandle>,
    buttons: RowButtons,
    rects: &mut Vec<(String, Rect)>,
) -> Option<RowChoice> {
    let h = 44.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::hover());
    if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::ZERO, p.surface_2);
    }
    let cy = rect.center().y;
    let mut x = rect.left() + m.space_3;
    // the buttons at the right
    let new_width = 104.0;
    let open_width = 60.0;
    let right = rect.right() - m.space_3;
    let b_new = Rect::from_center_size(Pos2::new(right - new_width / 2.0, cy), Vec2::new(new_width, m.control));
    let b_open = Rect::from_center_size(
        Pos2::new(b_new.left() - m.space_2 - open_width / 2.0, cy),
        Vec2::new(open_width, m.control),
    );
    let buttons_left = if buttons == RowButtons::OpenAndNew {
        b_open.left()
    } else {
        b_new.left()
    };
    // the thumbnail (the level picture from the game copy), when there is one
    let thumb = Rect::from_min_size(Pos2::new(x, cy - 16.0), Vec2::new(57.0, 32.0));
    if let Some(t) = tex {
        ui.painter().image(
            t.id(),
            thumb,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        ui.painter().rect_filled(thumb, CornerRadius::same(2), p.surface_2);
    }
    x = thumb.right() + m.space_3;
    let room = buttons_left - m.space_3 - x;
    let name_w = (room * 0.3).clamp(0.0, 220.0);
    let mode_w = (room * 0.2).clamp(0.0, 140.0);
    let kind_w = if buttons == RowButtons::NewOnly {
        (room * 0.16).clamp(0.0, 110.0)
    } else {
        0.0
    };
    let name = elided(
        ui,
        &l.lv.to_uppercase(),
        FontId::monospace(12.0),
        p.ink,
        name_w - m.space_2,
    );
    let mode_text = if mode.is_empty() { "-" } else { mode };
    let mode_g = elided(ui, mode_text, FontId::proportional(12.0), p.ink, mode_w - m.space_2);
    let kind_colour = if kind == LevelKind::Campaign {
        p.accent_text
    } else {
        p.ink_muted
    };
    let kind_g = elided(
        ui,
        kind.word(),
        FontId::proportional(12.0),
        kind_colour,
        kind_w - m.space_2,
    );
    let state = if l.prepared {
        "Ready"
    } else {
        "Not prepared yet - prepared when opened"
    };
    let state_g = elided(
        ui,
        state,
        FontId::proportional(12.0),
        p.ink_muted,
        room - name_w - mode_w - kind_w,
    );
    let cut = name.elided || mode_g.elided || state_g.elided || (kind_w > 0.0 && kind_g.elided);
    ui.painter().galley(Pos2::new(x, cy - name.size().y / 2.0), name, p.ink);
    ui.painter()
        .galley(Pos2::new(x + name_w, cy - mode_g.size().y / 2.0), mode_g, p.ink);
    if kind_w > 0.0 {
        ui.painter().galley(
            Pos2::new(x + name_w + mode_w, cy - kind_g.size().y / 2.0),
            kind_g,
            kind_colour,
        );
    }
    ui.painter().galley(
        Pos2::new(x + name_w + mode_w + kind_w, cy - state_g.size().y / 2.0),
        state_g,
        p.ink_muted,
    );
    ui.painter()
        .hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, p.line));
    if cut {
        resp.on_hover_text(format!(
            "{} - {mode_text} - {} - {state}",
            l.lv.to_uppercase(),
            kind.word()
        ));
    }
    let mut out = None;
    if buttons == RowButtons::OpenAndNew {
        rects.push((format!("Open:{}", l.lv), b_open));
        if ui
            .put(b_open, egui::Button::new("Open"))
            .on_hover_text(if l.prepared {
                "Open read-only"
            } else {
                "Prepare it, then open it read-only"
            })
            .clicked()
        {
            out = Some(RowChoice::Open(l.lv.clone()));
        }
    }
    rects.push((format!("New project:{}", l.lv), b_new));
    let (label, tip) = match buttons {
        RowButtons::OpenAndNew => ("New project", "Start a project of your edits on this map"),
        RowButtons::NewOnly => (
            "New level",
            "Create a new level from this one: a project of your edits on it",
        ),
    };
    if ui.put(b_new, egui::Button::new(label)).on_hover_text(tip).clicked() {
        out = Some(RowChoice::New(l.lv.clone()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trails() {
        assert_eq!(StartPage::Home.trail(), [StartPage::Home]);
        assert_eq!(
            StartPage::EditMaps(ContentSet::Ww2).trail(),
            [
                StartPage::Home,
                StartPage::EditModes,
                StartPage::EditMaps(ContentSet::Ww2)
            ]
        );
        assert_eq!(
            StartPage::CreateFromHeightmap.trail(),
            [StartPage::Home, StartPage::CreateChoice, StartPage::CreateFromHeightmap]
        );
        assert_eq!(StartPage::ImportProject.parent(), Some(StartPage::Home));
    }
}
