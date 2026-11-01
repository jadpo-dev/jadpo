# Deploy natively
<!-- topic: guide-deploy-natively -->

`jadpo build` turns your app into one program, with nothing else to
install beside it, that you run on any Linux or macOS server with a
PostgreSQL database. This guide builds the book catalogue from
[Build an API](01-build-an-api.md), applies its migrations to a real
database and runs it.

You need PostgreSQL that the server can reach, and `jadpo` on the
machine you build on. You start from the catalogue's code:

```jadpo check project=guide-native file=domains/books/entities/book.jadpo hidden
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

```jadpo check project=guide-native file=domains/books/types/new_book.jadpo hidden
use books/entities { Book }

/// What a caller sends to add a book.
type NewBook = Input {
    title: Book.title
    author: Book.author
}
```

```jadpo check project=guide-native file=domains/books/types/book_view.jadpo hidden
use books/entities { Book }

/// A book, as the API sends it.
type BookView = Output {
    pick: [Book.id, Book.title, Book.author, Book.added_at]
}
```

```jadpo check project=guide-native file=domains/books/actions/add_book.jadpo hidden
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

```jadpo check project=guide-native file=domains/books/queries/book_by_id.jadpo hidden
use books/entities { Book, BookNotFound }
use books/types { BookView }

/// One book.
query book_by_id(book_id: Book.id) fails BookNotFound -> BookView {
    from: Book
    where: Book.id == book_id
    missing: BookNotFound
}
```

```jadpo check project=guide-native file=domains/books/routes/books.jadpo hidden
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

## Name the database

`jadpo dev` and `jadpo test` start databases of their own. A deployed
app is told where its database is, through a connection called `main`,
the store every entity is kept in unless it says otherwise:

`app.jadpo`, changed. The `main` connection is new.

```jadpo check project=guide-native file=app.jadpo
application {
    jadpo: "0.2"
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

`config.jadpo`. The `database_url` setting is new.

```jadpo check project=guide-native file=config.jadpo
config {
    /// The PostgreSQL database the app keeps its data in.
    database_url: PostgresUrl
}
```

`PostgresUrl` is a database address, such as
`postgres://user:password@host:5432/books`. It holds a password, so it
is always a secret, and it has no default, so the program won't start
without it. On your machine, `jadpo dev` and `jadpo test` still start
their own database while `DATABASE_URL` isn't set.

## Build the program

```sh
jadpo build
```

```text
  binary: build/target/app/app
  build: b_bd8f8b62b3ed
  commit: none
  settings: DATABASE_URL

alert: Built from files git doesn't track: no commit explains its events

built: app, native in 101.0 s

next: jadpo migrate apply  it starts only once its database has every migration
```

The program is `build/target/app/app`. The first build fetches the Rust
compiler it needs, so it takes a few minutes. `build:` is an id for this
exact build, which every line the program logs carries. The alert says
the project isn't in git, so the build can't name its commit; commit
your project and it will. `settings:` lists the variables the program
reads.

## Put the settings in a file

The program reads its settings from its environment, never from
`.env.local`. Don't type them on the command line, where your shell's
history keeps them: put them in a file outside your project that only
you can read, with your editor.

`~/books-app.env`:

```text
DATABASE_URL=postgres://postgres@localhost:5433/books
```

```sh
chmod 600 ~/books-app.env
```

Use your own database's address. On a real server, set the same
variables wherever your host keeps secrets.

## Apply the migrations

Before the program first runs, and before each new build that brings
new migrations, apply them. This loads the file into the environment of
one command:

```sh
(set -a; . ~/books-app.env; jadpo migrate apply)
```

```text
Applied 1 migration to `main`
    20261020T1400_initial  applied
```

The program refuses to start against a database that is missing one of
its migrations.

## Run it

```sh
(set -a; . ~/books-app.env; build/target/app/app)
```

```text
{"address":"0.0.0.0:8080","build":"b_bd8f8b62b3ed","commit":null,"compiler":"0.1.0","deployable":"app","event":"runtime.started","modified":true,"target":"native","trigger":"native","unbuilt":[],"workers":10}
```

It writes one line of JSON for each thing worth logging, and listens on
port 8080; the variables `PORT` and `HOST` change that. A setting it
needs and doesn't have stops it at once, naming the setting but never
printing a value:

```text
{"build":"b_bd8f8b62b3ed","commit":null,"compiler":"0.1.0","deployable":"app","event":"runtime.startup_failed","modified":true,"problems":[{"message":"`DATABASE_URL` isn't set: `database_url` is required, so the app can't start without it.","problem":"setting_missing","setting":"database_url","variable":"DATABASE_URL"}],"target":"native"}
```

Every deployed app answers a health check, for your load balancer or
host to poll:

```sh
curl -i http://localhost:8080/health/live
```

```text
HTTP/1.1 200 OK
content-type: application/json
cache-control: no-store
x-request-id: req_d246e6a6b13b1c94
content-length: 17
date: Sat, 31 Oct 2026 11:03:38 GMT

{"status":"live"}
```

The routes answer as they do under `jadpo dev`. Ctrl-c, or the `SIGTERM`
a service manager sends, stops it cleanly: it finishes the requests it
has, then exits. On a real server, run it under a service manager such
as systemd, which restarts it if it stops, behind a proxy that serves
HTTPS. The program runs the app's schedules and delivers its events
itself; [Run background work](07-run-background-work.md) covers those.

## Read more

[A native deployable](../language/14-tooling.md#a-native-deployable),
[Declared deployables](../language/14-tooling.md#declared-deployables),
[Database and other connection URLs](../language/10-configuration-and-time.md#database-and-other-connection-urls),
[Values on your machine and in production](../language/10-configuration-and-time.md#values-on-your-machine-and-in-production)
and [Health](../language/07-http.md#health). To run on Cloudflare
instead, see [Deploy to Cloudflare Workers](11-deploy-to-workers.md).

## Not built yet

Counting rate limits in Redis isn't built yet, so a server program
counts each route's requests in its own memory. Run one copy of it, or
expect each copy to count on its own.
