//! Writing Python's pickles as CPython 3.13's C pickler writes them (round O3: `meshview_index.pkl`, written by
//! `bfbc2.exe` when it builds the index; the reference reads it as its own).

use std::collections::HashMap;
use std::rc::Rc;

use crate::pickle::{PickleError, Value};

/// `pickle.dumps(v)` (protocol 4, 3.13's default): framed in 64 KiB frames; each `str`, `bytes`, non-empty tuple,
/// list and dict memoized on first sight and named again by its memo index (object identity: values sharing one
/// `Rc` are one object, as the reference's shared objects); dicts and lists in batches of 1,000. Values the suite's
/// caches do not hold (sets, objects, globals) are an error.
///
/// # Errors
/// A value of a kind this writer does not write.
pub fn dumps(v: &Value) -> Result<Vec<u8>, PickleError> {
    let mut w = Writer::default();
    w.out.extend_from_slice(&[0x80, 4]);
    w.framing = true;
    w.save(v)?;
    w.write(b".");
    w.commit_frame();
    Ok(w.out)
}

const FRAME_HEADER: usize = 9;
const FRAME_TARGET: usize = 64 * 1024;
const BATCH: usize = 1000;

#[derive(Default)]
struct Writer {
    out: Vec<u8>,
    framing: bool,
    frame_start: Option<usize>,
    memo: HashMap<usize, u32>,
}

impl Writer {
    /// `_Pickler_Write`: a new frame's (blank) header first when no frame is open.
    fn write(&mut self, b: &[u8]) {
        if self.framing && self.frame_start.is_none() {
            self.frame_start = Some(self.out.len());
            self.out.extend_from_slice(&[0xFE; FRAME_HEADER]);
        }
        self.out.extend_from_slice(b);
    }

    /// `_Pickler_CommitFrame`: the open frame's header filled in (a frame under 4 bytes is unframed).
    fn commit_frame(&mut self) {
        let Some(s) = self.frame_start.take() else { return };
        let len = self.out.len() - s - FRAME_HEADER;
        if len >= 4 {
            let mut head = [0x95u8; FRAME_HEADER];
            head[1..].copy_from_slice(&(len as u64).to_le_bytes());
            self.out.splice(s..s + FRAME_HEADER, head);
        } else {
            self.out.drain(s..s + FRAME_HEADER);
        }
    }

    /// `_Pickler_OpcodeBoundary`, before each object is saved: the frame closed once it holds 64 KiB (measured on
    /// 3.13: a frame ends before an object, never between a tuple's last item and its TUPLE2).
    fn boundary(&mut self) {
        if let Some(s) = self.frame_start
            && self.out.len() - s - FRAME_HEADER >= FRAME_TARGET
        {
            self.commit_frame();
        }
    }

    /// `BINGET` / `LONG_BINGET` of an object seen before; false when it is new.
    fn get(&mut self, id: usize) -> bool {
        let Some(&i) = self.memo.get(&id) else { return false };
        if i < 256 {
            self.write(&[b'h', i as u8]);
        } else {
            let mut b = vec![b'j'];
            b.extend_from_slice(&i.to_le_bytes());
            self.write(&b);
        }
        true
    }

    /// `memo_put`: MEMOIZE.
    fn put(&mut self, id: usize) {
        let n = self.memo.len() as u32;
        self.memo.insert(id, n);
        self.write(&[0x94]);
    }

    fn sized(&mut self, short: u8, long: u8, data: &[u8]) {
        let mut b = Vec::with_capacity(data.len() + 5);
        if data.len() < 256 {
            b.push(short);
            b.push(data.len() as u8);
        } else {
            b.push(long);
            b.extend_from_slice(&(data.len() as u32).to_le_bytes());
        }
        b.extend_from_slice(data);
        self.write(&b);
    }

    fn save(&mut self, v: &Value) -> Result<(), PickleError> {
        self.boundary();
        self.save_inner(v)
    }

    fn save_inner(&mut self, v: &Value) -> Result<(), PickleError> {
        match v {
            Value::None => self.write(b"N"),
            Value::Bool(b) => self.write(if *b { &[0x88] } else { &[0x89] }),
            Value::Int(i) => self.int(*i),
            Value::Float(f) => {
                // save_float: BINFLOAT, big-endian (round CT1: culltree's rows)
                let mut b = vec![b'G'];
                b.extend_from_slice(&f.to_be_bytes());
                self.write(&b);
            }
            Value::Long(b) => {
                let mut o = vec![0x8a, b.len() as u8];
                o.extend_from_slice(b);
                self.write(&o);
            }
            Value::Str(s) => {
                let id = Rc::as_ptr(s).cast::<u8>() as usize;
                if !self.get(id) {
                    self.sized(0x8c, b'X', s.as_bytes());
                    self.put(id);
                }
            }
            Value::Bytes(b) => {
                let id = Rc::as_ptr(b).cast::<u8>() as usize;
                if !self.get(id) {
                    self.sized(b'C', b'B', b);
                    self.put(id);
                }
            }
            Value::Tuple(t) => self.tuple(t)?,
            Value::List(l) => {
                let id = Rc::as_ptr(l) as usize;
                if self.get(id) {
                    return Ok(());
                }
                self.write(b"]");
                self.put(id);
                let items = l.borrow();
                if let [one] = items.as_slice() {
                    self.save(one)?;
                    self.write(b"a");
                } else if !items.is_empty() {
                    for chunk in items.chunks(BATCH) {
                        self.write(b"(");
                        for x in chunk {
                            self.save(x)?;
                        }
                        self.write(b"e");
                    }
                }
            }
            Value::Dict(d) => {
                let id = Rc::as_ptr(d) as usize;
                if self.get(id) {
                    return Ok(());
                }
                self.write(b"}");
                self.put(id);
                let items = d.borrow();
                if let [(k, x)] = items.as_slice() {
                    self.save(k)?;
                    self.save(x)?;
                    self.write(b"s");
                } else if !items.is_empty() {
                    // batch_dict_exact: a full last batch is followed by an empty one
                    let mut rest: &[(Value, Value)] = &items;
                    loop {
                        self.write(b"(");
                        let (now, later) = rest.split_at(rest.len().min(BATCH));
                        for (k, x) in now {
                            self.save(k)?;
                            self.save(x)?;
                        }
                        self.write(b"u");
                        rest = later;
                        if now.len() < BATCH {
                            break;
                        }
                    }
                }
            }
            _ => return Err(PickleError("a value this writer does not write".to_owned())),
        }
        Ok(())
    }

    /// `save_tuple`: `EMPTY_TUPLE`; `TUPLE1`-`3` after the items; else `MARK`, the items, `TUPLE`; memoized.
    fn tuple(&mut self, t: &Rc<Vec<Value>>) -> Result<(), PickleError> {
        if t.is_empty() {
            self.write(b")");
            return Ok(());
        }
        let id = Rc::as_ptr(t) as usize;
        if self.get(id) {
            return Ok(());
        }
        if t.len() > 3 {
            self.write(b"(");
        }
        for x in t.iter() {
            self.save(x)?;
        }
        self.write(match t.len() {
            1 => &[0x85],
            2 => &[0x86],
            3 => &[0x87],
            _ => b"t",
        });
        self.put(id);
        Ok(())
    }

    /// `save_long`: BININT1 / BININT2 / BININT within 32 bits, else LONG1.
    fn int(&mut self, i: i64) {
        if (0..256).contains(&i) {
            self.write(&[b'K', i as u8]);
        } else if (0..65536).contains(&i) {
            let mut b = vec![b'M'];
            b.extend_from_slice(&(i as u16).to_le_bytes());
            self.write(&b);
        } else if let Ok(v) = i32::try_from(i) {
            let mut b = vec![b'J'];
            b.extend_from_slice(&v.to_le_bytes());
            self.write(&b);
        } else {
            let mut b = vec![0x8a];
            let body = long_bytes(i128::from(i));
            b.push(body.len() as u8);
            b.extend_from_slice(&body);
            self.write(&b);
        }
    }
}

/// An int's LONG1 bytes: `_PyLong_AsByteArray` of `nbits // 8 + 1` bytes (little-endian two's complement), a
/// negative one's needless top 0xff dropped.
pub fn long_bytes(i: i128) -> Vec<u8> {
    let nbits = 128 - if i < 0 { (!i).leading_zeros() } else { i.leading_zeros() };
    let mut n = (nbits / 8 + 1) as usize;
    let all = i.to_le_bytes();
    if i < 0 && n > 1 && all.get(n - 1) == Some(&0xff) && all.get(n - 2).is_some_and(|b| b & 0x80 != 0) {
        n -= 1;
    }
    all.get(..n).unwrap_or(&all).to_vec()
}

/// An int as a pickle value: `Int` within 64 bits, else `Long`.
pub fn int(i: i128) -> Value {
    match i64::try_from(i) {
        Ok(v) => Value::Int(v),
        Err(_) => Value::Long(Rc::from(long_bytes(i))),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    fn s(x: &str) -> Value {
        Value::Str(Rc::from(x))
    }

    #[test]
    fn small_pickle_as_cpython() {
        // pickle.dumps({'a': [1, b'x', (1, 2)], 'b': {}}) on 3.13
        let v = Value::Dict(Rc::new(RefCell::new(vec![
            (
                s("a"),
                Value::List(Rc::new(RefCell::new(vec![
                    Value::Int(1),
                    Value::Bytes(Rc::from(&b"x"[..])),
                    Value::Tuple(Rc::new(vec![Value::Int(1), Value::Int(2)])),
                ]))),
            ),
            (s("b"), Value::Dict(Rc::new(RefCell::new(Vec::new())))),
        ])));
        let want = "8004951f000000000000007d94288c0161945d94284b01430178944b014b028694658c0162947d94752e";
        let want: Vec<u8> = (0..want.len())
            .step_by(2)
            .filter_map(|i| want.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok()))
            .collect();
        let got = dumps(&v).unwrap_or_default();
        assert_eq!(got, want);
    }

    #[test]
    fn longs_as_cpython() {
        assert_eq!(long_bytes(255), vec![0xff, 0]);
        assert_eq!(long_bytes(-1), vec![0xff]);
        assert_eq!(long_bytes(-128), vec![0x80]);
        assert_eq!(long_bytes(1 << 40), vec![0, 0, 0, 0, 0, 1]);
    }
}
