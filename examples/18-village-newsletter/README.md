# 18 · A village newsletter

A Pembrokeshire village's newsletter, in English and Welsh: articles
written in Markdown, address labels for the printed copies, share links,
each edition's colours, readers' photos, subscriptions confirmed by
link or by phone, and the editor's notices. It teaches text and the web:
the text operations, text formats that are safe to put on a page,
colours, links and the encodings that write bytes as text.

Text is counted, cut and padded in code points, so an accent is one
character. Markdown and rich text become `Html` that is made safe as it
is built, so nothing a reader sends can run in another reader's browser.
Links are built from their parts, never glued from strings, and every
value that leaves as text is written by its type.

## What it teaches

1. **Text operations.** Trimming, replacing, padding, slugs, the first characters for a preview, splitting into words and joining them again, case, and testing what text starts with, ends with or contains. See `domains/newsletter/actions/publish_article.jadpo`, `domains/newsletter/actions/label_address.jadpo` and [area 19, Text](../../docs/language/19-standard-library.md#text).
2. **Text formats.** `Markdown` becomes `Html` with `markup.to_html`, raw HTML in it kept only where the sanitiser keeps it; `markup.to_plain_text` gives the words, and a `RichText` from an editor becomes HTML too. See `domains/newsletter/actions/publish_article.jadpo`, `domains/newsletter/actions/notice_box.jadpo` and [area 19, Text formats](../../docs/language/19-standard-library.md#text-formats).
3. **Text in each language.** A `LocalisedText` holds a village's name in English and Welsh, and `text.pick` takes the reader's. See `domains/newsletter/actions/local_village_name.jadpo`.
4. **Links.** `web.with_query` adds a query with its values encoded, `web.resolve` follows a relative link, and `encoding.to_url_component` encodes a `mailto:` subject. See `domains/newsletter/actions/make_share_links.jadpo` and [area 19, Web and software](../../docs/language/19-standard-library.md#web-and-software).
5. **Colours.** OKLCH and HSL to sRGB, a lighter shade, a tint mixed with white, and WCAG's contrast ratio for readable text. See `domains/newsletter/actions/edition_theme.jadpo` and [area 19, Colour](../../docs/language/19-standard-library.md#colour).
6. **Images and networks.** `media.fit` scales a photo into a tile without stretching it, and `web.cidr_contains` says whether an address is on the village hall's network. See `domains/newsletter/actions/photo_tile.jadpo` and `domains/newsletter/actions/editor_in_hall.jadpo`.
7. **Tokens, digests and encodings.** `random.token` and `random.code` make a link's token and a code to read out; the newsletter keeps the token's SHA-256 in hexadecimal and compares it, and an unsubscribe key is the address in Base64url, read back with `encoding.from_base64url`. See `domains/newsletter/actions/subscribe.jadpo`, `domains/newsletter/actions/check_confirmation.jadpo`, `domains/newsletter/actions/unsubscribe_address.jadpo`, `domains/newsletter/actions/page_etag.jadpo` and [area 19, Encodings](../../docs/language/19-standard-library.md#encodings).

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/18-village-newsletter
target/release/jadpo test examples/18-village-newsletter
target/release/jadpo fmt --check examples/18-village-newsletter
```

- `check` ends with `checked: <n> files` and no errors.
- `test` ends with `tested: 12 passed, 12 action tests`.

## Try next

- In `publish_article.jadpo`, pad the issue number with `fill: "00"`. `jadpo check` reports `type.mismatch`: the fill is one character, written out.
  <!-- try {"file": "domains/newsletter/actions/publish_article.jadpo", "replace": "fill: \"0\"", "with": "fill: \"00\"", "check": ["type.mismatch"]} -->
- In `label_address.jadpo`, start the inward half at `text.length(outward_half)` itself. `jadpo check` reports `type.argument_negative`: a position is proved never negative by its type, and a plain `Int` isn't; and `semantic.unused_variable` for `inward_from`, now unused.
  <!-- try {"file": "domains/newsletter/actions/label_address.jadpo", "replace": "text.slice(tidy, inward_from, 3)", "with": "text.slice(tidy, text.length(outward_half), 3)", "check": ["semantic.unused_variable", "type.argument_negative"]} -->
- In `subscribe.jadpo`, keep the token itself as its digest. The subscription's test fails with `test.assertion_failed`: a digest is 64 hexadecimal digits, and a token isn't.
  <!-- try {"file": "domains/newsletter/actions/subscribe.jadpo", "replace": "TokenDigest(encoding.to_hex(hash.sha256(token)))", "with": "TokenDigest(token)", "test": ["test.assertion_failed"], "failed": 1} -->
- Then read [19-book-club](../19-book-club/README.md), which works with lists, sets and maps.
