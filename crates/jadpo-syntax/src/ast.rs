//! The typed AST: one type per grammar node, generated from `jadpo.ungram`
//! into `generated/nodes.rs`, over the lossless rowan tree.

use std::marker::PhantomData;

use crate::{SyntaxKind, SyntaxNode, SyntaxNodeChildren, SyntaxToken};

pub use crate::generated::nodes::*;

/// A typed view of a syntax node.
pub trait AstNode {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized;

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized;

    fn syntax(&self) -> &SyntaxNode;
}

/// The children of a node that cast to `N`.
#[derive(Debug, Clone)]
pub struct AstChildren<N> {
    inner: SyntaxNodeChildren,
    marker: PhantomData<N>,
}

impl<N> AstChildren<N> {
    fn new(parent: &SyntaxNode) -> AstChildren<N> {
        AstChildren {
            inner: parent.children(),
            marker: PhantomData,
        }
    }
}

impl<N: AstNode> Iterator for AstChildren<N> {
    type Item = N;

    fn next(&mut self) -> Option<N> {
        self.inner.find_map(N::cast)
    }
}

pub(crate) mod support {
    use super::{AstChildren, AstNode};
    use crate::{SyntaxKind, SyntaxNode, SyntaxToken};

    pub(crate) fn child<N: AstNode>(parent: &SyntaxNode) -> Option<N> {
        parent.children().find_map(N::cast)
    }

    /// The first child that casts to `N` after the first child of a kind
    /// `before` accepts: the `b` of `a.b`, where `a` is a name too.
    pub(crate) fn child_after<N: AstNode>(
        parent: &SyntaxNode,
        before: fn(SyntaxKind) -> bool,
    ) -> Option<N> {
        let mut children = parent.children();
        children.find(|child| before(child.kind()))?;
        children.find_map(N::cast)
    }

    pub(crate) fn children<N: AstNode>(parent: &SyntaxNode) -> AstChildren<N> {
        AstChildren::new(parent)
    }

    pub(crate) fn token(parent: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
        parent
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == kind)
    }

    pub(crate) fn token_in(parent: &SyntaxNode, kinds: &[SyntaxKind]) -> Option<SyntaxToken> {
        parent
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| kinds.contains(&token.kind()))
    }
}
/// Documentation comments (`///`) written directly above a node, in order.
pub fn doc_comments(node: &SyntaxNode) -> Vec<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .take_while(|token| token.kind().is_trivia())
        .filter(|token| token.kind() == SyntaxKind::DOC_COMMENT)
        .collect()
}

/// The `decline` lines written directly above a declaration or member
/// (area 1, "Declining a judgement call"): its first children.
pub fn declines(node: &SyntaxNode) -> impl Iterator<Item = Decline> + use<> {
    node.children()
        .take_while(|child| child.kind() == SyntaxKind::DECLINE)
        .filter_map(Decline::cast)
        .collect::<Vec<_>>()
        .into_iter()
}

impl DeclineRule {
    /// The rule's id as written, `library.similar_exists`.
    pub fn id(&self) -> String {
        self.syntax
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| !token.kind().is_trivia())
            .map(|token| token.text().to_string())
            .collect()
    }
}
