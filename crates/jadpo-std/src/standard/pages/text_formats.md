# Markdown

Text written in Markdown, the plain-text format for bold, headings and lists.

Use it for content people write. `markup.to_html` turns it into `Html` to show. For
structured documents, use `RichText`. For plain text, use `Text`.

```jadpo
Markdown("Fresh **soup** daily.")
```

- `text.format` shows it as plain text, with the marks removed.

# Html

HTML that's always safe to put in a page or an email.

Use it for formatted content you send to a browser. Building one from text keeps only
safe elements and removes the rest, so a script can't get in. For the writer's own text,
store `Markdown` or `RichText`, and convert it when you show it.

```jadpo
Html("<p>Fresh <strong>soup</strong> daily.</p>")
```

- Building one from text never fails. Unsafe parts are removed instead, and links keep
  only `https:` and `mailto:`.
- `markup.to_plain_text` gives the text without its tags.

# RichText

A structured document: a list of paragraphs, headings, quotes and list items.

Use it to hold content from a block editor. `markup.rich_to_html` gives `Html`. For text
someone types by hand, use `Markdown`.

```jadpo
var example: RichText = [
    RichTextBlock.paragraph {
        spans: [
            RichTextSpan { text: "Fresh soup" marks: [] link: none }
        ]
    }
]
assert example == example
```

- It is flat: nothing nests inside anything else.

# LocalisedText

The same text in several languages: `{"en-GB": "Rye loaf", "fr-FR": "Pain de seigle"}`.

Use it for content people translate, such as product names. `text.pick` gets the entry
for a locale. For your app's own wording, use its translations instead.

```jadpo
type NotATranslation = Failure {
    kind: FailureKind.invalid_value
    message: "That isn't a translated name."
}

var example: LocalisedText = attempt json.read(
    "{\"en-GB\":\"Rye loaf\",\"fr-FR\":\"Pain de seigle\"}",
    as: LocalisedText
) {
    invalid: NotATranslation
}
assert example == example
```

- It arrives as JSON, from a request or from `json.read`, since code can't write out
  a map with entries.

# RichTextBlock

One block of a `RichText`: a paragraph, a heading, a quote or a list item.

Use it when you build a `RichText` in code. To hold a document, use `RichText`.

```jadpo
RichTextBlock.heading {
    level: 2
    spans: [
        RichTextSpan { text: "Soups" marks: [] link: none }
    ]
}
```

- A heading's level runs from 1 to 6, and a list item's from 1 to 9.

# RichTextSpan

A run of text in a block, with its marks and any link.

Use it when you build a `RichTextBlock` in code.

```jadpo
RichTextSpan { text: "today's soup" marks: [RichTextMark.strong] link: none }
```

- It can't be empty. A link must be `https:` with a host, or `mailto:`.

# RichTextMark

A mark on some text: `strong`, `emphasis`, `code`, `underline` or `strike_through`.

Use it as part of a `RichTextSpan`.

```jadpo
RichTextMark.strong
```
