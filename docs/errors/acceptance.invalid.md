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
