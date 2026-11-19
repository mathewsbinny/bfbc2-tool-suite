//! The editor's actions, in one registry: each has a name in the suite's voice, an optional fixed shortcut and a
//! line of help. The top bar, the keyboard and the command palette (Ctrl+Shift+P) all run actions from here, so a
//! new action is added once and appears everywhere (`EDITOR_ARCHITECTURE.md` 3.10, item 8; customisable shortcuts
//! are later: the shortcuts are fixed and shown until then).
//!
//! What an action does, and whether it can run now (with the reason when it cannot, for the disabled control's
//! tooltip), is the app's: `app::EditorApp::run` and `app::EditorApp::why_not`.

use eframe::egui::{Key, KeyboardShortcut, Modifiers};

use crate::camera::Projection;
use crate::gizmo::Tool;
use crate::workspace::{ViewMode, Workspace};

/// An action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    /// Opens the command palette.
    CommandPalette,
    /// Moves the camera to the selection.
    FocusSelection,
    /// Selects nothing.
    ClearSelection,
    /// Copies the selection's GUID.
    CopyGuid,
    /// Opens (or shows) a viewport of a projection.
    OpenViewport(Projection),
    /// Shows or hides the labels in the 3D views.
    ToggleLabels,
    /// The 3D view draws the level as the game does, or as the editor does.
    ToggleGameView,
    /// Puts every panel back where it starts.
    ResetLayout,
    /// Dark or Light.
    ToggleTheme,
    /// Back to the start screen.
    CloseMap,
    /// Undoes the last edit.
    Undo,
    /// Redoes the last undone edit.
    Redo,
    /// Saves the project.
    Save,
    /// Applies the whole log to the base again and reports.
    CheckEdits,
    /// Exports the project as a new multiplayer level (staged, checked; the game copy unchanged).
    Export,
    /// Deletes the selection.
    Delete,
    /// Duplicates the selection.
    Duplicate,
    /// Chooses a tool.
    UseTool(Tool),
    /// World or local axes.
    ToggleSpace,
    /// Grid snapping on or off.
    ToggleGridSnap,
    /// Angle snapping on or off.
    ToggleAngleSnap,
    /// Vertex and edge snapping on or off.
    ToggleVertexSnap,
    /// Surface snapping on or off.
    ToggleSurfaceSnap,
    /// Puts the selection on the surface below it, sunk by the toolbar's depth.
    PlaceOnSurface,
    /// Lines the selection up with the primary object on an axis (0 X, 1 Y, 2 Z).
    Align(u8),
    /// Spaces the selection evenly between its two ends on an axis.
    Distribute(u8),
    /// Groups the selection.
    Group,
    /// Ungroups the selection's group.
    Ungroup,
    /// Makes a layer and puts the selection on it.
    NewLayer,
    /// Switches to a workspace.
    SetWorkspace(Workspace),
    /// Copies the selection with the sub-instances it owns (a flag's capture area).
    DeepDuplicate,
    /// Hides the selection in the views.
    Hide,
    /// Shows every object hidden one by one.
    ShowHidden,
    /// Locks the selection.
    Lock,
    /// Unlocks every object locked one by one.
    UnlockAll,
    /// Opens the sink popup for the selection.
    SinkPopup,
    /// Opens the placing picker at the point under the pointer.
    PlaceObject,
    /// Copies the selection (for Paste).
    Copy,
    /// Pastes the copied objects at the point under the pointer.
    Paste,
    /// Selects every object of the primary's type (its blueprint, else its entity type).
    SelectAllOfType,
    /// Moves the camera to the point under the pointer.
    FocusHere,
    /// The editor view's display mode.
    SetViewMode(ViewMode),
    /// A terrain or vegetation brush (stubs until their rounds).
    Brush(Brush),
    /// Sets the team of the selected gameplay objects (1, 2; 0 neutral).
    SetTeam(u8),
    /// The AI workspace's path test (a later round).
    PathTest,
    /// Shows a panel.
    ShowPanel(PanelKind),
    /// Opens the Help window (controls and shortcuts, getting started, the workspaces).
    Help,
}

/// The panels an action can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelKind {
    /// History.
    History,
    /// Layers.
    Layers,
    /// Problems.
    Problems,
    /// Properties.
    Properties,
    /// The content browser.
    Content,
}

/// The Terrain and Vegetation workspaces' brushes (registered; their tools come in later rounds).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Brush {
    /// Raise the terrain.
    Raise,
    /// Lower the terrain.
    Lower,
    /// Smooth the terrain.
    Smooth,
    /// Flatten the terrain.
    Flatten,
    /// Paint a texture layer.
    Paint,
    /// Scatter vegetation.
    Scatter,
    /// Erase vegetation.
    Erase,
}

impl Brush {
    /// Every brush: the terrain's, then the vegetation's.
    pub const ALL: [Self; 7] = [
        Self::Raise,
        Self::Lower,
        Self::Smooth,
        Self::Flatten,
        Self::Paint,
        Self::Scatter,
        Self::Erase,
    ];

    /// The workspace it belongs to.
    #[must_use]
    pub fn workspace(self) -> Workspace {
        match self {
            Self::Scatter | Self::Erase => Workspace::Vegetation,
            _ => Workspace::Terrain,
        }
    }

    /// Its name (the stub toolbar).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Raise => "Raise",
            Self::Lower => "Lower",
            Self::Smooth => "Smooth",
            Self::Flatten => "Flatten",
            Self::Paint => "Paint layer",
            Self::Scatter => "Scatter",
            Self::Erase => "Erase",
        }
    }
}

/// An action's entry in the registry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Entry {
    /// The action.
    pub action: Action,
    /// Its name: what it does, as a button would say it.
    pub name: &'static str,
    /// Its shortcut.
    pub shortcut: Option<KeyboardShortcut>,
    /// A line of help (tooltips, the palette).
    pub help: &'static str,
}

#[allow(clippy::unnecessary_wraps, reason = "fills an Option field")]
const fn key(modifiers: Modifiers, k: Key) -> Option<KeyboardShortcut> {
    // an Option: an entry's shortcut is optional, and this fills that field
    Some(KeyboardShortcut::new(modifiers, k))
}

const fn e(action: Action, name: &'static str, shortcut: Option<KeyboardShortcut>, help: &'static str) -> Entry {
    Entry {
        action,
        name,
        shortcut,
        help,
    }
}

/// Every action, in the palette's order.
#[must_use]
pub fn registry() -> Vec<Entry> {
    let ctrl = Modifiers::COMMAND;
    let ctrl_shift = Modifiers::COMMAND.plus(Modifiers::SHIFT);
    let none = Modifiers::NONE;
    let mut out = vec![
        e(
            Action::CommandPalette,
            "Command palette",
            key(ctrl_shift, Key::P),
            "Find and run any action by name",
        ),
        e(
            Action::FocusSelection,
            "Focus selection",
            key(none, Key::F),
            "Moves the camera to the selected object",
        ),
        e(
            Action::ClearSelection,
            "Clear selection",
            key(none, Key::Escape),
            "Selects nothing",
        ),
        e(Action::CopyGuid, "Copy GUID", None, "Copies the selected object's GUID"),
    ];
    for (p, name, k) in [
        (Projection::Perspective, "Open perspective view", Key::Num1),
        (Projection::Top, "Open top view", Key::Num2),
        (Projection::Front, "Open front view", Key::Num3),
        (Projection::Side, "Open side view", Key::Num4),
    ] {
        out.push(e(
            Action::OpenViewport(p),
            name,
            key(Modifiers::ALT, k),
            "Shows the view, docked beside the others; each view has its own camera",
        ));
    }
    out.extend([
        e(
            Action::ToggleLabels,
            "Show or hide labels",
            key(none, Key::L),
            "Names and teams of spawns, flags and objectives in the views",
        ),
        e(
            Action::ToggleGameView,
            "Game-accurate view",
            None,
            "The 3D view drawn as the game draws it: DICE's own Direct3D 11 shaders, textures and the level's light",
        ),
        e(
            Action::Undo,
            "Undo",
            key(ctrl, Key::Z),
            "Takes back the last edit (History lists them all)",
        ),
        e(
            Action::Redo,
            "Redo",
            key(ctrl, Key::Y),
            "Makes the last undone edit again (also Ctrl+Shift+Z)",
        ),
        e(
            Action::Save,
            "Save project",
            key(ctrl, Key::S),
            "Writes the project file; until then every change is in its autosave",
        ),
        e(
            Action::CheckEdits,
            "Check edits",
            None,
            "Applies the whole log to the base level again and says what it changes",
        ),
        e(
            Action::Export,
            "Export as a multiplayer level",
            None,
            "Builds the map as a new level for the game's level list (staged and checked; installing is a separate step)",
        ),
        e(
            Action::Delete,
            "Delete",
            key(none, Key::Delete),
            "Deletes the selected objects (their event connections too)",
        ),
        e(
            Action::Duplicate,
            "Duplicate",
            key(ctrl, Key::D),
            "Copies the selected objects beside them, with their connections; sub-objects they own are shared",
        ),
        e(
            Action::DeepDuplicate,
            "Deep duplicate",
            key(ctrl_shift, Key::D),
            "Copies the selected objects with the sub-objects they own (a flag's capture area), references remapped",
        ),
        e(
            Action::Copy,
            "Copy",
            key(ctrl, Key::C),
            "Remembers the selected objects for Paste",
        ),
        e(
            Action::Paste,
            "Paste",
            key(ctrl, Key::V),
            "Copies the remembered objects to the point under the pointer, keeping their spacing",
        ),
        e(
            Action::PlaceObject,
            "Place object here\u{2026}",
            key(Modifiers::SHIFT, Key::A),
            "Opens a searchable list of blueprints to place at the point under the pointer",
        ),
        e(
            Action::SinkPopup,
            "Sink below surface\u{2026}",
            None,
            "Opens the sink popup: depth by slider or Alt+wheel, straight down, along or aligned to the normal",
        ),
        e(
            Action::Hide,
            "Hide",
            key(none, Key::H),
            "Hides the selected objects in the views (they stay in the build)",
        ),
        e(
            Action::ShowHidden,
            "Show hidden objects",
            key(Modifiers::ALT, Key::H),
            "Shows every object hidden with Hide",
        ),
        e(
            Action::Lock,
            "Lock",
            None,
            "Locks the selected objects: they cannot be selected in the views or edited",
        ),
        e(
            Action::UnlockAll,
            "Unlock all",
            None,
            "Unlocks every object locked with Lock",
        ),
        e(
            Action::SelectAllOfType,
            "Select all of this type",
            None,
            "Selects every object of the selected one's blueprint (or type), or of the content browser's entry",
        ),
        e(
            Action::FocusHere,
            "Focus here",
            None,
            "Moves the camera to look at the point under the pointer",
        ),
        e(
            Action::UseTool(Tool::Select),
            "Select tool",
            key(none, Key::Q),
            "Click to select, drag a box to select several",
        ),
        e(
            Action::UseTool(Tool::Move),
            "Move tool",
            key(none, Key::W),
            "Drag an arrow, a square or the centre of the gizmo to move",
        ),
        e(
            Action::UseTool(Tool::Rotate),
            "Rotate tool",
            key(none, Key::E),
            "Drag a ring of the gizmo to turn",
        ),
        e(
            Action::UseTool(Tool::Ruler),
            "Ruler",
            key(none, Key::R),
            "Click two points to measure the distance between them",
        ),
        e(
            Action::UseTool(Tool::Polygon),
            "Polygon select",
            key(none, Key::P),
            "Click the corners of a polygon; Enter or a double click selects what is inside, Esc cancels",
        ),
        e(
            Action::ToggleSpace,
            "Switch world or local axes",
            key(none, Key::X),
            "The gizmo follows the map's axes or the object's own",
        ),
        e(
            Action::ToggleGridSnap,
            "Snap to grid",
            key(none, Key::G),
            "Moves go in steps of the grid size",
        ),
        e(
            Action::ToggleAngleSnap,
            "Snap angles",
            None,
            "Turns go in steps of the angle step",
        ),
        e(
            Action::ToggleVertexSnap,
            "Snap to vertices and edges",
            key(none, Key::V),
            "Moves snap to the corner under the pointer (Shift: the edge)",
        ),
        e(
            Action::ToggleSurfaceSnap,
            "Snap to surface",
            key(none, Key::B),
            "Moves put objects on the terrain or mesh below them, sunk by the sink depth",
        ),
        e(
            Action::PlaceOnSurface,
            "Snap to ground",
            key(none, Key::End),
            "Puts the selection on the surface below it, sunk by the sink depth",
        ),
    ]);
    for (k, align, distribute) in [
        (0u8, "Align on X", Some("Distribute on X")),
        (1, "Align on Y", None),
        (2, "Align on Z", Some("Distribute on Z")),
    ] {
        out.push(e(
            Action::Align(k),
            align,
            None,
            "Lines the selection up with the primary object on this axis",
        ));
        if let Some(name) = distribute {
            out.push(e(
                Action::Distribute(k),
                name,
                None,
                "Spaces the selection evenly between its two ends on this axis",
            ));
        }
    }
    out.extend([
        e(
            Action::Group,
            "Group",
            key(ctrl, Key::G),
            "Groups the selection: clicking one selects them all",
        ),
        e(
            Action::Ungroup,
            "Ungroup",
            key(ctrl_shift, Key::G),
            "Undoes the selection's group (its objects stay)",
        ),
        e(
            Action::NewLayer,
            "New layer from selection",
            None,
            "Makes a layer and puts the selected objects on it",
        ),
        e(
            Action::ShowPanel(PanelKind::Properties),
            "Show properties",
            None,
            "The selection's fields; several objects show Mixed where they differ",
        ),
        e(
            Action::ShowPanel(PanelKind::Content),
            "Show content browser",
            None,
            "The level's and its bundles' blueprints, to drag into the view",
        ),
        e(
            Action::ShowPanel(PanelKind::History),
            "Show history",
            None,
            "The project's operations: undo or redo to any point",
        ),
        e(
            Action::ShowPanel(PanelKind::Layers),
            "Show layers",
            None,
            "Hide, isolate, lock and colour layers",
        ),
        e(
            Action::ShowPanel(PanelKind::Problems),
            "Show problems",
            None,
            "What did not load or apply, and why",
        ),
        e(
            Action::ResetLayout,
            "Reset layout",
            None,
            "Puts every panel back where it starts",
        ),
        e(
            Action::ToggleTheme,
            "Switch theme",
            None,
            "Switches the interface between Dark and Light",
        ),
        e(Action::CloseMap, "Close map", None, "Back to the start screen"),
        e(
            Action::SetTeam(1),
            "Set team 1",
            None,
            "Gives the selected spawns, flags and vehicle spawns to team 1",
        ),
        e(
            Action::SetTeam(2),
            "Set team 2",
            None,
            "Gives the selected spawns, flags and vehicle spawns to team 2",
        ),
        e(
            Action::SetTeam(0),
            "Set neutral",
            None,
            "Makes the selected flags neutral at the start",
        ),
        e(
            Action::PathTest,
            "Test a path from here",
            None,
            "The bots' route from this point to another (the AI workspace's tools, a later round)",
        ),
        e(
            Action::Help,
            "Help",
            key(none, Key::F1),
            "Controls and shortcuts, a getting-started guide, and what each workspace is for",
        ),
    ]);
    for w in Workspace::ALL {
        out.push(e(
            Action::SetWorkspace(w),
            match w {
                Workspace::View => "View workspace",
                Workspace::Objects => "Objects workspace",
                Workspace::Terrain => "Terrain workspace",
                Workspace::Vegetation => "Vegetation workspace",
                Workspace::Gameplay => "Gameplay workspace",
                Workspace::Ai => "AI workspace",
            },
            None,
            match w {
                Workspace::View => "Looks around the map, read only",
                Workspace::Objects => "Selects, moves, places, copies and deletes buildings, props, vehicles and more",
                Workspace::Terrain => "Sculpts and paints the terrain (its brushes come in the terrain round)",
                Workspace::Vegetation => "Scatters and erases vegetation (its brushes come in a later round)",
                Workspace::Gameplay => "Edits the mode's flags, spawns, objectives and areas",
                Workspace::Ai => "Edits the bots' orders and shows their navigation",
            },
        ));
    }
    for m in ViewMode::ALL {
        out.push(e(
            Action::SetViewMode(m),
            match m {
                ViewMode::Lit => "Lit view",
                ViewMode::Unlit => "Unlit view",
                ViewMode::Wireframe => "Wireframe view",
                ViewMode::Collision => "Collision view",
                ViewMode::Navmesh => "Navmesh view",
                ViewMode::CullTree => "Cull tree view",
            },
            None,
            match m {
                ViewMode::Lit => "The editor view shaded by its light",
                ViewMode::Unlit => "The editor view in flat colours, without light",
                ViewMode::Wireframe => "The editor view as the triangles' edges",
                ViewMode::Collision => "Only what the game collides with: terrain and the objects' Havok shapes",
                ViewMode::Navmesh => "The bots' walkable ground from the level's path data, over the level",
                ViewMode::CullTree => "The static cull tree: each object's slot (none, shared, its own) and the leaves",
            },
        ));
    }
    for b in Brush::ALL {
        out.push(e(
            Action::Brush(b),
            match b {
                Brush::Raise => "Raise terrain",
                Brush::Lower => "Lower terrain",
                Brush::Smooth => "Smooth terrain",
                Brush::Flatten => "Flatten terrain",
                Brush::Paint => "Paint terrain layer",
                Brush::Scatter => "Scatter vegetation",
                Brush::Erase => "Erase vegetation",
            },
            None,
            match b {
                Brush::Raise => "A brush that raises the terrain (the terrain round)",
                Brush::Lower => "A brush that lowers the terrain (the terrain round)",
                Brush::Smooth => "A brush that smooths the terrain (the terrain round)",
                Brush::Flatten => "A brush that flattens the terrain to a height (the terrain round)",
                Brush::Paint => "Paints a texture layer under the terrain-patch rules (the terrain round)",
                Brush::Scatter => "Scatters vegetation that passes the safe-to-place rules (a later round)",
                Brush::Erase => "Erases scattered vegetation (a later round)",
            },
        ));
    }
    out
}

/// The registry's entry of an action.
#[must_use]
pub fn entry(action: Action) -> Option<Entry> {
    registry().into_iter().find(|e| e.action == action)
}

/// The palette's matches for a query: every word of it in the name or help, ignoring case; names that start with
/// the query first.
#[must_use]
pub fn search(query: &str) -> Vec<Entry> {
    let q = query.trim().to_lowercase();
    let mut hits: Vec<(bool, Entry)> = registry()
        .into_iter()
        .filter(|e| {
            let hay = format!("{} {}", e.name, e.help).to_lowercase();
            q.split_whitespace().all(|w| hay.contains(w))
        })
        .map(|e| (!e.name.to_lowercase().starts_with(&q), e))
        .collect();
    hits.sort_by_key(|(later, _)| *later);
    hits.into_iter().map(|(_, e)| e).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_consistent() {
        let r = registry();
        // every action once, every shortcut once
        for (k, a) in r.iter().enumerate() {
            assert!(
                r.iter().skip(k + 1).all(|b| b.action != a.action),
                "{:?} twice",
                a.action
            );
            if let Some(s) = a.shortcut {
                assert!(
                    r.iter().skip(k + 1).all(|b| b.shortcut != Some(s)),
                    "{} shares its shortcut",
                    a.name
                );
            }
            assert!(!a.name.is_empty() && !a.help.is_empty());
            assert!(!a.name.ends_with('.') && !a.name.contains('!'));
        }
        assert_eq!(
            search("top").first().map(|e| e.action),
            Some(Action::OpenViewport(Projection::Top))
        );
        assert_eq!(search("FOCUS").first().map(|e| e.action), Some(Action::FocusSelection));
        assert_eq!(search("open view").len(), 4);
        assert_eq!(search("nothing like this"), Vec::new());
        assert_eq!(search("").len(), r.len());
        assert_eq!(entry(Action::ResetLayout).map(|e| e.name), Some("Reset layout"));
        assert_eq!(search("undo").first().map(|e| e.action), Some(Action::Undo));
        assert_eq!(search("sink").first().map(|e| e.action), Some(Action::SinkPopup));
        assert_eq!(search("distribute").len(), 2);
        assert_eq!(search("deep").first().map(|e| e.action), Some(Action::DeepDuplicate));
        assert_eq!(
            search("workspace")
                .iter()
                .filter(|e| matches!(e.action, Action::SetWorkspace(_)))
                .count(),
            Workspace::ALL.len()
        );
        assert_eq!(
            search("view")
                .iter()
                .filter(|e| matches!(e.action, Action::SetViewMode(_)))
                .count(),
            6
        );
    }
}
