//! `jadpo explain` (area 14, "`jadpo explain`"): a digest of the app read
//! from its checked graph, and `jadpo help topic <id>`, which prints the
//! topics it names offline. The helpdesk benchmark's digest is held to a
//! golden copy (`JADPO_BLESS=1` rewrites it).

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

use crate::support;

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut output = run(&args, &Environment::empty().with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A copy of `from`, without what isn't the Jadpo app (`skip`), so a run
/// writes nothing into the repository.
fn copy(from: &Path, name: &str, skip: &[&str]) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("explain-{name}"));
    let _ = fs::remove_dir_all(&root);
    copy_into(from, &root, skip);
    root
}

fn copy_into(from: &Path, to: &Path, skip: &[&str]) {
    fs::create_dir_all(to).expect("a folder");
    for entry in fs::read_dir(from).expect("a folder").flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if skip.contains(&name.as_ref()) || name == "build" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            copy_into(&path, &to.join(name.as_ref()), &[]);
        } else {
            fs::copy(&path, to.join(name.as_ref())).expect("a copy");
        }
    }
}

#[test]
fn the_helpdesk_digest_matches_its_golden_copy_and_never_changes() {
    let root = copy(
        &repository().join("benchmarks/helpdesk"),
        "helpdesk",
        &["typescript", "README.md"],
    );
    let first = jadpo(&root, &["explain"]);
    assert_eq!(first.code, 0, "{}{}", first.stdout, first.stderr);
    let again = jadpo(&root, &["explain"]);
    assert_eq!(
        first.stdout, again.stdout,
        "the digest is the same each run"
    );
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/goldens/helpdesk.explain.txt");
    if std::env::var_os("JADPO_BLESS").is_some() {
        fs::write(&path, &first.stdout).expect("bless");
        return;
    }
    let saved = fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        saved, first.stdout,
        "the helpdesk's digest changed; JADPO_BLESS=1 cargo test -p jadpo-cli --test integration explain:: rewrites it"
    );
    // About 1,000 to 3,000 tokens, at four characters a token.
    let tokens = first.stdout.len() / 4;
    assert!(
        (1_000..=3_000).contains(&tokens),
        "the helpdesk's digest is about {tokens} tokens"
    );
}

#[test]
fn the_digest_as_json_holds_each_part() {
    let root = copy(
        &repository().join("benchmarks/helpdesk"),
        "helpdesk-json",
        &["typescript", "README.md"],
    );
    let output = jadpo(&root, &["explain", "--json"]);
    assert_eq!(output.code, 0, "{}", output.stdout);
    let document: serde_json::Value =
        serde_json::from_str(&output.stdout).expect("one JSON document");
    let data = &document["data"];
    let ticket = data["entities"]
        .as_array()
        .expect("entities")
        .iter()
        .find(|entity| entity["name"] == "Ticket")
        .expect("Ticket");
    assert_eq!(ticket["at"], "domains/desk/entities/ticket.jadpo:5");
    assert!(
        ticket["rows"]
            .as_array()
            .expect("rows")
            .iter()
            .any(|row| row["label"] == "delete"
                && row["value"] == "WorkspaceRole.owner, WorkspaceRole.admin")
    );
    assert!(
        data["routes"]
            .as_array()
            .expect("routes")
            .iter()
            .any(|route| route
                == "POST /tickets/{ticket_id}/assign Assignment -> TicketView  runs assign_ticket")
    );
    assert!(
        data["docs"]
            .as_array()
            .expect("docs")
            .iter()
            .any(|doc| doc["construct"] == "tenancy" && doc["topic"] == "tenancy")
    );
}

#[test]
fn background_work_shows_events_and_their_subscribers_and_each_batch() {
    let cafe = copy(
        &repository().join("examples/23-repair-cafe"),
        "repair-cafe",
        &[],
    );
    let output = jadpo(&cafe, &["explain"]);
    assert_eq!(output.code, 0, "{}", output.stdout);
    assert!(
        output.stdout.contains(
            "  event: RepairBooked from book_repair; subscriber find_a_bench runs send_to_bench\n"
        ),
        "{}",
        output.stdout
    );
    assert!(output.stdout.contains("  jadpo help topic subscribers\n"));
    let library = copy(
        &repository().join("examples/24-seed-library"),
        "seed-library",
        &[],
    );
    let output = jadpo(&library, &["explain"]);
    assert_eq!(output.code, 0, "{}", output.stdout);
    assert!(
        output.stdout.contains(
            "  batch: start_check over shelf_packets, each check_packet, finished record_check\n"
        ),
        "{}",
        output.stdout
    );
    assert!(
        output
            .stdout
            .contains("  jadpo help topic batches-work-over-many-items\n")
    );
}

#[test]
fn the_acceptances_already_given_are_listed_with_their_reason() {
    let desk = copy(
        &repository().join("examples/29-lending-desk"),
        "lending-desk",
        &[],
    );
    let output = jadpo(&desk, &["explain"]);
    assert_eq!(output.code, 0, "{}", output.stdout);
    let block = output
        .stdout
        .split("\n\n")
        .find(|block| block.starts_with("Acceptances\n"))
        .unwrap_or_else(|| panic!("an Acceptances block: {}", output.stdout));
    let accepted = fs::read_dir(desk.join("acceptances"))
        .expect("its acceptances")
        .count();
    assert_eq!(block.lines().count(), 1 + accepted, "{block}");
    assert!(output.stdout.contains("  jadpo help topic acceptances\n"));
}

#[test]
fn it_takes_no_argument_and_refuses_a_project_with_errors() {
    let root = copy(
        &repository().join("benchmarks/helpdesk"),
        "helpdesk-refused",
        &["typescript", "README.md"],
    );
    let output = jadpo(&root, &["explain", "Ticket"]);
    assert_eq!(output.code, 2, "{}", output.stderr);
    assert!(
        output.stderr.contains("rule: cli.explain_arguments"),
        "{}",
        output.stderr
    );
    fs::write(
        root.join("domains/desk/entities/ticket.jadpo"),
        fs::read_to_string(root.join("domains/desk/entities/ticket.jadpo"))
            .expect("the ticket")
            .replace("subject: TicketSubject", "subject: TicketSubjekt"),
    )
    .expect("an error");
    let output = jadpo(&root, &["explain"]);
    assert_eq!(output.code, 1, "{}", output.stdout);
    assert!(
        output.stdout.contains("explain stopped: "),
        "{}",
        output.stdout
    );
    assert!(!output.stdout.contains("Access\n"), "{}", output.stdout);
}

#[test]
fn help_topic_prints_a_topic_of_the_area_pages_offline() {
    let cwd = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let output = jadpo(cwd, &["help", "topic", "lifecycle"]);
    assert_eq!(output.code, 0, "{}", output.stdout);
    assert!(
        output.stdout.starts_with("Lifecycle, area 5, lifecycle\n"),
        "{}",
        output.stdout
    );
    assert!(!output.stdout.contains("<!-- topic"), "{}", output.stdout);
    // A topic a command shares its name with is read through `topic`;
    // without it, the command's help is shown, as before.
    let schedules = jadpo(cwd, &["help", "topic", "schedules"]);
    assert!(
        schedules
            .stdout
            .starts_with("Schedules, area 12, schedules\n"),
        "{}",
        schedules.stdout
    );
    let command = jadpo(cwd, &["help", "schedules"]);
    assert!(
        command.stdout.starts_with("jadpo schedules"),
        "{}",
        command.stdout
    );
    // A topic no command shares its name with is read without it too.
    let tenancy = jadpo(cwd, &["help", "tenancy"]);
    assert!(
        tenancy.stdout.starts_with("Tenancy, area 9, tenancy\n"),
        "{}",
        tenancy.stdout
    );
    // An ID a topic had before a rename still reaches it.
    let renamed = jadpo(cwd, &["help", "topic", "background-work"]);
    assert!(
        renamed.stdout.contains("policy-and-background-work"),
        "{}",
        renamed.stdout
    );
}

#[test]
fn help_topic_refuses_an_id_no_topic_has_with_the_nearest() {
    let cwd = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let output = jadpo(cwd, &["help", "topic", "lifecycles"]);
    assert_eq!(output.code, 2, "{}", output.stderr);
    assert_eq!(
        output.stderr,
        "error: `lifecycles` isn't a docs topic
  reason: `jadpo help topic <id>` prints a topic of the language pages by its ID, as `jadpo explain` lists them and a topic's page declares it. Did you mean `lifecycle`?
  next: Run `jadpo help topic lifecycle` (agent)
  rule: cli.help_arguments
  help: https://jadpo.dev/docs/errors/cli.help_arguments
"
    );
}
