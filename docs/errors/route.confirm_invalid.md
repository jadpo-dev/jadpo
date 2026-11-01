# `route.confirm_invalid`

A route's `confirm:` holds something other than `Confirmation.passkey`, or no signed-in person could confirm the route.

## Why Jadpo refuses it

`confirm:` makes a route run only once the signed-in person presents one
of their own passkeys for that one request, so a stolen session or token
isn't enough. `Confirmation.passkey` is the only way a route asks for that,
and any other value is refused rather than read as no confirmation, since
the route you meant to protect would then run without one.

It also needs a person to confirm. A route open to anyone reads no
credential, so nobody is signed in whose passkey it could be, and a route
that passes on `current_principal.service` runs for a service, which signs
in with its key and holds no passkey.

## What it looks like

main.jadpo:

```jadpo check project=e-route-confirm expect=route.confirm_invalid
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
        confirm: passkey
        input: AddMemo
        status: SuccessStatus.no_content
        run: add_memo(input)
    }
}
```

```text output
error: `passkey` isn't a `Confirmation`
 --> main.jadpo:18:18
   |
18 |         confirm: passkey
   |                  ^^^^^^^ not `Confirmation.passkey`
  reason: `confirm:` says what the signed-in person presents before the route runs, and `Confirmation.passkey`, one of their own passkeys for this one request, is the only way there is.
  next: Write `confirm: Confirmation.passkey` (agent)
  rule: route.confirm_invalid
  help: https://jadpo.dev/docs/errors/route.confirm_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself: the value has only one spelling.

A route anyone may call has nobody signed in to confirm it:

```jadpo check project=e-route-confirm-public expect=route.confirm_invalid
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
        authentication: RouteAuthentication.none
        confirm: Confirmation.passkey
        input: AddMemo
        status: SuccessStatus.no_content
        run: add_memo(input)
    }
}
```

```text output
error: `POST /memos` is open to anyone, so no one signed in can confirm it
 --> main.jadpo:19:9
   |
19 |         confirm: Confirmation.passkey
   |         ^^^^^^^ needs a signed-in person
  reason: `confirm: Confirmation.passkey` asks the signed-in person for one of their own passkeys, and `authentication: RouteAuthentication.none` reads no credential, so there is no person whose passkey it could be.
  next: Remove `authentication: RouteAuthentication.none` so callers sign in (agent)
  rule: route.confirm_invalid
  help: https://jadpo.dev/docs/errors/route.confirm_invalid

check failed: 1 error, 1 file
```

And a route that runs for a service has no passkey to ask for:

```jadpo check project=e-route-confirm-service expect=route.confirm_invalid
entity Robot {
    id: Uuid
    name: Text {
        max_length: 40
        unique: true
    }
}

principal {
    service: {
        subject: Robot.name
    }
}

authentication {
    service_key: {}
}

action run_robot(robot: Principal.service) -> Unit {
    assert robot.subject != ""
}

routes Robots {
    prefix: /robots

    POST / {
        confirm: Confirmation.passkey
        status: SuccessStatus.no_content
        run: run_robot(current_principal.service)
    }
}
```

```text output
error: `POST /robots` runs for a service, which has no passkey
 --> main.jadpo:27:9
   |
27 |         confirm: Confirmation.passkey
   |         ^^^^^^^ asks for a passkey
29 |         run: run_robot(current_principal.service)
   |                        ----------------- a service's call
  reason: `confirm: Confirmation.passkey` asks the signed-in person for one of their own passkeys; a service signs in with its key and holds none.
  next: Pass `current_principal.user`, or remove `confirm:` from a route only services call (agent)
  rule: route.confirm_invalid
  help: https://jadpo.dev/docs/errors/route.confirm_invalid

check failed: 1 error, 1 file
```

## How to fix it

**If the value is wrong**, write `Confirmation.passkey`:

```jadpo fragment
        confirm: Confirmation.passkey
```

```jadpo check project=e-route-confirm-fixed file=domains/notes/routes/memos.jadpo hidden
use notes/types { NewMemo }
use notes/actions { add_memo }

routes Memos {
    prefix: /memos

    POST / {
        confirm: Confirmation.passkey
        input: NewMemo
        status: SuccessStatus.no_content
        run: add_memo(input, current_principal.user)
    }
}
```

```jadpo check project=e-route-confirm-fixed file=config.jadpo hidden
config {
    browser_origin: Url
    mail_sender: Email
    smtp_host: Hostname
}
```

```jadpo check project=e-route-confirm-fixed file=app.jadpo hidden
use notes/entities { Member }

application {
    display_name: "Notes"
    connections: {
        mail: {
            driver: Driver.smtp
            host: config.smtp_host
        }
    }
    email: {
        connection: mail
        from: config.mail_sender
    }
}

principal {
    user: {
        subject: Member.email
    }
}
```

```jadpo check project=e-route-confirm-fixed file=authentication.jadpo hidden
authentication {
    browser_session: {
        cookie: "notes_session"
        origin: config.browser_origin
    }
    sign_in: {
        magic_link: {
            link: "/sign-in/confirm"
        }
        passkey: {}
    }
}
```

```jadpo check project=e-route-confirm-fixed file=domains/notes/entities/member.jadpo hidden
entity Member {
    id: Uuid
    email: Email {
        unique: true
    }

    policy: {
        read: [id]
    }
}
```

```jadpo check project=e-route-confirm-fixed file=domains/notes/types/memo.jadpo hidden
type Memo = Text {
    max_length: 200
}
```

```jadpo check project=e-route-confirm-fixed file=domains/notes/types/new_memo.jadpo hidden
use notes/types { Memo }

type NewMemo = Input {
    text: Memo
}
```

```jadpo check project=e-route-confirm-fixed file=domains/notes/actions/add_memo.jadpo hidden
use notes/types { Memo, NewMemo }

/// Adds a memo for the signed-in member.
action add_memo(input: NewMemo, member: Principal.user) -> Unit {
    var memo: Memo = input.text
    assert memo != "" and member.subject != ""
}
```

```jadpo check project=e-route-confirm-fixed file=policy.jadpo hidden migrate=initial
use notes

policy {
    actions: {
        notes.add_memo: [Access.authenticated]
    }
}
```

The app must have `passkey:` in `sign_in:`, which brings magic links and an
email connection beside it. `POST /memos` now runs only once the signed-in
person presents one of their own passkeys.

**If the route is open to anyone**, remove `authentication:
RouteAuthentication.none`, so callers sign in:

```jadpo fragment
    POST / {
        confirm: Confirmation.passkey
        ...
    }
```

```jadpo check project=e-route-confirm-fixed file=domains/notes/routes/memos.jadpo hidden
use notes/types { NewMemo }
use notes/actions { add_memo }

routes Memos {
    prefix: /memos

    POST / {
        confirm: Confirmation.passkey
        input: NewMemo
        status: SuccessStatus.no_content
        run: add_memo(input, current_principal.user)
    }
}
```

Callers who aren't signed in now get a 401, and the signed-in person
confirms each request.

**If a person should call the route**, pass `current_principal.user`, with
the action taking a `Principal.user`:

```jadpo fragment
        run: add_memo(input, current_principal.user)
```

```jadpo check project=e-route-confirm-fixed file=domains/notes/routes/memos.jadpo hidden
use notes/types { NewMemo }
use notes/actions { add_memo }

routes Memos {
    prefix: /memos

    POST / {
        confirm: Confirmation.passkey
        input: NewMemo
        status: SuccessStatus.no_content
        run: add_memo(input, current_principal.user)
    }
}
```

A service's key no longer reaches the route, and the person who calls it
presents their passkey. If only services call it, remove `confirm:`.

## Related

- [Confirm one request with a passkey](../language/08-identity.md#confirm-one-request-with-a-passkey-confirm-confirmationpasskey)
- [Passkeys](../language/08-identity.md#passkeys)
- [Passkeys in tests](../language/08-identity.md#passkeys-in-tests)
