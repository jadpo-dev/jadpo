//! One provisioned-subject application over the shared HTTP runtime,
//! real PostgreSQL and a generated-authority local HTTPS issuer. No
//! provider accounts or public network are used.
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use jadpo_drivers::package::Fetch;
use jadpo_host_native::{NativeHost, Postgres, Runtime};
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::http::{Request, Worker, serve_request};
use jadpo_runtime::{Database, Host, LocalFuture, Outbound};
use jadpo_value::crypto::{KeyRing, public_key, sign};
use jadpo_value::encoding::to_base64url;
use serde_json::{Value, json};

#[allow(dead_code)]
#[path = "../../jadpo-host-native/tests/authority.rs"]
pub(super) mod authority;

pub(super) const ISSUER: &str = "https://identity.example/tenant/";
pub(super) const AUDIENCE: &str = "records";
pub(super) const SUBJECT: &str = "00000000-0000-4000-8000-000000000042";
pub(super) const SOURCE: &str = r#"application {
    connections: {
        limits: {
            driver: Driver.cloudflare_rate_limits
            namespace: 1001
        }
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
    rate_limits: {
        store: limits
    }
    deployables: {
        worker: {
            serves: [main]
            target: Target.wasm
        }
    }
}
config {
    database_url: PostgresUrl
    issuer: Url
    audience: Text
}
principal {
    user: {
        subject: Reader.subject
        active: Reader.active == true
    }
}
authentication {
    external_token: {
        issuer: config.issuer
        audience: config.audience
    }
    browser_session: {
        cookie: "records_session"
    }
}
entity Reader {
    id: Uuid
    subject: Uuid {
        unique: true
    }
    active: Bool
    email: Email
    policy: {
        read: [id]
    }
}
type ReaderView = Output {
    pick: [Reader.id, Reader.email]
}
query this_reader(user: Principal.user) fails ReaderNotFound -> ReaderView {
    from: Reader
    where: Reader.id == user.id
    missing: ReaderNotFound
}
routes Self {
    prefix: /self
    GET / {
        output: ReaderView
        deadline: 1s
        run: this_reader(current_principal.user)
    }
}
"#;

pub(super) fn signer() -> KeyRing {
    KeyRing::from_signing_text("0123456789abcdef0123456789abcdef", None)
}
pub(super) fn token_with(header: Value, claims: Value) -> String {
    let body = format!(
        "{}.{}",
        to_base64url(header.to_string().as_bytes()),
        to_base64url(claims.to_string().as_bytes())
    );
    let signature = sign(body.as_bytes(), &signer()).expect("signed fixture");
    format!("{body}.{}", to_base64url(&signature))
}
pub(super) fn claims(subject: &str) -> Value {
    json!({"iss":ISSUER, "aud":AUDIENCE, "sub":subject,
        "exp":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("clock").as_secs() as i64 + 3600,
        "email":"attacker@example.test", "role":"administrator"})
}
pub(super) fn token(subject: &str) -> String {
    token_with(json!({"alg":"EdDSA", "kid":"fixture"}), claims(subject))
}

/// Generates a fixture key and token for each asymmetric provider
/// algorithm; no fixed private key or external provider is involved.
pub(super) fn provider_token(algorithm: &str, subject: &str) -> (String, Value) {
    use openssl::{
        bn::BigNumContext,
        ec::{EcGroup, EcKey},
        hash::MessageDigest,
        nid::Nid,
        pkey::PKey,
        rsa::Rsa,
        sign::Signer,
    };
    let header = json!({"alg":algorithm,"kid":algorithm});
    let body = format!(
        "{}.{}",
        to_base64url(header.to_string().as_bytes()),
        to_base64url(claims(subject).to_string().as_bytes())
    );
    let (key, jwk) = if algorithm == "RS256" {
        let rsa = Rsa::generate(2048).expect("RSA fixture");
        let jwk = json!({"kty":"RSA","alg":"RS256","kid":"RS256","n":to_base64url(&rsa.n().to_vec()),"e":to_base64url(&rsa.e().to_vec())});
        (PKey::from_rsa(rsa).expect("RSA key"), jwk)
    } else {
        assert_eq!(algorithm, "ES256");
        let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1).expect("P256 group");
        let ec = EcKey::generate(&group).expect("EC fixture");
        let mut context = BigNumContext::new().expect("context");
        let point = ec
            .public_key()
            .to_bytes(
                &group,
                openssl::ec::PointConversionForm::UNCOMPRESSED,
                &mut context,
            )
            .expect("point");
        let jwk = json!({"kty":"EC","crv":"P-256","alg":"ES256","kid":"ES256","x":to_base64url(&point[1..33]),"y":to_base64url(&point[33..65])});
        (PKey::from_ec_key(ec).expect("EC key"), jwk)
    };
    let mut signer = Signer::new(MessageDigest::sha256(), &key).expect("signer");
    signer.update(body.as_bytes()).expect("body");
    let signature = signer.sign_to_vec().expect("signature");
    let signature = if algorithm == "ES256" {
        let signature = openssl::ecdsa::EcdsaSig::from_der(&signature).expect("DER signature");
        [
            signature.r().to_vec_padded(32).expect("R"),
            signature.s().to_vec_padded(32).expect("S"),
        ]
        .concat()
    } else {
        signature
    };
    (format!("{body}.{}", to_base64url(&signature)), jwk)
}

#[derive(Default)]
pub(super) struct ProviderState {
    pub paths: Vec<String>,
    pub status: u16,
    pub mismatch: bool,
    pub extra_keys: Vec<Value>,
    pub delay_ms: u64,
}
pub(super) struct Provider {
    pub port: u16,
    pub authority: authority::Authority,
    pub state: Arc<Mutex<ProviderState>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Provider {
    pub fn start() -> Self {
        let authority = authority::Authority::signing("localhost");
        let tls = authority.server_config();
        let listener = TcpListener::bind("127.0.0.1:0").expect("provider listens");
        let port = listener.local_addr().expect("address").port();
        listener.set_nonblocking(true).expect("nonblocking");
        let stop = Arc::new(AtomicBool::new(false));
        let state = Arc::new(Mutex::new(ProviderState::default()));
        let stopped = Arc::clone(&stop);
        let shared = Arc::clone(&state);
        let thread = std::thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                let (socket, _) = match listener.accept() {
                    Ok(socket) => socket,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(_) => break,
                };
                socket
                    .set_nonblocking(false)
                    .expect("blocking fixture socket");
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .expect("timeout");
                let connection = rustls::ServerConnection::new(Arc::clone(&tls)).expect("TLS");
                let mut stream = rustls::StreamOwned::new(connection, socket);
                let mut head = Vec::new();
                let mut byte = [0];
                while head.len() < 16_384 && !head.ends_with(b"\r\n\r\n") {
                    match stream.read(&mut byte) {
                        Ok(0) => break,
                        Ok(_) => {}
                        Err(error) => {
                            eprintln!("fixture provider read: {error}");
                            break;
                        }
                    }
                    head.push(byte[0]);
                }
                let head = String::from_utf8_lossy(&head);
                let path = head.split_whitespace().nth(1).unwrap_or("").to_string();
                if path.is_empty() {
                    continue;
                }
                let (status, mismatch, extra_keys, delay_ms) = {
                    let mut state = shared.lock().expect("state");
                    state.paths.push(path.clone());
                    (
                        if state.status == 0 { 200 } else { state.status },
                        state.mismatch,
                        state.extra_keys.clone(),
                        state.delay_ms,
                    )
                };
                std::thread::sleep(Duration::from_millis(delay_ms));
                let body = if path == "/tenant/.well-known/openid-configuration" {
                    json!({"issuer":if mismatch {"https://wrong.example/"} else {ISSUER}, "jwks_uri":"https://keys.example/published"})
                } else {
                    let mut keys = vec![json!({"kty":"OKP", "crv":"Ed25519", "kid":"fixture", "x":public_key(&signer()).expect("public key")})];
                    keys.extend(extra_keys);
                    json!({"keys":keys})
                }.to_string();
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.flush();
            }
        });
        Self {
            port,
            authority,
            state,
            stop,
            thread: Some(thread),
        }
    }
}
impl Drop for Provider {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(thread) = self.thread.take() {
            thread.join().expect("provider ended");
        }
    }
}

/// A test transport maps exactly two public fixture hosts to its own TLS
/// server, checking its certificate against this test's generated CA.
/// Production Outward's public-address guard is unchanged.
struct LocalProvider {
    port: u16,
    fetch: jadpo_host_native::fetch::NativeFetch,
}
impl Outbound for LocalProvider {
    fn get<'a>(
        &'a self,
        url: &'a str,
        limit: usize,
        timeout_ms: u64,
    ) -> LocalFuture<'a, Result<(u16, Vec<u8>), String>> {
        Box::pin(async move {
            let path = url
                .strip_prefix("https://identity.example/")
                .or_else(|| url.strip_prefix("https://keys.example/"))
                .ok_or("unrecognized fixture host")?;
            let request = jadpo_runtime::provider::Request {
                method: jadpo_std::HttpMethod::Get,
                url: format!("https://localhost:{}/{path}", self.port),
                headers: Vec::new(),
                body: None,
                timeout_ms,
                reading: Default::default(),
                repeatable: true,
            };
            let response = self.fetch.fetch(&request).await.map_err(|error| {
                eprintln!("fixture TLS exchange: {error:?}");
                "fixture TLS refused"
            })?;
            if response.body.len() > limit {
                return Err("fixture response too large".into());
            }
            Ok((response.status, response.body))
        })
    }
}
/// Count request/reply wire exchanges only after PostgreSQL's complete
/// ReadyForQuery message, independent of TCP packet/read fragmentation.
#[derive(Clone, Default)]
struct WireCounting {
    trips: Rc<std::cell::Cell<u32>>,
}
impl jadpo_drivers::Transport for WireCounting {
    fn connect(
        &self,
        endpoint: &jadpo_drivers::Endpoint,
    ) -> LocalFuture<'_, std::io::Result<Box<dyn jadpo_drivers::Stream>>> {
        let endpoint = endpoint.clone();
        let trips = Rc::clone(&self.trips);
        Box::pin(async move {
            let inner = jadpo_host_native::NativeTransport
                .connect(&endpoint)
                .await?;
            Ok(Box::new(WireCounted {
                inner,
                trips,
                answered: true,
                incoming: Vec::new(),
            }) as Box<dyn jadpo_drivers::Stream>)
        })
    }
    fn spawn(&self, task: LocalFuture<'static, ()>) {
        jadpo_drivers::Transport::spawn(&jadpo_host_native::NativeTransport, task);
    }
    fn sleep(&self, millis: u64) -> LocalFuture<'_, ()> {
        jadpo_drivers::Transport::sleep(&jadpo_host_native::NativeTransport, millis)
    }
}
struct WireCounted {
    inner: Box<dyn jadpo_drivers::Stream>,
    trips: Rc<std::cell::Cell<u32>>,
    answered: bool,
    incoming: Vec<u8>,
}
impl tokio::io::AsyncRead for WireCounted {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let read = std::pin::Pin::new(&mut self.inner).poll_read(cx, buf);
        if matches!(read, std::task::Poll::Ready(Ok(()))) {
            self.incoming.extend_from_slice(&buf.filled()[before..]);
            while self.incoming.len() >= 5 {
                let size = u32::from_be_bytes(self.incoming[1..5].try_into().expect("length"))
                    as usize
                    + 1;
                assert!(
                    (5..=16_777_216).contains(&size),
                    "bounded PostgreSQL message"
                );
                if self.incoming.len() < size {
                    break;
                }
                if self.incoming[0] == b'Z' {
                    self.answered = true;
                }
                self.incoming.drain(..size);
            }
        }
        read
    }
}
impl tokio::io::AsyncWrite for WireCounted {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bytes: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let result = std::pin::Pin::new(&mut self.inner).poll_write(cx, bytes);
        if matches!(result, std::task::Poll::Ready(Ok(n)) if n > 0) && self.answered {
            self.answered = false;
            self.trips.set(self.trips.get() + 1);
        }
        result
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

struct Native {
    inner: NativeHost,
    outbound: LocalProvider,
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
        Some(&self.outbound)
    }
}

pub(super) fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("external-token-{name}"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("project");
    let configuration = SOURCE.find("config {\n").expect("config");
    let principal = SOURCE.find("principal {\n").expect("principal");
    let authentication = SOURCE.find("authentication {\n").expect("authentication");
    let entity = SOURCE.find("entity Reader {\n").expect("entity");
    let output = SOURCE.find("type ReaderView =").expect("output");
    let query = SOURCE.find("query this_reader").expect("query");
    let routes = SOURCE.find("routes Self").expect("routes");
    let files = [
        (
            "app.jadpo",
            format!(
                "use records/entities {{ Reader }}\n\n{}{}",
                &SOURCE[..configuration],
                &SOURCE[principal..authentication]
            ),
        ),
        ("config.jadpo", SOURCE[configuration..principal].to_string()),
        (
            "authentication.jadpo",
            SOURCE[authentication..entity].to_string(),
        ),
        (
            "domains/records/entities/reader.jadpo",
            SOURCE[entity..output].to_string(),
        ),
        (
            "domains/records/types/reader_view.jadpo",
            format!(
                "use records/entities {{ Reader }}\n\n{}",
                &SOURCE[output..query]
            ),
        ),
        (
            "domains/records/queries/this_reader.jadpo",
            format!(
                "use records/entities {{ Reader, ReaderNotFound }}\nuse records/types {{ ReaderView }}\n\n{}",
                &SOURCE[query..routes]
            ),
        ),
        (
            "domains/records/routes/self.jadpo",
            format!(
                "use records/queries {{ this_reader }}\nuse records/types {{ ReaderView }}\n\n{}",
                &SOURCE[routes..]
            ),
        ),
    ];
    for (path, source) in files {
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
pub(super) fn seed(url: &str) {
    super::deployable::rows(
        url,
        "INSERT INTO \"reader\" (\"id\", \"subject\", \"active\", \"email\") VALUES ('00000000-0000-4000-8000-000000000001','00000000-0000-4000-8000-000000000042',true,'stored@example.test')",
    );
}
#[test]
fn external_subjects_use_current_authority_on_the_native_runtime() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("native");
    let loaded = jadpo_db::load(&root);
    let prepared = Prepared::load(&loaded.db, loaded.project).expect("checks");
    let server = jadpo_test::database::server().expect("server");
    let url = server.create_database("external_native").expect("database");
    super::deployable::apply(&url, &super::deployable::migrations(&root));
    seed(&url);
    let provider = Provider::start();
    let asymmetric: Vec<_> = ["RS256", "ES256"]
        .iter()
        .map(|algorithm| provider_token(algorithm, SUBJECT))
        .collect();
    provider.state.lock().expect("state").extra_keys =
        asymmetric.iter().map(|(_, key)| key.clone()).collect();
    let counting = WireCounting::default();
    let store = Postgres::new(&url, Rc::new(counting.clone())).expect("store");
    let host = Native {
        inner: NativeHost::new().with_store("main", store),
        outbound: LocalProvider {
            port: provider.port,
            fetch: jadpo_host_native::fetch::NativeFetch::trusting(
                jadpo_host_native::tls::trusting(&provider.authority.root)
                    .expect("trust fixture CA"),
            ),
        },
    };
    let mut variables = BTreeMap::new();
    variables.insert(
        "DATABASE_URL".into(),
        "postgresql://fixture@127.0.0.1:5432/local".into(),
    );
    variables.insert("ISSUER".into(), ISSUER.into());
    variables.insert("AUDIENCE".into(), AUDIENCE.into());
    let config = prepared
        .config(
            &variables,
            jadpo_runtime::config::Mode::Environment,
            &|bytes| host.random(bytes),
        )
        .expect("config");
    let runtime = Runtime::new().expect("runtime");
    provider.state.lock().expect("provider").delay_ms = 1_200;
    let slow = Program::new(Prepared::load(&loaded.db, loaded.project).expect("checks"))
        .with_config(&config);
    let before = std::time::Instant::now();
    let refused = runtime.block_on(serve_request(
        &slow.app(),
        &slow,
        &host,
        &Worker::default(),
        &Request {
            method: "GET".into(),
            path: "/self".into(),
            headers: vec![("authorization".into(), format!("Bearer {}", token(SUBJECT)))],
            ..Request::default()
        },
    ));
    assert_eq!(
        refused.status, 504,
        "outer route deadline covers provider discovery"
    );
    assert!(
        before.elapsed() < Duration::from_secs(3),
        "route ends before the 3-second provider bound"
    );
    {
        let mut state = provider.state.lock().expect("state");
        state.delay_ms = 0;
        state.paths.clear();
    }
    let program = Program::new(prepared).with_config(&config);
    runtime
        .block_on(
            host.database("main")
                .expect("store")
                .query_once("SELECT 1", &[]),
        )
        .expect("warm PostgreSQL connection before measuring");
    let worker = Worker::default();
    let send = |headers: Vec<(String, String)>| {
        let before = counting.trips.get();
        let request = Request {
            method: "GET".into(),
            path: "/self".into(),
            headers,
            ..Request::default()
        };
        let response = runtime.block_on(serve_request(
            &program.app(),
            &program,
            &host,
            &worker,
            &request,
        ));
        (response, counting.trips.get() - before)
    };
    let credential = token(SUBJECT);
    for _ in 0..2 {
        let (response, trips) = send(vec![(
            "authorization".into(),
            format!("Bearer {credential}"),
        )]);
        assert_eq!(
            response.status,
            200,
            "{:?} {}, trips={trips}, provider_paths={:?}",
            response.code,
            String::from_utf8_lossy(&response.body),
            provider.state.lock().expect("provider").paths
        );
        let body: Value = serde_json::from_slice(&response.body).expect("JSON");
        assert_eq!(body["email"], "stored@example.test");
        assert_eq!(
            trips, 2,
            "one authority read plus one queued terminal-read transaction exchange"
        );
    }
    for (credential, _) in &asymmetric {
        let (response, trips) = send(vec![(
            "authorization".into(),
            format!("Bearer {credential}"),
        )]);
        assert_eq!(
            response.status,
            200,
            "{:?} {}",
            response.code,
            String::from_utf8_lossy(&response.body)
        );
        assert_eq!(
            trips, 2,
            "one authority read plus one queued terminal-read transaction exchange"
        );
    }
    assert_eq!(
        provider.state.lock().expect("state").paths.len(),
        2,
        "only discovery and key fetch"
    );
    let (response, trips) = send(vec![
        ("authorization".into(), format!("Bearer {credential}")),
        ("cookie".into(), "records_session=invalid".into()),
    ]);
    assert_eq!(response.code.as_deref(), Some("ambiguous_credentials"));
    assert_eq!(trips, 0);
    let (response, trips) = send(vec![(
        "authorization".into(),
        format!("Bearer {}", token("00000000-0000-4000-8000-000000000099")),
    )]);
    assert_eq!(response.code.as_deref(), Some("sign_in_refused"));
    assert_eq!(trips, 1);
    let (response, trips) = send(vec![(
        "authorization".into(),
        format!("Bearer {}", token("not-a-uuid")),
    )]);
    assert_eq!(response.code.as_deref(), Some("invalid_credentials"));
    assert_eq!(
        trips, 0,
        "a malformed typed subject never reaches PostgreSQL"
    );
    super::deployable::rows(&url, "UPDATE \"reader\" SET \"active\"=false");
    let (response, trips) = send(vec![(
        "authorization".into(),
        format!("Bearer {credential}"),
    )]);
    assert_eq!(response.code.as_deref(), Some("user_disabled"));
    assert_eq!(trips, 1);
}
