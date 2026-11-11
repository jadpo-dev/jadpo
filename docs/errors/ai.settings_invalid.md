# `ai.settings_invalid`

The application's `ai:` holds a setting it doesn't take, or leaves out one it needs.

## Why Jadpo refuses it

`application { ai: }` says how the app reaches a model: `connection:`,
a connection with `driver: Driver.openai_compatible` or to a package
that fills `ai`, and `models:`, the models it asks, each a `ModelId`
setting, with `default` among them. A model is a setting so that moving
to another is a reviewed change to configuration, and a `ModelId`
always names a provider and a model, never free text a typo can slip
into. Jadpo refuses a model that isn't a `ModelId` setting, `models:`
without `default`, a connection that answers no model and a key `ai:`
doesn't take, since each leaves a call asking something other than
what the reader of `app.jadpo` sees.

## What it looks like

config.jadpo:

```jadpo check project=e-ai-settings-invalid file=config.jadpo
config {
    drafting_model: Text {
        default: "meta-llama/Llama-3.1-8B-Instruct"
    }
    model_url: Url {
        default: "http://localhost:8000/v1"
    }
}
```

app.jadpo:

```jadpo check project=e-ai-settings-invalid file=app.jadpo expect=ai.settings_invalid
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

```text output
error: Model `default` must be a `ModelId` setting
 --> app.jadpo:11:22
   |
11 |             default: config.drafting_model
   |                      ^^^^^^^^^^^^^^^^^^^^^ `drafting_model` is `Text`
  reason: Each model is a `ModelId` setting, its provider and name as Hugging Face writes a repository, `openai/gpt-4.1-mini`, so a model is never anything else.
  next: Declare `drafting_model: ModelId` in `config.jadpo` (agent)
  rule: ai.settings_invalid
  help: https://jadpo.dev/docs/errors/ai.settings_invalid

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`models:` without `default` is the same error:

```jadpo check project=e-ai-settings-invalid-default file=config.jadpo hidden
config {
    drafting_model: ModelId {
        default: "meta-llama/Llama-3.1-8B-Instruct"
    }
    model_url: Url {
        default: "http://localhost:8000/v1"
    }
}
```

```jadpo check project=e-ai-settings-invalid-default file=app.jadpo expect=ai.settings_invalid
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
            drafting: config.drafting_model
        }
        limits: {
            per_day: {
                tokens: 2000000
            }
        }
    }
}
```

```text output
error: `models:` has no `default:`
 --> app.jadpo:10:9
   |
10 |         models: {
   |         ^^^^^^^^^ no `default:`
  reason: `default` is the model a call asks when it names none with `model:`, so every application that asks a model names one.
  next: Add `default: config.<name>`, a `ModelId` setting (agent)
  rule: ai.settings_invalid
  help: https://jadpo.dev/docs/errors/ai.settings_invalid

check failed: 1 error, 2 files
```

## How to fix it

**If the setting holds a model**, declare it as a `ModelId`. In
`config.jadpo`:

```jadpo fragment
config {
    drafting_model: ModelId {
        default: "meta-llama/Llama-3.1-8B-Instruct"
    }
    ...
}
```

```jadpo check project=e-ai-settings-invalid-fix-1 file=config.jadpo hidden
config {
    drafting_model: ModelId {
        default: "meta-llama/Llama-3.1-8B-Instruct"
    }
    model_url: Url {
        default: "http://localhost:8000/v1"
    }
}
```

```jadpo check project=e-ai-settings-invalid-fix-1 file=app.jadpo hidden migrate=initial
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

A call that names no model now asks `meta-llama/Llama-3.1-8B-Instruct`,
and a value for `drafting_model` that doesn't name a provider and a
model, such as `llama`, stops the app at start.

**If `models:` has no `default`**, name the model a call asks when it
names none `default`. In `app.jadpo`:

```jadpo fragment
        models: {
            default: config.drafting_model
        }
```

```jadpo check project=e-ai-settings-invalid-fix-2 file=app.jadpo hidden
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

```jadpo check project=e-ai-settings-invalid-fix-2 file=config.jadpo hidden migrate=initial
config {
    drafting_model: ModelId {
        default: "meta-llama/Llama-3.1-8B-Instruct"
    }
    model_url: Url {
        default: "http://localhost:8000/v1"
    }
}
```

Every call that doesn't choose a model now asks the drafting model.

## Related

- [Connecting a provider](../language/21-ai.md#connecting-a-provider)
- [A model on this machine](../language/21-ai.md#a-model-on-this-machine)
- [Settings](../language/10-configuration-and-time.md#settings-configjadpo)
