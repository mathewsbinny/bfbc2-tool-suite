//! The mode view layer (round VW2): the 3D views, the outliner and the queries show or hide objects by the level's
//! game mode layer and by Rush stage. View settings only (never in the project's log), read from the edited
//! partition (the project's mode layer data, E3d: a target's layer is ordinary objects of the partition) or, with no
//! project open, from the level as loaded.
//!
//! - **Mode layer:** every object; the mode layer only (what [`bc2_levelwrite::modelayer::layer`] finds: the mode's
//!   gameplay instances, flags, bases, M-COMs, spawns, combat areas and their logic); or the level without it (the
//!   world).
//! - **Rush stage:** on a Rush level, one stage as it is played ([`bc2_levelwrite::modelayer::rush_play`], L1's
//!   simulation of the stages' events): the other stages' M-COMs with what belongs to them, and the bases, combat
//!   areas and spawns disabled during the stage, are hidden; the rest of the level stays.

use std::collections::HashSet;
use std::fmt::Write as _;

use bc2_levelwrite::modelayer::{LayerView, layer_view};
use bc2_levelwrite::write::LevelWriter;
use bc2_scene::scene::Scene;

/// Which objects the mode layer filter shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LayerFilter {
    /// Every object.
    #[default]
    All,
    /// The mode layer only.
    LayerOnly,
    /// The level without its mode layer.
    WithoutLayer,
}

impl LayerFilter {
    /// Every filter, in the menu's order.
    pub const ALL: [Self; 3] = [Self::All, Self::LayerOnly, Self::WithoutLayer];

    /// Its name in the menu.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::All => "Every object",
            Self::LayerOnly => "Mode layer only",
            Self::WithoutLayer => "Without the mode layer",
        }
    }
}

/// The filter's state: the choice and what it hides.
#[derive(Debug, Clone, Default)]
pub struct ModeView {
    /// The layer filter.
    pub filter: LayerFilter,
    /// The Rush stage shown (0-based, in play order), `None`: every stage.
    pub stage: Option<usize>,
    /// The layer and stages read, and the revision of the partition they were read at.
    pub view: Option<(u64, LayerView)>,
    /// The entities the filter hides (scene indices).
    pub hidden: HashSet<usize>,
    /// The hidden set changed since the outliner was last made (a level opened read only: its outliner follows).
    pub outliner_stale: bool,
}

impl ModeView {
    /// Whether the filter hides anything (a choice other than every object and every stage).
    #[must_use]
    pub fn active(&self) -> bool {
        self.filter != LayerFilter::All || self.stage.is_some()
    }

    /// Reads the layer and the stages from a partition when they are not read yet at `revision`.
    pub fn read(&mut self, w: Option<&LevelWriter>, revision: u64) {
        if self.view.as_ref().is_some_and(|(r, _)| *r == revision) {
            return;
        }
        self.view = Some((revision, w.map_or_else(LayerView::default, layer_view)));
    }

    /// The Rush stages of the level read (0 when it has none or nothing is read).
    #[must_use]
    pub fn stage_count(&self) -> usize {
        self.view.as_ref().map_or(0, |(_, v)| v.stages.len())
    }

    /// The layer's size (0 when nothing is read).
    #[must_use]
    pub fn layer_size(&self) -> usize {
        self.view.as_ref().map_or(0, |(_, v)| v.layer.len())
    }

    /// The guids the filter hides (upper case), from the layer read.
    #[must_use]
    pub fn hidden_guids(&self, scene_guids: impl Iterator<Item = String>) -> HashSet<String> {
        let Some((_, v)) = &self.view else {
            return HashSet::new();
        };
        let mut out: HashSet<String> = match self.filter {
            LayerFilter::All => HashSet::new(),
            LayerFilter::LayerOnly => scene_guids.filter(|g| !v.layer.contains(g)).collect(),
            LayerFilter::WithoutLayer => v.layer.clone(),
        };
        if let Some(s) = self.stage.and_then(|k| v.stages.get(k)) {
            out.extend(s.hidden.iter().cloned());
        }
        out
    }

    /// Works out the hidden entities of a scene (its entities' guids against [`ModeView::hidden_guids`]).
    pub fn apply(&mut self, scene: &Scene) {
        if !self.active() {
            self.hidden.clear();
            return;
        }
        let hide = self.hidden_guids(scene.entities.iter().map(|e| e.guid.to_ascii_uppercase()));
        self.hidden = scene
            .entities
            .iter()
            .filter(|e| hide.contains(&e.guid.to_ascii_uppercase()))
            .map(|e| e.index)
            .collect();
    }

    /// The filter's state in words (the status line, the view's badge).
    #[must_use]
    pub fn describe(&self) -> String {
        let mut s = self.filter.name().to_owned();
        if let Some(k) = self.stage {
            let _ = write!(s, ", Rush stage {} of {}", k + 1, self.stage_count());
        }
        let _ = write!(s, ": {} objects hidden", self.hidden.len());
        s
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use bc2_levelwrite::modelayer::StageView;

    #[test]
    fn filters_hide_the_layer_or_the_world_and_a_stage_hides_its_own() {
        let mut m = ModeView::default();
        let layer: HashSet<String> = ["A", "B", "C"].iter().map(|s| (*s).to_owned()).collect();
        m.view = Some((
            1,
            LayerView {
                layer,
                stages: vec![
                    StageView {
                        hidden: ["C".to_owned()].into(),
                        ..StageView::default()
                    },
                    StageView {
                        hidden: ["B".to_owned()].into(),
                        ..StageView::default()
                    },
                ],
                uncertain: false,
            },
        ));
        let all = || ["A", "B", "C", "W1", "W2"].iter().map(|s| (*s).to_owned());
        assert!(m.hidden_guids(all()).is_empty());
        m.filter = LayerFilter::LayerOnly;
        assert_eq!(m.hidden_guids(all()).len(), 2);
        m.filter = LayerFilter::WithoutLayer;
        assert_eq!(m.hidden_guids(all()).len(), 3);
        m.filter = LayerFilter::All;
        m.stage = Some(1);
        assert_eq!(m.hidden_guids(all()), ["B".to_owned()].into());
        assert!(m.active() && m.stage_count() == 2 && m.layer_size() == 3);
    }
}
