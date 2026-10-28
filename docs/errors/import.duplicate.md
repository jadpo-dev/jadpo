# `import.duplicate`

```jadpo check project=e-import-duplicate file=app.jadpo hidden
application {}
```

```jadpo check project=e-import-duplicate file=shared/types/stock.jadpo hidden
type Stock = Int {
    min: 0
}
```

```jadpo check project=e-import-duplicate file=shared/types/percent.jadpo hidden
type Percent = Percentage {
    min: 0%
    max: 100%
}
```

```jadpo check project=e-import-duplicate file=domains/bakery/actions/stock_loaf.jadpo expect=import.duplicate
use shared/types { Percent }
use shared/types { Stock }

action stock_loaf(each: Stock, reduction: Percent) -> Stock {
    return each
}
```

```text output
error: `shared/types` has two `use` lines
 --> domains/bakery/actions/stock_loaf.jadpo:2:5
  |
1 | use shared/types { Percent }
  |     ------------ first line
2 | use shared/types { Stock }
  |     ^^^^^^^^^^^^ a second line
  reason: Each folder has one `use` line, listing every name the file takes from it; `use shared/types` is written twice.
  next: Move its names onto the first `shared/types` line (agent)
  rule: import.duplicate
  help: docs/language/01-project-layout-and-modules.html#import-duplicate

check failed: 1 error, 4 files
```
