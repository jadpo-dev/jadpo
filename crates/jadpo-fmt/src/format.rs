//! Syntax tree to document. Every significant token is printed once, in
//! order, with the spacing its node asks for; every comment is printed with
//! the token it belongs to (the sink's rule: trailing comments stay at the
//! end of their token's line, leading comments on their own lines before
//! it).

use std::collections::HashSet;

use jadpo_syntax::SyntaxKind::{self, *};
use jadpo_syntax::{SyntaxElement, SyntaxNode, SyntaxToken};
use rowan::{NodeOrToken, TextSize};

use crate::doc::{Doc, concat, group, indent, nil, text};
use crate::imports;
use crate::sections::Order;

/// The gap before a trailing comment.
const COMMENT_GAP: &str = "  ";

pub(crate) struct Formatter {
    printed: HashSet<TextSize>,
}

/// One comment before a token, and whether a blank line follows it.
pub(crate) struct Leading {
    pub(crate) token: SyntaxToken,
    pub(crate) blank_after: bool,
}

fn newlines(token: &SyntaxToken) -> usize {
    if token.kind() == WHITESPACE {
        token.text().matches('\n').count()
    } else {
        0
    }
}

pub(crate) fn is_comment(kind: SyntaxKind) -> bool {
    matches!(kind, COMMENT | DOC_COMMENT)
}

/// The trivia between the previous significant token and `token`, in order.
fn trivia_before(token: &SyntaxToken) -> (Vec<SyntaxToken>, bool) {
    let mut run = Vec::new();
    let mut current = token.prev_token();
    while let Some(t) = current {
        if !t.kind().is_trivia() {
            run.reverse();
            return (run, true);
        }
        current = t.prev_token();
        run.push(t);
    }
    run.reverse();
    (run, false)
}

/// The trivia after `token` up to the next significant token, in order.
fn trivia_after(token: &SyntaxToken) -> Vec<SyntaxToken> {
    let mut run = Vec::new();
    let mut current = token.next_token();
    while let Some(t) = current {
        if !t.kind().is_trivia() {
            break;
        }
        current = t.next_token();
        run.push(t);
    }
    run
}

/// The comment on the same line after `token`, if any.
pub(crate) fn trailing_comment(token: &SyntaxToken) -> Option<SyntaxToken> {
    let after = trivia_after(token);
    let mut iter = after.into_iter();
    match iter.next() {
        Some(t) if is_comment(t.kind()) => Some(t),
        Some(t) if t.kind() == WHITESPACE && !t.text().contains('\n') => {
            iter.next().filter(|t| is_comment(t.kind()))
        }
        _ => None,
    }
}

/// The trivia before `token` that belongs to it: everything after the
/// previous token's trailing comment.
pub(crate) fn own_trivia(token: &SyntaxToken) -> Vec<SyntaxToken> {
    let (run, has_previous) = trivia_before(token);
    if !has_previous {
        return run;
    }
    // Skip the previous token's trailing comment: same-line whitespace and
    // the first comment.
    let mut start = 0;
    if run
        .first()
        .is_some_and(|t| t.kind() == WHITESPACE && !t.text().contains('\n'))
    {
        start = 1;
    }
    if run.get(start).is_some_and(|t| is_comment(t.kind())) {
        return run[start + 1..].to_vec();
    }
    run
}

/// Whether the source has a blank line before `token` (and its leading
/// comments).
pub(crate) fn blank_before(token: &SyntaxToken) -> bool {
    own_trivia(token)
        .iter()
        .take_while(|t| t.kind() == WHITESPACE)
        .map(newlines)
        .sum::<usize>()
        >= 2
}

pub(crate) fn leading_comments(token: &SyntaxToken) -> Vec<Leading> {
    let run = own_trivia(token);
    let mut out: Vec<Leading> = Vec::new();
    for (i, t) in run.iter().enumerate() {
        if !is_comment(t.kind()) {
            continue;
        }
        let gap: usize = run[i + 1..]
            .iter()
            .take_while(|t| t.kind() == WHITESPACE)
            .map(newlines)
            .sum();
        out.push(Leading {
            token: t.clone(),
            blank_after: gap >= 2,
        });
    }
    out
}

pub(crate) fn first_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| !token.kind().is_trivia())
}

fn significant_children(node: &SyntaxNode) -> Vec<SyntaxElement> {
    node.children_with_tokens()
        .filter(|element| !element.kind().is_trivia())
        .collect()
}

fn is_decl(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        TYPE_DECL
            | FUNCTION_DECL
            | ACTION_DECL
            | QUERY_DECL
            | FIXTURE_DECL
            | ENTITY_DECL
            | LIBRARY_DECL
            | ROUTES_DECL
            | SCHEDULE_DECL
            | SUBSCRIBER_DECL
            | TEST_DECL
            | SEED_DECL
            | PACKAGE_DECL
            | RESOURCE_DECL
            | PROVIDE_DECL
            | SINGLETON_DECL
    )
}

/// Nodes whose parts are written with no spaces between them.
fn tight(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        FIELD_EXPR
            | CALL_EXPR
            | PATH
            | PATH_TYPE
            | ROUTE_PATH
            | ROUTE_SEGMENT
            | PATH_PARAM
            | MEMBER_KEY
            | CONDITION
            | PAREN_EXPR
            | CASE_PAT
            | PAT_BINDING
            | LITERAL_PAT
            | GENERIC_EXPR
            | NULLABLE_EXPR
            | NULLABLE_TYPE
            | GENERIC_ARG_LIST
            | ROUTE_PATH_EXPR
            | USE_PATH
    )
}

fn space_between(parent: SyntaxKind, left: SyntaxKind, right: SyntaxKind) -> bool {
    if matches!(left, COMMA | COLON) {
        return true;
    }
    if tight(parent) {
        return false;
    }
    // `<` and `>` compare here; elsewhere they bracket type arguments.
    if parent == BIN_EXPR {
        return true;
    }
    if matches!(
        right,
        COMMA
            | COLON
            | R_PAREN
            | R_BRACK
            | R_ANGLE
            | QUESTION
            | DOT
            | PARAM_LIST
            | ARG_LIST
            | GENERIC_ARG_LIST
            | PAT_BINDING
    ) {
        return false;
    }
    if matches!(left, L_PAREN | L_BRACK | L_ANGLE | DOT) {
        return false;
    }
    !(parent == PREFIX_EXPR && left == MINUS)
}

impl Formatter {
    pub(crate) fn new() -> Formatter {
        Formatter {
            printed: HashSet::new(),
        }
    }

    /// The number of comments in `node` that were not printed: zero once
    /// formatting is right.
    pub(crate) fn unprinted_comments(&self, node: &SyntaxNode) -> usize {
        node.descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| is_comment(t.kind()) && !self.printed.contains(&t.text_range().start()))
            .count()
    }

    fn comment(&mut self, token: &SyntaxToken) -> Doc {
        self.printed.insert(token.text_range().start());
        text(token.text().trim_end())
    }

    /// A token's leading comments, each on its own line, keeping one blank
    /// line where the source had any.
    fn leading(&mut self, token: &SyntaxToken) -> Doc {
        let comments = leading_comments(token);
        let mut docs = Vec::new();
        for comment in comments {
            if self.printed.contains(&comment.token.text_range().start()) {
                continue;
            }
            docs.push(self.comment(&comment.token));
            docs.push(Doc::HardLine);
            if comment.blank_after {
                docs.push(Doc::HardLine);
            }
        }
        concat(docs)
    }

    fn trailing(&mut self, token: &SyntaxToken) -> Doc {
        match trailing_comment(token) {
            Some(comment) if !self.printed.contains(&comment.text_range().start()) => {
                self.printed.insert(comment.text_range().start());
                Doc::LineSuffix(format!("{COMMENT_GAP}{}", comment.text().trim_end()))
            }
            _ => nil(),
        }
    }

    /// A token with its comments.
    fn token(&mut self, token: &SyntaxToken) -> Doc {
        let leading = self.leading(token);
        let trailing = self.trailing(token);
        concat([leading, text(token.text()), trailing])
    }

    /// Only a dropped token's comments (a trailing comma).
    fn comments_only(&mut self, token: &SyntaxToken) -> Doc {
        let leading = self.leading(token);
        let trailing = self.trailing(token);
        concat([leading, trailing])
    }

    fn element(&mut self, element: &SyntaxElement) -> Doc {
        match element {
            NodeOrToken::Node(node) => self.node(node),
            NodeOrToken::Token(token) => self.token(token),
        }
    }

    pub(crate) fn node(&mut self, node: &SyntaxNode) -> Doc {
        match node.kind() {
            SOURCE_FILE => self.file(node, false),
            SNIPPET => self.file(node, true),
            MEMBER_LIST | BLOCK | MATCH_ARM_LIST => self.braces(node),
            PARAM_LIST | ARG_LIST | LIST_EXPR => self.bracketed(node),
            USE_NAMES => self.use_names(node),
            _ => self.sequence(node),
        }
    }

    /// Children with the spacing their node asks for.
    fn sequence(&mut self, node: &SyntaxNode) -> Doc {
        let parent = node.kind();
        let children = significant_children(node);
        let mut docs = Vec::new();
        let mut previous: Option<SyntaxKind> = None;
        for child in &children {
            if let Some(left) = previous
                && space_between(parent, left, child.kind())
            {
                docs.push(text(" "));
            }
            docs.push(self.element(child));
            previous = Some(child.kind());
        }
        concat(docs)
    }

    /// `use` lines first, sorted, with no blank line between them; then
    /// declarations, one blank line apart; in a snippet, other items keep
    /// one blank line where the source had any. The comments that open the
    /// file, up to its first blank line, stay above the `use` lines.
    fn file(&mut self, node: &SyntaxNode, snippet: bool) -> Doc {
        let (mut uses, items): (Vec<SyntaxNode>, Vec<SyntaxNode>) =
            node.children().partition(|child| child.kind() == USE_DECL);
        let mut docs = Vec::new();
        if !uses.is_empty() {
            uses.sort_by_key(imports::rank);
            if let Some(first) = first_token(node) {
                let comments = leading_comments(&first);
                if let Some(last) = comments.iter().rposition(|comment| comment.blank_after) {
                    for comment in &comments[..=last] {
                        docs.push(self.comment(&comment.token));
                        docs.push(Doc::HardLine);
                        if comment.blank_after {
                            docs.push(Doc::HardLine);
                        }
                    }
                }
            }
            for (i, line) in uses.iter().enumerate() {
                if i > 0 {
                    docs.push(Doc::HardLine);
                }
                docs.push(self.node(line));
            }
            if !items.is_empty() {
                docs.push(Doc::HardLine);
                docs.push(Doc::HardLine);
            }
        }
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                let blank = !snippet
                    || is_decl(item.kind())
                    || is_decl(items[i - 1].kind())
                    || first_token(item).is_some_and(|t| blank_before(&t));
                docs.push(Doc::HardLine);
                if blank {
                    docs.push(Doc::HardLine);
                }
            }
            docs.push(self.node(item));
        }
        // Comments after the last token, or in a file with no tokens.
        let dangling: Vec<SyntaxToken> = node
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| is_comment(t.kind()) && !self.printed.contains(&t.text_range().start()))
            .collect();
        for comment in dangling {
            if !docs.is_empty() {
                docs.push(Doc::HardLine);
                let blank = comment
                    .prev_token()
                    .is_some_and(|t| newlines(&t) >= 2 && t.prev_token().is_some());
                if blank {
                    docs.push(Doc::HardLine);
                }
            }
            docs.push(self.comment(&comment));
        }
        if !docs.is_empty() {
            docs.push(Doc::HardLine);
        }
        concat(docs)
    }

    /// `{ ... }` holding one item per line: members, statements or arms,
    /// with a declaration's sections in their order.
    fn braces(&mut self, node: &SyntaxNode) -> Doc {
        let mut open = None;
        let mut close = None;
        let mut items = Vec::new();
        for element in significant_children(node) {
            match element {
                NodeOrToken::Token(token) if token.kind() == L_BRACE && open.is_none() => {
                    open = Some(token)
                }
                NodeOrToken::Token(token) if token.kind() == R_BRACE => close = Some(token),
                other => items.push(other),
            }
        }
        let open_doc = match &open {
            Some(token) => self.token(token),
            None => nil(),
        };
        // Each item keeps the blank line written before it, wherever its
        // section moves to; the first item in the body never has one.
        let mut placed: Vec<(bool, SyntaxElement)> = items
            .into_iter()
            .enumerate()
            .map(|(i, item)| {
                let first = match &item {
                    NodeOrToken::Node(node) => first_token(node),
                    NodeOrToken::Token(token) => Some(token.clone()),
                };
                (i > 0 && first.is_some_and(|t| blank_before(&t)), item)
            })
            .collect();
        if let Some(order) = Order::of(node) {
            placed.sort_by_key(|(_, item)| order.rank(item));
        }
        let mut body = Vec::new();
        for (i, (blank, item)) in placed.iter().enumerate() {
            body.push(Doc::HardLine);
            if i > 0 && *blank {
                body.push(Doc::HardLine);
            }
            body.push(self.element(item));
        }
        let (close_leading, close_doc) = match &close {
            Some(token) => {
                let comments = leading_comments(token);
                let mut docs = Vec::new();
                for (i, comment) in comments.iter().enumerate() {
                    docs.push(Doc::HardLine);
                    let blank = if i == 0 {
                        !placed.is_empty() && blank_before(token)
                    } else {
                        comments[i - 1].blank_after
                    };
                    if blank {
                        docs.push(Doc::HardLine);
                    }
                    docs.push(self.comment(&comment.token));
                }
                let trailing = self.trailing(token);
                (concat(docs), concat([text("}"), trailing]))
            }
            None => (nil(), nil()),
        };
        if placed.is_empty() && close_leading.is_empty() {
            return concat([open_doc, close_doc]);
        }
        concat([
            open_doc,
            indent(concat([concat(body), close_leading])),
            Doc::HardLine,
            close_doc,
        ])
    }

    /// A `use` line's names, sorted: `{ Pence, Percent }` on one line when
    /// it fits, otherwise one name per line; no trailing comma either way.
    fn use_names(&mut self, node: &SyntaxNode) -> Doc {
        let children = significant_children(node);
        let mut open = None;
        let mut close = None;
        // Each name with the comma written after it, whose comments stay
        // with the name when the names are sorted.
        let mut names: Vec<(SyntaxNode, Option<SyntaxToken>)> = Vec::new();
        for child in &children {
            match child {
                NodeOrToken::Token(token) if token.kind() == L_BRACE => open = Some(token.clone()),
                NodeOrToken::Token(token) if token.kind() == R_BRACE => close = Some(token.clone()),
                NodeOrToken::Token(token) if token.kind() == COMMA => {
                    if let Some((_, comma)) = names.last_mut()
                        && comma.is_none()
                    {
                        *comma = Some(token.clone());
                    }
                }
                NodeOrToken::Node(name) => names.push((name.clone(), None)),
                NodeOrToken::Token(_) => {}
            }
        }
        names.sort_by_key(|(name, _)| imports::name_key(name));
        let open_doc = open.as_ref().map_or_else(nil, |token| self.token(token));
        let close_doc = close.as_ref().map_or_else(nil, |token| self.token(token));
        if names.is_empty() {
            return concat([open_doc, close_doc]);
        }
        let commented = node
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .any(|token| is_comment(token.kind()));
        let line = || if commented { Doc::HardLine } else { Doc::Line };
        let count = names.len();
        let mut inner = vec![line()];
        for (index, (name, comma)) in names.iter().enumerate() {
            inner.push(self.node(name));
            if index + 1 < count {
                inner.push(text(","));
            }
            if let Some(comma) = comma {
                inner.push(self.comments_only(comma));
            }
            if index + 1 < count {
                inner.push(line());
            }
        }
        group(concat([open_doc, indent(concat(inner)), line(), close_doc]))
    }

    /// `( ... )` or `[ ... ]`: on one line when it fits, otherwise one item
    /// per line; no trailing comma either way.
    fn bracketed(&mut self, node: &SyntaxNode) -> Doc {
        let children = significant_children(node);
        let mut open = nil();
        let mut close = nil();
        let mut closing: Option<SyntaxToken> = None;
        let mut items: Vec<Doc> = Vec::new();
        let mut i = 0;
        while i < children.len() {
            match &children[i] {
                NodeOrToken::Token(token)
                    if matches!(token.kind(), L_PAREN | L_BRACK) && i == 0 =>
                {
                    open = self.token(token);
                }
                NodeOrToken::Token(token)
                    if matches!(token.kind(), R_PAREN | R_BRACK) && i == children.len() - 1 =>
                {
                    closing = Some(token.clone());
                }
                NodeOrToken::Token(token) if token.kind() == COMMA => {
                    let last_item = !children[i + 1..].iter().any(|c| c.as_node().is_some());
                    let doc = if last_item {
                        self.comments_only(token)
                    } else {
                        self.token(token)
                    };
                    if let Some(item) = items.last_mut() {
                        let previous = std::mem::replace(item, nil());
                        *item = concat([previous, doc]);
                    }
                }
                other => items.push(self.element(other)),
            }
            i += 1;
        }
        if items.is_empty() {
            // Comments alone inside empty brackets go on their own lines,
            // indented, with the closing bracket on the line after them.
            if let Some(token) = &closing
                && !leading_comments(token).is_empty()
            {
                let mut inner = Vec::new();
                for comment in leading_comments(token) {
                    if self.printed.contains(&comment.token.text_range().start()) {
                        continue;
                    }
                    inner.push(Doc::HardLine);
                    inner.push(self.comment(&comment.token));
                }
                let trailing = self.trailing(token);
                return concat([
                    open,
                    indent(concat(inner)),
                    Doc::HardLine,
                    text(token.text()),
                    trailing,
                ]);
            }
            if let Some(token) = &closing {
                close = self.token(token);
            }
            return concat([open, close]);
        }
        if let Some(token) = &closing {
            close = self.token(token);
        }
        let mut inner = vec![Doc::SoftLine];
        let count = items.len();
        for (index, item) in items.into_iter().enumerate() {
            inner.push(item);
            if index + 1 < count {
                inner.push(Doc::Line);
            }
        }
        group(concat([open, indent(concat(inner)), Doc::SoftLine, close]))
    }
}
