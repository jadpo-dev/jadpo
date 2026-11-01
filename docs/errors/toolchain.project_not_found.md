# `toolchain.project_not_found`

You ran a command that works on a project in a folder with no `app.jadpo`, in it or in any folder above it.

## Why Jadpo refuses it

A project is the folder that holds `app.jadpo`, and commands such as
`jadpo check`, `jadpo test` and `jadpo config` work on a whole project.
Run from a folder inside it, they find it by looking up through the
folders above. When none holds `app.jadpo`, there is no project to work
on, and the command does nothing.

## What it looks like

`jadpo check`, run in a folder that holds no project:

```sh run project=e-project-not-found expect=toolchain.project_not_found
jadpo check
```

```text output
error: No Jadpo project here
  reason: Jadpo looks for `app.jadpo` in this folder and every folder above it, and found none.
  next: Run `jadpo check` inside a project, or give a file or folder: `jadpo check <path>` (agent)
  rule: toolchain.project_not_found
  help: https://jadpo.dev/docs/errors/toolchain.project_not_found
```

`(agent)` at the end of the `next:` line means your coding agent can
make the fix itself.

## How to fix it

**If the project is in another folder**, run the command there, or give
its path:

```sh
jadpo check todo-app
```

The command checks the project in `todo-app`.

**If there is no project yet**, make one:

```sh run project=e-project-not-found
jadpo new todo-app
```

```text output
Created todo-app
    jadpo  0.2
    route  POST /hello
    files  14 written

next: cd todo-app && jadpo dev  serves it on http://localhost:3000
```

`cd todo-app`, and every command finds the project from there.

## Related

- [The `jadpo` command](../language/14-tooling.md#the-jadpo-command)
- [`jadpo new`](../language/14-tooling.md#jadpo-new)
- [The root](../language/01-project-layout-and-modules.md#the-root)
