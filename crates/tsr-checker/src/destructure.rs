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
//! - **array rest elements**: `sliceTupleType` (`checker.go:17797`). The
//!   OBJECT half of the original rest refusal (172 lines) LANDED at §319 —
//!   `getRestType`'s member subtraction over `spread_members_of`; what that
//!   enumerator refuses (methods, nullables, instantiated references) still
//!   gaps the rest element;
//! - **pattern-named defaults**: `padObjectLiteralType`/`padTupleType`
//!   (`checker.go:16808`). The identifier-named half of the original default
//!   refusal (224 lines) LANDED at §315, both legs of `checker.go:17781` —
//!   the reduction the refusal named as missing was built at §206;
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
    /// §50's re-projection: the element against a NARROWED parent type.
    pub(crate) fn project_binding_element(
        &mut self,
        declaration: NodeId,
        parent_override: TypeId,
    ) -> TypeId {
        self.get_type_for_binding_element_impl(declaration, Some(parent_override))
    }

    pub(crate) fn get_type_for_binding_element(&mut self, declaration: NodeId) -> TypeId {
        self.get_type_for_binding_element_impl(declaration, None)
    }

    fn get_type_for_binding_element_impl(
        &mut self,
        declaration: NodeId,
        parent_override: Option<TypeId>,
    ) -> TypeId {
        let error = self.intrinsics.error;
        let Some(Node::BindingElement(element)) = self.node_map.get(declaration) else {
            return error;
        };
        // The refused element forms — rest, computed name, and every default
        // outside the annotated-root leg. The whole element refuses, because
        // `[Unported, string]` is not `[any, string]` (the tuple arm's rule,
        // applied to a name's slice of its parent).
        // A REST element branches by pattern kind below (§319): the object
        // form is `getRestType` (`checker.go:17792`), whose member subtraction
        // the spread machinery already knows how to enumerate; the array form
        // still needs `sliceTupleType` and stays refused in its arm.
        // A default is admitted on the two legs `checker.go:17781` separates —
        // annotated root (the strip below) and, since §315, the
        // annotation-less union (`getUnionTypeEx(strip(t) ∪ init,
        // UnionReductionSubtype)`, `checker.go:17789`) — the reduction the
        // original refusal named as its missing piece exists now
        // (`union_with_subtype_reduction`, §206's other client). The element's
        // name must still be an identifier: a pattern-named default takes
        // upstream through `padObjectLiteralType`/`padTupleType`
        // (`checker.go:16808`), unported. `checker-notes-destructure.md` §6.
        if element.initializer.is_some()
            && !matches!(element.name, Some(tsr_ast::BindingName::Identifier(_)))
        {
            return error;
        }
        // §691: a COMPUTED destructuring key is resolved against the source's
        // INDEX SIGNATURE, not declined. `let {[numed]: prop3} = numIndexed`
        // with `numed: number` and `numIndexed: { [idx: number]: string }`
        // records `prop3 : string`
        // (`lateBoundDestructuringImplicitAnyError`); the same file's
        // `{[named]: prop2} = numIndexed` — a STRING key against a NUMBER index
        // — correctly stays `any`, and `get_applicable_index_info` makes that
        // distinction itself.
        //
        // Handled below, once `parent_type` is known; a late-bound name that
        // reaches no index info still returns `error`.
        let computed_key = match element.property_name {
            Some(PropertyName::ComputedPropertyName(computed)) => {
                let Some(expression) = computed.expression else { return error };
                Some(self.check_expression(expression))
            }
            _ => None,
        };
        let Some(pattern_id) = self.nodes.parent(declaration) else { return error };
        let Some(holder) = self.nodes.parent(pattern_id) else { return error };
        let parent_type = match parent_override {
            Some(overridden) => overridden,
            None => self.get_type_for_binding_element_parent(holder),
        };
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
                if element.dot_dot_dot_token.is_some() {
                    return self.object_rest_type(parent_type, pattern_id, declaration);
                }
                if let Some(key) = computed_key {
                    if key == error {
                        return error;
                    }
                    return self
                        .get_applicable_index_info(parent_type, key)
                        .map_or(error, |info| info.value);
                }
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
                if element.dot_dot_dot_token.is_some() {
                    // §321: `sliceTupleType` (`checker.go:17797`), the PLAIN
                    // slice — `var [x, ...tail]: [number, string, string]`
                    // reads `[string, string]`. A tuple with an OPTIONAL or
                    // readonly shape declines: the sliced mask and the
                    // mutability of the destructured copy are their own
                    // questions, and a wrong spelling is worse than the gap
                    // this element always had. A non-tuple parent declines
                    // too (upstream builds `T[]` from the iterated type).
                    if let Some((elements, readonly)) =
                        self.tuple_element_lists.get(&parent_type).cloned()
                        && !readonly
                        && index <= elements.len()
                        && !self
                            .tuple_optional_masks
                            .get(&parent_type)
                            .is_some_and(|mask| mask.iter().any(|&optional| optional))
                    {
                        return self.create_tuple_type(elements[index..].to_vec(), false);
                    }
                    // §547: the NON-TUPLE parent, which the comment above
                    // named and left as a gap — *"upstream builds `T[]` from
                    // the iterated type"*. `getTypeForBindingElement`
                    // (`checker.go:17797`) reaches
                    // `checkIteratedTypeOrElementType` for a parent that is not
                    // a tuple and wraps the element in an array, so
                    // `var [a, ...b] = new SymbolIterator` reads
                    // `b : symbol[]` (`iterableArrayPattern2`) and
                    // `var [d, ...e] = [1, 2, 3]` reads `e : number[]`.
                    //
                    // The element comes from [`Checker::for_of_element_type`],
                    // the same §284/§285 seam the array-spread road uses, so a
                    // custom iterator and a plain array take one path. A parent
                    // whose iterated type this port cannot decide keeps the
                    // gap — `None` here, never a guess.
                    //
                    // A `readonly` or OPTIONAL-masked TUPLE still declines
                    // above and must keep declining: this arm is reached only
                    // when the parent is not a tuple at all, so it cannot
                    // silently answer the sliced-mask question §321 refused.
                    if !self.tuple_element_lists.contains_key(&parent_type)
                        && let Some(element_type) = self.for_of_element_type(parent_type)
                        && element_type != error
                        && let Some(array) = self.global_type_symbol("Array")
                    {
                        return self.create_type_reference(array, vec![element_type]);
                    }
                    return error;
                }
                // §497: upstream gates the positional read on
                // `isArrayLikeType` (`checker.go:17769`); an ANONYMOUS parent
                // — an object or type literal, a function — is never
                // array-like, so the numeric property is NOT consulted even
                // when it exists (`var [a, b] = { 0: "", 1: true }` reported
                // string/boolean here where upstream reports not-iterable and
                // answers error-any, `iterableArrayPattern21`). With no
                // computed member that could spell `[Symbol.iterator]` the
                // iteration road decidably fails →
                // `checkIteratedTypeOrElementType`'s `anyType`
                // (`checker.go:6103`); a computed member in either table is
                // undecidable and keeps the gap.
                // SS497: upstream gates the positional read on
                // `isArrayLikeType` (`checker.go:17769`); an OBJECT-SHAPED
                // parent - an object literal or a type literal, widened or
                // not - is never array-like, so the numeric property is NOT
                // consulted even when it exists
                // (`var [a, b] = { 0: "", 1: true }` reported string/boolean
                // here where upstream reports not-iterable and answers
                // error-any, `iterableArrayPattern21`). With no computed
                // member that could spell `[Symbol.iterator]` the iteration
                // road decidably fails - `checkIteratedTypeOrElementType`'s
                // `anyType` (`checker.go:6103`); a computed member is
                // undecidable and keeps the gap. Decided by the member
                // symbol's DECLARATIONS (a class instance prints Named too
                // and must not fire - its protocol question is
                // `iteration_decidably_fails`' separate arm).
                let literal_shaped_symbol = match self.store.get(parent_type).data {
                    TypeData::Anonymous { symbol, .. }
                    | TypeData::Named { members: Some(symbol), .. } => Some(symbol),
                    _ => None,
                }
                .filter(|&symbol| {
                    let declarations = &self.binder.symbols().get(symbol).declarations;
                    !declarations.is_empty()
                        && declarations.iter().all(|&declaration| {
                            matches!(
                                self.node_map.get(declaration),
                                Some(Node::ObjectLiteralExpression(_) | Node::TypeLiteralNode(_))
                            )
                        })
                });
                if let Some(symbol) = literal_shaped_symbol {
                    let entry = self.binder.symbols().get(symbol);
                    if entry.members.contains_key("__computed")
                        || entry.exports.contains_key("__computed")
                    {
                        return error;
                    }
                    return self.intrinsics.any;
                }
                let positional =
                    self.destructuring_property_lookup(parent_type, &index.to_string(), true);
                if positional == error {
                    // Upstream's else-arm (`checker.go:17771`): a receiver
                    // that is not array-like takes
                    // `checkIteratedTypeOrElementType`. §317 wires the slice
                    // of it that exists — §284's `for_of_element_type`, gated
                    // to receivers with a SYNTACTIC `[Symbol.iterator]` —
                    // `var [a, b] = new SymbolIterator` reads `symbol` from
                    // `next()`'s return (`iterableArrayPattern1/2`). A
                    // receiver neither road answers stays the gap it was.
                    // §571: the syntactic gate is DROPPED, exactly as §569
                    // dropped it on the spread road. `declares_symbol_iterator`
                    // asks whether the receiver's OWN declarations spell
                    // `[Symbol.iterator]`, which a LIB iterable never does —
                    // `Generator`, `Set`, `Map` carry it on the interface — so
                    // `var [a] = new SymbolIterator` read `symbol` while
                    // `var [a] = g()` gapped. `for_of_element_type` answers
                    // `Option` and declines where it cannot decide, so removing
                    // the gate can only turn a gap into an answer the for-of
                    // road already trusts.
                    if let Some(element) = self.for_of_element_type(parent_type) {
                        element
                    } else {
                        error
                    }
                } else {
                    positional
                }
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
            if self.binding_root_has_annotation(declaration) {
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
                // §315: the annotation-less leg (`checker.go:17789`) —
                // `getUnionTypeEx([strip(t), checkDeclarationInitializer],
                // UnionReductionSubtype)`: `var [x = 20] = [1, 2]` records
                // `>x : number`, the default folding INTO the element
                // (`sourceMapValidation…ArrayBindingPattern6/7`).
                // `check_expression_for_mutable_location` is the port's
                // `checkDeclarationInitializer`: same widening boundary, same
                // caller polarity. A reduction this port cannot run answers
                // `None` and the element stays a gap.
                // The default enters the union UNWIDENED — the fresh literal
                // is what the subtype reduction absorbs into the element's
                // annotated constituents (`let { a: a2 = 0 } = x` with
                // `a: 0 | 1 | undefined` records `>a2 : 0 | 1`,
                // `literalTypesAndDestructuring`). Widening is the TAIL's job
                // (`widenTypeInferredFromInitializer` wraps the union,
                // `checker.go:17789`; this function's tail is that wrap) —
                // the first draft widened the default BEFORE the union and
                // flattened those elements to `number`/`string` (7 G→W).
                let default_type = self.check_expression(default_expression);
                if default_type == error {
                    return error;
                }
                let stripped = self.get_type_with_facts(element_type, TypeFacts::NE_UNDEFINED);
                let Some(reduced) = self.union_with_subtype_reduction(&[stripped, default_type])
                else {
                    return error;
                };
                // `widenTypeInferredFromInitializer`'s union half: a FRESH
                // literal SURVIVING the reduction widens the whole inference
                // (`a3 = 2` against `0 | 1 | undefined` is `number`); one the
                // reduction absorbed does not (`a2 = 0` is `0 | 1`). Both
                // recorded in `literalTypesAndDestructuring`. A single
                // surviving fresh literal is the tail's job already. CONST
                // roots never widen — `const { c2 = 0 } = { c2: 1 }` records
                // `>c2 : 0 | 1` (`literalTypes2`), upstream's
                // `isConstVariable` gate on the same wrap.
                let root_is_const =
                    self.combined_node_flags(declaration).intersects(tsr_ast::NodeFlags::CONSTANT);
                match &self.store.get(reduced).data {
                    crate::types::TypeData::Union { types, .. }
                        if !root_is_const && types.iter().any(|&c| self.store.get(c).fresh) =>
                    {
                        let constituents = types.clone();
                        let widened: Vec<_> = constituents
                            .iter()
                            .map(|&c| self.get_widened_literal_type(c))
                            .collect();
                        let Some(rewidened) = self.union_with_subtype_reduction(&widened) else {
                            return error;
                        };
                        rewidened
                    }
                    _ => reduced,
                }
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
    pub(crate) fn get_type_for_binding_element_parent(&mut self, holder: NodeId) -> TypeId {
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
                    // §429: an UNCONTEXTUAL unannotated pattern parameter's
                    // elements read the pattern's IMPLIED type (the [any, any]
                    // tuple / { a: any } object the declaration road now
                    // mints), not a gap — `function fun([a, b]) {}` types
                    // both elements `any` (`iterableArrayPattern10/13`).
                    // FUNCTION DECLARATIONS only: expression/arrow parameters
                    // may be contextually typed upstream, and the implied
                    // `any` there measured 90 G->W
                    // (`coAndContraVariantInferences3`).
                    // §563: an ARROW or FUNCTION EXPRESSION too, where §94's
                    // predicate can SHOW there is no contextual type — §561's
                    // refinement applied to the fourth and last gate of this
                    // shape. The recorded reason here is the same one
                    // (90 G->W on `coAndContraVariantInferences3`) and it is
                    // the same claim about CONTEXTUALLY TYPED arrows.
                    //
                    // §561's lesson carried across: the implied type is
                    // consulted, and a BARE `any` result means the computation
                    // could not spell the pattern (a rest-only or
                    // optional-element shape). §136's arm would then hand every
                    // element that `any` — a confident wrong answer where a gap
                    // stood. So the bare-`any` case keeps the gap for the
                    // newly-admitted containers, exactly as `parameter_of` does.
                    let container = self.nodes.parent(holder);
                    let admitted = container.is_some_and(|f| match self.nodes.kind(f) {
                        SyntaxKind::FunctionDeclaration | SyntaxKind::MethodDeclaration => true,
                        SyntaxKind::ArrowFunction | SyntaxKind::FunctionExpression => {
                            self.has_no_contextual_type(f)
                        }
                        _ => false,
                    });
                    if admitted {
                        let implied = self.get_widened_type_for_variable_like_declaration(holder);
                        let expression_container = container.is_some_and(|f| {
                            matches!(
                                self.nodes.kind(f),
                                SyntaxKind::ArrowFunction | SyntaxKind::FunctionExpression
                            )
                        });
                        if expression_container && implied == self.intrinsics.any {
                            return error;
                        }
                        return implied;
                    }
                    return error;
                }
                // §399: a FOR-OF head's pattern destructures the ITERATED
                // element — `for (var {x: a, y: b} of array)` reads `a` and
                // `b` from the element's members (`for-of41/42/43`), the same
                // `for_of_element_type` the plain-name arm has had since §38.
                // `for await` and undecidable iterables keep the gap.
                if let Some(list) = self.nodes.parent(holder)
                    && let Some(statement) = self.nodes.parent(list)
                    && self.nodes.kind(statement) == SyntaxKind::ForOfStatement
                    && let Some(Node::ForInOrOfStatement(for_of)) = self.node_map.get(statement)
                    && for_of.await_modifier.is_none()
                    && let Some(expression) = for_of.expression
                {
                    let iterated = self.check_expression(expression);
                    if iterated == error {
                        return error;
                    }
                    if let Some(element) = self.for_of_element_type(iterated) {
                        return element;
                    }
                    return error;
                }
                if let Some(initializer) = self.initializer_of(holder) {
                    // **The pattern-implied contextual type** (`bd tsr-84iz`,
                    // `checker-notes-patctx.md`). Upstream threads
                    // `getTypeFromBindingPattern` (`checker.go:17904`) into
                    // `checkDeclarationInitializer` (`checker.go:16797`), and
                    // for an array pattern over an array literal that
                    // contextual type makes `checkArrayLiteral` infer a
                    // **tuple**: `var [a, b] = [1, "x"]` is `[number, string]`,
                    // so `a` is `number` and not `string | number`.
                    //
                    // Built here as the tuple of the literal's widened element
                    // types, through the **shared** `create_tuple_type`, so a
                    // tuple inferred from a literal and one written as an
                    // annotation are the same interned type.
                    if let tsr_ast::Expression::ArrayLiteralExpression(literal) = initializer
                        && self.holder_pattern_kind(holder) == Some(SyntaxKind::ArrayBindingPattern)
                    {
                        // §549: a DECLINE falls through to the plain
                        // initializer road below rather than poisoning the
                        // parent. `tuple_from_array_literal` refuses several
                        // shapes (a rest in the pattern, a spread in the
                        // literal, a pattern longer than the literal) and each
                        // refusal is about the TUPLE CONTEXT, not about the
                        // initializer having no type at all: upstream's
                        // `checkDeclarationInitializer` still answers
                        // `number[]` for `[1, 2, 3]` when the contextual tuple
                        // does not apply.
                        //
                        // Returning `error` here made the refusal contagious —
                        // `var [d, ...e] = [1, 2, 3]` gapped `e` AND `d`, a
                        // NON-rest element whose positional read
                        // (`isArrayLikeType`, `checker.go:17769`) works fine on
                        // `number[]`. §23.1 named this as §547's residue.
                        let tuple = self.tuple_from_array_literal(literal, holder);
                        if tuple != error {
                            return tuple;
                        }
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
        // Upstream tests `NodeFlagsAmbient`. This port's parser **sets that flag
        // nowhere** (§94, confirmed by §827's grep), so the branch was dead and
        // a destructuring parameter in an ambient context never got the
        // non-null adjustment. The syntactic stand-in is a `declare` on an
        // enclosing declaration, which is what the flag records upstream. §827.
        if self.is_in_ambient_context_for_overloads(declaration)
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

    /// The tuple an array literal implies when an array pattern destructures
    /// it — `bd tsr-84iz`, `docs/architecture/checker-notes-patctx.md`.
    ///
    /// This is `getTypeFromBindingPattern` (`checker.go:17904`) threaded into
    /// `checkDeclarationInitializer` (`checker.go:16797`), reduced to what
    /// that contextual type actually does to a plain array literal: each
    /// element keeps its own **widened** type instead of collapsing into the
    /// union an uncontextualised array literal produces.
    ///
    /// Every refusal below is a shape `examples/patctx.rs` measured this arm
    /// mispredicting, and each returns `errorType` so the construct gaps whole
    /// rather than answering part of it:
    ///
    /// - a **spread** in the literal, or a **rest** in the pattern — both need
    ///   `sliceTupleType`;
    /// - a pattern **longer than the literal**: upstream's out-of-range
    ///   element is optional and prints `T | undefined`, and the tuple minted
    ///   here carries no optional flag;
    /// - an **element that itself gaps** — a gap in an element gaps the tuple,
    ///   the rule the tuple type-node arm already follows.
    fn tuple_from_array_literal(
        &mut self,
        literal: &tsr_ast::ArrayLiteralExpression<'_>,
        holder: NodeId,
    ) -> TypeId {
        let error = self.intrinsics.error;
        let Some(Some(tsr_ast::BindingName::BindingPattern(pattern))) =
            self.node_map.get(holder).map(|node| match node {
                Node::VariableDeclaration(declaration) => declaration.name,
                _ => None,
            })
        else {
            return error;
        };
        if pattern.elements.iter().any(|element| element.dot_dot_dot_token.is_some()) {
            return error;
        }
        // A pattern longer than the literal reads out of range.
        if pattern.elements.len() > literal.elements.len() {
            return error;
        }
        let mut elements = Vec::with_capacity(literal.elements.len());
        for element in literal.elements {
            if matches!(element, tsr_ast::Expression::SpreadElement(_)) {
                return error;
            }
            let id = self.check_expression(*element);
            if id == error {
                return error;
            }
            // The element of a *contextually tuple-typed* literal widens its
            // literal types the same way a mutable location does — `[1, "x"]`
            // implies `[number, string]`, not `[1, "x"]`, which is what the
            // baselines record for a non-`const` declaration.
            elements.push(self.get_widened_literal_type(id));
        }
        self.create_tuple_type(elements, false)
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
    /// §319: `getRestType` (`checker.go:17792`), the object-pattern slice —
    /// the parent's spreadable members minus the names the pattern's OTHER
    /// elements bound: `var { a, ...rest } = { a: 1, b: "x" }` records
    /// `>rest : { b: string; }`. Everything `spread_members_of` refuses
    /// (methods, nullable members, instantiated references) keeps the gap
    /// this element always had — a partial rest object is a wrong answer
    /// that looks right. A sibling this port cannot name (a computed
    /// property) refuses too: subtracting an unknown name leaves a member
    /// upstream removed.
    /// Whether a CLASS-MEMBER declaration carries `keyword`. §776.
    ///
    /// Deliberately separate from `merged_export_spaces.rs`'s
    /// `declaration_has_modifier`, which covers TOP-LEVEL declaration kinds
    /// (interface, class, enum, function, module, `var` via its statement) and
    /// answers `false` for every member kind. The two match over disjoint node
    /// sets for different questions; merging them would make each caller carry
    /// the other's arms.
    ///
    /// Upstream reaches this through `getDeclarationModifierFlagsFromSymbol`
    /// (`utilities.go`); this port has no `ModifierFlags`, so the keywords are
    /// read off the node directly. `ParameterDeclaration` is in the list
    /// because a constructor PARAMETER PROPERTY (`constructor(private x: T)`)
    /// is how the corpus's private members are most often declared —
    /// `destructuringUnspreadableIntoRest` declares all five that way.
    fn member_declaration_has_modifier(&self, id: NodeId, keyword: SyntaxKind) -> bool {
        let modifiers = match self.node_map.get(id) {
            Some(Node::PropertyDeclaration(node)) => node.modifiers,
            Some(Node::MethodDeclaration(node)) => node.modifiers,
            Some(Node::GetAccessorDeclaration(node)) => node.modifiers,
            Some(Node::SetAccessorDeclaration(node)) => node.modifiers,
            Some(Node::ParameterDeclaration(node)) => node.modifiers,
            _ => return false,
        };
        crate::check::has_modifier(modifiers, keyword)
    }

    /// `isSpreadableProperty` (`checker.go:17830`) together with
    /// `getRestType`'s own private/protected test (`:17808`). §776.
    ///
    /// A property is spreadable into a rest element unless it is declared with
    /// a private identifier, or is a METHOD/ACCESSOR declared in a class, or
    /// carries `private` or `protected`. Upstream keeps the last of those at
    /// the call site rather than in `isSpreadableProperty`; the two are joined
    /// here because every caller wants both and separating them invites a
    /// caller that checks one.
    fn is_spreadable_property(&self, symbol: tsr_binder::SymbolId) -> bool {
        let data = self.binder.symbols().get(symbol);
        let declarations: Vec<NodeId> = data.declarations.iter().copied().collect();
        let flags = data.flags;
        for &declaration in &declarations {
            if self.member_declaration_has_modifier(declaration, SyntaxKind::PrivateKeyword)
                || self.member_declaration_has_modifier(declaration, SyntaxKind::ProtectedKeyword)
            {
                return false;
            }
        }
        if !flags.intersects(
            tsr_binder::SymbolFlags::METHOD
                | tsr_binder::SymbolFlags::GET_ACCESSOR
                | tsr_binder::SymbolFlags::SET_ACCESSOR,
        ) {
            return true;
        }
        // `!some(declarations, d => IsClassLike(d.Parent))` — a method on an
        // OBJECT TYPE spreads; a method on a CLASS does not.
        !declarations.iter().any(|&declaration| {
            self.nodes.parent(declaration).is_some_and(|parent| {
                matches!(
                    self.nodes.kind(parent),
                    SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
                )
            })
        })
    }

    fn object_rest_type(
        &mut self,
        parent_type: TypeId,
        pattern_id: NodeId,
        declaration: NodeId,
    ) -> TypeId {
        let error = self.intrinsics.error;
        let Some(Node::BindingPattern(pattern)) = self.node_map.get(pattern_id) else {
            return error;
        };
        let mut bound: Vec<String> = Vec::new();
        for sibling in pattern.elements {
            if sibling.node_id == Some(declaration) {
                continue;
            }
            let Some((name, _)) = Self::binding_element_property_name(sibling) else {
                return error;
            };
            bound.push(name);
        }
        // §776 (`checker.go:17813`-`:17827`): a GENERIC source cannot be spread
        // into a member list — upstream mints `Omit<source, omitKeyType>`
        // through the global `Omit` alias, and the baselines print exactly that
        // (`genericObjectRest` wants `Omit<T, "a">`).
        //
        // `isGenericObjectType` is reduced to a bare TYPE PARAMETER, the shape
        // the corpus's generic rests have; a mapped or indexed-access source
        // keeps the gap. The `isGenericIndexType(omitKeyType)` half is not
        // ported: a computed key declines above, at
        // `binding_element_property_name`.
        if self.store.get(parent_type).flags.intersects(TypeFlags::TYPE_PARAMETER) {
            let mut keys: Vec<TypeId> = Vec::new();
            let push_key = |checker: &mut Self, name: &str, keys: &mut Vec<TypeId>| {
                let key = checker.store.intern_literal(
                    TypeFlags::STRING_LITERAL,
                    TypeData::StringLiteral(name.to_string()),
                    false,
                );
                if !keys.contains(&key) {
                    keys.push(key);
                }
            };
            // `getUnionType(map(properties, getLiteralTypeFromPropertyName))`
            // (`:17802`) — the names BOUND by the sibling elements.
            for name in &bound {
                push_key(self, name, &mut keys);
            }
            // `unspreadableToRestKeys` (`:17806`-`:17818`): every property that
            // CANNOT be spread is omitted too — a method or accessor declared
            // in a class, and a `private` or `protected` member. §775 measured
            // what happens without this: `destructuringUnspreadableIntoRest`
            // went 30 RIGHT→WRONG, because an omit list missing them is a
            // confidently wrong type where the gap was honest.
            let apparent = self.apparent_type(parent_type);
            if let TypeData::Named { members: Some(owner), .. } = self.store.get(apparent).data {
                let properties: Vec<(String, tsr_binder::SymbolId)> = self
                    .binder
                    .symbols()
                    .get(owner)
                    .members
                    .iter()
                    .map(|(name, &symbol)| ((*name).to_string(), symbol))
                    .collect();
                let mut unspreadable: Vec<String> = properties
                    .into_iter()
                    .filter(|&(_, symbol)| !self.is_spreadable_property(symbol))
                    .map(|(name, _)| name)
                    .collect();
                unspreadable.sort();
                for name in &unspreadable {
                    push_key(self, name, &mut keys);
                }
            }
            if keys.is_empty() {
                // `omitKeyType.flags & Never` (`:17820`) — `let { ...r } = obj`
                // over a source with nothing to omit IS the source. Upstream
                // tests this BEFORE `getGlobalOmitSymbol` (`:17823`), so the
                // answer does not depend on the lib being present; the order is
                // kept because it is observable in a lib-less program.
                return parent_type;
            }
            let Some(omit) = self.global_type_symbol_with_arity("Omit", 2) else {
                return error;
            };
            let omit_key = self.get_union_type(&keys);
            return self.create_type_reference(omit, vec![parent_type, omit_key]);
        }
        let Some(members) = self.spread_members_of(parent_type) else {
            return error;
        };
        let remaining: Vec<crate::objects::Member> = members
            .into_iter()
            .filter(|member| match member {
                crate::objects::Member::Property { name, .. } => {
                    !bound.iter().any(|bound_name| bound_name == name)
                }
                _ => true,
            })
            .collect();
        let printed = crate::objects::render_object_type(&remaining);
        self.store.new_named(TypeFlags::OBJECT, printed, None)
    }

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
        // §565: a PATTERN-IMPLIED object type has no symbol and therefore no
        // members table, so the lookup above cannot see the members its own
        // printed form shows. Every such member is `any` by construction
        // (`getTypeFromObjectBindingPattern`, `checker.go:17938`, with no
        // initializer to infer from), so the name being present IS the answer.
        // A name the pattern does not declare keeps the gap.
        if self
            .pattern_implied_members
            .get(&parent_type)
            .is_some_and(|names| names.iter().any(|declared| declared == name))
        {
            return self.intrinsics.any;
        }
        // §303: the APPARENT-type hop the plain member road already takes —
        // `var { toExponential } = 0` reads `Number`'s member exactly as
        // `(0).toExponential` does (`destructuringWithNumberLiteral`). A
        // primitive carries no members of its own; its interface does.
        // A type parameter whose constraint is a UNION is excluded: those
        // destructured members belong to the NARROWING road
        // (`f<T extends A | B>` narrows by discriminant), and reading the raw
        // union here turned two of `narrowingDestructuring`'s gaps into
        // confident `any`s. A NON-union constraint has nothing to narrow and
        // reads straight (`genericObjectRest`,
        // `dependentDestructuredVariables` — 12 lines the blanket exclusion
        // of the second draft forfeited).
        let apparent = self.apparent_type(parent_type);
        if self.store.get(parent_type).flags.intersects(TypeFlags::TYPE_PARAMETER)
            && matches!(self.store.get(apparent).data, TypeData::Union { .. })
        {
            return self.intrinsics.error;
        }
        if apparent != parent_type
            && let Some(property_type) = self.get_type_of_property_of_type(apparent, name)
        {
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
