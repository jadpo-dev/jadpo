# `import.unused`

```jadpo check project=e-import-unused file=app.jadpo hidden
application {}
```

```jadpo check project=e-import-unused file=shared/types/stock.jadpo hidden
type Stock = Int {
    min: 0
}
```

```jadpo check project=e-import-unused file=shared/types/percent.jadpo hidden
type Percent = Percentage {
    min: 0%
    max: 100%
}
```

```jadpo check project=e-import-unused file=domains/bakery/actions/discount_loaf.jadpo hidden
use shared/types { Percent }

action discount_loaf(reduction: Percent) -> Unit {}
```

```jadpo check project=e-import-unused file=domains/bakery/actions/stock_loaf.jadpo expect=import.unused
use shared/types { Percent, Stock }

action stock_loaf(each: Stock) -> Stock {
    return each
}
```

```text output
error: `Percent` is imported but not used
 --> domains/bakery/actions/stock_loaf.jadpo:1:20
  |
1 | use shared/types { Percent, Stock }
  |                    ^^^^^^^ not used
  reason: A `use` line lists only the names its file uses, so each says where it comes from; nothing here uses `Percent`.
  next: Remove `Percent` from this line (agent)
  rule: import.unused
  help: https://jadpo.dev/docs/errors/import.unused

check failed: 1 error, 5 files
```
