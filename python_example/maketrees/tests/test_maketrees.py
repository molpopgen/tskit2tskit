import maketrees
import tskit


def test_return_type():
    ts = maketrees.maketrees()
    assert isinstance(ts, tskit.TreeSequence)


def test_return_value():
    ts = maketrees.maketrees()
    assert ts.num_nodes == 3
    assert ts.num_edges == 2
    times = ts.tables.nodes.time
    assert len([i for i in times if i == 0.0]) == 2
    assert len([i for i in times if i == 1.0]) == 1


def test_metadata():
    ts = maketrees.make_treeseq_with_metadata()
    m = ts.mutation(0).metadata
    assert m['data'] == "I am a mutation"


def test_mutable_sharing_of_tables():
    tables = tskit.TableCollection(100)
    tables.nodes.add_row(0, 0.0, -1, -1)
    assert tables.nodes.num_rows == 1
    maketrees.clear_shared_tables(tables)
    assert tables.nodes.num_rows == 0


def test_raise():
    try:
        _ = maketrees.raise_error()
    except RuntimeError:
        pass
    except _:
        raise ValueError("expected RuntimeError")
