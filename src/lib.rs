#![doc = include_str!("../README.md")]
//! # Core functionality
//! ## Working with pure [`tskit`] types.
//!
//! This approach gives access to the complete rust `tskit` API.
//! We create a [`tskit::TableCollection`] backed by a pointer to
//! [`tskit::bindings::tsk_table_collection_t`] allocated by `PyMem_Malloc`
//! (the Python memory allocator).
//!
//! Here is an example, with full type annotation for clarity:
//!
//! ```rust
//! use pyo3::prelude::*;
//!
//! // We must execute code in a Python interpreter
//! Python::attach(|py| -> Result<Py<PyAny>, tskit2tskit::Error> {
//!     // allocate a pointer and return initialized tables
//!     let mut tables: tskit::TableCollection = tskit2tskit::empty_tables(100.0)?;
//!
//!     // We can modify the tables using the standard rust API
//!     tables.add_node(tskit::NodeFlags::IS_SAMPLE, 0.0, -1, -1)?;
//!
//!     // return a Python tskit.TreeSequence
//!     // SAFETY: this is safe if the Python and rust
//!     // interfaces share the same layout for tsk_table_collection_t
//!     // The call to tables2treeseq ensures correct teardown of the input value.
//!     unsafe { tskit2tskit::tables2treeseq(py, tables) }
//! }).unwrap();
//! ```
//!
//! The following example is contrived because we never return the rust object
//! to Python and therefore never need a table collection with a pointer
//! managed by the Python allocator.
//! However, this example does show how to go from tables to tree sequence
//! and back on the rust side and how to correctly tear down the tables.
//! This example works because the Python-allocated pointer remains valid
//! during the round-trip through the tree sequence.
//!
//! ```rust
//! use pyo3::prelude::*;
//!
//! // We must execute code in a Python interpreter
//! Python::attach(|py| -> Result<(), tskit2tskit::Error> {
//!     // allocate a pointer and return initialized tables
//!     let mut tables: tskit::TableCollection = tskit2tskit::empty_tables(100.0)?;
//!
//!     // We can modify the tables using the standard rust API
//!     let child = tables.add_node(tskit::NodeFlags::IS_SAMPLE, 0.0, -1, -1)?;
//!     let parent = tables.add_node(0, 1.0, -1, -1)?;
//!     tables.add_edge(0., 100., parent, child)?;
//!
//!     let ts = tables.tree_sequence(tskit::TreeSequenceFlags::BUILD_INDEXES)?;
//!     assert!(!ts.tables().as_ptr().is_null());
//!     let tables = ts.dump_tables()?;
//!     assert!(!tables.as_ptr().is_null());
//!
//!     // tear down the tables using the correct methods to free memory
//!     // SAFETY: tables were allocated via `empty_tables`.
//!     unsafe { tskit2tskit::teardown_tables(tables) };
//!     Ok(())
//! }).unwrap();
//! ```
//!
//! ## Working with an encapsulation of [`tskit::TableCollection`].

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

use pyo3::prelude::*;

#[derive(Debug)]
#[non_exhaustive]
/// Error type.
///
/// Convertible to [`pyo3::PyErr`] via [`std::convert::From`].
pub enum Error {
    /// Error from tskit (rust).
    TskitRust(tskit::TskitError),
    /// Error from Python interpreter.
    Python(PyErr),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Python(err) => write!(f, "{err:?}"),
            Error::TskitRust(err) => write!(f, "{err:?}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<tskit::TskitError> for Error {
    fn from(value: tskit::TskitError) -> Self {
        Self::TskitRust(value)
    }
}

impl From<PyErr> for Error {
    fn from(value: PyErr) -> Self {
        Self::Python(value)
    }
}

impl From<Error> for PyErr {
    fn from(value: Error) -> Self {
        match value {
            Error::TskitRust(e) => pyo3::exceptions::PyRuntimeError::new_err(format!("{e:?}")),
            Error::Python(e) => e,
        }
    }
}

/// Holds an instance of [`tskit::TableCollection`] and
/// a Python `_tskit.TableCollection` (the so-called
/// "low-level" implementation of a table collection.)
pub struct TableCollectionHolder {
    tables: Option<tskit::TableCollection>,
    pytables: Option<Py<PyAny>>,
}

impl Drop for TableCollectionHolder {
    fn drop(&mut self) {
        if self.tables.is_some() {
            let t = self.tables.take().unwrap();
            let _ = t.into_mut_ptr();
        }
        if self.pytables.is_some() {
            self.pytables.take().unwrap();
        }
    }
}

impl TableCollectionHolder {
    /// Constructor
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pyo3::prelude::*;
    /// # use tskit2tskit::TableCollectionHolder;
    /// Python::attach(|py| {
    ///     let _ = TableCollectionHolder::new(py, 100.0).unwrap();
    /// });
    /// ```
    pub fn new<'py, P: Into<tskit::Position>>(
        py: Python<'py>,
        sequence_length: P,
    ) -> Result<Self, Error> {
        let sequence_length: f64 = sequence_length.into().into();
        if !sequence_length.is_finite() || sequence_length <= 0.0 {
            return Err(tskit::TskitError::ValueError {
                got: sequence_length.to_string(),
                expected: "sequence_length >= 0.0".to_string(),
            })?;
        }
        let ll_tskit = py.import("_tskit")?;
        let pytables = ll_tskit
            .getattr("TableCollection")?
            .call1((sequence_length,))?
            .unbind();
        // SAFETY: initialization happened just fine on the Python side
        let ptr = unsafe { read_tsk_ptr(pytables.as_ptr()) };
        assert!(!ptr.is_null());
        // SAFETY: ptr is not NULL
        assert!(!unsafe { *ptr }.is_null());
        // SAFETY: nothing is NULL
        // The pointee has been initialized w/o error over in Python
        let tables =
            unsafe { tskit::TableCollection::new_from_raw(std::ptr::NonNull::new(*ptr).unwrap()) }?;
        Ok(Self {
            tables: Some(tables),
            pytables: Some(pytables),
        })
    }

    /// Shared reference to [`tskit::TableCollection`]
    ///
    /// # Examples
    /// ```rust
    /// # use pyo3::prelude::*;
    /// # use tskit2tskit::TableCollectionHolder;
    /// Python::attach(|py| {
    ///     let holder = TableCollectionHolder::new(py, 100.0).unwrap();
    ///     let _: &tskit::TableCollection = holder.as_rust();
    /// });
    /// ```
    pub fn as_rust(&self) -> &tskit::TableCollection {
        self.tables.as_ref().unwrap()
    }

    /// Exclusive (mutable) reference to [`tskit::TableCollection`]
    ///
    /// # Safety
    ///
    /// This function may lead to undefined behavior by allowing modification
    /// of the tables when there is an ABI mismatch between `tskit-rust`
    /// and `tskit-python`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pyo3::prelude::*;
    /// # use tskit2tskit::TableCollectionHolder;
    /// Python::attach(|py| {
    ///     let mut holder = TableCollectionHolder::new(py, 100.0).unwrap();
    ///     // SAFETY: mutating the tables is safe if the rust and python side
    ///     // use the same ABI.
    ///     let tables: &mut tskit::TableCollection = unsafe { holder.as_mut_rust() };
    ///     tables.add_node(tskit::NodeFlags::IS_SAMPLE, 0.0, -1, 1).unwrap();
    /// });
    /// ```
    pub unsafe fn as_mut_rust(&mut self) -> &mut tskit::TableCollection {
        self.tables.as_mut().unwrap()
    }

    fn into_ll_python_tables(self) -> Py<PyAny> {
        let mut t = self;
        let mut tables = None;
        let mut pytables = None;
        std::mem::swap(&mut t.tables, &mut tables);
        std::mem::swap(&mut t.pytables, &mut pytables);
        let _ = tables.unwrap().into_mut_ptr();
        pytables.unwrap()
    }

    /// Consume and return a *python* `tskit.TableCollection`.
    /// # Examples
    ///
    /// ```rust
    /// # use pyo3::prelude::*;
    /// # use tskit2tskit::TableCollectionHolder;
    /// Python::attach(|py| {
    ///     let holder = TableCollectionHolder::new(py, 100.0).unwrap();
    ///     let pytables = holder.into_python_tables(py).unwrap();
    ///     pyo3::py_run!(py, pytables, "import tskit; assert isinstance(pytables,
    ///     tskit.TableCollection)");
    /// });
    /// ```
    pub fn into_python_tables(self, py: Python<'_>) -> Result<Py<PyAny>, Error> {
        let pytables = self.into_ll_python_tables();
        let tskit_mod = py.import("tskit")?;
        let kwargs = pyo3::types::PyDict::new(py);
        kwargs.set_item("ll_tables", &pytables)?;
        let tskit_py_tables = tskit_mod
            .getattr("TableCollection")?
            .call((), Some(&kwargs))?
            .unbind();
        Ok(tskit_py_tables)
    }

    /// Consume and return a *python* `tskit.TreeSequence`.
    /// # Examples
    ///
    /// ```rust
    /// # use pyo3::prelude::*;
    /// # use tskit2tskit::TableCollectionHolder;
    /// Python::attach(|py| {
    ///     let holder = TableCollectionHolder::new(py, 100.0).unwrap();
    ///     let pytreeseq = holder.into_python_tree_sequence(py).unwrap();
    ///     pyo3::py_run!(py, pytreeseq, "import tskit; assert isinstance(pytreeseq,
    ///     tskit.TreeSequence)");
    /// });
    /// ```
    pub fn into_python_tree_sequence(self, py: Python<'_>) -> Result<Py<PyAny>, Error> {
        let pytables = self.into_python_tables(py)?;
        let pytables = pytables.bind(py);
        let ts = pytables.call_method0("tree_sequence")?;
        Ok(ts.unbind())
    }
}

/// Create an empty [`tskit::TableCollection`] whose memory
/// was allocated by a Python interpreter.
///
/// In order to tear down the tables and avoid leaks and/or
/// undefined behavior:
///
/// * The return value needs to be torn down and deallocated via
///   [`teardown_tables`].
/// * OR it may be converted to a Python-side tree sequence
///   via [`tables2treeseq`]
///
/// # Examples
///
/// ```rust
/// # use pyo3::prelude::*;
/// Python::attach(|_| {
///    let t = tskit2tskit::empty_tables(1e6).unwrap();
///    // SAFETY: t was initialized with `empty_tables`.
///    unsafe { tskit2tskit::teardown_tables(t) };
/// })
/// ```
pub fn empty_tables<P: Into<tskit::Position>>(
    sequence_length: P,
) -> Result<tskit::TableCollection, tskit::TskitError> {
    let ptr = unsafe {
        pyo3::ffi::PyMem_Malloc(std::mem::size_of::<tskit::bindings::tsk_table_collection_t>())
            .cast::<tskit::bindings::tsk_table_collection_t>()
    };
    unsafe {
        tskit::bindings::tsk_table_collection_init(ptr, 0);
        (*ptr).sequence_length = sequence_length.into().into();
    }
    unsafe { tskit::TableCollection::new_from_raw(std::ptr::NonNull::new(ptr).unwrap()) }
}

/// Tear down and deallocate the pointer to a table collection.
///
/// # Safety
///
/// * `tables` *should* have been created via [``empty_tables``].
/// * `tables` **must** have been initialzed with a pointer allocated by ``PyMem_Malloc``.
///
/// # Examples
///
/// See [`empty_tables`] for examples.
pub unsafe fn teardown_tables(tables: tskit::TableCollection) {
    let ptr = tables.into_mut_ptr().unwrap();
    // SAFETY: ptr is not NULL and all tskit::TableCollection contain initialized
    // tsk_table_collection_t
    let rv = unsafe { tskit::bindings::tsk_table_collection_free(ptr.as_ptr()) };
    assert_eq!(rv, 0);
    // SAFETY:
    // * this is the correct free function if PyMem_Malloc was the allocator
    // * ptr is not NULL
    unsafe { pyo3::ffi::PyMem_Free(ptr.as_ptr().cast::<libc::c_void>()) }
}

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

/// Zero-copy conversion from rust TableCollection to Python TreeSequence
///
/// # Errors
///
/// * If the input tables do not represent a valid
///   TreeSequence.
/// * If an error occurs during teardown of a pointer obtained
///   from `tskit-python`. In this case, this function will call
///   `PyMem_Free` on the pointer obtained from `rust_tables`.
///   
/// # Safety
///
/// * The `_tskit.TableCollection` must be based on a tsk_table_collection_t
///   whose ABI is identical to that used to build tskit-rust
/// * `rust_tables` must have been constructed using a pointer allocated by
///   `PyMem_Malloc`.
///
/// # Note
///
/// * It is undefined behavior to pass tables obtained via the
///   return value of [`tskit::TreeSequence::simplify`] to this function.
///   The underlying pointer to those tables has **not** been allocated
///   by the Python memory allocator!
pub unsafe fn tables2treeseq(
    py: Python<'_>,
    rust_tables: tskit::TableCollection,
) -> Result<Py<PyAny>, Error> {
    let sequence_length: f64 = rust_tables.sequence_length().into();

    // Create an empty _tskit.TableCollection
    let ll_tskit = py.import("_tskit")?;
    let py_ll_tc = ll_tskit
        .getattr("TableCollection")?
        .call1((sequence_length,))?;

    let p: std::ptr::NonNull<tskit::bindings::tsk_table_collection_t> = rust_tables
        .into_mut_ptr()
        .ok_or(pyo3::exceptions::PyRuntimeError::new_err(
            "got a NULL pointer from the rust side...",
        ))?;
    unsafe {
        // 1. Get a pointer to the pointer to the Python-side
        //    TableCollection
        let py_obj_ptr = py_ll_tc.as_ptr();
        let dest_ptr = read_tsk_ptr(py_obj_ptr);
        // 2. Tear down the contents of the Python-side tables
        let rv = tskit::bindings::tsk_table_collection_free(*dest_ptr);
        if rv != 0 {
            let msg = tskit::error::get_tskit_error_message(rv);
            // Avoid leaks!!
            pyo3::ffi::PyMem_Free(p.as_ptr().cast::<libc::c_void>());
            Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                "error tearing down ll_tables from _tskit: {msg}"
            )))?;
        }
        // 3. Deallocate the Python-side pointer
        pyo3::ffi::PyMem_Free(*dest_ptr as *mut libc::c_void);
        // 4. Rebind the Python-side pointer
        *dest_ptr = p.as_ptr();
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

/// Zero-copy conversion of rust TreeSequence to Python TreeSequence.
///
/// # Safety
///
/// * The Python type `_tskit.TableCollection` must be based on a tsk_table_collection_t
///   whose ABI is identical to that used to build tskit-rust
/// * `rust_treeseq` must be based on a table collection initialized using
///   a pointer allocated via `PyMem_Malloc`.
///
/// # Notes
///
/// * The return value of [`tskit::TreeSequence::simplify`] is a *new*
///   [`tskit::TreeSequence`] whose tables pointer has been allocated by
///   `malloc` (the C memory allocator). It is undefined behavior (UB)
///   to pass such a tree sequence to this function!
/// * It is also UB to dump the tables from such a tree sequence and send
///   them to [`tables2treeseq`].
pub unsafe fn treeseq2treeseq(
    py: Python<'_>,
    rust_treeseq: tskit::TreeSequence,
) -> Result<Py<PyAny>, Error> {
    let rust_tables = rust_treeseq.dump_tables()?;
    unsafe { tables2treeseq(py, rust_tables) }
}

#[test]
fn demonstrate_memory_sharing() {
    Python::attach(|py| {
        let holder = TableCollectionHolder::new(py, 100.).unwrap();

        // Use Python API to add a row
        let pt = &holder.pytables;
        pyo3::py_run!(py, pt, "pt.nodes.add_row(0, 0.0, -1, -1)");
        assert_eq!(holder.as_rust().nodes().num_rows(), 1);
    })
}

#[test]
fn demonstrate_drop() {
    Python::attach(|py| {
        let holder = TableCollectionHolder::new(py, 100.).unwrap();
        drop(holder)
    })
}

#[test]
fn test_pytables_return_type() {
    Python::attach(|py| {
        let holder = TableCollectionHolder::new(py, 100.).unwrap();

        // Use Python API to add a row
        let pt = holder.into_python_tables(py).unwrap();
        pyo3::py_run!(
            py,
            pt,
            "import tskit; assert isinstance(pt, tskit.TableCollection)"
        );
    })
}

#[test]
fn test_err_tskit() {
    let e: Error = tskit::TskitError::ErrorCode {
        code: tskit::bindings::TSK_ERR_NULL_CHILD,
    }
    .into();
    let _ = e.to_string();
    let _: PyErr = e.into();
}

#[test]
fn test_err_py() {
    let e: Error = pyo3::exceptions::PyRuntimeError::new_err("boo").into();
    println!("{e}");
    let _: PyErr = e.into();
}

#[test]
fn holder_invalid_seqlen() {
    Python::attach(|py| {
        assert!(TableCollectionHolder::new(py, -1.).is_err());
    })
}
