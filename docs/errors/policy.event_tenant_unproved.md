# `policy.event_tenant_unproved`

An event or a batch names a tenant its action never reached under its caller's policy.

## Why Jadpo refuses it

An event's subscribers run as the system within the tenant its field of the
tenant's `id` type names, and a batch's run works within the one its saved
values of that type name. That id must come from a record of the tenant the
action read or wrote under its caller's policy. An id the caller passed could
be any tenant's, and it would aim the system at a tenant they don't belong to.

Jadpo can't choose which tenant you meant, so the action has to reach the
tenant first and name it from there.

## What it looks like

Here the event names the household the caller passed, not the chore's:

```jadpo check project=e-event-tenant-unproved expect=policy.event_tenant_unproved
type Handle = Text {
    max_length: 40
}

type Title = Text {
    max_length: 80
}

type HomeRole = Enum {
    member
}

entity Housemate {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        read: [id]
        tenant: Tenancy.shared
    }
}

principal {
    user: {
        subject: Housemate.handle
    }
}

entity Household {
    id: Uuid
    name: Title

    policy: {
        read: [HomeRole.member]
    }
}

entity HomeMembership {
    id: Uuid
    household_id: Household.id {
        on_delete: OnDelete.cascade
    }
    person_id: Housemate.id {
        on_delete: OnDelete.cascade
    }
    role: HomeRole

    membership: {
        scope: household_id
        member: person_id
        role: role
    }

    policy: {
        read: [person_id]
    }
}

entity Chore {
    id: Uuid
    household_id: Household.id {
        on_delete: OnDelete.cascade
    }
    title: Title
    thanked: Bool

    policy: {
        read: [HomeRole.member]
        update: [HomeRole.member]
    }
}

policy {
    tenant: Household
}

type ChoreThanked = Event {
    chore_id: Chore.id
    household_id: Household.id
}

action thank(chore_id: Chore.id, household_id: Household.id) fails ChoreNotFound -> Unit {
    var chore: Chore = attempt Chore.update(chore_id) {
        set: {
            thanked: true
        }
    }
    emit ChoreThanked {
        chore_id: chore.id
        household_id: household_id
    }
}
```

```text output
error: `thank` sends `ChoreThanked` to a `Household` it never reached
 --> main.jadpo:92:23
   |
92 |         household_id: household_id
   |                       ^^^^^^^^^^^^ `household_id`, whatever the caller passes
  reason: `ChoreThanked`'s subscribers run as the system within the `Household` its `household_id` names, so that id must come from a `Household`, or a record of one, that `thank` read or wrote under its caller's policy; one the caller passes could name a `Household` they don't belong to.
  next: Name the `Household` of a record `thank` reads or writes, or read it first with `attempt Household.get(household_id)` (agent)
  rule: policy.event_tenant_unproved
  help: https://jadpo.dev/docs/errors/policy.event_tenant_unproved

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A batch's `key:` naming the household the caller passed is refused the same way:

```jadpo check project=e-run-tenant-unproved expect=policy.event_tenant_unproved
type Handle = Text {
    max_length: 40
}

type Title = Text {
    max_length: 80
}

type HomeRole = Enum {
    member
}

entity Housemate {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        read: [id]
        tenant: Tenancy.shared
    }
}

principal {
    user: {
        subject: Housemate.handle
    }
}

entity Household {
    id: Uuid
    name: Title

    policy: {
        read: [HomeRole.member]
    }
}

entity HomeMembership {
    id: Uuid
    household_id: Household.id {
        on_delete: OnDelete.cascade
    }
    person_id: Housemate.id {
        on_delete: OnDelete.cascade
    }
    role: HomeRole

    membership: {
        scope: household_id
        member: person_id
        role: role
    }

    policy: {
        read: [person_id]
    }
}

entity Chore {
    id: Uuid
    household_id: Household.id {
        on_delete: OnDelete.cascade
    }
    title: Title
    times_done: Int

    policy: {
        read: [HomeRole.member, Access.system]
        update: [Access.system]
    }
}

policy {
    tenant: Household
}

type ChorePage = Input {
    page_size: PageSize {
        default: 25
    }
    after: PageCursor {
        optional: true
    }
}

query all_chores(page: ChorePage) -> Page<Chore> {
    from: Chore
    order: [Chore.id asc]
    page: {
        size: page.page_size
        after: page.after
    }
}

action bump(chore: Chore) fails ChoreNotFound -> Unit {
    attempt Chore.update(chore.id) {
        set: {
            times_done: Chore.times_done + 1
        }
    }
}

action sweep_house(household_id: Household.id, page: ChorePage) -> BatchRun {
    return batches.each(from: all_chores(page), each: chore -> bump(chore), key: household_id)
}
```

```text output
error: `sweep_house` starts a batch in a `Household` it never reached
 --> main.jadpo:106:82
    |
106 |     return batches.each(from: all_chores(page), each: chore -> bump(chore), key: household_id)
    |                                                                                  ^^^^^^^^^^^^ `household_id`, whatever the caller passes
  reason: A batch's run works as the system within the `Household` its saved values of type `Household.id` name, so each must come from a `Household`, or a record of one, that `sweep_house` read or wrote under its caller's policy; one the caller passes could name a `Household` they don't belong to.
  next: Name the `Household` of a record `sweep_house` reads or writes, or read it first with `attempt Household.get(household_id)` (agent)
  rule: policy.event_tenant_unproved
  help: https://jadpo.dev/docs/errors/policy.event_tenant_unproved

check failed: 1 error, 1 file
```

## How to fix it

**If the action changes or reads a record of the tenant**, name the tenant
through that record:

```jadpo fragment
    emit ChoreThanked {
        chore_id: chore.id
        household_id: chore.household_id
    }
```

```jadpo check project=event-tenant-proved hidden
type Handle = Text {
    max_length: 40
}

type Title = Text {
    max_length: 80
}

type HomeRole = Enum {
    member
}

entity Housemate {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        read: [id]
        tenant: Tenancy.shared
    }
}

principal {
    user: {
        subject: Housemate.handle
    }
}

entity Household {
    id: Uuid
    name: Title

    policy: {
        read: [HomeRole.member]
    }
}

entity HomeMembership {
    id: Uuid
    household_id: Household.id {
        on_delete: OnDelete.cascade
    }
    person_id: Housemate.id {
        on_delete: OnDelete.cascade
    }
    role: HomeRole

    membership: {
        scope: household_id
        member: person_id
        role: role
    }

    policy: {
        read: [person_id]
    }
}

entity Chore {
    id: Uuid
    household_id: Household.id {
        on_delete: OnDelete.cascade
    }
    title: Title
    thanked: Bool

    policy: {
        read: [HomeRole.member]
        update: [HomeRole.member]
    }
}

policy {
    tenant: Household
}

type ChoreThanked = Event {
    chore_id: Chore.id
    household_id: Household.id
}

action thank(chore_id: Chore.id, household_id: Household.id) fails ChoreNotFound -> Unit {
    var chore: Chore = attempt Chore.update(chore_id) {
        set: {
            thanked: true
        }
    }
    emit ChoreThanked {
        chore_id: chore.id
        household_id: chore.household_id
    }
}
```

The chore was changed under the caller's grants, so its household is one they
belong to, and a caller who isn't a member finds no chore and the action fails.

**If the action reaches nothing of the tenant yet**, read the tenant first:

```jadpo fragment
    var household: Household = attempt Household.get(household_id)
    return batches.each(from: all_chores(page), each: chore -> bump(chore), key: household.id)
```

```jadpo check project=run-tenant-proved hidden
type Handle = Text {
    max_length: 40
}

type Title = Text {
    max_length: 80
}

type HomeRole = Enum {
    member
}

entity Housemate {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        read: [id]
        tenant: Tenancy.shared
    }
}

principal {
    user: {
        subject: Housemate.handle
    }
}

entity Household {
    id: Uuid
    name: Title

    policy: {
        read: [HomeRole.member]
    }
}

entity HomeMembership {
    id: Uuid
    household_id: Household.id {
        on_delete: OnDelete.cascade
    }
    person_id: Housemate.id {
        on_delete: OnDelete.cascade
    }
    role: HomeRole

    membership: {
        scope: household_id
        member: person_id
        role: role
    }

    policy: {
        read: [person_id]
    }
}

entity Chore {
    id: Uuid
    household_id: Household.id {
        on_delete: OnDelete.cascade
    }
    title: Title
    times_done: Int

    policy: {
        read: [HomeRole.member, Access.system]
        update: [Access.system]
    }
}

policy {
    tenant: Household
}

type ChorePage = Input {
    page_size: PageSize {
        default: 25
    }
    after: PageCursor {
        optional: true
    }
}

query all_chores(page: ChorePage) -> Page<Chore> {
    from: Chore
    order: [Chore.id asc]
    page: {
        size: page.page_size
        after: page.after
    }
}

action bump(chore: Chore) fails ChoreNotFound -> Unit {
    attempt Chore.update(chore.id) {
        set: {
            times_done: Chore.times_done + 1
        }
    }
}

action sweep_house(
    household_id: Household.id,
    page: ChorePage
) fails HouseholdNotFound -> BatchRun {
    var household: Household = attempt Household.get(household_id)
    return batches.each(from: all_chores(page), each: chore -> bump(chore), key: household.id)
}
```

A caller who isn't in the household now fails at the read, so the batch only
ever runs in a household the caller belongs to.

## Related

- [Proving which tenant work is for](../language/09-policy.md#proving-which-tenant-work-is-for)
- [Which tenant background work reaches](../language/09-policy.md#which-tenant-background-work-reaches)
- [Tenancy](../language/09-policy.md#tenancy)
