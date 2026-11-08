# `type.inline_function_placement`

An inline function is written somewhere other than as an argument to a standard-library operation that takes one, such as an argument to your own function.

## Why Jadpo refuses it

Functions aren't values in Jadpo: you can't keep one in a variable or
pass one to your own functions and actions. So what a piece of code does
is always written where it runs, and a reviewer never has to trace which
function a variable held. The standard library's collection operations,
such as `list.map` and `list.filter`, are the one place an inline
function is written.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/type.inline_function_placement

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An inline function with two parameters, `(a, b) -> ...`, is the same
error anywhere but an operation that gives its function two values,
`list.zip_with`, `map.filter` or `map.map_values`:

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/type.inline_function_placement

check failed: 1 error, 1 file
```

## How to fix it

The question is whether the code works on a collection.

**If it works on one value**, pass the value, and write what the inline
function did inside the function you call:

```jadpo fragment
function passed_on(per_pack: Stock) -> Stock {
    return same_price(per_pack)
}
```

```jadpo check project=e-inline-function-fix-1 hidden
type Stock = Int {
    min: 0
}

function same_price(per_pack: Stock) -> Stock {
    return per_pack
}

function passed_on(per_pack: Stock) -> Stock {
    return same_price(per_pack)
}
```

`passed_on` gives `same_price` the value it works on.

**If it works on each item of a list**, pass the inline function to the
collection operation that does it:

```jadpo fragment
function priced(packs: List<Stock>) -> List<Stock> {
    return list.map(packs, item -> same_price(item))
}
```

```jadpo check project=e-inline-function-fix-2 hidden
type Stock = Int {
    min: 0
}

function same_price(per_pack: Stock) -> Stock {
    return per_pack
}

function priced(packs: List<Stock>) -> List<Stock> {
    return list.map(packs, item -> same_price(item))
}
```

`list.map` calls the inline function once for each item, and `priced`
gives the list of results.

## Related

- [Passing functions](../language/03-logic.md#passing-functions)
- [Two parameters](../language/03-logic.md#two-parameters)
- [Lists](../language/19-standard-library.md#lists)
