// This is a rust implementation of the example
// found in tskit-c
// NOTE: look for code in namespace tskit2tskit for
// why this example is interesting.

use clap::Parser;
use pyo3::prelude::*;
use rand::SeedableRng;
#[cfg(test)]
use rand::distributions::Distribution;
use rand::prelude::*;

#[derive(Debug)]
enum Error {
    Tskit(tskit::TskitError),
    Python(PyErr),
    Message(String),
}

impl From<tskit::TskitError> for Error {
    fn from(value: tskit::TskitError) -> Self {
        Self::Tskit(value)
    }
}

impl From<PyErr> for Error {
    fn from(value: PyErr) -> Self {
        Self::Python(value)
    }
}

impl From<tskit2tskit::Error> for Error {
    fn from(value: tskit2tskit::Error) -> Self {
        match value {
            tskit2tskit::Error::Python(msg) => Self::from(msg),
            tskit2tskit::Error::TskitRust(msg) => Self::from(msg),
            _ => panic!("unhandled error variant"),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tskit(e) => write!(f, "{e}"),
            Self::Message(m) => write!(f, "{m}"),
            Self::Python(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for Error {}

fn rotate_edges(bookmark: &tskit::types::Bookmark, tables: &mut tskit::TableCollection) {
    let num_edges = tables.edges().num_rows().as_usize();
    let left =
        unsafe { std::slice::from_raw_parts_mut((*tables.as_mut_ptr()).edges.left, num_edges) };
    let right =
        unsafe { std::slice::from_raw_parts_mut((*tables.as_mut_ptr()).edges.right, num_edges) };
    let parent =
        unsafe { std::slice::from_raw_parts_mut((*tables.as_mut_ptr()).edges.parent, num_edges) };
    let child =
        unsafe { std::slice::from_raw_parts_mut((*tables.as_mut_ptr()).edges.child, num_edges) };
    let mid = bookmark.edges().as_usize();
    left.rotate_left(mid);
    right.rotate_left(mid);
    parent.rotate_left(mid);
    child.rotate_left(mid);
}

fn simulate(
    py: Python<'_>,
    seed: u64,
    popsize: usize,
    num_generations: i32,
    simplify_interval: i32,
    update_bookmark: bool,
) -> Result<tskit2tskit::TableCollectionHolder, Error> {
    if popsize == 0 {
        return Err(Error::Message("popsize must be > 0".to_string()));
    }
    if num_generations == 0 {
        return Err(Error::Message("num_generations must be > 0".to_string()));
    }
    if simplify_interval == 0 {
        return Err(Error::Message("simplify_interval must be > 0".to_string()));
    }

    // Allocate a table collection backed by a pointer allocated from
    // PyMem_Malloc
    //let mut tables = tskit2tskit::empty_tables(1.0)?;
    let mut tables_holder = tskit2tskit::TableCollectionHolder::new(py, 1.0)?;
    let tables = tables_holder.as_mut_rust();
    // create parental nodes
    let mut parents_and_children = {
        let mut temp = vec![];
        let parental_time = f64::from(num_generations);
        for _ in 0..popsize {
            let node = tables.add_node(0, parental_time, -1, -1)?;
            temp.push(node);
        }
        temp
    };

    // allocate space for offspring nodes
    parents_and_children.resize(2 * parents_and_children.len(), tskit::NodeId::NULL);

    // Construct non-overlapping mutable slices into our vector.
    let (mut parents, mut children) = parents_and_children.split_at_mut(popsize);

    let parent_picker = rand::distributions::Uniform::new(0, popsize);
    let breakpoint_generator = rand::distributions::Uniform::new(0.0, 1.0);
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let mut bookmark = tskit::types::Bookmark::default();

    for birth_time in (0..num_generations).rev() {
        for c in children.iter_mut() {
            let bt = f64::from(birth_time);
            let child = tables.add_node(0, bt, -1, -1)?;
            let left_parent = parents
                .get(parent_picker.sample(&mut rng))
                .ok_or_else(|| Error::Message("invalid left_parent index".to_string()))?;
            let right_parent = parents
                .get(parent_picker.sample(&mut rng))
                .ok_or_else(|| Error::Message("invalid right_parent index".to_string()))?;
            let breakpoint = breakpoint_generator.sample(&mut rng);
            tables.add_edge(0., breakpoint, *left_parent, child)?;
            tables.add_edge(breakpoint, 1.0, *right_parent, child)?;
            *c = child;
        }

        if birth_time % simplify_interval == 0 {
            tables.sort(&bookmark, tskit::TableSortOptions::default())?;
            if update_bookmark {
                rotate_edges(&bookmark, tables);
            }
            if let Some(idmap) =
                tables.simplify(children, tskit::SimplificationOptions::default(), true)?
            {
                // remap child nodes
                for o in children.iter_mut() {
                    *o = idmap[usize::try_from(*o)?];
                }
            }
            if update_bookmark {
                bookmark.set_edges(tables.edges().num_rows());
            }
        }
        std::mem::swap(&mut parents, &mut children);
    }

    //tables.build_index()?;
    //let treeseq = tables.tree_sequence(tskit::TreeSequenceFlags::default())?;

    Ok(tables_holder)
}

#[derive(Clone, clap::Parser)]
struct SimParams {
    seed: u64,
    popsize: usize,
    num_generations: i32,
    simplify_interval: i32,
    treefile: Option<String>,
    #[clap(short, long, help = "Use bookmark to avoid sorting entire edge table.")]
    bookmark: bool,
}

fn main() -> Result<(), Error> {
    let params = SimParams::parse();

    // We will allocate the tables via Python allocator.
    // Therefore, we must run in an interpreter with the GiL
    // attached.
    // NOTE: doing this specific example this way is NOT useful!
    // As a CLI app, we should just call rust_treeseq.dump(...) and
    // move on. But the concept is generally useful -- one can write
    // a Python package in rust and then transfer data (zero-copy!)
    // to the "real" tskit-python so that analysis can take place!
    Python::attach(|py| -> Result<(), Error> {
        let rust_treeseq = simulate(
            py,
            params.seed,
            params.popsize,
            params.num_generations,
            params.simplify_interval,
            params.bookmark,
        )?;
        if let Some(filename) = params.treefile {
            // SAFETY: rust_treeseq was initialzed by tskit2tskit::empty_tables
            let python_treeseq = rust_treeseq.into_python_tree_sequence(py)?;
            pyo3::py_run!(py, python_treeseq, "print(python_treeseq)");
            python_treeseq.getattr(py, "dump")?.call1(py, (filename,))?;
        }
        Ok(())
    })
}
