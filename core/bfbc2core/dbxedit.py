r"""DbxEdit's commands: xml, edit, import, check, roundtrip and fixslots, run in process by the suite's Rust core
(rust\bc2-formats\src\dbxedit, a port of DbxEdit.cs), without starting .NET for each call. The same files are written,
byte for byte, with the same messages and exit codes DbxEdit.exe gave (rust\BENCHMARKS.md, "E3b"; the exe was retired
2026-11-18, round R4: core\tests\test_dbxedit.py checks against its recorded answers).

    from bfbc2core import dbxedit
    r = dbxedit.run(['edit', src, dst, 'drop=' + guid])        # a CompletedProcess: returncode, stdout, stderr
    r = dbxedit.edit(src, dst, ['replace=a|b', '@cmds.txt'])     # the same, by command
    r = dbxedit.import_xml(xml, dst, like=original)
    r = dbxedit.xml(path, ...);  dbxedit.check(level, bundles=[...]);  dbxedit.roundtrip(path)
    rs = dbxedit.run_many([[...], [...]])                         # many command lines on all cores
    r = dbxedit.run_cmd(['DbxEdit', 'xml', path], capture_output=True, text=True)   # drop-in for subprocess.run

    python bfbc2-dbx-tools\dbxedit.py <command> ...              # the command line DbxEdit.exe had (main())

run_cmd takes a command line whose program is DbxEdit ('DbxEdit', or DbxEdit.exe by any path: no file needs to be
there, as callers built them for subprocess.run) and the keyword arguments those callers pass (capture_output, text,
check, ...). Output is text with '\n' line ends (as subprocess.run(..., text=True) gave DbxEdit's), or bytes with
'\r\n' when text is false.

The Rust core is required (bfbc2core.native.require: NativeMissing says how to get it); BFBC2_DBXEDIT_EXE, the switch
that ran DbxEdit.exe instead, is gone with the exe.

Each run is logged as DbxEdit logged itself (%LOCALAPPDATA%\bfbc2-tool-suite\logs\dbxedit-<date>.log), its start line
marked "(in process)".
"""
from __future__ import annotations

import datetime
import os
import subprocess
import sys
from collections.abc import Iterable, Sequence
from typing import Any

from . import caches, native

NATIVE_ATTR = 'dbxedit_run'
WHAT = 'DbxEdit\'s commands'

Result = subprocess.CompletedProcess


def log_path() -> str | None:
    """DbxEdit's log file for today (as DbxEdit's ToolLog names it; once it reaches BFBC2_LOG_FILE_MB, the day's next
    part: caches.day_log), or None when the folder cannot be made."""
    try:
        base = os.environ.get('LOCALAPPDATA') or os.path.expanduser('~')
        d = os.path.join(base, 'bfbc2-tool-suite', 'logs')
        os.makedirs(d, exist_ok=True)
        return caches.day_log(d, 'dbxedit', datetime.date.today().isoformat())
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


def _native_run(args: Sequence[str]) -> tuple[int, str, str]:
    path = log_path()
    code, out, err, lines = native.require(WHAT, NATIVE_ATTR).dbxedit_run(list(args), path)
    _log(path, args, lines)
    return code, out, err


def run(args: Sequence[str], *, text: bool = True, check: bool = False) -> Result:
    """One DbxEdit command line (without the program name), in process. check: raise subprocess.CalledProcessError
    on a non-zero exit code, as subprocess.run(check=True)."""
    args = [os.fspath(a) for a in args]
    r = _result(args, *_native_run(args), text)
    if check:
        r.check_returncode()
    return r


def run_many(runs: Sequence[Sequence[str]], *, text: bool = True) -> list[Result]:
    """Many command lines at once, in process on all cores (none may write a file another reads or writes). Results
    in order."""
    runs = [[os.fspath(a) for a in r] for r in runs]
    path = log_path()
    outs = native.require(WHAT, NATIVE_ATTR).dbxedit_run_many(runs, path)
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
    """subprocess.run(cmd, ...) for a command line whose program is DbxEdit: the command runs in process. Other
    programs run with subprocess.run as given. capture_output defaults to False, as subprocess.run's does (True
    changed uncaptured callers: OPFOR's switch, 2026-11-17)."""
    cmd = [os.fspath(c) for c in cmd]
    as_text = bool(text or universal_newlines or encoding or errors)
    if not cmd or not is_dbxedit(cmd[0]):
        return subprocess.run(cmd, capture_output=capture_output, text=text, check=check, encoding=encoding,
                              errors=errors, **_ignored)
    code, out, err = _native_run(cmd[1:])
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


def main(argv: Sequence[str]) -> int:
    """The command line DbxEdit.exe had (bfbc2-dbx-tools\\dbxedit.py): its output printed, its exit code returned.
    Without arguments, the usage."""
    try:
        r = run(list(argv))
    except native.NativeMissing as e:
        print(f'error: {e}', file=sys.stderr)
        return 1
    if r.stdout:
        sys.stdout.write(r.stdout)
    if r.stderr:
        sys.stderr.write(r.stderr)
    return int(r.returncode)
