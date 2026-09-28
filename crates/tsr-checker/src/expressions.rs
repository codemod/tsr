//! `checkExpression` and the expression forms that do not have a module of
//! their own.
//!
//! Ported from `internal/checker/checker.go`. The binary operators live in
//! [`crate::binary`] and property access in [`crate::members`], because those
//! two are large enough to be worked on independently; everything else that
//! answers "what is the type of this expression" is here.

use tsr_ast::{Expression, Node, NodeId, SyntaxKind};
use tsr_binder::SymbolFlags;

use crate::{
    checker::Checker,
    flags::TypeFlags,
    printing,
    types::{TypeData, TypeId},
};

/// `AssignmentKind` (`internal/checker/utilities.go`): how a reference is
/// written, which decides whether `checkIdentifier` narrows it at all.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssignmentTargetKind {
    None,
    Definite,
    Compound,
}

impl Checker<'_, '_> {
    /// The type of an expression.
    ///
    /// Ported from `Checker.checkExpression` (`internal/checker/checker.go`),
    /// restricted to the forms this slice covers. Anything else yields
    /// `errorType` — **not** `anyType`: `errorType` is upstream's marker for "could
    /// not be computed", and using `any` would claim a real answer.
    pub fn check_expression(&mut self, expression: Expression<'_>) -> TypeId {
        let node = Node::from(expression);
        if let Some(id) = node.node_id()
            && let Some(&cached) = self.node_types.get(&id)
        {
            return cached;
        }

        self.computations += 1;
        let computed = self.check_expression_worker(expression);

        // Never persist a type computed during loop fixpoint analysis: the
        // walk hands out transient so-far unions, and an entry stamped from
        // one would outlive convergence (`foo(x)` resolved against a
        // provisional `string` stays `number` forever). Upstream's
        // formulation clears `flowLoopStack` before any computation it will
        // cache — `Checker.checkExpressionCachedEx`
        // (`internal/checker/checker.go:7517`); suppressing the write while
        // the stack is non-empty is the dual for this port's single cache.
        // See docs/architecture/checker-notes-narrow.md §12.6.
        if self.flow_loop_stack.is_empty()
            && let Some(id) = node.node_id()
        {
            self.node_types.insert(id, computed);
        }
        computed
    }

    /// `checkTemplateExpression` (`checker.go:7976`) — see the dispatch
    /// arm's comment and `checker-notes-narrow.md` §24 for the decline set.
    fn check_template_expression(&mut self, node: &tsr_ast::TemplateExpression<'_>) -> TypeId {
        let error = self.intrinsics.error;
        let mut span_types = Vec::with_capacity(node.template_spans.len());
        for span in node.template_spans {
            let Some(expression) = span.expression else { return error };
            span_types.push(self.check_expression(expression));
        }
        // **A span this port could not type does not make the template a gap.**
        // `checkTemplateExpression` (`checker.go:7976`) checks each span for
        // one thing only — an ESSymbol-like type, which it *reports* on — and
        // then returns `stringType` unless the node is in a const or
        // template-literal context. The span types are not an input to the
        // answer outside those contexts, so propagating a span's `error` here
        // was the §192 mistake in a second place: a refusal justified by
        // "the answer depends on something unknown" where upstream's answer
        // depends on nothing of the kind. `circularBaseConstraint`,
        // `constEnumErrors` and `destructuringParameterProperties4` all record
        // `` `${x}` : string `` over a substitution upstream itself cannot
        // resolve. §198.
        //
        // What the flag still suppresses is the **fold**. Folding to a string
        // literal reads each span's literal value, and `evaluate_constant_expression`
        // works off the SYNTAX, so an un-typed span could otherwise fold to a
        // literal this port has no business asserting. Skipping the fold sends
        // those to the context check and then to `string`, which is upstream's
        // answer, and leaves §101/§147's fold semantics untouched for every
        // template whose spans did type.
        let untyped_span = span_types.contains(&error);
        // §147 (`checker-notes-narrow.md`) replaced §24's length-based escape
        // decline: part values are now cooked exactly as upstream cooks them
        // (octal chars when reported, raw text otherwise), so folding an
        // escaped part is CORRECT — and `printing::quote` re-escapes control
        // characters on the way out. The one non-fold: a TAGGED template's
        // substitution form is never folded (the second §147 pair —
        // `taggedTemplateStringsHexadecimalEscapes` wants `string` for VALID
        // escapes too, so this is position, not escape validity; an invalid
        // escape's cooked value being undefined upstream is the same answer
        // by a different road).
        let tagged = node.node_id.is_some_and(|id| {
            self.nodes
                .parent(id)
                .is_some_and(|p| self.nodes.kind(p) == SyntaxKind::TaggedTemplateExpression)
        });
        if tagged {
            return self.intrinsics.string;
        }
        let mut folded: Option<String> =
            if untyped_span { None } else { node.head.map(|head| head.text.to_string()) };
        for (span, &span_type) in node.template_spans.iter().zip(&span_types) {
            let Some(previous) = folded else { break };
            let piece = match &self.store.get(span_type).data {
                crate::types::TypeData::StringLiteral(text)
                | crate::types::TypeData::NumberLiteral(text) => Some(text.clone()),
                // §101 (`checker-notes-narrow.md`): a span whose TYPE is not
                // a literal may still have a constant VALUE — upstream hands
                // the whole template to the evaluator (`checker.go:7991`).
                _ => span
                    .expression
                    .as_ref()
                    .and_then(evaluate_constant_expression)
                    .map(EvaluatedValue::render),
            };
            folded = match (piece, span.literal) {
                (Some(piece), Some(literal)) => {
                    let tail = match literal {
                        tsr_ast::TemplateMiddleOrTail::TemplateMiddle(part) => part.text,
                        tsr_ast::TemplateMiddleOrTail::TemplateTail(part) => part.text,
                    };
                    Some(previous + &piece + tail)
                }
                _ => None,
            };
        }
        if let Some(value) = folded {
            return self.store.intern_literal(
                TypeFlags::STRING_LITERAL,
                TypeData::StringLiteral(value),
                true,
            );
        }
        // The three §24 declines: a const context, the element-access
        // argument position (a template-literal context), and any span whose
        // literal kind the fold cannot evaluate is NOT declined — only the
        // CONTEXT questions are, because they change the ANSWER's shape.
        if let Some(id) = node.node_id {
            let mut current = self.nodes.parent(id);
            while let Some(parent) = current {
                match self.nodes.kind(parent) {
                    SyntaxKind::AsExpression
                    | SyntaxKind::TypeAssertionExpression
                    | SyntaxKind::ElementAccessExpression => {
                        return error;
                    }
                    SyntaxKind::ParenthesizedExpression => {
                        current = self.nodes.parent(parent);
                    }
                    _ => break,
                }
            }
        }
        self.intrinsics.string
    }

    /// Whether the node's source file carries COMMONJS module machinery — a
    /// reference to `require`, `module` or `exports`. §784.
    ///
    /// The sibling [`Checker::file_has_import_machinery`] asks the same
    /// question — *can an unresolved name here be this port's own binding gap
    /// rather than the source's?* — but only over ES `import`/`export`
    /// DECLARATIONS, so it answers `false` for every `CommonJS` file, which is
    /// the shape `allowJs` corpora are written in.
    pub(crate) fn file_has_commonjs_machinery(&mut self, node: NodeId) -> bool {
        let mut root = node;
        while let Some(parent) = self.nodes.parent(root) {
            root = parent;
        }
        if let Some(&cached) = self.file_commonjs_machinery.get(&root) {
            return cached;
        }
        let answer = self.node_map.get(root).is_none_or(|file| {
            let mut stack = vec![file];
            let mut children = Vec::new();
            let mut found = false;
            while let Some(current) = stack.pop() {
                if let Node::Identifier(identifier) = current
                    && matches!(identifier.text, "require" | "module" | "exports")
                {
                    found = true;
                    break;
                }
                children.clear();
                tsr_ast::push_children(current, &mut children);
                stack.extend(children.iter().copied());
            }
            found
        });
        self.file_commonjs_machinery.insert(root, answer);
        answer
    }

    /// Whether the node's source file carries any import/export declaration
    /// — the §31 gate's structural half (`checker-notes-narrow.md`).
    pub(crate) fn file_has_import_machinery(&mut self, node: NodeId) -> bool {
        let mut root = node;
        while let Some(parent) = self.nodes.parent(root) {
            root = parent;
        }
        if let Some(&cached) = self.file_import_machinery.get(&root) {
            return cached;
        }
        let answer = self.node_map.get(root).is_none_or(|file| {
            let mut stack = vec![file];
            let mut children = Vec::new();
            let mut found = false;
            while let Some(current) = stack.pop() {
                if matches!(
                    current,
                    // §173 (`checker-notes-narrow.md`): only declarations
                    // that BRING A NAME INTO SCOPE can explain an
                    // unresolved identifier as this port's own unported
                    // binding. An `export =` or `export { }` introduces
                    // nothing, so its presence cannot excuse the gap — the
                    // name is upstream's TS2304 and reads `any`
                    // (ExportAssignment7's oracle records `>B : any`).
                    Node::ImportDeclaration(_)
                        | Node::ImportEqualsDeclaration(_)
                        | Node::ExportDeclaration(_)
                ) {
                    found = true;
                    break;
                }
                children.clear();
                tsr_ast::push_children(current, &mut children);
                stack.extend(children.iter().copied());
            }
            found
        });
        self.file_import_machinery.insert(root, answer);
        answer
    }

    /// `GetRootDeclaration` (`ast/utilities.go:1173`). §765.
    ///
    /// Walk out of every enclosing binding pattern to the declaration that
    /// owns the whole destructuring — a parameter or a variable declaration.
    pub(crate) fn root_declaration_of(&self, node: NodeId) -> NodeId {
        let mut at = node;
        while self.nodes.kind(at) == SyntaxKind::BindingElement {
            let Some(pattern) = self.nodes.parent(at) else { break };
            let Some(owner) = self.nodes.parent(pattern) else { break };
            at = owner;
        }
        at
    }

    /// The §50 shape test + pseudo-narrow + re-projection
    /// (`getNarrowedTypeOfSymbol`'s binding-element case,
    /// `checker.go:13751`). `None` hands back to the ordinary flow road.
    fn dependent_destructured_type(
        &mut self,
        symbol: tsr_binder::SymbolId,
        reference: NodeId,
    ) -> Option<TypeId> {
        let declaration = self.binder.symbols().get(symbol).value_declaration?;
        // §50.3: the tuple-parameter shape (`checker.go:13806`), reduced to
        // the written-annotation slice (`checker-notes-narrow.md`).
        if self.nodes.kind(declaration) == SyntaxKind::Parameter {
            return self.dependent_tuple_parameter_type(declaration, reference);
        }
        let Node::BindingElement(element) = self.node_map.get(declaration)? else {
            return None;
        };
        if element.dot_dot_dot_token.is_some() || element.initializer.is_some() {
            return None;
        }
        let pattern_id = self.nodes.parent(declaration)?;
        let Node::BindingPattern(pattern) = self.node_map.get(pattern_id)? else {
            return None;
        };
        if pattern.elements.len() < 2 {
            return None;
        }
        // `parent := declaration.Parent.Parent` (`checker.go:13760`) — the
        // holder of the IMMEDIATE pattern, which is what the parent type is
        // read from. For a nested pattern that is itself a binding element.
        let holder = self.nodes.parent(pattern_id)?;
        // §765 (`checker.go:13752`/`:13761`): the const-like test is on the
        // ROOT declaration, not on the immediate holder. `GetRootDeclaration`
        // walks out of every enclosing binding pattern, so
        // `const { a: { b, c } } = x` reaches the variable declaration where a
        // one-hop test found a `BindingElement` and declined.
        let root = self.root_declaration_of(declaration);
        let root_ok = match self.nodes.kind(root) {
            SyntaxKind::Parameter => true,
            SyntaxKind::VariableDeclaration => {
                self.combined_node_flags(root).intersects(tsr_ast::NodeFlags::CONSTANT)
            }
            _ => return None,
        };
        if !root_ok {
            return None;
        }
        // §764 (`checker.go:13772`): `!(IsParameterDeclaration(root) &&
        // isSomeSymbolAssigned(root))`. A PARAMETER the body reassigns is no
        // longer one destructured value — its siblings stop being projections
        // of a single parent, so discriminating them against each other is
        // unsound. The guard is on the ROOT, and it asks about every symbol
        // the root's name binds, not just the one being read.
        if self.nodes.kind(root) == SyntaxKind::Parameter && self.is_some_symbol_assigned(root) {
            return None;
        }
        let parent_type = self.get_type_for_binding_element_parent(holder);
        if parent_type == self.intrinsics.error {
            return None;
        }
        // §766 (`checker.go:13768`): `mapType(parentType, getBaseConstraintOrType)`.
        // The union test is on the CONSTRAINT, so a destructured parameter
        // typed `T extends A | B` reaches the road — the constraint is the
        // union, the written type is a type parameter, and testing the written
        // type declined every generic destructuring.
        let parent_constraint = {
            let constituents: Vec<TypeId> = match &self.store.get(parent_type).data {
                crate::types::TypeData::Union { types, .. } => types.clone(),
                _ => vec![parent_type],
            };
            let mapped: Vec<TypeId> =
                constituents.iter().map(|&c| self.base_constraint_or_type(c)).collect();
            if mapped == constituents { parent_type } else { self.get_union_type(&mapped) }
        };
        if !self.store.get(parent_constraint).flags.intersects(crate::flags::TypeFlags::UNION) {
            return None;
        }
        let parent_type = parent_constraint;
        let narrowed = self.narrow_destructured_parent(reference, pattern_id, parent_type);
        // §764 (`checker.go:13775`): a parent narrowed to `never` makes the
        // ELEMENT `never` — upstream answers that directly rather than
        // projecting out of an empty union.
        if self.store.get(narrowed).flags.contains(crate::flags::TypeFlags::NEVER) {
            return Some(self.intrinsics.never);
        }
        if narrowed == parent_type {
            // Nothing narrowed: the ordinary projection road answers, and
            // taking it keeps this arm invisible when no discriminant fired.
            return None;
        }
        let projected = self.project_binding_element(declaration, narrowed);
        (projected != self.intrinsics.error).then_some(projected)
    }

    /// §50.3's arm: an unannotated parameter of a function whose VARIABLE
    /// annotation is a single-rest function type over a union of tuples;
    /// the function node is the pseudo-reference and the answer indexes the
    /// narrowed union at the parameter's position (`checker.go:13806`,
    /// `checker-notes-narrow.md` §50.3).
    fn dependent_tuple_parameter_type(
        &mut self,
        declaration: NodeId,
        reference: NodeId,
    ) -> Option<TypeId> {
        let Node::ParameterDeclaration(parameter) = self.node_map.get(declaration)? else {
            return None;
        };
        if parameter.r#type.is_some()
            || parameter.initializer.is_some()
            || parameter.dot_dot_dot_token.is_some()
        {
            return None;
        }
        let fn_id = self.nodes.parent(declaration)?;
        let parameters = match self.node_map.get(fn_id)? {
            Node::ArrowFunction(function) => function.parameters,
            Node::FunctionExpression(function) => function.parameters,
            _ => return None,
        };
        if parameters.len() < 2 {
            return None;
        }
        let index = parameters.iter().position(|p| p.node_id == Some(declaration))?;
        // The written-annotation slice of `getContextualSignature`: the
        // function is the DIRECT initializer of a variable whose annotation
        // is syntactically a function type with one `...rest` parameter.
        let holder = self.nodes.parent(fn_id)?;
        let Node::VariableDeclaration(variable) = self.node_map.get(holder)? else {
            return None;
        };
        if variable.initializer.and_then(|e| e.node_id()) != Some(fn_id) {
            return None;
        }
        let tsr_ast::TypeNode::FunctionTypeNode(annotation) = variable.r#type? else {
            return None;
        };
        let [rest] = annotation.parameters else { return None };
        rest.dot_dot_dot_token?;
        let rest_type = self.get_type_from_type_node(rest.r#type?);
        if !self.store.get(rest_type).flags.intersects(TypeFlags::UNION) {
            return None;
        }
        let TypeData::Union { types, .. } = &self.store.get(rest_type).data else {
            return None;
        };
        let constituents = types.clone();
        if !constituents.iter().all(|t| self.tuple_element_lists.contains_key(t)) {
            return None;
        }
        let narrowed = self.narrow_destructured_parent(reference, fn_id, rest_type);
        if narrowed == rest_type {
            return None;
        }
        let projected = self.get_type_of_property_of_type(narrowed, &index.to_string())?;
        (projected != self.intrinsics.error).then_some(projected)
    }

    fn check_expression_worker(&mut self, expression: Expression<'_>) -> TypeId {
        match expression {
            // Literal *expressions* produce **fresh** literal types, which is
            // what lets `let x = "a"` widen to `string` while `let x: "a"`
            // does not — a literal type node produces the regular form. See
            // `Checker::get_widened_literal_type`.
            Expression::StringLiteral(node) => self.store.intern_literal(
                TypeFlags::STRING_LITERAL,
                TypeData::StringLiteral(node.text.to_string()),
                true,
            ),
            // §147's fourth pair: the tagged-position `string` rule holds for
            // the SUBSTITUTION form only — a tagged NO-SUB template's node
            // reads the literal of its (raw, no-report) value, invalid
            // escapes included (`templateLiteralEscapeSequence` 0:100-109
            // want `"\u{}"`, `"\x"` as literals in tagged position).
            Expression::NoSubstitutionTemplateLiteral(node) => self.store.intern_literal(
                TypeFlags::STRING_LITERAL,
                TypeData::StringLiteral(node.text.to_string()),
                true,
            ),
            // §167 (`checker-notes-narrow.md`): `checkRegularExpressionLiteral`
            // (`checker.go:8014-8018`) is one line past its grammar check —
            // *return c.globalRegExpType*. This dispatch had no arm at all,
            // so every regex literal answered errorType and took its uses
            // with it (215 corpus lines want `RegExp`).
            Expression::RegularExpressionLiteral(_) => self
                .global_type_symbol_with_arity("RegExp", 0)
                .map_or(self.intrinsics.error, |symbol| self.get_declared_type_of_symbol(symbol)),
            Expression::NumericLiteral(node) => self.store.intern_literal(
                TypeFlags::NUMBER_LITERAL,
                TypeData::NumberLiteral(printing::normalise_number(node.text)),
                true,
            ),
            Expression::BigIntLiteral(node) => self.store.intern_literal(
                TypeFlags::BIG_INT_LITERAL,
                // A bigint literal's text carries its trailing `n`; the type's
                // payload is the digits, and `type_to_string` puts the `n` back.
                TypeData::BigIntLiteral(node.text.trim_end_matches('n').to_string()),
                true,
            ),
            Expression::KeywordExpression(node) => match node.kind {
                SyntaxKind::ThisKeyword => {
                    node.node_id.map_or(self.intrinsics.error, |id| self.check_this_expression(id))
                }
                // `checkSuperExpression` (`checker.go:7854`).
                SyntaxKind::SuperKeyword => {
                    node.node_id.map_or(self.intrinsics.error, |id| self.check_super_expression(id))
                }
                SyntaxKind::TrueKeyword => self.intrinsics.true_type,
                SyntaxKind::FalseKeyword => self.intrinsics.false_type,
                SyntaxKind::NullKeyword => self.intrinsics.null,
                // `undefined` is an identifier rather than a keyword in the
                // grammar, so it does not arrive here.
                _ => self.intrinsics.error,
            },
            // Ported from `Checker.checkIdentifier`: resolve the name, take the
            // symbol's type, and narrow it by the control flow reaching here.
            //
            // **Narrowing is partial**, and [`crate::flow`] says exactly which
            // guards are ported. The property that makes that safe is upstream's
            // own: `narrowType`'s default arm returns the type unchanged, so an
            // unported guard leaves the declared type rather than producing a
            // wrong one.
            //
            // Note that `let x = "a"` is `string` here *and* upstream —
            // `getTypeAtFlowAssignment` reduces only when the declared type is a
            // union, so that is not a narrowing gap however much it looks like
            // one (`bd tsr-4sc.11`).
            Expression::Identifier(node) => {
                let Some(id) = node.node_id else { return self.intrinsics.error };
                // `SymbolFlags::VALUE` is upstream's meaning for an identifier
                // expression (`checkIdentifier` -> `getResolvedSymbol`). It is
                // what keeps an enclosing class's type parameter from being
                // resolved here — see `BindResult::resolve_name`.
                let resolved = self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    id,
                    node.text,
                    SymbolFlags::VALUE,
                );
                if let Some(symbol) = resolved {
                    {
                        // §213: upstream refuses this resolution outright when
                        // the reference is in a non-static property initializer
                        // and the constructor declares the same name — see the
                        // predicate for why it needs the resolved symbol.
                        if self.identifier_is_an_invalid_class_field_initializer_reference(
                            id, node.text, symbol,
                        ) {
                            return self.intrinsics.error;
                        }
                        let declared = self.get_type_of_symbol(symbol);
                        // `getNarrowedTypeOfSymbol` (`checker.go`): only a
                        // variable or parameter reference is narrowed. A class,
                        // interface, enum or function reference is not, and
                        // narrowing one anyway would answer a question upstream
                        // does not ask.
                        if self.is_narrowable_symbol(symbol) {
                            let node_id = node.node_id.expect("checked above");
                            let target_kind = self.assignment_target_kind(node_id);
                            // `checker.go:11096`: assigning to a readonly
                            // symbol reports and answers `errorType` — whose
                            // observable is `any` (the §14/§27 boundary
                            // argument, `checker-notes-narrow.md`).
                            if target_kind != AssignmentTargetKind::None
                                && self.is_readonly_symbol(symbol)
                            {
                                return self.intrinsics.any;
                            }
                            match target_kind {
                                // `checker.go:11109`: a variable in a definite
                                // assignment-target position is returned at its
                                // DECLARED type — no flow analysis. This is what
                                // makes `x` in `x = foo(x)` print the full
                                // declared union, and an auto-typed target print
                                // `any` (`controlFlowSelfReferentialLoop.types:160`;
                                // `checker-notes-narrow.md` §12.7). A
                                // compound-like assignment (`x = x + 1`) reads at
                                // the literal's base, exactly as `x += 1` would.
                                AssignmentTargetKind::Definite => {
                                    if self.is_in_compound_like_assignment(node_id) {
                                        self.get_base_type_of_literal_type(declared)
                                    } else {
                                        declared
                                    }
                                }
                                // `checker.go:11196`: the TARGET of a compound
                                // assignment reads at the literal's base — `x |= …`
                                // sees `boolean`, not the narrowed `true`
                                // (`bitwiseCompoundAssignmentOperators.types`;
                                // `checker-notes-narrow.md` §10).
                                AssignmentTargetKind::Compound => {
                                    let flowed = self.get_flow_type_of_reference(
                                        node_id,
                                        Some(symbol),
                                        declared,
                                    );
                                    self.get_base_type_of_literal_type(flowed)
                                }
                                AssignmentTargetKind::None => {
                                    // §230: the expression of an
                                    // `export = x` is not flow-narrowed. It is
                                    // the entity name of an ALIAS declaration
                                    // (`getTargetOfExportAssignment`,
                                    // `checker.go:14889`), so it answers the
                                    // symbol's declared type and never enters
                                    // the flow walk — which is why an
                                    // auto-typed `var x;` reads `any` there and
                                    // `undefined` in an ordinary reference.
                                    //
                                    // **Both halves measured against upstream
                                    // ITSELF**, not inferred: a probe fixture
                                    // run through upstream's own
                                    // `TestLocal` baseline runner records
                                    // `x;` → `undefined` and, three lines
                                    // later in the same file on the same
                                    // symbol, `export = x;` → `any`. A symbol
                                    // answering two ways at two sites is what
                                    // rules out every symbol-level disjunct of
                                    // `assumeInitialized` (`:11150-11158`) —
                                    // those cannot vary by reference site.
                                    if self.nodes.parent(node_id).is_some_and(|parent| {
                                        self.nodes.kind(parent) == SyntaxKind::ExportAssignment
                                    }) {
                                        return declared;
                                    }
                                    // §839: `checkIdentifier`'s
                                    // uninitialized-variable arm
                                    // (`checker.go:11189-11192`) returns the
                                    // **declared** type and discards the
                                    // narrowing — "Return the declared type to
                                    // reduce follow-on errors". An uninitialized
                                    // annotated `var` starts the walk at
                                    // `declared | undefined`, and any branch
                                    // where `undefined` survives the narrowing
                                    // answers the declared type. That is why the
                                    // `else` branch of
                                    // `typeof strOrNum === "string"` is
                                    // `string | number` upstream and the `then`
                                    // branch is `string`.
                                    //
                                    // `declared` here is the printed type from
                                    // `get_type_of_symbol`, not the predicate's
                                    // unprinted annotation read: the two are the
                                    // same type, and this road is the one that
                                    // must print.
                                    if self
                                        .uninitialized_variable_reads_declared(node_id, node.text)
                                    {
                                        return declared;
                                    }
                                    // §50 (`checker-notes-narrow.md`): a
                                    // dependent destructured local narrows
                                    // its PARENT at the use site and
                                    // re-projects.
                                    // §50.2: the projection is the DECLARED
                                    // type of the ordinary walk — the two
                                    // walks compose (`checker.go:13751`).
                                    let start = self
                                        .dependent_destructured_type(symbol, node_id)
                                        .unwrap_or(declared);
                                    self.get_flow_type_of_reference(node_id, Some(symbol), start)
                                }
                            }
                        } else if self.assignment_target_kind(id) != AssignmentTargetKind::None
                            && self.binder.symbols().get(symbol).flags.intersects(
                                SymbolFlags::FUNCTION
                                    | SymbolFlags::CLASS
                                    | SymbolFlags::ENUM
                                    | SymbolFlags::VALUE_MODULE,
                            )
                        {
                            // §313: ASSIGNING to a function, class, enum or
                            // namespace name is upstream's TS2629/2630/2631/2632
                            // family — `checkReferenceExpression` reports and
                            // the target reads `errorType`, whose observable is
                            // `any` (every such case carries an errors
                            // baseline): `eval = 1` records `>eval : any`
                            // (`parserStrictMode3` — and its `-negative` twin
                            // proves strict mode is not the trigger),
                            // `fn = () => {}` records `>fn : any`
                            // (`assignmentToFunction`, `assignToEnum`,
                            // `assignToExistingClass`).
                            self.intrinsics.error
                        } else if let Some(&spelled) = self.enum_access_spelling.get(&declared) {
                            // §280: a bare enum-member REFERENCE takes the
                            // access spelling, exactly as the property-access
                            // road does — `a` inside `b = a` prints `E` when
                            // the enum's values collapse to one
                            // (`mergedEnumDeclarationCodeGen`,
                            // `preserveConstEnums`). An enum member is not a
                            // narrowable symbol, so this is that branch alone.
                            spelled
                        } else {
                            declared
                        }
                    }
                    // `checkIdentifier`'s unresolved exit: upstream reports
                    // TS2304 and answers `errorType` — printed `any`, the
                    // §14/§27/§31 boundary argument. Gated on the name being
                    // absent in EVERY meaning: a name this port can find as
                    // an alias/type/namespace but not resolve as a value is
                    // the PORT's resolution gap, and answering `any` there
                    // manufactured 1,909 adverse lines in the ungated
                    // measurement (`checker-notes-narrow.md` §31).
                } else {
                    {
                        let anywhere = self.binder.resolve_name(
                            self.nodes,
                            self.node_map,
                            id,
                            node.text,
                            SymbolFlags::VALUE
                                | SymbolFlags::TYPE
                                | SymbolFlags::NAMESPACE
                                | SymbolFlags::ALIAS,
                        );
                        // The structural gate: a file with import/export
                        // machinery can miss through the PORT's alias
                        // resolution; a `///<reference>`-style script cannot
                        // — its unresolved names are the SOURCE's.
                        // `arguments` is THIS PORT's miss (upstream binds
                        // `IArguments` in every function), and a JS/JSX file
                        // resolves through machinery with known port gaps —
                        // both stay honest gaps.
                        // §33: `globalThis` mints its own type; members
                        // resolve through the merged globals table.
                        if node.text == "globalThis" {
                            if let Some(existing) = self.global_this_type {
                                return existing;
                            }
                            let minted = self.store.new_named(
                                TypeFlags::OBJECT,
                                "typeof globalThis".to_string(),
                                None,
                            );
                            self.global_this_type = Some(minted);
                            return minted;
                        }
                        // §37 (`checker-notes-callres.md`): `arguments`
                        // inside a function-like container binds the global
                        // `IArguments` interface — resolvable since the
                        // bundled libs mount.
                        // The container is the nearest NON-ARROW function
                        // (an arrow's `arguments` is the enclosing one's,
                        // §37's fired leg); a class field, static block, or
                        // top level reached first declines to the old road.
                        let arguments_container = || -> bool {
                            let mut current = self.nodes.parent(id);
                            while let Some(ancestor) = current {
                                match self.nodes.kind(ancestor) {
                                    SyntaxKind::FunctionDeclaration
                                    | SyntaxKind::FunctionExpression
                                    | SyntaxKind::MethodDeclaration
                                    | SyntaxKind::Constructor
                                    | SyntaxKind::GetAccessor
                                    | SyntaxKind::SetAccessor => return true,
                                    SyntaxKind::SourceFile
                                    | SyntaxKind::PropertyDeclaration
                                    | SyntaxKind::ClassStaticBlockDeclaration => return false,
                                    _ => current = self.nodes.parent(ancestor),
                                }
                            }
                            false
                        };
                        if node.text == "arguments"
                            && arguments_container()
                            && let Some(global) = self.binder.global("IArguments")
                        {
                            let declared = self.get_declared_type_of_symbol(global);
                            if declared != self.intrinsics.error {
                                return declared;
                            }
                        }
                        // §475: a name that resolves ONLY to a TYPE
                        // PARAMETER is upstream's TS2693 ("only refers to a
                        // type") DETERMINISTICALLY — a type parameter can
                        // never carry a value meaning in any file this port
                        // has not loaded, so the §31 gate's "the port might
                        // be the one failing to resolve it" argument does
                        // not apply, and the answer is `errorType`, printed
                        // `any` (`class C<T> extends T` records `>T : any`,
                        // `typeParameterAsBaseClass`,
                        // `inheritFromGenericTypeParameter`).
                        if let Some(found) = anywhere
                            && self
                                .binder
                                .symbols()
                                .get(found)
                                .flags
                                .contains(SymbolFlags::TYPE_PARAMETER)
                        {
                            return self.intrinsics.any;
                        }
                        if anywhere.is_some()
                            || node.text == "arguments"
                            || self.file_has_import_machinery(id)
                            // §784: the JS half the §31 comment above already
                            // argues for but the gate never tested. A `.js`
                            // file is checked with `allowJs`, where upstream
                            // still reports TS2304 on an unresolved name and
                            // the oracle records `error` — so `any` is this
                            // port's own over-answer, not upstream's. The
                            // exception is a JS file carrying COMMONJS
                            // machinery: `require`/`module.exports` bring
                            // names into scope through roads this port only
                            // partly has, so an unresolved name THERE may be
                            // the port's miss, exactly as the ES-declaration
                            // test above allows. The ES-only detector cannot
                            // see them; measured at 11 RIGHT->GAP without
                            // this second half.
                            || (self.in_js_file(id)
                                && !self.file_has_commonjs_machinery(id))
                        {
                            self.intrinsics.error
                        } else {
                            self.intrinsics.any
                        }
                    }
                }
            }
            Expression::SatisfiesExpression(node) => {
                // §287: `expr satisfies T` is TRANSPARENT — upstream's
                // `checkSatisfiesExpression` reports on assignability and
                // answers `checkExpression(expression)` unchanged
                // (`checker.go`); the type is never the annotation's.
                // This arm was missing entirely, which is why `satisfies`
                // served as the §257-era stand-in for an unported operand;
                // that job now needs a genuinely unported form (the
                // unresolved-reference pins already carry it).
                node.expression.map_or(self.intrinsics.error, |inner| self.check_expression(inner))
            }
            Expression::SpreadElement(node) => {
                // §286: a spread EXPRESSION's own line is the element type it
                // contributes — `...new SymbolIterator : symbol`
                // (`iteratorSpreadInCall*`, upstream's `checkSpreadExpression`
                // through `getSpreadElementType`). The element read is the
                // §284/§285 seam; a shape that seam declines keeps the gap,
                // which is what preserves the array-literal guard's behaviour
                // (`a_spread_or_an_omitted_element_makes_the_literal_a_gap`
                // tests the LITERAL road, which checks its elements before
                // ever asking this arm).
                let Some(operand) = node.expression else { return self.intrinsics.error };
                let operand_type = self.check_expression(operand);
                self.array_spread_element_type(operand_type).unwrap_or(self.intrinsics.error)
            }
            Expression::ParenthesizedExpression(node) => {
                // §275: the JSDoc CAST — `/** @type {T} */ (expr)` asserts T,
                // upstream's `checkParenthesizedExpression` through
                // `isJSDocTypeAssertion` into the assertion worker. The inner
                // expression still checks (its lines print); the paren's own
                // type is the tag's, in regular form as an assertion answers.
                // A tag type that does not compute falls through to the
                // transparent-paren road, keeping the gap.
                if let Some(id) = node.node_id
                    && self.in_js_file(id)
                    && let Some(annotation) = self.jsdoc_cast_annotation(id)
                {
                    let asserted = self.get_type_from_type_node(annotation);
                    if asserted != self.intrinsics.error {
                        if let Some(inner) = node.expression {
                            self.check_expression(inner);
                        }
                        return self.get_regular_type_of_literal_type(asserted);
                    }
                }
                node.expression.map_or(self.intrinsics.error, |inner| self.check_expression(inner))
            }
            // `checkTemplateExpression` (`checker.go:7976`): spans check;
            // an all-literal template folds to the fresh string literal
            // (the evaluator's observable for string/number parts); else
            // `string` — with the const-context, element-access-argument,
            // and unfoldable-literal shapes declined to gaps
            // (`checker-notes-narrow.md` §24).
            Expression::TemplateExpression(node) => self.check_template_expression(node),
            Expression::BinaryExpression(node) => self.check_binary_expression(node),
            Expression::PropertyAccessExpression(node) => {
                self.check_property_access_expression(node)
            }
            Expression::CallExpression(node) => self.check_call_expression(node),
            Expression::ElementAccessExpression(node) => self.check_element_access_expression(node),
            // `checkObjectLiteral` (`checker.go:13144`). Distinct from the
            // `{ a: string }` *type* node in `crate::declared`, which prints
            // identically and is computed by unrelated code — see
            // [`crate::objects`].
            Expression::ObjectLiteralExpression(node) => self.check_object_literal(node),
            // `checkArrayLiteral` (`checker.go:8021`). The element union meets
            // the array type, both of which already existed — see
            // [`crate::array_literals`].
            Expression::ArrayLiteralExpression(node) => self.check_array_literal(node),
            // `checkAssertion` (`checker.go:12287`). Both spellings of the same
            // construct, and `const` is recognised before the type node is
            // resolved — see [`crate::assertions`].
            Expression::AsExpression(node) => {
                node.node_id.map_or(self.intrinsics.error, |id| self.check_assertion(id))
            }
            Expression::TypeAssertion(node) => {
                node.node_id.map_or(self.intrinsics.error, |id| self.check_assertion(id))
            }
            // `checkFunctionExpressionOrObjectLiteralMethod` (`checker.go:9077`).
            // Both kinds answer through the function's own symbol, which is the
            // same arm `getTypeOfFuncClassEnumModule` serves — see
            // [`Checker::get_type_of_function_expression`] for the one place that
            // is not safe, an unannotated parameter under a contextual type.
            Expression::FunctionExpression(node) => node
                .node_id
                .map_or(self.intrinsics.error, |id| self.get_type_of_function_expression(id)),
            Expression::ArrowFunction(node) => node
                .node_id
                .map_or(self.intrinsics.error, |id| self.get_type_of_function_expression(id)),
            // `checkTypeOfExpression` (`checker.go:10617`). The operand's type is
            // irrelevant to the answer — see below.
            Expression::TypeOfExpression(node) => self.check_type_of_expression(node),
            // `checkVoidExpression` (`checker.go:10633`): the operand checks
            // for its own lines; the expression is `undefined`.
            Expression::VoidExpression(node) => {
                // §293: the answer does not consult the operand — upstream
                // returns `undefinedType` whatever it is, exactly as the
                // comparison arms return `boolean`. The error-propagation
                // this arm carried was the per-site deviation §271/§291
                // retired at their sites; `void e.toUpperCase()` on an
                // `unknown` catch variable is `undefined`
                // (`useUnknownInCatchVariables01`).
                let Some(operand) = node.expression else { return self.intrinsics.error };
                self.check_expression(operand);
                self.intrinsics.undefined
            }
            // `checkDeleteExpression` (`checker.go:10570`): the operand
            // checks; the expression is `boolean` — unconditionally, the
            // same §293 rule as `void`.
            Expression::DeleteExpression(node) => {
                let Some(operand) = node.expression else { return self.intrinsics.error };
                self.check_expression(operand);
                self.intrinsics.boolean
            }
            // `checkPrefixUnaryExpression` (`checker.go:10855`).
            Expression::PrefixUnaryExpression(node) => self.check_prefix_unary_expression(node),
            // `checkNonNullAssertion` (`checker.go:10622`): the operand's
            // non-nullable remainder (`checker-notes-narrow.md` §26), and for a
            // chain link `checkNonNullChain` (`checker.go:10631`) instead — §884.
            Expression::NonNullExpression(node) => {
                let Some(operand) = node.expression else { return self.intrinsics.error };
                let checked = self.check_expression(operand);
                if checked == self.intrinsics.error {
                    return self.intrinsics.error;
                }
                // `!` takes the nullability off its OPERAND. The chain's own
                // `undefined` belongs to the chain's end, so it is stripped here
                // and re-unioned by `propagateOptionalTypeMarker` — without that
                // second half, `o2?.["b"]!` answers `{ c: string; }` where
                // upstream answers `{ c: string; } | undefined`.
                //
                // Which `!` is a chain link is NOT "the spine has `?.`" — see
                // [`Checker::non_null_is_optional_chain`], where the asymmetry
                // between an inner and a trailing `!` is owned.
                let Some(id) = node.node_id else { return self.get_non_nullable_type(checked) };
                if !self.non_null_is_optional_chain(id) {
                    return self.get_non_nullable_type(checked);
                }
                // A `NonNullExpression` carries no `?.` of its own, so it is
                // never an optional-chain ROOT: the `false` is upstream's
                // `IsExpressionOfOptionalChainRoot(node.Expression())` answering
                // for an operand whose parent has no question-dot token.
                let non_optional =
                    self.get_optional_expression_type(checked, operand.node_id(), false);
                let result = self.get_non_nullable_type(non_optional);
                self.propagate_optional_type_marker_at(
                    node.node_id,
                    result,
                    non_optional != checked,
                )
            }
            // `checkPostfixUnaryExpression` (`checker.go:10909`).
            Expression::PostfixUnaryExpression(node) => self.check_postfix_unary_expression(node),
            // `checkConditionalExpression` (`checker.go:10934`).
            Expression::ConditionalExpression(node) => self.check_conditional_expression(node),
            // `resolveNewExpression` (`checker.go:8575`).
            Expression::NewExpression(node) => self.check_new_expression(node),
            // `checkYieldExpression` (`checker.go:10952`).
            Expression::YieldExpression(node) => self.check_yield_expression(node),
            // `checkAwaitExpression` (`checker.go:10845`).
            Expression::AwaitExpression(node) => self.check_await_expression(node),
            // `checkTaggedTemplateExpression` (`checker.go:10034`) — see
            // [`crate::calls`], which owns signature resolution.
            Expression::TaggedTemplateExpression(node) => {
                self.check_tagged_template_expression(node)
            }
            // `checkJsxElement` (`jsx.go:72`) → `getJsxElementTypeAt`
            // (`jsx.go:1275`): an element or fragment expression has the
            // declared type of the in-scope `JSX` namespace's `Element`
            // export. Sized at 759 forecast lines with the bar in
            // `checker-notes-jsx.md`; when `JSX` or `Element` is absent the
            // arm gaps — that refusal is the bar's registered falsifier.
            Expression::JsxElement(node) => self.check_jsx_element(node.node_id),
            Expression::JsxSelfClosingElement(node) => self.check_jsx_element(node.node_id),
            Expression::JsxFragment(node) => self.check_jsx_element(node.node_id),
            // `checkClassExpression` (`checker.go:11832`) is
            // `getTypeOfSymbol(getSymbolOfDeclaration(node))` — the class's
            // static side, printed `typeof C`.
            //
            // **§168 refused this arm and the refusal was right about what it
            // measured and wider than it.** Its finding was that the arm prints
            // `typeof __class` — the binder's synthetic name — where the
            // baseline wants `typeof V`, the name of the variable the class
            // was assigned to. That is true of an **anonymous** class
            // expression and says nothing about a **named** one, whose symbol
            // carries the name the source wrote:
            // `compiler/exportDefaultParenthesizeES6` records
            // `>class Foo {} : typeof Foo`.
            //
            // So the arm is taken exactly where §168's measurement applies and
            // refused exactly where it does not. The anonymous case still needs
            // the per-site contextual naming that refused §145, §156, §158 and
            // §168, and `functionsInClassExpressions` and
            // `implementsInClassExpression` are still gaps because of it.
            // `docs/conventions.md` corollary 11. §207.
            //
            // §305 reopened the anonymous case: what §168 read as per-site
            // contextual naming is, upstream, a declaration walk baked into
            // `getNameOfSymbolAsWritten` (`nodebuilderimpl.go:1005`) — parent
            // `VariableDeclaration`'s name, else `(Anonymous class)` — so the
            // guard on `name` came off and the naming lives with the other
            // spellings in `symbols.rs`. A symbol the walk cannot name still
            // refuses there, exactly as this guard refused here.
            Expression::ClassExpression(node) => {
                let Some(id) = node.node_id else { return self.intrinsics.error };
                let Some(symbol) = self.binder.symbol_of(id) else {
                    return self.intrinsics.error;
                };
                self.get_type_of_symbol(symbol)
            }
            _ => self.intrinsics.error,
        }
    }

    /// `getJsxType(JsxNames.Element, location)` (`jsx.go:1275`, `:1295`),
    /// reduced to the resolving path: the `JSX` namespace in scope at the
    /// element, its `Element` export, that symbol's declared type. Every
    /// missing hop is a gap — `errorType` — never a substitute.
    pub(crate) fn check_jsx_element(&mut self, id: Option<tsr_ast::NodeId>) -> TypeId {
        let Some(id) = id else { return self.intrinsics.error };
        // §249, INDUCED AND REVERTED, both variants measured. Recorded here
        // because the induction is attractive and the corpus refuses it.
        //
        // The reasoning was: an absent `JSX.Element` is not a failure to
        // compute but a computed `any` — upstream's `getJsxElementTypeAt`
        // answers nil when the global is missing and the element falls back to
        // `anyType`. It looked confirmed, because a method whose body returns a
        // JSX element declines its whole signature on an `error` return
        // (`>render : () => any` in `jsxFactoryIdentifierWithAbsentParameter`,
        // whose `namespace JSX` declares `IntrinsicElements` and no `Element`).
        //
        // MEASURED, against a baseline accepted on the same tree with the
        // change stashed:
        //
        //   both branches -> any:     +3 cases, GAP->RIGHT 21, GAP->WRONG 7,
        //                             **RIGHT->WRONG 69**
        //   absent-Element only:      +0 cases, GAP->RIGHT 7,  GAP->WRONG 3,
        //                             **RIGHT->WRONG 36**
        //
        // Both DAMAGE correct answers, which is the one thing a per-case tally
        // cannot show (conventions corollary 24) — the +3 looked like a win.
        // The `error` here is load-bearing for between 36 and 69 lines, so
        // whatever upstream does when the global is missing, it is not what
        // this port's surrounding roads assume.
        //
        // What a correct version would need: the distinction upstream draws
        // between "no JSX namespace at all", "a JSX namespace without
        // `Element`", and the `jsxFactory`/`jsxFragmentFactory` pragma cases
        // that dominate the adverse lists (`inlineJsxFactoryDeclarations`,
        // `inlineJsxAndJsxFragPragmaOverridesCompilerOptions`). Those pragmas
        // are the population that regressed under BOTH variants and are the
        // thing to read before trying again.
        //
        // Established before building, and still true: the method road is NOT
        // the defect. A method with an inferred `any` return builds
        // `() => any` correctly — probed with
        // `declare const a: any; class C { m() { return a; } }`.
        let Some(jsx) = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            id,
            "JSX",
            tsr_binder::SymbolFlags::NAMESPACE,
        ) else {
            return self.intrinsics.error;
        };
        let jsx = self.binder.merged_symbol(jsx);
        let Some(&element) = self.binder.symbols().get(jsx).exports.get("Element") else {
            return self.intrinsics.error;
        };
        self.get_declared_type_of_symbol(element)
    }

    /// Ported from `Checker.checkTypeOfExpression` (`checker.go:10617`).
    ///
    /// # The operand's type does not reach the answer
    ///
    /// Upstream is two lines: check the operand, then return `typeofType`
    /// regardless of what came back. So `typeof` is the one expression form here
    /// whose result is **not** weakened by a gap in its operand —
    /// `typeof someUnportedThing` is still the full union, and answering
    /// `errorType` because the operand gapped would invent a gap upstream does not
    /// have. The operand is still checked, because that is what populates the
    /// operand's own line in the output.
    ///
    /// `typeofType` is built at `checker.go:1052` as the union of the sorted keys
    /// of `typeofNEFacts`, so the constituent order is alphabetical rather than
    /// the order a human would list them. 265 baseline lines record it exactly:
    ///
    /// ```text
    /// >typeof x : "bigint" | "boolean" | "function" | "number" | "object" | "string" | "symbol" | "undefined"
    /// ```
    fn check_type_of_expression(&mut self, node: &tsr_ast::TypeOfExpression<'_>) -> TypeId {
        if let Some(operand) = node.expression {
            self.check_expression(operand);
        }
        self.get_typeof_type()
    }

    /// `typeofType` (`checker.go:1052`).
    ///
    /// Rebuilt per call rather than cached on the checker: both the string
    /// literal types and the union are interned, so this is a lookup after the
    /// first call and needs no field on `Checker`.
    fn get_typeof_type(&mut self) -> TypeId {
        // `slices.Sorted(maps.Keys(typeofNEFacts))` — alphabetical, and the
        // union's constituent order follows the order the types are created in,
        // so this list must stay sorted.
        const NAMES: [&str; 8] =
            ["bigint", "boolean", "function", "number", "object", "string", "symbol", "undefined"];
        let types = NAMES
            .iter()
            .map(|name| {
                // `getStringLiteralType` yields the **regular** form; only a
                // string literal *expression* is fresh.
                self.store.intern_literal(
                    TypeFlags::STRING_LITERAL,
                    TypeData::StringLiteral((*name).to_string()),
                    false,
                )
            })
            .collect::<Vec<_>>();
        self.get_union_type(&types)
    }

    /// Ported from `Checker.checkPrefixUnaryExpression` (`checker.go:10855`).
    ///
    /// # A negated numeric literal is a literal, not `number`
    ///
    /// Upstream special-cases the operand being a numeric or bigint literal
    /// *before* it looks at the operator's general rule, so `-1` is the literal
    /// type `-1` and not `number`. The baselines record `>-1 : -1` and `>+1 : 1`,
    /// against `>-x : number` and `>-true : number` for every non-literal operand.
    /// Missing this case would be a plausible wrong line on every negative
    /// constant in the corpus.
    ///
    /// Not ported, each a gap: an operand this port cannot type (see
    /// [`Self::unary_result_type`] for why that is a gap rather than `number`),
    /// and `!` on an operand whose truthiness is not decidable here.
    fn check_prefix_unary_expression(
        &mut self,
        node: &tsr_ast::PrefixUnaryExpression<'_>,
    ) -> TypeId {
        let error = self.intrinsics.error;
        let Some(operand) = node.operand else { return error };
        let operand_type = self.check_expression(operand);
        let operator = node.operator.kind;

        // The literal special cases, which run before the operator's general
        // rule. `getFreshTypeOfLiteralType` — a unary expression is an
        // expression, so the literal it produces is fresh, exactly like the
        // `NumericLiteral` arm above.
        if let Expression::NumericLiteral(literal) = operand
            && matches!(operator, SyntaxKind::MinusToken | SyntaxKind::PlusToken)
        {
            let normalised = printing::normalise_number(literal.text);
            let text = if operator == SyntaxKind::MinusToken {
                negate_number_text(&normalised)
            } else {
                Some(normalised)
            };
            // An unparseable literal keeps `normalise_number`'s fallback of the
            // source text, which cannot be negated meaningfully; the scanner has
            // already reported it, so this is a gap rather than a guess.
            if let Some(text) = text {
                return self.store.intern_literal(
                    TypeFlags::NUMBER_LITERAL,
                    TypeData::NumberLiteral(text),
                    true,
                );
            }
            return error;
        }
        // The bigint half of the same special case. `-1n` is the literal `-1n`;
        // `+1n` is not a case at all, because unary `+` on a bigint is an error
        // upstream rather than a literal.
        if let Expression::BigIntLiteral(literal) = operand
            && operator == SyntaxKind::MinusToken
        {
            let digits = literal.text.trim_end_matches('n');
            return self.store.intern_literal(
                TypeFlags::BIG_INT_LITERAL,
                TypeData::BigIntLiteral(format!("-{digits}")),
                true,
            );
        }

        match operator {
            // `+` returns `numberType` unconditionally — upstream reports on a
            // bigint operand but still answers `number`, so there is no bigint
            // gap on this arm.
            SyntaxKind::PlusToken => self.intrinsics.number,
            SyntaxKind::MinusToken | SyntaxKind::TildeToken => self.unary_result_type(operand_type),
            SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken => {
                self.unary_result_type(operand_type)
            }
            SyntaxKind::ExclamationToken => self.negated_truthiness_type(operand_type),
            _ => error,
        }
    }

    /// Ported from `Checker.checkPostfixUnaryExpression` (`checker.go:10909`).
    ///
    /// `x++` and `x--` have no literal special case — upstream goes straight to
    /// `getUnaryResultType`, which is why `>i++ : number` even when `i` is the
    /// literal type `0`.
    fn check_postfix_unary_expression(
        &mut self,
        node: &tsr_ast::PostfixUnaryExpression<'_>,
    ) -> TypeId {
        let Some(operand) = node.operand else { return self.intrinsics.error };
        let operand_type = self.check_expression(operand);
        self.unary_result_type(operand_type)
    }

    /// Ported from `Checker.getUnaryResultType` (`checker.go:10923`).
    ///
    /// # The bigint arm is a gap; an un-typed operand is NOT
    ///
    /// Upstream answers `number` for everything that is not bigint-like, and
    /// `bigint` or `number | bigint` when it is. The bigint arm needs
    /// `numberOrBigIntType` and `isTypeAssignableToKind` — an assignability
    /// question this port cannot ask — so a bigint-like operand is a gap.
    ///
    /// **An `error` operand used to be a gap too, and that was wrong (§192).**
    /// The reasoning recorded here was that the answer depends only on whether
    /// the operand is bigint-like, and an operand this port could not type is
    /// precisely the case where that is unknown — so answering `number` would
    /// be a guess. But upstream is not guessing either: `maybeTypeOfKind` is a
    /// **flag test**, upstream's `errorType` is `TypeFlagsAny`-carrying, and an
    /// `Any` type does not carry the bigint bit. `getUnaryResultType` therefore
    /// falls straight to `numberType` for it, and **cannot return an error type
    /// at all** — `x[x++]++` in `compiler/decrementAndIncrementOperators` is
    /// `number` in upstream's baseline over an operand upstream itself renders
    /// as `any`.
    ///
    /// The paragraph below already said exactly this for `any` and `unknown`.
    /// The `error` arm contradicted its own neighbour for two sessions.
    /// Removing it: **+78 lines, 0 lost, +2 whole cases** across seventeen
    /// cases, seven of them the `incrementOperatorWith*` / `decrementOperator*`
    /// families whose whole subject this is.
    fn unary_result_type(&mut self, operand: TypeId) -> TypeId {
        let flags = self.store.get(operand).flags;
        if flags.intersects(TypeFlags::BIG_INT_LIKE) {
            return self.intrinsics.error;
        }
        // `any`/`unknown` answer `number`: `maybeTypeOfKind` is a FLAG test
        // (`checker.go:10923` reads `operandType`'s flags against
        // `TypeFlagsBigIntLike`), and neither carries the bigint bit, so
        // upstream falls straight to `numberType` —
        // `bitwiseNotOperatorWithAnyOtherType.types` records `~ANY1 : number`
        // throughout. The comment this replaces claimed `maybeTypeOfKind`
        // "answers yes for them" — an intuition falsified by its own anchor,
        // caught when the §9.4 initial-type change exposed 8 such lines.
        self.intrinsics.number
    }

    /// The `!` arm of `checkPrefixUnaryExpression` (`checker.go:10887`).
    ///
    /// Upstream calls `getTypeFacts(operandType, TypeFactsTruthy|TypeFactsFalsy)`
    /// and answers `false` when the operand can only be truthy, `true` when it
    /// can only be falsy, and `boolean` when it could be either. `>!x : boolean`
    /// is the common baseline line, but `!` on a literal is not `boolean` and
    /// answering `boolean` everywhere would be a wrong line on each one.
    ///
    /// # Only the decidable half of `getTypeFacts` is ported
    ///
    /// A unit type has one truthiness and the primitives have both, which is
    /// enough for the corpus shapes. Everything else — unions, objects,
    /// intersections, type parameters, `never` — is a gap. An object type is
    /// *always* truthy upstream and so would answer `false`, but that holds only
    /// once `TypeFacts` distinguishes an object from a possibly-`undefined` one,
    /// and guessing it here would be a wrong line on every optional value.
    fn negated_truthiness_type(&mut self, operand: TypeId) -> TypeId {
        if operand == self.intrinsics.error {
            return self.intrinsics.error;
        }
        let (flags, data) = {
            let t = self.store.get(operand);
            (t.flags, t.data.clone())
        };
        // Always falsy: `!null`, `!undefined`, `!void` are all `true`.
        if flags.intersects(TypeFlags::NULLABLE | TypeFlags::VOID) {
            return self.intrinsics.true_type;
        }
        let falsy = match data {
            TypeData::BooleanLiteral(value) => !value,
            TypeData::StringLiteral(text) => text.is_empty(),
            // The normalised text, so `0`, `0.0` and `0x0` all arrive as `"0"`,
            // and `0n` likewise for the bigint half.
            TypeData::NumberLiteral(text) | TypeData::BigIntLiteral(text) => text == "0",
            // §291: a UNION folds its constituents' truthiness — all-truthy
            // is `false`, all-falsy `true`, a mix `boolean`, and any
            // undecidable constituent keeps the gap
            // (`!abcOrXyzOrNumber : boolean`,
            // `stringLiteralTypesWithVariousOperators01`).
            TypeData::Union { types: constituents, .. } => {
                let mut saw_true = false;
                let mut saw_false = false;
                let mut saw_boolean = false;
                for constituent in constituents {
                    let negated = self.negated_truthiness_type(constituent);
                    if negated == self.intrinsics.error {
                        return self.intrinsics.error;
                    } else if negated == self.intrinsics.true_type {
                        saw_true = true;
                    } else if negated == self.intrinsics.false_type {
                        saw_false = true;
                    } else {
                        saw_boolean = true;
                    }
                }
                return if saw_boolean || (saw_true && saw_false) {
                    self.intrinsics.boolean
                } else if saw_true {
                    self.intrinsics.true_type
                } else {
                    self.intrinsics.false_type
                };
            }
            _ => {
                // Both truthiness values are possible for the unit-less
                // primitives, which is upstream's `Truthy|Falsy` and prints
                // `boolean`.
                return if flags.intersects(
                    TypeFlags::STRING
                        | TypeFlags::NUMBER
                        | TypeFlags::BIG_INT
                        | TypeFlags::BOOLEAN
                        | TypeFlags::ANY_OR_UNKNOWN,
                ) {
                    self.intrinsics.boolean
                } else if flags.intersects(TypeFlags::OBJECT | TypeFlags::ES_SYMBOL_LIKE) {
                    // §291: an object or symbol operand is ALWAYS truthy
                    // (upstream's TypeFacts), so its negation is the `false`
                    // literal.
                    self.intrinsics.false_type
                } else {
                    self.intrinsics.error
                };
            }
        };
        if falsy { self.intrinsics.true_type } else { self.intrinsics.false_type }
    }

    /// Ported from `Checker.checkThisExpression` (`checker.go:12077`), reduced to
    /// the class case.
    ///
    /// **`this` inside a class is the class's `this` *type*, printed `this`** —
    /// not the class type printed `C`. Upstream models it as a type parameter
    /// whose constraint is the class (`checker.go:17334`), and the corpus records
    /// it that way: `>this : this`. Its members are the class's, which is what
    /// makes `this.x` work.
    ///
    /// Arrow functions are transparent to `this` and a plain `function` is not,
    /// which is the only part of upstream's container walk that changes an
    /// answer here.
    ///
    /// **The container list is upstream's and the walk stops where it stops.**
    /// `getThisContainer` (`checker.go:12188`) names sixteen kinds; the three
    /// that this walk answers for on their own account rather than by falling
    /// through are the module/enum bodies (`any`), the file (`typeof globalThis`
    /// in a script, `undefined` in a module — §200) and a plain function
    /// (`any`). A container reached by falling *past* one of those would be the
    /// wrong container, which is what §200's first measurement cost six cases
    /// to learn.
    /// §142: whether an object literal declares any computed-name member.
    fn literal_has_computed_member(&self, literal: NodeId) -> bool {
        match self.node_map.get(literal) {
            Some(tsr_ast::Node::ObjectLiteralExpression(node)) => {
                node.properties.iter().any(|property| {
                    let name = match property {
                        tsr_ast::ObjectLiteralElementLike::PropertyAssignment(p) => Some(p.name),
                        tsr_ast::ObjectLiteralElementLike::MethodDeclaration(m) => Some(m.name),
                        tsr_ast::ObjectLiteralElementLike::GetAccessorDeclaration(a) => {
                            Some(a.name)
                        }
                        tsr_ast::ObjectLiteralElementLike::SetAccessorDeclaration(a) => {
                            Some(a.name)
                        }
                        _ => None,
                    };
                    matches!(name, Some(tsr_ast::PropertyName::ComputedPropertyName(_)))
                })
            }
            _ => false,
        }
    }

    /// Test-only wrapper: [`Checker::check_this_expression`] is `pub(crate)`
    /// and the `this` road has no other public entry point.
    pub fn check_this_expression_for_test(&mut self, node: NodeId) -> TypeId {
        self.check_this_expression(node)
    }

    /// The keyable name of a method declaration, for the member lookups §928's
    /// arm makes. `None` for a computed or numeric name, which
    /// [`Checker::get_property_of_type`] cannot be keyed by anyway.
    fn method_name_text(&self, method: NodeId) -> Option<String> {
        let Some(tsr_ast::Node::MethodDeclaration(declaration)) = self.node_map.get(method) else {
            return None;
        };
        match declaration.name {
            tsr_ast::PropertyName::Identifier(name) => Some(name.text.to_owned()),
            tsr_ast::PropertyName::StringLiteral(name) => Some(name.text.to_owned()),
            _ => None,
        }
    }

    /// The `this` type an object-literal method inherits from the contextual
    /// signature of the member it implements — §928, the second branch of
    /// `getContextualThisParameterType` (`checker.go:29104`).
    ///
    /// The member is looked up from the LITERAL's contextual type rather than
    /// through `get_contextual_type(method)`, because upstream's
    /// object-literal-method arm (`checker.go:29964`) is not ported and that
    /// call answers `None` here.
    pub(crate) fn contextual_this_parameter_type(&mut self, method: NodeId) -> Option<TypeId> {
        if self.nodes.kind(method) != SyntaxKind::MethodDeclaration {
            return None;
        }
        let literal = self.nodes.parent(method)?;
        if self.nodes.kind(literal) != SyntaxKind::ObjectLiteralExpression {
            return None;
        }
        let contextual = self.get_contextual_type(literal)?;
        let name = self.method_name_text(method)?;
        let member = self
            .get_property_of_type(contextual, &name)
            .map(|property| self.get_type_of_symbol(property))
            .or_else(|| self.contextual_property_type(contextual, &name))?;
        let signature = self.contextual_signature_of_type(member)?;
        Some(signature.this_parameter?.r#type)
    }

    pub(crate) fn check_this_expression(&mut self, node: NodeId) -> TypeId {
        let mut current = self.nodes.parent(node);
        while let Some(id) = current {
            // **Arm 1 shadows arm 2, and the order is upstream's.**
            // `tryGetThisTypeAtEx` (`checker.go:12146`) asks
            // `ast.IsFunctionLike(container)` *before* it asks
            // `ast.IsClassLike(container.Parent)`, so a **method** carrying a
            // `this` parameter answers the annotation and never reaches the
            // class's `thisType`. Testing class-ness first is the natural port
            // and it is backwards: it would print `this` where upstream prints
            // the written annotation. A method is function-like.
            //
            // Falling through when there is no `this` parameter is also
            // upstream's: `getThisTypeOfSignature` answers `nil`,
            // `getContextualThisParameterType` is unported and answers nothing
            // here, and the class arm below is what runs next.
            if let Some(this_type) = self.this_parameter_type(id) {
                return this_type;
            }
            // §928: **upstream's SECOND branch, which §912's comment named and
            // did not build.** `getContextualThisParameterType`
            // (`checker.go:29104`) asks for the method's own contextual
            // SIGNATURE and, when it carries a `this` parameter, answers that —
            // *before* either of the object-literal branches below.
            //
            // `let impl: I = { em() { return this.a; } }` with
            // `interface I { em(this: { a: number }): number }` answered `any`
            // for `this` while the same annotation written directly on a
            // function typed correctly. Every piece was already here:
            // `contextual_property_type` finds the member,
            // `contextual_signature_of_type` reads its signature, and
            // `Signature::this_parameter` has held the answer since the
            // signature module was written.
            //
            // **It sits outside the `no_implicit_this` gate deliberately.** The
            // first draft put it inside the §912 arm, which is so gated, and
            // measured ZERO transitions — `thisTypeInFunctions` is
            // `@strict: false`. Upstream gates only the object-literal
            // fallback on `noImplicitThis` (`checker.go:29119`); the contextual
            // signature's own `this` is an annotation the user wrote and is
            // read in every mode.
            if let Some(this_type) = self.contextual_this_parameter_type(id) {
                return this_type;
            }
            match self.nodes.kind(id) {
                // An arrow function is transparent — it keeps the enclosing
                // `this` — which is the same as walking past any other node, so
                // it needs no arm of its own. It is named here because that
                // transparency is a rule and not an omission.
                //
                // Opaque: a plain function rebinds `this` — to `any`, in
                // every mode (`tryGetThisTypeAtEx`'s fallthrough; TS2683 is
                // a diagnostic under `noImplicitThis`, not a type change) —
                // `checker-notes-narrow.md` §39.
                SyntaxKind::FunctionDeclaration | SyntaxKind::FunctionExpression => {
                    // §142 iteration 5: a FUNCTION EXPRESSION that is a
                    // property VALUE of an object literal takes the
                    // literal-self mint too (`f: function() { return
                    // this.d; }` — the head case's residual 7), same gates.
                    if self.nodes.kind(id) == SyntaxKind::FunctionExpression
                        && let Some(assignment) = self.nodes.parent(id)
                        && self.nodes.kind(assignment) == SyntaxKind::PropertyAssignment
                        && let Some(literal) = self.nodes.parent(assignment)
                        && self.nodes.kind(literal) == SyntaxKind::ObjectLiteralExpression
                        && self.no_implicit_this
                        && self.has_no_contextual_type(literal)
                        && !self.in_js_file(literal)
                        && !self.literal_has_computed_member(literal)
                    {
                        if let Some(&cached) = self.literal_this_types.get(&literal) {
                            return cached;
                        }
                        if let Some(symbol) = self.binder.symbol_of(literal) {
                            let minted = self.store.new_named(
                                crate::flags::TypeFlags::OBJECT,
                                "this".to_string(),
                                Some(symbol),
                            );
                            self.literal_this_types.insert(literal, minted);
                            return minted;
                        }
                    }
                    return self.intrinsics.any;
                }
                // §912: `getContextualThisParameterType`'s `noImplicitThis`
                // branch — when the containing object literal HAS a contextual
                // type, `this` inside its method is that type **discriminated by
                // the literal's own members**, not the whole union:
                //
                // ```ts
                // function foo(bar: X | Y) { }
                // foo({ type: 'y', value: 'done', method() { this } })  // this : Y
                // ```
                //
                // §911 built this and answered `X | Y`, which measured 4
                // `RIGHT→WRONG` and — the tell — **5 `GAP→WRONG` in the very case
                // it was meant to fix**. The want is the constituent the literal's
                // `type: 'y'` selects, and `discriminate_union_root`
                // (`symbols.rs`) has done exactly that selection since §750's
                // family. It was never called from here.
                //
                // The `ThisType<T>` marker half stays unported: upstream prefers a
                // `ThisType<T>` member of the contextual type over the type
                // itself, and this port has no such lookup.
                SyntaxKind::MethodDeclaration
                    if self.nodes.parent(id).is_some_and(|parent| {
                        self.nodes.kind(parent) == SyntaxKind::ObjectLiteralExpression
                            && self.no_implicit_this
                            && !self.in_js_file(parent)
                            && !self.has_no_contextual_type(parent)
                    }) =>
                {
                    let Some(literal) = self.nodes.parent(id) else {
                        return self.intrinsics.error;
                    };
                    // **Only a UNION contextual type is used.** Upstream's first
                    // branch — the method's own contextual SIGNATURE carrying a
                    // `this` parameter — wins ahead of the literal one, and this
                    // port's `contextual_signature` cannot reach an INDEX
                    // signature, which is where `thisTypeInFunctions2` gets its
                    // `(this: any, …) => any`. Without the restriction the
                    // literal road ran there instead and returned the index
                    // signature's whole union: 4 `RIGHT→WRONG`.
                    //
                    // A union is exactly the shape discrimination is for, and it
                    // is the shape this arm exists to serve, so restricting to it
                    // costs nothing measured and removes the whole adverse set.
                    // The non-union case keeps `any`, which is what it answered
                    // before.
                    let contextual = self.get_contextual_type(literal);
                    match contextual {
                        Some(contextual)
                            if matches!(
                                self.store.get(contextual).data,
                                crate::types::TypeData::Union { .. }
                            ) =>
                        {
                            return self.discriminate_union_root(contextual, literal);
                        }
                        _ => return self.intrinsics.any,
                    }
                }
                // §142 (parked state rebuilt for the looseThis probe):
                // literal-self this, methods only, all four gates.
                SyntaxKind::MethodDeclaration
                    if self.nodes.parent(id).is_some_and(|parent| {
                        self.nodes.kind(parent) == SyntaxKind::ObjectLiteralExpression
                            && self.no_implicit_this
                            && self.has_no_contextual_type(parent)
                            && !self.in_js_file(parent)
                            && !self.literal_has_computed_member(parent)
                    }) =>
                {
                    let Some(literal) = self.nodes.parent(id) else {
                        return self.intrinsics.error;
                    };
                    if let Some(&cached) = self.literal_this_types.get(&literal) {
                        return cached;
                    }
                    let Some(symbol) = self.binder.symbol_of(literal) else {
                        return self.intrinsics.any;
                    };
                    let minted = self.store.new_named(
                        crate::flags::TypeFlags::OBJECT,
                        "this".to_string(),
                        Some(symbol),
                    );
                    self.literal_this_types.insert(literal, minted);
                    return minted;
                }
                SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression => {
                    let Some(symbol) = self.binder.symbol_of(id) else {
                        return self.intrinsics.error;
                    };
                    // §69 (`checker-notes-narrow.md`): `this` in a STATIC
                    // member is the class's STATIC side — `typeof C`
                    // (`tryGetThisTypeAtEx`'s `getTypeOfSymbol(classSymbol)`
                    // arm for static containers; 58 corpus lines).
                    let mut member = self.nodes.parent(node);
                    let static_container = loop {
                        let Some(m) = member else { break false };
                        if m == id {
                            break false;
                        }
                        let is_member = matches!(
                            self.nodes.kind(m),
                            SyntaxKind::MethodDeclaration
                                | SyntaxKind::PropertyDeclaration
                                | SyntaxKind::GetAccessor
                                | SyntaxKind::SetAccessor
                        );
                        if is_member && self.nodes.parent(m) == Some(id) {
                            let has_static = self
                                .node_map
                                .get(m)
                                .and_then(|n| match n {
                                    Node::MethodDeclaration(d) => Some(&d.modifiers),
                                    Node::PropertyDeclaration(d) => Some(&d.modifiers),
                                    Node::GetAccessorDeclaration(d) => Some(&d.modifiers),
                                    Node::SetAccessorDeclaration(d) => Some(&d.modifiers),
                                    _ => None,
                                })
                                .is_some_and(|modifiers| {
                                    modifiers.iter().any(|modifier| {
                                        matches!(modifier, tsr_ast::ModifierLike::Token(token)
                                            if token.kind == SyntaxKind::StaticKeyword)
                                    })
                                });
                            break has_static;
                        }
                        if matches!(self.nodes.kind(m), SyntaxKind::ClassStaticBlockDeclaration)
                            && self.nodes.parent(m) == Some(id)
                        {
                            break true;
                        }
                        member = self.nodes.parent(m);
                    };
                    if static_container {
                        return self.get_type_of_symbol(symbol);
                    }
                    if let Some(&cached) = self.this_types.get(&symbol) {
                        return cached;
                    }
                    let this_type = self.store.new_named(
                        TypeFlags::TYPE_PARAMETER,
                        "this".to_string(),
                        Some(symbol),
                    );
                    self.this_types.insert(symbol, this_type);
                    return this_type;
                }
                // **`getThisContainer` stops here, so the walk must too**
                // (`checker.go:12225-12228`). A module or enum body is a
                // `this` container in upstream's list, and
                // `tryGetThisTypeAtEx` then finds it neither function-like nor
                // class-parented nor a source file and answers `nil`, which
                // `checkThisExpression` turns into `anyType` — beside a
                // reported TS2331/TS2332.
                //
                // Without these two arms the SourceFile arm below reaches
                // through a `namespace` or `enum` body and answers
                // `typeof globalThis`. That is not hypothetical: it cost **six
                // cases** on §200's first measurement (`thisInModule`,
                // `this_inside-enum-should-not-be-allowed`, `topLevelLambda`
                // and kin), which is how the omission was found.
                SyntaxKind::ModuleDeclaration | SyntaxKind::EnumDeclaration => {
                    return self.intrinsics.any;
                }
                // `tryGetThisTypeAtEx`'s last arm (`checker.go:12175-12184`):
                // at the top level of a **script**, `this` is
                // `getTypeOfSymbol(globalThisSymbol)`, printed
                // `typeof globalThis`; at the top level of an external
                // **module** it is `undefinedType`, because a module body's
                // `this` is `undefined` at runtime.
                //
                // The walk used to fall off the end here and answer `error` —
                // recorded at the site as "every other container is a gap:
                // upstream answers `anyType` there through a signature's
                // `this` parameter or a contextual type, and neither exists
                // yet". True of a plain function; **false of the file**, where
                // upstream reads a symbol this port already mints. §200.
                //
                // The mint is §33's, taken from the same memo so the two
                // spellings cannot drift: `>this : typeof globalThis` and
                // `>globalThis : typeof globalThis` are one type in upstream's
                // baselines and must be one `TypeId` here.
                SyntaxKind::SourceFile => {
                    let is_module = self.node_map.get(id).is_some_and(|node| match node {
                        Node::SourceFile(source) => tsr_binder::is_external_module(source),
                        _ => false,
                    });
                    if is_module {
                        return self.intrinsics.undefined;
                    }
                    if let Some(existing) = self.global_this_type {
                        return existing;
                    }
                    let minted = self.store.new_named(
                        TypeFlags::OBJECT,
                        "typeof globalThis".to_string(),
                        None,
                    );
                    self.global_this_type = Some(minted);
                    return minted;
                }
                _ => {}
            }
            current = self.nodes.parent(id);
        }
        self.intrinsics.error
    }

    /// The written annotation on a container's `this` parameter, if it has one.
    ///
    /// `ast.GetThisParameter` (`tryGetThisTypeAtEx`'s test, `checker.go:12146`)
    /// reads the container's **first** parameter and asks whether it is named
    /// `this`; the grammar allows it nowhere else.
    ///
    /// # An unannotated `this` parameter is deliberately not answered
    ///
    /// `function f(this) {}` would take its type from `getTypeOfSymbol`, which
    /// answers the implicit `any` — and `docs/architecture/checker-notes-rank.md`
    /// §6 records 21,685 gradient lines already banked on `any` with the computed
    /// and the defaulted not yet separated. Adding to that column for a parameter
    /// the source did not annotate is a claim, not a computation, so this returns
    /// `None` and the line stays a gap. Three such parameters exist in the corpus
    /// and `crates/tsr-conformance/examples/thisparam.rs` prints the count as a
    /// control on the exclusion.
    ///
    /// An **arrow function is not a `this` container** —
    /// `getThisContainer(node, includeArrowFunctions: false, …)`
    /// (`checker.go:12188`) — so it is absent from this match and stays
    /// transparent, which is the rule the caller's own comment already records.
    fn this_parameter_type(&mut self, container: NodeId) -> Option<TypeId> {
        // The map is a shared reference on the checker, so copying it out first
        // ends the borrow of `self` before `get_type_from_type_node` needs
        // `&mut self`.
        let map = self.node_map;
        let parameters = match map.get(container)? {
            Node::FunctionDeclaration(node) => node.parameters,
            Node::FunctionExpression(node) => node.parameters,
            Node::MethodDeclaration(node) => node.parameters,
            Node::GetAccessorDeclaration(node) => node.parameters,
            Node::SetAccessorDeclaration(node) => node.parameters,
            Node::ConstructorDeclaration(node) => node.parameters,
            _ => return None,
        };
        let first = parameters.first()?;
        if !matches!(first.name, Some(tsr_ast::BindingName::Identifier(name)) if name.text == "this")
        {
            return None;
        }
        let annotation = first.r#type?;
        // **An annotation this port cannot resolve falls through rather than
        // answering `errorType`.** `explicitThis(this: this, m: number)` in
        // `conformance/looseThisTypeInFunctions` is the case: the annotation is a
        // `ThisTypeNode`, which `getTypeFromTypeNode` has an arm for and this
        // port does not, and the class arm below answers `this` — **correctly**.
        // Returning the gap here instead turned 4 right lines into gaps and one
        // more in `compiler/unusedParametersThis`, measured, before this line
        // existed.
        //
        // Falling through is also upstream's own shape: `getThisTypeOfSignature`
        // answering `nil` is what sends `tryGetThisTypeAtEx` (`checker.go:12146`)
        // on to `ast.IsClassLike(container.Parent)`. "We could not read the
        // annotation" is this port's `nil`.
        let resolved = self.get_type_from_type_node(annotation);
        (resolved != self.intrinsics.error).then_some(resolved)
    }

    /// Ported from `Checker.checkSuperExpression` (`checker.go:7854`), reduced
    /// to the type it answers.
    ///
    /// # `super(...)` is the **static** side, and that is not about `static`
    ///
    /// Upstream's tail is `if ast.IsStatic(container) || isCallExpression`
    /// (`checker.go:7946`), where `isCallExpression` is *"this `super` is the
    /// callee of its own call"* (`:7855`). **A `super(...)` call answers the base
    /// **constructor** type — `typeof Base` — even inside an ordinary instance
    /// constructor**, because what it calls is the base constructor.
    ///
    /// This is measured, not reasoned. A first version of this arm split on
    /// `ast.IsStatic` alone, shipped nothing, and manufactured **257 wrong lines
    /// against 198 right** — of which ~151 were exactly this: `Base` where
    /// upstream prints `typeof Base`, `A` for `typeof A`, `C` for `typeof C`.
    /// The hypothesis at the time was that the container walk picked the wrong
    /// node; it did not. **The walk was right and the rule was wrong**, and only
    /// reading upstream's tail said so — the data structure had nothing to
    /// confess. See `docs/architecture/checker-notes-this.md` and `bd tsr-h1s`.
    ///
    /// # What is deliberately not answered
    ///
    /// - **An object-literal container.** Upstream assumes `any` there
    ///   (`checker.go:7917`), and `checker-notes-rank.md` §6 forbids banking on
    ///   `any`.
    /// - **A base this port cannot resolve.** `base_symbols_of`
    ///   (`crate::members`) resolves the heritage name in `SymbolFlags::TYPE`
    ///   meaning and answers `None` for `class C extends someExpression()` and
    ///   for a generic instantiation. Upstream prints `any` for many of those and
    ///   this gaps instead.
    /// - **`extends null`**, whose answer is the null-widening type
    ///   (`checker.go:7930`).
    pub(crate) fn check_super_expression(&mut self, node: NodeId) -> TypeId {
        let error = self.intrinsics.error;
        // `isCallExpression` (`checker.go:7855`): this `super` is its own call's
        // callee. Read before the walk, because it overrides the container's
        // static-ness rather than depending on it.
        let is_call = self.nodes.parent(node).is_some_and(|parent| {
            matches!(self.node_map.get(parent), Some(Node::CallExpression(call))
                if call.expression.and_then(|callee| callee.node_id()) == Some(node))
        });
        // `getSuperContainer(node, stopOnFunctions: true)` (`checker.go:7856`):
        // the nearest **member**, not the nearest class. An arrow is transparent,
        // so it is absent from this match for the same reason it is absent from
        // [`Checker::check_this_expression`]'s.
        let mut current = self.nodes.parent(node);
        let mut is_static = None;
        let mut class = None;
        // §481: a COMPUTED PROPERTY NAME is not inside the member it names —
        // upstream's `getSuperContainer` jumps from the name to the member
        // and keeps walking, so the member is SKIPPED and the search
        // continues outside it. `{ [super.bar()]() {} }` inside a class
        // method therefore finds THAT method and is legal
        // (`computedPropertyNames25_ES6` records `>super : Base`), while a
        // CLASS member's computed name walks out of the class entirely and
        // errors — "super cannot be referenced in a computed property name"
        // (`checker.go:7893`), `errorType` printed `any`
        // (`computedPropertyNames27_ES6`). The first §481 draft returned
        // `any` at the name itself and measured 7 G→W in the object-literal
        // fixtures; the skip is the rule, not the position.
        let mut crossed_computed_name = false;
        let mut skip_named_member = false;
        while let Some(id) = current {
            match self.nodes.kind(id) {
                // A plain function is where `getSuperContainer(node,
                // stopOnFunctions: true)` stops, so an outer class is not
                // reached.
                SyntaxKind::FunctionDeclaration | SyntaxKind::FunctionExpression => return error,
                // An object-literal CONTAINER is upstream's `any`
                // (`checker.go:7917`), which `checker-notes-rank.md` §6
                // forbids banking on — but only when a member was actually
                // found; a literal passed while skipping a computed-named
                // member is just an expression on the way.
                SyntaxKind::ObjectLiteralExpression => {
                    if is_static.is_some() {
                        return error;
                    }
                }
                // §481: upstream skips arrows ONLY for a non-call `super`
                // (`checker.go:7860`, the `if !isCallExpression` loop), so a
                // `super()` whose container is an arrow fails
                // `IsConstructorDeclaration(container)` and errors — "Super
                // calls are not permitted outside constructors or in nested
                // functions inside constructors" — `errorType`, printed
                // `any` (`derivedClassConstructorWithoutSuperCall` records
                // `>super : any` for `() => super()`). A property access
                // keeps the transparent-arrow behaviour.
                SyntaxKind::ArrowFunction if is_call => return self.intrinsics.any,
                SyntaxKind::ComputedPropertyName => {
                    crossed_computed_name = true;
                    skip_named_member = true;
                }
                SyntaxKind::MethodDeclaration
                | SyntaxKind::Constructor
                | SyntaxKind::PropertyDeclaration
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor => {
                    if skip_named_member {
                        skip_named_member = false;
                    } else if is_static.is_none() {
                        is_static = Some(self.has_static_modifier(id));
                    }
                }
                // §481: a class node is never a super CONTAINER upstream
                // — `getSuperContainer` returns members and functions
                // only — so a search still looking for its member (a
                // computed-name skip in flight) passes THROUGH a nested
                // class: `class { [super.foo()]() {} }` inside an outer
                // method finds that method and answers the OUTER base
                // (`superPropertyAccessInComputedPropertiesOfNestedType_ES6`
                // records `>super : A`). With a member already found,
                // stopping here is the same answer upstream reaches by
                // returning the member.
                SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
                    if is_static.is_some() || !crossed_computed_name =>
                {
                    class = Some(id);
                    break;
                }
                _ => {}
            }
            current = self.nodes.parent(id);
        }
        let (Some(class), Some(is_static)) = (class, is_static) else {
            // The walk found no member. Through a computed name that is
            // upstream's specific error and the deliberate error-any; every
            // other exit keeps the honest gap.
            if crossed_computed_name {
                return self.intrinsics.any;
            }
            return error;
        };
        let Some(symbol) = self.binder.symbol_of(class) else { return error };
        // §202. The **static** side, which is what `super(...)` and a `super`
        // inside a static member want, differs from the instance side twice
        // over, and both differences are `getBaseConstructorTypeOfClass`
        // (`checker.go:17434`) reading `getEffectiveBaseTypeNode` — the
        // heritage of **this class node** — and typing its *expression*:
        //
        // 1. **Type arguments are irrelevant.** `B` is `typeof B` whatever
        //    follows it in angle brackets, so `class D extends B<any>`'s
        //    `super()` is `typeof B`. The instance side really does depend on
        //    them and this port cannot instantiate one, so it keeps refusing —
        //    the refusal is passed as a parameter rather than baked into the
        //    shared helper, so the second caller does not inherit the first's.
        // 2. **Only the CLASS's own heritage counts.** `base_symbols_of` walks
        //    every declaration of the symbol, which for a class merged with an
        //    interface includes the interface's `extends`. Upstream never looks
        //    there: `interface Foo extends Array<number> {}` beside
        //    `class Foo { constructor() { super() } }` is an error and `any`,
        //    not `ArrayConstructor`. That case was the single loss on §202's
        //    first measurement and is why this arm reads the node.
        if is_static || is_call {
            let clauses = match self.node_map.get(class) {
                Some(Node::ClassDeclaration(node)) => node.heritage_clauses,
                Some(Node::ClassExpression(node)) => node.heritage_clauses,
                _ => return error,
            };
            let mut extends = clauses
                .iter()
                .filter(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)
                .flat_map(|clause| clause.types.iter());
            // Exactly one `extends` entry, which is the grammar for a class.
            let (Some(entry), None) = (extends.next(), extends.next()) else { return error };
            let Some(base) = self.base_symbol_of_heritage_entry(entry, false) else {
                return error;
            };
            // Type arguments are irrelevant to the static side only when they
            // are *legal*. A wrong ARITY makes the whole base type an error
            // upstream — `getTypeFromClassOrInterfaceReference` reports and
            // answers `errorType` — so `class B extends A<number, string>` over
            // a non-generic `A` records `>super : any`, not `>typeof A`. Two
            // cases lost a line to that on §202's first measurement, which is
            // the difference between "the arguments do not change the answer"
            // and "the arguments cannot be there at all".
            //
            // **Too many only.** Upstream's window is
            // `[minTypeArgumentCount, len(typeParameters)]` (§136), so a
            // *short* list is legal whenever the missing positions are
            // defaulted. Testing `!=` was measured and is wrong: it refuses
            // those, costing four lines in `compiler/genericDefaultsJs`.
            //
            // Residue, named: a list shorter than the **minimum** is also an
            // upstream error and is not detected here, because this port does
            // not compute `minTypeArgumentCount`. It costs one line in
            // `superCallFromClassThatDerivesFromGenericTypeButWithIncorrectNumberOfTypeArguments1`,
            // measured, and no case.
            if entry.type_arguments.len() > self.type_parameter_count_of(base) {
                return error;
            }
            return self.get_type_of_symbol(base);
        }
        let Some(bases) = self.base_symbols_of(symbol) else { return error };
        // Exactly one `extends` entry, which is the grammar for a class. Zero is
        // a base-less class — upstream's own error — and more than one cannot
        // arise; both gap rather than guessing which base `super` means.
        let [base] = bases[..] else { return error };
        self.get_declared_type_of_symbol(base)
    }

    /// How many type parameters a class or interface symbol declares.
    ///
    /// The **maximum** over its declarations rather than the first's: a merged
    /// interface may repeat the list, and an ambient declaration may carry it
    /// where the value declaration does not. Taking the maximum keeps a
    /// legal-arity heritage entry legal; taking the first would make one
    /// spelling of a merge refuse arguments the other admits.
    fn type_parameter_count_of(&self, symbol: tsr_binder::SymbolId) -> usize {
        self.binder
            .symbols()
            .get(symbol)
            .declarations
            .iter()
            .filter_map(|&declaration| match self.node_map.get(declaration) {
                Some(Node::ClassDeclaration(node)) => Some(node.type_parameters.len()),
                Some(Node::ClassExpression(node)) => Some(node.type_parameters.len()),
                Some(Node::InterfaceDeclaration(node)) => Some(node.type_parameters.len()),
                _ => None,
            })
            .max()
            .unwrap_or(0)
    }

    /// Whether a bare identifier sits in a position upstream refuses to resolve
    /// it from: the initializer of a **non-static property** of a class whose
    /// **constructor declares a local of the same name**.
    ///
    /// `NameResolver.Resolve`'s `KindPropertyDeclaration` arm
    /// (`nameresolver.go:160-169`) remembers such a property, and
    /// `checkAndReportErrorForInvalidInitializer` (`checker.go:1514`) then makes
    /// `Resolve` answer **nil** — so the reference is `errorType`, printed
    /// `any`, even though an outer binding of that name exists and is what the
    /// programmer meant. Upstream's own comment says why: the initializer is
    /// emitted *inside the constructor*, where the name would capture the
    /// constructor's local instead.
    ///
    /// `compiler/classMemberInitializerWithLamdaScoping` carries both halves two
    /// lines apart — the instance initializer's `field1` is `any` and the
    /// STATIC initializer's `field1` is the outer `string`, because the arm
    /// tests `!ast.IsStatic(location)`.
    ///
    /// Gated on `standard_class_fields`, which is upstream's own first
    /// condition: with `useDefineForClassFields` (or `target >= ES2022`) the
    /// scope semantics differ and there is no refusal. §213.
    ///
    /// # It fires only on a name the walk was still LOOKING for
    ///
    /// Upstream sets the flag *while walking outward* and consults it only at
    /// the end — so a name that resolves **inside** the initializer never
    /// reaches the property arm at all. The first draft of this tested the
    /// syntactic position alone and refused
    /// `messageHandler = () => { var field = this.field; console.log(field); }`,
    /// whose `field` is the arrow's own local. That measured **+5 / −4** across
    /// four otherwise-passing cases, and upstream's own fixture comment says
    /// the opposite in so many words: *"Using field here shouldnt be error"*.
    ///
    /// So the resolved symbol is passed in, and a symbol declared within the
    /// property's own subtree is exempt.
    fn identifier_is_an_invalid_class_field_initializer_reference(
        &mut self,
        node: NodeId,
        name: &str,
        resolved: tsr_binder::SymbolId,
    ) -> bool {
        if self.standard_class_fields {
            return false;
        }
        let mut current = self.nodes.parent(node);
        while let Some(id) = current {
            match self.nodes.kind(id) {
                SyntaxKind::PropertyDeclaration => {
                    if self.has_static_modifier(id) {
                        return false;
                    }
                    // Declared inside this very initializer: the walk would
                    // have stopped before reaching the property.
                    if self
                        .binder
                        .symbols()
                        .get(resolved)
                        .declarations
                        .iter()
                        .any(|&declaration| self.node_is_within(declaration, id))
                    {
                        return false;
                    }
                    let Some(class) = self.nodes.parent(id) else { return false };
                    let Some(constructor) = self.class_constructor_declaration(class) else {
                        return false;
                    };
                    return self.binder.lookup_local(constructor, name).is_some();
                }
                SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression => return false,
                _ => {}
            }
            current = self.nodes.parent(id);
        }
        false
    }

    /// Whether `node` is `ancestor` or sits beneath it.
    fn node_is_within(&self, node: NodeId, ancestor: NodeId) -> bool {
        let mut current = Some(node);
        while let Some(id) = current {
            if id == ancestor {
                return true;
            }
            current = self.nodes.parent(id);
        }
        false
    }

    /// The `constructor` member of a class node, if it has one —
    /// `ast.FindConstructorDeclaration` (`nameresolver.go:162`), which requires
    /// a BODY: an overload signature declares no locals.
    fn class_constructor_declaration(&self, class: NodeId) -> Option<NodeId> {
        let members = match self.node_map.get(class)? {
            Node::ClassDeclaration(node) => node.members,
            Node::ClassExpression(node) => node.members,
            _ => return None,
        };
        members.iter().find_map(|member| match member {
            tsr_ast::ClassElement::ConstructorDeclaration(constructor) => {
                constructor.body.and(constructor.node_id)
            }
            _ => None,
        })
    }

    /// Whether a class member carries `static`. `ast.IsStatic` (`checker.go:7946`).
    fn has_static_modifier(&self, member: NodeId) -> bool {
        let modifiers = match self.node_map.get(member) {
            Some(Node::MethodDeclaration(node)) => node.modifiers,
            Some(Node::PropertyDeclaration(node)) => node.modifiers,
            Some(Node::GetAccessorDeclaration(node)) => node.modifiers,
            Some(Node::SetAccessorDeclaration(node)) => node.modifiers,
            // A constructor cannot be static.
            _ => return false,
        };
        modifiers.iter().any(|modifier| {
            matches!(modifier, tsr_ast::ModifierLike::Token(token)
                if token.kind == SyntaxKind::StaticKeyword)
        })
    }

    /// Ported from `Checker.checkConditionalExpression` (`checker.go:10934`).
    ///
    /// # The union is built at the wrong reduction, so the safe cases are fenced
    ///
    /// Upstream builds the branch union at **`UnionReductionSubtype`**
    /// (`checker.go:10940`), and this port only has `UnionReductionLiteral`
    /// ([`Checker::get_union_type`]) because subtype reduction needs an
    /// assignability question that does not exist here. The two reductions agree
    /// on primitives and unit types — `bob ? 1 : 2` is `1 | 2` under both, and
    /// `x ? 1 : n` is `number` under both, because literal reduction already
    /// removes a literal whose base primitive is present.
    ///
    /// They part company on object types, where subtype reduction collapses a
    /// constituent into a supertype it is assignable to. The baselines record
    /// exactly that: `>true ? a : b : { Foo?: Base; }` is one object type, not the
    /// two-member union this port would build. So an object-typed branch is a
    /// **gap**, fenced by [`Self::is_subtype_reduction_free`], rather than a
    /// plausible `A | B` that is wrong on every such line.
    ///
    /// The rejected alternative was to emit the union everywhere and accept the
    /// object case as a known divergence. It wins the moment `relater.rs` can
    /// answer assignability for object types, at which point the fence comes out
    /// and real subtype reduction goes in — not before, because a union that
    /// should have collapsed is a wrong line rather than a missing one.
    ///
    /// **How we would know this is wrong:** a baseline `>c ? a : b` line whose
    /// printed type is a union of two primitives that this port gaps, or a
    /// non-union answer where it prints a union.
    fn check_conditional_expression(
        &mut self,
        node: &tsr_ast::ConditionalExpression<'_>,
    ) -> TypeId {
        let error = self.intrinsics.error;
        // Checked for its own line and its narrowing effects; the condition's
        // type does not reach the answer.
        if let Some(condition) = node.condition {
            self.check_expression(condition);
        }
        let (Some(when_true), Some(when_false)) = (node.when_true, node.when_false) else {
            return error;
        };
        let branches = [self.check_expression(when_true), self.check_expression(when_false)];
        // A gap in a branch is a gap in the conditional: `c ? 1 : unported` is
        // not `1`, and printing the known branch alone would be a wrong line.
        if branches.contains(&error) {
            return error;
        }
        // Two shapes need no reduction judgement (`checker-notes-assign.md`
        // §7): identical branches — the union of `[t, t]` is `t` under every
        // reduction, and references intern by `(symbol, args)` — and an
        // `any`/`unknown` branch, which absorbs the union under both
        // reductions. The freshness hop matters for the identity test:
        // `c ? 1 : 1` is the fresh literal twice, and `c ? x : 1` with
        // `x: 1` is the regular and the fresh spelling of one type, which
        // upstream's union regularises to one constituent.
        let regular = [
            self.get_regular_type_of_literal_type(branches[0]),
            self.get_regular_type_of_literal_type(branches[1]),
        ];
        let any = self.intrinsics.any;
        // §133: a TYPE PARAMETER is never subtype-collapsed into a sibling
        // in these baselines — `T | null`, `number | T`, `T | RegExp`,
        // `T | { foo: number; }` all keep both constituents, INCLUDING a
        // constrained `T extends number` beside bare `number`
        // (subtypesOfTypeParameterWithConstraints2's wants). A pair with
        // exactly ONE parameter side is reduction-free whole; TWO parameters
        // keep the fence (the constraint-related pair is undecided).
        // Iteration 2's split (subtypesOfTypeParameterWithConstraints2's
        // wants): a CONSTRAINED parameter collapses into an OBJECT-ish
        // sibling its constraint chain relates to (`T extends U extends
        // Date` beside `new Date()` wants `Date`) but NEVER into a
        // primitive/literal sibling (`T extends Number` beside `1` wants
        // `number | T`); an UNCONSTRAINED parameter never collapses at all
        // (`T | RegExp`, `T | { foo: number; }`). So: admit when the
        // sibling is primitive-safe, or the parameter is unconstrained;
        // a constrained parameter beside an object sibling stays fenced.
        let parameter_union_safe = {
            let sides: Vec<bool> = branches
                .iter()
                .map(|&branch| self.store.get(branch).flags.contains(TypeFlags::TYPE_PARAMETER))
                .collect();
            match (sides[0], sides[1]) {
                (true, true) | (false, false) => false,
                (parameter_first, _) => {
                    let (parameter, sibling) = if parameter_first {
                        (branches[0], branches[1])
                    } else {
                        (branches[1], branches[0])
                    };
                    self.is_subtype_reduction_free(sibling)
                        || self.type_parameter_constraint(parameter).is_none()
                }
            }
        };
        if regular[0] == regular[1]
            || branches.contains(&any)
            || parameter_union_safe
            || branches.iter().all(|&branch| self.is_subtype_reduction_free(branch))
        {
            return self.get_union_type(&branches);
        }
        // The non-agnostic pairs run the decidability-gated `removeSubtypes`
        // (`checker-notes-assign.md` §9); an undecidable pair stays a gap.
        self.union_with_subtype_reduction(&branches).unwrap_or(error)
    }

    /// Whether `UnionReductionLiteral` and `UnionReductionSubtype` must agree for
    /// this type as a union constituent.
    ///
    /// True for the primitives and the unit types: subtype relationships among
    /// them are exactly the literal-to-base-primitive ones that literal reduction
    /// already handles. False for everything else — objects, type parameters,
    /// intersections, and the enum types, whose reduction is [`crate::unions`]'s
    /// question rather than this one's.
    ///
    /// A union is transparent: it is reduction-free when all of its constituents
    /// are, because `addTypesToUnion` flattens it before either reduction runs.
    pub(crate) fn is_subtype_reduction_free(&self, id: TypeId) -> bool {
        const SAFE: TypeFlags = TypeFlags::STRING
            .union(TypeFlags::NUMBER)
            .union(TypeFlags::BIG_INT)
            .union(TypeFlags::BOOLEAN)
            .union(TypeFlags::STRING_LITERAL)
            .union(TypeFlags::NUMBER_LITERAL)
            .union(TypeFlags::BIG_INT_LITERAL)
            .union(TypeFlags::BOOLEAN_LITERAL)
            .union(TypeFlags::NULL)
            .union(TypeFlags::UNDEFINED)
            .union(TypeFlags::VOID)
            .union(TypeFlags::NEVER);
        let t = self.store.get(id);
        if let TypeData::Union { types, symbol, .. } = &t.data {
            // A *named* union prints as its symbol, and `union_type_worker`
            // already refuses to nest one; keeping it out here makes the reason
            // local rather than relying on that.
            return symbol.is_none()
                && types.iter().all(|&constituent| self.is_subtype_reduction_free(constituent));
        }
        // An enum literal carries STRING_LITERAL or NUMBER_LITERAL as well, so
        // the flag test alone would let it through.
        !t.flags.intersects(TypeFlags::ENUM_LIKE) && SAFE.contains(t.flags)
    }

    /// Does `new` on this target reach upstream's `anyType` recovery?
    ///
    /// `checker.go:8334-8342` returns `anyType` when the resolved signature's
    /// declaration is not a constructor, and `resolveNewExpression` reaches a
    /// call signature only after finding no construct signature
    /// (`checker.go:8603`, then `:8632`) — so the condition is expressible on
    /// the type: **call signatures, no construct signature.**
    ///
    /// A CLASS is excluded: its static side is handled by the class road,
    /// which resolves the declared instance type.
    ///
    /// Shared with [`Checker::any_is_written_in_an_annotation`] (§862), because
    /// the `any` this produces is upstream's own value and not an unported
    /// gap — so calling the result of such a `new` is `any` too.
    pub(crate) fn new_target_lacks_a_construct_signature(&mut self, callee_type: TypeId) -> bool {
        if matches!(self.store.get(callee_type).data,
            TypeData::Anonymous { symbol, .. }
                if self.binder.symbols().get(self.binder.merged_symbol(symbol))
                    .flags
                    .intersects(SymbolFlags::CLASS))
        {
            return false;
        }
        self.signature_candidates_of_named_type(
            callee_type,
            crate::signatures::SignatureKind::Construct,
        )
        .is_none_or(|candidates| candidates.is_empty())
            && self.call_signatures_of_type(callee_type).is_some_and(|c| !c.is_empty())
    }

    /// The type of `new C()`.
    ///
    /// Ported from `Checker.resolveNewExpression` (`checker.go:8575`), reduced to
    /// the single shape whose answer does not need a construct signature.
    ///
    /// # Why this does not go through signatures at all
    ///
    /// Upstream takes the callee's apparent type, pulls its
    /// `SignatureKindConstruct` signatures and runs `resolveCall` over them; the
    /// result is that signature's return type. [`crate::signatures::Signature`]
    /// **has no construct flag** — `signatures.rs` says so where it refuses to
    /// fold `ConstructorTypeNode` into the function-type arm — so that route is
    /// closed.
    ///
    /// It is closed but not needed for the common case, because of a fact about
    /// classes rather than about signatures: a class's implicit construct
    /// signature returns the class's *instance* type, and a constructor cannot
    /// carry a return type annotation to make it return anything else. So for a
    /// class callee the answer is `getDeclaredTypeOfSymbol` on the class symbol,
    /// which this port already computes, and the signature is not on the path to
    /// it. 160 baseline lines record `>new C() : C`.
    ///
    /// This is a **reduction, not a shortcut**: it answers exactly the cases
    /// where the signature would have been redundant, and gaps every case where
    /// the signature actually carries information.
    ///
    /// # What gaps, and why each one has to
    ///
    /// - **A generic class.** `new C<T>()` needs `inferTypeArguments`, and the
    ///   uninstantiated instance type would print `C<T>` where upstream prints
    ///   the inferred `C<number>`.
    /// - **Explicit type arguments**, for the same reason, and matching
    ///   [`Self::check_call_expression`]'s rule.
    /// - **An abstract class.** Upstream reports and answers `errorType`
    ///   (`checker.go:8620`), so this is upstream's own answer rather than a
    ///   local gap.
    /// - **Any non-class callee.** `new Date()` prints `Date` upstream, but it
    ///   goes through a `DateConstructor` *interface* with a real construct
    ///   signature member; there is nothing about it this port can shortcut, and
    ///   48 baseline lines of `>new StringHashTable() : any` are a reminder that
    ///   the non-class cases do not all answer the obvious thing.
    fn check_new_expression(&mut self, node: &tsr_ast::NewExpression<'_>) -> TypeId {
        use crate::calls::counters::{COUNTERS, bump};

        let error = self.intrinsics.error;
        // The `new` funnel (`bd tsr-klm`). Counting only; no answer below
        // depends on it. This path had no counters at all, and `new` is the
        // half of the call row whose callee is *most often* typed — so the
        // uninstrumented half was also the most admitted one.
        bump(&COUNTERS.new_expressions);
        let Some(callee) = node.expression else { return error };
        let callee_type = self.check_expression(callee);
        // §25 (`checker-notes-callres.md`): construction through a
        // §31-provenance callee — upstream's TS2304 `errorType` — answers
        // `any`, the chain's sixth hop, behind the same gates.
        if callee_type == self.intrinsics.any
            && let tsr_ast::Expression::Identifier(identifier) = callee
            && let Some(id) = identifier.node_id
            && self
                .binder
                .resolve_name(
                    self.nodes,
                    self.node_map,
                    id,
                    identifier.text,
                    SymbolFlags::VALUE
                        | SymbolFlags::TYPE
                        | SymbolFlags::NAMESPACE
                        | SymbolFlags::ALIAS,
                )
                .is_none()
            && !self.file_has_import_machinery(id)
        {
            return self.intrinsics.any;
        }
        // §30 (`checker-notes-callres.md`): `new` through an `any` callee is
        // the SAME untyped call the call arm answers (`checker.go:8490`,
        // `resolveUntypedCall`) — the gate's calibrated narrowings inherited
        // whole.
        if self.is_untyped_call_target(callee, callee_type) {
            return self.intrinsics.any;
        }
        // §861: `new` on a target that has CALL signatures and no CONSTRUCT
        // signature is `any` — upstream's own deliberate recovery
        // (`checker.go:8334-8342`), comment included:
        //
        // ```go
        // // When resolved signature is a call signature (and not a construct signature) the result type is any
        // if c.noImplicitAny {
        //     c.error(node, diagnostics.X_new_expression_whose_target_lacks_a_construct_signature_implicitly_has_an_any_type)
        // }
        // return c.anyType
        // ```
        //
        // `resolveNewExpression` tries construct signatures first and falls
        // back to call signatures (`checker.go:8603`, then `:8632`), so that
        // arm is reached exactly when the target has call signatures and no
        // construct signature — the condition tested here, on the type,
        // without resolving a signature first.
        //
        // **Not ADR-0038's forbidden rendering**: it is `anyType` and not
        // `errorType`, it carries upstream's own comment saying the result IS
        // any, and its diagnostic is `noImplicitAny`-gated — the same standing
        // as `anySignature` that `checker-notes-calleegap.md` argues from.
        //
        // A CLASS is excluded: its static side is handled by the class road
        // below, which resolves the declared instance type, and a class
        // reaching here would short-circuit to `any`.
        if self.new_target_lacks_a_construct_signature(callee_type) {
            return self.intrinsics.any;
        }
        // The callee's type is the class's *static* side, which
        // `getTypeOfFuncClassEnumModule` gives as an anonymous type carrying the
        // class symbol. Reaching the symbol through the type rather than through
        // the callee's syntax is what makes `new (C)()` and an aliased class
        // work the same way.
        let TypeData::Anonymous { symbol, .. } = self.store.get(callee_type).data else {
            // A constructor **interface** — `DateConstructor`, `ErrorConstructor`
            // — whose construct signatures live in its members rather than in
            // its symbol's declarations (`bd tsr-4sa`,
            // `docs/architecture/checker-notes-namedcallee.md`). Written type
            // arguments are not handled here: an interface's construct
            // signature that takes them is generic, and
            // `get_signature_of_named_type` declines a generic candidate.
            if node.type_arguments.is_empty() {
                if let Some(signature) = self.get_signature_of_named_type(
                    callee_type,
                    crate::signatures::SignatureKind::Construct,
                ) {
                    bump(&COUNTERS.new_resolved);
                    return signature.r#type;
                }
                // §273 at the `new` road: upstream's subtype pass over the
                // clean candidate PREFIX, before the §74 agree-loop below.
                // `new Array(3)` is the witness — ArrayConstructor's FIRST
                // construct signature is the non-generic
                // `new (arrayLength?: number): any[]`, `3` is a subtype of
                // `number`, and upstream never consults the generic
                // overloads behind it. A pick here is sound whether or not
                // a tail exists (pass one runs in candidate order); no pick
                // falls through to the agree-loop unchanged.
                if let Some(candidates) = self.signature_candidates_of_named_type(
                    callee_type,
                    crate::signatures::SignatureKind::Construct,
                ) && !candidates.is_empty()
                {
                    let clean = Self::clean_candidate_prefix_len(&candidates);
                    if clean > 0
                        && let Some(signature) =
                            self.subtype_pass_prefix_pick(&candidates, clean, node.arguments)
                    {
                        bump(&COUNTERS.new_resolved);
                        return signature.r#type;
                    }
                }
                // §74 (`checker-notes-narrow.md`): GENERIC construct
                // candidates infer from the arguments — `new Set([1, 2, 3])`
                // is `Set<number>`, not §44's default-map `Set<any>`. Each
                // candidate answers through the SAME `check_generic_call`
                // the call road uses (written arguments excluded above), and
                // the answers must AGREE — overload selection by
                // assignability is not ported, but candidates that converge
                // need none.
                if !node.arguments.is_empty()
                    && let Some(candidates) = self.signature_candidates_of_named_type(
                        callee_type,
                        crate::signatures::SignatureKind::Construct,
                    )
                    && !candidates.is_empty()
                {
                    let mut agreed: Option<TypeId> = None;
                    let mut ok = true;
                    for candidate in &candidates {
                        // A candidate whose type-parameter CONSTRAINT is
                        // itself a type parameter (`new <U extends T>` on
                        // `I<T>`) reaches here UNINSTANTIATED — inference
                        // against it widens the fresh literal upstream
                        // keeps. Decline the arm; the reading road stays.
                        if candidate.type_parameters.iter().any(|tp| {
                            tp.constraint
                                .is_some_and(|c| self.type_parameter_symbols.contains_key(&c))
                        }) {
                            ok = false;
                            break;
                        }
                        let answer = if candidate.type_parameters.is_empty() {
                            candidate.r#type
                        } else {
                            self.check_generic_call(candidate, None, node.arguments)
                        };
                        if answer == error {
                            ok = false;
                            break;
                        }
                        match agreed {
                            None => agreed = Some(answer),
                            Some(t) if t == answer => {}
                            Some(_) => {
                                ok = false;
                                break;
                            }
                        }
                    }
                    // §788: OVERLOADS ARE ALTERNATIVES, NOT AGREEMENTS.
                    //
                    // The walk above answers only when every generic candidate
                    // produces the SAME return, which is right for a set that
                    // is really one signature seen several ways
                    // (`DateConstructor`'s four construct signatures all return
                    // `Date`) and wrong for a genuine overload set. `SetConstructor`
                    // is `new <T = any>(values?: readonly T[] | null): Set<T>`
                    // AND `new <T = any>(iterable?: Iterable<T> | null): Set<T>`;
                    // `new Set([0, 1, 2])` fits the first and not the second, so
                    // the second answers `errorType`, `ok` goes false, and the
                    // whole call declined.
                    //
                    // Upstream's `chooseOverload` (`checker.go`) takes the FIRST
                    // candidate the arguments fit and never consults the rest.
                    // This is that rule, restricted to the shape this port can
                    // decide: a candidate is "fits" when its generic resolution
                    // does not answer `errorType` AND its arity accepts the
                    // argument count. Anything subtler — assignability-ranked
                    // selection among several fitting candidates — stays the
                    // already-recorded 926-line refusal.
                    //
                    // Purely additive: it runs only where the agreement walk
                    // ALREADY declined, so its failure direction is gap→wrong
                    // and it can take no right line away.
                    if !ok || agreed.is_none() {
                        for candidate in &candidates {
                            if candidate.type_parameters.iter().any(|tp| {
                                tp.constraint
                                    .is_some_and(|c| self.type_parameter_symbols.contains_key(&c))
                            }) {
                                continue;
                            }
                            if !Self::arity_accepts(candidate, node.arguments.len()) {
                                continue;
                            }
                            let answer = if candidate.type_parameters.is_empty() {
                                candidate.r#type
                            } else {
                                self.check_generic_call(candidate, None, node.arguments)
                            };
                            if answer != error {
                                bump(&COUNTERS.new_resolved);
                                return answer;
                            }
                        }
                    }
                    if ok && let Some(answer) = agreed {
                        bump(&COUNTERS.new_resolved);
                        return answer;
                    }
                }
            }
            // SS161 (checker-notes-callres2.md, the SS160.1
            // re-attribution): WRITTEN type arguments on a constructor
            // interface's generic construct signatures -
            // `new Set<number>()` on SetConstructor. Each arity-matching
            // generic candidate instantiates its return with the written
            // arguments; candidates must AGREE (the SS74 rule); defaults
            // and partial lists decline.
            if !node.type_arguments.is_empty()
                && let Some(candidates) = self.signature_candidates_of_named_type(
                    callee_type,
                    crate::signatures::SignatureKind::Construct,
                )
                && !candidates.is_empty()
            {
                // Re-fetch through node_map for the checker-lifetime view
                // of the written argument nodes.
                let Some(written_nodes) = node.node_id.and_then(|id| match self.node_map.get(id) {
                    Some(Node::NewExpression(fetched)) => Some(fetched.type_arguments),
                    _ => None,
                }) else {
                    bump(&COUNTERS.new_callee_not_anonymous);
                    return error;
                };
                let written: Vec<TypeId> = written_nodes
                    .iter()
                    .map(|&argument| self.get_type_from_type_node(argument))
                    .collect();
                if !written.contains(&error) {
                    let mut agreed: Option<TypeId> = None;
                    let mut ok = false;
                    for candidate in &candidates {
                        if candidate.type_parameters.len() != written.len() {
                            continue;
                        }
                        let Some(parameters) = self.type_parameter_types(candidate) else {
                            continue;
                        };
                        let names: Vec<&str> =
                            candidate.type_parameters.iter().map(|p| p.name.as_str()).collect();
                        let map: Vec<(TypeId, TypeId)> =
                            parameters.iter().copied().zip(written.iter().copied()).collect();
                        let answer =
                            self.instantiate_type(candidate.r#type, &map, &parameters, &names);
                        if answer == error {
                            ok = false;
                            break;
                        }
                        match agreed {
                            None => {
                                agreed = Some(answer);
                                ok = true;
                            }
                            Some(t) if t == answer => {}
                            Some(_) => {
                                ok = false;
                                break;
                            }
                        }
                    }
                    if ok && let Some(answer) = agreed {
                        bump(&COUNTERS.new_resolved);
                        return answer;
                    }
                }
            }
            bump(&COUNTERS.new_callee_not_anonymous);
            return error;
        };
        // §805: a CONSTRUCTOR TYPE NODE callee — `declare var C: new (tag: string) => L`
        // — carries its construct signature on the TYPE, in `signature_types`
        // (`function_types.rs` puts it there), not on a class symbol. The
        // `CLASS` gate below declined it whole, so every `new C(…)` through an
        // annotated constructor variable answered `errorType`.
        //
        // `conformance/localesObjectArgument` is 75 GAP lines of exactly this:
        // `new Intl.Locale("en-US")`, where `Intl.Locale` is a VARIABLE typed
        // `new (tag: …) => Intl.Locale` rather than a class or a constructor
        // interface. The two roads either side of it were already ported — a
        // class (below) and a constructor INTERFACE (`DateConstructor`, above)
        // — and this third spelling had no arm.
        //
        // Non-generic, single-signature only: an overload set here needs the
        // selection §788 built for the interface road, and a generic one needs
        // inference. Both keep the gap.
        if let Some(signatures) = self.signature_types.get(&callee_type).cloned()
            && let [signature] = signatures.as_slice()
            && signature.kind != crate::signatures::SignatureKind::Call
            && signature.type_parameters.is_empty()
            && signature.r#type != error
        {
            bump(&COUNTERS.new_resolved);
            return signature.r#type;
        }
        if !self.binder.symbols().get(symbol).flags.contains(SymbolFlags::CLASS) {
            // §298: `new f()` on a PLAIN FUNCTION — upstream reports TS7009
            // ("'new' expression, whose target lacks a construct signature,
            // implicitly has an 'any' type") and answers ANY: the deliberate
            // error-any, not a gap wearing one (`avoid.ts` records
            // `new f() : any` beside the error). Scoped to FUNCTION symbols
            // whose type genuinely lacks construct signatures — everything
            // else keeps the honest gap.
            if self.binder.symbols().get(symbol).flags.contains(SymbolFlags::FUNCTION)
                && self
                    .binder
                    .symbols()
                    .get(symbol)
                    .declarations
                    .first()
                    .is_some_and(|&declaration| !self.in_js_file(declaration))
            {
                bump(&COUNTERS.new_callee_not_class);
                return self.intrinsics.any;
            }
            bump(&COUNTERS.new_callee_not_class);
            return error;
        }
        // §922: the CLASS declaration, not the first one.
        //
        // A class MERGED with a namespace — `namespace D { … }` beside
        // `declare class D extends C {}` — carries both declarations on one
        // symbol, and the namespace comes first in source order. Taking
        // `declarations.first()` then matched a `ModuleDeclaration`, fell to the
        // `_` arm and answered `error`, so `new D()` gapped while every
        // unmerged form worked.
        //
        // Probed, not reasoned: `new C()`, `new D()` on an ambient class, and
        // `new D()` on an ambient class with `extends` all answer correctly, and
        // only the class+namespace merge fails. `getDeclaredTypeOfSymbol` reads
        // the class declaration wherever it sits, so this searches for it.
        let class_like =
            self.binder.symbols().get(symbol).declarations.iter().copied().find(|&declaration| {
                matches!(
                    self.node_map.get(declaration),
                    Some(Node::ClassDeclaration(_) | Node::ClassExpression(_))
                )
            });
        let Some(declaration) = class_like else {
            bump(&COUNTERS.new_no_declaration);
            return error;
        };
        let (type_parameters, modifiers) = match self.node_map.get(declaration) {
            Some(Node::ClassDeclaration(class)) => (class.type_parameters, class.modifiers),
            Some(Node::ClassExpression(class)) => (class.type_parameters, class.modifiers),
            _ => {
                bump(&COUNTERS.new_declaration_not_class_like);
                return error;
            }
        };
        if !type_parameters.is_empty() {
            // `new C<string>()` — the caller wrote the type arguments, so the
            // instance type is `createTypeReference(C, [string])` and there is
            // nothing to infer. This is the *same* rule the call side already
            // applies (`crate::inference`: "the caller wrote the type
            // arguments, so there is nothing to infer and substitution is all
            // that is left"); the two halves of one construct answered
            // differently until `bd tsr-tgov`, and `checker-notes-callres.md`
            // §13.4 has the 826 lines that cost.
            //
            // Reached through `create_type_reference`, which is what
            // `crate::declared` calls for `C<string>` in *type* position, so
            // the instance type is identical **by interning** to the
            // annotation's — `let c: C<string> = new C<string>()` is one type,
            // and `tsr-4qx`'s instantiated members hang off it unchanged.
            // Re-read the written arguments from `self.node_map`, which carries
            // the checker's `'a`; the `node` parameter's lifetime is
            // independent of it and `get_type_from_type_node` needs `'a`. Same
            // move as [`Self::this_parameter_type`], and its comment explains
            // why the map is copied out first.
            let map = self.node_map;
            let written = match node.node_id.and_then(|id| map.get(id)) {
                Some(Node::NewExpression(from_map)) => from_map.type_arguments,
                _ => &[],
            };
            // SS191 (measured -17, reverted): checker-1's SS202 lesson 2 says
            // upstream's arity window is `[minTypeArgumentCount, len]`, so a
            // SHORT written list is legal when the tail is defaulted, and
            // `==` refuses those. Widening the gate ALONE cost 17 cases:
            // admitting `C<A>` for `class C<A, B = X>` and then building a
            // ONE-argument reference prints `C<A>` where upstream prints
            // `C<A, X>`. The gate and the DEFAULT FILL are one change, not
            // two — `fillMissingTypeArguments` must run here before the
            // reference is created. Recorded so the next attempt starts with
            // the fill.
            if written.len() == type_parameters.len() {
                let mut arguments = Vec::with_capacity(written.len());
                for argument in written {
                    let id = self.get_type_from_type_node(*argument);
                    // A gap in an argument gaps the whole `new`: `C<Unported>`
                    // is not `C<any>`, the rule the tuple and array arms use.
                    if id == error {
                        bump(&COUNTERS.new_type_parameters);
                        return error;
                    }
                    arguments.push(id);
                }
                bump(&COUNTERS.new_instantiated);
                return self.create_type_reference(symbol, arguments);
            }
            // §43 (`checker-notes-narrow.md`): a SHORTER list fills its tail
            // from the class's declared defaults — `fillMissingTypeArguments`
            // at the `new` road. A tail position without a default, or with a
            // default that mentions another parameter, stays the arity error.
            if written.len() < type_parameters.len() {
                let mut arguments = Vec::with_capacity(type_parameters.len());
                for argument in written {
                    let id = self.get_type_from_type_node(*argument);
                    if id == error {
                        bump(&COUNTERS.new_type_parameters);
                        return error;
                    }
                    arguments.push(id);
                }
                let names: Vec<&str> = type_parameters
                    .iter()
                    .map(|parameter| parameter.name.map_or("", |n| n.text))
                    .collect();
                let mut filled = true;
                for parameter in &type_parameters[written.len()..] {
                    let Some(default) = parameter.default_type else {
                        filled = false;
                        break;
                    };
                    let image = self.get_type_from_type_node(default);
                    if image == error {
                        filled = false;
                        break;
                    }
                    // The conservative first cut: a default whose resolution
                    // prints another parameter's name is unresolved-by-name
                    // here and gaps.
                    let printed = self.type_to_string(image);
                    if names.contains(&printed.as_str()) {
                        filled = false;
                        break;
                    }
                    arguments.push(image);
                }
                if filled {
                    bump(&COUNTERS.new_instantiated);
                    return self.create_type_reference(symbol, arguments);
                }
            }
            // §162 (`checker-notes-narrow.md`): with NO written type
            // arguments the class's constructor INFERS them, which is §74's
            // rule (constructor interfaces) on the class side. The
            // transcription that makes it the call road's problem:
            // `getReturnTypeFromAnnotation` (`checker.go:20058-20061`) —
            // *a ConstructorDeclaration's return type IS
            // `getDeclaredTypeOfClassOrInterface` of its parent's merged
            // symbol* — so the signature is (class type parameters, ctor
            // parameters, declared instance type) and `check_generic_call`
            // does the rest. An OVERLOADED constructor declines whole
            // (§74's agreement rule); a class with no constructor of its
            // own declines rather than answer the uninstantiated `C<T>`
            // (the §343 two-endings rule: the implicit zero-argument
            // constructor can infer nothing).
            // §389: NO written arguments and NO call arguments is inference
            // with zero sources — every parameter takes upstream's failed-
            // inference fallback, `unknownType` (`new M` on `class M<T>` is
            // `M<unknown>`, `recursiveBaseCheck4/5/6`). Gated away from
            // defaults, which fill instead and belong to the arm above.
            if written.is_empty()
                && node.arguments.is_empty()
                && type_parameters.iter().all(|parameter| parameter.default_type.is_none())
            {
                // A JS CALL SITE's failed inference is `any`, not `unknown`
                // (`genericDefaultsJs` wants `f0_v0 : any` for a `.d.ts`
                // class newed from `main.js` — the site's file decides, not
                // the declaration's).
                let fallback = if node.node_id.is_some_and(|id| self.in_js_file(id)) {
                    self.intrinsics.any
                } else {
                    self.intrinsics.unknown
                };
                let arguments = vec![fallback; type_parameters.len()];
                bump(&COUNTERS.new_instantiated);
                return self.create_type_reference(symbol, arguments);
            }
            if written.is_empty() && !node.arguments.is_empty() {
                let constructors: Vec<NodeId> = match self.node_map.get(declaration) {
                    Some(Node::ClassDeclaration(class)) => class
                        .members
                        .iter()
                        .filter_map(|member| match member {
                            tsr_ast::ClassElement::ConstructorDeclaration(ctor) => ctor.node_id,
                            _ => None,
                        })
                        .collect(),
                    Some(Node::ClassExpression(class)) => class
                        .members
                        .iter()
                        .filter_map(|member| match member {
                            tsr_ast::ClassElement::ConstructorDeclaration(ctor) => ctor.node_id,
                            _ => None,
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                if let [constructor] = constructors.as_slice()
                    && let Some(mut signature) = self.get_signature_from_declaration(*constructor)
                {
                    // The return is the class's declared type AS A
                    // REFERENCE over its own parameters — upstream's
                    // `getDeclaredTypeOfClassOrInterface` hands back the
                    // generic type whose type arguments ARE its type
                    // parameters, and only that shape is substitutable
                    // (`instantiate_type` arm 3 reads
                    // `type_reference_targets`). The raw declared type
                    // prints `Box<T>` and substitutes nothing — the
                    // iteration-1 reading.
                    let own: Option<Vec<crate::types::TypeId>> = type_parameters
                        .iter()
                        .map(|parameter| {
                            parameter
                                .node_id
                                .and_then(|id| self.binder.symbol_of(id))
                                .map(|s| self.get_declared_type_of_symbol(s))
                        })
                        .collect();
                    let declared = match own {
                        Some(arguments) if arguments.iter().all(|&a| a != error) => {
                            self.create_type_reference(symbol, arguments)
                        }
                        _ => error,
                    };
                    if declared != error {
                        signature.r#type = declared;
                        let answer = self.check_generic_call(&signature, None, node.arguments);
                        if answer != error {
                            bump(&COUNTERS.new_instantiated);
                            return answer;
                        }
                    }
                }
            }
            bump(&COUNTERS.new_type_parameters);
            return error;
        }
        // A non-generic class cannot take type arguments.
        if !node.type_arguments.is_empty() {
            bump(&COUNTERS.new_type_arguments);
            return error;
        }
        // `ast.HasModifier(valueDecl, ast.ModifierFlagsAbstract)` — upstream
        // reports "Cannot create an instance of an abstract class" and answers
        // `errorType`, so gapping here agrees with upstream rather than
        // diverging from it.
        if modifiers.iter().any(|modifier| {
            matches!(modifier, tsr_ast::ModifierLike::Token(token)
                if token.kind == SyntaxKind::AbstractKeyword)
        }) {
            bump(&COUNTERS.new_abstract);
            return error;
        }
        let declared = self.get_declared_type_of_symbol(symbol);
        bump(&COUNTERS.new_resolved);
        if declared == error {
            bump(&COUNTERS.new_resolved_error);
        }
        declared
    }

    /// The type of a `yield` expression.
    ///
    /// Ported from `Checker.checkYieldExpression` (`checker.go:10952`), reduced
    /// to the paths that reach `anyType` **before** any machinery this port
    /// lacks.
    ///
    /// # `any` here is a computed answer, not a gap wearing `any`
    ///
    /// This is the one place in this module that returns `anyType`, and it needs
    /// justifying against the `errorType`-not-`anyType` rule. The rule forbids
    /// answering `any` for a form we could not compute. It does not forbid
    /// answering `any` where **upstream's own computation returns `anyType`** —
    /// `signatures.rs` already does this for a declaration with no body. 430 of
    /// roughly 540 `>yield` baseline lines are `any`, and the two paths below
    /// return it unconditionally:
    ///
    /// - **No containing function** (`checker.go:10963`). `fn == nil` returns
    ///   `anyType` outright.
    /// - **A containing function that is not a generator**
    ///   (`checker.go:10967`). This return happens *before* the function reads
    ///   any return annotation or contextual type, so it cannot be perturbed by
    ///   the contextual typing this port does not have. That ordering is what
    ///   makes the answer safe rather than merely common.
    ///
    /// # Inside a real generator, only the uncontextualisable case answers
    ///
    /// For a generator with no return annotation, upstream ends at
    /// `getContextualIterationType(IterationTypeKindNext, fn)` falling back to
    /// `anyType` (`checker.go:11005`). This port has no contextual typing, so it
    /// would always take the fallback — right whenever the function has no
    /// contextual type, wrong when it has one.
    ///
    /// The fence is therefore on the **container's kind**, not on the yield: a
    /// function *declaration* and a class *method* cannot be contextually typed,
    /// while a function expression, an arrow and an object-literal method can.
    /// So the first two answer `any` and the rest gap.
    ///
    /// Gapped: `yield*` (needs `getIterationTypeOfIterable`), a generator with a
    /// return type annotation (needs
    /// `getIterationTypesOfGeneratorFunctionReturnType` — note this is *not*
    /// `any`, since `Generator<number>`'s next type is `unknown`), and any
    /// contextualisable container.
    /// Ported from `Checker.checkAwaitExpression` (`checker.go:10845`) —
    /// `checkAwaitedType` of the operand, over the
    /// [`Checker::awaited_type_no_alias`] slice. The grammar check and the
    /// "no effect" suggestion are diagnostics and out of scope here.
    fn check_await_expression(&mut self, node: &tsr_ast::AwaitExpression<'_>) -> TypeId {
        let error = self.intrinsics.error;
        let Some(operand) = node.expression else { return error };
        let operand_type = self.check_expression(operand);
        if operand_type == error {
            return error;
        }
        self.awaited_type_no_alias(operand_type).unwrap_or(error)
    }

    /// `getAwaitedTypeNoAlias` (`checker.go:31270`), widened from the §18
    /// slice (`checker-notes-callres.md`) to every shape decidable without
    /// the `Awaited<T>` alias mint. `None` is a gap, never `any`.
    ///
    /// The arms, in upstream's order:
    ///
    /// - `IsTypeAny(t)` passes through (`:31271`) — and this port's
    ///   `errorType` never reaches here (both callers screen it first).
    /// - A **union** awaits per constituent (`:31285`); one undecidable
    ///   constituent gaps the whole.
    /// - A **generic** type wraps in `Awaited<T>` (`isAwaitedTypeNeeded`,
    ///   `:31395`) — this port mints no conditional alias instantiations, so
    ///   every type-variable-flavored shape declines.
    /// - A reference to the **global `Promise`** unwraps to its argument
    ///   (`getPromisedTypeOfPromiseEx`'s short-circuit, `checker.go:28941`),
    ///   recursively. `PromiseLike<T>` reaches the same `T` upstream through
    ///   the `then`-signature walk — its `onfulfilled` first parameter is
    ///   declared `value: T` — so the reference form answers directly.
    /// - Everything else is its own awaited type **unless it carries a
    ///   `then` member** (`:31417`): a primitive cannot (`isThenableType`'s
    ///   first test, `:31450`), and an object type is probed through
    ///   [`Checker::get_type_of_property_of_type`]. A `then`-carrying type
    ///   needs the promised-type signature walk — unported, gap.
    pub(crate) fn awaited_type_no_alias(&mut self, id: TypeId) -> Option<TypeId> {
        let flags = self.store.get(id).flags;
        if flags.intersects(TypeFlags::ANY | TypeFlags::UNKNOWN) {
            return Some(id);
        }
        if let crate::types::TypeData::Union { types, .. } = &self.store.get(id).data {
            let constituents = types.clone();
            let mut mapped = Vec::with_capacity(constituents.len());
            for constituent in constituents {
                mapped.push(self.awaited_type_no_alias(constituent)?);
            }
            return Some(self.get_union_type(&mapped));
        }
        if flags.intersects(TypeFlags::PRIMITIVE | TypeFlags::NEVER) {
            return Some(id);
        }
        // `isAwaitedTypeNeeded`'s domain plus the deferred kinds whose
        // members this port cannot probe: all decline rather than guess.
        if flags.intersects(
            TypeFlags::TYPE_PARAMETER
                .union(TypeFlags::CONDITIONAL)
                .union(TypeFlags::INDEX)
                .union(TypeFlags::INDEXED_ACCESS)
                .union(TypeFlags::SUBSTITUTION)
                .union(TypeFlags::INTERSECTION),
        ) {
            return None;
        }
        if let Some((target, arguments)) = self.type_reference_targets.get(&id).cloned()
            && arguments.len() == 1
        {
            let target = self.binder.merged_symbol(target);
            let is_promise = ["Promise", "PromiseLike"].iter().any(|name| {
                self.global_type_symbol(name)
                    .is_some_and(|symbol| self.binder.merged_symbol(symbol) == target)
            });
            if is_promise {
                return self.awaited_type_no_alias(arguments[0]);
            }
        }
        match self.get_type_of_property_of_type(id, "then") {
            None => Some(id),
            Some(_) => None,
        }
    }

    fn check_yield_expression(&mut self, node: &tsr_ast::YieldExpression<'_>) -> TypeId {
        let error = self.intrinsics.error;
        let any = self.intrinsics.any;
        // Upstream checks the operand even when the yield is outside a
        // generator, "so its identifiers are resolved ... keeping diagnostics
        // stable regardless of traversal order". The operand's type does not
        // reach the answer on any path this port takes.
        if let Some(operand) = node.expression {
            self.check_expression(operand);
        }
        let Some(id) = node.node_id else { return error };
        let Some(container) = self.containing_function(id) else {
            // `fn == nil` — a `yield` at the top level.
            return any;
        };
        let (asterisk, annotation, contextualisable) = match self.node_map.get(container) {
            Some(Node::FunctionDeclaration(f)) => (f.asterisk_token, f.r#type, false),
            Some(Node::MethodDeclaration(f)) => (f.asterisk_token, f.r#type, false),
            Some(Node::FunctionExpression(f)) => (f.asterisk_token, f.r#type, true),
            // An arrow cannot be a generator at all, so it always takes the
            // not-a-generator arm below.
            Some(Node::ArrowFunction(f)) => (None, f.r#type, true),
            // SS184: an ACCESSOR or CONSTRUCTOR container cannot be a
            // generator either, so upstream's `functionFlags&Generator == 0`
            // arm answers `anyType` (checker.go:10967-10969) — this
            // enumeration stopped before them and answered `error`. The
            // §169 shape: a gate that lists kinds, missing the ones nobody
            // had a case for yet.
            Some(
                Node::GetAccessorDeclaration(_)
                | Node::SetAccessorDeclaration(_)
                | Node::ConstructorDeclaration(_),
            ) => return any,
            _ => return error,
        };
        if asterisk.is_none() {
            // Not a generator. Upstream returns `anyType` here before reading
            // anything else, so the container's contextual type is irrelevant.
            return any;
        }
        // §224: `contextualisable` is a property of the container's KIND, and
        // a kind that *can* be contextually typed is not one that *is*. A
        // function expression initialising a variable with no annotation is the
        // case that matters: `getContextualTypeForInitializerExpression`
        // (`checker.go:29356`'s arm) reads the declaration's type node, so with
        // no annotation there is provably no contextual type and upstream takes
        // the `anyType` fallback. `compiler/generatorES6_3` —
        // `var v = function*() { yield 0 }` — is one line, and it was refused
        // for a capability the fixture does not use.
        //
        // Same predicate as §223 in `signatures.rs`, one function up the tree:
        // there it asks whether a YIELD has a contextual type, here whether its
        // CONTAINER does. Both are `getContextualType`'s parent switch, and
        // both are written as declines so a mistake costs a gap.
        // §869: `has_no_contextual_type` replaces the one-shape predicate that
        // stood in for it. `container_is_provably_uncontextualised` recognised
        // exactly `var x = function*() { }` — a variable declaration with no
        // annotation — where the general walk now answers for eleven node
        // kinds (§863–§866, §869). It is the same question §169 and §561
        // already route through this predicate, asked here too.
        let contextualised = contextualisable && !self.has_no_contextual_type(container);
        // §225: an ANNOTATED generator's yield type is the annotation's NEXT
        // type, and for the shape the corpus actually writes that is readable
        // without `getIterationTypesOfGeneratorFunctionReturnType`. The
        // refusal's stated reason — "note this is *not* `any`, since
        // `Generator<number>`'s next type is `unknown`" — is exactly right and
        // is the reason this reads the slot rather than assuming `any`:
        // `Generator<T = unknown, TReturn = any, TNext = unknown>` and
        // `IterableIterator<T, TReturn = any, TNext = any>` disagree, so the
        // answer has to come from the declaration.
        if let Some(annotation) = annotation {
            // §425: a `yield*` in an ANNOTATED generator still answers the
            // DELEGATED iterable's return, not the container's next slot —
            // `function* g(): IterableIterator<Foo> { yield * [new Bar]; }`
            // records `undefined` (`generatorTypeCheck19/20`). Scoped to the
            // same array shape §349 decided, so the asyncGenerators families
            // that priced §349's placement keep their roads.
            let is_async = self.node_map.get(container).is_some_and(|function| {
                let modifiers = match function {
                    Node::FunctionDeclaration(f) => f.modifiers,
                    Node::MethodDeclaration(f) => f.modifiers,
                    Node::FunctionExpression(f) => f.modifiers,
                    _ => return false,
                };
                modifiers.iter().any(|modifier| {
                    matches!(modifier, tsr_ast::ModifierLike::Token(token)
                        if token.kind == SyntaxKind::AsyncKeyword)
                })
            });
            if !is_async
                && node.asterisk_token.is_some()
                && let Some(operand) = node.expression
            {
                let operand_type = self.check_expression(operand);
                if let Some((target, arguments)) = self.type_reference_targets.get(&operand_type)
                    && arguments.len() == 1
                    && !self
                        .store
                        .get(arguments[0])
                        .flags
                        .intersects(TypeFlags::UNDEFINED | TypeFlags::NEVER)
                    && self.global_type_symbol("Array").is_some_and(|array| {
                        self.binder.merged_symbol(*target) == self.binder.merged_symbol(array)
                    })
                {
                    return self.intrinsics.undefined;
                }
            }
            return self.next_type_of_annotated_generator(annotation).unwrap_or(error);
        }
        // §349: in the ONE slot the old flow gapped — unannotated,
        // uncontextualised, non-async `yield*` — an ARRAY operand answers the
        // DELEGATED iterable's RETURN type
        // (`getIterationTypeOfIterable(IterationTypeKindReturn, ...)`,
        // `checker.go:10993`): `undefined`, the lib's `ArrayIterator` TReturn
        // (`generatorTypeCheck22/23/24`). The first draft ran this BEFORE the
        // annotation branch and ahead of the async question — 9 R→W across
        // the asyncGenerators families — so it is strictly additive now:
        // every previously-answered shape keeps its road.
        if node.asterisk_token.is_some() {
            let is_async = self.node_map.get(container).is_some_and(|function| {
                let modifiers = match function {
                    Node::FunctionDeclaration(f) => f.modifiers,
                    Node::MethodDeclaration(f) => f.modifiers,
                    Node::FunctionExpression(f) => f.modifiers,
                    _ => return false,
                };
                modifiers.iter().any(|modifier| {
                    matches!(modifier, tsr_ast::ModifierLike::Token(token)
                        if token.kind == SyntaxKind::AsyncKeyword)
                })
            });
            if !is_async
                && !contextualised
                && let Some(operand) = node.expression
            {
                let operand_type = self.check_expression(operand);
                if let Some((target, arguments)) = self.type_reference_targets.get(&operand_type)
                    && arguments.len() == 1
                    // A DEGENERATE element — `yield * []`, whose element is
                    // `undefined`/`never` — answers `any` upstream
                    // (`YieldStarExpression4_es6`); only real elements take
                    // the `undefined` return.
                    && !self
                        .store
                        .get(arguments[0])
                        .flags
                        .intersects(TypeFlags::UNDEFINED | TypeFlags::NEVER)
                    && self.global_type_symbol("Array").is_some_and(|array| {
                        self.binder.merged_symbol(*target) == self.binder.merged_symbol(array)
                    })
                {
                    return self.intrinsics.undefined;
                }
            }
            // §868: the delegated iterable's RETURN type, read off the
            // `Generator` family's declared `TReturn` slot.
            //
            // `checkYieldExpression` (`checker.go:10998-11001`) answers
            // `getIterationTypeOfIterable(use, IterationTypeKindReturn, …)`
            // for a `yield*`, and every one of
            //
            //     Generator<T, TReturn, TNext>   AsyncGenerator<T, TReturn, TNext>
            //     IterableIterator<…>            AsyncIterableIterator<…>
            //     Iterator<…>                    AsyncIterator<…>
            //
            // declares `TReturn` as its **second** type argument — so when the
            // operand is a reference to one of them the answer is readable
            // without the `[Symbol.iterator]` walk that late binding blocks.
            // That is the same sidestep `for_of_element_type`
            // (`crate::symbols`) already takes for the FIRST slot, at index 1
            // instead of index 0.
            //
            // Async-ness is MATCHED rather than ignored, because
            // `IterationUseAsyncYieldStar` and `IterationUseYieldStar` are
            // what upstream distinguishes.
            if let Some(operand) = node.expression {
                let operand_type = self.check_expression(operand);
                let families: &[&str] = if is_async {
                    &["AsyncGenerator", "AsyncIterableIterator", "AsyncIterator"]
                } else {
                    &["Generator", "IterableIterator", "Iterator"]
                };
                if let Some((target, arguments)) =
                    self.type_reference_targets.get(&operand_type).cloned()
                    && arguments.len() >= 2
                {
                    let target = self.binder.merged_symbol(target);
                    for family in families {
                        if self
                            .global_type_symbol(family)
                            .is_some_and(|declared| self.binder.merged_symbol(declared) == target)
                        {
                            return arguments[1];
                        }
                    }
                }
            }
            return error;
        }
        if contextualised {
            return error;
        }
        any
    }

    /// `ast.GetContainingFunction` (`utilities.go`).
    ///
    /// The nearest function-like ancestor. Shares its walk shape with
    /// [`Self::check_this_expression`] but not its rules: `this` treats an arrow
    /// as transparent, and this does not — an arrow is a function for the
    /// purpose of "which function contains this node", which is exactly why a
    /// `yield` inside an arrow inside a generator is not the generator's yield.
    pub(crate) fn containing_function(&self, node: NodeId) -> Option<NodeId> {
        let mut current = self.nodes.parent(node);
        while let Some(id) = current {
            if matches!(
                self.nodes.kind(id),
                SyntaxKind::FunctionDeclaration
                    | SyntaxKind::FunctionExpression
                    | SyntaxKind::ArrowFunction
                    | SyntaxKind::MethodDeclaration
                    | SyntaxKind::GetAccessor
                    | SyntaxKind::SetAccessor
                    | SyntaxKind::Constructor
            ) {
                return Some(id);
            }
            current = self.nodes.parent(id);
        }
        None
    }
}

/// Negate a number literal's already-normalised text.
///
/// `getNumberLiteralType(-jsnum.FromString(text))` (`checker.go:10864`) negates
/// the *value*, so the answer has to go back through the same normalisation the
/// positive literal did rather than gaining a `-` on the front. The two differ on
/// exactly one input: negative zero, which upstream prints `0` and the baselines
/// record four times as `>-0 : 0`. Prefixing the text would print `-0`.
///
/// `None` when the text is not a parseable number, which is
/// [`printing::normalise_number`]'s fallback for a literal the scanner has
/// already reported on.
fn negate_number_text(normalised: &str) -> Option<String> {
    let value: f64 = normalised.parse().ok()?;
    let negated = -value;
    // `-0.0 == 0.0` is true in IEEE 754, so this catches negative zero without
    // needing to inspect the sign bit, and turns it into the `0` upstream prints.
    if negated == 0.0 {
        return Some("0".to_string());
    }
    #[allow(clippy::float_cmp, reason = "exact integrality is the intended test")]
    let is_integral = negated == negated.trunc();
    Some(if is_integral && negated.abs() < 1e21 {
        format!("{negated:.0}")
    } else {
        format!("{negated}")
    })
}

impl Checker<'_, '_> {
    /// The assignment-target walk plus kind classification —
    /// `ast.GetAssignmentTarget` (`internal/ast/utilities.go:184`) and
    /// `getAssignmentTargetKind` (`internal/checker/utilities.go:90`). A `=`
    /// or logical-assignment binary and a for-in/for-of initializer position
    /// are DEFINITE; other assignment operators and `++`/`--` are COMPOUND.
    /// The walk climbs parentheses, array literals, spreads, non-null
    /// assertions, and the object-literal assignment shapes, so destructuring
    /// targets classify the same as direct ones.
    pub(crate) fn assignment_target_kind(&self, id: NodeId) -> AssignmentTargetKind {
        self.assignment_target(id).map_or(AssignmentTargetKind::None, |target| {
            match self.node_map.get(target) {
                Some(Node::BinaryExpression(binary)) => {
                    match binary.operator_token.map(|token| token.kind) {
                        Some(
                            SyntaxKind::EqualsToken
                            | SyntaxKind::AmpersandAmpersandEqualsToken
                            | SyntaxKind::BarBarEqualsToken
                            | SyntaxKind::QuestionQuestionEqualsToken,
                        ) => AssignmentTargetKind::Definite,
                        _ => AssignmentTargetKind::Compound,
                    }
                }
                Some(Node::PrefixUnaryExpression(_) | Node::PostfixUnaryExpression(_)) => {
                    AssignmentTargetKind::Compound
                }
                Some(Node::ForInOrOfStatement(_)) => AssignmentTargetKind::Definite,
                _ => AssignmentTargetKind::None,
            }
        })
    }

    /// The `BinaryExpression`, unary increment/decrement, or
    /// for-in/for-of statement that references `id` as an assignment target —
    /// `ast.GetAssignmentTarget` (`internal/ast/utilities.go:184`).
    fn assignment_target(&self, id: NodeId) -> Option<NodeId> {
        let mut current = id;
        loop {
            let parent = self.nodes.parent(current)?;
            match self.node_map.get(parent)? {
                Node::BinaryExpression(binary) => {
                    let is_assignment = binary.operator_token.is_some_and(|token| {
                        matches!(
                            token.kind,
                            SyntaxKind::EqualsToken
                                | SyntaxKind::PlusEqualsToken
                                | SyntaxKind::MinusEqualsToken
                                | SyntaxKind::AsteriskEqualsToken
                                | SyntaxKind::AsteriskAsteriskEqualsToken
                                | SyntaxKind::SlashEqualsToken
                                | SyntaxKind::PercentEqualsToken
                                | SyntaxKind::LessThanLessThanEqualsToken
                                | SyntaxKind::GreaterThanGreaterThanEqualsToken
                                | SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
                                | SyntaxKind::AmpersandEqualsToken
                                | SyntaxKind::BarEqualsToken
                                | SyntaxKind::CaretEqualsToken
                                | SyntaxKind::AmpersandAmpersandEqualsToken
                                | SyntaxKind::BarBarEqualsToken
                                | SyntaxKind::QuestionQuestionEqualsToken
                        )
                    });
                    let left = binary.left.and_then(|l| Node::from(l).node_id());
                    return (is_assignment && left == Some(current)).then_some(parent);
                }
                Node::PrefixUnaryExpression(unary) => {
                    return matches!(
                        unary.operator.kind,
                        SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken
                    )
                    .then_some(parent);
                }
                Node::PostfixUnaryExpression(unary) => {
                    return matches!(
                        unary.operator.kind,
                        SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken
                    )
                    .then_some(parent);
                }
                Node::ForInOrOfStatement(statement) => {
                    let initializer = statement.initializer.and_then(|i| Node::from(i).node_id());
                    return (initializer == Some(current)).then_some(parent);
                }
                Node::ParenthesizedExpression(_)
                | Node::ArrayLiteralExpression(_)
                | Node::SpreadElement(_)
                | Node::NonNullExpression(_) => current = parent,
                // The object-literal assignment shapes hop to the literal
                // itself, whose parent the next iteration classifies.
                Node::SpreadAssignment(_) => current = self.nodes.parent(parent)?,
                Node::ShorthandPropertyAssignment(shorthand) => {
                    if Node::from(shorthand.name).node_id() != Some(current) {
                        return None;
                    }
                    current = self.nodes.parent(parent)?;
                }
                Node::PropertyAssignment(property) => {
                    if Node::from(property.name).node_id() == Some(current) {
                        return None;
                    }
                    current = self.nodes.parent(parent)?;
                }
                _ => return None,
            }
        }
    }

    /// `isInCompoundLikeAssignment` (`internal/checker/utilities.go:118`): a
    /// definite `=` whose right side (parentheses skipped) is a
    /// shift-or-higher binary — `x = x + 1` reads its target like `x += 1`.
    fn is_in_compound_like_assignment(&self, id: NodeId) -> bool {
        let Some(target) = self.assignment_target(id) else { return false };
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(target) else {
            return false;
        };
        if binary.operator_token.map(|token| token.kind) != Some(SyntaxKind::EqualsToken) {
            return false;
        }
        let mut right = binary.right;
        while let Some(Expression::ParenthesizedExpression(inner)) = right {
            right = inner.expression;
        }
        let Some(Expression::BinaryExpression(inner)) = right else { return false };
        // `isShiftOperatorOrHigher`: shift, additive, multiplicative,
        // exponentiation.
        inner.operator_token.is_some_and(|token| {
            matches!(
                token.kind,
                SyntaxKind::LessThanLessThanToken
                    | SyntaxKind::GreaterThanGreaterThanToken
                    | SyntaxKind::GreaterThanGreaterThanGreaterThanToken
                    | SyntaxKind::PlusToken
                    | SyntaxKind::MinusToken
                    | SyntaxKind::AsteriskToken
                    | SyntaxKind::SlashToken
                    | SyntaxKind::PercentToken
                    | SyntaxKind::AsteriskAsteriskToken
            )
        })
    }

    /// `getBaseTypeOfLiteralType` (`checker.go`, the literal arms): a literal
    /// widens to its base primitive; a union maps constituents; everything
    /// else — including enum-like, whose base walk lives elsewhere — returns
    /// unchanged, which preserves current behaviour for the shapes §10's bar
    /// did not size.
    pub(crate) fn get_base_type_of_literal_type(&mut self, id: TypeId) -> TypeId {
        let flags = self.store.get(id).flags;
        if flags.intersects(TypeFlags::ENUM_LIKE) {
            return id;
        }
        if flags.intersects(TypeFlags::STRING_LITERAL) {
            return self.intrinsics.string;
        }
        if flags.intersects(TypeFlags::NUMBER_LITERAL) {
            return self.intrinsics.number;
        }
        if flags.intersects(TypeFlags::BIG_INT_LITERAL) {
            return self.intrinsics.bigint;
        }
        if flags.intersects(TypeFlags::BOOLEAN_LITERAL) {
            return self.intrinsics.boolean;
        }
        if flags.intersects(TypeFlags::UNION) {
            if let TypeData::Union { types, .. } = &self.store.get(id).data {
                let constituents = types.clone();
                let mapped: Vec<TypeId> = constituents
                    .into_iter()
                    .map(|constituent| self.get_base_type_of_literal_type(constituent))
                    .collect();
                return self.get_union_type(&mapped);
            }
        }
        id
    }
}

/// §101's constant value.
pub(crate) enum EvaluatedValue {
    Number(f64),
    Text(String),
}

impl EvaluatedValue {
    fn render(self) -> String {
        match self {
            EvaluatedValue::Number(n) => tsr_core::jsnum::format_number(n),
            EvaluatedValue::Text(s) => s,
        }
    }
}

/// §101 (`checker-notes-narrow.md`): the symbol-free slice of upstream's
/// constant evaluator (`c.evaluate`, consumed by `checkTemplateExpression`
/// at `checker.go:7991`): literals, parens, prefix `+`/`-`, numeric
/// arithmetic, `+` string concatenation, and templates recursively.
/// Identifiers, property accesses, bitwise/shift operators, and anything
/// else answer `None` — a `None` anywhere keeps the type-level answer, so
/// this only ever adds folds.
pub(crate) fn evaluate_constant_expression(
    expr: &tsr_ast::Expression<'_>,
) -> Option<EvaluatedValue> {
    use tsr_ast::SyntaxKind;
    match expr {
        tsr_ast::Expression::NumericLiteral(n) => {
            Some(EvaluatedValue::Number(tsr_core::jsnum::numeric_value(n.text)))
        }
        tsr_ast::Expression::StringLiteral(s) => Some(EvaluatedValue::Text(s.text.to_string())),
        tsr_ast::Expression::ParenthesizedExpression(node) => {
            node.expression.as_ref().and_then(evaluate_constant_expression)
        }
        tsr_ast::Expression::PrefixUnaryExpression(node) => {
            let operand = node.operand.as_ref().and_then(evaluate_constant_expression)?;
            let EvaluatedValue::Number(value) = operand else { return None };
            match node.operator.kind {
                SyntaxKind::MinusToken => Some(EvaluatedValue::Number(-value)),
                SyntaxKind::PlusToken => Some(EvaluatedValue::Number(value)),
                _ => None,
            }
        }
        tsr_ast::Expression::BinaryExpression(node) => {
            let operator = node.operator_token?.kind;
            let left = node.left.as_ref().and_then(evaluate_constant_expression)?;
            let right = node.right.as_ref().and_then(evaluate_constant_expression)?;
            match (left, right, operator) {
                (a, b, SyntaxKind::PlusToken) => match (a, b) {
                    (EvaluatedValue::Number(a), EvaluatedValue::Number(b)) => {
                        Some(EvaluatedValue::Number(a + b))
                    }
                    (a, b) => Some(EvaluatedValue::Text(a.render() + &b.render())),
                },
                (EvaluatedValue::Number(a), EvaluatedValue::Number(b), operator) => {
                    // §150 (`checker-notes-narrow.md`): the bitwise family
                    // folds with ECMA ToInt32/ToUint32 semantics — NaN and
                    // the infinities land 0, negatives wrap mod 2^32, and a
                    // shift count masks to its low five bits.
                    // The wrap IS the semantics: ToInt32 maps [0, 2^32) onto
                    // i32's two's-complement range, and rem_euclid's result
                    // is exact in f64 (< 2^32), so the truncating casts are
                    // deliberate.
                    #[allow(
                        clippy::cast_possible_wrap,
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss
                    )]
                    let to_int32 = |v: f64| -> i32 {
                        if !v.is_finite() {
                            return 0;
                        }
                        v.trunc().rem_euclid(4_294_967_296.0) as u32 as i32
                    };
                    // A shift count reads only its low five bits, so the
                    // sign-losing cast is the ECMA `& 31` mask itself.
                    #[allow(clippy::cast_sign_loss)]
                    let unsigned = |v: i32| v as u32;
                    let shift = move |v: f64| unsigned(to_int32(v)) & 31;
                    let value = match operator {
                        SyntaxKind::MinusToken => a - b,
                        SyntaxKind::AsteriskToken => a * b,
                        SyntaxKind::SlashToken => a / b,
                        SyntaxKind::PercentToken => a % b,
                        SyntaxKind::AsteriskAsteriskToken => a.powf(b),
                        SyntaxKind::AmpersandToken => f64::from(to_int32(a) & to_int32(b)),
                        SyntaxKind::BarToken => f64::from(to_int32(a) | to_int32(b)),
                        SyntaxKind::CaretToken => f64::from(to_int32(a) ^ to_int32(b)),
                        SyntaxKind::LessThanLessThanToken => f64::from(to_int32(a) << shift(b)),
                        SyntaxKind::GreaterThanGreaterThanToken => {
                            f64::from(to_int32(a) >> shift(b))
                        }
                        SyntaxKind::GreaterThanGreaterThanGreaterThanToken => {
                            f64::from(unsigned(to_int32(a)) >> shift(b))
                        }
                        _ => return None,
                    };
                    Some(EvaluatedValue::Number(value))
                }
                _ => None,
            }
        }
        // A no-substitution template is a **string literal with different
        // delimiters** and upstream's evaluator folds it as one
        // (`evaluator/evaluator.go:106`, alongside `KindStringLiteral`). §101 built
        // this evaluator for template *folding* and did not need the leaf,
        // because a template head already carries its own text; §819's consumer
        // reads `None` as *"not a constant"* and the omission became three
        // wrong lines on `enumConstantMemberWithTemplateLiterals`. §820.
        tsr_ast::Expression::NoSubstitutionTemplateLiteral(literal) => {
            Some(EvaluatedValue::Text(literal.text.to_string()))
        }
        tsr_ast::Expression::TemplateExpression(node) => {
            let mut folded = node.head.map(|head| head.text.to_string())?;
            for span in node.template_spans {
                let piece =
                    span.expression.as_ref().and_then(evaluate_constant_expression)?.render();
                let tail = match span.literal? {
                    tsr_ast::TemplateMiddleOrTail::TemplateMiddle(part) => part.text,
                    tsr_ast::TemplateMiddleOrTail::TemplateTail(part) => part.text,
                };
                folded = folded + &piece + tail;
            }
            Some(EvaluatedValue::Text(folded))
        }
        _ => None,
    }
}
