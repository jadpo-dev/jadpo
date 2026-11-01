# `toolchain.scaffold_read_failed`

You ran `jadpo new` on a path Jadpo can't look into, such as a folder you can't read or a path through a file.

## Why Jadpo refuses it

`jadpo new` writes only into a folder that doesn't exist yet or is
empty, so before writing it looks at what is there. When it can't look,
it can't tell whether writing would overwrite something, so it writes
nothing and says why looking failed.

Jadpo won't change permissions or pick another path for you: both are
decisions about your files.

## What it looks like

`notes` is a file, and the path asks for a project inside it:

```sh run project=e-scaffold-read expect=toolchain.scaffold_read_failed
touch notes
jadpo new notes/todo-app
```

```text output
error: `notes/todo-app` couldn't be looked at
  reason: `jadpo new` checks that the folder is empty before it writes, and reading it failed: Not a directory (os error 20). It wrote nothing.
  next: Check that `notes/todo-app` can be read, then run `jadpo new` again (human)
  rule: toolchain.scaffold_read_failed
  help: https://jadpo.dev/docs/errors/toolchain.scaffold_read_failed
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides where the project goes.

A folder you aren't allowed to read is refused the same way, with
`Permission denied` as the reason.

## How to fix it

**If the path was a mistake**, name a folder that doesn't exist yet, or
an empty one:

```sh run project=e-scaffold-read
jadpo new todo-app
```

```text output
Created todo-app
    jadpo  0.2
    route  POST /hello
    files  14 written

next: cd todo-app && jadpo dev  serves it on http://localhost:3000
```

The project is in `todo-app`, and `notes` is untouched.

**If the folder is one you aren't allowed to read**, let yourself read
it, then run the command again:

```sh
chmod u+rx todo-app
jadpo new todo-app
```

`jadpo new` sees that the folder is empty and writes the project into
it.

## Related

- [`jadpo new`](../language/14-tooling.md#jadpo-new)
- [The `jadpo` command](../language/14-tooling.md#the-jadpo-command)
