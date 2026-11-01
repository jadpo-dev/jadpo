# `toolchain.scaffold_write_failed`

You ran `jadpo new`, and writing the project's files failed, usually because the folder it goes in can't be written.

## Why Jadpo refuses it

`jadpo new` writes a whole project or none of it. When the operating
system refuses one of its files, it removes what it had already written
and says which file and why, so you are never left with half a project
that doesn't check.

Jadpo won't change permissions or pick another folder for you: both are
decisions about your files.

## What it looks like

The folder `apps` is read-only, and the project is asked for inside it:

```sh run project=e-scaffold-write expect=toolchain.scaffold_write_failed
mkdir apps
chmod a-w apps
jadpo new apps/todo-app
```

```text output
error: `apps/todo-app` couldn't be written
  reason: Writing it failed: Permission denied (os error 13). `jadpo new` removed what it had written, so `apps/todo-app` is as it was.
  next: Check that `apps/todo-app` can be written, then run `jadpo new apps/todo-app` again (human)
  rule: toolchain.scaffold_write_failed
  help: https://jadpo.dev/docs/errors/toolchain.scaffold_write_failed
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides who may write where the project goes.

## How to fix it

**If the folder should be writable**, let yourself write to it, then run
the command again:

```sh
chmod u+w apps
jadpo new apps/todo-app
```

The whole project is written into `apps/todo-app`.

**If the folder should stay read-only**, make the project somewhere you
can write:

```sh run project=e-scaffold-write
jadpo new todo-app
```

```text output
Created todo-app
    jadpo  X.Y
    route  POST /hello
    files  14 written

next: cd todo-app && jadpo dev  serves it on http://localhost:3000
```

The project is in `todo-app`, and `apps` is untouched.

## Related

- [`jadpo new`](../language/14-tooling.md#jadpo-new)
- [The `jadpo` command](../language/14-tooling.md#the-jadpo-command)
