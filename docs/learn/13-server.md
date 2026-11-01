# Deploying to a server
<!-- topic: learn-server -->

`jadpo dev` runs the app with a throwaway database and catches every
email. To deploy it, you build it into one program and run it with a
real database and real settings. On this page you do that on your own
machine, the same way you would on a server. Page 14 deploys it to
Cloudflare Workers instead.

## Name the real database

Until now, Jadpo started every database itself. A deployed app has to
be told where its database is. Add a connection for it to `app.jadpo`,
the same way page 11 added the mail server:

`app.jadpo`, changed. The `main` connection is new.

```jadpo check project=learn-todo-13 file=app.jadpo
use todos/entities { User }

application {
    display_name: "Todos"
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
        mail: {
            driver: Driver.smtp
            host: config.smtp_host
            credential: {
                username: config.smtp_username
                password: config.smtp_password
            }
        }
    }
    email: {
        connection: mail
        from: config.mail_from
    }
}

principal {
    user: {
        subject: User.email
    }
}
```

The connection must be called `main`, the name Jadpo gives the
database every entity is stored in unless it says otherwise. That's a
different thing from the API called `main` on page 2: one is a
connection, the other an API, and Jadpo always knows which from where
the name is written.

Add the setting it reads to `config.jadpo`:

`config.jadpo`, changed. The `database_url` setting is new.

```jadpo check project=learn-todo-13 file=config.jadpo
config {
    /// The PostgreSQL database the app keeps its data in.
    database_url: PostgresUrl
    /// Who the app's emails come from.
    mail_from: Email {
        default: "todos@example.com"
    }
    /// How far ahead a todo may be due.
    max_due_ahead: Duration {
        default: 365d
        test: 30d
    }
    /// The address of the site people sign in on.
    site: Url {
        default: "http://localhost:3000"
    }
    /// The mail server the app hands its emails to.
    smtp_host: Hostname {
        default: "localhost"
    }
    /// The password for the mail server.
    smtp_password: Text {
        secret: true
    }
    /// The user name for the mail server.
    smtp_username: Text {
        default: "todos"
    }
}
```

`PostgresUrl` is a standard type for a database address, such as
`postgres://user:password@host:5432/todos`. It holds a password, so it
is always a secret, and it has no default, so a deployed app won't
start without it. `jadpo dev` and `jadpo test` still start their own
database when `DATABASE_URL` isn't set, so nothing changes on your
machine. `jadpo check` and `jadpo test` pass as before.

## Build the program

```sh
jadpo build
```

```text
  binary: build/target/app/app
  build: b_eae4cf8f9e2e
  commit: none
  settings: DATABASE_URL, MAIL_FROM, MAX_DUE_AHEAD, SITE, SMTP_HOST, SMTP_PASSWORD, SMTP_USERNAME

alert: Built from files git doesn't track: no commit explains its events

built: app, native in 67.1 s

next: jadpo migrate apply  it starts only once its database has every migration
```

`jadpo build` checks the project as `jadpo check` does, turns it into
Rust and compiles it into one program, `build/target/app/app`, with
nothing else to install beside it. The first build fetches the Rust
compiler and libraries it needs, so it takes a few minutes. `build:` is
an id for this exact build. The alert says the project isn't in git, so
the build can't name the commit it came from; commit your project and it
will. `settings:` lists variables the program reads.

## Get a database

You need a PostgreSQL server the program can reach. On a server, that's
usually one your host runs for you, and it gives you the address. To
try it on your own machine, start one with the PostgreSQL programs from
page 1. This keeps its files in `~/todo-db` and listens on port 5433:

```sh
initdb -D ~/todo-db -U postgres --auth=trust
pg_ctl -D ~/todo-db -o "-p 5433" -l ~/todo-db.log start
createdb -h localhost -p 5433 -U postgres todos
```

`--auth=trust` lets anyone on your machine connect without a password,
which is fine for a trial and never for a real server. Stop it later
with `pg_ctl -D ~/todo-db stop`.

## Put the settings in a file

The program reads its settings from its environment, never from
`.env.local`. Don't type them on the command line, where your shell's
history keeps them. Put them in a file outside your project, readable
only by you, with your editor. Call it `~/todo-app.env`:

`~/todo-app.env`:

```text
DATABASE_URL=postgres://postgres@localhost:5433/todos
SITE=https://todos.example.com
SMTP_HOST=smtp.example.com
SMTP_USERNAME=todos
SMTP_PASSWORD=change-me
MAIL_FROM=todos@example.com
```

```sh
chmod 600 ~/todo-app.env
```

Replace each value with your own. Put a value in single quotes if it
has spaces or symbols in it. These are the settings without a default,
and the ones whose default is only right for your machine. On a real
server you'd set the same variables wherever your host keeps secrets.

## Apply the migrations

Before the program first runs, and before each new build with new
migrations, apply the migrations to the database. This loads the file
into your shell's environment for one command:

```sh
(set -a; . ~/todo-app.env; jadpo migrate apply)
```

```text
Applied 2 migrations to `main`
    20261029T1200_initial      applied
    20261029T1300_add_sign_in  applied
```

The program refuses to start against a database that's missing one of
its migrations.

## Run it

```sh
(set -a; . ~/todo-app.env; build/target/app/app)
```

```text
{"address":"0.0.0.0:8080","build":"b_eae4cf8f9e2e","commit":null,"compiler":"X.Y.Z","deployable":"app","event":"runtime.started","modified":true,"target":"native","trigger":"native","unbuilt":[],"workers":10}
```

It writes one line of JSON for each thing worth logging. It listens on
port 8080; the variables `PORT` and `HOST` change that, read by the
program itself rather than declared as settings.

If a setting it needs is missing, it stops at once, naming the setting
but never printing a value. Here `DATABASE_URL` was left out:

```text
{"build":"b_eae4cf8f9e2e","commit":null,"compiler":"X.Y.Z","deployable":"app","event":"runtime.startup_failed","modified":true,"problems":[{"message":"`DATABASE_URL` isn't set: `database_url` is required, so the app can't start without it.","problem":"setting_missing","setting":"database_url","variable":"DATABASE_URL"}],"target":"native"}
```

Every deployed app answers a health check, which a load balancer or
your host can poll:

```sh
curl -i http://localhost:8080/health/live
```

```text
HTTP/1.1 200 OK
content-type: application/json
cache-control: no-store
x-request-id: req_69a12724455206da
content-length: 17
date: Thu, 29 Oct 2026 21:57:41 GMT

{"status":"live"}
```

The routes behave as in `jadpo dev`:

```sh
curl http://localhost:8080/todos
```

```text
{"error":{"code":"authentication_required","message":"Sign in first.","request_id":"req_803740b5e57b78bd"}}
```

Ctrl-c, or the `SIGTERM` a service manager sends, stops it cleanly: it
finishes the requests it has, then exits. On a real server, put it
behind a proxy that serves HTTPS, since the session cookie is only sent
over HTTPS, and run it under a service manager such as systemd, which
restarts it if it stops.

## What you have

The whole app, built into one program and running against a database
you control. Next, page 14 deploys the same app to Cloudflare Workers.

Read more in the reference: [A native deployable](../language/14-tooling.md#a-native-deployable)
and [Database and other connection URLs](../language/10-configuration-and-time.md#database-and-other-connection-urls).

## Not built yet

- There are no prebuilt `jadpo` downloads yet, so you build on a
  machine where you built `jadpo`, as on page 1.
- Counting rate limits in Redis isn't built yet, so a server program
  counts sign-in emails per address in its own memory. Run one copy of
  it.
