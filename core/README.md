# core

The suite's shared Python package, `bfbc2core`: the game's data formats, the level loader and the 3D view
that the 3D tools are built on. Python 3.10+ with `numpy`; the view also needs `moderngl`, `glfw` and
`imgui-bundle`.

| Module | What it does | Checked by |
|---|---|---|
| `dbx.py` | reads binary `.dbx` files into an element tree and writes DbxEdit's XML, from `bfbc2-dbx-tools/FORMAT.md` | `tests/test_dbx.py`: XML identical to DbxEdit's on all 31,404 fingerprinted files |
| `terrain.py` | terrain height fields (`.Heightfield-000.res`): heights, exact height at any point, a mesh | `tests/test_terrain.py`: within 4.5 cm of the in-game terrain for 99.9 % of 40 million samples (13 levels) |
| `water.py` | water meshes (`.Water.Mesh.res`) and each triangle's shader (from `.Water.res`) | `tests/test_water.py`: all 36 levels |
| `level.py` | a level: entities (types, transforms, blueprints, teams, player types), static-group members, world-space collision, areas and routes, terrain and water; prepares a level on first use | `tests/test_level.py`: counts against `tests/level_snapshot.json` |
| `viewer.py` | the 3D view: triangles and lines tagged with object ids and layers, orbit and fly camera, GPU picking with the world point, an overlay for links and measurements, ImGui panels | the Map Inspector's screenshot runs |

```python
import sys; sys.path.insert(0, r'<suite>\core')
from bfbc2core import level
lvl = level.load('mp_005')                    # from Project OPFOR's game copy (found next to the suite)
flags = [e for e in lvl.entities if e.category == 'capture point']
lvl.heightfield.height(-111.0, 163.3)         # terrain height there
```

The formats are described in the module docstrings and in `../DESIGN.md` section 7. All tests:
`python ..\run_tests.py game`.
