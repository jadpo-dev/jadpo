# `flag.declaration_invalid`

A flag in `flags { }` isn't a `Bool` or an enum of plain variants, or its `default:`, `owner:` or `expires:` is missing or isn't valid.

## Why Jadpo refuses it

Each flag says three things: its `default:`, what everyone gets until it
is changed; its `owner:`, who decides when it goes; and `expires:`, the
date it should be gone by. Without the date, a flag stays in the code for
good, both its branches kept and tested; without an owner, no one is
asked when it is due. A flag is switched between a few values while the
app runs, so its type is a `Bool` or an enum whose variants carry no
data, which a test can set and a change can name.

## What it looks like

`new_checkout` says no date it should be gone by:

```jadpo check project=e-flag-declaration expect=flag.declaration_invalid
flags {
    new_checkout: Bool {
        default: false
        owner: "payments"
    }
}
```

```text output
error: `new_checkout` has no `expires:`
 --> main.jadpo:2:5
  |
2 |     new_checkout: Bool {
  |     ^^^^^^^^^^^^ missing what a flag says
  reason: Every flag says its `default:`, the value everyone gets until it is changed; its `owner:`, who decides when it goes; and `expires:`, the date it should be gone by, after which `jadpo check` fails until it is removed or the date moved on.
  next: Add `expires:` to `new_checkout` (agent)
  rule: flag.declaration_invalid
  help: https://jadpo.dev/docs/errors/flag.declaration_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A flag of a type that holds more than a few values is refused too:

```jadpo check project=e-flag-declaration-type expect=flag.declaration_invalid
flags {
    checkout_title: Text {
        default: "Checkout"
        owner: "payments"
        expires: "2099-01-31"
    }
}
```

```text output
error: `checkout_title` can't be a `Text`
 --> main.jadpo:2:21
  |
2 |     checkout_title: Text {
  |                     ^^^^ not a flag's type
  reason: A flag is switched between a few values while the app runs, so it is a `Bool`, or an enum whose variants carry no data.
  next: Make `checkout_title` a `Bool`, or an enum of its values (agent)
  rule: flag.declaration_invalid
  help: https://jadpo.dev/docs/errors/flag.declaration_invalid

check failed: 1 error, 1 file
```

## How to fix it

**If the flag has no date**, give it the date it should be gone by:

```jadpo fragment
    new_checkout: Bool {
        default: false
        owner: "payments"
        expires: "2099-01-31"
    }
```

```jadpo check project=e-flag-declaration-fix-1 hidden
flags {
    new_checkout: Bool {
        default: false
        owner: "payments"
        expires: "2099-01-31"
    }
}
```

`jadpo check` now fails once that date has passed, naming `payments`, until
the flag is removed or the date moved on.

**If the flag chooses between a few values**, make it an enum of them:

```jadpo fragment
type CheckoutTitle = Enum {
    checkout
    pay_now
}

flags {
    checkout_title: CheckoutTitle {
        default: CheckoutTitle.checkout
        owner: "payments"
        expires: "2099-01-31"
    }
}
```

```jadpo check project=e-flag-declaration-fix-2 hidden
type CheckoutTitle = Enum {
    checkout
    pay_now
}

flags {
    checkout_title: CheckoutTitle {
        default: CheckoutTitle.checkout
        owner: "payments"
        expires: "2099-01-31"
    }
}
```

Each title is now a value a test's `flags:` can give and a change can
switch to; the words each shows belong in the app's translations.

## Related

- [Feature flags](../language/16-built-in-services.md#feature-flags)
- [What flags are not for](../language/16-built-in-services.md#what-flags-are-not-for)
