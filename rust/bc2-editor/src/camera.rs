//! The 3D view's camera: orbit around a target (left drag), pan (middle drag), zoom (wheel), fly (hold the right
//! button: the mouse looks, W A S D move, Q / E down and up, Shift faster), focus on a point (F); bookmarks keep and
//! recall a camera. Y is up, as in the game's data; right-handed, as the Map Inspector's viewer draws it.
//!
//! Matrices are column-major `[[f32; 4]; 4]` (WGSL's `mat4x4<f32>`), with a reversed infinite projection (depth 1 at
//! the near plane, 0 at infinity) so a 4 km map keeps its depth precision.

/// A 3-vector of f64.
pub type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    if l < 1e-12 { [0.0, 0.0, 0.0] } else { mul(a, 1.0 / l) }
}

/// A camera.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Camera {
    /// What the orbit turns around.
    pub target: V3,
    /// Around Y, radians.
    pub yaw: f64,
    /// Up and down, radians.
    pub pitch: f64,
    /// From the target, metres.
    pub distance: f64,
    /// Vertical field of view, degrees.
    pub fov: f64,
    /// Perspective, or one of the orthographic views.
    #[serde(default)]
    pub projection: Projection,
}

/// How a viewport projects: the 3D view in perspective, or a top, front or side view in orthographic projection
/// (`EDITOR_ARCHITECTURE.md` 3.10, item 4: each viewport has its own camera).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub enum Projection {
    /// The 3D view: orbit, fly.
    #[default]
    Perspective,
    /// Looking down (-Y), north (-Z) up.
    Top,
    /// Looking along -Z.
    Front,
    /// Looking along -X.
    Side,
}

impl Projection {
    /// Every projection, the 3D view first.
    pub const ALL: [Self; 4] = [Self::Perspective, Self::Top, Self::Front, Self::Side];

    /// Its name on a viewport's tab.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Perspective => "Perspective",
            Self::Top => "Top",
            Self::Front => "Front",
            Self::Side => "Side",
        }
    }

    /// The fixed (right, up, back) of an orthographic view; `None` for the perspective one.
    #[must_use]
    pub fn basis(self) -> Option<(V3, V3, V3)> {
        match self {
            Self::Perspective => None,
            Self::Top => Some(([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0])),
            Self::Front => Some(([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0])),
            Self::Side => Some(([0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0])),
        }
    }
}

/// How far behind its target an orthographic camera stands, and how deep it sees (metres).
const ORTHO_BACK: f64 = 20_000.0;

impl Default for Camera {
    fn default() -> Self {
        Self {
            target: [0.0; 3],
            yaw: 0.6,
            pitch: 0.6,
            distance: 300.0,
            fov: 60.0,
            projection: Projection::Perspective,
        }
    }
}

/// A camera's view in the 3D view's pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    /// Where the camera is.
    pub eye: V3,
    /// Right, up and backward (the camera looks along -back).
    pub right: V3,
    /// Up.
    pub up: V3,
    /// Backward.
    pub back: V3,
    /// `1 / tan(fov / 2)`.
    pub focal: f64,
    /// The near plane.
    pub near: f64,
    /// Width and height in pixels.
    pub size: [f64; 2],
    /// Orthographic views: half the view's height in metres.
    pub ortho: Option<f64>,
}

impl Camera {
    /// The direction from the target to the eye.
    #[must_use]
    pub fn direction(&self) -> V3 {
        [
            self.pitch.cos() * self.yaw.sin(),
            self.pitch.sin(),
            self.pitch.cos() * self.yaw.cos(),
        ]
    }

    /// Where the camera is.
    #[must_use]
    pub fn eye(&self) -> V3 {
        match self.projection.basis() {
            Some((_, _, back)) => add(self.target, mul(back, ORTHO_BACK)),
            None => add(self.target, mul(self.direction(), self.distance)),
        }
    }

    /// A camera of this projection looking at the same place.
    #[must_use]
    pub fn with_projection(&self, projection: Projection) -> Self {
        Self { projection, ..*self }
    }

    /// Frames a box: the target at its middle, far enough to see it (`viewer.Camera.frame`).
    pub fn frame(&mut self, lo: V3, hi: V3) {
        self.target = mul(add(lo, hi), 0.5);
        self.distance = dot(sub(hi, lo), sub(hi, lo)).sqrt() * 0.6 + 10.0;
    }

    /// Looks at `point` from `distance` (or the current distance), keeping the angles.
    pub fn focus(&mut self, point: V3, distance: Option<f64>) {
        self.target = point;
        if let Some(d) = distance {
            self.distance = d.max(1.0);
        }
    }

    /// Turns the orbit by a mouse drag in pixels.
    pub fn orbit(&mut self, dx: f64, dy: f64) {
        if self.projection != Projection::Perspective {
            return;
        }
        self.yaw -= dx * 0.005;
        self.pitch = (self.pitch + dy * 0.005).clamp(-1.5, 1.5);
    }

    /// Looks around from the eye (fly mode): the eye stays, the target moves.
    pub fn look(&mut self, dx: f64, dy: f64) {
        if self.projection != Projection::Perspective {
            return;
        }
        let eye = self.eye();
        self.yaw -= dx * 0.004;
        self.pitch = (self.pitch + dy * 0.004).clamp(-1.55, 1.55);
        self.target = sub(eye, mul(self.direction(), self.distance));
    }

    /// Pans by a drag in pixels, at the target's depth.
    pub fn pan(&mut self, dx: f64, dy: f64, height: f64) {
        let v = self.view([1.0, height.max(1.0)]);
        let per_pixel = match v.ortho {
            Some(h) => 2.0 * h / height.max(1.0),
            None => 2.0 * self.distance / (v.focal * height.max(1.0)),
        };
        self.target = add(
            self.target,
            add(mul(v.right, -dx * per_pixel), mul(v.up, dy * per_pixel)),
        );
    }

    /// Zooms by wheel steps (positive: closer).
    pub fn zoom(&mut self, steps: f64) {
        self.distance = (self.distance * 0.85_f64.powf(steps)).clamp(1.0, 50_000.0);
    }

    /// Flies by a move in the camera's frame: forward, right, up (metres).
    pub fn fly(&mut self, forward: f64, right: f64, up: f64) {
        let v = self.view([1.0, 1.0]);
        let d = add(add(mul(v.back, -forward), mul(v.right, right)), [0.0, up, 0.0]);
        self.target = add(self.target, d);
    }

    /// The view for a viewport of `size` pixels.
    #[must_use]
    pub fn view(&self, size: [f64; 2]) -> View {
        if let Some((right, up, back)) = self.projection.basis() {
            return View {
                eye: self.eye(),
                right,
                up,
                back,
                focal: 1.0,
                near: 1.0,
                size,
                ortho: Some(self.distance * 0.5),
            };
        }
        let eye = self.eye();
        let back = norm(sub(eye, self.target));
        let mut right = cross([0.0, 1.0, 0.0], back);
        if dot(right, right) < 1e-12 {
            right = [1.0, 0.0, 0.0];
        }
        let right = norm(right);
        let up = cross(back, right);
        View {
            eye,
            right,
            up,
            back,
            focal: 1.0 / (self.fov.to_radians() / 2.0).tan(),
            near: (self.distance / 2000.0).max(0.05),
            size,
            ortho: None,
        }
    }
}

impl View {
    /// The view-projection matrix, column-major, reversed infinite depth.
    #[must_use]
    pub fn matrix(&self) -> [[f32; 4]; 4] {
        let aspect = self.size[0] / self.size[1].max(1.0);
        let (r, u, b, e) = (self.right, self.up, self.back, self.eye);
        // view: rows right, up, back; translation -R e
        let view = [
            [r[0], r[1], r[2], -dot(r, e)],
            [u[0], u[1], u[2], -dot(u, e)],
            [b[0], b[1], b[2], -dot(b, e)],
            [0.0, 0.0, 0.0, 1.0],
        ];
        // projection rows: x f/a, y f, z near (reversed infinite), w -z_view
        let f = self.focal;
        let proj = match self.ortho {
            // orthographic, reversed: depth 1 at the near plane, 0 at the far one
            Some(h) => {
                let span = 2.0 * ORTHO_BACK - self.near;
                [
                    [1.0 / (h * aspect), 0.0, 0.0, 0.0],
                    [0.0, 1.0 / h, 0.0, 0.0],
                    [0.0, 0.0, 1.0 / span, 2.0 * ORTHO_BACK / span],
                    [0.0, 0.0, 0.0, 1.0],
                ]
            }
            None => [
                [f / aspect, 0.0, 0.0, 0.0],
                [0.0, f, 0.0, 0.0],
                [0.0, 0.0, 0.0, self.near],
                [0.0, 0.0, -1.0, 0.0],
            ],
        };
        let mut out = [[0.0f32; 4]; 4];
        for (row, p) in proj.iter().enumerate() {
            for col in 0..4 {
                let v: f64 = p
                    .iter()
                    .zip(view.iter())
                    .map(|(a, vr)| a * vr.get(col).copied().unwrap_or(0.0))
                    .sum();
                // column-major: out[col][row]
                if let Some(c) = out.get_mut(col).and_then(|c| c.get_mut(row)) {
                    *c = v as f32;
                }
            }
        }
        out
    }

    /// Where a world point lands in the viewport (pixels from its top left), or `None` behind the camera.
    #[must_use]
    pub fn project(&self, p: V3) -> Option<[f64; 2]> {
        let d = sub(p, self.eye);
        let z = -dot(d, self.back);
        if z <= self.near {
            return None;
        }
        let aspect = self.size[0] / self.size[1].max(1.0);
        let (x, y) = match self.ortho {
            Some(h) => (dot(d, self.right) / (h * aspect), dot(d, self.up) / h),
            None => (
                dot(d, self.right) * self.focal / (aspect * z),
                dot(d, self.up) * self.focal / z,
            ),
        };
        Some([
            f64::midpoint(x, 1.0) * self.size[0],
            f64::midpoint(1.0, -y) * self.size[1],
        ])
    }

    /// The ray through a viewport pixel: origin and unit direction.
    #[must_use]
    pub fn ray(&self, px: [f64; 2]) -> (V3, V3) {
        let aspect = self.size[0] / self.size[1].max(1.0);
        if let Some(h) = self.ortho {
            let x = (px[0] / self.size[0] * 2.0 - 1.0) * aspect * h;
            let y = (1.0 - px[1] / self.size[1] * 2.0) * h;
            return (
                add(self.eye, add(mul(self.right, x), mul(self.up, y))),
                mul(self.back, -1.0),
            );
        }
        let x = (px[0] / self.size[0] * 2.0 - 1.0) * aspect / self.focal;
        let y = (1.0 - px[1] / self.size[1] * 2.0) / self.focal;
        let d = norm(add(add(mul(self.right, x), mul(self.up, y)), mul(self.back, -1.0)));
        (self.eye, d)
    }
}

/// The distance [`Camera::focus`] uses for an object of `size` metres.
#[must_use]
pub fn focus_distance(size: f64) -> f64 {
    (size * 2.5).clamp(15.0, 2000.0)
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn clip(m: &[[f32; 4]; 4], p: V3) -> [f64; 4] {
        std::array::from_fn(|r| (0..3).map(|c| f64::from(m[c][r]) * p[c]).sum::<f64>() + f64::from(m[3][r]))
    }

    #[test]
    fn matrix_and_project_agree() {
        let mut c = Camera::default();
        c.focus([100.0, 20.0, -50.0], Some(80.0));
        let v = c.view([800.0, 600.0]);
        let m = v.matrix();
        for p in [[100.0, 20.0, -50.0], [110.0, 25.0, -40.0], [90.0, 0.0, -70.0]] {
            let q = clip(&m, p);
            let ndc = [q[0] / q[3], q[1] / q[3], q[2] / q[3]];
            let px = v.project(p).unwrap_or([f64::NAN; 2]);
            assert!(((ndc[0] + 1.0) * 400.0 - px[0]).abs() < 1e-3);
            assert!(((1.0 - ndc[1]) * 300.0 - px[1]).abs() < 1e-3);
            assert!(ndc[2] > 0.0 && ndc[2] < 1.0);
        }
        // the target is in the middle; a ray through a projected point passes through it
        let mid = v.project(c.target).unwrap_or_default();
        assert!((mid[0] - 400.0).abs() < 1e-6 && (mid[1] - 300.0).abs() < 1e-6);
        let p = [110.0, 25.0, -40.0];
        let (o, d) = v.ray(v.project(p).unwrap_or_default());
        let t = dot(sub(p, o), d);
        let closest = add(o, mul(d, t));
        assert!(dot(sub(closest, p), sub(closest, p)).sqrt() < 1e-6);
        // behind the camera
        assert!(v.project(add(v.eye, mul(v.back, 10.0))).is_none());
    }

    #[test]
    fn orthographic_views() {
        for proj in [Projection::Top, Projection::Front, Projection::Side] {
            let mut c = Camera::default().with_projection(proj);
            c.focus([100.0, 20.0, -50.0], Some(80.0));
            let v = c.view([800.0, 600.0]);
            assert_eq!(v.ortho, Some(40.0));
            let m = v.matrix();
            for p in [[100.0, 20.0, -50.0], [110.0, 25.0, -40.0], [90.0, 0.0, -70.0]] {
                let q = clip(&m, p);
                assert!((q[3] - 1.0).abs() < 1e-9, "{proj:?}: w is 1");
                let px = v.project(p).unwrap_or([f64::NAN; 2]);
                assert!(((q[0] + 1.0) * 400.0 - px[0]).abs() < 1e-3, "{proj:?}");
                assert!(((1.0 - q[1]) * 300.0 - px[1]).abs() < 1e-3, "{proj:?}");
                assert!(q[2] > 0.0 && q[2] < 1.0, "{proj:?}: depth {}", q[2]);
                // the ray through the pixel passes through the point, along the view direction
                let (o, d) = v.ray(px);
                let t = dot(sub(p, o), d);
                let closest = add(o, mul(d, t));
                assert!(dot(sub(closest, p), sub(closest, p)).sqrt() < 1e-6, "{proj:?}");
            }
            // 40 m above the middle is the top edge
            let (_, up, _) = proj.basis().unwrap_or_default();
            let top = v.project(add(c.target, mul(up, 40.0))).unwrap_or_default();
            assert!(top[1].abs() < 1e-6, "{proj:?}");
            // orbiting does nothing; panning moves the target in the view's plane
            let before = c;
            c.orbit(50.0, 50.0);
            assert_eq!(c, before);
            c.pan(80.0, 0.0, 600.0);
            let moved = dot(sub(before.target, c.target), sub(before.target, c.target)).sqrt();
            assert!((moved - 80.0 * 80.0 / 600.0).abs() < 1e-9, "{proj:?}: {moved}");
        }
        // a top view looks straight down
        let v = Camera::default().with_projection(Projection::Top).view([100.0, 100.0]);
        assert_eq!(v.ray([50.0, 50.0]).1, [0.0, -1.0, 0.0]);
    }

    #[test]
    fn moves() {
        let mut c = Camera::default();
        let eye = c.eye();
        c.look(30.0, 10.0);
        let e2 = c.eye();
        assert!(dot(sub(eye, e2), sub(eye, e2)).sqrt() < 1e-9, "looking keeps the eye");
        c.zoom(1.0);
        assert!(c.distance < 300.0);
        let t = c.target;
        c.fly(10.0, 0.0, 0.0);
        assert!((dot(sub(c.target, t), sub(c.target, t)).sqrt() - 10.0).abs() < 1e-9);
        c.frame([0.0; 3], [100.0, 0.0, 0.0]);
        assert_eq!(c.target, [50.0, 0.0, 0.0]);
    }
}
