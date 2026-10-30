# The cake shop, as shown on jadpo.dev

The home page of [jadpo.dev](https://jadpo.dev) shows these files, line for
line. They are kept here so the gate compiles them: if the language changes
and an example stops checking, this page fails, and the site's copy is
updated from it.

A customer places an order for a cake. These are real files from a small shop
(imports left out), checked by the compiler with the rest of the project.

`domains/shop/types/quantity.jadpo`

```jadpo check project=homepage file=domains/shop/types/quantity.jadpo
/// How many cakes one order is for: the oven holds twelve.
type Quantity = Int {
    min: 1
    max: 12
}
```

`domains/shop/actions/place_order.jadpo`

```jadpo check project=homepage file=domains/shop/actions/place_order.jadpo hide=use
/// What placing an order takes; a quantity outside 1 to 12 never gets in.
type PlaceOrder = Input {
    cake_id: Cake.id
    quantity: Quantity
}

/// Charges the card, records the order and announces it.
action place_order(
    input: PlaceOrder,
    customer: Principal.user
) fails CakeNotFound, CardDeclined -> OrderReceipt {
    var cake: Cake = attempt orderable_cake(input.cake_id)
    // Cakes in grams, box in kilograms, packing in pounds: one `Mass`.
    var weight: Mass = cake.weight * input.quantity + 1.2kg + 3lb
    // Dividing a `Mass` by 1kg gives a plain number: how many kilograms.
    var kilograms: Decimal = weight / 1kg
    var per_kilo: Sterling = Sterling(money.from_minor(200, Currency.gbp))
    var goods: Sterling = Sterling(cake.price * input.quantity)
    var postage: Sterling = Sterling(per_kilo * kilograms)
    // `* 120%` adds 20% VAT to the goods and the postage.
    var total: Sterling = Sterling((goods + postage) * 120%)
    // The provider's decline becomes this app's own failure.
    attempt pay.charges.create(
        AcmePay.ChargeRequest {
            amount_minor: money.minor(total)
        }
    ) {
        declined_card: CardDeclined
    }
    // The order and the event commit together, or not at all.
    var order: Order = Order.create {
        set: {
            customer_id: customer.id
            cake_id: cake.id
            quantity: input.quantity
            total: total
        }
    }
    emit OrderPlaced {
        order_id: order.id
    }
    return OrderReceipt {
        id: order.id
        total: order.total
    }
}
```

`domains/shop/routes/orders.jadpo`

```jadpo check project=homepage file=domains/shop/routes/orders.jadpo hide=use
routes Orders {
    prefix: /orders

    // The action's failures become HTTP errors by their kind (404, 422) and
    // are listed in api/main.jadpo_record. One it left undeclared wouldn't compile.
    /// Places an order.
    POST / {
        input: PlaceOrder
        output: OrderReceipt
        status: SuccessStatus.created
        run: place_order(input, current_principal.user)
    }
}
```

What the compiler guarantees here:

- **Input can't be invalid.** A `Quantity` outside 1 to 12 never reaches `place_order`; the route refuses it.
- **Units add up exactly.** Grams, kilograms and pounds combine as one `Mass`; adding a duration to it is a compile error (`type.arithmetic_operand`).
- **Money keeps its currency.** Goods, postage and VAT (`* 120%`) are all `Sterling`, so another currency can't be added to them.
- **Every failure is declared.** `place_order` lists what can go wrong, and one it forgets to list is a compile error. A cake that is off the menu is `CakeNotFound`.
- **The provider's decline is the app's own failure.** `declined_card` is mapped to `CardDeclined`.
- **The order and its event commit together.** `Order.create` and `emit` are in one action.
- **Orders belong to their customer.** The `Order` policy grants `create` and `read` only to the customer it names, and the route passes the signed-in customer, not an ID from the request.

<!-- The rest of the project, so the files above are checked as a whole by
tools/docs/language_docs.py. GitHub does not show this comment.

```jadpo check project=homepage file=domains/shop/actions/place_order.jadpo uses
use app { pay }
use shop/entities { Cake, CakeNotFound, Order }
use shop/types { OrderReceipt, Quantity, Sterling }
use shop/failures { CardDeclined }
use shop/queries { orderable_cake }
use shop/events { OrderPlaced }
use shared/providers/acme_pay
```

```jadpo check project=homepage file=domains/shop/routes/orders.jadpo uses
use shop/types { OrderReceipt }
use shop/actions { PlaceOrder, place_order }
```

```jadpo check project=homepage file=app.jadpo
use shop/entities { Customer }

application {
    apis: {
        main: {
            prefix: /
            authentication: [AuthenticationMethod.browser_session]
        }
    }
    connections: {
        main: {
            driver: Driver.postgres
            url: config.main_url
        }
        pay: {
            package: AcmePay.v1
            domains: [shop]
            credential: config.pay_api_key
            environment: config.pay_environment
        }
    }
    events: {
        store: main
    }
    steps: {
        encryption_key: config.step_encryption_key
    }
}

principal {
    user: {
        subject: Customer.email
        active: Customer.active == true
    }
}

authentication {
    browser_session: {
        cookie: "shop_session"
    }
}
```

```jadpo check project=homepage file=config.jadpo
config {
    main_url: PostgresUrl
    pay_api_key: Text {
        secret: true
    }
    pay_environment: ProviderEnvironment {
        default: ProviderEnvironment.sandbox
    }
    step_encryption_key: SigningKey
}
```

```jadpo check project=homepage file=shared/providers/acme_pay/v1/package.jadpo
/// Acme Pay's payments API.
package AcmePay.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=homepage file=shared/providers/acme_pay/v1/provider.jadpo
/// Acme Pay's payments API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-pay.example"
        sandbox: "https://sandbox.acme-pay.example"
    }
    credential: CredentialScheme.bearer
    idempotency: {
        header: "Idempotency-Key"
        window: 24h
    }
    error_code: "error.code"
}
```

```jadpo check project=homepage file=shared/providers/acme_pay/v1/types/charge_request.jadpo
/// What a charge takes: pence.
public type ChargeRequest = Object {
    amount_minor: Int
}
```

```jadpo check project=homepage file=shared/providers/acme_pay/v1/types/charge.jadpo
/// What Acme Pay gives back.
public type Charge = Object {
    id: Text {
        max_length: 40
    }
}
```

```jadpo check project=homepage file=shared/providers/acme_pay/v1/failures/declined_card.jadpo
/// The card was declined.
public type DeclinedCard = Failure {
    kind: FailureKind.rejected
    message: "The card was declined."
}
```

```jadpo check project=homepage file=shared/providers/acme_pay/v1/resources/charges.jadpo
public resource charges {
    create: {
        method: HttpMethod.post
        path: "/v1/charges"
        input: ChargeRequest
        output: Charge
        changes: true
        idempotent: true
        failures: [DeclinedCard]
        source: PackageSource.written
    }
}
```

```jadpo check project=homepage file=domains/shop/queries/orderable_cake.jadpo
use shop/entities { Cake, CakeNotFound }

/// A cake that is on the menu; one that isn't reads as missing.
query orderable_cake(cake_id: Cake.id) fails CakeNotFound -> Cake {
    from: Cake
    where: Cake.id == cake_id and Cake.on_menu == true
    missing: CakeNotFound
}
```

```jadpo check project=homepage file=domains/shop/types/sterling.jadpo
/// An amount in pounds, held as whole pence.
type Sterling = Money {
    currency: Currency.gbp
    rounding: Rounding.half_even
}
```

```jadpo check project=homepage file=domains/shop/types/order_receipt.jadpo
use shop/entities { Order }

/// What the customer is shown after ordering.
type OrderReceipt = Output {
    pick: [Order.id, Order.total]
}
```

```jadpo check project=homepage file=domains/shop/failures/card_declined.jadpo
/// The customer's card was declined.
type CardDeclined = Failure {
    kind: FailureKind.rejected
    message: "The card was declined."
}
```

```jadpo check project=homepage file=domains/shop/entities/customer.jadpo
entity Customer {
    id: Uuid
    email: Email {
        unique: true
    }
    active: Bool

    policy: {
        create: [Access.public]
        read: [id]
    }
}
```

```jadpo check project=homepage file=domains/shop/entities/cake.jadpo
use shop/types { Sterling }

entity Cake {
    id: Uuid
    name: Text {
        min_length: 1
        max_length: 60
    }
    price: Sterling
    weight: Mass
    on_menu: Bool

    policy: {
        read: [Access.public]
    }
}
```

```jadpo check project=homepage file=domains/shop/entities/order.jadpo
use shop/entities { Cake, Customer }
use shop/types { Quantity, Sterling }

entity Order {
    id: Uuid
    customer_id: Customer.id {
        on_delete: OnDelete.restricted
    }
    cake_id: Cake.id {
        on_delete: OnDelete.restricted
    }
    quantity: Quantity
    total: Sterling

    policy: {
        create: [customer_id]
        read: [customer_id]
    }
}
```

```jadpo check project=homepage file=domains/shop/events/order_placed.jadpo
use shop/entities { Order }

/// A customer placed an order.
type OrderPlaced = Event {
    order_id: Order.id
}
```

```jadpo check project=homepage file=domains/shop/events/events.jadpo_record
// Generated by Jadpo from the events of domain shop; update it with `jadpo records update`, never by hand.
record events 1

event OrderPlaced
    order_id: Uuid
```

```jadpo check project=homepage file=api/main.jadpo_record migrate=initial
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

api main
    authentication browser_session

GET /openapi.json
    generated openapi
    status 200

POST /orders
    caller user
    input PlaceOrder (body)
    output OrderReceipt
    status 201
    fails cake_not_found
    fails card_declined

output OrderReceipt
    id: Uuid
    total: Money { currency: GBP, rounding: half_even }

input PlaceOrder
    cake_id: Uuid
    quantity: Int { min: 1, max: 12 }

failure cake_not_found
    status 404

failure card_declined
    status 422
    message "The card was declined."
```
-->
