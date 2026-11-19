//! The start screen (round G3, the user's design): "Choose mode" - a card per set of maps (BFBC1, BFBC2, Vietnam,
//! WW2) with its picture from the game copy ([`crate::catalog`]), its installed map count, greyed "Not installed"
//! without any - then that set's map list: a header with "Choose mode", the set's name and a search box, a row per map
//! (thumbnail, name, game mode, prepared or not, Open and New project). "New map from an existing map" is the same
//! flow: a map's New project button, then the name and folder. Shown on every start (nothing remembered).

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Stroke, TextureHandle, TextureOptions,
    Vec2,
};

use super::editui::{self, StartChoice, StartForm};
use super::{EditorApp, Then, elided, status_line, style};
use crate::catalog::Picture;
use crate::levels::{ContentSet, LevelInfo};
use crate::theme::{Metrics, Palette};

/// The cards' order on the screen.
pub const CARD_ORDER: [ContentSet; 4] = [
    ContentSet::Bfbc1,
    ContentSet::Bfbc2,
    ContentSet::Vietnam,
    ContentSet::Ww2,
];

/// What a map list row asked for.
enum RowChoice {
    Open(String),
    New(String),
}

impl EditorApp {
    /// The texture of a picture of the catalog (made once, kept by `key`).
    fn picture(&mut self, ctx: &egui::Context, key: &str, pic: Option<&Picture>) -> Option<TextureHandle> {
        if let Some(t) = self.pictures.get(key) {
            return Some(t.clone());
        }
        let pic = pic?;
        let img = egui::ColorImage::from_rgba_unmultiplied(pic.size, &pic.rgba);
        let t = ctx.load_texture(key, img, TextureOptions::LINEAR);
        self.pictures.insert(key.to_owned(), t.clone());
        Some(t)
    }

    /// The start screen: the mode cards, or the chosen set's map list.
    pub(super) fn start_screen(&mut self, ui: &mut egui::Ui, p: &Palette, m: &Metrics) {
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
                        status_line(ui, p, "Error", p.status_error, &e);
                        return;
                    }
                    match self.start_set {
                        None => self.mode_cards(ui, p, m, column),
                        Some(set) => self.map_list(ui, p, m, set, column),
                    }
                });
            });
    }

    fn mode_cards(&mut self, ui: &mut egui::Ui, p: &Palette, m: &Metrics, column: f32) {
        ui.label(
            RichText::new("BFBC2 Map Editor")
                .text_style(style("display"))
                .color(p.ink),
        );
        ui.add_space(m.space_1);
        ui.label(
            RichText::new(
                "Choose a mode, then a map to view or to make a new map from. Game files are never changed: a \
                 project holds only your edits.",
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
        let mut chosen = None;
        for row in CARD_ORDER.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for set in row {
                    let n = self.levels.iter().filter(|l| l.set == *set).count();
                    let art = self.catalog.as_ref().and_then(|c| c.cards.get(set)).cloned();
                    let tex = self.picture(&ctx, &format!("card:{}", set.word()), art.as_ref());
                    let sense = if n > 0 { Sense::click() } else { Sense::hover() };
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(card_w, card_h), sense);
                    mode_card(ui, p, m, rect, *set, n, tex.as_ref(), resp.hovered() && n > 0);
                    let resp = if n > 0 {
                        resp.on_hover_text(format!("Show the {} maps", set.word()))
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                    } else {
                        resp.on_hover_text(format!("No {} map is installed in this game copy", set.word()))
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
        ui.add_space(m.space_4);
        let choice = editui::open_card(
            ui,
            p,
            m,
            &mut StartForm {
                new_level: &mut self.new_level,
                new_name: &mut self.new_name,
                new_folder: &mut self.new_folder,
                open_path: &mut self.open_path,
                recent: &self.recent,
            },
        );
        if let Some(StartChoice::Open(path)) = choice {
            self.open_project(&path);
        }
    }

    fn map_list(&mut self, ui: &mut egui::Ui, p: &Palette, m: &Metrics, set: ContentSet, column: f32) {
        let mut back = false;
        ui.horizontal(|ui| {
            if ui
                .add(egui::Button::new("\u{2190} Choose mode").min_size(Vec2::new(0.0, m.control)))
                .on_hover_text("Back to the four modes")
                .clicked()
            {
                back = true;
            }
            ui.add_space(m.space_3);
            ui.label(RichText::new(set.word()).text_style(style("display")).color(p.ink));
            ui.add_space(m.space_3);
            ui.add(
                egui::TextEdit::singleline(&mut self.level_filter)
                    .hint_text(RichText::new("Search maps").color(p.ink_faint))
                    .desired_width((column * 0.3).clamp(140.0, 320.0)),
            );
        });
        if back {
            self.choose_mode(None);
            return;
        }
        ui.add_space(m.space_4);
        let choice = editui::new_map_form(
            ui,
            p,
            m,
            &mut StartForm {
                new_level: &mut self.new_level,
                new_name: &mut self.new_name,
                new_folder: &mut self.new_folder,
                open_path: &mut self.open_path,
                recent: &self.recent,
            },
        );
        if let Some(StartChoice::Create(lv, name, folder)) = choice {
            self.open_level_then(&lv, Then::Create(name, folder));
            return;
        }
        if self.new_level.is_some() {
            ui.add_space(m.space_4);
        }
        let f = self.level_filter.trim().to_lowercase();
        let rows: Vec<LevelInfo> = self
            .levels
            .iter()
            .filter(|l| l.set == set && (f.is_empty() || l.lv.contains(&f)))
            .cloned()
            .collect();
        let ctx = ui.ctx().clone();
        let mut asked = None;
        egui::Frame::new()
            .fill(p.surface_1)
            .stroke(Stroke::new(1.0, p.line))
            .corner_radius(CornerRadius::same(m.radius_md as u8))
            .show(ui, |ui| {
                ui.set_width(column - 2.0);
                if rows.is_empty() {
                    ui.add_space(m.space_3);
                    ui.label(RichText::new("   No map matches the search.").color(p.ink_muted));
                    ui.add_space(m.space_3);
                }
                for l in &rows {
                    let thumb = self.catalog.as_ref().and_then(|c| c.thumbs.get(&l.lv)).cloned();
                    let tex = self.picture(&ctx, &format!("thumb:{}", l.lv), thumb.as_ref());
                    let mode = self
                        .catalog
                        .as_ref()
                        .and_then(|c| c.modes.get(&l.lv))
                        .cloned()
                        .unwrap_or_default();
                    if let Some(c) = map_row(ui, p, m, l, &mode, tex.as_ref()) {
                        asked = Some(c);
                    }
                }
            });
        match asked {
            Some(RowChoice::Open(lv)) => self.open_level(&lv),
            Some(RowChoice::New(lv)) => self.new_level = Some(lv),
            None => {}
        }
    }

    /// Shows a set's map list (`Some`) or the mode cards (`None`); the search and a half-made new map are cleared.
    pub fn choose_mode(&mut self, set: Option<ContentSet>) {
        self.start_set = set;
        self.level_filter.clear();
        self.new_level = None;
    }

    /// The set whose map list the start screen shows (`None`: the mode cards).
    #[must_use]
    pub fn chosen_mode(&self) -> Option<ContentSet> {
        self.start_set
    }
}

/// A mode card: its picture (or the plain tile), its name and map count; greyed with "Not installed" without maps.
#[allow(clippy::too_many_arguments, reason = "a card's parts")]
fn mode_card(
    ui: &egui::Ui,
    p: &Palette,
    m: &Metrics,
    rect: Rect,
    set: ContentSet,
    n: usize,
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
    let count = match n {
        0 => "Not installed".to_owned(),
        1 => "1 map installed".to_owned(),
        n => format!("{n} maps installed"),
    };
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

/// One map of a set's list: thumbnail, name, game mode, prepared or not, Open and New project; what was clicked.
fn map_row(
    ui: &mut egui::Ui,
    p: &Palette,
    m: &Metrics,
    l: &LevelInfo,
    mode: &str,
    tex: Option<&TextureHandle>,
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
    let room = b_open.left() - m.space_3 - x;
    let name_w = (room * 0.36).clamp(0.0, 220.0);
    let mode_w = (room * 0.24).clamp(0.0, 140.0);
    let name = elided(
        ui,
        &l.lv.to_uppercase(),
        FontId::monospace(12.0),
        p.ink,
        name_w - m.space_2,
    );
    let mode_g = elided(ui, mode, FontId::proportional(12.0), p.ink, mode_w - m.space_2);
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
        room - name_w - mode_w,
    );
    let cut = name.elided || mode_g.elided || state_g.elided;
    ui.painter().galley(Pos2::new(x, cy - name.size().y / 2.0), name, p.ink);
    ui.painter()
        .galley(Pos2::new(x + name_w, cy - mode_g.size().y / 2.0), mode_g, p.ink);
    ui.painter().galley(
        Pos2::new(x + name_w + mode_w, cy - state_g.size().y / 2.0),
        state_g,
        p.ink_muted,
    );
    ui.painter()
        .hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, p.line));
    if cut {
        resp.on_hover_text(format!("{} - {mode} - {state}", l.lv.to_uppercase()));
    }
    let mut out = None;
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
    if ui
        .put(b_new, egui::Button::new("New project"))
        .on_hover_text("New map from this map: a project of your edits on it")
        .clicked()
    {
        out = Some(RowChoice::New(l.lv.clone()));
    }
    out
}
