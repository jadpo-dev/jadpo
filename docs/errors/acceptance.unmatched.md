# `acceptance.unmatched`

An acceptance lists a change that this change doesn't make.

## Why Jadpo refuses it

An acceptance allows exactly the changes a person read and agreed to. An
entry that matches nothing would sit in the history as a decision about
something that didn't happen, and if that change came along later, it
could be mistaken for one a person had already agreed to. So each entry
has to name a change this change makes, copied from `jadpo check`'s
message.

## What it looks like

A branch adds an acceptance for a field it never removed:

```jadpo check project=e-acceptance-unmatched file=app.jadpo hidden
application {}
```

`acceptances/20261030T1400_remove_todo_title.jadpo`:

```jadpo check project=e-acceptance-unmatched file=acceptances/20261030T1400_remove_todo_title.jadpo
/// Nothing reads the title any more.
acceptance {
    changes: {
        main.TodoView.title: ApiChange.output_field_removed
    }
}
```

```sh run project=e-acceptance-unmatched expect=acceptance.unmatched
export GIT_AUTHOR_NAME=Ada GIT_AUTHOR_EMAIL=ada@example.com GIT_COMMITTER_NAME=Ada GIT_COMMITTER_EMAIL=ada@example.com
export GIT_AUTHOR_DATE=2026-10-20T14:00:00Z GIT_COMMITTER_DATE=2026-10-20T14:00:00Z
mv acceptances ../acceptances
git init -q -b main
git add app.jadpo
git commit -q -m "chore: start the project"
git switch -q -c change
mv ../acceptances acceptances
jadpo check
```

```text output
error: `main.TodoView.title: ApiChange.output_field_removed` matches no change
 --> acceptances/20261030T1400_remove_todo_title.jadpo:4:9
  |
4 |         main.TodoView.title: ApiChange.output_field_removed
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ nothing changed like this
  reason: An acceptance allows the changes this change makes, so each entry must name one `jadpo check` reports.
  next: Remove the entry, or run `jadpo acceptance new <purpose>` to list what this change needs (agent)
  rule: acceptance.unmatched
  help: https://jadpo.dev/docs/errors/acceptance.unmatched

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An entry for a change that an earlier acceptance already allowed is the
same error, since it allows nothing new.

## How to fix it

**If the change was never made**, remove the entry, or the whole file if
it was the only one:

```sh
rm acceptances/20261030T1400_remove_todo_title.jadpo
jadpo check
```

Nothing claims a change that didn't happen, and the check passes.

**If the change is under a different name or kind**, let the command list
what this change really needs:

```sh
jadpo acceptance new remove_todo_title
```

The new draft lists each change `jadpo check` reports, and a person writes
why they are accepted.

## Related

- [Acceptances](../language/18-records-and-manifest.md#acceptances)
- [`jadpo acceptance new`](../language/14-tooling.md#jadpo-acceptance-new)
- [`acceptance.missing`](../errors/acceptance.missing.md)
