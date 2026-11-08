# `route.sends_invalid`

A route's `sends:` isn't a media type, or what it sends isn't exactly one `Bytes` field.

## Why Jadpo refuses it

`sends:` names the media type of a body of raw bytes, on a route whose
`Output` has exactly one field of a type built on `Bytes`, which is then the
body. There is no JSON around it, so nothing could carry a second field. The
media type is a lowercase `type/subtype`, with any parameters after a `;`. It
isn't JSON, which is what an `Output`'s own fields are sent as, or
`text/event-stream`, which is what an `AiStream` is sent as. A route with
`settle:` can't send raw bytes, since they can't say how far the work has got.

## What it looks like

main.jadpo:

```jadpo check project=e-route-sends-invalid expect=route.sends_invalid
type Note = Text {
    max_length: 200
}

type ScanRef = Text {
    max_length: 40
}

type Scan = Output {
    note: Note
    body: Bytes
}

action a_scan(scan_ref: ScanRef) -> Scan {
    return Scan {
        note: "A scan."
        body: hash.sha256(scan_ref)
    }
}

policy {
    actions: {
        a_scan: [Access.public]
    }
}

routes Scans {
    prefix: /scans

    GET /{scan_ref: ScanRef} {
        authentication: RouteAuthentication.none
        output: Scan
        sends: "application/pdf"
        run: a_scan(scan_ref)
    }
}
```

```text output
error: `GET /scans/{scan_ref}` sends raw bytes, but `Scan` isn't exactly one `Bytes` field
 --> main.jadpo:33:16
   |
33 |         sends: "application/pdf"
   |                ^^^^^^^^^^^^^^^^^ `Scan` holds 2 fields
  reason: `sends:` names the media type of a body of raw bytes: the `Output`'s one field, which is `Bytes`. There is no JSON around it to hold anything else, and a failure still answers in the JSON envelope.
  next: Give `Scan` one field of type `Bytes`, or remove `sends:` (agent)
  rule: route.sends_invalid
  help: https://jadpo.dev/docs/errors/route.sends_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

JSON is what a route sends without `sends:`:

```jadpo check project=e-route-sends-json expect=route.sends_invalid
type ScanRef = Text {
    max_length: 40
}

type Scan = Output {
    body: Bytes
}

action a_scan(scan_ref: ScanRef) -> Scan {
    return Scan {
        body: hash.sha256(scan_ref)
    }
}

policy {
    actions: {
        a_scan: [Access.public]
    }
}

routes Scans {
    prefix: /scans

    GET /{scan_ref: ScanRef} {
        authentication: RouteAuthentication.none
        output: Scan
        sends: "application/json"
        run: a_scan(scan_ref)
    }
}
```

```text output
error: `application/json` isn't a media type `GET /scans/{scan_ref}` can send
 --> main.jadpo:27:16
   |
27 |         sends: "application/json"
   |                ^^^^^^^^^^^^^^^^^^ JSON is what an `Output`'s own fields are sent as
  reason: `sends:` names the media type of a body of raw bytes: the `Output`'s one field, which is `Bytes`. There is no JSON around it to hold anything else, and a failure still answers in the JSON envelope.
  next: Write a lowercase `type/subtype`, as in `sends: "application/x-tar"` (agent)
  rule: route.sends_invalid
  help: https://jadpo.dev/docs/errors/route.sends_invalid

check failed: 1 error, 1 file
```

A media type is written as text:

```jadpo check project=e-route-sends-not-text expect=route.sends_invalid
type ScanRef = Text {
    max_length: 40
}

type Scan = Output {
    body: Bytes
}

action a_scan(scan_ref: ScanRef) -> Scan {
    return Scan {
        body: hash.sha256(scan_ref)
    }
}

policy {
    actions: {
        a_scan: [Access.public]
    }
}

routes Scans {
    prefix: /scans

    GET /{scan_ref: ScanRef} {
        authentication: RouteAuthentication.none
        output: Scan
        sends: pdf
        run: a_scan(scan_ref)
    }
}
```

```text output
error: `GET /scans/{scan_ref}` gives `sends:` something other than text
 --> main.jadpo:27:16
   |
27 |         sends: pdf
   |                ^^^ not a media type
  reason: `sends:` names the media type of a body of raw bytes: the `Output`'s one field, which is `Bytes`. There is no JSON around it to hold anything else, and a failure still answers in the JSON envelope.
  next: Write `sends: "application/x-tar"` (agent)
  rule: route.sends_invalid
  help: https://jadpo.dev/docs/errors/route.sends_invalid

check failed: 1 error, 1 file
```

## How to fix it

**If the bytes are the whole answer**, make them the `Output`'s one field, and
serve anything else from a route of its own:

```jadpo check project=e-route-sends-fixed
type ScanRef = Text {
    max_length: 40
}

type Scan = Output {
    body: Bytes
}

action a_scan(scan_ref: ScanRef) -> Scan {
    return Scan {
        body: hash.sha256(scan_ref)
    }
}

policy {
    actions: {
        a_scan: [Access.public]
    }
}

routes Scans {
    prefix: /scans

    GET /{scan_ref: ScanRef} {
        authentication: RouteAuthentication.none
        output: Scan
        sends: "application/pdf"
        run: a_scan(scan_ref)
    }
}
```

A client now opens the PDF it is sent.

**If the answer is JSON**, remove `sends:`:

```jadpo fragment
GET /{scan_ref: ScanRef} {
    output: Scan
    run: a_scan(scan_ref)
    ...
}
```

```jadpo check project=e-route-sends-removed hidden
type Note = Text {
    max_length: 200
}

type ScanRef = Text {
    max_length: 40
}

type Scan = Output {
    note: Note
    body: Bytes
}

action a_scan(scan_ref: ScanRef) -> Scan {
    return Scan {
        note: "A scan."
        body: hash.sha256(scan_ref)
    }
}

policy {
    actions: {
        a_scan: [Access.public]
    }
}

routes Scans {
    prefix: /scans

    GET /{scan_ref: ScanRef} {
        authentication: RouteAuthentication.none
        output: Scan
        run: a_scan(scan_ref)
    }
}
```

The route now answers with `Scan`'s fields as JSON.

## Related

- [Sending raw bytes](../language/07-http.md#sending-raw-bytes)
