//! The SMTP driver's conversation over the native transport's TLS
//! (area 16, "The SMTP driver"; chunk 8.1b), against a server on this
//! machine whose certificate, and the authority signing it, the test
//! makes as it runs. Nothing here reaches a network: a test's transport
//! sends every connection to that server, whatever name it was given, so
//! the name a certificate is checked against can be any.

use std::io::{Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use jadpo_drivers::smtp::{Envelope, RelayError, Server, Smtp, relay};
use jadpo_drivers::{Endpoint, Stream, Transport};
use jadpo_host_native::{NativeTransport, Runtime, tls};
use jadpo_runtime::LocalFuture;
use jadpo_runtime::capability::{Answer, Call, Capability};
use jadpo_value::Value;
use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair, KeyUsagePurpose};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

/// An authority made for one test, and a server certificate it signed.
struct Authority {
    root: Vec<u8>,
    certificate: Vec<u8>,
    key: Vec<u8>,
}

impl Authority {
    /// An authority, and a certificate it signs for `name`.
    fn signing(name: &str) -> Authority {
        let authority_key = KeyPair::generate().expect("a key");
        let mut params = CertificateParams::new(Vec::<String>::new()).expect("params");
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params
            .distinguished_name
            .push(DnType::CommonName, "Jadpo test authority");
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        let root = params.self_signed(&authority_key).expect("the authority");
        let issuer = Issuer::new(params, &authority_key);
        let key = KeyPair::generate().expect("a key");
        let certificate = CertificateParams::new(vec![name.to_string()])
            .expect("params")
            .signed_by(&key, &issuer)
            .expect("the server's certificate");
        Authority {
            root: root.der().to_vec(),
            certificate: certificate.der().to_vec(),
            key: key.serialize_der(),
        }
    }

    fn server_config(&self) -> Arc<rustls::ServerConfig> {
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .expect("versions")
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(self.certificate.clone())],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.key.clone())),
        )
        .expect("a server configuration");
        Arc::new(config)
    }
}

/// How the server secures its conversation.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Offers {
    /// `STARTTLS`, offered in its `EHLO` reply.
    StartTls,
    /// TLS from the start, as on port 465.
    FromStart,
    /// No TLS at all.
    Nothing,
    /// Nothing: it takes the connection and never greets.
    Silence,
}

/// What the server heard: each command, and whether it came over TLS.
#[derive(Default)]
struct Heard {
    commands: Vec<(String, bool)>,
    messages: Vec<String>,
}

impl Heard {
    fn over_tls(&self, start: &str) -> Option<bool> {
        self.commands
            .iter()
            .find(|(line, _)| line.starts_with(start))
            .map(|(_, tls)| *tls)
    }
}

/// A connection, in the clear or over TLS.
enum Link {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ServerConnection, TcpStream>>),
}

impl Link {
    fn say(&mut self, line: &str) {
        let bytes = format!("{line}\r\n");
        let _ = match self {
            Link::Plain(stream) => stream.write_all(bytes.as_bytes()),
            Link::Tls(stream) => stream.write_all(bytes.as_bytes()),
        };
    }

    fn line(&mut self) -> Option<String> {
        let mut bytes = Vec::new();
        let mut byte = [0u8; 1];
        while !bytes.ends_with(b"\r\n") {
            let read = match self {
                Link::Plain(stream) => stream.read(&mut byte),
                Link::Tls(stream) => stream.read(&mut byte),
            };
            if read.ok()? == 0 {
                return None;
            }
            bytes.push(byte[0]);
        }
        bytes.truncate(bytes.len() - 2);
        String::from_utf8(bytes).ok()
    }

    fn secure(self, config: &Arc<rustls::ServerConfig>) -> Option<Link> {
        let Link::Plain(stream) = self else {
            return None;
        };
        let connection = rustls::ServerConnection::new(Arc::clone(config)).ok()?;
        Some(Link::Tls(Box::new(rustls::StreamOwned::new(
            connection, stream,
        ))))
    }
}

/// An SMTP server on this machine for one conversation.
struct Mail {
    port: u16,
    heard: Arc<Mutex<Heard>>,
    thread: Option<JoinHandle<()>>,
}

impl Mail {
    fn start(offers: Offers, authority: &Authority) -> Mail {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port on this machine");
        let port = listener.local_addr().expect("its address").port();
        let heard = Arc::new(Mutex::new(Heard::default()));
        let config = authority.server_config();
        let thread = {
            let heard = Arc::clone(&heard);
            std::thread::spawn(move || {
                if let Ok((stream, _)) = listener.accept() {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                    converse(Link::Plain(stream), offers, &config, &heard);
                }
            })
        };
        Mail {
            port,
            heard,
            thread: Some(thread),
        }
    }

    /// What it heard, once its conversation is over.
    fn heard(mut self) -> Heard {
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        std::mem::take(&mut *self.heard.lock().expect("heard"))
    }
}

fn converse(
    mut link: Link,
    offers: Offers,
    config: &Arc<rustls::ServerConfig>,
    heard: &Mutex<Heard>,
) {
    match offers {
        Offers::Silence => {
            std::thread::sleep(Duration::from_millis(600));
            return;
        }
        Offers::FromStart => match link.secure(config) {
            Some(secure) => link = secure,
            None => return,
        },
        Offers::StartTls | Offers::Nothing => {}
    }
    link.say("220 localhost ESMTP test");
    while let Some(line) = link.line() {
        let tls = matches!(link, Link::Tls(_));
        heard
            .lock()
            .expect("heard")
            .commands
            .push((line.clone(), tls));
        let upper = line.to_ascii_uppercase();
        if upper.starts_with("EHLO") {
            if offers == Offers::StartTls && !tls {
                link.say("250-localhost");
                link.say("250 STARTTLS");
            } else {
                link.say("250-localhost");
                link.say("250 AUTH PLAIN");
            }
        } else if upper == "STARTTLS" {
            link.say("220 2.0.0 Ready to start TLS");
            match link.secure(config) {
                Some(secure) => link = secure,
                None => return,
            }
        } else if upper.starts_with("AUTH PLAIN") {
            link.say("235 2.7.0 Authentication successful");
        } else if upper.starts_with("MAIL FROM:") || upper.starts_with("RCPT TO:") {
            link.say("250 2.1.0 Ok");
        } else if upper == "DATA" {
            link.say("354 End data with <CR><LF>.<CR><LF>");
            let mut data = String::new();
            loop {
                let Some(part) = link.line() else { return };
                if part == "." {
                    break;
                }
                data.push_str(&part);
                data.push_str("\r\n");
            }
            heard.lock().expect("heard").messages.push(data);
            link.say("250 2.0.0 Ok: queued");
        } else if upper == "QUIT" {
            link.say("221 2.0.0 Bye");
            return;
        } else {
            link.say("502 5.5.2 Not implemented");
        }
    }
}

/// The native transport, trusting only the test's authority, and sending
/// every connection to the test's server on this machine.
struct ToTheTestServer {
    config: Arc<rustls::ClientConfig>,
    port: u16,
}

impl ToTheTestServer {
    fn new(authority: &Authority, mail: &Mail) -> ToTheTestServer {
        ToTheTestServer {
            config: tls::trusting(&authority.root).expect("the test's authority"),
            port: mail.port,
        }
    }
}

impl Transport for ToTheTestServer {
    fn connect(&self, _: &Endpoint) -> LocalFuture<'_, std::io::Result<Box<dyn Stream>>> {
        Box::pin(async move {
            NativeTransport
                .connect(&Endpoint::Tcp {
                    host: "127.0.0.1".into(),
                    port: self.port,
                })
                .await
        })
    }

    fn upgrade<'a>(
        &'a self,
        stream: Box<dyn Stream>,
        name: &'a str,
    ) -> LocalFuture<'a, std::io::Result<Box<dyn Stream>>> {
        Box::pin(tls::upgrade(Arc::clone(&self.config), stream, name))
    }

    fn spawn(&self, task: LocalFuture<'static, ()>) {
        NativeTransport.spawn(task);
    }

    fn sleep(&self, millis: u64) -> LocalFuture<'_, ()> {
        NativeTransport.sleep(millis)
    }

    fn local_address(&self, stream: &dyn Stream) -> Option<IpAddr> {
        NativeTransport.local_address(stream)
    }
}

fn envelope() -> Envelope {
    Envelope {
        from: "hello@acme.test".into(),
        to: vec!["ana@example.test".into()],
        data: b"Subject: Welcome\r\n\r\nHi Ana\r\n".to_vec(),
    }
}

fn relayed(transport: &dyn Transport, server: &Server) -> Result<(), RelayError> {
    Runtime::new()
        .expect("a runtime")
        .block_on(relay(transport, server, &envelope()))
}

#[test]
fn starttls_secures_the_conversation_before_the_credential_goes() {
    let authority = Authority::signing("localhost");
    let mail = Mail::start(Offers::StartTls, &authority);
    let transport = ToTheTestServer::new(&authority, &mail);
    let server = Server::new("localhost", 587).signing_in("acme", "s3cret");
    assert_eq!(relayed(&transport, &server), Ok(()));
    let heard = mail.heard();
    assert_eq!(heard.over_tls("STARTTLS"), Some(false));
    assert_eq!(heard.over_tls("AUTH PLAIN"), Some(true));
    assert_eq!(heard.over_tls("MAIL FROM:<hello@acme.test>"), Some(true));
    // EHLO again once secure, naming this end by its address.
    let hellos: Vec<_> = heard
        .commands
        .iter()
        .filter(|(line, _)| line.starts_with("EHLO "))
        .collect();
    assert_eq!(hellos.len(), 2);
    assert_eq!(hellos[0].0, "EHLO [127.0.0.1]");
    assert!(hellos[1].1);
    assert_eq!(heard.messages, vec!["Subject: Welcome\r\n\r\nHi Ana\r\n"]);
}

#[test]
fn port_465_speaks_tls_from_the_start() {
    let authority = Authority::signing("smtp.example.test");
    let mail = Mail::start(Offers::FromStart, &authority);
    let transport = ToTheTestServer::new(&authority, &mail);
    let server = Server::new("smtp.example.test", 465).signing_in("acme", "s3cret");
    assert_eq!(relayed(&transport, &server), Ok(()));
    let heard = mail.heard();
    assert!(heard.commands.iter().all(|(_, tls)| *tls));
    assert!(heard.over_tls("STARTTLS").is_none());
    assert_eq!(heard.messages.len(), 1);
}

#[test]
fn a_certificate_for_another_name_is_refused_and_nothing_is_sent() {
    let authority = Authority::signing("smtp.example.test");
    let mail = Mail::start(Offers::StartTls, &authority);
    let transport = ToTheTestServer::new(&authority, &mail);
    let server = Server::new("localhost", 587).signing_in("acme", "s3cret");
    let Err(RelayError::Insecure(why)) = relayed(&transport, &server) else {
        panic!("refused as insecure");
    };
    assert!(why.contains("no secure connection"), "{why}");
    let heard = mail.heard();
    assert!(heard.over_tls("AUTH").is_none());
    assert!(heard.messages.is_empty());
}

#[test]
fn a_certificate_mozillas_roots_dont_vouch_for_is_refused_natively() {
    let authority = Authority::signing("localhost");
    let mail = Mail::start(Offers::StartTls, &authority);
    let server = Server::new("localhost", mail.port).signing_in("acme", "s3cret");
    let mail_capability = Smtp::new(Rc::new(NativeTransport), server);
    let call = Call::new(
        "mail",
        "send",
        Value::record(
            "EmailMessage",
            vec![
                (Rc::from("from"), Value::text("hello@acme.test")),
                (Rc::from("to"), Value::text("ana@example.test")),
                (Rc::from("subject"), Value::text("Welcome")),
                (Rc::from("html"), Value::None),
                (Rc::from("text"), Value::text("Hi Ana")),
                (Rc::from("attachments"), Value::List(Vec::new().into())),
            ],
        ),
        jadpo_std::subscriber::Repeat::Unkeyed,
    );
    let answer = Runtime::new()
        .expect("a runtime")
        .block_on(mail_capability.call(&call));
    assert_eq!(answer, Answer::Misconfigured);
    let heard = mail.heard();
    assert!(heard.over_tls("AUTH").is_none());
    assert!(heard.messages.is_empty());
}

#[test]
fn a_server_elsewhere_offering_no_tls_is_never_sent_the_credential() {
    let authority = Authority::signing("smtp.example.test");
    let mail = Mail::start(Offers::Nothing, &authority);
    let transport = ToTheTestServer::new(&authority, &mail);
    let server = Server::new("smtp.example.test", 587).signing_in("acme", "s3cret");
    let Err(RelayError::Insecure(why)) = relayed(&transport, &server) else {
        panic!("refused as insecure");
    };
    assert!(why.contains("offers no TLS"), "{why}");
    let heard = mail.heard();
    assert!(heard.over_tls("AUTH").is_none());
    assert!(heard.over_tls("MAIL").is_none());
}

#[test]
fn a_server_on_this_machine_offering_no_tls_is_reached_in_the_clear() {
    let authority = Authority::signing("localhost");
    let mail = Mail::start(Offers::Nothing, &authority);
    let server = Server::new("127.0.0.1", mail.port).signing_in("acme", "s3cret");
    assert_eq!(relayed(&NativeTransport, &server), Ok(()));
    let heard = mail.heard();
    assert_eq!(heard.over_tls("AUTH PLAIN"), Some(false));
    assert_eq!(heard.messages.len(), 1);
}

#[test]
fn a_server_that_never_greets_took_nothing() {
    let authority = Authority::signing("localhost");
    let mail = Mail::start(Offers::Silence, &authority);
    let mut server = Server::new("127.0.0.1", mail.port);
    server.timeout_ms = 100;
    let Err(RelayError::NotSent(why)) = relayed(&NativeTransport, &server) else {
        panic!("not sent");
    };
    assert!(why.contains("in time"), "{why}");
    assert!(mail.heard().commands.is_empty());
}
