//! Every standard type through storage, against live PostgreSQL (area 19,
//! "Every standard type meets one contract"; chunk 5.5w): a project with an
//! entity holding each standard object and choice type stored in named
//! columns, a `ZonedTime`, a range of each item, a quantity, a fixed
//! currency and a fixed rate, made from each type's data entry, so a type
//! added later is tested without a line here. `jadpo test` stores each of
//! the entry's valid examples and reads it back equal, and the initial
//! migration lays each column out as the entry says: a quantity's column
//! with a comment naming its unit, a fixed currency and a fixed rate held
//! by check constraints, a standard enum stored by its code. The JSON-held
//! values (`RichText`, `RichTextBlock`, `LocalisedText`) are stored as
//! their contract's JSON in one `jsonb` column (chunk 5.5w2b). Each
//! geometry is stored in a PostGIS `geography` column (chunk 5.5u4), so
//! the server needs PostGIS.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips the
//! live part.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_std::standard::{Holds, STANDARD_TYPES, StandardType, Storage};
use jadpo_term::Environment;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    run(&args, &env.with_cwd(cwd))
}

/// `text` as a Jadpo string literal.
fn literal(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `PascalCase` as `snake_case`.
fn snake(name: &str) -> String {
    let mut out = String::new();
    for (at, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if at > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else if c.is_ascii_alphanumeric() {
            out.push(c);
        }
    }
    out
}

/// One stored type: the type as a field names it, a short name for its
/// entity, how a value is read from its text, and its values' texts.
struct Case {
    ty: String,
    name: String,
    reading: String,
    texts: Vec<String>,
}

/// Whether the entry's type is stored in named columns or natively, as
/// this chunk stores it: every standard object and choice but a choice of
/// lists, a `ZonedTime`, and the ranges; and each geometry, in its
/// PostGIS `geography` (area 20, chunk 5.5u4).
fn stored_in_columns(entry: &StandardType) -> bool {
    (matches!(entry.storage, Storage::Columns { .. })
        && !matches!(entry.holds, Holds::Builtin(jadpo_std::Builtin::Money)))
        || entry.storage == Storage::Geography
}

/// Whether the entry's type is stored as its contract's JSON in one
/// `jsonb` column (chunk 5.5w2b).
fn stored_as_json(entry: &StandardType) -> bool {
    entry.storage == Storage::Column(jadpo_std::read::SqlType::Json)
}

/// How a value of the entry's type is read from `raw`: from its
/// canonical text, or from its JSON for a type with no single text form.
fn reading(entry: &StandardType, ty: &str) -> String {
    match entry.text {
        Some(_) => format!(
            "attempt text.parse(raw, as: {ty}) {{\n            invalid: BadExample\n        }}"
        ),
        None => format!(
            "attempt json.read(raw, as: {ty}) {{\n            malformed: BadExample\n            mismatch: BadExample\n        }}"
        ),
    }
}

/// Whether a GeoJSON geometry's positions mix with and without an
/// altitude, which PostGIS's `geography` can't hold, so storing one is a
/// fault (area 20, "Chosen in chunk 5.5u4").
fn mixes_altitudes(text: &str) -> bool {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
        return false;
    };
    fn lengths(json: &serde_json::Value, out: &mut Vec<usize>) {
        if let Some(items) = json.as_array() {
            if items.iter().all(serde_json::Value::is_number) {
                out.push(items.len());
            } else {
                for item in items {
                    lengths(item, out);
                }
            }
        } else if let Some(members) = json.as_object() {
            for item in members.values() {
                lengths(item, out);
            }
        }
    }
    let mut found = Vec::new();
    lengths(&json, &mut found);
    found.contains(&2) && found.contains(&3)
}

fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    for entry in STANDARD_TYPES {
        // A feature isn't stored yet: its JSON is its type arguments' too
        // (area 20, chunk 5.5u6).
        if !(stored_in_columns(entry) || stored_as_json(entry))
            || entry.name == jadpo_std::RANGE
            || entry.json == jadpo_std::standard::Json::Feature
        {
            continue;
        }
        // A built-in type is named in a signature by a type built on it.
        let ty = match entry.holds {
            Holds::Builtin(_) => format!("Stored{}", entry.name),
            _ => entry.name.to_string(),
        };
        out.push(Case {
            reading: reading(entry, &ty),
            ty,
            name: entry.name.to_string(),
            texts: entry
                .examples
                .valid
                .iter()
                .map(|(text, _)| (*text).to_string())
                .filter(|text| !mixes_altitudes(text))
                .collect(),
        });
    }
    // A range of each item PostgreSQL has a range type for, with an open
    // end.
    let range = jadpo_std::standard_type(jadpo_std::RANGE).expect("Range");
    for (item, texts) in [
        ("Int", &["1/5", "../12", "-3/.."][..]),
        ("Decimal", &["1.5/2.25", "0/.."][..]),
        (
            "CalendarDate",
            &["2026-10-01/2026-11-01", "../2026-12-25"][..],
        ),
        (
            "Instant",
            &["2026-10-25T09:00:00Z/2026-10-25T10:30:00Z"][..],
        ),
        ("Duration", &["PT1M/PT1H30M"][..]),
    ] {
        let ty = format!("Range<{item}>");
        out.push(Case {
            name: format!("Range{item}"),
            reading: reading(range, &ty),
            texts: texts.iter().map(|text| (*text).to_string()).collect(),
            ty,
        });
    }
    out
}

/// The project: an entity and an action keeping one value for each case,
/// a test storing each value and reading it back, and the fixed and
/// commented columns the migration's SQL is checked for.
fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("standard-storage-{name}"));
    let _ = fs::remove_dir_all(&root);
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        "domains/store/types/pounds.jadpo",
        "/// An amount in pounds.\ntype Pounds = Money {\n    currency: Currency.gbp\n}\n",
    );
    write(
        &root,
        "domains/store/types/pounds_to_euros.jadpo",
        "/// A rate from pounds to euros.\ntype PoundsToEuros = ExchangeRate {\n    from: Currency.gbp\n    to: Currency.eur\n}\n",
    );
    write(
        &root,
        "domains/store/entities/fixed_holding.jadpo",
        "use store/types { Pounds, PoundsToEuros }\n\n/// A fixed currency, a fixed rate, a quantity and standard enums.\nentity FixedHolding {\n    id: Uuid\n    paid: Pounds\n    rate: PoundsToEuros\n    shelf: Length\n    zone: Zone\n    method: HttpMethod\n}\n",
    );
    write(
        &root,
        "domains/store/failures/bad_example.jadpo",
        "/// An example that isn't its type.\ntype BadExample = Failure {\n    kind: FailureKind.invalid_value\n}\n",
    );
    write(
        &root,
        "domains/store/types/stored_zoned_time.jadpo",
        "/// A zoned time, stored.\ntype StoredZonedTime = ZonedTime {}\n",
    );
    write(
        &root,
        "domains/store/types/raw_text.jadpo",
        "/// An example's text.\ntype RawText = Text {}\n",
    );
    let mut readers = String::new();
    let mut readers_use = Vec::new();
    for case in cases() {
        let reader = format!("read_{}", snake(&case.name));
        readers.push_str(&format!(
            "    /// The `{ty}` `raw` is.\n    function {reader}(raw: RawText) fails BadExample -> {ty} {{\n        return {reading}\n    }}\n\n",
            ty = case.ty,
            reading = case.reading,
        ));
        if case.ty.starts_with("Stored") {
            readers_use.push(case.ty.clone());
        }
        let entity = format!("Holds{}", case.name);
        let action = format!("keep_{}", snake(&case.name));
        let used = if case.ty.starts_with("Stored") {
            format!("use store/types {{ {} }}\n", case.ty)
        } else {
            String::new()
        };
        write(
            &root,
            &format!("domains/store/entities/{}.jadpo", snake(&entity)),
            &format!(
                "{used}{gap}/// Holds one `{ty}`.\nentity {entity} {{\n    id: Uuid\n    value: {ty}\n}}\n",
                gap = if used.is_empty() { "" } else { "\n" },
                ty = case.ty
            ),
        );
        write(
            &root,
            &format!("domains/store/actions/{action}.jadpo"),
            &format!(
                "use store/entities {{ {entity} }}\n{used}\n/// Keeps `value`.\naction {action}(value: {ty}) -> {entity} {{\n    return {entity}.create {{\n        set: {{\n            value: value\n        }}\n    }}\n}}\n",
                ty = case.ty,
            ),
        );
        let mut fixtures = Vec::new();
        let mut body = String::new();
        for (at, text) in case.texts.iter().enumerate() {
            let fixture = format!("example_{}_{at}", snake(&case.name));
            write(
                &root,
                &format!("domains/store/fixtures/{fixture}.jadpo"),
                &format!(
                    "use store/libraries {{ examples }}\n{used}\n/// Example {at} of `{ty}`.\nfixture {fixture}() -> {ty} {{\n    return attempt examples.{reader}({text})\n}}\n",
                    ty = case.ty,
                    text = literal(text),
                ),
            );
            body.push_str(&format!(
                "    var value_{at}: {ty} = {fixture}()\n    var kept_{at}: {entity} = {action}(value_{at})\n    var back_{at}: {entity} = attempt {entity}.get(kept_{at}.id)\n    assert back_{at}.value == value_{at}\n",
                ty = case.ty,
            ));
            fixtures.push(fixture);
        }
        write(
            &root,
            &format!("domains/store/actions/{action}.test.jadpo"),
            &format!(
                "use store/entities {{ {entity} }}\nuse store/fixtures {{ {} }}\n{used}\ntest \"a {ty} is stored and read back\" {{\n{body}}}\n",
                fixtures.join(", "),
                ty = case.ty,
            ),
        );
    }
    let more = readers_use
        .iter()
        .map(|ty| format!(", {ty}"))
        .collect::<String>();
    write(
        &root,
        "domains/store/libraries/examples.jadpo",
        &format!(
            "use store/failures {{ BadExample }}\nuse store/types {{ RawText{more} }}\n\n/// Each stored type's examples, read from their text.\nlibrary examples {{\n{}\n}}\n",
            readers.trim_end()
        ),
    );
    root
}

#[test]
fn every_standard_object_and_choice_is_stored_and_read_back() {
    let root = project("all");
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let sql = fs::read_dir(root.join("database/migrations"))
        .expect("migrations")
        .map(|entry| entry.expect("entry").path().join("postgres.sql"))
        .map(|path| fs::read_to_string(path).expect("postgres.sql"))
        .collect::<String>();
    // A quantity's column names its unit; a fixed currency and a fixed
    // rate are held by check constraints; a choice's kind and a native
    // range are columns of their own; a JSON-held value is one `jsonb`
    // column.
    for expected in [
        "COMMENT ON COLUMN \"fixed_holding\".\"shelf\" IS 'Length in metres (m)'",
        "\"fixed_holding_paid_currency_check\" CHECK (\"paid_currency\" = 'GBP')",
        "\"fixed_holding_rate_from_check\" CHECK (\"rate_from\" = 'GBP')",
        "\"fixed_holding_rate_to_check\" CHECK (\"rate_to\" = 'EUR')",
        "\"value_kind\" text NOT NULL",
        "\"value\" daterange NOT NULL",
        "\"value_amount\" numeric(38, 9)",
        "CREATE TABLE \"holds_rich_text\" (\n    \"id\" uuid NOT NULL,\n    \"value\" jsonb NOT NULL",
    ] {
        assert!(sql.contains(expected), "`{expected}` in:\n{sql}");
    }
    let snapshot = fs::read_dir(root.join("database/migrations"))
        .expect("migrations")
        .map(|entry| entry.expect("entry").path().join("snapshot.json"))
        .map(|path| fs::read_to_string(path).expect("snapshot.json"))
        .collect::<String>();
    // A standard enum is stored by its code.
    assert!(snapshot.contains("\"Europe/London\""), "{snapshot}");
    assert!(snapshot.contains("\"GET\""), "{snapshot}");
    if !jadpo_test::live() {
        return;
    }
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    let count = cases().len();
    assert!(
        tested.stdout.contains(&format!("{count} passed")),
        "{}",
        tested.stdout
    );
}
