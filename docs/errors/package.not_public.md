# `package.not_public`

Code uses a declaration of a package that the package doesn't mark `public`.

## Why Jadpo refuses it

A package offers only what it marks `public`, so its authors can change
everything else in any version without breaking the apps that use it. Code
that reaches past that line would break when the package changed something
it never promised to keep.

## What it looks like

`tidying` is a library only the package's own code uses:

```jadpo check project=p71-private file=app.jadpo hidden
application {}
```

```jadpo check project=p71-private file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71-private file=shared/providers/acme_couriers/v1/types/slot_text.jadpo hidden
/// A collection slot as Acme writes it.
public type SlotText = Text {
    max_length: 12
}
```

```jadpo check project=p71-private file=shared/providers/acme_couriers/v1/libraries/timetable.jadpo hidden
/// Reading collection slots.
public library timetable {
    /// A slot in the form Acme expects.
    function normal(slot: SlotText) -> SlotText {
        return tidying.same(slot)
    }
}
```

```jadpo check project=p71-private file=shared/providers/acme_couriers/v1/libraries/tidying.jadpo hidden
library tidying {
    function same(slot: SlotText) -> SlotText {
        return slot
    }
}
```

`domains/shipping/actions/keep_slot.jadpo`:

```jadpo check project=p71-private file=domains/shipping/actions/keep_slot.jadpo expect=package.not_public
use shared/providers/acme_couriers

action keep_slot(slot: AcmeCouriers.SlotText) -> AcmeCouriers.SlotText {
    return AcmeCouriers.tidying.same(slot)
}
```

```text output
error: `AcmeCouriers.tidying` isn't public
 --> domains/shipping/actions/keep_slot.jadpo:4:12
  |
4 |     return AcmeCouriers.tidying.same(slot)
  |            ^^^^^^^^^^^^^^^^^^^^ not part of the package's API
  reason: A package offers only what it marks `public`; `tidying` is a library in `shared/providers/acme_couriers/v1/libraries/tidying.jadpo` that only the package's own code uses, and may change in any version.
  next: Use what `AcmeCouriers` marks `public` instead (agent)
  rule: package.not_public
  help: https://jadpo.dev/docs/errors/package.not_public

check failed: 1 error, 6 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A resource, an action or a message of the package that isn't `public` is
refused the same way.

## How to fix it

**If the package offers something that does the job**, use that:

```jadpo check project=p71-private-fix file=domains/shipping/actions/keep_slot.jadpo
use shared/providers/acme_couriers

action keep_slot(slot: AcmeCouriers.SlotText) -> AcmeCouriers.SlotText {
    return AcmeCouriers.timetable.normal(slot)
}
```

```jadpo check project=p71-private-fix file=app.jadpo hidden
application {}
```

```jadpo check project=p71-private-fix file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71-private-fix file=shared/providers/acme_couriers/v1/types/slot_text.jadpo hidden
/// A collection slot as Acme writes it.
public type SlotText = Text {
    max_length: 12
}
```

```jadpo check project=p71-private-fix file=shared/providers/acme_couriers/v1/libraries/timetable.jadpo hidden
/// Reading collection slots.
public library timetable {
    /// A slot in the form Acme expects.
    function normal(slot: SlotText) -> SlotText {
        return tidying.same(slot)
    }
}
```

```jadpo check project=p71-private-fix file=shared/providers/acme_couriers/v1/libraries/tidying.jadpo hidden
library tidying {
    function same(slot: SlotText) -> SlotText {
        return slot
    }
}
```

The app now depends only on what the package promises to keep.

**If the package is yours and should offer it**, mark it `public`, with a
`///` line saying what it is:

```jadpo check project=p71-private-public file=shared/providers/acme_couriers/v1/libraries/tidying.jadpo
/// Tidying up collection slots.
public library tidying {
    /// The slot, unchanged.
    function same(slot: SlotText) -> SlotText {
        return slot
    }
}
```

```jadpo check project=p71-private-public file=app.jadpo hidden
application {}
```

```jadpo check project=p71-private-public file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71-private-public file=shared/providers/acme_couriers/v1/types/slot_text.jadpo hidden
/// A collection slot as Acme writes it.
public type SlotText = Text {
    max_length: 12
}
```

```jadpo check project=p71-private-public file=shared/providers/acme_couriers/v1/libraries/timetable.jadpo hidden
/// Reading collection slots.
public library timetable {
    /// A slot in the form Acme expects.
    function normal(slot: SlotText) -> SlotText {
        return tidying.same(slot)
    }
}
```

```jadpo check project=p71-private-public file=domains/shipping/actions/keep_slot.jadpo hidden
use shared/providers/acme_couriers

action keep_slot(slot: AcmeCouriers.SlotText) -> AcmeCouriers.SlotText {
    return AcmeCouriers.tidying.same(slot)
}
```

`tidying` is now part of the package's API, so changing it is a change an
app can notice.

## Related

- [What is public](../language/11-integrations.md#what-is-public)
- [A package's surface](../language/11-integrations.md#a-packages-surface)
