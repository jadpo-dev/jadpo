# `cli.metrics_arguments`

A `jadpo metrics` command is given something it doesn't take, or a filter it can't read.

## Why Jadpo refuses it

`jadpo metrics` adds up the metrics `jadpo dev` keeps of your app's requests, chosen by its filters. A filter it can't read would choose nothing, or
everything, without saying so.

## What it looks like

`--slower-than` is given a number with no unit:

```jadpo check project=e-metrics-arguments file=app.jadpo hidden
application {}
```

```sh run project=e-metrics-arguments expect=cli.metrics_arguments
jadpo metrics --slower-than 2
```

```text output
error: `2` isn't a duration
  reason: `--slower-than` takes a duration, as Jadpo writes one: `200ms`, `2s`, `1m`.
  next: Give a duration such as `--slower-than 200ms` (agent)
  rule: cli.metrics_arguments
  help: https://jadpo.dev/docs/errors/cli.metrics_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you meant seconds**, give the duration as Jadpo writes one, with its unit:

```sh
jadpo metrics --slower-than 2s
```

It now adds up only the requests that took longer than 2 seconds.

## Related

- [Traces, metrics and logs in development](../language/17-debugging-and-review.md#traces-metrics-and-logs-in-development)
- [`jadpo traces`, `jadpo metrics` and `jadpo logs`](../language/14-tooling.md#jadpo-traces-jadpo-metrics-and-jadpo-logs)
