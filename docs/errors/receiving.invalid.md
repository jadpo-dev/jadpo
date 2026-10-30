# `receiving.invalid`

A queue in `receiving:` is read through a connection to a package that
provides `receiving` or to Cloudflare Queues (`driver:
Driver.cloudflare_queues`), says where each message's id is with `id_at:`, and
lists the events its messages may be; with more than one event, `type_at:`
says where each message's type is. A database's connection reads no queue:

```jadpo check project=e-receiving-invalid file=app.jadpo expect=receiving.invalid
application {
    connections: {
        main: {
            driver: Driver.postgres
        }
    }
    receiving: {
        main: {
            id_at: "message_id"
            events: []
        }
    }
}
```

```text output
error: `main` isn't a connection a queue is read through
 --> app.jadpo:8:9
  |
8 |         main: {
  |         ^^^^ not a queue's connection
  reason: A queue in `receiving:` is a connection to a package that provides `receiving`, or one with `driver: Driver.cloudflare_queues`; it gives `id_at:`, where each message's id is in its body, `events:`, the events its messages may be, and `type_at:`, where each message's type is, unless it lists one event.
  next: Add `main` to `connections:` with the `package:` its queue is read through, or `driver: Driver.cloudflare_queues`, or name one that is (agent)
  rule: receiving.invalid
  help: https://jadpo.dev/docs/errors/receiving.invalid

check failed: 1 error, 1 file
```
