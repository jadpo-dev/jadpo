# `data.vector_store_unsupported`

A field holding a vector is on an entity kept in a store that isn't PostgreSQL.

## Why Jadpo refuses it

A vector is kept and searched by pgvector, an extension of PostgreSQL.
There is no fallback, because a search held anywhere else would behave
differently from the one that was tested. An entity in Redis can't hold
one, so Jadpo says so as soon as it reads the entity.

## What it looks like

`app.jadpo` declares `drafts`, a Redis connection:

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

The vector's type, `ArticleMeaning`, is declared in the domain's `types`
folder.

```jadpo check project=e-vector-store-unsupported file=domains/support/types/article_meaning.jadpo hidden
/// A help article's meaning, as the configured embedding model makes it.
type ArticleMeaning = Vector {
    dimensions: 1536
}
```

domains/support/entities/open_question.jadpo:

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
  help: https://jadpo.dev/docs/errors/data.vector_store_unsupported

check failed: 1 error, 3 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the entity can live in PostgreSQL**, remove `store:` to keep it in
`main`:

```jadpo fragment
entity OpenQuestion {
    id: Uuid
    question_meaning: ArticleMeaning
}
```

```jadpo check project=e-vector-store-fix-1 hidden
/// A help article's meaning, as the configured embedding model makes it.
type ArticleMeaning = Vector {
    dimensions: 1536
}

/// A question someone is still typing, kept for an hour.
entity OpenQuestion {
    id: Uuid
    question_meaning: ArticleMeaning
}
```

The question's meaning is now kept and searched in PostgreSQL.

**If the entity must stay in Redis**, keep its vector out of it, in an
entity of its own in PostgreSQL:

```jadpo fragment
/// The meaning of an open question.
entity OpenQuestionMeaning {
    id: Uuid
    question_meaning: ArticleMeaning
}
```

```jadpo check project=e-vector-store-fix-2 hidden
/// A help article's meaning, as the configured embedding model makes it.
type ArticleMeaning = Vector {
    dimensions: 1536
}

/// The meaning of an open question.
entity OpenQuestionMeaning {
    id: Uuid
    question_meaning: ArticleMeaning
}
```

The vector now lives in `main`, and the rest of the question can stay
where it is.

## Related

- [Without pgvector](../language/21-ai.md#without-pgvector)
- [Stores](../language/05-data.md#stores)
