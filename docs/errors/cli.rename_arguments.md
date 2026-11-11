# `cli.rename_arguments`

A `jadpo rename` command names a declaration or field the project doesn't have, one it doesn't rename, or a new name that is taken or isn't one.

## Why Jadpo refuses it

`jadpo rename` gives one declaration or field a new name everywhere code names it, and keeps what is stored under the old one. A name means one thing in a project, so a new name already in use would make two things of one, and the rename stops before it changes anything.

## What it looks like

A new project, whose `Visitor` type is renamed to `Greeting`, the name its other type already has:

```sh run project=e-rename-arguments expect=cli.rename_arguments
jadpo new todo > /dev/null
cd todo
jadpo rename Visitor Greeting
```

```text output
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If the name is taken**, choose one the `in use` list doesn't hold:

```sh
jadpo rename Visitor Guest
```

The list shows the names in use that share the new name's words, so a name that differs from them all can't clash.

**If the name isn't one for its kind**, write a type's or an entity's in UpperCamelCase and everything else's in lower_snake_case:

```sh
jadpo rename greet welcome
```

**If it names a field**, name the field after its entity or type, and give it a name of its own:

```sh
jadpo rename Todo.priority importance
```

## Related

- [`jadpo rename`](../language/14-tooling.md#jadpo-rename)
- [Stable identity](../language/18-records-and-manifest.md#stable-identity)
- [`semantic.duplicate_declaration`](semantic.duplicate_declaration.md)
