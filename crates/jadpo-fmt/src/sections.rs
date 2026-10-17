//! The order of sections inside a declaration (area 14, decided
//! 2026-10-13). The formatter owns it, so a failure, a route entry or an
//! entity reads the same wherever it is written. Fields keep their written
//! order (it is meaningful), and so does everything else this module does
//! not place: it follows the placed sections. The one exception is
//! `config { }`, whose fields are sorted by name (area 10, "Fields in name
//! order", 2026-10-16): a setting is read, bound and reported by name, so
//! its position means nothing, and two workspaces that each add one rarely
//! touch the same lines.

use jadpo_syntax::SyntaxElement;
use jadpo_syntax::SyntaxKind::*;
use jadpo_syntax::SyntaxNode;
use jadpo_syntax::ast::{AstNode, BareMember, Member, SingletonDecl, TypeDecl};

/// `type X = Failure { ... }` (area 4).
const FAILURE: &[&str] = &["kind", "code", "message", "client", "internal"];

/// A route entry, `GET /path { ... }` (area 7).
const ROUTE: &[&str] = &[
    "authentication",
    "rate_limit",
    "input",
    "output",
    "status",
    "deadline",
    "run",
];

/// An entity's sections, after its fields (areas 5, 8, 9 and 15).
const ENTITY: &[&str] = &[
    "identity",
    "persistence",
    "lifecycle",
    "membership",
    "policy",
    "freshness",
    "guard",
    "person",
];

/// Entity sections the pages use that the order above does not place
/// (areas 5, 15 and 16). They are sections, not fields, so they follow the
/// placed ones in written order.
const ENTITY_OTHER: &[&str] = &[
    "unique",
    "indexes",
    "caches",
    "retention",
    "audit",
    "search",
];

/// The declarations whose sections the formatter orders.
#[derive(Clone, Copy)]
pub(crate) enum Order {
    Failure,
    Route,
    Entity,
    /// `config { }`: its fields by name.
    Config,
}

impl Order {
    /// The order for a `{ ... }` member list, by the declaration it belongs
    /// to; `None` keeps written order.
    pub(crate) fn of(list: &SyntaxNode) -> Option<Order> {
        if list.kind() != MEMBER_LIST {
            return None;
        }
        let parent = list.parent()?;
        match parent.kind() {
            ENTITY_DECL => Some(Order::Entity),
            ROUTE_ENTRY => Some(Order::Route),
            SINGLETON_DECL => SingletonDecl::cast(parent)?
                .keyword()
                .is_some_and(|keyword| keyword.text() == "config")
                .then_some(Order::Config),
            TYPE_DECL => {
                let base = TypeDecl::cast(parent)?.base()?;
                let base = base.syntax();
                let is_failure = base.kind() == PATH_TYPE
                    && base
                        .descendants_with_tokens()
                        .filter_map(|e| e.into_token())
                        .filter(|t| !t.kind().is_trivia())
                        .map(|t| t.text().to_string())
                        .eq(["Failure".to_string()]);
                is_failure.then_some(Order::Failure)
            }
            _ => None,
        }
    }

    /// Where an item goes: items are stably sorted by this rank.
    pub(crate) fn rank(self, item: &SyntaxElement) -> (usize, String) {
        let name = section_name(item);
        let placed = match self {
            Order::Failure => FAILURE,
            Order::Route => ROUTE,
            Order::Entity => ENTITY,
            // Fields by name; anything else after them, as written.
            Order::Config => {
                return match name {
                    Some(name) if matches!(item.kind(), MEMBER) => (0, name),
                    _ => (1, String::new()),
                };
            }
        };
        let position = name
            .as_deref()
            .and_then(|name| placed.iter().position(|p| *p == name));
        let rank = match self {
            Order::Failure | Order::Route => position.unwrap_or(placed.len()),
            Order::Entity => {
                if let Some(position) = position {
                    1 + position
                } else {
                    let is_field = matches!(item.kind(), MEMBER | BARE_MEMBER | ELISION)
                        && !name
                            .as_deref()
                            .is_some_and(|name| ENTITY_OTHER.contains(&name));
                    if is_field { 0 } else { 1 + placed.len() }
                }
            }
            Order::Config => 0,
        };
        (rank, String::new())
    }
}

/// The name of a member (`kind: ...`) or a bare member (`identity`); a
/// dotted key is not a section.
fn section_name(item: &SyntaxElement) -> Option<String> {
    let node = item.as_node()?;
    let name = if let Some(member) = Member::cast(node.clone()) {
        let mut names = member.member_key()?.names();
        let name = names.next()?;
        if names.next().is_some() {
            return None;
        }
        name
    } else {
        BareMember::cast(node.clone())?.name()?
    };
    name.syntax()
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| !t.kind().is_trivia())
        .map(|t| t.text().to_string())
}
