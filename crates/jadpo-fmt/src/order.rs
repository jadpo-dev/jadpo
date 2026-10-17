//! Definition before use (area 1, "Files and names", decided 2026-10-17):
//! after a file's opening comments and `use` lines, every declaration
//! comes before the first one that uses it, so the primary declaration
//! comes last. Where nothing decides, ties are broken by kind (constants,
//! then types, then failures, then helpers) and within a kind by the order
//! the primary declaration first reaches them; declarations that refer to
//! each other keep their written order among themselves.
//!
//! A declaration moves with the comments the formatter prints with it:
//! those above it, up to the previous token's trailing comment, and its own
//! trailing comment. Whatever follows the last declaration stays at the end
//! of the file.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_syntax::SyntaxKind::{CONSTANT_DECL, FUNCTION_DECL, NAME, TYPE_DECL, USE_DECL};
use jadpo_syntax::ast::{AstNode, FieldExpr, NameRef, TypeDecl};
use jadpo_syntax::{SyntaxNode, SyntaxToken, parse_file};

use crate::format::{
    blank_before, first_token, is_comment, leading_comments, own_trivia, trailing_comment,
};

/// How far a declaration is from the primary's reach: never reached ones
/// come after every reached one.
const UNREACHED: usize = usize::MAX;

/// `text` with its declarations in definition-before-use order, the
/// declaration named `primary` last, or `None` when they already are, the
/// primary isn't there, or the file doesn't parse.
pub(crate) fn definition_before_use(text: &str, primary: &str) -> Option<String> {
    let parse = parse_file(text);
    if parse.has_errors() {
        return None;
    }
    let file = parse.syntax();
    let decls: Vec<SyntaxNode> = file
        .children()
        .filter(|child| child.kind() != USE_DECL)
        .collect();
    let primary = decls
        .iter()
        .position(|decl| name_of(decl).as_deref() == Some(primary))?;
    let order = order(&decls, primary);
    if order.iter().copied().eq(0..decls.len()) {
        return None;
    }
    let insert = chunk_start(&decls[0], true)?;
    let tail = chunk_end(decls.last()?)?;
    let mut chunks = Vec::with_capacity(decls.len());
    for (at, decl) in decls.iter().enumerate() {
        chunks.push(chunk_start(decl, at == 0)?..chunk_end(decl)?);
    }
    let mut out = String::with_capacity(text.len() + 2 * decls.len());
    out.push_str(&text[..insert]);
    let mut previous: Option<usize> = None;
    for &at in &order {
        if let Some(before) = previous {
            // Constants written one after another stay together.
            let adjacent = at == before + 1
                && decls[before].kind() == CONSTANT_DECL
                && decls[at].kind() == CONSTANT_DECL
                && first_token(&decls[at]).is_some_and(|t| !blank_before(&t));
            out.push_str(if adjacent { "\n" } else { "\n\n" });
        }
        out.push_str(&text[chunks[at].clone()]);
        previous = Some(at);
    }
    out.push_str(&text[tail..]);
    Some(out)
}

/// The order to write `decls` in, as their positions: each after every
/// declaration it uses, then by kind and reach, with `primary` last and a
/// group of declarations that use each other in the order they were
/// written.
fn order(decls: &[SyntaxNode], primary: usize) -> Vec<usize> {
    let names: BTreeMap<String, usize> = decls
        .iter()
        .enumerate()
        .rev()
        .filter_map(|(at, decl)| Some((name_of(decl)?, at)))
        .collect();
    // What each declaration uses, in the order it first writes it.
    let uses: Vec<Vec<usize>> = decls
        .iter()
        .enumerate()
        .map(|(at, decl)| {
            let mut seen = BTreeSet::new();
            names_in(decl)
                .into_iter()
                .filter_map(|name| names.get(&name).copied())
                .filter(|&used| used != at && seen.insert(used))
                .collect()
        })
        .collect();
    let reach = reach(&uses, primary);
    let groups = groups(&uses);
    // A group is held back by what its members use outside it.
    let group_of: Vec<usize> = {
        let mut of = vec![0; decls.len()];
        for (g, members) in groups.iter().enumerate() {
            for &member in members {
                of[member] = g;
            }
        }
        of
    };
    let mut waiting: Vec<BTreeSet<usize>> = groups
        .iter()
        .enumerate()
        .map(|(g, members)| {
            members
                .iter()
                .flat_map(|&member| uses[member].iter().map(|&used| group_of[used]))
                .filter(|&used| used != g)
                .collect()
        })
        .collect();
    let key = |g: usize| {
        let best = groups[g]
            .iter()
            .map(|&member| (kind_rank(&decls[member]), reach[member], member))
            .min();
        // The primary's group is last whatever else is true of it.
        (g == group_of[primary], best)
    };
    let mut ready: BTreeSet<_> = (0..groups.len())
        .filter(|&g| waiting[g].is_empty())
        .map(|g| (key(g), g))
        .collect();
    let mut out = Vec::with_capacity(decls.len());
    while let Some((_, g)) = ready.pop_first() {
        out.extend(groups[g].iter().copied());
        for (other, held) in waiting.iter_mut().enumerate() {
            if held.remove(&g) && held.is_empty() {
                ready.insert((key(other), other));
            }
        }
    }
    out
}

/// Where the primary first reaches each declaration: its position in a
/// walk that goes from the primary into each name it writes, in the order
/// it writes them, and into what that declaration writes before moving on.
fn reach(uses: &[Vec<usize>], primary: usize) -> Vec<usize> {
    let mut reach = vec![UNREACHED; uses.len()];
    let mut next = 0;
    let mut pending = vec![(primary, 0)];
    while let Some((at, from)) = pending.pop() {
        if from == 0 {
            if reach[at] != UNREACHED {
                continue;
            }
            reach[at] = next;
            next += 1;
        }
        if let Some(&used) = uses[at].get(from) {
            pending.push((at, from + 1));
            pending.push((used, 0));
        }
    }
    reach
}

/// The declarations that use each other, directly or not, in groups of
/// one or more, each group's members in written order and the groups in
/// the order of their first member.
fn groups(uses: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let count = uses.len();
    // Reachability by repeated search: files are small.
    let reaches: Vec<BTreeSet<usize>> = (0..count)
        .map(|start| {
            let mut seen = BTreeSet::new();
            let mut pending = uses[start].clone();
            while let Some(at) = pending.pop() {
                if seen.insert(at) {
                    pending.extend(uses[at].iter().copied());
                }
            }
            seen
        })
        .collect();
    let mut grouped = vec![false; count];
    let mut out = Vec::new();
    for start in 0..count {
        if grouped[start] {
            continue;
        }
        let members: Vec<usize> = (start..count)
            .filter(|&other| {
                other == start
                    || (reaches[start].contains(&other) && reaches[other].contains(&start))
            })
            .collect();
        for &member in &members {
            grouped[member] = true;
        }
        out.push(members);
    }
    out
}

/// Constants, then types, then failures, then helpers; anything else
/// after those.
fn kind_rank(decl: &SyntaxNode) -> u8 {
    match decl.kind() {
        CONSTANT_DECL => 0,
        TYPE_DECL => {
            let failure = TypeDecl::cast(decl.clone())
                .and_then(|decl| decl.base())
                .is_some_and(|base| {
                    base.syntax()
                        .descendants_with_tokens()
                        .filter_map(|element| element.into_token())
                        .filter(|token| !token.kind().is_trivia())
                        .map(|token| token.text().to_string())
                        .eq(["Failure".to_string()])
                });
            if failure { 2 } else { 1 }
        }
        FUNCTION_DECL => 3,
        _ => 4,
    }
}

/// Every name a declaration writes, in order, but not a field after a dot,
/// which names no declaration, and not the declaration's own name.
fn names_in(decl: &SyntaxNode) -> Vec<String> {
    decl.descendants()
        .filter_map(NameRef::cast)
        .filter(|name| {
            !name
                .syntax()
                .parent()
                .and_then(FieldExpr::cast)
                .and_then(|field| field.name_ref())
                .is_some_and(|field| field == *name)
        })
        .filter_map(|name| Some(name.ident_token()?.text().to_string()))
        .collect()
}

/// The name a declaration is written with; tests and setting blocks have
/// none.
fn name_of(decl: &SyntaxNode) -> Option<String> {
    let name = decl.children().find(|child| child.kind() == NAME)?;
    Some(first_token(&name)?.text().to_string())
}

/// Where a declaration and the comments above it begin. The file's
/// opening comments, up to its last blank line before the first token,
/// stay where they are, as they do above `use` lines.
fn chunk_start(decl: &SyntaxNode, first: bool) -> Option<usize> {
    let token = first_token(decl)?;
    let opens_file = first && trivia_only_before(&token);
    let comments: Vec<SyntaxToken> = if opens_file {
        let leading = leading_comments(&token);
        let kept = leading
            .iter()
            .rposition(|comment| comment.blank_after)
            .map_or(0, |last| last + 1);
        leading[kept..].iter().map(|c| c.token.clone()).collect()
    } else {
        own_trivia(&token)
            .into_iter()
            .filter(|t| is_comment(t.kind()))
            .collect()
    };
    let begins = comments.first().unwrap_or(&token);
    Some(usize::from(begins.text_range().start()))
}

/// Whether nothing but trivia comes before `token`.
fn trivia_only_before(token: &SyntaxToken) -> bool {
    let mut current = token.prev_token();
    while let Some(t) = current {
        if !t.kind().is_trivia() {
            return false;
        }
        current = t.prev_token();
    }
    true
}

/// Where a declaration ends: its last token, or the comment after it on
/// the same line.
fn chunk_end(decl: &SyntaxNode) -> Option<usize> {
    let last = decl
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .last()?;
    let end = trailing_comment(&last).unwrap_or(last);
    Some(usize::from(end.text_range().end()))
}
