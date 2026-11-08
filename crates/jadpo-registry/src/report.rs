//! What a contract run says of an answer a package can no longer decode
//! (area 11, "What a release shows"): its shape, and never a value.

use std::collections::BTreeSet;

use serde_json::Value;

use crate::protocol::{LONGEST_SHAPE_PATH, MOST_SHAPE_MEMBERS, ShapeKind, ShapeMember};

/// A member's name stands for itself only when it reads as a field's name.
/// Any other, such as a map keyed by a customer's id or address, is `*`, so
/// a value can't leave as a name.
const ANY_NAME: &str = "*";

/// The shortest list of the distinct places in `answer` and the kind of
/// value at each, by path; at most [`MOST_SHAPE_MEMBERS`], in path order.
pub fn shape_of(answer: &Value) -> Vec<ShapeMember> {
    let mut found = BTreeSet::new();
    walk(answer, "$", &mut found);
    found.into_iter().take(MOST_SHAPE_MEMBERS).collect()
}

fn walk(value: &Value, path: &str, found: &mut BTreeSet<ShapeMember>) {
    if path.len() > LONGEST_SHAPE_PATH {
        return;
    }
    let kind = match value {
        Value::Object(_) => ShapeKind::Object,
        Value::Array(_) => ShapeKind::List,
        Value::String(_) => ShapeKind::Text,
        Value::Number(_) => ShapeKind::Number,
        Value::Bool(_) => ShapeKind::Bool,
        Value::Null => ShapeKind::Null,
    };
    found.insert(ShapeMember {
        path: path.to_string(),
        kind,
    });
    match value {
        Value::Object(members) => {
            for (name, member) in members {
                walk(member, &format!("{path}.{}", shown_name(name)), found);
            }
        }
        Value::Array(items) => {
            for item in items {
                walk(item, &format!("{path}[]"), found);
            }
        }
        _ => {}
    }
}

/// `name` when it reads as a field's name: letters, `_` and `-` only,
/// starting with a letter, at most 64 bytes.
fn shown_name(name: &str) -> &str {
    let reads = name.len() <= 64
        && name.starts_with(|c: char| c.is_ascii_alphabetic())
        && name
            .chars()
            .all(|c| c.is_ascii_alphabetic() || c == '_' || c == '-');
    if reads { name } else { ANY_NAME }
}
