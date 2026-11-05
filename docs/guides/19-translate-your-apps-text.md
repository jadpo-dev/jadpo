# Translate your app's text
<!-- topic: guide-translate-your-apps-text -->

Text your server writes for people, such as a page's headline or a failure's wording,
is translated in Jadpo itself. Each message is a type, each language is a file of
wordings, and the compiler checks every call, every placeholder and every plural form
against them, so a missing translation is a compile error, not a blank on a French
customer's screen. This guide puts a parcel-tracking headline into English and French.

You start from an app with an `Order` entity holding how many parcels an order has,
and a `POST /orders` route to place one. [Build an API](01-build-an-api.md) shows how
to write those. The first steps need neither: they word one message and test it.

```jadpo check project=guide-translations file=domains/shipping/entities/order.jadpo hidden
/// An order on its way to a customer.
entity Order {
    id: Uuid
    parcel_count: Int {
        min: 1
        max: 100
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
```

```jadpo check project=guide-translations file=domains/shipping/types/new_order.jadpo hidden
use shipping/entities { Order }

/// What a caller sends to place an order.
type NewOrder = Input {
    parcel_count: Order.parcel_count
}
```

```jadpo check project=guide-translations file=domains/shipping/types/order_placed.jadpo hidden
use shipping/entities { Order }

/// An order, once it is placed.
type OrderPlaced = Output {
    pick: [Order.id]
}
```

```jadpo check project=guide-translations file=domains/shipping/actions/place_order.jadpo hidden
use shipping/entities { Order }
use shipping/types { NewOrder, OrderPlaced }

/// Places an order.
action place_order(input: NewOrder) -> OrderPlaced {
    var order: Order = Order.create {
        set: {
            parcel_count: input.parcel_count
        }
    }
    return OrderPlaced {
        id: order.id
    }
}
```

## List your languages

`locales` names the languages your app supports, and the one to use when no other
is chosen. A locale is written `Locale.<tag>`, in lower case with `_` for `-`, so
`Locale.fr_fr` is the French of France.

`app.jadpo`:

```jadpo check project=guide-translations file=app.jadpo
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

`locales` and `Locale` are built into Jadpo. Start with the one language, and add French
once the message works.

## Declare a message

A message is a type built on `Message`, one to a file in the domain's `translations/`
folder. Its fields are the wording's placeholders, and its `///` comment reaches
translators as context. `PluralCount` is a built-in whole number that picks the plural
form.

`domains/shipping/translations/parcels_on_the_way.jadpo`:

```jadpo check project=guide-translations file=domains/shipping/translations/parcels_on_the_way.jadpo
/// How many of a customer's parcels are on their way, on the tracking page.
type ParcelsOnTheWay = Message {
    count: PluralCount
}
```

## Word it

Each language is a file named after its locale. Each entry takes the message's name in
lower_snake_case. A field of the message becomes a `{field}` placeholder, and a
`PluralCount` gives one member per form that language needs: English has `one` and
`other`. Leave one out and `jadpo check` says which.

`domains/shipping/translations/en_gb.jadpo`:

```jadpo check project=guide-translations file=domains/shipping/translations/en_gb.jadpo
translations {
    parcels_on_the_way: {
        one: "{count} parcel is on its way"
        other: "{count} parcels are on their way"
    }
}
```

The compiler generates one call for each message, `translations.<domain>.<message>(...)`,
which takes every field by name, plus `locale:`. A test can call it straight away.

`domains/shipping/translations/parcels_on_the_way.test.jadpo`:

```jadpo check project=guide-translations file=domains/shipping/translations/parcels_on_the_way.test.jadpo
test "one parcel and many read differently" {
    assert translations.shipping.parcels_on_the_way(
        count: 1,
        locale: Locale.en_gb
    ) == "1 parcel is on its way"
    assert translations.shipping.parcels_on_the_way(
        count: 2,
        locale: Locale.en_gb
    ) == "2 parcels are on their way"
}
```

## Add a language

Adding French starts in `locales`, which names the languages your app supports, and
then needs a file of French wording. French has three plural forms: `one`, `many` and
`other`.

`app.jadpo`, changed. `Locale.fr_fr` is new:

```jadpo check project=guide-translations file=app.jadpo
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

`order_not_found` below is the wording of a failure, which needs the same care:
each failure a route can answer with is worded in every language. An `Order` that
isn't there is one, so English needs it too.

`domains/shipping/translations/en_gb.jadpo`, changed. `order_not_found` is new:

```jadpo check project=guide-translations file=domains/shipping/translations/en_gb.jadpo
translations {
    order_not_found: "We can't find that order."
    parcels_on_the_way: {
        one: "{count} parcel is on its way"
        other: "{count} parcels are on their way"
    }
}
```

`domains/shipping/translations/fr_fr.jadpo`:

```jadpo check project=guide-translations file=domains/shipping/translations/fr_fr.jadpo
translations {
    order_not_found: "Nous ne trouvons pas cette commande."
    parcels_on_the_way: {
        one: "{count} colis est en route"
        many: "{count} de colis sont en route"
        other: "{count} colis sont en route"
    }
}
```

## Word Jadpo's own messages

Jadpo ships its own wording, such as "Too many requests", in `en_gb` and `en_us` only.
For any other language `jadpo check` fails with `translation.jadpo_missing` until your
app words them. This lists them, with the English to translate from:

```sh
jadpo translations builtin
```

```text
Requests
  invalid_request         needs fr_fr
    The request couldn't be read. Try again.
  invalid_input           needs fr_fr
    Some of the information sent isn't valid. Check it and try again.
  route_not_found         needs fr_fr
    We couldn't find what you were looking for.
  method_not_allowed      needs fr_fr
    That isn't something this address can do.
  request_too_large       needs fr_fr
    What was sent is too big. Send less and try again.
  idempotency_key_reused  needs fr_fr
    That request was already sent with different details. Start again.
  rate_limited            needs fr_fr
    Too many requests. Wait a moment, then try again.
  unavailable             needs fr_fr
    A service this depends on isn't available right now. Try again soon.
  internal_fault          needs fr_fr
    Something went wrong on our side. Try again soon.
  deadline_exceeded       needs fr_fr
    That took too long, and nothing was saved. Try again.

Locales
  text_direction
    left_to_right  ltr
    right_to_left  rtl

11 messages: 10 need wording, 2 features

next: jadpo check  fails until the application words them
```

They go under `jadpo:` in a file in the root `translations/` folder, which holds only
your app's wording for text it didn't write. `text_direction` needs no wording.

`translations/fr_fr.jadpo`:

```jadpo check project=guide-translations file=translations/fr_fr.jadpo
translations {
    jadpo: {
        deadline_exceeded: "Cela a pris trop de temps, et rien n'a été enregistré. Réessayez."
        idempotency_key_reused: "Cette demande a déjà été envoyée avec d'autres détails. Recommencez."
        internal_fault: "Un problème est survenu de notre côté. Réessayez bientôt."
        invalid_input: "Certaines informations envoyées ne sont pas valides. Vérifiez-les et réessayez."
        invalid_request: "La demande n'a pas pu être lue. Réessayez."
        method_not_allowed: "Cette adresse ne permet pas de faire cela."
        rate_limited: "Trop de demandes. Attendez un instant, puis réessayez."
        request_too_large: "Ce qui a été envoyé est trop volumineux. Envoyez moins et réessayez."
        route_not_found: "Nous n'avons pas trouvé ce que vous cherchiez."
        unavailable: "Un service dont cela dépend n'est pas disponible pour le moment. Réessayez bientôt."
    }
}
```

## Use the message

A message's call returns `PresentationText`, so the result can be shown but never
stored.

A browser says which languages its reader wants in its `Accept-Language` header.
`locale.from_accept_language` gives the best of your supported locales, or `default:`
when none match, and it never fails. A language the request names in its own input
would be a `Locale` field instead.

`domains/shipping/types/browser_languages.jadpo`:

```jadpo check project=guide-translations file=domains/shipping/types/browser_languages.jadpo
/// What a browser says its reader's languages are.
type BrowserLanguages = Text {
    max_length: 2000
}
```

`domains/shipping/types/tracking_request.jadpo`:

```jadpo check project=guide-translations file=domains/shipping/types/tracking_request.jadpo
use shipping/types { BrowserLanguages }

/// What the tracking page asks with.
type TrackingRequest = Input {
    languages: BrowserLanguages {
        header: "Accept-Language"
        optional: true
    }
}
```

`domains/shipping/types/tracking_page.jadpo`:

```jadpo check project=guide-translations file=domains/shipping/types/tracking_page.jadpo
/// What the tracking page shows.
type TrackingPage = Output {
    headline: PresentationText
}
```

`domains/shipping/actions/show_tracking.jadpo`:

```jadpo check project=guide-translations file=domains/shipping/actions/show_tracking.jadpo
use shipping/entities { Order, OrderNotFound }
use shipping/types { TrackingPage, TrackingRequest }

/// The tracking page's text, in the reader's language.
action show_tracking(
    order_id: Order.id,
    asked: TrackingRequest
) fails OrderNotFound -> TrackingPage {
    var order: Order = attempt Order.get(order_id)
    var reader_locale: Locale = locale.from_accept_language(asked.languages)
    return TrackingPage {
        headline: translations.shipping.parcels_on_the_way(
            count: order.parcel_count,
            locale: reader_locale
        )
    }
}
```

`domains/shipping/routes/orders.jadpo`, with the new route and imports:

```jadpo check project=guide-translations file=domains/shipping/routes/orders.jadpo
use shipping/entities { Order }
use shipping/types { NewOrder, OrderPlaced, TrackingPage, TrackingRequest }
use shipping/actions { place_order, show_tracking }

/// Where customers follow their orders.
routes Orders {
    prefix: /orders

    /// Places an order.
    POST / {
        authentication: RouteAuthentication.none
        input: NewOrder
        output: OrderPlaced
        status: SuccessStatus.created
        run: place_order(input)
    }

    /// An order's tracking page.
    GET /{order_id: Order.id}/tracking {
        authentication: RouteAuthentication.none
        input: TrackingRequest
        output: TrackingPage
        run: show_tracking(order_id, input)
    }
}
```

## Test the page

`domains/shipping/actions/show_tracking.test.jadpo`:

```jadpo check project=guide-translations file=domains/shipping/actions/show_tracking.test.jadpo
use shipping/types { NewOrder, OrderPlaced, TrackingPage, TrackingRequest }
use shipping/actions { place_order }

test "the page is in the language the browser asks for" {
    var order: OrderPlaced = place_order(
        NewOrder {
            parcel_count: 2
        }
    )
    var page: TrackingPage = attempt show_tracking(
        order.id,
        TrackingRequest {
            languages: "fr-FR,fr;q=0.9"
        }
    )
    assert page.headline == "2 colis sont en route"
}

test "a language the app doesn't have falls back on the default" {
    var order: OrderPlaced = place_order(
        NewOrder {
            parcel_count: 1
        }
    )
    var page: TrackingPage = attempt show_tracking(
        order.id,
        TrackingRequest {
            languages: "ja"
        }
    )
    assert page.headline == "1 parcel is on its way"
}
```

```sh
jadpo test
```

```text
tested: 3 passed, 1 pure test, 2 action tests
```

## Try it

Run `jadpo dev`, place an order with two parcels, then ask for its tracking page with
the id you got. With no header, you get the default language:

```sh
curl http://localhost:3000/orders/01a1a65c-e939-7db9-a6b9-1a29cd430671/tracking
```

```text
{"headline":"2 parcels are on their way"}
```

```sh
curl -H 'Accept-Language: fr-FR,fr;q=0.9' http://localhost:3000/orders/01a1a65c-e939-7db9-a6b9-1a29cd430671/tracking
```

```text
{"headline":"2 colis sont en route"}
```

## Let client apps show failures

An error's `message` in a response is always the short English text. A client app
shows its reader the wording for the error's `code`, and `jadpo check` writes that for
each language beside the OpenAPI document:

```sh
curl -H 'Accept-Language: fr' http://localhost:3000/orders/00000000-0000-7000-8000-000000000000/tracking
```

```text
{"error":{"code":"order_not_found","message":"Not found.","request_id":"req_d5cf3d967d337471"}}
```

`build/openapi/main.failures.fr_fr.json` (written by `jadpo check`) holds, among the
others:

```text
  "order_not_found": {
    "formats": {},
    "text": "Nous ne trouvons pas cette commande."
  },
```

## Keep translations up to date

When someone changes the English, the French may no longer say the same thing. Jadpo
catches that once you record what each translation was made from:

```sh
jadpo translations update
```

```text
Wrote domains/shipping/translations/sources.jadpo_record
    fr_fr.order_not_found     recorded
    fr_fr.parcels_on_the_way  recorded

next: git diff  shows what was recorded
```

Commit the record, which `jadpo translations update` writes for each folder and nobody
edits by hand. Now change "{count} parcel is on its way" in `en_gb.jadpo`, and
`jadpo check` stops with `translation.stale`:

```text
error: `parcels_on_the_way` in `fr_fr` was translated from `en_gb` wording that has since changed
 --> domains/shipping/translations/fr_fr.jadpo:3:5
  |
3 |     parcels_on_the_way: {
  |     ^^^^^^^^^^^^^^^^^^ made from the old `en_gb` wording
  reason: `jadpo translations update` recorded the `en_gb` wording this message's translation was made from, and the `en_gb` wording has changed since, so the translation may no longer say the same thing.
  next: Reword `parcels_on_the_way` in `domains/shipping/translations/fr_fr.jadpo` to match the new `en_gb` wording, then run `jadpo translations update` (agent)
```

`jadpo translations missing` lists everything outstanding, by language file:

```sh
jadpo translations missing
```

```text
domains/shipping/translations/fr_fr.jadpo
  parcels_on_the_way  stale       en_gb reworded since it was translated

1 outstanding: 1 stale, 1 locale
```

Reword the French and run `jadpo translations update` again, and the check passes.

## A language that isn't finished

A language still being translated is listed in `supported:` and also in
`in_progress:`, as `in_progress: [Locale.pl_pl]`, in `locales`. It needs no wording for
every message: what it lacks falls back on the default language, and
`jadpo translations missing` lists what is left. Take it out of `in_progress:` once it is
done.

An enum's values can have wording of their own, so a template or message shows them in the
reader's language. Emails use these same messages, written without `locale:`, as
[Email](../language/16-built-in-services.md#email) shows. To send wording to a translation service and
bring it back, see
[Exchanging translations with a service](../language/10-configuration-and-time.md#exchanging-translations-with-a-service).

## Read more

[Locales](../language/10-configuration-and-time.md#locales),
[Translations as messages](../language/10-configuration-and-time.md#translations-as-messages),
[Enums with wording](../language/10-configuration-and-time.md#enums-with-wording),
[Tooling for translations](../language/10-configuration-and-time.md#tooling-for-translations) and
[Failures for client apps](../language/10-configuration-and-time.md#failures-for-client-apps).

## Not built yet

Moving a message to `shared/translations/` isn't built yet as a promotion, so
when a second domain needs a message, move it there by hand.
