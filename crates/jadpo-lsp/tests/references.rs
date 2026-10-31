//! Find references (area 14, "Editor support"): every place in the
//! project a name is used that the checker resolves to the same
//! declaration or local, read from the same analysis as go to
//! definition, so a same-named declaration elsewhere is never counted.

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::{Value, json};

use crate::support::{ask, at, project, start, uri};

const PENCE: &str = "shared/types/coins.jadpo";
const ROUNDING: &str = "shared/libraries/rounding.jadpo";
const BASKET: &str = "domains/sales/actions/basket_total.jadpo";
const REFUND: &str = "domains/refunds/actions/refund.jadpo";

const ROUNDING_TEXT: &str = "use shared/types { Coins }\n\nlibrary rounding {\n    /// The amount, never below nothing.\n    function capped(amount: Coins) -> Coins {\n        return amount\n    }\n}\n";
const BASKET_TEXT: &str = "use shared/libraries { rounding }\nuse shared/types { Coins }\n\naction basket_total(price: Coins) -> Coins {\n    var total: Coins = rounding.capped(price)\n    return rounding.capped(total)\n}\n";
// A private `capped` of its own, which isn't the library's.
const REFUND_TEXT: &str = "use shared/types { Coins }\n\naction refund(amount: Coins) -> Coins {\n    return capped(amount)\n}\n\nfunction capped(amount: Coins) -> Coins {\n    return amount\n}\n";

fn files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("app.jadpo", "application {}\n"),
        (PENCE, "type Coins = Int {\n    min: 0\n}\n"),
        (ROUNDING, ROUNDING_TEXT),
        (BASKET, BASKET_TEXT),
        (REFUND, REFUND_TEXT),
    ]
}

/// Each place found, as `path:line:character`.
fn references(root: &Path, path: &str, position: Value, declaration: bool) -> BTreeSet<String> {
    let mut server = start(root, json!({}));
    let found = ask(
        &mut server,
        "textDocument/references",
        json!({
            "textDocument": {"uri": uri(root, path)},
            "position": position,
            "context": {"includeDeclaration": declaration},
        }),
    );
    let prefix = format!("{}/", uri(root, "").trim_end_matches('/'));
    found
        .as_array()
        .expect("locations")
        .iter()
        .map(|location| {
            format!(
                "{}:{}:{}",
                location["uri"]
                    .as_str()
                    .expect("uri")
                    .trim_start_matches(&prefix),
                location["range"]["start"]["line"],
                location["range"]["start"]["character"],
            )
        })
        .collect()
}

fn place(path: &str, text: &str, needle: &str, nth: usize, ahead: usize) -> String {
    let position = at(text, needle, nth, ahead);
    format!("{path}:{}:{}", position["line"], position["character"])
}

#[test]
fn a_type_is_found_wherever_it_is_used() {
    let root = project("references-type", &files());
    let found = references(&root, PENCE, at("type Coins", "Coins", 0, 0), true);
    let mut expected: BTreeSet<String> = BTreeSet::new();
    expected.insert(format!("{PENCE}:0:5"));
    for nth in 0..3 {
        expected.insert(place(ROUNDING, ROUNDING_TEXT, "Coins", nth, 0));
        expected.insert(place(REFUND, REFUND_TEXT, "Coins", nth, 0));
    }
    for nth in 3..5 {
        expected.insert(place(REFUND, REFUND_TEXT, "Coins", nth, 0));
    }
    for nth in 0..4 {
        expected.insert(place(BASKET, BASKET_TEXT, "Coins", nth, 0));
    }
    assert_eq!(found, expected);
}

#[test]
fn the_declaration_is_left_out_when_the_editor_asks() {
    let root = project("references-no-declaration", &files());
    let found = references(&root, BASKET, at(BASKET_TEXT, "Coins", 1, 2), false);
    assert!(!found.contains(&format!("{PENCE}:0:5")), "{found:?}");
    assert!(found.contains(&place(BASKET, BASKET_TEXT, "Coins", 1, 0)));
    assert_eq!(found.len(), 12, "{found:?}");
}

#[test]
fn a_library_function_is_not_a_private_function_of_its_name() {
    let root = project("references-function", &files());
    let found = references(&root, BASKET, at(BASKET_TEXT, "capped", 0, 1), true);
    let expected: BTreeSet<String> = [
        place(ROUNDING, ROUNDING_TEXT, "capped", 0, 0),
        place(BASKET, BASKET_TEXT, "capped", 0, 0),
        place(BASKET, BASKET_TEXT, "capped", 1, 0),
    ]
    .into_iter()
    .collect();
    assert_eq!(found, expected);
}

#[test]
fn a_parameter_is_found_in_its_own_body_only() {
    let root = project("references-local", &files());
    let found = references(&root, REFUND, at(REFUND_TEXT, "amount", 1, 0), true);
    let expected: BTreeSet<String> = [
        place(REFUND, REFUND_TEXT, "amount", 0, 0),
        place(REFUND, REFUND_TEXT, "amount", 1, 0),
    ]
    .into_iter()
    .collect();
    assert_eq!(found, expected);
}

#[test]
fn a_variable_is_found_from_where_it_is_declared() {
    let root = project("references-variable", &files());
    let found = references(&root, BASKET, at(BASKET_TEXT, "total:", 0, 0), true);
    let expected: BTreeSet<String> = [
        place(BASKET, BASKET_TEXT, "total", 1, 0),
        place(BASKET, BASKET_TEXT, "total)", 0, 0),
    ]
    .into_iter()
    .collect();
    assert_eq!(found, expected);
}
