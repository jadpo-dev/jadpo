//! Client addresses over real HTTP (area 10, chunk 5.1c): `jadpo dev` runs
//! a project on the native host and each test sends requests over TCP as a
//! proxy in front of it would. Every connection comes from `127.0.0.1`, the
//! proxy's, and the proxy says in a header which client each request came
//! from:
//!
//! - with `client_address:` two clients get allowances of their own, and a
//!   caller writing another address to the left of the proxy's is still
//!   the address the proxy saw;
//! - without it, the header is not believed and every client shares the
//!   proxy's allowance;
//! - `Forwarded` and `CF-Connecting-IP` are read the same way, and `hops:`
//!   from a setting in `.env.local`;
//! - `jadpo dev`'s request lines carry the address a request counted as.
//!
//! The project has no entities, so no database is needed. Every request
//! counts against the default allowance of 120 a minute, a 404 included.

use std::path::Path;

use super::http::{Dev, Reply, tempdir, write};

/// The most requests an address may make in a minute.
const ALLOWANCE: usize = 120;

fn project(application: &str, env_local: Option<&str>) -> tempdir::Dir {
    let dir = tempdir::Dir::new("client-address");
    let root = dir.path();
    write(root, "app.jadpo", application);
    write(
        root,
        "config.jadpo",
        r#"use edge/types { ProxyHops }

config {
    proxy_hops: ProxyHops {
        default: 1
    }
}
"#,
    );
    write(
        root,
        "domains/edge/types/proxy_hops.jadpo",
        "type ProxyHops = Int {\n    min: 1\n    max: 8\n}\n",
    );
    if let Some(env_local) = env_local {
        write(root, ".env.local", env_local);
    }
    dir
}

const FORWARDED_FOR: &str = r#"application {
    client_address: {
        source: ClientAddressSource.forwarded_for
        hops: config.proxy_hops
    }
}
"#;

/// Sends `GET /nowhere`, as a proxy would for `client`.
fn through(dev: &Dev, header: &str, value: &str) -> Reply {
    dev.send("GET", "/nowhere", &[(header, value)], "")
}

/// Uses up `address`'s allowance with `send`, which must answer 404 each
/// time, then checks the next is refused.
fn exhaust(mut send: impl FnMut() -> Reply) {
    for sent in 1..=ALLOWANCE {
        let reply = send();
        assert_eq!(reply.status, 404, "request {sent} is within the allowance");
    }
    let refused = send();
    assert_eq!(refused.status, 429, "{refused:?}");
    assert_eq!(refused.json()["error"]["code"], "rate_limited");
}

/// The request lines `jadpo dev` has written, once one says `address`: they
/// are written by a thread of their own, a moment after the answer.
fn log_with(dev: &Dev, address: &str) -> String {
    let wanted = format!("\"address\":\"{address}\"");
    let started = std::time::Instant::now();
    loop {
        let log = dev.log();
        if log.contains(&wanted) || started.elapsed() > std::time::Duration::from_secs(10) {
            return log;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

fn root(dir: &tempdir::Dir) -> &Path {
    dir.path()
}

#[test]
fn two_clients_behind_a_proxy_have_allowances_of_their_own() {
    let dir = project(FORWARDED_FOR, None);
    let dev = Dev::start_without_database(root(&dir));
    exhaust(|| through(&dev, "X-Forwarded-For", "203.0.113.9"));

    let other = through(&dev, "X-Forwarded-For", "198.51.100.4");
    assert_eq!(
        other.status, 404,
        "a second client has an allowance of its own"
    );

    // The proxy adds the address it saw to the right of whatever the caller
    // sent; claiming another address to the left changes nothing.
    let claimed = through(&dev, "X-Forwarded-For", "192.0.2.77, 203.0.113.9");
    assert_eq!(claimed.status, 429, "{claimed:?}");
    // The same client written another way is the same client.
    let spelled = through(&dev, "X-Forwarded-For", "::ffff:203.0.113.9");
    assert_eq!(spelled.status, 429, "{spelled:?}");
    let ported = through(&dev, "X-Forwarded-For", "203.0.113.9:51234");
    assert_eq!(ported.status, 429, "{ported:?}");

    // A request that names no client counts as the connection's address,
    // the proxy's own, which has its allowance too.
    let bare = dev.send("GET", "/nowhere", &[], "");
    assert_eq!(bare.status, 404);
    // A header that is not an address is never guessed at either.
    let unknown = through(&dev, "X-Forwarded-For", "unknown");
    assert_eq!(unknown.status, 404);

    // `jadpo dev` shows the address each request counted as.
    for address in ["203.0.113.9", "198.51.100.4", "127.0.0.1"] {
        let log = log_with(&dev, address);
        assert!(
            log.contains(&format!("\"address\":\"{address}\"")),
            "no request line for {address}"
        );
    }
}

#[test]
fn without_a_declaration_the_header_is_not_believed() {
    let dir = project("application {}\n", None);
    let dev = Dev::start_without_database(root(&dir));
    // Alternating clients share the proxy's one allowance.
    let mut turn = 0;
    exhaust(|| {
        turn += 1;
        let client = if turn % 2 == 0 {
            "203.0.113.9"
        } else {
            "198.51.100.4"
        };
        through(&dev, "X-Forwarded-For", client)
    });
    let third = through(&dev, "X-Forwarded-For", "192.0.2.1");
    assert_eq!(
        third.status, 429,
        "a new address in the header doesn't help"
    );
    assert!(
        log_with(&dev, "127.0.0.1").contains("\"address\":\"127.0.0.1\""),
        "no request line for the proxy's address"
    );
}

#[test]
fn forwarded_is_read_with_hops_from_a_setting() {
    let dir = project(
        r#"application {
    client_address: {
        source: ClientAddressSource.forwarded
        hops: config.proxy_hops
    }
}
"#,
        Some("PROXY_HOPS=2\n"),
    );
    let dev = Dev::start_without_database(root(&dir));
    // Two proxies in front: the second saw the first's address, and the
    // first saw the client's (an IPv6 address, quoted and in brackets).
    let chain = |client: &str| format!("for=\"[{client}]:4711\", for=10.1.1.1");
    exhaust(|| through(&dev, "Forwarded", &chain("2001:db8::17")));

    // An IPv6 client counts by its /64 (chunk 8.4d): a neighbour in it
    // shares the allowance, a client in another /64 has its own.
    let neighbour = through(&dev, "Forwarded", &chain("2001:db8::18"));
    assert_eq!(neighbour.status, 429, "the same /64");
    let other = through(&dev, "Forwarded", &chain("2001:db8:0:1::17"));
    assert_eq!(other.status, 404, "another client");
    // One proxy fewer than the setting says: the chain is short, so the
    // request counts as the connection's address, never as a guess.
    let short = through(&dev, "Forwarded", "for=2001:db8::17");
    assert_eq!(short.status, 404, "{short:?}");
    // A header that does not parse is not believed either.
    let broken = through(&dev, "Forwarded", "for=\"2001:db8::17");
    assert_eq!(broken.status, 404);
    for address in ["2001:db8::17", "127.0.0.1"] {
        assert!(
            log_with(&dev, address).contains(&format!("\"address\":\"{address}\"")),
            "no request line for {address}"
        );
    }
}

#[test]
fn cloudflare_is_read_from_its_own_header() {
    let dir = project(
        r#"application {
    client_address: {
        source: ClientAddressSource.cloudflare
    }
}
"#,
        None,
    );
    let dev = Dev::start_without_database(root(&dir));
    exhaust(|| through(&dev, "CF-Connecting-IP", "203.0.113.9"));
    let other = through(&dev, "CF-Connecting-IP", "198.51.100.4");
    assert_eq!(other.status, 404);
    // `X-Forwarded-For` is nothing to this source.
    let wrong = through(&dev, "X-Forwarded-For", "203.0.113.9");
    assert_eq!(wrong.status, 404, "{wrong:?}");
}

#[test]
fn a_setting_below_one_is_refused_before_the_app_starts() {
    let dir = project(FORWARDED_FOR, Some("PROXY_HOPS=0\n"));
    let out = super::http::jadpo(root(&dir), &["config", "check"]);
    assert_ne!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert!(
        out.stdout.contains("proxy_hops") || out.stderr.contains("proxy_hops"),
        "{}{}",
        out.stdout,
        out.stderr
    );
}
