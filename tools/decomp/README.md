# tools/decomp: the decompiled database of BFBC2Game.exe

Round DC0 (2026-11-27). A Ghidra database of `BFBC2Game.exe` (PC, build 795745, 32-bit x86, 19,322,816 bytes),
with the spec's types and names applied, every function decompiled to pseudo-C, and an index to query it.

**The database is private and EA-derived.** It lives outside every repo, in
`C:\Programming\Projects\bfbc2-decomp-private\` (or `BFBC2_DECOMP_DB`): the Ghidra project, the pseudo-C, the index.
Only the scripts in this folder are committed. Never copy pseudo-C, strings or Ghidra output into a repo; cite
**addresses** and rule ids instead (spec/README.md, "The decompiled database").

## Looking things up (`decomp.py`)

```
python tools\decomp\decomp.py 0x5B33C0            the function containing the address: facts, then its pseudo-C
python tools\decomp\decomp.py Session::setState   a function by name (also FUN_00557150)
python tools\decomp\decomp.py --info 5B33C0       facts only: range, signature, class, callers, callees, strings, citing rules
python tools\decomp\decomp.py --callers 5B33C0    callers (--callees: callees), one per line
python tools\decomp\decomp.py --name Soldier      functions whose name or class contains the text
python tools\decomp\decomp.py --string GameSettings   functions referencing a string containing the text
python tools\decomp\decomp.py --rule AI-START-2   the addresses a spec rule cites and their functions
python tools\decomp\decomp.py --stats             the database's counts (functions, coverage, gaps)
```

Addresses are hex, with or without `0x`. `--db <folder>` picks another database; `--limit N` caps the lists.

## Names in the database

- **Functions** keep Ghidra's `FUN_<address>` unless something names them: the exe's exports (`Apt*`), MSVC RTTI
  (Kynapse, `Kaim::`), Frostbite's reflection (below), or the spec / OPFOR names (`[DC0]` comments).
- **Classes**: a function sits in a class when it is in exactly one class's vtable (or in several, of which one is
  the ancestor of the rest by typeinfo.json's parents), or when it stores only that class's vtable (constructors,
  destructors). Vtables come from MSVC RTTI and from the reflection's `getType` (`mov eax, <class info>; ret`, in
  every reflected class's vtable; RTTI-REG-1, RTTI-FAC-1).
- **Types**: each typeinfo.json record is a structure (members at their offsets) or enum; a class's functions are
  `__thiscall` with `this` typed as its structure.
- **Spec names**: a declared name (`Class::method`, backticked, a table row, a constant) renames the function;
  words picked from a sentence give `spec_<words>`. Every cited address carries a `[DC0]` comment naming its rules
  and sources.

## Building or refreshing it (`run_dc.py`)

Needs Ghidra 12.1.4 (`BFBC2_GHIDRA`, default `C:\Tools\ghidra_12.1.4_PUBLIC`) and a JDK (`JAVA_HOME`). The exe is only
read; the script refuses a database folder inside a git checkout.

```
python tools\decomp\run_dc.py all --heap 9G --threads 4     import + analyse (about 25 min), plan, apply, export, index
python tools\decomp\run_dc.py plan                          names (extract_names.py) + type plan (gen_types.py)
python tools\decomp\run_dc.py apply                         ApplyTypes.java, ApplyNames.java (saved into the project)
python tools\decomp\run_dc.py export --threads 4            ExportDb.java: pseudo-C + raw index + measure.json
python tools\decomp\run_dc.py index                         build_index.py: index\decomp.sqlite
```

After a spec change, `plan`, `apply`, `export`, `index` bring the names up to date (apply is safe to repeat).

| File | Does |
|---|---|
| `extract_names.py` | every exe address the spec (`spec/**/*.md`, with its rule id), the crash reader's symbols, OPFOR's research notes and hook tables, and an older Ghidra project name; names, conflicts |
| `gen_types.py` | `spec/typeinfo.json` as a Ghidra type plan (structures, enums, arrays) |
| `ghidra_scripts/ApplyTypes.java` | classes from RTTI and the reflection, functions into classes, the types |
| `ghidra_scripts/ApplyNames.java` | the names and `[DC0]` comments; where each address landed |
| `ghidra_scripts/ExportDb.java` | the pseudo-C (`pseudo\XXXX.c`, one file per 64 KiB), `index\functions.jsonl`, `index\measure.json` (threads 0: measure only) |
| `ghidra_scripts/ExportSymbols.java` | the user and imported names of any Ghidra project, as JSON lines |
| `build_index.py` | `index\decomp.sqlite` (functions, calls, strings, cites, meta) |
| `decomp.py` | the lookup |

The private folder's layout: `ghidra\` (the project `BFBC2`), `pseudo\`, `index\` (decomp.sqlite, names.json,
names_applied.jsonl, types_plan.json, types_report.json, measure.json), `logs\`, `README.md`.

Rulings: decompile everything, Kynapse included (the clean-room rule was removed 2026-11-27). Never analyse or work
around the FLEXnet licence check (`Kaim::CMapBuilder`'s gate) or any copy protection, and never run the licence-locked
generator.
