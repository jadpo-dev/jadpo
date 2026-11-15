# `telemetry.record_invalid`

A `telemetry.` call names a metric its domain doesn't declare, records it the wrong way, or gives a value, label or attribute of a type it can't carry.

## Why Jadpo refuses it

A metric is declared once, with its kind and the labels it is counted
by, so every value sent of it means the same thing and the monitoring
service can add them up. A call that names a metric no one declared, sets
a counter, or leaves out a label would send values that don't fit the
others. A label is an enum or a `Bool`, so the number of series stays
bounded; a note's attribute is text, a number, a `Bool` or an enum.

## What it looks like

`visits` is a `Counter`, which only rises, but `count_return` sets it:

```jadpo check project=e-telemetry-record-invalid expect=telemetry.record_invalid
/// Visits, by whether they came back.
metric visits = Counter {
    by: [returning]
}

/// Counts a visit that came back.
action count_return() {
    telemetry.visits.set(1, returning: true)
}
```

```text output
error: `visits` is a `Counter`, recorded with `add`
 --> main.jadpo:8:5
  |
8 |     telemetry.visits.set(1, returning: true)
  |     ^^^^^^^^^^^^^^^^^^^^ not `add`
  reason: A `Counter` only rises, recorded with `add`; a `Measure` is a distribution, recorded with `record`; a `Gauge` is a value at a moment, recorded with `set`.
  next: Write `telemetry.visits.add(...)` (agent)
  rule: telemetry.record_invalid
  help: https://jadpo.dev/docs/errors/telemetry.record_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If it is a count**, add to it, as a `Counter` is recorded; a `Measure`
records a value of a distribution, and a `Gauge` is set:

```jadpo fragment
    telemetry.visits.add(1, returning: true)
```

```jadpo check project=e-telemetry-record-invalid-fix hidden
/// Visits, by whether they came back.
metric visits = Counter {
    by: [returning]
}

/// Counts a visit that came back.
action count_return() {
    telemetry.visits.add(1, returning: true)
}
```

Each visit now adds one to the count for its label.

## Related

- [Custom metrics](../language/16-built-in-services.md#custom-metrics)
- [`telemetry.metric_invalid`](telemetry.metric_invalid.md)
