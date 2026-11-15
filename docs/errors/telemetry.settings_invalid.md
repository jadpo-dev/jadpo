# `telemetry.settings_invalid`

The application's `telemetry:` section holds something it doesn't take, or a value it can't use.

## Why Jadpo refuses it

`telemetry:` says where your app's traces, metrics and logs go and how
much of each request. A setting it doesn't know would otherwise be
dropped without a word, and a value it can't read, such as an endpoint
written in the source or a key that isn't secret, would send telemetry
somewhere you didn't mean, or put a credential where anyone reading the
code could see it.

## What it looks like

`send:` gives `logs:` as text, where it takes the lowest `LogLevel` sent:

```jadpo check project=e-telemetry-settings-invalid file=app.jadpo expect=telemetry.settings_invalid
application {
    telemetry: {
        send: {
            logs: "warn"
        }
    }
}
```

```text output
error: `send:`'s `logs:` must be a `LogLevel`
 --> app.jadpo:4:19
  |
4 |             logs: "warn"
  |                   ^^^^^^ not a `LogLevel`
  reason: `telemetry:` sends the app's traces, metrics and logs over OpenTelemetry's protocol: `endpoint:` is a `Url` setting, `credential:` a header's name and a secret setting, `send:` which signals go out and `detail:` how much of each request. Leave `endpoint:` and `credential:` out to read `OTEL_EXPORTER_OTLP_ENDPOINT` and `OTEL_EXPORTER_OTLP_HEADERS`.
  next: Write the level logs are sent from, such as `logs: LogLevel.warn` (agent)
  rule: telemetry.settings_invalid
  help: https://jadpo.dev/docs/errors/telemetry.settings_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you meant a level of log**, give the `LogLevel` it is, the lowest
level sent:

```jadpo fragment
        send: {
            logs: LogLevel.warn
        }
```

```jadpo check project=e-telemetry-settings-invalid-fix file=app.jadpo hidden
application {
    telemetry: {
        send: {
            logs: LogLevel.warn
        }
    }
}
```

With no `endpoint:`, the runtime sends to the collector
`OTEL_EXPORTER_OTLP_ENDPOINT` names.

## Related

- [Telemetry](../language/16-built-in-services.md#telemetry)
- [Settings: `config.jadpo`](../language/10-configuration-and-time.md#settings-configjadpo)
