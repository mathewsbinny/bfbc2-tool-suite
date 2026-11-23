# Bink videos: `BinkMemory` resources and the movies under `Output/win32/movies`

BFBC2 plays RAD Game Tools' Bink 1 videos through `binkw32.dll`: the level loading videos inside the bundles (bundle
type `BinkMemory`, MENU-LOAD-1) and the front-end, intro and campaign movies, which lie outside the bundles as `.res`
files under `Output/win32/movies/`. The DBX side is `Render.BinkMemoryAsset` (`Bink`, `DecompressAlphaChannel`) and
`Render.BinkTextureAsset` (`StreamFromMemory`, `Usage`, `Gamma`, `DecompressAlphaChannel`), played by
`PlayVideoEntityData` (CAMP-FLOW-2) and the menus. The suite copies the files; round SPG4 read their layout
(`spec/probes/resources.py`). The video payload's own format is RAD's (public, e.g. FFmpeg's `bink` demuxer); the
spec records only the header fields BFBC2's files are checked against.

## 1. The payload

### BINK-HDR-1: Every payload is a Bink 1 file of revision `i`

Status: **verified** · Variants: PC

A payload begins with the signature `BIKi` and the Bink 1 header (little-endian u32 words): the file size - 8, the
frame count, the largest frame's size, the frame count again, width, height, the frame rate's numerator and
denominator, video flags and the number of audio tracks. In every DICE video the size word is the payload's length - 8
and the two frame counts agree. The loading videos are 1280 x 720 at 25 fps (40) or 29.97 fps (16) with no audio;
of the 33 movies 7 carry one audio track (the attract loop, the two logos, the campaign's intro and outro and the two
concept movies, which are the only 640 x 360 ones); the rest, credits included, are silent.

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py on DICE's files, 2026-11-23: 56 distinct BinkMemory entries (57 live): BIKi 1280 x 720, 25 fps x 40, 29.97 fps x 16, no audio, the size word fits in 56; 33 movies: BIKi in 33, the size word fits in 33, one audio track in 7]
[exe: BFBC2Game.exe imports from binkw32.dll BinkOpen, BinkClose, BinkDoFrame, BinkNextFrame, BinkWait, BinkGoto, BinkPause, BinkSetVolume, BinkSetSoundSystem, BinkOpenWaveOut, BinkGetFrameBuffersInfo, BinkRegisterFrameBuffers (12 functions)]

### BINK-RES-1: A `BinkMemory` resource is the bare payload

Status: **verified** · Variants: PC

Inside a bundle the entry holds the Bink file itself from byte 0: no RES wrapper (RES-WRAP-2: only the on-demand
bundles wrap) and no header of the engine's. DICE's 56 are the levels' loading videos (`Levels/<level>/UI/<mode>`,
`LoadingMovie` for the campaign) and the front end's 16 `Movies/IntroTransitions` clips in the menu bundle.

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py, 2026-11-23: 56 of 56 BinkMemory entries begin with BIKi]

## 2. Movies outside the bundles

### BINK-OUT-1: An out-of-bundle movie is a RES wrapper with zero lengths

Status: **verified** · Variants: PC

Each of the 33 files under `Output/win32/movies/` (top level, `concept_movies_sp`, `introtransitions`, `sp_01`,
`sp_09`) starts with the 64-byte RES header of RES-WRAP-1 (`RES`, version 2, the type name zero-padded), but both
length words are 0. The type is `BinkMemory` (22: the front end's transitions and backgrounds) or `BinkTexture` (11: the
logos, rating screen, intro, credits, attract, game-over and campaign movies). A `BinkMemory` movie's Bink file
starts at byte 64; a `BinkTexture` movie has two u32 more (0, 0 in 10; 1, 1 in `fallbackbackgroundmovie`) and its
Bink file starts at byte 72. The file ends exactly after the Bink file. The names match the `Movies/...` names the
assets use (`BinkTextureAsset` partitions under `Movies/`), so the engine maps an asset's name to
`Output/win32/<name>.res` (inferred from the names; the opening function is not traced).

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py, 2026-11-23: 33 movies (DICE's, 2010 - 2012): RES version 2 with both length words 0 in 33; BinkMemory 22, BinkTexture 11 (prefix 0, 0 x 10, 1, 1 x 1); the Bink size word fits in 33]
[data: DICE's partitions, round SPG4 census: BinkTextureAsset 11 instances under movies/, BinkMemoryAsset 56 (16 under movies/introtransitions)]

### BINK-DLL-1: `binkw32.dll` is RAD's player, loaded by the executable

Status: **observed** · Variants: PC

`binkw32.dll` (175,616 bytes, DICE's file of 2010) is imported by name by `BFBC2Game.exe` (BINK-HDR-1's 12
functions); the suite neither reads nor replaces it. A video the game must play has to be a Bink 1 file this DLL
decodes; the suite has no Bink encoder (a new loading video for a new level reuses a stock one).

[exe: BFBC2Game.exe import table names binkw32.dll with _BinkOpen@8 ... _BinkWait@4]
[data: game copy: binkw32.dll dated 2010-01-12, 175,616 bytes]

## Open questions

- The `BinkTexture` movies' two extra words (looping, or the asset's `StreamFromMemory`?).
- Where the executable turns a movie asset's name into the `Output/win32/movies` path, and whether a movie inside a
  bundle would be found as well.
