//! A node's canonical form: its tokens without whitespace or comments, in
//! the order the formatter writes them. Formatting changes only trivia,
//! the order of sections (`sections`) and trailing commas, so two texts
//! that format alike give the same canonical tokens. The graph's digests
//! are taken over this form (docs/compiler-runtime.md §2.3).

use jadpo_syntax::SyntaxKind::*;
use jadpo_syntax::{SyntaxElement, SyntaxNode, SyntaxToken};
use rowan::NodeOrToken;

use crate::imports;
use crate::sections::Order;

/// The tokens of `node` as the formatter would write them, trivia left out.
pub fn canonical_tokens(node: &SyntaxNode) -> Vec<SyntaxToken> {
    let mut out = Vec::new();
    collect(node, &mut out);
    out
}

fn collect(node: &SyntaxNode, out: &mut Vec<SyntaxToken>) {
    let mut children: Vec<SyntaxElement> = node
        .children_with_tokens()
        .filter(|element| !element.kind().is_trivia())
        .collect();
    if matches!(node.kind(), SOURCE_FILE | SNIPPET) {
        // `use` lines go first, sorted.
        children.sort_by_key(|child| match child {
            NodeOrToken::Node(inner) if inner.kind() == USE_DECL => (0, Some(imports::rank(inner))),
            _ => (1, None),
        });
    }
    if node.kind() == USE_NAMES {
        // The names sorted, with commas between them and none after.
        let mut names: Vec<SyntaxNode> = children
            .iter()
            .filter_map(|child| child.as_node().cloned())
            .collect();
        names.sort_by_key(imports::name_key);
        let mut commas = children
            .iter()
            .filter_map(|child| child.as_token().filter(|t| t.kind() == COMMA).cloned());
        let mut placed: Vec<SyntaxElement> = Vec::new();
        placed.extend(children.iter().find(|c| c.kind() == L_BRACE).cloned());
        let count = names.len();
        for (i, name) in names.into_iter().enumerate() {
            placed.push(NodeOrToken::Node(name));
            if i + 1 < count
                && let Some(comma) = commas.next()
            {
                placed.push(NodeOrToken::Token(comma));
            }
        }
        placed.extend(children.iter().find(|c| c.kind() == R_BRACE).cloned());
        children = placed;
    }
    if let Some(order) = Order::of(node) {
        // The braces stay put; the items between them are placed.
        let open = children.iter().position(|c| c.kind() == L_BRACE);
        let close = children.iter().rposition(|c| c.kind() == R_BRACE);
        let start = open.map_or(0, |i| i + 1);
        let end = close.unwrap_or(children.len()).max(start);
        children[start..end].sort_by_key(|item| order.rank(item));
    }
    let bracketed = matches!(node.kind(), PARAM_LIST | ARG_LIST | LIST_EXPR);
    for (i, child) in children.iter().enumerate() {
        match child {
            NodeOrToken::Node(inner) => collect(inner, out),
            NodeOrToken::Token(token) => {
                let trailing_comma = bracketed
                    && token.kind() == COMMA
                    && !children[i + 1..].iter().any(|c| c.as_node().is_some());
                if !trailing_comma {
                    out.push(token.clone());
                }
            }
        }
    }
}
