//! The XDK shader container of Frostbite's Xbox 360 shader databases and the Xenos microcode inside it: a port of
//! `bfbc2-shader-tools/xenos.py`'s `parse_shader` (the format and its open questions: `bfbc2-shader-tools/XENOS.md`).
//!
//! Every field `xenos.py` decodes is here under its Python name, with its Python value: an integer field stays an
//! integer where Python keeps `_bits`' integer (`is_yield`, `condition` ...) and a `bool` where Python makes one, so
//! the bindings can build `xenos.py`'s exact tree from it. The decoder fails wherever `xenos.py` raises (a
//! [`XenosError::Format`] where it raises `XenosError`, a [`XenosError::Malformed`] where Python's own `struct.error`,
//! `IndexError` or `ValueError` would end it) and never panics.
//!
//! ```
//! // a pixel shader with no constant table, no literals, one `exec_end` of one ALU instruction
//! let mut b = vec![0u8; 0x24 + 32 + 4 * 6];
//! let put = |b: &mut Vec<u8>, o: usize, v: u32| b[o..o + 4].copy_from_slice(&v.to_be_bytes());
//! put(&mut b, 0, 0x102a_1100);
//! put(&mut b, 4, 0x24 + 32);           // virtual size: the container and the header
//! put(&mut b, 8, 24);                  // physical size: the microcode
//! put(&mut b, 0x18, 0x24);             // header offset
//! put(&mut b, 0x24 + 4, 24);           // the header's physical size
//! put(&mut b, 0x24 + 32, 0x0000_1001); // CF 0: exec_end ADDR(1) CNT(1)
//! put(&mut b, 0x24 + 36, 0x2000);
//! let sh = bc2_shader::xenos::parse(&b, None)?;
//! assert!(sh.is_pixel);
//! assert_eq!(sh.cf.len(), 1);
//! assert_eq!(sh.instructions.len(), 1);
//! # Ok::<(), bc2_shader::xenos::XenosError>(())
//! ```

use thiserror::Error;

/// Why a blob is not a shader `xenos.py` decodes.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum XenosError {
    /// What `xenos.py` raises as `XenosError`, with its message.
    #[error("{0}")]
    Format(String),
    /// Where `xenos.py` stops on another exception (a short read of `struct`, an index past a list, a string
    /// without its terminator): the bindings let the Python decoder raise its own.
    #[error("malformed shader: {0}")]
    Malformed(String),
}

type Result<T> = std::result::Result<T, XenosError>;

/// `CF_OPS`: control-flow opcode names.
pub const CF_OPS: [&str; 16] = [
    "nop",
    "exec",
    "exec_end",
    "cond_exec",
    "cond_exec_end",
    "cond_exec_pred",
    "cond_exec_pred_end",
    "loop_start",
    "loop_end",
    "cond_call",
    "return",
    "cond_jmp",
    "alloc",
    "cond_exec_pred_clean",
    "cond_exec_pred_clean_end",
    "mark_vs_fetch_done",
];

/// `SCALAR_OPS`: (name, operand count, a single operand reads W and X).
pub const SCALAR_OPS: [(&str, u8, bool); 64] = [
    ("adds", 1, true),
    ("adds_prev", 1, false),
    ("muls", 1, true),
    ("muls_prev", 1, false),
    ("muls_prev2", 1, true),
    ("maxs", 1, true),
    ("mins", 1, true),
    ("seqs", 1, false),
    ("sgts", 1, false),
    ("sges", 1, false),
    ("snes", 1, false),
    ("frcs", 1, false),
    ("truncs", 1, false),
    ("floors", 1, false),
    ("exp", 1, false),
    ("logc", 1, false),
    ("log", 1, false),
    ("rcpc", 1, false),
    ("rcpf", 1, false),
    ("rcp", 1, false),
    ("rsqc", 1, false),
    ("rsqf", 1, false),
    ("rsq", 1, false),
    ("maxas", 1, true),
    ("maxasf", 1, true),
    ("subs", 1, true),
    ("subs_prev", 1, false),
    ("setp_eq", 1, false),
    ("setp_ne", 1, false),
    ("setp_gt", 1, false),
    ("setp_ge", 1, false),
    ("setp_inv", 1, false),
    ("setp_pop", 1, false),
    ("setp_clr", 0, false),
    ("setp_rstr", 1, false),
    ("kills_eq", 1, false),
    ("kills_gt", 1, false),
    ("kills_ge", 1, false),
    ("kills_ne", 1, false),
    ("kills_one", 1, false),
    ("sqrt", 1, false),
    ("opcode_41", 0, false),
    ("mulsc", 2, false),
    ("mulsc", 2, false),
    ("addsc", 2, false),
    ("addsc", 2, false),
    ("subsc", 2, false),
    ("subsc", 2, false),
    ("sin", 1, false),
    ("cos", 1, false),
    ("retain_prev", 0, false),
    ("opcode_51", 0, false),
    ("opcode_52", 0, false),
    ("opcode_53", 0, false),
    ("opcode_54", 0, false),
    ("opcode_55", 0, false),
    ("opcode_56", 0, false),
    ("opcode_57", 0, false),
    ("opcode_58", 0, false),
    ("opcode_59", 0, false),
    ("opcode_60", 0, false),
    ("opcode_61", 0, false),
    ("opcode_62", 0, false),
    ("opcode_63", 0, false),
];

/// `VECTOR_OPS`: (name, operand count).
pub const VECTOR_OPS: [(&str, usize); 32] = [
    ("add", 2),
    ("mul", 2),
    ("max", 2),
    ("min", 2),
    ("seq", 2),
    ("sgt", 2),
    ("sge", 2),
    ("sne", 2),
    ("frc", 1),
    ("trunc", 1),
    ("floor", 1),
    ("mad", 3),
    ("cndeq", 3),
    ("cndge", 3),
    ("cndgt", 3),
    ("dp4", 2),
    ("dp3", 2),
    ("dp2add", 3),
    ("cube", 2),
    ("max4", 1),
    ("setp_eq_push", 2),
    ("setp_ne_push", 2),
    ("setp_gt_push", 2),
    ("setp_ge_push", 2),
    ("kill_eq", 2),
    ("kill_gt", 2),
    ("kill_ge", 2),
    ("kill_ne", 2),
    ("dst", 2),
    ("maxa", 2),
    ("opcode_30", 0),
    ("opcode_31", 0),
];

const TEX_FILTER: [&str; 4] = ["point", "linear", "basemap", "keep"];
const ANISO_FILTER: [&str; 8] = [
    "disabled", "max1to1", "max2to1", "max4to1", "max8to1", "max16to1", "aniso6", "keep",
];
const ARB_FILTER: [&str; 8] = [
    "2x4sym", "2x4asym", "4x2sym", "4x2asym", "4x4sym", "4x4asym", "arb6", "keep",
];
const DIMENSIONS: [&str; 4] = ["1D", "2D", "3D", "Cube"];
/// `DECL_USAGE`: vertex element / interpolator usages.
pub const DECL_USAGE: [&str; 16] = [
    "POSITION",
    "BLENDWEIGHT",
    "BLENDINDICES",
    "NORMAL",
    "PSIZE",
    "TEXCOORD",
    "TANGENT",
    "BINORMAL",
    "TESSFACTOR",
    "POSITIONT",
    "COLOR",
    "FOG",
    "DEPTH",
    "SAMPLE",
    "usage14",
    "usage15",
];
const REGSETS: [&str; 4] = ["bool", "int4", "float4", "sampler"];
const PARAM_CLASS: [&str; 6] = ["scalar", "vector", "matrix_rows", "matrix_columns", "object", "struct"];
const PARAM_TYPE: [&str; 20] = [
    "void",
    "bool",
    "int",
    "float",
    "string",
    "texture",
    "texture1D",
    "texture2D",
    "texture3D",
    "textureCube",
    "sampler",
    "sampler1D",
    "sampler2D",
    "sampler3D",
    "samplerCube",
    "pixelshader",
    "vertexshader",
    "pixelfragment",
    "vertexfragment",
    "unsupported",
];
const XYZW: [char; 4] = ['x', 'y', 'z', 'w'];

/// `_bits(v, lo, n)`.
#[must_use]
pub const fn bits(v: u32, lo: u32, n: u32) -> u32 {
    (v >> lo) & ((1 << n) - 1)
}

/// `_sbits(v, lo, n)`: the field sign-extended.
#[must_use]
pub const fn sbits(v: u32, lo: u32, n: u32) -> i32 {
    let x = bits(v, lo, n);
    if x & (1 << (n - 1)) != 0 {
        x as i32 - (1 << n)
    } else {
        x as i32
    }
}

/// A table name or, past the table, the raw number (`PARAM_CLASS[x] if x < 6 else x`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Named {
    /// The table's name.
    Name(&'static str),
    /// The number: not in the table.
    Number(u32),
}

impl Named {
    fn of(table: &[&'static str], v: u32) -> Self {
        table.get(v as usize).map_or(Self::Number(v), |n| Self::Name(n))
    }

    /// Is this the name `n`?
    #[must_use]
    pub fn is(self, n: &str) -> bool {
        matches!(self, Self::Name(m) if m == n)
    }
}

/// A constant's type (`_parse_type`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamType {
    /// `cls`.
    pub cls: Named,
    /// `type`.
    pub type_: Named,
    /// `rows`.
    pub rows: u16,
    /// `cols`.
    pub cols: u16,
    /// `elements`.
    pub elements: u16,
    /// `members`: (name, type).
    pub members: Vec<(String, ParamType)>,
}

/// A constant table entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Constant {
    /// `name`.
    pub name: String,
    /// `regset`.
    pub regset: Named,
    /// `reg`.
    pub reg: u16,
    /// `count`.
    pub count: u16,
    /// `reserved`.
    pub reserved: u16,
    /// `type`.
    pub type_: ParamType,
    /// `default_offset`.
    pub default_offset: u32,
}

/// The constant table (`_parse_ctab`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ctab {
    /// `offset`.
    pub offset: u32,
    /// `size`.
    pub size: u32,
    /// `base`.
    pub base: u64,
    /// `creator`.
    pub creator: String,
    /// `version`.
    pub version: u32,
    /// `target`.
    pub target: String,
    /// `flags`.
    pub flags: u32,
    /// `constants`.
    pub constants: Vec<Constant>,
}

/// One `def` run of the definition table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FloatDef {
    /// `reg`.
    pub reg: u32,
    /// `count`.
    pub count: u32,
    /// `physical_offset`.
    pub physical_offset: u32,
}

/// One `defi` run of the definition table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntDef {
    /// `reg`.
    pub reg: u32,
    /// `count`.
    pub count: u32,
    /// `values`.
    pub values: Vec<u32>,
}

/// The literal-constant definition table (`_parse_definitions`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefTable {
    /// `offset`.
    pub offset: u32,
    /// `field0`, `field4`, `field8`, `fieldC`.
    pub fields: [u32; 4],
    /// `size`.
    pub size: u32,
    /// `raw_words`.
    pub raw_words: Vec<u32>,
    /// `float_defs`.
    pub float_defs: Vec<FloatDef>,
    /// `int_defs`.
    pub int_defs: Vec<IntDef>,
    /// `bool_words`.
    pub bool_words: Vec<u32>,
}

/// A vertex element of a vertex shader's header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VertexElement {
    /// `raw`.
    pub raw: u32,
    /// `fetch_address`.
    pub fetch_address: u32,
    /// `usage`.
    pub usage: &'static str,
    /// `usage_index`.
    pub usage_index: u32,
    /// `unknown_hi`.
    pub unknown_hi: u32,
}

/// An interpolator of the header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interpolator {
    /// `raw`.
    pub raw: u32,
    /// `usage_index`.
    pub usage_index: u32,
    /// `usage`.
    pub usage: &'static str,
    /// `reg`.
    pub reg: u32,
    /// `mask`.
    pub mask: u32,
    /// `export_index`.
    pub export_index: u32,
    /// `export_addresses` (vertex shaders only).
    pub export_addresses: Option<Vec<u32>>,
}

/// An export slot of a vertex shader's header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportSlot {
    /// `raw`.
    pub raw: u32,
    /// `address`.
    pub address: u32,
    /// `last`.
    pub last: u32,
    /// `unknown_hi`.
    pub unknown_hi: u32,
}

/// The stage-specific part of the header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    /// A pixel shader's: `interp_mask`, `outputs`.
    Pixel {
        /// `interp_mask`.
        interp_mask: u32,
        /// `outputs`: bit k = oCk, bit 4 = oDepth.
        outputs: u32,
    },
    /// A vertex shader's.
    Vertex {
        /// `field18`.
        field18: u32,
        /// `vertex_element_count`.
        vertex_element_count: u32,
        /// `field20`.
        field20: u32,
        /// `field18_words`.
        field18_words: Vec<u32>,
    },
}

/// The shader header (`_parse_header`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// `offset`.
    pub offset: u32,
    /// `physical_offset`.
    pub physical_offset: u32,
    /// `physical_size`.
    pub physical_size: u32,
    /// `program_cntl`.
    pub program_cntl: u32,
    /// `context_misc`.
    pub context_misc: u32,
    /// `field10`.
    pub field10: u32,
    /// `interpolator_info`.
    pub interpolator_info: u32,
    /// `num_temp_regs`.
    pub num_temp_regs: u32,
    /// `interpolator_count`.
    pub interpolator_count: u32,
    /// `vertex_elements`.
    pub vertex_elements: Vec<VertexElement>,
    /// `interpolators`.
    pub interpolators: Vec<Interpolator>,
    /// the stage's own fields.
    pub stage: Stage,
    /// `export_slots`.
    pub export_slots: Vec<ExportSlot>,
    /// `end`.
    pub end: u64,
}

impl Header {
    /// `program_cntl_fields` (name, value) in `xenos.py`'s order.
    #[must_use]
    pub fn program_cntl_fields(&self) -> [(&'static str, u32); 10] {
        let pc = self.program_cntl;
        [
            ("vs_num_reg", bits(pc, 0, 6)),
            ("ps_num_reg", bits(pc, 8, 6)),
            ("vs_resource", bits(pc, 16, 1)),
            ("ps_resource", bits(pc, 17, 1)),
            ("param_gen", bits(pc, 18, 1)),
            ("gen_index_pix", bits(pc, 19, 1)),
            ("vs_export_count", bits(pc, 20, 4)),
            ("vs_export_mode", bits(pc, 24, 3)),
            ("ps_export_mode", bits(pc, 27, 4)),
            ("gen_index_vtx", bits(pc, 31, 1)),
        ]
    }

    /// `context_misc_fields` (name, value) in `xenos.py`'s order.
    #[must_use]
    pub fn context_misc_fields(&self) -> [(&'static str, u32); 4] {
        let cm = self.context_misc;
        [
            ("inst_pred_optimize", bits(cm, 0, 1)),
            ("sc_output_screen_xy", bits(cm, 1, 1)),
            ("sc_sample_cntl", bits(cm, 2, 2)),
            ("param_gen_pos", bits(cm, 8, 8)),
        ]
    }

    /// `context_misc_fields.sc_output_screen_xy`.
    #[must_use]
    pub fn sc_output_screen_xy(&self) -> bool {
        bits(self.context_misc, 1, 1) != 0
    }

    /// `context_misc_fields.param_gen_pos`.
    #[must_use]
    pub fn param_gen_pos(&self) -> u32 {
        bits(self.context_misc, 8, 8)
    }

    /// A pixel shader's `outputs` (0 for a vertex shader).
    #[must_use]
    pub fn outputs(&self) -> u32 {
        match self.stage {
            Stage::Pixel { outputs, .. } => outputs,
            Stage::Vertex { .. } => 0,
        }
    }
}

/// How an exec CF runs its block (`cf.type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CfType {
    /// `'unconditional'`.
    Unconditional,
    /// `'conditional'` (on a bool constant).
    Conditional,
    /// `'predicated'` (on p0).
    Predicated,
}

impl CfType {
    /// The Python name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Unconditional => "unconditional",
            Self::Conditional => "conditional",
            Self::Predicated => "predicated",
        }
    }
}

/// A control-flow instruction's own fields, by opcode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CfKind {
    /// `exec` / `cond_exec*` (opcodes 1-6, 13, 14).
    Exec {
        /// `address`.
        address: u32,
        /// `count`.
        count: u32,
        /// `is_yield`.
        is_yield: u32,
        /// `sequence`.
        sequence: u32,
        /// `vc`.
        vc: u32,
        /// `is_end`.
        is_end: bool,
        /// `type` (its key comes before `bool_index` / `condition`).
        type_: CfType,
        /// `bool_index` (3, 4: before `condition`; 13, 14: last).
        bool_index: Option<u32>,
        /// `condition` (3-6, 13, 14).
        condition: Option<u32>,
        /// `is_predicate_clean`.
        is_predicate_clean: bool,
    },
    /// `loop_start` (7).
    LoopStart {
        /// `address`.
        address: u32,
        /// `is_repeat`.
        is_repeat: u32,
        /// `loop_id`.
        loop_id: u32,
    },
    /// `loop_end` (8).
    LoopEnd {
        /// `address`.
        address: u32,
        /// `loop_id`.
        loop_id: u32,
        /// `is_predicated_break`.
        is_predicated_break: u32,
        /// `condition`.
        condition: u32,
    },
    /// `cond_call` (9) / `cond_jmp` (11).
    Jump {
        /// `address`.
        address: u32,
        /// `is_unconditional`.
        is_unconditional: u32,
        /// `is_predicated`.
        is_predicated: u32,
        /// `bool_index`.
        bool_index: u32,
        /// `condition`.
        condition: u32,
        /// `type`.
        type_: CfType,
        /// `direction` (`cond_jmp` only).
        direction: Option<u32>,
    },
    /// alloc (12).
    Alloc {
        /// `size`.
        size: u32,
        /// `is_unserialized`.
        is_unserialized: u32,
        /// `alloc_type`.
        alloc_type: u32,
    },
    /// `nop`, `return`, `mark_vs_fetch_done`: no fields.
    Plain,
}

/// A control-flow instruction (`_decode_cf`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cf {
    /// `index`.
    pub index: u32,
    /// `opcode`.
    pub opcode: u32,
    /// `address_mode`.
    pub address_mode: u32,
    /// `raw` (w0, w1).
    pub raw: (u32, u32),
    /// the opcode's fields.
    pub kind: CfKind,
    /// `instructions` (exec CFs): indexes into [`Shader::instructions`], in execution order.
    pub instructions: Option<Vec<usize>>,
}

impl Cf {
    /// `op`.
    #[must_use]
    pub fn op(&self) -> &'static str {
        CF_OPS.get(self.opcode as usize).copied().unwrap_or("?")
    }
}

/// A register file of an operand or destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum File {
    /// `'temp'`.
    Temp,
    /// `'const'`.
    Const,
    /// an export target (`'oPos'`, `'o'`, `'oC'`, `'eA'`, `'eM'`, `'oDepth'`, `'oPts'`, `'export?'`).
    Export(&'static str),
}

impl File {
    /// The Python name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Temp => "temp",
            Self::Const => "const",
            Self::Export(n) => n,
        }
    }
}

/// The address register of relative addressing (`rel`).
pub type Rel = Option<&'static str>;

/// An ALU source operand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operand {
    /// a mulsc / addsc / subsc operand (`kind, file, index, rel, negate, abs, swizzle_raw, components`); else
    /// `_alu_operand`'s (`kind, negate, swizzle_raw, swizzle, file, index, rel, abs[, components]`).
    pub two_op: bool,
    /// `negate`.
    pub negate: bool,
    /// `swizzle_raw`.
    pub swizzle_raw: u32,
    /// `swizzle` (`_alu_operand` only).
    pub swizzle: Option<String>,
    /// `file`.
    pub file: File,
    /// `index`.
    pub index: u32,
    /// `rel`.
    pub rel: Rel,
    /// `abs`.
    pub abs: bool,
    /// `components` (scalar operands).
    pub components: Option<String>,
}

impl Operand {
    /// The swizzle (`_alu_operand`'s; empty for a two-operand scalar's, which has only `components`).
    #[must_use]
    pub fn swizzle(&self) -> &str {
        self.swizzle.as_deref().unwrap_or("")
    }
}

/// An ALU destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dest {
    /// `file`.
    pub file: File,
    /// `index` (None: an export target without one).
    pub index: Option<u32>,
    /// `rel`.
    pub rel: Rel,
    /// `raw_index` (exports only).
    pub raw_index: Option<u32>,
    /// `write`: per component 'x'..'w', '0', '1' or '_'.
    pub write: String,
    /// `saturate`.
    pub saturate: bool,
}

impl Dest {
    /// `not d.write.strip('_')`.
    #[must_use]
    pub fn writes_nothing(&self) -> bool {
        self.write.bytes().all(|c| c == b'_')
    }
}

/// An ALU instruction (`_decode_alu`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alu {
    /// `predicated`.
    pub predicated: bool,
    /// `pred_condition`.
    pub pred_condition: bool,
    /// `is_export`.
    pub is_export: bool,
    /// `abs_constants`.
    pub abs_constants: bool,
    /// `const_addr_reg_relative`.
    pub const_addr_reg_relative: bool,
    /// `const_0_rel`.
    pub const_0_rel: bool,
    /// `const_1_rel`.
    pub const_1_rel: bool,
    /// `vector_opcode`.
    pub vector_opcode: u32,
    /// `scalar_opcode`.
    pub scalar_opcode: u32,
    /// `vector_write_mask_raw`.
    pub vector_write_mask_raw: u32,
    /// `scalar_write_mask_raw`.
    pub scalar_write_mask_raw: u32,
    /// `vector_dest`.
    pub vector_dest: Dest,
    /// `scalar_dest`.
    pub scalar_dest: Dest,
    /// `vector_operands`.
    pub vector_operands: Vec<Operand>,
    /// `scalar_operands`.
    pub scalar_operands: Vec<Operand>,
    /// `vector_nop`.
    pub vector_nop: bool,
    /// `scalar_nop`.
    pub scalar_nop: bool,
}

impl Alu {
    /// `vector_op`.
    #[must_use]
    pub fn vector_op(&self) -> &'static str {
        VECTOR_OPS.get(self.vector_opcode as usize).map_or("?", |o| o.0)
    }

    /// `scalar_op`.
    #[must_use]
    pub fn scalar_op(&self) -> &'static str {
        SCALAR_OPS.get(self.scalar_opcode as usize).map_or("?", |o| o.0)
    }
}

/// A vertex fetch's own fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VertexFetch {
    /// `must_be_one`.
    pub must_be_one: u32,
    /// `const_index`.
    pub const_index: u32,
    /// `const_index_sel`.
    pub const_index_sel: u32,
    /// `prefetch_count`.
    pub prefetch_count: u32,
    /// `src_swizzle`.
    pub src_swizzle: char,
    /// `is_signed`.
    pub is_signed: bool,
    /// `is_integer`.
    pub is_integer: bool,
    /// `signed_rf_mode`.
    pub signed_rf_mode: u32,
    /// `is_index_rounded`.
    pub is_index_rounded: bool,
    /// `format`.
    pub format: u32,
    /// `exp_adjust`.
    pub exp_adjust: i32,
    /// `is_mini`.
    pub is_mini: bool,
    /// `stride`.
    pub stride: u32,
    /// `offset`.
    pub offset: i32,
    /// `fetch_const`.
    pub fetch_const: u32,
}

impl VertexFetch {
    /// `format_name`.
    #[must_use]
    pub fn format_name(&self) -> String {
        let n = match self.format {
            6 => "FMT_8_8_8_8",
            7 => "FMT_2_10_10_10",
            16 => "FMT_10_11_11",
            17 => "FMT_11_11_10",
            25 => "FMT_16_16",
            26 => "FMT_16_16_16_16",
            31 => "FMT_16_16_FLOAT",
            32 => "FMT_16_16_16_16_FLOAT",
            33 => "FMT_32",
            34 => "FMT_32_32",
            35 => "FMT_32_32_32_32",
            36 => "FMT_32_FLOAT",
            37 => "FMT_32_32_FLOAT",
            38 => "FMT_32_32_32_32_FLOAT",
            57 => "FMT_32_32_32_FLOAT",
            f => return format!("FMT_{f}"),
        };
        n.to_owned()
    }

    /// `vf`: the fetch constant in Direct3D's numbering.
    #[must_use]
    pub fn vf(&self) -> i64 {
        95 - i64::from(self.fetch_const)
    }
}

/// A texture fetch's own fields.
#[derive(Debug, Clone, PartialEq)]
pub struct TextureFetch {
    /// `fetch_valid_only`.
    pub fetch_valid_only: bool,
    /// `const_index`.
    pub const_index: u32,
    /// `unnormalized`.
    pub unnormalized: bool,
    /// `src_swizzle_raw`.
    pub src_swizzle_raw: u32,
    /// `mag_filter`, `min_filter`, `mip_filter`.
    pub mag_filter: &'static str,
    /// `min_filter`.
    pub min_filter: &'static str,
    /// `mip_filter`.
    pub mip_filter: &'static str,
    /// `aniso_filter`.
    pub aniso_filter: &'static str,
    /// `arbitrary_filter`.
    pub arbitrary_filter: &'static str,
    /// `vol_mag_filter`.
    pub vol_mag_filter: &'static str,
    /// `vol_min_filter`.
    pub vol_min_filter: &'static str,
    /// `use_computed_lod`.
    pub use_computed_lod: bool,
    /// `use_register_lod`.
    pub use_register_lod: bool,
    /// `unk`.
    pub unk: u32,
    /// `use_register_gradients`.
    pub use_register_gradients: bool,
    /// `sample_location`.
    pub sample_location: &'static str,
    /// `lod_bias`.
    pub lod_bias: f64,
    /// `unused`.
    pub unused: u32,
    /// `dimension`.
    pub dimension: &'static str,
    /// `offset_x`.
    pub offset_x: f64,
    /// `offset_y`.
    pub offset_y: f64,
    /// `offset_z`.
    pub offset_z: f64,
    /// `src_swizzle`.
    pub src_swizzle: String,
    /// `has_dest`.
    pub has_dest: bool,
    /// `has_const`.
    pub has_const: bool,
}

/// A fetch's kind-specific fields.
#[derive(Debug, Clone, PartialEq)]
pub enum FetchKind {
    /// `fetch_kind == 'vertex'`.
    Vertex(VertexFetch),
    /// `fetch_kind == 'texture'`.
    Texture(TextureFetch),
}

/// A fetch instruction (`_decode_fetch`).
#[derive(Debug, Clone, PartialEq)]
pub struct Fetch {
    /// `opcode`.
    pub opcode: u32,
    /// `op` (its final value).
    pub op: String,
    /// `src`.
    pub src: u32,
    /// `src_rel`.
    pub src_rel: Rel,
    /// `dst`.
    pub dst: u32,
    /// `dst_rel`.
    pub dst_rel: Rel,
    /// `dst_swizzle`.
    pub dst_swizzle: String,
    /// `predicated`.
    pub predicated: bool,
    /// `pred_condition`.
    pub pred_condition: bool,
    /// the kind's own fields.
    pub kind: FetchKind,
}

impl Fetch {
    /// The first value `op` took (`FETCH_OPS.get(op, 'fetch_%d' % op)`), whose key position the tree keeps.
    #[must_use]
    pub fn first_op(&self) -> String {
        match self.opcode {
            0 => "vfetch".into(),
            1 => "tfetch".into(),
            16 => "getBCF".into(),
            17 => "getCompTexLOD".into(),
            18 => "getGradients".into(),
            19 => "getWeights".into(),
            24 => "setTexLOD".into(),
            25 => "setGradientH".into(),
            26 => "setGradientV".into(),
            o => format!("fetch_{o}"),
        }
    }
}

/// An instruction's kind.
#[derive(Debug, Clone, PartialEq)]
pub enum InstrKind {
    /// ALU.
    Alu(Alu),
    /// Fetch.
    Fetch(Fetch),
}

/// An instruction slot executed by a CF.
#[derive(Debug, Clone, PartialEq)]
pub struct Instr {
    /// `address`.
    pub address: u32,
    /// `raw`.
    pub raw: [u32; 3],
    /// the decoded fields.
    pub kind: InstrKind,
    /// `serialize`.
    pub serialize: bool,
    /// `cf_index`.
    pub cf_index: u32,
}

/// A decoded shader (`parse_shader`'s tree).
#[derive(Debug, Clone, PartialEq)]
pub struct Shader {
    /// `is_pixel`.
    pub is_pixel: bool,
    /// `magic`.
    pub magic: u32,
    /// `virtual_size`, `physical_size`, `fieldC`, `ctab_offset`, `def_offset`, `header_offset`, `field1C`,
    /// `field20`: the container's words 1-8.
    pub container: [u32; 8],
    /// `ctab`.
    pub ctab: Option<Ctab>,
    /// `literals`: register -> 4 floats, in the dict's insertion order.
    pub literals: Vec<(i64, [f32; 4])>,
    /// `int_literals`: register -> 4 ints, in insertion order.
    pub int_literals: Vec<(i64, [i32; 4])>,
    /// `def_table`.
    pub def_table: Option<DefTable>,
    /// `header`.
    pub header: Header,
    /// `layout_gaps`.
    pub layout_gaps: Vec<(u64, u64)>,
    /// `code_offset`.
    pub code_offset: u64,
    /// `code_size`.
    pub code_size: u32,
    /// `cf`.
    pub cf: Vec<Cf>,
    /// `instructions`, by address.
    pub instructions: Vec<Instr>,
    /// `cf_slots`.
    pub cf_slots: u32,
    /// `unused_slots`.
    pub unused_slots: Vec<u32>,
    /// `unused_nonzero_slots`.
    pub unused_nonzero_slots: Vec<u32>,
    /// `code_words`.
    pub code_words: Vec<u32>,
}

impl Shader {
    /// The constant table's constants (none without a table).
    #[must_use]
    pub fn constants(&self) -> &[Constant] {
        self.ctab.as_ref().map_or(&[], |c| &c.constants)
    }
}

// ------------------------------------------------------------------------------------------------- reading

struct Reader<'a> {
    b: &'a [u8],
}

fn malformed(what: &str, o: u64) -> XenosError {
    XenosError::Malformed(format!("{what} at 0x{o:x}"))
}

impl Reader<'_> {
    fn slice<const N: usize>(&self, o: u64) -> Option<[u8; N]> {
        let o = usize::try_from(o).ok()?;
        self.b.get(o..o.checked_add(N)?)?.try_into().ok()
    }

    /// `_Reader.u32`: `XenosError` past the end.
    fn u32(&self, o: u64) -> Result<u32> {
        self.slice::<4>(o)
            .map(u32::from_be_bytes)
            .ok_or_else(|| XenosError::Format(format!("read past end at 0x{o:x}")))
    }

    /// `_Reader.u16`: Python's struct.error past the end.
    fn u16(&self, o: u64) -> Result<u16> {
        self.slice::<2>(o)
            .map(u16::from_be_bytes)
            .ok_or_else(|| malformed("u16 past the end", o))
    }

    fn f32(&self, o: u64) -> Result<f32> {
        self.slice::<4>(o)
            .map(f32::from_be_bytes)
            .ok_or_else(|| malformed("f32 past the end", o))
    }

    /// `_Reader.cstr`: latin-1 up to the terminator; Python's `ValueError` without one.
    fn cstr(&self, o: u64) -> Result<String> {
        let tail = usize::try_from(o)
            .ok()
            .and_then(|o| self.b.get(o..))
            .ok_or_else(|| malformed("string", o))?;
        let end = tail
            .iter()
            .position(|&c| c == 0)
            .ok_or_else(|| malformed("unterminated string", o))?;
        Ok(tail.iter().take(end).map(|&c| char::from(c)).collect())
    }
}

fn dict_set<K: PartialEq, V>(d: &mut Vec<(K, V)>, k: K, v: V) {
    if let Some(e) = d.iter_mut().find(|e| e.0 == k) {
        e.1 = v;
    } else {
        d.push((k, v));
    }
}

fn parse_type(r: &Reader, base: u64, off: u64, depth: u32) -> Result<ParamType> {
    let o = base + off;
    let cls = Named::of(&PARAM_CLASS, u32::from(r.u16(o)?));
    let type_ = Named::of(&PARAM_TYPE, u32::from(r.u16(o + 2)?));
    let (rows, cols, elements) = (r.u16(o + 4)?, r.u16(o + 6)?, r.u16(o + 8)?);
    let nmem = r.u16(o + 10)?;
    let moff = r.u32(o + 12)?;
    let mut members = Vec::new();
    if nmem != 0 && depth < 8 {
        for k in 0..u64::from(nmem) {
            let mo = base + u64::from(moff) + 8 * k;
            let name = r.cstr(base + u64::from(r.u32(mo)?))?;
            let t = parse_type(r, base, u64::from(r.u32(mo + 4)?), depth + 1)?;
            members.push((name, t));
        }
    }
    Ok(ParamType {
        cls,
        type_,
        rows,
        cols,
        elements,
        members,
    })
}

fn parse_ctab(r: &Reader, off: u32) -> Result<Ctab> {
    let o = u64::from(off);
    let size = r.u32(o)?;
    let base = o + 4;
    let mut hdr = [0u32; 7];
    for (k, h) in (0u64..).zip(hdr.iter_mut()) {
        *h = r.u32(base + 4 * k)?;
    }
    let [h0, h1, h2, h3, h4, h5, h6] = hdr;
    if h0 != 0x1c {
        return Err(XenosError::Format(format!("CTAB header size {h0:#x}")));
    }
    let creator = r.cstr(base + u64::from(h1))?;
    let target = r.cstr(base + u64::from(h6))?;
    let mut constants = Vec::new();
    for k in 0..u64::from(h3) {
        let o = base + u64::from(h4) + 20 * k;
        let rs = r.u16(o + 4)?;
        let ti = r.u32(o + 12)?;
        let dv = r.u32(o + 16)?;
        let name = r.cstr(base + u64::from(r.u32(o)?))?;
        let regset = Named::of(&REGSETS, u32::from(rs));
        let (reg, count, reserved) = (r.u16(o + 6)?, r.u16(o + 8)?, r.u16(o + 10)?);
        let type_ = parse_type(r, base, u64::from(ti), 0)?;
        constants.push(Constant {
            name,
            regset,
            reg,
            count,
            reserved,
            type_,
            default_offset: dv,
        });
    }
    Ok(Ctab {
        offset: off,
        size,
        base,
        creator,
        version: h2,
        target,
        flags: h5,
        constants,
    })
}

type Literals = Vec<(i64, [f32; 4])>;
type IntLiterals = Vec<(i64, [i32; 4])>;

fn parse_definitions(r: &Reader, off: u32, vsize: u32, is_pixel: bool) -> Result<(DefTable, Literals, IntLiterals)> {
    let o = u64::from(off);
    let mut h = [0u32; 5];
    for (k, x) in (0u64..).zip(h.iter_mut()) {
        *x = r.u32(o + 4 * k)?;
    }
    let size = h[4];
    let words = (0..u64::from(size / 4))
        .map(|k| r.u32(o + 20 + 4 * k))
        .collect::<Result<Vec<u32>>>()?;
    let word = |p: usize| {
        words
            .get(p)
            .copied()
            .ok_or_else(|| malformed("definition word past the table", p as u64))
    };
    let mut floats: Literals = Vec::new();
    let mut ints: IntLiterals = Vec::new();
    let mut float_defs = Vec::new();
    let mut int_defs = Vec::new();
    let mut p = 0usize;
    while p < words.len() && word(p)? != 0 {
        let w = word(p)?;
        let (reg, cnt, phys) = (w >> 16, w & 0xffff, word(p + 1)?);
        float_defs.push(FloatDef {
            reg,
            count: cnt,
            physical_offset: phys,
        });
        let base_reg = i64::from(reg) - if is_pixel { 256 } else { 0 };
        for v in 0..u64::from(cnt.div_ceil(4)) {
            let fo = u64::from(vsize) + u64::from(phys) + 16 * v;
            let vals = [r.f32(fo)?, r.f32(fo + 4)?, r.f32(fo + 8)?, r.f32(fo + 12)?];
            dict_set(&mut floats, base_reg + v as i64, vals);
        }
        p += 2;
    }
    p += 1;
    while p < words.len() && word(p)? != 0 {
        let w = word(p)?;
        let (reg, cnt) = (w >> 16, w & 0xffff);
        let start = (p + 1).min(words.len());
        let end = (p + 1 + cnt as usize).min(words.len());
        let vals: Vec<u32> = words.get(start..end).map_or_default(<[u32]>::to_vec);
        let first = (i64::from(reg) - 8992).div_euclid(4);
        for (k, &v) in (0i64..).zip(vals.iter()) {
            dict_set(
                &mut ints,
                first + k,
                [sbits(v, 24, 8), sbits(v, 16, 8), sbits(v, 8, 8), sbits(v, 0, 8)],
            );
        }
        int_defs.push(IntDef {
            reg,
            count: cnt,
            values: vals,
        });
        p += 1 + cnt as usize;
    }
    p += 1;
    let mut rest: Vec<u32> = words.get(p..).map_or_default(<[u32]>::to_vec);
    while rest.last() == Some(&0) {
        rest.pop();
    }
    let dt = DefTable {
        offset: off,
        fields: [h[0], h[1], h[2], h[3]],
        size,
        raw_words: words,
        float_defs,
        int_defs,
        bool_words: rest,
    };
    Ok((dt, floats, ints))
}

fn usage(i: u32) -> &'static str {
    DECL_USAGE.get(i as usize).copied().unwrap_or("?")
}

fn parse_header(r: &Reader, off: u32, is_pixel: bool) -> Result<Header> {
    let off = u64::from(off);
    let w = |k: u64| r.u32(off + 4 * k);
    let (physical_offset, physical_size, program_cntl) = (w(0)?, w(1)?, w(2)?);
    let (context_misc, field10, interpolator_info) = (w(3)?, w(4)?, w(5)?);
    let num_temp_regs = if is_pixel {
        bits(program_cntl, 8, 6)
    } else {
        bits(program_cntl, 0, 6)
    } + 1;
    let n_interp = bits(interpolator_info, 5, 5);
    let mut vertex_elements = Vec::new();
    let mut interpolators = Vec::new();
    let mut o;
    let stage = if is_pixel {
        let s = Stage::Pixel {
            interp_mask: w(6)?,
            outputs: w(7)?,
        };
        o = off + 32;
        s
    } else {
        let (field18, vertex_element_count, field20) = (w(6)?, w(7)?, w(8)?);
        o = off + 36;
        let field18_words = (0..u64::from(field18))
            .map(|k| r.u32(o + 4 * k))
            .collect::<Result<Vec<u32>>>()?;
        o += 4 * u64::from(field18);
        for _ in 0..vertex_element_count {
            let v = r.u32(o)?;
            vertex_elements.push(VertexElement {
                raw: v,
                fetch_address: bits(v, 0, 12),
                usage: usage(bits(v, 12, 4)),
                usage_index: bits(v, 16, 4),
                unknown_hi: v >> 20,
            });
            o += 4;
        }
        Stage::Vertex {
            field18,
            vertex_element_count,
            field20,
            field18_words,
        }
    };
    for _ in 0..n_interp {
        let v = r.u32(o)?;
        interpolators.push(Interpolator {
            raw: v,
            usage_index: bits(v, 0, 4),
            usage: usage(bits(v, 4, 4)),
            reg: bits(v, 8, 4),
            mask: bits(v, 12, 4),
            export_index: v >> 16,
            export_addresses: None,
        });
        o += 4;
    }
    let mut export_slots = Vec::new();
    if let Stage::Vertex { field20, .. } = stage {
        for _ in 0..field20 {
            let v = r.u32(o)?;
            export_slots.push(ExportSlot {
                raw: v,
                address: bits(v, 0, 12),
                last: bits(v, 12, 1),
                unknown_hi: v >> 13,
            });
            o += 4;
        }
        for it in &mut interpolators {
            let mut sl = Vec::new();
            for e in export_slots.iter().skip(it.export_index as usize) {
                sl.push(e.address);
                if e.last != 0 {
                    break;
                }
            }
            it.export_addresses = Some(sl);
        }
    }
    Ok(Header {
        offset: off as u32,
        physical_offset,
        physical_size,
        program_cntl,
        context_misc,
        field10,
        interpolator_info,
        num_temp_regs,
        interpolator_count: n_interp,
        vertex_elements,
        interpolators,
        stage,
        export_slots,
        end: o,
    })
}

// ------------------------------------------------------------------------------------------------- microcode

const fn is_exec(op: u32) -> bool {
    matches!(op, 1..=6 | 13 | 14)
}

fn decode_cf(w0: u32, w1: u32, index: u32) -> Cf {
    let op = bits(w1, 12, 4);
    let kind = if is_exec(op) {
        let (type_, bool_index, condition, is_predicate_clean) = match op {
            3 | 4 => (CfType::Conditional, Some(bits(w1, 2, 8)), Some(bits(w1, 10, 1)), false),
            5 | 6 => (CfType::Predicated, None, Some(bits(w1, 10, 1)), bits(w1, 9, 1) != 0),
            13 | 14 => (CfType::Conditional, Some(bits(w1, 2, 8)), Some(bits(w1, 10, 1)), true),
            _ => (CfType::Unconditional, None, None, bits(w1, 9, 1) != 0),
        };
        CfKind::Exec {
            address: bits(w0, 0, 12),
            count: bits(w0, 12, 3),
            is_yield: bits(w0, 15, 1),
            sequence: bits(w0, 16, 12),
            vc: bits(w0, 28, 4) | (bits(w1, 0, 2) << 4),
            is_end: matches!(op, 2 | 4 | 6 | 14),
            type_,
            bool_index,
            condition,
            is_predicate_clean,
        }
    } else {
        match op {
            7 => CfKind::LoopStart {
                address: bits(w0, 0, 13),
                is_repeat: bits(w0, 13, 1),
                loop_id: bits(w0, 16, 5),
            },
            8 => CfKind::LoopEnd {
                address: bits(w0, 0, 13),
                loop_id: bits(w0, 16, 5),
                is_predicated_break: bits(w0, 21, 1),
                condition: bits(w1, 10, 1),
            },
            9 | 11 => {
                let (is_unconditional, is_predicated) = (bits(w0, 13, 1), bits(w0, 14, 1));
                CfKind::Jump {
                    address: bits(w0, 0, 13),
                    is_unconditional,
                    is_predicated,
                    bool_index: bits(w1, 2, 8),
                    condition: bits(w1, 10, 1),
                    type_: if is_unconditional != 0 {
                        CfType::Unconditional
                    } else if is_predicated != 0 {
                        CfType::Predicated
                    } else {
                        CfType::Conditional
                    },
                    direction: (op == 11).then(|| bits(w1, 1, 1)),
                }
            }
            12 => CfKind::Alloc {
                size: bits(w0, 0, 3),
                is_unserialized: bits(w1, 8, 1),
                alloc_type: bits(w1, 9, 2),
            },
            _ => CfKind::Plain,
        }
    };
    Cf {
        index,
        opcode: op,
        address_mode: bits(w1, 11, 1),
        raw: (w0, w1),
        kind,
        instructions: None,
    }
}

fn swz_alu(sw: u32, comp: u32) -> u32 {
    ((sw >> (2 * comp)) + comp) & 3
}

fn xyzw(i: u32) -> char {
    XYZW.get(i as usize).copied().unwrap_or('?')
}

fn const_rel(w: [u32; 3], i: u32) -> Rel {
    let [_, w1, w2] = w;
    let (c0, c1) = (bits(w1, 31, 1), bits(w1, 30, 1));
    let (t1, t2) = (bits(w2, 31, 1), bits(w2, 30, 1));
    let addressed = match i {
        1 => c0,
        2 => {
            if t1 != 0 {
                c0
            } else {
                c1
            }
        }
        _ => {
            if t1 != 0 && t2 != 0 {
                c0
            } else {
                c1
            }
        }
    };
    if addressed == 0 {
        None
    } else if bits(w1, 29, 1) != 0 {
        Some("a0")
    } else {
        Some("aL")
    }
}

fn alu_operand(w: [u32; 3], i: u32) -> Operand {
    let [w0, w1, w2] = w;
    let reg = bits(w2, 8 * (3 - i), 8);
    let is_temp = bits(w2, 32 - i, 1) != 0;
    let swz = bits(w1, 8 * (3 - i), 8);
    let neg = bits(w1, 27 - i, 1) != 0;
    let swizzle: String = (0..4).map(|c| xyzw(swz_alu(swz, c))).collect();
    let (file, index, rel, abs) = if is_temp {
        (
            File::Temp,
            reg & 0x3f,
            if reg & 0x40 != 0 { Some("aL") } else { None },
            reg & 0x80 != 0,
        )
    } else {
        (File::Const, reg, const_rel(w, i), bits(w0, 7, 1) != 0)
    };
    Operand {
        two_op: false,
        negate: neg,
        swizzle_raw: swz,
        swizzle: Some(swizzle),
        file,
        index,
        rel,
        abs,
        components: None,
    }
}

fn export_target(idx: u32, is_pixel: bool) -> (&'static str, Option<u32>) {
    match idx {
        32 => ("eA", None),
        33..=37 => ("eM", Some(idx - 33)),
        0..=3 if is_pixel => ("oC", Some(idx)),
        61 if is_pixel => ("oDepth", None),
        0..=15 if !is_pixel => ("o", Some(idx)),
        62 if !is_pixel => ("oPos", None),
        63 if !is_pixel => ("oPts", None),
        _ => ("export?", Some(idx)),
    }
}

fn mask_write(m: u32) -> String {
    (0..4).map(|c| if m >> c & 1 != 0 { xyzw(c) } else { '_' }).collect()
}

fn decode_alu(w: [u32; 3], is_pixel: bool) -> Alu {
    let [w0, w1, w2] = w;
    let vop = bits(w2, 24, 5);
    let sop = bits(w0, 26, 6);
    let is_export = bits(w0, 15, 1) != 0;
    let (vmask, smask) = (bits(w0, 16, 4), bits(w0, 20, 4));
    let vdest_i = bits(w0, 0, 6);
    let sdest_i = bits(w0, 8, 6);
    let (vsat, ssat) = (bits(w0, 24, 1) != 0, bits(w0, 25, 1) != 0);
    let (vector_dest, scalar_dest) = if is_export {
        let (tgt, ti) = export_target(vdest_i, is_pixel);
        let c0 = if bits(w0, 14, 1) != 0 { 15 & !(vmask | smask) } else { 0 };
        let c1 = vmask & smask;
        let vm = vmask & !smask;
        let sm = smask & !vmask;
        let vcomp: String = (0..4)
            .map(|c| {
                if c0 >> c & 1 != 0 {
                    '0'
                } else if c1 >> c & 1 != 0 {
                    '1'
                } else if vm >> c & 1 != 0 {
                    xyzw(c)
                } else {
                    '_'
                }
            })
            .collect();
        let dest = |write: String, saturate| Dest {
            file: File::Export(tgt),
            index: ti,
            rel: None,
            raw_index: Some(vdest_i),
            write,
            saturate,
        };
        (dest(vcomp, vsat), dest(mask_write(sm), ssat))
    } else {
        let rel = |b| if b != 0 { Some("aL") } else { None };
        (
            Dest {
                file: File::Temp,
                index: Some(vdest_i),
                rel: rel(bits(w0, 6, 1)),
                raw_index: None,
                write: mask_write(vmask),
                saturate: vsat,
            },
            Dest {
                file: File::Temp,
                index: Some(sdest_i),
                rel: rel(bits(w0, 14, 1)),
                raw_index: None,
                write: mask_write(smask),
                saturate: ssat,
            },
        )
    };
    let nvec = VECTOR_OPS.get(vop as usize).map_or(0, |o| o.1);
    let vector_operands: Vec<Operand> = (1..=nvec as u32).map(|i| alu_operand(w, i)).collect();
    let (_, nops, two) = SCALAR_OPS.get(sop as usize).copied().unwrap_or(("?", 0, false));
    let mut scalar_operands = Vec::new();
    if nops == 1 {
        let mut o = alu_operand(w, 3);
        let s: Vec<char> = o.swizzle().chars().collect();
        let mut comps = String::new();
        comps.extend(s.get(3));
        if two {
            comps.extend(s.first());
        }
        o.components = Some(comps);
        scalar_operands.push(o);
    } else if nops == 2 {
        let swz = bits(w1, 0, 8);
        let neg = bits(w1, 24, 1) != 0;
        let absc = bits(w0, 7, 1) != 0;
        scalar_operands.push(Operand {
            two_op: true,
            negate: neg,
            swizzle_raw: swz,
            swizzle: None,
            file: File::Const,
            index: bits(w2, 0, 8),
            rel: const_rel(w, 3),
            abs: absc,
            components: Some(xyzw(swz_alu(swz, 3)).to_string()),
        });
        let treg = (sop & 1) | (bits(w2, 29, 1) << 1) | (swz & 0x3c);
        scalar_operands.push(Operand {
            two_op: true,
            negate: neg,
            swizzle_raw: swz,
            swizzle: None,
            file: File::Temp,
            index: treg,
            rel: None,
            abs: absc,
            components: Some(xyzw(swz_alu(swz, 0)).to_string()),
        });
    }
    let mut alu = Alu {
        predicated: bits(w1, 28, 1) != 0,
        pred_condition: bits(w1, 27, 1) != 0,
        is_export,
        abs_constants: bits(w0, 7, 1) != 0,
        const_addr_reg_relative: bits(w1, 29, 1) != 0,
        const_0_rel: bits(w1, 31, 1) != 0,
        const_1_rel: bits(w1, 30, 1) != 0,
        vector_opcode: vop,
        scalar_opcode: sop,
        vector_write_mask_raw: vmask,
        scalar_write_mask_raw: smask,
        vector_dest,
        scalar_dest,
        vector_operands,
        scalar_operands,
        vector_nop: false,
        scalar_nop: false,
    };
    alu.vector_nop = vector_default_nop(&alu);
    let sd = &alu.scalar_dest;
    alu.scalar_nop =
        sop == 50 && sd.writes_nothing() && !sd.saturate && (is_export || (sd.index == Some(0) && sd.rel.is_none()));
    if alu.vector_nop && is_export && alu.scalar_nop {
        alu.vector_nop = false;
    }
    alu
}

fn vector_default_nop(ins: &Alu) -> bool {
    let d = &ins.vector_dest;
    if ins.vector_opcode != 2 || !d.writes_nothing() || d.saturate {
        return false;
    }
    for o in &ins.vector_operands {
        if o.file != File::Temp || o.index != 0 || o.rel.is_some() || o.negate || o.abs || o.swizzle() != "xyzw" {
            return false;
        }
    }
    !(!ins.is_export && (d.index.is_some_and(|i| i != 0) || d.rel.is_some()))
}

fn fetch_dst(swz: u32) -> String {
    (0..4)
        .map(|c| {
            b"xyzw01?_"
                .get(bits(swz, 3 * c, 3) as usize)
                .map_or('?', |&b| char::from(b))
        })
        .collect()
}

fn decode_fetch(w: [u32; 3], prev: Option<&Fetch>) -> Fetch {
    let [w0, w1, w2] = w;
    let op = bits(w0, 0, 5);
    let rel = |b| if b != 0 { Some("aL") } else { None };
    let mut f = Fetch {
        opcode: op,
        op: String::new(),
        src: bits(w0, 5, 6),
        src_rel: rel(bits(w0, 11, 1)),
        dst: bits(w0, 12, 6),
        dst_rel: rel(bits(w0, 18, 1)),
        dst_swizzle: fetch_dst(bits(w1, 0, 12)),
        predicated: bits(w1, 31, 1) != 0,
        pred_condition: bits(w2, 31, 1) != 0,
        kind: FetchKind::Vertex(VertexFetch {
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
        }),
    };
    f.op = f.first_op();
    if op == 0 {
        let mut v = VertexFetch {
            must_be_one: bits(w0, 19, 1),
            const_index: bits(w0, 20, 5),
            const_index_sel: bits(w0, 25, 2),
            prefetch_count: bits(w0, 27, 3),
            src_swizzle: xyzw(bits(w0, 30, 2)),
            is_signed: bits(w1, 12, 1) != 0,
            is_integer: bits(w1, 13, 1) != 0,
            signed_rf_mode: bits(w1, 14, 1),
            is_index_rounded: bits(w1, 15, 1) != 0,
            format: bits(w1, 16, 6),
            exp_adjust: sbits(w1, 24, 6),
            is_mini: bits(w1, 30, 1) != 0,
            stride: bits(w2, 0, 8),
            offset: sbits(w2, 8, 23),
            fetch_const: 0,
        };
        let full = if v.is_mini {
            prev.and_then(|p| {
                if let FetchKind::Vertex(pv) = &p.kind {
                    Some((p, pv))
                } else {
                    None
                }
            })
        } else {
            None
        };
        if let Some((p, pv)) = full {
            v.fetch_const = pv.const_index * 3 + pv.const_index_sel;
            f.src = p.src;
            f.src_rel = p.src_rel;
            v.src_swizzle = pv.src_swizzle;
            v.stride = pv.stride;
        } else {
            v.fetch_const = v.const_index * 3 + v.const_index_sel;
        }
        if v.is_mini { "vfetch_mini" } else { "vfetch_full" }.clone_into(&mut f.op);
        f.kind = FetchKind::Vertex(v);
    } else {
        let dimension = DIMENSIONS.get(bits(w2, 14, 2) as usize).copied().unwrap_or("2D");
        let mut ncomp = match dimension {
            "1D" => 1,
            "2D" => 2,
            _ => 3,
        };
        match op {
            18 => ncomp = 2,
            24 => ncomp = 1,
            25 | 26 => ncomp = 3,
            _ => {}
        }
        let src_swizzle_raw = bits(w0, 26, 6);
        let filt = |t: &[&'static str], v: u32| t.get(v as usize).copied().unwrap_or("?");
        let t = TextureFetch {
            fetch_valid_only: bits(w0, 19, 1) != 0,
            const_index: bits(w0, 20, 5),
            unnormalized: bits(w0, 25, 1) != 0,
            src_swizzle_raw,
            mag_filter: filt(&TEX_FILTER, bits(w1, 12, 2)),
            min_filter: filt(&TEX_FILTER, bits(w1, 14, 2)),
            mip_filter: filt(&TEX_FILTER, bits(w1, 16, 2)),
            aniso_filter: filt(&ANISO_FILTER, bits(w1, 18, 3)),
            arbitrary_filter: filt(&ARB_FILTER, bits(w1, 21, 3)),
            vol_mag_filter: filt(&TEX_FILTER, bits(w1, 24, 2)),
            vol_min_filter: filt(&TEX_FILTER, bits(w1, 26, 2)),
            use_computed_lod: bits(w1, 28, 1) != 0,
            use_register_lod: bits(w1, 29, 1) != 0,
            unk: bits(w1, 30, 1),
            use_register_gradients: bits(w2, 0, 1) != 0,
            sample_location: if bits(w2, 1, 1) != 0 { "center" } else { "centroid" },
            lod_bias: f64::from(sbits(w2, 2, 7)) / 16.0,
            unused: bits(w2, 9, 5),
            dimension,
            offset_x: f64::from(sbits(w2, 16, 5)) / 2.0,
            offset_y: f64::from(sbits(w2, 21, 5)) / 2.0,
            offset_z: f64::from(sbits(w2, 26, 5)) / 2.0,
            src_swizzle: (0..ncomp).map(|c| xyzw(bits(src_swizzle_raw, 2 * c, 2))).collect(),
            has_dest: !matches!(op, 24..=26),
            has_const: !matches!(op, 24..=26),
        };
        if matches!(op, 1 | 16 | 17 | 19) {
            f.op.push_str(dimension);
        }
        f.kind = FetchKind::Texture(t);
    }
    f
}

/// The texture fetch's fetch constant (`fetch_const`) or a vertex fetch's.
#[must_use]
pub fn fetch_const(f: &Fetch) -> u32 {
    match &f.kind {
        FetchKind::Vertex(v) => v.fetch_const,
        FetchKind::Texture(t) => t.const_index,
    }
}

struct Microcode {
    words: Vec<u32>,
    cf_slots: u32,
    cf: Vec<Cf>,
    instructions: Vec<Instr>,
    unused: Vec<u32>,
    unused_nonzero: Vec<u32>,
}

fn decode_microcode(r: &Reader, code_off: u64, size: u32, is_pixel: bool) -> Result<Microcode> {
    let nw = u64::from(size / 4);
    let words = (0..nw).map(|k| r.u32(code_off + 4 * k)).collect::<Result<Vec<u32>>>()?;
    let nslots = words.len() / 3;
    let slot = |i: usize| -> [u32; 3] {
        let s = words.get(3 * i..3 * i + 3).unwrap_or(&[0, 0, 0]);
        [
            s.first().copied().unwrap_or(0),
            s.get(1).copied().unwrap_or(0),
            s.get(2).copied().unwrap_or(0),
        ]
    };
    let pair = |[d0, d1, d2]: [u32; 3]| [(d0, d1 & 0xffff), ((d1 >> 16) | (d2 << 16), d2 >> 16)];
    let mut bound = nslots;
    let mut i = 0;
    while i < bound {
        for (cfw0, cfw1) in pair(slot(i)) {
            if is_exec(bits(cfw1, 12, 4)) {
                bound = bound.min(bits(cfw0, 0, 12) as usize);
            }
        }
        i += 1;
    }
    let mut cfs = Vec::with_capacity(2 * bound);
    for i in 0..bound {
        let [(a0, a1), (b0, b1)] = pair(slot(i));
        cfs.push(decode_cf(a0, a1, 2 * i as u32));
        cfs.push(decode_cf(b0, b1, 2 * i as u32 + 1));
    }
    while cfs.last().is_some_and(|c| c.opcode == 0) {
        cfs.pop();
    }
    // instructions by slot; then sorted by address with each CF's list pointing into them
    let mut by_slot: Vec<Option<Instr>> = Vec::new();
    let mut exec_slots: Vec<Vec<u32>> = Vec::with_capacity(cfs.len());
    let mut prev_vf: Option<Fetch> = None;
    for cf in &cfs {
        let CfKind::Exec {
            address,
            count,
            sequence,
            ..
        } = cf.kind
        else {
            exec_slots.push(Vec::new());
            continue;
        };
        let mut seq = sequence;
        let mut list = Vec::new();
        for a in address..address + count {
            if a as usize >= nslots {
                return Err(XenosError::Format(format!(
                    "exec at cf {} reads past code ({a} >= {nslots})",
                    cf.index
                )));
            }
            let w = slot(a as usize);
            let is_fetch = seq & 1 != 0;
            let serialize = seq & 2 != 0;
            seq >>= 2;
            let kind = if is_fetch {
                let f = decode_fetch(w, prev_vf.as_ref());
                if matches!(&f.kind, FetchKind::Vertex(v) if !v.is_mini) {
                    prev_vf = Some(f.clone());
                }
                InstrKind::Fetch(f)
            } else {
                InstrKind::Alu(decode_alu(w, is_pixel))
            };
            if by_slot.len() <= a as usize {
                by_slot.resize(a as usize + 1, None);
            }
            let cell = by_slot
                .get_mut(a as usize)
                .ok_or_else(|| malformed("slot", u64::from(a)))?;
            if cell.is_some() {
                return Err(XenosError::Format(format!("instruction slot {a} executed by two CFs")));
            }
            *cell = Some(Instr {
                address: a,
                raw: w,
                kind,
                serialize,
                cf_index: cf.index,
            });
            list.push(a);
        }
        exec_slots.push(list);
    }
    let mut position = vec![usize::MAX; by_slot.len()];
    let mut instructions = Vec::new();
    for (a, ins) in by_slot.into_iter().enumerate() {
        if let Some(ins) = ins {
            if let Some(p) = position.get_mut(a) {
                *p = instructions.len();
            }
            instructions.push(ins);
        }
    }
    for (cf, list) in cfs.iter_mut().zip(exec_slots) {
        if matches!(cf.kind, CfKind::Exec { .. }) {
            cf.instructions = Some(list.iter().filter_map(|&a| position.get(a as usize).copied()).collect());
        }
    }
    let executed = |a: usize| instructions.binary_search_by_key(&(a as u32), |i| i.address).is_ok();
    let unused: Vec<u32> = (bound..nslots).filter(|&a| !executed(a)).map(|a| a as u32).collect();
    let unused_nonzero = unused
        .iter()
        .copied()
        .filter(|&a| slot(a as usize).iter().any(|&x| x != 0))
        .collect();
    Ok(Microcode {
        words,
        cf_slots: bound as u32,
        cf: cfs,
        instructions,
        unused,
        unused_nonzero,
    })
}

/// `parse_shader(blob, is_pixel)`: the container, its tables and the microcode. `is_pixel` None takes the stage
/// from the magic's low byte (0: pixel).
///
/// # Errors
/// [`XenosError`] wherever `xenos.py` raises.
pub fn parse(blob: &[u8], is_pixel: Option<bool>) -> Result<Shader> {
    let r = Reader { b: blob };
    let magic = r.u32(0)?;
    if magic >> 8 != 0x0010_2a11 {
        return Err(XenosError::Format(format!("bad magic {magic:08x}")));
    }
    let is_pixel = is_pixel.unwrap_or(magic.trailing_zeros() >= 8);
    let mut c = [0u32; 9];
    for (k, x) in (0u64..).zip(c.iter_mut()) {
        *x = r.u32(4 * k)?;
    }
    let [_, vsize, psize, field_c, ctab_off, def_off, hdr_off, field_1c, field_20] = c;
    if u64::from(vsize) + u64::from(psize) != blob.len() as u64 {
        return Err(XenosError::Format(format!(
            "virtual+physical size {} != blob {}",
            u64::from(vsize) + u64::from(psize),
            blob.len()
        )));
    }
    let ctab = if ctab_off != 0 {
        Some(parse_ctab(&r, ctab_off)?)
    } else {
        None
    };
    let (def_table, literals, int_literals) = if def_off != 0 {
        let (d, f, i) = parse_definitions(&r, def_off, vsize, is_pixel)?;
        (Some(d), f, i)
    } else {
        (None, Vec::new(), Vec::new())
    };
    let header = parse_header(&r, hdr_off, is_pixel)?;
    let mut layout_gaps = Vec::new();
    let mut pos: u64 = 0x24;
    let spans = [
        (
            u64::from(ctab_off),
            u64::from(ctab_off) + ctab.as_ref().map_or(0, |t| 4 + u64::from(t.size)),
        ),
        (
            u64::from(def_off),
            u64::from(def_off) + def_table.as_ref().map_or(0, |d| 20 + u64::from(d.size)),
        ),
        (u64::from(hdr_off), header.end),
    ];
    for (start, end) in spans {
        if start == 0 {
            continue;
        }
        if start != pos {
            layout_gaps.push((pos, start));
        }
        pos = end;
    }
    if pos != u64::from(vsize) {
        layout_gaps.push((pos, u64::from(vsize)));
    }
    if u64::from(header.physical_offset) + u64::from(header.physical_size) != u64::from(psize) {
        return Err(XenosError::Format("microcode does not end the blob".into()));
    }
    let code_offset = u64::from(vsize) + u64::from(header.physical_offset);
    let code_size = header.physical_size;
    let mc = decode_microcode(&r, code_offset, code_size, is_pixel)?;
    Ok(Shader {
        is_pixel,
        magic,
        container: [vsize, psize, field_c, ctab_off, def_off, hdr_off, field_1c, field_20],
        ctab,
        literals,
        int_literals,
        def_table,
        header,
        layout_gaps,
        code_offset,
        code_size,
        cf: mc.cf,
        instructions: mc.instructions,
        cf_slots: mc.cf_slots,
        unused_slots: mc.unused,
        unused_nonzero_slots: mc.unused_nonzero,
        code_words: mc.words,
    })
}
