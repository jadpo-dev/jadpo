# Signing in by email
<!-- topic: learn-sign-in -->

Page 10 gave the app users, but no way to sign in. On this page people
sign in by email: they type their address and get an email holding a
link and a six-digit code, and either one signs them in. There are no
passwords to store, reset or leak. The first time someone signs in, the
app makes their user record.

The order is: the settings for sending email, the mail server, signing
in, the first sign-in, and trying it.

## The settings

Sending email needs a mail server's address, a user name and password
for it, and an address to send from. Sign-in links need your site's
address. Those are settings, so add them to `config.jadpo` first:

`config.jadpo`, changed. Every setting but `max_due_ahead` is new.

```jadpo check project=learn-todo-11 file=config.jadpo
config {
    /// Who the app's emails come from.
    mail_from: Email {
        default: "todos@example.com"
    }
    /// How far ahead a todo may be due.
    max_due_ahead: Duration {
        default: 365d
        test: 30d
    }
    /// The address of the site people sign in on.
    site: Url {
        default: "http://localhost:3000"
    }
    /// The mail server the app hands its emails to.
    smtp_host: Hostname {
        default: "localhost"
    }
    /// The password for the mail server.
    smtp_password: Text {
        secret: true
    }
    /// The user name for the mail server.
    smtp_username: Text {
        default: "todos"
    }
}
```

`Email`, `Url` and `Hostname` are standard types, so a value that isn't
an email address, a web address or a host name is refused before the
app starts. `smtp_password` is a secret, as page 9 described, with no
default.

`jadpo check` passes now: it checks your code, not your settings'
values. `jadpo config check` shows `smtp_password` as `missing`, which
matters only when you deploy, on page 13. On your machine, `jadpo dev`
and `jadpo test` never send email: they catch it so you can read it,
and give the mail server's settings stand-in values.

## The mail server

`app.jadpo` names the mail server and says that every email goes
through it:

`app.jadpo`, changed. `display_name:`, `connections:` and `email:` are new.

```jadpo check project=learn-todo-11 file=app.jadpo
use todos/entities { User }

application {
    jadpo: "0.1"
    display_name: "Todos"
    connections: {
        mail: {
            driver: Driver.smtp
            host: config.smtp_host
            credential: {
                username: config.smtp_username
                password: config.smtp_password
            }
        }
    }
    email: {
        connection: mail
        from: config.mail_from
    }
}

principal {
    user: {
        subject: User.email
    }
}
```

- `connections:` lists the other services the app talks to, each with a
  name. `mail` is an SMTP server, `Driver.smtp`, at the address in
  `smtp_host`, signed in to with `smtp_username` and `smtp_password`.
  This is where the secret goes: only the connection can read it.
- `email:` says every email goes through `mail`, from the address in
  `mail_from`.
- `display_name:` is the app's name, which the sign-in email uses.

Check again: it passes.

## Signing in

Now turn on sign-in by email in `authentication.jadpo`:

`authentication.jadpo`, changed. `origin:`, `sign_in:`, `on_first_sign_in:` and the `use` line are new.

```jadpo check project=learn-todo-11 file=authentication.jadpo
use todos

authentication {
    browser_session: {
        cookie: "todo_session"
        origin: config.site
    }
    sign_in: {
        magic_link: {
            link: "/sign-in"
        }
    }
    on_first_sign_in: todos.welcome_user
}
```

- `origin:` is your site's address, which the emailed link starts with.
- `magic_link:` turns on signing in by email. `link:` is the page of
  your site that the emailed link opens. Your front end owns that page:
  it reads the token from the link and sends it to the app. Jadpo writes
  routes, never pages.
- `on_first_sign_in:` names the action that runs the first time someone
  signs in with an address no user has. Leave it out, and only people
  who already have a user record can sign in.

You write no other sign-in code. Jadpo adds two routes:
`POST /auth/magic-link` sends the email, and
`POST /auth/magic-link/confirm` takes the link's token, or the address
and code, and starts the session.

Jadpo also protects them for you. One address can only be sent so many
emails an hour. Each link and code works once, for 15 minutes, and only
the newest works. Jadpo keeps only a hash of each, never the link or
the code itself.

## The first sign-in

This action makes the user:

`domains/todos/actions/welcome_user.jadpo`:

```jadpo check project=learn-todo-11 file=domains/todos/actions/welcome_user.jadpo
use todos/entities { User }
use todos/failures { AlreadyAUser }

/// Makes a user of someone signing in for the first time.
action welcome_user(first: FirstSignIn) fails AlreadyAUser -> User.id {
    match first.address {
        none -> {
            reject AlreadyAUser
        }
        some(address) -> {
            var user: User = attempt User.create {
                set: {
                    email: address
                }
                conflict: {
                    User.email: AlreadyAUser
                }
            }
            return user.id
        }
    }
}
```

- `FirstSignIn` is a built-in type: what Jadpo knows about the person.
  `first.address` is their email address, which they have just proved is
  theirs by using the link or code. It's optional because other ways of
  signing in might not give one; an emailed link always does.
- The result, a `User.id`, is the user to start the session for.
- Jadpo runs this action as the system, not as the person, since they
  aren't signed in yet. So `User` must let the system create users:

`domains/todos/entities/user.jadpo`, changed. The `create:` line is new.

```jadpo check project=learn-todo-11 file=domains/todos/entities/user.jadpo
/// Someone who keeps a todo list.
entity User {
    id: Uuid
    email: Email {
        unique: true
    }

    policy: {
        create: [Access.system]
        read: [id]
    }
}
```

`Access.system` is the app itself, acting with no person behind the
request. Then make the migration for Jadpo's sign-in table, and update
the record for the two new routes:

```sh
jadpo migrate new add_sign_in
jadpo api update
```

```text
  compared the entities with 20261029T1200_initial

  System tables
    + table  jadpo_sign_in_requests  system table for sign in requests

Wrote database/migrations/20261029T1300_add_sign_in/
    snapshot.json  the whole schema at this point
    postgres.sql   5 statements: create table, add unique rule, create index

next: jadpo check  confirms the migration matches the entities
Wrote api/main.jadpo_record
    version  dfd34f2ee89a
    added    POST /auth/magic-link
    added    POST /auth/magic-link/confirm
    added    output MagicLinkConfirmed

next: git diff api/  shows what changed for the API's clients
```

`jadpo check` and `jadpo test` pass, with the same tests as page 10.

## Try it

Run `jadpo dev`. It now also starts an inbox, which catches every email:

```text
jadpo dev, started 21:53:10
listening on http://localhost:3000
    routes    8 routes
    settings  6 checked
    database  disposable PostgreSQL, gone when dev stops
    inbox     http://localhost:3001 · mail caught, never sent
serving: saves reload, press ctrl-c to quit
```

Ask for a sign-in email:

```sh
curl -i -X POST http://localhost:3000/auth/magic-link \
  -H 'content-type: application/json' \
  -d '{"address": "ann@example.com"}'
```

```text
HTTP/1.1 202 Accepted
cache-control: no-store
x-request-id: req_21a42a330688bc23
content-length: 0
date: Thu, 29 Oct 2026 21:53:11 GMT
```

The answer is the same whether or not the address has a user, so no one
can use it to find out who has an account. Open the inbox at
`http://localhost:3001` in a browser to read the email, or read it in a
terminal. Each caught email has an id, `mail_1` for the first:

```sh
jadpo email show mail_1
```

```text
mail_1  Sign in to Todos
    email  jadpo.MagicLinkEmail
    from   todos@example.com
    to     ann@example.com
    at     2026-10-29T21:53:11.308Z
    trace  b154a2ac36fc660aae657e0b56048712
    parts  html 1.2 KB · text 295 B

text
  Sign in

  Someone asked to sign in as ann@example.com.

  Sign in: http://localhost:3000/sign-in?token=997fde0b81ff7602c81af14bcad8039e7e219d4aad50ab5d4023e87fe7a9c9a8

  Or enter this code: 293542

  The link and the code work for 15 minutes.

  If you didn't ask to sign in, you can ignore this email.
```

It goes on to list the email's links and a check of how well it's
written. Your front end's `/sign-in` page would send the token on. Here,
send it yourself. **Use the token from your own email**, and save the
cookies the answer sets in a file outside your project:

```sh
curl -i -c ~/todo-cookies.txt -X POST http://localhost:3000/auth/magic-link/confirm \
  -H 'content-type: application/json' \
  -H 'origin: http://localhost:3000' \
  -d '{"token": "997fde0b81ff7602c81af14bcad8039e7e219d4aad50ab5d4023e87fe7a9c9a8"}'
```

```text
HTTP/1.1 200 OK
content-type: application/json
set-cookie: todo_session=dabdb0306d0d5f705826dbc42240cca81500958e44447c19ca08784fc73cdaa9; Path=/; Max-Age=2592000; HttpOnly; Secure; SameSite=Strict
set-cookie: todo_session_csrf=cdb8cf8e68ac1ed368234967347a4e2d25fb1aff2f94e853e42fe812125d4ae7; Path=/; Max-Age=2592000; Secure; SameSite=Strict
cache-control: no-store
x-request-id: req_3dd5406c405345ad
content-length: 17
date: Thu, 29 Oct 2026 21:53:12 GMT

{"return_to":"/"}
```

The `origin` header says which site sent the request, as a browser
does; the confirm route refuses one from any other site. Ann is signed
in, the first sign-in made her user record, and her session lasts 30
days.

Every write with a session cookie needs two headers, so that another
site can't make Ann's browser send one: `origin`, and `x-jadpo-csrf`,
holding the value of the `todo_session_csrf` cookie. Your front end
reads that cookie and sends it on every write. Here, copy **your own**
value from the answer above:

```sh
curl -i -b ~/todo-cookies.txt -X POST http://localhost:3000/todos \
  -H 'content-type: application/json' \
  -H 'origin: http://localhost:3000' \
  -H 'x-jadpo-csrf: cdb8cf8e68ac1ed368234967347a4e2d25fb1aff2f94e853e42fe812125d4ae7' \
  -d '{"title": "Buy milk"}'
```

```text
HTTP/1.1 201 Created
content-type: application/json
x-request-id: req_84f55097f7af8ffc
content-length: 134
date: Thu, 29 Oct 2026 21:53:12 GMT

{"id":"01a189a7-1ae9-73d5-a2c3-a303f378d311","title":"Buy milk","status":"open","due_at":null,"created_at":"2026-10-29T21:53:12.425Z"}
```

Leave out `x-jadpo-csrf`, and the write is refused:

```text
{"error":{"code":"csrf_rejected","message":"The request's origin or CSRF token is missing or wrong.","request_id":"req_7b204bee1859694d"}}
```

A read needs only the session cookie. The cookie is marked `Secure`,
for HTTPS only; recent versions of curl still send it to `localhost`.
If yours doesn't and you get `authentication_required`, pass the cookie
yourself with `-H 'cookie: todo_session=<your session value>'`:

```sh
curl -b ~/todo-cookies.txt http://localhost:3000/todos
```

```text
[{"id":"01a189a7-1ae9-73d5-a2c3-a303f378d311","title":"Buy milk","status":"open","due_at":null,"created_at":"2026-10-29T21:53:12.425Z"}]
```

## Signing in with the code

The code is for when the link would open on a different device: the
person reads the code from their email and types it into your sign-in
page, which sends it with their address to the same confirm route.
Ask for an email for Bob, read his code with `jadpo email show mail_2`,
and send it:

```sh
curl -i -X POST http://localhost:3000/auth/magic-link/confirm \
  -H 'content-type: application/json' \
  -H 'origin: http://localhost:3000' \
  -d '{"address": "bob@example.com", "code": "571848"}'
```

The answer is the same as for the link, with Bob's cookies. A wrong
code gets the same answer every time, whatever was wrong, so it gives
nothing away:

```text
{"error":{"code":"sign_in_code_invalid","message":"The sign-in code isn't right.","request_id":"req_7220617b396b5a73"}}
```

## Test it

A route test signs someone in the same way: it asks for an email, reads
it, and sends the link's token back. A test that crosses the app like
this goes in the project's `tests/` folder:

`tests/sign_in.test.jadpo`:

```jadpo check project=learn-todo-11 file=tests/sign_in.test.jadpo
test "an emailed link signs Ann in" {
    var asked: Response = request {
        method: HttpMethod.post
        path: "/auth/magic-link"
        input: MagicLinkAsk {
            address: "ann@example.com"
        }
    }
    assert asked.status == 202
    match list.first(email.sent) {
        none -> {
            assert list.count(email.sent) == 1
        }
        some(sent) -> {
            var confirmed: Response = request {
                method: HttpMethod.post
                path: "/auth/magic-link/confirm"
                input: MagicLinkConfirmation {
                    token: sent.sign_in_token
                }
            }
            assert confirmed.status == 200
        }
    }
}
```

- `MagicLinkAsk` and `MagicLinkConfirmation` are what the two routes
  read, as the API record names them. Only a test builds them.
- `email.sent` lists the emails the test has sent. `list.first` gives
  the first, or `none` if there isn't one, which the `match` handles.
- `sent.sign_in_token` is the link's token, and `sent.sign_in_code` the
  code, so a test doesn't read them out of the email's text.
- Every request a test sends carries your site's `origin`, as a
  browser's does, so the confirm route takes it.

`jadpo test` runs it with the tests from page 10.

## What you have

Sign-in by email, with a link or a code, a session cookie, and users
made on their first sign-in. But any signed-in person can still read
and change anyone's todos. Page 12 fixes that.

Read more in the reference: [Magic links](../language/08-identity.md#magic-links),
[A person the app has not seen](../language/08-identity.md#a-person-the-app-has-not-seen)
and [The dev inbox](../language/16-built-in-services.md#the-dev-inbox-and-the-health-score).

## Not built yet

- Signing in with Google, GitHub or another provider isn't built yet,
  so `sign_in:` takes magic links and passkeys only. Passkeys, signing in
  with a fingerprint or face, are one line, `passkey: {}`, beside
  `magic_link:`; the reference covers them.
