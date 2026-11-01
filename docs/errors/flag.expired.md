# `flag.expired`

A flag's `expires:` date has passed while the flag is still declared.

## Why Jadpo refuses it

A flag is for releasing code, so it should be gone once the release is
done. While it stays, both of its branches stay in the code, each tested.
The date in `expires:` is when its owner said it would be gone, so once
it passes `jadpo check` fails, naming them. You can't decline it: keeping
a flag longer means editing its date, so the choice shows in review.

`jadpo check` judges the date at today, so CI fails a new change.
`jadpo build` judges it at the commit's date, so rebuilding a merged
revision for a hotfix is never blocked by a date passing.

## What it looks like

```jadpo check project=e-flag-expired expect=flag.expired
type CheckoutSteps = Int {
    min: 1
    max: 3
}

flags {
    /// The one-page checkout replacing the three-step form.
    new_checkout: Bool {
        default: false
        owner: "payments"
        expires: "2026-01-31"
    }
}

action checkout_steps() -> CheckoutSteps {
    if (flags.new_checkout) {
        return 1
    }
    return 3
}

test "the three-step form is shown by default" {
    assert checkout_steps() == 3
}

test "the new checkout takes one page" {
    flags: {
        new_checkout: true
    }
    assert checkout_steps() == 1
}
```

```text output
error: `new_checkout` expired on 2026-01-31
 --> main.jadpo:11:18
   |
11 |         expires: "2026-01-31"
   |                  ^^^^^^^^^^^^ past its date
  reason: `new_checkout` is owned by "payments", and was to be gone by 2026-01-31. A flag left in place keeps both of its branches in the code; removing it keeps one, and moving `expires:` on is the reviewed choice to keep it longer, so this can't be declined.
  next: Ask "payments" whether to remove `new_checkout` or move its `expires:` date on (human)
  rule: flag.expired
  help: https://jadpo.dev/docs/errors/flag.expired

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether the new checkout replaces the old for everyone, or stays
switchable for longer.

## How to fix it

The question for the flag's owner is whether the release is done.

**If it is done**, remove the flag and the `flags:` your tests give it,
keeping the branch that won. `jadpo flags remove new_checkout --keep
true` makes the change for you
([Removing a flag](../language/16-built-in-services.md#removing-a-flag)):

```jadpo fragment
action checkout_steps() -> CheckoutSteps {
    return 1
}
```

```jadpo check project=e-flag-expired-fix-1 hidden
type CheckoutSteps = Int {
    min: 1
    max: 3
}

action checkout_steps() -> CheckoutSteps {
    return 1
}
```

Everyone now gets the one-page checkout, and the three-step form is gone
from the code.

**If it needs longer**, move its date on:

```jadpo fragment
    new_checkout: Bool {
        default: false
        owner: "payments"
        expires: "2099-01-31"
    }
```

```jadpo check project=e-flag-expired-fix-2 hidden
type CheckoutSteps = Int {
    min: 1
    max: 3
}

flags {
    /// The one-page checkout replacing the three-step form.
    new_checkout: Bool {
        default: false
        owner: "payments"
        expires: "2099-01-31"
    }
}

action checkout_steps() -> CheckoutSteps {
    if (flags.new_checkout) {
        return 1
    }
    return 3
}

test "the three-step form is shown by default" {
    assert checkout_steps() == 3
}

test "the new checkout takes one page" {
    flags: {
        new_checkout: true
    }
    assert checkout_steps() == 1
}
```

The flag keeps working as before until the new date, and the change to
it is what a reviewer sees.

## Related

- [What the compiler checks](../language/16-built-in-services.md#what-the-compiler-checks)
- [Rules triggered by time passing](../language/01-project-layout-and-modules.md#rules-triggered-by-time-passing)
- [Removing a flag](../language/16-built-in-services.md#removing-a-flag)
