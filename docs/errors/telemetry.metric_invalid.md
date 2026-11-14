# `telemetry.metric_invalid`

A `metric` isn't in a domain's `monitoring.jadpo`, isn't a `Counter`, `Measure` or `Gauge`, or doesn't list its labels by name.

## Why Jadpo refuses it

A domain's metrics are declared in its `monitoring.jadpo`, beside its
thresholds, so a reviewer reads everything that watches the domain in
one place. Each is one of three kinds the monitoring service understands,
and its `by:` names the labels every value carries, so its series can be
added up. Anything else would send values no tool could read the same
way twice.

## What it looks like

`visits` is declared a `Count`, which isn't a kind of metric:

```jadpo check project=e-telemetry-metric-invalid expect=telemetry.metric_invalid
/// Visits, by whether they came back.
metric visits = Count {
    by: [returning]
}
```

```text output
error: `Count` isn't a kind of metric
 --> main.jadpo:2:17
  |
2 | metric visits = Count {
  |                 ^^^^^ not `Counter`, `Measure` or `Gauge`
  reason: A metric is declared in a domain's `monitoring.jadpo` as `metric name = Counter { by: [label] }`: a `Counter`, a count that only rises, a `Measure`, a distribution, or a `Gauge`, a value at a moment. `by:` names its labels, which code gives as enums or `Bool`, so the number of series stays bounded.
  next: Use `Counter` instead of `Count` (agent)
  rule: telemetry.metric_invalid
  help: https://jadpo.dev/docs/errors/telemetry.metric_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If it counts something**, declare it a `Counter`, the kind for a
count that only rises:

```jadpo fragment
metric visits = Counter {
    by: [returning]
}
```

```jadpo check project=e-telemetry-metric-invalid-fix hidden
/// Visits, by whether they came back.
metric visits = Counter {
    by: [returning]
}
```

`visits` is now a count each visit adds to, by whether it came back.

## Related

- [Custom metrics](../language/16-built-in-services.md#custom-metrics)
- [`telemetry.record_invalid`](telemetry.record_invalid.md)
