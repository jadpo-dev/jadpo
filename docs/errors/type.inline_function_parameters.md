# `type.inline_function_parameters`

An inline function has more or fewer parameters than the operation it is passed to gives it, such as one parameter for `map.filter`, which gives two.

## Why Jadpo refuses it

An operation gives its inline function a fixed set of values: one item at
a time for most, an entry's key and its value for `map.filter` and
`map.map_values`, and an item from each list for `list.zip_with`. A
function with one name for two values would leave unclear which one the
name means, so each value gets its own name.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/type.inline_function_parameters

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the operation gives two values**, name both, in brackets:

```jadpo fragment
function in_stock(stock: Map<Loaf, Stock>) -> Map<Loaf, Stock> {
    return map.filter(stock, (loaf, count) -> count > 0)
}
```

```jadpo check project=e-inline-function-parameters-fix hidden
type Loaf = Text {
    min_length: 1
}

type Stock = Int {
    min: 0
}

function in_stock(stock: Map<Loaf, Stock>) -> Map<Loaf, Stock> {
    return map.filter(stock, (loaf, count) -> count > 0)
}
```

`loaf` is each entry's key and `count` its value, so `in_stock` keeps the
loaves with some left.

## Related

- [Passing functions](../language/03-logic.md#passing-functions)
- [Two parameters](../language/03-logic.md#two-parameters)
- [Maps](../language/19-standard-library.md#maps)
