# `semantic.auth_method_unknown`

Passwords are out of scope: checking the person is the application's job,
and a session starts once it has (`sessions.start`):

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
  help: docs/language/08-identity.html#semantic-auth-method-unknown

check failed: 1 error, 1 file
```
