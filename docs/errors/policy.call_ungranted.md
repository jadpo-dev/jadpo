# `policy.call_ungranted`

A line under `actions:` replaces its entities' grants only on the
creates, updates, deletes and transitions in its own action's body. An
action it calls keeps its own grants, with the same caller, so it can
refuse some of the people the line lets in, who then get 403
`not_permitted`. Here `borrow` is listed for anyone signed in, but the
`Book` change is made in `lend`, which only `StaffRole.librarian` may make:

```jadpo check project=e-call-ungranted expect=policy.call_ungranted
type Handle = Text {
    max_length: 40
}

type StaffRole = Enum {
    librarian
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        read: [id]
    }
}

principal {
    user: {
        subject: Member.handle
    }
}

entity Staff {
    id: Uuid
    member_id: Member.id {
        on_delete: OnDelete.cascade
    }
    role: StaffRole

    membership: {
        scope: MembershipScope.application
        member: member_id
        role: role
    }

    policy: {
        read: [member_id]
    }
}

entity Book {
    id: Uuid
    title: Handle
    borrower_id: Member.id? {
        on_delete: OnDelete.restricted
    }

    policy: {
        create: [StaffRole.librarian]
        read: [Access.authenticated]
        update: [StaffRole.librarian]
    }
}

/// Lends a book to a member, as staff do at the desk.
action lend(book_id: Book.id, member_id: Member.id) fails BookNotFound, MemberNotFound -> Book {
    return attempt Book.update(book_id) {
        set: {
            borrower_id: member_id
        }
    }
}

/// Borrows a book for whoever is signed in.
action borrow(book_id: Book.id, caller: Principal.user) fails BookNotFound, MemberNotFound -> Book {
    return attempt lend(book_id, caller.id)
}

policy {
    actions: {
        borrow: [Access.authenticated]
    }
}
```

```text output
error: `borrow` calls `lend`, which refuses some of the people `borrow`'s line lets in
 --> main.jadpo:69:20
   |
69 |     return attempt lend(book_id, caller.id)
   |                    ^^^^^^^^^^^^^^^^^^^^^^^^ refuses some callers `Access.authenticated` lets in
  reason: `borrow`'s line under `actions:` covers only the creates, updates, deletes and transitions in `borrow`'s own body; `lend` keeps its own grants, with the same caller. `lend` changes `Book` records, which only `StaffRole.librarian` may, so some callers `Access.authenticated` lets in get 403 `not_permitted` from it.
  next: Make the change in `borrow`'s own body, or list `lend` under `actions:` for exactly the people `borrow`'s line names, or write `decline policy.call_ungranted: "<why>"` above it if this is deliberate (human)
  rule: policy.call_ungranted
  help: https://jadpo.dev/docs/errors/policy.call_ungranted

check failed: 1 error, 1 file
```

Make the `Book.update` in `borrow` itself, where its line applies, or
list `lend` for exactly the people `borrow`'s line names. Listing `lend`
for everyone widens it on every path that reaches it, a route of its own
included.

What the called action refuses is followed through everything it does:
its creates, updates and deletes, a field's own grants where they narrow
the record's, the line of a listed action or query it runs, and the same
for every action it calls in turn.

The error can be declined, with a `decline` line above the listed action,
for a call only some callers are meant to pass. It is a security rule, so
a decline will also need an
[acceptance](../language/18-records-and-manifest.md#acceptances); that
isn't built yet
([Writing a decline](../language/01-project-layout-and-modules.md#writing-a-decline)).
