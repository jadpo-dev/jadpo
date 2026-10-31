# `type.ordering_operand`

A comparison such as `<` or `>` is used on a value that has no order, such as text or a value that may be `none`.

## Why Jadpo refuses it

`<`, `<=`, `>` and `>=` put two values in order, so they work only where
an order means something: numbers and time values of one named type. A
value that may be `none` has no place in an order, and Jadpo won't guess
whether `none` comes first or last. Text has no order either, because
the order depends on the reader's language.

## What it looks like

main.jadpo:

```jadpo check project=e-ordering expect=type.ordering_operand
type Count = Int {}

type Flag = Bool {}

function over_maximum(count: Count?, maximum: Count) -> Flag {
    return Flag(count > maximum)
}
```

```text output
error: `count` may be `none`, so it can't be ordered
 --> main.jadpo:6:17
  |
6 |     return Flag(count > maximum)
  |                 ^^^^^ may be `none`
  reason: `count` is `Count?`; `<`, `<=`, `>` and `>=` order two values, and `none` has no place in an order.
  next: Match on `count` and compare inside `some(...)` (agent)
  rule: type.ordering_operand
  help: https://jadpo.dev/docs/errors/type.ordering_operand

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Comparing two pieces of text with `<` is the same error:

main.jadpo:

```jadpo check project=e-ordering-text expect=type.ordering_operand
type Name = Text {}

type Flag = Bool {}

function earlier(name: Name, other: Name) -> Flag {
    return Flag(name < other)
}
```

```text output
error: `Name` has no order
 --> main.jadpo:6:17
  |
6 |     return Flag(name < other)
  |                 ^^^^ this is `Name`
  reason: Text order depends on the reader's language, so `<`, `<=`, `>` and `>=` work only on numbers and time values.
  next: Compare the text with `==`, or order it with a standard-library function (agent)
  rule: type.ordering_operand
  help: https://jadpo.dev/docs/errors/type.ordering_operand

check failed: 1 error, 1 file
```

## How to fix it

**If the value may be `none`**, `match` on it, compare inside
`some(...)`, and say what the answer is when there is no value:

```jadpo fragment
    return match count {
        some(known) -> Flag(known > maximum)
        none -> false
    }
```

```jadpo check project=e-ordering-fix hidden
type Count = Int {}

type Flag = Bool {}

function over_maximum(count: Count?, maximum: Count) -> Flag {
    return match count {
        some(known) -> Flag(known > maximum)
        none -> false
    }
}
```

A count that is there is compared with the maximum, and a missing count
is never over it.

**If you are putting text in order**, sort a list of it with
`list.sort_by`, giving the text as the key. `main.jadpo` becomes:

```jadpo check project=e-ordering-text-fix
type Name = Text {}

function in_order(names: List<Name>) -> List<Name> {
    return list.sort_by(names, name -> name)
}
```

The names come back sorted by their code points, the same order a query
sorts text in. Given `locale:`, `list.sort_by` orders text by a
language's rules instead ([Lists](../language/19-standard-library.md#lists)).
To check that two pieces of text are the same, compare them with `==`.

## Related

- [Expressions and operators](../language/03-logic.md#expressions-and-operators)
- [Values that may be missing](../language/02-values-and-types.md#values-that-may-be-missing)
- [Lists](../language/19-standard-library.md#lists)
