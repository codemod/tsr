//! TS2322 — `Type '{0}' is not assignable to type '{1}'.`
//!
//! [ADR-0040](../../../docs/adr/0040-diagnostics-come-from-a-check-traversal-and-assignability-gets-a-reporting-twin.md)
//! decision (3), and the largest single row the `diagnostics` board has ever
//! carried: 544 cases blocked on this code and nothing else. See
//! `docs/architecture/checker-notes-diag2.md` §16 for the bar.
//!
//! # The message is not part of the comparison
//!
//! `diagnostics` compares the multiset of `(file, line, column, code)`. TS2322's
//! two type arguments — the whole of `Type 'X' is not assignable to type 'Y'` —
//! are **not compared**, which detaches this rule from the entire type-printing
//! subsystem. What must be right is the *position* and the *predicate*, and only
//! those.
//!
//! # Why a wrong answer here is worse than elsewhere
//!
//! Every other rule in [`crate::check`] reports on a syntactic fact or on a
//! failure to resolve. This one reports on a **relation**, and this port's
//! relation is incomplete: `checker_types` reads a 74% gradient, so roughly a
//! quarter of the types the relation is handed are not the types upstream would
//! hand it. An incomplete relation answers "not assignable" where upstream
//! answers "assignable", and that is a *wrong diagnostic* — which fails its own
//! case and can break one that passes.
//!
//! So the rule is written as a set of **declines**, each naming the situation
//! where this port's answer cannot be trusted, and the measurement in §16 is
//! what decides whether the declines are drawn in the right place.

use tsr_ast::{BinaryExpression, Node, NodeId, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::{
    checker::Checker,
    flags::TypeFlags,
    types::{TypeData, TypeId},
};

/// Verdicts recorded by §172's probe, in the order
/// `report_assignability_failure` tests them.
pub const PROBE_REPORTED: u8 = 0;
/// Declined: an object literal against a union target — TS2353/TS2561/TS2739's
/// machinery, not this site's.
pub const PROBE_OBJECT_LITERAL_UNION: u8 = 1;
/// Declined by `pair_is_reportable` — one side is a type this port will not
/// speak about.
pub const PROBE_PAIR_NOT_REPORTABLE: u8 = 2;
/// Declined because the three-valued relation did not answer `NotRelated`.
/// **This is the `checker_types` bucket**: `Unknown` means the relation could
/// not decide, which is the members-table gap.
pub const PROBE_RELATION_DECLINED: u8 = 3;

fn assign_probe_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("TSR_ASSIGN_PROBE").is_ok())
}

impl<'a> Checker<'a, '_> {
    /// `checkAssignmentOperator` (`checker.go:12757`), reached from
    /// `checkBinaryLikeExpressionWorker` for `=`, `+=`, `&&=`, `||=`, `??=`
    /// and (behind `leftOk && rightOk`) the arithmetic compound forms.
    ///
    /// The error node is the **left operand**, not the whole expression:
    /// `aliasAssignments_1.ts(3,1)` for `x = 1` puts the caret on `x`. The
    /// source is the right operand's type for `=` and the logical forms, and
    /// the operation's result type (`resultType`) for `+=` and the arithmetic
    /// forms — `x += ''` relates `string` to `x`.
    pub(crate) fn check_assignment_operator(
        &mut self,
        node: NodeId,
        binary: &BinaryExpression<'_>,
        ambient: bool,
    ) {
        if ambient || self.file_has_parse_errors {
            return;
        }
        let (Some(left), Some(right), Some(operator)) =
            (binary.left, binary.right, binary.operator_token)
        else {
            return;
        };
        let Some(left_id) = left.node_id() else { return };
        // `checkBinaryLikeExpression` (`checker.go:12338`) short-circuits a
        // destructuring `=` to `checkDestructuringAssignment`, which relates
        // per element and never reaches this site.
        if operator.kind == SyntaxKind::EqualsToken
            && matches!(
                self.nodes.kind(left_id),
                SyntaxKind::ObjectLiteralExpression | SyntaxKind::ArrayLiteralExpression
            )
        {
            return;
        }
        // `checkReferenceExpression` (`checker.go:13130`): assignability is
        // checked only when the left-hand side is a reference.
        if !self.is_assignable_reference(left_id) {
            return;
        }
        let Some(mut target) = self.assignment_target_type(left_id) else { return };
        // "getters can be a subtype of setters, so to check for assignability
        // we use the setter's type instead" (`checker.go:12765`): a compound
        // write through a property access reads `checkPropertyAccessExpression`
        // with `writeOnly`, whose divergent-accessor answer is the setter's
        // parameter type (`getWriteTypeOfAccessors`).
        if operator.kind != SyntaxKind::EqualsToken
            && let tsr_ast::Expression::PropertyAccessExpression(access) = left
            && let Some(tsr_ast::MemberName::Identifier(name)) = access.name
            && let Some(receiver) = access.expression
        {
            let receiver = self.check_expression(receiver);
            let receiver = self.check_non_null_type(receiver);
            if let Some(property) = self.get_property_of_type(receiver, name.text)
                && let Some(written) = self.write_type_of_accessors(property)
            {
                target = written;
            }
        }
        let source = match operator.kind {
            SyntaxKind::EqualsToken
            | SyntaxKind::AmpersandAmpersandEqualsToken
            | SyntaxKind::BarBarEqualsToken
            | SyntaxKind::QuestionQuestionEqualsToken => self.check_expression(right),
            // `resultType`: `checkBinaryLikeExpressionWorker`'s `+` and
            // arithmetic arms (`checker.go:12401`, `:12458`). An operator
            // error (`errorType`) returns before the call upstream.
            _ => {
                let result = self.check_expression_at_node(node);
                if result == self.intrinsics().error {
                    return;
                }
                result
            }
        };
        // checkAssignmentOperator (native 5b1047d1 checker.go:12760) ignores
        // undefined writes to named CommonJS exports with multiple declarations.
        // Unlike inference's first-initializer rule, this applies to later
        // undefined writes too. Resolve the actual receiver's property, since a
        // shadowed `exports` may not name the binder's file-module declaration.
        if self.is_commonjs_export_property_assignment(binary)
            && self.type_of(source).flags.intersects(TypeFlags::UNDEFINED)
            && let tsr_ast::Expression::PropertyAccessExpression(access) = left
            && let Some(tsr_ast::MemberName::Identifier(name)) = access.name
            && let Some(receiver) = access.expression
        {
            let receiver = self.check_expression(receiver);
            if self
                .get_property_of_type(receiver, name.text)
                .is_some_and(|property| self.binder.symbols().get(property).declarations.len() > 1)
            {
                return;
            }
        }
        let Some(right_id) = right.node_id() else { return };
        // §72's rule: if the **elaboration** spoke, the outer message does not.
        // `checkTypeRelatedToAndOptionallyElaborate` reports an object
        // literal's offending member instead of the outer assignability
        // failure — §73.
        let before = self.diagnostics.len();
        self.check_excess_properties(target, right_id);
        if self.diagnostics.len() != before {
            return;
        }
        self.report_assignability_failure(left_id, right_id, source, target);
    }

    /// `checkInExpression` (`checker.go:13077`): the left operand must be
    /// assignable to `string | number | symbol` (unless it is a private name)
    /// and the right operand to `object`, each through `checkTypeAssignableTo`
    /// with the operand as error node and no expression elaboration. Operand
    /// types are `checkNonNullType`'s; its own nullability diagnostics belong
    /// to that rule, and a refused (error) operand is silent here. When the
    /// right operand does relate, `hasEmptyObjectIntersection` still rejects a
    /// `{}` that may be a primitive (TS2638).
    pub(crate) fn check_in_expression(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(node) else { return };
        if binary.operator_token.is_none_or(|token| token.kind != SyntaxKind::InKeyword) {
            return;
        }
        let (Some(left), Some(right)) = (
            binary.left.and_then(|left| left.node_id()),
            binary.right.and_then(|right| right.node_id()),
        ) else {
            return;
        };
        let left_type = self.check_expression_at_node(left);
        let right_type = self.check_expression_at_node(right);
        if self.nodes.kind(left) != SyntaxKind::PrivateIdentifier {
            let intrinsics = self.intrinsics();
            let (string, number, symbol) =
                (intrinsics.string, intrinsics.number, intrinsics.es_symbol);
            let target = self.get_union_type(&[string, number, symbol]);
            let source = self.check_non_null_type(left_type);
            self.report_assignability_failure(left, left, source, target);
        }
        let source = self.check_non_null_type(right_type);
        let non_primitive = self.intrinsics().non_primitive;
        if self.report_assignability_failure(right, right, source, non_primitive)
            || self.relate_ternary(source, non_primitive, crate::relater::Relation::Assignable)
                != crate::relater::Ternary::Related
            || !self.has_empty_object_intersection(right_type)
        {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(right) else { return };
        let span = self.error_span(right);
        let text = self.type_to_string(right_type);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::TYPE_0_MAY_REPRESENT_A_PRIMITIVE_VALUE_WHICH_IS_NOT_PERMITTED_AS_THE_RIGHT_OPERAND_OF_THE_IN_OPERATOR,
                span,
                [text],
            ),
        );
    }

    /// `hasEmptyObjectIntersection` (`checker.go:13111`).
    fn has_empty_object_intersection(&mut self, t: TypeId) -> bool {
        let parts = match &self.type_of(t).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![t],
        };
        let unknown_empty_object = self.intrinsics().unknown_empty_object;
        parts.into_iter().any(|part| {
            part == unknown_empty_object
                || (self.type_of(part).flags.contains(TypeFlags::INTERSECTION) && {
                    let constraint = self.base_constraint_or_type(part);
                    self.is_empty_anonymous_object_type(constraint)
                })
        })
    }

    /// `checkForOfStatement` (`checker.go:4032`), the reference-expression arm:
    /// `for (v of xs)` relates the iterated element type to `v`'s type through
    /// `checkTypeAssignableToAndOptionallyElaborate`, error node the left
    /// expression. A declaration list is `checkVariableDeclarationList`'s, and
    /// an array/object literal is `checkDestructuringAssignment`'s, so neither
    /// is this position. An unresolved iterated type (native nil) is silent.
    ///
    /// `for await` iterates `getIteratedTypeOrElementType`'s async arm, which
    /// this port's `for_of_element_type` does not answer, so it is declined.
    /// The left-hand type comes from [`Checker::assignment_target_type`], the
    /// same declared-type reader (and declines) the `=` arm uses.
    pub(crate) fn check_for_of_reference_assignment(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::ForInOrOfStatement(statement)) = self.node_map.get(node) else { return };
        if statement.kind.kind != SyntaxKind::ForOfStatement || statement.await_modifier.is_some() {
            return;
        }
        let (Some(initializer), Some(expression)) = (statement.initializer, statement.expression)
        else {
            return;
        };
        let (Some(left_id), Some(right_id)) = (initializer.node_id(), expression.node_id()) else {
            return;
        };
        if matches!(
            self.nodes.kind(left_id),
            SyntaxKind::VariableDeclarationList
                | SyntaxKind::ArrayLiteralExpression
                | SyntaxKind::ObjectLiteralExpression
        ) {
            return;
        }
        // `checkReferenceAssignment`'s `checkReferenceExpression` gate.
        if !self.is_assignable_reference(left_id) {
            return;
        }
        let Some(target) = self.assignment_target_type(left_id) else { return };
        let iterable = self.check_expression_at_node(right_id);
        let Some(source) = self.for_of_element_type(iterable) else { return };
        self.report_assignability_failure(left_id, right_id, source, target);
    }

    /// `checkVariableLikeDeclaration` (`checker.go:9967`) — the annotation
    /// against the initialiser.
    ///
    /// The error node is the **declaration**, and this port's span for a
    /// `VariableDeclaration` starts at its name, which is what
    /// `arrayAssignmentTest1.ts(46,5)` records for
    /// `var i1_error: I1 = [];` — column 5 is the `i`.
    pub(crate) fn check_variable_like_declaration(
        &mut self,
        node: NodeId,
        declaration: &tsr_ast::VariableDeclaration<'a>,
        ambient: bool,
    ) {
        if ambient
            || (self.file_has_parse_errors && !self.has_complete_source_variable_initializer(node))
        {
            return;
        }
        let annotation = declaration.r#type.or_else(|| self.jsdoc_type_annotation(node));
        let (Some(annotation), Some(initializer)) = (annotation, declaration.initializer) else {
            return;
        };
        let target = self.get_type_from_type_node(annotation);
        let source = self.check_expression(initializer);
        let Some(initializer_id) = initializer.node_id() else { return };
        // §73: the elaboration reports the member instead of the outer message.
        let before = self.diagnostics.len();
        self.check_excess_properties(target, initializer_id);
        if self.diagnostics.len() != before {
            return;
        }
        self.report_assignability_failure(node, initializer_id, source, target);
    }

    /// Native `checkVariableLikeDeclaration` (5b1047d1, checker.go:5790–5945)
    /// has no whole-file syntax gate. This port may reuse its existing suppliers
    /// only for positively written leaves and complete ordinary owners.
    /// Error flags and absent optional children do not certify syntax. Source
    /// belongs to the exact host/table/file. The leaf scan remains bounded;
    /// function headers and registered direct children certify enclosing scope.
    fn has_complete_source_variable_initializer(&self, node: NodeId) -> bool {
        let certify = || -> Option<()> {
            let list = self.nodes.parent(node)?;
            let statement = self.nodes.parent(list)?;
            let owner = self.nodes.parent(statement)?;
            if self.nodes.kind(list) != SyntaxKind::VariableDeclarationList
                || self.nodes.kind(statement) != SyntaxKind::VariableStatement
            {
                return None;
            }
            let source = match self.nodes.kind(owner) {
                SyntaxKind::SourceFile => self.module_host?.source_text(owner, self.nodes)?,
                SyntaxKind::Block => {
                    let parent = self.nodes.parent(owner)?;
                    let (function, body) = if self.nodes.kind(parent) == SyntaxKind::Block {
                        (self.nodes.parent(parent)?, parent)
                    } else {
                        (parent, owner)
                    };
                    let (source, registered_body) = self.source_of_complete_function(function)?;
                    if body != registered_body {
                        return None;
                    }
                    if owner != body {
                        let Node::Block(block) = self.node_map.get(owner)? else { return None };
                        if !self.has_complete_written_block(block, body, source) {
                            return None;
                        }
                    }
                    source
                }
                _ => return None,
            };
            let mut statement_scanner =
                tsr_scanner::Scanner::new(source.get(self.nodes.span(statement).start as usize..)?);
            if !matches!(
                statement_scanner.scan().kind,
                SyntaxKind::VarKeyword | SyntaxKind::LetKeyword | SyntaxKind::ConstKeyword
            ) {
                return None;
            }
            self.has_complete_written_initializer(node, source).then_some(())
        };
        certify().is_some()
    }

    /// `SourceFile`-owned, ordinary named functions only. Witness the written
    /// header and every admitted parameter's interior, not just its span. The
    /// body must have its own closing token outside every direct child.
    fn source_of_complete_function(&self, node: NodeId) -> Option<(&str, NodeId)> {
        use tsr_ast::FunctionBody;
        use tsr_scanner::{Scanner, TokenFlags};

        let Node::FunctionDeclaration(function) = self.node_map.get(node)? else { return None };
        let file = self.nodes.parent(node)?;
        if self.nodes.kind(file) != SyntaxKind::SourceFile
            || !function.modifiers.is_empty()
            || !function.type_parameters.is_empty()
            || function.asterisk_token.is_some()
        {
            return None;
        }
        let source = self.module_host?.source_text(file, self.nodes)?;
        let name = function.name?.node_id?;
        if self.nodes.parent(name) != Some(node) {
            return None;
        }
        let FunctionBody::Block(body) = function.body?;
        let body_id = body.node_id?;
        let body_span = self.nodes.span(body_id);
        let function_span = self.nodes.span(node);
        if body_span.end != function_span.end
            || !self.has_complete_written_block(body, node, source)
        {
            return None;
        }
        let mut scanner =
            Scanner::new(source.get(function_span.start as usize..body_span.start as usize)?);
        let keyword = scanner.scan();
        if keyword.kind != SyntaxKind::FunctionKeyword
            || keyword.span != tsr_core::Span::new(0, 8)
            || keyword.flags.contains(TokenFlags::UNICODE_ESCAPE)
        {
            return None;
        }
        let name_token = scanner.scan();
        let name_span = self.nodes.span(name);
        if name_token.kind != SyntaxKind::Identifier
            || name_token.flags.contains(TokenFlags::UNICODE_ESCAPE)
            || name_token.span.start + function_span.start != name_span.start
            || name_token.span.end + function_span.start != name_span.end
            || scanner.scan().kind != SyntaxKind::OpenParenToken
        {
            return None;
        }
        for (index, parameter) in function.parameters.iter().enumerate() {
            let id = parameter.node_id?;
            if self.nodes.parent(id) != Some(node)
                || !self.has_complete_written_initializer(id, source)
                || (index != 0 && scanner.scan().kind != SyntaxKind::CommaToken)
            {
                return None;
            }
            let span = self.nodes.span(id);
            let mut token = scanner.scan();
            if token.span.start + function_span.start != span.start {
                return None;
            }
            // The independent leaf certificate bounds this complete interior
            // to nine written tokens; do not skip recovered parameter text.
            while token.span.end + function_span.start < span.end {
                if token.kind == SyntaxKind::EndOfFile {
                    return None;
                }
                token = scanner.scan();
            }
            if token.span.end + function_span.start != span.end {
                return None;
            }
        }
        if scanner.scan().kind != SyntaxKind::CloseParenToken
            || scanner.scan().kind != SyntaxKind::EndOfFile
            || !scanner.diagnostics().is_empty()
        {
            return None;
        }
        Some((source, body_id))
    }

    fn has_complete_written_block(
        &self,
        block: &tsr_ast::Block<'_>,
        parent: NodeId,
        source: &str,
    ) -> bool {
        let certify = || -> Option<()> {
            let id = block.node_id?;
            let span = self.nodes.span(id);
            if self.nodes.parent(id) != Some(parent) {
                return None;
            }
            let text = source.get(span.start as usize..span.end as usize)?;
            let mut opening = tsr_scanner::Scanner::new(text);
            let token = opening.scan();
            if token.kind != SyntaxKind::OpenBraceToken || token.span != tsr_core::Span::new(0, 1) {
                return None;
            }
            let mut end = span.start + 1;
            for statement in block.statements {
                let child = Node::from(*statement).node_id()?;
                let child_span = self.nodes.span(child);
                if self.nodes.parent(child) != Some(id)
                    || child_span.start < end
                    || child_span.start >= child_span.end
                    || child_span.end > span.end
                {
                    return None;
                }
                end = child_span.end;
            }
            // Start after the last child, not at the final character: an
            // initializer's '}' or a comment's '}' cannot close this owner.
            let mut closing =
                tsr_scanner::Scanner::new(source.get(end as usize..span.end as usize)?);
            let token = closing.scan();
            if token.kind != SyntaxKind::CloseBraceToken
                || token.span.end + end != span.end
                || closing.scan().kind != SyntaxKind::EndOfFile
                || !closing.diagnostics().is_empty()
            {
                return None;
            }
            Some(())
        };
        certify().is_some()
    }

    fn has_complete_source_parameter_initializer(&self, node: NodeId) -> bool {
        let certify = || -> Option<()> {
            let Node::ParameterDeclaration(_) = self.node_map.get(node)? else { return None };
            let function_id = self.nodes.parent(node)?;
            self.source_of_complete_function(function_id)?;
            let Node::FunctionDeclaration(function) = self.node_map.get(function_id)? else {
                return None;
            };
            function
                .parameters
                .iter()
                .any(|parameter| parameter.node_id == Some(node))
                .then_some(())
        };
        certify().is_some()
    }

    /// The written leaf is independent of whether its certified owner is a
    /// source statement, a function body, or an implemented function's parameter.
    /// It does not certify that enclosing owner by itself.
    fn has_complete_written_initializer(&self, node: NodeId, source: &str) -> bool {
        use tsr_ast::{BindingName, EntityName, Expression, FunctionBody, TypeNode};
        use tsr_core::Span;
        use tsr_scanner::{Scanner, TokenFlags};

        let certify = || -> Option<()> {
            let (name, annotation, initializer, parameter) = match self.node_map.get(node)? {
                Node::VariableDeclaration(declaration) => {
                    (declaration.name, declaration.r#type, declaration.initializer, false)
                }
                Node::ParameterDeclaration(parameter)
                    if parameter.modifiers.is_empty()
                        && parameter.question_token.is_none()
                        && parameter.dot_dot_dot_token.is_none() =>
                {
                    (parameter.name, parameter.r#type, parameter.initializer, true)
                }
                _ => return None,
            };
            let owned_span = |id: Option<NodeId>, parent: NodeId| {
                let id = id?;
                let span = self.nodes.span(id);
                (self.nodes.parent(id) == Some(parent)
                    && span.start < span.end
                    && span.end as usize <= source.len())
                .then_some(span)
            };
            let BindingName::Identifier(name) = name? else { return None };
            let name_span = owned_span(name.node_id, node)?;
            let annotation = annotation?;
            let annotation_span = owned_span(annotation.node_id(), node)?;
            let annotation_kind = match annotation {
                TypeNode::TypeReferenceNode(reference) if reference.type_arguments.is_empty() => {
                    let EntityName::Identifier(name) = reference.type_name? else { return None };
                    if owned_span(name.node_id, reference.node_id?)? != annotation_span {
                        return None;
                    }
                    SyntaxKind::Identifier
                }
                TypeNode::KeywordTypeNode(keyword) => keyword.kind,
                _ => return None,
            };
            let initializer = initializer?;
            let initializer_span = owned_span(initializer.node_id(), node)?;
            let declaration_span = self.nodes.span(node);
            if declaration_span.start != name_span.start
                || declaration_span.end != initializer_span.end
            {
                return None;
            }
            let mut expected = vec![
                SyntaxKind::Identifier,
                SyntaxKind::ColonToken,
                annotation_kind,
                SyntaxKind::EqualsToken,
            ];
            let (child_extent, simple_new) = match initializer {
                Expression::ObjectLiteralExpression(object) if object.properties.is_empty() => {
                    expected.extend([SyntaxKind::OpenBraceToken, SyntaxKind::CloseBraceToken]);
                    (None, false)
                }
                Expression::FunctionExpression(function) if function.parameters.is_empty() => {
                    let FunctionBody::Block(body) = function.body?;
                    if !body.statements.is_empty() {
                        return None;
                    }
                    let span = owned_span(body.node_id, function.node_id?)?;
                    expected.extend([
                        SyntaxKind::FunctionKeyword,
                        SyntaxKind::OpenParenToken,
                        SyntaxKind::CloseParenToken,
                        SyntaxKind::OpenBraceToken,
                        SyntaxKind::CloseBraceToken,
                    ]);
                    (Some((7, 8, span)), false)
                }
                Expression::NewExpression(new)
                    if new.arguments.is_empty() && new.type_arguments.is_empty() =>
                {
                    let Expression::Identifier(callee) = new.expression? else { return None };
                    let span = owned_span(callee.node_id, new.node_id?)?;
                    expected.extend([SyntaxKind::NewKeyword, SyntaxKind::Identifier]);
                    (Some((5, 5, span)), true)
                }
                _ => return None,
            };
            let mut scanner = Scanner::new(source.get(declaration_span.start as usize..)?);
            let mut tokens = Vec::new();
            for _ in 0..11 {
                let token = scanner.scan();
                if token.kind == SyntaxKind::EndOfFile
                    || token.span.start + declaration_span.start >= declaration_span.end
                {
                    let boundary = if parameter {
                        matches!(token.kind, SyntaxKind::CommaToken | SyntaxKind::CloseParenToken)
                    } else {
                        matches!(
                            token.kind,
                            SyntaxKind::EndOfFile
                                | SyntaxKind::SemicolonToken
                                | SyntaxKind::CommaToken
                                | SyntaxKind::CloseBraceToken
                        ) || token.has_preceding_line_break()
                    };
                    if !boundary {
                        return None;
                    }
                    break;
                }
                if token.flags.contains(TokenFlags::UNICODE_ESCAPE) {
                    return None;
                }
                tokens.push(token);
            }
            if !scanner.diagnostics().is_empty() {
                return None;
            }
            // Empty AST argument slices do not prove written, closed ().
            if simple_new && tokens.len() == expected.len() + 2 {
                expected.extend([SyntaxKind::OpenParenToken, SyntaxKind::CloseParenToken]);
            }
            // Native isBindingIdentifier (parser.go:6262) accepts keyword
            // tokens after LastReservedWord as Identifier bindings. Only the
            // original binding name gets this rule, not a type or callee name.
            if let Some(token) = tokens.first()
                && token.kind.is_keyword()
                && token.kind > SyntaxKind::LAST_RESERVED_WORD
                && source.get(name_span.start as usize..name_span.end as usize) == Some(name.text)
            {
                expected[0] = token.kind;
            }
            if !tokens.iter().map(|token| token.kind).eq(expected) {
                return None;
            }
            let absolute = |index: usize| {
                let span = tokens[index].span;
                Span::new(span.start + declaration_span.start, span.end + declaration_span.start)
            };
            if absolute(0) != name_span
                || absolute(2) != annotation_span
                || absolute(4).start != initializer_span.start
                || absolute(tokens.len() - 1).end != initializer_span.end
            {
                return None;
            }
            if let Some((first, last, span)) = child_extent
                && Span::new(absolute(first).start, absolute(last).end) != span
            {
                return None;
            }
            Some(())
        };
        certify().is_some()
    }

    /// `checkVariableLikeDeclaration`'s other two callers: a **property
    /// declaration** and a **parameter default**.
    ///
    /// Both are the same shape as the variable arm — a written annotation and an
    /// initialiser — and both report at the declaration node. They are separate
    /// only because the node kinds are, and each carries one decline the
    /// variable arm does not need: a property declaration with a `declare` or
    /// `abstract` modifier has no initialiser to check, and an **optional**
    /// parameter's default is compared against the type *without* `undefined`
    /// (`checker.go:9993`), which this port does not strip.
    pub(crate) fn check_annotated_initializer(&mut self, node: NodeId, ambient: bool) {
        if ambient
            || self.in_js_file(node)
            || (self.file_has_parse_errors && !self.has_complete_source_parameter_initializer(node))
        {
            return;
        }
        let (annotation, initializer) = match self.node_map.get(node) {
            Some(Node::PropertyDeclaration(property)) => (property.r#type, property.initializer),
            Some(Node::ParameterDeclaration(parameter)) => {
                if parameter.question_token.is_some() || parameter.dot_dot_dot_token.is_some() {
                    return;
                }
                (parameter.r#type, parameter.initializer)
            }
            _ => return,
        };
        let (Some(annotation), Some(initializer)) = (annotation, initializer) else { return };
        let target = self.get_type_from_type_node(annotation);
        let source = self.check_expression(initializer);
        let Some(initializer_id) = initializer.node_id() else { return };
        // §73: the elaboration reports the member instead of the outer message.
        let before = self.diagnostics.len();
        self.check_excess_properties(target, initializer_id);
        if self.diagnostics.len() != before {
            return;
        }
        self.report_assignability_failure(node, initializer_id, source, target);
    }

    /// `checkVariableLikeDeclaration`'s binding-element default check, limited
    /// to annotated variable leaves with literal sources and string-unit targets.
    /// The symbol supplier owns default adjustment; context is not a write target.
    pub(crate) fn check_binding_element_initializer(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::BindingElement(element)) = self.node_map.get(node) else { return };
        if !matches!(element.name, Some(tsr_ast::BindingName::Identifier(_)))
            || element.dot_dot_dot_token.is_some()
            || !matches!(
                element.property_name,
                None | Some(
                    tsr_ast::PropertyName::Identifier(_)
                        | tsr_ast::PropertyName::StringLiteral(_)
                        | tsr_ast::PropertyName::NumericLiteral(_)
                )
            )
        {
            return;
        }
        let Some(pattern) = self.nodes.parent(node) else { return };
        if !matches!(
            self.nodes.kind(pattern),
            SyntaxKind::ObjectBindingPattern | SyntaxKind::ArrayBindingPattern
        ) {
            return;
        }
        let root = self.root_declaration_of(node);
        let Some(Node::VariableDeclaration(declaration)) = self.node_map.get(root) else { return };
        if declaration.r#type.is_none() {
            return;
        }
        let Some(initializer) = element.initializer else { return };
        let undefined = match initializer {
            tsr_ast::Expression::StringLiteral(_) => false,
            tsr_ast::Expression::KeywordExpression(keyword)
                if keyword.kind == SyntaxKind::NullKeyword =>
            {
                false
            }
            tsr_ast::Expression::Identifier(identifier) if identifier.text == "undefined" => true,
            // Primitive-typed expressions can still have unsupported production
            // or circularity; structured defaults widen under nullable contexts.
            _ => return,
        };
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        if self.binder.symbols().get(symbol).value_declaration != Some(node) {
            return;
        }
        // Native computes the adjusted symbol type before checking the source.
        let target = self.get_type_of_symbol(symbol);
        let supported_part = |ty| {
            let flags = self.type_of(ty).flags;
            flags == TypeFlags::STRING_LITERAL
                || flags == TypeFlags::NULL
                || flags == TypeFlags::UNDEFINED
        };
        let supported = match &self.type_of(target).data {
            TypeData::StringLiteral(_) => true,
            TypeData::Union { types, .. } => {
                types.iter().copied().all(supported_part)
                    && types.iter().any(|&ty| self.type_of(ty).flags == TypeFlags::STRING_LITERAL)
            }
            _ => false,
        };
        if !supported {
            return;
        }
        let source = self.check_expression(initializer);
        if undefined && source != self.intrinsics.undefined {
            return;
        }
        let Some(initializer_id) = initializer.node_id() else { return };
        self.report_assignability_failure(node, initializer_id, source, target);
    }

    /// `checkReturnStatement` (`checker.go:12400`) — the returned expression
    /// against the function's **written** return annotation.
    ///
    /// The error node is the **return statement**, not the expression:
    /// `arrayAssignmentTest1.ts(6,16)` for `IM1():void[] {return null;}` is
    /// column 16, which is the `r` of `return`.
    ///
    /// Only a *written* annotation is used. An inferred return type is computed
    /// from the very returns being checked, so a mismatch against it is not a
    /// diagnostic upstream would ever report.
    pub(crate) fn check_return_statement(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::ReturnStatement(statement)) = self.node_map.get(node) else { return };
        let expression = statement.expression.and_then(|expression| expression.node_id());
        let Some(container) = self.return_statement_container(node) else { return };
        match self.node_map.get(container) {
            Some(Node::SetAccessorDeclaration(_)) => return,
            Some(Node::ConstructorDeclaration(_)) => {
                if let Some(expression) = expression {
                    self.check_constructor_return(container, node, expression);
                }
                return;
            }
            _ => {}
        }
        let Some(target) = self.return_type_from_annotation(container) else { return };
        if !self.strict_null_checks
            && expression.is_none()
            && !self.type_of(target).flags.contains(TypeFlags::NEVER)
        {
            return;
        }
        if let Some(expression) = expression {
            let source = self.check_expression_at_node(expression);
            self.check_return_expression(target, node, expression, source, false);
        } else {
            let undefined = self.intrinsics().undefined;
            self.report_assignability_failure(node, node, undefined, target);
        }
    }

    /// `checkReturnStatement`'s constructor arm (`checker.go:4115`): the
    /// returned value against `getReturnTypeFromAnnotation`'s class instance
    /// type (`checker.go:20058`), and TS2409 at the return statement when that
    /// relation fails.
    fn check_constructor_return(&mut self, constructor: NodeId, node: NodeId, expression: NodeId) {
        let Some(class) = self.nodes.parent(constructor) else { return };
        let Some(symbol) = self.binder.symbol_of(class) else { return };
        let symbol = self.binder.merged_symbol(symbol);
        let target = self.get_declared_type_of_symbol(symbol);
        let source = self.check_expression_at_node(expression);
        let before = self.diagnostics.len();
        self.check_excess_properties(target, expression);
        if self.diagnostics.len() == before
            && !self.report_assignability_failure(node, expression, source, target)
        {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::new(
                &messages::RETURN_TYPE_OF_CONSTRUCTOR_SIGNATURE_MUST_BE_ASSIGNABLE_TO_THE_INSTANCE_TYPE_OF_THE_CLASS,
                span,
            ),
        );
    }

    /// `getContainingFunctionOrClassStaticBlock` for a return statement; a
    /// static block (TS18041's rule) answers `None`.
    fn return_statement_container(&self, node: NodeId) -> Option<NodeId> {
        let mut at = self.nodes.parent(node);
        while let Some(current) = at {
            match self.nodes.kind(current) {
                SyntaxKind::FunctionDeclaration
                | SyntaxKind::FunctionExpression
                | SyntaxKind::ArrowFunction
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor
                | SyntaxKind::Constructor => return Some(current),
                SyntaxKind::ClassStaticBlockDeclaration => return None,
                _ => at = self.nodes.parent(current),
            }
        }
        None
    }

    /// `getReturnTypeFromAnnotation` (`checker.go:20058`) for a non-constructor
    /// container, unwrapped as `unwrapReturnType` would for a plain function: the
    /// written annotation, or for an unannotated get accessor with a bindable
    /// name its set accessor's parameter annotation (`getAnnotatedAccessorType`).
    ///
    /// **Async and generator functions are declined**: their annotation is a
    /// `Promise<T>` or an `Iterator<…>` and the value returned is compared
    /// against `unwrapReturnType`'s unwrapped `T` after `checkAwaitedType`.
    /// The awaited type of a generic alias (`await (x as PromiseOrValue<U>)`)
    /// is not yet `Awaited<U>` here (`discriminateWithOptionalProperty2`), so
    /// relating it would be a wrong diagnostic on correct code.
    fn return_type_from_annotation(&mut self, container: NodeId) -> Option<TypeId> {
        let (annotation, generator, modifiers) = match self.node_map.get(container)? {
            Node::FunctionDeclaration(n) => (n.r#type, n.asterisk_token.is_some(), n.modifiers),
            Node::FunctionExpression(n) => (n.r#type, n.asterisk_token.is_some(), n.modifiers),
            Node::ArrowFunction(n) => (n.r#type, false, n.modifiers),
            Node::MethodDeclaration(n) => (n.r#type, n.asterisk_token.is_some(), n.modifiers),
            Node::GetAccessorDeclaration(n) => (n.r#type, false, n.modifiers),
            _ => return None,
        };
        if generator || has_async(modifiers) {
            return None;
        }
        if let Some(annotation) = annotation {
            return Some(self.get_type_from_type_node(annotation));
        }
        let annotation = self.set_accessor_parameter_annotation(container)?;
        Some(self.get_type_from_type_node(annotation))
    }

    /// `getAnnotatedAccessorType` of the set accessor paired with an
    /// unannotated get accessor: its first non-`this` parameter's annotation.
    /// A computed name only pairs when the binder bound it (`hasBindableName`).
    fn set_accessor_parameter_annotation(&self, getter: NodeId) -> Option<tsr_ast::TypeNode<'a>> {
        if self.nodes.kind(getter) != SyntaxKind::GetAccessor {
            return None;
        }
        let symbol = self.binder.symbol_of(getter)?;
        let declarations = self.binder.symbols().get(symbol).declarations.clone();
        declarations.into_iter().find_map(|declaration| {
            let Some(Node::SetAccessorDeclaration(setter)) = self.node_map.get(declaration) else {
                return None;
            };
            setter
                .parameters
                .iter()
                .find(|parameter| {
                    !matches!(parameter.name, Some(tsr_ast::BindingName::Identifier(name))
                        if name.text == "this")
                })
                .and_then(|parameter| parameter.r#type)
        })
    }

    /// `checkFunctionExpressionOrObjectLiteralMethodDeferred`
    /// (`checker.go:10206`), the concise-body arm: an arrow function whose body
    /// is an expression relates that expression, through
    /// `checkReturnExpression`, to `unwrapReturnType` of its written return
    /// annotation. The error node is the body itself. Async arrows are declined
    /// with async functions (see [`Checker::enclosing_return_annotation`]).
    pub(crate) fn check_arrow_expression_body(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::ArrowFunction(arrow)) = self.node_map.get(node) else { return };
        let (Some(annotation), Some(body)) = (arrow.r#type, arrow.body) else { return };
        let Some(body_id) = body.node_id() else { return };
        if self.nodes.kind(body_id) == SyntaxKind::Block {
            return;
        }
        if has_async(arrow.modifiers) {
            return;
        }
        let target = self.get_type_from_type_node(annotation);
        let source = self.check_expression_at_node(body_id);
        self.check_return_expression(target, body_id, body_id, source, false);
    }

    /// `checkReturnExpression` (`checker.go:4131`). A conditional expression
    /// (under parentheses) checks each branch on its own, reporting at the
    /// branch. Otherwise the error node is the return statement, or the
    /// effective expression for a concise body or a conditional branch. Async
    /// containers (the `checkAwaitedType` arm) never reach here.
    fn check_return_expression(
        &mut self,
        target: TypeId,
        node: NodeId,
        expression: NodeId,
        source: TypeId,
        in_conditional: bool,
    ) {
        let unwrapped = self.skip_outer_parentheses(expression);
        if let Some(Node::ConditionalExpression(conditional)) = self.node_map.get(unwrapped) {
            for branch in [conditional.when_true, conditional.when_false].into_iter().flatten() {
                let Some(branch_id) = branch.node_id() else { continue };
                let branch_type = self.check_expression_at_node(branch_id);
                self.check_return_expression(target, node, branch_id, branch_type, true);
            }
            return;
        }
        let effective = self.effective_check_node(expression);
        let error_node = if self.nodes.kind(node) == SyntaxKind::ReturnStatement && !in_conditional
        {
            node
        } else {
            effective
        };
        // §73: the elaboration reports the member instead of the outer message.
        let before = self.diagnostics.len();
        self.check_excess_properties(target, effective);
        if self.diagnostics.len() != before {
            return;
        }
        self.report_assignability_failure(error_node, effective, source, target);
    }

    /// `ast.SkipParentheses`.
    fn skip_outer_parentheses(&self, mut node: NodeId) -> NodeId {
        while let Some(Node::ParenthesizedExpression(inner)) = self.node_map.get(node)
            && let Some(expression) = inner.expression.and_then(|e| e.node_id())
        {
            node = expression;
        }
        node
    }

    /// `getEffectiveCheckNode` (`checker.go:9381`): `ast.SkipOuterExpressions`
    /// over parentheses and `satisfies`, repeatedly (TS files; the JS-only
    /// JSDoc-assertion exclusion never applies because JS files are declined).
    fn effective_check_node(&self, mut node: NodeId) -> NodeId {
        loop {
            let inner = match self.node_map.get(node) {
                Some(Node::ParenthesizedExpression(inner)) => inner.expression,
                Some(Node::SatisfiesExpression(inner)) => inner.expression,
                _ => return node,
            };
            match inner.and_then(|expression| expression.node_id()) {
                Some(inner) => node = inner,
                None => return node,
            }
        }
    }

    /// `checkReferenceExpression`'s verdict (`checker.go:13130`) without its
    /// reports (those are `check_reference_expression`'s): an identifier or
    /// access under assertions and parentheses, not an optional chain.
    fn is_assignable_reference(&self, node: NodeId) -> bool {
        let spine = self.skip_reference_spine(node, true);
        matches!(
            self.nodes.kind(spine),
            SyntaxKind::Identifier
                | SyntaxKind::PropertyAccessExpression
                | SyntaxKind::ElementAccessExpression
        ) && !self.spine_has_optional_chain(spine)
    }

    /// The type an assignment writes *into*, or `None` where this port declines.
    ///
    /// # Why this is `getTypeOfSymbol` and not `checkExpression`
    ///
    /// `checkIdentifier` (`checker.go:11109`) returns early with the **declared**
    /// type when the reference is a definite assignment target — the flow type
    /// is what the variable holds *now*, and an assignment writes into what it
    /// was declared as. Using the flow type instead was 16 of one measurement's
    /// wrong lines, all in `controlFlowNoImplicitAny`: `let x;` narrows to
    /// `undefined` before its first assignment, so `x = 1` read as
    /// *number not assignable to undefined*. The declared type of an auto-typed
    /// variable is `any` and upstream reports nothing — the same auto-to-any
    /// mechanism `docs/architecture/checker-notes-narrow.md` §9 measures from the
    /// `.types` side.
    fn assignment_target_type(&mut self, node: NodeId) -> Option<TypeId> {
        // `checkParenthesizedExpression` answers its operand's type, and
        // `getAssignmentTargetKind` looks through parentheses, so `(x) = ''`
        // writes into `x`'s declared type exactly as `x = ''` does.
        let node = self.skip_outer_parentheses(node);
        // **A property-access target is admitted**, and §16's third decline —
        // which refused it because a `set` accessor's write type differs from
        // its getter's — is retired by measurement: +16 cases. The divergent-
        // accessor cases it was drawn for (`divergentAccessorsTypes2`) are a
        // handful; the ordinary `obj.field = value` it was also refusing is
        // sixteen. `getWriteTypeOfSymbol` would recover the handful.
        if self.nodes.kind(node) == SyntaxKind::PropertyAccessExpression {
            // An **inaccessible** property's access answers `errorType`
            // upstream — `checkPropertyAccessExpression` returns after
            // reporting TS2341/TS2445 — so no assignment check follows it.
            // `c.y = 1` on a private accessor is TS2341 alone, and this port
            // was adding a TS2322 beside it
            // (`checker-notes-diag2.md` §70).
            if self.inaccessible_property(node).is_some() {
                return None;
            }
            let ty = self.check_expression_at_node(node);
            return (ty != self.intrinsics().error).then_some(ty);
        }
        // `leftType := c.checkExpressionEx(left, checkMode)`
        // (`checker.go:12341`): an element access in a definite
        // assignment-target position answers the write type
        // (`checkElementAccessExpression`), and an asserted reference answers
        // the assertion's type.
        //
        // **Declined: a `unique symbol` key.** The write type is
        // `getIndexedAccessType(…, AccessFlagsWriting)`, which reaches the
        // setter of a late-bound accessor pair (`getWriteTypeOfSymbol`);
        // `check_element_access_expression`'s write arm resolves literal keys
        // only and answers the getter's type here
        // (`computedPropertiesWithSetterAssignment`).
        //
        // **Declined: an element access in a JS file.** The right operand's
        // contextual type there is `getContextualTypeForAssignmentDeclaration`'s
        // JS arm, which this port does not answer for element-access
        // assignments, so `handlers[++id] = [resolve, reject]` types the
        // literal as an array instead of the target's tuple
        // (`jsDeclarationsTypedefFunction`).
        if let Some(Node::ElementAccessExpression(access)) = self.node_map.get(node)
            && let Some(index) = access.argument_expression
        {
            if self.in_js_file(node) {
                return None;
            }
            let index = self.check_expression(index);
            if self.type_of(index).flags.intersects(TypeFlags::UNIQUE_ES_SYMBOL) {
                return None;
            }
        }
        if self.nodes.kind(node) != SyntaxKind::Identifier {
            let ty = self.check_expression_at_node(node);
            return (ty != self.intrinsics().error).then_some(ty);
        }
        let Some(Node::Identifier(identifier)) = self.node_map.get(node) else { return None };
        let text = identifier.text;
        let symbol = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            node,
            text,
            SymbolFlags::VALUE | SymbolFlags::ALIAS,
        )?;
        let symbol = self.binder.merged_symbol(symbol);
        let entry = self.binder.symbols().get(symbol);
        // `checkIdentifier`'s assignment arms report TS2628/2629/2630/2631/2632
        // and return the error type, so the relation never runs for any of them.
        if entry.flags.intersects(
            SymbolFlags::ENUM
                | SymbolFlags::CLASS
                | SymbolFlags::FUNCTION
                | SymbolFlags::MODULE
                | SymbolFlags::ALIAS,
        ) {
            return None;
        }
        // A `const` target is TS2588, reported *instead of* the relation. Two
        // `var` declarations of one name share the first declaration's type
        // (`getTypeOfVariableOrParameterOrProperty`); a later conflicting one
        // is TS2403's, not this site's.
        let declarations: Vec<NodeId> = entry.declarations.to_vec();
        if declarations.is_empty()
            || declarations.iter().any(|&declaration| {
                self.declaration_is_constant(declaration)
                    || self.declaration_is_auto_typed(declaration)
            })
        {
            return None;
        }
        Some(self.get_type_of_symbol(symbol))
    }

    /// Is this an **auto-typed** declaration — `let x;`, no annotation and no
    /// initialiser?
    ///
    /// `convertAutoToAny` (`checker.go:11182`) makes such a variable's declared
    /// type `any` once flow analysis is done with it, and upstream therefore
    /// never reports an assignment into one. This port's declared type for the
    /// same declaration is `undefined`, which is the same divergence
    /// `docs/architecture/checker-notes-narrow.md` §9.1 measured from the
    /// `.types` side and **refused there** — so it is declined here rather than
    /// worked around, and the refusal keeps one owner.
    /// `controlFlowNoImplicitAny` is the whole of what it costs.
    fn declaration_is_auto_typed(&self, declaration: NodeId) -> bool {
        let Some(Node::VariableDeclaration(variable)) = self.node_map.get(declaration) else {
            return false;
        };
        if variable.r#type.is_some() {
            return false;
        }
        // `getTypeForVariableLikeDeclaration` (`checker.go`) gives the auto type
        // to `let x;` **and** to `let x = undefined;` — `controlFlowNoImplicitAny`
        // writes both spellings against the same expectation, and only the second
        // survived the first version of this predicate.
        match variable.initializer {
            None => true,
            Some(initializer) => matches!(
                initializer.node_id().and_then(|id| self.node_map.get(id)),
                Some(Node::Identifier(name)) if name.text == "undefined"
            ),
        }
    }

    /// Is this declaration a `const` (or `using`) binding?
    ///
    /// Assigning to one is TS2588 `Cannot_assign_to_0_because_it_is_a_constant`,
    /// which upstream reports at the same position and *instead of* TS2322 —
    /// `constDeclarations-access2` and its siblings.
    fn declaration_is_constant(&self, declaration: NodeId) -> bool {
        self.combined_node_flags(declaration).intersects(tsr_ast::NodeFlags::CONSTANT)
    }

    /// TS2353 — `Object literal may only specify known properties, and '{0}'
    /// does not exist in type '{1}'.`
    ///
    /// `hasExcessProperties` (`relater.go`), reached when a **fresh** object
    /// literal type is checked against a target. Upstream's freshness marker is
    /// on the type; here the question is asked of the syntax — is the expression
    /// *written* as an object literal at this position — which is the same set
    /// for every anchor this module has, because none of them is a place a
    /// literal's type can arrive already widened.
    ///
    /// The error node is the offending **property name**:
    /// `arrayCast.ts(3,23)` is the `foo` of `{ foo: "s" }`.
    ///
    /// # Only the first, exactly as upstream
    ///
    /// `hasExcessProperties` reports and returns on the first excess property it
    /// finds. Reporting every one would fail the case under the exact-multiset
    /// rule just as surely as reporting none.
    pub(crate) fn check_excess_properties(&mut self, target: TypeId, initializer: NodeId) {
        let Some(Node::ObjectLiteralExpression(literal)) = self.node_map.get(initializer) else {
            return;
        };
        // A spread contributes properties this port cannot enumerate.
        if literal.properties.iter().any(|property| {
            matches!(property, tsr_ast::ObjectLiteralElementLike::SpreadAssignment(_))
        }) {
            return;
        }
        // Index-signature-fatal completeness: an index signature on the target
        // makes every name known, so this is the predicate that must decline it
        // — the same one `crate::nonexistent_property` uses, and *not* the
        // property enumeration TS2741 uses.
        if !self.relation_members_are_complete(target) {
            return;
        }
        let Some(known) = self.relation_property_table(target) else { return };
        // An **empty** target is not an excess-property site. `class C {}` with
        // `c = { foo: '' }` reads TS2322 upstream, not TS2353
        // (`conformance/classWithEmptyBody`), because nothing about the literal
        // is assignable in the first place and the excess check only speaks when
        // the rest of the relation would have succeeded. This port runs no
        // relation here, so the emptiness test stands in for that condition —
        // and it was this rule's only loss.
        if known.is_empty() {
            return;
        }
        let names: Vec<&str> = literal
            .properties
            .iter()
            .filter_map(|property| match property {
                tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                    Some(assignment.name)
                }
                tsr_ast::ObjectLiteralElementLike::MethodDeclaration(method) => Some(method.name),
                _ => None,
            })
            .filter_map(|name| {
                let id = name.node_id()?;
                match self.node_map.get(id) {
                    Some(Node::Identifier(identifier)) => Some(identifier.text),
                    Some(Node::StringLiteral(text)) => Some(text.text),
                    _ => None,
                }
            })
            .collect();
        // A shorthand or a computed name in the literal means the name list is
        // incomplete, and an incomplete list cannot say what is *excess*.
        if names.len() != literal.properties.len() {
            return;
        }
        for name in names {
            if known.iter().any(|(seen, _)| seen == name) {
                // A **known** property is not excess; its value is checked
                // against the target's member instead.
                // `everyTypeWithAnnotationAndInvalidInitializer.ts(43,28)` is the
                // `id` of `var anObjectLiteral: I = { id: 'a string' }` — the
                // property *name*, not the value and not the declaration.
                self.check_object_literal_member(literal, target, name);
                continue;
            }
            let Some(at) = self.excess_property_name_node(literal, name) else { return };
            let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
            let span = self.error_span(at);
            let printed = self.type_to_string(target);
            // hasExcessProperties (relater.go:2714): an identifier name with a
            // spelling suggestion among the target's properties
            // (getSuggestionForNonexistentProperty) is TS2561; a string-literal
            // name is never given a suggestion.
            let candidates: Vec<&str> = known.iter().map(|(seen, _)| seen.as_str()).collect();
            let suggestion = (self.nodes.kind(at) == SyntaxKind::Identifier)
                .then(|| crate::check::spelling_suggestion(name, &candidates))
                .flatten()
                .map(str::to_string);
            let diagnostic = if let Some(suggestion) = suggestion {
                Diagnostic::with_args(
                    &messages::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_BUT_0_DOES_NOT_EXIST_IN_TYPE_1_DID_YOU_MEAN_TO_WRITE_2,
                    span,
                    [name.to_string(), printed, suggestion],
                )
            } else {
                Diagnostic::with_args(
                    &messages::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_0_DOES_NOT_EXIST_IN_TYPE_1,
                    span,
                    [name.to_string(), printed],
                )
            };
            self.report(file, diagnostic);
            return;
        }
    }

    /// One known property of an object literal, against the target's member of
    /// the same name.
    ///
    /// `checkObjectLiteral`'s per-property contextual check, reduced to the
    /// comparison: the contextual type is the target the caller already has, and
    /// the verdict is the same `relate_ternary` every other rule in this module
    /// reads.
    fn check_object_literal_member(
        &mut self,
        literal: &tsr_ast::ObjectLiteralExpression<'_>,
        target: TypeId,
        name: &str,
    ) {
        let Some(at) = self.excess_property_name_node(literal, name) else { return };
        let Some(value) = self.object_literal_member_value(literal, name) else { return };
        let Some(member) = self.get_type_of_property_of_type(target, name) else { return };
        let source = self.check_expression_at_node(value);
        self.report_assignability_failure(at, value, source, member);
    }

    /// The initialiser node of the literal's property called `name`.
    fn object_literal_member_value(
        &self,
        literal: &tsr_ast::ObjectLiteralExpression<'_>,
        name: &str,
    ) -> Option<NodeId> {
        for property in literal.properties {
            let tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) = property else {
                continue;
            };
            let id = assignment.name.node_id()?;
            let text = match self.node_map.get(id) {
                Some(Node::Identifier(identifier)) => identifier.text,
                Some(Node::StringLiteral(literal)) => literal.text,
                _ => continue,
            };
            if text == name {
                return assignment.initializer.and_then(|value| value.node_id());
            }
        }
        None
    }

    /// [`Checker::check_expression`] reached from a [`NodeId`] — ADR-0013's
    /// read-drop-recurse, as [`crate::index_constraint`] does for type nodes.
    pub(crate) fn check_expression_at_node(&mut self, node: NodeId) -> TypeId {
        let error = self.intrinsics().error;
        let Some(typed) = self.node_map.get(node) else { return error };
        let Ok(expression) = tsr_ast::Expression::try_from(typed) else { return error };
        self.check_expression(expression)
    }

    /// The name node of the literal's property called `name`.
    pub(crate) fn excess_property_name_node(
        &self,
        literal: &tsr_ast::ObjectLiteralExpression<'_>,
        name: &str,
    ) -> Option<NodeId> {
        for property in literal.properties {
            let written = match property {
                tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                    assignment.name
                }
                tsr_ast::ObjectLiteralElementLike::MethodDeclaration(method) => method.name,
                _ => continue,
            };
            let id = written.node_id()?;
            let text = match self.node_map.get(id) {
                Some(Node::Identifier(identifier)) => identifier.text,
                Some(Node::StringLiteral(literal)) => literal.text,
                _ => continue,
            };
            if text == name {
                return Some(id);
            }
        }
        None
    }

    /// `reportUnmatchedProperty` (`relater.go:4345`): the required own keys
    /// absent from a source whose property table is complete.
    ///
    /// **Reported at the same position TS2322 would be**, and *instead of* it:
    /// `assignmentCompat1.ts(4,1)` is the `x` of `x = y`, and reporting TS2322
    /// there is a wrong code at a right position.
    ///
    /// # Why this can run where §16's gate declines
    ///
    /// §16's gate admits only types whose assignability is settled by flags,
    /// because the *structural relation* is incomplete. This asks a different
    /// question — **is a required property absent** — and that one is answered by
    /// the member tables alone, which [`crate::member_completeness`] can now
    /// certify. No relation runs, so no incompleteness leaks.
    ///
    /// Complete declared object tables exclude arrays and tuples, so native's
    /// `tryElaborateArrayLikeErrors` permits the multi-property head here. No
    /// array/tuple elaboration is inferred from incomplete tables.
    fn missing_required_property(&mut self, source: TypeId, target: TypeId) -> Option<Vec<String>> {
        let target_properties = self.relation_property_table(target)?;
        let source_properties = self.relation_property_table(source)?;
        if self.fresh_object_literal_types.contains(&source) {
            // hasExcessProperties precedes reportUnmatchedProperty. Captured
            // names alone cannot license a missing head when a written key
            // belongs to that earlier, possibly unported error path.
            let TypeData::Named { members: Some(owner), .. } = self.store.get(source).data else {
                return None;
            };
            let &[literal] = self.binder.symbols().get(owner).declarations.as_slice() else {
                return None;
            };
            if self.nodes.kind(literal) != SyntaxKind::ObjectLiteralExpression {
                return None;
            }
            let properties = self.anonymous_properties.get(&source)?.0.clone();
            for property in properties {
                // shouldCheckAsExcessProperty compares declaration parents:
                // a spread's keys retain their original declaration, and a
                // later spread can replace an earlier written property's origin.
                let origin = property.origin?;
                let declaration = self.binder.symbols().get(origin).value_declaration?;
                if self.nodes.parent(declaration) != Some(literal) {
                    continue;
                }
                // isKnownProperty reads the object's own/inherited table, not
                // global Object augmentation. The certified table also keeps
                // a mapped container's keys distinct from its origin's keys.
                if target_properties.iter().any(|(name, _)| name == &property.name) {
                    continue;
                }
                // None is an unresolved index table, not proof of no index.
                self.get_index_infos_of_type(target)?;
                let key = self.store.intern_literal(
                    TypeFlags::STRING_LITERAL,
                    TypeData::StringLiteral(property.name),
                    false,
                );
                self.get_applicable_index_info(target, key)?;
            }
        }
        let mut missing = Vec::new();
        for (name, optional) in target_properties {
            if !optional
                && !source_properties.iter().any(|(seen, _)| seen == &name)
                // getUnmatchedProperties uses getPropertyOfType, whose misses
                // can still be supplied by Function/Object augmentation.
                && self.get_type_of_property_of_type(source, &name).is_none()
            {
                missing.push(name);
            }
        }
        (!missing.is_empty()).then_some(missing)
    }

    /// `reportUnmatchedProperty`'s messages (`relater.go:4345`): TS2741 for one
    /// missing property, else TS2739, or TS2740 past five names.
    fn report_missing_properties(
        &mut self,
        file: NodeId,
        span: tsr_core::Span,
        source: TypeId,
        target: TypeId,
        properties: &[String],
    ) {
        let source_text = self.type_to_string(source);
        let target_text = self.type_to_string(target);
        let (message, args) = if properties.len() == 1 {
            (
                &messages::PROPERTY_0_IS_MISSING_IN_TYPE_1_BUT_REQUIRED_IN_TYPE_2,
                vec![properties[0].clone(), source_text, target_text],
            )
        } else if properties.len() > 5 {
            (
                &messages::TYPE_0_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_1_COLON_2_AND_3_MORE,
                vec![
                    source_text,
                    target_text,
                    properties[..4].join(", "),
                    (properties.len() - 4).to_string(),
                ],
            )
        } else {
            (
                &messages::TYPE_0_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_1_COLON_2,
                vec![source_text, target_text, properties.join(", ")],
            )
        };
        self.report(file, Diagnostic::with_args(message, span, args));
    }

    /// TS2345 at an argument position — the same verdict machinery as
    /// [`Checker::report_assignability_failure`] with a different head code.
    /// Answers **whether it reported**, so the caller can stop:
    /// `getSignatureApplicabilityError` returns on the first failing argument
    /// (`checker-notes-diag2.md` §59).
    pub(crate) fn report_argument_failure(
        &mut self,
        at: NodeId,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        if self.nodes.kind(at) == SyntaxKind::ObjectLiteralExpression
            && self.type_of(target).flags.contains(TypeFlags::UNION)
        {
            return false;
        }
        if !self.assignability_pair_is_reportable(source, target) {
            return false;
        }
        let not_related = self.relate_ternary(source, target, crate::relater::Relation::Assignable)
            == crate::relater::Ternary::NotRelated;
        if !not_related && !self.object_against_primitive(source, target) {
            return false;
        }
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return false };
        let span = self.error_span(at);
        // reportRelationError suppresses the TS2345 head when the chain ends in
        // the pair's missing-property message (relater.go:4751), exactly as it
        // does for TS2322; a fresh literal keeps the written-key guard.
        if not_related
            && let Some(properties) = self
                .missing_required_property(source, target)
                .or_else(|| self.unmatched_property_report(source, target))
        {
            self.report_missing_properties(file, span, source, target, &properties);
            return true;
        }
        let displayed_source = self.assignability_source_for_error_display(source, target);
        let source_text = self.type_to_string(displayed_source);
        let target_text = self.type_to_string(target);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::ARGUMENT_OF_TYPE_0_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE_1,
                span,
                [source_text, target_text],
            ),
        );
        true
    }

    /// Report the assignability failure at `span`, choosing the code the way
    /// upstream's relation does: absent required properties are TS2741/2739/2740,
    /// a direct exact-optional missing-property write is TS2412, a whole-object
    /// exact-optional mismatch is TS2375, and other failures are TS2322.
    pub(crate) fn report_assignability_failure(
        &mut self,
        at: NodeId,
        source_node: NodeId,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        self.report_assignability_failure_with(at, Some(source_node), source, target)
    }

    /// [`Checker::report_assignability_failure`]; with no `source_node` the
    /// expression is not elaborated — `checkTypeRelatedToEx` reached from an
    /// elaboration that already chose its error node.
    fn report_assignability_failure_with(
        &mut self,
        at: NodeId,
        source_node: Option<NodeId>,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let span = self.error_span(at);
        self.report_relation_failure(at, span, source_node, source, target, None)
    }

    /// [`Checker::report_assignability_failure`] with a caller's head message
    /// and error span: `checkTypeAssignableToAndOptionallyElaborate`'s
    /// `headMessage` (relater.go). `reportRelationError` (relater.go:4751)
    /// uses the head as given; the TS2322 defaults (exact-optional variants)
    /// apply only when there is none. A missing-property chain still replaces
    /// the head (relater.go:4816), since no caller here passes a conversion or
    /// interface-implementation message.
    pub(crate) fn report_relation_failure(
        &mut self,
        at: NodeId,
        span: tsr_core::Span,
        source_node: Option<NodeId>,
        source: TypeId,
        target: TypeId,
        head: Option<&'static tsr_diagnostics::Message>,
    ) -> bool {
        // An object literal against a **union** target is the excess-property
        // and discriminated-union machinery
        // (`getMatchingUnionConstituentForObjectLiteral`,
        // `findMatchingDiscriminantType`), and upstream reports TS2353 / TS2561 /
        // TS2739 there rather than TS2322. Asked of the *syntax* because the
        // literal's type is synthesised and carries no declaration to enumerate:
        // 31 wrong lines across six cases — `excessPropertyCheckWithUnions`,
        // `assignmentCompatWithDiscriminatedUnion`, both `missingDiscriminants`.
        // §172's probe. Records the gate that decided every position this site
        // was asked about, so the unemitted TS2322 population can be split into
        // "never visited" and "visited and declined". Off unless
        // `TSR_ASSIGN_PROBE` is set; the `OnceLock` keeps it to one `getenv`.
        macro_rules! probe {
            ($verdict:expr) => {
                if assign_probe_enabled()
                    && let Some(probe_file) = self.source_file_of_for_diagnostics(at)
                {
                    self.assignability_probe.push((probe_file, span, $verdict));
                }
            };
        }
        if let Some(node) = source_node
            && self.nodes.kind(node) == SyntaxKind::ObjectLiteralExpression
            && self.type_of(target).flags.contains(TypeFlags::UNION)
        {
            // elaborateObjectLiteral against a union reads each member through
            // getBestMatchingType; where that choice is certain, elaborate.
            if let Some(best) = self.best_matching_object_constituent(source, target)
                && self.relate_ternary(source, target, crate::relater::Relation::Assignable)
                    == crate::relater::Ternary::NotRelated
                && self.elaborate_object_literal(node, source, best)
            {
                probe!(PROBE_REPORTED);
                return true;
            }
            probe!(PROBE_OBJECT_LITERAL_UNION);
            return false;
        }
        // `elaborateError` (`relater.go:440`) runs **before** the whole-expression
        // report and, when it speaks, `checkTypeRelatedToEx` stays silent. The
        // hand-off is exclusive by construction here because both live in this
        // one function: elaborating returns, it does not fall through. §176.
        if source_node.is_some_and(|node| self.elaborate_error(node, source, target)) {
            probe!(PROBE_REPORTED);
            return true;
        }
        if !self.assignability_pair_is_reportable(source, target) {
            probe!(PROBE_PAIR_NOT_REPORTABLE);
            return false;
        }
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return false };
        if REPORT_MISSING_REQUIRED_PROPERTY
            && let Some(properties) = self.missing_required_property(source, target)
        {
            probe!(PROBE_REPORTED);
            self.report_missing_properties(file, span, source, target, &properties);
            return true;
        }
        // **`relate_ternary`, not `is_type_assignable_to`.** The relater is
        // three-valued (`crate::relater::Ternary`) and its own doc comment names
        // the caller this distinction exists for: *"one that acts on a
        // negative"*. TS2322 is exactly that caller, and the binary projection —
        // which collapses `Unknown` into `false` — is what produced this
        // module's first measurement of **947 right against 988 wrong**. Every
        // undecidable pair was being reported as an error.
        let not_related = self.relate_ternary(source, target, crate::relater::Relation::Assignable)
            == crate::relater::Ternary::NotRelated;
        if !not_related && !self.object_against_primitive(source, target) {
            probe!(PROBE_RELATION_DECLINED);
            return false;
        }
        probe!(PROBE_REPORTED);
        if not_related && let Some(properties) = self.unmatched_property_report(source, target) {
            self.report_missing_properties(file, span, source, target, &properties);
            return true;
        }
        let displayed_source = self.assignability_source_for_error_display(source, target);
        let source_text = self.type_to_string(displayed_source);
        let target_text = self.type_to_string(target);
        let message = if let Some(head) = head {
            head
        } else if self.exact_optional_property_assignment_mismatch(at, source) {
            &messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1_WITH_EXACTOPTIONALPROPERTYTYPES_COLON_TRUE_CONSIDER_ADDING_UNDEFINED_TO_THE_TYPE_OF_THE_TARGET
        } else if source_text != target_text && self.exact_optional_object_mismatch(source, target)
        {
            &messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1_WITH_EXACTOPTIONALPROPERTYTYPES_COLON_TRUE_CONSIDER_ADDING_UNDEFINED_TO_THE_TYPES_OF_THE_TARGET_S_PROPERTIES
        } else {
            &messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1
        };
        self.report(file, Diagnostic::with_args(message, span, [source_text, target_text]));
        true
    }

    /// `getExactOptionalUnassignableProperties` (`checker.go:13115`): inspect
    /// corresponding read members, not assignability or optional symbol flags.
    /// This port resolves instantiated members through the concrete receiver
    /// rather than native's instantiated property symbols. As in
    /// `containsMissingType`, explicit undefined ahead of missing admits it.
    fn exact_optional_object_mismatch(&mut self, source: TypeId, target: TypeId) -> bool {
        if !self.exact_optional_property_types
            || (self.tuple_element_lists.contains_key(&source)
                && self.tuple_element_lists.contains_key(&target))
        {
            return false;
        }
        let Some(names) = self.get_property_names_of_type(target) else { return false };
        names.iter().any(|name| {
            let Some(source_property) = self.get_type_of_property_of_type(source, name) else {
                return false;
            };
            if !self.maybe_type_of_kind(source_property, TypeFlags::UNDEFINED) {
                return false;
            }
            let Some(target_property) = self.get_type_of_property_of_type(target, name) else {
                return false;
            };
            target_property == self.intrinsics.missing
                || matches!(&self.type_of(target_property).data, TypeData::Union { types, .. }
                    if types.first() == Some(&self.intrinsics.missing))
        })
    }

    /// `checkAssignmentOperator` (`checker.go:12777`): the TS2412 head message
    /// reads the concrete receiver's property before the write type removes
    /// missing. `containsMissingType` (`checker.go:1250`) tests the first union
    /// constituent: explicit undefined can precede missing and admits undefined.
    fn exact_optional_property_assignment_mismatch(&mut self, at: NodeId, source: TypeId) -> bool {
        if !self.exact_optional_property_types
            || !self.maybe_type_of_kind(source, TypeFlags::UNDEFINED)
        {
            return false;
        }
        let Some(Node::PropertyAccessExpression(access)) = self.node_map.get(at) else {
            return false;
        };
        let (Some(receiver), Some(name)) = (access.expression, access.name) else {
            return false;
        };
        let name = match name {
            tsr_ast::MemberName::Identifier(name) => name.text,
            tsr_ast::MemberName::PrivateIdentifier(name) => name.text,
        };
        let receiver = self.check_expression(receiver);
        let Some(property_type) = self.get_type_of_property_of_type(receiver, name) else {
            return false;
        };
        property_type == self.intrinsics.missing
            || matches!(&self.type_of(property_type).data, TypeData::Union { types, .. }
                if types.first() == Some(&self.intrinsics.missing))
    }

    /// `reportRelationError`: generalize literal source names only for targets
    /// that cannot contain top-level singleton types. This changes the display,
    /// never the types passed to the relation.
    fn assignability_source_for_error_display(&mut self, source: TypeId, target: TypeId) -> TypeId {
        let source_type = self.type_of(source);
        let is_literal = source_type.flags.intersects(TypeFlags::BOOLEAN | TypeFlags::UNIT)
            || matches!(&source_type.data, TypeData::Union { types, .. }
                if types.iter().all(|&ty| self.type_of(ty).flags.intersects(TypeFlags::UNIT)));
        if !is_literal
            || self.type_of(target).flags.contains(TypeFlags::NEVER)
            || self.type_could_have_top_level_singleton_types(target, &mut Vec::new())
        {
            source
        } else {
            self.get_base_type_of_literal_type(source)
        }
    }

    /// `typeCouldHaveTopLevelSingletonTypes` (`relater.go:1305`). Constraint
    /// resolution stays in this checker's existing constraint domain.
    fn type_could_have_top_level_singleton_types(
        &mut self,
        target: TypeId,
        active: &mut Vec<TypeId>,
    ) -> bool {
        let flags = self.type_of(target).flags;
        if flags.contains(TypeFlags::BOOLEAN) {
            return false;
        }
        let singleton = flags
            .intersects(TypeFlags::UNIT | TypeFlags::TEMPLATE_LITERAL | TypeFlags::STRING_MAPPING);
        if active.contains(&target) {
            return singleton;
        }
        active.push(target);
        let result = if let TypeData::Union { types, .. } | TypeData::Intersection { types, .. } =
            self.type_of(target).data.clone()
        {
            types.iter().any(|&ty| self.type_could_have_top_level_singleton_types(ty, active))
        } else if flags.intersects(TypeFlags::INSTANTIABLE)
            && let Some(constraint) = self.base_constraint_of_type(target)
            && constraint != target
        {
            self.type_could_have_top_level_singleton_types(constraint, active)
        } else {
            singleton
        };
        active.pop();
        result
    }

    /// The declines that survive `relate_ternary` — situations where the
    /// relation answers a confident `NotRelated` that upstream would not.
    ///
    /// # What changed, and why this list is now short
    ///
    /// This gate used to admit **primitives only**, because the rule read
    /// `is_type_assignable_to` and that binary projection collapses
    /// `Ternary::Unknown` — *"this port cannot decide"* — into `false`. Every
    /// undecidable pair was reported as an error, which is the whole of the
    /// 988-wrong measurement in §16's build 0, and the primitives gate was the
    /// bound that made it survivable.
    ///
    /// `relate_ternary` reports the negative only when the relater is
    /// **entitled** to say so, which is precisely what
    /// `crate::relater::Ternary`'s own doc comment says it exists for. With it
    /// the gate no longer has to model the relation's incompleteness at all, and
    /// what is left is three places where the relater is confidently wrong
    /// rather than undecided:
    ///
    /// - **`unknown` on either side.** Its arms are not ported and it produced
    ///   25 wrong lines in one case (`conformance/unknownType2`) — the largest
    ///   single family after the switch.
    /// - **An object literal against a union target.** That is the
    ///   excess-property and discriminated-union machinery
    ///   (`getMatchingUnionConstituentForObjectLiteral`,
    ///   `findMatchingDiscriminantType`), and upstream reports TS2353 / TS2561 /
    ///   TS2739 there. 38 wrong lines across six cases —
    ///   `excessPropertyCheckWithUnions`, `assignmentCompatWithDiscriminatedUnion`,
    ///   both `missingDiscriminants`, and two more.
    /// - **`any` and the error type**, unchanged: neither can fail a relation,
    ///   so admitting them can only produce accidents.
    ///
    /// The **enum** veto was added when `isSimpleTypeRelatedTo`'s enum arms
    /// (`relater.go:206`) were unported. They are ported now (`relater.rs`'s
    /// member-level arms), and the assignability reporters ask
    /// [`Checker::assignability_pair_is_reportable`], which lifts the enum veto.
    /// The veto stays for the other rules sharing this gate (operators,
    /// comparisons, heritage, assertions), whose own enum-literal handling — e.g.
    /// `getBaseTypeOfLiteralTypeForComparison` for relational operators — is not
    /// ported (`mixedTypeEnumComparison`).
    pub(crate) fn pair_is_reportable(&mut self, source: TypeId, target: TypeId) -> bool {
        self.assignability_pair_is_reportable(source, target)
            && ![source, target].iter().any(|&side| {
                self.type_of(side).flags.intersects(TypeFlags::ENUM | TypeFlags::ENUM_LITERAL)
            })
    }

    /// [`Checker::pair_is_reportable`] without the enum veto: the gate for
    /// `checkTypeRelatedTo`'s own reporters (TS2322/TS2345 and their
    /// elaborations), whose verdict comes from the relater's ported enum arms.
    pub(crate) fn assignability_pair_is_reportable(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let intrinsics = self.intrinsics();
        let (unknown, any) = (intrinsics.unknown, intrinsics.any);
        for side in [source, target] {
            // `Checker::is_error` and not `== intrinsics.error`: an unresolved
            // type REFERENCE mints a `TypeData::Named` carrying the written
            // text and answers `is_error` without being that intrinsic
            // (`checker-notes-diag2.md` §44). It is the same accident the arm
            // above describes, under a different type id — a type the port
            // could not build, which the relater cannot distinguish from one
            // that failed.
            if self.is_error(side) || side == unknown || side == any {
                return false;
            }
            if self.type_of(side).flags.intersects(TypeFlags::ANY | TypeFlags::UNKNOWN) {
                return false;
            }
        }
        // `getMatchingUnionConstituentForObjectLiteral` and the discriminant
        // machinery decide this pair upstream, and neither is ported.
        true
    }

    /// An **object** source against a **primitive** target — a definite
    /// negative the relater declines to give.
    ///
    /// `is_related_to` answers `Unknown` for a pair whose source is an object
    /// type with no members table (a function type, an index-signature-only
    /// type) because its structural arm was never reached — row 3 of
    /// `checker-notes-assign.md` §2, and correct as a statement about the
    /// *structural* comparison.
    ///
    /// But nothing structured is assignable to `string`, and no members table is
    /// needed to know it: `let x1: string = demoNS.f` is TS2322 whatever `f`'s
    /// shape turns out to be. This is asked here rather than added to
    /// `crate::relater` deliberately — the relater is read by overload selection
    /// and by narrowing, and widening what it calls a definite negative moves
    /// `checker_types`. As a rule-local decision it moves nothing else.
    ///
    /// **One direction only.** The converse is false: `let x: {} = 5` is legal,
    /// because an object *target* can be satisfied by a primitive through its
    /// apparent type.
    pub(crate) fn object_against_primitive(&mut self, source: TypeId, target: TypeId) -> bool {
        if self.type_of(source).flags.intersects(TypeFlags::OBJECT)
            && self.type_of(target).flags.intersects(PRIMITIVE_TARGET)
        {
            return true;
        }
        // The converse, and it needs a condition. `let x: {} = 5` is legal
        // because an empty object target is satisfied through the primitive's
        // apparent type; `let x: { a: number } = 5` is not, because a number has
        // no `a`. So a primitive source is a definite negative against an object
        // target **that requires at least one property**.
        self.type_of(source).flags.intersects(PRIMITIVE_TARGET)
            && self
                .declared_property_table(target)
                .is_some_and(|table| table.iter().any(|(_, optional)| !optional))
    }

    /// `elaborateError` (`relater.go:440`): descend into the source expression
    /// to report on the innermost node that explains the failure. Answers
    /// whether it reported; the caller then stays silent. A generic conditional
    /// target is not elaborated. `elaborateDidYouMeanToCallOrConstruct` is not
    /// ported here, so those failures keep the caller's outer report.
    fn elaborate_error(&mut self, node: NodeId, source: TypeId, target: TypeId) -> bool {
        if self.is_or_has_generic_conditional(target) {
            return false;
        }
        if self.elaborate_did_you_mean_to_call_or_construct(
            node,
            source,
            target,
            crate::signatures::SignatureKind::Construct,
        ) || self.elaborate_did_you_mean_to_call_or_construct(
            node,
            source,
            target,
            crate::signatures::SignatureKind::Call,
        ) {
            return true;
        }
        let inner = match self.node_map.get(node) {
            Some(Node::ParenthesizedExpression(parenthesized)) => parenthesized.expression,
            Some(Node::AsExpression(assertion))
                if assertion.r#type.is_some_and(crate::assertions::is_const_type_reference) =>
            {
                assertion.expression
            }
            Some(Node::BinaryExpression(binary))
                if binary.operator_token.is_some_and(|token| {
                    matches!(token.kind, SyntaxKind::EqualsToken | SyntaxKind::CommaToken)
                }) =>
            {
                binary.right
            }
            Some(Node::ObjectLiteralExpression(_)) => {
                return self.elaborate_object_literal(node, source, target);
            }
            // checkTypeRelatedToAndOptionallyElaborate elaborates only a failed
            // relation. A member mismatch need not fail the whole (a `void`
            // target return accepts any source return), so these arms ask.
            Some(Node::ArrayLiteralExpression(_)) => {
                return self.relate_ternary(source, target, crate::relater::Relation::Assignable)
                    == crate::relater::Ternary::NotRelated
                    && self.elaborate_array_literal(node, source, target);
            }
            Some(Node::ArrowFunction(_)) => {
                return self.relate_ternary(source, target, crate::relater::Relation::Assignable)
                    == crate::relater::Ternary::NotRelated
                    && self.elaborate_arrow_function(node, source, target);
            }
            _ => return false,
        };
        inner
            .and_then(|inner| inner.node_id())
            .is_some_and(|inner| self.elaborate_error(inner, source, target))
    }

    /// `elaborateDidYouMeanToCallOrConstruct` (`relater.go:480`): when some
    /// `kind` signature of the source returns a type (not `any`/`never`)
    /// related to the target, the failure is reported at the expression
    /// itself — upstream adds "Did you mean to call this expression?" as
    /// related information, which the suite does not compare. A pair the
    /// relation does not reject falls through to the remaining arms.
    fn elaborate_did_you_mean_to_call_or_construct(
        &mut self,
        node: NodeId,
        source: TypeId,
        target: TypeId,
        kind: crate::signatures::SignatureKind,
    ) -> bool {
        let Some(signatures) = self.signatures_of_type_kind(source, kind) else { return false };
        let mut callable = false;
        for signature in &signatures {
            let Some(return_type) = self.get_return_type_of_signature(signature) else {
                continue;
            };
            if self.type_of(return_type).flags.intersects(TypeFlags::ANY | TypeFlags::NEVER) {
                continue;
            }
            if self.relate_ternary(return_type, target, crate::relater::Relation::Assignable)
                == crate::relater::Ternary::Related
            {
                callable = true;
                break;
            }
        }
        callable && self.report_assignability_failure_with(node, None, source, target)
    }

    /// `getBestMatchingType` (`relater.go`) for an object-literal source, in
    /// the one domain where its answer is certain without the discriminant
    /// machinery: the union has exactly one constituent that is not primitive,
    /// it is a plain object type and not array-like, and it shares a property
    /// name with the source. There `findMatchingDiscriminantType` can only
    /// pick that constituent or nothing, `findMatchingTypeReferenceOrTypeAliasReference`
    /// and `findBestTypeForInvokable` do not apply to a signature-less literal,
    /// `findBestTypeForObjectLiteral` needs an array-like constituent, and
    /// `findMostOverlappyType` picks it on any key overlap. Every other union
    /// answers `None` (the caller keeps its decline).
    fn best_matching_object_constituent(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<TypeId> {
        let TypeData::Union { types, .. } = self.type_of(target).data.clone() else { return None };
        let mut objects = types.iter().copied().filter(|&part| {
            !self.type_of(part).flags.intersects(
                TypeFlags::PRIMITIVE | TypeFlags::TEMPLATE_LITERAL | TypeFlags::STRING_MAPPING,
            )
        });
        let best = objects.next()?;
        if objects.next().is_some() {
            return None;
        }
        let flags = self.type_of(best).flags;
        if !flags.contains(TypeFlags::OBJECT)
            || flags
                .intersects(TypeFlags::UNION | TypeFlags::INTERSECTION | TypeFlags::INSTANTIABLE)
            || self.tuple_element_lists.contains_key(&best)
            || self.variadic_tuple_elements.contains_key(&best)
            || self.tuple_spread_array_element(best).is_some()
        {
            return None;
        }
        let source_names = self.get_property_names_of_type(source)?;
        let target_names = self.get_property_names_of_type(best)?;
        source_names.iter().any(|name| target_names.contains(name)).then_some(best)
    }

    /// `isOrHasGenericConditional` (`relater.go:474`).
    fn is_or_has_generic_conditional(&self, t: TypeId) -> bool {
        let ty = self.type_of(t);
        ty.flags.contains(TypeFlags::CONDITIONAL)
            || matches!(&ty.data, TypeData::Intersection { types, .. }
                if types.iter().any(|&part| self.is_or_has_generic_conditional(part)))
    }

    /// `elaborateElement` (`relater.go:546`) once its member types are known:
    /// a related pair is not elaborated; otherwise `next` elaborates first and
    /// the failure is reported on `prop` (excess properties of a fresh `next`
    /// first, as `checkTypeRelatedToEx` would meet them).
    fn elaborate_element(
        &mut self,
        prop: NodeId,
        next: Option<NodeId>,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        // getBestMatchIndexedAccessTypeOrUndefined: no elaboration into an
        // index on a generic variable.
        if self.type_of(target).flags.contains(TypeFlags::INDEXED_ACCESS)
            || self.relate_ternary(source, target, crate::relater::Relation::Assignable)
                != crate::relater::Ternary::NotRelated
        {
            return false;
        }
        let source_node = next.unwrap_or(prop);
        let before = self.diagnostics.len();
        self.check_excess_properties(target, source_node);
        if self.diagnostics.len() != before {
            return true;
        }
        self.report_assignability_failure(prop, source_node, source, target)
    }

    /// `elaborateObjectLiteral` (`relater.go:498`): each named member is an
    /// element — a property assignment elaborates into its initializer, and
    /// shorthand, method and accessor members report at their name.
    ///
    /// Returns whether it reported, which is upstream's contract: a `true` here
    /// is what stops the whole-expression diagnostic being issued at all.
    fn elaborate_object_literal(
        &mut self,
        source_node: NodeId,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some(Node::ObjectLiteralExpression(literal)) = self.node_map.get(source_node) else {
            return false;
        };
        // `target.flags&(TypeFlagsPrimitive|TypeFlagsNever) != 0` — a primitive
        // or `never` target has no properties to elaborate against, and
        // upstream returns before the loop.
        if self.type_of(target).flags.intersects(TypeFlags::PRIMITIVE | TypeFlags::NEVER) {
            return false;
        }
        // `getBestMatchIndexedAccessTypeOrUndefined` picks a union constituent
        // (`getBestMatchingType`), which this port does not have; the caller's
        // own object-literal arm declines a union target for the same reason.
        if self.type_of(target).flags.contains(TypeFlags::UNION) {
            return false;
        }
        // A spread contributes properties this port cannot enumerate — the same
        // decline `check_excess_properties` makes, for the same reason.
        if literal.properties.iter().any(|property| {
            matches!(property, tsr_ast::ObjectLiteralElementLike::SpreadAssignment(_))
        }) {
            return false;
        }
        let mut reported = false;
        for property in literal.properties {
            let (name, next) = match property {
                tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                    let Some(next) = assignment.initializer.and_then(|e| e.node_id()) else {
                        continue;
                    };
                    (assignment.name.node_id(), Some(next))
                }
                tsr_ast::ObjectLiteralElementLike::ShorthandPropertyAssignment(shorthand) => {
                    (shorthand.name.node_id(), None)
                }
                tsr_ast::ObjectLiteralElementLike::MethodDeclaration(method) => {
                    (method.name.node_id(), None)
                }
                tsr_ast::ObjectLiteralElementLike::GetAccessorDeclaration(accessor) => {
                    (accessor.name.node_id(), None)
                }
                tsr_ast::ObjectLiteralElementLike::SetAccessorDeclaration(accessor) => {
                    (accessor.name.node_id(), None)
                }
                tsr_ast::ObjectLiteralElementLike::SpreadAssignment(_) => continue,
            };
            let Some(name_id) = name else { continue };
            // `getLiteralTypeFromProperty(…, StringOrNumberLiteralOrUnique)` —
            // a computed non-literal name yields no usable name type and
            // upstream `continue`s.
            let Some(name) = self.identifier_text(name_id).map(str::to_string) else { continue };
            // The indexed-access result uses the concrete target receiver.
            // Reading the declaration symbol alone loses its mapper, so a
            // member declared as T on C<number> would be compared against T.
            // `getIndexedAccessTypeOrUndefined` falls back to the target's
            // applicable index signature; absent from both means excess,
            // TS2353's row.
            let Some(target_property_type) = self
                .get_type_of_property_of_type(target, &name)
                .or_else(|| self.elaboration_index_value(target, name_id, &name))
            else {
                continue;
            };
            // `getIndexedAccessTypeOrUndefined(source, nameType, …)` reads the
            // completed source member, including mutable-location widening.
            let Some(source_property_type) = self.get_type_of_property_of_type(source, &name)
            else {
                continue;
            };
            reported |=
                self.elaborate_element(name_id, next, source_property_type, target_property_type);
        }
        reported
    }

    /// `getIndexedAccessTypeOrUndefined(target, nameType)`'s index-signature
    /// arm (`getPropertyTypeForIndexType`, `checker.go`) for a member name the
    /// target has no property for: the value of `getApplicableIndexInfo` for
    /// the name's literal type (`getLiteralTypeFromPropertyName` — a numeric
    /// name is a number literal, any other a string literal).
    fn elaboration_index_value(
        &mut self,
        target: TypeId,
        name_id: NodeId,
        name: &str,
    ) -> Option<TypeId> {
        let name_type = if self.nodes.kind(name_id) == SyntaxKind::NumericLiteral {
            let literal = self.check_expression_at_node(name_id);
            self.get_regular_type_of_literal_type(literal)
        } else {
            self.store.intern_literal(
                TypeFlags::STRING_LITERAL,
                TypeData::StringLiteral(name.to_owned()),
                false,
            )
        };
        self.get_applicable_index_info(target, name_type).map(|info| info.value)
    }

    /// `elaborateArrayLiteral` (`relater.go:522`): each element is an element
    /// of the tuple-like source, keyed by its index, reported at
    /// `getEffectiveCheckNode` of the element. A non-tuple source is re-read
    /// upstream as a contextually typed tuple; each element's checked type is
    /// that tuple's member here. Spreads (whose index does not name one
    /// element) and union targets (`getBestMatchingType`) are declined.
    fn elaborate_array_literal(&mut self, node: NodeId, source: TypeId, target: TypeId) -> bool {
        let Some(Node::ArrayLiteralExpression(literal)) = self.node_map.get(node) else {
            return false;
        };
        let target_flags = self.type_of(target).flags;
        if target_flags.intersects(TypeFlags::PRIMITIVE | TypeFlags::NEVER | TypeFlags::UNION)
            || literal.elements.iter().any(|element| {
                element.node_id().is_some_and(|id| self.nodes.kind(id) == SyntaxKind::SpreadElement)
            })
        {
            return false;
        }
        let tuple_target = self.tuple_element_lists.contains_key(&target);
        if !tuple_target && self.variadic_tuple_elements.contains_key(&target) {
            return false;
        }
        let array_element =
            if tuple_target { None } else { self.tuple_spread_array_element(target) };
        let source_tuple = self.tuple_element_lists.contains_key(&source);
        let elements: Vec<NodeId> =
            literal.elements.iter().filter_map(tsr_ast::Expression::node_id).collect();
        let mut reported = false;
        for (index, element) in elements.into_iter().enumerate() {
            if self.nodes.kind(element) == SyntaxKind::OmittedExpression {
                continue;
            }
            let target_element = if tuple_target {
                // isTupleLikeType(target) && no property `index`: skipped.
                if self.tuple_element_lists.get(&target).is_none_or(|(list, _)| index >= list.len())
                {
                    continue;
                }
                let Some(member) = self.get_type_of_property_of_type(target, &index.to_string())
                else {
                    continue;
                };
                member
            } else if let Some(element_type) = array_element {
                element_type
            } else {
                // getIndexedAccessTypeOrUndefined(target, i) on a non-array
                // object target: a property named `i`, else the applicable
                // (numeric or string) index signature; neither skips it.
                let name = index.to_string();
                let index_type = self.store.intern_literal(
                    TypeFlags::NUMBER_LITERAL,
                    TypeData::NumberLiteral(name.clone()),
                    false,
                );
                let Some(member) = self.get_type_of_property_of_type(target, &name).or_else(|| {
                    self.get_applicable_index_info(target, index_type).map(|info| info.value)
                }) else {
                    continue;
                };
                member
            };
            let check_node = self.effective_check_node(element);
            let source_element = if source_tuple {
                let Some(member) = self.get_type_of_property_of_type(source, &index.to_string())
                else {
                    continue;
                };
                member
            } else {
                self.check_expression_at_node(check_node)
            };
            reported |= self.elaborate_element(
                check_node,
                Some(check_node),
                source_element,
                target_element,
            );
        }
        reported
    }

    /// `elaborateArrowFunction` (`relater.go:641`): an expression-bodied arrow
    /// with no annotated parameter relates its single call signature's return
    /// type to the union of the target's call signature returns, elaborating
    /// into the body and otherwise reporting at it.
    fn elaborate_arrow_function(&mut self, node: NodeId, source: TypeId, target: TypeId) -> bool {
        let Some(Node::ArrowFunction(arrow)) = self.node_map.get(node) else { return false };
        let Some(body) = arrow.body.and_then(|body| body.node_id()) else { return false };
        if self.nodes.kind(body) == SyntaxKind::Block
            || arrow.parameters.iter().any(|parameter| parameter.r#type.is_some())
        {
            return false;
        }
        let Some(signature) = self.single_call_signature(source) else { return false };
        let Some(targets) =
            self.signatures_of_type_kind(target, crate::signatures::SignatureKind::Call)
        else {
            return false;
        };
        if targets.is_empty() {
            return false;
        }
        let Some(source_return) = self.get_return_type_of_signature(&signature) else {
            return false;
        };
        let mut returns = Vec::with_capacity(targets.len());
        for target_signature in &targets {
            let Some(target_return) = self.get_return_type_of_signature(target_signature) else {
                return false;
            };
            returns.push(target_return);
        }
        let target_return = self.get_union_type(&returns);
        self.elaborate_element(body, Some(body), source_return, target_return)
    }
}

/// Does this modifier list carry `async`?
fn has_async(modifiers: &[tsr_ast::ModifierLike<'_>]) -> bool {
    modifiers.iter().any(|modifier| {
        matches!(modifier, tsr_ast::ModifierLike::Token(token) if token.kind == SyntaxKind::AsyncKeyword)
    })
}

/// The target flags that no object type can ever satisfy.
///
/// `void`, `null` and `undefined` are absent: their relation to an object source
/// depends on `strictNullChecks` and on `void`'s own arm, and this list must
/// hold only the pairs that are unrelated under every configuration.
const PRIMITIVE_TARGET: TypeFlags = TypeFlags::STRING
    .union(TypeFlags::NUMBER)
    .union(TypeFlags::BOOLEAN)
    .union(TypeFlags::BIG_INT)
    .union(TypeFlags::ES_SYMBOL)
    .union(TypeFlags::STRING_LITERAL)
    .union(TypeFlags::NUMBER_LITERAL)
    .union(TypeFlags::BOOLEAN_LITERAL)
    .union(TypeFlags::BIG_INT_LITERAL);

/// Is the TS2741 arm live?
///
/// **`true` since §41.2.** §22 refused it at 1 conversion for 8 wrong lines,
/// measured against a `declared_property_table` that declined instantiated
/// references, generic declarations and every `Anonymous` receiver. §35, §38.2
/// and §41.1 removed all three; re-running the switch is now positive.
///
/// A constant rather than a deletion, and the distinction is the point: the
/// machinery it switches — [`Checker::missing_required_property`] and
/// [`crate::member_completeness`]'s property enumeration — is *correct* and is
/// what four named residual families stand between and a positive score. §22
/// lists them. Deleting the code would make the refusal unrevisitable, which is
/// the one thing `docs/conventions.md` forbids about a refusal.
const REPORT_MISSING_REQUIRED_PROPERTY: bool = true;
