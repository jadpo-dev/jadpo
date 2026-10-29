# `semantic.ai_dimensions_mismatch`

`ai.embed`'s `as:` names a vector type with as many `dimensions:` as
`embedding_dimensions:` says the model makes, so a model change that
alters the length is caught before a column holds two kinds of vector.

```jadpo check project=e-ai-dimensions-mismatch file=app.jadpo
application {
    connections: {
        local_model: {
            driver: Driver.openai_compatible
            url: config.model_url
        }
    }
    ai: {
        connection: local_model
        models: {
            default: config.drafting_model
            embedding: config.search_model
        }
        embedding_dimensions: 1536
        limits: {
            per_day: {
                tokens: 2000000
            }
        }
    }
}
```

```jadpo check project=e-ai-dimensions-mismatch file=config.jadpo hidden migrate=initial
config {
    drafting_model: ModelId {
        default: "openai/gpt-4.1-mini"
    }
    model_url: Url {
        default: "http://localhost:8000/v1"
    }
    search_model: ModelId {
        default: "openai/text-embedding-3-small"
    }
}
```

```jadpo check project=e-ai-dimensions-mismatch file=domains/support/types/article_meaning.jadpo
/// A help article's meaning, as the older embedding model made it.
type ArticleMeaning = Vector {
    dimensions: 768
}
```

```jadpo check project=e-ai-dimensions-mismatch file=domains/support/failures/embedding_delayed.jadpo hidden
/// An article's meaning couldn't be made now; it is made again later.
type EmbeddingDelayed = Failure {
    kind: FailureKind.unavailable
}
```

```jadpo check project=e-ai-dimensions-mismatch file=domains/support/actions/meaning_of_article.jadpo expect=semantic.ai_dimensions_mismatch
use support/types { ArticleMeaning }
use support/failures { EmbeddingDelayed }

/// A help article's meaning, from its text.
action meaning_of_article(body: Markdown) fails EmbeddingDelayed -> ArticleMeaning {
    return attempt ai.embed(body, as: ArticleMeaning) {
        timed_out: EmbeddingDelayed
        over_budget: EmbeddingDelayed
        too_long: EmbeddingDelayed
    }
}
```

```text output
error: `ArticleMeaning` has 768 dimensions, and the model makes 1,536
 --> domains/support/actions/meaning_of_article.jadpo:6:39
  |
6 |     return attempt ai.embed(body, as: ArticleMeaning) {
  |                                       ^^^^^^^^^^^^^^ 768 dimensions, not 1,536
  reason: Each embedding the model makes has the 1,536 numbers `ai:`'s `embedding_dimensions:` says, and `ArticleMeaning` holds vectors of 768, so a column of it would hold two kinds of vector.
  next: Give `ArticleMeaning` `dimensions: 1536`, or set `embedding_dimensions: 768` in `ai:` if the model makes vectors that long (agent)
  rule: semantic.ai_dimensions_mismatch
  help: docs/errors/semantic.ai_dimensions_mismatch.md

check failed: 1 error, 5 files
```
