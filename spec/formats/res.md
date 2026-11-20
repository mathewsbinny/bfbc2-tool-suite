# Resource entries (`.res`) and the RES wrapper

A bundle entry (`formats/fbrb.md`) is either a DBX partition (`.dbx`, type `<non-resource>`) or a **resource**: a
binary blob in an engine-specific layout, named `.res`, whose kind is the bundle entry's type field. This file is
the catalogue of those kinds, how resources are named and where they live, and the one generic wrapper some
resources carry: the 64-byte `RES` header of the async on-demand bundles. Each kind's own layout is its own file.

Counts: the game copy, 2026-11-19 (FBRB's corpus: 444,286 live entries in 1,086 bundles), structure only.

## 1. The type catalogue

### RES-TYPE-1: Resource types

Status: **observed** · Variants: PC

The entry type of every live entry is one of the 37 below (plus `*deleted*` for markers, FBRB-DEL-1). Every
resource type is on `.res` names; `<non-resource>` is on `.dbx`, `.res`, `.bin` and `.dbmanifest` names.

| Type | Entries | Specified in | The suite |
|---|---|---|---|
| `Wave` | 216,393 | `formats/audio.md` | reads, writes PC (EA-XAS) |
| `<non-resource>` | 171,254 | `formats/dbx.md` (156,533 `.dbx`); RES-NAME-2 | reads, writes |
| `GrannyAnimation` | 14,634 | `formats/granny.md` | reads, converts Xbox 360 -> PC |
| `MeshData` | 11,273 | `formats/mesh.md` | reads, converts, writes |
| `ITexture` | 9,784 | `formats/texture.md` | reads, converts, writes |
| `HavokPhysicsData` | 4,128 | `formats/havok.md` | reads, converts |
| `RigidMeshSet` | 3,830 | `formats/mesh.md` | reads, writes |
| `AnimTreeInfo` | 3,409 | - | copied only |
| `DeltaAnimation` | 1,666 | `formats/granny.md` (as Granny) | reads |
| `SkinnedMeshSet` | 1,216 | `formats/mesh.md` | reads, writes |
| `AimAnimation` | 1,126 | `formats/aimanim.md` | reads, converts |
| `SwfMovie` | 1,027 | `formats/apt.md`, `formats/bigf.md` | reads, edits |
| `CompositeMeshSet` | 958 | `formats/mesh.md` | reads, writes |
| `OccluderMesh` | 814 | - | copied only |
| `TerrainHeightfield` | 527 | `formats/terrain.md` | reads, converts |
| `SootMesh` | 495 | - | converts Xbox 360 -> PC (`bc2-convert` `soot_mesh`) |
| `TreeMeshSet` | 247 | `formats/mesh.md` | reads, writes |
| `GrannyModel` | 178 | `formats/granny.md` | reads, converts |
| `Dx9ShaderDatabase`, `Dx10ShaderDatabase`, `Dx11ShaderDatabase` | 136, 137, 137 | `formats/shaderdb.md` | reads, writes |
| `Dx9PixelShader`, `Dx10PixelShader`, `Dx11PixelShader` | 115 each | `formats/shaderdb.md` | reads |
| `Dx9VertexShader`, `Dx10VertexShader`, `Dx11VertexShader` | 14 each | `formats/shaderdb.md` | reads |
| `VisualTerrain` | 97 | `formats/terrain.md` | reads, converts |
| `IrradianceVolume` | 76 | - | copied only |
| `BinkMemory` | 66 | - | copied only |
| `WeatherSystem` | 57 | - | copied only |
| `WaterMesh` | 53 | `formats/water.md` | reads, converts |
| `Terrain` | 53 | `formats/terrain.md` (always 0 bytes) | copied |
| `TerrainMaterialMap` | 53 | `formats/terrain.md` | reads, converts |
| `VisualWater` | 52 | `formats/water.md` | reads, converts |
| `ImpulseResponse` | 21 | - | copied only |
| `RagdollResource` | 2 | - | copied only |

Evidence: [data: 444,286 live entries of 1,086 bundles grouped by type and extension (round SP1 survey)] [code: rust/bc2-formats/src/fbrb/header.rs:20-21]

### RES-TYPE-2: A resource's kind is its entry type, not its bytes

Status: **implemented**

Most resource layouts carry no magic or type field of their own (a mesh set, MeshData, a Wave header, a height field
begin with plain numbers), so the entry type is the only reliable statement of the kind. A tool that reads a `.res`
outside its bundle must keep the type with it: FbrbTool's manifest records it (FBRB-MAN-1), and the suite's readers
of unpacked folders take it from there (texqa's texture search reads the manifest's type) or test the layout's own
consistency instead.

Evidence: [code: rust/bc2-cli/src/texqa/run.rs:263-277] [code: rust/bc2-formats/src/fbrb/tool.rs:917-922] [inferred: the type catalogue's layouts (texture, mesh, Havok, Wave ...) begin with counts and sizes, not identifiers; only the Havok packfile inside its wrapper, the BIGF archive of a SwfMovie and the RES wrapper have magic numbers]

### RES-TYPE-3: Byte order follows the platform

Status: **observed** · Variants: PC, Xbox 360, BF1943

PC resources are little-endian; Xbox 360 resources (BFBC2's console bundles and Battlefield 1943's) are big-endian,
with the same field order wherever the suite has compared both (Havok wrappers and packfiles, textures' headers,
MeshData, mesh sets, water, terrain). Within a resource, data the GPU reads keeps the GPU's layout (Xenos tiling and
16-bit swaps on the Xbox 360). The bundle container itself is big-endian on every platform (FBRB-HDR-2).

Evidence: [doc: bfbc2-console-convert/FORMAT.md:14] [doc: bfbc2-console-convert/FORMAT.md:79] [doc: bfbc2-console-convert/FORMAT.md:162] [data: 358 HavokPhysicsData resources shared by the console's and the PC's bundles: same size, byte order the only difference (FORMAT.md 1.3)]

## 2. Names

### RES-NAME-1: A resource sits beside its asset

Status: **observed**

An asset's resources are named after its partition: `<partition path>.res` for its main resource, or
`<partition path>_<suffix>.res` / `<partition path>.<suffix>.res` for the others (`_lod<N>_data.res` MeshData,
`_Havok.res` collision, `.heightfield-000.res` terrain blocks). The engine finds a resource by name from the asset,
not by a DBX reference; the naming rules per kind are in the data model (PART-NAME-2) and each format file.

Evidence: [data: 146 `_HavokAsset.dbx` entries of the Dist level bundles, each with a `_HavokAsset.res` of the same stem (model/bundles.md)] [doc: bfbc2-console-convert/FORMAT.md:162]

### RES-NAME-2: Resources typed `<non-resource>`

Status: **observed**

Not every non-DBX entry has a resource type. The `<non-resource>` entries that are not `.dbx` are: the streamed
sound data of `streaming_sounds` / `streaming_vo_en` (14,694 `.res` under `sound/`: the `waveDataSns` blobs of
`formats/audio.md`), the localization tables of `overlay` (25 `.bin` under `localization/`: `formats/loc.md`), and
one `database.dbmanifest` in each `build_overlay` bundle (Dist and Package).

Evidence: [data: Dist streaming_vo_en 7,513, streaming_sounds 4,744; Package streaming_vo_en 1,482, streaming_sounds 955; overlay 25 `.bin`; build_overlay 1 `.dbmanifest` in Dist and 1 in Package]

## 3. The RES wrapper

### RES-WRAP-1: The 64-byte RES header

Status: **verified** · Variants: PC

A resource in an async on-demand bundle is stored behind a 64-byte header:

| Offset | Size | Content |
|---|---|---|
| 0 | 3 | ASCII `RES` |
| 3 | 1 | u8 version: 2 |
| 4 | 52 | the resource type (the bundle entry's type, e.g. `ITexture`), ASCII, zero-padded to byte 56 |
| 56 | 4 | LE u32 the resource's length (the entry's size - 64) |
| 60 | 4 | LE u32 the resource's length again |
| 64 | | the resource, exactly as a plain entry of its type holds it |

In DICE's bundles the bundle entry's size2 is the inner length too (FBRB-ENT-5, FBRB-LAY-4).

Evidence: [code: rust/bc2-levelwrite/src/awards.rs:41-55] [code: rust/bc2-convert/src/texqa.rs:387-393] [test: rust/bc2-levelwrite/src/awards.rs::pictures_in_a_template_form] [test: rust/bc2-editor/src/catalog.rs::a_texture_decoded] [data: 1,403 wrapped entries: all version 2, the type name equal to the entry type and zero-padded, both length words equal to size - 64; 1,293 of them (all of DICE's) with size2 = size - 64]

### RES-WRAP-2: Only the async on-demand bundles wrap

Status: **observed** · Variants: PC

The wrapper occurs in exactly two bundle families: `async/ondemand_awards` (`ITexture`: the award, rank, map-list
and loading-screen pictures) and `async/ondemand_sounds` (`Wave`). No other `.res` or `.bin` entry of any bundle
begins with `RES`, and the same types in other bundles are not wrapped (8,600 other `ITexture` entries).

Evidence: [data: wrapped: Dist ondemand_awards 889, Package ondemand_awards 295, Dist ondemand_sounds 205, Package ondemand_sounds 14; every `.res` / `.bin` entry of the 1,086 bundles checked, no other begins with `RES`]

### RES-WRAP-3: Readers strip it, writers of on-demand entries add it

Status: **verified**

A reader that takes a texture or sound from any bundle checks for `RES` at byte 0 and reads from byte 64 (the
editor's picture decoder, texqa, the map exporter's award pictures); a writer of an entry for an on-demand bundle
wraps the resource (`awards::wrap`). The game reads OPFOR's wrapped pictures whose bundle size2 equals size
(FBRB-ENT-6), so the header's own length words, not size2, give the resource's length.

Evidence: [code: rust/bc2-editor/src/catalog.rs:185-190] [code: rust/bc2-levelwrite/src/awards.rs:58-64] [code: rust/bc2-cli/src/texqa/run.rs:268-271] [test: rust/bc2-editor/src/catalog.rs::a_texture_decoded] [test: rust/bc2-levelwrite/src/awards.rs::pictures_in_a_template_form]

## Open questions

- What the async loader does with the RES header: whether it checks the type name or version, and which of the two
  length words (and size2) it uses. Why the header is there only in the on-demand bundles (a streaming reader that
  learns a resource's type and length before the bundle header is parsed is the likely reason).
- The layouts of `AnimTreeInfo`, `OccluderMesh`, `IrradianceVolume`, `BinkMemory`, `WeatherSystem`,
  `ImpulseResponse` and `RagdollResource`, which the suite copies but does not read; `SootMesh` is converted but not
  specified yet.
- What `database.dbmanifest` lists and whether the game reads it.
- Whether the game resolves a resource by name alone or also checks the entry type (a resource of the right name and
  the wrong type).
