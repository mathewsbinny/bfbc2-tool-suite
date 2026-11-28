# Model tools

Build and edit Battlefield: Bad Company 2 skinned meshes (weapons) on the PC.

- `skinmesh.py` - a MeshData LOD: replace a part's triangles with new geometry, add geometry to a part (the
  depth-only one), write it again. Vertices are written in each part's own layout from the level shader databases
  (position, bone indices and weights, normal / tangent / binormal, texture coordinates), one bone per vertex, as
  the game's weapons have them; parts not changed keep their bytes. `lod_size()` is the size a mesh set records.
- `shapes.py` - simple solids with texture coordinates placed on a texture atlas: surfaces of revolution (`lathe`),
  extruded outlines (`extrude`), moving / turning / joining (`Geo`), winding to match normals (`fix_winding`), and
  tangents the way the game's weapon meshes store them.
- `texwrite.py` - PC textures (`ITexture` .res) from pictures: DXT1 colour maps (format 0), DXT1 normal maps
  (format 18) and DXT5 pictures with alpha (format 2; one level, as DICE's award images), laid out as DICE's
  textures are; `wrapped()` gives the on-demand bundles' form (`RES\x02` + type name, sizes, payload: every entry of
  `async\ondemand_awards-00.fbrb`); `alpha5()` decodes a DXT5 level's alpha. **DXT1 with 1-bit alpha**
  (2026-11-09): `texture(rgba, 'color')` writes each block holding a transparent texel (alpha under 128) in
  three-colour mode (colour 0 <= colour 1, the ends' midpoint, index 3 transparent; the end colours from the opaque
  texels only), and the others as before, so an opaque picture comes out byte-identical. `alpha1()` reads a DXT1
  level's punch-through alpha, which the console converter's `texture.decode` leaves out. OPFOR's grime pass
  decoded and re-encoded DXT1 without it, and every transparent texel came out solid black. For textures composed level by level
  (the emitter atlases, `bfbc2-bf1943-import\emitteratlas.py`): `resource()` (the header over given levels),
  `l8_down()` / `l8_bytes()` (an 8-bit level as DICE's emitter atlases have it: the rounded 2 x 2 mean, rows padded to
  4 bytes), `atlas_levels()` (their level count).
- `decimate.py` - fewer triangles (third-person LODs): quadric edge collapses onto existing vertices, so texture
  coordinates, normals and bones stay exact; texture seams stay closed, bones are never merged.
- Skinned mesh sets (`SkinnedMeshSet`) are read and written by the console converter's `meshset.py`: the rigid
  layout without the flag byte, then per LOD the skeleton bones its parts use and their name hashes.

To look at the result use the mesh viewer (`bfbc2-mesh-viewer`). Project OPFOR's `src\tools\ww2\attachments.py`
uses these to model the WW2 rifles' bayonets and rifle grenades onto DICE's surviving textures.

These are Python libraries (no command line; `bfbc2.exe` has no subcommand for them). With the suite's Rust core
built, `texwrite.py`'s encoders and `skinmesh.py`'s vertex work run in Rust (round F2a, `rust/bc2-convert`), the same
bytes, the Python kept as the reference and the fallback (`use_native(False)`, `BFBC2_PURE_PYTHON=1`); since round OR14
`texwrite`'s principal axes are the core's too (`bc2_convert::eigh3`: NumPy's `eigh` path written out, the same axes
to the bit on any processor; the Python reference still calls `numpy.linalg.eigh`). Python 3.12+ and `numpy`. `decimate.py`'s collapses and `shapes.py`'s products, tangents and
windings need the core (round PR1b-2-rest2, `bc2_convert::decimate` / `shapes`): NumPy's OpenBLAS products are written
out as the kernels of the machine that recorded Project OPFOR's WW2 weapon parts compute them, so the results are
those on any processor (spec XWW2-MOD-1); the NumPy originals stay as `decimate_reference` and `*_reference`.

Test: `tests\test_models.py` (game tier: every Garand / Type 5 LOD and mesh set written back byte-identical, a part
replaced and read back); `tests\test_texwrite.py` (DICE's level layout, round trips through the console converter's
decoder, unit-length normal mips, DXT5 colour and alpha, a cut-out picture keeping every transparent texel in DXT1;
with the game, two of DICE's weapon textures and an award image encoded again); `tests\test_decimate.py`
(a seamed sphere stays closed and round, two bones stay apart; the core against the NumPy originals and recorded
digests: spheres, shapes-built rifles, jittered spheres, moved / tangents / windings); `tests\test_model_f2a_native.py`
(round F2a: `texwrite` and `skinmesh` in the core against their Python reference, both ways); and
`tests\test_pictures_native.py` (round OR4: Project OPFOR's WW2 pictures in the core - resizing, the loadout
renderer, the picture checks, the weapon icons, the mesh viewer's preview page).

Developed for Project OPFOR BFBC2. Battlefield: Bad Company 2 is © Electronic Arts / DICE.
