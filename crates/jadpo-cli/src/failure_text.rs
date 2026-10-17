//! The text client apps show for failures (area 10, "Failures for client
//! apps"), written beside each API's OpenAPI document by `jadpo check`:
//! `build/openapi/<api>.failures.<locale>.json` for each supported locale,
//! mapping the code of every failure a route of the API can answer with
//! to `text`, in the wording's `{field}` syntax, and `formats`, which
//! placeholders the client formats and how.
//!
//! A locale whose translations are in progress is sent what the runtime
//! would write for it, its regional sibling's wording and then the
//! default's ([`jadpo_value::locale::fallback_chain`]), so a client always
//! has text for every code. An API no failure of which is worded gets no
//! file.

use jadpo_check::translations::failures::{FailureText, texts};
use jadpo_db::{Db, Project};
use jadpo_graph::json::Value;

/// The documents, named `<api>.failures.<locale>` for the OpenAPI folder.
pub fn documents(db: &dyn Db, project: Project) -> Vec<(String, Value)> {
    let Some(plan) = jadpo_check::locales::locales(db, project).plan.as_ref() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (api, failures) in texts(db, project) {
        if failures.is_empty() {
            continue;
        }
        for &locale in &plan.supported {
            let Some((tag, _)) = jadpo_std::LOCALES.get(locale as usize) else {
                continue;
            };
            let chain = jadpo_value::locale::fallback_chain(locale, plan);
            let mut document = Value::object();
            for failure in &failures {
                if let Some(entry) = entry(failure, &chain) {
                    document.set(&failure.code, entry);
                }
            }
            out.push((format!("{api}.failures.{tag}"), document));
        }
    }
    out
}

/// One failure's entry: the first wording along the fallback chain.
fn entry(failure: &FailureText, chain: &[u32]) -> Option<Value> {
    let template = chain.iter().find_map(|at| {
        failure
            .wordings
            .iter()
            .find(|(locale, _)| locale == at)
            .map(|(_, template)| template)
    })?;
    let mut formats = Value::object();
    let written: Vec<&str> = template.fields().collect();
    for (name, format) in &failure.formats {
        if written.contains(&name.as_str()) {
            formats.set(name, format.word());
        }
    }
    let mut entry = Value::object();
    entry.set("text", template.written());
    entry.set("formats", formats);
    Some(entry)
}
