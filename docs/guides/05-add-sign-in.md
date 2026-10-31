# Add sign-in
<!-- topic: guide-add-sign-in -->

Jadpo writes the sign-in routes for you: you say which ways in your app
accepts, which stored record is a person, and what happens the first
time someone new signs in. This guide lets people sign in to a book club
with an emailed link, or with their Google account, and gives every
route a signed-in caller.

You start from an app with one domain, `club`, and nothing stored yet.

## Say who a person is

Each person is a record. A member is found by the address they sign in
with:

`domains/club/entities/member.jadpo`:

```jadpo check project=guide-sign-in file=domains/club/entities/member.jadpo
/// Someone in the book club.
entity Member {
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

`Access.system` means only Jadpo itself may create a member, on a first
sign-in, and `read: [id]` means each member may read only their own
record.

## Connect Google and a mail server

Sign-in emails go out through a mail server, and Google is a sign-in
provider. Both are connections in `app.jadpo`, and `principal` says
which record a signed-in person is:

`app.jadpo`:

```jadpo check project=guide-sign-in file=app.jadpo
use club/entities { Member }

application {
    jadpo: "0.1"
    display_name: "The book club"
    connections: {
        google: {
            driver: Driver.oidc
            issuer: config.google_issuer
            client_id: config.google_client_id
            credential: config.google_client_secret
        }
        mail: {
            driver: Driver.smtp
            host: config.smtp_host
        }
    }
    email: {
        connection: mail
        from: config.mail_from
    }
}

principal {
    user: {
        subject: Member.email
    }
}
```

- `Driver.oidc` is for Google and any other OpenID Connect provider;
  Google's discovery document gives Jadpo the rest.
- `principal { user: ... }` says a signed-in person is a `Member`,
  found by their `email`.

The settings it reads go in `config.jadpo`:

`config.jadpo`:

```jadpo check project=guide-sign-in file=config.jadpo
config {
    /// The client ID Google gave the app.
    google_client_id: Text {
        test: "book-club"
    }
    /// The client secret Google gave the app.
    google_client_secret: Text {
        secret: true
    }
    /// Google's address as an OpenID Connect issuer.
    google_issuer: Url {
        default: "https://accounts.google.com"
    }
    /// Who the app's emails come from.
    mail_from: Email {
        default: "club@example.com"
    }
    /// The address of the site people sign in on.
    site: Url {
        default: "http://localhost:3000"
    }
    /// The mail server the app hands its emails to.
    smtp_host: Hostname {
        default: "localhost"
    }
}
```

`google_client_secret` is a secret: only the connection can read it,
and it never appears in a log or an error. It has no default, so a
deployed app won't start without it. Give it one on your machine with
`jadpo config set google_client_secret`, which asks for the value at a
prompt. `jadpo dev` doesn't need any of the three Google settings,
since it never reaches Google (below).

In Google's developer console, create an OAuth client for a web
application and register `<your site>/auth/google/callback` as its
redirect address, then use the client ID and secret it gives you.

## Turn sign-in on

`authentication.jadpo`, beside `app.jadpo`:

```jadpo check project=guide-sign-in file=authentication.jadpo
use club

authentication {
    browser_session: {
        cookie: "club_session"
        origin: config.site
    }
    sign_in: {
        magic_link: {
            link: "/sign-in"
        }
        providers: [google]
    }
    on_first_sign_in: club.welcome_member
}
```

- `browser_session:` keeps a signed-in person in a cookie. `origin:` is
  the address your site is served from; a write is accepted only from
  there.
- `magic_link:` emails a link and a code. `link:` is the page of your
  site the link opens, which sends the link's token on.
- `providers: [google]` names the connection to sign in through.
- `on_first_sign_in:` names the action that decides what happens the
  first time someone signs in whom the app has no record for, whichever
  way they came in.

## Decide who may join

`domains/club/failures/address_unverified.jadpo`:

```jadpo check project=guide-sign-in file=domains/club/failures/address_unverified.jadpo
/// The person signing in has no address the app can trust.
type AddressUnverified = Failure {
    kind: FailureKind.rejected
}
```

`domains/club/failures/already_a_member.jadpo`:

```jadpo check project=guide-sign-in file=domains/club/failures/already_a_member.jadpo
/// Someone has that address already.
type AlreadyAMember = Failure {
    kind: FailureKind.conflict
}
```

`domains/club/actions/welcome_member.jadpo`:

```jadpo check project=guide-sign-in file=domains/club/actions/welcome_member.jadpo
use club/entities { Member }
use club/failures { AddressUnverified, AlreadyAMember }

/// Makes a member of someone signing in for the first time, by either
/// way in, once their address is known to be theirs.
action welcome_member(first: FirstSignIn) fails AddressUnverified, AlreadyAMember -> Member.id {
    if (not first.address_verified) {
        reject AddressUnverified
    }
    match first.address {
        none -> {
            reject AddressUnverified
        }
        some(address) -> {
            var member: Member = attempt Member.create {
                set: {
                    email: address
                }
                conflict: {
                    Member.email: AlreadyAMember
                }
            }
            return member.id
        }
    }
}
```

It runs as the system, and gives back the new member's `id`. Any
failure refuses the person, who sees only `sign_in_refused`. A magic
link proves the address is theirs, so `address_verified` is `true`;
from Google it is `true` only when Google has verified the address.
Jadpo keeps the link between a Google account and its member, so a
later Google sign-in finds them without running this again. Here
anyone may join; a club that takes on members itself would refuse
addresses it doesn't know.

## Use the person signed in

Every route needs a signed-in caller unless it says otherwise, so a
route you write is protected from the start. `current_principal.user`
is the member making the request:

`domains/club/types/member_view.jadpo`:

```jadpo check project=guide-sign-in file=domains/club/types/member_view.jadpo
use club/entities { Member }

/// A member, as they see themselves.
type MemberView = Output {
    pick: [Member.id, Member.email]
}
```

`domains/club/queries/member_by_id.jadpo`:

```jadpo check project=guide-sign-in file=domains/club/queries/member_by_id.jadpo
use club/entities { Member, MemberNotFound }
use club/types { MemberView }

/// One member.
query member_by_id(member_id: Member.id) fails MemberNotFound -> MemberView {
    from: Member
    where: Member.id == member_id
    missing: MemberNotFound
}
```

`domains/club/routes/me.jadpo`:

```jadpo check project=guide-sign-in file=domains/club/routes/me.jadpo
use club/types { MemberView }
use club/queries { member_by_id }

/// The signed-in member.
routes Me {
    prefix: /me

    /// Who is signed in.
    GET / {
        output: MemberView
        run: member_by_id(current_principal.user.id)
    }
}
```

Run `jadpo migrate new initial`, `jadpo api update` and `jadpo check`.
The API record now lists the routes Jadpo wrote: `POST /auth/magic-link`
and `POST /auth/magic-link/confirm`, and `POST /auth/google/start` and
`GET /auth/google/callback`.

## Try it

```sh
jadpo dev
```

```text
jadpo dev, started 10:53:43
listening on http://localhost:3000
    routes    6 routes
    settings  6 checked
    database  disposable PostgreSQL, gone when dev stops
    inbox     http://localhost:3001 · mail caught, never sent
    sign-in   http://localhost:3002 · test accounts in the provider's place
serving: saves reload, press ctrl-c to quit
```

`jadpo dev` catches every email in its inbox, and stands in for Google
with test accounts of its own. Ask for a sign-in email:

```sh
curl -i -X POST http://localhost:3000/auth/magic-link \
  -H 'content-type: application/json' \
  -d '{"address": "ann@example.com"}'
```

```text
HTTP/1.1 202 Accepted
cache-control: no-store
x-request-id: req_7861b51220294fb6
content-length: 0
date: Sat, 31 Oct 2026 10:53:32 GMT
```

The answer is the same whether or not the address has a member, so
nobody can use it to find out who has joined. Read the email in the
inbox at `http://localhost:3001`, or in a terminal:

```sh
jadpo email show mail_1
```

```text
mail_1  Sign in to The book club
    email  jadpo.MagicLinkEmail
    from   club@example.com
    to     ann@example.com
    at     2026-10-31T10:53:32.436Z
    trace  9e08c48a2b3cbfbbce4997a6921d0608
    parts  html 1.2 KB · text 295 B

text
  Sign in

  Someone asked to sign in as ann@example.com.

  Sign in: http://localhost:3000/sign-in?token=d544502cc80a32e802c7bb37d74f772d093c1ec43fc91a3cd7c21dacce647a8f

  Or enter this code: 774111

  The link and the code work for 15 minutes.

  If you didn't ask to sign in, you can ignore this email.

```

Your site's `/sign-in` page sends the token on. Here, send it yourself,
**using the token from your own email**, and keep the cookies it sets in
a file outside your project:

```sh
curl -i -c ~/club-cookies.txt -X POST http://localhost:3000/auth/magic-link/confirm \
  -H 'content-type: application/json' \
  -H 'origin: http://localhost:3000' \
  -d '{"token": "d544502cc80a32e802c7bb37d74f772d093c1ec43fc91a3cd7c21dacce647a8f"}'
```

```text
HTTP/1.1 200 OK
content-type: application/json
set-cookie: club_session=3a23e044e74acbe02edacf5a7878ca4757c725ed276c698f5d6dab8dca98ecdd; Path=/; Max-Age=2592000; HttpOnly; Secure; SameSite=Strict
set-cookie: club_session_csrf=0ac0390662cb7c383443378e53a8348361ad3e6e18935902b3ff64804717d4c2; Path=/; Max-Age=2592000; Secure; SameSite=Strict
cache-control: no-store
x-request-id: req_0ab613ffd12d535a
content-length: 17
date: Sat, 31 Oct 2026 10:53:33 GMT

{"return_to":"/"}
```

Ann's first sign-in made her a member, and her session lasts 30 days:

```sh
curl -b ~/club-cookies.txt http://localhost:3000/me
```

```text
{"id":"01a19197-e5ad-7edf-ba42-a66ef56fc50d","email":"ann@example.com"}
```

Every write with the session cookie also needs the `origin` header and
an `x-jadpo-csrf` header holding the `club_session_csrf` cookie's value,
so another site can't make the browser send one. Your front end sends
both on every write.

To sign in with Google, your page posts to `/auth/google/start` and
sends the browser to the `address` it answers. In `jadpo dev` that is a
page on `http://localhost:3002` listing test accounts; choosing one, or
adding one, signs in as it, and the browser comes back to
`/auth/google/callback` as it would from Google.

## Test it

A route test asks for a link and reads the email it sent, and
`test_providers.sign_in` signs in through the provider's routes, with a
local stand-in for Google:

`tests/sign_in.test.jadpo`:

```jadpo check project=guide-sign-in file=tests/sign_in.test.jadpo
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

test "a verified Google account signs Bo in" {
    var signed_in: Response = test_providers.sign_in(
        SignInMethod.google,
        TestAccount {
            subject: "110248495921238986420"
            email: "bo@example.com"
            email_verified: true
            name: "Bo Lind"
        }
    )
    assert signed_in.status == 303
    assert signed_in.code == none
}

test "a Google account without a verified address is refused" {
    var refused: Response = test_providers.sign_in(
        SignInMethod.google,
        TestAccount {
            subject: "2001"
            email: "cy@example.com"
            email_verified: false
            name: none
        }
    )
    assert refused.code == "sign_in_refused"
}

test "who is signed in needs someone signed in" {
    var response: Response = request {
        method: HttpMethod.get
        path: "/me"
    }
    assert response.status == 401
}
```

```sh
jadpo test
```

```text
tested: 4 passed, 4 route tests
```

Other tests sign in with `as: Principal.user(member)` in a `request`,
without going through the routes; [Write tests](08-write-tests.md) shows
how.

## Keep staff to the company's accounts

When some people, such as staff, should keep their powers only while
their company's provider vouches for them, two lines do it:

- On the provider's connection, `organisation: config.google_domain`
  reads the company's Google Workspace domain, or its Microsoft tenant
  ID, from a `Text` setting. An account outside it can't sign in through
  the provider.
- On the membership that gives them their role, `requires:` gives each
  role a line: `OrganiserRole.host: { sign_in: [SignInMethod.google] }`
  lets the role count only for someone signed in through Google, and
  `strength: AuthenticationStrength.multi_factor` asks for a second
  factor too; `OrganiserRole.reader: {}` needs nothing. Signed in by
  link, an organiser is a member like any other, so switching their
  Google account off ends their powers, and a Google session lasts a
  day.

A route test then says how its user signed in, `sign_in:
SignInMethod.google` beside `as:`. The lending desk example
(`examples/29-lending-desk`) does all three. See
[Roles that need a way in](../language/09-policy.md#roles-that-need-a-way-in)
and [Company accounts](../language/08-identity.md#company-accounts).

## Read more

[The principal](../language/08-identity.md#the-principal),
[Ways to sign in](../language/08-identity.md#ways-to-sign-in),
[Checking the person](../language/08-identity.md#checking-the-person-sign_in),
[Magic links](../language/08-identity.md#magic-links),
[Passkeys](../language/08-identity.md#passkeys),
[A person the app has not seen](../language/08-identity.md#a-person-the-app-has-not-seen),
[Routes: signed in by default](../language/08-identity.md#routes-signed-in-by-default) and
[Who is calling](../language/08-identity.md#who-is-calling-current_principal).
To let each person see only their own records, see
[A user field on the record](../language/09-policy.md#a-user-field-on-the-record).

## Not built yet

Signing in with GitHub or Apple isn't built yet, since neither is plain
OpenID Connect. Use magic links, passkeys, or Google and other OpenID
Connect providers.
