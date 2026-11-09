# `query.nearest_page`

A query ordered by `nearest` returns a page of records.

## Why Jadpo refuses it

A page continues after a cursor, and a cursor holds the values of the
query's order. A distance is worked out for each call rather than stored,
and it changes as records are added and change, so the next page could skip
a record or show one again.

## What it looks like

main.jadpo:

```jadpo check project=e-nearest-page expect=query.nearest_page
/// A help article's meaning, as the configured embedding model makes it.
type ArticleMeaning = Vector {
    dimensions: 1536
}

/// An article on the help site, with its meaning once it is embedded.
entity HelpArticle {
    id: Uuid
    body_meaning: ArticleMeaning?
}

/// The articles nearest a question, twenty at a time.
query closest_articles(
    question_meaning: ArticleMeaning,
    after: PageCursor?
) -> Page<Nearest<HelpArticle>> {
    from: HelpArticle
    order: [HelpArticle.body_meaning nearest question_meaning]
    page: {
        size: 20
        after: after
    }
}
```

```text output
error: `closest_articles` returns its nearest records a page at a time
 --> main.jadpo:16:6
   |
16 | ) -> Page<Nearest<HelpArticle>> {
   |      ^^^^^^^^^^^^^^^^^^^^^^^^^^ a page of nearest records
  reason: A page continues after a cursor, and a cursor over distances isn't stable: as records are added and change, the next page could skip a record or show one again. A nearest-neighbour query returns one list, of at most 100.
  next: Return `List<Nearest<HelpArticle>>` with `limit:` in place of `page:` (agent)
  rule: query.nearest_page
  help: https://jadpo.dev/docs/errors/query.nearest_page

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If one list is enough**, return a `List` with `limit:`, of at most 100:

```jadpo fragment
query closest_articles(question_meaning: ArticleMeaning) -> List<Nearest<HelpArticle>> {
    ...
    limit: 20
}
```

```jadpo check project=e-nearest-page-fix-1 hidden
/// A help article's meaning, as the configured embedding model makes it.
type ArticleMeaning = Vector {
    dimensions: 1536
}

/// An article on the help site, with its meaning once it is embedded.
entity HelpArticle {
    id: Uuid
    body_meaning: ArticleMeaning?
}

/// The twenty articles nearest a question.
query closest_articles(question_meaning: ArticleMeaning) -> List<Nearest<HelpArticle>> {
    from: HelpArticle
    order: [HelpArticle.body_meaning nearest question_meaning]
    limit: 20
}
```

Callers get the nearest twenty, nearest first, in one list.

## Related

- [The nearest-neighbour query](../language/21-ai.md#the-nearest-neighbour-query)
- [Pages](../language/06-reads-and-writes.md#pages)
