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
| textures (XenonTexture -> ITexture) | DXT1 / DXT3 / DXT5 / BC5, 2D and cube maps | 251 shared console / PC textures: byte-identical or the same picture (FORMAT.md 2.3) |
| meshes (MeshData) and mesh sets (rigid, composite, skinned) | done (`mesh.py`, `meshset.py`; used by OPFOR's `console_markv.py`) | Mark V converted and loading on PC; every console MeshData and mesh set of the Onslaught builds parses (FORMAT.md 3-4) |
| a mesh part as arrays (`mesh.decode`), console layouts (`mesh.console_layouts`) | done (used by OPFOR's WW2 `attachments.py` for Battlefield 1943's rifle parts) | 103 BFBC2 mesh parts decode alike from the Xbox and PC copies; every BF1943 rifle part decodes in range |
| sounds (Wave: EA-XMA -> EA-XAS) | done (`audio.py`, below; needs ffmpeg for decoding) | 35,811 BFBC2 twins' wrappers; converted Xbox files match their PC twins' structure; Battlefield 1943's sounds convert |

`mesh.decode(meshdata, layout, part, big_endian)` gives one part's positions, normals, tangents, binormals, UVs,
weights and bones (the part's palette applied) and its triangles, for its own vertex window - from a console mesh
with the source game's layout, or a PC one with BFBC2's. `mesh.console_layouts(shader_db)` reads a console shader
database's part names and layout records (BFBC2's Xbox levels, Battlefield 1943's): names are stored as name, NUL,
layout guid, and walked in runs so a printable guid byte is not read as a name's first letter.

Folders are unpacked bundles (FbrbTool). Format notes and the checks: [FORMAT.md](FORMAT.md). Tests:
`tests\test_havok.py`, `tests\test_texture.py`, `tests\test_mesh_decode.py` (game tier: your game copy and console
files, and Battlefield 1943 in the Rising Sun workspace; skipped without them).
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
