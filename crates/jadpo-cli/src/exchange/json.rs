//! JSON in the shape Chrome's extension messages use, which Crowdin imports
//! as "Chrome JSON": an object of units, each `{ "message": the wording,
//! "description": what a translator is told }`. It is the one JSON shape
//! with a place for a description that Crowdin reads natively. A plain
//! object of strings is read too, since a service that cannot keep
//! descriptions returns that.

use jadpo_term::Json;

use super::{InFile, InUnit, OutFile, Unreadable};

pub fn write(file: &OutFile) -> String {
    let mut entries = Vec::new();
    for unit in &file.units {
        let mut fields = Vec::new();
        let description = unit
            .notes
            .iter()
            .map(|(_, note)| note.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if !description.is_empty() {
            fields.push(("description", Json::from(description)));
        }
        let message = if file.target_language.is_some() {
            unit.target.clone().unwrap_or_default()
        } else {
            unit.source.clone()
        };
        fields.push(("message", Json::from(message)));
        entries.push((unit.full_id(), Json::object(fields)));
    }
    Json::object(entries).to_document()
}

pub fn read(text: &str) -> Result<InFile, Unreadable> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|error| Unreadable {
        at: offset(text, error.line(), error.column()),
        why: format!("it is not JSON: {error}"),
    })?;
    let serde_json::Value::Object(map) = value else {
        return Err(Unreadable {
            at: 0,
            why: "it is not an object of messages".to_string(),
        });
    };
    let mut units = Vec::new();
    for (id, value) in map {
        let at = text.find(&format!("\"{id}\"")).unwrap_or(0);
        let message = match &value {
            serde_json::Value::String(message) => message.clone(),
            serde_json::Value::Object(fields) => match fields.get("message") {
                Some(serde_json::Value::String(message)) => message.clone(),
                _ => {
                    return Err(Unreadable {
                        at,
                        why: format!("`{id}` has no `message` string"),
                    });
                }
            },
            _ => {
                return Err(Unreadable {
                    at,
                    why: format!("`{id}` is neither a string nor an object with a `message`"),
                });
            }
        };
        units.push(InUnit {
            id,
            source: None,
            target: Some(message),
            start: at,
            end: at + 1,
        });
    }
    Ok(InFile {
        source_language: None,
        target_language: None,
        units,
    })
}

/// The byte a line and column of serde_json's mean.
fn offset(text: &str, line: usize, column: usize) -> usize {
    let start: usize = text
        .split_inclusive('\n')
        .take(line.saturating_sub(1))
        .map(str::len)
        .sum();
    (start + column.saturating_sub(1)).min(text.len())
}

#[cfg(test)]
mod tests {
    use super::super::OutUnit;
    use super::*;

    fn file(target: Option<&str>) -> OutFile {
        OutFile {
            source_language: "en-GB".to_string(),
            target_language: target.map(str::to_string),
            units: vec![OutUnit {
                scope: "shop".to_string(),
                folder: "domains/shop/translations".to_string(),
                id: "farewell".to_string(),
                notes: vec![("description", "Said when signing out.".to_string())],
                source: "Goodbye.".to_string(),
                target: Some("Tschüss.".to_string()),
                reviewed: false,
            }],
        }
    }

    #[test]
    fn writes_and_reads_the_chrome_shape() {
        let text = write(&file(Some("de-DE")));
        assert_eq!(
            text,
            "{\n  \"shop.farewell\": {\n    \"description\": \"Said when signing out.\",\n    \"message\": \"Tschüss.\"\n  }\n}\n"
        );
        let read = read(&text).expect("reads");
        assert_eq!(read.units[0].id, "shop.farewell");
        assert_eq!(read.units[0].target.as_deref(), Some("Tschüss."));
        assert!(write(&file(None)).contains("\"message\": \"Goodbye.\""));
    }

    #[test]
    fn reads_plain_strings_and_refuses_other_shapes() {
        let read =
            read(r#"{"a.b": "x", "c.d": {"message": "y", "description": "z"}}"#).expect("reads");
        assert_eq!(read.units.len(), 2);
        assert!(super::read(r#"{"a": 3}"#).is_err());
        assert!(super::read(r#"{"a": {"msg": "x"}}"#).is_err());
        assert!(super::read("[1]").is_err());
        assert!(super::read("{").is_err());
    }
}
