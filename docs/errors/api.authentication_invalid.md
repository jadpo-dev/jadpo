# `api.authentication_invalid`

An API lists the sign-in methods it accepts, each one `authentication.jadpo`
declares (area 8):

```jadpo check project=e-api-authentication file=app.jadpo expect=api.authentication_invalid
application {
    apis: {
        main: {
            prefix: /v1
            authentication: [AuthenticationMethod.browser_session]
        }
    }
}
```

```text output
error: API `main` accepts `browser_session`, which isn't declared
 --> app.jadpo:5:30
  |
5 |             authentication: [AuthenticationMethod.browser_session]
  |                              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not declared
  reason: An API accepts only the ways to sign in the application declares; `authentication.jadpo` declares no way to sign in yet.
  next: List the methods the API accepts, each one `authentication.jadpo` declares (agent)
  rule: api.authentication_invalid
  help: docs/language/07-http.html#api-authentication-invalid

check failed: 1 error, 1 file
```
