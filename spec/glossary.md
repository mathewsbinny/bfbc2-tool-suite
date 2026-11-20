# Glossary

Terms as the specification uses them. The rule that defines a term in full is named where there is one.

## Containers and files

| Term | Meaning |
|---|---|
| bundle | an `.fbrb` archive: a gzip header listing named, typed entries, then their data (`formats/fbrb.md`) |
| entry | one named item of a bundle: name, type, flags, offset, size, size2 (FBRB-ENT-1) |
| entry type | the resource kind of an entry: `<non-resource>`, a resource type such as `ITexture`, or `*deleted*` (RES-TYPE-1) |
| `<non-resource>` | the entry type of DBX partitions and of data without a resource type (RES-NAME-2) |
| `*deleted*` marker | a size-0 entry of type `*deleted*` that hides the name in later bundles of a level's stack (FBRB-DEL-1, BNDL-DEL-1) |
| size2 | an entry's second length: its size, or size - 64 in a RES-wrapped entry (FBRB-ENT-5) |
| padded layout | DICE's padding of a stored payload: `align4096` or `gap64`, 0xBF bytes between entries (FBRB-LAY-2) |
| fbrb.manifest | the suite's record of an unpacked bundle's entries and layout, from which `pack` rebuilds it (FBRB-MAN-1) |
| resource | a non-DBX entry (`.res`) in an engine-specific binary layout, its kind given by the entry type |
| RES wrapper | the 64-byte `RES` header before a resource in the async on-demand bundles (RES-WRAP-1) |
| Dist / Package | `game\Dist\win32` (the install) / `game\Package` (patches and DLC; its copy wins) (BNDL-LOC-1) |
| BIGF | EA's flat archive of named members; a SwfMovie is one; index big-endian, length word little-endian |
| STFS | the Xbox 360 content package format (LIVE / PIRS / CON): 4 KiB blocks with interleaved SHA-1 hash tables |
| XDVDFS | the Xbox disc file system (XISO); each directory a binary tree of 4-byte-aligned entries |

## DBX and the data model

| Term | Meaning |
|---|---|
| DBX | Frostbite 1's serialised object data; binary in the game (`formats/dbx.md`), XML in the suite's text form |
| varint | an unsigned LEB128 number, 7 bits a byte, least significant group first (DBX-VAR-1) |
| head byte | an element's kind (high nibble) and attribute count (low nibble) (DBX-ELEM-1) |
| kind | an element's storage class: 0xA container, 2 text, 7 numbers, else a two-byte scalar (6 in the game) |
| partition | one DBX file, `<name>.dbx` in a bundle: one asset's instances; its name is the entry name without `.dbx` |
| instance | a container under the partition with `guid` and `type` (`<Namespace>.<Class>`); unique by GUID within its partition only |
| primary instance | the instance the partition's `primaryInstance` names, the asset it stands for; it alone carries `id` |
| field / complex / array / item | a scalar or reference member / a structure, vector or matrix / a list / a list element |
| local reference | a `ref` value that is a lower-case 36-character GUID of the same partition |
| external reference | a `ref` value `<partition path>/<guid>` naming an instance of another partition |
| null reference | `ref="null"`, no target |
| LinearTransform | 16 floats in four rows (right, up, forward, translation); each row's 4th word is padding |
| bundle stack | the ordered bundles a level loads (its own, the base level's, the commons); the first entry of a name wins |
| package.mft | the bundle registry: BundleSource blocks listing each bundle's BundleTarget stack (`model/registration.md`) |
| bundleManifest | the flat bundle list, `path<TAB>zero-GUID<TAB>1` per line |
| level checksum | the server's start-up check over registered levels; crashes on unregistered or missing level bundles and null blueprints |
| LevelData | the level partition's primary instance: Objects, EventConnections, LevelDescription, cull-tree and level settings |
| Objects list | `LevelData.Objects`, the local references to the level's top-level entities; unlisted world objects are not loaded |
| event connection | a `{Source, Target, SourceEvent, TargetEvent}` complex in `LevelData.EventConnections` |
| model group | a Static / Rigid / Middlegrowth model group or Forest entity whose members its `_HavokAsset` partition places |
| HavokAsset partition | `Levels/<LV>/<Group>_HavokAsset.dbx` plus `.res`: a model group's placements (Assets, Transforms) and collision |
| root files | level files found by the level's name (shader databases, emitter system, model groups, cull trees, animation sound map) |
| mode layout | a level made from another (MP_005GR) that loads its base level's bundles after its own |
| mesh alias | a blueprint whose `Mesh` field names another blueprint's mesh, and which therefore uses that blueprint's collision |

## Terrain, water, physics

| Term | Meaning |
|---|---|
| main block | a level's central TerrainHeightfield (`Heightfield-000`, 1024 or 2048 samples a side) |
| sub-terrain | one of the lower-resolution height fields around the main block; its tail is a min / max quadtree |
| patch map | VisualTerrain's per-block grid of cells, each naming its mask-atlas tiles, flags and patch shader |
| tiled layer / rule layer | a terrain patch layer that reads a mask-atlas tile / one computed by a rule (base, slope, destruction, sea floor) |
| mask atlas | the texture of 64-pixel mask tiles that tiled terrain layers sample; the patch map's header gives its layout |
| DICE's (Havok) wrapper | the part, box and material-list header before the Havok packfile in a HavokPhysicsData resource |
| part run | the wrapper's bytes giving each part's index into the material list, or a material table (model groups) |
| part material / face material | the material read for part damage / the material of an extended-mesh subpart (hit material on model-group members) |
| packfile | Havok's binary serialisation (magic `57 E0 E0 57 10 C0 C0 10`): header, sections, fixup tables |
| fixup | a packfile table entry: local (a pointer within a section), global (across sections), virtual (an object and its class name) |
| hkClass initializer | a function in BFBC2Game.exe that constructs a Havok class description; the suite reads member layouts from them |
| MOPP | Havok's byte-coded bounding-volume tree over a mesh shape's triangles (`hkpMoppCode`) |
| DEC3N | the Xbox 360's packed vector of three signed 10-bit components plus a 2-bit w |

## Meshes, textures, shaders

| Term | Meaning |
|---|---|
| MeshData | one mesh LOD's resource: its parts and vertex and index buffers (`<mesh>_lod<N>_data.res`) |
| mesh set | the per-mesh resource (Rigid, Composite, Skinned or TreeMeshSet): LODs, shader names, bounds, parts or bones |
| LOD | level of detail; each LOD of a mesh set has one MeshData |
| vertex layout | a shader-database record (GUID, 16 elements, count, stride) defining a part's vertex layout |
| shader database | a level's compiled-shader resource (`Dx9/Dx10/Dx11_Single.res`; Xbox `Single.res`) |
| entry (shader database) | a `<mesh asset>/<shader asset>` name whose GUID is MD5(name); it owns a group of programs |
| program | a shader-database record pairing a vertex and a pixel shader with parameter blocks and a render state |
| parameter block | a shader-database record of a shader's textures, constants, engine textures and sampler states |
| render state word | the bit field at program +0x10 (transparency, depth, culling, blend, alpha to coverage) |
| ITexture / XenonTexture | the PC texture resource (92-byte header, levels in rows) / its Xbox 360 counterpart (big-endian, Xenos-tiled) |
| packed mip tail | on Xenos, the smallest levels stored together in one tile |
| Xenos tiling | the Xbox 360 GPU's texel order in 32 × 32-unit macro tiles (`XGAddress2DTiledOffset`) |
| XDK shader container | the Xbox 360 shader blob: magic 0x102A110x, constant table, literal table, header, microcode |
| Granny | RAD Game Tools' model and animation format; BFBC2 wraps Granny 2 files in DICE's per-bone table and a string table |
| DXT5A | an Xbox-only single-channel block format (DXT5's alpha half), converted to PC L8 |

## Navigation and culling

| Term | Meaning |
|---|---|
| Kynapse | the AI navigation middleware in `BFBC2Game.exe`; the bots route on its path data |
| pathdata.hpd | Kynapse path data for one level and pathfinding type (infantry or vehicle), in 100 m sectors |
| sector (path data) | a 100 m grid square of path data, id `0xXXYY`, holding a route graph and a runtime mesh |
| route graph | per-sector nodes and edges (cost = 16 × length) the path search runs on |
| border node | a route node with id bit 31 set, shared by neighbouring sectors; only the owner's copy has its edges |
| runtime mesh | a sector's `KynogonRuntimeMesh`: walkable areas in tiles, outlines, portals, step records, neighbour links |
| area (path data) | one walkable single-layer region inside one tile of a sector's runtime mesh |
| portal / border portal | a segment joining two areas of one sector / leading across the sector border |
| D line / F cell | step records of an area used by the straight-move test: obstacle edges / two-way height-break cells |
| detailed zone | a path-data sector built at pitch 0.2 m with 6 × 6 tiles instead of 0.8 m with 3 × 3 |
| SectorOffsets | the per-level, per-type partition that places the path-data sector grid in the world |
| navsample | NavKit's grid of collision samples (`ONS1` .bin), the input for building path data (the suite's own format) |
| static cull tree | a level's bounding-sphere hierarchy over its static objects; a client tree and an exclusive tree per level |
| cull slot (`Enumeration`) | a placed object's 1-based index into the cull trees' leaves; 0 means in no tree |
| EnumOffset | the exclusive cull tree's slot base, equal to the client tree's leaf count |

## Menus, sound, text

| Term | Meaning |
|---|---|
| APT | EA's UI movie format (compiled Flash for EA's runtime): BFBC2's menus, HUD and loading screens |
| SwfMovie | the resource type of an APT movie: a BIGF archive of `<name>.apt`, `.const` and `.geo` |
| global constant index | an index into a movie's `.const` table, as PushData and ConstantPool lists hold it |
| constant pool | the list the last ConstantPool instruction set; the EA pool opcodes index it |
| keep layout / pack layout | the suite's two ways of writing an edited movie: unchanged structures in place / everything shifted by multiples of 4 |
| Wave | a sound resource: a header and three segments (intro, loop, outro), each in-memory audio or a streamed stub |
| waveDataSns | the separate `<name>_waveDataSns_<0-2>.res` entry in `streaming_sounds` / `streaming_vo` holding a streamed segment |
| EA audio core stream | EA's framing: two big-endian header words then blocks; version 1 SPS (BFBC2), version 0 SNR/SNS (BF1943) |
| EA-XAS | EA's 4-bit ADPCM: 76-byte frames of 128 samples of one channel; the PC's in-memory audio codec |
| EA-XMA | the Xbox 360 audio codec: XMA2 packets in per-stream parts |
| key hash | the localization hash: `h = 0xFFFFFFFF; h = h*33 + byte` (32-bit) |
| histogram | a language's u16 character table used to decode localized texts |

## The specification

| Term | Meaning |
|---|---|
| rule | a numbered normative statement `<AREA>-<SECTION>-<n>` with a status and evidence (`README.md`) |
| verified / implemented / observed / inferred | a test checks it / code relies on it / seen in files or the exe / reasoned |
| the game copy | Project OPFOR BFBC2's `game\`: DICE's PC files plus OPFOR's installed bundles |
| stock | DICE's files only |
