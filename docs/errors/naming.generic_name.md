# `naming.generic_name`

A domain or library is called something generic, such as `helpers` or `utils`, that says nothing about what it holds.

## Why Jadpo refuses it

A domain's or library's name appears in every call into it, as in
`rounding.half_even(x)` or `inventory.reserve_stock(...)`. A name that
says what it holds tells the reader what the call does. `common`,
`core`, `helpers`, `misc` and `utils` say nothing, and they collect
unrelated code over time.

## What it looks like

main.jadpo:

```jadpo check project=e-generic expect=naming.generic_name
type Stock = Int {
    min: 0
}

library helpers {
    function add_holdback(subtotal: Stock) -> Stock {
        return subtotal
    }
}
```

```text output
error: `helpers` is too generic for a library
 --> main.jadpo:5:9
  |
5 | library helpers {
  |         ^^^^^^^ says nothing about what it holds
  reason: Domains and libraries are named after what they hold, because the name appears in every call; `common`, `core`, `helpers`, `misc` and `utils` say nothing.
  next: Rename the library after what its functions do (agent)
  rule: naming.generic_name
  help: https://jadpo.dev/docs/errors/naming.generic_name

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the library's functions share one job**, name it after that job.
Here they hold stock back:

```jadpo fragment
library holdback {
    ...
}
```

```jadpo check project=e-generic-fix-1 hidden
type Stock = Int {
    min: 0
}

library holdback {
    function add_holdback(subtotal: Stock) -> Stock {
        return subtotal
    }
}
```

Calls now read `holdback.add_holdback(subtotal)`, which says where the
function lives and what it is about.

When its functions do unrelated jobs, split them into libraries named
after each job, such as `rounding` and `pricing`. A domain is named the
same way, after a business area such as `ordering` or `planning`.

## Related

- [Domains](../language/01-project-layout-and-modules.md#domains)
- [Naming](../language/01-project-layout-and-modules.md#naming)
- [Calls name their owner](../language/01-project-layout-and-modules.md#calls-name-their-owner)
