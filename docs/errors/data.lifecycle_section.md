# `data.lifecycle_section`

A lifecycle holds `visible:`, `initial:`, `transitions:` and `purge:`, each once; a transition holds `when:` and `set:`, and `purge:` holds `after:` and `since:`:

```jadpo check project=e-lifecycle-section expect=data.lifecycle_section
entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
        hidden: deleted_at != none
    }
}
```

```text output
error: `hidden` isn't part of a lifecycle
 --> main.jadpo:7:9
  |
7 |         hidden: deleted_at != none
  |         ^^^^^^ not a section here
  reason: A lifecycle holds `visible:`, `initial:`, `transitions:` and `purge:`.
  next: Remove `hidden` from the lifecycle (agent)
  rule: data.lifecycle_section
  help: docs/errors/data.lifecycle_section.md

check failed: 1 error, 1 file
```
