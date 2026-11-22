//! Round A1's panel: the Gameplay workspace's auto layout ([`crate::autolayout`]). A seed, Propose (on a worker
//! thread), the proposal drawn over the views and listed - how the layer was turned, scaled and fitted, L1's and the
//! site rules' findings on it, each of DICE's statistics against DICE's range - and Accept (one operation of the log,
//! undone in one step) or Discard.
//!
//! Round A2: several proposals side by side (a column each: its turn and scale, warnings and notes, DICE's statistics
//! inside their ranges, its vehicles), one shown in the views at a time; the selection pinned (or unpinned) so the
//! next proposals keep it where it stands; each proposal's vehicles listed with what could not be placed.

#[allow(
    clippy::wildcard_imports,
    reason = "a part of app.rs in a file of its own: its types and helpers"
)]
use super::*;

use bc2_levelwrite::autolayout::Proposal;
use bc2_levelwrite::rules::Severity as RuleSeverity;

/// What the panel's buttons asked for this frame.
#[derive(Default)]
struct Asked {
    propose: bool,
    accept: Option<usize>,
    show: Option<usize>,
    discard: bool,
    pin: Option<bool>,
    unpin_all: bool,
}

impl Tabs<'_> {
    #[allow(clippy::too_many_lines, reason = "the panel top to bottom")]
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
        let all: Vec<Proposal> = self
            .view
            .doc
            .as_ref()
            .map_or_default(|d| self.view.autolayout.all(d).to_vec());
        let shown = self.view.autolayout.shown.min(all.len().saturating_sub(1));
        let summary = all.get(shown).map_or_else(String::new, |x| {
            let (w, n) = x.counts();
            format!("{w} warnings · {n} notes")
        });
        self.header(ui, "Auto layout", &summary);
        let why = crate::autolayout::why_not(self.view.doc.as_ref());
        let pinned = self.view.doc.as_ref().map_or(0, |d| d.org.pinned.len());
        let selected = self.view.selection.len();
        let mut asked = Asked::default();
        egui::Frame::new()
            .inner_margin(Margin::same(m.space_3 as i8))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(
                        "Lays the target mode's layer out on this level's ground in DICE's style: the layer keeps \
                         its shape (DICE's own layout) and is turned, scaled and placed where the level's ground, \
                         water, rocks and buildings suit it, its vehicles placed by class, then held against DICE's \
                         statistics. Pinned objects stay where they stand. Nothing changes until you accept a \
                         proposal; an undo takes it back.",
                    )
                    .color(p.ink_muted),
                );
                ui.add_space(m.space_2);
                ui.horizontal(|ui| {
                    ui.label("Seed");
                    ui.add(egui::DragValue::new(&mut self.view.autolayout.seed).range(0..=999_999));
                    ui.label("Proposals");
                    if self.view.autolayout.count == 0 {
                        self.view.autolayout.count = 3;
                    }
                    ui.add(egui::DragValue::new(&mut self.view.autolayout.count).range(1..=4));
                    let busy = self.view.autolayout.busy();
                    let resp = ui
                        .add_enabled(why.is_none() && !busy, egui::Button::new("Propose layouts"))
                        .on_hover_text(
                            "Searches the level for the best places, turns and scales of the layer and places its \
                             vehicles (a few seconds)",
                        );
                    let resp = match why {
                        Some(w) => resp.on_disabled_hover_text(w),
                        None => resp.on_disabled_hover_text("A proposal is being made"),
                    };
                    if resp.clicked() {
                        asked.propose = true;
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("Pinned: {pinned}")).color(p.ink));
                    if ui
                        .add_enabled(selected > 0 && why.is_none(), egui::Button::new("Pin selection"))
                        .on_hover_text(
                            "The next proposals keep the selected objects where they stand and lay the rest out \
                             around them (a pinned flag, base or M-COM keeps its spawns with it)",
                        )
                        .clicked()
                    {
                        asked.pin = Some(true);
                    }
                    if ui
                        .add_enabled(selected > 0 && pinned > 0, egui::Button::new("Unpin selection"))
                        .clicked()
                    {
                        asked.pin = Some(false);
                    }
                    if ui.add_enabled(pinned > 0, egui::Button::new("Unpin all")).clicked() {
                        asked.unpin_all = true;
                    }
                });
                if let Some(w) = why {
                    ui.label(RichText::new(w).color(p.ink_muted));
                }
                if !self.view.autolayout.message.is_empty() {
                    ui.label(RichText::new(&self.view.autolayout.message).color(p.ink));
                }
                let stale = self.view.autolayout.holds() && all.is_empty() && !self.view.autolayout.busy();
                if stale {
                    status_line(
                        ui,
                        &p,
                        "Information",
                        p.status_info,
                        "The project changed since the proposals were made: propose again.",
                    );
                }
                if all.is_empty() {
                    return;
                }
                ui.add_space(m.space_2);
                // the proposals side by side, a column each
                ui.columns(all.len(), |cols| {
                    for (k, (col, prop)) in cols.iter_mut().zip(&all).enumerate() {
                        proposal_column(col, &p, m, k, prop, k == shown, &mut asked);
                    }
                });
                ui.add_space(m.space_2);
                if ui.button("Discard all").clicked() {
                    asked.discard = true;
                }
                let Some(prop) = all.get(shown) else { return };
                egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("Proposal {}: DICE's statistics", shown + 1))
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
                        let shown_values: Vec<String> = s.values.iter().take(6).map(|v| format!("{v}")).collect();
                        let more = if s.values.len() > 6 { " ..." } else { "" };
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(&s.param).monospace().color(p.ink));
                            ui.label(RichText::new(word).color(colour));
                            ui.label(RichText::new(format!("{}{more}", shown_values.join(", "))).color(p.ink));
                            ui.label(
                                RichText::new(format!("DICE {} to {}, median {}", s.dice[0], s.dice[1], s.dice[2]))
                                    .color(p.ink_muted),
                            );
                        });
                    }
                    ui.add_space(m.space_2);
                    let v = &prop.vehicles;
                    if !v.vehicles.is_empty() || !v.notes.is_empty() {
                        ui.label(
                            RichText::new(format!(
                                "Vehicles: {} ({} of the level's own moved, {} new copies)",
                                v.vehicles.len(),
                                v.moves.len(),
                                v.places.len()
                            ))
                            .text_style(style("body-strong"))
                            .color(p.ink),
                        );
                        let mut by_class: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
                        for x in &v.vehicles {
                            *by_class.entry(x.class.as_str()).or_default() += 1;
                        }
                        let words: Vec<String> = by_class.iter().map(|(c, n)| format!("{n} {c}")).collect();
                        ui.label(RichText::new(words.join(", ")).color(p.ink_muted));
                        for note in &v.notes {
                            ui.label(RichText::new(note).color(p.status_info));
                        }
                    }
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
        if asked.propose
            && let Some(d) = self.view.doc.as_ref()
        {
            self.view.live.refresh(d);
            if let Some(site) = self.view.live.site() {
                let routes = self.view.live.routes();
                self.view.autolayout.start(d, site, routes);
            }
        }
        if let Some(k) = asked.show {
            self.view.autolayout.shown = k;
            self.view.nav_refresh(self.gpu, &palette);
        }
        if let Some(pin) = asked.pin {
            self.view.pin_selection(self.gpu, &palette, pin);
        }
        if asked.unpin_all {
            self.view.unpin_all(self.gpu, &palette);
        }
        if let Some(k) = asked.accept {
            self.view.autolayout.shown = k;
            self.view.accept_layout(self.gpu, &palette);
        }
        if asked.discard {
            self.view.autolayout.clear();
            self.view.autolayout.message = "Proposals discarded".into();
            self.view.nav_refresh(self.gpu, &palette);
        }
    }
}

/// One proposal's column: its number (the best first), Show and Accept, how it was laid out and what it measures.
fn proposal_column(
    ui: &mut egui::Ui,
    p: &Palette,
    m: Metrics,
    k: usize,
    prop: &Proposal,
    shown: bool,
    asked: &mut Asked,
) {
    let (w, n) = prop.counts();
    let (a, b) = prop.inside();
    let frame = egui::Frame::new()
        .inner_margin(Margin::same(m.space_2 as i8))
        .stroke(Stroke::new(
            if shown { 2.0 } else { 1.0 },
            if shown { p.accent } else { p.line },
        ))
        .corner_radius(4.0);
    let width = (ui.available_width() - 2.0 * m.space_2).max(40.0);
    frame.show(ui, |ui| {
        ui.set_width(width);
        // each line wraps to the column (three side by side in a narrow dock)
        let line = |ui: &mut egui::Ui, t: RichText| {
            ui.add(egui::Label::new(t).wrap());
        };
        line(
            ui,
            RichText::new(if k == 0 {
                "1 (best)".to_owned()
            } else {
                format!("{}", k + 1)
            })
            .text_style(style("body-strong"))
            .color(if shown { p.accent } else { p.ink }),
        );
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!shown, egui::Button::new("Show"))
                .on_hover_text("Draw this proposal in the views")
                .clicked()
            {
                asked.show = Some(k);
            }
            if ui
                .add(egui::Button::new(RichText::new("Accept").color(p.accent)))
                .on_hover_text("One operation of the project's log: edit any object afterwards, or undo it")
                .clicked()
            {
                asked.accept = Some(k);
            }
        });
        let wc = if w == 0 { p.status_ok } else { p.status_warn };
        line(ui, RichText::new(format!("{w} warnings, {n} notes")).color(wc));
        line(
            ui,
            RichText::new(format!("{a}/{b} inside DICE's ranges")).color(if a == b {
                p.status_ok
            } else {
                p.status_warn
            }),
        );
        line(
            ui,
            RichText::new(format!("{:.0} deg, scale {:.2}", prop.turn, prop.scale)).color(p.ink_muted),
        );
        let v = &prop.vehicles;
        line(
            ui,
            RichText::new(format!(
                "{} vehicles, {} new{}",
                v.vehicles.len(),
                v.places.len(),
                if prop.pinned > 0 {
                    format!("; {} pinned", prop.pinned)
                } else {
                    String::new()
                }
            ))
            .color(p.ink_muted),
        );
    });
}

impl ViewState {
    /// Commits the proposal shown as one operation (the History names it with the proposal's words): its moves, and
    /// with vehicles the copies placed and their links (one multi-part operation of the partition).
    pub fn accept_layout(&mut self, gpu: &GpuCtx, palette: &Palette) {
        let Some(d) = self.doc.as_ref() else { return };
        let Some(prop) = self.autolayout.current(d).cloned() else {
            return;
        };
        let label = prop.describe();
        if prop.moves.is_empty() && prop.vehicles.vehicles.is_empty() {
            self.autolayout.message = "The proposal moves nothing".into();
            return;
        }
        // the proposals' drawing goes with them
        self.autolayout.clear();
        self.commit(gpu, palette, prop.op(), &label);
        self.nav_refresh(gpu, palette);
        self.autolayout.message = format!("Accepted: {label}");
    }

    /// Pins (or unpins) the selected objects for the auto layout: one operation of the log (the project's
    /// organisation; the partition is not changed).
    pub fn pin_selection(&mut self, gpu: &GpuCtx, palette: &Palette, pinned: bool) {
        let Some(d) = self.doc.as_ref() else { return };
        let guids: Vec<String> = self
            .selection
            .iter()
            .filter_map(|&k| d.guid(k).map(str::to_owned))
            .filter(|g| d.org.pinned.contains(&g.to_ascii_uppercase()) != pinned)
            .collect();
        if guids.is_empty() {
            return;
        }
        let n = guids.len();
        self.commit(gpu, palette, Op::Pin { guids, pinned }, "");
        self.autolayout.message = format!(
            "{} {n} object{} for the auto layout",
            if pinned { "Pinned" } else { "Unpinned" },
            if n == 1 { "" } else { "s" }
        );
    }

    /// Unpins every pinned object.
    pub fn unpin_all(&mut self, gpu: &GpuCtx, palette: &Palette) {
        let Some(d) = self.doc.as_ref() else { return };
        let guids: Vec<String> = d.org.pinned.iter().cloned().collect();
        if guids.is_empty() {
            return;
        }
        self.commit(gpu, palette, Op::Pin { guids, pinned: false }, "");
        self.autolayout.message = "Unpinned every object".into();
    }
}
