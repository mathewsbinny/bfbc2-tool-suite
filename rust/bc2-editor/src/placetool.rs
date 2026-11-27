//! Placing from the content browser as a view tool (round E4-rest2, PR1b-0's leftover; audit AU5 ARC-6): an entry
//! dragged from the content browser or the asset browser, a Spawners or Objectives tile, or a blueprint chosen in the
//! "Place object here" picker follows the pointer as [`PlaceTool`] - the views' tool ([`Tool::Place`]) while it is
//! placed, the tool chosen before coming back once it is placed or dropped. Its input moves the object to the surface
//! under the pointer; a click (or the drag released over a view) is an [`Outcome::Place`], which the editor commits
//! (`place`, or a model group's `duplicate`); it draws the object's box and its yaw and sink over the views; Esc drops
//! it. The wheel's turn and sink while placing stay with the views' wheel (Shift+wheel, Alt+wheel).

use eframe::egui::{CornerRadius, FontId, Rect, Stroke, Vec2};

use crate::app::Placing;
use crate::gizmo::Tool;
use crate::math::{self, V3};
use crate::query;
use crate::tool::{Draw, Layer, Outcome, Pointer, ViewTool};

/// The object being placed and the tool to go back to (see the module).
#[derive(Debug, Clone, Default)]
pub struct PlaceTool {
    /// The object being placed.
    pub placing: Option<Placing>,
    /// The tool chosen before placing began (chosen again when it ends).
    pub back: Tool,
}

impl PlaceTool {
    /// The object to the surface under a ray (the project's hidden objects left out).
    pub fn to_ray(&mut self, o: V3, dir: V3, v: &crate::app::ViewState) {
        let hit = {
            let skip = |k: usize| v.doc.as_ref().is_some_and(|d| !d.shown(k));
            query::ray_hit_except(v.scene(), o, dir, &skip)
        };
        if let Some(p) = self.placing.as_mut() {
            p.at = hit.map(|h| query::place(&math::upright(h.point, p.yaw), p.extent, &h, &p.placement));
        }
    }
}

impl ViewTool for PlaceTool {
    fn id(&self) -> Tool {
        Tool::Place
    }

    fn boxes(&self) -> bool {
        false
    }

    fn input(&mut self, pointer: &Pointer, v: &crate::app::ViewState) -> Option<Outcome> {
        self.placing.as_ref()?;
        if let Some(h) = pointer.hover {
            let (o, d) = pointer
                .view
                .ray([f64::from(h.x - pointer.rect.min.x), f64::from(h.y - pointer.rect.min.y)]);
            self.to_ray(o, d, v);
        }
        Some(if pointer.dropped || pointer.clicked {
            Outcome::Place
        } else {
            Outcome::Done
        })
    }

    fn draw(&self, d: &Draw<'_>, layer: Layer) {
        if layer != Layer::Tools {
            return;
        }
        let Some(pl) = &self.placing else { return };
        let Some(m) = pl.at else { return };
        let p = d.palette;
        let (hx, hz) = pl.half;
        let (lo, hi) = pl.extent;
        let corner = |i: usize| -> V3 {
            let l = [
                if i & 1 == 0 { -hx } else { hx },
                if i & 2 == 0 { lo } else { hi },
                if i & 4 == 0 { -hz } else { hz },
            ];
            let mut w = math::row(&m, 3);
            for (r, v) in l.iter().enumerate() {
                w = math::add(w, math::mul(math::row(&m, r), *v));
            }
            w
        };
        for (a, b) in [
            (0, 1),
            (2, 3),
            (4, 5),
            (6, 7),
            (0, 2),
            (1, 3),
            (4, 6),
            (5, 7),
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7),
        ] {
            if let (Some(x), Some(y)) = (d.to_screen(corner(a)), d.to_screen(corner(b))) {
                d.painter.line_segment([x, y], Stroke::new(2.0, p.view_selection));
            }
        }
        if let Some(c) = d.to_screen(math::row(&m, 3)) {
            d.painter.circle_filled(c, 4.0, p.view_selection);
            let text = format!(
                "{}  yaw {:.0} deg  sink {:.2} m",
                pl.entry.name, pl.yaw, pl.placement.sink
            );
            let g = d.painter.layout_no_wrap(text, FontId::proportional(12.0), p.ink);
            let r = Rect::from_min_size(c + Vec2::new(12.0, 10.0), g.size()).expand(4.0);
            d.painter.rect_filled(r, CornerRadius::same(3), p.scene_label);
            d.painter.galley(r.min + Vec2::splat(4.0), g, p.ink);
        }
    }

    fn transient(&self) -> bool {
        self.placing.is_some()
    }

    fn cancel(&mut self) {
        self.placing = None;
    }

    fn left(&mut self) {
        self.placing = None;
    }
}
