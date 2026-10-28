# `query.nearest_exact`

`nearest` on a vector type of over 4,000 dimensions. pgvector's HNSW
index holds at most 4,000, at half precision, so the query reads and
measures every record on each call. It is declinable: for a table small
enough to read in full, a decline above the query's `order:` accepts the
cost, and the query searches exactly.

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
  help: docs/language/21-ai.html#query-nearest-exact

check failed: 1 error, 1 file
```

Declined, it checks, and runs as one statement:

```jadpo check project=e-nearest-exact-declined
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

```sql
SELECT "id", "abstract_meaning", "abstract_meaning" <=> $1 AS "jadpo_distance"
FROM "paper"
WHERE "abstract_meaning" IS NOT NULL
ORDER BY "jadpo_distance", "id"
LIMIT 10
```
