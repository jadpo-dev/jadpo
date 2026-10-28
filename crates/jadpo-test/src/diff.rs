//! Whole values compared field by field (area 13, "Comparing values"): each field,
//! item or key whose values differ, going into records, variants of the
//! same case, lists, sets and maps.

use jadpo_interp::Value;

use crate::outcome::{Change, ChangeMark};

/// The most differences a report lists; `--json` has the same cap, and
/// the expected and actual values are always whole.
const MOST: usize = 20;

/// The differences between `expected` and `actual`, when both are values
/// with parts of the same shape; none otherwise, since expected and actual
/// already say everything.
pub(crate) fn diff(expected: &Value, actual: &Value) -> Vec<Change> {
    let mut changes = Vec::new();
    if comparable(expected, actual) {
        walk("", expected, actual, &mut changes);
    }
    changes.truncate(MOST);
    changes
}

/// Whether two values have parts to compare one by one.
fn comparable(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::Record(e), Value::Record(a)) => e.ty == a.ty,
        (Value::Variant(e), Value::Variant(a)) => {
            e.ty == a.ty && e.name == a.name && !e.fields.is_empty()
        }
        (Value::List(_), Value::List(_))
        | (Value::Set(_), Value::Set(_))
        | (Value::Map(_), Value::Map(_)) => true,
        _ => false,
    }
}

fn join(path: &str, field: &str) -> String {
    if path.is_empty() {
        field.to_string()
    } else {
        format!("{path}.{field}")
    }
}

fn changed(path: String, expected: &Value, actual: &Value) -> Change {
    Change {
        mark: ChangeMark::Changed,
        path,
        expected: Some(expected.to_string()),
        actual: Some(actual.to_string()),
    }
}

fn walk(path: &str, expected: &Value, actual: &Value, out: &mut Vec<Change>) {
    if expected == actual || out.len() >= MOST {
        return;
    }
    let inner = |path: String, e: &Value, a: &Value, out: &mut Vec<Change>| {
        if comparable(e, a) {
            walk(&path, e, a, out);
        } else if e != a {
            out.push(changed(path, e, a));
        }
    };
    match (expected, actual) {
        (Value::Record(e), Value::Record(a)) => {
            for ((name, e), (_, a)) in e.fields.iter().zip(a.fields.iter()) {
                inner(join(path, name), e, a, out);
            }
        }
        (Value::Variant(e), Value::Variant(a)) => {
            for ((name, e), (_, a)) in e.fields.iter().zip(a.fields.iter()) {
                inner(join(path, name), e, a, out);
            }
        }
        (Value::List(e), Value::List(a)) => {
            for i in 0..e.len().max(a.len()) {
                let at = format!("{path}[{i}]");
                match (e.get(i), a.get(i)) {
                    (Some(e), Some(a)) => inner(at, e, a, out),
                    (None, Some(a)) => out.push(Change {
                        mark: ChangeMark::Extra,
                        path: at,
                        expected: None,
                        actual: Some(a.to_string()),
                    }),
                    (Some(e), None) => out.push(Change {
                        mark: ChangeMark::Missing,
                        path: at,
                        expected: Some(e.to_string()),
                        actual: None,
                    }),
                    (None, None) => {}
                }
            }
        }
        (Value::Set(e), Value::Set(a)) => {
            let shown = if path.is_empty() { "item" } else { path };
            for item in a.iter().filter(|item| !e.contains(item)) {
                out.push(Change {
                    mark: ChangeMark::Extra,
                    path: shown.to_string(),
                    expected: None,
                    actual: Some(item.to_string()),
                });
            }
            for item in e.iter().filter(|item| !a.contains(item)) {
                out.push(Change {
                    mark: ChangeMark::Missing,
                    path: shown.to_string(),
                    expected: Some(item.to_string()),
                    actual: None,
                });
            }
        }
        (Value::Map(e), Value::Map(a)) => {
            let mut keys: Vec<&Value> = e.keys().chain(a.keys()).collect();
            keys.sort();
            keys.dedup();
            for key in keys {
                let at = format!("{path}[{key}]");
                match (e.get(key), a.get(key)) {
                    (Some(e), Some(a)) => inner(at, e, a, out),
                    (None, Some(a)) => out.push(Change {
                        mark: ChangeMark::Extra,
                        path: at,
                        expected: None,
                        actual: Some(a.to_string()),
                    }),
                    (Some(e), None) => out.push(Change {
                        mark: ChangeMark::Missing,
                        path: at,
                        expected: Some(e.to_string()),
                        actual: None,
                    }),
                    (None, None) => {}
                }
            }
        }
        _ => out.push(changed(path.to_string(), expected, actual)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: i64, y: i64) -> Value {
        Value::record(
            "Point",
            vec![("x".into(), Value::Int(x)), ("y".into(), Value::Int(y))],
        )
    }

    fn shown(changes: &[Change]) -> Vec<String> {
        changes
            .iter()
            .map(|change| {
                format!(
                    "{} {} {} {}",
                    change.mark.symbol(),
                    change.path,
                    change.expected.as_deref().unwrap_or("-"),
                    change.actual.as_deref().unwrap_or("-")
                )
            })
            .collect()
    }

    #[test]
    fn records_differ_field_by_field_and_nested() {
        assert_eq!(shown(&diff(&point(3, 2), &point(2, 2))), ["~ x 3 2"]);
        let line = |p: Value| Value::record("Line", vec![("from".into(), p)]);
        assert_eq!(
            shown(&diff(&line(point(1, 1)), &line(point(1, 4)))),
            ["~ from.y 1 4"]
        );
    }

    #[test]
    fn lists_compare_item_by_item_and_count_what_is_left_over() {
        let list = |items: &[i64]| Value::list(items.iter().map(|n| Value::Int(*n)).collect());
        assert_eq!(
            shown(&diff(&list(&[1, 2, 3]), &list(&[1, 5]))),
            ["~ [1] 2 5", "- [2] 3 -"]
        );
        assert_eq!(shown(&diff(&list(&[1]), &list(&[1, 2]))), ["+ [1] - 2"]);
    }

    #[test]
    fn scalars_and_different_shapes_have_no_diff() {
        assert!(diff(&Value::Int(1), &Value::Int(2)).is_empty());
        assert!(diff(&point(1, 1), &Value::Int(2)).is_empty());
    }
}
