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
    /// The Objects workspace (editing) or the View workspace.
    ToggleWorkspace,
    /// Shows a panel.
    ShowPanel(PanelKind),
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
            Action::Delete,
            "Delete",
            key(none, Key::Delete),
            "Deletes the selected objects (their event connections too)",
        ),
        e(
            Action::Duplicate,
            "Duplicate",
            key(ctrl, Key::D),
            "Copies the selected objects beside them, with their connections",
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
            Action::ToggleWorkspace,
            "Switch workspace",
            None,
            "Objects (editing) or View (looking around)",
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
    ]);
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
        assert_eq!(
            search("sink").first().map(|e| e.action),
            Some(Action::ToggleSurfaceSnap)
        );
        assert_eq!(search("distribute").len(), 2);
    }
}
