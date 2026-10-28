# `route.authentication_invalid`

A route says only when it differs from needing a signed-in caller, and an
unclear value is refused without a guess, since one fix keeps the route
protected and the other opens it (area 8):

```jadpo check project=e-route-authentication expect=route.authentication_invalid
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
        authentication: none
        input: AddMemo
        status: SuccessStatus.no_content
        run: add_memo(input)
    }
}
```

```text output
error: `none` isn't a `RouteAuthentication`
 --> main.jadpo:18:25
   |
18 |         authentication: none
   |                         ^^^^ not `.none` or `.fresh`
  reason: Every route needs a signed-in caller unless it says otherwise: `RouteAuthentication.none` lets anyone call it, and `RouteAuthentication.fresh` checks the caller's record on this request. One opens the route and the other keeps it protected, so neither is guessed.
  next: Write `RouteAuthentication.fresh` to keep the route signed in, `RouteAuthentication.none` to let anyone call it, or remove `authentication:` (human)
  rule: route.authentication_invalid
  help: docs/errors/route.authentication_invalid.md

check failed: 1 error, 1 file
```
