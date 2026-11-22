r"""The game copy's install ledger (round DF2), whole-file part: the Python twin of rust\bc2-formats\src\gameledger.rs
(same store, same rules, same messages).

What each tool installed into the game copy, so a revert gives back exactly what was there before that tool, in any
order, and an interrupted install or revert can be finished. The store is Project OPFOR's opforcore.ledger (format
opfor-ledger/1) under <game>\opfor\ledger, shared by OPFOR's stages and the suite's NavKit and stock-MP installs
(spec transforms/install.md, XINST-LED). This module writes kind 'file' contributions; records of other kinds
(written by OPFOR) are kept as they are, except a spliced contribution's prior / below.

Per game file the owners form a stack: the bottom one's prior is what was there before any owner (DICE's original,
or nothing), each one's value the sha256 it wrote, its below the owner it sits on. Install puts a layer on top
(refused when the file changed outside the ledger); a re-install of the top owner keeps its prior. Uninstall of the
top owner writes its prior back; of a buried owner splices it out (the owner above inherits its prior) unless the
owner above is derived (built from the file it found): then it is refused, naming what to revert first.

    led = Ledger(game)
    led.install('navkit:mp_005', {rel: data}, derived=False)     # the owner's whole set; Plan, LedgerError
    led.uninstall('navkit:mp_005')
    led.original(rel); led.before(owner, rel); led.holders(rel); led.status(); led.recover()
"""
from __future__ import annotations

import contextlib
import datetime
import hashlib
import json
import os
import shutil
import socket
import time
import uuid
from collections.abc import Callable
from dataclasses import dataclass, field
from typing import Any

from . import atomicio

FORMAT = 'opfor-ledger/1'
JOURNAL_FORMAT = 'opfor-ledger-txn/1'
TMP = '.opfor-ledger-tmp'       # the fixed temporary name before round PR1a: never a legacy marker, removed on recovery
LOCK, JOURNAL, JOURNAL_DIR, DIRTY = '.lock', '.txn.json', '.txn', '.dirty'
TRIES = 11


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
    """A test hook stopped the operation as a kill would (journal and lock left)."""


class Interrupt(Exception):
    """Raised by a test hook: fail=True a write error (rolled back), else a crash."""

    def __init__(self, crash: bool = False) -> None:
        super().__init__('injected')
        self.crash = crash


def default_root(game: str) -> str:
    return os.path.join(game, 'opfor', 'ledger')


def norm_rel(rel: str) -> str:
    r = rel.replace('\\', '/').strip('/')
    parts = [p if p in ('.', '..') else p.rstrip(' .') for p in r.split('/')]
    if not r or rel[:1] in ('/', '\\') or (len(r) >= 2 and r[1] == ':') or any(p in ('', '.', '..') for p in parts):
        raise LedgerError(f'not a path inside the game copy: {rel!r}')
    return '/'.join(parts)


def valid_owner(owner: str) -> bool:
    import re
    return bool(re.fullmatch(r'[a-z0-9][a-z0-9_.-]*(:[a-z0-9][a-z0-9_.-]*)*', owner))


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


def legacy_markers(path: str) -> list[str]:
    """The legacy install markers beside a file (<name>.opfor-*, from before the ledger), as suffixes."""
    d, name = os.path.split(path)
    pre = name.lower() + '.opfor-'
    try:
        names = os.listdir(d or '.')
    except OSError:
        return []
    return sorted(f[len(pre) - len('.opfor-'):] for f in names
                  if f.lower().startswith(pre) and not f.lower().endswith(TMP) and os.path.isfile(os.path.join(d, f)))


def sha_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _now() -> tuple[str, str]:
    t = datetime.datetime.now(datetime.UTC)
    return t.strftime('%Y-%m-%dT%H:%M:%SZ'), t.strftime('%Y%m%dT%H%M%SZ')


def _new_txn() -> str:
    return _now()[1] + '-' + uuid.uuid4().hex[:8]


def _read(p: str) -> bytes | None:
    try:
        with open(p, 'rb') as f:
            return f.read()
    except FileNotFoundError:
        return None


def _transient(e: OSError) -> bool:
    return isinstance(e, PermissionError) or getattr(e, 'winerror', None) in (5, 32, 33)


def _retry(fn: Callable[[], None]) -> None:
    for n in range(1, TRIES + 1):
        try:
            fn()
            return
        except OSError as e:
            if n == TRIES or not _transient(e):
                raise
            time.sleep(0.05 * n)


def write_atomic(path: str, data: bytes) -> None:
    """data to path with atomicio.write_bytes (its folder made first): a per-process temporary name beside it, flushed
    to disk, moved over path (retried while held). Before round PR1a the temporary name was the fixed <path> + TMP,
    which two writers of one path would have shared."""
    os.makedirs(os.path.dirname(path) or '.', exist_ok=True)
    atomicio.write_bytes(path, data)


def _remove(path: str) -> None:
    def rm() -> None:
        with contextlib.suppress(FileNotFoundError):
            os.remove(path)
    _retry(rm)


def pid_alive(pid: int) -> bool:
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


def _sha_of(v: Any) -> str | None:
    return v.get('sha') if isinstance(v, dict) else None


def _derived(c: dict[str, Any]) -> bool:
    e = c.get('extra')
    return bool(isinstance(e, dict) and e.get('derived'))


@dataclass
class Plan:
    owner: str
    action: str
    writes: list[str] = field(default_factory=list)
    notes: list[str] = field(default_factory=list)
    applied: bool = False


@dataclass
class Adopt:
    rel: str
    prior: bytes | None


Layer = tuple[str, int]


class Ledger:
    def __init__(self, game: str, root: str | None = None) -> None:
        self.game = os.path.abspath(game)
        self.root = os.path.abspath(root) if root else default_root(self.game)
        self.hook: Callable[[str], None] | None = None     # tests: raises Interrupt at a write step
        v = os.environ.get('BC2_LEDGER_CRASH_AT', '').strip()
        self.crash_at = int(v) if v.isdigit() else None
        self.steps = 0

    def _step(self, label: str) -> None:
        self.steps += 1
        if self.crash_at == self.steps:
            print(f'BC2_LEDGER_CRASH_AT={self.steps}: stopped before {label}', flush=True)
            os._exit(9)
        if self.hook:
            try:
                self.hook(label)
            except Interrupt as e:
                if e.crash:
                    raise Crashed('stopped by the test hook') from None
                raise LedgerError(f'{label}: injected') from None

    def path(self, rel: str) -> str:
        return os.path.join(self.game, *rel.split('/'))

    def blob_path(self, sha: str) -> str:
        return os.path.join(self.root, 'blobs', sha + '.bin')

    def _blob(self, sha: str) -> bytes:
        p = self.blob_path(sha)
        d = _read(p)
        if d is None:
            raise OSError(f'{p}: missing')
        if sha_hex(d) != sha:
            raise OSError(f'{p}: damaged (its sha256 differs)')
        return d

    # ------------------------------------------------------------------------------------------------- records

    def records(self) -> dict[str, dict[str, Any]]:
        out: dict[str, dict[str, Any]] = {}
        if not os.path.isdir(self.root):
            return out
        for f in sorted(os.listdir(self.root)):
            if not f.endswith('.json') or f.startswith('.'):
                continue
            p = os.path.join(self.root, f)

            def bad(why: str, p: str = p) -> LedgerError:
                return LedgerError(f'damaged ledger record: {p} ({why}) - restore it from a backup, or drop it with '
                                   "Project OPFOR's ledger forget")
            try:
                with open(p, 'rb') as fh:
                    v = json.loads(fh.read())
            except ValueError as e:
                raise bad(str(e)) from e
            if not isinstance(v, dict) or v.get('format') != FORMAT:
                raise bad('not an opfor-ledger/1 record')
            owner = v.get('owner')
            if not (isinstance(owner, str) and valid_owner(owner) and owner_file(owner) == f):
                raise bad('its owner does not match its name')
            cs = v.get('contributions')
            if not isinstance(cs, list):
                raise bad('no contributions list')
            for c in cs:
                if not (isinstance(c, dict) and isinstance(c.get('kind'), str) and isinstance(c.get('file'), str)):
                    raise bad('a contribution without kind / file')
                if c['kind'] == 'file' and _sha_of(c.get('value')) is None:
                    raise bad("a file contribution without its value's sha")
            out[owner] = v
        return out

    @staticmethod
    def _layers(recs: dict[str, dict[str, Any]], rel: str) -> tuple[list[Layer], list[str]]:
        low = rel.lower()
        layers: list[Layer] = []
        slots: list[str] = []
        for o, r in recs.items():
            for i, c in enumerate(r['contributions']):
                if c['file'].lower() != low:
                    continue
                if c['kind'] == 'file':
                    layers.append((o, i))
                elif o not in slots:
                    slots.append(o)
        return layers, slots

    @staticmethod
    def _c(recs: dict[str, dict[str, Any]], layer: Layer) -> dict[str, Any]:
        c: dict[str, Any] = recs[layer[0]]['contributions'][layer[1]]
        return c

    @classmethod
    def _stack(cls, recs: dict[str, dict[str, Any]], layers: list[Layer]) -> list[Layer]:
        out: list[Layer] = []
        cur: str | None = None
        for _ in range(len(layers) + 1):
            nxt = next((l for l in layers if cls._c(recs, l).get('below') == cur and all(o[0] != l[0] for o in out)),
                       None)
            if nxt is None:
                break
            cur = nxt[0]
            out.append(nxt)
        out += [l for l in layers if all(o[0] != l[0] for o in out)]
        return out

    def holders(self, rel: str) -> list[str]:
        rel = norm_rel(rel)
        recs = self.records()
        return [l[0] for l in self._stack(recs, self._layers(recs, rel)[0])]

    def owner_files(self, owner: str) -> list[str]:
        r = self.records().get(owner)
        return [c['file'] for c in r['contributions']] if r else []

    def _prior_path(self, c: dict[str, Any]) -> str | None:
        p = c.get('prior')
        return self.blob_path(p['blob']) if isinstance(p, dict) and isinstance(p.get('blob'), str) else None

    def original(self, rel: str) -> str | None:
        """What was there before any owner: the bottom layer's prior (its blob path), None when an owner put the file
        where there was none; with no owner the file itself (None when missing)."""
        rel = norm_rel(rel)
        recs = self.records()
        st = self._stack(recs, self._layers(recs, rel)[0])
        if not st:
            p = self.path(rel)
            return p if os.path.isfile(p) else None
        return self._prior_path(self._c(recs, st[0]))

    def before(self, owner: str, rel: str) -> tuple[bool, str | None]:
        """(whether owner holds rel, the blob path of what was there before it or None)."""
        rel = norm_rel(rel)
        recs = self.records()
        for l in self._layers(recs, rel)[0]:
            if l[0] == owner:
                return True, self._prior_path(self._c(recs, l))
        return False, None

    def status(self) -> list[str]:
        out = []
        if os.path.isfile(os.path.join(self.root, JOURNAL)) or os.path.isfile(os.path.join(self.root, DIRTY)):
            out.append(f'interrupted: {os.path.join(self.root, JOURNAL)} - run recover')
        recs = self.records()
        for o, r in recs.items():
            for c in r['contributions']:
                f = c['file']
                if c['kind'] != 'file':
                    out.append(f"{o} {f} {c['kind']} (Project OPFOR's ledger)")
                    continue
                above = next((l for l in self._layers(recs, f)[0] if self._c(recs, l).get('below') == o), None)
                if above:
                    state = f'buried under {above[0]}'
                else:
                    d = _read(self.path(f))
                    state = 'missing' if d is None else 'ok' if sha_hex(d) == _sha_of(c['value']) else \
                        'changed outside the ledger'
                out.append(f'{o} {f} {state}')
        return out

    # ---------------------------------------------------------------------------------------------------- lock

    def _lock_info(self) -> dict[str, Any] | None:
        p = os.path.join(self.root, LOCK)
        d = _read(p)
        if d is None:
            return None
        try:
            age = time.time() - os.path.getmtime(p)
        except OSError:
            age = 0.0
        try:
            v = json.loads(d)
        except ValueError:
            t = d.decode('utf-8', 'replace').strip()
            v = {'pid': int(t)} if t.isdigit() else {}
        info: dict[str, Any] = v if isinstance(v, dict) else {'pid': v} if isinstance(v, int) else {}
        info['age'] = age
        return info

    @staticmethod
    def _stale(info: dict[str, Any]) -> bool:
        pid = info.get('pid')
        if not isinstance(pid, int):
            return float(info.get('age', 0)) > 60
        host = info.get('host')
        if host and host.lower() != socket.gethostname().lower():
            return False
        return not pid_alive(pid)

    def _lock(self, notes: list[str]) -> None:
        os.makedirs(self.root, exist_ok=True)
        p = os.path.join(self.root, LOCK)
        for _ in range(2):
            try:
                fd = os.open(p, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
            except FileExistsError:
                info = self._lock_info() or {}
                if not self._stale(info):
                    raise Locked(f'{p}: held by pid {info.get("pid", "?")} since {info.get("started", "?")}: another '
                                 'install / revert is running - try again when it has finished') from None
                notes.append(f'a stale lock (pid {info.get("pid", "?")} is gone: an install or revert was interrupted)'
                             ' was cleared')
                _remove(p)
                continue
            try:
                os.write(fd, json.dumps({'pid': os.getpid(), 'started': _now()[0]}).encode())
            finally:
                os.close(fd)
            try:
                self._recover_locked(notes)
            except BaseException:
                self._unlock()
                raise
            return
        raise Locked(f'{p}: could not be taken')

    def _unlock(self) -> None:
        with contextlib.suppress(OSError):
            _remove(os.path.join(self.root, LOCK))

    # ------------------------------------------------------------------------------------------------- journal

    def _target(self, base: str, rel: str) -> str:
        top = {'game': self.game, 'ledger': self.root}.get(base)
        if top is None:
            raise LedgerError(f'journal target base {base!r}')
        return os.path.join(top, *norm_rel(rel).split('/'))

    def _begin(self, owner: str, action: str, txn: str, targets: list[tuple[str, str]]) -> None:
        self._step('journal')
        d = os.path.join(self.root, JOURNAL_DIR)
        if os.path.isdir(d):
            shutil.rmtree(d)
        os.makedirs(d)
        entries = []
        seen = set()
        for base, rel in targets:
            if (base, rel.lower()) in seen:
                continue
            seen.add((base, rel.lower()))
            data = _read(self._target(base, rel))
            pre = None
            if data is not None:
                sha = sha_hex(data)
                bp = os.path.join(d, sha + '.bin')
                if not os.path.isfile(bp):
                    write_atomic(bp, data)
                pre = {'sha': sha, 'size': len(data)}
            entries.append({'base': base, 'path': rel, 'pre': pre})
        j = {'format': JOURNAL_FORMAT, 'owner': owner, 'action': action, 'txn': txn, 'pid': os.getpid(),
             'started': _now()[0], 'targets': entries}
        write_atomic(os.path.join(self.root, JOURNAL), (json.dumps(j, indent=1) + '\n').encode())

    def _clear_journal(self) -> None:
        with contextlib.suppress(OSError):
            _remove(os.path.join(self.root, JOURNAL))
        shutil.rmtree(os.path.join(self.root, JOURNAL_DIR), ignore_errors=True)
        with contextlib.suppress(OSError):
            _remove(os.path.join(self.root, DIRTY))

    def _rollback(self, j: dict[str, Any]) -> list[str]:
        d = os.path.join(self.root, JOURNAL_DIR)
        done, errors = [], []
        for t in reversed(j.get('targets') or []):
            try:
                base, rel = t['base'], t['path']
                p = self._target(base, rel)
                with contextlib.suppress(OSError):
                    os.remove(p + TMP)
                atomicio.remove_dead_temps(p, pid_alive)
                sha = _sha_of(t.get('pre'))
                if sha is None:
                    if os.path.isfile(p):
                        _remove(p)
                        done.append(f'{base}:{rel} removed')
                    continue
                cur = _read(p)
                if cur is not None and sha_hex(cur) == sha:
                    continue
                data = _read(os.path.join(d, sha + '.bin'))
                if data is None or sha_hex(data) != sha:
                    raise LedgerError('the saved pre-image is damaged')
                write_atomic(p, data)
                done.append(f'{base}:{rel} restored')
            except Exception as e:  # noqa: BLE001 - every target is tried; the errors are reported together
                errors.append(f'{t.get("path") if isinstance(t, dict) else t}: {e}')
        if errors:
            raise LedgerError('; '.join(errors))
        return done

    def _recover_locked(self, notes: list[str]) -> None:
        jp = os.path.join(self.root, JOURNAL)
        if not os.path.isfile(jp):
            if os.path.isfile(os.path.join(self.root, DIRTY)):
                self._clear_journal()
            return
        try:
            with open(jp, 'rb') as f:
                j = json.loads(f.read())
            if not isinstance(j, dict) or j.get('format') != JOURNAL_FORMAT:
                raise ValueError('not a journal')
        except ValueError:
            raise LedgerError(f'{jp}: damaged journal: the files of the interrupted commit cannot be rolled back '
                              'automatically - restore the game files, then delete the journal') from None
        try:
            done = self._rollback(j)
        except LedgerError as e:
            write_atomic(os.path.join(self.root, DIRTY), f'{_now()[0]} recover: {e}\n'.encode())
            raise LedgerError(f'an interrupted commit could not be rolled back ({e}) - free the files (close the '
                              'game) and run recover again') from e
        notes.append(f'an interrupted {j.get("action", "?")} of {j.get("owner", "?")} (txn {j.get("txn", "?")}) was '
                     'rolled back first' + (f': {", ".join(done)}' if done else ''))
        self._clear_journal()

    def recover(self, force: bool = False) -> list[str]:
        if force:
            with contextlib.suppress(OSError):
                _remove(os.path.join(self.root, LOCK))
        notes: list[str] = []
        if not os.path.isdir(self.root):
            return notes
        self._lock(notes)
        self._unlock()
        return notes

    # -------------------------------------------------------------------------------------------------- engine

    def _current(self, work: dict[str, bytes | None], rel: str) -> bytes | None:
        k = rel.lower()
        if k not in work:
            work[k] = _read(self.path(rel))
        return work[k]

    def _take_out(self, recs: dict[str, dict[str, Any]], owner: str, c: dict[str, Any],
                  work: dict[str, bytes | None], writes: dict[str, tuple[str, bytes | None]], dirty: set[str],
                  conflicts: list[str], notes: list[str]) -> None:
        rel = c['file']
        if c['kind'] != 'file':
            conflicts.append(f"{rel}: {owner} holds a {c['kind']} slot there, which only Project OPFOR's ledger takes "
                             'out')
            return
        layers, _ = self._layers(recs, rel)
        above = next((l for l in layers if l[0] != owner and self._c(recs, l).get('below') == owner), None)
        if above:
            o = above[0]
            d = self._c(recs, above)
            if _derived(d):
                conflicts.append(f"{rel}: {o} was built on {owner}'s file: revert {o} first")
                return
            d['prior'], d['below'] = c.get('prior'), c.get('below')
            dirty.add(o)
            notes.append(f"{rel}: taken out from under {o}'s install, which now puts back what was there before "
                         f'{owner}')
            return
        cur = self._current(work, rel)
        if cur is None:
            conflicts.append(f'{rel}: missing (deleted since {owner} installed it)')
            return
        if sha_hex(cur) != _sha_of(c.get('value')):
            conflicts.append(f'{rel}: changed outside the ledger since {owner} installed it - not reverted')
            return
        data = None
        prior = c.get('prior')
        if isinstance(prior, dict) and isinstance(prior.get('blob'), str):
            try:
                data = self._blob(prior['blob'])
            except OSError as e:
                conflicts.append(f'{rel}: the file from before {owner} is missing from the ledger ({e})')
                return
        work[rel.lower()] = data
        writes[rel.lower()] = (rel, data)

    def install(self, owner: str, files: dict[str, bytes], derived: bool = False, note: str = '',
                dry_run: bool = False) -> Plan:
        """files {rel: bytes}: the owner's whole set (files of its earlier install not in it are taken out)."""
        return self._commit(owner, files, derived, note, dry_run)

    def uninstall(self, owner: str, dry_run: bool = False) -> Plan:
        return self._commit(owner, None, False, '', dry_run)

    def _commit(self, owner: str, files: dict[str, bytes] | None, derived: bool, note: str, dry_run: bool) -> Plan:
        if not valid_owner(owner):
            raise LedgerError(f'not an owner name: {owner!r}')
        plan = Plan(owner, 'install' if files is not None else 'uninstall')
        if not dry_run:
            self._lock(plan.notes)
        elif os.path.isfile(os.path.join(self.root, JOURNAL)):
            plan.notes.append("an interrupted commit's journal is there: it is rolled back first")
        try:
            self._commit_locked(owner, files, derived, note, dry_run, plan)
        except Crashed:
            raise
        except BaseException:
            if not dry_run:
                self._unlock()
            raise
        if not dry_run:
            self._unlock()
        return plan

    def _commit_locked(self, owner: str, files: dict[str, bytes] | None, is_derived: bool, note: str, dry_run: bool,
                       plan: Plan) -> None:
        recs = self.records()
        old = recs.get(owner)
        if files is None and old is None:
            plan.notes.append('not installed')
            return
        work: dict[str, bytes | None] = {}
        writes: dict[str, tuple[str, bytes | None]] = {}
        dirty: set[str] = set()
        conflicts: list[str] = []
        blobs: dict[str, bytes] = {}
        new_cs: list[dict[str, Any]] = []
        unchanged = True
        new_rels = [(norm_rel(r), d) for r, d in (files or {}).items()]
        seen: set[str] = set()
        for r, _ in new_rels:
            if r.lower() in seen:
                raise LedgerError(f'{r}: given twice')
            seen.add(r.lower())
        for c in list(old['contributions'] if old else []):
            if c['file'].lower() not in seen:
                unchanged = False
                self._take_out(recs, owner, c, work, writes, dirty, conflicts, plan.notes)
        for rel, data in new_rels:
            layers, slots = self._layers(recs, rel)
            slots = [o for o in slots if o != owner]
            if slots:
                conflicts.append(f"{rel}: {', '.join(slots)} holds parts of it (Project OPFOR's ledger): a whole-file "
                                 'install would overwrite them')
                continue
            cur = self._current(work, rel)
            cur_sha = None if cur is None else sha_hex(cur)
            new_sha = sha_hex(data)
            mine = next((l for l in layers if l[0] == owner), None)
            if mine is not None:
                c = self._c(recs, mine)
                above = next((l for l in layers if l[0] != owner and self._c(recs, l).get('below') == owner), None)
                if above:
                    conflicts.append(f'{rel}: {above[0]} was installed after {owner}: revert {above[0]} first')
                    continue
                if cur_sha is not None and cur_sha != _sha_of(c.get('value')):
                    conflicts.append(f'{rel}: changed outside the ledger since {owner} installed it - not overwritten')
                    continue
                if cur_sha is None:
                    plan.notes.append(f'{rel}: deleted since {owner} installed it - installed again')
                elif _sha_of(c.get('value')) == new_sha and _derived(c) == is_derived:
                    new_cs.append(c)
                    plan.notes.append(f'{rel}: already installed')
                    continue
                prior, below = c.get('prior'), c.get('below')
            else:
                st = self._stack(recs, layers)
                below = None
                if st:
                    top = st[-1]
                    if cur_sha != _sha_of(self._c(recs, top).get('value')):
                        conflicts.append(f'{rel}: changed outside the ledger since {top[0]} installed it - not '
                                         'overwritten')
                        continue
                    below = top[0]
                prior = None
                if cur is not None:
                    blobs[cur_sha or ''] = cur
                    prior = {'sha': cur_sha, 'size': len(cur), 'blob': cur_sha}
            unchanged = False
            new_cs.append({'kind': 'file', 'file': rel, 'slot': '', 'value': {'sha': new_sha, 'size': len(data)},
                           'prior': prior, 'below': below, 'stack': True, 'extra': {'derived': is_derived}})
            work[rel.lower()] = data
            writes[rel.lower()] = (rel, data)
        if conflicts:
            raise Refused(conflicts)
        if files is not None and old is not None and unchanged and not dirty and note == old.get('note', ''):
            plan.notes.append('unchanged: nothing written')
            return
        writes = {k: v for k, v in sorted(writes.items()) if _read(self.path(v[0])) != v[1]}
        plan.writes += [f'{"write" if d is not None else "remove"} {rel}' for rel, d in writes.values()]
        if dry_run:
            return
        rec: dict[str, Any] | None = None if files is None else {'format': FORMAT, 'owner': owner, 'txn': _new_txn(), 'installed': _now()[0],
                                          'note': note, 'contributions': new_cs}
        txn = str(rec['txn']) if rec else _new_txn()
        targets = [('game', rel) for rel, _ in writes.values()] + [('ledger', owner_file(owner))]
        targets += [('ledger', owner_file(o)) for o in sorted(dirty)]
        self._begin(owner, plan.action, txn, targets)
        try:
            for sha, data in sorted(blobs.items()):
                if not os.path.isfile(self.blob_path(sha)):
                    self._step(f'blob {sha}')
                    write_atomic(self.blob_path(sha), data)
            for rel, d in writes.values():
                if d is not None:
                    self._step(f'game {rel}')
                    write_atomic(self.path(rel), d)
                else:
                    self._step(f'remove {rel}')
                    _remove(self.path(rel))
            self._step(f'record {owner}')
            self._write_record(owner, rec)
            for o in sorted(dirty):
                if o != owner:
                    self._step(f'record {o}')
                    self._write_record(o, recs.get(o))
        except Crashed:
            raise
        except BaseException as e:
            raise self._abort(e) from e
        self._clear_journal()
        if rec is None:
            recs.pop(owner, None)
        else:
            recs[owner] = rec
        self._gc(recs)
        plan.applied = True

    def _abort(self, e: BaseException) -> LedgerError:
        jp = os.path.join(self.root, JOURNAL)
        try:
            with open(jp, 'rb') as f:
                j = json.loads(f.read())
        except (OSError, ValueError):
            j = None
        if j is not None:
            try:
                self._rollback(j)
            except LedgerError as r:
                write_atomic(os.path.join(self.root, DIRTY), f'{_now()[0]} {e}\nrollback: {r}\n'.encode())
                return LedgerError(f'{e}, and its rollback failed too ({r}): the game copy is half-written. Free the '
                                   'files (close the game), then run recover (bfbc2 gameledger recover <game>)')
        self._clear_journal()
        return LedgerError(f'{e} - nothing changed: every file written was put back')

    def _write_record(self, owner: str, rec: dict[str, Any] | None) -> None:
        p = os.path.join(self.root, owner_file(owner))
        if rec is None or not rec.get('contributions'):
            _remove(p)
            return
        write_atomic(p, (json.dumps(rec, indent=1, ensure_ascii=True) + '\n').encode('utf-8'))

    def _gc(self, recs: dict[str, dict[str, Any]]) -> None:
        d = os.path.join(self.root, 'blobs')
        if not os.path.isdir(d):
            return
        used = {v['blob'] for r in recs.values() for c in r['contributions'] for v in (c.get('value'), c.get('prior'))
                if isinstance(v, dict) and isinstance(v.get('blob'), str)}
        for f in os.listdir(d):
            if f.endswith('.bin') and f[:-4] not in used:
                with contextlib.suppress(OSError):
                    os.remove(os.path.join(d, f))

    def adopt(self, owner: str, entries: list[Adopt], remove: list[str]) -> Plan:
        """Records an install made before the ledger: owner installed each file as it is now, over prior (the legacy
        backup's bytes, or None); the legacy marker files remove (game-relative) are deleted in the same commit."""
        if not valid_owner(owner):
            raise LedgerError(f'not an owner name: {owner!r}')
        plan = Plan(owner, 'adopt')
        self._lock(plan.notes)
        try:
            self._adopt_locked(owner, entries, remove, plan)
        except Crashed:
            raise
        except BaseException:
            self._unlock()
            raise
        self._unlock()
        return plan

    def _adopt_locked(self, owner: str, entries: list[Adopt], remove: list[str], plan: Plan) -> None:
        recs = self.records()
        conflicts: list[str] = []
        cs = list(recs[owner]['contributions']) if owner in recs else []
        blobs: dict[str, bytes] = {}
        for a in entries:
            rel = norm_rel(a.rel)
            layers, slots = self._layers(recs, rel)
            if layers or slots:
                conflicts.append(f'{rel}: {(layers[0][0] if layers else slots[0])} holds it in the ledger already')
                continue
            cur = _read(self.path(rel))
            if cur is None:
                conflicts.append(f"{rel}: missing - the legacy install's file is gone")
                continue
            prior = None
            if a.prior is not None:
                s = sha_hex(a.prior)
                blobs[s] = a.prior
                prior = {'sha': s, 'size': len(a.prior), 'blob': s}
            cs.append({'kind': 'file', 'file': rel, 'slot': '', 'value': {'sha': sha_hex(cur), 'size': len(cur)},
                       'prior': prior, 'below': None, 'stack': True, 'extra': {'derived': False}})
            plan.writes.append(f'adopt {rel}')
        if conflicts:
            raise Refused(conflicts)
        txn = _new_txn()
        rec = {'format': FORMAT, 'owner': owner, 'txn': txn, 'installed': _now()[0],
               'note': 'adopted from the markers of before the install ledger', 'contributions': cs}
        removes = [norm_rel(r) for r in remove]
        plan.writes += [f'remove {r}' for r in removes]
        self._begin(owner, 'adopt', txn, [('ledger', owner_file(owner))] + [('game', r) for r in removes])
        try:
            for s, data in sorted(blobs.items()):
                if not os.path.isfile(self.blob_path(s)):
                    self._step(f'blob {s}')
                    write_atomic(self.blob_path(s), data)
            self._step(f'record {owner}')
            self._write_record(owner, rec)
            for r in removes:
                self._step(f'remove {r}')
                _remove(self.path(r))
        except Crashed:
            raise
        except BaseException as e:
            raise self._abort(e) from e
        self._clear_journal()
        recs[owner] = rec
        self._gc(recs)
        plan.applied = True


# ------------------------------------------------------------------------------------------------- legacy markers

LEGACY = {'navkit': ('.opfor-orig', '.opfor-new'),
          'stockmp': ('.opfor-stockmp-orig', '.opfor-stockmp-applied', '.opfor-stockmp-new', '.opfor-stockmp-prev')}


def legacy_refusal(rel: str, markers: list[str]) -> str:
    return (f'{rel} has install markers from before the install ledger ({", ".join(markers)}): migrate them first - '
            'Project OPFOR: python src\\tools\\opforcore\\migrate.py --apply')


def adopt_legacy(led: Ledger, owner: str, rels: list[str], family: str) -> Plan | None:
    """Adopts the legacy markers of family ('navkit' / 'stockmp') beside the game files rels as owner's install,
    removing them (one journalled commit). None when there were none. Refused in words when another family's markers
    are there, or the markers do not describe one finished install."""
    entries: list[Adopt] = []
    remove: list[str] = []
    conflicts: list[str] = []
    own = LEGACY[family]
    for rel in rels:
        rel = norm_rel(rel)
        p = led.path(rel)
        marks = legacy_markers(p)
        if not marks:
            continue
        low = [m.lower() for m in marks]
        if any(m not in own for m in low):
            conflicts.append(legacy_refusal(rel, marks))
            continue
        remove += [rel + m for m in marks]
        if owner in led.holders(rel):
            continue                  # stale markers beside the owner's own install in the ledger: only removed
        cur = _read(p)
        if family == 'navkit':
            if cur is None:
                conflicts.append(f"{rel}: NavKit's markers ({', '.join(marks)}) but no installed file: restore by hand "
                                 f'(copy {rel}.opfor-orig back, or delete the markers when the level had no path data)')
                continue
            entries.append(Adopt(rel, _read(p + '.opfor-orig') if '.opfor-orig' in low else None))
            continue
        if '.opfor-stockmp-prev' in low:
            conflicts.append(f'{rel}: a stock-MP install was interrupted before the install ledger '
                             f'({rel}.opfor-stockmp-prev is the file from before it): put it back as {rel} (or delete '
                             f'it when {rel} is the file to keep), then run again')
            continue
        applied = None
        if '.opfor-stockmp-applied' in low:
            applied = (_read(p + '.opfor-stockmp-applied') or b'').decode('utf-8', 'replace').strip()
        if applied is None:
            if '.opfor-stockmp-orig' in low:
                conflicts.append(f'{rel}: a stock-MP backup ({rel}.opfor-stockmp-orig) with no install record: if {rel} '
                                 "is the game's own file, delete the backup; if it is stock-MP's, copy the backup over it "
                                 'first; then run again')
            continue
        if cur is None:
            conflicts.append(f"{rel}: stock-MP's record but no installed file: copy {rel}.opfor-stockmp-orig back (if "
                             'there is one) and delete the markers, then run again')
            continue
        if applied != sha_hex(cur):
            conflicts.append(f"{rel}: changed after stock-MP's install (its .opfor-stockmp-applied does not match): copy "
                             f"{rel}.opfor-stockmp-orig over it to take it back to the game's own file, or delete the "
                             'stock-MP markers to keep it as it is; then run again')
            continue
        entries.append(Adopt(rel, _read(p + '.opfor-stockmp-orig') if '.opfor-stockmp-orig' in low else None))
    if conflicts:
        raise Refused(conflicts)
    if not remove:
        return None
    return led.adopt(owner, entries, remove)
