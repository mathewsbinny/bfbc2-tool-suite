# Console resource converter (console-convert)

Converts Battlefield: Bad Company 2 resources from the Xbox 360 to the PC's formats - for content only the console
version has (e.g. the Onslaught Mark V patrol boat), taken from your own console files. Nothing is shipped: class
layouts and comparison files are read from your game copy.

```
python convert.py havok <console.res> <out.res> [--exe <BFBC2Game.exe>]
python convert.py verify-havok <pc folder> <console folder>... [--exe <BFBC2Game.exe>] [--report <file>]
python convert.py texture <console.res> <out.res>
python convert.py verify-texture <pc folder> <console folder>... [--report <file>]
```

| Resource | Status | Checked by |
|---|---|---|
| HavokPhysicsData (physics) | done | all 400 shared console / PC files equivalent (FORMAT.md 1.3) |
| textures (XenonTexture -> ITexture) | DXT1 / DXT3 / DXT5 / BC5, 2D and cube maps; console 3 (DXT5A) -> PC 10, 5 (RGB565) -> PC 9, 10 (L8, 8-bit) -> PC 10, 13 (RGBA16F, untiled); the header flags kept as the console's | 251 shared console / PC textures: byte-identical or the same picture (FORMAT.md 2.3); L8: BFBC2's 19 twins (2.4) |
| meshes (MeshData) and mesh sets (rigid, composite, skinned, tree) | done (`mesh.py`, `meshset.py`; used by OPFOR's `console_markv.py`); triangle strips (part record k = 5) unrolled into lists | Mark V converted and loading on PC; every console MeshData and mesh set of the Onslaught builds parses (FORMAT.md 3-4); all 92 BFBC2 PC tree sets round-trip byte-identical |
| soot meshes (scorch overlays) | done (`soot.py`) | 30 of 30 BFBC2 twins byte for byte; BF1943's 404 of 406 (the warehouse's 16-byte vertices refused) |
| a mesh part as arrays (`mesh.decode`), console layouts (`mesh.console_layouts`) | done (used by OPFOR's WW2 `attachments.py` for Battlefield 1943's rifle parts) | 103 BFBC2 mesh parts decode alike from the Xbox and PC copies; every BF1943 rifle part decodes in range |
| sounds (Wave: EA-XMA -> EA-XAS) | done (`audio.py`, below; needs ffmpeg for decoding) | 35,811 BFBC2 twins' wrappers; converted Xbox files match their PC twins' structure; Battlefield 1943's sounds convert |

`mesh.decode(meshdata, layout, part, big_endian)` gives one part's positions, normals, tangents, binormals, UVs,
weights and bones (the part's palette applied) and its triangles, for its own vertex window - from a console mesh
with the source game's layout, or a PC one with BFBC2's. **The UVs** (2026-11-09, `mesh.texcoord_element`; the mesh
viewer uses the same rule) are the first half2 / float2 element of usage 0, else the first other float one - not
simply the first usage-0 element. The rule was found over every layout of the game's shader databases:
- BFBC2's soldiers start with a grey half4 at offset 16 and keep the coordinates in the half2 at 40;
- trees have a half4 of zeros before the bark's half2;
- material-mask meshes have colour bytes before their half4.

float2 (0x02) is decoded too (FORMAT.md 3). `mesh.console_layouts(shader_db)` reads a console shader
database's part names and layout records (BFBC2's Xbox levels, Battlefield 1943's): names are stored as name, NUL,
layout guid, and walked in runs so a printable guid byte is not read as a name's first letter.

**Texture header flag bit 1** (2026-11-09, FORMAT.md 2.1): DICE's build halves the texture for the console.
- All 539 BFBC2 textures whose Xbox copy is smaller have the bit on the PC, and 11 of the other 929 do; no Xbox copy
  has it.
- DICE's PC copies of BF1943's textures have it exactly where they are twice BF1943's console size.

So a converted texture keeps the console's flags. Console 3 (DXT5A) -> PC 10 no longer writes flags 2 (Wake's
mask-tile atlas now 0, as DICE's), and it pads its rows as L8 does.

**Format 10 (L8, 8-bit)** is tiled like the block formats with 1-byte elements; its texels are not byte-swapped, and
each PC row is padded to 4 bytes. Of BFBC2's 19 twins, the 11 of the same size are byte-identical (one font is other
art). Where the PC copy is twice the console's, each console level is within 0.3 of 255 of the PC level of its size,
and the smallest are equal.

Folders are unpacked bundles (FbrbTool). Format notes and the checks: [FORMAT.md](FORMAT.md). Tests:
`tests\test_havok.py`, `tests\test_texture.py`, `tests\test_mesh_decode.py`, `tests\test_terrain.py`,
`tests\test_soot.py`, `tests\test_audio.py` (game tier: your game copy and console files, and Battlefield 1943 in the
Rising Sun workspace; skipped without them); the suite's `tests\test_converter_units.py` (quick: L8 untiling, padding
and flags, the texture-coordinate rule). `tests\test_trees.py [--game]` (triangle strips, tree mesh sets) is run by
hand: `run_tests.py` does not list it yet.
The texture check needs `numpy`.

Developed for Project OPFOR BFBC2. Havok is a trademark of Microsoft; Xbox is a trademark of Microsoft;
Battlefield: Bad Company 2 is © Electronic Arts / DICE.

## Terrain (terrain.py)

`terrain.heightfield(xbox_bytes)` and `terrain.water_mesh(xbox_bytes)`: a level's TerrainHeightfield and WaterMesh in
BFBC2's PC format; `terrain.heightfield(bf1943_bytes, source='bf1943')` for Battlefield 1943's, whose header lacks
one field (the length of the tail's first section, computed from the tail). Proven on BFBC2's Xbox / PC twins: the
heightfield byte for byte on the 7 levels DICE did not edit after the Xbox build (the other 13: the same structure,
PC terrain edits in the heights), the water mesh byte for byte on all 20 (`tests\test_terrain.py`).

`terrain.material_map(xbox_bytes)`: the TerrainMaterialMap, byte for byte on all 20.

`terrain.visual_terrain(xbox_bytes)`: the VisualTerrain (`<lv>.visual.res`: roads, patch shaders with their
undergrowth meshes, one patch map per terrain block), the same size on both platforms. Byte for byte on mp_003, mp_005,
mp_005sr; on the others every part is byte for byte except what DICE changed for the PC after the Xbox build: the
roads of mp_001, mp_004, mp_006, mp_008, mp_012 (rebuilt, patches reordered), mp_008's patch shaders, and the
sub-terrain patch maps (the PC's are 4 x 4 cells because its sub-heightfields have 64 samples; the Xbox's 8 x 8 are
kept, matching the Xbox's 128-sample sub-heightfields).

`terrain.visual_water(xbox_bytes)`: the VisualWater (`<lv>.water.res`). Not a byte swap: the Xbox packs each vertex's
normal and tangent as DEC3N (10 bits a component, 28-byte vertices), the PC keeps 4 half floats (36 bytes), so the PC
file is 8 bytes a vertex larger. They are decoded (x / 511); everything else is byte for byte on all 20 levels (whole
files on 6), the normals / tangents 91% exact and within 1 / 511 elsewhere (the PC's last bits are not in the Xbox file).

Battlefield 1943's visual terrain and visual water (Wake Island, Iwo Jima, Guadalcanal, Coral Sea) convert and read
back as BFBC2's PC formats. The grammars are in terrain.py's docstring.

## Sounds (audio.py)

`audio.convert(xbox_bytes, 'bfbc2' | 'bf1943', streams=None)`: an Xbox 360 Wave resource (EA-XMA audio) as BFBC2's
PC Wave resource with EA-XAS v1 audio, all of it in memory. A streamed sound (a stub Wave: Battlefield 1943's
voice-over and long ambiences) needs its audio entry, `<name>_waveDataSns_<n>.res` from `streaming_sounds-00.fbrb`,
passed as `streams={n: bytes}` (n: 0 intro, 1 loop, 2 outro). From the command line:

```
python audio.py bf1943 <xbox_wave.res> <out.res> [--stream 1=<name>_waveDataSns_1.res]
python audio.py bfbc2 <xbox_wave.res> <out.res>
```

Also `audio.read_wave(bytes, 'bfbc2-xbox' | 'bfbc2-pc' | 'bf1943')` (the Frostbite wrapper: header words and
intro / loop / outro segments), `audio.decode(bytes, source, streams)` (int16 samples and the rate),
`audio.encode_xas(pcm, rate)` / `audio.decode_xas(stream)` (EA-XAS v1 in numpy, DICE's block layout). EA-XMA is
decoded by ffmpeg (decoding only, through a pipe: each XMA stream wrapped as an XMA2 RIFF WAVE); ffmpeg is found on
PATH, in `$FFMPEG` or in the WinGet Gyan.FFmpeg package. The formats are in audio.py's docstring.

Proven (`tests\test_audio.py`, about 2 minutes):

| Check | Result |
|---|---|
| wrapper on BFBC2's 35,811 Xbox / PC twins | all parse; all PC files written back byte for byte; every header word equal on both platforms (one sample count: DICE's edit) |
| EA stream headers, 1,846 in-memory twin streams | Xbox EA-XMA / PC EA-XAS v1, channels, rate, type and samples equal (1,845; the edited one) |
| DICE's EA-XAS block layout | 256 * ceil(rate / 1280) samples a block (76 frames at 48 kHz), the rest last: all 1,846 PC streams |
| EA-XMA -> PCM (42 twins, 44 streams) | ffmpeg gives the header's sample count on all; vs the PC's audio: correlation median 0.992 (min 0.906 on noise-like impacts: XMA is perceptual), lag 0 |
| EA-XAS decoder | sample for sample ffmpeg's adpcm_ea_xas on all 44 |
| EA-XAS encoder (decoded XMA in, back out) | SNR median 31.8 dB, 41.6 dB on the end credits' music; the lowest (20.4 dB, noisy high-frequency effects) equal to the best any coefficient set / shift gives |
| BFBC2 Xbox -> PC (42 files) | the PC twin's header words, segments, EA headers, block layout and file size on all; audio correlation median 0.991 |
| Battlefield 1943 | all 2,750 Waves parse; Wake's loading music, global / material ambiences, voice-over, weapon, vehicle (intro / loop / outro) and explosion sounds convert, read back as PC files, and ffmpeg's adpcm_ea_xas decodes them to the XMA source's audio (correlation 0.998 - 1.0). A one-off sweep (not in the test: 15 minutes) converted all 2,750 with their streams: SNR vs the XMA decode median 44 dB on voice-over (min 31.5), 45 dB music, 31 - 39 dB effects (min ~20 dB on noise-like ones); 10 silent placeholders exact |

BFBC2's own voice-over Waves are 44-byte stubs, the same on both platforms: the audio is streamed (PC
`streaming_vo_<lang>-00.fbrb`, EALayer3). Battlefield 1943's voice-over is the same arrangement with EA-XMA streams,
and converts like the rest once its stream entry is given (the result holds the audio in memory, as BFBC2's in-memory
sounds do). The converter writes no streamed PC sounds (those are EALayer3 on the PC, which it does not encode).
