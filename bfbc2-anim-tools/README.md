# Animation tools (granny)

Read Battlefield: Bad Company 2's Granny resources - the soldier skeleton (GrannyModel) and animations
(GrannyAnimation) - and move a bone's position track in an animation.

```
python granny.py dump <resource> [--depth N]
python granny.py bones <skeleton>
python granny.py tracks <animation> [--bones a,b] [--at t1,t2,...]
python granny.py shift <animation> --bone <name> (--rest x,y,z | --by x,y,z) [--ramp] --out <file>
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

Test: `tests\test_granny.py` (game tier). Generated aim poses (`*_generatedaimanimation`) are another format and are
not read.

Developed for Project OPFOR BFBC2. Granny is a trademark of Epic Games (RAD Game Tools); Battlefield: Bad Company 2
is © Electronic Arts / DICE.
