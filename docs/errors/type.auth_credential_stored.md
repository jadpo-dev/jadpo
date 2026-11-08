# `type.auth_credential_stored`

An entity has a field that holds an API token or a service key.

## Why Jadpo refuses it

An API token or a service key signs someone in. Jadpo shows each one
once, in the output of the action that issues it, and keeps only a
verifier of it, so a copy of the database signs no one in. A field that
kept the token itself would put a working credential in every backup and
every read of the record.

## What it looks like

main.jadpo:

```jadpo check project=e-auth-credential-stored expect=type.auth_credential_stored
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
    last_token: ApiToken
}
```

```text output
error: `Member.last_token` keeps an API token
 --> main.jadpo:20:17
   |
20 |     last_token: ApiToken
   |                 ^^^^^^^^ a secret, stored
  reason: An API token is shown once, in the output of the action that issues it, and the runtime keeps only its verifier, so a copy of the database signs no one in.
  next: Remove `last_token`, and give the token in the output of the action that issues it (agent)
  rule: type.auth_credential_stored
  help: https://jadpo.dev/docs/errors/type.auth_credential_stored

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A `ServiceKey`, a `SignInCode` or a `SignInToken` kept in an entity is
the same error.

## How to fix it

**If the client needs the token**, remove the field and return the token
from the action that issues it, in its output:

```jadpo fragment
type AppSignedIn = Output {
    token: ApiToken
}

action issue_app_token(member_id: Member.id) -> AppSignedIn {
    return AppSignedIn {
        token: sessions.issue_token(member_id)
    }
}
```

```jadpo check project=e-auth-credential-stored-fix hidden
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
}

type AppSignedIn = Output {
    token: ApiToken
}

action issue_app_token(member_id: Member.id) -> AppSignedIn {
    return AppSignedIn {
        token: sessions.issue_token(member_id)
    }
}
```

The client gets the token once, in the response, and sends it as
`Authorization: Bearer`. Jadpo keeps only its verifier, and lists a
member's tokens with `sessions.tokens(member_id)`, without the tokens
themselves.

## Related

- [API tokens](../language/08-identity.md#api-tokens)
- [Service credentials](../language/08-identity.md#service-credentials)
