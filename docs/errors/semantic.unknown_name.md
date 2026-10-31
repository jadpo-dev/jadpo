# `semantic.unknown_name`

The code uses a name that nothing in the project or the standard library declares.

## Why Jadpo refuses it

Every name in the code has to lead to a declaration: in the file, on a
`use` line, or among the built-ins. A name that leads nowhere is most
often a misspelling or a declaration not written yet, and Jadpo reports
it when you check rather than leave it to fail when the code runs.

## What it looks like

main.jadpo:

```jadpo check project=e-unknown-name expect=semantic.unknown_name
type Pair = Object {
    first: Missing
}
```

```text output
error: `Missing` isn't declared
 --> main.jadpo:2:12
  |
2 |     first: Missing
  |            ^^^^^^^ not declared
  reason: Nothing in this project or the standard library is called `Missing`.
  next: Declare `Missing`, or use an existing name (agent)
  rule: semantic.unknown_name
  help: https://jadpo.dev/docs/errors/semantic.unknown_name

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the name is meant to be new**, declare it. `main.jadpo` becomes:

```jadpo check project=e-unknown-name-fix-1
type Missing = Text {
    max_length: 80
}

type Pair = Object {
    first: Missing
}
```

`Pair.first` now holds a `Missing`, text of at most 80 characters.

**If you meant a name that exists**, write that name. Here the field
holds a built-in type, and `main.jadpo` becomes:

```jadpo check project=e-unknown-name-fix-2
type Pair = Object {
    first: Text
}
```

`Pair.first` now holds any text. A name declared in another file of a
project also needs its `use` line.

## Related

- [Imports](../language/01-project-layout-and-modules.md#imports)
- [What is imported](../language/01-project-layout-and-modules.md#what-is-imported)
- [Named types](../language/02-values-and-types.md#named-types)
- [Built-in types](../language/02-values-and-types.md#built-in-types)
