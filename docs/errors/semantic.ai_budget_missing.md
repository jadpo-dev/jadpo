# `semantic.ai_budget_missing`

Nothing an application asks a model is unlimited: `ai:` gives `limits: {
per_day: { tokens: } }`, the most its calls may use in a day, so a loop or
an injected "write ten thousand words" can't spend without end. `ai:`
without `limits:`, `limits:` without `per_day:`, and `per_day:` without
`tokens:` are each this error.

```jadpo check project=e-ai-budget-missing file=config.jadpo hidden
config {
    drafting_model: ModelId {
        default: "meta-llama/Llama-3.1-8B-Instruct"
    }
    model_url: Url {
        default: "http://localhost:8000/v1"
    }
}
```

```jadpo check project=e-ai-budget-missing file=app.jadpo expect=semantic.ai_budget_missing
application {
    connections: {
        local_model: {
            driver: Driver.openai_compatible
            url: config.model_url
        }
    }
    ai: {
        connection: local_model
        models: {
            default: config.drafting_model
        }
    }
}
```

```text output
error: `ai:` sets no daily limit
 --> app.jadpo:8:5
  |
8 |     ai: {
  |     ^^ no `limits:`
  reason: Nothing an application asks a model is unlimited: `per_day: { tokens: }` caps the day's tokens, so a loop, or a prompt an injection makes run on, can't spend without end.
  next: Add `limits: { per_day: { tokens: 2000000 } }` to `ai:` (agent)
  rule: semantic.ai_budget_missing
  help: docs/errors/semantic.ai_budget_missing.md

check failed: 1 error, 2 files
```
