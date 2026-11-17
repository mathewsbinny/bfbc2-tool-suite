//! What `DbxEdit.cs` takes from the .NET Framework 4 it is built with, where its answer differs from Rust's own:
//! number parsing (`int` / `byte` / `ushort` / `float` / `double` `TryParse` and `Parse`), case-insensitive
//! comparison, `Path.ChangeExtension` / `GetFileName`, `File.ReadAllText` / `ReadAllLines` decoding and
//! `Directory.GetFiles`' order.

use std::fs;
use std::path::{Path, PathBuf};

use super::fault::{Fault, FaultKind, Result};

/// `Number.IsWhite`: the whitespace .NET's number parser skips (tab to carriage return, space).
fn is_number_white(c: char) -> bool {
    c == ' ' || ('\t'..='\r').contains(&c)
}

/// A number as .NET's `ParseNumber` reads it: sign, significant digits (at most 50 kept), the decimal exponent of
/// the first digit (`0.d1d2... x 10^scale`), and whether any digit was not zero.
struct NetNumber {
    negative: bool,
    digits: String,
    scale: i64,
    nonzero: bool,
}

/// `ParseNumber` with `NumberStyles.Float` (leading and trailing white, a leading sign, a decimal point, an
/// exponent) or, `integer`, with `AllowLeadingSign` alone; invariant culture. None: not a number.
fn parse_number(s: &str, integer: bool) -> Option<NetNumber> {
    let chars: Vec<char> = s.chars().collect();
    let mut p = 0;
    let at = |i: usize| chars.get(i).copied().unwrap_or('\0');
    if !integer {
        while is_number_white(at(p)) && p < chars.len() {
            p += 1;
        }
    }
    let mut negative = false;
    match at(p) {
        '+' => p += 1,
        '-' => {
            negative = true;
            p += 1;
        }
        _ => {}
    }
    let (mut digits, mut scale, mut nonzero, mut decimal, mut any) = (String::new(), 0i64, false, false, false);
    loop {
        let c = at(p);
        if c.is_ascii_digit() {
            any = true;
            if c != '0' || nonzero {
                if digits.len() < 50 {
                    digits.push(c);
                }
                if !decimal {
                    scale += 1;
                }
                nonzero = true;
            } else if decimal {
                scale -= 1;
            }
        } else if !integer && !decimal && c == '.' {
            decimal = true;
        } else {
            break;
        }
        p += 1;
    }
    if !any {
        return None;
    }
    if !integer && (at(p) == 'e' || at(p) == 'E') {
        let mut q = p + 1;
        let mut neg_exp = false;
        match at(q) {
            '+' => q += 1,
            '-' => {
                neg_exp = true;
                q += 1;
            }
            _ => {}
        }
        if at(q).is_ascii_digit() {
            let mut exp: i64 = 0;
            while at(q).is_ascii_digit() {
                exp = exp * 10 + i64::from(at(q) as u8 - b'0');
                q += 1;
                if exp > 1000 {
                    exp = 9999;
                    while at(q).is_ascii_digit() {
                        q += 1;
                    }
                }
            }
            scale += if neg_exp { -exp } else { exp };
            p = q;
        }
    }
    if !integer {
        while p < chars.len() && is_number_white(at(p)) {
            p += 1;
        }
    }
    // what is left may only be NUL characters (TrailingZeros)
    if chars.get(p..).is_some_and(|rest| rest.iter().any(|&c| c != '\0')) {
        return None;
    }
    if !nonzero {
        scale = 0;
        if !decimal {
            negative = false;
        }
    }
    Some(NetNumber {
        negative,
        digits,
        scale,
        nonzero,
    })
}

/// The value of a parsed number as a double (correctly rounded; .NET Framework's `NumberToDouble` can be one unit
/// off for inputs of more than 15 significant digits). Zero is always +0, as the Framework's
/// `NumberBufferToDouble` gives it.
fn to_double(n: &NetNumber) -> f64 {
    if !n.nonzero {
        return 0.0;
    }
    let text = format!("{}0.{}e{}", if n.negative { "-" } else { "" }, n.digits, n.scale);
    text.parse::<f64>().unwrap_or(0.0)
}

/// The special values `TryParse` falls back to: the trimmed text exactly `Infinity`, `-Infinity` or `NaN`.
fn special(s: &str) -> Option<f64> {
    match s.trim() {
        "Infinity" => Some(f64::INFINITY),
        "-Infinity" => Some(f64::NEG_INFINITY),
        "NaN" => Some(f64::NAN),
        _ => None,
    }
}

/// `double.TryParse(s, NumberStyles.Float, CultureInfo.InvariantCulture, out d)`.
#[must_use]
pub fn try_parse_double(s: &str) -> Option<f64> {
    if let Some(n) = parse_number(s, false) {
        let d = to_double(&n);
        if d.is_finite() {
            return Some(d);
        }
    }
    special(s)
}

/// `float.TryParse(s, NumberStyles.Float, CultureInfo.InvariantCulture, out f)`: the double, then rounded to a
/// single (an infinity is a failure).
#[must_use]
pub fn try_parse_single(s: &str) -> Option<f32> {
    if let Some(n) = parse_number(s, false) {
        let d = to_double(&n);
        if d.is_finite() {
            let f = d as f32;
            if f.is_finite() {
                return Some(f);
            }
        }
    }
    special(s).map(|d| d as f32)
}

/// The whole number of a parsed integer, or None when it has a fraction or does not fit `max`.
fn whole(n: &NetNumber, min: i64, max: i64) -> Option<i64> {
    if !n.nonzero {
        return Some(0);
    }
    let len = i64::try_from(n.digits.len()).ok()?;
    if n.scale > 19 || n.scale < len {
        return None;
    }
    let mut v: i64 = 0;
    for (i, c) in n
        .digits
        .chars()
        .chain(std::iter::repeat('0'))
        .take(n.scale as usize)
        .enumerate()
    {
        let _ = i;
        v = v.checked_mul(10)?.checked_add(i64::from(c as u8 - b'0'))?;
    }
    let v = if n.negative { -v } else { v };
    (min..=max).contains(&v).then_some(v)
}

/// `int.TryParse(s, NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out i)`.
#[must_use]
pub fn try_parse_int(s: &str) -> Option<i32> {
    let n = parse_number(s, true)?;
    whole(&n, i64::from(i32::MIN), i64::from(i32::MAX)).map(|v| v as i32)
}

/// Digits only (`NumberStyles.None`), at most `max`: `byte.TryParse` / `ushort.TryParse` as DbxEdit calls them.
#[must_use]
pub fn try_parse_unsigned(s: &str, max: u32) -> Option<u32> {
    let rest = s.trim_end_matches('\0');
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let digits = rest.trim_start_matches('0');
    if digits.len() > 10 {
        return None;
    }
    let v: u64 = if digits.is_empty() { 0 } else { digits.parse().ok()? };
    (v <= u64::from(max)).then_some(v as u32)
}

/// `byte.Parse(s)` (`NumberStyles.Integer`: white around it, a leading sign): `FormatException` or
/// `OverflowException` on failure.
///
/// # Errors
/// The exception `byte.Parse` throws.
pub fn parse_byte(s: &str) -> Result<u8> {
    let trimmed = s.trim_matches(is_number_white);
    let n = parse_number(trimmed, true)
        .ok_or_else(|| Fault::new(FaultKind::Format, "Input string was not in a correct format."))?;
    match whole(&n, i64::MIN, i64::MAX) {
        Some(v) if (0..=255).contains(&v) => Ok(v as u8),
        _ => Err(Fault::new(
            FaultKind::Overflow,
            "Value was either too large or too small for an unsigned byte.",
        )),
    }
}

/// `int.Parse(s, CultureInfo.InvariantCulture)` (`NumberStyles.Integer`).
///
/// # Errors
/// `FormatException` or `OverflowException`.
pub fn parse_int(s: &str) -> Result<i32> {
    let trimmed = s.trim_matches(is_number_white);
    let n = parse_number(trimmed, true)
        .ok_or_else(|| Fault::new(FaultKind::Format, "Input string was not in a correct format."))?;
    whole(&n, i64::from(i32::MIN), i64::from(i32::MAX))
        .map(|v| v as i32)
        .ok_or_else(|| {
            Fault::new(
                FaultKind::Overflow,
                "Value was either too large or too small for an Int32.",
            )
        })
}

/// `Convert.ToInt32(s, 16)`: hex digits (an optional `0x`), 8 at most, read as 32 bits.
///
/// # Errors
/// `FormatException`, `OverflowException`, or `ArgumentOutOfRangeException` for an empty string.
pub fn parse_hex(s: &str) -> Result<i32> {
    if s.is_empty() {
        return Err(Fault::new(
            FaultKind::ArgumentOutOfRange,
            "Index was out of range. Must be non-negative and less than the size of the collection.\nParameter name: startIndex",
        ));
    }
    let digits = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Fault::new(FaultKind::Format, "Could not find any recognizable digits."));
    }
    let digits = digits.trim_start_matches('0');
    if digits.len() > 8 {
        return Err(Fault::new(
            FaultKind::Overflow,
            "Value was either too large or too small for an Int32.",
        ));
    }
    Ok(u32::from_str_radix(if digits.is_empty() { "0" } else { digits }, 16).unwrap_or(0) as i32)
}

/// `char.ConvertFromUtf32(v)`.
///
/// # Errors
/// `ArgumentOutOfRangeException` for a value that is no Unicode scalar (negative, past U+10FFFF, a surrogate).
pub fn from_utf32(v: i32) -> Result<char> {
    u32::try_from(v).ok().and_then(char::from_u32).ok_or_else(|| {
        Fault::new(
            FaultKind::ArgumentOutOfRange,
            "A valid UTF32 value is between 0x000000 and 0x10ffff, inclusive, and should not include surrogate codepoint values (0x00d800 ~ 0x00dfff).\nParameter name: utf32",
        )
    })
}

/// A character upper-cased as .NET's invariant culture does it (one character for one).
fn upper(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_uppercase();
    }
    let mut u = c.to_uppercase();
    match (u.next(), u.next()) {
        (Some(x), None) => x,
        _ => c,
    }
}

/// A character lower-cased as `ToLowerInvariant` does it (one character for one).
fn lower(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_lowercase();
    }
    let mut l = c.to_lowercase();
    l.next().unwrap_or(c)
}

/// `string.ToUpperInvariant()`.
#[must_use]
pub fn to_upper(s: &str) -> String {
    s.chars().map(upper).collect()
}

/// `string.ToLowerInvariant()`.
#[must_use]
pub fn to_lower(s: &str) -> String {
    s.chars().map(lower).collect()
}

/// `string.Equals(a, b, StringComparison.OrdinalIgnoreCase)`.
#[must_use]
pub fn same_ignore_case(a: &str, b: &str) -> bool {
    if a.is_ascii() && b.is_ascii() {
        return a.eq_ignore_ascii_case(b);
    }
    a.chars().map(upper).eq(b.chars().map(upper))
}

/// The length in UTF-16 code units of the prefix of `s` that matches `prefix` ignoring case
/// (`s.StartsWith(prefix, StringComparison.OrdinalIgnoreCase)`), as a byte length of `s`; None when it does not.
#[must_use]
pub fn starts_with_ignore_case(s: &str, prefix: &str) -> Option<usize> {
    if s.is_ascii() && prefix.is_ascii() {
        return (s.len() >= prefix.len()
            && s.as_bytes()
                .get(..prefix.len())
                .is_some_and(|h| h.eq_ignore_ascii_case(prefix.as_bytes())))
        .then_some(prefix.len());
    }
    let mut sc = s.char_indices();
    for p in prefix.chars() {
        let (_, c) = sc.next()?;
        if upper(c) != upper(p) {
            return None;
        }
    }
    Some(sc.next().map_or(s.len(), |(i, _)| i))
}

/// `s.EndsWith(suffix, StringComparison.OrdinalIgnoreCase)`.
#[must_use]
pub fn ends_with_ignore_case(s: &str, suffix: &str) -> bool {
    let (n, m) = (s.chars().count(), suffix.chars().count());
    n >= m && s.chars().skip(n - m).map(upper).eq(suffix.chars().map(upper))
}

fn is_separator(c: char) -> bool {
    matches!(c, '\\' | '/' | ':')
}

/// `Path.ChangeExtension(path, ext)` (`ext` with its dot).
#[must_use]
pub fn change_extension(path: &str, ext: &str) -> String {
    let mut base = path;
    for (i, c) in path.char_indices().rev() {
        if c == '.' {
            base = path.get(..i).unwrap_or(path);
            break;
        }
        if is_separator(c) {
            break;
        }
    }
    if path.is_empty() {
        return String::new();
    }
    format!("{base}{ext}")
}

/// `Path.GetFileName(path)`.
#[must_use]
pub fn file_name(path: &str) -> &str {
    path.rfind(is_separator)
        .map_or(path, |i| path.get(i + 1..).unwrap_or(""))
}

/// Text as `StreamReader` decodes a file: a byte order mark chooses UTF-8, UTF-16 or UTF-32 (and is dropped), else
/// UTF-8; invalid bytes become U+FFFD.
#[must_use]
pub fn decode_text(data: &[u8]) -> String {
    fn utf16(data: &[u8], be: bool) -> String {
        let units: Vec<u16> = data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| {
                if be {
                    u16::from_be_bytes(c)
                } else {
                    u16::from_le_bytes(c)
                }
            })
            .collect();
        let mut s = String::from_utf16_lossy(&units);
        if data.len() % 2 == 1 {
            s.push('\u{FFFD}');
        }
        s
    }
    fn utf32(data: &[u8], be: bool) -> String {
        data.chunks(4)
            .map(|c| match <[u8; 4]>::try_from(c) {
                Ok(w) => char::from_u32(if be {
                    u32::from_be_bytes(w)
                } else {
                    u32::from_le_bytes(w)
                })
                .unwrap_or('\u{FFFD}'),
                Err(_) => '\u{FFFD}',
            })
            .collect()
    }
    match data {
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        [0xFF, 0xFE, 0, 0, rest @ ..] => utf32(rest, false),
        [0, 0, 0xFE, 0xFF, rest @ ..] => utf32(rest, true),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, true),
        [0xFF, 0xFE, rest @ ..] => utf16(rest, false),
        _ => String::from_utf8_lossy(data).into_owned(),
    }
}

/// `File.ReadAllBytes(path)`.
///
/// # Errors
/// The I/O error as .NET words it.
pub fn read_bytes(path: &str) -> Result<Vec<u8>> {
    fs::read(path).map_err(|e| Fault::io(&e, Path::new(path)))
}

/// `File.ReadAllLines(path)`: the text split at `\r\n`, `\n` or `\r` (no empty last line for a final break).
///
/// # Errors
/// The I/O error.
pub fn read_lines(path: &str) -> Result<Vec<String>> {
    let text = decode_text(&read_bytes(path)?);
    let mut out = Vec::new();
    let mut rest = text.as_str();
    while !rest.is_empty() {
        if let Some(i) = rest.find(['\r', '\n']) {
            out.push(rest.get(..i).unwrap_or("").to_owned());
            let skip = if rest.get(i..i + 2) == Some("\r\n") { 2 } else { 1 };
            rest = rest.get(i + skip..).unwrap_or("");
        } else {
            out.push(rest.to_owned());
            rest = "";
        }
    }
    Ok(out)
}

/// The size of a file (`new FileInfo(path).Length`).
///
/// # Errors
/// The I/O error (a missing file: `FileNotFoundException`).
pub fn file_len(path: &str) -> Result<u64> {
    let m = fs::metadata(path).map_err(|e| Fault::io(&e, Path::new(path)))?;
    if m.is_dir() {
        return Err(Fault::new(
            FaultKind::FileNotFound,
            format!("Could not find file '{path}'."),
        ));
    }
    Ok(m.len())
}

/// Whether a file name matches `*.dbx` as Windows' `FindFirstFile` matches it: a 3-letter extension pattern also
/// matches longer extensions that start with it (`a.dbxx`).
fn matches_dbx(name: &str) -> bool {
    name.rfind('.')
        .and_then(|i| name.get(i + 1..))
        .is_some_and(|ext| ext.len() >= 3 && ext.get(..3).is_some_and(|e| e.eq_ignore_ascii_case("dbx")))
}

/// `Directory.GetFiles(dir, "*.dbx", SearchOption.AllDirectories)` in .NET Framework's order: a folder's files in
/// the order Windows lists them, then each of its subfolders in turn (depth first).
///
/// # Errors
/// A folder that cannot be listed.
pub fn dbx_files_under(dir: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut stack: Vec<String> = vec![dir.to_owned()];
    while let Some(d) = stack.pop() {
        let entries = fs::read_dir(&d).map_err(|e| Fault::io(&e, Path::new(&d)))?;
        let mut subdirs = Vec::new();
        let sep = if d.ends_with(['\\', '/']) { "" } else { "\\" };
        for entry in entries {
            let entry = entry.map_err(|e| Fault::io(&e, Path::new(&d)))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
            if is_dir {
                subdirs.push(format!("{d}{sep}{name}"));
            } else if matches_dbx(&name) {
                out.push(format!("{d}{sep}{name}"));
            }
        }
        // the first subfolder is searched next
        stack.extend(subdirs.into_iter().rev());
    }
    Ok(out)
}

/// `DbxFiles(path)`: every `.dbx` under a folder, or the path itself.
///
/// # Errors
/// A folder that cannot be listed.
pub fn dbx_files(path: &str) -> Result<Vec<String>> {
    if Path::new(path).is_dir() {
        dbx_files_under(path)
    } else {
        Ok(vec![path.to_owned()])
    }
}

/// Whether `path` is an existing file (`File.Exists`).
#[must_use]
pub fn is_file(path: &str) -> bool {
    !path.is_empty() && Path::new(path).is_file()
}

/// The path as given, for messages that name it.
#[must_use]
pub fn path_buf(path: &str) -> PathBuf {
    PathBuf::from(path)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn numbers_parse_as_dotnet() {
        assert_eq!(try_parse_single("1.5"), Some(1.5));
        assert_eq!(try_parse_single(" -2e3 "), Some(-2000.0));
        assert_eq!(try_parse_single(".5"), Some(0.5));
        assert_eq!(try_parse_single("5."), Some(5.0));
        assert_eq!(try_parse_single("."), None);
        assert_eq!(try_parse_single("1e"), None);
        assert_eq!(try_parse_single("- 1"), None);
        assert_eq!(try_parse_single("1e39"), None);
        assert_eq!(try_parse_double("1e39"), Some(1e39));
        assert_eq!(try_parse_double("1e400"), None);
        assert!(try_parse_double("NaN").unwrap().is_nan());
        assert_eq!(try_parse_double(" Infinity "), Some(f64::INFINITY));
        assert_eq!(try_parse_double("infinity"), None);
        // zero is always positive in .NET Framework
        assert_eq!(try_parse_single("-0.0").unwrap().to_bits(), 0);
        assert_eq!(try_parse_double("1,5"), None);
        assert_eq!(try_parse_int("-12"), Some(-12));
        assert_eq!(try_parse_int("+7"), Some(7));
        assert_eq!(try_parse_int(" 7"), None);
        assert_eq!(try_parse_int("2147483648"), None);
        assert_eq!(try_parse_int("-2147483648"), Some(i32::MIN));
        assert_eq!(try_parse_int("1.0"), None);
        assert_eq!(try_parse_unsigned("255", 255), Some(255));
        assert_eq!(try_parse_unsigned("256", 255), None);
        assert_eq!(try_parse_unsigned("+1", 255), None);
        assert_eq!(parse_byte(" -0 ").unwrap(), 0);
        assert_eq!(parse_byte("x").unwrap_err().kind, FaultKind::Format);
        assert_eq!(parse_byte("300").unwrap_err().kind, FaultKind::Overflow);
        assert_eq!(parse_int("65").unwrap(), 65);
        assert_eq!(parse_hex("41").unwrap(), 0x41);
        assert_eq!(parse_hex("0x42").unwrap(), 0x42);
        assert_eq!(parse_hex("FFFFFFFF").unwrap(), -1);
        assert!(parse_hex("g").is_err() && parse_hex("").is_err() && parse_hex("123456789").is_err());
        assert!(from_utf32(0xD800).is_err() && from_utf32(-1).is_err());
        assert_eq!(from_utf32(65).unwrap(), 'A');
    }

    #[test]
    fn strings_as_dotnet() {
        assert!(same_ignore_case("abC", "ABc"));
        assert!(same_ignore_case("é", "É"));
        assert_eq!(starts_with_ignore_case("Levels/x", "levels/"), Some(7));
        assert_eq!(starts_with_ignore_case("Lé/x", "lÉ"), Some(3));
        assert_eq!(starts_with_ignore_case("le", "levels"), None);
        assert!(ends_with_ignore_case("a/AB", "/ab"));
        assert_eq!(to_upper("aé"), "AÉ");
        assert_eq!(to_lower("AΣ"), "aσ");
        assert_eq!(change_extension(r"c:\a.b\c.xml", ".dbx"), r"c:\a.b\c.dbx");
        assert_eq!(change_extension(r"c:\a.b\c", ".dbx"), r"c:\a.b\c.dbx");
        assert_eq!(file_name(r"c:\x/y\z.dbx"), "z.dbx");
        assert!(matches_dbx("a.dbx") && matches_dbx("a.DBXx") && !matches_dbx("a.db") && !matches_dbx("dbx"));
        assert_eq!(decode_text(b"\xEF\xBB\xBFab"), "ab");
        assert_eq!(decode_text(b"\xFF\xFEa\0"), "a");
        assert_eq!(decode_text(b"\xFE\xFF\0a"), "a");
    }
}
