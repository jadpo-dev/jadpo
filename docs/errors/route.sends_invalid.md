# `route.sends_invalid`

`sends:` names the media type of a body of raw bytes, on a route whose `Output`
has exactly one field, of a type built on `Bytes`, which is then the body. The
media type is a lowercase `type/subtype`, with any parameters after a `;`. It
isn't JSON, which is what an `Output`'s own fields are sent as, or
`text/event-stream`, which is what an `AiStream` is sent as. A route with
`settle:` can't send raw bytes, since they can't say how far the work has got.

Here `Scan` has a second field, so nothing says what would happen to it:

```jadpo check project=e-route-sends-invalid expect=route.sends_invalid
type Note = Text {
    max_length: 200
}

type Scan = Output {
    note: Note
    body: Bytes
}

action a_scan() -> Scan {
    return Scan {
        note: "A scan."
        body: hash.sha256("the scan")
    }
}

routes Scans {
    prefix: /scans

    GET / {
        authentication: RouteAuthentication.none
        output: Scan
        sends: "application/pdf"
        run: a_scan()
    }
}
```

```text output
error: `GET /scans` sends raw bytes, but `Scan` isn't exactly one `Bytes` field
 --> main.jadpo:23:16
   |
23 |         sends: "application/pdf"
   |                ^^^^^^^^^^^^^^^^^ `Scan` holds 2 fields
  reason: `sends:` names the media type of a body of raw bytes: the `Output`'s one field, which is `Bytes`. There is no JSON around it to hold anything else, and a failure still answers in the JSON envelope.
  next: Give `Scan` one field of type `Bytes`, or remove `sends:` (agent)
  rule: route.sends_invalid
  help: https://jadpo.dev/docs/errors/route.sends_invalid

check failed: 1 error, 1 file
```

Send the bytes alone, with `body` as `Scan`'s one field, and read the note from
a route of its own. See [Sending raw bytes](../language/07-http.md#sending-raw-bytes).
