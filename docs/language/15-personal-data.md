---
title: Personal data
area: 15
status: decided
checked-with: this checkout's jadpo on every gate run
---

# Personal data
<!-- topic: personal-data -->

Your app says which data is about a person, and Jadpo does the rest: it
exports that data when the person asks for a copy, erases it when they ask
to be forgotten, deletes it when its time is up, and reports where it
lives. The aim is that requests under GDPR, UK GDPR and CCPA/CPRA work out
of the box. Built so far: `person: true`, the check that erasure can find
every field marked `personal:`, [retention](#retention), which deletes
data when its time is up, and [erasure](#erasure) of a person's records in
their store, the bytes of their files included and their own record kept
field by field where it must be. `personal:` is also read for
[the AI page's](21-ai.md#personal-data-sent-to-a-model) check of what
reaches a model, and [the report](#the-personal-data-report) lists every
connection and host personal data reaches. Nothing exports personal data
yet, and the report doesn't yet list the rest of what it will, so an app
on this compiler does those itself. `User.export` is
`toolchain.not_built_yet` where you write it, and so is any retention or
erasure this compiler can't carry out in full, so none can be mistaken
for working ([What is built](#what-is-built)).

The law asks a few things of an app:

- Keep only the personal data you need, for as long as you need it (GDPR
  Article 5).
- Give a person a copy of everything you hold about them, in a
  machine-readable form, within a month (Articles 12, 15 and 20).
- Delete it when they ask, unless the law makes you keep some of it, such
  as invoices (Article 17).
- Build this in from the start (Article 25), keep a record of what you
  hold and why (Article 30), and protect it (Article 32).

CCPA/CPRA gives Californians the same rights to know and to delete. Each of
these needs the app to know which data is personal and whose it is. That
is the one fact Jadpo asks you to write down.

| Capability | What you write | What the compiler does | Built yet |
|---|---|---|---|
| Marking | `personal:` on a field | Hashes it in logs, lists it in the report | Partly: read for the AI check and the report's connections, and checked that erasure can find it; nothing hashes it |
| The person | `person: true` on an entity | Finds every record that belongs to them | Yes |
| Erasure | `erase:` on references it can't work out | Generates `User.erase(id)` across every store and cache | Partly: in the person's store, files' bytes and the person's own record kept field by field included; not yet another store |
| Export | nothing | Generates `User.export(id)` | No: `toolchain.not_built_yet` |
| Retention | `retention:` on a field or entity | Clears or deletes it when the time is up | Partly: not yet on an event |
| Report | nothing | Writes `build/report/personal-data.md` | Partly: every connection and host personal data reaches |

## Marking personal fields
<!-- topic: marking-personal-fields -->

A field that identifies or describes a person says so in its block, with
`personal:` and one category from a fixed list. The category decides how
carefully the compiler treats the value. Here a user's email is contact
data and their display name identifies them:

```jadpo fragment
// domains/accounts/entities/user.jadpo
entity User {
    id: Uuid
    email: Email {
        unique: true
        personal: PersonalData.contact
    }
    display_name: DisplayName {
        personal: PersonalData.identifying
    }
    ...
}
```

Every marked field needs a category: there is no bare `personal: true`, and
you can't add categories of your own. These are the fourteen:

| `PersonalData.` | For | Sensitive |
|---|---|---|
| `identifying` | Names, usernames, account numbers | |
| `contact` | Email, phone, postal address | |
| `financial` | Bank and card details, income, purchases | |
| `online` | IP address, device and cookie identifiers, browsing | |
| `demographic` | Date of birth, gender, nationality | |
| `government_id` | Passport, national insurance, driving licence | yes |
| `location` | Precise location | yes |
| `health` | Physical or mental health, including disability | yes |
| `biometric` | Fingerprints, face data used to identify | yes |
| `genetic` | Genetic data | yes |
| `beliefs` | Religion, political opinions, trade union membership | yes |
| `ethnicity` | Racial or ethnic origin | yes |
| `sex_life` | Sex life or sexual orientation | yes |
| `criminal` | Criminal convictions and offences | yes |

"Sensitive" covers GDPR's special categories (Articles 9 and 10) and the
extra ones CCPA/CPRA lists: government IDs and precise location.

`personal:` goes on a field of an entity, an event or an email, or of an
object, input or output a package declares, wherever Jadpo keeps or sends the
value. A provider package marks
what its API takes and gives, so an app sees what it is sending and
receiving without reading the provider's documentation. Here the package
says a customer's phone number is contact data, whatever the app fills it
with:

```jadpo fragment
// packages/stripe/v2024_06_20/types/customer_request.jadpo
/// A customer to create.
public type CustomerRequest = Input {
    phone: Text {
        personal: PersonalData.contact
    }
}
```

The category is checked the same way everywhere (`type.setting_value`). An
app's own object, input or output carries no mark
(`type.unknown_setting`): it holds what a marked field gave it, and
Jadpo follows the value back to that field.

### Marked fields in logs

A marked field is written to logs as a keyed hash, whatever its type. So a
marked `date_of_birth: CalendarDate` is hashed, though
[logs](04-failures.md#logging) otherwise show dates and numbers as
values, and so is a marked field in a failure's context. The marking, not
the type, decides.

The hash covers the UTF-8 bytes of the value's canonical text, as
`text.from` gives it: `2026-10-25` for a date, `GBP 10.50` for an amount.
A type with no single text form, such as a `PostalAddress`, is hashed from
its JSON as `json.write` writes it. So one value hashes the same in every
log and on both executors, because every boundary goes through
[the standard types' contract](19-standard-library.md#every-standard-type-meets-one-contract).

### Sensitive fields

A sensitive field is never put in a cache, and every read of one is
recorded in the [access records](#access-records).

### Records a person owns

You don't mark every field of a record a person owns. A todo's title is
the user's data because the todo belongs to them (see
[The person](#the-person)), so it is exported and erased with them. Mark
only the fields whose value points at, or describes, a person.

## The person
<!-- topic: person-entities (was the-person) -->

An entity that represents a person says `person: true`. The law calls this
person the *data subject*: the one the data is about. Most apps have one
such entity, `User`. You can have several, so a clinic might also have
`Patient`, and a shop a guest `Customer`.

```jadpo fragment
entity User {
    ...
    person: true
}
```

You don't list which records belong to the person. A record **belongs
to** a person when it refers to them directly: a todo through its
`owner_id`. The compiler reads every reference to find them, and the
[report](#the-personal-data-report) shows them for you to review.

A record that refers to two people, such as a message from one user to
another, belongs to both. A record that refers only to another record
belongs to no one: a reply someone writes on another user's todo refers
to its author and to the todo, not to the todo's owner, so it belongs to
its author alone.

Erasure still reaches further than a person's own records, through what
it deletes ([Erasure](#erasure)). Here a user is a person, a todo belongs
to its owner, and an attachment goes with its todo. The attachment's
caption is personal data, and erasing the user deletes it:

```jadpo check project=person-records
type Caption = Text {
    max_length: 200
}

entity User {
    id: Uuid
    email: Email {
        unique: true
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
    title: Caption
}

entity Attachment {
    id: Uuid
    todo_id: Todo.id {
        on_delete: OnDelete.cascade
    }
    caption: Caption {
        personal: PersonalData.identifying
    }
}
```

`person:` takes only `true`; an entity that isn't a person leaves the
line out. A field may still be called `person`.

## Erasure
<!-- topic: erasure -->

Erasing a person is a generated operation, `User.erase(user_id)`. You call
it from an action you write, for example behind a "delete my account"
route or in a support tool. It erases the person's record and decides what
happens to every record that belongs to them.

What happens to each of those records follows from the `on_delete:`
setting of the [reference](05-data.md#references-and-on_delete) that
links it to the person, so most references need nothing more:

- `OnDelete.cascade` means the record is deleted (`Erasure.deleted`).
- `OnDelete.cleared` means the record is kept and anonymised
  (`Erasure.anonymised`).
- `OnDelete.restricted` must say what erasure does, with `erase:`.

A restricted reference must say, because "a user with todos can't be
deleted" is the right rule for a mistaken click, but not for a person's
legal request to be forgotten. Most apps write one or two `erase:` lines,
and the compiler refuses an erasure it can't complete.

| `erase:` on a reference to a person | What happens to the record | Needs |
|---|---|---|
| `Erasure.deleted` | Deleted, with everything that cascades from it | |
| `Erasure.anonymised` | Kept; the reference is cleared, and every marked field once no other person the record belongs to remains | the reference and marked fields nullable |
| `Erasure.retained` | Kept as it is until its own retention ends; only the reference is cleared | the reference nullable, and `retention:` on the entity |

A marked field is one with a `personal:` setting, which says it holds
personal data ([Marking personal fields](#marking-personal-fields)). A
record that belongs to someone else too keeps its marked fields until
the last of its people is erased
([Records that belong to several people](#records-that-belong-to-several-people)). A
nullable field is one whose type ends in `?`, so it can hold no value
([Values that may be missing](02-values-and-types.md#values-that-may-be-missing)).

Erasure follows the references from every record it deletes, however
deep, so a reference to one of the person's todos decides what erasure
does to its record in the same way. A record erasure keeps stays with
whatever refers to it, as it does when an ordinary delete clears a
reference. Everything goes in one statement, so an erasure is never left
half done.

### Deleting the person's records: `Erasure.deleted`

Use it for records that are only the person's, such as their todos.
Here `Todo`'s reference to its owner is restricted, so ordinary code can't
delete a user who still has todos, but erasing the owner deletes their
todos:

```jadpo check project=erasure file=app.jadpo hidden
use accounts/entities { User }

application {}

principal {
    user: {
        subject: User.email
    }
}

authentication {
    api_token: {}
}
```

```jadpo check project=erasure file=config.jadpo hidden
config {
    invoice_retention: Duration {
        default: 2192d
    }
}
```

```jadpo check project=erasure file=domains/accounts/entities/user.jadpo hidden
/// Someone with an account, who may erase only themselves.
entity User {
    id: Uuid
    email: Email {
        unique: true
        personal: PersonalData.contact
    }

    policy: {
        create: [Access.public]
        read: [id]
        delete: [id]
    }

    person: true
}
```

```jadpo check project=erasure file=domains/accounts/types/todo_title.jadpo hidden
/// What a todo says.
type TodoTitle = Text {
    max_length: 200
}
```

```jadpo check project=erasure file=domains/accounts/types/billing_name.jadpo hidden
/// A name as it is billed.
type BillingName = Text {
    max_length: 120
}
```

```jadpo check project=erasure file=domains/accounts/entities/todo.jadpo
// domains/accounts/entities/todo.jadpo
use accounts/entities { User }
use accounts/types { TodoTitle }

/// Something a user means to do, deleted with them.
entity Todo {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.restricted
        erase: Erasure.deleted
    }
    title: TodoTitle

    policy: {
        create: [owner_id]
        read: [owner_id]
    }
}
```

### Keeping a record without the person: `Erasure.retained`

Use it for records the law makes you keep. Invoices are the usual case:
tax law requires them for several years, which GDPR Article 17(3) allows.
The invoice keeps its own copy of the billing name and address, so it no
longer needs the user. Erasing the user clears `customer_id` and keeps the
invoice:

```jadpo check project=erasure file=domains/accounts/entities/invoice.jadpo migrate=initial
// domains/accounts/entities/invoice.jadpo
use accounts/entities { User }
use accounts/types { BillingName }

/// A bill, kept for the tax years without its customer.
entity Invoice {
    id: Uuid
    customer_id: User.id? {
        on_delete: OnDelete.restricted
        erase: Erasure.retained
    }
    billing_name: BillingName {
        personal: PersonalData.identifying
    }
    issued_at: Instant {
        generated: Generated.on_create
    }

    policy: {
        create: [customer_id]
        read: [customer_id]
    }

    retention: {
        after: config.invoice_retention
        since: issued_at
    }
}
```

The invoice is then deleted when its own [retention](#retention) ends,
here `config.invoice_retention` after it was issued (six years in the UK).

A record you keep this way holds its own copy of the details it needs,
as the invoice holds the billing name. It never points at a stub left in
place of the person. It also needs a `retention:` of its own, which says
when it is deleted.

### Erasing someone

An action erases a person by their `id`. Here a signed-in user closes
their own account, which `User`'s `delete: [id]` allows only for
themselves
([Who may erase and export](#who-may-erase-and-export)):

```jadpo check project=erasure file=domains/accounts/actions/close_account.jadpo
// domains/accounts/actions/close_account.jadpo
use accounts/entities { User, UserNotFound }

/// Erases the signed-in user: their todos are deleted, and their
/// invoices kept without them.
action close_account(me: Principal.user) fails UserNotFound -> Unit {
    attempt User.erase(me.id)
}
```

`User.erase` takes no block, since what it does to each record is
written on the references, and nothing can refuse it but policy. A
person who isn't there, or whom the caller may not delete, is the
person's not-found failure, `UserNotFound`. A person a lifecycle hides,
such as one who deleted their account earlier, is erased too.

This test signs a user up, gives them a todo and an invoice, and closes
their account:

```jadpo check project=erasure file=domains/accounts/failures/email_taken.jadpo hidden
/// Someone has that address already.
type EmailTaken = Failure {
    kind: FailureKind.conflict
}
```

```jadpo check project=erasure file=domains/accounts/actions/sign_up.jadpo hidden
use accounts/entities { User }
use accounts/failures { EmailTaken }

/// Adds a user.
action sign_up(email: User.email) fails EmailTaken -> User {
    return attempt User.create {
        set: {
            email: email
        }
        conflict: {
            User.email: EmailTaken
        }
    }
}
```

```jadpo check project=erasure file=domains/accounts/actions/add_todo.jadpo hidden
use accounts/entities { Todo }

/// Adds a todo.
action add_todo(me: Principal.user, title: Todo.title) -> Todo {
    return Todo.create {
        set: {
            owner_id: me.id
            title: title
        }
    }
}
```

```jadpo check project=erasure file=domains/accounts/actions/bill.jadpo hidden
use accounts/entities { Invoice }

/// Issues an invoice.
action bill(me: Principal.user, billing_name: Invoice.billing_name) -> Invoice {
    return Invoice.create {
        set: {
            customer_id: me.id
            billing_name: billing_name
        }
    }
}
```

```jadpo check project=erasure file=domains/accounts/actions/close_account.test.jadpo
// domains/accounts/actions/close_account.test.jadpo
use accounts/entities { Invoice, Todo, TodoNotFound, User, UserNotFound }
use accounts/actions { add_todo, bill, sign_up }

test "closing an account deletes the todos and keeps the invoices" {
    var ada: User = attempt sign_up("ada@example.com")
    var me: Principal.user = Principal.user(ada)
    var todo: Todo = add_todo(me, "Buy milk")
    var invoice: Invoice = bill(me, "Ada Lovelace")
    attempt close_account(me)
    assert User.get(ada.id) fails UserNotFound
    assert Todo.get(todo.id) fails TodoNotFound
    var still_billed: Invoice = attempt Invoice.get(invoice.id)
    assert still_billed.customer_id == none
    assert still_billed.billing_name == "Ada Lovelace"
}
```

### Records that belong to several people

A record can belong to more than one person: a postcard from one user
to another belongs to both. Erasing one person never clears another
person's data. Erasing either user clears only their own reference, and
the postcard keeps its marked fields while the other remains; they are
cleared when the last of them is erased.

The people a record belongs to are those it refers to directly. A reply
someone writes on another user's todo belongs to its author alone, so
erasing the todo's owner clears only the reply's `todo_id`, and erasing
its author clears its marked fields too. A record kept that refers to no
person at all has no one left, so it loses its marked fields when
erasure deletes what it refers to.

Another person's own record is never anonymised: a user whose
`invited_by_id` refers to the person erased keeps their own fields, and
only the reference is cleared.

```jadpo check project=erasure file=domains/accounts/types/card_text.jadpo hidden
/// What a postcard says.
type CardText = Text {
    max_length: 500
}
```

```jadpo check project=erasure file=domains/accounts/types/signed_name.jadpo hidden
/// The name a postcard is signed with.
type SignedName = Text {
    max_length: 80
}
```

```jadpo check project=erasure file=domains/accounts/entities/postcard.jadpo migrate=postcards
// domains/accounts/entities/postcard.jadpo
use accounts/entities { User }
use accounts/types { CardText, SignedName }

/// A postcard one user sends another, which belongs to both of them.
entity Postcard {
    id: Uuid
    sender_id: User.id? {
        on_delete: OnDelete.cleared
    }
    recipient_id: User.id? {
        on_delete: OnDelete.cleared
    }
    body: CardText
    signed_as: SignedName? {
        personal: PersonalData.identifying
    }

    policy: {
        create: [sender_id]
        read: [sender_id, recipient_id]
    }
}
```

```jadpo check project=erasure file=domains/accounts/actions/write_postcard.jadpo hidden
use accounts/entities { Postcard, User }

/// Sends a postcard.
action write_postcard(
    me: Principal.user,
    to: User,
    body: Postcard.body,
    signed_as: Postcard.signed_as
) -> Postcard {
    return Postcard.create {
        set: {
            sender_id: me.id
            recipient_id: to.id
            body: body
            signed_as: signed_as
        }
    }
}
```

```jadpo check project=erasure file=domains/accounts/actions/write_postcard.test.jadpo
// domains/accounts/actions/write_postcard.test.jadpo
use accounts/entities { Postcard, User }
use accounts/actions { close_account, sign_up }

test "a postcard keeps its signature until both its users are erased" {
    var ada: User = attempt sign_up("ada@example.com")
    var bob: User = attempt sign_up("bob@example.com")
    var card: Postcard = write_postcard(Principal.user(ada), bob, "See you on Tuesday", "Ada")
    attempt close_account(Principal.user(ada))
    var after_ada: Postcard = attempt Postcard.get(card.id)
    assert after_ada.sender_id == none
    assert after_ada.signed_as == "Ada"
    attempt close_account(Principal.user(bob))
    var after_both: Postcard = attempt Postcard.get(card.id)
    assert after_both.signed_as == none
    assert after_both.body == "See you on Tuesday"
}
```

A reply shows the difference:

```jadpo check project=erasure file=domains/accounts/entities/reply.jadpo migrate=replies
// domains/accounts/entities/reply.jadpo
use accounts/entities { Todo, User }
use accounts/types { CardText, SignedName }

/// What someone writes on a todo, which belongs to whoever wrote it.
entity Reply {
    id: Uuid
    todo_id: Todo.id? {
        on_delete: OnDelete.cleared
    }
    author_id: User.id? {
        on_delete: OnDelete.cleared
    }
    body: CardText
    signed_as: SignedName? {
        personal: PersonalData.identifying
    }

    policy: {
        create: [author_id]
        read: [author_id]
    }
}
```

```jadpo check project=erasure file=domains/accounts/actions/write_reply.jadpo hidden
use accounts/entities { Reply, Todo }

/// Writes on a todo.
action write_reply(
    me: Principal.user,
    todo: Todo,
    body: Reply.body,
    signed_as: Reply.signed_as
) -> Reply {
    return Reply.create {
        set: {
            todo_id: todo.id
            author_id: me.id
            body: body
            signed_as: signed_as
        }
    }
}
```

```jadpo check project=erasure file=domains/accounts/actions/write_reply.test.jadpo
// domains/accounts/actions/write_reply.test.jadpo
use accounts/entities { Reply, Todo, User }
use accounts/actions { add_todo, close_account, sign_up }

test "a reply's signature goes with its author, not the todo's owner" {
    var ada: User = attempt sign_up("ada@example.com")
    var bob: User = attempt sign_up("bob@example.com")
    var todo: Todo = add_todo(Principal.user(ada), "Buy milk")
    var reply: Reply = write_reply(Principal.user(bob), todo, "Which kind?", "Bob")
    attempt close_account(Principal.user(ada))
    var after_owner: Reply = attempt Reply.get(reply.id)
    assert after_owner.todo_id == none
    assert after_owner.signed_as == "Bob"
    attempt close_account(Principal.user(bob))
    var after_author: Reply = attempt Reply.get(reply.id)
    assert after_author.signed_as == none
    assert after_author.body == "Which kind?"
}
```

### The person's own record

The person's own record is erased field by field, because it can't always
go: an insurer, for example, must keep policy records for at least five
years.

- Where nothing must be kept, the record is deleted.
- Otherwise a field that says `erase: Erasure.retained` stays, and so does
  every field not marked `personal:`. Every other marked field is cleared.

Here erasing a customer clears their name and keeps their policy number
until their retention ends:

```jadpo check project=kept-record file=app.jadpo hidden
use cover/entities { Customer }

application {}

principal {
    user: {
        subject: Customer.handle
    }
}

authentication {
    api_token: {}
}
```

```jadpo check project=kept-record file=config.jadpo hidden
config {
    policy_retention: Duration {
        default: 1826d
    }
}
```

```jadpo check project=kept-record file=domains/cover/types/policy_number.jadpo hidden
/// A policy's number.
type PolicyNumber = Text {
    max_length: 20
}
```

```jadpo check project=kept-record file=domains/cover/types/customer_name.jadpo hidden
/// What someone is called.
type CustomerName = Text {
    max_length: 120
}
```

```jadpo check project=kept-record file=domains/cover/types/account_handle.jadpo hidden
/// What someone signs in with, which says nothing about them.
type AccountHandle = Text {
    max_length: 40
}
```

```jadpo check project=kept-record file=domains/cover/entities/customer.jadpo
// domains/cover/entities/customer.jadpo
use cover/types { AccountHandle, CustomerName, PolicyNumber }

/// Someone with a policy, part of whose record the law makes us keep.
entity Customer {
    id: Uuid
    handle: AccountHandle {
        unique: true
    }
    customer_name: CustomerName? {
        personal: PersonalData.identifying
    }
    policy_number: PolicyNumber {
        personal: PersonalData.identifying
        erase: Erasure.retained
    }
    policy_ended_at: Instant?

    policy: {
        create: [Access.public]
        read: [Access.public]
        delete: [id]
    }

    person: true

    retention: {
        after: config.policy_retention
        since: policy_ended_at
    }
}
```

- A field that stays is marked `personal:`, since an unmarked field stays
  anyway ([`data.erase_own_field`](../errors/data.erase_own_field.md)).
- No reference of theirs deletes them with another entity's records, since
  what the law keeps doesn't depend on who else is erased
  ([`data.erase_own_field`](../errors/data.erase_own_field.md)). Their
  references to records erasure deletes are cleared.
- The entity needs a `retention:`, or what stays would stay for ever
  ([`data.erase_retention_required`](../errors/data.erase_retention_required.md)).
  It applies to every record, erased or not, so count from something like
  the end of the policy: a record whose `since:` field is `none` isn't
  counted, and a customer whose policy hasn't ended stays. When the time
  is up, the [purge schedule](12-jobs-and-events.md#the-purge-schedule)
  deletes the rest.
- Every marked field that doesn't stay can be `none`
  ([`data.erase_field_required`](../errors/data.erase_field_required.md)).
  What someone [signs in with](08-identity.md) must always have a value,
  so it stays too, or holds nothing personal, as `handle` does here.
- A column [retired](05-data.md#removing-data) on a record whose marked
  fields erasure clears is cleared with them, since nothing says any more
  whether it held personal data. A sealed field
  ([Encrypted fields](#encrypted-fields)) is treated like any other.

A record kept this way is hidden, as a [lifecycle](05-data.md#lifecycle)
hides one. No query, `get` or include finds it, and no update, delete or
transition reaches it, so the person receives nothing. Their sessions, API
tokens, service keys, passkeys and sign-in links go with the erasure, and
they can't sign in again, however they try: every way in looks them up, and
finds no one. Erasing them again finds them and changes nothing.

### Reading what is kept: `include_erased:`

A query that serves the reason the record was kept says so, with
`include_erased: true`. It finds erased people as well as everyone else.
It may read nothing but their `id` and the fields that say `erase:
Erasure.retained`, in its result, its conditions and its order, and it
brings no other records with it
([`query.include_erased_reads`](../errors/query.include_erased_reads.md)).
The personal-data report lists every such query.

```jadpo check project=kept-record file=domains/cover/types/kept_policy.jadpo
// domains/cover/types/kept_policy.jadpo
use cover/entities { Customer }

/// What the law makes us keep of a customer.
type KeptPolicy = Output {
    pick: [Customer.id, Customer.policy_number]
}
```

```jadpo check project=kept-record file=domains/cover/queries/kept_policies.jadpo
// domains/cover/queries/kept_policies.jadpo
use cover/entities { Customer }
use cover/types { KeptPolicy }

/// Whoever holds a policy number, erased or not, for a regulator's audit.
query kept_policies(number: Customer.policy_number) -> List<KeptPolicy> {
    from: Customer
    where: Customer.policy_number == number
    order: [Customer.id asc]
    limit: 10
    include_erased: true
}
```

On an entity that keeps nothing of an erased person there is nothing for
it to find, so it is refused
([`query.include_erased_unused`](../errors/query.include_erased_unused.md)).

```jadpo check project=kept-record file=domains/cover/failures/handle_taken.jadpo hidden
/// Someone signs in with that already.
type HandleTaken = Failure {
    kind: FailureKind.conflict
}
```

```jadpo check project=kept-record file=domains/cover/actions/take_out_policy.jadpo hidden
use cover/entities { Customer }
use cover/failures { HandleTaken }

/// Takes out a policy.
action take_out_policy(
    handle: Customer.handle,
    customer_name: Customer.customer_name,
    number: Customer.policy_number
) fails HandleTaken -> Customer {
    return attempt Customer.create {
        set: {
            handle: handle
            customer_name: customer_name
            policy_number: number
            policy_ended_at: none
        }
        conflict: {
            Customer.handle: HandleTaken
        }
    }
}
```

```jadpo check project=kept-record file=domains/cover/actions/close_account.jadpo hidden migrate=initial
use cover/entities { Customer, CustomerNotFound }

/// Erases the signed-in customer.
action close_account(me: Principal.user) fails CustomerNotFound -> Unit {
    attempt Customer.erase(me.id)
}
```

```jadpo check project=kept-record file=domains/cover/actions/close_account.test.jadpo
// domains/cover/actions/close_account.test.jadpo
use cover/entities { Customer, CustomerNotFound }
use cover/actions { take_out_policy }
use cover/queries { kept_policies }

test "closing an account hides the customer and keeps the policy number" {
    var ada: Customer = attempt take_out_policy("ada", "Ada Lovelace", "P-100")
    attempt close_account(Principal.user(ada))
    assert Customer.get(ada.id) fails CustomerNotFound
    assert list.map(kept_policies("P-100"), kept -> kept.policy_number) == ["P-100"]
}
```

### What erasure reaches

- **Every store and cache.** The main store's records go in one
  transaction, and cached copies are wiped at once
  ([Caches](05-data.md#caches)). An action can write to only one store,
  so records your app keeps in another store are erased straight after,
  by an event [subscriber](12-jobs-and-events.md#subscribers) that Jadpo
  generates. Such a record refers to the person with a reference that
  says `erase:` and has no `on_delete:`. It isn't a foreign key, since no
  database holds one across stores: the subscriber applies what `erase:`
  says in that store, after the main store commits.
  Any other reference across stores, with an `on_delete:` or to an entity
  that isn't a person, is
  [`data.reference_across_stores`](../errors/data.reference_across_stores.md).
- **Records with a lifecycle** are removed outright, not soft-deleted.
  Your own code can't do that, but erasure, like purge, is an operation
  the compiler generates, so it can
  ([Lifecycle](05-data.md#lifecycle)).
- **Files** attached to erased records are deleted with them
  ([Files](16-built-in-services.md#files)).
- **History.** An audited record erasure deletes takes its history with
  it, in the same statement
  ([Audit trail](16-built-in-services.md#audit-trail)).
- **OAuth grants**, the permissions a person gave your app to act for them
  at a provider, are deleted with the person or record that owns them
  ([OAuth grants](11-integrations.md#acting-for-each-user-oauth-grants)).
  They are revoked at the provider where its package declares how.
- **Other companies.** Data already sent to a provider, such as an email
  or payment provider, is outside your database. Providers that can erase
  are asked to (see "Erasure at providers" below). The
  [personal-data report](#the-personal-data-report) lists every connection that receives a marked field, and whether its
  copy is erased.

### What erasure doesn't rewrite

- **Backups** aren't rewritten. Each erasure leaves an entry in an erasure
  log, holding the person entity's name, the person's `id` and the time
  and nothing else. Your app names where the log is kept, in `privacy: {
  erasure_log: { store: } }`: a store that holds no person's records, or a
  store of files, so restoring a person's database can't take the log back
  with it. The runtime replays the log every time it starts, erasing again
  anyone logged who is still there, so a restored backup is erased before
  the app serves anything. Backups themselves expire on the hosting
  platform's schedule, which the report states. This is what the UK
  regulator, the ICO, calls putting data "beyond use".
- **There are no per-person encryption keys**, so erasure never works by
  deleting a person's key ("crypto-shredding").
- **Logs** aren't rewritten either. They hold identifiers and hashes,
  never marked values, and expire on their own retention.

### Legal holds and restricted use

A legal hold, which stops a person's data being erased while a case is
open, is application data that you write. An action checks a hold flag and
refuses with its own failure before it calls `User.erase`. Restricting use
of someone's data while a complaint is open (Article 18) works the same
way.

### Erasure at providers

When your app sends a person's data to another company, such as a payment
provider, erasing the person also asks that provider to erase them. Two
declarations make that work. The provider's package declares, in its
`provider.jadpo`, which call erases a person and what it takes. Your app's
connection names the field that holds the person's key at that provider:

```jadpo fragment
// packages/stripe/v2024_06_20/provider.jadpo
provider {
    ...
    erasure: {
        call: customers.delete
        by: CustomerId
    }
}

// app.jadpo
stripe: {
    package: Stripe.v2024_06_20
    domains: [payments]
    credential: config.stripe_secret_key
    erasure: {
        key: User.stripe_customer_id
    }
}
```

Erasure emits a `PersonErased` event, which holds the person entity's name,
the person's `id` and when they were erased, and nothing personal, besides
the keys below. The `erasure:` is a declaration on the package, not part of
any [capability](11-integrations.md#capabilities), so Stripe and Postmark
are erased the same way.

- **Your app first, then the providers.** When `User.erase` runs, Jadpo
  copies each connection's key, here `stripe_customer_id`, into the
  erasure event. It then erases the person in one transaction, as above.
  After that commits, a generated subscriber calls each provider's erasure
  operation, retried until the provider confirms. So a provider never
  holds the only record of someone your app still has.
- **The completion email waits for every provider.** The
  erasure-completed email, one of the
  [built-in emails](16-built-in-services.md#built-in-emails), goes only
  once every provider has confirmed. A delivery that gives up is parked
  for an operator and shown in the privacy request's state and in
  `jadpo privacy requests`. It is never left quietly incomplete.
- **A copied key that is itself personal data**, such as an email address
  used as the provider's key, stays only in the outbox until it is
  delivered. The personal-data report lists it there.
- **A connection that receives personal data but can't erase it** is a
  compile error. That is a connection whose package declares no
  `erasure:`, or which names no key. An
  [acceptance](18-records-and-manifest.md#acceptances), a committed file
  in which a person states why the data stays there (tax records at an
  accounting provider, say), allows it, and the report lists the
  connection and the reason.
- **Anything else** your app sends data to, such as a CRM it calls
  through its own integration, can be erased by your own `PersonErased`
  subscribers.

## Export
<!-- topic: personal-data-export (was export) -->

`User.export(user_id)` returns everything held about the person: their
record, every record that belongs to them and the records erasure deletes
with those, as JSON, one list per entity, with the same field names as the
API. You write nothing for it
beyond calling it:

```jadpo fragment
// domains/accounts/actions/export_my_data.jadpo
/// Gives the signed-in user a copy of everything held about them.
/// The route passes the caller in: `run: export_my_data(current_principal.id)`.
action export_my_data(user_id: User.id) -> PersonalDataExport {
    return User.export(user_id)
}
```

The export covers both the right of access and the right to take your
data elsewhere ("portability", Article 20). Credentials, such as token
hashes, are left out. It also says how long each kind of data is kept and
which companies receive it, taken from the report, because Article 15
asks for that too.

### Who may erase and export

[Policy](09-policy.md) decides who may call `erase` and `export`.
`export` needs read access to the person's record, and `erase` needs
delete access ([Grants](09-policy.md#grants)). Once a caller is allowed,
both reach every record that belongs to the person, as purge does,
because a partial export or erasure is worse than none.

## Retention
<!-- topic: retention -->

Personal data needs an end date: GDPR's storage limitation means you keep
it only as long as you need it. Nothing expires unless you say so: a value
or record without a `retention:` is kept until your code deletes it or the
person is erased. A `retention:` block gives the end date, in the same
`after:` and `since:` shape as a lifecycle's `purge:`
([Lifecycle](05-data.md#lifecycle)): how long to keep it, `after:`,
counted from one of the record's `Instant` fields, `since:`.

### On a field

The value is cleared when its time is up, and the record stays. Use it for
a value you need only for a while, such as the address a visitor signed
up from, kept for 30 days for fraud checks:

```jadpo check project=retention-field
config {
    signup_ip_retention: Duration {
        default: 30d
    }
}

entity Visitor {
    id: Uuid
    signup_ip: IpAddress? {
        personal: PersonalData.online
        retention: {
            after: config.signup_ip_retention
            since: created_at
        }
    }
    created_at: Instant {
        generated: Generated.on_create
    }

    person: true
}
```

Once a visitor signed up more than 30 days ago, `signup_ip` becomes
`none`, whether a lifecycle hides the visitor or not. Clearing it is a
change to the record, so its `Generated.on_change` fields take the time
of the run that cleared it.

The field must be one that can be `none`: its type ends in `?`
([Values that may be missing](02-values-and-types.md#values-that-may-be-missing)),
or retention could never clear it
([`data.retention_field_required`](../errors/data.retention_field_required.md)).
It can't be a field the entity's [lifecycle](05-data.md#lifecycle) owns
or reads, such as the `deleted_at` its `visible:` tests, because clearing
it would show a deleted record again or undo what a transition set
([`data.retention_lifecycle_field`](../errors/data.retention_lifecycle_field.md)).

### On an entity

The record is deleted, with whatever cascades from it. Use it for
invoices, as in [Erasure](#keeping-a-record-without-the-person-erasureretained),
or for accounts inactive for two years. Here an invoice goes six years
after it was issued, and its lines go with it:

```jadpo check project=retention-entity
config {
    invoice_retention: Duration {
        default: 2192d
    }
}

type InvoiceLineText = Text {
    max_length: 200
}

entity Invoice {
    id: Uuid
    number: InvoiceNumber
    issued_at: Instant {
        generated: Generated.on_create
    }

    retention: {
        after: config.invoice_retention
        since: issued_at
    }
}

entity InvoiceLine {
    id: Uuid
    invoice_id: Invoice.id {
        on_delete: OnDelete.cascade
    }
    text: InvoiceLineText
}
```

A record is deleted whether a lifecycle hides it or not, and so is every
record that cascades from it, records with a lifecycle among them, since
retention, like purge, is an operation the compiler generates. References
that are `OnDelete.cleared` are set to `none`, as for any delete
([References and `on_delete`](05-data.md#references-and-on_delete)).

A record an `OnDelete.restricted` reference holds, directly or through
what deleting it cascades to, can't be deleted, so its retention is an
error ([`data.retention_held`](../errors/data.retention_held.md)): say
what deleting it does to the record that holds it.

An entity that someone signs in as, the `user:` or `service:` of
[`principal { }`](08-identity.md), goes with its sign-in state when its
retention ends, or another record's retention cascades to it: its
sessions and API tokens, service keys, passkeys and sign-in links are
deleted in the same statement, so nobody can sign in as them again.

A `File` field's retention clears the field, and a record's deletes the
record, cascades included; either way the file's bytes are deleted once
no record holds them
([Deleting and erasing](16-built-in-services.md#deleting-and-erasing)).

### Where the periods come from

The periods are settings in `config.jadpo`
([Settings](10-configuration-and-time.md#settings-configjadpo)), so they
are set per environment and appear in the report. `after:` names a
`Duration` setting that isn't secret, never a duration written in place
([`data.retention_period`](../errors/data.retention_period.md)). A run uses
the value the app started with, and a period that isn't above zero
deletes and clears nothing: the run fails instead.

`since:` names one of the record's `Instant` fields, bare, as a lifecycle
names them ([`data.retention_since`](../errors/data.retention_since.md)).
A record whose `since:` field is `none` isn't counted yet, so it is kept
until the field has a value and that time is past.

### When it runs

The compiler's built-in [purge schedule](12-jobs-and-events.md#the-purge-schedule)
applies retention, every hour, so an entity with a `retention:`, its own
or a field's, has a schedule such as `Invoice.purge` even without a
lifecycle. A value or record goes at the first run once its `since:` is
longer ago than `after:`. Each run applies the entity's retention first,
then its lifecycle's `purge:`, then each field's retention, in small
batches, each logged, oldest first; the next hour's run carries on with
what one leaves. Its run record counts the records it deleted and the
values it cleared, and it emits no events.

Retention changes the database only. An event that was emitted holding a
value keeps its copy until the runtime removes the event, once every
subscriber has had it and a day has passed since it was emitted
([Events](12-jobs-and-events.md#declaring-an-event)), and a backup keeps
what it held until it expires on the hosting platform's schedule.

No test runs retention, as none runs purge: the purge schedule has no
name a test can write, and moving a test's clock on runs no schedule
([Schedule tests](13-testing.md#schedule-tests)). It is the compiler's
own operation, proven by the compiler's live tests on both executors.

A person entity, or an entity holding marked fields, with no retention
and no purge is listed in the report as "kept until erased".

## What the compiler checks
<!-- topic: personal-data-checks (was what-the-compiler-checks) -->

- **Marked fields nothing reads.** A personal field that no query, output
  or action reads is an error, because you are holding data you don't use
  (data minimisation, Article 5). You can
  [decline](01-project-layout-and-modules.md#declining-a-judgement-call)
  it, with a reason, because a field kept for a legal reason may be read
  only by export.
- **Erasure is complete.** Every reference into a person's records has an
  erasure outcome: a `restricted` one into records erasure deletes says
  what it does
  ([`data.erase_required`](../errors/data.erase_required.md)).
  `Erasure.anonymised` and `Erasure.retained`, on a reference or on the
  person's own field, have the nullable fields
  ([`data.erase_field_required`](../errors/data.erase_field_required.md))
  and retention
  ([`data.erase_retention_required`](../errors/data.erase_retention_required.md))
  they need. An `erase:` erasure never reads is refused
  ([`data.erase_unreached`](../errors/data.erase_unreached.md)), and so
  is one on the person's own field that says anything but
  `Erasure.retained`, or on an unmarked one
  ([`data.erase_own_field`](../errors/data.erase_own_field.md)).
- **What is kept is read only for the reason it is kept.** A query saying
  `include_erased: true` reads only `id` and the retained fields
  ([`query.include_erased_reads`](../errors/query.include_erased_reads.md)),
  on an entity that keeps someone's record
  ([`query.include_erased_unused`](../errors/query.include_erased_unused.md)).
- **No personal data erasure can't reach.** A marked field on an entity
  erasing a person never reaches is
  [`data.personal_unreachable`](../errors/data.personal_unreachable.md).
  Erasure reaches a person, the records that refer to them, and the
  records that refer to what it deletes, never past a record it keeps. So
  a marked field that erasure can't find needs you to refer to the person,
  or to mark the entity `person: true`. A field is marked here only by
  its `personal:`: a
  standard type that is personal data whatever holds it, such as a
  `PhoneNumber`, is hashed in logs and its reads recorded, but a shop's
  own phone number doesn't make the shop a person. With the completeness
  check above, every stored copy of personal data can be reached.
- **Sensitive fields stay out of caches.** A cache on an entity that holds
  one leaves those fields out.
- **Retention can be carried out.** A field's retention clears a field
  that can be `none` and that no lifecycle owns or reads, an entity's
  deletes records nothing `restricted` holds, and every period is a
  `Duration` setting ([Retention](#retention)).

There is no `purpose:` setting and no consent model in the language.
Purposes and lawful bases belong in your privacy notice, which you write
from the report. Consent is ordinary application data, such as a `Consent`
entity holding the purpose and when it was given and withdrawn.

## Data requests
<!-- topic: data-requests -->

`privacy:` in `app.jadpo` turns on generated routes for a person's
requests to export or erase their data. Jadpo generates the endpoints
only, never the pages: your frontend builds the page, and a recipe can
show you one. There are three ways a request starts.

### Self-service by email

`privacy: { self_service: {} }` turns it on. Anyone can ask by email
address, without signing in:

1. Your frontend posts the address to `POST /privacy/requests`
   (`address`). It answers `202` for every address, so it never reveals
   who has an account, and sends an email to check the address.
2. The email's single-use link opens your frontend's path, set as
   `self_service: { link: "/privacy/confirm" }`, at the browser origin,
   with a token.
3. That page posts `POST /privacy/requests/confirm` (`token`, and `kind`:
   `export` or `erasure`) to create the request.
4. The erasure confirmation email has a cancel link, which opens
   `self_service: { cancel_link: "/privacy/cancel" }`. That page posts
   `POST /privacy/requests/cancel` (`token`).

Loading a frontend page uses up nothing, so a mail scanner that follows
the links confirms and cancels nothing.

### Signed in

`POST /privacy/export` and `POST /privacy/erasure` need a recent sign-in
([`RouteAuthentication.fresh`](08-identity.md#check-the-caller-again-routeauthenticationfresh))
and act only on the caller's own record: there is no person id to pass.
Export answers `202` and delivers a download link when it is ready.

### Made by staff

Requests received by email or phone are made through generated actions,
after staff have checked who the person is. You grant them in
`policy.jadpo`, as `privacy.create_request: [StaffRole.support]`. Each
request records who created it and how the person's identity was
checked. The export still goes to the person's own email, never to the
staff member.

A schedule or subscriber never makes a request. The only erasure that
happens on a schedule is retention's.

### Exports

An export is delivered as a single-use download link that expires after 7
days, never as an attachment. The file is encrypted at rest and deleted
when the link expires, and every download is in the access records. You
set where the files are kept in `privacy: { exports: { store: keep: } }`.

### Erasures

An erasure waits out a cooling-off period your app sets, 14 days by
default, and the confirmation email has a cancel link. The completion
email says what was kept and why, since the person must be told. So an
entity marked `Erasure.retained` needs a wording in its domain's language
files, which the email lists.

### Deadlines and events

Every request is a record with its one-month legal deadline.
`jadpo privacy requests` and the console list them, and one nearing its
deadline raises an alert.

Each step emits an event: `PrivacyRequestReceived`, `PrivacyExportReady`
(with the download link), `PrivacyErasureScheduled`,
`PrivacyErasureCompleted` and `PrivacyRequestOverdue`. The built-in
emails are subscribers to these events. Your app can add its own, to
post to Slack or a CRM, say, or replace the emails. With no mail
connection, the compiler requires a subscriber that delivers
`PrivacyExportReady`.

### Protection

The routes have [rate limits](16-built-in-services.md#rate-limits), their
links are single-use and short-lived, and every step is in the access
records.

### Changing the routes

These routes take `path:` and `routes: false` like every
[generated route](07-http.md#generated-routes). They run over generated
`privacy.request`, `privacy.confirm` and `privacy.cancel` operations,
which keep every protection.

### The privacy emails

The privacy emails are the address check, export ready, erasure confirmed
and erasure completed. They are [built-in emails](16-built-in-services.md#built-in-emails),
which you can change as much as you need, in five levels:

1. Jadpo's own email, as it comes.
2. Its words, changed in the root `translations/` folder under
   `jadpo: { }` ([Translations](10-configuration-and-time.md#jadpos-and-packages-text)).
3. Your app's shared email layout.
4. Your app's own `EmailMessage`, with `email:`.
5. Your app's own sending, with `send:`.

They go through your app's one `email:` connection in `application { }`,
and need `application { display_name: }`.

Each email has a built-in type, and a request type for `send:`:
`PrivacyAddressCheckEmail`, `PrivacyExportEmail`,
`PrivacyErasureConfirmedEmail` and `PrivacyErasureCompletedEmail`. You set
them in `privacy: { emails: { address_check: { } export_ready: { }
erasure_confirmed: { } erasure_completed: { } } }`. Each entry takes
`email:` or `send:`, never both. Their fields are on the
[Built-in services](16-built-in-services.md#built-in-emails) page.

## Access records
<!-- topic: access-records -->

Every export and erasure is recorded: who asked, for which person's `id`,
and when, never the values. So is every read of a sensitive field. These
are log events kept for a long time, so your organisation can show a
regulator what it did.

Reads of ordinary personal data aren't recorded one by one.

## Where data is stored
<!-- topic: where-data-is-stored -->

Each store is one database in one place. Data that must stay in the EU
lives in a deployment whose stores are in the EU. An app serving several
regions runs one deployment per region ([Stores](05-data.md#stores)).

There is no region setting in source. Instead, the report lists each
store with the configuration setting that says where it is.

## The personal-data report
<!-- topic: the-personal-data-report -->

`jadpo check` writes `build/report/personal-data.md` and a `.json` twin,
in the same form as its other reports ([The report](14-tooling.md#the-report)).
For each person entity it lists:

- every marked field, with its category, entity, store and caches;
- the queries that say `include_erased: true`;
- who may read it, from policy;
- its retention, or "kept until erased";
- what erasure does to each record that belongs to the person;
- every connection a marked field is sent to, and whether its copy is
  erased by the provider's `erasure:` or kept for an accepted reason;
- keys copied into the outbox for erasure at providers, while they wait;
- the step records of actions with
  [several changing calls](12-jobs-and-events.md#several-changing-calls-in-one-action),
  which hold providers' answers, encrypted, until the action commits (an
  answer over 1 MB is a fault);
- connections holding OAuth grants, and whose grants they are;
- sensitive fields, flagged.

That is most of a record of processing activities (Article 30). Your team
adds what the compiler can't know: the purpose of each use, the lawful
basis, and the organisation's details.

Changes to the report appear in the approval review, the summary of a
change a person signs off before it ships
([Review and sign-off](14-tooling.md#review-and-sign-off)). So a pull
request that starts sending email addresses to a new provider is seen by
a person before it ships.

### Where personal data goes

So far the report lists each connection personal data leaves through, the
hosts that connection reaches and, for each one, every marked field and
every standard type that is personal data sent through it, by what sends it.
It is `build/report/personal-data.md`, with a `.json` twin, and `jadpo
check` writes it only for a project with no errors. Where a body doesn't
check, nothing can say what it sends, so a check that fails takes the last
report away rather than leave one that says less than the code does.

A connection is listed for each of the ways a value leaves the app:

| Through | What counts |
|---|---|
| A provider's operation | What it is given, and what its input's type holds, a package's marks among them |
| A package's action | Everything it is given, whatever the action then does, and what its own operations send |
| A store of files | What goes in a file `put` there, and each row a writer puts in one |
| A model | Each prompt built and each text sent for an embedding, through the connection `ai:` names |
| An email | Each one emitted, and a magic link's address, through the connection `email:` names |
| A webhook's reply | What the reply or a failure's client field holds, through the connection the webhook came from |

The report never says data stays when it may leave. When the compiler can't
tell, it counts as reaching:

- The compiler follows a value as it does for
  [what reaches a model](21-ai.md#following-personal-data): through
  variables, your app's functions and actions, standard operations, lists,
  objects, a batch's items and a query's `Output`. A parameter is followed
  to what every caller passes, and an event's part to what every `emit`
  put there.
- What decided a value leaves with it. A value made under an `if` or a
  `match` on a marked field, or a call made under one, sends that field, and
  the report says it only decided what was sent.
- A value counts for what its type says, however it got there: a value of a
  marked field's type, `caller.subject` among them, and a whole parameter by
  everything its type holds. So an action only a route calls counts for its
  input's type.
- A library's function gives back anything it was given, in any part, since
  nothing follows that code.
- A provider's answer may hold anything its call was sent, in any part. So
  an id a provider made counts as the name it was made from when it is sent
  on to another connection. This is for the report alone: storing that id or
  sending it to a model doesn't need the name's marks.

Stores and caches aren't in this list. What each keeps comes with the rest
of the report.

`jadpo package check` shows the same for a package, whoever calls it: what
each of its operations sends, from its input's type and what its actions and
subscribers read off marked fields, and the hosts it goes to
([What a release shows](11-integrations.md#what-a-release-shows)).

## Encrypted fields
<!-- topic: encrypted-fields -->

An entity field can be stored encrypted. It is sealed with your app's one
`field_encryption_key:` when it is written and opened when it is loaded,
so your code reads it as it reads any field, and a copy of the database
holds only sealed bytes. A clinic keeps each consultation's notes that way:

```jadpo check project=encrypted-notes file=app.jadpo
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
    field_encryption_key: config.field_encryption_key
}
```

```jadpo check project=encrypted-notes file=config.jadpo
config {
    database_url: PostgresUrl
    field_encryption_key: EncryptionKey
}
```

```jadpo check project=encrypted-notes file=domains/clinic/types/clinical_note.jadpo hidden
/// What a clinician wrote.
type ClinicalNote = Text {
    max_length: 2000
}
```

```jadpo check project=encrypted-notes file=domains/clinic/entities/consultation.jadpo
use clinic/types { ClinicalNote }

/// A consultation, whose notes only the application can read.
entity Consultation {
    id: Uuid
    notes: ClinicalNote {
        encrypted: true
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
```

```jadpo check project=encrypted-notes file=domains/clinic/actions/record_consultation.jadpo hidden
use clinic/entities { Consultation }

/// Records a consultation.
action record_consultation(notes: Consultation.notes) -> Consultation {
    return Consultation.create {
        set: {
            notes: notes
        }
    }
}
```

```jadpo check project=encrypted-notes file=domains/clinic/actions/record_consultation.test.jadpo migrate=initial
use clinic/entities { Consultation }

test "a consultation's notes read back as they were written" {
    var written_up: Consultation = record_consultation("Allergic to penicillin.")
    assert written_up.notes == "Allergic to penicillin."
    var read_back: Consultation = attempt Consultation.get(written_up.id)
    assert read_back.notes == "Allergic to penicillin."
}
```

A field of any stored type may be `encrypted: true`, whatever columns it
would have had: it is kept as its type's JSON, sealed, in one `bytea`, so
it holds as much as a structured field does. Each value is bound to its
column and its record's `id`, so moved to another row it opens for no one,
and one value sealed twice gives different bytes.

A key type, a `TotpSecret` and a `SecretText` are always kept sealed, in a
field of their own, whether or not the field says so, and `encrypted:
false` on one is refused
([How a secret is kept](19-standard-library.md#how-a-secret-is-kept)).
There are no per-person keys (see [Erasure](#what-erasure-doesnt-rewrite)).
Keys can also be held as data: a key per device or person in a key-type
field, usable only as a `crypto` call's `key:`
([A key is never a value](19-standard-library.md#a-key-is-never-a-value)).

Every field sent to an AI model is listed in the report above
([Following personal data](21-ai.md#following-personal-data)).

### What a sealed field can't do

Only your app can read a sealed column, and its bytes change every time it
is written, so nothing that has the database read it can work.
`data.encrypted_field_used` refuses a sealed field in:

- a unique rule or an index;
- a query's filter, ordering or group;
- a search's `fields:`;
- a lifecycle's conditions and the fields it sets, and a retention's or a
  purge's `since:`;
- a value the database works out in an update by `id`;
- a computed backfill, which is SQL or runs without the key;
- a failure's fields, since a failure is written to the log.

What the database itself has to read can't be sealed at all:
`data.encrypted_field_invalid` refuses `encrypted:` on the `id`, a
reference, a generated field and a `File`.

Reading one in an action and answering with it is fine, and policy decides
who may. A field's `retention:`, and erasure, clear a sealed value as they
clear any. The history of an [audited](16-built-in-services.md#audit-trail)
record says a sealed field changed and never what it held. Spans and
runtime logs carry no field's value at all.

### The key

`field_encryption_key:` is written at the top of `application { }`, and
reads an `EncryptionKey` setting: 32 random bytes, written as 43 Base64url
characters. A `SigningKey` won't do (`data.field_encryption_key_invalid`).
Your app needs one only when it has an encrypted, key-type, `TotpSecret` or
`SecretText` field, and then `data.field_encryption_key_required` says so at
the first of them. The key your fields are sealed with is derived from the
setting for this one use, so a `crypto.encrypt` call of your own under the
same setting never meets them.

| Where | How it is given |
|---|---|
| Your machine | `jadpo config set field_encryption_key --generate` writes 32 random bytes into `.env.local` and prints nothing. `jadpo config check` then says `set`, and `jadpo dev` refuses to start until it is |
| A native server | `FIELD_ENCRYPTION_KEY` in the environment or from a secret manager. `jadpo config generate field_encryption_key` makes one, only into a pipe; at a terminal it refuses |
| Workers | A Wrangler secret, which `jadpo build` lists in `secrets.required`: `jadpo config generate field_encryption_key \| wrangler secret put FIELD_ENCRYPTION_KEY` |
| Tests | `jadpo test` makes a throwaway key for each run, as it does every secret |

### Rotating the key

To replace the key without taking the app down:

1. Set the new key as `FIELD_ENCRYPTION_KEY`, and move the old one to
   `FIELD_ENCRYPTION_KEY_PREVIOUS`. New values are sealed with the new key,
   and values sealed with either read.
2. An app that keeps anything sealed has a built-in schedule,
   `field_encryption.reseal`, that runs every hour. While `_PREVIOUS` is set
   it rewrites rows still under the old key, at most 20 batches of 500 a run,
   and changes nothing else about them. `jadpo schedules` lists it, and
   `jadpo schedules run field_encryption.reseal` starts a run.
3. `jadpo config rotation` counts each sealed field's rows against both keys.
   When it says `FIELD_ENCRYPTION_KEY_PREVIOUS can go`, remove it.

A key held in a key-type field rotates by being replaced: store a new one,
as `crypto.new_key(as: EncryptionKey)` makes it.

### What can't be undone

Sealing trades one risk for another, and some of what follows has no fix:

- **Losing the key loses every sealed value for good.** Backups don't help,
  because they hold the same sealed bytes. Keep an offline copy of the key,
  somewhere apart from your backups.
- **Removing `_PREVIOUS` too soon, or rotating twice before the re-seal has
  finished, loses the rows still under the older key.** Removal is safe only
  once `jadpo config rotation` says so. Each rotation starts with a run of
  it.
- **A restored backup needs the key it was written with.** Keep each old key
  for as long as you keep a backup written under it.
- **A key leaked together with a copy of the database can't be fixed by
  rotating.** Rotating protects what is written from then on, and the copy
  still opens with the old key. Treat that as a breach of everything sealed.
- **Turning `encrypted:` on or off needs a data migration.** `jadpo migrate
  new` refuses it, since stored values would have to be sealed or opened one
  by one with a key SQL never sees. Add a new field with the setting you
  want, copy each record's value across in an action, then retire the old
  field. Its column keeps what it held, in the clear when you are turning
  encryption on, until you [drop it](05-data.md#removing-data). The same goes
  for a new required sealed field, and a narrower type on one.

## What this does not cover
<!-- topic: what-this-does-not-cover -->

The report says plainly what Jadpo covers and what it doesn't:

- **Covered by the language:** minimisation and storage limitation,
  access, rectification (by ordinary updates), erasure, portability,
  privacy by design, records of processing (most of Article 30), and
  security of processing.
- **Supported, but your app's choice:** restriction and objection, and
  consent.
- **Not code at all:** the lawful basis, the privacy notice, data
  protection impact assessments (DPIAs), processor contracts, and the data
  protection officer (DPO).
- **Still to add:** breach notification, which lists the people whose
  marked data a breached store or provider held, from the report and
  access records; and international transfers, where provider packages
  declare where data is processed and the report shows it.

This is not legal advice. A privacy professional reviews the report.

## What is built
<!-- topic: personal-data-built -->

| Feature | Status |
|---|---|
| `personal: PersonalData.<category>` on an entity's, an event's or an email's field, or an object's, input's or output's a package declares | Partly built: it is read and checked against the fourteen categories for the [AI](21-ai.md) page's check of what reaches a model and for [where personal data goes](#where-personal-data-goes), and a marked field on an entity erasure never reaches is `data.personal_unreachable`. On an app's own object, input or output it is `type.unknown_setting`. Not built yet: the rest of what this page says marking does. A marked field nothing reads compiles without an error, and no log or access record uses the marking. |
| `person: true` | Built: an entity marked `person: true` is a person, and every record that refers to one directly belongs to them. The build's manifest shows each person under its entity as a `person` node, listing the references its records belong through (`through`) and the fields erasure keeps of their record (`retained`). |
| `retention:` on a field or an entity | Partly built. Built: the entity's hourly purge schedule clears a nullable field's value, or deletes the record with what cascades from it and the sign-in state of everyone it deletes who signs in, cascades included, at the first run once `since:` is longer ago than `after:`, a `Duration` setting; a `File`'s bytes go once no record holds it ([Files](16-built-in-services.md#deleting-and-erasing)); each problem under [Retention](#retention) is an error where it is written. The graph holds a `retention` node for each, under its field or entity, with its period and `since:`. Not built yet: `retention:` on an event's or an email's field, `toolchain.not_built_yet` where it is written. |
| `erase:` on a reference or the person's own field | Built: each problem under [Erasure](#erasure) and "Erasure is complete" is an error where it is written. Not built yet: a reference to a person from another store, which stays [`data.reference_across_stores`](../errors/data.reference_across_stores.md) until a generated subscriber can apply it. |
| `include_erased:` on a query | Built: it finds the people erasure keeps a record of too, and is held to what they keep. The [report](#the-personal-data-report) that lists these queries isn't built yet. |
| `User.erase(id)` | Partly built. Built: for a caller with `delete` on the person's record, one statement deletes the record, hidden or not, with the sign-in state of anyone it deletes who signs in, and deletes, anonymises or keeps each record of theirs as its reference says, however deep, records with a lifecycle removed outright. A record that belongs to several people keeps its marked fields until the last of them is erased, and another person's own record loses only the reference ([Records that belong to several people](#records-that-belong-to-several-people)). The person's own record stays field by field, hidden, where one of their fields says so ([The person's own record](#the-persons-own-record)). The bytes of each `File` it deletes or clears go once it commits ([Files](16-built-in-services.md#deleting-and-erasing)), and an audited record it deletes takes its history with it. Not built yet: one that would keep an audited record, whose history still refers to the person ([Audit trail](16-built-in-services.md#audit-trail)), `toolchain.not_built_yet` where `erase` is called, so no erasure is ever partial; and records in another store, `PersonErased`, the erasure log and providers' erasure, which nothing emits or calls. |
| `User.export(id)` | Not built yet: it is `toolchain.not_built_yet` where it is called. |
| `privacy:` in `application { }` | Not built yet: it is `toolchain.not_built_yet` where it is written. |
| Encrypted fields | Built: `encrypted: true`, key-type, `TotpSecret` and `SecretText` fields, sealed on write and opened on read, interpreted, compiled and on Workers; each refusal above; rotation through `_PREVIOUS`, with `field_encryption.reseal` and `jadpo config rotation`. |
| The personal-data report | Partly built: `jadpo check` writes `build/report/personal-data.md` and `.json`, for a project with no errors, listing [where personal data goes](#where-personal-data-goes), and `jadpo package check` shows the same for a package. Not built yet: the rest of its list, each person entity's fields, category, store and readers, retention, what erasure does, keys in the outbox, step records and OAuth grants; whether a connection can erase its copy, and the acceptance for one that can't; and its diff in the approval review. |
| Everything else on this page | Not built yet: it is on [the roadmap](../roadmap.md). |

## Errors

Every error this page's checks produce, with an example that triggers
it and the compiler's real output from `jadpo check`.

- [`data.encrypted_field_invalid`](../errors/data.encrypted_field_invalid.md)
- [`data.encrypted_field_used`](../errors/data.encrypted_field_used.md)
- [`data.erase_field_required`](../errors/data.erase_field_required.md)
- [`data.erase_own_field`](../errors/data.erase_own_field.md)
- [`data.erase_required`](../errors/data.erase_required.md)
- [`data.erase_retention_required`](../errors/data.erase_retention_required.md)
- [`data.erase_unreached`](../errors/data.erase_unreached.md)
- [`data.field_encryption_key_invalid`](../errors/data.field_encryption_key_invalid.md)
- [`data.field_encryption_key_required`](../errors/data.field_encryption_key_required.md)
- [`data.personal_unreachable`](../errors/data.personal_unreachable.md)
- [`data.retention_field_required`](../errors/data.retention_field_required.md)
- [`data.retention_held`](../errors/data.retention_held.md)
- [`data.retention_lifecycle_field`](../errors/data.retention_lifecycle_field.md)
- [`data.retention_period`](../errors/data.retention_period.md)
- [`data.retention_since`](../errors/data.retention_since.md)
- [`query.include_erased_reads`](../errors/query.include_erased_reads.md)
- [`query.include_erased_unused`](../errors/query.include_erased_unused.md)
