//! Python's `repr(float)`, which the HLSL text writes its numbers with (`xenos_hlsl._f`): the shortest digits that
//! read back as the same double, in fixed notation from 1e-4 up to 1e16 and in exponent notation (`1e-05`,
//! `1.5e+16`) outside it, with a `.0` on whole numbers.

use std::fmt::Write as _;

fn split_sci(sci: &str) -> (String, i32) {
    let (mant, exp) = sci.split_once('e').unwrap_or((sci, "0"));
    (
        mant.chars().filter(char::is_ascii_digit).collect(),
        exp.parse().unwrap_or(0),
    )
}

/// The digits and decimal exponent of CPython's repr (David Gay's shortest mode): the fewest significant digits
/// that read back as `x`, and of those the nearest to `x`, a tie to the even digit. Rust's `{:e}` finds the length,
/// but breaks an exact tie upwards: an `f32` widened to `f64` often ends in such a tie at the 17th digit
/// (15.2146759033203125: Python 15.214675903320312, `{:e}` ...313). `{:.Ne}` rounds exactly, ties to even, so that
/// many digits are taken from it when they read back as `x`.
fn shortest_digits(x: f64) -> (String, i32) {
    let (short, exp) = split_sci(&format!("{x:e}"));
    let n = short.len().max(1);
    let exact = format!("{x:.prec$e}", prec = n - 1);
    if exact.parse::<f64>().ok() == Some(x) {
        return split_sci(&exact);
    }
    (short, exp)
}

/// `repr(x)` as CPython writes it (`float_repr_style == 'short'`).
#[must_use]
pub fn float_repr(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf".into() } else { "-inf".into() };
    }
    let (digits, exp) = shortest_digits(x.abs());
    let decpt = exp + 1;
    let mut out = String::with_capacity(digits.len() + 8);
    if x.is_sign_negative() {
        out.push('-');
    }
    let nd = digits.len() as i32;
    if decpt <= -4 || decpt > 16 {
        let (first, rest) = digits.split_at(1.min(digits.len()));
        out.push_str(first);
        if !rest.is_empty() {
            out.push('.');
            out.push_str(rest);
        }
        out.push('e');
        out.push(if exp < 0 { '-' } else { '+' });
        let _ = write!(out, "{:02}", exp.unsigned_abs());
    } else if decpt <= 0 {
        out.push_str("0.");
        out.extend(std::iter::repeat_n('0', decpt.unsigned_abs() as usize));
        out.push_str(&digits);
    } else if decpt < nd {
        let (a, b) = digits.split_at(decpt as usize);
        out.push_str(a);
        out.push('.');
        out.push_str(b);
    } else {
        out.push_str(&digits);
        out.extend(std::iter::repeat_n('0', (decpt - nd) as usize));
        out.push_str(".0");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::float_repr;

    #[test]
    #[allow(clippy::excessive_precision)] // the exact values of widened f32s: ties at the 17th digit
    fn python_reprs() {
        for (x, want) in [
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (1.0, "1.0"),
            (0.5, "0.5"),
            (-2.25, "-2.25"),
            (0.1, "0.1"),
            (1e16, "1e+16"),
            (1e15, "1000000000000000.0"),
            (1.5e16, "1.5e+16"),
            (123_456_789_012_345_680.0, "1.2345678901234568e+17"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (1.25e-7, "1.25e-07"),
            (5e-324, "5e-324"),
            (1.797_693_134_862_315_7e308, "1.7976931348623157e+308"),
            (f64::from(0.1f32), "0.10000000149011612"),
            (15.214_675_903_320_312_5, "15.214675903320312"),
            (607.946_350_097_656_25, "607.9463500976562"),
            (f64::from(3.402_823_5e38_f32), "3.4028234663852886e+38"),
            (100.0, "100.0"),
            (1234.5, "1234.5"),
            (0.001_234, "0.001234"),
        ] {
            assert_eq!(float_repr(x), want, "{x:e}");
        }
    }
}
