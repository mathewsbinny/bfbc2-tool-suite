//! Values as the XML shows them (FORMAT.md 3): .NET's round-trip ("R") formatting of singles and doubles, whole
//! numbers, two-byte values, bools. A port of `dbx._net_g`, `_net_r_single`, `_net_r_double`, `_number_text` and
//! `value`.

use super::cursor::Cursor;
use super::error::{DbxError, Result};
use super::model::{Body, DbxFile, Element, NUMBERS, TEXT};

/// Fields whose 4-byte values are always whole numbers (hashes and ids), as `dbx.WHOLE_NUMBER_FIELDS`.
pub const WHOLE_NUMBER_FIELDS: [&str; 7] = [
    "OriginalHashedWaveName",
    "HashedName",
    "HashedWaveName",
    "OnRoadMaterialNameHashes",
    "Hash",
    "Id",
    "CompositeMeshPartNames",
];

/// The digits and decimal exponent of Rust's scientific formatting (`"1.25e-5"` -> `(b"125", -5)`).
fn sci_digits(s: &str) -> (Vec<u8>, i32) {
    let (mantissa, exp) = s.split_once('e').unwrap_or((s, "0"));
    (
        mantissa.bytes().filter(u8::is_ascii_digit).collect(),
        exp.parse().unwrap_or(0),
    )
}

/// Whether `a` (finite, positive) lies exactly halfway between two `precision`-digit decimals, given `exp10`, the
/// decimal exponent of its first digit. `a = m * 2^e2` with `m` odd: for `e2 < 0` its exact expansion has exactly
/// `-e2` digits after the point, the last a 5, so `-e2 + exp10 + 1` significant digits; a tie has `precision + 1`.
fn is_tie(a: f64, exp10: i32, precision: usize) -> bool {
    let bits = a.to_bits();
    let exp_bits = ((bits >> 52) & 0x7FF) as i32;
    let frac = bits & ((1u64 << 52) - 1);
    let (m, e2) = if exp_bits == 0 {
        (frac, -1074)
    } else {
        (frac | (1u64 << 52), exp_bits - 1075)
    };
    if m == 0 {
        return false;
    }
    let e2 = e2 + m.trailing_zeros() as i32;
    if e2 < 0 {
        return i64::from(-e2) + i64::from(exp10) + 1 == precision as i64 + 1;
    }
    // a whole number: its exact digits
    let whole = format!("{a:.0}");
    let mut rest = whole.bytes().skip(precision);
    rest.next() == Some(b'5') && rest.all(|d| d == b'0')
}

/// The digits of `a` (finite, positive) rounded to `precision` significant digits, a tie rounded away from zero,
/// and the decimal exponent of the first digit: `(b"123", -2)` is 0.0123.
///
/// Rust's formatting rounds the exact binary value to nearest, a tie to even. One more digit than wanted decides
/// the rounding unless that digit is a 5; then [`is_tie`] decides between away from zero and Rust's own rounding.
fn round_half_up(a: f64, precision: usize) -> (Vec<u8>, i32) {
    let (mut digits, mut exp) = sci_digits(&format!("{a:.precision$e}"));
    let next = digits.get(precision).copied().unwrap_or(b'0');
    digits.truncate(precision);
    let up = match next {
        b'5' if !is_tie(a, exp, precision) => {
            let p = precision.saturating_sub(1);
            return sci_digits(&format!("{a:.p$e}"));
        }
        d => d >= b'5',
    };
    if up {
        // add one at the last kept digit, carrying
        let mut carry = true;
        for d in digits.iter_mut().rev() {
            if *d == b'9' {
                *d = b'0';
            } else {
                *d += 1;
                carry = false;
                break;
            }
        }
        if carry {
            digits.insert(0, b'1');
            digits.truncate(precision);
            exp += 1;
        }
    }
    (digits, exp)
}

/// .NET's `"G<precision>"` formatting of a finite double, as `dbx._net_g`: the exact binary value rounded to
/// `precision` significant digits (a tie away from zero), scientific notation (`E+XX` / `E-XX`) for an exponent
/// below -4 or from `precision` up, trailing zeros dropped; zero (either sign) is `0`.
///
/// ```
/// use bc2_formats::dbx::net_g;
/// assert_eq!(net_g(0.125, 2), "0.13");
/// assert_eq!(net_g(-1234.5, 7), "-1234.5");
/// assert_eq!(net_g(1.5e-5, 7), "1.5E-05");
/// assert_eq!(net_g(1e21, 15), "1E+21");
/// ```
#[must_use]
pub fn net_g(value: f64, precision: usize) -> String {
    if value == 0.0 || !value.is_finite() || precision == 0 {
        return "0".to_owned();
    }
    let (digits, exp) = round_half_up(value.abs(), precision);
    let trimmed_len = digits.iter().rposition(|&d| d != b'0').map_or(1, |i| i + 1);
    let digits = String::from_utf8_lossy(digits.get(..trimmed_len).unwrap_or(b"0")).into_owned();
    let sign = if value < 0.0 { "-" } else { "" };
    let p = i32::try_from(precision).unwrap_or(i32::MAX);
    if exp < -4 || exp >= p {
        let (first, rest) = digits.split_at(1);
        let m = if rest.is_empty() {
            first.to_owned()
        } else {
            format!("{first}.{rest}")
        };
        let es = if exp >= 0 { '+' } else { '-' };
        return format!("{sign}{m}E{es}{:02}", exp.unsigned_abs());
    }
    if exp >= 0 {
        let whole_len = exp as usize + 1;
        let (whole, frac) = if digits.len() > whole_len {
            digits.split_at(whole_len)
        } else {
            (digits.as_str(), "")
        };
        let whole = format!("{whole:0<whole_len$}");
        return if frac.is_empty() {
            format!("{sign}{whole}")
        } else {
            format!("{sign}{whole}.{frac}")
        };
    }
    let zeros = "0".repeat((-exp - 1) as usize);
    format!("{sign}0.{zeros}{digits}")
}

/// A single's round-trip text, as `dbx._net_r_single`: 7 significant digits, or 9 when 7 do not read back as the
/// same single (read as a double, then rounded to a single, as Python's `struct.pack('>f', float(s))`).
#[must_use]
#[expect(clippy::float_cmp, reason = "the reference's round-trip test is exact equality")]
pub fn net_r_single(bits: u32) -> String {
    let v = f32::from_bits(bits);
    let s = net_g(f64::from(v), 7);
    let back = s.parse::<f64>().map(|d| d as f32);
    if back.is_ok_and(|b| b == v) {
        s
    } else {
        net_g(f64::from(v), 9)
    }
}

/// A double's round-trip text, as `dbx._net_r_double`: `NaN`, `Infinity`, `-Infinity`, else 15 significant
/// digits, or 17 when 15 do not read back as the same double.
#[must_use]
#[expect(clippy::float_cmp, reason = "the reference's round-trip test is exact equality")]
pub fn net_r_double(bits: u64) -> String {
    let v = f64::from_bits(bits);
    if v.is_nan() {
        return "NaN".to_owned();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }
    let s = net_g(v, 15);
    if s.parse::<f64>().is_ok_and(|b| b == v) {
        s
    } else {
        net_g(v, 17)
    }
}

/// One number's text, as `dbx._number_text`: a 4-byte value is a whole number in a [`WHOLE_NUMBER_FIELDS`] field,
/// when its top byte is zero or its exponent is all ones (an infinity or NaN), else a single; an 8-byte value a
/// double; any other size its first two bytes as an unsigned 16-bit number.
///
/// # Errors
/// [`DbxError::NumberCutShort`] for a 4- or 8-byte value with fewer bytes.
///
/// ```
/// use bc2_formats::dbx::number_text;
/// assert_eq!(number_text(&1.5f32.to_be_bytes(), 4, "Health")?, "1.5");
/// assert_eq!(number_text(&1.5f32.to_be_bytes(), 4, "Id")?, "1069547520");
/// assert_eq!(number_text(&[0x12, 0x34], 2, "")?, "4660");
/// # Ok::<(), bc2_formats::dbx::DbxError>(())
/// ```
pub fn number_text(raw: &[u8], size: u64, field_name: &str) -> Result<String> {
    match size {
        4 => {
            let b: [u8; 4] = raw
                .try_into()
                .map_err(|_| DbxError::NumberCutShort { size, len: raw.len() })?;
            let whole = i32::from_be_bytes(b);
            if WHOLE_NUMBER_FIELDS.contains(&field_name) || (whole >> 24) == 0 || ((whole >> 23) & 0xFF) == 0xFF {
                Ok(whole.to_string())
            } else {
                Ok(net_r_single(u32::from_be_bytes(b)))
            }
        }
        8 => {
            let b: [u8; 8] = raw
                .try_into()
                .map_err(|_| DbxError::NumberCutShort { size, len: raw.len() })?;
            Ok(net_r_double(u64::from_be_bytes(b)))
        }
        _ => Ok(match raw {
            [a, b, ..] => (u16::from(*a) << 8 | u16::from(*b)).to_string(),
            [a] => a.to_string(),
            [] => "0".to_owned(),
        }),
    }
}

/// The most values a numbers element may have beyond one per byte of its data. Past the data every value is
/// empty (the reference's slices past the end); the reference would build a list of any length, this refuses one
/// longer than any real file.
pub(crate) const MAX_EMPTY_VALUES: u64 = 1 << 20;

/// [`DbxError::TooManyNumbers`] for a count no file of `len` bytes can hold.
pub(crate) fn check_count(count: u64, len: usize) -> Result<()> {
    if count > (len as u64).max(MAX_EMPTY_VALUES) {
        Err(DbxError::TooManyNumbers { count })
    } else {
        Ok(())
    }
}

/// A bool / byte value's text: `false`, `true`, else the second byte as a number.
pub(crate) fn scalar_text(second: u8) -> String {
    match second {
        0 => "false".to_owned(),
        1 => "true".to_owned(),
        v => v.to_string(),
    }
}

/// A numbers element's `(size, values)`: the size and each value's raw bytes, as `dbx.numbers` (a value cut short
/// by the end of the raw bytes is shorter).
///
/// # Errors
/// A raw value whose count or size cannot be read.
pub fn numbers(raw: &[u8]) -> Result<(u64, Vec<&[u8]>)> {
    let mut c = Cursor::new(raw, 0);
    let count = c.varint()?;
    let size = c.varint()?;
    let start = c.pos();
    let sz = usize::try_from(size).unwrap_or(usize::MAX);
    check_count(count, raw.len())?;
    let out = (0..count as usize)
        .map(|i| c.clamped(i.saturating_mul(sz).saturating_add(start), sz))
        .collect();
    Ok((size, out))
}

/// A non-container element's value as the XML shows it (unescaped), as `dbx.value`.
///
/// # Errors
/// A numbers value that cannot be read, or a two-byte value with other than two bytes.
pub fn value(file: &DbxFile, e: &Element) -> Result<String> {
    match (&e.body, e.kind) {
        (Body::Text(t), TEXT) => Ok(file.str(*t).to_owned()),
        (Body::Numbers(raw), NUMBERS) => {
            let (size, vals) = numbers(raw)?;
            let first = e.attributes.first().map_or("", |(_, v)| file.str(*v));
            let texts = vals
                .into_iter()
                .map(|v| number_text(v, size, first))
                .collect::<Result<Vec<_>>>()?;
            Ok(texts.join("/"))
        }
        (Body::Scalar([_, b]), _) => Ok(scalar_text(*b)),
        (body, _) => {
            let len = match body {
                Body::Numbers(r) => r.len(),
                _ => 0,
            };
            Err(DbxError::ValueSize { len })
        }
    }
}

/// [`value`] of a 4-byte numbers element read as singles (e.g. a transform), as `dbx.floats`; `None` when the
/// element is not 4-byte numbers.
#[must_use]
pub fn floats(e: &Element) -> Option<Vec<f32>> {
    let Body::Numbers(raw) = &e.body else { return None };
    let (size, vals) = numbers(raw).ok()?;
    if size != 4 {
        return None;
    }
    vals.into_iter()
        .map(|v| <[u8; 4]>::try_from(v).ok().map(f32::from_be_bytes))
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp, clippy::many_single_char_names)]
mod tests {
    use super::*;

    #[test]
    fn g_formatting() {
        assert_eq!(net_g(1.0, 7), "1");
        assert_eq!(net_g(-0.0, 7), "0");
        assert_eq!(net_g(100.0, 7), "100");
        assert_eq!(net_g(1_234_567.0, 7), "1234567");
        assert_eq!(net_g(12_345_678.0, 7), "1.234568E+07");
        assert_eq!(net_g(0.0001, 7), "0.0001");
        assert_eq!(net_g(0.000_012_5, 7), "1.25E-05");
        assert_eq!(net_g(9.999_999_9, 7), "10");
        assert_eq!(net_g(999_999.95, 7), "999999.9"); // 999999.94999... in binary
        assert_eq!(net_g(9_999_999.5, 7), "1E+07");
        assert_eq!(net_g(0.5, 1), "0.5");
        assert_eq!(net_g(2.5, 1), "3"); // a tie away from zero, not to even
        assert_eq!(net_g(-2.5, 1), "-3");
        assert_eq!(net_g(1e300, 15), "1E+300");
        assert_eq!(net_g(5e-324, 17), "4.9406564584124654E-324");
        assert_eq!(net_g(f64::NAN, 7), "0");
        assert_eq!(net_g(1.0, 0), "0");
    }

    #[test]
    fn ties_and_near_ties() {
        // exact ties round away from zero
        assert_eq!(net_g(0.125, 2), "0.13");
        assert_eq!(net_g(25.0, 1), "3E+01");
        assert_eq!(net_g(2.5e15, 1), "3E+15");
        // a 5 that is not a tie (the exact value just below or above) rounds to nearest
        assert_eq!(net_g(0.15, 1), "0.1"); // 0.1499999999999999944...
        assert_eq!(net_g(0.35, 1), "0.3"); // 0.34999999999999997779...
        assert_eq!(net_g(0.45, 1), "0.5"); // 0.45000000000000001110...
        assert_eq!(net_g(2.5e15 + 1.0, 1), "3E+15");
        assert_eq!(net_g(9.5, 1), "1E+01");
        assert_eq!(net_g(99.5, 2), "1E+02");
        assert!(!is_tie(1.0, 0, 7));
    }

    #[test]
    fn singles_and_doubles() {
        assert_eq!(net_r_single(0.1f32.to_bits()), "0.1");
        assert_eq!(net_r_single(16_777_217f32.to_bits()), "16777216");
        assert_eq!(net_r_single(0.333_333_34f32.to_bits()), "0.333333343");
        assert_eq!(net_r_single(1.000_000_1f32.to_bits()), "1.00000012");
        assert_eq!(net_r_double(0.1f64.to_bits()), "0.1");
        assert_eq!(net_r_double((1.0f64 / 3.0).to_bits()), "0.33333333333333331");
        assert_eq!(net_r_double(f64::NAN.to_bits()), "NaN");
        assert_eq!(net_r_double(f64::INFINITY.to_bits()), "Infinity");
        assert_eq!(net_r_double(f64::NEG_INFINITY.to_bits()), "-Infinity");
        assert_eq!(net_r_double((-0.0f64).to_bits()), "0");
    }

    #[test]
    fn number_texts() {
        // top byte zero: a whole number (a denormal single prints as its bits; memory: the denormal trap)
        assert_eq!(number_text(&[0, 8, 0, 0], 4, "").unwrap(), "524288");
        // infinity / NaN exponents print as whole numbers
        assert_eq!(number_text(&f32::INFINITY.to_be_bytes(), 4, "").unwrap(), "2139095040");
        assert_eq!(
            number_text(&f32::NEG_INFINITY.to_be_bytes(), 4, "").unwrap(),
            "-8388608"
        );
        assert_eq!(number_text(&(-0.0f32).to_be_bytes(), 4, "").unwrap(), "0");
        assert_eq!(number_text(&(-2.5f32).to_be_bytes(), 4, "").unwrap(), "-2.5");
        assert_eq!(number_text(&[1, 2, 3], 16, "").unwrap(), "258");
        assert_eq!(number_text(&[7], 1, "").unwrap(), "7");
        assert_eq!(number_text(&[], 1, "").unwrap(), "0");
        assert_eq!(number_text(&0.5f64.to_be_bytes(), 8, "").unwrap(), "0.5");
        assert_eq!(
            number_text(&[1, 2], 4, ""),
            Err(DbxError::NumberCutShort { size: 4, len: 2 })
        );
        assert_eq!(
            number_text(&[1, 2], 8, ""),
            Err(DbxError::NumberCutShort { size: 8, len: 2 })
        );
    }

    #[test]
    fn values_and_floats() {
        let mut f = DbxFile::default();
        let mut vals = 1.5f32.to_be_bytes().to_vec();
        vals.extend((-2f32).to_be_bytes());
        let n = Element::numbers(&mut f, "field", &[("name", "T")], 4, &vals);
        assert_eq!(value(&f, &n).unwrap(), "1.5/-2");
        assert_eq!(floats(&n).unwrap(), [1.5, -2.0]);
        let id = Element::numbers(&mut f, "field", &[("name", "Id")], 4, &1.5f32.to_be_bytes());
        assert_eq!(value(&f, &id).unwrap(), "1069547520");
        let t = Element::text(&mut f, "field", &[], "hello");
        assert_eq!(value(&f, &t).unwrap(), "hello");
        assert!(floats(&t).is_none());
        let b = Element::scalar(&mut f, "field", 6, &[], [0, 0]);
        assert_eq!(value(&f, &b).unwrap(), "false");
        let b = Element::scalar(&mut f, "field", 6, &[], [0, 9]);
        assert_eq!(value(&f, &b).unwrap(), "9");
        let shorts = Element::numbers(&mut f, "field", &[], 2, &[0, 1, 0, 2]);
        assert_eq!(value(&f, &shorts).unwrap(), "1/2");
        assert!(floats(&shorts).is_none());
        let c = Element::container(&mut f, "c", &[], vec![]);
        assert_eq!(value(&f, &c), Err(DbxError::ValueSize { len: 0 }));
    }

    #[test]
    fn numbers_cut_short() {
        // 3 values of 4 bytes, only 6 bytes there
        let raw = [3, 4, 0x3F, 0x80, 0, 0, 0x40, 0];
        let (size, vals) = numbers(&raw).unwrap();
        assert_eq!(size, 4);
        assert_eq!(vals, [&[0x3F, 0x80, 0, 0][..], &[0x40, 0][..], &[][..]]);
        assert!(numbers(&[0xFF, 0xFF, 0xFF, 0x7F, 4]).is_err());
        let (_, zero) = numbers(&[3, 0]).unwrap();
        assert_eq!(zero.len(), 3);
    }
}
