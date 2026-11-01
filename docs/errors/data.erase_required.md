# `data.erase_required`

A `restricted` reference into records that erasing a person deletes doesn't say what erasure does to the record holding it.

## Why Jadpo refuses it

Erasing a person deletes their record and decides what happens to every
record that refers to them. A reference marked `OnDelete.restricted`
stops ordinary code deleting the record it points at while it does,
which is the right rule for a mistaken click: a user who still has todos
can't be deleted. A person's legal request to be forgotten can't be
refused that way, so the reference must say what erasure does instead.

Jadpo won't choose for you: whether a todo goes with its owner, stays
without them, or is kept as it is for a legal reason is a decision about
your records.

## What it looks like

main.jadpo:

```jadpo check project=e-erase-required expect=data.erase_required
type TodoTitle = Text {
    max_length: 200
}

entity User {
    id: Uuid
    email: Email {
        personal: PersonalData.contact
    }

    person: true
}

entity Todo {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.restricted
    }
    title: TodoTitle
}
```

```text output
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether a person's todos are deleted with them.

A record further from the person is refused the same way. Once erasing
a user deletes their todos, a note that refers to a todo with a
restricted reference must say what erasure does to it too.

## How to fix it

The question is what should happen to a todo when its owner is erased.

**If the todo goes with its owner**, say `Erasure.deleted`:

```jadpo fragment
entity Todo {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.restricted
        erase: Erasure.deleted
    }
    ...
}
```

```jadpo check project=e-erase-required-deleted hidden
type TodoTitle = Text {
    max_length: 200
}

entity User {
    id: Uuid
    email: Email {
        personal: PersonalData.contact
    }

    person: true
}

entity Todo {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.restricted
        erase: Erasure.deleted
    }
    title: TodoTitle
}
```

Erasing a user now deletes their todos. Ordinary code still can't
delete a user who has todos.

**If the todo is kept without its owner**, make the reference one that
can be `none`, and say `Erasure.anonymised`:

```jadpo fragment
entity Todo {
    id: Uuid
    owner_id: User.id? {
        on_delete: OnDelete.restricted
        erase: Erasure.anonymised
    }
    ...
}
```

```jadpo check project=e-erase-required-anonymised hidden
type TodoTitle = Text {
    max_length: 200
}

entity User {
    id: Uuid
    email: Email {
        personal: PersonalData.contact
    }

    person: true
}

entity Todo {
    id: Uuid
    owner_id: User.id? {
        on_delete: OnDelete.restricted
        erase: Erasure.anonymised
    }
    title: TodoTitle
}
```

Erasing a user now keeps each of their todos with `owner_id` set to
`none` and every field marked `personal:` cleared.

**If the law makes you keep it as it is**, as with an invoice, say
`Erasure.retained` and give the entity a `retention:` saying when it
goes ([Keeping a record without the person](../language/15-personal-data.md#keeping-a-record-without-the-person-erasureretained)).

## Related

- [Erasure](../language/15-personal-data.md#erasure)
- [Deleting the person's records](../language/15-personal-data.md#deleting-the-persons-records-erasuredeleted)
- [References and `on_delete`](../language/05-data.md#references-and-on_delete)
