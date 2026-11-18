//! The map and bot test harness and the crash-reader tools (`bc2_harness`, round E4f), for
//! `bfbc2-automated-map-and-bot-test-harness`: opfor.log sessions, the crash log window, the exe handle (exeref's and
//! the crash reader's searches and disassembly with capstone's text), the pair bits and the cell index.
//!
//! A refusal (an input the port leaves to the Python reference: a Unicode digit on a session line, an instruction
//! outside the capstone parity proof, an address the reference indexes otherwise) is `None` from the functions and
//! `NotImplementedError` from the exe handle's methods; the caller then runs the reference.

use bc2_harness::exe::{self, Exe, Fallback, Syms};
use bc2_harness::graph::{CellIndex, pair_bits};
use bc2_harness::oplog::{self, Session};
use bc2_harness::pe::Image;
use bc2_harness::x86::{Insn, Operand, reg_name};
use iced_x86::Register;
use pyo3::exceptions::{PyIndexError, PyNotImplementedError, PyOSError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

/// A session as the tuple `ingame.Session` is made from: (pid, start, level, states, events, offset loads, override,
/// pathfinders, crashed, exited, lines).
type PySession = (
    String,
    String,
    Option<String>,
    Vec<i64>,
    Vec<&'static str>,
    Vec<(i64, i64)>,
    Option<(i64, i64)>,
    Vec<i64>,
    bool,
    bool,
    i64,
);

fn tuple(s: Session) -> PySession {
    (
        s.pid,
        s.start,
        s.level,
        s.states,
        s.events,
        s.offset_loads,
        s.override_,
        s.pathfinders,
        s.crashed,
        s.exited,
        s.lines,
    )
}

/// `ingame.sessions(text)` as tuples, or `None` (refused: the reference reads the text).
#[pyfunction]
pub fn harness_sessions(py: Python<'_>, text: &str) -> Option<Vec<PySession>> {
    py.detach(|| oplog::sessions(text))
        .ok()
        .map(|v| v.into_iter().map(tuple).collect())
}

/// The sessions of an opfor.log file (`open(path, encoding='utf-8', errors='replace').read()`), or `None`
/// (refused). `OSError` when the file cannot be read.
#[pyfunction]
pub fn harness_sessions_file(py: Python<'_>, path: &str) -> PyResult<Option<Vec<PySession>>> {
    match py.detach(|| oplog::sessions_of_file(std::path::Path::new(path))) {
        Err(e) => Err(PyOSError::new_err(format!("{path}: {e}"))),
        Ok(r) => Ok(r.ok().map(|v| v.into_iter().map(tuple).collect())),
    }
}

/// `crashreader.log_before_crash(path, lines, when)` with `want` the report's time of day in seconds (None: every
/// crash line), or `None` (refused). A file that cannot be read: `[]`, as the reference.
#[pyfunction]
#[pyo3(signature = (path, lines, want=None))]
pub fn crash_log_before(py: Python<'_>, path: &str, lines: i64, want: Option<i64>) -> Option<Vec<String>> {
    py.detach(|| match std::fs::read(path) {
        Err(_) => Some(Vec::new()),
        Ok(data) => oplog::log_before_crash(&data, lines, want).ok(),
    })
}

fn i64s(b: &[u8]) -> PyResult<Vec<i64>> {
    if !b.len().is_multiple_of(8) {
        return Err(PyValueError::new_err("not an int64 array"));
    }
    Ok(b.as_chunks::<8>().0.iter().map(|c| i64::from_le_bytes(*c)).collect())
}

fn f64s(b: &[u8]) -> PyResult<Vec<f64>> {
    if !b.len().is_multiple_of(8) {
        return Err(PyValueError::new_err("not a float64 array"));
    }
    Ok(b.as_chunks::<8>().0.iter().map(|c| f64::from_le_bytes(*c)).collect())
}

/// `checks.reachability`'s pair bits: `start` / `nbr` the adjacency as little-endian int64 bytes, the sources' and
/// targets' nodes (None: no node near). `IndexError` for a node outside the graph.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn harness_pair_bits(
    py: Python<'_>,
    start: &Bound<'_, PyBytes>,
    nbr: &Bound<'_, PyBytes>,
    sources: Vec<Option<i64>>,
    targets: Vec<Option<i64>>,
) -> PyResult<String> {
    let (s, n) = (i64s(start.as_bytes())?, i64s(nbr.as_bytes())?);
    py.detach(|| pair_bits(&s, &n, &sources, &targets))
        .map_err(PyIndexError::new_err)
}

/// `classify.Ground.cells`' index of a level's walkable cells.
#[pyclass(frozen, module = "bc2_native")]
pub struct HarnessCells {
    index: CellIndex,
}

#[pymethods]
impl HarnessCells {
    /// The index of cells: `xz` the (n, 2) float64 centres, `y` the n float64 heights, as little-endian bytes.
    #[new]
    fn new(py: Python<'_>, xz: &Bound<'_, PyBytes>, y: &Bound<'_, PyBytes>) -> PyResult<Self> {
        let xz = f64s(xz.as_bytes())?;
        let y = f64s(y.as_bytes())?;
        if xz.len() != 2 * y.len() {
            return Err(PyValueError::new_err("xz and y differ in length"));
        }
        let pairs: Vec<[f64; 2]> = xz.as_chunks::<2>().0.to_vec();
        Ok(Self {
            index: py.detach(|| CellIndex::new(pairs, y, 4.0)),
        })
    }

    /// The heights (float64 bytes, cell order) of the cells within `r` across of (x, z), or `None` (a coordinate
    /// that is not finite: the reference decides).
    fn cells<'py>(&self, py: Python<'py>, x: f64, z: f64, r: f64) -> Option<Bound<'py, PyBytes>> {
        let v = self.index.cells(x, z, r)?;
        let bytes: Vec<u8> = v.iter().flat_map(|f| f.to_le_bytes()).collect();
        Some(PyBytes::new(py, &bytes))
    }
}

/// An operand as the tools read it: ('imm', value, '', '', 0) or ('mem', 0, base, index, disp).
type PyOperand = (&'static str, i64, String, String, i64);
/// An instruction: (address, size, mnemonic, `op_str`, operands).
type PyInsn = (u32, u32, String, String, Vec<PyOperand>);

fn name(r: Register) -> String {
    if r == Register::None {
        String::new()
    } else {
        reg_name(r)
    }
}

fn py_insn(i: Insn) -> PyInsn {
    let ops = i
        .operands
        .iter()
        .map(|op| match *op {
            Operand::Imm(v) => ("imm", v, String::new(), String::new(), 0),
            Operand::Mem { base, index, disp } => ("mem", 0, name(base), name(index), disp),
        })
        .collect();
    (i.address, i.size, i.mnemonic, i.op_str, ops)
}

fn fallback(f: Fallback) -> PyErr {
    PyNotImplementedError::new_err(f.to_string())
}

/// capstone's first instruction of `code` at `address` (`next(md.disasm(code, address), None)` with detail), or
/// `None` where capstone decodes nothing; `NotImplementedError` (strict) for an encoding outside the parity proof.
#[pyfunction]
#[pyo3(signature = (code, address, strict=true))]
pub fn harness_x86_decode(code: &[u8], address: u32, strict: bool) -> PyResult<Option<PyInsn>> {
    let d = bc2_harness::x86::Disassembler::new();
    let got = if strict {
        d.decode(code, address)
            .map_err(|u| PyNotImplementedError::new_err(u.to_string()))?
    } else {
        d.decode_unchecked(code, address)
    };
    Ok(got.map(py_insn))
}

/// The exe for the crash reader and exeref: the mapped image (pefile's rules) and the searches over it.
#[pyclass(frozen, module = "bc2_native")]
pub struct ExeHandle {
    exe: Exe,
}

#[pymethods]
impl ExeHandle {
    /// The exe of a file: `strict` refuses instructions outside the capstone parity proof (`NotImplementedError`,
    /// the caller decodes them with capstone). `NotImplementedError` for an image pefile maps by rules not
    /// reproduced here, `OSError` for a file that cannot be read.
    #[staticmethod]
    #[pyo3(signature = (path, strict=true))]
    fn open(py: Python<'_>, path: &str, strict: bool) -> PyResult<Self> {
        let data = py
            .detach(|| std::fs::read(path))
            .map_err(|e| PyOSError::new_err(format!("{path}: {e}")))?;
        let img = py
            .detach(|| Image::parse(&data))
            .map_err(|e| PyNotImplementedError::new_err(e.to_string()))?;
        Ok(Self {
            exe: Exe::new(img, strict),
        })
    }

    /// `OPTIONAL_HEADER.ImageBase`.
    #[getter]
    fn base(&self) -> u32 {
        self.exe.base()
    }

    /// The end of the mapped image (base + its length).
    #[getter]
    fn end(&self) -> i64 {
        self.exe.end()
    }

    /// The mapped image (pefile's `get_memory_mapped_image()`).
    fn data<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.exe.img.data)
    }

    /// The sections: (name, virtual address (absolute), virtual size, characteristics).
    fn sections(&self) -> Vec<(String, u32, u32, u32)> {
        let base = self.exe.base();
        self.exe
            .img
            .sections
            .iter()
            .map(|s| {
                (
                    s.name.clone(),
                    base.wrapping_add(s.virtual_address),
                    s.virtual_size,
                    s.characteristics,
                )
            })
            .collect()
    }

    /// The code ranges (absolute, end exclusive).
    fn code_ranges(&self) -> Vec<(u32, u32)> {
        self.exe.code_ranges().to_vec()
    }

    /// `function_start(a, limit)`.
    #[pyo3(signature = (a, limit=0x4000))]
    fn function_start(&self, a: i64, limit: i64) -> PyResult<Option<i64>> {
        self.exe.function_start(a, limit).map_err(fallback)
    }

    /// `disasm(start, stop)`.
    fn disasm(&self, py: Python<'_>, start: i64, stop: i64) -> PyResult<Vec<PyInsn>> {
        let v = py.detach(|| self.exe.disasm(start, stop)).map_err(fallback)?;
        Ok(v.into_iter().map(py_insn).collect())
    }

    /// `around(eip, after)`: (start, instructions, index or None).
    #[pyo3(signature = (eip, after=0x10))]
    fn around(&self, py: Python<'_>, eip: i64, after: i64) -> PyResult<(i64, Vec<PyInsn>, Option<usize>)> {
        let (s, v, at) = py.detach(|| self.exe.around(eip, after)).map_err(fallback)?;
        Ok((s, v.into_iter().map(py_insn).collect(), at))
    }

    /// `call_before(ret)`.
    fn call_before(&self, ret: i64) -> PyResult<Option<PyInsn>> {
        Ok(self.exe.call_before(ret).map_err(fallback)?.map(py_insn))
    }

    /// `name_of(a, syms, image)`.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "PyO3 extracts a dict argument as an owned map"
    )]
    fn name_of(&self, a: i64, syms: Syms) -> PyResult<(String, String)> {
        exe::name_of(a, &syms, Some(&self.exe)).map_err(fallback)
    }

    /// exeref's `cstr(va, limit)`.
    #[pyo3(signature = (va, limit=120))]
    fn cstr(&self, va: i64, limit: i64) -> Option<String> {
        self.exe.cstr(va, limit)
    }

    /// exeref's `find_strings(text, exact)` (`text` Latin-1 bytes).
    #[pyo3(signature = (text, exact=false))]
    fn find_strings(&self, py: Python<'_>, text: &[u8], exact: bool) -> Vec<(u32, String)> {
        py.detach(|| self.exe.find_strings(text, exact))
    }

    /// exeref's `on_boundary(va)`.
    fn on_boundary(&self, va: i64) -> PyResult<bool> {
        self.exe.on_boundary(va).map_err(fallback)
    }

    /// exeref's `refs(value, limit)`: (address, instruction, kind).
    #[pyo3(signature = (value, limit=200))]
    fn refs(&self, py: Python<'_>, value: u32, limit: usize) -> PyResult<Vec<(u32, PyInsn, &'static str)>> {
        let v = py.detach(|| self.exe.refs(value, limit)).map_err(fallback)?;
        Ok(v.into_iter().map(|(a, i, k)| (a, py_insn(i), k.name())).collect())
    }

    /// exeref's `data_refs(value, limit)`.
    #[pyo3(signature = (value, limit=10))]
    fn data_refs(&self, value: u32, limit: usize) -> Vec<u32> {
        self.exe.data_refs(value, limit)
    }

    /// exeref's `window(exe, va, before, after, syms)`.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "PyO3 extracts a dict argument as an owned map"
    )]
    fn window(&self, py: Python<'_>, va: i64, before: i64, after: i64, syms: Syms) -> PyResult<Vec<String>> {
        py.detach(|| self.exe.window(va, before, after, &syms))
            .map_err(fallback)
    }
}
