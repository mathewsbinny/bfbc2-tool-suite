//! `dbxport.py`'s tables (`ONE_TO_ARRAY`, `RENAMED`, `NESTED`, `ENUM_NUMBERS`, `NUMBERS_TO_ITEMS`,
//! `COUNT_CONVERTERS`, `TYPE_AS`, `DICE_MOVED_CLASSES`, `HINT_ACTIONS`, `HINT_TEXTS`, `ROLE_GUARDS`,
//! `WEAPON_PARTITION`) and its small helpers
//! (`eia_value`, `_shape`, `_short`, `dbx.value`, `dbx.numbers`). The Python module checks before each port that its
//! tables are still these (a caller may change them): when not, it ports in Python.

use bc2_formats::dbx::{self, CONTAINER, NUMBERS, TEXT};

use super::Fail;
use super::arena::{Arena, Id, Tn};
use super::py::{self, R};

/// `ONE_TO_ARRAY`: BFBC2's array field -> BF1943's single reference field.
#[must_use]
pub fn one_to_array(n: &str) -> Option<&'static str> {
    match n {
        "Meshes1p" => Some("Mesh1p"),
        "Meshes3p" => Some("Mesh3p"),
        _ => None,
    }
}

/// `RENAMED`: BFBC2's field -> the BF1943 field it was renamed from.
#[must_use]
pub fn renamed(n: &str) -> Option<&'static str> {
    match n {
        "PartInfoDatas" => Some("PartMaterialDatas"),
        "AngularMomentumDampingAcc" | "AngularMomentumDampingDeacc" => Some("AngularMomentumDamping"),
        "CollectiveThrottleInputScale" | "CollectiveBrakeInputScale" => Some("CollectiveInputScale"),
        "UpsideDownAngle" => Some("UpsideDownAngel"),
        _ => None,
    }
}

/// `NESTED`: (the struct, BFBC2's new struct in it) -> the BF1943 fields moved into it.
#[must_use]
pub fn nested(here: &str, n: &str) -> Option<&'static [&'static str]> {
    const RECOIL: &[&str] = &[
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
    (here == "FireLogic" && n == "Recoil").then_some(RECOIL)
}

/// `ENUM_NUMBERS[(t, n)]` exists.
#[must_use]
pub fn has_enum_table(t: &str, n: &str) -> bool {
    matches!(
        (t, n),
        ("ChildRotationBodyData", "RotationInput") | ("ChildMovingBodyData", "InputAction")
    )
}

/// `ENUM_NUMBERS.get((t, n), {}).get(num)`.
#[must_use]
pub fn enum_name(t: &str, n: &str, num: i128) -> Option<&'static str> {
    match (t, n) {
        ("ChildRotationBodyData", "RotationInput") => match num {
            0 => Some("EiaThrottle"),
            4 => Some("EiaYaw"),
            5 => Some("EiaPitch"),
            6 => Some("EiaRoll"),
            7 => Some("EiaFire"),
            16 => Some("EiaUndefined"),
            _ => None,
        },
        ("ChildMovingBodyData", "InputAction") => match num {
            0 | 7 => Some("EiaFire"),
            _ => None,
        },
        _ => None,
    }
}

/// `NUMBERS_TO_ITEMS[(t, n)]`: the item field each number goes to, and the others' values.
#[must_use]
pub fn numbers_to_items(t: &str, n: &str) -> Option<(&'static str, &'static [(&'static str, i128)])> {
    (t == "DynamicWeaponPickupEntityData" && n == "WeaponSlots")
        .then_some(("WeaponSlot", &[("AltWeaponSlot", -1), ("LinkedToWeaponSlot", -1)][..]))
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

/// `TYPE_AS[t]`.
#[must_use]
pub fn type_as(t: &str) -> Option<&'static str> {
    (t == "GameSharedResources.LandingGearEntryComponentData").then_some("GameSharedResources.PlayerEntryComponentData")
}

/// `t in DICE_MOVED_CLASSES`.
#[must_use]
pub fn dice_moved(t: &str) -> bool {
    t == "GameSharedResources.PersistentWeapon"
}

/// `INPUT_FIELDS.search(n)`.
#[must_use]
pub fn is_input_field(n: &str) -> bool {
    ["InputAction", "RotationInput", "ActionToSuppress"]
        .iter()
        .any(|s| py::ends_dollar(n, s))
}

/// `eia_value(v)`.
///
/// # Errors
/// As [`py::is_eia`].
pub fn eia_value(v: &str) -> R<String> {
    match v {
        "EIUndefined" => return Ok("EiaUndefined".into()),
        "EIAFireCountermeasure" => return Ok("EiaFireCountermeasure".into()),
        _ => {}
    }
    if py::is_eia(v)? {
        return Ok(format!("Eia{}", v.get(3..).unwrap_or_default()));
    }
    Ok(v.to_owned())
}

/// `HINT_TEXTS.get(v)`.
#[must_use]
pub fn hint_text(v: &str) -> Option<&'static str> {
    match v {
        "VH_SWITCH" => Some("VEHICLE_HINT_CHANGE_POSITION"),
        "VH_BOAT_THROTTLE" => Some("VEHICLE_HINT_ACCELERATE"),
        _ => None,
    }
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
        assert_eq!(eia_value("EIAFire").unwrap(), "EiaFire");
        assert_eq!(eia_value("EIUndefined").unwrap(), "EiaUndefined");
        assert_eq!(eia_value("x").unwrap(), "x");
        assert!(is_input_field("FireInputAction") && !is_input_field("Input"));
        assert_eq!(enum_name("ChildMovingBodyData", "InputAction", 7), Some("EiaFire"));
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
