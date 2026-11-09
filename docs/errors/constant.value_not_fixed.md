# `constant.value_not_fixed`

A constant's value is only known while the app runs, because it comes from a call, `config` or `clock`.

## Why Jadpo refuses it

A constant is the same in every deployment and is worked out while compiling,
from literals, enum values, other constants, constructions, objects, lists and
operators. A call, `config` and `clock` only have values while the app runs, so
a constant can't hold one and still be a constant.

A value is one or the other, never both, so it has one source and one way to
read it. Jadpo won't pick for you, since that decides whether a deployment can
change it.

## What it looks like

```jadpo check project=e-constant-not-fixed expect=constant.value_not_fixed
type LoginAttempts = Int {
    min: 1
}

constant max_login_attempts: LoginAttempts = config.max_login_attempts

config {
    max_login_attempts: LoginAttempts {
        default: 5
    }
}
```

```text output
error: A constant can't read `config.max_login_attempts`
 --> main.jadpo:5:46
  |
5 | constant max_login_attempts: LoginAttempts = config.max_login_attempts
  |                                              ^^^^^^^^^^^^^^^^^^^^^^^^^ only known while the app runs
  reason: A constant is worked out while compiling, from literals, enum values, other constants, constructions, objects, lists and operators; a call, `config` and `clock` give values only while the app runs.
  next: Read `config.max_login_attempts` where the value is used: a setting is chosen per deployment, and a value is a constant or a setting, never both (agent)
  rule: constant.value_not_fixed
  help: https://jadpo.dev/docs/errors/constant.value_not_fixed

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A call only runs while the app does, so it can't give a constant its value:

```jadpo check project=e-constant-not-fixed-call expect=constant.value_not_fixed
type Stock = Int {
    min: 0
}

function batch() -> Stock {
    return 5
}

constant bulk: Stock = batch()
```

```text output
error: A constant can't call `batch`
 --> main.jadpo:9:24
  |
9 | constant bulk: Stock = batch()
  |                        ^^^^^^^ only known while the app runs
  reason: A constant is worked out while compiling, from literals, enum values, other constants, constructions, objects, lists and operators; a call, `config` and `clock` give values only while the app runs.
  next: Write the value `batch(...)` gives, or call `batch` where the value is used (agent)
  rule: constant.value_not_fixed
  help: https://jadpo.dev/docs/errors/constant.value_not_fixed

check failed: 1 error, 1 file
```

## How to fix it

The question is whether a deployment could ever need a different value.

**If it could**, make it a setting and read it where it is used:

```jadpo fragment
function attempts_allowed() -> LoginAttempts {
    return config.max_login_attempts
}
```

```jadpo check project=e-constant-not-fixed-fix-1 hidden
type LoginAttempts = Int {
    min: 1
}

function attempts_allowed() -> LoginAttempts {
    return config.max_login_attempts
}

config {
    max_login_attempts: LoginAttempts {
        default: 5
    }
}
```

Each deployment can now set the value, and it defaults to 5.

**If it never changes**, write the value itself:

```jadpo fragment
constant max_login_attempts: LoginAttempts = 5
```

```jadpo check project=e-constant-not-fixed-fix-2 hidden
type LoginAttempts = Int {
    min: 1
}

constant max_login_attempts: LoginAttempts = 5
```

The value is now fixed while compiling and the same in every deployment.

## Related

- [Constants](../language/03-logic.md#constants)
- [A constant or a setting](../language/03-logic.md#a-constant-or-a-setting)
