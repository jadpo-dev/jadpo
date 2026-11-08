# `semantic.auth_method_unknown`

An `authentication { }` or `sign_in:` block names a way of signing in the language doesn't have.

## Why Jadpo refuses it

Each way of signing in the language has decides where a credential travels
and how it is checked, so a new one can't be declared. Passwords are left
out on purpose: checking a person is the application's job, and a session
starts once it has (`sessions.start`).

## What it looks like

main.jadpo:

```jadpo check project=e-auth-method-unknown expect=semantic.auth_method_unknown
authentication {
    password: {}
}
```

```text output
error: `password` isn't a way to sign in
 --> main.jadpo:2:5
  |
2 |     password: {}
  |     ^^^^^^^^ not a sign-in method
  reason: A caller signs in with one of the language's methods: `browser_session`, `api_token`, `external_token`, `service_key` and `oauth_token`. Each decides where its credential travels and how it is checked.
  next: Remove `password:` (agent)
  rule: semantic.auth_method_unknown
  help: https://jadpo.dev/docs/errors/semantic.auth_method_unknown

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`sign_in:` has its own list, the ways of checking a person:

```jadpo check project=e-auth-sign-in-unknown expect=semantic.auth_method_unknown
authentication {
    sign_in: {
        password: {}
    }
}
```

```text output
error: `password` isn't a way to check a person
 --> main.jadpo:3:9
  |
3 |         password: {}
  |         ^^^^^^^^ not a way to sign in
  reason: `sign_in:` lists the ways the language checks a person: `magic_link`, `passkey` and `providers`; each ends by starting a session.
  next: Remove `password:` (agent)
  rule: semantic.auth_method_unknown
  help: https://jadpo.dev/docs/errors/semantic.auth_method_unknown

check failed: 1 error, 1 file
```

The old name for `sign_in:` is refused with the edit to the new one:

```jadpo check project=e-auth-sign-in-providers expect=semantic.auth_method_unknown
authentication {
    sign_in_providers: []
}
```

```text output
error: `sign_in_providers:` is now `sign_in:`
 --> main.jadpo:2:5
  |
2 |     sign_in_providers: []
  |     ^^^^^^^^^^^^^^^^^ renamed
  reason: `sign_in:` lists every way the language checks a person: `magic_link:`, `passkey:` and `providers: [...]`, the providers being connections in `app.jadpo`.
  next: Rename `sign_in_providers:` to `sign_in:` (agent)
  rule: semantic.auth_method_unknown
  help: https://jadpo.dev/docs/errors/semantic.auth_method_unknown

check failed: 1 error, 1 file
```

## How to fix it

**If the app checks people itself**, remove the block and start a session
from your own action once it has:

```jadpo check project=e-auth-method-unknown-fix-1
authentication {}
```

No sign-in method is declared now; `sessions.start` begins a session after
your code has checked the person.

**If you meant `sign_in_providers:`**, rename it:

```jadpo check project=e-auth-method-unknown-fix-2
authentication {
    sign_in: {
        providers: []
    }
}
```

`sign_in:` now lists the ways in.

## Related

- [Ways to sign in](../language/08-identity.md#ways-to-sign-in)
- [Checking the person: `sign_in:`](../language/08-identity.md#checking-the-person-sign_in)
