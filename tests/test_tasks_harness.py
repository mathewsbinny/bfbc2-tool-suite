r"""The task registry's agent-harness commands (tasks/build.py next / stale / budget / merge --actual-h, round HARN1),
run on a synthetic registry, slot folder and git worktrees in a temp dir: nothing of the real registry, rounds folder
or repositories is read or written.

  python tests\test_tasks_harness.py
"""
import contextlib
import io
import os
import subprocess
import sys
import tempfile
import time

SUITE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(SUITE, 'tasks'))
import build  # noqa: E402

REGISTRY = '''[meta]
schema = 1

[[category]]
id = "tooling"
name = "Tooling"
definition = "Tools."

[[priority]]
rank = 1
title = "First"
ruled = "2026-11-28"
tasks = ["b", "a"]

[[priority]]
rank = 2
title = "Second"
ruled = "2026-11-28"
lanes = ["next"]

[[task]]
id = "a"
code = "A"
title = "Needs a slot"
lane = "next"
repos = ["suite"]
category = "tooling"
needs_slot = true
est_h = 3.0

[[task]]
id = "b"
code = "B"
title = "No slot, scored"
lane = "next"
repos = ["suite"]
category = "tooling"
score = { d = 2, t = 2, h = 1.5 }

[[task]]
id = "c"
code = "C"
title = "Waits for D"
lane = "next"
repos = ["suite"]
category = "tooling"
depends = ["d"]

[[task]]
id = "d"
code = "D"
title = "Open dependency"
lane = "run"
repos = ["suite"]
category = "tooling"
agent_state = "running"
worktrees = ["{wt_old}", "{wt_new}"]
est_h = 2.0

[[task]]
id = "e"
code = "E"
title = "Running in a dead worktree"
lane = "run"
repos = ["suite"]
category = "tooling"
agent_state = "running"
worktrees = ["{wt_old}"]

[[task]]
id = "f"
code = "F"
title = "Blocked"
lane = "next"
repos = ["suite"]
category = "tooling"
agent_state = "blocked"

[[task]]
id = "g"
code = "G"
title = "Done earlier"
lane = "done"
repos = ["suite"]
category = "tooling"
done = "2026-11-01"
est_h = 4.0
actual_h = 6.0
tokens = 1000

[[task]]
id = "h"
code = "H"
title = "Depends on a done task"
lane = "next"
repos = ["suite"]
category = "tooling"
depends = ["g"]
'''


def git(path: str, *args: str, when: float | None = None) -> None:
    env = dict(os.environ, GIT_AUTHOR_NAME='t', GIT_AUTHOR_EMAIL='t@t', GIT_COMMITTER_NAME='t',
               GIT_COMMITTER_EMAIL='t@t')
    if when is not None:
        env['GIT_COMMITTER_DATE'] = env['GIT_AUTHOR_DATE'] = f'{int(when)} +0000'
    subprocess.run(['git', '-C', path, *args], check=True, capture_output=True, env=env)


def make_repo(path: str, commit_at: float) -> None:
    os.makedirs(path)
    git(path, 'init', '-q')
    with open(os.path.join(path, 'f.txt'), 'w') as f:
        f.write('x')
    git(path, 'add', '.')
    git(path, 'commit', '-q', '-m', 'c', when=commit_at)
    os.utime(os.path.join(path, 'f.txt'), (commit_at, commit_at))
    os.utime(os.path.join(path, '.git'), (commit_at, commit_at))


def run(*argv: str) -> tuple[int, str]:
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        code = build.main(list(argv))
    return code, buf.getvalue()


def main() -> int:
    fails: list[str] = []

    def check(ok: bool, what: str) -> None:
        if not ok:
            fails.append(what)
            print(f'FAIL {what}')

    now = time.time()
    with tempfile.TemporaryDirectory() as tmp:
        tmp = os.path.realpath(tmp)
        wt_old, wt_new = os.path.join(tmp, 'wt_old'), os.path.join(tmp, 'wt_new')
        make_repo(wt_old, now - 3 * 3600)
        make_repo(wt_new, now - 3 * 3600)
        with open(os.path.join(wt_new, 'edit.txt'), 'w') as f:  # an untracked file written just now
            f.write('y')
        reg_path = os.path.join(tmp, 'registry.toml')
        with open(reg_path, 'w', encoding='utf-8', newline='\n') as f:
            f.write(REGISTRY.replace('{wt_old}', wt_old.replace('\\', '/')).replace('{wt_new}', wt_new.replace('\\', '/')))
        rounds = os.path.join(tmp, 'rounds')
        os.makedirs(os.path.join(rounds, '.slots'))
        os.environ['BFBC2_ROUNDS_ROOT'] = rounds
        build.REGISTRY = reg_path
        reg = build.load(reg_path)

        # validation: the new fields are accepted and checked
        with open(os.path.join(tmp, 'CHANGELOG.md'), 'w') as f:
            f.write('# Changelog\n\n## [Unreleased]\n')
        bad = build.problems(reg, build.dump(reg), root=tmp)
        check(not [p for p in bad if 'canonical' not in p and 'category' not in p], f'valid fields rejected: {bad}')
        reg['task'][0]['agent_state'] = 'sleeping'
        reg['task'][0]['est_h'] = 'lots'
        reg['task'][0]['tokens'] = 1.5
        reg['task'][0]['needs_slot'] = 'yes'
        bad = build.problems(reg, build.dump(reg), root=tmp)
        for needle in ('agent_state', 'est_h', 'tokens', 'needs_slot'):
            check(any(needle in p for p in bad), f'{needle} not validated')
        reg = build.load(reg_path)

        # est_h defaults to score.h
        ix = build.index(reg)
        check(build.est_h(ix['a']) == 3.0 and build.est_h(ix['b']) == 1.5 and build.est_h(ix['c']) is None,
              'est_h default')

        # next: priority order, depends done, not running or blocked, slots
        code, out = run('next')
        names = [ln.split()[0] for ln in out.splitlines()[1:] if ln and not ln.startswith('...')]
        check(code == 0 and names == ['B', 'A', 'H'], f'next order {names}')
        check('2 of 2 free' in out and 'build slot free' in out, 'next reports the slot')
        with open(os.path.join(rounds, '.slots', 'slot1'), 'w') as f:
            f.write('x1 2026-11-28T10:00:00')
        with open(os.path.join(rounds, '.slots', 'slot2'), 'w') as f:
            f.write('x2 2026-11-28T10:00:00')
        names = [ln.split()[0] for ln in run('next')[1].splitlines()[1:]]
        check(names == ['B', 'H'], f'next with both slots held {names}')
        old = now - 2 * 3600  # a slot older than 90 minutes is stale, so free
        os.utime(os.path.join(rounds, '.slots', 'slot2'), (old, old))
        names = [ln.split()[0] for ln in run('next')[1].splitlines()[1:]]
        check(names == ['B', 'A', 'H'], f'next with a stale slot {names}')
        check(len(run('next', '--top', '1')[1].splitlines()) == 3, 'next --top 1')

        # stale: D has a fresh file in wt_new, E only the old worktree
        rows = build.stale_rounds(reg, 45)
        check([t['code'] for t, _ in rows] == ['E'], f'stale {[t["code"] for t, _ in rows]}')
        check([t['code'] for t, _ in build.stale_rounds(reg, 45, now + 3 * 3600)] == ['D', 'E'], 'stale later')
        code, out = run('stale', '--minutes', '600')
        check(code == 0 and 'no running round' not in out and 'orphan slot' in out, 'stale --minutes, orphan slots')

        # orphaned slots and acquire processes: x1 names no task, d is running (live), e is quiet
        slots, waiting = build.orphan_slots(reg, rounds, 45, procs=[(11, 'D'), (12, 'E'), (13, 'ghost')])
        check(len(slots) == 2 and 'slot1' in slots[0] and 'names no task' in slots[0], f'orphan slots {slots}')
        check([w.split(':')[0] for w in waiting] == ['pid 12', 'pid 13'], f'orphan acquires {waiting}')
        check(isinstance(build.acquire_processes(), list), 'acquire_processes lists')

        # budget
        code, out = run('budget')
        check(code == 0 and 'Per priority group' in out and 'Total' in out, 'budget output')
        check(build.budget_sums(reg['task'])[1:3] == (1.5 + 3.0 + 2.0 + 4.0, 6.0), 'budget sums')
        check('1.50x' in out, 'budget ratio')

        # merge records actual_h and tokens
        build.CHANGES = os.path.join(tmp, 'changes')
        build.CHANGELOG = os.path.join(tmp, 'CHANGELOG.md')
        build.VERSION = os.path.join(tmp, 'VERSION')
        build.ROADMAP = os.path.join(tmp, 'ROADMAP.md')
        build.BOARD = os.path.join(tmp, 'board.json')
        build.merge(reg, 'D', '2026-11-28', actual_h=2.5, tokens=123456)
        t = build.index(reg)['d']
        check(t['lane'] == 'done' and t['actual_h'] == 2.5 and t['tokens'] == 123456 and t['agent_state'] == 'done',
              'merge fields')
        text = build.dump(reg)
        check('actual_h = 2.5' in text and 'tokens = 123456' in text, 'merge dumped')

    print('FAIL' if fails else 'ok   tasks harness: next, stale, budget, merge fields')
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main())
