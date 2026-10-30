# `acceptance.invalid`

An acceptance file holds one `acceptance { changes: { ... } }`, each entry
`subject: Kind.change` as `jadpo check`'s message gives it, with the reason
on a `///` line above `acceptance`. Here the reason was written as a
member instead:

```jadpo check project=e-acceptance-invalid file=app.jadpo hidden
application {}
```

```jadpo check project=e-acceptance-invalid file=acceptances/20261030T1400_remove_memo_tag.jadpo expect=acceptance.invalid
acceptance {
    reason: "No client shows the tag"
    changes: {
        main.MemoView.tag: ApiChange.output_field_removed
    }
}
```

```text output
error: An acceptance holds only `changes:`
 --> acceptances/20261030T1400_remove_memo_tag.jadpo:2:5
  |
2 |     reason: "No client shows the tag"
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not part of an acceptance
  reason: An acceptance is one `acceptance { changes: { ... } }` per file, each entry `subject: Kind.change` copied from the compiler's message, with the reason on a `///` line above `acceptance`.
  next: Remove it, and put the reason on a `///` line above `acceptance` (agent)
  rule: acceptance.invalid
  help: https://jadpo.dev/docs/errors/acceptance.invalid

  before: none, so acceptances aren't judged: not a git repository

check failed: 1 error, 2 files
```
