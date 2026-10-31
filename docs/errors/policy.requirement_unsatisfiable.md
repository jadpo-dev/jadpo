# `policy.requirement_unsatisfiable`

A role counts only for someone whose sign-in meets its requirement.
When no way in the app declares could ever meet it, no one can hold the
role, so it is refused where it is written. Here the admin role needs an
external token, which `authentication { }` doesn't declare:

```jadpo check project=e-requirement-unsatisfiable expect=policy.requirement_unsatisfiable
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
                sign_in: [SignInMethod.external_token]
            }
            StaffRole.front_desk: {
                sign_in: [SignInMethod.google]
            }
        }
    }
}
```

```text output
error: No one can hold `StaffRole.admin`: nothing the app signs people in with is signed in through `external_token`
 --> main.jadpo:79:13
   |
79 |             StaffRole.admin: {
   |             ^^^^^^^^^^^^^^^ can never be met
  reason: A role counts only for someone whose sign-in meets its requirement; the ways in the app declares that a requirement may name are `SignInMethod.google`.
  next: Name a way in the app declares, or declare the one you mean in `authentication.jadpo` (human)
  rule: policy.requirement_unsatisfiable
  help: https://jadpo.dev/docs/errors/policy.requirement_unsatisfiable

check failed: 1 error, 1 file
```

Name a way in the app declares, or declare the one you mean in
`authentication.jadpo` ([Ways to sign in](../language/08-identity.md#ways-to-sign-in)).
A requirement with only `strength: AuthenticationStrength.multi_factor`
is met by a passkey or by a provider or external token whose token says
so. See
[Roles that need a way in](../language/09-policy.md#roles-that-need-a-way-in).
