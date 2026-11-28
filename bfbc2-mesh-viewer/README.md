# Mesh viewer (meshview)

A picture of any of Battlefield: Bad Company 2's models from your game copy: a mesh's parts with their colour
textures, drawn from the side, the top or the front; and the index of the game copy's bundles that other tools read it
through.

```
python meshview.py <mesh>... [--view side|top|front] [--size W] [--lod N] [--out <folder>]
python meshview.py find <text>
```

`<mesh>` is a mesh asset name, e.g. `Objects/Weapons/Handheld/Beach/M1Garand/M1Garand_Mesh`; several are drawn one
above the other. The picture goes to `<OPFOR>\work\tools_out\meshview\<names>_<view>.png` by default (`--png` is still
accepted). It is game content: keep it in your work folder, do not share it.

**To look around a mesh**, open the map editor's **Asset preview** (`bfbc2-editor`; round GR3 retired this tool's
browser page): pick an object in the asset browser's Place Objects, or any mesh by name in its Meshes mode. It reads
the mesh as this tool does (the same index and cache) and draws it with this tool's own drawing, so its Side, Top and
Front views are this command line's pictures pixel for pixel; besides, it orbits, and shows each level of detail, the
parts (on / off, picked in the view), a blueprint's destruction states, every texture of a part's shader by channel,
wireframe, normals, the bounds with their measurements, several meshes stacked, and saves a picture.
`bfbc2-map-editor\EDITOR_ARCHITECTURE.md` 9.3 lists where each feature of the page went.

The first run indexes the game copy's bundles (levels, async weapon / character bundles, the rest) and every level's
Dx9 shader database, and caches that in `work\tools_out\meshview_index.bin` (rebuilt when a bundle changes; the map
editor and `bfbc2.exe` read and write the same cache). How a mesh is read is in the docstring of `meshview.py` and the
console converter's FORMAT.md. Needs Python 3.12+, `numpy`, `Pillow` and the suite's Rust core.

Since round PR1b-2-rest the index, `load_mesh`, `mesh_surfaces`, `stack_models` and `render` are the Rust port's
(`rust\bc2-convert\src\meshview`, through `bc2_native`: `MeshViewIndex`, `meshview_stack`, `meshview_render`); the
module keeps the NumPy arrays and dicts its callers take, the stock bundles' list (`_bundles`) and `read()`'s partition
cache. The index's old pickle (`meshview_index.pkl`) is no longer read; the caches' budget clears a
leftover one.

`bin\bfbc2.exe meshview ...` (the suite's `bin\`, round O3) takes the same arguments: it answers `find` and the help
from the index's cache in Rust, and hands the drawing, and building an index that is missing, to the Python (the
suite's README, "bfbc2.exe").

`meshview.page()` (the page for a browser) is retired (round OR4): Project OPFOR BFBC2's WW2 tools (`attachments.py`,
`scopes.py`) write their preview pages with the Rust's `bc2_native.meshview_page` (`bc2_convert::meshview::page`: the
same page, its textures PNG files of the same pixels; spec XWW2-PIC-4).

`tests\record_preview_parity.py` recorded this tool's answers on real meshes (digests only) for the editor's parity
tests (`rust\bc2-editor\tests\asset_preview_parity.rs`, named `gr3.rs` until round PR1a) while the tool was Python; the recording is the retired Python's (run again, it
records the Rust port).

Developed for Project OPFOR BFBC2. Battlefield: Bad Company 2 is © Electronic Arts / DICE.
