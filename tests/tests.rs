use pyo3::prelude::*;

// NOTE: for tests to run, tskit-python must be installed!

#[test]
fn test_simple_tables() {
    Python::attach(|py| {
        let mut rust_tables = tskit2tskit::empty_tables(666.).unwrap();
        let c = rust_tables
            .add_node(tskit::NodeFlags::IS_SAMPLE, 0.0, -1, -1)
            .unwrap();
        let p = rust_tables.add_node(0, 1.0, -1, -1).unwrap();
        rust_tables.add_edge(0., 100., p, c).unwrap();

        // SAFETY: rust_tables is generated via empty_tables
        let pyt = unsafe { tskit2tskit::tables2treeseq(py, rust_tables).unwrap() };
        let seqlen = pyt.getattr(py, "sequence_length").unwrap();
        let seqlen: f64 = seqlen.extract(py).unwrap();
        assert_eq!(seqlen, 666.);
        let num_nodes: u64 = pyt.getattr(py, "num_nodes").unwrap().extract(py).unwrap();
        assert_eq!(num_nodes, 2);
        let num_edges: u64 = pyt.getattr(py, "num_edges").unwrap().extract(py).unwrap();
        assert_eq!(num_edges, 1);
        let edge = pyt
            .getattr(py, "edge")
            .unwrap()
            .call(py, (0,), None)
            .unwrap();
        let left: f64 = edge.getattr(py, "left").unwrap().extract(py).unwrap();
        let right: f64 = edge.getattr(py, "right").unwrap().extract(py).unwrap();
        assert_eq!(left, 0.);
        assert_eq!(right, 100.);
    })
}

#[test]
fn test_simplify_tables() {
    Python::attach(|py| {
        let mut rust_tables = tskit2tskit::empty_tables(666.).unwrap();
        let p = rust_tables.add_node(0, 1., -1, -1).unwrap();
        let c0 = rust_tables
            .add_node(tskit::NodeFlags::IS_SAMPLE, 0., -1, -1)
            .unwrap();
        let c1 = rust_tables
            .add_node(tskit::NodeFlags::IS_SAMPLE, 0., -1, -1)
            .unwrap();
        let _ = rust_tables.add_edge(0., 666., p, c0).unwrap();
        let _ = rust_tables.add_edge(0., 666., p, c1).unwrap();
        rust_tables.full_sort(0).unwrap();
        rust_tables.simplify(&[c0, c1], 0, false).unwrap();
        // SAFETY: rust_tables is generated via empty_tables
        let _pyt = unsafe { tskit2tskit::tables2treeseq(py, rust_tables).unwrap() };
    })
}

#[test]
fn test_simplify_tables_via_treeseq() {
    Python::attach(|py| {
        let mut rust_tables = tskit2tskit::empty_tables(666.).unwrap();
        let p = rust_tables.add_node(0, 1., -1, -1).unwrap();
        let c0 = rust_tables
            .add_node(tskit::NodeFlags::IS_SAMPLE, 0., -1, -1)
            .unwrap();
        let c1 = rust_tables
            .add_node(tskit::NodeFlags::IS_SAMPLE, 0., -1, -1)
            .unwrap();
        let _ = rust_tables.add_edge(0., 666., p, c0).unwrap();
        let _ = rust_tables.add_edge(0., 666., p, c1).unwrap();
        rust_tables.full_sort(0).unwrap();
        rust_tables.build_index().unwrap();
        let rust_ts = rust_tables.tree_sequence(0).unwrap();
        let (_ts, _idmap) = rust_ts.simplify(&[c0, c1], 0, false).unwrap();
        let rust_tables = rust_ts.dump_tables().unwrap();
        // SAFETY: rust_tables is generated via empty_tables
        let _pyt = unsafe { tskit2tskit::tables2treeseq(py, rust_tables).unwrap() };
    })
}
