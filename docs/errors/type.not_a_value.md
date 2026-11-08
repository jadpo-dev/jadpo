# `type.not_a_value`

A type, function or other declaration is named where a value is read, as in `var placed: Placed = Placed`.

## Why Jadpo refuses it

No declaration's name is a value. A type says what a value looks like
and holds none itself, and functions, actions and queries are called, not
passed around. A name written where a value is read is usually a value
the code forgot to build, or a call missing its brackets. The `next:` line gives the
step that makes a value from the name.

## What it looks like

main.jadpo:

```jadpo check project=e-not-a-value expect=type.not_a_value
type Count = Int {
    min: 0
}

type Placed = Object {
    count: Count
}

function first_placed() -> Placed {
    var placed: Placed = Placed
    return placed
}
```

```text output
error: `Placed` is a type, not a value
 --> main.jadpo:10:26
   |
10 |     var placed: Placed = Placed
   |                          ^^^^^^ a type
  reason: A type says what a value looks like and holds none itself; a value is built with the type's name and every field in a block.
  next: Build a value with `Placed { ... }`, giving each field (agent)
  rule: type.not_a_value
  help: https://jadpo.dev/docs/errors/type.not_a_value

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An operation read without a call is the same error:

main.jadpo:

```jadpo check project=e-not-a-value-call expect=type.not_a_value
type Count = Int {
    min: 0
}

function counted(items: List<Count>) -> Count {
    var how_many: Count = Count(list.count)
    return how_many
}
```

```text output
error: `list.count` is called, not a value
 --> main.jadpo:6:33
  |
6 |     var how_many: Count = Count(list.count)
  |                                 ^^^^^^^^^^ to call
  reason: What a namespace, domain, library or connection holds is an operation, function, action or query; each is called, and none is a value.
  next: Call it, as in `list.count(...)` (agent)
  rule: type.not_a_value
  help: https://jadpo.dev/docs/errors/type.not_a_value

check failed: 1 error, 1 file
```

Its fix is to call it, `list.count(items)`.

## How to fix it

**If you meant a value of the type**, build one, giving each field:

```jadpo fragment
function first_placed() -> Placed {
    var placed: Placed = Placed {
        count: 0
    }
    return placed
}
```

```jadpo check project=e-not-a-value-fix hidden
type Count = Int {
    min: 0
}

type Placed = Object {
    count: Count
}

function first_placed() -> Placed {
    var placed: Placed = Placed {
        count: 0
    }
    return placed
}
```

`first_placed` gives a `Placed` whose count is 0.

## Related

- [Names that aren't values](../language/03-logic.md#names-that-arent-values)
- [Creating values](../language/02-values-and-types.md#creating-values)
