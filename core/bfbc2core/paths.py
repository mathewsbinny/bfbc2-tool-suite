r"""Where things are: Project OPFOR BFBC2 and its game copy, the suite, the suite's built tools and the sibling projects.

The one implementation of the suite's path discovery (ARCHITECTURE_AUDIT 4.4, step M1). The old locators stay, with
their names, arguments, return values, exceptions and messages, as thin wrappers over this module:

    navkit.find_opfor / find_tool       given, OPFOR_ROOT, walk up from the suite        NavKitError
    menucommon.find_opfor / find_exe    given, OPFOR_ROOT, walk up from the suite        MenuToolError
    shaderdb.find_opfor                 given, OPFOR_ROOT, beside the suite only         ShaderDbError
    convert.default_exe                 OPFOR_ROOT, beside the suite; needs the exe      FileNotFoundError
    crashreader.find_exe                OPFOR_ROOT, walk up from crash-reader's parent   None
    detailed.opfor_root                 OPFOR_ROOT as given (unchecked), else walk up    GameCopyNotFound
    bf1943import.default_source         BF1943_XENON first, else beside the suite        (no check)
    bf1943import.fbrb1943               beside the suite FIRST, then BF1943_SUITE_FBRB    Bf1943ImportError
    pipeline.Paths, level.load          as navkit.find_opfor                             GameCopyNotFound
    level.heightfield_of (FbrbTool)     as navkit.find_tool                              ToolNotFound
    navparity / navvehicle.opfor_root, harness.opfor, run_tests.opfor: wrappers over navkit.find_opfor (NavKitError)

    import os, sys; sys.path.insert(0, r'<suite>\core')
    from bfbc2core import paths
    root = paths.find_opfor()                 # Project OPFOR BFBC2's folder (it has a game\ folder)
    exe = paths.game_exe(root)                # <OPFOR>\game\BFBC2Game.exe
    xenon = paths.sibling('operation-rising-sun', 'work', 'extracted', 'dist', 'xenon', env='BF1943_XENON')

**The search orders differ between the old locators, and each one keeps its own** (the differences were found in
M1 and preserved, not unified; a later step can unify them on purpose):
- `walk=True` (navkit, menucommon, crashreader, detailed, pipeline, level): a "project-opfor-bfbc2" folder in the
  starting folder or any folder above it. Starting from the suite root, the first candidate is *inside* the suite,
  the second beside it, then beside each folder further up. crashreader starts one level lower (its parent, the
  harness folder), so its first candidate is inside the harness folder.
- `walk=False` (shaderdb, convert): only the folder beside the suite.
- The given folder and OPFOR_ROOT come first, in that order, when a locator takes them (convert and crashreader
  take no given folder). An empty OPFOR_ROOT counts as unset everywhere. A candidate counts when it has a game\
  folder (`find_opfor`), or game\BFBC2Game.exe (`find_game_exe`: convert and crashreader).
- detailed.py uses OPFOR_ROOT as it is, without checking it has a game\ folder (it reads only work\); it used to
  fall back to a hard-coded C:\Programming\Projects\project-opfor-bfbc2 and now searches.
- The BF1943 sibling folders: default_source takes BF1943_XENON before the folder beside the suite and does not
  check that it exists; fbrb1943 takes the folder beside the suite first and BF1943_SUITE_FBRB only after it, and
  needs fbrb1943.py in it. The tests that read BF1943 data keep their own orders (some honour BF1943_XENON, none
  BF1943_SUITE_FBRB).
- The game-data tests keep theirs too: test_audio / test_soot / test_terrain / test_dbxport take the folder beside
  the suite and ignore OPFOR_ROOT; test_havok / test_texture take OPFOR_ROOT unchecked, else beside;
  test_granny_port / test_mesh_decode take OPFOR_ROOT, else beside, each only with a game\ folder.
- Paths are made absolute with os.path.abspath, never resolved (no realpath): a suite reached through a junction or
  a symbolic link looks beside and above the link, as before.
- The suite never reads OPFOR_TOOLKIT: that is the OPFOR project's name for the folder holding the suite.

Only os and sys: importable with nothing but core\ on sys.path (the OPFOR project's scripts import the suite's
modules that way).
"""
import os
import sys

OPFOR_DIR = 'project-opfor-bfbc2'          # Project OPFOR BFBC2's folder name, beside the suite
OPFOR_ENV = 'OPFOR_ROOT'                   # names Project OPFOR BFBC2's folder
GAME_DIR = 'game'                          # the game copy, <OPFOR>\game
GAME_EXE = 'BFBC2Game.exe'
BF1943_SUITE = 'bf1943-tool-suite'         # the BF1943 tool suite (its bundle reader: bf1943-fbrb-tool\fbrb1943.py)
BF1943_WORKSPACE = 'operation-rising-sun'  # Project Rising Sun: BF1943's extracted bundles (work\extracted\dist\xenon)

SUITE = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))   # core\bfbc2core\paths.py


class PathsError(Exception):
    """Something the suite looks for is not where it looked."""


class GameCopyNotFound(PathsError):
    """No Project OPFOR BFBC2 folder with a game copy. errors.explain recognises it by type."""

    def __init__(self, message: str, looked_in: list[str]) -> None:
        super().__init__(message)
        self.looked_in = looked_in        # every candidate folder, in the order tried


class ToolNotFound(PathsError):
    """A tool the suite builds (FbrbTool.exe, DbxEdit.exe, ...) is neither built nor in OPFOR's tools\\ folder."""


def suite_root() -> str:
    """The suite's root folder (the one holding core\\), as this file sees it."""
    return SUITE


def walk_up(start: str, name: str = OPFOR_DIR) -> list[str]:
    """<start>\\<name>, then <each folder above start>\\<name>, up to the drive's root."""
    d = os.path.abspath(start)
    out = [os.path.join(d, name)]
    while os.path.dirname(d) != d:
        d = os.path.dirname(d)
        out.append(os.path.join(d, name))
    return out


def beside(folder: str, name: str) -> str:
    """<the folder that holds folder>\\<name>."""
    return os.path.join(os.path.dirname(folder), name)


def opfor_candidates(given: str | None = None, *, walk: bool = True, start: str | None = None,
                     suite: str | None = None, env: bool = True) -> list[str]:
    """The folders tried for Project OPFOR BFBC2, in order, without empty ones: given, OPFOR_ROOT (env=True), then
    walk=True: start (default: the suite) and every folder above it; walk=False: the folder beside the suite."""
    out = [given, os.environ.get(OPFOR_ENV) if env else None]
    if walk:
        out += walk_up(start or suite or SUITE)
    else:
        out.append(beside(suite or SUITE, OPFOR_DIR))
    return [c for c in out if c]


def find_opfor(given: str | None = None, *, walk: bool = True, start: str | None = None,
               suite: str | None = None) -> str:
    """Project OPFOR BFBC2's folder, absolute: the first candidate (opfor_candidates) with a game\\ folder.
    GameCopyNotFound names every folder tried."""
    looked = opfor_candidates(given, walk=walk, start=start, suite=suite)
    for c in looked:
        if os.path.isdir(os.path.join(c, GAME_DIR)):
            return os.path.abspath(c)
    raise GameCopyNotFound('Project OPFOR BFBC2 not found - pass --opfor <folder> or set OPFOR_ROOT '
                           f'(looked for a game\\ folder in: {", ".join(looked)})', looked)


def game_dir(opfor: str) -> str:
    """The game copy: <OPFOR>\\game."""
    return os.path.join(opfor, GAME_DIR)


def game_exe(opfor: str) -> str:
    """The game copy's executable: <OPFOR>\\game\\BFBC2Game.exe (whether it exists or not)."""
    return os.path.join(opfor, GAME_DIR, GAME_EXE)


def find_game_exe(*, walk: bool = True, start: str | None = None, suite: str | None = None) -> str | None:
    """The first <candidate>\\game\\BFBC2Game.exe that exists, OPFOR_ROOT's first (no given folder), or None. The path
    is joined, not made absolute (a relative OPFOR_ROOT gives a relative path, as the old locators did)."""
    for c in opfor_candidates(None, walk=walk, start=start, suite=suite):
        if os.path.isfile(game_exe(c)):
            return game_exe(c)
    return None


def suite_tool(toolkit_dir: str, exe: str, suite: str | None = None) -> str:
    """A tool the suite builds, where its build puts it: <suite>\\<toolkit_dir>\\<exe> (whether it exists or not)."""
    return os.path.join(suite or SUITE, toolkit_dir, exe)


def find_tool(opfor: str, exe: str, toolkit_dir: str, suite: str | None = None) -> str:
    """A built tool: in the suite (<suite>\\<toolkit_dir>\\<exe>), else OPFOR's tools\\ folder."""
    for p in (suite_tool(toolkit_dir, exe, suite), os.path.join(opfor, 'tools', exe)):
        if os.path.isfile(p):
            return p
    raise ToolNotFound(f'{exe} not found - run build.bat in the toolkit\'s {toolkit_dir} folder')


def sibling(name: str, *sub: str, env: str | None = None, suite: str | None = None) -> str:
    """A project beside the suite: the environment variable env when it is set (as it is, unchecked), else
    <the folder holding the suite>\\<name>\\<sub...> (whether it exists or not)."""
    if env:
        v = os.environ.get(env)
        if v:
            return v
    return os.path.join(os.path.dirname(suite or SUITE), name, *sub)


def find_sibling(name: str, *sub: str, marker: str, env: str | None = None, env_first: bool = False,
                 suite: str | None = None) -> str | None:
    """The first of <beside the suite>\\<name>\\<sub...> and the environment variable env that holds the file marker;
    env_first=False tries the folder beside the suite first (bf1943import.fbrb1943's order). None: neither does."""
    here = os.path.join(os.path.dirname(suite or SUITE), name, *sub)
    there = os.environ.get(env, '') if env else ''
    for p in ((there, here) if env_first else (here, there)):
        if p and os.path.isfile(os.path.join(p, marker)):
            return p
    return None


def use_folder(folder: str) -> None:
    """A folder outside the suite (a sibling project's) first on sys.path, once: no growth when called again."""
    if folder not in sys.path:
        sys.path.insert(0, folder)
