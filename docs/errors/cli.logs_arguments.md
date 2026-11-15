# `cli.logs_arguments`

A `jadpo logs` command is given something it doesn't take, or a filter or level it can't read.

## Why Jadpo refuses it

`jadpo logs` lists the events `jadpo dev` keeps of your app's logs, chosen by its filters and `--level`. A filter it can't read would choose nothing, or
everything, without saying so.

## What it looks like

`--level` is given a word that isn't one of OpenTelemetry's six levels:

```jadpo check project=e-logs-arguments file=app.jadpo hidden
application {}
```

```sh run project=e-logs-arguments expect=cli.logs_arguments
jadpo logs --level warning
```

```text output
error: `warning` isn't a level
  reason: A log's level is one of OpenTelemetry's six: `trace`, `debug`, `info`, `warn`, `error` or `fatal`.
  next: Give a level such as `--level warn` (agent)
  rule: cli.logs_arguments
  help: https://jadpo.dev/docs/errors/cli.logs_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you meant warnings**, give the level as `LogLevel` names it:

```sh
jadpo logs --level warn
```

It now lists only the events at `warn` or above.

## Related

- [Traces, metrics and logs in development](../language/17-debugging-and-review.md#traces-metrics-and-logs-in-development)
- [`jadpo traces`, `jadpo metrics` and `jadpo logs`](../language/14-tooling.md#jadpo-traces-jadpo-metrics-and-jadpo-logs)
