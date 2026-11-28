r"""The game copy's install ledger, as the suite's Python uses it (NavKit, stock-MP, texqa): since round OR15 (audit
AU5-ARC-3) a client of the one implementation, rust\bc2-gamefs\src\ledger.rs, through the module's ledger_call
(rust\bc2-install\src\ledgerapi.rs). Until OR15 this file was a second implementation of the store (913 lines, the
"twin" of the Rust) and Project OPFOR's opforcore.ledger a third; the format did not change (opfor-ledger/1 under
<game>\opfor\ledger), so every ledger written by any of the three reads here (old ledgers on users' disks need no
conversion: the Rust reads them with both retired readers' rules and refuses a damaged record in words).

    led = Ledger(game)
    led.install('navkit:mp_005', {rel: data}, derived=False)     # the owner's whole set; Plan, LedgerError
    led.uninstall('navkit:mp_005')
    led.original(rel); led.before(owner, rel); led.holders(rel); led.status(); led.recover(); led.check()

What each tool installed into the game copy, so a revert gives back exactly what was there before that tool, in any
order, and an interrupted install or revert can be finished (spec transforms/install.md, XINST-LED). The rules, the
journal, the lock and the messages are the Rust's; this module only carries the calls.
"""
from __future__ import annotations

import hashlib
import json
import os
from collections.abc import Callable
from dataclasses import dataclass, field
from typing import Any

FORMAT = 'opfor-ledger/1'


class LedgerError(Exception):
    """Base: a damaged record or journal, a bad owner or path, a failed write (rolled back, the message says)."""


class Refused(LedgerError):
    """Checks failed: nothing was written. .conflicts says why, in plain words."""

    def __init__(self, conflicts: list[str]) -> None:
        super().__init__('; '.join(conflicts))
        self.conflicts = conflicts


class Locked(LedgerError):
    """Another operation holds the lock."""


class Crashed(LedgerError):
    """A test hook (BC2_LEDGER_CRASH_AT) stopped the operation as a kill would (journal and lock left)."""


def default_root(game: str) -> str:
    return os.path.join(game, 'opfor', 'ledger')


def owner_file(owner: str) -> str:
    return owner.replace(':', '~') + '.json'


def find_game(path: str) -> str | None:
    """The game copy a file is in: the nearest folder above it holding opfor\\ledger (None: no ledger there)."""
    d = os.path.dirname(os.path.abspath(path))
    while True:
        if os.path.isdir(default_root(d)):
            return d
        up = os.path.dirname(d)
        if up == d:
            return None
        d = up


def sha_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def pid_alive(pid: int) -> bool:
    """Whether a process with this id runs on this PC (the locks' liveness: a reused id counts as alive)."""
    if pid == os.getpid():
        return True
    if pid <= 0 or pid > 0xFFFFFFFF:
        return False
    if os.name == 'nt':
        import ctypes
        from ctypes import wintypes
        k32 = ctypes.WinDLL('kernel32', use_last_error=True)
        k32.OpenProcess.restype = wintypes.HANDLE
        k32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
        k32.GetExitCodeProcess.argtypes = [wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD)]
        k32.CloseHandle.argtypes = [wintypes.HANDLE]
        h = k32.OpenProcess(0x1000, False, pid)
        if not h:
            return bool(ctypes.get_last_error() == 5)
        try:
            code = wintypes.DWORD()
            return bool(k32.GetExitCodeProcess(h, ctypes.byref(code))) and code.value == 259
        finally:
            k32.CloseHandle(h)
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


@dataclass
class Plan:
    owner: str
    action: str
    writes: list[str] = field(default_factory=list)
    notes: list[str] = field(default_factory=list)
    applied: bool = False


_ERRORS: dict[str, type[LedgerError]] = {'locked': Locked, 'crashed': Crashed}


def call(game: str, root: str | None, op: str, args: dict[str, Any], datas: list[bytes] | None = None,
         log: Callable[[str], None] | None = None) -> Any:
    """One operation of the Rust ledger (bc2_install::ledgerapi: the ledger's own operations, named ledger_*, and
    Project OPFOR's stage operations); its result, or the LedgerError it ended with. log hears the lines the
    operation said (a resumed install)."""
    from . import native
    m = native.require('the install ledger')
    out = json.loads(m.ledger_call(game, root, op, json.dumps(args), list(datas or [])))
    for line in out.get('log') or []:
        if log is not None:
            log(line)
    if out.get('ok'):
        return out.get('result')
    kind, msg, conflicts = out.get('error', ''), out.get('message', ''), out.get('conflicts') or []
    if kind == 'refused':
        raise Refused(conflicts or [msg])
    raise _ERRORS.get(kind, LedgerError)(msg)


def _plan(d: dict[str, Any] | None) -> Plan | None:
    if d is None:
        return None
    return Plan(d['owner'], d['action'], list(d.get('writes', [])), list(d.get('notes', [])), bool(d.get('applied')))


class Ledger:
    def __init__(self, game: str, root: str | None = None) -> None:
        self.game = os.path.abspath(game)
        self.root = os.path.abspath(root) if root else default_root(self.game)
        self._root_arg = None if root is None else self.root

    def _call(self, op: str, datas: list[bytes] | None = None, **args: Any) -> Any:
        return call(self.game, self._root_arg, 'ledger_' + op, args, datas)

    def path(self, rel: str) -> str:
        return os.path.join(self.game, *rel.split('/'))

    def blob_path(self, sha: str) -> str:
        return os.path.join(self.root, 'blobs', sha + '.bin')

    def records(self) -> dict[str, dict[str, Any]]:
        """Every owner's record (LedgerError naming the first damaged one)."""
        return dict(self._call('records'))

    def holders(self, rel: str) -> list[str]:
        """The owners holding rel as a whole file, bottom first."""
        return list(self._call('holders', rel=rel))

    def owner_files(self, owner: str) -> list[str]:
        return list(self._call('owner_files', owner=owner))

    def original(self, rel: str) -> str | None:
        """What was there before any owner installed rel (a path to its bytes), None when an owner created it."""
        r = self._call('original', rel=rel)
        return None if r is None else str(r)

    def before(self, owner: str, rel: str) -> tuple[bool, str | None]:
        """(the owner holds rel, the path of what was there before it - None: nothing)."""
        held, p = self._call('before', owner=owner, rel=rel)
        return bool(held), None if p is None else str(p)

    def status(self) -> list[str]:
        return list(self._call('status'))

    def entries(self) -> list[dict[str, str]]:
        """Per contribution: owner, kind, file, slot, state (ok / buried / drift / missing / kept / corrupt /
        interrupted), detail."""
        return list(self._call('entries'))

    def check(self) -> dict[str, Any]:
        """The ledger's health (read only): healthy, lines, owners, kinds, damaged, blobs, interrupted, lock, kept."""
        return dict(self._call('check'))

    def recover(self, force: bool = False) -> list[str]:
        return list(self._call('recover', force=force))

    def install(self, owner: str, files: dict[str, bytes], derived: bool = False, note: str = '',
                dry_run: bool = False) -> Plan:
        rels = list(files)
        p = _plan(self._call('install', [files[r] for r in rels], owner=owner, rels=rels, derived=derived,
                             note=note, dry_run=dry_run))
        assert p is not None
        return p

    def uninstall(self, owner: str, dry_run: bool = False) -> Plan:
        p = _plan(self._call('uninstall', owner=owner, dry_run=dry_run))
        assert p is not None
        return p

    def forget(self, owner: str, only_missing: bool = False, dry_run: bool = False) -> Plan:
        p = _plan(self._call('forget', owner=owner, only_missing=only_missing, dry_run=dry_run))
        assert p is not None
        return p


def legacy_markers(path: str) -> list[str]:
    """The legacy install markers beside a file (<name>.opfor-*, from before the ledger), as suffixes."""
    d = os.path.dirname(os.path.abspath(path))
    return list(call(d, None, 'ledger_legacy_markers', {'path': path}))


def adopt_legacy(led: Ledger, owner: str, rels: list[str], family: str) -> Plan | None:
    """Adopts the legacy markers of family ('navkit' / 'stockmp') beside the game files rels as owner's install,
    removing them (one journalled commit). None when there were none. Refused in words when another family's markers
    are there, or the markers do not describe one finished install."""
    return _plan(led._call('adopt_legacy', owner=owner, rels=rels, family=family))
