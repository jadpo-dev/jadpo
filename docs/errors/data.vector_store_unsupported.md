# `data.vector_store_unsupported`

A field holding a vector, a vector type's or an `Embedding`'s, in an
entity whose `store:` isn't PostgreSQL. Vectors are stored and searched
by pgvector, and there is no fallback. It is said in place of Redis
entities' own `toolchain.not_built_yet`, since it stays when they are
built.

```jadpo check project=e-vector-store-unsupported file=app.jadpo
application {
    connections: {
        drafts: {
            driver: Driver.redis
            url: config.redis_url
        }
    }
}

config {
    redis_url: RedisUrl
}
```

```jadpo check project=e-vector-store-unsupported file=domains/support/types/article_meaning.jadpo hidden
/// A help article's meaning, as the configured embedding model makes it.
type ArticleMeaning = Vector {
    dimensions: 1536
}
```

```jadpo check project=e-vector-store-unsupported file=domains/support/entities/open_question.jadpo expect=data.vector_store_unsupported
use support/types { ArticleMeaning }

/// A question someone is still typing, kept for an hour.
entity OpenQuestion {
    id: Uuid
    question_meaning: ArticleMeaning

    store: drafts
}
```

```text output
error: `OpenQuestion.question_meaning` is a vector, which `drafts` can't store
 --> domains/support/entities/open_question.jadpo:6:5
  |
6 |     question_meaning: ArticleMeaning
  |     ^^^^^^^^^^^^^^^^ a vector, in Redis
  reason: A vector is kept in a column of pgvector, an extension of PostgreSQL, and `drafts` is a Redis connection. There is no fallback, since a search held anywhere else would behave differently from the one that was tested.
  next: Remove `store: drafts` to keep `OpenQuestion` in `main`, a PostgreSQL store (agent)
  rule: data.vector_store_unsupported
  help: docs/errors/data.vector_store_unsupported.md

check failed: 1 error, 3 files
```
