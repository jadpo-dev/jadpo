# `configuration.move_read_unsupported`

A constant `jadpo config move` was asked to make a setting is read where a setting can't be: in a query, an update by `id`, a backfill or another constant's value.

## Why Jadpo refuses it

`jadpo config move <constant>` turns a constant into a setting and
rewrites every read of it to `config.<name>`. A setting is known only
when the application starts. A query, an update by `id` and a backfill
are worked out by the database, and another constant's value while
compiling, so none of them can read one. The move would leave code that
doesn't check, so it refuses, names every such read, and changes
nothing.

## What it looks like

A library in the `accounts` domain works out when to warn from the
number of sign-ins before an account is locked:

```jadpo check project=e-move-read file=app.jadpo hidden
application {}
```

```jadpo check project=e-move-read file=domains/accounts/types/login_attempts.jadpo hidden
type LoginAttempts = Int {
    min: 1
    max: 100
}
```

```jadpo check project=e-move-read file=domains/accounts/types/account_locked.jadpo hidden
type AccountLocked = Bool {}
```

```jadpo check project=e-move-read file=domains/accounts/actions/sign_in.jadpo hidden
use accounts/types { AccountLocked, LoginAttempts }
use accounts/libraries { security }

action sign_in(failed_attempts: LoginAttempts) -> AccountLocked {
    return AccountLocked(failed_attempts >= security.max_login_attempts)
}
```

```jadpo check project=e-move-read file=domains/accounts/actions/should_warn.jadpo hidden
use accounts/types { AccountLocked, LoginAttempts }
use accounts/libraries { security }

action should_warn(failed_attempts: LoginAttempts) -> AccountLocked {
    return AccountLocked(failed_attempts >= security.warn_after)
}
```

domains/accounts/libraries/security.jadpo:

```jadpo check project=e-move-read file=domains/accounts/libraries/security.jadpo
use accounts/types { LoginAttempts }

library security {
    /// Failed sign-ins before an account is locked.
    constant max_login_attempts: LoginAttempts = 5
    /// Failed sign-ins before a warning is shown.
    constant warn_after: LoginAttempts = LoginAttempts(max_login_attempts - 2)
}
```

```sh run project=e-move-read expect=configuration.move_read_unsupported
jadpo config move security.max_login_attempts
```

```text output
error: `security.max_login_attempts` is read in a constant's value
 --> domains/accounts/libraries/security.jadpo:7:56
  |
7 |     constant warn_after: LoginAttempts = LoginAttempts(max_login_attempts - 2)
  |                                                        ^^^^^^^^^^^^^^^^^^ in a constant's value
  reason: `security.warn_after`'s value is worked out while compiling, and a setting is known only when the application starts. Moving `security.max_login_attempts` would make this read `config.max_login_attempts`.
  next: Write `security.warn_after`'s value out, or make `security.warn_after` a setting first, then run `jadpo config move security.max_login_attempts` again (agent)
  rule: configuration.move_read_unsupported
  help: https://jadpo.dev/docs/errors/configuration.move_read_unsupported

config move refused: nothing changed
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

A read in a query, an update by `id` or a backfill is refused the same
way, with what to pass instead.

## How to fix it

**If the warning should be a setting too**, make it one first, then
the constant it reads:

```sh
jadpo config move security.warn_after
jadpo config move security.max_login_attempts
```

`config.jadpo` then has both settings, `warn_after` with the value it
had, 3, as its `default:`. A deployment sets each on its own, as
`WARN_AFTER` and `MAX_LOGIN_ATTEMPTS`.

**If it should stay fixed**, write its value out:

```jadpo fragment
library security {
    ...
    constant warn_after: LoginAttempts = 3
}
```

```jadpo check project=e-move-read-fixed file=domains/accounts/libraries/security.jadpo hidden
use accounts/types { LoginAttempts }

library security {
    /// Failed sign-ins before an account is locked.
    constant max_login_attempts: LoginAttempts = 5
    /// Failed sign-ins before a warning is shown.
    constant warn_after: LoginAttempts = 3
}
```

```jadpo check project=e-move-read-fixed file=app.jadpo hidden
application {}
```

```jadpo check project=e-move-read-fixed file=domains/accounts/types/login_attempts.jadpo hidden
type LoginAttempts = Int {
    min: 1
    max: 100
}
```

```jadpo check project=e-move-read-fixed file=domains/accounts/types/account_locked.jadpo hidden
type AccountLocked = Bool {}
```

```jadpo check project=e-move-read-fixed file=domains/accounts/actions/sign_in.jadpo hidden
use accounts/types { AccountLocked, LoginAttempts }
use accounts/libraries { security }

action sign_in(failed_attempts: LoginAttempts) -> AccountLocked {
    return AccountLocked(failed_attempts >= security.max_login_attempts)
}
```

```jadpo check project=e-move-read-fixed file=domains/accounts/actions/should_warn.jadpo hidden
use accounts/types { AccountLocked, LoginAttempts }
use accounts/libraries { security }

action should_warn(failed_attempts: LoginAttempts) -> AccountLocked {
    return AccountLocked(failed_attempts >= security.warn_after)
}
```

`jadpo config move security.max_login_attempts` then makes it a
setting, and the warning stays at 3 whatever a deployment sets.

## Related

- [Making a constant a setting](../language/10-configuration-and-time.md#making-a-constant-a-setting)
- [`jadpo config move`](../language/14-tooling.md#jadpo-config-move)
