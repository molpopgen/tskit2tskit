# Zero-copy data exchange of tskit types from rust to Python.

This crate facilitates data sharing betweeen [tskit-rust](https://docs.rs/tskit) and [tskit-python](https://tskit.dev/tskit/docs/stable/introduction.html).

## Scope

* Provide methods for transferring a `TableCollection` from rust to python.
* The data transfer is zero-copy.

## Details

The implementation of this crate is based on understanding the internals of both `tskit-python` and `tskit-c`.
This crate relies on the C-side definition of the **python** `TableCollection` type!
We specifically rely on the fact that the pointer `tskit-c` `TableCollection` type immediately follows the python object head pointer.
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
