# `type.batch_row_invalid`

A file helper reads each row as the `Input` its `as:` names, and writes
each as the `Output` its `each:` gives. A CSV or TSV row
holds one value a cell, so a row type holding a list, a map, an object
or a standard type with no one text form is refused, pointing at JSON
Lines, whose rows hold any JSON value; so is a row type that isn't an
`Input` (or, written, an `Output`), or two fields reading or writing one
column. The same rule refuses `json.read`'s `items_at:`
when it isn't member names written out between dots (`"data.items"`),
and a reader's `max_unpacked_size:` when it isn't a size (`200mb`):

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
  help: docs/language/12-jobs-and-events.html#type-batch-row-invalid

check failed: 1 error, 9 files
```
