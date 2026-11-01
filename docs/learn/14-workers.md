# Deploying to Cloudflare Workers
<!-- topic: learn-workers -->

Cloudflare Workers runs your code on Cloudflare's network, close to
whoever calls it, with no server of your own to look after. On this
page you build the todo app as a Worker and deploy it. The app is the
same as on page 13; only `app.jadpo` changes.

## What you need

- **A Cloudflare account.** Check Cloudflare's pricing page for which
  plan includes Workers and Hyperdrive (below).
- **Wrangler**, Cloudflare's command-line tool for Workers. It needs
  Node.js, and you run it with `npx wrangler`, which fetches it the first
  time. `npx wrangler login` opens your browser, where you sign in to
  Cloudflare and allow Wrangler to manage Workers in your account.
- **A PostgreSQL database reachable from the internet**, such as one a
  hosted PostgreSQL provider gives you. The database on your own machine
  from page 13 won't do, because Cloudflare can't reach it.

## Say what to build

`app.jadpo`, changed. The `limits` connection, `rate_limits:` and `deployables:` are new.

```jadpo check project=learn-todo-14 file=app.jadpo
use todos/entities { User }

application {
    display_name: "Todos"
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
        limits: {
            driver: Driver.cloudflare_rate_limits
            namespace: 1001
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
    rate_limits: {
        store: limits
    }
    deployables: {
        web: {
            serves: [main]
            runs: [Trigger.subscribers]
            target: Target.wasm
        }
    }
}

principal {
    user: {
        subject: User.email
    }
}
```

- `deployables:` lists the programs the app is built into. Until now
  there was one, which Jadpo calls `app`, built as a server program.
  `web` is built for Workers, `Target.wasm`, and `serves: [main]` means
  it answers the API's routes.
- `runs: [Trigger.subscribers]` says it also does the work the app
  leaves for after a request. The sign-in email is one: it's sent after
  the request that asked for it has finished. A server program does this
  itself; a Worker has to be told to.
- `rate_limits:` says where to count requests, such as how many sign-in
  emails one address has asked for. A server program counts in its own
  memory. A Worker runs in many places at once, so the count is kept by
  Cloudflare, through the `limits` connection.
- `namespace: 1001` is a number you choose. Cloudflare needs an id for
  each limit, and Jadpo numbers them from this one.

## Build it

```sh
jadpo build
```

```text
  worker: build/target/web/wrangler.json
  module: build/target/web/web.wasm
  size: 7.2 MB, 2.6 MB gzipped
  database: DATABASE_URL through Hyperdrive, no id
  build: b_5e7729bfe6b8
  serves: main
  runs: subscribers
  commit: none
  settings: DATABASE_URL, MAIL_FROM, MAX_DUE_AHEAD, SITE, SMTP_HOST, SMTP_PASSWORD, SMTP_USERNAME

alert: `DATABASE_URL` has no Hyperdrive id: create one with caching off and build with it in `JADPO_HYPERDRIVE_DATABASE_URL`
alert: Built from files git doesn't track: no commit explains its events

built: web, wasm in 32.7 s

next: jadpo migrate apply  it starts only once its database has every migration
```

`build/target/web/` now holds the Worker: the WebAssembly module, the
JavaScript that loads it, and `wrangler.json`, Wrangler's description of
the Worker. It names the Worker after your project's folder, names
`SMTP_PASSWORD` as a secret the Worker needs, since it has no default,
and lists the two limits Jadpo counts, numbered from your `namespace:`:

```text
  "name": "todo-app-web",
  ...
  "secrets": { "required": ["SMTP_PASSWORD"] },
  "hyperdrive": [{ "binding": "DATABASE_URL" }],
  ...
  "ratelimits": [
    {
      "name": "LIMITS_120_PER_60S",
      "namespace_id": "1001",
      "simple": { "limit": 120, "period": 60 }
    },
    {
      "name": "LIMITS_600_PER_60S",
      "namespace_id": "1002",
      "simple": { "limit": 600, "period": 60 }
    }
  ]
```

## Connect the database through Hyperdrive

A Worker doesn't keep a connection to your database open between
requests, and opening one for every request is slow. Hyperdrive is
Cloudflare's service that keeps connections to your database open and
hands one to each request. The build's alert says it has no Hyperdrive
id yet.

Create a Hyperdrive for your database, with caching off: a cache could
answer a read with data that has since changed. Load the database's
address from your settings file, so it isn't typed into your shell:

```sh
(set -a; . ~/todo-app.env; npx wrangler hyperdrive create todos \
  --connection-string="$DATABASE_URL" --caching-disabled)
```

Wrangler prints the new Hyperdrive's id. Build again with it:

```sh
JADPO_HYPERDRIVE_DATABASE_URL=<the id Wrangler printed> jadpo build
```

## Deploy

Apply the migrations to the database, as on page 13:

```sh
(set -a; . ~/todo-app.env; jadpo migrate apply)
```

On Workers, settings are the Worker's variables and secrets, which
Cloudflare keeps. Store the secret with Wrangler, which asks for the
value at a prompt rather than taking it on the command line, then
deploy, giving the other settings as variables:

```sh
cd build/target/web
npx wrangler secret put SMTP_PASSWORD
npx wrangler deploy \
  --var SITE:https://todos.example.com \
  --var SMTP_HOST:smtp.example.com \
  --var MAIL_FROM:todos@example.com
```

Wrangler prints the Worker's address. It answers the same health check
and routes as the server program, at that address. Cloudflare lets a
Worker reach a mail server on port 587, the port the SMTP connection
uses unless you give it another.

## What you have

The todo app from page 2, now with types, stored data, failures,
queries, changes, tests, settings, sign-in by email and private todos,
built as a server program or a Worker.

From here, the [primer](../primer.md) is a one-page summary of the
language to keep beside you, and the reference has every rule. The
teaching projects in the `jadpo` repository's `examples/` folder show
the rest: background work, other services, money, time zones and more.

Read more in the reference: [A Workers deployable](../language/14-tooling.md#a-workers-deployable),
[Declared deployables](../language/14-tooling.md#declared-deployables)
and [Rate limits on Workers](../language/16-built-in-services.md#rate-limits-on-workers).

## Not built yet

- The limits Jadpo writes for Cloudflare's rate-limiting binding haven't
  yet been checked against Cloudflare's network, only against its local
  runtime.
