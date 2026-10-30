# `semantic.auth_not_set_up`

Every route needs a signed-in caller unless it says otherwise, so routes
with nothing to sign anyone in could never be called:

```jadpo check project=e-auth-not-set-up expect=semantic.auth_not_set_up
type Memo = Text {
    max_length: 200
}

type AddMemo = Input {
    text: Memo
}

action add_memo(input: AddMemo) -> Unit {
    var memo: Memo = input.text
    assert memo != ""
}

routes Memos {
    prefix: /memos

    POST / {
        input: AddMemo
        status: SuccessStatus.no_content
        run: add_memo(input)
    }
}
```

```text output
error: `POST /memos` needs a signed-in caller, but nothing signs anyone in
 --> main.jadpo:17:5
   |
17 |     POST / {
   |     ^^^^^^ needs a signed-in caller
  reason: Every route needs a signed-in caller unless it says `authentication: RouteAuthentication.none`, and `authentication.jadpo` declares no way to sign in that is built, so no request could reach this route.
  next: Declare how callers sign in, `authentication { browser_session: { cookie: "<name>" } }`, or let anyone call the route with `authentication: RouteAuthentication.none` (human)
  rule: semantic.auth_not_set_up
  help: https://jadpo.dev/docs/errors/semantic.auth_not_set_up

check failed: 1 error, 1 file
```
