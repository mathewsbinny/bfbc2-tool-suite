# bfbc2-navigation-studio

View a level's AI navigation data (Kynapse `pathdata.hpd`) in 3D: NavKit's build and DICE's original
side by side, over the level's terrain, water and objects. Milestone M3 (viewing); editing, the path test
and side files are M4 (`../DESIGN.md` sections 4.2 and 5).

```
python studio.py <level>                       e.g. python studio.py sp_01
python studio.py <level> --candidate <file.hpd> --reference <file.hpd> --color island|sector|pitch|difference
```

**Files.** The reference is DICE's path data in Project OPFOR's game copy; the candidate is NavKit's newest
build of the level (`work\navkit\<level>\pathdata.hpd`, `work\navkit\regress\<level>.hpd` or
`work\navbuild\<level>_W.hpd`). Either can be given, and either may be missing. The level is prepared on
first use (offline); each file's walkable cells are cached in `work\navstudio\cache` (the first opening of a
large level takes about a minute, later ones seconds).

**Layers, per file:** walkable ground (its 0.8 m cells, on the exact terrain height where they lie on
terrain), outlines, portals, border portals and the route graph; plus terrain, water, objects and soldier
spawns. NavKit's ground is shown at start, the rest can be switched on.

**Colour the ground by**

| Mode | Shows |
|---|---|
| island | connected parts of the route graph: the main network green, every other island its own colour |
| sector | the 100 m sectors |
| pitch | detailed zones (DICE's 0.2 m areas) in orange |
| difference | NavKit's ground that DICE also has (grey-green) or does not (orange); DICE's ground NavKit lacks is the red layer |

**Path test (P, milestone M4 step 1).** Press P, click a start and an end on the ground: the cheapest route
over each file's route graph (its own directed edges and stored costs) is drawn - NavKit's yellow, DICE's
cyan - with its cost, length, the detour against the straight line, and NavKit's cost against DICE's. Without
a route the panel says why: which island each point is on and where they come closest (drawn in red, with a
button that flies there), or that no route node is within 15 m of a point. It plans over the graph only; the
game also smooths routes through the areas. (`core/bfbc2core/navpath.py`; `--path "x,y,z;x,y,z"` for tests.)

**Edit mode (E, milestone M4 step 2).** Draw edits on the level; they are kept in a side file,
`work\navkit\<level>\<level>.navtune.json` (format: `../DESIGN.md` section 5.1; `core/bfbc2core/navtune.py`).
Tools: walkable or unwalkable region (click an outline and press Enter, or click two corners of a box; the
height range runs from 2 m below to 4 m above the clicked points, adjustable), seed, unseed, link two areas,
add or remove a route edge (two points), traversal (ladder, jump, vault or drop, one- or two-way: the
bottom, then the top), and note. Every edit records who made it (this PC's name unless you type another)
and when. The panel lists the edits (fly to one, delete one), with undo (Ctrl+Z), redo (Ctrl+Y), save and
reload; Esc cancels a drawing. Edits are drawn over the scene: walkable green, unwalkable red, seeds cyan,
unseeds magenta, links orange, route edges yellow (removals red), traversals white.

Edits are recorded and shown; building them into the path data comes with `navkit apply` (steps 3 to 6). On
DICE's own data only additive edits are offered (links, route edges, traversals, notes): region edits need
a NavKit build to rebuild. `--navtune <file>` opens a side file from elsewhere, `--edit` opens the panel.

**Panels**

- *Navigation:* per file the areas (detailed ones), cells, route nodes and edges, islands; DICE ground
  reproduced and NavKit's extra ground (a cell estimate - `navparity.py` gives the exact figures); colour
  mode; layers.
- *Islands and spawns:* the islands of either file, largest first, with their size and distance from the
  main network, each with a button that flies there; the soldier spawns that are not on the main network
  (or have no route node within 15 m).
- *Selection:* click ground to see its area (sector, tile, layer, pitch, cells, boundary pieces, island);
  click a spawn to see its island in each file. Hovering shows the area under the mouse.

The camera works as in the Map Inspector (left drag rotates, right drag pans, wheel zooms, double-click flies
to a point, F for fly mode). For tests: `--screenshot out.png [--frames N] [--color ...] [--island N]
[--hide <layer>,...]`.

The data comes from `core/bfbc2core/navdata.py`, which reads the files with NavKit's byte-exact reader.
