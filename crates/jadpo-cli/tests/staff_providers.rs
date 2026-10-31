//! Staff only through their provider (area 9, "Roles that need a way in";
//! area 8, "Company accounts"; chunk A10), the way a deployable runs it
//! over real PostgreSQL, with the stand-in Google of `provider_sign_in`:
//! a role counts only for a session started through the provider its
//! requirement names, with a second factor where it asks for one; the
//! provider signs in only the company's accounts; and its sessions last a
//! day.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use jadpo_host_native::{NativeHost, Runtime};
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::Host;
use jadpo_runtime::http::{Request, Response, Worker, serve_request};
use serde_json::{Value, json};

use super::provider_sign_in::{
    AUTHORIZE, CLIENT, Google, Grant, ISSUER, Native, SECRET, SITE, cookie, form, header,
};

const FILES: [(&str, &str); 15] = [
    (
        "app.jadpo",
        r#"use desk/entities { Patron }

application {
    display_name: "Lending desk"
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
        google: {
            driver: Driver.oidc
            issuer: config.google_issuer
            client_id: config.google_client_id
            credential: config.google_client_secret
            organisation: config.google_domain
        }
    }
}

principal {
    user: {
        subject: Patron.email
        active: Patron.active == true
    }
}
"#,
    ),
    (
        "config.jadpo",
        r#"config {
    database_url: PostgresUrl
    google_client_id: Text
    google_client_secret: Text {
        secret: true
    }
    google_issuer: Url
    google_domain: Text
    site: Url
}
"#,
    ),
    (
        "authentication.jadpo",
        r#"use desk

authentication {
    browser_session: {
        cookie: "desk_session"
        origin: config.site
    }
    sign_in: {
        providers: [google]
    }
    on_first_sign_in: desk.link_staff
}
"#,
    ),
    (
        "policy.jadpo",
        r#"use desk/types { StaffRole }
use desk

policy {
    actions: {
        desk.patrons: [StaffRole.admin, StaffRole.front_desk]
    }
}
"#,
    ),
    (
        "domains/desk/types/staff_role.jadpo",
        r#"/// What a member of staff may do.
type StaffRole = Enum {
    admin
    front_desk
}
"#,
    ),
    (
        "domains/desk/entities/patron.jadpo",
        r#"entity Patron {
    id: Uuid
    email: Email {
        unique: true
    }
    active: Bool

    policy: {
        create: [Access.system]
        read: [id, Access.system]
    }
}
"#,
    ),
    (
        "domains/desk/entities/staff_member.jadpo",
        r#"use desk/entities { Patron }
use desk/types { StaffRole }

/// A person's role on the staff, which counts only through Google.
entity StaffMember {
    id: Uuid
    patron_id: Patron.id {
        on_delete: OnDelete.cascade
    }
    role: StaffRole

    membership: {
        scope: MembershipScope.application
        member: patron_id
        role: role
        requires: {
            StaffRole.admin: {
                sign_in: [SignInMethod.google]
                strength: AuthenticationStrength.multi_factor
            }
            StaffRole.front_desk: {
                sign_in: [SignInMethod.google]
            }
        }
    }

    policy: {
        create: [Access.system]
        read: [StaffRole.admin]
    }
}
"#,
    ),
    (
        "domains/desk/types/patron_card.jadpo",
        r#"use desk/entities { Patron }

type PatronCard = Output {
    pick: [Patron.id, Patron.email]
}
"#,
    ),
    (
        "domains/desk/types/staff_card.jadpo",
        r#"use desk/entities { StaffMember }

type StaffCard = Output {
    pick: [StaffMember.id, StaffMember.role]
}
"#,
    ),
    (
        "domains/desk/failures/not_on_the_staff.jadpo",
        r#"/// The person isn't on the staff.
type NotOnTheStaff = Failure {
    kind: FailureKind.not_found
}
"#,
    ),
    (
        "domains/desk/queries/patrons.jadpo",
        r#"use desk/entities { Patron }
use desk/types { PatronCard }

/// Everyone the desk knows, for staff.
query patrons() -> List<PatronCard> {
    from: Patron
    order: [Patron.email asc]
    limit: 100
}
"#,
    ),
    (
        "domains/desk/queries/roles.jadpo",
        r#"use desk/entities { StaffMember }
use desk/types { StaffCard }

/// The staff's roles, for admins.
query roles() -> List<StaffCard> {
    from: StaffMember
    order: [StaffMember.id asc]
    limit: 100
}
"#,
    ),
    (
        "domains/desk/queries/patron_at.jadpo",
        r#"use desk/entities { Patron }
use desk/failures { NotOnTheStaff }

/// The person with an address.
query patron_at(address: Email) fails NotOnTheStaff -> Patron {
    from: Patron
    where: Patron.email == address
    missing: NotOnTheStaff
}
"#,
    ),
    (
        "domains/desk/actions/link_staff.jadpo",
        r#"use desk/entities { Patron }
use desk/failures { NotOnTheStaff }
use desk/queries { patron_at }

/// Links a Google account to the person with its verified address.
action link_staff(first: FirstSignIn) fails NotOnTheStaff -> Patron.id {
    if (first.method != SignInMethod.google or not first.address_verified) {
        reject NotOnTheStaff {}
    }
    match first.address {
        none -> {
            reject NotOnTheStaff {}
        }
        some(address) -> {
            var person: Patron = attempt patron_at(address)
            return person.id
        }
    }
}
"#,
    ),
    (
        "domains/desk/routes/staff.jadpo",
        r#"use desk/types { PatronCard, StaffCard }
use desk/queries { patrons, roles }

/// What the staff reach.
routes Staff {
    prefix: /staff
    GET /patrons {
        output: List<PatronCard>
        run: patrons()
    }
    GET /members {
        output: List<StaffCard>
        run: roles()
    }
}
"#,
    ),
];

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("staff-providers-{name}"));
    let _ = std::fs::remove_dir_all(&root);
    for (path, source) in FILES {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, source).expect("source");
    }
    for args in [
        &["migrate", "new", "initial"][..],
        &["records", "update"][..],
    ] {
        let output = super::http::jadpo(&root, args);
        assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    }
    root
}

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

#[test]
fn staff_roles_count_only_through_google_and_for_a_day() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("roles");
    let loaded = jadpo_db::load(&root);
    let prepared = Prepared::load(&loaded.db, loaded.project).expect("checks");
    let server = jadpo_test::database::server().expect("server");
    let url = server.create_database("staff_providers").expect("database");
    super::deployable::apply(&url, &super::deployable::migrations(&root));
    for statement in [
        "INSERT INTO \"patron\" (\"id\", \"email\", \"active\") VALUES ('00000000-0000-4000-8000-000000000001', 'ann@desk.example', true), ('00000000-0000-4000-8000-000000000002', 'bo@desk.example', true) RETURNING 1",
        "INSERT INTO \"staff_member\" (\"id\", \"patron_id\", \"role\") VALUES ('00000000-0000-4000-8000-0000000000a1', '00000000-0000-4000-8000-000000000001', 'front_desk'), ('00000000-0000-4000-8000-0000000000b2', '00000000-0000-4000-8000-000000000002', 'admin') RETURNING 1",
    ] {
        super::deployable::rows(&url, statement);
    }
    let store = jadpo_host_native::postgres(&url).expect("store");
    let host = Native {
        inner: NativeHost::new().with_store("main", store),
        google: Google::default(),
        skew: std::cell::Cell::new(0),
    };
    let mut variables = BTreeMap::new();
    variables.insert(
        "DATABASE_URL".into(),
        "postgresql://fixture@127.0.0.1:5432/local".into(),
    );
    variables.insert("GOOGLE_ISSUER".into(), ISSUER.into());
    variables.insert("GOOGLE_CLIENT_ID".into(), CLIENT.into());
    variables.insert("GOOGLE_CLIENT_SECRET".into(), SECRET.into());
    variables.insert("GOOGLE_DOMAIN".into(), "desk.example".into());
    variables.insert("SITE".into(), SITE.into());
    let config = prepared
        .config(
            &variables,
            jadpo_runtime::config::Mode::Environment,
            &|bytes| host.random(bytes),
        )
        .expect("config");
    let program = Program::new(prepared).with_config(&config);
    let runtime = Runtime::new().expect("runtime");
    let worker = Worker::default();
    let send = |request: Request| -> Response {
        runtime.block_on(serve_request(
            &program.app(),
            &program,
            &host,
            &worker,
            &request,
        ))
    };
    let mut codes = 0;
    // Google signs `account` in: the start from the app's own page, the
    // stand-in Google's code, and the callback with the binding cookie.
    let mut through_google = |account: Value| -> (Response, BTreeMap<String, String>) {
        let started = send(Request {
            method: "POST".into(),
            path: "/auth/google/start".into(),
            headers: vec![("origin".to_string(), SITE.to_string())],
            ..Request::default()
        });
        assert_eq!(started.status, 200, "{:?}", started.code);
        let address: Value = serde_json::from_slice(&started.body).expect("JSON");
        let address = address["address"].as_str().expect("an address").to_string();
        let (at, query) = address.split_once('?').expect("a query");
        assert_eq!(at, AUTHORIZE);
        let asked = form(query);
        let binding = cookie(&started, "desk_session_provider").expect("binding");
        codes += 1;
        let code = format!("code-{codes}");
        host.google.grants.borrow_mut().insert(
            code.clone(),
            Grant {
                nonce: asked["nonce"].clone(),
                challenge: asked["code_challenge"].clone(),
                account,
            },
        );
        let back = send(Request {
            method: "GET".into(),
            path: "/auth/google/callback".into(),
            query: Some(format!("code={code}&state={}", asked["state"])),
            headers: vec![(
                "cookie".to_string(),
                format!("desk_session_provider={binding}"),
            )],
            ..Request::default()
        });
        (back, asked)
    };
    let get = |path: &str, session: &str| {
        send(Request {
            method: "GET".into(),
            path: path.into(),
            headers: vec![("cookie".to_string(), format!("desk_session={session}"))],
            ..Request::default()
        })
    };
    let listed = |response: &Response| -> usize {
        let body: Value = serde_json::from_slice(&response.body).expect("JSON");
        body.as_array().map_or(0, Vec::len)
    };

    // Ann, at the front desk, signs in with her company account. The
    // start asks Google for the company's accounts and for how the person
    // signed in, since the admin role needs a second factor.
    let ann = json!({"sub":"1001","email":"ann@desk.example","email_verified":true,"hd":"desk.example","amr":["pwd"]});
    let (signed_in, asked) = through_google(ann);
    assert_eq!(signed_in.status, 303, "{:?}", signed_in.code);
    assert_eq!(header(&signed_in, "location"), ["/"]);
    assert_eq!(asked["hd"], "desk.example");
    assert_eq!(
        asked["claims"],
        r#"{"id_token":{"amr":{"essential":true}}}"#
    );
    let ann_session = cookie(&signed_in, "desk_session").expect("a session");
    // Her session records Google and one factor, and lasts a day.
    assert!(
        header(&signed_in, "set-cookie")
            .iter()
            .any(|set| set.starts_with("desk_session=") && set.contains("Max-Age=86400")),
        "{:?}",
        header(&signed_in, "set-cookie")
    );
    assert_eq!(
        super::deployable::rows(
            &url,
            "SELECT \"sign_in_method\", coalesce(\"strength\", 'primary'), (extract(epoch from \"ends_at\" - \"started_at\"))::bigint::text FROM \"jadpo_sessions\""
        ),
        [["google", "primary", "86400"]]
    );
    assert_eq!(get("/staff/patrons", &ann_session).status, 200);
    // She isn't an admin, so the staff list shows her nothing.
    let members = get("/staff/members", &ann_session);
    assert_eq!((members.status, listed(&members)), (200, 0));

    // Signed in as herself another way, by a session the app's own code
    // started, she holds no role.
    let token = "ab".repeat(32);
    super::deployable::rows(
        &url,
        &format!(
            "INSERT INTO \"jadpo_sessions\" (\"id\", \"principal_id\", \"verifier\", \"csrf_verifier\", \"started_at\", \"ends_at\", \"sign_in_method\") VALUES ('00000000-0000-4000-8000-0000000000c1', '00000000-0000-4000-8000-000000000001', '{}', '{}', now(), now() + interval '30 days', 'magic_link') RETURNING 1",
            jadpo_runtime::http::identity::verifier(&token),
            jadpo_runtime::http::identity::verifier(&"cd".repeat(32)),
        ),
    );
    let by_link = get("/staff/patrons", &token);
    assert_eq!(
        (by_link.status, by_link.code.as_deref()),
        (403, Some("not_permitted"))
    );

    // Bo, an admin, signs in through Google with one factor: the admin
    // role needs two, so he holds no role.
    let bo = json!({"sub":"1002","email":"bo@desk.example","email_verified":true,"hd":"desk.example","amr":["pwd"]});
    let (bo_once, _) = through_google(bo.clone());
    let bo_once = cookie(&bo_once, "desk_session").expect("a session");
    let members = get("/staff/members", &bo_once);
    assert_eq!((members.status, listed(&members)), (200, 0));
    assert_eq!(get("/staff/patrons", &bo_once).status, 403);
    // With a second factor, he is.
    let mut bo_twice = bo;
    bo_twice["amr"] = json!(["pwd", "mfa"]);
    let (bo_twice, _) = through_google(bo_twice);
    let bo_twice = cookie(&bo_twice, "desk_session").expect("a session");
    let members = get("/staff/members", &bo_twice);
    assert_eq!((members.status, listed(&members)), (200, 2));
    assert_eq!(get("/staff/patrons", &bo_twice).status, 200);

    // An account outside the company, or a personal one, signs no one in.
    for account in [
        json!({"sub":"1003","email":"ann@desk.example","email_verified":true,"hd":"elsewhere.example"}),
        json!({"sub":"1004","email":"ann@desk.example","email_verified":true}),
    ] {
        let (refused, _) = through_google(account);
        assert_eq!(header(&refused, "location"), ["/?error=sign_in_refused"]);
        assert!(cookie(&refused, "desk_session").is_none());
    }

    // A day on, Ann's Google session has ended.
    host.skew.set(DAY_MS + 60_000);
    let ended = get("/staff/patrons", &ann_session);
    assert_eq!(
        (ended.status, ended.code.as_deref()),
        (401, Some("invalid_credentials"))
    );
}
