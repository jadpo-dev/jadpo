//! Area 10's settings: `config { }` in `config.jadpo` declares each setting
//! the application needs, as `name: Type { secret: true, default: ...,
//! binding: "NAME", test: ... }`, and code reads it as `config.<name>`.
//!
//! Read once per project ([`config`]) into a [`Config`], which the bodies
//! (the type of `config.<name>`), the connections' addresses, `jadpo-plan`
//! (the runtime's [`jadpo_std::config::ConfigPlan`]) and the tooling that
//! lists or sets values read. A setting's variable is its name in capitals
//! unless `binding:` says otherwise; its value is checked against its type
//! when the application starts, and a `default:` and a `test:` value are
//! literals checked here.
//!
//! A secret is accepted only at places the compiler owns (a connection's
//! `url:`, an integration's `credential:` and a `signing_key:` in
//! `application { }` and `authentication { }`): [`use_problems`] reports
//! every other read as `configuration.secret_flow`, and a `config.<name>`
//! that names no setting as `configuration.field_unknown`.

use std::sync::Arc;

use jadpo_db::{Db, File, ItemKind, Project, item_tree};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_std::config::{TestSecret, derived_variable, is_variable_name, previous_variable};
use jadpo_syntax::ast::{self, AstNode, Expr, MemberItem};

use crate::data::ConnectionKind;
use crate::lookups::{Lookup, Name};
use crate::tree::{self, dotted, name_text, node_span, written};
use crate::types::literal::{self, Problem};
use crate::types::{
    Callable, Const, Def, Fact, Prim, Ty, TypeRef, TypeView, body_facts, callables_of, resolve,
    typed_body,
};

/// What a setting's block may hold.
const OPTIONS: [&str; 4] = ["secret", "default", "binding", "test"];

/// The members of `application { }` and `authentication { }` whose value
/// may be a secret: the places the compiler itself owns (area 10,
/// "Secrets"). A connection's address, an integration's credential (and a
/// webhook's `secret:`), and a signing key.
pub const SECRET_SINKS: [&str; 4] = ["url", "credential", "signing_key", "secret"];

/// How a setting's value is read from its variable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Reads {
    Text,
    Number,
    Bool,
    Uuid,
    Duration,
    Instant,
    Date,
    /// A declared enum with no data in its variants.
    Enum,
    Zone,
    Locale,
}

/// The word `config { }` is declared with and its settings are read
/// through: `config.<name>`.
pub const CONFIG: &str = "config";

/// One setting, as `config { }` declares it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Setting {
    pub name: String,
    /// The type as the setting's code reads it.
    pub ty: Ty,
    /// How its text is read.
    pub reads: Reads,
    /// Marked `secret: true`, or of a type that is always secret.
    pub secret: bool,
    /// The environment variable it is read from.
    pub variable: String,
    /// Its `default:`, in the spelling a variable has.
    pub default: Option<String>,
    /// Its `test:` value, in the spelling a variable has.
    pub test: Option<String>,
    /// What `jadpo test` makes for a secret.
    pub generated: Option<TestSecret>,
    /// A signing key, which a `_PREVIOUS` variable may also hold.
    pub rotates: bool,
}

/// Area 10's settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Config {
    /// Whether `config { }` is written at all.
    pub declared: bool,
    /// Each setting that read cleanly, in written order.
    pub settings: Vec<Setting>,
    /// Every setting name written, read cleanly or not, so a read of one
    /// that failed is not also reported as unknown.
    pub written: Vec<String>,
    pub problems: Vec<Diagnostic>,
}

impl Config {
    pub fn setting(&self, name: &str) -> Option<&Setting> {
        self.settings.iter().find(|setting| setting.name == name)
    }
}

/// The settings of `project`.
#[salsa::tracked(returns(ref))]
pub fn config(db: &dyn Db, project: Project) -> Arc<Config> {
    let mut reader = Reader {
        db,
        project,
        lookup: Lookup::new(db, project),
        out: Config::default(),
    };
    reader.read();
    Arc::new(reader.out)
}

/// What a read of setting `name` needs to know of it: its type and
/// whether it is secret. `None` when no setting has the name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SettingFact {
    pub ty: Ty,
    pub secret: bool,
}

/// The type and secrecy of setting `name`, so a body or a file that reads
/// it depends on that setting alone.
#[salsa::tracked(returns(ref))]
pub fn setting_fact<'db>(
    db: &'db dyn Db,
    project: Project,
    name: Name<'db>,
) -> Option<SettingFact> {
    let config = config(db, project);
    let text = name.text(db);
    match config.setting(text) {
        Some(setting) => Some(SettingFact {
            ty: setting.ty.clone(),
            secret: setting.secret,
        }),
        // A setting that was written but failed to read is a mistake
        // already reported: reading it gives a type that absorbs checks.
        None => config
            .written
            .iter()
            .any(|w| w == text)
            .then_some(SettingFact {
                ty: Ty::Error,
                secret: false,
            }),
    }
}

/// Whether a setting can hold a value of type `ty` (area 10, "What a
/// setting can hold"), for `jadpo config move`: `Err` with why not. A
/// secret type is refused too, since a secret has no `default:`.
pub fn holds(db: &dyn Db, project: Project, ty: &Ty) -> Result<(), String> {
    let reader = Reader {
        db,
        project,
        lookup: Lookup::new(db, project),
        out: Config::default(),
    };
    reader.classify(ty)?;
    if let Some(base) = reader.lookup.base_prim(ty).filter(|base| base.is_secret()) {
        return Err(format!(
            "A `{}` is secret, and a secret's value comes only from the deployment's environment, never from a `default:`.",
            base.name()
        ));
    }
    Ok(())
}

/// The problems in `config { }` itself.
pub fn config_problems(db: &dyn Db, project: Project) -> &Vec<Diagnostic> {
    config_diagnostics(db, project)
}

#[salsa::tracked(returns(ref))]
fn config_diagnostics(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    config(db, project).problems.clone()
}

// --- reading ---------------------------------------------------------------

/// One singleton as written: its file, its keyword's span and its block.
pub(crate) struct Found {
    pub(crate) path: String,
    pub(crate) keyword: Span,
    pub(crate) list: Option<ast::MemberList>,
}

struct Reader<'a> {
    db: &'a dyn Db,
    project: Project,
    lookup: Lookup<'a>,
    out: Config,
}

impl Reader<'_> {
    fn read(&mut self) {
        let mut all = singletons(self.db, self.project, "config").into_iter();
        let Some(first) = all.next() else { return };
        self.out.declared = true;
        for other in all {
            self.out.problems.push(
                Diagnostic::new(
                    &rule::CONFIGURATION_MULTIPLE_DECLARATIONS,
                    "`config { }` is declared twice",
                )
                .at(other.keyword.clone(), "declared again")
                .also(first.keyword.clone(), "first declared here")
                .reason("Every setting the application needs is a field of the one `config { }`, in `config.jadpo`, so one place lists everything a deployment must provide.")
                .next("Move the settings this `config` holds into the first, and remove it"),
            );
        }
        let path = first.path.clone();
        // The variables claimed so far, with the setting that took each.
        let mut claimed: Vec<(String, String, Span)> = Vec::new();
        for item in first.list.iter().flat_map(|list| list.member_items()) {
            match item {
                MemberItem::Elision(_) => {}
                MemberItem::Member(member) => {
                    let Some(key) = member.member_key() else {
                        continue;
                    };
                    let mut parts = key.names();
                    let (Some(name), None) = (parts.next(), parts.next()) else {
                        self.expected_field(&path, key.syntax());
                        continue;
                    };
                    let Some(text) = name_text(&name) else {
                        continue;
                    };
                    let span = node_span(&path, name.syntax());
                    self.out.written.push(text.clone());
                    if let Some(setting) = self.setting(&path, &text, &span, &member, &mut claimed)
                    {
                        self.out.settings.push(setting);
                    }
                }
                other => self.expected_field(&path, other.syntax()),
            }
        }
    }

    fn expected_field(&mut self, path: &str, node: &jadpo_syntax::SyntaxNode) {
        let text = written(node);
        self.out.problems.push(
            Diagnostic::new(
                &rule::CONFIGURATION_EXPECTED_FIELD,
                format!("`{text}` isn't a setting"),
            )
            .at(node_span(path, node), "not `name: Type`")
            .reason("`config { }` holds settings, each written `name: Type`, with a block after the type when it has `secret:`, `default:`, `binding:` or `test:`.")
            .next("Write the setting as `name: Type`, or remove this line"),
        );
    }

    /// One setting; `None` when it has a problem, which is reported.
    fn setting(
        &mut self,
        path: &str,
        name: &str,
        span: &Span,
        member: &ast::Member,
        claimed: &mut Vec<(String, String, Span)>,
    ) -> Option<Setting> {
        let Some(value) = member.value() else {
            self.out.problems.push(
                Diagnostic::new(
                    &rule::CONFIGURATION_EXPECTED_FIELD,
                    format!("`{name}:` has no type"),
                )
                .at(span.clone(), "no type")
                .reason("A setting says what its value must be, so the application can refuse to start with a value that isn't: `name: Type`.")
                .next(format!("Write the type after `{name}:`, such as `{name}: Text`")),
            );
            return None;
        };
        // `Email { binding: "MAIL" }` is read as a record whose fields are
        // the options; `Email` alone has none.
        let (value, options) = match &value {
            Expr::RecordExpr(record) => match record.expr() {
                Some(inner) => (inner, record.member_list()),
                None => (value.clone(), None),
            },
            other => (other.clone(), member.block()),
        };
        let type_span = node_span(path, value.syntax());
        let ty_text = written(value.syntax());
        let mut ok = true;
        let ty = match self.lookup.resolve(&TypeRef::from_expr(&value)) {
            // An unknown type name is reported by resolution.
            Ty::Error => return None,
            ty => ty,
        };
        let secret_by_type = self.lookup.base_prim(&ty).is_some_and(Prim::is_secret);
        let reads = match self.classify(&ty) {
            Ok(reads) => Some(reads),
            Err(why) => {
                ok = false;
                self.out.problems.push(
                    Diagnostic::new(
                        &rule::CONFIGURATION_TYPE_UNSUPPORTED,
                        format!("`{name}` can't be a `{ty_text}`"),
                    )
                    .at(type_span.clone(), "not read from one variable")
                    .reason(format!(
                        "A setting is read from one environment variable, so its type must be decodable from one piece of text: text-based types, `Int`, `Decimal`, `Bool`, `Uuid`, `Email`, `Url`, `IpAddress`, `Duration`, `Instant`, `CalendarDate` and enums. {why}"
                    ))
                    .next(format!("Split `{name}` into settings of those types")),
                );
                None
            }
        };
        // Options.
        let mut secret: Option<bool> = None;
        let mut binding: Option<(String, Span, Span)> = None;
        let mut default: Option<(Span, ast::Member)> = None;
        let mut test: Option<(Span, ast::Member)> = None;
        let mut seen: Vec<(String, Span)> = Vec::new();
        for (key, option) in tree::members(options) {
            let Some(word) = name_text(&key) else {
                continue;
            };
            let key_span = node_span(path, key.syntax());
            if !OPTIONS.contains(&word.as_str()) {
                ok = false;
                self.unknown_option(name, &word, &key_span);
                continue;
            }
            if let Some((_, first)) = seen.iter().find(|(seen, _)| *seen == word) {
                ok = false;
                self.out.problems.push(
                    Diagnostic::new(
                        &rule::CONFIGURATION_DUPLICATE_OPTION,
                        format!("`{word}:` is given twice on `{name}`"),
                    )
                    .at(key_span.clone(), "given again")
                    .also(first.clone(), "first given here")
                    .reason("A setting says each of `secret:`, `default:`, `binding:` and `test:` once, so there is one answer to read.")
                    .next(format!("Remove one `{word}:`")),
                );
                continue;
            }
            seen.push((word.clone(), key_span.clone()));
            match word.as_str() {
                "secret" => match option.value().map(|v| Const::from_expr(&v)) {
                    Some(Const::Bool(flag)) => secret = Some(flag),
                    _ => {
                        ok = false;
                        let at = option
                            .value()
                            .map_or_else(|| key_span.clone(), |v| node_span(path, v.syntax()));
                        self.out.problems.push(
                            Diagnostic::new(
                                &rule::CONFIGURATION_SECRET_INVALID,
                                format!("`secret:` on `{name}` isn't `true` or `false`"),
                            )
                            .at(at, "not a flag")
                            .reason("`secret: true` marks a value that must never leave the server; the setting is either secret or it is not.")
                            .next("Write `secret: true`, or leave `secret:` out"),
                        );
                    }
                },
                "binding" => {
                    if let Some(read) = self.binding(path, &key_span, &option) {
                        binding = Some(read);
                    } else {
                        ok = false;
                    }
                }
                "default" => default = Some((key_span, option)),
                _ => test = Some((key_span, option)),
            }
        }
        let is_secret = secret.unwrap_or(false) || secret_by_type;
        // A secret is text, which a test run can make.
        if is_secret && let Some(reads) = reads {
            let text_like = reads == Reads::Text;
            if !text_like {
                ok = false;
                self.out.problems.push(
                    Diagnostic::new(
                        &rule::CONFIGURATION_TYPE_UNSUPPORTED,
                        format!("`{name}` is secret, and `{ty_text}` isn't text"),
                    )
                    .at(type_span.clone(), "a secret that isn't text")
                    .reason("A secret is a key, a token or an address, which is text; `jadpo test` generates a throwaway one for every run, and cannot make a number or a flag.")
                    .next(format!("Make `{name}` text, or leave out `secret: true`")),
                );
            }
        }
        // The variable.
        let variable = match &binding {
            Some((variable, _, _)) => variable.clone(),
            None => derived_variable(name),
        };
        let variable_span = binding
            .as_ref()
            .map_or_else(|| span.clone(), |(_, _, at)| at.clone());
        let mut claims = vec![variable.clone()];
        if is_secret && self.lookup.base_prim(&ty) == Some(Prim::SigningKey) {
            claims.push(previous_variable(&variable));
        }
        for claim in claims {
            match claimed.iter().find(|(var, _, _)| *var == claim) {
                Some((_, other, first)) => {
                    ok = false;
                    self.out.problems.push(
                        Diagnostic::new(
                            &rule::CONFIGURATION_DUPLICATE_BINDING,
                            format!("`{name}` and `{other}` both read `{claim}`"),
                        )
                        .at(variable_span.clone(), "the same variable")
                        .also(first.clone(), "first read here")
                        .reason("Each setting has its own environment variable; two settings sharing one would always hold the same value, and renaming either would change both.")
                        .next(format!("Give `{name}` a variable of its own with `binding:`, or merge the two settings")),
                    );
                }
                None => claimed.push((claim, name.to_string(), variable_span.clone())),
            }
        }
        // Values written in source.
        let given = |this: &mut Self, option: Option<(Span, ast::Member)>, word: &str| {
            let (key_span, option) = option?;
            if is_secret {
                this.out.problems.push(
                    Diagnostic::new(
                        &rule::CONFIGURATION_SECRET_DEFAULT,
                        format!("`{name}` is secret, and has a `{word}:` in source"),
                    )
                    .at(key_span, "a secret written in source")
                    .reason(format!(
                        "A secret's value comes only from the deployment's environment, so a forgotten production secret stops the app instead of falling back to a value written in source; `jadpo test` generates its own for a test run, so it needs no `{word}:` either."
                    ))
                    .next(format!("Remove `{word}:` from `{name}`")),
                );
                return None;
            }
            let reads = reads?;
            this.given(path, name, word, &ty, reads, &key_span, &option)
        };
        let default = given(self, default, "default");
        let test = given(self, test, "test");
        let (Some(reads), true) = (reads, ok) else {
            return None;
        };
        let generated = is_secret.then(|| self.generated(&ty)).flatten();
        let rotates = self.lookup.base_prim(&ty) == Some(Prim::SigningKey);
        Some(Setting {
            name: name.to_string(),
            ty,
            reads,
            secret: is_secret,
            variable,
            default: default.flatten(),
            test: test.flatten(),
            generated,
            rotates,
        })
    }

    fn unknown_option(&mut self, setting: &str, word: &str, span: &Span) {
        let near = nearest(word, OPTIONS);
        let hint = if word == "required" {
            " A setting is required unless it has a `default:`; there are no optional settings, so give a default or leave the setting required.".to_string()
        } else {
            near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
        };
        let problem = Diagnostic::new(
            &rule::CONFIGURATION_UNKNOWN_OPTION,
            format!("`{word}` isn't an option a setting takes"),
        )
        .at(span.clone(), "unknown option")
        .reason(format!(
            "`{setting}` can say `secret:`, `default:`, `binding:` and `test:`.{hint}"
        ));
        self.out.problems.push(match near {
            Some(near) if word != "required" => problem
                .next(format!("Use `{near}` instead of `{word}`"))
                .edit(span.clone(), near),
            _ => problem.next(format!("Remove `{word}`")),
        });
    }

    /// `binding: "NAME"`: the variable and where it is written.
    fn binding(
        &mut self,
        path: &str,
        key: &Span,
        option: &ast::Member,
    ) -> Option<(String, Span, Span)> {
        let value = option.value();
        let at = value
            .as_ref()
            .map_or_else(|| key.clone(), |v| node_span(path, v.syntax()));
        let Some(Const::Str(name)) = value.as_ref().map(Const::from_expr) else {
            self.out.problems.push(
                Diagnostic::new(
                    &rule::CONFIGURATION_BINDING_INVALID,
                    "`binding:` isn't a variable name in quotes",
                )
                .at(at, "not text")
                .reason("`binding:` names the environment variable a setting is read from, as text in quotes: a platform's own name for a value, such as `\"HEROKU_POSTGRESQL_URL\"`.")
                .next("Write the variable's name in quotes, or leave `binding:` out to read the setting's name in capitals"),
            );
            return None;
        };
        if name.is_empty() {
            self.out.problems.push(
                Diagnostic::new(
                    &rule::CONFIGURATION_BINDING_EMPTY,
                    "`binding:` names no variable",
                )
                .at(at, "empty")
                .reason("`binding:` names the environment variable a setting is read from; an empty name reads nothing.")
                .next("Name the variable, or leave `binding:` out to read the setting's name in capitals"),
            );
            return None;
        }
        if !is_variable_name(&name) {
            let fixed: String = name
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '_' {
                        c.to_ascii_uppercase()
                    } else {
                        '_'
                    }
                })
                .collect();
            let problem = Diagnostic::new(
                &rule::CONFIGURATION_BINDING_INVALID,
                format!("`{name}` can't be a variable name"),
            )
            .at(at.clone(), "not a variable name")
            .reason("A variable name is capital letters, digits and `_`, starting with a capital letter, so every platform accepts it.");
            self.out.problems.push(if is_variable_name(&fixed) {
                problem
                    .next(format!("Use `{fixed}`"))
                    .edit(at, format!("\"{fixed}\""))
            } else {
                problem.next("Write a name of capitals, digits and `_`")
            });
            return None;
        }
        Some((name, key.clone(), at))
    }

    /// A `default:` or `test:` value, checked against the setting's type:
    /// its text as a variable spells it.
    #[allow(clippy::too_many_arguments)]
    fn given(
        &mut self,
        path: &str,
        name: &str,
        word: &str,
        ty: &Ty,
        reads: Reads,
        key: &Span,
        option: &ast::Member,
    ) -> Option<Option<String>> {
        let value = option.value();
        let at = value
            .as_ref()
            .map_or_else(|| key.clone(), |v| node_span(path, v.syntax()));
        let shown = value
            .as_ref()
            .map(|v| written(v.syntax()))
            .unwrap_or_default();
        let constant = value.as_ref().map(Const::from_expr);
        let is_enum = matches!(reads, Reads::Enum | Reads::Zone | Reads::Locale);
        let literal = match &constant {
            None | Some(Const::Other) => None,
            Some(Const::Path(names)) if !is_enum || names.len() != 2 => None,
            Some(constant) => Some(constant),
        };
        let Some(constant) = literal else {
            // A `default:` naming a constant: `jadpo config move` makes
            // that constant this setting (area 10, "Making a constant a
            // setting").
            let names_constant = word == "default"
                && matches!(&constant, Some(Const::Path(names))
                    if crate::setting_move::names_constant(&self.lookup, names).is_some());
            let next = if names_constant {
                format!(
                    "Run `jadpo config move {shown}`, which makes the constant this setting, or write a literal value after `{word}:`"
                )
            } else {
                format!("Write a literal value after `{word}:`, or remove it")
            };
            self.out.problems.push(
                Diagnostic::new(
                    &rule::CONFIGURATION_DEFAULT_LITERAL_REQUIRED,
                    if value.is_some() {
                        format!("`{word}:` on `{name}` isn't a literal")
                    } else {
                        format!("`{word}:` on `{name}` has no value")
                    },
                )
                .at(at, if value.is_some() { "not a literal" } else { "no value" })
                .reason(format!(
                    "A `{word}:` is a literal of the setting's type, checked while compiling (`default: 25`, `default: 5s`); it can't be computed, because nothing runs before the application starts."
                ))
                .next(next),
            );
            return None;
        };
        // An enum's variant, written `Type.value`.
        if let Const::Path(names) = constant {
            return self.given_variant(name, word, ty, reads, names, &at, &shown);
        }
        if let Err(problem) = literal::check(&self.lookup, ty, constant) {
            let diagnostic = match problem {
                Problem::Kind { .. } => Diagnostic::new(
                    &rule::CONFIGURATION_DEFAULT_TYPE,
                    format!("`{shown}` can't be the {word} of `{name}`"),
                )
                .at(at, "the wrong kind of value")
                .reason(format!(
                    "`{name}` is `{ty}`, and this is a different kind of value; the {word} must be a literal of the setting's own type."
                ))
                .next(format!("Write a `{ty}` value after `{word}:`")),
                Problem::Builtin(why) => Diagnostic::new(
                    &rule::CONFIGURATION_DEFAULT_INVALID,
                    format!("`{shown}` isn't a valid `{ty}`"),
                )
                .at(at, "not a valid value")
                .reason(format!(
                    "The value {why}; a {word} is checked while compiling, with the rules of the setting's type."
                ))
                .next(format!("Write a valid `{ty}` value")),
                Problem::Broken(host, broken) => Diagnostic::new(
                    &rule::CONFIGURATION_DEFAULT_INVALID,
                    format!("`{shown}` isn't a valid `{ty}`"),
                )
                .at(at, format!("breaks `{}`", broken.rule))
                .reason(format!(
                    "`{host}` declares `{}`, and the value {}. A {word} is checked against every rule while compiling.",
                    broken.rule, broken.why
                ))
                .next(format!("Use a value that `{ty}` allows")),
            };
            self.out.problems.push(diagnostic);
            return None;
        }
        Some(Some(match constant {
            Const::Number { value, .. } => value.to_string(),
            Const::Duration(text) | Const::Str(text) => text.clone(),
            Const::Bool(flag) => flag.to_string(),
            _ => return None,
        }))
    }

    #[allow(clippy::too_many_arguments)]
    fn given_variant(
        &mut self,
        name: &str,
        word: &str,
        ty: &Ty,
        reads: Reads,
        names: &[String],
        at: &Span,
        shown: &str,
    ) -> Option<Option<String>> {
        let structure = self.lookup.structure(ty);
        let (host, variant) = (names.first()?, names.get(1)?);
        let declared: Vec<String> = match &structure {
            Ty::Named(own) => self
                .lookup
                .variants(own)
                .map(|variants| variants.iter().map(|v| v.name.clone()).collect())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let own = match &structure {
            Ty::Named(own) => own.clone(),
            _ => String::new(),
        };
        if *host != own {
            self.out.problems.push(
                Diagnostic::new(
                    &rule::CONFIGURATION_DEFAULT_TYPE,
                    format!("`{shown}` can't be the {word} of `{name}`"),
                )
                .at(at.clone(), "a different type")
                .reason(format!(
                    "`{name}` is `{ty}`, and this is a variant of `{host}`; the {word} must be a variant of the setting's own type."
                ))
                .next(format!("Write a `{own}` variant, such as `{own}.{}`", declared.first().map_or("value", String::as_str))),
            );
            return None;
        }
        if !declared.contains(variant) {
            let near = nearest(variant, declared.iter().map(String::as_str));
            let problem = Diagnostic::new(
                &rule::CONFIGURATION_DEFAULT_INVALID,
                format!("`{shown}` isn't a variant of `{own}`"),
            )
            .at(at.clone(), "no such variant")
            .reason(format!(
                "`{own}` has no variant called `{variant}`; a {word} is checked while compiling."
            ));
            self.out.problems.push(match near {
                Some(near) => problem
                    .next(format!("Use `{own}.{near}` instead"))
                    .edit(at.clone(), format!("{own}.{near}")),
                None => problem.next(format!("Use a variant of `{own}`")),
            });
            return None;
        }
        // A `Locale` setting starts the application only with a locale it
        // supports (area 10, "Locales").
        if reads == Reads::Locale
            && let Some(plan) = &crate::locales::locales(self.db, self.project).plan
            && jadpo_std::locale_ordinal(variant).is_some_and(|at| !plan.supports(at))
        {
            self.out.problems.push(
                Diagnostic::new(
                    &rule::TYPE_LOCALE_UNSUPPORTED,
                    format!("`{shown}` isn't a locale this application supports"),
                )
                .at(at.clone(), "not in `locales`")
                .reason(format!(
                    "`{name}` is read when the application starts, and only the locales listed in `locales {{ supported: }}`, in `app.jadpo`, are accepted there; a {word} outside them could never start."
                ))
                .next(format!(
                    "Add `{shown}` to `supported:` in `app.jadpo`, or use a locale that is listed"
                )),
            );
            return None;
        }
        let spelling = match reads {
            Reads::Zone => jadpo_std::config::Spelling::of(jadpo_std::ZONE),
            Reads::Locale => jadpo_std::config::Spelling::of(jadpo_std::LOCALE),
            _ => jadpo_std::config::Spelling::Plain,
        };
        Some(Some(spelling.spelling(variant)))
    }

    /// How a type is read from one piece of text; why not otherwise.
    fn classify(&self, ty: &Ty) -> Result<Reads, String> {
        if matches!(ty, Ty::Nullable(_)) {
            return Err(
                "A setting is never optional: give it a `default:`, or leave it required."
                    .to_string(),
            );
        }
        let structure = self.lookup.structure(ty);
        match &structure {
            Ty::List(_) | Ty::Set(_) | Ty::Map(..) => {
                Err("A list is one value in many variables or one variable holding a format; split it into settings.".to_string())
            }
            Ty::Named(name) => match self.lookup.def(name) {
                Some(Def::Enum { variants }) if variants.iter().all(|v| v.fields.is_none()) => {
                    Ok(match name.as_str() {
                        "Zone" => Reads::Zone,
                        "Locale" => Reads::Locale,
                        _ => Reads::Enum,
                    })
                }
                Some(Def::Enum { .. }) => {
                    Err("An enum whose variants carry data can't be written as one piece of text.".to_string())
                }
                Some(Def::Fields { .. }) => {
                    Err("An object has several fields; make each a setting of its own.".to_string())
                }
                Some(Def::Scalar { .. }) => self.scalar(ty),
                // A format holds no value, which the type's own check says.
                Some(Def::Format { .. }) | None => Err(String::new()),
            },
            _ => self.scalar(ty),
        }
    }

    fn scalar(&self, ty: &Ty) -> Result<Reads, String> {
        let Some(base) = self.lookup.base_prim(ty) else {
            return Err(String::new());
        };
        Ok(match base {
            Prim::PageCursor
            | Prim::PageSize
            | Prim::ApiToken
            | Prim::ServiceKey
            | Prim::PresentationText => {
                return Err(format!(
                    "`{}` is made by the runtime, not read from a deployment.",
                    base.name()
                ));
            }
            Prim::Bytes | Prim::ZonedTime | Prim::Unit => {
                return Err(format!(
                    "`{}` has no one-piece text form for a variable.",
                    base.name()
                ));
            }
            other => match other.representation() {
                Prim::Text => Reads::Text,
                Prim::Int | Prim::Decimal => Reads::Number,
                Prim::Bool => Reads::Bool,
                Prim::Uuid => Reads::Uuid,
                Prim::Duration => Reads::Duration,
                Prim::Instant => Reads::Instant,
                Prim::CalendarDate => Reads::Date,
                _ => return Err(String::new()),
            },
        })
    }

    /// What a test run makes for a secret of type `ty`.
    fn generated(&self, ty: &Ty) -> Option<TestSecret> {
        match self.lookup.base_prim(ty)? {
            Prim::PostgresUrl => Some(TestSecret::PostgresUrl),
            Prim::RedisUrl => Some(TestSecret::RedisUrl),
            _ => {
                let mut min = jadpo_std::SIGNING_KEY_MIN as u64;
                let mut max: Option<u64> = None;
                for (_, rules) in self.lookup.rule_chain(ty) {
                    if let Some(m) = rules.min_length {
                        min = min.max(m);
                    }
                    if let Some(m) = rules.max_length {
                        max = Some(max.map_or(m, |old| old.min(m)));
                    }
                }
                Some(TestSecret::Hex {
                    min: u32::try_from(min).unwrap_or(u32::MAX),
                    max: max.map(|m| u32::try_from(m).unwrap_or(u32::MAX)),
                })
            }
        }
    }
}

/// Every `keyword { }` singleton in the project, in file order.
pub(crate) fn singletons(db: &dyn Db, project: Project, keyword: &str) -> Vec<Found> {
    let mut found = Vec::new();
    for &file in project.files(db) {
        let items = item_tree(db, file);
        let indexes: Vec<u32> = items
            .items
            .iter()
            .filter(|item| item.kind == ItemKind::Singleton && item.name == keyword)
            .map(|item| item.index)
            .collect();
        if indexes.is_empty() {
            continue;
        }
        let Some(source) = tree::source(db, file) else {
            continue;
        };
        let path = file.path(db).clone();
        for index in indexes {
            let Some(ast::Decl::SingletonDecl(decl)) = tree::decl_at(&source, index) else {
                continue;
            };
            let keyword = decl.keyword().map_or_else(
                || node_span(&path, decl.syntax()),
                |token| {
                    Span::new(
                        &path,
                        u32::from(token.text_range().start()),
                        u32::from(token.text_range().end()),
                    )
                },
            );
            found.push(Found {
                path: path.clone(),
                keyword,
                list: decl.member_list(),
            });
        }
    }
    found
}

// --- reads -----------------------------------------------------------------

/// Where in a file each `config.<name>` is read, with the setting's name:
/// the places [`use_problems`] and the resolution of names look at.
pub fn reads_in(source: &ast::SourceFile) -> Vec<(String, ast::FieldExpr)> {
    source
        .syntax()
        .descendants()
        .filter_map(ast::FieldExpr::cast)
        .filter_map(|field| {
            let (first, rest) = dotted(&Expr::FieldExpr(field.clone()))?;
            let [name] = rest.as_slice() else { return None };
            if tree::name_ref_text(&first).as_deref() != Some("config") {
                return None;
            }
            Some((tree::name_ref_text(name)?, field))
        })
        .collect()
}

/// Whether `expr` is the value of a member a secret may be given to: in
/// `application { }` or `authentication { }`, one of [`SECRET_SINKS`].
fn is_sink(expr: &ast::FieldExpr) -> bool {
    let Some(member) = expr.syntax().parent().and_then(ast::Member::cast) else {
        return false;
    };
    if member.value().map(|v| v.syntax().clone()) != Some(expr.syntax().clone()) {
        return false;
    }
    let key = member
        .member_key()
        .and_then(|key| key.names().last())
        .and_then(|name| name_text(&name));
    if !key.is_some_and(|key| SECRET_SINKS.contains(&key.as_str())) {
        return false;
    }
    expr.syntax()
        .ancestors()
        .filter_map(ast::SingletonDecl::cast)
        .any(|decl| {
            decl.keyword()
                .is_some_and(|word| matches!(word.text(), "application" | "authentication"))
        })
}

/// The problems of the `config.<name>` reads in one file: a name that is
/// no setting, and a secret read anywhere but a place the compiler owns.
#[salsa::tracked(returns(ref))]
pub fn use_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    let path = file.path(db);
    let mut out = Vec::new();
    for callable in callables_of(db, file) {
        out.extend(
            test_settings(db, project, callable)
                .problems
                .iter()
                .cloned(),
        );
    }
    for (name, field) in reads_in(&source) {
        let span = node_span(path, field.syntax());
        let fact = setting_fact(db, project, Name::new(db, name.clone()));
        let Some(fact) = fact else {
            out.push(unknown_setting(db, project, &name, span));
            continue;
        };
        if fact.secret && !is_sink(&field) {
            out.push(secret_flow(&name, &field, span));
        }
    }
    out
}

fn unknown_setting(db: &dyn Db, project: Project, name: &str, span: Span) -> Diagnostic {
    let config = config(db, project);
    let near = nearest(name, config.written.iter().map(String::as_str));
    let problem = Diagnostic::new(
        &rule::CONFIGURATION_FIELD_UNKNOWN,
        format!("`config.{name}` isn't a setting"),
    )
    .at(span.clone(), "no such setting")
    .reason(if config.declared {
        format!(
            "`config {{ }}` declares no setting called `{name}`.{}",
            near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
        )
    } else {
        "The project declares no `config { }`, so it has no settings; they are declared in `config.jadpo`.".to_string()
    });
    match near {
        Some(near) => problem
            .next(format!("Use `config.{near}` instead of `config.{name}`"))
            .edit(span, format!("config.{near}")),
        None => problem.next(format!("Declare `{name}: <Type>` in `config.jadpo`")),
    }
}

fn secret_flow(name: &str, field: &ast::FieldExpr, span: Span) -> Diagnostic {
    let compared = field
        .syntax()
        .parent()
        .and_then(ast::BinExpr::cast)
        .is_some();
    let reason = if compared {
        "Comparing a secret with something gives a yes or no that leaks it one guess at a time, so a secret is never compared, returned, stored, logged, put in a failure or used to build another value."
    } else {
        "A secret never leaves the server, so it is accepted only where the compiler itself owns the place: a connection's `url:`, an integration's `credential:` and an authentication `signing_key:`. Returning it, storing it, logging it, putting it in a failure, building another value from it or comparing it with something could leak it."
    };
    Diagnostic::new(
        &rule::CONFIGURATION_SECRET_FLOW,
        format!("`config.{name}` is a secret"),
    )
    .at(span, "a secret, read here")
    .reason(reason)
    .next(format!(
        "Give `config.{name}` to the connection, integration or authentication that needs it in `app.jadpo` or `authentication.jadpo`, or leave `secret: true` off `{name}` if its value is public"
    ))
}

// --- connections' addresses ---------------------------------------------------

/// The address a database connection is opened with: the setting its
/// `url:` reads.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConnectionUrl {
    /// The connection's name, as an entity's `store:` names it.
    pub connection: String,
    pub kind: ConnectionKind,
    /// The setting holding its address.
    pub setting: String,
}

/// The connections' addresses, and the problems with them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConnectionUrls {
    pub urls: Vec<ConnectionUrl>,
    pub problems: Vec<Diagnostic>,
}

impl ConnectionUrls {
    pub fn url_of(&self, connection: &str) -> Option<&ConnectionUrl> {
        self.urls.iter().find(|url| url.connection == connection)
    }
}

/// Each connection's address (area 10, "Database and other connection
/// URLs"): `url: config.<setting>`, of the address type the connection's
/// driver needs. The `main` PostgreSQL connection every project has when
/// it declares no database reads `config.database_url`, if it declares one.
#[salsa::tracked(returns(ref))]
pub fn connection_urls(db: &dyn Db, project: Project) -> ConnectionUrls {
    let config = config(db, project);
    let connections = crate::data::connections(db, project);
    let lookup = Lookup::new(db, project);
    let mut out = ConnectionUrls::default();
    for found in singletons(db, project, "application") {
        for (key, member) in tree::members(found.list.clone()) {
            if name_text(&key).as_deref() != Some("connections") {
                continue;
            }
            for (name, entry) in tree::members(member.block()) {
                let Some(connection) = name_text(&name) else {
                    continue;
                };
                let Some(kind) = connections.kind(&connection) else {
                    continue;
                };
                let needs = match kind {
                    ConnectionKind::Postgres => Prim::PostgresUrl,
                    ConnectionKind::Redis => Prim::RedisUrl,
                    _ => continue,
                };
                for (setting, option) in tree::members(entry.block()) {
                    if name_text(&setting).as_deref() != Some("url") {
                        continue;
                    }
                    let Some(value) = option.value() else {
                        continue;
                    };
                    let at = node_span(&found.path, value.syntax());
                    let reads = dotted(&value)
                        .filter(|(first, rest)| {
                            tree::name_ref_text(first).as_deref() == Some("config")
                                && rest.len() == 1
                        })
                        .and_then(|(_, rest)| rest.first().and_then(tree::name_ref_text));
                    let Some(field) = reads else {
                        out.problems.push(url_type(
                            &connection,
                            needs,
                            at,
                            "an address written in source",
                            "A database address normally carries a password, so it is never written in source: it is a secret setting, `config.<name>`, read from the deployment's environment.",
                        ));
                        continue;
                    };
                    let Some(found_setting) = config.setting(&field) else {
                        // An unknown or unreadable setting is reported
                        // where it is read.
                        continue;
                    };
                    if lookup.base_prim(&found_setting.ty) != Some(needs) {
                        out.problems.push(url_type(
                            &connection,
                            needs,
                            at,
                            &format!("`{field}` is `{}`", found_setting.ty),
                            &format!(
                                "A connection opened with `driver: Driver.{}` takes an address of its own type, which checks the scheme and is always secret; `config.{field}` is `{}`.",
                                match kind {
                                    ConnectionKind::Postgres => "postgres",
                                    _ => "redis",
                                },
                                found_setting.ty
                            ),
                        ));
                        continue;
                    }
                    out.urls.push(ConnectionUrl {
                        connection: connection.clone(),
                        kind,
                        setting: field,
                    });
                }
            }
        }
    }
    // The implied `main`.
    if connections.implied_main
        && let Some(setting) = config.setting("database_url")
    {
        if lookup.base_prim(&setting.ty) == Some(Prim::PostgresUrl) {
            out.urls.push(ConnectionUrl {
                connection: crate::data::MAIN.to_string(),
                kind: ConnectionKind::Postgres,
                setting: setting.name.clone(),
            });
        } else if let Some(at) = declared_at(db, project, "database_url") {
            out.problems.push(url_type(
                crate::data::MAIN,
                Prim::PostgresUrl,
                at,
                &format!("`database_url` is `{}`", setting.ty),
                "A project that declares no database has one called `main` on PostgreSQL, opened with `config.database_url`, which is a `PostgresUrl`.",
            ));
        }
    }
    out
}

/// Where setting `name` is written.
fn declared_at(db: &dyn Db, project: Project, name: &str) -> Option<Span> {
    let found = singletons(db, project, "config").into_iter().next()?;
    for (key, member) in tree::members(found.list) {
        if name_text(&key).as_deref() == Some(name) {
            return Some(member.value().map_or_else(
                || node_span(&found.path, key.syntax()),
                |v| node_span(&found.path, v.syntax()),
            ));
        }
    }
    None
}

fn url_type(connection: &str, needs: Prim, at: Span, label: &str, reason: &str) -> Diagnostic {
    Diagnostic::new(
        &rule::CONFIGURATION_URL_TYPE,
        format!(
            "`{connection}`'s address must be a `{}` setting",
            needs.name()
        ),
    )
    .at(at, label.to_string())
    .reason(reason.to_string())
    .next(format!(
        "Declare `<name>: {}` in `config.jadpo`, and write `url: config.<name>`",
        needs.name()
    ))
}

/// The problems with the connections' addresses.
pub fn connection_problems(db: &dyn Db, project: Project) -> &Vec<Diagnostic> {
    &connection_urls(db, project).problems
}

// --- settings tested code reads ------------------------------------------------------

/// A test reads each setting's `test:` value, or else its `default:`, and
/// a secret's value is made for the run; one that tested code reads with
/// none of them has nothing to read (area 13, "Configuration in tests").
/// Only the calls a test makes are followed, through the functions,
/// actions and fixtures it reaches; a request it sends through a route
/// runs what the route does, which the run reports instead.
pub fn test_value_problems(db: &dyn Db, project: Project) -> &Vec<Diagnostic> {
    test_values(db, project)
}

#[salsa::tracked(returns(ref))]
fn test_values(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    use std::collections::BTreeSet;

    use jadpo_graph::Kind;
    use jadpo_graph::body::{Expr, Node};

    let config = config(db, project);
    let wanting: BTreeSet<&str> = config
        .settings
        .iter()
        .filter(|setting| !setting.secret && setting.test.is_none() && setting.default.is_none())
        .map(|setting| setting.name.as_str())
        .collect();
    let mut out = Vec::new();
    if wanting.is_empty() {
        return out;
    }
    let mut reported: BTreeSet<(String, u32)> = BTreeSet::new();
    for &file in project.files(db) {
        for root in callables_of(db, file) {
            let facts = body_facts(db, project, root);
            if facts.key_of(&[Kind::Test]).is_none() {
                continue;
            }
            // A test's key reads `<file> "<sentence>"`.
            let sentence = facts.key.as_ref().map_or_else(String::new, |key| {
                let name = key.name();
                name.split_once('"')
                    .map_or(name, |(_, rest)| rest.trim_end_matches('"'))
                    .to_string()
            });
            let given: BTreeSet<&str> = test_settings(db, project, root)
                .overrides
                .iter()
                .map(|(name, _)| name.as_str())
                .collect();
            let mut visited = BTreeSet::new();
            let mut queue = vec![root];
            while let Some(current) = queue.pop() {
                let checked = typed_body(db, project, current);
                if !visited.insert((
                    current.file(db).path(db).clone(),
                    current.index(db),
                    current.function(db),
                )) {
                    continue;
                }
                let path = current.file(db).path(db).clone();
                for site in &checked.body.sites {
                    let Node::Expr(Expr::Config { field }) = &site.node else {
                        continue;
                    };
                    let name = field.name();
                    if !wanting.contains(name)
                        || given.contains(name)
                        || !reported.insert((path.clone(), site.start))
                    {
                        continue;
                    }
                    out.push(
                        Diagnostic::new(
                            &rule::CONFIGURATION_TEST_VALUE_MISSING,
                            format!("`config.{name}` has no value in tests"),
                        )
                        .at(Span::new(&path, site.start, site.end), "no `test:` or `default:`")
                        .reason(format!(
                            "The test \"{sentence}\" reaches this read. A test reads each setting's `test:` value, or its `default:`, and `{name}` has neither; only a secret is made up for a run, and `{name}` isn't one."
                        ))
                        .next(format!("Give `{name}` a `test:` value, or a `default:`, in `config.jadpo`")),
                    );
                }
                for fact in &body_facts(db, project, current).facts {
                    if let Fact::Action(key) | Fact::Fixture(key) | Fact::Function(key) = fact
                        && let Some(next) = resolve(db, project, key)
                    {
                        queue.push(next);
                    }
                }
            }
        }
    }
    out
}

// --- a test's own values ---------------------------------------------------------------

/// What a test's `config:` setting gives (area 13, "Configuration in
/// tests"): the settings it changes, each with its value in the spelling a
/// variable has, and what is wrong with it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TestSettings {
    pub overrides: Vec<(String, String)>,
    /// Where its `clock:` starts the test's clock, in milliseconds since
    /// 1970-01-01T00:00:00Z (area 13, "Test settings").
    pub clock: Option<i64>,
    pub problems: Vec<Diagnostic>,
}

/// The `config: { name: value }` at the top of a test: each name a setting
/// that isn't secret, each value a literal of the setting's type. Secrets
/// never appear in test source: a run makes its own.
#[salsa::tracked(returns(ref))]
pub fn test_settings<'db>(
    db: &'db dyn Db,
    project: Project,
    callable: Callable<'db>,
) -> TestSettings {
    let mut out = TestSettings::default();
    if callable.function(db).is_some() {
        return out;
    }
    let file = callable.file(db);
    let Some(source) = tree::source(db, file) else {
        return out;
    };
    let Some(ast::Decl::TestDecl(decl)) = tree::decl_at(&source, callable.index(db)) else {
        return out;
    };
    let path = file.path(db).clone();
    let model = config(db, project);
    let mut reader = Reader {
        db,
        project,
        lookup: Lookup::new(db, project),
        out: Config::default(),
    };
    let mut seen: Vec<(String, Span)> = Vec::new();
    for item in decl.block().iter().flat_map(|block| block.block_items()) {
        let ast::BlockItem::Member(member) = item else {
            continue;
        };
        let Some(key) = member.member_key() else {
            continue;
        };
        if tree::written(key.syntax()) == "clock" {
            if let Some(value) = member.value() {
                match test_clock(&path, &value) {
                    Ok(start) => out.clock = Some(start),
                    Err(problem) => reader.out.problems.push(*problem),
                }
            }
            continue;
        }
        if tree::written(key.syntax()) != "config" {
            continue;
        }
        for (name_node, entry) in tree::members(member.block()) {
            let Some(name) = name_text(&name_node) else {
                continue;
            };
            let span = node_span(&path, name_node.syntax());
            if let Some((_, first)) = seen.iter().find(|(seen, _)| *seen == name) {
                reader.out.problems.push(
                    Diagnostic::new(
                        &rule::CONFIGURATION_DUPLICATE_OPTION,
                        format!("`{name}` is given twice in `config:`"),
                    )
                    .at(span, "given again")
                    .also(first.clone(), "first given here")
                    .reason(
                        "A test says each setting's value once, so there is one answer to read.",
                    )
                    .next(format!("Remove one `{name}:`")),
                );
                continue;
            }
            seen.push((name.clone(), span.clone()));
            let Some(setting) = model.setting(&name) else {
                if !model.written.contains(&name) {
                    reader
                        .out
                        .problems
                        .push(unknown_in_test(model, &name, span));
                }
                continue;
            };
            if setting.secret {
                reader.out.problems.push(
                    Diagnostic::new(
                        &rule::CONFIGURATION_SECRET_DEFAULT,
                        format!("`{name}` is secret, and a test gives it a value"),
                    )
                    .at(span, "a secret written in source")
                    .reason("Secrets never appear in test source: every test run makes its own throwaway value, which is enough because a secret can only reach the places the compiler owns.")
                    .next(format!("Remove `{name}:` from `config:`")),
                );
                continue;
            }
            if let Some(Some(text)) = reader.given(
                &path,
                &name,
                "config",
                &setting.ty,
                setting.reads,
                &span,
                &entry,
            ) {
                out.overrides.push((name, text));
            }
        }
    }
    out.problems = reader.out.problems;
    out
}

/// A test's `clock:`: an instant written out, `"2026-01-15T12:00:00Z"`.
fn test_clock(path: &str, value: &ast::Expr) -> Result<i64, Box<Diagnostic>> {
    let span = node_span(path, value.syntax());
    let written = tree::written(value.syntax());
    let text = match value {
        ast::Expr::Literal(_) => match crate::types::Const::from_expr(value) {
            crate::types::Const::Str(text) => Some(text),
            _ => None,
        },
        _ => None,
    };
    if let Some(millis) = text.as_deref().and_then(jadpo_std::instant_millis) {
        return Ok(millis);
    }
    let why = match text.as_deref().map(jadpo_std::check_instant) {
        Some(Err(why)) => format!("The value {why}."),
        _ => format!("`{written}` isn't written out as an instant."),
    };
    Err(Box::new(Diagnostic::new(
        &rule::TYPE_SETTING_VALUE,
        "`clock:` is the instant the test starts at",
    )
    .at(span, "not an instant")
    .reason(format!(
        "{why} A test's clock starts at an instant written as text, ISO 8601 with a `Z` or an offset, and moves only with `clock.advance(...)`."
    ))
    .next("Write the instant, such as `clock: \"2026-01-15T12:00:00Z\"`")))
}

fn unknown_in_test(model: &Config, name: &str, span: Span) -> Diagnostic {
    let near = nearest(name, model.written.iter().map(String::as_str));
    let problem = Diagnostic::new(
        &rule::CONFIGURATION_FIELD_UNKNOWN,
        format!("`{name}` isn't a setting"),
    )
    .at(span.clone(), "no such setting")
    .reason(format!(
        "`config {{ }}` declares no setting called `{name}`, so a test has nothing to change.{}",
        near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
    ));
    match near {
        Some(near) => problem
            .next(format!("Use `{near}` instead of `{name}`"))
            .edit(span, near),
        None => problem.next(format!(
            "Declare `{name}: <Type>` in `config.jadpo`, or remove it"
        )),
    }
}
