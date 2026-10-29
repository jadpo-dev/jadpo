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

/// A setting whose binding is Hyperdrive's is the binding's address
/// (chunk 8.2i): its user, password and database, percent-encoded, at the
/// host workerd made for it, and no `sslmode`, which the connection
/// decides; that host is the platform's pooler, reached in a later request
/// of the isolate through the service the binding names, here the echo.
/// An address given as text is no pooler.
#[test]
fn a_hyperdrive_binding_is_the_settings_address_and_its_pooler() {
    let bed = bed();
    let read = bed.get("/variable?name=DATABASE");
    let address = read
        .strip_prefix("set:")
        .unwrap_or_else(|| panic!("{read}"));
    let host = address
        .strip_prefix("postgresql://kiosk:p%40ss%2Fword%21@")
        .and_then(|rest| rest.strip_suffix(":5432/repair%20cafe"))
        .unwrap_or_else(|| panic!("{address}"));
    assert!(
        host.len() == 32 + ".hyperdrive.local".len()
            && host.ends_with(".hyperdrive.local")
            && !address.contains("sslmode"),
        "{address}"
    );
    assert_eq!(bed.get(&format!("/pooler?host={host}&port=5432")), "pooler");
    assert_eq!(
        bed.get(&format!("/echo?host={host}&port=5432")),
        "answer:pong:ping"
    );
    assert_eq!(
        bed.get("/pooler?host=db.example.test&port=5432"),
        "not a pooler"
    );
    assert_eq!(
        bed.get(&format!("/pooler?host={host}&port=5433")),
        "not a pooler"
    );
}

/// PostgreSQL's driver reaches a server elsewhere over TLS through
/// `startTls()` after its SSL request, the certificate checked against
/// workerd's roots for the server's name; one from an authority nobody
/// vouches for fails the check as the startup's first bytes go out, which
/// is the fault naming the server it is natively, never a database down
/// for now (chunk 8.2i).
#[test]
fn postgresql_reaches_a_server_elsewhere_over_tls_and_a_failed_check_is_a_fault() {
    let bed = bed();
    assert_eq!(
        bed.get(&format!("/postgres?port={}", bed.postgres_tls)),
        "ran"
    );
    let refused = bed.get(&format!("/postgres?port={}", bed.postgres_untrusted));
    assert!(
        refused.starts_with(
            "fault:the database server at `db.example.test` failed the certificate check against the public authorities Jadpo trusts"
        ),
        "{refused}"
    );
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

/// The invocation taking on work handed on has what is left of its 30
/// seconds, counted from its start (chunk 8.4b): the time it spends before
/// the work runs, starting the isolate and reading the work back, is
/// spent from them.
#[test]
fn work_handed_on_has_what_is_left_of_its_invocations_seconds() {
    let seen = bed().get("/budget-handed-on?waited=1500");
    let left: Vec<u64> = seen
        .split(' ')
        .filter_map(|pair| pair.split_once('=')?.1.parse().ok())
        .collect();
    assert_eq!(left.len(), 2, "{seen}");
    for left in left {
        assert!((25_000..=28_500).contains(&left), "{seen}");
    }
}

/// A queue consumer's message whose `retry()` throws isn't acknowledged
/// (chunk 8.4b): the others are answered, and the handler then ends with
/// an error, so Cloudflare hands again every message not acknowledged
/// rather than acknowledging the unanswered one.
#[test]
fn a_message_whose_retry_throws_fails_the_batch() {
    assert_eq!(bed().get("/queue-answers"), "ack:m1 retry:m3:20 threw");
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

/// With Cloudflare's rate-limiting binding as the store, the binding
/// decides (chunk 8.2m2): the stand-in allows one request a minute where
/// the limit says two, and a refusal is told to wait the window's length,
/// since the binding says nothing of when its window ends. A binding that
/// throws, one the Worker hasn't got, or one that doesn't answer within
/// half a second, leaves the count to the isolate's memory, logged once,
/// and no request fails for it.
#[test]
fn rate_limits_are_counted_in_cloudflares_binding() {
    let bed = bed();
    assert_eq!(
        bed.get("/rate-binding?binding=LIMITS_2_PER_60S&key=bound"),
        "yes no:60"
    );
    assert_eq!(
        bed.get("/rate-binding?binding=LIMITS_2_PER_60S&key=another"),
        "yes no:60"
    );
    assert_eq!(
        bed.get("/rate-binding?binding=BROKEN_2_PER_60S&key=unreached"),
        "yes yes"
    );
    assert_eq!(
        bed.get("/rate-binding?binding=BROKEN_2_PER_60S&key=unreached"),
        "no:60 no:60"
    );
    assert_eq!(
        bed.get("/rate-binding?binding=MISSING_2_PER_60S&key=unbound"),
        "yes yes"
    );
    // A binding that never answers isn't waited for past half a second
    // (chunk 8.4b): the isolate counts instead.
    let began = Instant::now();
    assert_eq!(
        bed.get("/rate-binding?binding=STUCK_2_PER_60S&key=stuck"),
        "yes yes"
    );
    assert!(
        began.elapsed() < Duration::from_secs(5),
        "{:?}",
        began.elapsed()
    );
    // Each binding that couldn't be asked is logged once an isolate, by
    // name and why, never by key.
    assert_eq!(
        bed.get("/rate-binding?binding=MISSING_2_PER_60S&key=unbound-again"),
        "yes yes"
    );
    let log = bed.log();
    let fallbacks: Vec<serde_json::Value> = log
        .lines()
        .filter(|line| line.contains("\"rate_limits.fallback\""))
        .map(|line| serde_json::from_str(line).unwrap_or_else(|_| panic!("JSON: {line}")))
        .collect();
    let seen: Vec<(String, String)> = fallbacks
        .iter()
        .map(|event| {
            (
                event["binding"].as_str().unwrap_or_default().to_string(),
                event["reason"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    assert_eq!(
        seen,
        [
            ("BROKEN_2_PER_60S".to_string(), "failed".to_string()),
            ("MISSING_2_PER_60S".to_string(), "missing".to_string()),
            ("STUCK_2_PER_60S".to_string(), "timed_out".to_string()),
        ],
        "{log}"
    );
    for key in ["unreached", "unbound", "stuck"] {
        assert!(
            !fallbacks
                .iter()
                .any(|event| event.to_string().contains(key)),
            "{log}"
        );
    }
}

/// A connection's task, which a driver hands the transport, runs beside
/// the work that started it.
#[test]
fn the_transport_runs_a_connections_task() {
    assert_eq!(bed().get("/spawn"), "ran");
}

/// A connection's task an invocation's host started, still running when
/// the host is dropped, keeps the invocation, through its `waitUntil`,
/// until it ends (chunk 8.4e): the probe answers at once, and the task,
/// waiting on a timer, ends after the answer has gone, which it never does
/// once its invocation has ended.
#[test]
fn a_connections_task_runs_within_its_invocation() {
    let bed = bed();
    assert_eq!(bed.get("/spawn-within?key=within"), "spawned");
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let seen = bed.get("/deferred?key=within");
        if seen == "done" {
            break;
        }
        assert_eq!(seen, "not yet");
        assert!(Instant::now() < until, "the task never ended");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// An invocation's drivers are built when it first reaches a connection,
/// and once: one that reaches none, as a readiness check mostly doesn't,
/// builds none (chunk 8.4e).
#[test]
fn the_drivers_are_built_when_a_connection_is_first_reached() {
    assert_eq!(bed().get("/drivers"), "made:0 used:0 reached:1");
}

/// The driver factory is given Workers' transport and HTTP, and no disk.
#[test]
fn the_factory_is_supplied_for_workers() {
    assert_eq!(
        bed().get("/supplies"),
        "target:workers transport:yes fetch:yes disk:no"
    );
}

/// A passkey is registered and signed in with under workerd (area 8,
/// "Passkeys"): the runtime's ceremonies built for wasm32, their
/// challenges from Web Crypto, the test authenticator answering as the
/// device. What the Worker keeps is what the same ceremonies keep
/// natively of the same device's passkey; an answer from another site, and
/// a counter gone back, are refused.
#[test]
fn a_passkey_registers_and_signs_in_under_workerd() {
    use jadpo_runtime::Passkeys;
    use jadpo_runtime::test_authenticator::TestAuthenticator;
    use jadpo_runtime::webauthn::WebAuthn;
    use jadpo_value::encoding::{from_base64url, to_base64url};

    const ORIGIN: &str = "https://notes.example";
    let bed = bed();
    let pack = |text: &str| to_base64url(text.as_bytes());
    let unpack = |text: &str| String::from_utf8(from_base64url(text).unwrap()).unwrap();
    // `options:… state:…`, unpacked.
    let started = |answer: String| {
        let (options, state) = answer
            .strip_prefix("options:")
            .and_then(|rest| rest.split_once(" state:"))
            .unwrap_or_else(|| panic!("{answer}"));
        (unpack(options), state.to_string())
    };
    let kept = |answer: String| -> String {
        answer
            .strip_prefix("kept:")
            .unwrap_or_else(|| panic!("{answer}"))
            .to_string()
    };

    let mut device = TestAuthenticator::seeded(ORIGIN, 82);
    let (options, state) = started(bed.get(&format!("/passkey/register/options?origin={ORIGIN}")));
    let made = device.create(&options).unwrap();
    let stored = kept(bed.get(&format!(
        "/passkey/register?origin={ORIGIN}&state={state}&credential={}",
        pack(&made)
    )));

    // The same device registering natively keeps the same.
    let natively = WebAuthn::new(|bytes| {
        bytes.fill(9);
        true
    });
    let mut twin = TestAuthenticator::seeded(ORIGIN, 82);
    let (options, state) = natively
        .start_registration(ORIGIN, 300_000, [7; 16], "ada@example.com", &[])
        .unwrap();
    let native = natively
        .finish_registration(ORIGIN, 300_000, &state, &twin.create(&options).unwrap())
        .unwrap();
    let kept_json: serde_json::Value = serde_json::from_str(&unpack(&stored)).unwrap();
    assert_eq!(
        kept_json["credential_id"],
        to_base64url(&native.credential_id)
    );
    assert_eq!(kept_json["public_key"], to_base64url(&native.public_key));
    assert_eq!(kept_json["sign_count"], 0);

    let sign_in = |device: &mut TestAuthenticator, stored: &str| {
        let (options, state) =
            started(bed.get(&format!("/passkey/sign-in/options?origin={ORIGIN}")));
        let answer = device.get(&options).unwrap();
        bed.get(&format!(
            "/passkey/sign-in?origin={ORIGIN}&state={state}&credential={}&stored={stored}",
            pack(&answer)
        ))
    };
    let once = kept(sign_in(&mut device, &stored));
    let once_json: serde_json::Value = serde_json::from_str(&unpack(&once)).unwrap();
    assert_eq!(once_json["sign_count"], 1);
    let twice = kept(sign_in(&mut device, &once));
    let twice_json: serde_json::Value = serde_json::from_str(&unpack(&twice)).unwrap();
    assert_eq!(twice_json["sign_count"], 2);

    device.at("https://evil.example");
    assert_eq!(sign_in(&mut device, &twice), "error:Rejected");
    device.at(ORIGIN);
    device.set_sign_count(1);
    assert_eq!(sign_in(&mut device, &twice), "error:Rejected");
}
