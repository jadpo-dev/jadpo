# `policy.requirements_incomplete`

A membership's `requires:` leaves out one of its roles.

## Why Jadpo refuses it

`requires:` says how each role's holder must have signed in for the role
to count. Once it says so of one role, it says so of every role, so a
role nobody has decided about doesn't count however its holder signed
in, and a value added to the role's enum later is decided too rather
than open by default. Here `StaffRole.admin` needs Google, and
`StaffRole.front_desk` has no line.

Jadpo won't choose for you: whether the front desk must sign in through
the company's provider is a policy decision.

## What it looks like

main.jadpo:

```jadpo check project=e-requirements-incomplete expect=policy.requirements_incomplete
application {
    connections: {
        google: {
            driver: Driver.oidc
            issuer: config.google_issuer
            client_id: config.google_client_id
            credential: config.google_client_secret
        }
    }
}

config {
    google_client_id: Text
    google_client_secret: Text {
        secret: true
    }
    google_issuer: Url
    site: Url
}

type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        create: [Access.system]
        read: [id]
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NotStaff = Failure {
    kind: FailureKind.not_found
}

action meet(first: FirstSignIn) fails NotStaff -> User.id {
    reject NotStaff {}
}

authentication {
    browser_session: {
        cookie: "desk_session"
        origin: config.site
    }
    sign_in: {
        providers: [google]
    }
    on_first_sign_in: meet
}

type StaffRole = Enum {
    admin
    front_desk
}

entity StaffMember {
    id: Uuid
    user_id: User.id {
        on_delete: OnDelete.cascade
    }
    role: StaffRole

    membership: {
        scope: MembershipScope.application
        member: user_id
        role: role
        requires: {
            StaffRole.admin: {
                sign_in: [SignInMethod.google]
            }
        }
    }
}
```

```text output
error: `requires:` says nothing of `StaffRole.front_desk`
 --> main.jadpo:78:9
   |
78 |         requires: {
   |         ^^^^^^^^ not every role
  reason: Once a membership says how one role must be signed in to, it says so of every role, so a role added to `StaffRole` later is decided, not open by default; `StaffRole.<value>: {}` needs nothing.
  next: Add a line for `StaffRole.front_desk`, `{}` where any sign-in will do (human)
  rule: policy.requirements_incomplete
  help: https://jadpo.dev/docs/errors/policy.requirements_incomplete

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides how the front desk must sign in to keep its role.

## How to fix it

The question is whether the front desk, like the admins, must sign in
through Google.

**If the front desk needs Google too**, give it the same line:

```jadpo fragment
        requires: {
            StaffRole.admin: {
                sign_in: [SignInMethod.google]
            }
            StaffRole.front_desk: {
                sign_in: [SignInMethod.google]
            }
        }
```

```jadpo check project=e-requirements-incomplete-google hidden
application {
    connections: {
        google: {
            driver: Driver.oidc
            issuer: config.google_issuer
            client_id: config.google_client_id
            credential: config.google_client_secret
        }
    }
}

config {
    google_client_id: Text
    google_client_secret: Text {
        secret: true
    }
    google_issuer: Url
    site: Url
}

type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        create: [Access.system]
        read: [id]
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NotStaff = Failure {
    kind: FailureKind.not_found
}

action meet(first: FirstSignIn) fails NotStaff -> User.id {
    reject NotStaff {}
}

authentication {
    browser_session: {
        cookie: "desk_session"
        origin: config.site
    }
    sign_in: {
        providers: [google]
    }
    on_first_sign_in: meet
}

type StaffRole = Enum {
    admin
    front_desk
}

entity StaffMember {
    id: Uuid
    user_id: User.id {
        on_delete: OnDelete.cascade
    }
    role: StaffRole

    membership: {
        scope: MembershipScope.application
        member: user_id
        role: role
        requires: {
            StaffRole.admin: {
                sign_in: [SignInMethod.google]
            }
            StaffRole.front_desk: {
                sign_in: [SignInMethod.google]
            }
        }
    }
}
```

The front desk's role now counts only for someone signed in through
Google; signed in another way, they are themselves without it.

**If any sign-in will do**, say so with an empty line:

```jadpo fragment
        requires: {
            StaffRole.admin: {
                sign_in: [SignInMethod.google]
            }
            StaffRole.front_desk: {}
        }
```

```jadpo check project=e-requirements-incomplete-any hidden
application {
    connections: {
        google: {
            driver: Driver.oidc
            issuer: config.google_issuer
            client_id: config.google_client_id
            credential: config.google_client_secret
        }
    }
}

config {
    google_client_id: Text
    google_client_secret: Text {
        secret: true
    }
    google_issuer: Url
    site: Url
}

type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        create: [Access.system]
        read: [id]
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NotStaff = Failure {
    kind: FailureKind.not_found
}

action meet(first: FirstSignIn) fails NotStaff -> User.id {
    reject NotStaff {}
}

authentication {
    browser_session: {
        cookie: "desk_session"
        origin: config.site
    }
    sign_in: {
        providers: [google]
    }
    on_first_sign_in: meet
}

type StaffRole = Enum {
    admin
    front_desk
}

entity StaffMember {
    id: Uuid
    user_id: User.id {
        on_delete: OnDelete.cascade
    }
    role: StaffRole

    membership: {
        scope: MembershipScope.application
        member: user_id
        role: role
        requires: {
            StaffRole.admin: {
                sign_in: [SignInMethod.google]
            }
            StaffRole.front_desk: {}
        }
    }
}
```

The front desk's role counts however its holder signed in, as it did
before `requires:`. Changing a role's line later, either way, needs a
person's acceptance once the app is on its default branch
([Acceptances](../language/18-records-and-manifest.md#acceptances)).

## Related

- [Roles that need a way in](../language/09-policy.md#roles-that-need-a-way-in)
- [A membership role](../language/09-policy.md#a-membership-role)
- [Providers](../language/08-identity.md#providers)
