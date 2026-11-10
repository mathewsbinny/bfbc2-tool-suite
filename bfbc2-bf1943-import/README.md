# Battlefield 1943 import (bf1943import)

Brings Battlefield 1943 (Xbox Live Arcade, Xbox 360) resources into Bad Company 2's PC formats, from the user's own
copy. Nothing is shipped: the outputs go to Project OPFOR BFBC2's `work\bf1943\import` (or `--out`), never into a
game folder or the source.

```
python bf1943import.py list [<filter>...]          what the source holds and what import would do with each
python bf1943import.py import <filter>... [--out <folder>]
python bf1943import.py check                       every animation and skeleton converted and checked in memory
```

Filters are parts of a resource name (any case; all must match), e.g. `import M1Garand_Bayonet/` or
`import US_LongKnife_Knife/1P/`. `import.json` in the output folder keeps one record per resource: source bundle and
SHA-256, type, what was done (or why not), the output's SHA-256; later imports add to it.

**Source:** BF1943's extracted `dist\xenon` folder - by default the Project Rising Sun workspace next to the suite
(`operation-rising-sun\work\extracted\dist\xenon`), or `--source` / `BF1943_XENON`. The bundles are read with the
BF1943 suite's reader (`bf1943-tool-suite\bf1943-fbrb-tool\fbrb1943.py`, next to this suite). Read only.

## What is converted

| Type | Done |
|---|---|
| GrannyAnimation, DeltaAnimation | to little-endian (`granny.py` `to_pc`); when every track is a soldier bone, DICE's per-bone table is renumbered from BF1943's soldier skeleton to BFBC2's (the same 147 bones and parents in another order) and kept sorted as DICE writes it. Other animations (vehicles, props) keep their indices and the record says so. |
| GrannyModel | skeletons to little-endian |
| XenonTexture | to the PC's ITexture with the console converter (`bfbc2-console-convert\texture.py`): DXT1/3/5, BC5; 301 of BF1943's 330 shared-bundle textures at first (2026-10-31). The 8-bit formats convert since then: console 3 (DXT5A) and 10 (L8) to the PC's format 10, with the console's header flags kept (flag bit 1: DICE's build halved the texture for the console) |
| everything else | listed with the reason: mesh data and mesh sets (need PC vertex layouts from a shader database), Havok physics (BF1943's Havok differs), DBX partitions (another game's data), aim animations (hold memory addresses), animation trees, sounds |

How sure the animation conversion is: `granny.py`'s `to_pc` reproduces all 2,654 Granny resources of BFBC2's own
Xbox `mp_common` / `sp_common` that have a PC twin of the same size (DICE's table and strings byte for byte, every
value of the object tree, a correct CRC); and 31 of the 395 BF1943 animations that BFBC2 also has come out equal to
BFBC2's copy after renumbering. The others differ because DICE re-compressed or edited them for BFBC2 (other
curve formats; the eye and eyelid tracks, where the two soldiers' faces differ).

**Third-person undeploys (2026-11-09).** DICE added tracks to the 3p undeploys it brought into BFBC2: on all 22 it
shipped, exactly the bones the same weapon's same-stance deploy animates and the undeploy does not (25: 19 weapon
parts, 6 thumbs). `Importer.undeploy_tracks` adds them with `granny.add_tracks` (anim-tools). Each one is held at
the deploy's last frame, as DICE did, and a weapon part the deploy has parked 0.5 m or more from the weapon's
HandsIkPose takes that pose. Measured on the 22:
- the weapon parts match DICE's exactly in 12 undeploys and in 386 of 418 tracks; DICE re-animated the rest;
- the thumbs are 13.4 degrees from DICE's on average and 8 degrees median, against 20 for the bind pose.

The 14 BF1943-only undeploys (K98, Springfield, long knife, wrench, bayonet, grenades) get 88 tracks.
`Importer(source, thumb_donors={BF1943 folder: BFBC2 folder})` can take the 6 thumbs from a BFBC2 undeploy of the
same base set instead. It is off unless asked for: measured leave-one-out on the 22, another weapon's thumbs do not
beat the deploy's last frame. The Garand from the M24 is 9.1 / 15.1 degrees standing, against 0.1 / 7.3; it is
exact only where the donor is the same animation (Vietnam's Garand, Thompson, M1911, TNT).

## Parity with DICE's own conversions (`portparity.py`)

`python portparity.py [name part ...] [-o report.txt]` converts every model BFBC2 also ships in a PC version (148:
the campaign prologue's Zero and Corsair, props, trees, weapons, soldiers) with `meshport` and `texture.convert`, and
compares the result with DICE's, element by element. Per part: identical, precision (normal frames within 0.25 deg -
DICE built its PC normals from the source art, so about 0.1 deg is the Xbox data's own rounding), differs (the
elements named, with their largest difference; a whole part moved by one offset is reported as rebased), other-model
(another revision: other counts). Per mesh set: LODs, composite part names and transforms. Per texture: format, size,
mips and the picture at the smaller size (PSNR). Writes the report and a JSON of every row.

First run (2026-11-08): no converter fault in the matched models - the differences are DICE's edits (parts rebased,
UVs remapped, per-vertex data DICE added) or other revisions. The two gaps it found are closed the same day: tree mesh
sets (`meshset` TreeMeshSet: a composite LOD + the billboard shader's name + a number; all 92 of BFBC2's round-trip
byte-identical) and triangle strips (the part record's `k` = 5: BF1943's bark parts; `mesh.convert` unrolls them -
the palm's bark top gives DICE's triangles exactly). 147 of the 148 shared models now convert (the last,
objects/ui/uibackground, has no layout in BF1943's shader databases).

## Effects (`effectport.py`)

`effectport.port(src, ix, effect, level)` ports a BF1943 effect and everything it reaches: the EffectAsset, the
emitter documents (BF1943 keeps them per level: `levels/<level>/emitters/...`, the partition inside named
`emitters/...`), their textures (converted; BFBC2's own kept where it has the name) - sounds and other partitions
BFBC2 has stay references. Each partition goes through dbxport on BFBC2 templates that cover every Emitter.* type
(DICE's small detonation and the emitter documents found to hold the rest). Checked first with `dbxparity.py` on
the 70 effect partitions both games have: no enum, kind, nested or renamed field; of the 26 fields BFBC2 added, the
varying ones take DICE's usual value (`DEFAULTS`: SpawnProbability 1, no post effect), which the port checks.
Used for the Bofors' flak burst (effect_aa_detonationblast: 3 emitters, 6 textures) and the Sherman's shell
detonation (effect_detonation_l_01).

`effectport.port_many(src, ix, effects, level)` ports several at once with one template search - Wake's 44 impact
effects. The search over BFBC2's 18,343 emitter documents reads them a bundle at a time and parses only those whose
bytes name a type still needed (it read and parsed them one by one: ~35 min). A BF1943 sound an effect reaches (BFBC2
lacking it) is not ported here: aligned on effect templates its Audio.* types stayed unaligned and its waves were left
out. It is listed (a `sound` record, `sounds_of(report)`) for the caller's sound porter, and the effect's reference to
it is kept. The report is a `dbxport.Report`: its records hold every partition's dbxport records (their `partition`
set) and the effect's own (`summary`, `sound`, `missing`, `default`); its lines are the summary and the notable ones
as `port()` chose them. Before, `port_many` re-made a line from every record, and Wake's grid report grew to 22,172
lines.

Both take `catalogue=` (dbxcatalogue, passed on to dbxport) and dbxport's `schema` mode. With `schema='report'`, a
shape no converter handles is kept as BF1943's and reported as a `schema` record. This matters for BF1943's
`Emitter.SampleTextureData.GradientData`, which holds two 512-texel RGBA rows (4,096 floats) where all of DICE's hold
2,048; which one BFBC2 reads is not established. OPFOR's `material_grid.py` ports Wake's effects this way. A converter
for the gradients is in progress (branch tonight/s2fx, not merged).

## Emitter atlases (`emitteratlas.py`)

A level draws every particle from its two emitter atlases (`Levels/<level>/emittertexturealpha.res`: 8-bit, format
10; `emittertexturenormals.res`: BC5), and each emitter document stores where its image sits in them (its
TextureInfo: origin, frame step, frame count and columns, the same for the normal map). DICE wrote those fields per
level against that level's atlas, so a level made of other levels' emitters and of BF1943 ports draws most of them
from the wrong image when it ships any one atlas. `emitteratlas.build(emitters, sources, alpha_like, normals_like)`
takes each emitter with the key of the atlas it was authored against and gives one alpha and one normals atlas
holding every image (deduplicated: the same image in two levels' atlases is kept once), and each emitter re-pointed
into them - only its texture fields change. `prove()` reads every image back through the new fields and compares it
with its source's. The module docstring has what was measured (the fields, the frames drawn, DICE's slots, 3-texel
inset and mip rule) and the method. `tests\test_emitteratlas.py` (quick tier) builds one from synthetic atlases.

First use (OPFOR's `emitter_atlas.py`, 2026-11-09): Wake's 607 emitters from 12 BFBC2 levels' atlases and BF1943
Wake's - alpha 2048 x 2048 (mp_005's size, 82 % used), normals 1024 x 1024 (nam_mp_006's size; mp_005's 1024 x 512
cannot hold nam_mp_002cq's water spray, whose 2 x 2 normal frames span that level's whole normal atlas); all 1,581
images equal their sources at levels 0-1 (alpha texel for texel; normals block for block, or correlating 0.9999
where two levels' encoders differ).

## Models (`meshport.py`)

`meshport.Porter(source, meshview_index).port(mesh_name, donors=[...])` ports a BF1943 model: every LOD's MeshData
(the console converter's `mesh.convert` from BF1943's layouts - read from its levels' shader databases with
`mesh.console_layouts` - into PC layouts with the same elements), the mesh set (each LOD's size recorded), the
textures in the model's folder, and a plan for the shader database (each part's layout: BFBC2's own part, a donor
mesh's part - whose material the new one is cloned from - or any layout with those elements; the model's
materials and textures). Models skinned to the soldier (ske01) have their bone palettes and the mesh set's bone
list renumbered from BF1943's bone order to BFBC2's. `meshport.preview_parts(result)` gives the ported LOD read
back from its PC bytes for the mesh viewer's `render` / `page`.

Proven on the oracle: BF1943 weapon models that BFBC2 shipped (Type 100, Nambu pistol, M1911A1, Mk 2 grenade, Shin
Gunto) port to BFBC2's own files (parts, counts, offsets, index buffer, palettes; every position, normals within
0.01). Ported so far for the WW2 side project: the Springfield, the k98, both bazookas, the long knife and the wrench
(1p, 3p), and the vehicles and fixed guns: Willys, Sherman (hull, tracks), LCVP, B-25, Ki-21, the bombing-raid cockpit
and bunker, the window mount. BF1943's rigid mesh sets have no flag byte a LOD (`meshset.parse(..., flag=False)`;
165 read, 37 shared with BFBC2 write as its files). A part with no BFBC2 layout of exactly its elements takes the
layout keeping the most of them (a donor's first). Installing (the shader database entries, the level's bundles) is
Project OPFOR's `wake_install.py`.

`Porter.port(name, rename=...)` ports a mesh under a new name or namespace (2026-11-09). `rename` is a {old prefix:
new prefix} map (any case, the longest first) or a function. The rename covers:
- the mesh set's LOD settings, shader set names and shader names (`rename_meshset`);
- the LOD MeshData and mesh set names (`result['names']`);
- the textures;
- the mesh folder's skeleton pose, now converted onto BFBC2's soldier (`result['poses']`).

`plan['renamed']` maps every old name to its new one for the partitions the caller clones, for example BF1943's
soldiers next to BFBC2's same-named ones.

## Physics (`havokport.py`)

`havokport.convert(bf1943_resource, template, havok.classes(exe))` turns BF1943's physics (Havok 4.6.1, Xbox 360)
into BFBC2's PC format (Havok 5.5.0). BF1943's packfiles carry no class reflection, so what changed was measured on
the physics both games ship (Kurogane, Zero, Corsair): the physics classes gained a "p"; every object kept its size;
hkRigidBody and hkMoppBvTreeShape moved fields (their 4.6.1 layouts are in the module); the rest have 5.5.0's layout.
Stage 1 rewrites the packfile as a big-endian 5.5.0 one - the class-name section from a BFBC2 file (`template`: its
names and signatures), the two classes rewritten in place, the deactivator objects 5.5.0 dropped left out, class-name
strings renamed, fixups rebuilt; stage 2 is the console converter (`havok.py`), which swaps every field by BFBC2's
own layouts and stops on any byte they do not account for.

Proven: BF1943's Kurogane converts to BFBC2's Kurogane object for object (62) and field for field but for values the
game rebuilds at load; the Zero and Corsair (re-exported by DICE for BFBC2) pair all but the subparts DICE merged.
Converted for the WW2 side project: the Willys (and wreck), Sherman, LCVP and its shell, B-25, Ki-21, the air-raid
bunker and the machine-gun mounts. Velocity limits go through Havok's hkUFloat8 table, read from the user's
executable (`ufloat8_table`; the nearest value). Without the table only 200 (0x7F, the Kurogane oracle's) is known.

**Model-group w lanes (2026-11-09; Project OPFOR `research\HAVOK_AABB_W.md`).** BFBC2's model-group physics carry
values in w lanes that BF1943's 4.6.1 files leave 0, and havokport writes them as DICE's exporter does:
- every shape subpart with children gets `translation.w` 1.0 (all 277,950 such subparts of BFBC2's levels; Havok's
  empty last subpart keeps 0);
- `hkExtendedMeshShape` is in `AABB_W`;
- a shape built of shape subparts gets `aabbCenter.w` / `aabbHalfExtents.w` from `aabb_w`. That is each child's local
  box turned by the w lanes DICE's exporter leaves in the rotation columns (`subpart_w_lanes`; lane 0 is
  2(yz - xw)), merged subpart by subpart in file order with the centre / half extents stored in float32 after each,
  as `hkpExtendedMeshShape::addShapesSubpart` does.

The result is bit-identical to DICE's on all 299 model-group shapes of the stock levels. `tests\test_havokport.py`
requires every mp_004 / mp_005 shape bit-identical and every file byte for byte.

## MOPP trees (`mopp.py`)

`mopp.py` decodes Havok's MOPP byte code (`hkpMoppCode`, the bounding-volume tree of a `hkpMoppBvTreeShape`): the box
query machine (rescale, jumps, terminal offsets, axis / sum / difference splits with 8- and 16-bit jumps, one-value
splits, bounds, terminals of 1-4 bytes). `query` gives the terminals whose cells overlap a box, `ray_cast` is built on
it, and `complete` checks that every shape's own box reaches its key. The opcode table is in the module docstring.

**Verdict (2026-11-09): BF1943's 4.6.1-built MOPP code is valid in BFBC2's 5.5.0, so nothing is rebuilt.**
- Both versions use the same opcode set: no other opcode is in either game's 88 shared physics files.
- On those files every one of the 54,245 triangles of the 236 converted codes reaches its terminal. DICE's 203 codes
  reach all 49,744; 58 flat triangles on a cell boundary only with a 1 mm query tolerance, as Havok's queries have.
- 11,800 random rays through BF1943's codes and 10,150 through DICE's hit exactly what brute force hits.
- The 37 list-shape codes reach every child.
- The trees are as tight as DICE's: 28.3 % against 27.7 % of the keys per own-box query.

The byte differences from DICE's codes are the builders' split choices. Tests: `tests\test_converter_units.py`
(quick: a hand-made code queried cell by cell, the ray cast against brute force, an unknown opcode refused) and
`tests\test_havokport.py` (game: the Kurogane, boat house and sandbag barrier, converted and DICE's).

## Data partitions (`dbxport.py`)

**Stage 1 rewrite (2026-11-09; research DBXPORT_DEEP_AUDIT.md in Project OPFOR).** The module docstring has the rules.

```python
cat = dbxcatalogue.load(ix)                                    # meshview.Index; BFBC2's schema, guids, class names
t = dbxport.Templates.from_index(ix, [<BFBC2 partitions>], cat)
data, report = dbxport.port(bf1943_bytes, t, resolve, catalogue=cat, ours={partition: guids},
                            stand_ins=STAND_INS, donors=[<paired BFBC2 partitions>],
                            prune=False, required=None, schema='error', conventions=True, bones=None,
                            strict=False, name=<partition>)
report.records                                                 # [Record(path, action, old, new, source, step, ...)]
```

`dbxcatalogue.py` reads every stock partition once (84,733; meshview's index) and gives:
- the instance types (with the executable's class and enum names);
- each type's fields in DICE's order and the shapes DICE writes per field path;
- enum values, and whole-number or float evidence per 4-byte field;
- array item prototypes and a class default per type (plus a second one from another partition, for fair tests);
- guid -> partition for all 606,896 instances, and each partition's primaryInstance.

It is cached as `work\tools_out\dbx_catalogue.pkl` beside meshview's index, rebuilt when a bundle or BFBC2Game.exe
changes.

In short:
- Templates are ranked per instance: DICE's same-guid counterpart, the caller's paired donor partitions (`donors=`),
  the pool's instance whose role matches (input actions; slot - the array of local references naming it, so zoom
  level n takes zoom level n; position among its type - the seat), then the catalogue's class default. Every candidate
  is kept, folders are globbed sorted, a pool partition the game copy lacks fails (`MissingTemplate`), and each
  instance's choice is a `template` record.
- References are resolved over (partition, guid), in this order:
  1. local;
  2. a partition that will exist (`ours=` with its guids, the catalogue, the caller's `resolve`);
  3. relocated: a guid DICE moved is rewritten only when it is in exactly one BFBC2 partition (BF1943's beach halo
     aimers). A material's guid is in all 74 level grids: picking the first was arbitrary, and Wake's ambient sounds
     pointed at mp_001sr's grid;
  4. stand-ins: the caller's entry for the target (`target` or `(field, target)` -> `'partition/guid'`, a partition
     (its primary instance), `'partition/*'` (the same guid under that partition), `None` or `TEMPLATE`), then the
     counterpart's own reference, then a field-wide `(field, '*')` stand-in;
  5. otherwise null, recorded.

  A template's value is never substituted for another role's (the Sherman's cannon fired with the Humvee's horn);
  `ROLE_GUARDS` refuse a horn as a weapon sound, a vehicle's death explosion as a weapon's detonation, a soldier clip
  on an object. The same rules cover what a template adds and instances without a template; `primaryInstance` follows
  a left-out primary; `strict=True` fails on any of it.
- `drop_untemplated` drops only classes BFBC2 lacks (the catalogue's classes and the executable's names), after
  `TYPE_AS`.
- Every field's shape is checked against the template and the catalogue (container / leaf, number count and kind, enum
  names the executable has; an array of references where BFBC2 holds structs, or the reverse). A mismatch is a
  `SchemaError` unless a converter handles it:
  - `ENUM_NUMBERS`, with or without a template;
  - a bool into the target's own number type;
  - `ONE_TO_ARRAY` with empty or null;
  - the explicit `NESTED` map;
  - `INLINE`: BF1943's reference to a local instance where BFBC2 holds the struct inline becomes the instance's
    fields (Wake's RichPresenceConnection.ContextPairs);
  - an empty curve.

  `schema='report'` keeps such a field as BF1943's and records it (`schema`) instead. An empty template array takes
  its item from another instance of the type. Not portable, and refused: the five 3p soldier anim trees (their aim
  tables: 104 numbers in BFBC2, 40 in BF1943) and the achievements BFBC2 holds inline.
- Value passes:
  - EIA -> Eia in every field (`EIAFireCountermeasure` is BFBC2's `EiaFireCountermeasure`);
  - bone indices on BFBC2's skeleton (`BoneTables`: BoneId by its BoneName, SoldierIKData / SkeletonAsset indices).
    A BoneId of 0 stays 0: it is an unset entry, and DICE kept both grenades' 0;
  - DICE's conventions for the BF1943 weapons it brought over (`DICE_CONVENTIONS`: gravity, 600 m/s, finite ammo and
    pickups, no auto-aim, press to zoom, sprint look, AimHead, AnimBaseSet, the template's recoil).
    `conventions=False` keeps BF1943's values. They apply to the soldiers' weapons only (`objects/weapons/`): DICE
    kept the planes' guns at 480 m/s without drop.
  - An empty number array stays empty unless DICE always writes one count there (a curve takes the template's).
- Empty arrays are written as TEXT elements (all 9,400 of DICE's are).
- `port` returns `(bytes, Report)`: the old text lines, and `report.records` (`Record(path, action, old, new, source,
  step, note, partition)`) for callers to read instead of parsing text.

`python dbxparity.py --predict` scores the passes and conventions on the weapon and vehicle partitions both games
ship. It uses fair pools (`FAIR_POOLS`), and the catalogue's class defaults never come from the partition's own
instances. For each value it gives the baseline, the prediction and DICE's counterpart. That is how the conventions
were scoped to the soldiers' weapons: before, 9 of 18 gravity values were right and the Zero's four were wrong.
`tests\test_dbxport_rules.py` (quick) holds the deep analysis' 19 edge cases and a
test per rule; `tests\test_dbxport_fair.py` ports every partition both games ship on a fair pool and checks it against
DICE's (`--quick`: 7 partitions, in the quick tier when the game data is there).

BF1943's binary DBX is BFBC2's format (strings, varints, big-endian numbers); the schema differs by a few fields per
type. `dbxport.port(bf1943_bytes, dbxport.Templates([...BFBC2 partitions or unpacked folders...]))` aligns each
instance with a BFBC2 instance of its type: shared fields keep BF1943's values in BFBC2's order, BFBC2-only fields
take the template's value, BF1943-only fields are left out, every change reported; written with
`bfbc2core.dbx.write` (new in the core: a binary DBX writer; 300 BFBC2 partitions parse back to the same tree, and
DbxEdit reads its output and writes it back unchanged).

**Checked against DICE (`dbxparity.py`, 2026-11-08).** BFBC2 ships many BF1943 partitions under the same names with
the same instance GUIDs; `python dbxparity.py [name part ...] [-o report.txt]` compares every field of every instance
both games have and classifies the differences (enum: a number where DICE has a name; kind; nested: a field DICE keeps
one level deeper; renamed: a BF1943-only field whose value DICE holds under another name; value; bf1943-only;
dice-only). What it found is now in dbxport:
- `RENAMED` splits / renames: `AngularMomentumDamping` -> `AngularMomentumDampingAcc` + `...Deacc`,
  `CollectiveInputScale` -> `CollectiveThrottleInputScale` + `CollectiveBrakeInputScale`, `UpsideDownAngel` ->
  `UpsideDownAngle` (DICE fixed the typo).
- `ENUM_NUMBERS`: BF1943 stores some enums as numbers (`ChildRotationBodyData.RotationInput`,
  `ChildMovingBodyData.InputAction`); BFBC2 by name. The pairs are DICE's own (4 EiaYaw, 5 EiaPitch, 6 EiaRoll, 7
  EiaFire, ...); BFBC2's EntryInputActionEnum (its type table: EiaThrottle 0 .. EiaUndefined 49) agrees with BF1943's
  only up to 6, so unpaired numbers are reported `UNMAPPED` and take the template's value. Kept as numbers, BFBC2 read
  no input: the ported steering wheels and gun mounts did not turn.
- Kinds: a BF1943 bool where BFBC2 has a number (the sound emitters' `HFDamping*`) becomes 1.0 / 0.0.
- Nested: a BFBC2 struct whose fields BF1943 keeps one level up is built from them - BF1943's recoil
  (`FireLogic.Max/MinRecoilAngleX/Y/Z`, `Max/MinRecoilFov`, `RecoilFollowsDispersion`) is BFBC2's
  `FireLogic.Recoil.*`; before, every ported weapon fired with its template's recoil.
- Classes BFBC2 lacks: `TYPE_AS` writes BF1943's `LandingGearEntryComponentData` (the planes' pilot seat: a player
  entry that retracts the gear) as a `PlayerEntryComponentData`; `NOT_IN_BFBC2` drops `HudVehicleTrackerData` (no
  such class in BFBC2's executable). `RotationInput` 16 (the gear's legs and hatches) becomes `EiaUndefined`: BFBC2
  has no gear input (DICE's never-flown prologue planes put it on `EiaPitch`).
- BF1943-only fields left out (SpottedTime, GiveScore, NeverSlowDown, AutoFireDelay, ShotLimit, ...): checked with
  dbxparity - DICE's own conversions of the same instances lack every one of them too.
- Not portable: BF1943's `BulletEntityData.VehicleDetonationTargets` (the Bofors' proximity fuse by vehicle type) -
  BFBC2 has only the radius.

Values BF1943 writes otherwise are translated (`eia_value`, `hint_value`, reported as `value ...`): the input
actions in every field (BF1943's enum spelling `EIAChangeEntry` / `EIUndefined` -> BFBC2's `EiaChangeEntry` /
`EiaUndefined`: the engine resolves enums by name, so BF1943's named no key; `EIAFireCountermeasure` ->
`EiaFireCountermeasure`, BFBC2's value 11) and the seat hints' texts (`VH_SWITCH` -> `VEHICLE_HINT_CHANGE_POSITION`, which has BFBC2's PC text naming the key;
`VH_BOAT_THROTTLE`, which BFBC2 lacks -> `VEHICLE_HINT_ACCELERATE`).

Not ported, by design: BF1943's anim-tree and input partitions. BFBC2 ships its anim trees compiled (AnimTreeInfo)
with no node instances in the partitions, so a ported weapon uses a BFBC2 tree cloned with BF1943's animations swapped
in place. Four BF1943 types do not exist in BFBC2 (its executable has no such class): AirRaidEffectsData and
TeamVehicleProjectileEntityData (the air raid: the bunker fires the bombers as projectiles), CarHornComponentData,
ScorePartComponentData - the air raid has to be built from BFBC2's own classes.

Not yet: the converted animations still need BFBC2 data to use them (DBX animation entries, anim trees, a weapon that
plays them) - that is game work in Project OPFOR, not this tool's.

Tests: `tests\test_bf1943import.py` (quick without data: output-path containment, type routing, refused imports;
`--game`: `check` on the whole source, an import of the Garand bayonet's resources read back, `import.json`
merging, the source unchanged), `tests\test_meshport.py` (game: the oracle models, the WW2 models),
`tests\test_havokport.py` (game: the Kurogane oracle, the re-exported Zero and Corsair, the WW2 physics, the
model-group w lanes on mp_004 / mp_005, the MOPP codes), `tests\test_emitteratlas.py` (quick: a merged atlas from
synthetic ones), the suite's `tests\test_converter_units.py` (quick: w lanes, MOPP, meshport's rename),
`tests\test_dbxport.py` (game: the writer on BFBC2 partitions, the Springfield mesh partition ported),
`tests\test_dbxport_rules.py` (quick: the rules on crafted partitions), `tests\test_dbxport_fair.py` (game: every
shared partition against DICE's; `--quick` in the quick tier). The
converters' own: `bfbc2-anim-tools\tests\test_granny_port.py`, `bfbc2-console-convert\tests\test_mesh_decode.py`.

Developed for Project OPFOR BFBC2. Battlefield 1943 and Battlefield: Bad Company 2 are © Electronic Arts / DICE;
Granny is a trademark of Epic Games (RAD Game Tools).
