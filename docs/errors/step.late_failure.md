# `step.late_failure`

After an action's first changing call has taken effect, a failure that
ends it for good rolls back its writes but not what the call did, and a
retry fails the same way. Such a point (an `attempt` whose call or write
can fail with a failure that isn't retried, `reject`, `propagate`) is an
error until it is moved before the changing call, handled with `match`,
or declined. Here the shop could be checked before the sheet is kept:

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
  help: docs/language/12-jobs-and-events.html#step-late-failure

check failed: 1 error, 6 files
```
