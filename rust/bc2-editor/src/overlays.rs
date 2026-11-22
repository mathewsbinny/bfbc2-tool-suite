//! The view's overlays (round VW1, the user's request of 2026-11-20): effects, dynamic lights and sound emitters
//! with their reach, and the gameplay areas as the game uses them, each behind a View toggle ([`Toggles`]), with
//! labels. Every size and shape is read from the level's partitions (DBX), never made up:
//!
//! - **Effects**: the placed effects (`GameEffectEntityData.Effect`, and placed objects whose blueprint is an effect)
//!   and each emitter of the effect's blueprint (`EmitterEntityData`, at its `Transform` in the effect), with the
//!   emitter's spawn volume where its document gives one: `SpawnPositionData.Pre`'s `SphereEvaluatorData` (`Radius`,
//!   scaled per axis by `Scale`, at `Pivot`) or `BoxEvaluatorData` (`Dimensions` drawn as extents from `Pivot`). The
//!   particles themselves are not drawn.
//! - **Lights**: BC2 has no light entities; its dynamic lights are emitters whose template acts as a point light
//!   (`EmitterTemplateData.ActAsPointLight`): a sphere of `PointLightRadius` at the emitter plus `PointLightPivot`,
//!   its marker's swatch `PointLightColor` (HDR: scaled to its largest channel). No spot lights exist in the data.
//! - **Sounds**: sound emitters (`GameSoundEntityData.Sound` in the level, `SoundEntityData.SoundTemplate` in the
//!   effects) with the template's minimum distance (the largest of `MasterMinDistance` and its positional emitters'
//!   `MinDistance`: full volume inside). BC2's sounds have **no maximum distance**: past the minimum the level falls
//!   off logarithmically by `DistanceSlope` (`AttenuationType` `DatLogarithmic`), so the label says the slope
//!   instead of a made-up range. Sound areas (`SoundAreaEntityData`) are drawn with their inner `Geometry` and their
//!   `OuterGeometry` (the fade edge, `FadeWidth`).
//! - **Combat areas**: `CombatAreaTriggerEntityData.Geometry` (the in-bounds edge, drawn as a ribbon wall: its
//!   points' `Height` where the data gives one, else 20 m above the ground and labelled unbounded) and
//!   `SurroundingGeometry` (the outer edge); the team's colour with `UseTeamRestriction`, else `view-unowned`.
//! - **Capture areas**: `CapturePointEntityData.Geometry` (an area chain: a prism of its points' `Height`; or a
//!   `SphereGeometryEntityData` of `Radius`, `MP_005`'s islands; or an `AabbGeometryEntityData` of `Extents`) and
//!   `CaptureRadius` (a
//!   circle, a sphere with `IsCapturedInUpperSphere`) where it is above 0; `InitialOwnerTeam`'s colour; an HQ
//!   (`UnableToChangeTeam`) says so.
//! - **Spawn protection**: `SoldierSpawnEntityData.SpawnEnemyRadius` around each of its `AlternativeSpawnPoints`
//!   (the spawner's own position when it has none): the radius the spawn checks for enemies. BC2 has no other spawn
//!   protection volume in its level data (no safe-zone entity; `ContinueEntityData.SoldierImmortalTimer` is a time).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use bc2_formats::dbx::{self, DbxTree, Node};
use bc2_scene::scene::Scene;
use eframe::egui::Color32;
use serde::{Deserialize, Serialize};

use crate::theme::Palette;

/// A point in world space.
pub type V3 = [f64; 3];
/// A line segment in world space.
pub type Segment = [[f32; 3]; 2];
/// A transform: rows right, up, forward, translation (the data's 16 floats; the fourth of each row unused).
pub type M = [f64; 16];

/// Segments of a circle or sphere: the more, the closer a drawn radius is to the data's.
const STEPS: usize = 48;
/// A combat area's ribbon height where its data has none (metres above the ground).
pub const RIBBON: f64 = 20.0;

/// The overlay categories, each a View toggle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Overlay {
    /// Particle effect emitters and their spawn volumes.
    Effects,
    /// Dynamic point lights and their radius.
    Lights,
    /// Sound emitters (minimum distance) and sound areas.
    Sounds,
    /// Combat areas and their out-of-bounds edges.
    CombatAreas,
    /// Flags' and capture points' capture areas, HQs.
    CaptureAreas,
    /// Spawn points' enemy-check radius.
    SpawnProtection,
}

impl Overlay {
    /// Every overlay, in the View menu's order.
    pub const ALL: [Self; 6] = [
        Self::Effects,
        Self::Lights,
        Self::Sounds,
        Self::CombatAreas,
        Self::CaptureAreas,
        Self::SpawnProtection,
    ];

    /// Its name in the View menu.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Effects => "Effects",
            Self::Lights => "Lights",
            Self::Sounds => "Sounds",
            Self::CombatAreas => "Combat areas",
            Self::CaptureAreas => "Capture areas",
            Self::SpawnProtection => "Spawn protection",
        }
    }

    /// What it shows, in a line.
    #[must_use]
    pub const fn help(self) -> &'static str {
        match self {
            Self::Effects => "Particle effect emitters and their spawn volumes (the particles are not drawn)",
            Self::Lights => "Dynamic point lights (emitters that act as lights): their radius, their colour",
            Self::Sounds => {
                "Sound emitters with their minimum distance, sound areas with their fade edge (BC2's sounds have no \
                 maximum distance)"
            }
            Self::CombatAreas => "Combat areas: the out-of-bounds edge as a wall, the outer edge, the team they bound",
            Self::CaptureAreas => "Flags' capture areas (polygon and height, or capture radius), HQs",
            Self::SpawnProtection => "Spawn points' enemy-check radius (SpawnEnemyRadius)",
        }
    }
}

/// Which overlays the view draws (a view setting, kept with the editor's settings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools, reason = "one switch per View toggle")]
pub struct Toggles {
    /// Effects.
    #[serde(default)]
    pub effects: bool,
    /// Lights.
    #[serde(default)]
    pub lights: bool,
    /// Sounds.
    #[serde(default)]
    pub sounds: bool,
    /// Combat areas (on at the start: the view drew their outlines before round VW1).
    #[serde(default = "yes")]
    pub combat: bool,
    /// Capture areas (on at the start, as the combat areas).
    #[serde(default = "yes")]
    pub capture: bool,
    /// Spawn protection.
    #[serde(default)]
    pub spawn: bool,
    /// The overlays' labels.
    #[serde(default = "yes")]
    pub labels: bool,
}

const fn yes() -> bool {
    true
}

impl Default for Toggles {
    fn default() -> Self {
        Self {
            effects: false,
            lights: false,
            sounds: false,
            combat: true,
            capture: true,
            spawn: false,
            labels: true,
        }
    }
}

impl Toggles {
    /// Whether an overlay is on.
    #[must_use]
    pub const fn on(&self, o: Overlay) -> bool {
        match o {
            Overlay::Effects => self.effects,
            Overlay::Lights => self.lights,
            Overlay::Sounds => self.sounds,
            Overlay::CombatAreas => self.combat,
            Overlay::CaptureAreas => self.capture,
            Overlay::SpawnProtection => self.spawn,
        }
    }

    /// Turns an overlay on or off.
    pub fn set(&mut self, o: Overlay, on: bool) {
        match o {
            Overlay::Effects => self.effects = on,
            Overlay::Lights => self.lights = on,
            Overlay::Sounds => self.sounds = on,
            Overlay::CombatAreas => self.combat = on,
            Overlay::CaptureAreas => self.capture = on,
            Overlay::SpawnProtection => self.spawn = on,
        }
    }

    /// Whether any overlay is on.
    #[must_use]
    pub fn any(&self) -> bool {
        Overlay::ALL.iter().any(|o| self.on(*o))
    }
}

/// Which token an item is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paint {
    /// `view-fx-effect`.
    Effect,
    /// `view-fx-light`.
    Light,
    /// `view-fx-sound`.
    Sound,
    /// `view-team-1`, `view-team-2`, `view-team-neutral` (0).
    Team(u8),
    /// `view-unowned`: gameplay no team owns.
    Unowned,
}

impl Paint {
    /// Its colour.
    #[must_use]
    pub fn colour(self, p: &Palette) -> Color32 {
        match self {
            Self::Effect => p.view_fx_effect,
            Self::Light => p.view_fx_light,
            Self::Sound => p.view_fx_sound,
            Self::Team(t) => p.view_team(t),
            Self::Unowned => p.view_unowned,
        }
    }
}

/// One thing an overlay draws.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// Its overlay.
    pub overlay: Overlay,
    /// The level entity it belongs to (the effect, the sound, the area, the flag, the spawner).
    pub entity: Option<usize>,
    /// Where its label goes (and its centre for lights, sounds, capture radii and spawn checks).
    pub at: V3,
    /// Its label.
    pub label: String,
    /// The radius drawn, metres: a light's, a sound's minimum, a capture radius, a spawn check (`None`: shapes).
    pub radius: Option<f64>,
    /// A light's own colour (its marker's swatch).
    pub swatch: Option<Color32>,
    /// Its colour.
    pub paint: Paint,
    /// Its lines.
    pub lines: Vec<Segment>,
}

/// A level's overlays, read once ([`load`]).
#[derive(Debug, Clone, Default)]
pub struct Overlays {
    /// Everything they draw.
    pub items: Vec<Item>,
    /// The entities whose outlines the overlays draw instead of the scene's (combat areas, capture points, sound
    /// areas).
    pub covered: Vec<usize>,
    /// What could not be read.
    pub notes: Vec<String>,
    /// The time [`load`] took.
    pub seconds: f64,
}

impl Overlays {
    /// The lines the toggles show, each with its colour.
    #[must_use]
    pub fn lines(&self, t: &Toggles, p: &Palette) -> Vec<(Segment, Color32)> {
        self.lines_without(t, p, &|_| false)
    }

    /// [`Overlays::lines`] without the items of the entities `skip` says yes to (round VW2: deleted or hidden by the
    /// project, or filtered out by the mode view layer).
    #[must_use]
    pub fn lines_without(&self, t: &Toggles, p: &Palette, skip: &dyn Fn(usize) -> bool) -> Vec<(Segment, Color32)> {
        self.items
            .iter()
            .filter(|i| t.on(i.overlay))
            .filter(|i| !i.entity.is_some_and(skip))
            .flat_map(|i| {
                let c = i.paint.colour(p);
                i.lines.iter().map(move |s| (*s, c))
            })
            .collect()
    }

    /// How many items an overlay has.
    #[must_use]
    pub fn count(&self, o: Overlay) -> usize {
        self.items.iter().filter(|i| i.overlay == o).count()
    }
}

// ------------------------------------------------------------------------------------------------ geometry

/// A point through a transform.
#[must_use]
pub fn apply(m: &M, p: V3) -> V3 {
    std::array::from_fn(|a| {
        p[0] * m.get(a).copied().unwrap_or(0.0)
            + p[1] * m.get(4 + a).copied().unwrap_or(0.0)
            + p[2] * m.get(8 + a).copied().unwrap_or(0.0)
            + m.get(12 + a).copied().unwrap_or(0.0)
    })
}

/// `local` placed in `parent`'s space (`local` first, then `parent`).
#[must_use]
pub fn compose(local: &M, parent: &M) -> M {
    let mut out = [0.0; 16];
    for row in 0..4 {
        let r = [
            local.get(row * 4).copied().unwrap_or(0.0),
            local.get(row * 4 + 1).copied().unwrap_or(0.0),
            local.get(row * 4 + 2).copied().unwrap_or(0.0),
        ];
        let v = if row == 3 {
            apply(parent, r)
        } else {
            let o = apply(parent, [0.0; 3]);
            let q = apply(parent, r);
            [q[0] - o[0], q[1] - o[1], q[2] - o[2]]
        };
        for (a, value) in v.into_iter().enumerate() {
            if let Some(x) = out.get_mut(row * 4 + a) {
                *x = value;
            }
        }
    }
    out
}

/// The identity transform.
pub const IDENTITY: M = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

fn seg(a: V3, b: V3) -> Segment {
    [a.map(|v| v as f32), b.map(|v| v as f32)]
}

/// A horizontal circle of `r` around `c` (its first point at +X).
#[must_use]
pub fn circle(c: V3, r: f64) -> Vec<Segment> {
    ellipse(&IDENTITY, c, [r, 0.0, r], 1)
}

/// A sphere of `r` around `c`: three great circles.
#[must_use]
pub fn sphere(c: V3, r: f64) -> Vec<Segment> {
    [0, 1, 2]
        .iter()
        .flat_map(|&plane| ellipse(&IDENTITY, c, [r; 3], plane))
        .collect()
}

/// An ellipse of an ellipsoid with radii `radii` (local axes) around local point `c`, in one of its planes (0 XY,
/// 1 XZ, 2 YZ), through `m`.
fn ellipse(m: &M, c: V3, radii: V3, plane: usize) -> Vec<Segment> {
    let at = |k: usize| -> V3 {
        let a = k as f64 / STEPS as f64 * std::f64::consts::TAU;
        let (s, co) = a.sin_cos();
        let p = match plane {
            0 => [c[0] + radii[0] * co, c[1] + radii[1] * s, c[2]],
            1 => [c[0] + radii[0] * co, c[1], c[2] + radii[2] * s],
            _ => [c[0], c[1] + radii[1] * co, c[2] + radii[2] * s],
        };
        apply(m, p)
    };
    (0..STEPS).map(|k| seg(at(k), at(k + 1))).collect()
}

/// An ellipsoid (three ellipses; a flat one drawn once) through `m`.
fn ellipsoid(m: &M, c: V3, radii: V3) -> Vec<Segment> {
    let mut out = Vec::new();
    for (plane, axes) in [(0, [0, 1]), (1, [0, 2]), (2, [1, 2])] {
        if axes.iter().all(|&a| radii.get(a).is_some_and(|r| *r > 0.0)) {
            out.extend(ellipse(m, c, radii, plane));
        }
    }
    out
}

/// A box's 12 edges, extents `e` from local point `c`, through `m`.
fn box_edges(m: &M, c: V3, e: V3) -> Vec<Segment> {
    let corner = |k: usize| -> V3 {
        apply(
            m,
            [
                c[0] + if k & 1 == 0 { -e[0] } else { e[0] },
                c[1] + if k & 2 == 0 { -e[1] } else { e[1] },
                c[2] + if k & 4 == 0 { -e[2] } else { e[2] },
            ],
        )
    };
    let mut out = Vec::new();
    for k in 0..8usize {
        for bit in [1usize, 2, 4] {
            if k & bit == 0 {
                out.push(seg(corner(k), corner(k | bit)));
            }
        }
    }
    out
}

/// A marker: a small cross at `p` (`size` metres).
fn cross(p: V3, size: f64) -> Vec<Segment> {
    [[size, 0.0, 0.0], [0.0, size, 0.0], [0.0, 0.0, size]]
        .iter()
        .map(|d| {
            seg(
                [p[0] - d[0], p[1] - d[1], p[2] - d[2]],
                [p[0] + d[0], p[1] + d[1], p[2] + d[2]],
            )
        })
        .collect()
}

/// A closed polygon's edges at its points.
fn ring(pts: &[V3]) -> Vec<Segment> {
    (0..pts.len())
        .filter_map(|k| Some(seg(*pts.get(k)?, *pts.get((k + 1) % pts.len())?)))
        .collect()
}

/// A wall along a closed polygon: its bottom and top edges, a middle line, and posts at the corners and every 8 m.
fn wall(bottom: &[V3], top: &[V3]) -> Vec<Segment> {
    let mut out = ring(bottom);
    out.extend(ring(top));
    let mid: Vec<V3> = bottom
        .iter()
        .zip(top)
        .map(|(b, t)| [b[0], f64::midpoint(b[1], t[1]), b[2]])
        .collect();
    out.extend(ring(&mid));
    let n = bottom.len();
    for k in 0..n {
        let (Some(b0), Some(t0), Some(b1), Some(t1)) =
            (bottom.get(k), top.get(k), bottom.get((k + 1) % n), top.get((k + 1) % n))
        else {
            continue;
        };
        let len = ((b1[0] - b0[0]).powi(2) + (b1[2] - b0[2]).powi(2)).sqrt();
        let posts = (len / 8.0).floor() as usize;
        for j in 0..=posts {
            let f = if posts == 0 { 0.0 } else { j as f64 / (posts + 1) as f64 };
            let lerp = |a: &V3, b: &V3| -> V3 {
                [
                    a[0] + (b[0] - a[0]) * f,
                    a[1] + (b[1] - a[1]) * f,
                    a[2] + (b[2] - a[2]) * f,
                ]
            };
            out.push(seg(lerp(b0, b1), lerp(t0, t1)));
        }
    }
    out
}

/// The middle of some points.
fn centre(pts: &[V3]) -> V3 {
    let n = pts.len().max(1) as f64;
    let sum = pts
        .iter()
        .fold([0.0; 3], |s, p| [s[0] + p[0], s[1] + p[1], s[2] + p[2]]);
    sum.map(|v| v / n)
}

// ------------------------------------------------------------------------------------------------ reading

/// A field's number.
fn num(n: &Node<'_, '_>, f: &str) -> Option<f64> {
    n.child(f)?.value().ok()?.trim().parse().ok()
}

/// A field's text (enumerations, booleans, names).
fn text(n: &Node<'_, '_>, f: &str) -> String {
    n.child(f).and_then(|c| c.value().ok()).unwrap_or_default()
}

/// A field's first three numbers.
fn vec3(n: &Node<'_, '_>, f: &str) -> Option<V3> {
    let v = n.child(f)?.floats()?;
    Some([f64::from(*v.first()?), f64::from(*v.get(1)?), f64::from(*v.get(2)?)])
}

/// A `Transform` field.
fn transform(n: &Node<'_, '_>) -> Option<M> {
    let v = n.child("Transform")?.floats()?;
    (v.len() == 16).then(|| std::array::from_fn(|k| v.get(k).copied().map_or(0.0, f64::from)))
}

/// A reference field (`None`: null).
fn reference<'t>(n: &Node<'t, '_>, f: &str) -> Option<&'t str> {
    n.child(f)?.attr("ref").filter(|r| !r.is_empty() && *r != "null")
}

/// An array field's references.
fn references<'t>(n: &Node<'t, '_>, f: &str) -> Vec<&'t str> {
    n.child(f)
        .map_or_else(Vec::new, |a| a.children().filter_map(|i| i.attr("ref")).collect())
}

/// The type's last part (`Emitter.EmitterTemplateData` -> `EmitterTemplateData`).
fn short_type<'t>(n: &Node<'t, '_>) -> &'t str {
    let t = n.attr("type").unwrap_or_default();
    t.rsplit('.').next().unwrap_or(t)
}

/// A team field's number (`Team1` 1, `Team2` 2, else 0).
fn team(v: &str) -> u8 {
    match v {
        "Team1" => 1,
        "Team2" => 2,
        _ => 0,
    }
}

/// The partitions a level's data references, found in its folders (a reference's path, the level folders' own
/// `levels\<level>` copies too: the level-specific emitters live there), each read once.
struct Files {
    roots: Vec<PathBuf>,
    cache: HashMap<String, Option<Arc<Vec<u8>>>>,
}

impl Files {
    fn new(bases: &[PathBuf]) -> Self {
        let mut roots = Vec::new();
        for b in bases {
            roots.push(b.clone());
            if let Ok(rd) = std::fs::read_dir(b.join("levels")) {
                let mut subs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
                subs.sort();
                roots.extend(subs);
            }
        }
        Self {
            roots,
            cache: HashMap::new(),
        }
    }

    /// A reference's partition (`fx/a/b/<guid>` reads `fx/a/b.dbx`).
    fn get(&mut self, r: &str) -> Option<Arc<Vec<u8>>> {
        let path = r.rsplit_once('/').map_or(r, |(p, _)| p).to_lowercase();
        if let Some(c) = self.cache.get(&path) {
            return c.clone();
        }
        let found = self.roots.iter().find_map(|root| {
            let mut f = root.clone();
            for part in path.split('/') {
                f.push(part);
            }
            f.set_extension("dbx");
            std::fs::read(&f).ok().filter(|d| dbx::is_binary(d)).map(Arc::new)
        });
        self.cache.insert(path, found.clone());
        found
    }
}

/// An instance by its GUID (lower case) in a partition.
fn by_guid<'t, 'a>(tree: &'t DbxTree<'a>) -> HashMap<String, Node<'t, 'a>> {
    tree.instances()
        .filter_map(|i| Some((i.attr("guid")?.to_lowercase(), i)))
        .collect()
}

/// What an emitter document gives: its point light and its spawn volume.
#[derive(Debug, Clone, Default)]
struct EmitterInfo {
    /// (radius, colour, pivot).
    light: Option<(f64, [f64; 3], V3)>,
    /// The spawn volume's lines in the emitter's space.
    volume: Vec<(bool, V3, V3)>,
}

fn emitter_info(files: &mut Files, r: &str) -> EmitterInfo {
    let mut out = EmitterInfo::default();
    let Some(data) = files.get(r) else { return out };
    let Ok(tree) = dbx::parse_tree(&data) else { return out };
    let guids = by_guid(&tree);
    for i in tree.instances() {
        match short_type(&i) {
            "EmitterTemplateData" if text(&i, "ActAsPointLight") == "true" => {
                out.light = Some((
                    num(&i, "PointLightRadius").unwrap_or(0.0),
                    vec3(&i, "PointLightColor").unwrap_or([1.0; 3]),
                    vec3(&i, "PointLightPivot").unwrap_or([0.0; 3]),
                ));
            }
            "SpawnPositionData" => {
                let Some(ev) = reference(&i, "Pre").and_then(|g| guids.get(&g.to_lowercase())) else {
                    continue;
                };
                let pivot = vec3(ev, "Pivot").unwrap_or([0.0; 3]);
                match short_type(ev) {
                    "SphereEvaluatorData" => {
                        let r = num(ev, "Radius").unwrap_or(0.0);
                        let s = vec3(ev, "Scale").unwrap_or([1.0; 3]);
                        if r > 0.0 {
                            out.volume.push((true, pivot, s.map(|k| k * r)));
                        }
                    }
                    "BoxEvaluatorData" => {
                        if let Some(d) = vec3(ev, "Dimensions").filter(|d| d.iter().any(|v| *v > 0.0)) {
                            out.volume.push((false, pivot, d));
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    out
}

/// A sound template's minimum distance and its positional emitters' slope.
fn sound_info(files: &mut Files, r: &str) -> Option<(f64, f64, String)> {
    let data = files.get(r)?;
    let tree = dbx::parse_tree(&data).ok()?;
    let mut min = 0.0f64;
    let mut slope = 0.0f64;
    let mut name = String::new();
    for i in tree.instances() {
        let t = short_type(&i);
        if t == "SoundTemplate" {
            min = min.max(num(&i, "MasterMinDistance").unwrap_or(0.0));
            name = text(&i, "Name");
        } else if t.starts_with("SoundSource") && t.ends_with("Emitter") && t != "SoundSourceLfeEmitter" {
            min = min.max(num(&i, "MinDistance").unwrap_or(0.0));
            slope = slope.max(num(&i, "DistanceSlope").unwrap_or(0.0));
        }
    }
    Some((min, slope, name))
}

/// What an effect blueprint holds: its emitters and sounds, each at its transform in the effect.
#[derive(Debug, Clone, Default)]
struct EffectInfo {
    name: String,
    emitters: Vec<(M, String)>,
    sounds: Vec<(M, String)>,
}

fn effect_info(files: &mut Files, r: &str) -> Option<EffectInfo> {
    let data = files.get(r)?;
    let tree = dbx::parse_tree(&data).ok()?;
    let mut out = EffectInfo {
        name: r
            .rsplit_once('/')
            .map_or(r, |(p, _)| p)
            .rsplit('/')
            .next()
            .unwrap_or(r)
            .to_owned(),
        ..EffectInfo::default()
    };
    for i in tree.instances() {
        let m = transform(&i).unwrap_or(IDENTITY);
        match short_type(&i) {
            "EmitterEntityData" => {
                if let Some(e) = reference(&i, "Emitter") {
                    out.emitters.push((m, e.to_owned()));
                }
            }
            "SoundEntityData" => {
                if let Some(s) = reference(&i, "SoundTemplate") {
                    out.sounds.push((m, s.to_owned()));
                }
            }
            _ => {}
        }
    }
    Some(out)
}

/// The last part of a reference's path (`sound/x/y_01/<guid>` -> `y_01`).
fn short_name(r: &str) -> &str {
    let p = r.rsplit_once('/').map_or(r, |(p, _)| p);
    p.rsplit('/').next().unwrap_or(p)
}

/// The world points of an area chain (`AreaGeometryEntityData` linked by `Next`) and its first nonzero `Height`.
fn chain(guids: &HashMap<String, Node<'_, '_>>, first: &str) -> (Vec<V3>, f64) {
    let mut pts = Vec::new();
    let mut height = 0.0;
    let mut seen: Vec<String> = Vec::new();
    let mut at = Some(first.to_lowercase());
    while let Some(g) = at {
        if seen.contains(&g) || seen.len() > 4096 {
            break;
        }
        let Some(n) = guids.get(&g) else { break };
        let Some(m) = transform(n) else { break };
        pts.push([m[12], m[13], m[14]]);
        if height == 0.0 {
            height = num(n, "Height").unwrap_or(0.0);
        }
        seen.push(g);
        at = reference(n, "Next").map(str::to_lowercase);
    }
    (pts, height)
}

/// Reads a level's overlays from its partition and the partitions it references (`bases`: the level's folders).
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "one pass over the level's instances, a case per kind"
)]
pub fn load(scene: &Scene, bases: &[PathBuf]) -> Overlays {
    let t0 = std::time::Instant::now();
    let mut out = Overlays::default();
    let Some((_, data)) = scene.file(0) else {
        out.notes.push("The scene has no level partition: no overlays".into());
        return out;
    };
    let Ok(tree) = dbx::parse_tree(data) else {
        out.notes.push("The level partition does not read: no overlays".into());
        return out;
    };
    let guids = by_guid(&tree);
    let entity_of: HashMap<String, usize> = scene
        .entities
        .iter()
        .map(|e| (e.guid.to_lowercase(), e.index))
        .collect();
    let ground = |x: f64, z: f64, y: f64| -> f64 {
        scene
            .heightfield
            .as_ref()
            .map(|h| h.height(x, z))
            .filter(|h| h.is_finite())
            .unwrap_or(y)
    };
    let mut files = Files::new(bases);
    let mut effects: HashMap<String, Option<EffectInfo>> = HashMap::new();
    let mut emitters: HashMap<String, EmitterInfo> = HashMap::new();
    let mut sounds: HashMap<String, Option<(f64, f64, String)>> = HashMap::new();
    // the placed effects: GameEffectEntityData in the level, and placed objects whose blueprint is an effect
    let mut placed: Vec<(usize, M, String)> = Vec::new();
    for i in tree.instances() {
        let Some(k) = i.attr("guid").and_then(|g| entity_of.get(&g.to_lowercase())).copied() else {
            continue;
        };
        let Some(m) = scene.entities.get(k).and_then(|e| e.matrix) else {
            continue;
        };
        match short_type(&i) {
            "GameEffectEntityData" => {
                if let Some(r) = reference(&i, "Effect") {
                    placed.push((k, m, r.to_owned()));
                }
            }
            "ReferenceObjectData" if scene.entities.get(k).is_some_and(|e| e.category == "effect") => {
                if let Some(r) = reference(&i, "ReferencedObject") {
                    placed.push((k, m, r.to_owned()));
                }
            }
            _ => {}
        }
    }
    for (k, m, r) in &placed {
        let info = effects
            .entry(r.to_lowercase())
            .or_insert_with(|| effect_info(&mut files, r))
            .clone();
        let Some(info) = info else {
            out.notes
                .push(format!("effect {} not found in the level's folders", short_name(r)));
            continue;
        };
        let p = [m[12], m[13], m[14]];
        let mut lines = cross(p, 0.6);
        for (local, e) in &info.emitters {
            let em = emitters
                .entry(e.to_lowercase())
                .or_insert_with(|| emitter_info(&mut files, e))
                .clone();
            let world = compose(local, m);
            for (sphere, c, size) in &em.volume {
                if *sphere {
                    lines.extend(ellipsoid(&world, *c, *size));
                } else {
                    lines.extend(box_edges(&world, *c, *size));
                }
            }
            if let Some((radius, colour, pivot)) = em.light {
                let at = apply(&world, pivot);
                let top = colour.iter().copied().fold(0.0f64, f64::max).max(1e-6);
                let c = colour.map(|v| ((v / top).clamp(0.0, 1.0) * 255.0).round() as u8);
                let mut l = sphere(at, radius);
                l.extend(cross(at, 0.4));
                out.items.push(Item {
                    overlay: Overlay::Lights,
                    entity: Some(*k),
                    at,
                    label: format!("Light {radius:.0} m ({})", info.name),
                    radius: Some(radius),
                    swatch: Some(Color32::from_rgb(c[0], c[1], c[2])),
                    paint: Paint::Light,
                    lines: l,
                });
            }
        }
        out.items.push(Item {
            overlay: Overlay::Effects,
            entity: Some(*k),
            at: p,
            label: format!("Effect {} ({} emitters)", info.name, info.emitters.len()),
            radius: None,
            swatch: None,
            paint: Paint::Effect,
            lines,
        });
        for (local, s) in &info.sounds {
            let Some((min, slope, _)) = sounds
                .entry(s.to_lowercase())
                .or_insert_with(|| sound_info(&mut files, s))
                .clone()
            else {
                continue;
            };
            let at = apply(&compose(local, m), [0.0; 3]);
            out.items.push(sound_item(Some(*k), at, min, slope, short_name(s)));
        }
    }
    // the level's own instances: sounds, sound areas, combat areas, capture points, spawners
    for i in tree.instances() {
        let k = i.attr("guid").and_then(|g| entity_of.get(&g.to_lowercase())).copied();
        let m = k.and_then(|k| scene.entities.get(k)?.matrix).or_else(|| transform(&i));
        let here = m.map(|m| [m[12], m[13], m[14]]);
        match short_type(&i) {
            "GameSoundEntityData" => {
                let (Some(r), Some(p)) = (reference(&i, "Sound"), here) else {
                    continue;
                };
                let Some((min, slope, _)) = sounds
                    .entry(r.to_lowercase())
                    .or_insert_with(|| sound_info(&mut files, r))
                    .clone()
                else {
                    out.notes
                        .push(format!("sound {} not found in the level's folders", short_name(r)));
                    continue;
                };
                out.items.push(sound_item(k, p, min, slope, short_name(r)));
            }
            "SoundAreaEntityData" => {
                let mut lines = Vec::new();
                let mut anchor = here.unwrap_or_default();
                for f in ["Geometry", "OuterGeometry"] {
                    for r in references(&i, f) {
                        let (pts, _) = chain(&guids, r);
                        if pts.len() >= 3 {
                            if f == "Geometry" {
                                anchor = centre(&pts);
                            }
                            lines.extend(ring(&pts));
                        }
                    }
                }
                if lines.is_empty() {
                    continue;
                }
                if let Some(k) = k {
                    out.covered.push(k);
                }
                let fade = num(&i, "FadeWidth").unwrap_or(0.0);
                let sound = reference(&i, "Sound").map_or("(none)", short_name);
                out.items.push(Item {
                    overlay: Overlay::Sounds,
                    entity: k,
                    at: anchor,
                    label: format!("Sound area {sound} (fade {fade:.0} m)"),
                    radius: None,
                    swatch: None,
                    paint: Paint::Sound,
                    lines,
                });
            }
            "CombatAreaTriggerEntityData" | "CapturePointEntityData" => {
                if let Some((item, cover)) = area_item(&i, k, here, &guids, &ground) {
                    if let Some(c) = cover {
                        out.covered.push(c);
                    }
                    out.items.push(item);
                }
            }
            "SoldierSpawnEntityData" => {
                let radius = num(&i, "SpawnEnemyRadius").unwrap_or(0.0);
                let Some(p) = here.filter(|_| radius > 0.0) else {
                    continue;
                };
                let t = team(&text(&i, "Team"));
                let mut points: Vec<V3> = references(&i, "AlternativeSpawnPoints")
                    .iter()
                    .filter_map(|r| scene.entities.get(*entity_of.get(&r.to_lowercase())?)?.position())
                    .collect();
                if points.is_empty() {
                    points.push(p);
                }
                let lines = points.iter().flat_map(|q| circle(*q, radius)).collect();
                let name = text(&i, "Name");
                out.items.push(Item {
                    overlay: Overlay::SpawnProtection,
                    entity: k,
                    at: p,
                    label: format!(
                        "Spawn check {radius:.0} m: {} ({} points)",
                        if name.is_empty() { "spawner" } else { name.as_str() },
                        points.len()
                    ),
                    radius: Some(radius),
                    swatch: None,
                    paint: Paint::Team(t),
                    lines,
                });
            }
            _ => {}
        }
    }
    out.covered.sort_unstable();
    out.covered.dedup();
    out.notes.sort();
    out.notes.dedup();
    out.seconds = t0.elapsed().as_secs_f64();
    out
}

/// The overlays with a project's gameplay areas (round A2, VW1's leftover): the base level's effects, lights, sounds
/// and spawn checks as [`load`] read them, and the combat and capture areas read again from the project's partition
/// (`partition`: its bytes as the log leaves them), so the areas a project adds (a mode layer's) or moves (an accepted
/// auto layout) show where they stand now. An area of an object the scene has keeps its entity (hidden or deleted
/// with it); a new one has none. `None`: the partition does not read.
#[must_use]
pub fn with_project(base: &Overlays, scene: &Scene, partition: &[u8]) -> Option<Overlays> {
    let t0 = std::time::Instant::now();
    let tree = dbx::parse_tree(partition).ok()?;
    let guids = by_guid(&tree);
    let entity_of: HashMap<String, usize> = scene
        .entities
        .iter()
        .map(|e| (e.guid.to_lowercase(), e.index))
        .collect();
    let ground = |x: f64, z: f64, y: f64| -> f64 {
        scene
            .heightfield
            .as_ref()
            .map(|h| h.height(x, z))
            .filter(|h| h.is_finite())
            .unwrap_or(y)
    };
    let area = |o: Overlay| matches!(o, Overlay::CombatAreas | Overlay::CaptureAreas);
    let mut out = Overlays {
        items: base.items.iter().filter(|i| !area(i.overlay)).cloned().collect(),
        covered: Vec::new(),
        notes: base.notes.clone(),
        seconds: 0.0,
    };
    // the outlines the base's areas covered stay covered (their entities are the base's area objects)
    let base_area: std::collections::BTreeSet<usize> = base
        .items
        .iter()
        .filter(|i| area(i.overlay))
        .filter_map(|i| i.entity)
        .collect();
    out.covered = base
        .covered
        .iter()
        .copied()
        .filter(|k| !base_area.contains(k))
        .collect();
    for i in tree.instances() {
        if !matches!(short_type(&i), "CombatAreaTriggerEntityData" | "CapturePointEntityData") {
            continue;
        }
        let k = i.attr("guid").and_then(|g| entity_of.get(&g.to_lowercase())).copied();
        let here = transform(&i).map(|m| [m[12], m[13], m[14]]);
        if let Some((item, cover)) = area_item(&i, k, here, &guids, &ground) {
            out.covered.extend(cover);
            out.items.push(item);
        }
    }
    out.covered.extend(base_area.iter().copied());
    out.covered.sort_unstable();
    out.covered.dedup();
    out.seconds = base.seconds + t0.elapsed().as_secs_f64();
    Some(out)
}

/// The item of a combat area or a capture point (round VW1; round A2: also read from a project's partition, at its
/// own transforms), with the scene entity whose outline it covers.
#[allow(
    clippy::too_many_lines,
    reason = "the two kinds of gameplay area, as the data holds them"
)]
fn area_item(
    i: &Node<'_, '_>,
    k: Option<usize>,
    here: Option<V3>,
    guids: &HashMap<String, Node<'_, '_>>,
    ground: &dyn Fn(f64, f64, f64) -> f64,
) -> Option<(Item, Option<usize>)> {
    match short_type(i) {
        "CombatAreaTriggerEntityData" => {
            let restricted = text(i, "UseTeamRestriction") == "true";
            let t = team(&text(i, "Team"));
            let paint = if restricted { Paint::Team(t) } else { Paint::Unowned };
            let name = text(i, "Name");
            let mut lines = Vec::new();
            let mut anchor = here.unwrap_or_default();
            let mut unbounded = false;
            for r in references(i, "Geometry") {
                let (pts, height) = chain(guids, r);
                if pts.len() < 3 {
                    continue;
                }
                let bottom: Vec<V3> = pts.iter().map(|p| [p[0], ground(p[0], p[2], p[1]), p[2]]).collect();
                let top: Vec<V3> = if height > 0.0 {
                    pts.iter().map(|p| [p[0], p[1] + height, p[2]]).collect()
                } else {
                    unbounded = true;
                    bottom.iter().map(|p| [p[0], p[1] + RIBBON, p[2]]).collect()
                };
                anchor = centre(&top);
                lines.extend(wall(&bottom, &top));
            }
            for r in references(i, "SurroundingGeometry") {
                let (pts, _) = chain(guids, r);
                if pts.len() >= 3 {
                    let flat: Vec<V3> = pts
                        .iter()
                        .map(|p| [p[0], ground(p[0], p[2], p[1]) + 0.3, p[2]])
                        .collect();
                    lines.extend(ring(&flat));
                }
            }
            if lines.is_empty() {
                return None;
            }
            let whose = if restricted {
                format!("team {t}")
            } else {
                "every team".to_owned()
            };
            Some((
                Item {
                    overlay: Overlay::CombatAreas,
                    entity: k,
                    at: anchor,
                    label: format!(
                        "Combat area {} ({whose}{})",
                        if name.is_empty() { "" } else { name.as_str() },
                        if unbounded {
                            ", no height in the data: unbounded"
                        } else {
                            ""
                        }
                    ),
                    radius: None,
                    swatch: None,
                    paint,
                    lines,
                },
                k,
            ))
        }
        "CapturePointEntityData" => {
            let Some(p) = here else { return None };
            let t = team(&text(i, "InitialOwnerTeam"));
            let hq = text(i, "UnableToChangeTeam") == "true";
            let radius = num(i, "CaptureRadius").unwrap_or(0.0);
            let upper = text(i, "IsCapturedInUpperSphere") == "true";
            let mut lines = Vec::new();
            let mut volume = String::new();
            for r in references(i, "Geometry") {
                // a sphere or a box (MP_005's islands: SphereGeometryEntityData) instead of an area chain
                if let Some(g) = guids.get(&r.to_lowercase())
                    && let Some(m) = transform(g)
                {
                    let c = [m[12], m[13], m[14]];
                    match short_type(g) {
                        "SphereGeometryEntityData" => {
                            let rad = num(g, "Radius").unwrap_or(0.0);
                            if rad > 0.0 {
                                lines.extend(sphere(c, rad));
                                volume = format!(" (sphere {rad:.1} m)");
                            }
                            continue;
                        }
                        "AabbGeometryEntityData" => {
                            if let Some(e) = vec3(g, "Extents") {
                                lines.extend(box_edges(&m, [0.0; 3], e));
                                volume = " (box)".into();
                            }
                            continue;
                        }
                        _ => {}
                    }
                }
                let (pts, height) = chain(guids, r);
                if pts.len() < 3 {
                    continue;
                }
                if height > 0.0 {
                    let top: Vec<V3> = pts.iter().map(|q| [q[0], q[1] + height, q[2]]).collect();
                    lines.extend(wall(&pts, &top));
                } else {
                    lines.extend(ring(&pts));
                }
            }
            if radius > 0.0 {
                lines.extend(if upper { sphere(p, radius) } else { circle(p, radius) });
            }
            if lines.is_empty() {
                if !hq {
                    return None;
                }
                // an HQ the data gives no capture area (it cannot be taken): its marker only
                lines.extend(cross(p, 2.0));
                volume = " (no capture area: it cannot be taken)".into();
            }
            let name = text(i, "ControlPointName");
            Some((
                Item {
                    overlay: Overlay::CaptureAreas,
                    entity: k,
                    at: p,
                    label: format!(
                        "{} {}{}{volume}",
                        if hq { "HQ" } else { "Capture area" },
                        if name.is_empty() { text(i, "Name") } else { name },
                        if radius > 0.0 {
                            format!(" (radius {radius:.0} m)")
                        } else {
                            String::new()
                        }
                    ),
                    radius: (radius > 0.0).then_some(radius),
                    swatch: None,
                    paint: Paint::Team(t),
                    lines,
                },
                k,
            ))
        }
        _ => None,
    }
}

/// A sound emitter's item: its minimum distance as a sphere (a marker when it has none).
fn sound_item(entity: Option<usize>, at: V3, min: f64, slope: f64, name: &str) -> Item {
    let mut lines = cross(at, 0.5);
    if min > 0.0 {
        lines.extend(sphere(at, min));
    }
    let falloff = if slope > 0.0 {
        format!("log falloff, slope {slope}")
    } else {
        "no distance falloff".to_owned()
    };
    Item {
        overlay: Overlay::Sounds,
        entity,
        at,
        label: format!("Sound {name} (min {min:.0} m, {falloff}; no max distance in the data)"),
        radius: Some(min),
        swatch: None,
        paint: Paint::Sound,
        lines,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn shapes_have_their_sizes() {
        let c = circle([10.0, 2.0, -5.0], 7.0);
        assert_eq!(c.len(), STEPS);
        let first = c.first().unwrap()[0];
        assert!((f64::from(first[0]) - 17.0).abs() < 1e-4 && (f64::from(first[1]) - 2.0).abs() < 1e-6);
        let s = sphere([0.0; 3], 3.0);
        assert_eq!(s.len(), 3 * STEPS);
        assert!(s.iter().flatten().all(|p| {
            let r = p.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>().sqrt();
            (r - 3.0).abs() < 1e-4
        }));
        // a flat ellipsoid (Scale 1/0/1) is one ellipse; a box has 12 edges
        assert_eq!(ellipsoid(&IDENTITY, [0.0; 3], [5.0, 0.0, 5.0]).len(), STEPS);
        assert_eq!(box_edges(&IDENTITY, [0.0; 3], [1.0, 2.0, 3.0]).len(), 12);
        // a child transform placed in its parent: a translation and a quarter turn about Y
        let parent: M = [
            0.0, 0.0, -1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 100.0, 0.0, 0.0, 1.0,
        ];
        let mut child = IDENTITY;
        child[12] = 2.0;
        let w = compose(&child, &parent);
        assert_eq!(apply(&w, [0.0; 3]), [100.0, 0.0, -2.0]);
        assert_eq!(apply(&w, [1.0, 0.0, 0.0]), [100.0, 0.0, -3.0]);
        assert_eq!(team("Team2"), 2);
        assert_eq!(short_name("sound/a/b_01/0000-1"), "b_01");
        let mut t = Toggles::default();
        assert!(t.on(Overlay::CombatAreas) && !t.on(Overlay::Lights));
        t.set(Overlay::Lights, true);
        assert!(t.on(Overlay::Lights));
        let back: Toggles = serde_json::from_str("{}").unwrap();
        assert_eq!(back, Toggles::default());
        let w = wall(
            &[[0.0; 3], [16.0, 0.0, 0.0], [0.0, 0.0, 16.0]],
            &[[0.0, 5.0, 0.0], [16.0, 5.0, 0.0], [0.0, 5.0, 16.0]],
        );
        assert!(w.len() > 9);
    }
}
