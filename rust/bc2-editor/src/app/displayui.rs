//! Properties' **Display** section (round SV1): how the simplified view shows the selected object - Show as, and the
//! variants its data holds: its render mesh's levels of detail, and a destructible object's state (its parts' health
//! states, from its blueprint's `PartComponentData`). View settings, kept per level, never in the project's log.

#[allow(
    clippy::wildcard_imports,
    reason = "a part of app.rs in a file of its own: its types and helpers"
)]
use super::*;

use bc2_scene::scene::Entity;

use crate::display::{Destruction, ShowAs};

/// A change the section asks for, made after it is drawn.
type Change = Box<dyn FnOnce(&mut crate::display::Display)>;

impl Tabs<'_> {
    /// The Display section of an entity's properties: once the level's render meshes are read (a scene without them,
    /// the demo level's, has nothing to choose between).
    pub(super) fn display_section(&mut self, ui: &mut egui::Ui, e: &Entity) {
        if self.view.gpu_scene.visuals().is_none() {
            return;
        }
        let p = self.palette;
        let m = self.metrics;
        let guid = e.guid.to_lowercase();
        let g = ui.painter().layout_job(caption("Display", p.ink_muted));
        let (r, _) = ui.allocate_exact_size(g.size(), Sense::hover());
        ui.painter().galley(r.min, g, p.ink_muted);
        ui.add_space(m.space_1);
        let scene = &self.view.gpu_scene;
        let d = &scene.display;
        let lods = scene.lod_count(e);
        let parts = scene.components(e).len();
        let mut change: Option<Change> = None;
        egui::Grid::new("display")
            .num_columns(2)
            .spacing([m.space_3, m.space_1])
            .show(ui, |ui| {
                ui.label(RichText::new("Show as").color(p.ink_muted));
                let own = d.objects.get(&guid).copied();
                let shown = own.map_or_else(
                    || format!("As the view ({})", d.show_of(e).name()),
                    |s| s.name().to_owned(),
                );
                egui::ComboBox::from_id_salt("display show as")
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(own.is_none(), "As the view").clicked() {
                            let k = guid.clone();
                            change = Some(Box::new(move |d| {
                                d.objects.remove(&k);
                            }));
                        }
                        for s in ShowAs::OBJECT {
                            if ui.selectable_label(own == Some(s), s.name()).clicked() {
                                let k = guid.clone();
                                change = Some(Box::new(move |d| {
                                    d.objects.insert(k, s);
                                }));
                            }
                        }
                    });
                ui.end_row();
                ui.label(RichText::new("Level of detail").color(p.ink_muted));
                if lods > 1 {
                    let now = d.lods.get(&guid).copied();
                    let text = now.map_or_else(|| "Auto (LOD 0)".to_owned(), |l| format!("LOD {l}"));
                    egui::ComboBox::from_id_salt("display lod")
                        .selected_text(text)
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(now.is_none(), "Auto (LOD 0)").clicked() {
                                let k = guid.clone();
                                change = Some(Box::new(move |d| {
                                    d.lods.remove(&k);
                                }));
                            }
                            for l in 0..lods.min(usize::from(u8::MAX)) as u8 {
                                if ui.selectable_label(now == Some(l), format!("LOD {l}")).clicked() {
                                    let k = guid.clone();
                                    change = Some(Box::new(move |d| {
                                        d.lods.insert(k, l);
                                    }));
                                }
                            }
                        });
                } else {
                    let why = if lods == 1 {
                        "One (its mesh set has no other)"
                    } else {
                        "None (no render mesh read)"
                    };
                    ui.label(RichText::new(why).color(p.ink_faint));
                }
                ui.end_row();
                if parts > 0 {
                    ui.label(RichText::new("State").color(p.ink_muted))
                        .on_hover_text(format!(
                            "Its {parts} destructible parts' health states (its blueprint's data)"
                        ));
                    let now = d.state_of(e);
                    egui::ComboBox::from_id_salt("display state")
                        .selected_text(now.name())
                        .show_ui(ui, |ui| {
                            for s in Destruction::ALL {
                                if ui.selectable_label(now == s, s.name()).clicked() {
                                    let k = guid.clone();
                                    change = Some(Box::new(move |d| {
                                        if s == Destruction::Intact {
                                            d.states.remove(&k);
                                        } else {
                                            d.states.insert(k, s);
                                        }
                                    }));
                                }
                            }
                        });
                    ui.end_row();
                }
            });
        if let Some(c) = change {
            let palette = self.palette;
            self.view.set_display(self.gpu, &palette, c);
        }
        ui.add_space(m.space_3);
    }
}
