//! Completion (area 14, "Completion"; chunk 5.5a3): what the editor offers
//! at the cursor, in every context the page lists, generated from the
//! declarations and data the checker uses. Each test writes a file with
//! the cursor marked `|`, opens it, and reads what `textDocument/completion`
//! answers there.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use jadpo_lsp::Server;
use lsp_server::{Message, Notification, Request, RequestId};
use serde_json::{Value, json};

const WORK: &str = "domains/shop/actions/work.jadpo";

const HEADER: &str = "use shared/types { Note, ProvingTime, Shade }\nuse shared/libraries { rounding }\n\n/// Works out how long a bake waits.\naction work(note: Note, shade: Shade, wait: ProvingTime) -> ProvingTime {\n";

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        "application {}\n\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb]\n}\n",
    ),
    (
        "config.jadpo",
        "use shared/types { ShopName }\n\nconfig {\n    shop_name: ShopName {\n        default: \"Bakery\"\n    }\n}\n",
    ),
    (
        "shared/types/note.jadpo",
        "/// A baker's note.\ntype Note = Object {\n    body: Text\n    pinned: Bool\n}\n",
    ),
    (
        "shared/types/proving_time.jadpo",
        "/// How long the dough proves.\ntype ProvingTime = Duration {\n    min: 30m\n    max: 4h\n}\n",
    ),
    (
        "shared/types/shade.jadpo",
        "/// How dark a crust is.\ntype Shade = Enum {\n    plain\n    tinted\n}\n",
    ),
    (
        "shared/types/shop_name.jadpo",
        "/// The shop's name.\ntype ShopName = Text {\n    max_length: 40\n}\n",
    ),
    (
        "shared/libraries/rounding.jadpo",
        "use shared/types { ProvingTime }\n\nlibrary rounding {\n    /// A wait rounded up to the minute.\n    function to_minute(wait: ProvingTime) -> ProvingTime {\n        return wait\n    }\n}\n",
    ),
    (
        "domains/shop/translations/parcels.jadpo",
        "/// How many parcels an order has.\ntype Parcels = Message {\n    count: PluralCount\n}\n",
    ),
    (
        "domains/shop/translations/en_gb.jadpo",
        "translations {\n    parcels: {\n        one: \"{count} parcel\"\n        other: \"{count} parcels\"\n    }\n}\n",
    ),
    (
        WORK,
        "use shared/types { Note, ProvingTime, Shade }\nuse shared/libraries { rounding }\n\n/// Works out how long a bake waits.\naction work(note: Note, shade: Shade, wait: ProvingTime) -> ProvingTime {\n    return wait\n}\n",
    ),
];

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in FILES {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    root.canonicalize().expect("real path")
}

fn start(root: &Path) -> (Server, Value) {
    start_with(root, true)
}

fn start_with(root: &Path, snippets: bool) -> (Server, Value) {
    let params = serde_json::from_value(json!({
        "processId": null,
        "rootUri": format!("file://{}", root.display()),
        "capabilities": {
            "textDocument": {"completion": {"completionItem": {"snippetSupport": snippets}}},
        },
    }))
    .expect("initialize params");
    let (server, result) = Server::initialize(&params, root);
    (server, serde_json::to_value(result).expect("result"))
}

/// The file `text` with its `|` taken out, and where the `|` was as a
/// line and a character.
fn cursor(text: &str) -> (String, u32, u32) {
    let at = text.find('|').expect("a cursor");
    let before = &text[..at];
    let line = before.matches('\n').count() as u32;
    let character = (at - before.rfind('\n').map_or(0, |i| i + 1)) as u32;
    (text.replacen('|', "", 1), line, character)
}

/// Every item offered with the work file written as `text`.
fn complete_file(name: &str, text: &str) -> Vec<Value> {
    complete_in(name, text, true)
}

fn complete_in(name: &str, text: &str, snippets: bool) -> Vec<Value> {
    let root = project(name);
    let (mut server, _) = start_with(&root, snippets);
    let (text, line, character) = cursor(text);
    let uri = format!("file://{}", root.join(WORK).display());
    server.handle(Message::Notification(Notification::new(
        "textDocument/didOpen".to_string(),
        json!({"textDocument": {"uri": uri, "languageId": "jadpo", "version": 1, "text": text}}),
    )));
    let answer = server.handle(Message::Request(Request::new(
        RequestId::from(1),
        "textDocument/completion".to_string(),
        json!({
            "textDocument": {"uri": uri},
            "position": {"line": line, "character": character},
        }),
    )));
    let Some(Message::Response(response)) = answer.first() else {
        panic!("{answer:?}");
    };
    let result = response
        .response_result
        .clone()
        .unwrap_or_else(|error| panic!("{error:?}"));
    match result {
        Value::Null => Vec::new(),
        Value::Array(items) => items,
        Value::Object(list) => list["items"].as_array().cloned().unwrap_or_default(),
        other => panic!("{other}"),
    }
}

/// Every item offered with the work action's body written as `body`.
fn complete(name: &str, body: &str) -> Vec<Value> {
    complete_file(name, &format!("{HEADER}{body}\n}}\n"))
}

fn labels(items: &[Value]) -> BTreeSet<String> {
    items
        .iter()
        .map(|item| item["label"].as_str().expect("label").to_string())
        .collect()
}

fn item<'a>(items: &'a [Value], label: &str) -> &'a Value {
    items
        .iter()
        .find(|item| item["label"] == label)
        .unwrap_or_else(|| panic!("no `{label}` in {:?}", labels(items)))
}

fn has(items: &[Value], wanted: &[&str]) {
    let found = labels(items);
    for label in wanted {
        assert!(found.contains(*label), "no `{label}` in {found:?}");
    }
}

fn lacks(items: &[Value], unwanted: &[&str]) {
    let found = labels(items);
    for label in unwanted {
        assert!(!found.contains(*label), "`{label}` offered in {found:?}");
    }
}

fn set(labels: &[&str]) -> BTreeSet<String> {
    labels.iter().map(|label| label.to_string()).collect()
}

/// What choosing an item writes.
fn inserted(item: &Value) -> String {
    item["textEdit"]["newText"]
        .as_str()
        .expect("an edit")
        .to_string()
}

/// An item's text, as a client shows it: the docs or the detail.
fn text_of(item: &Value) -> String {
    let docs = match &item["documentation"] {
        Value::String(text) => text.clone(),
        Value::Object(markup) => markup["value"].as_str().unwrap_or_default().to_string(),
        _ => String::new(),
    };
    format!("{} {docs}", item["detail"].as_str().unwrap_or_default())
}

#[test]
fn the_server_offers_completion_on_dots_colons_and_digits() {
    let root = project("lsp-completion-capability");
    let (_, result) = start(&root);
    let provider = &result["capabilities"]["completionProvider"];
    let triggers: BTreeSet<&str> = provider["triggerCharacters"]
        .as_array()
        .expect("trigger characters")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    for wanted in [".", ":", "0", "5", "9"] {
        assert!(triggers.contains(wanted), "{triggers:?}");
    }
}

#[test]
fn names_in_scope_are_offered_with_those_that_fit_first() {
    let items = complete(
        "lsp-completion-names",
        "    var total: ProvingTime = wait\n    if note.pinned {\n        var hidden: ProvingTime = wait\n    }\n    return t|",
    );
    has(
        &items,
        &[
            "total", "wait", "note", "shade", "rounding", "temporal", "Shade", "true",
        ],
    );
    lacks(&items, &["hidden"]);
    // `return` expects a `ProvingTime`: the locals that are one come first.
    let sort = |label: &str| {
        item(&items, label)["sortText"]
            .as_str()
            .expect("sort")
            .to_string()
    };
    assert!(
        sort("total") < sort("note"),
        "{} {}",
        sort("total"),
        sort("note")
    );
    assert!(sort("wait") < sort("shade"));
}

#[test]
fn an_unimported_type_comes_with_its_use_line() {
    let items = complete("lsp-completion-import", "    return Sho|");
    let shop_name = item(&items, "ShopName");
    let edits = shop_name["additionalTextEdits"]
        .as_array()
        .expect("an import edit");
    assert_eq!(edits.len(), 1, "{edits:?}");
    assert_eq!(edits[0]["newText"], ", ShopName");
    // A name the file already imports needs nothing more.
    assert!(item(&items, "Shade")["additionalTextEdits"].is_null());
}

#[test]
fn where_an_enum_is_required_its_variants_come_first() {
    let items = complete(
        "lsp-completion-expected-enum",
        "    var dark: Shade = |\n    return wait",
    );
    has(&items, &["Shade.plain", "Shade.tinted", "shade"]);
    // A variable isn't offered in its own value.
    lacks(&items, &["dark"]);
    let sort = |label: &str| {
        item(&items, label)["sortText"]
            .as_str()
            .expect("sort")
            .to_string()
    };
    assert!(sort("Shade.plain") < sort("note"));
    let items = complete(
        "lsp-completion-expected-argument",
        "    var day: CalendarDate = \"2026-10-25\"\n    var at: ZonedTime = temporal.resolve(day, dst: |)\n    return wait",
    );
    let sort = |label: &str| {
        item(&items, label)["sortText"]
            .as_str()
            .expect("sort")
            .to_string()
    };
    assert!(sort("Dst.compatible") < sort("day"));
}

#[test]
fn a_records_fields_not_given_yet_are_offered() {
    let items = complete(
        "lsp-completion-record",
        "    var shown: Note = Note { body: \"x\", |\n    return wait",
    );
    assert_eq!(labels(&items), set(&["pinned"]));
    assert_eq!(inserted(item(&items, "pinned")), "pinned: ");
}

#[test]
fn a_values_fields_are_offered_after_a_dot() {
    let items = complete(
        "lsp-completion-fields",
        "    var shown: Bool = note.|\n    return wait",
    );
    assert_eq!(labels(&items), set(&["body", "pinned"]));
    assert!(text_of(item(&items, "pinned")).contains("Bool"));
    // `pinned` is the `Bool` the variable wants.
    let sort = |label: &str| {
        item(&items, label)["sortText"]
            .as_str()
            .expect("sort")
            .to_string()
    };
    assert!(sort("pinned") < sort("body"));
}

#[test]
fn a_namespaces_operations_carry_their_signature_failures_and_docs() {
    let items = complete("lsp-completion-operations", "    return temporal.|");
    has(&items, &["between", "add_days", "resolve", "in_zone"]);
    let resolve = text_of(item(&items, "resolve"));
    assert!(
        resolve
            .contains("temporal.resolve(date: T, at: Text?, zone: Zone, dst: Dst?) -> ZonedTime"),
        "{resolve}"
    );
    assert!(resolve.contains("The instant the clock reads"), "{resolve}");
    assert!(resolve.contains("`dst`"), "{resolve}");
}

/// Completion names no operation: a standard-library operation added to a
/// stub is offered with no completion code of its own.
#[test]
fn every_standard_library_operation_is_offered_from_its_stub() {
    let mut by_namespace: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for signature in jadpo_check::standard_operations() {
        let (namespace, name) = signature.name.split_once('.').expect("qualified");
        by_namespace
            .entry(namespace.to_string())
            .or_default()
            .insert(name.to_string());
    }
    assert!(by_namespace.len() >= 6, "{by_namespace:?}");
    for (namespace, operations) in by_namespace {
        let items = complete(
            &format!("lsp-completion-stub-{namespace}"),
            &format!("    return {namespace}.|"),
        );
        let offered = labels(&items);
        assert!(
            operations.is_subset(&offered),
            "{namespace}: {operations:?} not all in {offered:?}"
        );
    }
}

#[test]
fn an_enums_variants_are_offered_after_its_name() {
    let items = complete(
        "lsp-completion-variants",
        "    var dark: Shade = Shade.|\n    return wait",
    );
    assert_eq!(labels(&items), set(&["plain", "tinted"]));
    // A standard type's variants carry the name CLDR gives them.
    let items = complete(
        "lsp-completion-locale",
        "    var shown: Locale = Locale.|\n    return wait",
    );
    has(&items, &["en_gb", "fr_fr"]);
    assert!(
        text_of(item(&items, "en_gb")).contains("British English"),
        "{}",
        text_of(item(&items, "en_gb"))
    );
}

#[test]
fn a_numbers_units_are_offered_for_the_type_it_goes_into() {
    let items = complete("lsp-completion-units", "    return 2|");
    assert_eq!(labels(&items), set(&["d", "h", "m", "s", "ms"]));
    assert!(text_of(item(&items, "m")).contains("minutes"));
    // The suffix replaces only what is typed after the digits.
    let edit = &item(&items, "s")["textEdit"];
    assert_eq!(edit["range"]["start"], edit["range"]["end"]);
    assert_eq!(edit["newText"], "s");
    let items = complete("lsp-completion-units-typed", "    return 25m|");
    let edit = &item(&items, "ms")["textEdit"];
    assert_eq!(edit["range"]["start"]["character"], 13);
    assert_eq!(edit["range"]["end"]["character"], 14);
    // A number that goes into a type without units gets none.
    let items = complete(
        "lsp-completion-units-none",
        "    var count: Int = 2|\n    return wait",
    );
    assert!(items.is_empty(), "{:?}", labels(&items));
    // A quantity's own suffixes, narrowed to its kind (area 19): metres
    // are `metre`, since `m` is minutes, and inches `inch`.
    let items = complete(
        "lsp-completion-units-length",
        "    var shelf: Length = 2|\n    return wait",
    );
    assert_eq!(
        labels(&items),
        set(&["mm", "cm", "metre", "km", "inch", "ft", "yd", "mi", "nmi"])
    );
    assert!(text_of(item(&items, "metre")).contains("Length in metres"));
}

#[test]
fn a_calls_named_arguments_are_offered_once() {
    let items = complete(
        "lsp-completion-arguments",
        "    var day: CalendarDate = \"2026-10-25\"\n    var at: ZonedTime = temporal.resolve(day, |)\n    return wait",
    );
    has(&items, &["at", "zone", "dst", "day"]);
    lacks(&items, &["date"]);
    assert_eq!(inserted(item(&items, "zone")), "zone: ");
    let items = complete(
        "lsp-completion-arguments-named",
        "    var day: CalendarDate = \"2026-10-25\"\n    var at: ZonedTime = temporal.resolve(date: day, zone: Zone.europe_london, |)\n    return wait",
    );
    has(&items, &["at", "dst"]);
    lacks(&items, &["date", "zone"]);
}

#[test]
fn type_names_are_offered_where_a_type_goes() {
    let items = complete("lsp-completion-types", "    var later: |\n    return wait");
    has(
        &items,
        &[
            "ProvingTime",
            "Note",
            "Shade",
            "Int",
            "Text",
            "Duration",
            "Locale",
            "InstantRange",
        ],
    );
    lacks(&items, &["rounding", "temporal", "wait", "var"]);
    let items = complete_file(
        "lsp-completion-param-types",
        "use shared/types { Note, ProvingTime, Shade }\n\naction work(note: N|) -> Note {\n    return note\n}\n",
    );
    has(&items, &["Note", "ProvingTime"]);
    lacks(&items, &["note"]);
}

#[test]
fn only_declinable_rules_are_offered_after_decline() {
    let below = "use shared/types { Note, ProvingTime, Shade }\nuse shared/libraries { rounding }\n\n/// Works out how long a bake waits.\naction work(note: Note, shade: Shade, wait: ProvingTime) -> ProvingTime {\n    return wait\n}\n";
    let items = complete_file("lsp-completion-declines", &format!("decline |\n{below}"));
    let declinable: BTreeSet<String> = jadpo_check::declines::declinable_rules()
        .into_iter()
        .map(str::to_string)
        .collect();
    assert!(!declinable.is_empty());
    assert_eq!(labels(&items), declinable);
    // A rule partly written is replaced whole, dots and all.
    let items = complete_file(
        "lsp-completion-declines-typed",
        &format!("decline library.|\n{below}"),
    );
    let first = items.first().expect("a rule");
    assert_eq!(first["textEdit"]["range"]["start"]["character"], 8);
}

#[test]
fn settings_are_offered_after_config() {
    let items = complete(
        "lsp-completion-config",
        "    var shown: ShopName = config.|\n    return wait",
    );
    assert_eq!(labels(&items), set(&["shop_name"]));
}

#[test]
fn messages_are_offered_after_translations() {
    let items = complete(
        "lsp-completion-translations",
        "    var shown: PresentationText = translations.|\n    return wait",
    );
    has(&items, &["shop"]);
    let items = complete(
        "lsp-completion-messages",
        "    var shown: PresentationText = translations.shop.|\n    return wait",
    );
    assert_eq!(labels(&items), set(&["parcels"]));
    let insert = inserted(item(&items, "parcels"));
    assert!(
        insert.contains("locale: ") && insert.contains("count: "),
        "{insert}"
    );
}

#[test]
fn keywords_are_offered_where_a_statement_or_declaration_starts() {
    let items = complete("lsp-completion-statements", "    |\n    return wait");
    has(&items, &["var", "return", "if", "for", "wait"]);
    lacks(&items, &["action", "type"]);
    let items = complete_file(
        "lsp-completion-declarations",
        &format!("{HEADER}    return wait\n}}\n\n|\n"),
    );
    has(
        &items,
        &["type", "action", "function", "library", "use", "test"],
    );
    lacks(&items, &["var", "return", "wait"]);
}

#[test]
fn a_call_with_required_arguments_is_inserted_as_a_snippet() {
    let items = complete("lsp-completion-snippet", "    return temporal.res|");
    let resolve = item(&items, "resolve");
    assert_eq!(inserted(resolve), "resolve(${1:date}, zone: ${2:zone})");
    assert_eq!(resolve["insertTextFormat"], 2);
    // An editor that doesn't fill snippets in gets the name alone.
    let items = complete_in(
        "lsp-completion-no-snippets",
        &format!("{HEADER}    return temporal.res|\n}}\n"),
        false,
    );
    let resolve = item(&items, "resolve");
    assert_eq!(inserted(resolve), "resolve");
    assert_eq!(resolve["insertTextFormat"], 1);
    let items = complete("lsp-completion-snippet-library", "    return rounding.|");
    let to_minute = item(&items, "to_minute");
    assert_eq!(inserted(to_minute), "to_minute(${1:wait})");
    assert!(text_of(to_minute).contains("A wait rounded up to the minute"));
}

#[test]
fn nothing_is_offered_in_a_comment_or_a_string() {
    let items = complete(
        "lsp-completion-comment",
        "    // temporal.|\n    return wait",
    );
    assert!(items.is_empty(), "{:?}", labels(&items));
    let items = complete(
        "lsp-completion-string",
        "    var shown: ShopName = \"temporal.|\"\n    return wait",
    );
    assert!(items.is_empty(), "{:?}", labels(&items));
}
