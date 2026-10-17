# Package scenarios in code

Status: research, 2026-10-16, for
[packages and extension points](packages-and-extension-proposals.md).
Sketches of the decided design on the realistic package list, to see
whether it holds. Syntax is loose on purpose (types and imports mostly
left out); the shape is what matters. Findings at the end.

## 1. Call an API: post to Slack

```text
// packages/slack/v1/resources/chat.jadpo
resource chat {
    post_message: {
        method: HttpMethod.post
        path: "/api/chat.postMessage"
        input: PostMessage
        output: PostedMessage
        changes: true
        failed_when: { path: "ok"  equals: false }       // Slack answers 200 on failure
    }
}

// app.jadpo
slack: {
    package: Slack.v1
    domains: [support]
    credential: config.slack_bot_token
}

// domains/support/actions/alert_on_call.jadpo
action alert_on_call(ticket: Ticket) -> Unit {
    attempt slack.chat.post_message(Slack.PostMessage {
        channel: config.on_call_channel
        text: translations.support.urgent_ticket(title: ticket.title, locale: Locale.en_gb)
    })
}
```

Holds. Nothing beyond area 11.

## 2. Several calls in order: a Stripe subscription

```text
// packages/stripe/v2024_06_20/actions/start_subscription.jadpo
/// Creates the customer, attaches the card, starts the subscription and
/// finalises its first invoice, in the order Stripe requires.
public action start_subscription(input: StartSubscription) fails CardDeclined -> StartedSubscription {
    var customer = attempt customers.create({ email: input.email })
    attempt payment_methods.attach({ id: input.payment_method, customer: customer.id })
    var sub = attempt subscriptions.create({ customer: customer.id, price: input.price }) {
        card_declined: CardDeclined
    }
    var invoice = attempt invoices.finalise({ id: sub.latest_invoice })
    return { customer_id: customer.id, subscription_id: sub.id, invoice_id: invoice.id }
}

// domains/billing/actions/subscribe.jadpo
action subscribe(signup: Signup) fails PaymentDeclined -> Subscription {
    var started = attempt stripe.start_subscription({
        email: signup.email
        payment_method: signup.payment_method
        price: config.pro_price
    }) {
        card_declined: PaymentDeclined
    }
    return attempt Subscription.create({
        account_id: signup.account_id
        stripe_customer_id: started.customer_id
        stripe_subscription_id: started.subscription_id
    })
}
```

Holds, once multi-step calls (open item 7b) exist: four changing calls in
the package action, each recorded as it happens, a rerun resuming after
the last. Inside the package, operations are named without the connection
(`customers.create`); the app reaches them through its own (`stripe.`).

## 3. Enriching a webhook

```text
// packages/stripe/v2024_06_20/subscribers/renewal.jadpo
subscriber renewal {
    on: InvoicePaid                                      // Stripe's raw webhook event
    run: announce_renewal(event)
}

// packages/stripe/v2024_06_20/actions/announce_renewal.jadpo
action announce_renewal(paid: InvoicePaid) -> Unit {
    if paid.billing_reason != BillingReason.subscription_cycle {
        return
    }
    var sub = attempt subscriptions.retrieve({ id: paid.subscription })
    emit SubscriptionRenewed {                           // the package's own event
        subscription_id: sub.id
        customer_id: sub.customer
        period_end: sub.current_period_end
    }
}

// domains/billing/subscribers/extend_access.jadpo
subscriber extend_access {
    on: Stripe.SubscriptionRenewed
    run: extend_access_until(event.subscription_id, event.period_end)
}
```

Holds, with one rule to state (finding 2): an action that emits may only
be run by the package's own subscribers and polls, never called by the
app.

## 4. An API with no webhooks: exchange rates

```text
// packages/exchange_rates/v1/provider.jadpo
provider {
    protocol: Protocol.json_http
    hosts: { live: "https://api.rates.example" }
    credential: CredentialScheme.header_key
    polls: {
        latest: {
            call: rates.latest
            every: 1d
            emits: RatesPublished                        // only when the answer changes
        }
    }
}

// app.jadpo
rates: {
    package: ExchangeRates.v1
    domains: [finance]
    credential: config.rates_key
    polls: { latest: 6h }                                // optional: overrides the default
}

// domains/finance/subscribers/store_rates.jadpo
subscriber store_rates {
    on: ExchangeRates.RatesPublished
    run: save_rates(event.rates)                         // the app's table, the app's write
}
```

Holds for feeds that do not depend on the app's data. Polling each of the
app's own records (finding 3) does not fit `polls:`.

## 5. Polling the app's own records: parcel tracking

The courier has no webhooks, and the app wants to check each parcel still
in transit. A poll cannot know which parcels those are, so this is the
app's schedule calling a package action, from a recipe:

```text
jadpo package recipe acme_couriers track_parcels --into domains/shipping

// domains/shipping/schedules/track_parcels.jadpo        (copied, now the app's)
schedule track_parcels {
    every: 15m
    run: check_parcels_in_transit()
}

// domains/shipping/actions/check_parcels_in_transit.jadpo
action check_parcels_in_transit() -> Unit {
    for shipment in in_transit_shipments() {
        emit ParcelCheckDue { shipment_id: shipment.id }     // one delivery each, retried alone
    }
}

// domains/shipping/actions/check_parcel.jadpo
action check_parcel(shipment: Shipment) -> Shipment {
    var events = attempt couriers.bookings.tracking({ id: shipment.booking_id })
    return attempt Shipment.update(shipment) {
        set: { delivered: AcmeCouriers.tracking.delivered(events) }
    }
}
```

Holds, and is the case recipes exist for.

## 6. A service: invoices as PDFs

```text
// packages/gotenberg/v8/provider.jadpo
provider {
    protocol: Protocol.multipart_http
    runs: { image: "gotenberg/gotenberg:8@sha256:…"  port: 3000 }
}

// app.jadpo: the service starts only once a person accepts it in the approval report
pdfs: {
    package: Gotenberg.v8
    domains: [billing]
}

// domains/billing/actions/issue_invoice.jadpo
action issue_invoice(invoice: Invoice) -> Invoice {
    var html = ???                                       // finding 4: nothing renders a page yet
    var pdf = attempt pdfs.chromium.convert_html({ html: html })   // changes: false
    var file = attempt invoice_files.put(pdf, FileType.pdf)        // the one changing call
    emit InvoiceEmail { to: invoice.email  locale: invoice.locale  pdf: file }
    return attempt Invoice.update(invoice) { set: { pdf: file } }
}
```

Holds for the call, the file and the email with its attachment. The HTML
has nowhere to come from (finding 4).

## 7. Filling a capability: Postmark sends the app's email

```text
// packages/postmark/v1/provides/email.jadpo
provide email {
    send: {
        call: email.send
        input: {
            From: message.from
            To: message.to
            Subject: message.subject
            HtmlBody: message.html
            TextBody: message.text
            Attachments: { each: message.attachments  as: { ... } }
        }
        receipt: response.MessageID
    }
}

// app.jadpo
email: { connection: postmark  from: config.mail_sender }

// anywhere in the app
emit WelcomeEmail { to: user.email  locale: user.locale  display_name: user.display_name }
```

Holds. Switching to SMTP changes `email:` only.

## 8. A pure library: postcodes

```text
// packages/postcodes/v2/libraries/uk.jadpo
library uk {
    /// Parses a UK postcode, normalising spacing and case.
    ///
    /// ```jadpo
    /// assert Postcodes.uk.parse("sw1a1aa") == Postcodes.UkPostcode("SW1A 1AA")
    /// ```
    public function parse(text: Text) fails InvalidPostcode -> UkPostcode { ... }
}

// domains/accounts/actions/save_address.jadpo
var postcode = attempt Postcodes.uk.parse(input.postcode) {
    invalid_postcode: AddressInvalid
}
```

Holds.

## 9. Acting for each user: "connect your Google Calendar"

```text
// app.jadpo
calendar: {
    package: GoogleCalendar.v3
    domains: [scheduling]
    credential: ???                                      // finding 5: whose credential?
}

// domains/scheduling/actions/add_to_calendar.jadpo
action add_to_calendar(booking: Booking) -> Unit {
    attempt calendar.events.insert({ ... })              // as which user?
}
```

Does not hold. A connection has one credential; this needs one per user,
obtained by consent, kept safely and refreshed. Open item 7c.

## Findings

1. **The design holds for seven of the nine.** Calls, multi-step package
   actions, enriched webhooks, polls, recipes, services, capabilities and
   libraries all read cleanly, and in every one the app's own action is
   the only thing writing its data.
2. **An emitting package action needs a rule.** Package actions the app
   calls may not emit; ones run by the package's own subscribers and polls
   may. Simplest check: an action that contains `emit` cannot be `public`,
   so the app can never call it, and only the package's subscribers can.
3. **Polling splits in two.** `polls:` fits feeds that do not depend on the
   app's data (rates, a price list). Checking each of the app's own records
   is the app's schedule calling the package, given as a recipe. Worth
   saying so on the page, so nobody tries to stretch `polls:`.
4. **Nothing renders a document.** Templates exist only for email, so an
   invoice page to turn into a PDF has no home. The `documents` capability
   proposed under "Software and disk" (a typed template rendered to HTML or
   PDF, the email template rules reused) fills it; scenario 6 is its case.
5. **Per-user OAuth is a real gap** (open item 7c): the connection would
   name a per-user credential (`credential: OAuthGrant.per_user`), the
   runtime keeps each user's tokens and refreshes them, consent comes
   through generated routes like sign-in's, and a call says whose grant it
   uses (`as: booking.organiser_id`).
6. **Names on a connection need one rule**: a package's resources and its
   actions share the connection's namespace (`stripe.customers`,
   `stripe.start_subscription`), so a clash is an error in the package.
