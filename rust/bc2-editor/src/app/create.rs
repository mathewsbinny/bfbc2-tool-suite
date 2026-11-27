//! "Create level - from a height map" (round G4) and "Generate" beside it (round TG3): the form - the height map (a
//! 16-bit PNG, or a RAW whose size and byte order the user gives), the map's size in metres, the height range, the
//! water level, an optional colour map, or the terrain generator's settings (biome, size, seed, border, water share,
//! pads, ring); the base level and the mode whose content the level uses - checked as it is filled
//! ([`crate::heightmap`]). Create saves a project on the base level and opens it; its export writes the new terrain
//! as the level's own (round E7, `bc2_levelwrite::newterrain`). A G4 project saved without a base shows its page.

use std::path::{Path, PathBuf};

use eframe::egui::{self, RichText, Vec2};

use super::modes::{CARD_ORDER, StartPage};
use super::{EditorApp, status_line, style};
use crate::error::EditorError;
use crate::heightmap::{self, Checked, Settings, Source};
use crate::levels::ContentSet;
use crate::theme::{Metrics, Palette};

/// The form's note (round E7 / TG3).
pub const NOTE: &str = "The new level is built on a base level: everything but the terrain is the base's (its mode \
                        layer, buildings, weather), and its terrain's look (patch shaders, colour map) too. The \
                        project opens on the base; Export writes the new terrain as the level's own terrain bundle.";

/// The note of a G4 project made before round E7 (no base level).
pub const LATER: &str = "This project was made before Create Level wrote terrains (round E7): it has no base level, \
                         so it cannot be opened or exported. Create it again from its height map (kept beside it).";

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
    /// "Generate" chosen (round TG3) instead of "From a height map".
    pub generate: bool,
    /// The generator's settings.
    pub gen_settings: heightmap::GenerateForm,
    /// The pads as typed (`x, z[, r]; ...`).
    pub pads: String,
    /// The last check: what it was of (path, source, the file's size and time) and what it found.
    checked: Option<(Key, Result<Checked, EditorError>)>,
    colour_checked: Option<(Key, Result<[u32; 2], EditorError>)>,
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
            generate: false,
            gen_settings: heightmap::GenerateForm::default(),
            pads: String::new(),
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
    pub fn refresh(&mut self) -> Result<Checked, EditorError> {
        let path = unquoted(&self.path);
        let key = (self.path.clone(), Some(self.source()), stamp(&path));
        if self.checked.as_ref().is_none_or(|(k, _)| *k != key) {
            let r = if self.path.trim().is_empty() {
                Err(EditorError::Input("Choose a height map file.".to_owned()))
            } else {
                heightmap::check(&path, self.source())
            };
            self.checked = Some((key, r));
        }
        self.checked
            .as_ref()
            .map_or_else(|| Err(EditorError::Input(String::new())), |(_, r)| r.clone())
    }

    /// The colour map checked again when it changed (`None`: none given).
    pub fn refresh_colour(&mut self) -> Option<Result<[u32; 2], EditorError>> {
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
        if self.generate {
            match heightmap::parse_pads(&self.pads) {
                Ok(p) => self.gen_settings.pads = p,
                Err(e) => return Some(e.to_string()),
            }
            if let Err(e) = self.gen_settings.check() {
                return Some(e.to_string());
            }
            if self.settings.base.trim().is_empty() {
                return Some("Choose the base level.".into());
            }
            if self.name.trim().is_empty() {
                return Some("Give the new level a name.".into());
            }
            if folder.trim().is_empty() {
                return Some("Choose a folder for the project.".into());
            }
            return None;
        }
        if let Err(e) = self.refresh() {
            return Some(e.to_string());
        }
        if let Err(e) = self.settings.check() {
            return Some(e.to_string());
        }
        if let Ok(c) = self.refresh() {
            let side = if c.samples.is_power_of_two() {
                c.samples
            } else {
                c.samples - 1
            };
            if (self.settings.map_size_m - f64::from(side)).abs() > 1e-9 {
                return Some(format!(
                    "The height map is {side} samples a side: set the map size to {side} m (1 m samples)."
                ));
            }
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
            RichText::new(if self.heightmap.generate {
                "Generate a terrain"
            } else {
                "From a height map"
            })
            .text_style(style("display"))
            .color(p.ink),
        );
        ui.add_space(m.space_1);
        status_line(ui, p, "Note", p.status_info, NOTE);
        ui.add_space(m.space_2);
        let mut buttons = Vec::new();
        ui.horizontal(|ui| {
            for (gen_on, label) in [(false, "From a height map"), (true, "Generate")] {
                let r = ui.add(
                    egui::Button::selectable(self.heightmap.generate == gen_on, label)
                        .min_size(Vec2::new(0.0, m.control)),
                );
                buttons.push((format!("Source:{label}"), r.rect));
                if r.clicked() {
                    self.heightmap.generate = gen_on;
                }
            }
        });
        ui.add_space(m.space_3);
        if self.heightmap.generate {
            self.start_ui.start_buttons.extend(buttons);
            self.generate_form(ui, p, m, column);
            return;
        }
        let bases = self.base_choices();
        let installed: Vec<ContentSet> = CARD_ORDER
            .into_iter()
            .filter(|s| self.levels.iter().any(|l| l.set == *s))
            .collect();
        let label_w = 190.0;
        let field_w = (column - label_w - 4.0 * m.space_3).clamp(160.0, 560.0);
        let f = &mut self.heightmap;
        let checked = f.refresh();
        let colour = f.refresh_colour();
        let folder = &mut self.new_project.new_folder;
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
                    ui.label(
                        RichText::new(e.to_string())
                            .text_style(style("small"))
                            .color(p.ink_faint),
                    );
                }
                Err(e) => status_line(ui, p, "Error", p.status_error, &e.to_string()),
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
            base_rows(ui, p, m, &mut f.settings, &bases, &mut buttons, true);
            if let Err(e) = f.settings.check() {
                status_line(ui, p, "Error", p.status_error, &e.to_string());
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
        self.start_ui
            .start_buttons
            .extend(buttons.into_iter().filter(|(l, _)| !l.starts_with("clicked:")));
        if create {
            self.create_from_heightmap();
        }
    }

    /// The levels a new level can be built on (round E7: [`heightmap::is_base_candidate`]), of the form's content
    /// set, sorted.
    #[must_use]
    pub fn base_choices(&self) -> Vec<String> {
        let set = self.heightmap.settings.content_set;
        let mut v: Vec<String> = self
            .levels
            .iter()
            .filter(|l| l.set == set && heightmap::is_base_candidate(&l.lv))
            .map(|l| l.lv.clone())
            .collect();
        v.sort();
        v
    }

    /// The base level's partition from the game copy (the new project's base sha1).
    ///
    /// # Errors
    /// No game copy, or the level does not read.
    pub fn base_partition(&self, lv: &str) -> Result<Vec<u8>, EditorError> {
        let root = self.opfor.as_ref().map_err(Clone::clone)?;
        let game =
            bc2_levelwrite::export::Game::open(&root.join("game")).map_err(|e| EditorError::Level(e.to_string()))?;
        bc2_levelwrite::export::level_partition(&game, lv)
            .map_err(|e| EditorError::Input(format!("The base level {lv} does not read from the game copy: {e}")))
    }

    /// "Generate": the generator's settings, the base level, the content set, the project (round TG3).
    fn generate_form(&mut self, ui: &mut egui::Ui, p: &Palette, m: &Metrics, column: f32) {
        let bases = self.base_choices();
        let label_w = 190.0;
        let field_w = (column - label_w - 4.0 * m.space_3).clamp(160.0, 560.0);
        let f = &mut self.heightmap;
        let folder = &mut self.new_project.new_folder;
        let mut buttons = Vec::new();
        super::editui::card(ui, p, m, (column - 2.0 * m.space_3 - 2.0).max(200.0), &mut |ui| {
            let label = |ui: &mut egui::Ui, t: &str| {
                ui.label(RichText::new(t).color(p.ink_muted));
            };
            ui.label(
                RichText::new("Terrain generator")
                    .text_style(style("title"))
                    .color(p.ink),
            );
            let (g, pads) = (&mut f.gen_settings, &mut f.pads);
            egui::Grid::new("gen settings")
                .num_columns(2)
                .min_col_width(label_w)
                .spacing([m.space_3, m.space_2])
                .show(ui, |ui| {
                    label(ui, "Biome");
                    ui.horizontal_wrapped(|ui| {
                        for b in bc2_terragen::Biome::ALL {
                            let r = ui.add(egui::Button::selectable(g.biome == b, b.name()));
                            buttons.push((format!("Biome:{}", b.name()), r.rect));
                            if r.clicked() {
                                g.biome = b;
                            }
                        }
                    });
                    ui.end_row();
                    label(ui, "Size");
                    ui.horizontal(|ui| {
                        for s in [1024u32, 2048] {
                            ui.radio_value(&mut g.size, s, format!("{s} m ({s} x {s} samples)"));
                        }
                    });
                    ui.end_row();
                    label(ui, "Seed");
                    ui.add(egui::DragValue::new(&mut g.seed));
                    ui.end_row();
                    let optional = |ui: &mut egui::Ui,
                                    v: &mut Option<f64>,
                                    dflt: f64,
                                    range: std::ops::RangeInclusive<f64>,
                                    unit: &str,
                                    what: &str| {
                        ui.horizontal(|ui| {
                            let mut on = v.is_some();
                            ui.checkbox(&mut on, "");
                            match (on, v.as_mut()) {
                                (true, Some(x)) => {
                                    ui.add(egui::DragValue::new(x).range(range).speed(0.1).suffix(unit));
                                }
                                (true, None) => *v = Some(dflt),
                                (false, _) => {
                                    *v = None;
                                    ui.label(RichText::new(what).color(p.ink_faint));
                                }
                            }
                        });
                    };
                    label(ui, "Border minus centre");
                    optional(ui, &mut g.border_m, 0.0, -300.0..=300.0, " m", "the biome's");
                    ui.end_row();
                    label(ui, "Share under water");
                    optional(ui, &mut g.water_share, 0.1, 0.0..=1.0, "", "the biome's");
                    ui.end_row();
                    label(ui, "Water level");
                    optional(
                        ui,
                        &mut g.water_level_m,
                        0.0,
                        0.0..=1500.0,
                        " m",
                        "the base level's (its look expects it)",
                    );
                    ui.end_row();
                    label(ui, "Rough relief");
                    optional(ui, &mut g.rough, 0.5, 0.0..=1.0, "", "the biome's");
                    ui.end_row();
                    label(ui, "Objective pads");
                    ui.add(
                        egui::TextEdit::singleline(&mut *pads)
                            .hint_text(RichText::new("x, z[, radius]; ...").color(p.ink_faint))
                            .desired_width(field_w),
                    );
                    ui.end_row();
                    label(ui, "Sub-terrain ring");
                    ui.checkbox(&mut g.ring, "12 distant blocks around the map");
                    ui.end_row();
                });
            ui.add_space(m.space_4);
            base_rows(ui, p, m, &mut f.settings, &bases, &mut buttons, false);
            ui.add_space(m.space_4);
            ui.label(RichText::new("Project").text_style(style("title")).color(p.ink));
            egui::Grid::new("gen project")
                .num_columns(2)
                .min_col_width(label_w)
                .spacing([m.space_3, m.space_2])
                .show(ui, |ui| {
                    label(ui, "Name");
                    ui.add(egui::TextEdit::singleline(&mut f.name).desired_width(field_w));
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
        self.start_ui
            .start_buttons
            .extend(buttons.into_iter().filter(|(l, _)| !l.starts_with("clicked:")));
        if create {
            self.create_from_heightmap();
        }
    }

    /// Saves the project the form describes - a height map's or the generator's (round TG3) - on its base level,
    /// and opens it ("Create project").
    pub fn create_from_heightmap(&mut self) {
        let base = self.heightmap.settings.base.clone();
        let base_bytes = match self.base_partition(&base) {
            Ok(b) => b,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        let f = &mut self.heightmap;
        let folder = PathBuf::from(self.new_project.new_folder.trim());
        let saved = if f.generate {
            match heightmap::parse_pads(&f.pads) {
                Ok(p) => f.gen_settings.pads = p,
                Err(e) => {
                    self.error = Some(e.to_string());
                    return;
                }
            }
            heightmap::save_generated(&heightmap::GenerateRequest {
                name: &f.name,
                folder: &folder,
                settings: f.settings.clone(),
                generate: f.gen_settings.clone(),
                base_bytes: &base_bytes,
            })
        } else {
            let (hm, colour) = (unquoted(&f.path), unquoted(&f.colour));
            heightmap::save(&heightmap::Request {
                name: &f.name,
                folder: &folder,
                heightmap: &hm,
                source: f.source(),
                colour_map: (!f.colour.trim().is_empty()).then_some(colour.as_path()),
                settings: f.settings.clone(),
                base_bytes: &base_bytes,
            })
        };
        match saved {
            Ok(path) => {
                self.error = None;
                self.heightmap = HeightmapForm::default();
                self.open_project(&path);
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// A G4 height-map project's page (no base level): what it records, and that it must be made again.
    pub(super) fn import_page(&mut self, ui: &mut egui::Ui, p: &Palette, m: &Metrics, column: f32) {
        let Some((path, project)) = self.start_ui.import_shown.clone() else {
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
        self.start_ui.start_buttons.push(("Back to the start".into(), b.rect));
        if b.clicked() {
            self.go(StartPage::Home);
        }
    }
}

/// The base level and the content set rows of both forms (round E7).
fn base_rows(
    ui: &mut egui::Ui,
    p: &Palette,
    m: &Metrics,
    s: &mut Settings,
    bases: &[String],
    buttons: &mut Vec<(String, egui::Rect)>,
    heightmap: bool,
) {
    ui.label(RichText::new("Base level").text_style(style("title")).color(p.ink));
    ui.label(
        RichText::new(
            "The multiplayer map the level is built on: its mode layer, objects and weather, and its terrain's look \
             (patch shaders, colour map, water) drawn on the new ground.",
        )
        .text_style(style("small"))
        .color(p.ink_muted),
    );
    egui::Grid::new(if heightmap { "hm base" } else { "gen base" })
        .num_columns(2)
        .min_col_width(190.0)
        .spacing([m.space_3, m.space_2])
        .show(ui, |ui| {
            ui.label(RichText::new("Base level").color(p.ink_muted));
            let r = egui::ComboBox::from_id_salt(if heightmap { "hm base level" } else { "gen base level" })
                .selected_text(s.base.to_ascii_uppercase())
                .show_ui(ui, |ui| {
                    for b in bases {
                        ui.selectable_value(&mut s.base, b.clone(), b.to_ascii_uppercase());
                    }
                });
            buttons.push(("Base level".into(), r.response.rect));
            ui.end_row();
            if heightmap {
                ui.label(RichText::new("Materials").color(p.ink_muted));
                ui.horizontal_wrapped(|ui| {
                    for b in bc2_terragen::Biome::ALL {
                        ui.radio_value(&mut s.biome, b, b.name());
                    }
                });
                ui.end_row();
                ui.label(RichText::new("Sub-terrain ring").color(p.ink_muted));
                ui.checkbox(&mut s.ring, "12 distant blocks continuing the border");
                ui.end_row();
            }
        });
    if bases.is_empty() {
        status_line(
            ui,
            p,
            "Warning",
            p.status_warn,
            "No multiplayer map of this content set is installed to build on.",
        );
    }
}
