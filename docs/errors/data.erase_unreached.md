# `data.erase_unreached`

A field's `erase:` is on a field erasing a person never reaches: one that isn't a reference, or a reference to records erasure never deletes.

## Why Jadpo refuses it

`erase:` says what erasing a person does to a record when erasure
deletes the record it refers to: deletes it too, keeps it without the
person, or keeps it as it is. Erasure deletes only the person's record and
what their references say goes with it, so an `erase:` on a reference to
records erasure never deletes, or on a field that is no reference at all,
would never be used. Accepted, it would look like a decision about erasure
that does nothing. Here a tag refers to a category, which is neither a
person nor anyone's.

## What it looks like

main.jadpo:

```jadpo check project=e-erase-unreached expect=data.erase_unreached
type TagLabel = Text {
    max_length: 40
}

entity Category {
    id: Uuid
}

entity Tag {
    id: Uuid
    category_id: Category.id {
        on_delete: OnDelete.cascade
        erase: Erasure.deleted
    }
    label: TagLabel
}
```

```text output
error: Erasing a person never reaches `Tag.category_id`
 --> main.jadpo:13:9
   |
13 |         erase: Erasure.deleted
   |         ^^^^^ erasing a person never deletes a `Category`
  reason: `erase:` says what erasing a person does to a `Tag` when erasure deletes the `Category` it refers to. Erasure deletes a person's record and what its references say goes with it, and a `Category` isn't either, so it never deletes one.
  next: Remove `erase:` from `category_id` (agent)
  rule: data.erase_unreached
  help: https://jadpo.dev/docs/errors/data.erase_unreached

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An `erase:` on a field that isn't a reference, such as `label`, is
refused the same way, and so is one on a reference to a record erasure
keeps, whose own references stay with it, and one on an event's field: an
event is sent as it was written, and erasure never rewrites it. On a field
of a person's own record that isn't a reference, it means something else
([`data.erase_own_field`](data.erase_own_field.md)).

## How to fix it

**If the record isn't anyone's**, remove `erase:`:

```jadpo fragment
entity Tag {
    id: Uuid
    category_id: Category.id {
        on_delete: OnDelete.cascade
    }
    ...
}
```

```jadpo check project=e-erase-unreached-removed hidden
type TagLabel = Text {
    max_length: 40
}

entity Category {
    id: Uuid
}

entity Tag {
    id: Uuid
    category_id: Category.id {
        on_delete: OnDelete.cascade
    }
    label: TagLabel
}
```

Deleting a category still deletes its tags, as `on_delete:` says.

**If the records belong to a person**, refer to that person
([The person](../language/15-personal-data.md#the-person)):

```jadpo fragment
entity Category {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
}
```

```jadpo check project=e-erase-unreached-owned hidden
type TagLabel = Text {
    max_length: 40
}

entity User {
    id: Uuid
    email: Email {
        personal: PersonalData.contact
    }

    person: true
}

entity Category {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
}

entity Tag {
    id: Uuid
    category_id: Category.id {
        on_delete: OnDelete.cascade
        erase: Erasure.deleted
    }
    label: TagLabel
}
```

Erasing a user now deletes their categories, and `erase:` says what
that does to each tag in one.

## Related

- [Erasure](../language/15-personal-data.md#erasure)
- [The person](../language/15-personal-data.md#the-person)
- [References and `on_delete`](../language/05-data.md#references-and-on_delete)
