# Deploy to Cloudflare Workers
<!-- topic: guide-deploy-to-workers -->

`jadpo build` can build your app as WebAssembly for Cloudflare Workers,
with the JavaScript that loads it and the description Wrangler,
Cloudflare's command-line tool, deploys. This guide builds the book
catalogue from [Build an API](01-build-an-api.md) as a Worker that
reaches its PostgreSQL database through Hyperdrive.

You need:

- **A Cloudflare account.** Check Cloudflare's pricing for which plan
  includes Workers and Hyperdrive.
- **Wrangler.** It needs Node.js; run it with `npx wrangler`, which
  fetches it the first time. `npx wrangler login` opens your browser to
  sign in to Cloudflare.
- **A PostgreSQL database reachable from the internet**, such as one
  from a hosted PostgreSQL provider. One on your own machine won't do,
  since Cloudflare can't reach it.

You start from the catalogue as [Deploy natively](10-deploy-natively.md)
leaves it, with its `main` database connection and the `database_url`
setting:

```jadpo check project=guide-workers file=domains/books/entities/book.jadpo hidden
/// A book the library lends.
entity Book {
    id: Uuid
    title: Text {
        min_length: 1
        max_length: 200
    }
    author: Text {
        min_length: 1
        max_length: 100
    }
    added_at: Instant {
        generated: Generated.on_create
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
```

```jadpo check project=guide-workers file=domains/books/types/new_book.jadpo hidden
use books/entities { Book }

/// What a caller sends to add a book.
type NewBook = Input {
    title: Book.title
    author: Book.author
}
```

```jadpo check project=guide-workers file=domains/books/types/book_view.jadpo hidden
use books/entities { Book }

/// A book, as the API sends it.
type BookView = Output {
    pick: [Book.id, Book.title, Book.author, Book.added_at]
}
```

```jadpo check project=guide-workers file=domains/books/actions/add_book.jadpo hidden
use books/entities { Book }
use books/types { BookView, NewBook }

/// Adds a book to the catalogue.
action add_book(input: NewBook) -> BookView {
    var book: Book = Book.create {
        set: {
            title: input.title
            author: input.author
        }
    }
    return BookView {
        id: book.id
        title: book.title
        author: book.author
        added_at: book.added_at
    }
}
```

```jadpo check project=guide-workers file=domains/books/queries/book_by_id.jadpo hidden
use books/entities { Book, BookNotFound }
use books/types { BookView }

/// One book.
query book_by_id(book_id: Book.id) fails BookNotFound -> BookView {
    from: Book
    where: Book.id == book_id
    missing: BookNotFound
}
```

```jadpo check project=guide-workers file=domains/books/routes/books.jadpo hidden
use books/entities { Book }
use books/types { BookView, NewBook }
use books/actions { add_book }
use books/queries { book_by_id }

/// The library's catalogue.
routes Books {
    prefix: /books

    /// Adds a book.
    POST / {
        authentication: RouteAuthentication.none
        input: NewBook
        output: BookView
        status: SuccessStatus.created
        run: add_book(input)
    }

    /// One book.
    GET /{book_id: Book.id} {
        authentication: RouteAuthentication.none
        output: BookView
        run: book_by_id(book_id)
    }
}
```

```jadpo check project=guide-workers file=config.jadpo hidden
config {
    /// The PostgreSQL database the app keeps its data in.
    database_url: PostgresUrl
}
```

## Say what to build

`app.jadpo`, changed. The `limits` connection, `rate_limits:` and
`deployables:` are new.

```jadpo check project=guide-workers file=app.jadpo
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
        limits: {
            driver: Driver.cloudflare_rate_limits
            namespace: 1001
        }
    }
    rate_limits: {
        store: limits
    }
    deployables: {
        web: {
            serves: [main]
            target: Target.wasm
        }
    }
}
```

- `deployables:` lists the programs the app is built into. Without it
  there is one, `app`, built as a server program. `web` is built for
  Workers, `Target.wasm`, and `serves: [main]` means it answers the
  API's routes.
- Every route is rate-limited. A server program counts requests in its
  own memory, but a Worker runs in many places at once, so the counts
  are kept by Cloudflare's rate-limiting binding, through the `limits`
  connection. `namespace: 1001` is a number you choose; Cloudflare needs
  an id for each limit, and Jadpo numbers them from this one.
- An app that sends email or has subscribers or schedules also says the
  Worker runs them: `runs: [Trigger.subscribers, Trigger.schedules]`.

## Build it

```sh
jadpo build
```

```text
  worker: build/target/web/wrangler.json
  module: build/target/web/web.wasm
  size: 7.2 MB, 2.7 MB gzipped
  database: DATABASE_URL through Hyperdrive, no id
  build: b_0ccbb5029fcf
  serves: main
  commit: none
  settings: DATABASE_URL

alert: `DATABASE_URL` has no Hyperdrive id: create one with caching off and build with it in `JADPO_HYPERDRIVE_DATABASE_URL`
alert: Built from files git doesn't track: no commit explains its events

built: web, wasm in 61.7 s

next: jadpo migrate apply  it starts only once its database has every migration
```

`build/target/web/` now holds the Worker: the WebAssembly module, the
JavaScript that loads it, and `wrangler.json`. It names the Worker after
your project's folder and lists the two limits Jadpo counts, numbered
from your `namespace:`. A secret setting with no default would be listed
there too, as one the Worker needs.

## Connect the database through Hyperdrive

A Worker keeps no connection to your database open between requests,
and opening one for each request is slow. Hyperdrive is Cloudflare's
service that keeps connections open and hands one to each request.
Create one for your database with caching off, since a cache could
answer a read with data that has since changed. Load the database's
address from your settings file, as in
[Deploy natively](10-deploy-natively.md), so it isn't typed into your
shell:

```sh
(set -a; . ~/books-app.env; npx wrangler hyperdrive create books \
  --connection-string="$DATABASE_URL" --caching-disabled)
```

Wrangler prints the new Hyperdrive's id. Build again with it:

```sh
JADPO_HYPERDRIVE_DATABASE_URL=<the id Wrangler printed> jadpo build
```

## Deploy

Apply the migrations to the database first:

```sh
(set -a; . ~/books-app.env; jadpo migrate apply)
```

Then deploy from the Worker's folder:

```sh
cd build/target/web
npx wrangler deploy
```

On Workers, settings are the Worker's variables and secrets, which
Cloudflare keeps. Store a secret with `npx wrangler secret put <NAME>`,
which asks for the value at a prompt, and give other settings as
`--var NAME:value` to `npx wrangler deploy`. Wrangler prints the
Worker's address, which answers the same routes and health check as
the server program.

## Read more

[A Workers deployable](../language/14-tooling.md#a-workers-deployable),
[Declared deployables](../language/14-tooling.md#declared-deployables)
and [Rate limits on Workers](../language/16-built-in-services.md#rate-limits-on-workers).

## Not built yet

The limits Jadpo writes for Cloudflare's rate-limiting binding haven't
yet been checked against Cloudflare's network, only against its local
runtime.
