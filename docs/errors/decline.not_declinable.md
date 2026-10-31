# `decline.not_declinable`

Only judgement calls can be declined; a plain mistake is fixed. A rule id
Jadpo doesn't have is reported the same way, naming the nearest declinable
rule.

```jadpo check project=e-decline-not-declinable expect=decline.not_declinable
type Stock = Int {
    min: 0
}

decline layout.unused_declaration: "Kept for next quarter"
function cap_total(total: Stock) -> Stock {
    return total
}
```

```text output
error: `layout.unused_declaration` can't be declined
 --> main.jadpo:5:9
  |
5 | decline layout.unused_declaration: "Kept for next quarter"
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^ a plain mistake
  reason: Only judgement calls can be declined: `constant.similar_exists`, `email.field_unused`, `email.health_problem`, `library.similar_exists`, `package.standard_candidate`, `policy.call_ungranted`, `query.nearest_exact`, `rate_limits.counted_in_memory`, `step.late_failure`, `translation.field_unused`, `type.batch_order_updatable`, `type.standard_exists` and `type.variant_retired`. `layout.unused_declaration` reports a mistake, which only a fix answers.
  next: Remove the `decline` line, and fix what `layout.unused_declaration` reports if it fires (agent)
  rule: decline.not_declinable
  help: https://jadpo.dev/docs/errors/decline.not_declinable

check failed: 1 error, 1 file
```
