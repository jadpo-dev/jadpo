//! A release's facts, change record and fingerprints (area 11, "What a
//! release shows", "The changelog", "Search" and "Yanks, takedowns and
//! advisories"; chunk R3): what the registry shows of a release, each
//! worked out from the compiled package.

use crate::support;

use jadpo_check::publishing;
use jadpo_check::release::{self, Change, Facts};
use jadpo_check::surface;
use jadpo_db::{Database, Layout};
use support::{APP, project_with_records};

const POSTCODES: &str = "shared/providers/postcodes/v2";
const PAY: &str = "shared/providers/acme_pay/v1";

fn library() -> Vec<(&'static str, &'static str)> {
    vec![
        APP,
        (
            "shared/providers/postcodes/v2/package.jadpo",
            "/// UK postcodes.\npackage Postcodes.v2 {\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}\n",
        ),
        (
            "shared/providers/postcodes/v2/types/written_postcode.jadpo",
            "/// A postcode as someone typed it.\npublic type WrittenPostcode = Text {\n    max_length: 20\n}\n",
        ),
        (
            "shared/providers/postcodes/v2/types/uk_postcode.jadpo",
            "/// A UK postcode.\npublic type UkPostcode = WrittenPostcode {\n    min_length: 5\n}\n",
        ),
        (
            "shared/providers/postcodes/v2/types/district.jadpo",
            "/// Where a postcode is.\npublic type District = Object {\n    outward: Text {\n        max_length: 4\n    }\n    note: Text?\n    within: District?\n}\n",
        ),
        (
            "shared/providers/postcodes/v2/types/region.jadpo",
            "/// The nations.\npublic type Region = Enum {\n    england\n    wales\n}\n",
        ),
        (
            "shared/providers/postcodes/v2/failures/invalid_postcode.jadpo",
            "/// Not a UK postcode.\npublic type InvalidPostcode = Failure {\n    kind: FailureKind.invalid_value\n}\n",
        ),
        (
            "shared/providers/postcodes/v2/libraries/uk.jadpo",
            "/// Reading UK postcodes.\npublic library uk {\n    /// Reads a UK postcode.\n    function parse(written: WrittenPostcode) fails InvalidPostcode -> UkPostcode {\n        var upper: Text = text.upper(written)\n        return attempt UkPostcode(upper) {\n            invalid: InvalidPostcode\n        }\n    }\n\n    /// Where a postcode is.\n    function district_of(postcode: List<UkPostcode>) -> District? {\n        return none\n    }\n}\n",
        ),
    ]
}

/// A provider: a public resource, a private one its action calls, a
/// recording of one operation, personal data, and a declined candidate.
fn provider() -> Vec<(&'static str, &'static str)> {
    vec![
        APP,
        (
            "shared/providers/acme_pay/v1/package.jadpo",
            "/// Acme Pay's API.\npackage AcmePay.v1 {\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}\n",
        ),
        (
            "shared/providers/acme_pay/v1/provider.jadpo",
            "/// Acme Pay's API.\nprovider {\n    protocol: Protocol.json_http\n    hosts: {\n        live: \"https://api.acme-pay.example\"\n        sandbox: \"https://sandbox.acme-pay.example\"\n    }\n    credential: CredentialScheme.bearer\n    erasure: {\n        call: customers.delete\n        by: CustomerId\n    }\n}\n",
        ),
        (
            "shared/providers/acme_pay/v1/types/customer_id.jadpo",
            "/// Acme Pay's id for a customer.\npublic type CustomerId = Text {\n    max_length: 40\n}\n",
        ),
        (
            "shared/providers/acme_pay/v1/types/plan.jadpo",
            "/// A plan.\npublic type Plan = Enum {\n    free\n    paid\n}\n",
        ),
        (
            "shared/providers/acme_pay/v1/types/customer.jadpo",
            "/// A customer to create.\npublic type Customer = Object {\n    id: CustomerId\n    notes: Text {\n        personal: PersonalData.health\n    }\n    decline package.standard_candidate: \"A sequence number, not a time\"\n    created: Int\n}\n",
        ),
        (
            "shared/providers/acme_pay/v1/failures/card_declined.jadpo",
            "/// The card was declined.\npublic type CardDeclined = Failure {\n    kind: FailureKind.conflict\n}\n",
        ),
        (
            "shared/providers/acme_pay/v1/resources/customers.jadpo",
            "/// Customers.\npublic resource customers {\n    /// Creates a customer.\n    create: {\n        method: HttpMethod.post\n        path: \"/v1/customers\"\n        input: Customer\n        output: Customer\n        changes: true\n        failures: [CardDeclined]\n        source: PackageSource.written\n    }\n\n    /// Reads a customer.\n    get: {\n        method: HttpMethod.get\n        path: \"/v1/customers\"\n        output: Customer\n        source: PackageSource.written\n    }\n\n    /// Erases a customer.\n    delete: {\n        method: HttpMethod.delete\n        path: \"/v1/customers\"\n        changes: true\n        source: PackageSource.written\n    }\n}\n",
        ),
        (
            "shared/providers/acme_pay/v1/resources/audit.jadpo",
            "/// What the package keeps to itself.\nresource audit {\n    /// Notes a change.\n    note: {\n        method: HttpMethod.post\n        path: \"/v1/audit\"\n        changes: true\n        source: PackageSource.written\n    }\n}\n",
        ),
    ]
}

fn facts_of(files: &[(&str, &str)], records: &[(&str, &str)], folder: &str) -> Facts {
    let db = Database::default();
    let project = project_with_records(&db, Layout::Project, files, records);
    release::facts(&db, project, folder).expect("a package")
}

fn surface_text(files: &[(&str, &str)], folder: &str) -> String {
    let db = Database::default();
    let project = project_with_records(&db, Layout::Project, files, &[]);
    surface::text(&db, project, folder, Some("2.0.0")).expect("a surface")
}

/// `files` with `path`'s text replaced.
fn with<'a>(files: &[(&'a str, &'a str)], path: &'a str, text: &'a str) -> Vec<(&'a str, &'a str)> {
    let mut out: Vec<(&str, &str)> = files
        .iter()
        .filter(|(at, _)| *at != path)
        .copied()
        .collect();
    out.push((path, text));
    out
}

// --- facts -----------------------------------------------------------------

#[test]
fn a_library_is_pure_and_its_public_types_may_be_stored() {
    let facts = facts_of(&library(), &[], POSTCODES);
    assert_eq!(facts.kind, "library");
    assert!(facts.pure);
    assert!(facts.hosts.is_empty());
    assert!(facts.changing_calls.is_empty());
    assert!(facts.personal_data.is_empty());
    assert!(!facts.erasure);
    assert_eq!(
        facts.storable_types,
        ["District", "Region", "UkPostcode", "WrittenPostcode"]
    );
    assert_eq!(facts.failures.len(), 1);
    assert_eq!(facts.failures[0].declaration, "uk.parse");
    assert_eq!(facts.failures[0].failures, ["InvalidPostcode"]);
    assert!(facts.confirmed_operations.is_empty());
    assert!(facts.standard_candidates.is_empty());
}

#[test]
fn a_providers_facts_cover_what_its_code_can_do_public_or_not() {
    let facts = facts_of(
        &provider(),
        &[(
            "shared/providers/acme_pay/v1/recordings/customers.get.json",
            "{}\n",
        )],
        PAY,
    );
    assert_eq!(facts.kind, "provider");
    assert!(!facts.pure);
    let hosts: Vec<&str> = facts.hosts.iter().map(|host| host.host.as_str()).collect();
    assert_eq!(
        hosts,
        [
            "https://api.acme-pay.example",
            "https://sandbox.acme-pay.example"
        ]
    );
    // An operation of a resource the package keeps to itself still reaches
    // the host, and still changes something there.
    assert_eq!(
        facts.hosts[0].operations,
        [
            "audit.note",
            "customers.create",
            "customers.delete",
            "customers.get"
        ]
    );
    assert_eq!(
        facts.changing_calls,
        ["audit.note", "customers.create", "customers.delete"]
    );
    assert!(facts.erasure);
    // Only ids and plain values of a provider's may be stored.
    assert_eq!(facts.storable_types, ["CustomerId"]);
    assert_eq!(facts.failures[0].declaration, "customers.create");
    assert_eq!(facts.failures[0].failures, ["CardDeclined"]);
    assert_eq!(facts.confirmed_operations, ["customers.get"]);
    assert_eq!(facts.operations, 4);
    assert_eq!(facts.standard_candidates.len(), 1);
    assert_eq!(facts.standard_candidates[0].declaration, "Customer.created");
    assert_eq!(facts.standard_candidates[0].standard, "Instant");
}

#[test]
fn personal_data_is_listed_for_each_host_it_reaches() {
    let facts = facts_of(&provider(), &[], PAY);
    let pairs: Vec<(&str, &str, &str)> = facts
        .personal_data
        .iter()
        .map(|sent| {
            (
                sent.operation.as_str(),
                sent.field.as_str(),
                sent.host.as_str(),
            )
        })
        .collect();
    assert_eq!(
        pairs,
        [
            (
                "customers.create",
                "Customer.notes",
                "https://api.acme-pay.example"
            ),
            (
                "customers.create",
                "Customer.notes",
                "https://sandbox.acme-pay.example"
            ),
        ]
    );
    assert_eq!(facts.personal_data[0].categories, ["health"]);
    assert!(!facts.personal_data[0].decided_only);
}

// --- the change record -----------------------------------------------------

/// The record's entries as `(declaration, change, detail)`.
fn entries(before: &str, now: &str) -> Vec<(String, Change, Option<String>)> {
    release::surface_changes(before, now)
        .into_iter()
        .map(|entry| (entry.declaration, entry.change, entry.detail))
        .collect()
}

#[test]
fn the_change_record_names_each_declaration_and_what_happened_to_it() {
    let before = surface_text(&library(), POSTCODES);
    let files = library();
    let files = with(
        &files,
        "shared/providers/postcodes/v2/types/written_postcode.jadpo",
        "/// Reworded.\npublic type WrittenPostcode = Text {\n    max_length: 30\n}\n",
    );
    let files = with(
        &files,
        "shared/providers/postcodes/v2/types/outward_code.jadpo",
        "/// The first half.\npublic type OutwardCode = Text {\n    max_length: 4\n}\n",
    );
    let files = with(
        &files,
        "shared/providers/postcodes/v2/libraries/uk.jadpo",
        "/// Reading UK postcodes.\npublic library uk {\n    /// Reads a UK postcode.\n    function parse(written: WrittenPostcode) fails InvalidPostcode, OutwardCodeUnknown -> UkPostcode {\n        var upper: Text = text.upper(written)\n        return attempt UkPostcode(upper) {\n            invalid: InvalidPostcode\n        }\n    }\n}\n",
    );
    let files = with(
        &files,
        "shared/providers/postcodes/v2/failures/outward_code_unknown.jadpo",
        "/// Nobody knows it.\npublic type OutwardCodeUnknown = Failure {\n    kind: FailureKind.not_found\n}\n",
    );
    let now = surface_text(&files, POSTCODES);
    let found = entries(&before, &now);
    let named = |name: &str| -> Vec<&(String, Change, Option<String>)> {
        found.iter().filter(|entry| entry.0 == name).collect()
    };
    assert_eq!(named("OutwardCode")[0].1, Change::Added);
    assert_eq!(named("OutwardCodeUnknown")[0].1, Change::Added);
    assert_eq!(named("uk.district_of")[0].1, Change::Removed);
    assert_eq!(named("uk.parse")[0].1, Change::NewFailure);
    assert_eq!(
        named("uk.parse")[0].2.as_deref(),
        Some("OutwardCodeUnknown")
    );
    assert_eq!(named("WrittenPostcode")[0].1, Change::Retyped);
    assert_eq!(
        named("WrittenPostcode")[0].2.as_deref(),
        Some("max_length: 30, was max_length: 20")
    );
    // A description alone isn't a change; nothing else moved.
    assert_eq!(found.len(), 5, "{found:?}");
}

#[test]
fn a_new_host_is_recorded_and_a_new_minor_version_a_removed_one_breaks() {
    let provider = provider();
    let before = surface_text(&provider, PAY);
    let added = with(
        &provider,
        "shared/providers/acme_pay/v1/provider.jadpo",
        "/// Acme Pay's API.\nprovider {\n    protocol: Protocol.json_http\n    hosts: {\n        live: \"https://eu.acme-pay.example\"\n        sandbox: \"https://sandbox.acme-pay.example\"\n    }\n    credential: CredentialScheme.bearer\n}\n",
    );
    let moved = surface_text(&added, PAY);
    let found = entries(&before, &moved);
    assert_eq!(found[0].1, Change::Retyped, "{found:?}");
    assert!(
        publishing::version("v1", Some(("1.0.0", &before)), &moved).is_err(),
        "moving a host breaks"
    );

    let one_host = with(
        &provider,
        "shared/providers/acme_pay/v1/provider.jadpo",
        "/// Acme Pay's API.\nprovider {\n    protocol: Protocol.json_http\n    hosts: {\n        live: \"https://api.acme-pay.example\"\n    }\n    credential: CredentialScheme.bearer\n}\n",
    );
    let earlier = surface_text(&one_host, PAY);
    let found = entries(&earlier, &before);
    assert_eq!(
        found,
        [(
            "provider".to_string(),
            Change::NewHost,
            Some("https://sandbox.acme-pay.example".to_string())
        )]
    );
    let bump = publishing::version("v1", Some(("1.0.0", &earlier)), &before).expect("allowed");
    assert_eq!(bump.version, "1.1.0");
}

#[test]
fn personal_data_sent_somewhere_new_is_recorded_once_per_field_and_host() {
    let now = facts_of(&provider(), &[], PAY);
    let mut earlier = now.clone();
    earlier.personal_data.remove(1);
    let found = release::personal_data_changes(Some(&earlier), &now);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].declaration, "customers.create");
    assert_eq!(found[0].change, Change::PersonalDataSent);
    assert_eq!(
        found[0].detail.as_deref(),
        Some("Customer.notes to https://sandbox.acme-pay.example")
    );
    // What was sent before stays unlisted however many operations send it.
    assert!(release::personal_data_changes(Some(&now), &now).is_empty());
    // A release whose facts can't be read is taken to have sent nothing, so
    // nothing is hidden by a compiler that can no longer read it.
    assert_eq!(release::personal_data_changes(None, &now).len(), 2);
}

#[test]
fn each_entry_is_a_line_of_the_changelog() {
    let before = surface_text(&library(), POSTCODES);
    let files = library();
    let files = with(
        &files,
        "shared/providers/postcodes/v2/types/outward_code.jadpo",
        "/// The first half.\npublic type OutwardCode = Text {\n    max_length: 4\n}\n",
    );
    let now = surface_text(&files, POSTCODES);
    let found = release::surface_changes(&before, &now);
    assert_eq!(found[0].line(), "added type `OutwardCode`");
    assert_eq!(
        release::behaviour_changed("uk.parse", "function", "example of uk.parse").line(),
        "behaviour changed `uk.parse`: example of uk.parse"
    );
}

#[test]
fn a_fix_is_marked_in_the_new_versions_section() {
    let log = "# Changelog

## 2.1.1

- behaviour changed `uk.parse`: example of uk.parse

Fix `uk.parse`: it read lower-case outward codes wrongly, which no courier could read.
Fix `uk.tidy`:

## 2.1.0

- added type `OutwardCode`

Fix `uk.old`: an older section isn't this release's.
";
    let fixes = publishing::fixes(log, "2.1.1");
    assert_eq!(
        fixes.get("uk.parse").map(String::as_str),
        Some("it read lower-case outward codes wrongly, which no courier could read.")
    );
    // A mark with no reason is no mark.
    assert!(!fixes.contains_key("uk.tidy"));
    assert!(!fixes.contains_key("uk.old"));
}

// --- fingerprints ----------------------------------------------------------

#[test]
fn an_actions_effects_are_what_it_calls_and_a_message_has_a_fingerprint() {
    let mut files = provider();
    files.push((
        "shared/providers/acme_pay/v1/actions/start.jadpo",
        "/// Creates the customer, then notes it.\npublic action start(input: Customer) fails CardDeclined -> Customer {\n    var customer: Customer = attempt customers.create(input)\n    return customer\n}\n",
    ));
    let start = fingerprint_of(&files, PAY, "start");
    assert_eq!(start.kind, "action");
    let shape = start.shape.expect("an action has a shape");
    assert_eq!(shape.effects, ["calls", "changes"]);
    assert_eq!(shape.failures, ["conflict"]);

    let mut files = library();
    files.push((
        "shared/providers/postcodes/v2/translations/postcode_hint.jadpo",
        "/// Shown under a postcode field.\npublic type PostcodeHint = Message {}\n",
    ));
    files.push((
        "shared/providers/postcodes/v2/translations/en_gb.jadpo",
        "translations {\n    postcode_hint: \"Write it as it is on a letter.\"\n}\n",
    ));
    let hint = fingerprint_of(&files, POSTCODES, "translations.postcode_hint");
    assert_eq!(hint.kind, "message");
    assert!(hint.shape.is_none());
}

fn fingerprint_of(files: &[(&str, &str)], folder: &str, name: &str) -> release::Fingerprint {
    let db = Database::default();
    let project = project_with_records(&db, Layout::Project, files, &[]);
    release::fingerprints(&db, project, folder)
        .into_iter()
        .find(|print| print.declaration == name)
        .unwrap_or_else(|| panic!("no fingerprint of {name}"))
}

#[test]
fn every_public_declaration_has_a_fingerprint_and_only_those() {
    let db = Database::default();
    let project = project_with_records(&db, Layout::Project, &library(), &[]);
    let prints = release::fingerprints(&db, project, POSTCODES);
    let names: Vec<&str> = prints
        .iter()
        .map(|print| print.declaration.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "uk.district_of",
            "uk.parse",
            "District",
            "Region",
            "UkPostcode",
            "WrittenPostcode",
            "InvalidPostcode",
        ]
    );
    assert!(
        prints
            .iter()
            .all(|print| print.source.starts_with("sha256:"))
    );
}

#[test]
fn a_functions_shape_reduces_its_types_to_standard_ones() {
    let parse = fingerprint_of(&library(), POSTCODES, "uk.parse");
    let shape = parse.shape.expect("a function has a shape");
    assert_eq!(shape.inputs, ["Text { max_length: 20 }"]);
    assert_eq!(
        shape.output.as_deref(),
        Some("Text { max_length: 20, min_length: 5 }")
    );
    assert_eq!(shape.failures, ["invalid_value"]);
    assert!(shape.effects.is_empty());

    let district = fingerprint_of(&library(), POSTCODES, "uk.district_of");
    let shape = district.shape.expect("a function has a shape");
    assert_eq!(
        shape.inputs,
        ["List<Text { max_length: 20, min_length: 5 }>"]
    );
    // A type that holds itself ends where it comes round again.
    assert_eq!(
        shape.output.as_deref(),
        Some("{ note: Text?, outward: Text { max_length: 4 }, within: …? }?")
    );
    assert!(
        fingerprint_of(&library(), POSTCODES, "Region")
            .shape
            .is_none()
    );
}

#[test]
fn the_same_shape_under_other_names_is_the_same_fingerprint() {
    let renamed = library();
    let renamed = with(
        &renamed,
        "shared/providers/postcodes/v2/types/written_postcode.jadpo",
        "/// Renamed.\npublic type TypedPostcode = Text {\n    max_length: 20\n}\n",
    );
    let renamed = with(
        &renamed,
        "shared/providers/postcodes/v2/types/uk_postcode.jadpo",
        "/// A UK postcode.\npublic type UkPostcode = TypedPostcode {\n    min_length: 5\n}\n",
    );
    let renamed = with(
        &renamed,
        "shared/providers/postcodes/v2/libraries/uk.jadpo",
        "/// Reading UK postcodes.\npublic library uk {\n    /// Reads a UK postcode.\n    function parse(entered: TypedPostcode) fails InvalidPostcode -> UkPostcode {\n        var upper: Text = text.upper(entered)\n        return attempt UkPostcode(upper) {\n            invalid: InvalidPostcode\n        }\n    }\n}\n",
    );
    assert_eq!(
        fingerprint_of(&library(), POSTCODES, "uk.parse").shape,
        fingerprint_of(&renamed, POSTCODES, "uk.parse").shape,
    );
}

#[test]
fn an_operations_effects_are_its_calls_and_whether_they_change_something() {
    let create = fingerprint_of(&provider(), PAY, "customers.create");
    let shape = create.shape.expect("an operation has a shape");
    assert_eq!(shape.effects, ["calls", "changes"]);
    assert_eq!(shape.failures, ["conflict"]);
    let get = fingerprint_of(&provider(), PAY, "customers.get");
    assert_eq!(get.shape.expect("a shape").effects, ["calls"]);
}

#[test]
fn a_copy_has_the_same_source_fingerprint_however_it_is_written() {
    let own = fingerprint_of(&library(), POSTCODES, "uk.parse").source;
    // Another description, other local names and other formatting.
    let copied = library();
    let copied = with(
        &copied,
        "shared/providers/postcodes/v2/libraries/uk.jadpo",
        "/// Reading postcodes.\npublic library uk {\n    /// Copied from somewhere else.\n    function parse(typed: WrittenPostcode) fails InvalidPostcode -> UkPostcode {\n\n        var loud: Text = text.upper(typed) // shouted\n        return attempt UkPostcode(loud) { invalid: InvalidPostcode }\n    }\n}\n",
    );
    assert_eq!(fingerprint_of(&copied, POSTCODES, "uk.parse").source, own);
    // A changed body isn't a copy.
    let changed = library();
    let changed = with(
        &changed,
        "shared/providers/postcodes/v2/libraries/uk.jadpo",
        "/// Reading UK postcodes.\npublic library uk {\n    /// Reads a UK postcode.\n    function parse(written: WrittenPostcode) fails InvalidPostcode -> UkPostcode {\n        var upper: Text = text.lower(written)\n        return attempt UkPostcode(upper) {\n            invalid: InvalidPostcode\n        }\n    }\n}\n",
    );
    assert_ne!(fingerprint_of(&changed, POSTCODES, "uk.parse").source, own);
    // A type's field names are what it is: renaming one isn't a copy.
    let district = fingerprint_of(&library(), POSTCODES, "District").source;
    let renamed = library();
    let renamed = with(
        &renamed,
        "shared/providers/postcodes/v2/types/district.jadpo",
        "/// Where a postcode is.\npublic type District = Object {\n    outward: Text {\n        max_length: 4\n    }\n    remark: Text?\n    within: District?\n}\n",
    );
    assert_ne!(
        fingerprint_of(&renamed, POSTCODES, "District").source,
        district
    );
}
