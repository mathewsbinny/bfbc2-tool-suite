//! The AI workspace's nine navigation edit tools as one view tool (round E4-rest2, PR1b-0's leftover; audit AU5
//! ARC-6): [`NavDrawTool`] holds the chosen [`NavTool`], the drawing's clicks and the panel's settings for it (a
//! region's outline or box and its height range, a traversal's kind, two-way, a note's text, who the edits are
//! recorded as), and implements [`ViewTool`] like the object tools: chosen as [`Tool::Nav`], its clicks through
//! [`ViewTool::click`] (the point is the navigation ground under the pointer: the views hand it the surface the
//! picture shows there, else the navigation ground along the ray, else a marker's object), Enter through
//! [`ViewTool::takes_enter`] (a region's outline), Esc through [`ViewTool::cancel`], and another tool chosen through
//! [`ViewTool::left`]. A complete drawing is an [`Outcome::NavDrawing`] with its edit; the editor checks it against
//! the base (`nav::problems`), commits it (`nav-add`) and redraws the navigation's lines (the clicks are drawn with
//! them, as before).
//!
//! What it did before (`NavState::tool`, `clicks` and the settings beside the navigation's data; `nav_click` and
//! `nav_overlay` in the views) is the same: a click is the tool's only while the navigation is drawn in the AI
//! workspace with a project open ([`NavDrawTool::live`], refreshed by [`ViewTool::input`] every frame), else it
//! selects.

use crate::gizmo::Tool;
use crate::math::V3;
use crate::nav::{self, NavTool};
use crate::query::Hit;
use crate::tool::{Outcome, Pointer, ViewTool};

/// The navigation edit tools' state (see the module).
#[derive(Debug, Clone)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "the panel's settings and the frame's state, each its own question"
)]
pub struct NavDrawTool {
    /// The edit tool chosen (while [`Tool::Nav`] is the views' tool).
    pub kind: Option<NavTool>,
    /// Regions are drawn as polygons (else boxes).
    pub polygon: bool,
    /// The drawing's clicks.
    pub clicks: Vec<[f64; 3]>,
    /// A region's height range below and above the clicked points.
    pub margin: [f64; 2],
    /// The traversal kind (index into `navtune::TRAVERSALS`).
    pub traversal: usize,
    /// Route edges and traversals both ways.
    pub two_way: bool,
    /// A note's text.
    pub note: String,
    /// Who the edits are recorded as.
    pub by: String,
    /// Clicks are the tool's this frame: the navigation is drawn in the AI workspace with a project open.
    pub live: bool,
    /// Enter was pressed: a region's outline closes at the next input.
    close: bool,
}

impl Default for NavDrawTool {
    /// The Studio's settings: polygons, 2 m below and 4 m above, two-way, the PC's name.
    fn default() -> Self {
        Self {
            kind: None,
            polygon: true,
            clicks: Vec::new(),
            margin: [2.0, 4.0],
            traversal: 0,
            two_way: true,
            note: String::new(),
            by: author(),
            live: false,
            close: false,
        }
    }
}

/// The PC's name, read once (the views' tools are made again every frame while one has the input).
fn author() -> String {
    static NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    NAME.get_or_init(bc2_levelwrite::project::author).clone()
}

impl NavDrawTool {
    /// A point added to the drawing (a click, or the context menu's point): the edit when the drawing is complete
    /// (`finish`: a region's outline closed by Enter).
    #[must_use]
    pub fn add(&mut self, p: Option<V3>, finish: bool) -> Outcome {
        let Some(kind) = self.kind else {
            return Outcome::Done;
        };
        if let Some(p) = p {
            self.clicks.push(p);
        }
        self.drawn(kind, finish)
    }

    /// The edit of the drawing when it is complete; the drawing is then cleared.
    fn drawn(&mut self, kind: NavTool, finish: bool) -> Outcome {
        let Some(edit) = nav::edit_of(
            kind,
            &self.clicks,
            self.polygon,
            finish,
            self.margin,
            self.traversal,
            self.two_way,
            &self.note,
            &self.by,
        ) else {
            if kind == NavTool::Note && self.note.trim().is_empty() && !self.clicks.is_empty() {
                self.clicks.clear();
                return Outcome::NavDrawing(None, Some("Type the note first".into()));
            }
            return Outcome::NavDrawing(None, None);
        };
        self.clicks.clear();
        Outcome::NavDrawing(Some(Box::new(edit)), None)
    }

    /// Whether a region's outline is being drawn (Enter closes it at three corners).
    #[must_use]
    pub fn outlining(&self) -> bool {
        self.polygon && matches!(self.kind, Some(NavTool::Walkable | NavTool::Unwalkable))
    }
}

impl ViewTool for NavDrawTool {
    fn id(&self) -> Tool {
        Tool::Nav
    }

    fn input(&mut self, _pointer: &Pointer, v: &crate::app::ViewState) -> Option<Outcome> {
        self.live = v.nav_active() && v.workspace == crate::workspace::Workspace::Ai && v.doc.is_some();
        if std::mem::take(&mut self.close)
            && self.live
            && let Some(kind) = self.kind
        {
            return Some(self.drawn(kind, true));
        }
        None
    }

    fn click(&mut self, hit: Option<Hit>, _snap_vertex: bool) -> Option<Outcome> {
        if !self.live || self.kind.is_none() {
            return None;
        }
        Some(self.add(hit.map(|h| h.point), false))
    }

    fn takes_enter(&self) -> bool {
        self.live && self.outlining() && self.clicks.len() >= 3
    }

    fn enter(&mut self) {
        self.close = true;
    }

    fn transient(&self) -> bool {
        !self.clicks.is_empty()
    }

    fn cancel(&mut self) {
        self.clicks.clear();
    }

    fn left(&mut self) {
        self.kind = None;
        self.clicks.clear();
        self.close = false;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_drawing_makes_its_edit_and_is_cleared() {
        let mut t = NavDrawTool {
            kind: Some(NavTool::EdgeAdd),
            live: true,
            ..NavDrawTool::default()
        };
        let hit = |x: f64| Hit {
            t: 1.0,
            point: [x, 0.0, 0.0],
            normal: [0.0, 1.0, 0.0],
            entity: None,
            tri: [[x, 0.0, 0.0], [x + 1.0, 0.0, 0.0], [x, 0.0, 1.0]],
        };
        assert_eq!(t.click(Some(hit(1.0)), false), Some(Outcome::NavDrawing(None, None)));
        assert!(t.transient());
        let Some(Outcome::NavDrawing(Some(e), None)) = t.click(Some(hit(5.0)), false) else {
            panic!("an edge")
        };
        assert_eq!(e.op, "route-edge");
        assert_eq!(t.clicks.len(), 0);
        // a note needs its text
        t.kind = Some(NavTool::Note);
        assert_eq!(
            t.click(Some(hit(1.0)), false),
            Some(Outcome::NavDrawing(None, Some("Type the note first".into())))
        );
        // not live (the navigation not drawn, or not the AI workspace): the click selects
        t.live = false;
        assert_eq!(t.click(Some(hit(1.0)), false), None);
        // a region's outline: Enter at three corners, closed at the next input
        t.live = true;
        t.kind = Some(NavTool::Walkable);
        for x in [0.0, 10.0, 20.0] {
            let _ = t.click(Some(hit(x)), false);
        }
        assert!(t.takes_enter());
        t.left();
        assert!(t.kind.is_none() && t.clicks.is_empty() && !t.transient());
    }
}
