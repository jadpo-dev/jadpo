# `acceptance.edited`

An acceptance counts only in the change that adds it. Once it is on the
default branch it is history, so editing it, to list another change or
reword its reason, is an error. A new decision is a new file:

```text
error: `acceptances/20261020T1400_remove_todo_priority.jadpo` was accepted before this change
 --> acceptances/20261020T1400_remove_todo_priority.jadpo:2:1
  |
2 | acceptance {
  | ^^^^^^^^^^ edited since
  reason: It is already on the before side (origin/main at 8d113e6), so it is history: it allowed the change that added it, and allows nothing new.
  next: Put it back with `git restore --source=8d113e6 acceptances/20261020T1400_remove_todo_priority.jadpo`, and list new changes in an acceptance of their own (agent)
  rule: acceptance.edited
  help: https://jadpo.dev/docs/errors/acceptance.edited
```

[Acceptances](../language/18-records-and-manifest.md#acceptances) says
how the before side is found.
