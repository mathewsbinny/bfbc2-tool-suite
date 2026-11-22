//! The views' editing tools behind one trait (round PR1b-0, audit AU5 ARC-6 / AU1 R21): each tool is a small type
//! that owns its state and implements [`ViewTool`] (its pointer input on a view, a click, what it draws over the
//! views, its keys, and what it asks of the editor as [`Outcome`]s: a message, a selection, an operation committed),
//! so a new tool (E4's flags, spawns and objectives, E5's brushes) is a new type, not new arms in the views'
//! 300-700-line functions.
//!
//! The editor keeps every tool's state in [`Tools`] and the chosen one's id ([`Tool`], what the actions and the
//! toolbar name); the views call:
//!
//! - [`ViewTool::input`] on the chosen tool, before the editing input (the left button is the tool's when it answers);
//! - [`ViewTool::click`] on the chosen tool, for a click while editing (else the click selects);
//! - [`ViewTool::draw`] on every tool, at each overlay [`Layer`] (a tool draws what it holds, chosen or not);
//! - [`ViewTool::enter`], [`ViewTool::transient`] / [`ViewTool::cancel`] (Esc) and [`ViewTool::left`] (another
//!   tool chosen) on every tool.
//!
//! The gizmo tools (Move, Rotate) keep the gizmo's drag in the views (its handles, snapping and surface placement are
//! shared by both: [`crate::gizmo`]); [`ViewTool::gizmo`] says a tool uses it, [`ViewTool::boxes`] that a drag on
//! empty ground draws a selection box.

use eframe::egui::{self, CornerRadius, FontId, Pos2, Rect, Stroke, Vec2};

use bc2_levelwrite::Op;

use crate::camera::{Projection, View};
use crate::gizmo::Tool;
use crate::math::{self, V3};
use crate::query::{self, Hit};
use crate::theme::Palette;

/// What a tool asks the editor to do after an input.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Nothing more (the input was the tool's).
    Done,
    /// A message in the status bar.
    Message(String),
    /// The selection replaced by these entities, with a message said first.
    Select(Vec<usize>, String),
    /// An operation committed to the project (with its History label; empty: the operation's own name).
    Commit(Box<Op>, String),
}

/// Where in a view's overlays a tool draws (the order the views draw them in).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// With the editing overlays: after the gizmo and the selection box, before the gizmo toolbar.
    Edit,
    /// With the tools' overlays: after the object being placed, before the workspace's toolbar.
    Tools,
}

/// A view's pointer as the tools' input sees it (this frame).
#[derive(Debug, Clone, Copy)]
pub struct Pointer {
    /// The view.
    pub projection: Projection,
    /// Its rectangle on screen.
    pub rect: Rect,
    /// Its camera's view of the scene at that size.
    pub view: View,
    /// A click this frame.
    pub clicked: bool,
    /// A double click this frame.
    pub double_clicked: bool,
    /// Where the button went down, when it did.
    pub at: Option<Pos2>,
    /// A document is open and the workspace moves objects (the object tools' input is live).
    pub editing: bool,
}

/// What a tool draws with: a view's painter (clipped to it) and where things are on it.
pub struct Draw<'a> {
    /// The painter.
    pub painter: &'a egui::Painter,
    /// The view.
    pub projection: Projection,
    /// Its rectangle on screen.
    pub rect: Rect,
    /// Its camera's view of the scene.
    pub view: View,
    /// The pointer over it, if it is.
    pub hover: Option<Pos2>,
    /// The colours.
    pub palette: &'a Palette,
}

impl Draw<'_> {
    /// A world point on the screen.
    #[must_use]
    pub fn to_screen(&self, w: V3) -> Option<Pos2> {
        self.view
            .project(w)
            .map(|s| self.rect.min + Vec2::new(s[0] as f32, s[1] as f32))
    }
}

/// An editing tool of the views (see the module).
pub trait ViewTool {
    /// Its id (the actions' and the toolbar's name for it).
    fn id(&self) -> Tool;

    /// Its key on the gizmo toolbar, when it is on it.
    fn key(&self) -> Option<&'static str> {
        None
    }

    /// Whether it moves or turns the selection with the gizmo.
    fn gizmo(&self) -> bool {
        false
    }

    /// Whether a left drag on empty ground draws a selection box while it is chosen.
    fn boxes(&self) -> bool {
        true
    }

    /// The pointer on a view while it is chosen, before the editing input: `Some` when the left button is the
    /// tool's this frame (with what to do), `None` when it is not.
    fn input(&mut self, _pointer: &Pointer, _view: &crate::app::ViewState) -> Option<Outcome> {
        None
    }

    /// A click while editing with it chosen (`hit`: the surface under the pointer): `Some` when the click is the
    /// tool's (the selection is left alone), `None` when the click selects.
    fn click(&mut self, _hit: Option<Hit>, _snap_vertex: bool) -> Option<Outcome> {
        None
    }

    /// What it draws over a view at `layer` (whether chosen or not: what it holds).
    fn draw(&self, _d: &Draw<'_>, _layer: Layer) {}

    /// Whether Enter is its key now (the key is then taken for it).
    fn takes_enter(&self) -> bool {
        false
    }

    /// Enter pressed (when [`ViewTool::takes_enter`]).
    fn enter(&mut self) {}

    /// Whether it holds something Esc cancels first.
    fn transient(&self) -> bool {
        false
    }

    /// Esc: what it holds dropped.
    fn cancel(&mut self) {}

    /// Another tool was chosen.
    fn left(&mut self) {}
}

/// Select: a click selects, a drag draws a box.
#[derive(Debug, Default)]
pub struct SelectTool;

impl ViewTool for SelectTool {
    fn id(&self) -> Tool {
        Tool::Select
    }

    fn key(&self) -> Option<&'static str> {
        Some("Q")
    }
}

/// Move or Rotate: the gizmo on the selection.
#[derive(Debug)]
pub struct GizmoTool {
    rotate: bool,
}

impl ViewTool for GizmoTool {
    fn id(&self) -> Tool {
        if self.rotate { Tool::Rotate } else { Tool::Move }
    }

    fn key(&self) -> Option<&'static str> {
        Some(if self.rotate { "E" } else { "W" })
    }

    fn gizmo(&self) -> bool {
        true
    }
}

/// The ruler: two clicked points and the distance between them.
#[derive(Debug, Default)]
pub struct RulerTool {
    /// Its points (up to two).
    pub points: Vec<V3>,
}

impl ViewTool for RulerTool {
    fn id(&self) -> Tool {
        Tool::Ruler
    }

    fn key(&self) -> Option<&'static str> {
        Some("R")
    }

    fn boxes(&self) -> bool {
        false
    }

    fn click(&mut self, hit: Option<Hit>, snap_vertex: bool) -> Option<Outcome> {
        let Some(h) = hit else {
            return Some(Outcome::Done);
        };
        let p = if snap_vertex { query::snap_vertex(&h) } else { h.point };
        if self.points.len() >= 2 {
            self.points.clear();
        }
        self.points.push(p);
        Some(match self.points.as_slice() {
            [a, b] => Outcome::Message(ruler_text(*a, *b)),
            _ => Outcome::Done,
        })
    }

    fn draw(&self, d: &Draw<'_>, layer: Layer) {
        if layer != Layer::Edit {
            return;
        }
        let p = d.palette;
        let pts: Vec<Pos2> = self.points.iter().filter_map(|w| d.to_screen(*w)).collect();
        for q in &pts {
            d.painter.circle_filled(*q, 4.0, p.view_selection);
        }
        if let ([a, b], [wa, wb]) = (pts.as_slice(), self.points.as_slice()) {
            d.painter.line_segment([*a, *b], Stroke::new(2.0, p.view_selection));
            let label = ruler_text(*wa, *wb);
            let g = d.painter.layout_no_wrap(label, FontId::monospace(12.0), p.ink);
            let mid = Pos2::new(f32::midpoint(a.x, b.x), f32::midpoint(a.y, b.y)) - Vec2::new(0.0, 14.0);
            let r = Rect::from_center_size(mid, g.size() + Vec2::splat(8.0));
            d.painter.rect_filled(r, CornerRadius::same(2), p.scene_label);
            d.painter.galley(r.min + Vec2::splat(4.0), g, p.ink);
        }
    }

    fn left(&mut self) {
        self.points.clear();
    }
}

/// Polygon selection: clicks add corners, a double click or Enter selects what is inside.
#[derive(Debug, Default)]
pub struct PolygonTool {
    /// The corners (screen).
    pub corners: Vec<Pos2>,
    /// The view they are drawn in.
    view: Option<Projection>,
    /// Enter was pressed: the polygon closes.
    close: bool,
}

impl PolygonTool {
    /// The selectable entities whose position is inside the polygon (drawn on `view` at `rect`).
    fn inside_of(corners: &[Pos2], rect: Rect, view: &View, v: &crate::app::ViewState) -> Vec<usize> {
        v.scene()
            .entities
            .iter()
            .filter(|e| v.selectable(e.index) && v.doc.as_ref().is_none_or(|d| d.why_not(e.index).is_none()))
            .filter_map(|e| {
                let s = view.project(e.position()?)?;
                inside(corners, rect.min + Vec2::new(s[0] as f32, s[1] as f32)).then_some(e.index)
            })
            .collect()
    }
}

impl ViewTool for PolygonTool {
    fn id(&self) -> Tool {
        Tool::Polygon
    }

    fn input(&mut self, pointer: &Pointer, v: &crate::app::ViewState) -> Option<Outcome> {
        if !pointer.editing {
            return None;
        }
        if pointer.double_clicked || (self.close && !self.corners.is_empty()) {
            self.close = false;
            let corners = std::mem::take(&mut self.corners);
            if corners.len() < 3 {
                return Some(Outcome::Done);
            }
            let list = Self::inside_of(&corners, pointer.rect, &pointer.view, v);
            let said = format!("{} selected", list.len());
            return Some(Outcome::Select(list, said));
        }
        if pointer.clicked
            && let Some(p) = pointer.at
        {
            if self.view != Some(pointer.projection) {
                self.corners.clear();
                self.view = Some(pointer.projection);
            }
            self.corners.push(p);
        }
        Some(Outcome::Done)
    }

    fn draw(&self, d: &Draw<'_>, layer: Layer) {
        if layer != Layer::Tools || self.view != Some(d.projection) || self.corners.is_empty() {
            return;
        }
        let mut pts = self.corners.clone();
        if let Some(h) = d.hover {
            pts.push(h);
        }
        d.painter.add(egui::Shape::closed_line(
            pts.clone(),
            Stroke::new(1.5, d.palette.view_selection),
        ));
        for q in &self.corners {
            d.painter.circle_filled(*q, 3.5, d.palette.view_selection);
        }
    }

    fn takes_enter(&self) -> bool {
        self.corners.len() >= 3
    }

    fn enter(&mut self) {
        self.close = true;
    }

    fn transient(&self) -> bool {
        !self.corners.is_empty()
    }

    fn cancel(&mut self) {
        self.corners.clear();
    }
}

/// Every tool's state; the chosen one is [`crate::app::ViewState::tool`].
#[derive(Debug)]
pub struct Tools {
    select: SelectTool,
    moving: GizmoTool,
    rotating: GizmoTool,
    /// The ruler.
    pub ruler: RulerTool,
    /// The polygon selection.
    pub polygon: PolygonTool,
}

impl Default for Tools {
    fn default() -> Self {
        Self {
            select: SelectTool,
            moving: GizmoTool { rotate: false },
            rotating: GizmoTool { rotate: true },
            ruler: RulerTool::default(),
            polygon: PolygonTool::default(),
        }
    }
}

impl Tools {
    /// The tool of an id.
    #[must_use]
    pub fn get(&self, t: Tool) -> &dyn ViewTool {
        match t {
            Tool::Select => &self.select,
            Tool::Move => &self.moving,
            Tool::Rotate => &self.rotating,
            Tool::Ruler => &self.ruler,
            Tool::Polygon => &self.polygon,
        }
    }

    /// The tool of an id, to give it input.
    pub fn get_mut(&mut self, t: Tool) -> &mut dyn ViewTool {
        match t {
            Tool::Select => &mut self.select,
            Tool::Move => &mut self.moving,
            Tool::Rotate => &mut self.rotating,
            Tool::Ruler => &mut self.ruler,
            Tool::Polygon => &mut self.polygon,
        }
    }

    /// Every tool, in the toolbar's order.
    #[must_use]
    pub fn all(&self) -> [&dyn ViewTool; 5] {
        [&self.select, &self.moving, &self.rotating, &self.ruler, &self.polygon]
    }

    /// Every tool, to tell each of them something.
    pub fn all_mut(&mut self) -> [&mut dyn ViewTool; 5] {
        [
            &mut self.select,
            &mut self.moving,
            &mut self.rotating,
            &mut self.ruler,
            &mut self.polygon,
        ]
    }

    /// `t` chosen: every other tool is told it was left.
    pub fn choose(&mut self, t: Tool) {
        for other in self.all_mut() {
            if other.id() != t {
                other.left();
            }
        }
    }

    /// Whether any tool holds something Esc cancels.
    #[must_use]
    pub fn transient(&self) -> bool {
        self.all().iter().any(|t| t.transient())
    }

    /// Esc: every tool drops what it holds.
    pub fn cancel(&mut self) {
        for t in self.all_mut() {
            t.cancel();
        }
    }

    /// Every tool's drawing at `layer`.
    pub fn draw(&self, d: &Draw<'_>, layer: Layer) {
        for t in self.all() {
            t.draw(d, layer);
        }
    }
}

/// The ruler's read-out: total, horizontal and height difference.
#[must_use]
pub fn ruler_text(a: V3, b: V3) -> String {
    let d = math::sub(b, a);
    format!(
        "{:.2} m  (horizontal {:.2} m, height {:+.2} m)",
        math::len(d),
        d[0].hypot(d[2]),
        d[1]
    )
}

/// Whether a screen point is inside a polygon (even-odd).
#[must_use]
pub fn inside(poly: &[Pos2], p: Pos2) -> bool {
    let mut odd = false;
    let n = poly.len();
    for i in 0..n {
        let (Some(a), Some(b)) = (poly.get(i), poly.get((i + n - 1) % n)) else {
            continue;
        };
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            odd = !odd;
        }
    }
    odd
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_is_its_id_and_the_toolbar_keys_are_kept() {
        let tools = Tools::default();
        for t in [Tool::Select, Tool::Move, Tool::Rotate, Tool::Ruler, Tool::Polygon] {
            assert_eq!(tools.get(t).id(), t);
        }
        let keys: Vec<(Tool, &str)> = tools.all().iter().filter_map(|t| Some((t.id(), t.key()?))).collect();
        assert_eq!(
            keys,
            [
                (Tool::Select, "Q"),
                (Tool::Move, "W"),
                (Tool::Rotate, "E"),
                (Tool::Ruler, "R")
            ]
        );
        assert!(tools.get(Tool::Move).gizmo() && tools.get(Tool::Rotate).gizmo());
        assert!(!tools.get(Tool::Ruler).boxes() && tools.get(Tool::Polygon).boxes());
    }

    #[test]
    fn the_ruler_takes_two_points_and_is_cleared_when_left() {
        let mut tools = Tools::default();
        let hit = |x: f64| Hit {
            t: 1.0,
            point: [x, 0.0, 0.0],
            normal: [0.0, 1.0, 0.0],
            entity: None,
            tri: [[x, 0.0, 0.0], [x + 1.0, 0.0, 0.0], [x, 0.0, 1.0]],
        };
        let r = tools.get_mut(Tool::Ruler);
        assert_eq!(r.click(None, false), Some(Outcome::Done));
        assert_eq!(r.click(Some(hit(1.0)), false), Some(Outcome::Done));
        assert_eq!(
            r.click(Some(hit(4.0)), false),
            Some(Outcome::Message(ruler_text([1.0, 0.0, 0.0], [4.0, 0.0, 0.0])))
        );
        // a third point starts again
        assert_eq!(r.click(Some(hit(2.0)), false), Some(Outcome::Done));
        assert_eq!(tools.ruler.points.len(), 1);
        tools.choose(Tool::Ruler);
        assert_eq!(tools.ruler.points.len(), 1);
        tools.choose(Tool::Move);
        assert_eq!(tools.ruler.points.len(), 0);
        // the selecting tools leave clicks to the selection
        assert_eq!(tools.get_mut(Tool::Select).click(Some(hit(0.0)), false), None);
    }

    #[test]
    fn the_polygon_is_transient_and_takes_enter_at_three_corners() {
        let mut tools = Tools::default();
        tools.polygon.corners = vec![Pos2::ZERO, Pos2::new(10.0, 0.0)];
        assert!(tools.transient() && !tools.polygon.takes_enter());
        tools.polygon.corners.push(Pos2::new(0.0, 10.0));
        assert!(tools.polygon.takes_enter());
        // choosing another tool keeps the corners (as before the trait); Esc drops them
        tools.choose(Tool::Select);
        assert_eq!(tools.polygon.corners.len(), 3);
        tools.cancel();
        assert!(!tools.transient());
    }
}
