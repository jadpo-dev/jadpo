# `step.too_many`

An action makes at most 8 changing calls on one way through it, counting
those of the actions it calls:

```jadpo check project=e-step-too-many file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
    steps: {
        encryption_key: config.step_encryption_key
    }
}
```

```jadpo check project=e-step-too-many file=config.jadpo hidden
config {
    step_encryption_key: SigningKey
}
```

```jadpo check project=e-step-too-many file=domains/bakery/types/price_list.jadpo hidden
/// A supplier's prices, one line for each loaf.
type PriceList = Text {
    min_length: 1
    max_length: 2000
}
```

```jadpo check project=e-step-too-many file=domains/bakery/failures/sheet_not_kept.jadpo hidden
/// The sheets store had no room for a sheet.
type SheetNotKept = Failure {
    kind: FailureKind.unavailable
    message: "There was no room to keep the sheet."
}
```

```jadpo check project=e-step-too-many file=domains/bakery/actions/share_prices.jadpo expect=step.too_many
use app { sheets }
use bakery/types { PriceList }
use bakery/failures { SheetNotKept }

/// Keeps the week's price sheet for each of the bakery's nine shops.
action share_prices(price_list: PriceList) fails SheetNotKept -> List<File> {
    var bakehouse: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
    var market: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
    var station: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
    var harbour: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
    var castle: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
    var bridge: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
    var mill: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
    var abbey: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
    var quayside: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
    return [bakehouse, market, station, harbour, castle, bridge, mill, abbey, quayside]
}
```

```text output
error: `share_prices` makes 9 changing calls in a row, more than 8
 --> domains/bakery/actions/share_prices.jadpo:31:34
   |
31 |     var quayside: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
   |                                  ^^^^^^^^^^ the 9th changing call
  reason: Each changing call is a step its action records before sending and replays on a retry, and an action makes at most 8 on any one way through it, counting those of the actions it calls.
  next: Finish part of the work in a subscriber of an event `share_prices` emits (agent)
  rule: step.too_many
  help: docs/errors/step.too_many.md

check failed: 1 error, 5 files
```
