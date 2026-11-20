//! How a shader database is found and opened (round SD1): `shaderdb.py`'s `ShaderDb.parse` (the byte order from the
//! first bytes, the Dx9 then the Dx10 / Dx11 layout, the messages), `pick_database` (which of a bundle's databases
//! a Dx means) and `dx_from_name` (the kind a file or entry name says). One implementation for `bfbc2.exe shaderdb`
//! (`bc2-cli`) and for `shaderdb.py` in process (`bc2_native.shaderdb_open`, `shaderdb_pick`, `shaderdb_dx_in_name`).
//!
//! | Python (`shaderdb.py`) | here |
//! |---|---|
//! | `ShaderDb.parse(data, dx, where, big_endian)` | [`open`] (the database, the `dx` it keeps, its byte order) |
//! | `pick_database(path, dx, entry)` after `bundle_databases(path)` | [`pick_database`] |
//! | `re.search(r'dx(9\|10\|11)', basename, re.I)` in `dx_from_name` | [`dx_in_name`] (the base name given) |
//!
//! Names are compared lower-cased as Python's `str.lower` does; a name with a character past U+00FF, where this
//! port does not know Python's rules, makes [`pick_database`] answer `None` (the caller asks the Python).

use super::query::lower_byte;
use super::{ShaderDb, is_big_endian, parse};

/// A database as `ShaderDb.parse` gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened<'a> {
    /// The records.
    pub db: ShaderDb<'a>,
    /// `db.dx`: the layout it was read with (9 or 10 when no kind was given; an Xbox 360 file keeps the kind given,
    /// else 9).
    pub dx: u32,
    /// Read as an Xbox 360 (big-endian) file.
    pub big_endian: bool,
}

/// `ShaderDb.parse(data, dx, where, big_endian)`: a database at least 9 bytes long; `big_endian` None decides the
/// byte order from the first bytes ([`is_big_endian`]). A big-endian file is read once (as `dx`, default 9: its
/// vertex shaders have one layout); a PC file as `dx`'s layout, or with no `dx` (None or 0) as Dx9 then Dx10.
///
/// # Errors
/// `shaderdb.py`'s message, `place` (the `where`) in it: `<place> is not a shader database (too short)`, `<place>
/// is not a shader database this tool reads (as Xbox 360: <place>: <why>)` or `(as Dx9: <place>: <why>; as Dx10:
/// <place>: <why>)`.
pub fn open<'a>(data: &'a [u8], dx: Option<u32>, big_endian: Option<bool>, place: &str) -> Result<Opened<'a>, String> {
    if data.len() < 9 {
        return Err(format!("{place} is not a shader database (too short)"));
    }
    let dx = dx.filter(|&d| d != 0);
    if big_endian.unwrap_or_else(|| is_big_endian(data)) {
        let d = dx.unwrap_or(9);
        return match parse(data, d, true) {
            Ok(db) => Ok(Opened {
                db,
                dx: d,
                big_endian: true,
            }),
            Err(e) => Err(format!(
                "{place} is not a shader database this tool reads (as Xbox 360: {place}: {e})"
            )),
        };
    }
    let mut errors = Vec::new();
    for d in dx.map_or_else(|| vec![9, 10], |d| vec![d]) {
        match parse(data, d, false) {
            Ok(db) => {
                return Ok(Opened {
                    db,
                    dx: d,
                    big_endian: false,
                });
            }
            Err(e) => errors.push(format!("as Dx{d}: {place}: {e}")),
        }
    }
    Err(format!(
        "{place} is not a shader database this tool reads ({})",
        errors.join("; ")
    ))
}

/// Python's `str.lower()` of a `str` within latin-1; None past U+00FF.
#[must_use]
pub fn py_lower_latin1(s: &str) -> Option<String> {
    s.chars()
        .map(|c| u8::try_from(u32::from(c)).ok().map(|b| char::from(lower_byte(b))))
        .collect()
}

/// `pick_database(path, dx, entry)`: the entry name of the Dx`dx` database among a bundle's databases (`dbs`: its
/// (entry name, FbrbTool type) of the types ending `ShaderDatabase`, in bundle order): the one `entry` names (the
/// name, or a folder above it; any case), else the only one, else the only one in the bundle's own level folder
/// (`levels/<folder>/shaders/`, `folder` the bundle's folder name). `path` is the bundle as the messages name it.
///
/// None: a name past U+00FF had to be lower-cased (the caller asks the Python).
///
/// # Errors
/// `shaderdb.py`'s message: `<path> has <n> Dx<dx> shader databases matching <entry>: <names>` or `<path> has <n>
/// Dx<dx> shader databases; name one (<bundle>:<entry>): <names>`.
#[must_use]
pub fn pick_database(
    path: &str,
    dbs: &[(String, String)],
    dx: u32,
    entry: Option<&str>,
    folder: &str,
) -> Option<Result<String, String>> {
    let want = format!("dx{dx}shaderdatabase");
    let mut names: Vec<&str> = Vec::new();
    for (n, t) in dbs {
        // a type past latin-1 never lower-cases to the wanted one
        if py_lower_latin1(t).is_some_and(|l| l == want) {
            names.push(n);
        }
    }
    let joined = names.join(", ");
    if let Some(entry) = entry.filter(|e| !e.is_empty()) {
        let e = py_lower_latin1(entry)?;
        let under = format!("{}/", e.trim_end_matches('/'));
        let mut hit = Vec::new();
        for &n in &names {
            let l = py_lower_latin1(n)?;
            if l == e || l.starts_with(&under) {
                hit.push(n);
            }
        }
        return Some(match hit.as_slice() {
            [one] => Ok((*one).to_owned()),
            _ => Err(format!(
                "{path} has {} Dx{dx} shader databases matching {entry}: {joined}",
                hit.len()
            )),
        });
    }
    if let [one] = names.as_slice() {
        return Some(Ok((*one).to_owned()));
    }
    let own = format!("levels/{}/shaders/", py_lower_latin1(folder)?);
    let mut hit = Vec::new();
    for &n in &names {
        if py_lower_latin1(n)?.starts_with(&own) {
            hit.push(n);
        }
    }
    Some(match hit.as_slice() {
        [one] => Ok((*one).to_owned()),
        _ => Err(format!(
            "{path} has {} Dx{dx} shader databases; name one (<bundle>:<entry>): {joined}",
            names.len()
        )),
    })
}

/// `dx_from_name`'s search of a base name: the first `dx9`, `dx10` or `dx11` in it, any case (`re.search(r'dx(9|10|
/// 11)', name, re.I)`: no other character folds to `d` or `x`).
#[must_use]
pub fn dx_in_name(name: &str) -> Option<u32> {
    let b = name.as_bytes();
    (0..b.len()).find_map(|i| match b.get(i..)? {
        [d, x, rest @ ..] if d.eq_ignore_ascii_case(&b'd') && x.eq_ignore_ascii_case(&b'x') => match rest {
            [b'9', ..] => Some(9),
            [b'1', b'0', ..] => Some(10),
            [b'1', b'1', ..] => Some(11),
            _ => None,
        },
        _ => None,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::shaderdb::write;

    fn empty(big: bool) -> Vec<u8> {
        let mut v = if big {
            120u32.to_be_bytes()
        } else {
            120u32.to_le_bytes()
        }
        .to_vec();
        v.push(0);
        v.extend([0u8; 48]);
        v
    }

    // spec: SHDB-X-1, SHDB-HDR-3
    #[test]
    fn opens_as_shaderdb_py() {
        let pc = empty(false);
        let got = open(&pc, None, None, "f").unwrap();
        assert_eq!((got.dx, got.big_endian), (9, false));
        assert_eq!(open(&pc, Some(11), None, "f").unwrap().dx, 11);
        assert_eq!(open(&pc, Some(0), None, "f").unwrap().dx, 9);
        let big = empty(true);
        let got = open(&big, None, None, "f").unwrap();
        assert_eq!((got.dx, got.big_endian), (9, true));
        assert_eq!(open(&big, Some(10), None, "f").unwrap().dx, 10);
        assert_eq!(write(&got.db), big);
        assert_eq!(
            open(&pc[..8], None, None, "f"),
            Err("f is not a shader database (too short)".into())
        );
        assert_eq!(
            open(&pc[..20], None, None, "w"),
            Err(
                "w is not a shader database this tool reads (as Dx9: w: ends inside a record at 0x11; as Dx10: w: \
                 ends inside a record at 0x11)"
                    .into()
            )
        );
        assert_eq!(
            open(&pc[..20], Some(10), None, "w"),
            Err("w is not a shader database this tool reads (as Dx10: w: ends inside a record at 0x11)".into())
        );
        let as_big = open(&pc, None, Some(true), "x").unwrap();
        assert_eq!((as_big.db.version, as_big.big_endian), (0x7800_0000, true));
        assert_eq!(
            open(&big[..20], Some(11), None, "x"),
            Err("x is not a shader database this tool reads (as Xbox 360: x: ends inside a record at 0x11)".into())
        );
    }

    #[test]
    fn picks_as_shaderdb_py() {
        let dbs: Vec<(String, String)> = [
            ("levels/mp_two/shaders/dx9_single.res", "Dx9ShaderDatabase"),
            ("levels/other/shaders/dx9_single.res", "DX9SHADERDATABASE"),
            ("levels/mp_two/shaders/dx10_single.res", "Dx10ShaderDatabase"),
        ]
        .iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect();
        let p = "b.fbrb";
        assert_eq!(
            pick_database(p, &dbs, 9, None, "MP_TWO"),
            Some(Ok("levels/mp_two/shaders/dx9_single.res".into()))
        );
        assert_eq!(
            pick_database(p, &dbs, 10, None, "x"),
            Some(Ok("levels/mp_two/shaders/dx10_single.res".into()))
        );
        assert_eq!(
            pick_database(p, &dbs, 9, Some("LEVELS/Other//"), "x"),
            Some(Ok("levels/other/shaders/dx9_single.res".into()))
        );
        assert_eq!(
            pick_database(p, &dbs, 9, None, "x"),
            Some(Err("b.fbrb has 2 Dx9 shader databases; name one (<bundle>:<entry>): \
                      levels/mp_two/shaders/dx9_single.res, levels/other/shaders/dx9_single.res"
                .into()))
        );
        assert_eq!(
            pick_database(p, &dbs, 11, Some("levels"), "x"),
            Some(Err("b.fbrb has 0 Dx11 shader databases matching levels: ".into()))
        );
        assert_eq!(pick_database(p, &dbs, 9, Some("\u{130}"), "x"), None);
        assert_eq!(pick_database(p, &dbs, 9, None, "\u{130}"), None);
        assert_eq!(
            pick_database(p, &dbs, 10, None, "\u{130}"),
            Some(Ok("levels/mp_two/shaders/dx10_single.res".into()))
        );
    }

    #[test]
    fn kinds_in_names() {
        for (n, want) in [
            ("Dx9_Single.res", Some(9)),
            ("DX10", Some(10)),
            ("x_dX11", Some(11)),
            ("dx1", None),
            ("dx12", None),
            ("ddxx9", None),
            ("dx99dx10", Some(9)),
            ("x_dx110.res", Some(11)),
            ("caf\u{e9}dx9", Some(9)),
            ("", None),
        ] {
            assert_eq!(dx_in_name(n), want, "{n}");
        }
    }
}
