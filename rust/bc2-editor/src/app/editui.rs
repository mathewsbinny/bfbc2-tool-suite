//! The Objects workspace (round E2): what editing adds to the window - the start screen's project forms, the gizmo
//! and its toolbar (tools, world or local axes, snapping: grid, angle, vertex and edge, surface with a sink depth),
//! box selection, the ruler, the editing actions (delete, duplicate, align, distribute, groups, layers, snap to
//! ground, undo and redo), the properties' editable transform, placement and typed fields, and the History, Layers
//! and outliner folder panels. Every edit goes through [`Doc::commit`] as one operation of the project's log.

#[allow(
    clippy::wildcard_imports,
    reason = "a part of app.rs in a file of its own: its types and helpers"
)]
use super::*;

use crate::error::EditorError;
use bc2_levelwrite::Operation;
use bc2_levelwrite::ops::Organisation;
use std::fmt::Write as _;

/// The views' corner hint in the Objects workspace.
pub(super) const EDITING_HINT: &str = "Select: click, Ctrl+click, drag a box   Gizmo: drag a handle   Menu: right \
                                       click   Sink: Alt+wheel   Orbit / pan: Alt+left drag, middle drag   Fly: \
                                       hold right   Undo: Ctrl+Z   Help: F1";

/// Coordinate `a` (0 X, 1 Y, 2 Z) of a point.
fn coord(p: V3, a: usize) -> f64 {
    p.get(a).copied().unwrap_or(0.0)
}

/// The point with coordinate `a` set.
fn with_coord(mut p: V3, a: usize, v: f64) -> V3 {
    if let Some(c) = p.get_mut(a) {
        *c = v;
    }
    p
}

/// The layer colour after `c` in [`LAYER_COLOURS`].
fn colour_after(c: [u8; 3], k: usize) -> [u8; 3] {
    let at = LAYER_COLOURS.iter().position(|x| *x == c).map_or(k, |p| p + 1);
    LAYER_COLOURS
        .get(at % LAYER_COLOURS.len())
        .copied()
        .unwrap_or([0x90; 3])
}

/// Whether two transforms differ (bit for bit: an edit that changes nothing is not an operation).
fn differ(a: &M, b: &M) -> bool {
    a.iter().zip(b).any(|(x, y)| x.to_bits() != y.to_bits())
}

// ------------------------------------------------------------------------------------------------- start screen

/// The start screen's project forms.
pub(super) struct StartForm<'a> {
    pub(super) new_level: &'a mut Option<String>,
    pub(super) new_name: &'a mut String,
    pub(super) new_folder: &'a mut String,
    pub(super) open_path: &'a mut String,
    pub(super) recent: &'a [PathBuf],
    /// The start screen's buttons drawn, by label (the tests click them).
    pub(super) buttons: &'a mut Vec<(String, Rect)>,
    /// The new project's target (round E3d): the era (`None`: the base's own, its layer kept) and the mode.
    pub(super) target: Option<&'a mut (Option<bc2_levelwrite::Profile>, String)>,
    /// The game copy's modes per era (the pickers list them; `None` while not read).
    pub(super) modes: Option<&'a EraModes>,
}

/// What the new project form says: its title, its button, a note (a campaign base's).
pub(super) struct FormText<'a> {
    pub(super) title: String,
    pub(super) button: &'a str,
    pub(super) note: Option<&'a str>,
}

/// What the start screen asked for.
pub(super) enum StartChoice {
    /// A new project: the base level, the name, the folder.
    Create(String, String, PathBuf),
    /// Open a project file.
    Open(PathBuf),
}

/// A card of the start screen (`surface-1`, a `line` outline), its contents top to bottom, `width` wide inside.
pub(super) fn card(ui: &mut egui::Ui, p: &Palette, m: &Metrics, width: f32, add: &mut dyn FnMut(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(p.surface_1)
        .stroke(Stroke::new(1.0, p.line))
        .corner_radius(CornerRadius::same(m.radius_md as u8))
        .inner_margin(Margin::same(m.space_3 as i8))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.set_width(width);
                add(ui);
            });
        });
}

/// A new project once its base is chosen (round G4: a map's New project in "Edit existing level", or a level of
/// "Create level - from existing levels"): the name and the folder, the button, Cancel.
pub(super) fn new_map_form(
    ui: &mut egui::Ui,
    p: &Palette,
    m: &Metrics,
    f: &mut StartForm<'_>,
    text: &FormText<'_>,
) -> Option<StartChoice> {
    let lv = f.new_level.clone()?;
    let mut out = None;
    let width = (ui.available_width() - 2.0 * m.space_3 - 2.0).max(200.0);
    let field_w = (width - 90.0).clamp(120.0, 320.0);
    card(ui, p, m, width, &mut |ui| {
        ui.label(RichText::new(&text.title).text_style(style("title")).color(p.ink));
        ui.label(
            RichText::new(format!(
                "{} is never changed: the new level is a project of your edits on it.",
                lv.to_uppercase()
            ))
            .text_style(style("small"))
            .color(p.ink_muted),
        );
        if let Some(n) = text.note {
            ui.add_space(m.space_2);
            status_line(ui, p, "Note", p.status_info, n);
        }
        ui.add_space(m.space_2);
        egui::Grid::new("new map")
            .num_columns(2)
            .spacing([m.space_2, m.space_2])
            .show(ui, |ui| {
                ui.label(RichText::new("Name").color(p.ink_muted));
                ui.add(
                    egui::TextEdit::singleline(f.new_name)
                        .hint_text(RichText::new("Harvest Day North").color(p.ink_faint))
                        .desired_width(field_w),
                );
                ui.end_row();
                ui.label(RichText::new("Folder").color(p.ink_muted));
                ui.add(egui::TextEdit::singleline(f.new_folder).desired_width(field_w));
                ui.end_row();
                if let Some(t) = f.target.as_deref_mut() {
                    target_rows(ui, p, t, f.modes, field_w);
                }
            });
        ui.add_space(m.space_2);
        let why = if f.new_name.trim().is_empty() {
            Some("Give the new level a name")
        } else if f.new_folder.trim().is_empty() {
            Some("Choose a folder for the project")
        } else {
            None
        };
        ui.horizontal(|ui| {
            let b = ui.add_enabled(
                why.is_none(),
                egui::Button::new(RichText::new(text.button).color(p.on_accent))
                    .fill(p.accent)
                    .min_size(Vec2::new(0.0, m.control)),
            );
            f.buttons.push((text.button.to_owned(), b.rect));
            if let Some(w) = why {
                b.on_disabled_hover_text(w);
            } else if b.clicked() {
                out = Some(StartChoice::Create(
                    lv.clone(),
                    f.new_name.trim().to_owned(),
                    PathBuf::from(f.new_folder.trim()),
                ));
            }
            if ui
                .add(egui::Button::new("Cancel").min_size(Vec2::new(0.0, m.control)))
                .clicked()
            {
                *f.new_level = None;
            }
        });
    });
    out
}

/// The target pickers (round E3d): the era the new level is made for and its multiplayer mode, the modes listed
/// from the game copy's stock levels of that era (a mode no stock level of the era has is not offered; Onslaught
/// and Air Superiority are listed with why they are not offered yet).
pub(super) fn target_rows(
    ui: &mut egui::Ui,
    p: &Palette,
    t: &mut (Option<bc2_levelwrite::Profile>, String),
    modes: Option<&EraModes>,
    width: f32,
) {
    use bc2_levelwrite::Profile;
    use bc2_levelwrite::modelayer::{mode_name, not_offered};
    ui.label(RichText::new("Era").color(p.ink_muted));
    let era_text = |e: Option<Profile>| {
        e.map_or_else(
            || "The base's own (keep its mode layer)".to_owned(),
            |x| x.word().to_owned(),
        )
    };
    egui::ComboBox::from_id_salt("new-project-era")
        .width(width)
        .selected_text(era_text(t.0))
        .show_ui(ui, |ui| {
            if ui.selectable_label(t.0.is_none(), era_text(None)).clicked() {
                t.0 = None;
            }
            for e in Profile::ALL {
                let have = modes.is_none_or(|m| m.contains_key(&e));
                if ui
                    .add_enabled(have, egui::Button::selectable(t.0 == Some(e), e.word()))
                    .on_disabled_hover_text("The game copy has no multiplayer level of this era")
                    .clicked()
                {
                    t.0 = Some(e);
                    let offered = modes
                        .and_then(|m| m.get(&e))
                        .is_some_and(|m| m.contains_key(&t.1) && not_offered(&t.1).is_none());
                    if !offered {
                        "conquest".clone_into(&mut t.1);
                    }
                }
            }
        });
    ui.end_row();
    ui.label(RichText::new("Mode").color(p.ink_muted));
    let era_modes = t.0.and_then(|e| modes.and_then(|m| m.get(&e)));
    ui.add_enabled_ui(t.0.is_some(), |ui| {
        egui::ComboBox::from_id_salt("new-project-mode")
            .width(width)
            .selected_text(if t.0.is_some() {
                mode_name(&t.1)
            } else {
                "The base's own".into()
            })
            .show_ui(ui, |ui| {
                for (k, lvs) in era_modes.into_iter().flatten() {
                    let why = not_offered(k);
                    let r = ui
                        .add_enabled(
                            why.is_none(),
                            egui::Button::selectable(
                                t.1 == *k,
                                format!("{} ({} stock levels)", mode_name(k), lvs.len()),
                            ),
                        )
                        .on_disabled_hover_text(why.unwrap_or_default());
                    if r.clicked() {
                        k.clone_into(&mut t.1);
                    }
                }
            });
    });
    ui.end_row();
    if let (Some(e), None) = (t.0, era_modes) {
        ui.label("");
        ui.label(RichText::new(format!("Reading the game copy's {} modes...", e.word())).color(p.ink_faint));
        ui.end_row();
    }
}

/// One project row of "Open project": its name (a link), its folder; `missing` greys it. The click opens it.
fn project_row(
    ui: &mut egui::Ui,
    p: &Palette,
    f: &mut StartForm<'_>,
    tag: &str,
    path: &Path,
    out: &mut Option<StartChoice>,
) {
    let name = path
        .file_name()
        .map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
    let missing = !path.is_file();
    ui.horizontal(|ui| {
        let b = ui
            .add_enabled(
                !missing,
                egui::Button::new(RichText::new(&name).color(if missing { p.ink_faint } else { p.accent_text }))
                    .frame(false),
            )
            .on_hover_text(path.display().to_string())
            .on_disabled_hover_text(format!("{} is no longer there", path.display()));
        f.buttons.push((format!("{tag}:{name}"), b.rect));
        if b.clicked() {
            *out = Some(StartChoice::Open(path.to_owned()));
        }
        let folder = path.parent().map_or_else(String::new, |d| d.display().to_string());
        ui.add(
            egui::Label::new(
                RichText::new(if missing {
                    format!("{folder}  -  missing")
                } else {
                    folder
                })
                .text_style(style("small"))
                .color(p.ink_faint),
            )
            .truncate(),
        );
    });
}

/// "Open project" (round G4): a project file's path, the recent projects, and the projects found in the projects
/// folder (`found`; `folder` names it). An autosave newer than the file is offered on opening (crash recovery).
pub(super) fn open_card(
    ui: &mut egui::Ui,
    p: &Palette,
    m: &Metrics,
    f: &mut StartForm<'_>,
    found: &[PathBuf],
    folder: &str,
) -> Option<StartChoice> {
    let mut out = None;
    let width = (ui.available_width() - 2.0 * m.space_3 - 2.0).max(200.0);
    card(ui, p, m, width, &mut |ui| {
        ui.label(RichText::new("Project file").text_style(style("title")).color(p.ink));
        ui.label(
            RichText::new(
                "A .mapedit.json file: its base level loads first, then your edits on it. Type or paste its path.",
            )
            .text_style(style("small"))
            .color(p.ink_muted),
        );
        ui.add_space(m.space_2);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(f.open_path)
                    .hint_text(RichText::new("C:\\...\\My map.mapedit.json").color(p.ink_faint))
                    .desired_width((width - 80.0).clamp(120.0, 560.0)),
            );
            let b = ui.add_enabled(
                !f.open_path.trim().is_empty(),
                egui::Button::new(RichText::new("Open").color(p.on_accent)).fill(p.accent),
            );
            f.buttons.push(("Open file".into(), b.rect));
            if b.clicked() {
                out = Some(StartChoice::Open(PathBuf::from(f.open_path.trim().trim_matches('"'))));
            }
        });
    });
    ui.add_space(m.space_4);
    card(ui, p, m, width, &mut |ui| {
        ui.label(RichText::new("Recent projects").text_style(style("title")).color(p.ink));
        if f.recent.is_empty() {
            ui.label(
                RichText::new("None yet: a project you make or open is listed here.")
                    .text_style(style("small"))
                    .color(p.ink_faint),
            );
        }
        let recent = f.recent.to_vec();
        for r in &recent {
            project_row(ui, p, f, "recent", r, &mut out);
        }
    });
    ui.add_space(m.space_4);
    card(ui, p, m, width, &mut |ui| {
        ui.label(
            RichText::new("In the projects folder")
                .text_style(style("title"))
                .color(p.ink),
        );
        ui.label(RichText::new(folder).text_style(style("small")).color(p.ink_muted));
        if found.is_empty() {
            ui.label(
                RichText::new("No project there yet.")
                    .text_style(style("small"))
                    .color(p.ink_faint),
            );
        }
        for r in found {
            project_row(ui, p, f, "found", r, &mut out);
        }
    });
    out
}

// --------------------------------------------------------------------------------------------- the edits

fn rows(m: &M) -> bc2_levelwrite::Rows {
    bc2_levelwrite::rows_of(m)
}

impl ViewState {
    /// The views and the outliner again after the project changed (an edit, an undo, a layer hidden).
    pub(super) fn refresh(&mut self, gpu: &GpuCtx, palette: &Palette) {
        // round A2: the overlays' gameplay areas follow the project (an edit, an undo)
        if self.overlays_rev.is_some() && self.doc.as_ref().is_some_and(|d| self.overlays_rev != Some(d.revision)) {
            self.apply_look(palette);
        }
        if self.update_mode_view() {
            self.mode_view.outliner_stale = true;
        }
        if let Some(d) = &self.doc {
            let skip = |k: usize| !d.shown(k);
            self.gpu_scene.terrain_rev = d.terrain_rev;
            self.gpu_scene.refresh(&gpu.gpu, &d.scene, palette, &skip);
            let mut without = d.deleted.clone();
            without.extend(d.filtered.iter().copied());
            self.outliner = Outliner::new_without(&d.scene, &without);
            let n = d.scene.entities.len();
            self.selection.retain(|&k| k < n && d.shown(k));
        } else {
            // round VW2: the mode layer filter hides objects of a level opened read only too
            let hidden = &self.mode_view.hidden;
            self.gpu_scene
                .refresh(&gpu.gpu, &self.base, palette, &|k| hidden.contains(&k));
            if self.mode_view.outliner_stale {
                self.outliner = Outliner::new_without(&self.base, &hidden.iter().copied().collect());
            }
            self.selection.retain(|k| !hidden.contains(k));
        }
        self.mode_view.outliner_stale = false;
        self.selected = self.selection.last().copied();
        let sel = self.selection.clone();
        self.gpu_scene.set_selection(&gpu.gpu, &sel);
        // round E5: the game-accurate view's terrain follows a stroke or its undo
        self.sync_game_terrain();
        self.redraw();
    }

    /// The mode view layer's hidden objects again (round VW2): the layer and the Rush stages read from the edited
    /// partition (or the level as loaded) when the project changed, the filter applied to the scene and handed to the
    /// project. Returns whether the hidden set changed.
    pub(super) fn update_mode_view(&mut self) -> bool {
        let before = std::mem::take(&mut self.mode_view.hidden);
        if self.mode_view.active() {
            if let Some(d) = &self.doc {
                self.mode_view.read(d.writer(), d.revision);
            } else {
                let w = self
                    .base
                    .file(0)
                    .and_then(|(_, b)| bc2_levelwrite::write::LevelWriter::parse(b).ok());
                self.mode_view.read(w.as_ref(), 0);
            }
        }
        let scene = self.doc.as_ref().map_or(&self.base, |d| &d.scene);
        self.mode_view.apply(scene);
        let hidden = self.mode_view.hidden.clone();
        if let Some(d) = self.doc.as_mut() {
            d.filtered = hidden;
        }
        let changed = before != self.mode_view.hidden;
        self.mode_view.outliner_stale |= changed;
        changed
    }

    /// The mode view layer's state in words (the status bar).
    pub(super) fn mode_view_message(&self) -> String {
        let m = &self.mode_view;
        let mut s = format!("Mode view layer: {}", m.describe());
        if m.active() {
            let _ = write!(s, " (the mode layer has {} instances", m.layer_size());
            if let Some(st) = m.stage.and_then(|k| m.view.as_ref().and_then(|(_, v)| v.stages.get(k))) {
                let _ = write!(
                    s,
                    "; this stage's {} M-COMs, {} bases and {} combat areas in play",
                    st.mcoms.len(),
                    st.bases.len(),
                    st.combat_areas.len()
                );
            }
            s.push(')');
        }
        s
    }

    /// Round VW2: the level's weather read (once): from the game-accurate view's files when it is loaded, else the
    /// level's own (a prepared level's folders, a Package level's bundles).
    pub(super) fn ensure_weather(&mut self) -> Result<(), EditorError> {
        if self.weather.is_some() {
            return Ok(());
        }
        let from_game = self
            .game_view
            .game
            .as_ref()
            .and_then(|g| Some((g.files().clone(), g.world().level_weather.clone()?)));
        let w = if let Some((files, lw)) = from_game {
            crate::weatherview::WeatherView::with(files, lw)
        } else {
            let root = self
                .root
                .clone()
                .ok_or(EditorError::Level("Project OPFOR BFBC2's folder was not found".into()))?;
            let scene = self.scene();
            let placed = crate::gameview::placed(scene);
            let package = crate::gameview::is_package(&root, &self.lv);
            crate::weatherview::WeatherView::load(&root, &self.lv, &scene.name, &placed, package)?
        };
        self.weather = Some(w);
        Ok(())
    }

    /// Round VW2: both views under weather state `k` (`None`: the level's own, the one players see as it starts);
    /// the status line's words.
    pub(super) fn show_weather(&mut self, k: Option<usize>) -> Result<String, EditorError> {
        self.ensure_weather()?;
        let w = self.weather.as_mut().ok_or(EditorError::Level("no weather".into()))?;
        let k = k.unwrap_or_else(|| w.level.starts_in());
        let row = w.rows.get(k).ok_or_else(|| {
            EditorError::Level(format!(
                "the level's weather system has {} states, not {}",
                w.rows.len(),
                k + 1
            ))
        })?;
        let tint = row.tint;
        w.shown = Some(k);
        let label = w.label(k);
        self.gpu_scene.weather_tint = tint;
        let mut note = String::new();
        if let Some(g) = self.game_view.game.as_mut()
            && let Err(e) = g.set_state(k)
        {
            note = format!(" (the game-accurate view: {e})");
        }
        self.redraw();
        Ok(format!(
            "Weather: state {label}{}{note}",
            tint.map_or_else(String::new, |t| format!(
                "; the simplified view tinted ({:.2}, {:.2}, {:.2})",
                t[0], t[1], t[2]
            ))
        ))
    }

    /// The selected entities that can be edited (all of them when not editing: none).
    pub(super) fn editable_selection(&self) -> Vec<usize> {
        let Some(d) = &self.doc else { return Vec::new() };
        self.selection
            .iter()
            .copied()
            .filter(|&k| d.why_not(k).is_none())
            .collect()
    }

    pub(super) fn matrix(&self, k: usize) -> Option<M> {
        self.scene().entities.get(k).and_then(|e| e.matrix)
    }

    /// The placement an object keeps (its last surface placement), else the toolbar's.
    pub(super) fn placement_for(&self, k: usize) -> Placement {
        self.doc
            .as_ref()
            .and_then(|d| {
                d.guid(k)
                    .and_then(|g| d.org.placement.get(&g.to_ascii_uppercase()).copied())
            })
            .unwrap_or(self.snap.placement)
    }

    /// Commits moves or turns: (entity, before, after, placement) per object.
    pub(super) fn commit_transforms(
        &mut self,
        gpu: &GpuCtx,
        palette: &Palette,
        rotate: bool,
        items: &[(usize, M, M, Option<Placement>)],
        label: &str,
    ) {
        // a per-object sink being typed is now in the log (or was left unused)
        self.props.prop_place = None;
        let Some(d) = self.doc.as_mut() else { return };
        let moved: Vec<Moved> = items
            .iter()
            .filter(|(_, a, b, p)| differ(a, b) || p.is_some())
            .filter_map(|(k, a, b, p)| {
                Some(Moved {
                    guid: d.guid(*k)?.to_owned(),
                    from: rows(a),
                    to: rows(b),
                    placement: *p,
                })
            })
            .collect();
        if moved.is_empty() {
            // nothing changed: put the scene back as the log has it
            d.rebuild();
        } else {
            let op = if rotate {
                Op::Rotate { items: moved }
            } else {
                Op::Move { items: moved }
            };
            if let Some(why) = d.refusal(&op) {
                // round SG1: refused before it enters the log; the scene back as the log has it
                d.rebuild();
                self.message = format!("Not done: {why}");
                self.refresh(gpu, palette);
                return;
            }
            d.commit(op, label);
            self.message = d.project.operations.last().map_or_else(String::new, Operation::name);
        }
        self.refresh(gpu, palette);
    }

    /// Commits an operation and refreshes the views.
    pub(super) fn commit(&mut self, gpu: &GpuCtx, palette: &Palette, op: Op, label: &str) {
        let Some(d) = self.doc.as_mut() else { return };
        // round SG1: an operation that would not apply (the last member of a kind in its group deleted ...) is
        // refused in words and never enters the log
        if let Some(why) = d.refusal(&op) {
            self.message = format!("Not done: {why}");
            return;
        }
        d.commit(op, label);
        self.message = d.project.operations.last().map_or_else(String::new, Operation::name);
        self.refresh(gpu, palette);
    }

    /// The selection put on the surface below each object (terrain or a mesh, passing through the selection),
    /// each with the placement it keeps or `placement`.
    pub(super) fn placed_on_surface(
        &self,
        ks: &[usize],
        placement: Option<Placement>,
    ) -> Vec<(usize, M, M, Option<Placement>)> {
        let mut out = Vec::new();
        let scene = self.scene();
        let skip = |k: usize| ks.contains(&k) || self.doc.as_ref().is_some_and(|d| !d.shown(k));
        for &k in ks {
            let Some(m) = self.matrix(k) else { continue };
            let p = placement.unwrap_or_else(|| self.placement_for(k));
            let pos = math::row(&m, 3);
            if let Some(hit) = query::surface_below_except(scene, pos[0], pos[2], &skip) {
                out.push((k, m, query::place(&m, query::extent(scene, k), &hit, &p), Some(p)));
            }
        }
        out
    }

    /// The editing actions; `false` for actions that are not editing ones.
    pub(super) fn edit_action(&mut self, gpu: &GpuCtx, palette: &Palette, action: Action) -> bool {
        let sel = self.editable_selection();
        match action {
            Action::Undo | Action::Redo => {
                let Some(d) = self.doc.as_mut() else { return true };
                let name = if action == Action::Undo {
                    d.project.operations.last().map(Operation::name)
                } else {
                    d.project.redo.last().map(Operation::name)
                };
                let done = if action == Action::Undo { d.undo() } else { d.redo() };
                if done {
                    let verb = if action == Action::Undo { "Undid" } else { "Redid" };
                    self.message = format!("{verb}: {}", name.unwrap_or_default());
                }
                self.pointer.drag = None;
                self.refresh(gpu, palette);
            }
            Action::CheckEdits => {
                if let Some(d) = &self.doc {
                    self.message = d.check();
                }
            }
            Action::Delete => {
                let Some(d) = &self.doc else { return true };
                // round E4: in the Gameplay workspace a flag, spawner or M-COM goes with its unit (its zone, marker,
                // spawns and logic; an M-COM's stage counting one fewer), refused in words where the mode needs it
                if self.workspace == Workspace::Gameplay
                    && !sel.is_empty()
                    && sel.iter().all(|&k| crate::gameplay::is_unit_root(d, k))
                {
                    match crate::gameplay::delete_op(d, &sel) {
                        Ok(op) => {
                            self.selection.clear();
                            self.commit(gpu, palette, op, "Delete with what belongs to it");
                        }
                        Err(why) => self.message = format!("Not deleted: {why}"),
                    }
                    return true;
                }
                let guids: Vec<String> = sel.iter().filter_map(|&k| d.guid(k).map(str::to_owned)).collect();
                self.selection.clear();
                self.commit(gpu, palette, Op::Delete { guids }, "");
            }
            Action::Duplicate => {
                let Some(d) = &self.doc else { return true };
                let offset = if self.snap.grid {
                    self.snap.grid_size.max(1.0)
                } else {
                    2.0
                };
                let items: Vec<bc2_levelwrite::ops::Duplicated> = sel
                    .iter()
                    .filter_map(|&k| {
                        let g = d.guid(k)?;
                        let m = self.matrix(k)?;
                        Some(bc2_levelwrite::ops::Duplicated {
                            source: g.to_owned(),
                            guid: d.project.new_guid(g),
                            transform: rows(&math::translate(&m, [offset, 0.0, 0.0])),
                            owned: Vec::new(),
                        })
                    })
                    .collect();
                let n = self.doc.as_ref().and_then(Doc::last_n);
                self.commit(gpu, palette, Op::Duplicate { items }, "");
                // the copies are selected, to be moved (a static group member's copy is named by the group: SG1)
                if let Some(d) = &self.doc
                    && d.last_n() != n
                {
                    let ks: Vec<usize> = d.created_by_last().iter().filter_map(|g| d.index(g)).collect();
                    self.set_selection(gpu, ks, true);
                }
            }
            Action::UseTool(t) => {
                self.tool = t;
                self.tools.choose(t);
                self.message = format!("{} tool", t.name());
            }
            Action::ToggleSpace => {
                self.space = match self.space {
                    Space::World => Space::Local,
                    Space::Local => Space::World,
                };
            }
            Action::ToggleGridSnap => self.snap.grid = !self.snap.grid,
            Action::ToggleAngleSnap => self.snap.angle = !self.snap.angle,
            Action::ToggleVertexSnap => self.snap.vertex = !self.snap.vertex,
            Action::ToggleSurfaceSnap => self.snap.surface = !self.snap.surface,
            Action::PlaceOnSurface => {
                let items = self.placed_on_surface(&sel, None);
                self.commit_transforms(gpu, palette, false, &items, "Snap to ground");
            }
            Action::Align(a) => {
                let a = usize::from(a);
                let Some(target) = self
                    .selected
                    .and_then(|k| self.matrix(k))
                    .map(|m| coord(math::row(&m, 3), a))
                else {
                    return true;
                };
                let items: Vec<_> = sel
                    .iter()
                    .filter_map(|&k| {
                        let m = self.matrix(k)?;
                        Some((k, m, math::at(&m, with_coord(math::row(&m, 3), a, target)), None))
                    })
                    .collect();
                let label = ["Align on X", "Align on Y", "Align on Z"]
                    .get(a)
                    .copied()
                    .unwrap_or("Align");
                self.commit_transforms(gpu, palette, false, &items, label);
            }
            Action::Distribute(a) => {
                let a = usize::from(a);
                let mut at: Vec<(usize, M)> = sel.iter().filter_map(|&k| Some((k, self.matrix(k)?))).collect();
                at.sort_by(|x, y| coord(math::row(&x.1, 3), a).total_cmp(&coord(math::row(&y.1, 3), a)));
                let (Some(lo), Some(hi)) = (at.first(), at.last()) else {
                    return true;
                };
                let (lo, hi) = (coord(math::row(&lo.1, 3), a), coord(math::row(&hi.1, 3), a));
                let n = at.len().saturating_sub(1).max(1) as f64;
                let items: Vec<_> = at
                    .iter()
                    .enumerate()
                    .map(|(i, (k, m))| {
                        let p = with_coord(math::row(m, 3), a, lo + (hi - lo) * i as f64 / n);
                        (*k, *m, math::at(m, p), None)
                    })
                    .collect();
                let label = ["Distribute on X", "Distribute on Y", "Distribute on Z"]
                    .get(a)
                    .copied()
                    .unwrap_or("Distribute");
                self.commit_transforms(gpu, palette, false, &items, label);
            }
            Action::Group => {
                let Some(d) = &self.doc else { return true };
                let group = d.org.free_group_name();
                let guids = sel.iter().filter_map(|&k| d.guid(k).map(str::to_owned)).collect();
                self.commit(gpu, palette, Op::Group { group, guids }, "");
            }
            Action::Ungroup => {
                let Some(d) = &self.doc else { return true };
                let group = self
                    .selection
                    .iter()
                    .find_map(|&k| d.guid(k).and_then(|g| d.org.group_of(g)).map(|(n, _)| n.to_owned()));
                if let Some(group) = group {
                    self.commit(gpu, palette, Op::Ungroup { group }, "");
                }
            }
            Action::NewLayer => {
                let Some(d) = &self.doc else { return true };
                let layer = d.org.free_layer_name();
                let colour = colour_after([0; 3], d.org.layers.len());
                let guids: Vec<String> = sel.iter().filter_map(|&k| d.guid(k).map(str::to_owned)).collect();
                self.commit(
                    gpu,
                    palette,
                    Op::AddLayer {
                        layer: layer.clone(),
                        colour,
                    },
                    "",
                );
                self.commit(gpu, palette, Op::AssignLayer { layer, guids }, "");
            }
            _ => return false,
        }
        self.redraw();
        true
    }

    /// The gizmo's frame: at the primary object, in the chosen axes (none when it cannot be edited).
    pub(super) fn gizmo_frame(&self) -> Option<Frame> {
        let k = self.selected?;
        if self.doc.as_ref()?.why_not(k).is_some() {
            return None;
        }
        if let Some(d) = &self.pointer.drag {
            // while dragging, the frame moves with the primary
            let now = d.now.iter().find(|(x, _)| *x == k).map(|(_, m)| *m)?;
            return Some(Frame {
                origin: math::row(&now, 3),
                axes: if self.space == Space::Local {
                    Frame::of(&now, Space::Local).axes
                } else {
                    d.frame.axes
                },
            });
        }
        Some(Frame::of(&self.matrix(k)?, self.space))
    }

    /// Updates a drag with the pointer at `px` (viewport points).
    fn drag_to(&mut self, view: &camera::View, px: [f64; 2], shift: bool) {
        let Some(mut drag) = self.pointer.drag.take() else {
            return;
        };
        let grid = if self.snap.grid { self.snap.grid_size } else { 0.0 };
        let angle = if self.snap.angle {
            self.snap.angle_step.to_radians()
        } else {
            0.0
        };
        let ks: Vec<usize> = drag.objects.iter().map(|(k, _)| *k).collect();
        let doc = self.doc.as_ref();
        let skip = |k: usize| ks.contains(&k) || doc.is_some_and(|d| !d.shown(k));
        let scene = self.scene();
        let mut delta = gizmo::delta(view, &drag.frame, drag.handle, drag.start, px, grid, angle);
        // vertex and edge snapping: the pivot goes to the corner (Shift: the edge) under the pointer
        if self.snap.vertex
            && matches!(delta, Some(gizmo::Delta::Move(_)))
            && let Some(hit) = {
                let (o, d) = view.ray(px);
                query::ray_hit_except(scene, o, d, &skip)
            }
        {
            let target = if shift {
                query::snap_edge(&hit)
            } else {
                query::snap_vertex(&hit)
            };
            delta = Some(gizmo::Delta::Move(math::sub(target, drag.frame.origin)));
        }
        let Some(delta) = delta else {
            self.pointer.drag = Some(drag);
            return;
        };
        let mut now: Vec<(usize, M)> = drag
            .objects
            .iter()
            .map(|(k, m)| (*k, gizmo::apply(m, &drag.frame, delta)))
            .collect();
        drag.placed = false;
        // surface snapping: each object on the surface below it (not for the up arm, which sets the height)
        if self.snap.surface && matches!(delta, gizmo::Delta::Move(_)) && drag.handle != Handle::Axis(1) {
            let centre_hit = if drag.handle == Handle::Centre {
                let (o, d) = view.ray(px);
                query::ray_hit_except(scene, o, d, &skip)
            } else {
                None
            };
            for (k, m) in &mut now {
                let p = self.placement_for(*k);
                let mut pos = math::row(m, 3);
                if let Some(h) = centre_hit {
                    // the centre drags the primary to the point under the pointer; the others keep their offsets
                    let shift_by = math::sub(h.point, drag.frame.origin);
                    let start = drag
                        .objects
                        .iter()
                        .find(|(x, _)| x == k)
                        .map_or(pos, |(_, s)| math::row(s, 3));
                    pos = math::add(start, shift_by);
                }
                if let Some(hit) = query::surface_below_except(scene, pos[0], pos[2], &skip) {
                    *m = query::place(&math::at(m, pos), query::extent(scene, *k), &hit, &p);
                    drag.placed = true;
                }
            }
        }
        if let Some(d) = self.doc.as_mut() {
            d.preview(&now);
        }
        self.message = match delta {
            gizmo::Delta::Move(v) => format!("Moving {:.2}, {:.2}, {:.2} m", v[0], v[1], v[2]),
            gizmo::Delta::Turn { angle, .. } => format!("Turning {:.1} deg", angle.to_degrees()),
        };
        drag.now = now;
        self.pointer.drag = Some(drag);
    }

    /// Ends a drag: one operation for every object moved.
    fn finish_drag(&mut self, gpu: &GpuCtx, palette: &Palette) {
        let Some(drag) = self.pointer.drag.take() else { return };
        let rotate = matches!(drag.handle, Handle::Ring(_));
        let items: Vec<_> = drag
            .objects
            .iter()
            .zip(&drag.now)
            .map(|((k, a), (_, b))| (*k, *a, *b, drag.placed.then(|| self.placement_for(*k))))
            .collect();
        self.commit_transforms(gpu, palette, rotate, &items, "");
    }
}

// ------------------------------------------------------------------------------------------------ the views

impl Tabs<'_> {
    pub(super) fn view_of(rect: Rect, cam: &Camera) -> camera::View {
        cam.view([f64::from(rect.width()), f64::from(rect.height())])
    }

    /// Editing input on a viewport; `true` when the left button's drag is the editor's (the gizmo or a box).
    pub(super) fn edit_input(
        &mut self,
        ui: &egui::Ui,
        resp: &egui::Response,
        rect: Rect,
        projection: Projection,
        vp: &mut Viewport,
    ) -> bool {
        let (alt, shift, origin, hover) = ui.input(|i| {
            (
                i.modifiers.alt,
                i.modifiers.shift,
                i.pointer.press_origin(),
                i.pointer.hover_pos(),
            )
        });
        let view = Self::view_of(rect, &vp.camera);
        let local = |p: Pos2| [f64::from(p.x - rect.min.x), f64::from(p.y - rect.min.y)];
        let gizmo_tool = self.view.tools.get(self.view.tool).gizmo();
        let frame = self.view.gizmo_frame();
        if self.view.pointer.drag.is_none() {
            self.view.pointer.hover = match (frame, hover) {
                (Some(f), Some(h)) if gizmo_tool && rect.contains(h) => {
                    gizmo::pick(&view, &f, self.view.tool, local(h))
                }
                _ => None,
            };
        }
        let palette = self.palette;
        if resp.drag_started_by(PointerButton::Primary) && !alt {
            let at = origin.unwrap_or_else(|| resp.interact_pointer_pos().unwrap_or(rect.center()));
            if let Some(f) = frame
                && gizmo_tool
                && let Some(h) = gizmo::pick(&view, &f, self.view.tool, local(at))
                && let Some(start) = gizmo::start(&view, &f, h, local(at))
            {
                let objects: Vec<(usize, M)> = self
                    .view
                    .editable_selection()
                    .into_iter()
                    .filter_map(|k| Some((k, self.view.matrix(k)?)))
                    .collect();
                self.view.pointer.drag = Some(Drag {
                    projection,
                    handle: h,
                    frame: f,
                    start,
                    now: objects.clone(),
                    objects,
                    placed: false,
                });
            } else if self.view.tools.get(self.view.tool).boxes() {
                self.view.pointer.box_from = Some((projection, at));
            }
            return true;
        }
        let mine = self
            .view
            .pointer
            .drag
            .as_ref()
            .is_some_and(|d| d.projection == projection);
        if mine && resp.dragged_by(PointerButton::Primary) {
            if let Some(p) = resp.interact_pointer_pos() {
                self.view.drag_to(&view, local(p), shift);
                if let Some(d) = self.view.doc.as_ref() {
                    let skip = |k: usize| !d.shown(k);
                    self.view.gpu_scene.refresh(&self.gpu.gpu, &d.scene, &palette, &skip);
                }
                self.view.redraw();
                vp.dirty = true;
            }
            return true;
        }
        let boxing = self.view.pointer.box_from.is_some_and(|(p, _)| p == projection);
        if boxing && resp.dragged_by(PointerButton::Primary) {
            return true;
        }
        if resp.drag_stopped() {
            if mine {
                self.view.finish_drag(self.gpu, &palette);
                vp.dirty = true;
                return true;
            }
            if boxing
                && let (Some((_, from)), Some(to)) = (self.view.pointer.box_from.take(), resp.interact_pointer_pos())
            {
                let add = ui.input(|i| i.modifiers.command || i.modifiers.shift);
                self.box_select(Rect::from_two_pos(from, to), rect, &vp.camera, add);
                vp.dirty = true;
                return true;
            }
        }
        !alt && resp.dragged_by(PointerButton::Primary)
    }

    /// Selects the editable, shown entities whose position is inside a box drawn on a viewport.
    fn box_select(&mut self, area: Rect, rect: Rect, cam: &Camera, add: bool) {
        let view = Self::view_of(rect, cam);
        let Some(d) = self.view.doc.as_ref() else { return };
        let mut list = if add { self.view.selection.clone() } else { Vec::new() };
        for e in &d.scene.entities {
            if !self.view.selectable(e.index) || d.why_not(e.index).is_some() || list.contains(&e.index) {
                continue;
            }
            let Some(p) = e.position() else { continue };
            if let Some(s) = view.project(p)
                && area.contains(rect.min + Vec2::new(s[0] as f32, s[1] as f32))
            {
                list.push(e.index);
            }
        }
        self.view.message = format!("{} selected", list.len());
        self.view.set_selection(self.gpu, list, true);
    }

    /// A click while editing: select (with its group), Ctrl or Shift adds or removes; the ruler takes the point.
    pub(super) fn edit_click(&mut self, entity: Option<usize>, add: bool, hit: Option<query::Hit>) {
        // the chosen tool's click (round PR1b-0: the ruler's points), else the click selects
        let (tool, snap_vertex) = (self.view.tool, self.view.snap.vertex);
        if let Some(outcome) = self.view.tools.get_mut(tool).click(hit, snap_vertex) {
            let palette = self.palette;
            self.view.tool_outcome(self.gpu, &palette, outcome);
            return;
        }
        let Some(d) = self.view.doc.as_ref() else { return };
        // objects on a locked layer, and deleted ones, are not selected in the views
        let _ = d;
        let entity = entity.filter(|&k| self.view.selectable(k));
        if add {
            let Some(k) = entity else { return };
            let group = d.with_group(k);
            let mut list = self.view.selection.clone();
            if list.contains(&k) {
                list.retain(|x| !group.contains(x));
            } else {
                list.retain(|x| !group.contains(x));
                list.extend(group.iter().filter(|&&x| x != k));
                list.push(k);
            }
            self.view.set_selection(self.gpu, list, true);
        } else {
            self.view.select(self.gpu, entity, true);
        }
    }

    /// The gizmo, the box, the ruler and the gizmo toolbar over a viewport.
    pub(super) fn edit_overlay(&mut self, ui: &mut egui::Ui, rect: Rect, projection: Projection, cam: &Camera) {
        let p = self.palette;
        let painter = ui.painter().with_clip_rect(rect);
        let view = Self::view_of(rect, cam);
        let to_screen = |s: [f64; 2]| rect.min + Vec2::new(s[0] as f32, s[1] as f32);
        // the gizmo
        if let Some(f) = self.view.gizmo_frame() {
            let active = self
                .view
                .pointer
                .drag
                .as_ref()
                .map(|d| d.handle)
                .or(self.view.pointer.hover);
            let axis = |k: usize| [p.axis_x, p.axis_y, p.axis_z].get(k).copied().unwrap_or(p.ink);
            for (h, pts) in gizmo::handles(&view, &f, self.view.tool) {
                let hot = active == Some(h);
                let pts: Vec<Pos2> = pts.into_iter().map(to_screen).collect();
                match h {
                    Handle::Axis(k) => {
                        let c = if hot { p.view_selection } else { axis(k) };
                        if let [a, b] = pts.as_slice() {
                            painter.line_segment([*a, *b], Stroke::new(3.0, c));
                            painter.circle_filled(*b, 5.0, c);
                        }
                    }
                    Handle::Plane(k) => {
                        let c = if hot { p.view_selection } else { axis(k) };
                        painter.add(egui::Shape::convex_polygon(
                            pts.clone(),
                            c.gamma_multiply(0.35),
                            Stroke::new(1.0, c),
                        ));
                    }
                    Handle::Centre => {
                        if let Some(c) = pts.first() {
                            let col = if hot { p.view_selection } else { p.ink };
                            painter.rect_stroke(
                                Rect::from_center_size(*c, Vec2::splat(12.0)),
                                CornerRadius::same(2),
                                Stroke::new(2.0, col),
                                egui::StrokeKind::Middle,
                            );
                        }
                    }
                    Handle::Ring(k) => {
                        let c = if hot { p.view_selection } else { axis(k) };
                        painter.add(egui::Shape::line(pts, Stroke::new(if hot { 3.0 } else { 2.0 }, c)));
                    }
                }
            }
        }
        // the selection box
        if let (Some((pr, from)), Some(to)) = (self.view.pointer.box_from, ui.input(|i| i.pointer.hover_pos()))
            && pr == projection
        {
            let r = Rect::from_two_pos(from, to);
            painter.rect_filled(r, CornerRadius::ZERO, p.view_selection.gamma_multiply(0.12));
            painter.rect_stroke(
                r,
                CornerRadius::ZERO,
                Stroke::new(1.0, p.view_selection),
                egui::StrokeKind::Middle,
            );
        }
        // what the tools hold (the ruler)
        self.view.tools.draw(
            &crate::tool::Draw {
                painter: &painter,
                projection,
                rect,
                view,
                hover: ui.input(|i| i.pointer.hover_pos()),
                palette: &p,
            },
            crate::tool::Layer::Edit,
        );
        // the gizmo toolbar, along the top of the view
        // (it wraps onto a second row in a narrow view)
        let bar = Rect::from_min_size(rect.min + Vec2::splat(6.0), Vec2::new(rect.width() - 12.0, 64.0));
        let mut ui2 = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(bar)
                .layout(Layout::top_down(Align::Min)),
        );
        egui::Frame::new()
            .fill(p.surface_2.gamma_multiply(0.94))
            .stroke(Stroke::new(1.0, p.line))
            .corner_radius(CornerRadius::same(4))
            .inner_margin(Margin::symmetric(6, 2))
            .show(&mut ui2, |ui| {
                ui.horizontal_wrapped(|ui| self.toolbar(ui));
            });
    }

    /// The gizmo toolbar: the tool, the axes, snapping and the sink depth.
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let v = &mut *self.view;
        ui.spacing_mut().item_spacing.x = 6.0;
        // the tools with a key, in their order (round PR1b-0: from the tools themselves)
        let bar: Vec<(Tool, &str)> = v.tools.all().iter().filter_map(|t| Some((t.id(), t.key()?))).collect();
        for (t, key) in bar {
            if ui
                .selectable_label(v.tool == t, t.name())
                .on_hover_text(format!("{} ({key})", t.name()))
                .clicked()
            {
                v.tool = t;
                v.tools.choose(t);
            }
        }
        ui.separator();
        let space = if v.space == Space::World { "World" } else { "Local" };
        if ui
            .button(space)
            .on_hover_text("The gizmo's axes: the map's, or the object's own (X)")
            .clicked()
        {
            v.space = if v.space == Space::World {
                Space::Local
            } else {
                Space::World
            };
        }
        ui.separator();
        ui.checkbox(&mut v.snap.grid, "Grid")
            .on_hover_text("Moves go in steps of the grid (G)");
        ui.add(
            egui::DragValue::new(&mut v.snap.grid_size)
                .range(0.05..=100.0)
                .speed(0.05)
                .suffix(" m"),
        );
        ui.checkbox(&mut v.snap.angle, "Angle")
            .on_hover_text("Turns go in steps of the angle");
        ui.add(
            egui::DragValue::new(&mut v.snap.angle_step)
                .range(1.0..=90.0)
                .speed(0.5)
                .suffix(" deg"),
        );
        ui.checkbox(&mut v.snap.vertex, "Vertex")
            .on_hover_text("Moves snap to the corner under the pointer; hold Shift for the edge (V)");
        ui.separator();
        ui.checkbox(&mut v.snap.surface, "Surface")
            .on_hover_text("Moves put objects on the terrain or mesh below them (B)");
        ui.label(RichText::new("Sink").color(p.ink_muted));
        sink_editor(ui, &mut v.snap.placement, "toolbar sink");
    }

    /// The properties' editing part: the transform as numbers, the surface placement and sink depth, the layer,
    /// folder and group, and the typed fields.
    pub(super) fn edit_properties(&mut self, ui: &mut egui::Ui, e: &scene::Entity) {
        let p = self.palette;
        let m = self.metrics;
        let gpu = self.gpu;
        let k = e.index;
        let Some(d) = self.view.doc.as_ref() else { return };
        ui.add_space(m.space_3);
        let g = ui.painter().layout_job(caption("Edit", p.ink_muted));
        let (r, _) = ui.allocate_exact_size(g.size(), Sense::hover());
        ui.painter().galley(r.min, g, p.ink_muted);
        ui.add_space(m.space_1);
        if let Some(why) = d.why_not(k) {
            ui.label(RichText::new(format!("Not editable: {why}.")).color(p.ink_muted));
            return;
        }
        // round SG1: a static group member says which group and which member it is
        if let Some(about) = crate::memberedit::describe(d, k) {
            ui.label(RichText::new(about).color(p.ink_muted));
            ui.add_space(m.space_1);
        }
        let Some(mat) = e.matrix else { return };
        // position and yaw as numbers: previewed while dragged, one operation when let go
        let mut pos = math::row(&mat, 3);
        let mut yaw = math::yaw_degrees(&mat);
        let mut changed = None;
        let mut done = false;
        egui::Grid::new("edit transform")
            .num_columns(4)
            .spacing([m.space_2, m.space_1])
            .show(ui, |ui| {
                ui.label(RichText::new("Position").color(p.ink_muted));
                let mut moved = false;
                for ((name, c), value) in [("X", p.axis_x), ("Y", p.axis_y), ("Z", p.axis_z)]
                    .into_iter()
                    .zip(pos.iter_mut())
                {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(name).text_style(style("body-strong")).color(c));
                        let r = ui.add(egui::DragValue::new(value).speed(0.05).max_decimals(3).suffix(" m"));
                        moved |= r.changed();
                        done |= r.drag_stopped() || r.lost_focus();
                    });
                }
                if moved {
                    changed = Some(math::at(&mat, pos));
                }
                ui.end_row();
                ui.label(RichText::new("Yaw").color(p.ink_muted));
                let r = ui.add(egui::DragValue::new(&mut yaw).speed(0.5).max_decimals(2).suffix(" deg"));
                if r.changed() {
                    let now = math::yaw_degrees(&mat);
                    changed = Some(math::rotate_about(
                        &mat,
                        math::row(&mat, 3),
                        [0.0, 1.0, 0.0],
                        (yaw - now).to_radians(),
                    ));
                }
                done |= r.drag_stopped() || r.lost_focus();
                ui.end_row();
            });
        if let Some(now) = changed {
            if self.view.props.prop_from.is_none_or(|(x, _)| x != k) {
                self.view.props.prop_from = Some((k, mat));
            }
            if let Some(d) = self.view.doc.as_mut() {
                d.preview(&[(k, now)]);
            }
            self.view.refresh(gpu, &p);
        }
        if done && let Some((x, from)) = self.view.props.prop_from.take() {
            let to = self.view.matrix(x).unwrap_or(from);
            // the position as it was: a turn
            let rotate = {
                let (a, b) = (math::row(&from, 3), math::row(&to, 3));
                a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
            };
            self.view
                .commit_transforms(gpu, &p, rotate, &[(x, from, to, None)], "Typed");
        }
        // the surface placement, kept with the object
        ui.add_space(m.space_2);
        let Some(d) = self.view.doc.as_ref() else { return };
        let stored = d
            .guid(k)
            .and_then(|g| d.org.placement.get(&g.to_ascii_uppercase()).copied());
        ui.label(
            RichText::new(match stored {
                Some(s) => format!("On a surface, {}", s.describe()),
                None => "Not placed on a surface yet".into(),
            })
            .color(p.ink_muted),
        );
        let mut place = match self.view.props.prop_place {
            Some((x, pl)) if x == k => pl,
            _ => stored.unwrap_or(self.view.snap.placement),
        };
        let shown = place;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Sink").color(p.ink_muted));
            sink_editor(ui, &mut place, "object sink");
        });
        // kept only while the user changes it (else it follows the object's stored placement)
        if place != shown {
            self.view.props.prop_place = Some((k, place));
        }
        let place_btn = ui.button("Place on surface").on_hover_text(
            "Puts this object on the terrain or mesh below it with this sink depth; it keeps the depth when moved",
        );
        // layer, folder, group
        let org: &Organisation = &d.org;
        let guid = d.guid(k).unwrap_or_default().to_owned();
        let layer_now = org.layer(&guid).map_or_else(String::new, |l| l.name.clone());
        let mut layer_pick = layer_now.clone();
        let names: Vec<String> = org.layers.iter().map(|l| l.name.clone()).collect();
        let group = org.group_of(&guid).map(|(n, _)| n.to_owned());
        let folder_now = org
            .folder_of
            .get(&guid.to_ascii_uppercase())
            .cloned()
            .unwrap_or_default();
        let mut folder_set = None;
        ui.add_space(m.space_2);
        egui::Grid::new("edit organisation")
            .num_columns(2)
            .spacing([m.space_2, m.space_1])
            .show(ui, |ui| {
                ui.label(RichText::new("Layer").color(p.ink_muted));
                egui::ComboBox::from_id_salt("object layer")
                    .selected_text(if layer_pick.is_empty() {
                        "None".to_owned()
                    } else {
                        layer_pick.clone()
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut layer_pick, String::new(), "None");
                        for n in &names {
                            ui.selectable_value(&mut layer_pick, n.clone(), n);
                        }
                    });
                ui.end_row();
                ui.label(RichText::new("Folder").color(p.ink_muted));
                ui.horizontal(|ui| {
                    if self.view.props.folder_input.is_empty() && !folder_now.is_empty() {
                        self.view.props.folder_input.clone_from(&folder_now);
                    }
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.view.props.folder_input)
                            .hint_text(RichText::new("Buildings/North").color(p.ink_faint))
                            .desired_width(140.0),
                    );
                    if (r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))) || ui.button("Set").clicked() {
                        folder_set = Some(self.view.props.folder_input.trim().to_owned());
                    }
                });
                ui.end_row();
                ui.label(RichText::new("Group").color(p.ink_muted));
                ui.label(RichText::new(group.unwrap_or_else(|| "None".into())).color(p.ink));
                ui.end_row();
            });
        if place_btn.clicked() {
            let items = self.view.placed_on_surface(&[k], Some(place));
            self.view.commit_transforms(gpu, &p, false, &items, "Place on surface");
        }
        let targets: Vec<String> = {
            let d = self.view.doc.as_ref();
            self.view
                .editable_selection()
                .iter()
                .filter_map(|&x| d.and_then(|d| d.guid(x)).map(str::to_owned))
                .collect()
        };
        if layer_pick != layer_now {
            self.view.commit(
                gpu,
                &p,
                Op::AssignLayer {
                    layer: layer_pick,
                    guids: targets.clone(),
                },
                "",
            );
        }
        if let Some(folder) = folder_set
            && folder != folder_now
        {
            self.view.commit(gpu, &p, Op::SetFolder { folder, guids: targets }, "");
        }
        self.fields(ui, k);
    }

    /// The typed fields of the primary object (the schema's types), each change one operation.
    fn fields(&mut self, ui: &mut egui::Ui, k: usize) {
        let p = self.palette;
        let m = self.metrics;
        let Some(d) = self.view.doc.as_ref() else { return };
        let rows = d.fields(k);
        if rows.is_empty() {
            return;
        }
        let guid = d.guid(k).unwrap_or_default().to_owned();
        ui.add_space(m.space_3);
        let g = ui.painter().layout_job(caption("Fields", p.ink_muted));
        let (r, _) = ui.allocate_exact_size(g.size(), Sense::hover());
        ui.painter().galley(r.min, g, p.ink_muted);
        ui.add_space(m.space_1);
        let mut set: Option<(String, FieldValue, Option<FieldValue>)> = None;
        egui::Grid::new("fields")
            .num_columns(2)
            .spacing([m.space_2, m.space_1])
            .show(ui, |ui| {
                for row in &rows {
                    ui.label(RichText::new(&row.path).color(p.ink_muted))
                        .on_hover_text(format!("{} ({})", row.path, row.ty.name()));
                    let editing = self
                        .view
                        .props
                        .field_edit
                        .as_ref()
                        .filter(|(x, f, _)| *x == k && *f == row.path)
                        .map(|(_, _, t)| t.clone());
                    match (&row.ty, &row.value) {
                        (FieldType::Bool, Some(FieldValue::Bool(b))) => {
                            let mut v = *b;
                            if ui.checkbox(&mut v, "").changed() {
                                set = Some((row.path.clone(), FieldValue::Bool(v), row.value.clone()));
                            }
                        }
                        (FieldType::Int, Some(FieldValue::Int(i))) => {
                            let mut v = *i;
                            let r = ui.add(egui::DragValue::new(&mut v).speed(0.1));
                            if r.changed() && !r.dragged() || r.drag_stopped() && v != *i {
                                set = Some((row.path.clone(), FieldValue::Int(v), row.value.clone()));
                            }
                        }
                        (FieldType::Float, Some(FieldValue::Float(f))) => {
                            let mut v = *f;
                            let r = ui.add(egui::DragValue::new(&mut v).speed(0.01).max_decimals(4));
                            if r.changed() && !r.dragged() || r.drag_stopped() && v.to_bits() != f.to_bits() {
                                set = Some((row.path.clone(), FieldValue::Float(v), row.value.clone()));
                            }
                        }
                        (FieldType::Text | FieldType::Ref, Some(FieldValue::Text(t) | FieldValue::Ref(t))) => {
                            let mut text = editing.unwrap_or_else(|| t.clone());
                            ui.horizontal(|ui| {
                                let r = ui.add(
                                    egui::TextEdit::singleline(&mut text)
                                        .desired_width(150.0)
                                        .font(TextStyle::Monospace),
                                );
                                if r.changed() {
                                    self.view.props.field_edit = Some((k, row.path.clone(), text.clone()));
                                }
                                if r.lost_focus() && text != *t {
                                    let v = if row.ty == FieldType::Ref {
                                        FieldValue::Ref(text.clone())
                                    } else {
                                        FieldValue::Text(text.clone())
                                    };
                                    set = Some((row.path.clone(), v, row.value.clone()));
                                    self.view.props.field_edit = None;
                                }
                                if !row.choices.is_empty() {
                                    egui::ComboBox::from_id_salt(("choices", &row.path))
                                        .selected_text("")
                                        .width(18.0)
                                        .show_ui(ui, |ui| {
                                            for c in row.choices.iter().take(40) {
                                                if ui.selectable_label(c == t, c).clicked() && c != t {
                                                    set = Some((
                                                        row.path.clone(),
                                                        FieldValue::Text(c.clone()),
                                                        row.value.clone(),
                                                    ));
                                                }
                                            }
                                        });
                                }
                            });
                        }
                        (ty, v) => {
                            let text = v.as_ref().map_or_else(|| ty.name(), FieldValue::text);
                            ui.label(RichText::new(text).text_style(style("value")).color(p.ink_faint))
                                .on_hover_text("Not editable here yet");
                        }
                    }
                    ui.end_row();
                }
            });
        if let Some((field, to, from)) = set {
            let gpu = self.gpu;
            self.view.commit(
                gpu,
                &p,
                Op::SetField {
                    items: vec![FieldSet { guid, field, from, to }],
                },
                "",
            );
        }
    }

    /// The History panel: every operation with who and when; click one to undo or redo to just after it.
    pub(super) fn history(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let m = self.metrics;
        let Some(d) = self.view.doc.as_ref() else {
            self.header(ui, "History", "");
            ui.label(RichText::new("Open a project to edit; its operations are listed here.").color(p.ink_muted));
            return;
        };
        let applied = d.project.operations.len();
        self.header(
            ui,
            "History",
            &format!(
                "{applied} operation(s){}",
                if d.project.redo.is_empty() {
                    String::new()
                } else {
                    format!(", {} undone", d.project.redo.len())
                }
            ),
        );
        let mut go = None;
        let rows: Vec<(String, String, bool)> = std::iter::once((
            format!("Base: {}", d.project.base.partition),
            "as loaded".to_owned(),
            true,
        ))
        .chain(
            d.project
                .operations
                .iter()
                .map(|o| (o.name(), format!("{}  {}", o.by, o.when), true)),
        )
        .chain(
            d.project
                .redo
                .iter()
                .rev()
                .map(|o| (o.name(), format!("{}  {}  (undone)", o.by, o.when), false)),
        )
        .collect();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for (i, (name, when, done)) in rows.iter().enumerate() {
                    let (rect, resp) =
                        ui.allocate_exact_size(Vec2::new(ui.available_width(), m.row + 14.0), Sense::click());
                    let current = i == applied;
                    if current {
                        ui.painter()
                            .rect_filled(rect, CornerRadius::same(m.radius_sm as u8), p.accent_subtle);
                        ui.painter().rect_filled(
                            Rect::from_min_size(rect.min, Vec2::new(2.0, rect.height())),
                            CornerRadius::ZERO,
                            p.accent,
                        );
                    } else if resp.hovered() {
                        ui.painter().rect_filled(rect, CornerRadius::ZERO, p.surface_2);
                    }
                    let ink = if *done { p.ink } else { p.ink_faint };
                    ui.painter().text(
                        rect.left_top() + Vec2::new(m.space_2, 2.0),
                        Align2::LEFT_TOP,
                        name,
                        FontId::proportional(13.0),
                        ink,
                    );
                    ui.painter().text(
                        rect.left_bottom() + Vec2::new(m.space_2, -2.0),
                        Align2::LEFT_BOTTOM,
                        when,
                        FontId::proportional(10.5),
                        p.ink_muted,
                    );
                    if resp
                        .on_hover_text(if *done {
                            "Undo to just after this"
                        } else {
                            "Redo up to this"
                        })
                        .clicked()
                    {
                        go = Some(i);
                    }
                }
            });
        if let Some(i) = go {
            let palette = self.palette;
            if let Some(d) = self.view.doc.as_mut() {
                d.go_to(i);
                self.view.message = format!("History: {i} operation(s) applied");
            }
            self.view.pointer.drag = None;
            self.view.refresh(self.gpu, &palette);
        }
    }

    /// The Layers panel: hide, isolate, lock, colour; add the selection; remove.
    pub(super) fn layers(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let m = self.metrics;
        let Some(d) = self.view.doc.as_ref() else {
            self.header(ui, "Layers", "");
            ui.label(RichText::new("Open a project to use layers.").color(p.ink_muted));
            return;
        };
        let layers = d.org.layers.clone();
        let count = |name: &str| d.org.layer_of.values().filter(|l| *l == name).count();
        let counts: Vec<usize> = layers.iter().map(|l| count(&l.name)).collect();
        let sel: Vec<String> = self
            .view
            .editable_selection()
            .iter()
            .filter_map(|&k| d.guid(k).map(str::to_owned))
            .collect();
        self.header(ui, "Layers", &format!("{}", layers.len()));
        let mut ops: Vec<Op> = Vec::new();
        let mut new_layer = false;
        egui::Frame::new()
            .inner_margin(Margin::same(m.space_2 as i8))
            .show(ui, |ui| {
                if ui
                    .add_enabled(!sel.is_empty(), egui::Button::new("New layer from selection"))
                    .on_disabled_hover_text("Select objects to put on the new layer")
                    .clicked()
                {
                    new_layer = true;
                }
                ui.add_space(m.space_2);
                if layers.is_empty() {
                    ui.label(RichText::new("No layers yet. Objects on no layer are always shown.").color(p.ink_muted));
                }
                for (l, n) in layers.iter().zip(counts) {
                    let mut to = l.state.clone();
                    ui.horizontal(|ui| {
                        let [r, g, b] = l.state.colour;
                        let (sw, resp) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::click());
                        ui.painter()
                            .rect_filled(sw, CornerRadius::same(2), Color32::from_rgb(r, g, b));
                        if resp.on_hover_text("Next colour").clicked() {
                            to.colour = colour_after(l.state.colour, 1);
                        }
                        ui.label(RichText::new(&l.name).color(if l.state.hidden { p.ink_faint } else { p.ink }));
                        let mut state = Vec::new();
                        if l.state.hidden {
                            state.push("hidden");
                        }
                        if l.state.isolated {
                            state.push("isolated");
                        }
                        if l.state.locked {
                            state.push("locked");
                        }
                        let what = if n == 1 {
                            "1 object".to_owned()
                        } else {
                            format!("{n} objects")
                        };
                        let tail = if state.is_empty() {
                            what
                        } else {
                            format!("{what}, {}", state.join(", "))
                        };
                        ui.label(RichText::new(tail).text_style(style("small")).color(p.ink_muted));
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.add_space(18.0);
                        let toggles = [
                            (&mut to.hidden, "Show", "Hide", "Hidden objects stay in the build"),
                            (
                                &mut to.isolated,
                                "Stop isolating",
                                "Isolate",
                                "Only isolated layers are shown",
                            ),
                            (
                                &mut to.locked,
                                "Unlock",
                                "Lock",
                                "Locked objects cannot be selected in the views",
                            ),
                        ];
                        for (flag, on, off, help) in toggles {
                            if ui
                                .small_button(if *flag { on } else { off })
                                .on_hover_text(help)
                                .clicked()
                            {
                                *flag = !*flag;
                            }
                        }
                        if ui
                            .add_enabled(!sel.is_empty(), egui::Button::new("Add selection").small())
                            .on_hover_text("Puts the selection on this layer")
                            .clicked()
                        {
                            ops.push(Op::AssignLayer {
                                layer: l.name.clone(),
                                guids: sel.clone(),
                            });
                        }
                        if ui
                            .small_button("Remove")
                            .on_hover_text("Its objects go back to no layer")
                            .clicked()
                        {
                            ops.push(Op::RemoveLayer { layer: l.name.clone() });
                        }
                    });
                    if to != l.state {
                        ops.push(Op::SetLayer {
                            layer: l.name.clone(),
                            from: l.state.clone(),
                            to,
                        });
                    }
                    ui.add_space(m.space_1);
                }
            });
        let gpu = self.gpu;
        if new_layer {
            let palette = self.palette;
            self.view.edit_action(gpu, &palette, Action::NewLayer);
        }
        for op in ops {
            self.view.commit(gpu, &p, op, "");
        }
    }
}

/// The sink depth editor: the depth, its unit (metres or a share of the object's height), along the normal or
/// straight down, turned to the normal.
fn sink_editor(ui: &mut egui::Ui, place: &mut Placement, id: &str) {
    match place.unit {
        SinkUnit::Metres => {
            ui.add(
                egui::DragValue::new(&mut place.sink)
                    .range(-50.0..=50.0)
                    .speed(0.05)
                    .max_decimals(2)
                    .suffix(" m"),
            );
        }
        SinkUnit::Fraction => {
            let mut pct = place.sink * 100.0;
            if ui
                .add(
                    egui::DragValue::new(&mut pct)
                        .range(-100.0..=100.0)
                        .speed(0.5)
                        .max_decimals(1)
                        .suffix(" %"),
                )
                .changed()
            {
                place.sink = pct / 100.0;
            }
        }
    }
    egui::ComboBox::from_id_salt(id)
        .selected_text(match place.unit {
            SinkUnit::Metres => "metres",
            SinkUnit::Fraction => "of height",
        })
        .width(76.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut place.unit, SinkUnit::Metres, "metres");
            ui.selectable_value(&mut place.unit, SinkUnit::Fraction, "of height");
        });
    ui.checkbox(&mut place.along_normal, "Along normal")
        .on_hover_text("Sink along the surface normal (off: straight down)");
    ui.checkbox(&mut place.align_to_normal, "Align")
        .on_hover_text("Turn the object's up axis to the surface normal");
}

/// The outliner's Folders and Groups (a project's own organisation), above the kinds.
pub(super) fn folders_section(
    ui: &mut egui::Ui,
    v: &ViewState,
    p: &Palette,
    m: &Metrics,
    query: &str,
    clicked: &mut Option<(usize, bool)>,
) {
    let Some(d) = &v.doc else { return };
    let q = query.trim().to_lowercase();
    let mut folders: std::collections::BTreeMap<&str, Vec<usize>> = std::collections::BTreeMap::new();
    #[allow(
        clippy::iter_over_hash_type,
        reason = "into a sorted map; each folder's members are sorted before they are shown"
    )]
    for (g, f) in &d.org.folder_of {
        if let Some(k) = d.index(g) {
            folders.entry(f.as_str()).or_default().push(k);
        }
    }
    let groups: Vec<(&str, Vec<usize>)> = d
        .org
        .groups
        .iter()
        .map(|(n, ms)| (n.as_str(), ms.iter().filter_map(|g| d.index(g)).collect()))
        .collect();
    if folders.is_empty() && groups.is_empty() {
        return;
    }
    for (title, list) in [("Folders", folders.into_iter().collect::<Vec<_>>()), ("Groups", groups)] {
        if list.is_empty() {
            continue;
        }
        let g = ui.painter().layout_job(caption(title, p.ink_muted));
        let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), m.row), Sense::hover());
        ui.painter().galley(
            Pos2::new(r.left() + m.space_2, r.center().y - g.size().y / 2.0),
            g,
            p.ink_muted,
        );
        for (name, mut members) in list {
            members.sort_unstable();
            let id = ui.make_persistent_id(("outliner folder", title, name));
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
                .show_header(ui, |ui| {
                    ui.label(RichText::new(name).color(p.ink));
                    ui.label(
                        RichText::new(group_digits(members.len() as u64))
                            .text_style(style("value"))
                            .color(p.ink_muted),
                    );
                })
                .body(|ui| {
                    for k in members {
                        let Some(e) = d.scene.entities.get(k) else { continue };
                        let label = model::display_name(e);
                        if !q.is_empty() && !label.to_lowercase().contains(&q) {
                            continue;
                        }
                        let sel = v.selection.contains(&k);
                        let resp = ui.selectable_label(sel, RichText::new(label).color(p.ink));
                        if resp.double_clicked() {
                            *clicked = Some((k, true));
                        } else if resp.clicked() {
                            *clicked = Some((k, false));
                        }
                    }
                });
        }
    }
}
