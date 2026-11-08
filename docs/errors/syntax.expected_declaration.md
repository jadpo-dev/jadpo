# `syntax.expected_declaration`

A line at the top of a file doesn't start a declaration.

## Why Jadpo refuses it

A file holds declarations, such as `type`, `function`, `action`, `query` and
`entity`, each starting a line. Code at the top of a file belongs to no
declaration, so nothing could ever run it.

## What it looks like

main.jadpo:

```jadpo check project=e-expected-declaration expect=syntax.expected_declaration
Count = Int
```

```text output
error: `Count` doesn't start a declaration
 --> main.jadpo:1:1
  |
1 | Count = Int
  | ^^^^^ expected a declaration
  reason: A file holds declarations, such as `type`, `function`, `action`, `query` and `entity`, each starting a line.
  next: Start the declaration with `type`: `type Count = ...` (agent)
  rule: syntax.expected_declaration
  help: https://jadpo.dev/docs/errors/syntax.expected_declaration

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A top-level `var` is this error too, and its `next:` points to `constant`,
because at the top of a file a fixed value is a constant:

main.jadpo:

```jadpo check project=e-expected-declaration-var expect=syntax.expected_declaration
type VatRate = Decimal {
    scale: 4
}

var vat_rate: VatRate = 0.2000
```

```text output
error: `var` doesn't start a declaration
 --> main.jadpo:5:1
  |
5 | var vat_rate: VatRate = 0.2000
  | ^^^ expected a declaration
  reason: `var` names a value inside a body; at the top of a file a fixed value is a `constant`.
  next: Write `constant vat_rate: VatRate = 0.2000` (agent)
  rule: syntax.expected_declaration
  help: https://jadpo.dev/docs/errors/syntax.expected_declaration

check failed: 1 error, 1 file
```

`public` with nothing after it is this error too, because it marks a
declaration another domain may use:

main.jadpo:

```jadpo check project=e-expected-declaration-3 expect=syntax.expected_declaration
public
```

```text output
error: `public` has no declaration after it
 --> main.jadpo:1:7
  |
1 | public
  |       ^ expected a declaration
  reason: `public` marks a declaration another domain may use; found the end of the file instead.
  next: Write the declaration after `public`, or remove `public` (agent)
  rule: syntax.expected_declaration
  help: https://jadpo.dev/docs/errors/syntax.expected_declaration

check failed: 1 error, 1 file
```

A `decline` with no declaration under it is this error too, because a
decline stands directly above the declaration or member it covers:

main.jadpo:

```jadpo check project=e-expected-declaration-4 expect=syntax.expected_declaration
type Count = Int {
    min: 0
}

decline type.unit_as_number: "Knots the scouts can tie, not a speed"
```

```text output
error: This decline has no declaration under it
 --> main.jadpo:5:69
  |
5 | decline type.unit_as_number: "Knots the scouts can tie, not a speed"
  |                                                                     ^ expected a declaration
  reason: A `decline` line stands directly above the declaration or member whose finding it declines; found the end of the file instead.
  next: Put the `decline` line directly above its declaration, or remove it (agent)
  rule: syntax.expected_declaration
  help: https://jadpo.dev/docs/errors/syntax.expected_declaration

check failed: 1 error, 1 file
```

## How to fix it

**If you meant to declare a type**, start the line with `type`:

```jadpo check project=e-expected-declaration-fix-1
type Count = Int {}
```

`Count` is now a type holding a whole number.

**If you meant a fixed value**, write `constant`:

```jadpo fragment
constant vat_rate: VatRate = 0.2000
```

```jadpo check project=e-expected-declaration-fix-2 hidden
type VatRate = Decimal {
    scale: 4
}

constant vat_rate: VatRate = 0.2000
```

`vat_rate` is now a constant any code in the domain can read.

**If you meant to open a declaration to other domains**, write it after
`public`:

```jadpo fragment
public action same(count: Count) -> Count {
    return count
}
```

```jadpo check project=e-expected-declaration-fix-3 hidden
type Count = Int {
    min: 0
}

public action same(count: Count) -> Count {
    return count
}
```

`same` can now be called from another domain.

**If you meant to decline a finding**, put the line directly above its
declaration, or remove it:

```jadpo check project=e-expected-declaration-fix-4
decline type.unit_as_number: "Knots the scouts can tie, not a speed"
type Knots = Int {
    min: 0
}
```

The decline now covers `type.unit_as_number` on `Knots`, the declaration
below it.

## Related

- [Source files](../language/01-project-layout-and-modules.md#source-files)
- [Constants](../language/03-logic.md#constants)
- [What can be public](../language/01-project-layout-and-modules.md#what-can-be-public)
- [Writing a decline](../language/01-project-layout-and-modules.md#writing-a-decline)
