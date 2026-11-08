# `semantic.ai_budget_missing`

An application's `ai:` settings don't give a daily limit on tokens.

## Why Jadpo refuses it

Every call to a model costs tokens, and a loop or a prompt injection that
asks for ten thousand words could spend without end. So nothing an
application asks a model is unlimited: `ai:` always says the most its
calls may use in a day, and Jadpo won't pick that number for you.

## What it looks like

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

app.jadpo:


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
  help: https://jadpo.dev/docs/errors/semantic.ai_budget_missing

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A `limits:` with no `per_day:` is refused the same way, and so is a
`per_day:` with no `tokens:`:

```jadpo check project=e-ai-budget-no-tokens file=config.jadpo hidden
config {
    drafting_model: ModelId {
        default: "meta-llama/Llama-3.1-8B-Instruct"
    }
    model_url: Url {
        default: "http://localhost:8000/v1"
    }
}
```

```jadpo check project=e-ai-budget-no-tokens file=app.jadpo expect=semantic.ai_budget_missing
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
        limits: {
            per_day: {}
        }
    }
}
```

```text output
error: `per_day:` sets no daily tokens
 --> app.jadpo:14:13
   |
14 |             per_day: {}
   |             ^^^^^^^ no `tokens:`
  reason: Nothing an application asks a model is unlimited: `per_day: { tokens: }` caps the day's tokens, so a loop, or a prompt an injection makes run on, can't spend without end.
  next: Add `tokens: 2000000` to `per_day:` (agent)
  rule: semantic.ai_budget_missing
  help: https://jadpo.dev/docs/errors/semantic.ai_budget_missing

check failed: 1 error, 2 files
```

## How to fix it

**If the app calls a model**, give `ai:` the most tokens its calls may use
in a day:

```jadpo fragment
    ai: {
        connection: local_model
        models: {
            default: config.drafting_model
        }
        limits: {
            per_day: {
                tokens: 2000000
            }
        }
    }
```

```jadpo check project=e-ai-budget-fix file=app.jadpo hidden
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
        limits: {
            per_day: {
                tokens: 2000000
            }
        }
    }
}
```

```jadpo check project=e-ai-budget-fix file=config.jadpo hidden migrate=initial
config {
    drafting_model: ModelId {
        default: "meta-llama/Llama-3.1-8B-Instruct"
    }
    model_url: Url {
        default: "http://localhost:8000/v1"
    }
}
```

Once the day's tokens are used up, further calls fail as `over_budget`
until the next day.

## Related

- [Budgets](../language/21-ai.md#budgets)
- [How budgets are counted](../language/21-ai.md#how-budgets-are-counted)
