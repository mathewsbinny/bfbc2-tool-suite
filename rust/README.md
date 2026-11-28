# rust/: the suite's Rust core

The Cargo workspace behind the suite's three binaries and its Python module:

- **`bin\bfbc2.exe`:** the tools' command lines (crate `bc2-cli`).
- **`bin\bfbc2-editor.exe`:** the map editor (`bc2-editor`).
- **`bin\mapexport.exe`:** the map export (`bc2-levelwrite`).
- **`core\bfbc2core\bc2_native.pyd`:** what the Python tools call (`bc2-py`).

How to get them or build them, and which environment variables steer them, is in the suite's `README.md`, "The Rust core" and "bfbc2.exe". The map editor is in `bfbc2-map-editor/README.md`.

This file is the crate index. The normative format and transform rules the crates implement are in `spec/`. Each round's measurements and parity proofs are in `BENCHMARKS.md`; the older rounds are in `BENCHMARKS_ARCHIVE_1.md`.

## The crates

Listed bottom-up: a crate depends only on crates above it in its group's order. The descriptions are the crates' own `Cargo.toml` descriptions.

| Crate | Rust name | What it is | Depends on (workspace) |
|---|---|---|---|
| `bc2-formats` | `bc2_formats` | The game's file formats: DBX, FBRB, Granny, Havok, APT, HPD, shader databases, terrain and water, the Xbox containers, and more (`src/`, one module per format) | none |
| `bc2-pyrt` | `bc2_pyrt` | What the Python command lines showed of the Python runtime: exception texts, `repr` / format, ntpath. It is kept only so the Rust ports answer byte for byte, and it goes once the Python exit (PX1-PX7) is done | none |
| `bc2-shader` | `bc2_shader` | The Xbox 360 (Xenos) shader translator: microcode decoding, and HLSL for BFBC2's Dx10 / Dx11 (`bfbc2-shader-tools/XENOS.md`) | none |
| `bc2-xma` | `bc2_xma` | The XMA2 sound decoder, FFmpeg's ported to Rust. **LGPL-2.1-or-later**; the rest of the suite is MIT (`bc2-xma/README.md`) | none |
| `bc2-gamefs` | `bc2_gamefs` | The game layer over the formats: a game copy found and written safely (atomic writes, the install ledger), the bundle cache and the caches' budget, the mesh index | formats |
| `bc2-tools` | `bc2_tools` | FbrbTool's and DbxEdit's commands in process (`fbrbtool.rs`, `dbxedit.rs`, `unpack.rs`, `leveldiff.rs`) | formats, gamefs |
| `bc2-scene` | `bc2_scene` | A level as the tools see it: entities, collision, terrain, water | formats, gamefs |
| `bc2-terragen` | `bc2_terragen` | A new terrain from a seed (`spec/transforms/terrain_gen.md`) | formats |
| `bc2-harness` | `bc2_harness` | The map and bot test harness and the crash reader's tools | formats |
| `bc2-stockmp` | `bc2_stockmp` | The stock-MP AI layer: the level model, the authoring, the offline checks | formats, gamefs |
| `bc2-render` | `bc2_render` | The editor's game-accurate view, drawn with DICE's own Dx11 shaders | formats, gamefs |
| `bc2-convert` | `bc2_convert` | The converters: console to PC (textures, meshes, Havok, audio), the BF1943 ports, pictures | formats, gamefs, shader, xma |
| `bc2-navbuild` | `bc2_navbuild` | NavKit's path-data generator: sampling, walks, areas, graphs, the file | formats, tools, gamefs, shader, scene |
| `bc2-menu` | `bc2_menu` | The menu tools' library: APT model, grafting, the menu art | formats, convert, gamefs, pyrt, tools |
| `bc2-install` | `bc2_install` | The game-copy installer on the install ledger: Project OPFOR's WW2 install and the map export's install (`spec/transforms/install.md`) | formats, gamefs, tools, convert, menu, pyrt |
| `bc2-levelwrite` | `bc2_levelwrite` | The level writer and the map project (the operation log, `spec/transforms/export.md`, `projectlog.md`); the `mapexport` binary | formats, tools, gamefs, install, pyrt, convert, scene, navbuild, terragen |
| `bc2-editor` | `bc2_editor` | The map editor application (`bfbc2-map-editor/`) | formats, tools, gamefs, scene, levelwrite, install, terragen, navbuild, render, convert |
| `bc2-cli` | `bc2_cli` | `bfbc2.exe`: one binary with the tools as subcommands (`bfbc2.exe` prints the list) | most of the above |
| `bc2-py` | `bc2_native` | The Python module the suite's Python tools import | most of the above |

## The scripts in this folder

| Script | Does |
|---|---|
| `build_native.py` | builds or downloads the module and the binaries (`--download`, `--editor`, `--if-stale`; the suite's README) |
| `python_status.py` | writes `PYTHON_STATUS.md` and `python_status.json` at the suite's root: every Python file and its port status |
| `native_census.py` | who calls each function and class `bc2_native` exports (`native_census.json`) |
| `hang_stacks.py` | where each thread of a hung process waits (`python rust\hang_stacks.py <pid> [dump file]`) |
| `bench_*.py` | the rounds' timings. Each script's docstring names its `BENCHMARKS.md` section; the older rounds' sections (up to about 2026-11-22) moved to `BENCHMARKS_ARCHIVE_1.md` |
| `parity_*.py` | the rounds' parity checks on real data (NK1-NK6, E3o, OR5, OR14): the Rust ports against the Python they replaced |

The tests are each crate's `tests/` folder and its `#[cfg(test)]` modules (`cargo test -p <crate>`); the suite's README, "Tests", has the tiers. `rustfmt.toml`, `clippy.toml` and `rust-toolchain.toml` hold the house style and the toolchain the checks workflow uses.
