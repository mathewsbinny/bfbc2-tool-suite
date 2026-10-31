# Shader tools

`shaderdb.py` reads, checks, lists and merges Battlefield: Bad Company 2's per-level compiled shader databases:
`Levels/<LV>/Shaders/Dx9_Single.res`, `Dx10_Single.res` and `Dx11_Single.res` in each level bundle (FbrbTool types
`Dx9ShaderDatabase`, `Dx10ShaderDatabase`, `Dx11ShaderDatabase`). The format is in [FORMAT.md](FORMAT.md).

Why: a level's database only has entries for the meshes that level places, each keyed by
`<mesh path>/<shader path>`. A mesh with no entry draws invisibly. When a level is given objects its database was
not built for (an Onslaught map made from PC Rush data has capture flags, other vehicles, other kits), the entries
can be taken from other levels' databases: **merge** builds a database that is one level's plus the entries other
levels have for the meshes it lacks.

Python 3.10 or newer, nothing else. It reads your own game copy (straight from its bundles, or files you unpacked
with FbrbTool) and writes only the file you name; it never writes into the game folder.

## Commands

```
python shaderdb.py info  level:mp_008cq
python shaderdb.py list  "C:\...\Levels\ONS_MP_008\Shaders\Dx9_Single.res" flag_
python shaderdb.py cover "C:\...\Levels\ONS_MP_008\Shaders\Dx9_Single.res" "C:\...\levels_ons_mp_008_level-00"
python shaderdb.py needs "C:\...\levels_ons_mp_008_level-00" -o needs.txt
python shaderdb.py find  needs.txt --base level:ons_mp_008
python shaderdb.py merge level:ons_mp_008 level:mp_008cq level:mp_001 --need needs.txt -o "C:\...\work\shaderdb\ons_mp_008\Dx9_Single.res"
python shaderdb.py --dx 10 merge level:ons_mp_008 level:mp_008cq level:mp_001 --need needs.txt -o "...\Dx10_Single.res"
python shaderdb.py roundtrip --levels
```

| Command | Does |
|---|---|
| `info <db>` | section counts, the consistency checks (FORMAT.md section 4), and whether writing the file again gives the same bytes; exit code 1 if not |
| `list <db> [filter] [--parts]` | the entries as `mesh \| shader` with their program count and shader bytes; `--parts` the mesh-part names and their vertex layouts |
| `cover <db> <level folder> [--all]` | the meshes the level's `.dbx` files use that have no entry; references it cannot classify are listed apart |
| `needs <level folder> [-o file]` | those meshes, one per line: the list `merge --need` takes |
| `find <needs> [--base <db>]` | for each needed mesh (with `--base`, each the base lacks), the game copy's levels whose database has entries for it |
| `merge <base> <donor>... --need <needs> -o <out> [--all-names]` | the merged database and a report `<out>.merge.txt` |
| `roundtrip <db>... [--levels]` | parse and write again, byte for byte, with the checks; `--levels` does every database in the game copy |

A database (`<db>`, base, donor) is given as:

| You give | Read from |
|---|---|
| a path to a `.res` | that file (unpacked with `FbrbTool unpack`) |
| `level:mp_008cq` | the game copy's `levels\mp_008cq\level-00.fbrb` (Package over Dist), its own `Levels/MP_008CQ/Shaders/` database |
| `<bundle.fbrb>[:<entry>]` | any bundle; name the entry when it has several databases of the kind |

The kind (Dx9 / Dx10 / Dx11) comes from the file name, else `--dx` (default 9; donors take the base's kind).
Project OPFOR BFBC2's folder is found as the other tools find it (`--opfor`, `OPFOR_ROOT`, or a
`project-opfor-bfbc2` folder next to the suite).

### The needs list

`--need` takes a text file (one path per line, `#` comments) or an unpacked level folder. A line is a mesh path or
a folder: it matches every entry whose name starts with it as a whole path (`objects/vehicles/land/bmd3` covers
`BMD3_Mesh`, `BMD3_TOW_Mesh`, `BMD3Tracks_Mesh`). Case does not matter.

From a level folder, a mesh is a partition there whose primary instance is a mesh asset, or a partition named by a
field with "Mesh" in its name (`Mesh`, `Meshes1p`, `Meshes3p`, `WeaponMesh`, `MeshShaderSet` ...) - when it is
outside the folder, only if its name has "mesh" in it (others, such as edge models and shader sets, are listed by
`cover` as not checked). Meshes that come from shared bundles and that the level folder never names (kits,
vehicles spawned by shared blueprints) must be added to the list by hand. Mesh assets that only hold shader sets
(the weapons' `KitUpgrade_01B`, LodCount 0) show as missing in every level: their shaders are compiled under the
weapon meshes they are applied to.

### What merge does

- The base is kept entry for entry.
- For each needed name the base has **no** entry for, every donor entry of it is added - all its shader sets, since
  a level picks one (Nelson Bay draws the quad bike with `Quadbike_Main_Snow`; one level has the Bradley's
  `Main_Desert`, another its `Main_Snow`). An entry name more than one donor has is taken from the **first** in the
  order given (the same name had the same content in every pair of levels compared). Meshes the base has entries
  for are left alone; `--all-names` also adds their missing shader sets.
- An entry comes with everything it draws with: its programs, vertex and pixel shaders, parameter blocks, binding
  and register tables, fragment names, vertex layouts and its mesh's mesh-part names. A record already in the file
  with the same bytes is shared instead of copied.
- The result is read back and must write the same bytes, pass every check, and each entry must draw with exactly
  what it drew with in its source (by content). Otherwise nothing is written.
- The same inputs give the same bytes.

The report lists the entries added (and from which donor), programs shared, any conflicts (a donor program or
mesh-part name the file has with other content: the existing one is kept), and the needed names still without an
entry.

### Cloning an entry for a new mesh (from Python)

`Merger.clone_entry(donor, entry, new_name, textures)` adds a donor's entry again under a new name - a mesh the
database has never drawn, e.g. a mesh converted from the console (OPFOR's `console_markv.py`) or a new weapon
variant: its programs get new ids (MD5 of the new name and the old id), their keys name the new entry, and copies of
their parameter blocks carry the new texture names (`textures`: {old name: new name}). Texture names are replaced only
as whole names at the start of their field, within the field's 128 bytes; every name to replace must be one the
entry uses (exact case) - otherwise nothing is added and the error names it. `Merger.add_part(name, layout_guid,
donor)` names a mesh part's vertex layout (`<mesh>_lod<N>/<part>/<technique>`). A plain database file's kind comes
from its name (`Dx9`, `Dx10`, `Dx11`); a contradicting `--dx` or base kind is refused, not merged (Dx10 and Dx11
share a layout and would parse either way).

### Putting a merged database into the game

Not done by the tool. Copy the level bundle out of the game folder, `FbrbTool unpack` it, replace the three
`Levels/<LV>/Shaders/Dx*_Single.res`, `FbrbTool pack` it and install it the way OPFOR installs bundles. Build all
three kinds from the same donors so every renderer has the same entries.

Things the tool cannot check (FORMAT.md section 7): engine limits other than the 65,535 programs an entry group can
index; and the textures an added entry's parameter blocks name must exist in the level (they normally come with the
mesh's own bundle). No merged database has been loaded in the game yet.

## Tests

`tests\test_shaderdb.py` (in `run_tests.py quick`): databases built by the test, Dx9 and Dx10/11 - round trip,
refused and broken files, merges (which donor wins, `--all-names`, shared records, renumbering, re-parse, content
check, determinism, the program limit) and the command line. `--game` adds the game copy: mp_008cq's and
ons_mp_008's databases round trip, and ONS_MP_008 merged with mp_008cq's capture flags in all three kinds.
