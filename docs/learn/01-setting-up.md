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
git clone https://github.com/richardwillars/jadpo.git
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

## Check that it works

The repository has teaching projects in `examples/`. Go back to the
`jadpo` folder, and ask `jadpo` to check the first one:

```sh
jadpo check examples/01-first-project
```

```text
checked: 6 files in 0.5 s
```

`jadpo check` reads the whole project and reports every problem it
finds. It doesn't compile the app into a program or run it, so it's
quick, and you run it after every change. It does write a few reports,
such as a description of your API, into a `build/` folder inside the
project, which you don't commit. Then run the project's tests:

```sh
jadpo test examples/01-first-project
```

```text
tested: 3 passed, 3 action tests
```

The tests ran three of the project's actions, the pieces of an app that
do its work. You write your first action on the next page.

## When something is wrong

`jadpo` never warns: everything it reports is an error, and the app
doesn't run until you fix it. Every error has the same parts:

- where the problem is, with the line of code;
- `reason:`, why it's a problem;
- `next:`, what to do about it, and who should do it. `(agent)` means a
  coding agent can make the fix. `(human)` means a person has to decide,
  so an agent stops and asks;
- `rule:`, the error's name, and `help:`, where its page is.

Every error has a page on this site: `https://jadpo.dev/docs/errors/`
followed by the rule, such as
`https://jadpo.dev/docs/errors/record.out_of_date`. The `help:` line
gives the page's place in Jadpo's own source instead, which you can't
open from your project, so use the site. You'll meet real errors on the
next page, and fix them.

## Using it with a coding agent

An agent reads [the primer](../primer.md), one page about the language,
before it writes Jadpo, and the help page of each error it meets. To
get an agent started, ask it to read the primer and then follow these
pages with you.

Next: [A project and your first route](02-first-route.md).

## Not built yet

There is no installer yet, so you build from source as above. Prebuilt
downloads, Homebrew and `npx jadpo` come with the first release, and so
does `jadpo new`, which will start a project for you. Until then you
make the few files of a new project by hand, which the next page does.
