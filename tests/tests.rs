use pyo3::prelude::*;

// NOTE: for tests to run, tskit-python must be installed!

#[test]
fn test_mutable_holder() {
    Python::attach(|py| {
        let mut holder = tskit2tskit::TableCollectionHolder::new(py, 100.).unwrap();
        // SAFETY: no ABI mismatch b/w rust and Python is possible b/c
        // we never make further access of the Python tables
        unsafe { holder.with_mut_tables(|tables| assert!(tables.add_node(0, 0., -1, -1).is_ok())) }
    })
}

#[test]
fn test_mutable_holder_into_tree_sequence() {
    Python::attach(|py| {
        let mut holder = tskit2tskit::TableCollectionHolder::new(py, 666.).unwrap();
        unsafe {
            holder.with_mut_tables(|tables| {
                let p = tables.add_node(0, 1., -1, -1).unwrap();
                let c0 = tables
                    .add_node(tskit::NodeFlags::IS_SAMPLE, 0., -1, -1)
                    .unwrap();
                let c1 = tables
                    .add_node(tskit::NodeFlags::IS_SAMPLE, 0., -1, -1)
                    .unwrap();
                let _ = tables.add_edge(0., 666., p, c0).unwrap();
                let _ = tables.add_edge(0., 666., p, c1).unwrap();
                tables.full_sort(0).unwrap();
                tables.build_index().unwrap();
            })
        }
        let _pyts = holder.into_python_tree_sequence(py).unwrap();
    })
}
