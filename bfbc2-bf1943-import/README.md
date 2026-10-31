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
