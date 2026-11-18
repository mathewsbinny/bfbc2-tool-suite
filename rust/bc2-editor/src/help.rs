//! The Help window (the top bar's Help menu, F1; `EDITOR_ARCHITECTURE.md` 3.10, "Decisions on G1's first version"):
//! controls and shortcuts, a getting-started guide for new users, and what each workspace is for; searchable.
//!
//! Nothing here is written twice. The shortcuts come from the action registry ([`crate::actions::registry`]: its
//! names, shortcuts and help lines), the mouse and keyboard controls of the views from [`VIEW_CONTROLS`] (which the
//! views' corner hint is made from too: [`corner_hint`]), and the workspaces from [`WORKSPACES`] (which the top bar's
//! switcher is made from). A new action is in Help as soon as it is in the registry; one without a description fails
//! the tests.

use eframe::egui::{self, Align, Color32, CornerRadius, Layout, Margin, ModifierNames, RichText, Sense, Stroke, Vec2};

use crate::actions;
use crate::theme::{Metrics, Palette};

/// A page of the Help window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Page {
    /// Controls and shortcuts.
    #[default]
    Controls,
    /// The getting-started guide.
    GettingStarted,
    /// What each workspace is for.
    Workspaces,
}

impl Page {
    /// Every page, in the window's order.
    pub const ALL: [Self; 3] = [Self::GettingStarted, Self::Controls, Self::Workspaces];

    /// Its name (the Help menu's items, the window's tabs).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Controls => "Controls and shortcuts",
            Self::GettingStarted => "Getting started",
            Self::Workspaces => "Workspaces",
        }
    }
}

/// Which views a control works in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Views {
    /// Every view.
    All,
    /// The 3D (perspective) view.
    Perspective,
    /// The top, front and side views.
    Orthographic,
}

/// A mouse or keyboard control of the views (not an action of the registry: these act where the pointer is).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Control {
    /// What it does.
    pub name: &'static str,
    /// The keys or buttons.
    pub keys: &'static str,
    /// Which views.
    pub views: Views,
    /// A line of help.
    pub help: &'static str,
}

/// The views' controls, as `app.rs`'s viewport handles them.
pub const VIEW_CONTROLS: [Control; 9] = [
    Control {
        name: "Orbit",
        keys: "Left drag",
        views: Views::Perspective,
        help: "Turns the camera around the point it looks at",
    },
    Control {
        name: "Pan",
        keys: "Middle drag",
        views: Views::Perspective,
        help: "Slides the camera sideways and up or down",
    },
    Control {
        name: "Pan",
        keys: "Left or middle drag",
        views: Views::Orthographic,
        help: "Slides the top, front or side view",
    },
    Control {
        name: "Zoom",
        keys: "Wheel",
        views: Views::All,
        help: "Moves the camera nearer or further",
    },
    Control {
        name: "Fly",
        keys: "Hold right, W A S D, Q E",
        views: Views::Perspective,
        help: "Looks around with the mouse while the right button is held; W A S D move, Q E go down and up, Shift \
               goes faster",
    },
    Control {
        name: "Select",
        keys: "Click",
        views: Views::All,
        help: "Selects what is under the pointer and outlines it in orange, through walls; a click on the sky \
               selects nothing",
    },
    Control {
        name: "Select and focus",
        keys: "Double click",
        views: Views::All,
        help: "Selects what is under the pointer and moves the camera to it",
    },
    Control {
        name: "Keep a camera bookmark",
        keys: "Ctrl+1 - Ctrl+9",
        views: Views::All,
        help: "Keeps where this view looks, per level and view (the pointer over the view)",
    },
    Control {
        name: "Go to a camera bookmark",
        keys: "1 - 9",
        views: Views::All,
        help: "Moves this view's camera to a bookmark kept with Ctrl",
    },
];

/// A workspace of the top bar's switcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Workspace {
    /// Its name.
    pub name: &'static str,
    /// Whether it can be used in this version.
    pub available: bool,
    /// What it is for.
    pub what: &'static str,
    /// When it comes (empty when it is here).
    pub when: &'static str,
}

/// The workspaces, in the switcher's order (`EDITOR_ARCHITECTURE.md` 3.9).
pub const WORKSPACES: [Workspace; 4] = [
    Workspace {
        name: "View",
        available: true,
        what: "Look at any map of your game copy, read-only: fly around it, find objects in the outliner, read their \
               properties. Nothing is changed.",
        when: "",
    },
    Workspace {
        name: "Objects",
        available: false,
        what: "Move, rotate, place, duplicate and delete objects, with snapping to the ground, the grid and surfaces.",
        when: "Comes with editing (round E2).",
    },
    Workspace {
        name: "Gameplay",
        available: false,
        what: "The game mode's flags, spawns, vehicles, objectives and areas, with a checklist of what the mode \
               still needs.",
        when: "Comes with gameplay editing (round E4).",
    },
    Workspace {
        name: "AI",
        available: false,
        what: "The bots' navigation mesh, coverage, routes and links, and a path test between two points.",
        when: "Comes later (round E5a).",
    },
];

/// The getting-started guide: (title, text) steps.
pub const GETTING_STARTED: [(&str, &str); 7] = [
    (
        "Open a map",
        "The start screen lists the levels of your game copy. Click one to open it. A level opened for the first \
         time is prepared first - its bundles unpacked, its objects and terrain read - which takes up to a minute; \
         the progress shows each step. Nothing in your game copy is changed.",
    ),
    (
        "Look around",
        "Drag with the left button to orbit, with the middle button to pan, and turn the wheel to zoom. Hold the \
         right button to fly: move the mouse to look, W A S D to move, Q and E to go down and up.",
    ),
    (
        "Find things",
        "Click an object in the 3D view, or a row in the Outliner on the left, to select it; Properties on the right \
         shows what it is. F moves the camera to the selection. Search the Outliner by name, type, GUID or \
         blueprint, and hide whole groups you do not need.",
    ),
    (
        "Read the colours",
        "Each team's spawns, flags and vehicles are in its colour with its faction's label (US and RU, US and NVA \
         in Vietnam, US and JP in the Pacific War). Flags no team holds yet are light grey with NEU; objectives and \
         combat areas no team owns are violet. The selection is outlined in orange.",
    ),
    (
        "Use more views",
        "Alt+2, Alt+3 and Alt+4 open the top, front and side views beside the 3D view, each with its own camera. \
         Ctrl+1 to Ctrl+9 keep a camera bookmark, 1 to 9 go back to it.",
    ),
    (
        "Arrange the window",
        "Drag a panel's tab to dock it elsewhere, put it beside another as a tab, or float it. Reset layout puts \
         everything back. The theme button switches between Dark and Light.",
    ),
    (
        "Find any command",
        "Ctrl+Shift+P opens the command palette: type a few letters of what you want to do and press Enter. Every \
         button of the top bar is there too, with its shortcut.",
    ),
];

/// One entry of the Help window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Topic {
    /// Its page.
    pub page: Page,
    /// The heading it sits under.
    pub section: &'static str,
    /// Its title.
    pub title: String,
    /// Its keys (controls) or state (workspaces), empty for guide steps.
    pub keys: String,
    /// Its text.
    pub body: String,
}

/// A shortcut in words, as the window and the tests write it (`Ctrl+Shift+P`, `F1`).
#[must_use]
pub fn shortcut_text(sc: &egui::KeyboardShortcut) -> String {
    sc.format(&ModifierNames::NAMES, false)
}

/// Every entry of the Help window, in page order: the controls of the views, then every action of the registry
/// (with its shortcut, or where to find it), then the guide's steps, then the workspaces.
#[must_use]
pub fn topics() -> Vec<Topic> {
    let mut out = Vec::new();
    // grouped by the views they work in: the 3D view, the orthographic views, every view
    let mut controls = VIEW_CONTROLS.to_vec();
    controls.sort_by_key(|c| match c.views {
        Views::Perspective => 0,
        Views::Orthographic => 1,
        Views::All => 2,
    });
    for c in controls {
        let section = match c.views {
            Views::All => "In every view",
            Views::Perspective => "In the 3D view",
            Views::Orthographic => "In the top, front and side views",
        };
        out.push(Topic {
            page: Page::Controls,
            section,
            title: c.name.into(),
            keys: c.keys.into(),
            body: c.help.into(),
        });
    }
    for e in actions::registry() {
        out.push(Topic {
            page: Page::Controls,
            section: "Commands",
            title: e.name.into(),
            keys: e
                .shortcut
                .as_ref()
                .map_or_else(|| "Command palette".to_owned(), shortcut_text),
            body: e.help.into(),
        });
    }
    for (k, (title, text)) in GETTING_STARTED.iter().enumerate() {
        out.push(Topic {
            page: Page::GettingStarted,
            section: "A first look at a map",
            title: format!("{}. {title}", k + 1),
            keys: String::new(),
            body: (*text).into(),
        });
    }
    for w in WORKSPACES {
        out.push(Topic {
            page: Page::Workspaces,
            section: "Workspaces",
            title: w.name.into(),
            keys: if w.available { "Available".into() } else { w.when.into() },
            body: w.what.into(),
        });
    }
    out
}

/// The topics a search finds: every word of the query in the title, keys, text or heading, ignoring case.
#[must_use]
pub fn search<'a>(topics: &'a [Topic], query: &str) -> Vec<&'a Topic> {
    let q = query.trim().to_lowercase();
    topics
        .iter()
        .filter(|t| {
            let hay = format!("{} {} {} {}", t.title, t.keys, t.body, t.section).to_lowercase();
            q.split_whitespace().all(|w| hay.contains(w))
        })
        .collect()
}

/// The hint in a view's corner: its controls and the two commands a newcomer needs (focus, the palette), from
/// [`VIEW_CONTROLS`] and the registry.
#[must_use]
pub fn corner_hint(orthographic: bool) -> String {
    let mut parts: Vec<String> = VIEW_CONTROLS
        .iter()
        .filter(|c| match c.views {
            Views::All => !c.name.contains("bookmark") && c.name != "Select" && c.name != "Select and focus",
            Views::Perspective => !orthographic,
            Views::Orthographic => orthographic,
        })
        .map(|c| {
            format!(
                "{}: {}",
                c.name,
                c.keys.to_lowercase().replace("w a s d, q e", "W A S D, Q E")
            )
        })
        .collect();
    for a in [actions::Action::FocusSelection, actions::Action::Help] {
        if let Some(e) = actions::entry(a)
            && let Some(sc) = e.shortcut
        {
            parts.push(format!("{}: {}", e.name, shortcut_text(&sc)));
        }
    }
    parts.join("   ")
}

/// The Help window's state.
#[derive(Debug, Clone, Default)]
pub struct HelpState {
    /// Shown.
    pub open: bool,
    /// The page shown when there is no search.
    pub page: Page,
    /// The search.
    pub query: String,
}

impl HelpState {
    /// Opens the window at `page`.
    pub fn show(&mut self, page: Page) {
        self.open = true;
        self.page = page;
    }
}

fn caption(text: &str, color: Color32) -> RichText {
    RichText::new(text.to_uppercase())
        .text_style(egui::TextStyle::Name("label".into()))
        .color(color)
}

/// The Help window: a floating panel (the design's dialog: `surface-1`, `radius-lg`, `shadow-popover`) with the
/// search above the page tabs.
pub fn window(ctx: &egui::Context, state: &mut HelpState, p: &Palette, m: &Metrics) {
    if !state.open {
        return;
    }
    let mut open = true;
    let frame = egui::Frame::new()
        .fill(p.surface_1)
        .stroke(Stroke::new(1.0, p.line))
        .corner_radius(CornerRadius::same(m.radius_lg as u8))
        .shadow(ctx.global_style().visuals.window_shadow)
        .inner_margin(Margin::same(m.space_4 as i8));
    egui::Window::new(RichText::new("Help").text_style(egui::TextStyle::Name("section".into())))
        .id(egui::Id::new("help window"))
        .open(&mut open)
        .frame(frame)
        .default_size(Vec2::new(680.0, 540.0))
        .min_size(Vec2::new(420.0, 300.0))
        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
        .collapsible(false)
        .resizable(true)
        .show(ctx, |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut state.query)
                    .hint_text(RichText::new("Search help (fly, bookmark, faction ...)").color(p.ink_faint))
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(m.space_2);
            let searching = !state.query.trim().is_empty();
            ui.horizontal(|ui| {
                for page in Page::ALL {
                    let active = !searching && state.page == page;
                    let (rect, resp) = ui.allocate_exact_size(
                        Vec2::new(
                            ui.painter()
                                .layout_no_wrap(page.name().into(), egui::FontId::proportional(13.0), p.ink)
                                .size()
                                .x
                                + 2.0 * m.space_3,
                            m.control,
                        ),
                        Sense::click(),
                    );
                    if active {
                        ui.painter()
                            .rect_filled(rect, CornerRadius::same(m.radius_md as u8), p.accent_subtle);
                        ui.painter().rect_filled(
                            egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - 2.0), rect.right_bottom()),
                            CornerRadius::ZERO,
                            p.accent,
                        );
                    } else if resp.hovered() {
                        ui.painter()
                            .rect_filled(rect, CornerRadius::same(m.radius_md as u8), p.surface_2);
                    }
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        page.name(),
                        egui::FontId::new(13.0, egui::FontFamily::Name(crate::fonts::SANS_SEMIBOLD.into())),
                        if active { p.ink } else { p.ink_muted },
                    );
                    if resp.clicked() {
                        state.page = page;
                        state.query.clear();
                    }
                }
            });
            ui.painter()
                .hline(ui.min_rect().x_range(), ui.cursor().top(), Stroke::new(1.0, p.line));
            ui.add_space(m.space_3);
            let all = topics();
            let shown: Vec<&Topic> = if searching {
                search(&all, &state.query)
            } else {
                all.iter().filter(|t| t.page == state.page).collect()
            };
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                if searching {
                    ui.label(
                        RichText::new(format!(
                            "{} {} for \"{}\"",
                            shown.len(),
                            if shown.len() == 1 { "entry" } else { "entries" },
                            state.query.trim()
                        ))
                        .color(p.ink_muted),
                    );
                    ui.add_space(m.space_2);
                }
                let mut section = "";
                for t in shown {
                    let heading = if searching { t.page.name() } else { t.section };
                    if heading != section {
                        section = heading;
                        ui.add_space(m.space_2);
                        ui.label(caption(heading, p.ink_muted));
                        ui.add_space(m.space_1);
                    }
                    topic_row(ui, p, m, t);
                }
                if searching && search(&all, &state.query).is_empty() {
                    ui.label(RichText::new("Nothing in Help matches. Try fewer words.").color(p.ink_muted));
                }
            });
        });
    if !open {
        state.open = false;
    }
}

fn topic_row(ui: &mut egui::Ui, p: &Palette, m: &Metrics, t: &Topic) {
    egui::Frame::new()
        .inner_margin(Margin::symmetric(0, m.space_1 as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(&t.title)
                        .text_style(egui::TextStyle::Name("body-strong".into()))
                        .color(p.ink),
                );
                if !t.keys.is_empty() {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let colour = match (t.page, t.keys.as_str()) {
                            (Page::Workspaces, "Available") => p.status_ok,
                            (Page::Workspaces, _) => p.ink_muted,
                            _ => p.ink,
                        };
                        egui::Frame::new()
                            .fill(p.surface_2)
                            .stroke(Stroke::new(1.0, p.line))
                            .corner_radius(CornerRadius::same(m.radius_sm as u8))
                            .inner_margin(Margin::symmetric(m.space_1 as i8 + 2, 1))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(&t.keys)
                                        .text_style(egui::TextStyle::Name("value".into()))
                                        .color(colour),
                                );
                            });
                    });
                }
            });
            ui.add(egui::Label::new(RichText::new(&t.body).color(p.ink_muted)).wrap());
        });
    ui.painter().hline(
        ui.min_rect().x_range(),
        ui.cursor().top() - 1.0,
        Stroke::new(1.0, p.line),
    );
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Help is generated from the registry: every action is there once, under its own name, with its shortcut (or
    /// the palette) and its description; an action without a description fails here.
    #[test]
    fn every_action_is_in_help() {
        let t = topics();
        for e in actions::registry() {
            assert!(
                e.help.trim().len() >= 12 && e.help.chars().next().is_some_and(char::is_uppercase),
                "{:?} has no real description: {:?}",
                e.action,
                e.help
            );
            assert!(!e.help.ends_with('.'), "{:?}: a help line has no full stop", e.action);
            let hits: Vec<&Topic> = t
                .iter()
                .filter(|x| x.section == "Commands" && x.title == e.name)
                .collect();
            assert_eq!(hits.len(), 1, "{} in Help", e.name);
            let h = hits.first().unwrap();
            assert_eq!(h.body, e.help);
            match e.shortcut {
                Some(sc) => assert_eq!(h.keys, shortcut_text(&sc)),
                None => assert_eq!(h.keys, "Command palette"),
            }
        }
        let commands = t.iter().filter(|x| x.section == "Commands").count();
        assert_eq!(commands, actions::registry().len());
        // the views' controls and the workspaces are there, each with a description
        for c in VIEW_CONTROLS {
            assert!(c.help.len() >= 12, "{}", c.name);
            assert!(t.iter().any(|x| x.title == c.name && x.keys == c.keys));
        }
        for w in WORKSPACES {
            assert!(w.available == w.when.is_empty(), "{}", w.name);
            assert!(t.iter().any(|x| x.page == Page::Workspaces && x.title == w.name));
        }
        assert_eq!(
            t.iter().filter(|x| x.page == Page::GettingStarted).count(),
            GETTING_STARTED.len()
        );
    }

    #[test]
    fn search_and_hint() {
        let t = topics();
        let fly = search(&t, "FLY");
        assert!(fly.iter().any(|x| x.title == "Fly"));
        assert!(search(&t, "f1").iter().any(|x| x.title == "Help" && x.keys == "F1"));
        assert!(search(&t, "ctrl+shift+p").iter().any(|x| x.title == "Command palette"));
        assert!(search(&t, "faction").iter().any(|x| x.page == Page::GettingStarted));
        assert!(search(&t, "gameplay checklist").iter().any(|x| x.title == "Gameplay"));
        assert_eq!(search(&t, "no such words anywhere"), Vec::<&Topic>::new());
        assert_eq!(search(&t, "").len(), t.len());
        let h = corner_hint(false);
        assert!(
            h.contains("Orbit: left drag") && h.contains("Fly: hold right, W A S D, Q E"),
            "{h}"
        );
        assert!(h.contains("Focus selection: F") && h.contains("Help: F1"), "{h}");
        let o = corner_hint(true);
        assert!(
            o.contains("Pan: left or middle drag") && !o.contains("Fly") && !o.contains("Orbit"),
            "{o}"
        );
    }
}
