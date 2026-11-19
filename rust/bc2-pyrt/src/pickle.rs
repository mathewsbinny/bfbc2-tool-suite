//! Reading Python's pickles (protocols 2-5) as the suite's caches write them (round O3): `meshview_index.pkl`
//! (`meshview.Index`) and `dbx_catalogue.pkl` (`dbxcatalogue.load`). Values are kept as a tree ([`Value`]); class
//! instances (`dbxcatalogue.Catalogue`, `collections.Counter`, `bfbc2core.dbx.Element` ...) as [`Obj`]: their class,
//! constructor arguments, state and the items set on them, never imported or run. Anything this reader does not know
//! (an opcode, a cut file) is an error: the caller treats the cache as Python's `pickle.load` failing would (a miss).

use std::cell::RefCell;
use std::rc::Rc;

/// A pickled value.
#[derive(Debug, Clone)]
pub enum Value {
    None,
    Bool(bool),
    Int(i64),
    /// an int beyond 64 bits: its little-endian two's-complement bytes
    Long(Rc<[u8]>),
    Float(f64),
    Str(Rc<str>),
    Bytes(Rc<[u8]>),
    Tuple(Rc<Vec<Value>>),
    List(Rc<RefCell<Vec<Value>>>),
    Dict(Rc<RefCell<Vec<(Value, Value)>>>),
    Set(Rc<RefCell<Vec<Value>>>),
    FrozenSet(Rc<Vec<Value>>),
    /// a class or function named by module and qualified name
    Global(Rc<(String, String)>),
    Obj(Rc<RefCell<Obj>>),
}

/// An instance made by calling a class (`REDUCE`, `NEWOBJ`), with what was set on it afterwards.
#[derive(Debug, Clone)]
pub struct Obj {
    pub class: Value,
    pub args: Value,
    pub state: Option<Value>,
    /// `SETITEM(S)` on it (a dict subclass: `Counter`, `defaultdict`)
    pub items: Vec<(Value, Value)>,
    /// `APPEND(S)` on it (a list subclass)
    pub list: Vec<Value>,
}

impl Value {
    /// `str` of a `Str`.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            Self::Bool(b) => Some(i64::from(*b)),
            _ => None,
        }
    }

    /// `len()` of a container (an object: its dict items, else its list items); None for other values.
    pub fn len(&self) -> Option<usize> {
        Some(match self {
            Self::Str(s) => s.chars().count(),
            Self::Bytes(b) => b.len(),
            Self::Tuple(t) => t.len(),
            Self::List(l) => l.borrow().len(),
            Self::Dict(d) => d.borrow().len(),
            Self::Set(s) => s.borrow().len(),
            Self::FrozenSet(s) => s.len(),
            Self::Obj(o) => {
                let o = o.borrow();
                o.items.len().max(o.list.len())
            }
            _ => return None,
        })
    }

    /// `d[key]` of a dict (or a dict-like object, or an object's state dict) for a str key.
    pub fn get(&self, key: &str) -> Option<Value> {
        match self {
            Self::Dict(d) => d
                .borrow()
                .iter()
                .find(|(k, _)| k.as_str() == Some(key))
                .map(|(_, v)| v.clone()),
            Self::Obj(o) => {
                let o = o.borrow();
                if let Some((_, v)) = o.items.iter().find(|(k, _)| k.as_str() == Some(key)) {
                    return Some(v.clone());
                }
                o.state.as_ref().and_then(|s| s.get(key))
            }
            _ => None,
        }
    }

    /// The items of a dict (or a dict-like object), in their order.
    pub fn items(&self) -> Vec<(Value, Value)> {
        match self {
            Self::Dict(d) => d.borrow().clone(),
            Self::Obj(o) => o.borrow().items.clone(),
            _ => Vec::new(),
        }
    }

    /// The items of a list, tuple, set or frozenset.
    pub fn elements(&self) -> Vec<Value> {
        match self {
            Self::Tuple(t) | Self::FrozenSet(t) => t.as_ref().clone(),
            Self::List(l) | Self::Set(l) => l.borrow().clone(),
            _ => Vec::new(),
        }
    }

    /// `not v` of a container (see [`Value::len`]).
    pub fn is_empty(&self) -> bool {
        self.len() == Some(0)
    }

    /// Python's `==` for the plain values (numbers, strings, bytes, tuples and lists of them); objects and mappings
    /// compare unequal.
    #[allow(
        clippy::cast_precision_loss,
        reason = "Python compares an int with a float as this float"
    )]
    pub fn py_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::None, Self::None) => true,
            (Self::Str(a), Self::Str(b)) => a == b,
            (Self::Bytes(a), Self::Bytes(b)) | (Self::Long(a), Self::Long(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => a == b,
            (a @ (Self::Int(_) | Self::Bool(_)), b @ (Self::Int(_) | Self::Bool(_))) => a.as_int() == b.as_int(),
            (Self::Int(i), Self::Float(f)) | (Self::Float(f), Self::Int(i)) => (*i as f64) == *f,
            (Self::Tuple(a), Self::Tuple(b)) => a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.py_eq(y)),
            (Self::List(a), Self::List(b)) => {
                let (a, b) = (a.borrow(), b.borrow());
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.py_eq(y))
            }
            _ => false,
        }
    }
}

/// Why a pickle could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickleError(pub String);

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], PickleError> {
        let end = self
            .at
            .checked_add(n)
            .filter(|&e| e <= self.b.len())
            .ok_or_else(|| PickleError("cut short".into()))?;
        let s = self
            .b
            .get(self.at..end)
            .ok_or_else(|| PickleError("cut short".into()))?;
        self.at = end;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u8, PickleError> {
        Ok(self.take(1)?.first().copied().unwrap_or(0))
    }

    fn uint(&mut self, n: usize) -> Result<u64, PickleError> {
        let s = self.take(n)?;
        Ok(s.iter().rev().fold(0u64, |v, &x| (v << 8) | u64::from(x)))
    }

    fn len(&mut self, n: usize) -> Result<usize, PickleError> {
        usize::try_from(self.uint(n)?).map_err(|_| PickleError("length too large".into()))
    }

    fn line(&mut self) -> Result<String, PickleError> {
        let rest = self.b.get(self.at..).unwrap_or(&[]);
        let i = rest
            .iter()
            .position(|&c| c == b'\n')
            .ok_or_else(|| PickleError("cut short".into()))?;
        let s = String::from_utf8_lossy(rest.get(..i).unwrap_or(&[])).into_owned();
        self.at += i + 1;
        Ok(s)
    }
}

fn text(b: &[u8]) -> Result<Value, PickleError> {
    std::str::from_utf8(b)
        .map(|s| Value::Str(Rc::from(s)))
        .map_err(|_| PickleError("a string that is not UTF-8".into()))
}

fn int_le(b: &[u8]) -> Value {
    if b.is_empty() {
        return Value::Int(0);
    }
    if b.len() <= 8 {
        let neg = b.last().is_some_and(|&x| x & 0x80 != 0);
        let mut v: u64 = if neg { u64::MAX } else { 0 };
        for (i, &x) in b.iter().enumerate() {
            v &= !(0xFFu64 << (8 * i));
            v |= u64::from(x) << (8 * i);
        }
        return Value::Int(v as i64);
    }
    Value::Long(Rc::from(b))
}

/// `pickle.loads(data)`.
#[allow(clippy::too_many_lines, reason = "the opcode table, one arm each")]
pub fn loads(data: &[u8]) -> Result<Value, PickleError> {
    let mut r = Reader { b: data, at: 0 };
    let mut stack: Vec<Value> = Vec::new();
    let mut marks: Vec<usize> = Vec::new();
    let mut memo: Vec<Option<Value>> = Vec::new();
    let err = |s: &str| PickleError(s.to_owned());
    let pop = |stack: &mut Vec<Value>| stack.pop().ok_or_else(|| PickleError("stack underflow".into()));
    let pop_mark = |stack: &mut Vec<Value>, marks: &mut Vec<usize>| -> Result<Vec<Value>, PickleError> {
        let m = marks.pop().ok_or_else(|| PickleError("no mark".into()))?;
        if m > stack.len() {
            return Err(PickleError("bad mark".into()));
        }
        Ok(stack.split_off(m))
    };
    let put = |memo: &mut Vec<Option<Value>>, i: usize, v: Value| {
        if memo.len() <= i {
            memo.resize(i + 1, None);
        }
        if let Some(slot) = memo.get_mut(i) {
            *slot = Some(v);
        }
    };
    loop {
        let op = r.u8()?;
        match op {
            0x80 => {
                r.u8()?; // PROTO
            }
            0x95 => {
                r.uint(8)?; // FRAME
            }
            b'.' => return pop(&mut stack),
            b'(' => marks.push(stack.len()),
            b'}' => stack.push(Value::Dict(Rc::default())),
            b']' => stack.push(Value::List(Rc::default())),
            b')' => stack.push(Value::Tuple(Rc::default())),
            0x8f => stack.push(Value::Set(Rc::default())),
            b'N' => stack.push(Value::None),
            0x88 => stack.push(Value::Bool(true)),
            0x89 => stack.push(Value::Bool(false)),
            b'J' => {
                let v = r.uint(4)? as u32 as i32;
                stack.push(Value::Int(i64::from(v)));
            }
            b'K' => stack.push(Value::Int(i64::from(r.u8()?))),
            b'M' => stack.push(Value::Int(r.uint(2)? as i64)),
            0x8a => {
                let n = usize::from(r.u8()?);
                stack.push(int_le(r.take(n)?));
            }
            0x8b => {
                let n = r.len(4)?;
                stack.push(int_le(r.take(n)?));
            }
            b'G' => {
                let s = r.take(8)?;
                let mut a = [0u8; 8];
                a.copy_from_slice(s);
                stack.push(Value::Float(f64::from_be_bytes(a)));
            }
            0x8c => {
                let n = usize::from(r.u8()?);
                stack.push(text(r.take(n)?)?);
            }
            b'X' => {
                let n = r.len(4)?;
                stack.push(text(r.take(n)?)?);
            }
            0x8d => {
                let n = r.len(8)?;
                stack.push(text(r.take(n)?)?);
            }
            b'C' => {
                let n = usize::from(r.u8()?);
                stack.push(Value::Bytes(Rc::from(r.take(n)?)));
            }
            b'B' => {
                let n = r.len(4)?;
                stack.push(Value::Bytes(Rc::from(r.take(n)?)));
            }
            0x8e | 0x96 => {
                let n = r.len(8)?;
                stack.push(Value::Bytes(Rc::from(r.take(n)?)));
            }
            0x94 => {
                let v = stack.last().cloned().ok_or_else(|| err("memoize on an empty stack"))?;
                let i = memo.len();
                put(&mut memo, i, v);
            }
            b'q' | b'r' => {
                let i = if op == b'q' { usize::from(r.u8()?) } else { r.len(4)? };
                let v = stack.last().cloned().ok_or_else(|| err("put on an empty stack"))?;
                put(&mut memo, i, v);
            }
            b'h' | b'j' => {
                let i = if op == b'h' { usize::from(r.u8()?) } else { r.len(4)? };
                let v = memo.get(i).cloned().flatten().ok_or_else(|| err("memo key missing"))?;
                stack.push(v);
            }
            b't' => {
                let items = pop_mark(&mut stack, &mut marks)?;
                stack.push(Value::Tuple(Rc::new(items)));
            }
            0x85..=0x87 => {
                let n = usize::from(op - 0x84);
                if stack.len() < n {
                    return Err(err("stack underflow"));
                }
                let items = stack.split_off(stack.len() - n);
                stack.push(Value::Tuple(Rc::new(items)));
            }
            0x91 => {
                let items = pop_mark(&mut stack, &mut marks)?;
                stack.push(Value::FrozenSet(Rc::new(items)));
            }
            b'a' | b'e' => {
                let items = if op == b'a' {
                    vec![pop(&mut stack)?]
                } else {
                    pop_mark(&mut stack, &mut marks)?
                };
                match stack.last() {
                    Some(Value::List(l)) => l.borrow_mut().extend(items),
                    Some(Value::Obj(o)) => o.borrow_mut().list.extend(items),
                    _ => return Err(err("append to a non-list")),
                }
            }
            b's' | b'u' => {
                let flat = if op == b's' {
                    let v = pop(&mut stack)?;
                    let k = pop(&mut stack)?;
                    vec![k, v]
                } else {
                    pop_mark(&mut stack, &mut marks)?
                };
                if flat.len() % 2 != 0 {
                    return Err(err("odd setitems"));
                }
                let mut it = flat.into_iter();
                let mut pairs = Vec::new();
                while let (Some(k), Some(v)) = (it.next(), it.next()) {
                    pairs.push((k, v));
                }
                match stack.last() {
                    Some(Value::Dict(d)) => d.borrow_mut().extend(pairs),
                    Some(Value::Obj(o)) => o.borrow_mut().items.extend(pairs),
                    _ => return Err(err("setitem on a non-dict")),
                }
            }
            0x90 => {
                let items = pop_mark(&mut stack, &mut marks)?;
                match stack.last() {
                    Some(Value::Set(s)) => s.borrow_mut().extend(items),
                    _ => return Err(err("additems on a non-set")),
                }
            }
            b'c' => {
                let module = r.line()?;
                let name = r.line()?;
                stack.push(Value::Global(Rc::new((module, name))));
            }
            0x93 => {
                let name = pop(&mut stack)?;
                let module = pop(&mut stack)?;
                let (Some(m), Some(n)) = (module.as_str(), name.as_str()) else {
                    return Err(err("stack_global of non-strings"));
                };
                stack.push(Value::Global(Rc::new((m.to_owned(), n.to_owned()))));
            }
            b'R' | 0x81 => {
                let args = pop(&mut stack)?;
                let class = pop(&mut stack)?;
                stack.push(Value::Obj(Rc::new(RefCell::new(Obj {
                    class,
                    args,
                    state: None,
                    items: Vec::new(),
                    list: Vec::new(),
                }))));
            }
            0x92 => {
                let _kwargs = pop(&mut stack)?;
                let args = pop(&mut stack)?;
                let class = pop(&mut stack)?;
                stack.push(Value::Obj(Rc::new(RefCell::new(Obj {
                    class,
                    args,
                    state: None,
                    items: Vec::new(),
                    list: Vec::new(),
                }))));
            }
            b'b' => {
                let state = pop(&mut stack)?;
                match stack.last() {
                    Some(Value::Obj(o)) => o.borrow_mut().state = Some(state),
                    _ => return Err(err("build on a non-object")),
                }
            }
            b'0' => {
                pop(&mut stack)?;
            }
            b'1' => {
                pop_mark(&mut stack, &mut marks)?;
            }
            b'2' => {
                let v = stack.last().cloned().ok_or_else(|| err("dup on an empty stack"))?;
                stack.push(v);
            }
            _ => return Err(PickleError(format!("opcode 0x{op:02x} is not read here"))),
        }
    }
}
