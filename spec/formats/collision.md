# Collision as the level uses it

How a level's placed objects find their collision: which `HavokPhysicsData` resource a blueprint uses, how a
blueprint borrows another's through its `Mesh` reference, how static model groups hold many objects' collision in
one resource, and where the terrain's and water's collision comes from. The resources themselves are in
[havok.md](havok.md), [terrain.md](terrain.md) and [water.md](water.md); the level's structure (LevelData, the
`Objects` list) is in the data model files.

The suite's index is `rust/bc2-scene/src/collision.rs` (NavKit's `levelobjects.havok_index` / `mesh_aliases`,
ported with their order and first-wins rules); the level loader that places it is `rust/bc2-scene/src/scene.rs`.

## 1. Blueprint to packfile

### COLL-IDX-1: A blueprint's physics resource is named after it

Status: **verified**

A blueprint `<path>` (its partition path, compared lower-case) has its collision in `<path>_Havok.res` in the same
folder. The suite indexes every `*_havok.res` under the level's data folders by that path; when several folders
(the level's Package bundle, Dist, the shared `_common` bundles) hold one, the first folder in load order wins,
as a later bundle's file is shadowed by an earlier one (the bundle precedence of the data model).

[code: rust/bc2-scene/src/collision.rs:113-138]
[test: rust/bc2-scene/src/collision.rs::index_and_aliases]
[test: core/tests/test_geometry_native.py::main]
[data: MP_005: 592 packfiles indexed; the same index as NavKit's reference on all 37 prepared levels]

### COLL-IDX-2: A doubled suffix names the same blueprint

Status: **verified**

A resource named `<name>_Havok_Havok.res` serves the blueprint `<name>` (and `<name>_Havok`): each trailing
`_havok` is stripped in turn, each shorter name indexed unless already taken.

[code: rust/bc2-scene/src/collision.rs:129-135]
[test: rust/bc2-scene/src/collision.rs::index_and_aliases]
[data: game copy: 4 `_Havok_Havok.res` resources (an animated aircraft door, a wall)]

### COLL-ALIAS-1: Mesh aliases

Status: **verified**

A blueprint without its own physics resource uses the one of the blueprint whose mesh it names: its partition's
`Mesh` field references `<target path>/<target name>_Mesh`; the alias target is that reference's partition path
lower-cased with a trailing `_mesh` removed. Aliases are taken in the order the partitions are listed, the first
alias of a blueprint winning, and only fill blueprints the packfile index lacks. An empty or `null` `Mesh` is no
alias.

[code: rust/bc2-scene/src/collision.rs:96-110]
[code: rust/bc2-scene/src/collision.rs:212-225]
[code: rust/bc2-scene/src/collision.rs:282-290]
[test: rust/bc2-scene/src/collision.rs::mesh_suffixes]
[test: rust/bc2-scene/src/collision.rs::index_and_aliases]
[data: MP_005: 953 aliases, the same table and order as the reference on all 37 prepared levels]

### COLL-ALIAS-2: A deleted partition can leave a stale Mesh

Status: **observed**

A level may hold a `*deleted*` marker for a partition whose converted XML (from an earlier unpack) still names a
`Mesh`. The suite reads `Mesh` from the binary partition when it can and from the XML only where the binary
cannot answer (empty marker, text, damage), as the reference did; the game itself sees only the marker.

[code: rust/bc2-scene/src/collision.rs:229-241]
[data: MP_005's `emitter_sw_meshparticle_tow_01`: an empty deleted marker with a left-over XML naming a Mesh]

## 2. Static model groups

### COLL-MG-1: A model group's members share one resource

Status: **verified**

A static model group partition (`*_HavokAsset.dbx`) holds instances with a `Transforms` field (16 f32 per member:
a 4 x 4 matrix) and an `Assets` array of references to the members' `<blueprint>_Havok` instances. Member k is
placed by matrix k. The group's collision is one `<group>_HavokAsset.res`: an `hkpExtendedMeshShape` whose shape
subparts are the placed members (havok.md HAVOK-AABBW-1), its wrapper in the material-table form (HAVOK-RUN-2).
The group's members are not separate entities to the physics; their face materials are the hit material
(HAVOK-MAT-2).

[code: rust/bc2-scene/src/scene.rs:534-580]
[test: rust/bc2-scene/tests/scene.rs::loads_a_level]
[data: game copy: 529 `_HavokAsset.res` resources, all in the material-table form]

### COLL-MG-2: Model groups must name their physics

Status: **observed**

A model group, model-group member, forest or tree member whose `PhysicsData` or `MeshEntityType` reference is
`null` crashes the game when the level sets up its model groups. Terrain is exempt: its collision comes from the
height field.

[exe: model-group set-up dereferences the reference, crash at 0x00AF7144]
[doc: bfbc2-dbx-tools/FORMAT.md:166-169]

### COLL-MG-3: Model groups must be in the level's Objects

Status: **inferred**

Terrain, model groups, forests and the static environment map are loaded only when listed in the level's
`LevelData.Objects`; one left out is silently not loaded (its collision with it).

[doc: bfbc2-dbx-tools/FORMAT.md:171-172]
[inferred: the level loader walks the Objects list (0x006EFBC0) and loads nothing it does not list]

## 3. Terrain and water

### COLL-TERR-1: Terrain collision is the height field

Status: **inferred**

The terrain has no Havok resource: its collision is the `TerrainHeightfield` (terrain.md), split along the
(i, j)-(i+1, j+1) diagonal. The suite's level loader meshes the height field for its collision and NavKit's samples
agree with it within 0.1 m.

[inferred: no HavokPhysicsData names a terrain; DbxEdit's check exempts terrain from the PhysicsData rule because its collision comes from elsewhere; the in-game samples match the height field (TERR-HF-3)]
[test: core/tests/test_terrain.py::check]

### COLL-WATER-1: Water surfaces are the WaterMesh

Status: **implemented**

The suite takes the water's surface from the `WaterMesh` (water.md), placed in world space as stored.

[code: rust/bc2-formats/src/water.rs:128-151]
[test: core/tests/test_water.py::main]

## Open questions

- Which folder order the game itself uses to resolve a blueprint's `_Havok.res` (the suite's order is the level's
  bundle stack; the data model's precedence rule is the claim).
- Whether the game resolves the `Mesh` alias for physics at all, or the alias only reflects that DICE's blueprints
  share a mesh and the partition's own `PhysicsData` reference decides (the suite follows NavKit's reading).
- How a dynamic (rigid) model group differs from a static one in the files beyond the class of its instance.
- Whether the game uses the WaterMesh for anything (buoyancy, swimming) or only the DBX water entities.
- The exact crash conditions of COLL-MG-2 for forests and tree members (the address is the model-group set-up's).
