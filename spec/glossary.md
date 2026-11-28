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
| patch map | VisualTerrain's per-block grid of cells, each naming its mask-atlas tiles, flags and patch shader (RNDR-TERR-1) |
| tiled layer / rule layer | a terrain patch layer that reads a mask-atlas tile / one computed in the shader by a rule (base, slope, destruction, sea floor) (RNDR-TERR-3) |
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

## Engine subsystems (layer 3)

| Term | Meaning |
|---|---|
| session state | the session's state machine number; a load runs 5, 7, 13, 4, 8, 6, 9, 14, 3 (3 = in play) (LOAD-PH-1) |
| Md5Buffer | the level checksum's pass over `LevelData.Objects` (0x6EFBC0), faulting at 0x6EFF8A / 0x6EFCA4 (LOAD-CSUM-1) |
| startup bundle | `async/startup-00.fbrb` (Package, then Dist): descriptions, game modes, the level list, menu config, post shaders (LOAD-START-1) |
| mode level | one level per game mode of a map (MP_005, MP_005GR, MP_005SR), loading its map's bundles after its own (LVL-MODE-1) |
| content set / game mod | BFBC2, BFBC1, Vietnam or WW2; DICE's client knows game mod BC2 (0) and VIETNAM (1), Project OPFOR BFBC2's native eras add PACIFIC (2) and BC1 (3) (ERA-NAT-1); the game mod follows the loaded level's `MatchmakingModID` (GAME-MODE-6, ERA-MOD-2, LVL-DESC-1) |
| mode layer | a level's gameplay entities for its mode (seed types, objectives and what they reach), apart from the world objects (GAME-LAYER-2) |
| HQ / base | a capture point with `UnableToChangeTeam` true, owned by one team; a flag has it false (GAME-CQ-1) |
| M-COM | a Rush objective: a placed object whose events reach an arm / disarm `GameInteractionEntityData` (GAME-RUSH-1) |
| stage (Rush) | the M-COMs whose `onDestroyed` feed the same `MultipleTriggerEntityData` counters (GAME-RUSH-1) |
| prominence | an objective's ground height minus the median ground 30-100 m around it (GAME-CQ-9) |
| mode key | a mode's matchmaking key (`CONQUEST`, `RUSH`, `SQRUSH`, `SQDM`), the server browser's mode column (GAME-MODE-3, NET-SB-1) |
| in-process host | the server `hostMultiplayerLocal` creates inside the game's own process, reached through the in-memory transport (NET-SES-2, NET-SES-3) |
| transport mode | the client-server link: 0 in-memory, 1 sockets, 2 remote join (NET-SES-3) |
| level catalogue | the UI manager's table of levels built from the description partitions (MENU-CAT-1) |
| working AI level | a level on which bots were seen to move and fight in a play test (AI-NEED-2) |
| Flat data mode | Kynapse's mode in which a go-to computes its path inside the call (AI-KY-3) |
| AI player | a server player with the AI flag (+0xD84), made by an AI spawner's spawn; a bot (BOT-CTL-1) |
| AI proxy | the AI's per-player object (0xC0 bytes), for humans and bots alike (BOT-RT-5) |
| AI input object | the proxy's input writer: an EntryInputState pushed into the player's input handler each frame (BOT-CTL-2) |
| EIA | an `EntryInputAction` (EiaThrottle 0 ... EiaUndefined 49): one input of an EntryInputState (BOT-CTL-3) |
| order | an `*OrderEntityData` of the level telling named AI soldiers what to do once issued (AIO-CLS-1, AIO-EVT-1) |
| readiness | a bot's alertness: patrol, ready or combat; it picks sensing limits and reaction times (AIO-RDY-4, BOT-PER-1) |
| draw class | opaque, tested (alpha test, alpha to coverage or a discard) or blended; it sets the draw order (RNDR-ORD-1) |
| depth pass | `psUnprojectDepthBuffer`, turning scene depth into view distances before the first draw that reads them (RNDR-ORD-3) |
| EnvironmentMain | the weather pass drawing the sky, haze and fog over the scene by depth (RNDR-WTHR-3) |
| shader-set override | a placement's choice of another of its mesh set's shader sets, named `<partition>/<GUID>` (RNDR-DRAW-2) |
| on-demand wave | a wave with `IsOnDemand`, its audio in `async/ondemand_sounds`, loaded by the game itself (SND-OD-1) |
| streamed wave | a wave with `IsStreaming`: a stub whose audio is in `streaming_sounds` or `streaming_vo_<lang>` (SND-WAVE-1) |
| trigger set (VOAsset) | voice-over events with their conditions and lines (SND-VO-1) |
| line bank | `sound/vo/vo_<lang>` or `vo_common`: the announcer's recorded lines (SND-VO-1) |
| VOTypes | a kit's list of voice-over persons (SND-VO-1) |
| voice-over system | the server's or the client's instance of the raise-and-match machinery (SND-EVT-2) |

## The suite's transformations (layer 4)

| Term | Meaning |
|---|---|
| twin | a resource both the console (or BF1943) copy and the PC copy of BFBC2 ship: a conversion's ground truth (XCONS-HK-1) |
| oracle (WW2 rebuild) | the recorded sha1 of every WW2 rebuild output; a change gives the same outputs only if a full rebuild reproduces them all (XWW2-RB-2) |
| recorded golden | a retired Python or C# tool's answers, recorded before it was deleted: the parity oracle of the Rust ports |
| stage protocol | OPFOR's install bookkeeping: `<file>.opfor-<stage>-orig` backups, last-writer records, atomic writes (XINST-STG-1 to XINST-STG-5) |
| last writer | the stage whose record names a shared file's current sha1; only it may rebuild from its backup or revert (XINST-STG-3, XINST-STG-4) |
| ledger | the game copy's install records (bc2_gamefs::ledger): each owner's whole files stacked with what was there before, giving files back byte for byte in any order of installs and uninstalls (XINST-GL-1, XINST-LED-2) |
| stage (map export) | the folder `mapexport build` writes: the bundles, `stage.json`, `report.txt`, `registration.txt`; `mapexport check` proves it (XEXP-CHK-1) |
| closure (export) | the walk of the partitions a new level names; what does not resolve is copied into the level's bundle, never registered (XEXP-CLO-1) |
| reslot | the export's step giving moved or duplicated objects new exclusive cull-tree leaves (XCULL-RES-1) |
| side file (navtune) | a level's navigation edits, which `navkit apply` applies (XNAV-APP-1) |

## The specification

| Term | Meaning |
|---|---|
| rule | a numbered normative statement `<AREA>-<SECTION>-<n>` with a status and evidence (`README.md`) |
| verified / implemented / observed / inferred | a test checks it / code relies on it / seen in files or the exe / reasoned |
| the game copy | Project OPFOR BFBC2's `game\`: DICE's PC files plus OPFOR's installed bundles |
| stock | DICE's files only |
| L1 study / L1 rule | the map editor's placement study of DICE's layouts (`bfbc2-map-editor/placement/`) and its warning rules (`L1-CQ-001` ...), which the gameplay rules name; `trace.py` checks each named L1 ID against `l1_rules.json` |
| crash index | `engine/crashes.md`: every recorded fault address with the rule behind it |
