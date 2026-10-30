# `acceptance.no_reason`

An acceptance records a person's decision, so it says why on a `///` line
above `acceptance`. `jadpo acceptance new` drafts the file without one,
and `jadpo check` fails until a person writes it:

```jadpo fragment
// acceptances/20261030T1400_remove_todo_priority.jadpo
acceptance {
    changes: {
        main.TodoView.priority: ApiChange.output_field_removed
    }
}
```

```text
error: `acceptances/20261030T1400_remove_todo_priority.jadpo` has no reason
 --> acceptances/20261030T1400_remove_todo_priority.jadpo:1:1
  |
1 | acceptance {
  | ^^^^^^^^^^ no `///` reason above
  reason: An acceptance records a person's decision, so it says why on a `///` line above `acceptance`; an agent writes the draft and stops there.
  next: A person writes why these changes are accepted on a `///` line above `acceptance` (human)
  rule: acceptance.no_reason
  help: https://jadpo.dev/docs/errors/acceptance.no_reason
```

Only an acceptance this change adds is judged: one already on the default
branch is history ([Acceptances](../language/18-records-and-manifest.md#acceptances)).
