//! `bfbc2 aimanim <file.res> [--big]`: `bfbc2-anim-tools\aimanim.py`'s command line (the bones and the first pose)
//! over `bc2_formats::aimanim::parse`. Where the core refuses (the reference raises `AimError`, or a value is a
//! NaN) and where the Python would raise `IndexError` (no poses), the Python answers.

use bc2_formats::aimanim::{self, Block, Quad};

use crate::pyfmt::{docstring, float_repr};
use crate::pyproc::{Fail, Py, PyResult, read_file};

const SOURCE: &str = include_str!("../../../../bfbc2-anim-tools/aimanim.py");

/// A pose's four floats as Python prints a tuple of them.
fn quad(q: &Quad) -> String {
    let parts: Vec<String> = q.iter().map(|&v| float_repr(f64::from(v))).collect();
    format!("({})", parts.join(", "))
}

pub fn run(py: &mut Py, args: &[String]) -> PyResult<i32> {
    let Some(path) = args.first() else {
        py.print(&docstring(SOURCE))?;
        return Ok(2);
    };
    let big = args.iter().any(|a| a == "--big");
    let data = read_file(path)?;
    let a = aimanim::parse(&data, big).map_err(|_| Fail::Python)?;
    py.print(&format!("{} bones, {} poses", a.bones.len(), a.poses))?;
    for (bone, kind, block) in &a.bones {
        let (name, first) = match block {
            Block::Fixed(p, _) => ("fixed", quad(p)),
            Block::Rotation(p, qs) => (
                "rotation",
                format!("({}, {})", quad(p), quad(qs.first().ok_or(Fail::Python)?)),
            ),
            Block::Moving(ps) => {
                let (p, q) = ps.first().ok_or(Fail::Python)?;
                ("moving", format!("({}, {})", quad(p), quad(q)))
            }
        };
        // KIND_NAMES[kind]: the parser reads kinds 0..=2 only (others are refused)
        let name = match kind {
            0 => "fixed",
            1 => "rotation",
            2 => "moving",
            _ => name,
        };
        py.print(&format!("  bone {bone:3} {name:8} {first}"))?;
    }
    Ok(0)
}
