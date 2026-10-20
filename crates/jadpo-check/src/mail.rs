//! A mail package as the `email` connection (area 16, "Email"; area 11,
//! "How a package provides one"; chunk 7.3c).
//!
//! A package fills `email` with `provides/email.jadpo`, `provide email {
//! send: { } limits: { } }`, mapping the capability's one operation onto
//! one of its own as data ([`mappings`]): `send:` calls an operation with
//! its `input:`, each field given a part of the message the runtime
//! rendered (`message.from`, `message.to`, `message.subject`,
//! `message.html`, `message.text`), a setting the package declares
//! (`settings.<name>`) or a value written there, a nested block for an
//! object, and the attachments as `{ each: message.attachments  as: { } }`,
//! each one's `item.name`, `item.content` and `item.content_type`. Every
//! part of the message is given somewhere, the attachments included: a
//! provider that can't send them can't be the email connection. `receipt:`
//! names where the provider's id for the message is in its answer
//! (`response.<field>`), and `failures:` maps the operation's failures onto
//! an email's (`EmailFailure.recipient_rejected` or `.message_rejected`).
//! `limits: { attachments: }` is the total size of attachments the provider
//! accepts, which each email's `File` fields' `max_size:` must fit
//! ([`attachment_problems`]).

use std::collections::BTreeMap;

use jadpo_db::{Db, Folder, Ownership, Place, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_syntax::ast::{self, AstNode, Decl};

use crate::case::to_snake;
use crate::tree::{self, dotted, name_text, node_span};
use crate::types::{Const, Prim, Ty, TypeView, type_table};

/// The parts of the message a field may be given, besides its attachments,
/// each of which `send:` gives somewhere.
pub const MESSAGE_PARTS: [&str; 5] = ["from", "to", "subject", "html", "text"];

/// The parts of the message that may be `none`: an email without that
/// template.
pub const MAYBE_NONE: [&str; 2] = ["html", "text"];

/// The parts of each attachment `as:` may give, and those it must.
pub const ATTACHMENT_PARTS: [&str; 3] = ["name", "content", "content_type"];
pub const ATTACHMENT_NEEDED: [&str; 2] = ["name", "content"];

/// What a failure of the package's operation may be mapped to (area 11,
/// `EmailFailure`): the address refused for good, or the message.
pub const EMAIL_FAILURES: [&str; 2] = ["recipient_rejected", "message_rejected"];

/// The keys `provide email` takes, and `send:`.
const KEYS: [&str; 2] = ["send", "limits"];
const SEND_KEYS: [&str; 4] = ["call", "input", "receipt", "failures"];

/// A value a mapping gives one field of the operation's input.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Mapped {
    /// A part of the message: `from`, `to`, `subject`, `html`, `text`.
    Message(String),
    /// A part of one attachment, inside `as:`: `name`, `content`,
    /// `content_type`.
    Item(String),
    /// `settings.<name>`: the package's setting, which each connection
    /// gives.
    Setting(String),
    Text(String),
    Int(i64),
    Bool(bool),
    /// A nested block for an object: its fields, by their wire names.
    Object(Vec<(String, Mapped)>),
    /// The attachments, each mapped as these fields, by wire name.
    Each(Vec<(String, Mapped)>),
}

/// A package's `provide email`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Mapping {
    /// `emails.send`.
    pub operation: String,
    /// Each field of its input, by its spelling on the wire.
    pub input: Vec<(String, Mapped)>,
    /// Where the provider's id for the message is in the answer.
    pub receipt: Option<Vec<String>>,
    /// Each of the operation's failures, by its code, and the email
    /// failure it is.
    pub failures: Vec<(String, String)>,
    /// The total size of attachments the provider accepts, in bytes.
    pub attachments: u64,
}

/// Every provider package's `provide email`, by its folder, and the
/// problems with them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mappings {
    pub of: BTreeMap<String, Mapping>,
    /// The folders whose mapping doesn't read.
    pub broken: Vec<String>,
    pub problems: Vec<Diagnostic>,
}

/// Every provider package's `provides/email.jadpo`, read and checked
/// against its operations and settings.
#[salsa::tracked(returns(ref))]
pub fn mappings(db: &dyn Db, project: Project) -> Mappings {
    let mut out = Mappings::default();
    for &file in project.files(db) {
        let location = jadpo_db::location(db, project, file);
        let (Ownership::Package(home), Place::Folder(Folder::Provides)) =
            (&location.ownership, &location.place)
        else {
            continue;
        };
        if location.stem != jadpo_std::email::NAMESPACE || location.test {
            continue;
        }
        let Some(source) = tree::source(db, file) else {
            continue;
        };
        let path = file.path(db).clone();
        let Some(decl) = source.decls().find_map(|decl| match decl {
            Decl::ProvideDecl(decl) => Some(decl),
            _ => None,
        }) else {
            continue;
        };
        let before = out.problems.len();
        let mut reader = Reader {
            db,
            project,
            path: &path,
            folder: &home.folder,
            settings: crate::connections::reach(db, project, home)
                .map(|reach| {
                    reach
                        .settings
                        .iter()
                        .map(|(name, _)| name.clone())
                        .collect()
                })
                .unwrap_or_default(),
            out: &mut out.problems,
        };
        let mapping = reader.mapping(&decl);
        match mapping {
            Some(mapping) if out.problems.len() == before => {
                out.of.insert(home.folder.clone(), mapping);
            }
            _ => out.broken.push(home.folder.clone()),
        }
    }
    out
}

const REASON: &str = "`provide email` maps the capability's `send` onto one of the package's operations, as data: `call:` names it, and `input:` gives each field of its input a part of the message (`message.from`, `message.to`, `message.subject`, `message.html`, `message.text`), the attachments as `{ each: message.attachments  as: { <field>: item.name ... } }`, a setting (`settings.<name>`) or a value; `receipt:` names the provider's id for the message in its answer, `failures:` maps the operation's failures onto `EmailFailure`'s, and `limits: { attachments: }` is the total size of attachments the provider accepts.";

struct Reader<'a> {
    db: &'a dyn Db,
    project: Project,
    path: &'a str,
    folder: &'a str,
    /// The settings its package declares.
    settings: Vec<String>,
    out: &'a mut Vec<Diagnostic>,
}

/// Where a value is being mapped: the message itself, or one attachment.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Within {
    Message,
    Attachment,
}

impl Reader<'_> {
    fn invalid(&mut self, span: Span, message: String, label: &str, next: String) {
        self.out.push(
            Diagnostic::new(&rule::PACKAGE_PROVIDER_INVALID, message)
                .at(span, label.to_string())
                .reason(REASON)
                .next(next),
        );
    }

    fn span(&self, node: &impl AstNode) -> Span {
        node_span(self.path, node.syntax())
    }

    fn mapping(&mut self, decl: &ast::ProvideDecl) -> Option<Mapping> {
        let head = decl
            .name()
            .map_or_else(|| self.span(decl), |name| self.span(&name));
        let mut send = None;
        let mut limits = None;
        for (key, member) in tree::members(decl.member_list()) {
            let written = name_text(&key).unwrap_or_default();
            match written.as_str() {
                "send" => send = Some(member),
                "limits" => limits = Some(member),
                other => {
                    let next = match nearest(other, KEYS.iter().copied()) {
                        Some(near) => format!("Write `{near}:` instead of `{other}:`"),
                        None => format!("Remove `{other}:`; `email` has `send:` and `limits:`"),
                    };
                    self.invalid(
                        self.span(&key),
                        format!("`provide email` has no `{other}:`"),
                        "not part of `provide email`",
                        next,
                    );
                }
            }
        }
        let attachments = match &limits {
            Some(limits) => self.limits(limits),
            None => {
                self.invalid(
                    head.clone(),
                    "`provide email` doesn't say how large attachments may be".to_string(),
                    "no `limits:`",
                    "Add `limits: { attachments: 10mb }`, the total size of attachments the provider accepts".to_string(),
                );
                None
            }
        };
        let Some(send) = send else {
            self.invalid(
                head,
                "`provide email` doesn't map `send:`".to_string(),
                "no `send:`",
                "Map `send:` onto the package's operation that sends one email".to_string(),
            );
            return None;
        };
        let mapping = self.send(&send);
        Some(Mapping {
            attachments: attachments?,
            ..mapping?
        })
    }

    /// `limits: { attachments: 10mb }`.
    fn limits(&mut self, member: &ast::Member) -> Option<u64> {
        let mut found = None;
        for (key, inner) in tree::members(member.block()) {
            let written = name_text(&key).unwrap_or_default();
            if written != "attachments" {
                self.invalid(
                    self.span(&key),
                    format!("`limits:` has no `{written}:`"),
                    "not a limit",
                    format!("Remove `{written}:`; `limits:` gives `attachments:`"),
                );
                continue;
            }
            let given = inner
                .value()
                .map(|value| tree::written(value.syntax()))
                .unwrap_or_default();
            match jadpo_std::files::size(&given) {
                Some(bytes) if bytes > 0 => found = Some(bytes),
                _ => self.invalid(
                    inner
                        .value()
                        .map_or_else(|| self.span(&inner), |value| self.span(&value)),
                    "`attachments:` isn't a size".to_string(),
                    "not a size",
                    "Write the total size of attachments the provider accepts, such as `10mb`"
                        .to_string(),
                ),
            }
        }
        if found.is_none()
            && !self
                .out
                .iter()
                .any(|problem| problem.message == "`attachments:` isn't a size")
        {
            self.invalid(
                self.span(member),
                "`provide email` doesn't say how large attachments may be".to_string(),
                "no `attachments:`",
                "Add `attachments: 10mb` to `limits:`, the total size of attachments the provider accepts".to_string(),
            );
        }
        found
    }

    fn send(&mut self, member: &ast::Member) -> Option<Mapping> {
        let Some(block) = member.block() else {
            self.invalid(
                self.span(member),
                "`send:` isn't a block".to_string(),
                "not a block",
                "Write `send: { call: <resource>.<operation>  input: { } }`".to_string(),
            );
            return None;
        };
        let mut members = BTreeMap::new();
        for (key, inner) in tree::members(Some(block)) {
            let written = name_text(&key).unwrap_or_default();
            if !SEND_KEYS.contains(&written.as_str()) {
                self.invalid(
                    self.span(&key),
                    format!("`send:` has no key `{written}:`"),
                    "not a mapping key",
                    format!("Remove `{written}:`"),
                );
                return None;
            }
            members.insert(written, inner);
        }
        let Some(call) = members.get("call") else {
            self.invalid(
                self.span(member),
                "`send:` doesn't give `call:`".to_string(),
                "no `call:`",
                "Add `call: <resource>.<operation>`, the package's operation that sends one email"
                    .to_string(),
            );
            return None;
        };
        let written = call
            .value()
            .map(|value| tree::written(value.syntax()))
            .unwrap_or_default();
        let operations = crate::providers::operations(self.db, self.project);
        let found = written
            .split_once('.')
            .and_then(|(resource, name)| operations.find(self.folder, resource, name))
            .cloned();
        let Some(operation) = found else {
            let known: Vec<String> = operations
                .of(self.folder)
                .map(crate::providers::Operation::full_name)
                .collect();
            let next = match nearest(&written, known.iter().map(String::as_str)) {
                Some(near) => format!("Call `{near}`"),
                None => {
                    "Call one of the package's operations, `<resource>.<operation>`".to_string()
                }
            };
            self.invalid(
                call.value()
                    .map_or_else(|| self.span(call), |value| self.span(&value)),
                format!("`{written}` isn't one of the package's operations"),
                "no such operation",
                next,
            );
            return None;
        };
        if operation.decode.is_some() {
            self.invalid(
                self.span(call),
                format!("`{written}`'s answer is read by the package's own `decode:`"),
                "read by a function",
                "Map an operation whose output is read by its types".to_string(),
            );
            return None;
        }
        let table = type_table(self.db, self.project);
        let input_ty = match &operation.input {
            Some((ty, _)) => ty.clone(),
            None => {
                self.invalid(
                    self.span(call),
                    format!("`{written}` takes no input"),
                    "nothing to give the message to",
                    "Call the operation that sends one email, which takes the message".to_string(),
                );
                return None;
            }
        };
        let Some(input_member) = members.get("input") else {
            self.invalid(
                self.span(member),
                "`send:` doesn't give `input:`".to_string(),
                "no `input:`",
                "Add `input: { }`, giving each field of the operation's input a part of the message"
                    .to_string(),
            );
            return None;
        };
        let mut given: Vec<Mapped> = Vec::new();
        let input = self.object(
            input_member,
            &input_ty,
            &written,
            Within::Message,
            table,
            &mut given,
        )?;
        // Every part of the message goes to the provider.
        for part in MESSAGE_PARTS {
            if !given
                .iter()
                .any(|mapped| *mapped == Mapped::Message(part.to_string()))
            {
                self.invalid(
                    self.span(input_member),
                    format!("`send:` doesn't give the message's `{part}`"),
                    "a part of the message left out",
                    format!("Give `message.{part}` to the input field the provider reads it from"),
                );
            }
        }
        if !given.iter().any(|mapped| matches!(mapped, Mapped::Each(_))) {
            self.invalid(
                self.span(input_member),
                "`send:` doesn't give the message's `attachments`".to_string(),
                "no attachments",
                "Give the attachments as `{ each: message.attachments  as: { ... } }`; a provider that can't send attachments can't be the email connection".to_string(),
            );
        }
        let receipt = match members.get("receipt") {
            Some(receipt) => Some(self.receipt(receipt, &operation.output, table)?),
            None => None,
        };
        let failures = match members.get("failures") {
            Some(failures) => self.failures(failures, &operation)?,
            None => Vec::new(),
        };
        Some(Mapping {
            operation: written,
            input,
            receipt,
            failures,
            attachments: 0,
        })
    }

    /// The fields of `ty`, an object, as `member`'s block gives them, by
    /// wire name; each part of the message given is added to `given`.
    fn object(
        &mut self,
        member: &ast::Member,
        ty: &Ty,
        operation: &str,
        within: Within,
        table: &dyn TypeView,
        given: &mut Vec<Mapped>,
    ) -> Option<Vec<(String, Mapped)>> {
        let fields = match ty.strip_nullable() {
            Ty::Named(name) => table.fields(name).map(<[_]>::to_vec).unwrap_or_default(),
            _ => Vec::new(),
        };
        let entries = tree::members(member.block());
        let mut out = Vec::new();
        let mut ok = true;
        for (key, value) in &entries {
            let name = name_text(key).unwrap_or_default();
            let Some(field) = fields.iter().find(|field| field.name == name) else {
                self.invalid(
                    self.span(key),
                    format!("`{operation}`'s input has no field `{name}`"),
                    "not an input field",
                    format!("Map only the fields of `{operation}`'s input"),
                );
                ok = false;
                continue;
            };
            let Some(mapped) = self.field(
                value,
                &name,
                &field.declared,
                operation,
                within,
                table,
                given,
            ) else {
                ok = false;
                continue;
            };
            given.push(mapped.clone());
            out.push((field.wire_name.clone().unwrap_or(name), mapped));
        }
        for field in &fields {
            let written = entries
                .iter()
                .any(|(key, _)| name_text(key).as_deref() == Some(field.name.as_str()));
            if !written && !matches!(field.declared, Ty::Nullable(_)) && !field.optional {
                self.invalid(
                    self.span(member),
                    format!("`{operation}`'s input field `{}` isn't mapped", field.name),
                    "a required field left out",
                    format!("Add `{}:`", field.name),
                );
                ok = false;
            }
        }
        ok.then_some(out)
    }

    /// One field's value: a part of the message or an attachment, a
    /// setting, a value, a nested object, or the attachments.
    #[allow(clippy::too_many_arguments)]
    fn field(
        &mut self,
        member: &ast::Member,
        name: &str,
        declared: &Ty,
        operation: &str,
        within: Within,
        table: &dyn TypeView,
        given: &mut Vec<Mapped>,
    ) -> Option<Mapped> {
        if member.block().is_some() {
            let each = tree::members(member.block())
                .iter()
                .any(|(key, _)| name_text(key).as_deref() == Some("each"));
            if each && within == Within::Message {
                return self.each(member, name, declared, operation, table);
            }
            let nested = self.object(member, declared, operation, within, table, given)?;
            return Some(Mapped::Object(nested));
        }
        let value = member.value()?;
        if let Some((first, rest)) = dotted(&value) {
            let first = tree::name_ref_text(&first).unwrap_or_default();
            let rest: Vec<String> = rest.iter().filter_map(tree::name_ref_text).collect();
            let written = tree::written(value.syntax());
            let (mapped, part_ty, maybe_none) = match (first.as_str(), rest.as_slice(), within) {
                ("settings", [setting], _) if self.settings.contains(setting) => {
                    return Some(Mapped::Setting(setting.clone()));
                }
                ("message", [part], Within::Message) if MESSAGE_PARTS.contains(&part.as_str()) => (
                    Mapped::Message(part.clone()),
                    if part == "from" || part == "to" {
                        Prim::Email
                    } else {
                        Prim::Text
                    },
                    MAYBE_NONE.contains(&part.as_str()),
                ),
                ("item", [part], Within::Attachment)
                    if ATTACHMENT_PARTS.contains(&part.as_str()) =>
                {
                    (
                        Mapped::Item(part.clone()),
                        if part == "content" {
                            Prim::Bytes
                        } else {
                            Prim::Text
                        },
                        false,
                    )
                }
                ("message", [_], Within::Message) => {
                    let next = if rest.first().map(String::as_str) == Some("attachments") {
                        "Give the attachments as `{ each: message.attachments  as: { ... } }`"
                            .to_string()
                    } else {
                        format!(
                            "Give one of the message's parts: {}",
                            crate::and_list(
                                &MESSAGE_PARTS
                                    .iter()
                                    .map(|part| format!("`message.{part}`"))
                                    .collect::<Vec<_>>()
                            )
                        )
                    };
                    self.invalid(
                        self.span(&value),
                        format!("`{written}` isn't part of the message"),
                        "not a part of the message",
                        next,
                    );
                    return None;
                }
                ("item", [_], Within::Attachment) => {
                    self.invalid(
                        self.span(&value),
                        format!("`{written}` isn't part of an attachment"),
                        "not a part of an attachment",
                        "Give `item.name`, `item.content` or `item.content_type`".to_string(),
                    );
                    return None;
                }
                _ => {
                    self.invalid(
                        self.span(&value),
                        format!("`{written}` isn't a value a mapping can give"),
                        "not mapped data",
                        "Give a part of the message, `settings.<name>`, or a value".to_string(),
                    );
                    return None;
                }
            };
            if maybe_none && !matches!(declared, Ty::Nullable(_)) {
                self.invalid(
                    self.span(&value),
                    format!("`{written}` may be `none`, and `{name}` can't be"),
                    "may be `none`",
                    format!(
                        "Make `{name}` optional in the package's input, `{name}: Text?`, since an email may have no {} template",
                        if written.ends_with("html") { "`.html`" } else { "`.txt`" }
                    ),
                );
                return None;
            }
            let field_prim = table.base_prim(declared.strip_nullable());
            let fits = match part_ty {
                Prim::Bytes => field_prim == Some(Prim::Bytes),
                // An address is text on the wire.
                _ => matches!(field_prim, Some(Prim::Text | Prim::Email)),
            };
            if !fits {
                self.invalid(
                    self.span(&value),
                    format!("`{written}` doesn't fit `{name}`, which is `{declared}`"),
                    "a different type",
                    if part_ty == Prim::Bytes {
                        format!("Give the bytes to a `Bytes` field, `{name}: Bytes`")
                    } else {
                        format!("Give it to a text field, `{name}: Text`")
                    },
                );
                return None;
            }
            return Some(mapped);
        }
        match Const::from_expr(&value) {
            Const::Str(text) => Some(Mapped::Text(text)),
            Const::Bool(flag) => Some(Mapped::Bool(flag)),
            Const::Number {
                value: number,
                decimal: false,
            } if number.scale == 0 => i64::try_from(number.mantissa).ok().map(Mapped::Int),
            _ => {
                self.invalid(
                    self.span(&value),
                    format!(
                        "`{}` isn't a value a mapping can give",
                        tree::written(value.syntax())
                    ),
                    "not mapped data",
                    "Give a part of the message, `settings.<name>`, or text, a whole number or `true`".to_string(),
                );
                None
            }
        }
    }

    /// `{ each: message.attachments  as: { ... } }`: a list of objects,
    /// each given an attachment's parts.
    fn each(
        &mut self,
        member: &ast::Member,
        name: &str,
        declared: &Ty,
        operation: &str,
        table: &dyn TypeView,
    ) -> Option<Mapped> {
        let entries = tree::members(member.block());
        let mut list = None;
        let mut shape = None;
        for (key, inner) in &entries {
            match name_text(key).as_deref() {
                Some("each") => list = inner.value(),
                Some("as") => shape = Some(inner.clone()),
                other => {
                    self.invalid(
                        self.span(key),
                        format!("`{name}:` has no `{}:`", other.unwrap_or_default()),
                        "not part of `each:`",
                        "Give `each: message.attachments` and `as: { ... }`".to_string(),
                    );
                    return None;
                }
            }
        }
        let list_written = list.as_ref().map(|value| tree::written(value.syntax()));
        if list_written.as_deref() != Some("message.attachments") {
            self.invalid(
                list.as_ref()
                    .map_or_else(|| self.span(member), |value| self.span(value)),
                format!("`{name}:` doesn't walk `message.attachments`"),
                "not the attachments",
                "Write `each: message.attachments`".to_string(),
            );
            return None;
        }
        let item = match table.structure(declared.strip_nullable()) {
            Ty::List(item) => *item,
            _ => {
                self.invalid(
                    self.span(member),
                    format!("`{name}` isn't a list, so it can't hold the attachments"),
                    "not a list",
                    format!(
                        "Give the attachments to a list of objects, `{name}: List<Attachment>`"
                    ),
                );
                return None;
            }
        };
        let Some(shape) = shape else {
            self.invalid(
                self.span(member),
                format!("`{name}:` doesn't say how each attachment is given"),
                "no `as:`",
                "Add `as: { <field>: item.name  <field>: item.content }`".to_string(),
            );
            return None;
        };
        let mut given = Vec::new();
        let fields = self.object(
            &shape,
            &item,
            operation,
            Within::Attachment,
            table,
            &mut given,
        )?;
        for part in ATTACHMENT_NEEDED {
            if !given
                .iter()
                .any(|mapped| *mapped == Mapped::Item(part.to_string()))
            {
                self.invalid(
                    self.span(&shape),
                    format!("`{name}:` doesn't give each attachment's `{part}`"),
                    "a part of the attachment left out",
                    format!("Give `item.{part}` to the field the provider reads it from"),
                );
                return None;
            }
        }
        Some(Mapped::Each(fields))
    }

    /// `receipt: response.<field>`: text in the operation's answer.
    fn receipt(
        &mut self,
        member: &ast::Member,
        output: &Ty,
        table: &dyn TypeView,
    ) -> Option<Vec<String>> {
        let value = member.value()?;
        let written = tree::written(value.syntax());
        let parts: Vec<String> = dotted(&value)
            .map(|(first, rest)| {
                std::iter::once(first)
                    .chain(rest)
                    .filter_map(|part| tree::name_ref_text(&part))
                    .collect()
            })
            .unwrap_or_default();
        if parts.first().map(String::as_str) != Some("response") || parts.len() < 2 {
            self.invalid(
                self.span(&value),
                format!("`{written}` isn't a place in the answer"),
                "not a place",
                "Write `receipt: response.<field>`, the provider's id for the message".to_string(),
            );
            return None;
        }
        let mut at = output.clone();
        for name in &parts[1..] {
            let field = match at.strip_nullable() {
                Ty::Named(host) => table
                    .fields(host)
                    .and_then(|fields| fields.iter().find(|field| field.name == *name).cloned()),
                _ => None,
            };
            let Some(field) = field else {
                self.invalid(
                    self.span(&value),
                    format!("`{written}` has no field `{name}`"),
                    "no such field",
                    "Name a text field of the operation's answer".to_string(),
                );
                return None;
            };
            at = field.declared.clone();
        }
        if !matches!(table.base_prim(at.strip_nullable()), Some(Prim::Text)) {
            self.invalid(
                self.span(&value),
                format!("`{written}` isn't text"),
                "not text",
                "Name a text field of the operation's answer, the provider's id for the message"
                    .to_string(),
            );
            return None;
        }
        Some(parts[1..].to_vec())
    }

    /// `failures: { <code>: EmailFailure.<failure> }`.
    fn failures(
        &mut self,
        member: &ast::Member,
        operation: &crate::providers::Operation,
    ) -> Option<Vec<(String, String)>> {
        let codes: Vec<String> = operation
            .failures
            .iter()
            .map(|failure| to_snake(failure.rsplit('.').next().unwrap_or(failure)))
            .collect();
        let name = operation.full_name();
        let mut out = Vec::new();
        let mut ok = true;
        for (key, value) in tree::members(member.block()) {
            let code = name_text(&key).unwrap_or_default();
            if !codes.contains(&code) {
                let next = match nearest(&code, codes.iter().map(String::as_str)) {
                    Some(near) => format!("Map `{near}:` instead of `{code}:`"),
                    None => format!("Remove `{code}:`; `{name}` answers with none of that code"),
                };
                self.invalid(
                    self.span(&key),
                    format!("`{name}` has no failure `{code}`"),
                    "not one of its failures",
                    next,
                );
                ok = false;
                continue;
            }
            let written = value
                .value()
                .map(|value| tree::written(value.syntax()))
                .unwrap_or_default();
            let failure = written
                .strip_prefix("EmailFailure.")
                .filter(|failure| EMAIL_FAILURES.contains(failure));
            let Some(failure) = failure else {
                self.invalid(
                    value
                        .value()
                        .map_or_else(|| self.span(&value), |inner| self.span(&inner)),
                    format!("`{written}` isn't an email's failure"),
                    "not an `EmailFailure`",
                    "Write `EmailFailure.recipient_rejected`, the address refused for good, or `EmailFailure.message_rejected`, the message".to_string(),
                );
                ok = false;
                continue;
            };
            out.push((code, failure.to_string()));
        }
        ok.then_some(out)
    }
}

/// The mapping of the package connection `connection` names, when its
/// package provides `email`.
pub fn mapping_of<'db>(
    db: &'db dyn Db,
    project: Project,
    connection: &str,
) -> Option<&'db Mapping> {
    let connections = &crate::connections::package_connections(db, project).connections;
    let found = connections.iter().find(|known| known.name == connection)?;
    mappings(db, project).of.get(&found.folder)
}

/// `email.attachment_limit`: an email whose `File` fields may hold more
/// than the mail package the email connection names accepts, or one that
/// doesn't say how large it may be (chunk 7.3c). SMTP declares no limit.
pub fn attachment_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let settings = crate::emails::settings(db, project);
    let Some(connection) = settings.connection.as_deref() else {
        return Vec::new();
    };
    let Some(mapping) = mapping_of(db, project, connection) else {
        return Vec::new();
    };
    let limit = mapping.attachments;
    let shown = jadpo_std::files::shown_size(limit);
    let mut out = Vec::new();
    for email in &crate::emails::analysis(db, project).emails {
        let mut total: u64 = 0;
        let mut sized = true;
        for (name, size, span) in &email.attachment_sizes {
            match size {
                Some(bytes) => total = total.saturating_add(*bytes),
                None => {
                    sized = false;
                    out.push(
                        Diagnostic::new(
                            &rule::EMAIL_ATTACHMENT_LIMIT,
                            format!("`{}.{name}` doesn't say how large it may be", email.type_name),
                        )
                        .at(span.clone(), "no `max_size:`")
                        .reason(format!(
                            "An email's `File` fields are sent as attachments, and `{connection}` accepts {shown} of them in all, so each says the most it may hold, and together they fit."
                        ))
                        .next(format!(
                            "Give `{name}` a `max_size:`, such as `{name}: File {{ max_size: 5mb }}`, keeping the email's total within {shown}"
                        )),
                    );
                }
            }
        }
        if sized && total > limit {
            out.push(
                Diagnostic::new(
                    &rule::EMAIL_ATTACHMENT_LIMIT,
                    format!(
                        "`{}`'s attachments may be {}, over the {shown} `{connection}` accepts",
                        email.type_name,
                        jadpo_std::files::shown_size(total)
                    ),
                )
                .at(email.name_span.clone(), "too large to send")
                .reason(format!(
                    "An email's `File` fields are sent as attachments, each at most its `max_size:`, and `{connection}`'s package accepts {shown} of them in all, so an email at its largest would be refused."
                ))
                .next(format!(
                    "Lower the `max_size:` of `{}`'s `File` fields to {shown} in all, or send the larger files as links",
                    email.type_name
                )),
            );
        }
    }
    out
}
