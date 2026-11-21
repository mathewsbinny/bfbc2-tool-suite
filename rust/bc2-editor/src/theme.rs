//! The suite's design language as an egui theme, generated from `design/project/tokens.json` (the one source; the
//! file is embedded at build time, never copied by hand).
//!
//! Every token reaches the interface: the colour tokens set [`egui::Visuals`] (grounds, ink, borders, accent, focus,
//! status), the dock's style ([`dock_style`]: tabs, separators, the drop overlay) or the [`Palette`] the 3D view and
//! the editor's own widgets read (teams, selection, navigation, axes, `on-accent`, `ink-faint`); the type styles become
//! named [`egui::TextStyle`]s in IBM Plex Sans / Mono at their sizes and weights; spacing, radius, size and shadow
//! tokens set egui's spacing, corner radii and shadows ([`Metrics`]). `tests` checks each one, in both themes.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Margin, Shadow, Stroke, Style, TextStyle, Visuals,
};
use serde_json::Value;

use crate::fonts;

/// tokens.json, as the design-system artifact exports it.
pub const TOKENS_JSON: &str = include_str!("../../../design/project/tokens.json");

/// Which theme: Dark is the default, Light follows the system or the user's choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, Default)]
pub enum ThemeId {
    /// The default.
    #[default]
    Dark,
    /// Mid-grey panels.
    Light,
}

impl ThemeId {
    /// The theme's id in tokens.json.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    /// Both themes, Dark first.
    pub const ALL: [Self; 2] = [Self::Dark, Self::Light];
}

/// A type style of tokens.json (`display`, `body`, `value` ...).
#[derive(Debug, Clone, PartialEq)]
pub struct TypeStyle {
    /// Its name.
    pub name: String,
    /// `display`, `sans` or `mono`.
    pub family: String,
    /// In points (the tokens' px at 1:1).
    pub size: f32,
    /// Line height in points.
    pub line_height: f32,
    /// 400 or 600.
    pub weight: u32,
    /// Extra letter spacing in em (`label`).
    pub letter_spacing: f32,
}

impl TypeStyle {
    /// The egui font family: Plex Mono for `mono`, Plex Sans (regular or semibold) otherwise.
    #[must_use]
    pub fn font_family(&self) -> FontFamily {
        if self.family == "mono" {
            FontFamily::Monospace
        } else if self.weight >= 600 {
            FontFamily::Name(fonts::SANS_SEMIBOLD.into())
        } else {
            FontFamily::Proportional
        }
    }

    /// The egui font.
    #[must_use]
    pub fn font_id(&self) -> FontId {
        FontId::new(self.size, self.font_family())
    }
}

/// Why tokens.json could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenError(pub String);

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "tokens.json: {}", self.0)
    }
}

impl std::error::Error for TokenError {}

/// tokens.json, read.
#[derive(Debug, Clone, PartialEq)]
pub struct Tokens {
    /// Colour tokens in file order: name, then (theme key, colour).
    pub colors: Vec<(String, BTreeMap<String, Color32>)>,
    /// Type styles in file order.
    pub types: Vec<TypeStyle>,
    /// Spacing, radius and size tokens (px), by name (`space-2`, `radius-md`, `row` ...).
    pub lengths: BTreeMap<String, f32>,
    /// Shadow tokens: name, then (theme key, shadow).
    pub shadows: Vec<(String, BTreeMap<String, Shadow>)>,
}

fn px(v: &Value) -> Result<f32, TokenError> {
    let s = v
        .as_str()
        .ok_or_else(|| TokenError(format!("{v} is not a px length")))?;
    s.trim()
        .strip_suffix("px")
        .and_then(|n| n.trim().parse::<f32>().ok())
        .ok_or_else(|| TokenError(format!("{s:?} is not a px length")))
}

/// `#rrggbb` (or `#rrggbbaa`) as a colour.
///
/// # Errors
///
/// Anything else.
pub fn hex(s: &str) -> Result<Color32, TokenError> {
    let h = s
        .strip_prefix('#')
        .ok_or_else(|| TokenError(format!("{s:?} is not #rrggbb")))?;
    let byte = |k: usize| {
        h.get(k..k + 2)
            .and_then(|b| u8::from_str_radix(b, 16).ok())
            .ok_or_else(|| TokenError(format!("{s:?} is not #rrggbb")))
    };
    match h.len() {
        6 => Ok(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?)),
        8 => Ok(Color32::from_rgba_unmultiplied(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
        _ => Err(TokenError(format!("{s:?} is not #rrggbb"))),
    }
}

/// A CSS `box-shadow` of tokens.json (`0 6px 18px rgba(0, 0, 0, 0.45)`) as an egui shadow.
///
/// # Errors
///
/// Another form.
pub fn shadow(s: &str) -> Result<Shadow, TokenError> {
    let bad = || TokenError(format!("{s:?} is not 'x y blur rgba(r, g, b, a)'"));
    let (lengths, colour) = s.split_once("rgba(").ok_or_else(bad)?;
    let n: Vec<f32> = lengths
        .split_whitespace()
        .map(|p| p.trim_end_matches("px").parse::<f32>())
        .collect::<Result<_, _>>()
        .map_err(|_| bad())?;
    let c: Vec<f32> = colour
        .trim_end_matches(')')
        .split(',')
        .map(|p| p.trim().parse::<f32>())
        .collect::<Result<_, _>>()
        .map_err(|_| bad())?;
    let (&[x, y, blur], &[r, g, b, a]) = (n.as_slice(), c.as_slice()) else {
        return Err(bad());
    };
    let byte = |v: f32| v.round().clamp(0.0, 255.0) as u8;
    Ok(Shadow {
        offset: [x.round() as i8, y.round() as i8],
        blur: byte(blur),
        spread: 0,
        color: Color32::from_rgba_unmultiplied(byte(r), byte(g), byte(b), byte(a * 255.0)),
    })
}

impl Tokens {
    /// tokens.json embedded in the editor, read once.
    ///
    /// # Errors
    ///
    /// The embedded file does not read (`tests` makes sure it does).
    pub fn embedded() -> Result<&'static Self, TokenError> {
        static TOKENS: OnceLock<Result<Tokens, TokenError>> = OnceLock::new();
        TOKENS
            .get_or_init(|| Self::parse(TOKENS_JSON))
            .as_ref()
            .map_err(Clone::clone)
    }

    /// Reads a tokens.json.
    ///
    /// # Errors
    ///
    /// Not JSON, or a token that is not in the form the design system exports.
    #[allow(
        clippy::indexing_slicing,
        reason = "serde_json's Value[key] gives Null for what is missing, never panics"
    )]
    pub fn parse(json: &str) -> Result<Self, TokenError> {
        let root: Value = serde_json::from_str(json).map_err(|e| TokenError(e.to_string()))?;
        let list = |v: &Value, what: &str| -> Result<Vec<Value>, TokenError> {
            v.as_array()
                .cloned()
                .ok_or_else(|| TokenError(format!("{what} is not a list")))
        };
        let name_of = |t: &Value| -> Result<String, TokenError> {
            t.get("name")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| TokenError(format!("a token without a name: {t}")))
        };
        let mut colors = Vec::new();
        for t in list(&root["color"]["tokens"], "color.tokens")? {
            let mut by = BTreeMap::new();
            for theme in ThemeId::ALL {
                let v = t["value"][theme.key()].as_str().ok_or_else(|| {
                    TokenError(format!(
                        "{} has no {} value",
                        name_of(&t).unwrap_or_default(),
                        theme.key()
                    ))
                })?;
                by.insert(theme.key().to_owned(), hex(v)?);
            }
            colors.push((name_of(&t)?, by));
        }
        let mut types = Vec::new();
        for group in list(&root["type"]["groups"], "type.groups")? {
            let family = group["family"].as_str().unwrap_or("sans").to_owned();
            for s in list(&group["styles"], "type styles")? {
                let em = s
                    .get("letterSpacing")
                    .and_then(Value::as_str)
                    .and_then(|v| v.strip_suffix("em"))
                    .and_then(|v| v.parse::<f32>().ok())
                    .unwrap_or(0.0);
                types.push(TypeStyle {
                    name: name_of(&s)?,
                    family: family.clone(),
                    size: px(&s["fontSize"])?,
                    line_height: px(&s["lineHeight"])?,
                    weight: s["fontWeight"].as_u64().unwrap_or(400) as u32,
                    letter_spacing: em,
                });
            }
        }
        let mut lengths = BTreeMap::new();
        for group in ["spacing", "radius", "size"] {
            for t in list(&root[group]["tokens"], group)? {
                lengths.insert(name_of(&t)?, px(&t["value"])?);
            }
        }
        let mut shadows = Vec::new();
        for t in list(&root["shadow"]["tokens"], "shadow.tokens")? {
            let mut by = BTreeMap::new();
            for theme in ThemeId::ALL {
                let v = t["value"][theme.key()]
                    .as_str()
                    .ok_or_else(|| TokenError(format!("shadow without a {} value", theme.key())))?;
                by.insert(theme.key().to_owned(), shadow(v)?);
            }
            shadows.push((name_of(&t)?, by));
        }
        Ok(Self {
            colors,
            types,
            lengths,
            shadows,
        })
    }

    /// Colour token `name` in `theme` (magenta when there is no such token: never silently grey).
    #[must_use]
    pub fn color(&self, theme: ThemeId, name: &str) -> Color32 {
        self.colors
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, by)| by.get(theme.key()).copied())
            .unwrap_or(Color32::from_rgb(255, 0, 255))
    }

    /// Length token `name` in points (0 when there is none).
    #[must_use]
    pub fn length(&self, name: &str) -> f32 {
        self.lengths.get(name).copied().unwrap_or(0.0)
    }

    /// Type style `name` (the `body` style when there is none).
    #[must_use]
    pub fn text(&self, name: &str) -> TypeStyle {
        self.types
            .iter()
            .find(|t| t.name == name)
            .or_else(|| self.types.iter().find(|t| t.name == "body"))
            .cloned()
            .unwrap_or(TypeStyle {
                name: name.to_owned(),
                family: "sans".into(),
                size: 13.0,
                line_height: 18.0,
                weight: 400,
                letter_spacing: 0.0,
            })
    }

    /// Shadow token `name` in `theme`.
    #[must_use]
    pub fn shadow(&self, theme: ThemeId, name: &str) -> Shadow {
        self.shadows
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, by)| by.get(theme.key()).copied())
            .unwrap_or(Shadow::NONE)
    }

    /// The egui style of `theme`: colours, text styles, spacing, radii and shadows from the tokens.
    #[must_use]
    pub fn style(&self, theme: ThemeId) -> Style {
        let c = |n: &str| self.color(theme, n);
        let l = |n: &str| self.length(n);
        let r = |n: &str| CornerRadius::same(l(n).round() as u8);
        let mut v = if theme == ThemeId::Dark {
            Visuals::dark()
        } else {
            Visuals::light()
        };
        v.panel_fill = c("surface-1");
        v.window_fill = c("surface-1");
        v.window_stroke = Stroke::new(1.0, c("line"));
        v.extreme_bg_color = c("surface-2");
        v.text_edit_bg_color = Some(c("surface-2"));
        v.faint_bg_color = c("surface-2");
        v.code_bg_color = c("surface-2");
        v.hyperlink_color = c("accent-text");
        v.warn_fg_color = c("status-warn");
        v.error_fg_color = c("status-error");
        v.override_text_color = None;
        v.weak_text_color = Some(c("ink-muted"));
        v.selection.bg_fill = c("accent-subtle");
        v.selection.stroke = Stroke::new(1.0, c("ink"));
        v.text_cursor.stroke = Stroke::new(2.0, c("focus"));
        v.window_corner_radius = r("radius-lg");
        v.menu_corner_radius = r("radius-md");
        v.window_shadow = self.shadow(theme, "shadow-popover");
        v.popup_shadow = self.shadow(theme, "shadow-popover");
        // the widget states: resting text on panels, controls, hover, pressed / focused (the focus ring), open
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = c("surface-1");
        w.noninteractive.weak_bg_fill = c("surface-1");
        w.noninteractive.bg_stroke = Stroke::new(1.0, c("line"));
        w.noninteractive.fg_stroke = Stroke::new(1.0, c("ink"));
        w.noninteractive.corner_radius = r("radius-sm");
        w.inactive.bg_fill = c("surface-2");
        w.inactive.weak_bg_fill = c("surface-2");
        w.inactive.bg_stroke = Stroke::new(1.0, c("line-strong"));
        w.inactive.fg_stroke = Stroke::new(1.0, c("ink"));
        w.inactive.corner_radius = r("radius-md");
        w.hovered.bg_fill = c("surface-3");
        w.hovered.weak_bg_fill = c("surface-3");
        w.hovered.bg_stroke = Stroke::new(1.0, c("line-strong"));
        w.hovered.fg_stroke = Stroke::new(1.0, c("ink"));
        w.hovered.corner_radius = r("radius-md");
        w.hovered.expansion = 0.0;
        w.active.bg_fill = c("surface-3");
        w.active.weak_bg_fill = c("surface-3");
        w.active.bg_stroke = Stroke::new(2.0, c("focus"));
        w.active.fg_stroke = Stroke::new(1.0, c("ink"));
        w.active.corner_radius = r("radius-md");
        w.active.expansion = 0.0;
        w.open.bg_fill = c("surface-3");
        w.open.weak_bg_fill = c("surface-3");
        w.open.bg_stroke = Stroke::new(1.0, c("line-strong"));
        w.open.fg_stroke = Stroke::new(1.0, c("ink"));
        w.open.corner_radius = r("radius-md");
        v.striped = false;
        v.indent_has_left_vline = true;

        let mut style = Style {
            visuals: v,
            ..Style::default()
        };
        // type: egui's five built-in styles take the design's names, and every design style is there by name
        let mut ts = BTreeMap::new();
        for t in &self.types {
            ts.insert(TextStyle::Name(t.name.as_str().into()), t.font_id());
        }
        ts.insert(TextStyle::Small, self.text("small").font_id());
        ts.insert(TextStyle::Body, self.text("body").font_id());
        ts.insert(TextStyle::Button, self.text("body-strong").font_id());
        ts.insert(TextStyle::Heading, self.text("title").font_id());
        ts.insert(TextStyle::Monospace, self.text("value").font_id());
        style.text_styles = ts;
        // spacing and sizes: 4px base, 22px rows, 26px controls
        let s = &mut style.spacing;
        s.item_spacing = egui::vec2(l("space-2"), l("space-1"));
        s.button_padding = egui::vec2(
            l("space-2"),
            ((l("control") - self.text("body-strong").line_height) / 2.0).max(0.0),
        );
        s.window_margin = Margin::same(l("space-3").round() as i8);
        s.menu_margin = Margin::same(l("space-1_5").round() as i8);
        s.indent = l("space-4");
        s.interact_size.y = l("row");
        s.icon_width = l("icon") - l("space-0_5");
        s.icon_width_inner = (l("icon") - l("space-0_5")) / 2.0;
        s.icon_spacing = l("space-1");
        style.animation_time = 0.12;
        style.url_in_tooltip = false;
        style
    }
}

/// The colours the editor draws with itself (not through egui's widgets): the 3D view's data colours and the few
/// interface colours egui's style has no slot for. Each field is the token of the same name.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(missing_docs)]
pub struct Palette {
    pub ground: Color32,
    pub surface_1: Color32,
    pub surface_2: Color32,
    pub surface_3: Color32,
    pub line: Color32,
    pub ink: Color32,
    pub ink_muted: Color32,
    pub ink_faint: Color32,
    pub accent: Color32,
    pub on_accent: Color32,
    pub accent_text: Color32,
    pub accent_subtle: Color32,
    pub selection: Color32,
    pub focus: Color32,
    pub team_1: Color32,
    pub team_2: Color32,
    pub team_neutral: Color32,
    pub unowned: Color32,
    pub status_ok: Color32,
    pub status_warn: Color32,
    pub status_error: Color32,
    pub status_info: Color32,
    pub nav_walkable: Color32,
    pub nav_blocked: Color32,
    pub nav_route: Color32,
    pub axis_x: Color32,
    pub axis_y: Color32,
    pub axis_z: Color32,
    pub view_team_1: Color32,
    pub view_team_2: Color32,
    pub view_team_neutral: Color32,
    pub view_unowned: Color32,
    pub view_selection: Color32,
    pub view_status_ok: Color32,
    pub view_status_warn: Color32,
    pub view_status_error: Color32,
    pub view_status_info: Color32,
    pub view_link: Color32,
    pub view_fx_effect: Color32,
    pub view_fx_light: Color32,
    pub view_fx_sound: Color32,
    pub view_nav_main: Color32,
    pub view_nav_ground: Color32,
    pub view_nav_ground_dice: Color32,
    pub view_nav_extra: Color32,
    pub view_nav_missing: Color32,
    pub view_nav_ladder: Color32,
    pub view_nav_normal: Color32,
    pub view_nav_dimmed: Color32,
    pub view_nav_small: Color32,
    pub view_nav_outline: Color32,
    pub view_nav_portal: Color32,
    pub view_nav_border: Color32,
    pub view_nav_graph: Color32,
    pub view_nav_route_navkit: Color32,
    pub view_nav_route_dice: Color32,
    pub view_nav_edit_add: Color32,
    pub view_nav_edit_remove: Color32,
    pub view_nav_edit_link: Color32,
    pub view_nav_edit_mark: Color32,
    pub view_ref_ai: Color32,
    pub view_ref_human: Color32,
    pub view_ref_order: Color32,
    pub view_ref_patrol: Color32,
    pub view_ref_objective: Color32,
    pub view_ref_outline: Color32,
    pub view_diff_added: Color32,
    pub view_diff_removed: Color32,
    pub view_diff_moved: Color32,
    pub view_diff_changed: Color32,
    pub scene_sky: Color32,
    pub scene_terrain: Color32,
    pub scene_water: Color32,
    pub scene_object: Color32,
    pub scene_object_destructible: Color32,
    pub scene_object_dynamic: Color32,
    pub scene_object_group: Color32,
    pub scene_ladder: Color32,
    pub scene_effect: Color32,
    pub scene_area_point: Color32,
    pub scene_other: Color32,
    pub scene_label: Color32,
}

impl Palette {
    /// The palette of `theme`.
    #[must_use]
    pub fn new(tokens: &Tokens, theme: ThemeId) -> Self {
        let c = |n: &str| tokens.color(theme, n);
        Self {
            ground: c("ground"),
            surface_1: c("surface-1"),
            surface_2: c("surface-2"),
            surface_3: c("surface-3"),
            line: c("line"),
            ink: c("ink"),
            ink_muted: c("ink-muted"),
            ink_faint: c("ink-faint"),
            accent: c("accent"),
            on_accent: c("on-accent"),
            accent_text: c("accent-text"),
            accent_subtle: c("accent-subtle"),
            selection: c("selection"),
            focus: c("focus"),
            team_1: c("team-1"),
            team_2: c("team-2"),
            team_neutral: c("team-neutral"),
            unowned: c("unowned"),
            status_ok: c("status-ok"),
            status_warn: c("status-warn"),
            status_error: c("status-error"),
            status_info: c("status-info"),
            nav_walkable: c("nav-walkable"),
            nav_blocked: c("nav-blocked"),
            nav_route: c("nav-route"),
            axis_x: c("axis-x"),
            axis_y: c("axis-y"),
            axis_z: c("axis-z"),
            view_team_1: c("view-team-1"),
            view_team_2: c("view-team-2"),
            view_team_neutral: c("view-team-neutral"),
            view_unowned: c("view-unowned"),
            view_selection: c("view-selection"),
            view_status_ok: c("view-status-ok"),
            view_status_warn: c("view-status-warn"),
            view_status_error: c("view-status-error"),
            view_status_info: c("view-status-info"),
            view_link: c("view-link"),
            view_fx_effect: c("view-fx-effect"),
            view_fx_light: c("view-fx-light"),
            view_fx_sound: c("view-fx-sound"),
            view_nav_main: c("view-nav-main"),
            view_nav_ground: c("view-nav-ground"),
            view_nav_ground_dice: c("view-nav-ground-dice"),
            view_nav_extra: c("view-nav-extra"),
            view_nav_missing: c("view-nav-missing"),
            view_nav_ladder: c("view-nav-ladder"),
            view_nav_normal: c("view-nav-normal"),
            view_nav_dimmed: c("view-nav-dimmed"),
            view_nav_small: c("view-nav-small"),
            view_nav_outline: c("view-nav-outline"),
            view_nav_portal: c("view-nav-portal"),
            view_nav_border: c("view-nav-border"),
            view_nav_graph: c("view-nav-graph"),
            view_nav_route_navkit: c("view-nav-route-navkit"),
            view_nav_route_dice: c("view-nav-route-dice"),
            view_nav_edit_add: c("view-nav-edit-add"),
            view_nav_edit_remove: c("view-nav-edit-remove"),
            view_nav_edit_link: c("view-nav-edit-link"),
            view_nav_edit_mark: c("view-nav-edit-mark"),
            view_ref_ai: c("view-ref-ai"),
            view_ref_human: c("view-ref-human"),
            view_ref_order: c("view-ref-order"),
            view_ref_patrol: c("view-ref-patrol"),
            view_ref_objective: c("view-ref-objective"),
            view_ref_outline: c("view-ref-outline"),
            view_diff_added: c("view-diff-added"),
            view_diff_removed: c("view-diff-removed"),
            view_diff_moved: c("view-diff-moved"),
            view_diff_changed: c("view-diff-changed"),
            scene_sky: c("scene-sky"),
            scene_terrain: c("scene-terrain"),
            scene_water: c("scene-water"),
            scene_object: c("scene-object"),
            scene_object_destructible: c("scene-object-destructible"),
            scene_object_dynamic: c("scene-object-dynamic"),
            scene_object_group: c("scene-object-group"),
            scene_ladder: c("scene-ladder"),
            scene_effect: c("scene-effect"),
            scene_area_point: c("scene-area-point"),
            scene_other: c("scene-other"),
            scene_label: c("scene-label"),
        }
    }

    /// The 3D view's colours (the `view-*` and `scene-*` tokens) with their names: what the renderer and the
    /// view's overlays draw with.
    #[must_use]
    pub fn view_entries(&self) -> Vec<(&'static str, Color32)> {
        vec![
            ("view-team-1", self.view_team_1),
            ("view-team-2", self.view_team_2),
            ("view-team-neutral", self.view_team_neutral),
            ("view-unowned", self.view_unowned),
            ("view-selection", self.view_selection),
            ("view-status-ok", self.view_status_ok),
            ("view-status-warn", self.view_status_warn),
            ("view-status-error", self.view_status_error),
            ("view-status-info", self.view_status_info),
            ("view-link", self.view_link),
            ("view-fx-effect", self.view_fx_effect),
            ("view-fx-light", self.view_fx_light),
            ("view-fx-sound", self.view_fx_sound),
        ]
        .into_iter()
        .chain(self.nav_entries())
        .chain(self.diff_entries())
        .chain([
            ("scene-sky", self.scene_sky),
            ("scene-terrain", self.scene_terrain),
            ("scene-water", self.scene_water),
            ("scene-object", self.scene_object),
            ("scene-object-destructible", self.scene_object_destructible),
            ("scene-object-dynamic", self.scene_object_dynamic),
            ("scene-object-group", self.scene_object_group),
            ("scene-ladder", self.scene_ladder),
            ("scene-effect", self.scene_effect),
            ("scene-area-point", self.scene_area_point),
            ("scene-other", self.scene_other),
            ("scene-label", self.scene_label),
        ])
        .collect()
    }

    /// The AI workspace's 3D colours (round GR2: the `view-nav-*` and `view-ref-*` tokens) with their names: the
    /// navigation layers, the colourings, the path test, the edits and a reference level's AI layer.
    #[must_use]
    pub fn nav_entries(&self) -> Vec<(&'static str, Color32)> {
        vec![
            ("view-nav-main", self.view_nav_main),
            ("view-nav-ground", self.view_nav_ground),
            ("view-nav-ground-dice", self.view_nav_ground_dice),
            ("view-nav-extra", self.view_nav_extra),
            ("view-nav-missing", self.view_nav_missing),
            ("view-nav-ladder", self.view_nav_ladder),
            ("view-nav-normal", self.view_nav_normal),
            ("view-nav-dimmed", self.view_nav_dimmed),
            ("view-nav-small", self.view_nav_small),
            ("view-nav-outline", self.view_nav_outline),
            ("view-nav-portal", self.view_nav_portal),
            ("view-nav-border", self.view_nav_border),
            ("view-nav-graph", self.view_nav_graph),
            ("view-nav-route-navkit", self.view_nav_route_navkit),
            ("view-nav-route-dice", self.view_nav_route_dice),
            ("view-nav-edit-add", self.view_nav_edit_add),
            ("view-nav-edit-remove", self.view_nav_edit_remove),
            ("view-nav-edit-link", self.view_nav_edit_link),
            ("view-nav-edit-mark", self.view_nav_edit_mark),
            ("view-ref-ai", self.view_ref_ai),
            ("view-ref-human", self.view_ref_human),
            ("view-ref-order", self.view_ref_order),
            ("view-ref-patrol", self.view_ref_patrol),
            ("view-ref-objective", self.view_ref_objective),
            ("view-ref-outline", self.view_ref_outline),
        ]
    }

    /// The Diff panel's 3D colours (round EX1: the `view-diff-*` tokens) with their names: added, removed, moved,
    /// changed.
    #[must_use]
    pub fn diff_entries(&self) -> Vec<(&'static str, Color32)> {
        vec![
            ("view-diff-added", self.view_diff_added),
            ("view-diff-removed", self.view_diff_removed),
            ("view-diff-moved", self.view_diff_moved),
            ("view-diff-changed", self.view_diff_changed),
        ]
    }

    /// A team's colour in the 3D view, on the terrain: 1, 2, or team-neutral (0 and anything else).
    #[must_use]
    pub fn view_team(&self, team: u8) -> Color32 {
        match team {
            1 => self.view_team_1,
            2 => self.view_team_2,
            _ => self.view_team_neutral,
        }
    }

    /// Every field with its token's name (for the tests: each one is the token it says).
    #[must_use]
    pub fn entries(&self) -> Vec<(&'static str, Color32)> {
        vec![
            ("ground", self.ground),
            ("surface-1", self.surface_1),
            ("surface-2", self.surface_2),
            ("surface-3", self.surface_3),
            ("line", self.line),
            ("ink", self.ink),
            ("ink-muted", self.ink_muted),
            ("ink-faint", self.ink_faint),
            ("accent", self.accent),
            ("on-accent", self.on_accent),
            ("accent-text", self.accent_text),
            ("accent-subtle", self.accent_subtle),
            ("selection", self.selection),
            ("focus", self.focus),
            ("team-1", self.team_1),
            ("team-2", self.team_2),
            ("team-neutral", self.team_neutral),
            ("unowned", self.unowned),
            ("status-ok", self.status_ok),
            ("status-warn", self.status_warn),
            ("status-error", self.status_error),
            ("status-info", self.status_info),
            ("nav-walkable", self.nav_walkable),
            ("nav-blocked", self.nav_blocked),
            ("nav-route", self.nav_route),
            ("axis-x", self.axis_x),
            ("axis-y", self.axis_y),
            ("axis-z", self.axis_z),
        ]
        .into_iter()
        .chain(self.view_entries())
        .collect()
    }

    /// A team's colour: 1, 2, or neutral (0 and anything else).
    #[must_use]
    pub fn team(&self, team: u8) -> Color32 {
        match team {
            1 => self.team_1,
            2 => self.team_2,
            _ => self.team_neutral,
        }
    }
}

/// A colour's relative luminance (WCAG 2): sRGB channels to linear light, weighted.
#[must_use]
pub fn luminance(c: Color32) -> f64 {
    let lin = |v: u8| {
        let s = f64::from(v) / 255.0;
        if s <= 0.040_45 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
}

/// The WCAG 2 contrast ratio of two colours (1 to 21).
#[must_use]
pub fn contrast(a: Color32, b: Color32) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// A colour with each channel scaled by `k`, as the view's shader lights it (`view.wgsl`: the colour times the
/// shade, in the stored gamma values).
#[must_use]
pub fn shaded(c: Color32, k: f64) -> Color32 {
    let s = |v: u8| (f64::from(v) * k).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(s(c.r()), s(c.g()), s(c.b()))
}

/// A colour's hue in degrees (0 - 360) and its saturation (HSL, 0 - 1).
#[must_use]
pub fn hue_saturation(c: Color32) -> (f64, f64) {
    let [r, g, b] = [c.r(), c.g(), c.b()].map(|v| f64::from(v) / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let d = max - min;
    if d <= f64::EPSILON {
        return (0.0, 0.0);
    }
    let l = f64::midpoint(max, min);
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if (max - r).abs() <= f64::EPSILON {
        ((g - b) / d).rem_euclid(6.0)
    } else if (max - g).abs() <= f64::EPSILON {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h * 60.0, s)
}

/// The distance between two hues, in degrees (0 - 180).
#[must_use]
pub fn hue_distance(a: Color32, b: Color32) -> f64 {
    let d = (hue_saturation(a).0 - hue_saturation(b).0).abs() % 360.0;
    d.min(360.0 - d)
}

/// The sizes the editor lays out with (the size tokens, in points).
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(missing_docs)]
pub struct Metrics {
    pub row: f32,
    pub control: f32,
    pub toolbar: f32,
    pub panel_header: f32,
    pub icon: f32,
    pub space_1: f32,
    pub space_2: f32,
    pub space_3: f32,
    pub space_4: f32,
    pub space_6: f32,
    pub space_8: f32,
    pub radius_sm: f32,
    pub radius_md: f32,
    pub radius_lg: f32,
}

impl Metrics {
    /// From the tokens.
    #[must_use]
    pub fn new(tokens: &Tokens) -> Self {
        let l = |n: &str| tokens.length(n);
        Self {
            row: l("row"),
            control: l("control"),
            toolbar: l("toolbar"),
            panel_header: l("panel-header"),
            icon: l("icon"),
            space_1: l("space-1"),
            space_2: l("space-2"),
            space_3: l("space-3"),
            space_4: l("space-4"),
            space_6: l("space-6"),
            space_8: l("space-8"),
            radius_sm: l("radius-sm"),
            radius_md: l("radius-md"),
            radius_lg: l("radius-lg"),
        }
    }
}

/// The dock's look (`egui_dock`): tabs as the Panel component says (the active tab `surface-3` with an `accent`
/// underline, the others `ink-muted` on `surface-1`), `line` hairlines between docked panels, `ground` behind them.
#[must_use]
pub fn dock_style(tokens: &Tokens, theme: ThemeId, style: &Style) -> egui_dock::Style {
    let c = |n: &str| tokens.color(theme, n);
    let l = |n: &str| tokens.length(n);
    let mut d = egui_dock::Style::from_egui(style);
    d.main_surface_border_stroke = Stroke::new(1.0, c("line"));
    d.main_surface_border_rounding = CornerRadius::ZERO;
    d.dock_area_padding = None;
    d.separator.width = 1.0;
    d.separator.color_idle = c("line");
    d.separator.color_hovered = c("line-strong");
    d.separator.color_dragged = c("accent");
    d.tab_bar.bg_fill = c("ground");
    d.tab_bar.height = l("panel-header");
    d.tab_bar.hline_color = c("line");
    d.tab_bar.corner_radius = CornerRadius::ZERO;
    let tab = |bg: &str, text: &str, outline: &str| egui_dock::TabInteractionStyle {
        outline_color: c(outline),
        corner_radius: CornerRadius {
            nw: l("radius-md").round() as u8,
            ne: l("radius-md").round() as u8,
            sw: 0,
            se: 0,
        },
        bg_fill: c(bg),
        text_color: c(text),
    };
    d.tab.active = tab("surface-3", "ink", "surface-3");
    d.tab.focused = tab("surface-3", "ink", "surface-3");
    d.tab.inactive = tab("surface-1", "ink-muted", "surface-1");
    d.tab.hovered = tab("surface-2", "ink", "surface-2");
    d.tab.active_with_kb_focus = tab("surface-3", "ink", "focus");
    d.tab.focused_with_kb_focus = tab("surface-3", "ink", "focus");
    d.tab.inactive_with_kb_focus = tab("surface-1", "ink-muted", "focus");
    d.tab.hline_below_active_tab_name = true;
    d.tab.tab_body.bg_fill = c("surface-1");
    d.tab.tab_body.stroke = Stroke::new(1.0, c("line"));
    d.tab.tab_body.corner_radius = CornerRadius::ZERO;
    d.tab.tab_body.inner_margin = Margin::ZERO;
    d.overlay.selection_color = c("accent").gamma_multiply(0.35);
    d.overlay.button_color = c("ink-muted");
    d.overlay.button_border_stroke = Stroke::new(1.0, c("line-strong"));
    d.buttons.close_tab_color = c("ink-muted");
    d.buttons.close_tab_active_color = c("ink");
    d.buttons.close_tab_bg_fill = c("surface-3");
    d
}

/// The colours a style and dock style hold, each with the token it must be (for the tests and for a reader: where
/// each token goes).
#[must_use]
pub fn style_slots(style: &Style, dock: &egui_dock::Style) -> Vec<(&'static str, &'static str, Color32)> {
    let v = &style.visuals;
    let w = &v.widgets;
    vec![
        ("panel_fill", "surface-1", v.panel_fill),
        ("window_fill", "surface-1", v.window_fill),
        ("window_stroke", "line", v.window_stroke.color),
        ("extreme_bg_color", "surface-2", v.extreme_bg_color),
        (
            "text_edit_bg_color",
            "surface-2",
            v.text_edit_bg_color.unwrap_or_default(),
        ),
        ("faint_bg_color", "surface-2", v.faint_bg_color),
        ("hyperlink_color", "accent-text", v.hyperlink_color),
        ("warn_fg_color", "status-warn", v.warn_fg_color),
        ("error_fg_color", "status-error", v.error_fg_color),
        ("weak_text_color", "ink-muted", v.weak_text_color.unwrap_or_default()),
        ("selection.bg_fill", "accent-subtle", v.selection.bg_fill),
        ("selection.stroke", "ink", v.selection.stroke.color),
        ("text_cursor", "focus", v.text_cursor.stroke.color),
        ("noninteractive.bg_fill", "surface-1", w.noninteractive.bg_fill),
        ("noninteractive.bg_stroke", "line", w.noninteractive.bg_stroke.color),
        ("noninteractive.fg_stroke", "ink", w.noninteractive.fg_stroke.color),
        ("inactive.bg_fill", "surface-2", w.inactive.bg_fill),
        ("inactive.bg_stroke", "line-strong", w.inactive.bg_stroke.color),
        ("inactive.fg_stroke", "ink", w.inactive.fg_stroke.color),
        ("hovered.bg_fill", "surface-3", w.hovered.bg_fill),
        ("active.bg_fill", "surface-3", w.active.bg_fill),
        ("active.bg_stroke (focus ring)", "focus", w.active.bg_stroke.color),
        ("open.bg_fill", "surface-3", w.open.bg_fill),
        ("dock tab bar", "ground", dock.tab_bar.bg_fill),
        ("dock separator", "line", dock.separator.color_idle),
        ("dock active tab", "surface-3", dock.tab.active.bg_fill),
        ("dock inactive tab text", "ink-muted", dock.tab.inactive.text_color),
        ("dock tab body", "surface-1", dock.tab.tab_body.bg_fill),
        ("dock separator dragged", "accent", dock.separator.color_dragged),
        ("dock tab focus", "focus", dock.tab.active_with_kb_focus.outline_color),
    ]
}

/// The theme applied to a context: fonts, both styles (egui keeps one per theme), and the chosen theme.
pub fn apply(ctx: &egui::Context, tokens: &Tokens, theme: ThemeId) {
    ctx.set_fonts(fonts::definitions());
    ctx.set_style_of(egui::Theme::Dark, tokens.style(ThemeId::Dark));
    ctx.set_style_of(egui::Theme::Light, tokens.style(ThemeId::Light));
    ctx.set_theme(match theme {
        ThemeId::Dark => egui::Theme::Dark,
        ThemeId::Light => egui::Theme::Light,
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::float_cmp)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn tokens() -> &'static Tokens {
        Tokens::embedded().unwrap()
    }

    #[test]
    fn reads_every_token() {
        let t = tokens();
        let raw: Value = serde_json::from_str(TOKENS_JSON).unwrap();
        assert_eq!(t.colors.len(), raw["color"]["tokens"].as_array().unwrap().len());
        assert_eq!(t.colors.len(), 83);
        let styles: usize = raw["type"]["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| g["styles"].as_array().unwrap().len())
            .sum();
        assert_eq!(t.types.len(), styles);
        assert_eq!(t.color(ThemeId::Dark, "selection"), Color32::from_rgb(0xf0, 0xa0, 0x30));
        assert_eq!(t.color(ThemeId::Light, "ground"), Color32::from_rgb(0xb4, 0xb4, 0xb4));
        assert_eq!(t.length("row"), 22.0);
        assert_eq!(t.length("space-0_5"), 2.0);
        let s = t.shadow(ThemeId::Dark, "shadow-popover");
        assert_eq!((s.offset, s.blur, s.spread), ([0, 6], 18, 0));
        assert_eq!(s.color, Color32::from_rgba_unmultiplied(0, 0, 0, 115));
    }

    /// Every colour token reaches the style, the dock style or the palette, with its own value, in both themes;
    /// a token added to tokens.json fails here until it is wired.
    #[test]
    fn every_colour_token_reaches_the_theme() {
        let t = tokens();
        for theme in ThemeId::ALL {
            let style = t.style(theme);
            let dock = dock_style(t, theme, &style);
            let mut reached = BTreeSet::new();
            for (slot, token, colour) in style_slots(&style, &dock) {
                assert_eq!(colour, t.color(theme, token), "{theme:?}: {slot} is not {token}");
                reached.insert(token);
            }
            for (token, colour) in Palette::new(t, theme).entries() {
                assert_eq!(colour, t.color(theme, token), "{theme:?}: palette {token}");
                reached.insert(token);
            }
            let all: BTreeSet<&str> = t.colors.iter().map(|(n, _)| n.as_str()).collect();
            let missing: Vec<_> = all.difference(&reached).collect();
            assert!(missing.is_empty(), "{theme:?}: tokens that reach nothing: {missing:?}");
            let unknown: Vec<_> = reached.difference(&all).collect();
            assert!(unknown.is_empty(), "{theme:?}: slots naming no token: {unknown:?}");
            assert_eq!(style.visuals.dark_mode, theme == ThemeId::Dark);
        }
        // the two themes differ where the tokens do
        assert_ne!(
            t.style(ThemeId::Dark).visuals.panel_fill,
            t.style(ThemeId::Light).visuals.panel_fill
        );
    }

    #[test]
    fn every_type_style_is_a_text_style() {
        let t = tokens();
        for theme in ThemeId::ALL {
            let style = t.style(theme);
            for ty in &t.types {
                let f = &style.text_styles[&TextStyle::Name(ty.name.as_str().into())];
                assert_eq!(f.size, ty.size, "{}", ty.name);
                let want = match (ty.family.as_str(), ty.weight) {
                    ("mono", _) => FontFamily::Monospace,
                    (_, 600) => FontFamily::Name(fonts::SANS_SEMIBOLD.into()),
                    _ => FontFamily::Proportional,
                };
                assert_eq!(f.family, want, "{}", ty.name);
            }
            assert_eq!(style.text_styles[&TextStyle::Body].size, 13.0);
            assert_eq!(style.text_styles[&TextStyle::Monospace].family, FontFamily::Monospace);
            assert_eq!(
                style.text_styles[&TextStyle::Button].family,
                FontFamily::Name(fonts::SANS_SEMIBOLD.into())
            );
            // the label style's letter spacing is kept for the editor's captions
            assert!(t.text("label").letter_spacing > 0.0);
        }
    }

    #[test]
    fn every_length_and_shadow_token_is_used() {
        let t = tokens();
        let style = t.style(ThemeId::Dark);
        let m = Metrics::new(t);
        let s = &style.spacing;
        assert_eq!(s.item_spacing, egui::vec2(t.length("space-2"), t.length("space-1")));
        assert_eq!(s.interact_size.y, t.length("row"));
        assert_eq!(s.window_margin, Margin::same(12));
        assert_eq!(s.menu_margin, Margin::same(6));
        assert_eq!(s.indent, t.length("space-4"));
        assert_eq!(s.icon_width, t.length("icon") - t.length("space-0_5"));
        // control height: body-strong's line plus the button padding, top and bottom
        assert_eq!(
            t.text("body-strong").line_height + 2.0 * s.button_padding.y,
            t.length("control")
        );
        assert_eq!(style.visuals.widgets.inactive.corner_radius, CornerRadius::same(4));
        assert_eq!(
            style.visuals.widgets.noninteractive.corner_radius,
            CornerRadius::same(2)
        );
        assert_eq!(style.visuals.window_corner_radius, CornerRadius::same(6));
        let used = [
            m.row,
            m.control,
            m.toolbar,
            m.panel_header,
            m.icon,
            m.space_1,
            m.space_2,
            m.space_3,
            m.space_4,
            m.space_6,
            m.space_8,
            m.radius_sm,
            m.radius_md,
            m.radius_lg,
        ];
        assert!(used.iter().all(|v| *v > 0.0));
        // every length token is read by the style or the metrics
        let wired = [
            "row",
            "control",
            "toolbar",
            "panel-header",
            "icon",
            "space-0_5",
            "space-1",
            "space-1_5",
            "space-2",
            "space-3",
            "space-4",
            "space-6",
            "space-8",
            "radius-sm",
            "radius-md",
            "radius-lg",
        ];
        let all: BTreeSet<&str> = t.lengths.keys().map(String::as_str).collect();
        assert_eq!(all, wired.into_iter().collect());
        for theme in ThemeId::ALL {
            let st = t.style(theme);
            assert_eq!(st.visuals.popup_shadow, t.shadow(theme, "shadow-popover"));
            assert_eq!(st.visuals.window_shadow, t.shadow(theme, "shadow-popover"));
        }
        assert_ne!(
            t.shadow(ThemeId::Dark, "shadow-popover"),
            t.shadow(ThemeId::Light, "shadow-popover")
        );
    }

    /// The README's promise, checked: text pairs 4.5:1; borders, the accent, the selection, the focus ring and the
    /// status, team, unowned and axis colours 3:1 on surface-1 and surface-2; in both themes.
    #[test]
    fn panel_contrast() {
        let t = tokens();
        for theme in ThemeId::ALL {
            let c = |n: &str| t.color(theme, n);
            for ink in ["ink", "ink-muted"] {
                for ground in ["ground", "surface-1", "surface-2", "surface-3"] {
                    let r = contrast(c(ink), c(ground));
                    assert!(r >= 4.5, "{theme:?}: {ink} on {ground} {r:.2}");
                }
            }
            for n in [
                "line-strong",
                "accent",
                "selection",
                "focus",
                "team-1",
                "team-2",
                "team-neutral",
                "unowned",
                "status-ok",
                "status-warn",
                "status-error",
                "status-info",
                "axis-x",
                "axis-y",
                "axis-z",
            ] {
                for ground in ["surface-1", "surface-2"] {
                    let r = contrast(c(n), c(ground));
                    assert!(r >= 3.0, "{theme:?}: {n} on {ground} {r:.2}");
                }
            }
        }
    }

    /// The third neutral (`unowned`) is its own colour: a violet at least 30 degrees of hue from every coloured token
    /// it could be confused with, and a saturated colour where team-neutral is a near-grey; in panels and in the
    /// view, both themes.
    #[test]
    fn unowned_is_distinct() {
        let t = tokens();
        for theme in ThemeId::ALL {
            let c = |n: &str| t.color(theme, n);
            for (own, others, neutral) in [
                (
                    "unowned",
                    [
                        "accent",
                        "selection",
                        "team-1",
                        "team-2",
                        "status-ok",
                        "status-warn",
                        "status-error",
                        "status-info",
                        "nav-route",
                    ],
                    "team-neutral",
                ),
                (
                    "view-unowned",
                    [
                        "accent",
                        "view-selection",
                        "view-team-1",
                        "view-team-2",
                        "view-status-ok",
                        "view-status-warn",
                        "view-status-error",
                        "view-status-info",
                        "nav-route",
                    ],
                    "view-team-neutral",
                ),
            ] {
                for o in others {
                    let d = hue_distance(c(own), c(o));
                    assert!(d >= 30.0, "{theme:?}: {own} is {d:.0} degrees from {o}");
                }
                let (_, s_own) = hue_saturation(c(own));
                let (_, s_neu) = hue_saturation(c(neutral));
                assert!(
                    s_own >= 0.5 && s_neu <= 0.35,
                    "{theme:?}: {own} {s_own:.2} / {neutral} {s_neu:.2}"
                );
            }
        }
    }

    /// The Diff panel's colours (round EX1): never orange (the selection), each kind its own hue, in both themes.
    #[test]
    fn diff_colours_are_distinct() {
        let t = tokens();
        for theme in ThemeId::ALL {
            let p = Palette::new(t, theme);
            let d = p.diff_entries();
            for (k, (n, c)) in d.iter().enumerate() {
                let to_sel = hue_distance(*c, p.view_selection);
                assert!(
                    to_sel >= 45.0,
                    "{theme:?}: {n} is {to_sel:.0} degrees from the selection"
                );
                for (m, o) in d.iter().skip(k + 1) {
                    let h = hue_distance(*c, *o);
                    assert!(h >= 45.0, "{theme:?}: {n} and {m} are {h:.0} degrees apart");
                }
            }
        }
    }

    /// The view variants read on the terrain as the view lights it: the overlay colours at their darkest shade
    /// (0.88, `view.wgsl`'s overlay shading) against the terrain at its brightest (0.93, flat ground) and on
    /// slopes, at least 3.5:1; and the selection outline on the sky too. (`tests/render.rs` measures the same in
    /// rendered frames.)
    #[test]
    fn view_colours_read_on_the_terrain() {
        let t = tokens();
        for theme in ThemeId::ALL {
            let p = Palette::new(t, theme);
            for (name, colour) in [
                ("view-team-1", p.view_team_1),
                ("view-team-2", p.view_team_2),
                ("view-team-neutral", p.view_team_neutral),
                ("view-unowned", p.view_unowned),
                ("view-selection", p.view_selection),
                ("view-status-ok", p.view_status_ok),
                ("view-status-warn", p.view_status_warn),
                ("view-status-error", p.view_status_error),
                ("view-status-info", p.view_status_info),
                ("view-link", p.view_link),
                ("view-fx-effect", p.view_fx_effect),
                ("view-fx-light", p.view_fx_light),
                ("view-fx-sound", p.view_fx_sound),
            ]
            .into_iter()
            .chain(p.nav_entries())
            .chain(p.diff_entries())
            {
                for k_mark in [1.0, 0.88] {
                    for k_ground in [0.93, 0.75, 0.6] {
                        let r = contrast(shaded(colour, k_mark), shaded(p.scene_terrain, k_ground));
                        assert!(
                            r >= 3.5,
                            "{theme:?}: {name} on the terrain {r:.2} ({k_mark} / {k_ground})"
                        );
                    }
                }
                let w = contrast(colour, shaded(p.scene_water, 0.93));
                assert!(w >= 3.5, "{theme:?}: {name} on the water {w:.2}");
            }
            let s = contrast(p.view_selection, p.scene_sky);
            assert!(s >= 1.8, "{theme:?}: the selection on the sky {s:.2}");
            // the label plate is the panel ground it says, at 85 %
            let plate = if theme == ThemeId::Dark {
                p.surface_1
            } else {
                p.surface_2
            };
            assert_eq!(
                p.scene_label,
                Color32::from_rgba_unmultiplied(plate.r(), plate.g(), plate.b(), 0xd9)
            );
        }
    }

    #[test]
    fn colour_maths() {
        assert!((contrast(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 1e-9);
        assert!((contrast(Color32::WHITE, Color32::WHITE) - 1.0).abs() < 1e-9);
        assert!((hue_saturation(Color32::from_rgb(255, 0, 0)).0).abs() < 1e-9);
        assert!((hue_saturation(Color32::from_rgb(0, 0, 255)).0 - 240.0).abs() < 1e-9);
        assert!((hue_distance(Color32::from_rgb(255, 0, 10), Color32::from_rgb(255, 10, 0)) - 4.7).abs() < 0.1);
        assert_eq!(
            shaded(Color32::from_rgb(200, 100, 0), 0.5),
            Color32::from_rgb(100, 50, 0)
        );
    }

    #[test]
    fn bad_tokens_are_refused() {
        assert!(hex("#12345").is_err());
        assert!(hex("123456").is_err());
        assert_eq!(
            hex("#ff000080").unwrap(),
            Color32::from_rgba_unmultiplied(255, 0, 0, 128)
        );
        assert!(shadow("0 6px").is_err());
        assert!(Tokens::parse("{}").is_err());
        assert!(Tokens::parse("not json").is_err());
    }
}
