//! The bindings of the Xbox 360 shader translator (`bc2_shader`): `bfbc2-shader-tools/xenos.py`'s `parse_shader`
//! and `xenos_hlsl.py`'s `translate` call them when the module is built.
//!
//! `xenos_parse` decodes a blob and gives a [`XenosProgram`] (the decoded program, kept in Rust) with the tree
//! `parse_shader` builds without its microcode: dicts in `xenos.py`'s attribute order, every nested record made
//! through the caller's `Obj` class (`object.__new__` and its `__dict__`, no Python call per record).
//! `xenos_microcode` gives the rest (`cf`, `instructions` ...: `xenos.py` builds them only when a caller reads them).
//! `xenos_translate` translates a program with an `Interface` read from the Python object; `xenos_tree_program`
//! makes the program of a tree a caller handed over (the Python decoder's, a tree whose microcode was read, one built
//! by hand). The translation is Rust's alone: `xenos_hlsl.py`'s Python translator was retired on 2026-11-18.
//!
//! A blob the decoder refuses gives `None` (the Python decoder, the format reference, raises its own exception).
//! Status 1 is `TranslateError` with its message; status 2 a program or interface the translator does not take (the
//! reason: `xenos_hlsl` raises `ValueError` with it).
// the Python's names (vsize / psize, ctab_off / def_off ...), the tree's attributes one after another, and the
// batch functions' lists, which PyO3 extracts by value
#![allow(clippy::similar_names, clippy::too_many_lines, clippy::needless_pass_by_value)]

use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::sync::{Arc, Mutex, OnceLock};

use bc2_shader::hlsl::{self, Interface, TranslateError};
use bc2_shader::xenos::{
    self, Alu, Cf, CfKind, CfType, Constant, Ctab, Dest, Fetch, FetchKind, File, Header, Instr, InstrKind,
    Interpolator, Named, Operand, ParamType, Shader, Stage, VertexElement,
};
use pyo3::IntoPyObjectExt;
use pyo3::exceptions::{PyAttributeError, PyValueError};
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyBytesMethods, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};

/// A decoded Xenos program, kept in Rust for the translator: decoded here (`xenos_parse`), or a tree a caller handed
/// over (`xenos_tree_program`: only the instructions its CFs run, and `cube` - any vector `cube` among the tree's own
/// `instructions` - given).
#[pyclass(frozen, module = "bc2_native")]
pub struct XenosProgram {
    shader: Arc<Shader>,
    cube: Option<bool>,
}

#[pymethods]
impl XenosProgram {
    /// `is_pixel` as decoded.
    #[getter]
    fn is_pixel(&self) -> bool {
        self.shader.is_pixel
    }
}

/// Makes `Obj` records: `cls.__new__(cls)` with a filled `__dict__`.
struct Builder<'py> {
    py: Python<'py>,
    cls: Bound<'py, PyAny>,
    new: Bound<'py, PyAny>,
}

macro_rules! obj {
    ($b:expr, { $($k:literal => $v:expr),* $(,)? }) => {{
        let d = PyDict::new($b.py);
        $( d.set_item(intern!($b.py, $k), $v)?; )*
        $b.wrap(d)
    }};
}

impl<'py> Builder<'py> {
    fn new(cls: &Bound<'py, PyAny>) -> PyResult<Self> {
        Ok(Self {
            py: cls.py(),
            cls: cls.clone(),
            new: cls.getattr(intern!(cls.py(), "__new__"))?,
        })
    }

    fn wrap(&self, d: Bound<'py, PyDict>) -> PyResult<Bound<'py, PyAny>> {
        let o = self.new.call1((&self.cls,))?;
        o.setattr(intern!(self.py, "__dict__"), d)?;
        Ok(o)
    }

    fn named(&self, n: Named) -> PyResult<Bound<'py, PyAny>> {
        match n {
            Named::Name(s) => Ok(PyString::new(self.py, s).into_any()),
            Named::Number(v) => v.into_bound_py_any(self.py),
        }
    }

    fn list<T>(&self, items: &[T], f: impl Fn(&T) -> PyResult<Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyList>> {
        let l = PyList::empty(self.py);
        for x in items {
            l.append(f(x)?)?;
        }
        Ok(l)
    }

    fn ptype(&self, t: &ParamType) -> PyResult<Bound<'py, PyAny>> {
        let members = self.list(
            &t.members,
            |(name, mt)| obj!(self, { "name" => name, "type" => self.ptype(mt)? }),
        )?;
        obj!(self, {
            "cls" => self.named(t.cls)?, "type" => self.named(t.type_)?, "rows" => t.rows, "cols" => t.cols,
            "elements" => t.elements, "members" => members,
        })
    }

    fn constant(&self, c: &Constant) -> PyResult<Bound<'py, PyAny>> {
        obj!(self, {
            "name" => &c.name, "regset" => self.named(c.regset)?, "reg" => c.reg, "count" => c.count,
            "reserved" => c.reserved, "type" => self.ptype(&c.type_)?, "default_offset" => c.default_offset,
        })
    }

    fn header(&self, h: &Header) -> PyResult<Bound<'py, PyAny>> {
        let py = self.py;
        let pcf = PyDict::new(py);
        for (k, v) in h.program_cntl_fields() {
            pcf.set_item(k, v)?;
        }
        let cmf = PyDict::new(py);
        for (k, v) in h.context_misc_fields() {
            cmf.set_item(k, v)?;
        }
        let d = PyDict::new(py);
        d.set_item(intern!(py, "offset"), h.offset)?;
        d.set_item(intern!(py, "physical_offset"), h.physical_offset)?;
        d.set_item(intern!(py, "physical_size"), h.physical_size)?;
        d.set_item(intern!(py, "program_cntl"), h.program_cntl)?;
        d.set_item(intern!(py, "context_misc"), h.context_misc)?;
        d.set_item(intern!(py, "field10"), h.field10)?;
        d.set_item(intern!(py, "interpolator_info"), h.interpolator_info)?;
        d.set_item(intern!(py, "num_temp_regs"), h.num_temp_regs)?;
        d.set_item(intern!(py, "program_cntl_fields"), self.wrap(pcf)?)?;
        d.set_item(intern!(py, "context_misc_fields"), self.wrap(cmf)?)?;
        d.set_item(intern!(py, "interpolator_count"), h.interpolator_count)?;
        let ve = self.list(&h.vertex_elements, |v| {
            obj!(self, {
                "raw" => v.raw, "fetch_address" => v.fetch_address, "usage" => v.usage,
                "usage_index" => v.usage_index, "unknown_hi" => v.unknown_hi,
            })
        })?;
        d.set_item(intern!(py, "vertex_elements"), ve)?;
        let it = self.list(&h.interpolators, |i| {
            let o = PyDict::new(py);
            o.set_item(intern!(py, "raw"), i.raw)?;
            o.set_item(intern!(py, "usage_index"), i.usage_index)?;
            o.set_item(intern!(py, "usage"), i.usage)?;
            o.set_item(intern!(py, "reg"), i.reg)?;
            o.set_item(intern!(py, "mask"), i.mask)?;
            o.set_item(intern!(py, "export_index"), i.export_index)?;
            if let Some(a) = &i.export_addresses {
                o.set_item(intern!(py, "export_addresses"), PyList::new(py, a)?)?;
            }
            self.wrap(o)
        })?;
        d.set_item(intern!(py, "interpolators"), it)?;
        match &h.stage {
            Stage::Pixel { interp_mask, outputs } => {
                d.set_item(intern!(py, "interp_mask"), interp_mask)?;
                d.set_item(intern!(py, "outputs"), outputs)?;
            }
            Stage::Vertex {
                field18,
                vertex_element_count,
                field20,
                field18_words,
            } => {
                d.set_item(intern!(py, "field18"), field18)?;
                d.set_item(intern!(py, "vertex_element_count"), vertex_element_count)?;
                d.set_item(intern!(py, "field20"), field20)?;
                d.set_item(intern!(py, "field18_words"), PyList::new(py, field18_words)?)?;
            }
        }
        let es = self.list(
            &h.export_slots,
            |e| obj!(self, { "raw" => e.raw, "address" => e.address, "last" => e.last, "unknown_hi" => e.unknown_hi }),
        )?;
        d.set_item(intern!(py, "export_slots"), es)?;
        d.set_item(intern!(py, "end"), h.end)?;
        self.wrap(d)
    }

    /// `parse_shader`'s tree without the microcode's fields, as the dict of the shader object.
    fn shader(&self, sh: &Shader) -> PyResult<Bound<'py, PyDict>> {
        let py = self.py;
        let [vsize, psize, field_c, ctab_off, def_off, hdr_off, field_1c, field_20] = sh.container;
        let ctab = match &sh.ctab {
            Some(t) => {
                let cs = self.list(&t.constants, |c| self.constant(c))?;
                obj!(self, {
                    "offset" => t.offset, "size" => t.size, "base" => t.base, "creator" => &t.creator,
                    "version" => t.version, "target" => &t.target, "flags" => t.flags, "constants" => cs,
                })?
            }
            None => py.None().into_bound(py),
        };
        let lits = PyDict::new(py);
        for (r, v) in &sh.literals {
            lits.set_item(r, PyTuple::new(py, v.iter().map(|&x| f64::from(x)))?)?;
        }
        let ilits = PyDict::new(py);
        for (r, v) in &sh.int_literals {
            ilits.set_item(r, PyTuple::new(py, v)?)?;
        }
        let def_table = match &sh.def_table {
            Some(dt) => {
                let [f0, f4, f8, fc] = dt.fields;
                let fd = self.list(
                    &dt.float_defs,
                    |f| obj!(self, { "reg" => f.reg, "count" => f.count, "physical_offset" => f.physical_offset }),
                )?;
                let id = self.list(
                    &dt.int_defs,
                    |f| obj!(self, { "reg" => f.reg, "count" => f.count, "values" => PyList::new(py, &f.values)? }),
                )?;
                obj!(self, {
                    "offset" => dt.offset, "field0" => f0, "field4" => f4, "field8" => f8, "fieldC" => fc,
                    "size" => dt.size, "raw_words" => PyList::new(py, &dt.raw_words)?, "float_defs" => fd,
                    "int_defs" => id, "bool_words" => PyList::new(py, &dt.bool_words)?,
                })?
            }
            None => py.None().into_bound(py),
        };
        let gaps = PyList::empty(py);
        for g in &sh.layout_gaps {
            gaps.append(*g)?;
        }
        let d = PyDict::new(py);
        d.set_item(intern!(py, "kind"), intern!(py, "XenosShader"))?;
        d.set_item(intern!(py, "is_pixel"), sh.is_pixel)?;
        d.set_item(intern!(py, "magic"), sh.magic)?;
        d.set_item(intern!(py, "virtual_size"), vsize)?;
        d.set_item(intern!(py, "physical_size"), psize)?;
        d.set_item(intern!(py, "fieldC"), field_c)?;
        d.set_item(intern!(py, "ctab_offset"), ctab_off)?;
        d.set_item(intern!(py, "def_offset"), def_off)?;
        d.set_item(intern!(py, "header_offset"), hdr_off)?;
        d.set_item(intern!(py, "field1C"), field_1c)?;
        d.set_item(intern!(py, "field20"), field_20)?;
        d.set_item(intern!(py, "ctab"), ctab)?;
        d.set_item(intern!(py, "literals"), lits)?;
        d.set_item(intern!(py, "int_literals"), ilits)?;
        d.set_item(intern!(py, "bool_literals"), PyDict::new(py))?;
        d.set_item(intern!(py, "def_table"), def_table)?;
        d.set_item(intern!(py, "header"), self.header(&sh.header)?)?;
        d.set_item(intern!(py, "layout_gaps"), gaps)?;
        d.set_item(intern!(py, "code_offset"), sh.code_offset)?;
        d.set_item(intern!(py, "code_size"), sh.code_size)?;
        Ok(d)
    }

    fn dest(&self, d: &Dest) -> PyResult<Bound<'py, PyAny>> {
        if let Some(raw) = d.raw_index {
            obj!(self, {
                "file" => d.file.name(), "index" => d.index, "rel" => d.rel, "raw_index" => raw,
                "write" => &d.write, "saturate" => d.saturate,
            })
        } else {
            obj!(self, {
                "file" => d.file.name(), "index" => d.index, "rel" => d.rel, "write" => &d.write,
                "saturate" => d.saturate,
            })
        }
    }

    fn operand(&self, o: &Operand) -> PyResult<Bound<'py, PyAny>> {
        let py = self.py;
        let d = PyDict::new(py);
        d.set_item(intern!(py, "kind"), intern!(py, "operand"))?;
        if o.two_op {
            d.set_item(intern!(py, "file"), o.file.name())?;
            d.set_item(intern!(py, "index"), o.index)?;
            d.set_item(intern!(py, "rel"), o.rel)?;
            d.set_item(intern!(py, "negate"), o.negate)?;
            d.set_item(intern!(py, "abs"), o.abs)?;
            d.set_item(intern!(py, "swizzle_raw"), o.swizzle_raw)?;
        } else {
            d.set_item(intern!(py, "negate"), o.negate)?;
            d.set_item(intern!(py, "swizzle_raw"), o.swizzle_raw)?;
            d.set_item(intern!(py, "swizzle"), o.swizzle())?;
            d.set_item(intern!(py, "file"), o.file.name())?;
            d.set_item(intern!(py, "index"), o.index)?;
            d.set_item(intern!(py, "rel"), o.rel)?;
            d.set_item(intern!(py, "abs"), o.abs)?;
        }
        if let Some(c) = &o.components {
            d.set_item(intern!(py, "components"), c)?;
        }
        self.wrap(d)
    }

    fn alu(&self, d: &Bound<'py, PyDict>, a: &Alu) -> PyResult<()> {
        let py = self.py;
        d.set_item(intern!(py, "predicated"), a.predicated)?;
        d.set_item(intern!(py, "pred_condition"), a.pred_condition)?;
        d.set_item(intern!(py, "is_export"), a.is_export)?;
        d.set_item(intern!(py, "abs_constants"), a.abs_constants)?;
        d.set_item(intern!(py, "const_addr_reg_relative"), a.const_addr_reg_relative)?;
        d.set_item(intern!(py, "const_0_rel"), a.const_0_rel)?;
        d.set_item(intern!(py, "const_1_rel"), a.const_1_rel)?;
        d.set_item(intern!(py, "vector_opcode"), a.vector_opcode)?;
        d.set_item(intern!(py, "vector_op"), a.vector_op())?;
        d.set_item(intern!(py, "scalar_opcode"), a.scalar_opcode)?;
        d.set_item(intern!(py, "scalar_op"), a.scalar_op())?;
        d.set_item(intern!(py, "vector_write_mask_raw"), a.vector_write_mask_raw)?;
        d.set_item(intern!(py, "scalar_write_mask_raw"), a.scalar_write_mask_raw)?;
        d.set_item(intern!(py, "vector_dest"), self.dest(&a.vector_dest)?)?;
        d.set_item(intern!(py, "scalar_dest"), self.dest(&a.scalar_dest)?)?;
        d.set_item(
            intern!(py, "vector_operands"),
            self.list(&a.vector_operands, |o| self.operand(o))?,
        )?;
        d.set_item(
            intern!(py, "scalar_operands"),
            self.list(&a.scalar_operands, |o| self.operand(o))?,
        )?;
        d.set_item(intern!(py, "vector_nop"), a.vector_nop)?;
        d.set_item(intern!(py, "scalar_nop"), a.scalar_nop)
    }

    fn fetch(&self, d: &Bound<'py, PyDict>, f: &Fetch) -> PyResult<()> {
        let py = self.py;
        d.set_item(intern!(py, "opcode"), f.opcode)?;
        d.set_item(intern!(py, "op"), &f.op)?;
        d.set_item(intern!(py, "src"), f.src)?;
        d.set_item(intern!(py, "src_rel"), f.src_rel)?;
        d.set_item(intern!(py, "dst"), f.dst)?;
        d.set_item(intern!(py, "dst_rel"), f.dst_rel)?;
        d.set_item(intern!(py, "dst_swizzle"), &f.dst_swizzle)?;
        d.set_item(intern!(py, "predicated"), f.predicated)?;
        d.set_item(intern!(py, "pred_condition"), f.pred_condition)?;
        match &f.kind {
            FetchKind::Vertex(v) => {
                d.set_item(intern!(py, "fetch_kind"), intern!(py, "vertex"))?;
                d.set_item(intern!(py, "must_be_one"), v.must_be_one)?;
                d.set_item(intern!(py, "const_index"), v.const_index)?;
                d.set_item(intern!(py, "const_index_sel"), v.const_index_sel)?;
                d.set_item(intern!(py, "prefetch_count"), v.prefetch_count)?;
                d.set_item(intern!(py, "src_swizzle"), v.src_swizzle)?;
                d.set_item(intern!(py, "is_signed"), v.is_signed)?;
                d.set_item(intern!(py, "is_integer"), v.is_integer)?;
                d.set_item(intern!(py, "signed_rf_mode"), v.signed_rf_mode)?;
                d.set_item(intern!(py, "is_index_rounded"), v.is_index_rounded)?;
                d.set_item(intern!(py, "format"), v.format)?;
                d.set_item(intern!(py, "format_name"), v.format_name())?;
                d.set_item(intern!(py, "exp_adjust"), v.exp_adjust)?;
                d.set_item(intern!(py, "is_mini"), v.is_mini)?;
                d.set_item(intern!(py, "stride"), v.stride)?;
                d.set_item(intern!(py, "offset"), v.offset)?;
                d.set_item(intern!(py, "fetch_const"), v.fetch_const)?;
                d.set_item(intern!(py, "vf"), v.vf())
            }
            FetchKind::Texture(t) => {
                d.set_item(intern!(py, "fetch_kind"), intern!(py, "texture"))?;
                d.set_item(intern!(py, "fetch_valid_only"), t.fetch_valid_only)?;
                d.set_item(intern!(py, "const_index"), t.const_index)?;
                d.set_item(intern!(py, "unnormalized"), t.unnormalized)?;
                d.set_item(intern!(py, "src_swizzle_raw"), t.src_swizzle_raw)?;
                d.set_item(intern!(py, "mag_filter"), t.mag_filter)?;
                d.set_item(intern!(py, "min_filter"), t.min_filter)?;
                d.set_item(intern!(py, "mip_filter"), t.mip_filter)?;
                d.set_item(intern!(py, "aniso_filter"), t.aniso_filter)?;
                d.set_item(intern!(py, "arbitrary_filter"), t.arbitrary_filter)?;
                d.set_item(intern!(py, "vol_mag_filter"), t.vol_mag_filter)?;
                d.set_item(intern!(py, "vol_min_filter"), t.vol_min_filter)?;
                d.set_item(intern!(py, "use_computed_lod"), t.use_computed_lod)?;
                d.set_item(intern!(py, "use_register_lod"), t.use_register_lod)?;
                d.set_item(intern!(py, "unk"), t.unk)?;
                d.set_item(intern!(py, "use_register_gradients"), t.use_register_gradients)?;
                d.set_item(intern!(py, "sample_location"), t.sample_location)?;
                d.set_item(intern!(py, "lod_bias"), t.lod_bias)?;
                d.set_item(intern!(py, "unused"), t.unused)?;
                d.set_item(intern!(py, "dimension"), t.dimension)?;
                d.set_item(intern!(py, "offset_x"), t.offset_x)?;
                d.set_item(intern!(py, "offset_y"), t.offset_y)?;
                d.set_item(intern!(py, "offset_z"), t.offset_z)?;
                d.set_item(intern!(py, "src_swizzle"), &t.src_swizzle)?;
                d.set_item(intern!(py, "has_dest"), t.has_dest)?;
                d.set_item(intern!(py, "has_const"), t.has_const)?;
                d.set_item(intern!(py, "fetch_const"), t.const_index)
            }
        }
    }

    fn instr(&self, i: &Instr) -> PyResult<Bound<'py, PyAny>> {
        let py = self.py;
        let d = PyDict::new(py);
        let is_alu = matches!(i.kind, InstrKind::Alu(_));
        d.set_item(
            intern!(py, "kind"),
            if is_alu {
                intern!(py, "alu")
            } else {
                intern!(py, "fetch")
            },
        )?;
        d.set_item(intern!(py, "address"), i.address)?;
        d.set_item(intern!(py, "raw"), PyTuple::new(py, i.raw)?)?;
        match &i.kind {
            InstrKind::Alu(a) => self.alu(&d, a)?,
            InstrKind::Fetch(f) => self.fetch(&d, f)?,
        }
        d.set_item(intern!(py, "serialize"), i.serialize)?;
        d.set_item(intern!(py, "cf_index"), i.cf_index)?;
        self.wrap(d)
    }

    fn cf(&self, cf: &Cf, instrs: &[Bound<'py, PyAny>]) -> PyResult<Bound<'py, PyAny>> {
        let py = self.py;
        let d = PyDict::new(py);
        d.set_item(intern!(py, "kind"), intern!(py, "cf"))?;
        d.set_item(intern!(py, "index"), cf.index)?;
        d.set_item(intern!(py, "opcode"), cf.opcode)?;
        d.set_item(intern!(py, "op"), cf.op())?;
        d.set_item(intern!(py, "address_mode"), cf.address_mode)?;
        d.set_item(intern!(py, "raw"), cf.raw)?;
        match &cf.kind {
            CfKind::Exec {
                address,
                count,
                is_yield,
                sequence,
                vc,
                is_end,
                type_,
                bool_index,
                condition,
                is_predicate_clean,
            } => {
                d.set_item(intern!(py, "address"), address)?;
                d.set_item(intern!(py, "count"), count)?;
                d.set_item(intern!(py, "is_yield"), is_yield)?;
                d.set_item(intern!(py, "sequence"), sequence)?;
                d.set_item(intern!(py, "vc"), vc)?;
                d.set_item(intern!(py, "is_end"), is_end)?;
                d.set_item(intern!(py, "type"), type_.name())?;
                if matches!(cf.opcode, 3 | 4)
                    && let Some(b) = bool_index
                {
                    d.set_item(intern!(py, "bool_index"), b)?;
                }
                if let Some(c) = condition {
                    d.set_item(intern!(py, "condition"), c)?;
                }
                d.set_item(intern!(py, "is_predicate_clean"), is_predicate_clean)?;
                if matches!(cf.opcode, 13 | 14)
                    && let Some(b) = bool_index
                {
                    d.set_item(intern!(py, "bool_index"), b)?;
                }
            }
            CfKind::LoopStart {
                address,
                is_repeat,
                loop_id,
            } => {
                d.set_item(intern!(py, "address"), address)?;
                d.set_item(intern!(py, "is_repeat"), is_repeat)?;
                d.set_item(intern!(py, "loop_id"), loop_id)?;
            }
            CfKind::LoopEnd {
                address,
                loop_id,
                is_predicated_break,
                condition,
            } => {
                d.set_item(intern!(py, "address"), address)?;
                d.set_item(intern!(py, "loop_id"), loop_id)?;
                d.set_item(intern!(py, "is_predicated_break"), is_predicated_break)?;
                d.set_item(intern!(py, "condition"), condition)?;
            }
            CfKind::Jump {
                address,
                is_unconditional,
                is_predicated,
                bool_index,
                condition,
                type_,
                direction,
            } => {
                d.set_item(intern!(py, "address"), address)?;
                d.set_item(intern!(py, "is_unconditional"), is_unconditional)?;
                d.set_item(intern!(py, "is_predicated"), is_predicated)?;
                d.set_item(intern!(py, "bool_index"), bool_index)?;
                d.set_item(intern!(py, "condition"), condition)?;
                d.set_item(intern!(py, "type"), type_.name())?;
                if let Some(dir) = direction {
                    d.set_item(intern!(py, "direction"), dir)?;
                }
            }
            CfKind::Alloc {
                size,
                is_unserialized,
                alloc_type,
            } => {
                d.set_item(intern!(py, "size"), size)?;
                d.set_item(intern!(py, "is_unserialized"), is_unserialized)?;
                d.set_item(intern!(py, "alloc_type"), alloc_type)?;
            }
            CfKind::Plain => {}
        }
        if let Some(list) = &cf.instructions {
            let l = PyList::empty(py);
            for &k in list {
                if let Some(o) = instrs.get(k) {
                    l.append(o)?;
                }
            }
            d.set_item(intern!(py, "instructions"), l)?;
        }
        self.wrap(d)
    }

    /// The microcode's fields: `cf`, `instructions`, `cf_slots`, `unused_slots`, `unused_nonzero_slots`,
    /// `code_words` (the CFs' instruction lists hold the same objects as `instructions`).
    fn microcode(&self, sh: &Shader) -> PyResult<Bound<'py, PyDict>> {
        let py = self.py;
        let instrs = sh
            .instructions
            .iter()
            .map(|i| self.instr(i))
            .collect::<PyResult<Vec<_>>>()?;
        let cfs = PyList::empty(py);
        for cf in &sh.cf {
            cfs.append(self.cf(cf, &instrs)?)?;
        }
        let d = PyDict::new(py);
        d.set_item(intern!(py, "cf"), cfs)?;
        d.set_item(intern!(py, "instructions"), PyList::new(py, &instrs)?)?;
        d.set_item(intern!(py, "cf_slots"), sh.cf_slots)?;
        d.set_item(intern!(py, "unused_slots"), PyList::new(py, &sh.unused_slots)?)?;
        d.set_item(
            intern!(py, "unused_nonzero_slots"),
            PyList::new(py, &sh.unused_nonzero_slots)?,
        )?;
        d.set_item(intern!(py, "code_words"), PyList::new(py, &sh.code_words)?)?;
        Ok(d)
    }
}

fn parsed<'py>(b: &Builder<'py>, sh: Shader) -> PyResult<Bound<'py, PyTuple>> {
    let d = b.shader(&sh)?;
    PyTuple::new(
        b.py,
        [
            Bound::new(
                b.py,
                XenosProgram {
                    shader: Arc::new(sh),
                    cube: None,
                },
            )?
            .into_any(),
            d.into_any(),
        ],
    )
}

/// `xenos.parse_shader(blob, is_pixel)` through Rust: `(program, dict)` - the program for `xenos_translate` and
/// `xenos_microcode`, and the shader's attributes without the microcode's, nested records made with `obj_cls`; `None`
/// when the decoder refuses the blob (the Python decoder then raises its own exception).
#[pyfunction]
#[pyo3(signature = (blob, is_pixel, obj_cls))]
pub fn xenos_parse<'py>(
    py: Python<'py>,
    blob: &Bound<'py, PyBytes>,
    is_pixel: Option<bool>,
    obj_cls: &Bound<'py, PyAny>,
) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let bytes = blob.as_bytes();
    match py.detach(|| xenos::parse(bytes, is_pixel)) {
        Ok(sh) => Ok(Some(parsed(&Builder::new(obj_cls)?, sh)?)),
        Err(_) => Ok(None),
    }
}

/// [`xenos_parse`] of many `(blob, is_pixel)` at once, decoded on all cores.
#[pyfunction]
pub fn xenos_parse_many<'py>(
    py: Python<'py>,
    items: Vec<(Bound<'py, PyBytes>, Option<bool>)>,
    obj_cls: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyList>> {
    let jobs: Vec<(&[u8], Option<bool>)> = items.iter().map(|(b, p)| (b.as_bytes(), *p)).collect();
    let decoded: Vec<Result<Shader, xenos::XenosError>> = py.detach(|| {
        use rayon::prelude::*;
        jobs.par_iter().map(|(b, p)| xenos::parse(b, *p)).collect()
    });
    let b = Builder::new(obj_cls)?;
    let out = PyList::empty(py);
    for r in decoded {
        match r {
            Ok(sh) => out.append(parsed(&b, sh)?)?,
            Err(_) => out.append(py.None())?,
        }
    }
    Ok(out)
}

/// The microcode's attributes of a program (`cf`, `instructions`, `cf_slots`, `unused_slots`,
/// `unused_nonzero_slots`, `code_words`), records made with `obj_cls`.
#[pyfunction]
pub fn xenos_microcode<'py>(
    program: &Bound<'py, XenosProgram>,
    obj_cls: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyDict>> {
    Builder::new(obj_cls)?.microcode(&program.get().shader)
}

// ------------------------------------------------------------------------------------------------- interface

/// The value read from the Python object, or `None` when it is not of the type the translator reads it as exactly.
type Got<T> = PyResult<Option<T>>;

/// An exact `int` (not `bool`: it would print as `True`).
fn exact_int(v: &Bound<'_, PyAny>) -> Option<i64> {
    if v.is_exact_instance_of::<PyInt>() {
        v.extract().ok()
    } else {
        None
    }
}

/// A dictionary key: `int` or `bool` (the same key in a dict).
fn key_int(v: &Bound<'_, PyAny>) -> Option<i64> {
    if v.is_instance_of::<PyInt>() {
        v.extract().ok()
    } else {
        None
    }
}

fn exact_str(v: &Bound<'_, PyAny>) -> Option<String> {
    if v.is_exact_instance_of::<PyString>() {
        v.extract().ok()
    } else {
        None
    }
}

fn seq<'py>(v: &Bound<'py, PyAny>) -> Option<Vec<Bound<'py, PyAny>>> {
    if let Ok(l) = v.cast::<PyList>() {
        Some(l.iter().collect())
    } else if let Ok(t) = v.cast::<PyTuple>() {
        Some(t.iter().collect())
    } else {
        None
    }
}

fn numbers(v: &Bound<'_, PyAny>) -> Option<Vec<f64>> {
    seq(v)?.iter().map(|x| x.extract::<f64>().ok()).collect()
}

fn dict_of<'py, K: Eq + Hash, V>(
    v: &Bound<'py, PyAny>,
    key: impl Fn(&Bound<'py, PyAny>) -> Option<K>,
    val: impl Fn(&Bound<'py, PyAny>) -> Option<Option<V>>,
) -> Option<HashMap<K, V>> {
    let d = v.cast::<PyDict>().ok()?;
    let mut out = HashMap::with_capacity(d.len());
    for (k, x) in d.iter() {
        if let Some(x) = val(&x)? {
            out.insert(key(&k)?, x);
        }
    }
    Some(out)
}

/// `Interface`'s fields as the translator reads them, or the field that is of another type (`Err`).
fn interface<'py>(o: &Bound<'py, PyAny>) -> PyResult<Result<Interface, &'static str>> {
    let py = o.py();
    let get = |n: &Bound<'py, PyString>| -> PyResult<Bound<'py, PyAny>> { o.getattr(n) };
    let Some(interpolators) = seq(&get(intern!(py, "interpolators"))?).and_then(|s| s.iter().map(exact_int).collect())
    else {
        return Ok(Err("interpolators"));
    };
    let Some(vs_inputs) = dict_of(&get(intern!(py, "vs_inputs"))?, exact_int, |v| exact_str(v).map(Some)) else {
        return Ok(Err("vs_inputs"));
    };
    let Some(material) = dict_of(&get(intern!(py, "material"))?, exact_str, |v| {
        if v.is_none() {
            return Some(None);
        }
        let s = seq(v)?;
        let first = s.first()?;
        if first.is_instance_of::<PyInt>() || first.is_instance_of::<PyFloat>() {
            Some(Some(vec![numbers(v)?]))
        } else {
            s.iter().map(numbers).collect::<Option<Vec<_>>>().map(Some)
        }
    }) else {
        return Ok(Err("material"));
    };
    let Some(sampler_fix) = dict_of(&get(intern!(py, "sampler_fix"))?, key_int, |v| {
        if v.is_truthy().ok()? {
            exact_str(v).map(Some)
        } else {
            Some(None)
        }
    }) else {
        return Ok(Err("sampler_fix"));
    };
    let Ok(colour_scale) = get(intern!(py, "colour_scale"))?.extract::<f64>() else {
        return Ok(Err("colour_scale"));
    };
    let truth = |n: &Bound<'py, PyString>| -> PyResult<bool> { get(n)?.is_truthy() };
    let Some(material_regs) = dict_of(&get(intern!(py, "material_regs"))?, key_int, |v| numbers(v).map(Some)) else {
        return Ok(Err("material_regs"));
    };
    let ints = |n: &Bound<'py, PyString>| -> Got<HashMap<i64, i64>> {
        Ok(dict_of(&get(n)?, key_int, |v| exact_int(v).map(Some)))
    };
    let names = |n: &Bound<'py, PyString>| -> Got<HashMap<i64, String>> {
        Ok(dict_of(&get(n)?, key_int, |v| {
            if v.is_none() {
                Some(None)
            } else {
                exact_str(v).map(Some)
            }
        }))
    };
    let Some(sampler_registers) = ints(intern!(py, "sampler_registers"))? else {
        return Ok(Err("sampler_registers"));
    };
    let Some(fetch_registers) = ints(intern!(py, "fetch_registers"))? else {
        return Ok(Err("fetch_registers"));
    };
    let Some(named_vs) = names(intern!(py, "named_vs"))? else {
        return Ok(Err("named_vs"));
    };
    let Some(named_ps) = names(intern!(py, "named_ps"))? else {
        return Ok(Err("named_ps"));
    };
    let probe_v = get(intern!(py, "probe"))?;
    let probe = if probe_v.is_truthy()? {
        let Some(p) = seq(&probe_v).and_then(|s| s.iter().map(exact_str).collect::<Option<Vec<_>>>()) else {
            return Ok(Err("probe"));
        };
        Some(p)
    } else {
        None
    };
    Ok(Ok(Interface {
        interpolators,
        vs_inputs: vs_inputs.into_iter().collect(),
        material,
        sampler_fix,
        colour_scale,
        instanced: truth(intern!(py, "instanced"))?,
        bones_as_world: truth(intern!(py, "bones_as_world"))?,
        material_regs,
        pc_shadow_atlas: truth(intern!(py, "pc_shadow_atlas"))?,
        sampler_registers,
        named_vs,
        named_ps,
        fetch_registers,
        trace_nan: truth(intern!(py, "trace_nan"))?,
        probe,
    }))
}

/// (status, text): 0 the HLSL, 1 a `TranslateError` and its message, 2 not translated and why (`xenos_hlsl` raises
/// `ValueError` with it).
fn status(r: Result<String, TranslateError>) -> (u8, String) {
    match r {
        Ok(t) => (0, t),
        Err(TranslateError::Translate(m)) => (1, m),
        Err(e) => (2, e.to_string()),
    }
}

fn run(sh: &Shader, cube: Option<bool>, iface: &Interface) -> (u8, String) {
    status(match cube {
        Some(c) => hlsl::translate_given(sh, iface, c),
        None => hlsl::translate(sh, iface),
    })
}

fn interface_of(iface: &Bound<'_, PyAny>) -> PyResult<Result<Interface, String>> {
    Ok(interface(iface)?.map_err(|f| {
        format!("the interface's {f} is of another type than the translator reads (Interface's comments)")
    }))
}

/// `xenos_hlsl.translate(program, iface)`: `(status, text)` - 0 the HLSL text, 1 a `TranslateError`'s message, 2
/// not translated and why (an interface value of another type than the translator reads, or a program the Python
/// translator ended with another exception: an empty material value, a sampler fix with other format fields, an
/// engine-named constant that is not an ASCII name).
#[pyfunction]
pub fn xenos_translate(
    py: Python<'_>,
    program: &Bound<'_, XenosProgram>,
    iface: &Bound<'_, PyAny>,
) -> PyResult<(u8, String)> {
    let iface = match interface_of(iface)? {
        Ok(i) => i,
        Err(why) => return Ok((2, why)),
    };
    let p = program.get();
    let (sh, cube) = (Arc::clone(&p.shader), p.cube);
    Ok(py.detach(|| run(&sh, cube, &iface)))
}

/// A translation to run: the program, its `cube` and the interface.
type Job = (Arc<Shader>, Option<bool>, Interface);

/// [`xenos_translate`] of many `(program, iface)` on all cores, in input order.
#[pyfunction]
pub fn xenos_translate_many<'py>(
    py: Python<'py>,
    items: Vec<(Bound<'py, XenosProgram>, Bound<'py, PyAny>)>,
) -> PyResult<Vec<(u8, String)>> {
    let mut jobs: Vec<Result<Job, String>> = Vec::with_capacity(items.len());
    for (p, i) in &items {
        let p = p.get();
        jobs.push(interface_of(i)?.map(|iface| (Arc::clone(&p.shader), p.cube, iface)));
    }
    Ok(py.detach(|| {
        use rayon::prelude::*;
        jobs.par_iter()
            .map(|j| match j {
                Ok((sh, cube, iface)) => run(sh, *cube, iface),
                Err(why) => (2, why.clone()),
            })
            .collect()
    }))
}

/// The helper functions every translation starts with (`hlsl::HELPERS`): tests compile pieces of a translation
/// with them.
#[pyfunction]
pub fn xenos_helpers() -> &'static str {
    hlsl::HELPERS
}

/// `repr(float(x))` as the translator writes numbers (the test of the Python repr rules).
#[pyfunction]
pub fn xenos_float_repr(x: f64) -> String {
    bc2_shader::pyrepr::float_repr(x)
}

// ------------------------------------------------------------------------------------------------- trees by hand

/// The translator's refusal of a tree it cannot hold (`ValueError`).
fn decline(what: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(format!("xenos_hlsl.translate: the tree is not translated: {what}"))
}

/// A name the decoded program keeps as a `&'static str`: each distinct one leaked once (the tables' names, and
/// whatever few others hand-built trees hold).
fn interned(s: &str) -> &'static str {
    static NAMES: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut set = NAMES
        .get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(&n) = set.get(s) {
        return n;
    }
    let n: &'static str = Box::leak(s.to_owned().into_boxed_str());
    set.insert(n);
    n
}

/// A node of a tree: the object and its instance `__dict__`, read first (a plain lookup, no exception for an
/// attribute it lacks); an attribute not there (a class attribute, a property, `__getattr__`) is read with `getattr`.
struct Node<'py> {
    o: Bound<'py, PyAny>,
    d: Option<Bound<'py, PyDict>>,
}

type Name<'a, 'py> = &'a Bound<'py, PyString>;

impl<'py> Node<'py> {
    fn new(o: Bound<'py, PyAny>) -> Self {
        let d = o
            .getattr(intern!(o.py(), "__dict__"))
            .ok()
            .and_then(|d| d.cast_into::<PyDict>().ok());
        Self { o, d }
    }

    fn py(&self) -> Python<'py> {
        self.o.py()
    }

    /// `getattr(o, name, None)`: `None` for an attribute it lacks.
    fn get(&self, name: Name<'_, 'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
        if let Some(d) = &self.d
            && let Some(v) = d.get_item(name)?
        {
            return Ok(Some(v));
        }
        match self.o.getattr(name) {
            Ok(v) => Ok(Some(v)),
            Err(e) if e.is_instance_of::<PyAttributeError>(self.py()) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// `o.name`: `AttributeError` for an attribute it lacks (as the Python translator's read raised).
    fn req(&self, name: Name<'_, 'py>) -> PyResult<Bound<'py, PyAny>> {
        match self.get(name)? {
            Some(v) => Ok(v),
            None => self.o.getattr(name),
        }
    }

    fn node(&self, name: Name<'_, 'py>) -> PyResult<Node<'py>> {
        Ok(Node::new(self.req(name)?))
    }

    fn truthy(&self, name: Name<'_, 'py>) -> PyResult<bool> {
        self.req(name)?.is_truthy()
    }

    fn truthy_or(&self, name: Name<'_, 'py>, default: bool) -> PyResult<bool> {
        self.get(name)?.map_or(Ok(default), |v| v.is_truthy())
    }

    fn uint(&self, name: Name<'_, 'py>) -> PyResult<u32> {
        number(&self.req(name)?, name)
    }

    fn uint_or(&self, name: Name<'_, 'py>, default: u32) -> PyResult<u32> {
        self.get(name)?.map_or(Ok(default), |v| number(&v, name))
    }

    fn float_or(&self, name: Name<'_, 'py>, default: f64) -> PyResult<f64> {
        self.get(name)?.map_or(Ok(default), |v| number(&v, name))
    }

    fn text(&self, name: Name<'_, 'py>) -> PyResult<String> {
        text(&self.req(name)?, name)
    }

    fn text_or(&self, name: Name<'_, 'py>) -> PyResult<String> {
        self.get(name)?.map_or(Ok(String::new()), |v| text(&v, name))
    }

    /// `o.name == s`, compared as Python compares.
    fn is(&self, name: Name<'_, 'py>, s: &str) -> PyResult<bool> {
        self.req(name)?.eq(s)
    }

    fn items(&self, name: Name<'_, 'py>) -> PyResult<Vec<Bound<'py, PyAny>>> {
        self.req(name)?.try_iter()?.collect()
    }

    fn nodes(&self, name: Name<'_, 'py>) -> PyResult<Vec<Node<'py>>> {
        Ok(self.items(name)?.into_iter().map(Node::new).collect())
    }
}

fn number<T: for<'a, 'py> FromPyObject<'a, 'py>>(v: &Bound<'_, PyAny>, name: &Bound<'_, PyString>) -> PyResult<T> {
    v.extract::<T>().map_err(|_| {
        decline(format!(
            "{name} {}",
            v.repr().map_or_else(|_| "?".into(), |r| r.to_string())
        ))
    })
}

fn text(v: &Bound<'_, PyAny>, name: &Bound<'_, PyString>) -> PyResult<String> {
    if let Ok(s) = v.cast::<PyString>() {
        Ok(s.to_cow()?.into_owned())
    } else {
        Err(decline(format!("{name} is not a str")))
    }
}

/// A table name, or a number past the table.
fn named_of(v: Option<Bound<'_, PyAny>>) -> PyResult<Named> {
    Ok(match v {
        Some(v) if v.is_instance_of::<PyString>() => Named::Name(interned(&text(&v, intern!(v.py(), "name"))?)),
        Some(v) => Named::Number(v.extract::<u32>().unwrap_or(u32::MAX)),
        None => Named::Number(u32::MAX),
    })
}

/// `rel`: None (or anything false) or the address register's name.
fn rel_of(v: Option<Bound<'_, PyAny>>) -> PyResult<xenos::Rel> {
    match v {
        Some(v) if v.is_truthy()? => Ok(Some(interned(&text(&v, intern!(v.py(), "rel"))?))),
        _ => Ok(None),
    }
}

/// A name's index in an opcode table (`xenos.py`'s names), or a refusal.
fn opcode<'a>(table: impl Iterator<Item = &'a str>, name: &str, what: &str) -> PyResult<u32> {
    table
        .zip(0u32..)
        .find_map(|(n, k)| (n == name).then_some(k))
        .ok_or_else(|| decline(format!("{what} {name:?} is no Xenos operation")))
}

/// An operand: a vector one's `swizzle`, a scalar one's `components` (each read where the translation reads it).
fn tree_operand(o: &Node<'_>, scalar: bool) -> PyResult<Operand> {
    let py = o.py();
    let (swizzle, components) = if scalar {
        (None, o.get(intern!(py, "components"))?)
    } else {
        (o.get(intern!(py, "swizzle"))?, None)
    };
    Ok(Operand {
        two_op: false,
        negate: o.truthy(intern!(py, "negate"))?,
        swizzle_raw: 0,
        swizzle: swizzle.map(|v| text(&v, intern!(py, "swizzle"))).transpose()?,
        file: if o.is(intern!(py, "file"), "temp")? {
            File::Temp
        } else {
            File::Const
        },
        index: o.uint(intern!(py, "index"))?,
        rel: rel_of(Some(o.req(intern!(py, "rel"))?))?,
        abs: o.truthy(intern!(py, "abs"))?,
        components: components.map(|v| text(&v, intern!(py, "components"))).transpose()?,
    })
}

fn tree_dest(d: &Node<'_>) -> PyResult<Dest> {
    let py = d.py();
    let file = d.text(intern!(py, "file"))?;
    let index = match d.get(intern!(py, "index"))? {
        Some(v) if !v.is_none() => Some(number(&v, intern!(py, "index"))?),
        _ => None,
    };
    Ok(Dest {
        file: match file.as_str() {
            "temp" => File::Temp,
            "const" => File::Const,
            f => File::Export(interned(f)),
        },
        index,
        rel: rel_of(d.get(intern!(py, "rel"))?)?,
        raw_index: None,
        write: d.text(intern!(py, "write"))?,
        saturate: d.truthy_or(intern!(py, "saturate"), false)?,
    })
}

fn no_dest() -> Dest {
    Dest {
        file: File::Temp,
        index: Some(0),
        rel: None,
        raw_index: None,
        write: String::new(),
        saturate: false,
    }
}

fn tree_alu(i: &Node<'_>) -> PyResult<Alu> {
    let py = i.py();
    let vector_nop = i.truthy(intern!(py, "vector_nop"))?;
    let scalar_nop = i.truthy(intern!(py, "scalar_nop"))?;
    let (vector_opcode, vector_operands, vector_dest) = if vector_nop {
        (0, Vec::new(), no_dest())
    } else {
        let op = i.text(intern!(py, "vector_op"))?;
        let ops = i
            .nodes(intern!(py, "vector_operands"))?
            .iter()
            .map(|o| tree_operand(o, false))
            .collect::<PyResult<Vec<_>>>()?;
        (
            opcode(xenos::VECTOR_OPS.iter().map(|o| o.0), &op, "vector op")?,
            ops,
            tree_dest(&i.node(intern!(py, "vector_dest"))?)?,
        )
    };
    let (scalar_opcode, scalar_operands, scalar_dest) = if scalar_nop {
        (0, Vec::new(), no_dest())
    } else {
        let op = i.text(intern!(py, "scalar_op"))?;
        let ops = i
            .nodes(intern!(py, "scalar_operands"))?
            .iter()
            .map(|o| tree_operand(o, true))
            .collect::<PyResult<Vec<_>>>()?;
        (
            opcode(xenos::SCALAR_OPS.iter().map(|o| o.0), &op, "scalar op")?,
            ops,
            tree_dest(&i.node(intern!(py, "scalar_dest"))?)?,
        )
    };
    Ok(Alu {
        predicated: i.truthy(intern!(py, "predicated"))?,
        pred_condition: i.truthy_or(intern!(py, "pred_condition"), false)?,
        is_export: false,
        abs_constants: false,
        const_addr_reg_relative: false,
        const_0_rel: false,
        const_1_rel: false,
        vector_opcode,
        scalar_opcode,
        vector_write_mask_raw: 0,
        scalar_write_mask_raw: 0,
        vector_dest,
        scalar_dest,
        vector_operands,
        scalar_operands,
        vector_nop,
        scalar_nop,
    })
}

fn tree_fetch(i: &Node<'_>) -> PyResult<Fetch> {
    let py = i.py();
    let vertex = i.is(intern!(py, "fetch_kind"), "vertex")?;
    let kind = if vertex {
        FetchKind::Vertex(xenos::VertexFetch {
            must_be_one: 0,
            const_index: 0,
            const_index_sel: 0,
            prefetch_count: 0,
            src_swizzle: 'x',
            is_signed: false,
            is_integer: false,
            signed_rf_mode: 0,
            is_index_rounded: false,
            format: 0,
            exp_adjust: 0,
            is_mini: false,
            stride: 0,
            offset: 0,
            fetch_const: 0,
        })
    } else {
        FetchKind::Texture(xenos::TextureFetch {
            fetch_valid_only: false,
            const_index: i.uint_or(intern!(py, "const_index"), 0)?,
            unnormalized: i.truthy_or(intern!(py, "unnormalized"), false)?,
            src_swizzle_raw: i.uint_or(intern!(py, "src_swizzle_raw"), 0)?,
            mag_filter: interned(&i.text_or(intern!(py, "mag_filter"))?),
            min_filter: interned(&i.text_or(intern!(py, "min_filter"))?),
            mip_filter: "",
            aniso_filter: "",
            arbitrary_filter: "",
            vol_mag_filter: "",
            vol_min_filter: "",
            use_computed_lod: i.truthy_or(intern!(py, "use_computed_lod"), true)?,
            use_register_lod: i.truthy_or(intern!(py, "use_register_lod"), false)?,
            unk: 0,
            use_register_gradients: i.truthy_or(intern!(py, "use_register_gradients"), false)?,
            sample_location: "",
            lod_bias: i.float_or(intern!(py, "lod_bias"), 0.0)?,
            unused: 0,
            dimension: "",
            offset_x: i.float_or(intern!(py, "offset_x"), 0.0)?,
            offset_y: i.float_or(intern!(py, "offset_y"), 0.0)?,
            offset_z: 0.0,
            src_swizzle: i.text_or(intern!(py, "src_swizzle"))?,
            has_dest: i.truthy_or(intern!(py, "has_dest"), false)?,
            has_const: false,
        })
    };
    Ok(Fetch {
        opcode: 0,
        op: if vertex {
            "vfetch".into()
        } else {
            i.text(intern!(py, "op"))?
        },
        src: i.uint_or(intern!(py, "src"), 0)?,
        src_rel: None,
        dst: i.uint_or(intern!(py, "dst"), 0)?,
        dst_rel: None,
        dst_swizzle: i.text_or(intern!(py, "dst_swizzle"))?,
        predicated: i.truthy(intern!(py, "predicated"))?,
        pred_condition: i.truthy_or(intern!(py, "pred_condition"), false)?,
        kind,
    })
}

fn tree_instr(i: &Node<'_>) -> PyResult<Instr> {
    let py = i.py();
    let alu = i.is(intern!(py, "kind"), "alu")?;
    Ok(Instr {
        address: if alu {
            i.uint(intern!(py, "address"))?
        } else {
            i.uint_or(intern!(py, "address"), 0)?
        },
        raw: [0; 3],
        kind: if alu {
            InstrKind::Alu(tree_alu(i)?)
        } else {
            InstrKind::Fetch(tree_fetch(i)?)
        },
        serialize: false,
        cf_index: 0,
    })
}

fn tree_constant(c: &Node<'_>) -> PyResult<Constant> {
    let py = c.py();
    let regset = named_of(Some(c.req(intern!(py, "regset"))?))?;
    let float4 = regset.is("float4");
    let used = float4 || regset.is("sampler");
    let (cls, type_) = match c.get(intern!(py, "type"))? {
        Some(t) => {
            let t = Node::new(t);
            (
                named_of(t.get(intern!(py, "cls"))?)?,
                named_of(t.get(intern!(py, "type"))?)?,
            )
        }
        None => (Named::Number(u32::MAX), Named::Number(u32::MAX)),
    };
    Ok(Constant {
        name: if used {
            c.text(intern!(py, "name"))?
        } else {
            String::new()
        },
        regset,
        reg: if used {
            number(&c.req(intern!(py, "reg"))?, intern!(py, "reg"))?
        } else {
            0
        },
        count: if float4 {
            number(&c.req(intern!(py, "count"))?, intern!(py, "count"))?
        } else {
            1
        },
        reserved: 0,
        type_: ParamType {
            cls,
            type_,
            rows: 0,
            cols: 0,
            elements: 0,
            members: Vec::new(),
        },
        default_offset: 0,
    })
}

fn tree_header(h: &Node<'_>, ps: bool) -> PyResult<Header> {
    let py = h.py();
    let mut interpolators = Vec::new();
    for it in h.nodes(intern!(py, "interpolators"))? {
        interpolators.push(Interpolator {
            raw: 0,
            usage_index: it.uint(intern!(py, "usage_index"))?,
            usage: "",
            reg: if ps {
                it.uint(intern!(py, "reg"))?
            } else {
                it.uint_or(intern!(py, "reg"), 0)?
            },
            mask: 0,
            export_index: 0,
            export_addresses: None,
        });
    }
    let mut vertex_elements = Vec::new();
    if !ps {
        for el in h.nodes(intern!(py, "vertex_elements"))? {
            let usage = interned(&el.text(intern!(py, "usage"))?);
            let texcoord = usage == "TEXCOORD";
            vertex_elements.push(VertexElement {
                raw: 0,
                fetch_address: if texcoord {
                    el.uint(intern!(py, "fetch_address"))?
                } else {
                    0
                },
                usage,
                usage_index: if texcoord {
                    el.uint(intern!(py, "usage_index"))?
                } else {
                    0
                },
                unknown_hi: 0,
            });
            if !texcoord {
                break; // the translation stops at it
            }
        }
    }
    let (stage, context_misc) = if ps {
        let cmf = h.node(intern!(py, "context_misc_fields"))?;
        let xy = cmf.truthy(intern!(py, "sc_output_screen_xy"))?;
        let pos = if xy { cmf.uint(intern!(py, "param_gen_pos"))? } else { 0 };
        if pos > 0xff {
            return Err(decline(format!("param_gen_pos {pos} (8 bits)")));
        }
        (
            Stage::Pixel {
                interp_mask: 0,
                outputs: h.uint(intern!(py, "outputs"))?,
            },
            u32::from(xy) << 1 | pos << 8,
        )
    } else {
        (
            Stage::Vertex {
                field18: 0,
                vertex_element_count: 0,
                field20: 0,
                field18_words: Vec::new(),
            },
            0,
        )
    };
    Ok(Header {
        offset: 0,
        physical_offset: 0,
        physical_size: 0,
        program_cntl: 0,
        context_misc,
        field10: 0,
        interpolator_info: 0,
        num_temp_regs: h.uint(intern!(py, "num_temp_regs"))?,
        interpolator_count: 0,
        vertex_elements,
        interpolators,
        stage,
        export_slots: Vec::new(),
        end: 0,
    })
}

fn tree_literals(v: &Bound<'_, PyAny>) -> PyResult<Vec<(i64, [f32; 4])>> {
    let py = v.py();
    let mut literals = Vec::new();
    for kv in v.call_method0(intern!(py, "items"))?.try_iter()? {
        let (k, v): (Bound<'_, PyAny>, Bound<'_, PyAny>) = kv?.extract()?;
        let reg: i64 = number(&k, intern!(py, "literal register"))?;
        let vals = v
            .try_iter()?
            .map(|x| number::<f64>(&x?, intern!(py, "literal value")))
            .collect::<PyResult<Vec<_>>>()?;
        if vals.len() != 4 {
            return Err(decline(format!("literal c{reg}: {} values (4)", vals.len())));
        }
        let mut four = [0f32; 4];
        for (o, &x) in four.iter_mut().zip(&vals) {
            #[allow(clippy::cast_possible_truncation)] // checked: the value is a 32-bit float's
            let f = x as f32;
            if f64::from(f).to_bits() != x.to_bits() && !x.is_nan() {
                return Err(decline(format!("literal c{reg}: {x:?} is no 32-bit float")));
            }
            *o = f;
        }
        literals.push((reg, four));
    }
    Ok(literals)
}

/// The program's instructions as the CFs run them: each object converted once (by identity).
#[derive(Default)]
struct Run {
    instructions: Vec<Instr>,
    seen: HashMap<usize, usize>,
}

impl Run {
    fn index(&mut self, i: Bound<'_, PyAny>) -> PyResult<usize> {
        let id = i.as_ptr() as usize;
        if let Some(&k) = self.seen.get(&id) {
            return Ok(k);
        }
        self.instructions.push(tree_instr(&Node::new(i))?);
        self.seen.insert(id, self.instructions.len() - 1);
        Ok(self.instructions.len() - 1)
    }
}

fn tree_cf(c: &Node<'_>, run: &mut Run) -> PyResult<Cf> {
    let py = c.py();
    let op = c.text(intern!(py, "op"))?;
    let code = opcode(xenos::CF_OPS.iter().copied(), &op, "control flow")?;
    let index = c.uint_or(intern!(py, "index"), u32::MAX)?;
    let (kind, list) = match op.as_str() {
        "exec" | "exec_end" | "cond_exec_pred" | "cond_exec_pred_end" => {
            let condition = op.starts_with("cond_exec_pred") && c.truthy(intern!(py, "condition"))?;
            let mut list = Vec::new();
            for i in c.items(intern!(py, "instructions"))? {
                list.push(run.index(i)?);
            }
            (
                CfKind::Exec {
                    address: 0,
                    count: 0,
                    is_yield: 0,
                    sequence: 0,
                    vc: 0,
                    is_end: c.truthy_or(intern!(py, "is_end"), false)?,
                    type_: CfType::Unconditional,
                    bool_index: None,
                    condition: Some(u32::from(condition)),
                    is_predicate_clean: false,
                },
                Some(list),
            )
        }
        "cond_jmp" => {
            let type_ = if c.is(intern!(py, "type"), "unconditional")? {
                CfType::Unconditional
            } else if c.is(intern!(py, "type"), "predicated")? {
                CfType::Predicated
            } else {
                CfType::Conditional
            };
            let condition = type_ == CfType::Predicated && c.truthy(intern!(py, "condition"))?;
            (
                CfKind::Jump {
                    address: c.uint(intern!(py, "address"))?,
                    is_unconditional: 0,
                    is_predicated: 0,
                    bool_index: 0,
                    condition: u32::from(condition),
                    type_,
                    direction: None,
                },
                None,
            )
        }
        _ => (CfKind::Plain, None),
    };
    Ok(Cf {
        index,
        opcode: code,
        address_mode: 0,
        raw: (0, 0),
        kind,
        instructions: list,
    })
}

/// A tree a caller handed over (`xenos_tree_program`): the program, and whether the tree's own `instructions` hold
/// a vector `cube` (`i.kind == 'alu' and i.vector_op == 'cube' and not i.vector_nop`, compared as Python compares).
fn tree(sh: &Node<'_>) -> PyResult<(Shader, bool)> {
    let py = sh.py();
    let is_pixel = sh.truthy(intern!(py, "is_pixel"))?;
    let ctab_v = sh.req(intern!(py, "ctab"))?;
    let ctab = if ctab_v.is_truthy()? {
        let constants = Node::new(ctab_v)
            .nodes(intern!(py, "constants"))?
            .iter()
            .map(tree_constant)
            .collect::<PyResult<Vec<_>>>()?;
        Some(Ctab {
            offset: 0,
            size: 0,
            base: 0,
            creator: String::new(),
            version: 0,
            target: String::new(),
            flags: 0,
            constants,
        })
    } else {
        None
    };
    let literals = tree_literals(&sh.req(intern!(py, "literals"))?)?;
    let mut cube = false;
    for i in sh.nodes(intern!(py, "instructions"))? {
        if i.is(intern!(py, "kind"), "alu")?
            && i.is(intern!(py, "vector_op"), "cube")?
            && !i.truthy(intern!(py, "vector_nop"))?
        {
            cube = true;
            break;
        }
    }
    let header = tree_header(&sh.node(intern!(py, "header"))?, is_pixel)?;
    let mut run = Run::default();
    let mut cf = Vec::new();
    for c in sh.nodes(intern!(py, "cf"))? {
        cf.push(tree_cf(&c, &mut run)?);
    }
    Ok((
        Shader {
            is_pixel,
            magic: 0,
            container: [0; 8],
            ctab,
            literals,
            int_literals: Vec::new(),
            def_table: None,
            header,
            layout_gaps: Vec::new(),
            code_offset: 0,
            code_size: 0,
            cf,
            instructions: run.instructions,
            cf_slots: 0,
            unused_slots: Vec::new(),
            unused_nonzero_slots: Vec::new(),
            code_words: Vec::new(),
        },
        cube,
    ))
}

/// The program of a tree a caller handed over: `xenos.parse_shader_python`'s, a Rust-decoded tree whose microcode
/// was read (and maybe edited), its copies and pickles, or one built by hand of any class. The attributes the
/// translation reads are read by name, as `xenos_hlsl.py`'s Python translator read them (retired 2026-11-18): one it
/// read only on some paths may be missing (its default taken). `ValueError` for what the program cannot hold: a
/// literal of another length or not a 32-bit float, an operation's unknown name, a number or name of another type.
#[pyfunction]
pub fn xenos_tree_program<'py>(py: Python<'py>, tree_obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, XenosProgram>> {
    let (shader, cube) = tree(&Node::new(tree_obj.clone()))?;
    Bound::new(
        py,
        XenosProgram {
            shader: Arc::new(shader),
            cube: Some(cube),
        },
    )
}
