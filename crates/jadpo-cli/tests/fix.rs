//! `jadpo fix` (area 14; area 19, "A release adding a standard type"; chunk 5.5a2): the findings
//! a release's new standard type gives, listed with the fixes each takes,
//! and the swap or the rename the author picks, made through the code the
//! editor's quick fix runs, as text and `--json`. A fix that would add an
//! error changes nothing.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

use crate::support;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("fix-{name}"));
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
    fs::read_to_string(root.join(path)).expect("read")
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut output = run(&args, &Environment::empty().with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

/// A room booking app written before Jadpo had `Uuid`, with a slot that
/// is the standard `InstantRange` by shape.
const ROOMS: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "domains/rooms/types/uuid.jadpo",
        "/// A room booking's reference.\ntype Uuid = Text {\n    max_length: 36\n}\n",
    ),
    (
        "domains/rooms/types/slot.jadpo",
        "/// When a room is booked, from its start to its end.\ntype Slot = Object {\n    starts_at: Instant\n    ends_at: Instant\n}\n",
    ),
    (
        "domains/rooms/types/booking.jadpo",
        "use rooms/types { Slot }\n\n/// A room booked for a slot.\ntype Booking = Object {\n    reference: Uuid\n    slot: Slot\n}\n",
    ),
    (
        "domains/rooms/types/slot_start.jadpo",
        "/// When a slot starts.\ntype SlotStart = Instant {}\n",
    ),
    (
        "domains/rooms/actions/start_of.jadpo",
        "use rooms/types { Booking, SlotStart }\n\n/// When a booking starts.\naction start_of(booking: Booking) -> SlotStart {\n    return SlotStart(booking.slot.starts_at)\n}\n",
    ),
    (
        "domains/rooms/actions/slot_of.jadpo",
        "use rooms/types { Booking, Slot }\n\n/// The slot a booking takes.\naction slot_of(booking: Booking) -> Slot {\n    return Slot {\n        starts_at: \"2026-10-26T09:00:00Z\"\n        ends_at: \"2026-10-26T17:00:00Z\"\n    }\n}\n",
    ),
];

fn rooms(name: &str) -> PathBuf {
    let root = folder(name);
    for (path, text) in ROOMS {
        write(&root, path, text);
    }
    root
}

#[test]
fn alone_it_lists_each_finding_and_the_fixes_it_takes() {
    let root = rooms("list");
    let output = jadpo(&root, &["fix"]);
    assert_eq!(output.stderr, "");
    assert_eq!(
        output.stdout,
        "Fixes
  Slot  domains/rooms/types/slot.jadpo:2  swap          type.standard_exists
  Uuid  domains/rooms/types/uuid.jadpo:2  swap, rename  type.standard_clash

2 findings to fix: 2 files

next: jadpo fix Slot swap  uses the standard InstantRange in its place
"
    );
    assert_eq!(output.code, 0);
    let json = jadpo(&root, &["fix", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&json.stdout).expect("JSON");
    assert_eq!(
        document["data"]["findings"][1],
        serde_json::json!({
            "name": "Uuid",
            "rule": "type.standard_clash",
            "path": "domains/rooms/types/uuid.jadpo",
            "line": 2,
            "standard": "Uuid",
            "release": "0.1.0",
            "fixes": ["swap", "rename"],
        })
    );
}

#[test]
fn a_swap_lists_what_changed_and_writes_it() {
    let root = rooms("swap");
    let output = jadpo(&root, &["fix", "Slot", "swap"]);
    assert_eq!(output.stderr, "");
    assert_eq!(
        output.stdout,
        "  domains/rooms/types/slot.jadpo
    - Slot        type, the standard InstantRange replaces it
    - slot.jadpo  file, nothing left in it

  domains/rooms/actions/slot_of.jadpo
    ~ line 4      InstantRange, was Slot
    ~ line 5      InstantRange, was Slot
    ~ line 6      start, was starts_at
    ~ line 7      end, was ends_at

  domains/rooms/actions/start_of.jadpo
    ~ line 5      start, was starts_at

  domains/rooms/types/booking.jadpo
    ~ line 6      InstantRange, was Slot

Swapped Slot for the standard InstantRange

next: jadpo check  shows the error the project still has
"
    );
    assert_eq!(output.code, 0);
    assert!(!root.join("domains/rooms/types/slot.jadpo").exists());
    assert_eq!(
        read(&root, "domains/rooms/actions/slot_of.jadpo"),
        "use rooms/types { Booking }\n\n/// The slot a booking takes.\naction slot_of(booking: Booking) -> InstantRange {\n    return InstantRange {\n        start: \"2026-10-26T09:00:00Z\"\n        end: \"2026-10-26T17:00:00Z\"\n    }\n}\n"
    );
    assert_eq!(jadpo(&root, &["fmt", "--check"]).code, 0);
}

#[test]
fn a_rename_moves_the_file_and_then_the_project_checks() {
    let root = rooms("rename");
    assert_eq!(jadpo(&root, &["fix", "Slot", "swap"]).code, 0);
    let output = jadpo(&root, &["fix", "Uuid", "rename", "RoomReference", "--json"]);
    assert_eq!(output.code, 0);
    let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
    let data = &document["data"];
    assert_eq!(data["fixed"], true);
    assert_eq!(data["fix"], "rename");
    assert_eq!(data["renamed"], "RoomReference");
    assert_eq!(data["errorsLeft"], 0);
    let files: Vec<(&str, &str)> = data["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|f| {
            (
                f["path"].as_str().unwrap_or_default(),
                f["change"].as_str().unwrap_or_default(),
            )
        })
        .collect();
    assert_eq!(
        files,
        [
            ("domains/rooms/types/uuid.jadpo", "removed"),
            ("domains/rooms/types/room_reference.jadpo", "added"),
            ("domains/rooms/types/booking.jadpo", "changed"),
        ]
    );
    assert!(!root.join("domains/rooms/types/uuid.jadpo").exists());
    assert_eq!(
        read(&root, "domains/rooms/types/room_reference.jadpo"),
        "/// A room booking's reference.\ntype RoomReference = Text {\n    max_length: 36\n}\n"
    );
    assert_eq!(jadpo(&root, &["check"]).code, 0);
    let nothing = jadpo(&root, &["fix"]);
    assert_eq!(nothing.stdout, "Nothing to fix\n");
    assert_eq!(nothing.code, 0);
}

#[test]
fn a_fix_that_would_add_an_error_changes_nothing() {
    let root = folder("stopped");
    write(&root, "app.jadpo", "application {}\n");
    let days = "/// The days the shop opens.\ntype Weekday = Enum {\n    mon\n    sat\n}\n";
    write(&root, "domains/shop/types/weekday.jadpo", days);
    write(
        &root,
        "domains/shop/types/opens_saturday.jadpo",
        "/// Whether the shop opens on a Saturday.\ntype OpensSaturday = Bool {}\n",
    );
    write(
        &root,
        "domains/shop/actions/is_saturday.jadpo",
        "use shop/types { OpensSaturday, Weekday }\n\n/// Whether `day` is a Saturday.\naction is_saturday(day: Weekday) -> OpensSaturday {\n    return OpensSaturday(day == Weekday.sat)\n}\n",
    );
    let output = jadpo(&root, &["fix", "Weekday", "swap"]);
    assert_eq!(
        output.stdout,
        "error: `Weekday` has no variant `sat`
 --> domains/shop/actions/is_saturday.jadpo:5:41
  |
5 |     return OpensSaturday(day == Weekday.sat)
  |                                         ^^^ not a variant
  reason: `Weekday`'s variants are `monday`, `tuesday`, `wednesday`, `thursday`, `friday`, `saturday` and `sunday`. Did you mean `saturday`?
  next: Use `Weekday.saturday` (agent)
  fix:
    5 - return OpensSaturday(day == Weekday.sat)
    5 + return OpensSaturday(day == Weekday.saturday)
  rule: type.unknown_enum_variant
  help: https://jadpo.dev/docs/errors/type.unknown_enum_variant

fix stopped: 1 error after the fix, nothing changed
"
    );
    assert_eq!(output.code, 1);
    assert_eq!(read(&root, "domains/shop/types/weekday.jadpo"), days);
}

#[test]
fn bad_arguments_are_cli_errors() {
    let root = rooms("arguments");
    let cases: &[(&[&str], &str, &str)] = &[
        (
            &["fix", "Uid", "swap"],
            "error: `Uid` has nothing to fix",
            "next: Use `jadpo fix Uuid` (agent)",
        ),
        (
            &["fix", "Uuid"],
            "error: `jadpo fix Uuid` needs a fix",
            "next: Run `jadpo fix Uuid swap` or `jadpo fix Uuid rename <NewName>` (agent)",
        ),
        (
            &["fix", "Uuid", "swop"],
            "error: `swop` isn't a fix",
            "next: Use `swap` instead of `swop` (agent)",
        ),
        (
            &["fix", "Uuid", "rename"],
            "error: `jadpo fix Uuid rename` needs the new name",
            "next: Run `jadpo fix Uuid rename <NewName>` (agent)",
        ),
        (
            &["fix", "Uuid", "rename", "Booking"],
            "error: `Booking` is already declared",
            "reason: `Booking` is declared in `domains/rooms/types/booking.jadpo`",
        ),
        (
            &["fix", "Slot", "rename", "Window"],
            "error: `Slot` can't be fixed with `rename`",
            "next: Run `jadpo fix Slot swap` (agent)",
        ),
    ];
    for (args, headline, line) in cases {
        let output = jadpo(&root, args);
        assert_eq!(output.code, 2, "{args:?}: {}", output.stderr);
        assert!(output.stderr.starts_with(headline), "{}", output.stderr);
        assert!(output.stderr.contains(line), "{}", output.stderr);
        assert!(output.stderr.contains("rule: cli.fix_arguments"));
    }
    assert!(root.join("domains/rooms/types/uuid.jadpo").exists());
}

#[test]
fn outside_a_project_it_says_so() {
    let root = folder("outside");
    let output = jadpo(&root, &["fix"]);
    assert_eq!(output.code, 1);
    assert!(
        output
            .stdout
            .contains("next: Run `jadpo fix` inside a project (agent)"),
        "{}",
        output.stdout
    );
}

/// A project whose errors each carry their own repair: a call that can
/// fail with nothing handling it, and an object built without a field
/// that may be absent.
const COUNTS: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "domains/notes/failures/too_big.jadpo",
        "/// A count over the limit.\ntype TooBig = Failure {\n    kind: FailureKind.invalid_value\n}\n",
    ),
    (
        "domains/notes/types/label.jadpo",
        "/// A label.\ntype Label = Object {\n    text: Text\n    note: Text?\n}\n",
    ),
    (
        "domains/notes/types/count.jadpo",
        "/// How many.\ntype Count = Int {}\n",
    ),
    (
        "domains/notes/actions/doubled.jadpo",
        "use notes/failures { TooBig }\nuse notes/types { Count, Label }\n\n/// A count, checked.\nfunction checked(count: Count) fails TooBig -> Count {\n    if (count > 500) {\n        reject TooBig\n    }\n    return count\n}\n\n/// The count, checked and labelled.\naction doubled(count: Count) -> Label {\n    var once: Count = checked(count)\n    return Label {\n        text: text.from(once)\n    }\n}\n",
    ),
];

fn counts(name: &str) -> PathBuf {
    let root = folder(name);
    for (path, text) in COUNTS {
        write(&root, path, text);
    }
    root
}

/// Alone, `jadpo fix` applies every edit an agent's errors carry, round by
/// round, since one fix can bring the next to light (`attempt`, then the
/// `fails` list it needs), then says what it changed and what is left
/// (chunk 10.9a).
#[test]
fn alone_it_applies_every_automatic_edit_and_checks_again() {
    let root = counts("all");
    let output = jadpo(&root, &["fix"]);
    assert_eq!(output.stderr, "");
    assert_eq!(
        output.stdout,
        "  domains/notes/actions/doubled.jadpo
    + line 13  Add `TooBig` to `doubled`'s `fails` list
    + line 14  Start the statement with `attempt`
    + line 17  Add `note: none` to the block

Fixed 3 errors in 1 file
"
    );
    assert_eq!(output.code, 0);
    let source = read(&root, "domains/notes/actions/doubled.jadpo");
    assert!(
        source.contains("action doubled(count: Count) fails TooBig -> Label {"),
        "{source}"
    );
    assert!(
        source.contains("    var once: Count = attempt checked(count)\n"),
        "{source}"
    );
    assert!(
        source.contains("        text: text.from(once)\n        note: none\n    }"),
        "{source}"
    );
    assert_eq!(jadpo(&root, &["check"]).code, 0);
    // Nothing left to apply: the findings list, empty.
    assert_eq!(jadpo(&root, &["fix"]).stdout, "Nothing to fix\n");
}
