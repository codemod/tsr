//! Binding elements: the type a destructured name takes from its pattern's
//! parent. The plain leg of `bd tsr-o00`, registered in
//! `docs/architecture/checker-notes-destructure.md` before this file existed.
//!
//! Ported from `Checker.getTypeForBindingElement` (`checker.go:17684`),
//! `getTypeForBindingElementParent` (`checker.go:17695`) and
//! `getBindingElementTypeFromParentType` (`checker.go:17707`), restricted to
//! the legs the sizing probe (`examples/bindgap.rs`) showed are answerable
//! with machinery that exists:
//!
//! - an **object pattern** takes the element's literal property name into the
//!   same lookup pair `a["b"]` uses — `get_type_of_property_of_type`, then
//!   the applicable index signature (`crate::indexed` states the observation
//!   that a literal element access *is* a property access; upstream's
//!   `getIndexedAccessTypeEx` at `AccessFlagsExpressionPosition` is the same
//!   pair);
//! - an **array pattern** takes the element's *position* as a numeric
//!   property, which the tuple reverse index (`bd tsr-5ll`) answers for
//!   tuples and the number index signature answers for arrays — upstream's
//!   `isArrayLikeType` branch (`checker.go:17768`). A receiver that is
//!   neither (an iterable, a generic) finds no property and no applicable
//!   index signature and stays a gap, where upstream would consult
//!   `checkIteratedTypeOrElementType` — unported, refused rather than
//!   approximated.
//!
//! # What refuses, each with its number (`checker-notes-destructure.md` §3)
//!
//! - **rest elements** (172 lines): `getRestType` (`checker.go:17792`) needs
//!   spreadability and `Omit`; array rest needs `sliceTupleType`;
//! - **defaults** (224): the annotation-less path is
//!   `getUnionTypeEx(..., UnionReductionSubtype, ...)` (`checker.go:17789`),
//!   the reduction STATUS.md §5 refused for `||`/`??`; the whole element
//!   refuses, not just the default's contribution;
//! - **computed property names** (86): late-bound names (`bd tsr-y4u.11`);
//! - **contextual pattern parameters** (604, 57% want-any): upstream
//!   consults `getContextuallyTypedParameterType` (`checker.go:16735`) and
//!   then the parameter's initializer; this port's contextual slice answers
//!   `None` both for an *absent* context and an *unported* one, and the two
//!   demand opposite answers (implicit `any` vs the context's type), so a
//!   `None` here refuses rather than guessing — see
//!   [`Checker::get_type_for_binding_element_parent`];
//! - **no-source holders** (113): `for-of`/`for-in` heads need iteration
//!   machinery, a source-less pattern needs `getTypeFromBindingPattern`
//!   (`checker.go:17904`), the implied-type constructor.
//!
//! # The named risk
//!
//! `getFlowTypeOfDestructuring` (`checker.go:17849`) — flow narrowing of the
//! element through a synthetic reference — is unported, so this module
//! answers the *declared* slice where upstream may answer a *narrowed* one.
//! The registered bar attributes `conformance/dependentDestructuredVariables`
//! to this in advance; new wrong lines outside that family indict the arm.

use tsr_ast::{Node, NodeId, PropertyName, SyntaxKind};

use crate::{
    checker::Checker,
    flags::TypeFlags,
    flow::TypeFacts,
    printing,
    types::{TypeData, TypeId},
};

impl Checker<'_, '_> {
    /// Ported from `Checker.getTypeForBindingElement` (`checker.go:17684`),
    /// reached from the `SyntaxKind::BindingElement` arm of
    /// `get_type_of_variable_or_parameter_or_property_worker` — the same
    /// dispatch upstream makes at `checker.go:16603`, where a binding element
    /// travels with the other variable-likes.
    ///
    /// `errorType` is a refusal, never an answer: every early return below is
    /// a leg the module doc refuses by name.
    pub(crate) fn get_type_for_binding_element(&mut self, declaration: NodeId) -> TypeId {
        let error = self.intrinsics.error;
        let Some(Node::BindingElement(element)) = self.node_map.get(declaration) else {
            return error;
        };
        // The refused element forms — rest, computed name, and every default
        // outside the annotated-root leg. The whole element refuses, because
        // `[Unported, string]` is not `[any, string]` (the tuple arm's rule,
        // applied to a name's slice of its parent).
        if element.dot_dot_dot_token.is_some() {
            return error;
        }
        // A default is admitted only on the leg `checker.go:17781` separates:
        // the root declaration carries an annotation — so the default never
        // unions into the element (that is the refused `UnionReductionSubtype`
        // path, which runs annotation-less) — and the element's name is an
        // identifier, because a pattern-named default takes upstream through
        // `padObjectLiteralType`/`padTupleType` (`checker.go:16808`),
        // unported. `checker-notes-destructure.md` §6.
        if element.initializer.is_some()
            && (!self.binding_root_has_annotation(declaration)
                || !matches!(element.name, Some(tsr_ast::BindingName::Identifier(_))))
        {
            return error;
        }
        if matches!(element.property_name, Some(PropertyName::ComputedPropertyName(_))) {
            return error;
        }
        let Some(pattern_id) = self.nodes.parent(declaration) else { return error };
        let Some(holder) = self.nodes.parent(pattern_id) else { return error };
        let parent_type = self.get_type_for_binding_element_parent(holder);
        if parent_type == error {
            return error;
        }
        // "If an any type was inferred for parent, infer that for the binding
        // element" (`checker.go:17709`). Identity against `intrinsics.any`,
        // not a flag test — `errorType` also carries `ANY`, and the guard
        // above must keep owning it (the same reasoning `indexed.rs` records).
        if parent_type == self.intrinsics.any {
            return parent_type;
        }
        let parent_type = self.destructuring_parent_adjusted(declaration, holder, parent_type);

        let element_type = match self.nodes.kind(pattern_id) {
            SyntaxKind::ObjectBindingPattern => {
                // "Use explicitly specified property name ({ p: xxx } form),
                // or otherwise the implied name ({ p } form)"
                // (`checker.go:17740`), then `getLiteralTypeFromPropertyName`
                // into an indexed access. `getFlowTypeOfDestructuring`
                // (`checker.go:17743`) is unported — the module doc's named
                // risk — so the declared slice is the answer.
                let Some((name, numeric)) = Self::binding_element_property_name(element) else {
                    return error;
                };
                self.destructuring_property_lookup(parent_type, &name, numeric)
            }
            SyntaxKind::ArrayBindingPattern => {
                // The element's position is the property name
                // (`checker.go:17769`), which is why the parser records holes
                // as empty elements — upstream's `slices.Index` counts them.
                let Some(Node::BindingPattern(pattern)) = self.node_map.get(pattern_id) else {
                    return error;
                };
                let Some(index) =
                    pattern.elements.iter().position(|e| e.node_id == Some(declaration))
                else {
                    return error;
                };
                self.destructuring_property_lookup(parent_type, &index.to_string(), true)
            }
            _ => error,
        };
        if element_type == error {
            return error;
        }
        // The annotated-root default strip (`checker.go:17782`–`:17786`),
        // under the standing strict-throughout assumption: a default of a
        // non-`undefined` type removes `undefined` from the element. The
        // facts test is upstream's `hasTypeFacts(.., TypeFactsIsUndefined)`;
        // the strip is `getNonUndefinedType` minus its generic-constraint
        // mapping (§6 of the notes page — an instantiable constituent passes
        // through `get_type_with_facts` untouched where upstream may consult
        // its constraint, stated rather than verified).
        let element_type = if let Some(default_expression) = element.initializer {
            let default_type = self.check_expression(default_expression);
            if default_type == error {
                return error;
            }
            if self.get_type_facts(default_type).contains(TypeFacts::IS_UNDEFINED) {
                element_type
            } else {
                self.get_type_with_facts(element_type, TypeFacts::NE_UNDEFINED)
            }
        } else {
            element_type
        };
        // `getWidenedTypeForVariableLikeDeclaration` wraps every binding
        // element in `widenTypeForVariableLikeDeclaration` →
        // `getWidenedType` (`checker.go:16647`, `:18258`). **No measured
        // corpus line is known to depend on this wrap**, stated rather than
        // presented as verified: every parent this module accepts already
        // carries widened member types (annotations are regular, object
        // literal members widen at `check_expression_for_mutable_location`'s
        // boundary), so a fresh literal may be unreachable here. It was
        // first suspected of owning the `want boolean / got true` family in
        // `bd tsr-o00`'s first run; that family turned out to be *shorthand
        // members* under assignment narrowing losing freshness, which is not
        // this module's code and is filed separately. Kept because it is
        // upstream's wrap and a regular type passes through untouched.
        self.get_widened_literal_type(element_type)
    }

    /// Ported from `Checker.getTypeForBindingElementParent`
    /// (`checker.go:17695`) into the parent half of
    /// `getTypeForVariableLikeDeclaration` (`checker.go:16652`), with
    /// `includeOptionality: false` — the parent path adds no `undefined` for
    /// an optional holder; the strict-mode adjustments in
    /// [`Self::destructuring_parent_adjusted`] own that instead.
    ///
    /// Upstream's symbol-cache probe (`checker.go:17698`) is skipped: a
    /// holder whose name is a pattern declares no symbol of its own, so the
    /// probe answers nil for every holder this function sees.
    ///
    /// Refusals, where upstream computes (module doc §refused):
    /// a `Parameter` without an annotation, because contextual typing's
    /// `None` cannot distinguish "no context" (upstream: implicit `any` or
    /// the initializer) from "unported context" (upstream: the context's
    /// type); and a holder with neither annotation nor initializer, where
    /// upstream reaches iteration (`for-of`), index keys (`for-in`), catch
    /// variables, or the pattern's implied type — all unported.
    fn get_type_for_binding_element_parent(&mut self, holder: NodeId) -> TypeId {
        let error = self.intrinsics.error;
        match self.nodes.kind(holder) {
            // A nested pattern: the holder is itself a binding element, and
            // its type is this module again (`checker.go:16672`'s dispatch).
            SyntaxKind::BindingElement => self.get_type_for_binding_element(holder),
            SyntaxKind::VariableDeclaration | SyntaxKind::Parameter => {
                // An annotation wins over an initialiser, always
                // (`checker.go:16694`).
                if let Some(annotation) = self.type_annotation_of(holder) {
                    return self.get_type_from_type_node(annotation);
                }
                if self.nodes.kind(holder) == SyntaxKind::Parameter {
                    if let Some(contextual) = self.get_contextually_typed_parameter_type(holder) {
                        return contextual;
                    }
                    return error;
                }
                if let Some(initializer) = self.initializer_of(holder) {
                    // An **array literal destructured by an array pattern**
                    // refuses whole. Upstream types the initializer under the
                    // pattern's implied contextual type
                    // (`checkDeclarationInitializer` threads
                    // `getTypeFromBindingPattern`, `checker.go:16748` →
                    // `:17904`), which is what makes `var [a, b] = [1, "x"]`
                    // infer the *tuple* `[number, string]` — so `a` is
                    // `number`. Without that machinery this port's
                    // `check_expression` answers `(string | number)[]` and
                    // every element would print the union: measured on the
                    // first run of `bd tsr-o00` as ~80 wrong lines across
                    // `declarationEmitDestructuringArrayPattern1/2/4` and
                    // `destructuringArrayBindingPatternAndAssignment1*`. A
                    // gap beats that wrong answer.
                    if matches!(initializer, tsr_ast::Expression::ArrayLiteralExpression(_))
                        && self.holder_pattern_kind(holder) == Some(SyntaxKind::ArrayBindingPattern)
                    {
                        return error;
                    }
                    // `widenTypeInferredFromInitializer(checkDeclarationInitializer(..))`
                    // (`checker.go:16748`), the same pair the identifier path
                    // takes — a `const` keeps literals, anything else widens.
                    let initializer_type = self.check_expression(initializer);
                    if initializer_type == error {
                        return error;
                    }
                    return self.get_widened_literal_type_for_initializer(holder, initializer_type);
                }
                error
            }
            _ => error,
        }
    }

    /// The strict-mode parent adjustments of
    /// `getBindingElementTypeFromParentType` (`checker.go:17713`–`:17718`),
    /// under this port's standing strict-throughout assumption
    /// (`array_literals.rs` and `unions.rs` state the same one):
    ///
    /// - an **ambient parameter's** pattern strips nullables — "the
    ///   parameters have no implementation and are just documentation";
    /// - a holder **initializer that cannot be `undefined`** removes
    ///   `undefined` from the parent — `getTypeWithFacts(parentType,
    ///   TypeFactsNEUndefined)`.
    fn destructuring_parent_adjusted(
        &mut self,
        declaration: NodeId,
        holder: NodeId,
        parent_type: TypeId,
    ) -> TypeId {
        if self.combined_node_flags(declaration).intersects(tsr_ast::NodeFlags::AMBIENT)
            && self.is_part_of_parameter_declaration(declaration)
        {
            return self.check_non_null_type(parent_type);
        }
        if let Some(initializer) = self.initializer_of(holder) {
            let initializer_type = self.check_expression(initializer);
            if initializer_type != self.intrinsics.error
                && !self.get_type_facts(initializer_type).contains(TypeFacts::EQ_UNDEFINED)
            {
                return self.get_type_with_facts(parent_type, TypeFacts::NE_UNDEFINED);
            }
        }
        parent_type
    }

    /// The kind of the pattern a holder declares — `holder.name` when it is a
    /// pattern. `None` for an identifier name, which cannot reach this module.
    fn holder_pattern_kind(&self, holder: NodeId) -> Option<SyntaxKind> {
        let name = match self.node_map.get(holder)? {
            Node::VariableDeclaration(declaration) => declaration.name,
            Node::ParameterDeclaration(parameter) => parameter.name,
            _ => None,
        }?;
        match name {
            tsr_ast::BindingName::BindingPattern(pattern) => {
                Some(self.nodes.kind(pattern.node_id?))
            }
            tsr_ast::BindingName::Identifier(_) => None,
        }
    }

    /// Upstream's `ast.WalkUpBindingElementsAndPatterns(declaration).Type()
    /// != nil` (`checker.go:17781`): whether the **root** declaration of the
    /// binding chain — the variable declaration or parameter the outermost
    /// pattern names — carries a type annotation. This is what separates the
    /// strip-`undefined` default leg from the refused union leg.
    fn binding_root_has_annotation(&self, declaration: NodeId) -> bool {
        let mut current = self.nodes.parent(declaration);
        while let Some(node) = current {
            match self.nodes.kind(node) {
                SyntaxKind::ObjectBindingPattern
                | SyntaxKind::ArrayBindingPattern
                | SyntaxKind::BindingElement => current = self.nodes.parent(node),
                _ => return self.type_annotation_of(node).is_some(),
            }
        }
        false
    }

    /// Upstream's `ast.IsPartOfParameterDeclaration` — the walked-up root of
    /// the binding chain is a `Parameter` (`utilities.go`).
    fn is_part_of_parameter_declaration(&self, declaration: NodeId) -> bool {
        let mut current = self.nodes.parent(declaration);
        while let Some(node) = current {
            match self.nodes.kind(node) {
                SyntaxKind::ObjectBindingPattern
                | SyntaxKind::ArrayBindingPattern
                | SyntaxKind::BindingElement => current = self.nodes.parent(node),
                kind => return kind == SyntaxKind::Parameter,
            }
        }
        false
    }

    /// `getLiteralTypeFromPropertyName` (`checker.go:21762`)'s literal arms,
    /// as a name string plus whether it is numeric. A `PropertyName` form
    /// outside them — bigint, private, template — refuses.
    fn binding_element_property_name(
        element: &tsr_ast::BindingElement<'_>,
    ) -> Option<(String, bool)> {
        match &element.property_name {
            Some(PropertyName::Identifier(identifier)) => {
                Some((identifier.text.to_string(), false))
            }
            Some(PropertyName::StringLiteral(literal)) => Some((literal.text.to_string(), false)),
            Some(PropertyName::NumericLiteral(literal)) => {
                Some((printing::normalise_number(literal.text), true))
            }
            Some(_) => None,
            None => match &element.name {
                Some(tsr_ast::BindingName::Identifier(identifier)) => {
                    Some((identifier.text.to_string(), false))
                }
                _ => None,
            },
        }
    }

    /// The lookup pair `getIndexedAccessTypeEx` reduces to for a literal name
    /// at `AccessFlagsExpressionPosition`: the property, else the applicable
    /// index signature — the same pair `indexed.rs` runs for `a["b"]`, and
    /// the reason `[a, b] = t` answers tuple elements through the `tsr-5ll`
    /// reverse index without an arm of its own here.
    ///
    /// A miss on both is a gap: upstream reports TS2339/TS2493 and answers
    /// `errorType`, the same text by a different route.
    fn destructuring_property_lookup(
        &mut self,
        parent_type: TypeId,
        name: &str,
        numeric: bool,
    ) -> TypeId {
        if let Some(property_type) = self.get_type_of_property_of_type(parent_type, name) {
            return property_type;
        }
        let key = if numeric {
            self.store.intern_literal(
                TypeFlags::NUMBER_LITERAL,
                TypeData::NumberLiteral(name.to_string()),
                false,
            )
        } else {
            self.store.intern_literal(
                TypeFlags::STRING_LITERAL,
                TypeData::StringLiteral(name.to_string()),
                false,
            )
        };
        self.get_applicable_index_info(parent_type, key)
            .map_or(self.intrinsics.error, |info| info.value)
    }
}
