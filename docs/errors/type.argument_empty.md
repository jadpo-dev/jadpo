# `type.argument_empty`

Text an operation never takes empty (`never_empty`): `text.replace`'s
`find` and `text.split`'s `separator`. A literal isn't `""` and a value's
type has a `min_length` of at least 1, so a search never looks for
nothing and a split never cuts between every character, which other
languages each do differently.

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
