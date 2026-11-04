# Send an email
<!-- topic: guide-send-an-email -->

An email in Jadpo is a type with its fields, and your code emits one the
way it emits an event. It goes after the action commits, so a change that
is rolled back never sends anything. The compiler checks each template
against the type, and tests and `jadpo dev` catch the mail so nothing
leaves your machine. This guide sends a new member of a book club a
welcome email over SMTP.

You start from an app with a `Member` entity and a `POST /members` route
that runs an action called `join`.
[Store and change a record](02-store-and-change-a-record.md) and
[Build an API](01-build-an-api.md) show how to write those.

```jadpo check project=guide-email file=domains/club/entities/member.jadpo hidden
/// A member of the book club.
entity Member {
    id: Uuid
    address: Email
    name: Text {
        min_length: 1
        max_length: 80
    }

    policy: {
        create: [Access.public]
    }
}
```

```jadpo check project=guide-email file=domains/club/types/new_member.jadpo hidden
use club/entities { Member }

/// What someone sends to join.
type NewMember = Input {
    address: Member.address
    name: Member.name
}
```

```jadpo check project=guide-email file=domains/club/types/joined.jadpo hidden
use club/entities { Member }

/// What someone gets back for joining.
type Joined = Output {
    pick: [Member.id, Member.name]
}
```

```jadpo check project=guide-email file=domains/club/routes/members.jadpo hidden
use club/types { Joined, NewMember }
use club/actions { join }

/// Joining the club.
routes Members {
    prefix: /members

    /// Joins the club.
    POST / {
        authentication: RouteAuthentication.none
        input: NewMember
        output: Joined
        status: SuccessStatus.created
        run: join(input)
    }
}
```

## Say where mail goes

Mail leaves through a connection, and `email:` names the one every email
uses. Here that is the built-in SMTP driver, which talks to any mail
server you name. A language is needed too, since each email is worded in
one.

`app.jadpo`:

```jadpo check project=guide-email file=app.jadpo
application {
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
        from: config.mail_sender
    }
}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

`config.jadpo`:

```jadpo check project=guide-email file=config.jadpo
config {
    club_url: Url {
        default: "https://club.example"
    }
    mail_sender: Email {
        default: "club@club.example"
    }
    smtp_host: Hostname {
        default: "mail.club.example"
    }
    smtp_password: Text {
        secret: true
    }
    smtp_username: Text {
        default: "club"
    }
}
```

## Declare the email

An email is a type built on `EmailMessage`, in its domain's `emails/`
folder. Its fields are the values its text uses. Every `EmailMessage`
also has `to:` and `locale:`, which you don't declare.

`domains/club/emails/welcome_email.jadpo`:

```jadpo check project=guide-email file=domains/club/emails/welcome_email.jadpo
/// Sent once someone has joined.
type WelcomeEmail = EmailMessage {
    name: Text
    club_link: Url
}
```

## Word it

An email's words live in its domain's language file, so a second language
needs no change to the templates. The entry with the email's own name is
its subject line, and its fields are the placeholders.

Each other piece of wording is a message, a type built on `Message` whose
fields are its placeholders.

`domains/club/translations/welcome_heading.jadpo`:

```jadpo check project=guide-email file=domains/club/translations/welcome_heading.jadpo
/// The heading of the welcome email.
type WelcomeHeading = Message {
    name: Text
}
```

`domains/club/translations/next_meeting.jadpo`:

```jadpo check project=guide-email file=domains/club/translations/next_meeting.jadpo
/// The welcome email's link to the club's page.
type NextMeeting = Message {}
```

`domains/club/translations/en_gb.jadpo`:

```jadpo check project=guide-email file=domains/club/translations/en_gb.jadpo
translations {
    next_meeting: "See the next meeting"
    welcome_email: "Welcome to the club, {name}"
    welcome_heading: "Welcome, {name}"
}
```

## Write the templates

The templates sit beside the type and take its name. Write both an HTML
one and a text one, because a mail with no plain-text part is one of the
things `jadpo check` warns about. Inside `{{ }}` a template can use the
email's fields, translations, `if` and `for`, and nothing else, and every
value is escaped.

`domains/club/emails/welcome_email.html`:

```jadpo check project=guide-email file=domains/club/emails/welcome_email.html
<h1>{{ translations.club.welcome_heading(name: name) }}</h1>
<p><a href="{{ club_link }}">{{ translations.club.next_meeting() }}</a></p>
```

`domains/club/emails/welcome_email.txt`:

```jadpo check project=guide-email file=domains/club/emails/welcome_email.txt
{{ translations.club.welcome_heading(name: name) }}

{{ translations.club.next_meeting() }}: {{ club_link }}
```

A template that names a field or a message that doesn't exist is a compile
error. So is a field no template or subject uses.

## Send it

`emit` sends an email. It joins the action's transaction, so the email goes
only once the action has committed, and a failed send is retried on its
own.

`domains/club/actions/join.jadpo`:

```jadpo check project=guide-email file=domains/club/actions/join.jadpo
use club/entities { Member }
use club/types { Joined, NewMember }
use club/emails { WelcomeEmail }

/// Adds a member, and welcomes them once that is done.
action join(input: NewMember) -> Joined {
    var member: Member = Member.create {
        set: {
            address: input.address
            name: input.name
        }
    }
    emit WelcomeEmail {
        to: member.address
        locale: Locale.en_gb
        name: member.name
        club_link: config.club_url
    }
    return Joined {
        id: member.id
        name: member.name
    }
}
```

## Test it

Tests never send mail. They catch each email, rendered whole once its
action commits, so a template that can't render fails the test. A test
reads what has gone so far as `email.sent`.

`domains/club/actions/join.test.jadpo`:

```jadpo check project=guide-email file=domains/club/actions/join.test.jadpo
use club/types { Joined, NewMember }

test "joining welcomes the new member" {
    var joined: Joined = join(
        NewMember {
            address: "ann@example.com"
            name: "Ann"
        }
    )
    assert joined.name == "Ann"
    var sent: List<SentEmail> = email.sent
    assert list.count(sent) == 1
    assert list.any(
        sent,
        item -> item.to == "ann@example.com" and item.subject == "Welcome to the club, Ann"
    )
    assert list.all(sent, item -> list.contains(item.links, "https://club.example"))
}
```

```sh
jadpo test
```

```text
tested: 1 passed, 1 action test
```

## Accept where the address goes

An email address is personal data, and this app now hands one to a mail
server it couldn't reach before. That is two things a person has to decide,
so once your app has a commit to compare with, `jadpo check` stops. First
write the record of the hosts your app can reach:

```sh
jadpo records update
jadpo check
```

```text
error: a connection that can't erase the personal data it receives needs a person: mail
 --> app.jadpo:3:9
  |
3 |         mail: {
  |         ^^^^ ErasureNotPossible.data_stays
  reason: Erasing a person asks each provider that holds their data to erase it, and this one can't be asked. Here: connection `mail` keeps an `Email` after a person is erased: it is reached with a driver, so it has no package to declare `erasure:`. So a person says why the data stays: no acceptance lists `mail: ErasureNotPossible.data_stays`.
  next: Declare the package's `erasure:` and name the connection's `erasure: { key: }`, or a person accepts it: `jadpo acceptance new <purpose>` writes the draft for them (human)
  rule: acceptance.missing
  help: https://jadpo.dev/docs/errors/acceptance.missing

error: a new outside host the app can send data to needs a person: mail
 --> network.jadpo_record:4:1
  |
4 | connection mail
  | ^^^^^^^^^^^^^^^ NetworkChange.host_added
  reason: Data the app holds can now leave for a host it couldn't reach before. So a person accepts it: no acceptance this change adds lists `mail: NetworkChange.host_added`.
  next: A person accepts it: `jadpo acceptance new <purpose>` writes the draft for them (human)
  rule: acceptance.missing
  help: https://jadpo.dev/docs/errors/acceptance.missing

check failed: 2 errors, 12 files
```

`jadpo acceptance new` writes the file listing both, and a person writes
the reason on the `///` line above it. An agent stops there and hands over,
because the reason has to come from you.
[Get a person's sign-off](09-get-a-sign-off.md) goes through it step by
step.

```sh
jadpo acceptance new addresses_go_to_the_mail_server
```

```text
Wrote acceptances/20261104T1111_addresses_go_to_the_mail_server.jadpo
    mail  ErasureNotPossible.data_stays
    mail  NetworkChange.host_added

decision: 2 changes wait for a person's reason
 --> acceptances/20261104T1111_addresses_go_to_the_mail_server.jadpo:1
  next: Stop and hand over: a person reads the changes and writes why they are accepted on a `///` line above `acceptance`. (human)
```

The time in the file's name is yours. With the reason added,
`acceptances/20261104T1111_addresses_go_to_the_mail_server.jadpo`:

```jadpo check project=guide-email file=acceptances/20261104T1111_addresses_go_to_the_mail_server.jadpo
/// Our mail server keeps sent mail for 30 days and then deletes it, as our privacy notice says.
acceptance {
    changes: {
        mail: ErasureNotPossible.data_stays
        mail: NetworkChange.host_added
    }
}
```

`jadpo check` passes, and you commit the acceptance with the change. A
project with no commit yet gets a note in place of these two errors, saying
the connection is refused from the next change.

## Read it in development

`jadpo dev` catches every email in a local inbox, so nothing leaves your
machine and the SMTP settings don't have to reach a real server.

```sh
jadpo dev
```

```text
jadpo dev, started 11:11:44
listening on http://localhost:3000
    routes    2 routes
    settings  5 checked
    database  disposable PostgreSQL, gone when dev stops
    inbox     http://localhost:3001 · mail caught, never sent
serving: saves reload, press ctrl-c to quit
```

In another terminal, join the club:

```sh
curl -i -X POST http://localhost:3000/members \
  -H 'content-type: application/json' \
  -d '{"address": "ann@example.com", "name": "Ann"}'
```

```text
HTTP/1.1 201 Created
content-type: application/json
x-request-id: req_c5fe488d0b27084a
content-length: 58
date: Wed, 04 Nov 2026 11:11:46 GMT

{"id":"01a1a642-0154-75c8-92e7-42dc081a74a4","name":"Ann"}
```

Open the inbox at `http://localhost:3001` to see each mail as HTML and as
text, or read it from the terminal, as a coding agent does:

```sh
jadpo email list
```

```text
  mail    email         to               subject                   health
  mail_1  WelcomeEmail  ann@example.com  Welcome to the club, Ann  + clean

  health: An early indication only: it can't predict whether Gmail, Outlook or any other provider puts a mail in the inbox, since each uses its own filters and the sender's history. A clean score means the common mistakes are absent, nothing more.

email: 1 mail

next: jadpo email show mail_1  shows its words, links and health
```

```sh
jadpo email show mail_1
```

This is the start of what it prints. After it come the health checks' results
and, when SpamAssassin is installed, its score.

```text
mail_1  Welcome to the club, Ann
    email  club.WelcomeEmail
    from   club@club.example
    to     ann@example.com
    at     2026-11-04T11:11:46.011Z
    trace  fe35a94130f0acccd146bb13e6a4d8ca
    parts  html 85 B · text 57 B

text
  Welcome, Ann

  See the next meeting: https://club.example

links
   1  https://club.example

Health: all 7 of Jadpo's checks pass
```

## Give every email the same look

Every email is wrapped in your app's layout when it has one, in
`shared/emails/layout.html`. `{{ content }}` is the email's own HTML, and
Jadpo's own emails, such as the sign-in email, get the same wrapper.

`shared/emails/layout.html`:

```jadpo check project=guide-email file=shared/emails/layout.html
<html dir="{{ locale.direction }}">
<head><title>{{ subject }}</title></head>
<body>
  <header>The book club</header>
  {{ content }}
</body>
</html>
```

## Send through a provider

`app.jadpo` is the only file that knows how mail leaves. To send through
Postmark, Amazon SES or Resend, replace the `mail` connection with one that
names a mail package, and the email's type, templates and `emit` stay as
they are. A package that declares an `erasure:` call, on a connection that
names the person's key, needs no `ErasureNotPossible.data_stays` acceptance,
because erasing a person then asks the provider to erase them too.

A `File` field on an email is an attachment, read from its store when the
email is sent.

## Read more

[Email](../language/16-built-in-services.md#email),
[The SMTP driver](../language/16-built-in-services.md#the-smtp-driver),
[Built-in emails](../language/16-built-in-services.md#built-in-emails),
[The dev inbox and the health score](../language/16-built-in-services.md#the-dev-inbox-and-the-health-score),
[Email subjects](../language/10-configuration-and-time.md#email-subjects) and
[Erasure at providers](../language/15-personal-data.md#erasure-at-providers).

## Not built yet

- The inbox's link from a mail to its trace, and `jadpo trace`, aren't built
  yet. A mail's `trace` line is an id you can look for in your logs.
- The privacy emails Jadpo sends with data requests aren't built yet.
