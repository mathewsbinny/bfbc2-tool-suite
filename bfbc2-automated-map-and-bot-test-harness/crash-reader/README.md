# BC2 crash reader

When Battlefield: Bad Company 2 crashes, it writes `crashreport.xml`: a crash address, a list of raw
addresses and the CPU registers. This tool turns that into something you can act on:

- **which function crashed**, by name where it is known;
- **the code around the crash**, disassembled from your own `BFBC2Game.exe`, with a null pointer pointed out
  when the faulting instruction reads through one;
- **the call stack**, each step with its function and the call instruction that led to it. Values on the
  stack that aren't code addresses are marked.
- **hints for known crashes**, for example a level with an empty entry in its object list:
  "run `DbxEdit check`";
- **the log lines just before the crash**, if you use Project OPFOR BFBC2.

It only reads the report and the exe; nothing is sent anywhere.

## Setup

Python 3.12 or newer (the suite's). For the code view, also run `pip install pefile capstone`. Without them (and
without the Rust core, below) you still get the names, the stack and the hints.

Without Python: `bin\bfbc2.exe crashreader ...` and `bin\bfbc2.exe exeref ...` (the suite's `bin\`, round O2) take the
same arguments and print the same. Round RT3 retired the crash reader's Python command line (`crashreader.py`; its
library is `crashreaderlib.py`, which exeref.py and the tests import): `bfbc2 crashreader` is the only one, and a
report it does not read (not XML, UTF-16), an exe that is not a PE file or another exe's code the Rust decoder does
not take ends with an error naming the retirement. `exeref` keeps its Python, which answers what the port hands it
(the suite's README, "bfbc2.exe"). The suite's hub and Project OPFOR's `testrun.bat` run the binary; without it they
say how to get it (`python rust\build_native.py --download`).

With the suite's Rust core built (`python rust\build_native.py`; round E4f, `rust/bc2-harness`) the exe is mapped
and disassembled there, with capstone's own text: every byte offset of `BFBC2Game.exe`'s code (build 795745) decodes
exactly as capstone decodes it. An instruction outside that proof (another exe's rare encodings) is handed back to
pefile + capstone, so the output never differs; with the core and without pefile / capstone the code view still works.
The log lines before a crash are found there too. `BFBC2_HARNESS_PYTHON=1` runs the Python alone. Test:
`tests\test_crash_native.py` (seeded x86, built exes; `--game`: the game's exe, both command lines byte for byte).

## Use

```
bfbc2 crashreader                             the newest crashreport.xml in Documents (OPFOR's or the game's)
bfbc2 crashreader path\to\crashreport.xml
bfbc2 crashreader --exe "C:\Games\Battlefield Bad Company 2\BFBC2Game.exe"
bfbc2 crashreader --symbols my_names.txt      add your own function names and hints
bfbc2 crashreader --log path\to\opfor.log
```

It finds `BFBC2Game.exe` in Project OPFOR BFBC2's game copy (`OPFOR_ROOT`, or a `project-opfor-bfbc2` folder next to one of
the folders it sits in).
Otherwise give it with `--exe`.

Example (a ported level whose model groups lacked physics data):

```
Crash at 0x00AF7144  model-group part data access
  registers: Eax 00000000 (null)  Ebx 0667C7BC  ...
Code (function starts near 0x00AF7140):
    00AF7140  mov eax, dword ptr [esp + 4]
  > 00AF7144  mov edx, dword ptr [eax + 0x2c]
  The faulting instruction reads memory through Eax = 0x0 (+0x2C): a null pointer.
Hint: a model group or member whose PhysicsData / MeshEntityType is empty - run: DbxEdit check <level.dbx>
Call stack (innermost first; return addresses):
  0x0061BE69  part set-up helper (reads the part data's +0x8C) +0x29    <- call eax
  0x008FC292  model-group entity construction +0x32    <- call 0x61be40
```

## Where the exe uses a string or an address (exeref.py)

`exeref.py`, beside the crash reader, answers the targeted questions research keeps asking of `BFBC2Game.exe`, with the
same code view, names and symbol files:

```
python exeref.py str Server.UnrestrictedUnlocks --exact --dis    the string, each instruction using its address, a window
python exeref.py refs 0x8527D0                                    who uses an address (operands, call / jmp targets, tables)
python exeref.py dis 0x95B7D3 --before 0x10 --after 0x10          a short window of code, strings and known functions named
```

`str` finds C strings containing a text (any case; `--exact` for the whole string), then the instructions that use
the string's address and the function each is in; an address held in data (a table of pointers) is listed too. `refs`
does the same for any address, including calls to a function. Windows are capped at 0x100 bytes each way: short,
targeted disassembly, as the suite's rules allow - no bulk decompiling, nothing exported. Needs pefile and capstone.
Test: `tests\test_exeref.py` (a program built in the test; with `--game`, known sites in the game's exe).

## Names and hints

`symbols_795745.txt` lists functions and crash sites in the PC game, build 795745, that Project OPFOR BFBC2's
research identified. Format, one per line:

```
ADDRESS  name  | hint
```

The hint is optional and is shown when a crash happens there. Functions without a name are shown as
`FUN_<address>`, found by looking for the padding the compiler puts between functions. Add your own files
with `--symbols`. Contributions to the bundled list are welcome.

## Credits

Developed for Project OPFOR BFBC2. Disassembly by [Capstone](https://www.capstone-engine.org/); exe reading by
[pefile](https://github.com/erocarrera/pefile). Battlefield: Bad Company 2 is © Electronic Arts / DICE.
