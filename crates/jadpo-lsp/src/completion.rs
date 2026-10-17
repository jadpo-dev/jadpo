//! Completion (area 14, "Completion"): the items
//! `jadpo_check::completion` finds at the cursor, in the protocol's terms.
//! Each replaces what is typed there with its text, a call with its
//! required arguments as a snippet where the editor fills snippets in (its
//! name alone otherwise), and brings a `use` line with it where
//! the file doesn't import it yet. Those whose type the position requires
//! sort first. A standard type's variant is shown with the name its entry
//! gives people, worked out by the runtime's own `text.format` code.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Mutex, OnceLock};

use jadpo_check::completion::{self, Item, What};
use jadpo_std::standard::FormatKey;
use jadpo_value::{Value, Variant};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionList, CompletionParams, CompletionResponse,
    CompletionTextEdit, Documentation, InsertTextFormat, MarkupContent, MarkupKind, Range,
    TextEdit,
};

use crate::convert::{self, Encoding};
use crate::server::Server;

/// What starts completion as it is typed, besides a word: a dot for
/// members, a colon for an argument's or a field's value, and a digit for
/// a number's units.
pub const TRIGGERS: [&str; 12] = [".", ":", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];

/// The locale a standard value's name for people is shown in.
const READER: &str = "en-GB";

impl Server {
    pub(crate) fn completion(&self, params: &CompletionParams) -> Option<CompletionResponse> {
        let position = &params.text_document_position;
        let loaded = self.loaded()?;
        let file = loaded.file(&self.relative(&position.text_document.uri)?)?;
        let text = file.text(&loaded.db).clone();
        let offset = convert::offset(&text, position.position, self.encoding())?;
        let found = completion::complete(&loaded.db, loaded.project, file, offset)?;
        let range = convert::range(&text, found.start, found.end, self.encoding());
        let items = found
            .items
            .into_iter()
            .map(|mut item| {
                if item.snippet && !self.snippets {
                    item.insert = None;
                    item.snippet = false;
                }
                protocol_item(item, range, &text, self.encoding())
            })
            .collect();
        Some(CompletionResponse::List(CompletionList {
            is_incomplete: false,
            items,
        }))
    }
}

fn protocol_item(item: Item, range: Range, text: &str, encoding: Encoding) -> CompletionItem {
    let shown = item.variant_of.as_deref().and_then(|ty| {
        let variant = item.label.rsplit('.').next().unwrap_or(&item.label);
        variant_name(ty, variant)
    });
    let new_text = item.insert.clone().unwrap_or_else(|| item.label.clone());
    CompletionItem {
        label: item.label.clone(),
        kind: Some(kind(item.what)),
        detail: shown.or(item.detail),
        documentation: item.docs.map(|value| {
            Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            })
        }),
        sort_text: Some(format!(
            "{}{:02}{}",
            u8::from(!item.fits),
            item.what as u8,
            item.label
        )),
        filter_text: Some(item.label.clone()),
        text_edit: Some(CompletionTextEdit::Edit(TextEdit { range, new_text })),
        insert_text_format: Some(if item.snippet {
            InsertTextFormat::SNIPPET
        } else {
            InsertTextFormat::PLAIN_TEXT
        }),
        additional_text_edits: item.import.map(|(start, end, new_text)| {
            vec![TextEdit {
                range: convert::range(text, start, end, encoding),
                new_text,
            }]
        }),
        ..CompletionItem::default()
    }
}

fn kind(what: What) -> CompletionItemKind {
    match what {
        What::Argument | What::Field => CompletionItemKind::FIELD,
        What::Parameter | What::Local => CompletionItemKind::VARIABLE,
        What::Variant => CompletionItemKind::ENUM_MEMBER,
        What::Unit => CompletionItemKind::UNIT,
        What::Rule => CompletionItemKind::REFERENCE,
        What::Setting => CompletionItemKind::PROPERTY,
        What::Message => CompletionItemKind::TEXT,
        What::Constant => CompletionItemKind::CONSTANT,
        What::Function | What::Action | What::Operation => CompletionItemKind::FUNCTION,
        What::Library | What::Domain | What::Namespace => CompletionItemKind::MODULE,
        What::Type => CompletionItemKind::CLASS,
        What::Keyword => CompletionItemKind::KEYWORD,
    }
}

/// Each standard variant's name for people, by its type and its name.
type Shown = HashMap<(String, String), Option<String>>;

/// How a standard enum's variant is shown: the name its entry's format
/// gives people (`Locale.en_gb` is "British English", `Zone.europe_london`
/// "UK Time"), or its code where people see the code; `None` for a
/// project's enum. Worked out once per variant, since the zones alone are
/// hundreds.
fn variant_name(ty: &str, variant: &str) -> Option<String> {
    static NAMES: OnceLock<Mutex<Shown>> = OnceLock::new();
    let names = NAMES.get_or_init(Mutex::default);
    let Ok(mut names) = names.lock() else {
        return shown(ty, variant);
    };
    names
        .entry((ty.to_string(), variant.to_string()))
        .or_insert_with(|| shown(ty, variant))
        .clone()
}

fn shown(ty: &str, variant: &str) -> Option<String> {
    let entry = jadpo_std::standard_type(ty)?;
    let ordinal = jadpo_std::variant_ordinal(ty, variant)?;
    let value = Value::Variant(Rc::new(Variant {
        ty: ty.into(),
        ordinal,
        name: variant.into(),
        fields: Vec::new(),
    }));
    if entry.format.key == FormatKey::Canonical {
        return jadpo_value::standard::canonical(entry, &value).ok();
    }
    jadpo_value::standard::shown(entry, &value, READER, &Value::None, &Value::None).ok()
}
