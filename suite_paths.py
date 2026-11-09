r"""The suite's one sys.path bootstrap: which folders a tool's imports come from, each added once.

Every tool, library module and test finds this file through the suite root, then names the folders it imports from:

    import os, sys
    _ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))     # the suite root (as many dirnames as deep)
    if _ROOT not in sys.path:
        sys.path.insert(0, _ROOT)
    import suite_paths  # noqa: E402
    suite_paths.use('core', 'navkit-tools')

use() puts the named folders first on sys.path in the order given, and never twice: a tool that imports another
(or a regression run that calls a function per level) no longer grows sys.path. A script outside the suite (the
OPFOR project's src\tools) may still put one tool folder on sys.path and import its modules: each module brings in
what it needs itself.
"""
import os
import sys

ROOT = os.path.dirname(os.path.abspath(__file__))

# short name -> folder under the suite root
FOLDERS = {
    'root': '',
    'core': 'core',
    'navkit': 'bfbc2-auto-navkit',
    'navkit-tools': os.path.join('bfbc2-auto-navkit', 'tools'),
    'harness': 'bfbc2-automated-map-and-bot-test-harness',
    'crash-reader': os.path.join('bfbc2-automated-map-and-bot-test-harness', 'crash-reader'),
    'bf1943-import': 'bfbc2-bf1943-import',
    'console-convert': 'bfbc2-console-convert',
    'apt': os.path.join('bfbc2-console-readers', 'apt'),
    'stfs': os.path.join('bfbc2-console-readers', 'stfs'),
    'xiso': os.path.join('bfbc2-console-readers', 'xiso'),
    'culltree': 'bfbc2-culltree',
    'map-inspector': 'bfbc2-map-inspector',
    'menu-tools': 'bfbc2-menu-tools',
    'mesh-viewer': 'bfbc2-mesh-viewer',
    'model-tools': 'bfbc2-model-tools',
    'anim-tools': 'bfbc2-anim-tools',
    'navigation-studio': 'bfbc2-navigation-studio',
    'shader-tools': 'bfbc2-shader-tools',
    'stockmp': 'bfbc2-stockmp-ai',
}


def folder(name: str) -> str:
    """The absolute folder of a short name (see FOLDERS)."""
    if name not in FOLDERS:
        raise KeyError(f'suite_paths: no folder named {name!r} (known: {", ".join(sorted(FOLDERS))})')
    return os.path.normpath(os.path.join(ROOT, FOLDERS[name]))


def _norm(p: str) -> str:
    return os.path.normcase(os.path.normpath(os.path.abspath(p)))


def use(*names: str) -> None:
    """The named folders first on sys.path, in the order given; each one only once (an existing entry, in any
    spelling, is moved to its place rather than added again)."""
    wanted = [folder(n) for n in names]
    keys = {_norm(p) for p in wanted}
    sys.path[:] = wanted + [p for p in sys.path if not (p and _norm(p) in keys)]
