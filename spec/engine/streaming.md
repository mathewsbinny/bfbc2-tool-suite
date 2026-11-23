# Streaming: the async, streaming and on-demand bundles

Besides each level's bundles BFBC2 loads a set of global bundles: the start-up bundle, the streamed sounds and voice,
the on-demand sounds and award pictures, and per-weapon and per-cutscene async bundles. This file is where they are,
what holds them, how the executable mounts them and what the suite does with them. Written by round SPG2 (2026-11-22)
from the game copy's bundles and registry files and a read of the executable around the bundles' names (no RE of the
mount functions themselves); the audio side is SND-WAVE-1 / 2, SND-STRM-1, SND-OD-1 ([audio.md](audio.md)), the
registry REG-MFT-5 / REG-BM-2 ([model/registration.md](../model/registration.md)), the absent voice bundles BNDL-LOC-3
and the precedence PREC-ORD-3 ([model/bundles.md](../model/bundles.md)), the stored layouts [formats/fbrb.md](../formats/fbrb.md).

## 1. Mounting

### STRM-MNT-1: The global sound and award bundles are mounted at fixed paths

Status: **observed** · Variants: PC

One function (0x5657B0 - 0x566043) reads `Core.DiskLayoutEmulation`, `Network.Version` and "Game", then pairs each
global bundle with a virtual path and calls 0x4E48E0 for each pair, in this order: `streaming_sounds` ->
`/sound/streaming/`, `async/ondemand_sounds` -> `/sound/ondemand/`, `async/ondemand_awards` -> `/async/awards/`.
0x4E48E0 does not mount: it grows the table the pairs go in (STRM-MNT-3, round SPG2-rest2).

[exe: 0x565C79 push "streaming_sounds" (0x14185D4); 0x565CC7 push "/sound/streaming/" (0x14185C0); 0x565DC6 "async/ondemand_sounds"; 0x565E14 "/sound/ondemand/"; 0x565F14 "async/ondemand_awards"; 0x565F62 "/async/awards/"; calls 0x4E48E0 at 0x565D0F, 0x565E5D, 0x565FAC]

### STRM-MNT-3: The mount table and the mount: a hashed entry index at a virtual path

Status: **observed** · Variants: PC

0x5657B0 does not open the bundles: it appends one 0x1C-byte `BundleMountPoint` record per pair to a table it gets
from 0x1386AE0 (0x4E48E0 is the table's grow call, a virtual call at +0xC). The record's fields are named by the type
information: `VfsPath` (+0, the virtual path), `Bundle` (+0xC, the bundle name) and `IsResourcePath` (+0x18), false for
`streaming_sounds` and true for the two `async/ondemand_*` bundles (corrected by round SPG2-rest3: round SPG2-rest2 had
the two strings the other way round and called the flag "on demand"). The mount itself is 0x464B10 (file system,
bundle name, path), the one STRM-MNT-2's voice bundle goes through: it makes a 0x238-byte bundle object (0x4DE490)
that reads the bundle's entry names and indexes them by a case-insensitive hash (h = 5381; h = h x 33 ^ lower(c) per
byte) in a sorted table of 0x20-byte records searched by halves, then attaches it at the path (0x4DCF30 -> 0x4DCCB0).
A name is therefore found in a mounted bundle whatever its case, and two names that differ only in case are one entry
to the engine.

The table is drained by the game object's constructor 0x56EDE0 ("Battlefield", vtable 0x01418170), right after
0x566050 (which runs 0x5657B0): when the "Game" settings' `UseBundles` (+0x94) is set, it mounts every record in
order through 0x464B10(file system, `Bundle`, `VfsPath`), and hands each record whose `IsResourcePath` is set to the
resource manager as well (`*(0x0154C444)` vtable +0x38 with the record and 0). The same constructor then mounts the
voice bundle (STRM-MNT-2). With `UseBundles` off nothing is mounted from the table.

[exe: BFBC2Game.exe 0x5657B0 (records: 0x4245B0 string assigns, flag stores 0x565CFD (0), then 1, 1; 0x1386AE0 the table); 0x4E48E0 (`jmp [vtable + 0xC]`); 0x464B10 (alloc 0x238, 0x4DE490, 0x4C4410, 0x4DCF30); 0x4DE490 (hash loop: 0x1505, `* 0x21 ^ c`, upper case folded; binary search over 0x20-byte records); 0x4DCF30 -> 0x4DCCB0; field descriptors 0x1C149E0 `Bundle` +0xC, 0x1C149F8 `VfsPath` +0, 0x1C14A10 `IsResourcePath` +0x18, GameSettings `UseBundles` +0x94 (0x1C14FB0), `BundleMountPoints` +0x20 (0x1C14FC8)]
[exe: BFBC2Game.exe 0x56EDE0 (0x566050 at 0x56EF03; settings group "Game" 0x1410BA0 through 0x4F4C80 at 0x56F033, `cmp byte [+0x94]`; the loop 0x56F087 - 0x56F0D2: `push [rec]; push [rec+0xC]; push fs; call 0x464B10` at 0x56F0A6, `cmp byte [rec+0x18], 0` then `[0x154C444]` vtable +0x38; records 0x1C apart); the voice bundle at 0x56F43D; 0x800200 the same voice mount when the language changes (0x800282)]

### STRM-MNT-2: One streamed voice bundle per session, by language

Status: **observed** · Variants: PC

The language id (0xF99E30, minus 1, through a 7-entry jump table at 0x56F4F0: fr, de, es, it, en, ru, pl; any other
value en) names `streaming_vo_<lang>` (format 0x56F422), mounted at `/sound/streaming_loc/` (0x56F432, call
0x464B10); the start-up bundle `async/startup` is loaded right after (0x56F489 -> 0x561C10, push at 0x561FBA). Only the
active language's voice bundle is ever opened, which is why the six absent ones do not matter (BNDL-LOC-3).

[exe: 0x56F3DA call 0xF99E30; jump table 0x56F4F0, default "en"; 0x56F422 push "streaming_vo_%s"; 0x56F432 push "/sound/streaming_loc/"; 0x56F489 call 0x561C10; 0x561FBA push "async/startup"; a second copy of the sequence at 0x800267 / 0x800277]
[inferred: "only one is opened" from the code's shape: one formatted name per call]

## 2. The async bundles

### STRM-ASYNC-1: A level's async bundles are its cutscenes, named by a hash

Status: **observed** · Variants: PC

`async/levels/<sp level>/cutscene/c_<32 hex>-00.fbrb`: 119 bundles over the 13 campaign levels (sp_003_b 17, sp_01 17,
sp_007 12, sp_02 12, sp_006 10, sp_04 10, sp_009 9, sp_005 7, sp_03 7, sp_004_b 6, sp_008 5, sp_005_b 4, sp_008_b 3).
The executable builds the name as `async/` + level + `/cutscene/c_` + hash (an `a_` variant exists in code, no file).
No multiplayer, Onslaught or OPFOR level has an async bundle.

[exe: 0x62D9C9 push "async/", 0x62D9BF push "/cutscene/c_"; 0x8AF66F "/cutscene/a_"]
[data: the game copy's Dist async/levels, 2026-11-22: 119 files in 13 level folders, all under cutscene/, none a_*; Package async has no levels folder]

### STRM-ASYNC-2: A weapon's async bundle, and which registry lists it

Status: **observed** · Variants: PC

`async/weapon/<name>-00.fbrb` (the executable builds `async/weapon/` + name: 0x80F175, 0x80F2F1): Dist has 323, Package
47 (15 also in Dist, 32 Package-only: Vietnam's and later weapons). `package.mft` lists exactly Package's 47 plus
`async/ondemand_sounds`, `async/startup`, `async/ondemand_awards`; `bundleManifest` lists Dist's 442 (323 weapons, 119
cutscenes) plus the same three, and none of the 32 Package-only weapon bundles.

[exe: 0x80F175 / 0x80F2F1 push "async/weapon/" (0x1431C64); 0x891535 / 0x8B4532 push "async/weapon"]
[data: the game copy, 2026-11-22: Dist async 445 bundles, Package async 50; package.mft's async sources 50; bundleManifest's 445 async lines]

### STRM-REG-1: The registry's spelling of the global bundles

Status: **observed** · Variants: PC

`bundleManifest` writes the streamed and on-demand bundles with a leading slash (`/streaming_vo_<lang>` x 7,
`/streaming_sounds`, `/async/ondemand_sounds`, `/async/ondemand_awards`), `async/startup` without one and the root
voice bundle as `VO`; `package.mft` lists the seven `streaming_vo` and `streaming_sounds` as `game:` sources with
`package:` then `game:` targets. Whether the slash matters is not known.

[data: the game copy's Dist bundleManifest and package.mft, 2026-11-22: the lines as described]

### STRM-CNT-1: What each global bundle holds

Status: **observed** · Variants: PC

| Bundle | Dist | Package |
|---|---:|---:|
| `streaming_sounds` (all `_waveDataSns_<n>` streams) | 4,744 | 956 (1 `*deleted*`) |
| `streaming_vo_en` (all streams) | 7,513 | 1,482 |
| `async/ondemand_sounds` (all Wave) | 205 | 14 |
| `async/ondemand_awards` (all ITexture) | 889 | 295 (with OPFOR's WW2 pictures) |
| `async/startup` | 806 (112 pixel shaders per Dx) | 268 (1 marker) |

Names Package shares with Dist: `streaming_sounds` 2 (one the Package marker hiding a Dist entry), `streaming_vo_en`
13, `async/startup` 84 (among them 3 Dx11 particle-shadow shaders Package overrides). A `*deleted*` marker in a
Package streaming bundle against its Dist twin bears on bundles.md's open question of markers on resources.

[data: the suite's bundle lister over the 10 files, 2026-11-22; the game copy includes OPFOR's installs in Package ondemand_awards and startup]

## 3. The suite

### STRM-SUITE-1: The suite reads the global bundles Package first and writes none

Status: **implemented** · Variants: PC

The level writer and the editor's catalogue read `ondemand_awards` and `startup` Package first, then Dist; the export's
closure treats every bundle under a `_common` folder or `async` as shared (what it names resolves there, the walk
stops); a staged bundle holding a streamed wave is refused (XVAL-SND-1, SND-STRM-1). The WW2 install's check counts
the WW2 pictures and the loading picture in the on-demand awards bundle. No suite code writes `streaming_sounds`,
`streaming_vo_*`, the weapon or the cutscene bundles; the console converter reads streamed stubs and their segments.
The renderer's post-process shaders come from Dist's start-up bundle only, so the particle-shadow shaders Package
overrides are not seen there.

[test: rust/bc2-levelwrite/src/export.rs::streamed_waves_named]
[code: rust/bc2-levelwrite/src/export.rs:393-415]
[code: rust/bc2-editor/src/catalog.rs:140-151]
[code: rust/bc2-install/src/plan.rs:117-123]
[code: rust/bc2-render/src/post.rs:103-112]
[code: bfbc2-console-convert/audio.py:46]

## Open questions

- The mount and its drain are read (STRM-MNT-3, round SPG2-rest3: 0x56EDE0 under `UseBundles`); whether a mount
  point shadows a level's own `sound/...` names, and what the resource manager does with an `IsResourcePath` mount,
  are not.
- When the weapon and cutscene bundles are opened (kit selection, first spawn, the cutscene's start), and whether a
  level's bundle stack consults them for names.
- Whether `Core.DiskLayoutEmulation` and the registry's leading slash affect lookup.
- Whether Package's `streaming_sounds` / `streaming_vo` replace or overlay Dist's (the marker suggests an overlay).
