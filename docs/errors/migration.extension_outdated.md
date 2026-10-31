# `migration.extension_outdated`

The database server's PostgreSQL extension, such as pgvector, is older than the project's migrations and queries need.

## Why Jadpo refuses it

A `nearest` query is served by an HNSW index, which the project's
migrations build. The query runs with pgvector's iterative index scans,
so a search that also filters still fills its limit. Those came in
pgvector 0.8.0, and an older pgvector rejects the setting that turns
them on, so the query would fail the first time it ran. `jadpo migrate
apply`, and `jadpo dev` and `jadpo test` as they migrate, check the
version the server offers before they run a migration that builds one,
and the app checks it again as it starts.

Jadpo won't run the query some other way on an older pgvector: a
filtered search could then give back fewer results than its limit, unlike
the one you tested. Upgrading the extension is a change to your database
server only you can make.

## What it looks like

A project whose help articles have a vector field, and a query that
finds the articles nearest a question:

```jadpo check project=e-extension-outdated file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

```jadpo check project=e-extension-outdated file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=e-extension-outdated file=domains/support/types/article_title.jadpo hidden
/// A help article's title.
type ArticleTitle = Text {
    max_length: 120
}
```

```jadpo check project=e-extension-outdated file=domains/support/types/article_meaning.jadpo hidden
/// A help article's meaning, as the configured embedding model makes it.
type ArticleMeaning = Vector {
    dimensions: 1536
    distance: VectorDistance.cosine
}
```

domains/support/entities/help_article.jadpo:

```jadpo check project=e-extension-outdated file=domains/support/entities/help_article.jadpo
use support/types { ArticleMeaning, ArticleTitle }

/// An article on the help site, with its meaning once it is embedded.
entity HelpArticle {
    id: Uuid
    title: ArticleTitle
    body_meaning: ArticleMeaning?
}
```
domains/support/queries/closest_articles.jadpo:

```jadpo check project=e-extension-outdated file=domains/support/queries/closest_articles.jadpo migrate=initial
use support/entities { HelpArticle }
use support/types { ArticleMeaning }

/// The five articles nearest a question.
query closest_articles(question_meaning: ArticleMeaning) -> List<Nearest<HelpArticle>> {
    from: HelpArticle
    order: [HelpArticle.body_meaning nearest question_meaning]
    limit: 5
}
```

Only a server with an older pgvector raises this error, which the docs
check can't count on having, so this output is copied from a real run
rather than checked. It is `jadpo migrate apply` on a Mac, with `DATABASE_URL`
naming a database on Postgres.app's PostgreSQL 16, which ships pgvector
0.7.0:

```sh
jadpo migrate apply
```

```text
error: `main` has pgvector 0.7.0, and its nearest-neighbour queries need 0.8.0
  reason: The project's migrations build an HNSW index for a `nearest` query, which runs with pgvector's iterative index scans so a filtered search still fills its limit; they came in pgvector 0.8.0, and the database server offers 0.7.0, so nothing was applied.
  next: Upgrade pgvector on the database server to 0.8.0 or later (the major hosted services offer it), then run the command again (human)
  rule: migration.extension_outdated
  help: https://jadpo.dev/docs/errors/migration.extension_outdated
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which database server the app runs on.

The version checked is the one the server loads, its
`pg_available_extensions.default_version`, not the one a database last
installed, so upgrading the server's pgvector is enough. An app with
vectors and no HNSW index, derived for a `nearest` query or declared,
runs on pgvector 0.7.

## How to fix it

**If the server's pgvector can be upgraded**, upgrade it to 0.8.0 or
later, then apply again:

```sh
jadpo migrate apply
```

The migration builds the index, and the query runs with iterative
scans. On Linux, pgvector is a package of its own, such as
`postgresql-16-pgvector`; on a hosted service, it is the version the
service offers.

## Related

- [Without pgvector](../language/21-ai.md#without-pgvector)
- [The nearest-neighbour query](../language/21-ai.md#the-nearest-neighbour-query)
- [`jadpo migrate`](../language/14-tooling.md#jadpo-migrate)
- [`migration.extension_missing`](migration.extension_missing.md)
