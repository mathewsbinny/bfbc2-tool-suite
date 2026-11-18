//! `dbxport.py`'s tables and its small helpers (`eia_value`, `_shape`, `_short`, `dbx.value`, `dbx.numbers`).
//!
//! The data tables a caller may change (`ONE_TO_ARRAY`, `RENAMED`, `NESTED`, `ENUM_NUMBERS`, `NUMBERS_TO_ITEMS`,
//! `TYPE_AS`, `DICE_MOVED_CLASSES`, `HINT_ACTIONS`, `HINT_TEXTS`) are a [`Rules`] value the Python module builds
//! from its live tables for each port (round E4d's fix: Project OPFOR's `bf1943_vehicles` puts the planes' gear input
//! into `ENUM_NUMBERS` around its ports, and built-in tables ported it as the unchanged table). The rest
//! (`COUNT_CONVERTERS`, `ROLE_GUARDS`, `WEAPON_PARTITION`, `INPUT_FIELDS`, `FIREARM_SPEED`, `AUDIO_ID_SIZE` and the
//! functions) is built in: the Python module compares them with their values at import before each port and ports
//! in Python when one changed.

use std::collections::{HashMap, HashSet};

use bc2_formats::dbx::{self, CONTAINER, NUMBERS, TEXT};

use super::Fail;
use super::arena::{Arena, Id, Tn};
use super::py::{self, R};

/// Per (type or struct, field).
pub type ByField<V> = HashMap<String, HashMap<String, V>>;

/// `NUMBERS_TO_ITEMS`' value: the item field each number goes to, and the other fields' values, in order.
pub type ItemRule = (String, Vec<(String, i128)>);

/// The data tables of `dbxport.py`, as the caller's process has them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rules {
    /// `ONE_TO_ARRAY`: BFBC2's array field -> BF1943's single reference field.
    pub one_to_array: HashMap<String, String>,
    /// `RENAMED`: BFBC2's field -> the BF1943 field it was renamed from.
    pub renamed: HashMap<String, String>,
    /// `NESTED`: (the struct, BFBC2's new struct in it) -> the BF1943 fields moved into it.
    pub nested: ByField<Vec<String>>,
    /// `ENUM_NUMBERS`: (type, field) -> {BF1943's number: BFBC2's name}.
    pub enum_numbers: ByField<HashMap<i128, String>>,
    /// `NUMBERS_TO_ITEMS`: (type, field) -> the item rule.
    pub numbers_to_items: ByField<ItemRule>,
    /// `TYPE_AS`: a BF1943 class BFBC2 lacks -> the class written.
    pub type_as: HashMap<String, String>,
    /// `DICE_MOVED_CLASSES`.
    pub dice_moved: HashSet<String>,
    /// `HINT_ACTIONS`: `eia_value`'s exact spellings.
    pub hint_actions: HashMap<String, String>,
    /// `HINT_TEXTS`: the seat-hint texts.
    pub hint_texts: HashMap<String, String>,
}

fn by_field<V>(rows: impl IntoIterator<Item = ((&'static str, &'static str), V)>) -> ByField<V> {
    let mut m: ByField<V> = HashMap::new();
    for ((t, n), v) in rows {
        m.entry(t.to_owned()).or_default().insert(n.to_owned(), v);
    }
    m
}

fn pairs(rows: &[(&str, &str)]) -> HashMap<String, String> {
    rows.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect()
}

impl Default for Rules {
    /// `dbxport.py`'s tables as it defines them.
    fn default() -> Self {
        let recoil = [
            "MaxRecoilAngleX",
            "MinRecoilAngleX",
            "MaxRecoilAngleY",
            "MinRecoilAngleY",
            "MaxRecoilAngleZ",
            "MinRecoilAngleZ",
            "MaxRecoilFov",
            "MinRecoilFov",
            "RecoilFollowsDispersion",
        ];
        let rotation: HashMap<i128, String> = [
            (0, "EiaThrottle"),
            (4, "EiaYaw"),
            (5, "EiaPitch"),
            (6, "EiaRoll"),
            (7, "EiaFire"),
            (16, "EiaUndefined"),
        ]
        .into_iter()
        .map(|(k, v)| (k, v.to_owned()))
        .collect();
        let moving: HashMap<i128, String> = [(0, "EiaFire"), (7, "EiaFire")]
            .into_iter()
            .map(|(k, v)| (k, v.to_owned()))
            .collect();
        Self {
            one_to_array: pairs(&[("Meshes1p", "Mesh1p"), ("Meshes3p", "Mesh3p")]),
            renamed: pairs(&[
                ("PartInfoDatas", "PartMaterialDatas"),
                ("AngularMomentumDampingAcc", "AngularMomentumDamping"),
                ("AngularMomentumDampingDeacc", "AngularMomentumDamping"),
                ("CollectiveThrottleInputScale", "CollectiveInputScale"),
                ("CollectiveBrakeInputScale", "CollectiveInputScale"),
                ("UpsideDownAngle", "UpsideDownAngel"),
            ]),
            nested: by_field([(
                ("FireLogic", "Recoil"),
                recoil.iter().map(|s| (*s).to_owned()).collect(),
            )]),
            enum_numbers: by_field([
                (("ChildRotationBodyData", "RotationInput"), rotation),
                (("ChildMovingBodyData", "InputAction"), moving),
            ]),
            numbers_to_items: by_field([(
                ("DynamicWeaponPickupEntityData", "WeaponSlots"),
                (
                    "WeaponSlot".to_owned(),
                    vec![("AltWeaponSlot".to_owned(), -1), ("LinkedToWeaponSlot".to_owned(), -1)],
                ),
            )]),
            type_as: pairs(&[(
                "GameSharedResources.LandingGearEntryComponentData",
                "GameSharedResources.PlayerEntryComponentData",
            )]),
            dice_moved: std::iter::once("GameSharedResources.PersistentWeapon".to_owned()).collect(),
            hint_actions: pairs(&[
                ("EIUndefined", "EiaUndefined"),
                ("EIAFireCountermeasure", "EiaFireCountermeasure"),
            ]),
            hint_texts: pairs(&[
                ("VH_SWITCH", "VEHICLE_HINT_CHANGE_POSITION"),
                ("VH_BOAT_THROTTLE", "VEHICLE_HINT_ACCELERATE"),
            ]),
        }
    }
}

impl Rules {
    /// `ONE_TO_ARRAY.get(n)`.
    #[must_use]
    pub fn one_to_array(&self, n: &str) -> Option<&str> {
        self.one_to_array.get(n).map(String::as_str)
    }

    /// `RENAMED.get(n)`.
    #[must_use]
    pub fn renamed(&self, n: &str) -> Option<&str> {
        self.renamed.get(n).map(String::as_str)
    }

    /// `NESTED.get((here, n))`.
    #[must_use]
    pub fn nested(&self, here: &str, n: &str) -> Option<&[String]> {
        self.nested.get(here)?.get(n).map(Vec::as_slice)
    }

    /// `ENUM_NUMBERS.get((t, n))`.
    #[must_use]
    pub fn enum_table(&self, t: &str, n: &str) -> Option<&HashMap<i128, String>> {
        self.enum_numbers.get(t)?.get(n)
    }

    /// `ENUM_NUMBERS.get((t, n), {}).get(num)`.
    #[must_use]
    pub fn enum_name(&self, t: &str, n: &str, num: i128) -> Option<&str> {
        self.enum_table(t, n)?.get(&num).map(String::as_str)
    }

    /// `NUMBERS_TO_ITEMS.get((t, n))`.
    #[must_use]
    pub fn numbers_to_items(&self, t: &str, n: &str) -> Option<&ItemRule> {
        self.numbers_to_items.get(t)?.get(n)
    }

    /// `TYPE_AS.get(t)`.
    #[must_use]
    pub fn type_as(&self, t: &str) -> Option<&str> {
        self.type_as.get(t).map(String::as_str)
    }

    /// `t in DICE_MOVED_CLASSES`.
    #[must_use]
    pub fn dice_moved(&self, t: &str) -> bool {
        self.dice_moved.contains(t)
    }

    /// `eia_value(v)`.
    ///
    /// # Errors
    /// As [`py::is_eia`].
    pub fn eia_value(&self, v: &str) -> R<String> {
        if let Some(x) = self.hint_actions.get(v) {
            return Ok(x.clone());
        }
        if py::is_eia(v)? {
            return Ok(format!("Eia{}", v.get(3..).unwrap_or_default()));
        }
        Ok(v.to_owned())
    }

    /// `HINT_TEXTS.get(v)`.
    #[must_use]
    pub fn hint_text(&self, v: &str) -> Option<&str> {
        self.hint_texts.get(v).map(String::as_str)
    }
}

/// `COUNT_CONVERTERS[(t, n)]` is `gradient_rows`.
#[must_use]
pub fn has_gradient_rows(t: &str, n: &str) -> bool {
    t == "SampleTextureData" && n == "GradientData"
}

/// `gradient_rows(vals, want)`.
#[must_use]
pub fn gradient_rows<'v>(vals: &[&'v [u8]], want: usize) -> Option<Vec<&'v [u8]>> {
    if want == 0 || vals.len() <= want || !vals.len().is_multiple_of(want) {
        return None;
    }
    let mut rows = vals.chunks(want);
    let first = rows.next()?;
    rows.all(|r| r == first).then(|| first.to_vec())
}

/// `INPUT_FIELDS.search(n)`.
#[must_use]
pub fn is_input_field(n: &str) -> bool {
    ["InputAction", "RotationInput", "ActionToSuppress"]
        .iter()
        .any(|s| py::ends_dollar(n, s))
}

/// `WEAPON_PARTITION.search(name)`: `^objects/weapons/`, any case.
#[must_use]
pub fn weapon_partition(name: &str) -> bool {
    py::ci_starts(name, "objects/weapons/")
}

/// The first `ROLE_GUARDS` entry refusing a template's reference to `target` at `path` (the partition ported:
/// `name`), with why.
#[must_use]
pub fn guard(path: &str, target: &str, name: &str) -> Option<&'static str> {
    if py::firing_sound(path) && py::ci_contains(target, "horn") {
        return Some("a horn as a weapon sound");
    }
    if (py::ci_contains(name, "firing") || py::ci_contains(name, "projectile") || py::ci_contains(name, "bomb"))
        && py::ends_dollar(path, "DetonationEffect")
        && (py::ci_contains(target, "vehicleexplosions") || py::ci_contains(target, "/fx_vexp"))
    {
        return Some("a vehicle's death explosion as a weapon's detonation");
    }
    if py::ends_dollar(path, "AnimationEntityData.Animation") && py::ci_ai_or_soldier(target) {
        return Some("a soldier animation on an object");
    }
    None
}

/// `dbx.numbers(e)` on raw bytes: `(size, values)`; what the Rust reader refuses (the reference's `IndexError`,
/// a count past any file) is left to Python.
///
/// # Errors
/// [`Fail::Unsupported`] as said.
pub fn numbers(raw: &[u8]) -> R<(u64, Vec<&[u8]>)> {
    dbx::numbers(raw).map_err(|e| Fail::Unsupported(format!("dbx.numbers: {e}")))
}

/// `_ints(e)`.
///
/// # Errors
/// As [`numbers`] and [`py::int_be`].
pub fn ints(raw: &[u8]) -> R<Vec<i128>> {
    numbers(raw)?.1.into_iter().map(py::int_be).collect()
}

/// `_num(e)`: singles or doubles as Python floats, `[]` for other sizes.
///
/// # Errors
/// As [`numbers`], and a value cut short (`struct.error`).
pub fn floats(raw: &[u8]) -> R<Vec<f64>> {
    let (size, vals) = numbers(raw)?;
    let cut = || Fail::Unsupported("a number cut short".into());
    match size {
        4 => vals
            .into_iter()
            .map(|v| {
                <[u8; 4]>::try_from(v)
                    .map(|b| f64::from(f32::from_be_bytes(b)))
                    .map_err(|_| cut())
            })
            .collect(),
        8 => vals
            .into_iter()
            .map(|v| <[u8; 8]>::try_from(v).map(f64::from_be_bytes).map_err(|_| cut()))
            .collect(),
        _ => Ok(Vec::new()),
    }
}

/// `_set_numbers`' raw bytes: the count and size varints, then the values.
#[must_use]
pub fn numbers_raw<B: AsRef<[u8]>>(vals: &[B], size: u64) -> Vec<u8> {
    let mut raw = Vec::new();
    dbx::put_varint(&mut raw, vals.len() as u64);
    dbx::put_varint(&mut raw, size);
    for v in vals {
        raw.extend_from_slice(v.as_ref());
    }
    raw
}

/// What `dbx.value` raises.
#[derive(Debug)]
pub enum ValueErr {
    /// `ValueError` (a two-byte value of another length).
    Value,
    /// Anything else (left to Python).
    Other(Fail),
}

impl From<ValueErr> for Fail {
    fn from(e: ValueErr) -> Self {
        match e {
            ValueErr::Value => Self::Unsupported("dbx.value: a bool / byte value of other than 2 bytes".into()),
            ValueErr::Other(f) => f,
        }
    }
}

/// `dbx.value(e)` of a node (kind, attributes, text, raw).
///
/// # Errors
/// As said in [`ValueErr`].
pub fn value(ar: &Arena, id: Id) -> Result<String, ValueErr> {
    let n = ar.n(id);
    match n.kind {
        TEXT => Ok(ar.s(n.text).to_owned()),
        NUMBERS => {
            let (size, vals) = numbers(&n.raw).map_err(ValueErr::Other)?;
            let first = n.attrs.first().map_or("", |&(_, v)| ar.s(v));
            let mut out = Vec::with_capacity(vals.len());
            for v in vals {
                out.push(
                    dbx::number_text(v, size, first)
                        .map_err(|e| ValueErr::Other(Fail::Unsupported(format!("dbx.value: {e}"))))?,
                );
            }
            Ok(out.join("/"))
        }
        _ => match n.raw.get(1) {
            Some(&v) if n.raw.len() >= 2 => Ok(match v {
                0 => "false".into(),
                1 => "true".into(),
                v => v.to_string(),
            }),
            _ => Err(ValueErr::Value),
        },
    }
}

/// `dbx.value` of a node of another arena.
///
/// # Errors
/// As [`value`].
pub fn tvalue(t: Tn<'_>) -> Result<String, ValueErr> {
    value(t.ar, t.id)
}

/// `_shape(e)`.
///
/// # Errors
/// As [`numbers`].
pub fn shape(ar: &Arena, id: Id) -> R<String> {
    let n = ar.n(id);
    if n.kind == CONTAINER {
        return Ok(format!("{}-container", ar.s(n.tag)));
    }
    if ar.attr(id, "ref").is_some() {
        return Ok("ref".into());
    }
    if n.kind == TEXT {
        return Ok(if ar.s(n.tag) == "array" { "array-text" } else { "text" }.into());
    }
    if n.kind == NUMBERS {
        let (size, vals) = numbers(&n.raw)?;
        return Ok(format!("num{size}x{}", vals.len()));
    }
    Ok(format!("k{}", n.kind))
}

/// `_short(e)`: a field's value for the report.
///
/// # Errors
/// What `dbx.value` raises besides `ValueError` (that one gives `?`).
pub fn short(ar: &Arena, id: Id) -> R<String> {
    let n = ar.n(id);
    if n.kind == CONTAINER {
        return Ok(format!("<{} {}>", ar.s(n.tag), n.children.len()));
    }
    if let Some(r) = ar.attr(id, "ref") {
        return Ok(r.to_owned());
    }
    let v = match value(ar, id) {
        Ok(v) => v,
        Err(ValueErr::Value) => return Ok("?".into()),
        Err(ValueErr::Other(f)) => return Err(f),
    };
    if v.chars().count() <= 80 {
        return Ok(v);
    }
    Ok(format!("{}...", v.chars().take(77).collect::<String>()))
}

/// `_at_path(e, tpath)`: the element at a schema path (`.A.B`, `.Arr[].C`: an array's first item).
#[must_use]
pub fn at_path<'a>(e: Tn<'a>, tpath: &str) -> Option<Tn<'a>> {
    let mut cur = Some(e);
    for part in path_parts(tpath) {
        let c = cur?;
        cur = match part {
            PathPart::Item => c.kid(0),
            PathPart::Field(f) => c.field(f),
        };
    }
    cur
}

enum PathPart<'p> {
    Field(&'p str),
    Item,
}

/// `re.findall(r'\.([^.\[]+)|(\[\])', tpath)`.
fn path_parts(s: &str) -> Vec<PathPart<'_>> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b.get(i) == Some(&b'.') {
            let start = i + 1;
            let mut j = start;
            while j < b.len() && !matches!(b.get(j), Some(b'.' | b'[')) {
                j += 1;
            }
            if j > start {
                out.push(PathPart::Field(s.get(start..j).unwrap_or_default()));
                i = j;
                continue;
            }
        }
        if b.get(i..i + 2) == Some(b"[]") {
            out.push(PathPart::Item);
            i += 2;
            continue;
        }
        i += 1;
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn tables_and_helpers() {
        let r = Rules::default();
        assert_eq!(r.eia_value("EIAFire").unwrap(), "EiaFire");
        assert_eq!(r.eia_value("EIUndefined").unwrap(), "EiaUndefined");
        assert_eq!(r.eia_value("x").unwrap(), "x");
        assert!(is_input_field("FireInputAction") && !is_input_field("Input"));
        assert_eq!(r.enum_name("ChildMovingBodyData", "InputAction", 7), Some("EiaFire"));
        assert_eq!(
            r.enum_name("ChildRotationBodyData", "RotationInput", 16),
            Some("EiaUndefined")
        );
        // a caller's table is the one asked (OPFOR's gear input: 16 -> EiaDynamicGadget2)
        let mut gear = r.clone();
        if let Some(t) = gear
            .enum_numbers
            .get_mut("ChildRotationBodyData")
            .and_then(|m| m.get_mut("RotationInput"))
        {
            t.insert(16, "EiaDynamicGadget2".into());
        }
        assert_eq!(
            gear.enum_name("ChildRotationBodyData", "RotationInput", 16),
            Some("EiaDynamicGadget2")
        );
        assert!(r.enum_table("ChildRotationBodyData", "Other").is_none());
        assert_eq!(r.nested("FireLogic", "Recoil").map(<[String]>::len), Some(9));
        assert_eq!(
            r.numbers_to_items("DynamicWeaponPickupEntityData", "WeaponSlots")
                .map(|x| x.1.len()),
            Some(2)
        );
        let a: &[u8] = &[1, 2];
        assert_eq!(gradient_rows(&[a, a, a, a], 2).unwrap(), vec![a, a]);
        assert!(gradient_rows(&[a, &[3, 4][..]], 1).is_none());
        assert_eq!(
            guard("X.FiringFunctionData.Sound", "sounds/horn", ""),
            Some("a horn as a weapon sound")
        );
        assert_eq!(
            guard(
                "X.DetonationEffect",
                "fx/vehicleexplosions/a",
                "objects/weapons/x/firing"
            ),
            Some("a vehicle's death explosion as a weapon's detonation")
        );
        assert_eq!(guard("X.DetonationEffect", "fx/vehicleexplosions/a", "x"), None);
        assert_eq!(
            guard("A.AnimationEntityData.Animation", "anims/soldier_x", ""),
            Some("a soldier animation on an object")
        );
        assert!(weapon_partition("Objects/Weapons/m1"));
        let parts = path_parts(".A.B[].C[x]..D");
        assert_eq!(parts.len(), 5);
    }
}
