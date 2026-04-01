use pyo3::prelude::*;

/// # Safety
/// * `py_obj` must be a valid pointer to a `_tskit.TableCollection` Python object,
///   whose layout has `tsk_table_collection_t` immediately after `PyObject_HEAD`.
/// * Further, the `_tskit.TableCollection` must be based on a tsk_table_collection_t
///   whose ABI is identical to that used to build tskit-rust
pub unsafe fn read_tsk_ptr(
    py_obj: *mut pyo3::ffi::PyObject,
) -> *mut *mut tskit::bindings::tsk_table_collection_t {
    unsafe {
        // _tskit.TableCollection stores a *pointer* to a heap-allocated
        // tsk_table_collection_t immediately after PyObject_HEAD (offset 16).
        let offset = std::mem::size_of::<pyo3::ffi::PyObject>();
        let ptr_field =
            (py_obj as *mut u8).add(offset) as *mut *mut tskit::bindings::tsk_table_collection_t;
        ptr_field
    }
}

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

    let rust_tables_ptr = rust_tables.into_mut_ptr().unwrap();
    unsafe {
        let py_obj_ptr = py_ll_tc.as_ptr();
        let dest_ptr = read_tsk_ptr(py_obj_ptr);
        tskit::bindings::tsk_table_collection_free(*dest_ptr);
        libc::free((*dest_ptr).cast::<libc::c_void>());
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

pub unsafe fn treeseq2treeseq(
    py: Python<'_>,
    rust_treeseq: tskit::TreeSequence,
) -> PyResult<Py<PyAny>> {
    let rust_tables = rust_treeseq.dump_tables().unwrap();
    tables2treeseq(py, rust_tables)
}

#[test]
fn testit() {
    Python::attach(|py| {
        let t = tskit::TableCollection::new(666.).unwrap();
        let _ = unsafe { tables2treeseq(py, t).unwrap() };
    })
}
