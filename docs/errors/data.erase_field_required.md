# `data.erase_field_required`

Erasing a person would clear a field that can't be `none`: a reference whose record erasure keeps, a marked field of a record it anonymises, or a marked field of the person's own record it keeps.

## Why Jadpo refuses it

Erasure keeps some records without the person. One kept with
`Erasure.anonymised`, which an `OnDelete.cleared` reference means, loses
its reference to the person and every field marked `personal:`. One kept
with `Erasure.retained` loses only the reference. A field whose type
doesn't end in `?` must always hold a value, so erasure couldn't clear
it, and the person's data would stay. A person's own record kept with
`erase: Erasure.retained` on one of its fields loses every other marked
field the same way. Here a comment is kept when its author is erased, but
the name it was signed with can't be cleared.

Jadpo won't choose for you: whether the comment is kept without the
name, or deleted with its author, is a decision about your records.

## What it looks like

main.jadpo:

```jadpo check project=e-erase-field-required expect=data.erase_field_required
type CommentText = Text {
    max_length: 500
}

type SignedName = Text {
    max_length: 80
}

entity User {
    id: Uuid
    email: Email {
        personal: PersonalData.contact
    }

    person: true
}

entity Comment {
    id: Uuid
    author_id: User.id? {
        on_delete: OnDelete.cleared
    }
    body: CommentText
    signed_as: SignedName {
        personal: PersonalData.identifying
    }
}
```

```text output
error: Erasing a `User` can't clear `Comment.signed_as`
 --> main.jadpo:24:5
   |
20 |     author_id: User.id? {
   |     --------- keeps the record without the person
24 |     signed_as: SignedName {
   |     ^^^^^^^^^ personal data that can't be `none`
  reason: `author_id` is `OnDelete.cleared`, which means `Erasure.anonymised`: erasing a `User` keeps each comment that refers to them, clearing the reference and every marked field. `signed_as` is marked `personal:`, and only an optional field (`SignedName?`) can be cleared.
  next: Make `signed_as` `SignedName?`, or say `erase: Erasure.deleted` on `author_id` to delete the comment instead (human)
  rule: data.erase_field_required
  help: https://jadpo.dev/docs/errors/data.erase_field_required

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether a person's comments outlive them.

A reference erasure clears is refused the same way. `Erasure.retained`
or `Erasure.anonymised` on `author_id: User.id`, which can't be `none`,
keeps the comment while it still points at someone who is gone.

## How to fix it

The question is whether a comment is kept once its author is erased.

**If the comment stays without its author's name**, make the field one
that can be `none`:

```jadpo fragment
entity Comment {
    ...
    signed_as: SignedName? {
        personal: PersonalData.identifying
    }
}
```

```jadpo check project=e-erase-field-required-optional hidden
type CommentText = Text {
    max_length: 500
}

type SignedName = Text {
    max_length: 80
}

entity User {
    id: Uuid
    email: Email {
        personal: PersonalData.contact
    }

    person: true
}

entity Comment {
    id: Uuid
    author_id: User.id? {
        on_delete: OnDelete.cleared
    }
    body: CommentText
    signed_as: SignedName? {
        personal: PersonalData.identifying
    }
}
```

Erasing a user now keeps each of their comments with `author_id` and
`signed_as` set to `none`. The body isn't marked, so it stays.

**If the comment goes with its author**, say `Erasure.deleted` on the
reference:

```jadpo fragment
entity Comment {
    id: Uuid
    author_id: User.id? {
        on_delete: OnDelete.cleared
        erase: Erasure.deleted
    }
    ...
}
```

```jadpo check project=e-erase-field-required-deleted hidden
type CommentText = Text {
    max_length: 500
}

type SignedName = Text {
    max_length: 80
}

entity User {
    id: Uuid
    email: Email {
        personal: PersonalData.contact
    }

    person: true
}

entity Comment {
    id: Uuid
    author_id: User.id? {
        on_delete: OnDelete.cleared
        erase: Erasure.deleted
    }
    body: CommentText
    signed_as: SignedName {
        personal: PersonalData.identifying
    }
}
```

Erasing a user now deletes their comments. Ordinary code deleting a
user still keeps them, with `author_id` set to `none`.

## Related

- [Erasure](../language/15-personal-data.md#erasure)
- [Marking personal fields](../language/15-personal-data.md#marking-personal-fields)
- [Values that may be missing](../language/02-values-and-types.md#values-that-may-be-missing)
