# `type.argument_empty`

A call passes text that may be empty where the operation never takes empty text, such as `text.split`'s `separator`.

## Why Jadpo refuses it

`text.replace`'s `find` and `text.split`'s `separator` are never empty.
Searching for nothing, or splitting between every character, is
something other languages each answer differently, so Jadpo rules it
out while compiling: a literal there isn't `""`, and a value's type has a
`min_length` of at least 1. A type that allows empty text could hand the
call an empty value when the app runs.

## What it looks like

main.jadpo:

```jadpo check project=e-argument-empty expect=type.argument_empty
type Ingredients = Text {
    max_length: 400
}

type Divider = Text {
    max_length: 3
}

type IngredientCount = Int {
    min: 0
}

/// How many ingredients a recipe line lists, split where the cook says.
function ingredient_count(line: Ingredients, spacer: Divider) -> IngredientCount {
    return IngredientCount(list.count(text.split(line, spacer)))
}
```

```text output
error: `spacer` may be empty
 --> main.jadpo:15:56
   |
15 |     return IngredientCount(list.count(text.split(line, spacer)))
   |                                                        ^^^^^^ `Divider` can be empty
  reason: `text.split`'s `separator` is never empty, which is proved while compiling: a literal isn't `""` and a value's type has a `min_length` of at least 1; `Divider` allows empty text.
  next: Pass a value whose type has `min_length: 1`, building it where empty text is handled (agent)
  rule: type.argument_empty
  help: https://jadpo.dev/docs/errors/type.argument_empty

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the cook always chooses a separator**, give its type a
`min_length` of 1:

```jadpo fragment
type Divider = Text {
    min_length: 1
    max_length: 3
}
```

```jadpo check project=e-argument-empty-fix-1 hidden
type Ingredients = Text {
    max_length: 400
}

type Divider = Text {
    min_length: 1
    max_length: 3
}

type IngredientCount = Int {
    min: 0
}

/// How many ingredients a recipe line lists, split where the cook says.
function ingredient_count(line: Ingredients, spacer: Divider) -> IngredientCount {
    return IngredientCount(list.count(text.split(line, spacer)))
}
```

A `Divider` is now at least one character, so building one from empty
text fails where the text comes in, before the split is reached.

**If the separator is always the same**, write it as a literal:

```jadpo fragment
function ingredient_count(line: Ingredients) -> IngredientCount {
    return IngredientCount(list.count(text.split(line, ",")))
}
```

```jadpo check project=e-argument-empty-fix-2 hidden
type Ingredients = Text {
    max_length: 400
}

type IngredientCount = Int {
    min: 0
}

/// How many ingredients a recipe line lists, split at each comma.
function ingredient_count(line: Ingredients) -> IngredientCount {
    return IngredientCount(list.count(text.split(line, ",")))
}
```

Every line is split at its commas, and callers no longer pass a
separator.

## Related

- [Arguments checked while compiling](../language/19-standard-library.md#arguments-checked-while-compiling)
- [Constraints](../language/02-values-and-types.md#constraints)
