# `data.erase_unreached`

A field's `erase:` is on a field erasing a person never reaches: one that isn't a reference, or a reference to records that belong to no person.

## Why Jadpo refuses it

`erase:` says what erasing a person does to a record when erasure
deletes the record it refers to: deletes it too, keeps it without the
person, or keeps it as it is. Erasure deletes only the person's record
and the records that belong to them, so an `erase:` on a reference to
records that belong to no person, or on a field that is no reference at
all, would never be used. Accepted, it would look like a decision about
erasure that does nothing. Here a tag refers to a category, which is
neither a person nor anyone's.

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
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An `erase:` on a field that isn't a reference, such as `label`, is
refused the same way, and so is one on an event's field: an event is
sent as it was written, and erasure never rewrites it.

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

**If the records belong to a person**, refer to that person, or mark
the entity `person: true` if each record is one
([The person](../language/15-personal-data.md#the-person)). Erasure then
reaches them, and `erase:` says what it does.

## Related

- [Erasure](../language/15-personal-data.md#erasure)
- [The person](../language/15-personal-data.md#the-person)
- [References and `on_delete`](../language/05-data.md#references-and-on_delete)
