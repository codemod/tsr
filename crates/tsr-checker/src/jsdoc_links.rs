//! `{@link …}` references in JSDoc comments, recorded for the unused check.
//!
//! Ported from typescript-go's `Checker.checkJSDocComments` and
//! `Checker.checkJSDocComment` (`internal/checker/checker.go`), called from
//! `checkSourceElementWorker` for every eager JSDoc comment of a checked node
//! and for every tag of that comment. The only work they do is
//! `resolveJSDocMemberName` with `ignoreErrors`, whose one observable effect is
//! the `SymbolReferenced` marking `noUnusedLocals` reads — so the walk runs
//! only when the unused check is enabled.
//!
//! # Resolution location
//!
//! Upstream resolves from the link's name, whose parent chain runs through the
//! JSDoc into its host. This port's JSDoc keeps no parent edge to its host
//! (`tsr_parser::jsdoc`'s `attach_jsdoc`), so the name is resolved from the
//! host node, which is the first scope the upstream walk reaches.

use tsr_ast::{EntityName, JSDocComment, JSDocTag, NodeId, SyntaxKind};
use tsr_binder::SymbolFlags;

use crate::checker::Checker;

impl Checker<'_, '_> {
    /// `checkJSDocComments` for every JSDoc comment hosted by `node`.
    pub(crate) fn check_jsdoc_link_references(&mut self, node: NodeId) {
        if !self.unused_check_enabled() {
            return;
        }
        let Some(docs) = self.jsdoc_entries.get(&node).copied() else { return };
        for doc in docs {
            self.check_jsdoc_comment_links(node, doc.comment);
            for tag in doc.tags {
                self.check_jsdoc_comment_links(node, jsdoc_tag_comments(tag));
            }
        }
    }

    /// `checkJSDocComment`: a link-like comment resolves its name.
    fn check_jsdoc_comment_links(&mut self, host: NodeId, comments: &[JSDocComment<'_>]) {
        for comment in comments {
            let name = match comment {
                JSDocComment::JSDocLink(link) => link.name,
                JSDocComment::JSDocLinkCode(link) => link.name,
                JSDocComment::JSDocLinkPlain(link) => link.name,
                JSDocComment::JSDocText(_) => None,
            };
            if let Some(name) = name {
                self.resolve_jsdoc_member_name(host, name);
            }
        }
    }

    /// `resolveJSDocMemberName`'s `resolveEntityName(name, Type | Namespace |
    /// Value, ignoreErrors, dontResolveAlias)`: an identifier resolves with the
    /// full meaning, a qualified name's leftmost identifier with `Namespace`
    /// (`resolveQualifiedName`). Each hit marks `referenceKinds |= meaning`
    /// (`symbolReferenced`, `checker.go:1499`), unless it is the host's own
    /// self-reference (`resolveNameHelper`'s `lastSelfReferenceLocation`).
    fn resolve_jsdoc_member_name(&mut self, host: NodeId, name: EntityName<'_>) {
        let mut left = name;
        let mut meaning = SymbolFlags::TYPE | SymbolFlags::NAMESPACE | SymbolFlags::VALUE;
        while let EntityName::QualifiedName(qualified) = left {
            let Some(inner) = qualified.left else { return };
            left = inner;
            meaning = SymbolFlags::NAMESPACE;
        }
        let EntityName::Identifier(identifier) = left else { return };
        let Some(symbol) = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            host,
            identifier.text,
            meaning | SymbolFlags::ALIAS,
        ) else {
            return;
        };
        let symbol = self.binder.merged_symbol(symbol);
        let self_reference = self.binder.symbols().get(symbol).declarations.contains(&host)
            && matches!(
                self.nodes.kind(host),
                SyntaxKind::FunctionDeclaration
                    | SyntaxKind::ClassDeclaration
                    | SyntaxKind::InterfaceDeclaration
                    | SyntaxKind::EnumDeclaration
                    | SyntaxKind::TypeAliasDeclaration
                    | SyntaxKind::ModuleDeclaration
            );
        if !self_reference {
            *self.symbol_reference_kinds.entry(symbol).or_default() |= meaning;
        }
    }
}

/// The `comment` list every JSDoc tag carries (`ast.Node.CommentList`).
fn jsdoc_tag_comments<'a>(tag: &JSDocTag<'a>) -> &'a [JSDocComment<'a>] {
    match tag {
        JSDocTag::JSDocAugmentsTag(tag) => tag.comment,
        JSDocTag::JSDocCallbackTag(tag) => tag.comment,
        JSDocTag::JSDocDeprecatedTag(tag) => tag.comment,
        JSDocTag::JSDocImplementsTag(tag) => tag.comment,
        JSDocTag::JSDocImportTag(tag) => tag.comment,
        JSDocTag::JSDocOverloadTag(tag) => tag.comment,
        JSDocTag::JSDocOverrideTag(tag) => tag.comment,
        JSDocTag::JSDocParameterOrPropertyTag(tag) => tag.comment,
        JSDocTag::JSDocPrivateTag(tag) => tag.comment,
        JSDocTag::JSDocProtectedTag(tag) => tag.comment,
        JSDocTag::JSDocPublicTag(tag) => tag.comment,
        JSDocTag::JSDocReadonlyTag(tag) => tag.comment,
        JSDocTag::JSDocReturnTag(tag) => tag.comment,
        JSDocTag::JSDocSatisfiesTag(tag) => tag.comment,
        JSDocTag::JSDocSeeTag(tag) => tag.comment,
        JSDocTag::JSDocTemplateTag(tag) => tag.comment,
        JSDocTag::JSDocThisTag(tag) => tag.comment,
        JSDocTag::JSDocThrowsTag(tag) => tag.comment,
        JSDocTag::JSDocTypeTag(tag) => tag.comment,
        JSDocTag::JSDocTypedefTag(tag) => tag.comment,
        JSDocTag::JSDocUnknownTag(tag) => tag.comment,
    }
}
