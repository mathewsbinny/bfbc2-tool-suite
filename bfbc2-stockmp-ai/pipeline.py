"""Sources, build and packaging of a stock level's AI layer.

Inputs, all read from the user's own installs (nothing is shipped):
- the level's level-00 bundles from Project OPFOR BFBC2's game copy (Dist and Package), unpacked into the work folder;
- the donor level (DICE's SP_004_B: AreaOrder / AreaTrigger layouts, the SectorOffsets asset layout);
- sp_common (the AI template chain to pack) and mp_common (kits, PathfindingSettings - only read for checks);
- the path data named in the configuration (DICE's file where one exists for the world, else a NavKit build).

Output, only under <OPFOR>\\work\\stockmp\\<lv>\\:
    src\\dist, src\\pkg                 the unpacked stock bundles (cache)
    build\\dist, build\\pkg             the bundles' folders with the authored level and the added entries
    out\\game\\...                      what an install copies into the game, at the game's own paths:
        Dist\\win32\\levels\\<lv>\\level-00.fbrb, Package\\levels\\<lv>\\level-00.fbrb,
        Output\\win32\\ai\\levels\\<lv>\\infantry|vehicle\\pathdata.hpd
    build_report.json                   hashes of every input and output, counts, the SectorOffsets values
"""
import hashlib
import json
import math
import os
import shutil
import subprocess
import sys
from collections import Counter
from collections.abc import Callable, Iterable
from typing import Any

import author
import config as cfgmod
import dbxbin
import levelmodel

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(HERE)
_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('navkit', 'navkit-tools', 'core')
from bfbc2core import paths as corepaths  # noqa: E402  (not `paths`: the functions below name their Paths that)

FBRB = corepaths.suite_tool('bfbc2-fbrb-tool', 'FbrbTool.exe', SUITE)
DBX = corepaths.suite_tool('bfbc2-dbx-tools', 'DbxEdit.exe', SUITE)
NAVKIT = os.path.join(SUITE, 'bfbc2-auto-navkit')

DONOR_LEVEL = 'SP_004_B'
MIN_AGREEMENT = 0.75          # as apply_sector_offsets.py
SECTOR_NS = 'opfor:sectoroffsets'
KINDS = (('Infantry', 'infantry'), ('Vehicle', 'vehicle'))


class BuildError(Exception):
    pass


def run(*args: object, ok: tuple[int, ...] = (0,)) -> str:
    r = subprocess.run([str(a) for a in args], capture_output=True, text=True, errors='replace')
    if r.returncode not in ok:
        raise BuildError(f'failed: {" ".join(map(str, args))}\n{r.stdout[-2000:]}\n{r.stderr[-2000:]}')
    return r.stdout


def sha(path: str) -> str:
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for b in iter(lambda: f.read(1 << 20), b''):
            h.update(b)
    return h.hexdigest()


class Paths:
    """variant: a second build of the same level next to the first (e.g. 'exp0', the minimal layer of experiment 0):
    <work>\\<lv>-<variant>\\, sharing <work>\\<lv>\\src\\ (the unpacked stock bundles)."""

    def __init__(self, lv: str, opfor: str | None = None, work: str | None = None, variant: str | None = None,
                 game: str | None = None) -> None:
        if opfor is None:      # NavKit's search (walking up from the suite) and message; GameCopyNotFound
            opfor = corepaths.find_opfor(None, start=SUITE)
        self.opfor: str = opfor
        self.lv = lv.lower()
        self.game = game or os.path.join(opfor, 'game')
        self.work = work or os.path.join(opfor, 'work', 'stockmp')
        self.variant = variant
        self.root = os.path.join(self.work, self.lv + (f'-{variant}' if variant else ''))
        self.cache = os.path.join(self.work, '_cache')
        self.common = os.path.join(opfor, 'work', 'navkit', 'common')

    def src(self, variant: str) -> str:
        return os.path.join(self.work, self.lv, 'src', variant)

    def bundles(self) -> list[tuple[str, str]]:
        """(variant, game bundle path) of the level's level-00 bundles that exist."""
        out: list[tuple[str, str]] = []
        for v, p in (('dist', os.path.join(self.game, 'Dist', 'win32', 'levels', self.lv, 'level-00.fbrb')),
                     ('pkg', os.path.join(self.game, 'Package', 'levels', self.lv, 'level-00.fbrb'))):
            if os.path.isfile(p):
                out.append((v, p))
        return out

    def out_bundle(self, variant: str) -> str:
        rel = (['Dist', 'win32', 'levels', self.lv] if variant == 'dist' else ['Package', 'levels', self.lv])
        return os.path.join(self.root, 'out', 'game', *rel, 'level-00.fbrb')

    def out_pathdata(self, kind: str) -> str:
        return os.path.join(self.root, 'out', 'game', 'Output', 'win32', 'ai', 'levels', self.lv, kind, 'pathdata.hpd')

    def config(self) -> str:
        return os.path.join(self.root, 'config.json')


def unpacked(bundle: str, d: str) -> str:
    """A cached unpack of a bundle (re-unpacked when the bundle's hash changed)."""
    stamp = os.path.join(d, '.source.sha256')
    h = sha(bundle)
    if os.path.isfile(os.path.join(d, 'fbrb.manifest')) and os.path.isfile(stamp) and open(stamp).read().strip() == h:
        return d
    shutil.rmtree(d, ignore_errors=True)
    run(FBRB, 'unpack', bundle, d)
    open(stamp, 'w').write(h)
    return d


def level_entry(d: str, lv_upper: str | None = None) -> str | None:
    """The level partition's entry path in an unpacked bundle (Levels/<LV>.dbx), or None."""
    for line in open(os.path.join(d, 'fbrb.manifest'), encoding='utf-8').read().splitlines():
        name = line.split('|', 1)[0]
        parts = name.replace('\\', '/').split('/')
        if len(parts) == 2 and parts[0].lower() == 'levels' and parts[1].lower() == (lv_upper or '').lower() + '.dbx' \
                and '|*deleted*|' not in line:
            return name
    return None


def find_entry(folder: str, name: str) -> str | None:
    p = os.path.join(folder, *name.split('/'))
    return p if os.path.isfile(p) else None


def donor(paths: Paths) -> tuple[str, str]:
    """DICE's SP_004_B level partition and its SectorOffsets assets (from the prepared copy, else the game)."""
    for base in (os.path.join(paths.opfor, 'work', 'levels', 'sp_004_b', v) for v in ('pkg', 'dist')):
        lvl = os.path.join(base, 'levels', 'SP_004_B.dbx')
        inf = os.path.join(base, 'levels', 'sp_004_b', 'Precomputed', 'Infantry', 'SectorOffsets.dbx')
        if os.path.isfile(lvl) and os.path.isfile(inf):
            return lvl, inf
    d = unpacked(os.path.join(paths.game, 'Dist', 'win32', 'levels', 'sp_004_B', 'level-00.fbrb'),
                 os.path.join(paths.cache, 'sp_004_b'))
    lvl_e = find_entry(d, 'Levels/SP_004_B.dbx')
    inf_e = find_entry(d, 'Levels/SP_004_B/Precomputed/Infantry/SectorOffsets.dbx')
    if not (lvl_e and inf_e):
        raise BuildError('the donor level SP_004_B or its SectorOffsets asset was not found')
    return lvl_e, inf_e


def common(paths: Paths, name: str) -> str:
    """A common bundle (mp_common, sp_common) unpacked: NavKit's shared unpack (work\\navkit\\common\\<name>) while it
    is at least as new as the game's bundle, else this tool's own cached unpack (stamped with the bundle's hash) - a
    game bundle changed since NavKit unpacked it (a patch, a mod installed) is not read from the stale copy."""
    bundle = os.path.join(paths.game, 'Dist', 'win32', 'levels', name, 'level-00.fbrb')
    d = os.path.join(paths.common, name)
    manifest = os.path.join(d, 'fbrb.manifest')
    if os.path.isfile(manifest) and (not os.path.isfile(bundle) or os.path.getmtime(manifest) >= os.path.getmtime(bundle)):
        return d
    return unpacked(bundle, os.path.join(paths.cache, name))


def kit_refs(paths: Paths) -> dict[str, str]:
    """'us/assault' -> 'objects/soldiers/us/assault/<SoldierAsset guid>' from mp_common."""
    mp = common(paths, 'mp_common')
    out: dict[str, str] = {}
    for fac in ('US', 'RU'):
        for kit in cfgmod.KIT_NAMES:
            p = os.path.join(mp, 'Objects', 'Soldiers', fac, kit.capitalize() + '.dbx')
            if not os.path.isfile(p):
                raise BuildError(f'mp_common has no {fac}/{kit} kit ({p})')
            d = dbxbin.read(p)
            asset = [e for e in d.instances() if (e.attr('type') or '').endswith('.SoldierAsset')]
            if len(asset) != 1:
                raise BuildError(f'{p}: expected one SoldierAsset')
            guid = asset[0].attr('guid')
            assert guid is not None
            out[f'{fac.lower()}/{kit}'] = f'objects/soldiers/{fac.lower()}/{kit}/{guid.lower()}'
    return out


def sector_offsets(hpd: str) -> tuple[tuple[int, int], tuple[float, float]]:
    """(x, z) and agreement from a path-data file's own sector ids (apply_sector_offsets.py's rule)."""
    import hpdtool
    cx: Counter[int] = Counter()
    cz: Counter[int] = Counter()
    n = 0
    for s in hpdtool.parse(open(hpd, 'rb').read()).sectors:
        b = s.bounds
        cx[math.floor((b[0] + b[1]) / 200) - (s.id >> 8)] += 1
        cz[math.floor((b[4] + b[5]) / 200) - (s.id & 0xff)] += 1
        n += 1
    if not n:                                   # an empty build: nothing to take the offsets from
        raise BuildError(f'{hpd}: no sectors in the path data (an empty or failed build?)')
    (x, nx), (z, nz) = cx.most_common(1)[0], cz.most_common(1)[0]
    if min(nx, nz) / n < MIN_AGREEMENT:
        raise BuildError(f'{hpd}: sector ids disagree ({dict(cx)} / {dict(cz)})')
    return (x, z), (nx / n, nz / n)


def sector_asset(ref_path: str, level_upper: str, kind: str, lv: str, xz: tuple[int, int]) -> tuple[bytes, str, str]:
    """A PathdataSectorOffsets partition (bytes) for the level, from the donor asset's layout (dbxbin keeps its integer
    type); guids as apply_sector_offsets.py makes them."""
    d = dbxbin.read(ref_path)
    inst = d.instances()
    if len(inst) != 1 or not (inst[0].attr('type') or '').endswith('.PathdataSectorOffsets'):
        raise BuildError(f'{ref_path} is not a PathdataSectorOffsets asset')
    e = inst[0]
    part_g = dbxbin.new_guid(SECTOR_NS, lv, kind.lower(), 'partition')
    inst_g = dbxbin.new_guid(SECTOR_NS, lv, kind.lower(), 'instance')
    d.set_attr(d.partition, 'guid', part_g)
    d.set_attr(d.partition, 'primaryInstance', inst_g)
    d.set_attr(e, 'guid', inst_g)
    name = e.child('Name')
    assert name is not None
    d.set_text(name, f'Levels/{level_upper}/Precomputed/{kind}/SectorOffsets')
    for f, v in (('XOffset', xz[0]), ('ZOffset', xz[1])):
        c = e.child(f)
        assert c is not None
        size, old = c.numbers()
        if size != 4 or len(old) != 1:
            raise BuildError(f'{ref_path}: {f} is not one 4-byte number')
        d.set_numbers(c, [int(v)], 'i')
    return dbxbin.write(d), part_g, inst_g


def template_parts(paths: Paths, parts: Iterable[str]) -> list[tuple[str, str, str]]:
    """[(entry name, file, manifest line)] of the AI template partitions, from sp_common."""
    sp = common(paths, 'sp_common')
    lines = open(os.path.join(sp, 'fbrb.manifest'), encoding='utf-8').read().splitlines()
    out: list[tuple[str, str, str]] = []
    for p in parts:
        hit = [l for l in lines if l.split('|', 1)[0].lower() == p.lower() and '|*deleted*|' not in l]
        f = find_entry(sp, hit[0].split('|', 1)[0]) if hit else None
        if not f:
            raise BuildError(f'{p} is not in sp_common')
        out.append((hit[0].split('|', 1)[0], f, hit[0]))
    return out


def loads_sp_common(paths: Paths) -> bool:
    """Whether the level's level-00 bundle loads sp_common (package.mft), i.e. the template resolves without packing."""
    mft = os.path.join(paths.game, 'Dist', 'win32', 'package.mft')
    if not os.path.isfile(mft):
        return False
    block: list[str] = []
    cur = False
    for line in open(mft, encoding='utf-8', errors='replace').read().splitlines():
        if line.startswith('BundleSource '):
            cur = line.split(' ', 1)[1].strip().lower() == f'game:levels/{paths.lv}/level-00.fbrb'
        elif cur and line.startswith('BundleTarget '):
            block.append(line.split(' ', 1)[1].strip().lower())
    return 'game:levels/sp_common/level-00.fbrb' in block


def generate(paths: Paths, **kw: Any) -> dict[str, Any]:
    """Write a first configuration for the level (config.json) and return it."""
    variants = paths.bundles()
    if not variants:
        raise BuildError(f'{paths.lv}: no level-00 bundle in {paths.game}')
    v, b = variants[-1]                            # the Package copy when there is one (the game loads it first)
    d = unpacked(b, paths.src(v))
    entry = level_entry(d, _level_name(d, paths.lv))
    assert entry is not None
    lvl = find_entry(d, entry)
    assert lvl is not None
    m = levelmodel.Model(dbxbin.read(lvl))
    pd = kw.pop('pathdata')
    c = cfgmod.generate(m, paths.lv, kit_refs(paths), pd, **kw)
    cfgmod.save(c, paths.config())
    return c


def _level_name(d: str, lv: str) -> str:
    for line in open(os.path.join(d, 'fbrb.manifest'), encoding='utf-8').read().splitlines():
        name = line.split('|', 1)[0].replace('\\', '/')
        parts = name.split('/')
        if len(parts) == 2 and parts[0].lower() == 'levels' and parts[1].lower() == lv + '.dbx' and '|*deleted*|' not in line:
            return parts[1][:-4]
    raise BuildError(f'no Levels/{lv}.dbx in {d}')


def build(paths: Paths, c: dict[str, Any], log: Callable[[str], object] = print) -> dict[str, Any]:
    """Author the level in every level-00 bundle of the level, pack, stage the path data. Returns the report."""
    if cfgmod.check(c):
        raise BuildError('configuration: ' + '; '.join(cfgmod.check(c)))
    rep: dict[str, Any] = {'lv': paths.lv, 'level': c['level'], 'layer': c.get('layer', 'full'), 'inputs': {},
                           'outputs': {}, 'variants': {}, 'sector_offsets': {}}
    donor_lvl, donor_so = donor(paths)
    rep['inputs']['donor'] = {'path': donor_lvl, 'sha256': sha(donor_lvl)}
    pd = c.get('pathdata') or {}
    so: dict[str, tuple[int, int]] = {}
    for kind, folder in KINDS:
        src = pd.get(folder)
        if not src or not os.path.isfile(src):
            raise BuildError(f'path data for {folder} not found: {src}')
        xz, agree = sector_offsets(src)
        so[kind] = xz
        rep['sector_offsets'][kind] = {'x': xz[0], 'z': xz[1], 'agree': [round(a, 3) for a in agree]}
        rep['inputs'][f'pathdata_{folder}'] = {'path': src, 'sha256': sha(src)}
        dst = paths.out_pathdata(folder)
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        shutil.copyfile(src, dst)
        rep['outputs'][os.path.relpath(dst, paths.root)] = sha(dst)
    pack = not loads_sp_common(paths)
    parts = template_parts(paths, c.get('template_parts') or cfgmod.TEMPLATE_PARTS) if pack and c.get('layer', 'full') != 'minimal' else []
    rep['template_pack'] = [p for p, _f, _l in parts]
    for v, bundle in paths.bundles():
        src = unpacked(bundle, paths.src(v))
        rep['inputs'][f'bundle_{v}'] = {'path': bundle, 'sha256': sha(bundle)}
        lv_upper = _level_name(src, paths.lv)
        entry = level_entry(src, lv_upper)
        dst = os.path.join(paths.root, 'build', v)
        log(f'{v}: copying the unpacked bundle to {os.path.relpath(dst, paths.opfor)}')
        shutil.rmtree(dst, ignore_errors=True)
        shutil.copytree(src, dst, ignore=shutil.ignore_patterns('.source.sha256'))
        assert entry is not None
        lvl_file = find_entry(dst, entry)
        assert lvl_file is not None
        original = open(lvl_file, 'rb').read()
        d = dbxbin.parse(original)
        if dbxbin.write(d) != original:
            raise BuildError(f'{entry}: the DBX writer does not reproduce the original - stopping')
        a = author.Author(d, dbxbin.read(donor_lvl), c, levelmodel.Model(d))
        counts = a.build()
        data = dbxbin.write(d)
        shutil.copyfile(lvl_file, os.path.join(paths.root, 'build', f'{lv_upper}_{v}.original.dbx'))
        open(lvl_file, 'wb').write(data)
        shutil.copyfile(lvl_file, os.path.join(paths.root, 'build', f'{lv_upper}_{v}.authored.dbx'))
        added: list[tuple[str, str, str]] = []
        for kind, _folder in KINDS:
            ref = os.path.join(os.path.dirname(os.path.dirname(donor_so)), kind, 'SectorOffsets.dbx')
            raw, pg, ig = sector_asset(ref, lv_upper, kind, paths.lv, so[kind])
            name = f'Levels/{lv_upper}/Precomputed/{kind}/SectorOffsets.dbx'
            f = os.path.join(paths.root, 'build', f'SectorOffsets_{kind}.dbx')
            open(f, 'wb').write(raw)
            added.append((name, f, f'{name}|<non-resource>|00010000|{len(raw)}'))
        for p, f, line in parts:
            added.append((p, f, line))
        _add_entries(dst, added)
        out = paths.out_bundle(v)
        os.makedirs(os.path.dirname(out), exist_ok=True)
        run(FBRB, 'pack', dst, out)
        rep['variants'][v] = {'entry': entry, 'authored': counts, 'added_entries': [n for n, _f, _l in added],
                              'level_sha256_before': hashlib.sha256(original).hexdigest(),
                              'level_sha256_after': hashlib.sha256(data).hexdigest()}
        rep['outputs'][os.path.relpath(out, paths.root)] = sha(out)
        log(f'{v}: {counts}; packed {os.path.relpath(out, paths.opfor)}')
    rep['guids'] = {'note': 'deterministic: UUIDv5(NAMESPACE_URL, "opfor:stockmp:<lv>:<role>:<id>")'}
    with open(os.path.join(paths.root, 'build_report.json'), 'w', encoding='utf-8') as fh:
        json.dump(rep, fh, indent=1)
    return rep


def _add_entries(d: str, added: list[tuple[str, str, str]]) -> None:
    m = os.path.join(d, 'fbrb.manifest')
    text = open(m, encoding='utf-8').read()
    names = {x.split('|', 1)[0].lower() for x in text.splitlines() if '|*deleted*|' not in x}
    for name, f, line in added:
        if name.lower() in names:
            raise BuildError(f'{name} is already in the bundle')
        dst = os.path.join(d, *name.split('/'))
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        shutil.copyfile(f, dst)
        text = text.rstrip('\r\n') + '\n' + line + '\n'
    open(m, 'w', encoding='utf-8', newline='').write(text)
