r"""Build or refresh the private decompiled database of BFBC2Game.exe with Ghidra headless (round DC0;
tools/decomp/README.md).

    python tools/decomp/run_dc.py all       [--db <folder>] [--exe <BFBC2Game.exe>] [--heap 9G] [--threads 4]
    python tools/decomp/run_dc.py import    import and auto-analyse the exe into <db>\ghidra (once; about an hour)
    python tools/decomp/run_dc.py plan      extract_names.py + gen_types.py into <db>\index
    python tools/decomp/run_dc.py apply     ApplyTypes.java then ApplyNames.java on the project (saved)
    python tools/decomp/run_dc.py export    ExportDb.java: the pseudo-C and the raw index (read only)
    python tools/decomp/run_dc.py index     build_index.py: <db>\index\decomp.sqlite

The exe is only read. Everything is written under the database folder, which must not be inside any git checkout
(the script refuses one that is). Ghidra comes from BFBC2_GHIDRA (default C:\Tools\ghidra_12.1.4_PUBLIC), Java
from JAVA_HOME; --heap sets Ghidra's Java heap (GHIDRA_HEADLESS_MAXMEM).
"""
from __future__ import annotations

import argparse
import os
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from decomp import default_db  # noqa: E402
from extract_names import DEFAULT_OPFOR  # noqa: E402

HERE = Path(__file__).resolve().parent
SCRIPTS = HERE / 'ghidra_scripts'
PROJECT = 'BFBC2'
PROGRAM = 'BFBC2Game.exe'


def in_git(p: Path) -> Path | None:
    for up in [p, *p.parents]:
        if (up / '.git').exists():
            return up
    return None


def headless(db: Path, heap: str, args: list[str], log: str) -> int:
    ghidra = Path(os.environ.get('BFBC2_GHIDRA', r'C:\Tools\ghidra_12.1.4_PUBLIC'))
    env = dict(os.environ, GHIDRA_HEADLESS_MAXMEM=heap)
    cmd = [str(ghidra / 'support' / 'analyzeHeadless.bat'), str(db / 'ghidra'), PROJECT, *args]
    (db / 'logs').mkdir(parents=True, exist_ok=True)
    with open(db / 'logs' / log, 'w', encoding='utf-8', errors='replace') as f:
        r = subprocess.run(cmd, stdout=f, stderr=subprocess.STDOUT, env=env)
    print(f'{" ".join(args[:3])} ... exit {r.returncode} (log {db / "logs" / log})')
    return r.returncode


def py(script: str, *args: str) -> int:
    return subprocess.run([sys.executable, str(HERE / script), *args]).returncode


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('step', choices=('all', 'import', 'plan', 'apply', 'export', 'index'))
    ap.add_argument('--db', default='')
    ap.add_argument('--exe', default='')
    ap.add_argument('--heap', default='9G')
    ap.add_argument('--threads', type=int, default=4)
    a = ap.parse_args(argv)
    db = Path(a.db) if a.db else default_db()
    repo = in_git(db.resolve())
    if repo:
        print(f'refused: the database folder {db} is inside the git checkout {repo}; it must stay outside every repo')
        return 2
    idx = db / 'index'
    idx.mkdir(parents=True, exist_ok=True)
    steps = ['import', 'plan', 'apply', 'export', 'index'] if a.step == 'all' else [a.step]
    for s in steps:
        rc = 0
        if s == 'import':
            exe = a.exe or str(DEFAULT_OPFOR / 'game' / PROGRAM)
            (db / 'ghidra').mkdir(parents=True, exist_ok=True)
            rc = headless(db, a.heap, ['-import', exe, '-analysisTimeoutPerFile', '10800'], 'import.log')
        elif s == 'plan':
            old = idx / 'old_project_symbols.jsonl'
            rc = py('extract_names.py', '--out', str(idx / 'names.json'),
                    *(['--old-symbols', str(old)] if old.is_file() else []), *(['--exe', a.exe] if a.exe else []))
            rc = rc or py('gen_types.py', '--out', str(idx / 'types_plan.json'))
        elif s == 'apply':
            rc = headless(db, a.heap, ['-process', PROGRAM, '-noanalysis', '-scriptPath', str(SCRIPTS),
                                       '-postScript', 'ApplyTypes.java', str(idx / 'types_plan.json'),
                                       str(idx / 'types_report.json'),
                                       '-postScript', 'ApplyNames.java', str(idx / 'names.json'),
                                       str(idx / 'names_applied.jsonl')], 'apply.log')
        elif s == 'export':
            rc = headless(db, a.heap, ['-process', PROGRAM, '-noanalysis', '-readOnly', '-scriptPath', str(SCRIPTS),
                                       '-postScript', 'ExportDb.java', str(db), str(a.threads), '90'], 'export.log')
        elif s == 'index':
            rc = py('build_index.py', '--db', str(db))
        if rc:
            return rc
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
