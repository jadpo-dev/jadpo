# `syntax.import_statement`

A file brings names in with another language's import instead of `use`.

## Why Jadpo refuses it

`import … from`, `from … import` and `import a.b.C` bring names in in other
languages, and Rust's paths are written with `::`. A Jadpo file brings names
in from a folder with `use <folder> { Names }`, so where each name comes
from reads the same in every file, and `jadpo fmt` writes those lines for
you.

## What it looks like

main.jadpo:

```jadpo check project=e-habit-import expect=syntax.import_statement
import { Count } from "./shared/types"

function same(count: Count) -> Count {
    return count
}
```

```text output
error: Jadpo writes `use`, not `import`
 --> main.jadpo:1:1
  |
1 | import { Count } from "./shared/types"
  | ^^^^^^ write `use`
  reason: `import … from` brings names into a JavaScript or TypeScript file, `from … import` into a Python module and `import a.b.C` into a Java or Kotlin file; a Jadpo file brings names in from a folder with `use <folder> { Names }`, and `jadpo fmt` writes those lines for you.
  next: Write `use shared/types { Count }` (agent)
  rule: syntax.import_statement
  help: https://jadpo.dev/docs/errors/syntax.import_statement

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A path written with `::` is this error too, and its `next:` tells you to
write `/` instead:

main.jadpo:

```jadpo check project=e-habit-import-2 expect=syntax.import_statement
use shared::types { Count }

function same(count: Count) -> Count {
    return count
}
```

```text output
error: Jadpo writes `use shared/types { Count }`, not `::`
 --> main.jadpo:1:11
  |
1 | use shared::types { Count }
  |           ^^ write `use shared/types { Count }`
  reason: `::` separates a path in Rust; a Jadpo `use` line names a folder with `/`, as in `use shared/types { Amount }`.
  next: Write `use shared/types { Count }` (agent)
  rule: syntax.import_statement
  help: https://jadpo.dev/docs/errors/syntax.import_statement

check failed: 1 error, 1 file
```

## How to fix it

**If you meant to bring in a name from a folder**, write a `use` line:

```jadpo fragment
use shared/types { Count }
```

```jadpo check project=e-habit-import-fix-1 file=domains/orders/actions/same.jadpo hidden
use shared/types { Count }

action same(count: Count) -> Count {
    return count
}
```

```jadpo check project=e-habit-import-fix-1 file=app.jadpo hidden
application {}
```

```jadpo check project=e-habit-import-fix-1 file=shared/types/count.jadpo hidden
type Count = Int {
    min: 0
}
```

`same` now reads `Count` from the shared folder.

**If you wrote `::`**, write `/`:

```jadpo fragment
use shared/types { Count }
```

```jadpo check project=e-habit-import-fix-2 file=domains/orders/actions/same.jadpo hidden
use shared/types { Count }

action same(count: Count) -> Count {
    return count
}
```

```jadpo check project=e-habit-import-fix-2 file=app.jadpo hidden
application {}
```

```jadpo check project=e-habit-import-fix-2 file=shared/types/count.jadpo hidden
type Count = Int {
    min: 0
}
```

The `use` line now names the folder as Jadpo writes it.

## Related

- [Imports](../language/01-project-layout-and-modules.md#imports)
- [The formatter keeps imports](../language/01-project-layout-and-modules.md#the-formatter-keeps-imports)
