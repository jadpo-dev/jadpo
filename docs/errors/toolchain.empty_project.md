# `toolchain.empty_project`

The project declares nothing.

## Why Jadpo refuses it

A project with no declarations gives Jadpo nothing to check, and a project
that passes with nothing in it would look like a project that works. A file
of only comments is the usual way to get here.

## What it looks like

```jadpo check project=e-empty file=notes.jadpo expect=toolchain.empty_project
// Nothing declared yet.
```

```text output
error: This project declares nothing
  reason: Its one `.jadpo` file holds no declarations, only comments or nothing at all.
  next: Add a declaration, such as a `type`, `function` or `action` (agent)
  rule: toolchain.empty_project
  help: https://jadpo.dev/docs/errors/toolchain.empty_project

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you were about to write the first declaration**, add it:

```jadpo check project=e-empty-fix file=notes.jadpo
/// A note someone wrote.
type Note = Text {
    max_length: 200
}
```

`jadpo check` now has something to check, and passes.

## Related

- [Source files](../language/01-project-layout-and-modules.md#source-files)
- [`jadpo check`](../language/14-tooling.md#jadpo-check)
