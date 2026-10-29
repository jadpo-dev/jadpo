# `naming.generic_name`

```jadpo check project=e-generic expect=naming.generic_name
type Stock = Int {
    min: 0
}

library helpers {
    function add_holdback(subtotal: Stock) -> Stock {
        return subtotal
    }
}
```

```text output
error: `helpers` is too generic for a library
 --> main.jadpo:5:9
  |
5 | library helpers {
  |         ^^^^^^^ says nothing about what it holds
  reason: Domains and libraries are named after what they hold, because the name appears in every call; `common`, `core`, `helpers`, `misc` and `utils` say nothing.
  next: Rename the library after what its functions do (agent)
  rule: naming.generic_name
  help: docs/errors/naming.generic_name.md

check failed: 1 error, 1 file
```
