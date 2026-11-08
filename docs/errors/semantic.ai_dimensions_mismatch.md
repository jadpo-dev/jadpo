# `semantic.ai_dimensions_mismatch`

The vector type an embedding is asked for has a different length from the model's embeddings, or `ai:` doesn't say how long those are.

## Why Jadpo refuses it

Embeddings of different lengths can't be compared, so a column that held
two kinds would give wrong search results without any error. `ai:` says
how long the model's embeddings are, in `embedding_dimensions:`, and every
vector type asked for has to match it. A change of model that changes the
length is then caught when you check, before a column holds two kinds.

## What it looks like

An application's model makes 1,536 numbers, and an action asks for a type
that holds 768:

app.jadpo:


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

domains/support/types/article_meaning.jadpo:


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

domains/support/actions/meaning_of_article.jadpo:


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
  help: https://jadpo.dev/docs/errors/semantic.ai_dimensions_mismatch

check failed: 1 error, 5 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

With no `embedding_dimensions:` in `ai:`, there is nothing to compare the
vector type with:

```jadpo check project=e-ai-dimensions-unset file=app.jadpo hidden
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
        limits: {
            per_day: {
                tokens: 2000000
            }
        }
    }
}
```

```jadpo check project=e-ai-dimensions-unset file=config.jadpo migrate=initial hidden
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

```jadpo check project=e-ai-dimensions-unset file=domains/support/types/article_meaning.jadpo hidden
/// A help article's meaning, as the older embedding model made it.
type ArticleMeaning = Vector {
    dimensions: 768
}
```

```jadpo check project=e-ai-dimensions-unset file=domains/support/failures/embedding_delayed.jadpo hidden
/// An article's meaning couldn't be made now; it is made again later.
type EmbeddingDelayed = Failure {
    kind: FailureKind.unavailable
}
```

domains/support/actions/meaning_of_article.jadpo:

```jadpo check project=e-ai-dimensions-unset file=domains/support/actions/meaning_of_article.jadpo expect=semantic.ai_dimensions_mismatch
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
error: `ArticleMeaning` has 768 dimensions, and `ai:` doesn't say how long the model's embeddings are
 --> domains/support/actions/meaning_of_article.jadpo:6:39
  |
6 |     return attempt ai.embed(body, as: ArticleMeaning) {
  |                                       ^^^^^^^^^^^^^^ 768 dimensions
  reason: `embedding_dimensions:` in `ai:` says how many numbers each of the model's embeddings has, and every vector type `ai.embed` answers as must have as many `dimensions:`, so a model change that alters the length is caught before a column holds two kinds of vector.
  next: Add `embedding_dimensions: 768` to `ai:` in `app.jadpo`, if that is the model's length (agent)
  rule: semantic.ai_dimensions_mismatch
  help: https://jadpo.dev/docs/errors/semantic.ai_dimensions_mismatch

check failed: 1 error, 5 files
```

## How to fix it

**If the model's length is right**, make the vector type the same length:

```jadpo fragment
type ArticleMeaning = Vector {
    dimensions: 1536
}
```

```jadpo check project=e-ai-dimensions-fix-1 file=domains/support/types/article_meaning.jadpo hidden
/// A help article's meaning, as the older embedding model made it.
type ArticleMeaning = Vector {
    dimensions: 1536
}
```

```jadpo check project=e-ai-dimensions-fix-1 file=app.jadpo hidden
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

```jadpo check project=e-ai-dimensions-fix-1 file=config.jadpo hidden
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

```jadpo check project=e-ai-dimensions-fix-1 file=domains/support/failures/embedding_delayed.jadpo hidden
/// An article's meaning couldn't be made now; it is made again later.
type EmbeddingDelayed = Failure {
    kind: FailureKind.unavailable
}
```

```jadpo check project=e-ai-dimensions-fix-1 file=domains/support/actions/meaning_of_article.jadpo hidden migrate=initial
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

`ArticleMeaning` now holds what the model makes. Vectors already stored at
the old length have to be made again.

**If the vector type's length is right**, say so in `ai:`, when the model
really makes vectors of that length:

```jadpo fragment
    ai: {
        connection: local_model
        models: {
            default: config.drafting_model
            embedding: config.search_model
        }
        embedding_dimensions: 768
        ...
    }
```

```jadpo check project=e-ai-dimensions-fix-2 file=app.jadpo hidden
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
        embedding_dimensions: 768
        limits: {
            per_day: {
                tokens: 2000000
            }
        }
    }
}
```

```jadpo check project=e-ai-dimensions-fix-2 file=config.jadpo hidden
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

```jadpo check project=e-ai-dimensions-fix-2 file=domains/support/types/article_meaning.jadpo hidden
/// A help article's meaning, as the older embedding model made it.
type ArticleMeaning = Vector {
    dimensions: 768
}
```

```jadpo check project=e-ai-dimensions-fix-2 file=domains/support/failures/embedding_delayed.jadpo hidden
/// An article's meaning couldn't be made now; it is made again later.
type EmbeddingDelayed = Failure {
    kind: FailureKind.unavailable
}
```

```jadpo check project=e-ai-dimensions-fix-2 file=domains/support/actions/meaning_of_article.jadpo hidden migrate=initial
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

The check now compares `ArticleMeaning` with 768, the length you have said
the model makes.

## Related

- [Asking for an embedding](../language/21-ai.md#asking-for-an-embedding)
- [Storing a vector](../language/21-ai.md#storing-a-vector)
