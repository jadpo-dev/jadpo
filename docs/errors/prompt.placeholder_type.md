# `prompt.placeholder_type`

A prompt's field is of a type a prompt can't send, such as bytes, a file, a secret or a value that may be `none`.

## Why Jadpo refuses it

A prompt is text: its fields are written into the instructions or the
request, as text or as JSON, and sent to a model. Bytes and files aren't
text: images, audio and other bytes are sent by a provider package's own
operations, and a `File` is only the store and key its bytes are kept
under. A secret, such as an `ApiKey`, must never leave the app, and a
model's provider is outside it. A value that may be `none` has no text
to write, and a prompt's text has no conditions. A conversation is sent
as the turns before the request, so a second one, or one written as a
placeholder, has nowhere to go. Jadpo refuses each of these, however
deep in an object the field sits, rather than send something other than
what the reviewer reads.

## What it looks like

```jadpo check project=e-prompt-placeholder-type file=app.jadpo hidden
application {}
```

domains/support/prompts/describe_photo.prompt:

```jadpo check project=e-prompt-placeholder-type file=domains/support/prompts/describe_photo.prompt
# Instructions

Describe what the photo shows, in the language of {reader_locale}.

# Request

What is wrong with the order in the photo?
```

domains/support/prompts/describe_photo.jadpo:

```jadpo check project=e-prompt-placeholder-type file=domains/support/prompts/describe_photo.jadpo expect=prompt.placeholder_type
/// Describes the photo a customer sent of their order.
type DescribePhoto = Prompt {
    photo: Bytes
    reader_locale: Locale
}
```

```text output
error: `DescribePhoto.photo` is `Bytes`, which a prompt can't send
 --> domains/support/prompts/describe_photo.jadpo:3:5
  |
3 |     photo: Bytes
  |     ^^^^^ not text
  reason: A prompt is text. Images, audio and other bytes are sent by a provider package's own operations, not in a prompt, and a `File`, even inside an object, is only the store and key its bytes are under.
  next: Remove `photo`, or send what it holds as text (agent)
  rule: prompt.placeholder_type
  help: https://jadpo.dev/docs/errors/prompt.placeholder_type

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the model can work from a description of the photo**, send that
text instead of the bytes, under `# Request`, since it is free text. In
`domains/support/prompts/describe_photo.jadpo`:

```jadpo fragment
type DescribePhoto = Prompt {
    photo_description: Text
    ...
}
```

```jadpo check project=e-prompt-placeholder-type-fix-1 file=domains/support/prompts/describe_photo.jadpo hidden
/// Describes the photo a customer sent of their order.
type DescribePhoto = Prompt {
    photo_description: Text
    reader_locale: Locale
}
```

and in `domains/support/prompts/describe_photo.prompt`:

```text
{photo_description}
```

```jadpo check project=e-prompt-placeholder-type-fix-1 file=domains/support/prompts/describe_photo.prompt hidden
# Instructions

Describe what the photo shows, in the language of {reader_locale}.

# Request

What is wrong with the order in the photo?

{photo_description}
```

```jadpo check project=e-prompt-placeholder-type-fix-1 file=app.jadpo hidden
application {}
```

The model is now sent the description as a data block tagged
`<photo_description>`, after the question.

**If the photo itself must reach a model**, remove the field from the
prompt, and send the bytes through the provider package's own operation
for images. In `domains/support/prompts/describe_photo.jadpo`:

```jadpo fragment
type DescribePhoto = Prompt {
    reader_locale: Locale
}
```

```jadpo check project=e-prompt-placeholder-type-fix-2 file=domains/support/prompts/describe_photo.jadpo hidden
/// Describes the photo a customer sent of their order.
type DescribePhoto = Prompt {
    reader_locale: Locale
}
```

```jadpo check project=e-prompt-placeholder-type-fix-2 file=domains/support/prompts/describe_photo.prompt hidden
# Instructions

Describe what the photo shows, in the language of {reader_locale}.

# Request

What is wrong with the order in the photo?
```

```jadpo check project=e-prompt-placeholder-type-fix-2 file=app.jadpo hidden
application {}
```

The prompt now sends only text, and code that builds a `DescribePhoto`
stops giving it the photo.

## Related

- [Prompts as files](../language/21-ai.md#prompts-as-files)
- [Secret values](../language/19-standard-library.md#secret-values)
- [`prompt.placeholder_unused`](../errors/prompt.placeholder_unused.md)
