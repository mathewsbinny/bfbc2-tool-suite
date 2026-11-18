r"""shaderview -serve (round E3l): many jobs on one Direct3D device, against one shaderview process per job.

Quick (no game data; WARP, so the bytes compare across processes): built jobs - a textured quad (2D and cube
textures, a sampler state, a $Globals buffer, quoted paths with spaces), depth and stencil frames with the game's
render states and the coverage classes, a counting draw, several frames in one job - each run through
renderqa.run_job served and with BFBC2_SHADERVIEW_SERVE=0 (one process per job, the reference): every output file
byte for byte, the output lines and the exit codes alike;
- a job's state does not reach the next: a job with a vertex shader constant buffer and 6 textures, then one with
  none, on the same server, gives what the second gives alone;
- failures (a missing input, an unknown command, bad render-state numbers, a malformed sampler state) give the
  exit code and message the job alone gives (RenderQaError's text), and the server keeps going: the next job on it
  is still the reference's;
- jobs from 8 threads at once (each its own server) all the reference's; the servers left idle end with
  close_servers();
- a long run on one server (400 jobs, each loading its textures again) does not grow the process (every texture,
  sampler and state a job makes released).

  python test_shaderview_serve.py
"""
import concurrent.futures
import ctypes
import hashlib
import os
import shutil
import struct
import sys
import tempfile
from ctypes import wintypes

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(HERE))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('shader-tools', 'console-convert', 'core')
import pcbuild as P  # noqa: E402
import renderqa as RQ  # noqa: E402

F = np.float32
fails: list[str] = []


def check(cond, what):
    print(('ok   ' if cond else 'FAIL ') + what, flush=True)
    if not cond:
        fails.append(what)


VS = """
float4 tint;
struct VI { float4 p : TEXCOORD0; float4 c : TEXCOORD1; };
struct VO { float4 pos : SV_Position; float4 c : TEXCOORD0; };
VO main(VI i) { VO o; o.pos = float4(i.p.xy, i.p.z, 1); o.c = i.c + tint * 0.0; return o; }
"""
PS = """
float4 scale;
Texture2D t2 : register(t2); TextureCube t3 : register(t3); SamplerState s1 : register(s1);
SamplerState s4 : register(s4);
float4 main(float4 pos : SV_Position, float4 c : TEXCOORD0) : SV_Target0 {
    float4 a = t2.Sample(s1, c.xy);
    float4 b = t3.Sample(s4, normalize(float3(c.xy * 2 - 1, 0.7)));
    if (c.w < 0.05) discard;
    return a * scale + b * 0.25 + c * 0.1;
}
"""


class Built:
    """Inputs written once per test folder; each job names them (some jobs leave some out)."""

    def __init__(self, work: str, seed: int = 1) -> None:
        self.work = work
        rng = np.random.default_rng(seed)
        self.vs = self.put('the vs.dxbc', P.compile_hlsl(VS, 'main', 'vs_5_0'))
        self.ps = self.put('the ps.dxbc', P.compile_hlsl(PS, 'main', 'ps_5_0'))
        quads = []
        for q in range(3):                                  # three overlapping quads at different depths
            x0, y0, z = -0.9 + 0.3 * q, -0.8 + 0.25 * q, 0.2 + 0.25 * q
            for (x, y) in ((x0, y0), (x0 + 1.1, y0), (x0, y0 + 1.1), (x0 + 1.1, y0 + 1.1)):
                quads.append([x, y, z, 1] + list(rng.uniform(0, 1, 4)))
        self.vb = self.put('vb.bin', np.array(quads, F).tobytes())
        ib = []
        for q in range(3):
            b = 4 * q
            ib += [b, b + 1, b + 2, b + 2, b + 1, b + 3]
        self.ib = self.put('ib.bin', np.array(ib, np.uint16).tobytes())
        self.cbv = self.put('cbv.bin', np.array([0.5, 0.25, 0.125, 1], F).tobytes())
        self.cbp = self.put('cbp.bin', np.array([1.5, 0.75, 0.5, 1], F).tobytes())
        self.t2 = self.texture('t2.bin', rng, 1)
        self.t3 = self.texture('t3.bin', rng, 6)
        self.samp = struct.pack('<IIIIfII4fff', 0x15, 1, 1, 1, 0.0, 1, 1, 0, 0, 0, 0, 0.0, 3.4e38).hex()

    def put(self, name: str, data: bytes) -> str:
        p = os.path.join(self.work, name)
        with open(p, 'wb') as f:
            f.write(data)
        return RQ.job_path(p)

    def texture(self, name: str, rng: np.random.Generator, faces: int) -> str:
        tx = rng.uniform(0, 1, (faces, 8, 8, 4)).astype(F)
        return self.put(name, struct.pack('<III', 8, 8, faces) + tx.tobytes())

    def out(self, name: str) -> str:
        return RQ.job_path(os.path.join(self.work, name))

    def draw(self, cbv: bool = True, textures: bool = True, count: int = 18) -> list[str]:
        lines = [f'vs {self.vs}', f'ps {self.ps}', 'el 0 2 0 0 0', 'el 1 2 0 16 0', f'vb {self.vb} 0 32',
                 f'ib {self.ib} 0 16 {count} 0', f'cb ps {self.cbp}']
        if cbv:
            lines.append(f'cb vs {self.cbv}')
        if textures:
            lines += [f'tex 2 2d {self.t2}', f'tex 3 cube {self.t3}', f'samp 4 {self.samp}']
        return lines + ['draw']

    def jobs(self) -> dict[str, list[str]]:
        """name -> job lines; outputs named after the job."""
        j = {}
        j['plain'] = [f'frame {self.out("plain.raw")} 48 32 0.1 0.2 0.3'] + self.draw() + ['end']
        j['depth'] = ([f'frame {self.out("depth.raw")} 48 32 0 0 0 0', 'depth 1'] + self.draw() +
                      ['blend 1'] + self.draw(count=6) + ['end'])
        j['stencil'] = ([f'frame {self.out("stencil.raw")} 40 40 0 0 0 0', 'stencil 1', 'class 2',
                         'blendstate 0 1 5 6 1 2 6 1 15', 'depthstate 1 0 4', 'raster 3 1'] + self.draw() +
                        ['class 3', 'blendstate 1 0 2 1 1 2 1 1 15', 'depthstate 1 1 2', 'raster 1 0'] +
                        self.draw(count=12) + ['end'])
        j['count'] = ([f'frame {self.out("count.raw")} 32 32 0 0 0', 'count 1'] + self.draw() + ['count 0'] +
                      self.draw(count=6) + ['end'])
        j['frames'] = ([f'frame {self.out("frames_a.raw")} 16 16 1 0 0'] + self.draw() + ['end',
                       f'frame {self.out("frames_b.raw")} 24 8 0 1 0 0.5'] + self.draw(textures=False) + ['end'])
        j['bare'] = [f'frame {self.out("bare.raw")} 48 32 0 0 0'] + self.draw(cbv=False, textures=False) + ['end']
        return j


def outputs(work: str) -> dict[str, str]:
    return {n: hashlib.sha256(open(os.path.join(work, n), 'rb').read()).hexdigest()
            for n in sorted(os.listdir(work)) if n.endswith(('.raw', '.depth', '.stencil'))}


def clear(work: str) -> None:
    for n in os.listdir(work):
        if n.endswith(('.raw', '.depth', '.stencil')):
            os.remove(os.path.join(work, n))


def run(job: str) -> tuple[str, list[str] | str]:
    try:
        lines, warp = RQ.run_job(job, warp=True)
        return 'ok', lines
    except RQ.RenderQaError as e:
        return 'error', str(e)


def write_job(work: str, name: str, lines: list[str]) -> str:
    p = os.path.join(work, f'{name} job.txt')
    with open(p, 'w') as f:
        f.write('\n'.join(lines) + '\n')
    return p


def served(on: bool) -> None:
    os.environ['BFBC2_SHADERVIEW_SERVE'] = '1' if on else '0'


def both(work: str, jobs: list[str]) -> tuple[list, dict, list, dict]:
    """Every job one process each (the reference), then served in order: (results, files) of each."""
    served(False)
    clear(work)
    ref = [run(j) for j in jobs]
    ref_files = outputs(work)
    served(True)
    clear(work)
    got = [run(j) for j in jobs]
    return ref, ref_files, got, outputs(work)


def same_jobs(work: str, b: Built) -> None:
    jobs = {k: write_job(work, k, v) for k, v in b.jobs().items()}
    ref, rf, got, gf = both(work, list(jobs.values()))
    check(all(r[0] == 'ok' for r in ref) and len(rf) >= 9, f'reference: {len(jobs)} built jobs drawn, '
          f'{len(rf)} output files (colour, depth, stencil)')
    check(rf == gf, f'served: every output file byte for byte the reference\'s ({sum(rf[k] == gf.get(k) for k in rf)} '
          f'of {len(rf)})')
    check(ref == got, 'served: the output lines alike (frames, draw counts, sample counts)')
    k = list(jobs).index('count')
    check(any(ln.startswith('count 0 ') for ln in got[k][1]), 'served: the counting draw\'s line is there')
    # state between jobs: everything bound by one job gone for the next (run each after another on one server)
    order = ['stencil', 'bare', 'count', 'bare', 'depth', 'plain', 'frames', 'bare']
    served(True)
    RQ.close_servers()
    clear(work)
    seq = [run(jobs[n]) for n in order]
    check(outputs(work) == {n: h for n, h in rf.items() if any(n.startswith(o) for o in order)} and
          seq == [ref[list(jobs).index(n)] for n in order],
          'served: one server, jobs after jobs with more state (a vs constant buffer, textures, render states, a '
          'stencil frame): each the reference\'s alone')


def failures(work: str, b: Built) -> None:
    bad = {
        'missing': [f'frame {b.out("m.raw")} 8 8 0 0 0', f'vs {b.out("no such file.dxbc")}'],
        'unknown': [f'frame {b.out("u.raw")} 8 8 0 0 0', 'wobble 1'],
        'blendstate': [f'frame {b.out("b.raw")} 8 8 0 0 0', 'blendstate 1 2 3'],
        'sampler': [f'frame {b.out("s.raw")} 8 8 0 0 0', 'samp 1 abcd'],
        'texture': [f'frame {b.out("t.raw")} 8 8 0 0 0', f'texres 0 {b.out("cbp.bin")}'],
    }
    good = write_job(work, 'good', b.jobs()['plain'])
    jobs = []
    for k, v in bad.items():
        jobs += [write_job(work, k, v), good]
    jobs.append(os.path.join(work, 'no such job.txt'))
    jobs.append(good)
    ref, rf, got, gf = both(work, jobs)
    errs = [r for r in ref if r[0] == 'error']
    check(len(errs) == len(bad) + 1 and all('shaderview failed (' in r[1] for r in errs),
          f'reference: {len(errs)} failing jobs fail ({", ".join(sorted({r[1][:22] for r in errs}))})')
    check(got == ref, 'served: each failure\'s exit code and message the reference\'s, and the job after each '
          'failure on the same server drawn alike')
    check(rf == gf, 'served: the files written alike')
    print('     ' + ' | '.join(r[1].split(': ', 1)[-1][:60] for r in errs[:3]))


def threads(work: str, b: Built) -> None:
    jobs = {k: write_job(work, k, v) for k, v in b.jobs().items()}
    served(False)
    clear(work)
    ref = {k: run(j) for k, j in jobs.items()}
    rf = outputs(work)
    served(True)
    RQ.close_servers()
    clear(work)
    names = list(jobs) * 6
    with concurrent.futures.ThreadPoolExecutor(8) as ex:
        got = list(ex.map(lambda k: (k, run(jobs[k])), names))
    check(all(r == ref[k] for k, r in got) and outputs(work) == rf,
          f'8 threads, {len(names)} jobs (each output written 6 times): every result and file the reference\'s')
    idle = sum(len(v) for v in RQ._IDLE.values())
    check(1 <= idle <= 8, f'a server per thread at most ({idle} idle after)')
    procs = [s.p for v in RQ._IDLE.values() for s in v]
    RQ.close_servers()
    check(not RQ._IDLE and all(p.poll() is not None for p in procs), 'close_servers ends them all')


class PMC(ctypes.Structure):
    _fields_ = [('cb', wintypes.DWORD), ('PageFaultCount', wintypes.DWORD)] + \
               [(n, ctypes.c_size_t) for n in ('PeakWorkingSetSize', 'WorkingSetSize', 'QuotaPeakPagedPoolUsage',
                                               'QuotaPagedPoolUsage', 'QuotaPeakNonPagedPoolUsage',
                                               'QuotaNonPagedPoolUsage', 'PagefileUsage', 'PeakPagefileUsage')]


def private_bytes(pid: int) -> int:
    k32 = ctypes.WinDLL('kernel32', use_last_error=True)
    psapi = ctypes.WinDLL('psapi', use_last_error=True)
    k32.OpenProcess.restype = wintypes.HANDLE
    h = k32.OpenProcess(0x1000 | 0x0010, False, pid)           # query limited information, vm read
    try:
        c = PMC()
        c.cb = ctypes.sizeof(c)
        psapi.GetProcessMemoryInfo(h, ctypes.byref(c), c.cb)
        return c.PagefileUsage
    finally:
        k32.CloseHandle(h)


def no_growth(work: str) -> None:
    served(True)
    RQ.close_servers()
    builts = []
    for k in range(8):
        os.makedirs(os.path.join(work, f'b{k}'))
        builts.append(Built(os.path.join(work, f'b{k}'), seed=k))
    jobs = [write_job(bt.work, 'j', bt.jobs()['stencil'] + bt.jobs()['depth']) for bt in builts]
    for j in jobs[:4]:
        run(j)
    srv = RQ._IDLE[next(iter(RQ._IDLE))][0]
    before = private_bytes(srv.p.pid)
    for k in range(400):
        run(jobs[k % len(jobs)])
    after = private_bytes(srv.p.pid)
    check(len(RQ._IDLE[next(iter(RQ._IDLE))]) == 1 and after - before < 8 << 20,
          f'400 jobs on one server: its memory {before >> 20} -> {after >> 20} MB (under 8 MB growth)')
    RQ.close_servers()


def main() -> int:
    work = tempfile.mkdtemp(prefix='shaderview serve ')
    saved = os.environ.get('BFBC2_SHADERVIEW_SERVE')
    try:
        b = Built(work)
        same_jobs(work, b)
        failures(work, b)
        threads(work, b)
        no_growth(work)
    finally:
        RQ.close_servers()
        if saved is None:
            os.environ.pop('BFBC2_SHADERVIEW_SERVE', None)
        else:
            os.environ['BFBC2_SHADERVIEW_SERVE'] = saved
        shutil.rmtree(work, ignore_errors=True)
    print('all passed' if not fails else f'{len(fails)} failed')
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main())
