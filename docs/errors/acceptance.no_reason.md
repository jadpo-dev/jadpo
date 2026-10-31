# `acceptance.no_reason`

An acceptance this change adds has no `///` line above `acceptance` saying why a person accepted the changes it lists.

## Why Jadpo refuses it

An acceptance records a person's decision that a change which could
break someone else, or widen who can do what, is safe. The reason is the
decision: it tells the reviewer of the pull request, and anyone reading
the history later, why it was safe.

`jadpo acceptance new` drafts the file listing the changes, and an agent
may run it, but it writes no reason. The reason has to come from a
person, so Jadpo won't write it, and neither should your agent.

## What it looks like

A book catalogue's `BookView` stops sending each book's `author`, which
is [`acceptance.missing`](../errors/acceptance.missing.md) until a person
accepts it. The agent drafts the acceptance:

```sh
jadpo acceptance new remove_book_author
```

```text
Wrote acceptances/20261020T1400_remove_book_author.jadpo
    main.BookView.author  ApiChange.output_field_removed

decision: 1 change waits for a person's reason
 --> acceptances/20261020T1400_remove_book_author.jadpo:1
  next: Stop and hand over: a person reads the changes and writes why they are accepted on a `///` line above `acceptance`. (human)
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

Until a person writes the reason, `jadpo check` fails:

```sh
jadpo check
```

```text
error: `acceptances/20261020T1400_remove_book_author.jadpo` has no reason
 --> acceptances/20261020T1400_remove_book_author.jadpo:1:1
  |
1 | acceptance {
  | ^^^^^^^^^^ no `///` reason above
  reason: An acceptance records a person's decision, so it says why on a `///` line above `acceptance`; an agent writes the draft and stops there.
  next: A person writes why these changes are accepted on a `///` line above `acceptance` (human)
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

**If you accept the changes**, write why on a `///` line directly above
`acceptance`, in your own words, such as "No client reads a book's
author: the catalogue pages stopped showing it in version 2." Then check
again:

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
- [Get a person's sign-off](../guides/09-get-a-sign-off.md)
