//! Outgoing webhooks to the addresses customers register (area 16,
//! "Outgoing webhooks"; chunk C1b): the operations generated on the
//! entity whose `webhooks:` lists events, which both executors call through
//! [`crate::http::session`], and what each delivery of one of those events
//! hands the runtime-owned exception, [`jadpo_std::outgoing::CONNECTION`].
//!
//! - **Registering.** `register_webhook(id, url)` refuses an address that
//!   isn't HTTPS to a public host as `webhook_address_invalid`
//!   ([`address_problem`]), holds the record under the caller's `update`
//!   grants (`not_permitted` otherwise), refuses an eleventh address as
//!   `webhook_addresses_full`, then keeps the address with a secret made
//!   of 32 bytes of the host's secret randomness, never a recording's,
//!   sealed with the application's `field_encryption_key:` and bound to
//!   its column and the address's `id`. The secret is given once, in the
//!   action's output, whose response is never kept for a repeat.
//! - **Delivering.** Each attempt reads the record's addresses as the
//!   system, opens each one's secret and signs the event for it as
//!   Standard Webhooks signs, at the time it is sent, then hands the
//!   signed post ([`Outgoing`]) to the seam: the secret never reaches the
//!   seam, its log or a fake. The exception's driver posts it over HTTPS
//!   to a public address, checked as the attempt is made, following no
//!   redirect.

use std::rc::Rc;

use jadpo_std::outgoing::{
    MOST_ADDRESSES, MOST_URL_BYTES, OUTGOING_WEBHOOK, REGISTERED_FIELDS, REGISTERED_WEBHOOK,
    SECRET_BOUND, SECRET_BYTES, SECRET_PREFIX, WebhooksPlan,
};
use jadpo_std::read::sealed_context;
use jadpo_value::Value;

use crate::capability::{Call, Repeat};
use crate::ctx::Ctx;
use crate::http::identity::{execute, policed, stopped, unpoliced};
use crate::read::uuid_text;
use crate::{Fault, FaultKind, SqlType, SqlValue, Stored};

/// Why `url` can't be registered, in a few words, never the address
/// itself; `None` when it may be: HTTPS, at most
/// [`MOST_URL_BYTES`], with a host that is a name or a public address,
/// and no user name, password or fragment. A name is checked again on
/// every attempt, at the address it resolves to then, so one that
/// resolves somewhere private later is still refused.
pub fn address_problem(url: &str) -> Option<&'static str> {
    if url.len() > MOST_URL_BYTES {
        return Some("is longer than 2,048 bytes");
    }
    if url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Some("holds spaces or control characters");
    }
    let Some(rest) = url
        .get(..8)
        .filter(|scheme| scheme.eq_ignore_ascii_case("https://"))
        .map(|_| &url[8..])
    else {
        return Some("isn't HTTPS");
    };
    if rest.contains('#') {
        return Some("has a fragment, which is never sent");
    }
    let authority = rest.split(['/', '?']).next().unwrap_or_default();
    if authority.contains('@') {
        return Some("carries a user name or password");
    }
    let (host, port) = if let Some(inner) = authority.strip_prefix('[') {
        let Some((host, after)) = inner.split_once(']') else {
            return Some("isn't an address");
        };
        (host, after.strip_prefix(':'))
    } else {
        match authority.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        }
    };
    if let Some(port) = port
        && !port.parse::<u16>().is_ok_and(|port| port > 0)
    {
        return Some("names a port that isn't one");
    }
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return Some("names no host");
    }
    if host == "localhost" || host.ends_with(".localhost") {
        return Some("names this machine");
    }
    if authority.starts_with('[') {
        return match host.parse::<std::net::Ipv6Addr>() {
            Ok(ip) if crate::public_address(ip.into()) => None,
            Ok(_) => Some("is a private, loopback or link-local address"),
            Err(_) => Some("isn't an address"),
        };
    }
    // A host whose last part is a number is an IPv4 address, however it is
    // written (`127.1`, `2130706433`): only the plain form is taken.
    let last = host.rsplit('.').next().unwrap_or_default();
    if last.chars().all(|c| c.is_ascii_digit()) || last.starts_with("0x") {
        return match host.parse::<std::net::Ipv4Addr>() {
            Ok(ip) if crate::public_address(ip.into()) => None,
            Ok(_) => Some("is a private, loopback or link-local address"),
            Err(_) => Some("isn't an address written in the usual form"),
        };
    }
    let label = |label: &str| {
        !label.is_empty()
            && label.len() <= 63
            && label
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    };
    if !host.split('.').all(label) {
        return Some("names a host that isn't a name");
    }
    None
}

/// A refusal of the runtime's own, raised at `site`: its status, code and
/// message.
fn refusal(ctx: &mut Ctx<'_>, (status, code, message): (u16, &str, &str), site: u32) -> Stored {
    let name: String = code
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + chars.as_str()
            })
        })
        .collect();
    let mut failure = crate::Failure::new(format!("runtime {code}"), name, code);
    failure.kind = Some(
        match status {
            409 => "conflict",
            _ => "invalid_value",
        }
        .to_string(),
    );
    failure.status = Some(status);
    failure.message = Some(message.to_string());
    Stored::Failed(ctx.fail(failure, site))
}

/// `register_webhook`'s refusal of an address that isn't HTTPS to a public
/// host.
pub const ADDRESS_INVALID: (u16, &str, &str) = (
    422,
    "webhook_address_invalid",
    "That address can't receive webhooks. Use an https:// address on the public internet.",
);

/// `register_webhook`'s refusal of an address past
/// [`MOST_ADDRESSES`].
pub const ADDRESSES_FULL: (u16, &str, &str) = (
    409,
    "webhook_addresses_full",
    "You have as many webhook addresses as you can. Remove one to add another.",
);

fn defect(ctx: &mut Ctx<'_>, what: &str, site: u32) -> Box<Fault> {
    ctx.fault(FaultKind::Defect(what.to_string()), site)
}

/// The webhooks entity's plan, when the program has one.
fn plan(executor: &dyn crate::Executor) -> Option<&WebhooksPlan> {
    executor.plan().events?.webhooks.as_ref()
}

/// `<Entity>.register_webhook(id, url)` at site `site`: keeps the address
/// in the running action's transaction and gives its secret, the one time
/// anyone sees it.
pub async fn register(
    executor: &dyn crate::Executor,
    ctx: &mut Ctx<'_>,
    args: &[Value],
    site: u32,
) -> Stored {
    let Some(plan) = plan(executor) else {
        return Stored::Fault(defect(
            ctx,
            "a webhook registered with no `webhooks:`",
            site,
        ));
    };
    let [holder, Value::Text(url)] = args else {
        return Stored::Fault(defect(
            ctx,
            "`register_webhook` called with the wrong arguments",
            site,
        ));
    };
    let Ok(holder) = crate::to_sql(holder, SqlType::Uuid) else {
        return Stored::Fault(defect(
            ctx,
            "a webhook registered for a value that isn't an id",
            site,
        ));
    };
    let url = url.trim().to_string();
    if address_problem(&url).is_some() {
        return refusal(ctx, ADDRESS_INVALID, site);
    }
    // The record, held for the action under the caller's grants, and how
    // many addresses it has.
    let lock = if ctx.unpoliced() {
        Some((plan.lock_plain.clone(), vec![holder.clone()]))
    } else {
        policed(
            ctx,
            &plan.lock_plain,
            plan.lock.as_ref(),
            vec![holder.clone()],
        )
    };
    let Some((statement, params)) = lock else {
        return Stored::Fault(unpoliced(ctx, site));
    };
    let rows = match ctx.transaction(&plan.store).await {
        Ok(transaction) => transaction.query(&statement, &params).await,
        Err(error) => Err(error),
    };
    let held = match rows {
        Ok(rows) => rows
            .first()
            .and_then(|row| row.first())
            .map(|count| match count {
                SqlValue::Int8(count) => *count,
                _ => i64::MAX,
            }),
        Err(error) => return stopped(ctx, error, site),
    };
    let Some(count) = held else {
        return Stored::Failed(ctx.fail(crate::read::not_permitted(), site));
    };
    if count >= MOST_ADDRESSES {
        return refusal(ctx, ADDRESSES_FULL, site);
    }
    let Some(id) = ctx.new_id() else {
        return Stored::Fault(defect(ctx, "a webhook registered with no randomness", site));
    };
    let mut bytes = [0u8; SECRET_BYTES];
    if !ctx.secret_random_bytes(&mut bytes) {
        return Stored::Fault(defect(ctx, "a webhook registered with no randomness", site));
    }
    let secret = format!(
        "{SECRET_PREFIX}{}",
        jadpo_value::encoding::to_base64(&bytes)
    );
    let started = ctx.started();
    let Some(ring) = started.field_ring() else {
        return Stored::Fault(defect(
            ctx,
            "a webhook's secret is kept sealed, and `field_encryption_key:` has no value in this run",
            site,
        ));
    };
    let context = sealed_context(SECRET_BOUND, &uuid_text(&id));
    let sealed = jadpo_value::crypto::encrypt(
        secret.as_bytes(),
        ring,
        &context,
        &mut |out: &mut [u8]| ctx.secret_random_bytes(out),
    );
    let Ok(sealed) = sealed else {
        return Stored::Fault(defect(
            ctx,
            "a webhook's secret couldn't be sealed: the host gave no randomness",
            site,
        ));
    };
    let params = [
        SqlValue::Uuid(id),
        holder,
        SqlValue::Text(url),
        SqlValue::Bytes(sealed),
        SqlValue::Timestamp(ctx.now().saturating_mul(1000)),
    ];
    if let Err(error) = execute(ctx, &plan.store, REGISTER, &params).await {
        return stopped(ctx, error, site);
    }
    // Shown once: the response carrying it is never kept for a repeat.
    ctx.issued();
    Stored::Value(Value::text(&secret))
}

/// Keeps a new address: `$1` to `$5` its identity, its record, its address,
/// its sealed secret and when it was registered.
pub const REGISTER: &str = "INSERT INTO \"jadpo_webhook_addresses\" (\"id\", \"holder_id\", \"url\", \"secret\", \"registered_at\") VALUES ($1, $2, $3, $4, $5)";

/// A record's addresses without policy, for a call a test makes without a
/// caller: `$1` the record.
pub const LIST: &str = "SELECT \"id\", \"url\", \"registered_at\" FROM \"jadpo_webhook_addresses\" WHERE \"holder_id\" = $1 ORDER BY \"registered_at\", \"id\"";

/// Deletes one address without policy: `$1` the address.
pub const REMOVE: &str = "DELETE FROM \"jadpo_webhook_addresses\" WHERE \"id\" = $1";

/// The addresses a delivery has reached already: `$1` the delivery.
pub const SENT: &str =
    "SELECT \"address_id\" FROM \"jadpo_webhook_sends\" WHERE \"delivery_id\" = $1";

/// Keeps that a delivery reached an address: `$1` the delivery, `$2` the
/// address, `$3` when.
pub const RECORD_SENT: &str = "INSERT INTO \"jadpo_webhook_sends\" (\"delivery_id\", \"address_id\", \"sent_at\") VALUES ($1, $2, $3) ON CONFLICT DO NOTHING";

/// `<Entity>.webhooks(id)` at site `site`: the record's addresses, oldest
/// first, as `RegisteredWebhook`s, never their secrets.
pub async fn listed(
    executor: &dyn crate::Executor,
    ctx: &mut Ctx<'_>,
    holder: &Value,
    site: u32,
) -> Stored {
    let Some(plan) = plan(executor) else {
        return Stored::Fault(defect(ctx, "webhooks listed with no `webhooks:`", site));
    };
    let Ok(holder) = crate::to_sql(holder, SqlType::Uuid) else {
        return Stored::Fault(defect(
            ctx,
            "webhooks listed for a value that isn't an id",
            site,
        ));
    };
    let Some((statement, params)) = policed(ctx, LIST, plan.list.as_ref(), vec![holder]) else {
        return Stored::Fault(unpoliced(ctx, site));
    };
    let rows = match ctx.read_transaction(&plan.store).await {
        Ok(transaction) => transaction.query(&statement, &params).await,
        Err(error) => Err(error),
    };
    let rows = match rows {
        Ok(rows) => rows,
        Err(error) => return stopped(ctx, error, site),
    };
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let mut fields = Vec::with_capacity(REGISTERED_FIELDS.len());
        for ((name, builtin), column) in REGISTERED_FIELDS.iter().zip(&row) {
            let read = match builtin {
                jadpo_std::Builtin::Uuid => jadpo_std::read::Column::Uuid,
                jadpo_std::Builtin::Instant => jadpo_std::read::Column::Instant,
                _ => jadpo_std::read::Column::Text,
            };
            match crate::from_sql(column, &read) {
                Ok(value) => fields.push(((*name).into(), value)),
                Err(error) => return Stored::Fault(defect(ctx, &error, site)),
            }
        }
        items.push(Value::record(REGISTERED_WEBHOOK, fields));
    }
    Stored::Value(Value::list(items))
}

/// `<Entity>.remove_webhook(webhook_id)` at site `site`: deletes it in the
/// running action's transaction; an `id` no address has removes nothing.
pub async fn remove(
    executor: &dyn crate::Executor,
    ctx: &mut Ctx<'_>,
    id: &Value,
    site: u32,
) -> Stored {
    let Some(plan) = plan(executor) else {
        return Stored::Fault(defect(ctx, "a webhook removed with no `webhooks:`", site));
    };
    let Ok(id) = crate::to_sql(id, SqlType::Uuid) else {
        return Stored::Fault(defect(
            ctx,
            "a webhook removed by a value that isn't an id",
            site,
        ));
    };
    let Some((statement, params)) = policed(ctx, REMOVE, plan.remove.as_ref(), vec![id]) else {
        return Stored::Fault(unpoliced(ctx, site));
    };
    if let Err(error) = execute(ctx, &plan.store, &statement, &params).await {
        return stopped(ctx, error, site);
    }
    Stored::Value(Value::Unit)
}

/// One signed post to one address, as a delivery hands it to the
/// exception's driver: the address, the event's id, type and body, and
/// the signature made for it at `timestamp`, with the trace context of
/// the work delivering it. Never the secret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outgoing {
    pub url: String,
    pub published: crate::publishing::Published,
    /// Seconds since 1970, as `webhook-timestamp` sends them.
    pub timestamp: i64,
    /// `webhook-signature`'s value.
    pub signature: String,
}

impl Outgoing {
    /// `published` for `url`, signed with `secret` at `now` (milliseconds
    /// since 1970).
    pub fn signed(
        url: &str,
        published: &crate::publishing::Published,
        secret: &str,
        now: i64,
    ) -> Outgoing {
        let timestamp = now.div_euclid(1000);
        let key = crate::publishing::signing_key(secret);
        Outgoing {
            url: url.to_string(),
            published: published.clone(),
            timestamp,
            signature: crate::publishing::signature(
                &key,
                &published.id,
                timestamp,
                &published.body,
            ),
        }
    }

    /// What a call is given: an [`OUTGOING_WEBHOOK`].
    pub fn value(&self) -> Value {
        let text = |text: &str| Value::text(text);
        Value::record(
            OUTGOING_WEBHOOK,
            vec![
                (Rc::from("url"), text(&self.url)),
                (Rc::from("id"), text(&self.published.id)),
                (Rc::from("type"), text(&self.published.type_name)),
                (Rc::from("body"), text(&self.published.body)),
                (Rc::from("timestamp"), Value::Int(self.timestamp)),
                (Rc::from("signature"), text(&self.signature)),
                (
                    Rc::from("traceparent"),
                    self.published
                        .traceparent
                        .as_deref()
                        .map_or(Value::None, Value::text),
                ),
            ],
        )
    }

    /// What `value`, a call's input, holds: `None` for anything that isn't
    /// an outgoing webhook.
    pub fn from_value(value: &Value) -> Option<Outgoing> {
        let Value::Record(record) = value else {
            return None;
        };
        if &*record.ty != OUTGOING_WEBHOOK {
            return None;
        }
        let text = |name: &str| match record.get(name) {
            Some(Value::Text(text)) => Some(text.to_string()),
            _ => None,
        };
        let timestamp = match record.get("timestamp") {
            Some(Value::Int(seconds)) => *seconds,
            _ => return None,
        };
        Some(Outgoing {
            url: text("url")?,
            published: crate::publishing::Published {
                id: text("id")?,
                type_name: text("type")?,
                body: text("body")?,
                traceparent: text("traceparent"),
            },
            timestamp,
            signature: text("signature")?,
        })
    }

    /// The call that posts it through the exception: sent again under the
    /// event's id while it may be, so it is keyed.
    pub fn call(&self) -> Call {
        Call::new(
            jadpo_std::outgoing::CONNECTION,
            jadpo_std::outgoing::POST,
            self.value(),
            Repeat::Keyed,
        )
    }
}

/// One address a delivery posts to: its `id`, its address and its secret,
/// opened, or why it couldn't be.
pub struct Address {
    pub id: [u8; 16],
    pub url: String,
    /// Never printed ([`Address`]'s `Debug` leaves it out).
    pub secret: Result<String, String>,
}

impl std::fmt::Debug for Address {
    /// Never its secret.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Address")
            .field("id", &uuid_text(&self.id))
            .finish_non_exhaustive()
    }
}

/// The addresses the record `holder` has that the delivery `delivery`
/// hasn't reached yet, oldest first, each secret opened with the field
/// key; `Err` when a store couldn't be read.
pub async fn due(
    ctx: &Ctx<'_>,
    plan: &WebhooksPlan,
    delivery: &crate::outbox::Delivery,
    holder: &Value,
) -> Result<Vec<Address>, String> {
    let holder = crate::to_sql(holder, SqlType::Uuid)
        .map_err(|_| "an event whose record isn't an id".to_string())?;
    let host = ctx.host();
    let addresses = host
        .database(&plan.store)
        .ok_or("the store keeping the addresses isn't reached")?
        .query_once(&plan.addresses, &[holder])
        .await
        .map_err(|_| "the store keeping the addresses couldn't be read".to_string())?;
    if addresses.is_empty() {
        return Ok(Vec::new());
    }
    let sent: Vec<[u8; 16]> = host
        .database(&delivery.store)
        .ok_or("the store keeping the delivery isn't reached")?
        .query_once(SENT, &[SqlValue::Uuid(delivery.id)])
        .await
        .map_err(|_| "the store keeping the delivery couldn't be read".to_string())?
        .into_iter()
        .filter_map(|row| match row.first() {
            Some(SqlValue::Uuid(id)) => Some(*id),
            _ => None,
        })
        .collect();
    let started = ctx.started();
    let ring = started.field_ring();
    let mut out = Vec::new();
    for row in addresses {
        let (Some(SqlValue::Uuid(id)), Some(SqlValue::Text(url)), Some(SqlValue::Bytes(sealed))) =
            (row.first(), row.get(1), row.get(2))
        else {
            return Err("an address row that isn't one".to_string());
        };
        if sent.contains(id) {
            continue;
        }
        let secret = open(sealed, id, ring);
        out.push(Address {
            id: *id,
            url: url.clone(),
            secret,
        });
    }
    Ok(out)
}

/// `sealed`, the address `id`'s secret, opened with `ring`; why not, never
/// its bytes.
fn open(
    sealed: &[u8],
    id: &[u8; 16],
    ring: Option<&jadpo_value::crypto::KeyRing>,
) -> Result<String, String> {
    let ring = ring.ok_or_else(|| {
        "a webhook's secret is kept sealed, and `field_encryption_key:` has no value in this run"
            .to_string()
    })?;
    let context = sealed_context(SECRET_BOUND, &uuid_text(id));
    let opened = jadpo_value::crypto::decrypt(sealed, ring, &context).map_err(|_| {
        "a sealed webhook secret no key in this run opens: changed, moved from another address, or sealed with a key the application no longer has".to_string()
    })?;
    String::from_utf8(opened).map_err(|_| "a webhook secret that isn't text".to_string())
}

/// Keeps that `delivery` reached the address `address` at `now`. A record
/// that isn't kept only means the address may be sent the event again,
/// under the same id.
pub async fn reached(
    ctx: &Ctx<'_>,
    delivery: &crate::outbox::Delivery,
    address: &[u8; 16],
    now: i64,
) {
    if let Some(database) = ctx.host().database(&delivery.store) {
        let _ = database
            .query_once(
                RECORD_SENT,
                &[
                    SqlValue::Uuid(delivery.id),
                    SqlValue::Uuid(*address),
                    SqlValue::Timestamp(now.saturating_mul(1000)),
                ],
            )
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_to_a_public_host_is_taken() {
        for good in [
            "https://hooks.example.com/in",
            "https://hooks.example.com:8443/in?token=abc",
            "HTTPS://Hooks.Example.com/",
            "https://52.94.236.248/hook",
            "https://[2600:1f18::1]/hook",
            "https://api.example.com.",
        ] {
            assert_eq!(address_problem(good), None, "{good}");
        }
        for (bad, why) in [
            ("http://hooks.example.com/in", "isn't HTTPS"),
            ("ftp://hooks.example.com/in", "isn't HTTPS"),
            ("https://localhost/in", "names this machine"),
            ("https://api.localhost/in", "names this machine"),
            (
                "https://127.0.0.1/in",
                "is a private, loopback or link-local address",
            ),
            (
                "https://10.0.0.8/in",
                "is a private, loopback or link-local address",
            ),
            (
                "https://192.168.1.1/in",
                "is a private, loopback or link-local address",
            ),
            (
                "https://169.254.169.254/latest",
                "is a private, loopback or link-local address",
            ),
            (
                "https://100.64.0.1/",
                "is a private, loopback or link-local address",
            ),
            (
                "https://[::1]/in",
                "is a private, loopback or link-local address",
            ),
            (
                "https://[fe80::1]/in",
                "is a private, loopback or link-local address",
            ),
            (
                "https://[fd00::1]/in",
                "is a private, loopback or link-local address",
            ),
            (
                "https://[::ffff:10.0.0.1]/in",
                "is a private, loopback or link-local address",
            ),
            ("https://[fe80::1%25en0]/in", "isn't an address"),
            (
                "https://127.1/in",
                "isn't an address written in the usual form",
            ),
            (
                "https://2130706433/in",
                "isn't an address written in the usual form",
            ),
            (
                "https://0x7f.0.0.1/in",
                "isn't an address written in the usual form",
            ),
            (
                "https://user:pass@hooks.example.com/",
                "carries a user name or password",
            ),
            (
                "https://hooks.example.com/#x",
                "has a fragment, which is never sent",
            ),
            (
                "https://hooks.example.com:0/",
                "names a port that isn't one",
            ),
            (
                "https://hooks.example.com:99999/",
                "names a port that isn't one",
            ),
            ("https:///in", "names no host"),
            (
                "https://hooks example.com/",
                "holds spaces or control characters",
            ),
            (
                "https://ho!oks.example.com/",
                "names a host that isn't a name",
            ),
        ] {
            assert_eq!(address_problem(bad), Some(why), "{bad}");
        }
        let long = format!("https://hooks.example.com/{}", "a".repeat(2048));
        assert_eq!(address_problem(&long), Some("is longer than 2,048 bytes"));
    }

    #[test]
    fn an_outgoing_post_is_signed_for_its_address_and_reads_back() {
        let published = crate::publishing::Published {
            id: "4a8b1c62-0d6f-4e0a-9b1d-7c3e2f1a0b9c".into(),
            type_name: "orders.OrderPlaced".into(),
            body: "{}".into(),
            traceparent: None,
        };
        let bytes: Vec<u8> = (0u8..32).map(|at| at.wrapping_mul(31) ^ 7).collect();
        let secret = format!("whsec_{}", jadpo_value::encoding::to_base64(&bytes));
        let signed = Outgoing::signed(
            "https://hooks.example.com/in",
            &published,
            &secret,
            1_700_000_000_500,
        );
        assert_eq!(signed.timestamp, 1_700_000_000);
        assert_eq!(
            signed.signature,
            crate::publishing::signature(&bytes, &published.id, 1_700_000_000, "{}")
        );
        assert_eq!(Outgoing::from_value(&signed.value()), Some(signed.clone()));
        assert!(!format!("{:?}", signed.value()).contains(&secret));
        let other: Vec<u8> = (0u8..32).map(|at| at.wrapping_mul(17) ^ 3).collect();
        let another = format!("whsec_{}", jadpo_value::encoding::to_base64(&other));
        assert_ne!(
            Outgoing::signed(
                "https://hooks.example.com/in",
                &published,
                &another,
                1_700_000_000_500
            )
            .signature,
            signed.signature,
            "each address signs with its own secret"
        );
    }
}
