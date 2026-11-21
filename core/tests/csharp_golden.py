r"""What the retired C# tools answered (round R4, 2026-11-18): DbxEdit.exe's and FbrbTool.exe's outputs, messages,
exit codes and files, recorded from the exes themselves before they were retired (bfbc2-map-editor/
EDITOR_ARCHITECTURE.md 3.8; RETIRED.md), so the tests keep checking the in-process tools (bfbc2core.dbxedit /
bfbc2core.fbrbtool, the Rust core) against fixed answers.

    dbxedit_golden.json        test_dbxedit's cases (DbxTests.cs' unit cases as command lines)
    dbxedit_golden_game_0.json / _1.json   a corpus sample (OPFOR's work\levels, work\navkit\common, work\ww2) through every
                               command, keyed by each input's SHA-1
    fbrbtool_golden.json       test_fbrbtool's command lines, its damaged bundles, .NET's gzip reader on the seeded
                               streams; test_fbrb_unpack_native's and test_fbrb_pack_native's built bundles and folders;
                               test_fbrb_native's built bundles
    fbrbtool_golden_game.json  every bundle of the game copy up to 300 MiB (unpack, list, pack of the unpacked folder),
                               keyed by the bundle's SHA-1

Digests are the first 16 hex digits of a SHA-256 (digest()); a folder's is tree_digest() over every file's path (case
kept) and SHA-1, empty folders too. Printed text has '\n' line ends, the test folder as '<dir>' and temporary files'
numbers as '.tmp-N'.
"""
from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import tempfile
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))


def path(name: str) -> str:
    return os.path.join(HERE, name)


def load(name: str) -> dict[str, Any]:
    """A golden file, or {} when it is not there (a test then fails saying so)."""
    try:
        with open(path(name), encoding='utf-8') as f:
            data = json.load(f)
        return data if isinstance(data, dict) else {}
    except (OSError, ValueError):
        return {}


def save(name: str, data: dict[str, Any]) -> None:
    """data as JSON, one entry of each top-level section per line (diffs stay readable)."""
    lines = ['{']
    sections = list(data.items())
    for k, (section, value) in enumerate(sections):
        if isinstance(value, dict) and value:
            lines.append(f'{json.dumps(section)}: {{')
            items = list(value.items())
            for j, (key, v) in enumerate(items):
                lines.append(f'{json.dumps(key)}: {json.dumps(v, ensure_ascii=False)}' + (',' if j < len(items) - 1 else ''))
            lines.append('}' + (',' if k < len(sections) - 1 else ''))
        else:
            lines.append(f'{json.dumps(section)}: {json.dumps(value, ensure_ascii=False)}' + (',' if k < len(sections) - 1 else ''))
    lines.append('}')
    with open(path(name), 'w', encoding='utf-8', newline='\n') as f:
        f.write('\n'.join(lines) + '\n')


def digest(data: bytes | str) -> str:
    if isinstance(data, str):
        data = data.encode('utf-8')
    return hashlib.sha256(data).hexdigest()[:16]


def sha1_file(p: str) -> str:
    h = hashlib.sha1()
    with open(p, 'rb') as fh:
        for b in iter(lambda: fh.read(1 << 20), b''):
            h.update(b)
    return h.hexdigest()


def snapshot(folder: str) -> dict[str, str]:
    """Every file under folder: its SHA-1 (empty folders too, as '<dir>'); {} when there is no folder."""
    out = {}
    for d, ds, fs in os.walk(folder):
        if not ds and not fs and d != folder:
            out[os.path.relpath(d, folder) + os.sep] = '<dir>'
        for f in fs:
            out[os.path.relpath(os.path.join(d, f), folder)] = sha1_file(os.path.join(d, f))
    return out


def tree_digest(folder: str) -> str:
    """digest() of snapshot(folder): every path (case kept) and its SHA-1, in path order."""
    snap = snapshot(folder) if os.path.isdir(folder) else {}
    return digest(''.join(f'{k}\t{v}\n' for k, v in sorted(snap.items())))


def tmp_names(s: str) -> str:
    """Temporary files' names without their process and counter numbers (<out>.tmp-<pid>[-<n>])."""
    return re.sub(r'\.tmp-\d+(-\d+)?', '.tmp-N', s)


def logs() -> str:
    """The folder the tools log into (%LOCALAPPDATA%\\bfbc2-tool-suite\\logs)."""
    return os.path.join(os.environ.get('LOCALAPPDATA') or os.path.expanduser('~'), 'bfbc2-tool-suite', 'logs')


def text(s: str | bytes, *places: tuple[str, str]) -> str:
    """Printed text as recorded: '\n' line ends, each (path, name) of places replaced by its name (the longest path
    first), the log file an error names as '<log>' (its folder and day differ), temporary files' numbers taken out."""
    if isinstance(s, bytes):
        s = s.decode('utf-8', 'replace')
    s = s.replace('\r\n', '\n')
    for p, name in sorted(places, key=lambda x: -len(x[0])):
        if p:
            s = s.replace(p, name)
    s = re.sub(r'<logs>\\[^\s\\]+\.log', '<log>', s.replace(logs(), '<logs>'))
    return tmp_names(s)


def case_root(length: int, prefix: str) -> tuple[str, str]:
    """(a new temporary folder, a folder inside it whose path is `length` characters long when the temporary
    folder's path is shorter): the recorded cases ran in such a folder, and FbrbTool's path-length rules (MAX_PATH)
    depend on it. When the temporary folder's path is already longer, the inner folder is as short as it can be."""
    root = tempfile.mkdtemp(prefix=prefix)
    pad = max(1, length - len(root) - 1)
    return root, os.path.join(root, 'c' * pad)


def fresh(folder: str) -> None:
    shutil.rmtree(folder, ignore_errors=True)
    os.makedirs(folder)
