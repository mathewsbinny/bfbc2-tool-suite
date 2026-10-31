# Model tools

Build and edit Battlefield: Bad Company 2 skinned meshes (weapons) on the PC.

- `skinmesh.py` - a MeshData LOD: replace a part's triangles with new geometry, add geometry to a part (the
  depth-only one), write it again. Vertices are written in each part's own layout from the level shader databases
  (position, bone indices and weights, normal / tangent / binormal, texture coordinates), one bone per vertex, as
  the game's weapons have them; parts not changed keep their bytes. `lod_size()` is the size a mesh set records.
- `shapes.py` - simple solids with texture coordinates placed on a texture atlas: surfaces of revolution (`lathe`),
  extruded outlines (`extrude`), moving / turning / joining (`Geo`), winding to match normals (`fix_winding`), and
  tangents the way the game's weapon meshes store them.
- Skinned mesh sets (`SkinnedMeshSet`) are read and written by the console converter's `meshset.py`: the rigid
  layout without the flag byte, then per LOD the skeleton bones its parts use and their name hashes.

To look at the result use the mesh viewer (`bfbc2-mesh-viewer`). Project OPFOR's `src\tools\ww2\attachments.py`
uses these to model the WW2 rifles' bayonets and rifle grenades onto DICE's surviving textures.

Test: `tests\test_models.py` (game tier: every Garand / Type 5 LOD and mesh set written back byte-identical, a part
replaced and read back).

Developed for Project OPFOR BFBC2. Battlefield: Bad Company 2 is © Electronic Arts / DICE.
