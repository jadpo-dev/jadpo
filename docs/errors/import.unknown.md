# `import.unknown`

A `use` line names a domain, a folder of a domain or of `shared/`, or a
root file, and only names that are there. A name imported from the wrong
folder gets the edit that moves it:

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
  help: docs/language/01-project-layout-and-modules.html#import-unknown

check failed: 1 error, 3 files
```
