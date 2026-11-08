# `type.password_sent_to_provider`

A `Password` goes out on a provider's operation.

## Why Jadpo refuses it

A password goes no further than its hash, because anyone holding one can
sign in with it. An operation's input does go further: it is written out
to a provider, a system you don't run, along with whatever that system
logs or keeps. So a call whose input holds a `Password`, however deep, is
refused until a person says why this one should go. Jadpo won't pick for
you, since a directory bind may be exactly what the app is for.

The rule covers everything that goes out on the call: the body, the path
placeholders and the query string are all read from the input.

## What it looks like

```jadpo check project=e-password-to-provider file=app.jadpo hidden
application {
    connections: {
        directory: {
            package: OldDirectory.v1
            domains: [accounts]
            credential: config.directory_key
            environment: config.directory_environment
        }
    }
}
```

```jadpo check project=e-password-to-provider file=config.jadpo hidden
config {
    directory_environment: ProviderEnvironment
    directory_key: Text {
        secret: true
    }
}
```

```jadpo check project=e-password-to-provider file=shared/providers/old_directory/v1/package.jadpo hidden
/// The directory the company is moving off.
package OldDirectory.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-password-to-provider file=shared/providers/old_directory/v1/provider.jadpo hidden
/// How the old directory is reached.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://directory.example"
        sandbox: "https://sandbox.directory.example"
    }
    credential: CredentialScheme.bearer
}
```

`shared/providers/old_directory/v1/types/bind_request.jadpo`:

```jadpo check project=e-password-to-provider file=shared/providers/old_directory/v1/types/bind_request.jadpo
/// A sign-in to check against the directory.
public type BindRequest = Object {
    user: Text
    account_password: Password
}
```

```jadpo check project=e-password-to-provider file=shared/providers/old_directory/v1/types/bind_result.jadpo hidden
/// What the directory says of it.
public type BindResult = Object {
    match_found: Bool
}
```

```jadpo check project=e-password-to-provider file=shared/providers/old_directory/v1/resources/sign_ons.jadpo hidden
public resource sign_ons {
    create: {
        method: HttpMethod.post
        path: "/v1/sign_ons"
        input: BindRequest
        output: BindResult
        changes: false
        source: PackageSource.written
    }
}
```

`domains/accounts/actions/check_password.jadpo`:

```jadpo check project=e-password-to-provider file=domains/accounts/actions/check_password.jadpo expect=type.password_sent_to_provider
use app { directory }
use shared/providers/old_directory

/// Whether a person's password is the one the old directory has.
action check_password(request: OldDirectory.BindRequest) -> OldDirectory.BindResult {
    return directory.sign_ons.create(request)
}
```

```text output
error: `directory.sign_ons.create` sends a `Password` to a provider
 --> domains/accounts/actions/check_password.jadpo:6:12
  |
6 |     return directory.sign_ons.create(request)
  |            ^^^^^^^^^^^^^^^^^^^^^^^^^ a password, sent out
  reason: `OldDirectory.BindRequest` holds a `Password`. A password goes no further than its hash, because anyone holding one can sign in with it; sent to a provider, it is in the hands of a system you don't run, and of whatever that system logs or keeps.
  next: Send what the provider needs instead, such as a token issued for this, and keep the password's hash here, or write `decline type.password_sent_to_provider: "<why>"` above the action if this is deliberate, which a person then accepts (human)
  rule: type.password_sent_to_provider
  help: https://jadpo.dev/docs/errors/type.password_sent_to_provider

check failed: 1 error, 8 files
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether someone else's system gets to see your people's
passwords.

## How to fix it

Decide whether this call has to carry the password.

**If the provider can take something else**, send that: a token it
issued for this. Keep `crypto.hash_password` of the password here.

```jadpo fragment
public type BindRequest = Object {
    user: Text
    token: BearerToken
}
```

```jadpo check project=e-password-to-provider-token file=shared/providers/old_directory/v1/types/bind_request.jadpo hidden
/// A sign-in to check against the directory.
public type BindRequest = Object {
    user: Text
    token: BearerToken
}
```

```jadpo check project=e-password-to-provider-token file=app.jadpo hidden
application {
    connections: {
        directory: {
            package: OldDirectory.v1
            domains: [accounts]
            credential: config.directory_key
            environment: config.directory_environment
        }
    }
}
```

```jadpo check project=e-password-to-provider-token file=config.jadpo hidden
config {
    directory_environment: ProviderEnvironment
    directory_key: Text {
        secret: true
    }
}
```

```jadpo check project=e-password-to-provider-token file=shared/providers/old_directory/v1/package.jadpo hidden
/// The directory the company is moving off.
package OldDirectory.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-password-to-provider-token file=shared/providers/old_directory/v1/provider.jadpo hidden
/// How the old directory is reached.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://directory.example"
        sandbox: "https://sandbox.directory.example"
    }
    credential: CredentialScheme.bearer
}
```

```jadpo check project=e-password-to-provider-token file=shared/providers/old_directory/v1/types/bind_result.jadpo hidden
/// What the directory says of it.
public type BindResult = Object {
    match_found: Bool
}
```

```jadpo check project=e-password-to-provider-token file=shared/providers/old_directory/v1/resources/sign_ons.jadpo hidden
public resource sign_ons {
    create: {
        method: HttpMethod.post
        path: "/v1/sign_ons"
        input: BindRequest
        output: BindResult
        changes: false
        source: PackageSource.written
    }
}
```

```jadpo check project=e-password-to-provider-token file=domains/accounts/actions/check_password.jadpo hidden
use app { directory }
use shared/providers/old_directory

/// Whether a person's token is one the old directory issued.
action check_password(request: OldDirectory.BindRequest) -> OldDirectory.BindResult {
    return directory.sign_ons.create(request)
}
```

The check now passes, and a token goes out as itself.

**If this call has to carry the password**, decline the rule above the
action with the reason, and a person accepts that decline in an
[acceptance](../language/18-records-and-manifest.md#acceptances):

```jadpo fragment
/// Whether a person's password is the one the old directory has.
decline type.password_sent_to_provider: "Binds against the directory we are moving off, until the migration ends"
action check_password(request: OldDirectory.BindRequest) -> OldDirectory.BindResult {
    return directory.sign_ons.create(request)
}
```

```jadpo check project=e-password-to-provider-declined file=domains/accounts/actions/check_password.jadpo hidden
use app { directory }
use shared/providers/old_directory

/// Whether a person's password is the one the old directory has.
decline type.password_sent_to_provider: "Binds against the directory we are moving off, until the migration ends"
action check_password(request: OldDirectory.BindRequest) -> OldDirectory.BindResult {
    return directory.sign_ons.create(request)
}
```

```jadpo check project=e-password-to-provider-declined file=app.jadpo hidden
application {
    connections: {
        directory: {
            package: OldDirectory.v1
            domains: [accounts]
            credential: config.directory_key
            environment: config.directory_environment
        }
    }
}
```

```jadpo check project=e-password-to-provider-declined file=config.jadpo hidden
config {
    directory_environment: ProviderEnvironment
    directory_key: Text {
        secret: true
    }
}
```

```jadpo check project=e-password-to-provider-declined file=shared/providers/old_directory/v1/package.jadpo hidden
/// The directory the company is moving off.
package OldDirectory.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-password-to-provider-declined file=shared/providers/old_directory/v1/provider.jadpo hidden
/// How the old directory is reached.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://directory.example"
        sandbox: "https://sandbox.directory.example"
    }
    credential: CredentialScheme.bearer
}
```

```jadpo check project=e-password-to-provider-declined file=shared/providers/old_directory/v1/types/bind_result.jadpo hidden
/// What the directory says of it.
public type BindResult = Object {
    match_found: Bool
}
```

```jadpo check project=e-password-to-provider-declined file=shared/providers/old_directory/v1/resources/sign_ons.jadpo hidden
public resource sign_ons {
    create: {
        method: HttpMethod.post
        path: "/v1/sign_ons"
        input: BindRequest
        output: BindResult
        changes: false
        source: PackageSource.written
    }
}
```

```jadpo check project=e-password-to-provider-declined file=shared/providers/old_directory/v1/types/bind_request.jadpo hidden
/// A sign-in to check against the directory.
public type BindRequest = Object {
    user: Text
    account_password: Password
}
```

The cases this is for are binding to an LDAP or Active Directory server,
checking a password against an old system while people move off it, and
setting a person's first password at an identity provider. A signed-off
password is written as itself into that one request. Everywhere Jadpo shows
what was sent, the development call log and `jadpo calls` included, it
still shows as `<redacted Password>`.

The decline is `PolicyChange.password_sent_to_provider_declined` on the
action, and `access.jadpo_record` lists it. A package's own code can't
sign it off for the apps that install it, so the rule fires at the app's
call: of an operation, or of a package's action that sends the password
on.

## Related

- [Where a secret can go](../language/19-standard-library.md#where-a-secret-can-go)
- [Calling a provider](../language/11-integrations.md#calling-a-provider)
- [Writing a decline](../language/01-project-layout-and-modules.md#writing-a-decline)
- [Acceptances](../language/18-records-and-manifest.md#acceptances)
