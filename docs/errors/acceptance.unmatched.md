# `acceptance.unmatched`

Each entry of an acceptance names a change this change makes, copied from
`jadpo check`'s message. An entry that matches none, such as a field that
wasn't removed, or a change accepted by an earlier acceptance already, is
an error, so an acceptance never allows more than its reason covers:

```text
error: `main.TodoView.title: ApiChange.output_field_removed` matches no change
 --> acceptances/20261030T1400_remove_todo_priority.jadpo:6:9
  |
6 |         main.TodoView.title: ApiChange.output_field_removed
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ nothing changed like this
  reason: An acceptance allows the changes this change makes, so each entry must name one `jadpo check` reports.
  next: Remove the entry, or run `jadpo acceptance new <purpose>` to list what this change needs (agent)
  rule: acceptance.unmatched
  help: https://jadpo.dev/docs/errors/acceptance.unmatched
```

[Acceptances](../language/18-records-and-manifest.md#acceptances) says
what each entry names.
