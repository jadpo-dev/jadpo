# `cli.traces_arguments`

A `jadpo traces` command is given something it doesn't take, or a filter it can't read.

## Why Jadpo refuses it

`jadpo traces` reads what `jadpo dev` keeps of your app's traces and
chooses requests by its filters. A filter it can't read would choose
nothing, or everything, without saying so.

## What it looks like

`--slower-than` is given a number with no unit:

```jadpo check project=e-traces-arguments file=app.jadpo hidden
application {}
```

```sh run project=e-traces-arguments expect=cli.traces_arguments
jadpo traces --slower-than 200
```

```text output
error: `200` isn't a duration
  reason: `--slower-than` takes a duration, as Jadpo writes one: `200ms`, `2s`, `1m`.
  next: Give a duration such as `--slower-than 200ms` (agent)
  rule: cli.traces_arguments
  help: https://jadpo.dev/docs/errors/cli.traces_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you meant milliseconds**, give the duration as Jadpo writes one, with its unit:

```sh
jadpo traces --slower-than 200ms
```

It now lists only the requests that took longer than 200 ms.

## Related

- [Traces, metrics and logs in development](../language/17-debugging-and-review.md#traces-metrics-and-logs-in-development)
- [`jadpo traces`, `jadpo metrics` and `jadpo logs`](../language/14-tooling.md#jadpo-traces-jadpo-metrics-and-jadpo-logs)
