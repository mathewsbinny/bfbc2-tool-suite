"""L1 exploration: terrain names and height fields found for some levels (output not kept)."""
import sys

from l1_geom import heightfield
from l1_level import load

for lv in sys.argv[1:]:
    L = load(lv)
    t = L.of('TerrainEntityData')
    ref = t[0].f('TerrainAsset').attr('ref') if t else None
    name = ref.split('/')[1] if ref else None
    common = 'nam_common' if lv.startswith('nam_') else ('ww2_common' if lv in ('guadal_canal', 'wake_island_s', 'iwo_jima_s', 'coral_sea') else 'mp2_common')
    r = heightfield(name, lv, common) if name else None
    if r:
        hf, b = r
        print(lv, ref, b, hf.size, hf.min, hf.max, float(hf.height(0.0, 0.0)))
    else:
        print(lv, ref, 'NOT FOUND')
