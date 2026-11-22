"""BFBC2 suite core: game data readers, scene model, 3D view (see ../../DESIGN.md section 3).

__version__ is the suite's version: the VERSION file at the repository's root (round P1; CHANGELOG.md, RELEASING.md),
the one place bfbc2.exe --version, the map editor's Help and the hub's title read it from too.
"""
import os

# One OpenBLAS thread unless the environment says otherwise (suite_paths.py: 490 MB of commit per process saved).
os.environ.setdefault('OPENBLAS_NUM_THREADS', '1')

VERSION_FILE = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))), 'VERSION')


def _read_version(path: str = VERSION_FILE) -> str:
    """The first line of the VERSION file, or '0.0.0+unknown' when core\\ was copied somewhere without it."""
    try:
        with open(path, encoding='utf-8') as f:
            return f.readline().strip() or '0.0.0+unknown'
    except OSError:
        return '0.0.0+unknown'


__version__ = _read_version()
