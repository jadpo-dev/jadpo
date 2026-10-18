//! `prompts.render` (area 21, "Prompts as files"): a prompt's value as the
//! messages a model is sent: its instructions as the system message, its
//! conversation's turns in order, then its request as the user message,
//! each placeholder filled by `jadpo-std`'s [`fill`], which sends free
//! text only as data blocks whose closing tags can't be forged. The same
//! function serves the interpreter and compiled code.

use std::rc::Rc;

use jadpo_std::prompt::{Form, PromptEntry, fill};
use jadpo_std::standard::ai::{CHAT_MESSAGE, CHAT_ROLE, CHAT_ROLES};

use crate::Value;

/// A message of the role called `role`, saying `content`.
fn message(role: &str, content: Value) -> Value {
    let ordinal = CHAT_ROLES
        .iter()
        .position(|(name, _)| *name == role)
        .and_then(|at| u32::try_from(at).ok())
        .unwrap_or(0);
    Value::record(
        CHAT_MESSAGE,
        vec![
            (Rc::from("role"), Value::variant(CHAT_ROLE, ordinal, role, Vec::new())),
            (Rc::from("content"), content),
        ],
    )
}

/// A field's value as its placeholder writes it, given its JSON as its
/// type's codec writes it: free text as it is, a closed value in its text
/// form (its JSON, without the quotes of a string), and an object or a
/// list as its JSON.
fn written(form: Form, value: &Value, json: &str) -> Option<String> {
    match (form, value) {
        (Form::Text, Value::Text(text) | Value::Uuid(text)) => Some(text.to_string()),
        (Form::Json, _) => Some(json.to_string()),
        _ if json.starts_with('"') => serde_json::from_str::<String>(json).ok(),
        _ => Some(json.to_string()),
    }
}

/// The messages `prompt`, a value of `entry`'s type, is sent as. `json`
/// writes the value of the entry's field at a place as its type's JSON.
/// `None` when the value isn't the prompt's, which the checker rules out.
pub fn render(
    entry: &PromptEntry,
    prompt: &Value,
    json: &mut dyn FnMut(usize, &Value) -> Option<String>,
) -> Option<Value> {
    let Value::Record(record) = prompt else {
        return None;
    };
    let mut texts: Vec<(String, String)> = Vec::new();
    for (at, field) in entry.fields.iter().enumerate() {
        let value = record.get(&field.name)?;
        let text = written(field.form, value, &json(at, value)?)?;
        texts.push((field.name.clone(), text));
    }
    let mut value_of = |name: &str| {
        texts
            .iter()
            .find(|(field, _)| field == name)
            .map(|(_, text)| text.clone())
    };
    let instructions = fill(&entry.instructions, &entry.fields, &mut value_of)?;
    let request = fill(&entry.request, &entry.fields, &mut value_of)?;
    let mut messages = vec![message("system", Value::text(&instructions))];
    if let Some(conversation) = &entry.conversation {
        match record.get(conversation)? {
            Value::List(turns) => messages.extend(turns.iter().cloned()),
            _ => return None,
        }
    }
    messages.push(message("user", Value::text(&request)));
    Some(Value::list(messages))
}
