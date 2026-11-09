# `effect.call_in_loop`

An outside call is made for each item of a `for` loop, directly or through an action the loop calls.

## Why Jadpo refuses it

An outside call leaves your app: a provider's operation, a package's action,
a file store's `put` or `read`, or a `documents` render. An action's outside
calls lie along a way through it whose length is known before it runs, so a
retry can replay them in order. A loop makes one for each item, as many as
the data holds. `batches.each` makes them in the background instead, a slice
at a time, and keeps its place. A provider that takes a list is called once,
with the list.

## What it looks like

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

domains/bakery/actions/keep_sheets.jadpo:

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
  help: https://jadpo.dev/docs/errors/effect.call_in_loop

check failed: 1 error, 4 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An action the loop calls counts too, when it makes an outside call:

```jadpo check project=e-call-in-loop-through file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=e-call-in-loop-through file=domains/bakery/types/sheet_text.jadpo hidden
/// A price sheet as a supplier sends it.
type SheetText = Text {
    min_length: 1
    max_length: 100000
}
```

```jadpo check project=e-call-in-loop-through file=domains/bakery/failures/sheet_not_kept.jadpo hidden
/// The sheets store had no room for a sheet.
type SheetNotKept = Failure {
    kind: FailureKind.unavailable
    message: "There was no room to keep the sheet."
}
```

```jadpo check project=e-call-in-loop-through file=domains/bakery/actions/keep_sheet.jadpo hidden
use app { sheets }
use bakery/types { SheetText }
use bakery/failures { SheetNotKept }

/// Keeps one price sheet a supplier sent.
action keep_sheet(sheet_text: SheetText) fails SheetNotKept -> File {
    return attempt sheets.put(encoding.to_utf8(sheet_text), FileType.csv) {
        storage_full: SheetNotKept
    }
}
```

domains/bakery/actions/keep_sheets.jadpo:

```jadpo check project=e-call-in-loop-through file=domains/bakery/actions/keep_sheets.jadpo expect=effect.call_in_loop
use bakery/types { SheetText }
use bakery/failures { SheetNotKept }
use bakery/actions { keep_sheet }

/// Keeps each price sheet a supplier sent, one call for each.
action keep_sheets(sheet_texts: List<SheetText>) fails SheetNotKept -> List<File> {
    var mut kept: List<File> = []
    for sheet_text in sheet_texts {
        var sheet: File = attempt keep_sheet(sheet_text)
        kept = list.append(kept, sheet)
    }
    return kept
}
```

```text output
error: `keep_sheet` calls `sheets.put` once for every item of the loop
 --> domains/bakery/actions/keep_sheets.jadpo:9:35
  |
9 |         var sheet: File = attempt keep_sheet(sheet_text)
  |                                   ^^^^^^^^^^ inside `for`
  reason: An action's outside calls lie along a way through it whose length is known before it runs, so a retry can replay them in order; a loop makes one for each item. A provider that takes a list is called once with the list.
  next: Make the calls one item at a time with `batches.each` (agent)
  rule: effect.call_in_loop
  help: https://jadpo.dev/docs/errors/effect.call_in_loop

check failed: 1 error, 5 files
```

## How to fix it

**If each item needs its own call**, keep the items as records, move the call
into an action of its own, and run it with `batches.each`, which walks what a
query finds:

```jadpo fragment
action keep_sheets(page: SheetPage) -> BatchRun {
    return batches.each(from: all_sheets(page), each: sheet -> keep_sheet(sheet))
}
```

```jadpo check project=e-call-in-loop-fix file=domains/bakery/actions/keep_sheets.jadpo hidden
use bakery/types { SheetPage }
use bakery/actions { keep_sheet }
use bakery/queries { all_sheets }

/// Keeps every waiting sheet, in the background.
action keep_sheets(page: SheetPage) -> BatchRun {
    return batches.each(from: all_sheets(page), each: sheet -> keep_sheet(sheet))
}
```

```jadpo check project=e-call-in-loop-fix file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=e-call-in-loop-fix file=domains/bakery/types/sheet_text.jadpo hidden
/// A price sheet as a supplier sends it.
type SheetText = Text {
    min_length: 1
    max_length: 100000
}
```

```jadpo check project=e-call-in-loop-fix file=domains/bakery/types/sheet_page.jadpo hidden
/// A page of sheets waiting to be kept.
type SheetPage = Input {
    page_size: PageSize {
        default: 25
    }
    after: PageCursor {
        optional: true
    }
}
```

```jadpo check project=e-call-in-loop-fix file=domains/bakery/failures/sheet_not_kept.jadpo hidden
/// The sheets store had no room for a sheet.
type SheetNotKept = Failure {
    kind: FailureKind.unavailable
    message: "There was no room to keep the sheet."
}
```

```jadpo check project=e-call-in-loop-fix file=domains/bakery/queries/all_sheets.jadpo hidden
use bakery/entities { Sheet }
use bakery/types { SheetPage }

/// Every sheet waiting to be kept, a page at a time.
query all_sheets(page: SheetPage) -> Page<Sheet> {
    from: Sheet
    order: [Sheet.id asc]
    page: {
        size: page.page_size
        after: page.after
    }
}
```

```jadpo check project=e-call-in-loop-fix file=domains/bakery/actions/keep_sheet.jadpo hidden
use app { sheets }
use bakery/entities { Sheet }
use bakery/failures { SheetNotKept }

/// Keeps one price sheet.
action keep_sheet(sheet: Sheet) fails SheetNotKept -> File {
    return attempt sheets.put(encoding.to_utf8(sheet.text), FileType.csv) {
        storage_full: SheetNotKept
    }
}
```

```jadpo check project=e-call-in-loop-fix file=domains/bakery/entities/sheet.jadpo hidden migrate=initial
use bakery/types { SheetText }

/// A price sheet waiting to be kept.
entity Sheet {
    id: Uuid
    text: SheetText

    policy: {
        read: [Access.public, Access.system]
    }
}
```

The calls now run in the background, a slice at a time. A crash or a deploy
carries on where it stopped, and nothing is done twice.

## Related

- [No outside calls inside a loop](../language/03-logic.md#no-outside-calls-inside-a-loop)
- [Batches](../language/12-jobs-and-events.md#batches)
- [Passing a failure on: `attempt`](../language/04-failures.md#passing-a-failure-on-attempt)
