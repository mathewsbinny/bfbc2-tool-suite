# Decals and roads

The three kinds of decal BFBC2 draws: the terrain's **baked strips** (roads, tracks and quad decals laid on the ground
when DICE built the level, stored in the visual terrain: DECAL-STRIP-*), DICE's **editor sources** they were baked
from (the terrain partition's decals, which do not ship their materials: DECAL-SRC-1), and **impact decals** made at
run time from the material grid (DECAL-IMP-1). Written by round SPG2 (2026-11-22) for the map editor's terrain
rounds (E5, E7) and the WW2 port; measured by `spec/probes/terrain.py` over the 30 DICE terrains and 74 level
partitions of the game copy. The strip grammar is TERR-VT-2 ([formats/terrain.md](../formats/terrain.md)), their
drawing RNDR-TERR-4 ([rendering.md](rendering.md)).

## 1. Baked strips

### DECAL-STRIP-1: Roads and terrain decals are strips in the visual terrain

Status: **observed** · Variants: PC

A visual terrain's road blocks (TERR-VT-2; two per terrain block: full detail, then low detail) hold the level's
roads, tyre tracks and quad decals as triangle strips with their own world-space vertices (28 bytes: position,
texture coordinate, four halves), each strip naming its shader entry, an optional second shader (a depth-only pass)
and a mask shader, a first index, a triangle count, bounds and a layer 0 - 7 (the draw order among overlapping
strips). DICE's 30 terrains hold 0 - 188 strips (1,424): by shader name 1,264 roads, 30 tracks, 28 quads, 16 decals,
86 others; layers 0 (596), 1 (662), 2 (144), 3 - 7 (22); 124 with a second shader; all with a mask shader.

[data: spec/probes/terrain.py, 2026-11-22: 30 DICE visual terrains, 1,424 strips; per-terrain 0 - 188; kinds by the shader name's last part; layers and second / mask shader counts as listed]
[code: rust/bc2-render/src/terrain.rs:183-205]

### DECAL-STRIP-2: Strips draw on the strip layout, over the patches, reading the height map

Status: **implemented** · Variants: PC

A strip entry's programs are compiled for BFBC2's strip vertex layout (`bcd37755-...`, BF1943's the same GUID in Xbox
byte order) and read the terrain's named constants (`heightmapScaleAndOffsetXZ`, `heightmapDecalOffsetY`, the dynamic
mask's parameters), which the engine writes by name at draw time; a translated or cloned strip shader must take them
from its constant buffer, not bake the database's placeholders (0: every strip vertex at the decal offset 0). A strip
shader cloned from an entry of another vertex layout (an eyeball's, an undergrowth mask's, a terrain patch's) draws
its strips as sheets across the sky.

[code: bfbc2-shader-tools/pcbuild.py:36-52]
[code: opfor:src/tools/ww2/wake_shaders.py:298-320]
[test: opfor:src/tools/tests/test_ww2_terrain_patches.py::test_strip_and_patch_position_contracts]
[data: OPFOR's Wake Island, 2026-11-12: 60 quad-decal strips cloned from shaders of other layouts drew as sheets across the sky until the strip donors were chosen by layout]

### DECAL-STRIP-3: A road edit rewrites the strips; nothing re-bakes them in game

Status: **inferred** · Variants: PC

The game draws the baked strips as stored: their vertices carry the world height (DECAL-STRIP-1), so a sculpt under a
road leaves it floating or buried until its strip vertices are moved to the new ground (or the road re-built from a
spline the editor keeps). DICE's own splines do not ship (DECAL-SRC-1), so an editor that edits roads must keep its
own source (the strips' vertices as the base, or a spline of its own).

[inferred: DECAL-STRIP-1 (world positions in the strip) and DECAL-SRC-1 (the sources are not in any bundle)]

## 2. DICE's sources

### DECAL-SRC-1: The terrain partition's decals are editor sources

Status: **verified** · Variants: PC

The terrain partition (`Terrains/<T>/<T>.dbx`) holds DICE's decal tree: `TerrainGroupDecal`s (80: folders, `Decals`
lists, the root "Root decal group") over `TerrainFullDecal`s (314: the paint layers, TEDIT-SRC-1), `TerrainSplineDecal`s
(416: roads and tracks, each naming a material `terrains/<T>/materials/<name>` and a spline `levels/<LV>/roads/<guid>`,
`UseInnerSpline` false on all) and `TerrainQuadDecal`s (40). No bundle holds any material or spline they name (770
references); `TerrainData.RootDecal` is null on all 30. The game draws what was baked from them (DECAL-STRIP-1). OPFOR
keeps BF1943's the same way (not remapped, not nulled).

[test: core/tests/test_spec_probes.py::terrain_partitions]
[data: spec/probes/terrain.py, 2026-11-22: 30 terrain partitions: the class counts and references as listed; Enable true on every group, spline and quad decal, 280 of 314 full decals]
[doc: opfor:src/tools/ww2/wake_install.py:390-398]

### DECAL-SRC-2: The level partition's terrain splines

Status: **verified** · Variants: PC

DICE's level partitions hold `TerrainSplineData` objects (1,360 splines, 14,449 `TerrainSplinePointData` points,
1,372 `TerrainSplinePlaneData` planes, 44,132 `TerrainSplinePlanePointData`), with water-surface fields on the planes
(`BaseSpeed`, `DefaultFoamLeft` / `Right`, `DefaultWaveHeightLeft` / `Right`, `WaterSpeed`): rivers and shorelines, not
the road decals' splines (those are in `levels/<LV>/roads`, which does not ship). What the game uses them for is not
traced.

[test: core/tests/test_spec_probes.py::level_partitions] [data: spec/probes/level.py, round SPV1 2026-11-22, DICE's 74 level partitions: TerrainSplineData 1,360, TerrainSplinePointData 14,449, TerrainSplinePlaneData 1,372 (all with the water fields), TerrainSplinePlanePointData 44,132 (all with WaterSpeed)]
[data: spec/probes/terrain.py, 2026-11-22: 74 DICE level partitions: the four spline classes' counts and fields as listed]

## 3. Impact decals

### DECAL-IMP-1: Bullet and blast decals come from the material grid

Status: **observed** · Variants: PC

A bullet or explosion leaves the decal the material grid gives for the pair (projectile material, hit material):
`MaterialRelationDecalData.Decal` (set on 17,338 of DICE's 19,730 decal relations; MAT-FX-1). On terrain the crater's
look is the destruction layer and its size `MaterialRelationTerrainDestructionData` (DSTR-TERR-2, DSTR-TERR-3). A port
whose materials the target grid lacks leaves no decals (MAT-FX-3).

[data: spec/probes/materials.py, 2026-11-22: 19,730 MaterialRelationDecalData in DICE's grids, Decal set on 17,338]

## Open questions

- What a strip's mask shader does (all 1,424 name one) and whether the low-detail road blocks are drawn at a
  distance or on a lower setting.
- The strips' four halves (the vertex's last 8 bytes): normal or blend values.
- The `TerrainSplineData` objects' run-time use (DECAL-SRC-2).
- An impact decal's lifetime and cap (the decal pool), not traced.
