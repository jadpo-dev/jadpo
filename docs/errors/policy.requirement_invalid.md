# `policy.requirement_invalid`

A role's line in a membership's `requires:` says how its holder must have
signed in for the role to count: `sign_in:`, a list of ways in, any of
which will do, and `strength: AuthenticationStrength.multi_factor`. A
magic link or a passkey can't be listed: each proves an address or a
device, not that the company still vouches for the person, so access
wouldn't end where the company ends it:

```jadpo check project=e-requirement-invalid expect=policy.requirement_invalid
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
                sign_in: [SignInMethod.magic_link]
            }
            StaffRole.front_desk: {}
        }
    }
}
```

```text output
error: A role can't require `SignInMethod.magic_link`
 --> main.jadpo:79:42
   |
79 |             StaffRole.admin: { sign_in: [SignInMethod.magic_link] }
   |                                          ^^^^^^^^^^^^^^^^^^^^^^^ not a provider
  reason: A requirement names the company's own way in, a sign-in provider or `external_token`, so access ends where the company ends it; a magic link or a passkey proves only an address or a device. Use `strength: AuthenticationStrength.multi_factor` to ask for a second factor.
  next: Name a sign-in provider or `SignInMethod.external_token` (agent)
  rule: policy.requirement_invalid
  help: https://jadpo.dev/docs/errors/policy.requirement_invalid

check failed: 1 error, 1 file
```

An empty `sign_in:`, which would let no one hold the role,
`strength: AuthenticationStrength.primary`, which every sign-in meets,
anything other than `sign_in:` and `strength:`, and a role given twice
are this error too. Name the company's provider, and ask for a second
factor with `strength:`. See
[Roles that need a way in](../language/09-policy.md#roles-that-need-a-way-in).
