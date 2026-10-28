# `query.nearest_page`

A query ordered by `nearest` returning `Page<T>`. A page continues after
a cursor, and a cursor over distances isn't stable as records are added
and change.

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
  help: docs/errors/query.nearest_page.md

check failed: 1 error, 1 file
```
