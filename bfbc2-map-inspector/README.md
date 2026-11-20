# bfbc2-map-inspector

The **Map Inspector** (`inspector.py`, a Python 3D window) was retired on 2026-11-20 (round GR1). Everything it did is
in the suite's **map editor** (`rust/bc2-editor`, `bin\bfbc2-editor.exe`; README.md, "The map editor"), which grew out
of it. This folder keeps the editor's plan, [EDITOR_ARCHITECTURE.md](EDITOR_ARCHITECTURE.md).

**Opening a level:** in the suite window (`BFBC2 Tool Suite.pyw`), Levels tab: choose a level, then **Open in Map
Editor** (or double-click the level). Or from a command line: `bin\bfbc2-editor.exe mp_005`. Without a level the
editor opens on its start screen. A level is read only; a map project edits a copy.

Where the Map Inspector's features went:

| Map Inspector | Map editor |
|---|---|
| The 3D view: terrain, water, collision, markers per layer, outlines; orbit, pan, zoom, fly (F) | The 3D view (orbit, pan, zoom; fly: hold the right button; F focuses); top, front and side views; the Game-accurate view |
| Layers with counts, shown and hidden (area points, effects and other hidden at start) | The Outliner's groups with counts and Hide / Show (the same groups hidden at start) |
| Search by GUID, type, id, name or blueprint | The Outliner's search; Ctrl+P |
| Selection panel: type, GUID, id, name, category, blueprint, source, team, player type, position | Properties |
| The full property tree, references as buttons that follow them, *back* | Properties, **All fields**: every field nested, with its type and value; references go to their object |
| Referenced by; yellow and cyan link lines in the view | The **Links** panel (references, referenced by, and the level's event connections, both ways; a row goes there, Back returns) and the `view-link` lines in the views (K) |
| Measure (M): segment, horizontal, height, slope | The ruler (Objects workspace) and the status bar's surface read-out |
| Hover tooltip | Labels at spawns, flags and objectives; the status bar |
| `--screenshot` for tests | `bfbc2-editor --screenshot out.png [--select <guid>]` |

The editor's property tree and links were compared with what the Map Inspector showed before it was retired:
`rust/bc2-editor/tests/inspect_parity.rs` (1,074 objects of SP_01, MP_005, MP_009GR and ONS_MP_004, their recorded
digests in `tests/data/inspector_props_*.json.gz`), as its outliner was in round G1 (`inspector_parity.rs`).
