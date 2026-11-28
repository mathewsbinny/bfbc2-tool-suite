"""In-game tests: how they plug in. The harness never starts the game; these read what a run the user made left behind.

Now (offline reading of a finished run):
  sessions(text)        the sessions of an opfor.log (one per "---- opfor.dll loaded" line): level, session states,
                        AI events, SectorOffsets loads and override, unit-counter pathfinder counts, crash, exit
  evaluate(s, asset)    the three in-game criteria of an AI level (STOCK_MP_AI_PLAN.md 6.3 / 8, in battlefield-opfor's
                        docs/plans/opfor/) as checks.Results: loads-ai-init, offsets-read, pathfinders
  harness.py import-log <run> --level <lv> [--log <opfor.log>] adds them to a saved run.

ON HOLD - needs playtest (designed, not run by the harness):
  testrun        `testrun.bat <level>` (OPFOR) starts the level, follows opfor.log and reports in game / crashed /
                 timed out / exited (exit codes 0-4). The harness will call it per level of a run with --yes and
                 [dev] unit_counters_dump=1, then import the session as above. Kept out until a play test confirms
                 the sequence on a stock MP map (campaign levels stall on a black screen through the dev trigger).
  telemetry (M6) OPFOR's telemetry module streams JSON lines over 127.0.0.1 (DESIGN.md 5.4); a recording of it
                 feeds bot traversal (bots reach given points within a time), stuck detection (no progress for
                 N seconds), squad coordination and vehicle use - TELEMETRY_TESTS below, each 'hold' until the
                 stream exists.
"""
import bisect
import os
import re
import sys
from dataclasses import dataclass, field
from types import ModuleType

from .checks import Result

_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

START = re.compile(r'---- opfor\.dll loaded into PID (\d+) ----')
STAMP = re.compile(r'^\[(\d\d:\d\d:\d\d)\.\d+\]')
LEVEL = re.compile(r"(?:queued hostMultiplayerLocal request|dev trigger): level='([^']+)'")
STATE = re.compile(r'Session::updateState.*arg1=([0-9A-F]{8})')
OFFSET_LOAD = re.compile(r'sector offsets load handle .*X\(\+1Ch\)=(-?\d+) Z\(\+18h\)=(-?\d+)')
OVERRIDE = re.compile(r'sector offsets: on - X=(-?\d+) Z=(-?\d+)')
COUNTERS = re.compile(r'unit counters \((interval|exit)\): (\d+) pathfinders')
EVENTS = [(re.compile(r'AI: server level load'), 'server level load'), (re.compile(r'AI: init'), 'AI init'),
          (re.compile(r'AI: set active'), 'AI active'), (re.compile(r'AI: path data world'), 'path data loaded'),
          (re.compile(r'AI: Kynapse world'), 'navigation world built'), (re.compile(r'AI: update \(per frame\)'), 'AI running')]
# a literal every line sessions() acts on holds (each pattern above has one; and the crash / exit marks): the other
# lines are only counted (round E3m: 13 searches per line were 10 s of a 349 MB opfor.log)
ANY = re.compile(r"opfor\.dll loaded into PID |level='|Session::updateState|sector offsets|unit counters \("
                 r'|AI: (?:server level load|init|set active|path data world|Kynapse world|update \(per frame\))'
                 r'|crashreport\.xml|exit\(0\) called')
LITERALS = ('opfor.dll loaded into PID ', "level='", 'Session::updateState', 'sector offsets', 'unit counters (',
            'AI: server level load', 'AI: init', 'AI: set active', 'AI: path data world', 'AI: Kynapse world',
            'AI: update (per frame)', 'crashreport.xml', 'exit(0) called')     # ANY's alternatives, as text
OTHER_BREAKS = re.compile(r'[\r\x0b\x0c\x1c\x1d\x1e\x85\u2028\u2029]')   # str.splitlines' line breaks other than \n
BREAK_CHARS = tuple(chr(c) for c in (0x0B, 0x0C, 0x1C, 0x1D, 0x1E, 0x85, 0x2028, 0x2029))   # OTHER_BREAKS but CR
IN_GAME = 3                                     # Session state 3: in game (testrun.py)
TELEMETRY_TESTS = [('bot-traversal', 'bots reach given points within a time'),
                   ('stuck', 'no bot without progress for N seconds'),
                   ('squads', 'squad members keep together and follow their orders'),
                   ('vehicles', 'bots enter and drive vehicles')]


@dataclass
class Session:
    pid: str
    start: str = ''
    level: str | None = None
    states: list[int] = field(default_factory=list)
    events: list[str] = field(default_factory=list)
    offset_loads: list[tuple] = field(default_factory=list)
    override: tuple | None = None
    pathfinders: list[int] = field(default_factory=list)
    crashed: bool = False
    exited: bool = False
    lines: int = 0

    @property
    def in_game(self) -> bool:
        return IN_GAME in self.states


def _native() -> 'ModuleType | None':
    """bc2_native with the harness functions (crash-reader\\harnesscore.py), or None."""
    suite_paths.use('crash-reader')
    import harnesscore
    return harnesscore.native()


def _from_tuples(rows: list) -> list[Session]:
    return [Session(pid, start, level, list(states), list(events), [tuple(o) for o in loads],
                    tuple(override) if override is not None else None, list(pf), crashed, exited, lines)
            for pid, start, level, states, events, loads, override, pf, crashed, exited, lines in rows]


def sessions(text: str) -> list[Session]:
    """The sessions of an opfor.log's text: the Rust core's (round E4f, rust\\bc2-harness: the text split and searched
    on all cores) when it is built and takes the text, else sessions_python."""
    nat = _native()
    if nat is not None:
        rows = nat.harness_sessions(text)
        if rows is not None:
            return _from_tuples(rows)
    return sessions_python(text)


def sessions_of_file(path: str) -> list[Session]:
    """sessions(open(path, encoding='utf-8', errors='replace').read()): with the Rust core the file is read and decoded
    there (no Python text of the whole log: 2.4 s of a 349 MB log)."""
    nat = _native()
    if nat is not None:
        try:
            rows = nat.harness_sessions_file(path)
        except OSError:
            rows = None                 # the file not readable: Python's open raises the error, in its words
        if rows is not None:
            return _from_tuples(rows)
    with open(path, encoding='utf-8', errors='replace') as fh:
        return sessions_python(fh.read())


def sessions_python(text: str) -> list[Session]:
    """The sessions of an opfor.log's text. Only the lines ANY finds are read by sessions_reference's rules, one
    by one; between them the lines are counted (the same Sessions: tests/test_harness.py). A text without a
    carriage return (a log read in text mode) is searched whole: its line breaks are single characters, newlines
    counted with str.count and the rarer breaks str.splitlines knows (OTHER_BREAKS, a handful in a game log) by
    their offsets."""
    if '\r' in text:
        return _sessions_by_line(text)
    nl = chr(10)
    rare = _find_all(text, BREAK_CHARS)        # (str.find per character: the class's regex is 1 s on 349 MB)

    def line_start(pos: int) -> int:           # the offset after the last line break before pos
        k = bisect.bisect_left(rare, pos)
        return max(text.rfind(nl, 0, pos), rare[k - 1] if k else -1) + 1

    def line_end(pos: int) -> int:             # the offset of the first line break at or after pos (or the end)
        e = text.find(nl, pos)
        k = bisect.bisect_left(rare, pos)
        r = rare[k] if k < len(rare) else len(text)
        return min(len(text) if e < 0 else e, r)

    def breaks(a: int, b: int) -> int:          # the line breaks in text[a:b]
        return text.count(nl, a, b) + bisect.bisect_left(rare, b) - bisect.bisect_left(rare, a)
    out: list[Session] = []
    cur: Session | None = None
    prev = -1                                   # the line number of the last line read
    at_line, at = 0, 0                          # the line number at offset `at` (a line start)
    for pos in _hits(text):
        start = line_start(pos)
        if start < at or (start == at and prev == at_line):
            continue                            # a second match on a line already read
        k = at_line + breaks(at, start)
        if cur is not None:
            cur.lines += k - prev - 1          # the lines in between: counted, nothing else
        prev, at_line, at = k, k, start
        cur = _line(text[start:line_end(pos)], cur, out)
    total = breaks(0, len(text)) + (1 if text and line_start(len(text)) < len(text) else 0)
    if cur is not None:
        cur.lines += total - prev - 1
    return out


def _hits(text: str) -> list[int]:
    """The offsets of every occurrence of ANY's literals, ascending (str.find per literal: a regular expression of
    alternatives scans a 349 MB log at a fraction of str.find's speed)."""
    return _find_all(text, LITERALS)


def _find_all(text: str, needles: 'tuple[str, ...]') -> list[int]:
    """The offsets where any of needles occurs, ascending, each once."""
    found: set[int] = set()
    for lit in needles:
        i = text.find(lit)
        while i >= 0:
            found.add(i)
            i = text.find(lit, i + 1)
    return sorted(found)


def _sessions_by_line(text: str) -> list[Session]:
    """sessions() on str.splitlines' lines (any line break)."""
    out: list[Session] = []
    cur: Session | None = None
    lines = text.splitlines()
    hits = [k for k, line in enumerate(lines) if ANY.search(line)]
    prev = -1
    for k in hits:
        if cur is not None:
            cur.lines += k - prev - 1          # the lines in between: counted, nothing else
        prev = k
        cur = _line(lines[k], cur, out)
    if cur is not None:
        cur.lines += len(lines) - prev - 1
    return out


def sessions_reference(text: str) -> list[Session]:
    """sessions as it was before round E3m: every pattern searched on every line."""
    out: list[Session] = []
    cur: Session | None = None
    for line in text.splitlines():
        cur = _line(line, cur, out)
    return out


def _line(line: str, cur: 'Session | None', out: list[Session]) -> 'Session | None':
    """One line of the log: a new session (appended to out) or what the line says about the current one."""
    m = START.search(line)
    if m:
        cur = Session(m.group(1))
        t = STAMP.match(line)
        cur.start = t.group(1) if t else ''
        out.append(cur)
        return cur
    if cur is None:
        return None
    cur.lines += 1
    m = LEVEL.search(line)
    if m:
        cur.level = m.group(1)
    m = STATE.search(line)
    if m:
        s = int(m.group(1), 16)
        if not cur.states or cur.states[-1] != s:
            cur.states.append(s)
    for rx, label in EVENTS:
        if label not in cur.events and rx.search(line):
            cur.events.append(label)
    m = OFFSET_LOAD.search(line)
    if m:
        cur.offset_loads.append((int(m.group(1)), int(m.group(2))))
    m = OVERRIDE.search(line)
    if m:
        cur.override = (int(m.group(1)), int(m.group(2)))
    m = COUNTERS.search(line)
    if m:
        cur.pathfinders.append(int(m.group(2)))
    if 'crashreport.xml' in line:
        cur.crashed = True
    if 'exit(0) called' in line:
        cur.exited = True
    return cur


def for_level(all_sessions: list[Session], lv: str) -> list[Session]:
    """The sessions that started the level (latest last)."""
    want = lv.lower().split('/')[-1]
    return [s for s in all_sessions if s.level and s.level.lower().split('/')[-1] == want]


def evaluate(s: Session, asset: tuple[int, int] | None, vehicle: tuple[int, int] | None = None) -> dict[str, Result]:
    """{name: Result} for one session. asset: the level's Infantry SectorOffsets (x, z), or None; vehicle: its
    Vehicle SectorOffsets (x, z) where known - the loads (infantry and vehicle alike in the log) may read either."""
    src = f'opfor.log session {s.start} (PID {s.pid})'
    out: dict[str, Result] = {}
    ok = s.in_game and 'AI init' in s.events and not s.crashed
    out['loads-ai-init'] = Result('pass' if ok else 'fail',
                                  f'{src}: ' + ('crashed' if s.crashed else
                                                f'{"in game" if s.in_game else "never in game"}, AI init {"seen" if "AI init" in s.events else "not seen"}'),
                                  {'in_game': int(s.in_game), 'ai_init': int('AI init' in s.events), 'crashed': int(s.crashed)},
                                  [f'events: {", ".join(s.events) or "none"}'])
    if not s.offset_loads:
        out['offsets-read'] = Result('hold', f'{src}: no SectorOffsets load lines (needs OPFOR\'s probes, [dev] probes=1)')
    else:
        reads = sorted(set(s.offset_loads))
        want = tuple(asset) if isinstance(asset, tuple) else None
        allowed = {want} | ({tuple(vehicle)} if isinstance(vehicle, tuple) else set())
        # every load must read the Infantry or the Vehicle asset's values (the log does not say which kind a load
        # is; stock-MP AI levels can give the two kinds different offsets); with the Infantry asset unknown there is
        # nothing to compare with (the level not in the run, the run made without the level tests): held, not failed
        good = want is not None and all(r in allowed for r in s.offset_loads) and s.override is None
        why = (f'loads read {", ".join(f"{x},{z}" for x, z in reads)}' + (f'; override on ({s.override[0]},{s.override[1]})' if s.override else '')
               + (f'; the asset says {want[0]},{want[1]}' if want else '; the asset\'s values are not known')
               + (f' (Vehicle {vehicle[0]},{vehicle[1]})' if want and isinstance(vehicle, tuple) and tuple(vehicle) != want else ''))
        out['offsets-read'] = Result('hold' if want is None else 'pass' if good else 'fail', f'{src}: {why}',
                                     {'loads': len(s.offset_loads), 'override': int(s.override is not None)})
    if not s.pathfinders:
        out['pathfinders'] = Result('hold', f'{src}: no unit-counter lines (set [dev] unit_counters_dump=1 for the run)')
    else:
        peak = max(s.pathfinders)
        out['pathfinders'] = Result('pass' if peak > 0 else 'fail',
                                    f'{src}: pathfinders {" -> ".join(map(str, s.pathfinders[:8]))}' + (' ...' if len(s.pathfinders) > 8 else ''),
                                    {'pathfinders_peak': peak})
    return out


def telemetry_tests() -> dict[str, Result]:
    return {name: Result('hold', f'{what} - ON HOLD: needs OPFOR\'s telemetry module (milestone M6)') for name, what in TELEMETRY_TESTS}
