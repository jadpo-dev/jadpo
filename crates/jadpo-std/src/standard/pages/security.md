# Password

A password as a person types it: 8 to 256 characters.

Use it for what a sign-in form sends. `crypto.hash_password` makes the `PasswordHash` you
store, and `crypto.verify_password` checks one. For any other secret, use `SecretText`.

```jadpo
Password("correct horse battery staple")
```

- It is never sent, logged, stored or shown, and `text.format` won't take it. In JSON it
  is written redacted.

# PasswordHash

A password's hash, ready to store.

Use it in place of a password on your user's record. `crypto.hash_password` makes one,
and `crypto.verify_password` checks a `Password` against it. To hash anything else, use
`Hash`.

```jadpo
PasswordHash("$argon2id$v=19$m=19456,t=2,p=1$AAECAwQFBgcICQoLDA0ODw$gYJZtjEAJqjg26xdLmknq8/bB7MiWPrE9hsYuA+SkIU")
```

- It holds the Argon2id settings it was made with, so older hashes still check.

# PasswordPolicy

The rules a password must meet: how short, how long, and whether to check breached lists.

Use it in sign-in settings. It follows NIST's guidance, so there are no rules about
capitals or symbols, and passwords don't expire.

```jadpo
PasswordPolicy { min_length: 15 max_length: 64 check_breached: true }
```

- The fewest is at least 8, and the most is at least 64.

# Hash

A SHA-256 digest: the 32 bytes `hash.sha256` gives.

Use it to spot changed content or to keep a fingerprint of a key. For a password, use
`PasswordHash`. For a signature with a key, use `Hmac`.

```jadpo
Hash(hash.sha256("hello"))
```

- `==` on two of them takes the same time wherever they differ, so it leaks nothing.
- JSON sends it as Base64.

# Hmac

An HMAC-SHA-256 tag, such as a webhook's signature.

Use it to check that a webhook came from who it says. `crypto.hmac(body, key: key)`
gives the bytes, with a key from your settings, and `Hmac(...)` holds them. For a digest
with no key, use `Hash`.

```jadpo
type NotASignature = Failure {
    kind: FailureKind.invalid_value
    message: "That isn't a signature."
}

var example: Hmac = attempt text.parse(
    "sDRMYdjbOFNcqK/OrwvxK4gdwgDJgz2nJuk3bC4yz/c=",
    as: Hmac
) {
    malformed: NotASignature
}
assert example == example
```

- `==` on two of them takes the same time wherever they differ, so it leaks nothing.

# Jwt

A signed token: three parts joined by `.`.

Use it for a token you receive. `crypto.verify_jwt` checks it and gives its `JwtClaims`.
Don't read one without checking it.

```jadpo
var example: Jwt = Jwt("eyJhbGciOiJFZERTQSJ9.RXhhbXBsZSBvZiBFZDI1NTE5IHNpZ25pbmc.hgyY0il_MGCjP0JzlnLWG1PPOt7-09PGcvMg3AIbQR6dWbhijcNR4ki4iylGjg5BhVsPt9g7sVvpAr_MuM0KAg")
assert example == example
```

- It is a secret, so it is never sent, logged, stored or shown.
- A token with no signature is refused.

# JwtClaims

What a checked token says: who issued it, who it is about and when it expires.

Use what `crypto.verify_jwt` gives you, once it has checked the token. For a token itself,
use `Jwt`. For a signed-in person through OpenID Connect, use `OidcClaims`.

```jadpo
JwtClaims {
    iss: none
    sub: "member-42"
    aud: []
    exp: "2026-10-18T10:00:00Z"
    nbf: none
    iat: none
    jti: none
}
```

- `exp` is always there, and `aud` is always a list.

# Jwk

A public key that a token is checked against, as an identity provider publishes it.

Use it with `crypto.verify_jwt`. To check other signatures with a raw Ed25519 key, use
`PublicKey`.

```jadpo
Jwk {
    kty: "OKP"
    crv: "Ed25519"
    x: "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo"
    y: none
    n: none
    e: none
    kid: none
    alg: none
    use_: none
    key_ops: none
    x5c: none
}
```

- `kty` decides which of the other parts it needs: `OKP`, `RSA` or `EC`.

# PublicKey

An Ed25519 public key: 32 bytes as Base64url.

Use it to check a signature with `crypto.verify_signature`. It isn't secret, so it may be
stored and sent. For a token's key from a provider, use `Jwk`.

```jadpo
PublicKey("11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo")
```

# Certificate

An X.509 certificate, the kind that secures a website.

Use it to read a certificate's subject, issuer and dates. `crypto.read_certificate` and
`text.parse` read one from its PEM text. For a signing key alone, use `PublicKey`.

```jadpo
type NotACertificate = Failure {
    kind: FailureKind.invalid_value
    message: "That isn't a certificate."
}

var example: Certificate = attempt text.parse(
    "-----BEGIN CERTIFICATE-----\nMIIBljCCAUigAwIBAgIIHy49TFtqeYgwBQYDK2VwMEgxCzAJBgNVBAYTAkdCMRww\nGgYDVQQKDBNKYWRwbyBUZXN0LCBTaWduaW5nMRswGQYDVQQDDBJqYWRwby10ZXN0\nLmV4YW1wbGUwHhcNMjYxMDAxMDAwMDAwWhcNMzYxMDAxMDAwMDAwWjBIMQswCQYD\nVQQGEwJHQjEcMBoGA1UECgwTSmFkcG8gVGVzdCwgU2lnbmluZzEbMBkGA1UEAwwS\namFkcG8tdGVzdC5leGFtcGxlMCowBQYDK2VwAyEAAJyTOxfkgCmTOYhl/AXEMRpT\n07QqDzpBZIRCSESxhYejUDBOMB0GA1UdDgQWBBTH4Arltue6aEupl+oqXRoIZos6\nLTAfBgNVHSMEGDAWgBTH4Arltue6aEupl+oqXRoIZos6LTAMBgNVHRMBAf8EAjAA\nMAUGAytlcANBAHkV9ilHQ5TVpSZyPobRcjo9bERYbugOAsKXD4ZJjCRPqpyK+aDc\n+FxV2FFnXI2UQxnYsieWNSclooiCSIuDmwA=\n-----END CERTIFICATE-----\n",
    as: Certificate
) {
    malformed: NotACertificate
}
assert example.not_before < example.not_after
```

- You can't build one from its parts, since they are read from what its issuer signed.
- Reading it doesn't check it is trusted. It gives you what the certificate says.

# TotpSecret

The shared secret behind an authenticator app.

Use it when you set up two-step sign-in. Jadpo's own passkeys and magic links need no such
value. For the six digits a person types, use `OneTimeCode`.

```jadpo
TotpSecret("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ")
```

- It is never sent, logged or shown, and stored only once keys held as data arrive.

# OneTimeCode

A code a person types from an app, a text or an email: six to eight digits.

Use it for what arrives on a form. Check it when it arrives and don't keep it. For the
secret behind an app, use `TotpSecret`.

```jadpo
OneTimeCode("287082")
```

- It is text, so a leading zero is kept.

# ApiKey

A key for an API: up to 512 characters with no spaces.

Use it for a key someone sends you or that you send to a service. Keep `hash.sha256` of it,
not the key. For a token after `Bearer`, use `BearerToken`.

```jadpo
ApiKey("example_key_0123456789abcdef")
```

- It is never sent, logged, stored or shown.

# SecretText

Any other secret text, up to 8,192 characters.

Use it for a value a type below doesn't fit, such as a webhook's signing secret. For a
password, use `Password`, and for a key, use `ApiKey`.

```jadpo
SecretText("whsec_example")
```

- It is never sent, logged, stored or shown.

# BearerToken

An access token sent as `Authorization: Bearer`.

Use it for such a token on an outside call or an incoming request. Keep `hash.sha256` of it,
not the token. For a signed token you can read, use `Jwt`.

```jadpo
BearerToken("mF_9.B5f-4.1JqM")
```

- It is never logged, stored or shown, and a response never sends it.

# OAuthScope

One permission an OAuth token grants: `todos:read`.

Use it to list what a token may do. It holds one scope, so a token's several scopes are a
list of these.

```jadpo
OAuthScope("todos:read")
```

- A space is refused, since it separates scopes.

# OidcClaims

Who an identity provider says signed in.

Use what your sign-in gets after a person signs in through a provider such as Google. `sub`
identifies them for good, and `email` can change. For a token's general claims, use
`JwtClaims`.

```jadpo
OidcClaims {
    sub: "248289761001"
    name: "Jane Doe"
    given_name: none
    family_name: none
    preferred_username: none
    email: none
    email_verified: none
    phone_number: none
    phone_number_verified: none
    picture: none
}
```

# WebAuthnCredential

A passkey a person registered.

Use it if you handle passkeys yourself. Jadpo's own sign-in stores these for you, so most
apps never touch one.

```jadpo
type NotAPasskey = Failure {
    kind: FailureKind.invalid_value
    message: "That isn't a passkey."
}

var example: WebAuthnCredential = attempt WebAuthnCredential {
    credential_id: encoding.to_utf8("id")
    public_key: encoding.to_utf8("key")
    sign_count: 0
    transports: []
    backup_eligible: false
    backed_up: false
    name: "Work phone"
    registered_at: "2026-10-18T10:00:00Z"
    last_used_at: none
} {
    invalid: NotAPasskey
}
assert example == example
```

# EncryptedValue

A value sealed by `crypto.encrypt`.

Use it to store something you must read back later, such as a provider's token.
`crypto.encrypt(value, key: key, context: context)` gives the bytes, with a key from your
settings. For a value you only need to compare, use `Hash` or `PasswordHash`.

```jadpo
type NotSealed = Failure {
    kind: FailureKind.invalid_value
    message: "That isn't a sealed value."
}

var example: EncryptedValue = attempt text.parse(
    "AQGhssPUAAECAwQFBgcICQoLDA0ODxAREhMUFRYXgIGCg4SFhoeIiYqLjI2Ojw==",
    as: EncryptedValue
) {
    malformed: NotSealed
}
assert example == example
```

- It may be stored and sent. Only `crypto.decrypt`, with the key, reads what it holds.

# PasswordCheck

What checking a password says: `wrong`, `correct` or `correct_outdated`.

Use what `crypto.verify_password` gives you. On `correct_outdated` the password is right, but
its hash was made with older settings, so store a fresh one.

```jadpo
PasswordCheck.correct
```
