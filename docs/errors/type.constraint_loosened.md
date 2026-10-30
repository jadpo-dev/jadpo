# `type.constraint_loosened`

A refinement can only add to the rules of the type it builds on. Every rule along the chain still holds, so a looser bound would
never apply and would only mislead the reader:

```jadpo check project=e-constraint-loosened expect=type.constraint_loosened
type ProductName = Text {
    min_length: 1
    max_length: 120
}

type ShortProductName = ProductName {
    max_length: 150
}
```

```text output
error: `max_length: 150` loosens `ProductName`'s `max_length: 120`
 --> main.jadpo:7:5
  |
7 |     max_length: 150
  |     ^^^^^^^^^^ looser than `max_length: 120`
  reason: A refinement only adds to the rules of the type it builds on, and every rule along the way still holds, so a value still needs `max_length: 120` and this one would never apply.
  next: Make `max_length` at most 120, or remove it (agent)
  rule: type.constraint_loosened
  help: https://jadpo.dev/docs/errors/type.constraint_loosened

check failed: 1 error, 1 file
```
