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

Not yet: the converted animations still need BFBC2 data to use them (DBX animation entries, anim trees, a weapon that
plays them) - that is game work in Project OPFOR, not this tool's.

Tests: `tests\test_bf1943import.py` (quick without data: output-path containment, type routing, refused imports;
`--game`: `check` on the whole source, an import of the Garand bayonet's resources read back, `import.json`
merging, the source unchanged). The converter's own: `bfbc2-anim-tools\tests\test_granny_port.py`.

Developed for Project OPFOR BFBC2. Battlefield 1943 and Battlefield: Bad Company 2 are © Electronic Arts / DICE;
Granny is a trademark of Epic Games (RAD Game Tools).
