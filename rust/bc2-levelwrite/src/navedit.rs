//! The bots' navigation edits a map project carries (round GR2: the Navigation Studio's edit mode in the map editor's
//! AI workspace): the operations of a navigation side file (`<level>.navtune.json`, `bfbc2-auto-navkit/DESIGN.md`
//! 5.1, `core/bfbc2core/navtune.py`) - walkable and unwalkable regions, seeds and unseeds, linked areas, route edges
//! added or removed, traversals and notes - recorded by position, so they still apply to a regenerated base.
//!
//! They are operations of the project's log ([`crate::Op::NavAdd`], [`crate::Op::NavRemove`], [`crate::Op::NavLoad`]),
//! so they undo, redo and save with the object edits; [`crate::MapProject::nav_edits`] gives the list the log leaves.
//! They do not change the level partition: the editor writes them as a side file and NavKit builds them
//! (`bfbc2.exe navkit apply`). The fields are the side file's own, in its order (`navtune.op`).

use serde::{Deserialize, Serialize};

/// A region of a walkable or unwalkable edit: a polygon on the xz plane with a height range, or a box.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NavRegion {
    /// The polygon's corners (x, z), three or more.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polygon: Option<Vec<[f64; 2]>>,
    /// The polygon's height range (low, high).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<[f64; 2]>,
    /// A box: x0, y0, z0, x1, y1, z1 (each low before high).
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "box")]
    pub cube: Option<[f64; 6]>,
}

/// One navigation edit, as the side file keeps it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NavEdit {
    /// `walkable`, `unwalkable`, `seed`, `unseed`, `link-areas`, `route-edge`, `traversal` or `note`.
    pub op: String,
    /// A traversal's kind: `ladder`, `jump`, `vault` or `drop`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// A region edit's region.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<NavRegion>,
    /// A walkable region's value (`true`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<bool>,
    /// A seed's, an unseed's or a note's point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<[f64; 3]>,
    /// A note's text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// A link's or route edge's first point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a: Option<[f64; 3]>,
    /// Its second point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b: Option<[f64; 3]>,
    /// A route edge's action: `add` or `remove`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// A traversal's bottom.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<[f64; 3]>,
    /// A traversal's top.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<[f64; 3]>,
    /// Both ways (route edges and traversals).
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "two-way")]
    pub two_way: Option<bool>,
    /// Who made it.
    #[serde(default)]
    pub by: String,
    /// When.
    #[serde(default)]
    pub when: String,
}

impl NavEdit {
    /// Where it is (the point a camera flies to): its point, its first point, its bottom, or its region's centre at
    /// the region's floor.
    #[must_use]
    pub fn anchor(&self) -> Option<[f64; 3]> {
        if let Some(p) = self.at.or(self.a).or(self.bottom) {
            return Some(p);
        }
        let r = self.region.as_ref()?;
        if let (Some(poly), Some(y)) = (&r.polygon, r.y) {
            // a polygon has a few corners: exact as a double
            #[allow(clippy::cast_precision_loss)]
            let n = poly.len().max(1) as f64;
            let (sx, sz) = poly.iter().fold((0.0, 0.0), |(x, z), p| (x + p[0], z + p[1]));
            return Some([sx / n, y[0], sz / n]);
        }
        r.cube
            .map(|b| [f64::midpoint(b[0], b[3]), b[1], f64::midpoint(b[2], b[5])])
    }
}

/// The file a set of navigation edits was made on (the side file's `base`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NavBase {
    /// `navkit` (NavKit's build: every edit allowed) or `dice` (DICE's file: only the additive edits).
    pub kind: String,
    /// The file's SHA-1.
    #[serde(default)]
    pub sha1: String,
    /// Its name.
    #[serde(default)]
    pub file: String,
}

/// The navigation edits a log leaves: the base they were loaded with (`None`: made in the editor; the base is the
/// file being edited) and the edits in order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NavEdits {
    /// The base of a loaded side file.
    pub base: Option<NavBase>,
    /// The edits.
    pub edits: Vec<NavEdit>,
}

impl NavEdits {
    /// The list after one operation of the log.
    pub fn apply(&mut self, op: &crate::Op) {
        match op {
            crate::Op::NavAdd { edit } => self.edits.push(edit.clone()),
            crate::Op::NavRemove { index, edit } => {
                let at = if self.edits.get(*index) == Some(edit) {
                    Some(*index)
                } else {
                    self.edits.iter().position(|e| e == edit)
                };
                if let Some(k) = at {
                    self.edits.remove(k);
                }
            }
            crate::Op::NavLoad { base, edits, .. } => {
                self.base = Some(base.clone());
                self.edits.clone_from(edits);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn edits_keep_the_side_files_fields_in_order() {
        let e = NavEdit {
            op: "route-edge".into(),
            a: Some([1.0, 2.0, 3.0]),
            b: Some([4.0, 5.0, 6.5]),
            action: Some("add".into()),
            two_way: Some(true),
            by: "PC".into(),
            when: "2026-11-20T10:00:00Z".into(),
            ..NavEdit::default()
        };
        let text = serde_json::to_string(&e).unwrap();
        assert_eq!(
            text,
            r#"{"op":"route-edge","a":[1.0,2.0,3.0],"b":[4.0,5.0,6.5],"action":"add","two-way":true,"by":"PC","when":"2026-11-20T10:00:00Z"}"#
        );
        assert_eq!(serde_json::from_str::<NavEdit>(&text).unwrap(), e);
        let r = NavEdit {
            op: "walkable".into(),
            region: Some(NavRegion {
                cube: Some([0.0, -2.0, 0.0, 4.0, 6.0, 2.0]),
                ..NavRegion::default()
            }),
            value: Some(true),
            ..NavEdit::default()
        };
        assert!(
            serde_json::to_string(&r)
                .unwrap()
                .starts_with(r#"{"op":"walkable","region":{"box":[0.0,-2.0"#)
        );
        assert_eq!(r.anchor(), Some([2.0, -2.0, 1.0]));
        let mut list = NavEdits::default();
        list.apply(&crate::Op::NavAdd { edit: e.clone() });
        list.apply(&crate::Op::NavAdd { edit: r.clone() });
        list.apply(&crate::Op::NavRemove { index: 0, edit: e });
        assert_eq!(list.edits, [r]);
        list.apply(&crate::Op::NavLoad {
            from: "x.navtune.json".into(),
            base: NavBase {
                kind: "dice".into(),
                ..NavBase::default()
            },
            edits: Vec::new(),
        });
        assert!(list.edits.is_empty() && list.base.as_ref().unwrap().kind == "dice");
    }
}
