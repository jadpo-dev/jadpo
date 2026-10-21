//! PostgreSQL through Hyperdrive (docs/compiler-runtime.md §2.6,
//! "PostgreSQL through Hyperdrive"; area 5, "PostgreSQL through
//! Hyperdrive"; chunk 8.2i): Cloudflare's pooling, which a database
//! connection reaches on Workers when its address setting's binding is a
//! Hyperdrive binding rather than text.
//!
//! - **The setting is the binding's address.** Its user, password and
//!   database, percent-encoded, at the host and port the binding names: a
//!   name the Workers runtime made for it (`<hex>.hyperdrive.local`),
//!   which it routes to Hyperdrive inside the platform once the binding's
//!   host has been read. The binding's own `connectionString` adds
//!   `sslmode=disable`, which an address is refused for, since a database
//!   connection's TLS is its connection's in `app.jadpo`; and it writes
//!   the password as given, where a `@` or a `/` would break the address.
//! - **That host is the platform's pooler** ([`is_pooler`]), which the
//!   driver reaches without TLS: the binding's socket offers none and its
//!   address says `sslmode=disable`, since the connection stays inside
//!   Cloudflare; TLS to the database, and its certificate's check, are
//!   Hyperdrive's, as its configuration says.

use std::cell::RefCell;
use std::collections::BTreeSet;

use js_sys::Reflect;
use wasm_bindgen::JsValue;

thread_local! {
    /// The host and port of each Hyperdrive binding the isolate has read:
    /// the platform's pooler, whose route the runtime keeps for the
    /// isolate's life once the binding's host is read.
    static POOLERS: RefCell<BTreeSet<(String, u16)>> = const { RefCell::new(BTreeSet::new()) };
}

/// The address `binding` gives when it is a Hyperdrive binding, noting its
/// host as the platform's pooler; `None` for anything else.
pub(crate) fn address(binding: &JsValue) -> Option<String> {
    if !binding.is_object() {
        return None;
    }
    let text = |name: &str| {
        Reflect::get(binding, &JsValue::from_str(name))
            .ok()
            .and_then(|value| value.as_string())
    };
    // A Hyperdrive binding gives an address and a way to connect; no other
    // binding has both.
    text("connectionString")?;
    if !Reflect::get(binding, &JsValue::from_str("connect"))
        .ok()?
        .is_function()
    {
        return None;
    }
    let host = text("host")?;
    let port = Reflect::get(binding, &JsValue::from_str("port"))
        .ok()?
        .as_f64()
        .and_then(|port| u16::try_from(port as u64).ok())?;
    let user = text("user").unwrap_or_default();
    let password = text("password").unwrap_or_default();
    let database = text("database").unwrap_or_default();
    POOLERS.with(|poolers| poolers.borrow_mut().insert((host.clone(), port)));
    Some(compose(&user, &password, &host, port, &database))
}

/// Whether `host:port` is a Hyperdrive binding's, which this isolate has
/// read: the platform's pooler.
pub(crate) fn is_pooler(host: &str, port: u16) -> bool {
    POOLERS.with(|poolers| poolers.borrow().contains(&(host.to_string(), port)))
}

/// A PostgreSQL address of its parts, each percent-encoded where it needs
/// to be: no password when there is none.
fn compose(user: &str, password: &str, host: &str, port: u16, database: &str) -> String {
    let secret = if password.is_empty() {
        String::new()
    } else {
        format!(":{}", encoded(password))
    };
    format!(
        "postgresql://{}{secret}@{host}:{port}/{}",
        encoded(user),
        encoded(database)
    )
}

/// `text` with every byte but the unreserved ones (RFC 3986: letters,
/// digits, `-`, `.`, `_` and `~`) percent-encoded.
fn encoded(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_is_its_parts_percent_encoded() {
        assert_eq!(
            compose(
                "kiosk",
                "p@ss/word!",
                "5f0c.hyperdrive.local",
                5432,
                "repair cafe"
            ),
            "postgresql://kiosk:p%40ss%2Fword%21@5f0c.hyperdrive.local:5432/repair%20cafe"
        );
        assert_eq!(
            compose("app", "", "5f0c.hyperdrive.local", 5432, "app"),
            "postgresql://app@5f0c.hyperdrive.local:5432/app"
        );
        assert_eq!(encoded("Ünï~-._"), "%C3%9Cn%C3%AF~-._");
    }
}
