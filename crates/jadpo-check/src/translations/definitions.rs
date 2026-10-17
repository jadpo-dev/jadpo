//! Go to definition from a translation call (area 10, "Tooling"):
//! `translations.shop.parcels(...)` lands on the message's type and on its
//! wording in each language file of the scope, so an editor shows what is
//! being said and in which languages.

use jadpo_db::{Db, File, Project};
use jadpo_syntax::SyntaxKind::IDENT;
use jadpo_syntax::ast::{self, AstNode};
use jadpo_syntax::{SyntaxToken, TokenAtOffset};

use super::declared::{language_files, subjects};
use super::language::language_file;
use crate::lookups::Name;
use crate::tree::{self, dotted};

/// A place to go to: a file and a byte range in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub path: String,
    pub start: u32,
    pub end: u32,
}

/// Where the name at byte `offset` of `file` goes, when it is part of a
/// translation call's callee: the message's (or enum's) type first, then
/// its member in each language file, in path order.
pub fn definitions(db: &dyn Db, project: Project, file: File, offset: u32) -> Option<Vec<Target>> {
    let source = tree::source(db, file)?;
    let token = ident(source.syntax().token_at_offset(offset.into()))?;
    let range = token.text_range();
    let call = token
        .parent()?
        .ancestors()
        .filter_map(ast::CallExpr::cast)
        .find(|call| {
            call.expr()
                .is_some_and(|callee| callee.syntax().text_range().contains_range(range))
        })?;
    let (first, rest) = dotted(&call.expr()?)?;
    let [scope, key] = rest.as_slice() else {
        return None;
    };
    if tree::name_ref_text(&first).as_deref() != Some("translations") {
        return None;
    }
    let scope = tree::name_ref_text(scope)?;
    let key = tree::name_ref_text(key)?;
    let found = subjects(db, project, Name::new(db, scope.clone()));
    let subject = found.get(&key).filter(|subject| subject.callable())?;
    let mut out = vec![Target {
        path: subject.symbol.path.clone(),
        start: subject.name_span.start,
        end: subject.name_span.end,
    }];
    for file in language_files(db, project, &scope) {
        let Some(language) = language_file(db, project, file).as_ref() else {
            continue;
        };
        if let Some(member) = language.member(&key) {
            out.push(Target {
                path: language.path.clone(),
                start: member.key_span.start,
                end: member.key_span.end,
            });
        }
    }
    Some(out)
}

fn ident(tokens: TokenAtOffset<SyntaxToken>) -> Option<SyntaxToken> {
    match tokens {
        TokenAtOffset::None => None,
        TokenAtOffset::Single(token) => (token.kind() == IDENT).then_some(token),
        TokenAtOffset::Between(left, right) => {
            if right.kind() == IDENT {
                Some(right)
            } else {
                (left.kind() == IDENT).then_some(left)
            }
        }
    }
}
