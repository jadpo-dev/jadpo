# `package.not_public`

A package offers only what it marks `public`:

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
