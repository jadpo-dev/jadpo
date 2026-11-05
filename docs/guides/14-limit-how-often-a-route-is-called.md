# Limit how often a route is called
<!-- topic: guide-limit-how-often-a-route-is-called -->

Every route already has a rate limit, so you write nothing to get one: 600
requests a minute for each signed-in caller, and 120 a minute for each address
on routes open to anyone. You change a limit when a route needs a different
one, such as a public form that should take far fewer. This guide limits a
"request a demo" form to five requests a minute from each address.

You start from an app with nothing in it. Every file this guide uses is shown.

## Limit one route

`rate_limit:` goes on the route. `requests:` is how many it takes and `within:`
is the period. `RateLimitKey` and `RouteAuthentication` are built into Jadpo.
`DemoForm`, `DemoReceived` and `request_demo` are yours, and their files come
straight after this one.

`domains/sales/routes/demo_requests.jadpo`:

```jadpo check project=guide-rate-limits file=domains/sales/routes/demo_requests.jadpo
use sales/types { DemoForm, DemoReceived }
use sales/actions { request_demo }

/// Requests for a demo.
routes DemoRequests {
    prefix: /demo-requests

    /// Asks for a demo.
    POST / {
        authentication: RouteAuthentication.none
        rate_limit: {
            requests: 5
            within: 1m
            by: RateLimitKey.ip
        }
        input: DemoForm
        output: DemoReceived
        status: SuccessStatus.created
        run: request_demo(input)
    }
}
```

The form it takes, what it answers with and the action it runs are ordinary
files. `DemoRequest` is the record that keeps each request.

`domains/sales/entities/demo_request.jadpo`:

```jadpo check project=guide-rate-limits file=domains/sales/entities/demo_request.jadpo
/// Someone asking to see the product.
entity DemoRequest {
    id: Uuid
    company: Text {
        min_length: 1
        max_length: 100
    }

    policy: {
        create: [Access.public]
    }
}
```

`domains/sales/types/demo_form.jadpo`:

```jadpo check project=guide-rate-limits file=domains/sales/types/demo_form.jadpo
use sales/entities { DemoRequest }

/// What someone sends to ask for a demo.
type DemoForm = Input {
    company: DemoRequest.company
}
```

`domains/sales/types/demo_received.jadpo`:

```jadpo check project=guide-rate-limits file=domains/sales/types/demo_received.jadpo
use sales/entities { DemoRequest }

/// What someone gets back for asking.
type DemoReceived = Output {
    pick: [DemoRequest.id]
}
```

`domains/sales/actions/request_demo.jadpo`:

```jadpo check project=guide-rate-limits file=domains/sales/actions/request_demo.jadpo
use sales/entities { DemoRequest }
use sales/types { DemoForm, DemoReceived }

/// Keeps a request for a demo.
action request_demo(input: DemoForm) -> DemoReceived {
    var demo_request: DemoRequest = DemoRequest.create {
        set: {
            company: input.company
        }
    }
    return DemoReceived {
        id: demo_request.id
    }
}
```

`by:` says what is counted:

- `RateLimitKey.ip` counts each address.
- `RateLimitKey.caller`, which you get if you leave `by:` out, counts each
  signed-in caller, or each address when the caller isn't signed in.

A limit you write replaces the default for that route, so it can loosen a limit
as well as tighten it. This route's requests have a count of their own and use
none of the caller's default allowance.

`within:` takes whole seconds, from `1s` to `24h`, and a server program counts any
of them. Cloudflare Workers counts only `10s` and `1m`, so this route runs on both,
and a longer window such as `1h` is a compile error on a Worker.

`jadpo routes` shows each route's limit, and the route's OpenAPI entry lists it
as `x-jadpo-rate-limit`, so a client can see it:

```sh
jadpo routes
```

```text
main  /
  POST  /demo-requests  action request_demo  none  5/1m ip  domains/sales/routes/demo_requests.jadpo:9
  GET   /openapi.json   generated openapi    none           generated

2 routes: 1 API
```

## Try it

A route test never gets a 429, because a test's host counts nothing. You see a
limit on `jadpo dev` and in a deployed app, so start `jadpo dev`, and send the
form five times in another terminal:

```sh
for i in 1 2 3 4 5; do
  curl -s -o /dev/null -w '%{http_code}\n' -X POST http://localhost:3000/demo-requests \
    -H 'content-type: application/json' -d '{"company": "Acme"}'
done
```

```text
201
201
201
201
201
```

The sixth is refused:

```sh
curl -i -X POST http://localhost:3000/demo-requests \
  -H 'content-type: application/json' -d '{"company": "Acme"}'
```

```text
HTTP/1.1 429 Too Many Requests
content-type: application/json
retry-after: 60
ratelimit-policy: "default";q=5;w=60
ratelimit: "default";r=0;t=60
x-request-id: req_70033ff2b9581e87
content-length: 100
date: Wed, 04 Nov 2026 11:18:18 GMT

{"error":{"code":"rate_limited","message":"Too many requests.","request_id":"req_70033ff2b9581e87"}}
```

`Retry-After` is the seconds to wait. `RateLimit-Policy` holds the limit's
requests (`q`) and its period in seconds (`w`), and `RateLimit` says none are
left (`r=0`) for `t` seconds. A refused request never reaches your action, so
you write no code for it. `rate_limited` is a failure every route can give, and
its message comes from translations like any other failure's.

## Change the limit for a whole API

`rate_limit:` on an API replaces the default limit for its routes that write
none of their own, and they share one count, as they share the default. Routes
Jadpo answers itself, such as sign-in and webhooks, keep their own limits.

`app.jadpo`:

```jadpo check project=guide-rate-limits file=app.jadpo
application {
    apis: {
        main: {
            prefix: /
            rate_limit: {
                requests: 300
                within: 1m
            }
        }
    }
    client_address: {
        source: ClientAddressSource.forwarded_for
        hops: config.proxy_hops
    }
}
```

## Count clients behind a proxy

The `client_address:` above matters as soon as your app runs behind a load
balancer. A native app takes an address from its connection, which would be the
balancer's, so all your clients would share one allowance. `client_address:`
says which header holds the real address, and how many proxies of your own
to believe, so a caller can't choose an address by sending that header itself.
On Workers, you write nothing: Jadpo reads Cloudflare's header.

`config.jadpo`:

```jadpo check project=guide-rate-limits file=config.jadpo
use sales/types { ProxyHops }

config {
    proxy_hops: ProxyHops {
        default: 1
    }
}
```

`domains/sales/types/proxy_hops.jadpo`:

```jadpo check project=guide-rate-limits file=domains/sales/types/proxy_hops.jadpo
/// How many proxies of our own stand in front of the app.
type ProxyHops = Int {
    min: 1
    max: 5
}
```

## Know what a limit doesn't do

A limit counts each caller or each address, never everyone together, so a client
with many addresses gets the allowance many times. It makes a public route more
costly to abuse, but it isn't a budget. A route that spends money on each call
needs a cap of its own as well.

Where the counts are kept decides how exact they are:

- **A native app** counts in each server's memory. Several servers each give
  the whole allowance, and a restart starts every count again.
- **On Cloudflare Workers** a Worker's instances share no memory, so a deployable
  must name a store, Cloudflare's rate-limiting binding.
  [Deploy to Cloudflare Workers](11-deploy-to-workers.md) sets it up. Its counts
  are per Cloudflare location, and rough.

## Read more

[Rate limits](../language/16-built-in-services.md#rate-limits),
[Rate-limited requests](../language/07-http.md#rate-limited-requests),
[Client addresses](../language/10-configuration-and-time.md#client-addresses) and
[Rate limits on Workers](../language/16-built-in-services.md#rate-limits-on-workers).

## Not built yet

- Sharing counts between native servers through Redis isn't built yet. A
  deployable whose store is Redis counts in its own memory for now.
- Turning a limit off has no syntax yet. To loosen one, raise `requests:` or
  shorten `within:`.
- A generated route's own limits aren't shown yet in `jadpo routes` or OpenAPI.
