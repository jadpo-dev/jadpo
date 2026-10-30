# `import.unneeded`

Built-ins, what the file declares, what a test file's subject declares and
the file's own domain are seen without an import:

```jadpo check project=e-import-unneeded file=app.jadpo hidden
application {}
```

```jadpo check project=e-import-unneeded file=shared/types/stock.jadpo hidden
type Stock = Int {
    min: 0
}
```

```jadpo check project=e-import-unneeded file=domains/menu/actions/stock_item.jadpo
use shared/types { Stock }

/// A loaf's stock before any choices.
function loaf_stock() -> Stock {
    return 250
}

action stock_item() -> Stock {
    return loaf_stock()
}
```

```jadpo check project=e-import-unneeded file=domains/menu/actions/stock_item.test.jadpo expect=import.unneeded
use menu/actions { loaf_stock }

test "a loaf has 250 in stock" {
    assert loaf_stock() == 250
}
```

```text output
error: `loaf_stock` is declared in this test's subject
 --> domains/menu/actions/stock_item.test.jadpo:1:20
  |
1 | use menu/actions { loaf_stock }
  |                    ^^^^^^^^^^ needs no import
  reason: A test file sees everything its subject declares without importing it, and `loaf_stock` is in `stock_item.jadpo`.
  next: Remove this line (agent)
  rule: import.unneeded
  help: https://jadpo.dev/docs/errors/import.unneeded

check failed: 1 error, 4 files
```
