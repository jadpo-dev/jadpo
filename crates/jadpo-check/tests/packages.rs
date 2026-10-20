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

#[test]
fn a_package_s_code_sees_only_its_own_names() {
    let pence = (
        "shared/types/pence.jadpo",
        "type Pence = Int {\n    min: 0\n}\n",
    );
    let uses = (
        "packages/postcodes/v2/types/fee.jadpo",
        "/// A fee.\npublic type Fee = Object {\n    amount: Pence\n}\n",
    );
    let found = installed(&with(&[pence, uses]));
    assert!(
        found.iter().any(|p| p.contains(
            "packages/postcodes/v2/types/fee.jadpo semantic.unknown_name: `Pence` isn't declared"
        )),
        "{found:#?}"
    );
}

/// `Postcodes.v3` beside `Postcodes.v2`: the same files under the next
/// major version, as an upgrade copies them in.
fn version_three() -> Vec<(String, String)> {
    library()
        .into_iter()
        .map(|(path, text)| {
            (
                path.replace("packages/postcodes/v2/", "packages/postcodes/v3/"),
                text.replace("Postcodes.v2", "Postcodes.v3"),
            )
        })
        .collect()
}

/// The problems of `files` with both versions installed and locked.
fn both_installed(files: &[(&str, &str)]) -> Vec<String> {
    let three = version_three();
    let mut all: Vec<(&str, &str)> = files.to_vec();
    all.extend(three.iter().map(|(p, t)| (p.as_str(), t.as_str())));
    let held = |prefix: &str, extra: &[(&str, &str)]| -> String {
        let bytes: Vec<(&str, &[u8])> = all
            .iter()
            .chain(extra.iter())
            .filter_map(|(path, text)| Some((path.strip_prefix(prefix)?, text.as_bytes())))
            .collect();
        package_digest(&bytes)
    };
    let lock = write_lock(&[
        Locked {
            package: "Postcodes.v2".to_string(),
            source: Source::Path {
                version: "2.1.0".to_string(),
                path: "../postcodes".to_string(),
                digest: held("packages/postcodes/v2/", &[GUIDE]),
            },
        },
        Locked {
            package: "Postcodes.v3".to_string(),
            source: Source::Path {
                version: "3.0.0".to_string(),
                path: "../postcodes-3".to_string(),
                digest: held("packages/postcodes/v3/", &[]),
            },
        },
    ]);
    problems(&all, &[GUIDE, ("jadpo.lock", &lock)])
}

/// Two actions while callers move across: one still on `Postcodes.v2`,
/// one moved to `Postcodes.v3`.
const MOVING: (&str, &str) = (
    "domains/addresses/actions/read_postcode.jadpo",
    "use packages/postcodes

/// Reads a customer's postcode with the version it was written for.
action read_postcode(
    written: Postcodes.v2.WrittenPostcode
) fails Postcodes.v2.InvalidPostcode -> Postcodes.v2.UkPostcode {
    return attempt Postcodes.v2.uk.parse(written)
}
",
);

const MOVED: (&str, &str) = (
    "domains/addresses/actions/read_new_postcode.jadpo",
    "use packages/postcodes

/// Reads a customer's postcode with the next version.
action read_new_postcode(
    written: Postcodes.v3.WrittenPostcode
) fails Postcodes.v3.InvalidPostcode -> Postcodes.v3.UkPostcode {
    var postcode: Postcodes.v3.UkPostcode = attempt Postcodes.v3.uk.parse(written)
    return postcode
}
",
);

#[test]
fn two_versions_sit_side_by_side_with_names_carrying_the_version() {
    assert_eq!(both_installed(&with(&[MOVING, MOVED])), Vec::<String>::new());
}

#[test]
fn a_name_without_its_version_is_refused_while_two_are_held() {
    let found = both_installed(&with(&[SAVE]));
    assert!(
        found.iter().any(|p| p.starts_with(
            "domains/addresses/actions/read_postcode.jadpo package.version_needed: `Postcodes` is held in two versions"
        )),
        "{found:#?}"
    );
    assert!(
        found
            .iter()
            .all(|p| p.contains("package.version_needed")),
        "only the missing versions are reported: {found:#?}"
    );
}

#[test]
fn a_version_is_refused_while_one_is_held() {
    let versioned = (
        "domains/addresses/actions/read_postcode.jadpo",
        "use packages/postcodes

/// Reads a customer's postcode.
action read_postcode(
    written: Postcodes.WrittenPostcode
) fails Postcodes.InvalidPostcode -> Postcodes.v2.UkPostcode {
    return attempt Postcodes.uk.parse(written)
}
",
    );
    let found = installed(&with(&[versioned]));
    assert_eq!(
        found,
        vec![
            "domains/addresses/actions/read_postcode.jadpo package.version_unneeded: `Postcodes` is held in one version, so its names don't carry it"
                .to_string()
        ]
    );
}

#[test]
fn a_package_s_own_code_is_bare_beside_another_version() {
    // Each version's code still writes its own names bare.
    let found = both_installed(&with(&[]));
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn an_installed_package_and_the_app_s_own_still_never_share_a_name() {
    let own = (
        "shared/providers/postcodes/v3/package.jadpo",
        "/// The app's own postcodes.\npackage Postcodes.v3 {\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}\n",
    );
    let found = installed(&with(&[own]));
    assert!(
        found
            .iter()
            .any(|p| p.contains("package.name_taken: Package `Postcodes`'s name is already taken")),
        "{found:#?}"
    );
}

#[test]
fn the_graph_knows_each_version_s_declarations_apart() {
    let three = version_three();
    let mut all: Vec<(&str, &str)> = with(&[MOVING, MOVED]);
    all.extend(three.iter().map(|(p, t)| (p.as_str(), t.as_str())));
    let db = Database::default();
    let project = project_with_records(&db, Layout::Project, &all, &[GUIDE]);
    let graph = jadpo_check::graph::graph(&db, project);
    let keys: Vec<String> = graph
        .nodes()
        .iter()
        .map(|node| node.key.to_string())
        .filter(|key| key.contains("Postcodes"))
        .collect();
    for expected in [
        "function Postcodes.v2.uk.parse",
        "function Postcodes.v3.uk.parse",
        "type Postcodes.v2.UkPostcode",
        "type Postcodes.v3.UkPostcode",
    ] {
        assert!(keys.iter().any(|key| key == expected), "{expected}: {keys:#?}");
    }
}

/// The app's own declarations of every kind, named as the package's are,
/// all unique across the project: a type, a failure, an event, an action
/// and a library.
fn namesakes() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "shared/types/uk_postcode.jadpo",
            "type UkPostcode = Text {\n    max_length: 10\n}\n",
        ),
        (
            "shared/failures/invalid_postcode.jadpo",
            "type InvalidPostcode = Failure {\n    kind: FailureKind.invalid_value\n}\n",
        ),
        (
            "domains/addresses/events/written_postcode.jadpo",
            "use shared/types { UkPostcode }

/// A customer wrote a postcode.
type WrittenPostcode = Event {
    postcode: UkPostcode
}
",
        ),
        (
            "domains/addresses/actions/parse.jadpo",
            "use shared/types { UkPostcode }
use shared/failures { InvalidPostcode }
use shared/libraries { uk }

/// Reads a postcode in lower case.
action parse(postcode: UkPostcode) fails InvalidPostcode -> UkPostcode {
    return attempt uk.lower(postcode)
}
",
        ),
        (
            "shared/libraries/uk.jadpo",
            "use shared/types { UkPostcode }
use shared/failures { InvalidPostcode }

/// The app's own postcode helpers.
library uk {
    /// The postcode in lower case.
    function lower(postcode: UkPostcode) fails InvalidPostcode -> UkPostcode {
        return attempt UkPostcode(text.lower(postcode)) {
            invalid: InvalidPostcode
        }
    }
}
",
        ),
    ]
}

/// The problems of `files`, but for a missing record or migration, which
/// no test here writes.
fn installed_apart_from_records(files: &[(&str, &str)]) -> Vec<String> {
    installed(files)
        .into_iter()
        .filter(|problem| !problem.contains(".out_of_date:"))
        .collect()
}

#[test]
fn a_package_s_names_never_clash_with_the_app_s() {
    // `Postcodes.uk.parse` beside the app's `parse`, and each of the
    // package's types beside the app's of the same name (chunk 7.1d).
    let mut extra = vec![SAVE];
    extra.extend(namesakes());
    assert_eq!(
        installed_apart_from_records(&with(&extra)),
        Vec::<String>::new()
    );
}

#[test]
fn a_query_may_share_a_package_function_s_name() {
    let address = (
        "domains/addresses/entities/address.jadpo",
        "/// A delivery address.
entity Address {
    id: Uuid
    postcode: Text {
        max_length: 10
    }
}
",
    );
    let parse = (
        "domains/addresses/queries/parse.jadpo",
        "use addresses/entities { Address }

/// The addresses at a postcode.
query parse(postcode: Address.postcode) -> List<Address> {
    from: Address
    where: Address.postcode == postcode
    order: [Address.id asc]
    limit: 50
}
",
    );
    assert_eq!(
        installed_apart_from_records(&with(&[SAVE, address, parse])),
        Vec::<String>::new()
    );
}

#[test]
fn a_bare_name_in_the_app_is_the_app_s() {
    // The app's `UkPostcode` isn't the package's, so the package's can't
    // be returned as it.
    let mixed = (
        "domains/addresses/actions/keep.jadpo",
        "use packages/postcodes
use shared/types { UkPostcode }

/// Reads a postcode into the app's own type.
action keep(written: Postcodes.WrittenPostcode) fails Postcodes.InvalidPostcode -> UkPostcode {
    return attempt Postcodes.uk.parse(written)
}
",
    );
    let mut extra = vec![mixed];
    extra.extend(namesakes());
    let found = installed_apart_from_records(&with(&extra));
    assert!(
        found
            .iter()
            .any(|p| p.starts_with("domains/addresses/actions/keep.jadpo type.mismatch: `Postcodes.UkPostcode` can't be used where `UkPostcode` is required")),
        "{found:#?}"
    );
    // And the app's own call reads its own action.
    let own = (
        "domains/addresses/actions/keep.jadpo",
        "use addresses/actions { parse }
use shared/types { UkPostcode }
use shared/failures { InvalidPostcode }

/// Keeps a postcode through the app's own action.
action keep(postcode: UkPostcode) fails InvalidPostcode -> UkPostcode {
    return attempt parse(postcode)
}
",
    );
    let mut extra = vec![SAVE, own];
    extra.extend(namesakes());
    assert_eq!(
        installed_apart_from_records(&with(&extra)),
        Vec::<String>::new()
    );
}

#[test]
fn a_package_s_library_may_be_named_like_a_domain() {
    let library = (
        "packages/postcodes/v2/libraries/addresses.jadpo",
        "/// Reading addresses.
public library addresses {
    /// The postcode as it is.
    function postcode(written: WrittenPostcode) fails InvalidPostcode -> UkPostcode {
        return attempt uk.parse(written)
    }
}
",
    );
    assert_eq!(installed(&with(&[SAVE, library])), Vec::<String>::new());
}

#[test]
fn a_name_declared_twice_in_one_package_still_clashes() {
    let again = (
        "packages/postcodes/v2/types/postcode_again.jadpo",
        "/// A UK postcode again.\npublic type UkPostcode = Text {\n    max_length: 9\n}\n",
    );
    let found = installed(&with(&[again]));
    assert!(
        found
            .iter()
            .any(|p| p.contains("semantic.duplicate_declaration: `UkPostcode` is declared twice")),
        "{found:#?}"
    );
}

#[test]
fn two_packages_may_declare_the_same_names() {
    // `ZipCodes` declares every name `Postcodes` does.
    let zip = |path: &'static str| -> (&'static str, String) {
        let (from, text) = library()
            .into_iter()
            .find(|(from, _)| from.ends_with(path))
            .expect("a package file");
        let _ = from;
        let text = text.replace("Postcodes.v2", "ZipCodes.v1");
        (path, text)
    };
    let zipped: Vec<(String, String)> = [
        "package.jadpo",
        "types/uk_postcode.jadpo",
        "failures/invalid_postcode.jadpo",
        "types/written_postcode.jadpo",
        "libraries/uk.jadpo",
    ]
    .into_iter()
    .map(|path| {
        let (path, text) = zip(path);
        (format!("packages/zip_codes/v1/{path}"), text)
    })
    .collect();
    let both = (
        "domains/addresses/actions/read_zip.jadpo",
        "use packages/zip_codes

/// Reads a postcode with the other package.
action read_zip(
    written: ZipCodes.WrittenPostcode
) fails ZipCodes.InvalidPostcode -> ZipCodes.UkPostcode {
    return attempt ZipCodes.uk.parse(written)
}
",
    );
    let mut files: Vec<(&str, &str)> = with(&[SAVE, both]);
    files.extend(zipped.iter().map(|(p, t)| (p.as_str(), t.as_str())));
    let held = |prefix: &str| -> Vec<(String, Vec<u8>)> {
        files
            .iter()
            .filter_map(|(path, text)| {
                Some((
                    path.strip_prefix(prefix)?.to_string(),
                    text.as_bytes().to_vec(),
                ))
            })
            .chain(
                (prefix == "packages/postcodes/v2/")
                    .then(|| ("GUIDE.md".to_string(), GUIDE.1.as_bytes().to_vec())),
            )
            .collect()
    };
    let digest = |prefix: &str| {
        let held = held(prefix);
        let refs: Vec<(&str, &[u8])> = held
            .iter()
            .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
            .collect();
        package_digest(&refs)
    };
    let lock = write_lock(&[
        Locked {
            package: "Postcodes.v2".to_string(),
            source: Source::Path {
                version: "2.1.0".to_string(),
                path: "../postcodes".to_string(),
                digest: digest("packages/postcodes/v2/"),
            },
        },
        Locked {
            package: "ZipCodes.v1".to_string(),
            source: Source::Path {
                version: "1.0.0".to_string(),
                path: "../zip_codes".to_string(),
                digest: digest("packages/zip_codes/v1/"),
            },
        },
    ]);
    assert_eq!(
        problems(&files, &[GUIDE, ("jadpo.lock", &lock)]),
        Vec::<String>::new()
    );
}

/// An app whose one connection is called `name`.
fn connected_as(name: &str) -> String {
    format!(
        "application {{
    connections: {{
        {name}: {{
            driver: Driver.postgres
            url: config.database_url
        }}
    }}
}}
"
    )
}

#[test]
fn a_connection_is_never_named_like_a_domain_or_a_library() {
    // Area 11, question 17: connections share one namespace with domains
    // and libraries (chunk 7.1c2).
    let config = (
        "config.jadpo",
        "config {\n    database_url: Text {\n        secret: true\n    }\n}\n",
    );
    let domain = (
        "domains/billing/actions/bill.jadpo",
        "/// Bills.\naction bill() -> Unit {}\n",
    );
    let app = connected_as("billing");
    let found = problems(&[("app.jadpo", &app), config, domain], &[]);
    assert!(
        found.iter().any(|p| p
            == "app.jadpo semantic.duplicate_declaration: Connection `billing` is named like domain `billing`"),
        "{found:#?}"
    );
    let library = (
        "shared/libraries/rounding.jadpo",
        "/// Rounding.\nlibrary rounding {\n    /// Keeps it.\n    function keep(value: Int) -> Int {\n        return value\n    }\n}\n",
    );
    let app = connected_as("rounding");
    let found = problems(&[("app.jadpo", &app), config, library], &[]);
    assert!(
        found.iter().any(|p| p
            == "app.jadpo semantic.duplicate_declaration: Connection `rounding` is named like library `rounding` in `shared/libraries/rounding.jadpo`"),
        "{found:#?}"
    );
    let app = connected_as("main");
    let found = problems(&[("app.jadpo", &app), config, domain], &[]);
    assert!(
        !found.iter().any(|p| p.contains("Connection `main`")),
        "{found:#?}"
    );
}
