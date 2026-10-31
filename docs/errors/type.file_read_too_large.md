# `type.file_read_too_large`

Code reads a whole file into memory from a file field that can hold more than the 10 MB a whole read allows.

## Why Jadpo refuses it

`store.read` reads a file's bytes into memory all at once, for passing
them on, such as to an email as an attachment. A file field's
`max_size:` says how large its files can be. When that is over 10 MB, one
large upload could fill the server's memory, so the read is refused
where it is written rather than failing when a large file arrives.

A file's rows, of any size, are read a batch at a time by the format
helpers instead, which never hold the whole file.

## What it looks like

The bakery keeps suppliers' price sheets of up to 50 MB, and an action
reads one whole.

```jadpo check project=e-file-read-too-large file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

domains/bakery/entities/price_sheet.jadpo:

```jadpo check project=e-file-read-too-large file=domains/bakery/entities/price_sheet.jadpo migrate=initial
/// A supplier's price sheet, kept as it was sent.
entity PriceSheet {
    id: Uuid
    sheet: File {
        store: sheets
        max_size: 50mb
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
```

domains/bakery/actions/sheet_bytes.jadpo:

```jadpo check project=e-file-read-too-large file=domains/bakery/actions/sheet_bytes.jadpo expect=type.file_read_too_large
use app { sheets }
use bakery/entities { PriceSheet }

/// A price sheet's bytes, whole.
action sheet_bytes(sheet: PriceSheet.sheet) -> FileContents {
    return sheets.read(sheet)
}
```

```text output
error: `sheets.read` can't read `PriceSheet.sheet` whole
 --> domains/bakery/actions/sheet_bytes.jadpo:6:24
  |
6 |     return sheets.read(sheet)
  |                        ^^^^^ holds up to 50 MB
  reason: `sheets.read` reads a whole file into memory, for passing its bytes on, so it reads at most 10 MB; `PriceSheet.sheet` holds up to 50 MB. A file's rows, of any size, are read a batch at a time by the format helpers (`csv.read`, `tsv.read`, `json_lines.read`).
  next: Read `PriceSheet.sheet`'s rows with a format helper, such as `csv.read`, or keep its `max_size:` at 10 MB or less if `sheets.read` is what it needs (agent)
  rule: type.file_read_too_large
  help: https://jadpo.dev/docs/errors/type.file_read_too_large

check failed: 1 error, 3 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether the code needs the file's bytes whole, or its
rows.

**If the code needs the bytes whole**, to pass them on, keep the field's
`max_size:` at 10 MB or less. In
`domains/bakery/entities/price_sheet.jadpo`:

```jadpo fragment
        max_size: 10mb
```

```jadpo check project=e-file-read-too-large-fix-1 file=domains/bakery/entities/price_sheet.jadpo hidden
/// A supplier's price sheet, kept as it was sent.
entity PriceSheet {
    id: Uuid
    sheet: File {
        store: sheets
        max_size: 10mb
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
```

```jadpo check project=e-file-read-too-large-fix-1 file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=e-file-read-too-large-fix-1 file=domains/bakery/actions/sheet_bytes.jadpo migrate=initial hidden
use app { sheets }
use bakery/entities { PriceSheet }

/// A price sheet's bytes, whole.
action sheet_bytes(sheet: PriceSheet.sheet) -> FileContents {
    return sheets.read(sheet)
}
```

`PriceSheet.sheet` now holds files of up to 10 MB, so `sheets.read` can
read any of them whole.

If the code needs the sheet's rows, keep `max_size: 50mb` and read the
file with a format helper such as `csv.read`, which reads a batch of rows
at a time and hands each to an action
([Data formats](../language/19-standard-library.md#data-formats-json-csv-tsv-and-json-lines)).

## Related

- [Reading a file: `read`, and formats](../language/16-built-in-services.md#reading-a-file-read-and-formats)
- [`store.read(file)`](../language/16-built-in-services.md#storereadfile)
- [A `File` field's settings](../language/16-built-in-services.md#a-file-fields-settings)
- [Data formats: JSON, CSV, TSV and JSON Lines](../language/19-standard-library.md#data-formats-json-csv-tsv-and-json-lines)
