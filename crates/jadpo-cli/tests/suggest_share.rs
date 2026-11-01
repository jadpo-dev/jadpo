//! `jadpo suggest share` (area 14, "Sharing with Jadpo HQ"; chunk 5.5y),
//! against a local fake of Jadpo HQ that speaks the protocol: the hashes go
//! at once, the snippets HQ asks for only with `--approve`, nothing at all
//! when sharing is turned off, and taught patterns reach a teammate through
//! the committed record.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

use crate::support;
use crate::support::fake_hq::FakeHq;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("share-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary folder");
    dir
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn read(root: &Path, path: &str) -> String {
    fs::read_to_string(root.join(path)).unwrap_or_default()
}

fn jadpo_with(cwd: &Path, args: &[&str], vars: &[(&str, &str)]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_cwd(cwd);
    for (key, value) in vars {
        env = env.with_var(key, value);
    }
    let mut output = run(&args, &env);
    output.stdout = support::without_run_time(&output.stdout);
    output
}

/// `jadpo` with HQ at the fake.
fn jadpo(cwd: &Path, hq: &FakeHq, args: &[&str]) -> Output {
    jadpo_with(cwd, args, &[("JADPO_SUGGEST_HQ", &hq.url)])
}

fn json(cwd: &Path, hq: &FakeHq, args: &[&str]) -> serde_json::Value {
    let mut args = args.to_vec();
    args.push("--json");
    let output = jadpo(cwd, hq, &args);
    serde_json::from_str(&output.stdout).expect("JSON")
}

const WHOLE: (&str, &str) = (
    "domains/shop/types/whole.jadpo",
    "/// A whole number.\ntype Whole = Int {}\n",
);

fn pence_of(body: &str) -> String {
    format!(
        "use shop/types {{ Whole }}\n\n/// Pence from a price in pence.\naction pence_of(total_price: Whole) -> Whole {{\n    return {body}\n}}\n"
    )
}

/// A shop whose action converts a price by hand, with that suggestion
/// ignored: one `ignored` entry waits to be shared.
fn ignored_shop(name: &str, hq: &FakeHq) -> PathBuf {
    let root = folder(name);
    write(&root, "app.jadpo", "application {}\n");
    write(&root, WHOLE.0, WHOLE.1);
    write(
        &root,
        "domains/shop/actions/pence_of.jadpo",
        &pence_of("Whole(total_price * 100)"),
    );
    jadpo(&root, hq, &["suggest"]);
    let ignored = jadpo(
        &root,
        hq,
        &["suggest", "ignore", "S2", "Basis points, not money"],
    );
    assert_eq!(ignored.code, 0, "{}", ignored.stderr);
    root
}

/// The text hash of the ignored snippet.
fn snippet_hash() -> String {
    jadpo_check::suggest::hash("total_price * 100")
}

/// Other projects' tokens, for seeding the fake.
const OTHERS: [&str; 2] = [
    "00000000000000000000000000000001",
    "00000000000000000000000000000002",
];

fn token_of(root: &Path) -> Option<String> {
    let app = read(root, "app.jadpo");
    let start = app.find("project_token: \"")? + "project_token: \"".len();
    Some(app[start..start + 32].to_string())
}

#[test]
fn the_whole_exchange_runs_against_the_fake() {
    let hq = FakeHq::start(2);
    hq.seed(
        &snippet_hash(),
        "ignored",
        "conversion",
        "Money",
        &OTHERS[..1],
    );
    let root = ignored_shop("exchange", &hq);

    let output = jadpo(&root, &hq, &["suggest", "share"]);
    assert_eq!(output.stderr, "");
    assert_eq!(output.code, 0);
    let url = &hq.url;
    assert_eq!(
        output.stdout,
        format!(
            "Sent 1 hash to Jadpo HQ
    kinds  1 ignored
    asked  1 snippet
    token  new, in app.jadpo: commit it so every copy counts as one project

What `jadpo suggest share --approve` sends
  POST {url}/v1/suggestions/snippets
  {{
    \"snippets\": [
      {{
        \"kind\": \"ignored\",
        \"reason\": \"Basis points, not money\",
        \"signal\": \"conversion\",
        \"suggestion\": \"Money\",
        \"text\": \"total_price * 100\"
      }}
    ]
  }}

Sharing is on
  Only hashes went. To stop sharing, add `share: false` to `suggestions {{ }}`.

decision: Jadpo HQ asks for 1 snippet of this project's code
  next: Ask the person whether to send it, showing them the payload above. Run `jadpo suggest share --approve` only after they say yes; otherwise run `jadpo suggest share --decline`. (human)
"
        )
    );

    // The token was written, and the project still checks and is formatted.
    let token = token_of(&root).expect("a token in app.jadpo");
    assert!(jadpo_check::suggest::settings::is_token(&token));
    let check = jadpo(&root, &hq, &["fmt", "--check"]);
    assert_eq!(check.code, 0, "{}{}", check.stdout, check.stderr);

    // Only the hash went, with the token.
    let requests = hq.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/v1/suggestions/hashes");
    assert_eq!(
        requests[0].json(),
        serde_json::json!({
            "token": token,
            "entries": [{
                "hash": snippet_hash(),
                "kind": "ignored",
                "signal": "conversion",
                "suggestion": "Money",
            }],
        })
    );
    assert!(!requests[0].body.contains("total_price"));
    assert!(hq.snippets().is_empty());
    // The pending file is wiped; what `--approve` sends waits.
    assert_eq!(read(&root, ".jadpo/suggest_pending"), "");

    // Approved, the snippet goes exactly as shown, with its reason and no
    // token.
    let approved = jadpo(&root, &hq, &["suggest", "share", "--approve"]);
    assert_eq!(approved.stderr, "");
    assert_eq!(approved.stdout, "Sent 1 snippet to Jadpo HQ\n");
    let requests = hq.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].path, "/v1/suggestions/snippets");
    assert!(
        output
            .stdout
            .contains(&requests[1].body.replace('\n', "\n  "))
    );
    assert!(!requests[1].body.contains(&token));
    assert_eq!(
        hq.snippets(),
        [serde_json::json!({
            "kind": "ignored",
            "reason": "Basis points, not money",
            "signal": "conversion",
            "suggestion": "Money",
            "text": "total_price * 100",
        })]
    );
    // Nothing waits any more.
    let again = jadpo(&root, &hq, &["suggest", "share", "--approve"]);
    assert!(
        again.stderr.contains("cli.suggest_arguments"),
        "{}",
        again.stderr
    );
    assert_eq!(hq.requests().len(), 2);

    // The headers carry no identifier.
    for request in hq.requests() {
        let names: Vec<&str> = request.headers.iter().map(|(n, _)| n.as_str()).collect();
        for name in &names {
            assert!(
                matches!(
                    *name,
                    "host"
                        | "content-type"
                        | "content-length"
                        | "accept"
                        | "user-agent"
                        | "connection"
                ),
                "{name}"
            );
        }
        assert!(
            request
                .headers
                .iter()
                .any(|(n, v)| n == "user-agent" && v == "jadpo")
        );
    }
}

#[test]
fn json_gives_an_approval_an_agent_can_relay() {
    let hq = FakeHq::start(1);
    let root = ignored_shop("json", &hq);
    let document = json(&root, &hq, &["suggest", "share"]);
    let data = &document["data"];
    assert_eq!(data["sent"], 1);
    assert_eq!(data["kinds"]["ignored"], 1);
    assert_eq!(data["asked"][0]["hash"], snippet_hash());
    let approval = &data["approval"];
    assert_eq!(
        approval["question"],
        "Jadpo HQ asks for 1 snippet of this project's code. Send it?"
    );
    assert_eq!(approval["approve"], "jadpo suggest share --approve");
    assert_eq!(approval["decline"], "jadpo suggest share --decline");
    assert_eq!(
        approval["payload"]["snippets"][0]["text"],
        "total_price * 100"
    );
    assert!(
        approval["rule"]
            .as_str()
            .unwrap_or_default()
            .contains("only after the person said yes"),
        "{approval}"
    );
}

#[test]
fn nothing_is_sent_when_sharing_is_off() {
    let hq = FakeHq::start(1);

    // In app.jadpo.
    let root = ignored_shop("off-app", &hq);
    write(
        &root,
        "app.jadpo",
        "application {}\n\nsuggestions {\n    share: false\n}\n",
    );
    let output = jadpo(&root, &hq, &["suggest", "share"]);
    assert_eq!(output.stderr, "");
    assert_eq!(output.code, 0);
    assert_eq!(
        output.stdout,
        "Sent nothing: sharing is turned off\n    where  app.jadpo:4  share: false\n"
    );
    assert!(hq.requests().is_empty());
    // Nothing is written: no token, and the entries stay.
    assert!(token_of(&root).is_none());
    assert!(!read(&root, ".jadpo/suggest_pending").is_empty());
    // `--new-token` stops too.
    jadpo(&root, &hq, &["suggest", "share", "--new-token"]);
    assert!(token_of(&root).is_none());
    assert!(hq.requests().is_empty());
    // The report drops its instruction to share.
    let report = jadpo(&root, &hq, &["suggest"]);
    assert!(
        !report.stdout.contains("jadpo suggest share"),
        "{}",
        report.stdout
    );

    // In the environment, for a whole machine.
    let root = ignored_shop("off-env", &hq);
    let output = jadpo_with(
        &root,
        &["suggest", "share"],
        &[
            ("JADPO_SUGGEST_HQ", &hq.url),
            ("JADPO_SUGGEST_SHARE", "off"),
        ],
    );
    assert_eq!(
        output.stdout,
        "Sent nothing: sharing is turned off\n    where  JADPO_SUGGEST_SHARE=off in the environment\n"
    );
    assert!(hq.requests().is_empty());
    let report = jadpo_with(&root, &["suggest"], &[("JADPO_SUGGEST_SHARE", "off")]);
    assert!(!report.stdout.contains("jadpo suggest share"));

    // Turned off after HQ asked: `--approve` sends nothing.
    let root = ignored_shop("off-after", &hq);
    let asked = jadpo(&root, &hq, &["suggest", "share"]);
    assert!(
        asked.stdout.contains("asked  1 snippet"),
        "{}",
        asked.stdout
    );
    let before = hq.requests().len();
    let output = jadpo_with(
        &root,
        &["suggest", "share", "--approve"],
        &[("JADPO_SUGGEST_HQ", &hq.url), ("JADPO_SUGGEST_SHARE", "no")],
    );
    assert!(
        output
            .stdout
            .starts_with("Sent nothing: sharing is turned off"),
        "{}",
        output.stdout
    );
    assert_eq!(hq.requests().len(), before);
    assert!(hq.snippets().is_empty());
}

#[test]
fn no_snippet_text_is_sent_without_approval() {
    let hq = FakeHq::start(1);
    let root = ignored_shop("approval", &hq);
    let output = jadpo(&root, &hq, &["suggest", "share"]);
    assert!(
        output.stdout.contains("asked  1 snippet"),
        "{}",
        output.stdout
    );
    for request in hq.requests() {
        assert!(!request.body.contains("total_price"), "{}", request.body);
        assert!(!request.body.contains("Basis points"), "{}", request.body);
    }

    // Declined, nothing goes and nothing waits.
    let declined = jadpo(&root, &hq, &["suggest", "share", "--decline"]);
    assert_eq!(declined.stderr, "");
    assert_eq!(
        declined.stdout,
        "Sent nothing: dropped the 1 snippet Jadpo HQ asked for\n"
    );
    let approve = jadpo(&root, &hq, &["suggest", "share", "--approve"]);
    assert!(approve.stderr.contains("cli.suggest_arguments"));
    assert_eq!(hq.requests().len(), 1);
    assert!(hq.snippets().is_empty());

    // Unattended, only hashes go and the request is skipped.
    for (name, args, vars) in [
        (
            "unattended-flag",
            vec!["suggest", "share", "--unattended"],
            vec![],
        ),
        (
            "unattended-ci",
            vec!["suggest", "share"],
            vec![("CI", "true")],
        ),
    ] {
        let root = ignored_shop(name, &hq);
        let before = hq.requests().len();
        let mut vars = vars;
        vars.push(("JADPO_SUGGEST_HQ", hq.url.as_str()));
        let output = jadpo_with(&root, &args, &vars);
        assert_eq!(output.code, 0, "{}", output.stderr);
        assert!(
            output
                .stdout
                .contains("asked  1 snippet, skipped: an unattended run sends only hashes"),
            "{}",
            output.stdout
        );
        assert!(!output.stdout.contains("--approve"), "{}", output.stdout);
        assert_eq!(hq.requests().len(), before + 1);
        let approve = jadpo(&root, &hq, &["suggest", "share", "--approve"]);
        assert!(approve.stderr.contains("cli.suggest_arguments"));
        assert_eq!(hq.requests().len(), before + 1);
        assert_eq!(read(&root, ".jadpo/suggest_pending"), "");
    }
    assert!(hq.snippets().is_empty());
}

#[test]
fn a_hash_below_the_threshold_is_never_asked_for() {
    let hq = FakeHq::start(3);
    let hash = snippet_hash();
    // Two projects send it: below the threshold, nobody is asked.
    for name in ["below-a", "below-b"] {
        let root = ignored_shop(name, &hq);
        let output = jadpo(&root, &hq, &["suggest", "share"]);
        assert!(
            output.stdout.contains("asked  nothing more"),
            "{}",
            output.stdout
        );
        assert!(!read(&root, ".jadpo/suggest_wanted").contains("total_price"));
    }
    // The same project again still counts once.
    let root = ignored_shop("below-a", &hq);
    write(
        &root,
        "app.jadpo",
        &format!(
            "application {{}}\n\nsuggestions {{\n    project_token: \"{}\"\n}}\n",
            hq.requests()[0].json()["token"]
                .as_str()
                .unwrap_or_default()
        ),
    );
    let output = jadpo(&root, &hq, &["suggest", "share"]);
    assert!(
        output.stdout.contains("asked  nothing more"),
        "{}",
        output.stdout
    );
    assert_eq!(hq.projects(&hash, "ignored"), 2);
    // A third project reaches the threshold and is asked.
    let root = ignored_shop("below-c", &hq);
    let output = jadpo(&root, &hq, &["suggest", "share"]);
    assert!(
        output.stdout.contains("asked  1 snippet"),
        "{}",
        output.stdout
    );
    assert_eq!(hq.projects(&hash, "ignored"), 3);
}

#[test]
fn the_token_is_written_when_missing_and_replaced_on_request() {
    let hq = FakeHq::start(5);
    let root = ignored_shop("token", &hq);
    write(
        &root,
        "app.jadpo",
        "application {}\n\nsuggestions {\n    share: true\n}\n",
    );
    jadpo(&root, &hq, &["suggest", "share"]);
    let first = token_of(&root).expect("written into the block");
    assert_eq!(read(&root, "app.jadpo").matches("suggestions").count(), 1);
    // `--new-token` replaces it, and sends with the new one.
    jadpo(&root, &hq, &["suggest", "ignore", "S1", "Not money"]);
    let output = jadpo(&root, &hq, &["suggest", "share", "--new-token"]);
    assert_eq!(output.code, 0, "{}", output.stderr);
    assert!(
        output.stdout.contains("    token  new, in app.jadpo"),
        "{}",
        output.stdout
    );
    let second = token_of(&root).expect("still there");
    assert_ne!(first, second);
    assert_eq!(
        hq.requests().last().map(|r| r.json()["token"].clone()),
        Some(serde_json::Value::from(second.clone()))
    );
    let check = jadpo(&root, &hq, &["fmt", "--check"]);
    assert_eq!(check.code, 0, "{}", check.stdout);

    // A hand-written token isn't one: nothing is sent.
    let before = hq.requests().len();
    write(
        &root,
        "app.jadpo",
        "application {}\n\nsuggestions {\n    project_token: \"acme-shop\"\n}\n",
    );
    let output = jadpo(&root, &hq, &["suggest", "share"]);
    assert_ne!(output.code, 0);
    assert!(
        output.stdout.contains("type.suggestions_value_invalid"),
        "{}",
        output.stdout
    );
    assert_eq!(hq.requests().len(), before);
}

#[test]
fn the_first_share_says_so_once() {
    let hq = FakeHq::start(5);
    let root = ignored_shop("first", &hq);
    let first = jadpo(&root, &hq, &["suggest", "share"]);
    assert!(first.stdout.contains("Sharing is on\n"), "{}", first.stdout);
    jadpo(&root, &hq, &["suggest", "ignore", "S1", "Not money"]);
    let second = jadpo(&root, &hq, &["suggest", "share"]);
    assert!(
        !second.stdout.contains("Sharing is on"),
        "{}",
        second.stdout
    );
    assert!(!second.stdout.contains("token"), "{}", second.stdout);
}

#[test]
fn with_nothing_pending_nothing_is_sent() {
    let hq = FakeHq::start(1);
    let root = folder("nothing");
    write(&root, "app.jadpo", "application {}\n");
    let output = jadpo(&root, &hq, &["suggest", "share"]);
    assert_eq!(output.code, 0, "{}", output.stderr);
    assert_eq!(output.stdout, "Nothing to share\n");
    assert!(hq.requests().is_empty());
}

#[test]
fn a_failed_send_is_retried_then_keeps_the_entries() {
    // HQ busy once, then answering: the retry goes through.
    let hq = FakeHq::start(5);
    hq.answer_next(
        503,
        &[("Retry-After", "0")],
        "{\"error\":{\"code\":\"unavailable\",\"message\":\"Busy\"}}",
    );
    let root = ignored_shop("retry", &hq);
    let output = jadpo(&root, &hq, &["suggest", "share"]);
    assert_eq!(output.code, 0, "{}", output.stderr);
    assert_eq!(hq.requests().len(), 2);

    // Refused: not retried, and the entries stay for the next run.
    let hq = FakeHq::start(5);
    hq.answer_next(
        410,
        &[],
        "{\"error\":{\"code\":\"version_retired\",\"message\":\"Update Jadpo to share\"}}",
    );
    let root = ignored_shop("refused", &hq);
    let output = jadpo(&root, &hq, &["suggest", "share"]);
    assert_ne!(output.code, 0);
    assert!(
        output.stderr.contains("suggest.share_failed"),
        "{}",
        output.stderr
    );
    assert!(
        output.stderr.contains("Update Jadpo to share"),
        "{}",
        output.stderr
    );
    assert_eq!(hq.requests().len(), 1);
    assert!(!read(&root, ".jadpo/suggest_pending").is_empty());

    // Told to wait longer than a minute: it stops rather than waits.
    let hq = FakeHq::start(5);
    hq.answer_next(
        429,
        &[("Retry-After", "120")],
        "{\"error\":{\"code\":\"rate_limited\",\"message\":\"Slow down\"}}",
    );
    let root = ignored_shop("rate-limited", &hq);
    let output = jadpo(&root, &hq, &["suggest", "share"]);
    assert!(output.stderr.contains("suggest.share_failed"));
    assert_eq!(hq.requests().len(), 1);

    // Nobody listening: the entries stay.
    let url = {
        let gone = FakeHq::start(1);
        gone.url.clone()
    };
    let root = ignored_shop("unreachable", &FakeHq::start(1));
    let output = jadpo_with(&root, &["suggest", "share"], &[("JADPO_SUGGEST_HQ", &url)]);
    assert!(
        output.stderr.contains("suggest.share_failed"),
        "{}",
        output.stderr
    );
    assert!(!read(&root, ".jadpo/suggest_pending").is_empty());
}

#[test]
fn plain_http_is_refused_except_on_this_machine() {
    let hq = FakeHq::start(1);
    let root = ignored_shop("plain-http", &hq);
    let output = jadpo_with(
        &root,
        &["suggest", "share"],
        &[("JADPO_SUGGEST_HQ", "http://hq.example.test")],
    );
    assert_ne!(output.code, 0);
    assert!(
        output.stderr.contains("suggest.share_failed"),
        "{}",
        output.stderr
    );
    assert!(output.stderr.contains("https://"), "{}", output.stderr);
    assert!(!read(&root, ".jadpo/suggest_pending").is_empty());
}

#[test]
fn a_teammates_checkout_picks_up_taught_patterns() {
    let hq = FakeHq::start(5);
    let total = "use shop/types { Whole }\n\n/// An order's total.\naction total_of(subtotal: Whole, delivery: Whole) -> Whole {\n    return Whole(subtotal + delivery)\n}\n";
    let charge = "use shop/types { Whole }\n\n/// What to charge.\naction charge_of(subtotal: Whole, delivery: Whole) -> Whole {\n    return Whole(delivery + subtotal)\n}\n";
    let mine = folder("taught-mine");
    write(&mine, "app.jadpo", "application {}\n");
    write(&mine, WHOLE.0, WHOLE.1);
    write(&mine, "domains/shop/actions/total_of.jadpo", total);
    let found = jadpo(
        &mine,
        &hq,
        &[
            "suggest",
            "found",
            "domains/shop/actions/total_of.jadpo:5",
            "Money",
            "Pence held as Int",
        ],
    );
    assert_eq!(found.code, 0, "{}", found.stderr);
    let record = read(&mine, "taught_suggestions.jadpo_record");
    assert!(
        record.starts_with("// Generated by Jadpo from the patterns this project taught `jadpo suggest`; update it with `jadpo suggest found`, never by hand.\nrecord taught_suggestions 2\n"),
        "{record}"
    );
    assert!(
        record.contains(" Money \"Pence held as Int\" \"(+ delivery subtotal)\"\n"),
        "{record}"
    );
    assert!(!record.contains("subtotal + delivery"), "{record}");

    // A teammate's checkout: the committed files, none of `.jadpo/`.
    let theirs = folder("taught-theirs");
    write(&theirs, "app.jadpo", "application {}\n");
    write(&theirs, WHOLE.0, WHOLE.1);
    write(&theirs, "domains/shop/actions/charge_of.jadpo", charge);
    write(&theirs, "taught_suggestions.jadpo_record", &record);
    let scan = json(&theirs, &hq, &["suggest"]);
    let findings = scan["data"]["findings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0]["signal"], "taught");
    assert_eq!(findings[0]["text"], "delivery + subtotal");

    // A pattern taught before the record existed moves into it.
    let old = folder("taught-old");
    write(&old, "app.jadpo", "application {}\n");
    write(&old, WHOLE.0, WHOLE.1);
    write(&old, "domains/shop/actions/charge_of.jadpo", charge);
    let canonical = "(+ delivery subtotal)";
    write(
        &old,
        ".jadpo/suggest_signals",
        &format!(
            "{}\n",
            serde_json::json!({
                "hash": jadpo_check::suggest::hash(canonical),
                "canonical": canonical,
                "suggestion": "Money",
                "reason": "Pence held as Int",
                "text": "subtotal + delivery",
            })
        ),
    );
    let scan = json(&old, &hq, &["suggest"]);
    assert_eq!(scan["data"]["findings"][0]["signal"], "taught");
    assert!(
        read(&old, "taught_suggestions.jadpo_record").contains("(+ delivery subtotal)"),
        "{}",
        read(&old, "taught_suggestions.jadpo_record")
    );

    // A hand edit is refused, by line.
    write(
        &theirs,
        "taught_suggestions.jadpo_record",
        &record.replace("taught ", "taugth "),
    );
    let output = jadpo(&theirs, &hq, &["suggest"]);
    assert!(
        output.stderr.contains("suggest.record_invalid"),
        "{}",
        output.stderr
    );
    assert!(
        output
            .stderr
            .contains("Restore `taught_suggestions.jadpo_record` from git"),
        "{}",
        output.stderr
    );
    // A merge's conflict markers are said to be them, and both sides kept.
    write(
        &theirs,
        "taught_suggestions.jadpo_record",
        &format!("{record}<<<<<<< HEAD\n=======\n>>>>>>> pricing\n"),
    );
    let output = jadpo(&theirs, &hq, &["suggest"]);
    assert!(
        output
            .stderr
            .contains("is a marker git leaves when two branches both changed the record"),
        "{}",
        output.stderr
    );
    assert!(
        output
            .stderr
            .contains("next: Keep the entries of both sides, delete the marker lines"),
        "{}",
        output.stderr
    );
    assert!(hq.requests().is_empty());
}

#[test]
fn an_unattended_run_cannot_approve_and_share_flags_need_share() {
    let hq = FakeHq::start(1);
    let root = ignored_shop("unattended-approve", &hq);
    jadpo(&root, &hq, &["suggest", "share"]);
    let output = jadpo_with(
        &root,
        &["suggest", "share", "--approve"],
        &[("JADPO_SUGGEST_HQ", &hq.url), ("CI", "1")],
    );
    assert!(
        output.stderr.contains("An unattended run can't approve"),
        "{}",
        output.stderr
    );
    assert_eq!(hq.requests().len(), 1);
    assert!(hq.snippets().is_empty());
    let output = jadpo(&root, &hq, &["suggest", "--approve"]);
    assert!(
        output.stderr.contains("goes with `jadpo suggest share`"),
        "{}",
        output.stderr
    );
    assert_eq!(hq.requests().len(), 1);
}

#[test]
fn without_an_hq_address_nothing_is_sent_and_nothing_fails() {
    let hq = FakeHq::start(1);
    let root = ignored_shop("unavailable", &hq);
    let output = jadpo_with(&root, &["suggest", "share"], &[]);
    assert_eq!(output.stderr, "");
    assert_eq!(output.code, 0);
    assert_eq!(
        output.stdout,
        "Sharing isn't available yet: Jadpo HQ isn't running. Nothing was sent; your entries are kept.\n"
    );
    assert!(!read(&root, ".jadpo/suggest_pending").is_empty());
    assert!(token_of(&root).is_none());
    assert!(hq.requests().is_empty());
    // The report asks for a share only once there is an HQ to share with.
    let report = jadpo_with(&root, &["suggest"], &[]);
    assert!(
        !report.stdout.contains("jadpo suggest share"),
        "{}",
        report.stdout
    );
    let report = jadpo(&root, &hq, &["suggest"]);
    assert!(
        report
            .stdout
            .contains("When done, run `jadpo suggest share`."),
        "{}",
        report.stdout
    );
}
