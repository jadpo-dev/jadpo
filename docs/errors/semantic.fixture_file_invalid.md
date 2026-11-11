# `semantic.fixture_file_invalid`

A test's `fixture_file` names a file its domain's `fixtures/files/` doesn't hold, one of a type a `File` can't hold, or a store that doesn't keep files.

## Why Jadpo refuses it

`fixture_file("customers.csv", store: imports)` keeps a real file in the
test's in-memory store and gives the stored `File`. The file is found
as the test is checked, so a name that finds nothing is an error before any
test runs, not a fault in the middle of one. Its `FileType` is what its
name ends in, so a name that ends in no type a `File` holds is refused
too. Each domain keeps its own fixture files beside its tests, so a test
reads only its own domain's, and a test in the root `tests/` has none.

## What it looks like

A shop keeps the sheets of customers it is sent:

```jadpo check project=e-fixture-file-invalid file=app.jadpo hidden
application {
    connections: {
        imports: {
            driver: Driver.disk
            path: "/var/lib/shop/imports"
        }
    }
}
```

```jadpo check project=e-fixture-file-invalid file=domains/crm/entities/customer_sheet.jadpo hidden migrate=initial
/// A sheet of customers to add.
entity CustomerSheet {
    id: Uuid
    sheet: File {
        store: imports
        max_size: 1mb
        types: [FileType.csv]
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
```

```jadpo check project=e-fixture-file-invalid file=domains/crm/actions/keep_sheet.jadpo hidden
use crm/entities { CustomerSheet }

/// Keeps a sheet of customers.
action keep_sheet(sheet: File) -> CustomerSheet {
    return CustomerSheet.create {
        set: {
            sheet: sheet
        }
    }
}
```

```jadpo check project=e-fixture-file-invalid file=domains/crm/fixtures/files/customers.csv hidden
Name
Ada Lovelace
```

The sheet is `customers.csv`, and the test names `customer.csv`:

domains/crm/actions/keep_sheet.test.jadpo:

```jadpo check project=e-fixture-file-invalid file=domains/crm/actions/keep_sheet.test.jadpo expect=semantic.fixture_file_invalid
use app { imports }
use crm/entities { CustomerSheet }

test "a kept sheet reads back" {
    var sheet_kept: CustomerSheet = keep_sheet(fixture_file("customer.csv", store: imports))
    var read_back: CustomerSheet = attempt CustomerSheet.get(sheet_kept.id)
    assert read_back == sheet_kept
}
```

```text output
error: `domains/crm/fixtures/files/` holds no `customer.csv`
 --> domains/crm/actions/keep_sheet.test.jadpo:5:61
  |
5 |     var sheet_kept: CustomerSheet = keep_sheet(fixture_file("customer.csv", store: imports))
  |                                                             ^^^^^^^^^^^^^^ no such file
  reason: `fixture_file` reads a file from the test's own domain's `fixtures/files/`, which holds `customers.csv`.
  next: Use `"customers.csv"` instead (agent)
  rule: semantic.fixture_file_invalid
  help: https://jadpo.dev/docs/errors/semantic.fixture_file_invalid

check failed: 1 error, 4 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the name is misspelt**, use the file's name as it is in the
domain's `fixtures/files/`:

```jadpo fragment
    var sheet_kept: CustomerSheet = keep_sheet(fixture_file("customers.csv", store: imports))
```

```jadpo check project=e-fixture-file-invalid-fix file=domains/crm/actions/keep_sheet.test.jadpo hidden
use app { imports }
use crm/entities { CustomerSheet }

test "a kept sheet reads back" {
    var sheet_kept: CustomerSheet = keep_sheet(fixture_file("customers.csv", store: imports))
    var read_back: CustomerSheet = attempt CustomerSheet.get(sheet_kept.id)
    assert read_back == sheet_kept
}
```

```jadpo check project=e-fixture-file-invalid-fix file=app.jadpo hidden
application {
    connections: {
        imports: {
            driver: Driver.disk
            path: "/var/lib/shop/imports"
        }
    }
}
```

```jadpo check project=e-fixture-file-invalid-fix file=domains/crm/entities/customer_sheet.jadpo hidden
/// A sheet of customers to add.
entity CustomerSheet {
    id: Uuid
    sheet: File {
        store: imports
        max_size: 1mb
        types: [FileType.csv]
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
```

```jadpo check project=e-fixture-file-invalid-fix file=domains/crm/fixtures/files/customers.csv hidden
Name
Ada Lovelace
```

```jadpo check project=e-fixture-file-invalid-fix file=domains/crm/actions/keep_sheet.jadpo hidden migrate=initial
use crm/entities { CustomerSheet }

/// Keeps a sheet of customers.
action keep_sheet(sheet: File) -> CustomerSheet {
    return CustomerSheet.create {
        set: {
            sheet: sheet
        }
    }
}
```

The test keeps the domain's `customers.csv` and passes it to the action.

**If the file isn't there yet**, put it beside the domain's tests:

```sh
cp ~/Downloads/customers.csv domains/crm/fixtures/files/
```

The name then finds it, and the test reads it as it runs.

**If its name ends in no `FileType`**, rename it to end in its type,
`.csv`, `.tsv`, `.json`, `.jsonl`, `.pdf`, `.png`, `.jpeg` or `.jpg`, or
`.webp`, and use the new name in the test:

```sh
mv domains/crm/fixtures/files/customers.txt domains/crm/fixtures/files/customers.csv
```

Its `File` is then a `FileType.csv`.

**If the test is in the root `tests/`**, move it beside its subject in the
domain whose `fixtures/files/` holds the file:

```sh
mv tests/keep_sheet.test.jadpo domains/crm/actions/keep_sheet.test.jadpo
```

The test then reads its own domain's fixture files.

## Related

- [Files in tests](../language/13-testing.md#files)
- [Shared test data: fixtures](../language/13-testing.md#shared-test-data-fixtures)
- [Files](../language/16-built-in-services.md#files)
