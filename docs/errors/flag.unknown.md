# `flag.unknown`

Code or a test's `flags:` names a flag that `flags { }` doesn't declare.

## Why Jadpo refuses it

Every flag is declared once, in `flags.jadpo`, so Jadpo knows every place
that reads one. A misspelt name would otherwise read nothing: the branch
it guards would never switch on, and no error would say why.

## What it looks like

`checkout_steps` reads `flags.new_chekout`, and the flag is
`new_checkout`:

```jadpo check project=e-flag-unknown expect=flag.unknown
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
    if (flags.new_chekout) {
        return 1
    }
    return 3
}
```

```text output
error: `flags.new_chekout` isn't a flag
 --> main.jadpo:15:9
   |
15 |     if (flags.new_chekout) {
   |         ^^^^^^^^^^^^^^^^^ no such flag
  reason: `flags { }` declares no flag called `new_chekout`. Did you mean `new_checkout`?
  next: Use `flags.new_checkout` instead of `flags.new_chekout` (agent)
  rule: flag.unknown
  help: https://jadpo.dev/docs/errors/flag.unknown

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A test's `flags:` naming a flag no one declared is refused the same way,
with nothing near enough to suggest:

```jadpo check project=e-flag-unknown-test expect=flag.unknown
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

test "the new checkout takes one page" {
    flags: {
        one_page_checkout: true
    }
    assert checkout_steps() == 1
}
```

```text output
error: `one_page_checkout` isn't a flag
 --> main.jadpo:23:9
   |
23 |         one_page_checkout: true
   |         ^^^^^^^^^^^^^^^^^ no such flag
  reason: `flags { }` declares no flag called `one_page_checkout`.
  next: Declare `one_page_checkout: Bool { ... }` in `flags.jadpo`, or remove `one_page_checkout:` from `flags:` (agent)
  rule: flag.unknown
  help: https://jadpo.dev/docs/errors/flag.unknown

check failed: 1 error, 1 file
```

## How to fix it

**If the flag is declared under another name**, use that name:

```jadpo fragment
    if (flags.new_checkout) {
        ...
    }
```

```jadpo check project=e-flag-unknown-fix-1 hidden
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

`checkout_steps` now reads `new_checkout`, so it takes one page wherever
the flag is on.

**If the flag doesn't exist yet**, declare it in `flags { }` with its
`default:`, `owner:` and `expires:`:

```jadpo fragment
flags {
    one_page_checkout: Bool {
        default: false
        owner: "payments"
        expires: "2099-01-31"
    }
}
```

```jadpo check project=e-flag-unknown-fix-2 hidden
type CheckoutSteps = Int {
    min: 1
    max: 3
}

flags {
    one_page_checkout: Bool {
        default: false
        owner: "payments"
        expires: "2099-01-31"
    }
}

action checkout_steps() -> CheckoutSteps {
    if (flags.one_page_checkout) {
        return 1
    }
    return 3
}

test "the three-step form is shown by default" {
    assert checkout_steps() == 3
}

test "the new checkout takes one page" {
    flags: {
        one_page_checkout: true
    }
    assert checkout_steps() == 1
}
```

The test now gives `one_page_checkout` the value `true`, and everyone
else gets `false` until the flag is changed.

## Related

- [Feature flags](../language/16-built-in-services.md#feature-flags)
- [What the compiler checks](../language/16-built-in-services.md#what-the-compiler-checks)
- [Test settings](../language/13-testing.md#test-settings)
