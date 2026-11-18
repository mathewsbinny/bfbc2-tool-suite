//! Python's number rules the ports reproduce: `round(x, n)` (the correctly rounded decimal, ties to even, read back
//! as the nearest double), `round(x)` (ties to even), and a dict's insertion order ([`OrderedMap`]).

use std::collections::HashMap;
use std::hash::Hash;

/// Python's `round(x, n)` for a float: the decimal with `n` places nearest to the exact value of `x` (an exact tie
/// to the even last digit), as the nearest double. Infinities and NaN come back as they are.
#[must_use]
pub fn round_to(x: f64, n: usize) -> f64 {
    if !x.is_finite() {
        return x;
    }
    fixed(x, n).parse::<f64>().unwrap_or(x)
}

/// Python's `format(x, f'.{n}f')`: the exact value of `x` rounded to `n` decimals, an exact tie to the even last
/// digit (Rust's own formatting does not round every tie that way).
#[must_use]
#[allow(clippy::many_single_char_names)] // x, n, e, q, s, r: the arithmetic's usual names
pub fn fixed(x: f64, n: usize) -> String {
    if !x.is_finite() || x.abs() >= 9.0e15 || n > 18 {
        // no fraction to round (or not a number): Rust's formatting prints these exactly
        return format!("{x:.n$}");
    }
    let bits = x.to_bits();
    let neg = bits >> 63 == 1;
    let exp = ((bits >> 52) & 0x7FF) as i64;
    let frac = bits & ((1u64 << 52) - 1);
    let (mant, e) = if exp == 0 {
        (frac, -1074i64)
    } else {
        (frac | (1u64 << 52), exp - 1075)
    };
    // x = mant * 2^e; the scaled value mant * 10^n * 2^e, rounded half to even
    let num = u128::from(mant) * 10u128.pow(n as u32);
    let q = if e >= 0 {
        num << e.min(60)
    } else {
        let s = (-e) as u32;
        if s >= 128 {
            0
        } else {
            let q = num >> s;
            let r = num - (q << s);
            let half = 1u128 << (s - 1);
            if r > half || (r == half && q % 2 == 1) {
                q + 1
            } else {
                q
            }
        }
    };
    let digits = q.to_string();
    let body = if n == 0 {
        digits
    } else {
        let padded = format!("{digits:0>width$}", width = n + 1);
        let (int, dec) = padded.split_at(padded.len() - n);
        format!("{int}.{dec}")
    };
    if neg { format!("-{body}") } else { body }
}

/// Python's `round(x)` of a float: the nearest integer, ties to even (as a float; the caller converts).
#[must_use]
pub fn round_even(x: f64) -> f64 {
    x.round_ties_even()
}

/// Python's `sum(xs)` of floats (CPython 3.12 and later): the integer start 0 plus the first item, then Neumaier's
/// compensated sum of the rest, the compensation added at the end when it is non-zero and finite.
#[must_use]
pub fn fsum_py(xs: &[f64]) -> f64 {
    let Some((&first, rest)) = xs.split_first() else {
        return 0.0;
    };
    let mut f = 0.0 + first;
    let mut c = 0.0f64;
    for &x in rest {
        let t = f + x;
        if f.abs() >= x.abs() {
            c += (f - t) + x;
        } else {
            c += (x - t) + f;
        }
        f = t;
    }
    if c != 0.0 && c.is_finite() {
        f += c;
    }
    f
}

/// A map that keeps its keys' first insertion order, as a Python dict does (a value set again keeps its place; a
/// key removed and set again goes to the end).
#[derive(Debug, Clone)]
pub struct OrderedMap<K, V> {
    slots: Vec<Option<(K, V)>>,
    index: HashMap<K, usize>,
}

impl<K: Hash + Eq + Clone, V> Default for OrderedMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Hash + Eq + Clone, V> OrderedMap<K, V> {
    /// An empty map.
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// `d[k] = v`.
    pub fn insert(&mut self, k: K, v: V) {
        if let Some(&i) = self.index.get(&k)
            && let Some(Some(slot)) = self.slots.get_mut(i)
        {
            slot.1 = v;
            return;
        }
        self.index.insert(k.clone(), self.slots.len());
        self.slots.push(Some((k, v)));
    }

    /// `d.get(k)`.
    pub fn get(&self, k: &K) -> Option<&V> {
        let i = *self.index.get(k)?;
        self.slots.get(i)?.as_ref().map(|(_, v)| v)
    }

    /// `d[k]` to change in place.
    pub fn get_mut(&mut self, k: &K) -> Option<&mut V> {
        let i = *self.index.get(k)?;
        self.slots.get_mut(i)?.as_mut().map(|(_, v)| v)
    }

    /// `d[k]` of a `defaultdict`: the value, a default one inserted at the end first when absent.
    pub fn entry_default(&mut self, k: K) -> &mut V
    where
        V: Default,
    {
        let i = if let Some(&i) = self.index.get(&k) {
            i
        } else {
            self.index.insert(k.clone(), self.slots.len());
            self.slots.push(None);
            self.slots.len() - 1
        };
        #[allow(clippy::indexing_slicing)] // a slot of this map: found in its index or just pushed
        let slot = &mut self.slots[i];
        &mut slot.get_or_insert_with(|| (k, V::default())).1
    }

    /// `k in d`.
    pub fn contains_key(&self, k: &K) -> bool {
        self.index.contains_key(k)
    }

    /// `del d[k]` (nothing when absent).
    pub fn remove(&mut self, k: &K) {
        if let Some(i) = self.index.remove(k)
            && let Some(slot) = self.slots.get_mut(i)
        {
            *slot = None;
        }
    }

    /// `d.items()` in order.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.slots.iter().filter_map(|s| s.as_ref().map(|(k, v)| (k, v)))
    }

    /// `d.keys()` in order.
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.iter().map(|(k, _)| k)
    }

    /// `len(d)`.
    #[must_use]
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// `not d`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_as_python() {
        // values Python 3.12 gives
        assert_eq!(round_to(0.25, 1), 0.2);
        assert_eq!(round_to(0.125, 2), 0.12);
        assert_eq!(round_to(0.375, 2), 0.38);
        assert_eq!(round_to(2.675, 2), 2.67);
        assert_eq!(round_to(0.03125, 4), 0.0312);
        assert_eq!(round_to(0.09375, 4), 0.0938);
        assert_eq!(round_to(200.000_007_629_394_53, 3), 200.0);
        assert_eq!(round_to(-0.0004, 3), -0.0);
        assert!(round_to(-0.0004, 3).is_sign_negative());
        assert_eq!(round_to(1.0005, 3), 1.0);
        assert_eq!(round_even(2.5), 2.0);
        assert_eq!(fixed(12.5, 0), "12");
        assert_eq!(fixed(13.5, 0), "14");
        assert_eq!(fixed(0.5, 0), "0");
        assert_eq!(fixed(-0.0004, 3), "-0.000");
        assert_eq!(fixed(2.675, 2), "2.67");
        assert_eq!(fixed(1e-300, 2), "0.00");
        assert_eq!(round_even(-1.5), -2.0);
    }

    #[test]
    fn keeps_insertion_order() {
        let mut m = OrderedMap::new();
        m.insert(3, 'a');
        m.insert(1, 'b');
        m.insert(3, 'c');
        m.remove(&1);
        m.insert(1, 'd');
        assert_eq!(
            m.iter().map(|(k, v)| (*k, *v)).collect::<Vec<_>>(),
            vec![(3, 'c'), (1, 'd')]
        );
    }
}
