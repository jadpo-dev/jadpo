# `decline.not_declinable`

A `decline` line names a rule that can't be declined, or no rule at all.

## Why Jadpo refuses it

A few rules ask for a judgement: they point at code that is often right to
leave, such as a function that looks like one in a library, and those can be
declined. Every other rule reports a mistake, which a reason can't answer,
because the app would still be wrong. The error's `reason:` lists the rules
that can be declined.

## What it looks like

main.jadpo:

```jadpo check project=e-decline-not-declinable expect=decline.not_declinable
type Stock = Int {
    min: 0
}

decline layout.unused_declaration: "Kept for next quarter"
function cap_total(total: Stock) -> Stock {
    return total
}
```

```text output
error: `layout.unused_declaration` can't be declined
 --> main.jadpo:5:9
  |
5 | decline layout.unused_declaration: "Kept for next quarter"
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^ a plain mistake
  reason: Only judgement calls can be declined: `constant.similar_exists`, `data.personal_unread`, `email.field_unused`, `email.health_problem`, `library.similar_exists`, `package.standard_candidate`, `policy.call_ungranted`, `query.nearest_exact`, `rate_limits.counted_in_memory`, `step.late_failure`, `translation.field_unused`, `type.batch_order_updatable`, `type.password_sent_to_provider`, `type.standard_exists`, `type.unit_as_number` and `type.variant_retired`. `layout.unused_declaration` reports a mistake, which only a fix answers.
  next: Remove the `decline` line, and fix what `layout.unused_declaration` reports if it fires (agent)
  rule: decline.not_declinable
  help: https://jadpo.dev/docs/errors/decline.not_declinable

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A name Jadpo has no rule for is reported the same way, with the nearest
rule that can be declined:

main.jadpo:

```jadpo check project=e-decline-unknown-rule expect=decline.not_declinable
type Stock = Int {
    min: 0
}

decline library.similar_exist: "Caps the basket for the till, which changes on its own"
function cap_total(total: Stock) -> Stock {
    return total
}
```

```text output
error: `library.similar_exist` isn't a rule
 --> main.jadpo:5:9
  |
5 | decline library.similar_exist: "Caps the basket for the till, which changes on its own"
  |         ^^^^^^^^^^^^^^^^^^^^^ no rule with this id
  reason: Jadpo has no rule `library.similar_exist`, so there is nothing to decline. Did you mean `library.similar_exists`?
  next: Use `library.similar_exists` instead of `library.similar_exist` (agent)
  rule: decline.not_declinable
  help: https://jadpo.dev/docs/errors/decline.not_declinable

check failed: 1 error, 1 file
```

## How to fix it

**If the rule reports a mistake**, remove the `decline` line, and fix what
the rule reports if it fires:

```jadpo fragment
function cap_total(total: Stock) -> Stock {
    ...
}
```

```jadpo check project=e-decline-not-declinable-fix-1 hidden
type Stock = Int {
    min: 0
}

function cap_total(total: Stock) -> Stock {
    return total
}
```

The line is gone. If the rule fires on this code, its own error says what
to change.

**If you misspelled the rule's name**, write it as its error's `rule:` line
gives it:

```jadpo fragment
decline library.similar_exists: "Caps the basket for the till, which changes on its own"
function cap_total(total: Stock) -> Stock {
    ...
}
```

```jadpo check project=e-decline-not-declinable-fix-2 hidden
type Stock = Int {
    min: 0
}

library rounding {
    /// The amount, capped at a thousand.
    function capped(amount: Stock) -> Stock {
        if (amount > Stock(1000)) {
            return Stock(1000)
        }
        return amount
    }
}

decline library.similar_exists: "Caps the basket for the till, which changes on its own"
function cap_total(total: Stock) -> Stock {
    if (total > Stock(1000)) {
        return Stock(1000)
    }
    return total
}
```

The line now declines the finding it was written for.

## Related

- [Writing a decline](../language/01-project-layout-and-modules.md#writing-a-decline)
- [`decline.unused`](decline.unused.md)
