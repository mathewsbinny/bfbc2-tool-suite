r"""DbxEdit's commands in process: bfbc2-dbx-tools\DbxEdit.exe's xml, edit, import, check, roundtrip and fixslots run
by the suite's Rust core (rust\bc2-formats\src\dbxedit, a port of DbxEdit.cs), without starting .NET for each call.
The same files are written, byte for byte, with the same messages and exit codes (rust\BENCHMARKS.md, "E3b").

    from bfbc2core import dbxedit
    r = dbxedit.run(['edit', src, dst, 'drop=' + guid])        # a CompletedProcess: returncode, stdout, stderr
    r = dbxedit.edit(src, dst, ['replace=a|b', '@cmds.txt'])     # the same, by command
    r = dbxedit.import_xml(xml, dst, like=original)
    r = dbxedit.xml(path, ...);  dbxedit.check(level, bundles=[...]);  dbxedit.roundtrip(path)
    rs = dbxedit.run_many([[...], [...]])                         # many command lines on all cores
    r = dbxedit.run_cmd([DBXEDIT_EXE, 'xml', path], capture_output=True, text=True)   # drop-in for subprocess.run

run_cmd takes a command line that starts with DbxEdit.exe's path (as callers build them for subprocess.run) and the
keyword arguments those callers pass (capture_output, text, check, ...): switching a caller is one line. Output is
text with '\n' line ends (as subprocess.run(..., text=True) gives DbxEdit's), or bytes with '\r\n' when text is
false.

BFBC2_DBXEDIT_EXE=1 runs DbxEdit.exe instead (for comparison; DbxEdit.exe must be built: bfbc2-dbx-tools\build.bat).
Without the Rust core (bfbc2core.native: not built, or built from other sources) the exe is used as well, with a
warning once - the exe stays the reference this round.

Each in-process run is logged as DbxEdit logs itself (%LOCALAPPDATA%\bfbc2-tool-suite\logs\dbxedit-<date>.log), its
start line marked "(in process)".
"""
from __future__ import annotations

import datetime
import os
import subprocess
import sys
import warnings
from collections.abc import Iterable, Sequence
from typing import Any

from . import native

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(os.path.dirname(HERE))
EXE = os.path.join(SUITE, 'bfbc2-dbx-tools', 'DbxEdit.exe')
EXE_ENV = 'BFBC2_DBXEDIT_EXE'
NATIVE_ATTR = 'dbxedit_run'

Result = subprocess.CompletedProcess


def use_exe() -> bool:
    """Whether DbxEdit.exe runs instead of the Rust core: BFBC2_DBXEDIT_EXE=1, or no usable Rust core."""
    if os.environ.get(EXE_ENV, '') not in ('', '0'):
        return True
    m = native.load()
    if m is None or not hasattr(m, NATIVE_ATTR):
        if not _state.get('warned'):
            _state['warned'] = True
            warnings.warn('bfbc2core.dbxedit: the Rust core is not available, DbxEdit.exe runs instead', RuntimeWarning,
                          stacklevel=3)
        return True
    return False


_state: dict[str, Any] = {}


def log_path() -> str | None:
    """DbxEdit's log file for today (as DbxEdit's ToolLog names it), or None when the folder cannot be made."""
    try:
        base = os.environ.get('LOCALAPPDATA') or os.path.expanduser('~')
        d = os.path.join(base, 'bfbc2-tool-suite', 'logs')
        os.makedirs(d, exist_ok=True)
        return os.path.join(d, f'dbxedit-{datetime.date.today().isoformat()}.log')
    except OSError:
        return None


def _log(path: str | None, args: Sequence[str], lines: Iterable[tuple[str, str]]) -> None:
    if path is None:
        return
    now = datetime.datetime.now()
    stamp = now.strftime('%Y-%m-%d %H:%M:%S,') + f'{now.microsecond // 1000:03d}'
    text = f'{stamp} INFO    --- dbxedit started (in process): {" ".join(args)} ---\r\n'
    for level, message in lines:
        text += f'{stamp} {level:<7} {message}\r\n'
    try:
        with open(path, 'a', encoding='utf-8', newline='') as f:
            f.write(text)
    except OSError:
        pass                                  # no log: the command still ran


def _result(args: Sequence[str], code: int, out: str, err: str, text: bool, cmd: Sequence[str] | None = None) -> Result:
    if text:
        return subprocess.CompletedProcess(list(cmd or args), code, out, err)
    return subprocess.CompletedProcess(list(cmd or args), code, out.replace('\n', '\r\n').encode('utf-8'),
                                       err.replace('\n', '\r\n').encode('utf-8'))


def _exe(args: Sequence[str], exe: str | None, text: bool) -> Result:
    r = subprocess.run([exe or EXE, *args], capture_output=True, text=text)
    return r


def run(args: Sequence[str], *, text: bool = True, check: bool = False, exe: str | None = None) -> Result:
    """One DbxEdit command line (without the program name), in process unless use_exe(). check: raise
    subprocess.CalledProcessError on a non-zero exit code, as subprocess.run(check=True)."""
    args = [os.fspath(a) for a in args]
    if use_exe():
        r = _exe(args, exe, text)
    else:
        path = log_path()
        code, out, err, lines = native.require('DbxEdit in process', NATIVE_ATTR).dbxedit_run(args, path)
        _log(path, args, lines)
        r = _result(args, code, out, err, text)
    if check:
        r.check_returncode()
    return r


def run_many(runs: Sequence[Sequence[str]], *, text: bool = True) -> list[Result]:
    """Many command lines at once: in process on all cores (none may write a file another reads or writes), else
    DbxEdit.exe one after another. Results in order."""
    runs = [[os.fspath(a) for a in r] for r in runs]
    if use_exe():
        return [_exe(r, None, text) for r in runs]
    path = log_path()
    outs = native.require('DbxEdit in process', NATIVE_ATTR).dbxedit_run_many(runs, path)
    results = []
    for args, (code, out, err, lines) in zip(runs, outs):
        _log(path, args, lines)
        results.append(_result(args, code, out, err, text))
    return results


def is_dbxedit(program: str) -> bool:
    """Whether a command line's program is DbxEdit (DbxEdit.exe by any path, or 'DbxEdit')."""
    name = os.path.basename(os.fspath(program)).lower()
    return name in ('dbxedit.exe', 'dbxedit')


def run_cmd(cmd: Sequence[Any], *, capture_output: bool = False, text: bool = False, check: bool = False,
            encoding: str | None = None, errors: str | None = None, universal_newlines: bool | None = None,
            **_ignored: Any) -> Result:
    """subprocess.run(cmd, ...) for a command line starting with DbxEdit.exe's path: the command runs in process
    (unless use_exe(): then that exe). Other programs run with subprocess.run as given.
    capture_output defaults to False, as subprocess.run's does (True made a forced exe's stdout=DEVNULL call raise
    ValueError, and changed uncaptured callers: OPFOR's switch, 2026-11-17)."""
    cmd = [os.fspath(c) for c in cmd]
    as_text = bool(text or universal_newlines or encoding or errors)
    if not cmd or not is_dbxedit(cmd[0]) or use_exe():
        return subprocess.run(cmd, capture_output=capture_output, text=text, check=check, encoding=encoding,
                              errors=errors, **_ignored)
    path = log_path()
    code, out, err, lines = native.require('DbxEdit in process', NATIVE_ATTR).dbxedit_run(cmd[1:], path)
    _log(path, cmd[1:], lines)
    r = _result(cmd[1:], code, out, err, as_text, cmd)
    if not capture_output:
        # not captured: printed where the exe's output would go, unless the caller sent it elsewhere (DEVNULL ...)
        for stream, sink in (('stdout', sys.stdout), ('stderr', sys.stderr)):
            text_out = out if stream == 'stdout' else err
            target = _ignored.get(stream)
            writer = getattr(target, 'write', None)
            if not text_out:
                pass
            elif target is None and sink is not None:            # sys.stdout is None under pythonw
                sink.write(text_out)
            elif target is subprocess.STDOUT and stream == 'stderr' and sys.stdout is not None:
                sys.stdout.write(text_out)
            elif writer is not None:
                writer(text_out if 'b' not in getattr(target, 'mode', '') else text_out.encode('utf-8'))
            setattr(r, stream, None)
    if check:
        r.check_returncode()
    return r


# ------------------------------------------------------------------------------------------- the commands by name

def xml(*paths: str, **kw: Any) -> Result:
    """DbxEdit xml: <name>.xml next to every binary .dbx of the files and folders."""
    return run(['xml', *paths], **kw)


def edit(src: str, dst: str, commands: Iterable[str], **kw: Any) -> Result:
    """DbxEdit edit: the commands (replace=..., drop=..., setfield=..., '@file' ...) applied to src, written to dst."""
    return run(['edit', src, dst, *commands], **kw)


def import_xml(xml_path: str, dst: str, like: str | None = None, types: Iterable[str] = (), **kw: Any) -> Result:
    """DbxEdit import: the XML as a binary .dbx (exit code 3: written with guessed types). like: the original .dbx
    (default: <xml>.dbx beside it when there is one); types: more files or folders to learn field types from."""
    args = ['import', xml_path, dst]
    if like is not None:
        args += ['--like', like]
    for t in types:
        args += ['--types', t]
    return run(args, **kw)


def check(level: str, slots_from: str | None = None, bundles: Iterable[str] = (), **kw: Any) -> Result:
    """DbxEdit check: problems that crash or break a level (exit code 1: errors found)."""
    args = ['check', level]
    if slots_from is not None:
        args += ['--slots-from', slots_from]
    for b in bundles:
        args += ['--bundle', b]
    return run(args, **kw)


def roundtrip(path: str, **kw: Any) -> Result:
    """DbxEdit roundtrip: read and write every file, report any that change (exit code 1)."""
    return run(['roundtrip', path], **kw)


def fixslots(src: str, dst: str, from_level: str, **kw: Any) -> Result:
    """DbxEdit fixslots: cull slots matched to the level whose cull trees the file uses."""
    return run(['fixslots', src, dst, '--from', from_level], **kw)
