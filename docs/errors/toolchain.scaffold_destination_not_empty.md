# `toolchain.scaffold_destination_not_empty`

You ran `jadpo new` on a folder that already has files in it, hidden files included.

## Why Jadpo refuses it

`jadpo new <name>` writes a whole project: `app.jadpo`, a first route,
its records and the rest. Writing it into a folder that holds anything
could overwrite your files or mix them with the new project's, so it
writes only into a folder that doesn't exist yet or is empty. A hidden
file, such as `.git` or `.env.local`, counts. It refuses and changes
nothing.

Jadpo won't pick another folder or empty this one for you: a new name
gives you a second project, and emptying the folder deletes what was in
it.

## What it looks like

A folder `todo-app` already holds a hidden file:

```sh
jadpo new todo-app
```

```text
error: `todo-app` already has files in it
  reason: `jadpo new` writes a whole project, so it writes only into a folder that doesn't exist yet or is empty. It wrote nothing.
  next: Name a folder that doesn't exist yet, or empty `todo-app` first (human)
  rule: toolchain.scaffold_destination_not_empty
  help: https://jadpo.dev/docs/errors/toolchain.scaffold_destination_not_empty
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether the files already in `todo-app` are kept.

## How to fix it

**If the files in the folder should stay**, name a folder that doesn't
exist yet:

```sh
jadpo new todo-app-2
```

```text
Created todo-app-2
    jadpo  0.1
    route  POST /hello
    files  14 written

next: cd todo-app-2 && jadpo dev  serves it on http://localhost:3000
```

The new project is in `todo-app-2`, and `todo-app` is untouched.

**If they can go**, empty `todo-app` yourself, hidden files included,
then run the command again:

```sh
jadpo new todo-app
```

The project is written into `todo-app`.

## Related

- [`jadpo new`](../language/14-tooling.md#jadpo-new)
- [The `jadpo` command](../language/14-tooling.md#the-jadpo-command)
