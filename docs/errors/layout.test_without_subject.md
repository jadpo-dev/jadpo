# `layout.test_without_subject`

A test file has no file of the same name beside it.

## Why Jadpo refuses it

A test file sits beside the file it tests, with the same name and
`.test.jadpo` added, so a change to one leads to the other. A test named for
a file that doesn't exist tests nothing you can find from its path. Scenarios
that cross domains belong in the root `tests/` folder.

## What it looks like

`archive_todo.test.jadpo` has no `archive_todo.jadpo` beside it.

```jadpo check project=e-test-subject file=app.jadpo hidden
application {}
```

`domains/planning/actions/archive_todo.test.jadpo`:

```jadpo check project=e-test-subject file=domains/planning/actions/archive_todo.test.jadpo expect=layout.test_without_subject
test "archiving a todo hides it" {}
```

```text output
error: `archive_todo.test.jadpo` has no `archive_todo.jadpo` beside it
 --> domains/planning/actions/archive_todo.test.jadpo
  reason: A test file sits beside the file it tests, named after it; scenarios across domains go in the root `tests/` folder. `actions/` has no `archive_todo.jadpo`.
  next: Rename it after the file it tests, or move it to `tests/` (agent)
  rule: layout.test_without_subject
  help: https://jadpo.dev/docs/errors/layout.test_without_subject

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If it tests an action**, put it beside the action's file. Here the action
goes in `domains/planning/actions/archive_todo.jadpo`:

```jadpo check project=e-test-subject-fix file=domains/planning/actions/archive_todo.jadpo
action archive_todo() -> Unit {}
```

```jadpo check project=e-test-subject-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e-test-subject-fix file=domains/planning/actions/archive_todo.test.jadpo hidden
test "archiving a todo hides it" {
    archive_todo()
}
```

The test now sits beside what it tests, and can call it.

**If it is a scenario across domains**, move it to the root `tests/` folder:
`tests/archive_todo.test.jadpo`.

```jadpo check project=e-test-subject-move file=tests/archive_todo.test.jadpo
test "archiving a todo hides it" {}
```

```jadpo check project=e-test-subject-move file=app.jadpo hidden
application {}
```

The file is now a scenario of its own, which needs no file beside it.

## Related

- [Tests and other files](../language/01-project-layout-and-modules.md#tests-and-other-files)
- [The layout in detail](../language/01-project-layout-and-modules.md#the-layout-in-detail)
