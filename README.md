# Jadpo — a programming language for AI-written backends

Jadpo is an experimental, statically typed programming language for web
backends written mainly by LLMs and reviewed by people. Data, HTTP APIs,
authentication, permissions, external services and background work belong
to one application model that the compiler checks.

The aim is to make a large AI-written change reviewable: what can this
application do, who can do it, and which data and outside services can it
reach? A model writes the implementation; the compiler checks the rules;
a person owns the intent and permission decisions.

[Try Jadpo](#try-jadpo) · [Language specification](docs/language/README.md) ·
[Examples](examples/README.md) · [Explore with your agent](#explore-with-claude-code-or-codex) ·
[Roadmap](docs/roadmap.md) · [Contribute](CONTRIBUTING.md)

## What Jadpo code looks like

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
tools/language_docs.py. GitHub does not show this comment.

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

## Explore with Claude Code or Codex

Want a guided introduction? Give your coding assistant this prompt. It can
explain the design, walk through real code and help you run a first example.
A local clone gives it access to the full specification and tests.

```text
Help me explore Jadpo: https://github.com/richardwillars/jadpo
Use a local clone, or clone it into a new directory. Read README.md, then
docs/language/README.md and the current decisions and status notes in
areas 1–4. Explain why Jadpo exists, what it can build, how its
checks work and what is still planned. Cite the files you actually read.
Walk me through examples/01-first-project, including its source and tests.
Check the prerequisites, build the CLI and run check and test on that
example. If a prerequisite is missing, explain what I need to install.
Then help me try a small change in a separate copy of the example and use
the compiler and tests to understand the result. Follow the relevant
specification pages as we go.
```

Already have the repository? Open its folder in your assistant. In a
terminal, run `claude` for [Claude Code](https://code.claude.com/docs/en/quickstart)
or `codex` for [Codex](https://learn.chatgpt.com/docs/cli), then paste the
prompt. You can also open the local folder in their desktop or editor
interfaces. No Jadpo-specific agent integration is required.

Prefer to read and run it yourself? [Start with the setup below](#try-jadpo).
For other assistants, see [direct links and Gemini import](#giving-an-assistant-direct-links).

## Why build a language for this?

Coding agents can produce more software than a person can comfortably
review line by line. In a conventional backend, important facts are spread
across framework conventions, middleware, database code, configuration and
instructions to the agent. Jadpo makes more of those facts explicit in the
language, so a deterministic checker can follow them across the application.

- **Less architecture to infer.** Every project uses the same domain-based
  layout. A declaration's kind and business domain determine its file path;
  imports say where each name comes from.
- **Meaning in the types.** An order ID and a customer ID are different
  types even when both are UUIDs. Named types carry validation rules;
  values cross boundaries through checked contracts.
- **Failures a caller must consider.** An action declares its possible
  domain failures. Each caller must handle them or declare that it passes
  them on.
- **Permissions in the application model.** Entity and field grants,
  ownership and tenancy are checked together with queries, outputs and
  writes. The compiler produces a permissions report.
- **One graph for the tools and executable.** The Rust compiler builds a
  typed graph with control flow. Checking, editor tooling, the development
  interpreter and code generation read that graph, giving review tools a
  common account of the program.

These checks do not establish that the requirements are right. Human
review and application tests remain necessary. The longer explanation is
in the [vision and design philosophy](docs/vision.md).

## What can you build with Jadpo?

Jadpo targets data-backed HTTP services: business applications, APIs,
applications serving several customers, integrations and background
workflows. It can also call AI providers and search stored embeddings.
Your frontend can use its HTTP API; browser UI components and CSS are
outside Jadpo's scope.

The following capabilities are implemented in the experimental compiler.
The linked specification pages define their rules and record limitations.

| Capability | What Jadpo provides | Learn more |
|---|---|---|
| Types and business logic | Named and refined types, objects, enums, collections, pure functions, actions and checked failures | [Types](docs/language/02-values-and-types.md), [logic](docs/language/03-logic.md), [failures](docs/language/04-failures.md) |
| Stored data | PostgreSQL entities, references, constraints, migrations, named queries, pagination, aggregates and transactional writes | [Data](docs/language/05-data.md), [reads and writes](docs/language/06-reads-and-writes.md) |
| HTTP APIs | Typed request and response contracts, routes, API records and generated OpenAPI documents | [HTTP](docs/language/07-http.md) |
| Identity and access | Sessions, tokens, service keys, magic links and passkeys; ownership, roles, tenancy and field permissions | [Identity](docs/language/08-identity.md), [policy](docs/language/09-policy.md) |
| Configuration and everyday values | Typed settings and secret bindings, time zones, recurring times, translations, money, units and validated standard types | [Configuration and time](docs/language/10-configuration-and-time.md), [standard library](docs/language/19-standard-library.md) |
| External services | Typed provider packages and connections, webhooks, recordings, test fakes and multi-step calls | [Integrations](docs/language/11-integrations.md) |
| Background work | Events, a transactional outbox, subscribers, retries, batches and schedules | [Jobs and events](docs/language/12-jobs-and-events.md) |
| Testing and tools | Pure, action, route and schedule tests; real test databases; controlled clocks and provider fakes; watch mode, formatter and language server | [Testing](docs/language/13-testing.md), [tooling](docs/language/14-tooling.md) |
| Geography | Points, lines and polygons, geodesic operations, PostGIS queries and GeoJSON map layers | [Geospatial](docs/language/20-geospatial.md) |
| AI inside your app | Checked prompt files, typed model answers, streaming, token budgets, embeddings, pgvector search and checks on personal data sent to models | [AI](docs/language/21-ai.md) |

## Project status

**Experimental and under active development.** The language has 21 decided
specification areas. A decided feature is not necessarily implemented:
each area's status notes and the [roadmap](docs/roadmap.md) distinguish the
two.

The compiler supports an interpreter for development and tests, compiled
native deployables, and WebAssembly deployables for Cloudflare Workers.
The verification gate compares interpreted and compiled test outcomes and
includes tests under Cloudflare's local `workerd` runtime. Performance work
is in progress; a real Cloudflare deployment remains an explicit roadmap
validation step.

Automated personal-data export, erasure and retention; the full behavioural
change review and approval workflow; richer debugging tools; the developer
console; an MCP interface; and a public package registry are planned work.
Existing checks on personal data sent to AI models do not mean that the
whole [personal-data specification](docs/language/15-personal-data.md) is built.

Start by exploring and testing the examples. Production readiness and
performance parity with established stacks have not been established.

## Try Jadpo

Install Git and Rust through `rustup`, then build from source. The
repository's `rust-toolchain.toml` pins the Rust toolchain and components.
The first example needs no database or AI-provider credentials.

```sh
git clone https://github.com/richardwillars/jadpo.git
cd jadpo
cargo build --release --locked -p jadpo-cli
target/release/jadpo check examples/01-first-project
target/release/jadpo test examples/01-first-project
```

The [28 teaching projects](examples/README.md) progress from a first
function to databases, authentication, permissions, integrations and AI.
Each has a reading order, comments, tests and instructions. Examples that
use stored data need PostgreSQL; see [contributing](CONTRIBUTING.md) for
the build and verification prerequisites.

For an existing example, the development tools include:

```sh
target/release/jadpo fmt --check examples/01-first-project
target/release/jadpo test --watch examples/01-first-project
target/release/jadpo check --json examples/01-first-project
```

See [tooling](docs/language/14-tooling.md) for development servers, native
and Workers builds, editor support, commands and their implementation status.

## Failures a caller must handle

Failures are part of an action's signature. This excerpt from the
[bakery example](examples/09-bakery-orders/README.md) handles an unknown or wrong-currency discount code
by charging full price, while passing other failures to its caller:

```jadpo
use shared/types { Sterling }
use sales/types { Basket }
use sales/failures { BasketEmpty, CodeNotInPounds, CodeUnknown, DiscountTooLarge }
use sales/actions { price_basket, price_lines }

action till_total(basket: Basket) fails BasketEmpty, DiscountTooLarge -> Sterling {
    return match price_basket(basket) {
        success(receipt) -> receipt.total
        CodeUnknown -> price_lines(basket)
        CodeNotInPounds -> price_lines(basket)
        BasketEmpty -> propagate
        DiscountTooLarge -> propagate
    }
}
```

The [complete source](examples/09-bakery-orders/domains/sales/actions/till_total.jadpo)
and its [tests](examples/09-bakery-orders/domains/sales/actions/till_total.test.jadpo)
show the surrounding declarations. Handled failures leave the action's
`fails` list; unhandled failures prevent it from checking.

For the wider application model, follow the
[recipe book's tenancy and field permissions](examples/12-recipe-book/README.md),
the [repair café's events and outbox](examples/23-repair-cafe/README.md), and
[lost property's typed AI calls and vector search](examples/28-lost-property/README.md).

## How the compiler works

```text
.jadpo source → typed application graph → checks, reports and editor tooling
                                      → interpreter for development and tests
                                      → generated Rust → native or WebAssembly
```

The graph includes declarations, calls and control flow. Stored schemas,
API contracts and package locks have committed records whose changes can
be reviewed; manifests and reports are regenerated from the checked
application. The planned change-review workflow will compare manifests
to show behaviour and permission changes before a reviewer reads the code.

See the [compiler and runtime architecture](docs/compiler-runtime.md) and
[records and manifest specification](docs/language/18-records-and-manifest.md)
for what is built and how the remaining review tools fit.

## Use Jadpo with an AI agent

Jadpo is new: give an assistant the specification rather than expecting it
to know the language from training or infer it from one example. You can
use any coding assistant that can read the relevant files and run the CLI;
Jadpo does not require a particular model provider.

1. **Read the [primer](docs/primer.md)** first: the layout, the syntax and
   the constructs nearly every app uses, in one page. Then the
   [language map](docs/language/README.md) lists all 21 areas; start with
   layout, types, logic and failures (areas 1–4).
2. **Read the rules for the task:** for an authenticated API that changes
   data, continue with areas 5–9 and 13. For integrations, read 11–12;
   for AI features, read 21. Read the current decisions and status notes
   as well as the examples; pages retain older alternatives for context.
3. **Open a complete example** from the [example index](examples/README.md),
   including its tests. Check intended features against their area pages
   and the roadmap before using them.
4. **Run the compiler and tests.** Use `jadpo check --json` for structured
   diagnostics, then `jadpo test`. Treat a missing rule or unresolved
   permission decision as something to resolve explicitly.

For work on the compiler itself, also read [AGENTS.md](AGENTS.md),
[contributing](CONTRIBUTING.md) and the architecture document.

### Giving an assistant direct links

For assistants with web-fetch tools, start with the
[plain-text README](https://raw.githubusercontent.com/richardwillars/jadpo/main/README.md)
and [plain-text specification index](https://raw.githubusercontent.com/richardwillars/jadpo/main/docs/language/README.md).
The [llms.txt reading index](llms.txt) supplies absolute, direct links to
every area. It is a navigation aid: assistants may need each relevant URL
provided explicitly, and file access depends on their tools and GitHub's
access controls. A link does not guarantee that its contents were read.

### Gemini repository import

In the Gemini web app on a computer, use **Add file → More uploads → Import
code**, then enter `https://github.com/richardwillars/jadpo`. Google's
[GitHub import documentation](https://support.google.com/gemini/answer/16176929?hl=en)
says that the GitHub app cannot read a repository merely from a URL in the
prompt. It supports one repository per chat, up to 5,000 files and 100 MB;
the import is a snapshot and does not sync later changes.

The Gemini API's [URL context tool](https://ai.google.dev/gemini-api/docs/url-context)
is a separate mechanism: it can fetch supplied public URLs, but does not
follow their nested links automatically. Supply the individual pages the
task needs and verify retrieval. If importing or fetching is unavailable,
attach the relevant Markdown and example files, or use a local clone with
a file-reading coding agent.

## Contribute and follow the project

Jadpo is maintained by [Richard Willars](https://github.com/richardwillars).
The public home for the project is currently
[this GitHub repository](https://github.com/richardwillars/jadpo).

- [Contributing](CONTRIBUTING.md): setup, the verification gate and working rules.
- [Roadmap](docs/roadmap.md): the only tracker for planned implementation work.
- [Issues](https://github.com/richardwillars/jadpo/issues): report bugs or ask questions.
  Include the source, compiler output and expected behaviour where possible.
- [Documentation index](docs/README.md): architecture, design rationale and decisions.

Feedback on the language model, diagnostics and complete examples is
particularly useful. Check the roadmap and open pull requests before
starting implementation, since other people and agents may be working on it.
Star the repository to find it again, or use GitHub's Watch menu for updates.

## Licence

Code is [Apache 2.0](LICENSE); documentation is [CC BY 4.0](docs/LICENSE.md).
See [TRADEMARKS.md](TRADEMARKS.md) for use of the Jadpo name and the naming
requirements for modified versions. Design skills in `.agents/skills/`
are third-party material under their own licences.
