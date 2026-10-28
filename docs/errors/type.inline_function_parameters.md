# `type.inline_function_parameters`

An inline function has as many parameters as its operation gives it
values: one item at a time, or two for `list.zip_with`, `map.filter` and
`map.map_values` (area 19):

```jadpo check project=e-inline-function-parameters expect=type.inline_function_parameters
type Loaf = Text {
    min_length: 1
}

type Stock = Int {
    min: 0
}

function in_stock(stock: Map<Loaf, Stock>) -> Map<Loaf, Stock> {
    return map.filter(stock, count -> count > 0)
}
```

```text output
error: `map.filter`'s `keep` takes two parameters, not one
 --> main.jadpo:10:30
   |
10 |     return map.filter(stock, count -> count > 0)
   |                              ^^^^^^^^^^^^^^^^^^ one parameter
  reason: `map.filter` gives its function each entry's key and its value, so it is written `(key, value) -> ...`, with a name for each.
  next: Write `(key, value) -> ...`, naming both (agent)
  rule: type.inline_function_parameters
  help: docs/language/03-logic.html#type-inline-function-parameters

check failed: 1 error, 1 file
```
