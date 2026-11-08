# `semantic.unknown_callee`

A call names something the code can't reach by that name.

## Why Jadpo refuses it

A call has to lead to one declaration: a function, action, query or
fixture, a library's function, a standard-library operation or an entity's
operation. A name that leads nowhere is most often a misspelling or
something not written yet, and Jadpo reports it when you check rather than
leave it to fail when the code runs.

## What it looks like

main.jadpo:

```jadpo check project=e-unknown-callee expect=semantic.unknown_callee
type Count = Int {
    min: 0
}

function f(x: Count) -> Count {
    return lookup(x)
}
```

```text output
error: `lookup` isn't a function, action or query
 --> main.jadpo:6:12
  |
6 |     return lookup(x)
  |            ^^^^^^ not declared
  reason: Nothing in this project called `lookup` can be called.
  next: Declare `lookup`, or call an existing function (agent)
  rule: semantic.unknown_callee
  help: https://jadpo.dev/docs/errors/semantic.unknown_callee

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A name close to one that exists is refused with the edit to that one:

```jadpo check project=e-unknown-callee-near expect=semantic.unknown_callee
type Count = Int {
    min: 0
}

function doubled(count: Count) -> Count {
    return Count(count * 2)
}

function twice(count: Count) -> Count {
    return doubld(count)
}
```

```text output
error: `doubld` isn't a function, action or query
 --> main.jadpo:10:12
   |
10 |     return doubld(count)
   |            ^^^^^^ not declared
  reason: Nothing in this project called `doubld` can be called. Did you mean `doubled`?
  next: Use `doubled` instead of `doubld` (agent)
  rule: semantic.unknown_callee
  help: https://jadpo.dev/docs/errors/semantic.unknown_callee

check failed: 1 error, 1 file
```

Another domain's action is called through that domain, so the call says
where it goes:

```jadpo check project=e-unknown-callee-domain file=app.jadpo hidden
application {}
```

```jadpo check project=e-unknown-callee-domain file=domains/planning/actions/snooze_todo.jadpo hidden
public action snooze_todo() -> Unit {}
```

```jadpo check project=e-unknown-callee-domain file=domains/billing/actions/tidy_up.jadpo expect=semantic.unknown_callee
action tidy_up() -> Unit {
    snooze_todo()
}
```

```text output
error: `snooze_todo` belongs to domain `planning`
 --> domains/billing/actions/tidy_up.jadpo:2:5
  |
2 |     snooze_todo()
  |     ^^^^^^^^^^^ needs its domain
  reason: Your own domain's actions, queries and fixtures are called bare; another domain's are called through it, so each call says which domain it reaches.
  next: Call `planning.snooze_todo(...)` (agent)
  rule: semantic.unknown_callee
  help: https://jadpo.dev/docs/errors/semantic.unknown_callee

check failed: 1 error, 3 files
```

An entity has only the operations the language generates for it:

```jadpo check project=e-unknown-callee-entity expect=semantic.unknown_callee
type Title = Text {
    max_length: 40
}

entity Note {
    id: Uuid
    title: Title
}

action archive_note(note_id: Note.id) -> Unit {
    Note.archive(note_id)
}
```

```text output
error: `Note` has no operation `archive`
 --> main.jadpo:11:10
   |
11 |     Note.archive(note_id)
   |          ^^^^^^^ not an operation
  reason: An entity's operations are `get`, `create`, `update` and `delete`, with one more for each transition its lifecycle names.
  next: Call one of `Note`'s operations (agent)
  rule: semantic.unknown_callee
  help: https://jadpo.dev/docs/errors/semantic.unknown_callee

check failed: 1 error, 1 file
```

## How to fix it

**If the name is misspelt**, write the name it meant:

```jadpo fragment
function twice(count: Count) -> Count {
    return doubled(count)
}
```

```jadpo check project=e-unknown-callee-fix-1 hidden
type Count = Int {
    min: 0
}

function doubled(count: Count) -> Count {
    return Count(count * 2)
}

function twice(count: Count) -> Count {
    return doubled(count)
}
```

`twice` now calls `doubled`.

**If it belongs to another domain**, call it through that domain:

```jadpo fragment
use planning

action tidy_up() -> Unit {
    planning.snooze_todo()
}
```

```jadpo check project=e-unknown-callee-fix-2 file=domains/billing/actions/tidy_up.jadpo hidden
use planning

action tidy_up() -> Unit {
    planning.snooze_todo()
}
```

```jadpo check project=e-unknown-callee-fix-2 file=app.jadpo hidden
application {}
```

```jadpo check project=e-unknown-callee-fix-2 file=domains/planning/actions/snooze_todo.jadpo hidden
public action snooze_todo() -> Unit {}
```

The call now says which domain it reaches.

## Related

- [Calling](../language/03-logic.md#calling)
- [Calls name their owner](../language/01-project-layout-and-modules.md#calls-name-their-owner)
