# Layout stress scenarios

**Purpose:** test cases a single, compiler-enforced Jadpo project layout must
survive. This is input to the layout decision in
[area 1, open question 1](../01-project-layout-and-modules.md#open-questions);
it does not propose a layout.
**Status:** research, 2026-10-11. Scenario inventories are designed estimates,
not measured applications. Observations about the golden app are verified
against `examples/golden-todo-migration/` and `examples/golden-todo/app.jadpo`.

## Ground rules the scenarios assume

These come from [project structure](../../project-structure.md), the
[entity, query and transaction model](../../entity-query-model.md), area 1 and
the [event model](../../event-model.md) (a candidate design):

- Names are unique across the whole project, modules or not.
- Exactly one entity per file under `entities/`. The entity dossier owns its
  fields, persistence, lifecycle, policy, entity-centred queries, functions,
  actions and supporting failures. No partial entities or extension files.
- Cross-entity reads are `queries/`; multi-entity actions are `workflows/`;
  routes bind HTTP and only call operations; jobs are scheduled entry points
  calling the same operations; services describe external HTTP APIs.
- Events are nominal families with variants; subscribers are discovered
  handlers. Ownership of emitting authority is mapped by module namespace,
  not path.
- Area 1 recommends a closed role-first layout: `app.jadpo` at the root plus
  only `entities/ values/ queries/ workflows/ routes/ jobs/ services/ tests/`,
  one home per declaration kind, file named after its main declaration.

Two documentation conflicts are already visible: `project-structure.md` says
`values/` rejects failures, while area 1 says failures are accepted there and
the rule is not enforced. And the recommended single root `app.jadpo` is
already three root files in the migrated golden app.

---

## Scenario 1 — Golden todo (baseline)

A personal todo API: users own todos, services authenticate with API keys,
overdue todos trigger a reminder email, soft-deleted todos are purged.

| Kind | Count | Names |
|---|---|---|
| Entities | 4 | User, Todo, Service, ServiceCredential |
| Values | ~25 | TodoTitle, PageSize, 5 status/role enums, 4 secret/URL types; CreateTodo, PatchTodo, ListTodos, TodoCursor, TodoView, TodoPage, UserWithTodos, PublicHealth, ReminderMessage, ReminderReceipt |
| Queries | 0 in `queries/` | `User.with_todos` reads Todo but lives in the User dossier |
| Workflows | 1 (original) | `send_due_reminders` (Todo + User + ReminderMail) |
| Routes | 8 + 1 | health; todos ×5; users ×2; `/auth/exchange` declared inside `authentication` |
| Jobs | 2 | `overdue_reminders`, `purge_deleted_todos` (purge is now a lifecycle rule) |
| Services | 1 | ReminderMail, whose contract is a JSON file under `tests/assurance/` |
| Other | | application, principal, 2 authentications, config, readiness, human-owned `policy.jadpo`, ~13 failures |

**Cross-cutting.** `UserDisabled`/`ServiceDisabled` failures belong to
authentication, not an entity. `ReminderMessage` is a service payload sitting
beside HTTP contracts in `values/contracts.jadpo`. `TodoRole` and `UserRole` are
used by one entity each but live in `values/domain.jadpo`. The original keeps
policy in a separate human-owned file that agents may read but not change;
the migration moved policy into entity blocks.

**Verified drift in the migration.** `routes/liveness.jadpo` contains
`GET /todos/{todo_id}` — a todo route in the health file, accepted silently.
Jobs, the mail service and readiness were dropped from the migration, so the
layout has never held them.

**Stress questions.**
1. Is one root file or several (`application`, `authentication`, `config`)
   canonical? Can a tiny app stay a single `app.jadpo`?
2. Does `GET /users/{user_id}/todos` go in `routes/users.jadpo` or
   `routes/todos.jadpo`? Is a route file keyed by path prefix, called entity or
   returned type?
3. Where do failures raised by authentication or a service live?
4. Do entity-local enums (`TodoRole`) go in the dossier or `values/`?
5. Where does a service's imported contract artifact live, given it is not
   a test?
6. When does a cross-entity read in a dossier (`User.with_todos`) have to
   move to `queries/`?
7. Can human-owned policy be a separate file when the model says policy is
   an entity block?

---

## Scenario 2 — E-commerce

Storefront and admin for a single merchant: catalogue, stock across
warehouses, carts, checkout, Stripe-like payments with webhooks, refunds,
shipping, transactional email.

| Kind | Count | Names (representative) |
|---|---|---|
| Entities | 22 | Customer, SavedAddress, Category, Product, ProductVariant, PriceList, StockLocation, InventoryItem, StockReservation, Cart, CartLine, Coupon, Promotion, Order, OrderLine, Payment, PaymentAttempt, Refund, Shipment, ShipmentLine, StaffUser, WebhookReceipt |
| Values | ~110 | Money, Currency, Sku, Quantity, TaxRate, AddressSnapshot, OrderStatus, PaymentStatus; ~85 inputs/outputs |
| Queries | 10 | product_listing, product_detail, cart_summary, order_detail, order_history, admin_order_search, low_stock_report, daily_sales, refundable_amount, customer_overview |
| Workflows | 11 | checkout, confirm_payment, fail_payment, cancel_order, issue_refund, fulfil_order, merge_guest_cart, apply_coupon, expire_reservations, restock_return, reconcile_payment |
| Routes | ~75 | storefront: products 6, categories 3, cart 8, checkout 3, orders 5, account 6, addresses 5; admin: products 12, inventory 6, orders 10, refunds 4, coupons 5, staff 4; webhooks: payments 1, carrier 1; health 2 |
| Jobs | 6 | expire_cart_reservations, abandoned_cart_email, reconcile_payments, nightly_sales_rollup, purge_old_carts, retry_stuck_refunds |
| Services | 4 | PaymentProvider (create_intent, capture, refund, retrieve), Mailer, TaxCalculator, ShippingCarrier |
| Events | 4 families / 9 subscribers | OrderEvent, PaymentEvent, RefundEvent, InventoryEvent; send_order_confirmation, send_shipping_email, release_stock_on_cancel, … |
| Tests | ~180 | fixtures: seeded catalogue, fake provider, signed webhook bodies |
| Failures | ~45 | PaymentDeclined, OutOfStock, CouponExpired, RefundExceedsCapture, … |

**Cross-cutting.** `Money` is used by eight entities. `calculate_totals(lines)`
is a pure function needed by both Cart and Order. `PaymentStatus` appears on
Order and Payment. Address is both an entity (saved addresses) and a value
(snapshot on an order), so global uniqueness forces two names. Webhook
signature verification is an `authentication` declaration with no user
behind it. `WebhookReceipt` (inbound dedupe) is owned by the integration, not
a business entity. Admin `StaffRole` policy applies to every entity.

**Stress questions.**
1. Where does `Money` live, and is `values/` one file, one file per type, or
   one file per entity's contracts? At ~110 values the current two-file
   split is unusable.
2. Where does a pure function shared by Cart and Order live? There is no
   `functions/` role.
3. The Order dossier (fields, 8-state lifecycle, 12 actions, 8 queries, 10
   failures, customer and staff policy) is ~800 lines. One file or companion
   files?
4. Inbound payment webhook needs a route, an authentication, a dedupe entity
   and a workflow. Is there one place to look, or four?
5. Do `/orders` (customer) and `/admin/orders` share `routes/orders.jadpo`,
   or does admin get its own file or folder?
6. Do `PaymentDeclined` and the provider's request/response values live in
   the service file or `values/`?
7. Is an event family owned by an entity (in its dossier) or a peer
   declaration (`events/`)? Where do subscribers that only send email go?
8. Where do email templates and signed webhook fixture bodies (non-Jadpo
   files) live?
9. Are CartLine and OrderLine really separate files, given they have no life
   outside their parent?

---

## Scenario 3 — Multi-tenant B2B SaaS

A project-tracking product sold to companies: organisations, members and
custom roles, SSO, per-seat subscriptions, audit log, organisation API keys,
plus a separate back-office API for support staff.

| Kind | Count | Names (representative) |
|---|---|---|
| Entities | 19 | Organisation, OrganisationSettings, User, Membership, Invitation, CustomRole, Team, Project, Task, Comment, Plan, Subscription, Invoice, UsageRecord, ApiKey, AuditEntry, SsoConnection, StaffUser, ImpersonationSession |
| Values | ~120 | OrganisationRole, Permission, Scope, Seats, Money, Slug, TimeZone; ~95 inputs/outputs |
| Queries | 9 | organisation_overview, member_directory, seat_usage, audit_search, project_board, staff_org_search, billing_summary, … |
| Workflows | 12 | create_organisation, invite_member, accept_invitation, remove_member, change_plan, sync_subscription, rotate_api_key, start_impersonation, transfer_ownership, delete_organisation, … |
| Routes | ~95 | public `/v1/`: organisations 6, members 7, invitations 5, roles 5, teams 6, projects 8, tasks 10, comments 4, api-keys 4, billing 5, audit 2; SCIM 8; back-office `/internal/`: orgs 8, users 6, subscriptions 5, impersonation 3; webhooks: billing 1 |
| Authentications | 4 | browser_session, org_api_key, tenant_sso (per-tenant OIDC/SAML), staff_mfa |
| Jobs | 7 | aggregate_usage, sync_invoices, expire_trials, expire_invitations, purge_audit, notify_key_expiry, reconcile_seats |
| Services | 4 | BillingProvider, Mailer, IdentityProviderMetadata, ProductAnalytics |
| Events | 4 families / 10 subscribers | MembershipEvent, SubscriptionEvent, OrganisationEvent, ProjectEvent; every subscriber also records an AuditEntry |
| Deployables | 2 | public API, back-office API (different network exposure) |

**Cross-cutting.** Tenant scoping applies to every entity. Audit entries are
written as a side effect of nearly every action, by an entity whose writes
everyone needs. `OrganisationRole` is used by Membership, Invitation and ApiKey.
`app.jadpo` must hold four authentications that reference ApiKey, User,
StaffUser and SsoConnection, plus ~30 config keys. Per-tenant settings
(an entity) are distinct from app config (a declaration).

**Stress questions.**
1. Where is the tenancy rule declared once — `app.jadpo`, a policy file, or
   repeated in 19 policy blocks?
2. Is the back-office API a route group in the same app or a second
   deployable? If second, how do two deployables share entities without a
   library layer?
3. Do staff-only actions (`Organisation.force_delete`) live in the entity
   dossier beside customer actions, or under `workflows/`?
4. Does `app.jadpo` stay readable at four authentications, a three-variant
   principal and 30 config keys, or do authentication and config get files?
5. `AuditEntry` versus an `event` family named `AuditEvent`: names collide
   conceptually across two declaration kinds.
6. Public `/v1/` from day one: is the version a path segment only, or a
   layout level?
7. SCIM is a protocol surface, not a resource. Which routes file?

---

## Scenario 4 — Marketplace and booking

Short-stay rentals: hosts list properties, guests book dates, payments are
held and paid out to hosts, both sides review each other, and notifications go
by email, SMS and push. Heavy scheduled work.

| Kind | Count | Names (representative) |
|---|---|---|
| Entities | 21 | User, HostProfile, GuestProfile, Listing, ListingPhoto, PricingRule, AvailabilityBlock, ExternalCalendar, Booking, Payment, Payout, PayoutAccount, Review, Conversation, Message, Dispute, Notification, NotificationPreference, Wishlist, PromoCredit, StaffUser |
| Values | ~140 | DateRange, StayNights, Money, GeoPoint, TimeZone, CancellationPolicy, BookingStatus; ~110 inputs/outputs |
| Queries | 11 | search_listings, listing_detail, listing_calendar, quote_stay, host_dashboard, host_earnings, guest_trips, inbox, review_summary, … |
| Workflows | 14 | request_booking, accept_booking, decline_booking, cancel_by_guest, cancel_by_host, complete_stay, release_payout, submit_review, open_dispute, resolve_dispute, connect_payout_account, … |
| Routes | ~85 | guest 30, host `/host/` 28, admin 15, messaging 6, webhooks 3 (payments, payout account updates, SMS delivery), public calendar feed `.ics` 1, health 2 |
| Jobs | 14 | expire_booking_requests (5m), sync_external_calendars (30m), check_in_reminders, auto_complete_stays, release_payouts (daily), review_reminders, close_review_window, notification_digest, recompute_rankings, reconcile_payouts, host_statements (monthly), escalate_disputes, retry_photo_processing, purge_stale_holds |
| Services | 7 | PaymentProvider (charges + connected accounts + payouts), Mailer, Sms, Push, Geocoding, ImageProcessing, CalendarFeed (arbitrary host URLs) |
| Events | 5 families / ~25 subscribers | BookingEvent, PaymentEvent, ReviewEvent, MessageEvent, DisputeEvent; ~20 notification kinds × channel |

**Cross-cutting.** `DateRange` and `TimeZone` are shared by Listing,
AvailabilityBlock, PricingRule and Booking. `refund_for_cancellation(policy,
range, now)` is a pure function used by Booking, Payment and Payout. Two user
types share one `User` with profiles. Notifications are fan-out from every
domain. The payments shape repeats Scenario 2 almost exactly.

**Stress questions.**
1. 14 jobs: one file per job, one `jobs/schedule.jadpo`, or jobs grouped by
   entity? A job that only calls `Booking.expire_requests` is single-entity —
   does it still leave the Booking dossier?
2. Routes by resource or by audience? `/host/listings` and `/listings` call
   different actions on the same entity.
3. Where do ~25 subscribers go: by event family, by channel, by recipient?
4. `CalendarFeed` calls arbitrary URLs: does a `service` with no fixed base
   URL still belong in `services/`?
5. Blind two-sided reviews need a workflow, a job and lifecycle rules on
   Review: where does a reader find the whole rule?
6. Could payments be a reusable library shared with Scenario 2, and where
   would a library sit?
7. The Booking dossier has 9 states and ~15 actions: companion files again.

---

## Scenario 5 — Content and editorial workflow

A newsroom API: writers draft, editors and legal review, approved articles
are scheduled and published, every save is an immutable version, media is
uploaded and transcoded, a search index is kept in sync, and a translation
vendor returns work by webhook.

| Kind | Count | Names (representative) |
|---|---|---|
| Entities | 15 | Staff, Publication, Section, Tag, Article, ArticleVersion, ArticleTag, ReviewRequest, EditorialComment, MediaAsset, MediaRendition, SlugRedirect, TranslationJob, PartnerEndpoint, PreviewToken |
| Values | ~90 | Slug, RichBody, Locale, EditorialState, Embargo; ~70 inputs/outputs |
| Projections | 1 | Article → search store (declared in the Article dossier) |
| Queries | 8 | article_by_slug, section_front, editorial_dashboard, version_diff, review_queue, media_library, author_page, sitemap_entries |
| Workflows | 9 | submit_for_review, approve, request_changes, schedule_publish, publish, unpublish, restore_version, request_translation, receive_translation |
| Routes | ~70 | public read 12 (incl. RSS and sitemap, non-JSON); articles 22 (CRUD, submit, approve, reject, schedule, unpublish, versions list/get/restore, diff, preview); reviews 6; media 8 (multipart upload); sections/tags 10; partners 5; webhooks 2; health 2 |
| Jobs | 6 | publish_scheduled (1m), lift_embargoes, reconcile_search_index, prune_versions, retry_renditions, regenerate_sitemap |
| Services | 4 | SearchIndex, ObjectStorage/CDN purge, TranslationVendor, PartnerWebhook (outbound, URL per PartnerEndpoint) |
| Events | 2 families / 6 subscribers | ArticleEvent (published, updated, unpublished), MediaEvent; purge_cdn, notify_partners, … |

**Cross-cutting.** Editorial roles (writer, editor, legal, admin) with field
rules span Article, ArticleVersion and ReviewRequest. Article and
ArticleVersion are one concept in two entities. The search projection has an
index mapping that is an external JSON artifact. Preview tokens are a fifth
authentication path.

**Stress questions.**
1. 22 routes on one resource plus a nested `versions` sub-resource: one
   `routes/articles.jadpo` (~450 lines) or split?
2. Is ArticleVersion a separate dossier or a companion of Article?
3. The state machine is half lifecycle (in the dossier) and half workflows
   (`submit_for_review`, `publish`). Is that split navigable?
4. Where do search mappings, RSS templates and the vendor's contract live?
5. Inbound vendor webhook and outbound partner webhooks are mirror images:
   do both sit with their service?
6. Who owns cross-entity editorial policy?

---

## Scenario 6 — Large application at scale

A retail platform with eight teams, each owning a business domain, one shared
codebase, a versioned public API, a partner API and a back-office.

| Domain (team) | Entities | Examples |
|---|---|---|
| Identity & access | 8 | User, Session, ApiClient, Role, Grant, MfaFactor, StaffUser, Consent |
| Catalogue | 9 | Product, Variant, Category, Brand, Attribute, MediaAsset, Bundle, … |
| Pricing & promotions | 7 | PriceBook, Price, Promotion, Coupon, Voucher, TaxClass, … |
| Ordering & checkout | 10 | Cart, CartLine, Order, OrderLine, Quote, Subscription, Return, … |
| Payments & ledger | 11 | Payment, Refund, Payout, LedgerAccount, LedgerEntry, Invoice, Dispute, … |
| Fulfilment & warehouse | 12 | Warehouse, Bin, StockItem, Reservation, PickList, Shipment, Carrier, … |
| Customer support | 7 | Ticket, Message, Macro, SlaPolicy, Escalation, Csat, Agent |
| Partners & reporting | 8 | Partner, PartnerUser, Feed, Webhook, ExportJob, ReportDefinition, … |

| Kind | Count |
|---|---|
| Entities | 72 |
| Values | ~650 (≈80 shared domain types, rest contracts) |
| Queries / workflows | ~90 / ~70 (≈20 workflows cross two domains) |
| Routes | ~430: public v1 150, public v2 90, partner 60, back-office 120, webhooks 10 |
| Jobs | ~45 |
| Services | 12: 2 payment providers, 3 carriers, tax, fraud, mail, SMS, ERP, warehouse-export, address validation |
| Events | ~25 families, ~120 subscribers |
| Failures | ~350 |
| Tests | ~2,500 |
| Deployables | 4: public API, partner API, back-office API, worker (jobs + subscribers) |

**Cross-cutting.** A shared kernel (`Money`, `CountryCode`, `Address`,
`Locale`) is used by every team. Ledger posting workflows touch Ordering,
Payments and Fulfilment. Every domain emits events others consume. Global
uniqueness makes `LineItem`, `Status`, `NotFound` collide across teams. v1 and
v2 expose the same entities through different contract values
(`OrderViewV1`, `OrderViewV2`).

**Stress questions.**
1. Flat `entities/` with 72 files, or domain subfolders? Role-first spreads one
   team across seven folders; domain-first spreads one role across eight.
2. CODEOWNERS needs path prefixes. Role-first means ~56 ownership lines and
   shared files nobody owns cleanly.
3. The event model maps component ownership to module namespaces, but area 1
   recommends dropping modules. Without modules, is the folder the owner?
4. Where does a workflow that spans three teams live, and who reviews it?
5. v1/v2: separate route files, separate contract values, or a layout level
   (`routes/v1/`)? How is v1 removed cleanly?
6. Four deployables from one source: how does each select its routes, jobs
   and subscribers, and does the worker compile route code it never serves?
7. Does an agent editing Fulfilment need to load 650 values to find the 40 it
   uses?
8. Where do authored data migrations and backfills live once schema changes
   are compiler-owned?
9. 2,500 tests: mirrored tree, colocated, or per domain?

---

## The hardest layout questions, ranked

Ranked by how often each bites: how many scenarios hit it, weighted by how
often a developer or agent meets it day to day.

| # | Question | Bites in | Why it ranks here |
|---|---|---|---|
| 1 | **Shared values: where do they live and how does `values/` avoid becoming a dumping ground?** One file, per type, per owning entity, per domain? | 1–6 | Every new input/output hits it; contracts outnumber entities 5–10×. |
| 2 | **Route file granularity:** keyed by path prefix, entity called, or audience? Where do nested routes (`/users/{id}/todos`) go? | 1–6 | Already misfiled in the golden app; every new endpoint decides it. |
| 3 | **Large entity dossiers:** one file at 500–800 lines, or canonical companion files without partial entities? | 2–6 (Todo is already 211 lines) | Core entities (Order, Booking, Article) grow fastest and are edited most. |
| 4 | **Failure homes:** entity file, root, service file, or `values/`? Shared failures (auth, not-found patterns)? | 1–6 | Docs disagree today; every action adds failures. |
| 5 | **Route surfaces:** public, admin/back-office, partner, internal, webhook — folder, file prefix, or route attribute? | 2–6 | Every non-trivial app has at least two audiences. |
| 6 | **Root files:** one `app.jadpo`, or fixed root files for application, authentication, config? When does it split? | 1, 3, 5, 6 | Grows with every auth method and config key; migration already split it. |
| 7 | **Service bundles:** where its failures, request/response values, credentials and imported contract artifact live. | 1–6 | Every integration; contract artifacts currently live under `tests/`. |
| 8 | **Inbound webhooks:** route + signature authentication + dedupe entity + workflow — one place or four? | 2–6 | Every payment or vendor integration; easy to scatter. |
| 9 | **Entity-local types:** enums and values used by one entity — dossier or `values/`? | 1–6 | Decided on every new status enum. |
| 10 | **Events and subscribers:** event family in the emitting dossier or `events/`; subscribers by family, by reaction, or by channel? | 2–6 | Fan-out grows faster than entities (~120 subscribers in S6). |
| 11 | **Jobs:** one per file or a schedule file; does a single-entity job's action stay in the dossier? | 1–6 | 2 to 45 jobs; the job/action split is decided every time. |
| 12 | **Tests and fixtures:** `tests/` mirror, colocated, or root `app.jadpo`; where do data fixtures (signed payloads, provider recordings) go? | 1–6 | Every change adds tests; data fixtures have no home. |
| 13 | **Pure functions shared by two entities** (`calculate_totals`, `refund_for_cancellation`): there is no `functions/` role. | 2, 4, 5, 6 | Appears as soon as two entities share a rule. |
| 14 | **Cross-cutting policy:** tenancy, staff roles, editorial roles across many entities; human-owned policy file versus entity policy blocks. | 1, 3, 5, 6 | Security-relevant; repeated blocks drift. |
| 15 | **Child entities** (OrderLine, CartLine, ArticleVersion): forced into separate dossiers or grouped with the parent? | 2, 4, 5, 6 | Doubles file count for aggregate-shaped domains. |
| 16 | **Cross-entity reads in a dossier:** the threshold for moving to `queries/` (`User.with_todos`). | 1, 2, 5, 6 | Silent drift; the golden app already has one. |
| 17 | **Non-Jadpo assets:** email/RSS templates, search mappings, provider contracts, generated OpenAPI. | 1, 2, 4, 5, 6 | Closed layout must name a home or reject them. |
| 18 | **Domain grouping at scale:** flat role folders versus domain subfolders; CODEOWNERS. | 4, 6 (3 partly) | Bites only large apps, but it is the hardest to change later. |
| 19 | **Multiple deployables from one source:** API + worker + back-office selecting routes, jobs, subscribers and config. | 3, 4, 6 | Needed as soon as a worker or back-office is split out. |
| 20 | **Ownership mechanism:** folders versus modules, given the event model needs component owners and area 1 proposes dropping modules. | 3, 6 | Blocks the event design and team boundaries. |
| 21 | **Global name uniqueness pressure:** Address entity vs AddressSnapshot, `LineItem` in two domains, AuditEntry vs AuditEvent. | 2, 3, 6 | Forces naming conventions the layout cannot express. |
| 22 | **API versioning:** v1/v2 routes and contract values side by side, and removal. | 3, 6 | Rare to start, permanent once present. |
| 23 | **Cross-domain workflows:** which team's folder, who reviews. | 4, 6 | Only at scale, but every ledger or fulfilment change. |
| 24 | **Reusable libraries across apps** (payments in S2 and S4): where a library sits and how it is versioned. | 2, 4, 6 | Needs packages, which are undesigned. |
| 25 | **Authored data migrations and backfills** alongside compiler-owned schema output. | 5, 6 | Infrequent, but has no home at all today. |

Items 1–12 bite in nearly every scenario and should be settled before any
layout is enforced. Items 18–20 are the decisions most expensive to reverse:
they determine whether the layout is role-first, domain-first or a hybrid,
and whether modules survive.
