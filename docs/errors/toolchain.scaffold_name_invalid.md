# `toolchain.scaffold_name_invalid`

You ran `jadpo new` with a path that doesn't end in a folder's name, such as `..` or `/`.

## Why Jadpo refuses it

`jadpo new <name>` makes the project in the folder its argument names,
and names the project after that folder. A path ending in `..`, or `/`
alone, names no folder of its own, so there is nothing to call the
project, and the command writes nothing.

## What it looks like

Asking for the project in the folder above:

```sh run project=e-scaffold-name expect=toolchain.scaffold_name_invalid
jadpo new ..
```

```text output
error: `..` doesn't name a folder for the project
  reason: `jadpo new` makes the project in the folder its argument names, and names the project after that folder, so the argument ends in a folder's name.
  next: Give the project's name, for example `jadpo new my-app` (agent)
  rule: toolchain.scaffold_name_invalid
  help: https://jadpo.dev/docs/errors/toolchain.scaffold_name_invalid
```

`(agent)` at the end of the `next:` line means your coding agent can
make the fix itself.

## How to fix it

**If the project should go in the folder you are in**, name it `.`. The
folder must be empty, and the project is named after it:

```sh run project=e-scaffold-name
jadpo new .
```

```text output
Created .
    jadpo  0.1
    route  POST /hello
    files  14 written

next: jadpo dev  serves it on http://localhost:3000
```

The project is written where you are.

**If it should go in a new folder**, name that folder:

```sh run project=e-scaffold-name
jadpo new todo-app
```

```text output
Created todo-app
    jadpo  0.1
    route  POST /hello
    files  14 written

next: cd todo-app && jadpo dev  serves it on http://localhost:3000
```

The project is in `todo-app`, and named after it.

## Related

- [`jadpo new`](../language/14-tooling.md#jadpo-new)
- [The `jadpo` command](../language/14-tooling.md#the-jadpo-command)
