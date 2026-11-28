# BC2 NavKit

Standalone tooling to create AI navigation data (`pathdata.hpd`) for Battlefield: Bad Company 2
levels – including multiplayer maps that never shipped with any – so the game's own AI can move on
them. Developed for Project OPFOR BFBC2 and published for other modders as `bfbc2-auto-navkit`, the automated
navigation-data generator of the BFBC2 suite. It measures levels through Project OPFOR BFBC2's in-game sampler,
so it needs an OPFOR install; the level files are read in process by the suite's Rust core (`bfbc2.exe fbrbtool` /
`dbxedit` since round R4). Since 2026-10-26 the same measurements can also be computed without the game ("Standalone
sampling": `bfbc2 navkit sample <level> --offline`, the Rust core's sampler since round NKS-deep C; not in the window).

**To use it, start with [QUICKSTART.md](QUICKSTART.md).** Double-click `navkitGUI.pyw` for the window, or
run `python navkit.py` from a command prompt.

**Today (suite 0.37.0):** NavKit needs the suite's Rust core: its generator (walkability rules, areas, tracing, route
graphs, the writer) runs only in Rust (rounds E1d, R1, NK1-NK5-rest; `navbuild.py` retired), the tools around the
build (round E4e) with their Python as the fallback (`tools\navnative.py`, `BFBC2_NAVTOOLS_PYTHON=1`).
`bin\bfbc2.exe navkit ...` answers in Rust `levels`, `prepare`, `status`, `build`, `all`, `apply`, `check`, `compare`,
`install`, `restore` and `sample --offline`; only the in-game `sample` (it starts the game) is handed to `navkit.py`.
`bin\bfbc2.exe <tool> ...` is each tool's command line (`hpdtool`, `hkpack`, `levelobjects`, `ladders`, `ordercheck`,
`navcompare`, `hpdcheck`, `hpdview`, `detailed`, `navparity`, `navregress`, `sample_all`, `graphstats`; rounds O2 -
O5); for `navparity run`, `navregress`, `sample_all` and `hpdview` it checks the arguments and hands the run to the
Python (round OR16 moves navparity's level row to Rust). Python left: `PYTHON_STATUS.md`; retired tools: `RETIRED.md`.

## Why

BC2's AI uses Autodesk Kynapse. The game ships the Kynapse *runtime* (which loads `pathdata.hpd`)
under DICE's runtime licence, but the *generator* that DICE used to create that data is a separately
licensed developer tool: the copy that is still inside the game executable stops with a FLEXnet
licence error. NavKit replaces it with independent code.

## Rules (clean-room approach)

1. **Research** (`research\`): the Kynapse map builder in BFBC2Game.exe and the `.hpd` files are
   reverse engineered for interoperability. Results are written up as a *specification* – file
   formats, data structures, units, what each build stage produces. Licence checks are only located
   and documented as the boundary; they are not analysed in order to defeat them and nothing in this
   project bypasses them.
2. **Implementation** (`src\`): written from the specification with our own code and open-source
   libraries (e.g. recastnavigation, zlib licence). No Kynapse code is copied or translated
   line-by-line, no Kynapse binaries are shipped, and the tool never runs Kynapse code.
3. Game data (levels, collision meshes) always comes from the user's own installation.
4. Geometry is sampled through the **game engine's own physics queries** (DICE code, via Project
   OPFOR's `[dev] navsample` option) or read from the level's own data files – never through Kynapse.
   Observations of the original generator are limited to its boundary with DICE's code (the point
   queries it makes, the files it writes) and its output data.
5. The private folder `research\kynapse mapbuilder research\` is git-ignored and never published.

## Layout

```
QUICKSTART.md        how to build path data for a level, step by step
navkitGUI.pyw        the NavKit window (Tkinter): the steps below as buttons
navkit.py            driver: levels / prepare / sample / build / check / install / restore / status / all
research\            KYNAPSE_SPEC.md (the research record; the rules: the suite's spec\formats\hpd.md) and img\
tools\               NavKit tools (Python 3.12+, numpy / matplotlib, the suite's Rust core)
  hpdtool.py         pathdata.hpd: info / sector / roundtrip / coverage / meshtrip
  kymesh.py          per-sector runtime mesh: parse_mesh / write_mesh (byte-exact)
  hpdview.py         render a sector to PNG (outlines, portals, graph)
  navsample.py       read in-game collision samples; compare with a pathdata.hpd
  hkpack.py          Havok 5.5 binary packfile reader (the level's *_Havok.res collision assets)
  levelobjects.py    object placements, pathfinding flags, forbidden-object triangles of a level
                     (reads the level XML through core's bfbc2core/xmlinst.py: instances and field refs in any
                     attribute order - DICE's tools write guid / ref first, Napisal's type / name first)
  navwalk.py         walkability rules + scoring against existing path data; navwalk.build: the level's whole
                     build in the Rust core (rust\bc2-navbuild: build.rs, stage.rs), its settings the Rust defaults
                     (bc2_native.navkit_settings(); a caller passes what it changes, e.g. --no-detail)
  detailed.py        which sectors are detailed zones (pitch 0.2), from the level alone
  ladders.py         the level's ladders and their collision boxes, for ladder links (written by prepare)
  sample_all.py      unattended in-game sampling of several levels
  hpdcheck.py        structure of a pathdata.hpd: closed boundaries, edges, node order, D / F (as DICE's files)
  navdetail.py       D lines and F cells of an area (DICE's rule; in the Rust core since round NK3:
                     rust\bc2-navbuild\src\detail.rs; the build's is the stage's own; tests\test_detail_features.py)
  navislands.py      shadow islands: small route islands beside a larger one lose their nodes (the rule's
                     settings and terrain nodes; the rule runs in the Rust core since round NK2, in the build's
                     writer; tests\test_navislands.py; see "Route islands")
  ordercheck.py      scoring: AI order positions on the areas, pairwise route check, self-check
  navcompare.py      two files on a level's AI positions: coverage, main network, pairwise, size
  navdiff.py         two files layer by layer: sectors, walkable cells, areas, boundaries, route graph
  navdf.py           two files' D lines and F cells: counts, precision / recall per pitch (navparity adds them)
  navparity.py       DICE-parity report over many levels, saved per named run
  navregress.py      regression test: rebuild levels with the current code, fail if a parity measure got worse
  navwater.py        water depth against path-data coverage (does the data leave out ground under water?)
  navboundary.py     area boundaries cell by cell against DICE's: matching / missing / extra edges, merged areas, portals; CSV + maps
  navedits.py        the walkability edits of a navigation side file as generator inputs (navkit.py apply)
  navvehicle.py      land-vehicle path data (vehicle\pathdata.hpd): build / compare / parity / info (see "Vehicle path data")
  navwater_mesh.py   boat navigation mesh on a level's water, a side file for OPFOR's bot layer (prototype)
  navdomains.py      domain graphs and a route planner across foot / land vehicle / boat / aircraft (prototype;
                     research\CROSS_DOMAIN_ROUTES.md)
  navsample_offline.py  the in-game sampler's output computed from the level's Havok collision and height field, no
                     game launch; the stock MP batch (see "Standalone sampling")
  navnative.py       the Rust core's switch for the tools around the build (round E4e; BFBC2_NAVTOOLS_PYTHON=1)
  (graphstats.py     retired 2026-11-20, round RT1: bfbc2.exe graphstats)
  (navbuild.py       retired 2026-11-22, round NK5-rest: the generator - samples + level objects -> pathdata.hpd - is
                     the Rust core's since round NK5 (navwalk.build calls it); its settings and switches, which the
                     notes below call navbuild.NAME, are the Rust BuildSettings / Switches fields of the lower-case
                     name (rust\bc2-navbuild\src\stage.rs); its tests are Rust tests (stage_tests.rs) and
                     tests\test_navbuild_native.py)
  (navlinks.py, navtraversal.py  retired 2026-11-21, round NK1: navkit apply's links, route edges and traversal
                     annotations are the Rust core's - rust\bc2-navbuild\src\graphedit.rs; navkit.graph_edits /
                     write_traversal call it, bfbc2.exe navkit apply answers a DICE base itself)
tests\               NavKit's tests (run by the suite's run_tests.py; the suite README lists them)
src\                 (reserved, empty)
```

## Research and status

The README's research and status sections (2026-10-21 to 2026-11-28) moved to battlefield-opfor's knowledge base in
round KB2 (2026-11-28): `bfo:docs/research/navigation/NAVKIT_README_RESEARCH.md` (below: "the KB's copy"). Their
headings stay here so the references to them still lead somewhere; the normative rules are `spec/formats/hpd.md`,
`spec/transforms/kynapse_generation.md` and `spec/transforms/navkit.md`.

### Standalone sampling (2026-10-26)

`tools\navsample_offline.py` and `bfbc2 navkit sample <level> --offline` (the Rust core's sampler since round
NKS-deep C): the in-game sampler's files from the level's collision and height field, no game launch. The KB's copy.

### Vehicle path data (2026-10-26)

`tools\navvehicle.py`: land-vehicle path data (`vehicle\pathdata.hpd`), build, compare and parity. The KB's copy.

### Route islands (2026-10-26)

The shadow-island rule (`tools\navislands.py`, in the Rust core since round NK2) and what it fixed. The KB's copy.

### D lines and F cells (2026-10-26)

DICE's detail features and the generator rule that makes them (`tools\navdetail.py`, `navdf.py`). The KB's copy.

### Detailed zones, ladders and walkability rules (2026-10-21 / 22)

`tools\detailed.py`, `ladders.py` and the walkability rules measured against DICE's data. The KB's copy.

### Plan and status (2026-10-26)

The step-by-step plan with its history and the next steps (a play test on ONS_MP_002 first): the KB's copy. The live
queue is `ROADMAP.md` (NKS-deep, NKM).

Credits: level data is read with the suite's FbrbTool and DbxEdit, based on **Frankelstner**'s format
documentation and **Heico**'s BFBC2 Toolkit. **Napisal**'s path data for multiplayer maps (BBC2 ModPack)
showed that BC2's AI can run on them. Kynapse is a trademark of Autodesk; Battlefield: Bad Company 2 is
© Electronic Arts / DICE.
