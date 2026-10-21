//! The Workers host under workerd (docs/compiler-runtime.md §2.6, "The
//! Workers host"; chunk 8.2a): each test asks the probe Worker
//! (examples/probe.rs) to use one part of the host, against servers on
//! this machine, and reads what it saw.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::bed::{bed, closed};

/// Random bytes come from Web Crypto's generator, each fill new, and a
/// fill longer than one `getRandomValues` gives is filled whole.
#[test]
fn random_bytes_come_from_web_crypto() {
    assert_eq!(bed().get("/random"), "filled distinct whole");
}

/// The wall clock is `Date.now()`, and a timer waits on `setTimeout` for
/// at least as long as it was asked to, the monotonic clock moving on as
/// far.
#[test]
fn the_clocks_and_the_timer_are_the_workers() {
    let answer = bed().get("/clock");
    let field = |name: &str| -> i64 {
        answer
            .split_whitespace()
            .find_map(|part| part.strip_prefix(&format!("{name}=")))
            .and_then(|value| value.parse().ok())
            .unwrap_or_else(|| panic!("no {name} in {answer:?}"))
    };
    let now = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap();
    assert!(
        (field("now") - now).abs() < 60_000,
        "{answer} against {now}"
    );
    assert!(field("slept") >= 120, "{answer}");
    assert!(field("monotonic") >= 120, "{answer}");
}

/// A setting's variable is the Worker's binding of that name, and one
/// the Worker isn't given isn't set.
#[test]
fn settings_are_read_from_the_workers_bindings() {
    assert_eq!(bed().get("/variable?name=GREETING"), "set:hello");
    assert_eq!(bed().get("/variable?name=NOT_GIVEN"), "unset");
}

/// Background work has Workers' budgets: 30 seconds for what a response
/// leaves, and for what is handed on, which runs as such work, and 15
/// minutes for the cron (area 12, "Running background work").
#[test]
fn background_work_has_the_workers_budgets() {
    assert_eq!(
        bed().get("/budget"),
        "after_response=30000 tick=900000 handed_on=30000"
    );
}

/// The transport opens a TCP connection with `connect()` and reads and
/// writes over it.
#[test]
fn the_transport_connects_over_tcp() {
    let bed = bed();
    assert_eq!(
        bed.get(&format!("/echo?host=127.0.0.1&port={}", bed.echo)),
        "answer:pong:ping"
    );
}

/// A stream the transport opened is upgraded to TLS with `startTls()`,
/// the server's certificate checked against the roots workerd trusts and
/// for the name given; a certificate for another name, or from an
/// authority nobody vouches for, fails the check.
#[test]
fn the_transport_upgrades_to_tls_checked_for_the_name() {
    let bed = bed();
    assert_eq!(
        bed.get(&format!(
            "/echo?host=localhost&port={}&tls=localhost",
            bed.echo_tls
        )),
        "answer:pong:ping"
    );
    assert_eq!(
        bed.get(&format!(
            "/echo?host=localhost&port={}&tls=db.example.com",
            bed.echo_tls
        )),
        "error:InvalidData"
    );
    assert_eq!(
        bed.get(&format!(
            "/echo?host=localhost&port={}&tls=localhost",
            bed.echo_untrusted
        )),
        "error:InvalidData"
    );
}

/// What Workers can't do is said, never tried: a Unix socket, and trusting
/// a connection's own authority, since workerd checks against its roots
/// only.
#[test]
fn what_workers_cant_do_is_refused() {
    let bed = bed();
    assert_eq!(bed.get("/unix"), "error:Unsupported");
    assert_eq!(
        bed.get(&format!("/authority?port={}", bed.echo_tls)),
        "error:InvalidInput"
    );
}

/// The package driver's `Fetch` sends a request with its method, headers
/// and body through `fetch()` and reads the whole answer.
#[test]
fn fetch_sends_a_request_and_reads_its_answer() {
    let bed = bed();
    assert_eq!(
        bed.get(&format!(
            "/fetch?url=http://127.0.0.1:{}/answer&timeout=5000",
            bed.http
        )),
        "status:200 x-test:yes body:{\"ok\":true}"
    );
    assert_eq!(
        bed.get(&format!(
            "/post?url=http://127.0.0.1:{}/echo&timeout=5000",
            bed.http
        )),
        "status:200 x-test:none body:POST application/json {\"sent\":1}"
    );
}

/// A redirect is an answer like any other: `redirect: "manual"`.
#[test]
fn fetch_follows_no_redirect() {
    let bed = bed();
    assert_eq!(
        bed.get(&format!(
            "/fetch?url=http://127.0.0.1:{}/redirect&timeout=5000",
            bed.http
        )),
        "status:302 x-test:none body:moved"
    );
}

/// A call past its timeout ends then, not when the server answers, and is
/// aborted through its `AbortSignal`, which lets its connection go while
/// the invocation carries on (the probe lingers two seconds after it).
#[test]
fn a_fetch_past_its_timeout_is_aborted() {
    let bed = bed();
    let began = Instant::now();
    assert_eq!(
        bed.get(&format!(
            "/fetch?url=http://127.0.0.1:{}/hold%3Ftag%3Dfetch&timeout=300&linger=2000",
            bed.http
        )),
        "error:TimedOut"
    );
    assert!(began.elapsed() < Duration::from_millis(4_000));
    let closed = closed("fetch").expect("the aborted call's connection was let go");
    assert!(closed < Duration::from_millis(1_500), "{closed:?}");
}

/// A streamed answer is read as it comes, each piece waited for at most
/// the call's timeout, and one stopped there lets its connection go while
/// the invocation carries on.
#[test]
fn a_streamed_answer_is_read_as_it_comes() {
    let bed = bed();
    assert_eq!(
        bed.get(&format!(
            "/open?url=http://127.0.0.1:{}/pieces&timeout=3000",
            bed.http
        )),
        "status:200 pieces:one|two|end"
    );
    let began = Instant::now();
    assert_eq!(
        bed.get(&format!(
            "/open?url=http://127.0.0.1:{}/hang%3Ftag%3Dopen&timeout=300&linger=2000",
            bed.http
        )),
        "status:200 pieces:first|timed_out"
    );
    assert!(began.elapsed() < Duration::from_millis(4_000));
    let closed = closed("open").expect("the stopped answer's connection was let go");
    assert!(closed < Duration::from_millis(1_500), "{closed:?}");
}

/// What the runtime fetches on its own account goes over HTTPS only, to
/// no address that isn't public, follows no redirect and reads no more
/// than asked for. The probe's guard lets a test's server stand in for a
/// public host.
#[test]
fn the_outbound_guard_holds() {
    let bed = bed();
    let https = bed.https;
    assert_eq!(
        bed.get(&format!(
            "/outbound?url=https://localhost:{https}/answer&limit=1000"
        )),
        "status:200 body:{\"ok\":true}"
    );
    assert_eq!(
        bed.get(&format!(
            "/outbound?url=https://localhost:{https}/redirect&limit=1000"
        )),
        "status:302 body:moved"
    );
    assert_eq!(
        bed.get(&format!(
            "/outbound?url=https://localhost:{https}/big&limit=1000"
        )),
        "error:an answer too long"
    );
    assert_eq!(
        bed.get(&format!(
            "/outbound?url=http://localhost:{}/answer&limit=1000",
            bed.http
        )),
        "error:not HTTPS"
    );
    assert_eq!(
        bed.get(&format!(
            "/outbound?url=https://127.0.0.1:{https}/answer&limit=1000"
        )),
        "error:`127.0.0.1` has no public address"
    );
}

/// Work a response leaves runs after it, through `ctx.waitUntil`: the
/// probe answers at once, and the work it deferred is seen done by a
/// later request.
#[test]
fn work_a_response_leaves_runs_after_it() {
    let bed = bed();
    assert_eq!(bed.get("/defer?key=after"), "deferred");
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let seen = bed.get("/deferred?key=after");
        if seen == "done" {
            break;
        }
        assert_eq!(seen, "not yet");
        assert!(Instant::now() < until, "the deferred work never ran");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Rate limits are counted in the isolate's memory, which lasts across
/// its requests.
#[test]
fn rate_limits_are_counted_in_the_isolates_memory() {
    let bed = bed();
    assert_eq!(bed.get("/rate?key=one"), "yes yes");
    assert_eq!(bed.get("/rate?key=one"), "no no");
    assert_eq!(bed.get("/rate?key=two"), "yes yes");
}

/// A connection's task, which a driver hands the transport, runs beside
/// the work that started it.
#[test]
fn the_transport_runs_a_connections_task() {
    assert_eq!(bed().get("/spawn"), "ran");
}

/// The driver factory is given Workers' transport and HTTP, and no disk.
#[test]
fn the_factory_is_supplied_for_workers() {
    assert_eq!(
        bed().get("/supplies"),
        "target:workers transport:yes fetch:yes disk:no"
    );
}
