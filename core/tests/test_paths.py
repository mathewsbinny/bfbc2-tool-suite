r"""bfbc2core.paths on a stand-in folder tree (quick tier, no game data): where the suite finds Project OPFOR BFBC2,
its game copy, the suite's built tools and the sibling projects.

- OPFOR beside the suite; inside the suite and further up (walk=True) or beside only (walk=False);
- the given folder before OPFOR_ROOT before the search, a candidate without game\ skipped, an empty OPFOR_ROOT unset;
- no game copy: GameCopyNotFound naming every folder tried, in NavKit's words, explained by errors.explain by type;
- the game executable (find_game_exe: the exe must exist; a relative OPFOR_ROOT stays relative);
- the built tools (the suite's build before OPFOR's tools\, else ToolNotFound);
- siblings: present, missing, overridden by their environment variable, in either order; sys.path not grown;
- relative given folders and OPFOR_ROOT made absolute; a suite reached through a junction looks beside the junction
  (paths are never resolved);
- the module imports with nothing but core\ on sys.path, and needs only os and sys.

  python core\tests\test_paths.py
"""
import ast
import os
import shutil
import subprocess
import sys
import tempfile
from collections.abc import Callable, Iterator
from contextlib import contextmanager

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import errors, paths  # noqa: E402

ENVS = (paths.OPFOR_ENV, 'BF1943_XENON', 'BF1943_SUITE_FBRB')


def touch(p: str, data: bytes = b'') -> str:
    os.makedirs(os.path.dirname(p), exist_ok=True)
    with open(p, 'wb') as f:
        f.write(data)
    return p


def opfor_at(folder: str, exe: bool = True) -> str:
    """A stand-in Project OPFOR BFBC2 folder: game\\ (with BFBC2Game.exe when exe)."""
    os.makedirs(os.path.join(folder, 'game'), exist_ok=True)
    if exe:
        touch(os.path.join(folder, 'game', 'BFBC2Game.exe'), b'MZ')
    return folder


@contextmanager
def env(**values: str | None) -> Iterator[None]:
    saved = {k: os.environ.get(k) for k in values}
    try:
        for k, v in values.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
        yield
    finally:
        for k, old in saved.items():
            if old is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = old


def raised(fn: Callable[[], object]) -> BaseException | None:
    try:
        fn()
    except Exception as e:
        return e
    return None


def same(a: str | None, b: str | None) -> bool:
    return a is not None and b is not None and os.path.normcase(a) == os.path.normcase(b)


def junction(link: str, target: str) -> bool:
    """A directory junction (Windows; no rights needed) or a symbolic link elsewhere. False: none could be made."""
    if os.name == 'nt':
        r = subprocess.run(['cmd', '/c', 'mklink', '/J', link, target], capture_output=True, text=True)
        return r.returncode == 0 and os.path.isdir(link)
    try:
        os.symlink(target, link, target_is_directory=True)
    except OSError:
        return False
    return True


def main(argv: list[str]) -> int:
    failed = 0

    def check(ok: bool, what: str, detail: str = '') -> None:
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''))

    saved_env = {k: os.environ.pop(k, None) for k in ENVS}       # the machine's own settings out of the way
    saved_path = list(sys.path)
    cwd = os.getcwd()
    tmp = os.path.realpath(tempfile.mkdtemp(prefix='core_paths_'))
    try:
        projects = os.path.join(tmp, 'Projects')
        suite = os.path.join(projects, 'bfbc2-tool-suite')
        os.makedirs(os.path.join(suite, 'core', 'bfbc2core'))
        opfor = opfor_at(os.path.join(projects, 'project-opfor-bfbc2'))
        other = opfor_at(os.path.join(tmp, 'elsewhere', 'my-opfor'))
        nogame = os.path.join(tmp, 'no-game')
        os.makedirs(nogame)

        # --- OPFOR beside the suite --------------------------------------------------------------------------
        check(paths.find_opfor(suite=suite) == opfor, 'find_opfor: beside the suite (walk=True)')
        check(paths.find_opfor(walk=False, suite=suite) == opfor, 'find_opfor: beside the suite (walk=False)')
        check(paths.find_opfor(start=os.path.join(suite, 'bfbc2-auto-navkit')) == opfor,
              'find_opfor: walking up from a tool folder inside the suite')

        # --- precedence: given, OPFOR_ROOT, the search; candidates without game\ skipped ---------------------
        with env(OPFOR_ROOT=other):
            check(paths.find_opfor(suite=suite) == other, 'OPFOR_ROOT before the folder beside the suite')
            check(paths.find_opfor(walk=False, suite=suite) == other, 'OPFOR_ROOT before beside (walk=False)')
            check(paths.find_opfor(opfor, suite=suite) == opfor, 'the given folder before OPFOR_ROOT')
            check(paths.find_opfor(nogame, suite=suite) == other, 'a given folder without game\\ is skipped')
        with env(OPFOR_ROOT=nogame):
            check(paths.find_opfor(suite=suite) == opfor, 'an OPFOR_ROOT without game\\ is skipped')
        with env(OPFOR_ROOT=''):
            check(paths.opfor_candidates(None, walk=False, suite=suite) == [os.path.join(projects, paths.OPFOR_DIR)],
                  'an empty OPFOR_ROOT counts as unset')
        with env(OPFOR_ROOT=other):
            check(paths.opfor_candidates('x', walk=False, suite=suite, env=False) == ['x', opfor],
                  'opfor_candidates(env=False) leaves OPFOR_ROOT out')

        # --- walk=True: inside the suite first, then further up; walk=False: beside only ----------------------
        inside = opfor_at(os.path.join(suite, 'project-opfor-bfbc2'))
        check(paths.find_opfor(suite=suite) == inside and paths.find_opfor(walk=False, suite=suite) == opfor,
              'walk=True takes a project-opfor-bfbc2 inside the suite first; walk=False only looks beside it')
        shutil.rmtree(inside)
        deep = os.path.join(tmp, 'a', 'b', 'c', 'bfbc2-tool-suite')
        os.makedirs(deep)
        upper = opfor_at(os.path.join(tmp, 'a', 'project-opfor-bfbc2'))
        check(paths.find_opfor(suite=deep) == upper, 'walk=True finds Project OPFOR BFBC2 two folders further up')
        e = raised(lambda: paths.find_opfor(walk=False, suite=deep))
        check(isinstance(e, paths.GameCopyNotFound), 'walk=False does not look further up', repr(e))
        check(paths.walk_up(deep)[:3] == [os.path.join(deep, paths.OPFOR_DIR), os.path.join(tmp, 'a', 'b', 'c', paths.OPFOR_DIR),
                                          os.path.join(tmp, 'a', 'b', paths.OPFOR_DIR)]
              and paths.walk_up(deep)[-1] == os.path.join(os.path.splitdrive(tmp)[0] + os.sep, paths.OPFOR_DIR),
              'walk_up: the start folder first, up to the drive root')

        # --- missing game copy ----------------------------------------------------------------------------------
        lonely = os.path.join(tmp, 'lonely', 'bfbc2-tool-suite')
        os.makedirs(lonely)
        with env(OPFOR_ROOT=nogame):
            e = raised(lambda: paths.find_opfor('given-nowhere', walk=False, suite=lonely))
        want = [ 'given-nowhere', nogame, os.path.join(tmp, 'lonely', paths.OPFOR_DIR)]
        check(isinstance(e, paths.GameCopyNotFound) and isinstance(e, paths.PathsError)
              and getattr(e, 'looked_in', None) == want, 'GameCopyNotFound: every folder tried, in order', repr(e))
        check(str(e) == 'Project OPFOR BFBC2 not found - pass --opfor <folder> or set OPFOR_ROOT '
                        f'(looked for a game\\ folder in: {", ".join(want)})', 'GameCopyNotFound: NavKit\'s message', str(e))
        check(e is not None and errors.explain(e)[0] == 'The tools could not find Project OPFOR BFBC2\'s game copy.',
              'errors.explain: GameCopyNotFound is the game-copy case')
        renamed = paths.GameCopyNotFound('worded otherwise', [])
        check(errors.explain(renamed)[0] == 'The tools could not find Project OPFOR BFBC2\'s game copy.',
              'errors.explain matches GameCopyNotFound by type, whatever the words')

        # --- the game executable --------------------------------------------------------------------------------
        check(paths.game_dir(opfor) == os.path.join(opfor, 'game')
              and paths.game_exe(opfor) == os.path.join(opfor, 'game', 'BFBC2Game.exe'), 'game_dir / game_exe')
        check(paths.find_game_exe(walk=False, suite=suite) == paths.game_exe(opfor), 'find_game_exe: beside the suite')
        exeless = opfor_at(os.path.join(tmp, 'exeless'), exe=False)
        with env(OPFOR_ROOT=exeless):
            check(paths.find_game_exe(walk=False, suite=suite) == paths.game_exe(opfor)
                  and paths.find_opfor(walk=False, suite=suite) == exeless,
                  'find_game_exe needs the exe (an OPFOR_ROOT with an empty game\\ is skipped); find_opfor only game\\')
        check(paths.find_game_exe(walk=False, suite=lonely) is None, 'find_game_exe: None when there is none')
        os.chdir(tmp)
        with env(OPFOR_ROOT=os.path.join('elsewhere', 'my-opfor')):
            check(paths.find_game_exe(walk=False, suite=lonely) == os.path.join('elsewhere', 'my-opfor', 'game', 'BFBC2Game.exe'),
                  'find_game_exe: a relative OPFOR_ROOT gives a relative path (joined, as convert / crashreader did)')
            check(paths.find_opfor(walk=False, suite=lonely) == other, 'find_opfor: a relative OPFOR_ROOT made absolute')
        os.chdir(projects)
        check(paths.find_opfor('project-opfor-bfbc2', walk=False, suite=lonely) == opfor,
              'find_opfor: a relative given folder made absolute')
        os.chdir(cwd)

        # --- built tools ----------------------------------------------------------------------------------------
        e = raised(lambda: paths.find_tool(opfor, 'FbrbTool.exe', 'bfbc2-fbrb-tool', suite=suite))
        check(isinstance(e, paths.ToolNotFound)
              and str(e) == "FbrbTool.exe not found - run build.bat in the toolkit's bfbc2-fbrb-tool folder",
              'find_tool: neither built nor in tools\\ - ToolNotFound in NavKit\'s words', repr(e))
        in_tools = touch(os.path.join(opfor, 'tools', 'FbrbTool.exe'))
        check(paths.find_tool(opfor, 'FbrbTool.exe', 'bfbc2-fbrb-tool', suite=suite) == in_tools,
              'find_tool: OPFOR\'s tools\\ folder')
        built = touch(paths.suite_tool('bfbc2-fbrb-tool', 'FbrbTool.exe', suite))
        check(built == os.path.join(suite, 'bfbc2-fbrb-tool', 'FbrbTool.exe')
              and paths.find_tool(opfor, 'FbrbTool.exe', 'bfbc2-fbrb-tool', suite=suite) == built,
              'find_tool: the suite\'s own build first')

        # --- sibling projects -----------------------------------------------------------------------------------
        xenon = os.path.join(projects, 'operation-rising-sun', 'work', 'extracted', 'dist', 'xenon')
        args = (paths.BF1943_WORKSPACE, 'work', 'extracted', 'dist', 'xenon')
        check(paths.sibling(*args, env='BF1943_XENON', suite=suite) == xenon and not os.path.exists(xenon),
              'sibling: the path beside the suite even when it is missing (default_source does not check)')
        with env(BF1943_XENON=r'X:\somewhere'):
            check(paths.sibling(*args, env='BF1943_XENON', suite=suite) == r'X:\somewhere',
                  'sibling: the environment variable first, as it is')
            check(paths.sibling(*args, suite=suite) == xenon, 'sibling without env= ignores the variable')
        check(paths.find_sibling(paths.BF1943_SUITE, 'bf1943-fbrb-tool', marker='fbrb1943.py', env='BF1943_SUITE_FBRB',
                                 suite=suite) is None, 'find_sibling: None when the sibling is missing')
        fb_env = os.path.dirname(touch(os.path.join(tmp, 'fb-env', 'fbrb1943.py')))
        with env(BF1943_SUITE_FBRB=fb_env):
            check(paths.find_sibling(paths.BF1943_SUITE, 'bf1943-fbrb-tool', marker='fbrb1943.py',
                                     env='BF1943_SUITE_FBRB', suite=suite) == fb_env,
                  'find_sibling: the environment variable when the sibling is missing')
            fb_here = os.path.dirname(touch(os.path.join(projects, 'bf1943-tool-suite', 'bf1943-fbrb-tool', 'fbrb1943.py')))
            check(paths.find_sibling(paths.BF1943_SUITE, 'bf1943-fbrb-tool', marker='fbrb1943.py',
                                     env='BF1943_SUITE_FBRB', suite=suite) == fb_here,
                  'find_sibling: the folder beside the suite before the variable (fbrb1943\'s order)')
            check(paths.find_sibling(paths.BF1943_SUITE, 'bf1943-fbrb-tool', marker='fbrb1943.py',
                                     env='BF1943_SUITE_FBRB', env_first=True, suite=suite) == fb_env,
                  'find_sibling(env_first=True): the variable first')
        n = len(sys.path)
        paths.use_folder(fb_here)
        paths.use_folder(fb_here)
        check(sys.path[0] == fb_here and len(sys.path) == n + 1, 'use_folder: first on sys.path, once')

        # --- a suite reached through a junction -----------------------------------------------------------------
        jdir = os.path.join(tmp, 'J')
        os.makedirs(jdir)
        link = os.path.join(jdir, 'bfbc2-tool-suite')
        if junction(link, suite):
            e = raised(lambda: paths.find_opfor(walk=False, suite=link))
            check(isinstance(e, paths.GameCopyNotFound),
                  'junction: the folder beside the junction is searched, not beside its target (no realpath)')
            beside_link = opfor_at(os.path.join(jdir, 'project-opfor-bfbc2'))
            check(paths.find_opfor(walk=False, suite=link) == beside_link, 'junction: Project OPFOR BFBC2 beside it')
            olink = os.path.join(tmp, 'opfor-link')
            if junction(olink, opfor):
                check(paths.find_opfor(olink, walk=False, suite=lonely) == olink,
                      'junction: a given folder that is a junction is returned as given (absolute, unresolved)')
            os.rmdir(link)                                          # the junction only, not its target
            if os.path.isdir(olink):
                os.rmdir(olink)
        else:
            print('skip junction checks: no junction or symbolic link could be made here')

        # --- the module on its own --------------------------------------------------------------------------------
        core = os.path.dirname(os.path.dirname(os.path.abspath(paths.__file__)))
        r = subprocess.run([sys.executable, '-I', '-B', '-c',
                            f'import sys; sys.path.insert(0, {core!r}); from bfbc2core import paths; print(paths.SUITE)'],
                           capture_output=True, text=True, cwd=tmp)
        check(r.returncode == 0 and same(r.stdout.strip(), suite_paths.ROOT),
              'imports with only core\\ on sys.path; SUITE is the suite root', r.stderr[-300:])
        tree = ast.parse(open(paths.__file__, encoding='utf-8').read())
        mods = {a.name for n in ast.walk(tree) if isinstance(n, ast.Import) for a in n.names}
        mods |= {n.module or '' for n in ast.walk(tree) if isinstance(n, ast.ImportFrom)}
        check(mods == {'os', 'sys'}, 'paths.py needs only os and sys', str(sorted(mods)))
        check(same(paths.suite_root(), suite_paths.ROOT), 'suite_root() is suite_paths.ROOT')
    finally:
        os.chdir(cwd)
        sys.path[:] = saved_path
        for k, v in saved_env.items():
            if v is not None:
                os.environ[k] = v
        for d in (os.path.join(tmp, 'J', 'bfbc2-tool-suite'), os.path.join(tmp, 'opfor-link')):
            if os.path.isdir(d) and os.path.realpath(d) != d:
                os.rmdir(d)                                     # never follow a junction into its target
        shutil.rmtree(tmp, ignore_errors=True)
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
