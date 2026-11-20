"""Navigation side files: `<level>.navtune.json` (DESIGN.md section 5.1) - the edits made to a level's AI
navigation data, recorded by position so they still apply after the level is regenerated.

    t = NavTune.new('Levels/SP_01', base_path, kind='navkit')     # or load(path)
    t.add(op('unwalkable', region=box(...)))                       # by / when filled in
    t.undo(); t.redo()
    t.save(path)

Bases: 'navkit' (NavKit's output; every operation allowed) or 'dice' (the level's own path data; only additive
operations: links, route edges, traversal annotations, notes - DESIGN.md section 5.1, decided 2026-10-16).
Each operation records who made it (`by`: the PC's name unless set) and when (`when`, local time).

validate, the side file's text (save) and summary run in the suite's Rust core (round F2b,
rust/bc2-navbuild/src/navtune.rs), which they need (bfbc2core.native.NativeMissing says how to get it). A value the
port does not take (a type JSON does not have, a comparison Python would raise on, a value printed with repr) raises
bfbc2core.native.Refused (a ValueError). Their Python (validate_reference, NavTune.save_text_reference,
summary_reference) was retired in round RT2 against its recorded answers (core/tests/f2b_navtune_replay*.json).
"""
import copy
import datetime
import hashlib
import json
import os
import socket
from collections.abc import Iterable, Sequence
from types import ModuleType
from typing import Any

from . import atomicio

FORMAT, VERSION = 'bfbc2-navtune', 1
KINDS = ('navkit', 'dice')
TRAVERSALS = ('ladder', 'jump', 'vault', 'drop')
# operation -> required fields (besides op, by, when)
OPS = {
    'walkable': ('region',),
    'unwalkable': ('region',),
    'seed': ('at',),
    'unseed': ('at',),
    'link-areas': ('a', 'b'),
    'route-edge': ('a', 'b', 'action'),
    'traversal': ('kind', 'bottom', 'top'),
    'note': ('at', 'text'),
}
REGION_OPS = ('walkable', 'unwalkable', 'seed', 'unseed')          # need a rebuild: NavKit bases only


def author() -> str:
    """The name recorded with edits: the PC's name (Windows' computer name)."""
    return os.environ.get('COMPUTERNAME') or socket.gethostname() or 'unknown'


def now() -> str:
    return datetime.datetime.now().replace(microsecond=0).isoformat()


def sha1_of(path: str) -> str:
    h = hashlib.sha1()
    with open(path, 'rb') as f:
        for chunk in iter(lambda: f.read(1 << 20), b''):
            h.update(chunk)
    return h.hexdigest()


def polygon(points: Iterable[Sequence[float]], y_low: float, y_high: float) -> dict[str, Any]:
    return {'polygon': [[round(float(x), 3), round(float(z), 3)] for x, z in points], 'y': [round(float(y_low), 3), round(float(y_high), 3)]}


def box(x0: float, y0: float, z0: float, x1: float, y1: float, z1: float) -> dict[str, Any]:
    lo = [min(x0, x1), min(y0, y1), min(z0, z1)]
    hi = [max(x0, x1), max(y0, y1), max(z0, z1)]
    return {'box': [round(float(v), 3) for v in lo + hi]}


def point(p: Iterable[float]) -> list[float]:
    return [round(float(v), 3) for v in p]


def op(name: str, by: str | None = None, when: str | None = None, **fields: Any) -> dict[str, Any]:
    """An operation with its fields; positions may be numpy arrays (stored rounded to millimetres)."""
    o: dict[str, Any] = {'op': name}
    for k, v in fields.items():
        if k in ('at', 'a', 'b', 'bottom', 'top'):
            v = point(v)
        o[k] = v
    o['by'] = by or author()
    o['when'] = when or now()
    return o


def _core() -> ModuleType:
    """The Rust core (bfbc2core.native.require: NativeMissing, saying how to get it, when it is not there)."""
    from . import native
    return native.require('navigation side files (bfbc2core.navtune)', 'navtune_validate')


def _refused(what: str) -> Exception:
    from . import native
    return native.refused(f'{what}: a value the side-file code does not take (a type JSON does not have, or one it '
                          'cannot compare or print)')


def validate(o: dict[str, Any], kind: str = 'navkit') -> list[str]:
    """Problems with one operation (empty list: fine)."""
    got = _core().navtune_validate(o, kind) if type(kind) is str else None
    if got is None:
        raise _refused(f'operation {o.get("op") if isinstance(o, dict) else o!r}')
    return list(got)


class NavTune:
    def __init__(self, level: str, base: dict[str, Any], operations: Iterable[dict[str, Any]] | None = None) -> None:
        self.level = level
        self.base = base                       # {'kind', 'sha1', 'file'}
        self.operations = list(operations or [])
        self.redo_stack: list[dict[str, Any]] = []
        self.dirty = False

    @classmethod
    def new(cls, level: str, base_path: str, kind: str = 'navkit') -> 'NavTune':
        if kind not in KINDS:
            raise ValueError(f'base kind must be one of {KINDS}')
        return cls(level, {'kind': kind, 'sha1': sha1_of(base_path), 'file': os.path.basename(base_path)})

    @property
    def kind(self) -> str:
        return self.base.get('kind', 'navkit')

    def allowed(self, name: str) -> bool:
        return not (self.kind == 'dice' and name in REGION_OPS)

    def add(self, o: dict[str, Any]) -> dict[str, Any]:
        problems = validate(o, self.kind)
        if problems:
            raise ValueError('; '.join(problems))
        self.operations.append(o)
        self.redo_stack.clear()
        self.dirty = True
        return o

    def remove(self, index: int) -> dict[str, Any]:
        o = self.operations.pop(index)
        self.redo_stack.clear()
        self.dirty = True
        return o

    def undo(self) -> dict[str, Any] | None:
        if not self.operations:
            return None
        o = self.operations.pop()
        self.redo_stack.append(o)
        self.dirty = True
        return o

    def redo(self) -> dict[str, Any] | None:
        if not self.redo_stack:
            return None
        o = self.redo_stack.pop()
        self.operations.append(o)
        self.dirty = True
        return o

    def base_changed(self, base_path: str) -> bool:
        """True when the file at base_path is not the one the edits were made on."""
        return bool(self.base.get('sha1')) and sha1_of(base_path) != self.base['sha1']

    def to_json(self) -> dict[str, Any]:
        return {'format': FORMAT, 'version': VERSION, 'level': self.level, 'base': self.base,
                'operations': copy.deepcopy(self.operations)}

    def save_text(self) -> str:
        """The side file's text."""
        got = _core().navtune_dumps(self.to_json())
        if got is None:
            raise _refused(f'the side file of {self.level}')
        return str(got)

    def save(self, path: str) -> None:
        os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
        tmp = path + '.tmp'
        text = self.save_text()
        with open(tmp, 'w', encoding='utf-8', newline='\n') as f:
            f.write(text)
        atomicio.replace(tmp, path)            # never a half-written side file
        self.dirty = False

    def summary(self, k: int) -> str:
        o = self.operations[k]
        got = _core().navtune_summary(o)
        if got is None:
            raise _refused(f'operation {k}')
        return str(got)


def fmt(p: Iterable[float]) -> str:
    return '(' + ', '.join(f'{v:.1f}' for v in p) + ')'


def load(path: str) -> NavTune:
    d = json.load(open(path, encoding='utf-8'))
    if d.get('format') != FORMAT:
        raise ValueError(f'{path}: not a navtune side file')
    if d.get('version', 0) > VERSION:
        raise ValueError(f'{path}: version {d["version"]} is newer than this tool reads ({VERSION})')
    t = NavTune(d.get('level', ''), d.get('base', {}), d.get('operations', []))
    problems = [f'operation {k}: {p}' for k, o in enumerate(t.operations) for p in validate(o, t.kind)]
    if problems:
        raise ValueError(f'{path}: ' + '; '.join(problems[:10]))
    return t


def default_path(root: str, lv: str) -> str:
    """Where the studio keeps a level's side file: OPFOR's work\\navkit\\<level>\\<level>.navtune.json."""
    return os.path.join(root, 'work', 'navkit', lv, f'{lv}.navtune.json')
