# `type.token_scopes_invalid`

Code gives `sessions.issue_token` `scopes:` that aren't a list of scopes `api_token: { scopes: }` declares.

## Why Jadpo refuses it

`sessions.issue_token(user_id, scopes: [OAuthScope.<name>])` issues an
API token narrowed to the scopes listed, which policy then holds it to,
as it holds a provider's narrowed token
([Scopes](../language/09-policy.md#scopes)). Each must be one
`api_token: { scopes: }` declares, listed once, and at least one. A
token with no `scopes:` may do whatever its person may.

## What it looks like

main.jadpo:

```jadpo check project=e-token-scopes expect=type.token_scopes_invalid
principal {
    user: {
        subject: Member.handle
    }
}

authentication {
    api_token: {}
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
    }
}

type AppSignedIn = Output {
    token: ApiToken
}

/// A token for the member's app that may only read.
action reading_token(member: Principal.user) -> AppSignedIn {
    return AppSignedIn {
        token: sessions.issue_token(member.id, scopes: [OAuthScope.notes_read])
    }
}
```

```text output
error: `sessions.issue_token` is given scopes, and `api_token:` declares none
 --> main.jadpo:34:56
   |
34 |         token: sessions.issue_token(member.id, scopes: [OAuthScope.notes_read])
   |                                                        ^^^^^^^^^^^^^^^^^^^^^^^ no scopes declared
  reason: `sessions.issue_token(user_id, scopes: [OAuthScope.<name>])` narrows the token to the scopes listed, each one `api_token: { scopes: }` declares, once; leave `scopes:` out for a token that may do whatever its person may. `api_token:` in `authentication { }` declares no `scopes:`, so there is nothing to narrow a token to.
  next: Declare the scopes in `api_token: { scopes: { <name>: "<scope-token>" } }`, or remove `scopes:` (agent)
  rule: type.token_scopes_invalid
  help: https://jadpo.dev/docs/errors/type.token_scopes_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself: it declares the scope.

## How to fix it

**If the token should carry `notes_read`**, declare it in `api_token:`,
with the scope-token the token carries for it:

```jadpo fragment
authentication {
    api_token: {
        scopes: {
            notes_read: "notes:read"
        }
    }
}
```

```jadpo check project=e-token-scopes-fix hidden
principal {
    user: {
        subject: Member.handle
    }
}

authentication {
    api_token: {
        scopes: {
            notes_read: "notes:read"
        }
    }
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
    }
}

type AppSignedIn = Output {
    token: ApiToken
}

/// A token for the member's app that may only read.
action reading_token(member: Principal.user) -> AppSignedIn {
    return AppSignedIn {
        token: sessions.issue_token(member.id, scopes: [OAuthScope.notes_read])
    }
}
```

The token is now narrowed to `notes:read`, and policy holds it to the
grants whose effects name that scope.

## Related

- [Narrowing an API token to scopes](../language/08-identity.md#narrowing-an-api-token-to-scopes)
- [Scopes](../language/09-policy.md#scopes)
