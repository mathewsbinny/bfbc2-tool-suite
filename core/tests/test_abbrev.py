r"""argparse's option abbreviations in bfbc2.exe (quick tier, round RT3): the ports of the argparse tools (caches, convert
and its subcommands, audio, shaderdb and its subcommands, renderqa, xdiff, dbxparity, portparity, texqa) resolve a
unique prefix of a long option as Python 3.13's argparse does (`--fo`, `--fo=v`, `--he` for --help) and give its
error for a prefix two or more options share (`ambiguous option: --x could match --xa, --xb`), where they used to
hand the command line to the Python.

The command lines come from the Python parsers themselves (rust\bc2-cli\gen_argspecs.py's capture, subparsers
walked): per parser, each long option's shortest unique prefix (alone, and with its value as `=v` and as the next
argument where it takes one), the --help prefix, and every prefix shared by two or more option strings (alone and with
`=v`). Each line ends with an argument no option matches, so argparse stops with an error (or the help) before the
tool does anything: checked in process on the captured parsers before any line runs. They run at the pipe's width
(COLUMNS unset) with LOCALAPPDATA in a temporary folder.

  python core\tests\test_abbrev.py            bfbc2.exe against the recording (abbrev_golden.json): stdout, stderr,
                                              exit code, and BFBC2_CLI_TRACE must say `rust` for every line
  python core\tests\test_abbrev.py --live     the same, and the Python command lines again against the recording
  python core\tests\test_abbrev.py --record   run the Python command lines and write the recording

texqa's port leaves every argparse error to the Python by design (O4: its errors and the option forms it does not
read), so its lines that end in an error are expected to be handed off; its help prefix is answered.
"""
import argparse
import contextlib
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(os.path.dirname(HERE))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
sys.path.insert(0, os.path.join(SUITE, 'rust', 'bc2-cli'))
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import native  # noqa: E402

GOLDEN = os.path.join(HERE, 'abbrev_golden.json')
RECORD = '--record' in sys.argv
LIVE = '--live' in sys.argv or RECORD
JUNK = '--zz-not-an-option'
HANDED_BY_DESIGN = {'texqa'}           # its argparse errors are the Python's (the port reads only good lines)
# texqa's unique prefixes end with an input that is not there instead: a line that parses, then stops at once with
# texqa's own error (it checks its inputs before anything else)
MISSING = 'zz-no-such-input'


def value_of(a: argparse.Action) -> str:
    """A value the option takes without an error of its own."""
    if a.choices:
        return str(next(iter(a.choices)))
    if a.type is int:
        return '1'
    if a.type is float:
        return '1.5'
    return 'x'


def takes_value(a: argparse.Action) -> bool:
    return a.nargs != 0 and not isinstance(a, argparse._StoreConstAction | argparse._HelpAction)


def parser_lines(p: argparse.ArgumentParser, pre: list[str], end: str = JUNK) -> list[list[str]]:
    """The abbreviation lines of one parser (pre: the subcommand that reaches it)."""
    strings = list(p._option_string_actions)
    longs = [s for s in strings if s.startswith('--')]
    out: list[list[str]] = []

    def matches(prefix: str) -> list[str]:
        return [s for s in strings if s.startswith(prefix)]
    for s in longs:
        a = p._option_string_actions[s]
        for k in range(3, len(s)):
            pre_s = s[:k]
            if matches(pre_s) == [s]:
                out.append([*pre, pre_s, end])
                if takes_value(a):
                    out.append([*pre, f'{pre_s}={value_of(a)}', end])
                    out.append([*pre, pre_s, value_of(a), end])
                break
    shared: set[str] = set()
    for i, s in enumerate(longs):
        for t in longs[i + 1:]:
            n = 0
            while n < min(len(s), len(t)) and s[n] == t[n]:
                n += 1
            lcp = s[:n]
            if len(lcp) >= 3 and lcp not in strings and len(matches(lcp)) >= 2:
                shared.add(lcp)
    for lcp in sorted(shared):
        out.append([*pre, lcp, JUNK])
        out.append([*pre, f'{lcp}=v', JUNK])
    return out


def cases() -> list[tuple[str, list[str], argparse.ArgumentParser]]:
    """(tool, arguments, the main parser) for every line."""
    import gen_argspecs
    out = []
    parsers = {tool: gen_argspecs.capture(tool) for tool in gen_argspecs.TOOLS}     # (some tools reconfigure stdout)
    for tool, p in parsers.items():
        lines = parser_lines(p, [], MISSING if tool in HANDED_BY_DESIGN else JUNK)
        for a in p._actions:
            if isinstance(a, argparse._SubParsersAction):
                for name, sp in a.choices.items():
                    lines += parser_lines(sp, [name])
        out += [(tool, line, p) for line in lines]
    return out


def stops_before_main(p: argparse.ArgumentParser, args: list[str]) -> bool:
    """parse_args ends the program (an error, or the help) on these arguments: the tool's main never acts."""
    with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
        try:
            p.parse_args(args)
        except SystemExit:
            return True
    return False


def run(cmd: list[str], env: dict[str, str]) -> dict[str, Any]:
    r = subprocess.run(cmd, capture_output=True, env=env, cwd=SUITE, timeout=300)
    lad = env['LOCALAPPDATA']

    def norm(b: bytes) -> str:                      # the temporary folder (a log's path) and the day's date out
        return re.sub(r'\d{4}-\d\d-\d\d\.log', '<date>.log', b.decode('utf-8', 'replace').replace(lad, '<lad>'))
    return {'exit': r.returncode, 'out': norm(r.stdout), 'err': norm(r.stderr)}


def main() -> int:
    rows = cases()
    unsafe = [f'{t} {" ".join(a)}' for t, a, p in rows if not stops_before_main(p, a)
              and not (a[-1] == MISSING and not os.path.exists(os.path.join(SUITE, MISSING)))]
    if unsafe:
        print('FAIL  these lines would run the tool (not stopped by argparse); nothing was run: ' + '; '.join(unsafe[:5]))
        return 1
    exe = native.cli_exe()
    if exe is None and not RECORD:
        print('FAIL  bfbc2.exe is not built from this checkout (python rust\\build_native.py)')
        return 1
    golden: dict[str, Any] = {}
    if os.path.isfile(GOLDEN) and not RECORD:
        with open(GOLDEN, encoding='utf-8') as f:
            golden = json.load(f)
    tmp = tempfile.mkdtemp(prefix='abbrev_')
    env = {k: v for k, v in os.environ.items() if k not in ('COLUMNS', 'BFBC2_CLI_TRACE', 'BFBC2_CLI_PYTHON')}
    env.update(LOCALAPPDATA=tmp, BFBC2_FBRB_CACHE='0', PYTHONIOENCODING='utf-8')
    trace = os.path.join(tmp, 'trace.txt')
    rec: dict[str, Any] = {}
    failed = 0
    who: dict[str, int] = {}
    try:
        for tool, args, _ in rows:
            k = json.dumps([tool, args])
            if LIVE:
                py = run([sys.executable, native.CLI_TOOLS[tool], *args], env)
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
            with contextlib.suppress(OSError):
                os.remove(trace)
            got = run([exe, tool, *args], dict(env, BFBC2_CLI_TRACE=trace))
            with open(trace, encoding='utf-8') as f:
                w = (f.read().split() or ['?'])[-1]
            who[f'{tool} {w}'] = who.get(f'{tool} {w}', 0) + 1
            if w != 'rust' and not (tool in HANDED_BY_DESIGN and want['exit'] != 0):
                failed += 1
                print(f'FAIL  {k}: bfbc2.exe handed it to the Python ({w})')
            elif got != want:
                failed += 1
                if failed <= 8:
                    print(f'FAIL  {k}: exit {got["exit"]} / {want["exit"]}\n      bfbc2.exe: {got["err"][-300:]!r}\n'
                          f'      Python:    {want["err"][-300:]!r}')
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    if RECORD:
        with open(GOLDEN, 'w', encoding='utf-8', newline='\n') as f:
            json.dump(rec, f, indent=0, sort_keys=True)
            f.write('\n')
        print(f'recorded {len(rec)} answers of the Python command lines')
    tools = sorted({t for t, _, _ in rows})
    amb = sum('ambiguous option' in (golden or rec).get(json.dumps([t, a]), {}).get('err', '') for t, a, _ in rows)
    print(f'{len(rows) - failed if failed <= len(rows) else 0} of {len(rows)} command lines as the Python answers them '
          f'({len(tools)} tools, {amb} ambiguous); answered by: '
          + ', '.join(f'{k} {v}' for k, v in sorted(who.items())))
    print('all passed' if not failed else f'{failed} failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
