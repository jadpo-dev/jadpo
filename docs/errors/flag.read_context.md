# `flag.read_context`

A function, a constant or other pure code reads a flag, which only an action, a test or a route's `run:` can.

## Why Jadpo refuses it

A flag's value is worked out for whoever is calling, and can change while
the app runs. A function gives the same result for the same arguments,
so a flag read inside one would make it give different answers for the
same call, and its tests would no longer say what it does.

## What it looks like

```jadpo check project=e-flag-read-context expect=flag.read_context
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

function checkout_steps() -> CheckoutSteps {
    if (flags.new_checkout) {
        return 1
    }
    return 3
}
```

```text output
error: A function can't read `flags.new_checkout`
 --> main.jadpo:15:9
   |
15 |     if (flags.new_checkout) {
   |         ^^^^^^^^^^^^^^^^^^ reads a flag
  reason: A flag's value is worked out for the caller while the app runs, and a function gives the same result for the same arguments, so it can't read one; an action reads it and passes its value in as a parameter.
  next: Read `flags.new_checkout` in the action that calls this, and pass its value as a parameter (agent)
  rule: flag.read_context
  help: https://jadpo.dev/docs/errors/flag.read_context

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the function's callers are actions**, read the flag there and pass
its value in:

```jadpo fragment
function checkout_steps(new_checkout: OnePage) -> CheckoutSteps {
    if (new_checkout) {
        return 1
    }
    return 3
}

action start_checkout() -> CheckoutSteps {
    return checkout_steps(OnePage(flags.new_checkout))
}
```

```jadpo check project=e-flag-read-context-fix-1 hidden
type CheckoutSteps = Int {
    min: 1
    max: 3
}

type OnePage = Bool {}

flags {
    new_checkout: Bool {
        default: false
        owner: "payments"
        expires: "2099-01-31"
    }
}

function checkout_steps(new_checkout: OnePage) -> CheckoutSteps {
    if (new_checkout) {
        return 1
    }
    return 3
}

action start_checkout() -> CheckoutSteps {
    return checkout_steps(OnePage(flags.new_checkout))
}
```

`checkout_steps` is pure again, and its tests can pass it either value.

## Related

- [Feature flags](../language/16-built-in-services.md#feature-flags)
- [Functions and actions](../language/03-logic.md#functions-and-actions)
