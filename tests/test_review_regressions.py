"""Regression checks for HPD discovery, launcher completion races and mypy failures."""
import contextlib
import io
import queue
import runpy
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

SUITE = Path(__file__).resolve().parents[1]
if str(SUITE) not in sys.path:
    sys.path.insert(0, str(SUITE))
import run_tests

TYPES = runpy.run_path(str(SUITE / 'tests' / 'test_types.py'))
Hub = runpy.run_path(str(SUITE / 'BFBC2 Tool Suite.pyw'))['Hub']


class ReviewRegressions(unittest.TestCase):
    def test_hpd_discovery(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            ai = root / 'game' / 'Output' / 'win32' / 'ai'
            fixtures = {
                'stock': ['PathData.HPD', 'pathdata.hpd.report.txt'],
                'navkit': ['pathdata.hpd', 'pathdata.hpd.opfor-orig', 'pathdata.hpd.opfor-new'],
                'new': ['pathdata.hpd', 'pathdata.hpd.opfor-new'],
                'stockmp': ['pathdata.hpd', 'pathdata.hpd.opfor-stockmp-applied', 'pathdata.hpd.opfor-stockmp-orig'],
                'stockmp_new': ['pathdata.hpd', 'pathdata.hpd.opfor-stockmp-applied'],
                'backup_only': ['pathdata.hpd.opfor-orig'],
            }
            for folder, files in fixtures.items():
                (ai / folder).mkdir(parents=True)
                for name in files:
                    (ai / folder / name).touch()
            work = root / 'work'
            work.mkdir()
            (work / 'pathdata.hpd').touch()
            (work / 'pathdata.hpd.report.txt').touch()
            expected = [ai / 'stock' / 'PathData.HPD', ai / 'navkit' / 'pathdata.hpd.opfor-orig',
                        ai / 'stockmp' / 'pathdata.hpd.opfor-stockmp-orig', ai / 'backup_only' / 'pathdata.hpd.opfor-orig']
            self.assertEqual(run_tests.dice_pathdata_files(tmp), sorted(map(str, expected)))
            all_files = run_tests.pathdata_files(tmp)
            self.assertEqual(len(all_files), 9)
            self.assertIn(str(work / 'pathdata.hpd'), all_files)
            self.assertTrue(all(Path(f).name.lower() in run_tests.HPD_FILES for f in all_files))

    def test_mypy_exit_status(self):
        for code, output, expected in [(2, "mypy: can't read file missing.py", 1),
                                       (1, 'x.py:1: error: bad type', 1), (2, '', 1), (0, '', 0)]:
            with self.subTest(code=code, output=output), contextlib.redirect_stdout(io.StringIO()):
                result = SimpleNamespace(returncode=code, stdout='', stderr=output)
                with patch.dict(TYPES['main'].__globals__, FOLDERS=['missing']), \
                        patch('subprocess.run', return_value=result):
                    self.assertEqual(TYPES['main']([]), expected)

    def test_launcher_completion_identity_and_cancellation(self):
        hub = Hub.__new__(Hub)
        old, active, detached = object(), object(), object()
        hub.proc = active
        hub.stopped_procs = {active}
        hub.ok_codes = {old: (0, 1), active: (0,)}
        hub.lines = queue.Queue()
        hub.status = Mock()
        hub.stop_btn = Mock()
        hub.root = Mock()
        hub.write = Mock()
        hub.fill_levels = Mock()
        hub.failed = Mock()
        hub.lines.put(('done', 'Levels old', 1, True, old))
        hub.lines.put(('done', 'Detached', 3, False, detached))
        hub.pump()
        hub.status.set.assert_not_called()
        hub.stop_btn.configure.assert_not_called()
        hub.fill_levels.assert_not_called()
        hub.failed.assert_called_once_with('Detached', 3, False)
        self.assertIn(active, hub.stopped_procs)
        hub.lines.put(('done', 'Active', 1, True, active))
        hub.pump()
        hub.status.set.assert_called_once_with('Ready.')
        hub.stop_btn.configure.assert_called_once_with(state='disabled')
        self.assertEqual(hub.failed.call_count, 1)
        self.assertFalse(hub.stopped_procs)
        self.assertFalse(hub.ok_codes)
        # Cancellation of the previous task must not suppress a new task's failure.
        hub.lines.put(('done', 'Active again', 2, True, active))
        hub.pump()
        hub.failed.assert_called_with('Active again', 2, True)

    def test_reader_carries_process_identity(self):
        hub = Hub.__new__(Hub)
        hub.lines = queue.Queue()
        hub.tails = {}
        proc = Mock(stdout=['hello\n'])
        proc.wait.return_value = 7
        hub.reader(proc, 'tool', True)
        self.assertEqual(hub.lines.get_nowait(), 'hello\n')
        self.assertEqual(hub.lines.get_nowait(), ('done', 'tool', 7, True, proc))


if __name__ == '__main__':
    unittest.main()
