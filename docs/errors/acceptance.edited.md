# `acceptance.edited`

An acceptance already on the default branch was edited.

## Why Jadpo refuses it

An acceptance records a decision a person made about one change, and it
counts only in the change that adds it. Once it is on the default branch
it is history: it has done its job, and it allows nothing new. Editing one
afterwards, to list another change or reword its reason, would rewrite the
record of what was decided, so `jadpo check` refuses it.

A new decision is a new file, which the pull request's reviewer reads
beside the change it allows.

## What it looks like

An acceptance already on the default branch:

```jadpo check project=e-acceptance-edited file=app.jadpo hidden
application {}
```

`acceptances/20261020T1400_remove_todo_priority.jadpo`:

```jadpo check project=e-acceptance-edited file=acceptances/20261020T1400_remove_todo_priority.jadpo
/// Priority was never shown by any client.
acceptance {
    changes: {
        main.TodoView.priority: ApiChange.output_field_removed
    }
}
```

A branch rewords its reason:

```sh run project=e-acceptance-edited expect=acceptance.edited
export GIT_AUTHOR_NAME=Ada GIT_AUTHOR_EMAIL=ada@example.com GIT_COMMITTER_NAME=Ada GIT_COMMITTER_EMAIL=ada@example.com
export GIT_AUTHOR_DATE=2026-10-20T14:00:00Z GIT_COMMITTER_DATE=2026-10-20T14:00:00Z
git init -q -b main
git add app.jadpo acceptances
git commit -q -m "feat: remove the priority field"
git switch -q -c reword
cat > acceptances/20261020T1400_remove_todo_priority.jadpo <<'END'
/// No client shows the priority.
acceptance {
    changes: {
        main.TodoView.priority: ApiChange.output_field_removed
    }
}
END
jadpo check
```

```text output
error: `acceptances/20261020T1400_remove_todo_priority.jadpo` was accepted before this change
 --> acceptances/20261020T1400_remove_todo_priority.jadpo:2:1
  |
2 | acceptance {
  | ^^^^^^^^^^ edited since
  reason: It is already on the before side (main at 42bee27), so it is history: it allowed the change that added it, and allows nothing new.
  next: Put it back with `git restore --source=42bee27 acceptances/20261020T1400_remove_todo_priority.jadpo`, and list new changes in an acceptance of their own (agent)
  rule: acceptance.edited
  help: https://jadpo.dev/docs/errors/acceptance.edited

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the edit shouldn't have happened**, put the file back as it was on
the default branch, using the command the error gives:

```sh
git restore --source=42bee27 acceptances/20261020T1400_remove_todo_priority.jadpo
jadpo check
```

The acceptance is as it was accepted, and the check passes.

**If there is a new change to accept**, restore the old file as above, then
draft a new acceptance for the new change:

```sh
jadpo acceptance new remove_todo_title
```

The new file lists the new change, and a person writes its reason.

## Related

- [Acceptances](../language/18-records-and-manifest.md#acceptances)
- [Comparing two builds](../language/18-records-and-manifest.md#comparing-two-builds)
- [`jadpo acceptance new`](../language/14-tooling.md#jadpo-acceptance-new)
- [`acceptance.no_reason`](../errors/acceptance.no_reason.md)
