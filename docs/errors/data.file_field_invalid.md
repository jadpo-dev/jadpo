# `data.file_field_invalid`

A `File` field says where its bytes are kept, `store:` naming a
connection that provides `files`, and how many it holds, `max_size:`;
`types:`, when given, lists `FileType`'s variants. A field whose
`store:` names the database is refused:

```jadpo check project=e-file-field-invalid file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=e-file-field-invalid file=domains/bakery/entities/price_sheet.jadpo expect=data.file_field_invalid
/// A supplier's price sheet, kept as it was sent.
entity PriceSheet {
    id: Uuid
    sheet: File {
        store: main
        max_size: 5mb
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
```

```text output
error: `PriceSheet.sheet`'s `store:` isn't a store of files
 --> domains/bakery/entities/price_sheet.jadpo:5:16
  |
5 |         store: main
  |                ^^^^ not a `files` connection
  reason: A `File` field's `store:` names a connection that provides `files`; `main` is a connection that keeps no files, and `app.jadpo` declares `sheets`.
  next: Write `store: sheets` (agent)
  rule: data.file_field_invalid
  help: https://jadpo.dev/docs/errors/data.file_field_invalid

check failed: 1 error, 2 files
```
