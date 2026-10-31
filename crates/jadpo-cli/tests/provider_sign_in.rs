//! Signing in through a provider the way a deployable does (area 8,
//! "Providers"; chunk A7b): discovery, the start's address, the code
//! exchanged with the client secret and the PKCE verifier, the ID token's
//! checks, the link and the browser session, over real PostgreSQL. The
//! provider is a stand-in on the host's outbound seam, which answers only
//! Google's addresses and checks what the runtime sends it; no network is
//! used and no local issuer stands in.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use jadpo_host_native::{NativeHost, Runtime};
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::http::{Request, Response, Worker, serve_request};
use jadpo_runtime::{Database, Host, LocalFuture, Outbound};
use jadpo_value::crypto::{KeyRing, public_key, sign};
use jadpo_value::encoding::to_base64url;
use serde_json::{Value, json};

const ISSUER: &str = "https://accounts.google.com";
const DISCOVERY: &str = "https://accounts.google.com/.well-known/openid-configuration";
const AUTHORIZE: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN: &str = "https://oauth2.googleapis.com/token";
const KEYS: &str = "https://www.googleapis.com/oauth2/v3/certs";
const CLIENT: &str = "desk.apps.googleusercontent.com";
const SECRET: &str = "the-client-secret";
const SITE: &str = "https://desk.example";

const FILES: [(&str, &str); 10] = [
    (
        "app.jadpo",
        r#"use desk/entities { Librarian }

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
        }
    }
}

principal {
    user: {
        subject: Librarian.email
        active: Librarian.active == true
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
        "domains/desk/entities/librarian.jadpo",
        r#"entity Librarian {
    id: Uuid
    email: Email {
        unique: true
    }
    name: Text
    active: Bool

    policy: {
        create: [Access.system]
        read: [id, Access.system]
    }
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
        "domains/desk/types/librarian_card.jadpo",
        r#"use desk/entities { Librarian }

type LibrarianCard = Output {
    pick: [Librarian.id, Librarian.name]
}
"#,
    ),
    (
        "domains/desk/queries/staff_member.jadpo",
        r#"use desk/entities { Librarian }
use desk/failures { NotOnTheStaff }

/// The member of staff with an address.
query staff_member(address: Email) fails NotOnTheStaff -> Librarian {
    from: Librarian
    where: Librarian.email == address
    missing: NotOnTheStaff
}
"#,
    ),
    (
        "domains/desk/actions/link_staff.jadpo",
        r#"use desk/entities { Librarian }
use desk/failures { NotOnTheStaff }
use desk/queries { staff_member }

/// Links a Google account to the member of staff with its address, once
/// Google has verified it.
action link_staff(first: FirstSignIn) fails NotOnTheStaff -> Librarian.id {
    if (first.method != SignInMethod.google or not first.address_verified) {
        reject NotOnTheStaff {}
    }
    match first.address {
        none -> {
            reject NotOnTheStaff {}
        }
        some(address) -> {
            var librarian: Librarian = attempt staff_member(address)
            return librarian.id
        }
    }
}
"#,
    ),
    (
        "domains/desk/queries/me.jadpo",
        r#"use desk/entities { Librarian }
use desk/types { LibrarianCard }
use desk/failures { NotOnTheStaff }

/// The signed-in member of staff.
query me(user: Principal.user) fails NotOnTheStaff -> LibrarianCard {
    from: Librarian
    where: Librarian.id == user.id
    missing: NotOnTheStaff
}
"#,
    ),
    (
        "domains/desk/routes/staff.jadpo",
        r#"use desk/types { LibrarianCard }
use desk/queries { me }

/// What a signed-in member of staff reaches.
routes Staff {
    prefix: /staff
    GET /me {
        output: LibrarianCard
        run: me(current_principal.user)
    }
}
"#,
    ),
];

fn project() -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("provider-sign-in");
    let _ = std::fs::remove_dir_all(&root);
    for (path, source) in FILES {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, source).expect("source");
    }
    for args in [&["migrate", "new", "initial"][..], &["api", "update"][..]] {
        let output = super::http::jadpo(&root, args);
        assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    }
    root
}

fn signer() -> KeyRing {
    KeyRing::from_signing_text("fedcba9876543210fedcba9876543210", None)
}

/// What the stand-in Google was asked to sign in, by code.
struct Grant {
    nonce: String,
    challenge: String,
    account: Value,
}

/// Google as the runtime reaches it: discovery, its keys and its token
/// endpoint, each at Google's own address, checking the exchange as
/// Google does.
#[derive(Default)]
struct Google {
    grants: RefCell<BTreeMap<String, Grant>>,
    /// The forms the token endpoint was sent.
    exchanged: RefCell<Vec<BTreeMap<String, String>>>,
    /// The issuer its discovery document names, when not Google's.
    issuer: RefCell<Option<String>>,
    /// The nonce its ID tokens carry, when not the one asked for.
    nonce: RefCell<Option<String>>,
}

impl Google {
    fn id_token(&self, claims: Value) -> String {
        let header = json!({"alg":"EdDSA","kid":"google-1"});
        let body = format!(
            "{}.{}",
            to_base64url(header.to_string().as_bytes()),
            to_base64url(claims.to_string().as_bytes())
        );
        let signature = sign(body.as_bytes(), &signer()).expect("signed");
        format!("{body}.{}", to_base64url(&signature))
    }
}

fn form(text: &str) -> BTreeMap<String, String> {
    text.split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| (decode(key), decode(value)))
        .collect()
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'%' if at + 2 < bytes.len() + 1 => {
                let hex = std::str::from_utf8(&bytes[at + 1..at + 3]).expect("hex");
                out.push(u8::from_str_radix(hex, 16).expect("a byte"));
                at += 3;
            }
            b'+' => {
                out.push(b' ');
                at += 1;
            }
            byte => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8(out).expect("UTF-8")
}

impl Outbound for Google {
    fn get<'a>(
        &'a self,
        url: &'a str,
        _limit: usize,
        _timeout_ms: u64,
    ) -> LocalFuture<'a, Result<(u16, Vec<u8>), String>> {
        Box::pin(async move {
            let body = match url {
                DISCOVERY => json!({
                    "issuer": self.issuer.borrow().clone().unwrap_or_else(|| ISSUER.to_string()),
                    "authorization_endpoint": AUTHORIZE,
                    "token_endpoint": TOKEN,
                    "jwks_uri": KEYS,
                }),
                KEYS => json!({"keys": [{
                    "kty":"OKP", "crv":"Ed25519", "kid":"google-1", "use":"sig",
                    "x": public_key(&signer()).expect("public key"),
                }]}),
                _ => return Err(format!("no such address: {url}")),
            };
            Ok((200, body.to_string().into_bytes()))
        })
    }

    fn post_form<'a>(
        &'a self,
        url: &'a str,
        form_text: &'a str,
        _limit: usize,
        _timeout_ms: u64,
    ) -> LocalFuture<'a, Result<(u16, Vec<u8>), String>> {
        Box::pin(async move {
            assert_eq!(url, TOKEN, "the code goes only to the token endpoint");
            let sent = form(form_text);
            self.exchanged.borrow_mut().push(sent.clone());
            let refused = || Ok((400, br#"{"error":"invalid_grant"}"#.to_vec()));
            let Some(grant) = self.grants.borrow_mut().remove(&sent["code"]) else {
                return refused();
            };
            let verifier = sent.get("code_verifier").cloned().unwrap_or_default();
            let challenge = to_base64url(&jadpo_value::hash::sha256(verifier.as_bytes()));
            if sent.get("grant_type").map(String::as_str) != Some("authorization_code")
                || sent.get("client_id").map(String::as_str) != Some(CLIENT)
                || sent.get("client_secret").map(String::as_str) != Some(SECRET)
                || sent.get("redirect_uri").map(String::as_str)
                    != Some("https://desk.example/auth/google/callback")
                || challenge != grant.challenge
            {
                return refused();
            }
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_secs() as i64;
            let mut claims = grant.account;
            claims["iss"] = ISSUER.into();
            claims["aud"] = CLIENT.into();
            claims["azp"] = CLIENT.into();
            claims["iat"] = now.into();
            claims["exp"] = (now + 3600).into();
            claims["nonce"] = self.nonce.borrow().clone().unwrap_or(grant.nonce).into();
            let body = json!({"id_token": self.id_token(claims), "token_type": "Bearer"});
            Ok((200, body.to_string().into_bytes()))
        })
    }
}

struct Native {
    inner: NativeHost,
    google: Google,
}

impl Host for Native {
    fn database(&self, store: &str) -> Option<&dyn Database> {
        self.inner.database(store)
    }
    fn random(&self, bytes: &mut [u8]) -> bool {
        self.inner.random(bytes)
    }
    fn sleep(&self, ms: u64) -> LocalFuture<'_, ()> {
        self.inner.sleep(ms)
    }
    fn now_ms(&self) -> Option<i64> {
        self.inner.now_ms()
    }
    fn monotonic_ms(&self) -> Option<u64> {
        self.inner.monotonic_ms()
    }
    fn outbound(&self) -> Option<&dyn Outbound> {
        Some(&self.google)
    }
}

fn header<'r>(response: &'r Response, name: &str) -> Vec<&'r str> {
    response
        .headers
        .iter()
        .filter(|(header, _)| header.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
        .collect()
}

/// The value `response` sets cookie `name` to.
fn cookie(response: &Response, name: &str) -> Option<String> {
    header(response, "set-cookie").into_iter().find_map(|value| {
        let (key, rest) = value.split_once('=')?;
        (key == name).then(|| rest.split(';').next().unwrap_or("").to_string())
    })
}

#[test]
fn a_member_of_staff_signs_in_with_google_through_the_runtime_alone() {
    if !jadpo_test::live() {
        return;
    }
    let root = project();
    let loaded = jadpo_db::load(&root);
    let prepared = Prepared::load(&loaded.db, loaded.project).expect("checks");
    let server = jadpo_test::database::server().expect("server");
    let url = server.create_database("provider_sign_in").expect("database");
    super::deployable::apply(&url, &super::deployable::migrations(&root));
    super::deployable::rows(
        &url,
        "INSERT INTO \"librarian\" (\"id\", \"email\", \"name\", \"active\") VALUES ('00000000-0000-4000-8000-000000000001', 'ann@desk.example', 'Ann', true) RETURNING 1",
    );
    let store = jadpo_host_native::postgres(&url).expect("store");
    let host = Native {
        inner: NativeHost::new().with_store("main", store),
        google: Google::default(),
    };
    let mut variables = BTreeMap::new();
    variables.insert(
        "DATABASE_URL".into(),
        "postgresql://fixture@127.0.0.1:5432/local".into(),
    );
    variables.insert("GOOGLE_ISSUER".into(), ISSUER.into());
    variables.insert("GOOGLE_CLIENT_ID".into(), CLIENT.into());
    variables.insert("GOOGLE_CLIENT_SECRET".into(), SECRET.into());
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
    let start = |origin: Option<&str>, body: &str| {
        let mut headers = vec![("content-type".to_string(), "application/json".to_string())];
        if let Some(origin) = origin {
            headers.push(("origin".to_string(), origin.to_string()));
        }
        send(Request {
            method: "POST".into(),
            path: "/auth/google/start".into(),
            headers,
            body: body.as_bytes().to_vec(),
            ..Request::default()
        })
    };
    // The start, from the app's own page: Google's address with this
    // sign-in's state, nonce and S256 challenge, and the binding cookie.
    let started = start(Some(SITE), r#"{"return_to":"/books"}"#);
    assert_eq!(started.status, 200, "{:?}", String::from_utf8_lossy(&started.body));
    let address: Value = serde_json::from_slice(&started.body).expect("JSON");
    let address = address["address"].as_str().expect("an address").to_string();
    let (at, query) = address.split_once('?').expect("a query");
    assert_eq!(at, AUTHORIZE);
    let asked = form(query);
    assert_eq!(asked["client_id"], CLIENT);
    assert_eq!(asked["response_type"], "code");
    assert_eq!(asked["scope"], "openid email profile");
    assert_eq!(asked["code_challenge_method"], "S256");
    assert_eq!(asked["redirect_uri"], "https://desk.example/auth/google/callback");
    let binding_header = header(&started, "set-cookie").join("\n");
    assert!(
        binding_header.contains("desk_session_provider=")
            && binding_header.contains("HttpOnly")
            && binding_header.contains("Secure")
            && binding_header.contains("SameSite=Lax"),
        "{binding_header}"
    );
    let binding = cookie(&started, "desk_session_provider").expect("a binding cookie");
    // Neither the verifier nor the nonce is kept: only digests.
    let kept = super::deployable::rows(
        &url,
        "SELECT \"state\", \"binding\" FROM \"jadpo_provider_sign_ins\"",
    );
    assert_eq!(kept.len(), 1);
    assert_ne!(kept[0][0], asked["state"]);
    assert_ne!(kept[0][1], binding);
    // Google signs Ann in and sends the browser back with a code.
    let grant = |code: &str, account: Value| {
        host.google.grants.borrow_mut().insert(
            code.to_string(),
            Grant {
                nonce: asked["nonce"].clone(),
                challenge: asked["code_challenge"].clone(),
                account,
            },
        );
    };
    let ann = json!({"sub":"110248495921238986420","email":"ann@desk.example","email_verified":true,"name":"Ann Lee","hd":"desk.example","roles":["admin"]});
    grant("code-1", ann.clone());
    let back = |code: &str, state: &str, binding: &str| {
        send(Request {
            method: "GET".into(),
            path: "/auth/google/callback".into(),
            query: Some(format!("code={code}&state={state}")),
            headers: vec![(
                "cookie".to_string(),
                format!("desk_session_provider={binding}"),
            )],
            ..Request::default()
        })
    };
    let state = asked["state"].clone();
    let signed_in = back("code-1", &state, &binding);
    assert_eq!(signed_in.status, 303, "{:?}", signed_in.code);
    assert_eq!(header(&signed_in, "location"), ["/books"]);
    let session = cookie(&signed_in, "desk_session").expect("a session");
    assert!(!session.is_empty());
    // What Google was sent: the code, the client's secret and the
    // verifier its challenge was made from.
    let exchanged = host.google.exchanged.borrow().last().cloned().expect("an exchange");
    assert_eq!(exchanged["client_secret"], SECRET);
    assert_eq!(
        to_base64url(&jadpo_value::hash::sha256(exchanged["code_verifier"].as_bytes())),
        asked["code_challenge"]
    );
    assert_eq!(
        super::deployable::rows(
            &url,
            "SELECT \"method\", \"subject\" FROM \"jadpo_sign_in_links\""
        ),
        [["google", "110248495921238986420"]]
    );
    // The session signs Ann in: no claim of Google's became a role.
    let me = send(Request {
        method: "GET".into(),
        path: "/staff/me".into(),
        headers: vec![("cookie".to_string(), format!("desk_session={session}"))],
        ..Request::default()
    });
    assert_eq!(me.status, 200);
    let card: Value = serde_json::from_slice(&me.body).expect("JSON");
    assert_eq!(card["name"], "Ann");
    // The same callback again: the sign-in was used.
    let replayed = back("code-1", &state, &binding);
    assert_eq!(header(&replayed, "location"), ["/?error=sign_in_provider_failed"]);
    // Another browser can't finish a sign-in this one started.
    let restart = |body: &str| {
        let started = start(Some(SITE), body);
        let address: Value = serde_json::from_slice(&started.body).expect("JSON");
        let asked = form(
            address["address"]
                .as_str()
                .and_then(|address| address.split_once('?'))
                .expect("a query")
                .1,
        );
        (asked, cookie(&started, "desk_session_provider").expect("binding"))
    };
    let (asked_again, _) = restart("");
    host.google.grants.borrow_mut().insert(
        "code-2".to_string(),
        Grant {
            nonce: asked_again["nonce"].clone(),
            challenge: asked_again["code_challenge"].clone(),
            account: ann.clone(),
        },
    );
    let elsewhere = back("code-2", &asked_again["state"], "another-browsers-cookie");
    assert_eq!(header(&elsewhere, "location"), ["/?error=sign_in_provider_failed"]);
    // An ID token for another sign-in, its nonce not this one's, is
    // refused.
    let (asked_third, third_binding) = restart(r#"{"return_to":"/loans?due=today"}"#);
    host.google.grants.borrow_mut().insert(
        "code-3".to_string(),
        Grant {
            nonce: asked_third["nonce"].clone(),
            challenge: asked_third["code_challenge"].clone(),
            account: ann.clone(),
        },
    );
    *host.google.nonce.borrow_mut() = Some("someone-elses-nonce".to_string());
    let swapped = back("code-3", &asked_third["state"], &third_binding);
    assert_eq!(
        header(&swapped, "location"),
        ["/loans?due=today&error=sign_in_provider_failed"]
    );
    *host.google.nonce.borrow_mut() = None;
    // A start from another site's page, or one going elsewhere, is
    // refused before anything is kept.
    assert_eq!(start(None, "").code.as_deref(), Some("csrf_rejected"));
    assert_eq!(
        start(Some("https://evil.example"), "").code.as_deref(),
        Some("csrf_rejected")
    );
    assert_eq!(
        start(Some(SITE), r#"{"return_to":"https://evil.example/"}"#)
            .code
            .as_deref(),
        Some("sign_in_return_invalid")
    );
}

#[test]
fn a_provider_whose_discovery_names_another_issuer_is_misconfigured() {
    if !jadpo_test::live() {
        return;
    }
    let root = project();
    let loaded = jadpo_db::load(&root);
    let prepared = Prepared::load(&loaded.db, loaded.project).expect("checks");
    let server = jadpo_test::database::server().expect("server");
    let url = server
        .create_database("provider_sign_in_issuer")
        .expect("database");
    super::deployable::apply(&url, &super::deployable::migrations(&root));
    let store = jadpo_host_native::postgres(&url).expect("store");
    let host = Native {
        inner: NativeHost::new().with_store("main", store),
        google: Google::default(),
    };
    // As Microsoft's `common` address publishes a placeholder in its
    // issuer, which no exact comparison matches.
    *host.google.issuer.borrow_mut() =
        Some("https://login.microsoftonline.com/{tenantid}/v2.0".to_string());
    let mut variables = BTreeMap::new();
    variables.insert(
        "DATABASE_URL".into(),
        "postgresql://fixture@127.0.0.1:5432/local".into(),
    );
    variables.insert("GOOGLE_ISSUER".into(), ISSUER.into());
    variables.insert("GOOGLE_CLIENT_ID".into(), CLIENT.into());
    variables.insert("GOOGLE_CLIENT_SECRET".into(), SECRET.into());
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
    let started = runtime.block_on(serve_request(
        &program.app(),
        &program,
        &host,
        &Worker::default(),
        &Request {
            method: "POST".into(),
            path: "/auth/google/start".into(),
            headers: vec![("origin".to_string(), SITE.to_string())],
            ..Request::default()
        },
    ));
    assert_eq!(started.code.as_deref(), Some("authentication_misconfigured"));
}
