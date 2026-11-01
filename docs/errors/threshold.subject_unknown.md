# `threshold.subject_unknown`

A threshold watches a route, action, query, schedule or subscriber its domain doesn't have.

## Why Jadpo refuses it

A threshold says when something your app does is too slow or failing
too often, and the rules generated from it alert on that one thing. One
that names nothing would never be judged, so a misspelt name would leave
the route or action it meant unwatched without anyone noticing. A
domain's `monitoring.jadpo` watches only its own domain, so each
domain's file is the whole of what watches it.

## What it looks like

`counting` names an action that isn't declared:

```jadpo check project=e-threshold-subject expect=threshold.subject_unknown
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
        watches: count_visitor
        p95: 50ms
    }
}
```

```text output
error: Nothing called `count_visitor` to watch
 --> main.jadpo:14:18
   |
14 |         watches: count_visitor
   |                  ^^^^^^^^^^^^^ nothing to watch
  reason: `watches:` names an action, query, schedule or subscriber of the threshold's own domain, and the project declares none called `count_visitor`.
  next: Name the action, query, schedule or subscriber of the project that `counting` watches (agent)
  rule: threshold.subject_unknown
  help: https://jadpo.dev/docs/errors/threshold.subject_unknown

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the name is misspelt**, name what the threshold watches:

```jadpo fragment
    counting: {
        watches: count_visitors
        p95: 50ms
    }
```

```jadpo check project=e-threshold-subject-fix-1 hidden
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
        p95: 50ms
    }
}
```

`counting` now watches `count_visitors`, and the rules made from it
alert when that action's 95th percentile passes 50 ms.

A route is named by its method and its path as `jadpo routes` prints it,
as a route test names one: `route: { method: HttpMethod.post path:
"/hello" }`. Another domain's route or action is watched from that
domain's own `monitoring.jadpo`.

## Related

- [Monitoring thresholds](../language/16-built-in-services.md#monitoring-thresholds)
- [Writing a threshold](../language/16-built-in-services.md#writing-a-threshold)
