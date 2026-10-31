# `policy.requirements_incomplete`

Once a membership's `requires:` says how one role must be signed in to,
it says so of every role, so a role added to the enum later is decided
rather than open by default:

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

Give each role a line; `StaffRole.front_desk: {}` says any sign-in will
do. A person decides each: who keeps a role, and where their access is
revoked, follows from it. See
[Roles that need a way in](../language/09-policy.md#roles-that-need-a-way-in).
