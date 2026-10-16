# FbrbTool

A command-line tool to unpack and repack Battlefield: Bad Company 2's `.fbrb` bundles (Frostbite 1
archives: the game's levels, menus and shared data).

## Build

Run `build.bat`. It compiles `FbrbTool.exe` into this folder with the C# compiler that ships with
Windows (.NET Framework 4) and needs no install.

## Use

```
FbrbTool list   <file.fbrb>                list the entries
FbrbTool unpack <file.fbrb> <outdir>       write every entry under <outdir>, plus fbrb.manifest
FbrbTool pack   <dir> <file.fbrb>          rebuild a bundle from an unpacked folder
```

`unpack` writes `fbrb.manifest`, which lists the entries in archive order with their type, flags and
sizes. `pack` uses it to rebuild the bundle faithfully. To change a bundle, unpack it, edit or add files
(add new ones to the manifest), then pack it.

Zero-byte entries are `*deleted*` markers: they remove shared data from the bundle's level (mp_005's
level bundle has 4,334 of its 6,692 entries as markers; all bundles together 286,055). `unpack` lists them in
`fbrb.manifest` but writes no file for them, so other tools do not meet thousands of empty files that look
broken; `pack` restores them from the manifest, and the rebuilt bundle's header and payload are identical
to the original's. Folders unpacked by older versions, which hold the markers as empty files, pack the same.

## Credits

The format follows **Frankelstner**'s fbrb script and **Heico**'s BFBC2 Toolkit. Developed for
Project OPFOR. Battlefield: Bad Company 2 is © Electronic Arts / DICE.
