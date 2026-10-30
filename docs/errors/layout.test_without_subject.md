# `layout.test_without_subject`

```jadpo check project=e-test-subject file=app.jadpo hidden
application {}
```

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
