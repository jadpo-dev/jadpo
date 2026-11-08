# `type.batch_row_invalid`

A file helper's rows can't be read as the `Input` its `as:` names or written as the `Output` its `each:` gives, or `json.read`'s `items_at:` isn't member names written out between dots, or a reader's `max_unpacked_size:` isn't a size.

## Why Jadpo refuses it

A file helper reads each row of a file as the `Input` its `as:` names,
and writes each as the `Output` its `each:` gives, by the same rules a
route reads its input and writes its output. A CSV or TSV row holds one
value a cell, so a row type with a field holding a list, a map, an
object, or a standard type with no one text form has no cell to read
from or write to. Two fields reading or writing one column would
disagree about what is in it. Each of these is caught while compiling,
rather than turning every row of a file into a problem once the run
starts. JSON Lines rows hold any JSON value, so a row with a list is
read from a JSON Lines file.

The same rule holds `json.read`'s `items_at:` to member names written
out between dots, such as `"data.items"`, and a reader's
`max_unpacked_size:` to a size, such as `200mb`, so where the list sits
and how far a packed file may unpack are known before the run starts.

## What it looks like

An order row with a list of lines, `domains/bakery/types/order_row.jadpo`,
read from a CSV file by `domains/bakery/actions/read_orders.jadpo`, in an
app whose `OrderSheet` keeps an uploaded sheet as a `File` in its
`sheets` store:

```jadpo check project=e-batch-row-invalid file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=e-batch-row-invalid file=domains/bakery/types/order_code.jadpo hidden
/// A line's code on an order.
type OrderCode = Text {
    min_length: 1
    max_length: 20
}
```

```jadpo check project=e-batch-row-invalid file=domains/bakery/types/code_row.jadpo hidden
use bakery/types { OrderCode }

/// One code a sheet lists.
type CodeRow = Input {
    reference: OrderCode
}
```

```jadpo check project=e-batch-row-invalid file=domains/bakery/entities/order_sheet.jadpo hidden
/// A sheet of order_sheet, kept as it was sent.
entity OrderSheet {
    id: Uuid
    sheet: File {
        store: sheets
        max_size: 5mb
    }

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
    }
}
```

```jadpo check project=e-batch-row-invalid file=domains/bakery/actions/take_code.jadpo hidden
use bakery/types { CodeRow }

/// Takes one code a sheet lists.
action take_code(listed_code: CodeRow) -> Unit {
    assert listed_code.reference == listed_code.reference
}
```

```jadpo check project=e-batch-row-invalid file=domains/bakery/actions/read_codes.jadpo hidden migrate=initial
use bakery/entities { OrderSheet, OrderSheetNotFound }
use bakery/types { CodeRow }
use bakery/actions { take_code }

/// Starts taking the codes a kept sheet lists.
action read_codes(sheet_id: OrderSheet.id) fails OrderSheetNotFound -> BatchRun {
    var code_sheet: OrderSheet = attempt OrderSheet.get(sheet_id)
    return csv.read(code_sheet.sheet, as: CodeRow, each: listed_code -> take_code(listed_code))
}
```

```jadpo check project=e-batch-row-invalid file=domains/bakery/types/order_row.jadpo
use bakery/types { OrderCode }

/// One order a sheet lists, with its lines.
type OrderRow = Input {
    reference: OrderCode
    lines: List<OrderCode>
}
```

```jadpo check project=e-batch-row-invalid file=domains/bakery/actions/take_order.jadpo hidden
use bakery/types { OrderRow }

/// Takes one order a sheet lists.
action take_order(order_entry: OrderRow) -> Unit {
    assert order_entry.reference == order_entry.reference
}
```

```jadpo check project=e-batch-row-invalid file=domains/bakery/actions/read_orders.jadpo expect=type.batch_row_invalid
use bakery/entities { OrderSheet, OrderSheetNotFound }
use bakery/types { OrderRow }
use bakery/actions { take_order }

/// Starts taking the orders a kept sheet lists.
action read_orders(sheet_id: OrderSheet.id) fails OrderSheetNotFound -> BatchRun {
    var order_sheet: OrderSheet = attempt OrderSheet.get(sheet_id)
    return csv.read(order_sheet.sheet, as: OrderRow, each: order_entry -> take_order(order_entry))
}
```

```text output
error: `csv.read` can't read its rows as `OrderRow`
 --> domains/bakery/actions/read_orders.jadpo:8:44
  |
8 |     return csv.read(order_sheet.sheet, as: OrderRow, each: order_entry -> take_order(order_entry))
  |                                            ^^^^^^^^ `OrderRow.lines` holds a list
  reason: A CSV row holds one value a cell, and `OrderRow.lines` holds a list; a JSON Lines row holds lists and objects, each line one JSON value.
  next: Read the file with `json_lines.read`, or give `lines` a type one cell holds (agent)
  rule: type.batch_row_invalid
  help: https://jadpo.dev/docs/errors/type.batch_row_invalid

check failed: 1 error, 9 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is what the file holds.

**If each row holds a list**, read a JSON Lines file, whose lines each
hold one JSON value, lists and objects included. In
`domains/bakery/actions/read_orders.jadpo`:

```jadpo fragment
    return json_lines.read(
        order_sheet.sheet,
        as: OrderRow,
        each: order_entry -> take_order(order_entry)
    )
```

```jadpo check project=e-batch-row-invalid-fix-1 file=domains/bakery/actions/read_orders.jadpo hidden
use bakery/entities { OrderSheet, OrderSheetNotFound }
use bakery/types { OrderRow }
use bakery/actions { take_order }

/// Starts taking the orders a kept sheet lists.
action read_orders(sheet_id: OrderSheet.id) fails OrderSheetNotFound -> BatchRun {
    var order_sheet: OrderSheet = attempt OrderSheet.get(sheet_id)
    return json_lines.read(
        order_sheet.sheet,
        as: OrderRow,
        each: order_entry -> take_order(order_entry)
    )
}
```

```jadpo check project=e-batch-row-invalid-fix-1 file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=e-batch-row-invalid-fix-1 file=domains/bakery/types/order_code.jadpo hidden
/// A line's code on an order.
type OrderCode = Text {
    min_length: 1
    max_length: 20
}
```

```jadpo check project=e-batch-row-invalid-fix-1 file=domains/bakery/types/code_row.jadpo hidden
use bakery/types { OrderCode }

/// One code a sheet lists.
type CodeRow = Input {
    reference: OrderCode
}
```

```jadpo check project=e-batch-row-invalid-fix-1 file=domains/bakery/entities/order_sheet.jadpo hidden
/// A sheet of order_sheet, kept as it was sent.
entity OrderSheet {
    id: Uuid
    sheet: File {
        store: sheets
        max_size: 5mb
    }

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
    }
}
```

```jadpo check project=e-batch-row-invalid-fix-1 file=domains/bakery/actions/take_code.jadpo hidden
use bakery/types { CodeRow }

/// Takes one code a sheet lists.
action take_code(listed_code: CodeRow) -> Unit {
    assert listed_code.reference == listed_code.reference
}
```

```jadpo check project=e-batch-row-invalid-fix-1 file=domains/bakery/actions/read_codes.jadpo hidden
use bakery/entities { OrderSheet, OrderSheetNotFound }
use bakery/types { CodeRow }
use bakery/actions { take_code }

/// Starts taking the codes a kept sheet lists.
action read_codes(sheet_id: OrderSheet.id) fails OrderSheetNotFound -> BatchRun {
    var code_sheet: OrderSheet = attempt OrderSheet.get(sheet_id)
    return csv.read(code_sheet.sheet, as: CodeRow, each: listed_code -> take_code(listed_code))
}
```

```jadpo check project=e-batch-row-invalid-fix-1 file=domains/bakery/types/order_row.jadpo hidden
use bakery/types { OrderCode }

/// One order a sheet lists, with its lines.
type OrderRow = Input {
    reference: OrderCode
    lines: List<OrderCode>
}
```

```jadpo check project=e-batch-row-invalid-fix-1 file=domains/bakery/actions/take_order.jadpo hidden migrate=initial
use bakery/types { OrderRow }

/// Takes one order a sheet lists.
action take_order(order_entry: OrderRow) -> Unit {
    assert order_entry.reference == order_entry.reference
}
```

The sheet is now read a line at a time, each line one order with its
`lines` as a JSON list. A line that isn't JSON, or doesn't fit
`OrderRow`, becomes a problem in the run's summary, and the rest carry
on.

**If the file stays a CSV**, give each row a type one cell holds: here
one line of an order a row, so an order with three lines is three rows
with the same `reference`. In `domains/bakery/types/order_row.jadpo`:

```jadpo fragment
type OrderRow = Input {
    reference: OrderCode
    line: OrderCode
}
```

```jadpo check project=e-batch-row-invalid-fix-2 file=domains/bakery/types/order_row.jadpo hidden
use bakery/types { OrderCode }

/// One line of an order a sheet lists.
type OrderRow = Input {
    reference: OrderCode
    line: OrderCode
}
```

```jadpo check project=e-batch-row-invalid-fix-2 file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=e-batch-row-invalid-fix-2 file=domains/bakery/types/order_code.jadpo hidden
/// A line's code on an order.
type OrderCode = Text {
    min_length: 1
    max_length: 20
}
```

```jadpo check project=e-batch-row-invalid-fix-2 file=domains/bakery/types/code_row.jadpo hidden
use bakery/types { OrderCode }

/// One code a sheet lists.
type CodeRow = Input {
    reference: OrderCode
}
```

```jadpo check project=e-batch-row-invalid-fix-2 file=domains/bakery/entities/order_sheet.jadpo hidden
/// A sheet of order_sheet, kept as it was sent.
entity OrderSheet {
    id: Uuid
    sheet: File {
        store: sheets
        max_size: 5mb
    }

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
    }
}
```

```jadpo check project=e-batch-row-invalid-fix-2 file=domains/bakery/actions/take_code.jadpo hidden
use bakery/types { CodeRow }

/// Takes one code a sheet lists.
action take_code(listed_code: CodeRow) -> Unit {
    assert listed_code.reference == listed_code.reference
}
```

```jadpo check project=e-batch-row-invalid-fix-2 file=domains/bakery/actions/read_codes.jadpo hidden
use bakery/entities { OrderSheet, OrderSheetNotFound }
use bakery/types { CodeRow }
use bakery/actions { take_code }

/// Starts taking the codes a kept sheet lists.
action read_codes(sheet_id: OrderSheet.id) fails OrderSheetNotFound -> BatchRun {
    var code_sheet: OrderSheet = attempt OrderSheet.get(sheet_id)
    return csv.read(code_sheet.sheet, as: CodeRow, each: listed_code -> take_code(listed_code))
}
```

```jadpo check project=e-batch-row-invalid-fix-2 file=domains/bakery/actions/take_order.jadpo hidden
use bakery/types { OrderRow }

/// Takes one order a sheet lists.
action take_order(order_entry: OrderRow) -> Unit {
    assert order_entry.reference == order_entry.reference
}
```

```jadpo check project=e-batch-row-invalid-fix-2 file=domains/bakery/actions/read_orders.jadpo hidden migrate=initial
use bakery/entities { OrderSheet, OrderSheetNotFound }
use bakery/types { OrderRow }
use bakery/actions { take_order }

/// Starts taking the orders a kept sheet lists.
action read_orders(sheet_id: OrderSheet.id) fails OrderSheetNotFound -> BatchRun {
    var order_sheet: OrderSheet = attempt OrderSheet.get(sheet_id)
    return csv.read(order_sheet.sheet, as: OrderRow, each: order_entry -> take_order(order_entry))
}
```

Each cell now holds one code, under the column named for its field,
`reference` or `line`.

## Related

- [Rows are typed both ways](../language/12-jobs-and-events.md#rows-are-typed-both-ways)
- [Batches over files](../language/12-jobs-and-events.md#batches-over-files)
- [Data formats: JSON, CSV, TSV and JSON Lines](../language/19-standard-library.md#data-formats-json-csv-tsv-and-json-lines)
