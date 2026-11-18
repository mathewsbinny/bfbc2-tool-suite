The shared look of every BFBC2 Tool Suite window: the map editor and Map Inspector, Navigation Studio, NavKit's
window, DbxEditGUI, the suite launcher and every GUI to come. It follows the conventions modders already know from
game-engine editors such as Unity and Unreal Engine: neutral grey docked panels around a 3D viewport, a blue
interface accent, orange for what is selected in the viewport, compact rows, and a dark and a light theme. It
borrows those conventions, not either product's look: the colours, sizes and parts here are this suite's own.

## Principles

- **The map is the hero.** Chrome stays low-contrast and neutral; colour is spent on the 3D view's overlays and on
  state. A panel never competes with the viewport.
- **Colour means something.** `accent` (blue) marks the active control: the primary action, the active tool and
  tab, a checked toggle. `selection` (orange) marks what is selected in the 3D view. Team, status, navigation and
  axis colours are data. Nothing is coloured for decoration.
- **State has a shape and a word.** Severity, team and build state always carry an icon, a letter or a word next to
  their colour, so nothing depends on telling red from green.
- **Dense, not cramped.** 22px rows, 13px text, mono figures for numbers. Room comes from alignment and hairlines,
  not from padding.
- **Keyboard first.** Every action has a visible focus state (`focus`) and a shortcut shown in its tooltip and menu.

## Voice

Write as the tool's user would speak: name what they see, not how it is built ("Spawn point", not
`AlternativeSpawnPointEntityData`; the type name sits in the properties panel for those who want it). Sentence case
everywhere except `label` captions, which are set in capitals. Buttons say exactly what happens: **Build**,
**Install**, **Play-test**, **Snap to ground**. Errors say what is wrong, why it matters and how to fix it, in that
order:

> Flag C has no spawn point. Players who hold it cannot deploy there. Add a spawn point near the flag, or turn on
> "Spawn at flag" in its properties.

No emoji, no exclamation marks, no apologies. Numbers carry their unit: `12 m`, `3 m 12 s`, `1,300 m`.

## Colour

- **Grounds.** Neutral greys with no tint. `ground` behind and between docked panels; `surface-1` for panel bodies;
  `surface-2` for inputs, raised toolbars and popovers; `surface-3` for panel headers, the active tab and pressed
  controls. Separate surfaces with `line` hairlines, not shadows. In Light the panels are mid grey, not white, so a
  long session is not glaring and the viewport still stands out.
- **Text.** `ink` for content and values, `ink-muted` for labels, column headers and timestamps, `ink-faint` only
  for disabled controls and placeholders.
- **Accent.** `accent` as a fill: the primary button (text in `on-accent`), the active tool, the active tab's
  underline, a selected row's marker. `accent-text` where the accent is text: links, references, the content
  profile badge. `accent-subtle` as a selected row's ground with `ink` on it. At most one primary button per panel.
- **Selection.** `selection` outlines what is selected in the 3D view and colours the gizmo handle being dragged.
  It is never used in panels; a selected row uses `accent-subtle`.
- **Teams.** `team-1` (a light sky blue, kept apart from the interface blue) and `team-2` for everything a team
  owns: spawns, bases, held flags, AI orders. Each is always shown with its letter or label (US, RU, JP; A, B, C for
  flags). `team-neutral` for neutral flags.
- **Status.** `status-ok`, `status-warn`, `status-error`, `status-info`, each with its icon (tick, triangle, octagon,
  circle) and, in lists, the word.
- **Navigation overlays** (AI workspace). `nav-walkable` at 45% over the terrain, `nav-blocked` for unwalkable
  regions and coverage gaps, `nav-route` for routes (solid) and hand-made links (dashed).
- **Gizmo and axes.** `axis-x`, `axis-y` (up), `axis-z`, as every 3D tool draws them; the handle being dragged turns
  `selection`.
- **Focus.** A 2px solid `focus` ring offset by 1px on every focused control.

Both themes are designed: Dark is the default, Light follows the system setting or the user's choice. Every text
pair above meets 4.5:1 in both themes, and control borders (`line-strong`), the accent, the selection, the focus
ring and the status, team and axis colours meet 3:1 on `surface-1` and `surface-2`.

## Type

One plain interface family, as editors use: IBM Plex Sans for everything, IBM Plex Mono for data. Both are open
licence (SIL OFL) and ship with the suite as font files.

- `display` once per screen (start screen, dialog title) and `section` for workspace and wizard step titles: the
  same family at a larger size and weight.
- `title` for headings inside panels, `body` for everything else, `body-strong` for emphasis and button labels.
- `label` for panel header captions and column headers, in capitals.
- `small` for help text under fields, the status bar and timestamps.
- `value` for numbers, coordinates, GUIDs, paths and file names, with tabular figures so columns line up; `log`
  for the build log.

## Spacing, size and shape

A 4px base: `space-1` inside rows, `space-2` between toolbar controls, `space-3` panel padding, `space-4` between
sections. Rows are `row` (22px), controls `control` (26px), toolbars `toolbar` (38px), panel headers
`panel-header` (28px), icons `icon` (16px). Corners are small and precise: `radius-sm` for chips and selection,
`radius-md` for controls and tabs, `radius-lg` for dialogs and floating panels only. `shadow-popover` only on
things that float: menus, popovers, floating panels, dialogs.

## Layout

Docked panels around the 3D view (the map editor's default: outliner and assets left, properties right, problems,
build log and history below, the top bar above). Panels can be dragged, docked, tabbed and floated; **Reset layout**
always returns to the default. A panel is a `panel-header` strip (`surface-3`, a `label` caption, its tools on the
right) over a `surface-1` body. Workspaces (Objects, Gameplay, AI) switch from the top bar's segmented control.

## Iconography

The suite draws its own icon set (the user, 2026-11-16) rather than taking a stock one, so the icons can show this
game's own things: flags, spawn points, M-COMs, vehicles by kind, AI orders, navmesh regions, routes and links,
alongside the usual editor actions. The set's rules:

- Drawn on a 16px grid (`icon`) with a 1.5px stroke, square ends and joins, 1px clear margin; no fills except
  small solid marks (a flag's cloth, a status shape).
- Single colour, taking the text colour of where it sits; team, status and axis colours only where the icon *is*
  that thing (a team-owned flag, an error).
- Made as SVG sources in the suite, exported to each GUI stack's format (a font or texture atlas) by the same
  generation step as the themes.
- Status shapes are fixed: tick (ok), triangle (warning), octagon (error), circle (information).

Until the set exists, previews use plain glyphs and words.

## Motion

State changes fade or slide in 120ms; nothing bounces. Progress shows as a bar or the build graph, never a spinner
alone. The 3D view's overlays do not animate except the active gizmo handle. Respect the system's reduced-motion
setting.

## How the suite uses this

`tokens.json` is the one source. Each GUI stack gets its theme generated from it rather than hand-copied: the map
editor's Rust interface (egui), the Python 3D tools (imgui-bundle) and the Tk launcher. A new GUI takes its colours,
type and sizes from these tokens by name, never as literal values. The component previews here show how the parts
should look; each stack builds them from its own widgets.
