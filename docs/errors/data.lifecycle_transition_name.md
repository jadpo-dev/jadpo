# `data.lifecycle_transition_name`

A transition is an operation of its entity, so it can't take another operation's name or a field's:

```jadpo check project=e-lifecycle-transition-name expect=data.lifecycle_transition_name
entity Todo {
    id: Uuid
    archived_at: Instant?

    lifecycle: {
        transitions: {
            update: {
                set: {
                    archived_at: clock.now
                }
            }
        }
    }
}
```

```text output
error: A transition can't be called `update`
 --> main.jadpo:7:13
  |
7 |             update: {
  |             ^^^^^^ name in use
  reason: `Todo.update` is already an operation every entity has, so a transition can't take its name; `delete` is the one a transition may take over.
  next: Name the transition after the change it makes, such as `archive` (agent)
  rule: data.lifecycle_transition_name
  help: docs/errors/data.lifecycle_transition_name.md

check failed: 1 error, 1 file
```
