# `policy.requirement_invalid`

A role's line in `requires:` holds something a requirement can't.

## Why Jadpo refuses it

A role's line says how its holder must have signed in for the role to
count: `sign_in:`, the company's ways in, any of which will do, and
`strength: AuthenticationStrength.multi_factor`. Here it lists a magic
link, which proves only that someone reads an address. A requirement
names the company's own way in, a sign-in provider or an external token,
so the person's access ends where the company ends it.

## What it looks like

main.jadpo:

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
 --> main.jadpo:80:27
   |
80 |                 sign_in: [SignInMethod.magic_link]
   |                           ^^^^^^^^^^^^^^^^^^^^^^^ not a provider
  reason: A requirement names the company's own way in, a sign-in provider or `external_token`, so access ends where the company ends it; a magic link or a passkey proves only an address or a device. Use `strength: AuthenticationStrength.multi_factor` to ask for a second factor.
  next: Name a sign-in provider or `SignInMethod.external_token` (agent)
  rule: policy.requirement_invalid
  help: https://jadpo.dev/docs/errors/policy.requirement_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

The same error is raised for an empty `sign_in:`, which would let no one
hold the role; for `strength: AuthenticationStrength.primary`, which
every sign-in meets; for anything other than `sign_in:` and `strength:`;
for a passkey listed in `sign_in:`; and for a role given two lines.

## How to fix it

**If the role should need the company's provider**, name it:

```jadpo fragment
        requires: {
            StaffRole.admin: {
                sign_in: [SignInMethod.google]
            }
            ...
        }
```

```jadpo check project=e-requirement-invalid-google hidden
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

The admin role now counts only for someone signed in through Google.
To ask for a second factor as well, add
`strength: AuthenticationStrength.multi_factor` to the line; a passkey
gives one.

## Related

- [Roles that need a way in](../language/09-policy.md#roles-that-need-a-way-in)
- [How strongly a person signed in](../language/08-identity.md#how-strongly-a-person-signed-in)
- [Providers](../language/08-identity.md#providers)
