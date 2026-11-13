//! A package's capability mappings, whatever the capability (area 11,
//! "How a package provides a capability" and "Mistakes not reported yet";
//! chunk K4).
//!
//! `provides/<capability>.jadpo` maps each standard operation of a
//! capability onto the package's own, as data: field paths, literals,
//! lists, nested blocks, enum tables and `each:` with `as:`. `email`'s,
//! `receiving`'s and `ai`'s are read whole by their own readers
//! ([`crate::mail`], [`crate::receiving`], [`crate::ai_mapping`]), which
//! already refuse anything but data. Every other mapping is checked here:
//!
//! - it names a capability a package may fill: Jadpo owns the list, and
//!   `database`, `cache` and `schedules` are filled only by its built-in
//!   drivers;
//! - it is data, with no call or expression anywhere in it, since any
//!   computing belongs in the package's decoders and encoders;
//! - and, while that capability's mapping isn't read yet (`files`,
//!   `publishing`, `search`, `bot_check`, `documents` and `scanning`), it
//!   is `toolchain.not_built_yet`, since it would be accepted and never
//!   used.

use jadpo_db::{Db, Folder, Ownership, Place, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_syntax::ast::{self, AstNode, Decl, Expr, MemberItem};

use crate::tree::{self, name_text, node_span};

/// The capabilities a package may fill, in area 11's table's order.
pub const FILLABLE: [&str; 9] = [
    "files",
    "email",
    "publishing",
    "receiving",
    "search",
    "bot_check",
    "documents",
    "scanning",
    "ai",
];

/// The capabilities only Jadpo's own drivers fill.
pub const BUILT_IN_ONLY: [&str; 3] = ["database", "cache", "schedules"];

/// The capabilities whose mapping is read and used.
pub const READ: [&str; 3] = ["email", "receiving", "ai"];

const DATA_REASON: &str = "A capability mapping is data: field paths, literals, lists, nested blocks, enum tables, and `each:` with `as:` to map a list element by element. It has no calls or expressions, so the compiler can check both sides of it; any computing is done in the package's decoders and encoders.";

/// Every problem with the package mappings other readers don't read.
#[salsa::tracked(returns(ref))]
pub fn problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for &file in project.files(db) {
        let location = jadpo_db::location(db, project, file);
        let (Ownership::Package(home), Place::Folder(Folder::Provides)) =
            (&location.ownership, &location.place)
        else {
            continue;
        };
        if location.test {
            continue;
        }
        let Some(source) = tree::source(db, file) else {
            continue;
        };
        let path = file.path(db).clone();
        for decl in source.decls() {
            let Decl::ProvideDecl(decl) = decl else {
                continue;
            };
            let Some(name) = decl.name() else {
                continue;
            };
            let Some(capability) = name_text(&name) else {
                continue;
            };
            if READ.contains(&capability.as_str()) {
                continue;
            }
            let head = node_span(&path, name.syntax());
            if !FILLABLE.contains(&capability.as_str()) {
                out.push(unfillable(&capability, head.clone(), &home.folder));
            } else {
                out.push(not_read(&capability, head.clone(), &home.folder));
            }
            let mut checker = Data {
                path: &path,
                capability: &capability,
                out: &mut out,
            };
            checker.block(decl.member_list());
        }
    }
    out
}

/// `provide <capability>` for a capability no package fills.
fn unfillable(capability: &str, span: Span, folder: &str) -> Diagnostic {
    let quoted: Vec<String> = FILLABLE.iter().map(|name| format!("`{name}`")).collect();
    let fillable = match quoted.split_last() {
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
        None => String::new(),
    };
    if BUILT_IN_ONLY.contains(&capability) {
        return Diagnostic::new(
            &rule::PACKAGE_PROVIDER_INVALID,
            format!("`provide {capability}`: no package fills `{capability}`"),
        )
        .at(span, "filled only by Jadpo's own drivers")
        .reason(format!(
            "Jadpo owns the list of capabilities. `database`, `cache` and `schedules` are filled only by its built-in drivers, configured in `app.jadpo`; a package may fill {fillable}."
        ))
        .next(format!(
            "Remove `{folder}/provides/{capability}.jadpo`; what the provider does beyond the capability is the package's own operations"
        ));
    }
    let next = match nearest(capability, FILLABLE.iter().copied()) {
        Some(near) => format!("Write `provide {near}` in `provides/{near}.jadpo`"),
        None => format!(
            "Remove `{folder}/provides/{capability}.jadpo`; what the provider does is the package's own operations, called from the domains its connection lists"
        ),
    };
    Diagnostic::new(
        &rule::PACKAGE_PROVIDER_INVALID,
        format!("`provide {capability}` names no capability"),
    )
    .at(span, "not a capability")
    .reason(format!(
        "A capability is a job Jadpo does itself, and Jadpo owns the list: packages can't add one. A package may fill {fillable}."
    ))
    .next(next)
}

/// `provide <capability>` for a capability whose mapping isn't read yet.
fn not_read(capability: &str, span: Span, folder: &str) -> Diagnostic {
    Diagnostic::new(
        &rule::TOOLCHAIN_NOT_BUILT_YET,
        format!("A package filling `{capability}` isn't built yet"),
    )
    .at(span, "decided, not built")
    .reason(format!(
        "Area 11 lets a package fill `{capability}` with `provides/{capability}.jadpo`, but nothing reads that mapping yet: only `email`'s, `receiving`'s and `ai`'s are read. Accepted, it would never be used."
    ))
    .next(format!(
        "Leave `{folder}/provides/{capability}.jadpo` out for now; the package's own operations can still be called"
    ))
}

/// Walks a mapping, refusing anything that isn't data.
struct Data<'a> {
    path: &'a str,
    capability: &'a str,
    out: &'a mut Vec<Diagnostic>,
}

impl Data<'_> {
    fn block(&mut self, list: Option<ast::MemberList>) {
        for item in list.iter().flat_map(|list| list.member_items()) {
            match item {
                MemberItem::Member(member) => {
                    if let Some(block) = member.block() {
                        self.block(Some(block));
                    }
                    if let Some(value) = member.value() {
                        self.value(&value);
                    }
                }
                other => {
                    let span = node_span(self.path, other.syntax());
                    self.refuse(span, "not a mapping key");
                }
            }
        }
    }

    fn value(&mut self, value: &Expr) {
        match value {
            Expr::Literal(_) | Expr::NameRef(_) => {}
            Expr::FieldExpr(field) if path_only(field.syntax()) => {}
            Expr::ListExpr(list) => {
                for item in list.syntax().children().filter_map(Expr::cast) {
                    self.value(&item);
                }
            }
            // A negative number written out.
            Expr::PrefixExpr(prefix)
                if tree::written(prefix.syntax()).starts_with('-')
                    && prefix
                        .syntax()
                        .children()
                        .filter_map(Expr::cast)
                        .all(|inner| matches!(inner, Expr::Literal(_))) => {}
            other => {
                let label = match other {
                    Expr::CallExpr(_) => "a call",
                    _ => "an expression",
                };
                let span = node_span(self.path, other.syntax());
                self.refuse(span, label);
            }
        }
    }

    fn refuse(&mut self, span: Span, label: &str) {
        self.out.push(
            Diagnostic::new(
                &rule::PACKAGE_PROVIDER_INVALID,
                format!("`provide {}` computes a value", self.capability),
            )
            .at(span, label.to_string())
            .reason(DATA_REASON)
            .next("Do the computing in a field's `decode:` or `encode:` in the package's types, and map the field here"),
        );
    }
}

/// Whether `node`, a field expression, is a dotted path of names alone:
/// `message.to`, `EmailFailure.recipient_rejected`.
fn path_only(node: &jadpo_syntax::SyntaxNode) -> bool {
    node.descendants()
        .filter_map(Expr::cast)
        .all(|expr| matches!(expr, Expr::FieldExpr(_) | Expr::NameRef(_)))
}
