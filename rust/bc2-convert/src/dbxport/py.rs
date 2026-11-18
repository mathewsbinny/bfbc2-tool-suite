//! Python's string and number semantics the reference's rules lean on: `str.strip`, `str.lower`, `rsplit`, the
//! regular expressions' `$` and `re.I`, `int.from_bytes`, `struct.pack`. Where Python's answer depends on Unicode
//! tables this module does not carry (a `\w` past ASCII), the answer is [`Fail::Unsupported`]: the caller falls back
//! to the Python reference, which decides.

use super::Fail;

/// `Result` with [`Fail`].
pub type R<T> = Result<T, Fail>;

/// Python's `str.isspace` for one character (bidirectional classes WS, B, S and category Zs).
#[must_use]
pub fn is_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n'
            | '\x0b'
            | '\x0c'
            | '\r'
            | '\x1c'..='\x1f'
            | ' '
            | '\u{85}'
            | '\u{a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
    )
}

/// `s.strip()`.
#[must_use]
pub fn strip(s: &str) -> &str {
    s.trim_matches(is_space)
}

/// `s.lower()` (ASCII strings, all the game's names and guids, the ASCII way; others by Unicode's full mapping,
/// as Python's).
#[must_use]
pub fn lower(s: &str) -> String {
    if s.is_ascii() {
        s.to_ascii_lowercase()
    } else {
        s.to_lowercase()
    }
}

/// `s.rsplit(sep, 1)`: `(head, tail)` at the last `sep`, or `None` when there is none.
#[must_use]
pub fn rsplit1(s: &str, sep: char) -> Option<(&str, &str)> {
    s.rsplit_once(sep)
}

/// `s.rsplit(sep, 1)[-1]`.
#[must_use]
pub fn rlast(s: &str, sep: char) -> &str {
    s.rsplit_once(sep).map_or(s, |(_, t)| t)
}

/// `s.rsplit(sep, 1)[0]`.
#[must_use]
pub fn rhead(s: &str, sep: char) -> &str {
    s.rsplit_once(sep).map_or(s, |(h, _)| h)
}

/// `s.split(sep, 1)[0]`.
#[must_use]
pub fn head(s: &str, sep: char) -> &str {
    s.split_once(sep).map_or(s, |(h, _)| h)
}

/// `path.rsplit('.', 1)[-1].split('[', 1)[0]`: the field a path ends in.
#[must_use]
pub fn last_field(path: &str) -> &str {
    head(rlast(path, '.'), '[')
}

/// `path.rsplit('.', 1)[0].rsplit('.', 1)[-1].split('[', 1)[0]`: the struct holding the field (`'Ns.Type.Field'`
/// -> `'Type'`).
#[must_use]
pub fn owner(path: &str) -> &str {
    head(rlast(rhead(path, '.'), '.'), '[')
}

/// A regular expression's `X$` searched in `s` with `X` literal: `s` ends with it, or with it and one newline.
#[must_use]
pub fn ends_dollar(s: &str, suffix: &str) -> bool {
    s.ends_with(suffix) || s.strip_suffix('\n').is_some_and(|t| t.ends_with(suffix))
}

/// The ways `$` can end a match in `s`: at its end, and before a final newline.
fn dollar_ends(s: &str) -> impl Iterator<Item = &str> {
    std::iter::once(s).chain(s.strip_suffix('\n'))
}

/// `re.search(r'FiringFunctionData\..*(Sound|SoundEffect)$', s)` (`.` is no newline).
#[must_use]
pub fn firing_sound(s: &str) -> bool {
    const P: &str = "FiringFunctionData.";
    s.match_indices(P).any(|(i, _)| {
        let rest = s.get(i + P.len()..).unwrap_or_default();
        dollar_ends(rest).any(|r| !r.contains('\n') && (r.ends_with("Sound") || r.ends_with("SoundEffect")))
    })
}

/// `re.search(r'/[0-9a-fA-F-]{36}$', s)`.
#[must_use]
pub fn ends_in_guid(s: &str) -> bool {
    dollar_ends(s).any(|r| {
        let b = r.as_bytes();
        b.len() >= 37
            && b.get(b.len() - 37) == Some(&b'/')
            && b.get(b.len() - 36..)
                .is_some_and(|g| g.iter().all(|c| c.is_ascii_hexdigit() || *c == b'-'))
    })
}

/// A character as `re.I` compares it with an ASCII letter of a pattern: lower case, and the four non-ASCII
/// characters Python folds onto ASCII ones (dotted and dotless i, long s, the Kelvin sign).
fn fold(c: char) -> char {
    match c {
        '\u{130}' | '\u{131}' => 'i',
        '\u{17f}' => 's',
        '\u{212a}' => 'k',
        c => c.to_ascii_lowercase(),
    }
}

/// Whether `pat` (ASCII lower case) occurs in `hay` at char position `at`, as `re.I` compares.
fn ci_at(hay: &[char], at: usize, pat: &str) -> bool {
    let n = pat.chars().count();
    hay.get(at..at + n)
        .is_some_and(|w| w.iter().zip(pat.chars()).all(|(&h, p)| fold(h) == p))
}

/// `re.search(pat, hay, re.I)` for a literal `pat` (ASCII lower case).
#[must_use]
pub fn ci_contains(hay: &str, pat: &str) -> bool {
    let h: Vec<char> = hay.chars().collect();
    (0..=h.len()).any(|i| ci_at(&h, i, pat))
}

/// `re.match(pat, hay, re.I)` for a literal `pat` (ASCII lower case).
#[must_use]
pub fn ci_starts(hay: &str, pat: &str) -> bool {
    let h: Vec<char> = hay.chars().collect();
    ci_at(&h, 0, pat)
}

/// `re.search(r'(^|/)(ai_|soldier)', hay, re.I)`.
#[must_use]
pub fn ci_ai_or_soldier(hay: &str) -> bool {
    let h: Vec<char> = hay.chars().collect();
    (0..=h.len()).any(|i| (i == 0 || h.get(i - 1) == Some(&'/')) && (ci_at(&h, i, "ai_") || ci_at(&h, i, "soldier")))
}

/// `re.fullmatch(r'[A-Za-z][A-Za-z0-9_]*', s)`.
#[must_use]
pub fn is_identifier(s: &str) -> bool {
    let mut c = s.bytes();
    c.next().is_some_and(|b| b.is_ascii_alphabetic()) && c.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// `re.fullmatch(r'EIA[A-Z]\w*', v)`; past ASCII Python's `\w` decides ([`Fail::Unsupported`]).
///
/// # Errors
/// [`Fail::Unsupported`] for a candidate with a non-ASCII character.
pub fn is_eia(v: &str) -> R<bool> {
    let Some(rest) = v.strip_prefix("EIA") else {
        return Ok(false);
    };
    let mut c = rest.chars();
    if !c.next().is_some_and(|x| x.is_ascii_uppercase()) {
        return Ok(false);
    }
    for x in c {
        if !x.is_ascii() {
            return Err(Fail::Unsupported(format!("a \\w test past ASCII: {v:?}")));
        }
        if !(x.is_ascii_alphanumeric() || x == '_') {
            return Ok(false);
        }
    }
    Ok(true)
}

/// `int.from_bytes(b, 'big', signed=True)` (up to 16 bytes; more is left to Python).
///
/// # Errors
/// [`Fail::Unsupported`] for a value past 16 bytes.
pub fn int_be(b: &[u8]) -> R<i128> {
    if b.len() > 16 {
        return Err(Fail::Unsupported("a number past 16 bytes".into()));
    }
    let Some(&first) = b.first() else { return Ok(0) };
    let mut v: i128 = if first & 0x80 != 0 { -1 } else { 0 };
    for &x in b {
        v = (v << 8) | i128::from(x);
    }
    Ok(v)
}

/// `struct.pack('>f', x)`: a NaN (whose bits Python's conversion may change) or a value past a single's range
/// (Python's `OverflowError`) is left to Python.
///
/// # Errors
/// [`Fail::Unsupported`] as said.
pub fn pack_f(x: f64) -> R<[u8; 4]> {
    let y = x as f32;
    if x.is_nan() || (y.is_infinite() && !x.is_infinite()) {
        return Err(Fail::Unsupported(format!("struct.pack('>f', {x})")));
    }
    Ok(y.to_be_bytes())
}

/// `struct.pack('>i', v)`.
///
/// # Errors
/// [`Fail::Unsupported`] past 32 bits (Python's `struct.error`).
pub fn pack_i(v: i128) -> R<[u8; 4]> {
    i32::try_from(v)
        .map(i32::to_be_bytes)
        .map_err(|_| Fail::Unsupported(format!("struct.pack('>i', {v})")))
}

/// Python's `str(x)` of an optional string in an f-string: the string, or `None`.
#[must_use]
pub fn opt(s: Option<&str>) -> &str {
    s.unwrap_or("None")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn strings_as_python() {
        assert_eq!(strip("\x1c a \u{3000}"), "a");
        assert_eq!(last_field("A.B.C[3]"), "C");
        assert_eq!(owner("Ns.Type.Field"), "Type");
        assert_eq!(owner("Field"), "Field");
        assert_eq!(owner("X.Arr[2].F"), "Arr");
        assert!(ends_dollar("aDetonationEffect\n", "DetonationEffect"));
        assert!(!ends_dollar("aDetonationEffect\n\n", "DetonationEffect"));
        assert!(firing_sound("W.FiringFunctionData.Sound"));
        assert!(firing_sound("W.FiringFunctionData.X.SoundEffect\n"));
        assert!(!firing_sound("W.FiringFunctionData.X\n.Sound"));
        assert!(!firing_sound("FiringFunctionData.SoundX"));
        assert!(ends_in_guid("a/12345678-1234-1234-1234-123456789abc"));
        assert!(ends_in_guid("a/12345678-1234-1234-1234-123456789abc\n"));
        assert!(!ends_in_guid("12345678-1234-1234-1234-123456789abc"));
        assert!(ci_contains("XHORNy", "horn"));
        assert!(ci_contains("vehicle\u{17f}", "vehicles"));
        assert!(ci_ai_or_soldier("x/AI_a") && ci_ai_or_soldier("Soldier") && !ci_ai_or_soldier("xai_"));
        assert!(ci_starts("Objects/Weapons/x", "objects/weapons/"));
        assert!(is_identifier("Eia_1") && !is_identifier("1a") && !is_identifier(""));
        assert!(is_eia("EIAFire").unwrap() && !is_eia("EIAfire").unwrap() && !is_eia("EIA").unwrap());
        assert!(is_eia("EIAF\u{e9}").is_err());
        assert_eq!(int_be(&[0xFF, 0xFF]).unwrap(), -1);
        assert_eq!(int_be(&[0x01, 0x00]).unwrap(), 256);
        assert_eq!(int_be(&[]).unwrap(), 0);
        assert!(pack_f(1e300).is_err() && pack_f(f64::NAN).is_err());
        assert_eq!(pack_f(f64::INFINITY).unwrap(), f32::INFINITY.to_be_bytes());
    }
}
