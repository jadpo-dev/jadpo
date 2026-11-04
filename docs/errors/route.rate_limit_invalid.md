# `route.rate_limit_invalid`

A `rate_limit:` says how many requests over how long. Here it gives a window
longer than a day, which belongs in the application's data as a plan's limit:

```jadpo check project=e-route-rate-limit-invalid expect=route.rate_limit_invalid
type MemoView = Output {
    text: Text
}

action keep_memo() -> MemoView {
    return MemoView {
        text: "kept"
    }
}

routes Memos {
    prefix: /memos

    POST / {
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
 --> main.jadpo:17:21
   |
17 |             within: 30d
   |                     ^^^ not whole seconds from 1s to 24h
  reason: A rate limit says how many requests (`requests:`, a whole number of at least 1) over how long (`within:`, whole seconds from `1s` to `24h`), and what it counts them by (`by:`, `RateLimitKey.caller`, the default, or `RateLimitKey.ip`). A longer allowance is a plan's limit, which is the application's data.
  next: Write a duration such as `within: 1m` (agent)
  rule: route.rate_limit_invalid
  help: https://jadpo.dev/docs/errors/route.rate_limit_invalid

check failed: 1 error, 1 file
```
