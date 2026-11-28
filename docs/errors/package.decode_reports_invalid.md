# `package.decode_reports_invalid`

Application's `packages:` sets `report_decode_failures:` to something other than `true` or `false`.

## Why Jadpo refuses it

`report_decode_failures:` says whether the app's contract runs tell the
registry of an answer a package from it can no longer decode, sending the
answer's shape and never a value of it. Something sent to a third party
is sent only when the app says so plainly, so the setting is `true` or
`false`, written once. Anything else sends nothing, and is refused, so a
setting that reads as on is never quietly off, nor the other way round.

## What it looks like

A word that reads as yes, but isn't `true`:

```jadpo check project=e-decode-reports-word expect=package.decode_reports_invalid
application {
    packages: {
        report_decode_failures: "yes"
    }
}
```

```text output
error: `report_decode_failures:` isn't `true` or `false`
 --> main.jadpo:3:33
  |
3 |         report_decode_failures: "yes"
  |                                 ^^^^^ not true or false
  reason: `report_decode_failures:` says whether the app's contract runs tell the registry of an answer a package from it can no longer decode, sending the answer's shape and never a value. Left out, it is `false` and nothing is sent.
  next: Write `report_decode_failures: true`, or remove it to send nothing (agent)
  rule: package.decode_reports_invalid
  help: https://jadpo.dev/docs/errors/package.decode_reports_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A misspelt setting is refused with the edit to the right name, since a
report meant to be on would otherwise be off without a word:

```jadpo check project=e-decode-reports-misspelt expect=package.decode_reports_invalid
application {
    packages: {
        report_decode_failure: true
    }
}
```

```text output
error: `report_decode_failure` isn't something `packages:` holds
 --> main.jadpo:3:9
  |
3 |         report_decode_failure: true
  |         ^^^^^^^^^^^^^^^^^^^^^ not a setting
  reason: `packages:` holds `own_sources:`, your company's git locations, and `report_decode_failures:`, whether contract runs tell the registry of an answer a package can't decode.
  next: Use `report_decode_failures` instead of `report_decode_failure` (agent)
  fix:
    3 - report_decode_failure: true
    3 + report_decode_failures: true
  rule: package.decode_reports_invalid
  help: https://jadpo.dev/docs/errors/package.decode_reports_invalid

check failed: 1 error, 1 file

next: jadpo fix  fixes this error for you, then checks again
```

## How to fix it

**If the app's contract runs should report**, write `true`:

```jadpo check project=e-decode-reports-fix-1
application {
    packages: {
        report_decode_failures: true
    }
}
```

**If they shouldn't**, remove the setting: left out, it is `false`, and
nothing is sent.

## Related

- [Reporting what a package can't decode](../language/11-integrations.md#reporting-what-a-package-cant-decode)
- [The registry protocol](../language/11-integrations.md#the-registry-protocol)
