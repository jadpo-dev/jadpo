# `route.cache_invalid`

`cache: immutable` says an answer never changes, so any cache on the way may
keep it for a year. It goes on a `GET` anyone may call, answering 200, and what
the route runs can't give personal data, since no erasure reaches a public
cache: a field marked `personal:`, or a standard type that is personal data
whatever holds it, such as `PhoneNumber`, on a field with no mark or as the output's
own type ([What the compiler checks](../language/15-personal-data.md#what-the-compiler-checks)).
`immutable` is the one thing `cache:` says.

Here the route needs a signed-in caller, and a public cache would hand what
that caller was sent to anyone who asked:

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

Say who may call it, or don't let caches keep it:

```jadpo fragment
GET / {
    authentication: RouteAuthentication.none
    output: NoteView
    cache: immutable
    run: a_note()
}
```

See [Answers that never change](../language/07-http.md#answers-that-never-change).
