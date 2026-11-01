# `configuration.move_type_unsupported`

A constant `jadpo config move` was asked to make a setting has a type one environment variable can't hold, such as a list or an object.

## Why Jadpo refuses it

A setting is read from one environment variable when the application
starts, so its type has to be one a single piece of text decodes as: a
text, a number, a date, an enum. A list holds any number of values and
an object several fields, and Jadpo won't invent a format to pack them
into one variable. So the move is refused and nothing changes.

## What it looks like

A library in the `accounts` domain lists the ways to sign in:

```jadpo check project=e-move-type file=app.jadpo hidden
application {}
```

```jadpo check project=e-move-type file=domains/accounts/types/login_attempts.jadpo hidden
type LoginAttempts = Int {
    min: 1
    max: 100
}
```

```jadpo check project=e-move-type file=domains/accounts/types/account_locked.jadpo hidden
type AccountLocked = Bool {}
```

```jadpo check project=e-move-type file=domains/accounts/actions/sign_in.jadpo hidden
use accounts/types { AccountLocked, LoginAttempts }
use accounts/libraries { security }

action sign_in(failed_attempts: LoginAttempts) -> AccountLocked {
    return AccountLocked(failed_attempts >= security.max_login_attempts)
}
```

```jadpo check project=e-move-type file=domains/accounts/types/login_method.jadpo hidden
type LoginMethod = Text {
    max_length: 20
}
```

```jadpo check project=e-move-type file=domains/accounts/actions/sign_in_methods.jadpo hidden
use accounts/types { LoginMethod }
use accounts/libraries { security }

action sign_in_methods() -> List<LoginMethod> {
    return security.methods
}
```

domains/accounts/libraries/security.jadpo:

```jadpo check project=e-move-type file=domains/accounts/libraries/security.jadpo
use accounts/types { LoginAttempts, LoginMethod }

library security {
    /// Failed sign-ins before an account is locked.
    constant max_login_attempts: LoginAttempts = 5
    /// The ways to sign in, in the order they are offered.
    constant methods: List<LoginMethod> = ["password", "passkey"]
}
```

```sh run project=e-move-type expect=configuration.move_type_unsupported
jadpo config move security.methods
```

```text output
error: `security.methods` can't be a setting
 --> domains/accounts/libraries/security.jadpo:7:23
  |
7 |     constant methods: List<LoginMethod> = ["password", "passkey"]
  |                       ^^^^^^^^^^^^^^^^^ not read from one variable
  reason: A setting is read from one environment variable, so its type must be decodable from one piece of text: text-based types, `Int`, `Decimal`, `Bool`, `Uuid`, `Email`, `Url`, `IpAddress`, `Duration`, `Instant`, `CalendarDate`, `ModelId`, `Money`, `Certificate` and enums. A list holds any number of values, and a variable holds one; make each value a setting of its own.
  next: Keep `methods` a constant, or split it into settings of those types by hand (agent)
  rule: configuration.move_type_unsupported
  help: https://jadpo.dev/docs/errors/configuration.move_type_unsupported

config move refused: nothing changed
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

The question is what a deployment needs to change.

**If it only needs to turn a way to sign in on or off**, keep the list
a constant and add a `Bool` setting for each one that varies, by hand:

```jadpo check project=e-move-type-flags file=config.jadpo
config {
    /// Whether people may sign in with a passkey.
    passkeys_on: Bool {
        default: true
    }
}
```

```jadpo check project=e-move-type-flags file=app.jadpo hidden
application {}
```

```jadpo check project=e-move-type-flags file=domains/accounts/types/login_attempts.jadpo hidden
type LoginAttempts = Int {
    min: 1
    max: 100
}
```

```jadpo check project=e-move-type-flags file=domains/accounts/types/account_locked.jadpo hidden
type AccountLocked = Bool {}
```

```jadpo check project=e-move-type-flags file=domains/accounts/actions/sign_in.jadpo hidden
use accounts/types { AccountLocked, LoginAttempts }
use accounts/libraries { security }

action sign_in(failed_attempts: LoginAttempts) -> AccountLocked {
    return AccountLocked(failed_attempts >= security.max_login_attempts)
}
```

```jadpo check project=e-move-type-flags file=domains/accounts/types/login_method.jadpo hidden
type LoginMethod = Text {
    max_length: 20
}
```

```jadpo check project=e-move-type-flags file=domains/accounts/libraries/security.jadpo hidden
use accounts/types { LoginAttempts, LoginMethod }

library security {
    /// Failed sign-ins before an account is locked.
    constant max_login_attempts: LoginAttempts = 5
    /// The ways to sign in, in the order they are offered.
    constant methods: List<LoginMethod> = ["password", "passkey"]
}
```

```jadpo check project=e-move-type-flags file=domains/accounts/actions/sign_in_methods.jadpo hidden
use accounts/types { LoginMethod }
use accounts/libraries { security }

action sign_in_methods() -> List<LoginMethod> {
    if (config.passkeys_on) {
        return security.methods
    }
    return ["password"]
}
```

A deployment then sets `PASSKEYS_ON=false` to offer passwords alone,
and the list itself stays in code. If every deployment offers the
same ways, the list stays a constant and nothing needs to change.

## Related

- [Making a constant a setting](../language/10-configuration-and-time.md#making-a-constant-a-setting)
- [`jadpo config move`](../language/14-tooling.md#jadpo-config-move)
