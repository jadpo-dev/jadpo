# `semantic.standard_operation_qualification`

A standard-library operation is called without its namespace.

## Why Jadpo refuses it

The standard library is always called through its namespace, so the dot
says who owns the operation. A bare `in_zone(...)` could otherwise be
mistaken for a function of the project's own.

## What it looks like

main.jadpo:

```jadpo check project=e-std-qualify expect=semantic.standard_operation_qualification
type DueAt = Instant {}

type HomeZone = Zone {}

type DueTime = ZonedTime {}

function local_due(at: DueAt, zone: HomeZone) -> DueTime {
    var local: ZonedTime = in_zone(at, zone)
    return DueTime(local)
}
```

```text output
error: `in_zone` is a standard-library operation
 --> main.jadpo:8:28
  |
8 |     var local: ZonedTime = in_zone(at, zone)
  |                            ^^^^^^^ needs its namespace
  reason: The standard library is always called through its namespace, so the dot says who owns the operation.
  next: Call `temporal.in_zone(...)` (agent)
  rule: semantic.standard_operation_qualification
  help: https://jadpo.dev/docs/errors/semantic.standard_operation_qualification

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the operation is the standard library's**, call it through its
namespace:

```jadpo fragment
function local_due(at: DueAt, zone: HomeZone) -> DueTime {
    var local: ZonedTime = temporal.in_zone(at, zone)
    return DueTime(local)
}
```

```jadpo check project=e-std-qualify-fix hidden
type DueAt = Instant {}

type HomeZone = Zone {}

type DueTime = ZonedTime {}

function local_due(at: DueAt, zone: HomeZone) -> DueTime {
    var local: ZonedTime = temporal.in_zone(at, zone)
    return DueTime(local)
}
```

The call now says which namespace `in_zone` belongs to.

## Related

- [Calls name their owner](../language/01-project-layout-and-modules.md#calls-name-their-owner)
- [All operations](../language/19-standard-library.md#all-operations)
