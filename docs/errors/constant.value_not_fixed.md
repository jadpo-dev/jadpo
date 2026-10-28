# `constant.value_not_fixed`

A constant is worked out while compiling, so its value can't come from a
call, `config` or `clock`:

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
  help: docs/language/03-logic.html#constant-value-not-fixed

check failed: 1 error, 1 file
```
