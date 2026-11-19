//! The campaign-content checklist (round E3b, `EDITOR_ARCHITECTURE.md` 3.12): what an export of a campaign level
//! keeps. Offered when a campaign level is opened as a project's base (a level with campaign scripting and no choice
//! in its log yet) and from the Export window.
//!
//! The kinds are `bc2_levelwrite::mpconvert::STRIP_GROUPS`: the 61 campaign types the user may keep, grouped (AI
//! orders, squads, waypoints, cutscenes, objectives, triggers, screen texts, set pieces, campaign vehicles), and the
//! ten the game mode's layer replaces (shown, always removed). Each row counts the level's instances (the base with
//! the project's operations). The default, nothing kept, is DICE's own choice for `SP_005` -> `MP_SP_005CQ`.
//!
//! "Apply" adds the choice to the project's log as one operation (`Op::SetStrip`): undo and redo take it back like
//! any edit, and the export reads the log's last choice.

use std::collections::BTreeMap;

use bc2_levelwrite::mpconvert::{CAMPAIGN_VEHICLES, STRIP_GROUPS, Strip};
use bc2_levelwrite::{LevelWriter, Op};
use eframe::egui;

use crate::edit::Doc;

/// The window's state.
#[derive(Default)]
pub struct StripWindow {
    /// Shown.
    pub open: bool,
    /// The choice being made (applied with "Apply").
    pub choice: Strip,
    /// The level's instances of each campaign type (type name after the module's dot).
    pub counts: BTreeMap<String, usize>,
    /// The level (for the title).
    pub level: String,
    /// The project's choice when the window opened (Apply adds an operation only when it differs).
    applied: Strip,
}

/// The instances of each campaign type of a partition (the checklist's types only).
#[must_use]
pub fn type_counts(w: &LevelWriter) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for e in w.d.instances() {
        let t = String::from_utf8_lossy(w.d.attr(e, b"type").unwrap_or_default()).into_owned();
        let s = t.rsplit('.').next().unwrap_or(&t);
        if STRIP_GROUPS.iter().any(|g| g.types.contains(&s)) {
            *out.entry(s.to_owned()).or_default() += 1;
        }
    }
    out
}

/// Whether a level has campaign content a multiplayer export takes out (any choosable kind but the vehicles).
#[must_use]
pub fn is_campaign(counts: &BTreeMap<String, usize>) -> bool {
    STRIP_GROUPS
        .iter()
        .filter(|g| g.choosable && !g.types.contains(&CAMPAIGN_VEHICLES))
        .flat_map(|g| g.types.iter())
        .any(|t| counts.get(*t).copied().unwrap_or(0) > 0)
}

impl StripWindow {
    /// Opens the checklist on a project: its log's choice, the counts of its level.
    pub fn open_for(&mut self, doc: &Doc) {
        self.counts = doc.writer().map_or_default(type_counts);
        self.applied = doc.project.strip();
        self.choice = self.applied.clone();
        self.level = doc.project.base.level.to_ascii_uppercase();
        self.open = true;
    }

    /// Offers the checklist when a project opens on a campaign level whose log has no choice yet.
    pub fn offer(&mut self, doc: &Doc) {
        if doc.project.strip_chosen().is_none() {
            let counts = doc.writer().map_or_default(type_counts);
            if is_campaign(&counts) {
                self.open_for(doc);
            }
        }
    }

    fn count(&self, types: &[&str]) -> usize {
        types.iter().map(|t| self.counts.get(*t).copied().unwrap_or(0)).sum()
    }
}

/// Draws the checklist (when open); "Apply" commits the choice to `doc`'s log.
pub fn window(ctx: &egui::Context, w: &mut StripWindow, doc: Option<&mut Doc>) {
    if !w.open {
        return;
    }
    let mut open = w.open;
    let mut apply = false;
    let mut close = false;
    egui::Window::new("Campaign content in the export")
        .id(egui::Id::new("strip-checklist"))
        .open(&mut open)
        .resizable(true)
        .default_width(560.0)
        .default_height(720.0)
        .show(ctx, |ui| {
            ui.label(format!(
                "{} is a campaign level. A multiplayer export takes out what only the campaign uses; tick what to \
                 keep. The default (nothing ticked) is DICE's own choice when it made Heavy Metal's Conquest from \
                 the campaign.",
                if w.level.is_empty() { "This" } else { &w.level }
            ));
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height((ctx.content_rect().height() - 260.0).max(240.0))
                .show(ui, |ui| {
                    for g in STRIP_GROUPS {
                        let n = w.count(g.types);
                        let kept = g.types.iter().filter(|t| w.choice.keep.contains(**t)).count();
                        ui.horizontal(|ui| {
                            let mut all = g.choosable && kept == g.types.len();
                            let label = format!("{} ({n})", g.title);
                            let r = ui.add_enabled(g.choosable, egui::Checkbox::new(&mut all, label));
                            if r.changed() {
                                for t in g.types {
                                    if all {
                                        w.choice.keep.insert((*t).to_owned());
                                    } else {
                                        w.choice.keep.remove(*t);
                                    }
                                }
                            }
                            if g.choosable && kept > 0 && kept < g.types.len() {
                                ui.weak(format!("{kept} of {} kinds kept", g.types.len()));
                            }
                        });
                        ui.indent(g.id, |ui| {
                            ui.weak(g.note);
                            if g.types.len() > 1 {
                                egui::CollapsingHeader::new("Kinds")
                                    .id_salt(g.id)
                                    .default_open(false)
                                    .show(ui, |ui| {
                                        for t in g.types {
                                            let mut keep = w.choice.keep.contains(*t);
                                            let c = w.counts.get(*t).copied().unwrap_or(0);
                                            let label = format!("{} ({c})", t.trim_end_matches("Data"));
                                            if ui
                                                .add_enabled(g.choosable, egui::Checkbox::new(&mut keep, label))
                                                .changed()
                                            {
                                                if keep {
                                                    w.choice.keep.insert((*t).to_owned());
                                                } else {
                                                    w.choice.keep.remove(*t);
                                                }
                                            }
                                        }
                                    });
                            }
                        });
                    }
                });
            ui.separator();
            ui.label(format!("Kept: {}", w.choice.describe()));
            ui.horizontal(|ui| {
                if ui.button("DICE's choice (keep nothing)").clicked() {
                    w.choice = Strip::default();
                }
                let changed = w.choice != w.applied;
                if ui
                    .add_enabled(changed, egui::Button::new("Apply"))
                    .on_hover_text("Adds the choice to the project (undo takes it back)")
                    .on_disabled_hover_text("The project already has this choice")
                    .clicked()
                {
                    apply = true;
                }
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
        });
    w.open = open && !close;
    if apply && let Some(d) = doc {
        let from = d.project.strip();
        d.commit(
            Op::SetStrip {
                from,
                to: w.choice.clone(),
            },
            "Export",
        );
        w.applied = w.choice.clone();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn campaign_levels_are_recognised() {
        let mut c = BTreeMap::new();
        assert!(!is_campaign(&c));
        c.insert(CAMPAIGN_VEHICLES.to_owned(), 5);
        c.insert("SoldierSpawnEntityData".to_owned(), 9);
        assert!(!is_campaign(&c), "vehicles and spawns are in multiplayer levels too");
        c.insert("CutsceneEntityData".to_owned(), 1);
        assert!(is_campaign(&c));
        let w = StripWindow {
            counts: c,
            ..StripWindow::default()
        };
        assert_eq!(w.count(&[CAMPAIGN_VEHICLES, "CutsceneEntityData", "X"]), 6);
    }
}
