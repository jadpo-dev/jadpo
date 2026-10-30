# `toolchain.scaffold_destination_not_empty`

`jadpo new <name>` writes a whole project, so it writes only into a folder
that doesn't exist yet or is empty
([The `jadpo` command](../language/14-tooling.md#the-jadpo-command)). Given a folder that
holds anything, hidden files included, it refuses and changes nothing:

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

Which to do is the person's choice: a new name gives a second project, and
emptying the folder deletes what was in it.
