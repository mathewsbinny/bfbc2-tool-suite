//! `levelmodel.Model`: what the tool reads from a stock level partition - mode, factions, capture points, soldier
//! spawners and their ground points, M-COMs and the Rush stages of the stock stage logic.
//!
//! Strings are the file's bytes (see [`bc2_formats::dbx::edit`]); positions are the transform's singles as doubles,
//! as Python's `struct.unpack('>f')` gives them. Where `levelmodel` would raise (no `LevelData`, an instance without a
//! `guid` where one is read, a transform that is not 4-byte numbers, a missing event part) or would format a numbers
//! field as text, [`Model::new`] refuses.

use std::collections::{HashMap, HashSet};

use bc2_formats::dbx::edit::{EditDbx, NodeId, numbers_of};
use bc2_formats::dbx::{CONTAINER, NUMBERS, TEXT};

use crate::pytext::{lower, upper};
use crate::{R, Refuse};

/// A position (x, y, z).
pub type Pos = (f64, f64, f64);

/// A string's bytes.
pub type Bytes = Vec<u8>;

const TEAMS: [&[u8]; 2] = [b"Team1", b"Team2"];

/// `levelmodel.local`: the GUID of a local reference, upper case; `None` for none, null or another partition's.
#[must_use]
pub fn local(r: Option<&[u8]>) -> Option<Bytes> {
    let r = r?;
    if r.is_empty() || r == b"null" || r.contains(&b'/') {
        return None;
    }
    Some(upper(r))
}

/// `levelmodel.position`: the translation of the element's `Transform` (`None` without a 16-number one).
///
/// # Errors
/// A `Transform` of numbers that are not 4 bytes (`floats()` raises).
pub fn position(d: &EditDbx, e: NodeId) -> R<Option<Pos>> {
    let Some(t) = d.child(e, b"Transform") else {
        return Ok(None);
    };
    if d.kind(t) != NUMBERS {
        return Ok(None);
    }
    let (size, vals) = numbers_of(d.raw(t)).ok_or(Refuse("damaged numbers"))?;
    if size != 4 {
        return Err(Refuse("a transform not of 4-byte numbers"));
    }
    if vals.len() != 16 {
        return Ok(None);
    }
    let f = |i: usize| -> R<f64> {
        let b = vals.get(i).ok_or(Refuse("transform"))?;
        let b = <[u8; 4]>::try_from(*b).map_err(|_| Refuse("a short single"))?;
        Ok(f64::from(f32::from_be_bytes(b)))
    };
    Ok(Some((f(12)?, f(13)?, f(14)?)))
}

/// `levelmodel.text(e, name)`: a text field's string, a bool / byte's `true` / `false` / number, `None` for a
/// missing field or a container.
///
/// # Errors
/// A numbers field (`levelmodel` formats it with Python's float text).
pub fn text(d: &EditDbx, e: NodeId, name: &[u8]) -> R<Option<Bytes>> {
    let Some(c) = d.child(e, name) else {
        return Ok(None);
    };
    match d.kind(c) {
        TEXT => Ok(d.text(c).map(<[u8]>::to_vec)),
        NUMBERS => Err(Refuse("a numbers field read as text")),
        CONTAINER => Ok(None),
        _ => match d.raw(c).get(1) {
            Some(1) => Ok(Some(b"true".to_vec())),
            Some(0) => Ok(Some(b"false".to_vec())),
            Some(v) => Ok(Some(v.to_string().into_bytes())),
            None => Err(Refuse("a value cut short")),
        },
    }
}

/// [`text`] where only a text field's string can match what the caller compares it with: any other kind gives
/// `None` (a numbers field's text, digits, never equals the words compared).
#[must_use]
pub fn text_only(d: &EditDbx, e: NodeId, name: &[u8]) -> Option<Bytes> {
    let c = d.child(e, name)?;
    d.text(c).map(<[u8]>::to_vec)
}

/// `levelmodel.ref_of`.
#[must_use]
pub fn ref_of<'a>(d: &'a EditDbx, e: NodeId, name: &[u8]) -> Option<&'a [u8]> {
    d.child(e, name).and_then(|c| d.attr(c, b"ref"))
}

/// `levelmodel.item_refs`: the `ref` of each item of an array (`None` for an item without one).
#[must_use]
pub fn item_refs<'a>(d: &'a EditDbx, e: NodeId, name: &[u8]) -> Vec<Option<&'a [u8]>> {
    d.child(e, name)
        .and_then(|a| d.children(a))
        .unwrap_or_default()
        .iter()
        .map(|&i| d.attr(i, b"ref"))
        .collect()
}

fn guid_upper(d: &EditDbx, e: NodeId) -> R<Bytes> {
    d.attr(e, b"guid")
        .map(upper)
        .ok_or(Refuse("an instance without a guid"))
}

/// `levelmodel.CapturePoint`.
#[derive(Debug, Clone, PartialEq)]
pub struct CapturePoint {
    /// Upper-case GUID.
    pub guid: Bytes,
    /// `Name` or empty.
    pub name: Bytes,
    /// `ControlPointName`, else `LocationName`, else empty.
    pub label: Bytes,
    /// Position.
    pub pos: Option<Pos>,
    /// `InitialOwnerTeam`.
    pub owner: Option<Bytes>,
    /// The spawners it lists (local GUIDs).
    pub spawners: Vec<Bytes>,
    /// The capture area's outline (its geometry chain's positions).
    pub outline: Vec<Pos>,
}

/// `levelmodel.Spawner`.
#[derive(Debug, Clone, PartialEq)]
pub struct Spawner {
    /// Upper-case GUID.
    pub guid: Bytes,
    /// `Name` or empty.
    pub name: Bytes,
    /// `Team`.
    pub team: Option<Bytes>,
    /// `PlayerType`.
    pub player_type: Option<Bytes>,
    /// Position.
    pub pos: Option<Pos>,
    /// `Enabled` is true.
    pub enabled: bool,
    /// Ground spawn points: (local GUID, position).
    pub alternates: Vec<(Option<Bytes>, Pos)>,
    /// The capture point that lists it.
    pub flag: Option<Bytes>,
}

/// A placed M-COM: (upper-case GUID, name, position).
pub type Mcom = (Bytes, Bytes, Option<Pos>);

/// `levelmodel.Stage`.
#[derive(Debug, Clone, PartialEq)]
pub struct Stage {
    /// Order.
    pub index: usize,
    /// The stage's `MultipleTrigger` GUID.
    pub trigger: Bytes,
    /// Its `Name`.
    pub trigger_name: Bytes,
    /// Its M-COMs, sorted.
    pub mcoms: Vec<(Bytes, Option<Pos>)>,
}

/// An event connection: (source GUID, source event, target GUID, target event).
pub type Event = (Option<Bytes>, Option<Bytes>, Option<Bytes>, Option<Bytes>);

/// `levelmodel.Model`.
#[derive(Debug, Clone)]
pub struct Model {
    /// Upper-case GUID -> instance (the last of a GUID, as Python's dict).
    pub inst: HashMap<Bytes, NodeId>,
    /// The `LevelData` instance.
    pub level: NodeId,
    /// `LevelData.Name`.
    pub name: Option<Bytes>,
    /// The game mode reference.
    pub game_mode: Bytes,
    /// `conquest`, `rush`, `squadrush`, `sdm`, `tdm` or the game mode reference.
    pub mode: Bytes,
    /// (team, faction) of Team1 and Team2; empty without a level description.
    pub factions: Vec<(Bytes, Option<Bytes>)>,
    /// `AttackerTeam`.
    pub attacker: Option<Bytes>,
    /// `HasAI` is true.
    pub has_ai: bool,
    /// The level's event connections.
    pub events: Vec<Event>,
    /// `LevelData.Objects` (local GUIDs).
    pub objects: Vec<Option<Bytes>>,
    /// Capture points.
    pub capture_points: Vec<CapturePoint>,
    /// Soldier spawners.
    pub spawners: Vec<Spawner>,
    /// Placed M-COMs.
    pub mcoms: Vec<Mcom>,
}

impl Model {
    /// `levelmodel.Model(d)`.
    ///
    /// # Errors
    /// Where `levelmodel` raises or formats a numbers field as text (module documentation).
    #[expect(clippy::too_many_lines, reason = "Model.__init__, field for field")]
    pub fn new(d: &EditDbx) -> R<Self> {
        if !d.has_partition() {
            return Err(Refuse("no partition"));
        }
        let instances = d.instances();
        let mut inst = HashMap::with_capacity(instances.len());
        for &e in &instances {
            inst.insert(upper(d.attr(e, b"guid").unwrap_or_default()), e);
        }
        let level = instances
            .iter()
            .copied()
            .find(|&e| d.attr(e, b"type").unwrap_or_default().ends_with(b".LevelData"))
            .ok_or(Refuse("no LevelData"))?;
        let name = text(d, level, b"Name")?;
        let desc = d.child(level, b"LevelDescription");
        let game_mode = desc
            .and_then(|x| ref_of(d, x, b"GameMode"))
            .unwrap_or_default()
            .to_vec();
        let g = lower(&game_mode);
        let has = |hay: &[u8], needle: &[u8]| hay.windows(needle.len()).any(|w| w == needle);
        let mode = if g.starts_with(b"gamemodes/conquest") {
            b"conquest".to_vec()
        } else if g.starts_with(b"gamemodes/goldrush") {
            b"rush".to_vec()
        } else if has(&g, b"squadrush") {
            b"squadrush".to_vec()
        } else if has(&g, b"squaddeathmatch") {
            b"sdm".to_vec()
        } else if has(&g, b"teamdeathmatch") {
            b"tdm".to_vec()
        } else {
            game_mode.clone()
        };
        let mut factions = Vec::new();
        let mut attacker = None;
        if let Some(desc) = desc {
            for t in TEAMS {
                let mut field = b"Faction".to_vec();
                field.extend_from_slice(t);
                factions.push((t.to_vec(), text(d, desc, &field)?));
            }
            attacker = text(d, desc, b"AttackerTeam")?;
        }
        let has_ai = text(d, level, b"HasAI")?.as_deref() == Some(b"true");
        let mut events = Vec::new();
        if let Some(ec) = d.child(level, b"EventConnections") {
            for &c in d.children(ec).unwrap_or_default() {
                let src = local(ref_of(d, c, b"Source"));
                let tgt = local(ref_of(d, c, b"Target"));
                let se = d
                    .child(c, b"SourceEvent")
                    .ok_or(Refuse("an event without its SourceEvent"))?;
                let te = d
                    .child(c, b"TargetEvent")
                    .ok_or(Refuse("an event without its TargetEvent"))?;
                events.push((src, text(d, se, b"Name")?, tgt, text(d, te, b"Name")?));
            }
        }
        let objects = item_refs(d, level, b"Objects").into_iter().map(local).collect();
        let mut m = Self {
            inst,
            level,
            name,
            game_mode,
            mode,
            factions,
            attacker,
            has_ai,
            events,
            objects,
            capture_points: Vec::new(),
            spawners: Vec::new(),
            mcoms: Vec::new(),
        };
        for e in d.of_type(b".CapturePointEntityData") {
            let cp = m.capture_point(d, e)?;
            m.capture_points.push(cp);
        }
        let mut flag_of: HashMap<&[u8], &[u8]> = HashMap::new();
        for cp in &m.capture_points {
            for s in &cp.spawners {
                flag_of.insert(s, &cp.guid);
            }
        }
        let mut spawners = Vec::new();
        for e in d.of_type(b".SoldierSpawnEntityData") {
            let g = guid_upper(d, e)?;
            let mut alts = Vec::new();
            for r in item_refs(d, e, b"AlternativeSpawnPoints") {
                let l = local(r);
                if let Some(&a) = m.inst.get(l.as_deref().unwrap_or_default())
                    && let Some(p) = position(d, a)?
                {
                    alts.push((l, p));
                }
            }
            spawners.push(Spawner {
                name: text(d, e, b"Name")?.unwrap_or_default(),
                team: text(d, e, b"Team")?,
                player_type: text(d, e, b"PlayerType")?,
                pos: position(d, e)?,
                enabled: text(d, e, b"Enabled")?.as_deref() == Some(b"true"),
                alternates: alts,
                flag: flag_of.get(g.as_slice()).map(|f| f.to_vec()),
                guid: g,
            });
        }
        m.spawners = spawners;
        for e in d.of_type(b".ReferenceObjectData") {
            let r = lower(ref_of(d, e, b"ReferencedObject").unwrap_or_default());
            if r.starts_with(b"objects/objects/mpobjective/") {
                m.mcoms.push((
                    guid_upper(d, e)?,
                    text(d, e, b"Name")?.unwrap_or_default(),
                    position(d, e)?,
                ));
            }
        }
        Ok(m)
    }

    fn capture_point(&self, d: &EditDbx, e: NodeId) -> R<CapturePoint> {
        let guid = guid_upper(d, e)?;
        let mut outline = Vec::new();
        for r in item_refs(d, e, b"Geometry") {
            let mut seen = HashSet::new();
            let mut cur = local(r);
            while let Some(c) = cur.take() {
                let Some(&x) = self.inst.get(&c) else { break };
                if !seen.insert(c) {
                    break;
                }
                if let Some(p) = position(d, x)? {
                    outline.push(p);
                }
                cur = local(ref_of(d, x, b"Next"));
            }
        }
        let name = text(d, e, b"Name")?.unwrap_or_default();
        let label = match text(d, e, b"ControlPointName")? {
            Some(l) if !l.is_empty() => l,
            _ => text(d, e, b"LocationName")?.unwrap_or_default(),
        };
        Ok(CapturePoint {
            guid,
            name,
            label,
            pos: position(d, e)?,
            owner: text(d, e, b"InitialOwnerTeam")?,
            spawners: item_refs(d, e, b"Spawners").into_iter().filter_map(local).collect(),
            outline,
        })
    }

    /// `Model.typ(guid)`: the instance's type after its last dot (`None` for no such instance).
    #[must_use]
    pub fn typ<'a>(&self, d: &'a EditDbx, guid: Option<&[u8]>) -> Option<&'a [u8]> {
        let &e = self.inst.get(guid.unwrap_or_default())?;
        let t = d.attr(e, b"type").unwrap_or_default();
        Some(t.rsplit(|&b| b == b'.').next().unwrap_or(t))
    }

    /// `Model.rush_stages()`: the stages in order, from the stock stage logic.
    ///
    /// # Errors
    /// Two feeds of one trigger by the same M-COM, or a GUID that is not ASCII (Python's sort of them is not
    /// ported), or a trigger name that is a numbers field.
    #[expect(clippy::too_many_lines, reason = "Model.rush_stages, step for step")]
    pub fn rush_stages(&self, d: &EditDbx) -> R<Vec<Stage>> {
        const MT: &[u8] = b"MultipleTriggerEntityData";
        let mut mcom: HashMap<&[u8], Option<Pos>> = HashMap::new();
        for (g, _, p) in &self.mcoms {
            mcom.insert(g, *p);
        }
        // fed: insertion order kept (Python's dict)
        let mut fed_order: Vec<Bytes> = Vec::new();
        let mut fed: HashMap<Bytes, Vec<(Bytes, Option<Pos>)>> = HashMap::new();
        let mut enabled_by: HashMap<Bytes, Option<Bytes>> = HashMap::new();
        let mut enabled_order: Vec<Bytes> = Vec::new();
        for (s, se, t, te) in &self.events {
            if self.typ(d, t.as_deref()) != Some(MT) {
                continue;
            }
            // a non-local target that names a GUID-less trigger: Python keys it None and then fails on it
            let t = t.clone().ok_or(Refuse("a trigger event without a local target"))?;
            if let Some(s) = s
                && let Some(&p) = mcom.get(s.as_slice())
                && se.as_deref() == Some(b"onDestroyed")
                && te.as_deref() == Some(b"Trigger")
            {
                if !fed.contains_key(&t) {
                    fed_order.push(t.clone());
                }
                fed.entry(t.clone()).or_default().push((s.clone(), p));
            }
            if self.typ(d, s.as_deref()) == Some(MT)
                && se.as_deref() == Some(b"OnTrigger")
                && te.as_deref() == Some(b"Enable")
            {
                if !enabled_by.contains_key(&t) {
                    enabled_order.push(t.clone());
                }
                enabled_by.insert(t, s.clone());
            }
        }
        for list in fed.values() {
            let mut gs: Vec<&Bytes> = list.iter().map(|(g, _)| g).collect();
            if gs.iter().any(|g| !g.is_ascii()) {
                return Err(Refuse("a non-ASCII M-COM GUID"));
            }
            gs.sort();
            if gs.windows(2).any(|w| w.first() == w.get(1)) {
                return Err(Refuse("an M-COM feeding one trigger twice"));
            }
        }
        let stage_mts: Vec<Bytes> = fed_order.clone();
        // nxt = {v: k for k, v in enabled_by.items() if k in fed and v in fed}
        let mut nxt: HashMap<Bytes, Bytes> = HashMap::new();
        for k in &enabled_order {
            if let Some(Some(v)) = enabled_by.get(k)
                && fed.contains_key(k)
                && fed.contains_key(v)
            {
                nxt.insert(v.clone(), k.clone());
            }
        }
        let mut firsts: Vec<Bytes> = stage_mts
            .iter()
            .filter(|t| match enabled_by.get(*t) {
                None => true,
                Some(by) => !by.as_ref().is_some_and(|b| fed.contains_key(b)),
            })
            .cloned()
            .collect();
        let chain_len = |t: &Bytes| {
            let mut n = 0usize;
            let mut seen = HashSet::new();
            let mut cur = Some(t.clone());
            while let Some(c) = cur {
                if c.is_empty() || !seen.insert(c.clone()) {
                    break;
                }
                n += 1;
                cur = nxt.get(&c).cloned();
            }
            n
        };
        firsts.sort_by_key(|t| std::cmp::Reverse(chain_len(t))); // stable, as Python's sort
        let Some(first) = firsts.first().cloned() else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        let mut seen: HashSet<Bytes> = HashSet::new();
        let mut cur = Some(first);
        let sorted = |t: &Bytes| -> Vec<(Bytes, Option<Pos>)> {
            let mut v = fed.get(t).cloned().unwrap_or_default();
            v.sort_by(|a, b| a.0.cmp(&b.0));
            v
        };
        let name_of = |t: &Bytes| -> R<Bytes> {
            let &e = self.inst.get(t).ok_or(Refuse("a trigger not in the level"))?;
            Ok(text(d, e, b"Name")?.unwrap_or_default())
        };
        while let Some(c) = cur {
            if c.is_empty() || seen.contains(&c) {
                break;
            }
            seen.insert(c.clone());
            out.push(Stage {
                index: out.len(),
                trigger_name: name_of(&c)?,
                mcoms: sorted(&c),
                trigger: c.clone(),
            });
            cur = nxt.get(&c).cloned();
        }
        let used: HashSet<Bytes> = out
            .iter()
            .flat_map(|st| st.mcoms.iter().map(|(g, _)| g.clone()))
            .collect();
        for t in &stage_mts {
            let list = fed.get(t).map_or_default(Vec::as_slice);
            if !seen.contains(t) && list.iter().all(|(g, _)| !used.contains(g)) && list.len() >= 2 {
                out.push(Stage {
                    index: out.len(),
                    trigger_name: name_of(t)?,
                    mcoms: sorted(t),
                    trigger: t.clone(),
                });
            }
        }
        Ok(out)
    }
}
