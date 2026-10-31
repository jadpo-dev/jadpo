# `migration.extension_missing`

The project's migrations need a PostgreSQL extension, such as pgvector, that the database server doesn't offer or that the role applying them can't create.

## Why Jadpo refuses it

Some fields need an extension: a `Vector` field needs pgvector, and a
geography needs PostGIS. The first migration with such a column creates
the extension. `jadpo migrate apply`, and `jadpo dev` and `jadpo test`
as they migrate, check that the server offers every extension a pending
migration creates before they run anything, so a deploy never stops
halfway through a migration for want of one.

Jadpo won't keep vectors or geometries any other way: a search or a
distance worked out without the extension would behave differently from
the one you tested. Installing an extension, or choosing a server that
has it, is a decision about your database only you can make.

## What it looks like

A project whose help articles have a vector field:

```jadpo check project=e-extension-missing file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

```jadpo check project=e-extension-missing file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=e-extension-missing file=domains/support/types/article_title.jadpo hidden
/// A help article's title.
type ArticleTitle = Text {
    max_length: 120
}
```

```jadpo check project=e-extension-missing file=domains/support/types/article_meaning.jadpo hidden
/// A help article's meaning, as the configured embedding model makes it.
type ArticleMeaning = Vector {
    dimensions: 1536
    distance: VectorDistance.cosine
}
```

domains/support/entities/help_article.jadpo:

```jadpo check project=e-extension-missing file=domains/support/entities/help_article.jadpo migrate=initial
use support/types { ArticleMeaning, ArticleTitle }

/// An article on the help site, with its meaning once it is embedded.
entity HelpArticle {
    id: Uuid
    title: ArticleTitle
    body_meaning: ArticleMeaning?
}
```
Only a server without pgvector raises this error, and the docs check's
server has it, so this output is copied from a real run rather than
checked. It is `jadpo migrate apply` on a Mac, with `DATABASE_URL`
naming a database on a PostgreSQL 16 installed without pgvector:

```sh
jadpo migrate apply
```

```text
error: `main` has no `vector` extension
  reason: `vector` is pgvector, which stores and searches the project's vectors; there is no fallback, since a search held anywhere else would behave differently from the one that was tested. The project's migrations create it, and its database server neither has it nor offers it, so nothing was applied.
  next: Install pgvector on the database server, or use a server that has it (the major hosted services do), then run the command again (human)
  rule: migration.extension_missing
  help: https://jadpo.dev/docs/errors/migration.extension_missing
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which database server the app runs on.

A server that offers the extension, reached as a role that isn't allowed
to create it, gives the same error saying so. pgvector and PostGIS are
not trusted extensions, so only a superuser, or a hosted service's own
admin role, can create them. This output is copied from a real run too:
the same command against Postgres.app's PostgreSQL 16, which has
pgvector, as a role made with `CREATE ROLE app LOGIN` that owns the
database:

```sh
jadpo migrate apply
```

```text
error: `main` has no `vector` extension, and its role can't create it
  reason: `vector` is pgvector, which stores and searches the project's vectors; there is no fallback, since a search held anywhere else would behave differently from the one that was tested. The project's migrations create it, and its database server offers it, but the role applying migrations isn't allowed to create it, so nothing was applied.
  next: Have a role allowed to create it run `CREATE EXTENSION vector` once in this database, then run the command again (human)
  rule: migration.extension_missing
  help: https://jadpo.dev/docs/errors/migration.extension_missing
```

## How to fix it

The question is whether the server has the extension.

**If the server doesn't have it**, install it there, or move to a
server that has it, then apply again:

```sh
jadpo migrate apply
```

The migration creates the extension, then the tables. On a Mac,
Postgres.app includes pgvector and PostGIS; on Linux, they are packages
of their own, such as `postgresql-16-pgvector`.

**If the server has it, and the role can't create it**, have a role that
can, such as the server's superuser, create it once in the database:

```sh
psql "$ADMIN_DATABASE_URL" -c 'CREATE EXTENSION vector'
```

`ADMIN_DATABASE_URL` is the same database's address with that role's
name and password. `jadpo migrate apply` then finds the extension in
place and runs the migration as the app's own role.

## Related

- [Without pgvector](../language/21-ai.md#without-pgvector)
- [Migrations](../language/05-data.md#migrations)
- [`jadpo migrate`](../language/14-tooling.md#jadpo-migrate)
- [`migration.extension_outdated`](migration.extension_outdated.md)
