# `import.duplicate`

A file imports the same name twice, or has two `use` lines for one folder.

## Why Jadpo refuses it

A folder has one `use` line, listing every name the file takes from it, and
each name is imported once. Then the top of a file reads as one list per
place, and a reviewer sees every name a file takes from a folder at a glance.

## What it looks like

domains/bakery/actions/stock_loaf.jadpo:

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
  help: https://jadpo.dev/docs/errors/import.duplicate

check failed: 1 error, 4 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A name listed twice on one line is the same mistake:

```jadpo check project=e-import-duplicate-name file=app.jadpo hidden
application {}
```

```jadpo check project=e-import-duplicate-name file=shared/types/stock.jadpo hidden
type Stock = Int {
    min: 0
}
```

```jadpo check project=e-import-duplicate-name file=domains/bakery/actions/stock_loaf.jadpo expect=import.duplicate
use shared/types { Stock, Stock }

action stock_loaf(each: Stock) -> Stock {
    return each
}
```

```text output
error: `Stock` is imported twice
 --> domains/bakery/actions/stock_loaf.jadpo:1:27
  |
1 | use shared/types { Stock, Stock }
  |                           ^^^^^ imported again
  |                    ----- first imported here
  reason: Each name is imported once, on the line of the folder that holds it.
  next: Remove this `Stock` (agent)
  rule: import.duplicate
  help: https://jadpo.dev/docs/errors/import.duplicate

check failed: 1 error, 3 files
```

## How to fix it

**If a folder has two lines**, move their names onto one:

```jadpo fragment
use shared/types { Percent, Stock }
```

```jadpo check project=e-import-duplicate-fix-1 file=domains/bakery/actions/stock_loaf.jadpo hidden
use shared/types { Percent, Stock }

action stock_loaf(each: Stock, reduction: Percent) -> Stock {
    return each
}
```

```jadpo check project=e-import-duplicate-fix-1 file=app.jadpo hidden
application {}
```

```jadpo check project=e-import-duplicate-fix-1 file=shared/types/stock.jadpo hidden
type Stock = Int {
    min: 0
}
```

```jadpo check project=e-import-duplicate-fix-1 file=shared/types/percent.jadpo hidden
type Percent = Percentage {
    min: 0%
    max: 100%
}
```

The folder now has one line naming everything taken from it. `jadpo fmt` makes
the same edit, for every file at once.

## Related

- [Imports](../language/01-project-layout-and-modules.md#imports)
- [What is imported](../language/01-project-layout-and-modules.md#what-is-imported)
- [The formatter keeps imports](../language/01-project-layout-and-modules.md#the-formatter-keeps-imports)
