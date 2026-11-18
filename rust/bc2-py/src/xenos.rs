//! The bindings of the Xbox 360 shader translator (`bc2_shader`): `bfbc2-shader-tools/xenos.py`'s `parse_shader`
//! and `xenos_hlsl.py`'s `translate` call them when the module is built.
//!
//! `xenos_parse` decodes a blob and gives a [`XenosProgram`] (the decoded program, kept in Rust) with the tree
//! `parse_shader` builds without its microcode: dicts in `xenos.py`'s attribute order, every nested record made
//! through the caller's `Obj` class (`object.__new__` and its `__dict__`, no Python call per record).
//! `xenos_microcode` gives the rest (`cf`, `instructions` ...: `xenos.py` builds them only when a caller reads them).
//! `xenos_translate` translates a program with an `Interface` read from the Python object.
//!
//! Anything the Python would treat differently is handed back to it: a blob the decoder refuses (`None`: the Python
//! decoder raises its own exception), an interface value of another type than the Python reads it as, or a program
//! the translator leaves to the Python (status 2). Status 1 is `TranslateError` with its message.
// the Python's names (vsize / psize, ctab_off / def_off ...), the tree's attributes one after another, and the
// batch functions' lists, which PyO3 extracts by value
#![allow(clippy::similar_names, clippy::too_many_lines, clippy::needless_pass_by_value)]

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

use bc2_shader::hlsl::{self, Interface, TranslateError};
use bc2_shader::xenos::{
    self, Alu, Cf, CfKind, Constant, Dest, Fetch, FetchKind, Header, Instr, InstrKind, Named, Operand, ParamType,
    Shader, Stage,
};
use pyo3::IntoPyObjectExt;
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyBytesMethods, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};

/// A decoded Xenos program, kept in Rust for the translator.
#[pyclass(frozen, module = "bc2_native")]
pub struct XenosProgram {
    shader: Arc<Shader>,
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
            Bound::new(b.py, XenosProgram { shader: Arc::new(sh) })?.into_any(),
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

/// `Interface`'s fields as the translator reads them, or `None` when one is of another type.
fn interface<'py>(o: &Bound<'py, PyAny>) -> Got<Interface> {
    let py = o.py();
    let get = |n: &Bound<'py, PyString>| -> PyResult<Bound<'py, PyAny>> { o.getattr(n) };
    let Some(interpolators) = seq(&get(intern!(py, "interpolators"))?).and_then(|s| s.iter().map(exact_int).collect())
    else {
        return Ok(None);
    };
    let Some(vs_inputs) = dict_of(&get(intern!(py, "vs_inputs"))?, exact_int, |v| exact_str(v).map(Some)) else {
        return Ok(None);
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
        return Ok(None);
    };
    let Some(sampler_fix) = dict_of(&get(intern!(py, "sampler_fix"))?, key_int, |v| {
        if v.is_truthy().ok()? {
            exact_str(v).map(Some)
        } else {
            Some(None)
        }
    }) else {
        return Ok(None);
    };
    let Ok(colour_scale) = get(intern!(py, "colour_scale"))?.extract::<f64>() else {
        return Ok(None);
    };
    let truth = |n: &Bound<'py, PyString>| -> PyResult<bool> { get(n)?.is_truthy() };
    let Some(material_regs) = dict_of(&get(intern!(py, "material_regs"))?, key_int, |v| numbers(v).map(Some)) else {
        return Ok(None);
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
    let (Some(sampler_registers), Some(fetch_registers)) = (
        ints(intern!(py, "sampler_registers"))?,
        ints(intern!(py, "fetch_registers"))?,
    ) else {
        return Ok(None);
    };
    let (Some(named_vs), Some(named_ps)) = (names(intern!(py, "named_vs"))?, names(intern!(py, "named_ps"))?) else {
        return Ok(None);
    };
    let probe_v = get(intern!(py, "probe"))?;
    let probe = if probe_v.is_truthy()? {
        let Some(p) = seq(&probe_v).and_then(|s| s.iter().map(exact_str).collect::<Option<Vec<_>>>()) else {
            return Ok(None);
        };
        Some(p)
    } else {
        None
    };
    Ok(Some(Interface {
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

/// (status, text): 0 the HLSL, 1 a `TranslateError` and its message, 2 left to the Python.
fn status(r: Result<String, TranslateError>) -> (u8, String) {
    match r {
        Ok(t) => (0, t),
        Err(TranslateError::Translate(m)) => (1, m),
        Err(e) => (2, e.to_string()),
    }
}

/// `xenos_hlsl.translate(program, iface)` through Rust: `(status, text)` - 0 the HLSL text, 1 a `TranslateError`'s
/// message, 2 left to the Python (an interface value of another type than the translator reads, or a program whose
/// translation the Python would end with another exception).
#[pyfunction]
pub fn xenos_translate(
    py: Python<'_>,
    program: &Bound<'_, XenosProgram>,
    iface: &Bound<'_, PyAny>,
) -> PyResult<(u8, String)> {
    let Some(iface) = interface(iface)? else {
        return Ok((2, "interface".into()));
    };
    let sh = Arc::clone(&program.get().shader);
    Ok(py.detach(|| status(hlsl::translate(&sh, &iface))))
}

/// [`xenos_translate`] of many `(program, iface)` on all cores, in input order.
#[pyfunction]
pub fn xenos_translate_many<'py>(
    py: Python<'py>,
    items: Vec<(Bound<'py, XenosProgram>, Bound<'py, PyAny>)>,
) -> PyResult<Vec<(u8, String)>> {
    let mut jobs: Vec<Option<(Arc<Shader>, Interface)>> = Vec::with_capacity(items.len());
    for (p, i) in &items {
        jobs.push(interface(i)?.map(|iface| (Arc::clone(&p.get().shader), iface)));
    }
    Ok(py.detach(|| {
        use rayon::prelude::*;
        jobs.par_iter()
            .map(|j| match j {
                Some((sh, iface)) => status(hlsl::translate(sh, iface)),
                None => (2, "interface".into()),
            })
            .collect()
    }))
}

/// `repr(float(x))` as the translator writes numbers (the test of the Python repr rules).
#[pyfunction]
pub fn xenos_float_repr(x: f64) -> String {
    bc2_shader::pyrepr::float_repr(x)
}
