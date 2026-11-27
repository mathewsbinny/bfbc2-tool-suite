//! The move and rotate gizmo (`EDITOR_ARCHITECTURE.md` 3.2, item 2): its handles on screen, which one is under the
//! pointer, and what a drag of it does, in world or local axes. Drawn by the view over the 3D image (egui's painter)
//! in the axis colours, the dragged handle in `selection` (design/project/README.md, "Gizmo and axes").
//!
//! Handles keep a fixed size on screen ([`ARM_PX`]): the arms' length in metres follows the distance.

use crate::camera::View;
use crate::math::{self, M, V3, add, cross, dot, len, mul, norm, row, sub};

/// The arms' length on screen, pixels.
pub const ARM_PX: f64 = 90.0;
/// How near the pointer must be to a handle, pixels.
pub const GRAB_PX: f64 = 8.0;

/// The editing tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub enum Tool {
    /// Select (click, box).
    #[default]
    Select,
    /// Move with the gizmo.
    Move,
    /// Rotate with the gizmo.
    Rotate,
    /// Measure: click two points.
    Ruler,
    /// Select inside a polygon: click its corners.
    Polygon,
    /// The Gameplay workspace's tool (round E4): add flags, spawn points and M-COMs, carry them by the cursor.
    Gameplay,
    /// The Terrain workspace's brushes (round E5): sculpt and paint the terrain.
    Terrain,
    /// The AI workspace's navigation edit tools (round E4-rest2): regions, seeds, links, route edges, traversals,
    /// notes ([`crate::navtool::NavDrawTool`]).
    Nav,
    /// Placing an object from the content browser, a tile or the picker (round E4-rest2,
    /// [`crate::placetool::PlaceTool`]).
    Place,
}

impl Tool {
    /// Its name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::Move => "Move",
            Self::Rotate => "Rotate",
            Self::Ruler => "Ruler",
            Self::Polygon => "Polygon",
            Self::Gameplay => "Gameplay",
            Self::Terrain => "Terrain brush",
            Self::Nav => "Navigation edit",
            Self::Place => "Place",
        }
    }
}

/// The gizmo's axes: the world's, or the object's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Space {
    /// X, Y (up), Z of the map.
    #[default]
    World,
    /// The selected object's right, up and forward.
    Local,
}

/// A handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    /// Along an axis (0 X, 1 Y, 2 Z).
    Axis(usize),
    /// In the plane at right angles to an axis.
    Plane(usize),
    /// The centre: free (on the surface under the pointer when surface snapping is on, else in the view's plane).
    Centre,
    /// Around an axis.
    Ring(usize),
}

/// Where the gizmo is and its axes (unit vectors).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    /// The pivot.
    pub origin: V3,
    /// The three axes.
    pub axes: [V3; 3],
}

impl Frame {
    /// The frame of a transform in a space.
    #[must_use]
    pub fn of(m: &M, space: Space) -> Self {
        let axes = match space {
            Space::World => [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            Space::Local => {
                let a = [norm(row(m, 0)), norm(row(m, 1)), norm(row(m, 2))];
                if a.iter().any(|v| len(*v) == 0.0) {
                    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
                } else {
                    a
                }
            }
        };
        Self {
            origin: row(m, 3),
            axes,
        }
    }

    fn axis(&self, k: usize) -> V3 {
        self.axes.get(k).copied().unwrap_or([0.0, 1.0, 0.0])
    }
}

/// The arms' length in metres at the frame's origin, for [`ARM_PX`] on screen.
#[must_use]
pub fn arm(view: &View, origin: V3) -> f64 {
    let ppm = match (view.project(origin), view.project(add(origin, view.right))) {
        (Some(a), Some(b)) => ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt(),
        _ => 1.0,
    };
    ARM_PX / ppm.max(1e-6)
}

/// A handle's outline on screen: a polyline in viewport pixels.
pub type Outline = Vec<[f64; 2]>;

/// The handles of a tool and their outlines on screen (empty for tools without a gizmo).
#[must_use]
pub fn handles(view: &View, f: &Frame, tool: Tool) -> Vec<(Handle, Outline)> {
    let l = arm(view, f.origin);
    let p = |w: V3| view.project(w);
    let mut out = Vec::new();
    match tool {
        Tool::Move => {
            for k in 0..3 {
                if let (Some(a), Some(b)) = (p(f.origin), p(add(f.origin, mul(f.axis(k), l)))) {
                    out.push((Handle::Axis(k), vec![a, b]));
                }
            }
            for k in 0..3 {
                let (i, j) = ((k + 1) % 3, (k + 2) % 3);
                let (u, v) = (mul(f.axis(i), l * 0.22), mul(f.axis(j), l * 0.22));
                let c = add(f.origin, add(mul(f.axis(i), l * 0.18), mul(f.axis(j), l * 0.18)));
                let quad = [c, add(c, u), add(add(c, u), v), add(c, v), c];
                let pts: Option<Outline> = quad.iter().map(|w| p(*w)).collect();
                if let Some(pts) = pts {
                    out.push((Handle::Plane(k), pts));
                }
            }
            if let Some(c) = p(f.origin) {
                out.push((Handle::Centre, vec![c]));
            }
        }
        Tool::Rotate => {
            for k in 0..3 {
                let (u, v) = (f.axis((k + 1) % 3), f.axis((k + 2) % 3));
                let pts: Option<Outline> = (0..=48)
                    .map(|s| {
                        let a = f64::from(s) / 48.0 * std::f64::consts::TAU;
                        p(add(f.origin, add(mul(u, a.cos() * l * 0.9), mul(v, a.sin() * l * 0.9))))
                    })
                    .collect();
                if let Some(pts) = pts {
                    out.push((Handle::Ring(k), pts));
                }
            }
        }
        Tool::Select | Tool::Ruler | Tool::Polygon | Tool::Gameplay | Tool::Terrain | Tool::Nav | Tool::Place => {}
    }
    out
}

fn seg_dist(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    ((p[0] - a[0] - t * dx).powi(2) + (p[1] - a[1] - t * dy).powi(2)).sqrt()
}

fn inside(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
    let mut c = false;
    let Some(&last) = poly.last() else { return false };
    let mut b = last;
    for &a in poly {
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            c = !c;
        }
        b = a;
    }
    c
}

/// The handle under the pointer (`px`, viewport pixels): the centre first, then plane squares, then the nearest arm or
/// ring within [`GRAB_PX`].
#[must_use]
pub fn pick(view: &View, f: &Frame, tool: Tool, px: [f64; 2]) -> Option<Handle> {
    let hs = handles(view, f, tool);
    let mut best: Option<(f64, Handle)> = None;
    for (h, pts) in &hs {
        match h {
            Handle::Centre => {
                if let Some(c) = pts.first()
                    && (px[0] - c[0]).abs() <= GRAB_PX
                    && (px[1] - c[1]).abs() <= GRAB_PX
                {
                    return Some(*h);
                }
            }
            Handle::Plane(_) => {
                if inside(px, pts) {
                    return Some(*h);
                }
            }
            _ => {
                let d = pts
                    .windows(2)
                    .filter_map(|w| match w {
                        [a, b] => Some(seg_dist(px, *a, *b)),
                        _ => None,
                    })
                    .fold(f64::INFINITY, f64::min);
                if d <= GRAB_PX && best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, *h));
                }
            }
        }
    }
    best.map(|(_, h)| h)
}

/// Where along the line through `origin` in unit direction `axis` the pointer's ray passes nearest (metres from the
/// origin); `None` when the ray runs along the axis.
#[must_use]
pub fn axis_param(view: &View, origin: V3, axis: V3, px: [f64; 2]) -> Option<f64> {
    let (ro, rd) = view.ray(px);
    let w0 = sub(ro, origin);
    let b = dot(rd, axis);
    let d = dot(rd, w0);
    let e = dot(axis, w0);
    let denom = 1.0 - b * b;
    if denom < 1e-6 {
        return None;
    }
    Some((e - b * d) / denom)
}

/// Where the pointer's ray meets the plane through `origin` at right angles to `normal`.
#[must_use]
pub fn plane_point(view: &View, origin: V3, normal: V3, px: [f64; 2]) -> Option<V3> {
    let (ro, rd) = view.ray(px);
    let den = dot(rd, normal);
    if den.abs() < 1e-9 {
        return None;
    }
    let t = dot(sub(origin, ro), normal) / den;
    (t > 0.0).then(|| add(ro, mul(rd, t)))
}

/// The pointer's angle around `axis` through `origin` (radians, in the plane at right angles to the axis).
#[must_use]
pub fn ring_angle(view: &View, origin: V3, axis: V3, px: [f64; 2]) -> Option<f64> {
    let q = plane_point(view, origin, axis, px)?;
    let any = if axis[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let u = norm(cross(axis, any));
    let v = cross(axis, u);
    let d = sub(q, origin);
    Some(dot(d, v).atan2(dot(d, u)))
}

/// What a drag started with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Start {
    /// An arm: the parameter along it.
    Along(f64),
    /// A plane or the centre: the point in it.
    At(V3),
    /// A ring: the angle.
    Angle(f64),
}

/// The start of a drag of handle `h` with the pointer at `px`.
#[must_use]
pub fn start(view: &View, f: &Frame, h: Handle, px: [f64; 2]) -> Option<Start> {
    match h {
        Handle::Axis(k) => axis_param(view, f.origin, f.axis(k), px).map(Start::Along),
        Handle::Plane(k) => plane_point(view, f.origin, f.axis(k), px).map(Start::At),
        Handle::Centre => plane_point(view, f.origin, view.back, px).map(Start::At),
        Handle::Ring(k) => ring_angle(view, f.origin, f.axis(k), px).map(Start::Angle),
    }
}

/// What a drag does so far: a move (metres, world) or a turn (radians around a world axis through the origin).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Delta {
    /// Moved.
    Move(V3),
    /// Turned.
    Turn {
        /// The unit axis.
        axis: V3,
        /// The angle, radians.
        angle: f64,
    },
}

/// The drag's delta with the pointer now at `px`; `grid` snaps a move's length along an arm (and each axis of a plane
/// move), `angle` snaps a turn (radians; 0: none).
#[must_use]
pub fn delta(view: &View, f: &Frame, h: Handle, s: Start, px: [f64; 2], grid: f64, angle: f64) -> Option<Delta> {
    match (h, s) {
        (Handle::Axis(k), Start::Along(t0)) => {
            let t = axis_param(view, f.origin, f.axis(k), px)?;
            Some(Delta::Move(mul(f.axis(k), math::snap(t - t0, grid))))
        }
        (Handle::Plane(k), Start::At(p0)) => {
            let p = plane_point(view, f.origin, f.axis(k), px)?;
            let d = sub(p, p0);
            let (i, j) = (f.axis((k + 1) % 3), f.axis((k + 2) % 3));
            Some(Delta::Move(add(
                mul(i, math::snap(dot(d, i), grid)),
                mul(j, math::snap(dot(d, j), grid)),
            )))
        }
        (Handle::Centre, Start::At(p0)) => {
            let p = plane_point(view, f.origin, view.back, px)?;
            Some(Delta::Move(sub(p, p0)))
        }
        (Handle::Ring(k), Start::Angle(a0)) => {
            let a = ring_angle(view, f.origin, f.axis(k), px)?;
            let mut d = a - a0;
            while d > std::f64::consts::PI {
                d -= std::f64::consts::TAU;
            }
            while d < -std::f64::consts::PI {
                d += std::f64::consts::TAU;
            }
            // ring_angle measures around `axis` from u to v = axis x u: positive is a right-handed turn
            Some(Delta::Turn {
                axis: f.axis(k),
                angle: math::snap(d, angle),
            })
        }
        _ => None,
    }
}

/// A transform with a delta applied (a turn around the frame's origin).
#[must_use]
pub fn apply(m: &M, f: &Frame, d: Delta) -> M {
    match d {
        Delta::Move(v) => math::translate(m, v),
        Delta::Turn { axis, angle } => math::rotate_about(m, f.origin, axis, angle),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
mod tests {
    use super::*;
    use crate::camera::{Camera, Projection};

    fn view() -> View {
        Camera {
            target: [0.0, 0.0, 0.0],
            yaw: 0.6,
            pitch: 0.5,
            distance: 60.0,
            fov: 60.0,
            projection: Projection::Perspective,
        }
        .view([800.0, 600.0])
    }

    const I: M = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.];

    #[test]
    fn handles_are_found_and_dragged() {
        let v = view();
        let f = Frame::of(&I, Space::World);
        let hs = handles(&v, &f, Tool::Move);
        assert_eq!(hs.len(), 7);
        // the X arm's tip picks the X arm; the centre picks the centre
        let tip = hs.iter().find(|(h, _)| *h == Handle::Axis(0)).unwrap().1[1];
        let mid = |a: [f64; 2], b: [f64; 2]| [f64::midpoint(a[0], b[0]), f64::midpoint(a[1], b[1])];
        let o = v.project([0.0; 3]).unwrap();
        assert_eq!(pick(&v, &f, Tool::Move, mid(o, tip)), Some(Handle::Axis(0)));
        assert_eq!(pick(&v, &f, Tool::Move, o), Some(Handle::Centre));
        assert_eq!(pick(&v, &f, Tool::Move, [5.0, 5.0]), None);
        // dragging the X arm from its middle to where x = 7.3 m projects: 7.3 m, or 7 m on a 1 m grid
        let l = arm(&v, [0.0; 3]);
        let from = v.project([l / 2.0, 0.0, 0.0]).unwrap();
        let to = v.project([l / 2.0 + 7.3, 0.0, 0.0]).unwrap();
        let s = start(&v, &f, Handle::Axis(0), from).unwrap();
        let Some(Delta::Move(d)) = delta(&v, &f, Handle::Axis(0), s, to, 0.0, 0.0) else {
            panic!()
        };
        assert!(
            (d[0] - 7.3).abs() < 1e-6 && d[1].abs() < 1e-9 && d[2].abs() < 1e-9,
            "{d:?}"
        );
        let Some(Delta::Move(d)) = delta(&v, &f, Handle::Axis(0), s, to, 1.0, 0.0) else {
            panic!()
        };
        assert!((d[0] - 7.0).abs() < 1e-9);
        // a plane drag (the ground plane, at right angles to Y)
        let p0 = v.project([2.0, 0.0, 2.0]).unwrap();
        let p1 = v.project([5.0, 0.0, -1.0]).unwrap();
        let s = Start::At(plane_point(&v, [0.0; 3], [0.0, 1.0, 0.0], p0).unwrap());
        let Some(Delta::Move(d)) = delta(&v, &f, Handle::Plane(1), s, p1, 0.0, 0.0) else {
            panic!()
        };
        assert!((d[0] - 3.0).abs() < 1e-6 && d[1].abs() < 1e-9 && (d[2] + 3.0).abs() < 1e-6);
    }

    #[test]
    fn rings_turn_and_snap() {
        let v = view();
        let f = Frame::of(&I, Space::World);
        assert_eq!(handles(&v, &f, Tool::Rotate).len(), 3);
        // around Y: from the +X side of the ring to the -Z side is a quarter turn
        let r = arm(&v, [0.0; 3]) * 0.9;
        let a = v.project([r, 0.0, 0.0]).unwrap();
        let b = v.project([0.0, 0.0, -r]).unwrap();
        assert_eq!(pick(&v, &f, Tool::Rotate, a), Some(Handle::Ring(1)));
        let s = start(&v, &f, Handle::Ring(1), a).unwrap();
        let Some(Delta::Turn { axis, angle }) = delta(&v, &f, Handle::Ring(1), s, b, 0.0, 0.0) else {
            panic!()
        };
        let m = apply(&math::at(&I, [r, 0.0, 0.0]), &f, Delta::Turn { axis, angle });
        // the object at +X went round to -Z
        assert!(len(sub(row(&m, 3), [0.0, 0.0, -r])) < 1e-6, "{:?}", row(&m, 3));
        let Some(Delta::Turn { angle, .. }) = delta(&v, &f, Handle::Ring(1), s, b, 0.0, 40f64.to_radians()) else {
            panic!()
        };
        assert!((angle.abs().to_degrees() - 80.0).abs() < 1e-9);
        // local axes follow the object
        let turned = math::rotate_about(&I, [0.0; 3], [0.0, 1.0, 0.0], 0.5);
        let fl = Frame::of(&turned, Space::Local);
        assert!(len(sub(fl.axes[0], row(&turned, 0))) < 1e-12);
    }
}
