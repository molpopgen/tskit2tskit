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
