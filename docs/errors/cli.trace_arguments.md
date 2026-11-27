# `cli.trace_arguments`

A `jadpo trace` command is given no id, or one no kept timeline has, or the start of several requests' ids.

## Why Jadpo refuses it

`jadpo trace` shows one request `jadpo dev` answered, from the timelines
it keeps on your machine, or one failed test's trace `jadpo test
--keep-failed` kept. An id that names none, or names several, would show
the wrong request, or none, without saying so.

## What it looks like

The id was mistyped, so no request `jadpo dev` kept has it:

```jadpo check project=e-trace-arguments file=app.jadpo hidden
application {}
```

```sh run project=e-trace-arguments expect=cli.trace_arguments
jadpo trace req_0b1x
```

```text output
error: No request `req_0b1x` is kept
  reason: `jadpo dev` keeps the last 500 requests' timelines, and holds none yet.
  next: Run `jadpo traces` to see the requests kept (agent)
  rule: cli.trace_arguments
  help: https://jadpo.dev/docs/errors/cli.trace_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the id was mistyped**, take it from the request's line in `jadpo
dev`, its `x-request-id` header or its error's `request_id`, or list the
requests kept:

```sh
jadpo traces
```

Then give any start of its id that no other request's shares:

```sh
jadpo trace req_0b1e
```

## Related

- [The request timeline in `jadpo dev`](../language/17-debugging-and-review.md#the-request-timeline-in-jadpo-dev)
- [`jadpo trace`](../language/14-tooling.md#jadpo-trace)
