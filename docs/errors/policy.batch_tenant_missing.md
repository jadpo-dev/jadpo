# `policy.batch_tenant_missing`

A batch reaches a tenant's records but saves no value naming which tenant, so every run would find none of them.

## Why Jadpo refuses it

In an app with a `tenant:`, a batch's run works as the system within the
tenant its saved values of the tenant's `id` type name: a `from:` argument,
a value `each:` or `finished:` captures, or `key:`. A run that saves none
reaches no tenant's records, only those every tenant shares, and a value
naming no tenant never widens what it reaches.

So a batch that saves no tenant's id and walks, reads or changes a tenant's
records would start, walk nothing and finish, every time, as if there were
nothing to do. No test would see anything go wrong. Jadpo refuses it
instead, at the call that reaches the tenant's records.

## What it looks like

Here the batch walks every household's chores and saves no household:

```jadpo check project=e-batch-tenant-missing expect=policy.batch_tenant_missing
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
        read: [HomeRole.member, Access.system]
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

query household_chores(household_id: Household.id, page: ChorePage) -> Page<Chore> {
    from: Chore
    where: Chore.household_id == household_id
    order: [Chore.id asc]
    page: {
        size: page.page_size
        after: page.after
    }
}

query every_chore() -> List<Chore> {
    from: Chore
    order: [Chore.id asc]
    limit: every
}

action bump(chore: Chore) fails ChoreNotFound -> Unit {
    attempt Chore.update(chore.id) {
        set: {
            times_done: Chore.times_done + 1
        }
    }
}

action bump_every_chore(page: ChorePage) -> BatchRun {
    return batches.each(from: all_chores(page), each: chore -> bump(chore))
}
```

```text output
error: `bump_every_chore` starts a batch that names no `Household`
 --> main.jadpo:122:31
    |
122 |     return batches.each(from: all_chores(page), each: chore -> bump(chore))
    |                               ^^^^^^^^^^^^^^^^ reads `Chore` records
  reason: A batch's run works as the system within the `Household` its saved values of type `Household.id` name, and `bump_every_chore` saves none, so its run would reach no `Household`'s records: it would find no `Chore` and finish as if there were nothing to do.
  next: Save the `Household` the run is for with `key:`, read from a record `bump_every_chore` reaches, such as `key: household.id`; for every `Household` at once, do the work in a schedule's own action, which reaches them all, with a `limit: every` query and a loop of writes (agent)
  rule: policy.batch_tenant_missing
  help: https://jadpo.dev/docs/errors/policy.batch_tenant_missing

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the run is for one tenant**, save that tenant, read from a record the
action reaches. A record the action is handed whole counts as read, and
`key:` is the usual place:

```jadpo fragment
    return batches.each(
        from: household_chores(household.id, page),
        each: chore -> bump(chore),
        key: household.id
    )
```

```jadpo check project=batch-tenant-keyed hidden
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
        read: [HomeRole.member, Access.system]
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

query household_chores(household_id: Household.id, page: ChorePage) -> Page<Chore> {
    from: Chore
    where: Chore.household_id == household_id
    order: [Chore.id asc]
    page: {
        size: page.page_size
        after: page.after
    }
}

query every_chore() -> List<Chore> {
    from: Chore
    order: [Chore.id asc]
    limit: every
}

action bump(chore: Chore) fails ChoreNotFound -> Unit {
    attempt Chore.update(chore.id) {
        set: {
            times_done: Chore.times_done + 1
        }
    }
}

action bump_household(household: Household, page: ChorePage) -> BatchRun {
    return batches.each(
        from: household_chores(household.id, page),
        each: chore -> bump(chore),
        key: household.id
    )
}
```

The run then reaches that household's chores alone. An id the caller passes
proves nothing, and is
[`policy.event_tenant_unproved`](policy.event_tenant_unproved.md).

**If the work is for every tenant at once**, such as an hourly job over
every tenant's records, do it in a schedule's own action. A schedule's run
names no tenant by design, so it reaches every one. Read the records with a
`limit: every` query and change them in a loop:

```jadpo fragment
action bump_every_household() -> Unit {
    var chores: List<Chore> = every_chore()
    for chore in chores {
        var bumped: Chore = Chore.update(chore) {
            set: {
                times_done: chore.times_done + 1
            }
        }
        assert bumped.times_done > 0
    }
}

schedule nightly_bump {
    every: 1d
    run: bump_every_household()
}
```

```jadpo check project=batch-tenant-schedule hidden
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
        read: [HomeRole.member, Access.system]
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

query household_chores(household_id: Household.id, page: ChorePage) -> Page<Chore> {
    from: Chore
    where: Chore.household_id == household_id
    order: [Chore.id asc]
    page: {
        size: page.page_size
        after: page.after
    }
}

query every_chore() -> List<Chore> {
    from: Chore
    order: [Chore.id asc]
    limit: every
}

action bump(chore: Chore) fails ChoreNotFound -> Unit {
    attempt Chore.update(chore.id) {
        set: {
            times_done: Chore.times_done + 1
        }
    }
}

action bump_every_household() -> Unit {
    var chores: List<Chore> = every_chore()
    for chore in chores {
        var bumped: Chore = Chore.update(chore) {
            set: {
                times_done: chore.times_done + 1
            }
        }
        assert bumped.times_done > 0
    }
}

schedule nightly_bump {
    every: 1d
    run: bump_every_household()
}
```

A batch can't be started for each tenant in a loop
([`data.read_in_loop`](data.read_in_loop.md)), and there is no batch that
reaches every tenant: only a schedule's run does.

## Related

- [Which tenant a run reaches](../language/12-jobs-and-events.md#which-tenant-a-run-reaches)
- [Background work and tenants](../language/12-jobs-and-events.md#background-work-and-tenants)
- [Which tenant background work reaches](../language/09-policy.md#which-tenant-background-work-reaches)
