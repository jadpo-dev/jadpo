//! Area 1, "Files and names" (decided 2026-10-16, chunk 5.3d): a private
//! function or type needs a name unique only within its domain, and
//! everything else stays unique across the project. Each domain's code
//! reads its own private names; the identity key of a function or type
//! declared in a domain carries the domain (area 18, question 8).

use crate::support;

use jadpo_db::{Database, Layout};
use support::{APP, problems};

/// Two domains, each with a private `Address` type and a private
/// `format_address` helper of its own, with different fields.
const TWO_ADDRESSES: &[(&str, &str)] = &[
    APP,
    (
        "shared/types/label.jadpo",
        "type Label = Text {\n    max_length: 200\n}\n",
    ),
    (
        "domains/billing/actions/resend_invoice.jadpo",
        "use billing/types { Address }\n\naction resend_invoice(address: Address) -> Unit {}\n",
    ),
    (
        "domains/shipping/actions/return_parcel.jadpo",
        "use shipping/types { Address }\n\naction return_parcel(address: Address) -> Unit {}\n",
    ),
    (
        "domains/billing/types/address.jadpo",
        "use shared/types { Label }\n\ntype Address = Object {\n    street: Label\n}\n",
    ),
    (
        "domains/billing/actions/send_invoice.jadpo",
        "use shared/types { Label }\nuse billing/types { Address }\n\naction send_invoice(address: Address) -> Label {\n    return format_address(address)\n}\n\nfunction format_address(address: Address) -> Label {\n    return address.street\n}\n",
    ),
    (
        "domains/shipping/types/address.jadpo",
        "use shared/types { Label }\n\ntype Address = Object {\n    line_one: Label\n    postcode: Label\n}\n",
    ),
    (
        "domains/shipping/actions/ship_parcel.jadpo",
        "use shared/types { Label }\nuse shipping/types { Address }\n\naction ship_parcel(address: Address) -> Label {\n    return format_address(address)\n}\n\nfunction format_address(address: Address) -> Label {\n    return address.postcode\n}\n",
    ),
];

fn with(extra: &[(&str, &str)]) -> Vec<String> {
    let mut files = TWO_ADDRESSES.to_vec();
    for (path, text) in extra {
        files.retain(|(p, _)| p != path);
        files.push((path, text));
    }
    problems(Layout::Project, &files)
}

#[test]
fn private_functions_and_types_may_share_a_name_across_domains() {
    assert_eq!(
        problems(Layout::Project, TWO_ADDRESSES),
        Vec::<String>::new()
    );
}

#[test]
fn each_domain_reads_its_own_private_names() {
    // `shipping`'s `Address` has no `street`: its helper reads its own type.
    assert_eq!(
        with(&[(
            "domains/shipping/actions/ship_parcel.jadpo",
            "use shared/types { Label }\nuse shipping/types { Address }\n\naction ship_parcel(address: Address) -> Label {\n    return format_address(address)\n}\n\nfunction format_address(address: Address) -> Label {\n    return address.street\n}\n",
        )]),
        [
            "domains/shipping/actions/ship_parcel.jadpo type.unknown_field: `Address` has no field `street`"
        ]
    );
}

#[test]
fn a_private_name_twice_in_one_domain_is_a_duplicate() {
    let found = with(&[(
        "domains/billing/actions/send_invoice.jadpo",
        "use shared/types { Label }\nuse billing/types { Address }\n\naction send_invoice(address: Address) -> Label {\n    return format_address(address)\n}\n\nfunction format_address(address: Address) -> Label {\n    return address.street\n}\n\ntype Address = Object {\n    street: Label\n    town: Label\n}\n",
    )]);
    assert!(
        found.contains(
            &"domains/billing/types/address.jadpo semantic.duplicate_declaration: `Address` is declared twice in `billing`".to_string()
        ),
        "{found:#?}"
    );
}

#[test]
fn a_project_wide_name_clashes_with_a_private_one() {
    // `public` makes `shipping`'s `Address` unique across the project.
    let found = with(&[(
        "domains/shipping/types/address.jadpo",
        "use shared/types { Label }\n\npublic type Address = Object {\n    line_one: Label\n    postcode: Label\n}\n",
    )]);
    assert!(
        found.contains(
            &"domains/shipping/types/address.jadpo semantic.duplicate_declaration: `Address` is declared twice".to_string()
        ),
        "{found:#?}"
    );
}

#[test]
fn every_other_kind_stays_unique_across_the_project() {
    let failure = "type Declined = Failure {\n    kind: FailureKind.rejected\n}\n";
    let found = with(&[
        ("domains/billing/failures/declined.jadpo", failure),
        ("domains/shipping/failures/declined.jadpo", failure),
    ]);
    assert!(
        found.contains(
            &"domains/shipping/failures/declined.jadpo semantic.duplicate_declaration: `Declined` is declared twice".to_string()
        ),
        "{found:#?}"
    );
    let found = with(&[
        (
            "domains/billing/actions/refund.jadpo",
            "action refund() -> Unit {}\n",
        ),
        (
            "domains/shipping/actions/refund.jadpo",
            "action refund() -> Unit {}\n",
        ),
    ]);
    assert!(
        found.contains(
            &"domains/shipping/actions/refund.jadpo semantic.duplicate_declaration: `refund` is declared twice".to_string()
        ),
        "{found:#?}"
    );
}

#[test]
fn a_type_an_apis_record_names_stays_unique_across_the_project() {
    let found = with(&[
        (
            "domains/billing/routes/invoices.jadpo",
            "use billing/types { Address }\n\ntype InvoiceView = Output {\n    address: Address\n}\n\nroutes Invoices {\n    prefix: /invoices\n\n    GET / {\n        output: InvoiceView\n        run: invoice_view()\n    }\n}\n",
        ),
        (
            "domains/billing/queries/invoice_view.jadpo",
            "query invoice_view() -> InvoiceView {}\n",
        ),
        (
            "domains/shipping/routes/parcels.jadpo",
            "use shipping/types { Address }\n\ntype ParcelView = Output {\n    address: Address\n}\n\nroutes Parcels {\n    prefix: /parcels\n\n    GET / {\n        output: ParcelView\n        run: parcel_view()\n    }\n}\n",
        ),
    ]);
    assert!(
        found.iter().any(|problem| problem
            .contains("semantic.duplicate_declaration: `Address` is declared twice")),
        "{found:#?}"
    );
}

#[test]
fn names_folding_together_clash_only_in_the_same_scope() {
    let due = "type DueDate = CalendarDate {}\n";
    let helper = "use billing/types { Due }\n\naction plan_payment(due: Due) -> Due {\n    return due_date(due)\n}\n\nfunction due_date(due: Due) -> Due {\n    return due\n}\n";
    let found = with(&[
        ("domains/shipping/types/due_date.jadpo", due),
        (
            "domains/shipping/actions/use_due.jadpo",
            "use shipping/types { DueDate }\n\naction use_due(due: DueDate) -> DueDate {\n    return due\n}\n",
        ),
        (
            "domains/billing/types/due.jadpo",
            "type Due = CalendarDate {}\n",
        ),
        ("domains/billing/actions/plan_payment.jadpo", helper),
    ]);
    assert_eq!(found, Vec::<String>::new());
    let found = with(&[
        ("domains/billing/types/due_date.jadpo", due),
        (
            "domains/billing/actions/use_due.jadpo",
            "use billing/types { DueDate }\n\naction use_due(due: DueDate) -> DueDate {\n    return due\n}\n",
        ),
        (
            "domains/billing/types/due.jadpo",
            "type Due = CalendarDate {}\n",
        ),
        ("domains/billing/actions/plan_payment.jadpo", helper),
    ]);
    assert!(
        found
            .iter()
            .any(|problem| problem.contains("naming.folded_collision")),
        "{found:#?}"
    );
}

#[test]
fn another_domains_private_name_is_still_private() {
    let found = with(&[(
        "domains/ordering/actions/checkout.jadpo",
        "use billing/types { Address }\n\naction checkout(address: Address) -> Unit {}\n",
    )]);
    assert!(
        found
            .iter()
            .any(|problem| problem.contains("visibility.private_cross_domain")),
        "{found:#?}"
    );
}

#[test]
fn a_duplicate_says_copy_or_clash_and_lists_the_names_in_use() {
    let db = Database::default();
    let mut files = TWO_ADDRESSES.to_vec();
    files.push((
        "domains/shipping/types/shipping_address.jadpo",
        "use shared/types { Label }\n\ntype ShippingAddress = Object {\n    line_one: Label\n}\n",
    ));
    files.push((
        "domains/shipping/actions/print_label.jadpo",
        "use shared/types { Label }\nuse shipping/types { ShippingAddress }\n\naction print_label(address: ShippingAddress) -> Label {\n    return address.line_one\n}\n",
    ));
    files.push((
        "domains/billing/actions/remind.jadpo",
        "use shared/types { Label }\n\naction remind(address: Address) -> Label {\n    return address.street\n}\n\ntype Address = Object {\n    street: Label\n}\n",
    ));
    let project = support::project(&db, Layout::Project, &files);
    let found: Vec<_> = jadpo_check::check(&db, project)
        .into_iter()
        .filter(|d| d.rule.id == "semantic.duplicate_declaration")
        .collect();
    assert_eq!(found.len(), 1, "{found:#?}");
    let copy = &found[0];
    assert_eq!(copy.message, "`Address` is declared twice in `billing`");
    let reason = copy.reason.as_deref().unwrap_or_default();
    assert!(reason.contains("a copy"), "{reason}");
    let rows: Vec<(String, String)> = copy.in_use.clone();
    assert_eq!(
        rows,
        [
            (
                "Address".to_string(),
                "domains/billing/actions/remind.jadpo".to_string()
            ),
            (
                "Address".to_string(),
                "domains/shipping/types/address.jadpo".to_string()
            ),
            (
                "ShippingAddress".to_string(),
                "domains/shipping/types/shipping_address.jadpo".to_string()
            ),
            (
                "format_address".to_string(),
                "domains/billing/actions/send_invoice.jadpo".to_string()
            ),
            (
                "format_address".to_string(),
                "domains/shipping/actions/ship_parcel.jadpo".to_string()
            ),
        ]
    );
}

#[test]
fn the_identity_of_a_function_or_type_in_a_domain_carries_the_domain() {
    let db = Database::default();
    let project = support::project(&db, Layout::Project, TWO_ADDRESSES);
    let graph = jadpo_check::graph::graph(&db, project);
    let keys: Vec<String> = graph
        .nodes()
        .iter()
        .map(|node| node.key.to_string())
        .collect();
    for key in [
        "type billing.Address",
        "type shipping.Address",
        "field billing.Address.street",
        "field shipping.Address.postcode",
        "function billing.format_address",
        "function shipping.format_address",
        "action send_invoice",
        "action ship_parcel",
    ] {
        assert!(keys.iter().any(|k| k == key), "{key} not in {keys:#?}");
    }
}

#[test]
fn a_registry_move_line_keeps_a_promoted_types_identity() {
    let db = Database::default();
    let files: &[(&str, &str)] = &[
        APP,
        (
            "shared/types/address.jadpo",
            "type Address = Object {\n    street: Label\n}\n",
        ),
        (
            "domains/billing/actions/send_invoice.jadpo",
            "use shared/types { Address }\n\naction send_invoice(address: Address) -> Text {\n    return address.street\n}\n",
        ),
        (
            "domains/shipping/actions/ship_parcel.jadpo",
            "use shared/types { Address }\n\naction ship_parcel(address: Address) -> Text {\n    return address.street\n}\n",
        ),
    ];
    let project = support::project(&db, Layout::Project, files);
    let mut lines = std::collections::BTreeMap::new();
    lines.insert("type Address".to_string(), "billing.Address".to_string());
    use salsa::Setter;
    let mut db = db;
    project.set_identities(&mut db).to(lines);
    let graph = jadpo_check::graph::graph(&db, project);
    let keys: Vec<String> = graph
        .nodes()
        .iter()
        .map(|node| node.key.to_string())
        .collect();
    assert!(
        keys.iter().any(|k| k == "type billing.Address"),
        "{keys:#?}"
    );
    assert!(
        keys.iter().any(|k| k == "field billing.Address.street"),
        "{keys:#?}"
    );
}

#[test]
fn promotion_to_shared_writes_a_registry_move_line() {
    let db = Database::default();
    let files: &[(&str, &str)] = &[
        APP,
        (
            "domains/billing/types/address.jadpo",
            "public type Address = Object {\n    street: Text\n}\n",
        ),
        (
            "domains/billing/actions/send_invoice.jadpo",
            "use billing/types { Address }\n\naction send_invoice(address: Address) -> Unit {}\n",
        ),
        (
            "domains/shipping/actions/ship_parcel.jadpo",
            "use billing/types { Address }\n\naction ship_parcel(address: Address) -> Unit {}\n",
        ),
    ];
    let project = support::project(&db, Layout::Project, files);
    let found: Vec<_> = jadpo_check::check(&db, project)
        .into_iter()
        .filter(|d| d.rule.id == "layout.promotion_required")
        .collect();
    assert_eq!(found.len(), 1, "{found:#?}");
    let edits = &found[0].next.as_ref().expect("a next step").edits;
    let registry: Vec<_> = edits
        .iter()
        .filter(|edit| edit.span.path == "database/identities.jadpo_record")
        .collect();
    assert_eq!(registry.len(), 1, "{edits:#?}");
    assert_eq!(
        registry[0].replacement,
        "// Generated by Jadpo from renames and moves; update it with `jadpo rename`, `jadpo domain` or `jadpo fmt --layout`, never by hand.\nrecord identities 1\n\ntype billing.Address -> Address\n"
    );
    assert_eq!((registry[0].span.start, registry[0].span.end), (0, 0));
}
