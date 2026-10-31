# `policy.call_ungranted`

An action listed under `actions:` calls another action that refuses some of the people the listed action's line lets in.

## Why Jadpo refuses it

A line under `actions:` says who may run an action, and it replaces the
entities' grants only on the creates, updates, deletes and transitions in
that action's own body. An action it calls keeps its own grants, with the
same caller. So the called action can refuse some of the people the line
lets in, and they get 403 `not_permitted` part way through.

Jadpo won't choose the fix for you. Each one decides who may make the
change: everyone the line names, everyone on every path to the called
action, or only some callers, on purpose.

## What it looks like

main.jadpo:

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
  next: Make the change in `borrow`'s own body, or list `lend` under `actions:` for exactly the people `borrow`'s line names, or write `decline policy.call_ungranted: "<why>"` above it if this is deliberate, which a person then accepts (human)
  rule: policy.call_ungranted
  help: https://jadpo.dev/docs/errors/policy.call_ungranted

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides who may lend a book.

`borrow` is listed for anyone signed in, but the `Book` change is made in
`lend`, which only `StaffRole.librarian` may make. What the called action
refuses is followed through everything it does: its creates, updates and
deletes, a field's own grants where they narrow the record's, the line of
a listed action or query it runs, and the same for every action it calls
in turn.

## How to fix it

The question is who should be able to make the change the called action
makes.

**If everyone `borrow`'s line names may make it, only through `borrow`**,
make the change in `borrow`'s own body, where its line applies:

```jadpo fragment
action borrow(book_id: Book.id, caller: Principal.user) fails BookNotFound -> Book {
    return attempt Book.update(book_id) {
        set: {
            borrower_id: caller.id
        }
    }
}
```

```jadpo check project=e-call-ungranted-own-body hidden
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
action borrow(book_id: Book.id, caller: Principal.user) fails BookNotFound -> Book {
    return attempt Book.update(book_id) {
        set: {
            borrower_id: caller.id
        }
    }
}

policy {
    actions: {
        borrow: [Access.authenticated]
    }
}
```

Anyone signed in may borrow a book for themselves through `borrow`.
`lend` is unchanged, so only librarians may lend a book to someone else.

**If they may make it through `lend` too**, list `lend` under `actions:`
for exactly the people `borrow`'s line names:

```jadpo fragment
policy {
    actions: {
        borrow: [Access.authenticated]
        lend: [Access.authenticated]
    }
}
```

```jadpo check project=e-call-ungranted-list-called hidden
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
        lend: [Access.authenticated]
    }
}
```

Anyone signed in may now run `lend`, on every path that reaches it, a
route of its own included. So any member may lend a book to anyone, not
only to themselves. Adding the line widens who may change a `Book`, so
it also needs a person's acceptance
([Protecting policy](../language/09-policy.md#protecting-policy)).

**If only some callers are meant to get through**, decline the error
with a `decline` line above `borrow`, after its `///` comment, giving the
reason:

```jadpo fragment
decline policy.call_ungranted: "Only staff borrow through the desk app."
action borrow(book_id: Book.id, caller: Principal.user) fails BookNotFound, MemberNotFound -> Book {
    ...
}
```

```jadpo check project=e-call-ungranted-declined hidden
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
decline policy.call_ungranted: "Only staff borrow through the desk app."
action borrow(book_id: Book.id, caller: Principal.user) fails BookNotFound, MemberNotFound -> Book {
    return attempt lend(book_id, caller.id)
}

policy {
    actions: {
        borrow: [Access.authenticated]
    }
}
```

`borrow` stays as it is, and a member who isn't a librarian still gets
403 `not_permitted` from `lend`. `policy.call_ungranted` is a security
rule, so a new decline of it also needs a person's acceptance:
`jadpo check` fails on the change until an acceptance lists
`PolicyChange.call_ungranted_declined` on `borrow`, and
`access.jadpo_record` keeps the decline.

## Related

- [A call the listing doesn't cover](../language/09-policy.md#a-call-the-listing-doesnt-cover)
- [Actions](../language/09-policy.md#actions)
- [Lines that depend on a record](../language/09-policy.md#lines-that-depend-on-a-record)
- [Writing a decline](../language/01-project-layout-and-modules.md#writing-a-decline)
- [Acceptances](../language/18-records-and-manifest.md#acceptances)
