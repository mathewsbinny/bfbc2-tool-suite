r"""dbxparitylib.py - the library half of the DBX parity checks: a partition's instances flattened field by field, and
the fair pools the parity runs and the fair-port test port on.

Round PR1b-1d retired the command line dbxparity.py: `bfbc2 dbxparity [names ...] [-o report.txt] [--predict]`
(rust\bc2-cli\src\bf1943\dbxparity.rs) answers it alone, its reports byte for byte the Python's on the user's files
(rust/BENCHMARKS.md "PR1b-1d"). This module keeps what other code imports:

  instances(data)          {GUID upper case: (type, {field path: (kind, value)})}: every instance of a partition with
                           its fields flattened (nested fields and array items included, an array's length as
                           path#len, a reference as its GUID) - Project OPFOR's WW2 QA (qa_checks: the weather ranges)
  FAIR_POOLS, family(k), fair_pool(k), fair_catalogue(cat, exclude)
                           the fair ports' donors and class defaults (tests\test_dbxport_fair.py, rust\bench_bf1943.py)

The Rust tool keeps the same pools and rules (rust\bc2-cli\src\bf1943\dbxparity.rs: fair_pools, family, fair_pool,
fair_defaults); a change here is made there too.
"""
import os
import sys

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('bf1943-import', 'core')
from bfbc2core import dbx  # noqa: E402


def flatten(e, path, out):
    """{field path: (kind, value)} of an instance's fields; arrays / complexes nested (items by index)."""
    kids = e.children or []
    if e.kind == dbx.CONTAINER or e.tag in ('array', 'complex', 'instance') and kids:
        for i, c in enumerate(kids):
            n = c.attr('name')
            sub = f'{path}.{n}' if n is not None else f'{path}[{i}]'
            flatten(c, sub, out)
        if e.tag == 'array':
            out[path + '#len'] = ('len', str(len(kids)))
        return
    ref = e.attr('ref')
    if ref is not None:
        out[path] = ('ref', ref.split('/')[-1].lower() if ref != 'null' else 'null')
        return
    try:
        out[path] = (e.kind, dbx.value(e).strip())
    except Exception:                                               # noqa: BLE001 - an unreadable value
        out[path] = (e.kind, '?')


def instances(data):
    f = dbx.parse(data)
    out = {}
    for inst in f.instances:
        fields: dict[str, tuple[int | str, str]] = {}
        flatten(inst, '', fields)
        out[(inst.attr('guid') or '').upper()] = (inst.attr('type'), fields)
    return out


# the fair pools: BFBC2 donors of the kind the callers use (OPFOR's ww2 builds), per partition family
_V, _VIET = 'objects/vehicles/beach', 'objects/vehicles/vietnam'
_COMMON = [f'{_V}/kurogane/kurogane_wreck', f'{_V}/kurogane/kurogane_mesh_mesh', f'{_V}/type97-chiha/chiha_mb_mesh',
           'systems/mesh/defaultmeshlodsettings', 'objects/weapons/handheld/beach/thompson/thompson_lodsettings',
           'objects/vehicles/sea/lcrs/lcrs', 'objects/vehicles/air/ah60/ah60']
_JEEP = [f'{_VIET}/m151a1/m151a1', f'{_V}/kurogane/kurogane', 'objects/vehicles/land/humvee/humvee']
_PLANE = [f'{_V}/us_corsair/corsair', f'{_V}/jap_zero_fighter/zero_fighter']
_M24 = 'objects/weapons/handheld/us_sni_m24'
FAIR_POOLS = {
    'objects': ['objects/objects/boat_house/boat_house', 'objects/objects/boat_house/boat_house_mesh',
                'objects/objects/boat_house/boat_house_havok',
                'objects/street_objects/rowing_boat_t01v01/rowing_boat_t01v01',
                'objects/street_objects/rowing_boat_t01v01/rowing_boat_t01v01_mesh',
                'objects/street_objects/rowing_boat_t01v01/rowing_boat_t01v01_havok',
                'systems/mesh/defaultmeshlodsettings', 'objects/vehicles/beach/kurogane/kurogane_wreck',
                'objects/buildings/barrack/barrack', 'objects/buildings/barrack/barrack_mesh',
                'objects/buildings/barrack/barrack_havok', 'objects/buildings/house_06_8x8/house_06_8x8'],
    'zero': [f'{_V}/jap_zero_fighter/zero_fighter', f'{_V}/jap_zero_fighter/zero_firing',
             f'{_V}/jap_zero_fighter/zero_bombs_firing'] + _PLANE + _COMMON,
    'corsair': [f'{_V}/us_corsair/corsair', f'{_V}/us_corsair/corsair_firing',
                f'{_V}/us_corsair/corsair_bombs_firing'] + _PLANE + _COMMON,
    'kurogane': _JEEP + _COMMON,
    'chiha': [f'{_V}/type97-chiha/type97-chiha', f'{_VIET}/m48pattontank/m48'] + _JEEP + _COMMON,
    'bofors': [f'{_V}/stationary_aa_bofors/aa_bofors', f'{_V}/stationary_aa_bofors/aa_bofors_firing'] + _COMMON,
    'vehicles': [f'{_V}/stationary_hmg_30cal/30cal_window'] + _COMMON,
    'weapons': [f'{_M24}/us_sni_m24', f'{_M24}/us_sni_m24_firing', f'{_M24}/us_sni_m24_mesh',
                'objects/weapons/handheld/beach/thompson/thompson_lodsettings', 'systems/mesh/defaultmeshlodsettings'],
}


def family(k):
    """A shared partition's family: weapons, vehicles (by vehicle), objects, effects, sounds, other."""
    if k.startswith('objects/weapons/'):
        return 'weapons'
    if k.startswith('objects/vehicles/'):
        return next((v for v in ('zero', 'corsair', 'kurogane', 'chiha', 'bofors') if v in k), 'vehicles')
    if k.startswith('objects/'):
        return 'objects'
    if k.startswith(('effects/', 'fx/', 'emitters/')):
        return 'effects'
    return 'sounds' if k.startswith('sound/') else 'other'


def fair_pool(k):
    """The fair pool of a shared partition (its own name left out): FAIR_POOLS, none for the families without one
    (the catalogue's class defaults)."""
    name = k[:-4] if k.endswith('.dbx') else k
    return [p for p in FAIR_POOLS.get(family(k), []) if p != name]


def fair_catalogue(cat, exclude):
    """The catalogue with class defaults that are not the partition's own (its second instance instead)."""
    import copy
    c = copy.copy(cat)
    c.defaults = {t: (cat.others[t] if v[0] == exclude and t in cat.others else v)
                  for t, v in cat.defaults.items() if v[0] != exclude or t in cat.others}
    return c
