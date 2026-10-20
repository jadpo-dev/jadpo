//! A release that adds a standard type (area 19, question 4; chunk
//! 5.5a2): an application's declaration of the name is
//! `type.standard_clash`, naming the release, and the code `jadpo fix`
//! and the editor's quick fix share swaps it for the standard type or
//! renames it; `type.standard_exists`'s swap goes through the same code.
//! A fix never adds an error: one that would is undone.

use std::collections::BTreeMap;
use std::path::PathBuf;

use jadpo_check::fixes::{self, Fix, FixKind, Fixed};
use jadpo_check::setting_move::Mark;
use jadpo_db::{Database, Layout, Loaded};

use crate::support::{self, APP};

fn loaded(layout: Layout, files: &[(&str, &str)]) -> Loaded {
    let db = Database::default();
    let project = support::project(&db, layout, files);
    Loaded {
        db,
        project,
        root: PathBuf::from("/nowhere"),
        unreadable: Vec::new(),
        links: Vec::new(),
    }
}

fn texts(loaded: &Loaded) -> BTreeMap<String, String> {
    jadpo_db::source_texts(&loaded.db, loaded.project)
}

fn errors(loaded: &Loaded) -> Vec<String> {
    jadpo_check::check(&loaded.db, loaded.project)
        .iter()
        .map(|d| format!("{}: {}", d.rule.id, d.message))
        .collect()
}

/// Makes `fix` on the finding called `name`, which must succeed.
fn fixed(loaded: &mut Loaded, name: &str, fix: Fix) -> Fixed {
    let finding = fixes::find(&loaded.db, loaded.project, name).expect("a finding");
    let plan = fixes::plan(&loaded.db, loaded.project, &finding, &fix).expect("a plan");
    fixes::apply(loaded, &plan).expect("fixed")
}

/// The refusals of `fix` on `name`, as `rule: message`.
fn refused(loaded: &Loaded, name: &str, fix: Fix) -> Vec<String> {
    let finding = fixes::find(&loaded.db, loaded.project, name).expect("a finding");
    fixes::plan(&loaded.db, loaded.project, &finding, &fix)
        .expect_err("refused")
        .iter()
        .map(|d| format!("{}: {}", d.rule.id, d.message))
        .collect()
}

type Rows = Vec<(Mark, String, String)>;

fn rows(fixed: &Fixed) -> Vec<(String, Rows)> {
    fixed
        .files
        .iter()
        .map(|file| {
            (
                file.path.clone(),
                file.rows
                    .iter()
                    .map(|row| (row.mark, row.subject.clone(), row.note.clone()))
                    .collect(),
            )
        })
        .collect()
}

fn row(mark: Mark, subject: &str, note: &str) -> (Mark, String, String) {
    (mark, subject.to_string(), note.to_string())
}

/// A room booking app written before Jadpo had `Uuid`, with a slot that
/// is the standard `InstantRange` by shape.
const ROOMS: &[(&str, &str)] = &[
    APP,
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

#[test]
fn a_declaration_named_like_a_standard_type_is_one_error_naming_its_release() {
    let loaded = loaded(Layout::Project, ROOMS);
    assert_eq!(
        errors(&loaded),
        [
            "type.standard_exists: `Slot` is the standard `InstantRange` under another name",
            "type.standard_clash: `Uuid` is the name of a standard type",
        ]
    );
    let clash = jadpo_check::check(&loaded.db, loaded.project)
        .into_iter()
        .find(|d| d.rule.id == "type.standard_clash")
        .expect("the clash");
    let label = clash.primary.as_ref().expect("a label");
    assert_eq!(label.text, "standard since Jadpo 0.1.0");
    assert!(clash.reason.as_deref().unwrap_or("").starts_with(
        "Jadpo 0.1.0 added the standard type `Uuid`, and the application's own type can't share its name"
    ));
    assert_eq!(
        clash.next.map(|next| next.text).as_deref(),
        Some(
            "Run `jadpo fix Uuid swap` to use the standard `Uuid` in its place, or `jadpo fix Uuid rename <NewName>` to keep this one under a name of its own"
        )
    );
}

#[test]
fn the_findings_list_each_fix_they_take() {
    let loaded = loaded(Layout::Project, ROOMS);
    let found: Vec<(String, &str, &str, &str, Vec<FixKind>)> =
        fixes::findings(&loaded.db, loaded.project)
            .into_iter()
            .map(|f| (f.name, f.rule.id, f.standard, f.release, f.fixes))
            .collect();
    assert_eq!(
        found,
        [
            (
                "Slot".to_string(),
                "type.standard_exists",
                "InstantRange",
                "0.1.0",
                vec![FixKind::Swap]
            ),
            (
                "Uuid".to_string(),
                "type.standard_clash",
                "Uuid",
                "0.1.0",
                vec![FixKind::Swap, FixKind::Rename]
            ),
        ]
    );
}

#[test]
fn a_swap_uses_the_standard_type_and_its_field_names_everywhere() {
    let mut loaded = loaded(Layout::Project, ROOMS);
    let fixed = fixed(&mut loaded, "Slot", Fix::Swap);
    let texts = texts(&loaded);
    assert!(!texts.contains_key("domains/rooms/types/slot.jadpo"));
    assert_eq!(
        texts["domains/rooms/types/booking.jadpo"],
        "/// A room booked for a slot.\ntype Booking = Object {\n    reference: Uuid\n    slot: InstantRange\n}\n"
    );
    assert_eq!(
        texts["domains/rooms/actions/slot_of.jadpo"],
        "use rooms/types { Booking }\n\n/// The slot a booking takes.\naction slot_of(booking: Booking) -> InstantRange {\n    return InstantRange {\n        start: \"2026-10-26T09:00:00Z\"\n        end: \"2026-10-26T17:00:00Z\"\n    }\n}\n"
    );
    assert_eq!(
        texts["domains/rooms/actions/start_of.jadpo"],
        "use rooms/types { Booking, SlotStart }\n\n/// When a booking starts.\naction start_of(booking: Booking) -> SlotStart {\n    return SlotStart(booking.slot.start)\n}\n"
    );
    assert_eq!(
        rows(&fixed),
        [
            (
                "domains/rooms/types/slot.jadpo".to_string(),
                vec![
                    row(
                        Mark::Removed,
                        "Slot",
                        "type, the standard InstantRange replaces it"
                    ),
                    row(Mark::Removed, "slot.jadpo", "file, nothing left in it"),
                ]
            ),
            (
                "domains/rooms/actions/slot_of.jadpo".to_string(),
                vec![
                    row(Mark::Changed, "line 4", "InstantRange, was Slot"),
                    row(Mark::Changed, "line 5", "InstantRange, was Slot"),
                    row(Mark::Changed, "line 6", "start, was starts_at"),
                    row(Mark::Changed, "line 7", "end, was ends_at"),
                ]
            ),
            (
                "domains/rooms/actions/start_of.jadpo".to_string(),
                vec![row(Mark::Changed, "line 5", "start, was starts_at")]
            ),
            (
                "domains/rooms/types/booking.jadpo".to_string(),
                vec![row(Mark::Changed, "line 6", "InstantRange, was Slot")]
            ),
        ]
    );
    // The clash it didn't touch is the one error left.
    assert_eq!(fixed.errors_left, 1);
    assert_eq!(
        errors(&loaded),
        ["type.standard_clash: `Uuid` is the name of a standard type"]
    );
}

#[test]
fn a_clash_swapped_leaves_every_reference_meaning_the_standard_type() {
    let mut loaded = loaded(Layout::Project, ROOMS);
    let fixed = fixed(&mut loaded, "Uuid", Fix::Swap);
    let texts = texts(&loaded);
    assert!(!texts.contains_key("domains/rooms/types/uuid.jadpo"));
    assert!(texts["domains/rooms/types/booking.jadpo"].contains("    reference: Uuid\n"));
    assert_eq!(
        rows(&fixed),
        [(
            "domains/rooms/types/uuid.jadpo".to_string(),
            vec![
                row(Mark::Removed, "Uuid", "type, the standard Uuid replaces it"),
                row(Mark::Removed, "uuid.jadpo", "file, nothing left in it"),
            ]
        )]
    );
    assert_eq!(
        errors(&loaded),
        ["type.standard_exists: `Slot` is the standard `InstantRange` under another name"]
    );
}

#[test]
fn a_rename_writes_the_name_everywhere_and_moves_its_file() {
    let mut loaded = loaded(Layout::Project, ROOMS);
    let fixed = fixed(
        &mut loaded,
        "Uuid",
        Fix::Rename("RoomReference".to_string()),
    );
    let texts = texts(&loaded);
    assert!(!texts.contains_key("domains/rooms/types/uuid.jadpo"));
    assert_eq!(
        texts["domains/rooms/types/room_reference.jadpo"],
        "/// A room booking's reference.\ntype RoomReference = Text {\n    max_length: 36\n}\n"
    );
    // `Uuid` was the built-in one to the checker, so needed no import;
    // the application's own type does.
    assert_eq!(
        texts["domains/rooms/types/booking.jadpo"],
        "use rooms/types { RoomReference, Slot }\n\n/// A room booked for a slot.\ntype Booking = Object {\n    reference: RoomReference\n    slot: Slot\n}\n"
    );
    assert_eq!(
        rows(&fixed),
        [
            (
                "domains/rooms/types/uuid.jadpo".to_string(),
                vec![row(
                    Mark::Removed,
                    "uuid.jadpo",
                    "file, renamed room_reference.jadpo"
                )]
            ),
            (
                "domains/rooms/types/room_reference.jadpo".to_string(),
                vec![
                    row(Mark::Added, "room_reference.jadpo", "file, was uuid.jadpo"),
                    row(Mark::Changed, "line 2", "RoomReference, was Uuid"),
                ]
            ),
            (
                "domains/rooms/types/booking.jadpo".to_string(),
                vec![row(Mark::Changed, "line 5", "RoomReference, was Uuid")]
            ),
        ]
    );
    assert_eq!(
        errors(&loaded),
        ["type.standard_exists: `Slot` is the standard `InstantRange` under another name"]
    );
}

#[test]
fn both_fixes_leave_a_project_that_checks() {
    let mut loaded = loaded(Layout::Project, ROOMS);
    fixed(&mut loaded, "Slot", Fix::Swap);
    fixed(
        &mut loaded,
        "Uuid",
        Fix::Rename("RoomReference".to_string()),
    );
    assert_eq!(errors(&loaded), Vec::<String>::new());
    assert!(fixes::findings(&loaded.db, loaded.project).is_empty());
}

#[test]
fn a_swap_that_would_add_an_error_is_undone_with_it() {
    // The shop's own days, written before the standard `Weekday`: the
    // standard one has no `sat`.
    let files: &[(&str, &str)] = &[(
        "main.jadpo",
        "/// The days the shop opens.\ntype Weekday = Enum {\n    mon\n    sat\n}\n\n/// Whether the shop opens on a Saturday.\ntype OpensSaturday = Bool {}\n\n/// Whether `day` is a Saturday.\nfunction is_saturday(day: Weekday) -> OpensSaturday {\n    return OpensSaturday(day == Weekday.sat)\n}\n",
    )];
    let mut loaded = loaded(Layout::Loose, files);
    let before = texts(&loaded);
    assert_eq!(
        errors(&loaded),
        ["type.standard_clash: `Weekday` is the name of a standard type"]
    );
    let finding = fixes::find(&loaded.db, loaded.project, "Weekday").expect("found");
    let plan = fixes::plan(&loaded.db, loaded.project, &finding, &Fix::Swap).expect("planned");
    let stopped = fixes::apply(&mut loaded, &plan).expect_err("stopped");
    let added: Vec<String> = stopped
        .problems
        .iter()
        .map(|d| format!("{}: {}", d.rule.id, d.message))
        .collect();
    assert_eq!(
        added,
        ["type.unknown_enum_variant: `Weekday` has no variant `sat`"]
    );
    assert_eq!(texts(&loaded), before);
    // Renamed, it keeps its own variants.
    fixed(&mut loaded, "Weekday", Fix::Rename("ShopDay".to_string()));
    assert_eq!(errors(&loaded), Vec::<String>::new());
    assert!(texts(&loaded)["main.jadpo"].contains("return OpensSaturday(day == ShopDay.sat)"));
}

#[test]
fn a_rename_needs_a_free_type_name() {
    let loaded = loaded(Layout::Project, ROOMS);
    let rename = |name: &str| refused(&loaded, "Uuid", Fix::Rename(name.to_string()));
    assert_eq!(
        rename("roomReference"),
        ["cli.fix_arguments: `roomReference` isn't a name for a type"]
    );
    assert_eq!(
        rename("Booking"),
        ["cli.fix_arguments: `Booking` is already declared"]
    );
    assert_eq!(
        rename("Email"),
        ["cli.fix_arguments: `Email` is a built-in type"]
    );
    assert_eq!(
        rename("Uuid"),
        ["cli.fix_arguments: `Uuid` is the name it has"]
    );
    // A copy of a standard shape keeps the shape under any name.
    assert_eq!(
        refused(&loaded, "Slot", Fix::Rename("Window".to_string())),
        ["cli.fix_arguments: `Slot` can't be fixed with `rename`"]
    );
}

#[test]
fn only_a_type_can_be_swapped() {
    let files: &[(&str, &str)] = &[
        APP,
        (
            "domains/sites/entities/locale.jadpo",
            "/// A place the company works.\nentity Locale {\n    name: Text\n}\n",
        ),
    ];
    let loaded = loaded(Layout::Project, files);
    let finding = fixes::find(&loaded.db, loaded.project, "Locale").expect("found");
    assert_eq!(finding.fixes, [FixKind::Rename]);
    assert_eq!(
        refused(&loaded, "Locale", Fix::Swap),
        ["cli.fix_arguments: `Locale` can't be fixed with `swap`"]
    );
}

#[test]
fn a_name_with_nothing_to_fix_is_refused_with_the_nearest() {
    let loaded = loaded(Layout::Project, ROOMS);
    let problem = fixes::find(&loaded.db, loaded.project, "Uid").expect_err("refused");
    assert_eq!(problem.message, "`Uid` has nothing to fix");
    assert_eq!(
        problem.next.map(|next| next.text).as_deref(),
        Some("Use `jadpo fix Uuid`")
    );
}

/// A list of points is a `LineString` or a `Polygon` (area 20, chunk
/// 5.5u6); its swap makes it a `LineString`, its list read as `points`.
#[test]
fn a_list_of_points_swaps_to_a_line_string() {
    let mut loaded = loaded(
        Layout::Project,
        &[
            APP,
            (
                "domains/vans/types/van_round.jadpo",
                "/// A van's round, stop to stop.\ntype VanRound = Object {\n    coordinates: List<Point>\n}\n",
            ),
            (
                "domains/vans/types/round_length.jadpo",
                "/// How far a van drives.\ntype RoundLength = Length {}\n",
            ),
            (
                "domains/vans/actions/round_length_of.jadpo",
                "use vans/failures { NotARound }\nuse vans/types { RoundLength, VanRound }\n\n/// How far a van drives on its round.\naction round_length_of(round: VanRound) fails NotARound -> RoundLength {\n    var line: LineString = attempt LineString {\n        points: round.coordinates\n    } {\n        altitudes_agree: NotARound\n    }\n    return RoundLength(geo.length(line))\n}\n",
            ),
            (
                "domains/vans/failures/not_a_round.jadpo",
                "/// Stops some at a height and some not.\ntype NotARound = Failure {\n    kind: FailureKind.invalid_value\n}\n",
            ),
        ],
    );
    assert_eq!(
        errors(&loaded),
        [
            "type.standard_exists: `VanRound` is the standard `LineString` or `Polygon` under another name"
        ]
    );
    fixed(&mut loaded, "VanRound", Fix::Swap);
    assert_eq!(
        texts(&loaded)["domains/vans/actions/round_length_of.jadpo"],
        "use vans/failures { NotARound }\nuse vans/types { RoundLength }\n\n/// How far a van drives on its round.\naction round_length_of(round: LineString) fails NotARound -> RoundLength {\n    var line: LineString = attempt LineString {\n        points: round.points\n    } {\n        altitudes_agree: NotARound\n    }\n    return RoundLength(geo.length(line))\n}\n"
    );
    assert_eq!(errors(&loaded), Vec::<String>::new());
}
