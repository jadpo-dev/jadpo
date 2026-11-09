# `query.nearest_exact`

A query orders by `nearest` on a vector too long for an index, so each call reads every record.

## Why Jadpo refuses it

A nearest-neighbour query normally reads an HNSW index, which pgvector
builds on a vector of up to 4,000 dimensions, at half precision. Past that
there is no index, so every call measures the distance to every record and
sorts them, which slows as the table grows.

For a table small enough to read in full that may be fine, so Jadpo reports
it and lets you say so with a `decline`.

## What it looks like

main.jadpo:

```jadpo check project=e-nearest-exact expect=query.nearest_exact
/// A paper's meaning, as the research team's own model makes it.
type PaperMeaning = Vector {
    dimensions: 5000
}

/// A research paper, with its meaning once it is embedded.
entity Paper {
    id: Uuid
    abstract_meaning: PaperMeaning?
}

/// The ten papers nearest a question.
query closest_papers(question_meaning: PaperMeaning) -> List<Nearest<Paper>> {
    from: Paper
    order: [Paper.abstract_meaning nearest question_meaning]
    limit: 10
}
```

```text output
error: `Paper.abstract_meaning` is too long to index, so `nearest` reads every `Paper`
 --> main.jadpo:15:13
   |
15 |     order: [Paper.abstract_meaning nearest question_meaning]
   |             ^^^^^^^^^^^^^^^^^^^^^^ 5,000 dimensions
  reason: pgvector's HNSW index takes a vector of up to 4,000 dimensions, at half precision, and `PaperMeaning` has 5,000; with no index, each call measures the distance to every `Paper` whose `abstract_meaning` is set and sorts them all, which slows as the table grows.
  next: Give `PaperMeaning` at most 4,000 `dimensions:`, asking the model for fewer with `embedding_dimensions:`, or write `decline query.nearest_exact: "<why>"` above it if this is deliberate (agent)
  rule: query.nearest_exact
  help: https://jadpo.dev/docs/errors/query.nearest_exact

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the model can give shorter vectors**, use at most 4,000 dimensions, asking the model for fewer with `embedding_dimensions:`:

```jadpo fragment
type PaperMeaning = Vector {
    ...
    dimensions: 3072
}
```

```jadpo check project=e-nearest-exact-fix-1 hidden
/// A paper's meaning, as the research team's own model makes it.
type PaperMeaning = Vector {
    dimensions: 3072
}

/// A research paper, with its meaning once it is embedded.
entity Paper {
    id: Uuid
    abstract_meaning: PaperMeaning?
}

/// The ten papers nearest a question.
query closest_papers(question_meaning: PaperMeaning) -> List<Nearest<Paper>> {
    from: Paper
    order: [Paper.abstract_meaning nearest question_meaning]
    limit: 10
}
```

The query now reads an HNSW index, at half precision.

**If the table is small enough to read in full**, decline the rule above the query's `order:`:

```jadpo fragment
    decline query.nearest_exact: "Under a thousand papers, read in full in a few milliseconds"
    order: [Paper.abstract_meaning nearest question_meaning]
```

```jadpo check project=e-nearest-exact-declined hidden
/// A paper's meaning, as the research team's own model makes it.
type PaperMeaning = Vector {
    dimensions: 5000
}

/// A research paper, with its meaning once it is embedded.
entity Paper {
    id: Uuid
    abstract_meaning: PaperMeaning?
}

/// The ten papers nearest a question.
query closest_papers(question_meaning: PaperMeaning) -> List<Nearest<Paper>> {
    from: Paper
    decline query.nearest_exact: "Under a thousand papers, read in full in a few milliseconds"
    order: [Paper.abstract_meaning nearest question_meaning]
    limit: 10
}
```

The query checks, and searches exactly, reading every paper that has a meaning on each call.

## Related

- [Vectors of more than 2,000 dimensions](../language/21-ai.md#vectors-of-more-than-2000-dimensions)
- [The vector index](../language/21-ai.md#the-vector-index)
- [Writing a decline](../language/01-project-layout-and-modules.md#writing-a-decline)
