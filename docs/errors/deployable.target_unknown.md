# `deployable.target_unknown`

A deployable's `target:` isn't one Jadpo builds for.

## Why Jadpo refuses it

`target:` decides what a deployable is compiled into and what it may reach.
Jadpo builds for a server, `Target.native`, and for WebAssembly on
Cloudflare Workers, `Target.wasm`. Workers is where WebAssembly runs, so it
isn't a target of its own.

## What it looks like

```jadpo check project=e-deployable-target-unknown expect=deployable.target_unknown file=app.jadpo
application {
    deployables: {
        kiosk: {
            serves: [main]
            target: Target.workers
        }
    }
}
```

```text output
error: `Target.workers` isn't a target
 --> app.jadpo:5:21
  |
5 |             target: Target.workers
  |                     ^^^^^^^^^^^^^^ not a `Target`
  reason: A deployable is built for `Target.native`, a server, or `Target.wasm`, WebAssembly on Cloudflare Workers. Did you mean `Target.wasm`?
  next: Use `Target.wasm` instead (agent)
  rule: deployable.target_unknown
  help: https://jadpo.dev/docs/errors/deployable.target_unknown

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If it runs on Cloudflare Workers**, build it for WebAssembly, with a store
for its rate limits:

```jadpo check project=e-deployable-target-wasm file=app.jadpo
application {
    connections: {
        limits: {
            driver: Driver.cloudflare_rate_limits
            namespace: 1001
        }
    }
    deployables: {
        kiosk: {
            serves: [main]
            target: Target.wasm
        }
    }
    rate_limits: {
        store: limits
    }
}
```

`jadpo build` now writes `kiosk` as a WebAssembly module, ready for
`wrangler deploy`.

**If it runs on a server**, build it natively:

```jadpo check project=e-deployable-target-native file=app.jadpo
application {
    deployables: {
        kiosk: {
            serves: [main]
            target: Target.native
        }
    }
}
```

`jadpo build` now writes `kiosk` as a server binary.

## Related

- [Declared deployables](../language/14-tooling.md#declared-deployables)
- [A Workers deployable](../language/14-tooling.md#a-workers-deployable)
- [A native deployable](../language/14-tooling.md#a-native-deployable)
