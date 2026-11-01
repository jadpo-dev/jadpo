//! `search: { fields: language: }` on an entity (area 16, "Search"; chunk
//! B12): the fields people search its records by words in, listed first
//! counting most, and the language their words are read in. PostgreSQL
//! keeps the words in a generated `tsvector` column with a GIN index,
//! which the schema adds ([`super::table_of`]); a query finds records with
//! `where: Entity matches phrase` ([`super::queries`]).
//!
//! `store:`, an engine's connection instead of PostgreSQL, isn't built:
//! it is `toolchain.not_built_yet` where it is written. So is a field of a
//! sensitive category (area 15): a search reads what the field says, and
//! every read of a sensitive field is to be recorded in the access
//! records, which aren't built yet. Any other personal field may be
//! searched: its words live in the record's own row, worked out from the
//! field, so whatever clears or deletes the field clears its words too.

use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_std::SearchLanguage;
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::{EntityModel, path_names};
use crate::tree::{self, node_span};
use crate::types::{Prim, Ty, TypeView};

/// An entity's search, as the schema, its queries and the plan read it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SearchModel {
    /// The fields its words come from, by name, in the order written:
    /// the first weighs most.
    pub fields: Vec<String>,
    pub language: SearchLanguage,
}

/// The settings `search:` takes.
const SETTINGS: [&str; 3] = ["fields", "language", "store"];

const TAKES: &str = "`search:` lists the text fields people search its records by, `fields: [title, notes]`, the first counting most, and the language their words are in, `language: SearchLanguage.english`.";

/// `search:` as written, read once every field is: `None` with each
/// problem reported, so nothing searches what it couldn't read.
pub(super) fn read(
    lookup: &dyn TypeView,
    path: &str,
    entity: &EntityModel,
    sensitive: &[(String, String)],
    member: &ast::Member,
    key_span: &Span,
    out: &mut Vec<Diagnostic>,
) -> Option<SearchModel> {
    let name = &entity.name;
    let invalid = |span: Span, title: String, label: &str, reason: String, next: String| {
        Diagnostic::new(&rule::SEARCH_SETTINGS_INVALID, title)
            .at(span, label.to_string())
            .reason(reason)
            .next(next)
    };
    let Some(block) = member.block() else {
        out.push(invalid(
            member
                .value()
                .map_or_else(|| key_span.clone(), |v| node_span(path, v.syntax())),
            format!("`{name}`'s `search:` isn't a block"),
            "not a block",
            TAKES.to_string(),
            "Write `search: { fields: [...] language: SearchLanguage.english }`".to_string(),
        ));
        return None;
    };
    let members = tree::members(Some(block));
    let mut problems = false;
    let mut fields_member = None;
    let mut language_member = None;
    for (key, setting) in &members {
        let Some(word) = tree::name_text(key) else {
            continue;
        };
        let at = node_span(path, key.syntax());
        match word.as_str() {
            "fields" => fields_member = Some((at, setting.clone())),
            "language" => language_member = Some((at, setting.clone())),
            // An engine's connection (area 16, "An engine instead of
            // PostgreSQL"): decided, not built.
            "store" => {
                out.push(
                    Diagnostic::new(
                        &rule::TOOLCHAIN_NOT_BUILT_YET,
                        format!("`{name}`'s search in an engine isn't built yet"),
                    )
                    .at(at, "decided, not built")
                    .reason("`store:` names a connection whose package provides `search`, an engine such as Meilisearch, Typesense, Algolia or OpenSearch, in place of PostgreSQL. That isn't built yet: accepted, the records would never reach the engine.")
                    .next("Leave `store:` out, and the search is PostgreSQL's"),
                );
                problems = true;
            }
            other => {
                let near = nearest(other, SETTINGS.iter().copied());
                let mut problem = invalid(
                    at.clone(),
                    format!("`{other}` isn't something `search:` holds"),
                    "not a setting",
                    TAKES.to_string(),
                    format!("Remove `{other}`"),
                );
                if let Some(near) = near {
                    problem = invalid(
                        at.clone(),
                        format!("`{other}` isn't something `search:` holds"),
                        "not a setting",
                        format!("{TAKES} Did you mean `{near}:`?"),
                        format!("Use `{near}:`"),
                    )
                    .edit(at, near);
                }
                out.push(problem);
                problems = true;
            }
        }
    }
    // The fields, each a text field of the entity's, once.
    let mut fields: Vec<String> = Vec::new();
    match &fields_member {
        None => {
            out.push(invalid(
                key_span.clone(),
                format!("`{name}`'s `search:` names no fields"),
                "no `fields:`",
                TAKES.to_string(),
                "Add `fields: [...]`, the text fields people search by".to_string(),
            ));
            problems = true;
        }
        Some((at, setting)) => {
            let items: Option<Vec<Expr>> = match setting.value() {
                Some(Expr::ListExpr(list)) => Some(list.exprs().collect()),
                _ => None,
            };
            match items {
                Some(items) if !items.is_empty() => {
                    for item in items {
                        let span = node_span(path, item.syntax());
                        let written = tree::written(item.syntax());
                        let named = match &item {
                            Expr::NameRef(name) => tree::name_ref_text(name),
                            _ => None,
                        };
                        let Some(field_name) = named else {
                            out.push(invalid(
                                span,
                                format!("`{written}` isn't one of `{name}`'s fields"),
                                "not a field's name",
                                format!("{TAKES} Each field is named as the entity names it, `title`, not `{name}.title`."),
                                "Write the field's name alone".to_string(),
                            ));
                            problems = true;
                            continue;
                        };
                        let Some(field) = entity.fields.iter().find(|f| f.name == field_name)
                        else {
                            let near =
                                nearest(&field_name, entity.fields.iter().map(|f| f.name.as_str()));
                            let mut problem = invalid(
                                span.clone(),
                                format!("`{name}` has no field `{field_name}`"),
                                "no such field",
                                TAKES.to_string(),
                                format!("Name one of `{name}`'s text fields"),
                            );
                            if let Some(near) = near {
                                problem = problem.next(format!("Use `{near}`")).edit(span, near);
                            }
                            out.push(problem);
                            problems = true;
                            continue;
                        };
                        if fields.contains(&field.name) {
                            out.push(invalid(
                                span,
                                format!("`{field_name}` is in `fields:` twice"),
                                "again",
                                "Each field's words are kept once, weighted by where it is first listed.".to_string(),
                                format!("Remove the second `{field_name}`"),
                            ));
                            problems = true;
                            continue;
                        }
                        let ty = Ty::Field(name.clone(), vec![field.name.clone()]);
                        let text = field.sql_type.as_deref() == Some("text")
                            && field.json.is_none()
                            && field.compound.is_none()
                            && lookup.representation(&ty) == Some(Prim::Text);
                        if !text {
                            out.push(invalid(
                                span,
                                format!("`{name}.{field_name}` isn't text, so it can't be searched by words"),
                                &format!("`{}`", field.ty),
                                format!(
                                    "A search reads the words of text fields: a type built on `Text`, kept in one text column. `{name}.{field_name}` is `{}`.",
                                    field.ty
                                ),
                                format!("Remove `{field_name}` from `fields:`"),
                            ));
                            problems = true;
                            continue;
                        }
                        // Marked sensitive, or a standard type that is
                        // sensitive data whatever holds it (area 19).
                        let category = sensitive
                            .iter()
                            .find(|(field, _)| *field == field_name)
                            .map(|(_, category)| category.clone())
                            .or_else(|| {
                                lookup.rule_chain(&ty).iter().find_map(|(held, _)| {
                                    crate::ai_personal::standard_category(held)
                                        .filter(|c| crate::ai_personal::is_sensitive(c))
                                        .map(str::to_string)
                                })
                            });
                        if let Some(category) = category {
                            out.push(
                                Diagnostic::new(
                                    &rule::TOOLCHAIN_NOT_BUILT_YET,
                                    format!("Searching `{name}.{field_name}`, `{category}` data, isn't built yet"),
                                )
                                .at(span, format!("`PersonalData.{category}`, a sensitive category"))
                                .reason(format!(
                                    "Every read of a sensitive field is recorded in the access records (area 15, \"Sensitive fields\"), and a search reads what `{field_name}` says. The access records aren't built yet, so this compiler can't record it."
                                ))
                                .next(format!("Remove `{field_name}` from `fields:` for now")),
                            );
                            problems = true;
                            continue;
                        }
                        fields.push(field.name.clone());
                    }
                }
                _ => {
                    out.push(invalid(
                        setting
                            .value()
                            .map_or_else(|| at.clone(), |v| node_span(path, v.syntax())),
                        format!("`{name}`'s `search:` `fields:` isn't a list of its fields"),
                        "not a list of fields",
                        TAKES.to_string(),
                        "List the text fields people search by, such as `fields: [title]`"
                            .to_string(),
                    ));
                    problems = true;
                }
            }
        }
    }
    // The language.
    let languages = || {
        SearchLanguage::NAMES
            .iter()
            .map(|n| format!("`SearchLanguage.{n}`"))
            .collect::<Vec<_>>()
    };
    let language = match &language_member {
        None => {
            out.push(invalid(
                key_span.clone(),
                format!("`{name}`'s `search:` doesn't say what language its words are in"),
                "no `language:`",
                "Words are found by their stem in a language (\"running\" finds \"runs\" in English), and stop words such as \"the\" are left out, so the search needs the language its fields are written in; `SearchLanguage.simple` stems nothing and drops nothing.".to_string(),
                "Add `language: SearchLanguage.english`, or the language its fields are in"
                    .to_string(),
            ));
            problems = true;
            None
        }
        Some((at, setting)) => {
            let value = setting.value();
            let names = value.as_ref().map(path_names).unwrap_or_default();
            match names.as_slice() {
                [host, variant] if host == SearchLanguage::NAME => {
                    match SearchLanguage::from_name(variant) {
                        Some(language) => Some(language),
                        None => {
                            let span =
                                value.map_or_else(|| at.clone(), |v| node_span(path, v.syntax()));
                            let near = nearest(variant, SearchLanguage::NAMES.iter().copied());
                            let mut problem = invalid(
                                span.clone(),
                                format!(
                                    "`SearchLanguage.{variant}` isn't a language a search reads"
                                ),
                                "not a `SearchLanguage`",
                                format!(
                                    "A search reads its words in one of PostgreSQL's languages: {}.",
                                    crate::and_list(&languages())
                                ),
                                "Name one of `SearchLanguage`'s languages".to_string(),
                            );
                            if let Some(near) = near {
                                problem = problem
                                    .next(format!("Use `SearchLanguage.{near}`"))
                                    .edit(span, format!("SearchLanguage.{near}"));
                            }
                            out.push(problem);
                            problems = true;
                            None
                        }
                    }
                }
                _ => {
                    out.push(invalid(
                        value.map_or_else(|| at.clone(), |v| node_span(path, v.syntax())),
                        format!("`{name}`'s `search:` `language:` isn't a `SearchLanguage`"),
                        "not a `SearchLanguage`",
                        format!(
                            "`language:` is one of `SearchLanguage`'s languages, PostgreSQL's: {}.",
                            crate::and_list(&languages())
                        ),
                        "Write `language: SearchLanguage.english`, or the language its fields are in".to_string(),
                    ));
                    problems = true;
                    None
                }
            }
        }
    };
    // The column the words are kept in is the runtime's.
    if let Some(field) = entity
        .fields
        .iter()
        .find(|f| f.born == jadpo_std::search::COLUMN)
    {
        out.push(invalid(
            key_span.clone(),
            format!(
                "`{name}` has a field `{}`, the column its searchable words are kept in",
                field.name
            ),
            "the words' column",
            format!(
                "An entity with `search:` keeps its searchable words in a column called `{}`, so no field may be stored under that name.",
                jadpo_std::search::COLUMN
            ),
            format!("Rename `{}`", field.name),
        ));
        problems = true;
    }
    if problems {
        return None;
    }
    Some(SearchModel {
        fields,
        language: language?,
    })
}
