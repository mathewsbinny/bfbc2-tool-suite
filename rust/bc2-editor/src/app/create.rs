//! "Create level - from a height map" (round G4): the form - the height map (a 16-bit PNG, or a RAW whose size and
//! byte order the user gives), the map's size in metres, the height range, the water level, an optional colour map
//! and the mode whose content the level uses - checked as it is filled ([`crate::heightmap`]); Create saves a project
//! recording them and the height map. Then the project's page: what it records, and that the terrain is generated in
//! a later round (E7). Nothing is generated, nothing faked.

use std::path::{Path, PathBuf};

use eframe::egui::{self, RichText, Vec2};

use super::modes::{CARD_ORDER, StartPage};
use super::{EditorApp, status_line, style};
use crate::heightmap::{self, Checked, Settings, Source};
use crate::levels::ContentSet;
use crate::theme::{Metrics, Palette};

/// The note every height-map project carries until the terrain round.
pub const LATER: &str = "Terrain generation arrives in a later round (E7). This project records the import settings \
                         and keeps the height map beside it; the level is made from them then.";

/// The height-map form's state.
#[derive(Debug, Clone)]
pub struct HeightmapForm {
    /// The height map's path, as typed.
    pub path: String,
    /// A RAW file (else a PNG).
    pub raw: bool,
    /// The RAW's samples per side.
    pub raw_side: u32,
    /// The RAW's byte order.
    pub little_endian: bool,
    /// The numbers and the mode.
    pub settings: Settings,
    /// The colour map's path, as typed (empty: none).
    pub colour: String,
    /// The new level's name.
    pub name: String,
    /// The last check: what it was of (path, source, the file's size and time) and what it found.
    checked: Option<(Key, Result<Checked, String>)>,
    colour_checked: Option<(Key, Result<[u32; 2], String>)>,
}

/// What a check was made of: the path, how it is read, the file's size and time (a changed file is checked again).
type Key = (String, Option<Source>, Option<(u64, std::time::SystemTime)>);

impl Default for HeightmapForm {
    fn default() -> Self {
        Self {
            path: String::new(),
            raw: false,
            raw_side: 1025,
            little_endian: true,
            settings: Settings::default(),
            colour: String::new(),
            name: String::new(),
            checked: None,
            colour_checked: None,
        }
    }
}

fn stamp(path: &Path) -> Option<(u64, std::time::SystemTime)> {
    let m = std::fs::metadata(path).ok()?;
    Some((m.len(), m.modified().ok()?))
}

fn unquoted(s: &str) -> PathBuf {
    PathBuf::from(s.trim().trim_matches('"'))
}

impl HeightmapForm {
    /// How the height map is read, as chosen.
    #[must_use]
    pub fn source(&self) -> Source {
        if self.raw {
            Source::Raw {
                side: self.raw_side,
                little_endian: self.little_endian,
            }
        } else {
            Source::Png
        }
    }

    /// The height map checked again when its path, its format or the file changed; the result.
    ///
    /// # Errors
    /// Why the height map cannot be used, in words.
    pub fn refresh(&mut self) -> Result<Checked, String> {
        let path = unquoted(&self.path);
        let key = (self.path.clone(), Some(self.source()), stamp(&path));
        if self.checked.as_ref().is_none_or(|(k, _)| *k != key) {
            let r = if self.path.trim().is_empty() {
                Err("Choose a height map file.".to_owned())
            } else {
                heightmap::check(&path, self.source())
            };
            self.checked = Some((key, r));
        }
        self.checked
            .as_ref()
            .map_or_else(|| Err(String::new()), |(_, r)| r.clone())
    }

    /// The colour map checked again when it changed (`None`: none given).
    pub fn refresh_colour(&mut self) -> Option<Result<[u32; 2], String>> {
        if self.colour.trim().is_empty() {
            return None;
        }
        let path = unquoted(&self.colour);
        let key = (self.colour.clone(), None, stamp(&path));
        if self.colour_checked.as_ref().is_none_or(|(k, _)| *k != key) {
            self.colour_checked = Some((key, heightmap::check_colour(&path)));
        }
        self.colour_checked.as_ref().map(|(_, r)| r.clone())
    }

    /// Why Create cannot run yet (`None`: it can), with `folder` the project's folder.
    pub fn why_not(&mut self, folder: &str) -> Option<String> {
        if let Err(e) = self.refresh() {
            return Some(e);
        }
        if let Err(e) = self.settings.check() {
            return Some(e);
        }
        if let Some(Err(e)) = self.refresh_colour() {
            return Some(format!("Colour map: {e}"));
        }
        if self.name.trim().is_empty() {
            return Some("Give the new level a name.".into());
        }
        if folder.trim().is_empty() {
            return Some("Choose a folder for the project.".into());
        }
        None
    }
}

impl EditorApp {
    /// Create level - from a height map: the form.
    pub(super) fn heightmap_page(&mut self, ui: &mut egui::Ui, p: &Palette, m: &Metrics, column: f32) {
        ui.label(
            RichText::new("From a height map")
                .text_style(style("display"))
                .color(p.ink),
        );
        ui.add_space(m.space_1);
        status_line(ui, p, "Note", p.status_info, LATER);
        ui.add_space(m.space_4);
        let installed: Vec<ContentSet> = CARD_ORDER
            .into_iter()
            .filter(|s| self.levels.iter().any(|l| l.set == *s))
            .collect();
        let label_w = 190.0;
        let field_w = (column - label_w - 4.0 * m.space_3).clamp(160.0, 560.0);
        let f = &mut self.heightmap;
        let checked = f.refresh();
        let colour = f.refresh_colour();
        let folder = &mut self.new_folder;
        let mut buttons = Vec::new();
        super::editui::card(ui, p, m, (column - 2.0 * m.space_3 - 2.0).max(200.0), &mut |ui| {
            let grid = |ui: &mut egui::Ui, id: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
                egui::Grid::new(id)
                    .num_columns(2)
                    .min_col_width(label_w)
                    .spacing([m.space_3, m.space_2])
                    .show(ui, |ui| add(ui));
            };
            let label = |ui: &mut egui::Ui, t: &str| {
                ui.label(RichText::new(t).color(p.ink_muted));
            };
            let metres = |ui: &mut egui::Ui, v: &mut f64, range: std::ops::RangeInclusive<f64>| {
                ui.add(egui::DragValue::new(v).range(range).speed(1.0).suffix(" m"));
            };
            ui.label(RichText::new("Height map").text_style(style("title")).color(p.ink));
            grid(ui, "hm file", &mut |ui| {
                label(ui, "File");
                ui.add(
                    egui::TextEdit::singleline(&mut f.path)
                        .hint_text(RichText::new("C:\\...\\terrain.png").color(p.ink_faint))
                        .desired_width(field_w),
                );
                ui.end_row();
                label(ui, "Format");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut f.raw, false, "16-bit greyscale PNG");
                    ui.radio_value(&mut f.raw, true, "16-bit RAW");
                });
                ui.end_row();
                if f.raw {
                    label(ui, "RAW size (samples a side)");
                    egui::ComboBox::from_id_salt("raw side")
                        .selected_text(format!("{0} x {0}", f.raw_side))
                        .show_ui(ui, |ui| {
                            for s in heightmap::SIDES {
                                ui.selectable_value(&mut f.raw_side, s, format!("{s} x {s}"));
                            }
                        });
                    ui.end_row();
                    label(ui, "RAW byte order");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut f.little_endian, true, "Little-endian (PC)");
                        ui.radio_value(&mut f.little_endian, false, "Big-endian (Mac)");
                    });
                    ui.end_row();
                }
            });
            match &checked {
                Ok(c) => status_line(ui, p, "Ready", p.status_ok, &c.summary()),
                Err(e) if f.path.trim().is_empty() => {
                    ui.label(RichText::new(e).text_style(style("small")).color(p.ink_faint));
                }
                Err(e) => status_line(ui, p, "Error", p.status_error, e),
            }
            ui.label(
                RichText::new(
                    "BFBC2's terrain is 1024 or 2048 samples a side; 1025 and 2049 also do (the last row and column \
                     are dropped).",
                )
                .text_style(style("small"))
                .color(p.ink_faint),
            );
            ui.add_space(m.space_4);
            ui.label(RichText::new("Terrain").text_style(style("title")).color(p.ink));
            grid(ui, "hm terrain", &mut |ui| {
                label(ui, "Map size (x and z)");
                metres(ui, &mut f.settings.map_size_m, 64.0..=heightmap::MAX_MAP_M);
                ui.end_row();
                label(ui, "Lowest height (value 0)");
                metres(ui, &mut f.settings.height_min_m, -4096.0..=4096.0);
                ui.end_row();
                label(ui, "Highest height (value 65535)");
                metres(ui, &mut f.settings.height_max_m, -4096.0..=4096.0);
                ui.end_row();
                label(ui, "Water level");
                metres(ui, &mut f.settings.water_level_m, -4096.0..=4096.0);
                ui.end_row();
                label(ui, "Colour map (optional)");
                ui.add(
                    egui::TextEdit::singleline(&mut f.colour)
                        .hint_text(RichText::new("A PNG laid over the terrain").color(p.ink_faint))
                        .desired_width(field_w),
                );
                ui.end_row();
            });
            if let Err(e) = f.settings.check() {
                status_line(ui, p, "Error", p.status_error, &e);
            } else if let Some(n) = f.settings.water_note() {
                status_line(ui, p, "Warning", p.status_warn, n);
            }
            match &colour {
                Some(Ok([w, h])) => status_line(ui, p, "Ready", p.status_ok, &format!("Colour map: {w} x {h}")),
                Some(Err(e)) => status_line(ui, p, "Error", p.status_error, &format!("Colour map: {e}")),
                None => {}
            }
            ui.add_space(m.space_4);
            ui.label(RichText::new("Content").text_style(style("title")).color(p.ink));
            ui.label(
                RichText::new("The mode whose objects, vehicles and kits the level uses.")
                    .text_style(style("small"))
                    .color(p.ink_muted),
            );
            ui.horizontal(|ui| {
                for set in CARD_ORDER {
                    let ok = installed.contains(&set);
                    let r = ui
                        .add_enabled(
                            ok,
                            egui::Button::selectable(f.settings.content_set == set, set.word())
                                .min_size(Vec2::new(0.0, m.control)),
                        )
                        .on_disabled_hover_text(format!("{} is not installed in this game copy", set.word()));
                    buttons.push((format!("Set:{}", set.word()), r.rect));
                    if r.clicked() {
                        f.settings.content_set = set;
                    }
                }
            });
            ui.add_space(m.space_4);
            ui.label(RichText::new("Project").text_style(style("title")).color(p.ink));
            grid(ui, "hm project", &mut |ui| {
                label(ui, "Name");
                ui.add(
                    egui::TextEdit::singleline(&mut f.name)
                        .hint_text(RichText::new("Atoll").color(p.ink_faint))
                        .desired_width(field_w),
                );
                ui.end_row();
                label(ui, "Folder");
                ui.add(egui::TextEdit::singleline(folder).desired_width(field_w));
                ui.end_row();
            });
            ui.add_space(m.space_3);
            let why = f.why_not(folder);
            ui.horizontal(|ui| {
                let b = ui.add_enabled(
                    why.is_none(),
                    egui::Button::new(RichText::new("Create project").color(p.on_accent))
                        .fill(p.accent)
                        .min_size(Vec2::new(0.0, m.control)),
                );
                buttons.push(("Create project".into(), b.rect));
                if let Some(w) = &why {
                    b.on_disabled_hover_text(w.as_str());
                    ui.label(RichText::new(w).text_style(style("small")).color(p.ink_muted));
                } else if b.clicked() {
                    buttons.push(("clicked:Create project".into(), egui::Rect::NOTHING));
                }
            });
        });
        let create = buttons.iter().any(|(l, _)| l == "clicked:Create project");
        self.start_buttons
            .extend(buttons.into_iter().filter(|(l, _)| !l.starts_with("clicked:")));
        if create {
            self.create_from_heightmap();
        }
    }

    /// Saves the height-map project the form describes and shows it ("Create project").
    pub fn create_from_heightmap(&mut self) {
        let f = &self.heightmap;
        let (hm, colour) = (unquoted(&f.path), unquoted(&f.colour));
        let folder = PathBuf::from(self.new_folder.trim());
        let r = heightmap::Request {
            name: &f.name,
            folder: &folder,
            heightmap: &hm,
            source: f.source(),
            colour_map: (!f.colour.trim().is_empty()).then_some(colour.as_path()),
            settings: f.settings,
        };
        match heightmap::save(&r) {
            Ok(path) => {
                self.error = None;
                self.heightmap = HeightmapForm::default();
                self.open_project(&path);
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// A height-map project's page: what it records, and that the terrain comes later.
    pub(super) fn import_page(&mut self, ui: &mut egui::Ui, p: &Palette, m: &Metrics, column: f32) {
        let Some((path, project)) = self.import_shown.clone() else {
            self.go(StartPage::Home);
            return;
        };
        let Some(t) = project.terrain_import.clone() else {
            self.go(StartPage::Home);
            return;
        };
        ui.label(RichText::new(&project.name).text_style(style("display")).color(p.ink));
        ui.label(
            RichText::new(path.display().to_string())
                .text_style(style("small"))
                .color(p.ink_muted),
        );
        ui.add_space(m.space_3);
        status_line(ui, p, "Note", p.status_info, LATER);
        ui.add_space(m.space_4);
        let folder = path.parent().map_or_else(PathBuf::new, Path::to_path_buf);
        let kept = |file: &str| {
            let there = folder.join(file).is_file();
            format!("{file}{}", if there { "" } else { "  (missing beside the project)" })
        };
        let format = match t.format {
            bc2_levelwrite::project::HeightFormat::Png16 => "16-bit greyscale PNG".to_owned(),
            bc2_levelwrite::project::HeightFormat::Raw16 { side, little_endian } => format!(
                "16-bit RAW, {side} x {side}, {}",
                if little_endian { "little-endian" } else { "big-endian" }
            ),
        };
        let rows: Vec<(&str, String)> = vec![
            ("Content", t.content_set.clone()),
            ("Height map", kept(&t.heightmap.file)),
            ("Copied from", t.heightmap.source.clone()),
            ("Format", format),
            (
                "Samples",
                format!(
                    "{0} x {0}, values {1} - {2}",
                    t.samples, t.sample_range[0], t.sample_range[1]
                ),
            ),
            ("sha1", t.heightmap.sha1.clone()),
            ("Map size", format!("{} m x {} m", t.map_size_m, t.map_size_m)),
            (
                "Height range",
                format!("{} m to {} m", t.height_range_m[0], t.height_range_m[1]),
            ),
            ("Water level", format!("{} m", t.water_level_m)),
            (
                "Colour map",
                t.colour_map
                    .as_ref()
                    .map_or_else(|| "none".to_owned(), |c| kept(&c.file)),
            ),
            ("Made", format!("{} by {}", project.created, project.by)),
        ];
        super::editui::card(ui, p, m, (column - 2.0 * m.space_3 - 2.0).max(200.0), &mut |ui| {
            egui::Grid::new("import project")
                .num_columns(2)
                .min_col_width(150.0)
                .spacing([m.space_3, m.space_2])
                .show(ui, |ui| {
                    for (k, v) in &rows {
                        ui.label(RichText::new(*k).color(p.ink_muted));
                        ui.label(RichText::new(v).color(p.ink));
                        ui.end_row();
                    }
                });
        });
        ui.add_space(m.space_4);
        let b = ui.add(egui::Button::new("Back to the start").min_size(Vec2::new(0.0, m.control)));
        self.start_buttons.push(("Back to the start".into(), b.rect));
        if b.clicked() {
            self.go(StartPage::Home);
        }
    }
}
