//! TS1100 / TS1210 / TS1215 — `eval` and `arguments` as a binding name or an
//! assignment target.
//!
//! `checkStrictModeEvalOrArguments` (`binder.go:1449`) and its message chooser
//! `getStrictModeEvalOrArgumentsMessage` (`binder.go:1457`).
//!
//! # This rule has no strict-mode gate, and that is upstream's shape
//!
//! The name says otherwise and so did this workstream's own pricing: §105 read
//! the row as *"`b.inStrictMode` plus a three-way message split"* and §137
//! confirmed by `grep` that no strict-mode tracking exists in `tsr_binder`.
//! Both were reasoning about the wrong side. `binder.Binder`
//! (`binder.go:83-113`) has **no `inStrictMode` field**, and every one of the
//! seven call sites is dispatched unconditionally from `bind`
//! (`binder.go:617-640`, `:1165`, `:1194`, `:1369`).
//!
//! The corpus carries the falsifier: `parserStrictMode3-negative.ts` is the
//! single line `eval = 1;` with no `"use strict"` prologue, no `export`, no
//! class and no module indicator, and its baseline records
//! `TS1100: Invalid use of 'eval' in strict mode.` all the same. Under
//! [ADR-0006](../../../docs/adr/0006-conformance-oracle.md) the generated Go is
//! the oracle, so that is the behaviour this port owes.
//!
//! `alwaysStrict`, `alwaysStrictES6` and `alwaysStrictModule` set the compiler
//! option and none of them changes which line is reported — the option is an
//! *emit* concern here, not a binder one, which is why this rule reads no
//! options at all.
//!
//! # Where the rule lives
//!
//! In `crate::check`'s walk rather than in `tsr_binder`, because it needs
//! nothing the binder has: no symbol table, no container chain, no flow. Every
//! input is the finished tree. Keeping it out of the binder also keeps the
//! `binder_symbols` rail out of the blast radius, which §156 registered as a
//! falsifier before this was written.
//!
//! `docs/architecture/checker-notes-diag2.md` §156.

use tsr_ast::{Expression, Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

impl Checker<'_, '_> {
    /// The seven `checkStrictModeEvalOrArguments` call sites, dispatched from
    /// one place.
    ///
    /// A single entry point rather than seven additions to `check_node`'s two
    /// `match` blocks: §140 recorded a rule deleted outright by an earlier arm
    /// claiming its kind, and `BinaryExpression`, `ParameterDeclaration`,
    /// `FunctionDeclaration` and the two unary kinds are all already claimed
    /// there by guards this rule must not be filtered through.
    pub(crate) fn check_strict_mode_eval_or_arguments_sites(
        &mut self,
        node: NodeId,
        typed: Node<'_>,
        ambient: bool,
    ) {
        let name = match typed {
            // `bindVariableDeclarationOrBindingElement` (`binder.go:1164`),
            // called unconditionally and before any of its own branching.
            //
            // **This arm also covers `checkStrictModeCatchClause`**
            // (`binder.go:1393`). Upstream reaches `catch (eval)` twice — once
            // through the catch clause and once through the same
            // `VariableDeclaration` node under `bind` — and its final
            // `SortAndDeduplicateDiagnostics` collapses the pair. §151 verified
            // that `Checker::report` is a plain `push` with no dedup, and the
            // suite compares sorted *multisets*, so a second report here would
            // be an extra line. The catch clause's variable declaration is a
            // `VariableDeclaration` in this tree too and the walk reaches it,
            // so the single arm is both sufficient and duplicate-free.
            Node::VariableDeclaration(declaration) => declaration.name.and_then(|n| n.node_id()),
            Node::BindingElement(element) => element.name.and_then(|n| n.node_id()),
            // `bindParameter` (`binder.go:1188`) — gated on
            // `node.Flags&ast.NodeFlagsAmbient == 0`. This port declares
            // `NodeFlags::AMBIENT` and sets it nowhere (§94's list), so the
            // gate is the walk-threaded `ambient`.
            Node::ParameterDeclaration(parameter) if !ambient => {
                parameter.name.and_then(|n| n.node_id())
            }
            // `checkStrictModeFunctionName` (`binder.go:1366`), from
            // `bindFunctionDeclaration` (`:1215`) and `bindFunctionExpression`
            // (`:913`) — the same ambient gate, and the expression form is
            // reached only when it has a name.
            Node::FunctionDeclaration(declaration) if !ambient => {
                declaration.name.and_then(|n| n.node_id)
            }
            Node::FunctionExpression(declaration) if !ambient => {
                declaration.name.and_then(|n| n.node_id)
            }
            // `checkStrictModeBinaryExpression` (`binder.go:1384`) — the
            // left-hand side of an assignment, and only when it is one:
            // `eval == 1` is a comparison and reports nothing.
            Node::BinaryExpression(binary) => {
                let operator = binary.operator_token.map(|token| token.kind);
                let assignment = operator.is_some_and(SyntaxKind::is_assignment_operator);
                if assignment && binary.left.is_some_and(is_left_hand_side_expression) {
                    binary.left.and_then(|left| left.node_id())
                } else {
                    None
                }
            }
            // `checkStrictModePostfixUnaryExpression` (`binder.go:1412`) —
            // unconditional, because `++` and `--` are the only postfix
            // operators.
            Node::PostfixUnaryExpression(unary) => unary.operand.and_then(|e| e.node_id()),
            // `checkStrictModePrefixUnaryExpression` (`binder.go:1420`) — the
            // operator test is upstream's, and it excludes `!eval`, `-eval`
            // and the rest.
            Node::PrefixUnaryExpression(unary)
                if matches!(
                    unary.operator.kind,
                    SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken
                ) =>
            {
                unary.operand.and_then(|e| e.node_id())
            }
            _ => None,
        };
        let Some(name) = name else { return };
        self.check_strict_mode_eval_or_arguments(node, name);
    }

    /// `checkStrictModeEvalOrArguments` (`binder.go:1449`).
    ///
    /// `contextNode` is the node the *message* is chosen from and `name` is the
    /// node the diagnostic is placed on — upstream passes two arguments for
    /// exactly that reason, and they differ at every call site.
    /// TS1102 — `'delete' cannot be called on an identifier in strict mode.`
    ///
    /// `checkStrictModeDeleteExpression` (`binder.go:1402`), dispatched from
    /// the same unconditional `switch` at `binder.go:632` as the eval/arguments
    /// sites above — so **it has no strict-mode gate either**, for the reason
    /// this module's header establishes and `parserStrictMode3-negative.ts`
    /// demonstrates.
    ///
    /// `docs/architecture/checker-notes-diag2.md` §552.
    pub(crate) fn check_strict_mode_delete_expression(&mut self, node: NodeId) {
        let Some(Node::DeleteExpression(delete)) = self.node_map.get(node) else { return };
        let Some(operand) = delete.expression.and_then(|e| e.node_id()) else { return };
        if self.nodes.kind(operand) != SyntaxKind::Identifier {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(operand) else { return };
        // `errorOnNode` — the operand's own span, as the sites above.
        let span = self.nodes.span(operand);
        self.report(
            file,
            Diagnostic::new(
                &messages::DELETE_CANNOT_BE_CALLED_ON_AN_IDENTIFIER_IN_STRICT_MODE,
                span,
            ),
        );
    }

    fn check_strict_mode_eval_or_arguments(&mut self, context: NodeId, name: NodeId) {
        // `isEvalOrArgumentsIdentifier` (`binder.go:1440`): an `Identifier`
        // whose text is one of the two. A binding *pattern*, a string literal
        // name and a property access all fail it.
        let Some(text) = self.identifier_text(name) else { return };
        if text != "eval" && text != "arguments" {
            return;
        }
        let text = text.to_string();
        let Some(file) = self.source_file_of_for_diagnostics(name) else { return };
        // `errorOnNode` takes the node's own `Loc`, not `GetErrorRangeForNode`
        // — and for an identifier the two agree, so this is the same span
        // `error_span` would give. Written as the node's span to match the call
        // upstream actually makes; §48's centralisation is for the sites that
        // route through `GetErrorRangeForNode`, and this is not one.
        let span = self.nodes.span(name);
        let message = self.strict_mode_eval_or_arguments_message(context, file);
        self.report(file, Diagnostic::with_args(message, span, [text]));
    }

    /// `getStrictModeEvalOrArgumentsMessage` (`binder.go:1457`) — a three-way
    /// choice, in order.
    ///
    /// TS1215 has **zero** missing lines in the corpus, which is not a reason
    /// to leave it out: a port that emitted TS1100 in a module would be wrong
    /// at the right position on every one of those lines. The arm is built for
    /// the wrong column rather than the right one (§156).
    fn strict_mode_eval_or_arguments_message(
        &self,
        context: NodeId,
        file: NodeId,
    ) -> &'static tsr_diagnostics::Message {
        if self.containing_class_of(context).is_some() {
            return &messages::CODE_CONTAINED_IN_A_CLASS_IS_EVALUATED_IN_JAVASCRIPT_S_STRICT_MODE_WHICH_DOES_NOT_ALLOW_THIS_USE_OF_0_FOR_MORE_INFORMATION_SEE_HTTPS_COLON_SLASH_SLASHDEVELOPER_MOZILLA_ORG_SLASHEN_US_SLASHDOCS_SLASHWEB_SLASHJAVASCRIPT_SLASHREFERENCE_SLASHSTRICT_MODE;
        }
        // `b.file.ExternalModuleIndicator != nil`. The parser records the
        // indicator upstream; this port recomputes it from the top-level
        // statements in `tsr_binder::is_external_module`, which is the same
        // question and is documented there.
        if let Some(Node::SourceFile(source)) = self.node_map.get(file) {
            if tsr_binder::is_external_module(source) {
                return &messages::INVALID_USE_OF_0_MODULES_ARE_AUTOMATICALLY_IN_STRICT_MODE;
            }
        }
        &messages::INVALID_USE_OF_0_IN_STRICT_MODE
    }

    /// `ast.GetContainingClass` — `FindAncestor(node.Parent, IsClassLike)`.
    ///
    /// It crosses function boundaries, which is deliberate upstream: a
    /// `function` nested in a method body is still *code contained in a class*
    /// and takes TS1210.
    fn containing_class_of(&self, node: NodeId) -> Option<NodeId> {
        self.nodes.ancestors(node).find(|ancestor| {
            matches!(
                self.node_map.get(*ancestor),
                Some(Node::ClassDeclaration(_) | Node::ClassExpression(_))
            )
        })
    }
}

/// `ast.IsLeftHandSideExpression`, bounded to what an assignment target can be.
///
/// The full predicate lives in `tsr_binder::narrowing` and is crate-private
/// there. Only the kinds that can carry the name `eval` or `arguments` matter
/// here, because `isEvalOrArgumentsIdentifier` rejects everything else a line
/// later — so the two agree on every input this rule can see.
fn is_left_hand_side_expression(expression: Expression<'_>) -> bool {
    matches!(
        expression,
        Expression::Identifier(_)
            | Expression::PropertyAccessExpression(_)
            | Expression::ElementAccessExpression(_)
            | Expression::ParenthesizedExpression(_)
            | Expression::NonNullExpression(_)
    )
}

impl Checker<'_, '_> {
    /// TS1212 / TS1213 / TS1214 — a future reserved word used as an identifier.
    ///
    /// `checkContextualIdentifier` (`binder.go:1303`), whose own comment says
    /// why it lives where it does: *"the binder visits every node in the syntax
    /// tree so it is a convenient place to perform a single localized check for
    /// reserved words used as identifiers in strict mode code"*. The check
    /// traversal is this port's equivalent of that visit.
    ///
    /// # It has no strict-mode gate either
    ///
    /// `TASK-diagnostics.md` §7 carried *"TS1212 needs `alwaysStrict` inside
    /// `tsr_binder::bind`"* for four handoffs. The gate is parse errors,
    /// `NodeFlagsAmbient`, `NodeFlagsJSDoc` and `IsIdentifierName` — and
    /// nothing else. `letIdentifierInElementAccess01.ts` is `var let: any = {};`
    /// with no prologue, no export and no class, and upstream reports TS1212 on
    /// it twice. See `checker-notes-diag2.md` §161.
    ///
    /// # The arm that is not ported
    ///
    /// `originalKeywordKind == KindAwaitKeyword` (`binder.go:1312`) needs
    /// `NodeFlags::AWAIT_CONTEXT`, which this port declares and never sets —
    /// the same shape §104 hit with `YIELD_CONTEXT`. **Owner: `tsr_parser`'s
    /// await-context tracking.** Upstream's *third* arm, `KindYieldKeyword`
    /// under `YieldContext`, is dead code there: `KindYieldKeyword` **is**
    /// `LastFutureReservedWord`, so the first arm always claims it.
    pub(crate) fn check_contextual_identifier(&mut self, node: NodeId, ambient: bool) {
        // `len(b.file.Diagnostics()) == 0` — stated explicitly upstream, so
        // ported rather than measured. §161's third falsifier is the re-measure
        // if the wrong column turns out to concentrate in recovered trees.
        if self.file_has_parse_errors || ambient {
            return;
        }
        let Some(Node::Identifier(identifier)) = self.node_map.get(node) else { return };
        // `NodeFlagsJSDoc` is a fifth declared-and-never-set flag and needs no
        // derivation: this parser keeps JSDoc out of the tree, so the walk
        // never reaches a `@param` name.
        //
        // `scanner.GetIdentifierToken` — the generated keyword table, reused
        // rather than re-listed so that a codegen change cannot silently
        // desynchronise the two.
        let Some(keyword) = tsr_scanner::keyword_kind(identifier.text) else { return };
        let future_reserved = (keyword as u16) >= (SyntaxKind::FIRST_FUTURE_RESERVED_WORD as u16)
            && (keyword as u16) <= (SyntaxKind::LAST_FUTURE_RESERVED_WORD as u16);
        if !future_reserved && keyword != SyntaxKind::AwaitKeyword {
            return;
        }
        if self.is_identifier_name(node) {
            return;
        }
        // **`await` is its own keyword, outside the future-reserved range**, so
        // the second arm of upstream's four-way branch (`binder.go:1311`) was
        // unreachable here rather than declined: an `await` used as a name at
        // the **top level of an external module** is TS1262. The two TS1359
        // arms need `AwaitContext`/`YieldContext` flags this parser does not
        // set. §1043.
        if !future_reserved {
            let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
            let external = matches!(
                self.node_map.get(file),
                Some(Node::SourceFile(source)) if tsr_binder::is_external_module(source)
            );
            // **`IsInTopLevelContext` stops at an arrow; §951's helper does
            // not.** That helper answers *"is there an `arguments` object
            // here"*, for which an arrow is transparent — and reusing it put a
            // TS1262 on the `await` inside `async(() => await(…))`. Same
            // question shape, different container rule; §1030's lesson. §1044.
            if !external
                || self.nodes.ancestors(node).any(|a| self.is_function_like_or_static_block(a))
            {
                return;
            }
            let span = self.nodes.span(node);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::IDENTIFIER_EXPECTED_0_IS_A_RESERVED_WORD_AT_THE_TOP_LEVEL_OF_A_MODULE,
                    span,
                    [identifier.text.to_string()],
                ),
            );
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        // `errorOnNode` — the identifier's own span, as in
        // `check_strict_mode_eval_or_arguments`.
        let span = self.nodes.span(node);
        // `getStrictModeIdentifierMessage` (`binder.go:1332`), the same
        // three-way choice as `getStrictModeEvalOrArgumentsMessage` and in the
        // same order.
        let message = if self.containing_class_of(node).is_some() {
            &messages::IDENTIFIER_EXPECTED_0_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTOMATICALLY_IN_STRICT_MODE
        } else if matches!(self.node_map.get(file), Some(Node::SourceFile(source)) if tsr_binder::is_external_module(source))
        {
            &messages::IDENTIFIER_EXPECTED_0_IS_A_RESERVED_WORD_IN_STRICT_MODE_MODULES_ARE_AUTOMATICALLY_IN_STRICT_MODE
        } else {
            &messages::IDENTIFIER_EXPECTED_0_IS_A_RESERVED_WORD_IN_STRICT_MODE
        };
        // `scanner.DeclarationNameToString` — the identifier's text.
        self.report(file, Diagnostic::with_args(message, span, [identifier.text.to_string()]));
    }

    /// `ast.IsIdentifierName` (`utilities.go:292`) — is this identifier a
    /// *name* rather than a reference?
    ///
    /// Transcribed arm for arm. The three groups are not interchangeable: the
    /// first nine kinds ask whether the identifier is the parent's `name`, the
    /// next three ask about a *different* field, and the last five are true for
    /// any identifier child at all.
    fn is_identifier_name(&self, node: NodeId) -> bool {
        let Some(parent) = self.nodes.parent(node) else { return false };
        let Some(typed) = self.node_map.get(parent) else { return false };
        let field = match typed {
            // `parent.Name() == node`.
            Node::PropertyDeclaration(n) => n.name.node_id(),
            Node::PropertySignatureDeclaration(n) => n.name.node_id(),
            Node::MethodDeclaration(n) => n.name.node_id(),
            Node::MethodSignatureDeclaration(n) => n.name.node_id(),
            Node::GetAccessorDeclaration(n) => n.name.node_id(),
            Node::SetAccessorDeclaration(n) => n.name.node_id(),
            Node::EnumMember(n) => n.name.node_id(),
            Node::PropertyAssignment(n) => n.name.node_id(),
            Node::PropertyAccessExpression(n) => n.name.and_then(|name| name.node_id()),
            // `parent.AsQualifiedName().Right == node` — the LEFT of a
            // qualified name is a reference and is deliberately not covered.
            Node::QualifiedName(n) => n.right.and_then(|right| right.node_id),
            // `parent.PropertyName() == node` — the `a` of `{ a: b }`, whose
            // `name` half (`b`) is a real binding and stays reportable.
            Node::BindingElement(n) => n.property_name.and_then(|name| name.node_id()),
            Node::ImportSpecifier(n) => n.property_name.and_then(|name| name.node_id()),
            // `return true` for any identifier child.
            Node::ExportSpecifier(_)
            | Node::JsxAttribute(_)
            | Node::JsxSelfClosingElement(_)
            | Node::JsxOpeningElement(_)
            | Node::JsxClosingElement(_) => return true,
            _ => return false,
        };
        field == Some(node)
    }
}
