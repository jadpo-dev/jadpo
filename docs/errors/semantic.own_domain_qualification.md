# `semantic.own_domain_qualification`

The dot names the owner of what you call, so code calls its own domain's
actions and queries by their bare names:

```jadpo check project=e-own-domain file=app.jadpo hidden
application {}
```

```jadpo check project=e-own-domain file=domains/planning/actions/snooze_todo.jadpo
action snooze_todo() -> Unit {}
```

```jadpo check project=e-own-domain file=domains/planning/actions/tidy_up.jadpo expect=semantic.own_domain_qualification
action tidy_up() -> Unit {
    planning.snooze_todo()
}
```

```text output
error: `snooze_todo` is in this file's own domain
 --> domains/planning/actions/tidy_up.jadpo:2:5
  |
2 |     planning.snooze_todo()
  |     ^^^^^^^^ own domain
  reason: The dot names who owns what you call, and `snooze_todo` belongs to `planning` itself, so code there calls it by its bare name.
  next: Call `snooze_todo(...)` without `planning.` (agent)
  rule: semantic.own_domain_qualification
  help: docs/language/01-project-layout-and-modules.html#semantic-own-domain-qualification

check failed: 1 error, 3 files
```
