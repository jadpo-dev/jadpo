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
fn an_index_may_name_its_publisher_and_has_to_name_it_well() {
    let named = (
        INDEX.0,
        "/// UK postcodes.\npackage Postcodes.v2 {\n    publisher: \"@acme-labs\"\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}\n",
    );
    let mut files = with(&[SAVE]);
    files.retain(|(path, _)| *path != INDEX.0);
    files.push(named);
    assert_eq!(installed(&files), Vec::<String>::new());
    for written in [
        "\"acme\"",
        "\"@Acme\"",
        "\"@acme/postcodes\"",
        "\"@-acme\"",
        "\"@acme--labs\"",
        "\"\"",
        "Licence.mit",
        "3",
    ] {
        let text = format!(
            "/// UK postcodes.\npackage Postcodes.v2 {{\n    publisher: {written}\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}}\n"
        );
        let mut files = with(&[SAVE]);
        files.retain(|(path, _)| *path != INDEX.0);
        files.push((INDEX.0, text.as_str()));
        let found = installed(&files);
        let expected = format!("package.index_invalid: `{written}` isn't a package's `publisher:`");
        assert!(
            found.iter().any(|p| p.contains(&expected)),
            "{expected}: {found:#?}"
        );
    }
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
        "shared/types/coins.jadpo",
        "type Coins = Int {\n    min: 0\n}\n",
    );
    let uses = (
        "packages/postcodes/v2/types/fee.jadpo",
        "/// A fee.\npublic type Fee = Object {\n    amount: Coins\n}\n",
    );
    let found = installed(&with(&[pence, uses]));
    assert!(
        found.iter().any(|p| p.contains(
            "packages/postcodes/v2/types/fee.jadpo semantic.unknown_name: `Coins` isn't declared"
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
    assert_eq!(
        both_installed(&with(&[MOVING, MOVED])),
        Vec::<String>::new()
    );
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
        found.iter().all(|p| p.contains("package.version_needed")),
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
        assert!(
            keys.iter().any(|key| key == expected),
            "{expected}: {keys:#?}"
        );
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

#[test]
fn a_package_and_its_version_alone_is_not_called() {
    let call = (
        "domains/addresses/actions/read_postcode.jadpo",
        "use packages/postcodes

/// Reads a customer's postcode.
action read_postcode(written: Postcodes.v3.WrittenPostcode) -> Postcodes.v3.UkPostcode {
    return Postcodes.v3(written)
}
",
    );
    let found = both_installed(&with(&[call]));
    assert!(
        found.iter().any(|p| p
            .contains("semantic.not_callable: `Postcodes.v3` is a package and can't be called")),
        "{found:#?}"
    );
}

// --- a package's enum values (chunk 7.1e) --------------------------------

/// A package's enum, one variant carrying fields.
const ROUND: (&str, &str) = (
    "packages/postcodes/v2/types/round.jadpo",
    "/// When a postcode's collections run.
public type Round = Enum {
    morning
    evening
    /// Collected with another postcode's round.
    shared: {
        postcode: UkPostcode
    }
}
",
);

/// The app writing the package's enum values through the package's name:
/// a value, one with fields, a constant, a comparison and patterns.
const ROUNDS: (&str, &str) = (
    "domains/addresses/actions/round_for.jadpo",
    "use packages/postcodes

/// The round a new postcode starts on.
constant first_round: Postcodes.Round = Postcodes.Round.morning

/// The round a postcode's collections run in.
action round_for(postcode: Postcodes.UkPostcode) -> Postcodes.Round {
    if (postcode == \"SW1A 1AA\") {
        return Postcodes.Round.morning
    }
    if (postcode == \"SW1A 2AA\") {
        return first_round
    }
    return Postcodes.Round.shared {
        postcode: postcode
    }
}
",
);

const LATER: (&str, &str) = (
    "domains/addresses/actions/later_round.jadpo",
    "use packages/postcodes

/// The round after `round`.
action later_round(round: Postcodes.Round) -> Postcodes.Round {
    if (round == Postcodes.Round.evening) {
        return round
    }
    return match round {
        Postcodes.Round.morning -> Postcodes.Round.evening
        Postcodes.Round.evening -> Postcodes.Round.evening
        Postcodes.Round.shared -> round
    }
}
",
);

#[test]
fn a_package_s_enum_value_is_written_through_its_name() {
    assert_eq!(
        installed(&with(&[ROUND, ROUNDS, LATER])),
        Vec::<String>::new()
    );
}

#[test]
fn a_package_s_unknown_variant_is_reported_by_its_name() {
    let wrong = (
        "domains/addresses/actions/round_for.jadpo",
        "use packages/postcodes

/// The round a postcode's collections run in.
action round_for() -> Postcodes.Round {
    return Postcodes.Round.mornin
}
",
    );
    let found = installed(&with(&[ROUND, wrong]));
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("type.unknown_enum_variant"), "{found:#?}");
}

/// The app's enum values with both versions held: the package's enum in
/// each, and the app's files as `version` rewrites them.
fn rounds_in_both(version: impl Fn(&str) -> String) -> Vec<String> {
    let three = (ROUND.0.replace("/v2/", "/v3/"), ROUND.1.to_string());
    let app: Vec<(&str, String)> = [ROUNDS, LATER]
        .iter()
        .map(|(path, text)| (*path, version(text)))
        .collect();
    let mut files: Vec<(&str, &str)> = with(&[ROUND]);
    files.push((three.0.as_str(), three.1.as_str()));
    files.extend(app.iter().map(|(path, text)| (*path, text.as_str())));
    both_installed(&files)
}

#[test]
fn a_package_s_enum_value_carries_the_version_while_two_are_held() {
    let moved = rounds_in_both(|text| text.replace("Postcodes.", "Postcodes.v3."));
    assert_eq!(moved, Vec::<String>::new());
}

#[test]
fn a_package_s_enum_value_without_its_version_is_refused_while_two_are_held() {
    let found = rounds_in_both(str::to_string);
    assert!(!found.is_empty(), "{found:#?}");
    assert!(
        found.iter().all(|p| p.contains("package.version_needed")),
        "only the missing versions are reported: {found:#?}"
    );
}

#[test]
fn a_package_s_variant_pattern_or_block_without_its_version_is_refused_once() {
    let found = rounds_in_both(|text| {
        text.replace("Postcodes.", "Postcodes.v3.")
            .replace(
                "Postcodes.v3.Round.morning ->",
                "Postcodes.Round.morning ->",
            )
            .replace("Postcodes.v3.Round.shared {", "Postcodes.Round.shared {")
    });
    assert_eq!(
        found,
        vec![
            "domains/addresses/actions/later_round.jadpo package.version_needed: `Postcodes` is held in two versions, so a name through it says which".to_string(),
            "domains/addresses/actions/round_for.jadpo package.version_needed: `Postcodes` is held in two versions, so a name through it says which".to_string(),
        ]
    );
}

#[test]
fn a_package_s_enum_alone_is_not_a_value() {
    let alone = (
        "domains/addresses/actions/round_for.jadpo",
        "use packages/postcodes

/// The round a postcode's collections run in.
action round_for() -> Postcodes.Round {
    return Postcodes.Round
}
",
    );
    assert_eq!(
        installed(&with(&[ROUND, alone])),
        vec![
            "domains/addresses/actions/round_for.jadpo type.not_a_value: `Postcodes.Round` is an enum, not a value"
                .to_string()
        ]
    );
}

// --- a package's failures in an outcome match (chunk 7.8d) ----------------

/// The app matching the package's call on its outcome, its failure's arm
/// written through the package's name as `fails` writes it.
const MATCHED: (&str, &str) = (
    "domains/addresses/actions/maybe_postcode.jadpo",
    "use packages/postcodes

/// Reads a customer's postcode, or nothing when it isn't one.
action maybe_postcode(written: Postcodes.WrittenPostcode) -> Postcodes.UkPostcode? {
    return match Postcodes.uk.parse(written) {
        success(postcode) -> postcode
        Postcodes.InvalidPostcode -> none
    }
}
",
);

#[test]
fn a_package_s_failure_is_matched_through_its_name() {
    assert_eq!(installed(&with(&[MATCHED])), Vec::<String>::new());
}

#[test]
fn a_package_s_unhandled_failure_is_named_through_the_package() {
    let unhandled = (
        MATCHED.0,
        "use packages/postcodes

/// Reads a customer's postcode.
action maybe_postcode(written: Postcodes.WrittenPostcode) -> Postcodes.UkPostcode? {
    return match Postcodes.uk.parse(written) {
        success(postcode) -> postcode
    }
}
",
    );
    assert_eq!(
        installed(&with(&[unhandled])),
        vec![
            "domains/addresses/actions/maybe_postcode.jadpo failure.outcome_missing_arm: This `match` doesn't handle `Postcodes.InvalidPostcode`"
                .to_string()
        ]
    );
}

/// The project with [`MATCHED`]'s action written as `text`.
fn matched(text: &str) -> Vec<(&str, &str)> {
    let mut files: Vec<(&str, &str)> = with(&[]);
    files.push((MATCHED.0, text));
    files
}

#[test]
fn a_package_s_failure_carries_the_version_while_two_are_held() {
    let moved = MATCHED.1.replace("Postcodes.", "Postcodes.v3.");
    assert_eq!(both_installed(&matched(&moved)), Vec::<String>::new());
}

#[test]
fn a_package_s_failure_arm_without_its_version_is_refused_once() {
    let moved = MATCHED.1.replace("Postcodes.", "Postcodes.v3.").replace(
        "Postcodes.v3.InvalidPostcode ->",
        "Postcodes.InvalidPostcode ->",
    );
    assert_eq!(
        both_installed(&matched(&moved)),
        vec![
            "domains/addresses/actions/maybe_postcode.jadpo package.version_needed: `Postcodes` is held in two versions, so a name through it says which".to_string(),
        ]
    );
}

/// A mail provider written by hand, its public types and one it keeps to
/// itself, in `folder`.
fn mailer(folder: &str, provider: &str) -> Vec<(String, String)> {
    vec![
        (
            format!("{folder}/package.jadpo"),
            "/// Acme Mail.\npackage AcmeMail.v1 {\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}\n"
                .to_string(),
        ),
        (format!("{folder}/provider.jadpo"), provider.to_string()),
        (
            format!("{folder}/types/outgoing_email.jadpo"),
            "/// One email to send.
public type OutgoingEmail = Object {
    from: Text
    to: List<Text>
    cc: List<Text>?
    subject: Text
    html: Text {
        personal: PersonalData.identifying
    }
    customer_email: Text
    first_name: Text
    surname: Text
    full_name: Text
    billing_phone: Text
    name: Text
    tag: Text
}
"
            .to_string(),
        ),
        (
            format!("{folder}/types/conversion.jadpo"),
            "/// A rate between two currencies.
public type Conversion = Object {
    from: Text
    to: Text
    text: Text
}
"
            .to_string(),
        ),
        (
            format!("{folder}/types/kept.jadpo"),
            "/// The provider's own shape, kept inside.
type Kept = Object {
    email: Text
}
"
            .to_string(),
        ),
        (
            format!("{folder}/types/chat.jadpo"),
            "/// One message to a model.
public type ChatTurn = Object {
    role: Text
    content: Text
    model: Text
    prompt: Text
}
"
            .to_string(),
        ),
    ]
}

const MAIL_PROVIDER: &str = "/// Acme Mail's sending API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: \"https://api.acme-mail.example\"
    }
}
";

fn borrowed(files: &[(String, String)]) -> Vec<(&str, &str)> {
    files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect()
}

/// `package.standard_candidate` reads a provider's text by its name (area
/// 11, "Shape and name detection"): an email's sender and recipients,
/// addresses, names and phone numbers may be standard types, and free
/// text a person writes is marked `personal:`. A `to` that isn't an
/// email's, a field already marked, a type kept inside and a name that
/// says nothing aren't flagged.
#[test]
fn a_providers_text_named_like_personal_data_is_flagged() {
    let files = mailer("shared/providers/acme_mail/v1", MAIL_PROVIDER);
    let mut all = vec![APP];
    all.extend(borrowed(&files));
    let found: Vec<String> = problems(&all, &[])
        .into_iter()
        .filter(|p| p.contains("package.standard_candidate"))
        .map(|p| p.split_once(": ").map(|(_, m)| m.to_string()).unwrap_or(p))
        .collect();
    assert_eq!(
        found,
        [
            "`ChatTurn.content` may hold personal data with no `personal:`",
            "`ChatTurn.prompt` may hold personal data with no `personal:`",
            "`OutgoingEmail.from` may be an `Email`",
            "`OutgoingEmail.to` may be an `Email`",
            "`OutgoingEmail.cc` may be an `Email`",
            "`OutgoingEmail.subject` may hold personal data with no `personal:`",
            "`OutgoingEmail.customer_email` may be an `Email`",
            "`OutgoingEmail.first_name` may be a `GivenName`",
            "`OutgoingEmail.surname` may be a `FamilyName`",
            "`OutgoingEmail.full_name` may hold personal data with no `personal:`",
            "`OutgoingEmail.billing_phone` may be a `PhoneNumber`",
        ],
        "{found:#?}"
    );
    // A list keeps its shape in the swap it suggests.
    let db = Database::default();
    let project = project_with_records(&db, Layout::Project, &all, &[]);
    let cc = jadpo_check::check(&db, project)
        .into_iter()
        .find(|d| d.message == "`OutgoingEmail.cc` may be an `Email`")
        .expect("the cc finding");
    assert!(
        cc.next
            .as_ref()
            .is_some_and(|next| next.text.contains("`cc: List<Email>?`")),
        "{:?}",
        cc.next
    );
}

/// An installed package's files are held to its digest, so a release
/// from before its text was read isn't refused in the app.
#[test]
fn an_installed_providers_text_is_left_to_its_publisher() {
    let folder = "packages/acme_mail/v1";
    let mut files = mailer(folder, MAIL_PROVIDER);
    // A decline its publisher wrote is no one's to remove in the app.
    files[2].1 = files[2].1.replace(
        "    tag: Text\n",
        "    decline package.standard_candidate: \"Checked by its publisher\"\n    tag: Text\n",
    );
    let held = borrowed(&files);
    let lock = lock_for("AcmeMail.v1", folder, &held, &[]);
    let mut all = vec![APP];
    all.extend(held);
    let found = problems(&all, &[("jadpo.lock", &lock)]);
    assert!(
        !found
            .iter()
            .any(|p| p.contains("package.standard_candidate") || p.contains("decline.unused")),
        "{found:#?}"
    );
}

/// A hand-written package lists its reviewed types in `provider { }`
/// itself, each once and each its own; an imported one keeps the list in
/// `imported:`.
#[test]
fn a_hand_written_provider_lists_what_it_reviewed() {
    let check = |provider: &str| {
        let files = mailer("shared/providers/acme_mail/v1", provider);
        let mut all = vec![APP];
        all.extend(borrowed(&files));
        problems(&all, &[])
            .into_iter()
            .filter(|p| p.contains("package.provider_invalid"))
            .collect::<Vec<_>>()
    };
    let with = |reviewed: &str| MAIL_PROVIDER.replace("\n}\n", &format!("\n    {reviewed}\n}}\n"));
    assert_eq!(
        check(&with("reviewed: [OutgoingEmail, Conversion]")),
        Vec::<String>::new()
    );
    let found = check(&with("reviewed: [OutgoingEmial, Conversion, Conversion]"));
    for expected in [
        "`OutgoingEmial` in `reviewed:` isn't one of the package's types",
        "`Conversion` is in `reviewed:` twice",
    ] {
        assert!(
            found.iter().any(|p| p.contains(expected)),
            "{expected}: {found:#?}"
        );
    }
    let beside = with(
        "imported: {\n        file: \"acme-mail.json\"\n        digest: \"sha256:4f1c\"\n    }\n    reviewed: [Conversion]",
    );
    let found = check(&beside);
    assert!(
        found
            .iter()
            .any(|p| p.contains("`reviewed:` is beside `imported:`")),
        "{found:#?}"
    );
}

/// The package's library with two constants of one type holding one value,
/// a judgement call a newer Jadpo might raise in a published release.
const ALIKE: (&str, &str) = (
    "packages/postcodes/v2/libraries/uk.jadpo",
    "/// Reading UK postcodes.
public library uk {
    /// The shortest postcode.
    constant shortest: WrittenPostcode = \"W1 1AA\"

    /// The shortest postcode, again.
    constant also_shortest: WrittenPostcode = \"W1 1AA\"

    /// Reads a UK postcode as it is written.
    function parse(written: WrittenPostcode) fails InvalidPostcode -> UkPostcode {
        if (written == shortest or written == also_shortest) {
            return attempt UkPostcode(written) {
                invalid: InvalidPostcode
            }
        }
        return attempt UkPostcode(written) {
            invalid: InvalidPostcode
        }
    }
}
",
);

/// The rules of the files given, as `path rule` lines, set apart or not.
fn checked_installed(files: &[(&str, &str)]) -> (Vec<String>, Vec<(String, Vec<String>)>) {
    let lock = lock_for("Postcodes.v2", "packages/postcodes/v2", files, &[GUIDE]);
    let db = Database::default();
    let records = [GUIDE, ("jadpo.lock", lock.as_str())];
    let project = project_with_records(&db, Layout::Project, files, &records);
    let line = |d: &jadpo_diag::Diagnostic| {
        let path = d
            .primary
            .as_ref()
            .map_or("", |label| label.span.path.as_str());
        format!("{path} {}", d.rule.id)
    };
    let checked = jadpo_check::checked(&db, project);
    (
        checked.problems.iter().map(line).collect(),
        checked
            .publishers
            .iter()
            .map(|(package, found)| (package.clone(), found.iter().map(line).collect()))
            .collect(),
    )
}

/// A newer Jadpo's publishing-bar finding in an installed package's own
/// code is its publisher's to answer: the app is told and keeps working.
/// A real error still refuses (area 11, "Installing safely").
#[test]
fn an_installed_packages_publishing_bar_finding_is_left_to_its_publisher() {
    let files: Vec<(&str, &str)> = with(&[SAVE])
        .into_iter()
        .map(|file| if file.0 == UK.0 { ALIKE } else { file })
        .collect();
    let (problems, publishers) = checked_installed(&files);
    assert_eq!(problems, Vec::<String>::new());
    assert_eq!(
        publishers,
        [(
            "Postcodes.v2".to_string(),
            vec!["packages/postcodes/v2/libraries/uk.jadpo constant.similar_exists".to_string()]
        )]
    );
    // An error that isn't a judgement call is a package that doesn't
    // compile, and refuses the app as before.
    let broken = ALIKE.1.replace(
        "return attempt UkPostcode(written) {\n                invalid",
        "return attempt UkPostcode(also) {\n                invalid",
    );
    let files: Vec<(&str, &str)> = files
        .into_iter()
        .map(|file| {
            if file.0 == UK.0 {
                (UK.0, broken.as_str())
            } else {
                file
            }
        })
        .collect();
    let (problems, _) = checked_installed(&files);
    assert!(
        problems
            .iter()
            .any(|line| line.starts_with("packages/postcodes/v2/libraries/uk.jadpo ")),
        "{problems:?}"
    );
}

/// The app's own integration is the app's code: its findings are errors.
#[test]
fn the_apps_own_integrations_publishing_bar_finding_is_an_error() {
    let own: Vec<(String, String)> = with(&[])
        .into_iter()
        .map(|(path, text)| {
            let path = path.replace("packages/postcodes", "shared/providers/postcodes");
            let text = if path.ends_with("libraries/uk.jadpo") {
                ALIKE.1
            } else {
                text
            };
            (path, text.to_string())
        })
        .chain([(
            SAVE.0.to_string(),
            SAVE.1
                .replace("use packages/postcodes", "use shared/providers/postcodes"),
        )])
        .collect();
    let own: Vec<(&str, &str)> = own.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    let found = problems(&own, &[]);
    assert_eq!(
        found
            .iter()
            .filter(|line| line.contains("constant.similar_exists"))
            .count(),
        1,
        "{found:?}"
    );
}
