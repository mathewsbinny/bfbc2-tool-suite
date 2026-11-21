//! What the Map Inspector showed of an object, folded into the editor (round GR1): the **property tree** (every field
//! of an instance, nested, with its type and its value as DbxEdit's XML writes it) and the **links** between objects
//! (the references in their fields and the level's event connections, both ways).
//!
//! The Map Inspector (`inspector.py`, retired in GR1) read these from Python element trees; the editor reads them
//! from the scene's own partitions ([`Scene::file`]): the level partition (its edited bytes when a project is open)
//! and the static groups' files. An entity's element is its instance in the level partition, or for a static group
//! member the item of its group instance's `Assets` - as `level.Entity.element` was. Its fields are the element's
//! children; a field's name is its `name` attribute, else its tag; a reference is a `ref` attribute.
//!
//! - **References** ([`Links::refs`]): every `ref` in an entity's tree in tree order, with its field path
//!   (`Objects/item`, `Template`), resolved to an entity of the level when one has that guid.
//! - **Referenced by** ([`Links::referrers`]): the entities whose fields reference this one, in entity order, the
//!   level's `LevelData` left out (it lists every object, so it is not a link), as the Map Inspector listed them.
//! - **Event connections** ([`Links::events`]): the `EventConnections` arrays of the level partition's instances
//!   (`LevelData`'s in a level): source, source event, target, target event. An object *sends* the events whose
//!   source it is and *receives* those whose target it is.
//!
//! - **Measure** ([`measure`], [`water_above`]): the Map Inspector's measuring figures of a path of clicked points.
//!
//! `tests/data/inspector_props_*.json.gz` hold what the Map Inspector showed for a sample of instances of four
//! levels (recorded by round GR1's `record_inspector_props.py` before it was retired; removed with it, in the
//! history); `tests/inspect_parity.rs` compares.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::error::EditorError;
use bc2_formats::dbx::{self, DbxTree, NUMBERS, Node, TEXT};
use bc2_scene::scene::{Entity, Scene, Source};

/// One field of the property tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropNode {
    /// Its name: the `name` attribute, else the tag (`item` for a list's entries).
    pub name: String,
    /// The element's tag (`field`, `complex`, `array`, `item`).
    pub tag: String,
    /// Its type in words: `struct`, `array`, `text`, `reference`, `float32`, `float32 x16`, `int32`, `bool` ...
    pub ty: String,
    /// The value as DbxEdit's XML shows it (empty for a container).
    pub value: String,
    /// Its `ref` attribute (`null` included), when it has one.
    pub reference: Option<String>,
    /// A container: its fields (an empty container is still one).
    pub children: Option<Vec<PropNode>>,
}

impl PropNode {
    /// Every field below this one (itself not counted).
    #[must_use]
    pub fn count(&self) -> usize {
        self.children
            .as_ref()
            .map_or(0, |c| c.iter().map(|n| 1 + n.count()).sum())
    }
}

/// The type in words of a non-container element.
fn value_type(n: &Node<'_, '_>) -> String {
    if n.kind() == TEXT {
        return if n.attr("ref").is_some() { "reference" } else { "text" }.to_owned();
    }
    if n.kind() == NUMBERS {
        let Ok((size, vals)) = dbx::numbers(n.raw()) else {
            return "numbers".to_owned();
        };
        let one = match size {
            4 => {
                // DbxEdit's rule (FORMAT.md 3): whole numbers in the hash and id fields and where the top byte is
                // zero or the exponent all ones; a value it writes as a single makes the field a float
                let first = n.attributes().next().map_or("", |(_, v)| v);
                let whole = dbx::WHOLE_NUMBER_FIELDS.contains(&first)
                    || vals.iter().all(|v| {
                        <[u8; 4]>::try_from(*v).map_or(true, |b| {
                            let w = i32::from_be_bytes(b);
                            (w >> 24) == 0 || ((w >> 23) & 0xFF) == 0xFF
                        })
                    });
                if whole { "int32" } else { "float32" }.to_owned()
            }
            8 => "float64".to_owned(),
            s => format!("{s}-byte number"),
        };
        return match vals.len() {
            1 => one,
            k => format!("{one} x{k}"),
        };
    }
    if n.attr("ref").is_some() {
        return "reference".to_owned();
    }
    match n.raw().get(1) {
        Some(0 | 1) => "bool".to_owned(),
        _ => "byte".to_owned(),
    }
}

/// An element's fields as property-tree nodes.
fn nodes_of(el: &Node<'_, '_>) -> Vec<PropNode> {
    el.children()
        .map(|c| {
            let name = c.attr("name").filter(|v| !v.is_empty()).unwrap_or(c.tag()).to_owned();
            let reference = c.attr("ref").map(str::to_owned);
            if c.is_container() {
                let kids = nodes_of(&c);
                let ty = match c.tag() {
                    "array" => format!("array [{}]", kids.len()),
                    "complex" => "struct".to_owned(),
                    t => t.to_owned(),
                };
                PropNode {
                    name,
                    tag: c.tag().to_owned(),
                    ty,
                    value: String::new(),
                    reference,
                    children: Some(kids),
                }
            } else {
                PropNode {
                    ty: value_type(&c),
                    name,
                    tag: c.tag().to_owned(),
                    value: c.value().unwrap_or_else(|e| format!("(unreadable: {e})")),
                    reference,
                    children: None,
                }
            }
        })
        .collect()
}

/// The references in an element's fields, as `level.Entity.refs`: (field path, ref) in tree order, `null` left out.
fn refs_of(el: &Node<'_, '_>, path: &str, out: &mut Vec<(String, String)>) {
    for c in el.children() {
        let name = c.attr("name").filter(|v| !v.is_empty()).unwrap_or(c.tag());
        if let Some(r) = c.attr("ref")
            && r != "null"
        {
            out.push((format!("{path}{name}"), r.to_owned()));
        }
        if c.is_container() {
            refs_of(&c, &format!("{path}{name}/"), out);
        }
    }
}

/// One reference of an entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ref {
    /// The field path (`Objects/item`, `ReferencedObject`).
    pub path: String,
    /// The `ref` value: a guid in this partition, or `partition/guid` in another.
    pub reference: String,
    /// The entity of the level it names, when one has that guid.
    pub target: Option<usize>,
}

/// One event connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// The entity whose `EventConnections` hold it (the level's `LevelData`).
    pub owner: usize,
    /// The source's `ref`.
    pub source_ref: String,
    /// The source entity, when the level has it.
    pub source: Option<usize>,
    /// The event the source sends (`OnPlayerEnter`; `Id <n>` without a name).
    pub source_event: String,
    /// The target's `ref`.
    pub target_ref: String,
    /// The target entity, when the level has it.
    pub target: Option<usize>,
    /// The event the target receives (`Show`).
    pub target_event: String,
}

/// The links between a scene's entities.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Links {
    /// The level's guids (lower case) and their entities; a later entity wins, as the Map Inspector's dict did.
    guids: HashMap<String, usize>,
    refs: Vec<Vec<Ref>>,
    referrers: Vec<Vec<(String, usize)>>,
    /// Every event connection, in partition order.
    pub events: Vec<Event>,
    sends: Vec<Vec<usize>>,
    receives: Vec<Vec<usize>>,
}

impl Links {
    /// The entity a `ref` names (a guid of this level, any case), if any.
    #[must_use]
    pub fn resolve(&self, reference: &str) -> Option<usize> {
        self.guids.get(&reference.to_lowercase()).copied()
    }

    /// Entity `k`'s references, in tree order.
    #[must_use]
    pub fn refs(&self, k: usize) -> &[Ref] {
        self.refs.get(k).map_or(&[], Vec::as_slice)
    }

    /// The entities referencing entity `k` (field path, entity), in entity order; `LevelData` and `k` itself left
    /// out.
    #[must_use]
    pub fn referrers(&self, k: usize) -> &[(String, usize)] {
        self.referrers.get(k).map_or(&[], Vec::as_slice)
    }

    /// The event connections entity `k` is the source of.
    pub fn sends(&self, k: usize) -> impl Iterator<Item = &Event> {
        self.sends
            .get(k)
            .into_iter()
            .flatten()
            .filter_map(|&i| self.events.get(i))
    }

    /// The event connections entity `k` is the target of.
    pub fn receives(&self, k: usize) -> impl Iterator<Item = &Event> {
        self.receives
            .get(k)
            .into_iter()
            .flatten()
            .filter_map(|&i| self.events.get(i))
    }

    /// How many links entity `k` has: references, referrers, events sent and received.
    #[must_use]
    pub fn count(&self, k: usize) -> usize {
        self.refs(k).len()
            + self.referrers(k).len()
            + self.sends.get(k).map_or(0, Vec::len)
            + self.receives.get(k).map_or(0, Vec::len)
    }
}

/// An event's name: its `Name`, else `Id <n>`.
fn event_name(c: Option<Node<'_, '_>>) -> String {
    let Some(c) = c else { return String::new() };
    match c.child("Name").map(|n| n.text()).filter(|t| !t.is_empty()) {
        Some(n) => n.to_owned(),
        None => c
            .child("Id")
            .and_then(|n| n.value().ok())
            .map_or_else(String::new, |v| format!("Id {v}")),
    }
}

/// A scene's partitions read for inspecting: the level partition (the scene's own, or a project's edited bytes) and
/// the static groups' files.
pub struct Inspection {
    /// The level partition's bytes when they are not the scene's (a project's edited partition).
    partition: Option<Vec<u8>>,
    /// The links.
    pub links: Links,
}

/// Where an entity's element is: the file, then the instance (by position, or by guid in an edited partition) and
/// the item of its `Assets` for a group member.
enum Place<'g> {
    Index(usize, usize, Option<usize>),
    Guid(&'g str),
}

impl Inspection {
    /// Reads the scene's links; `partition` is a project's edited level partition (`None`: the scene's own), and
    /// `deleted` the entities a project deleted (no links to or from them).
    #[must_use]
    pub fn new(scene: &Scene, partition: Option<Vec<u8>>, deleted: &BTreeSet<usize>) -> Self {
        let mut me = Self {
            partition,
            links: Links::default(),
        };
        me.links = me.read_links(scene, deleted);
        me
    }

    /// The level partition's bytes this inspection reads.
    fn partition_bytes<'s>(&'s self, scene: &'s Scene) -> Option<&'s [u8]> {
        self.partition.as_deref().or_else(|| scene.file(0).map(|(_, b)| b))
    }

    fn place<'e>(&self, e: &'e Entity) -> Place<'e> {
        match e.source {
            Source::Instance { instance } if self.partition.is_none() && instance != usize::MAX => {
                Place::Index(0, instance, None)
            }
            Source::Instance { .. } => Place::Guid(&e.guid),
            Source::GroupMember { file, instance, item } => Place::Index(file, instance, Some(item)),
        }
    }

    /// Calls `f` with entity `k`'s element.
    ///
    /// # Errors
    /// The partition cannot be read, or the entity's element is not in it.
    pub fn with_element<T>(
        &self,
        scene: &Scene,
        k: usize,
        f: impl FnOnce(&Node<'_, '_>) -> T,
    ) -> Result<T, EditorError> {
        let e = scene
            .entities
            .get(k)
            .ok_or(EditorError::Level("no such object".into()))?;
        let (file, pick) = match self.place(e) {
            Place::Index(file, inst, item) => (file, (Some(inst), item, None)),
            Place::Guid(g) => (0, (None, None, Some(g))),
        };
        let bytes = if file == 0 {
            self.partition_bytes(scene)
        } else {
            scene.file(file).map(|(_, b)| b)
        }
        .ok_or(EditorError::Level("its partition is not loaded".into()))?;
        let tree = dbx::parse_tree(bytes).map_err(|e| EditorError::Level(e.to_string()))?;
        let inst = match pick {
            (Some(i), _, _) => tree.instances().nth(i),
            (_, _, Some(g)) => tree.instance(g),
            _ => None,
        }
        .ok_or_else(|| EditorError::Level(format!("{} is not in its partition", e.guid)))?;
        let el = match pick.1 {
            Some(item) => inst
                .child("Assets")
                .and_then(|a| a.children().nth(item))
                .ok_or_else(|| EditorError::Level(format!("{} has no Assets item {item}", e.guid)))?,
            None => inst,
        };
        Ok(f(&el))
    }

    /// Entity `k`'s property tree: its fields, nested.
    ///
    /// # Errors
    /// As [`Inspection::with_element`].
    pub fn tree(&self, scene: &Scene, k: usize) -> Result<Vec<PropNode>, EditorError> {
        self.with_element(scene, k, nodes_of)
    }

    /// The file an entity's element is in (0: the level partition).
    fn file_of(&self, e: &Entity) -> usize {
        match self.place(e) {
            Place::Index(f, ..) => f,
            Place::Guid(_) => 0,
        }
    }

    /// Reads every entity's references and the level partition's event connections.
    fn read_links(&self, scene: &Scene, deleted: &BTreeSet<usize>) -> Links {
        let n = scene.entities.len();
        let mut refs: Vec<Vec<(String, String)>> = vec![Vec::new(); n];
        // the entities of each file, each file's tree read once
        let mut by_file: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for e in scene.entities.iter().filter(|e| !deleted.contains(&e.index)) {
            by_file.entry(self.file_of(e)).or_default().push(e.index);
        }
        by_file.entry(0).or_default();
        let mut events = Vec::new();
        for (&file, members) in &by_file {
            let bytes = if file == 0 {
                self.partition_bytes(scene)
            } else {
                scene.file(file).map(|(_, b)| b)
            };
            let Some(Ok(tree)) = bytes.map(dbx::parse_tree) else {
                continue;
            };
            let instances: Vec<Node<'_, '_>> = tree.instances().collect();
            let by_guid: HashMap<String, usize> = instances
                .iter()
                .enumerate()
                .filter_map(|(i, n)| n.attr("guid").map(|g| (g.to_ascii_uppercase(), i)))
                .collect();
            for &k in members {
                let Some(e) = scene.entities.get(k) else { continue };
                let (inst, item) = match self.place(e) {
                    Place::Index(_, i, item) => (Some(i), item),
                    Place::Guid(g) => (by_guid.get(&g.to_ascii_uppercase()).copied(), None),
                };
                let Some(node) = inst.and_then(|i| instances.get(i)) else {
                    continue;
                };
                let el = match item {
                    Some(it) => node.child("Assets").and_then(|a| a.children().nth(it)),
                    None => Some(*node),
                };
                if let (Some(el), Some(out)) = (el, refs.get_mut(k)) {
                    refs_of(&el, "", out);
                }
            }
            if file == 0 {
                events = read_events(&tree, scene, deleted);
            }
        }
        // guids of the level, lower case; a later entity wins, as the Map Inspector's dict did
        let mut guids: HashMap<String, usize> = HashMap::new();
        for e in scene.entities.iter().filter(|e| !deleted.contains(&e.index)) {
            guids.insert(e.guid.to_lowercase(), e.index);
        }
        let mut referrers: Vec<Vec<(String, usize)>> = vec![Vec::new(); n];
        let resolved: Vec<Vec<Ref>> = refs
            .into_iter()
            .enumerate()
            .map(|(k, list)| {
                let level_data = scene.entities.get(k).is_some_and(|e| e.short_type == "LevelData");
                list.into_iter()
                    .map(|(path, r)| {
                        let target = guids.get(&r.to_lowercase()).copied();
                        if let Some(t) = target
                            && t != k
                            && !level_data
                            && let Some(list) = referrers.get_mut(t)
                        {
                            list.push((path.clone(), k));
                        }
                        Ref {
                            path,
                            reference: r,
                            target,
                        }
                    })
                    .collect()
            })
            .collect();
        let mut sends = vec![Vec::new(); n];
        let mut receives = vec![Vec::new(); n];
        let resolve = |r: &str| guids.get(&r.to_lowercase()).copied();
        for (i, ev) in events.iter_mut().enumerate() {
            ev.source = resolve(&ev.source_ref);
            ev.target = resolve(&ev.target_ref);
            if let Some(list) = ev.source.and_then(|s| sends.get_mut(s)) {
                list.push(i);
            }
            if let Some(list) = ev.target.and_then(|t| receives.get_mut(t)) {
                list.push(i);
            }
        }
        Links {
            guids,
            refs: resolved,
            referrers,
            events,
            sends,
            receives,
        }
    }
}

/// The event connections of a level partition's instances (owners found by guid in the scene).
fn read_events(tree: &DbxTree<'_>, scene: &Scene, deleted: &BTreeSet<usize>) -> Vec<Event> {
    let owners: HashMap<String, usize> = scene
        .entities
        .iter()
        .filter(|e| matches!(e.source, Source::Instance { .. }) && !deleted.contains(&e.index))
        .map(|e| (e.guid.to_ascii_uppercase(), e.index))
        .collect();
    let mut out = Vec::new();
    for inst in tree.instances() {
        let Some(list) = inst.child("EventConnections") else {
            continue;
        };
        let owner = inst
            .attr("guid")
            .and_then(|g| owners.get(&g.to_ascii_uppercase()).copied())
            .unwrap_or(usize::MAX);
        for c in list.children() {
            let r = |name: &str| c.child(name).and_then(|f| f.attr("ref")).unwrap_or("null").to_owned();
            out.push(Event {
                owner,
                source_ref: r("Source"),
                source: None,
                source_event: event_name(c.child("SourceEvent")),
                target_ref: r("Target"),
                target: None,
                target_event: event_name(c.child("TargetEvent")),
            });
        }
    }
    out
}

/// The Map Inspector's measure (M) figures of a path of points: the last segment's length, horizontal distance,
/// height difference and slope (degrees, up positive), and the whole path's length (`inspector.measure_figures`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measure {
    /// The last segment's length, metres.
    pub segment: f64,
    /// Its horizontal distance (X and Z), metres.
    pub horizontal: f64,
    /// Its height difference (Y), metres.
    pub height: f64,
    /// Its slope, degrees.
    pub slope: f64,
    /// The whole path's length, metres.
    pub total: f64,
}

/// The figures of a path (`None` under two points).
#[must_use]
pub fn measure(points: &[[f64; 3]]) -> Option<Measure> {
    let dist =
        |a: [f64; 3], b: [f64; 3]| ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt();
    let [.., a, b] = points else { return None };
    let flat = (b[0] - a[0]).hypot(b[2] - a[2]);
    let height = b[1] - a[1];
    Some(Measure {
        segment: dist(*a, *b),
        horizontal: flat,
        height,
        slope: height.atan2(flat.max(1e-9)).to_degrees(),
        total: points
            .windows(2)
            .map(|w| match w {
                [a, b] => dist(*a, *b),
                _ => 0.0,
            })
            .sum(),
    })
}

/// The height of the water surface over a point minus the point's height (`None` where there is no water above
/// or below it), as `inspector.water_above`: the first water triangle (in file order) the point lies in, seen from
/// above.
#[must_use]
pub fn water_above(mesh: &bc2_formats::water::WaterMesh, p: [f64; 3]) -> Option<f64> {
    let v = |i: u32| mesh.vertices.get(i as usize).map(|v| v.map(f64::from));
    mesh.triangles.iter().find_map(|t| {
        let (a, b, c) = (v(t[0])?, v(t[1])?, v(t[2])?);
        let d = (b[2] - c[2]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[2] - c[2]);
        let l1 = ((b[2] - c[2]) * (p[0] - c[0]) + (c[0] - b[0]) * (p[2] - c[2])) / d;
        let l2 = ((c[2] - a[2]) * (p[0] - c[0]) + (a[0] - c[0]) * (p[2] - c[2])) / d;
        let l3 = 1.0 - l1 - l2;
        (l1.is_finite() && l1 >= 0.0 && l2 >= 0.0 && l3 >= 0.0).then(|| l1 * a[1] + l2 * b[1] + l3 * c[1] - p[1])
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    /// The demo level with its partition (`demo::level_partition`): `LevelData`, a house and two crates, a spawn, a
    /// flag (its ring 6-9 and marker 10), a vehicle spawn; one event connection, the flag to its marker.
    fn demo() -> Scene {
        let dir = std::env::temp_dir().join(format!("bc2_editor_inspect_{}", std::process::id()));
        crate::demo::level(&dir.join("level")).unwrap()
    }

    #[test]
    fn links_both_ways_and_events() {
        let s = demo();
        let ins = Inspection::new(&s, None, &BTreeSet::new());
        let l = &ins.links;
        // the flag's references in tree order; the house's blueprint is in another partition
        let r: Vec<(&str, Option<usize>)> = l.refs(5).iter().map(|r| (r.path.as_str(), r.target)).collect();
        assert_eq!(r, [("Geometry/item", Some(6)), ("MapMarker", Some(10))]);
        assert_eq!(l.refs(1).len(), 1);
        assert_eq!(l.refs(1)[0].path, "ReferencedObject");
        assert_eq!(l.refs(1)[0].target, None);
        // the ring's first point is referenced by the flag and by the last point; LevelData's Objects is not a link
        assert_eq!(
            l.referrers(6),
            [("Geometry/item".to_owned(), 5), ("Next".to_owned(), 9)]
        );
        assert_eq!(l.referrers(10), [("MapMarker".to_owned(), 5)]);
        assert!(l.referrers(4).is_empty(), "only LevelData lists the spawn");
        // LevelData's own references are listed: every object, then the connection's source and target
        assert_eq!(l.refs(0).len(), 13);
        assert_eq!(l.refs(0)[11].path, "EventConnections/complex/Source");
        // the event: the flag sends OnCaptured to its marker, which has no target event
        let sent: Vec<&Event> = l.sends(5).collect();
        assert_eq!(sent.len(), 1);
        assert_eq!(
            (
                sent[0].source_event.as_str(),
                sent[0].target,
                sent[0].target_event.as_str(),
                sent[0].owner
            ),
            ("OnCaptured", Some(10), "", 0)
        );
        assert_eq!(l.receives(10).count(), 1);
        assert_eq!(l.sends(10).count(), 0);
        assert_eq!(l.count(5), 3, "two references, one event sent");
        assert_eq!(l.count(10), 2, "a referrer, one event received");
        // the same read from a copy of the partition, by guid (as a project's edited partition is read)
        let edited = Inspection::new(&s, Some(crate::demo::level_partition()), &BTreeSet::new());
        assert_eq!(edited.links, ins.links);
    }

    #[test]
    fn property_tree_with_types() {
        let s = demo();
        let ins = Inspection::new(&s, None, &BTreeSet::new());
        let t = ins.tree(&s, 1).unwrap();
        let rows: Vec<(&str, &str, &str)> = t
            .iter()
            .map(|n| (n.name.as_str(), n.ty.as_str(), n.value.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                ("Enumeration", "int32", "1"),
                ("Name", "text", ""),
                ("Transform", "float32 x16", "1/0/0/0/0/1/0/0/0/0/1/0/0/-1/0/0"),
                ("ReferencedObject", "reference", ""),
                ("Health", "float32", "100"),
            ]
        );
        assert_eq!(
            t[3].reference.as_deref(),
            Some("objects/buildings/demo_house/demo_house/d0000000-0000-0000-0000-000000000001")
        );
        let flag = ins.tree(&s, 5).unwrap();
        let geo = &flag[2];
        assert_eq!(
            (geo.name.as_str(), geo.ty.as_str(), geo.count()),
            ("Geometry", "array [1]", 1)
        );
        let item = &geo.children.as_ref().unwrap()[0];
        assert_eq!((item.name.as_str(), item.tag.as_str()), ("item", "item"));
        assert_eq!(item.reference.as_deref(), Some("de300000-0000-0000-0000-000000000006"));
        let ld = ins.tree(&s, 0).unwrap();
        assert_eq!(ld[2].ty, "array [1]");
        assert_eq!(ld[2].children.as_ref().unwrap()[0].ty, "struct");
        assert!(ins.tree(&s, 99).is_err());
    }

    #[test]
    fn measure_as_the_map_inspector() {
        assert_eq!(measure(&[[0.0, 0.0, 0.0]]), None);
        let m = measure(&[[0.0, 0.0, 0.0], [3.0, 0.0, 4.0], [3.0, 5.0, 4.0]]).unwrap();
        assert_eq!((m.segment, m.horizontal, m.height, m.total), (5.0, 0.0, 5.0, 10.0));
        assert!((m.slope - 90.0).abs() < 1e-6);
        let m = measure(&[[0.0, 0.0, 0.0], [10.0, 10.0, 0.0]]).unwrap();
        assert!((m.slope - 45.0).abs() < 1e-9);
        let mesh = bc2_formats::water::WaterMesh {
            vertices: vec![[0.0, 2.0, 0.0], [10.0, 2.0, 0.0], [0.0, 2.0, 10.0]],
            triangles: vec![[0, 1, 2]],
        };
        assert_eq!(water_above(&mesh, [1.0, -3.0, 1.0]), Some(5.0));
        assert_eq!(water_above(&mesh, [9.0, 0.0, 9.0]), None);
    }
}
