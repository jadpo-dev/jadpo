# `configuration.move_name_taken`

A constant `jadpo config move` was asked to make a setting has the name of a setting `config { }` already declares.

## Why Jadpo refuses it

`jadpo config move <constant>` adds a setting of the constant's name and
rewrites every read to `config.<name>`. When `config { }` already has a
setting of that name, the two would be one name for two values, and
reads that meant the constant would start reading the setting. Which
of them should keep the name is a choice about what the code means, so
the move is refused and nothing changes.

## What it looks like

`config.jadpo` already has a `max_login_attempts`, for the admin site:

```jadpo check project=e-move-name file=app.jadpo hidden
application {}
```

```jadpo check project=e-move-name file=domains/accounts/types/login_attempts.jadpo hidden
type LoginAttempts = Int {
    min: 1
    max: 100
}
```

```jadpo check project=e-move-name file=domains/accounts/types/account_locked.jadpo hidden
type AccountLocked = Bool {}
```

```jadpo check project=e-move-name file=domains/accounts/actions/sign_in.jadpo hidden
use accounts/types { AccountLocked, LoginAttempts }
use accounts/libraries { security }

action sign_in(failed_attempts: LoginAttempts) -> AccountLocked {
    return AccountLocked(failed_attempts >= security.max_login_attempts)
}
```

```jadpo check project=e-move-name file=domains/accounts/libraries/security.jadpo hidden
use accounts/types { LoginAttempts }

library security {
    /// Failed sign-ins before an account is locked.
    constant max_login_attempts: LoginAttempts = 5
}
```

```jadpo check project=e-move-name file=domains/accounts/actions/sign_in_to_admin.jadpo hidden
use accounts/types { AccountLocked, LoginAttempts }

action sign_in_to_admin(failed_attempts: LoginAttempts) -> AccountLocked {
    return AccountLocked(failed_attempts >= config.max_login_attempts)
}
```

config.jadpo:

```jadpo check project=e-move-name file=config.jadpo
use accounts/types { LoginAttempts }

config {
    /// Failed sign-ins to the admin site before an account is locked.
    max_login_attempts: LoginAttempts {
        default: 3
    }
}
```

```sh run project=e-move-name expect=configuration.move_name_taken
jadpo config move security.max_login_attempts
```

```text output
error: A setting is already called `max_login_attempts`
 --> domains/accounts/libraries/security.jadpo:5:14
  |
5 |     constant max_login_attempts: LoginAttempts = 5
  |              ^^^^^^^^^^^^^^^^^^ a setting has this name
  reason: `config { }` declares `max_login_attempts` already, so `security.max_login_attempts` can't become `config.max_login_attempts` without two settings of one name.
  next: Rename the constant or the setting `max_login_attempts`, then run `jadpo config move` on the constant again (agent)
  rule: configuration.move_name_taken
  help: https://jadpo.dev/docs/errors/configuration.move_name_taken

config move refused: nothing changed
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If the setting is the admin site's own**, give it a name that says
so, then move the constant:

```jadpo fragment
config {
    /// Failed sign-ins to the admin site before an account is locked.
    admin_max_login_attempts: LoginAttempts {
        default: 3
    }
}
```

```jadpo check project=e-move-name-renamed file=config.jadpo hidden
use accounts/types { LoginAttempts }

config {
    /// Failed sign-ins to the admin site before an account is locked.
    admin_max_login_attempts: LoginAttempts {
        default: 3
    }
}
```

```jadpo check project=e-move-name-renamed file=app.jadpo hidden
application {}
```

```jadpo check project=e-move-name-renamed file=domains/accounts/types/login_attempts.jadpo hidden
type LoginAttempts = Int {
    min: 1
    max: 100
}
```

```jadpo check project=e-move-name-renamed file=domains/accounts/types/account_locked.jadpo hidden
type AccountLocked = Bool {}
```

```jadpo check project=e-move-name-renamed file=domains/accounts/actions/sign_in.jadpo hidden
use accounts/types { AccountLocked, LoginAttempts }
use accounts/libraries { security }

action sign_in(failed_attempts: LoginAttempts) -> AccountLocked {
    return AccountLocked(failed_attempts >= security.max_login_attempts)
}
```

```jadpo check project=e-move-name-renamed file=domains/accounts/libraries/security.jadpo hidden
use accounts/types { LoginAttempts }

library security {
    /// Failed sign-ins before an account is locked.
    constant max_login_attempts: LoginAttempts = 5
}
```

```jadpo check project=e-move-name-renamed file=domains/accounts/actions/sign_in_to_admin.jadpo hidden
use accounts/types { AccountLocked, LoginAttempts }

action sign_in_to_admin(failed_attempts: LoginAttempts) -> AccountLocked {
    return AccountLocked(failed_attempts >= config.admin_max_login_attempts)
}
```

```sh
jadpo config move security.max_login_attempts
```

A deployment then sets the two apart, as `ADMIN_MAX_LOGIN_ATTEMPTS` and
`MAX_LOGIN_ATTEMPTS`.

**If both should be one value**, delete the constant, and the library
it leaves empty, and read the setting instead:

```jadpo fragment
action sign_in(failed_attempts: LoginAttempts) -> AccountLocked {
    return AccountLocked(failed_attempts >= config.max_login_attempts)
}
```

```jadpo check project=e-move-name-one file=domains/accounts/actions/sign_in.jadpo hidden
use accounts/types { AccountLocked, LoginAttempts }

action sign_in(failed_attempts: LoginAttempts) -> AccountLocked {
    return AccountLocked(failed_attempts >= config.max_login_attempts)
}
```

```jadpo check project=e-move-name-one file=app.jadpo hidden
application {}
```

```jadpo check project=e-move-name-one file=domains/accounts/types/login_attempts.jadpo hidden
type LoginAttempts = Int {
    min: 1
    max: 100
}
```

```jadpo check project=e-move-name-one file=domains/accounts/types/account_locked.jadpo hidden
type AccountLocked = Bool {}
```

```jadpo check project=e-move-name-one file=domains/accounts/actions/sign_in_to_admin.jadpo hidden
use accounts/types { AccountLocked, LoginAttempts }

action sign_in_to_admin(failed_attempts: LoginAttempts) -> AccountLocked {
    return AccountLocked(failed_attempts >= config.max_login_attempts)
}
```

```jadpo check project=e-move-name-one file=config.jadpo hidden
use accounts/types { LoginAttempts }

config {
    /// Failed sign-ins before an account is locked.
    max_login_attempts: LoginAttempts {
        default: 3
    }
}
```

Every sign-in, to the app or the admin site, is then locked after the
setting's number of attempts.

## Related

- [Making a constant a setting](../language/10-configuration-and-time.md#making-a-constant-a-setting)
- [`jadpo config move`](../language/14-tooling.md#jadpo-config-move)
