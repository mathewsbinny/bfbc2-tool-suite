//! The Export window (`EDITOR_ARCHITECTURE.md` 3.12, "The editor"): the project exported as a NEW multiplayer level,
//! staged in OPFOR's `work\mapedit\<level>\stage` and checked offline (`bc2_levelwrite::export`). The window never
//! writes the game copy: installing is Project OPFOR's `mapedit_install.py --apply`, shown with its command.
//!
//! The form: the level's name (what the launcher and the server browser show) and id, the game mode (Conquest; the
//! other modes need E4's mode definitions), the teams (the mode donor's factions), the bases' and flags' places and
//! the combat area kept. The settings are kept beside the project as `<project>.export.json` (data only). The checks
//! before an export run as the form changes; Export stays disabled while one fails, with the reason.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use bc2_levelwrite::MapProject;
use bc2_levelwrite::export::{self, ExportSpec, Game};
use bc2_levelwrite::mpconvert::{Anchor, LayerSpec};
use eframe::egui;

/// The modes the export builds now, and the stock donor of each one's layer.
pub const MODES: [(&str, &str, &str); 1] = [("conquest", "Conquest", "mp_009cq")];

/// The window's state.
#[derive(Default)]
pub struct ExportWindow {
    /// Shown.
    pub open: bool,
    /// The project file.
    pub project: Option<PathBuf>,
    /// The game folder (OPFOR's `game\`) and OPFOR's folder.
    pub opfor: Option<PathBuf>,
    /// The form.
    pub spec: Option<ExportSpec>,
    /// The last export's lines (report, then the checks).
    pub log: Vec<String>,
    running: Option<Receiver<Result<Vec<String>, String>>>,
}

/// The settings file beside a project.
#[must_use]
pub fn spec_path(project: &Path) -> PathBuf {
    let mut s = project.as_os_str().to_owned();
    s.push(".export.json");
    PathBuf::from(s)
}

/// A first form for a project: its name as the level's, Conquest with `MP_009CQ`'s layer, the bases and flags where
/// the donor has them (to be moved), no combat area chosen yet.
#[must_use]
pub fn default_spec(project: &MapProject) -> ExportSpec {
    let id: String = project
        .name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    let a = |from: &str| Anchor {
        from: from.into(),
        at: [0.0; 3],
    };
    ExportSpec {
        format: export::FORMAT.into(),
        version: export::VERSION,
        level: format!("MAPEDIT_{}_CQ", id.trim_matches('_')),
        title: format!("{} (Conquest)", project.name),
        mode: "conquest".into(),
        layer: LayerSpec {
            donor: "mp_009cq".into(),
            bases: vec![a("US Base"), a("RUS Base")],
            flags: vec![a("A: Crash Site"), a("B: Hill")],
            combat_area: String::new(),
        },
    }
}

/// The checks before an export: each failing one's reason (empty: Export can run).
#[must_use]
pub fn checks(spec: &ExportSpec, game: Option<&Game>, saved: bool) -> Vec<String> {
    let mut out = Vec::new();
    if !saved {
        out.push("Save the project first (the export builds the saved project)".into());
    }
    match spec.level_id() {
        Err(e) => out.push(e.to_string()),
        Ok(id) => {
            if let Some(g) = game {
                let low = id.to_ascii_lowercase();
                let exists = !g.level_bundles(&low, "level").is_empty();
                if exists && !export::is_custom(g, &low) {
                    out.push(format!("{id} is a level of the game copy: choose another name"));
                }
            }
        }
    }
    if spec.title.trim().is_empty() {
        out.push("Give the map a name".into());
    }
    if !MODES.iter().any(|m| m.0 == spec.mode) {
        out.push(format!("The {} mode is not built yet (Conquest is)", spec.mode));
    }
    if spec.layer.bases.len() != 2 {
        out.push("Conquest needs two bases, one per team".into());
    }
    if spec.layer.flags.is_empty() {
        out.push("Conquest needs at least one flag".into());
    }
    if spec.layer.combat_area.trim().len() != 36 {
        out.push("Choose the combat area the map keeps (its guid)".into());
    }
    if game.is_none() {
        out.push("No game copy found (OPFOR's game folder)".into());
    }
    out
}

impl ExportWindow {
    /// Opens the window for a project (its settings read from beside it, else a first form).
    pub fn open_for(&mut self, project: &Path, opfor: Option<&Path>) {
        self.open = true;
        self.project = Some(project.to_path_buf());
        self.opfor = opfor.map(Path::to_path_buf);
        let from_file = std::fs::read_to_string(spec_path(project))
            .ok()
            .and_then(|t| ExportSpec::from_json(&t).ok());
        self.spec = from_file.or_else(|| MapProject::load(project).ok().map(|p| default_spec(&p)));
    }

    fn game(&self) -> Option<Game> {
        Game::open(&self.opfor.as_ref()?.join("game")).ok()
    }

    fn start(&mut self) {
        let (Some(project), Some(opfor), Some(spec)) = (self.project.clone(), self.opfor.clone(), self.spec.clone())
        else {
            return;
        };
        if let Ok(json) = serde_json::to_string_pretty(&spec) {
            let _ = std::fs::write(spec_path(&project), json + "\n");
        }
        let (tx, rx) = channel();
        self.running = Some(rx);
        self.log = vec!["Exporting...".into()];
        std::thread::spawn(move || {
            let run = || -> Result<Vec<String>, String> {
                let game = Game::open(&opfor.join("game")).map_err(|e| e.to_string())?;
                let p = MapProject::load(&project).map_err(|e| e.to_string())?;
                let lv = spec.level_id().map_err(|e| e.to_string())?.to_ascii_lowercase();
                let stage = opfor.join("work").join("mapedit").join(&lv).join("stage");
                let rec = export::build(&game, &p, &project.display().to_string(), &spec, &stage)
                    .map_err(|e| e.to_string())?;
                let r = export::check(&game, &stage).map_err(|e| e.to_string())?;
                let mut out = rec.report;
                out.extend(
                    r.checks
                        .iter()
                        .map(|(ok, w)| format!("{} {w}", if *ok { "ok" } else { "FAILED" })),
                );
                out.push(if r.ok() {
                    format!(
                        "Staged in {} - nothing in the game copy changed. To install it (the game closed): python \
                         src\\tools\\mapedit\\mapedit_install.py --apply {}",
                        stage.display(),
                        stage.display()
                    )
                } else {
                    "A check failed: the stage is not ready to install".into()
                });
                Ok(out)
            };
            let _ = tx.send(run());
        });
    }

    fn poll(&mut self) {
        if let Some(rx) = &self.running
            && let Ok(r) = rx.try_recv()
        {
            self.log = match r {
                Ok(lines) => lines,
                Err(e) => vec![format!("Not exported: {e}")],
            };
            self.running = None;
        }
    }
}

fn anchor_row(ui: &mut egui::Ui, a: &mut Anchor) {
    ui.horizontal(|ui| {
        ui.label(&a.from);
        ui.label("x");
        ui.add(egui::DragValue::new(&mut a.at[0]).speed(1.0));
        ui.label("z");
        ui.add(egui::DragValue::new(&mut a.at[2]).speed(1.0));
    });
}

/// Draws the window (when open).
pub fn window(ctx: &egui::Context, w: &mut ExportWindow) {
    if !w.open {
        return;
    }
    w.poll();
    let game = w.game();
    let saved = w
        .project
        .as_ref()
        .is_some_and(|p| !bc2_levelwrite::project::autosave_path(p).exists());
    let mut open = w.open;
    let mut go = false;
    egui::Window::new("Export as a multiplayer level")
        .open(&mut open)
        .resizable(true)
        .default_width(560.0)
        .show(ctx, |ui| {
            let Some(spec) = w.spec.as_mut() else {
                ui.label("Open a project first");
                return;
            };
            egui::Grid::new("export-form").num_columns(2).show(ui, |ui| {
                ui.label("Map name");
                ui.text_edit_singleline(&mut spec.title);
                ui.end_row();
                ui.label("Level id");
                ui.text_edit_singleline(&mut spec.level);
                ui.end_row();
                ui.label("Game mode");
                egui::ComboBox::from_id_salt("export-mode")
                    .selected_text(MODES.iter().find(|m| m.0 == spec.mode).map_or("?", |m| m.1))
                    .show_ui(ui, |ui| {
                        for (id, name, donor) in MODES {
                            if ui.selectable_label(spec.mode == id, name).clicked() {
                                spec.mode = id.into();
                                spec.layer.donor = donor.into();
                            }
                        }
                    });
                ui.end_row();
                ui.label("Teams");
                ui.label("US / RU (the mode layer's)");
                ui.end_row();
                ui.label("Combat area kept");
                ui.text_edit_singleline(&mut spec.layer.combat_area);
                ui.end_row();
            });
            ui.separator();
            ui.label("Bases");
            for a in &mut spec.layer.bases {
                anchor_row(ui, a);
            }
            ui.label("Flags");
            for a in &mut spec.layer.flags {
                anchor_row(ui, a);
            }
            ui.separator();
            let problems = checks(spec, game.as_ref(), saved);
            for p in &problems {
                ui.colored_label(ui.visuals().warn_fg_color, p);
            }
            let busy = w.running.is_some();
            let tip = problems
                .first()
                .cloned()
                .unwrap_or_else(|| "Builds and checks the new level; the game copy is not changed".into());
            if ui
                .add_enabled(problems.is_empty() && !busy, egui::Button::new("Export"))
                .on_hover_text(tip)
                .on_disabled_hover_text(problems.first().cloned().unwrap_or_else(|| "Exporting...".into()))
                .clicked()
            {
                go = true;
            }
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                for l in &w.log {
                    ui.monospace(l);
                }
            });
        });
    w.open = open;
    if go {
        w.start();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn checks_name_mode_and_layer() {
        let p = MapProject::new(
            "Aurora: north",
            "sp_01",
            "levels/SP_01",
            b"x",
            bc2_levelwrite::Profile::Bfbc2,
        );
        let mut s = default_spec(&p);
        assert_eq!(s.level, "MAPEDIT_AURORA__NORTH_CQ");
        let c = checks(&s, None, true);
        assert!(c.iter().any(|x| x.contains("combat area")));
        assert!(c.iter().any(|x| x.contains("game copy")));
        s.layer.combat_area = "E4C7B1C2-BB9F-481A-B3CC-F67E4E5CB088".into();
        s.level = "bad name".into();
        s.mode = "rush".into();
        let c = checks(&s, None, false);
        assert!(c.iter().any(|x| x.starts_with("Save the project")));
        assert!(c.iter().any(|x| x.contains("3 to 40")));
        assert!(c.iter().any(|x| x.contains("not built yet")));
        assert_eq!(
            spec_path(Path::new("a/b.mapedit.json")),
            PathBuf::from("a/b.mapedit.json.export.json")
        );
    }
}
