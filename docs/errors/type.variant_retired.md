# `type.variant_retired`

Code writes a retired code of a closed list, one its authority no longer issues, such as `Country.yu`.

## Why Jadpo refuses it

A closed list keeps a code its authority withdraws, so stored data and old
messages still read, but new code that writes one is refused: a shipment
can't be sent to Yugoslavia. A `match` arm on a retired code only reads,
so it isn't refused. Code that reads old records may name the code on
purpose, which only you know, so you can decline this with a reason.

## What it looks like

main.jadpo:

```jadpo check project=e-variant-retired expect=type.variant_retired
type Consignment = Object {
    delivered_to: Country
}

/// A consignment to a country by its old code.
function to_belgrade() -> Consignment {
    return Consignment {
        delivered_to: Country.yu
    }
}
```

```text output
error: `Country.yu` is a retired code
 --> main.jadpo:8:23
  |
8 |         delivered_to: Country.yu
  |                       ^^^^^^^^^^ only read
  reason: ISO 3166-1 withdrew `YU` (Yugoslavia). `Country.yu` stays so that stored data and old messages still read, but new code doesn't write it.
  next: Use the code that replaced it, or write `decline type.variant_retired: "<why>"` above it if this is deliberate (agent)
  rule: type.variant_retired
  help: https://jadpo.dev/docs/errors/type.variant_retired

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether the code is meant to be written here.

**If it should be today's code**, use the one that replaced it:

```jadpo fragment
        delivered_to: Country.rs
```

```jadpo check project=e-variant-retired-fix-1 hidden
type Consignment = Object {
    delivered_to: Country
}

/// A consignment to a country by its current code.
function to_belgrade() -> Consignment {
    return Consignment {
        delivered_to: Country.rs
    }
}
```

The consignment now goes to Serbia, under the code in use today.

**If the old code is what you mean**, write a decline line with the reason
directly above the declaration that writes it, with the code in a `var` of
its own:

```jadpo fragment
decline type.variant_retired: "Reissues paperwork for consignments sent before the code was withdrawn"
function to_belgrade() -> Consignment {
    var old_code: Country = Country.yu
    return Consignment {
        delivered_to: old_code
    }
}
```

```jadpo check project=e-variant-retired-fix-2 hidden
type Consignment = Object {
    delivered_to: Country
}

/// A consignment to a country by its old code.
decline type.variant_retired: "Reissues paperwork for consignments sent before the code was withdrawn"
function to_belgrade() -> Consignment {
    var old_code: Country = Country.yu
    return Consignment {
        delivered_to: old_code
    }
}
```

The code stays, and `jadpo approval` lists the decline with its reason, so
a reviewer sees why it is written.

## Related

- [Retired codes](../language/19-standard-library.md#retired-codes)
- [Closed lists are enums](../language/19-standard-library.md#closed-lists-are-enums)
- [Writing a decline](../language/01-project-layout-and-modules.md#writing-a-decline)
