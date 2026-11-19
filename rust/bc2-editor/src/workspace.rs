//! The editor's workspaces - one per editing method (round E2b, the user's "an edit mode with subvarieties") - the
//! Objects workspace's category filter, and the editor view's display modes.
//!
//! - [`Workspace`]: View (read only), Objects (buildings, props, vehicles ... all edited alike, with a category
//!   filter), Terrain and Vegetation (their layouts and actions registered, the brushes come in later rounds),
//!   Gameplay (the mode's flags, spawns, objectives and areas) and AI (orders and the navigation overlay).
//! - [`Category`]: what an object is, decided from its blueprint's data (the type of the blueprint's object, its
//!   health states, its path in the game's folders) or, for entities without a blueprint, from their type.
//! - [`ViewMode`]: lit, unlit, wireframe, collision, navmesh and cull tree (the editor view; the Game-accurate view
//!   draws as the game does).

use bc2_scene::scene::Entity;

/// A workspace of the top bar's switcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Workspace {
    /// Look around (read only).
    #[default]
    View,
    /// Edit objects: select, move, rotate, place, duplicate, delete.
    Objects,
    /// Sculpt and paint the terrain (the terrain round).
    Terrain,
    /// Scatter and erase vegetation (a later round).
    Vegetation,
    /// The game mode's flags, spawns, objectives and areas.
    Gameplay,
    /// The bots' orders and navigation.
    Ai,
}

impl Workspace {
    /// Every workspace, in the switcher's order.
    pub const ALL: [Self; 6] = [
        Self::View,
        Self::Objects,
        Self::Terrain,
        Self::Vegetation,
        Self::Gameplay,
        Self::Ai,
    ];

    /// Its name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::View => "View",
            Self::Objects => "Objects",
            Self::Terrain => "Terrain",
            Self::Vegetation => "Vegetation",
            Self::Gameplay => "Gameplay",
            Self::Ai => "AI",
        }
    }

    /// Whether it edits a project (all but View).
    #[must_use]
    pub fn edits(self) -> bool {
        self != Self::View
    }

    /// Whether objects are selected, moved and turned in it (Objects, Gameplay, AI; Terrain and Vegetation have
    /// brushes instead).
    #[must_use]
    pub fn moves_objects(self) -> bool {
        matches!(self, Self::Objects | Self::Gameplay | Self::Ai)
    }

    /// Why its tools cannot be used yet (the stubs), or `None`.
    #[must_use]
    pub fn coming(self) -> Option<&'static str> {
        match self {
            Self::Terrain => Some("Coming in the terrain round"),
            Self::Vegetation => Some("Coming in the vegetation round"),
            _ => None,
        }
    }

    /// Whether an entity can be selected in this workspace (with the Objects workspace's category filter).
    #[must_use]
    pub fn selects(self, e: &Entity, filter: Option<Category>) -> bool {
        match self {
            Self::View => true,
            Self::Objects => filter.is_none_or(|f| Category::of(e) == f),
            Self::Terrain | Self::Vegetation => false,
            Self::Gameplay => is_gameplay(e),
            Self::Ai => is_ai(e),
        }
    }
}

/// Whether an entity is the Gameplay workspace's: spawns, vehicle spawners, flags, objectives (M-COMs), combat
/// areas.
#[must_use]
pub fn is_gameplay(e: &Entity) -> bool {
    matches!(
        e.category.as_str(),
        "soldier spawn" | "vehicle spawn" | "capture point" | "objective" | "combat area"
    )
}

/// Whether an entity is the AI workspace's: orders, AI soldier spawns and the points of their areas.
#[must_use]
pub fn is_ai(e: &Entity) -> bool {
    e.category == "ai order" || (e.category == "soldier spawn" && e.ai == Some(true)) || e.category == "area point"
}

/// What an object is (the Objects workspace's filter and the content browser's categories).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    /// Buildings and structures (destructible models, and models in the game's building and architecture folders).
    Buildings,
    /// Props: other static and dynamic models, ladders.
    Props,
    /// Vehicles (vehicle blueprints and vehicle spawners).
    Vehicles,
    /// Effects.
    Effects,
    /// Sounds.
    Sounds,
    /// Trees and plants.
    Vegetation,
    /// The game mode's entities: spawns, flags, objectives, combat areas.
    Gameplay,
    /// Everything else (logic, triggers, the level's own entities).
    Other,
}

impl Category {
    /// Every category, in the filter's order.
    pub const ALL: [Self; 8] = [
        Self::Buildings,
        Self::Props,
        Self::Vehicles,
        Self::Effects,
        Self::Sounds,
        Self::Vegetation,
        Self::Gameplay,
        Self::Other,
    ];

    /// Its name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Buildings => "Buildings",
            Self::Props => "Props",
            Self::Vehicles => "Vehicles",
            Self::Effects => "Effects",
            Self::Sounds => "Sounds",
            Self::Vegetation => "Vegetation",
            Self::Gameplay => "Gameplay",
            Self::Other => "Other",
        }
    }

    /// A blueprint's category from its data: the type of its object (`StaticModelEntityData`, `VehicleEntityData`
    /// ...; the blueprint's own type where it has no object), whether it breaks up (health states, part links), and
    /// its path in the game's folders.
    #[must_use]
    pub fn of_blueprint(path: &str, object_type: Option<&str>, destructible: bool) -> Self {
        let t = object_type.unwrap_or("").to_ascii_lowercase();
        let p = path.to_ascii_lowercase();
        let in_folder = |names: &[&str]| p.split('/').any(|s| names.iter().any(|n| s.starts_with(n)));
        if t.contains("vehicle") || in_folder(&["vehicles"]) {
            Self::Vehicles
        } else if t.contains("effect") || in_folder(&["fx", "effects"]) {
            Self::Effects
        } else if t.contains("sound") || in_folder(&["sound"]) {
            Self::Sounds
        } else if t.contains("tree") || t.contains("vegetation") || in_folder(&["vegetation", "foliage", "trees"]) {
            Self::Vegetation
        } else if destructible || in_folder(&["buildings", "architecture", "bridge"]) {
            Self::Buildings
        } else if t.contains("model") || t.contains("ladder") || t.contains("banger") || p.starts_with("objects/") {
            Self::Props
        } else {
            Self::Other
        }
    }

    /// An entity's category: its blueprint's for placed objects, else by its type.
    #[must_use]
    pub fn of(e: &Entity) -> Self {
        if matches!(
            e.category.as_str(),
            "objective" | "capture point" | "soldier spawn" | "combat area"
        ) {
            return Self::Gameplay;
        }
        if let Some(bp) = &e.blueprint {
            return Self::of_blueprint(bp, e.blueprint_type.as_deref(), e.destructible);
        }
        match e.category.as_str() {
            "vehicle spawn" => Self::Vehicles,
            "soldier spawn" | "capture point" | "objective" | "combat area" => Self::Gameplay,
            "effect" => Self::Effects,
            _ => {
                let t = e.short_type.to_ascii_lowercase();
                if t.contains("sound") {
                    Self::Sounds
                } else if t.contains("effect") {
                    Self::Effects
                } else if t.contains("tree") || t.contains("overgrowth") {
                    Self::Vegetation
                } else {
                    Self::Other
                }
            }
        }
    }
}

/// How the editor view draws the level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ViewMode {
    /// Shaded by the editor's light.
    #[default]
    Lit,
    /// Flat colours, no light.
    Unlit,
    /// The triangles' edges.
    Wireframe,
    /// What the game collides with: the terrain and the objects' Havok shapes, each blueprint in its own colour.
    Collision,
    /// The bots' walkable ground (the level's Kynapse path data) over the level.
    Navmesh,
    /// The static cull tree: each placed object's slot (none, shared, its own) and the leaves' spheres.
    CullTree,
}

impl ViewMode {
    /// Every mode, in the menu's order.
    pub const ALL: [Self; 6] = [
        Self::Lit,
        Self::Unlit,
        Self::Wireframe,
        Self::Collision,
        Self::Navmesh,
        Self::CullTree,
    ];

    /// Its name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Lit => "Lit",
            Self::Unlit => "Unlit",
            Self::Wireframe => "Wireframe",
            Self::Collision => "Collision",
            Self::Navmesh => "Navmesh",
            Self::CullTree => "Cull tree",
        }
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn categories_from_blueprint_data() {
        let c = Category::of_blueprint;
        assert_eq!(
            c("objects/buildings/barn/barn", Some("StaticModelEntityData"), false),
            Category::Buildings
        );
        assert_eq!(
            c("objects/props/crate/crate", Some("StaticModelEntityData"), true),
            Category::Buildings
        );
        assert_eq!(
            c("objects/props/crate/crate", Some("StaticModelEntityData"), false),
            Category::Props
        );
        assert_eq!(
            c("vehicles/us/m1a2/m1a2", Some("VehicleEntityData"), false),
            Category::Vehicles
        );
        assert_eq!(c("fx/smoke/smoke", Some("EffectEntityData"), false), Category::Effects);
        assert_eq!(c("objects/x/x", Some("SoundEntityData"), false), Category::Sounds);
        assert_eq!(c("vegetation/trees/palm/palm", None, false), Category::Vegetation);
        let mut d = crate::demo::scene();
        assert_eq!(Category::of(&d.entities[1]), Category::Gameplay);
        assert_eq!(Category::of(&d.entities[3]), Category::Vehicles);
        assert_eq!(Category::of(&d.entities[0]), Category::Props);
        assert!(Workspace::Objects.selects(&d.entities[1], None));
        assert!(!Workspace::Objects.selects(&d.entities[1], Some(Category::Props)));
        assert!(Workspace::Gameplay.selects(&d.entities[2], None));
        assert!(!Workspace::Gameplay.selects(&d.entities[0], None));
        d.entities[1].ai = Some(true);
        assert!(Workspace::Ai.selects(&d.entities[1], None));
        assert!(!Workspace::Terrain.selects(&d.entities[0], None));
    }
}
