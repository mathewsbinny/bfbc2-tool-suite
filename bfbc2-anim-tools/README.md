# Animation tools (granny)

Read Battlefield: Bad Company 2's Granny resources - the soldier skeleton (GrannyModel) and animations
(GrannyAnimation) - and move a bone's position track in an animation.

```
python granny.py dump <resource> [--depth N]
python granny.py bones <skeleton>
python granny.py tracks <animation> [--bones a,b] [--at t1,t2,...]
python granny.py shift <animation> --bone <name> (--rest x,y,z | --by x,y,z) [--ramp] --out <file>
python granny.py topc <xbox resource> --out <file> [--from-skeleton <s> --to-skeleton <s>]
```

`<resource>` is a file or a resource name in the game copy, e.g. `characters/skeletons/ske01` or
`animations/weapons/handheld/m1garand_rgl/1p/1p_fire`. The format (a string table, then a 32-bit Granny 2 file whose
names are indices into that table) and the curve formats are described in `granny.py`'s docstring.

`shift` writes the animation with the bone's keys moved; everything else stays byte for byte, and the Granny CRC is
recomputed. `--ramp` moves each key by how far it has got towards the last one - for a part a hand carries to a mount
(a rifle grenade onto its launcher), so it stays in the hand and only the mounted end moves.

What it found (2026-10-30): every weapon bone's bind pose is the weapon root (0, 1.097, -0.096); where a part on
jntWpn_6 / jntWpn_7 shows depends on the animation's offset of that bone from jntWpn_1. DICE's rifle grenade sets
disagree: the Garand's hold the grenade bone at z 1.2826, the Type 5's at 1.4162.

Xbox 360 resources (big-endian: BFBC2's console copy, Battlefield 1943) are read too - `dump`, `bones`, `tracks` -
and converted to the PC's format by `topc` (`to_pc`); they are never patched in place. Found with it (2026-10-31):
Battlefield 1943's `ske01` has BFBC2's 147 bones with the same parents, in another order; the body and weapon bones
bind within 1.3 mm of BFBC2's, only the face (eyes, lids, brows) differs by up to 3 cm.

**Porting animations (`topc`).** The Granny file is reversed as Granny itself reverses a file: each section's 32-bit
part as u32, its 16-bit part as u16, the 8-bit rest kept, then every object on the section's mixed-marshalling list
member by member from its type; the header, section table, pointer fixups and marshalling lists as u32; the CRC
recomputed. DICE's table before the string table is u32 throughout. That table (`bone_table`) holds bone indices:
animations a list of {bone, k, k time ranges}, delta animations their bones and then such a list; with
`--from-skeleton` / `--to-skeleton` they are renumbered by bone name and kept sorted as DICE writes them (a port
from BF1943's soldier to BFBC2's). Tracks bind by name, so nothing else changes. Checked: all 2,654 Granny resources
of BFBC2's own Xbox `mp_common` / `sp_common` with a PC twin of the same size convert to that twin (DICE's table and
strings byte for byte, every value of the object tree, Granny's unused Padding aside; the console's section layout is
kept); 31 BF1943 animations convert to exactly BFBC2's copy. The pipeline that uses it: `bfbc2-bf1943-import`.

Tests: `tests\test_granny.py` (game tier; also a big-endian header, the write refusal, and - when extracted into
OPFOR's `work\bf1943\skeleton` - BF1943's skeleton against the PC one) and `tests\test_granny_port.py` (quick: the
bone tables; `--game`: the Xbox corpus and BF1943). Generated aim poses (`*_generatedaimanimation`, AimAnimation) are
another format - they hold memory addresses - and are not read or converted.

Developed for Project OPFOR BFBC2. Granny is a trademark of Epic Games (RAD Game Tools); Battlefield: Bad Company 2
is © Electronic Arts / DICE.
