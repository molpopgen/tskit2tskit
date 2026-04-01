use pyo3::prelude::*;

unsafe fn read_tsk_ptr(
    py_obj: *mut pyo3::ffi::PyObject,
) -> *mut *mut tskit::bindings::tsk_table_collection_t {
    unsafe {
        // _tskit.TableCollection stores a *pointer* to a heap-allocated
        // tsk_table_collection_t immediately after PyObject_HEAD (offset 16).
        let offset = std::mem::size_of::<pyo3::ffi::PyObject>();
        (py_obj as *mut u8).add(offset) as *mut *mut tskit::bindings::tsk_table_collection_t
    }
}

/// Convert a rust TableCollection into a Python TreeSequence, making
/// zero extra copies!
///
/// # Errors
///
/// * An error will be returned if the input tables do not represent a valid
///   TreeSequence.
///   
/// # Safety
///
/// * The `_tskit.TableCollection` must be based on a tsk_table_collection_t
///   whose ABI is identical to that used to build tskit-rust
///
pub unsafe fn tables2treeseq(
    py: Python<'_>,
    rust_tables: tskit::TableCollection,
) -> PyResult<Py<PyAny>> {
    let sequence_length: f64 = rust_tables.sequence_length().into();

    // Create an empty _tskit.TableCollection
    let ll_tskit = py.import("_tskit")?;
    let py_ll_tc = ll_tskit
        .getattr("TableCollection")?
        .call1((sequence_length,))?;

    // Convert the rust tables into a raw pointer.
    // We unwrap the Option here because a NULL pointer
    // is a HARD error!
    // NOTE: tskit-rust uses malloc for these pointers!
    // (Otherwise nothing below would be valid.)
    let rust_tables_ptr = rust_tables.into_mut_ptr().unwrap();

    unsafe {
        // 1. Get a pointer to the pointer to the Python-side
        //    TableCollection
        let py_obj_ptr = py_ll_tc.as_ptr();
        let dest_ptr = read_tsk_ptr(py_obj_ptr);
        // 2. Tear down the contents of the Python-side tables
        tskit::bindings::tsk_table_collection_free(*dest_ptr);
        // 3. Deallocate the memory of the Python-side tables
        libc::free((*dest_ptr).cast::<libc::c_void>());
        // 4. Rebind the pointer to what we got from rust!
        *dest_ptr = rust_tables_ptr.as_ptr();
    }

    // Wrap in high-level tskit.TableCollection and create tree sequence
    let tskit_mod = py.import("tskit")?;
    let kwargs = pyo3::types::PyDict::new(py);
    kwargs.set_item("ll_tables", &py_ll_tc)?;
    let py_tc = tskit_mod
        .getattr("TableCollection")?
        .call((), Some(&kwargs))?;
    let ts = py_tc.call_method0("tree_sequence")?;
    Ok(ts.unbind())
}

/// Convert a rust TreeSequence into a Python TreeSequence without
/// making extra copies of the TableCollection.
///
/// # Safety
///
/// * The `_tskit.TableCollection` must be based on a tsk_table_collection_t
///   whose ABI is identical to that used to build tskit-rust
pub unsafe fn treeseq2treeseq(
    py: Python<'_>,
    rust_treeseq: tskit::TreeSequence,
) -> PyResult<Py<PyAny>> {
    let rust_tables = rust_treeseq.dump_tables().unwrap();
    unsafe { tables2treeseq(py, rust_tables) }
}

// NOTE: for tests to run, tskit-python must be installed!

#[test]
fn testit() {
    Python::attach(|py| {
        let mut t = tskit::TableCollection::new(666.).unwrap();
        let c = t
            .add_node(tskit::NodeFlags::IS_SAMPLE, 0.0, -1, -1)
            .unwrap();
        let p = t.add_node(0, 1.0, -1, -1).unwrap();
        t.add_edge(0., 100., p, c).unwrap();
        let pyt = unsafe { tables2treeseq(py, t).unwrap() };
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
