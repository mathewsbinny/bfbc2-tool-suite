//! Round E3o: `bfbc2core.navpath.route`'s A* in the Rust core (`bc2_navbuild::astar`). A `PathGraph` holds a file's
//! adjacency and positions (made once per `NavData`); `route` answers one search; `norms` gives the estimate's
//! formula's values so the Python can check them against `np.linalg.norm` on the machine before trusting them.

use bc2_navbuild::astar::{Graph, Norm};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

fn norm_of(name: &str) -> PyResult<Norm> {
    match name {
        "fma" => Ok(Norm::Fma),
        "plain" => Ok(Norm::Plain),
        _ => Err(PyValueError::new_err(format!("unknown norm {name:?}"))),
    }
}

fn f64s(b: &[u8]) -> Vec<f64> {
    b.as_chunks::<8>().0.iter().map(|c| f64::from_le_bytes(*c)).collect()
}

fn indices(b: &[u8]) -> PyResult<Vec<usize>> {
    b.as_chunks::<8>()
        .0
        .iter()
        .map(|c| usize::try_from(i64::from_le_bytes(*c)).map_err(|_| PyValueError::new_err("a negative index")))
        .collect()
}

/// A file's route graph for `navpath.route`: `first` (int64, nodes + 1), `to` (int64), `cost` (float64) as
/// `navpath._adjacency` gives them, `pos` the nodes' positions (float64 x 3). A `ValueError` when they are not
/// consistent or not all finite (the reference then runs).
#[pyclass(frozen)]
pub struct PathGraph {
    graph: Graph,
}

#[pymethods]
impl PathGraph {
    #[new]
    fn new(
        first: &Bound<'_, PyBytes>,
        to: &Bound<'_, PyBytes>,
        cost: &Bound<'_, PyBytes>,
        pos: &Bound<'_, PyBytes>,
    ) -> PyResult<Self> {
        let p = f64s(pos.as_bytes());
        if !p.len().is_multiple_of(3) {
            return Err(PyValueError::new_err("positions: three float64 per node"));
        }
        let graph = Graph {
            first: indices(first.as_bytes())?,
            to: indices(to.as_bytes())?,
            cost: f64s(cost.as_bytes()),
            pos: p.as_chunks::<3>().0.to_vec(),
        };
        if !graph.valid() {
            return Err(PyValueError::new_err("an inconsistent or non-finite route graph"));
        }
        Ok(Self { graph })
    }

    /// `(expanded, nodes, cost)`: the search from `start` to `goal` with the estimate's formula `norm` ("fma" or
    /// "plain"); `nodes` empty when the goal is not reached.
    fn route(&self, py: Python<'_>, start: usize, goal: usize, norm: &str) -> PyResult<(usize, Vec<usize>, f64)> {
        let norm = norm_of(norm)?;
        let n = self.graph.pos.len();
        if start >= n || goal >= n {
            return Err(PyValueError::new_err("a node out of range"));
        }
        let f = py.detach(|| self.graph.search(norm, start, goal));
        Ok((f.expanded, f.nodes, f.cost))
    }

    /// The formula's norm of each float64 x 3 vector of `vectors`, as float64 bytes.
    #[staticmethod]
    fn norms<'py>(py: Python<'py>, vectors: &Bound<'py, PyBytes>, norm: &str) -> PyResult<Bound<'py, PyBytes>> {
        let norm = norm_of(norm)?;
        let v = f64s(vectors.as_bytes());
        let out: Vec<u8> = v
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&[x, y, z]| norm.of(x, y, z).to_le_bytes())
            .collect();
        Ok(PyBytes::new(py, &out))
    }
}
