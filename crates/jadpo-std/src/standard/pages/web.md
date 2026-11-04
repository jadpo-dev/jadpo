# Url

An `http://` or `https://` address with a host.

Use it for links you store or send. For a path on your own site, use `UrlPath`. For
just the scheme and host, use `WebOrigin`.

```jadpo
Url("https://example.com/orders")
```

- `example.com` alone and `ftp://` are refused, so add the scheme.
- `web.with_query` adds query parameters, and `web.resolve` turns a `RelativeUrl` into
  one.

# UrlPath

A URL's path, starting with `/`: `/products/42`.

Use it for a path on your own site. For a whole address, use `Url`. For a query or
`../` as well, use `RelativeUrl`.

```jadpo
UrlPath("/products/42")
```

- It has no query or fragment, so `/a?x=1` is refused.

# RelativeUrl

An address relative to another: `../images/logo.png`, `?page=2`.

Use it for links inside a document. `web.resolve` makes a `Url` of one. For a plain
path from the root, use `UrlPath`.

```jadpo
RelativeUrl("../images/logo.png")
```

- A scheme is refused, so `https://example.com/` is a `Url` instead.

# Slug

A URL-safe name: lower-case words joined by hyphens, `autumn-sale`.

Use it for readable addresses, such as a blog post's. `text.slug` makes one from a
title. For a login name, use `Username`.

```jadpo
Slug("autumn-sale")
```

- Capitals and double hyphens are refused. `text.slug` tidies text so it fits.

# DomainName

A domain name: `example.com`.

Use it for a customer's or a company's domain. To allow an IP address or `localhost`
as well, use `Hostname`. For a whole address, use `Url`.

```jadpo
DomainName("example.com")
```

- It is lower case with no dot at the end. A non-ASCII label is held in its `xn--` form,
  such as `xn--mnchen-3ya.de`.

# Hostname

A host: a domain name or an IP address.

Use it for what a server connects to. For a name that must be a real domain, use
`DomainName`. To add a scheme, use `WebOrigin`.

```jadpo
Hostname("localhost")
```

- It is lower case. An IPv6 address has no brackets, so `[2001:db8::1]` is refused.

# WebOrigin

A web origin: the scheme, the host and any port, `https://example.com`.

Use it for browser-security lists, such as which sites may call your API. For a whole
address, use `Url`.

```jadpo
WebOrigin("https://example.com")
```

- It has no path, so no trailing `/`. The default port is left out, so
  `https://example.com:443` is refused.

# HttpMethod

An HTTP method a route is written with: `GET`, `POST`, `PUT`, `PATCH` or `DELETE`.

Use it when you record or compare requests, as in a test's `request { }`.

```jadpo
HttpMethod.post
```

- `HEAD` and others that routes aren't written with are refused.

# HttpStatus

An HTTP response's status code, from 100 to 599.

Use it for what a response carried, such as in a log. In a route, return your failures,
and let Jadpo pick the status.

```jadpo
HttpStatus(404)
```

# HttpHeaderName

An HTTP header's name, in lower case: `content-type`.

Use it to name a header in a list or in logs. Capitals are refused, so lower-case any
you read.

```jadpo
HttpHeaderName("content-type")
```

# FileType

A file's media type: `image/png`, `application/pdf` and so on.

Use it to say which kinds of file an upload accepts.

```jadpo
FileType.png
```

- It holds a fixed list. Any other media type, such as `image/tiff`, is refused.

# UserAgent

What an HTTP client says it is: `curl/8.7.1`.

Use it to record how a request was made, such as in an audit log. A client can claim
anything, so don't use it to decide what someone may do.

```jadpo
UserAgent("curl/8.7.1")
```

# ProblemDetails

An HTTP API's error body, as other APIs often send one.

Use it to read errors from an outside service. In your own routes, declare failures
instead.

```jadpo
ProblemDetails {
    type: none
    status: 404
    title: "Not Found"
    detail: none
    instance: none
}
```

# JsonPointer

A pointer into a JSON document: `/foo/0`.

Use it to name a value in a document, such as in an error. Empty text means the whole
document.

```jadpo
JsonPointer("/foo/0")
```

- `~1` stands for `/` in a name, and `~0` for `~`. A pointer that doesn't start with
  `/` is refused.

# Cookie

A cookie as a server sets it: `SID=31d4d96e; Path=/; Secure; HttpOnly`.

Use it to read or record cookies from an outside service. Your own app's sign-in
cookies are handled for you, so you don't set them.

```jadpo
Cookie("SID=31d4d96e; Path=/; Secure; HttpOnly")
```

- Attributes are in the standard's order and spelling, so `secure` is refused.

# UtmParameters

Where a visit came from, as marketing links carry it: source, medium and campaign.

Use it to record a campaign with a sign-up or an order. For a whole address, use `Url`.

```jadpo
UtmParameters {
    source: "mailing"
    medium: "email"
    campaign: "autumn_sale"
    term: none
    content: none
}
```

- As text, parameters must be in the standard's order, `utm_source` first.

# IpAddress

An IPv4 or IPv6 address: `192.0.2.1`, `2001:db8::1`.

Use it for an address you allow or block. For a range of addresses, use `Cidr`. For a
name, use `Hostname`.

```jadpo
IpAddress("192.0.2.1")
```

- When it identifies a person, mark the field that holds it `personal: PersonalData.online`.

# Cidr

A range of IP addresses: `192.0.2.0/24`.

Use it for allow lists. `web.cidr_contains` tells you whether an `IpAddress` is inside.

```jadpo
Cidr("192.0.2.0/24")
```

- It has to start at the range's first address, so `192.0.2.1/24` is refused.

# NetworkPort

A TCP or UDP port, from 1 to 65535.

Use it for what a service listens on. In JSON it is a plain number.

```jadpo
NetworkPort(443)
```

# MacAddress

A network card's hardware address: `AC-DE-48-00-00-80`.

Use it for device inventories. For a phone, use `Imei`.

```jadpo
MacAddress("AC-DE-48-00-00-80")
```

- It is upper case and joined by `-`, so change `:` and lower case first.

# Imei

A mobile device's 15-digit IMEI.

Use it to identify a phone. For an address to push notifications to, use `PushToken`.

```jadpo
Imei("490154203237518")
```

- It checks its own last digit, so a mistyped number is usually refused.

# Emoji

One emoji: `😀`, `👍🏽`.

Use it for reactions and labels, when you want exactly one emoji. For text with emoji in
it, use `Text`.

```jadpo
Emoji("😀")
```

- It has to be in the full form Unicode lists. A red heart needs its variation selector
  (`U+2764 U+FE0F`), so the bare `U+2764` is refused.

# PushToken

The address of a device for push notifications, from Apple's or Google's service.

Use it to send a push notification. A token can stop working, so replace it when the
service says so. For a phone itself, use `Imei`.

```jadpo
PushToken("bk3RNwTe3H0:CI2k_HHwgIpoDKCIZvvDMExUdFQ3P1")
```
