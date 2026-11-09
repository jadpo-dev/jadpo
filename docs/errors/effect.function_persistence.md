# `effect.function_persistence`

A function, or other code that only works out a value, reads or writes stored data, or calls a store of files, a model or a provider.

## Why Jadpo refuses it

A function only works out a value from its arguments, so the same arguments
always give the same result and a test needs no database. Reading or changing
an entity reaches outside its arguments. So does a store of files, a model or
a provider's operation, and only an action makes those calls. The same goes
for an inline function, a constant, a type's rule, a backfill and a template.

## What it looks like

main.jadpo:

```jadpo check project=e-fn-persist expect=effect.function_persistence
entity Customer {
    id: Uuid
}

function find_customer(id: Customer.id) -> Customer {
    return attempt Customer.get(id)
}
```

```text output
error: A function can't call `Customer.get`
 --> main.jadpo:6:20
  |
6 |     return attempt Customer.get(id)
  |                    ^^^^^^^^^^^^ reads or writes stored data
  reason: A function is pure: it computes its result from its arguments alone, so reading and writing entities belongs in an action.
  next: Make the caller an action, or pass the data in as an argument (agent)
  rule: effect.function_persistence
  help: https://jadpo.dev/docs/errors/effect.function_persistence

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A store of files is outside the program too:

```jadpo check project=e-fn-persist-files file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=e-fn-persist-files file=domains/bakery/types/sheet_text.jadpo hidden
/// A price sheet as a supplier sends it.
type SheetText = Text {
    min_length: 1
    max_length: 100000
}
```

```jadpo check project=e-fn-persist-files file=domains/bakery/actions/keep_sheet.jadpo hidden
use bakery/types { SheetText }
use bakery/libraries { sheet_files }

/// Keeps one price sheet.
action keep_sheet(sheet_text: SheetText) -> File {
    return sheet_files.kept(sheet_text)
}
```

domains/bakery/libraries/sheet_files.jadpo:

```jadpo check project=e-fn-persist-files file=domains/bakery/libraries/sheet_files.jadpo expect=effect.function_persistence
use app { sheets }
use bakery/types { SheetText }

/// Helpers for price sheets.
library sheet_files {
    /// A price sheet, kept as a file.
    function kept(sheet_text: SheetText) -> File {
        return sheets.put(encoding.to_utf8(sheet_text), FileType.csv)
    }
}
```

```text output
error: A function can't call `sheets.put`
 --> domains/bakery/libraries/sheet_files.jadpo:8:16
  |
8 |         return sheets.put(encoding.to_utf8(sheet_text), FileType.csv)
  |                ^^^^^^^^^^ a store of files
  reason: A function is pure, and `sheets.put` keeps bytes in a store of files, outside the program; only an action calls a store.
  next: Call `sheets.put` in an action, and pass in what it gives (agent)
  rule: effect.function_persistence
  help: https://jadpo.dev/docs/errors/effect.function_persistence

check failed: 1 error, 4 files
```

## How to fix it

**If the caller has to read the customer**, make it an action, and list the
failure `Customer.get` can give:

```jadpo fragment
action find_customer(id: Customer.id) fails CustomerNotFound -> Customer {
    return attempt Customer.get(id)
}
```

```jadpo check project=e-fn-persist-fix-1 hidden
entity Customer {
    id: Uuid

    policy: {
        read: [Access.public]
    }
}

action find_customer(id: Customer.id) fails CustomerNotFound -> Customer {
    return attempt Customer.get(id)
}
```

A customer that isn't there now ends `find_customer` with `CustomerNotFound`,
which Jadpo provides.

**If the function only works something out**, have the action read the data
and pass it in:

```jadpo fragment
function customer_key(customer: Customer) -> Customer.id {
    return customer.id
}
```

```jadpo check project=e-fn-persist-fix-2 hidden
entity Customer {
    id: Uuid
}

function customer_key(customer: Customer) -> Customer.id {
    return customer.id
}
```

The function now works from its argument alone, and the action that calls
it does the reading.

## Related

- [Functions and actions](../language/03-logic.md#functions-and-actions)
- [`effect.function_calls_action`](effect.function_calls_action.md)
