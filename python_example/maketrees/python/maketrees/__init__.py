import tskit

from ._maketrees import maketrees


def make_treeseq_with_metadata() -> tskit.TreeSequence:
    import maketrees._maketrees
    tables = maketrees._maketrees._make_tables_with_metadata()
    tables.mutations.metadata_schema = tskit.metadata.MetadataSchema(
        {
            "codec": "json",
            "type": "object",
            "name": "Mutation metadata",
            "properties": {"data": {"type": "string"}},
            "additionalProperties": False,
        })
    return tables.tree_sequence()
