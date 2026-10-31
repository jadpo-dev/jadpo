# `step.late_failure`

An action can fail for good after a call that changes something outside the app took effect, leaving that change behind.

## Why Jadpo refuses it

A changing call, such as a store's `put` or a provider operation with
`changes: true`, takes effect as soon as it is made. When the action
then fails for good, Jadpo rolls back the action's own writes, but it
can't take back what the call did. A retry fails the same way, so the
effect stays with nothing pointing at it.

A failure that ends an action for good is any point after the first
changing call that passes on a failure whose kind isn't `unavailable` or
`rate_limited`: an `attempt` whose call or write can fail that way, a
`reject`, or a `propagate`. Those two kinds are retried instead, and a
failure handled with `match` doesn't end the action.

## What it looks like

domains/bakery/actions/keep_price_sheet.jadpo, in a bakery app that keeps
price sheets in `sheets`, a folder of files:

```jadpo check project=e-step-late-failure file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=e-step-late-failure file=domains/bakery/types/price_list.jadpo hidden
/// A supplier's prices, one line for each loaf.
type PriceList = Text {
    min_length: 1
    max_length: 2000
}
```

```jadpo check project=e-step-late-failure file=domains/bakery/types/shop_state.jadpo hidden
/// Whether a shop is trading this week.
type ShopState = Enum {
    open
    closed
}
```

```jadpo check project=e-step-late-failure file=domains/bakery/failures/sheet_not_kept.jadpo hidden
/// The sheets store had no room for a sheet.
type SheetNotKept = Failure {
    kind: FailureKind.unavailable
    message: "There was no room to keep the sheet."
}
```

```jadpo check project=e-step-late-failure file=domains/bakery/failures/shop_closed.jadpo hidden
/// The shop isn't trading this week.
type ShopClosed = Failure {
    kind: FailureKind.rejected
    message: "The shop is closed this week."
}
```

```jadpo check project=e-step-late-failure file=domains/bakery/actions/keep_price_sheet.jadpo expect=step.late_failure
use app { sheets }
use bakery/types { PriceList, ShopState }
use bakery/failures { SheetNotKept, ShopClosed }

/// Keeps the week's price sheet for a shop that is trading.
action keep_price_sheet(
    price_list: PriceList,
    state: ShopState
) fails SheetNotKept, ShopClosed -> File {
    var sheet: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
    if (state == ShopState.closed) {
        reject ShopClosed
    }
    return sheet
}
```

```text output
error: `keep_price_sheet` can fail for good at `reject ShopClosed`, after `sheets.put` took effect
 --> domains/bakery/actions/keep_price_sheet.jadpo:14:9
   |
10 |     var sheet: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
   |                               ---------- takes effect first
14 |         reject ShopClosed
   |         ^^^^^^^^^^^^^^^^^ can fail with `ShopClosed`
  reason: Once a changing call has taken effect, a failure that ends `keep_price_sheet` for good rolls back its writes but not what `sheets.put` did, and a retry fails the same way; a failure of kind `unavailable` or `rate_limited` is retried instead, and one handled with `match` doesn't end it.
  next: Check the condition before `sheets.put`, record what happened and finish in a subscriber of an event `keep_price_sheet` emits, or undo `sheets.put` as another step in a `match` arm, or write `decline step.late_failure: "<why>"` above it if this is deliberate (agent)
  rule: step.late_failure
  help: https://jadpo.dev/docs/errors/step.late_failure

check failed: 1 error, 6 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the condition can be checked before the call**, move the check
first. In `domains/bakery/actions/keep_price_sheet.jadpo`:

```jadpo fragment
    if (state == ShopState.closed) {
        reject ShopClosed
    }
    var sheet: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
```

```jadpo check project=e-step-late-failure-fix-1 file=domains/bakery/actions/keep_price_sheet.jadpo hidden
use app { sheets }
use bakery/types { PriceList, ShopState }
use bakery/failures { SheetNotKept, ShopClosed }

/// Keeps the week's price sheet for a shop that is trading.
action keep_price_sheet(
    price_list: PriceList,
    state: ShopState
) fails SheetNotKept, ShopClosed -> File {
    if (state == ShopState.closed) {
        reject ShopClosed
    }
    var sheet: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
    return sheet
}
```

```jadpo check project=e-step-late-failure-fix-1 file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=e-step-late-failure-fix-1 file=domains/bakery/types/price_list.jadpo hidden
/// A supplier's prices, one line for each loaf.
type PriceList = Text {
    min_length: 1
    max_length: 2000
}
```

```jadpo check project=e-step-late-failure-fix-1 file=domains/bakery/types/shop_state.jadpo hidden
/// Whether a shop is trading this week.
type ShopState = Enum {
    open
    closed
}
```

```jadpo check project=e-step-late-failure-fix-1 file=domains/bakery/failures/sheet_not_kept.jadpo hidden
/// The sheets store had no room for a sheet.
type SheetNotKept = Failure {
    kind: FailureKind.unavailable
    message: "There was no room to keep the sheet."
}
```

```jadpo check project=e-step-late-failure-fix-1 file=domains/bakery/failures/shop_closed.jadpo hidden
/// The shop isn't trading this week.
type ShopClosed = Failure {
    kind: FailureKind.rejected
    message: "The shop is closed this week."
}
```

A closed shop is now refused before anything is kept, so a refusal
leaves no sheet behind. A failure of `sheets.put` itself is the last
point, and nothing has taken effect before it.

**If what is left behind does no harm**, decline the finding above the
action, with the reason:

```jadpo fragment
/// Keeps the week's price sheet for a shop that is trading.
decline step.late_failure: "A sheet kept for a closed shop does no harm, and nothing reads it"
action keep_price_sheet(
```

```jadpo check project=e-step-late-failure-fix-2 file=domains/bakery/actions/keep_price_sheet.jadpo hidden
use app { sheets }
use bakery/types { PriceList, ShopState }
use bakery/failures { SheetNotKept, ShopClosed }

/// Keeps the week's price sheet for a shop that is trading.
decline step.late_failure: "A sheet kept for a closed shop does no harm, and nothing reads it"
action keep_price_sheet(
    price_list: PriceList,
    state: ShopState
) fails SheetNotKept, ShopClosed -> File {
    var sheet: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
    if (state == ShopState.closed) {
        reject ShopClosed
    }
    return sheet
}
```

```jadpo check project=e-step-late-failure-fix-2 file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=e-step-late-failure-fix-2 file=domains/bakery/types/price_list.jadpo hidden
/// A supplier's prices, one line for each loaf.
type PriceList = Text {
    min_length: 1
    max_length: 2000
}
```

```jadpo check project=e-step-late-failure-fix-2 file=domains/bakery/types/shop_state.jadpo hidden
/// Whether a shop is trading this week.
type ShopState = Enum {
    open
    closed
}
```

```jadpo check project=e-step-late-failure-fix-2 file=domains/bakery/failures/sheet_not_kept.jadpo hidden
/// The sheets store had no room for a sheet.
type SheetNotKept = Failure {
    kind: FailureKind.unavailable
    message: "There was no room to keep the sheet."
}
```

```jadpo check project=e-step-late-failure-fix-2 file=domains/bakery/failures/shop_closed.jadpo hidden
/// The shop isn't trading this week.
type ShopClosed = Failure {
    kind: FailureKind.rejected
    message: "The shop is closed this week."
}
```

The action is unchanged, and the reason stays on it for whoever reads it
next. One decline answers every such point in the action.

When the effect must not be left behind and the condition can only be
known afterwards, the next step names two more fixes: record what
happened and finish in a subscriber of an event the action emits, or
undo the call as another changing call in a `match` arm.

## Related

- [The late-failure warning](../language/12-jobs-and-events.md#the-late-failure-warning)
- [Ending for good after a step took effect](../language/12-jobs-and-events.md#ending-for-good-after-a-step-took-effect)
- [Retries and giving up](../language/12-jobs-and-events.md#retries-and-giving-up)
- [Declining a judgement call](../language/01-project-layout-and-modules.md#declining-a-judgement-call)
