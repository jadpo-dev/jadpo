# `type.inline_function_placement`

An inline function is written only as an argument to a standard-library collection operation:

```jadpo check project=e-inline-function expect=type.inline_function_placement
type Stock = Int {
    min: 0
}

function same_price(per_pack: Stock) -> Stock {
    return per_pack
}

function passed_on(per_pack: Stock) -> Stock {
    return same_price(item -> per_pack)
}
```

```text output
error: An inline function is only an argument to a collection operation
 --> main.jadpo:10:23
   |
10 |     return same_price(item -> per_pack)
   |                       ^^^^^^^^^^^^^^^^ inline function
  reason: Functions aren't values: `item -> expression` is written only as an argument to a standard-library collection operation, such as `list.map` or `list.filter`.
  next: Write the expression on its own, or pass it to a collection operation (agent)
  rule: type.inline_function_placement
  help: docs/errors/type.inline_function_placement.md

check failed: 1 error, 1 file
```

Two parameters, `(a, b) -> expression`, are only for a standard operation
that gives its function two values (area 19):

```jadpo check project=e-inline-function-two expect=type.inline_function_placement
type Stock = Int {
    min: 0
}

function larger(first: Stock, second: Stock) -> Stock {
    if (first > second) {
        return first
    }
    return second
}

function dearer(per_pack: Stock) -> Stock {
    return larger((a, b) -> a, per_pack)
}
```

```text output
error: An inline function is only an argument to a collection operation
 --> main.jadpo:13:19
   |
13 |     return larger((a, b) -> a, per_pack)
   |                   ^^^^^^^^^^^ inline function
  reason: Functions aren't values: `(key, value) -> expression` is written only as an argument to a standard operation that gives its function two values, `list.zip_with`, `map.filter` or `map.map_values`.
  next: Write the expression on its own, or pass it to a collection operation (agent)
  rule: type.inline_function_placement
  help: docs/errors/type.inline_function_placement.md

check failed: 1 error, 1 file
```
