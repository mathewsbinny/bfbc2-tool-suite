# Text: fonts, the text databases, subtitles

**Sources.** Round SPG4 (2026-11-23): the census of DICE's partitions (`spec/census.json`, re-measured by the game
tier) and `BFBC2Game.exe`'s names. The localized strings' binary tables are `formats/loc.md`; the Flash movies'
own text fields are `formats/apt.md`. The tables list the members DICE's instances carry, less `Name`,
`MemoryReportGroup`, `Components`, `Enabled`, `Enumeration` and `Transform`.

## 1. Fonts

### TEXT-FONT-1: Fonts are pre-rendered glyph sheets per language, size and style

Status: **verified** · Variants: PC

The game draws text from bitmap fonts DICE's pipeline rendered: a `UIFontAsset` (45: a source font file, point size,
style, border, a glow effect and its blur, the text database whose characters it must cover, or
`GenerateCharacterSet`) produces `UIFontData` (56: the glyph sheet's texture file and size, the line height, border
and effect, a kerning file) with its `UIFontGlyph`s (26,596: per glyph its code, place on the sheet, size, offsets,
advance and pitch; 18,610 of them Japanese) and `UIFontGlyphKerning` pairs (1,870). The partitions are per language
under `Fonts/<lang>/` (us, de, fr, es, it, pl, ru, jp). A movie asks for a font by name: `Fonts/<lang>_Fonts` holds a
`UIFontMapAsset` per language (16, the EULA's maps separate) of `UIFontProxyAsset`s (52: a font name, the font, its
size and style, the tracking, and a signed-distance variant `FontSD` with its pixel offset). The executable's enums
`UIFontStyle` (`_Regular`, `_Bold`) and `UIFontEffect` (`_None`, `_Glow`) and the menus' `loadEulaFonts` /
`unloadEulaFonts` / `unloadStoreFonts` belong here; Windows' `CreateFontIndirectA` is used only by the debug overlay
(inferred).

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| UIFontAsset (UI) | 45 | 45 | `fonts/us`, `fonts/ru`, `fonts/jp` | BorderSize, Effect, GenerateCharacterSet, GlowBlurHeight, GlowBlurWidth, GlowBorderSize, GlowGaussianBase, PointSize, SourceFile, Style, TextDatabase |
| UIFontData (UI) | 56 | 56 | `fonts/us`, `fonts/ru`, `fonts/jp` | BorderSize, Effect, Glyphs, Height, Kerning, KerningFile, TextureFile, TextureHeight, TextureWidth |
| UIFontGlyph (UI) | 26,596 | 56 | `fonts/jp`, `fonts/us`, `fonts/ru` | Advance, Glyph, MaxX, MaxY, MinX, MinY, Pitch, Rows, Width, X, Y, YOffset |
| UIFontGlyphKerning (UI) | 1,870 | 23 | `fonts/us`, `fonts/de`, `fonts/pl` | First, Kerning, Second |
| UIFontMapAsset (UI) | 16 | 16 | `fonts/fr_eula.dbx`, `fonts/es_fonts.dbx`, `fonts/us_eula.dbx` | Language, Proxies |
| UIFontProxyAsset (UI) | 52 | 11 | `fonts/us_fonts.dbx`, `fonts/jp_fonts.dbx`, `fonts/ru_fonts.dbx` | Font, FontName, FontSD, PointSize, SDPixelOffset, Style, Tracking |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json (DICE's partitions, round SPG4, 2026-11-23): the classes above]
[exe: BFBC2Game.exe strings UIFontStyle_Regular, UIFontStyle_Bold, UIFontEffect_None, UIFontEffect_Glow, UIFontMapProxy, loadEulaFonts, unloadEulaFonts, unloadStoreFonts, CreateFontIndirectA]

### TEXT-DB-1: The text databases and the localization asset

Status: **verified** · Variants: PC

`Localization/Localization` holds the one `LocalizationAsset` (the localized texts per language, the font maps and the
EULA's font maps); the `UITextDatabase`s (23: a language, its binary file and its histogram file - the `.bin` pair
of LOC-HDR-1 and LOC-HIST-1) are six there and the rest in the per-language `Localization/<lang>_Loc` and `<lang>_Loc_Huge`
partitions (de, es, fr, it, jp, pl, ru, us). A string is found by its id's hash (LOC-KEY-1); the HUD and menu classes
name strings by `...Sid` members (`ObjectiveSid`, `TooltipSid`, `DescriptionSID` ...).

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| LocalizationAsset (UI) | 1 | 1 | `localization/localization.dbx` | EulaFontMaps, FontMaps, LocalizedTexts |
| UITextDatabase (UI) | 23 | 18 | `localization/localization.dbx`, `localization/fr_loc.dbx`, `localization/jp_loc.dbx` | BinaryFile, HistogramFile, Language |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]

### TEXT-ATL-1: UI texture atlases

Status: **verified** · Variants: PC

The menus' and HUD's textures are packed into atlases, each a `UITextureAtlasData` (52: `Infos`, the sub-images) in a
`Menu/<name>_Textures_Win32` partition (shared graphics, HUD pop-ups, the kill cam ...); the minimap's icon atlas is
built by HUD-MAP-1's manager instead.

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| UITextureAtlasData (UI) | 52 | 52 | `menu/sharedgraphics_textures_win32`, `menu/hud_popups_textures_win32`, `menu/mp_killcam_textures_win32` | Infos |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: UITextureAtlasData 52]

## 2. Subtitles

### TEXT-SUB-1: Subtitles map a voice line's wave to timed strings, per language

Status: **verified** · Variants: PC

A campaign level's subtitles are one `LevelSubtitles` with a `SubtitleLevelMap` per language (8 a level: 104 in all),
each a list of `SubtitleWaveHashPair`s (22,878: a wave's name and its hash, WAVE / SND-VO, and its subtitles) of
`SubtitleRuntimeData` (15,519: a string id, a start time and a display time). The level names the asset in
`LevelData.SubtitleAsset`; a cutscene in its `Subtitle` member. `VOFacialAnimMapData` (13, one per campaign level)
maps waves to facial animations for the same lines.

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| LevelSubtitles (GameSharedResources) | 13 | 13 | `levels/<level>` | SubtitleMaps |
| SubtitleLevelMap (GameSharedResources) | 104 | 13 | `levels/<level>` | Language, SubtitleWavePairs |
| SubtitleWaveHashPair (GameSharedResources) | 22,878 | 13 | `levels/<level>` | HashedWaveName, Subtitles, WaveName |
| SubtitleRuntimeData (GameSharedResources) | 15,519 | 13 | `levels/<level>` | DisplayTime, SID, Time |
| VOFacialAnimMapData (GameSharedResources) | 13 | 13 | `levels/<level>` | WaveAssets |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above; LevelData member SubtitleAsset; CutsceneEntityData member Subtitle]

## Open questions

- The glyph sheets' textures (whether `TextureFile` is an `ITexture` resource of the font's partition) and the
  signed-distance fonts' use.
- Which `UITextDatabase` the game picks for a language with no database of its own.
