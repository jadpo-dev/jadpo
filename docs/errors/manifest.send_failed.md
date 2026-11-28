# `manifest.send_failed`

`jadpo manifest send` had nowhere to send a build's manifest, couldn't reach the endpoint, or was refused.

## Why Jadpo refuses it

A monitoring product decodes your app's telemetry against the manifest of
the build that sent it, which `jadpo manifest send` gives it as you
deploy, through [the manifest
protocol](../language/16-built-in-services.md#the-manifest-protocol). It
goes to the endpoint the app's `telemetry:` section names, with its
credential. When there is no section, no variable gives the endpoint, the
product can't be reached or it refuses the manifest, the command exits 1
so the deploy stops before a build goes out that the product can't
decode.

Jadpo won't send the manifest anywhere else: where your app's telemetry
goes is yours to decide.

## What it looks like

An app that sends no telemetry:

```jadpo check project=e-manifest-send-failed file=app.jadpo hidden
application {}
```

```sh run project=e-manifest-send-failed expect=manifest.send_failed
jadpo check > /dev/null
jadpo manifest send
```

```text output
error: The app sends no telemetry, so its manifest has nowhere to go
  reason: A build's manifest goes to the endpoint the app's `telemetry:` section names, read with its credential, and `app.jadpo` has no `telemetry:` section.
  next: Add a `telemetry:` section to `application { }`, naming where the app's telemetry goes (human)
  rule: manifest.send_failed
  help: https://jadpo.dev/docs/errors/manifest.send_failed
```

`(human)` at the end of the `next:` line tells your coding agent to stop and ask you which fix you want, because the choice is yours to make: it decides where your app's telemetry goes.

## How to fix it

The question is whether the app should send telemetry at all.

**If it should**, add a `telemetry:` section naming the collector, set
its endpoint as the deployment does, and send again:

```jadpo check project=e-manifest-send-failed-fixed file=app.jadpo
application {
    telemetry: {
        endpoint: config.telemetry_endpoint
    }
}
```

```jadpo check project=e-manifest-send-failed-fixed file=config.jadpo hidden
config {
    telemetry_endpoint: Url
}
```

```sh
TELEMETRY_ENDPOINT=https://collector.example jadpo manifest send
```

Each deployable's manifest is sent, and sending again is safe: the
product keeps the first it was sent.

**If it shouldn't**, leave `jadpo manifest send` out of the deploy.

## Related

- [The manifest protocol](../language/16-built-in-services.md#the-manifest-protocol)
- [Telemetry](../language/16-built-in-services.md#telemetry)
- [`jadpo manifest`](../language/14-tooling.md#jadpo-manifest)
