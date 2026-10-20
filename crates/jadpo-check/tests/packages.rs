//! Packages (area 11, "The package format", "Where packages live",
//! "Names" and "Installing safely"; chunk 7.1): a library package
//! installed in `packages/` and locked by its digest, the app's own
//! integration in `shared/providers/`, names bare inside a package and
//! written through it outside, and each mistake the format, the names and
//! the lock can hold.

use crate::support;

use jadpo_db::{Database, Layout, Locked, Source, package_digest, write_lock};
use support::{APP, project_with_records};

const INDEX: (&str, &str) = (
    "packages/postcodes/v2/package.jadpo",
    "/// UK postcodes: reading and normalising them.
package Postcodes.v2 {
    licence: Licence.mit
    jadpo: \"1.0\"
}
",
);

const POSTCODE: (&str, &str) = (
    "packages/postcodes/v2/types/uk_postcode.jadpo",
    "/// A UK postcode in standard form, such as \"SW1A 1AA\".
public type UkPostcode = Text {
    min_length: 5
    max_length: 8
}
",
);

const INVALID: (&str, &str) = (
    "packages/postcodes/v2/failures/invalid_postcode.jadpo",
    "/// The text isn't a UK postcode.
public type InvalidPostcode = Failure {
    kind: FailureKind.invalid_value
}
",
);

const WRITTEN: (&str, &str) = (
    "packages/postcodes/v2/types/written_postcode.jadpo",
    "/// A postcode as someone typed it.
public type WrittenPostcode = Text {
    max_length: 20
}
",
);

const UK: (&str, &str) = (
    "packages/postcodes/v2/libraries/uk.jadpo",
    "/// Reading UK postcodes.
public library uk {
    /// Reads a UK postcode as it is written.
    function parse(written: WrittenPostcode) fails InvalidPostcode -> UkPostcode {
        return attempt UkPostcode(written) {
            invalid: InvalidPostcode
        }
    }
}
",
);

const GUIDE: (&str, &str) = (
    "packages/postcodes/v2/GUIDE.md",
    "# Postcodes\n\nParse first, then store the `UkPostcode`.\n",
);

/// The app's action that reads a postcode through the package.
const SAVE: (&str, &str) = (
    "domains/addresses/actions/read_postcode.jadpo",
    "use packages/postcodes

/// Reads a customer's postcode.
action read_postcode(
    written: Postcodes.WrittenPostcode
) fails Postcodes.InvalidPostcode -> Postcodes.UkPostcode {
    return attempt Postcodes.uk.parse(written)
}
",
);

/// The package's files, as `jadpo package add` copies them in.
fn library() -> Vec<(&'static str, &'static str)> {
    vec![INDEX, POSTCODE, INVALID, WRITTEN, UK]
}

/// The lock line for the package at `folder`, from `files` and `data`.
fn lock_for(package: &str, folder: &str, files: &[(&str, &str)], data: &[(&str, &str)]) -> String {
    let prefix = format!("{folder}/");
    let held: Vec<(&str, &[u8])> = files
        .iter()
        .chain(data.iter())
        .filter_map(|(path, text)| Some((path.strip_prefix(&prefix)?, text.as_bytes())))
        .collect();
    write_lock(&[Locked {
        package: package.to_string(),
        source: Source::Path {
            version: "2.1.0".to_string(),
            path: "../postcodes".to_string(),
            digest: package_digest(&held),
        },
    }])
}

/// Each problem as `path rule: message`, for `files` with `records`.
fn problems(files: &[(&str, &str)], records: &[(&str, &str)]) -> Vec<String> {
    let db = Database::default();
    let project = project_with_records(&db, Layout::Project, files, records);
    jadpo_check::check(&db, project)
        .iter()
        .map(|d| {
            let path = d
                .primary
                .as_ref()
                .map_or("", |label| label.span.path.as_str());
            format!("{path} {}: {}", d.rule.id, d.message)
        })
        .collect()
}

/// `files` and the package's data, locked as installed.
fn installed(files: &[(&str, &str)]) -> Vec<String> {
    let lock = lock_for("Postcodes.v2", "packages/postcodes/v2", files, &[GUIDE]);
    problems(files, &[GUIDE, ("jadpo.lock", &lock)])
}

fn with(extra: &[(&'static str, &'static str)]) -> Vec<(&'static str, &'static str)> {
    let mut files = vec![APP];
    files.extend(library());
    files.extend_from_slice(extra);
    files
}

#[test]
fn an_installed_library_package_is_used_through_its_name() {
    assert_eq!(installed(&with(&[SAVE])), Vec::<String>::new());
}

#[test]
fn the_app_s_own_integration_needs_no_lock() {
    let files = [
        APP,
        (
            "shared/providers/acme_couriers/v1/package.jadpo",
            "/// Acme Couriers' booking API, written from its documentation.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: \"1.0\"
}
",
        ),
        (
            "shared/providers/acme_couriers/v1/provider.jadpo",
            "/// Acme Couriers' booking and tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: \"https://api.acme-couriers.example\"
        sandbox: \"https://sandbox.acme-couriers.example\"
    }
    credential: CredentialScheme.bearer
    idempotency: {
        header: \"Idempotency-Key\"
        window: 24h
    }
}
",
        ),
        (
            "shared/providers/acme_couriers/v1/resources/bookings.jadpo",
            "public resource bookings {
    create: {
        method: HttpMethod.post
        path: \"/v1/bookings\"
        input: CreateBooking
        output: Booking
        changes: true
        idempotent: true
    }
}
",
        ),
        (
            "shared/providers/acme_couriers/v1/types/create_booking.jadpo",
            "/// What booking a collection takes.
public type CreateBooking = Object {
    postcode: Text
    slot: Text
}
",
        ),
        (
            "shared/providers/acme_couriers/v1/types/booking.jadpo",
            "/// A booked collection.
public type Booking = Object {
    id: Text
}
",
        ),
    ];
    assert_eq!(problems(&files, &[]), Vec::<String>::new());
}

#[test]
fn names_inside_a_package_are_its_own() {
    // The app's own `UkPostcode` and the package's are two types.
    let mine = (
        "domains/addresses/types/uk_postcode.jadpo",
        "type UkPostcode = Text {\n    max_length: 10\n}\n",
    );
    let uses = (
        "domains/addresses/actions/keep_postcode.jadpo",
        "use addresses/types { UkPostcode }

action keep_postcode(postcode: UkPostcode) -> UkPostcode {
    return postcode
}
",
    );
    assert_eq!(installed(&with(&[SAVE, mine, uses])), Vec::<String>::new());
}

#[test]
fn a_package_name_needs_its_use_line() {
    let save = (
        SAVE.0,
        "/// Reads a customer's postcode.
action read_postcode(
    written: Postcodes.WrittenPostcode
) fails Postcodes.InvalidPostcode -> Postcodes.UkPostcode {
    return attempt Postcodes.uk.parse(written)
}
",
    );
    let found = installed(&with(&[save]));
    assert!(
        found.iter().any(|p| p.contains("import.missing")),
        "{found:#?}"
    );
}

#[test]
fn only_what_a_package_makes_public_is_seen_outside_it() {
    let hidden = (
        "packages/postcodes/v2/types/outward_code.jadpo",
        "/// The outward half of a postcode.
type OutwardCode = Text {
    max_length: 4
}
",
    );
    let uses = (
        "domains/addresses/actions/outward.jadpo",
        "use packages/postcodes

action outward(code: Postcodes.OutwardCode) -> Postcodes.OutwardCode {
    return code
}
",
    );
    let found = installed(&with(&[hidden, uses]));
    assert!(
        found.iter().any(|p| p.contains("package.not_public")),
        "{found:#?}"
    );
}

#[test]
fn a_public_signature_names_only_public_types() {
    let hidden = (
        "packages/postcodes/v2/types/outward_code.jadpo",
        "/// The outward half of a postcode.
type OutwardCode = Text {
    max_length: 4
}
",
    );
    let lib = (
        UK.0,
        "/// Reading UK postcodes.
public library uk {
    /// The outward code of a postcode.
    function outward(postcode: UkPostcode) -> OutwardCode {
        return attempt OutwardCode(postcode) {
            invalid: OutwardCode(\"?\")
        }
    }
}
",
    );
    let found = installed(&[APP, INDEX, POSTCODE, INVALID, lib, hidden]);
    assert!(
        found
            .iter()
            .any(|p| p.contains("visibility.public_reaches_private")),
        "{found:#?}"
    );
}

#[test]
fn a_domain_s_public_action_names_only_public_types() {
    let files = [
        APP,
        (
            "domains/billing/types/amount.jadpo",
            "type Amount = Int {\n    min: 0\n}\n",
        ),
        (
            "domains/billing/actions/charge.jadpo",
            "use billing/types { Amount }

public action charge(amount: Amount) -> Unit {}
",
        ),
        (
            "domains/shop/actions/pay.jadpo",
            "use billing

action pay() -> Unit {
    billing.charge(5)
}
",
        ),
    ];
    let found = problems(&files, &[]);
    assert!(
        found
            .iter()
            .any(|p| p.contains("visibility.public_reaches_private: `charge`")),
        "{found:#?}"
    );
}

#[test]
fn an_installed_package_is_held_to_the_lock() {
    // No line for it.
    let files = with(&[]);
    let found = problems(&files, &[]);
    assert!(
        found
            .iter()
            .any(|p| p == "packages/postcodes/v2/package.jadpo package.not_locked: Package `Postcodes.v2` isn't in `jadpo.lock`"),
        "{found:#?}"
    );
    // A file changed after it was installed.
    let lock = lock_for("Postcodes.v2", "packages/postcodes/v2", &files, &[]);
    let mut edited = files.clone();
    edited[2] = (
        POSTCODE.0,
        "/// A UK postcode.\npublic type UkPostcode = Text {\n    max_length: 9\n}\n",
    );
    let found = problems(&edited, &[("jadpo.lock", &lock)]);
    assert!(
        found.iter().any(|p| p.contains(
            "package.digest_mismatch: Package `Postcodes.v2` differs from what was installed"
        )),
        "{found:#?}"
    );
    // A file added: the guide, which the digest covers too.
    let found = problems(&files, &[GUIDE, ("jadpo.lock", &lock)]);
    assert!(
        found.iter().any(|p| p.contains("package.digest_mismatch")),
        "{found:#?}"
    );
    // A line with no package.
    let other = lock_for("Tables.v1", "packages/tables/v1", &[], &[]);
    let found = problems(&[APP], &[("jadpo.lock", &other)]);
    assert_eq!(
        found,
        ["jadpo.lock package.not_installed: `jadpo.lock` names `Tables.v1`, which isn't installed"]
    );
}

#[test]
fn a_proprietary_package_comes_from_git() {
    let index = (
        INDEX.0,
        "/// UK postcodes: reading and normalising them.
package Postcodes.v2 {
    licence: Licence.proprietary
    jadpo: \"1.0\"
}
",
    );
    let files = vec![APP, index, POSTCODE, INVALID, WRITTEN, UK];
    let found = installed(&files);
    assert!(
        found
            .iter()
            .any(|p| p.contains("package.licence_not_allowed")),
        "{found:#?}"
    );
}

#[test]
fn the_index_holds_its_description_licence_and_version_only() {
    let index = (
        INDEX.0,
        "package Postcodes.v2 {
    licence: Licence.gpl
    requires: [Tables.v1]
}
",
    );
    let files = vec![APP, index, POSTCODE, INVALID, UK];
    let found = installed(&files);
    for expected in [
        "package.index_invalid: Package `Postcodes.v2` has no description",
        "package.index_invalid: `Licence.gpl` isn't a package's `licence:`",
        "package.index_invalid: A package's index can't hold `requires:`",
        "package.index_invalid: Package `Postcodes.v2` doesn't give `jadpo:`",
    ] {
        assert!(
            found.iter().any(|p| p.contains(expected)),
            "{expected}: {found:#?}"
        );
    }
    let renamed = (
        INDEX.0,
        "/// UK postcodes.\npackage Postcode.v3 {\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}\n",
    );
    let found = installed(&[APP, renamed, POSTCODE, INVALID, UK]);
    assert!(
        found.iter().any(|p| p
            .contains("package.name_mismatch: `Postcode.v3` is in the folder of `Postcodes.v2`")),
        "{found:#?}"
    );
    let missing = problems(&[APP, POSTCODE], &[]);
    assert!(
        missing
            .iter()
            .any(|p| p.contains("package.index_invalid: Package `Postcodes.v2` has no index")),
        "{missing:#?}"
    );
}

#[test]
fn a_package_holds_no_email_templates() {
    let email = (
        "packages/postcodes/v2/emails/welcome.jadpo",
        "type Welcome = EmailMessage {}\n",
    );
    let found = installed(&with(&[email]));
    assert!(
        found
            .iter()
            .any(|p| p.contains("package.emails_folder: A package can't hold email templates")),
        "{found:#?}"
    );
}

fn provider(text: &'static str) -> Vec<(&'static str, &'static str)> {
    vec![
        APP,
        (
            "shared/providers/acme_couriers/v1/package.jadpo",
            "/// Acme Couriers.\npackage AcmeCouriers.v1 {\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}\n",
        ),
        ("shared/providers/acme_couriers/v1/provider.jadpo", text),
    ]
}

#[test]
fn provider_jadpo_has_a_fixed_set_of_keys() {
    let found = problems(
        &provider(
            "provider {
    protocl: Protocol.json_http
    hosts: {
        live: \"https://api.acme.example\"
        staging: \"https://staging.acme.example\"
    }
    runs: {
        image: \"acme/api:1\"
    }
}
",
        ),
        &[],
    );
    for expected in [
        "package.provider_invalid: `provider` has no key `protocl:`",
        "package.provider_invalid: `hosts:` has no key `staging:`",
        "package.provider_invalid: `provider` doesn't give `protocol:`",
        "package.provider_invalid: `provider` gives both `hosts:` and `runs:`",
    ] {
        assert!(
            found.iter().any(|p| p.contains(expected)),
            "{expected}: {found:#?}"
        );
    }
}

#[test]
fn a_package_with_resources_is_a_provider() {
    let mut files = vec![APP];
    files.push((
        "shared/providers/acme_couriers/v1/package.jadpo",
        "/// Acme Couriers.\npackage AcmeCouriers.v1 {\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}\n",
    ));
    files.push((
        "shared/providers/acme_couriers/v1/resources/bookings.jadpo",
        "public resource bookings {}\n",
    ));
    let found = problems(&files, &[]);
    assert!(
        found.iter().any(|p| p.contains("package.provider_missing")),
        "{found:#?}"
    );
    let mut files = provider("provider {\n    protocol: Protocol.json_http\n}\n");
    files.push((
        "shared/providers/acme_couriers/v1/translations/booking_hint.jadpo",
        "/// Shown under a booking.\ntype BookingHint = Message {}\n",
    ));
    let found = problems(&files, &[]);
    assert!(
        found
            .iter()
            .any(|p| p.contains("package.provider_translations")),
        "{found:#?}"
    );
}

#[test]
fn the_app_s_own_integration_takes_no_reserved_name() {
    let files = [
        APP,
        (
            "shared/providers/stripe/v1/package.jadpo",
            "/// Our Stripe calls.\npackage Stripe.v1 {\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}\n",
        ),
    ];
    let found = problems(&files, &[]);
    assert_eq!(
        found,
        [
            "shared/providers/stripe/v1/package.jadpo package.reserved_name: `Stripe` is a reserved provider name"
        ]
    );
}

#[test]
fn a_package_s_name_is_not_the_app_s() {
    let domain = (
        "domains/postcodes/actions/look_up.jadpo",
        "action look_up() -> Unit {}\n",
    );
    let found = installed(&with(&[domain]));
    assert!(
        found
            .iter()
            .any(|p| p.contains("package.name_taken: Package `Postcodes`'s name is already taken")),
        "{found:#?}"
    );
}

#[test]
fn a_package_s_tests_and_recipes_are_data() {
    let test = (
        "packages/postcodes/v2/libraries/uk.test.jadpo",
        "test \"a postcode is read\" {\n    assert uk.parse(\"SW1A 1AA\") == \"SW1A 1AA\"\n}\n",
    );
    let recipe = (
        "packages/postcodes/v2/recipes/check_address/check_address.jadpo",
        "this would not parse as a program",
    );
    let files = with(&[]);
    let lock = lock_for(
        "Postcodes.v2",
        "packages/postcodes/v2",
        &files,
        &[GUIDE, test, recipe],
    );
    assert_eq!(
        problems(&files, &[GUIDE, test, recipe, ("jadpo.lock", &lock)]),
        Vec::<String>::new()
    );
}
