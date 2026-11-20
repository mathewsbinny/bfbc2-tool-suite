# Mesh viewer (meshview)

Look at Battlefield: Bad Company 2's models from your game copy: a mesh's parts with their colour textures, as a page
you open in a browser (orbit, zoom, a switch per part, wireframe) and, with `--png`, as a picture.

```
python meshview.py <mesh>... [--png] [--view side|top|front] [--size W] [--lod N] [--out <folder>]
python meshview.py find <text>
```

`<mesh>` is a mesh asset name, e.g. `Objects/Weapons/Handheld/Beach/M1Garand/M1Garand_Mesh`; several are drawn one
above the other. Output goes to `<OPFOR>\work\tools_out\meshview` by default. The page loads three.js from a CDN; the
models and textures are embedded in it, so it is game content: keep it in your work folder, do not share it.

The first run indexes the game copy's bundles (levels, async weapon / character bundles, the rest) and every level's
Dx9 shader database, and caches that in `work\tools_out\meshview_index.pkl` (rebuilt when a bundle changes). How a
mesh is read is in the docstring of `meshview.py` and the console converter's FORMAT.md. Needs Python 3.12+, `numpy`,
`Pillow` and the suite's Rust core (the bundles are read through it; since round F4 its part reader runs there too).

`bin\bfbc2.exe meshview ...` (the suite's `bin\`, round O3) takes the same arguments: it answers `find` and the help
from a current index in Rust and hands the drawing (the page and the picture) to the Python (the suite's README,
"bfbc2.exe"). The map editor's asset browser is to take over the viewing (round GR3, `ROADMAP.md`); the picture
command line stays.

Developed for Project OPFOR BFBC2. Battlefield: Bad Company 2 is © Electronic Arts / DICE.
