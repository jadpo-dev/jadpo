# What Jadpo is, and setting up
<!-- topic: learn-setting-up -->

Jadpo is a programming language for web backends: the server side of an
app, which keeps its data and answers its HTTP requests. Coding agents
write most Jadpo, and people review it. This page says what that means
in practice, then gets the `jadpo` command running on your machine.

## What Jadpo does for you

In a TypeScript backend, the rules of your app are spread across many
places. A validation schema says what a request may hold. An ORM model
says what a table holds. Middleware decides who is signed in, and a
check in each handler decides what they may see. Nothing makes sure these
agree, so a reviewer has to read every one.

In Jadpo you write each rule once, in the place the language gives it.
Then one compiler checks the whole app against all of them. It checks
the types and the stored data. It checks the HTTP routes, who can sign
in, and who may see and change each record. It also checks the
settings, calls to other services, background work and tests. So if a
route would send a secret field to a client, or skip sign-in, the app
doesn't compile.

That is also why Jadpo suits coding agents. An agent can't forget a
permission check the compiler adds for it, and a reviewer reads one
declaration instead of tracing every handler.

## How it differs from TypeScript

Some habits don't carry over. The compiler catches most of them and
tells you the Jadpo way:

- No `null`. A value that may be missing has a type ending in `?`, and
  holds `none` when it is.
- No exceptions. Something that can go wrong is declared, like a return
  type, and the compiler checks every caller deals with it.
- No `async`, classes, methods, your own generics, `while`, `break` or
  floating point. `for` is the only loop, and decimal numbers are exact.
- No hand-written SQL. You declare what to read and write, and the
  compiler writes the SQL.
- No truthiness, no semicolons, and no `!`, `&&` or `||`: you write
  `not`, `and` and `or`.

Jadpo has no front end. A web page or a phone app calls it over HTTP,
the same as any other API.

## Install it

You build `jadpo` from its source code, so you need Git, and Rust
installed through [rustup](https://rustup.rs). The repository pins the
version of Rust it needs, and rustup fetches it on the first build.

```sh
git clone https://github.com/jadpo-dev/jadpo.git
cd jadpo
cargo build --release --locked -p jadpo-cli
```

The first build can take several minutes. It puts the program at
`target/release/jadpo`. So that you can type `jadpo` in any folder and
any new terminal, add that folder to your `PATH` in your shell's
startup file. Run this from the `jadpo` folder; it writes the folder's
full path into the file. On a Mac the file is `~/.zshrc`; on most Linux
systems use `~/.bashrc` instead:

```sh
echo "export PATH=\"$PWD/target/release:\$PATH\"" >> ~/.zshrc
```

Open a new terminal, and check that `jadpo` runs:

```sh
jadpo --version
```

```text
jadpo 0.1.0
```

From page 4 on, the app stores its data in PostgreSQL, so install
PostgreSQL now if you don't have it, from postgresql.org or your package
manager. You don't create a database or set a password: when you run or
test the app, Jadpo starts a throwaway database of its own. It finds
PostgreSQL's programs (`initdb`, `pg_ctl` and `postgres`) on your `PATH`,
or where the usual installers put them.

## Start a project

`jadpo new` starts a project, which is the todo app the rest of these
pages build. Make it outside the `jadpo` folder you built from, for
instance in your home folder:

```sh
cd ~
jadpo new todo-app
```

```text
Created todo-app
    jadpo  0.1
    route  POST /hello
    files  12 written

next: cd todo-app && jadpo dev  serves it on http://localhost:3000
```

It made a folder, `todo-app`, holding a small starter app: one route, an
HTTP endpoint that answers `POST /hello`, with a test. Go into the
folder, and ask `jadpo` to check the project:

```sh
cd todo-app
jadpo check
```

```text
checked: 8 files in 0.7 s
```

`jadpo check` reads the whole project and reports every problem it
finds. It doesn't compile the app into a program or run it, so it's
quick, and you run it after every change. It does write a few reports,
such as a description of your API, into a `build/` folder inside the
project, which you don't commit. Then run the project's tests:

```sh
jadpo test
```

```text
tested: 1 passed, 1 route test
```

The test sent the starter route a request and checked its answer. On
the next page you look at what `jadpo new` wrote, and replace the
starter with the todo app's first route.

## When something is wrong

`jadpo` never warns: everything it reports is an error, and the app
doesn't run until you fix it. Every error has the same parts:

- where the problem is, with the line of code;
- `reason:`, why it's a problem;
- `next:`, what to do about it. `(human)` at the end of an error's
  `next:` line tells your coding agent to stop and ask you which fix you
  want, because the choice is yours to make (who can see data, what a
  setting holds). `(agent)` means your agent can make the fix itself;
- `rule:`, the error's name, and `help:`, the address of its page.

Every error has a page on this site: `https://jadpo.dev/docs/errors/`
followed by the rule, such as
`https://jadpo.dev/docs/errors/record.out_of_date`, which the `help:`
line prints for you to open. You'll meet real errors on the next page,
and fix them.

## Using it with a coding agent

An agent reads [the primer](../primer.md), one page about the language,
before it writes Jadpo, and the help page of each error it meets. To
get an agent started, ask it to read the primer and then follow these
pages with you.

Next: [A project and your first route](02-first-route.md).

## Not built yet

There is no installer yet, so you build from source as above. Prebuilt
downloads, Homebrew (`brew install jadpo-dev/tap/jadpo`) and `npx @jadpo/jadpo` come with the first release. Each project names
the Jadpo release line it is written for, and whichever `jadpo` you run
fetches and runs that release for it, so projects on different releases
work side by side.
