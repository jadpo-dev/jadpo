# `query.nearest_unindexed`

A query orders by `nearest` on a vector field with no fixed length.

## Why Jadpo refuses it

A nearest-neighbour query reads an HNSW index, and pgvector builds one only
on a vector column of fixed length. A plain `Vector`, or an `Embedding`,
keeps its vector without a length, so no index could serve the query and
every call would read the whole table.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/query.nearest_unindexed

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An `Embedding`, which holds a vector with the model that made it, has no
fixed length either:

main.jadpo:

```jadpo check project=e-nearest-unindexed-embedding expect=query.nearest_unindexed
/// A reply an agent is still writing, with its meaning as a model made it.
entity DraftReply {
    id: Uuid
    reply_meaning: Embedding
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
   |             ^^^^^^^^^^^^^^^^^^^^^^^^ an `Embedding`
  reason: A nearest-neighbour query reads an HNSW index, which pgvector builds only on a vector column of fixed length, and `DraftReply.reply_meaning` is an `Embedding`, whose vector is kept without one.
  next: Give `DraftReply.reply_meaning` a type built on `Vector` with the model's `dimensions:` (agent)
  rule: query.nearest_unindexed
  help: https://jadpo.dev/docs/errors/query.nearest_unindexed

check failed: 1 error, 1 file
```

## How to fix it

**If you know the model's length**, give the field a type built on `Vector` with it:

```jadpo fragment
type ReplyMeaning = Vector {
    ...
    dimensions: 1536
}
```

```jadpo check project=e-nearest-unindexed-fix-1 hidden
/// A reply's meaning, as the configured embedding model makes it.
type ReplyMeaning = Vector {
    dimensions: 1536
}

/// A reply an agent is still writing, with its meaning as a model made it.
entity DraftReply {
    id: Uuid
    reply_meaning: ReplyMeaning
}

/// The five replies nearest one being written.
query similar_drafts(reply_meaning: ReplyMeaning) -> List<Nearest<DraftReply>> {
    from: DraftReply
    order: [DraftReply.reply_meaning nearest reply_meaning]
    limit: 5
}
```

The column now has a fixed length, so an index serves the query. The parameter takes the same type.

## Related

- [Storing a vector](../language/21-ai.md#storing-a-vector)
- [The vector index](../language/21-ai.md#the-vector-index)
