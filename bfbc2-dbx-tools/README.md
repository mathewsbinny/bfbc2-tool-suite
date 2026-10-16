# DBX tools

**DbxEdit** reads, converts and edits Battlefield: Bad Company 2's binary `.dbx` files (Frostbite 1 data
as a tokenised XML tree). The format is the same on PC and Xbox 360.

- `DbxEdit xml` converts binary `.dbx` files to readable XML (`<name>.xml` next to each file).
- `DbxEdit edit` edits binary `.dbx` files directly. Anything it does not change is written back byte for
  byte, so unchanged files stay identical.

[FORMAT.md](FORMAT.md) describes the format and everything DbxEdit outputs; the code is written from it.

## Build

Run `build.bat`. It compiles `DbxEdit.exe` into this folder with the C# compiler that ships with Windows
(.NET Framework 4) and needs no install.

## Convert to XML

```
DbxEdit xml <file.dbx | folder> [more ...]
```

It converts every binary `.dbx` file it finds (folders are searched recursively) and skips files that
are already plain XML.

## Edit

```
DbxEdit roundtrip <file.dbx | folder>         parse and rewrite every file, report any that change
DbxEdit edit <in.dbx> <out.dbx> <command> ...  apply commands in order, write the result
```

Commands (a `@file` argument reads more commands from a file, one per line; `#` starts a comment):

| Command | What it does |
|---|---|
| `replace=<old>\|<new>` | every string exactly `<old>` becomes `<new>` (re-points references) |
| `replaceprefix=<old>\|<new>[\|<except>...]` | strings starting with `<old>` get `<new>` as prefix |
| `drop=<guid>` | removes that instance; array items pointing to it go, other references become `null` |
| `dropfield=<name>` | removes every field called `<name>` |
| `graft=<src.dbx>\|<guid>[\|<before guid>]` | copies an instance from another file, with every instance of that file it references |
| `copyfield=<src.dbx>\|<src guid>\|<field>\|<guid>` | copies one field (arrays included) from another file's instance |
| `additem=<guid>\|<array>\|<ref>[\|<before ref>]` | adds a reference to an array field of an instance |
| `setfield=<guid>\|<field>\|<value>` | sets a text field or a single 4-byte number of an instance |
| `set=<field>\|<true\|false\|0-255>` | sets every bool / byte field called `<field>` |
| `partguid=<guid>` | sets the partition's own GUID (for a copy under a new name) |

The exact rules for each command are in [FORMAT.md](FORMAT.md) section 5.

Example: re-point a level's object list from a missing instance to another one, then check the result:

```
DbxEdit edit Level.dbx Level_fixed.dbx replace=77bb70bd-4472-466b-ac88-b3510bd55f8e|48a152fe-b52e-4f5c-9f06-1c26c8d8df2d
DbxEdit xml Level_fixed.dbx
```

## Tested

On 43,290 unique `.dbx` files (PC, Xbox 360 and Linux server data, plus edited levels):

- `roundtrip`: every binary file comes back byte for byte.
- `xml`: output byte-identical to the earlier DbxTool (a port of Frankelstner's `dbx.py`), which this
  replaces.
- `edit`: every command, alone and combined, on 12 levels, plus a real 9,000-command level merge:
  output, messages and exit codes identical to the earlier DbxEdit.

## Credits

The binary DBX format was worked out by the BC2 modding community, above all by **Frankelstner** (his
`dbx.py` scripts); **Heico**'s BFBC2 Toolkit was a format reference. DbxEdit's code is this project's
own, written from [FORMAT.md](FORMAT.md). Developed for Project OPFOR. Battlefield: Bad Company 2 is
© Electronic Arts / DICE.
