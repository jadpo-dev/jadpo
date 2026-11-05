# Ship a change behind a flag
<!-- topic: guide-ship-a-change-behind-a-flag -->

A feature flag lets you merge and deploy a change switched off, then turn
it on for some users or for everyone without another deploy. Jadpo knows
every read of every flag, so a misspelt flag is a compile error, both sides
of each flag must have a test, and removing a flag is one command. This
guide puts a shop's new one-page checkout behind a flag, switches it on in
a running app, and removes the flag.

You start from an app with a `GET /checkout` route that runs an action
called `start_checkout`, which answers with how many pages the checkout
takes. A line in `policy.jadpo` lets anyone call it, since it reaches no
entity that would say who may.
[Build an API](01-build-an-api.md) shows how to write a route.

```jadpo check project=guide-flags file=app.jadpo hidden
application {}
```

```jadpo check project=guide-flags file=policy.jadpo hidden
use shop

policy {
    actions: {
        shop.start_checkout: [Access.public]
    }
}
```

```jadpo check project=guide-flags file=domains/shop/routes/checkout.jadpo hidden
use shop/types { CheckoutShown }
use shop/actions { start_checkout }

/// The checkout.
routes Checkout {
    prefix: /checkout

    /// The checkout this customer is shown.
    GET / {
        authentication: RouteAuthentication.none
        output: CheckoutShown
        run: start_checkout()
    }
}
```

## Declare the flag

Every flag is declared once, in `flags.jadpo` at the root of the project.
`flags { }` and `Bool` are built into Jadpo, and the name `new_checkout` is
yours. `default:` is what everyone gets until you change it. `owner:` and
`expires:` make sure a flag doesn't stay for good: past that date
`jadpo check` fails with an error naming the owner.

A flag is meant to be short-lived. You deploy it switched off, turn it on, and
remove it with its old code once the change has proved itself, so give yours an
`expires:` a few weeks ahead. The date here is far off only so that this example
never expires.

`flags.jadpo`:

```jadpo check project=guide-flags file=flags.jadpo
flags {
    /// The one-page checkout replacing the three-step form.
    new_checkout: Bool {
        default: false
        owner: "payments"
        expires: "2099-01-31"
    }
}
```

A flag that chooses between named options, such as two layouts, is an enum
instead of a `Bool`.

## Read it

An action reads the flag as `flags.new_checkout`. The value stays the same
for the whole operation, so one request never sees it change halfway
through. You don't pass the caller: Jadpo already knows who is asking. Reading
a flag sends no query, because each server holds the values in memory and rereads
them from your database at most every 5 seconds, as a request begins.

`CheckoutShown` is what the action answers with, a type of your own:

`domains/shop/types/checkout_shown.jadpo`:

```jadpo check project=guide-flags file=domains/shop/types/checkout_shown.jadpo
/// The checkout a customer is shown.
type CheckoutShown = Output {
    page_count: Int {
        min: 1
        max: 3
    }
}
```

`domains/shop/actions/start_checkout.jadpo`, changed. The `if` is new:

```jadpo check project=guide-flags file=domains/shop/actions/start_checkout.jadpo
use shop/types { CheckoutShown }

/// The checkout this customer is shown.
action start_checkout() -> CheckoutShown {
    if (flags.new_checkout) {
        return CheckoutShown {
            page_count: 1
        }
    }
    return CheckoutShown {
        page_count: 3
    }
}
```

Actions and a route's `run:` arguments can read a flag. Functions and
queries can't, so you pass them its value as a parameter.

## Test both sides

`jadpo check` won't pass until tests reach this read with every value the
flag can have, so the path you are about to switch on is known to work:

```text
error: No test reaches this read of `flags.new_checkout`
 --> domains/shop/actions/start_checkout.jadpo:5:9
  |
5 |     if (flags.new_checkout) {
  |         ^^^^^^^^^^^^^^^^^^ not tested with every value
  reason: Each read of a flag is tested with every value it can have, so each branch is known to work before the flag is switched. No test reaches it. A test that doesn't name the flag in its `flags:` gives it the `default:`, `false`.
  next: Add a test that reaches this read and leaves `new_checkout` at its default, `false` (agent)
  rule: flag.value_untested
  help: https://jadpo.dev/docs/errors/flag.value_untested
```

A test chooses a value with `flags:`, and a flag it doesn't name has its
`default:`.

`domains/shop/actions/start_checkout.test.jadpo`:

```jadpo check project=guide-flags file=domains/shop/actions/start_checkout.test.jadpo
use shop/types { CheckoutShown }

test "the three-step form is shown by default" {
    var shown: CheckoutShown = start_checkout()
    assert shown.page_count == 3
}

test "the new checkout takes one page" {
    flags: {
        new_checkout: true
    }
    var shown: CheckoutShown = start_checkout()
    assert shown.page_count == 1
}
```

A flag's state lives in your app's own PostgreSQL database, in tables Jadpo
owns, such as `jadpo_flags`. The first flag needs a migration to make them:

```sh
jadpo migrate new add_flags
```

```text
  compared the entities with an empty database

  System tables
    + table  jadpo_flag_changes  system table for flag changes
    + table  jadpo_flags         system table for flags
    + table  jadpo_housekeeping  system table for housekeeping

Wrote database/migrations/20261104T1115_add_flags/
    snapshot.json  the whole schema at this point
    postgres.sql   4 statements: create table, create index

next: jadpo check  confirms the migration matches the entities
```

```sh
jadpo test
```

```text
tested: 2 passed, 2 action tests
```

Now you can merge and deploy. Nobody sees the new checkout yet.

## Switch it on

With the app running under `jadpo dev`, the checkout takes three pages:

```sh
curl http://localhost:3000/checkout
```

```text
{"page_count":3}
```

`jadpo flags` changes the flag in the running app's database, with no deploy:

```sh
jadpo flags set new_checkout true
```

```text
Set `new_checkout`
    was      false for everyone (the default)
    now      true for everyone
    logged   ann · role jadpo
    applies  within 10 s, on every server
```

Each server rereads its flags every few seconds, so ask again a little later:

```sh
curl http://localhost:3000/checkout
```

```text
{"page_count":1}
```

If it goes wrong, one command puts everyone back on the default. Nothing is
redeployed, and every change is logged with who made it:

```sh
jadpo flags reset new_checkout
```

```text
Reset `new_checkout`: everyone gets its default
    was      true for everyone
    now      false for everyone (the default)
    logged   ann · role jadpo
    applies  within 10 s, on every server
```

`jadpo flags` finds the database as other operator commands do: a running
`jadpo dev`'s, the address its connection's setting gives, or the one `--url`
names. So production's flags are changed with production's address.

## Roll it out a little at a time

A flag doesn't have to be all or nothing. `--percent` gives a value to a
share of signed-in users, and `--users` to the users you name, by their `id`.
A named user comes first, then the share, then everyone else.

```sh
jadpo flags set new_checkout --percent 10
```

```text
Set `new_checkout`
    was      false for everyone (the default)
    now      true for 10% of users; false (the default) for everyone else
    logged   ann · role jadpo
    applies  within 10 s, on every server
```

The same user always gets the same answer, because their share is worked out
from a hash of the flag's name and their `id`, with no lookup. Raising 10% to 20%
keeps the first 10% in. A caller who isn't signed in gets the everyone value, and so do
schedules and subscribers, which have no caller.

`jadpo flags list` shows where each flag stands:

```sh
jadpo flags list
```

```text
  flag          everyone  users  shares    changed     state
  new_checkout  false         –  true 10%  2026-11-04  * rolling out

1 flag: main  /private/var/folders/c3/bd2p8dtn3vs2wk9ld2fmk4k00000gn/T/jadpo-pg-75891-0/jadpo_f811f521d4_0_dev_main
```

It marks a flag "ready to remove" once every caller has had one value for
seven days.

## Show it to a browser

A page that has to know a flag's value can read it from the app. Add
`client: true` to the flag, and Jadpo generates `GET /flags`, answered for
whoever the request signed in as. A flag without `client: true` never leaves
the server.

## Remove the flag

Once the change has proved itself, the flag and the old code have to go. Jadpo
knows every read, so it does that for you:

```sh
jadpo flags remove new_checkout --keep true
```

```text
  domains/shop/actions/start_checkout.jadpo
    ~ flags.new_checkout                              1 read, 1 `if` decided by true

  domains/shop/actions/start_checkout.test.jadpo
    - test "the three-step form is shown by default"  tested a branch that goes
    - new_checkout: true                              in test "the new checkout takes one page"

  flags.jadpo
    - new_checkout                                    flag, the last: `flags { }` stays, keeping its tables

Removed `new_checkout`
    kept   true
    tests  1 removed, each tested a branch that went

next: jadpo test  runs the tests that are left
```

The `if` has gone, along with the three-step branch and its test, and the
project still checks. Review the diff and commit it as you would any change.

## Read more

[Feature flags](../language/16-built-in-services.md#feature-flags),
[Flag rules: declaring, reading, testing](../language/16-built-in-services.md#flag-rules-declaring-reading-testing),
[Flag rules: changing, listing, removing](../language/16-built-in-services.md#flag-rules-changing-listing-removing),
[`jadpo flags`](../language/14-tooling.md#jadpo-flags) and
[Test settings](../language/13-testing.md#test-settings).

## Not built yet

- Giving a named tenant a fixed value isn't built yet. Name the tenant's
  users with `--users` for now.
- Erasing a user doesn't yet remove their `id` from `jadpo_flags` and
  `jadpo_flag_changes`, so a user named with `--users` stays there. Prefer
  `--percent` where you can.
