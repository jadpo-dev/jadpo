---
title: Policy
area: 9
status: decided
checked-with: this checkout's jadpo on every gate run
---

# Policy
<!-- topic: policy -->

Policy says who may do what to which stored records. Each entity, a kind
of stored record such as a todo, lists who may create, read, change and
delete its records. The compiler applies those lists to every read and
write your code makes, so you never write a permission check yourself.

This todo may be created, read, changed and deleted only by its owner:

`domains/todos/entities/todo.jadpo`:

```jadpo check project=policy-first file=domains/todos/entities/todo.jadpo
use todos/entities { User }

/// Something to do, belonging to one person.
entity Todo {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.restricted
    }
    title: Text {
        max_length: 200
    }

    policy: {
        create: [owner_id]
        read: [owner_id]
        update: [owner_id]
        delete: [owner_id]
    }
}
```

`owner_id` is a reference to a `User`, so the person it points to is the
one let in. A caller no list names is refused, and a todo they may not
read behaves as if it didn't exist ([When policy says no](#when-policy-says-no)).

`entity`, `policy:`, `Uuid`, `Text` and `OnDelete` are built into Jadpo.
`Todo` and `User` are your app's own, and `use` brings `User` in from its
own file. Your app says which entity is a person in `app.jadpo`, which is
what lets a list mean "the signed-in user"
([The principal](08-identity.md#the-principal)):

`domains/todos/entities/user.jadpo`:

```jadpo check project=policy-first file=domains/todos/entities/user.jadpo
/// A person with an account.
entity User {
    id: Uuid
    email: Email {
        unique: true
    }

    policy: {
        create: [Access.system]
        read: [id]
    }
}
```

`app.jadpo`:

```jadpo check project=policy-first file=app.jadpo migrate=initial
use todos/entities { User }

application {}

principal {
    user: {
        subject: User.email
    }
}
```

Each entry in a list names who it lets in, in one of four ways:

| Who | Written | Lets in |
|---|---|---|
| [A user field on the record](#a-user-field-on-the-record) | `owner_id` | the user that field points to |
| [A membership role](#a-membership-role) | `CompanyRole.editor` | a user with that role in the record's company |
| [Anyone signed in, or anyone](#anyone-signed-in-or-anyone) | `Access.authenticated`, `Access.public` | anyone signed in; anyone at all |
| [Policy and background work](#policy-and-background-work) | `Access.system` | schedules and subscribers, limited to the tenant their event came from |

When your app is shared by many customer companies, [Tenancy](#tenancy)
keeps each company's records to its own members, whatever the lists say.

## Grants
<!-- topic: grants -->

A grant says who may do one thing to an entity's records. An entity's
`policy:` holds up to four lists of them, one for each thing: `create`,
`read`, `update` and `delete`. Anyone a list names is enough to let a
caller in, and anything not listed is refused. So "who can delete a
project?" is one line in the project's file. In the `Todo` at the top of
[Policy](#policy), `delete: [owner_id]` lets the todo's owner delete it,
and nobody else.

Policy is always on. From the first line of code, every create, read,
update or delete your code can reach needs a grant.

### Which grant an action needs

You never write which grant an action needs. The compiler sees what each
query and action reads and writes. Every action that updates a `Todo`
needs `update` on it, so ten actions that edit todos share one line of
policy. An action that reaches several entities needs a grant on each.
When an action calls another domain's action, the called action is
checked against its own entities, with the same caller.

### Transitions and deletes

A lifecycle transition, a named change of a record's state such as
archiving it ([Lifecycle](05-data.md#lifecycle)), is an update. The one
exception is deleting: `Todo.delete(todo)` needs `delete`, whether it
removes the record or runs the lifecycle's `delete` transition
([The operations every entity gets](06-reads-and-writes.md#the-operations-every-entity-gets)).
So a soft delete, which keeps the record but hides it, still needs
`delete`.

Bringing a record back needs both. A transition marked `reaches: hidden`
([Restoring a hidden record](05-data.md#restoring-a-hidden-record)) needs
`update`, as every transition does, and, when the lifecycle has a `delete`
transition, `delete` as well, so nobody can undo a delete they couldn't
have made. Without a `delete` transition, the records it reaches were
hidden by other transitions or by time, which `update` governs, so
`update` alone decides. An action listed under `actions:` has its line
decide both, as it decides every write in its own body
([Permissions on actions](#permissions-on-actions)). A caller who lacks
either gets the record's not-found failure, as for any record they may not
change ([When policy says no](#when-policy-says-no)).

Here responders work on a support desk's enquiries, and only its owners may
bin one or take it out of the bin:

```jadpo check project=restore-by-role file=app.jadpo hidden
use desk/entities { Member }

application {}

principal {
    user: {
        subject: Member.handle
    }
}
```

```jadpo check project=restore-by-role file=authentication.jadpo hidden
authentication {
    browser_session: {
        cookie: "desk_session"
    }
}
```

```jadpo check project=restore-by-role file=policy.jadpo hidden
use desk/entities { Desk }
use desk

policy {
    tenant: Desk
    actions: {
        desk.found_desk: [Access.authenticated]
    }
}
```

```jadpo check project=restore-by-role file=domains/desk/types/handle.jadpo hidden
type Handle = Text {
    min_length: 3
    max_length: 40
}
```

```jadpo check project=restore-by-role file=domains/desk/types/desk_name.jadpo hidden
type DeskName = Text {
    min_length: 1
    max_length: 80
}
```

```jadpo check project=restore-by-role file=domains/desk/types/desk_role.jadpo hidden
/// What a member may do on a desk.
type DeskRole = Enum {
    owner
    responder
}
```

```jadpo check project=restore-by-role file=domains/desk/types/enquiry_title.jadpo hidden
type EnquiryTitle = Text {
    min_length: 1
    max_length: 120
}
```

```jadpo check project=restore-by-role file=domains/desk/failures/handle_taken.jadpo hidden
type HandleTaken = Failure {
    kind: FailureKind.conflict
    message: "That handle is taken."
}
```

```jadpo check project=restore-by-role file=domains/desk/entities/member.jadpo hidden
use desk/types { Handle }

/// Someone who signs in.
entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
        tenant: Tenancy.shared
    }
}
```

```jadpo check project=restore-by-role file=domains/desk/entities/desk.jadpo hidden
use desk/types { DeskName, DeskRole }

/// A front_desk desk: the tenant every other record belongs to.
entity Desk {
    id: Uuid
    name: DeskName

    policy: {
        read: [DeskRole.owner, DeskRole.responder]
    }
}
```

```jadpo check project=restore-by-role file=domains/desk/entities/desk_membership.jadpo hidden
use desk/entities { Desk, Member }
use desk/types { DeskRole }

/// A member's role on a desk.
entity DeskMembership {
    id: Uuid
    desk_id: Desk.id {
        on_delete: OnDelete.cascade
    }
    member_id: Member.id {
        on_delete: OnDelete.cascade
    }
    role: DeskRole

    membership: {
        scope: desk_id
        member: member_id
        role: role
    }

    policy: {
        create: [DeskRole.owner]
        read: [DeskRole.owner, member_id]
        delete: [DeskRole.owner, member_id]
    }
}
```

```jadpo check project=restore-by-role file=domains/desk/entities/enquiry.jadpo
use desk/entities { Desk }
use desk/types { DeskRole, EnquiryTitle }

/// A customer's question. Responders work on enquiries; only owners bin
/// them.
entity Enquiry {
    id: Uuid
    desk_id: Desk.id {
        on_delete: OnDelete.cascade
    }
    title: EnquiryTitle
    binned_at: Instant?

    lifecycle: {
        visible: binned_at == none
        transitions: {
            delete: {
                when: binned_at == none
                set: {
                    binned_at: clock.now
                }
            }
            restore: {
                reaches: hidden  // needs `update` and `delete`
                set: {
                    binned_at: none
                }
            }
        }
        purge: {
            after: 30d
            since: binned_at
        }
    }

    policy: {
        create: [DeskRole.owner, DeskRole.responder]
        read: [DeskRole.owner, DeskRole.responder]
        update: [DeskRole.owner, DeskRole.responder]
        delete: [DeskRole.owner]
    }
}
```

```jadpo check project=restore-by-role file=domains/desk/types/enquiry_view.jadpo hidden
use desk/entities { Enquiry }

/// An enquiry as members see it.
type EnquiryView = Output {
    pick: [Enquiry.id, Enquiry.title]
}
```

```jadpo check project=restore-by-role file=domains/desk/actions/register.jadpo hidden
use desk/entities { Member }
use desk/types { Handle }
use desk/failures { HandleTaken }

/// Signs a new member up; anyone may.
action register(handle: Handle) fails HandleTaken -> Member {
    return attempt Member.create {
        set: {
            handle: handle
        }
        conflict: {
            Member.handle: HandleTaken
        }
    }
}
```

```jadpo check project=restore-by-role file=domains/desk/actions/found_desk.jadpo hidden
use desk/entities { Desk, DeskMembership }
use desk/types { DeskName, DeskRole }

/// Starts a desk with its founder as its owner.
action found_desk(name: DeskName, founder: Principal.user) -> Desk {
    var founded: Desk = Desk.create {
        set: {
            name: name
        }
    }
    var owner: DeskMembership = DeskMembership.create {
        set: {
            desk_id: founded.id
            member_id: founder.id
            role: DeskRole.owner
        }
    }
    assert owner.role == DeskRole.owner
    return founded
}
```

```jadpo check project=restore-by-role file=domains/desk/actions/add_member.jadpo hidden
use desk/entities { Desk, DeskMembership, DeskNotFound, Member, MemberNotFound }
use desk/types { DeskRole }

/// Gives a member a role on a desk; only its owners may.
action add_member(
    desk_id: Desk.id,
    member_id: Member.id,
    role: DeskRole
) fails DeskNotFound, MemberNotFound -> DeskMembership {
    return attempt DeskMembership.create {
        set: {
            desk_id: desk_id
            member_id: member_id
            role: role
        }
    }
}
```

```jadpo check project=restore-by-role file=domains/desk/actions/retitle_enquiry.jadpo hidden
use desk/entities { Enquiry, EnquiryNotFound }
use desk/types { EnquiryTitle, EnquiryView }

/// What retitling an enquiry takes.
type RetitleEnquiry = Input {
    title: EnquiryTitle
}

/// Gives an enquiry a clearer title; owners and responders may.
action retitle_enquiry(
    enquiry_id: Enquiry.id,
    input: RetitleEnquiry
) fails EnquiryNotFound -> EnquiryView {
    var changed: Enquiry = attempt Enquiry.update(enquiry_id) {
        set: {
            title: input.title
        }
    }
    return EnquiryView {
        id: changed.id
        title: changed.title
    }
}
```

```jadpo check project=restore-by-role file=domains/desk/actions/bin_enquiry.jadpo hidden
use desk/entities { Enquiry, EnquiryNotFound }

/// Moves an enquiry to the bin; only owners may.
action bin_enquiry(enquiry_id: Enquiry.id) fails EnquiryNotFound -> Unit {
    attempt Enquiry.delete(enquiry_id)
}
```

```jadpo check project=restore-by-role file=domains/desk/actions/restore_enquiry.jadpo
use desk/entities { Enquiry, EnquiryNotFound }
use desk/types { EnquiryView }

/// Takes an enquiry out of the bin; only those who may bin it may.
action restore_enquiry(enquiry_id: Enquiry.id) fails EnquiryNotFound -> EnquiryView {
    var changed: Enquiry = attempt Enquiry.restore(enquiry_id)
    return EnquiryView {
        id: changed.id
        title: changed.title
    }
}
```

```jadpo check project=restore-by-role file=domains/desk/routes/enquiries.jadpo migrate=initial hidden
use desk/entities { Enquiry }
use desk/types { EnquiryView }
use desk/actions { RetitleEnquiry, bin_enquiry, restore_enquiry, retitle_enquiry }

/// A desk's enquiries.
routes Enquiries {
    prefix: /enquiries

    /// Retitles an enquiry.
    PATCH /{enquiry_id: Enquiry.id} {
        input: RetitleEnquiry
        output: EnquiryView
        run: retitle_enquiry(enquiry_id, input)
    }

    /// Moves an enquiry to the bin.
    DELETE /{enquiry_id: Enquiry.id} {
        status: SuccessStatus.no_content
        run: bin_enquiry(enquiry_id)
    }

    /// Takes an enquiry out of the bin.
    POST /{enquiry_id: Enquiry.id}/restore {
        output: EnquiryView
        run: restore_enquiry(enquiry_id)
    }
}
```

```jadpo check project=restore-by-role file=api/main.jadpo_record hidden
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

api main
    authentication browser_session

PATCH /enquiries/{enquiry_id: Uuid}
    input RetitleEnquiry (body)
    output EnquiryView
    status 200
    fails enquiry_not_found

DELETE /enquiries/{enquiry_id: Uuid}
    status 204
    fails enquiry_not_found

POST /enquiries/{enquiry_id: Uuid}/restore
    output EnquiryView
    status 200
    fails enquiry_not_found

GET /openapi.json
    generated openapi
    status 200

output EnquiryView
    id: Uuid
    title: Text { min_length: 1, max_length: 120 }

input RetitleEnquiry
    title: Text { min_length: 1, max_length: 120 }

failure enquiry_not_found
    status 404
```

The responder may retitle the enquiry, but binning it and bringing it back
are both not found to them:

```jadpo check project=restore-by-role file=domains/desk/routes/enquiries.test.jadpo
use desk/entities { Desk, DeskMembership, Enquiry, Member }
use desk/types { DeskRole }
use desk/actions { RetitleEnquiry, add_member, found_desk, register }

test "a responder may retitle an enquiry, but not bin it or bring it back" {
    var ann: Member = attempt register("ann")
    var bob: Member = attempt register("bob")
    var front_desk: Desk = found_desk("Support", Principal.user(ann))
    var bobs_seat: DeskMembership = attempt add_member(front_desk.id, bob.id, DeskRole.responder)
    assert bobs_seat.role == DeskRole.responder
    var printer: Enquiry = make.Enquiry {
        desk_id: front_desk.id
        title: "Printer jam"
    }
    var retitled: Response = request {
        method: HttpMethod.patch
        path: "/enquiries/{enquiry_id}"
        params: {
            enquiry_id: printer.id
        }
        as: Principal.user(bob)
        input: RetitleEnquiry {
            title: "Printer jam on floor 2"
        }
    }
    assert retitled.status == 200
    var binned_by_bob: Response = request {
        method: HttpMethod.delete
        path: "/enquiries/{enquiry_id}"
        params: {
            enquiry_id: printer.id
        }
        as: Principal.user(bob)
    }
    assert binned_by_bob.status == 404
    var binned: Response = request {
        method: HttpMethod.delete
        path: "/enquiries/{enquiry_id}"
        params: {
            enquiry_id: printer.id
        }
        as: Principal.user(ann)
    }
    assert binned.status == 204
    var restored_by_bob: Response = request {
        method: HttpMethod.post
        path: "/enquiries/{enquiry_id}/restore"
        params: {
            enquiry_id: printer.id
        }
        as: Principal.user(bob)
    }
    assert restored_by_bob.status == 404
    assert restored_by_bob.code == "enquiry_not_found"
    var back_again: Response = request {
        method: HttpMethod.post
        path: "/enquiries/{enquiry_id}/restore"
        params: {
            enquiry_id: printer.id
        }
        as: Principal.user(ann)
    }
    assert back_again.status == 200
}
```

### Asking for an upload slot

`Entity.field_upload()` gives a client somewhere to send a file for one of
the entity's `File` fields ([Uploading](16-built-in-services.md#uploading)).
Asking for one needs the `create` or `update` grant on the entity. Anyone
either list names may ask. A grant that tests a record's fields, such as a
user field or a role held in the record, has no record to test yet, so it
lets in anyone signed in; the write that attaches the file is held to the
grant in full.

## A user field on the record
<!-- topic: a-user-field-on-the-record -->

Use a user field when a record belongs to one person, such as a todo and
its owner. A user field is a reference to the user entity, the entity your
app's [principal](08-identity.md#the-principal) names for the people who
sign in. Naming it in a list, such as `read: [owner_id]`, lets in the user
it points to. A reference to the principal's service entity lets in that
service in the same way.

On the user entity itself, `id` names the caller's own record:
`read: [id]` lets each person read only their own.

### On a create

A create is checked against the new record. So `create: [owner_id]` means
a caller can create a todo only if they are its owner. A request that
names someone else as the owner is refused.

### The field can't be changed by an update

A user field that any list names can't be changed by an ordinary update,
since changing it would hand the record to someone else. You don't mark it
`immutable:` for that. To hand a record to someone else, write a named
action and list it in root `policy.jadpo`, as [Permissions on actions](#permissions-on-actions) shows.

## A membership role
<!-- topic: a-membership-role -->

Use a role when people get access by belonging to something, such as the
editors of a company. A role is stored in a record of its own: a
membership entity, whose `membership:` section names three of its fields:

- `scope`: the record the role is held in, such as the company;
- `member`: who holds it, a reference to the user entity;
- `role`: the field that holds the role, typed with an enum.

The role enum lives beside the membership in the same domain, and other
domains use it by its full name. There is no global roles file.

```jadpo fragment
// domains/accounts/types/company_role.jadpo
type CompanyRole = Enum {
    owner
    editor
    viewer
}

// domains/accounts/entities/company_membership.jadpo
entity CompanyMembership {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    user_id: User.id {
        on_delete: OnDelete.cascade
    }
    role: CompanyRole

    membership: {
        scope: company_id
        member: user_id
        role: role
    }

    policy: {
        create: [CompanyRole.owner]
        read: [CompanyRole.owner, user_id]
        update: [CompanyRole.owner]
        delete: [CompanyRole.owner, user_id]
    }
}
```

Here only a company's owners may add, change or remove members. Each
member may also read their own membership, and delete it to leave.

### Using a role on other records

A role applies to another entity through that entity's reference to the
scope, the same reference [tenancy](#tenancy) uses. A `Project` with a
`company_id` takes its grants from the roles held in its company:

```jadpo fragment
// domains/projects/entities/project.jadpo
entity Project {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    name: ProjectName

    policy: {
        create: [CompanyRole.editor]
        read: [CompanyRole.viewer, CompanyRole.editor]
        update: [CompanyRole.editor]
        delete: [CompanyRole.owner]
    }
}
```

### How roles combine

- Roles have no ranking. A role lets in only where a list names it, so an
  owner who may also edit is listed under both.
- Only current memberships count. A membership its lifecycle hides grants
  nothing, and removing one takes effect on the next request.
- A role held across the whole application, such as staff, has a
  membership with `scope: MembershipScope.application`.
- An entity can have two references to the scope, such as a handover with
  a `from_team_id` and a `to_team_id`. Then each grant names the reference
  it means: `TeamRole.lead(from_team_id)` is the lead of the from-team.

### Roles that need a way in

Staff often sign in through their company's provider, such as Google or
Microsoft, so that switching their account off there ends their access
everywhere. If they could also sign in by magic link and keep their
roles, it wouldn't. A membership's `requires:` says, for each role, how
its holder must have signed in for the role to count:

```jadpo fragment
// domains/desk/entities/staff_member.jadpo
entity StaffMember {
    id: Uuid
    patron_id: Patron.id {
        on_delete: OnDelete.cascade
    }
    role: StaffRole

    membership: {
        scope: MembershipScope.application
        member: patron_id
        role: role
        requires: {
            StaffRole.admin: {
                sign_in: [SignInMethod.google]
                strength: AuthenticationStrength.multi_factor
            }
            StaffRole.front_desk: {
                sign_in: [SignInMethod.google]
            }
        }
    }
}
```

- `sign_in:` lists the ways in, any of which will do: a provider that
  `sign_in: { providers: }` names ([Providers](08-identity.md#providers)),
  or `SignInMethod.external_token`. A magic link or a passkey proves an
  address or a device, not that the company still vouches for the
  person, so neither can be listed.
- `strength: AuthenticationStrength.multi_factor` asks for a second
  factor: a passkey, or a provider whose ID token says the person used
  one ([How strongly a person signed in](08-identity.md#how-strongly-a-person-signed-in)).
- `StaffRole.front_desk: {}` needs nothing. Once one role has a line,
  every value of the enum has one, so a role added later is decided
  rather than open by default.

A role counts on a request only when the caller's current session meets
every line of its block. Otherwise the person is just themselves,
without that role: Ann, signed in by link, is a customer like any other.
Nothing is refused when she signs in. The requirement applies wherever
the role is named: grants, a field's own `policy:`, lines under
`actions:`, listed queries and what the proofs count on. You write it
once.

A membership whose role doesn't count doesn't make its holder a member
of its tenant for that request either, so tenancy fails closed
([Tenancy](#tenancy)).

A session started through a provider that any role's requirement names
lasts a day, not 30 days, so a person the company switches off loses the
role within a day ([How a session started](08-identity.md#how-a-session-started)).

The permissions report lists each role's requirement, and
`access.jadpo_record` records it. Any change to a role's requirement,
tighter or looser, and adding or removing a role's line, needs a person's
acceptance, `PolicyChange.role_requirements_changed`
([Acceptances](18-records-and-manifest.md#acceptances)).

What the compiler checks:

- A role or a way in the enum doesn't have is
  [`type.unknown_enum_variant`](../errors/type.unknown_enum_variant.md).
- A role left out once another has a line is
  [`policy.requirements_incomplete`](../errors/policy.requirements_incomplete.md).
- Anything else a role's line can't hold, an empty `sign_in:`,
  `strength: AuthenticationStrength.primary`, a way Jadpo's own sign-in
  gives, or a role given twice is
  [`policy.requirement_invalid`](../errors/policy.requirement_invalid.md).
- A line no way the app declares could ever meet, such as
  `SignInMethod.external_token` without `external_token:`, is
  [`policy.requirement_unsatisfiable`](../errors/policy.requirement_unsatisfiable.md).
- A route whose API can't carry the sign-in that every role letting
  anyone in needs is [`policy.effect_ungranted`](../errors/policy.effect_ungranted.md),
  which names the requirement. A provider's sign-in comes as a browser
  session and `external_token`'s as its token; an API token or a service
  key carries neither.

A route test says how its user signed in with `sign_in:` and `strength:`
beside `as:`; without them, `as:` is someone who signed in by magic link
with one factor ([Signing in for a route test](13-testing.md#signing-in-for-a-route-test)).

## Anyone signed in, or anyone
<!-- topic: anyone-signed-in-or-anyone -->

Use these for records that everyone signed in, or the whole world, may
see, such as published articles. `Access.authenticated` lets in anyone who
is signed in. `Access.public` lets in anyone at all, signed in or not.

```jadpo fragment
entity Article {
    ...
    policy: {
        read: [Access.public]
        create: [CompanyRole.editor]
        update: [CompanyRole.editor]
        delete: [CompanyRole.owner]
    }
}
```

Anyone may read an article. Only the company's editors may create or
change one, and only its owners may delete one.

A public read also needs a route that skips sign-in, one that says
`authentication: RouteAuthentication.none`
([Routes: signed in by default](08-identity.md#routes-signed-in-by-default)).
Either alone isn't enough: the route lets in callers who aren't signed in,
and the grant lets them read the record. A project's first record holding
personal data anyone may read says so to a person
([What a first record holds](18-records-and-manifest.md#what-a-first-record-holds)).

There is no administrator who bypasses policy.

## Policy and background work
<!-- topic: policy-and-background-work (was background-work) -->

Schedules and event subscribers are the work your app does without a
request ([Background work](12-jobs-and-events.md)). They run as the system
caller, which gets exactly what `Access.system` grants. To let them read or
change an entity's records, name `Access.system` in its lists, such as
`update: [owner_id, Access.system]`.

The system is nobody signed in, and it holds no membership. So:

- a user field, a role and `Access.authenticated` never let it in;
- a grant list, a field's own list or an `actions:` line lets it in only
  by naming `Access.system`.

### `Access.public` doesn't let background work in

An entity grants background work explicitly. A schedule, a subscriber or
a batch that reaches a statement only `Access.public` grants is a compile
error, `policy.system_grant_missing`, in the same way that
`policy.effect_ungranted` refuses such a route.

### Nothing grants it

So is one that reaches a statement no list grants anyone, such as a create
on an entity with no `create:`. It could only ever be refused, and a route
reaching it is `policy.effect_ungranted`, so nothing could run it.

### A list naming only people

So is one that reaches a list naming only people: `create:
[Access.authenticated]`, `update: [owner_id]`, a role, a line under
`actions:` without `Access.system`, or a listed query, whose line can't
name it. What counts is what the code could reach, whichever branch runs,
as for a route anyone may call (`policy.public_grant_missing`).

So an action a route and a subscriber share is refused when the people's
write is on a branch the subscriber never takes. Split it: the subscriber
runs an action of its own, which the route's action may call
([How to fix it](../errors/policy.system_grant_missing.md#how-to-fix-it)).

### Which tenant background work reaches

On a tenant's records ([Tenancy](#tenancy)), a grant naming `Access.system`
lets in:

- a schedule's run in every tenant, since no tenant's event starts it;
- a subscriber within the tenant its event names, in the event's field
  of the tenant's `id` type;
- a batch's run within the tenant its saved values of that type name.

A subscriber or a batch whose value names no tenant reaches no tenant's
records, only those every tenant shares. A value names no tenant when
there is none, when it is `none`, or when its values name different
tenants. So "no tenant" means every tenant for a schedule, which names
none by design, and none for work that names its tenant by a value. [Jobs
and events](12-jobs-and-events.md) covers this under "Background work and
tenants".

A batch that saves no value of the tenant's `id` type, and reaches a
tenant's records, would walk nothing every time, so it is
[`policy.batch_tenant_missing`](../errors/policy.batch_tenant_missing.md).
Work for every tenant at once belongs in a schedule's own action.

The system never moves a record to another tenant, which needs a
membership there, and never founds a tenant's first membership.

### The system and `actions:` lines

A line under `actions:` that names `Access.system` lets the system in
beside whoever else the line names. A line naming only `Access.system`
still lets in no request.

No request is ever the system. Only the runtime's runner makes the system
caller. Every request, and every call a test makes, with a caller or
without one, runs as someone else.

### Actions the system runs inside a request

The runtime runs three of your app's actions as the system, even inside a
request:

- the magic-link `send:` action ([Magic links](08-identity.md#magic-links)),
  delivered as a subscriber of `jadpo.MagicLinkEmail` with its delivery's
  tenant, which is none;
- a webhook's reply action
  ([Webhooks that need a reply](11-integrations.md#webhooks-that-need-a-reply));
- the `on_first_sign_in:` action, which [Sign-in and identity](08-identity.md) runs
  for a person the app hasn't seen before.

None of them reaches a tenant's records, only those every tenant shares,
since no tenant's event starts them and "no tenant" means every tenant
only for a schedule. Each is let in only by `Access.system`. So when one
of them reaches a grant, or a line under `actions:`, that doesn't name
`Access.system`, that is `policy.system_grant_missing`, reported where the
action is named.

### Proving which tenant work is for

A subscriber or a batch's run works in the tenant a value names: the
event's field of the tenant's `id` type, or what the run saves of it. A
caller could pass the id of a tenant they don't belong to. So that value
must come from a record of that tenant the action read or wrote under its
caller's policy. Otherwise it is `policy.event_tenant_unproved`, reported
at the value.

The usual fix is to read the tenant first,
`attempt Household.get(household_id)`, or to name the tenant of a record
the action changes. In full, a value proves its tenant when it is one of
these:

- the tenant's own `id`, or the tenancy reference of one of the tenant's
  records, that the action, or an action or query it calls, read, created
  or changed. The tenancy reference is the field that ties a record to
  its tenant, such as `company_id`.
- the same, of a record the action is handed whole, which counts as read.
  A route passes path placeholders, `input` and the caller, and a
  subscriber passes its event's fields. So only a body that read a record
  with the same caller, or a run limited to its tenant, can hand one to an
  action. For example, a batch's action
  `invoice_round(rent_round: RentRound)` names its run's society from
  `rent_round.society_id`.
- the value a create sets a record's tenancy reference from, when the
  create runs whenever the action does. The create needs the caller's
  membership in that tenant, and the action fails with it. A create
  inside an `if`, a `match` or a loop, or after a `return` that may leave
  first, proves nothing.

Anything else is refused, including a parameter that holds an id. Only
where the value came from counts, not what decided it.

The check runs in the action whose `emit` or `batches.each` uses the
value, never at its callers. An action that emits from a parameter is
refused even when every action calling it passes a proved id, since a
route may call it too. Pass it the record instead. The same rule holds
for every action, so a subscriber's action that passes its event's tenant
on, to another `emit` or a batch, reads a record of that tenant first, as
[Declaring an event](12-jobs-and-events.md#declaring-an-event) advises
for anything more than ids.

What a batch's run stores, from its items, what it saved and its
summary, is proved as a call's arguments are, by the tenant the run is
limited to. What `batches.status` gives is held to what decides every
batch's failures. [Background work](12-jobs-and-events.md) covers this
under "What a run reaches under policy".

## Tenancy
<!-- topic: tenancy -->

[Keep customers' data apart](../guides/21-keep-customers-data-apart.md) does the whole job, start to finish.

Use tenancy when your app is shared by many customer companies, so that
one company never sees another's data. Each customer is a tenant: one
record of an entity you choose, such as `Company`.

People belong to a company through a membership. A membership is a
stored record, such as a `CompanyMembership`, that says which user holds
which role in which company. Its entity has a `membership:` section naming
those three fields: `scope`, the company; `member`, the user; and `role`,
such as `CompanyRole.owner` ([A membership role](#a-membership-role)
shows one in full). Grants then name roles: an entry such as
`CompanyRole.viewer` in a record's `read:` list lets in the users who hold
that role in the record's company.

### Naming the tenant

The project's root `policy.jadpo`, the policy file at the top of the
project beside `app.jadpo`, names the tenant entity once:

```jadpo fragment
// policy.jadpo
policy {
    tenant: Company
}
```

This file holds the rules for the whole app: the tenant, and the
[actions](#permissions-on-actions) that need a permission of their own. Each entity's own
`policy:` section still lists who may create, read, change and delete its
records. Naming a tenant adds these rules to every entity:

- Every entity needs one reference to `Company`, its tenancy reference,
  such as `company_id`. A task carries its own `company_id`, not only its
  project's.
- Every grant on a tenant's records applies only within companies where
  the caller has a membership. A grant is an entry in one of an entity's
  lists, such as `read: [CompanyRole.viewer]` ([Grants](#grants)). This
  holds for `Access.authenticated`, which lets in anyone signed in, too:
  on a tenant's records it lets in anyone signed in who belongs to the
  record's company.
- An entity every tenant shares, such as a list of countries or `User`
  itself, says `tenant: Tenancy.shared` in its `policy:`. Its records
  belong to no company, and its grants apply as they would without
  tenancy.

A request has no current tenant of its own, and the signed-in caller
carries no company. Each record's tenant is the company its tenancy
reference names, and a caller who belongs to several companies is let in
to each one's records by their membership there. A route says which
company it works in through its input, such as a `company_id` in its path
or body, and policy checks the caller's membership in the company of
each record the route reads or changes.

An entity with two references to `Company` names, in its `policy:`, the
one that decides where it belongs, such as `tenant: source_company_id`.

An entity you add belongs to a tenant unless a person accepts otherwise,
as every change that widens access needs
([Protecting policy](#protecting-policy)).

A company's first membership founds it, in the action that creates the
company ([Founding a tenant](#founding-a-tenant)). Every other membership
is created by someone who belongs already, as the membership's own
`create:` says. So nobody makes themselves a member, or an owner, of
someone else's company.

### References stay inside the tenant

A reference from one of a tenant's records to another always points at a
record of the same tenant: a chore of Northwind's names one of Northwind's
projects, never Contoso's. Every create and update that sets such a
reference checks it as part of that same write, whether or not the
caller's grants would allow it. One that points into another tenant is
refused with 403 `not_permitted`. A reference to a record that doesn't
exist at all still gets that entity's own not-found failure, such as
`ProjectNotFound`.

Because a chore and its project are always in the same company, anyone
who may read a chore has their role in the project's company too. So a
query of chores may include each chore's project without showing anyone
another company's data. Here a chore belongs to a company and to one of
its projects:

```jadpo check project=tenant-references file=app.jadpo hidden
use work/entities { Member }

application {}

principal {
    user: {
        subject: Member.handle
    }
}
```

```jadpo check project=tenant-references file=authentication.jadpo hidden
authentication {
    browser_session: {
        cookie: "work_session"
    }
}
```

```jadpo check project=tenant-references file=policy.jadpo hidden
use work/entities { Company }
use work

policy {
    tenant: Company
    actions: {
        work.found_company: [Access.authenticated]
    }
}
```

```jadpo check project=tenant-references file=domains/work/types/handle.jadpo hidden
type Handle = Text {
    min_length: 3
    max_length: 40
}
```

```jadpo check project=tenant-references file=domains/work/types/label.jadpo hidden
type Label = Text {
    min_length: 1
    max_length: 80
}
```

```jadpo check project=tenant-references file=domains/work/types/company_role.jadpo hidden
/// What a member may do in a company.
type CompanyRole = Enum {
    owner
    viewer
}
```

```jadpo check project=tenant-references file=domains/work/types/project_card.jadpo hidden
use work/entities { Project }

/// A project as members see it.
type ProjectCard = Output {
    pick: [Project.id, Project.name]
}
```

```jadpo check project=tenant-references file=domains/work/types/chore_card.jadpo hidden
use work/entities { Chore }

/// A chore on its own.
type ChoreCard = Output {
    pick: [Chore.id, Chore.title]
}
```

```jadpo check project=tenant-references file=domains/work/types/chore_view.jadpo hidden
use work/entities { Chore }
use work/types { ProjectCard }

/// A chore with the project it belongs to.
type ChoreView = Output {
    pick: [Chore.id, Chore.title]
    project: ProjectCard
}
```

```jadpo check project=tenant-references file=domains/work/failures/handle_taken.jadpo hidden
type HandleTaken = Failure {
    kind: FailureKind.conflict
    message: "That handle is taken."
}
```

```jadpo check project=tenant-references file=domains/work/entities/member.jadpo hidden
use work/types { Handle }

/// Someone who signs in.
entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
        tenant: Tenancy.shared
    }
}
```

```jadpo check project=tenant-references file=domains/work/entities/company.jadpo hidden
use work/types { CompanyRole, Label }

/// A customer: the tenant every other record belongs to.
entity Company {
    id: Uuid
    name: Label

    policy: {
        read: [CompanyRole.owner, CompanyRole.viewer]
    }
}
```

```jadpo check project=tenant-references file=domains/work/entities/company_membership.jadpo hidden
use work/entities { Company, Member }
use work/types { CompanyRole }

/// A member's role in a company.
entity CompanyMembership {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    member_id: Member.id {
        on_delete: OnDelete.cascade
    }
    role: CompanyRole

    membership: {
        scope: company_id
        member: member_id
        role: role
    }

    policy: {
        create: [CompanyRole.owner]
        read: [CompanyRole.owner, member_id]
    }
}
```

```jadpo check project=tenant-references file=domains/work/entities/project.jadpo hidden
use work/entities { Company }
use work/types { CompanyRole, Label }

/// A piece of work in a company.
entity Project {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    name: Label

    policy: {
        create: [CompanyRole.owner]
        read: [CompanyRole.owner, CompanyRole.viewer]
    }
}
```

```jadpo check project=tenant-references file=domains/work/actions/register.jadpo hidden
use work/entities { Member }
use work/types { Handle }
use work/failures { HandleTaken }

/// Signs a new member up; anyone may.
action register(handle: Handle) fails HandleTaken -> Member {
    return attempt Member.create {
        set: {
            handle: handle
        }
        conflict: {
            Member.handle: HandleTaken
        }
    }
}
```

```jadpo check project=tenant-references file=domains/work/actions/found_company.jadpo hidden
use work/entities { Company, CompanyMembership }
use work/types { CompanyRole, Label }

/// Starts a company with its founder as its owner.
action found_company(name: Label, founder: Principal.user) -> Company {
    var company: Company = Company.create {
        set: {
            name: name
        }
    }
    var owner: CompanyMembership = CompanyMembership.create {
        set: {
            company_id: company.id
            member_id: founder.id
            role: CompanyRole.owner
        }
    }
    assert owner.role == CompanyRole.owner
    return company
}
```

```jadpo check project=tenant-references file=domains/work/actions/add_member.jadpo hidden
use work/entities { Company, CompanyMembership, CompanyNotFound, Member, MemberNotFound }
use work/types { CompanyRole }

/// Gives a member a role in a company; only its owners may.
action add_member(
    company_id: Company.id,
    member_id: Member.id,
    role: CompanyRole
) fails CompanyNotFound, MemberNotFound -> CompanyMembership {
    return attempt CompanyMembership.create {
        set: {
            company_id: company_id
            member_id: member_id
            role: role
        }
    }
}
```

```jadpo check project=tenant-references file=domains/work/actions/start_project.jadpo hidden
use work/entities { Company, CompanyNotFound, Project }
use work/types { Label, ProjectCard }

/// Starts a project in a company.
action start_project(company_id: Company.id, name: Label) fails CompanyNotFound -> ProjectCard {
    var project: Project = attempt Project.create {
        set: {
            company_id: company_id
            name: name
        }
    }
    return ProjectCard {
        id: project.id
        name: project.name
    }
}
```

```jadpo check project=tenant-references file=domains/work/actions/add_chore.jadpo hidden
use work/entities { Chore, Company, CompanyNotFound, Project, ProjectNotFound }
use work/types { ChoreCard, Label }

/// What adding a chore takes.
type AddChore = Input {
    company_id: Company.id
    project_id: Project.id
    title: Label
}

/// Adds a chore to a project.
action add_chore(input: AddChore) fails CompanyNotFound, ProjectNotFound -> ChoreCard {
    var chore: Chore = attempt Chore.create {
        set: {
            company_id: input.company_id
            project_id: input.project_id
            title: input.title
        }
    }
    return ChoreCard {
        id: chore.id
        title: chore.title
    }
}
```

```jadpo check project=tenant-references file=domains/work/entities/chore.jadpo
use work/entities { Company, Project }
use work/types { CompanyRole, Label }

/// One step of a project.
entity Chore {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    project_id: Project.id {
        on_delete: OnDelete.cascade
    }
    title: Label

    policy: {
        create: [CompanyRole.owner]
        read: [CompanyRole.owner, CompanyRole.viewer]
    }
}
```

```jadpo check project=tenant-references file=domains/work/queries/company_chores.jadpo
use work/entities { Chore, Company }
use work/types { ChoreView }

/// A company's chores, each with its project.
query company_chores(company_id: Company.id) -> List<ChoreView> {
    from: Chore
    where: Chore.company_id == company_id
    order: [Chore.title asc]
    limit: 100
    include: {
        project: Chore.project_id
    }
}
```

```jadpo check project=tenant-references file=domains/work/routes/chores.jadpo migrate=initial hidden
use work/entities { Company }
use work/types { ChoreCard, ChoreView }
use work/actions { AddChore, add_chore }
use work/queries { company_chores }

/// A company's chores.
routes Chores {
    prefix: /chores

    /// Adds a chore.
    POST / {
        input: AddChore
        output: ChoreCard
        status: SuccessStatus.created
        run: add_chore(input)
    }

    /// A company's chores, each with its project.
    GET /{company_id: Company.id} {
        output: List<ChoreView>
        run: company_chores(company_id)
    }
}
```

```jadpo check project=tenant-references file=api/main.jadpo_record hidden
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

api main
    authentication browser_session

POST /chores
    input AddChore (body)
    output ChoreCard
    status 201
    fails company_not_found
    fails project_not_found

GET /chores/{company_id: Uuid}
    output List<ChoreView>
    status 200

GET /openapi.json
    generated openapi
    status 200

input AddChore
    company_id: Uuid
    project_id: Uuid
    title: Text { min_length: 1, max_length: 80 }

output ChoreCard
    id: Uuid
    title: Text { min_length: 1, max_length: 80 }

output ChoreView
    id: Uuid
    project: ProjectCard
    title: Text { min_length: 1, max_length: 80 }

output ProjectCard
    id: Uuid
    name: Text { min_length: 1, max_length: 80 }

failure company_not_found
    status 404

failure project_not_found
    status 404
```

In the first test, Ann owns both companies, so only the check on the
project reference refuses her. In the second, Bob, a viewer of Northwind,
sees its chores with their projects:

```jadpo check project=tenant-references file=domains/work/routes/chores.test.jadpo
use work/entities { Company, CompanyMembership, Member }
use work/types { ChoreCard, ChoreView, CompanyRole, ProjectCard }
use work/actions { AddChore, add_chore, add_member, found_company, register, start_project }

test "a chore can't name another company's project" {
    var ann: Member = attempt register("ann")
    var northwind: Company = found_company("Northwind", Principal.user(ann))
    var contoso: Company = found_company("Contoso", Principal.user(ann))
    var rival: ProjectCard = attempt start_project(contoso.id, "Rival")
    var response: Response = request {
        method: HttpMethod.post
        path: "/chores"
        as: Principal.user(ann)
        input: AddChore {
            company_id: northwind.id
            project_id: rival.id
            title: "Borrow"
        }
    }
    assert response.status == 403
    assert response.code == "not_permitted"
}

test "a viewer sees each chore with its project" {
    var ann: Member = attempt register("ann")
    var bob: Member = attempt register("bob")
    var northwind: Company = found_company("Northwind", Principal.user(ann))
    var viewer: CompanyMembership = attempt add_member(northwind.id, bob.id, CompanyRole.viewer)
    assert viewer.role == CompanyRole.viewer
    var launch: ProjectCard = attempt start_project(northwind.id, "Launch")
    var kickoff: ChoreCard = attempt add_chore(
        AddChore {
            company_id: northwind.id
            project_id: launch.id
            title: "Plan"
        }
    )
    var seen: Response = request {
        method: HttpMethod.get
        path: "/chores/{company_id}"
        params: {
            company_id: northwind.id
        }
        as: Principal.user(bob)
    }
    assert seen.status == 200
    assert seen.output == [
        ChoreView {
            id: kickoff.id
            title: "Plan"
            project: launch
        }
    ]
}
```

A reference straight to `Company` isn't checked this way. Either it is
the record's tenancy reference, which is what puts the record in its
company, or the entity has two references to `Company`, and the one its
`tenant:` doesn't name points at another company on purpose.

### A unique rule holds within its tenant

A `unique:` rule on a tenant's records holds within each tenant. Northwind
and Contoso may each have a project called Launch, but a second Launch in
Northwind is the failure the create maps it to under `conflict:`, here
`ProjectNameTaken`. So a conflict never tells one company what another
stores.

The compiler does this by adding the record's tenancy reference to the
rule's key, the set of fields that must be unique together, so `name`
becomes unique per `company_id`. A rule that already includes the tenancy
reference is left as it is. An entity every tenant shares, and the tenant
entity itself, keep one key across the whole application. Here `Member` is
shared, so two members can never have the same `handle`.

```jadpo check project=tenant-keys file=app.jadpo hidden
use work/entities { Member }

application {}

principal {
    user: {
        subject: Member.handle
    }
}
```

```jadpo check project=tenant-keys file=authentication.jadpo hidden
authentication {
    browser_session: {
        cookie: "work_session"
    }
}
```

```jadpo check project=tenant-keys file=policy.jadpo hidden
use work/entities { Company }

policy {
    tenant: Company
}
```

```jadpo check project=tenant-keys file=domains/work/types/handle.jadpo hidden
type Handle = Text {
    min_length: 3
    max_length: 40
}
```

```jadpo check project=tenant-keys file=domains/work/types/label.jadpo hidden
type Label = Text {
    min_length: 1
    max_length: 80
}
```

```jadpo check project=tenant-keys file=domains/work/types/company_role.jadpo hidden
/// What a member may do in a company.
type CompanyRole = Enum {
    owner
    viewer
}
```

```jadpo check project=tenant-keys file=domains/work/types/project_card.jadpo hidden
use work/entities { Project }

/// A project as members see it.
type ProjectCard = Output {
    pick: [Project.id, Project.name]
}
```

```jadpo check project=tenant-keys file=domains/work/types/seat_view.jadpo hidden
use work/entities { CompanyMembership }

/// A member's seat in a company.
type SeatView = Output {
    pick: [CompanyMembership.id, CompanyMembership.company_id, CompanyMembership.role]
}
```

```jadpo check project=tenant-keys file=domains/work/failures/handle_taken.jadpo hidden
type HandleTaken = Failure {
    kind: FailureKind.conflict
    message: "That handle is taken."
}
```

```jadpo check project=tenant-keys file=domains/work/failures/project_name_taken.jadpo hidden
type ProjectNameTaken = Failure {
    kind: FailureKind.conflict
    message: "The company already has a project with that name."
}
```

```jadpo check project=tenant-keys file=domains/work/entities/member.jadpo hidden
use work/types { Handle }

/// Someone who signs in; every company shares them, so a handle is unique
/// across the application.
entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
        tenant: Tenancy.shared
    }
}
```

```jadpo check project=tenant-keys file=domains/work/entities/company.jadpo hidden
use work/types { CompanyRole, Label }

/// A customer: the tenant every other record belongs to.
entity Company {
    id: Uuid
    name: Label

    policy: {
        create: [Access.authenticated]
        read: [CompanyRole.owner, CompanyRole.viewer]
    }
}
```

```jadpo check project=tenant-keys file=domains/work/actions/register.jadpo hidden
use work/entities { Member }
use work/types { Handle }
use work/failures { HandleTaken }

/// Signs a new member up; anyone may.
action register(handle: Handle) fails HandleTaken -> Member {
    return attempt Member.create {
        set: {
            handle: handle
        }
        conflict: {
            Member.handle: HandleTaken
        }
    }
}
```

```jadpo check project=tenant-keys file=domains/work/actions/start_project.jadpo hidden
use work/entities { Company, CompanyNotFound, Project }
use work/types { Label, ProjectCard }
use work/failures { ProjectNameTaken }

/// What starting a project takes.
type StartProject = Input {
    company_id: Company.id
    name: Label
}

/// Starts a project in a company.
action start_project(input: StartProject) fails CompanyNotFound, ProjectNameTaken -> ProjectCard {
    var project: Project = attempt Project.create {
        set: {
            company_id: input.company_id
            name: input.name
        }
        conflict: {
            Project.name: ProjectNameTaken
        }
    }
    return ProjectCard {
        id: project.id
        name: project.name
    }
}
```

```jadpo check project=tenant-keys file=domains/work/routes/projects.jadpo hidden
use work/types { ProjectCard }
use work/actions { StartProject, start_project }

/// A company's projects.
routes Projects {
    prefix: /projects

    /// Starts a project.
    POST / {
        input: StartProject
        output: ProjectCard
        status: SuccessStatus.created
        run: start_project(input)
    }
}
```

```jadpo check project=tenant-keys file=domains/work/entities/project.jadpo
use work/entities { Company }
use work/types { CompanyRole, Label }

/// A piece of work in a company.
entity Project {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    name: Label {
        unique: true  // within its company
    }

    policy: {
        create: [CompanyRole.owner]
        read: [CompanyRole.owner, CompanyRole.viewer]
    }
}
```

```jadpo check project=tenant-keys file=domains/work/routes/projects.test.jadpo
use work/entities { Member }
use work/types { SeatView }
use work/actions { NewCompany, StartProject, found_company, register }

test "two companies may each have a project called Launch, but not two in one" {
    var ann: Member = attempt register("ann")
    var bob: Member = attempt register("bob")
    var northwind: SeatView = found_company(
        NewCompany {
            name: "Northwind"
        },
        Principal.user(ann)
    )
    var contoso: SeatView = found_company(
        NewCompany {
            name: "Contoso"
        },
        Principal.user(bob)
    )
    var first: Response = request {
        method: HttpMethod.post
        path: "/projects"
        as: Principal.user(ann)
        input: StartProject {
            company_id: northwind.company_id
            name: "Launch"
        }
    }
    assert first.status == 201
    var second: Response = request {
        method: HttpMethod.post
        path: "/projects"
        as: Principal.user(bob)
        input: StartProject {
            company_id: contoso.company_id
            name: "Launch"
        }
    }
    assert second.status == 201
    var refused: Response = request {
        method: HttpMethod.post
        path: "/projects"
        as: Principal.user(ann)
        input: StartProject {
            company_id: northwind.company_id
            name: "Launch"
        }
    }
    assert refused.status == 409
    assert refused.code == "project_name_taken"
}
```

Since the key holds the tenancy reference, a query that finds one record
by a unique rule
([How many records come back](06-reads-and-writes.md#how-many-records-come-back))
must test the tenancy reference with `==` beside the rule's fields, such
as `Project.company_id == company_id`. It needn't when it tests a
reference to another of the tenant's records, since that record already
names the tenant.

The field a person signs in by, their subject, such as an email address,
is on an entity every tenant shares, so it is unique across the app
([The principal](08-identity.md#the-principal)).

When you add tenancy to an existing app, `jadpo migrate new` drops each
unique key that didn't hold the tenancy reference and makes it again with
the tenancy column, `UNIQUE ("name", "company_id")`. The migration needs
no decision from you, since every set of rows the old key allowed, the new
one allows too.

### Founding a tenant

A new company has no members, so nobody holds a role that could let
them create its first membership. That first membership founds the
company, and it is allowed in one place only: the action that creates the
company's record. There, the membership's scope is the `id` that the
company's create gave back, its member is the caller, and nobody belongs
to the company yet.

Any other new membership needs someone who belongs already, even one that
names its own caller as the member. In a grant list, a user field such as
`member_id` lets in the user it points to. So
`create: [CompanyRole.owner, member_id]` lets an owner add anyone, and
anyone else only themselves, and only in a company they already belong
to. A stranger can't add themselves to a company whose members have all
left: that is 403 `not_permitted`. Founding a new company still makes its
founder a member:

```jadpo check project=tenant-keys file=domains/work/entities/company_membership.jadpo
use work/entities { Company, Member }
use work/types { CompanyRole }

/// A member's role in a company.
entity CompanyMembership {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    member_id: Member.id {
        on_delete: OnDelete.cascade
    }
    role: CompanyRole

    membership: {
        scope: company_id
        member: member_id
        role: role
    }

    policy: {
        create: [CompanyRole.owner, member_id]
        read: [CompanyRole.owner, member_id]
        delete: [CompanyRole.owner, member_id]
    }
}
```

```jadpo check project=tenant-keys file=domains/work/actions/found_company.jadpo
use work/entities { Company, CompanyMembership }
use work/types { CompanyRole, Label, SeatView }

/// What founding a company takes.
type NewCompany = Input {
    name: Label
}

/// Starts a company with its founder as its owner.
action found_company(input: NewCompany, founder: Principal.user) -> SeatView {
    var company: Company = Company.create {
        set: {
            name: input.name
        }
    }
    var seat: CompanyMembership = CompanyMembership.create {
        set: {
            company_id: company.id
            member_id: founder.id
            role: CompanyRole.owner
        }
    }
    return SeatView {
        id: seat.id
        company_id: seat.company_id
        role: seat.role
    }
}
```

```jadpo check project=tenant-keys file=domains/work/actions/add_member.jadpo hidden
use work/entities { Company, CompanyMembership, CompanyNotFound, Member, MemberNotFound }
use work/types { CompanyRole, SeatView }

/// Who joins a company, and as what.
type NewMember = Input {
    member_id: Member.id
    role: CompanyRole
}

/// Gives a member a role in a company.
action add_member(
    company_id: Company.id,
    input: NewMember
) fails CompanyNotFound, MemberNotFound -> SeatView {
    var seat: CompanyMembership = attempt CompanyMembership.create {
        set: {
            company_id: company_id
            member_id: input.member_id
            role: input.role
        }
    }
    return SeatView {
        id: seat.id
        company_id: seat.company_id
        role: seat.role
    }
}
```

```jadpo check project=tenant-keys file=domains/work/actions/leave.jadpo hidden
use work/entities { CompanyMembership, CompanyMembershipNotFound }

/// Gives up a seat in a company.
action leave(seat_id: CompanyMembership.id) fails CompanyMembershipNotFound -> Unit {
    attempt CompanyMembership.delete(seat_id)
}
```

```jadpo check project=tenant-keys file=domains/work/routes/companies.jadpo migrate=initial hidden
use work/entities { Company }
use work/types { SeatView }
use work/actions { NewCompany, NewMember, add_member, found_company }

/// Companies and their members.
routes Companies {
    prefix: /companies

    /// Founds a company, its caller its owner.
    POST / {
        input: NewCompany
        output: SeatView
        status: SuccessStatus.created
        run: found_company(input, current_principal.user)
    }

    /// Gives a member a role in a company.
    POST /{company_id: Company.id}/members {
        input: NewMember
        output: SeatView
        status: SuccessStatus.created
        run: add_member(company_id, input)
    }
}
```

```jadpo check project=tenant-keys file=api/main.jadpo_record hidden
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

api main
    authentication browser_session

POST /companies
    caller user
    input NewCompany (body)
    output SeatView
    status 201

POST /companies/{company_id: Uuid}/members
    input NewMember (body)
    output SeatView
    status 201
    fails company_not_found
    fails member_not_found

GET /openapi.json
    generated openapi
    status 200

POST /projects
    input StartProject (body)
    output ProjectCard
    status 201
    fails company_not_found
    fails project_name_taken

enum CompanyRole
    variant owner
    variant viewer

input NewCompany
    name: Text { min_length: 1, max_length: 80 }

input NewMember
    member_id: Uuid
    role: CompanyRole

output ProjectCard
    id: Uuid
    name: Text { min_length: 1, max_length: 80 }

output SeatView
    company_id: Uuid
    id: Uuid
    role: CompanyRole

input StartProject
    company_id: Uuid
    name: Text { min_length: 1, max_length: 80 }

failure company_not_found
    status 404

failure member_not_found
    status 404

failure project_name_taken
    status 409
    message "The company already has a project with that name."
```

```jadpo check project=tenant-keys file=domains/work/routes/companies.test.jadpo
use work/entities { Member }
use work/types { CompanyRole, SeatView }
use work/actions { NewCompany, NewMember, found_company, leave, register }

test "nobody seats themselves in a company everyone has left" {
    var ann: Member = attempt register("ann")
    var eve: Member = attempt register("eve")
    var seat: SeatView = found_company(
        NewCompany {
            name: "Northwind"
        },
        Principal.user(ann)
    )
    attempt leave(seat.id)
    var seized: Response = request {
        method: HttpMethod.post
        path: "/companies/{company_id}/members"
        params: {
            company_id: seat.company_id
        }
        as: Principal.user(eve)
        input: NewMember {
            member_id: eve.id
            role: CompanyRole.owner
        }
    }
    assert seized.status == 403
    assert seized.code == "not_permitted"
    var founded: Response = request {
        method: HttpMethod.post
        path: "/companies"
        as: Principal.user(eve)
        input: NewCompany {
            name: "Contoso"
        }
    }
    assert founded.status == 201
}
```

### Moving a record to another tenant

Moving a record to another company is rare, and it needs an action of its
own. Only an action listed under `actions:` in root `policy.jadpo` may do
it ([Permissions on actions](#permissions-on-actions) shows the list). The write checks every reference
the record keeps, and that no record of the old company refers to it. In
the new company, it needs what creating the moved record there would: the
entity's own `create:` and the caller's own membership there.

A membership moved in from another company holds no role its mover
couldn't give there. So a viewer can't move in an owner's membership they
hold elsewhere.

## Permissions on actions
<!-- topic: action-permissions (was actions) -->

Most actions need no permission of their own: an action takes its
permission from the entities it touches. Root `policy.jadpo` lists the few
that can't, under `actions:`:

- an action that touches no entity, such as an operator's health report,
  which is otherwise refused;
- an action that needs different people from its entity's grants, such as
  handing a todo to someone else, or creating a company together with its
  first owner's membership.

Each line names an action by its domain and name, and lists who may run
it, with the same kinds of entry as a grant:

```jadpo fragment
// policy.jadpo
policy {
    tenant: Company
    actions: {
        todos.transfer_todo: [Todo.owner_id]
        accounts.create_company: [Access.authenticated]
        monitoring.health_report: [StaffRole.operator]
    }
}
```

The domain is part of the name, as in a call from another domain. A bare
name that is one domain's action, `transfer_todo:`, is
`policy.binding_invalid` with the edit that writes `todos.transfer_todo:`,
and the `use` line that brought the bare name in isn't also reported
unused; `jadpo fmt` then writes `use todos` in its place.

`Todo.owner_id` means the current owner of the todo the action changes.
For that action, the listed people replace the entity grants on the
creates, updates, deletes and transitions in its own body. Its reads, and
the actions it calls, keep their own entities' grants, with the same
caller. [Field restrictions](#field-restrictions) and
[tenancy](#tenancy) still apply.

Calling an integration, an outside service such as a payment provider,
needs nothing extra: permission to run the action covers it.

### Lines that depend on a record

A user field such as `Todo.owner_id`, or a role held in a record such as
`CompanyRole.owner`, is checked on the records the action itself creates,
changes or deletes. On an action that changes none of them, such as a
report, it lets nobody in. The action is then checked as it starts against
the line's other entries: anyone signed in, or a role held across the
whole application. With none left, it refuses every caller, so its route
is `policy.effect_ungranted`.

A change that matches no record when it runs changes nothing, so no line
and no grant is checked
([A change that may match nothing](#a-change-that-may-match-nothing)).

### A call the listing doesn't cover

Because a called action keeps its own grants, a listed action can call
one that refuses some of the people its line lets in. Say
`lending.borrow_book` is listed for `Access.authenticated` and calls
`lend_book`, whose `Book` update only `StaffRole.admin` holds. A customer
would get 403 from `lend_book`. `jadpo check` reports this at the call as
`policy.call_ungranted`, naming the people the line lets in that the
called action refuses. Fix it by making the write in the listed action's
own body, or by listing the called action for exactly those people.
Listing it for everyone widens it on every path that reaches it. It is
an error you can decline, for a call that only some callers are meant to
pass.

### Listing a query

A query is listed under `actions:` like an action, when it needs fewer
people than its entity's `read:`. Each listed query's line may only hold
entries that depend on no record: `Access.authenticated`, or a role held
across the whole application. Anything else is `policy.binding_invalid`.
The line is checked as the query starts, on every request, so removing
someone's role takes effect on their next request. Its records are still
filtered by the entity's `read:`, and its output fields are proved
against the line's people, not everyone the entity's `read:` lets in. So
one entity can give staff a query with stock and sale fields restricted
to `StaffRole.admin`, beside a catalogue query for every signed-in
customer.

A route anyone may call can't run a listed query, since its line never
names `Access.public` (`policy.public_grant_missing`). A schedule,
subscriber or batch never passes one either: the system is let in only
by a line naming `Access.system`, which a query's can't. Reaching one is
`policy.system_grant_missing`.

Here every signed-in customer reads a book, and only a bookseller where
it is shelved:

```jadpo check project=listed-query file=app.jadpo hidden
use shop/entities { Member }

application {}

principal {
    user: {
        subject: Member.handle
    }
}
```

```jadpo check project=listed-query file=authentication.jadpo hidden
authentication {
    browser_session: {
        cookie: "shop_session"
    }
}
```

```jadpo check project=listed-query file=policy.jadpo
use shop/types { StaffRole }
use shop

policy {
    actions: {
        shop.stock: [StaffRole.bookseller]
    }
}
```

```jadpo check project=listed-query file=domains/shop/types/handle.jadpo hidden
type Handle = Text {
    min_length: 3
    max_length: 40
}
```

```jadpo check project=listed-query file=domains/shop/types/title.jadpo hidden
type Title = Text {
    min_length: 1
    max_length: 80
}
```

```jadpo check project=listed-query file=domains/shop/types/staff_role.jadpo hidden
/// What a member of staff may do, in the whole shop.
type StaffRole = Enum {
    bookseller
}
```

```jadpo check project=listed-query file=domains/shop/types/catalogue_book.jadpo hidden
use shop/entities { Book }

/// A book as every signed-in customer sees it.
type CatalogueBook = Output {
    pick: [Book.id, Book.title]
}
```

```jadpo check project=listed-query file=domains/shop/types/stock_book.jadpo hidden
use shop/entities { Book }

/// A book as staff see it, with where it is shelved.
type StockBook = Output {
    pick: [Book.id, Book.title, Book.shelf_note]
}
```

```jadpo check project=listed-query file=domains/shop/failures/handle_taken.jadpo hidden
type HandleTaken = Failure {
    kind: FailureKind.conflict
    message: "That handle is taken."
}
```

```jadpo check project=listed-query file=domains/shop/entities/member.jadpo hidden
use shop/types { Handle }

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        read: [id]
    }
}
```

```jadpo check project=listed-query file=domains/shop/entities/staff.jadpo hidden
use shop/entities { Member }
use shop/types { StaffRole }

/// A member's role on the staff, held across the whole shop.
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
```

```jadpo check project=listed-query file=domains/shop/entities/book.jadpo
use shop/types { StaffRole, Title }

/// A book every signed-in customer may read, with where it is shelved,
/// which only a bookseller may.
entity Book {
    id: Uuid
    title: Title
    shelf_note: Title {
        policy: {
            read: [StaffRole.bookseller]
        }
    }

    policy: {
        create: [StaffRole.bookseller]
        read: [Access.authenticated]
    }
}
```

```jadpo check project=listed-query file=domains/shop/actions/register.jadpo hidden
use shop/entities { Member }
use shop/types { Handle }
use shop/failures { HandleTaken }

action register(handle: Handle) fails HandleTaken -> Member {
    return attempt Member.create {
        set: {
            handle: handle
        }
        conflict: {
            Member.handle: HandleTaken
        }
    }
}
```

```jadpo check project=listed-query file=domains/shop/actions/appoint.jadpo hidden
use shop/entities { Member, MemberNotFound, Staff }
use shop/types { StaffRole }

action appoint(member_id: Member.id) fails MemberNotFound -> Staff {
    return attempt Staff.create {
        set: {
            member_id: member_id
            role: StaffRole.bookseller
        }
    }
}
```

```jadpo check project=listed-query file=domains/shop/actions/stand_down.jadpo hidden
use shop/entities { Staff, StaffNotFound }

action stand_down(staff_id: Staff.id) fails StaffNotFound -> Unit {
    attempt Staff.delete(staff_id)
}
```

```jadpo check project=listed-query file=domains/shop/actions/add_book.jadpo hidden
use shop/entities { Book }
use shop/types { Title }

action add_book(title: Title, shelf_note: Title) -> Book {
    return Book.create {
        set: {
            title: title
            shelf_note: shelf_note
        }
    }
}
```

```jadpo check project=listed-query file=domains/shop/queries/browse.jadpo hidden
use shop/entities { Book }
use shop/types { CatalogueBook }

/// Every book, for any signed-in customer.
query browse() -> List<CatalogueBook> {
    from: Book
    order: [Book.title asc]
    limit: 50
}
```

Unlisted, `stock` would be `policy.output_field_unproved`, since
anyone signed in may call it and not all of them may read `shelf_note`.
Listed, it is proved against `StaffRole.bookseller` alone:

```jadpo check project=listed-query file=domains/shop/queries/stock.jadpo
use shop/entities { Book }
use shop/types { StockBook }

/// Every book with where it is shelved, for staff.
query stock() -> List<StockBook> {
    from: Book
    order: [Book.title asc]
    limit: 50
}
```

```jadpo check project=listed-query file=domains/shop/routes/books.jadpo migrate=initial hidden
use shop/types { CatalogueBook, StockBook }
use shop/queries { browse, stock }

/// The shop's books.
routes Books {
    prefix: /books

    /// The catalogue.
    GET / {
        output: List<CatalogueBook>
        run: browse()
    }

    /// The stock, for staff.
    GET /stock {
        output: List<StockBook>
        run: stock()
    }
}
```

```jadpo check project=listed-query file=api/main.jadpo_record hidden
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

api main
    authentication browser_session

GET /books
    output List<CatalogueBook>
    status 200

GET /books/stock
    output List<StockBook>
    status 200

GET /openapi.json
    generated openapi
    status 200

output CatalogueBook
    id: Uuid
    title: Text { min_length: 1, max_length: 80 }

output StockBook
    id: Uuid
    shelf_note: Text { min_length: 1, max_length: 80 }
    title: Text { min_length: 1, max_length: 80 }
```

A customer is refused the stock with 403 `not_permitted`, and a member
of staff whose role is removed is refused on their next request:

```jadpo check project=listed-query file=domains/shop/routes/books.test.jadpo
use shop/entities { Book, Member, Staff }
use shop/types { CatalogueBook, StockBook }
use shop/actions { add_book, appoint, register, stand_down }

test "staff see where a book is shelved, and customers only the catalogue" {
    var ann: Member = attempt register("ann")
    var cyd: Member = attempt register("cyd")
    var staff: Staff = attempt appoint(ann.id)
    assert staff.member_id == ann.id
    var dune: Book = add_book("Dune", "Back room")
    var stock: Response = request {
        method: HttpMethod.get
        path: "/books/stock"
        as: Principal.user(ann)
    }
    var shelved: StockBook = StockBook {
        id: dune.id
        title: "Dune"
        shelf_note: "Back room"
    }
    assert stock.output == [shelved]
    var refused: Response = request {
        method: HttpMethod.get
        path: "/books/stock"
        as: Principal.user(cyd)
    }
    assert refused.status == 403
    assert refused.code == "not_permitted"
    var browsed: Response = request {
        method: HttpMethod.get
        path: "/books"
        as: Principal.user(cyd)
    }
    var on_shelf: CatalogueBook = CatalogueBook {
        id: dune.id
        title: "Dune"
    }
    assert browsed.output == [on_shelf]
}

test "a role removed is refused on the next request" {
    var ann: Member = attempt register("ann")
    var staff: Staff = attempt appoint(ann.id)
    var dune: Book = add_book("Dune", "Back room")
    var before: Response = request {
        method: HttpMethod.get
        path: "/books/stock"
        as: Principal.user(ann)
    }
    assert before.status == 200
    attempt stand_down(staff.id)
    var after: Response = request {
        method: HttpMethod.get
        path: "/books/stock"
        as: Principal.user(ann)
    }
    assert after.status == 403
    assert dune.title == "Dune"
}
```

## Field restrictions
<!-- topic: field-restrictions -->

A field follows its entity's grants unless it says otherwise. Use a
field's own `policy:` when some of a record's fields are more sensitive
than the rest, such as a project's budget. It has the same shape as the
entity's, and it can only take away: a field can't grant anyone more than
its entity does (`policy.field_widens_entity`). A field no client may
read or write says so by name.

```jadpo fragment
entity Project {
    ...
    budget: ProjectBudget {
        policy: {
            read: [CompanyRole.editor]
            update: [CompanyRole.editor]
        }
    }
    import_reference: Text {
        policy: FieldAccess.server_only
    }
}
```

Here viewers may read a project, but only editors may read or change its
`budget`. A field's `policy:` narrows `create`, `read` and `update`. It
has no `delete`, since a field is deleted with its record.

### A field no request may touch: `FieldAccess.server_only`

`FieldAccess.server_only`, as on `import_reference` above, lets no request
read or write the field. A test's or a seed's call without a caller runs
without policy, so it may. [Runs as the system](12-jobs-and-events.md#runs-as-the-system)
says how schedules and subscribers write one.

### Writes

If a request changes a field the caller may not change, the whole write
is refused; nothing is dropped silently. A caller who may change the
record but not the field gets 403 `not_permitted`. A create that leaves an
optional field `none` sets nothing, so the field's policy never refuses
it.

### Who may set a field

Who may set a field is the app owner's decision, like the rest of
policy. A field says it in its own `create:` and `update:`, with
`FieldAccess.server_only`, with `immutable: true`, which no update
changes, or with `generated:`, which only the store sets
([Fields and their settings](05-data.md#fields-and-their-settings)). A
field that says none of these follows its record's grants: whoever may
change the record may change it.

A route whose action may set a field nobody may set is refused at check
(`policy.effect_ungranted`), not answered with 403 at run time. That
covers a field the action always sets, and one it sets only when the
request supplies it: a field of a patch's `Input`, or one
`when_supplied:` sets. The error's next
step is a person's: leave the field out, or ask the app's owner, who
names who may set it. Here the email comes from the identity provider,
so no request changes it:

```jadpo fragment
entity User {
    ...
    email: Email {
        policy: {
            update: []
        }
    }
}
```

Letting more people set a field is a widening a person accepts, like
any other ([Protecting policy](#protecting-policy)): a caller added to
its `update:`, or its `immutable: true` or `generated:` removed.

### Outputs

An output type has one shape for everyone. If anyone allowed to call a
query can't read one of its output fields, the query doesn't compile
(`policy.output_field_unproved`). The fix is a second query with a
narrower output, or, for what only some roles see, a record of its own
([Fields shown by role](#fields-shown-by-role)). The same holds for a
record a query includes through a reference: everyone who may read the
record must be able to read what it refers to.

Who may read a value is proved from the grants alone, not from the data.
Two records that each refer to a company may refer to different
companies, so a role held in one says nothing about the other.

### Webhook replies

A webhook's reply
([Webhooks that need a reply](11-integrations.md#webhooks-that-need-a-reply))
goes back to whoever sent the webhook, so it is an output to
`Access.public`. Every field it carries, or that decided it, must be one
anyone at all may read, though its action runs as the system.

The system finds every record its grants name. So a reply that counts or
tests for records shows whether they are there, and they must be records
anyone at all may read. A failure is answered to the sender too. So what
decides whether the action fails (a condition, a `get` or a change by
`id` finding nothing, a `conflict:`) and a failure's client fields are
held to the same proof.

### What else shows a field

A field's value can reach a caller in other ways than an output, and the
compiler checks each like an output:

- **Conditions and orders.** Which records pass a condition, and the order
  they come in, show the fields they test. So a query, `get`, include or
  write may filter or order on a field only when everyone who may run it
  may read that field. Otherwise `policy.output_field_unproved` names the
  condition or order. A transition's `when:` is a write's condition:
  refusing a record shows what it tests. A lifecycle's `visible:` is a
  condition on every read of its entity, so it tests only fields everyone
  the entity's `read:` lets in may read.
- **Copies.** Passing a value through a variable, another type, a
  function or the record a write stores it in doesn't escape the field's
  rules, because the compiler tracks where it came from. A migration's
  [computed backfill](05-data.md#computed-backfills) is a copy too: each
  field it reads must be one everyone who may read the field it fills may
  read.
- **What decides a value.** A value made under an `if`, a `match` or a
  loop, or kept by a filter, shows what they test. So it carries their
  fields' rules too. So do a count of what a filter kept, a failure raised
  on such a condition, and what comes after a `return` or `reject` it may
  have taken.
- **Finding records.** Which record a `get`, a query or a write finds
  shows the value it found it by, and an include shows the reference it
  follows. So each is proved like an output.
- **What a write gives back.** The record an update returns is proved
  against its entity's `read:` as well as its fields', since a caller may
  change a record they may not read. What its `set:` gave a field, and a
  created record's values, are the caller's own.
- **What a write stores.** A value stored in a field is read later, from
  any request, by everyone who may read that field. So a create or update
  stores a value in a field only when every one of them may read it,
  proved as an output is. Otherwise `policy.output_field_unproved` names
  the value in the write. A copy between fields the same callers read
  stays allowed: within a record, into a record that refers to the one it
  came from, or into one of the same tenant. A record is one record
  however the body holds it: the record a change gives back is the one it
  changed, so a `var mut` holding it through a second change still names
  it, as does a `match` binding the one record a value such as
  `list.first(...)` holds (never a collection's records, which are many),
  and a change finds a record it is given by its `id`, not by what an
  earlier change stored in it. What the change gives back is still proved
  against its entity's `read:`. So does a copy into a
  record whose user field the write sets from the caller, such as a
  duplicate of the caller's own todo: whoever that field lets in is the
  caller, who read the value. Who reads a field is known
  only of the record it is stored in, so a copy into any other record
  proves only what holds of anyone signed in. A patch, `when_supplied:`, a
  counter and a transition store values too, and so does an action a
  value is passed to. An action that finds the record it changes by an
  `id` it is given changes the record whose `id` its caller passes, so a
  value read from that record and passed beside its `id` stays within
  the record, as a copy in one body does. What decided a value is stored
  with it. Since an action is all-or-nothing, so is whatever a `reject`
  in it tests. A value read through a query's `Output` is still the field
  it picks, so storing it is checked as storing one read from the record
  is.
- **A lookup isn't a copy.** A restricted field used only to choose
  which existing record's `id` a write stores shows the field's readers
  nothing it holds: they learn which record was chosen, and they read
  its `id` anyway. So no acceptance is needed when every value stored is
  an `id` of the field's own entity (a reference to the chosen record,
  or `none`), everyone who may run the action may read the field, and the
  field decided it only by equality with a key: `==`, `!=` or
  `text.equals_ignoring_case`, between the field, or a standard
  operation of it such as `text.lower`, and a value made only of the
  action's parameters, or a standard operation of them, never a literal,
  a constant, a setting or a variable. A loop over the records, an `if`
  or a `match` on the comparison, `list.filter` and `list.find` keep it
  a lookup; so does an action that does the lookup and returns or
  stores the `id`, while each caller, an action or a route, passes the
  key only from its own parameters and passes no part of it again as a
  value the action stores where those readers read it. Anything a check
  can't see through, a call included, is held to be a copy. It is a copy, as before, when the value is worked out from what
  the field holds (`text.ends_with(c.email, "@vip.test")`, a length),
  when the key is a literal or anything but the request's
  values, when the action also stores the key in a field that a role
  may read who may not read the source field (the readers would then
  know which record holds that value), and when anything but the `id` is
  stored, the field's value above all.
- **Copies a person approves.** A copy that lets people read what a
  field keeps from them, such as a staff-only macro's text stored in a
  reply every member reads, is the app owner's decision, so it is allowed
  only when a person approves it. An acceptance may approve a named field
  flow from a source field to a target field
  (`"desk.Macro.body -> desk.Reply.body": PolicyChange.field_copied`,
  [Acceptances](18-records-and-manifest.md#acceptances)). It allows that
  flow from any action while the acceptance is kept, and nothing else:
  another source or another target needs its own. `access.jadpo_record`
  keeps each as a `copy` line, so a reviewer sees it beside the grants.
  The error says which reader may not read the source field, and whether
  the cause is their role, which only such an approval answers, the
  tenant, when nothing shows the copy is in the source's tenant, or the
  record, when a reference of the target lets in someone else.

### Copying a record the caller owns

A copy of a caller's own record, owned by the same caller, is stored
like a copy within one record. The write sets the copy's user field from
the caller, so whoever that field lets in is the caller who read the
value. Here a person duplicates one of their todos, and the copy keeps
its title and due date:

```jadpo check project=copy-own-todo file=app.jadpo hidden
use todos/entities { User }

application {}

principal {
    user: {
        subject: User.handle
    }
}
```

```jadpo check project=copy-own-todo file=authentication.jadpo hidden
authentication {
    api_token: {}
}
```

```jadpo check project=copy-own-todo file=domains/todos/types/handle.jadpo hidden
type Handle = Text {
    max_length: 40
}
```

```jadpo check project=copy-own-todo file=domains/todos/types/title.jadpo hidden
type Title = Text {
    min_length: 1
    max_length: 200
}
```

```jadpo check project=copy-own-todo file=domains/todos/types/todo_view.jadpo hidden
use todos/entities { Todo }

/// A todo as its owner sees it.
type TodoView = Output {
    pick: [Todo.id, Todo.title, Todo.due_at]
}
```

```jadpo check project=copy-own-todo file=domains/todos/failures/handle_taken.jadpo hidden
type HandleTaken = Failure {
    kind: FailureKind.conflict
    message: "That handle is taken."
}
```

```jadpo check project=copy-own-todo file=domains/todos/entities/user.jadpo hidden
use todos/types { Handle }

/// A person with an account.
entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        read: [id]
    }
}
```

```jadpo check project=copy-own-todo file=domains/todos/entities/todo.jadpo
use todos/entities { User }
use todos/types { Title }

/// Something to do, which only its owner sees.
entity Todo {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    title: Title
    due_at: Instant?

    policy: {
        create: [owner_id]
        read: [owner_id]
    }
}
```

```jadpo check project=copy-own-todo file=domains/todos/fixtures/account_named.jadpo hidden
use todos/entities { User }
use todos/types { Handle }
use todos/failures { HandleTaken }

/// Someone with an account called `handle`.
fixture account_named(handle: Handle) -> User {
    return attempt User.create {
        set: {
            handle: handle
        }
        conflict: {
            User.handle: HandleTaken
        }
    }
}
```

```jadpo check project=copy-own-todo file=domains/todos/actions/duplicate_todo.jadpo
use todos/entities { Todo, TodoNotFound }
use todos/types { TodoView }

/// A new todo for the caller, with the title and due date of one of
/// theirs.
action duplicate_todo(todo_id: Todo.id, caller: Principal.user) fails TodoNotFound -> TodoView {
    var first_todo: Todo = attempt Todo.get(todo_id)
    var twin: Todo = Todo.create {
        set: {
            owner_id: caller.id
            title: first_todo.title
            due_at: first_todo.due_at
        }
    }
    return TodoView {
        id: twin.id
        title: twin.title
        due_at: twin.due_at
    }
}
```

```jadpo check project=copy-own-todo file=domains/todos/routes/todos.jadpo migrate=initial hidden
use todos/entities { Todo }
use todos/types { TodoView }
use todos/actions { duplicate_todo }

routes Todos {
    prefix: /todos

    /// A copy of one of the caller's todos.
    POST /{todo_id: Todo.id}/duplicate {
        output: TodoView
        status: SuccessStatus.created
        run: duplicate_todo(todo_id, current_principal.user)
    }
}
```

```jadpo check project=copy-own-todo file=api/main.jadpo_record hidden
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

api main
    authentication api_token

GET /openapi.json
    generated openapi
    status 200

POST /todos/{todo_id: Uuid}/duplicate
    caller user
    output TodoView
    status 201
    fails todo_not_found

output TodoView
    due_at: Instant?
    id: Uuid
    title: Text { min_length: 1, max_length: 200 }

failure todo_not_found
    status 404
```

Another person's todo is hidden from them, so asking to copy it is
404, as reading it would be. A copy stored in a record whose owner comes
from anywhere else, such as the request, is still
`policy.output_field_unproved`, since that owner may not be who read
the value:

```jadpo check project=copy-own-todo file=domains/todos/routes/todos.test.jadpo
use todos/entities { Todo, User }
use todos/types { TodoView }
use todos/fixtures { account_named }

test "a copy belongs to the caller, and another's todo is not found" {
    var ann: User = account_named("ann")
    var cyd: User = account_named("cyd")
    var bins: Todo = make.Todo {
        owner_id: ann.id
        title: "Put the bins out"
    }
    var duplicated: Response = request {
        method: HttpMethod.post
        path: "/todos/{todo_id}/duplicate"
        params: {
            todo_id: bins.id
        }
        as: Principal.user(ann)
    }
    assert duplicated.status == 201
    var view: TodoView = match duplicated.output {
        some(made) -> made
        none -> TodoView {
            id: bins.id
            title: "No copy"
            due_at: none
        }
    }
    assert view.title == "Put the bins out"
    assert view.id != bins.id
    var twin: Todo = attempt Todo.get(view.id)
    assert twin.owner_id == ann.id
    var refused: Response = request {
        method: HttpMethod.post
        path: "/todos/{todo_id}/duplicate"
        params: {
            todo_id: bins.id
        }
        as: Principal.user(cyd)
    }
    assert refused.status == 404
}
```


### Events

A restricted field read only in an event isn't an output. An event's
subscribers are server work, so `emit` stores nothing a caller reads.
What a subscriber stores from an event is checked where it stores it
([Subscribers](12-jobs-and-events.md#subscribers)). What a log holds is
covered by log redaction in [Personal data](15-personal-data.md).

## Fields shown by role
<!-- topic: fields-shown-by-role -->

Policy filters records by their `read:` grants, so a list can carry
something only some callers may see. Keep that part on a record of its
own whose `read:` names only them, read it with a query of its own, and
give the output a field `T?` for it: everyone who may see the record gets
it, and everyone else gets `none`. Here a company's projects are listed
to all its members, and each project's customer, with their email, only to
owners and editors.

The `Company`, its `CompanyRole` and the memberships that hold roles are as
in [Policy, end to end](#policy-end-to-end). A customer is a record of its
own, which viewers may not read at all:

```jadpo check project=roles-in-practice file=app.jadpo hidden
use work/entities { Member }

application {}

principal {
    user: {
        subject: Member.handle
    }
}
```

```jadpo check project=roles-in-practice file=authentication.jadpo hidden
authentication {
    browser_session: {
        cookie: "studio_session"
    }
}
```

```jadpo check project=roles-in-practice file=policy.jadpo hidden
use work/entities { Company }
use work

policy {
    tenant: Company
    actions: {
        work.found_company: [Access.authenticated]
    }
}
```

```jadpo check project=roles-in-practice file=domains/work/types/handle.jadpo hidden
type Handle = Text {
    min_length: 3
    max_length: 40
}
```

```jadpo check project=roles-in-practice file=domains/work/types/company_name.jadpo hidden
type CompanyName = Text {
    min_length: 1
    max_length: 80
}
```

```jadpo check project=roles-in-practice file=domains/work/types/company_role.jadpo hidden
/// What a member may do in a company.
type CompanyRole = Enum {
    owner
    editor
    viewer
}
```

```jadpo check project=roles-in-practice file=domains/work/types/project_name.jadpo hidden
type ProjectName = Text {
    min_length: 1
    max_length: 80
}
```

```jadpo check project=roles-in-practice file=domains/work/types/customer_name.jadpo hidden
type CustomerName = Text {
    min_length: 1
    max_length: 120
}
```

```jadpo check project=roles-in-practice file=domains/work/failures/handle_taken.jadpo hidden
type HandleTaken = Failure {
    kind: FailureKind.conflict
    message: "That handle is taken."
}
```

```jadpo check project=roles-in-practice file=domains/work/entities/member.jadpo hidden
use work/types { Handle }

/// Someone who signs in.
entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
        tenant: Tenancy.shared
    }
}
```

```jadpo check project=roles-in-practice file=domains/work/entities/company.jadpo hidden
use work/types { CompanyName, CompanyRole }

/// A customer: the tenant every other record belongs to.
entity Company {
    id: Uuid
    name: CompanyName

    policy: {
        read: [CompanyRole.owner, CompanyRole.editor, CompanyRole.viewer]
    }
}
```

```jadpo check project=roles-in-practice file=domains/work/entities/company_membership.jadpo hidden
use work/entities { Company, Member }
use work/types { CompanyRole }

/// A member's role in a company.
entity CompanyMembership {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    member_id: Member.id {
        on_delete: OnDelete.cascade
    }
    role: CompanyRole

    membership: {
        scope: company_id
        member: member_id
        role: role
    }

    policy: {
        create: [CompanyRole.owner]
        read: [CompanyRole.owner, member_id]
        delete: [CompanyRole.owner, member_id]
    }
}
```

```jadpo check project=roles-in-practice file=domains/work/actions/register.jadpo hidden
use work/entities { Member }
use work/types { Handle }
use work/failures { HandleTaken }

/// Signs a new member up; anyone may.
action register(handle: Handle) fails HandleTaken -> Member {
    return attempt Member.create {
        set: {
            handle: handle
        }
        conflict: {
            Member.handle: HandleTaken
        }
    }
}
```

```jadpo check project=roles-in-practice file=domains/work/actions/found_company.jadpo hidden
use work/entities { Company, CompanyMembership }
use work/types { CompanyName, CompanyRole }

/// Starts a company with its founder as its owner.
action found_company(name: CompanyName, founder: Principal.user) -> Company {
    var company: Company = Company.create {
        set: {
            name: name
        }
    }
    var owner: CompanyMembership = CompanyMembership.create {
        set: {
            company_id: company.id
            member_id: founder.id
            role: CompanyRole.owner
        }
    }
    assert owner.role == CompanyRole.owner
    return company
}
```

```jadpo check project=roles-in-practice file=domains/work/actions/add_member.jadpo hidden
use work/entities { Company, CompanyMembership, CompanyNotFound, Member, MemberNotFound }
use work/types { CompanyRole }

/// Gives a member a role in a company; only its owners may.
action add_member(
    company_id: Company.id,
    member_id: Member.id,
    role: CompanyRole
) fails CompanyNotFound, MemberNotFound -> CompanyMembership {
    return attempt CompanyMembership.create {
        set: {
            company_id: company_id
            member_id: member_id
            role: role
        }
    }
}
```

```jadpo check project=roles-in-practice file=domains/work/entities/customer.jadpo
use work/entities { Company }
use work/types { CompanyRole, CustomerName }

/// Who a project is for. Only a company's owners and editors see its
/// customers.
entity Customer {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    name: CustomerName
    email: Email

    policy: {
        create: [CompanyRole.owner, CompanyRole.editor]
        read: [CompanyRole.owner, CompanyRole.editor]
    }
}
```

```jadpo check project=roles-in-practice file=domains/work/entities/project.jadpo hidden
use work/entities { Company, Customer }
use work/types { CompanyRole, ProjectName }

/// A piece of work in a company, perhaps for a customer.
entity Project {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    name: ProjectName
    archived: Bool
    customer_id: Customer.id? {
        on_delete: OnDelete.cleared
    }

    policy: {
        create: [CompanyRole.owner, CompanyRole.editor]
        read: [CompanyRole.owner, CompanyRole.editor, CompanyRole.viewer]
        update: [CompanyRole.owner, CompanyRole.editor]
        delete: [CompanyRole.owner]
    }
}
```

The row holds the customer's fields as `T?`, so it is one shape for
everyone, as every output is:

```jadpo check project=roles-in-practice file=domains/work/types/project_row.jadpo
use work/entities { Customer, Project }

/// A project in a company's list. Its customer's name and email are there
/// only for those who may see the company's customers, and `none` for
/// everyone else.
type ProjectRow = Output {
    pick: [Project.id, Project.name]
    customer_name: Customer.name?
    contact_email: Customer.email?
}
```

```jadpo check project=roles-in-practice file=domains/work/types/project_source.jadpo hidden
use work/entities { Project }

/// A project as the list reads it, with the customer it refers to.
type ProjectSource = Object {
    pick: [Project.id, Project.name, Project.customer_id]
}
```

```jadpo check project=roles-in-practice file=domains/work/types/customer_card.jadpo hidden
use work/entities { Customer }

/// What the list shows of a customer.
type CustomerCard = Object {
    pick: [Customer.id, Customer.name, Customer.email]
}
```

```jadpo check project=roles-in-practice file=domains/work/queries/projects_of.jadpo hidden
use work/entities { Company, Project }
use work/types { ProjectSource }

/// A company's projects, by name.
query projects_of(company_id: Company.id) -> List<ProjectSource> {
    from: Project
    where: Project.company_id == company_id
    order: [Project.name asc]
    limit: 100
}
```

The customers come from a query of their own. For a viewer, policy keeps
every customer out of it, so it gives an empty list rather than failing:

```jadpo check project=roles-in-practice file=domains/work/queries/customers_of.jadpo
use work/entities { Company, Customer }
use work/types { CustomerCard }

/// The company's customers the caller may see: none for a viewer.
query customers_of(company_id: Company.id) -> List<CustomerCard> {
    from: Customer
    where: Customer.company_id == company_id
    order: [Customer.name asc]
    limit: every
}
```

The action puts the two together, by each project's `customer_id`. It
never asks for the caller's role: what policy let the caller read decides
what each row holds.

```jadpo check project=roles-in-practice file=domains/work/actions/project_list.jadpo
use work/entities { Company, Customer }
use work/types { CustomerCard, ProjectRow, ProjectSource }
use work/queries { customers_of, projects_of }

/// A company's projects, each with its customer for those who may see it.
action project_list(company_id: Company.id) -> List<ProjectRow> {
    var projects: List<ProjectSource> = projects_of(company_id)
    var customers: List<CustomerCard> = customers_of(company_id)
    var mut by_id: Map<Customer.id, CustomerCard> = []
    for customer in customers {
        by_id = map.with(by_id, customer.id, customer)
    }
    var mut rows: List<ProjectRow> = []
    for project in projects {
        var customer: CustomerCard? = match project.customer_id {
            some(customer_id) -> map.get(by_id, customer_id)
            none -> none
        }
        rows = list.append(
            rows,
            ProjectRow {
                id: project.id
                name: project.name
                customer_name: match customer {
                    some(found) -> found.name
                    none -> none
                }
                contact_email: match customer {
                    some(found) -> found.email
                    none -> none
                }
            }
        )
    }
    return rows
}
```

```jadpo check project=roles-in-practice file=domains/work/actions/project_list.test.jadpo
use work/entities { Company, CompanyMembership, Customer, Member, Project }
use work/types { CompanyRole, ProjectRow }
use work/actions { add_member, found_company, register }

test "a viewer sees each project without its customer" {
    var ann: Member = attempt register("ann")
    var bob: Member = attempt register("bob")
    var northwind: Company = found_company("Northwind", Principal.user(ann))
    var viewer: CompanyMembership = attempt add_member(northwind.id, bob.id, CompanyRole.viewer)
    assert viewer.role == CompanyRole.viewer
    var first_customer: Customer = make.Customer {
        company_id: northwind.id
        name: "Acme"
        email: "buyer@first_customer.example"
    }
    var launch: Project = make.Project {
        company_id: northwind.id
        name: "Launch"
        archived: false
        customer_id: first_customer.id
    }
    var for_ann: Response = request {
        method: HttpMethod.get
        path: "/projects/{company_id}"
        params: {
            company_id: northwind.id
        }
        as: Principal.user(ann)
    }
    assert for_ann.output == [
        ProjectRow {
            id: launch.id
            name: "Launch"
            customer_name: "Acme"
            contact_email: "buyer@first_customer.example"
        }
    ]
    var for_bob: Response = request {
        method: HttpMethod.get
        path: "/projects/{company_id}"
        params: {
            company_id: northwind.id
        }
        as: Principal.user(bob)
    }
    assert for_bob.output == [
        ProjectRow {
            id: launch.id
            name: "Launch"
            customer_name: none
            contact_email: none
        }
    ]
}
```

A field restricted on the project itself, with its own `policy:`,
can't be shown this way: a query whose callers include someone who may
not read one of its output fields doesn't compile
(`policy.output_field_unproved`). Put what only some roles see on a
record of its own, as `Customer` is here.

## A change that may match nothing
<!-- topic: a-change-that-may-match-nothing -->

Policy checks a change on each record it changes. A change to every
listed record that matches nothing changes nothing, so nothing is checked:
the action succeeds, writes nothing and says so. A caller who may not
make the change still can't make it to any record, and learns nothing
they couldn't read. So a bulk change needs no permission check of its
own. A role check written by hand is refused: the role would decide what
is stored, and is proved like a stored value
(`policy.output_field_unproved`).

Here archiving several of a company's projects at once is one query and
a loop of updates. A viewer who lists a project gets 403 and nothing is
archived, since the action is all-or-nothing. A viewer who lists none, or
only projects that aren't there, gets `{"archived": 0}`:

```jadpo check project=roles-in-practice file=domains/work/types/projects_to_archive.jadpo hidden
use work/entities { Project }

/// Which projects to archive.
type ProjectsToArchive = Input {
    project_ids: List<Project.id> {
        max_length: 100
    }
}
```

```jadpo check project=roles-in-practice file=domains/work/types/archived_count.jadpo hidden
/// How many projects an archive changed.
type ArchivedCount = Output {
    archived: Int
}
```

```jadpo check project=roles-in-practice file=domains/work/queries/listed_projects.jadpo
use work/entities { Company, Project }

/// The listed projects that are in the company and not yet archived.
query listed_projects(company_id: Company.id, project_ids: List<Project.id>) -> List<Project> {
    from: Project
    where: Project.company_id == company_id
    where: Project.id in project_ids
    where: Project.archived == false
    order: [Project.name asc]
    limit: 100
}
```

```jadpo check project=roles-in-practice file=domains/work/actions/archive_projects.jadpo
use work/entities { Company, Project }
use work/types { ArchivedCount, ProjectsToArchive }
use work/queries { listed_projects }

/// Archives each listed project of the company that isn't archived yet.
action archive_projects(company_id: Company.id, input: ProjectsToArchive) -> ArchivedCount {
    var found: List<Project> = listed_projects(company_id, input.project_ids)
    for project in found {
        Project.update(project) {
            set: {
                archived: true
            }
        }
    }
    return ArchivedCount {
        archived: list.count(found)
    }
}
```

```jadpo check project=roles-in-practice file=domains/work/routes/projects.jadpo hidden migrate=initial
use work/entities { Company }
use work/types { ArchivedCount, ProjectRow, ProjectsToArchive }
use work/actions { archive_projects, project_list }

/// A company's projects.
routes Projects {
    prefix: /projects

    /// A company's projects, with their customers for those who may see them.
    GET /{company_id: Company.id} {
        output: List<ProjectRow>
        run: project_list(company_id)
    }

    /// Archives several of a company's projects at once.
    POST /{company_id: Company.id}/archive {
        input: ProjectsToArchive
        output: ArchivedCount
        run: archive_projects(company_id, input)
    }
}
```

```jadpo check project=roles-in-practice file=domains/work/actions/archive_projects.test.jadpo
use work/entities { Company, CompanyMembership, Member, Project }
use work/types { ArchivedCount, CompanyRole, ProjectsToArchive }
use work/actions { add_member, found_company, register }
use work/queries { listed_projects }

test "each project archived is checked, and an archive of none checks nothing" {
    var ann: Member = attempt register("ann")
    var bob: Member = attempt register("bob")
    var northwind: Company = found_company("Northwind", Principal.user(ann))
    var viewer: CompanyMembership = attempt add_member(northwind.id, bob.id, CompanyRole.viewer)
    assert viewer.role == CompanyRole.viewer
    var launch: Project = make.Project {
        company_id: northwind.id
        name: "Launch"
        archived: false
        customer_id: none
    }
    var by_bob: Response = request {
        method: HttpMethod.post
        path: "/projects/{company_id}/archive"
        params: {
            company_id: northwind.id
        }
        as: Principal.user(bob)
        input: ProjectsToArchive {
            project_ids: [launch.id]
        }
    }
    assert by_bob.status == 403
    assert list.count(listed_projects(northwind.id, [launch.id])) == 1
    var none_by_bob: Response = request {
        method: HttpMethod.post
        path: "/projects/{company_id}/archive"
        params: {
            company_id: northwind.id
        }
        as: Principal.user(bob)
        input: ProjectsToArchive {
            project_ids: []
        }
    }
    assert none_by_bob.status == 200
    assert none_by_bob.output == ArchivedCount {
        archived: 0
    }
    var by_ann: Response = request {
        method: HttpMethod.post
        path: "/projects/{company_id}/archive"
        params: {
            company_id: northwind.id
        }
        as: Principal.user(ann)
        input: ProjectsToArchive {
            project_ids: [launch.id]
        }
    }
    assert by_ann.output == ArchivedCount {
        archived: 1
    }
}
```

## The permissions report
<!-- topic: the-permissions-report -->

Policy is written beside each entity, so nobody has to keep a central
copy. To see the whole app at once, read the report `jadpo check` writes:
`build/report/permissions.md`, with a `.json` twin. It says who can do
what to every entity and field, and which actions and queries are listed
separately.
Each list is shown as written, and an effect nobody is granted says so.

Here is part of the report for [Policy, end to end](#policy-end-to-end):

| `work.Project` | create | read | update | delete |
|---|---|---|---|---|
| the record | `CompanyRole.owner`, `CompanyRole.editor` | `CompanyRole.owner`, `CompanyRole.editor`, `CompanyRole.viewer` | `CompanyRole.owner`, `CompanyRole.editor` | `CompanyRole.owner` |

Belongs to a `Company` through `company_id`. Only an action listed under `actions:` changes `company_id`.

A field that narrows its entity's grants gets a row of its own beside "the
record". For the `Project` under [Field restrictions](#field-restrictions):

| `work.Project` | create | read | update | delete |
|---|---|---|---|---|
| the record | `CompanyRole.editor` | `CompanyRole.viewer`, `CompanyRole.editor` | `CompanyRole.editor` | `CompanyRole.owner` |
| `budget` | as the record | `CompanyRole.editor` | `CompanyRole.editor` | as the record |
| `import_reference` | server only | server only | server only | as the record |

An audited entity's line says how long each change is kept in its
history, which its `history` shows to those who may read the record
([Audit trail](16-built-in-services.md#audit-trail)).

When a change alters this table, the approval report shows it in plain
terms, such as "`CompanyRole.viewer` can now read `work.Project.budget`"
([Reviewing a change](17-debugging-and-review.md#reviewing-a-change)).

Policy adds no index of its own. Every field a condition filters on is a
reference (a user field, a role's or the tenant's reference, a
membership's scope and member), and each already has its own
[index](05-data.md#indexes). Keeping a reference inside the tenant finds
the record it points at by `id`, its primary key. A move to another tenant
finds the records referring to it by their reference, which has its
index. So neither needs an index or a migration of its own.

## Protecting policy
<!-- topic: protecting-policy -->

An agent can edit policy, but the change can't ship without a person's
approval, wherever it is written. The compiler compares the application
before and after, because access widens wherever policy is written and
also when a route becomes public.

Each widening needs an acceptance: a file in `acceptances/` in which a
person accepts that change
([Acceptances](18-records-and-manifest.md#acceptances)). Breaking API
changes and database drops are accepted the same way. `jadpo check`
compares the change with the default branch at its merge base through
`access.jadpo_record`, who can do what, which it keeps at the root beside
`api/`, and the API record. These need a person:

- a caller added to any grant list, an entity's or a field's
  (`library.Book.read: PolicyChange.widened`);
- a field's own row, its `FieldAccess.server_only`, its `immutable: true`
  or its `generated:` removed, from a field that is still there
  (`PolicyChange.restriction_removed`);
- tenancy loosened: an entity that belonged to the tenant now shared by
  every tenant, or the app's tenant gone (`PolicyChange.tenancy_loosened`),
  or a new entity shared by every tenant
  (`PolicyChange.shared_entity_added`);
- an action's line under `actions:` added or removed, since a listing
  replaces its entities' grants (`PolicyChange.listing_added`,
  `.listing_removed`), and a caller added to an action's line;
- a listed query's line removed, or a caller added to it;
- a route that needed a signed-in caller made open to anyone
  (`"main GET /books": PolicyChange.route_public`);
- a route that needed the person's passkey no longer asking for it
  (`"main POST /payouts/{payout_id: Uuid}/send":
  PolicyChange.confirmation_removed`);
- a decline of `policy.call_ungranted`, a security rule
  (`PolicyChange.call_ungranted_declined`);
- a field copied into a field people may read who may not read it
  (`"desk.Macro.body -> desk.Reply.body": PolicyChange.field_copied`),
  which only an acceptance allows
  ([What else shows a field](#what-else-shows-a-field));
- scopes loosened: `external_token:`'s `scopes:` removed, so its tokens
  are no longer narrowed; a declared scope reading another scope-token; or
  an effect naming a scope it didn't, or another
  (`PolicyChange.scopes_loosened`).

Narrowing needs no one: a caller removed, a restriction added, an
effect's scope removed or tokens narrowed for the first time, a query
newly listed, a new route even one anyone may call, a new entity that
belongs to the tenant. Access also widens when an output gains a field or
an action starts touching another entity; [`jadpo
approval`](17-debugging-and-review.md#reviewing-a-change) shows those ways
there.

Schedules and subscribers act as the system
([Policy and background work](#policy-and-background-work)), and what `Access.system` is granted
shows in the permissions report. The compiler's own purge of soft-deleted
records ([The purge schedule](12-jobs-and-events.md#the-purge-schedule)) is
the only operation outside policy. Erasing a person needs `delete` on
their record, and then reaches every record of theirs as purge does
([Erasure](15-personal-data.md#erasure)).

## When policy says no
<!-- topic: when-policy-says-no -->

Your code never handles a policy refusal itself. The compiler adds policy
to the SQL `WHERE` of every query and write, and indexes every field a
policy condition filters on. `jadpo sql` shows the policy each statement
carries. What a refused caller sees depends on what they asked for:

| Situation | The client sees |
|---|---|
| Not signed in, on a route that needs it | 401 `unauthenticated` ([Routes: signed in by default](08-identity.md#routes-signed-in-by-default)) |
| A record the caller may not see, read or changed by id | the code's own `not_found` failure, as if it did not exist |
| A list | only the records the caller may see, filtered before paging |
| A create, or an action they may not call | 403 `not_permitted` |

A hidden record reports the same `not_found` failure the code already
maps, so a caller can't tell it from one that doesn't exist. The log
records that policy concealed it
([Concealing records](04-failures.md#concealing-records)).

`not_permitted` is raised by the runtime and never listed in `fails`. The
generated API description lists the 403 wherever it can happen.

Writes are checked in the same statement as the change, so a membership
removed a moment earlier can't slip through.

A token issued to another application carries scopes, which say what
that application may do. Scopes only narrow: the token may do what both
the person's grants and its scopes allow ([Scopes](#scopes)).

### Policy, end to end

A whole application whose customers are companies. Members sign up, found
a company and give each other roles in it, and a company's projects are
seen and changed only by its members.

First, the principal, its one sign-in method, and root `policy.jadpo`.
The policy names the tenant, and the one action that needs other people
than its entities' grants: founding a company, since a company with no
members yet has no owner to grant anything:

```jadpo check project=policy-end-to-end file=app.jadpo
use work/entities { Member }

application {}

principal {
    user: {
        subject: Member.handle
    }
}
```

```jadpo check project=policy-end-to-end file=authentication.jadpo hidden
authentication {
    browser_session: {
        cookie: "projects_session"
    }
}
```

```jadpo check project=policy-end-to-end file=policy.jadpo
use work/entities { Company }
use work

policy {
    tenant: Company
    actions: {
        work.found_company: [Access.authenticated]
    }
}
```

```jadpo check project=policy-end-to-end file=domains/work/types/handle.jadpo hidden
type Handle = Text {
    min_length: 3
    max_length: 40
}
```

```jadpo check project=policy-end-to-end file=domains/work/types/company_name.jadpo hidden
type CompanyName = Text {
    min_length: 1
    max_length: 80
}
```

```jadpo check project=policy-end-to-end file=domains/work/types/company_role.jadpo hidden
/// What a member may do in a company.
type CompanyRole = Enum {
    owner
    editor
    viewer
}
```

```jadpo check project=policy-end-to-end file=domains/work/types/project_name.jadpo hidden
type ProjectName = Text {
    min_length: 1
    max_length: 80
}
```

```jadpo check project=policy-end-to-end file=domains/work/types/project_view.jadpo hidden
use work/entities { Project }

/// A project as members see it.
type ProjectView = Output {
    pick: [Project.id, Project.name]
}
```

```jadpo check project=policy-end-to-end file=domains/work/failures/handle_taken.jadpo hidden
type HandleTaken = Failure {
    kind: FailureKind.conflict
    message: "That handle is taken."
}
```

A member may read only their own record, and anyone may sign up. Members
are shared by every company. The membership says where its role is held,
who holds it and which field holds it. A company's records follow the
roles held in it:

```jadpo check project=policy-end-to-end file=domains/work/entities/member.jadpo
use work/types { Handle }

/// Someone who signs in.
entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
        tenant: Tenancy.shared
    }
}
```

```jadpo check project=policy-end-to-end file=domains/work/entities/company.jadpo
use work/types { CompanyName, CompanyRole }

/// A customer: the tenant every other record belongs to.
entity Company {
    id: Uuid
    name: CompanyName

    policy: {
        read: [CompanyRole.owner, CompanyRole.editor, CompanyRole.viewer]
    }
}
```

```jadpo check project=policy-end-to-end file=domains/work/entities/company_membership.jadpo
use work/entities { Company, Member }
use work/types { CompanyRole }

/// A member's role in a company.
entity CompanyMembership {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    member_id: Member.id {
        on_delete: OnDelete.cascade
    }
    role: CompanyRole

    membership: {
        scope: company_id
        member: member_id
        role: role
    }

    policy: {
        create: [CompanyRole.owner]
        read: [CompanyRole.owner, member_id]
        delete: [CompanyRole.owner, member_id]
    }
}
```

```jadpo check project=policy-end-to-end file=domains/work/entities/project.jadpo
use work/entities { Company }
use work/types { CompanyRole, ProjectName }

/// A piece of work in a company.
entity Project {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    name: ProjectName

    policy: {
        create: [CompanyRole.owner, CompanyRole.editor]
        read: [CompanyRole.owner, CompanyRole.editor, CompanyRole.viewer]
        update: [CompanyRole.owner, CompanyRole.editor]
        delete: [CompanyRole.owner]
    }
}
```

Founding a company is the listed action: anyone signed in may call it,
and it makes its founder the owner. Adding a member is an ordinary create
of a membership, so only the company's owners may do it. The code says
nothing about who may do either:

```jadpo check project=policy-end-to-end file=domains/work/actions/register.jadpo hidden
use work/entities { Member }
use work/types { Handle }
use work/failures { HandleTaken }

/// Signs a new member up; anyone may.
action register(handle: Handle) fails HandleTaken -> Member {
    return attempt Member.create {
        set: {
            handle: handle
        }
        conflict: {
            Member.handle: HandleTaken
        }
    }
}
```

```jadpo check project=policy-end-to-end file=domains/work/actions/found_company.jadpo
use work/entities { Company, CompanyMembership }
use work/types { CompanyName, CompanyRole }

/// Starts a company with its founder as its owner.
action found_company(name: CompanyName, founder: Principal.user) -> Company {
    var company: Company = Company.create {
        set: {
            name: name
        }
    }
    var owner: CompanyMembership = CompanyMembership.create {
        set: {
            company_id: company.id
            member_id: founder.id
            role: CompanyRole.owner
        }
    }
    assert owner.role == CompanyRole.owner
    return company
}
```

```jadpo check project=policy-end-to-end file=domains/work/actions/add_member.jadpo
use work/entities { Company, CompanyMembership, CompanyNotFound, Member, MemberNotFound }
use work/types { CompanyRole }

/// Gives a member a role in a company; only its owners may.
action add_member(
    company_id: Company.id,
    member_id: Member.id,
    role: CompanyRole
) fails CompanyNotFound, MemberNotFound -> CompanyMembership {
    return attempt CompanyMembership.create {
        set: {
            company_id: company_id
            member_id: member_id
            role: role
        }
    }
}
```

```jadpo check project=policy-end-to-end file=domains/work/actions/start_project.jadpo hidden
use work/entities { Company, CompanyNotFound, Project }
use work/types { ProjectName, ProjectView }

/// What starting a project takes.
type StartProject = Input {
    company_id: Company.id
    name: ProjectName
}

/// Starts a project in a company.
action start_project(input: StartProject) fails CompanyNotFound -> ProjectView {
    var project: Project = attempt Project.create {
        set: {
            company_id: input.company_id
            name: input.name
        }
    }
    return ProjectView {
        id: project.id
        name: project.name
    }
}
```

```jadpo check project=policy-end-to-end file=domains/work/actions/rename_project.jadpo hidden
use work/entities { Project, ProjectNotFound }
use work/types { ProjectName, ProjectView }

/// What renaming a project takes.
type RenameProject = Input {
    name: ProjectName
}

/// Renames a project.
action rename_project(
    project_id: Project.id,
    input: RenameProject
) fails ProjectNotFound -> ProjectView {
    var project: Project = attempt Project.update(project_id) {
        set: {
            name: input.name
        }
    }
    return ProjectView {
        id: project.id
        name: project.name
    }
}
```

The query names no caller and no company check: policy adds both to its
`WHERE`, so a caller sees only the projects of companies where they hold a
role that may read them:

```jadpo check project=policy-end-to-end file=domains/work/queries/company_projects.jadpo
use work/entities { Company, Project }
use work/types { ProjectView }

/// A company's projects, by name.
query company_projects(company_id: Company.id) -> List<ProjectView> {
    from: Project
    where: Project.company_id == company_id
    order: [Project.name asc]
    limit: 100
}
```

```jadpo check project=policy-end-to-end file=domains/work/routes/projects.jadpo migrate=initial
use work/entities { Company, Project }
use work/types { ProjectView }
use work/actions { RenameProject, StartProject, rename_project, start_project }
use work/queries { company_projects }

/// A company's projects.
routes Projects {
    prefix: /projects

    /// Starts a project.
    POST / {
        input: StartProject
        output: ProjectView
        status: SuccessStatus.created
        run: start_project(input)
    }

    /// A company's projects the caller may see.
    GET /{company_id: Company.id} {
        output: List<ProjectView>
        run: company_projects(company_id)
    }

    /// Renames a project.
    PATCH /{project_id: Project.id} {
        input: RenameProject
        output: ProjectView
        run: rename_project(project_id, input)
    }
}
```

```jadpo check project=policy-end-to-end file=api/main.jadpo_record hidden
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

api main
    authentication browser_session

GET /openapi.json
    generated openapi
    status 200

POST /projects
    input StartProject (body)
    output ProjectView
    status 201
    fails company_not_found

GET /projects/{company_id: Uuid}
    output List<ProjectView>
    status 200

PATCH /projects/{project_id: Uuid}
    input RenameProject (body)
    output ProjectView
    status 200
    fails project_not_found

output ProjectView
    id: Uuid
    name: Text { min_length: 1, max_length: 80 }

input RenameProject
    name: Text { min_length: 1, max_length: 80 }

input StartProject
    company_id: Uuid
    name: Text { min_length: 1, max_length: 80 }

failure company_not_found
    status 404

failure project_not_found
    status 404
```

A route test signs in as each member, so what it checks is exactly what a
browser would see. The test sets up its data by calling actions without a
caller, which runs them without policy. `found_company` takes its founder,
so it runs as the founder and its grants apply:

```jadpo check project=policy-end-to-end file=domains/work/routes/projects.test.jadpo
use work/entities { Company, CompanyMembership, Member }
use work/types { CompanyRole, ProjectView }
use work/actions { RenameProject, StartProject, add_member, found_company, register, start_project }

test "a company's projects are listed only to its members" {
    var ann: Member = attempt register("ann")
    var bob: Member = attempt register("bob")
    var cy: Member = attempt register("cyd")
    var northwind: Company = found_company("Northwind", Principal.user(ann))
    var viewer: CompanyMembership = attempt add_member(northwind.id, bob.id, CompanyRole.viewer)
    assert viewer.role == CompanyRole.viewer
    var launch: ProjectView = attempt start_project(
        StartProject {
            company_id: northwind.id
            name: "Launch"
        }
    )
    var seen: Response = request {
        method: HttpMethod.get
        path: "/projects/{company_id}"
        params: {
            company_id: northwind.id
        }
        as: Principal.user(bob)
    }
    assert seen.status == 200
    assert seen.output == [launch]
    var outsider: Response = request {
        method: HttpMethod.get
        path: "/projects/{company_id}"
        params: {
            company_id: northwind.id
        }
        as: Principal.user(cy)
    }
    assert outsider.output == []
}

test "a viewer can't rename a project, which reads as missing" {
    var ann: Member = attempt register("ann")
    var bob: Member = attempt register("bob")
    var northwind: Company = found_company("Northwind", Principal.user(ann))
    var viewer: CompanyMembership = attempt add_member(northwind.id, bob.id, CompanyRole.viewer)
    assert viewer.role == CompanyRole.viewer
    var launch: ProjectView = attempt start_project(
        StartProject {
            company_id: northwind.id
            name: "Launch"
        }
    )
    var by_bob: Response = request {
        method: HttpMethod.patch
        path: "/projects/{project_id}"
        params: {
            project_id: launch.id
        }
        as: Principal.user(bob)
        input: RenameProject {
            name: "Plans"
        }
    }
    assert by_bob.status == 404
    assert by_bob.code == "project_not_found"
    var by_ann: Response = request {
        method: HttpMethod.patch
        path: "/projects/{project_id}"
        params: {
            project_id: launch.id
        }
        as: Principal.user(ann)
        input: RenameProject {
            name: "Plans"
        }
    }
    assert by_ann.status == 200
}

test "someone outside a company can't start a project in it" {
    var ann: Member = attempt register("ann")
    var cyd: Member = attempt register("cyd")
    var northwind: Company = found_company("Northwind", Principal.user(ann))
    var response: Response = request {
        method: HttpMethod.post
        path: "/projects"
        as: Principal.user(cyd)
        input: StartProject {
            company_id: northwind.id
            name: "Takeover"
        }
    }
    assert response.status == 403
    assert response.code == "not_permitted"
}
```

## Scopes
<!-- topic: scopes -->

A person can let another application act for them, such as a partner's app
holding a token their company's identity provider issued it. Such a token
carries scopes: what the person let that application do.
[`external_token:`](08-identity.md#external_token) declares the scopes its
tokens carry, and every such token is narrowed to them. It may do what
both the person's grants and its scopes allow, and never more than the
person could do themselves.

An entity's `policy:` says, in `scopes:`, which scope a narrowed token
needs for each effect, as well as a grant. Here a partner's app reads a
diarist's notes with `notes:read`, and writes them with `notes.write`:

```jadpo check project=scoped-notes
config {
    audience: Text
    issuer: Url
}

principal {
    user: {
        subject: Diarist.handle
    }
}

authentication {
    browser_session: {
        cookie: "notebook_session"
    }
    external_token: {
        issuer: config.issuer
        audience: config.audience
        scopes: {
            notes_read: "notes:read"
            notes_write: "notes.write"
        }
    }
}

type Handle = Text {
    max_length: 40
}

entity Diarist {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        read: [id]
    }
}

entity Note {
    id: Uuid
    diarist_id: Diarist.id {
        on_delete: OnDelete.cascade
    }
    text: Text

    policy: {
        create: [diarist_id]
        read: [diarist_id]
        update: [diarist_id]
        delete: [diarist_id]
        scopes: {
            create: OAuthScope.notes_write
            read: OAuthScope.notes_read
            update: OAuthScope.notes_write
        }
    }
}
```

- Each effect names one scope, `OAuthScope.<name>`. `OAuthScope` here
  holds only the scopes `authentication.jadpo` declares, so a misspelt or
  undeclared scope is `policy.binding_invalid`. A list would read as "any
  of", and a scope must hold as well as a grant, so there is none.
- An effect `scopes:` leaves out is refused to every narrowed token. So
  no token deletes a note, and none reads a `Diarist`, whose policy names
  no scope: a new entity is never open to other applications by accident.
- `Access.public` and `Access.authenticated` are grants like any other: a
  narrowed token still needs the effect's scope.
- Browser sessions, API tokens and service keys carry no scopes, and
  neither do schedules and subscribers, so scopes never touch them. Nor do
  they touch a token from a provider whose `external_token:` declares none.

A narrowed token is refused as any caller policy refuses is ([When policy
says no](#when-policy-says-no)): a note it may not read is the code's own
`not_found` failure, a list leaves it out, and a create is 403
`not_permitted`. The check is in each statement's `WHERE`, as every grant
is.

Some things no effect's scope covers, so a narrowed token is refused them:

- an action listed under `actions:` that is checked as it starts, since
  no entity's `scopes:` names it. A listed query is let through, and its
  records are still filtered by their entity's `read:` scope;
- starting a session or issuing an API token, `sessions.start` and
  `sessions.issue_token` (403 `not_permitted`), since either would carry
  the person's whole authority;
- a slot for an upload, unless it carries the scope of its entity's
  `create:` or `update:`.

The [permissions report](#the-permissions-report) gives each entity a
"scope" row, with "refused" where no scope lets a narrowed token in, and
`access.jadpo_record` keeps the declared scopes and each effect's.
Loosening them needs a person ([Protecting policy](#protecting-policy)). A
route test says which scopes its token carries with `scopes:`
([Signing in for a route test](13-testing.md#signing-in-for-a-route-test)).

## Caches and personal data
<!-- topic: caches-and-personal-data -->

A [cache](05-data.md#caches) never lets a caller around policy. A record
cache keeps one copy of each record for every caller and applies policy
to it on every read, by what the record holds: `Access.public`,
`Access.authenticated`, `Access.system` and a user field. A role or a
tenant reads memberships from the database as policy is checked, so an
entity with one isn't given a record cache (`data.cache_invalid`). A
query-result cache is keyed by the tenant and every input policy used.
Policy itself always reads the database.

[Personal data](15-personal-data.md) takes its permissions from policy:
erasing a person's data needs `delete` on their record, and exporting it
needs `read`.

## What is built
<!-- topic: policy-built -->

Everything on this page is built except the features below. Each is
`toolchain.not_built_yet` where you write it.

| Feature | Status |
|---|---|
| Grants, user fields, membership roles, roles that need a way in (`requires:`), `Access.authenticated`, `Access.public` and `Access.system`, tenancy, `actions:` with listed queries and `policy.call_ungranted`, field restrictions, the permissions report, and the refusals under "When policy says no" | Built. |
| [Scopes](#scopes): `scopes:`, with the tokens `external_token` narrows | Built. |
| Scopes on `oauth_token` tokens | Not built yet: they come with acting as an OAuth provider ([What is built: Sign-in and identity](08-identity.md#what-is-built)). |
| An acceptance for a change that widens access, with `access.jadpo_record` ([Acceptances](18-records-and-manifest.md#acceptances)) | Built. |
| A field copy a person approves, `PolicyChange.field_copied` ([What else shows a field](#what-else-shows-a-field)) | Built. |
| The approval report that shows the report's changes in plain terms ([Reviewing a change](17-debugging-and-review.md#reviewing-a-change)) | Built. |

## Errors

Every error this area's checks produce, with an example that triggers it.
The examples are one file each, so the principal and root `policy { }` sit
beside the entities they name; a project keeps them in `app.jadpo` and
`policy.jadpo`.

### Owner fields and memberships

- [`policy.role_unbound`](../errors/policy.role_unbound.md)
- [`policy.binding_invalid`](../errors/policy.binding_invalid.md)
- [`policy.membership_scope_required`](../errors/policy.membership_scope_required.md)
- [`policy.membership_member_required`](../errors/policy.membership_member_required.md)
- [`policy.membership_role_required`](../errors/policy.membership_role_required.md)
- [`policy.membership_duplicate`](../errors/policy.membership_duplicate.md)
- [`policy.requirement_invalid`](../errors/policy.requirement_invalid.md)
- [`policy.requirements_incomplete`](../errors/policy.requirements_incomplete.md)
- [`policy.requirement_unsatisfiable`](../errors/policy.requirement_unsatisfiable.md)
### Scope

- [`policy.scope_missing`](../errors/policy.scope_missing.md)
- [`policy.scope_ambiguous`](../errors/policy.scope_ambiguous.md)
- [`policy.scope_duplicate`](../errors/policy.scope_duplicate.md)
### Grants

- [`policy.effect_ungranted`](../errors/policy.effect_ungranted.md)
- [`policy.call_ungranted`](../errors/policy.call_ungranted.md)
- [`policy.effect_unknown`](../errors/policy.effect_unknown.md)
- [`policy.effect_duplicate`](../errors/policy.effect_duplicate.md)
- [`policy.subject_duplicate`](../errors/policy.subject_duplicate.md)
- [`policy.declaration_duplicate`](../errors/policy.declaration_duplicate.md)
- [`policy.system_grant_missing`](../errors/policy.system_grant_missing.md)
- [`policy.event_tenant_unproved`](../errors/policy.event_tenant_unproved.md)
- [`policy.batch_tenant_missing`](../errors/policy.batch_tenant_missing.md)
- [`policy.public_grant_missing`](../errors/policy.public_grant_missing.md)
### Fields

- [`type.field_update_forbidden`](../errors/type.field_update_forbidden.md)
- [`policy.field_widens_entity`](../errors/policy.field_widens_entity.md)
- [`policy.output_field_unproved`](../errors/policy.output_field_unproved.md)
- [`policy.role_field_create_forbidden`](../errors/policy.role_field_create_forbidden.md)
