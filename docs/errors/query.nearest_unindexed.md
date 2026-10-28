# `query.nearest_unindexed`

`nearest` on a vector field whose type has no `dimensions:`: a plain
`Vector`, or an `Embedding`, which keeps its vector without a length.
pgvector builds an HNSW index only on a column of fixed length.

```jadpo check project=e-nearest-unindexed expect=query.nearest_unindexed
/// A reply an agent is still writing, with its meaning as a model made it.
entity DraftReply {
    id: Uuid
    reply_meaning: Vector
}

/// The five replies nearest one being written.
query similar_drafts(reply_meaning: Vector) -> List<Nearest<DraftReply>> {
    from: DraftReply
    order: [DraftReply.reply_meaning nearest reply_meaning]
    limit: 5
}
```

```text output
error: `DraftReply.reply_meaning` has no fixed length, so no index can order it by nearness
 --> main.jadpo:10:13
   |
10 |     order: [DraftReply.reply_meaning nearest reply_meaning]
   |             ^^^^^^^^^^^^^^^^^^^^^^^^ a `Vector` without `dimensions:`
  reason: A nearest-neighbour query reads an HNSW index, which pgvector builds only on a vector column of fixed length, and `DraftReply.reply_meaning` is a `Vector`, kept without one.
  next: Give `DraftReply.reply_meaning` a type built on `Vector` with the model's `dimensions:` (agent)
  rule: query.nearest_unindexed
  help: docs/language/21-ai.html#query-nearest-unindexed

check failed: 1 error, 1 file
```
