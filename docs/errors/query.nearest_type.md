# `query.nearest_type`

A query's `nearest` order measures from something it can't, sits beside another order, or doesn't agree with what the query returns.

## Why Jadpo refuses it

`nearest` measures how far a stored vector is from one the call supplies. A
vector type stands for the model that made it and how it is compared, so
both have to be the same type, or the distance would mean nothing.

The query's shape follows from that. An index gives its records back nearest
first, so `nearest` is the only order. Each record comes back with the
distance it measured, as `List<Nearest<T>>`, and only a `nearest` order can
give one.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/query.nearest_type

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

The field has to be a vector:

main.jadpo:

```jadpo check project=e-nearest-type-field expect=query.nearest_type
type Title = Text {
    max_length: 80
}

/// An article on the help site.
entity HelpArticle {
    id: Uuid
    title: Title
}

/// The five articles nearest a title.
query closest_articles(title: Title) -> List<Nearest<HelpArticle>> {
    from: HelpArticle
    order: [HelpArticle.title nearest title]
    limit: 5
}
```

```text output
error: `HelpArticle.title` isn't a vector, so records can't be ordered by nearness to it
 --> main.jadpo:14:13
   |
14 |     order: [HelpArticle.title nearest title]
   |             ^^^^^^^^^^^^^^^^^ `Title`
  reason: `nearest` orders records by how near a vector field is to a vector, by the distance its type measures; `HelpArticle.title` is `Title`.
  next: Order by a vector field, or by this one with `asc` or `desc` (agent)
  rule: query.nearest_type
  help: https://jadpo.dev/docs/errors/query.nearest_type

check failed: 1 error, 1 file
```

It measures from a parameter of the query, not a value written in the query:

main.jadpo:

```jadpo check project=e-nearest-type-value expect=query.nearest_type
/// A help article's meaning, as the configured embedding model makes it.
type ArticleMeaning = Vector {
    dimensions: 1536
}

/// An article on the help site, with its meaning once it is embedded.
entity HelpArticle {
    id: Uuid
    body_meaning: ArticleMeaning?
}

/// The five articles nearest nothing the caller gave.
query closest_articles() -> List<Nearest<HelpArticle>> {
    from: HelpArticle
    order: [HelpArticle.body_meaning nearest 5]
    limit: 5
}
```

```text output
error: `nearest` measures from `5`, which isn't a parameter
 --> main.jadpo:15:46
   |
15 |     order: [HelpArticle.body_meaning nearest 5]
   |                                              ^ not a parameter
  reason: `nearest` measures from a vector the call supplies, a parameter of `ArticleMeaning`, the type of `HelpArticle.body_meaning`.
  next: Add a `ArticleMeaning` parameter and measure from it (agent)
  rule: query.nearest_type
  help: https://jadpo.dev/docs/errors/query.nearest_type

check failed: 1 error, 1 file
```

`nearest` is the only order a query can have, since sorting by another field
as well would read and sort every match:

main.jadpo:

```jadpo check project=e-nearest-type-order expect=query.nearest_type
/// A help article's meaning, as the configured embedding model makes it.
type ArticleMeaning = Vector {
    dimensions: 1536
}

/// An article on the help site, with its meaning once it is embedded.
entity HelpArticle {
    id: Uuid
    body_meaning: ArticleMeaning?
}

/// The five articles nearest a question, then by id.
query closest_articles(question_meaning: ArticleMeaning) -> List<Nearest<HelpArticle>> {
    from: HelpArticle
    order: [HelpArticle.body_meaning nearest question_meaning, HelpArticle.id asc]
    limit: 5
}
```

```text output
error: `nearest` is the only order a query can have
 --> main.jadpo:15:64
   |
15 |     order: [HelpArticle.body_meaning nearest question_meaning, HelpArticle.id asc]
   |                                                                ^^^^^^^^^^^^^^^^^^ another order beside `nearest`
  reason: A nearest-neighbour query comes back nearest first, in the order its index finds the records; sorting by another field as well would read and sort every match, which no index serves.
  next: Remove `HelpArticle.id asc` from `order:`, and sort the results in code if they need it (agent)
  rule: query.nearest_type
  help: https://jadpo.dev/docs/errors/query.nearest_type

check failed: 1 error, 1 file
```

A query ordered by `nearest` returns `List<Nearest<T>>`, whose items hold
`item` and `distance`:

main.jadpo:

```jadpo check project=e-nearest-type-result expect=query.nearest_type
/// A help article's meaning, as the configured embedding model makes it.
type ArticleMeaning = Vector {
    dimensions: 1536
}

/// An article on the help site, with its meaning once it is embedded.
entity HelpArticle {
    id: Uuid
    body_meaning: ArticleMeaning?
}

/// The five articles nearest a question.
query closest_articles(question_meaning: ArticleMeaning) -> List<HelpArticle> {
    from: HelpArticle
    order: [HelpArticle.body_meaning nearest question_meaning]
    limit: 5
}
```

```text output
error: `closest_articles` orders by `nearest` but returns `List<HelpArticle>`
 --> main.jadpo:13:61
   |
13 | query closest_articles(question_meaning: ArticleMeaning) -> List<HelpArticle> {
   |                                                             ^^^^^^^^^^^^^^^^^ not `List<Nearest<T>>`
  reason: A nearest-neighbour query returns one bounded list of its records, each with the distance it measured: `List<Nearest<T>>`, whose items hold `item` and `distance`.
  next: Return `List<Nearest<HelpArticle>>` (agent)
  rule: query.nearest_type
  help: https://jadpo.dev/docs/errors/query.nearest_type

check failed: 1 error, 1 file
```

A `Nearest<T>` holds the distance a `nearest` order measured, so a query
ordered any other way can't return one:

main.jadpo:

```jadpo check project=e-nearest-type-no-order expect=query.nearest_type
/// A help article's meaning, as the configured embedding model makes it.
type ArticleMeaning = Vector {
    dimensions: 1536
}

/// An article on the help site, with its meaning once it is embedded.
entity HelpArticle {
    id: Uuid
    body_meaning: ArticleMeaning?
}

/// The five oldest articles.
query oldest_articles() -> List<Nearest<HelpArticle>> {
    from: HelpArticle
    order: [HelpArticle.id asc]
    limit: 5
}
```

```text output
error: `oldest_articles` returns `List<Nearest<HelpArticle>>` but doesn't order by `nearest`
 --> main.jadpo:13:28
   |
13 | query oldest_articles() -> List<Nearest<HelpArticle>> {
   |                            ^^^^^^^^^^^^^^^^^^^^^^^^^^ records with a distance
  reason: A `Nearest<T>` holds the distance a nearest-neighbour query measured, so only a query ordered by `nearest` returns one: `order: [<Entity>.<vector field> nearest <value>]`.
  next: Order by `nearest`, or return `List<HelpArticle>` (agent)
  rule: query.nearest_type
  help: https://jadpo.dev/docs/errors/query.nearest_type

check failed: 1 error, 1 file
```

## How to fix it

**If the vector comes from the same model**, pass it as the field's own type:

```jadpo fragment
query closest_articles(question_meaning: ArticleMeaning) -> List<Nearest<HelpArticle>> {
    ...
}
```

```jadpo check project=e-nearest-type-fix-1 hidden
/// A help article's meaning, as the configured embedding model makes it.
type ArticleMeaning = Vector {
    dimensions: 1536
}

/// An article on the help site, with its meaning once it is embedded.
entity HelpArticle {
    id: Uuid
    body_meaning: ArticleMeaning?
}

/// The five articles nearest a question.
query closest_articles(question_meaning: ArticleMeaning) -> List<Nearest<HelpArticle>> {
    from: HelpArticle
    order: [HelpArticle.body_meaning nearest question_meaning]
    limit: 5
}
```

The distance now compares two vectors from one model. A question that came from another model has to be embedded again with this one.

**If you want the records in another order**, order them with `asc` or `desc`, and return the records alone:

```jadpo fragment
query oldest_articles() -> List<HelpArticle> {
    ...
    order: [HelpArticle.id asc]
    ...
}
```

```jadpo check project=e-nearest-type-fix-2 hidden
/// A help article's meaning, as the configured embedding model makes it.
type ArticleMeaning = Vector {
    dimensions: 1536
}

/// An article on the help site, with its meaning once it is embedded.
entity HelpArticle {
    id: Uuid
    body_meaning: ArticleMeaning?
}

/// The five oldest articles.
query oldest_articles() -> List<HelpArticle> {
    from: HelpArticle
    order: [HelpArticle.id asc]
    limit: 5
}
```

To sort results that came from a `nearest` query, do it in code, after the query.

## Related

- [The nearest-neighbour query](../language/21-ai.md#the-nearest-neighbour-query)
- [Nearest first](../language/06-reads-and-writes.md#nearest-first)
- [Storing a vector](../language/21-ai.md#storing-a-vector)
