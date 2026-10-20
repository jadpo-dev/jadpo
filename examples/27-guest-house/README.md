# 27 · A guest house

A guest house in Snowdonia takes bookings for its rooms and confirms
each one by email, in English or in Welsh, in the language the guest
chose. It teaches email: an email as a type, its HTML and text written
beside it and checked against its fields, its subject worded like any
message, a layout that wraps every email, and sending as emitting, so
only a booking that was made is ever confirmed.

An email's words are templates: HTML and plain text with Jadpo inside
`{{ }}`. They read the email's fields, translate and format, and
nothing else, so what an email says is decided where it is sent, and a
reviewer reads its words in one place.

## What it teaches

1. **An email type.** `type X = EmailMessage { ... }` in a domain's `emails/` folder: its fields are what its templates write, and every email has `to:` and `locale:`. See `domains/stays/emails/booking_confirmed.jadpo` and [area 16, Email](../../docs/language/16-built-in-services.md#email).
2. **Templates.** `booking_confirmed.html` and `booking_confirmed.txt` sit beside the type. `{{ value }}` writes a field, escaped in HTML; `translations.stays.<message>(...)` and `text.format(...)` write in the email's language without `locale:`; `{{ if (...) }} ... {{ else }} ... {{ end }}` and `{{ for item in list }} ... {{ end }}` choose and repeat; an enum with wording writes its wording. Anything else is a compile error. See `domains/stays/emails/booking_confirmed.html` and `domains/stays/emails/booking_confirmed.txt`.
3. **The subject.** An email's member in the domain's language files is its subject line, its fields its placeholders, worded in every supported language. See `domains/stays/translations/en_gb.jadpo` and `domains/stays/translations/cy_gb.jadpo`.
4. **The layout.** `shared/emails/layout.html` and `layout.txt` wrap every email: `{{ content }}` is the email's own text, `{{ subject }}` its subject, `{{ locale.direction }}` `ltr` or `rtl`, and `{{ app.display_name }}` the application's name. See `shared/emails/layout.html` and [area 16, Level 3: branding](../../docs/language/16-built-in-services.md#built-in-emails).
5. **Sending is emitting.** `emit BookingConfirmed { ... }` keeps the email with the booking, and it is sent once the booking commits, rendered whole as it is sent. See `domains/stays/actions/book_stay.jadpo`.
6. **Where it goes.** `email:` in `app.jadpo` names the connection every email goes through and who it is from; here an SMTP server, its password a secret setting given only to the connection. Changing provider changes `app.jadpo` alone. See `app.jadpo`, `config.jadpo` and [area 16, The SMTP driver](../../docs/language/16-built-in-services.md#the-smtp-driver).
7. **Tests catch email.** A test sends nothing: each email is rendered once its booking commits and caught, so a template that couldn't be rendered fails the test. See `domains/stays/actions/book_stay.test.jadpo`.

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/27-guest-house
target/release/jadpo test examples/27-guest-house
target/release/jadpo fmt --check examples/27-guest-house
```

- `check` ends with `checked: 18 files` and no errors.
- `test` needs PostgreSQL, and ends with `tested: 1 passed, 1 action test`.

## Try next

- In `booking_confirmed.html`, write the arrival date as it is, `{{ arrives_on }}`. `jadpo check` reports `email.template_value_invalid`: a date is shown to a guest formatted, in their language, with `text.format`.
  <!-- try {"file": "domains/stays/emails/booking_confirmed.html", "replace": "<p>{{ room }}</p>", "with": "<p>{{ room }}, {{ arrives_on }}</p>", "check": ["email.template_value_invalid"]} -->
- In `booking_confirmed.txt`, shout the guest's name with `text.upper(guest_name)`. `jadpo check` reports `email.template_not_allowed`: a template only reads its fields, translates and formats, and anything else is worked out where the email is sent.
  <!-- try {"file": "domains/stays/emails/booking_confirmed.txt", "replace": "{{ room }}", "with": "{{ text.upper(guest_name) }}", "check": ["email.template_not_allowed"]} -->
- In `cy_gb.jadpo`, leave out the subject, `booking_confirmed`. `jadpo check` reports `translation.wording_missing`: an email has a subject in every language the guest house writes.
  <!-- try {"file": "domains/stays/translations/cy_gb.jadpo", "replace": "    booking_confirmed: \"Eich arhosiad yn Nhŷ Gwyn, {guest_name}\"\n", "with": "", "check": ["translation.wording_missing"]} -->
- That is the last example so far. The [language pages](../../docs/language/README.md) hold the full specification.
