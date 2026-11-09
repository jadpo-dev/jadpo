# `import.unknown`

A `use` line names a folder or a name that isn't there.

## Why Jadpo refuses it

A `use` line says where a name comes from, so a reader can trust it without
looking. A line naming a folder that doesn't exist, or a name that folder
doesn't hold, would be a claim about the project that isn't true.

When a name is declared somewhere else, the error says where. When nothing is
called that, Jadpo can't tell what you meant.

## What it looks like

domains/bakery/actions/stock_loaf.jadpo:

```jadpo check project=e-import-unknown file=app.jadpo hidden
application {}
```

```jadpo check project=e-import-unknown file=shared/types/stock.jadpo hidden
type Stock = Int {
    min: 0
}
```

```jadpo check project=e-import-unknown file=domains/bakery/actions/stock_loaf.jadpo expect=import.unknown
use shared/failures { Stock }

action stock_loaf(each: Stock) -> Stock {
    return each
}
```

```text output
error: `Stock` isn't in `shared/failures`
 --> domains/bakery/actions/stock_loaf.jadpo:1:23
  |
1 | use shared/failures { Stock }
  |                       ^^^^^ not in this folder
  reason: A `use` line brings in names from the folder it names; `Stock` is a type in `shared/types/stock.jadpo`, so it comes in from `shared/types`.
  next: Import `Stock` from `shared/types` (agent)
  rule: import.unknown
  help: https://jadpo.dev/docs/errors/import.unknown

check failed: 1 error, 3 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A name nothing in the project declares gets the nearest name, if there is one:

```jadpo check project=e-import-unknown-name file=app.jadpo hidden
application {}
```

```jadpo check project=e-import-unknown-name file=shared/types/stock.jadpo hidden
type Stock = Int {
    min: 0
}
```

```jadpo check project=e-import-unknown-name file=domains/bakery/actions/stock_loaf.jadpo expect=import.unknown
use shared/types { Stock, Stok }

action stock_loaf(each: Stock) -> Stock {
    return each
}
```

```text output
error: `Stok` isn't declared
 --> domains/bakery/actions/stock_loaf.jadpo:1:27
  |
1 | use shared/types { Stock, Stok }
  |                           ^^^^ not declared
  reason: Nothing in this project is called `Stok`, so this line brings in nothing by that name. Did you mean `Stock`?
  next: Use `Stock` instead of `Stok` (agent)
  rule: import.unknown
  help: https://jadpo.dev/docs/errors/import.unknown

check failed: 1 error, 3 files
```

A folder that isn't there is refused the same way:

```jadpo check project=e-import-unknown-folder file=app.jadpo hidden
application {}
```

```jadpo check project=e-import-unknown-folder file=shared/types/stock.jadpo hidden
type Stock = Int {
    min: 0
}
```

```jadpo check project=e-import-unknown-folder file=domains/bakery/actions/stock_loaf.jadpo expect=import.unknown
use shared/tipes { Stock }

action stock_loaf(each: Stock) -> Stock {
    return each
}
```

```text output
error: `shared/tipes` isn't a folder of `shared/`
 --> domains/bakery/actions/stock_loaf.jadpo:1:5
  |
1 | use shared/tipes { Stock }
  |     ^^^^^^^^^^^^ not a folder
  reason: `shared/` holds only `types/`, `libraries/` and `failures/`: what every domain may use. Did you mean `types`?
  next: Use `shared/types` instead of `shared/tipes` (agent)
  rule: import.unknown
  help: https://jadpo.dev/docs/errors/import.unknown

check failed: 1 error, 3 files
```

A library comes in whole, by its name, and a line that names one of its
functions is told to import the library. A `use` line in files with no
`app.jadpo` above them is refused too, since those files have no folders.

## How to fix it

**If the name is in another folder**, import it from there:

```jadpo fragment
use shared/types { Stock }
```

```jadpo check project=e-import-unknown-fix-1 file=domains/bakery/actions/stock_loaf.jadpo hidden
use shared/types { Stock }

action stock_loaf(each: Stock) -> Stock {
    return each
}
```

```jadpo check project=e-import-unknown-fix-1 file=app.jadpo hidden
application {}
```

```jadpo check project=e-import-unknown-fix-1 file=shared/types/stock.jadpo hidden
type Stock = Int {
    min: 0
}
```

The line now names the folder that holds `Stock`.

## Related

- [Imports](../language/01-project-layout-and-modules.md#imports)
- [What is imported](../language/01-project-layout-and-modules.md#what-is-imported)
- [What `jadpo check` reports](../language/01-project-layout-and-modules.md#what-jadpo-check-reports)
