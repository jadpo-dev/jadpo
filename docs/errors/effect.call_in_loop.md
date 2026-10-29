# `effect.call_in_loop`

An action's outside calls lie along a way through it whose length is known
before it runs, so a loop of them is `batches.each`:

```jadpo check project=e-call-in-loop file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=e-call-in-loop file=domains/bakery/types/sheet_text.jadpo hidden
/// A price sheet as a supplier sends it.
type SheetText = Text {
    min_length: 1
    max_length: 100000
}
```

```jadpo check project=e-call-in-loop file=domains/bakery/failures/sheet_not_kept.jadpo hidden
/// The sheets store had no room for a sheet.
type SheetNotKept = Failure {
    kind: FailureKind.unavailable
    message: "There was no room to keep the sheet."
}
```

```jadpo check project=e-call-in-loop file=domains/bakery/actions/keep_sheets.jadpo expect=effect.call_in_loop
use app { sheets }
use bakery/types { SheetText }
use bakery/failures { SheetNotKept }

/// Keeps each price sheet a supplier sent, one call for each.
action keep_sheets(sheet_texts: List<SheetText>) fails SheetNotKept -> List<File> {
    var mut kept: List<File> = []
    for sheet_text in sheet_texts {
        var sheet: File = attempt sheets.put(encoding.to_utf8(sheet_text), FileType.csv) {
            storage_full: SheetNotKept
        }
        kept = list.append(kept, sheet)
    }
    return kept
}
```

```text output
error: `sheets.put` is called once for every item of the loop
 --> domains/bakery/actions/keep_sheets.jadpo:9:35
  |
9 |         var sheet: File = attempt sheets.put(encoding.to_utf8(sheet_text), FileType.csv) {
  |                                   ^^^^^^^^^^ inside `for`
  reason: An action's outside calls lie along a way through it whose length is known before it runs, so a retry can replay them in order; a loop makes one for each item. A provider that takes a list is called once with the list.
  next: Make the calls one item at a time with `batches.each` (agent)
  rule: effect.call_in_loop
  help: docs/errors/effect.call_in_loop.md

check failed: 1 error, 4 files
```
