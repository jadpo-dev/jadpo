# `data.lifecycle_condition`

A lifecycle's `visible:` or `when:` condition isn't a plain test of the record's own fields against fixed values.

## Why Jadpo refuses it

A lifecycle's conditions are about the record they are written in. They
compare its fields with fixed values: a literal, an enum value, `none` or
`clock.now`. Because of that, Jadpo can add `visible:` to every statement
that reads the entity, and to an index, without knowing who is asking.

A condition that mentions another record, or the person making the
request, can't become part of every statement in that way. Who may see a
record is a different question, and the entity's `policy:` answers it.

## What it looks like

main.jadpo:

```jadpo check project=e-lifecycle-condition expect=data.lifecycle_condition
entity Todo {
    id: Uuid
    owner_id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none and owner_id == current_user.id
    }
}
```

```text output
error: `current_user.id` isn't something a lifecycle condition can compare with
 --> main.jadpo:7:53
  |
7 |         visible: deleted_at == none and owner_id == current_user.id
  |                                                     ^^^^^^^^^^^^^^^ from outside the record
  reason: A lifecycle condition compares the record's own fields with fixed values: a literal, an enum value or `clock.now`. It never mentions other records or who is asking, which is policy (area 9), so it can join every statement's `WHERE` and an index.
  next: Compare with a literal, an enum value or `clock.now` (agent)
  rule: data.lifecycle_condition
  help: https://jadpo.dev/docs/errors/data.lifecycle_condition

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A condition that names a field with its entity in front, as a query
writes it, is refused too. Inside the lifecycle the record is already
known, so its fields are named bare.

main.jadpo:

```jadpo check project=e-lifecycle-condition-qualified expect=data.lifecycle_condition
entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: Todo.deleted_at == none
    }
}
```

```text output
error: `Todo.deleted_at` names its own entity
 --> main.jadpo:6:18
  |
6 |         visible: Todo.deleted_at == none
  |                  ^^^^^^^^^^^^^^^ written as a query writes it
  reason: A lifecycle's conditions are about the record they are written in, so they name its fields bare: `deleted_at`, not `Todo.deleted_at`.
  next: Write `deleted_at` (agent)
  rule: data.lifecycle_condition
  help: https://jadpo.dev/docs/errors/data.lifecycle_condition

check failed: 1 error, 1 file
```

## How to fix it

**If the condition says who may see the record**, keep only the test of
the record's own fields in `visible:`, and say who may read it in the
entity's `policy:`. Here `owner_id` becomes a reference to the `User` who
owns the todo, so `read: [owner_id]` lets that user read it:

```jadpo fragment
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
    }

    policy: {
        read: [owner_id]
    }
```

```jadpo check project=e-lifecycle-condition-fix-1 hidden
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

authentication {
    api_token: {}
}

entity Todo {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
    }

    policy: {
        read: [owner_id]
    }
}
```

Deleted todos still drop out of every read. Each signed-in user now reads
only the todos they own, and policy refuses the rest.

**If a field is written with its entity's name**, write it bare:

```jadpo fragment
        visible: deleted_at == none
```

```jadpo check project=e-lifecycle-condition-fix-2 hidden
entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
    }
}
```

The condition means the same as before, and normal reads leave out todos
whose `deleted_at` is set.

## Related

- [Lifecycle](../language/05-data.md#lifecycle)
- [Grants](../language/09-policy.md#grants)
- [A user field on the record](../language/09-policy.md#a-user-field-on-the-record)
