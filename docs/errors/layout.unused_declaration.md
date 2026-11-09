# `layout.unused_declaration`

A declaration nothing uses is left in the code.

## Why Jadpo refuses it

A shared declaration exists to be used by several files, and everything else
in a file is there for that file's primary declaration. One that nothing
uses is dead code that a reader has to work through, and it can be mistaken
for something that matters. Tests don't count as users, so code kept alive
only by its tests is refused too.

## What it looks like

Nothing uses the type `DueWindow`.

```jadpo check project=e-unused file=app.jadpo hidden
application {}
```

`domains/planning/types/due_window.jadpo`:

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
  next: Use `DueWindow`, or remove it if nothing will (agent)
  rule: layout.unused_declaration
  help: https://jadpo.dev/docs/errors/layout.unused_declaration

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Beside a file's primary declaration, a supporting type or helper is there
for it: the primary uses it, directly or through another supporting
declaration it uses. One only the file's tests use is dead code too:

```jadpo check project=e-unused-beside file=app.jadpo hidden
application {}
```

`domains/kitchen/actions/set_oven.jadpo`:

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
  next: Use `top_shelf` in `set_oven`, or remove it if nothing will (agent)
  rule: layout.unused_declaration
  help: https://jadpo.dev/docs/errors/layout.unused_declaration

check failed: 1 error, 3 files
```

The file's test calls `top_shelf`, which doesn't count:

`domains/kitchen/actions/set_oven.test.jadpo`:

```jadpo check project=e-unused-beside file=domains/kitchen/actions/set_oven.test.jadpo
test "the top shelf is turned down" {
    assert set_oven(top_shelf()) == 190
}
```

## How to fix it

**If something is about to use it**, write it and its first user as one
change, then check. In `domains/planning/actions/next_due.jadpo`:

```jadpo check project=e-unused-staged file=domains/planning/actions/next_due.jadpo
use planning/types { DueWindow }

/// The window a reminder waits, as it was asked for.
action next_due(days: DueWindow) -> DueWindow {
    return days
}
```

```jadpo check project=e-unused-staged file=app.jadpo hidden
application {}
```

```jadpo check project=e-unused-staged file=domains/planning/types/due_window.jadpo hidden
type DueWindow = Int {
    min: 1
}
```

The type and its first user arrive together, so `jadpo check` never sees
it unused.

**If nothing will use it**, remove it:

```sh
rm domains/planning/types/due_window.jadpo
```

```jadpo check project=e-unused-removed file=app.jadpo hidden
application {}
```

The dead code has gone, and nothing else changes.

## Related

- [One primary declaration per file](../language/01-project-layout-and-modules.md#one-primary-declaration-per-file)
- [Order within a file](../language/01-project-layout-and-modules.md#order-within-a-file)
- [Sharing and promotion](../language/01-project-layout-and-modules.md#sharing-and-promotion)
