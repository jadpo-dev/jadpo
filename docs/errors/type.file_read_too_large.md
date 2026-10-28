# `type.file_read_too_large`

`store.read` reads a whole file into memory, so it reads a field only
when its `max_size:` is 10 MB or less; a file's rows, of any size, are
read a batch at a time by the format helpers:

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

```jadpo check project=e-file-read-too-large file=domains/bakery/entities/price_sheet.jadpo hidden migrate=initial
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
  help: docs/language/16-built-in-services.html#type-file-read-too-large

check failed: 1 error, 3 files
```
