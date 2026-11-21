//! How the simplified view shows the level's objects (round SV1): a view setting, never an edit (not in the project's
//! log), kept per level with the editor's own settings and cleared with "Reset view overrides".
//!
//! - [`ShowAs`] for the whole view (the View dropdown, `C` cycles it): the render mesh (the default), its collision,
//!   both (the collision's edges over the mesh), the mesh's edges, or its bounding box;
//! - per object or per blueprint (the object menu's "Show as"): the same, or hidden;
//! - per object, the variants its data holds (the Properties' Display section): the mesh's level of detail and, for a
//!   destructible object, its parts' state (intact, each damaged rank, destroyed, every part; round VW1: the
//!   collision drawn follows it where the packfile splits it per part);
//! - the objects the game does not show (invisible walls, blueprints without a mesh): hidden, outlined or solid.

use std::collections::BTreeMap;

use bc2_render::simple::State;
use bc2_scene::scene::Entity;
use serde::{Deserialize, Serialize};

/// How an object is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ShowAs {
    /// Its render mesh (alpha-tested, tinted).
    #[default]
    Visual,
    /// Its Havok collision.
    Collision,
    /// Its render mesh with its collision's edges over it.
    Both,
    /// Its render mesh's edges.
    Wireframe,
    /// Its bounding box's edges.
    Bounds,
    /// Not drawn (an object or blueprint only).
    Hidden,
}

impl ShowAs {
    /// The whole view's choices, in the menu's order.
    pub const VIEW: [Self; 5] = [Self::Visual, Self::Collision, Self::Both, Self::Wireframe, Self::Bounds];
    /// An object's or a blueprint's choices.
    pub const OBJECT: [Self; 5] = [Self::Visual, Self::Collision, Self::Both, Self::Bounds, Self::Hidden];

    /// Its name in menus.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Visual => "Visual",
            Self::Collision => "Collision",
            Self::Both => "Visual + collision",
            Self::Wireframe => "Wireframe",
            Self::Bounds => "Bounds",
            Self::Hidden => "Hidden",
        }
    }

    /// The next of the whole view's choices (the shortcut cycles them).
    #[must_use]
    pub fn next(self) -> Self {
        let k = Self::VIEW.iter().position(|s| *s == self).map_or(0, |k| k + 1);
        Self::VIEW.get(k).copied().unwrap_or(Self::Visual)
    }
}

/// How the objects the game does not show are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum InvisibleAs {
    /// Not drawn (and not picked).
    #[default]
    Hidden,
    /// Their collision's edges.
    Outline,
    /// Their collision, solid.
    Solid,
}

impl InvisibleAs {
    /// Every choice.
    pub const ALL: [Self; 3] = [Self::Hidden, Self::Outline, Self::Solid];

    /// Its name in menus.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Hidden => "Hidden",
            Self::Outline => "Outline",
            Self::Solid => "Solid",
        }
    }
}

/// A destructible object's state, as kept (`bc2_render::simple::State`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Destruction {
    /// As the level starts.
    #[default]
    Intact,
    /// Every part at its last state.
    Destroyed,
    /// Every mesh part.
    AllParts,
    /// Round VW1: every part at its health state of this rank (1 the first damaged one), between intact and
    /// destroyed ([`Destruction::steps`]).
    Stage(u8),
}

impl Destruction {
    /// Every choice.
    pub const ALL: [Self; 3] = [Self::Intact, Self::Destroyed, Self::AllParts];

    /// Its name in menus.
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Intact => "Intact".into(),
            Self::Destroyed => "Destroyed".into(),
            Self::AllParts => "All parts".into(),
            Self::Stage(k) => format!("Damaged {k}"),
        }
    }

    /// The renderer's state.
    #[must_use]
    pub const fn state(self) -> State {
        match self {
            Self::Intact => State::Intact,
            Self::Destroyed => State::Destroyed,
            Self::AllParts => State::All,
            Self::Stage(k) => State::Stage(k),
        }
    }

    /// The states an object with `count` health-state ranks steps through, in order (round VW1: the picker's list
    /// and its up / down arrows): intact, each damaged rank, destroyed, then every part. `count` is
    /// `bc2_render::simple::stage_count` of its parts (`SP_01`'s guard tower: 3 - intact, damaged 1, destroyed).
    #[must_use]
    pub fn steps(count: usize) -> Vec<Self> {
        if count == 0 {
            return Vec::new();
        }
        let mut out = vec![Self::Intact];
        out.extend((1..count.saturating_sub(1)).map(|k| Self::Stage(k.min(usize::from(u8::MAX)) as u8)));
        if count > 1 {
            out.push(Self::Destroyed);
        }
        out.push(Self::AllParts);
        out
    }

    /// The state `delta` steps from this one in `steps` (clamped at the ends; a state not in the list starts from
    /// intact).
    #[must_use]
    pub fn step(self, steps: &[Self], delta: i32) -> Self {
        let at = steps.iter().position(|s| *s == self).unwrap_or(0) as i64;
        let to = (at + i64::from(delta)).clamp(0, steps.len().saturating_sub(1) as i64) as usize;
        steps.get(to).copied().unwrap_or(self)
    }
}

/// The simplified view's display settings of one level.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Display {
    /// The whole view's.
    #[serde(default)]
    pub show: ShowAs,
    /// The objects the game does not show.
    #[serde(default)]
    pub invisible: InvisibleAs,
    /// Per object (its GUID, lower case).
    #[serde(default)]
    pub objects: BTreeMap<String, ShowAs>,
    /// Per blueprint (lower case).
    #[serde(default)]
    pub blueprints: BTreeMap<String, ShowAs>,
    /// An object's level of detail (its GUID; none: Auto, LOD 0).
    #[serde(default)]
    pub lods: BTreeMap<String, u8>,
    /// A destructible object's state (its GUID; none: intact).
    #[serde(default)]
    pub states: BTreeMap<String, Destruction>,
    /// Round VW1: which overlays the view draws (effects, lights, sounds, the gameplay areas) and their labels.
    #[serde(default)]
    pub overlays: crate::overlays::Toggles,
}

impl Display {
    /// How an entity is shown: its own override, its blueprint's, else the view's.
    #[must_use]
    pub fn show_of(&self, e: &Entity) -> ShowAs {
        self.objects
            .get(&e.guid.to_lowercase())
            .or_else(|| self.blueprints.get(&e.blueprint.as_deref()?.to_lowercase()))
            .copied()
            .unwrap_or(self.show)
    }

    /// An entity's level of detail (0: Auto).
    #[must_use]
    pub fn lod_of(&self, e: &Entity) -> usize {
        self.lods.get(&e.guid.to_lowercase()).map_or(0, |l| usize::from(*l))
    }

    /// An entity's destruction state.
    #[must_use]
    pub fn state_of(&self, e: &Entity) -> Destruction {
        self.states.get(&e.guid.to_lowercase()).copied().unwrap_or_default()
    }

    /// Whether any object, blueprint, level of detail or state is overridden.
    #[must_use]
    pub fn has_overrides(&self) -> bool {
        !(self.objects.is_empty() && self.blueprints.is_empty() && self.lods.is_empty() && self.states.is_empty())
    }

    /// Clears every override (the view's own setting and the invisible objects' stay).
    pub fn reset_overrides(&mut self) {
        self.objects.clear();
        self.blueprints.clear();
        self.lods.clear();
        self.states.clear();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn overrides_win_in_order_and_reset() {
        let e = Entity {
            index: 0,
            guid: "AB-1".into(),
            type_name: String::new(),
            short_type: String::new(),
            category: "placed object".into(),
            id: None,
            name: None,
            partition: String::new(),
            matrix: None,
            blueprint: Some("objects/a".into()),
            group: None,
            blueprint_type: None,
            destructible: false,
            team: None,
            ai: None,
            source: bc2_scene::scene::Source::Instance { instance: 0 },
        };
        let mut d = Display {
            show: ShowAs::Wireframe,
            ..Display::default()
        };
        assert_eq!(d.show_of(&e), ShowAs::Wireframe);
        d.blueprints.insert("objects/a".into(), ShowAs::Bounds);
        assert_eq!(d.show_of(&e), ShowAs::Bounds);
        d.objects.insert("ab-1".into(), ShowAs::Hidden);
        assert_eq!(d.show_of(&e), ShowAs::Hidden);
        d.lods.insert("ab-1".into(), 2);
        assert_eq!(d.lod_of(&e), 2);
        let text = serde_json::to_string(&d).unwrap();
        assert_eq!(serde_json::from_str::<Display>(&text).unwrap(), d);
        d.reset_overrides();
        assert!(!d.has_overrides());
        assert_eq!(d.show_of(&e), ShowAs::Wireframe);
        assert_eq!(ShowAs::Bounds.next(), ShowAs::Visual);
    }

    #[test]
    fn destruction_steps_in_order() {
        use Destruction::{AllParts, Destroyed, Intact, Stage};
        assert_eq!(Destruction::steps(0), []);
        assert_eq!(Destruction::steps(1), [Intact, AllParts]);
        assert_eq!(Destruction::steps(2), [Intact, Destroyed, AllParts]);
        let s = Destruction::steps(3);
        assert_eq!(s, [Intact, Stage(1), Destroyed, AllParts]);
        assert_eq!(Intact.step(&s, 1), Stage(1));
        assert_eq!(Stage(1).step(&s, 1), Destroyed);
        assert_eq!(AllParts.step(&s, 1), AllParts);
        assert_eq!(Intact.step(&s, -1), Intact);
        assert_eq!(Destroyed.step(&s, -2), Intact);
        assert_eq!(Stage(1).name(), "Damaged 1");
        let text = serde_json::to_string(&Stage(2)).unwrap();
        assert_eq!(serde_json::from_str::<Destruction>(&text).unwrap(), Stage(2));
    }
}
