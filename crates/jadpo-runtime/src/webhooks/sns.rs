//! AWS SNS's messages (area 11, "Receiving from outside"; chunk 7.7b):
//! `SignatureScheme.sns` and `WebhookHandshake.sns`.
//!
//! SNS signs each message itself, with an RSA key whose certificate is at
//! the message's `SigningCertURL`, over the members its `Type` lists
//! ([`string_to_sign`]), with SHA-1 (`SignatureVersion` 1) or SHA-256 (2).
//! The runtime:
//!
//! - takes the message only from the connection's topic, whose ARN is the
//!   connection's secret (or the one it replaced while a deployment
//!   rotates it), before fetching anything, since any AWS account can have
//!   SNS sign a message from its own topic;
//! - fetches the certificate only from one of AWS's SNS hosts
//!   (`sns.<region>.amazonaws.com`, or `.amazonaws.com.cn`) over HTTPS, a
//!   `.pem` path, through the host's [`crate::Outbound`], which follows no
//!   redirect and reaches no private address; and keeps it, by its
//!   address, for a day;
//! - refuses a message signed more than five minutes ahead of its clock,
//!   or as long ago as a received id is kept (30 days), since SNS signs
//!   the time it published and its retries keep it: within that, a message
//!   sent again is a repeat by its `MessageId`.
//!
//! A `SubscriptionConfirmation`, signed the same way, is confirmed by
//! visiting its `SubscribeURL` on an SNS host, when the package declares
//! the handshake.

use std::cell::RefCell;

use jadpo_value::certificate::RsaDigest;

use super::{Refusal, TOLERANCE_MS};
use crate::http::Request;

/// The most bytes of a signing certificate read.
const CERTIFICATE_LIMIT: usize = 16 * 1024;

/// How long a fetch from AWS is waited for.
const FETCH_TIMEOUT_MS: u64 = 5_000;

/// How long a certificate fetched is kept.
const KEPT_MS: i64 = 24 * 60 * 60 * 1000;

/// How many certificates are kept at once; the oldest goes first.
const KEPT_MOST: usize = 16;

thread_local! {
    /// The certificates fetched, by address, with when.
    static CERTIFICATES: RefCell<Vec<(String, String, i64)>> = const { RefCell::new(Vec::new()) };
}

/// A message SNS signed, checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    /// `Notification`, `SubscriptionConfirmation` or
    /// `UnsubscribeConfirmation`.
    pub kind: String,
    /// When SNS published it, in milliseconds since 1970.
    pub signed_at: i64,
    /// Where a confirmation is confirmed.
    pub confirm_at: Option<String>,
}

/// Why a message wasn't taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unchecked {
    /// Its signature, its topic or its time: answered as any refusal is.
    Refused(Refusal),
    /// Its certificate couldn't be fetched: `503`, so SNS sends it again.
    Unreachable,
}

/// Checks the SNS message `request` carries against `topics` at `now`,
/// fetching its signing certificate through `outbound` when it isn't kept.
pub async fn check(
    outbound: Option<&dyn crate::Outbound>,
    topics: &[&str],
    request: &Request,
    now: i64,
) -> Result<Message, Unchecked> {
    let refused = |refusal| Err(Unchecked::Refused(refusal));
    if topics.is_empty() {
        return refused(Refusal::NoSecret);
    }
    let Some((serde_json::Value::Object(body), _)) = jadpo_value::json::parse(&request.body) else {
        return refused(Refusal::Unsigned);
    };
    let text = |name: &str| match body.get(name) {
        Some(serde_json::Value::String(text)) => Some(text.as_str()),
        _ => None,
    };
    let (Some(kind), Some(topic), Some(timestamp), Some(signature), Some(version), Some(url)) = (
        text("Type"),
        text("TopicArn"),
        text("Timestamp"),
        text("Signature"),
        text("SignatureVersion"),
        text("SigningCertURL"),
    ) else {
        return refused(Refusal::Unsigned);
    };
    // The topic first, compared with each in constant time, so nothing is
    // fetched for a message from anyone else's.
    let mut ours = false;
    for held in topics {
        ours |= jadpo_value::crypto::same_secret(topic.as_bytes(), held.as_bytes());
    }
    if !ours {
        return refused(Refusal::Mismatch);
    }
    let digest = match version {
        "1" => RsaDigest::Sha1,
        "2" => RsaDigest::Sha256,
        _ => return refused(Refusal::Unsigned),
    };
    let Some(signed) = string_to_sign(kind, &body) else {
        return refused(Refusal::Unsigned);
    };
    let Some(signature) = jadpo_value::encoding::from_base64(signature) else {
        return refused(Refusal::Unsigned);
    };
    let Some(signed_at) = jadpo_value::json::instant_from_wire(
        jadpo_std::provider::WireEncoding::Iso8601,
        &serde_json::Value::String(timestamp.to_string()),
    ) else {
        return refused(Refusal::Unsigned);
    };
    if signed_at - now > TOLERANCE_MS || now - signed_at >= super::RECEIVED_KEPT_MS {
        return refused(Refusal::Stale);
    }
    if !aws_address(url, true) {
        return refused(Refusal::Unsigned);
    }
    let Some(pem) = certificate(outbound, url, now).await else {
        return Err(Unchecked::Unreachable);
    };
    if !jadpo_value::certificate::rsa_signed(&pem, now, digest, signed.as_bytes(), &signature) {
        return refused(Refusal::Mismatch);
    }
    Ok(Message {
        kind: kind.to_string(),
        signed_at,
        confirm_at: text("SubscribeURL").map(str::to_string),
    })
}

/// What SNS signs of a message of type `kind`: each member its type lists,
/// as its name and its value each on a line of its own (`Subject` only
/// when a notification has one). `None` when one it needs is missing, or
/// for a type SNS doesn't send.
pub fn string_to_sign(
    kind: &str,
    members: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    let names: &[&str] = match kind {
        "Notification" => &[
            "Message",
            "MessageId",
            "Subject",
            "Timestamp",
            "TopicArn",
            "Type",
        ],
        "SubscriptionConfirmation" | "UnsubscribeConfirmation" => &[
            "Message",
            "MessageId",
            "SubscribeURL",
            "Timestamp",
            "Token",
            "TopicArn",
            "Type",
        ],
        _ => return None,
    };
    let mut out = String::new();
    for name in names {
        match members.get(*name).and_then(serde_json::Value::as_str) {
            Some(value) => {
                out.push_str(name);
                out.push('\n');
                out.push_str(value);
                out.push('\n');
            }
            None if *name == "Subject" => {}
            None => return None,
        }
    }
    Some(out)
}

/// Whether `url` is an address on one of AWS's SNS hosts the runtime may
/// fetch: HTTPS, `sns.<region>.amazonaws.com` or `.amazonaws.com.cn`, no
/// port, credentials or fragment; a certificate's a `.pem` path with no
/// query.
pub fn aws_address(url: &str, certificate: bool) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let (host, path) = match rest.find('/') {
        Some(at) => rest.split_at(at),
        None => return false,
    };
    let region = host
        .strip_prefix("sns.")
        .and_then(|rest| {
            rest.strip_suffix(".amazonaws.com")
                .or_else(|| rest.strip_suffix(".amazonaws.com.cn"))
        })
        .unwrap_or_default();
    let host_fits = region.len() >= 3
        && region
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if !host_fits || path.contains('#') || path.contains('\\') {
        return false;
    }
    let (path, query) = match path.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (path, None),
    };
    let printable = |text: &str| text.bytes().all(|b| b.is_ascii_graphic());
    if !printable(path) || !query.is_none_or(printable) || path.contains("/../") {
        return false;
    }
    if certificate {
        query.is_none() && path.ends_with(".pem")
    } else {
        true
    }
}

/// The certificate at `url`, kept or fetched; `None` when it can't be.
async fn certificate(
    outbound: Option<&dyn crate::Outbound>,
    url: &str,
    now: i64,
) -> Option<String> {
    let kept = CERTIFICATES.with(|kept| {
        let mut kept = kept.borrow_mut();
        kept.retain(|(_, _, fetched)| now - fetched < KEPT_MS && *fetched <= now);
        kept.iter()
            .find(|(at, _, _)| at == url)
            .map(|(_, pem, _)| pem.clone())
    });
    if kept.is_some() {
        return kept;
    }
    let (status, body) = outbound?
        .get(url, CERTIFICATE_LIMIT, FETCH_TIMEOUT_MS)
        .await
        .ok()?;
    if status != 200 {
        return None;
    }
    let pem = String::from_utf8(body).ok()?;
    CERTIFICATES.with(|kept| {
        let mut kept = kept.borrow_mut();
        if kept.len() >= KEPT_MOST {
            kept.remove(0);
        }
        kept.push((url.to_string(), pem.clone(), now));
    });
    Some(pem)
}

/// Confirms a subscription by visiting `url`, its `SubscribeURL`, on an
/// SNS host: whether SNS took it.
pub async fn confirm(outbound: Option<&dyn crate::Outbound>, url: &str) -> bool {
    if !aws_address(url, false) {
        return false;
    }
    let Some(outbound) = outbound else {
        return false;
    };
    matches!(
        outbound.get(url, CERTIFICATE_LIMIT, FETCH_TIMEOUT_MS).await,
        Ok((200..=299, _))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only AWS's SNS hosts, over HTTPS, and for a certificate a `.pem`
    /// path with nothing after it.
    #[test]
    fn only_an_sns_hosts_address_is_fetched() {
        let pem = |url| aws_address(url, true);
        assert!(pem(
            "https://sns.eu-west-2.amazonaws.com/SimpleNotificationService-abc.pem"
        ));
        assert!(pem(
            "https://sns.cn-north-1.amazonaws.com.cn/SimpleNotificationService-abc.pem"
        ));
        for refused in [
            "http://sns.eu-west-2.amazonaws.com/SimpleNotificationService-abc.pem",
            "https://sns.eu-west-2.amazonaws.com.evil.example/a.pem",
            "https://evil.example/sns.eu-west-2.amazonaws.com/a.pem",
            "https://sns.eu-west-2.amazonaws.com:8443/a.pem",
            "https://user@sns.eu-west-2.amazonaws.com/a.pem",
            "https://sns..amazonaws.com/a.pem",
            "https://sns.EU-WEST-2.amazonaws.com/a.pem",
            "https://sns.eu-west-2.amazonaws.com/a.pem?x=1",
            "https://sns.eu-west-2.amazonaws.com/a.txt",
            "https://sns.eu-west-2.amazonaws.com",
        ] {
            assert!(!pem(refused), "{refused}");
        }
        assert!(aws_address(
            "https://sns.eu-west-2.amazonaws.com/?Action=ConfirmSubscription&TopicArn=arn&Token=t",
            false
        ));
    }

    /// The members SNS's documentation lists for each type, in order.
    #[test]
    fn what_sns_signs_follows_the_messages_type() {
        let serde_json::Value::Object(members) = serde_json::json!({
            "Type": "Notification",
            "Token": "t-1",
            "TopicArn": "arn:aws:sns:eu-west-2:123456789012:orders",
            "Timestamp": "2026-10-19T09:00:00.000Z",
            "SubscribeURL": "https://sns.eu-west-2.amazonaws.com/?Action=x",
            "MessageId": "m-1",
            "Message": "hello",
            "Subject": null,
        }) else {
            unreachable!()
        };
        assert_eq!(
            string_to_sign("Notification", &members).as_deref(),
            Some(
                "Message\nhello\nMessageId\nm-1\nTimestamp\n2026-10-19T09:00:00.000Z\nTopicArn\narn:aws:sns:eu-west-2:123456789012:orders\nType\nNotification\n"
            )
        );
        let confirmation = string_to_sign("SubscriptionConfirmation", &members).unwrap();
        assert!(
            confirmation.contains("SubscribeURL\nhttps://sns"),
            "{confirmation}"
        );
        assert!(confirmation.contains("Token\nt-1\n"), "{confirmation}");
        assert_eq!(string_to_sign("Other", &members), None);
    }
}
