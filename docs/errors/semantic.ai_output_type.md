# `semantic.ai_output_type`

The type a model is asked to answer as holds something a model's answer can't be.

## Why Jadpo refuses it

A model answers in text. Its answer is JSON decoded into the type you ask
for, so a type holding bytes, a `File`, `PresentationText` or a reference
to a record could never be filled in. Anything in the type, however deep,
counts.

## What it looks like

The bakers' `CakeDesign` includes a picture, and the action asks a model
for one:

```jadpo check project=e-ai-output-type file=app.jadpo hidden
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

```jadpo check project=e-ai-output-type file=config.jadpo hidden migrate=initial
config {
    drafting_model: ModelId {
        default: "meta-llama/Llama-3.1-8B-Instruct"
    }
    model_url: Url {
        default: "http://localhost:8000/v1"
    }
}
```

```jadpo check project=e-ai-output-type file=domains/bakery/types/cake_brief.jadpo hidden
/// What a customer wants their cake to look like.
type CakeBrief = Text {
    max_length: 1000
}
```

domains/bakery/types/cake_design.jadpo:


```jadpo check project=e-ai-output-type file=domains/bakery/types/cake_design.jadpo
/// A cake a customer described, as the bakers make it.
type CakeDesign = Object {
    /// What the bakers call it.
    title: Text
    /// A picture of it.
    preview: Bytes
}
```

```jadpo check project=e-ai-output-type file=domains/bakery/failures/designer_unavailable.jadpo hidden
/// No design can be drafted now.
type DesignerUnavailable = Failure {
    kind: FailureKind.unavailable
}
```

```jadpo check project=e-ai-output-type file=domains/bakery/prompts/design_cake.jadpo hidden
use bakery/types { CakeBrief }

/// Drafts a cake from a customer's brief.
type DesignCake = Prompt {
    brief: CakeBrief
}
```

```jadpo check project=e-ai-output-type file=domains/bakery/prompts/design_cake.prompt hidden
# Instructions

You draft cakes for a bakery from what a customer asks for.

# Request

{brief}
```

domains/bakery/actions/draft_cake.jadpo:


```jadpo check project=e-ai-output-type file=domains/bakery/actions/draft_cake.jadpo expect=semantic.ai_output_type
use bakery/types { CakeBrief, CakeDesign }
use bakery/failures { DesignerUnavailable }
use bakery/prompts { DesignCake }

/// Drafts a cake from a customer's brief.
action draft_cake(brief: CakeBrief) fails DesignerUnavailable -> CakeDesign {
    return attempt ai.generate(
        DesignCake {
            brief: brief
        },
        as: CakeDesign
    ) {
        refused: DesignerUnavailable
        malformed: DesignerUnavailable
        timed_out: DesignerUnavailable
        over_budget: DesignerUnavailable
        too_long: DesignerUnavailable
    }
}
```

```text output
error: A model can't answer as `CakeDesign`
 --> domains/bakery/actions/draft_cake.jadpo:11:13
   |
11 |         as: CakeDesign
   |             ^^^^^^^^^^ `CakeDesign.preview` is bytes
  reason: A model's answer is JSON decoded into `as:`'s type, and `CakeDesign.preview` is bytes, which a model's JSON can't be. A model answers in text.
  next: Ask for a type without `CakeDesign.preview` (agent)
  rule: semantic.ai_output_type
  help: https://jadpo.dev/docs/errors/semantic.ai_output_type

check failed: 1 error, 7 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the model only needs to answer the part it can write**, ask for a
type without the other part:

```jadpo fragment
type CakeDesign = Object {
    /// What the bakers call it.
    title: Text
}
```

```jadpo check project=e-ai-output-type-fix file=domains/bakery/types/cake_design.jadpo hidden
/// A cake a customer described, as the bakers make it.
type CakeDesign = Object {
    /// What the bakers call it.
    title: Text
}
```

```jadpo check project=e-ai-output-type-fix file=app.jadpo hidden
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

```jadpo check project=e-ai-output-type-fix file=config.jadpo hidden
config {
    drafting_model: ModelId {
        default: "meta-llama/Llama-3.1-8B-Instruct"
    }
    model_url: Url {
        default: "http://localhost:8000/v1"
    }
}
```

```jadpo check project=e-ai-output-type-fix file=domains/bakery/types/cake_brief.jadpo hidden
/// What a customer wants their cake to look like.
type CakeBrief = Text {
    max_length: 1000
}
```

```jadpo check project=e-ai-output-type-fix file=domains/bakery/failures/designer_unavailable.jadpo hidden
/// No design can be drafted now.
type DesignerUnavailable = Failure {
    kind: FailureKind.unavailable
}
```

```jadpo check project=e-ai-output-type-fix file=domains/bakery/prompts/design_cake.jadpo hidden
use bakery/types { CakeBrief }

/// Drafts a cake from a customer's brief.
type DesignCake = Prompt {
    brief: CakeBrief
}
```

```jadpo check project=e-ai-output-type-fix file=domains/bakery/prompts/design_cake.prompt hidden
# Instructions

You draft cakes for a bakery from what a customer asks for.

# Request

{brief}
```

```jadpo check project=e-ai-output-type-fix file=domains/bakery/actions/draft_cake.jadpo hidden migrate=initial
use bakery/types { CakeBrief, CakeDesign }
use bakery/failures { DesignerUnavailable }
use bakery/prompts { DesignCake }

/// Drafts a cake from a customer's brief.
action draft_cake(brief: CakeBrief) fails DesignerUnavailable -> CakeDesign {
    return attempt ai.generate(
        DesignCake {
            brief: brief
        },
        as: CakeDesign
    ) {
        refused: DesignerUnavailable
        malformed: DesignerUnavailable
        timed_out: DesignerUnavailable
        over_budget: DesignerUnavailable
        too_long: DesignerUnavailable
    }
}
```

`ai.generate` now decodes the answer into `CakeDesign`. A picture has to
come from somewhere else and be stored by your own code.

## Related

- [Typed output](../language/21-ai.md#typed-output)
