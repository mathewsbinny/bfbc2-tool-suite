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
//! Round E4-rest2 moved PR1b-0's leftovers here: the AI workspace's nine navigation edit tools are one implementor
//! ([`crate::navtool::NavDrawTool`], [`Tool::Nav`]) and placing from the content browser another
//! ([`crate::placetool::PlaceTool`], [`Tool::Place`], the views' tool while an object is placed).
//!
//! The gizmo tools (Move, Rotate) keep the gizmo's drag in the views (its handles, snapping and surface placement are
//! shared by both: [`crate::gizmo`]); [`ViewTool::gizmo`] says a tool uses it, [`ViewTool::boxes`] that a drag on
//! empty ground draws a selection box.

use eframe::egui::{self, CornerRadius, FontId, Pos2, Rect, Stroke, Vec2};

use bc2_levelwrite::Op;

use crate::camera::{Projection, View};
use crate::gameplay::GameplayKind;
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
    /// An operation committed, then the objects it made (these guids) selected (round E4: a flag added).
    CommitSelect(Box<Op>, String, Vec<String>),
    /// A brush stroke under way (round E5): the region of the raster it just changed, to redraw at once (the raster
    /// is the tool's, [`crate::sculpt::TerrainTool::stroke`]).
    TerrainPreview(bc2_levelwrite::files::Target, bc2_formats::terrain_codec::Region),
    /// A brush stroke finished (round E5): the rasters as it leaves them (round E5-rest: the first the brush's, then
    /// the ring or the paired material it reached), committed as one operation with its History label; whether a
    /// height was clamped to the level's range.
    Stroke(
        Vec<(bc2_levelwrite::files::Target, bc2_levelwrite::delta::Raster)>,
        String,
        bool,
    ),
    /// The AI workspace's drawing changed (round E4-rest2, [`crate::navtool::NavDrawTool`]): the navigation edit it
    /// made when it is complete, and a note for the Navigation edits panel; the navigation's lines are drawn again.
    NavDrawing(Option<Box<bc2_levelwrite::navedit::NavEdit>>, Option<String>),
    /// The object being placed put down where it is (round E4-rest2, [`crate::placetool::PlaceTool`]).
    Place,
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
#[allow(
    clippy::struct_excessive_bools,
    reason = "the pointer's states this frame, each its own question"
)]
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
    /// Where the pointer is over the view, when it is (round E4: the gameplay tool's ghost).
    pub hover: Option<Pos2>,
    /// A document is open and the workspace moves objects (the object tools' input is live).
    pub editing: bool,
    /// The primary button is held on the view (round E5: a brush paints while it is).
    pub down: bool,
    /// A document is open in the Terrain workspace (round E5: the brushes' input is live).
    pub terrain: bool,
    /// A content browser entry or tile dragged was released over the view this frame (round E4-rest2: placing).
    pub dropped: bool,
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

/// The Gameplay workspace's tool (round E4): a click adds a flag, a spawn point or an M-COM where the pointer meets
/// the ground (one operation each, [`crate::gameplay::place_op`]), or picks up a flag, base, spawner or M-COM and puts
/// it down at the next click with what moves with it ([`crate::gameplay::carry_op`]); what it does is chosen in the
/// Gameplay panel ([`GameplayKind`]). The team is the asset browser's team picker's.
#[derive(Debug, Default)]
pub struct GameplayTool {
    /// What a click does.
    pub kind: GameplayKind,
    /// The object being carried: its guid, its index and where it stood.
    pub carrying: Option<(String, usize, V3)>,
    /// Where the pointer meets the ground (this frame's), for the ghost.
    hover: Option<(Projection, V3)>,
}

impl GameplayTool {
    /// The entity whose marker is nearest a screen point (within 12 points), else the one the ray meets.
    fn pick(pointer: &Pointer, at: Pos2, v: &crate::app::ViewState) -> Option<usize> {
        let mut best: Option<(f32, usize)> = None;
        for e in &v.scene().entities {
            if v.doc.as_ref().is_some_and(|d| !d.shown(e.index)) || !v.selectable(e.index) {
                continue;
            }
            let Some(p) = e.position() else { continue };
            let off = v.marker_offset(e.index);
            let Some(s) = pointer.view.project([p[0], p[1] + off, p[2]]) else {
                continue;
            };
            let d = (pointer.rect.min + Vec2::new(s[0] as f32, s[1] as f32)).distance(at);
            if d < 12.0 && best.is_none_or(|(b, _)| d < b) {
                best = Some((d, e.index));
            }
        }
        best.map(|(_, k)| k).or_else(|| {
            let (o, dir) = pointer.view.ray(local(pointer.rect, at));
            let skip = |k: usize| v.doc.as_ref().is_some_and(|d| !d.shown(k));
            query::ray_hit_except(v.scene(), o, dir, &skip)
                .and_then(|h| h.entity)
                .filter(|&k| v.selectable(k))
        })
    }

    /// Where the ray under a screen point meets the ground or an object's top (the carried object left out).
    fn ground(&self, pointer: &Pointer, at: Pos2, v: &crate::app::ViewState) -> Option<V3> {
        let (o, dir) = pointer.view.ray(local(pointer.rect, at));
        let carried = self.carrying.as_ref().map(|c| c.1);
        let skip = |k: usize| v.doc.as_ref().is_some_and(|d| !d.shown(k)) || carried == Some(k);
        query::ray_hit_except(v.scene(), o, dir, &skip).map(|h| h.point)
    }
}

/// A screen point in a view's own pixels.
fn local(rect: Rect, p: Pos2) -> [f64; 2] {
    [f64::from(p.x - rect.min.x), f64::from(p.y - rect.min.y)]
}

impl ViewTool for GameplayTool {
    fn id(&self) -> Tool {
        Tool::Gameplay
    }

    fn boxes(&self) -> bool {
        false
    }

    fn input(&mut self, pointer: &Pointer, v: &crate::app::ViewState) -> Option<Outcome> {
        if !pointer.editing {
            return None;
        }
        self.hover = pointer
            .hover
            .and_then(|h| self.ground(pointer, h, v))
            .map(|p| (pointer.projection, p));
        if !pointer.clicked {
            return None;
        }
        let at = pointer.at?;
        let doc = v.doc.as_ref()?;
        if self.kind == GameplayKind::Carry {
            if let Some((g, _, _)) = self.carrying.clone() {
                let Some(to) = self.ground(pointer, at, v) else {
                    return Some(Outcome::Message("Nothing under the pointer to put it on".into()));
                };
                self.carrying = None;
                return Some(match crate::gameplay::carry_op(doc, &g, to) {
                    Some(op) => Outcome::Commit(Box::new(op), "Carry".into()),
                    None => Outcome::Message(format!("{g} has no position to carry")),
                });
            }
            let Some(k) = Self::pick(pointer, at, v) else {
                return Some(Outcome::Message(
                    "Click a flag, base, spawner or M-COM to carry it".into(),
                ));
            };
            if !crate::gameplay::is_unit_root(doc, k) {
                return Some(Outcome::Message(
                    "Only flags, bases, spawners, spawn points and M-COMs are carried; the Move tool moves the rest"
                        .into(),
                ));
            }
            let (Some(g), Some(p)) = (
                doc.guid(k),
                v.scene().entities.get(k).and_then(bc2_scene::scene::Entity::position),
            ) else {
                return Some(Outcome::Done);
            };
            self.carrying = Some((g.to_owned(), k, p));
            return Some(Outcome::Select(
                vec![k],
                "Carrying it: click where to put it down (Esc drops it)".into(),
            ));
        }
        let Some(p) = self.ground(pointer, at, v) else {
            return Some(Outcome::Message("Nothing under the pointer to place on".into()));
        };
        let team = crate::gameplay::team_of_picker(v.browser.place_team);
        Some(match crate::gameplay::place_op(doc, self.kind, p, team, &v.selection) {
            Ok((op, label, guid)) => Outcome::CommitSelect(Box::new(op), label, vec![guid]),
            Err(why) => Outcome::Message(format!("Not added: {why}")),
        })
    }

    fn draw(&self, d: &Draw<'_>, layer: Layer) {
        if layer != Layer::Tools {
            return;
        }
        let Some((_, w)) = self.hover.filter(|(p, _)| *p == d.projection) else {
            return;
        };
        let Some(q) = d.to_screen(w) else { return };
        let p = d.palette;
        let glyph = match self.kind {
            GameplayKind::Flag => "F",
            GameplayKind::SpawnPoint => "S",
            GameplayKind::Mcom(_) => "M",
            GameplayKind::Spawner => "P",
            GameplayKind::Stage | GameplayKind::InsertStage(_) => "R",
            GameplayKind::Carry => "",
        };
        if let Some((_, _, from)) = &self.carrying
            && let Some(a) = d.to_screen(*from)
        {
            d.painter.line_segment([a, q], Stroke::new(1.5, p.view_selection));
            d.painter.circle_stroke(q, 7.0, Stroke::new(2.0, p.view_selection));
        } else if !glyph.is_empty() {
            d.painter.circle_stroke(q, 9.0, Stroke::new(2.0, p.view_selection));
            let g = d
                .painter
                .layout_no_wrap(glyph.to_owned(), FontId::monospace(11.0), p.view_selection);
            d.painter.galley(q - g.size() / 2.0, g, p.view_selection);
        }
    }

    fn transient(&self) -> bool {
        self.carrying.is_some()
    }

    fn cancel(&mut self) {
        self.carrying = None;
    }

    fn left(&mut self) {
        self.carrying = None;
        self.hover = None;
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
    /// The Gameplay workspace's tool (round E4).
    pub gameplay: GameplayTool,
    /// The Terrain workspace's brushes (round E5).
    pub terrain: crate::sculpt::TerrainTool,
    /// The AI workspace's navigation edit tools (round E4-rest2).
    pub nav: crate::navtool::NavDrawTool,
    /// Placing from the content browser (round E4-rest2).
    pub place: crate::placetool::PlaceTool,
}

impl Default for Tools {
    fn default() -> Self {
        Self {
            select: SelectTool,
            moving: GizmoTool { rotate: false },
            rotating: GizmoTool { rotate: true },
            ruler: RulerTool::default(),
            polygon: PolygonTool::default(),
            gameplay: GameplayTool::default(),
            terrain: crate::sculpt::TerrainTool::default(),
            nav: crate::navtool::NavDrawTool::default(),
            place: crate::placetool::PlaceTool::default(),
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
            Tool::Gameplay => &self.gameplay,
            Tool::Terrain => &self.terrain,
            Tool::Nav => &self.nav,
            Tool::Place => &self.place,
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
            Tool::Gameplay => &mut self.gameplay,
            Tool::Terrain => &mut self.terrain,
            Tool::Nav => &mut self.nav,
            Tool::Place => &mut self.place,
        }
    }

    /// Every tool, in the toolbar's order.
    #[must_use]
    pub fn all(&self) -> [&dyn ViewTool; 9] {
        [
            &self.select,
            &self.moving,
            &self.rotating,
            &self.ruler,
            &self.polygon,
            &self.gameplay,
            &self.terrain,
            &self.nav,
            &self.place,
        ]
    }

    /// Every tool, to tell each of them something.
    pub fn all_mut(&mut self) -> [&mut dyn ViewTool; 9] {
        [
            &mut self.select,
            &mut self.moving,
            &mut self.rotating,
            &mut self.ruler,
            &mut self.polygon,
            &mut self.gameplay,
            &mut self.terrain,
            &mut self.nav,
            &mut self.place,
        ]
    }

    /// The entity the gameplay tool carries, if it carries one.
    #[must_use]
    pub fn gameplay_carried(&self) -> Option<usize> {
        self.gameplay.carrying.as_ref().map(|c| c.1)
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
        for t in [
            Tool::Select,
            Tool::Move,
            Tool::Rotate,
            Tool::Ruler,
            Tool::Polygon,
            Tool::Gameplay,
            Tool::Terrain,
            Tool::Nav,
            Tool::Place,
        ] {
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
