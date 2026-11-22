//! Round A1's panel: the Gameplay workspace's auto layout ([`crate::autolayout`]). A seed, Propose (on a worker
//! thread), the proposal drawn over the views and listed - how the layer was turned, scaled and fitted, L1's and the
//! site rules' findings on it, each of DICE's statistics against DICE's range - and Accept (one move operation of the
//! log, undone in one step) or Discard.

#[allow(
    clippy::wildcard_imports,
    reason = "a part of app.rs in a file of its own: its types and helpers"
)]
use super::*;

use bc2_levelwrite::rules::Severity as RuleSeverity;

impl Tabs<'_> {
    pub(super) fn auto_layout(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette;
        if self.view.autolayout.poll() {
            // the proposal drawn over the views
            self.view.nav_refresh(self.gpu, &palette);
        }
        if self.view.autolayout.busy() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
        let p = self.palette;
        let m = self.metrics;
        let current = self
            .view
            .doc
            .as_ref()
            .and_then(|d| self.view.autolayout.current(d))
            .cloned();
        let summary = current.as_ref().map_or_else(String::new, |x| {
            let (w, n) = x.counts();
            format!("{w} warnings · {n} notes")
        });
        self.header(ui, "Auto layout", &summary);
        let why = crate::autolayout::why_not(self.view.doc.as_ref());
        let mut propose = false;
        let mut accept = false;
        let mut discard = false;
        egui::Frame::new()
            .inner_margin(Margin::same(m.space_3 as i8))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(
                        "Lays the target mode's layer out on this level's ground in DICE's style: the layer keeps \
                         its shape (DICE's own layout) and is turned, scaled and placed where the level's ground, \
                         water, rocks and buildings suit it, then held against DICE's statistics. Nothing changes \
                         until you accept it; an undo takes it back.",
                    )
                    .color(p.ink_muted),
                );
                ui.add_space(m.space_2);
                ui.horizontal(|ui| {
                    ui.label("Seed");
                    ui.add(egui::DragValue::new(&mut self.view.autolayout.seed).range(0..=999_999));
                    let busy = self.view.autolayout.busy();
                    let resp = ui
                        .add_enabled(why.is_none() && !busy, egui::Button::new("Propose layout"))
                        .on_hover_text(
                            "Searches the level for the best place, turn and scale of the layer (a few seconds)",
                        );
                    let resp = match why {
                        Some(w) => resp.on_disabled_hover_text(w),
                        None => resp.on_disabled_hover_text("A proposal is being made"),
                    };
                    if resp.clicked() {
                        propose = true;
                    }
                });
                if let Some(w) = why {
                    ui.label(RichText::new(w).color(p.ink_muted));
                }
                if !self.view.autolayout.message.is_empty() {
                    ui.label(RichText::new(&self.view.autolayout.message).color(p.ink));
                }
                let stale =
                    self.view.autolayout.proposal.is_some() && current.is_none() && !self.view.autolayout.busy();
                if stale {
                    status_line(
                        ui,
                        &p,
                        "Information",
                        p.status_info,
                        "The project changed since the proposal was made: propose again.",
                    );
                }
                let Some(prop) = current else { return };
                ui.add_space(m.space_2);
                ui.horizontal(|ui| {
                    if ui
                        .add(egui::Button::new(RichText::new("Accept").color(p.accent)))
                        .on_hover_text(
                            "One move operation of the project's log: edit any object afterwards, or undo it",
                        )
                        .clicked()
                    {
                        accept = true;
                    }
                    if ui.button("Discard").clicked() {
                        discard = true;
                    }
                });
                ui.label(
                    RichText::new(format!(
                        "Turned {:.0} deg, scale {:.2}, centre ({:.0}, {:.0}); {} objects moved; {} flags, bases or \
                         M-COMs and {} spawn points fitted to the ground; {} candidates",
                        prop.turn,
                        prop.scale,
                        prop.centre[0],
                        prop.centre[1],
                        prop.moves.len(),
                        prop.clusters_fitted,
                        prop.points_fitted,
                        prop.candidates
                    ))
                    .color(p.ink_muted),
                );
                egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
                    let (a, b) = prop.inside();
                    ui.label(
                        RichText::new(format!("DICE's statistics: {a} of {b} values inside DICE's ranges"))
                            .text_style(style("body-strong"))
                            .color(p.ink),
                    );
                    // a statistic per row, wrapped to the panel: its name and verdict, its values, DICE's range
                    for s in &prop.stats {
                        let ok = s.inside == s.values.len();
                        let (word, colour) = if ok {
                            ("inside", p.status_ok)
                        } else {
                            ("outside", p.status_warn)
                        };
                        let shown: Vec<String> = s.values.iter().take(6).map(|v| format!("{v}")).collect();
                        let more = if s.values.len() > 6 { " ..." } else { "" };
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(&s.param).monospace().color(p.ink));
                            ui.label(RichText::new(word).color(colour));
                            ui.label(RichText::new(format!("{}{more}", shown.join(", "))).color(p.ink));
                            ui.label(
                                RichText::new(format!("DICE {} to {}, median {}", s.dice[0], s.dice[1], s.dice[2]))
                                    .color(p.ink_muted),
                            );
                        });
                    }
                    ui.add_space(m.space_2);
                    if prop.findings.is_empty() {
                        ui.label(RichText::new("No finding: every placement and site rule holds.").color(p.status_ok));
                    }
                    for f in &prop.findings {
                        let (word, colour) = match f.severity {
                            RuleSeverity::Error => ("Error", p.status_error),
                            RuleSeverity::Warning => ("Warning", p.status_warn),
                            RuleSeverity::Note => ("Note", p.status_info),
                        };
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(word).color(colour));
                            ui.label(RichText::new(&f.code).monospace().color(p.ink_muted));
                            ui.label(RichText::new(&f.message).color(p.ink));
                        });
                    }
                });
            });
        if propose && let Some(d) = self.view.doc.as_ref() {
            self.view.live.refresh(d);
            if let Some(site) = self.view.live.site() {
                let routes = self.view.live.routes();
                self.view.autolayout.start(d, site, routes);
            }
        }
        if accept {
            self.view.accept_layout(self.gpu, &palette);
        }
        if discard {
            self.view.autolayout.clear();
            self.view.autolayout.message = "Proposal discarded".into();
            self.view.nav_refresh(self.gpu, &palette);
        }
    }
}

impl ViewState {
    /// Commits the current proposal as one move operation (the History names it with the proposal's words).
    pub fn accept_layout(&mut self, gpu: &GpuCtx, palette: &Palette) {
        let Some(d) = self.doc.as_ref() else { return };
        let Some(prop) = self.autolayout.current(d).cloned() else {
            return;
        };
        let label = prop.describe();
        let items = prop.moves;
        if items.is_empty() {
            self.autolayout.message = "The proposal moves nothing".into();
            return;
        }
        // the proposal's drawing goes with it
        self.autolayout.clear();
        self.commit(gpu, palette, Op::Move { items }, &label);
        self.nav_refresh(gpu, palette);
        self.autolayout.message = format!("Accepted: {label}");
    }
}
