# `toolchain.scaffold_destination_exists`

You ran `jadpo new` with the name of a file that already exists.

## Why Jadpo refuses it

`jadpo new <name>` makes a folder called `<name>` and writes the project
into it. A file already has that name, and replacing it would lose it,
so the command writes nothing.

Jadpo won't pick another name or move the file for you: the name is the
project's, and the file is yours.

## What it looks like

A file `todo-app` is already in the folder:

```sh run project=e-scaffold-exists expect=toolchain.scaffold_destination_exists
touch todo-app
jadpo new todo-app
```

```text output
error: `todo-app` is a file
  reason: `jadpo new` makes a folder holding the project, and a file already has this name. It wrote nothing.
  next: Name a folder that doesn't exist yet, or move `todo-app` first (human)
  rule: toolchain.scaffold_destination_exists
  help: https://jadpo.dev/docs/errors/toolchain.scaffold_destination_exists
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides what the project is called, and whether the file keeps its
name.

## How to fix it

**If the project can have another name**, give one no file or folder
has:

```sh run project=e-scaffold-exists
jadpo new todo-service
```

```text output
Created todo-service
    jadpo  X.Y
    route  POST /hello
    files  14 written

next: cd todo-service && jadpo dev  serves it on http://localhost:3000
```

The project is in `todo-service`, and the file is untouched.

**If the project should have this name**, move the file out of the way,
then run the command again:

```sh
mv todo-app todo-app.txt
jadpo new todo-app
```

The project is in `todo-app`, and the file keeps its contents under its
new name.

## Related

- [`jadpo new`](../language/14-tooling.md#jadpo-new)
- [The `jadpo` command](../language/14-tooling.md#the-jadpo-command)
