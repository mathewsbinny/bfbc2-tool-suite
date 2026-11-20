//! The Map Inspector's views in the editor (round GR1): the **Links** panel (what the selection references, what
//! references it, and the event connections it sends and receives; a row goes to that object, Back returns), the
//! **All fields** section of Properties (the selection's whole property tree, nested, with each field's type and
//! value as DbxEdit's XML writes it; references lead to their objects), and the links drawn in the 3D views
//! (`view-link`: references solid, event connections dashed, the arrow at the object referenced or the receiver;
//! K shows or hides them). The data is [`crate::inspect`]'s, read from the level's partitions (a project's edited
//! partition when one is open).

#[allow(
    clippy::wildcard_imports,
    reason = "a part of app.rs in a file of its own: its types and helpers"
)]
use super::*;

use std::collections::BTreeSet;

use crate::inspect::{Inspection, Links, PropNode};

/// The most rows a list or a section shows at first ("Show more" adds as many again).
const PAGE: usize = 400;

/// The longest value shown on its row (the whole value is in its tooltip).
const LONG: usize = 140;

impl ViewState {
    /// The revision the property tree and the links are read at: the project's, 0 for a level only viewed.
    fn inspection_rev(&self) -> u64 {
        self.doc.as_ref().map_or(0, |d| d.revision + 1)
    }

    /// Reads the links again when the project changed since they were read.
    pub(super) fn refresh_inspection(&mut self) {
        let rev = self.inspection_rev();
        if self.inspection.as_ref().is_some_and(|(r, _)| *r == rev) {
            return;
        }
        let ins = match &self.doc {
            Some(d) => Inspection::new(&d.scene, d.partition(), &d.deleted),
            None => Inspection::new(&self.base, None, &BTreeSet::new()),
        };
        self.inspection = Some((rev, ins));
    }

    /// The links (read when they are first asked for, and again after an edit).
    pub fn links(&mut self) -> &Links {
        static EMPTY: std::sync::OnceLock<Links> = std::sync::OnceLock::new();
        self.refresh_inspection();
        self.inspection
            .as_ref()
            .map_or_else(|| EMPTY.get_or_init(Links::default), |(_, i)| &i.links)
    }

    /// Entity `k`'s property tree (read once per selection and project revision).
    ///
    /// # Errors
    /// Its partition cannot be read, or its element is not in it ([`Inspection::with_element`]).
    pub fn property_tree(&mut self, k: usize) -> Result<&[PropNode], String> {
        self.refresh_inspection();
        let rev = self.inspection_rev();
        if !self.fields_tree.as_ref().is_some_and(|(e, r, _)| *e == k && *r == rev) {
            let tree = match &self.inspection {
                Some((_, ins)) => ins.tree(self.scene(), k),
                None => Err("not read".to_owned()),
            };
            self.fields_tree = Some((k, rev, tree));
        }
        match &self.fields_tree {
            Some((_, _, Ok(t))) => Ok(t.as_slice()),
            Some((_, _, Err(e))) => Err(e.clone()),
            None => Err("not read".to_owned()),
        }
    }

    /// Goes to a linked object: selects it (the outliner shows it), the views look at it, and Back returns here.
    pub fn follow(&mut self, gpu: &GpuCtx, target: usize) {
        if let Some(cur) = self.selected
            && cur != target
        {
            self.link_back.push(cur);
            if self.link_back.len() > 100 {
                self.link_back.remove(0);
            }
        }
        self.select(gpu, Some(target), true);
        self.focus_selected();
    }

    /// Back to the object the last link was followed from.
    pub fn go_back(&mut self, gpu: &GpuCtx) {
        if let Some(k) = self.link_back.pop() {
            self.select(gpu, Some(k), true);
            self.focus_selected();
        }
    }
}

/// An object as a link names it: its name and its type.
fn object_text(scene: &Scene, k: usize) -> (String, String) {
    scene.entities.get(k).map_or_else(
        || (format!("#{k}"), String::new()),
        |e| (model::display_name(e), e.short_type.clone()),
    )
}

/// A value cut to [`LONG`] characters (with an ellipsis).
fn shortened(v: &str) -> String {
    if v.chars().count() > LONG {
        let mut s: String = v.chars().take(LONG).collect();
        s.push('\u{2026}');
        s
    } else {
        v.to_owned()
    }
}

/// A cell `w` wide on a row: `add` draws in it, left to right, cut at its width.
fn cell<R>(ui: &mut egui::Ui, w: f32, h: f32, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.allocate_ui_with_layout(Vec2::new(w, h), Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_width(w);
        ui.set_max_width(w);
        add(ui)
    })
    .inner
}

/// How many of a list's rows to show (a "Show more" button pages on), kept per list.
fn shown(ui: &egui::Ui, id: egui::Id, len: usize) -> usize {
    ui.ctx().data(|d| d.get_temp::<usize>(id)).unwrap_or(PAGE).min(len)
}

/// "Show more" under a cut list.
fn more_button(ui: &mut egui::Ui, p: &Palette, id: egui::Id, shown: usize, len: usize) {
    if shown < len {
        let more = (len - shown).min(PAGE);
        if ui
            .button(RichText::new(format!("Show {more} more ({} not shown)", len - shown)).color(p.ink))
            .clicked()
        {
            ui.ctx().data_mut(|d| d.insert_temp(id, shown + more));
        }
    }
}

/// What the tree's rows read from the view.
struct TreeCtx<'a> {
    p: &'a Palette,
    m: &'a Metrics,
    scene: &'a Scene,
    links: &'a Links,
    shown: &'a dyn Fn(usize) -> bool,
}

/// What the tree's rows give back: the object a reference row was clicked for, and where each container's header
/// is (its field path by name, its rectangle: the tests open them).
#[derive(Default)]
struct TreeOut {
    follow: Option<usize>,
    headers: Vec<(String, Rect)>,
}

/// The rows of a property tree: a container opens to its fields, a value shows its text and type, a reference
/// leads to its object. `path` keeps the rows' ids apart, `names` is the field path by name.
fn prop_rows(ui: &mut egui::Ui, c: &TreeCtx<'_>, nodes: &[PropNode], path: &str, names: &str, out: &mut TreeOut) {
    let list_id = ui.make_persistent_id(("fields page", path));
    let n = shown(ui, list_id, nodes.len());
    for (i, node) in nodes.iter().take(n).enumerate() {
        let here = format!("{path}/{i}");
        match &node.children {
            Some(kids) => {
                let mut job = egui::text::LayoutJob::default();
                job.append(
                    &node.name,
                    0.0,
                    egui::TextFormat::simple(FontId::proportional(13.0), c.p.ink),
                );
                job.append(
                    &format!("   {}", node.ty),
                    0.0,
                    egui::TextFormat::simple(FontId::proportional(11.0), c.p.ink_muted),
                );
                let field_path = if names.is_empty() {
                    node.name.clone()
                } else {
                    format!("{names}/{}", node.name)
                };
                let r = egui::CollapsingHeader::new(job)
                    .id_salt(("field", &here))
                    .default_open(false)
                    .show(ui, |ui| prop_rows(ui, c, kids, &here, &field_path, out));
                out.headers.push((field_path, r.header_response.rect));
            }
            None => value_row(ui, c, node, &mut out.follow),
        }
    }
    more_button(ui, c.p, list_id, n, nodes.len());
}

/// One value's row: its name, its value (a reference: the object it leads to), its type.
fn value_row(ui: &mut egui::Ui, c: &TreeCtx<'_>, node: &PropNode, follow: &mut Option<usize>) {
    let (p, m) = (c.p, c.m);
    ui.horizontal(|ui| {
        let total = ui.available_width();
        let gap = ui.spacing().item_spacing.x;
        let name_w = (total * 0.38).clamp(60.0, 240.0);
        let ty_w = (total * 0.2).clamp(48.0, 96.0);
        let val_w = (total - name_w - ty_w - 2.0 * gap).max(24.0);
        cell(ui, name_w, m.row, |ui| {
            ui.add(egui::Label::new(RichText::new(&node.name).color(p.ink_muted)).truncate())
                .on_hover_text(&node.name);
        });
        cell(ui, val_w, m.row, |ui| {
            if let Some(r) = &node.reference {
                if let Some(t) = c.links.resolve(r).filter(|&t| (c.shown)(t)) {
                    let (name, ty) = object_text(c.scene, t);
                    let resp = ui
                        .add(
                            egui::Button::new(RichText::new(format!("\u{2192} {name}")).color(p.accent_text))
                                .frame(false)
                                .truncate(),
                        )
                        .on_hover_text(format!("Go to {name} ({ty}, {r})"));
                    if resp.clicked() {
                        *follow = Some(t);
                    }
                } else {
                    let why = if r == "null" {
                        "No object (null)".to_owned()
                    } else if r.contains('/') {
                        format!("In another partition: {r}")
                    } else {
                        format!("Not an object of this level: {r}")
                    };
                    ui.add(egui::Label::new(RichText::new(r).text_style(style("value")).color(p.ink_muted)).truncate())
                        .on_hover_text(why);
                }
            } else {
                let text = if node.value.is_empty() { "(empty)" } else { &node.value };
                ui.add(
                    egui::Label::new(
                        RichText::new(shortened(text))
                            .text_style(style("value"))
                            .color(if node.value.is_empty() { p.ink_muted } else { p.ink }),
                    )
                    .truncate(),
                )
                .on_hover_text(format!("{} ({}): {}", node.name, node.ty, node.value));
            }
        });
        cell(ui, ty_w, m.row, |ui| {
            ui.add(egui::Label::new(RichText::new(&node.ty).text_style(style("small")).color(p.ink_muted)).truncate());
        });
    });
}

/// A link of the Links panel: its field or events, and the object it leads to (or the reference, when the level
/// has no such object).
struct LinkRow {
    left: String,
    target: Option<usize>,
    reference: String,
}

impl Tabs<'_> {
    /// Properties' "All fields": the selection's property tree.
    pub(super) fn all_fields(&mut self, ui: &mut egui::Ui, k: usize) {
        let p = self.palette;
        let m = self.metrics;
        // read (or kept) once per selection and revision; drawn from the cache below
        let _ = self.view.property_tree(k);
        let v = &*self.view;
        let tree: Result<&[PropNode], &str> = match &v.fields_tree {
            Some((_, _, Ok(t))) => Ok(t.as_slice()),
            Some((_, _, Err(e))) => Err(e.as_str()),
            None => Err("not read"),
        };
        ui.add_space(m.space_4);
        let id = ui.make_persistent_id("all fields");
        let state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true);
        let count = match &tree {
            Ok(t) => format!(
                "{} fields",
                group_digits(t.iter().map(|n| 1 + n.count()).sum::<usize>() as u64)
            ),
            Err(_) => String::new(),
        };
        let mut out = TreeOut::default();
        state
            .show_header(ui, |ui| {
                ui.label(caption("All fields", p.ink_muted));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(count).text_style(style("small")).color(p.ink_muted));
                });
            })
            .body(|ui| match &tree {
                Ok([]) => {
                    ui.label(RichText::new("This object has no fields.").color(p.ink_muted));
                }
                Ok(t) => {
                    let shown = |t: usize| v.doc.as_ref().is_none_or(|d| !d.deleted.contains(&t));
                    let links = v.inspection.as_ref().map(|(_, i)| &i.links);
                    let empty = Links::default();
                    let c = TreeCtx {
                        p: &p,
                        m: &m,
                        scene: v.scene(),
                        links: links.unwrap_or(&empty),
                        shown: &shown,
                    };
                    prop_rows(ui, &c, t, &format!("{k}"), "", &mut out);
                }
                Err(e) => {
                    ui.label(RichText::new(format!("The fields could not be read: {e}")).color(p.ink_muted));
                }
            });
        self.view.field_rects = out.headers;
        if let Some(t) = out.follow {
            self.view.follow(self.gpu, t);
        }
    }

    /// The Links panel.
    pub(super) fn links(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let m = self.metrics;
        self.view.link_rects.clear();
        let Some(k) = self
            .view
            .selected
            .filter(|&k| self.view.doc.as_ref().is_none_or(|d| !d.deleted.contains(&k)))
        else {
            self.header(ui, "Links", "");
            egui::Frame::new()
                .inner_margin(Margin::same(m.space_3 as i8))
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(
                                "Select an object to see what it references, what references it and the events it \
                                 sends and receives. Click a link to go there; Back returns.",
                            )
                            .color(p.ink_muted),
                        )
                        .wrap(),
                    );
                });
            return;
        };
        self.view.refresh_inspection();
        let v = &*self.view;
        let empty = Links::default();
        let links = v.inspection.as_ref().map_or(&empty, |(_, i)| &i.links);
        let scene = v.scene();
        let sections: [(&str, &str, Vec<LinkRow>); 4] = [
            (
                "References",
                "Its fields that name another object",
                links
                    .refs(k)
                    .iter()
                    .map(|r| LinkRow {
                        left: r.path.clone(),
                        target: r.target.filter(|&t| t != k),
                        reference: r.reference.clone(),
                    })
                    .collect(),
            ),
            (
                "Referenced by",
                "Objects whose fields name it (the level's object list left out)",
                links
                    .referrers(k)
                    .iter()
                    .map(|(f, t)| LinkRow {
                        left: f.clone(),
                        target: Some(*t),
                        reference: String::new(),
                    })
                    .collect(),
            ),
            (
                "Events sent",
                "Event connections it is the source of: its event, then what the target does",
                links
                    .sends(k)
                    .map(|e| LinkRow {
                        left: format!("{} \u{2192} {}", e.source_event, e.target_event),
                        target: e.target,
                        reference: e.target_ref.clone(),
                    })
                    .collect(),
            ),
            (
                "Events received",
                "Event connections it is the target of: the source's event, then what it does",
                links
                    .receives(k)
                    .map(|e| LinkRow {
                        left: format!("{} \u{2192} {}", e.source_event, e.target_event),
                        target: e.source,
                        reference: e.source_ref.clone(),
                    })
                    .collect(),
            ),
        ];
        let total: usize = sections.iter().map(|s| s.2.len()).sum();
        let (name, ty) = object_text(scene, k);
        self.header(ui, "Links", &format!("{} links", group_digits(total as u64)));
        let mut go: Option<usize> = None;
        let mut back = false;
        let mut rects: Vec<(usize, Rect)> = Vec::new();
        let can_back = !self.view.link_back.is_empty();
        let v = &*self.view;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(Margin::same(m.space_2 as i8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(can_back, egui::Button::new("\u{2190} Back"))
                            .on_hover_text("Back to the object you came from")
                            .on_disabled_hover_text("No link followed yet")
                            .clicked()
                        {
                            back = true;
                        }
                        ui.label(RichText::new(&name).text_style(style("body-strong")).color(p.ink));
                        ui.label(RichText::new(&ty).text_style(style("small")).color(p.ink_muted));
                    });
                    for (title, help, rows) in &sections {
                        ui.add_space(m.space_2);
                        let id = ui.make_persistent_id(("links section", *title));
                        let state =
                            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true);
                        state
                            .show_header(ui, |ui| {
                                ui.label(caption(title, p.ink_muted)).on_hover_text(*help);
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(
                                        RichText::new(group_digits(rows.len() as u64))
                                            .text_style(style("value"))
                                            .color(p.ink_muted),
                                    );
                                });
                            })
                            .body(|ui| {
                                if rows.is_empty() {
                                    ui.label(RichText::new("None").color(p.ink_muted));
                                }
                                let page = ui.make_persistent_id(("links page", *title));
                                let n = shown(ui, page, rows.len());
                                for r in rows.iter().take(n) {
                                    if let Some((t, r, clicked)) = link_row(ui, &p, &m, scene, r, v) {
                                        rects.push((t, r));
                                        if clicked {
                                            go = Some(t);
                                        }
                                    }
                                }
                                more_button(ui, &p, page, n, rows.len());
                            });
                    }
                });
        });
        self.view.link_rects = rects;
        if back {
            self.view.go_back(self.gpu);
        } else if let Some(t) = go {
            self.view.follow(self.gpu, t);
        }
    }

    /// The selection's links in a view: references solid, event connections dashed, each with an arrow at the
    /// object referenced or the receiver.
    pub(super) fn links_overlay(&mut self, ui: &egui::Ui, rect: Rect, cam: &Camera) {
        let Some(k) = self.view.selected else { return };
        self.view.refresh_inspection();
        let v = &*self.view;
        let Some((_, ins)) = &v.inspection else { return };
        let l = &ins.links;
        let shown = |t: usize| v.doc.as_ref().is_none_or(|d| d.shown(t));
        if !shown(k) {
            return;
        }
        let view = cam.view([f64::from(rect.width()), f64::from(rect.height())]);
        let at = |e: usize| -> Option<Pos2> {
            let p = v.scene().entities.get(e)?.position()?;
            let s = view.project([p[0], p[1] + v.marker_offset(e), p[2]])?;
            Some(rect.min + Vec2::new(s[0] as f32, s[1] as f32))
        };
        // (from, to, dashed), each pair once
        let mut seen: BTreeSet<(usize, usize, bool)> = BTreeSet::new();
        let mut add = |a: Option<usize>, b: Option<usize>, dashed: bool| {
            if let (Some(a), Some(b)) = (a, b)
                && a != b
                && shown(a)
                && shown(b)
                && seen.len() < 400
            {
                seen.insert((a, b, dashed));
            }
        };
        for r in l.refs(k) {
            add(Some(k), r.target, false);
        }
        for (_, t) in l.referrers(k) {
            add(Some(*t), Some(k), false);
        }
        for e in l.sends(k) {
            add(Some(k), e.target, true);
        }
        for e in l.receives(k) {
            add(e.source, Some(k), true);
        }
        let painter = ui.painter().with_clip_rect(rect);
        let colour = self.palette.view_link;
        let stroke = Stroke::new(2.0, colour);
        for (a, b, dashed) in seen {
            let (Some(pa), Some(pb)) = (at(a), at(b)) else { continue };
            let d = pb - pa;
            let len = d.length();
            if len < 4.0 {
                continue;
            }
            let dir = d / len;
            // the arrow stops short of the marker's middle
            let tip = pb - dir * 6.0_f32.min(len / 2.0);
            if dashed {
                painter.extend(egui::Shape::dashed_line(&[pa, tip], stroke, 7.0, 5.0));
            } else {
                painter.line_segment([pa, tip], stroke);
            }
            let side = Vec2::new(-dir.y, dir.x) * 5.0;
            let base = tip - dir * 11.0;
            painter.add(egui::Shape::convex_polygon(
                vec![tip, base + side, base - side],
                colour,
                Stroke::NONE,
            ));
        }
    }
}

impl Tabs<'_> {
    /// The measured path in a view: its points and segments in the selection colour, and the figures beside the
    /// last point (the Map Inspector's Measure panel).
    pub(super) fn measure_overlay(&self, ui: &egui::Ui, rect: Rect, cam: &Camera) {
        let v = &*self.view;
        if !v.measuring && v.measure.is_empty() {
            return;
        }
        let p = &self.palette;
        let view = cam.view([f64::from(rect.width()), f64::from(rect.height())]);
        let to_screen = |w: V3| view.project(w).map(|s| rect.min + Vec2::new(s[0] as f32, s[1] as f32));
        let painter = ui.painter().with_clip_rect(rect);
        let stroke = Stroke::new(2.0, p.view_selection);
        for w in v.measure.windows(2) {
            if let [a, b] = w
                && let (Some(a), Some(b)) = (to_screen(*a), to_screen(*b))
            {
                painter.line_segment([a, b], stroke);
            }
        }
        for w in &v.measure {
            if let Some(q) = to_screen(*w) {
                painter.circle_filled(q, 4.0, p.view_selection);
            }
        }
        let Some(&last) = v.measure.last() else { return };
        let mut lines = vec![format!(
            "{} points; last at {:.2}, {:.2}, {:.2}",
            v.measure.len(),
            last[0],
            last[1],
            last[2]
        )];
        let scene = v.scene();
        if let Some(h) = scene.heightfield.as_ref() {
            lines.push(format!(
                "above the terrain: {:+.2} m",
                last[1] - h.height(last[0], last[2])
            ));
        }
        if let Some(d) = scene
            .water
            .as_ref()
            .and_then(|w| crate::inspect::water_above(&w.mesh, last))
        {
            lines.push(format!("water surface above it: {d:+.2} m"));
        }
        if let Some(f) = crate::inspect::measure(&v.measure) {
            lines.push(format!("last segment: {:.2} m", f.segment));
            lines.push(format!(
                "  horizontal {:.2} m, height {:+.2} m, slope {:+.1} deg",
                f.horizontal, f.height, f.slope
            ));
            lines.push(format!("path total: {:.2} m", f.total));
        }
        let Some(at) = to_screen(last) else { return };
        let g = painter.layout_no_wrap(lines.join("\n"), FontId::monospace(12.0), p.ink);
        let r = Rect::from_min_size(at + Vec2::new(12.0, 8.0), g.size() + Vec2::splat(8.0));
        painter.rect_filled(r, CornerRadius::same(self.metrics.radius_sm as u8), p.scene_label);
        painter.galley(r.min + Vec2::splat(4.0), g, p.ink);
    }
}

/// One row of a Links section: the field or the events on the left, the object on the right in the link colour
/// (or the reference, muted, when the level has no such object). When it leads somewhere: the object's index, the
/// row's rectangle and whether it was clicked.
fn link_row(
    ui: &mut egui::Ui,
    p: &Palette,
    m: &Metrics,
    scene: &Scene,
    r: &LinkRow,
    v: &ViewState,
) -> Option<(usize, Rect, bool)> {
    let target = r
        .target
        .filter(|&t| v.doc.as_ref().is_none_or(|d| !d.deleted.contains(&t)));
    let sense = if target.is_some() {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), m.row), sense);
    let painter = ui.painter_at(rect);
    if resp.hovered() && target.is_some() {
        painter.rect_filled(rect, CornerRadius::same(m.radius_sm as u8), p.surface_2);
    }
    let half = rect.width() * 0.45;
    let left = elided(
        ui,
        &r.left,
        FontId::monospace(11.5),
        p.ink_muted,
        half - m.space_2 * 2.0,
    );
    painter.galley(
        Pos2::new(rect.left() + m.space_2, rect.center().y - left.size().y / 2.0),
        left,
        p.ink_muted,
    );
    let (text, colour) = match target {
        Some(t) => {
            let (name, ty) = object_text(scene, t);
            (format!("\u{2192} {name}  {ty}"), p.accent_text)
        }
        None if r.reference == "null" || r.reference.is_empty() => ("(no object)".to_owned(), p.ink_muted),
        None => (r.reference.clone(), p.ink_muted),
    };
    let right = elided(
        ui,
        &text,
        FontId::proportional(12.5),
        colour,
        rect.width() - half - m.space_2,
    );
    painter.galley(
        Pos2::new(rect.left() + half, rect.center().y - right.size().y / 2.0),
        right,
        colour,
    );
    painter.hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, p.line));
    let tip = match target {
        Some(t) => {
            let e = scene.entities.get(t);
            format!(
                "Go to {} ({})",
                e.map_or_else(String::new, model::display_name),
                e.map_or("", |e| e.guid.as_str())
            )
        }
        None if r.reference.contains('/') => format!("In another partition: {}", r.reference),
        None => format!("Not an object of this level: {}", r.reference),
    };
    let clicked = resp.on_hover_text(tip).clicked();
    target.map(|t| (t, rect, clicked))
}
