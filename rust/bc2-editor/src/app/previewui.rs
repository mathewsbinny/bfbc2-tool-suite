//! Round GR3's panels: the asset preview ([`crate::preview`], the mesh viewer folded in) and the asset browser's
//! Meshes mode (every mesh of the game copy by name, `meshview.py find`).

#[allow(
    clippy::wildcard_imports,
    reason = "a part of app.rs in a file of its own: its types and helpers"
)]
use super::*;

use bc2_convert::meshview::Channel;
use bc2_convert::meshview::render::View as PictureView;

use crate::preview::{Camera, Colours, Preview, Shading, Subject};

/// The asset preview's picture on screen: the texture, what it was drawn from, its rectangle and size, and each
/// pixel's part.
#[derive(Default)]
pub struct PreviewShown {
    texture: Option<egui::TextureHandle>,
    key: String,
    /// The picture's rectangle on screen (last frame).
    pub rect: Option<Rect>,
    size: [usize; 2],
    owner: Vec<u32>,
    /// The Meshes mode's search and its answer.
    pub mesh_query: String,
    mesh_list: Option<(String, Vec<String>)>,
}

fn rgb(c: Color32) -> [u8; 3] {
    [c.r(), c.g(), c.b()]
}

/// The preview's overlay colours from the theme: the scene's sky behind, the selection's orange, the accent.
#[must_use]
pub fn preview_colours(p: &Palette) -> Colours {
    Colours {
        background: rgb(p.scene_sky),
        edges: rgb(p.ink_faint),
        selection: rgb(p.view_selection),
        accent: rgb(p.accent),
    }
}

/// Keeps a panel inside its tab: its width the tab's visible width (content wider than that is cut at the tab's edge
/// instead of pushing the picture and the toolbar past the window's edge in a tab without scroll bars).
fn fit_to_tab(ui: &mut egui::Ui) {
    let room = (ui.clip_rect().right() - ui.max_rect().left())
        .min(ui.available_width())
        .max(80.0);
    ui.set_max_width(room);
    ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
}

impl Tabs<'_> {
    /// Shows a subject in the preview (and the preview's panel).
    pub(super) fn preview_show(&mut self, subject: Subject, add: bool) {
        let Some(root) = self.view.root.clone() else {
            self.view.preview.status = "No game copy (Project OPFOR BFBC2 not found)".into();
            return;
        };
        if add {
            self.view.preview.add(&root, subject);
        } else {
            self.view.preview.show(&root, subject);
        }
        self.view.pending.push(Action::ShowPanel(PanelKind::Preview));
    }

    /// The asset browser's Meshes mode: every mesh of the game copy whose name contains the search (as
    /// `meshview.py find`); click one to preview it, Shift+click to add it to the comparison.
    pub(super) fn mesh_browser(&mut self, ui: &mut egui::Ui) {
        fit_to_tab(ui);
        let p = self.palette;
        let m = self.metrics;
        let ix = crate::preview::index_ready();
        let total = ix.as_ref().map_or(0, |ix| ix.find("").len());
        let query = self.view.preview_shown.mesh_query.clone();
        let list = match (&ix, &self.view.preview_shown.mesh_list) {
            (Some(_), Some((q, l))) if *q == query => l.clone(),
            (Some(ix), _) => {
                let l = ix.find(&query);
                self.view.preview_shown.mesh_list = Some((query.clone(), l.clone()));
                l
            }
            (None, _) => Vec::new(),
        };
        self.header(
            ui,
            "Meshes",
            &format!("{} of {}", group_digits(list.len() as u64), group_digits(total as u64)),
        );
        egui::Frame::new()
            .inner_margin(Margin::same(m.space_2 as i8))
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.view.preview_shown.mesh_query)
                        .hint_text(RichText::new("Search mesh names").color(p.ink_faint))
                        .desired_width(f32::INFINITY),
                );
                if ix.is_none() {
                    ui.label(
                        RichText::new(
                            "The game copy's mesh index is read the first time a mesh is previewed (built once if \
                             the game copy changed: minutes).",
                        )
                        .text_style(style("small"))
                        .color(p.ink_muted),
                    );
                    if ui.button("Read the mesh index").clicked()
                        && let Some(root) = self.view.root.clone()
                    {
                        std::thread::spawn(move || {
                            let _ = crate::preview::index(&root);
                        });
                    }
                } else {
                    ui.label(
                        RichText::new("Click: preview; Shift+click: add to the comparison")
                            .text_style(style("small"))
                            .color(p.ink_muted),
                    );
                }
            });
        if ix.is_none() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
            return;
        }
        let current: Vec<String> = self
            .view
            .preview
            .subjects
            .iter()
            .filter_map(|s| match s {
                Subject::Mesh(n) => Some(n.clone()),
                Subject::Blueprint(_) => None,
            })
            .collect();
        let mut chosen = None;
        let row_h = 20.0;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, row_h, list.len(), |ui, range| {
                for name in list.get(range).unwrap_or_default() {
                    let on = current.contains(name);
                    let leaf = name.rsplit('/').next().unwrap_or(name);
                    let r = ui
                        .add(egui::Button::selectable(on, leaf).min_size(Vec2::new(0.0, row_h)))
                        .on_hover_text(name);
                    if r.clicked() {
                        chosen = Some((name.clone(), ui.input(|i| i.modifiers.shift)));
                    }
                }
            });
        if let Some((name, add)) = chosen {
            self.preview_show(Subject::Mesh(name), add);
        }
    }

    /// The asset preview panel: the toolbar, the picture, the measurements, the parts and the materials.
    #[allow(clippy::too_many_lines, reason = "one panel, top to bottom")]
    pub(super) fn preview_panel(&mut self, ui: &mut egui::Ui) {
        fit_to_tab(ui);
        let p = self.palette;
        let m = self.metrics;
        if self.view.preview.poll() {
            self.view.preview_shown.key.clear();
        }
        if self.view.preview.reading() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(150));
        }
        let pv = &self.view.preview;
        let title = pv.models.first().map_or_else(String::new, |md| {
            format!(
                "{}  LOD {}  {} tris",
                md.mesh.rsplit('/').next().unwrap_or(&md.mesh),
                md.read.lod,
                group_digits(pv.models.iter().map(crate::preview::Model::triangles).sum::<usize>() as u64)
            )
        });
        self.header(ui, "Asset preview", &title);
        if pv.subjects.is_empty() {
            egui::Frame::new()
                .inner_margin(Margin::same(m.space_3 as i8))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(
                            "Pick an object in Place Objects, or a mesh in the browser's Meshes mode, to see it here: \
                             its levels of detail, parts, destruction states, textures by channel, wireframe, \
                             normals and bounds.",
                        )
                        .color(p.ink_muted),
                    );
                });
            return;
        }
        // ---- the toolbar
        let mut reload = false;
        let mut save = false;
        egui::Frame::new()
            .inner_margin(Margin::symmetric(m.space_2 as i8, m.space_1 as i8))
            .show(ui, |ui| {
                let pv = &mut self.view.preview;
                ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                // the camera
                ui.horizontal(|ui| {
                    let orbit = matches!(pv.camera, Camera::Orbit { .. });
                    if ui
                        .selectable_label(orbit, "Orbit")
                        .on_hover_text("Drag to turn, right-drag to move, wheel to zoom")
                        .clicked()
                        && !orbit
                    {
                        pv.camera = Camera::orbit();
                    }
                    for v in PictureView::ALL {
                        let on = pv.camera == Camera::Preset(v);
                        let name = match v {
                            PictureView::Side => "Side",
                            PictureView::Top => "Top",
                            PictureView::Front => "Front",
                        };
                        if ui
                            .selectable_label(on, name)
                            .on_hover_text("The mesh viewer's picture framing (meshview.py --png --view)")
                            .clicked()
                        {
                            pv.camera = Camera::Preset(v);
                        }
                    }
                    if let Camera::Orbit { perspective, .. } = &mut pv.camera {
                        ui.checkbox(perspective, "Perspective");
                    }
                    if ui
                        .button("Frame")
                        .on_hover_text("The camera back to its start")
                        .clicked()
                    {
                        pv.camera = match pv.camera {
                            Camera::Orbit { .. } => Camera::orbit(),
                            c @ Camera::Preset(_) => c,
                        };
                    }
                });
                // what is drawn
                ui.horizontal(|ui| {
                    let lods = pv.models.first().map_or_default(|md| md.lods.clone());
                    egui::ComboBox::from_id_salt("preview lod")
                        .width(64.0)
                        .selected_text(format!("LOD {}", pv.lod))
                        .show_ui(ui, |ui| {
                            for l in &lods {
                                if ui.selectable_label(pv.lod == *l, format!("LOD {l}")).clicked() && pv.lod != *l {
                                    pv.lod = *l;
                                    reload = true;
                                }
                            }
                        });
                    egui::ComboBox::from_id_salt("preview shading")
                        .width(88.0)
                        .selected_text(pv.shading.name())
                        .show_ui(ui, |ui| {
                            for s in Shading::ALL {
                                ui.selectable_value(&mut pv.shading, s, s.name());
                            }
                        });
                    let ch = pv.channel.unwrap_or(Channel::Colour);
                    egui::ComboBox::from_id_salt("preview channel")
                        .width(96.0)
                        .selected_text(ch.name())
                        .show_ui(ui, |ui| {
                            for c in Channel::ALL {
                                if ui.selectable_label(ch == c, c.name()).clicked() {
                                    pv.channel = Some(c);
                                }
                            }
                        })
                        .response
                        .on_hover_text("The texture of each part's shader drawn (Colour: the mesh viewer's)");
                });
                // the overlays and the destruction state
                ui.horizontal(|ui| {
                    ui.checkbox(&mut pv.wireframe, "Edges");
                    ui.checkbox(&mut pv.normals, "Normals");
                    ui.checkbox(&mut pv.bounds, "Bounds");
                    let stages = pv
                        .models
                        .iter()
                        .map(|md| bc2_render::simple::stage_count(&md.components))
                        .max()
                        .unwrap_or(0);
                    if stages > 0 {
                        let label = |s: bc2_render::simple::State| match s {
                            bc2_render::simple::State::Intact => "Intact".to_owned(),
                            bc2_render::simple::State::Destroyed => "Destroyed".to_owned(),
                            bc2_render::simple::State::All => "Every part".to_owned(),
                            bc2_render::simple::State::Stage(k) => format!("Stage {k}"),
                        };
                        egui::ComboBox::from_id_salt("preview state")
                            .width(96.0)
                            .selected_text(label(pv.state))
                            .show_ui(ui, |ui| {
                                let mut all = vec![bc2_render::simple::State::Intact];
                                all.extend(
                                    (1..stages.saturating_sub(1)).map(|k| bc2_render::simple::State::Stage(k as u8)),
                                );
                                all.extend([bc2_render::simple::State::Destroyed, bc2_render::simple::State::All]);
                                for s in all {
                                    ui.selectable_value(&mut pv.state, s, label(s));
                                }
                            })
                            .response
                            .on_hover_text("The blueprint's destruction states: the parts each health state shows");
                    }
                });
                ui.horizontal(|ui| {
                    if ui
                        .button("Save picture")
                        .on_hover_text(
                            "A PNG in work\\tools_out\\meshview (presets: 1200 px wide, as meshview.py --png)",
                        )
                        .clicked()
                    {
                        save = true;
                    }
                    if pv.subjects.len() > 1 && ui.button("Clear comparison").clicked() {
                        pv.subjects.truncate(1);
                        reload = true;
                    }
                });
            });
        let root = self.view.root.clone();
        if reload && let Some(r) = &root {
            self.view.preview.reload(r);
        }
        let colours = preview_colours(&p);
        if save && let Some(r) = &root {
            let file = self.view.preview.picture_file(r);
            let size = match self.view.preview.camera {
                Camera::Preset(_) => 1200,
                Camera::Orbit { .. } => 1600,
            };
            // the command line's grey behind (a preset's picture is then `meshview.py --png`'s, pixel for pixel)
            self.view.message = match self.view.preview.save_picture(&file, size, Colours::default()) {
                Ok((w, h)) => format!("Saved the preview ({w} x {h}): {}", file.display()),
                Err(e) => format!("Not saved: {e}"),
            };
        }
        // ---- the picture
        let ppp = ui.ctx().pixels_per_point();
        let avail = ui.available_size();
        let view_h = (avail.y * 0.58).max(160.0);
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(avail.x.max(80.0), view_h), Sense::click_and_drag());
        let px = [
            ((rect.width() * ppp).round() as usize).max(48),
            ((rect.height() * ppp).round() as usize).max(48),
        ];
        // the mouse: orbit, pan, zoom
        let mut moved = false;
        let (drag, secondary, scroll) = ui.input(|i| {
            (
                resp.dragged_by(PointerButton::Primary).then_some(i.pointer.delta()),
                resp.dragged_by(PointerButton::Secondary) || resp.dragged_by(PointerButton::Middle),
                if resp.hovered() { i.smooth_scroll_delta.y } else { 0.0 },
            )
        });
        let pv = &mut self.view.preview;
        if let Some(d) = drag
            && d != Vec2::ZERO
        {
            if let Camera::Preset(_) = pv.camera {
                pv.camera = Camera::orbit();
            }
            if let Camera::Orbit { yaw, pitch, .. } = &mut pv.camera {
                *yaw -= d.x * 0.01;
                *pitch = (*pitch + d.y * 0.01).clamp(-1.5, 1.5);
                moved = true;
            }
        }
        if secondary && let Camera::Orbit { pan, .. } = &mut pv.camera {
            let d = ui.input(|i| i.pointer.delta()) * ppp;
            pan[0] += d.x;
            pan[1] += d.y;
            moved = true;
        }
        if scroll != 0.0
            && let Camera::Orbit { distance, .. } = &mut pv.camera
        {
            *distance = (*distance * (-scroll * 0.002).exp()).clamp(0.3, 40.0);
            moved = true;
        }
        let key = format!(
            "{} {:?} {:?} {:?} {:?} {} {} {} {:?} {:?} {:?} {px:?} {:?}",
            pv.revision,
            pv.camera,
            pv.shading,
            pv.channel,
            pv.state,
            pv.wireframe,
            pv.normals,
            pv.bounds,
            pv.hidden,
            pv.selected,
            p.view_selection,
            p.scene_sky
        );
        let shown = &mut self.view.preview_shown;
        if (shown.key != key || shown.texture.is_none() || moved) && !pv.models.is_empty() {
            let (img, owner) = pv.draw(px[0], px[1], colours);
            let ci = egui::ColorImage::from_rgb([img.w, img.h], &img.rgb);
            match &mut shown.texture {
                Some(t) => t.set(ci, egui::TextureOptions::LINEAR),
                None => {
                    shown.texture = Some(ui.ctx().load_texture("asset preview", ci, egui::TextureOptions::LINEAR));
                }
            }
            shown.size = [img.w, img.h];
            shown.owner = owner;
            shown.key = key;
        }
        ui.painter().rect_filled(rect, CornerRadius::ZERO, p.scene_sky);
        let mut pic = rect;
        if let Some(t) = &shown.texture
            && !pv.models.is_empty()
        {
            // fitted in the rectangle, its aspect kept (a preset's height is its framing's)
            let (w, h) = (shown.size[0] as f32, shown.size[1] as f32);
            let s = (rect.width() / w).min(rect.height() / h);
            pic = Rect::from_center_size(rect.center(), Vec2::new(w * s, h * s));
            ui.painter().image(
                t.id(),
                pic,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        shown.rect = Some(pic);
        if pv.reading() || pv.models.is_empty() {
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                if pv.status.is_empty() {
                    "Reading\u{2026}"
                } else {
                    &pv.status
                },
                FontId::proportional(13.0),
                p.ink_muted,
            );
        }
        if resp.clicked()
            && let Some(at) = resp.interact_pointer_pos()
            && pic.contains(at)
        {
            let fx = ((at.x - pic.left()) / pic.width() * shown.size[0] as f32) as usize;
            let fy = ((at.y - pic.top()) / pic.height() * shown.size[1] as f32) as usize;
            let o = shown.owner.get(fy * shown.size[0] + fx).copied().unwrap_or(u32::MAX);
            pv.selected = Preview::part_at(o);
        }
        // ---- the measurements
        let pv = &self.view.preview;
        egui::Frame::new()
            .inner_margin(Margin::symmetric(m.space_2 as i8, m.space_1 as i8))
            .show(ui, |ui| {
                if let Some((lo, hi)) = pv.bounds_box() {
                    ui.label(
                        RichText::new(format!(
                            "Bounds {:.3} x {:.3} x {:.3} m (x, y, z)\ncentre ({:.3}, {:.3}, {:.3})",
                            hi[0] - lo[0],
                            hi[1] - lo[1],
                            hi[2] - lo[2],
                            f32::midpoint(lo[0], hi[0]),
                            f32::midpoint(lo[1], hi[1]),
                            f32::midpoint(lo[2], hi[2]),
                        ))
                        .text_style(style("value"))
                        .color(p.ink),
                    );
                }
                if !pv.status.is_empty() && !pv.reading() {
                    ui.label(RichText::new(&pv.status).text_style(style("small")).color(p.ink_muted));
                }
            });
        // ---- the parts and materials
        let mut toggle: Option<(usize, String)> = None;
        let mut select: Option<(usize, usize)> = None;
        let mut remove: Option<usize> = None;
        egui::ScrollArea::vertical()
            .id_salt("preview parts")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(m.space_1);
                for (k, md) in pv.models.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "{}  LOD {} of {}  {} parts  {} tris  {} verts",
                                md.mesh.rsplit('/').next().unwrap_or(&md.mesh),
                                md.read.lod,
                                md.lods.len(),
                                md.read.parts.len(),
                                group_digits(md.triangles() as u64),
                                group_digits(md.vertices() as u64)
                            ))
                            .text_style(style("body-strong"))
                            .color(p.ink),
                        )
                        .on_hover_text(&md.mesh);
                        if k > 0 && ui.small_button("Remove").clicked() {
                            remove = Some(k);
                        }
                    });
                    for (pi, part) in md.read.parts.iter().enumerate() {
                        let drawn = pv.part_drawn(k, part);
                        let hidden = pv.hidden.contains(&(k, part.name.clone()));
                        ui.horizontal(|ui| {
                            let mut vis = !hidden;
                            if ui
                                .checkbox(&mut vis, "")
                                .on_hover_text("Show or hide the part")
                                .changed()
                            {
                                toggle = Some((k, part.name.clone()));
                            }
                            let sel = pv.selected == Some((k, pi));
                            let name = RichText::new(&part.name).color(if drawn { p.ink } else { p.ink_faint });
                            if ui.selectable_label(sel, name).clicked() {
                                select = Some((k, pi));
                            }
                            ui.label(
                                RichText::new(format!(
                                    "{} tris  {}",
                                    group_digits(part.indices.len() as u64),
                                    part.texture
                                        .as_deref()
                                        .map_or("(no colour texture)", |t| t.rsplit('/').next().unwrap_or(t))
                                ))
                                .text_style(style("small"))
                                .color(p.ink_muted),
                            );
                        });
                    }
                    egui::CollapsingHeader::new(RichText::new("Materials and textures").color(p.ink))
                        .id_salt(("preview materials", k))
                        .show(ui, |ui| {
                            for mt in &md.materials {
                                ui.label(
                                    RichText::new(format!("{}  ({})", mt.surface, mt.shader))
                                        .text_style(style("body-strong"))
                                        .color(p.ink),
                                );
                                for (t, ch) in &mt.textures {
                                    ui.label(
                                        RichText::new(format!("  {:<12} {t}", ch.name()))
                                            .text_style(style("value"))
                                            .color(p.ink_muted),
                                    );
                                }
                            }
                            for n in &md.read.notes {
                                ui.label(
                                    RichText::new(n.trim())
                                        .text_style(style("small"))
                                        .color(p.view_status_warn),
                                );
                            }
                        });
                    ui.add_space(m.space_2);
                }
            });
        let pv = &mut self.view.preview;
        if let Some(t) = toggle
            && !pv.hidden.remove(&t)
        {
            pv.hidden.insert(t);
        }
        if let Some(s) = select {
            pv.selected = (pv.selected != Some(s)).then_some(s);
        }
        if let Some(k) = remove
            && let Some(r) = &root
        {
            pv.subjects.remove(k);
            pv.reload(r);
        }
    }
}
