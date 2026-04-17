#![doc = include_str!("../README.md")]

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
pub struct SharedTableCollection {
    tables: Option<tskit::TableCollection>,
    pytables: Option<Py<PyAny>>,
}

impl Drop for SharedTableCollection {
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

impl SharedTableCollection {
    /// Constructor
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pyo3::prelude::*;
    /// # use tskit2tskit::SharedTableCollection;
    /// Python::attach(|py| {
    ///     let _ = SharedTableCollection::new(py, 100.0).unwrap();
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

    unsafe fn as_rust(&self) -> &tskit::TableCollection {
        self.tables.as_ref().unwrap()
    }

    unsafe fn as_mut_rust(&mut self) -> &mut tskit::TableCollection {
        self.tables.as_mut().unwrap()
    }

    /// Perform operations on an immutable [`tskit::TableCollection`]
    ///
    /// # Safety
    ///
    /// * `tskit-rust` and `tskit-python` must have the same layout
    ///   for `tsk_table_collection_t`
    ///
    /// # Example
    ///
    /// ```rust
    /// # use pyo3::prelude::*;
    /// Python::attach(|py| {
    ///     let holder = tskit2tskit::SharedTableCollection::new(py, 100.).unwrap();
    ///     // Using a closure
    ///     let seqlen = unsafe {holder.with_tables(|t| t.sequence_length())};
    ///     assert_eq!(seqlen, 100.);
    ///     // More obscure method (member fns are just namespaced fns)
    ///     let seqlen = unsafe {holder.with_tables(tskit::TableCollection::sequence_length)};
    ///     assert_eq!(seqlen, 100.);
    /// });
    /// ```
    pub unsafe fn with_tables<F, R>(&self, mut f: F) -> R
    where
        F: FnMut(&tskit::TableCollection) -> R,
    {
        f(self.as_rust())
    }

    /// Perform operations on a mutable [`tskit::TableCollection`]
    ///
    /// # Safety
    ///
    /// * `tskit-rust` and `tskit-python` must have the same layout
    ///   for `tsk_table_collection_t`
    ///
    /// # Example
    ///
    /// ```rust
    /// # use pyo3::prelude::*;
    /// Python::attach(|py| {
    ///     let mut holder = tskit2tskit::SharedTableCollection::new(py, 100.).unwrap();
    ///     unsafe { holder.with_mut_tables(|t| {
    ///         t.add_node(0, 0.0, -1, -1).unwrap();
    ///     })}
    ///     # unsafe { holder.with_tables(|t| assert_eq!(t.nodes().num_rows(), 1))}
    /// });
    /// ```
    pub unsafe fn with_mut_tables<F, R>(&mut self, mut f: F) -> R
    where
        F: FnMut(&mut tskit::TableCollection) -> R,
    {
        f(self.as_mut_rust())
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
    /// # use tskit2tskit::SharedTableCollection;
    /// Python::attach(|py| {
    ///     let holder = SharedTableCollection::new(py, 100.0).unwrap();
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
    /// # use tskit2tskit::SharedTableCollection;
    /// Python::attach(|py| {
    ///     let holder = SharedTableCollection::new(py, 100.0).unwrap();
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

#[test]
fn demonstrate_memory_sharing() {
    Python::attach(|py| {
        let holder = SharedTableCollection::new(py, 100.).unwrap();

        // Use Python API to add a row
        let pt = &holder.pytables;
        pyo3::py_run!(py, pt, "pt.nodes.add_row(0, 0.0, -1, -1)");
        assert_eq!(unsafe { holder.as_rust() }.nodes().num_rows(), 1);
    })
}

#[test]
fn demonstrate_drop() {
    Python::attach(|py| {
        let holder = SharedTableCollection::new(py, 100.).unwrap();
        drop(holder)
    })
}

#[test]
fn test_pytables_return_type() {
    Python::attach(|py| {
        let holder = SharedTableCollection::new(py, 100.).unwrap();

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
        assert!(SharedTableCollection::new(py, -1.).is_err());
    })
}

#[test]
fn closure_capture_mut() {
    Python::attach(|py| {
        let mut holder = SharedTableCollection::new(py, 10.).unwrap();
        let nodes = unsafe {
            holder.with_mut_tables(|t| {
                let mut nodes = vec![];
                let n = t.add_node(0, 1., -1, -1).unwrap();
                nodes.push(n);
                nodes
            })
        };
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0], 0);
    })
}

#[test]
fn closure_capture_mut_thru_fn() {
    fn add_node(t: &mut tskit::TableCollection) -> Vec<tskit::NodeId> {
        let mut nodes = vec![];
        let n = t.add_node(0, 1., -1, -1).unwrap();
        nodes.push(n);
        nodes
    }
    Python::attach(|py| {
        let mut holder = SharedTableCollection::new(py, 10.).unwrap();
        let nodes = unsafe { holder.with_mut_tables(add_node) };
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0], 0);
    })
}

#[test]
fn mutable_closure_capture_mut() {
    Python::attach(|py| {
        let mut holder = SharedTableCollection::new(py, 10.).unwrap();
        let mut nodes = vec![];
        unsafe {
            holder.with_mut_tables(|t| {
                let n = t.add_node(0, 1., -1, -1).unwrap();
                nodes.push(n);
            });
        };
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0], 0);
    })
}

#[test]
fn closure_shared_ref() {
    Python::attach(|py| {
        let holder = SharedTableCollection::new(py, 100.).unwrap();
        let seqlen = unsafe { holder.with_tables(|t| t.sequence_length()) };
        assert_eq!(seqlen, 100.);
        let seqlen = unsafe { holder.with_tables(tskit::TableCollection::sequence_length) };
        assert_eq!(seqlen, 100.);
    });
}

#[test]
fn closure_returning_error() {
    Python::attach(|py| {
        let mut holder = SharedTableCollection::new(py, 1.0).unwrap();
        unsafe {
            holder.with_mut_tables(|t| -> Result<tskit::NodeId, Error> {
                // map tskit error type to that of this crate
                Ok(t.add_node(0, 0., -1, -1)?)
            })
        }
        .unwrap();
    })
}
