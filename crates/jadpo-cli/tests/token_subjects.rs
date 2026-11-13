//! A principal's `subject:` kept in several columns or whole as its JSON
//! (area 8, "`subject:`" and "`external_token`"; chunk N1b), read whole by
//! every way in against live PostgreSQL: a session, an API token, a
//! service key and the token its exchange gives, each with its signed
//! part, and an external token, whose subject a standard type's text is
//! read from.
//!
//! Two projects: a fleet whose couriers are known by an `EmailWithName`
//! and whose vans by a `VehicleRegistration`, each kept in two columns,
//! and a club whose members are known by an object kept as JSON. Each
//! project's route tests run through `jadpo test` here, and compiled with
//! the interpreter's records in the gate's compiled step (`compiled.rs`,
//! "a subject kept in several columns" and "a subject kept as JSON");
//! the signed parts, which a route test never sends, are sent here.

use std::fs;
use std::path::{Path, PathBuf};

use super::signed::{KEY, Live, MINUTE, START, code, cookie, json, request};

pub(super) const SEVERAL_COLUMNS: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"use fleet/entities { Courier, Van }

application {
    revocation_delay: 5m
    apis: {
        main: {
            prefix: /
            authentication: [
                AuthenticationMethod.browser_session,
                AuthenticationMethod.api_token,
                AuthenticationMethod.service_key
            ]
        }
        staff: {
            prefix: /staff
            authentication: [AuthenticationMethod.external_token]
        }
    }
}

principal {
    user: {
        subject: Courier.mailbox
        active: Courier.active == true
    }
    service: {
        subject: Van.registration
        active: Van.active == true
    }
}
"#,
    ),
    (
        "config.jadpo",
        r#"config {
    audience: Text {
        default: "fleet"
    }
    browser_origin: Url
    issuer: Url {
        default: "https://id.fleet.example/"
    }
    session_signing_key: SigningKey
}
"#,
    ),
    (
        "authentication.jadpo",
        r#"authentication {
    browser_session: {
        cookie: "fleet_session"
        signing_key: config.session_signing_key
        origin: config.browser_origin
    }
    api_token: {
        signing_key: config.session_signing_key
    }
    service_key: {
        signing_key: config.session_signing_key
    }
    external_token: {
        issuer: config.issuer
        audience: config.audience
    }
}
"#,
    ),
    (
        "domains/fleet/actions/add_van.jadpo",
        r#"use fleet/entities { Van }
use fleet/types { NewVan, VanAdded }
use fleet/failures { Taken }
use fleet/actions { register_van }

/// Adds a van and gives its first key.
action add_van(input: NewVan, owner: Principal.user) fails Taken -> VanAdded {
    var van: Van = attempt register_van(input.registration, owner)
    return VanAdded {
        key: Van.issue_key(van.id, input.key_expires_at)
    }
}
"#,
    ),
    (
        "domains/fleet/actions/join.jadpo",
        r#"use fleet/entities { Courier }
use fleet/failures { Taken }

/// Registers a courier.
action join(mailbox: EmailWithName) fails Taken -> Courier {
    return attempt Courier.create {
        set: {
            mailbox: mailbox
            active: true
        }
        conflict: {
            Courier.mailbox: Taken
        }
    }
}
"#,
    ),
    (
        "domains/fleet/actions/register_van.jadpo",
        r#"use fleet/entities { Van }
use fleet/failures { Taken }

/// Registers a van to its owner.
action register_van(registration: VehicleRegistration, owner: Principal.user) fails Taken -> Van {
    return attempt Van.create {
        set: {
            owner_id: owner.id
            registration: registration
            active: true
        }
        conflict: {
            Van.registration: Taken
        }
    }
}
"#,
    ),
    (
        "domains/fleet/actions/sign_up.jadpo",
        r#"use fleet/entities { Courier }
use fleet/types { NewCourier, SignedUp }
use fleet/failures { Taken }
use fleet/actions { join }

/// Registers a courier, signs this browser in and gives their app a token.
action sign_up(input: NewCourier) fails Taken -> SignedUp {
    var courier: Courier = attempt join(input.mailbox)
    sessions.start(courier.id)
    return SignedUp {
        token: sessions.issue_token(courier.id)
    }
}
"#,
    ),
    (
        "domains/fleet/entities/courier.jadpo",
        r#"/// Someone who drives the vans, known by their mailbox.
entity Courier {
    id: Uuid
    mailbox: EmailWithName {
        unique: true
    }
    active: Bool

    policy: {
        create: [Access.public]
        read: [id]
        update: [id]
    }
}
"#,
    ),
    (
        "domains/fleet/entities/van.jadpo",
        r#"use fleet/entities { Courier }

/// A van that signs in with its key, known by its registration.
entity Van {
    id: Uuid
    owner_id: Courier.id {
        on_delete: OnDelete.restricted
    }
    registration: VehicleRegistration {
        unique: true
    }
    active: Bool

    policy: {
        create: [owner_id]
        read: [id, owner_id]
        update: [owner_id]
    }
}
"#,
    ),
    (
        "domains/fleet/failures/not_signed_in.jadpo",
        r#"type NotSignedIn = Failure {
    kind: FailureKind.not_found
}
"#,
    ),
    (
        "domains/fleet/failures/taken.jadpo",
        r#"type Taken = Failure {
    kind: FailureKind.conflict
}
"#,
    ),
    (
        "domains/fleet/queries/this_courier.jadpo",
        r#"use fleet/entities { Courier }
use fleet/types { CourierCard }
use fleet/failures { NotSignedIn }

/// The courier signed in, found by the whole mailbox they signed in as.
query this_courier(courier: Principal.user) fails NotSignedIn -> CourierCard {
    from: Courier
    where: Courier.mailbox == courier.subject
    missing: NotSignedIn
}
"#,
    ),
    (
        "domains/fleet/queries/this_van.jadpo",
        r#"use fleet/entities { Van }
use fleet/types { VanCard }
use fleet/failures { NotSignedIn }

/// The van signed in, found by the whole registration it signed in as.
query this_van(van: Principal.service) fails NotSignedIn -> VanCard {
    from: Van
    where: Van.registration == van.subject
    missing: NotSignedIn
}
"#,
    ),
    (
        "domains/fleet/routes/account.jadpo",
        r#"use fleet/types { NewCourier, SignedUp }
use fleet/actions { sign_up }

routes Account {
    prefix: /account

    POST /sign-up {
        authentication: RouteAuthentication.none
        input: NewCourier
        output: SignedUp
        status: SuccessStatus.created
        run: sign_up(input)
    }
}
"#,
    ),
    (
        "domains/fleet/routes/me.jadpo",
        r#"use fleet/types { CourierCard, VanCard }
use fleet/queries { this_courier, this_van }

routes Me {
    prefix: /me

    GET / {
        output: CourierCard
        run: this_courier(current_principal.user)
    }

    GET /fresh {
        authentication: RouteAuthentication.fresh
        output: CourierCard
        run: this_courier(current_principal.user)
    }

    GET /van {
        output: VanCard
        run: this_van(current_principal.service)
    }
}
"#,
    ),
    (
        "domains/fleet/routes/me.test.jadpo",
        r#"use fleet/entities { Courier, Van }
use fleet/actions { join, register_van }

test "a courier signs in as their whole mailbox, each way in" {
    var ann: Courier = attempt join(
        EmailWithName {
            name: "Ann Lee"
            address: "ann@fleet.example"
        }
    )
    var by_session: Response = request {
        method: HttpMethod.get
        path: "/me"
        as: Principal.user(ann)
    }
    assert by_session.status == 200
    var by_provider: Response = request {
        method: HttpMethod.get
        path: "/staff/me"
        as: Principal.user(ann)
    }
    assert by_provider.status == 200
}

test "a van signs in with its key as its whole registration" {
    var bo: Courier = attempt join(
        EmailWithName {
            name: "Bo Diaz"
            address: "bo@fleet.example"
        }
    )
    var van: Van = attempt register_van(
        VehicleRegistration {
            country: Country.gb
            plate: "AB12 CDE"
        },
        Principal.user(bo)
    )
    var by_key: Response = request {
        method: HttpMethod.get
        path: "/me/van"
        as: Principal.service(van)
    }
    assert by_key.status == 200
}
"#,
    ),
    (
        "domains/fleet/routes/staff.jadpo",
        r#"use fleet/types { CourierCard }
use fleet/queries { this_courier }

routes Staff {
    api: staff
    prefix: /staff

    GET /me {
        output: CourierCard
        run: this_courier(current_principal.user)
    }
}
"#,
    ),
    (
        "domains/fleet/routes/vans.jadpo",
        r#"use fleet/types { NewVan, VanAdded }
use fleet/actions { add_van }

routes Vans {
    prefix: /vans

    POST / {
        input: NewVan
        output: VanAdded
        status: SuccessStatus.created
        run: add_van(input, current_principal.user)
    }
}
"#,
    ),
    (
        "domains/fleet/types/courier_card.jadpo",
        r#"use fleet/entities { Courier }

type CourierCard = Output {
    pick: [Courier.id, Courier.mailbox]
}
"#,
    ),
    (
        "domains/fleet/types/new_courier.jadpo",
        r#"type NewCourier = Input {
    mailbox: EmailWithName
}
"#,
    ),
    (
        "domains/fleet/types/new_van.jadpo",
        r#"type NewVan = Input {
    registration: VehicleRegistration
    key_expires_at: Instant
}
"#,
    ),
    (
        "domains/fleet/types/signed_up.jadpo",
        r#"type SignedUp = Output {
    token: ApiToken
}
"#,
    ),
    (
        "domains/fleet/types/van_added.jadpo",
        r#"type VanAdded = Output {
    key: ServiceKey
}
"#,
    ),
    (
        "domains/fleet/types/van_card.jadpo",
        r#"use fleet/entities { Van }

type VanCard = Output {
    pick: [Van.id, Van.registration]
}
"#,
    ),
];

pub(super) const KEPT_AS_JSON: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"use club/entities { Member }

application {
    revocation_delay: 5m
}

principal {
    user: {
        subject: Member.card
    }
}
"#,
    ),
    (
        "config.jadpo",
        r#"config {
    browser_origin: Url
    session_signing_key: SigningKey
}
"#,
    ),
    (
        "authentication.jadpo",
        r#"authentication {
    browser_session: {
        cookie: "club_session"
        signing_key: config.session_signing_key
        origin: config.browser_origin
    }
    api_token: {
        signing_key: config.session_signing_key
    }
}
"#,
    ),
    (
        "domains/club/actions/enrol.jadpo",
        r#"use club/entities { Member }
use club/types { MembershipCard }
use club/failures { CardTaken }

/// Enrols a member under their card.
action enrol(card: MembershipCard) fails CardTaken -> Member {
    return attempt Member.create {
        set: {
            card: card
        }
        conflict: {
            Member.card: CardTaken
        }
    }
}
"#,
    ),
    (
        "domains/club/actions/join.jadpo",
        r#"use club/entities { Member }
use club/types { Joined, NewMember }
use club/failures { CardTaken }
use club/actions { enrol }

/// Enrols a member, signs this browser in and gives their app a token.
action join(input: NewMember) fails CardTaken -> Joined {
    var member: Member = attempt enrol(input.card)
    sessions.start(member.id)
    return Joined {
        token: sessions.issue_token(member.id)
    }
}
"#,
    ),
    (
        "domains/club/entities/member.jadpo",
        r#"use club/types { MembershipCard }

/// Someone in the club, known by the card they show at the door, kept
/// whole as its JSON.
entity Member {
    id: Uuid
    card: MembershipCard {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
    }
}
"#,
    ),
    (
        "domains/club/failures/card_taken.jadpo",
        r#"type CardTaken = Failure {
    kind: FailureKind.conflict
}
"#,
    ),
    (
        "domains/club/failures/not_signed_in.jadpo",
        r#"type NotSignedIn = Failure {
    kind: FailureKind.not_found
}
"#,
    ),
    (
        "domains/club/queries/this_member.jadpo",
        r#"use club/entities { Member }
use club/types { MemberCard }
use club/failures { NotSignedIn }

/// The member signed in, found by the whole card they signed in as.
query this_member(member: Principal.user) fails NotSignedIn -> MemberCard {
    from: Member
    where: Member.card == member.subject
    missing: NotSignedIn
}
"#,
    ),
    (
        "domains/club/routes/account.jadpo",
        r#"use club/types { Joined, NewMember }
use club/actions { join }

routes Account {
    prefix: /account

    POST /join {
        authentication: RouteAuthentication.none
        input: NewMember
        output: Joined
        status: SuccessStatus.created
        run: join(input)
    }
}
"#,
    ),
    (
        "domains/club/routes/me.jadpo",
        r#"use club/types { MemberCard }
use club/queries { this_member }

routes Me {
    prefix: /me

    GET / {
        output: MemberCard
        run: this_member(current_principal.user)
    }
}
"#,
    ),
    (
        "domains/club/routes/me.test.jadpo",
        r#"use club/entities { Member }
use club/types { CardScheme, MembershipCard }
use club/actions { enrol }

test "a member signs in as their whole card" {
    var ann: Member = attempt enrol(
        MembershipCard {
            scheme: CardScheme.senior
            number: "S-0042"
        }
    )
    var own: Response = request {
        method: HttpMethod.get
        path: "/me"
        as: Principal.user(ann)
    }
    assert own.status == 200
}
"#,
    ),
    (
        "domains/club/types/card_scheme.jadpo",
        r#"type CardScheme = Enum {
    junior
    senior
}
"#,
    ),
    (
        "domains/club/types/joined.jadpo",
        r#"type Joined = Output {
    token: ApiToken
}
"#,
    ),
    (
        "domains/club/types/member_card.jadpo",
        r#"use club/entities { Member }

type MemberCard = Output {
    pick: [Member.id, Member.card]
}
"#,
    ),
    (
        "domains/club/types/membership_card.jadpo",
        r#"use club/types { CardScheme }

/// The card a member shows at the door.
type MembershipCard = Object {
    scheme: CardScheme
    number: Text
}
"#,
    ),
    (
        "domains/club/types/new_member.jadpo",
        r#"use club/types { MembershipCard }

type NewMember = Input {
    card: MembershipCard
}
"#,
    ),
];

/// A project's files, as the compiled step's corpus takes them.
fn owned(files: &[(&str, &str)]) -> Vec<(String, String)> {
    files
        .iter()
        .map(|(path, text)| (path.to_string(), text.to_string()))
        .collect()
}

/// The fleet, whose subjects are kept in several columns.
pub(crate) fn several_columns() -> Vec<(String, String)> {
    owned(SEVERAL_COLUMNS)
}

/// The club, whose subject is kept whole as its JSON.
pub(crate) fn kept_as_json() -> Vec<(String, String)> {
    owned(KEPT_AS_JSON)
}

fn jadpo(cwd: &Path, args: &[&str]) -> jadpo_cli::Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = jadpo_term::Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    jadpo_cli::run(&args, &env.with_cwd(cwd))
}

fn project(name: &str, files: &[(String, String)]) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("token-subjects-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        fs::write(path, text).expect("write");
    }
    for args in [&["migrate", "new", "initial"][..], &["records", "update"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{args:?}: {}{}", done.stdout, done.stderr);
    }
    root
}

#[test]
fn each_project_passes_its_route_tests() {
    if !jadpo_test::live() {
        return;
    }
    for (name, files, passed) in [
        ("columns", several_columns(), "2 route tests"),
        ("json", kept_as_json(), "1 route test"),
    ] {
        let root = project(name, &files);
        let tested = jadpo(&root, &["test", "--seed", "1"]);
        assert_eq!(tested.code, 0, "{name}: {}{}", tested.stdout, tested.stderr);
        assert!(tested.stdout.contains(passed), "{name}: {}", tested.stdout);
    }
}

/// What a signed part keeps of the subject: its `s` member.
fn kept_subject(signed: &str) -> serde_json::Value {
    let key = jadpo_runtime::http::signed::SigningKey::new(KEY, None);
    let payload = key.open(signed).expect("signed with the current key");
    let claims: serde_json::Value = serde_json::from_str(&payload).expect("JSON");
    claims["s"].clone()
}

fn bearer(credential: &str) -> String {
    format!("Bearer {credential}")
}

/// The round trips a route's query of the signed-in record takes; signing
/// in by a credential's row takes one more, and by a signed part none.
const READ: u32 = 2;

const ANN: &str = r#"{"name":"Ann Lee","address":"ann@fleet.example"}"#;

#[test]
fn a_subject_in_several_columns_signs_in_whole_every_way() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::with("subject-columns", SEVERAL_COLUMNS);
    let ann: serde_json::Value = serde_json::from_str(ANN).expect("JSON");
    let (joined, _) = live.send(&request(
        "POST",
        "/account/sign-up",
        &[],
        &format!(r#"{{"mailbox":{ANN}}}"#),
    ));
    assert_eq!(joined.status, 201, "{}", json(&joined));
    let session = cookie(&joined, "fleet_session").expect("a session");
    let token = json(&joined)["token"]
        .as_str()
        .expect("a token")
        .to_string();

    // A browser session: the first request reads the record, the whole
    // mailbox with it, and renews the cookie with a part keeping both its
    // columns; the next signs in by that part alone.
    let by_cookie = |session: &str| {
        live.send(&request(
            "GET",
            "/me",
            &[("cookie", &format!("fleet_session={session}"))],
            "",
        ))
    };
    let (first, trips) = by_cookie(&session);
    assert_eq!((first.status, trips), (200, READ + 1), "{}", json(&first));
    assert_eq!(json(&first)["mailbox"], ann);
    let renewed = cookie(&first, "fleet_session").expect("a renewed cookie");
    let (_, signed) = renewed.split_once('.').expect("a signed part");
    assert_eq!(
        kept_subject(signed),
        serde_json::json!(["tAnn Lee", "tann@fleet.example"])
    );
    let (second, trips) = by_cookie(&renewed);
    assert_eq!((second.status, trips), (200, READ), "{}", json(&second));
    assert_eq!(json(&second)["mailbox"], ann);

    // An API token, the same way, and a `fresh` route reading the record.
    let by_token = |token: &str, path: &str| {
        live.send(&request(
            "GET",
            path,
            &[("authorization", &bearer(token))],
            "",
        ))
    };
    let (first, _) = by_token(&token, "/me");
    assert_eq!(first.status, 200, "{}", json(&first));
    let renewed = first
        .header("x-jadpo-token")
        .expect("a renewed token")
        .to_string();
    let (second, trips) = by_token(&renewed, "/me");
    assert_eq!((second.status, trips), (200, READ), "{}", json(&second));
    assert_eq!(json(&second)["mailbox"], ann);
    let (fresh, trips) = by_token(&renewed, "/me/fresh");
    assert_eq!((fresh.status, trips), (200, READ + 1), "{}", json(&fresh));
    assert_eq!(json(&fresh)["mailbox"], ann);

    // A van's key, read with its whole registration, and the token its
    // exchange gives, which keeps both columns.
    let (added, _) = live.send(&request(
        "POST",
        "/vans",
        &[("authorization", &bearer(&token))],
        r#"{"registration":"GB AB12 CDE","key_expires_at":"2027-01-01T00:00:00Z"}"#,
    ));
    assert_eq!(added.status, 201, "{}", json(&added));
    let key = json(&added)["key"].as_str().expect("a key").to_string();
    let (by_key, trips) = by_token(&key, "/me/van");
    assert_eq!((by_key.status, trips), (200, READ + 1), "{}", json(&by_key));
    assert_eq!(json(&by_key)["registration"], "GB AB12 CDE");
    let (traded, _) = live.send(&request(
        "POST",
        "/auth/exchange",
        &[("authorization", &bearer(&key))],
        "",
    ));
    assert_eq!(traded.status, 200, "{}", json(&traded));
    let exchanged = json(&traded)["token"]
        .as_str()
        .expect("a token")
        .to_string();
    let signed = exchanged
        .strip_prefix("jadpo_service_token_")
        .expect("a service token");
    assert_eq!(
        kept_subject(signed),
        serde_json::json!(["tGB", "tAB12 CDE"])
    );
    let (by_exchanged, trips) = by_token(&exchanged, "/me/van");
    assert_eq!(
        (by_exchanged.status, trips),
        (200, READ),
        "{}",
        json(&by_exchanged)
    );
    assert_eq!(json(&by_exchanged)["registration"], "GB AB12 CDE");

    // Disabled, the van is refused once its token expires and its key is
    // read again.
    live.sql("UPDATE \"van\" SET \"active\" = false");
    live.at(START + 5 * MINUTE);
    let (refused, _) = by_token(&key, "/me/van");
    assert_eq!(refused.status, 403, "{}", json(&refused));
    assert_eq!(code(&refused), "service_disabled");
}

#[test]
fn a_subject_kept_as_json_signs_in_whole() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::with("subject-json", KEPT_AS_JSON);
    let card = serde_json::json!({"scheme": "senior", "number": "S-0042"});
    let (joined, _) = live.send(&request(
        "POST",
        "/account/join",
        &[],
        &serde_json::json!({ "card": card }).to_string(),
    ));
    assert_eq!(joined.status, 201, "{}", json(&joined));
    let session = cookie(&joined, "club_session").expect("a session");
    let by_cookie = |session: &str| {
        live.send(&request(
            "GET",
            "/me",
            &[("cookie", &format!("club_session={session}"))],
            "",
        ))
    };
    let (first, _) = by_cookie(&session);
    assert_eq!(first.status, 200, "{}", json(&first));
    assert_eq!(json(&first)["card"], card);
    let renewed = cookie(&first, "club_session").expect("a renewed cookie");
    let (_, signed) = renewed.split_once('.').expect("a signed part");
    let kept = kept_subject(signed);
    let parts = kept.as_array().expect("its parts");
    assert_eq!(parts.len(), 1, "{kept}");
    let text = parts[0].as_str().expect("text");
    let stored: serde_json::Value =
        serde_json::from_str(text.strip_prefix('j').expect("JSON")).expect("its JSON");
    assert_eq!(stored, card);
    let (second, trips) = by_cookie(&renewed);
    assert_eq!((second.status, trips), (200, READ), "{}", json(&second));
    assert_eq!(json(&second)["card"], card);
}
