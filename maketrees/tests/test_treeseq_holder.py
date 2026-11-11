import maketrees
import tskit


def test_basic_sharing():
    ts = tskit.TableCollection(100.).tree_sequence()
    assert isinstance(ts, tskit.TreeSequence)
    holder = maketrees.TreeSeqHolder(ts)
    assert holder.sequence_length == 100.


def test_ownership():
    tables = tskit.TableCollection(100.)
    ts = tables.tree_sequence()
    assert isinstance(ts, tskit.TreeSequence)
    holder = maketrees.TreeSeqHolder(ts)
    assert holder.sequence_length == 100.
    del holder
    assert tables.sequence_length == 100.
    assert ts.tables.sequence_length == 100.
