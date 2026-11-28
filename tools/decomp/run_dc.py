r"""Build or refresh the private decompiled database of BFBC2Game.exe with Ghidra headless (round DC0;
tools/decomp/README.md).

    python tools/decomp/run_dc.py all       [--db <folder>] [--exe <BFBC2Game.exe>] [--heap 9G] [--threads 4]
    python tools/decomp/run_dc.py import    import and auto-analyse the exe into <db>\ghidra (once; about an hour)
    python tools/decomp/run_dc.py plan      extract_names.py + gen_types.py into <db>\index
    python tools/decomp/run_dc.py apply     ApplyTypes.java then ApplyNames.java on the project (saved)
    python tools/decomp/run_dc.py discover  DiscoverFunctions.java: functions in what analysis left outside any
                                            (saved); --skip FIRST-LAST (hex, default 0124F000-012BFFFF: a region
                                            auto-analysis disassembled out of non-code bytes) is left alone
    python tools/decomp/run_dc.py confirm   ConfirmFunctions.java (read only): how each function was found
                                            (analysis / orphan / prologue) and whether the doubtful ones (prologue,
                                            and all in the --skip range) are referenced: <db>\index\provenance.jsonl
    python tools/decomp/run_dc.py export    ExportDb.java: the pseudo-C and the raw index (read only), then the
                                            measurement; --range FIRST-LAST (hex) redoes part of the address
                                            space into <db>\index\functions_<FIRST>.jsonl (it wins over functions.jsonl),
                                            --timeout the seconds per function (default 90)
    python tools/decomp/run_dc.py measure   ExportDb.java with threads 0: index\measure.json only
    python tools/decomp/run_dc.py index     build_index.py: <db>\index\decomp.sqlite

The exe is only read. Everything is written under the database folder, which must not be inside any git checkout
(the script refuses one that is) except the private repository itself (round DOC1: the folder is its root, it has
no remote, and its .gitignore leaves ghidra, pseudo, index and logs untracked). Ghidra comes from BFBC2_GHIDRA (default C:\Tools\ghidra_12.1.4_PUBLIC), Java
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


WRITTEN = ('ghidra', 'pseudo', 'index', 'logs')     # the folders the steps write under the database folder


def private_repo(repo: Path, db: Path) -> bool:
    """The private decompilation repository (round DOC1): the database folder is that checkout's root, it has no
    remote (it must never get one), and its .gitignore leaves every folder the steps write untracked."""
    git = repo / '.git'
    if repo != db or not git.is_dir():
        return False
    try:
        config = (git / 'config').read_text(encoding='utf-8', errors='replace')
        ignored = {ln.strip().strip('/') for ln in (repo / '.gitignore').read_text(encoding='utf-8').splitlines()}
    except OSError:
        return False
    return '[remote' not in config and all(w in ignored for w in WRITTEN)


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
    ap.add_argument('step', choices=('all', 'import', 'plan', 'apply', 'discover', 'confirm', 'export', 'measure',
                                       'index'))
    ap.add_argument('--db', default='')
    ap.add_argument('--exe', default='')
    ap.add_argument('--heap', default='9G')
    ap.add_argument('--threads', type=int, default=4)
    ap.add_argument('--timeout', type=int, default=90)
    ap.add_argument('--skip', default='0124F000-012BFFFF')
    ap.add_argument('--range', default='', help='FIRST-LAST, hex: export only functions whose entry lies there')
    a = ap.parse_args(argv)
    db = Path(a.db) if a.db else default_db()
    repo = in_git(db.resolve())
    if repo and not private_repo(repo, db.resolve()):
        print(f'refused: the database folder {db} is inside the git checkout {repo}; it must stay outside every repo')
        return 2
    idx = db / 'index'
    idx.mkdir(parents=True, exist_ok=True)
    steps = ['import', 'plan', 'apply', 'discover', 'confirm', 'export', 'index'] if a.step == 'all' else [a.step]
    for s in steps:
        rc = 0
        if s == 'import':
            exe = a.exe or str(DEFAULT_OPFOR / 'game' / PROGRAM)
            (db / 'ghidra').mkdir(parents=True, exist_ok=True)
            rc = headless(db, a.heap, ['-import', exe, '-analysisTimeoutPerFile', '10800'], 'import.log')
        elif s == 'plan':
            old = idx / 'old_project_symbols.jsonl'
            rc = py('extract_names.py', '--out', str(idx / 'names.json'), '--private', str(db),
                    *(['--old-symbols', str(old)] if old.is_file() else []), *(['--exe', a.exe] if a.exe else []))
            rc = rc or py('gen_types.py', '--out', str(idx / 'types_plan.json'))
        elif s == 'apply':
            rc = headless(db, a.heap, ['-process', PROGRAM, '-noanalysis', '-scriptPath', str(SCRIPTS),
                                       '-postScript', 'ApplyTypes.java', str(idx / 'types_plan.json'),
                                       str(idx / 'types_report.json'),
                                       '-postScript', 'ApplyNames.java', str(idx / 'names.json'),
                                       str(idx / 'names_applied.jsonl')], 'apply.log')
        elif s == 'discover':
            rc = headless(db, a.heap, ['-process', PROGRAM, '-noanalysis', '-scriptPath', str(SCRIPTS),
                                       '-postScript', 'DiscoverFunctions.java', str(idx / 'discover_report.json'),
                                       str(idx / 'discovered.jsonl'), *a.skip.replace('0x', '').split('-')],
                          'discover.log')
        elif s == 'confirm':
            rc = headless(db, a.heap, ['-process', PROGRAM, '-noanalysis', '-readOnly', '-scriptPath', str(SCRIPTS),
                                       '-postScript', 'ConfirmFunctions.java', str(idx / 'discovered.jsonl'),
                                       str(idx / 'provenance.jsonl'), *a.skip.replace('0x', '').split('-')],
                          'confirm.log')
        elif s in ('export', 'measure'):
            rng = a.range.replace('0x', '').split('-') if a.range and s == 'export' else []
            if rng and (int(rng[0], 16) & 0xFFFF or (int(rng[1], 16) + 1) & 0xFFFF):
                print('refused: --range must start and end on 64 KiB boundaries (a pseudo-C file holds 64 KiB)')
                return 2
            threads = str(a.threads) if s == 'export' else '0'
            log = 'measure.log' if s == 'measure' else f'export{"_" + rng[0] if rng else ""}.log'
            rc = headless(db, a.heap, ['-process', PROGRAM, '-noanalysis', '-readOnly', '-scriptPath', str(SCRIPTS),
                                       '-postScript', 'ExportDb.java', str(db), threads, str(a.timeout), *rng], log)
        elif s == 'index':
            rc = py('build_index.py', '--db', str(db))
        if rc:
            return rc
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
