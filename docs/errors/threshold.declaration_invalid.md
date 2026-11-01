# `threshold.declaration_invalid`

A threshold in `thresholds { }` doesn't watch one thing, gives a limit that doesn't apply to it, or gives a value a limit can't take.

## Why Jadpo refuses it

A threshold watches one route, action, query, schedule or subscriber,
and each of its limits is an upper bound on something that one thing
does. A limit that can't apply, such as the age of the oldest waiting
delivery on an action, which has no deliveries, would generate a rule
that never fires; a value of the wrong kind has no bound to judge.
Thresholds live in a domain's `monitoring.jadpo` so a reviewer reads
every one that watches the domain in one place.

## What it looks like

`counting` gives `oldest_waiting:`, which only a subscriber's
deliveries have:

```jadpo check project=e-threshold-declaration expect=threshold.declaration_invalid
/// How many visitors have come.
type VisitorCount = Int {
    min: 0
}

/// How many visitors have come so far.
action count_visitors() -> VisitorCount {
    return 1
}

thresholds {
    /// Counting the visitors stays quick.
    counting: {
        watches: count_visitors
        oldest_waiting: 5m
    }
}
```

```text output
error: `oldest_waiting:` can't watch an action
 --> main.jadpo:15:25
   |
15 |         oldest_waiting: 5m
   |                         ^^ not for an action
  reason: `oldest_waiting:` is the age a subscriber's oldest waiting delivery may reach, a duration above zero. It watches a subscriber.
  next: Remove `oldest_waiting:` from `counting` (agent)
  rule: threshold.declaration_invalid
  help: https://jadpo.dev/docs/errors/threshold.declaration_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A percentile shorter than a lower one beside it is refused too, since
no request could meet it:

```jadpo check project=e-threshold-declaration-order expect=threshold.declaration_invalid
/// How many visitors have come.
type VisitorCount = Int {
    min: 0
}

/// How many visitors have come so far.
action count_visitors() -> VisitorCount {
    return 1
}

thresholds {
    /// Counting the visitors stays quick.
    counting: {
        watches: count_visitors
        p50: 80ms
        p99: 50ms
    }
}
```

```text output
error: `p99:` on `counting` is shorter than its `p50:`
 --> main.jadpo:16:14
   |
16 |         p99: 50ms
   |              ^^^^ shorter than a lower percentile
  reason: A percentile counts the requests done within its time, so a higher one takes at least as long as a lower one: no `p99:` can be met while a shorter one beside it isn't.
  next: Make `p99:` at least `p50:`'s, or remove one (agent)
  rule: threshold.declaration_invalid
  help: https://jadpo.dev/docs/errors/threshold.declaration_invalid

check failed: 1 error, 1 file
```

## How to fix it

**If the limit doesn't apply**, give one that does. An action's runs
have a duration, a share that fails on the server and percentiles:

```jadpo fragment
    counting: {
        watches: count_visitors
        p99: 50ms
    }
```

```jadpo check project=e-threshold-declaration-fix-1 hidden
/// How many visitors have come.
type VisitorCount = Int {
    min: 0
}

/// How many visitors have come so far.
action count_visitors() -> VisitorCount {
    return 1
}

thresholds {
    /// Counting the visitors stays quick.
    counting: {
        watches: count_visitors
        p99: 50ms
    }
}
```

`counting` now judges the action's runs by how long 99% of them take,
which the rules made from it alert on.

**If the percentiles are the wrong way round**, make each higher one at
least as long as the one below it:

```jadpo fragment
        p50: 30ms
        p99: 50ms
```

```jadpo check project=e-threshold-declaration-fix-2 hidden
/// How many visitors have come.
type VisitorCount = Int {
    min: 0
}

/// How many visitors have come so far.
action count_visitors() -> VisitorCount {
    return 1
}

thresholds {
    /// Counting the visitors stays quick.
    counting: {
        watches: count_visitors
        p50: 30ms
        p99: 50ms
    }
}
```

Half the runs must now finish within 30 ms and 99% within 50 ms, which
both can.

## Related

- [Writing a threshold](../language/16-built-in-services.md#writing-a-threshold)
- [Monitoring thresholds](../language/16-built-in-services.md#monitoring-thresholds)
