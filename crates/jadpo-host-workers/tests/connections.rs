//! Six connections an invocation, and `max_concurrent:` across an
//! isolate's invocations, under workerd (docs/compiler-runtime.md §2.6,
//! "Six connections an invocation"; chunk 8.2j). workerd itself holds no
//! connection back, as its source says (its server's limit enforcer
//! enforces none), so what is seen here is the host's own counting. Each
//! test sends its requests to the bed's gate, which holds an answer's head
//! for as long as asked and counts how many of a tag's requests wait for
//! theirs at once.

use std::time::Duration;

use crate::bed::{bed, gate};

/// The gate on the bed's HTTP server holding each head `ms` milliseconds,
/// counting under `tag`, as a probe's `url=` parameter.
fn gated(ms: u64, tag: &str) -> String {
    format!("http://127.0.0.1:{}/gate/{ms}%3Ftag%3D{tag}", bed().http)
}

/// Requests sent at once without counting all reach the server together:
/// workerd holds none back. Counted among the invocation's six, at most
/// six wait for their head at once, the rest going, in the order sent, as
/// heads come. A request waits for a place within its own timeout, and one
/// whose timeout passes while it waits never went out: `NotSent`, safe to
/// send again.
#[test]
fn an_invocation_has_six_connections_waiting_for_their_heads_at_once() {
    let bed = bed();
    assert_eq!(
        bed.get(&format!(
            "/six?url={}&timeouts=5000,5000,5000,5000,5000,5000,5000,5000&counted=no",
            gated(1_500, "uncounted")
        )),
        ["status:200"; 8].join(" ")
    );
    assert_eq!(gate("uncounted").most, 8, "{:?}", gate("uncounted"));

    assert_eq!(
        bed.get(&format!(
            "/six?url={}&timeouts=5000,5000,5000,5000,5000,5000,300,5000",
            gated(1_000, "counted")
        )),
        "status:200 status:200 status:200 status:200 status:200 status:200 error:NotSent status:200"
    );
    let counted = gate("counted");
    assert_eq!((counted.most, counted.came), (6, 7), "{counted:?}");
}

/// A connection the transport opens takes a place as a request does: with
/// six requests waiting for their heads, an echo's `connect()` waits until
/// one has come.
#[test]
fn a_socket_and_a_request_count_alike() {
    let bed = bed();
    let answer = bed.get(&format!(
        "/six-mixed?url={}&echo={}",
        gated(1_000, "mixed"),
        bed.echo
    ));
    let after: u64 = answer
        .rsplit_once("after:")
        .and_then(|(_, after)| after.parse().ok())
        .unwrap_or_else(|| panic!("{answer}"));
    assert!(
        answer.starts_with(&format!(
            "fetches:{} echo:answer:pong:ping",
            ["status:200"; 6].join(",")
        )),
        "{answer}"
    );
    assert!(after >= 900, "the echo waited for a head: {answer}");
    assert_eq!(gate("mixed").most, 6);
}

/// The work a response leaves in `waitUntil` shares its invocation's six
/// with the request: four requests from each, never more than six waiting
/// at once, and all eight answered.
#[test]
fn the_work_a_response_leaves_shares_its_six() {
    let bed = bed();
    assert_eq!(
        bed.get(&format!(
            "/six-deferred?url={}&key=shared",
            gated(1_000, "deferred")
        )),
        "answered:4"
    );
    let until = std::time::Instant::now() + Duration::from_secs(10);
    while bed.get("/deferred?key=shared:4") != "done" {
        assert!(
            std::time::Instant::now() < until,
            "the deferred requests: {:?}",
            gate("deferred")
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let deferred = gate("deferred");
    assert_eq!((deferred.most, deferred.came), (6, 8), "{deferred:?}");
}

/// Two invocations calling through one connection under `max_concurrent:
/// 1`: the second waits while the first holds the isolate's one slot, and
/// once it is given back takes it in its own invocation, where its request
/// is sent and answered. Waking the second from the first would run it in
/// the first's invocation, and with nothing of its own pending workerd
/// would end it as hung.
#[test]
fn a_slot_one_invocation_gives_back_is_taken_up_by_another() {
    let bed = bed();
    let first = std::thread::spawn(move || {
        bed.get(&format!(
            "/slot?url={}&connection=handed",
            gated(1_000, "handed-first")
        ))
    });
    // The first holds the slot once its request reaches the gate.
    let until = std::time::Instant::now() + Duration::from_secs(10);
    while gate("handed-first").came == 0 {
        assert!(
            std::time::Instant::now() < until,
            "the first call never went"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let second = bed.get(&format!(
        "/slot?url={}&connection=handed",
        gated(10, "handed-second")
    ));
    let first = first.join().expect("the first call");
    assert!(
        first.starts_with("waited:") && first.ends_with(" status:200"),
        "{first}"
    );
    let waited: u64 = second
        .strip_prefix("waited:")
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|waited| waited.parse().ok())
        .unwrap_or_else(|| panic!("{second}"));
    assert!(second.ends_with(" status:200"), "{second}");
    assert!(
        (500..10_000).contains(&waited),
        "the second waited for the first's slot: {second}"
    );
    let handed = gate("handed-second");
    assert_eq!((handed.most, handed.came), (1, 1), "{handed:?}");
}

/// An invocation the platform ends mid-call never gives its slot back
/// and never says again that it holds it: Workers cancels a request whose
/// client went away before its response, and work in `waitUntil` past its
/// 30 seconds, without dropping what it held. workerd lets such a request
/// run on until it answers, so the probe leaves its slot as one would: it
/// is another call's once its holder has been quiet for five seconds.
#[test]
fn a_slot_never_given_back_is_taken_back_once_its_holder_is_quiet() {
    let bed = bed();
    assert_eq!(bed.get("/slot-left?connection=left"), "left");
    let second = bed.get(&format!(
        "/slot?url={}&connection=left",
        gated(10, "left-second")
    ));
    let waited: u64 = second
        .strip_prefix("waited:")
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|waited| waited.parse().ok())
        .unwrap_or_else(|| panic!("{second}"));
    assert!(second.ends_with(" status:200"), "{second}");
    assert!(
        (3_000..15_000).contains(&waited),
        "the slot came back once its holder was quiet: {second}"
    );
}
