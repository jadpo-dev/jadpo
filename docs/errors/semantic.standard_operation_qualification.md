# `semantic.standard_operation_qualification`

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
