# `layout.unused_declaration`

```jadpo check project=e-unused file=app.jadpo hidden
application {}
```

```jadpo check project=e-unused file=domains/planning/types/due_window.jadpo expect=layout.unused_declaration
type DueWindow = Int {
    min: 1
}
```

```text output
error: `DueWindow` isn't used
 --> domains/planning/types/due_window.jadpo:1:6
  |
1 | type DueWindow = Int {
  |      ^^^^^^^^^ nothing uses this
  reason: A shared type exists to be used by several files; one that nothing uses is dead code. Tests don't count as users.
  next: Remove `DueWindow` (agent)
  rule: layout.unused_declaration
  help: docs/language/01-project-layout-and-modules.html#layout-unused-declaration

check failed: 1 error, 2 files
```

Beside a file's primary declaration, a supporting type or helper is there
for it: the primary uses it, directly or through another supporting
declaration it uses. One only the file's tests use is dead code too:

```jadpo check project=e-unused-beside file=app.jadpo hidden
application {}
```

```jadpo check project=e-unused-beside file=domains/kitchen/actions/set_oven.jadpo expect=layout.unused_declaration
type Shelf = Int {
    min: -2
    max: 2
}

type OvenTemperature = Int {
    min: 0
    max: 300
}

function top_shelf() -> Shelf {
    return 2
}

/// The temperature for a bake on `shelf`: a high shelf runs hot.
action set_oven(shelf: Shelf) -> OvenTemperature {
    if (shelf > 0) {
        return 190
    }
    return 200
}
```

```text output
error: `top_shelf` isn't used by `set_oven`
 --> domains/kitchen/actions/set_oven.jadpo:11:10
   |
11 | function top_shelf() -> Shelf {
   |          ^^^^^^^^^ `set_oven` doesn't use this
  reason: Everything in a file besides the declaration it is named after is there for that declaration, used by it directly or through one another; anything else is dead code. Tests don't count as users.
  next: Remove `top_shelf` (agent)
  rule: layout.unused_declaration
  help: docs/language/01-project-layout-and-modules.html#layout-unused-declaration

check failed: 1 error, 3 files
```

The file's test calls `top_shelf`, which doesn't count:

```jadpo check project=e-unused-beside file=domains/kitchen/actions/set_oven.test.jadpo
test "the top shelf is turned down" {
    assert set_oven(top_shelf()) == 190
}
```
