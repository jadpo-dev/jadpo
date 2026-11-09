# `rate_limits.window_unsupported`

A rate limit counted in Cloudflare's binding has a window other than 10 or 60 seconds.

## Why Jadpo refuses it

Cloudflare's rate-limiting binding counts only windows of 10 or 60 seconds, so
a limit over any other window can't be counted in it. Jadpo refuses it where
you wrote it, because the only alternative is quietly counting a different
window from the one you wrote.

## What it looks like

```jadpo check project=e-rate-limits-window-unsupported expect=rate_limits.window_unsupported file=app.jadpo
application {
    apis: {
        main: {
            prefix: /
            rate_limit: {
                requests: 5
                within: 1h
            }
        }
    }
    connections: {
        limits: {
            driver: Driver.cloudflare_rate_limits
            namespace: 1001
        }
    }
    rate_limits: {
        store: limits
    }
}
```

```text output
error: `within: 1h` can't be counted in Cloudflare's rate-limiting binding
 --> app.jadpo:7:25
   |
 7 |                 within: 1h
   |                         ^^ not 10 or 60 seconds
18 |         store: limits
   |                ------ counted in the binding
  reason: Cloudflare's rate-limiting binding counts only windows of 10 or 60 seconds. A Redis connection can count any window, but until Redis's driver comes each of a Worker's instances would count it in its own memory, which is the judgement call `rate_limits.counted_in_memory`.
  next: Write `within: 1m` or `within: 10s`, with `requests:` to suit, or name a Redis connection in `rate_limits: { store: }` (agent)
  rule: rate_limits.window_unsupported
  help: https://jadpo.dev/docs/errors/rate_limits.window_unsupported

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If a shorter window will do**, write a window the binding counts, with
`requests:` to suit:

```jadpo fragment
            rate_limit: {
                requests: 5
                within: 1m
            }
```

```jadpo check project=e-rate-limits-window-unsupported-fix file=app.jadpo hidden
application {
    apis: {
        main: {
            prefix: /
            rate_limit: {
                requests: 5
                within: 1m
            }
        }
    }
    connections: {
        limits: {
            driver: Driver.cloudflare_rate_limits
            namespace: 1001
        }
    }
    rate_limits: {
        store: limits
    }
}
```

The API now allows five requests a minute. A Redis store can count any
window, but on Workers it raises
[`rate_limits.counted_in_memory`](rate_limits.counted_in_memory.md).

## Related

- [Cloudflare's binding](../language/16-built-in-services.md#cloudflares-binding-drivercloudflare_rate_limits)
- [Rate limits on Workers](../language/16-built-in-services.md#rate-limits-on-workers)
