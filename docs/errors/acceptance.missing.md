# `acceptance.missing`

A change could break someone else or widen who can do what, and no acceptance this change adds says a person agreed to it.

## Why Jadpo refuses it

Some changes can hurt someone outside the code: removing a field a
client reads, opening a route to anyone, adding a caller to a grant,
reaching a new outside host, declining a security rule, creating a
domain, installing a package from git, or a migration removing data
([Removing data](../language/05-data.md#removing-data)). Whether such a change is safe
depends on who relies on the old behaviour, which the code can't know.

So a person decides. `jadpo check` compares the change with the default
branch where this branch left it, and fails on each such change until an
acceptance the change adds lists it. An acceptance is a small file in
`acceptances/` that lists the changes and says why, in a person's words.

## What it looks like

A book catalogue's `BookView` stops sending each book's `author`, and
`jadpo api update` rewrites the API record without it. Then:

```sh
jadpo check
```

```text
error: removing an output field can break clients: main.BookView.author
 --> api/main.jadpo_record:21:1
   |
21 | output BookView
   | ^^^^^^^^^^^^^^^ ApiChange.output_field_removed
  reason: Clients that read it find it gone. So a person accepts it: no acceptance this change adds lists `main.BookView.author: ApiChange.output_field_removed`.
  next: A person accepts it: `jadpo acceptance new <purpose>` writes the draft for them (human)
  rule: acceptance.missing
  help: https://jadpo.dev/docs/errors/acceptance.missing

check failed: 1 error, 7 files
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether your API's clients lose the `author` field.

## How to fix it

The question is whether the change should happen at all.

**If it shouldn't**, put back what it removed: `Book.author` in
`BookView`'s `pick:`, and `author: book.author` in what `add_book`
returns. Then rewrite the API record and check again:

```sh
jadpo api update
jadpo check
```

The API record matches the default branch's again, so there is nothing
to accept and the check passes.

**If it should**, draft the acceptance. An agent may run this:

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

The draft lists every change this branch makes that needs a person, each
as `subject: Kind.change`. Then the agent stops, because `jadpo check`
fails with [`acceptance.no_reason`](../errors/acceptance.no_reason.md)
until a person writes why on the `///` line above `acceptance`, such
as "No client reads a book's author: the catalogue pages stopped showing
it in version 2."

Once the reason is written, `jadpo check` passes. The acceptance is
committed with the change, and the pull request's review is where an
owner of `acceptances/` approves it. Once merged, it is history.

## Related

- [Acceptances](../language/18-records-and-manifest.md#acceptances)
- [The public API record](../language/07-http.md#the-public-api-record)
- [Review and sign-off](../language/14-tooling.md#review-and-sign-off)
- [`jadpo acceptance new`](../language/14-tooling.md#jadpo-acceptance-new)
- [Get a person's sign-off](../guides/09-get-a-sign-off.md)
