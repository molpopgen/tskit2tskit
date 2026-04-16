use pyo3::prelude::*;

/// A Python module implemented in Rust.
#[pymodule]
// The setup is designed for a mixed rust/python
// project. We compile the rust side to the following name,
// with the intent that __init__.py imports this to bring
// the public API into scope.
#[pyo3(name = "_maketrees")]
mod maketrees {
    use pyo3::prelude::*;
    #[pyfunction]
    fn maketrees(py: Python<'_>) -> PyResult<Py<PyAny>> {
        let mut holder = tskit2tskit::TableCollectionHolder::new(py, 100.0).unwrap();
        // Release the gil to work only on the rust side of the data,
        // potentially allowing other Python threads to run.
        py.detach(|| -> Result<(), PyErr> {
            // SAFETY: the code below is safe if tskit-rust and tskit-python
            // are built around the same layout for `tsk_table_collection_t`.
            Ok(unsafe {
                holder.with_mut_tables(|t| -> Result<(), tskit2tskit::Error> {
                    // Everything in this block is the standard tskit rust API.
                    // Note that the use if ? will convert and TskitError into
                    // a tskit2tskit::Error.
                    let parent = t.add_node(0, 1.0, -1, -1)?;
                    let c0 = t.add_node(tskit::NodeFlags::IS_SAMPLE, 0.0, -1, -1)?;
                    let c1 = t.add_node(tskit::NodeFlags::IS_SAMPLE, 0.0, -1, -1)?;
                    t.add_edge(0., 100., parent, c0)?;
                    t.add_edge(0., 100., parent, c1)?;
                    Ok(())
                })
            }?) // The ? here will convert tskit2tskit::Error into a PyErr
        })?;
        // Returns Python tskit.TreeSequence
        // Again, error types will propagate into PyErr as needed.
        Ok(holder.into_python_tree_sequence(py)?)
    }
}
