# `flag.value_untested`

A flag is read where no test that reaches the read gives it one of its values.

## Why Jadpo refuses it

A flag ships code switched off, then turns it on while the app runs,
with no deploy in between. Each value takes a different way through the
code, so a branch no test ran is first run by your users, the moment
someone changes the flag. Jadpo knows every read of every flag and what
each test reaches, so it checks that the tests reaching each read give
the flag every value it can have, before anyone can switch it.

A test gives a flag the value its `flags:` names, or else its
`default:`. This can't be declined: a flag is only safe to change when
each of its branches has been tested.

## What it looks like

A test of the default only:

```jadpo check project=e-flag-value-untested expect=flag.value_untested
type CheckoutSteps = Int {
    min: 1
    max: 3
}

flags {
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
```

```text output
error: No test that reaches `flags.new_checkout` gives it `true`
 --> main.jadpo:15:9
   |
15 |     if (flags.new_checkout) {
   |         ^^^^^^^^^^^^^^^^^^ not tested with every value
  reason: Each read of a flag is tested with every value it can have, so each branch is known to work before the flag is switched. The tests that reach it give it only `false`. A test that doesn't name the flag in its `flags:` gives it the `default:`, `false`.
  next: Add a test that reaches this read with `flags: { new_checkout: true }` (agent)
  rule: flag.value_untested
  help: https://jadpo.dev/docs/errors/flag.value_untested

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If a value has no test**, add one that reaches the read with that
value in its `flags:`:

```jadpo fragment
test "the new checkout takes one page" {
    flags: {
        new_checkout: true
    }
    assert checkout_steps() == 1
}
```

```jadpo check project=e-flag-value-untested-fix-1 hidden
type CheckoutSteps = Int {
    min: 1
    max: 3
}

flags {
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

Both branches are now tested, so the flag can be switched either way.

## Related

- [What the compiler checks](../language/16-built-in-services.md#what-the-compiler-checks)
- [`flags:` in a test](../language/13-testing.md#flags)
- [Flags: changed, listed and removed](../language/16-built-in-services.md#flags-changed-listed-and-removed)
