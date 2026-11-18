//! `author.Author`: the AI layer a configuration describes, inserted into a stock level partition (an [`EditDbx`]),
//! step for step as the reference makes it - the same edits in the same order, so new strings join the table in the
//! same order and the file comes out byte for byte the same.
//!
//! The configuration arrives resolved ([`Config`]: the values the reference reads from its dictionary, in its order).
//! Every `AuthorError` the reference raises, and every value it would pack or format other than the port does,
//! is a [`Refuse`]: the caller then runs the reference.

use std::collections::{HashMap, HashSet};

use bc2_formats::dbx::edit::{EditDbx, NodeId, numbers_of, varint_bytes};
use bc2_formats::dbx::{CONTAINER, NUMBERS, TEXT};

use crate::guid::new_guid;
use crate::model::{Model, text_only};
use crate::pytext::{lower_str, upper, upper_str};
use crate::{R, Refuse};

/// `author.NS`.
pub const NS: &str = "opfor:stockmp";
/// `author.PATHFINDING`: `mp_common`'s infantry and vehicle `PathfindingSettings`.
pub const PATHFINDING: [&str; 2] = [
    "ai/pathfinding/infantry/3096da77-3146-479c-ae35-c822273a54a7",
    "ai/pathfinding/vehicles/de958285-789b-4273-8a8d-a35e95c2612e",
];

/// A configuration value as JSON gives it (`null`, lists and objects are refused where a value is read).
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    /// `true` / `false`.
    Bool(bool),
    /// An integer (within 64 bits).
    Int(i64),
    /// A decimal.
    Float(f64),
    /// A string.
    Str(String),
}

impl Val {
    /// `float(value)` (a string refused).
    fn float(&self) -> R<f64> {
        match self {
            Self::Bool(b) => Ok(f64::from(u8::from(*b))),
            #[expect(
                clippy::cast_precision_loss,
                reason = "Python's float(int): rounded to nearest, as this cast"
            )]
            Self::Int(i) => Ok(*i as f64),
            Self::Float(f) => Ok(*f),
            Self::Str(_) => Err(Refuse("a string where a number is set")),
        }
    }

    /// `str(value)` (a decimal refused: Python's float text is not ported).
    fn text(&self) -> R<String> {
        match self {
            Self::Bool(true) => Ok("True".into()),
            Self::Bool(false) => Ok("False".into()),
            Self::Int(i) => Ok(i.to_string()),
            Self::Str(s) => Ok(s.clone()),
            Self::Float(_) => Err(Refuse("a decimal set as text")),
        }
    }

    /// `value != 0`.
    fn nonzero(&self) -> bool {
        match self {
            Self::Bool(b) => *b,
            Self::Int(i) => *i != 0,
            Self::Float(f) => *f != 0.0,
            Self::Str(_) => true,
        }
    }
}

/// An ordered `dict` of field values: `dict(...)`, `update`, `d[k] = v` keep Python's order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Fields(pub Vec<(String, Val)>);

impl Fields {
    /// `d[k] = v`: in place when the key is there, else at the end.
    pub fn set(&mut self, k: &str, v: Val) {
        match self.0.iter_mut().find(|(x, _)| x == k) {
            Some(slot) => slot.1 = v,
            None => self.0.push((k.to_owned(), v)),
        }
    }

    /// `d.update(other)`.
    pub fn update(&mut self, other: &Fields) {
        for (k, v) in &other.0 {
            self.set(k, v.clone());
        }
    }
}

/// One AI spawner of the configuration, its values resolved as the reference reads them.
#[derive(Debug, Clone, PartialEq)]
pub struct SpawnerCfg {
    /// `id`.
    pub id: String,
    /// `team`.
    pub team: String,
    /// `cfg['teams'][team]['kits'][kit]`.
    pub kit_ref: Val,
    /// `template`, else the configuration's.
    pub template: Val,
    /// `name`, else `OPFOR AI <id>`.
    pub name: Val,
    /// `stage` (`None`: none).
    pub stage: Option<i64>,
    /// `enabled`, else true.
    pub enabled: Val,
    /// `fields`.
    pub fields: Fields,
    /// `pos`.
    pub pos: [f64; 3],
    /// `yaw`, else 0.
    pub yaw: f64,
}

/// One order of the configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderCfg {
    /// `id`.
    pub id: String,
    /// `soldiers` (spawner ids).
    pub soldiers: Vec<String>,
    /// `fields`.
    pub fields: Fields,
    /// `name`, else `OPFOR <id>`.
    pub name: Val,
    /// `stage`.
    pub stage: Option<i64>,
    /// `type`, else `AreaMoveTo`.
    pub area_type: Val,
    /// `pos`.
    pub pos: [f64; 3],
    /// `half_extents`, else (8, 3, 8).
    pub half: [f64; 3],
    /// `issue_on`, else the defaults', else `["spawn"]`.
    pub issue_on: Vec<String>,
    /// `trigger_radius`, else 100.
    pub trigger_radius: f64,
}

/// One Rush stage switch.
#[derive(Debug, Clone, PartialEq)]
pub struct StageCfg {
    /// `index`.
    pub index: i64,
    /// `advance.source`.
    pub source: String,
    /// `advance.event`, else `OnTrigger`.
    pub event: String,
}

/// The configuration as the reference reads it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Config {
    /// `lv`.
    pub lv: String,
    /// `stock_spawners == "human"`.
    pub stock_human: bool,
    /// `layer == "minimal"`.
    pub minimal: bool,
    /// `defaults.spawner`.
    pub defaults_spawner: Fields,
    /// `defaults.order`.
    pub defaults_order: Fields,
    /// `spawners`.
    pub spawners: Vec<SpawnerCfg>,
    /// `orders`.
    pub orders: Vec<OrderCfg>,
    /// `stages`.
    pub stages: Vec<StageCfg>,
}

/// `author.NumberKinds`: per field name, whether its single 4-byte value is an integer or a decimal, learned from
/// the files (a non-zero value counts; one inside +-2^24 as an integer).
#[derive(Debug, Clone, Default)]
pub struct NumberKinds {
    counts: HashMap<Option<Vec<u8>>, (u64, u64)>,
}

impl NumberKinds {
    /// The counts over each file's partition.
    ///
    /// # Errors
    /// A file without a partition.
    pub fn new(files: &[&EditDbx]) -> R<Self> {
        let mut counts: HashMap<Option<Vec<u8>>, (u64, u64)> = HashMap::new();
        for d in files {
            let p = d.partition().ok_or(Refuse("no partition"))?;
            for e in d.walk(p) {
                if d.kind(e) != NUMBERS || d.attributes(e).next().is_none() {
                    continue;
                }
                let Some((size, vals)) = numbers_of(d.raw(e)) else {
                    return Err(Refuse("damaged numbers"));
                };
                if size != 4 || vals.len() != 1 {
                    continue;
                }
                let b = vals
                    .first()
                    .and_then(|v| <[u8; 4]>::try_from(*v).ok())
                    .ok_or(Refuse("a short number"))?;
                let u = u32::from_be_bytes(b);
                if u == 0 {
                    continue;
                }
                let i = u as i32;
                let slot = counts.entry(d.attr(e, b"name").map(<[u8]>::to_vec)).or_default();
                if -(1 << 24) < i && i < (1 << 24) {
                    slot.0 += 1;
                } else {
                    slot.1 += 1;
                }
            }
        }
        Ok(Self { counts })
    }

    /// `NumberKinds.kind(name)`: `Some('i')`, `Some('f')`, or `None` when the files say nothing.
    #[must_use]
    pub fn kind(&self, name: &str) -> Option<char> {
        let &(i, f) = self.counts.get(&Some(name.as_bytes().to_vec()))?;
        if i == 0 && f == 0 {
            return None;
        }
        Some(if i > f { 'i' } else { 'f' })
    }
}

/// `struct.pack('>f', x)` (non-finite or out of range refused).
fn single(x: f64) -> R<[u8; 4]> {
    if !x.is_finite() {
        return Err(Refuse("a non-finite single"));
    }
    let y = x as f32;
    if y.is_infinite() {
        return Err(Refuse("a single out of range"));
    }
    Ok(y.to_be_bytes())
}

fn singles<const N: usize>(xs: [f64; N]) -> R<[f32; N]> {
    let mut out = [0f32; N];
    for (o, x) in out.iter_mut().zip(xs) {
        *o = f32::from_be_bytes(single(x)?);
    }
    Ok(out)
}

/// An ordered `Counter` (the report).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report(pub Vec<(String, i64)>);

impl Report {
    fn add(&mut self, k: &str, n: i64) {
        match self.0.iter_mut().find(|(x, _)| x == k) {
            Some(slot) => slot.1 += n,
            None => self.0.push((k.to_owned(), n)),
        }
    }
}

type Ev = (String, String, String, String);

struct Author<'a> {
    d: EditDbx,
    donor: &'a EditDbx,
    cfg: &'a Config,
    m: Model,
    kinds: NumberKinds,
    level: NodeId,
    stock: HashSet<Vec<u8>>,
    new: Vec<NodeId>,
    events: Vec<Ev>,
    guid_of: HashMap<String, String>,
    report: Report,
    p_spawner: NodeId,
    p_aabb: NodeId,
    p_aabb_donor: bool,
    p_order: NodeId,
    p_trigger: NodeId,
    p_item: NodeId,
    p_event: NodeId,
}

const AUTHOR_ERROR: Refuse = Refuse("an AuthorError");

impl<'a> Author<'a> {
    fn new(d: EditDbx, donor: &'a EditDbx, cfg: &'a Config, m: Model) -> R<Self> {
        let kinds = NumberKinds::new(&[&d, donor])?;
        let sp = d.of_type(b".SoldierSpawnEntityData");
        let p_spawner = *sp.first().ok_or(AUTHOR_ERROR)?;
        let own_aabb = d.of_type(b".AabbGeometryEntityData");
        let (p_aabb, p_aabb_donor) = match own_aabb.first() {
            Some(&a) => (a, false),
            None => (
                *donor
                    .of_type(b".AabbGeometryEntityData")
                    .first()
                    .ok_or(Refuse("no AabbGeometryEntityData (an IndexError)"))?,
                true,
            ),
        };
        let p_order = *donor.of_type(b".AreaOrderEntityData").first().ok_or(AUTHOR_ERROR)?;
        let p_trigger = *donor.of_type(b".AreaTriggerEntityData").first().ok_or(AUTHOR_ERROR)?;
        let level = m.level;
        let first_child = |name: &[u8]| -> R<NodeId> {
            let a = d.child(level, name).ok_or(AUTHOR_ERROR)?;
            d.children(a).and_then(|c| c.first().copied()).ok_or(AUTHOR_ERROR)
        };
        let p_item = first_child(b"Objects")?;
        let p_event = first_child(b"EventConnections")?;
        let stock = d
            .instances()
            .into_iter()
            .map(|e| upper(d.attr(e, b"guid").unwrap_or_default()))
            .collect();
        Ok(Self {
            d,
            donor,
            cfg,
            m,
            kinds,
            level,
            stock,
            new: Vec::new(),
            events: Vec::new(),
            guid_of: HashMap::new(),
            report: Report::default(),
            p_spawner,
            p_aabb,
            p_aabb_donor,
            p_order,
            p_trigger,
            p_item,
            p_event,
        })
    }

    fn guid(&self, role: &str, id: &str) -> String {
        new_guid(NS, &[&self.cfg.lv, role, id])
    }

    /// `Author.instance`: a clone of a prototype (of this file or the donor) with a new GUID, only `guid` and `type`
    /// kept.
    fn instance(&mut self, proto: NodeId, from_donor: bool, guid: &str) -> R<NodeId> {
        // the partition's instances do not change before finish(): the stock GUIDs answer Dbx.instance
        if self.stock.contains(&upper(guid.as_bytes())) {
            return Err(AUTHOR_ERROR);
        }
        let e = if from_donor {
            self.d.copy_from(self.donor, proto)
        } else {
            self.d.clone_node(proto)
        };
        self.d
            .set_attr(e, b"guid", guid.as_bytes())
            .map_err(|_| Refuse("set_attr"))?;
        self.d
            .retain_attrs(e, |k| k == b"guid" || k == b"type")
            .map_err(|_| Refuse("attrs"))?;
        Ok(e)
    }

    fn field(&self, e: NodeId, name: &str) -> R<NodeId> {
        self.d.child(e, name.as_bytes()).ok_or(AUTHOR_ERROR)
    }

    /// `Author.set`: a direct field set in the kind it has.
    fn set(&mut self, e: NodeId, name: &str, value: &Val) -> R<()> {
        let c = self.field(e, name)?;
        let d = &mut self.d;
        let refuse = |_| Refuse("an edit dbxbin refuses");
        if d.attr(c, b"ref").is_some() {
            let Val::Str(s) = value else {
                return Err(Refuse("a reference that is not a string"));
            };
            return d.set_ref(c, s.as_bytes()).map_err(refuse);
        }
        match d.kind(c) {
            TEXT => d.set_text(c, value.text()?.as_bytes()).map_err(refuse),
            NUMBERS => {
                let mut k = self.kinds.kind(name);
                let fv = value.float()?;
                let is_integer = fv.is_finite() && fv.fract() == 0.0;
                if matches!(value, Val::Float(_)) && !is_integer {
                    k = Some(k.unwrap_or('f'));
                }
                let k = match k {
                    Some(k) => k,
                    None if value.nonzero() => return Err(AUTHOR_ERROR),
                    None => 'i',
                };
                let packed = if k == 'i' {
                    if !is_integer {
                        return Err(AUTHOR_ERROR);
                    }
                    // fv is a whole number here; past i64 the cast saturates and i32 refuses it
                    i32::try_from(fv as i64)
                        .map_err(|_| Refuse("an integer out of range"))?
                        .to_be_bytes()
                } else {
                    single(fv)?
                };
                d.set_numbers(c, &[packed]).map_err(refuse)
            }
            CONTAINER => Err(AUTHOR_ERROR),
            _ => {
                let b = match value {
                    Val::Bool(b) => u8::from(*b),
                    Val::Int(i) => u8::try_from(*i).map_err(|_| Refuse("a byte out of range"))?,
                    Val::Float(f) => {
                        if !f.is_finite() {
                            return Err(Refuse("int() of a non-finite"));
                        }
                        u8::try_from(f.trunc() as i64).map_err(|_| Refuse("a byte out of range"))?
                    }
                    Val::Str(_) => return Err(Refuse("int() of a string")),
                };
                d.set_bool(c, b).map_err(refuse)
            }
        }
    }

    /// `Author.set_items`: an array of references; empty as DICE stores it (a text element), filled as a container.
    fn set_items(&mut self, e: NodeId, name: &str, refs: &[String]) -> R<()> {
        let a = self.field(e, name)?;
        let d = &mut self.d;
        let k = d.kind(a);
        if d.tag(a) != b"array"
            || !(k == CONTAINER || k == TEXT)
            || (k == TEXT && !d.text(a).unwrap_or_default().is_empty())
        {
            return Err(AUTHOR_ERROR);
        }
        let refuse = |_| Refuse("an edit dbxbin refuses");
        if refs.is_empty() {
            return d.make_text(a, b"").map_err(refuse);
        }
        d.make_container(a).map_err(refuse)?;
        for r in refs {
            let it = d.clone_node(self.p_item);
            d.set_ref(it, r.as_bytes()).map_err(refuse)?;
            d.children_mut(a).map_err(refuse)?.push(it);
        }
        Ok(())
    }

    #[expect(
        clippy::many_single_char_names,
        reason = "the reference's names: c, s for cos and sin"
    )]
    fn transform(&mut self, e: NodeId, pos: [f64; 3], yaw_deg: f64) -> R<()> {
        let r = yaw_deg.to_radians();
        let (c, s) = (r.cos(), r.sin());
        let rows = [
            singles([c, 0.0, -s])?,
            singles([0.0, 1.0, 0.0])?,
            singles([s, 0.0, c])?,
            singles(pos)?,
        ];
        let t = self.field(e, "Transform")?;
        self.d.set_transform(t, &rows).map_err(|_| Refuse("set_transform"))
    }

    fn event(&mut self, src: &str, se: &str, tgt: &str, te: &str) {
        self.events
            .push((src.to_owned(), se.to_owned(), tgt.to_owned(), te.to_owned()));
    }

    /// `Author.authored_already` (a layer found is an `AuthorError`).
    fn authored_already(&self) -> R<()> {
        let have: HashSet<Vec<u8>> = self
            .d
            .instances()
            .into_iter()
            .map(|e| upper(self.d.attr(e, b"guid").unwrap_or_default()))
            .collect();
        let mut ours = Vec::new();
        for s in &self.cfg.spawners {
            ours.push(self.guid("spawner", &s.id));
        }
        for o in &self.cfg.orders {
            for role in ["order", "area", "trigger"] {
                ours.push(self.guid(role, &o.id));
            }
        }
        if ours.iter().any(|g| have.contains(g.as_bytes())) {
            return Err(AUTHOR_ERROR);
        }
        for e in self.d.instances() {
            if let Some(n) = text_only(&self.d, e, b"Name")
                && (n.starts_with(b"OPFOR area ") || n.starts_with(b"OPFOR trigger "))
            {
                return Err(AUTHOR_ERROR);
            }
        }
        Ok(())
    }

    fn build(mut self) -> R<(EditDbx, Report)> {
        self.authored_already()?;
        let level = self.level;
        self.set(level, "HasAI", &Val::Bool(true))?;
        let pt = self.field(level, "PathfindingTypes")?;
        let have: Vec<Option<Vec<u8>>> = self
            .d
            .children(pt)
            .unwrap_or_default()
            .iter()
            .map(|&i| self.d.attr(i, b"ref").map(<[u8]>::to_vec))
            .collect();
        if have.is_empty() {
            let refs: Vec<String> = PATHFINDING.iter().map(|s| (*s).to_owned()).collect();
            self.set_items(level, "PathfindingTypes", &refs)?;
            self.report.add("PathfindingTypes set", 1);
        } else {
            let lowered: Option<Vec<Vec<u8>>> = have.iter().map(|h| h.as_deref().map(crate::pytext::lower)).collect();
            let lowered = lowered.ok_or(Refuse("a PathfindingTypes item without a ref"))?;
            if lowered.len() != PATHFINDING.len() || lowered.iter().zip(PATHFINDING).any(|(h, p)| h != p.as_bytes()) {
                return Err(AUTHOR_ERROR);
            }
        }
        if self.cfg.stock_human {
            for e in self.d.of_type(b".SoldierSpawnEntityData") {
                if text_only(&self.d, e, b"PlayerType").as_deref() == Some(b"SSAiPlayer") {
                    self.set(e, "PlayerType", &Val::Str("SSHumanPlayer".into()))?;
                    self.report.add("stock spawners made SSHumanPlayer", 1);
                }
            }
        }
        if self.cfg.minimal {
            return self.finish();
        }
        let cfg = self.cfg;
        for s in &cfg.spawners {
            self.spawner(s)?;
        }
        let mut triggers: Vec<((f64, f64, f64), String)> = Vec::new();
        for o in &cfg.orders {
            self.order(o, &mut triggers)?;
        }
        self.stages()?;
        self.finish()
    }

    fn spawner(&mut self, s: &SpawnerCfg) -> R<()> {
        let cfg = self.cfg;
        let g = self.guid("spawner", &s.id);
        if self.guid_of.contains_key(&s.id) {
            return Err(AUTHOR_ERROR);
        }
        self.guid_of.insert(s.id.clone(), g.clone());
        let e = self.instance(self.p_spawner, false, &g)?;
        let mut values = cfg.defaults_spawner.clone();
        values.update(&s.fields);
        values.set("Name", s.name.clone());
        values.set("Team", Val::Str(s.team.clone()));
        let enabled = if matches!(s.stage, None | Some(0)) {
            s.enabled.clone()
        } else {
            Val::Bool(false)
        };
        values.set("Enabled", enabled);
        values.set("SoldierKit", s.kit_ref.clone());
        values.set("Template", s.template.clone());
        values.set("PlayerType", Val::Str("SSAiPlayer".into()));
        for (k, v) in &values.0 {
            self.set(e, k, v)?;
        }
        for arr in ["AlternativeSpawnPoints", "Squad", "Vehicle"] {
            if self.d.child(e, arr.as_bytes()).is_some() {
                self.set_items(e, arr, &[])?;
            }
        }
        self.transform(e, s.pos, s.yaw)?;
        self.new.push(e);
        self.report.add("AI spawners", 1);
        self.report.add(&format!("AI spawners {}", s.team), 1);
        Ok(())
    }

    #[expect(clippy::too_many_lines, reason = "Author.order, step for step")]
    fn order(&mut self, o: &OrderCfg, triggers: &mut Vec<((f64, f64, f64), String)>) -> R<()> {
        let cfg = self.cfg;
        let g = self.guid("order", &o.id);
        self.guid_of.insert(o.id.clone(), g.clone());
        let e = self.instance(self.p_order, true, &g)?;
        let mut values = cfg.defaults_order.clone();
        values.update(&o.fields);
        values.set("Name", o.name.clone());
        values.set("Enabled", Val::Bool(matches!(o.stage, None | Some(0))));
        values.set("AreaType", o.area_type.clone());
        for (k, v) in &values.0 {
            self.set(e, k, v)?;
        }
        let soldiers: Vec<String> = o
            .soldiers
            .iter()
            .map(|x| {
                self.guid_of
                    .get(x)
                    .cloned()
                    .ok_or(Refuse("an unknown soldier (KeyError)"))
            })
            .collect::<R<_>>()?;
        if soldiers.is_empty() {
            return Err(AUTHOR_ERROR);
        }
        let lowered: Vec<String> = soldiers.iter().map(|x| lower_str(x)).collect();
        self.set_items(e, "Soldiers", &lowered)?;
        self.set_items(e, "Squads", &[])?;
        let area = self.instance(self.p_aabb, self.p_aabb_donor, &self.guid("area", &o.id))?;
        let t = self.field(area, "Transform")?;
        let rows = [
            singles([1.0, 0.0, 0.0])?,
            singles([0.0, 1.0, 0.0])?,
            singles([0.0, 0.0, 1.0])?,
            singles(o.pos)?,
        ];
        self.d.set_transform(t, &rows).map_err(|_| Refuse("set_transform"))?;
        let ext = self.field(area, "Extents")?;
        let old: Vec<u8> = {
            let (_, vals) = numbers_of(self.d.raw(ext)).ok_or(Refuse("damaged numbers"))?;
            if vals.len() == 4 {
                vals.get(3).copied().unwrap_or_default().to_vec()
            } else {
                0f32.to_be_bytes().to_vec()
            }
        };
        let mut raw = varint_bytes(4);
        raw.extend(varint_bytes(4));
        for v in o.half {
            raw.extend_from_slice(&single(v)?);
        }
        raw.extend(old);
        self.d.set_raw(ext, raw).map_err(|_| Refuse("set_raw"))?;
        if self.d.child(area, b"Name").is_some() {
            self.set(area, "Name", &Val::Str(format!("OPFOR area {}", o.id)))?;
        }
        let area_guid = String::from_utf8_lossy(self.d.attr(area, b"guid").unwrap_or_default()).to_lowercase();
        self.set_items(e, "Area", &[area_guid])?;
        self.transform(e, o.pos, 0.0)?;
        self.new.push(e);
        self.new.push(area);
        self.report.add("orders", 1);
        self.report.add("order areas", 1);
        for x in &o.issue_on {
            match x.as_str() {
                "spawn" => {
                    for sg in &soldiers {
                        self.event(sg, "OnSpawn", &g, "IssueOrder");
                    }
                }
                "player_enter" => {
                    let hp = self.m_human_player()?;
                    self.event(&hp, "OnPlayerEnter", &g, "IssueOrder");
                }
                "trigger" => {
                    let key = (o.pos[0].round_ties_even(), o.pos[2].round_ties_even(), o.trigger_radius);
                    let found = triggers.iter().find(|(k, _)| *k == key).map(|(_, t)| t.clone());
                    let tg = if let Some(t) = found {
                        t
                    } else {
                        let tg = self.guid("trigger", &o.id);
                        let t = self.instance(self.p_trigger, true, &tg)?;
                        let tv = [
                            ("Name", Val::Str(format!("OPFOR trigger {}", o.id))),
                            ("Enabled", Val::Bool(true)),
                            ("RunOnce", Val::Bool(false)),
                            ("Include", Val::Str("ATPlayers".into())),
                            ("Radius", Val::Float(key.2)),
                            ("Delay", Val::Float(0.0)),
                            ("OneInsideAreaEventPerSoldier", Val::Bool(false)),
                            ("TriggerOnlyOnLeave", Val::Bool(false)),
                        ];
                        for (k, v) in &tv {
                            if self.d.child(t, k.as_bytes()).is_some() {
                                self.set(t, k, v)?;
                            }
                        }
                        for arr in ["Geometry", "Soldiers"] {
                            if self.d.child(t, arr.as_bytes()).is_some() {
                                self.set_items(t, arr, &[])?;
                            }
                        }
                        if self.d.child(t, b"ObjectToFollow").is_some() {
                            self.set(t, "ObjectToFollow", &Val::Str("null".into()))?;
                        }
                        self.transform(t, o.pos, 0.0)?;
                        self.new.push(t);
                        triggers.push((key, tg.clone()));
                        self.report.add("area triggers", 1);
                        tg
                    };
                    self.event(&tg, "OnEnter", &g, "IssueOrder");
                }
                _ => return Err(AUTHOR_ERROR),
            }
        }
        Ok(())
    }

    /// `self.m.d.of_type('.HumanPlayerEntityData')[0]`'s GUID, upper case.
    fn m_human_player(&self) -> R<String> {
        let hp = self.d.of_type(b".HumanPlayerEntityData");
        let &h = hp.first().ok_or(AUTHOR_ERROR)?;
        let g = self
            .d
            .attr(h, b"guid")
            .ok_or(Refuse("a HumanPlayerEntityData without a guid"))?;
        String::from_utf8(upper(g)).map_err(|_| Refuse("a GUID that is not UTF-8"))
    }

    fn stages(&mut self) -> R<()> {
        let cfg = self.cfg;
        if cfg.stages.is_empty() {
            return Ok(());
        }
        let mut by_stage: HashMap<i64, (Vec<String>, Vec<String>)> = HashMap::new();
        for s in &cfg.spawners {
            if let Some(k) = s.stage {
                let g = self.guid_of.get(&s.id).cloned().ok_or(Refuse("KeyError"))?;
                by_stage.entry(k).or_default().0.push(g);
            }
        }
        for o in &cfg.orders {
            if let Some(k) = o.stage {
                let g = self.guid_of.get(&o.id).cloned().ok_or(Refuse("KeyError"))?;
                by_stage.entry(k).or_default().1.push(g);
            }
        }
        for st in &cfg.stages {
            let k = st.index;
            let src = upper_str(&st.source);
            let ev = st.event.clone();
            if !self.m.inst.contains_key(src.as_bytes()) {
                return Err(AUTHOR_ERROR);
            }
            let (sp, or) = by_stage.get(&k).cloned().unwrap_or_default();
            for sg in &sp {
                self.event(&src, &ev, sg, "Disable");
                self.event(&src, &ev, sg, "KillSoldier");
            }
            for og in &or {
                self.event(&src, &ev, og, "Disable");
            }
            let next = k.checked_add(1).ok_or(Refuse("a stage index out of range"))?;
            let (sp, or) = by_stage.get(&next).cloned().unwrap_or_default();
            for sg in &sp {
                self.event(&src, &ev, sg, "Enable");
            }
            for og in &or {
                self.event(&src, &ev, og, "Enable");
                self.event(&src, &ev, og, "IssueOrder");
            }
            self.report.add("stage switches", 1);
        }
        Ok(())
    }

    fn finish(mut self) -> R<(EditDbx, Report)> {
        let refuse = |_| Refuse("an edit dbxbin refuses");
        let part = self.d.partition().ok_or(Refuse("no partition"))?;
        let new = std::mem::take(&mut self.new);
        self.d.children_mut(part).map_err(refuse)?.extend(&new);
        let objs = self.field(self.level, "Objects")?;
        for &e in &new {
            let it = self.d.clone_node(self.p_item);
            let g = lower_bytes(self.d.attr(e, b"guid").ok_or(Refuse("no guid"))?);
            self.d.set_ref(it, &g).map_err(refuse)?;
            self.d.children_mut(objs).map_err(refuse)?.push(it);
        }
        let ec = self.field(self.level, "EventConnections")?;
        let events = std::mem::take(&mut self.events);
        for (s, se, t, te) in &events {
            let c = self.d.clone_node(self.p_event);
            let src = self.d.child(c, b"Source").ok_or(Refuse("an event without Source"))?;
            self.d.set_ref(src, lower_str(s).as_bytes()).map_err(refuse)?;
            let tgt = self.d.child(c, b"Target").ok_or(Refuse("an event without Target"))?;
            self.d.set_ref(tgt, lower_str(t).as_bytes()).map_err(refuse)?;
            let sev = self
                .d
                .child(c, b"SourceEvent")
                .ok_or(Refuse("an event without SourceEvent"))?;
            let sn = self.d.child(sev, b"Name").ok_or(Refuse("an event without its name"))?;
            self.d.set_text(sn, se.as_bytes()).map_err(refuse)?;
            let tev = self
                .d
                .child(c, b"TargetEvent")
                .ok_or(Refuse("an event without TargetEvent"))?;
            let tn = self.d.child(tev, b"Name").ok_or(Refuse("an event without its name"))?;
            self.d.set_text(tn, te.as_bytes()).map_err(refuse)?;
            self.d.children_mut(ec).map_err(refuse)?.push(c);
        }
        self.report.add("event connections", events.len() as i64);
        self.report.add("objects added", new.len() as i64);
        Ok((self.d, self.report))
    }
}

fn lower_bytes(b: &[u8]) -> Vec<u8> {
    crate::pytext::lower(b)
}

/// What `pipeline.build` does with one level partition: `dbxbin.parse(original)` (and its check that the writer
/// gives the file back), `author.Author(d, dbxbin.parse(donor), config, levelmodel.Model(d)).build()`,
/// `dbxbin.write(d)`. Returns the authored file and the report (`dict(Counter)`, in its order).
///
/// # Errors
/// [`Refuse`] wherever the reference raises or would act other than the port (module documentation).
pub fn author(original: &[u8], donor: &[u8], cfg: &Config) -> R<(Vec<u8>, Report)> {
    let d = EditDbx::parse(original).map_err(|_| Refuse("the level is not read"))?;
    if d.write().map_err(|_| Refuse("unwritable"))? != original {
        return Err(Refuse("the writer does not reproduce the original"));
    }
    let donor = EditDbx::parse(donor).map_err(|_| Refuse("the donor is not read"))?;
    if !donor.has_partition() {
        return Err(Refuse("the donor has no partition"));
    }
    let m = Model::new(&d)?;
    let a = Author::new(d, &donor, cfg, m)?;
    let (d, report) = a.build()?;
    let out = d.write().map_err(|_| Refuse("unwritable"))?;
    Ok((out, report))
}
