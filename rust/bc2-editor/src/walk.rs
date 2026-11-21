//! The player's-eye camera (round VW1, the user's request of 2026-11-20): the 3D view at a soldier's eye, walking on
//! the surface, with the game's own numbers read from its data:
//!
//! - **eye height**: `Physics.CharacterPoseData.EyePosition` of `Objects/Soldiers/Common/DefaultSoldierPhysics`
//!   (stand 1.6 m, crouch 1.0 m; BC2 has no prone pose: its soldier physics has only those two), with the pose's
//!   `StepHeight` (0.3 m) for what the walk climbs;
//! - **speed**: the on-ground state's `CharacterStatePoseInfo.Velocity` per pose (stand 4 m/s, crouch 2.5 m/s) and
//!   `SprintMultiplier` (1.5) with Shift;
//! - **field of view**: the level's `LevelData.DefaultFOV` (55 degrees on every level) times its
//!   `InfantryFOVMultiplier` (1 on the multiplayer levels, 0.82 on the campaign's), as a **vertical** field of view
//!   with the width following the window's aspect (Hor+: a wider window sees more at the sides, never less above and
//!   below; how Frostbite 1 treats it, the editor's cameras too - not proved against the executable in this round).
//!
//! The walk follows the ground with the editor's CPU ray cast ([`crate::query`]: terrain and collision): each step
//! casts down from the step height above the feet, so it climbs what the soldier could step onto and walks through
//! walls (a viewer, not the game's physics). Esc returns to the camera it started from.

use std::path::PathBuf;

use bc2_formats::dbx::{self, Node};
use bc2_scene::scene::Scene;

use crate::camera::{Camera, Projection, V3};

/// The soldier's numbers the walk uses, and where they came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Soldier {
    /// Standing eye height (m).
    pub stand: f64,
    /// Crouching eye height (m).
    pub crouch: f64,
    /// What a step climbs (m).
    pub step: f64,
    /// Walking speed standing and crouching (m/s).
    pub speed: [f64; 2],
    /// Sprint's multiplier.
    pub sprint: f64,
    /// The level's `DefaultFOV` (degrees).
    pub default_fov: f64,
    /// The level's `InfantryFOVMultiplier`.
    pub multiplier: f64,
    /// Where the numbers came from (or which fell back to the defaults, and why).
    pub source: String,
}

impl Default for Soldier {
    /// DICE's values (`DefaultSoldierPhysics`, a multiplayer `LevelData`), for a level whose data cannot be read.
    fn default() -> Self {
        Self {
            stand: 1.6,
            crouch: 1.0,
            step: 0.3,
            speed: [4.0, 2.5],
            sprint: 1.5,
            default_fov: 55.0,
            multiplier: 1.0,
            source: "DICE's values (the level's data was not read)".into(),
        }
    }
}

impl Soldier {
    /// The vertical field of view (degrees): `DefaultFOV x InfantryFOVMultiplier`.
    #[must_use]
    pub fn fov(&self) -> f64 {
        self.default_fov * self.multiplier
    }

    /// The horizontal field of view (degrees) in a window of `aspect` (width / height).
    #[must_use]
    pub fn horizontal_fov(&self, aspect: f64) -> f64 {
        2.0 * ((self.fov().to_radians() / 2.0).tan() * aspect).atan().to_degrees()
    }
}

fn num(n: &Node<'_, '_>, f: &str) -> Option<f64> {
    n.child(f)?.value().ok()?.trim().parse().ok()
}

/// The soldier's numbers from a level: its `LevelData` (field of view) and the soldier physics in its folders.
#[must_use]
pub fn soldier(scene: &Scene, bases: &[PathBuf]) -> Soldier {
    let mut out = Soldier::default();
    let mut found = Vec::new();
    if let Some(Ok(tree)) = scene.tree(0)
        && let Some(level) = tree
            .instances()
            .find(|i| i.attr("type").is_some_and(|t| t.ends_with(".LevelData")))
        && let Some(fov) = num(&level, "DefaultFOV")
    {
        out.default_fov = fov;
        out.multiplier = num(&level, "InfantryFOVMultiplier").unwrap_or(1.0);
        found.push(format!(
            "LevelData: DefaultFOV {fov}, InfantryFOVMultiplier {}",
            out.multiplier
        ));
    }
    let physics = bases.iter().find_map(|b| {
        let f = b
            .join("Objects")
            .join("Soldiers")
            .join("Common")
            .join("DefaultSoldierPhysics.dbx");
        std::fs::read(f).ok().filter(|d| dbx::is_binary(d))
    });
    if let Some(data) = physics
        && let Ok(tree) = dbx::parse_tree(&data)
    {
        let guids: std::collections::HashMap<String, Node<'_, '_>> = tree
            .instances()
            .filter_map(|i| Some((i.attr("guid")?.to_lowercase(), i)))
            .collect();
        let pose = |n: &Node<'_, '_>| n.child("PoseType").and_then(|p| p.value().ok()).unwrap_or_default();
        for i in tree.instances() {
            match i.attr("type").unwrap_or_default() {
                "Physics.CharacterPoseData" => {
                    // the single as the data writes it (1.6, not 1.6000000238)
                    let eye = i
                        .child("EyePosition")
                        .and_then(|e| e.floats())
                        .and_then(|v| v.get(1).copied())
                        .and_then(|y| y.to_string().parse::<f64>().ok());
                    match (pose(&i).as_str(), eye) {
                        ("Stand", Some(y)) => {
                            out.stand = y;
                            out.step = num(&i, "StepHeight").unwrap_or(out.step);
                        }
                        ("Crouch", Some(y)) => out.crouch = y,
                        _ => {}
                    }
                }
                "Physics.OnGroundStateData" => {
                    for r in i.child("PoseInfo").map_or_else(Vec::new, |a| {
                        a.children().filter_map(|c| c.attr("ref")).collect::<Vec<_>>()
                    }) {
                        let Some(p) = guids.get(&r.to_lowercase()) else {
                            continue;
                        };
                        let v = num(p, "Velocity");
                        match (pose(p).as_str(), v) {
                            ("Stand", Some(v)) => {
                                out.speed[0] = v;
                                out.sprint = num(p, "SprintMultiplier").unwrap_or(out.sprint);
                            }
                            ("Crouch", Some(v)) => out.speed[1] = v,
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        found.push(format!(
            "DefaultSoldierPhysics: eye {} / {} m, step {} m, {} / {} m/s, sprint x{}",
            out.stand, out.crouch, out.step, out.speed[0], out.speed[1], out.sprint
        ));
    }
    if !found.is_empty() {
        out.source = found.join("; ");
    }
    out
}

/// The walk under way.
#[derive(Debug, Clone, PartialEq)]
pub struct Walk {
    /// The camera it started from (Esc goes back to it).
    pub previous: Camera,
    /// Where the soldier stands.
    pub feet: V3,
    /// Around Y, radians (the editor camera's convention).
    pub yaw: f64,
    /// Up and down, radians.
    pub pitch: f64,
    /// Crouching.
    pub crouch: bool,
    /// The numbers.
    pub soldier: Soldier,
}

impl Walk {
    /// A walk starting below where `camera` looks (its target), facing its way.
    #[must_use]
    pub fn start(camera: &Camera, soldier: Soldier, ground: impl Fn(V3, f64) -> Option<f64>) -> Self {
        let t = camera.target;
        // the surface at the target (a little above it, so not a roof or a canopy over it), else the highest below
        let y = ground([t[0], t[1] + 2.0, t[2]], 2000.0)
            .or_else(|| ground([t[0], t[1] + 2000.0, t[2]], 4000.0))
            .unwrap_or(t[1]);
        Self {
            previous: *camera,
            feet: [t[0], y, t[2]],
            yaw: camera.yaw,
            pitch: 0.0,
            crouch: false,
            soldier,
        }
    }

    /// The eye's height above the feet.
    #[must_use]
    pub fn eye_height(&self) -> f64 {
        if self.crouch {
            self.soldier.crouch
        } else {
            self.soldier.stand
        }
    }

    /// Where the eye is.
    #[must_use]
    pub fn eye(&self) -> V3 {
        [self.feet[0], self.feet[1] + self.eye_height(), self.feet[2]]
    }

    /// The camera at the eye: perspective, the soldier's field of view, a short distance to its target (so its near
    /// plane is close).
    #[must_use]
    pub fn camera(&self) -> Camera {
        let mut c = Camera {
            target: [0.0; 3],
            yaw: self.yaw,
            pitch: self.pitch,
            distance: 1.0,
            fov: self.soldier.fov(),
            projection: Projection::Perspective,
        };
        let d = c.direction();
        let e = self.eye();
        c.target = [e[0] - d[0], e[1] - d[1], e[2] - d[2]];
        c
    }

    /// Looks around by a mouse move in pixels (as the fly mode looks; the game's look limits are the pose's, +-90
    /// degrees of pitch kept just short).
    pub fn look(&mut self, dx: f64, dy: f64) {
        self.yaw -= dx * 0.004;
        self.pitch = (self.pitch + dy * 0.004).clamp(-1.55, 1.55);
    }

    /// Walks `forward` and `right` (-1 to 1 each) for `dt` seconds at the pose's speed (sprinting: times the sprint
    /// multiplier), then stands on the ground: `ground(from, depth)` is the surface's height below `from` within
    /// `depth`, or `None`.
    pub fn step(&mut self, forward: f64, right: f64, dt: f64, sprint: bool, ground: impl Fn(V3, f64) -> Option<f64>) {
        let pose = if self.crouch {
            self.soldier.speed[1]
        } else {
            self.soldier.speed[0]
        };
        let speed = pose * if sprint { self.soldier.sprint } else { 1.0 };
        // flat directions: where the camera looks, and its right
        let (s, c) = self.yaw.sin_cos();
        let fwd = [-s, 0.0, -c];
        let side = [c, 0.0, -s];
        let len = (forward * forward + right * right).sqrt().max(1.0);
        let d = speed * dt / len;
        let to = [
            self.feet[0] + (fwd[0] * forward + side[0] * right) * d,
            self.feet[1],
            self.feet[2] + (fwd[2] * forward + side[2] * right) * d,
        ];
        self.settle(to, &ground);
    }

    /// Stands at `to`'s x and z on the ground below its step height (keeps its height where there is none).
    pub fn settle(&mut self, to: V3, ground: impl Fn(V3, f64) -> Option<f64>) {
        let from = [to[0], to[1] + self.soldier.step, to[2]];
        // down from the step height: a step it can climb, the ground below it, or (falling) far below
        let y = ground(from, 500.0).unwrap_or(to[1]);
        self.feet = [to[0], y, to[2]];
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn walks_on_a_slope_at_eye_height() {
        let s = Soldier::default();
        assert_eq!(s.fov(), 55.0);
        assert!(
            (s.horizontal_fov(16.0 / 9.0) - 85.57).abs() < 0.01,
            "{}",
            s.horizontal_fov(16.0 / 9.0)
        );
        let ground = |p: V3, depth: f64| {
            let h = p[0] * 0.1; // a 10 % slope up along +X
            (h <= p[1] && p[1] - h <= depth).then_some(h)
        };
        let cam = Camera {
            target: [0.0, 50.0, 0.0],
            yaw: -std::f64::consts::FRAC_PI_2, // looking along +X
            ..Camera::default()
        };
        let mut w = Walk::start(&cam, s, ground);
        assert_eq!(w.feet, [0.0, 0.0, 0.0]);
        for _ in 0..100 {
            w.step(1.0, 0.0, 0.05, false, ground);
        }
        // 5 s at 4 m/s along +X, the eye 1.6 m above the slope
        assert!((w.feet[0] - 20.0).abs() < 0.2, "{:?}", w.feet);
        assert!((w.eye()[1] - (w.feet[0] * 0.1 + 1.6)).abs() < 1e-9);
        let c = w.camera();
        assert!((c.eye()[1] - w.eye()[1]).abs() < 1e-9 && c.fov == 55.0);
        w.crouch = true;
        assert_eq!(w.eye_height(), 1.0);
    }
}
