//! Signing in with GitHub the way a deployable does (area 8, "Providers";
//! chunk N1): the start's address at GitHub, the code exchanged with the
//! client secret and the PKCE verifier, the account read from GitHub's API
//! with the access token, the link and the browser session, over real
//! PostgreSQL. GitHub is a stand-in on the host's outbound seam, which
//! answers only GitHub's own addresses and checks what the runtime sends
//! it; no network is used and no local issuer stands in.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use jadpo_host_native::{NativeHost, Runtime};
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::http::{Request, Response, Worker, serve_request};
use jadpo_runtime::{Database, Host, LocalFuture, Outbound};
use jadpo_value::encoding::to_base64url;
use serde_json::{Value, json};

use super::provider_sign_in::{cookie, form, header};

const AUTHORIZE: &str = "https://github.com/login/oauth/authorize";
const TOKEN: &str = "https://github.com/login/oauth/access_token";
const USER: &str = "https://api.github.com/user";
const EMAILS: &str = "https://api.github.com/user/emails";
const CLIENT: &str = "Iv1.8a61f9b3a7aba766";
const SECRET: &str = "the-github-client-secret";
const SITE: &str = "https://forge.example";

const FILES: [(&str, &str); 11] = [
    (
        "app.jadpo",
        r#"use accounts/entities { Member }

application {
    display_name: "Forge"
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
        github: {
            driver: Driver.github
            client_id: config.github_client_id
            credential: config.github_client_secret
        }
    }
}

principal {
    user: {
        subject: Member.handle
        active: Member.active == true
    }
}
"#,
    ),
    (
        "config.jadpo",
        r#"config {
    database_url: PostgresUrl
    github_client_id: Text
    github_client_secret: Text {
        secret: true
    }
    site: Url
}
"#,
    ),
    (
        "authentication.jadpo",
        r#"use accounts

authentication {
    browser_session: {
        cookie: "forge_session"
        origin: config.site
    }
    sign_in: {
        providers: [github]
    }
    on_first_sign_in: accounts.join
}
"#,
    ),
    (
        "domains/accounts/entities/member.jadpo",
        r#"entity Member {
    id: Uuid
    handle: Text {
        unique: true
    }
    email: Email?
    name: Text?
    active: Bool

    policy: {
        create: [Access.system]
        read: [id, Access.system]
    }
}
"#,
    ),
    (
        "domains/accounts/failures/no_address.jadpo",
        r#"/// GitHub gave no address it has verified.
type NoAddress = Failure {
    kind: FailureKind.rejected
}
"#,
    ),
    (
        "domains/accounts/failures/already_joined.jadpo",
        r#"/// Someone has that handle already.
type AlreadyJoined = Failure {
    kind: FailureKind.conflict
}
"#,
    ),
    (
        "domains/accounts/failures/not_a_member.jadpo",
        r#"/// The caller isn't a member.
type NotAMember = Failure {
    kind: FailureKind.not_found
}
"#,
    ),
    (
        "domains/accounts/actions/join.jadpo",
        r#"use accounts/entities { Member }
use accounts/failures { NoAddress, AlreadyJoined }

/// Makes an account for someone signing in with GitHub for the first
/// time, once GitHub has verified their address.
action join(first: FirstSignIn) fails NoAddress, AlreadyJoined -> Member.id {
    if (first.method != SignInMethod.github or not first.address_verified) {
        reject NoAddress {}
    }
    match first.subject {
        none -> {
            reject NoAddress {}
        }
        some(subject) -> {
            var member: Member = attempt Member.create {
                set: {
                    handle: subject
                    email: first.address
                    name: first.name
                    active: true
                }
                conflict: {
                    Member.handle: AlreadyJoined
                }
            }
            return member.id
        }
    }
}
"#,
    ),
    (
        "domains/accounts/types/member_card.jadpo",
        r#"use accounts/entities { Member }

type MemberCard = Output {
    pick: [Member.id, Member.name, Member.email]
}
"#,
    ),
    (
        "domains/accounts/queries/me.jadpo",
        r#"use accounts/entities { Member }
use accounts/types { MemberCard }
use accounts/failures { NotAMember }

/// The signed-in member.
query me(user: Principal.user) fails NotAMember -> MemberCard {
    from: Member
    where: Member.id == user.id
    missing: NotAMember
}
"#,
    ),
    (
        "domains/accounts/routes/account.jadpo",
        r#"use accounts/types { MemberCard }
use accounts/queries { me }

/// What a signed-in member reaches.
routes Account {
    prefix: /account
    GET / {
        output: MemberCard
        run: me(current_principal.user)
    }
}
"#,
    ),
];

/// The project, in a folder of `name`'s own.
fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("github-sign-in-{name}"));
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

/// What the stand-in GitHub was asked to sign in, by code.
struct Grant {
    challenge: String,
    user: Value,
    emails: Value,
}

/// GitHub as the runtime reaches it: its token endpoint and its API, each
/// at GitHub's own address, checking what it is sent as GitHub does.
#[derive(Default)]
struct GitHub {
    grants: RefCell<BTreeMap<String, Grant>>,
    /// The forms the token endpoint was sent.
    exchanged: RefCell<Vec<BTreeMap<String, String>>>,
    /// The access tokens it gave, each with the account it reads.
    tokens: RefCell<BTreeMap<String, (Value, Value)>>,
    /// Each API request: its address and headers.
    asked: RefCell<Vec<(String, Vec<(String, String)>)>>,
    /// Every address any request went to.
    reached: RefCell<Vec<String>>,
    /// What its API answers with instead, when set.
    api_status: std::cell::Cell<Option<u16>>,
}

impl Outbound for GitHub {
    fn get<'a>(
        &'a self,
        url: &'a str,
        _limit: usize,
        _timeout_ms: u64,
    ) -> LocalFuture<'a, Result<(u16, Vec<u8>), String>> {
        self.reached.borrow_mut().push(url.to_string());
        Box::pin(async move { Err(format!("asked without a token: {url}")) })
    }

    fn get_with<'a>(
        &'a self,
        url: &'a str,
        headers: &'a [(&'a str, String)],
        _limit: usize,
        _timeout_ms: u64,
    ) -> LocalFuture<'a, Result<(u16, Vec<u8>), String>> {
        Box::pin(async move {
            self.reached.borrow_mut().push(url.to_string());
            self.asked.borrow_mut().push((
                url.to_string(),
                headers
                    .iter()
                    .map(|(name, value)| (name.to_string(), value.clone()))
                    .collect(),
            ));
            if let Some(status) = self.api_status.get() {
                return Ok((status, br#"{"message":"Server Error"}"#.to_vec()));
            }
            let given = |name: &str| {
                headers
                    .iter()
                    .find(|(header, _)| header.eq_ignore_ascii_case(name))
                    .map(|(_, value)| value.clone())
            };
            // GitHub's API refuses a request with no user agent.
            if given("user-agent").is_none_or(|agent| agent.is_empty()) {
                return Ok((403, br#"{"message":"no user agent"}"#.to_vec()));
            }
            let Some(account) = given("authorization")
                .and_then(|value| value.strip_prefix("Bearer ").map(str::to_string))
                .and_then(|token| self.tokens.borrow().get(&token).cloned())
            else {
                return Ok((401, br#"{"message":"Bad credentials"}"#.to_vec()));
            };
            let body = match url {
                USER => account.0,
                EMAILS => account.1,
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
            self.reached.borrow_mut().push(url.to_string());
            assert_eq!(url, TOKEN, "the code goes only to the token endpoint");
            let sent = form(form_text);
            self.exchanged.borrow_mut().push(sent.clone());
            // GitHub answers a bad code with `200` and an `error`.
            let refused = || {
                Ok((
                    200,
                    br#"{"error":"bad_verification_code","error_description":"The code passed is incorrect or expired."}"#.to_vec(),
                ))
            };
            let Some(grant) = self.grants.borrow_mut().remove(&sent["code"]) else {
                return refused();
            };
            let verifier = sent.get("code_verifier").cloned().unwrap_or_default();
            let challenge = to_base64url(&jadpo_value::hash::sha256(verifier.as_bytes()));
            if sent.get("client_id").map(String::as_str) != Some(CLIENT)
                || sent.get("client_secret").map(String::as_str) != Some(SECRET)
                || sent.get("redirect_uri").map(String::as_str)
                    != Some("https://forge.example/auth/github/callback")
                || challenge != grant.challenge
            {
                return refused();
            }
            let token = format!("gho_{}", self.tokens.borrow().len() + 1);
            self.tokens
                .borrow_mut()
                .insert(token.clone(), (grant.user, grant.emails));
            let body = json!({"access_token": token, "token_type": "bearer", "scope": "read:user,user:email"});
            Ok((200, body.to_string().into_bytes()))
        })
    }
}

struct Native {
    inner: NativeHost,
    github: GitHub,
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
        Some(&self.github)
    }
}

#[test]
fn someone_signs_in_with_github_through_the_runtime_alone() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("forge");
    let loaded = jadpo_db::load(&root);
    let prepared = Prepared::load(&loaded.db, loaded.project).expect("checks");
    let server = jadpo_test::database::server().expect("server");
    let url = server.create_database("github_sign_in").expect("database");
    super::deployable::apply(&url, &super::deployable::migrations(&root));
    let store = jadpo_host_native::postgres(&url).expect("store");
    let host = Native {
        inner: NativeHost::new().with_store("main", store),
        github: GitHub::default(),
    };
    let mut variables = BTreeMap::new();
    variables.insert(
        "DATABASE_URL".into(),
        "postgresql://fixture@127.0.0.1:5432/local".into(),
    );
    variables.insert("GITHUB_CLIENT_ID".into(), CLIENT.into());
    variables.insert("GITHUB_CLIENT_SECRET".into(), SECRET.into());
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
    let start = |body: &str| {
        send(Request {
            method: "POST".into(),
            path: "/auth/github/start".into(),
            headers: vec![
                ("content-type".to_string(), "application/json".to_string()),
                ("origin".to_string(), SITE.to_string()),
            ],
            body: body.as_bytes().to_vec(),
            ..Request::default()
        })
    };
    let begin = |body: &str| {
        let started = start(body);
        assert_eq!(
            started.status,
            200,
            "{:?}",
            String::from_utf8_lossy(&started.body)
        );
        let address: Value = serde_json::from_slice(&started.body).expect("JSON");
        let address = address["address"].as_str().expect("an address").to_string();
        let (at, query) = address.split_once('?').expect("a query");
        assert_eq!(at, AUTHORIZE);
        (
            form(query),
            cookie(&started, "forge_session_provider").expect("a binding cookie"),
        )
    };
    // The start: GitHub's address, asking for the profile and the
    // addresses only, with a state and an S256 challenge and no nonce.
    let (asked, binding) = begin(r#"{"return_to":"/account"}"#);
    assert_eq!(asked["client_id"], CLIENT);
    assert_eq!(asked["scope"], "read:user user:email");
    assert_eq!(asked["code_challenge_method"], "S256");
    assert_eq!(
        asked["redirect_uri"],
        "https://forge.example/auth/github/callback"
    );
    assert!(!asked.contains_key("nonce"), "{asked:?}");
    assert!(!asked.contains_key("client_secret"), "{asked:?}");
    let grant = |code: &str, asked: &BTreeMap<String, String>, user: Value, emails: Value| {
        host.github.grants.borrow_mut().insert(
            code.to_string(),
            Grant {
                challenge: asked["code_challenge"].clone(),
                user,
                emails,
            },
        );
    };
    let octocat = json!({"login":"octocat","id":583231,"name":"Mona Lisa Octocat","site_admin":true,"email":"public@octo.example"});
    let addresses = json!([
        {"email":"old@octo.example","primary":false,"verified":true,"visibility":null},
        {"email":"mona@octo.example","primary":true,"verified":true,"visibility":"public"}
    ]);
    grant("code-1", &asked, octocat.clone(), addresses.clone());
    let back = |code: &str, state: &str, binding: &str| {
        send(Request {
            method: "GET".into(),
            path: "/auth/github/callback".into(),
            query: Some(format!("code={code}&state={state}")),
            headers: vec![(
                "cookie".to_string(),
                format!("forge_session_provider={binding}"),
            )],
            ..Request::default()
        })
    };
    let signed_in = back("code-1", &asked["state"], &binding);
    assert_eq!(signed_in.status, 303, "{:?}", signed_in.code);
    assert_eq!(header(&signed_in, "location"), ["/account"]);
    let session = cookie(&signed_in, "forge_session").expect("a session");
    // What GitHub was sent: the code, the client's secret and the verifier
    // its challenge was made from; then its API, with the access token.
    let exchanged = host
        .github
        .exchanged
        .borrow()
        .last()
        .cloned()
        .expect("an exchange");
    assert_eq!(exchanged["client_secret"], SECRET);
    assert_eq!(
        to_base64url(&jadpo_value::hash::sha256(
            exchanged["code_verifier"].as_bytes()
        )),
        asked["code_challenge"]
    );
    let api: Vec<(String, Vec<(String, String)>)> = host.github.asked.borrow().clone();
    assert_eq!(
        api.iter().map(|(at, _)| at.as_str()).collect::<Vec<_>>(),
        [USER, EMAILS]
    );
    for (_, headers) in &api {
        let named = |name: &str| {
            headers
                .iter()
                .find(|(header, _)| header == name)
                .map(|(_, value)| value.as_str())
        };
        assert_eq!(named("authorization"), Some("Bearer gho_1"));
        assert_eq!(named("accept"), Some("application/vnd.github+json"));
        assert_eq!(named("x-github-api-version"), Some("2022-11-28"));
    }
    // Only GitHub's own addresses were reached, and the client secret went
    // only to its token endpoint.
    assert!(
        host.github
            .reached
            .borrow()
            .iter()
            .all(|at| [TOKEN, USER, EMAILS].contains(&at.as_str())),
        "{:?}",
        host.github.reached.borrow()
    );
    // The account is linked by its numeric ID, never its login, and the
    // access token is kept nowhere.
    assert_eq!(
        super::deployable::rows(
            &url,
            "SELECT \"method\", \"subject\" FROM \"jadpo_sign_in_links\""
        ),
        [["github", "583231"]]
    );
    let dumped = super::deployable::rows(
        &url,
        "SELECT string_agg(t::text, ' ') FROM (SELECT * FROM \"jadpo_sign_in_links\") t UNION ALL SELECT string_agg(t::text, ' ') FROM (SELECT * FROM \"jadpo_sessions\") t UNION ALL SELECT string_agg(t::text, ' ') FROM (SELECT * FROM \"member\") t",
    );
    let dumped = format!("{dumped:?}");
    assert!(!dumped.contains("gho_"), "{dumped}");
    assert!(!dumped.contains(SECRET), "{dumped}");
    assert!(!dumped.contains("octocat"), "{dumped}");
    // The session signs the new member in, made from what GitHub said;
    // GitHub's sessions are never `multi_factor`.
    let me = send(Request {
        method: "GET".into(),
        path: "/account".into(),
        headers: vec![("cookie".to_string(), format!("forge_session={session}"))],
        ..Request::default()
    });
    assert_eq!(me.status, 200, "{:?}", String::from_utf8_lossy(&me.body));
    let card: Value = serde_json::from_slice(&me.body).expect("JSON");
    assert_eq!(card["name"], "Mona Lisa Octocat");
    assert_eq!(card["email"], "mona@octo.example");
    assert_eq!(
        super::deployable::rows(
            &url,
            "SELECT \"strength\" IS NULL, \"sign_in_method\" FROM \"jadpo_sessions\""
        ),
        [["Bool(true)", "github"]]
    );
    // Signing in again finds the member through the link, whatever the
    // login and address are now.
    let (again, again_binding) = begin("");
    grant(
        "code-2",
        &again,
        json!({"login":"mona-renamed","id":583231,"name":null}),
        json!([]),
    );
    let second = back("code-2", &again["state"], &again_binding);
    assert_eq!(header(&second, "location"), ["/"], "{:?}", second.code);
    assert_eq!(super::deployable::count(&url, "\"member\""), 1);
    // The same callback again: the sign-in was used.
    let replayed = back("code-1", &asked["state"], &binding);
    assert_eq!(
        header(&replayed, "location"),
        ["/?error=sign_in_provider_failed"]
    );
    // An address GitHub hasn't verified signs no one new in.
    let (third, third_binding) = begin(r#"{"return_to":"/account"}"#);
    grant(
        "code-3",
        &third,
        json!({"login":"hubot","id":7,"name":"Hubot"}),
        json!([{"email":"hubot@octo.example","primary":true,"verified":false}]),
    );
    let refused = back("code-3", &third["state"], &third_binding);
    assert_eq!(
        header(&refused, "location"),
        ["/account?error=sign_in_refused"]
    );
    // A code GitHub refuses, and a GitHub that can't answer.
    let (fourth, fourth_binding) = begin("");
    let wrong = back("not-a-code", &fourth["state"], &fourth_binding);
    assert_eq!(
        header(&wrong, "location"),
        ["/?error=sign_in_provider_failed"]
    );
    let (fifth, fifth_binding) = begin("");
    grant("code-5", &fifth, octocat, addresses);
    host.github.api_status.set(Some(502));
    let down = back("code-5", &fifth["state"], &fifth_binding);
    assert_eq!(
        header(&down, "location"),
        ["/?error=authentication_unavailable"]
    );
    host.github.api_status.set(None);
}
