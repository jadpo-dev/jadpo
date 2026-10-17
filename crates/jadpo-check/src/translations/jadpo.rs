//! Jadpo's own text (area 10, "Jadpo's and packages' text"): the messages
//! Jadpo writes itself, worded by what it ships (`en_gb` and `en_us`) and by
//! the application in the root `translations/<locale>.jadpo`, under
//! `jadpo: { }`. Only the messages of the features the application has on
//! count: they are listed by `jadpo translations builtin`, required in every
//! supported locale that is not in progress, and the only ones the app may
//! reword.
//!
//! The root file's wording is read and checked as a folder's is, against
//! Jadpo's own message types, which are data in `jadpo-std` and not
//! declarations in the project. A message is looked up in the app's root
//! wording for the locale, then in what Jadpo ships.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, File, Folder, Place, Project, Symbol};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_std::jadpo_text::{self, Feature, Message, OWNER, Shape};
use jadpo_std::translation::{ParamKind, Wording, parse_template};
use jadpo_std::{LOCALES, LocalePlan};

use super::check::{Analysis, Ctx, FailureWords, variant_name};
use super::declared::{Kind, Mode, ParamSpec, Subject, Subjects, VariantInfo};
use super::language::{LanguageFile, language_file};
use crate::locales::locales;

/// The APIs the project's routes belong to, in name order.
pub fn apis(db: &dyn Db, project: Project) -> Vec<String> {
    let mut out: Vec<String> = crate::routes::groups(db, project)
        .groups
        .iter()
        .map(|group| group.api.clone())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The features one API has on: what its routes can meet (area 7, area 8).
pub fn api_features(db: &dyn Db, project: Project, api: &str) -> BTreeSet<Feature> {
    use jadpo_std::AuthenticationMethod as M;
    let mut out = BTreeSet::new();
    let groups = crate::routes::groups(db, project);
    let routes: Vec<_> = groups
        .routes()
        .filter(|(group, _)| group.api == api)
        .map(|(_, route)| route)
        .collect();
    if routes.is_empty() {
        return out;
    }
    out.insert(Feature::Requests);
    let methods = crate::routes::apis::accepted_methods(db, project, api);
    let users = methods
        .iter()
        .any(|method| matches!(method, M::BrowserSession | M::ApiToken));
    let services = methods.contains(&M::ServiceKey);
    let browser = methods.contains(&M::BrowserSession);
    let callers: Vec<_> = routes
        .iter()
        .filter(|route| route.authentication.needs_caller())
        .collect();
    if callers.is_empty() {
        return out;
    }
    out.insert(Feature::SignIn);
    if users {
        out.insert(Feature::UserSignIn);
    }
    if services {
        out.insert(Feature::ServiceSignIn);
    }
    if browser
        && callers
            .iter()
            .any(|route| route.method != jadpo_std::HttpMethod::Get)
    {
        out.insert(Feature::BrowserSession);
    }
    out
}

/// The features the application has on: every API's, and the wording of
/// its locales when it declares them.
#[salsa::tracked(returns(ref))]
pub fn features(db: &dyn Db, project: Project) -> BTreeSet<Feature> {
    let mut out = BTreeSet::new();
    for api in apis(db, project) {
        out.extend(api_features(db, project, &api));
    }
    if locales(db, project).declared {
        out.insert(Feature::Locales);
    }
    out
}

/// The messages of the features the application has on, in catalogue order.
pub fn messages_on(db: &dyn Db, project: Project) -> Vec<&'static Message> {
    let on = features(db, project);
    jadpo_text::MESSAGES
        .iter()
        .filter(|message| on.contains(&message.feature))
        .collect()
}

/// The root `translations/` folder's language files, in path order.
pub fn root_files(db: &dyn Db, project: Project) -> Vec<File> {
    let mut found: Vec<File> = project
        .files(db)
        .iter()
        .copied()
        .filter(|&file| {
            let location = jadpo_db::location(db, project, file);
            location.place == Place::RootTranslations
                && !location.test
                && super::language::is_language_file(db, file)
        })
        .collect();
    found.sort_by(|a, b| a.path(db).cmp(b.path(db)));
    found
}

/// The root files, read.
pub fn root_language_files(db: &dyn Db, project: Project) -> Vec<&LanguageFile> {
    root_files(db, project)
        .into_iter()
        .filter_map(|file| language_file(db, project, file).as_ref())
        .collect()
}

/// The path of a locale's root language file, whether or not it exists.
pub fn root_path(tag: &str) -> String {
    format!("translations/{tag}.jadpo")
}

/// A catalogue message as a subject a wording is checked against.
fn subject(message: &Message) -> Subject {
    let kind = match message.kind {
        jadpo_text::Kind::Message => Kind::Message,
        jadpo_text::Kind::Failure => Kind::Failure,
        jadpo_text::Kind::Enum => Kind::Enum,
    };
    let placeholders: Vec<(String, ParamKind)> = message
        .placeholders()
        .map(|(name, kind)| (name.to_string(), kind))
        .collect();
    let (params, mode) = match message.shape {
        Shape::Variants(variants) => (
            vec![ParamSpec {
                name: "value".to_string(),
                kind: ParamKind::Choice,
                enum_type: Some(message.type_name.to_string()),
            }],
            Mode::Variants(
                variants
                    .iter()
                    .map(|(name, _)| VariantInfo {
                        name: (*name).to_string(),
                        payload: Vec::new(),
                    })
                    .collect(),
            ),
        ),
        Shape::Plain(_) => (
            message
                .fields
                .iter()
                .map(|(name, kind)| ParamSpec {
                    name: (*name).to_string(),
                    kind: *kind,
                    enum_type: None,
                })
                .collect(),
            Mode::Plain,
        ),
    };
    Subject {
        key: message.key.to_string(),
        type_name: if message.kind == jadpo_text::Kind::Enum {
            message.type_name.to_string()
        } else {
            message.key.to_string()
        },
        kind,
        symbol: Symbol {
            name: message.key.to_string(),
            kind: jadpo_db::ItemKind::Type(jadpo_db::Base::Message),
            public: true,
            path: String::new(),
            index: 0,
            library: None,
            ownership: jadpo_db::Ownership::Root,
            place: Place::Folder(Folder::Translations),
            test: false,
            transitions: Vec::new(),
            provided: None,
            scope: None,
            canonical: message.key.to_string(),
        },
        name_span: Span::new("", 0, 0),
        params,
        placeholders,
        mode,
        formats: Vec::new(),
        broken: false,
        builtin: Some(message.feature),
    }
}

/// Every message of Jadpo's as a subject, whether or not its feature is on.
pub fn subjects() -> Subjects {
    Subjects {
        list: jadpo_text::MESSAGES.iter().map(subject).collect(),
        problems: Vec::new(),
    }
}

/// What Jadpo ships for a message in a locale, as a wording.
pub fn shipped(message: &Message, tag: &str) -> Option<Wording> {
    if !message.ships_in(tag) {
        return None;
    }
    let template = |text: &str| parse_template(text).ok().map(|(template, _)| template);
    match message.shape {
        Shape::Plain(text) => template(text).map(Wording::Plain),
        Shape::Variants(variants) => variants
            .iter()
            .map(|(name, text)| Some(((*name).to_string(), template(text)?)))
            .collect::<Option<Vec<_>>>()
            .map(Wording::Variants),
    }
}

/// The text a shipped wording is compared as, the way a written member's
/// is (values in name order).
pub fn shipped_canonical(message: &Message) -> String {
    match message.shape {
        Shape::Plain(text) => text.to_string(),
        Shape::Variants(variants) => {
            let mut parts: Vec<String> = variants
                .iter()
                .map(|(name, text)| format!("{name}={text}"))
                .collect();
            parts.sort();
            parts.join("\u{1f}")
        }
    }
}

/// What the root files, checked, give.
pub fn analysis(db: &dyn Db, project: Project) -> &Analysis {
    analyse(db, project)
}

/// The root files checked against Jadpo's messages.
#[salsa::tracked(returns(ref))]
fn analyse(db: &dyn Db, project: Project) -> Analysis {
    let mut out = Analysis::default();
    let declared = locales(db, project);
    let files = root_language_files(db, project);
    for file in &files {
        out.problems.extend(file.problems.iter().cloned());
    }
    let Some(plan) = &declared.plan else {
        return out;
    };
    let on = features(db, project);
    let subjects = subjects();
    let reachable = BTreeSet::new();
    let mut ctx = Ctx {
        subjects: &subjects,
        plan,
        declared,
        reachable: &reachable,
        scope: OWNER,
        features: Some(on),
        found: BTreeMap::new(),
        problems: Vec::new(),
    };
    for file in &files {
        let Some((locale, in_progress)) = ctx.locale_of(file) else {
            continue;
        };
        let mut owners: BTreeMap<&str, Span> = BTreeMap::new();
        for group in &file.groups {
            if let Some(first) = owners.get(group.name.as_str()) {
                ctx.problems.push(
                    Diagnostic::new(
                        &rule::TRANSLATION_MEMBER_DUPLICATE,
                        format!("`{}` is written twice in `{}`", group.name, file.path),
                    )
                    .at(group.key_span.clone(), "written again")
                    .also(first.clone(), "first written here")
                    .reason("A group words one owner's text, once; the second would be dropped or the first, whichever a reader of the file believed.")
                    .next(format!("Merge the two `{}` groups", group.name)),
                );
                continue;
            }
            owners.insert(&group.name, group.key_span.clone());
            if group.name != OWNER {
                ctx.problems.push(owner_unknown(file, group));
                continue;
            }
            ctx.members(file, locale, in_progress, &group.members);
        }
    }
    out.problems.append(&mut ctx.problems);
    out.problems.extend(completeness(db, project, &files));
    // The wording of each failure Jadpo writes, in every supported locale
    // that has it: the app's own, then what Jadpo ships.
    let mut found = ctx.found;
    for message in jadpo_text::MESSAGES
        .iter()
        .filter(|message| message.kind == jadpo_text::Kind::Failure)
    {
        let mut by_locale = found.remove(message.key).unwrap_or_default();
        for &locale in &plan.supported {
            let tag = variant_name(locale);
            if by_locale.iter().any(|(at, _)| *at == locale) {
                continue;
            }
            if let Some(wording) = shipped(message, tag) {
                by_locale.push((locale, wording));
            }
        }
        out.failures.push(FailureWords {
            name: message.key.to_string(),
            code: message.key.to_string(),
            formats: Vec::new(),
            wordings: by_locale
                .into_iter()
                .filter_map(|(locale, wording)| match wording {
                    Wording::Plain(template) => Some((locale, template)),
                    _ => None,
                })
                .collect(),
        });
    }
    out
}

fn owner_unknown(file: &LanguageFile, group: &super::language::Group) -> Diagnostic {
    let near = nearest(&group.name, [OWNER]);
    let problem = Diagnostic::new(
        &rule::TRANSLATION_OWNER_UNKNOWN,
        format!("`{}` isn't an owner of text", group.name),
    )
    .at(group.key_span.clone(), "no such owner")
    .reason(format!(
        "The root file words the text the application did not write, grouped by whose it is: `jadpo` for Jadpo's own messages, or the name of a library package the application uses for that package's (library packages come with Phase 7).{}",
        near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
    ));
    match near {
        Some(near) => problem
            .next(format!("Rename the group to `{near}`"))
            .edit(group.key_span.clone(), near),
        None => problem.next(format!(
            "Remove the `{}` group from `{}`, or install the package that owns it",
            group.name, file.path
        )),
    }
}

/// Whether the application words, or Jadpo ships, a message in a locale.
fn worded(files: &[&LanguageFile], message: &Message, locale: u32) -> bool {
    message.ships_in(variant_name(locale))
        || files.iter().any(|file| {
            file.locale == Some(locale)
                && file
                    .group(OWNER)
                    .is_some_and(|group| group.member(message.key).is_some())
        })
}

/// The supported locales that are not in progress, with the messages of
/// the features that are on that neither Jadpo ships nor the application
/// words in them: what makes `jadpo check` fail.
pub fn needs(db: &dyn Db, project: Project) -> Vec<(u32, Vec<&'static Message>)> {
    let declared = locales(db, project);
    let Some(plan) = &declared.plan else {
        return Vec::new();
    };
    let files = root_language_files(db, project);
    let required = messages_on(db, project);
    plan.supported
        .iter()
        .copied()
        .filter(|&locale| {
            !(plan.in_progress.contains(&locale) || LocalePlan::is_pseudo(locale))
                || locale == plan.default
        })
        .filter_map(|locale| {
            let missing: Vec<&'static Message> = required
                .iter()
                .copied()
                .filter(|message| !worded(&files, message, locale))
                .collect();
            (!missing.is_empty()).then_some((locale, missing))
        })
        .collect()
}

/// One error for each supported locale that is not in progress and lacks
/// wording of Jadpo's messages.
fn completeness(db: &dyn Db, project: Project, files: &[&LanguageFile]) -> Vec<Diagnostic> {
    let declared = locales(db, project);
    let mut out = Vec::new();
    for (locale, missing) in needs(db, project) {
        let tag = variant_name(locale);
        let names: Vec<String> = missing
            .iter()
            .map(|message| format!("`{}`", message.key))
            .collect();
        let at = declared
            .entries
            .iter()
            .find(|(seen, _)| *seen == locale)
            .map(|(_, at)| at.clone())
            .or_else(|| declared.keyword.clone());
        let path = root_path(tag);
        let exists = files.iter().any(|file| file.locale == Some(locale));
        let mut problem = Diagnostic::new(
            &rule::TRANSLATION_JADPO_MISSING,
            format!("`Locale.{tag}` is supported but Jadpo's own messages aren't worded in it"),
        );
        if let Some(at) = at {
            problem = problem.at(
                at,
                if exists {
                    format!("{} to word in `{path}`", missing.len())
                } else {
                    format!("no `{path}`")
                },
            );
        }
        out.push(
            problem
                .reason(format!(
                    "Jadpo ships its own wording in {} and no other locale, so a client app or a person using `Locale.{tag}` would be shown English. An application words the messages of the features it has on itself, in the `jadpo: {{ }}` group of the root `translations/{tag}.jadpo`. Still to word: {}.",
                    crate::and_list(
                        &jadpo_text::SHIPPED
                            .iter()
                            .map(|tag| format!("`{tag}`"))
                            .collect::<Vec<_>>()
                    ),
                    crate::and_list(&names)
                ))
                .next(format!(
                    "Run `jadpo translations builtin` to list them with their fields, then word each under `jadpo: {{ }}` in `{path}`"
                )),
        );
    }
    out
}

/// A message Jadpo writes that a supported locale lacks, for `jadpo
/// translations missing`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JadpoGap {
    pub locale: u32,
    pub tag: String,
    pub message: &'static Message,
    pub path: String,
    pub file_exists: bool,
    pub in_progress: bool,
    /// The supported locales that word it, by the application or by what
    /// Jadpo ships.
    pub worded_in: Vec<u32>,
}

/// Every message of a feature that is on that a supported locale besides
/// the default lacks.
pub fn gaps(db: &dyn Db, project: Project) -> Vec<JadpoGap> {
    let declared = locales(db, project);
    let Some(plan) = &declared.plan else {
        return Vec::new();
    };
    let files = root_language_files(db, project);
    let required = messages_on(db, project);
    let mut out = Vec::new();
    for &locale in &plan.supported {
        if locale == plan.default || LocalePlan::is_pseudo(locale) {
            continue;
        }
        let tag = variant_name(locale);
        let exists = files.iter().any(|file| file.locale == Some(locale));
        for message in &required {
            if worded(&files, message, locale) {
                continue;
            }
            out.push(JadpoGap {
                locale,
                tag: tag.to_string(),
                message,
                path: root_path(tag),
                file_exists: exists,
                in_progress: plan.in_progress.contains(&locale),
                worded_in: plan
                    .supported
                    .iter()
                    .copied()
                    .filter(|&other| worded(&files, message, other))
                    .collect(),
            });
        }
    }
    out
}

/// The wording of Jadpo's failures each API's clients are sent: the
/// failures of the features that API has on.
pub fn failure_texts(
    db: &dyn Db,
    project: Project,
) -> BTreeMap<String, Vec<super::failures::FailureText>> {
    let analysis = analyse(db, project);
    let mut out = BTreeMap::new();
    for api in apis(db, project) {
        let on = api_features(db, project, &api);
        let mut texts: Vec<super::failures::FailureText> = analysis
            .failures
            .iter()
            .filter(|words| {
                jadpo_text::message_named(&words.code)
                    .is_some_and(|message| on.contains(&message.feature))
            })
            .map(|words| super::failures::FailureText {
                code: words.code.clone(),
                formats: words.formats.clone(),
                wordings: words.wordings.clone(),
            })
            .collect();
        texts.sort_by(|a, b| a.code.cmp(&b.code));
        out.insert(api, texts);
    }
    out
}

/// The member name a record and a list use for one of Jadpo's messages.
pub fn member_name(key: &str) -> String {
    format!("{OWNER}.{key}")
}

/// The locale tags a message is shipped in, for the listing.
pub fn shipped_in(message: &Message) -> Vec<&'static str> {
    match message.ships {
        jadpo_text::Ships::English => jadpo_text::SHIPPED.to_vec(),
        jadpo_text::Ships::Everywhere => LOCALES.iter().map(|(name, _)| *name).collect(),
    }
}
