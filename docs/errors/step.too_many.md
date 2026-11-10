# `step.too_many`

An action makes more than 8 changing calls on one way through it.

## Why Jadpo refuses it

Each changing call, a provider operation with `changes: true` or a
store's `put`, is a step: the runtime records it before sending it and
keeps its answer, so a retry replays the steps already answered instead
of sending them again. An action may make at most 8 on any one way
through it, counting those of the actions it calls. Past that, a retry
has more to replay and more that can be left in doubt than one action
should carry, so Jadpo asks for the rest of the work to be a follow-on
of its own, which is retried on its own.

## What it looks like

domains/bakery/actions/share_prices.jadpo:

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
  help: https://jadpo.dev/docs/errors/step.too_many

check failed: 1 error, 5 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether each shop needs a sheet of its own.

**If the shops can share one sheet**, keep it once. In
`domains/bakery/actions/share_prices.jadpo`:

```jadpo fragment
action share_prices(price_list: PriceList) fails SheetNotKept -> File {
    return attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
}
```

```jadpo check project=e-step-too-many-fix-1 file=domains/bakery/actions/share_prices.jadpo hidden
use app { sheets }
use bakery/types { PriceList }
use bakery/failures { SheetNotKept }

/// Keeps the week's price sheet, which all nine of the bakery's shops read.
action share_prices(price_list: PriceList) fails SheetNotKept -> File {
    return attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
}
```

```jadpo check project=e-step-too-many-fix-1 file=app.jadpo hidden
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

```jadpo check project=e-step-too-many-fix-1 file=config.jadpo hidden
config {
    step_encryption_key: SigningKey
}
```

```jadpo check project=e-step-too-many-fix-1 file=domains/bakery/types/price_list.jadpo hidden
/// A supplier's prices, one line for each loaf.
type PriceList = Text {
    min_length: 1
    max_length: 2000
}
```

```jadpo check project=e-step-too-many-fix-1 file=domains/bakery/failures/sheet_not_kept.jadpo hidden
/// The sheets store had no room for a sheet.
type SheetNotKept = Failure {
    kind: FailureKind.unavailable
    message: "There was no room to keep the sheet."
}
```

`share_prices` now makes one changing call, and every shop's page links
the same `File`.

**If each shop needs its own sheet**, keep eight in `share_prices` and
emit an event, and leave the ninth to a subscriber of it, which is
delivered once `share_prices` has committed and retried on its own. In
`domains/bakery/actions/share_prices.jadpo`:

```jadpo fragment
action share_prices(price_list: PriceList) fails SheetNotKept -> List<File> {
    ...
    emit PricesShared {
        price_list: price_list
    }
    return [bakehouse, market, station, harbour, castle, bridge, mill, abbey]
}
```

```jadpo check project=e-step-too-many-fix-2 file=domains/bakery/actions/share_prices.jadpo hidden
use app { sheets }
use bakery/types { PriceList }
use bakery/failures { SheetNotKept }
use bakery/events { PricesShared }

/// Keeps the week's price sheet for eight of the bakery's nine shops.
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
    emit PricesShared {
        price_list: price_list
    }
    return [bakehouse, market, station, harbour, castle, bridge, mill, abbey]
}
```

`domains/bakery/events/prices_shared.jadpo`:

```jadpo check project=e-step-too-many-fix-2 file=domains/bakery/events/prices_shared.jadpo
use bakery/types { PriceList }

/// The week's prices were kept for the first eight shops.
type PricesShared = Event {
    price_list: PriceList
}
```

`domains/bakery/subscribers/share_with_quayside.jadpo`:

```jadpo check project=e-step-too-many-fix-2 file=domains/bakery/subscribers/share_with_quayside.jadpo
use bakery/actions { share_quayside_prices }
use bakery/events { PricesShared }

/// Keeps the quayside shop's sheet once the others are kept.
subscriber share_with_quayside {
    on: PricesShared
    run: share_quayside_prices(event)
}
```

`domains/bakery/actions/share_quayside_prices.jadpo`:

```jadpo check project=e-step-too-many-fix-2 file=domains/bakery/actions/share_quayside_prices.jadpo
use app { sheets }
use bakery/failures { SheetNotKept }
use bakery/events { PricesShared }

/// Keeps the week's price sheet for the quayside shop.
action share_quayside_prices(shared: PricesShared) fails SheetNotKept -> File {
    return attempt sheets.put(encoding.to_utf8(shared.price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
}
```

An event waits in a database's outbox, so `app.jadpo` names one for the
events of an action that writes none:

```jadpo fragment
    connections: {
        main: {
            driver: Driver.postgres
            url: config.main_url
        }
        ...
    }
    events: {
        store: main
    }
```

```jadpo check project=e-step-too-many-fix-2 file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.main_url
        }
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
    events: {
        store: main
    }
    steps: {
        encryption_key: config.step_encryption_key
    }
}
```

```jadpo check project=e-step-too-many-fix-2 file=config.jadpo hidden
config {
    main_url: PostgresUrl
    step_encryption_key: SigningKey
}
```

```jadpo check project=e-step-too-many-fix-2 file=domains/bakery/types/price_list.jadpo hidden
/// A supplier's prices, one line for each loaf.
type PriceList = Text {
    min_length: 1
    max_length: 2000
}
```

```jadpo check project=e-step-too-many-fix-2 file=domains/bakery/failures/sheet_not_kept.jadpo hidden
/// The sheets store had no room for a sheet.
type SheetNotKept = Failure {
    kind: FailureKind.unavailable
    message: "There was no room to keep the sheet."
}
```

The eight sheets are kept, and the event saved, by `share_prices`; the
quayside sheet follows in its own delivery, and a failure there is
retried without keeping the other eight again.

## Related

- [What is allowed](../language/12-jobs-and-events.md#what-is-allowed)
- [Several changing calls in one action](../language/12-jobs-and-events.md#several-changing-calls-in-one-action)
- [Subscribers](../language/12-jobs-and-events.md#subscribers)
