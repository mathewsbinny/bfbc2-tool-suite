//! The editor's actions, in one registry: each has a name in the suite's voice, an optional fixed shortcut and a
//! line of help. The top bar, the keyboard and the command palette (Ctrl+Shift+P) all run actions from here, so a
//! new action is added once and appears everywhere (`EDITOR_ARCHITECTURE.md` 3.10, item 8; customisable shortcuts
//! are later: the shortcuts are fixed and shown until then).
//!
//! What an action does, and whether it can run now (with the reason when it cannot, for the disabled control's
//! tooltip), is the app's: `app::EditorApp::run` and `app::EditorApp::why_not`.

use eframe::egui::{Key, KeyboardShortcut, Modifiers};

use crate::camera::Projection;

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
    /// Opens the Help window (controls and shortcuts, getting started, the workspaces).
    Help,
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

/// Every action, in the palette's order.
#[must_use]
pub fn registry() -> Vec<Entry> {
    let ctrl_shift = Modifiers::COMMAND.plus(Modifiers::SHIFT);
    let mut out = vec![
        Entry {
            action: Action::CommandPalette,
            name: "Command palette",
            shortcut: key(ctrl_shift, Key::P),
            help: "Find and run any action by name",
        },
        Entry {
            action: Action::FocusSelection,
            name: "Focus selection",
            shortcut: key(Modifiers::NONE, Key::F),
            help: "Moves the camera to the selected object",
        },
        Entry {
            action: Action::ClearSelection,
            name: "Clear selection",
            shortcut: key(Modifiers::NONE, Key::Escape),
            help: "Selects nothing",
        },
        Entry {
            action: Action::CopyGuid,
            name: "Copy GUID",
            shortcut: None,
            help: "Copies the selected object's GUID",
        },
    ];
    for (p, name, k) in [
        (Projection::Perspective, "Open perspective view", Key::Num1),
        (Projection::Top, "Open top view", Key::Num2),
        (Projection::Front, "Open front view", Key::Num3),
        (Projection::Side, "Open side view", Key::Num4),
    ] {
        out.push(Entry {
            action: Action::OpenViewport(p),
            name,
            shortcut: key(Modifiers::ALT, k),
            help: "Shows the view, docked beside the others; each view has its own camera",
        });
    }
    out.extend([
        Entry {
            action: Action::ToggleLabels,
            name: "Show or hide labels",
            shortcut: key(Modifiers::NONE, Key::L),
            help: "Names and teams of spawns, flags and objectives in the views",
        },
        Entry {
            action: Action::ResetLayout,
            name: "Reset layout",
            shortcut: None,
            help: "Puts every panel back where it starts",
        },
        Entry {
            action: Action::ToggleTheme,
            name: "Switch theme",
            shortcut: None,
            help: "Switches the interface between Dark and Light",
        },
        Entry {
            action: Action::CloseMap,
            name: "Close map",
            shortcut: None,
            help: "Back to the start screen",
        },
        Entry {
            action: Action::Help,
            name: "Help",
            shortcut: key(Modifiers::NONE, Key::F1),
            help: "Controls and shortcuts, a getting-started guide, and what each workspace is for",
        },
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
    }
}
