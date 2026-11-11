# Zero-copy data exchange of tskit types from rust to Python.

This crate facilitates data sharing betweeen [tskit-rust](https://docs.rs/tskit) and [tskit-python](https://tskit.dev/tskit/docs/stable/introduction.html).

## Scope

* Provide methods for transferring a `TableCollection` from rust to python.
* The data transfer is zero-copy.

## Details

The implementation of this crate is based on understanding the internals of both `tskit-python` and `tskit-c`.
This crate relies on the C-side definition of the **python** `TableCollection` type!
We specifically rely on the fact that the pointer to the `tskit-c` `TableCollection` type immediately follows the python object head pointer.
The relevant file in `tskit-python` is `_tskitmodule.c`, which contains the following:

```c
typedef struct _TableCollection {
    PyObject_HEAD
    tsk_table_collection_t *tables;
} TableCollection;
```

The key here is that a Python table collection contains a pointer to a `tsk_table_collection_t *` that has been allocated with `PyMem_Malloc`.
Given the definition in `_tskitmodule.c`, we can access that pointer on the rust side and create a table collection in rust that does *not* own its data.
Doing so is fundamentally `unsafe` and the API documentation outlines how to avoid undefined behavior.

## Guidelines for use

### `rust` dependencies

This crate depends on:

* [`tskit` rust binding](https://docs.rs/tskit)
* [`pyo3`](https://docs.rs/pyo3)

This crate pins to minimum versions of both dependencies and does not re-export their APIs.
The `cargo` dependency resolver will be sufficient for picking the correct versions of these dependencies in downstream projects.

### `python` dependencies

This crate must be used in a `venv` containing `tskit-python`.
See the `GitHub` workflow files for guidance on versions of `tskit-python`.

It is critical to appreciate the following complication:

* The `tskit` `rust` bindings are compiled with a specific version of `tskit-c`.
  (That version of `tskit-c` is bundled with the `rust` bindings.)
* `tskit-python` releases are also compiled with a specific version of `tskit-c`.

For things to work, both `rust` and `python` must be based on ABI-compatible versions of `tsk_table_collection_t`!
There is *no way to test this* and incompatibility can only be detected at run time.

### Testing downstream projects

We **strongly** suggest that downstream projects test using debug versions of the Python interpreter!
Importantly, most installations of Python are **not** debug versions.
We expect developers of downstream tools to understand how to install debug versions on their supported platforms.
([`uv`](https://docs.astral.sh/uv/) is one of many methods for installing debug versions of the Python interpreter but you may experience linkage issues at runtime.)
The reason for this suggestion is that certain types of memory errors can only be reliably caught using the debug interpreter.

## Alternative approaches

In general, it is far simpler and far more memory safe to to all work using the rust API, dump tables (or a tree sequence) to a "`.trees`" file, and then load that file for downstream work in `tskit-python`.
In reality, the primary (and perhaps *only*?) use cases for this crate are:

* To avoid the additional I/O entailed by data exchange through files.
* To provide a single "product" (such as a Python package) that enables a complete work flow.
  This is desirable in cases where the rust side generates tables with metadata and wants to
  define a validated metadata schema, which is best done using `tskit-python`.
  See [here](https://tskit.dev/tskit/docs/stable/metadata.html) for more about metadata and schema.

## The (current) reality of the lack of memory safety.

It is technically correct that this crate relies on `unsafe` memory operations.
However, the reality on the ground is that the `tskit-c` type `tsk_table_collection_t` has been stable for a long time now and any changes to that type would break a LOT of peoples' projects.
It is thus *reasonable* to claim that the memory safety issues that this crate needs to worry about are *unlikely* to be an issue.
(And now that we have written the previous sentence, a layout change to that `struct` is all but guaranteed! ;) ).

## Rationale for some design decisions

It is also possible that our marking of certain functions `unsafe` is overly strict!
The rationale for our choice is as follows.
Fundamentally, access to tables in rust requires the conversion of a *pointer* to a `&` or `& mut`.
These operations are safe if the criteria [here](https://doc.rust-lang.org/std/ptr/index.html#pointer-to-reference-conversion) are met.
The following statement from that list is the devilish one:

> * The pointer must point to a valid value of type T.

When dealing solely with a rust `TableCollection`, and not using any `unsafe` API, we know that this is true because the rust API takes great pains to make sure of this.
The problem for this crate is the Python types are based on a definition of `tsk_table_collection_t` that resides in the compiled library backing `tskit-python`.
We *cannot* know at *compile time* if the layout of that type in `tskit-rust` is the same!
As a result, we can always take a reference to a `tskit::TableCollection` but the undefined behavior may only occur when that reference is used.
Thus, we consider any block of code making use of a reference obtained by this crate as `unsafe`.

# Core functionality

## Working with an encapsulation of `tskit::TableCollection`.

`SharedTableCollection` is an opaque wrapper around a rust
`tskit::TableCollection` and a Python `tskit._tskit.TableCollection` 
(the low-level wrapper around the C type `tsk_table_collection_t`).
By means of some `unsafe` magic, these two objects *share* a pointer
to the same `tsk_table_collection_t`.

The high-level idea is that we can modify the tables in rust and then
return them to Python, consuming the holder instance.

By way of example:

```rust
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
        let mut holder = tskit2tskit::SharedTableCollection::new(py, 100.0).unwrap();
        // Release the gil to work only on the rust side of the data,
        // potentially allowing other Python threads to run.
        py.detach(|| -> Result<(), PyErr> {
            // SAFETY: the code below is safe if tskit-rust and tskit-python
            // are built around the same layout for `tsk_table_collection_t`.
            Ok(unsafe {
                // In order to modify a table collection, we define a function
                // that operates on an exclusive reference to the tables.
                // The return value of this function is generic and therefore
                // up to developers of downstream code.
                // Here, we use a closure that returns a `Result`.
                // This operation is `unsafe` because an ABI mismatch between Python
                // and rust will lead to undefined behavior.
                holder.with_mut_tables(|t: &mut tskit::TableCollection| -> Result<(), tskit2tskit::Error> {
                    // Everything in this block is the standard tskit rust API.
                    // Note that the use of ? will convert and TskitError into
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
```
