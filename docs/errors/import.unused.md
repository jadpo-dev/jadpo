# `import.unused`

A `use` line brings in something the file doesn't use.

## Why Jadpo refuses it

A `use` line says where each name a file uses comes from, so it lists only
those names. A name nothing uses would point a reader at a dependency that
isn't there, and would stop being true without anyone noticing.

## What it looks like

domains/bakery/actions/stock_loaf.jadpo:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A `use` line for a domain brings it in for calls such as `lending.borrow()`,
so a file that makes none doesn't need it:

```jadpo check project=e-import-unused-domain file=app.jadpo hidden
application {}
```

```jadpo check project=e-import-unused-domain file=domains/lending/actions/borrow.jadpo hidden
action borrow() -> Unit {}
```

```jadpo check project=e-import-unused-domain file=domains/bakery/actions/tidy_up.jadpo expect=import.unused
use lending

action tidy_up() -> Unit {}
```

```text output
error: `lending` is imported but not used
 --> domains/bakery/actions/tidy_up.jadpo:1:5
  |
1 | use lending
  |     ^^^^^^^ no calls through it
  reason: `use lending` brings in a domain for calls such as `lending.<action>(...)`, and this file makes none.
  next: Remove this line (agent)
  rule: import.unused
  help: https://jadpo.dev/docs/errors/import.unused

check failed: 1 error, 3 files
```

A package's line is refused the same way when the file writes nothing through
the package, and empty braces, `use shared/types {}`, bring in nothing.

## How to fix it

**If one name on the line is unused**, remove it, or run `jadpo fmt`, which
removes every unused import:

```jadpo fragment
use shared/types { Stock }
```

```jadpo check project=e-import-unused-fix file=domains/bakery/actions/stock_loaf.jadpo hidden
use shared/types { Stock }

action stock_loaf(each: Stock) -> Stock {
    return each
}
```

```jadpo check project=e-import-unused-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e-import-unused-fix file=shared/types/stock.jadpo hidden
type Stock = Int {
    min: 0
}
```

```jadpo check project=e-import-unused-fix file=shared/types/percent.jadpo hidden
type Percent = Percentage {
    min: 0%
    max: 100%
}
```

```jadpo check project=e-import-unused-fix file=domains/bakery/actions/discount_loaf.jadpo hidden
use shared/types { Percent }

action discount_loaf(reduction: Percent) -> Unit {}
```

The line now says only what this file takes from `shared/types`.

## Related

- [Imports](../language/01-project-layout-and-modules.md#imports)
- [What is imported](../language/01-project-layout-and-modules.md#what-is-imported)
- [The formatter keeps imports](../language/01-project-layout-and-modules.md#the-formatter-keeps-imports)
