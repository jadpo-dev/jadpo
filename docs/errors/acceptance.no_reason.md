# `acceptance.no_reason`

An acceptance this change adds has no `///` line above `acceptance` saying why a person accepted the changes it lists.

## Why Jadpo refuses it

An acceptance records a person's decision that a change which could
break someone else, or widen who can do what, is safe. The reason is the
decision: it tells the reviewer of the pull request, and anyone reading
the history later, why it was safe.

`jadpo acceptance new` drafts the file listing the changes, and an agent
runs it only once you agree to them, but it writes no reason. The reason
has to come from a person: your own words, with who said them and where.
Your agent may write them on the `///` line, quoted as you said them, and
never a reason of its own; without them, it stops and asks you.

## What it looks like

A book catalogue's `BookView` stops sending each book's `author`, which
is [`acceptance.missing`](../errors/acceptance.missing.md) until a person
accepts it. You agree, and the agent drafts the acceptance:

```sh
jadpo acceptance new remove_book_author
```

```text
Wrote acceptances/20261020T1400_remove_book_author.jadpo
    main.BookView.author  ApiChange.output_field_removed

decision: 1 change waits for a person's reason
 --> acceptances/20261020T1400_remove_book_author.jadpo:1
  next: Write your user's own words agreeing to these changes on a `///` line above `acceptance`, with who said them and where; without their agreement, stop and ask them. (human)
```

The draft lists the change and nothing else:

```jadpo fragment
// acceptances/20261020T1400_remove_book_author.jadpo
acceptance {
    changes: {
        main.BookView.author: ApiChange.output_field_removed
    }
}
```

Until the reason is written, `jadpo check` fails:

```sh
jadpo check
```

```text
error: `acceptances/20261020T1400_remove_book_author.jadpo` has no reason
 --> acceptances/20261020T1400_remove_book_author.jadpo:1:1
  |
1 | acceptance {
  | ^^^^^^^^^^ no `///` reason above
  reason: An acceptance records a person's decision, so its reason is their own words, on a `///` line above `acceptance`, saying who gave them and where.
  next: Write your user's own words agreeing to these changes as the reason, with who said them and where, and never a reason of your own; if they haven't agreed, stop and ask them, and if they don't accept, delete the draft and undo the changes (human)
  rule: acceptance.no_reason
  help: https://jadpo.dev/docs/errors/acceptance.no_reason

check failed: 1 error, 8 files
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether your API's clients lose the `author` field, and why that
is safe.

Only an acceptance this change adds is judged. One already on the
default branch is history, and editing it is
[`acceptance.edited`](../errors/acceptance.edited.md).

## How to fix it

**If you accept the changes**, the `///` line directly above
`acceptance` holds your words, with who said them and where, such as
`/// Ana Ruiz (catalogue owner), in chat on 2026-10-20: "No client reads
a book's author."` You write it yourself, or your agent writes what you
told it. Then check again:

```sh
jadpo check
```

The check passes. Commit the acceptance with the change, so the pull
request's reviewer reads your reason beside the diff.

**If you don't**, delete the draft and undo the change it lists:

```sh
rm acceptances/20261020T1400_remove_book_author.jadpo
```

Until the change is undone, `jadpo check` fails with
`acceptance.missing` again.

## Related

- [Acceptances](../language/18-records-and-manifest.md#acceptances)
- [`jadpo acceptance new`](../language/14-tooling.md#jadpo-acceptance-new)
- [Review and sign-off](../language/14-tooling.md#review-and-sign-off)
- [Approve a risky change](../guides/09-get-a-sign-off.md)
