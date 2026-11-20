//! Round E2b's editing tools (the user's "a Windows context menu type thing", the edit modes, "the scroll wheel on
//! the mouse for [sink], along with a popup prompt"):
//!
//! - **Context menus**: right-click an object (or the selection) or the ground in a view, or an outliner row. The
//!   entries come from the action registry ([`menu_items`]: names, shortcuts) and depend on the workspace; a
//!   disabled entry says why in its tooltip.
//! - **The sink popup**: Alt + wheel over a view (0.1 m a notch, 0.01 m with Shift) or "Sink below surface..." sinks
//!   the selection into the surface below it; a floating card beside the pointer shows the depth in metres and as a
//!   share of the object's height, a slider and the mode (straight down, along the normal, aligned to the normal).
//!   A wheel burst, a slider drag or a mode click is one operation ([`Doc::amend`] keeps adding to it).
//! - **Placing**: from the content browser (drag an entry into a view) or "Place object here..." (a searchable
//!   picker); the object follows the pointer on the surface with its sink (Alt + wheel), Shift + wheel turns it
//!   15 degrees a notch (1 degree with Ctrl), a click places it (one `place` operation), Esc cancels.
//! - **Polygon selection**, **the workspaces' stubs** (Terrain, Vegetation), **the view modes** and **several objects'
//!   fields at once** ("Mixed" where they differ).

#[allow(
    clippy::wildcard_imports,
    reason = "a part of app.rs in a file of its own: its types and helpers"
)]
use super::*;

use bc2_levelwrite::Operation;
use bc2_levelwrite::ops::{FieldIs, Placed};

use crate::actions::Brush;
use crate::content::Entry;
use crate::viewmodes;
use crate::workspace::{Category, ViewMode};

/// What a right-click was on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContextTarget {
    /// The object under the pointer (`None`: the ground or the sky).
    pub entity: Option<usize>,
    /// The surface point under the pointer.
    pub point: Option<V3>,
    /// Where the menu opened (screen).
    pub at: Pos2,
}

/// Which context menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKind {
    /// On an object or the selection (views and outliner).
    Object,
    /// On the ground.
    Ground,
}

/// One entry of a context menu.
#[derive(Debug, Clone, PartialEq)]
pub struct MenuItem {
    /// The action it runs.
    pub action: Action,
    /// Its label.
    pub name: String,
    /// Why it cannot run now (shown disabled with this tooltip).
    pub why: Option<&'static str>,
}

/// An object being placed: it follows the pointer until a click places it.
#[derive(Debug, Clone)]
pub struct Placing {
    /// The blueprint.
    pub entry: Entry,
    /// Its turn about the up axis, degrees.
    pub yaw: f64,
    /// How it sits on the surface.
    pub placement: Placement,
    /// Where it is now (the pointer's surface point), if the pointer is over a surface.
    pub at: Option<M>,
    /// Its extent along its up axis (from its collision), and its half width and depth for the preview box.
    pub extent: (f64, f64),
    /// Half its width and depth (the preview box).
    pub half: (f64, f64),
    /// A spawner or objective of the Spawners and Objectives modes (`entry` is then its stand-in).
    pub gameplay: Option<crate::gameplay::Tile>,
}

/// A Spawners or Objectives tile being dragged (its key).
#[derive(Debug, Clone)]
pub struct GameplayDrag(pub String);

/// What a gesture of the sink popup is (each is one operation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Gesture {
    Wheel,
    Slider,
    Mode,
}

/// The sink popup.
#[derive(Debug, Clone)]
pub struct SinkPopup {
    /// Where it shows (screen).
    pub at: Pos2,
    /// The objects and their transforms when it opened.
    pub objects: Vec<(usize, M)>,
    /// The sink (metres) and mode.
    pub placement: Placement,
    /// The primary object's height (the depth's share of it).
    pub height: f64,
    /// The gesture under way: its kind, its operation's number, when its last input came (seconds).
    gesture: Option<(Gesture, u64, f64)>,
}

/// The "Place object here..." picker.
#[derive(Debug, Clone)]
pub struct Picker {
    /// Where it shows (screen).
    pub at: Pos2,
    /// The surface point to place at.
    pub point: Option<V3>,
    /// The search.
    pub query: String,
}

/// A content browser entry being dragged.
#[derive(Debug, Clone)]
pub struct ContentDrag(pub String);

/// A wheel burst ends when no notch comes for this long (seconds).
const BURST: f64 = 0.6;

/// A gameplay entity's team field and its text for team `t`.
fn team_field(e: &scene::Entity, t: u8) -> (&'static str, &'static str) {
    let field = if e.category == "capture point" {
        "InitialOwnerTeam"
    } else {
        "Team"
    };
    let value = match t {
        1 => "Team1",
        2 => "Team2",
        _ => "TeamNeutral",
    };
    (field, value)
}

/// The scene category a placed blueprint takes (as the loader names placed objects).
fn scene_category(e: &Entry) -> &'static str {
    match e.object_type.as_deref() {
        _ if e.destructible => "destructible",
        Some("LadderEntityData") => "ladder",
        Some("DynamicModelEntityData" | "RigidModelEntityData" | "BangerEntityData") => "dynamic object",
        Some("EffectEntityData") => "effect",
        _ => "placed object",
    }
}

/// Whether a screen point is inside a polygon (even-odd).
fn inside(poly: &[Pos2], p: Pos2) -> bool {
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

/// The context menu's entries for a workspace (separators as `None`), with why each cannot run.
#[must_use]
pub fn menu_items(v: &ViewState, kind: MenuKind, target: Option<usize>) -> Vec<Option<MenuItem>> {
    use Action as A;
    let ws = v.workspace;
    let editing = v.doc.is_some() && ws.moves_objects();
    let actions: Vec<Option<A>> = match (kind, ws) {
        (MenuKind::Object, Workspace::Objects) => vec![
            Some(A::UseTool(Tool::Move)),
            Some(A::UseTool(Tool::Rotate)),
            Some(A::PlaceOnSurface),
            Some(A::SinkPopup),
            None,
            Some(A::Duplicate),
            Some(A::DeepDuplicate),
            Some(A::Copy),
            Some(A::Delete),
            None,
            Some(A::Group),
            Some(A::Ungroup),
            Some(A::Hide),
            Some(A::Lock),
            None,
            Some(A::SelectAllOfType),
            Some(A::FocusSelection),
            Some(A::ShowPanel(PanelKind::Properties)),
            Some(A::ShowPanel(PanelKind::Links)),
            Some(A::CopyGuid),
        ],
        (MenuKind::Object, Workspace::Gameplay) => vec![
            Some(A::UseTool(Tool::Move)),
            Some(A::UseTool(Tool::Rotate)),
            Some(A::PlaceOnSurface),
            Some(A::SinkPopup),
            None,
            Some(A::SetTeam(1)),
            Some(A::SetTeam(2)),
            Some(A::SetTeam(0)),
            None,
            Some(A::Duplicate),
            Some(A::DeepDuplicate),
            Some(A::Delete),
            Some(A::Hide),
            Some(A::Lock),
            None,
            Some(A::FocusSelection),
            Some(A::ShowPanel(PanelKind::Properties)),
            Some(A::ShowPanel(PanelKind::Links)),
            Some(A::CopyGuid),
        ],
        (MenuKind::Object, Workspace::Ai) => vec![
            Some(A::UseTool(Tool::Move)),
            Some(A::UseTool(Tool::Rotate)),
            Some(A::PlaceOnSurface),
            None,
            Some(A::Duplicate),
            Some(A::Delete),
            Some(A::Hide),
            Some(A::Lock),
            None,
            Some(A::SetViewMode(ViewMode::Navmesh)),
            Some(A::PathTest),
            None,
            Some(A::FocusSelection),
            Some(A::ShowPanel(PanelKind::Properties)),
            Some(A::ShowPanel(PanelKind::Links)),
            Some(A::CopyGuid),
        ],
        (MenuKind::Object, _) => vec![
            Some(A::SelectAllOfType),
            Some(A::FocusSelection),
            Some(A::ShowPanel(PanelKind::Properties)),
            Some(A::ShowPanel(PanelKind::Links)),
            Some(A::CopyGuid),
        ],
        (MenuKind::Ground, Workspace::Objects) => vec![
            Some(A::PlaceObject),
            Some(A::Paste),
            None,
            Some(A::SelectAllOfType),
            Some(A::FocusHere),
        ],
        (MenuKind::Ground, Workspace::Gameplay) => vec![Some(A::Paste), None, Some(A::FocusHere)],
        (MenuKind::Ground, Workspace::Ai) => vec![
            Some(A::SetViewMode(ViewMode::Navmesh)),
            Some(A::PathTest),
            None,
            Some(A::FocusHere),
        ],
        (MenuKind::Ground, Workspace::Terrain) => Brush::ALL
            .iter()
            .filter(|b| b.workspace() == Workspace::Terrain)
            .map(|b| Some(A::Brush(*b)))
            .chain([None, Some(A::FocusHere)])
            .collect(),
        (MenuKind::Ground, Workspace::Vegetation) => Brush::ALL
            .iter()
            .filter(|b| b.workspace() == Workspace::Vegetation)
            .map(|b| Some(A::Brush(*b)))
            .chain([None, Some(A::FocusHere)])
            .collect(),
        (MenuKind::Ground, Workspace::View) => vec![Some(A::FocusHere)],
    };
    // an object that cannot be edited says why on its editing entries (a static group member ...)
    let object_why = target.and_then(|k| v.doc.as_ref().and_then(|d| d.why_not(k)));
    let edits = |a: A| {
        matches!(
            a,
            A::UseTool(Tool::Move | Tool::Rotate)
                | A::PlaceOnSurface
                | A::SinkPopup
                | A::Duplicate
                | A::DeepDuplicate
                | A::Copy
                | A::Delete
                | A::Group
                | A::Hide
                | A::Lock
                | A::SetTeam(_)
        )
    };
    actions
        .into_iter()
        .map(|a| {
            let a = a?;
            let entry = actions::entry(a);
            let name = match a {
                A::UseTool(Tool::Move) => "Move".to_owned(),
                A::UseTool(Tool::Rotate) => "Rotate".to_owned(),
                A::ShowPanel(PanelKind::Properties) => "Properties".to_owned(),
                A::ShowPanel(PanelKind::Links) => "Links".to_owned(),
                A::FocusSelection => "Focus".to_owned(),
                A::SetViewMode(ViewMode::Navmesh) => "Show the navmesh".to_owned(),
                _ => entry.map_or_else(String::new, |e| e.name.to_owned()),
            };
            let why = why_not_in(Some(v), a).or_else(|| {
                (edits(a) && editing)
                    .then_some(object_why)
                    .flatten()
                    .filter(|_| v.editable_selection().is_empty())
            });
            Some(MenuItem { action: a, name, why })
        })
        .collect()
}

impl ViewState {
    /// The surface point under the pointer, or where the context menu was opened (taken once).
    pub(super) fn point_here(&mut self) -> Option<V3> {
        if let Some(p) = self.menu_point.take() {
            return Some(p);
        }
        let (o, d) = self.pointer_ray?;
        let skip = |k: usize| self.doc.as_ref().is_some_and(|d| !d.shown(k));
        query::ray_hit_except(self.scene(), o, d, &skip).map(|h| h.point)
    }

    /// Whether a transient tool is open (placing, the polygon, the sink popup, the picker): Esc cancels it first.
    #[must_use]
    pub fn has_transient(&self) -> bool {
        self.placing.is_some() || !self.polygon.is_empty() || self.sink.is_some() || self.picker.is_some()
    }

    /// Cancels the transient tools.
    pub fn cancel_transient(&mut self) {
        if self.placing.take().is_some() {
            self.message = "Placing cancelled".into();
        }
        self.polygon.clear();
        self.sink = None;
        self.picker = None;
    }

    /// Whether an entity can be selected in the views now: shown, on an unlocked layer, and the workspace's (with
    /// the category filter).
    #[must_use]
    pub fn selectable(&self, k: usize) -> bool {
        let Some(e) = self.scene().entities.get(k) else {
            return false;
        };
        let ok_doc = self
            .doc
            .as_ref()
            .is_none_or(|d| d.shown(k) && d.guid(k).is_some_and(|g| d.org.editable(g)));
        ok_doc && self.workspace.selects(e, self.filter)
    }

    /// The editor view's look again (after a mode change, an edit, a theme change): the mode, the cull-tree
    /// colours and the overlay lines; loads what the mode needs the first time.
    pub(super) fn apply_look(&mut self, palette: &Palette) {
        let mode = self.view_mode;
        let mut look = render::Look {
            mode,
            ..render::Look::default()
        };
        match mode {
            ViewMode::Navmesh => {
                if self.nav.is_none() {
                    let file = self.root.as_deref().and_then(|r| viewmodes::pathdata_file(r, &self.lv));
                    self.nav = Some(match file {
                        Some(f) => viewmodes::navmesh_outlines(&f).map(|l| (f.display().to_string(), l)),
                        None => Err(format!(
                            "{} has no navigation data (no pathdata.hpd installed or built by NavKit)",
                            self.lv.to_uppercase()
                        )),
                    });
                }
                match &self.nav {
                    Some(Ok((file, lines))) => {
                        look.overlay = lines.iter().map(|s| (*s, palette.status_ok)).collect();
                        self.mode_note = format!("Navmesh: {} outline segments from {file}", lines.len());
                    }
                    Some(Err(e)) => self.mode_note = format!("Navmesh: {e}"),
                    None => {}
                }
            }
            ViewMode::CullTree => {
                if self.cull.is_none() {
                    self.cull = Some(viewmodes::cull_view(self.scene(), &self.bases));
                }
                if let Some(c) = &self.cull {
                    for &k in &c.unslotted {
                        look.tint.insert(k, palette.status_warn);
                    }
                    for &k in &c.shared {
                        look.tint.insert(k, palette.status_error);
                    }
                    look.overlay = c
                        .leaves
                        .iter()
                        .take(4000)
                        .flat_map(|s| viewmodes::sphere_lines(*s))
                        .map(|s| (s, palette.status_info))
                        .collect();
                    self.mode_note = format!("Cull tree: {} (orange: no slot, red: a shared slot)", c.source);
                }
            }
            ViewMode::Collision => {
                self.mode_note = "Collision: terrain and the objects' Havok shapes, a colour per blueprint".into();
            }
            _ => self.mode_note.clear(),
        }
        self.gpu_scene.look = look;
    }

    /// The tools' actions; `false` for actions that are not theirs.
    pub(super) fn tool_action(&mut self, gpu: &GpuCtx, palette: &Palette, action: Action) -> bool {
        let sel = self.editable_selection();
        match action {
            Action::SetWorkspace(w) => {
                self.workspace = w;
                self.drag = None;
                self.box_from = None;
                self.cancel_transient();
                let keep: Vec<usize> = self.selection.iter().copied().filter(|&k| self.selectable(k)).collect();
                self.set_selection(gpu, keep, false);
                self.message = format!("{} workspace", w.name());
            }
            Action::SetViewMode(m) => {
                self.view_mode = m;
                self.apply_look(palette);
                self.refresh(gpu, palette);
                self.message = format!("{} view", m.name());
            }
            Action::DeepDuplicate => {
                let Some(d) = &self.doc else { return true };
                let offset = if self.snap.grid {
                    self.snap.grid_size.max(1.0)
                } else {
                    2.0
                };
                // an object another selected one owns is copied as that one's sub-object
                let owned: Vec<(usize, Vec<String>)> = sel.iter().map(|&k| (k, d.owned(k))).collect();
                let all_owned: Vec<&String> = owned.iter().flat_map(|(_, o)| o.iter()).collect();
                let mut items = Vec::new();
                for (k, subs) in &owned {
                    let Some(g) = d.guid(*k) else { continue };
                    if all_owned.iter().any(|o| o.eq_ignore_ascii_case(g)) {
                        continue;
                    }
                    let Some(m) = self.matrix(*k) else { continue };
                    let owned = subs
                        .iter()
                        .map(|s| bc2_levelwrite::ops::SubCopy {
                            source: s.clone(),
                            guid: d.project.new_guid(s),
                            transform: d
                                .index(s)
                                .and_then(|x| self.matrix(x))
                                .map(|sm| bc2_levelwrite::rows_of(&math::translate(&sm, [offset, 0.0, 0.0]))),
                        })
                        .collect();
                    items.push(bc2_levelwrite::ops::Duplicated {
                        source: g.to_owned(),
                        guid: d.project.new_guid(g),
                        transform: bc2_levelwrite::rows_of(&math::translate(&m, [offset, 0.0, 0.0])),
                        owned,
                    });
                }
                let copies: Vec<String> = items.iter().map(|i| i.guid.clone()).collect();
                self.commit(gpu, palette, Op::Duplicate { items }, "");
                if let Some(d) = &self.doc {
                    let ks: Vec<usize> = copies.iter().filter_map(|g| d.index(g)).collect();
                    self.set_selection(gpu, ks, true);
                }
            }
            Action::Hide | Action::Lock => {
                let Some(d) = &self.doc else { return true };
                let guids: Vec<String> = sel.iter().filter_map(|&k| d.guid(k).map(str::to_owned)).collect();
                let op = if action == Action::Hide {
                    Op::Hide { guids, hidden: true }
                } else {
                    Op::Lock { guids, locked: true }
                };
                self.selection.clear();
                self.commit(gpu, palette, op, "");
            }
            Action::ShowHidden => {
                let Some(d) = &self.doc else { return true };
                let guids: Vec<String> = d.org.hidden.iter().cloned().collect();
                self.commit(gpu, palette, Op::Hide { guids, hidden: false }, "");
            }
            Action::UnlockAll => {
                let Some(d) = &self.doc else { return true };
                let guids: Vec<String> = d.org.locked.iter().cloned().collect();
                self.commit(gpu, palette, Op::Lock { guids, locked: false }, "");
            }
            Action::Copy => {
                let Some(d) = &self.doc else { return true };
                self.clipboard = sel.iter().filter_map(|&k| d.guid(k).map(str::to_owned)).collect();
                self.message = format!("Copied {} object(s)", self.clipboard.len());
            }
            Action::Paste => {
                let at = self.point_here();
                let Some(d) = &self.doc else { return true };
                let ms: Vec<(String, M)> = self
                    .clipboard
                    .iter()
                    .filter_map(|g| Some((g.clone(), self.matrix(d.index(g)?)?)))
                    .collect();
                if ms.is_empty() {
                    return true;
                }
                let n = ms.len() as f64;
                let c = ms.iter().fold([0.0; 3], |a, (_, m)| math::add(a, math::row(m, 3)));
                let centre = math::mul(c, 1.0 / n);
                let shift = match at {
                    Some(p) => math::sub(
                        p,
                        [
                            centre[0],
                            ms.iter().map(|(_, m)| m[13]).fold(f64::INFINITY, f64::min),
                            centre[2],
                        ],
                    ),
                    None => [2.0, 0.0, 2.0],
                };
                let items: Vec<bc2_levelwrite::ops::Duplicated> = ms
                    .iter()
                    .map(|(g, m)| bc2_levelwrite::ops::Duplicated {
                        source: g.clone(),
                        guid: d.project.new_guid(g),
                        transform: bc2_levelwrite::rows_of(&math::translate(m, shift)),
                        owned: Vec::new(),
                    })
                    .collect();
                let copies: Vec<String> = items.iter().map(|i| i.guid.clone()).collect();
                self.commit(gpu, palette, Op::Duplicate { items }, "Paste");
                if let Some(d) = &self.doc {
                    let ks: Vec<usize> = copies.iter().filter_map(|g| d.index(g)).collect();
                    self.set_selection(gpu, ks, true);
                }
            }
            Action::SelectAllOfType => {
                let want_bp = self.content_pick.clone().or_else(|| {
                    self.selected
                        .and_then(|k| self.scene().entities.get(k))
                        .and_then(|e| e.blueprint.clone())
                });
                let want_type = self
                    .selected
                    .and_then(|k| self.scene().entities.get(k))
                    .map(|e| e.type_name.clone());
                let list: Vec<usize> = self
                    .scene()
                    .entities
                    .iter()
                    .filter(|e| match (&want_bp, &want_type) {
                        (Some(bp), _) => e.blueprint.as_deref() == Some(bp.as_str()),
                        (None, Some(t)) => e.type_name == *t,
                        (None, None) => false,
                    })
                    .map(|e| e.index)
                    .filter(|&k| self.selectable(k) && self.doc.as_ref().is_none_or(|d| !d.deleted.contains(&k)))
                    .collect();
                self.message = format!("{} selected", list.len());
                self.set_selection(gpu, list, true);
            }
            Action::FocusHere => {
                if let Some(p) = self.point_here() {
                    for vp in self.viewports.values_mut() {
                        vp.camera.focus(p, None);
                        vp.dirty = true;
                    }
                }
            }
            Action::SetTeam(t) => {
                let Some(d) = &self.doc else { return true };
                let items: Vec<FieldSet> = sel
                    .iter()
                    .filter_map(|&k| {
                        let e = d.scene.entities.get(k)?;
                        e.team?;
                        let (field, value) = team_field(e, t);
                        let row = d.fields(k).into_iter().find(|r| r.path == field)?;
                        (row.value != Some(FieldValue::Text(value.into()))).then(|| FieldSet {
                            guid: e.guid.clone(),
                            field: field.into(),
                            from: row.value,
                            to: FieldValue::Text(value.into()),
                        })
                    })
                    .collect();
                if !items.is_empty() {
                    self.commit(gpu, palette, Op::SetField { items }, "");
                }
            }
            Action::SinkPopup => {
                let at = self
                    .context
                    .map(|c| c.at)
                    .or_else(|| self.view_rect().map(|r| r.center()))
                    .unwrap_or_default();
                self.open_sink(at);
            }
            Action::PlaceObject => {
                let point = self.point_here();
                let at = self
                    .context
                    .map(|c| c.at)
                    .or_else(|| self.view_rect().map(|r| r.center()))
                    .unwrap_or_default();
                self.picker = Some(Picker {
                    at,
                    point,
                    query: String::new(),
                });
            }
            _ => return false,
        }
        self.redraw();
        true
    }

    /// Opens the sink popup at `at` for the editable selection.
    pub fn open_sink(&mut self, at: Pos2) {
        let objects: Vec<(usize, M)> = self
            .editable_selection()
            .into_iter()
            .filter_map(|k| Some((k, self.matrix(k)?)))
            .collect();
        let Some(&(primary, _)) = objects.last() else { return };
        let height = query::height(self.scene(), primary);
        let mut placement = self.placement_for(primary);
        if placement.unit == SinkUnit::Fraction {
            placement.sink *= height;
            placement.unit = SinkUnit::Metres;
        }
        self.sink = Some(SinkPopup {
            at,
            objects,
            placement,
            height,
            gesture: None,
        });
    }

    /// Applies the sink popup's placement to its objects: one operation per gesture (a wheel burst, a slider drag,
    /// a mode click), amended while the gesture goes on.
    fn sink_to(&mut self, gpu: &GpuCtx, palette: &Palette, kind: Gesture, now: f64) {
        let Some(popup) = self.sink.clone() else { return };
        let ks: Vec<usize> = popup.objects.iter().map(|(k, _)| *k).collect();
        let placed = self.placed_on_surface(&ks, Some(popup.placement));
        let Some(d) = self.doc.as_mut() else { return };
        let items: Vec<Moved> = placed
            .iter()
            .filter_map(|(k, _, to, p)| {
                let from = popup.objects.iter().find(|(x, _)| x == k).map(|(_, m)| *m)?;
                Some(Moved {
                    guid: d.guid(*k)?.to_owned(),
                    from: bc2_levelwrite::rows_of(&from),
                    to: bc2_levelwrite::rows_of(to),
                    placement: *p,
                })
            })
            .collect();
        if items.is_empty() {
            self.message = "Nothing below to sink into".into();
            return;
        }
        let op = Op::Move { items };
        let going = popup
            .gesture
            .filter(|(g, n, t)| *g == kind && d.last_n() == Some(*n) && (kind != Gesture::Wheel || now - t < BURST));
        let n = match going {
            Some((_, n, _)) if d.amend(n, op.clone()) => n,
            _ => d.commit(op, "Sink below surface"),
        };
        self.message = d.project.operations.last().map_or_else(String::new, Operation::name);
        if let Some(s) = self.sink.as_mut() {
            s.gesture = Some((kind, n, now));
        }
        self.refresh(gpu, palette);
    }

    /// The values several selected objects share, field by field (`None`: Mixed), for the fields they all have in
    /// one type; the position's coordinates first (`Position.X` ...).
    #[must_use]
    pub fn common_fields(&self) -> Vec<(String, FieldType, Option<FieldValue>)> {
        let Some(d) = &self.doc else { return Vec::new() };
        let ks = self.editable_selection();
        let Some((&first, rest)) = ks.split_first() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (c, axis) in ["X", "Y", "Z"].iter().enumerate() {
            let vals: Vec<f64> = ks
                .iter()
                .filter_map(|&k| self.matrix(k).map(|m| math::row(&m, 3).get(c).copied().unwrap_or(0.0)))
                .collect();
            let same = vals
                .windows(2)
                .all(|w| w.first().map(|a| a.to_bits()) == w.get(1).map(|b| b.to_bits()));
            out.push((
                format!("Position.{axis}"),
                FieldType::Float,
                same.then(|| vals.first().copied().map(FieldValue::Float)).flatten(),
            ));
        }
        let others: Vec<Vec<crate::edit::FieldRow>> = rest.iter().map(|&k| d.fields(k)).collect();
        for row in d.fields(first) {
            if !row.ty.settable() || row.ty == FieldType::Ref {
                continue;
            }
            let mut same = true;
            let mut all = true;
            for o in &others {
                match o.iter().find(|r| r.path == row.path && r.ty == row.ty) {
                    Some(r) => same &= r.value == row.value,
                    None => all = false,
                }
            }
            if all {
                out.push((row.path, row.ty, if same { row.value } else { None }));
            }
        }
        out
    }

    /// Sets one field (or a position coordinate, `Position.X`) of every editable selected object from text: one
    /// operation for all of them.
    ///
    /// # Errors
    /// The text is not a value of the field's type, or nothing is selected.
    pub fn set_common_field(&mut self, gpu: &GpuCtx, palette: &Palette, path: &str, text: &str) -> Result<(), String> {
        let ks = self.editable_selection();
        if ks.is_empty() {
            return Err("Select objects that can be edited first".into());
        }
        let text = text.trim();
        if let Some(axis) = path.strip_prefix("Position.") {
            let c = ["X", "Y", "Z"]
                .iter()
                .position(|a| *a == axis)
                .ok_or("no such coordinate")?;
            let v: f64 = text.parse().map_err(|_| format!("{text} is not a number"))?;
            let items: Vec<_> = ks
                .iter()
                .filter_map(|&k| {
                    let m = self.matrix(k)?;
                    let mut p = math::row(&m, 3);
                    if let Some(x) = p.get_mut(c) {
                        *x = v;
                    }
                    Some((k, m, math::at(&m, p), None))
                })
                .collect();
            self.commit_transforms(gpu, palette, false, &items, &format!("Set {path} of the selection"));
            return Ok(());
        }
        let Some(ty) = self
            .common_fields()
            .into_iter()
            .find(|(p, _, _)| p == path)
            .map(|(_, t, _)| t)
        else {
            return Err(format!("not every selected object has {path}"));
        };
        let value = match ty {
            FieldType::Int => FieldValue::Int(text.parse().map_err(|_| format!("{text} is not a whole number"))?),
            FieldType::Float => FieldValue::Float(text.parse().map_err(|_| format!("{text} is not a number"))?),
            FieldType::Bool => FieldValue::Bool(match text {
                "true" | "1" => true,
                "false" | "0" => false,
                _ => return Err(format!("{text} is not true or false")),
            }),
            FieldType::Byte => FieldValue::Byte(text.parse().map_err(|_| format!("{text} is not a byte"))?),
            _ => FieldValue::Text(text.to_owned()),
        };
        let Some(d) = &self.doc else { return Ok(()) };
        let items: Vec<FieldSet> = ks
            .iter()
            .filter_map(|&k| {
                let from = d.fields(k).into_iter().find(|r| r.path == path)?.value;
                let guid = d.guid(k)?.to_owned();
                (from.as_ref() != Some(&value)).then(|| FieldSet {
                    guid,
                    field: path.to_owned(),
                    from,
                    to: value.clone(),
                })
            })
            .collect();
        if !items.is_empty() {
            self.commit(gpu, palette, Op::SetField { items }, "");
        }
        Ok(())
    }

    /// A blueprint's rendered thumbnail from the cache ([`crate::content::thumbnail_file`]), loaded once; `None`
    /// while there is none (the tile shows its category's icon).
    pub(super) fn thumbnail(
        &mut self,
        ctx: &egui::Context,
        source: &crate::content::GameSource,
        path: &str,
    ) -> Option<egui::TextureHandle> {
        if let Some(t) = self.thumbs.get(path) {
            return t.clone();
        }
        let file = crate::content::thumbnail_file(&source.opfor, source.set, path);
        let t = file.is_file().then(|| catalog::read_png(&file)).flatten().map(|pic| {
            let img = egui::ColorImage::from_rgba_unmultiplied(pic.size, &pic.rgba);
            ctx.load_texture(format!("thumb:{path}"), img, egui::TextureOptions::LINEAR)
        });
        self.thumbs.insert(path.to_owned(), t.clone());
        t
    }

    /// Starts placing a content browser entry (it follows the pointer); an entry listed from a bundle's header is
    /// read from its bundle first.
    pub fn start_placing(&mut self, entry: Entry) {
        let entry = if entry.reference.is_empty() {
            match self.content.as_mut().map(|c| c.resolved(&entry.path)) {
                Some(Ok(e)) => e,
                Some(Err(x)) => {
                    self.message = format!("Not placed: {x}");
                    self.place_failed = Some(entry.path);
                    return;
                }
                None => return,
            }
        } else {
            entry
        };
        if let Some(w) = entry.why_not {
            self.message = format!("{} cannot be placed as an object: {w}", entry.name);
            self.place_failed = Some(entry.path);
            return;
        }
        let path = entry.path.clone();
        let tris = self
            .scene()
            .collision
            .as_ref()
            .and_then(|c| c.blueprints.get(&path).cloned().flatten());
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for t in tris.iter().flat_map(|t| t.iter()) {
            for p in t {
                for c in 0..3 {
                    let v = f64::from(p.get(c).copied().unwrap_or(0.0));
                    if let (Some(l), Some(h)) = (lo.get_mut(c), hi.get_mut(c)) {
                        *l = l.min(v);
                        *h = h.max(v);
                    }
                }
            }
        }
        let (extent, half) = if lo[0].is_finite() {
            ((lo[1], hi[1]), ((hi[0] - lo[0]) / 2.0, (hi[2] - lo[2]) / 2.0))
        } else {
            ((0.0, 1.0), (0.5, 0.5))
        };
        let mut placement = self.snap.placement;
        if placement.unit == SinkUnit::Fraction {
            placement.sink *= extent.1 - extent.0;
            placement.unit = SinkUnit::Metres;
        }
        self.message = format!(
            "Placing {}: click to place, Alt+wheel sinks, Shift+wheel turns, Esc cancels",
            entry.name
        );
        self.placing = Some(Placing {
            entry,
            yaw: 0.0,
            placement,
            at: None,
            extent,
            half,
            gameplay: None,
        });
    }

    /// The Spawners and Objectives tiles (made again when the log or the catalogue changed).
    pub fn gameplay_tiles(&mut self) -> Vec<crate::gameplay::Tile> {
        let key = (
            self.doc
                .as_ref()
                .map_or(0, |d| d.project.operations.len() + d.project.redo.len() * 1000),
            self.content.as_ref().map_or(0, |c| c.entries.len()),
        );
        if let Some((k, t)) = &self.tiles
            && *k == key
        {
            return t.clone();
        }
        let Some(d) = &self.doc else { return Vec::new() };
        let t = crate::gameplay::tiles(d, self.content.as_ref());
        self.tiles = Some((key, t.clone()));
        t
    }

    /// Starts placing a Spawners or Objectives tile: it follows the pointer like an object.
    pub fn start_placing_gameplay(&mut self, tile: crate::gameplay::Tile) {
        if let Some(w) = tile.why_not {
            self.message = format!("{} cannot be placed: {w}", tile.label);
            self.place_failed = Some(tile.key());
            return;
        }
        let size = self
            .doc
            .as_ref()
            .and_then(|d| d.index(&tile.template))
            .and_then(|k| self.scene().entities.get(k))
            .and_then(|e| render::marker_size(&e.category))
            .map_or(1.0, f64::from);
        let entry = Entry {
            reference: String::new(),
            path: format!("gameplay:{}", tile.key()),
            name: tile.label.clone(),
            category: Category::Gameplay,
            object_type: None,
            destructible: false,
            used: tile.count,
            source: "This level".into(),
            why_not: None,
            bundle: None,
            in_stack: true,
        };
        self.message = format!(
            "Placing {}: click to place, Alt+wheel sinks, Shift+wheel turns, Esc cancels",
            tile.label
        );
        self.placing = Some(Placing {
            entry,
            yaw: 0.0,
            placement: Placement::default(),
            at: None,
            extent: (0.0, size),
            half: (size / 2.0, size / 2.0),
            gameplay: Some(tile),
        });
    }

    /// Places a Spawners or Objectives tile at `m`: a copy of its template with the sub-instances it owns (carried
    /// to the new place), the browser's team, a vehicle spawner's vehicle; one `place` operation.
    fn finish_gameplay(
        &mut self,
        gpu: &GpuCtx,
        palette: &Palette,
        tile: &crate::gameplay::Tile,
        m: M,
        place: Placement,
    ) {
        let mut vehicle = tile.vehicle.clone();
        if vehicle.is_empty()
            && let Some(path) = &tile.vehicle_path
        {
            match self.content.as_mut().map(|c| c.resolved(path)) {
                Some(Ok(e)) => vehicle = e.reference,
                Some(Err(x)) => {
                    self.message = format!("Not placed: {x}");
                    return;
                }
                None => return,
            }
        }
        let team = self.place_team;
        let Some(d) = self.doc.as_ref() else { return };
        let Some(old) = d
            .index(&tile.template)
            .and_then(|k| d.scene.entities.get(k))
            .and_then(|e| e.matrix)
        else {
            return;
        };
        let owned: Vec<bc2_levelwrite::ops::SubCopy> = tile
            .owned
            .iter()
            .map(|g| bc2_levelwrite::ops::SubCopy {
                source: g.clone(),
                guid: d.project.new_guid(g),
                transform: d
                    .index(g)
                    .and_then(|k| d.scene.entities.get(k))
                    .and_then(|e| e.matrix)
                    .map(|sm| bc2_levelwrite::rows_of(&math::carry_matrix(&old, &m, &sm))),
            })
            .collect();
        let fields = tile
            .team_field
            .filter(|_| {
                d.index(&tile.template)
                    .and_then(|k| d.scene.entities.get(k))
                    .is_some_and(|e| e.team.is_some())
            })
            .map(|f| FieldIs {
                field: f.into(),
                value: FieldValue::Text(
                    match team {
                        1 => "Team1",
                        2 => "Team2",
                        _ => "TeamNeutral",
                    }
                    .into(),
                ),
            })
            .into_iter()
            .collect();
        let vehicle_spawner = matches!(tile.kind, crate::gameplay::Kind::VehicleSpawner(_));
        let guid = d.project.new_guid(&tile.template);
        let item = Placed {
            guid: guid.clone(),
            template: tile.template.clone(),
            blueprint: if vehicle_spawner { vehicle } else { String::new() },
            ref_field: vehicle_spawner.then(|| "Vehicle".to_owned()),
            owned,
            transform: bc2_levelwrite::rows_of(&m),
            fields,
            placement: Some(place),
        };
        self.commit(gpu, palette, Op::Place { items: vec![item] }, "");
        if let Some(k) = self.doc.as_ref().and_then(|d| d.index(&guid)) {
            self.set_selection(gpu, vec![k], true);
        }
    }

    /// Moves the object being placed to the surface under a ray.
    pub(super) fn placing_to(&mut self, o: V3, dir: V3) {
        let hit = {
            let skip = |k: usize| self.doc.as_ref().is_some_and(|d| !d.shown(k));
            query::ray_hit_except(self.scene(), o, dir, &skip)
        };
        if let Some(p) = self.placing.as_mut() {
            p.at = hit.map(|h| query::place(&math::upright(h.point, p.yaw), p.extent, &h, &p.placement));
        }
    }

    /// Places the object being placed where it is: one `place` operation; it is selected.
    pub fn finish_placing(&mut self, gpu: &GpuCtx, palette: &Palette) {
        let Some(p) = self.placing.take() else { return };
        let Some(m) = p.at else {
            self.message = "Nothing under the pointer to place on".into();
            return;
        };
        if let Some(tile) = p.gameplay {
            self.finish_gameplay(gpu, palette, &tile, m, p.placement);
            return;
        }
        let Some(d) = self.doc.as_mut() else { return };
        let Some((template, fields)) = d.template_for(&p.entry.reference) else {
            self.message = "This level has no placed object to make a new one from".into();
            return;
        };
        if p.entry.used == 0 {
            d.know_blueprint(
                &p.entry.path,
                scene_category(&p.entry),
                p.entry.object_type.clone(),
                p.entry.destructible,
            );
        }
        let guid = d.project.new_guid(&p.entry.reference);
        let item = Placed {
            guid: guid.clone(),
            template,
            blueprint: p.entry.reference.clone(),
            transform: bc2_levelwrite::rows_of(&m),
            fields: fields.into_iter().collect::<Vec<FieldIs>>(),
            ref_field: None,
            owned: Vec::new(),
            placement: Some(p.placement),
        };
        self.commit(gpu, palette, Op::Place { items: vec![item] }, "");
        if let Some(k) = self.doc.as_ref().and_then(|d| d.index(&guid)) {
            self.set_selection(gpu, vec![k], true);
        }
    }

    /// Selects the selectable entities whose position is inside a polygon drawn on a view.
    pub(super) fn polygon_select(&mut self, gpu: &GpuCtx, rect: Rect, cam: &Camera) {
        let poly = std::mem::take(&mut self.polygon);
        if poly.len() < 3 {
            return;
        }
        let view = cam.view([f64::from(rect.width()), f64::from(rect.height())]);
        let list: Vec<usize> = self
            .scene()
            .entities
            .iter()
            .filter(|e| self.selectable(e.index) && self.doc.as_ref().is_none_or(|d| d.why_not(e.index).is_none()))
            .filter_map(|e| {
                let s = view.project(e.position()?)?;
                inside(&poly, rect.min + Vec2::new(s[0] as f32, s[1] as f32)).then_some(e.index)
            })
            .collect();
        self.message = format!("{} selected", list.len());
        self.set_selection(gpu, list, true);
    }
}

/// The wheel notches of this frame's raw wheel events with these modifiers held (positive: towards the user).
fn notches(ui: &egui::Ui) -> Vec<(f64, egui::Modifiers)> {
    ui.input(|i| {
        i.raw
            .events
            .iter()
            .filter_map(|e| match e {
                egui::Event::MouseWheel {
                    unit, delta, modifiers, ..
                } => {
                    let d = f64::from(delta.y + delta.x);
                    let n = match unit {
                        egui::MouseWheelUnit::Point => d / 50.0,
                        egui::MouseWheelUnit::Line => d,
                        egui::MouseWheelUnit::Page => d * 3.0,
                    };
                    (n != 0.0).then(|| (n.signum() * n.abs().round().max(1.0), *modifiers))
                }
                _ => None,
            })
            .collect()
    })
}

impl Tabs<'_> {
    /// The tools' input on a viewport before the editing input: the wheel (Alt: sink, Shift while placing: turn),
    /// placing (the pointer moves the object, a click places it), polygon corners, a right-click's target, and
    /// a content browser entry dropped. `true` when the wheel was the tools' (no zoom) - the second value: the left
    /// button is theirs.
    pub(super) fn tools_input(
        &mut self,
        ui: &egui::Ui,
        resp: &egui::Response,
        rect: Rect,
        projection: Projection,
        vp: &Viewport,
    ) -> (bool, bool) {
        let palette = self.palette;
        let gpu = self.gpu;
        let view = Tabs::view_of(rect, &vp.camera);
        let local = |p: Pos2| [f64::from(p.x - rect.min.x), f64::from(p.y - rect.min.y)];
        let hover = ui.input(|i| i.pointer.hover_pos()).filter(|p| rect.contains(*p));
        let now = ui.input(|i| i.time);
        if let Some(h) = hover {
            self.view.pointer_ray = Some(view.ray(local(h)));
        }
        let mut wheel = false;
        let mut left = false;
        // a content browser entry dragged over the view: placing follows the pointer; released: placed
        if let Some(payload) = resp.dnd_hover_payload::<ContentDrag>()
            && self.view.placing.as_ref().is_none_or(|p| p.entry.path != payload.0)
            && self.view.place_failed.as_deref() != Some(payload.0.as_str())
            && let Some(e) = self.view.content.as_ref().and_then(|c| c.by_path(&payload.0)).cloned()
        {
            self.view.start_placing(e);
        }
        if let Some(payload) = resp.dnd_hover_payload::<GameplayDrag>()
            && self
                .view
                .placing
                .as_ref()
                .is_none_or(|p| p.entry.path != format!("gameplay:{}", payload.0))
            && self.view.place_failed.as_deref() != Some(payload.0.as_str())
            && let Some(t) = self.view.gameplay_tiles().into_iter().find(|t| t.key() == payload.0)
        {
            self.view.start_placing_gameplay(t);
        }
        // (taking a payload of another type drops it: ask for the type being carried)
        let released = if egui::DragAndDrop::has_payload_of_type::<GameplayDrag>(ui.ctx()) {
            resp.dnd_release_payload::<GameplayDrag>().is_some()
        } else {
            resp.dnd_release_payload::<ContentDrag>().is_some()
        };
        if resp.hovered() || released {
            let alt_held = ui.input(|i| i.modifiers.alt);
            for (n, m) in notches(ui) {
                let alt = m.alt || alt_held;
                let shift = m.shift || ui.input(|i| i.modifiers.shift);
                let ctrl = m.ctrl || m.command || ui.input(|i| i.modifiers.command);
                let step = if shift { 0.01 } else { 0.1 };
                if alt && let Some(p) = self.view.placing.as_mut() {
                    p.placement.sink = ((p.placement.sink - n * step) * 1000.0).round() / 1000.0;
                    wheel = true;
                } else if !alt
                    && shift
                    && let Some(p) = self.view.placing.as_mut()
                {
                    p.yaw += n * if ctrl { 1.0 } else { 15.0 };
                    wheel = true;
                } else if alt && self.view.doc.is_some() && !self.view.editable_selection().is_empty() {
                    if self.view.sink.is_none() {
                        self.view.open_sink(hover.unwrap_or(rect.center()));
                    }
                    if let Some(s) = self.view.sink.as_mut() {
                        s.placement.sink = ((s.placement.sink - n * step) * 1000.0).round() / 1000.0;
                    }
                    self.view.sink_to(gpu, &palette, Gesture::Wheel, now);
                    wheel = true;
                }
            }
        }
        if self.view.placing.is_some() {
            if let Some(h) = hover {
                let (o, d) = view.ray(local(h));
                self.view.placing_to(o, d);
            }
            if released || resp.clicked() {
                self.view.finish_placing(gpu, &palette);
                self.view.redraw();
            }
            left = true;
        }
        // polygon corners: clicks add, a double click or Enter closes
        if self.view.tool == Tool::Polygon && self.view.doc.is_some() && self.view.workspace.moves_objects() {
            if resp.double_clicked() || (self.view.polygon_close && !self.view.polygon.is_empty()) {
                self.view.polygon_close = false;
                self.view.polygon_select(gpu, rect, &vp.camera);
            } else if resp.clicked()
                && let Some(p) = resp.interact_pointer_pos()
            {
                if self.view.polygon_view != Some(projection) {
                    self.view.polygon.clear();
                    self.view.polygon_view = Some(projection);
                }
                self.view.polygon.push(p);
            }
            left = true;
        }
        // a right-click: what is under the pointer; an object outside the selection is selected
        if resp.secondary_clicked()
            && let Some(at) = resp.interact_pointer_pos()
        {
            let (o, d) = view.ray(local(at));
            let skip = |k: usize| self.view.doc.as_ref().is_some_and(|d| !d.shown(k));
            let hit = query::ray_hit_except(self.view.scene(), o, d, &skip);
            let picked = self.pick_at(rect, &vp.camera, at);
            let entity = picked
                .or(hit.and_then(|h| h.entity))
                .filter(|&k| self.view.selectable(k));
            if let Some(k) = entity
                && !self.view.selection.contains(&k)
            {
                self.view.select(gpu, Some(k), true);
            }
            self.view.context = Some(ContextTarget {
                entity,
                point: hit.map(|h| h.point),
                at,
            });
        }
        (wheel, left)
    }

    /// The entity whose position marker is nearest a screen point (within 12 points), for a right-click on a
    /// marker the ray does not meet.
    fn pick_at(&self, rect: Rect, cam: &Camera, at: Pos2) -> Option<usize> {
        let view = Tabs::view_of(rect, cam);
        let mut best: Option<(f32, usize)> = None;
        for e in &self.view.scene().entities {
            if self.view.doc.as_ref().is_some_and(|d| !d.shown(e.index)) || e.matrix.is_none() {
                continue;
            }
            let Some(p) = e.position() else { continue };
            let off = self.view.marker_offset(e.index);
            let Some(s) = view.project([p[0], p[1] + off, p[2]]) else {
                continue;
            };
            let d = (rect.min + Vec2::new(s[0] as f32, s[1] as f32)).distance(at);
            if d < 12.0 && best.is_none_or(|(b, _)| d < b) {
                best = Some((d, e.index));
            }
        }
        best.map(|(_, k)| k)
    }

    /// The context menu of a view (when it is open): the object's or the ground's.
    pub(super) fn view_menu(&mut self, resp: &egui::Response) {
        let target = self.view.context;
        let kind = if target.is_some_and(|t| t.entity.is_some()) {
            MenuKind::Object
        } else {
            MenuKind::Ground
        };
        let items = menu_items(self.view, kind, target.and_then(|t| t.entity));
        let mut chosen = None;
        let mut rects = Vec::new();
        resp.context_menu(|ui| {
            chosen = menu_ui(ui, &items, &mut rects);
        });
        if !rects.is_empty() {
            self.view.menu_rects = rects;
            self.view.menu_kind = Some(kind);
        }
        if let Some(a) = chosen {
            self.view.menu_point = target.and_then(|t| t.point);
            self.view.pending.push(a);
        }
    }

    /// The object menu on an outliner row.
    pub(super) fn row_menu(&mut self, resp: &egui::Response, entity: usize) {
        if resp.secondary_clicked() && !self.view.selection.contains(&entity) {
            self.view.select(self.gpu, Some(entity), false);
        }
        let items = menu_items(self.view, MenuKind::Object, Some(entity));
        let mut chosen = None;
        let mut rects = Vec::new();
        resp.context_menu(|ui| {
            chosen = menu_ui(ui, &items, &mut rects);
        });
        if !rects.is_empty() {
            self.view.menu_rects = rects;
            self.view.menu_kind = Some(MenuKind::Object);
        }
        if let Some(a) = chosen {
            self.view.pending.push(a);
        }
    }

    /// The tools over a view: the object being placed, the polygon, the stub toolbar of Terrain and Vegetation, the
    /// view mode's note and switch, the sink popup and the picker.
    pub(super) fn tools_overlay(&mut self, ui: &mut egui::Ui, rect: Rect, projection: Projection, cam: &Camera) {
        let p = self.palette;
        let painter = ui.painter().with_clip_rect(rect);
        let view = Tabs::view_of(rect, cam);
        let to_screen = |w: V3| view.project(w).map(|s| rect.min + Vec2::new(s[0] as f32, s[1] as f32));
        // the object being placed: its box, in the selection colour
        if let Some(pl) = &self.view.placing
            && let Some(m) = pl.at
        {
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
                if let (Some(x), Some(y)) = (to_screen(corner(a)), to_screen(corner(b))) {
                    painter.line_segment([x, y], Stroke::new(2.0, p.view_selection));
                }
            }
            if let Some(c) = to_screen(math::row(&m, 3)) {
                painter.circle_filled(c, 4.0, p.view_selection);
                let text = format!(
                    "{}  yaw {:.0} deg  sink {:.2} m",
                    pl.entry.name, pl.yaw, pl.placement.sink
                );
                let g = painter.layout_no_wrap(text, FontId::proportional(12.0), p.ink);
                let r = Rect::from_min_size(c + Vec2::new(12.0, 10.0), g.size()).expand(4.0);
                painter.rect_filled(r, CornerRadius::same(3), p.scene_label);
                painter.galley(r.min + Vec2::splat(4.0), g, p.ink);
            }
        }
        // the polygon being drawn
        if self.view.polygon_view == Some(projection) && !self.view.polygon.is_empty() {
            let mut pts = self.view.polygon.clone();
            if let Some(h) = ui.input(|i| i.pointer.hover_pos()) {
                pts.push(h);
            }
            painter.add(egui::Shape::closed_line(
                pts.clone(),
                Stroke::new(1.5, p.view_selection),
            ));
            for q in &self.view.polygon {
                painter.circle_filled(*q, 3.5, p.view_selection);
            }
        }
        // the Terrain and Vegetation workspaces: their brushes, greyed until their rounds
        if let Some(why) = self.view.workspace.coming() {
            let bar = Rect::from_min_size(rect.min + Vec2::splat(6.0), Vec2::new(rect.width() - 12.0, 40.0));
            let mut ui2 = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(bar)
                    .layout(Layout::left_to_right(Align::Center)),
            );
            egui::Frame::new()
                .fill(p.surface_2.gamma_multiply(0.94))
                .stroke(Stroke::new(1.0, p.line))
                .corner_radius(CornerRadius::same(4))
                .inner_margin(Margin::symmetric(6, 4))
                .show(&mut ui2, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(self.view.workspace.name()).color(p.ink));
                        ui.separator();
                        for b in Brush::ALL.iter().filter(|b| b.workspace() == self.view.workspace) {
                            ui.add_enabled(false, egui::Button::new(b.name()))
                                .on_disabled_hover_text(why);
                        }
                        ui.separator();
                        ui.label(
                            RichText::new(format!("{why}: the layout and its actions are ready")).color(p.ink_muted),
                        );
                    });
                });
        }
        // the view mode: its switch (top right) and its note
        if !(projection == Projection::Perspective && self.view.game_accurate) {
            let at = Rect::from_min_size(
                Pos2::new(
                    rect.right() - 150.0,
                    rect.top() + if self.view.doc.is_some() { 76.0 } else { 6.0 },
                ),
                Vec2::new(144.0, 24.0),
            );
            let mut ui3 = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(at)
                    .layout(Layout::right_to_left(Align::Center)),
            );
            let mut pick = None;
            egui::Frame::new()
                .fill(p.surface_2.gamma_multiply(0.94))
                .stroke(Stroke::new(1.0, p.line))
                .corner_radius(CornerRadius::same(4))
                .inner_margin(Margin::symmetric(4, 0))
                .show(&mut ui3, |ui| {
                    egui::ComboBox::from_id_salt(("view mode", projection))
                        .selected_text(format!("View: {}", self.view.view_mode.name()))
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            for m in ViewMode::ALL {
                                if ui.selectable_label(self.view.view_mode == m, m.name()).clicked() {
                                    pick = Some(m);
                                }
                            }
                        });
                });
            if let Some(m) = pick {
                self.view.pending.push(Action::SetViewMode(m));
            }
            if !self.view.mode_note.is_empty() && self.view.view_mode != ViewMode::Lit {
                let g = painter.layout(
                    self.view.mode_note.clone(),
                    FontId::proportional(12.0),
                    p.ink,
                    rect.width() * 0.5,
                );
                let r = Rect::from_min_size(Pos2::new(rect.right() - g.size().x - 14.0, at.bottom() + 6.0), g.size())
                    .expand(4.0);
                painter.rect_filled(r, CornerRadius::same(3), p.scene_label);
                painter.galley(r.min + Vec2::splat(4.0), g, p.ink);
            }
        }
        if projection == Projection::Perspective {
            self.sink_popup(ui);
            self.picker(ui);
        }
    }

    /// The sink popup: a card beside the pointer.
    fn sink_popup(&mut self, ui: &egui::Ui) {
        let Some(mut s) = self.view.sink.clone() else { return };
        let p = self.palette;
        let now = ui.input(|i| i.time);
        let mut slider_drag = false;
        let mut slider_end = false;
        let mut mode_click = false;
        let mut close = false;
        egui::Area::new(egui::Id::new("sink popup"))
            .fixed_pos(s.at + Vec2::new(18.0, 18.0))
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(p.surface_1)
                    .stroke(Stroke::new(1.0, p.line))
                    .corner_radius(CornerRadius::same(6))
                    .shadow(ui.ctx().global_style().visuals.popup_shadow)
                    .inner_margin(Margin::same(10))
                    .show(ui, |ui| {
                        ui.set_width(260.0);
                        ui.label(
                            RichText::new("Sink below surface")
                                .text_style(style("body-strong"))
                                .color(p.ink),
                        );
                        let pct = if s.height > 0.0 {
                            s.placement.sink / s.height * 100.0
                        } else {
                            0.0
                        };
                        ui.label(
                            RichText::new(format!(
                                "{:.2} m  ({pct:.0} % of its {:.2} m height)",
                                s.placement.sink, s.height
                            ))
                            .text_style(style("value"))
                            .color(p.ink),
                        );
                        let r = ui.add(
                            egui::Slider::new(&mut s.placement.sink, -1.0..=s.height.max(2.0))
                                .suffix(" m")
                                .max_decimals(2),
                        );
                        // a change the user made (the slider also rounds a value it is shown)
                        slider_drag = r.changed() && (r.dragged() || r.drag_stopped() || r.has_focus() || r.clicked());
                        slider_end = r.drag_stopped();
                        let modes = [
                            ("Straight down", false, false),
                            ("Along the normal", true, false),
                            ("Align to the normal", true, true),
                        ];
                        for (name, along, align) in modes {
                            let on = s.placement.along_normal == along && s.placement.align_to_normal == align;
                            if ui.radio(on, name).clicked() && !on {
                                s.placement.along_normal = along;
                                s.placement.align_to_normal = align;
                                mode_click = true;
                            }
                        }
                        ui.label(
                            RichText::new(
                                "Alt+wheel: 0.1 m a notch, with Shift 0.01 m. Each gesture is one undo step.",
                            )
                            .text_style(style("small"))
                            .color(p.ink_muted),
                        );
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                    });
            });
        if close {
            self.view.sink = None;
            return;
        }
        let placement_changed = self.view.sink.as_ref().is_some_and(|o| o.placement != s.placement);
        if let Some(cur) = self.view.sink.as_mut() {
            cur.placement = s.placement;
        }
        let palette = self.palette;
        if slider_drag && placement_changed {
            self.view.sink_to(self.gpu, &palette, Gesture::Slider, now);
        } else if mode_click {
            self.view.sink_to(self.gpu, &palette, Gesture::Mode, now);
        }
        if slider_end && let Some(cur) = self.view.sink.as_mut() {
            // the drag is over: the next one is another operation
            cur.gesture = None;
        }
    }

    /// The "Place object here..." picker: a search and the matching entries; choosing one starts placing it at the
    /// point.
    fn picker(&mut self, ui: &egui::Ui) {
        let Some(mut pk) = self.view.picker.clone() else { return };
        let p = self.palette;
        let mut chosen = None;
        let mut close = false;
        let entries: Vec<Entry> = self.view.content.as_ref().map_or_else(Vec::new, |c| {
            c.shown(&pk.query, self.view.filter)
                .into_iter()
                .take(14)
                .cloned()
                .collect()
        });
        egui::Area::new(egui::Id::new("place picker"))
            .fixed_pos(pk.at + Vec2::new(12.0, 12.0))
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(p.surface_1)
                    .stroke(Stroke::new(1.0, p.line))
                    .corner_radius(CornerRadius::same(6))
                    .shadow(ui.ctx().global_style().visuals.popup_shadow)
                    .inner_margin(Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_width(300.0);
                        ui.label(
                            RichText::new("Place object here")
                                .text_style(style("body-strong"))
                                .color(p.ink),
                        );
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut pk.query)
                                .hint_text(RichText::new("Search blueprints").color(p.ink_faint))
                                .desired_width(f32::INFINITY),
                        );
                        r.request_focus();
                        let enter = ui.input(|i| i.key_pressed(Key::Enter));
                        for (k, e) in entries.iter().enumerate() {
                            let label = format!("{}   {}  {}", e.name, e.category.name(), e.source);
                            let b = ui.add_enabled(e.why_not.is_none(), egui::Button::new(label).frame(false));
                            let b = match e.why_not {
                                Some(w) => b.on_disabled_hover_text(w),
                                None => b.on_hover_text(&e.path),
                            };
                            if b.clicked() || (enter && k == 0 && e.why_not.is_none()) {
                                chosen = Some(e.clone());
                            }
                        }
                        if entries.is_empty() {
                            ui.label(RichText::new("No blueprint matches.").color(p.ink_muted));
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
            });
        if close {
            self.view.picker = None;
            return;
        }
        if let Some(e) = chosen {
            self.view.picker = None;
            self.view.start_placing(e);
            // it starts where the menu was opened
            if let Some(pt) = pk.point {
                let hit = query::surface_below(self.view.scene(), pt[0], pt[2]);
                if let (Some(h), Some(pl)) = (hit, self.view.placing.as_mut()) {
                    pl.at = Some(query::place(
                        &math::upright(h.point, pl.yaw),
                        pl.extent,
                        &h,
                        &pl.placement,
                    ));
                }
            }
            return;
        }
        self.view.picker = Some(pk);
    }

    /// "Place Objects", the asset browser: a tile for every placeable object of the content set (the level's own
    /// first), category tabs and a search; drag a tile into a view to place it, double-click it to place it in the
    /// middle of the 3D view. A tile shows the blueprint's rendered thumbnail when one is in the cache
    /// ([`crate::content::thumbnail_file`]), else its category's icon.
    pub(super) fn content(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let m = self.metrics;
        if let Some(c) = self.view.content.as_mut() {
            c.poll();
            if c.reading {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
            }
        }
        if self.view.content.is_none() {
            self.header(ui, "Place objects", "");
            ui.label(RichText::new("Open a project to place objects.").color(p.ink_muted));
            return;
        }
        // the browser's modes
        let mut mode = self.view.browser_mode;
        egui::Frame::new()
            .inner_margin(Margin::symmetric(m.space_2 as i8, m.space_1 as i8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for bm in crate::gameplay::BrowserMode::ALL {
                        if ui.selectable_label(mode == bm, bm.name()).clicked() {
                            mode = bm;
                        }
                    }
                });
            });
        self.view.browser_mode = mode;
        if mode != crate::gameplay::BrowserMode::PlaceObjects {
            self.gameplay_browser(ui, mode);
            return;
        }
        let Some(cat) = self.view.content.as_ref() else { return };
        let reading = cat.reading;
        let source = cat.source.clone();
        let shown: Vec<Entry> = cat
            .shown(&self.view.content_query, self.view.filter)
            .into_iter()
            .cloned()
            .collect();
        let total = cat.entries.len();
        let set = source.as_ref().map_or("", |s| s.set.word());
        self.header(ui, "Place objects", &format!("{} of {total} {set}", shown.len()));
        let mut filter = self.view.filter;
        egui::Frame::new()
            .inner_margin(Margin::same(m.space_2 as i8))
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.view.content_query)
                        .hint_text(RichText::new("Search objects").color(p.ink_faint))
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(m.space_1);
                // the category tabs (they also limit what the Objects workspace selects)
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(2.0, 2.0);
                    if ui.selectable_label(filter.is_none(), "All").clicked() {
                        filter = None;
                    }
                    for c in Category::ALL {
                        if ui
                            .selectable_label(filter == Some(c), c.name())
                            .on_hover_text("Also limits what can be selected in the Objects workspace")
                            .clicked()
                        {
                            filter = Some(c);
                        }
                    }
                });
                if reading {
                    ui.label(
                        RichText::new("Reading the content set's bundles\u{2026}")
                            .text_style(style("small"))
                            .color(p.ink_muted),
                    );
                }
            });
        if filter != self.view.filter {
            self.view.filter = filter;
            let keep: Vec<usize> = self
                .view
                .selection
                .iter()
                .copied()
                .filter(|&k| self.view.selectable(k))
                .collect();
            self.view.set_selection(self.gpu, keep, false);
        }
        let mut start = None;
        let pick = self.view.content_pick.clone();
        let mut picked = None;
        let mut rects = Vec::new();
        let (tile_w, tile_h, gap) = (118.0f32, 112.0f32, 6.0f32);
        let width = ui.available_width() - m.space_2 * 2.0;
        let cols = ((width + gap) / (tile_w + gap)).floor().max(1.0) as usize;
        let rows = shown.len().div_ceil(cols);
        let ctx = ui.ctx().clone();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, tile_h + gap, rows, |ui, range| {
                for r in range {
                    ui.horizontal(|ui| {
                        ui.add_space(m.space_2);
                        ui.spacing_mut().item_spacing.x = gap;
                        for e in shown.iter().skip(r * cols).take(cols) {
                            let thumb = source.as_ref().and_then(|s| self.view.thumbnail(&ctx, s, &e.path));
                            let sel = pick.as_deref() == Some(e.path.as_str());
                            let draw = |ui: &mut egui::Ui| tile(ui, &p, e, thumb.as_ref(), sel, [tile_w, tile_h]);
                            let id = egui::Id::new(("content tile", &e.path));
                            let resp = if e.why_not.is_none() {
                                ui.dnd_drag_source(id, ContentDrag(e.path.clone()), draw).response
                            } else {
                                draw(ui)
                            };
                            let tip = match e.why_not {
                                Some(w) => format!("{}\n{w}", e.path),
                                None => format!(
                                    "{}\n{}{}Drag into a view to place it; double-click to place it in the middle of \
                                     the 3D view",
                                    e.path,
                                    e.object_type.as_deref().map_or_else(String::new, |t| format!("{t}\n")),
                                    if e.in_stack {
                                        ""
                                    } else {
                                        "From another level's bundle: the build adds that bundle (a later round)\n"
                                    }
                                ),
                            };
                            let resp = resp.on_hover_text(tip);
                            rects.push((e.path.clone(), resp.rect));
                            if resp.clicked() {
                                picked = Some(e.path.clone());
                            }
                            if resp.double_clicked() && e.why_not.is_none() {
                                start = Some(e.clone());
                            }
                        }
                    });
                    ui.add_space(gap);
                }
            });
        self.view.content_rects = rects;
        if picked.is_some() {
            self.view.content_pick = picked;
        }
        if let Some(e) = start {
            self.view.start_placing(e);
            if let Some(r) = self.view.view_rect()
                && let Some(vp) = self.view.viewports.get(&Projection::Perspective)
            {
                let view = Tabs::view_of(r, &vp.camera);
                let (o, d) = view.ray([f64::from(r.width()) / 2.0, f64::from(r.height()) / 2.0]);
                self.view.placing_to(o, d);
            }
        }
    }

    /// The Spawners and Objectives modes: the team picker and the level's gameplay tiles; drag one into a view.
    fn gameplay_browser(&mut self, ui: &mut egui::Ui, mode: crate::gameplay::BrowserMode) {
        let p = self.palette;
        let m = self.metrics;
        let tiles: Vec<crate::gameplay::Tile> = self
            .view
            .gameplay_tiles()
            .into_iter()
            .filter(|t| t.mode == mode)
            .filter(|t| {
                let q = self.view.content_query.to_lowercase();
                q.split_whitespace().all(|w| t.label.to_lowercase().contains(w))
            })
            .collect();
        self.header(ui, mode.name(), &format!("{} kinds", tiles.len()));
        let mut team = self.view.place_team;
        egui::Frame::new()
            .inner_margin(Margin::same(m.space_2 as i8))
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.view.content_query)
                        .hint_text(RichText::new("Search").color(p.ink_faint))
                        .desired_width(f32::INFINITY),
                );
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Team").color(p.ink_muted));
                    for t in [1u8, 2, 0] {
                        let label = if t == 0 {
                            "Neutral".to_owned()
                        } else {
                            format!("Team {t} ({})", self.view.teams.label(t))
                        };
                        if ui
                            .selectable_label(team == t, label)
                            .on_hover_text("Placed spawns, flags and vehicle spawners take this team")
                            .clicked()
                        {
                            team = t;
                        }
                    }
                });
            });
        self.view.place_team = team;
        let (tile_w, tile_h, gap) = (118.0f32, 112.0f32, 6.0f32);
        let width = ui.available_width() - m.space_2 * 2.0;
        let cols = ((width + gap) / (tile_w + gap)).floor().max(1.0) as usize;
        let rows = tiles.len().div_ceil(cols);
        let mut rects = Vec::new();
        let mut start = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, tile_h + gap, rows, |ui, range| {
                for r in range {
                    ui.horizontal(|ui| {
                        ui.add_space(m.space_2);
                        ui.spacing_mut().item_spacing.x = gap;
                        for t in tiles.iter().skip(r * cols).take(cols) {
                            let stand_in = Entry {
                                reference: String::new(),
                                path: t.key(),
                                name: t.label.clone(),
                                category: if matches!(t.kind, crate::gameplay::Kind::VehicleSpawner(_)) {
                                    Category::Vehicles
                                } else {
                                    Category::Gameplay
                                },
                                object_type: None,
                                destructible: false,
                                used: t.count,
                                source: String::new(),
                                why_not: t.why_not,
                                bundle: None,
                                in_stack: true,
                            };
                            let draw = |ui: &mut egui::Ui| tile(ui, &p, &stand_in, None, false, [tile_w, tile_h]);
                            let id = egui::Id::new(("gameplay tile", t.key()));
                            let resp = if t.why_not.is_none() {
                                ui.dnd_drag_source(id, GameplayDrag(t.key()), draw).response
                            } else {
                                draw(ui)
                            };
                            let tip = format!(
                                "{}\n{} in this level{}\n{}{}",
                                t.label,
                                t.count,
                                if t.owned.is_empty() {
                                    String::new()
                                } else {
                                    format!("; placed with the {} sub-objects it owns", t.owned.len())
                                },
                                t.why_not.map_or_else(
                                    || "Drag into a view to place it (the team picker's team). ".to_owned(),
                                    |w| format!("{w}. ")
                                ),
                                t.wiring
                            );
                            let resp = resp.on_hover_text(tip);
                            rects.push((t.key(), resp.rect));
                            if resp.double_clicked() && t.why_not.is_none() {
                                start = Some(t.clone());
                            }
                        }
                    });
                    ui.add_space(gap);
                }
            });
        self.view.content_rects = rects;
        if let Some(t) = start {
            self.view.start_placing_gameplay(t);
            if let Some(r) = self.view.view_rect()
                && let Some(vp) = self.view.viewports.get(&Projection::Perspective)
            {
                let view = Tabs::view_of(r, &vp.camera);
                let (o, d) = view.ray([f64::from(r.width()) / 2.0, f64::from(r.height()) / 2.0]);
                self.view.placing_to(o, d);
            }
        }
    }

    /// The properties of several selected objects: the fields they share, "Mixed" where their values differ; a
    /// value typed sets it on all of them (one operation).
    pub(super) fn multi_properties(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let m = self.metrics;
        let rows = self.view.common_fields();
        let n = self.view.editable_selection().len();
        self.header(ui, "Properties", &format!("{n} objects"));
        let mut set: Option<(String, String)> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(Margin::same(m.space_3 as i8))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("{n} objects selected"))
                            .text_style(style("title"))
                            .color(p.ink),
                    );
                    ui.label(
                        RichText::new(
                            "Fields they all have. Mixed: their values differ; a value typed sets it on all.",
                        )
                        .text_style(style("small"))
                        .color(p.ink_muted),
                    );
                    ui.add_space(m.space_2);
                    egui::Grid::new("common fields")
                        .num_columns(2)
                        .spacing([m.space_2, m.space_1])
                        .show(ui, |ui| {
                            for (path, ty, value) in &rows {
                                ui.label(RichText::new(path).color(p.ink_muted))
                                    .on_hover_text(format!("{path} ({})", ty.name()));
                                let id = ui.make_persistent_id(("common", path));
                                let shown = value.as_ref().map_or_else(String::new, FieldValue::text);
                                let mut text = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| shown.clone());
                                let r = ui.add(
                                    egui::TextEdit::singleline(&mut text)
                                        .hint_text(
                                            RichText::new(if value.is_some() { "(empty)" } else { "Mixed" })
                                                .color(p.ink_faint)
                                                .italics(),
                                        )
                                        .desired_width(150.0)
                                        .font(TextStyle::Monospace),
                                );
                                if r.changed() {
                                    ui.data_mut(|d| d.insert_temp(id, text.clone()));
                                }
                                if r.lost_focus() {
                                    ui.data_mut(|d| d.remove::<String>(id));
                                    if !text.trim().is_empty() && text != shown {
                                        set = Some((path.clone(), text.clone()));
                                    }
                                }
                                ui.end_row();
                            }
                        });
                });
        });
        if let Some((path, text)) = set {
            let palette = self.palette;
            if let Err(e) = self.view.set_common_field(self.gpu, &palette, &path, &text) {
                self.view.message = format!("Not set: {e}");
            }
        }
    }
}

/// A context menu's entries as buttons (disabled ones with their reason); the one clicked. Each button's label and
/// rectangle go to `rects` (tests find them there).
fn menu_ui(ui: &mut egui::Ui, items: &[Option<MenuItem>], rects: &mut Vec<(String, Rect, bool)>) -> Option<Action> {
    let mut chosen = None;
    ui.set_min_width(220.0);
    for it in items {
        let Some(it) = it else {
            ui.separator();
            continue;
        };
        let mut b = egui::Button::new(&it.name);
        if let Some(sc) = actions::entry(it.action).and_then(|e| e.shortcut) {
            b = b.shortcut_text(ui.ctx().format_shortcut(&sc));
        }
        let r = ui.add_enabled(it.why.is_none(), b);
        rects.push((it.name.clone(), r.rect, it.why.is_none()));
        match it.why {
            Some(w) => {
                r.on_disabled_hover_text(w);
            }
            None => {
                if r.clicked() {
                    chosen = Some(it.action);
                    ui.close();
                }
            }
        }
    }
    chosen
}

/// A category's icon: a few strokes (a house, a crate, a vehicle, a spark, sound waves, a tree, a flag, a dot).
fn icon(painter: &egui::Painter, r: Rect, c: Category, ink: Color32) {
    let st = Stroke::new(2.0, ink);
    let cx = r.center();
    let s = r.height().min(r.width()) * 0.32;
    let at = |x: f32, y: f32| cx + Vec2::new(x * s, y * s);
    match c {
        Category::Buildings => {
            painter.add(egui::Shape::closed_line(
                vec![
                    at(-1.0, 1.0),
                    at(-1.0, -0.2),
                    at(0.0, -1.0),
                    at(1.0, -0.2),
                    at(1.0, 1.0),
                ],
                st,
            ));
            painter.rect_stroke(
                Rect::from_min_max(at(-0.25, 0.3), at(0.25, 1.0)),
                CornerRadius::ZERO,
                st,
                egui::StrokeKind::Middle,
            );
        }
        Category::Props => {
            painter.rect_stroke(
                Rect::from_min_max(at(-0.8, -0.8), at(0.8, 0.8)),
                CornerRadius::same(2),
                st,
                egui::StrokeKind::Middle,
            );
            painter.line_segment([at(-0.8, -0.8), at(0.8, 0.8)], st);
            painter.line_segment([at(0.8, -0.8), at(-0.8, 0.8)], st);
        }
        Category::Vehicles => {
            painter.rect_stroke(
                Rect::from_min_max(at(-1.1, -0.4), at(1.1, 0.4)),
                CornerRadius::same(3),
                st,
                egui::StrokeKind::Middle,
            );
            painter.circle_stroke(at(-0.6, 0.65), s * 0.3, st);
            painter.circle_stroke(at(0.6, 0.65), s * 0.3, st);
        }
        Category::Effects => {
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::TAU / 8.0;
                let l = if k % 2 == 0 { 1.0 } else { 0.55 };
                painter.line_segment([cx, at(a.cos() * l, a.sin() * l)], st);
            }
        }
        Category::Sounds => {
            painter.circle_filled(at(-0.8, 0.0), s * 0.18, ink);
            for k in 1..=3 {
                let rad = k as f32 * 0.45;
                let pts: Vec<Pos2> = (0..=8)
                    .map(|i| {
                        let a = -0.8 + 1.6 * i as f32 / 8.0;
                        at(-0.8 + a.cos() * rad, a.sin() * rad)
                    })
                    .collect();
                painter.add(egui::Shape::line(pts, st));
            }
        }
        Category::Vegetation => {
            painter.line_segment([at(0.0, 1.0), at(0.0, 0.1)], st);
            painter.add(egui::Shape::closed_line(
                vec![at(0.0, -1.0), at(0.8, 0.2), at(-0.8, 0.2)],
                st,
            ));
        }
        Category::Gameplay => {
            painter.line_segment([at(-0.6, 1.0), at(-0.6, -1.0)], st);
            painter.add(egui::Shape::closed_line(
                vec![at(-0.6, -1.0), at(0.8, -0.6), at(-0.6, -0.2)],
                st,
            ));
        }
        Category::Other => {
            painter.circle_stroke(cx, s * 0.6, st);
        }
    }
}

/// One tile of the asset browser: the thumbnail (or the category's icon), the name, the category; a selected tile
/// in the accent, an entry that cannot be placed greyed, a mark for objects outside the level's bundles.
fn tile(
    ui: &mut egui::Ui,
    p: &Palette,
    e: &Entry,
    thumb: Option<&egui::TextureHandle>,
    selected: bool,
    size: [f32; 2],
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(size[0], size[1]), Sense::click());
    let painter = ui.painter();
    let fill = if selected {
        p.accent_subtle
    } else if resp.hovered() {
        p.surface_2
    } else {
        p.surface_1
    };
    painter.rect_filled(rect, CornerRadius::same(4), fill);
    painter.rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(1.0, if selected { p.accent } else { p.line }),
        egui::StrokeKind::Inside,
    );
    let pic = Rect::from_min_size(rect.min + Vec2::splat(4.0), Vec2::new(rect.width() - 8.0, 64.0));
    let ink = if e.why_not.is_some() { p.ink_faint } else { p.ink_muted };
    if let Some(t) = thumb {
        painter.image(
            t.id(),
            pic,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        painter.rect_filled(pic, CornerRadius::same(3), p.surface_3);
        icon(painter, pic, e.category, ink);
    }
    if e.used > 0 {
        painter.text(
            pic.right_top() + Vec2::new(-4.0, 3.0),
            Align2::RIGHT_TOP,
            format!("x{}", e.used),
            FontId::proportional(10.5),
            p.accent_text,
        );
    } else if !e.in_stack {
        painter.text(
            pic.right_top() + Vec2::new(-4.0, 3.0),
            Align2::RIGHT_TOP,
            "+bundle",
            FontId::proportional(10.0),
            p.status_warn,
        );
    }
    let name = elided(
        ui,
        &e.name,
        FontId::proportional(12.0),
        if e.why_not.is_some() { p.ink_faint } else { p.ink },
        rect.width() - 8.0,
    );
    painter.galley(Pos2::new(rect.left() + 4.0, pic.bottom() + 4.0), name, p.ink);
    painter.text(
        Pos2::new(rect.left() + 4.0, rect.bottom() - 4.0),
        Align2::LEFT_BOTTOM,
        e.category.name(),
        FontId::proportional(10.5),
        p.ink_muted,
    );
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polygons_contain() {
        let sq = [
            Pos2::new(0.0, 0.0),
            Pos2::new(10.0, 0.0),
            Pos2::new(10.0, 10.0),
            Pos2::new(0.0, 10.0),
        ];
        assert!(inside(&sq, Pos2::new(5.0, 5.0)));
        assert!(!inside(&sq, Pos2::new(15.0, 5.0)));
        assert!(!inside(&sq[..2], Pos2::new(5.0, 0.0)));
    }
}
