# `import.unneeded`

A file imports a name it already sees without a `use` line, such as a built-in or something its test subject declares.

## Why Jadpo refuses it

A `use` line tells a reader where a name lives. Some names need no such
line, because where they live is already plain: built-in types and the
standard library, what the file declares itself, what a test file's
subject declares, and the file's own domain. An import of one of them
says nothing, and suggests the name lives somewhere it doesn't.

## What it looks like

domains/menu/actions/stock_item.jadpo:

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

domains/menu/actions/stock_item.test.jadpo, its test file:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the name is one the file already sees**, remove the line.
`domains/menu/actions/stock_item.test.jadpo` becomes:

```jadpo check project=e-import-unneeded-fix-1 file=domains/menu/actions/stock_item.test.jadpo
test "a loaf has 250 in stock" {
    assert loaf_stock() == 250
}
```

```jadpo check project=e-import-unneeded-fix-1 file=app.jadpo hidden
application {}
```

```jadpo check project=e-import-unneeded-fix-1 file=shared/types/stock.jadpo hidden
type Stock = Int {
    min: 0
}
```

```jadpo check project=e-import-unneeded-fix-1 file=domains/menu/actions/stock_item.jadpo hidden
use shared/types { Stock }

/// A loaf's stock before any choices.
function loaf_stock() -> Stock {
    return 250
}

action stock_item() -> Stock {
    return loaf_stock()
}
```

The test calls `loaf_stock` directly, as it already could. Anything else
it uses still comes in on a `use` line.

## Related

- [What is imported](../language/01-project-layout-and-modules.md#what-is-imported)
- [What `jadpo check` reports](../language/01-project-layout-and-modules.md#what-jadpo-check-reports)
- [A test beside its subject](../language/13-testing.md#a-test-beside-its-subject)
