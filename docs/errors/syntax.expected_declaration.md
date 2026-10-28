# `syntax.expected_declaration`

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
  help: docs/language/01-project-layout-and-modules.html#syntax-expected-declaration

check failed: 1 error, 1 file
```

A top-level `var` is this error too, and its `next:` points to `constant`,
because at the top of a file a fixed value is a constant (area 3,
[Constants](../language/03-logic.md#constants)):

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
  help: docs/language/01-project-layout-and-modules.html#syntax-expected-declaration

check failed: 1 error, 1 file
```
