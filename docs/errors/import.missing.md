# `import.missing`

Every name a file uses that it doesn't declare comes in on a `use` line
(see [Imports](../language/01-project-layout-and-modules.md#imports)). The edit adds it, and `jadpo fmt` adds every
missing import at once:

```jadpo check project=e-import-missing file=app.jadpo hidden
application {}
```

```jadpo check project=e-import-missing file=shared/types/stock.jadpo hidden
type Stock = Int {
    min: 0
}
```

```jadpo check project=e-import-missing file=domains/bakery/actions/stock_loaf.jadpo expect=import.missing
action stock_loaf(each: Stock) -> Stock {
    return each
}
```

```text output
error: `Stock` isn't imported
 --> domains/bakery/actions/stock_loaf.jadpo:1:25
  |
1 | action stock_loaf(each: Stock) -> Stock {
  |                         ^^^^^ not imported
  reason: Every name a file uses that it doesn't declare comes in on a `use` line, so a reader sees where it lives; `Stock` is a type in `shared/types/stock.jadpo`.
  next: Add `use shared/types { Stock }`, or run `jadpo fmt`, which adds every missing import (agent)
  rule: import.missing
  help: https://jadpo.dev/docs/errors/import.missing

check failed: 1 error, 3 files
```
