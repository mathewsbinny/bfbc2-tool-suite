The shared look of every BFBC2 Tool Suite window: the map editor (the Map Inspector's and the Navigation Studio's successor), NavKit's
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
- **Selection.** `selection` marks what is selected where it sits on a panel ground (a selected label's frame in
  the view); in the 3D view itself the outline and the gizmo handle being dragged take its view variant,
  `view-selection`. A selected row uses `accent-subtle`.
- **Teams.** `team-1` (a light sky blue, kept apart from the interface blue) and `team-2` for everything a team
  owns: spawns, bases, held flags, AI orders. Each is always shown with its label: the side's faction as the level's
  own data says it (`LevelDescription.FactionTeam1` / `FactionTeam2`, read through the content profile - the game's
  enum has US, RUS and Neutral, and DICE's Vietnam levels and the Pacific War levels use RUS for their second side:
  US and RU on BFBC2 levels, US and NVA on Vietnam's, US and JP on the Pacific War's), T1 and T2 only where the
  level names no faction (the campaign, squad deathmatch: the properties panel says so); A, B, C for flags.
  `team-neutral` (a near-grey) for what the data marks team-neutral: flags no team holds yet, neutral vehicle spawns
  (label NEU).
- **Unowned gameplay (the third neutral).** `unowned`, a violet, for gameplay objects whose data has no team at
  all: objectives (M-COMs), combat areas, AI orders. Chosen by hue distance: violet (250 degrees) sits in the widest
  gap of the palette's hues, between the blues (`accent`, `team-1`, `status-info`, `axis-z`, `focus`: 199 - 215
  degrees) and `nav-route`'s orchid (298 degrees). It is 35 - 47 degrees from each of those and 105 - 157 degrees
  from the selection orange, the reds, the greens and the yellow, in both themes; `team-neutral` is told from it by
  saturation (0.1 - 0.25 against 0.5 and more). Dark `#a897f2` (6.6:1 on `surface-1`, 5.7:1 on `surface-2`), Light
  `#4a35ad` (5.2:1 and 7.0:1).
- **Status.** `status-ok`, `status-warn`, `status-error`, `status-info`, each with its icon (tick, triangle, octagon,
  circle) and, in lists, the word.
- **Navigation overlays** (AI workspace). `nav-walkable` at 45% over the terrain, `nav-blocked` for unwalkable
  regions and coverage gaps, `nav-route` for routes (solid) and hand-made links (dashed).
- **Gizmo and axes.** `axis-x`, `axis-y` (up), `axis-z`, as every 3D tool draws them; the handle being dragged turns
  `selection`.
- **Focus.** A 2px solid `focus` ring offset by 1px on every focused control.

Both themes are designed: Dark is the default, Light follows the system setting or the user's choice. Every text
pair above meets 4.5:1 in both themes, and control borders (`line-strong`), the accent, the selection, the focus
ring and the status, team, unowned and axis colours meet 3:1 on `surface-1` and `surface-2` (the editor's tests
check each pair).

### The 3D view's colours

The colours above are made for panel grounds. The 3D view draws on its terrain, so it has tokens of its own
(round G2, 2026-11-18):

- `scene-*`: the scene itself, muted so gameplay stands out - `scene-sky` (behind the map), `scene-terrain`,
  `scene-water`, the object greys by kind (`scene-object`, `scene-object-destructible`, `scene-object-dynamic`,
  `scene-object-group`, `scene-ladder`, `scene-effect`, `scene-area-point`, `scene-other`) and `scene-label`, the
  plate behind a label or note drawn over the map (`surface-1` in Dark, `surface-2` in Light, at 85 %; `ink` and the
  panel team colours on it). The terrain is deliberately dark (luminance 0.06 - 0.07) so that every overlay reads on
  it; the scene is nearly the same in both themes, the sky follows the chrome.
- `view-*`: what is drawn on the terrain - `view-team-1`, `view-team-2`, `view-team-neutral`, `view-unowned`,
  `view-selection` (the outline, through walls), `view-status-ok`, `-warn`, `-error`, `-info` (problem markers in
  the view, with their shapes) and `view-link` (round GR1: the selection's links, a teal line with an arrow -
  references solid, event connections dashed; a teal 29 degrees from `view-team-1` and from `view-status-ok`, told
  from them by being a line with an arrow, never a marker). Same hues as the panel tokens, lighter (`view-link` has
  no panel twin: links in panels are `accent-text`). The view lights overlays evenly (0.88 - 1.0 of the colour, where
  the terrain and objects take 0.40 - 1.0), so they read from every side.

Each `view-*` colour meets **3.5:1 on the lit terrain** - at its darkest against the terrain at its brightest (flat
ground) and on slopes - and on the water, in both themes; the editor's tests compute this and also measure it on
rendered frames (each overlay marker's own pixels against the terrain's around it: 4.0:1 median to median, 3.9:1
between the marker's darkest tenth and the terrain's brightest tenth).

| token | Dark | Light | on flat lit terrain, Dark / Light | worst case |
|---|---|---|---|---|
| `view-team-1` | `#2bb8ff` | `#3bbeff` | 4.6 / 4.6 | 3.6 |
| `view-team-2` | `#ff8f75` | `#ff9780` | 4.6 / 4.6 | 3.6 |
| `view-team-neutral` | `#d4ceba` | `#dad4c2` | 6.6 / 6.6 | 5.0 |
| `view-unowned` | `#b4a1ff` | `#baa8ff` | 4.7 / 4.7 | 3.6 |
| `view-selection` | `#ff9326` | `#ff9c38` | 4.6 / 4.7 | 3.6 |
| `view-status-ok` | `#2ac864` | `#2cce67` | 4.7 / 4.7 | 3.6 |
| `view-status-warn` | `#e0c400` | `#e6cb00` | 5.9 / 6.0 | 4.6 |
| `view-status-error` | `#ff8c9c` | `#ff94a2` | 4.7 / 4.6 | 3.6 |
| `view-status-info` | `#85adff` | `#8cb3ff` | 4.6 / 4.6 | 3.6 |
| `view-link` | `#40d8c0` | `#46dcc6` | 5.8 / 5.7 | 4.4 |
| `view-fx-effect` | `#b6e36a` | `#bce970` | 7.0 / 7.0 | 5.3 |
| `view-fx-light` | `#fff08a` | `#fff690` | 8.9 / 8.7 | 6.6 |
| `view-fx-sound` | `#ffa3e0` | `#ffa9e6` | 5.7 / 5.6 | 4.3 |

The `view-fx-*` tokens (round VW1) are the overlays of what the level's data holds besides gameplay: effect
emitters and their spawn volumes (a lime, 82 degrees), dynamic lights and their radius (a pale yellow, 52 degrees; a
light's own colour is its label's swatch) and sound emitters and areas (a pink, 320 degrees). Told from their
neighbours by shape and label as the others are: the lime from `view-nav-edit-link` (another workspace's), the yellow
from `view-status-warn` (a warning is always its triangle), the pink from `view-nav-small`.

The terrain is `#3f4837` (Dark) and `#434c3b` (Light), the sky `#1c1f23` and `#747d87`. Known closenesses, carried
by shape and word as everywhere: `view-team-2` and `view-status-error` (both light reds, 19 degrees apart), and
`view-team-1` and `view-status-info` (20 degrees).

**The AI workspace's navigation** (round GR2, the Navigation Studio's colours made tokens; each also meets 3.5:1 on
the lit terrain and the water, worst case 3.6:1, the editor's tests check every one; Light is Dark lifted by 6 per
channel):

- the ground's colourings: `view-nav-main` (`#6fd17a`, the main route network), `view-nav-ground` (`#a9c2a6`,
  NavKit's ground DICE also has; a lone file's ground), `view-nav-ground-dice` (`#a7afca`), `view-nav-extra`
  (`#f5d76e`, NavKit's extra ground and the detailed 0.2 m zones), `view-nav-missing` (`#ff9a94`, DICE's ground
  NavKit lacks), `view-nav-ladder` (`#c4a8ff`), `view-nav-normal` (`#7fb6e0`, the 0.8 m pitch), `view-nav-dimmed`
  (`#b0b8b0`, islands of 25 nodes or more) and `view-nav-small` (`#ff90e8`, smaller islands; lonely nodes). Every
  other island and sector takes a hue of its own in golden-ratio steps (saturation 0.5, value 0.95), as the Studio's did;
- the layers' lines: `view-nav-outline` (`#e8ece8`), `view-nav-portal` (`#6fe3ff`), `view-nav-border` (`#ffe066`,
  also sliver areas), `view-nav-graph` (`#f3a6ff`);
- the path test's routes: `view-nav-route-navkit` (`#ffe14d`) and `view-nav-route-dice` (`#5ee7ff`);
- the edits: `view-nav-edit-add` (`#66f08a`: walkable regions, seeds), `view-nav-edit-remove` (`#ff978c`:
  unwalkable regions, unseeds, removed route edges), `view-nav-edit-link` (`#c8f56b`: links, added route edges),
  `view-nav-edit-mark` (`#f2f2ff`: traversals, notes, the drawing in progress);
- a reference level's AI layer: `view-ref-ai` (`#ff9e9e`), `view-ref-human` (`#7ab0ff`), `view-ref-order` (`#ffe066`),
  `view-ref-patrol` (`#e0b0ff`), `view-ref-objective` (`#d9d9d9`), `view-ref-outline` (`#a8f0a8`).

**The differences** (round EX1, the Diff panel and the export's preview; each also 3.5:1 on the lit terrain and the
water, at least 45 degrees of hue from the selection and from each other, the editor's tests check every one):
`view-diff-added` (`#5cf0a8`: an object only in B, and on A a box where it stands), `view-diff-removed` (`#ff8fb6`: an
object only in A, and on B a box where it stood), `view-diff-moved` (`#7cc8ff`: its transform differs; the line from its
old place) and `view-diff-changed` (`#e2a8ff`: other fields differ).

None is orange: orange stays the viewport selection's.

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

`tokens.json` is the one source. **Since round G2 (2026-11-18) the copy in this repository is the authoritative
one:** it gained `unowned` and the `view-*` and `scene-*` tokens here, so the design-system artifact it was first
exported from is behind and needs a sync from this file, not the other way round. Each GUI stack gets its theme
generated from it rather than hand-copied: the map
editor's Rust interface (egui), the Python 3D tools (imgui-bundle) and the Tk launcher. A new GUI takes its colours,
type and sizes from these tokens by name, never as literal values. The component previews here show how the parts
should look; each stack builds them from its own widgets.
