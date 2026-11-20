r"""Regression tests for defects found in a code review (2026-10-26), no game data:

- NavKit's apply (a build with the navigation side file's walkability edits) builds with the same inputs as a normal
  build - detailed zones and ladders included (it used to leave both out);
- the stock-MP installer reinstalls a rebuilt file (it used to say "already" when the installed file matched its own
  install stamp, without comparing it with the new build) and keeps the original as the backup, not an earlier build;
- the navigation-data cache key depends on the terrain's content (it used to record only whether there was one).

  python tests\test_review_fixes.py
"""
import contextlib
import io
import os
import shutil
import sys
import tempfile
from typing import Any

SUITE = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('navkit-tools', 'navkit', 'core')


class Checks:
    def __init__(self):
        self.failed = 0

    def check(self, ok, what, detail=''):
        self.failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''))


class Stop(Exception):
    pass


def navkit_apply(c, work):
    import navbuild
    import navkit
    from bfbc2core import navtune
    seen = {}

    class Pr:
        root = work

        def out_dir(self, lv):
            return work

    class Tune:
        kind = 'navkit'
        operations = [{'op': 'walkable'}]

    def fake_inputs(pr, lv, log=print):
        return 's.bin', 'forbid.npy', 1, 2, dict(detailed={'sectors': {1}}, ladders=[{'a': 1}], seeds=[], log=log)

    def fake_load(path: str) -> Any:                 # navtune.load's stand-in: a Tune, not a NavTune
        return Tune()

    def fake_build(*a, **kw):
        seen.update(kw)
        raise Stop
    side = os.path.join(work, 'lv.navtune.json')
    open(side, 'w').write('{}')
    saved = navkit.build_inputs, navbuild.build, navtune.load
    navkit.build_inputs, navbuild.build, navtune.load = fake_inputs, fake_build, fake_load
    try:
        with contextlib.redirect_stdout(io.StringIO()):
            navkit._apply(Pr(), 'lv', side, None, lambda *a: None)
    except Stop:
        pass
    finally:
        navkit.build_inputs, navbuild.build, navtune.load = saved
    c.check(seen.get('detailed') == {'sectors': {1}} and seen.get('ladders') == [{'a': 1}] and 'edits' in seen,
            'navkit apply: built with the normal build\'s inputs (detailed zones, ladders) plus the edits', str(seen.keys()))

    # a failed apply without --out still keeps its report beside the default output
    def failing_build(*a, **kw):
        raise navkit.NavKitError('the build failed here')
    navkit.build_inputs, navbuild.build, navtune.load = fake_inputs, failing_build, fake_load
    report = os.path.join(work, 'pathdata.hpd.report.txt')
    try:
        with contextlib.redirect_stdout(io.StringIO()):
            navkit.apply(Pr(), 'lv', side, None, log=lambda *a: None)
    except navkit.NavKitError:
        pass
    finally:
        navkit.build_inputs, navbuild.build, navtune.load = saved
    c.check(os.path.isfile(report) and 'error: the build failed here' in open(report, encoding='utf-8').read(),
            'navkit apply: a failure without --out still writes <output>.report.txt with the error')




def write_build_report(pipeline, paths):
    """The build_report.json a finished build writes: every file of out\\game with its sha256 (pipeline.check_report
    refuses a build without one - the 2026-11-14 QA fix)."""
    import json
    out = os.path.join(paths.root, 'out', 'game')
    outputs = {os.path.relpath(os.path.join(dp, f), paths.root): pipeline.sha(os.path.join(dp, f))
               for dp, _d, fs in os.walk(out) for f in fs}
    with open(paths.report(), 'w', encoding='utf-8') as fh:
        json.dump({'outputs': outputs}, fh)


def stockmp_install(c, work):
    suite_paths.use('stockmp')
    import pipeline
    import stockmp
    game = os.path.join(work, 'game')
    paths = pipeline.Paths('mp_test', opfor=work, work=os.path.join(work, 'stockmp'), game=game)
    out = os.path.join(paths.root, 'out', 'game', 'Levels')
    os.makedirs(out)
    os.makedirs(os.path.join(game, 'Levels'))
    built, dst = os.path.join(out, 'level.fbrb'), os.path.join(game, 'Levels', 'level.fbrb')
    open(dst, 'wb').write(b'original')
    run = (lambda: stockmp.install(paths, ['stockmp.py', 'install', 'mp_test', '--apply']))
    open(built, 'wb').write(b'build 1')
    write_build_report(pipeline, paths)
    with contextlib.redirect_stdout(io.StringIO()):
        run()
    open(built, 'wb').write(b'build 2')
    write_build_report(pipeline, paths)
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        run()
    c.check('installed' in buf.getvalue() and open(dst, 'rb').read() == b'build 2',
            'stock-MP install: a rebuilt file is installed again, not reported "already"', buf.getvalue().strip())
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        run()
    c.check('already' in buf.getvalue(), 'stock-MP install: an unchanged build is "already" installed', buf.getvalue().strip())
    c.check(open(dst + stockmp.BACKUP, 'rb').read() == b'original', 'stock-MP install: the backup stays the original')
    with contextlib.redirect_stdout(io.StringIO()):
        stockmp.install(paths, ['stockmp.py', 'install', 'mp_test', '--revert'])
    c.check(open(dst, 'rb').read() == b'original', 'stock-MP install: revert restores the original after a reinstall')


def navdata_key(c, work):
    import numpy as np
    from bfbc2core import navdata, terrain
    p = os.path.join(work, 'x.hpd')
    open(p, 'wb').write(b'x')

    def hf(v):
        raw = np.full((4, 4), v, np.uint16)
        return terrain.Heightfield(np.array([0., 0., 0.]), np.array([4., 10., 4.]), 4, raw, raw.astype(np.float32))
    a, b, a2 = navdata._key(p, hf(1)), navdata._key(p, hf(2)), navdata._key(p, hf(1))
    c.check(a != b and a == a2 and a != navdata._key(p, None),
            'navigation cache key: different terrain content -> a different key; the same content -> the same key')


def main(argv):
    c = Checks()
    work = tempfile.mkdtemp(prefix='review_fixes_')
    try:
        navkit_apply(c, work)
        stockmp_install(c, work)
        navdata_key(c, work)
    finally:
        shutil.rmtree(work, ignore_errors=True)
    print('all passed' if not c.failed else f'{c.failed} check(s) failed')
    return 1 if c.failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
