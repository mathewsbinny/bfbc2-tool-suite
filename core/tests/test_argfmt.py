r"""Round RT2: argparse's help and usage at the console's width in bfbc2.exe (rust\bc2-cli\src\argfmt.rs) against the
Python command lines' own, for every parser of the tools whose texts were handed to the Python in a console or with
COLUMNS set (caches, convert, audio, shaderdb, renderqa, xdiff, dbxparity, portparity; texqa's --help).

Each case is a command line (its --help, or one that ends in a usage error) run with COLUMNS set to a width (and a few
values of COLUMNS that are not a width: the console's or the fallback 80 is taken then); stdout, stderr and the exit
code are compared. The Python's answers are recorded in argfmt_golden.json (--record), so the comparison does not
need the Python command lines to run.

  python core\tests\test_argfmt.py            bfbc2.exe against the recording, and the generated parser pieces
                                              (rust\bc2-cli\src\argspecs.rs) against the Python parsers
  python core\tests\test_argfmt.py --live     the same, and the Python run again: its answers against the recording
  python core\tests\test_argfmt.py --record   run the Python command lines and write the recording
"""
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(os.path.dirname(HERE))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import native  # noqa: E402

GOLDEN = os.path.join(HERE, 'argfmt_golden.json')
SCRATCH = os.environ.get('BFBC2_TEST_SCRATCH') or tempfile.gettempdir()
RECORD = '--record' in sys.argv
LIVE = '--live' in sys.argv or RECORD
# widths (COLUMNS) around every place the layout changes: argparse's help column (24), its 0.75 rule for the usage, the
# pipe's 80, very narrow and very wide; and values of COLUMNS that are not a width (int() refuses them, or not
# positive: the console's width is asked, here a pipe's, so 80)
WIDTHS = ['10', '24', '33', '40', '47', '55', '62', '70', '79', '80', '81', '90', '100', '120', '160', '250',
          ' 100 ', '1_00', '+64', '0', '-5', 'abc', '']
CASES = [
    ('caches', ['--help']), ('caches', ['--no-such-option']),
    ('convert', ['--help']), ('convert', []),
    *[(c, a) for s in ('havok', 'verify-havok', 'texture', 'verify-texture')
      for c, a in (('convert', [s, '--help']), ('convert', [s]))],
    ('audio', ['--help']), ('audio', []),
    ('shaderdb', ['--help']), ('shaderdb', []),
    *[(c, a) for s in ('info', 'list', 'textures', 'merge', 'cover', 'find', 'needs')
      for c, a in (('shaderdb', [s, '--help']), ('shaderdb', [s]))],
    ('shaderdb', ['roundtrip', '--help']), ('shaderdb', ['roundtrip', '--no-such-option']),
    ('renderqa', ['--help']), ('renderqa', ['--db']),
    ('xdiff', ['--help']), ('xdiff', ['--n']),
    ('dbxparity', ['--help']), ('dbxparity', ['-o']),
    ('portparity', ['--help']), ('portparity', ['-o']),
    ('texqa', ['--help']),
]


def run(cmd: list[str], columns: str, trace: str | None = None) -> dict[str, object]:
    env = dict(os.environ)
    env.pop('BFBC2_CLI_TRACE', None)
    env.pop('BFBC2_CLI_PYTHON', None)
    if trace:
        env['BFBC2_CLI_TRACE'] = trace
    env['PYTHONIOENCODING'] = 'utf-8'
    if columns:
        env['COLUMNS'] = columns
    else:
        env.pop('COLUMNS', None)
    r = subprocess.run(cmd, capture_output=True, env=env, cwd=SUITE, timeout=300)
    return {'exit': r.returncode, 'out': r.stdout.decode('utf-8', 'replace'), 'err': r.stderr.decode('utf-8', 'replace')}


def key(tool: str, args: list[str], columns: str) -> str:
    return json.dumps([tool, args, columns])


def main() -> int:
    exe = native.cli_exe()
    if exe is None and not RECORD:
        print('FAIL  bfbc2.exe is not built from this checkout (python rust\\build_native.py)')
        return 1
    golden: dict[str, dict[str, object]] = {}
    if os.path.isfile(GOLDEN) and not RECORD:
        with open(GOLDEN, encoding='utf-8') as f:
            golden = json.load(f)
    kept = golden
    if RECORD and os.path.isfile(GOLDEN):
        with open(GOLDEN, encoding='utf-8') as f:
            kept = json.load(f)
    # the trace in a folder of this run's own (was SCRATCH/argfmt_trace.txt: two worktrees' runs at once deleted each
    # other's trace before it was read, a FileNotFoundError)
    tmp = tempfile.mkdtemp(prefix='argfmt_', dir=SCRATCH)
    try:
        return compare(exe, golden, os.path.join(tmp, 'trace.txt'), kept)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def compare(exe: str | None, golden: dict[str, dict[str, object]], trace: str,
            kept: dict[str, dict[str, object]]) -> int:
    """kept: the recording of the retired tools' lines (no Python to record them from any more)."""
    failed = 0
    rec: dict[str, dict[str, object]] = {}
    for tool, args in CASES:
        script = native.CLI_TOOLS[tool]
        for w in WIDTHS:
            k = key(tool, args, w)
            if LIVE and tool in native.RETIRED_CLI:
                if k in kept:                        # no Python command line any more: its recording stays
                    rec[k] = kept[k]
            elif LIVE:
                py = run([sys.executable, script, *args], w)
                rec[k] = py
                if not RECORD and golden.get(k) != py:
                    failed += 1
                    print(f'FAIL  the Python against the recording: {k}')
            want = rec.get(k) if RECORD else golden.get(k)
            if want is None:
                failed += 1
                print(f'FAIL  not recorded: {k}')
                continue
            if exe is None:
                continue
            if os.path.exists(trace):
                os.remove(trace)
            got = run([exe, tool, *args], w, trace)
            with open(trace, encoding='utf-8') as f:
                who = f.read().split()
            if who[-1:] != ['rust']:
                failed += 1
                print(f'FAIL  {k}: bfbc2.exe handed it to the Python ({" ".join(who)})')
            elif got != want:
                failed += 1
                if failed <= 8:
                    print(f'FAIL  {k}: exit {got["exit"]} / {want["exit"]}')
                    for part in ('out', 'err'):
                        if got[part] != want[part]:
                            print(f'      {part} (bfbc2.exe):\n{got[part]}      {part} (Python):\n{want[part]}')
    n = len(CASES) * len(WIDTHS)
    if RECORD:
        with open(GOLDEN, 'w', encoding='utf-8', newline='\n') as f:
            json.dump(rec, f, indent=0, sort_keys=True)
            f.write('\n')
        print(f'recorded {len(rec)} answers of the Python command lines')
    gen = subprocess.run([sys.executable, os.path.join(SUITE, 'rust', 'bc2-cli', 'gen_argspecs.py'), '--check'],
                         capture_output=True, text=True)
    if gen.returncode:
        failed += 1
        print('FAIL  ' + (gen.stdout + gen.stderr).strip())
    digest = hashlib.sha1(json.dumps(golden or rec, sort_keys=True).encode()).hexdigest()[:12]
    print(f'{n - failed if failed <= n else 0} of {n} command lines as the Python answers them '
          f'({len(CASES)} cases x {len(WIDTHS)} values of COLUMNS; recording {digest}); argspecs.rs '
          f'{"stale" if gen.returncode else "as the parsers give it"}')
    print('all passed' if not failed else f'{failed} failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
