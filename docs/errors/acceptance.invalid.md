# `acceptance.invalid`

An acceptance file holds something other than one `acceptance { changes: { ... } }`.

## Why Jadpo refuses it

An acceptance file is read by `jadpo check`, which matches each entry
against the changes it finds, so a file it can't read allows nothing. It
holds one `acceptance { changes: { ... } }`, each entry
`subject: Kind.change` as `jadpo check`'s message gives it, with the reason
on a `///` line above `acceptance`.

## What it looks like

The reason is written as a member, not on a `///` line:

```jadpo check project=e-acceptance-invalid file=app.jadpo hidden
application {}
```

`acceptances/20261030T1400_remove_memo_tag.jadpo`:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An entry whose kind isn't one Jadpo knows is the same error:

```jadpo check project=e-acceptance-invalid-kind file=app.jadpo hidden
application {}
```

`acceptances/20261030T1400_remove_memo_tag.jadpo`:

```jadpo check project=e-acceptance-invalid-kind file=acceptances/20261030T1400_remove_memo_tag.jadpo expect=acceptance.invalid
/// No client shows the tag.
acceptance {
    changes: {
        main.MemoView.tag: MemoChange.tag_removed
    }
}
```

```text output
error: This isn't a change an acceptance can list
 --> acceptances/20261030T1400_remove_memo_tag.jadpo:4:9
  |
4 |         main.MemoView.tag: MemoChange.tag_removed
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ expected `subject: Kind.change`
  reason: Each entry names what changed and how, as the compiler's message gives it, such as `<api>.<Type>.<field>: ApiChange.output_field_removed`; the kinds are `ApiChange`, `DataChange`, `DomainChange`, `ErasureNotPossible`, `NetworkChange`, `PackageChange`, `PolicyChange` and `ThresholdChange`.
  next: Copy the entry from `jadpo check`'s message, or run `jadpo acceptance new <purpose>` (agent)
  rule: acceptance.invalid
  help: https://jadpo.dev/docs/errors/acceptance.invalid

  before: none, so acceptances aren't judged: not a git repository

check failed: 1 error, 2 files
```

A file that holds no acceptance, holds anything beside it, lists no
change, or has a `changes:` that isn't a list is this error too, each with
its own message.

## How to fix it

**If the reason is written as a member**, move it to a `///` line above
`acceptance`:

```jadpo check project=e-acceptance-invalid-fix file=acceptances/20261030T1400_remove_memo_tag.jadpo
/// No client shows the tag.
acceptance {
    changes: {
        main.MemoView.tag: ApiChange.output_field_removed
    }
}
```

```jadpo check project=e-acceptance-invalid-fix file=app.jadpo hidden
application {}
```

The acceptance now holds only its changes, and its reason is where a
reviewer reads it.

**If an entry isn't one `jadpo check` reported**, copy it from the error
that asked for the acceptance, or let the command draft the file:

```sh
jadpo acceptance new remove_memo_tag
```

The draft lists each change not yet accepted, as `subject: Kind.change`,
and a person writes why.

## Related

- [Acceptances](../language/18-records-and-manifest.md#acceptances)
- [`jadpo acceptance new`](../language/14-tooling.md#jadpo-acceptance-new)
