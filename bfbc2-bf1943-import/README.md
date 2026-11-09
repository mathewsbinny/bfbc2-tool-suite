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
| XenonTexture | to the PC's ITexture with the console converter (`bfbc2-console-convert\texture.py`): DXT1/3/5, BC5; 301 of BF1943's 330 shared-bundle textures (8-bit formats not yet) |
| everything else | listed with the reason: mesh data and mesh sets (need PC vertex layouts from a shader database), Havok physics (BF1943's Havok differs), DBX partitions (another game's data), aim animations (hold memory addresses), animation trees, sounds |

How sure the animation conversion is: `granny.py`'s `to_pc` reproduces all 2,654 Granny resources of BFBC2's own
Xbox `mp_common` / `sp_common` that have a PC twin of the same size (DICE's table and strings byte for byte, every
value of the object tree, a correct CRC); and 31 of the 395 BF1943 animations that BFBC2 also has come out equal to
BFBC2's copy after renumbering. The others differ because DICE re-compressed or edited them for BFBC2 (other
curve formats; the eye and eyelid tracks, where the two soldiers' faces differ).

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

`effectport.port_many(src, ix, effects, level)` ports several at once with one template search (the search reads
BFBC2's 18,343 emitter documents when a type is rare: ~35 min; once per call, not per effect) - Wake's 44 impact
effects. A BF1943 sound an effect reaches (BFBC2 lacking it) is not ported here: aligned on effect templates its
Audio.* types stayed unaligned and its waves were left out. It is listed (`SOUND` report lines, `sounds_of(report)`)
for the caller's sound porter, and the effect's reference to it is kept.

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
layout keeping the most of them (a donor's first). Not yet: installing (the shader database entries).

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
bunker and the machine-gun mounts. Open: the MOPP code is BF1943's (4.6.1-built; BFBC2's own differ by a byte or two
where DICE rebuilt them) - the game has to show it collides right; velocity limits other than 200 are refused
until their 8-bit encoding is known.

## Data partitions (`dbxport.py`)

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

Values BF1943 writes otherwise are translated (`hint_value`, reported as `value ...`): the vehicle seat hints'
input actions (BF1943's enum spelling `EIAChangeEntry` / `EIUndefined` -> BFBC2's `EiaChangeEntry` /
`EiaUndefined`: the engine resolves enums by name, so BF1943's named no key; the countermeasure action BFBC2 lacks ->
undefined) and texts (`VH_SWITCH` -> `VEHICLE_HINT_CHANGE_POSITION`, which has BFBC2's PC text naming the key;
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
`tests\test_havokport.py` (game: the Kurogane oracle, the re-exported Zero and Corsair, the WW2 physics),
`tests\test_dbxport.py` (game: the writer on BFBC2 partitions, the Springfield mesh partition ported). The
converters' own: `bfbc2-anim-tools\tests\test_granny_port.py`, `bfbc2-console-convert\tests\test_mesh_decode.py`.

Developed for Project OPFOR BFBC2. Battlefield 1943 and Battlefield: Bad Company 2 are © Electronic Arts / DICE;
Granny is a trademark of Epic Games (RAD Game Tools).
