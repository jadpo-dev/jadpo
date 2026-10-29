# `query.nearest_type`

`nearest` measuring from something it can't: a value of another vector
type than the field's, which may come from another model; a field that
isn't a vector; an order beside it, which no index serves; or a query
whose result isn't `List<Nearest<T>>`.

```jadpo check project=e-nearest-type expect=query.nearest_type
/// A help article's meaning, as the configured embedding model makes it.
type ArticleMeaning = Vector {
    dimensions: 1536
}

/// A customer's question's meaning, as the older model made it.
type QuestionMeaning = Vector {
    dimensions: 1536
}

/// An article on the help site, with its meaning once it is embedded.
entity HelpArticle {
    id: Uuid
    body_meaning: ArticleMeaning?
}

/// The five articles nearest a question.
query closest_articles(question_meaning: QuestionMeaning) -> List<Nearest<HelpArticle>> {
    from: HelpArticle
    order: [HelpArticle.body_meaning nearest question_meaning]
    limit: 5
}
```

```text output
error: `nearest` compares `HelpArticle.body_meaning` with a `QuestionMeaning`
 --> main.jadpo:20:46
   |
20 |     order: [HelpArticle.body_meaning nearest question_meaning]
   |                                              ^^^^^^^^^^^^^^^^ a `QuestionMeaning`, not an `ArticleMeaning`
  reason: A vector type stands for the model that made its vectors and how they are compared, so `nearest` measures from a vector of the field's own type; a `QuestionMeaning` may come from another model, and its distance from an `ArticleMeaning` would mean nothing.
  next: Pass an `ArticleMeaning`, made by the same model (agent)
  rule: query.nearest_type
  help: docs/errors/query.nearest_type.md

check failed: 1 error, 1 file
```
