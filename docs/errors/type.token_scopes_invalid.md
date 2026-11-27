# `type.token_scopes_invalid`

`sessions.issue_token` is given `scopes:` that aren't a list of scopes `api_token: { scopes: }` declares.

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
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

Declare the scope in `api_token:`, with the scope-token the token
carries for it:

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

## Related

- [API tokens](../language/08-identity.md#api-tokens)
- [Scopes](../language/09-policy.md#scopes)
