# `syntax.import_statement`

A file brings names in with `use <folder> { Names }`; `jadpo fmt` writes those lines.

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
  help: docs/language/01-project-layout-and-modules.html#syntax-import-statement

check failed: 1 error, 1 file
```
