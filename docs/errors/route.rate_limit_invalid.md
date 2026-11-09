# `route.rate_limit_invalid`

A `rate_limit:` doesn't say how many requests over how long, or says it wrongly.

## Why Jadpo refuses it

A rate limit is a promise about how many requests a route takes, so it has to
say both how many and over how long, each in a form the runtime can count. A
longer allowance than a day isn't a rate limit but a plan's limit, which is the
application's data.

## What it looks like

main.jadpo:

```jadpo check project=e-route-rate-limit-invalid expect=route.rate_limit_invalid
type MemoView = Output {
    text: Text
}

action keep_memo() -> MemoView {
    return MemoView {
        text: "kept"
    }
}

policy {
    actions: {
        keep_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        rate_limit: {
            requests: 5
            within: 30d
        }
        output: MemoView
        run: keep_memo()
    }
}
```

```text output
error: `30d` can't be a limit's `within:`
 --> main.jadpo:24:21
   |
24 |             within: 30d
   |                     ^^^ not whole seconds from 1s to 24h
  reason: A rate limit says how many requests (`requests:`, a whole number of at least 1) over how long (`within:`, whole seconds from `1s` to `24h`), and what it counts them by (`by:`, `RateLimitKey.caller`, the default, or `RateLimitKey.ip`). A longer allowance is a plan's limit, which is the application's data.
  next: Write a duration such as `within: 1m` (agent)
  rule: route.rate_limit_invalid
  help: https://jadpo.dev/docs/errors/route.rate_limit_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`requests:` is a whole number of at least 1:

```jadpo check project=e-route-rate-limit-count expect=route.rate_limit_invalid
type MemoView = Output {
    text: Text
}

action keep_memo() -> MemoView {
    return MemoView {
        text: "kept"
    }
}

policy {
    actions: {
        keep_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        rate_limit: {
            requests: 0
            within: 1m
        }
        output: MemoView
        run: keep_memo()
    }
}
```

```text output
error: `0` can't be a limit's `requests:`
 --> main.jadpo:23:23
   |
23 |             requests: 0
   |                       ^ not a whole number of at least 1
  reason: A rate limit says how many requests (`requests:`, a whole number of at least 1) over how long (`within:`, whole seconds from `1s` to `24h`), and what it counts them by (`by:`, `RateLimitKey.caller`, the default, or `RateLimitKey.ip`). A longer allowance is a plan's limit, which is the application's data.
  next: Write a whole number of requests, such as `requests: 100` (agent)
  rule: route.rate_limit_invalid
  help: https://jadpo.dev/docs/errors/route.rate_limit_invalid

check failed: 1 error, 1 file
```

A limit says both how many and over how long:

```jadpo check project=e-route-rate-limit-missing expect=route.rate_limit_invalid
type MemoView = Output {
    text: Text
}

action keep_memo() -> MemoView {
    return MemoView {
        text: "kept"
    }
}

policy {
    actions: {
        keep_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        rate_limit: {
            requests: 5
        }
        output: MemoView
        run: keep_memo()
    }
}
```

```text output
error: `rate_limit:` says no `within:`
 --> main.jadpo:22:9
   |
22 |         rate_limit: {
   |         ^^^^^^^^^^^^^ no `within:`
  reason: A rate limit says how many requests (`requests:`, a whole number of at least 1) over how long (`within:`, whole seconds from `1s` to `24h`), and what it counts them by (`by:`, `RateLimitKey.caller`, the default, or `RateLimitKey.ip`). A longer allowance is a plan's limit, which is the application's data.
  next: Add `within: 1m`, the window it counts them over (agent)
  rule: route.rate_limit_invalid
  help: https://jadpo.dev/docs/errors/route.rate_limit_invalid

check failed: 1 error, 1 file
```

`by:` takes a `RateLimitKey`:

```jadpo check project=e-route-rate-limit-key expect=route.rate_limit_invalid
type MemoView = Output {
    text: Text
}

action keep_memo() -> MemoView {
    return MemoView {
        text: "kept"
    }
}

policy {
    actions: {
        keep_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        rate_limit: {
            requests: 5
            within: 1m
            by: ip
        }
        output: MemoView
        run: keep_memo()
    }
}
```

```text output
error: `ip` isn't a `RateLimitKey`
 --> main.jadpo:25:17
   |
25 |             by: ip
   |                 ^^ not a `RateLimitKey`
  reason: A rate limit says how many requests (`requests:`, a whole number of at least 1) over how long (`within:`, whole seconds from `1s` to `24h`), and what it counts them by (`by:`, `RateLimitKey.caller`, the default, or `RateLimitKey.ip`). A longer allowance is a plan's limit, which is the application's data.
  next: Write `by: RateLimitKey.ip` or `by: RateLimitKey.caller` (agent)
  rule: route.rate_limit_invalid
  help: https://jadpo.dev/docs/errors/route.rate_limit_invalid

check failed: 1 error, 1 file
```

## How to fix it

**If the limit is about requests over a short time**, write it in whole seconds
from `1s` to `24h`:

```jadpo fragment
rate_limit: {
    requests: 5
    within: 1m
}
```

```jadpo check project=e-route-rate-limit-fixed hidden
type MemoView = Output {
    text: Text
}

action keep_memo() -> MemoView {
    return MemoView {
        text: "kept"
    }
}

policy {
    actions: {
        keep_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        rate_limit: {
            requests: 5
            within: 1m
        }
        output: MemoView
        run: keep_memo()
    }
}
```

The route now takes 5 requests a minute from each caller, and answers 429
to more.

## Related

- [Rate limits](../language/16-built-in-services.md#rate-limits)
