# `import.missing`

A file uses a name declared elsewhere in the project without bringing it in on a `use` line.

## Why Jadpo refuses it

Each file says where its other names come from, on `use` lines at its
top, so you or an agent can read where a name lives from the file
itself, without searching the project. A name the file uses but doesn't
import leaves that question open.

The fix is never in doubt. Every name a file can import is unique where
it can be imported, so only one `use` line brings it in.

## What it looks like

domains/bakery/actions/stock_loaf.jadpo:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If one name is missing**, add its `use` line at the top of the file.
In `domains/bakery/actions/stock_loaf.jadpo`:

```jadpo fragment
use shared/types { Stock }
```

```jadpo check project=e-import-missing-fix-1 file=domains/bakery/actions/stock_loaf.jadpo hidden
use shared/types { Stock }

action stock_loaf(each: Stock) -> Stock {
    return each
}
```

```jadpo check project=e-import-missing-fix-1 file=app.jadpo hidden
application {}
```

```jadpo check project=e-import-missing-fix-1 file=shared/types/stock.jadpo hidden
type Stock = Int {
    min: 0
}
```

The file now says that `Stock` is in `shared/types/stock.jadpo`, and the
project checks.

**If several names are missing**, let the formatter write the lines:

```sh
jadpo fmt
```

`jadpo fmt` adds every missing import and removes every unused one, one
line per folder, sorted. It does this once every file in the project
parses.

## Related

- [Imports](../language/01-project-layout-and-modules.md#imports)
- [What is imported](../language/01-project-layout-and-modules.md#what-is-imported)
- [The formatter keeps imports](../language/01-project-layout-and-modules.md#the-formatter-keeps-imports)
- [Where the lines go](../language/01-project-layout-and-modules.md#where-the-lines-go)
