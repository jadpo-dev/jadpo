# `data.write_before_call`

A write comes before a later outside call in the same action.

## Why Jadpo refuses it

No transaction is open while an outside call is out, because a slow
provider would hold a database connection and its locks for as long as it
took. So an action's writes come after its last outside call, and run as
written the call would be refused. An action that an action calls joins
its transaction, so the same goes for a call made through one.

## What it looks like

A bakery's action marks a supplier's price sheet checked, then keeps the
sheet again.

```jadpo check project=e-write-before-call file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=e-write-before-call file=domains/bakery/entities/price_sheet.jadpo hidden migrate=initial
/// A supplier's price sheet, kept as it was sent, and whether it was
/// checked.
entity PriceSheet {
    id: Uuid
    sheet: File {
        store: sheets
        max_size: 5mb
        types: [FileType.csv]
    }
    checked: Bool

    policy: {
        create: [Access.public]
        read: [Access.public]
        update: [Access.public]
    }
}
```

```jadpo check project=e-write-before-call file=domains/bakery/types/sheet_text.jadpo hidden
/// A price sheet as a supplier sends it.
type SheetText = Text {
    min_length: 1
    max_length: 100000
}
```

```jadpo check project=e-write-before-call file=domains/bakery/failures/sheet_not_kept.jadpo hidden
/// The sheets store had no room for a sheet.
type SheetNotKept = Failure {
    kind: FailureKind.unavailable
    message: "There was no room to keep the sheet."
}
```

domains/bakery/actions/recheck_prices.jadpo:

```jadpo check project=e-write-before-call file=domains/bakery/actions/recheck_prices.jadpo expect=data.write_before_call
use app { sheets }
use bakery/entities { PriceSheet }
use bakery/types { SheetText }
use bakery/failures { SheetNotKept }

/// Marks a supplier's price sheet checked, then keeps the sheet they sent
/// again: the write comes before the call.
action recheck_prices(
    price_sheet: PriceSheet,
    sheet_text: SheetText
) fails SheetNotKept -> PriceSheet {
    var checked: PriceSheet = PriceSheet.update(price_sheet) {
        set: {
            checked: true
        }
    }
    var sheet: File = attempt sheets.put(encoding.to_utf8(sheet_text), FileType.csv) {
        storage_full: SheetNotKept
    }
    return PriceSheet.update(checked) {
        set: {
            sheet: sheet
        }
    }
}
```

```text output
error: `PriceSheet.update` writes before the call to `sheets.put`
 --> domains/bakery/actions/recheck_prices.jadpo:12:31
   |
12 |     var checked: PriceSheet = PriceSheet.update(price_sheet) {
   |                               ^^^^^^^^^^^^^^^^^ writes here
17 |     var sheet: File = attempt sheets.put(encoding.to_utf8(sheet_text), FileType.csv) {
   |                               ---------- the call it must follow
  reason: No transaction is open while an outside call is out, so an action writes only after its last call; run as written, `sheets.put` would be refused.
  next: Move `PriceSheet.update` after `sheets.put` (agent)
  rule: data.write_before_call
  help: https://jadpo.dev/docs/errors/data.write_before_call

check failed: 1 error, 5 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A call made through another action is refused the same way, naming the
action and the call it makes.

```jadpo check project=e-write-before-call-through file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=e-write-before-call-through file=domains/bakery/entities/price_sheet.jadpo hidden migrate=initial
/// A supplier's price sheet, kept as it was sent, and whether it was
/// checked.
entity PriceSheet {
    id: Uuid
    sheet: File {
        store: sheets
        max_size: 5mb
        types: [FileType.csv]
    }
    checked: Bool

    policy: {
        create: [Access.public]
        read: [Access.public]
        update: [Access.public]
    }
}
```

```jadpo check project=e-write-before-call-through file=domains/bakery/types/sheet_text.jadpo hidden
/// A price sheet as a supplier sends it.
type SheetText = Text {
    min_length: 1
    max_length: 100000
}
```

```jadpo check project=e-write-before-call-through file=domains/bakery/failures/sheet_not_kept.jadpo hidden
/// The sheets store had no room for a sheet.
type SheetNotKept = Failure {
    kind: FailureKind.unavailable
    message: "There was no room to keep the sheet."
}
```

domains/bakery/actions/keep_sheet.jadpo:

```jadpo check project=e-write-before-call-through file=domains/bakery/actions/keep_sheet.jadpo
use app { sheets }
use bakery/types { SheetText }
use bakery/failures { SheetNotKept }

/// Keeps a sheet in the store.
action keep_sheet(sheet_text: SheetText) fails SheetNotKept -> File {
    return attempt sheets.put(encoding.to_utf8(sheet_text), FileType.csv) {
        storage_full: SheetNotKept
    }
}
```

domains/bakery/actions/recheck_prices.jadpo:

```jadpo check project=e-write-before-call-through file=domains/bakery/actions/recheck_prices.jadpo expect=data.write_before_call
use bakery/entities { PriceSheet }
use bakery/types { SheetText }
use bakery/failures { SheetNotKept }
use bakery/actions { keep_sheet }

/// Marks a sheet checked, then keeps it through `keep_sheet`.
action recheck_prices(
    price_sheet: PriceSheet,
    sheet_text: SheetText
) fails SheetNotKept -> PriceSheet {
    var checked: PriceSheet = PriceSheet.update(price_sheet) {
        set: {
            checked: true
        }
    }
    var sheet: File = attempt keep_sheet(sheet_text)
    return PriceSheet.update(checked) {
        set: {
            sheet: sheet
        }
    }
}
```

```text output
error: `PriceSheet.update` writes before `keep_sheet` calls `sheets.put`
 --> domains/bakery/actions/recheck_prices.jadpo:11:31
   |
11 |     var checked: PriceSheet = PriceSheet.update(price_sheet) {
   |                               ^^^^^^^^^^^^^^^^^ writes here
16 |     var sheet: File = attempt keep_sheet(sheet_text)
   |                               ---------- calls `sheets.put`
  reason: No transaction is open while an outside call is out, so an action writes only after its last call; run as written, `sheets.put` would be refused.
  next: Move `PriceSheet.update` after the call to `keep_sheet` (agent)
  rule: data.write_before_call
  help: https://jadpo.dev/docs/errors/data.write_before_call

check failed: 1 error, 6 files
```

## How to fix it

**If the write can wait for the call**, make the call first, then write
what it changed:

```jadpo fragment
    var sheet: File = attempt sheets.put(encoding.to_utf8(sheet_text), FileType.csv) {
        storage_full: SheetNotKept
    }
    return PriceSheet.update(price_sheet) {
        set: {
            sheet: sheet
            checked: true
        }
    }
```

```jadpo check project=e-write-before-call-fix file=domains/bakery/actions/recheck_prices.jadpo hidden
use app { sheets }
use bakery/entities { PriceSheet }
use bakery/types { SheetText }
use bakery/failures { SheetNotKept }

/// Keeps the sheet a supplier sent again, then marks it checked.
action recheck_prices(
    price_sheet: PriceSheet,
    sheet_text: SheetText
) fails SheetNotKept -> PriceSheet {
    var sheet: File = attempt sheets.put(encoding.to_utf8(sheet_text), FileType.csv) {
        storage_full: SheetNotKept
    }
    return PriceSheet.update(price_sheet) {
        set: {
            sheet: sheet
            checked: true
        }
    }
}
```

```jadpo check project=e-write-before-call-fix file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=e-write-before-call-fix file=domains/bakery/entities/price_sheet.jadpo hidden
/// A supplier's price sheet, kept as it was sent, and whether it was
/// checked.
entity PriceSheet {
    id: Uuid
    sheet: File {
        store: sheets
        max_size: 5mb
        types: [FileType.csv]
    }
    checked: Bool

    policy: {
        create: [Access.public]
        read: [Access.public]
        update: [Access.public]
    }
}
```

```jadpo check project=e-write-before-call-fix file=domains/bakery/types/sheet_text.jadpo hidden
/// A price sheet as a supplier sends it.
type SheetText = Text {
    min_length: 1
    max_length: 100000
}
```

```jadpo check project=e-write-before-call-fix file=domains/bakery/failures/sheet_not_kept.jadpo hidden migrate=initial
/// The sheets store had no room for a sheet.
type SheetNotKept = Failure {
    kind: FailureKind.unavailable
    message: "There was no room to keep the sheet."
}
```

The one write now happens after the call, in a transaction of its own.

## Related

- [A write before a call](../language/06-reads-and-writes.md#a-write-before-a-call)
