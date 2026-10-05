//! JSDoc modifier tags as the modifiers upstream's reparser makes of them.
//!
//! Ported from typescript-go's `Parser.reparseHosted` (`parser/reparser.go:342`),
//! whose `KindJSDocReadonlyTag`/`KindJSDocPrivateTag`/`KindJSDocPublicTag`/
//! `KindJSDocProtectedTag`/`KindJSDocOverrideTag` arm (`parser/reparser.go:521`)
//! appends a `NodeFlagsReparsed` keyword modifier, located at the tag, to the
//! host's modifier list in a JS file. Every upstream modifier read
//! (`ast.HasSyntacticModifier`, `ast.GetEffectiveModifierFlags`,
//! `hasOverrideModifier`) then sees it.
//!
//! # Port-convention boundary
//!
//! This port has no reparser: JSDoc stays in the checker's `jsdoc_entries`
//! side table (keyed by the comment's host node, filled by the parser), and
//! the AST modifier slices hold only written tokens. So the reparsed half of
//! the list is answered here, on demand, from the host's tags — no cache, no
//! side table of its own. The work is one hash lookup plus a walk of that
//! host's tags, and only in JS files; TypeScript files return before the
//! lookup. Callers that upstream reads modifiers at combine the written slice
//! with [`Checker::jsdoc_reparsed_modifiers`] /
//! [`Checker::has_effective_modifier`].

use tsr_ast::{JSDocTag, Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, Message, messages};

use crate::checker::Checker;

impl Checker<'_, '_> {
    /// The keyword modifiers `reparseHosted` appends to `node`, in tag order,
    /// each with the tag that stands in for its token (the error location
    /// upstream's `modifier.Loc = tag.Loc` gives it).
    ///
    /// The hosts are the arm's: a method or accessor outside an object
    /// literal, a property declaration, a constructor, and the binary
    /// expression of an expression statement (`this.x = …`). Upstream tests
    /// `p.parsingContexts&(1<<PCObjectLiteralMembers)`, which stays set for
    /// everything parsed inside an object literal's member list — so any
    /// enclosing object literal disqualifies a method, as it does there.
    pub(crate) fn jsdoc_reparsed_modifiers(&self, node: NodeId) -> Vec<(SyntaxKind, NodeId)> {
        let mut out = Vec::new();
        if !self.in_js_file(node) {
            return out;
        }
        let host = match self.nodes.kind(node) {
            SyntaxKind::MethodDeclaration | SyntaxKind::GetAccessor | SyntaxKind::SetAccessor => {
                if self.nodes.ancestors(node).any(|ancestor| {
                    self.nodes.kind(ancestor) == SyntaxKind::ObjectLiteralExpression
                }) {
                    return out;
                }
                node
            }
            SyntaxKind::PropertyDeclaration | SyntaxKind::Constructor => node,
            SyntaxKind::BinaryExpression => {
                let Some(parent) = self.nodes.parent(node) else { return out };
                let Some(Node::ExpressionStatement(statement)) = self.node_map.get(parent) else {
                    return out;
                };
                if statement.expression.and_then(|e| Node::from(e).node_id()) != Some(node) {
                    return out;
                }
                parent
            }
            _ => return out,
        };
        let Some(docs) = self.jsdoc_entries.get(&host) else { return out };
        for doc in *docs {
            for tag in doc.tags {
                let (keyword, id) = match tag {
                    JSDocTag::JSDocReadonlyTag(t) => (SyntaxKind::ReadonlyKeyword, t.node_id),
                    JSDocTag::JSDocPrivateTag(t) => (SyntaxKind::PrivateKeyword, t.node_id),
                    JSDocTag::JSDocPublicTag(t) => (SyntaxKind::PublicKeyword, t.node_id),
                    JSDocTag::JSDocProtectedTag(t) => (SyntaxKind::ProtectedKeyword, t.node_id),
                    JSDocTag::JSDocOverrideTag(t) => (SyntaxKind::OverrideKeyword, t.node_id),
                    _ => continue,
                };
                if let Some(id) = id {
                    out.push((keyword, id));
                }
            }
        }
        out
    }

    /// `ast.HasSyntacticModifier(node, flag)` over the written modifiers plus
    /// the reparsed JSDoc ones ([`Checker::jsdoc_reparsed_modifiers`]).
    pub(crate) fn has_effective_modifier(&self, node: NodeId, keyword: SyntaxKind) -> bool {
        let written = self
            .node_map
            .get(node)
            .and_then(crate::check::modifiers_of)
            .is_some_and(|modifiers| crate::check::has_modifier(modifiers, keyword));
        written || self.jsdoc_reparsed_modifiers(node).iter().any(|&(kind, _)| kind == keyword)
    }

    /// `ast.IsPrivateIdentifierClassElementDeclaration` (`ast/utilities.go:562`):
    /// a property, method or accessor named by a private identifier.
    pub(crate) fn is_private_identifier_class_element_declaration(&self, node: NodeId) -> bool {
        matches!(
            self.nodes.kind(node),
            SyntaxKind::PropertyDeclaration
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor
        ) && self
            .declaration_name_of(node)
            .is_some_and(|name| self.nodes.kind(name) == SyntaxKind::PrivateIdentifier)
    }

    /// The rest of `checkGrammarModifiers`' modifier walk
    /// (`checker/grammarchecks.go:214`) over the reparsed JSDoc modifiers,
    /// which follow the written ones in upstream's list. `seen` is the
    /// written walk's state. Only the arms a reparsed `public`/`private`/
    /// `protected`/`readonly`/`override` can reach are here: every
    /// must-precede arm is guarded by `modifier.Flags&ast.NodeFlagsReparsed
    /// == 0`, and the module-element arms cannot apply to the hosts
    /// [`Checker::jsdoc_reparsed_modifiers`] admits.
    pub(crate) fn check_jsdoc_reparsed_modifier_grammar(
        &mut self,
        node: NodeId,
        seen: &mut Vec<SyntaxKind>,
    ) {
        for (kind, tag) in self.jsdoc_reparsed_modifiers(node) {
            let report: Option<(&'static Message, Vec<String>)> = match kind {
                SyntaxKind::PublicKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::PrivateKeyword => {
                    if seen.iter().any(|earlier| {
                        matches!(
                            earlier,
                            SyntaxKind::PublicKeyword
                                | SyntaxKind::ProtectedKeyword
                                | SyntaxKind::PrivateKeyword
                        )
                    }) {
                        Some((&messages::ACCESSIBILITY_MODIFIER_ALREADY_SEEN, Vec::new()))
                    } else if seen.contains(&SyntaxKind::AbstractKeyword) {
                        (kind == SyntaxKind::PrivateKeyword).then(|| {
                            (
                                &messages::_0_MODIFIER_CANNOT_BE_USED_WITH_1_MODIFIER,
                                vec!["private".to_string(), "abstract".to_string()],
                            )
                        })
                    } else if self.is_private_identifier_class_element_declaration(node) {
                        Some((
                            &messages::AN_ACCESSIBILITY_MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER,
                            Vec::new(),
                        ))
                    } else {
                        None
                    }
                }
                SyntaxKind::ReadonlyKeyword => {
                    if seen.contains(&kind) {
                        Some((&messages::_0_MODIFIER_ALREADY_SEEN, vec!["readonly".to_string()]))
                    } else if !matches!(
                        self.nodes.kind(node),
                        SyntaxKind::PropertyDeclaration
                            | SyntaxKind::PropertySignature
                            | SyntaxKind::IndexSignature
                            | SyntaxKind::Parameter
                    ) {
                        Some((
                            &messages::READONLY_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION_OR_INDEX_SIGNATURE,
                            Vec::new(),
                        ))
                    } else if seen.contains(&SyntaxKind::AccessorKeyword) {
                        Some((
                            &messages::_0_MODIFIER_CANNOT_BE_USED_WITH_1_MODIFIER,
                            vec!["readonly".to_string(), "accessor".to_string()],
                        ))
                    } else {
                        None
                    }
                }
                SyntaxKind::OverrideKeyword => {
                    if seen.contains(&kind) {
                        Some((&messages::_0_MODIFIER_ALREADY_SEEN, vec!["override".to_string()]))
                    } else if seen.contains(&SyntaxKind::DeclareKeyword) {
                        Some((
                            &messages::_0_MODIFIER_CANNOT_BE_USED_WITH_1_MODIFIER,
                            vec!["override".to_string(), "declare".to_string()],
                        ))
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some((message, arguments)) = report {
                // The tag is the modifier's location (`modifier.Loc = tag.Loc`);
                // a JSDoc root has no parent edge, so the file is the host's.
                let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
                let span = self.nodes.span(tag);
                self.report(file, Diagnostic::with_args(message, span, arguments));
                return;
            }
            seen.push(kind);
        }
    }
}
