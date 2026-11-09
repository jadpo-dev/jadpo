# `route.cache_invalid`

A route's `cache:` isn't `immutable`, or it keeps an answer a public cache may not hold.

## Why Jadpo refuses it

`cache: immutable` says an answer never changes, so any cache on the way may
keep it for a year and hand it to anyone who asks. It is only safe on a
`GET` anyone may call, answering 200, with no personal data in what the
route gives, since no erasure reaches a public cache. A field marked
`personal:` counts, and so does a standard type that is personal data
whatever holds it, such as `PhoneNumber` or `Email`. `immutable` is the one
thing `cache:` says.

## What it looks like

main.jadpo:

```jadpo check project=e-route-cache-invalid expect=route.cache_invalid
type Note = Text {
    max_length: 200
}

type NoteView = Output {
    note: Note
}

action a_note() -> NoteView {
    return NoteView {
        note: "Never changes."
    }
}

routes Notes {
    prefix: /notes

    GET / {
        output: NoteView
        cache: immutable
        run: a_note()
    }
}
```

```text output
error: `GET /notes` needs a signed-in caller, but says `cache: immutable`
 --> main.jadpo:20:9
   |
20 |         cache: immutable
   |         ^^^^^ a public cache would hand it to anyone
  reason: `cache: immutable` promises the answer never changes, so any cache on the way may keep it for a year: a `GET` anyone may call, answering 200, with no personal data in it, marked `personal:` or of a standard type that is, since no erasure reaches a public cache.
  next: Remove `cache:`, or say `authentication: RouteAuthentication.none` (agent)
  rule: route.cache_invalid
  help: https://jadpo.dev/docs/errors/route.cache_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`immutable` is the only value:

```jadpo check project=e-route-cache-word expect=route.cache_invalid
type Note = Text {
    max_length: 200
}

type NoteView = Output {
    note: Note
}

action a_note() -> NoteView {
    return NoteView {
        note: "Never changes."
    }
}

policy {
    actions: {
        a_note: [Access.public]
    }
}

routes Notes {
    prefix: /notes

    GET / {
        authentication: RouteAuthentication.none
        output: NoteView
        cache: forever
        run: a_note()
    }
}
```

```text output
error: `forever` isn't something a route's `cache:` says
 --> main.jadpo:27:9
   |
27 |         cache: forever
   |         ^^^^^ not `immutable`
  reason: `cache: immutable` promises the answer never changes, so any cache on the way may keep it for a year: a `GET` anyone may call, answering 200, with no personal data in it, marked `personal:` or of a standard type that is, since no erasure reaches a public cache.
  next: Write `cache: immutable`, or leave `cache:` out (agent)
  rule: route.cache_invalid
  help: https://jadpo.dev/docs/errors/route.cache_invalid

check failed: 1 error, 1 file
```

Only a `GET`'s answer can be kept:

```jadpo check project=e-route-cache-post expect=route.cache_invalid
type Note = Text {
    max_length: 200
}

type NoteView = Output {
    note: Note
}

action a_note() -> NoteView {
    return NoteView {
        note: "Never changes."
    }
}

policy {
    actions: {
        a_note: [Access.public]
    }
}

routes Notes {
    prefix: /notes

    POST / {
        authentication: RouteAuthentication.none
        output: NoteView
        cache: immutable
        run: a_note()
    }
}
```

```text output
error: `POST /notes` keeps its answer with `cache: immutable`
 --> main.jadpo:27:9
   |
27 |         cache: immutable
   |         ^^^^^ not a `GET`
  reason: `cache: immutable` promises the answer never changes, so any cache on the way may keep it for a year: a `GET` anyone may call, answering 200, with no personal data in it, marked `personal:` or of a standard type that is, since no erasure reaches a public cache.
  next: Remove `cache:`; only a `GET`'s answer can be kept (agent)
  rule: route.cache_invalid
  help: https://jadpo.dev/docs/errors/route.cache_invalid

check failed: 1 error, 1 file
```

Only an answer of 200 can be kept:

```jadpo check project=e-route-cache-status expect=route.cache_invalid
type Note = Text {
    max_length: 200
}

type NoteView = Output {
    note: Note
}

action a_note() -> NoteView {
    return NoteView {
        note: "Never changes."
    }
}

policy {
    actions: {
        a_note: [Access.public]
    }
}

routes Notes {
    prefix: /notes

    GET / {
        authentication: RouteAuthentication.none
        output: NoteView
        status: SuccessStatus.accepted
        cache: immutable
        run: a_note()
    }
}
```

```text output
error: `GET /notes` answers `SuccessStatus.accepted`, but says `cache: immutable`
 --> main.jadpo:28:9
   |
28 |         cache: immutable
   |         ^^^^^ not an answer of 200
  reason: `cache: immutable` promises the answer never changes, so any cache on the way may keep it for a year: a `GET` anyone may call, answering 200, with no personal data in it, marked `personal:` or of a standard type that is, since no erasure reaches a public cache.
  next: Remove `cache:` or `status:` (agent)
  rule: route.cache_invalid
  help: https://jadpo.dev/docs/errors/route.cache_invalid

check failed: 1 error, 1 file
```

And an answer with personal data in it, here a `PhoneNumber`, can never be
kept:

```jadpo check project=e-route-cache-personal expect=route.cache_invalid
type ContactView = Output {
    phone: PhoneNumber
}

action a_contact() -> ContactView {
    return ContactView {
        phone: "+442079460000"
    }
}

policy {
    actions: {
        a_contact: [Access.public]
    }
}

routes Contacts {
    prefix: /contacts

    GET / {
        authentication: RouteAuthentication.none
        output: ContactView
        cache: immutable
        run: a_contact()
    }
}
```

```text output
error: `GET /contacts` says `cache: immutable`, but `a_contact` gives a `PhoneNumber`, which is personal data
 --> main.jadpo:24:14
   |
24 |         run: a_contact()
   |              ^^^^^^^^^ gives a `PhoneNumber`
  reason: `cache: immutable` promises the answer never changes, so any cache on the way may keep it for a year: a `GET` anyone may call, answering 200, with no personal data in it, marked `personal:` or of a standard type that is, since no erasure reaches a public cache.
  next: Remove `cache:`, or send something that isn't personal data (agent)
  rule: route.cache_invalid
  help: https://jadpo.dev/docs/errors/route.cache_invalid

check failed: 1 error, 1 file
```

## How to fix it

The question is whether the same answer is right for anyone who asks.

**If it is**, let anyone call the route, and list what it runs for
`Access.public` in `policy { }`:

```jadpo fragment
GET / {
    authentication: RouteAuthentication.none
    cache: immutable
    run: a_note()
    ...
}
```

```jadpo check project=e-route-cache-public hidden
type Note = Text {
    max_length: 200
}

type NoteView = Output {
    note: Note
}

action a_note() -> NoteView {
    return NoteView {
        note: "Never changes."
    }
}

policy {
    actions: {
        a_note: [Access.public]
    }
}

routes Notes {
    prefix: /notes

    GET / {
        authentication: RouteAuthentication.none
        output: NoteView
        cache: immutable
        run: a_note()
    }
}
```

The route's 200 now carries `Cache-Control: public, max-age=31536000,
immutable`, so a browser or a CDN keeps it for a year.

**If it isn't**, remove `cache:`, and keep the route for signed-in callers:

```jadpo fragment
GET / {
    output: NoteView
    run: a_note()
}
```

```jadpo check project=e-route-cache-removed hidden
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

authentication {
    api_token: {}
}

type Note = Text {
    max_length: 200
}

type NoteView = Output {
    note: Note
}

action a_note() -> NoteView {
    return NoteView {
        note: "Never changes."
    }
}

policy {
    actions: {
        a_note: [Access.authenticated]
    }
}

routes Notes {
    prefix: /notes

    GET / {
        output: NoteView
        run: a_note()
    }
}
```

Nothing keeps the answer, so each caller is asked for and sent their own.

## Related

- [Answers that never change](../language/07-http.md#answers-that-never-change)
- [What the compiler checks](../language/15-personal-data.md#what-the-compiler-checks)
