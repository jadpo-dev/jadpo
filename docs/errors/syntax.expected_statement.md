# `syntax.expected_statement`

A body holds something that isn't a Jadpo statement.

## Why Jadpo refuses it

A body is a list of statements, and only the words Jadpo knows can start
one. The only loop is `for item in items`, which always ends after its list,
so a `while` or a `loop` has no meaning here. An `else` needs the `if` it
answers, a fixed value in a body is a `var`, and a `decline` belongs above a
declaration.

## What it looks like

main.jadpo:

```jadpo check project=e-statement expect=syntax.expected_statement
type Count = Int {
    min: 0
}

function first(count: Count) -> Count {
    while (count > 0) {
        return count
    }
    return count
}
```

```text output
error: Jadpo has no `while` loop
 --> main.jadpo:6:5
  |
6 |     while (count > 0) {
  |     ^^^^^ not a Jadpo statement
  reason: The only loop is `for item in items`, which always ends after its list; stop early with `return`.
  next: Loop over a list with `for`, or narrow it first with `list.filter` (agent)
  rule: syntax.expected_statement
  help: https://jadpo.dev/docs/errors/syntax.expected_statement

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An `else` with no `if` before it is this error too:

main.jadpo:

```jadpo check project=e-statement-2 expect=syntax.expected_statement
type Count = Int {
    min: 0
}

type Size = Text {}

function size(count: Count) -> Size {
    else {
        return "small"
    }
    return "large"
}
```

```text output
error: `else` without an `if`
 --> main.jadpo:8:5
  |
8 |     else {
  |     ^^^^ no `if` before this
  reason: `else` follows the `}` that closes an `if` block.
  next: Put `else` right after the `if` block's `}`, or remove it (agent)
  rule: syntax.expected_statement
  help: https://jadpo.dev/docs/errors/syntax.expected_statement

check failed: 1 error, 1 file
```

A `constant` inside a body is this error too, and its `next:` points to
`var`, because a constant lives at the top of a file or in a library:

main.jadpo:

```jadpo check project=e-statement-3 expect=syntax.expected_statement
type Count = Int {
    min: 0
}

function kept(count: Count) -> Count {
    constant copy: Count = count
    return copy
}
```

```text output
error: `constant` doesn't start a statement
 --> main.jadpo:6:5
  |
6 |     constant copy: Count = count
  |     ^^^^^^^^ write `var`
  reason: A constant lives at the top of a file or in a library; inside a body a value that never changes is a `var`.
  next: Use `var` instead of `constant` (agent)
  rule: syntax.expected_statement
  help: https://jadpo.dev/docs/errors/syntax.expected_statement

check failed: 1 error, 1 file
```

A `decline` among a body's statements is this error too, because it stands
directly above the declaration or member whose finding it declines:

main.jadpo:

```jadpo check project=e-statement-4 expect=syntax.expected_statement
type Knots = Int {
    min: 0
}

function kept(count: Knots) -> Knots {
    decline type.unit_as_number: "Knots the scouts can tie, not a speed"
    return count
}
```

```text output
error: A decline can't go inside a body
 --> main.jadpo:6:5
  |
6 |     decline type.unit_as_number: "Knots the scouts can tie, not a speed"
  |     ^^^^^^^ not a statement
  reason: A `decline` line stands directly above the declaration or member whose finding it declines, never among a body's statements.
  next: Move the `decline` line above the declaration the finding is on (agent)
  rule: syntax.expected_statement
  help: https://jadpo.dev/docs/errors/syntax.expected_statement

check failed: 1 error, 1 file
```

## How to fix it

**If the loop runs at most once**, use `if`:

```jadpo fragment
function first(count: Count) -> Count {
    if (count > 0) {
        return count
    }
    return count
}
```

```jadpo check project=e-statement-fix-1 hidden
type Count = Int {
    min: 0
}

function first(count: Count) -> Count {
    if (count > 0) {
        return count
    }
    return count
}
```

`first` now takes the branch only when `count` is above zero.

**If the loop walks a list**, use `for`:

```jadpo fragment
function first(counts: List<Count>) -> Count {
    for count in counts {
        return count
    }
    return 0
}
```

```jadpo check project=e-statement-fix-2 hidden
type Count = Int {
    min: 0
}

function first(counts: List<Count>) -> Count {
    for count in counts {
        return count
    }
    return 0
}
```

`first` now goes through the list and returns its first count, or `0` when
the list is empty.

**If an `else` has no `if`**, put it right after the `if` block's `}`, or
remove it:

```jadpo fragment
function size(count: Count) -> Size {
    if (count > 10) {
        return "large"
    } else {
        return "small"
    }
}
```

```jadpo check project=e-statement-fix-3 hidden
type Count = Int {
    min: 0
}

type Size = Text {}

function size(count: Count) -> Size {
    if (count > 10) {
        return "large"
    } else {
        return "small"
    }
}
```

`size` now returns `"small"` whenever the `if` is false.

**If you meant a fixed value inside the body**, write `var`:

```jadpo fragment
function kept(count: Count) -> Count {
    var copy: Count = count
    return copy
}
```

```jadpo check project=e-statement-fix-4 hidden
type Count = Int {
    min: 0
}

function kept(count: Count) -> Count {
    var copy: Count = count
    return copy
}
```

`copy` now holds the count and never changes.

**If you meant to decline a finding**, move the line directly above the
declaration it is on:

```jadpo check project=e-statement-fix-5
decline type.unit_as_number: "Knots the scouts can tie, not a speed"
type Knots = Int {
    min: 0
}

function kept(count: Knots) -> Knots {
    return count
}
```

The decline now covers `type.unit_as_number` on `Knots`, and the body holds
only its statements.

## Related

- [Statements](../language/03-logic.md#statements)
- [Loops](../language/03-logic.md#loops)
- [Conditions](../language/03-logic.md#conditions)
- [Writing a decline](../language/01-project-layout-and-modules.md#writing-a-decline)
