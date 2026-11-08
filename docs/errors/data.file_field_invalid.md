# `data.file_field_invalid`

A `File` field's store, size or types are missing or wrong.

## Why Jadpo refuses it

A `File` field's column holds only a pointer to the file; the bytes live
in a connection that provides `files`, so the field has to name one in
`store:`. `max_size:` is the most bytes the field holds, which decides
when an upload is refused and whether `store.read` may read the file
whole, so there is no default. `types:`, when it is given, lists the kinds
of file the field takes.

## What it looks like

The field's `store:` names a connection that keeps no files. `app.jadpo`
declares `sheets`, a store of files, and `main` is the app's database.

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

domains/bakery/entities/price_sheet.jadpo:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A field with no `store:` is refused the same way:

```jadpo check project=e-file-no-store file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=e-file-no-store file=domains/bakery/entities/price_sheet.jadpo expect=data.file_field_invalid
/// A supplier's price sheet, kept as it was sent.
entity PriceSheet {
    id: Uuid
    sheet: File {
        max_size: 5mb
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
```

```text output
error: `PriceSheet.sheet` names no store for its bytes
 --> domains/bakery/entities/price_sheet.jadpo:4:12
  |
4 |     sheet: File {
  |            ^^^^^^ no `store:`
  reason: A `File` field's bytes live in a connection that provides `files`, `driver: Driver.disk` or a store of its own, which its `store:` names, and `app.jadpo` declares `sheets`.
  next: Add `store: sheets` to `sheet`'s block (agent)
  rule: data.file_field_invalid
  help: https://jadpo.dev/docs/errors/data.file_field_invalid

check failed: 1 error, 2 files
```

So is a field with no `max_size:`:

```jadpo check project=e-file-no-size file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=e-file-no-size file=domains/bakery/entities/price_sheet.jadpo expect=data.file_field_invalid
/// A supplier's price sheet, kept as it was sent.
entity PriceSheet {
    id: Uuid
    sheet: File {
        store: sheets
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
```

```text output
error: `PriceSheet.sheet` doesn't say how large a file it holds
 --> domains/bakery/entities/price_sheet.jadpo:4:12
  |
4 |     sheet: File {
  |            ^^^^^^ no `max_size:`
  reason: A `File` field's `max_size:` is the most bytes it holds, written as a size such as `5mb`: an upload past it is refused, and it decides whether `store.read` may read the file whole.
  next: Add `max_size:` to `sheet`'s block, such as `max_size: 5mb` (agent)
  rule: data.file_field_invalid
  help: https://jadpo.dev/docs/errors/data.file_field_invalid

check failed: 1 error, 2 files
```

`max_size:` is a whole number and a unit of `b`, `kb`, `mb` or `gb`:

```jadpo check project=e-file-bad-size file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=e-file-bad-size file=domains/bakery/entities/price_sheet.jadpo expect=data.file_field_invalid
/// A supplier's price sheet, kept as it was sent.
entity PriceSheet {
    id: Uuid
    sheet: File {
        store: sheets
        max_size: 5
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
```

```text output
error: `PriceSheet.sheet`'s `max_size:` isn't a size
 --> domains/bakery/entities/price_sheet.jadpo:6:19
  |
6 |         max_size: 5
  |                   ^ not a size
  reason: `max_size:` is a whole number of bytes, kilobytes, megabytes or gigabytes, written `700b`, `512kb`, `5mb` or `2gb`, each a thousand times the last.
  next: Write the size as a whole number and its unit, such as `5mb` (agent)
  rule: data.file_field_invalid
  help: https://jadpo.dev/docs/errors/data.file_field_invalid

check failed: 1 error, 2 files
```

`types:` lists `FileType`'s variants, and at least one:

```jadpo check project=e-file-bad-types file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=e-file-bad-types file=domains/bakery/entities/price_sheet.jadpo expect=data.file_field_invalid
/// A supplier's price sheet, kept as it was sent.
entity PriceSheet {
    id: Uuid
    sheet: File {
        store: sheets
        max_size: 5mb
        types: [csv]
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
```

```text output
error: `PriceSheet.sheet`'s `types:` isn't a list of file types
 --> domains/bakery/entities/price_sheet.jadpo:7:17
  |
7 |         types: [csv]
  |                 ^^^ not a `FileType`
  reason: `types:` lists the types a `File` field takes, each one of `FileType`'s: `FileType.png`, `FileType.jpeg`, `FileType.webp`, `FileType.pdf`, `FileType.csv`, `FileType.tsv`, `FileType.json` and `FileType.json_lines`.
  next: List the file types it takes, such as `types: [FileType.csv]` (agent)
  rule: data.file_field_invalid
  help: https://jadpo.dev/docs/errors/data.file_field_invalid

check failed: 1 error, 2 files
```

## How to fix it

**If the field names no store, or the wrong one**, name a connection that
provides `files`, and say how large a file may be:

```jadpo fragment
    sheet: File {
        store: sheets
        max_size: 5mb
    }
```

```jadpo check project=e-file-field-invalid-fix-1 file=domains/bakery/entities/price_sheet.jadpo hidden
/// A supplier's price sheet, kept as it was sent.
entity PriceSheet {
    id: Uuid
    sheet: File {
        store: sheets
        max_size: 5mb
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
```

```jadpo check project=e-file-field-invalid-fix-1 file=app.jadpo hidden migrate=initial
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

The field's bytes now go in `sheets`, and a larger upload is refused.

**If the field only takes some kinds of file**, list them as `FileType`'s
variants:

```jadpo fragment
    sheet: File {
        store: sheets
        max_size: 5mb
        types: [FileType.csv]
    }
```

```jadpo check project=e-file-field-invalid-fix-2 file=domains/bakery/entities/price_sheet.jadpo hidden
/// A supplier's price sheet, kept as it was sent.
entity PriceSheet {
    id: Uuid
    sheet: File {
        store: sheets
        max_size: 5mb
        types: [FileType.csv]
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
```

```jadpo check project=e-file-field-invalid-fix-2 file=app.jadpo hidden migrate=initial
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

An upload of any other kind of file is now refused.

**If there is no store of files yet**, declare one in `app.jadpo`, with
the `driver: Driver.disk` and `path:` shown here, and name it in the
field's `store:`.

```jadpo fragment
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
```

```jadpo check project=e-file-field-invalid-fix-3 file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=e-file-field-invalid-fix-3 file=domains/bakery/entities/price_sheet.jadpo hidden migrate=initial
/// A supplier's price sheet, kept as it was sent.
entity PriceSheet {
    id: Uuid
    sheet: File {
        store: sheets
        max_size: 5mb
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
```

The app now has a place to keep the field's files.

## Related

- [Files](../language/16-built-in-services.md#files)
