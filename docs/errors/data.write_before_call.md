# `data.write_before_call`

No transaction is open while an outside call is out, so an action's
writes come after its last call ("Several outside calls"). The error is
at the write, naming the last call it comes before; one made through an
action it calls names that action and the call it makes:

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
