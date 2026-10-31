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
mesh is read is in the docstring of `meshview.py` and the console converter's FORMAT.md. Needs `numpy` and `Pillow`.

Developed for Project OPFOR BFBC2. Battlefield: Bad Company 2 is © Electronic Arts / DICE.
