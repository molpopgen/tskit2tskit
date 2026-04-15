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
    print(times)
    assert len([i for i in times if i == 0.0]) == 2
    assert len([i for i in times if i == 1.0]) == 1
