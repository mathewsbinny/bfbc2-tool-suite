//! Python's text forms of values, as the tools' f-strings print them (round O2): `repr()` of a float and of a
//! str, `format(x, 'g')`, `round(x, n)`.

use std::fmt::Write as _;

use crate::unprintable::UNPRINTABLE;

/// The shortest round-trip digits of a finite, non-zero `x` and its decimal point position: `x = 0.d1d2... *
/// 10^decpt` (what Python's `repr` builds on: David Gay's shortest mode, as Rust's `{:e}`).
fn shortest(x: f64) -> (String, i32) {
    let s = format!("{:e}", x.abs());
    let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let digits: String = mant.chars().filter(char::is_ascii_digit).collect();
    let e: i32 = exp.parse().unwrap_or(0);
    (digits, e + 1)
}

/// `repr(x)` (and `str(x)`) of a Python float: `1.0`, `0.1`, `1e-05`, `1e+16`, `inf`, `nan`, `-0.0`.
pub fn float_repr(x: f64) -> String {
    if x.is_nan() {
        return "nan".to_owned();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf" } else { "-inf" }.to_owned();
    }
    let sign = if x.is_sign_negative() { "-" } else { "" };
    if x == 0.0 {
        return format!("{sign}0.0");
    }
    let (digits, decpt) = shortest(x);
    let n = i32::try_from(digits.len()).unwrap_or(i32::MAX);
    if decpt > -4 && decpt <= 16 {
        // fixed: digits with the point at decpt, zeros added either side, ".0" when integral
        let body = if decpt <= 0 {
            format!("0.{}{digits}", "0".repeat(usize::try_from(-decpt).unwrap_or(0)))
        } else if decpt >= n {
            format!("{digits}{}.0", "0".repeat(usize::try_from(decpt - n).unwrap_or(0)))
        } else {
            let (a, b) = digits.split_at(usize::try_from(decpt).unwrap_or(0));
            format!("{a}.{b}")
        };
        format!("{sign}{body}")
    } else {
        let e = decpt - 1;
        let mant = if digits.len() == 1 {
            digits
        } else {
            format!("{}.{}", &digits[..1], &digits[1..])
        };
        format!("{sign}{mant}e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs())
    }
}

/// `format(x, 'g')` (`f'{x:g}'`): six significant digits, trailing zeros taken off, an exponent below 1e-4 or
/// from 1e6 on.
pub fn float_g(x: f64) -> String {
    float_g_prec(x, 6)
}

/// `format(x, '.{p}g')`.
pub fn float_g_prec(x: f64, prec: usize) -> String {
    if x.is_nan() {
        return "nan".to_owned();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf" } else { "-inf" }.to_owned();
    }
    let prec = prec.max(1);
    let sign = if x.is_sign_negative() { "-" } else { "" };
    if x == 0.0 {
        return format!("{sign}0");
    }
    // the exponent as %e with prec - 1 decimals gives it (rounding may carry into a new digit)
    let sci = format!("{:.*e}", prec - 1, x.abs());
    let (mant, exp_text) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exp: i64 = exp_text.parse().unwrap_or(0);
    let digits = i64::try_from(prec).unwrap_or(i64::MAX);
    if -4 <= exp && exp < digits {
        let decimals = usize::try_from(digits - 1 - exp).unwrap_or(0);
        let fixed = format!("{:.*}", decimals, x.abs());
        let fixed = if fixed.contains('.') {
            fixed.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            fixed
        };
        format!("{sign}{fixed}")
    } else {
        let mant = if mant.contains('.') {
            mant.trim_end_matches('0').trim_end_matches('.')
        } else {
            mant
        };
        format!("{sign}{mant}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs())
    }
}

/// `round(x, n)` of a Python float (correctly rounded, ties to even on the exact binary value).
pub fn round_to(x: f64, n: usize) -> f64 {
    if !x.is_finite() {
        return x;
    }
    format!("{x:.n$}").parse().unwrap_or(x)
}

/// Whether Python's `str.isprintable()` takes the character (`repr` keeps it as it is).
pub fn printable(c: char) -> bool {
    let u = c as u32;
    if (0x20..0x7F).contains(&u) {
        return true;
    }
    UNPRINTABLE
        .binary_search_by(|&(a, b)| {
            if b < u {
                std::cmp::Ordering::Less
            } else if a > u {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_err()
}

/// `repr(s)` of a Python str: single quotes unless the text has a `'` and no `"`; `\\`, `\n`, `\r`, `\t` and the
/// quote escaped, other unprintable characters as `\xNN`, `\uNNNN`, `\UNNNNNNNN`.
pub fn str_repr(s: &str) -> String {
    let q = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(q);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == q => {
                out.push('\\');
                out.push(c);
            }
            c if printable(c) => out.push(c),
            c => {
                let u = c as u32;
                let _ = if u <= 0xFF {
                    write!(out, "\\x{u:02x}")
                } else if u <= 0xFFFF {
                    write!(out, "\\u{u:04x}")
                } else {
                    write!(out, "\\U{u:08x}")
                };
            }
        }
    }
    out.push(q);
    out
}

/// A module's `__doc__` from its source (`include_str!` of the `.py`): the first triple-quoted string, its escapes
/// read as Python reads them unless it is a raw (`r"""`) string; `\r\n` line ends as `\n`.
pub fn docstring(src: &str) -> String {
    let src = src.replace("\r\n", "\n");
    let Some(start) = src.find("\"\"\"") else {
        return String::new();
    };
    let raw = src[..start].ends_with('r') || src[..start].ends_with('R');
    let body = &src[start + 3..];
    let end = body.find("\"\"\"").unwrap_or(body.len());
    let body = &body[..end];
    if raw {
        return body.to_owned();
    }
    let mut out = String::with_capacity(body.len());
    let mut it = body.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.peek().copied() {
            Some('\\') => out.push('\\'),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\'') => out.push('\''),
            Some('\n') => {}
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => {
                out.push('\\');
                continue;
            }
        }
        it.next();
    }
    out
}

/// `f'{n:,}'` of an integer: thousands separated by commas.
pub fn thousands(n: i128) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_as_python_prints_them() {
        for (x, want) in [
            (1.0, "1.0"),
            (0.1, "0.1"),
            (1e-5, "1e-05"),
            (0.0001, "0.0001"),
            (1e16, "1e+16"),
            (1e15, "1000000000000000.0"),
            (123_456.789, "123456.789"),
            (-0.0, "-0.0"),
            (f64::from(0.1_f32), "0.10000000149011612"),
            (1.5e300, "1.5e+300"),
            (f64::INFINITY, "inf"),
            (2.5e-7, "2.5e-07"),
        ] {
            assert_eq!(float_repr(x), want, "{x}");
        }
        for (x, want) in [
            (1.0, "1"),
            (0.5, "0.5"),
            (1e-5, "1e-05"),
            (123_456.0, "123456"),
            (1_234_567.0, "1.23457e+06"),
            (0.000_123_456_7, "0.000123457"),
            (100.0, "100"),
            (-2.5, "-2.5"),
            (999_999.5, "1e+06"),
        ] {
            assert_eq!(float_g(x), want, "{x}");
        }
        assert!((round_to(2.675, 2) - 2.67).abs() < 1e-12); // 2.675 is below the tie in binary
        assert!((round_to(0.125, 2) - 0.12).abs() < 1e-12); // an exact tie: to even
    }

    #[test]
    fn strings_as_python_reprs_them() {
        assert_eq!(str_repr("a'b"), "\"a'b\"");
        assert_eq!(str_repr("a'b\""), "'a\\'b\"'");
        assert_eq!(str_repr("x\ny\t\\"), "'x\\ny\\t\\\\'");
        assert_eq!(
            str_repr("\u{7f}\u{a0}\u{e9}\u{416}\u{ad}\u{2028}\u{1F600}"),
            "'\\x7f\\xa0\u{e9}\u{416}\\xad\\u2028\u{1F600}'"
        );
        assert_eq!(str_repr("\u{e000}\u{10ffff}"), "'\\ue000\\U0010ffff'");
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(thousands(999), "999");
    }
}
