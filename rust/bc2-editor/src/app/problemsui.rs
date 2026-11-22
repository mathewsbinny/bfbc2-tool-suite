//! The Problems panel (round V1, `EDITOR_ARCHITECTURE.md` 9.1): the project's own problems (OP1's typed problems:
//! operations that do not apply, the base changed) and the export's rules checked live after every change
//! ([`crate::validation`]: the edit phase of `bc2_levelwrite::rules` and DICE's placement tendencies from round L1),
//! errors first. Each row shows its severity, its rule's code and what was found; a row naming objects has a "Go to"
//! link that selects the first one the scene has and moves every view to it, as F does. The loading notes and an
//! autosave failure stay below.

use super::{Margin, RichText, Sense, Tabs, Vec2, egui, status_line, status_shape, style};
use bc2_levelwrite::rules::Severity as RuleSeverity;

impl Tabs<'_> {
    pub(super) fn problems(&mut self, ui: &mut egui::Ui) {
        if let Some(d) = self.view.doc.as_ref() {
            self.view.live.refresh(d);
        }
        let rows = self
            .view
            .doc
            .as_ref()
            .map_or_default(|d| crate::validation::rows(d, &self.view.live));
        let (errors, warnings, notes) = crate::validation::counts(&rows);
        let summary = if rows.is_empty() {
            String::new()
        } else {
            let n = |k: usize, one: &str, many: &str| format!("{k} {}", if k == 1 { one } else { many });
            format!(
                "{} · {} · {}",
                n(errors, "error", "errors"),
                n(warnings, "warning", "warnings"),
                n(notes, "note", "notes")
            )
        };
        self.header(ui, "Problems", &summary);
        let p = self.palette;
        let m = self.metrics;
        let show_notes_id = egui::Id::new("problems: show notes");
        let mut show_notes: bool = ui.data(|d| d.get_temp(show_notes_id)).unwrap_or(true);
        let mut go: Option<usize> = None;
        egui::Frame::new()
            .inner_margin(Margin::same(m.space_3 as i8))
            .show(ui, |ui| {
                let autosave = self.view.doc.as_ref().and_then(|d| d.autosave_error.clone());
                if notes > 0
                    && ui
                        .checkbox(&mut show_notes, "Show notes (unlike DICE's maps, nothing wrong)")
                        .changed()
                {
                    ui.data_mut(|d| d.insert_temp(show_notes_id, show_notes));
                }
                if self.view.notes.is_empty() && rows.is_empty() && autosave.is_none() {
                    ui.label(
                        RichText::new(if self.view.doc.is_some() {
                            "No problems: every edit applies and every rule of the export holds."
                        } else {
                            "No problems. A read-only map has nothing to check."
                        })
                        .color(p.ink_muted),
                    );
                }
                egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
                    for (k, r) in rows.iter().enumerate() {
                        if r.severity == RuleSeverity::Note && !show_notes {
                            continue;
                        }
                        let (word, colour) = match r.severity {
                            RuleSeverity::Error => ("Error", p.status_error),
                            RuleSeverity::Warning => ("Warning", p.status_warn),
                            RuleSeverity::Note => ("Note", p.status_info),
                        };
                        let target = self
                            .view
                            .doc
                            .as_ref()
                            .and_then(|d| r.objects.iter().find_map(|g| d.index(g)));
                        ui.push_id(("problem", k), |ui| {
                            ui.horizontal_wrapped(|ui| {
                                let (rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                                status_shape(ui.painter(), rect.center(), 5.5, word, colour);
                                ui.label(RichText::new(word).text_style(style("body-strong")).color(colour));
                                ui.label(RichText::new(&r.code).monospace().color(p.ink_muted))
                                    .on_hover_text(rule_help(&r.code));
                                if let Some(e) = target
                                    && ui
                                        .link(RichText::new("Go to").color(p.accent))
                                        .on_hover_text("Select it and move the views to it (F)")
                                        .clicked()
                                {
                                    go = Some(e);
                                }
                                ui.label(RichText::new(&r.text).color(p.ink));
                            });
                        });
                    }
                    if let Some(a) = autosave {
                        status_line(
                            ui,
                            &p,
                            "Warning",
                            p.status_warn,
                            &format!("The autosave could not be written ({a}). Save the project to keep your changes."),
                        );
                    }
                    for n in &self.view.notes {
                        status_line(ui, &p, "Information", p.status_info, n);
                    }
                });
            });
        if let Some(e) = go {
            self.view.select(self.gpu, Some(e), true);
            self.view.focus_selected();
        }
        let _ = self.tokens;
    }
}

/// A rule's title, message and specification rules (the code's tooltip).
fn rule_help(code: &str) -> String {
    bc2_levelwrite::rules::rule(code).map_or_else(
        || code.to_owned(),
        |r| {
            format!(
                "{} - {} ({}, {})\n\n{}\n\nSpecification: {}",
                r.code,
                r.title,
                r.severity.word(),
                r.from,
                r.message,
                r.spec.join(", ")
            )
        },
    )
}
