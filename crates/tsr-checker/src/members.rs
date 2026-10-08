//! Looking inside an object type: property access, `getPropertyOfType`, and
//! inherited members.
//!
//! Ported from `checker.go:11244`, `:11258`, `getPropertyOfTypeEx`
//! (`checker.go:18899`) and `getPropertyOfObjectType` (`:21403`).

use tsr_ast::Node;
use tsr_binder::{SymbolFlags, SymbolId};

use crate::{
    checker::Checker,
    flags::TypeFlags,
    types::{TypeData, TypeId},
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TypeParameterConstraintKey {
    parameter: TypeId,
    bindings: Vec<(SymbolId, TypeId)>,
}

/// Which symbol table a property lookup should read, decided by the type's
/// shape before any `&mut self` call borrows the store back.
///
/// The two arms are upstream's two branches of `resolveAnonymousTypeMembers` /
/// `resolveDeclaredMembers`, not a convenience: they read *different tables* of
/// the same symbol, and conflating them is the wrong-answer case
/// `crate::symbols` documents.
#[derive(Clone, Copy)]
enum Owner {
    /// The instance side: read `members`, then walk base types.
    Declared(SymbolId),
    /// The `typeof X` side: read `exports`.
    Anonymous(SymbolId),
}

/// The property `createUnionOrIntersectionProperty` (`checker.go:21452`)
/// answers for a union or intersection receiver.
///
/// # No synthetic symbol
///
/// Upstream answers `singleProp` itself when every constituent holds the same
/// property (or instantiations of it, which share one [`SymbolId`] here),
/// and otherwise mints a transient synthetic symbol carrying the combined
/// `CheckFlags`, type and write type. This port mints no synthetic symbols
/// (`get_property_of_type` answers a binder [`SymbolId`]), so for a minted
/// one `get_property_of_type` answers `single`, the first constituent's
/// property, as its stand-in: presence is upstream's exactly, and the
/// synthetic's own facts are read off [`SyntheticProperty`] by the
/// receiver-and-name readers instead of the stand-in —
/// `get_type_of_property_with_this_argument` (the union projection and
/// `property_type_via_shape`), `is_readonly_property_of_type` and
/// `write_type_of_property_of_type`. Recomputed per ask, never published.
struct UnionOrIntersectionProperty {
    /// `singleProp`.
    single: SymbolId,
    /// What upstream's minted symbol records, when it mints one.
    synthetic: Option<SyntheticProperty>,
}

/// The facts of `createUnionOrIntersectionProperty`'s minted symbol.
struct SyntheticProperty {
    /// `links.containingType`.
    containing_type: TypeId,
    is_union: bool,
    /// `propSet` in constituent order, each with the apparent constituent
    /// it was found in.
    properties: Vec<(SymbolId, TypeId)>,
    /// A union constituent lacking the name answered by a readonly index
    /// signature (part of `CheckFlagsReadonly`; the properties' part is read
    /// per ask).
    index_readonly: bool,
}

impl Checker<'_, '_> {
    /// Ported from `Checker.checkPropertyAccessExpression` into
    /// `checkPropertyAccessExpressionOrQualifiedName` (`checker.go:11244`,
    /// `:11258`), reduced to the lookup.
    ///
    /// Upstream takes the receiver's **apparent** type first, which is what makes
    /// `"a".length` work: a primitive's apparent type is its wrapper interface
    /// from `lib.d.ts`. That is [`Checker::apparent_type`], and it is now taken
    /// here.
    ///
    /// > **This comment used to say "there are no lib files (`bd tsr-9or.1`), so
    /// > a primitive receiver has no members here"**, and that has been false
    /// > since the program started loading `internal/bundled/libs`. The gap it
    /// > described was being attributed to lib rather than to this function, and
    /// > it was this function's. Corrected rather than deleted, because
    /// > `docs/conventions.md` records four stale comments outliving their truth
    /// > in one session and this was a fifth.
    ///
    /// Not ported: `super` and index signatures. Both answer `errorType`.
    ///
    /// **A private name is not a special lookup** (`bd tsr-0opd`). Upstream
    /// reaches `this.#x` through the same `getPropertyOfType`
    /// (`checker.go:11258`) as any other member, because a private field's
    /// symbol is filed under its own text — and `PrivateIdentifier.text`
    /// carries the leading `#`, so `#x` is simply a member name that cannot be
    /// written as a dotted identifier. This port's binder already files it
    /// that way (`crates/tsr-binder/src/binder.rs:3995`), so the arm is the
    /// name extraction below and nothing else.
    ///
    /// The *scope* rule upstream enforces separately —
    /// `lookupSymbolForPrivateIdentifierDeclaration`, which reports when a
    /// private name is used outside its declaring class — is a **diagnostic**
    /// and does not change the type answer (ADR-0040's distinction). What it
    /// would reject, the lookup here misses anyway: a `#x` that is not a
    /// member of the receiver's type answers `errorType`, which is upstream's
    /// type answer too.
    pub fn check_property_access_expression(
        &mut self,
        node: &tsr_ast::PropertyAccessExpression<'_>,
    ) -> TypeId {
        // Direct public reads (including the property's name in a baseline)
        // share the expression's cached result. Rechecking an access after
        // its call exhausted the instantiation budget must not change it.
        if let Some(id) = node.node_id
            && let Some(&cached) = self.node_types.get(&id)
        {
            return cached;
        }
        self.instantiation_count = 0;
        // §944: a property access that is the TARGET of an assignment and names
        // a READONLY property answers `any`.
        //
        // Upstream reports "cannot assign to a read-only property" and the
        // erroneous reference answers `errorType`, which the producer prints as
        // `any`: `bigintWithLib` records `>bigIntArray.length : any` for
        // `bigIntArray.length = 10`. §933 met these rows as 4 `RIGHT->WRONG` and
        // named the diagnostic as their reopening condition — they were right
        // by coincidence before, because the receiver did not resolve at all.
        //
        // The predicate is already here: `property_signature_is_readonly` backs
        // `check_readonly_assignment_target`, the DIAGNOSTIC road. The type road
        // never consulted it — the eighteenth instance this session of a
        // capability present and a caller that does not reach for it.
        if let Some(id) = node.node_id
            && self.is_readonly_assignment_target(id)
        {
            return self.intrinsics.any;
        }
        let computed = self.check_property_access_expression_worker(node);
        // §55.1 (`checker-notes-narrow.md`): a single-member enum's ACCESS
        // prints the enum spelling while its declaration line keeps the
        // per-name fresh form (`Enum.A : Enum` beside `>A : Enum.A`).
        if let Some(&spelled) = self.enum_access_spelling.get(&computed) {
            return spelled;
        }
        computed
    }

    /// SS186: does the class body lexically containing `node` declare the
    /// private name `name`? Upstream's `lookupSymbolForPrivateIdentifier
    /// Declaration` walks containing classes; a name found on an ANCESTOR
    /// class is not accessible, which is what distinguishes this from a
    /// members lookup.
    /// §471 sharpened SS186's bool into the INNERMOST declaring class,
    /// because the shadow rule needs to know WHICH class's `#foo` the site
    /// sees, not merely that one exists: upstream's
    /// `lookupSymbolForPrivateIdentifierDeclaration` returns the first
    /// class's member walking OUTWARD, and the type-side lookup then runs on
    /// that symbol's per-class MANGLED name
    /// (`binder.GetSymbolNameForPrivateIdentifier`), so a same-spelled
    /// private on any other class can never match.
    pub(crate) fn lexical_private_declaring_class(
        &self,
        node: tsr_ast::NodeId,
        name: &str,
    ) -> Option<tsr_ast::NodeId> {
        // SS190 (measured +0, reverted): the decorator exclusion —
        // `getContainingClassExcludingClassDecorators`
        // (utilities.go:994-1011) starts the walk ABOVE a class when the
        // private name is written inside that class's own DECORATOR, so
        // `@dec(this.#x) class C { #x }` must not resolve. Transcribed and
        // measured: zero cases, no corpus fixture writes a private name in a
        // decorator position. Reverted under the unexercised-branch rule;
        // this is the one place SS186's walk is knowingly wrong, and it is
        // recorded here rather than left implicit.
        let mut current = self.nodes.parent(node);
        while let Some(id) = current {
            let members = match self.node_map.get(id) {
                Some(Node::ClassDeclaration(class)) => Some(class.members),
                Some(Node::ClassExpression(class)) => Some(class.members),
                _ => None,
            };
            if let Some(members) = members {
                let declares = members.iter().any(|member| {
                    let member_name = match member {
                        tsr_ast::ClassElement::PropertyDeclaration(p) => Some(p.name),
                        tsr_ast::ClassElement::MethodDeclaration(m) => Some(m.name),
                        tsr_ast::ClassElement::GetAccessorDeclaration(a) => Some(a.name),
                        tsr_ast::ClassElement::SetAccessorDeclaration(a) => Some(a.name),
                        _ => None,
                    };
                    matches!(member_name, Some(tsr_ast::PropertyName::PrivateIdentifier(p))
                        if p.text == name)
                });
                if declares {
                    return Some(id);
                }
            }
            current = self.nodes.parent(id);
        }
        None
    }

    /// §471: whether a member declaration's NAME is a private identifier —
    /// the test that separates a declared `#foo` from a computed `["#foo"]`,
    /// which spell the same property text in this port's by-text member
    /// tables and are different names upstream (mangled vs. plain).
    pub(crate) fn declaration_names_a_private(&self, declaration: tsr_ast::NodeId) -> bool {
        let member_name = match self.node_map.get(declaration) {
            Some(Node::PropertyDeclaration(p)) => Some(p.name),
            Some(Node::MethodDeclaration(m)) => Some(m.name),
            Some(Node::GetAccessorDeclaration(a)) => Some(a.name),
            Some(Node::SetAccessorDeclaration(a)) => Some(a.name),
            _ => None,
        };
        matches!(member_name, Some(tsr_ast::PropertyName::PrivateIdentifier(_)))
    }

    fn check_property_access_expression_worker(
        &mut self,
        node: &tsr_ast::PropertyAccessExpression<'_>,
    ) -> TypeId {
        let error = self.intrinsics.error;
        let (Some(receiver), Some(member)) = (node.expression, node.name) else {
            return error;
        };
        let name = match member {
            tsr_ast::MemberName::Identifier(name) => name.text,
            tsr_ast::MemberName::PrivateIdentifier(name) => name.text,
        };
        // SS186: a PRIVATE name is lexically scoped to the class body that
        // DECLARES it (`lookupSymbolForPrivateIdentifierDeclaration`,
        // checker.go:11475). The doc comment above claimed the lookup
        // "misses anyway" what that rule would reject — the FOURTH
        // unfalsified redundancy claim this session, and false here: in
        // `class Derived extends Base`, `#prop` IS a member of `Derived`'s
        // type by inheritance, so the lookup succeeds where upstream
        // rejects. The oracle records the same `x.#prop` as `number` inside
        // `Base` and `any` (its errorType) inside `Derived`
        // (`privateNameFieldDerivedClasses`).
        let mut lexical_private_class = None;
        if let tsr_ast::MemberName::PrivateIdentifier(_) = member
            && let Some(access_id) = node.node_id
        {
            lexical_private_class = self.lexical_private_declaring_class(access_id, name);
            if lexical_private_class.is_none() {
                return error;
            }
        }
        let receiver_type = self.check_expression(receiver);
        if receiver_type == error {
            return error;
        }
        // `checkPropertyAccessExpression` hands `checkNonNullExpression(expr)`
        // to the lookup (`checker.go:11258`), and a chain link goes through
        // `checkPropertyAccessChain` (`checker.go:11253`) — the receiver is
        // stripped of `null`/`undefined` (the "possibly undefined" report is a
        // *diagnostic*, a channel this port does not have — ADR-0040), the
        // lookup and flow narrowing run on the remainder, and
        // `propagateOptionalTypeMarker` (`checker.go:29082`) unions
        // `undefined` back in when `?.` stripped anything.
        // `docs/architecture/checker-notes-nnaccess.md`.
        let non_optional = self.get_optional_expression_type(
            receiver_type,
            receiver.node_id(),
            node.question_dot_token.is_some(),
        );
        let mut stripped = self.check_non_null_type(non_optional);
        if stripped == error {
            // §121 (`checker-notes-narrow.md`): the receiver COMPUTED (the
            // gap test above passed) and the non-null strip itself refused.
            // Upstream reports (TS2532/TS18048/TS18047) and returns
            // `errorType`, printed `any`: the deliberate error-answer, the
            // boundary-argument chain's seventh hop. GATED to receivers that
            // are PURELY nullish — the ungated arm measured 64:36 with the
            // adverse concentrated in `unknown` receivers this port fails to
            // narrow (catch variables, assertion predicates); those keep the
            // honest gap.
            if self.receiver_is_purely_nullish(non_optional) {
                // §14.1's split: a JS baseline prints upstream's errorType
                // verbatim (`jsFileClassSelfReferencedProperty`'s
                // `this.testStackOverflow.bind : error`).
                if node.node_id.is_some_and(|id| self.in_js_file(id)) {
                    return error;
                }
                return self.intrinsics.any;
            }
            return error;
        }
        let widen_receiver = node.node_id.is_some_and(|access| {
            if self.assignment_target_kind(access) != crate::expressions::AssignmentTargetKind::None
            {
                return true;
            }
            let mut current = access;
            while let Some(parent) = self.nodes.parent(current) {
                match self.node_map.get(parent) {
                    Some(Node::ParenthesizedExpression(_)) => current = parent,
                    Some(Node::CallExpression(call)) => {
                        return call.expression.and_then(|expr| expr.node_id()) == Some(current);
                    }
                    Some(Node::NewExpression(call)) => {
                        return call.expression.and_then(|expr| expr.node_id()) == Some(current);
                    }
                    _ => return false,
                }
            }
            false
        });
        if widen_receiver {
            stripped = self.widen_object_literal_freshness(stripped);
        }
        // §471 the shadow half of upstream's mangled-name lookup: the
        // property the receiver's type serves under this spelling must be
        // THE lexical class's member — `getPrivateIdentifierPropertyOfType`
        // asks for `lexicallyScopedSymbol.Name`, the per-class mangled name
        // (`checker.go:11296`), so B's `#foo` seen from inside A can never
        // resolve A's-typed receiver's `#foo`, and
        // `checkPrivateIdentifierPropertyAccess` reports shadowing and
        // answers `errorType` (`:11300`), printed `any`
        // (`privateNamesInNestedClasses-1/-2`,
        // `privateNameNestedClassAccessorsShadowing`,
        // `privateNamesAndStaticFields`). Inheritance survives by symbol
        // identity: `Derived`'s type serves BASE's member symbol for a base
        // private, and that symbol's declaration sits in the lexical class
        // (`privateNameFieldDerivedClasses`).
        if let Some(lexical) = lexical_private_class
            && let Some(property) = self.get_property_of_type(stripped, name)
        {
            let record = self.binder.symbols().get(property);
            let declared_in_lexical = record
                .value_declaration
                .and_then(|declaration| self.containing_class_of(declaration))
                == Some(lexical);
            if !declared_in_lexical {
                return self.intrinsics.any;
            }
        }
        // isAssignmentToReadonlyEntity's namespace-import branch (5b1047d
        // checker.go:27279). Reuse alias/declaration identity, not module shape;
        // the existing readonly diagnostic owner reports TS2540 separately.
        if node.node_id.is_some_and(|id| {
            self.assignment_target_kind(id) != crate::expressions::AssignmentTargetKind::None
        }) && self.receiver_alias_is_namespace_import(receiver) == Some(true)
            && self.get_property_of_type(stripped, name).is_some()
        {
            return self.intrinsics.any;
        }
        // `isAssignmentToReadonlyEntity` (`checker.go:11377`): a readonly
        // property as an assignment target answers upstream's `errorType`,
        // printed `any` (`checker-notes-narrow.md` §27).
        if node
            .node_id
            .is_some_and(|id| {
                self.assignment_target_kind(id)
                    != crate::expressions::AssignmentTargetKind::None
            })
            // Upstream's constructor exception: `this.x = …` inside a
            // constructor assigns a readonly property legally
            // (`isAssignmentToReadonlyEntity`'s same-class carve-out —
            // approximated as this-receiver-in-constructor, the §27 bar's
            // fired leg). §471: the carve-out is for readonly FIELDS only —
            // upstream reaches it through the `CheckFlagsReadonly`/
            // `readonly`-modifier arms, and a GET-ONLY ACCESSOR write is
            // illegal even there (`isReadonlySymbol`'s accessor arm has no
            // constructor escape; `privateNameAccessors` wants
            // `this.#roProp : any` inside the constructor).
            && !(matches!(
                receiver,
                tsr_ast::Expression::KeywordExpression(keyword)
                    if keyword.kind == tsr_ast::SyntaxKind::ThisKeyword
            ) && node.node_id.is_some_and(|id| {
                self.control_flow_container(id).is_some_and(|container| {
                    self.nodes.kind(container) == tsr_ast::SyntaxKind::Constructor
                })
            }) && !self.get_property_of_type(stripped, name).is_some_and(|property| {
                let flags = self.binder.symbols().get(property).flags;
                flags.intersects(tsr_binder::SymbolFlags::GET_ACCESSOR)
                    && !flags.intersects(tsr_binder::SymbolFlags::SET_ACCESSOR)
            }))
            && self.is_readonly_property_of_type(stripped, name)
        {
            return self.intrinsics.any;
        }
        // `isThisPropertyAccessInConstructor` (`checker.go:27328`): inside the
        // constructor that declares a this-property, the property reads as
        // `autoType` and `getFlowTypeOfAccessExpression` follows its flow.
        if let Some(access) = node.node_id
            && self.in_js_file(access)
            && let Some(constructor) = self.get_this_container(access, true)
            && self.nodes.kind(constructor) == tsr_ast::SyntaxKind::Constructor
            && let Some(property) = self.get_property_of_type(stripped, name)
            && matches!(
                self.is_constructor_declared_this_property(property),
                crate::assignment_declarations::ThisAssignmentDeclaration::Constructor(declaring)
                    if declaring == constructor
            )
        {
            if self.assignment_target_kind(access)
                == crate::expressions::AssignmentTargetKind::Definite
            {
                return self.intrinsics.any;
            }
            return self.get_flow_type_of_property_symbol(access, None, property);
        }
        let result = self.access_member_lookup(stripped, name, node.node_id);
        // checkPropertyAccessExpressionOrQualifiedName (`checker.go:11334`): a
        // miss on a JS literal receiver (`isJSLiteralType`) answers `anyType`.
        if result == error
            && self.get_property_of_type(stripped, name).is_none()
            && self.is_js_literal_type(receiver_type)
        {
            return self.intrinsics.any;
        }
        if result == error {
            return error;
        }
        // §78 (`checker-notes-narrow.md`): `getWriteTypeOfSymbol` under
        // `exactOptionalPropertyTypes` — a WRITE position removes
        // `missingType`, so `obj.a = 'hello'` prints `string` while the read
        // keeps `string | undefined` (`strictOptionalProperties1`).
        // §616: the DIVERGENT-ACCESSOR half of `getWriteTypeOfSymbol`, beside
        // the `exactOptionalPropertyTypes` half below. A write to a property
        // whose setter is annotated sees the SETTER's parameter type
        // (`getWriteTypeOfAccessors`, `checker.go:16447`), which differs from
        // the read type only for a divergent pair — `get x(): string` beside
        // `set x(v: string | number | boolean)`.
        let is_definite_write = node.node_id.is_some_and(|id| {
            self.assignment_target_kind(id) == crate::expressions::AssignmentTargetKind::Definite
        });
        let result = if is_definite_write
            && let Some(written) = self.write_type_of_property_of_type(stripped, name)
        {
            written
        } else {
            result
        };
        let result = if self.exact_optional_property_types && is_definite_write {
            self.remove_missing_type(result)
        } else {
            result
        };
        self.propagate_optional_type_marker_at(node.node_id, result, non_optional != receiver_type)
    }

    /// The §45 `Record<string, V>` read: `Some(V)` only for a reference to
    /// the GLOBAL `Record` with a plain-`string` key.
    fn record_string_value(&mut self, receiver: TypeId) -> Option<TypeId> {
        let (target, arguments) = self.type_reference_targets.get(&receiver)?.clone();
        if arguments.len() != 2 || arguments[0] != self.intrinsics.string {
            return None;
        }
        let record = self.binder.global("Record")?;
        (self.binder.merged_symbol(target) == self.binder.merged_symbol(record))
            .then_some(arguments[1])
    }

    /// `checkNonNullType` (`checker.go:7409`), without the diagnostics: an
    /// `unknown` receiver is `errorType` in strict mode; a nullable one is
    /// answered by its non-nullable remainder; a remainder that is itself
    /// nullable or `never` refuses.
    pub(crate) fn check_non_null_type(&mut self, id: TypeId) -> TypeId {
        let error = self.intrinsics.error;
        if self.strict_null_checks && self.store.get(id).flags.intersects(TypeFlags::UNKNOWN) {
            return error;
        }
        if !self
            .get_type_facts(id)
            .intersects(crate::flow::TypeFacts::IS_UNDEFINED | crate::flow::TypeFacts::IS_NULL)
        {
            return id;
        }
        let non_nullable = self.get_non_nullable_type(id);
        if non_nullable == id {
            return id;
        }
        let flags = self.store.get(non_nullable).flags;
        if flags.intersects(crate::flags::TypeFlags::NULLABLE.union(crate::flags::TypeFlags::NEVER))
        {
            return error;
        }
        non_nullable
    }

    /// §121's gate: the receiver is `undefined`, `null`, or a union of only
    /// those — every constituent carries `TypeFlags::NULLABLE`. `unknown`
    /// and mixed remainders are excluded: the ungated arm measured its
    /// adverse there (receivers this port fails to narrow).
    fn receiver_is_purely_nullish(&mut self, id: TypeId) -> bool {
        let non_nullable = self.get_non_nullable_type(id);
        let original = self.store.get(id).flags;
        original.intersects(crate::flags::TypeFlags::NULLABLE.union(crate::flags::TypeFlags::UNION))
            && self.store.get(non_nullable).flags.intersects(crate::flags::TypeFlags::NEVER)
            && !original.intersects(crate::flags::TypeFlags::UNKNOWN)
    }

    /// `GetNonNullableType` (`checker.go:18663`): adjust non-null facts in
    /// strict mode, including semantic intersections for type variables.
    pub(crate) fn get_non_nullable_type(&mut self, id: TypeId) -> TypeId {
        if self.strict_null_checks {
            self.get_adjusted_type_with_facts(id, crate::flow::TypeFacts::NE_UNDEFINED_OR_NULL)
        } else {
            id
        }
    }

    /// `getOptionalExpressionType` (`checker.go:29064`): a chain **root**
    /// strips nullable outright; an inner link removes the propagated marker.
    ///
    /// Upstream's marker is `optionalType`, an `undefined` distinct from the
    /// real one; this port has one `undefined`, and the divergence is owned in
    /// `checker-notes-nnaccess.md` §2 — the type answers coincide because
    /// upstream's own `checkNonNullType` strips a genuine `undefined` on the
    /// same path.
    pub(crate) fn get_optional_expression_type(
        &mut self,
        expression_type: TypeId,
        receiver: Option<tsr_ast::NodeId>,
        node_is_chain_root: bool,
    ) -> TypeId {
        if node_is_chain_root {
            return self.get_non_nullable_type(expression_type);
        }
        if let Some(receiver) = receiver.filter(|&id| self.expression_is_optional_chain(id)) {
            // `removeOptionalTypeMarker` (`checker.go:29073`), done by identity:
            // the receiver link's own pre-union type is precisely what the
            // marker was added to. Filtering `undefined` by flag instead would
            // also take away a genuine one — `this?.a.#b` with `a?: A`, which
            // upstream reports on. §5 of `checker-notes-nnaccess.md`.
            return self.pre_optional_marker.get(&receiver).copied().unwrap_or(expression_type);
        }
        expression_type
    }

    /// Remember an optional-chain link's type before the marker joins it, so
    /// the next link can subtract exactly that. §5 of `checker-notes-nnaccess.md`.
    pub(crate) fn propagate_optional_type_marker_at(
        &mut self,
        node: Option<tsr_ast::NodeId>,
        id: TypeId,
        was_optional: bool,
    ) -> TypeId {
        if was_optional && let Some(node) = node {
            self.pre_optional_marker.insert(node, id);
        }
        self.propagate_optional_type_marker(id, was_optional)
    }

    /// `propagateOptionalTypeMarker` (`checker.go:29082`): when the chain
    /// stripped anything, `undefined` joins the result. Upstream distinguishes
    /// the outermost link (`getOptionalType`, a real `undefined`) from an
    /// inner one (the marker); with one `undefined` the two are the same
    /// union.
    pub(crate) fn propagate_optional_type_marker(
        &mut self,
        id: TypeId,
        was_optional: bool,
    ) -> TypeId {
        if !was_optional {
            return id;
        }
        let undefined = self.intrinsics.undefined;
        self.get_union_type(&[id, undefined])
    }

    /// Whether this expression is a link of an optional chain: it, or an
    /// access/call/non-null assertion on its receiver spine, carries `?.`.
    /// Upstream stores this as `NodeFlagsOptionalChain`, stamped by the
    /// parser; this port derives it by walking the spine, which a parenthesis
    /// deliberately breaks — `(a?.b).c` is not a chain link, exactly as
    /// upstream's flag propagation stops at the parenthesis.
    pub(crate) fn expression_is_optional_chain(&self, id: tsr_ast::NodeId) -> bool {
        let mut current = id;
        loop {
            let next = match self.node_map.get(current) {
                Some(Node::PropertyAccessExpression(access)) => {
                    if access.question_dot_token.is_some() {
                        return true;
                    }
                    access.expression.and_then(|e| e.node_id())
                }
                Some(Node::ElementAccessExpression(access)) => {
                    if access.question_dot_token.is_some() {
                        return true;
                    }
                    access.expression.and_then(|e| e.node_id())
                }
                Some(Node::CallExpression(call)) => {
                    if call.question_dot_token.is_some() {
                        return true;
                    }
                    call.expression.and_then(|e| e.node_id())
                }
                Some(Node::NonNullExpression(assertion)) => {
                    assertion.expression.and_then(|e| e.node_id())
                }
                _ => return false,
            };
            let Some(next) = next else { return false };
            current = next;
        }
    }

    /// Whether a `NonNullExpression` carries `NodeFlagsOptionalChain` — the
    /// predicate `checkNonNullAssertion` (`checker.go:10623`) branches on. §884.
    ///
    /// **This is not "the operand spine contains `?.`".** The parser builds every
    /// `!` with `ast.NodeFlagsNone` (`parser.go:5375`) and the flag is added
    /// *retroactively*, by `tryReparseOptionalChain` (`parser.go:5414`), which is
    /// called from exactly three places — the property-access, element-access and
    /// call rests (`parser.go:5399`, `5444`, `5468`). A `!` is therefore a chain
    /// link only when **another link was parsed on top of it**:
    ///
    /// ```ts
    /// o2?.["b"]!.c   // the `!` IS a chain link — `.c` reparsed it
    /// o2?.["b"]!.c!  // the trailing `!` is NOT — nothing follows it
    /// m?.[0]! && …   // not a link either; `&&` is not one of the three rests
    /// ```
    ///
    /// That asymmetry is the whole observable difference: an inner `!` re-unions
    /// the chain's `undefined` and the outermost `!` removes it. Deriving the flag
    /// from the operand alone marks the trailing `!` as a link too and answers
    /// `string | undefined` where upstream answers `string` — measured as six
    /// `RIGHT->WRONG` on the first build of §884.
    ///
    /// The upward walk climbs a *run* of `!`s because `tryReparseOptionalChain`
    /// stamps the whole run in one go (`o2?.b!!.c`).
    pub(crate) fn non_null_is_optional_chain(&self, node: tsr_ast::NodeId) -> bool {
        // Downward: `expr := node.Expression(); for IsNonNullExpression(expr) …`
        // — the port derives the operand's flag rather than storing it, and the
        // spine walker already passes through `!` links.
        let Some(Node::NonNullExpression(assertion)) = self.node_map.get(node) else {
            return false;
        };
        let Some(operand) = assertion.expression.and_then(|e| e.node_id()) else { return false };
        if !self.expression_is_optional_chain(operand) {
            return false;
        }
        // Upward: did one of the three rests reparse this run?
        let mut current = node;
        loop {
            let Some(parent) = self.nodes.parent(current) else { return false };
            if matches!(self.node_map.get(parent), Some(Node::NonNullExpression(_))) {
                current = parent;
                continue;
            }
            let continues = match self.node_map.get(parent) {
                Some(Node::PropertyAccessExpression(access)) => access.expression,
                Some(Node::ElementAccessExpression(access)) => access.expression,
                Some(Node::CallExpression(call)) => call.expression,
                _ => return false,
            };
            return continues.and_then(|e| e.node_id()) == Some(current);
        }
    }

    /// Ported from `Checker.checkQualifiedName` (`checker.go:8122`), which is
    /// one call into the same `checkPropertyAccessExpressionOrQualifiedName`
    /// tail as a property access — the reason the lookup below is shared
    /// rather than duplicated. Reached from `typeof A.B` in type position
    /// (`bd tsr-4sc.10`); an expression-position qualified name does not exist
    /// in the grammar outside import assignments.
    ///
    /// `typeof this.x` is refused whole-construct: upstream routes a `this`
    /// left through `checkThisExpression` plus `checkNonNullType`
    /// (`checker.go:8125`), a per-container answer this port only partially
    /// has. `docs/architecture/checker-notes-tquery.md` §4 owns the refusal.
    pub fn check_qualified_name(&mut self, node: &tsr_ast::QualifiedName<'_>) -> TypeId {
        let error = self.intrinsics.error;
        let (Some(left), Some(right)) = (node.left, node.right) else {
            return error;
        };
        let left_type = match left {
            // §268. A `this`-headed qualified name checks `this` as an
            // EXPRESSION rather than resolving it as a name.
            //
            //     // @noImplicitThis: false
            //     function Test1() { let x: typeof this.no = 1 }
            //     >this.no : any
            //     >x : any
            //
            // Witness `conformance/typeofThisWithImplicitThis`. `this` is not a
            // name, so `resolve_name` finds nothing and this arm answered the
            // error type; `check_this_expression` already answers `any` for a
            // plain function in every mode (`tryGetThisTypeAtEx`'s
            // fallthrough), and `.no` on `any` is `any` — upstream's route.
            //
            // The sibling arm in `get_type_from_type_query_node`
            // (`declared.rs`) stays as it is and is NOT this bug: that one is
            // upstream's `isThisIdentifier` dispatch for a BARE `typeof this`,
            // it is correct, and it is the arm anyone reading this diagnosis
            // would go to first.
            tsr_ast::EntityName::Identifier(identifier) if identifier.text == "this" => {
                match identifier.node_id {
                    Some(id) => self.check_this_expression(id),
                    None => return error,
                }
            }
            tsr_ast::EntityName::Identifier(identifier) => {
                self.check_expression(tsr_ast::Expression::Identifier(identifier))
            }
            tsr_ast::EntityName::QualifiedName(inner) => self.check_qualified_name(inner),
        };
        // `checkQualifiedName` routes the left through `checkNonNullExpression`
        // (`checker.go:8127`) exactly as a property access does its receiver.
        if left_type == error {
            return error;
        }
        let stripped = self.check_non_null_type(left_type);
        if stripped == error {
            // §121: same gate as the property-access road — a computed,
            // purely-nullish left takes upstream's deliberate error-answer.
            if self.receiver_is_purely_nullish(left_type) {
                return self.intrinsics.any;
            }
            return error;
        }
        self.access_member_lookup(stripped, right.text, node.node_id)
    }

    /// The shared tail of `checkPropertyAccessExpressionOrQualifiedName`
    /// (`checker.go:11244`): apparent type, the `any` fast path, the member
    /// lookup, and flow narrowing keyed on the access node itself.
    fn access_member_lookup(
        &mut self,
        receiver_type: TypeId,
        name: &str,
        node_id: Option<tsr_ast::NodeId>,
    ) -> TypeId {
        let error = self.intrinsics.error;
        // `isAnyLike` (`checker.go:11266`) and the branch it guards
        // (`checker.go:11314`): a property access on `any` is `any`, whatever
        // the property name.
        //
        // This is the other half of the same cause as element access — see
        // [`crate::indexed`]. 6,329 of 27,140 property-access lines in the
        // baselines answer `any`, against a gap row of 8,154, so an `any`
        // receiver is most of what this row is. `anyPropertyAccess.types`
        // records both spellings failing together, which is why they are ported
        // together.
        //
        // **Upstream distinguishes `errorType` from `anyType` inside this very
        // branch** — `if c.isErrorType(apparentType) { return c.errorType }`
        // (`checker.go:11318`) sits between `isAnyLike` and the return. This
        // port gets that for free by testing **identity** against
        // `intrinsics.any`: `errorType` is a different type with the same `ANY`
        // flag, so it cannot match, and a `TypeFlags::ANY` test would answer
        // `any` for every gap in the corpus. That is the single most dangerous
        // edit that could be made to this function, and it would look like a
        // large win in the measurement.
        // # This arm produces wrong lines, and they are not its fault
        //
        // **Measured: +555 wrong property-access lines when this landed**, against
        // 894 gaps closed. They are all one shape, and the owner is elsewhere.
        //
        // This port has **no contextual typing**, so an unannotated parameter is
        // the implicit `any` here where upstream infers a real type from the
        // contextual signature — `getContextuallyTypedParameterType`
        // (`checker.go:29458`), reached through `assignContextualParameterTypes`
        // (`checker.go:10349`). Probe: `const y = x => x.foo;` answers
        // `x.foo : any` here, while upstream in `arr.map(x => x.foo)` types `x`
        // from context and answers a real member type.
        //
        // So the receiver is wrong before this arm sees it, and the arm then
        // converts what used to be an honest gap into a confident claim. Those
        // lines cost no gradient and no cases — the receiver's own line was
        // already wrong, so every case containing one was already failing — but
        // they do cost **diagnostic separability**, which is the thing this
        // project's method rests on. Someone reading a property-access histogram
        // will see 555 wrong lines and cannot tell from the instrument that they
        // are contextual-typing lines rather than a defect here.
        //
        // **Closing contextual typing is what removes them.** That is recorded
        // here rather than left to be rediscovered, because a limitation naming
        // no owner is how four stale comments outlived their truth this session.
        // **The apparent type is taken before the `any` test, as upstream takes
        // it** (`checker.go:11265`, `apparentType := c.getApparentType(...)`,
        // one line above `isAnyLike`). The order is not cosmetic: upstream's
        // `isAnyLike` asks about the *apparent* type, so a receiver whose
        // apparent type is `any` takes the `any` path even when the original
        // was not. Reversing it here would answer `any` for a primitive whose
        // global interface is missing — the direction that manufactures
        // confident wrong answers.
        // §164/§165: the THIS-ARGUMENT is the ORIGINAL receiver, not the
        // apparent one — `getTypeWithThisArgument(apparentType, receiver)`
        // (`checker.go:19573` at its member call sites) reads members from
        // the apparent type while substituting `this` with what the caller
        // actually wrote. `x: T extends A` calling `x.self(): this` is
        // `T`, not `A` (thisTypeAndConstraints, the §165 first pair's four
        // R→W).
        let this_argument = receiver_type;
        let receiver_type = self.apparent_type(receiver_type);
        if receiver_type == self.intrinsics.any {
            return self.intrinsics.any;
        }
        // resolveAnonymousTypeMembers exposes only runtime global properties;
        // block-scoped bindings are lexical globals, not globalThis members.
        // A missing global property recovers with any in checkPropertyAccess.
        if Some(receiver_type) == self.global_this_type {
            return match self.binder.global(name) {
                Some(symbol)
                    if !self.binder.symbols().get(symbol).flags.intersects(
                        SymbolFlags::BLOCK_SCOPED_VARIABLE | SymbolFlags::CLASS | SymbolFlags::ENUM,
                    ) && self.symbol_is_value(symbol) =>
                {
                    self.get_type_of_symbol(symbol)
                }
                _ => self.intrinsics.any,
            };
        }
        // §45 (`checker-notes-narrow.md`): the global `Record<K, V>` answers
        // V for property reads when K is `string` — the one mapped alias
        // special-cased, everything else mapped stays the subsystem. The
        // `noUncheckedIndexedAccess` refinement was measured at −6 net
        // (10 R→W / 4 W→R): the option's property-vs-element split differs
        // inside this road and stays recorded, not guessed.
        if let Some(value) = self.record_string_value(receiver_type) {
            return value;
        }
        // §32 (`checker-notes-narrow.md`): a receiver minted for an
        // UNRESOLVED type reference is upstream's `errorType`, and member
        // access through it answers the same — printed `any`. The §31
        // structural gate applies: in an import-machinery file the mint may
        // be the PORT's resolution miss.
        if self.unresolved_types.contains(&receiver_type)
            && node_id.is_some_and(|id| !self.file_has_import_machinery(id))
        {
            return self.intrinsics.any;
        }
        // No explicit test for an `errorType` receiver: it is an intrinsic and
        // never carries a members table, so the lookup below misses and answers
        // `errorType` anyway. An earlier draft guarded it and no mutation could
        // make the guard observable, so it was removed rather than kept as
        // decoration. The identity test above is what keeps that true now that
        // an `ANY`-flagged type has a fast path.
        // On a property miss, the `prop == nil` path of
        // `checkPropertyAccessExpressionOrQualifiedName`: an applicable index
        // signature answers the member's type (`noImplicitAny` errors are
        // diagnostics, not types). The key is the name's string literal type.
        // See `checker-notes-narrow.md` §17.
        let property_type = if let Some(found) =
            self.get_type_of_property_with_this_argument(receiver_type, name, this_argument, false)
        {
            found
        } else {
            let key = self.store.intern_literal(
                crate::flags::TypeFlags::STRING_LITERAL,
                crate::types::TypeData::StringLiteral(name.to_string()),
                false,
            );
            if let Some(info) = self.get_applicable_index_info(receiver_type, key) {
                if self.no_unchecked_indexed_access {
                    let undefined = self.intrinsics.undefined;
                    self.get_union_type(&[info.value, undefined])
                } else {
                    info.value
                }
            } else if node_id.is_some_and(|id| !self.in_js_file(id))
                && self.miss_is_established(receiver_type, name)
            {
                // §123 (`checker-notes-narrow.md`): the walk COMPLETED —
                // every base on the chain was followed and the name is
                // established absent — so this is upstream's TS2339/TS2550
                // report and its errorType-printed-any, not a table this
                // port failed to read. A blocked walk keeps the gap.
                // JS positions excluded — unchecked-JS misses answer
                // differently upstream (spellingUncheckedJS's 7 R→W).
                self.intrinsics.any
            } else {
                error
            }
        };
        // §164 (`checker-notes-narrow.md`), §163's decidable slice 1: a
        // member whose type IS the owner's `this` type answers the
        // RECEIVER — `getTypeWithThisArgument` (`checker.go:19573`) reads a
        // member with the receiver as the this-argument, and where the
        // member's type is the this-type itself the substitution's whole
        // effect is that answer. A receiver that IS a this-type substitutes
        // to itself, so it is skipped rather than looped.
        // §166 (`checker-notes-narrow.md`): the port mints `this` in TWO
        // tables — `this_types` per CLASS symbol (`expressions.rs`) and
        // `this_type_nodes` per INTERFACE declaration (`declared.rs:163`) —
        // and §164/§165 reached only the first, so a lib INTERFACE's
        // `valueOf(): this` still printed `this`. Consulting both at the
        // substitution site completes the rule WITHOUT unifying the mints
        // (that unification is rock #3's prerequisite for the
        // representation work, and is deliberately not attempted here).
        //
        // One walk over all minted this-types decides whether any is mentioned;
        // only then is the first mentioned one (table order) looked for.
        let property_type =
            if property_type != this_argument && self.is_minted_this_type(property_type) {
                this_argument
            } else if property_type != this_argument
                && self.mentions_this_type(property_type)
                && let Some(minted) = self
                    .this_types
                    .values()
                    .chain(self.this_type_nodes.values())
                    .copied()
                    .find(|&minted| self.mentions_type_parameter(property_type, &[minted], &[]))
            {
                // §165 (`checker-notes-narrow.md`), §164's embedded half: where
                // the this-type sits INSIDE the member's type — `fn(): this`
                // read off `c` wants `() => C` — the same
                // `getTypeWithThisArgument` substitution runs through
                // `instantiate_type`, which already handles it: a this-type is
                // TYPE_PARAMETER-flagged, so arm 1 maps it and the signature
                // text is REBUILT rather than reused. An instantiation that
                // gaps keeps the original rather than answering error.
                let map = [(minted, this_argument)];
                let names = ["this"];
                let image = self.instantiate_type(property_type, &map, &[minted], &names);
                if image == self.intrinsics.error { property_type } else { image }
            } else {
                property_type
            };
        // `checkPropertyAccessExpressionOrQualifiedName` ends by narrowing the
        // property's declared type by the flow reaching this access
        // (`getFlowTypeOfReference`, `checker.go:11430`) — `bd tsr-6ka`. Until
        // this call existed, *no* property access reached the flow walk, which
        // made every ported narrowing guard dead on the forms the corpus
        // actually writes (`a.b !== undefined`, `o.foo != null`).
        //
        // The binder already records a flow node for a narrowable access
        // (`record_flow`'s `PropertyAccessExpression` arm), so nothing in the
        // binder moves; `get_flow_type_of_reference` returns the declared type
        // unchanged when there is none.
        //
        // A gap is not narrowed: `errorType` carries `TypeFlags::ANY`, and
        // filtering it would answer `never` for a property this port could not
        // type — a confident wrong line out of a missing one.
        let Some(id) = node_id else { return property_type };
        if property_type == error {
            return property_type;
        }
        // `getFlowTypeOfAccessExpression` (`checker.go:11396`): a definite
        // assignment target is not narrowed by the flow reaching it.
        // Under `exactOptionalPropertyTypes` the caller's `removeMissingType`
        // still misses members whose optionality this port spells as plain
        // `undefined` (`strictOptionalProperties1`'s `Partial<…>` writes), so
        // that mode keeps the earlier narrowed answer until those members
        // carry `missingType`.
        // Sibling JSDoc properties now have alias-owned declarations and the
        // ordinary missingType optionality, so they can use the native write
        // type without this legacy flow compensation.
        let jsdoc_property = self.exact_optional_property_types
            && self.get_property_of_type(receiver_type, name).is_some_and(|property| {
                let entry = self.binder.symbols().get(property);
                entry.parent.is_some_and(|owner| {
                    self.binder
                        .symbols()
                        .get(owner)
                        .flags
                        .contains(tsr_binder::SymbolFlags::TYPE_ALIAS)
                }) && entry.declarations.iter().any(|&declaration| {
                    self.nodes.kind(declaration) == tsr_ast::SyntaxKind::JSDocPropertyTag
                })
            });
        if (!self.exact_optional_property_types || jsdoc_property)
            && self.assignment_target_kind(id) == crate::expressions::AssignmentTargetKind::Definite
        {
            return property_type;
        }
        if self.declared_method_access_skips_flow(receiver_type, name, property_type) {
            return property_type;
        }
        self.get_flow_type_of_reference(id, None, property_type)
    }

    /// `getFlowTypeOfAccessExpression` (pinned 5b1047d, `checker.go:11400`):
    /// only a variable, property or accessor, or a method whose type is a
    /// union (an optional method), is narrowed by the flow reaching the
    /// access; any other property answers its type without the flow walk.
    ///
    /// Applied only where this port's property symbol is the one native
    /// reads: a member of a declared class or interface (or a reference to
    /// one), whose instantiation keeps the declaration's flags
    /// (`instantiateSymbol`). Union/intersection properties (native
    /// synthesizes them as `Property`), mapped and reverse-mapped members and
    /// object-literal images keep the walk, as before.
    fn declared_method_access_skips_flow(
        &mut self,
        receiver_type: TypeId,
        name: &str,
        property_type: TypeId,
    ) -> bool {
        if self.store.get(property_type).flags.intersects(TypeFlags::UNION)
            || self.mapped_identity_optionality.contains_key(&receiver_type)
            || self.mapped_types.contains_key(&receiver_type)
            || self.anonymous_properties.contains_key(&receiver_type)
        {
            return false;
        }
        let TypeData::Named { members: Some(owner), .. } = self.store.get(receiver_type).data
        else {
            return false;
        };
        if !self
            .binder
            .symbols()
            .get(owner)
            .flags
            .intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE)
        {
            return false;
        }
        self.get_property_of_type(receiver_type, name).is_some_and(|property| {
            let flags = self.binder.symbols().get(property).flags;
            flags.contains(SymbolFlags::METHOD)
                && !flags.intersects(
                    SymbolFlags::VARIABLE | SymbolFlags::PROPERTY | SymbolFlags::ACCESSOR,
                )
        })
    }

    /// The type whose members a property access should be looked up in.
    ///
    /// Ported from `Checker.getApparentType` (`checker.go:21729`, from `grep -n`
    /// on the declaration), **primitive arms only** — the five `switch` cases
    /// that map a primitive to its global wrapper interface, in upstream's own
    /// order.
    ///
    /// # What is deliberately not ported, and who owns each
    ///
    /// Upstream's `getApparentType` has five arms before these five and three
    /// after. Every one of them is in a file this workstream does not own, and
    /// each returns the type unchanged here rather than guessing:
    ///
    /// | upstream arm | what it needs | owner |
    /// |---|---|---|
    /// | `TypeFlagsInstantiable` → base constraint | `getBaseConstraintOfType` | type parameters |
    /// | `ObjectFlagsMapped` | mapped types | not ported at all |
    /// | `ObjectFlagsReference` → `getTypeWithThisArgument` | instantiation | `bd tsr-4qx` |
    /// | `TypeFlagsIntersection` | `getApparentTypeOfIntersectionType` | `intersections.rs` |
    /// | `TypeFlagsNonPrimitive` / `Index` / `Unknown` | `emptyObjectType`, `stringNumberSymbolType` | `intrinsics.rs` |
    ///
    /// The earlier intrinsic prerequisite is now satisfied: `empty_object`
    /// and `unknown_empty_object` are canonical, store-owned identities with
    /// completed empty own members. The non-primitive and loose-unknown arms
    /// below reuse those identities; strict free unknown remains unchanged.
    /// This does not certify missing-member diagnostics, whose receiver and
    /// completeness gates are owned by `nonexistent_property`.
    ///
    /// **This is why the slice is 1,165 lines and not 13,156.** The row
    /// `property access, the receiver has no such property` blocks 13,156 gap
    /// lines; measured by `examples/gaproot.rs`, 36.7% of them have an
    /// instantiated-generic receiver and 9.6% an array receiver — the
    /// `ObjectFlagsReference` arm, which is `bd tsr-4qx` — and only 11.3% have a
    /// primitive one. See `docs/architecture/checker-notes-gaproot.md` Part 2.
    ///
    /// # A missing global is a gap, never `any`
    ///
    /// `globals()` may not hold `String` — a lib-less unit test never does. The
    /// original type is returned then, and the lookup below misses and answers
    /// `errorType`. Returning `anyType` or `emptyObjectType` instead would turn
    /// a lib configuration into a confident wrong answer on every primitive
    /// member access in the corpus.
    ///
    /// Likewise a global whose *declared type* gaps: `get_declared_type_of_symbol`
    /// answering `errorType` means the interface is there and unreadable, and the
    /// original primitive is the more honest receiver to fail on.
    /// The `extends` constraint of a type-parameter type, resolved.
    ///
    /// getConstraintOfTypeParameter (checker.go), retaining the effective
    /// outer mapper for written constraints. A polymorphic this parameter
    /// instead has the declaring class or interface as its constraint.
    pub(crate) fn type_parameter_constraint(&mut self, id: TypeId) -> Option<TypeId> {
        if !self.store.get(id).flags.contains(TypeFlags::TYPE_PARAMETER) {
            return None;
        }
        if let Some(symbol) =
            self.this_types.iter().find_map(|(&symbol, &ty)| (ty == id).then_some(symbol)).or_else(
                || {
                    self.this_type_nodes.iter().find_map(|(&node, &ty)| {
                        (ty == id).then(|| self.binder.symbol_of(node)).flatten()
                    })
                },
            )
        {
            return Some(self.get_declared_type_of_symbol(symbol));
        }
        // getConstraintOfTypeParameter keeps a resolved constraint identity.
        // This port evaluates nodes under alias binding frames, so the cache
        // also retains the effective mapper rather than sharing substitutions.
        let bindings: rustc_hash::FxHashMap<_, _> = self
            .alias_evaluation_bindings
            .iter()
            .flat_map(|frame| frame.iter().map(|(&symbol, &ty)| (symbol, ty)))
            .collect();
        let mut bindings: Vec<_> = bindings.into_iter().collect();
        bindings.sort_unstable_by_key(|&(symbol, _)| symbol);
        let key = TypeParameterConstraintKey { parameter: id, bindings };
        if let Some(&constraint) = self.type_parameter_constraint_cache.get(&key) {
            return constraint;
        }
        if let Some(instance) = self.instantiated_type_parameters.get(&id).cloned() {
            self.type_parameter_constraint_cache.insert(key.clone(), None);
            let constraint =
                self.type_parameter_constraint(instance.target).and_then(|constraint| {
                    let names: Vec<_> = instance.names.iter().map(String::as_str).collect();
                    let image = self.instantiate_type(
                        constraint,
                        &instance.map,
                        &instance.parameters,
                        &names,
                    );
                    (image != self.intrinsics.error).then_some(image)
                });
            self.type_parameter_constraint_cache.insert(key, constraint);
            return constraint;
        }
        let symbol = *self.type_parameter_symbols.get(&id)?;
        // getConstraintDeclaration searches all merged declarations; an infer
        // parameter can have its constraint on a later occurrence.
        let constraint_node =
            self.binder.symbols().get(symbol).declarations.iter().find_map(|&declaration| {
                match self.node_map.get(declaration) {
                    Some(Node::TypeParameterDeclaration(parameter)) => parameter.constraint,
                    _ => None,
                }
            })?;
        self.type_parameter_constraint_cache.insert(key.clone(), None);
        let constraint = self.mapped_constraint_type(constraint_node);
        // A constraint that itself gaps leaves the parameter as it was: a gap
        // beats reading members off `errorType`.
        let constraint = (constraint != self.intrinsics.error).then_some(constraint);
        self.type_parameter_constraint_cache.insert(key, constraint);
        constraint
    }

    /// getInferredTypeParameterConstraint (checker.go:17114). Conditional
    /// inference reads this before applying its non-fixing constraint mapper.
    pub(crate) fn inferred_type_parameter_constraint(&mut self, id: TypeId) -> Option<TypeId> {
        let symbol = *self.type_parameter_symbols.get(&id)?;
        let declarations = self.binder.symbols().get(symbol).declarations.clone();
        let mut constraints = Vec::new();
        for declaration in declarations {
            let Some(infer) = self.nodes.parent(declaration) else { continue };
            if !matches!(self.node_map.get(infer), Some(Node::InferTypeNode(_))) {
                continue;
            }
            let mut child = infer;
            let mut parent = self.nodes.parent(child);
            while let Some(node) = parent
                && matches!(self.node_map.get(node), Some(Node::ParenthesizedTypeNode(_)))
            {
                child = node;
                parent = self.nodes.parent(child);
            }
            let Some(parent) = parent else { continue };
            let constraint = match self.node_map.get(parent) {
                Some(Node::TypeReferenceNode(reference)) => {
                    let Some(symbol) = reference
                        .type_name
                        .and_then(|name| self.resolve_entity_name(name, SymbolFlags::TYPE))
                    else {
                        continue;
                    };
                    let Some(index) = reference
                        .type_arguments
                        .iter()
                        .position(|argument| argument.node_id() == Some(child))
                    else {
                        continue;
                    };
                    let Some(parameters) = self.local_type_parameter_types_of(symbol) else {
                        continue;
                    };
                    if reference.type_arguments.len() != parameters.len() {
                        continue;
                    }
                    let Some(&(parameter, _)) = parameters.get(index) else { continue };
                    let Some(constraint) = self.type_parameter_constraint(parameter) else {
                        continue;
                    };
                    let arguments: Vec<_> = reference
                        .type_arguments
                        .iter()
                        .map(|&argument| self.get_type_from_type_node(argument))
                        .collect();
                    if arguments.contains(&self.intrinsics.error) {
                        continue;
                    }
                    let types: Vec<_> = parameters.iter().map(|&(id, _)| id).collect();
                    let names: Vec<_> = parameters.iter().map(|(_, name)| name.as_str()).collect();
                    let map: Vec<_> = types.iter().copied().zip(arguments).collect();
                    let instantiated = self.instantiate_type(constraint, &map, &types, &names);
                    if instantiated == id || instantiated == self.intrinsics.error {
                        continue;
                    }
                    instantiated
                }
                Some(Node::RestTypeNode(_)) => {
                    let target = self.global_type_symbol("Array")?;
                    self.create_type_reference(target, vec![self.intrinsics.unknown])
                }
                Some(Node::ParameterDeclaration(parameter))
                    if parameter.dot_dot_dot_token.is_some() =>
                {
                    let target = self.global_type_symbol("Array")?;
                    self.create_type_reference(target, vec![self.intrinsics.unknown])
                }
                Some(Node::NamedTupleMember(member))
                    if member.dot_dot_dot_token.is_some()
                        || self.nodes.parent(parent).is_some_and(|outer| {
                            matches!(self.node_map.get(outer), Some(Node::RestTypeNode(_)))
                        }) =>
                {
                    let target = self.global_type_symbol("Array")?;
                    self.create_type_reference(target, vec![self.intrinsics.unknown])
                }
                Some(Node::TemplateLiteralTypeSpan(_)) => self.intrinsics.string,
                Some(Node::TypeParameterDeclaration(_))
                    if self.nodes.parent(parent).is_some_and(|outer| {
                        matches!(self.node_map.get(outer), Some(Node::MappedTypeNode(_)))
                    }) =>
                {
                    self.get_union_type(&[
                        self.intrinsics.string,
                        self.intrinsics.number,
                        self.intrinsics.es_symbol,
                    ])
                }
                _ => continue,
            };
            if constraint != self.intrinsics.error {
                constraints.push(constraint);
            }
        }
        if constraints.is_empty() {
            None
        } else {
            Some(self.get_intersection_type(&constraints, None))
        }
    }

    pub(crate) fn apparent_type(&mut self, id: TypeId) -> TypeId {
        let id = self.apparent_mapped_type(id);
        // Upstream's order, arm for arm (`checker.go:21745-21751`). `NUMBER_LIKE`
        // carrying `ENUM` is upstream's too, not a widening added here.
        //
        // A union carries `TypeFlags::UNION` and **not** its constituents'
        // flags (`crate::unions::create_union`), with the single exception of
        // `false | true`, which is given `BOOLEAN` deliberately so that it *is*
        // `boolean`. So `string | number` reaches no arm here and `boolean`
        // reaches the fourth — which is exactly upstream's behaviour, and the
        // reason this needs no union guard of its own.
        // `getApparentType`'s **head**, before the switch (`checker.go:21731`):
        // *"if t.flags&TypeFlagsInstantiable != 0 { t = getBaseConstraintOfType(t);
        // if t == nil { t = unknownType } }"*. A type parameter is read
        // through its constraint, which is what makes `T.x` resolve for
        // `T extends Shape` (`bd tsr-rppd`).
        //
        // An **unconstrained** parameter becomes `unknown`, whose lookup finds
        // nothing — the same gap as today, by a different route, which is why
        // this cannot lose a line on its own.
        //
        // Recursive base constraints also expose indexed and template types;
        // polymorphic this reads through its declaring class/interface.
        let id = self.base_constraint_of_type(id).unwrap_or(id);
        let flags = self.store.get(id).flags;
        let global = if flags.intersects(TypeFlags::STRING_LIKE) {
            "String"
        } else if flags.intersects(TypeFlags::NUMBER_LIKE) {
            "Number"
        } else if flags.intersects(TypeFlags::BIG_INT_LIKE) {
            "BigInt"
        } else if flags.intersects(TypeFlags::BOOLEAN_LIKE) {
            "Boolean"
        } else if flags.intersects(TypeFlags::ES_SYMBOL_LIKE) {
            "Symbol"
        } else if flags.contains(TypeFlags::NON_PRIMITIVE)
            || (flags.contains(TypeFlags::UNKNOWN) && !self.strict_null_checks)
        {
            // Pinned 5b1047d getApparentType (checker.go:21754-21759).
            // Reuse the canonical completed empty object; keep the original
            // receiver as the caller's this_argument, not a fresh wrapper.
            return self.intrinsics.empty_object;
        } else {
            return id;
        };
        // Not `crate::declared::global_type_symbol`, which gates on the symbol
        // having **exactly one** type parameter — right for `Array<T>` and
        // `Promise<T>`, and wrong for every interface here, all of which have
        // none. Reusing it would have made this arm silently dead.
        let Some(&symbol) = self.binder.globals().get(global) else { return id };
        let declared = self.get_declared_type_of_symbol(symbol);
        if declared == self.intrinsics.error { id } else { declared }
    }

    /// The **type** of a property of `id`, or `None` if there is no such
    /// property.
    ///
    /// Ported from `Checker.getTypeOfPropertyOfType` (`checker.go:18951`),
    /// which is three lines and the same three: look the property up, and if it
    /// is there, take its type.
    ///
    /// # Why a two-line function is worth having
    ///
    /// Not for the deduplication. `get_property_of_type` returns a
    /// [`SymbolId`], and a symbol is the *uninstantiated* declaration: on a
    /// receiver `C<number>` whose member is declared `a: T`, the symbol's type
    /// is `T` and every caller that turns the symbol into a type on its own
    /// gets `T` where upstream answers `number`. There were three such callers
    /// — property access (here), element access (`crate::indexed`) and both
    /// sides of the relater's structural comparison (`crate::relater`) — and
    /// substituting in one of them would leave the other two answering a
    /// **confident wrong type**. At the relater that is the dangerous
    /// direction: it acts on a `false`, so a wrong member type promotes the
    /// next overload candidate rather than degrading to a gap. See
    /// *"A conservative `false` is safe for one kind of consumer and unsafe for
    /// the other"* in `docs/conventions.md`.
    ///
    /// So this is the single place instantiation can be added once and be true
    /// everywhere, which is `bd tsr-4qx` — and it is now added: on a receiver
    /// that came out of `create_type_reference`, the found property's declared
    /// type is substituted through [`Checker::instantiate_type`] with the
    /// target's own type parameters mapped to the reference's arguments. A
    /// member whose declared type this port cannot rebuild — baked signature
    /// text, a tuple — falls through `instantiate_type`'s arms to `errorType`,
    /// an honest gap rather than the uninstantiated `T`.
    ///
    /// `None` means *no such property*, exactly as upstream's `nil` does. A
    /// property that exists and whose type this port cannot compute answers
    /// `Some(errorType)`, which is what keeps the relater's existence test
    /// (`crate::relater`'s `properties_related_to`) meaning what it did.
    #[must_use]
    pub fn get_type_of_property_of_type(&mut self, id: TypeId, name: &str) -> Option<TypeId> {
        self.get_type_of_property_with_this_argument(id, name, id, false)
    }

    /// Source optionality follows the ordinary supplier, not the reused symbol's flags.
    fn identity_mapped_source_property_is_optional(&mut self, id: TypeId, name: &str) -> bool {
        self.anonymous_properties
            .get(&id)
            .and_then(|(properties, instantiated)| {
                instantiated
                    .then(|| properties.iter().find(|property| property.name == name))
                    .flatten()
            })
            .map(|property| property.optional)
            .or_else(|| {
                let (optional, _) = *self.mapped_identity_optionality.get(&id)?;
                optional.or_else(|| {
                    let source =
                        self.type_reference_targets.get(&id).and_then(|(_, arguments)| {
                            (arguments.len() == 1).then_some(arguments[0])
                        })?;
                    Some(self.identity_mapped_source_property_is_optional(source, name))
                })
            })
            .unwrap_or_else(|| {
                self.get_property_of_type(id, name)
                    .is_some_and(|property| self.property_is_optional(property))
            })
    }

    /// getTypeWithThisArgument retains the original receiver when member
    /// lookup proceeds through its apparent constraint.
    pub(crate) fn get_type_of_property_with_this_argument(
        &mut self,
        id: TypeId,
        name: &str,
        this_argument: TypeId,
        skip_object_function_augment: bool,
    ) -> Option<TypeId> {
        if name == "length"
            && let Some(body) = self.completed_array_placeholder_length_body(id)
        {
            return self.get_type_of_property_with_this_argument(
                body,
                name,
                this_argument,
                skip_object_function_augment,
            );
        }
        self.resolve_mapped_type_members(id);
        if let Some(property) = self
            .anonymous_properties
            .get(&id)
            .and_then(|(properties, instantiated)| {
                instantiated
                    .then(|| properties.iter().find(|property| property.name == name))
                    .flatten()
            })
            .cloned()
        {
            // getTypeOfReverseMappedSymbol (inference.go:1145): a reverse
            // mapped property type resolves its own members when read.
            let property_type = self.property_type(&property);
            self.complete_reverse_mapped_type(property_type);
            return Some(if property.optional {
                self.get_optional_type(property_type, true)
            } else {
                property_type
            });
        }

        // §952: a homomorphic IDENTITY mapped type reads the SOURCE's member
        // and then applies the mapping's optionality modifier — the one thing
        // the reused member owner cannot carry. `Partial<O>`'s `x` is
        // `string | undefined`, `Required<{ x?: string }>`'s is `string`, and
        // `Readonly<O>`'s is `string` unchanged.
        //
        // Upstream sets optionality on a freshly synthesised property symbol in
        // `resolveMappedTypeMembers` (`checker.go`); see
        // `Checker::mapped_identity_optionality` for why this port cannot and
        // what it does instead. The read is here rather than at the mint because
        // the source's member type is only known per name.
        if let Some(&(optionality, _)) = self.mapped_identity_optionality.get(&id) {
            let owner = match &self.store.get(id).data {
                TypeData::Named { members: Some(owner), .. } => *owner,
                _ => return None,
            };
            let source = self
                .type_reference_targets
                .get(&id)
                .and_then(|(_, arguments)| (arguments.len() == 1).then_some(arguments[0]));
            let (member, source_property) = if let Some(source) = source {
                (
                    self.get_type_of_property_with_this_argument(
                        source,
                        name,
                        source,
                        skip_object_function_augment,
                    )?,
                    None,
                )
            } else {
                let mut visiting = Vec::new();
                let property = self.get_property_of_declared_symbol(owner, name, &mut visiting)?;
                (self.get_type_of_symbol(property), Some(property))
            };
            return Some(match optionality {
                // `?` / `+?`: the property becomes optional, which a READ sees
                // as `| undefined`.
                Some(true) => {
                    let undefined = self.intrinsics.undefined;
                    self.get_union_type(&[member, undefined])
                }
                // getTypeOfMappedSymbol strips only when the source was optional
                // in strict mode. Exact mode removes missing, never genuine U.
                Some(false) => {
                    let strip_optional = self.strict_null_checks
                        && match source {
                            Some(source) => {
                                self.identity_mapped_source_property_is_optional(source, name)
                            }
                            None => source_property
                                .is_some_and(|property| self.property_is_optional(property)),
                        };
                    if strip_optional {
                        self.remove_missing_or_undefined_type(member)
                    } else {
                        member
                    }
                }
                None => member,
            });
        }
        // §49 (`checker-notes-narrow.md`): a UNION projects across its
        // constituents — every one must carry the name, and the answer is
        // the union of the member types.
        if let TypeData::Union { types, .. } = &self.store.get(id).data {
            let constituents = types.clone();
            let mut projected = Vec::with_capacity(constituents.len());
            let mut has_property = false;
            // createUnionOrIntersectionProperty (5b1047d checker.go:21554)
            // retains declaration identity independently of value projection.
            // Distinct private/protected origins without a common declaration
            // are not a union property. Query-local roots publish no image.
            let mut non_public = false;
            for constituent in constituents {
                // §117 slice 3: each constituent reads through its APPARENT
                // type — upstream's per-constituent getReducedApparentType;
                // `(string | number).constructor` answers through the
                // wrapper interfaces, and slice 1's Object fallback then
                // covers the object constituents.
                let apparent = self.apparent_type(constituent);
                if self.store.get(apparent).flags.contains(TypeFlags::NEVER)
                    || self.intersection_has_never_discriminant(apparent)
                {
                    continue;
                }
                if let Some(property) =
                    self.get_property_of_type_ex(apparent, name, skip_object_function_augment)
                {
                    non_public |= self.property_is_non_public(property);
                }
                let member = if let Some(member) = self.get_type_of_property_with_this_argument(
                    apparent,
                    name,
                    constituent,
                    skip_object_function_augment,
                ) {
                    has_property = true;
                    member
                } else {
                    // createUnionOrIntersectionProperty accepts an applicable
                    // index in a constituent missing the named property. For
                    // example, [boolean, string] | string[] has a property 0
                    // contributed by the tuple and an index from the array.
                    // A purely indexed union still has no named property.
                    if name.starts_with('[')
                        || self.tuple_element_lists.contains_key(&apparent)
                        || self.variadic_tuple_elements.contains_key(&apparent)
                    {
                        return None;
                    }
                    let key = self.store.intern_literal(
                        TypeFlags::STRING_LITERAL,
                        TypeData::StringLiteral(name.to_string()),
                        false,
                    );
                    self.get_applicable_index_info(apparent, key)?.value
                };
                projected.push(member);
            }
            if non_public {
                self.get_property_of_union_or_intersection_type(
                    id,
                    name,
                    skip_object_function_augment,
                )?;
            }
            return has_property.then(|| self.get_union_type(&projected));
        }
        // A tuple's numeric-literal property IS its element. Upstream reaches
        // this through the synthesised tuple target's members
        // (`createNormalizedTupleType`) and the reference's resolved type
        // arguments; this port's tuple stores its element list in
        // `tuple_element_lists` (`bd tsr-5ll`) and answers from it directly.
        // Exact by construction — the modifier tuple forms refuse at the mint,
        // so every recorded element is a plain one. Out of range answers
        // `undefined`: §8's registration guessed a gap there and the baseline
        // corrected it before the code ran — `indexerWithTuple.types` records
        // `>strNumTuple[2] : undefined` (the TS2493 diagnostic lives beside
        // it, not in the type). The round-trip test rejects `"01"`/`"-0"`,
        // which name no element. `checker-notes-tuple.md` §8.
        if self.variadic_tuple_elements.contains_key(&id) {
            if name == "length" {
                return Some(self.intrinsics.number);
            }
            if let Ok(index) = name.parse::<usize>()
                && index.to_string() == name
                && let Some(element) = self.variadic_tuple_element_type(id, index, false)
            {
                return Some(element);
            }
        }
        if let Some((elements, _)) = self.tuple_element_lists.get(&id)
            && let Ok(index) = name.parse::<usize>()
            && index.to_string() == name
        {
            let element = elements.get(index).copied();
            // §79: an OPTIONAL element reads `| undefined` — the mint's
            // "exact by construction" claim predates optional tuples, and
            // the first measurement without this arm priced it at 100 G→W
            // (`optionalTupleElements1`).
            let optional = self
                .tuple_optional_masks
                .get(&id)
                .is_some_and(|mask| mask.get(index).copied().unwrap_or(false));
            return Some(match element {
                None => self.intrinsics.undefined,
                Some(element) if optional => {
                    let undefined = self.intrinsics.undefined;
                    self.get_union_type(&[element, undefined])
                }
                Some(element) => element,
            });
        }
        // §117 slice 2: the synthetic `.prototype` on a class STATIC side —
        // upstream mints it typed as the INSTANCE (generic classes: the
        // reference over per-parameter `any`); a WRITTEN member named
        // `prototype` would be found by the symbol road, but upstream's mint
        // shadows statics only where no such member exists, and a class
        // cannot declare one (TS2699), so answering first is safe. Function
        // receivers keep the fallback's `any`-family answers.
        if name == "prototype"
            && let TypeData::Anonymous { symbol, .. } = self.store.get(id).data
            && self
                .binder
                .symbols()
                .get(self.binder.merged_symbol(symbol))
                .flags
                .contains(SymbolFlags::CLASS)
        {
            let class = self.binder.merged_symbol(symbol);
            let parameters = self.local_type_parameters_of(class);
            if parameters.is_empty() {
                let instance = self.get_declared_type_of_symbol(class);
                if instance != self.intrinsics.error {
                    return Some(instance);
                }
            } else {
                let any = self.intrinsics.any;
                let arguments = vec![any; parameters.len()];
                return Some(self.create_type_reference(class, arguments));
            }
        }
        // §117 slice 4: a TUPLE's `.length` is the LITERAL element count
        // (`createNormalizedTupleType` mints it; plain tuples only — the
        // §37 registry has no rest/optional shapes to miscount... optional
        // masks DO exist: a tuple with optionals answers the union of
        // possible lengths upstream; decline those, plain counts only).
        if name == "length"
            && let Some((elements, _)) = self.tuple_element_lists.get(&id)
        {
            let count = elements.len();
            let min_length = self
                .tuple_optional_masks
                .get(&id)
                .map_or(count, |mask| mask.iter().filter(|&&optional| !optional).count());
            let lengths: Vec<_> = (min_length..=count)
                .map(|length| {
                    self.store.intern_literal(
                        crate::flags::TypeFlags::NUMBER_LITERAL,
                        crate::types::TypeData::NumberLiteral(length.to_string()),
                        false,
                    )
                })
                .collect();
            return Some(self.get_union_type(&lengths));
        }
        // An intersection's property is synthetic upstream, typed as the
        // intersection of its constituents' property types; the symbol road
        // below holds only a constituent's stand-in
        // (`get_property_of_union_or_intersection_type`).
        if matches!(self.store.get(id).data, TypeData::Intersection { .. }) {
            return self.property_type_via_shape(id, name, skip_object_function_augment);
        }
        if let Some(property) = self.get_property_of_type_ex(id, name, skip_object_function_augment)
        {
            let declared = self.get_type_of_symbol(property);
            let instantiated =
                self.instantiate_for_reference_with_this(id, declared, this_argument);
            // §92: a property the symbol road FINDS but cannot type may
            // still answer through the shape road (chain1's low reads — the
            // alias symbol's table hands back a symbol whose declared type
            // does not compute).
            if instantiated == self.intrinsics.error
                && let Some(shaped) =
                    self.property_type_via_shape(id, name, skip_object_function_augment)
            {
                return Some(shaped);
            }
            return Some(instantiated);
        }
        // §393: a member inherited through a GENERIC heritage entry reads
        // through the INSTANTIATED base — `class T6 extends T5<number>`
        // answers `this.foo : number` for `foo: T` on `T5<T>`
        // (`superCallArgsMustMatch`). The symbol road above cannot see it:
        // `base_symbols_of` refuses type-argument heritage for exactly this
        // reason (§202 — a member table cannot be instantiated), and the
        // instantiation goes through §226's `base_type_of_heritage_entry`
        // plus the reference road's own member typing instead. Cycles are
        // guarded by the walk's visited set.
        if let Some(member) = self.generic_heritage_member(id, name, &mut Vec::new()) {
            return Some(member);
        }
        // §770: a TUPLE's non-numeric members come from `Array<T>`.
        //
        // Upstream's tuple is a REFERENCE to a target synthesised by
        // `createNormalizedTupleType` (`checker.go:24148`) whose base is
        // `Array<union of the element types>` (`ReadonlyArray` when readonly),
        // so `.length`, `.some`, `.every` and the rest resolve through the
        // ordinary base-member road. This port mints a tuple as a bare `Named`
        // object carrying an element list (`create_tuple_type`,
        // `declared.rs:1775`) with NO base at all, so it answered its numeric
        // indices (§8, above) and nothing else.
        //
        // §769 is what found this: giving an IIFE rest parameter its correct
        // TUPLE type turned `noNumbers.some(…)` from RIGHT to GAP, because the
        // parameter had been `any[]` — a type that does have array members —
        // and the tuple did not. That arm was reverted and this is its
        // recorded prerequisite.
        // `length` is NOT one of them. A tuple's own `length` is its element
        // COUNT — a literal for a plain tuple (§117 slice 4, above), and a
        // UNION of possible lengths when elements are optional, which §117
        // declines. Upstream's tuple target declares its own `length` rather
        // than inheriting `Array`'s `number`, so falling back here would
        // replace a deliberate decline with a confidently wrong answer:
        // measured at 2 R→W on `tupleTypes`' `readonly [number?]`, whose
        // baseline wants `0 | 1` and got `number`.
        if name != "length"
            && let Some((elements, readonly)) = self.tuple_element_lists.get(&id)
        {
            let (elements, readonly) = (elements.clone(), *readonly);
            let target = if readonly { "ReadonlyArray" } else { "Array" };
            if let Some(target) = self.global_type_symbol(target) {
                // The element type is the UNION of the tuple's elements, which
                // is what upstream's target is instantiated over. An empty
                // tuple has no elements and takes `never`, so `[].length` is
                // still `number` and `[].some` still resolves.
                let element = if elements.is_empty() {
                    self.intrinsics.never
                } else {
                    self.get_union_type(&elements)
                };
                let array = self.create_type_reference(target, vec![element]);
                if array != self.intrinsics.error
                    && let Some(property) = self.get_property_of_type(array, name)
                {
                    let declared = self.get_type_of_symbol(property);
                    return Some(self.instantiate_for_reference_with_this(array, declared, id));
                }
            }
        }
        self.property_type_via_shape(id, name, skip_object_function_augment)
    }

    /// Native 5b1047d checker.go:24115/25121 publishes the original array
    /// before recursive arguments (:21893); its tuple retains that identity.
    /// This port's tuple retains `alias_placeholders`[owner] instead. Read only
    /// the existing successful `declared_types`[owner] for a length lookup:
    /// canonical Array/ReadonlyArray and its exact ordered argument key.
    /// The explicit number property cannot depend on the original `this` or
    /// element mapper. Other members, active/captured images and incomplete
    /// owners decline; one existing-owner scan, no evaluation, publication,
    /// source rekey or cache. The normal length getter retains its work boundary.
    fn completed_array_placeholder_length_body(&self, id: TypeId) -> Option<TypeId> {
        if !matches!(self.store.get(id).data, TypeData::Named { members: None, .. })
            || !self.alias_evaluation_bindings.is_empty()
            || self.mapped_template_depth != 0
            || self.instantiation_depth != 0
            || self.identity_unmapped_type_parameters
            || !self.render_type_parameter_scope.is_empty()
        {
            return None;
        }
        let mut owners =
            self.alias_placeholders.iter().filter(|(_, placeholder)| **placeholder == id);
        let (&owner, _) = owners.next()?;
        if owners.next().is_some()
            || self.resolutions.on_stack(owner, crate::resolution::PropertyName::DeclaredType)
            || !self.binder.symbols().get(owner).flags.contains(SymbolFlags::TYPE_ALIAS)
        {
            return None;
        }
        let [declaration] = self.binder.symbols().get(owner).declarations.as_slice() else {
            return None;
        };
        let Node::TypeAliasDeclaration(alias) = self.node_map.get(*declaration)? else {
            return None;
        };
        let root = self.nodes.parent(*declaration)?;
        if !alias.type_parameters.is_empty()
            || self.nodes.kind(root) != tsr_ast::SyntaxKind::SourceFile
            || self.nodes.flags(root).contains(tsr_ast::NodeFlags::JAVASCRIPT_FILE)
        {
            return None;
        }
        let array = match alias.r#type? {
            tsr_ast::TypeNode::ArrayTypeNode(_) => "Array",
            tsr_ast::TypeNode::TypeOperatorNode(operator)
                if operator.operator.kind == tsr_ast::SyntaxKind::ReadonlyKeyword
                    && matches!(operator.r#type, Some(tsr_ast::TypeNode::ArrayTypeNode(_))) =>
            {
                "ReadonlyArray"
            }
            _ => return None,
        };
        let target = self.global_type_symbol_with_arity(array, 1)?;
        let body = self.without_alias(*self.declared_types.get(&owner)?);
        if body == id || body == self.intrinsics.unresolved || self.is_error(body) {
            return None;
        }
        let key @ (body_target, arguments) = self.type_reference_targets.get(&body)?;
        let [element] = arguments.as_slice() else { return None };
        if *body_target != target
            || self.instantiations.get(key) != Some(&body)
            || self.resolutions.on_stack(target, crate::resolution::PropertyName::DeclaredType)
            || !self.binder.symbols().get(target).flags.contains(SymbolFlags::INTERFACE)
            || !matches!(self.store.get(body).data, TypeData::Named { members: Some(member), .. } if member == target)
            || *element == self.intrinsics.unresolved
            || self.is_error(*element)
        {
            return None;
        }
        let length = *self.binder.symbols().get(target).members.get("length")?;
        let [declaration] = self.binder.symbols().get(length).declarations.as_slice() else {
            return None;
        };
        let Node::PropertySignatureDeclaration(property) = self.node_map.get(*declaration)? else {
            return None;
        };
        (property.postfix_token.is_none()
            && matches!(property.r#type, Some(tsr_ast::TypeNode::KeywordTypeNode(keyword)) if keyword.kind == tsr_ast::SyntaxKind::NumberKeyword))
        .then_some(body)
    }

    /// Resolve inherited members through each instantiated base, guarded by
    /// symbol identity against cyclic heritage.
    fn generic_heritage_member(
        &mut self,
        id: TypeId,
        name: &str,
        visiting: &mut Vec<SymbolId>,
    ) -> Option<TypeId> {
        let TypeData::Named { members: Some(owner), .. } = self.store.get(id).data else {
            return None;
        };
        if visiting.contains(&owner) {
            return None;
        }
        visiting.push(owner);
        let declarations: Vec<tsr_ast::NodeId> =
            self.binder.symbols().get(owner).declarations.iter().copied().collect();
        for declaration in declarations {
            let clauses = match self.node_map.get(declaration) {
                Some(Node::ClassDeclaration(node)) => node.heritage_clauses,
                Some(Node::ClassExpression(node)) => node.heritage_clauses,
                Some(Node::InterfaceDeclaration(node)) => node.heritage_clauses,
                _ => continue,
            };
            for clause in clauses {
                if clause.token.kind != tsr_ast::SyntaxKind::ExtendsKeyword {
                    continue;
                }
                for entry in clause.types {
                    let Some(base) = self.base_symbol_of_heritage_entry(entry, false) else {
                        continue;
                    };
                    if visiting.contains(&base) {
                        continue;
                    }
                    // reparseHosted copies JS @augments arguments onto the
                    // native heritage reference. Reuse the existing supplier
                    // when the written entry has none (5b1047d getBaseTypes).
                    let arguments = if entry.type_arguments.is_empty()
                        && entry.node_id.is_some_and(|node| self.in_js_file(node))
                    {
                        entry
                            .node_id
                            .and_then(|node| self.jsdoc_augments_type_arguments(node))
                            .unwrap_or(entry.type_arguments)
                    } else {
                        entry.type_arguments
                    };
                    let Some(base_type) =
                        self.instantiated_heritage_base(base, arguments, entry.node_id)
                    else {
                        continue;
                    };
                    // The ordinary found-path pair (symbol, then the
                    // reference's instantiation), with the DEEPER generic
                    // heritage recursing through THIS walk so the visited
                    // set holds across levels — a fresh set would spin on
                    // mutually-generic bases.
                    if let Some(property) = self.get_property_of_type(base_type, name) {
                        let declared = self.get_type_of_symbol(property);
                        let instantiated = self.instantiate_for_reference(base_type, declared);
                        if instantiated != self.intrinsics.error {
                            // §923: the SECOND substitution. The step above maps
                            // the base's parameters onto the heritage entry's
                            // arguments — for `interface D<T> extends C<T>` that
                            // is `C`'s `U := T`, which leaves `T`. The
                            // REFERENCE's own arguments (`D<string>`) are a
                            // separate map and nothing applied them, so
                            // `d.m` answered `(x: T) => T`.
                            //
                            // `extends C<string>` worked and hid it: a concrete
                            // heritage argument needs no second step, so the
                            // road looked complete.
                            //
                            // Instantiating for `id` composes the two. A
                            // non-generic reference maps nothing and the result
                            // is unchanged, so this cannot disturb the shapes
                            // that already worked.
                            let composed = self.instantiate_for_reference(id, instantiated);
                            if composed != self.intrinsics.error {
                                return Some(composed);
                            }
                            return Some(instantiated);
                        }
                    }
                    if let Some(member) = self.generic_heritage_member(base_type, name, visiting) {
                        // Resolve inherited members under every enclosing
                        // reference mapper, including indirect generic bases.
                        let member = self.instantiate_for_reference(id, member);
                        if member != self.intrinsics.error {
                            return Some(member);
                        }
                    }
                }
            }
        }
        None
    }

    /// The property symbol a member inherited through a GENERIC heritage
    /// entry comes from, found by [`Self::generic_heritage_member`]'s walk
    /// (each base instantiated, then the ordinary symbol road on it).
    ///
    /// Upstream's `getPropertyOfType` answers the instantiated symbol
    /// (`instantiateSymbol`), which keeps the declaration's flags. This
    /// answers the declaring symbol: its optional/readonly modifiers are the
    /// same, its type is not instantiated, so only flag readers may use it —
    /// which is why [`Self::get_property_of_type`] itself does not fall back
    /// to it (`base_symbols_of`'s §202 refusal).
    pub(crate) fn generic_heritage_property_symbol(
        &mut self,
        id: TypeId,
        name: &str,
        visiting: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        let TypeData::Named { members: Some(owner), .. } = self.store.get(id).data else {
            return None;
        };
        if visiting.contains(&owner) {
            return None;
        }
        visiting.push(owner);
        let declarations: Vec<tsr_ast::NodeId> =
            self.binder.symbols().get(owner).declarations.iter().copied().collect();
        for declaration in declarations {
            let clauses = match self.node_map.get(declaration) {
                Some(Node::ClassDeclaration(node)) => node.heritage_clauses,
                Some(Node::ClassExpression(node)) => node.heritage_clauses,
                Some(Node::InterfaceDeclaration(node)) => node.heritage_clauses,
                _ => continue,
            };
            for clause in clauses {
                if clause.token.kind != tsr_ast::SyntaxKind::ExtendsKeyword {
                    continue;
                }
                for entry in clause.types {
                    let Some(base) = self.base_symbol_of_heritage_entry(entry, false) else {
                        continue;
                    };
                    if visiting.contains(&base) {
                        continue;
                    }
                    let arguments = if entry.type_arguments.is_empty()
                        && entry.node_id.is_some_and(|node| self.in_js_file(node))
                    {
                        entry
                            .node_id
                            .and_then(|node| self.jsdoc_augments_type_arguments(node))
                            .unwrap_or(entry.type_arguments)
                    } else {
                        entry.type_arguments
                    };
                    let Some(base_type) =
                        self.instantiated_heritage_base(base, arguments, entry.node_id)
                    else {
                        continue;
                    };
                    if let Some(property) = self.get_property_of_type(base_type, name) {
                        return Some(property);
                    }
                    if let Some(property) =
                        self.generic_heritage_property_symbol(base_type, name, visiting)
                    {
                        return Some(property);
                    }
                }
            }
        }
        None
    }

    /// Root declarations contributing an intersection property, corresponding
    /// to createUnionOrIntersectionProperty's distinct property set.
    pub(crate) fn intersection_property_symbols(
        &mut self,
        id: TypeId,
        name: &str,
    ) -> Vec<SymbolId> {
        let TypeData::Intersection { types, .. } = self.store.get(id).data.clone() else {
            return self.get_property_of_type(id, name).into_iter().collect();
        };
        let mut symbols = Vec::new();
        for part in types {
            let apparent = self.apparent_type(part);
            for symbol in self.intersection_property_symbols(apparent, name) {
                if !symbols.contains(&symbol) {
                    symbols.push(symbol);
                }
            }
        }
        symbols
    }

    /// §92 (`checker-notes-narrow.md`): the property roads the symbol table
    /// cannot answer — an INTERSECTION's constituents (multiple hits
    /// intersect, upstream's synthesized intersection property), `Omit<T, K>`
    /// by its global symbol (a name outside `K` reads through `T`), and an
    /// alias reference with a non-literal body (evaluated under §91's
    /// bindings, then re-asked). `None` stays *no such property*.
    fn property_type_via_shape(
        &mut self,
        id: TypeId,
        name: &str,
        skip_object_function_augment: bool,
    ) -> Option<TypeId> {
        // createUnionOrIntersectionProperty (checker.go): each constituent
        // contributes its apparent property type, with the entire receiver
        // substituted for polymorphic this before intersecting the types.
        if let TypeData::Intersection { types, .. } = &self.store.get(id).data {
            let constituents = types.clone();
            if self.intersection_has_never_discriminant(id) {
                return None;
            }
            for skip_augment in [true, false] {
                if !skip_augment && skip_object_function_augment {
                    break;
                }
                let mut hits = Vec::new();
                for &constituent in &constituents {
                    let apparent = self.apparent_type(constituent);
                    if let Some(member) = self.get_type_of_property_with_this_argument(
                        apparent,
                        name,
                        id,
                        skip_augment,
                    ) {
                        hits.push(member);
                    }
                }
                if !hits.is_empty() {
                    return Some(self.get_intersection_type(&hits, None));
                }
            }
            return None;
        }
        let (target, arguments) = self.type_reference_targets.get(&id).cloned()?;
        if self.global_type_symbol_with_arity("Omit", 2) == Some(target) && arguments.len() == 2 {
            let removed = self.literal_key_texts(arguments[1])?;
            if removed.iter().any(|key| key == name) {
                return None;
            }
            // `Omit<T, K>` is `Pick<T, Exclude<keyof T, K>>`: a key exists
            // only where `keyof T` has one, and getLiteralTypeFromProperty
            // excludes private/protected members. For a generic `T` the
            // resolved mapped member is the template `T[P]` with `P` fixed,
            // which stays a deferred indexed access (`this["publicProp"]`,
            // `destructuringUnspreadableIntoRest`).
            let source = arguments[0];
            let apparent = self.apparent_type(source);
            if let Some(property) = self.get_property_of_type(apparent, name)
                && self.is_non_public_member(property)
            {
                return None;
            }
            if self.store.get(source).flags.intersects(TypeFlags::TYPE_PARAMETER) {
                self.get_property_of_type(apparent, name)?;
                let key = self.store.intern_literal(
                    TypeFlags::STRING_LITERAL,
                    crate::types::TypeData::StringLiteral(name.to_string()),
                    false,
                );
                return self.resolved_indexed_access_type(source, key, false);
            }
            return self.get_type_of_property_with_this_argument(
                source,
                name,
                source,
                skip_object_function_augment,
            );
        }
        if self.binder.symbols().get(target).flags.contains(tsr_binder::SymbolFlags::TYPE_ALIAS) {
            let evaluated = self.evaluate_alias_body(target, &arguments)?;
            if evaluated != id {
                return self.get_type_of_property_with_this_argument(
                    evaluated,
                    name,
                    evaluated,
                    skip_object_function_augment,
                );
            }
        }
        None
    }

    /// A member's type as seen through an instantiated reference: `declared`
    /// with the receiver's type arguments substituted in, or `declared`
    /// unchanged when the receiver is not an instantiated reference.
    ///
    /// The instantiation half of `getTypeOfPropertyOfType` reached through
    /// upstream's `instantiateSymbol` (`checker.go:20753`) — upstream
    /// instantiates the *symbol* when members are resolved and the type falls
    /// out; this port has no instantiated symbols, so the same substitution is
    /// applied to the type at the one seam every consumer shares.
    ///
    /// An arity mismatch between the target's parameters and the reference's
    /// arguments — or a parameter list this port cannot resolve — answers
    /// `errorType` rather than substituting partially.
    pub(crate) fn instantiate_for_reference(
        &mut self,
        receiver: TypeId,
        declared: TypeId,
    ) -> TypeId {
        self.instantiate_for_reference_with_this(receiver, declared, receiver)
    }

    /// # Memoised like native's instantiated-symbol links
    ///
    /// Native computes an instantiated member's type once, in
    /// `getTypeOfInstantiatedSymbol` (`checker.go:15987`), and keeps it in the
    /// instantiated symbol's links. This port has no instantiated symbols, so
    /// every read re-ran the substitution here.
    /// [`PerfLinks::reference_member_types`](crate::perf_links::PerfLinks)
    /// keeps the decided answers; key, publication and context are
    /// `docs/parity/notes/r4-perf2.md` §2.
    fn instantiate_for_reference_with_this(
        &mut self,
        receiver: TypeId,
        declared: TypeId,
        this_argument: TypeId,
    ) -> TypeId {
        let Some(symbol) = self.reference_target_symbol(receiver) else {
            return declared;
        };
        let binder = self.binder;
        let Some(frames) = self.memo_frames(&binder.symbols().get(symbol).declarations, None)
        else {
            return self
                .instantiate_for_reference_with_this_worker(
                    symbol,
                    receiver,
                    declared,
                    this_argument,
                )
                .0;
        };
        let key = (receiver, declared, this_argument);
        let this_type = self.polymorphic_this_of(symbol);
        let result = match self.perf_links.reference_member_types.get(&key) {
            Some(&(published_this, result)) if published_this == this_type => result,
            _ => {
                let mark = self.publication_mark();
                let (result, mapped) = self.instantiate_for_reference_with_this_worker(
                    symbol,
                    receiver,
                    declared,
                    this_argument,
                );
                // An error answer may be provisional (an unresolved parameter
                // list, an arity mismatch, a pending return, the depth or
                // count limit); an unchanged `declared` after a real
                // substitution read `declared`'s current contents, which
                // `TypeStore::complete_object` may still fill in place.
                let decided = result != self.intrinsics.error
                    && (!mapped || result != declared || self.is_leaf_type(declared));
                if decided
                    && self.publishable_since(mark)
                    && self.polymorphic_this_of(symbol) == this_type
                {
                    self.perf_links.reference_member_types.insert(key, (this_type, result));
                }
                result
            }
        };
        self.alias_evaluation_bindings = frames;
        result
    }

    /// Whether `id` is a primitive, literal or enum type, or a union of them:
    /// a type with no object, instantiable or intersection content, so no
    /// later in-place completion can give it a type parameter to substitute.
    fn is_leaf_type(&self, id: TypeId) -> bool {
        let non_leaf = TypeFlags::OBJECT | TypeFlags::INSTANTIABLE | TypeFlags::INTERSECTION;
        let ty = self.store.get(id);
        match &ty.data {
            TypeData::Union { types, .. } => types.iter().all(|&member| {
                !self.store.get(member).flags.intersects(non_leaf | TypeFlags::UNION)
            }),
            _ => !ty.flags.intersects(non_leaf | TypeFlags::UNION),
        }
    }

    /// The class or interface whose members `receiver` reads: a reference's
    /// target, or a non-generic class/interface kept as a Named type
    /// (`resolveTypeReferenceMembers` supplies a this argument for those too).
    fn reference_target_symbol(&self, receiver: TypeId) -> Option<SymbolId> {
        if let Some((symbol, _)) = self.type_reference_targets.get(&receiver) {
            return Some(*symbol);
        }
        if self.store.get(receiver).flags.contains(TypeFlags::OBJECT)
            && let TypeData::Named { members: Some(symbol), .. } = self.store.get(receiver).data
            && self
                .binder
                .symbols()
                .get(symbol)
                .flags
                .intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE)
        {
            return Some(symbol);
        }
        None
    }

    /// The target's polymorphic `this` type if one has been minted yet; it is
    /// minted lazily on the first `this` reference, so the answer can change.
    fn polymorphic_this_of(&self, symbol: SymbolId) -> Option<TypeId> {
        self.this_types.get(&symbol).copied().or_else(|| {
            self.binder
                .symbols()
                .get(symbol)
                .declarations
                .iter()
                .find_map(|node| self.this_type_nodes.get(node).copied())
        })
    }

    /// The substitution itself: `declared` with `receiver`'s arguments and
    /// its this argument substituted for `symbol`'s parameters, and whether a
    /// non-empty mapper was applied (an empty one answers `declared` whatever
    /// `declared` holds).
    fn instantiate_for_reference_with_this_worker(
        &mut self,
        symbol: SymbolId,
        receiver: TypeId,
        declared: TypeId,
        this_argument: TypeId,
    ) -> (TypeId, bool) {
        let arguments =
            self.type_reference_targets.get(&receiver).map(|(_, arguments)| arguments.clone());
        let error = self.intrinsics.error;
        let Some(parameters) = self.local_type_parameter_types_of(symbol) else {
            return (error, false);
        };
        let arguments = arguments
            .unwrap_or_else(|| parameters.iter().map(|(parameter, _)| *parameter).collect());
        if parameters.len() != arguments.len() {
            return (error, false);
        }
        // instantiateSymbol (checker.go:20753) retains the complete receiver
        // mapper. A member's own same-named parameter has a distinct TypeId,
        // so it survives while an outer parameter in the same return is mapped.
        let mut names: Vec<&str> = Vec::new();
        let mut types: Vec<TypeId> = Vec::new();
        let mut map: Vec<(TypeId, TypeId)> = Vec::new();
        for (index, (parameter, name)) in parameters.iter().enumerate() {
            if *parameter != arguments[index] {
                names.push(name.as_str());
                types.push(*parameter);
                map.push((*parameter, arguments[index]));
            }
        }
        // resolveTypeReferenceMembers pads the type arguments with the
        // reference itself for the target's polymorphic this parameter.
        if let Some(this_type) = self.polymorphic_this_of(symbol)
            && this_type != this_argument
        {
            types.push(this_type);
            map.push((this_type, this_argument));
        }
        // A reference without type parameters or a polymorphic this needs no map.
        if map.is_empty() {
            return (declared, false);
        }
        (self.instantiate_type(declared, &map, &types, &names), true)
    }

    /// `getPropertyOfTypeEx`'s union and intersection arms
    /// (`checker.go:18899`) as [`UnionOrIntersectionProperty`]: an
    /// intersection's property without the Object/Function augment first,
    /// then with it. A union receiver is not yet routed here (its consumers
    /// keep their constituent walks), so it answers `None` like any other
    /// non-composite receiver.
    fn composite_property_of_type(
        &mut self,
        id: TypeId,
        name: &str,
        skip_object_function_augment: bool,
    ) -> Option<UnionOrIntersectionProperty> {
        if !matches!(self.store.get(id).data, TypeData::Intersection { .. }) {
            return None;
        }
        self.get_property_of_union_or_intersection_type(id, name, true).or_else(|| {
            (!skip_object_function_augment)
                .then(|| self.get_property_of_union_or_intersection_type(id, name, false))
                .flatten()
        })
    }

    /// `getPropertyOfUnionOrIntersectionType` (`checker.go:21414`) through
    /// `getUnionOrIntersectionProperty` (`checker.go:21428`) and
    /// `createUnionOrIntersectionProperty` (`checker.go:21452`): each
    /// constituent's apparent type is asked (error and `never` constituents
    /// skipped); no hit is no property. In a union a constituent without the
    /// name makes the property write-partial when an applicable index
    /// signature or a spread-free object literal answers the read, and
    /// read-partial otherwise, which this answers `None`; a union property
    /// that is partial or held by distinct symbols, one of them private or
    /// protected, with no declaration common to all, is no property either.
    ///
    /// No cache: upstream's `propertyCache` memoizes the minted symbol; this
    /// port mints none (see [`UnionOrIntersectionProperty`]), so the work is
    /// one lookup per constituent, each the members subsystem's own.
    fn get_property_of_union_or_intersection_type(
        &mut self,
        id: TypeId,
        name: &str,
        skip_object_function_augment: bool,
    ) -> Option<UnionOrIntersectionProperty> {
        let (constituents, is_union) = match &self.store.get(id).data {
            TypeData::Union { types, .. } => (types.clone(), true),
            TypeData::Intersection { types, .. } => (types.clone(), false),
            _ => return None,
        };
        // getReducedApparentType: an intersection getReducedType turns into
        // `never` has no properties.
        if !is_union && self.intersection_has_never_discriminant(id) {
            return None;
        }
        let mut single: Option<SymbolId> = None;
        let mut properties: Vec<(SymbolId, TypeId)> = Vec::new();
        let mut partial = false;
        let mut read_partial = false;
        let mut contains_non_public = false;
        // A union constituent's readonly index signature sets the union
        // property's CheckFlagsReadonly.
        let mut index_readonly = false;
        for current in constituents {
            let t = self.apparent_type(current);
            if self.is_error(t) || self.store.get(t).flags.contains(TypeFlags::NEVER) {
                continue;
            }
            if let Some(property) =
                self.get_property_of_type_ex(t, name, skip_object_function_augment)
            {
                single.get_or_insert(property);
                if !properties.iter().any(|&(known, _)| known == property) {
                    properties.push((property, t));
                }
                if is_union {
                    contains_non_public |= self.property_is_non_public(property);
                }
            } else if is_union {
                partial = true;
                // isLateBoundName: a `[Symbol.x]` key is asked of the symbol
                // index, which no identifier text reaches.
                let index = if name.starts_with('[') {
                    None
                } else {
                    let key = self.store.intern_literal(
                        TypeFlags::STRING_LITERAL,
                        TypeData::StringLiteral(name.to_string()),
                        false,
                    );
                    self.get_applicable_index_info(t, key)
                };
                if let Some(index) = index {
                    index_readonly |= index.readonly;
                } else if self.object_literal_spread_flags.get(&t) != Some(&false) {
                    read_partial = true;
                }
            }
        }
        let single = single?;
        let distinct = properties.len() > 1;
        if is_union
            && (distinct || partial)
            && contains_non_public
            && !(distinct && self.properties_have_common_declaration(&properties))
        {
            return None;
        }
        // getPropertyOfUnionOrIntersectionType filters read-partial properties.
        if read_partial {
            return None;
        }
        Some(UnionOrIntersectionProperty {
            single,
            synthetic: (distinct || partial).then_some(SyntheticProperty {
                containing_type: id,
                is_union,
                properties,
                index_readonly,
            }),
        })
    }

    /// `isReadonlySymbol(getPropertyOfType(receiver, name))`
    /// (`checker.go:13849`): for the synthetic property
    /// `createUnionOrIntersectionProperty` mints, its `CheckFlagsReadonly`
    /// (this port mints no synthetic symbol; see
    /// [`UnionOrIntersectionProperty`]); otherwise the found symbol's own
    /// answer.
    pub(crate) fn is_readonly_property_of_type(&mut self, receiver: TypeId, name: &str) -> bool {
        if let Some(composite) = self.composite_property_of_type(receiver, name, false)
            && let Some(synthetic) = composite.synthetic
        {
            // CheckFlagsReadonly: a union's when any constituent's property
            // or index signature is readonly, an intersection's only when
            // every constituent's property is.
            if synthetic.index_readonly {
                return true;
            }
            let mut readonly = synthetic.properties.iter().map(|&(property, _)| property);
            let mut is_readonly = |property| {
                self.is_readonly_symbol(property) || self.property_signature_is_readonly(property)
            };
            return if synthetic.is_union {
                readonly.any(&mut is_readonly)
            } else {
                readonly.all(&mut is_readonly)
            };
        }
        self.get_property_of_type(receiver, name)
            .is_some_and(|property| self.is_readonly_symbol(property))
    }

    /// `getWriteTypeOfSymbol(getPropertyOfType(receiver, name))`
    /// (`checker.go`) where it differs from the read type. For the synthetic
    /// property `createUnionOrIntersectionProperty` mints (this port mints no
    /// synthetic symbol; see [`UnionOrIntersectionProperty`]) that is
    /// `links.writeType`: once any constituent property's write type differs
    /// from its type, the union (intersection) of every constituent
    /// property's write type. Otherwise the found symbol's divergent setter
    /// ([`Checker::write_type_of_accessors`]). `None` keeps the read type.
    pub(crate) fn write_type_of_property_of_type(
        &mut self,
        receiver: TypeId,
        name: &str,
    ) -> Option<TypeId> {
        if let Some(composite) = self.composite_property_of_type(receiver, name, false)
            && let Some(synthetic) = composite.synthetic
        {
            let mut divergent = false;
            let mut write_types = Vec::with_capacity(synthetic.properties.len());
            for (property, constituent) in synthetic.properties {
                if let Some(written) = self.write_type_of_accessors(property) {
                    divergent = true;
                    write_types.push(written);
                } else {
                    write_types.push(self.get_type_of_property_with_this_argument(
                        constituent,
                        name,
                        synthetic.containing_type,
                        false,
                    )?);
                }
            }
            if !divergent {
                return None;
            }
            return Some(if synthetic.is_union {
                self.get_union_type(&write_types)
            } else {
                self.get_intersection_type(&write_types, None)
            });
        }
        let property = self.get_property_of_type(receiver, name)?;
        let written = self.write_type_of_accessors(property)?;
        // getWriteTypeOfInstantiatedSymbol (5b1047d): setter values use the
        // same ordered reference mapper as reads. Existing instantiation owns
        // completion; no write-only mapper/cache is created.
        Some(self.instantiate_for_reference(receiver, written))
    }

    /// `hasCommonDeclaration` (`checker.go:21679`): some declaration of the
    /// first property is a declaration of every other.
    fn properties_have_common_declaration(&self, properties: &[(SymbolId, TypeId)]) -> bool {
        let symbols = self.binder.symbols();
        symbols.get(properties[0].0).declarations.iter().any(|declaration| {
            properties[1..]
                .iter()
                .all(|&(property, _)| symbols.get(property).declarations.contains(declaration))
        })
    }

    /// Ported from `Checker.getPropertyOfTypeEx` (`checker.go:18899`) through
    /// `getPropertyOfObjectType` (`:21403`).
    ///
    /// # Inherited members are found by walking base types, not by flattening
    ///
    /// Upstream does **not** put a base class's properties in the derived
    /// symbol's members table. `resolveDeclaredMembers` (`checker.go:19612`) takes
    /// exactly `getMembersOfSymbol(t.symbol)` — the declared members and nothing
    /// else — and `resolveObjectTypeMembers` layers the base types' properties
    /// *underneath* them, so a derived declaration shadows the base's and the
    /// answer for an inherited name is still **the base's symbol**. Flattening the
    /// two tables in the binder would merge identities upstream keeps apart and
    /// would answer with the wrong symbol for an overridden member, so this walks
    /// instead: own members first, then each base in declaration order, first hit
    /// wins.
    ///
    /// # The walk is over symbols, not types
    ///
    /// Upstream reaches a base through `resolvedBaseTypes`, which are `*Type`s.
    /// Here `TypeData::Named` carries the owning [`SymbolId`] and
    /// `getDeclaredTypeOfClassOrInterface` is `new_named_type(symbol, …)` — one
    /// type per symbol, with no members of its own yet
    /// (`crate::declared::get_declared_type_of_class_or_interface`). Going
    /// type → symbol → base symbol and back is therefore the same graph with one
    /// indirection removed, and it avoids creating a type for every base merely to
    /// read its symbol back out.
    ///
    /// **The consequence accepted:** the moment `getDeclaredTypeOfClassOrInterface`
    /// grows real member resolution — the instantiated members of `class C extends
    /// B<number>` — this must move to the type level, because a symbol has no place
    /// to hold an instantiated table. That is the falsifier for this shape.
    ///
    /// # `symbolIsValue`
    ///
    /// The gate is upstream's (`symbolIsValueEx`, `checker.go:22095`) and it is not
    /// cosmetic here: a class's or interface's members table also holds its **type
    /// parameters** (`declareSymbolAndAddToSymbolTable` →
    /// `declareClassMember`, `internal/binder/binder.go:429-441`), so without it
    /// `new C().T` would answer with the type parameter `T`. **The alias half is
    /// ported too** — see [`Checker::symbol_is_value`]; it used to be a miss, and
    /// a miss on this gate is what `nonexistent_property` reports as TS2339.
    ///
    /// # Two tables, chosen by what the type *is*
    ///
    /// A `TypeData::Named` is the instance side and looks in `members`; a
    /// `TypeData::Anonymous` is `typeof X` and looks in `exports`. See
    /// [`Checker::get_property_of_anonymous_symbol`] for why that is not the same
    /// table with a different name.
    #[must_use]
    pub fn get_property_of_type(&mut self, id: TypeId, name: &str) -> Option<SymbolId> {
        self.get_property_of_type_ex(id, name, false)
    }

    pub(crate) fn get_property_of_type_ex(
        &mut self,
        id: TypeId,
        name: &str,
        skip_object_function_augment: bool,
    ) -> Option<SymbolId> {
        // getPropertyOfType resolves an open homomorphic map's members before
        // exposing their declaration roots. Values and mapped modifiers remain
        // on the synthesized property, not the unmodified origin symbol.
        if self.is_generic_homomorphic_mapped_type(id) && self.apparent_mapped_type(id) == id {
            self.resolve_mapped_type_members(id);
            if let Some((properties, true)) = self.anonymous_properties.get(&id)
                && let Some(property) = properties.iter().find(|property| property.name == name)
            {
                return property.origin;
            }
        }
        // getPropertyOfTypeEx's intersection arm (`checker.go:18899`).
        if matches!(self.store.get(id).data, TypeData::Intersection { .. }) {
            return self
                .composite_property_of_type(id, name, skip_object_function_augment)
                .map(|property| property.single);
        }
        // The borrow of `self.store` has to end before the recursion below, which
        // takes `&mut self`. Both bindings are `Copy`, so this statement copies
        // out what it needs and releases the type. ADR-0013's read-drop-recurse.
        let owner = match &self.store.get(id).data {
            TypeData::Named { members: Some(owner), .. } => Owner::Declared(*owner),
            TypeData::Anonymous { symbol, .. } => Owner::Anonymous(*symbol),
            _ if id == self.intrinsics.empty_object
                || id == self.intrinsics.unknown_empty_object =>
            {
                // getPropertyOfTypeEx (checker.go:18939): these store-owned
                // canonical objects have completed empty own members and no
                // signatures. Their only augment is the existing global
                // Object owner. A skipped augment or missing lib stays absent;
                // do not infer completeness for other symbol-less objects.
                if skip_object_function_augment {
                    return None;
                }
                let object = self.global_type_symbol_with_arity("Object", 0)?;
                return self.get_property_of_declared_symbol(object, name, &mut Vec::new());
            }
            _ => return None,
        };
        let found = if let Some(&(alias, source)) = self.module_value_clones.get(&id) {
            if name == "default"
                && let Some(default) = self.module_clone_default_symbol(alias)
            {
                return Some(default);
            }
            let member = self.get_property_of_type_ex(source, name, true);
            if self.class_static_symbol(source).is_some() {
                // Synthetic-default imports spread the resolved static surface
                // before cloning it. Filter the winning raw symbol, not an
                // alias target or a hidden member from a base class.
                member.filter(|&member| self.is_spreadable_property(member))
            } else {
                member
            }
        } else {
            match owner {
                Owner::Declared(owner) => {
                    let mut visiting = Vec::new();
                    let found = self.get_property_of_declared_symbol(owner, name, &mut visiting);
                    // `bindThisPropertyAssignment` (`binder.go:1115`) declares a
                    // JS `this.x = …` inside an object-literal method on the
                    // literal's symbol, but `checkObjectLiteral` builds the
                    // literal's type from its elements only
                    // (`checker.go:13173`), so such a member is not a property
                    // of the type.
                    if self.binder.symbols().get(owner).flags.contains(SymbolFlags::OBJECT_LITERAL)
                        && let Some(member) = found
                        && self.binder.symbols().get(member).declarations.iter().all(
                            |&declaration| {
                                self.nodes.kind(declaration)
                                    == tsr_ast::SyntaxKind::BinaryExpression
                            },
                        )
                    {
                        None
                    } else {
                        found
                    }
                }
                Owner::Anonymous(symbol) => self.get_property_of_anonymous_symbol(symbol, name),
            }
        };
        if found.is_some() {
            return found;
        }
        // §117 slice 1 (`getPropertyOfTypeEx`, `checker.go:18918`): an
        // OBJECT-flagged receiver's miss falls back to the FUNCTION interface
        // family when it carries signatures (the strictBindCallApply
        // interfaces, falling to `Function` when unmounted), then to the
        // global `Object` — always, which is what makes `.toString` answer
        // on every object while a genuine `.foo` still misses (Object misses
        // it too, byte-identical by construction).
        // Upstream withholds the fallback from CONST-ENUM objects (their
        // prototype access is TS2748-family territory) —
        // `constEnumNoObjectPrototypePropertyAccess`'s 14 G→W measured
        // without this gate.
        let withheld = match owner {
            Owner::Declared(owner) | Owner::Anonymous(owner) => self
                .binder
                .symbols()
                .get(self.binder.merged_symbol(owner))
                .flags
                .contains(SymbolFlags::CONST_ENUM),
        };
        if !skip_object_function_augment
            && !withheld
            && self.store.get(id).flags.contains(TypeFlags::OBJECT)
        {
            let mut fallbacks: Vec<&str> = Vec::new();
            // §395: a CLASS's static side is a constructor function — its
            // misses fall through the Function interface before Object
            // (`Foo.name : string`, `deleteReadonlyInStrictNullChecks`).
            // The signature-carrying test below cannot see it because a
            // class's anonymous static type registers no entry in
            // `signature_types`.
            let class_static = match owner {
                Owner::Anonymous(symbol) => self
                    .binder
                    .symbols()
                    .get(self.binder.merged_symbol(symbol))
                    .flags
                    .contains(SymbolFlags::CLASS),
                Owner::Declared(_) => false,
            } && !self.module_value_clones.contains_key(&id);
            let has_call = self
                .signatures_of_type_kind(id, crate::signatures::SignatureKind::Call)
                .is_some_and(|signatures| !signatures.is_empty());
            let has_construct = self
                .signatures_of_type_kind(id, crate::signatures::SignatureKind::Construct)
                .is_some_and(|signatures| !signatures.is_empty());
            if has_call || has_construct {
                let all_construct = !has_call;
                // §846: `CallableFunction`/`NewableFunction` are
                // `getGlobalStrictFunctionType`'s answer, and that function is
                // gated on the flag:
                //
                // ```go
                // func (c *Checker) getGlobalStrictFunctionType(name string) *Type {
                //     if c.strictBindCallApply {
                //         return c.getGlobalType(name, 0 /*arity*/, true /*reportErrors*/)
                //     }
                //     return c.globalFunctionType
                // }
                // ```
                //
                // With the flag off both ARE `Function`. Reaching for
                // `CallableFunction` unconditionally made `f.bind` resolve to
                // its two generic `this`-parameter overloads instead of
                // `Function.bind`, so relating a function type to `Function`
                // stopped being identity and became a structurally hard
                // question the relater answers `false` — the corpus's
                // `ElementRef & Function` rows, via
                // `narrow_type_by_type_facts`' third arm.
                if self.strict_bind_call_apply {
                    fallbacks.push(if all_construct {
                        "NewableFunction"
                    } else {
                        "CallableFunction"
                    });
                }
                fallbacks.push("Function");
            }
            // Resolved class constructors use NewableFunction before Function,
            // just as getPropertyOfTypeEx selects by signature kind. Retain the
            // previous fallback only when constructor resolution is unsupported.
            if class_static && !has_call && !has_construct {
                fallbacks.push("Function");
            }
            fallbacks.push("Object");
            for global in fallbacks {
                let Some(interface) = self.global_type_symbol_with_arity(global, 0) else {
                    continue;
                };
                let mut visiting = Vec::new();
                if let Some(found) =
                    self.get_property_of_declared_symbol(interface, name, &mut visiting)
                {
                    return Some(found);
                }
            }
        }
        None
    }

    /// A property of `typeof X` — the static side of a class, the exports of a
    /// namespace, the members of an enum.
    ///
    /// Ported from the tail of `Checker.resolveAnonymousTypeMembers`
    /// (`checker.go:20650`), whose third and last branch is introduced by the
    /// comment *"Combinations of function, class, enum and module"*
    /// (`checker.go:20671`) and reads
    /// `members := c.getExportsOfSymbol(symbol)` (`checker.go:20672`).
    ///
    /// # `exports`, not `members`, and the distinction is the whole point
    ///
    /// `crate::symbols` records why `TypeData::Anonymous` deliberately carried no
    /// members table until now: a class's statics and a namespace's exports live
    /// in the symbol's `exports`, and pointing this lookup at `members` would
    /// resolve `C.x` against the **instance** members — answering the wrong
    /// symbol rather than none. That constraint is why the fix is a second table
    /// consulted for a second type shape, and not a repointing of the existing
    /// walk. `Named` still reads `members`; nothing about the instance side moves.
    ///
    /// # The flags gate is upstream's branch, not a filter
    ///
    /// `resolveAnonymousTypeMembers` reaches the exports line only after two
    /// earlier returns: an instantiated type (`checker.go:20652`) and a
    /// **type-literal** symbol (`checker.go:20662`), which takes `getMembersOfSymbol`
    /// instead. The second one is live here: `crate::function_types` builds an
    /// anonymous type over `bindFunctionOrConstructorType`'s `__type` symbol.
    ///
    /// **This gate is unobservable today, and that is stated rather than
    /// implied.** Deleting it reddens no test in the workspace — I ran that
    /// mutation — because the only symbol it excludes is `__type`, whose
    /// `exports` table is empty, so gated and ungated both miss. It is kept
    /// rather than removed because the two are equal only by accident: the
    /// moment a `__type` or object-literal symbol carries an export, the ungated
    /// form answers from a table upstream never reads, and that is a wrong answer
    /// rather than a gap. The named edit that makes it bite is
    /// `crate::function_types` gaining the `__call`/`__new` member lookup, or
    /// `crate::objects` building `TypeData::Anonymous` for an object literal.
    /// Contrast [`Checker::check_property_access_expression`]'s removed
    /// `errorType` guard, which was dropped because its fallthrough was
    /// *provably* identical, not merely identical for now.
    ///
    /// # What is a miss here, and why a miss is safe
    ///
    /// Unlike [`Checker::base_symbols_of`], an unfollowable case here costs
    /// nothing but a gap: a miss returns `None`, which
    /// [`Checker::check_property_access_expression`] turns into `errorType`. There
    /// is no ordering hazard, because upstream layers inherited statics
    /// *underneath* own ones (`addInheritedMembers`, `checker.go:20690` — it adds
    /// only names not already present), so a name found in the symbol's own
    /// `exports` is always the symbol upstream would have answered with.
    ///
    /// Three things are therefore gaps rather than wrong answers:
    ///
    /// - **Statics inherited from a base class.** `class B { static x = 1 }` with
    ///   `class C extends B {}` gives `C.x` upstream through
    ///   `getBaseConstructorTypeOfClass` (`checker.go:20687`), which needs
    ///   construct signatures (`bd tsr-4sc.8`). `C`'s own statics are unaffected.
    /// - **`globalThis`** (`checker.go:20674`), which has no symbol here.
    /// - **An enum's numeric index signature** (`checker.go:20703`), so `E[0]`
    ///   stays a gap; index signatures are `bd tsr-4sc.8`'s.
    ///
    /// The `symbolIsValue` gate is upstream's own
    /// (`getPropertyOfTypeEx`, `checker.go:18916`) and matters more here than on
    /// the instance side: a namespace's `exports` holds its exported *types* too,
    /// so without it `namespace M { export interface I {} }` would answer `M.I`
    /// with an interface symbol in a value position.
    fn get_property_of_anonymous_symbol(
        &mut self,
        symbol: SymbolId,
        name: &str,
    ) -> Option<SymbolId> {
        let data = self.binder.symbols().get(symbol);
        // The synthetic CommonJS `module` object owns a members table, not a
        // static exports table (getTypeOfVariableOrParameterOrPropertyWorker).
        if data.flags.contains(SymbolFlags::MODULE_EXPORTS) {
            return data.members.get(name).copied().filter(|&member| self.symbol_is_value(member));
        }
        if !data.flags.intersects(
            SymbolFlags::FUNCTION
                | SymbolFlags::METHOD
                | SymbolFlags::CLASS
                | SymbolFlags::ENUM
                | SymbolFlags::VALUE_MODULE,
        ) {
            return None;
        }
        let is_class = data.flags.contains(SymbolFlags::CLASS);
        // An own export first, then the module's `export *` re-exports. Reading
        // the table alone is what made every member of a barrel module missing:
        // `import * as z from "zod"` names a module whose entire surface arrives
        // through `export *`, so `z.string` found nothing.
        let found = match data.exports.get(name).copied() {
            Some(found) => Some(found),
            None if is_class => self
                .late_bound_static_members_of(symbol)
                .into_iter()
                .find_map(|(spelled, member)| (spelled == name).then_some(member))
                .or_else(|| self.get_export_from_star(symbol, name)),
            None => self.get_export_from_star(symbol, name),
        };
        let found = match found {
            Some(found) => found,
            // §122 (`checker-notes-narrow.md`): a class's STATIC side is a
            // real inheritance chain — `resolveAnonymousTypeMembers`
            // (`checker.go:20650`, class arm) adds
            // `getPropertiesOfType(getBaseConstructorTypeOfClass(classType))`
            // when that type is an object, intersection or type variable, so
            // `class D extends B` finds `B.x` through `typeof D`, and `class
            // D2<T> extends C2<T>` or `extends o.A` reads the checked base
            // expression's statics. Own exports answered above, which is
            // what makes a derived redeclaration shadow by construction.
            // A `#private` static never inherits: private names are
            // lexically scoped to the declaring class body, and upstream
            // answers its error-any for `Derived.#x` (TS18013 territory) —
            // the walk carrying it measured 4 G→W
            // (privateNameStaticAccessorssDerivedClasses wants `any`).
            None if is_class && !name.starts_with('#') => {
                let base = self.get_base_constructor_type_of_class(symbol);
                if !self.store.get(base).flags.intersects(
                    TypeFlags::OBJECT
                        | TypeFlags::INTERSECTION
                        | TypeFlags::TYPE_PARAMETER
                        | TypeFlags::INDEXED_ACCESS,
                ) {
                    return None;
                }
                self.get_property_of_type(base, name)?
            }
            None => return None,
        };
        self.symbol_is_value(found).then_some(found)
    }

    /// §123's completeness probe: whether a Named receiver's base-type walk
    /// can be followed to the end — every `base_symbols_of` on the chain
    /// answers. Only then is a member's absence ESTABLISHED rather than
    /// unknown. A cycle answers `false` (decline, honest gap) — upstream
    /// reports a base-cycle diagnostic there, a channel this port lacks.
    fn named_walk_is_complete(&mut self, receiver: TypeId) -> bool {
        // §124: the Anonymous side's analogue. A CLASS owner establishes
        // absence through the same extends-chain walk §122 reads; a FUNCTION
        // or ENUM owner's exports are single-declaration-set and whole by
        // construction. VALUE_MODULE is NOT admitted — `export *` can carry
        // surface this port cannot see (the merged-symbol read happens first,
        // so `function f` merged with `namespace f` is excluded with it).
        if let TypeData::Anonymous { symbol, .. } = self.store.get(receiver).data {
            let merged = self.binder.merged_symbol(symbol);
            let flags = self.binder.symbols().get(merged).flags;
            if flags.contains(SymbolFlags::VALUE_MODULE) {
                return false;
            }
            if flags.contains(SymbolFlags::CLASS) {
                let mut visiting = Vec::new();
                return self.walk_completes(merged, &mut visiting)
                    && !self.chain_declares_index_signature(merged);
            }
            // FUNCTION owners measured 24 G→W (strictBindCallApply1): a
            // "missing" function member may live on the CallableFunction /
            // Function wrapper interfaces with a specialized type this
            // port's fallback answers differently — absence not established.
            return flags.intersects(SymbolFlags::ENUM);
        }
        let TypeData::Named { members: Some(owner), .. } = self.store.get(receiver).data else {
            return false;
        };
        // Only owners declared EXCLUSIVELY by class/interface declarations —
        // the two forms whose members tables the binder populates whole. A
        // Named minted for an alias of a mapped/conditional body carries a
        // table that was never the type's member list (mappedTypes2 21,
        // conditionalTypes1 26, recursiveIntersectionTypes 24 G→W on the
        // ungated pair — upstream computes real members there).
        // §124.2: VALUE-only declarations are IGNORED by the test — a lib
        // wrapper merges `interface Number` with `var Number:
        // NumberConstructor`, and the var contributes nothing to the
        // instance side; requiring all() of the raw list meant §123 never
        // established a single lib-wrapper miss (probe 14's walk=false on
        // `.length`-of-number, the §142 repro's final conjunct).
        let declared_only = {
            let symbol = self.binder.symbols().get(owner);
            let mut type_side = 0usize;
            let mut foreign = 0usize;
            for &declaration in &symbol.declarations {
                match self.nodes.kind(declaration) {
                    tsr_ast::SyntaxKind::ClassDeclaration
                    | tsr_ast::SyntaxKind::ClassExpression
                    | tsr_ast::SyntaxKind::InterfaceDeclaration => type_side += 1,
                    tsr_ast::SyntaxKind::VariableDeclaration
                    | tsr_ast::SyntaxKind::FunctionDeclaration => {}
                    _ => foreign += 1,
                }
            }
            type_side >= 1 && foreign == 0
        };
        if !declared_only {
            return false;
        }
        let mut visiting = Vec::new();
        self.walk_completes(owner, &mut visiting)
    }

    /// §123/§124's combined question: is this name's absence ESTABLISHED?
    /// A `CONST_ENUM` owner skips the `Function`-family gate — upstream
    /// deliberately withholds the prototype road there (TS2748-family, the
    /// §117 fallback's own recorded gate), so `E.toString`'s error-any IS
    /// the established answer (constEnumNoObjectPrototypePropertyAccess 14).
    pub(crate) fn miss_is_established(&mut self, receiver: TypeId, name: &str) -> bool {
        // §125, re-measured 2026-08-10 as §161 (`checker-notes-narrow.md`):
        // the union per-constituent arm re-refused at 63:213 — the same
        // ~1:3.4 as the original 39:164 despite the flow arc's +170. The
        // class is narrowing-owned WHOLE, twice measured.
        let const_enum = if let TypeData::Anonymous { symbol, .. } = self.store.get(receiver).data {
            let merged = self.binder.merged_symbol(symbol);
            self.binder.symbols().get(merged).flags.contains(SymbolFlags::CONST_ENUM)
        } else {
            false
        };
        if !const_enum && self.function_family_declares(receiver, name) {
            return false;
        }
        self.named_walk_is_complete(receiver)
    }

    /// §124 iteration 3: a name any of the Function-family or Object globals
    /// declares is never an established absence — the §117 fallback family
    /// may answer it with a type this port computes differently or not at
    /// all (`C.bind` under strictBindCallApply wants the specialized
    /// signature; strictBindCallApply1 measured 24 G→W through two
    /// iterations before this gate named the mechanism).
    /// §124.1's third callable detector: any declaration of the symbol
    /// carries a call- or construct-signature member.
    fn symbol_declares_signature_member(&self, owner: SymbolId) -> bool {
        self.binder.symbols().get(owner).declarations.iter().any(|&declaration| {
            let members: &[tsr_ast::TypeElement<'_>] = match self.node_map.get(declaration) {
                Some(Node::InterfaceDeclaration(node)) => node.members,
                Some(Node::TypeLiteralNode(node)) => node.members,
                _ => return false,
            };
            members.iter().any(|member| {
                matches!(
                    member,
                    tsr_ast::TypeElement::CallSignatureDeclaration(_)
                        | tsr_ast::TypeElement::ConstructSignatureDeclaration(_)
                )
            })
        })
    }

    fn function_family_declares(&mut self, receiver: TypeId, name: &str) -> bool {
        // §124.1: the Function-family interfaces only gate SIGNATURE-BEARING
        // receivers — the §117 fallback never consults them otherwise, so a
        // `.length` miss on `number` is fully established (§142's probe 5
        // found the over-wide gate erring whole literals through inferred
        // returns). Object's names gate unconditionally, mirroring its
        // unconditional fallback.
        let callable = self.signature_types.get(&receiver).is_some_and(|s| !s.is_empty())
            || matches!(self.store.get(receiver).data,
                crate::types::TypeData::Anonymous { symbol, .. }
                    if self.binder.symbols().get(self.binder.merged_symbol(symbol)).flags.intersects(
                        SymbolFlags::FUNCTION | SymbolFlags::METHOD | SymbolFlags::CLASS))
            // Third detector: a NAMED owner whose declarations carry
            // call/construct SIGNATURE members (`{ (): void; }` type
            // literals and interfaces — objectTypeWithCallSignature*'s 10
            // on the two-detector pair).
            || matches!(self.store.get(receiver).data,
                crate::types::TypeData::Named { members: Some(owner), .. }
                    if self.symbol_declares_signature_member(owner));
        let families: &[&str] = if callable {
            &["CallableFunction", "NewableFunction", "Function", "Object"]
        } else {
            &["Object"]
        };
        for global in families {
            if let Some(interface) = self.global_type_symbol_with_arity(global, 0) {
                let mut visiting = Vec::new();
                if self.get_property_of_declared_symbol(interface, name, &mut visiting).is_some() {
                    return true;
                }
            }
        }
        false
    }

    /// §124: whether any class declaration on the extends chain declares an
    /// index signature — a member this port's static index road cannot
    /// answer, so a miss beside one is not an established absence
    /// (staticIndexSignature4's 20 G→W on the ungated pair).
    fn chain_declares_index_signature(&mut self, owner: SymbolId) -> bool {
        let mut visiting = Vec::new();
        self.chain_declares_index_signature_worker(owner, &mut visiting)
    }

    fn chain_declares_index_signature_worker(
        &mut self,
        owner: SymbolId,
        visiting: &mut Vec<SymbolId>,
    ) -> bool {
        if visiting.contains(&owner) {
            return true;
        }
        visiting.push(owner);
        let declarations: Vec<_> =
            self.binder.symbols().get(owner).declarations.iter().copied().collect();
        for declaration in declarations {
            if let Some(Node::ClassDeclaration(class)) = self.node_map.get(declaration)
                && class.members.iter().any(|member| {
                    matches!(member, tsr_ast::ClassElement::IndexSignatureDeclaration(_))
                })
            {
                return true;
            }
        }
        match self.base_symbols_of(owner) {
            None => true,
            Some(bases) => bases
                .into_iter()
                .any(|base| self.chain_declares_index_signature_worker(base, visiting)),
        }
    }

    fn walk_completes(&mut self, owner: SymbolId, visiting: &mut Vec<SymbolId>) -> bool {
        if visiting.contains(&owner) {
            return false;
        }
        visiting.push(owner);
        match self.base_symbols_of(owner) {
            None => false,
            Some(bases) => bases.into_iter().all(|base| self.walk_completes(base, visiting)),
        }
    }

    /// `lateBindMember` / `getPropertyNameFromType` (checker.go, utilities.go):
    /// resolve computed declarations to semantic names. Literal keys use their
    /// values; symbol keys retain this port's existing bracketed identity.
    /// Printing reads the declaration separately. The cache sentinel makes a
    /// recursive key fall back to the early-bound members, as upstream does.
    pub(crate) fn late_bound_members_of(
        &mut self,
        owner: SymbolId,
        is_static: bool,
    ) -> Vec<(String, tsr_ast::NodeId)> {
        let cache_key = (owner, is_static);
        if let Some(members) = self.late_bound_member_names.get(&cache_key) {
            return members.clone();
        }
        self.late_bound_member_names.insert(cache_key, Vec::new());
        self.perf_links.late_bound_active.insert(cache_key);
        let declarations: Vec<tsr_ast::NodeId> =
            self.binder.symbols().get(owner).declarations.iter().copied().collect();
        let mut out = Vec::new();
        for declaration in declarations {
            let member_ids: Vec<tsr_ast::NodeId> = match self.node_map.get(declaration) {
                Some(Node::ClassDeclaration(class)) => {
                    class.members.iter().filter_map(|m| tsr_ast::Node::from(*m).node_id()).collect()
                }
                Some(Node::ClassExpression(class)) => {
                    class.members.iter().filter_map(|m| tsr_ast::Node::from(*m).node_id()).collect()
                }
                Some(Node::InterfaceDeclaration(interface)) => interface
                    .members
                    .iter()
                    .filter_map(|m| tsr_ast::Node::from(*m).node_id())
                    .collect(),
                Some(Node::TypeLiteralNode(literal)) => literal
                    .members
                    .iter()
                    .filter_map(|m| tsr_ast::Node::from(*m).node_id())
                    .collect(),
                Some(Node::ObjectLiteralExpression(literal)) => literal
                    .properties
                    .iter()
                    .filter_map(|member| tsr_ast::Node::from(*member).node_id())
                    .collect(),
                _ => continue,
            };
            for member in member_ids {
                let Some(
                    Node::PropertyDeclaration(&tsr_ast::PropertyDeclaration {
                        name: tsr_ast::PropertyName::ComputedPropertyName(computed),
                        ..
                    })
                    | Node::PropertySignatureDeclaration(&tsr_ast::PropertySignatureDeclaration {
                        name: tsr_ast::PropertyName::ComputedPropertyName(computed),
                        ..
                    })
                    | Node::MethodDeclaration(&tsr_ast::MethodDeclaration {
                        name: tsr_ast::PropertyName::ComputedPropertyName(computed),
                        ..
                    })
                    | Node::MethodSignatureDeclaration(&tsr_ast::MethodSignatureDeclaration {
                        name: tsr_ast::PropertyName::ComputedPropertyName(computed),
                        ..
                    })
                    | Node::GetAccessorDeclaration(&tsr_ast::GetAccessorDeclaration {
                        name: tsr_ast::PropertyName::ComputedPropertyName(computed),
                        ..
                    })
                    | Node::SetAccessorDeclaration(&tsr_ast::SetAccessorDeclaration {
                        name: tsr_ast::PropertyName::ComputedPropertyName(computed),
                        ..
                    }),
                ) = self.node_map.get(member)
                else {
                    continue;
                };
                let member_is_static = self.binder.symbol_of(member).is_some_and(|symbol| {
                    self.property_has_modifier(symbol, tsr_ast::SyntaxKind::StaticKeyword)
                });
                if member_is_static != is_static {
                    continue;
                }
                let Some(expression) = computed.expression else { continue };
                let name_type = self.check_expression(expression);
                let name = self.property_name_from_index(name_type).or_else(|| {
                    self.late_bound_symbol_member_name(computed).map(|(spelled, _)| spelled)
                });
                if let Some(name) = name {
                    out.push((name, member));
                }
            }
        }
        self.late_bound_member_names.insert(cache_key, out.clone());
        self.perf_links.late_bound_active.remove(&cache_key);
        out
    }

    /// resolveAnonymousTypeMembers (checker.go): class values own a static side.
    pub(crate) fn class_static_symbol(&self, id: TypeId) -> Option<tsr_binder::SymbolId> {
        if self.module_value_clones.contains_key(&id) {
            return None;
        }
        let TypeData::Anonymous { symbol, .. } = self.type_of(id).data else { return None };
        let symbol = self.binder.merged_symbol(symbol);
        self.binder
            .symbols()
            .get(symbol)
            .flags
            .contains(tsr_binder::SymbolFlags::CLASS)
            .then_some(symbol)
    }

    /// The names of every property of `id`, own and inherited, or `None` if any
    /// base type could not be followed.
    ///
    /// Ported from `Checker.getPropertiesOfType` → `getPropertiesOfObjectType`
    /// (`internal/checker/checker.go`). Upstream reads a resolved members table
    /// that already has the base types layered in; there is none here, so this
    /// walks the same base-symbol graph [`Checker::get_property_of_declared_symbol`]
    /// walks and collects names instead of resolving one. Names only: the
    /// *symbol* for a name is then taken from
    /// [`Checker::get_property_of_type`], so shadowing is decided in exactly one
    /// place rather than twice.
    ///
    /// The `None`-on-an-unfollowable-base rule is [`Checker::base_symbols_of`]'s
    /// and is why the walk cannot silently under-report a requirement.
    pub(crate) fn get_property_names_of_type(&mut self, id: TypeId) -> Option<Vec<String>> {
        self.property_names_of_type(id).map(PropertyNames::into_vec)
    }

    /// [`Self::get_property_names_of_type`] without copying a memoised list:
    /// a class's or interface's names are shared with
    /// [`PerfLinks::structured_property_names`](crate::perf_links::PerfLinks)
    /// (`r4-perf3.md` §4). Same answer, same work, same side effects.
    pub(crate) fn get_property_names_of_type_shared(
        &mut self,
        id: TypeId,
    ) -> Option<std::rc::Rc<[String]>> {
        self.property_names_of_type(id).map(PropertyNames::into_shared)
    }

    /// [`Self::get_property_names_of_type`]'s enumeration, answering a
    /// memoised list without copying it.
    fn property_names_of_type(&mut self, id: TypeId) -> Option<PropertyNames> {
        // Pinned 5b1047d checker.go:18846/18861: composite enumeration reads
        // completed constituent own tables, then certifies combined properties.
        // Checker-local TypeIds retain alias/receiver identity; temporary name
        // lists publish only as a complete Some, never a recursive assumption.
        // No synthetic symbol, member image or cache is created. Alias/member,
        // index and property workers keep their existing completion boundaries;
        // this traversal and type forcing run per query (tsr-1yb.11), not a speed
        // claim. Unknown tables or unsupported partial/privacy metadata decline.
        if id == self.intrinsics.empty_object || id == self.intrinsics.unknown_empty_object {
            return Some(Vec::new().into());
        }
        let composite = match &self.store.get(id).data {
            TypeData::Union { types, .. } => Some((types.clone(), true)),
            TypeData::Intersection { types, .. } => Some((types.clone(), false)),
            _ => None,
        };
        if let Some((types, is_union)) = composite {
            let mut completed = Vec::with_capacity(types.len());
            let mut candidates = Vec::new();
            for part in types {
                let body = self.binding_type_alias_body(part);
                if self.is_error(body) || body == self.intrinsics.unresolved {
                    return None;
                }
                let apparent = self.apparent_type(body);
                if self.is_error(apparent) || apparent == self.intrinsics.unresolved {
                    return None;
                }
                // Declaration roots do not carry mapped optional modifiers.
                // The type supplier can read those values, but composite
                // metadata is not represented by a synthetic symbol here.
                if self.mapped_types.contains_key(&body)
                    || self.mapped_types.contains_key(&apparent)
                    || self.mapped_identity_optionality.contains_key(&body)
                    || self.mapped_identity_optionality.contains_key(&apparent)
                {
                    return None;
                }
                if apparent != self.intrinsics.empty_object
                    && apparent != self.intrinsics.unknown_empty_object
                {
                    // A lookup-mode boolean is not MembersResolved. This
                    // existing traversal certifies names, including fresh
                    // literal capture, without trusting active placeholders.
                    self.declared_property_table(apparent)?;
                }
                // Complete every table before filtering even a known partial.
                let names = self.get_property_names_of_type(apparent)?;
                for name in &names {
                    if !candidates.contains(name) {
                        candidates.push(name.clone());
                    }
                }
                completed.push((apparent, names));
            }
            let mut names = Vec::new();
            for name in candidates {
                let mut read_partial = false;
                for (part, own_names) in &completed {
                    if own_names.contains(&name) {
                        let image = self
                            .anonymous_properties
                            .get(part)
                            .filter(|(_, instantiated)| *instantiated)
                            .and_then(|(properties, _)| {
                                properties.iter().find(|property| property.name == name)
                            })
                            .cloned();
                        let origin = image
                            .as_ref()
                            .and_then(|property| property.origin)
                            .or_else(|| self.get_property_of_type_ex(*part, &name, true));
                        if let Some(origin) = origin {
                            if self
                                .property_has_modifier(origin, tsr_ast::SyntaxKind::PrivateKeyword)
                                || self.property_has_modifier(
                                    origin,
                                    tsr_ast::SyntaxKind::ProtectedKeyword,
                                )
                                || self
                                    .binder
                                    .symbols()
                                    .get(origin)
                                    .declarations
                                    .iter()
                                    .any(|&node| self.declaration_names_a_private(node))
                            {
                                // Common private declaration identity is not
                                // represented by the existing type supplier.
                                return None;
                            }
                        } else if image.is_none() {
                            return None;
                        }
                        let receiver = if is_union { *part } else { id };
                        let member = self.get_type_of_property_with_this_argument(
                            *part, &name, receiver, true,
                        )?;
                        if self.is_error(member) || member == self.intrinsics.unresolved {
                            return None;
                        }
                        if self.mentions_this_type(member) {
                            // The supplier has not substituted inherited this.
                            return None;
                        }
                    } else if is_union {
                        if *part == self.intrinsics.empty_object
                            || *part == self.intrinsics.unknown_empty_object
                        {
                            read_partial = true;
                            continue;
                        }
                        // None here is incomplete index work, not no indexes.
                        let indexes = self.get_index_infos_of_type(*part)?;
                        if indexes.iter().any(|info| {
                            self.is_error(info.key)
                                || info.key == self.intrinsics.unresolved
                                || self.is_error(info.value)
                                || info.value == self.intrinsics.unresolved
                        }) {
                            return None;
                        }
                        let key = self.store.intern_literal(
                            TypeFlags::STRING_LITERAL,
                            TypeData::StringLiteral(name.clone()),
                            false,
                        );
                        for info in &indexes {
                            if self.relate_ternary(
                                key,
                                info.key,
                                crate::relater::Relation::Assignable,
                            ) == crate::relater::Ternary::Unknown
                            {
                                // The applicable-index bool supplier declines
                                // Unknown; it cannot prove ReadPartial absence.
                                return None;
                            }
                        }
                        if self.get_applicable_index_info(*part, key).is_none() {
                            // Plain declared-object absence is ReadPartial.
                            // Object-literal/spread WritePartial is unrepresented
                            // by the existing semantic composite supplier.
                            let declared = match self.store.get(*part).data {
                                TypeData::Named { members: Some(owner), .. } => {
                                    self.binder.symbols().get(owner).flags.intersects(
                                        SymbolFlags::CLASS
                                            | SymbolFlags::INTERFACE
                                            | SymbolFlags::TYPE_LITERAL,
                                    )
                                }
                                _ => false,
                            } || self.type_literal_origins.contains_key(part)
                                || *part == self.intrinsics.empty_object
                                || *part == self.intrinsics.unknown_empty_object;
                            if !declared {
                                return None;
                            }
                            read_partial = true;
                        }
                    }
                }
                if read_partial {
                    continue;
                }
                // Intersection own enumeration must not augment Object or
                // Function, and polymorphic this uses the entire composite.
                let member =
                    self.get_type_of_property_with_this_argument(id, &name, id, !is_union)?;
                if self.is_error(member) || member == self.intrinsics.unresolved {
                    return None;
                }
                if self.mentions_this_type(member) {
                    return None;
                }
                names.push(name);
            }
            return Some(names.into());
        }
        if let Some(&(alias, source)) = self.module_value_clones.get(&id) {
            let mut names = self.get_property_names_of_type(source)?;
            if self.class_static_symbol(source).is_some() {
                let mut copied = Vec::with_capacity(names.len());
                for name in names {
                    // Class name enumeration supplies this synthetic property;
                    // its instance type has no corresponding binder symbol.
                    if name == "prototype" {
                        copied.push(name);
                        continue;
                    }
                    let member = self.get_property_of_type_ex(source, &name, true)?;
                    if self.is_spreadable_property(member) {
                        copied.push(name);
                    }
                }
                names = copied;
            }
            if self.module_clone_default_symbol(alias).is_some()
                && !names.iter().any(|name| name == "default")
            {
                names.push("default".to_owned());
            }
            return Some(names.into());
        }
        // resolveMappedTypeMembers supplies guaranteed keys of an open keyof
        // map from its apparent object constraint. Sequence apparent types keep
        // their existing tuple/array path; unsupported keys remain unenumerated.
        if self.is_generic_homomorphic_mapped_type(id) && self.apparent_mapped_type(id) == id {
            self.resolve_mapped_type_members(id);
        }
        if let Some((properties, true)) = self.anonymous_properties.get(&id) {
            return Some(
                properties.iter().map(|property| property.name.clone()).collect::<Vec<_>>().into(),
            );
        }
        if let Some(symbol) = self.class_static_symbol(id) {
            let mut names = vec!["prototype".to_owned()];
            return self.collect_static_property_names(symbol, &mut names).then(|| names.into());
        }
        if let TypeData::Anonymous { symbol, .. } = self.type_of(id).data
            && self.binder.symbols().get(symbol).flags.contains(SymbolFlags::MODULE_EXPORTS)
        {
            return Some(
                self.binder
                    .symbols()
                    .get(symbol)
                    .members
                    .iter()
                    .filter(|(_, member)| self.symbol_is_value(**member))
                    .map(|(&name, _)| name.to_owned())
                    .collect::<Vec<_>>()
                    .into(),
            );
        }
        // resolveAnonymousTypeMembers: function, enum and module values expose
        // exports. Class statics were handled above; instance members stay separate.
        if let TypeData::Anonymous { symbol, signature, .. } = self.type_of(id).data
            && (signature
                || self
                    .binder
                    .symbols()
                    .get(symbol)
                    .flags
                    .intersects(SymbolFlags::ENUM | SymbolFlags::VALUE_MODULE))
        {
            let mut names: Vec<_> = self
                .binder
                .symbols()
                .get(symbol)
                .exports
                .iter()
                .filter(|(_, member)| self.symbol_is_value(**member))
                .map(|(&name, _)| name.to_owned())
                .collect();
            if let Some((properties, _)) = self.anonymous_properties.get(&id) {
                for property in properties {
                    if !names.contains(&property.name) {
                        names.push(property.name.clone());
                    }
                }
            }
            return Some(names.into());
        }
        let TypeData::Named { members: Some(owner), .. } = self.type_of(id).data else {
            return None;
        };
        if let Some(names) = self.mapped_alias_literal_key_names(id, owner) {
            return Some(names.into());
        }
        self.structured_property_names(owner).map(PropertyNames::Shared)
    }

    /// A class's or interface's instance property names, own then
    /// inherited: [`Self::collect_structured_property_names`] from the top.
    ///
    /// # Memoised like native's resolved members
    ///
    /// Native resolves a structured type's member list once
    /// (`resolveStructuredTypeMembers` → `resolveObjectTypeMembers`,
    /// `checker.go:19106`, publishing `resolvedProperties`); this port rebuilt
    /// and re-sorted it per query.
    /// [`PerfLinks::structured_property_names`](crate::perf_links::PerfLinks)
    /// keeps the decided lists; key, publication and context are
    /// `docs/parity/notes/r4-perf2.md` §3.
    fn structured_property_names(&mut self, owner: SymbolId) -> Option<std::rc::Rc<[String]>> {
        let binder = self.binder;
        let Some(frames) = self.memo_frames(&binder.symbols().get(owner).declarations, None) else {
            let mut walk = StructuredNamesWalk::uncached();
            return self
                .collect_structured_property_names(owner, &mut walk)
                .then(|| walk.names.into());
        };
        let names = if let Some(names) = self.perf_links.structured_property_names.get(&owner) {
            Some(names.clone())
        } else {
            let mut walk = StructuredNamesWalk::memoised();
            let mark = self.publication_mark();
            let complete = self.collect_structured_property_names(owner, &mut walk);
            let names: std::rc::Rc<[String]> = walk.names.into();
            if complete && !walk.cycle && !walk.unsettled && self.publishable_since(mark) {
                self.perf_links.structured_property_names.insert(owner, names.clone());
            }
            complete.then_some(names)
        };
        self.alias_evaluation_bindings = frames;
        names
    }

    /// The property names resolveMappedTypeMembers (checker.go) gives a
    /// reference to a generic alias whose body is `{ [P in K]: … }` over its own
    /// parameter `K`, when the reference supplies string literal keys:
    /// `Record<"name", T>` has `name`. The reference is owned by the alias
    /// symbol, whose table was never the type's member list, so enumerating
    /// that table would answer no names and let a structured target demand
    /// nothing. Other key shapes keep the existing enumeration.
    fn mapped_alias_literal_key_names(&self, id: TypeId, owner: SymbolId) -> Option<Vec<String>> {
        if !self.binder.symbols().get(owner).flags.contains(SymbolFlags::TYPE_ALIAS) {
            return None;
        }
        let (target, arguments) = self.type_reference_targets.get(&id)?;
        if *target != owner {
            return None;
        }
        let declaration = self.binder.symbols().get(owner).declarations.first().copied()?;
        let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration) else {
            return None;
        };
        let Some(tsr_ast::TypeNode::MappedTypeNode(mapped)) = alias.r#type else { return None };
        if mapped.name_type.is_some() {
            return None;
        }
        let Some(tsr_ast::TypeNode::TypeReferenceNode(constraint)) =
            mapped.type_parameter.and_then(|parameter| parameter.constraint)
        else {
            return None;
        };
        let Some(tsr_ast::EntityName::Identifier(name)) = constraint.type_name else {
            return None;
        };
        if !constraint.type_arguments.is_empty() {
            return None;
        }
        let position = alias
            .type_parameters
            .iter()
            .position(|parameter| parameter.name.is_some_and(|own| own.text == name.text))?;
        self.literal_key_texts(*arguments.get(position)?)
    }

    /// `resolveAnonymousTypeMembers` (`checker.go:20650`, class arm): the
    /// class's static properties are its own exports plus, through
    /// `addInheritedMembers`, the properties of
    /// `getBaseConstructorTypeOfClass` when that is an object, intersection
    /// or type variable. The synthetic prototype is added by the caller. A
    /// base whose names cannot be enumerated, or an `any` base (upstream
    /// adds an `any` index signature instead), answers `false`.
    fn collect_static_property_names(
        &mut self,
        owner: tsr_binder::SymbolId,
        names: &mut Vec<String>,
    ) -> bool {
        let mut own: Vec<_> = self
            .binder
            .symbols()
            .get(owner)
            .exports
            .iter()
            .filter(|&(_, &symbol)| self.symbol_is_value(symbol))
            .map(|(&name, &symbol)| (name.to_owned(), symbol))
            .collect();
        own.extend(self.late_bound_static_members_of(owner));
        // getNamedMembers/compareSymbols (5b1047d): source declaration order
        // within the own table, retaining original symbols and value filtering.
        own.sort_by_cached_key(|&(_, symbol)| self.compare_symbols_key(symbol));
        for (name, _) in own {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        let base = self.get_base_constructor_type_of_class(owner);
        let flags = self.store.get(base).flags;
        if flags.contains(TypeFlags::ANY) {
            return false;
        }
        if !flags.intersects(
            TypeFlags::OBJECT
                | TypeFlags::INTERSECTION
                | TypeFlags::TYPE_PARAMETER
                | TypeFlags::INDEXED_ACCESS,
        ) {
            return true;
        }
        let Some(inherited) = self.get_property_names_of_type(base) else { return false };
        for name in inherited {
            // A `#private` static is lexically scoped to its declaring class
            // body and never inherits (see `get_property_of_anonymous_symbol`).
            if !name.starts_with('#') && !names.contains(&name) {
                names.push(name);
            }
        }
        true
    }

    /// One step of [`Checker::get_property_names_of_type`]'s walk.
    ///
    /// The `visiting` guard is [`Checker::get_property_of_declared_symbol`]'s,
    /// for the same reason: `class A extends B` with `class B extends A` is a
    /// real cycle in the base-type graph. Re-entry contributes nothing rather
    /// than failing — every name reachable through the cycle has already been
    /// collected by the outer visit — and marks the walk cyclic, so its list
    /// is not published.
    ///
    /// A memoised walk reads a base's published list instead of walking it:
    /// a published list met no cycle, so every name the base's own walk would
    /// add here is either already present or appears in the same relative
    /// order in the list (`r4-perf2.md` §3).
    fn collect_structured_property_names(
        &mut self,
        owner: tsr_binder::SymbolId,
        walk: &mut StructuredNamesWalk,
    ) -> bool {
        if walk.visiting.contains(&owner) {
            walk.cycle = true;
            return true;
        }
        if walk.memoised
            && !walk.visiting.is_empty()
            && let Some(published) = self.perf_links.structured_property_names.get(&owner)
        {
            for name in published.iter() {
                if !walk.names.contains(name) {
                    walk.names.push(name.clone());
                }
            }
            return true;
        }
        walk.visiting.push(owner);
        // A members table also holds type parameters, so the value gate is the
        // same one `getPropertyOfType` applies; without it `interface I<T>`
        // would demand a property named `T`.
        let mut own: Vec<_> = self
            .binder
            .symbols()
            .get(owner)
            .members
            .iter()
            .filter(|&(_, &symbol)| self.symbol_is_value(symbol))
            .map(|(&name, &symbol)| (name.to_owned(), symbol))
            .collect();
        // Late-bound own declarations belong to this same ordered partition,
        // not an appended table. Instance and static identities stay separate.
        // `late_bound_members_of` parks an empty list while it computes and
        // marks the entry active; the placeholder's empty answer is not the
        // owner's late-bound names, so the walk's list is not published.
        if walk.memoised && self.perf_links.late_bound_active.contains(&(owner, false)) {
            walk.unsettled = true;
        }
        own.extend(self.late_bound_members_of(owner, false).into_iter().filter_map(
            |(name, declaration)| self.binder.symbol_of(declaration).map(|symbol| (name, symbol)),
        ));
        own.sort_by_cached_key(|&(_, symbol)| self.compare_symbols_key(symbol));
        for (name, _) in own {
            if !walk.names.contains(&name) {
                walk.names.push(name);
            }
        }
        let Some(bases) = self.base_symbols_of_ex(owner, false) else {
            return false;
        };
        bases.into_iter().all(|base| self.collect_structured_property_names(base, walk))
    }

    pub(crate) fn late_bound_static_members_of(
        &mut self,
        owner: SymbolId,
    ) -> Vec<(String, SymbolId)> {
        // resolveAnonymousTypeMembers / getLateBoundSymbol (checker.go):
        // computed static members belong to exports, never instance members.
        self.late_bound_members_of(owner, true)
            .into_iter()
            .filter_map(|(name, declaration)| {
                self.binder.symbol_of(declaration).map(|symbol| (name, symbol))
            })
            .collect()
    }

    /// One step of the walk: `owner`'s own members, then its base types'.
    ///
    /// # The circularity guard is load-bearing
    ///
    /// `class A extends B {}` with `class B extends A {}` is a real cycle in the
    /// base-type graph and the corpus contains such cases deliberately. Upstream
    /// guards it in `resolveBaseTypesOfClass`, which parks a `resolvingEmptyArray`
    /// sentinel in `resolvedBaseTypes` and reports
    /// `Type_0_recursively_references_itself_as_a_base_type` on re-entry. There is
    /// no `resolvedBaseTypes` memo here to park a sentinel in, so the guard is the
    /// **path** — the symbols already on the walk — which is the same question
    /// asked with the state that exists. It is not [`crate::resolution::Resolutions`]:
    /// that stack is keyed on `(symbol, PropertyName)` and is about *type*
    /// resolution, and giving base-type walking a `PropertyName` of its own is a
    /// change to a file this work does not own.
    ///
    /// A cycle answers `None` — a miss — rather than a diagnostic, because the
    /// checker has none (`bd tsr-5e7.6`).
    fn get_property_of_declared_symbol(
        &mut self,
        owner: SymbolId,
        name: &str,
        visiting: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        if visiting.contains(&owner) {
            return None;
        }
        visiting.push(owner);
        if let Some(&found) = self.binder.symbols().get(owner).members.get(name)
            && self.symbol_is_value(found)
        {
            return Some(found);
        }
        // getPropertyOfType searches the resolved member table for every
        // semantic key, including string and numeric late-bound names.
        for (key, member) in self.late_bound_members_of(owner, false) {
            if key == name
                && let Some(symbol) = self.binder.symbol_of(member)
            {
                return Some(symbol);
            }
        }
        for base in self.base_symbols_of(owner)? {
            if let Some(found) = self.get_property_of_declared_symbol(base, name, visiting) {
                return Some(found);
            }
        }
        None
    }

    /// The symbols named by `owner`'s `extends` clauses, in declaration order.
    ///
    /// Ported from `getEffectiveBaseTypeNode` plus `resolveBaseTypesOfClass` /
    /// `resolveBaseTypesOfInterface` (`checker.go`), reduced to naming the base.
    ///
    /// # `None` means "this type's bases are a gap", and it stops the whole lookup
    ///
    /// Returning an empty list for a base this port cannot follow would be worse
    /// than answering nothing: for `interface I extends A, B<number>`, skipping
    /// `B<number>` and answering from `A` gives a property that upstream would have
    /// taken from `B` — the wrong *symbol*, not merely a missing one. So any base
    /// that cannot be followed makes the whole lookup a miss. `owner`'s own members
    /// are already answered above, so a gap in a base never costs a declared
    /// member.
    ///
    /// Three things are gaps:
    ///
    /// - **A base with type arguments** (`extends B<number>`). Upstream
    ///   instantiates; nothing here does (`bd tsr-4sc.7`,
    ///   `crate::declared::get_instantiated_type_reference`), and answering `B`'s
    ///   uninstantiated member would give `T` where upstream gives `number`.
    /// - **A base that is not a plain identifier** (`extends M.B`, `extends
    ///   mixin()`). Upstream resolves the first through `resolveEntityName` and the
    ///   second through `getBaseConstructorTypeOfClass`, which needs `typeof` and
    ///   construct signatures.
    /// - **A name that resolves to something with no members**, an alias among
    ///   them.
    ///
    /// # `implements` is not a base type
    ///
    /// Upstream reads only the `extends` clause for base types
    /// (`getEffectiveBaseTypeNode`); `implements` is checked for conformance and
    /// contributes no members. The token is the only thing that distinguishes the
    /// two clauses.
    ///
    /// # The meaning passed to resolution is narrower than upstream's
    ///
    /// A class's `extends` names an **expression**, which upstream resolves in
    /// value meaning and then reduces with `getBaseConstructorTypeOfClass`. That
    /// path needs `typeof C` and construct signatures, so this resolves the name in
    /// `SymbolFlags::TYPE` meaning instead — upstream's meaning for the *interface*
    /// case, and the one that finds a class or interface declaration in both. The
    /// consequence is that `class C extends someExpression` is a gap; the
    /// declaration form, which is what the corpus is mostly made of, is not.
    pub(crate) fn base_symbols_of(&mut self, owner: SymbolId) -> Option<Vec<SymbolId>> {
        self.base_symbols_of_ex(owner, true)
    }

    /// [`Self::base_symbols_of`], with the type-argument refusal made optional.
    ///
    /// **The refusal belongs to the INSTANCE side only.** A heritage entry's
    /// type arguments decide what `extends B<any>` contributes as a member
    /// table, and this port cannot instantiate one, so the member road must
    /// gap. They decide nothing about the base's **static** side:
    /// `getBaseConstructorTypeOfClass` (`checker.go:17434`) types the heritage
    /// entry's *expression*, and `B` is `typeof B` whatever follows it in
    /// angle brackets.
    ///
    /// So `super()` inside `class D extends B<any>` is `typeof B` upstream and
    /// was a gap here — the caller inherited a conservatism it did not need,
    /// which is `docs/conventions.md` corollary 11 one layer in: not a refusal
    /// whose stated reason is wrong, but a shared helper whose reason is right
    /// for its first caller and wider than the second caller requires. §202.
    ///
    /// # Publication (`getBaseTypes` / `resolvedBaseTypes`, pinned 5b1047d)
    ///
    /// Native resolves a class or interface's bases once and stores them on
    /// the declared type. The answer here is a pure function of the binder and
    /// completed alias targets, so a completed `Some` is kept per (owner,
    /// `refuse_type_arguments`) in `base_symbols`, private to this checker.
    /// `None` is not stored. Nor is an answer computed while a `resolve_alias`
    /// worker is active (`alias_resolving`): its `Resolving` entry answers
    /// `None` provisionally. The saved work is the heritage entity resolution
    /// (`resolve_name`, alias chains) that every member lookup through a base
    /// repeated.
    pub(crate) fn base_symbols_of_ex(
        &mut self,
        owner: SymbolId,
        refuse_type_arguments: bool,
    ) -> Option<Vec<SymbolId>> {
        if let Some(bases) = self.base_symbols.get(&(owner, refuse_type_arguments)) {
            return Some(bases.clone());
        }
        let bases = self.base_symbols_of_ex_worker(owner, refuse_type_arguments)?;
        if self.alias_resolving == 0 {
            self.base_symbols.insert((owner, refuse_type_arguments), bases.clone());
        }
        Some(bases)
    }

    fn base_symbols_of_ex_worker(
        &mut self,
        owner: SymbolId,
        refuse_type_arguments: bool,
    ) -> Option<Vec<SymbolId>> {
        let declarations = self.binder.symbols().get(owner).declarations.clone();
        let mut bases = Vec::new();
        for declaration in declarations {
            let clauses = match self.node_map.get(declaration) {
                Some(Node::ClassDeclaration(node)) => node.heritage_clauses,
                Some(Node::ClassExpression(node)) => node.heritage_clauses,
                Some(Node::InterfaceDeclaration(node)) => node.heritage_clauses,
                // A symbol with a declaration that is neither — a class merged
                // with a namespace, say — contributes no bases from it.
                _ => continue,
            };
            for clause in clauses {
                if clause.token.kind != tsr_ast::SyntaxKind::ExtendsKeyword {
                    continue;
                }
                for base in clause.types {
                    bases.push(self.base_symbol_of_heritage_entry(base, refuse_type_arguments)?);
                }
            }
        }
        Some(bases)
    }

    /// The symbol one `extends` entry names, or `None` if it is a gap.
    pub(crate) fn base_symbol_of_heritage_entry(
        &mut self,
        entry: &tsr_ast::ExpressionWithTypeArguments<'_>,
        refuse_type_arguments: bool,
    ) -> Option<SymbolId> {
        if refuse_type_arguments && !entry.type_arguments.is_empty() {
            return None;
        }
        let symbol = self.heritage_entity_symbol(entry.expression?, SymbolFlags::TYPE)?;
        // A symbol-only member walk cannot apply either explicit arguments or
        // omitted defaults. Route generic bases through their instantiated type.
        if refuse_type_arguments && !self.local_type_parameters_of(symbol).is_empty() {
            return None;
        }
        // Only a class or an interface has a members table to inherit from. A
        // type alias or a type parameter resolving here is a gap, not an empty
        // base: upstream would have expanded the alias.
        self.binder
            .symbols()
            .get(symbol)
            .flags
            .intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE)
            .then_some(symbol)
    }

    /// resolveEntityName for the expression-shaped names in heritage clauses.
    /// Intermediate namespaces and imported aliases are resolved before exports.
    ///
    /// Native publishes the answer in the name's `links.resolvedSymbol`; this
    /// port keeps resolved answers in [`crate::perf_links::PerfLinks`]
    /// (`docs/parity/notes/r4-perf.md` §3). An unresolved answer may come
    /// from an alias still resolving, so it is recomputed.
    pub(crate) fn heritage_entity_symbol(
        &mut self,
        expression: tsr_ast::Expression<'_>,
        meaning: SymbolFlags,
    ) -> Option<SymbolId> {
        let key = expression.node_id().map(|node| (node, meaning.bits()));
        if let Some(key) = key
            && let Some(&symbol) = self.perf_links.heritage_entity_symbols.get(&key)
        {
            return Some(symbol);
        }
        let symbol = self.heritage_entity_symbol_worker(expression, meaning)?;
        if let Some(key) = key {
            self.perf_links.heritage_entity_symbols.insert(key, symbol);
        }
        Some(symbol)
    }

    /// [`Self::heritage_entity_symbol`]'s resolution, without the memo.
    fn heritage_entity_symbol_worker(
        &mut self,
        expression: tsr_ast::Expression<'_>,
        meaning: SymbolFlags,
    ) -> Option<SymbolId> {
        let symbol = match expression {
            tsr_ast::Expression::Identifier(name) => self.binder.resolve_name(
                self.nodes,
                self.node_map,
                name.node_id?,
                name.text,
                meaning | SymbolFlags::ALIAS,
            )?,
            tsr_ast::Expression::PropertyAccessExpression(access) => {
                let owner =
                    self.heritage_entity_symbol(access.expression?, SymbolFlags::NAMESPACE)?;
                let tsr_ast::MemberName::Identifier(name) = access.name? else { return None };
                *self.binder.symbols().get(owner).exports.get(name.text)?
            }
            _ => return None,
        };
        let symbol = self.qualified_alias_target(symbol).unwrap_or(symbol);
        let symbol = self.resolve_alias_fully(symbol);
        let symbol = self.binder.merged_symbol(symbol);
        self.binder.symbols().get(symbol).flags.intersects(meaning).then_some(symbol)
    }

    /// `Checker.symbolIsValueEx` (`checker.go:22095`), **both halves**:
    ///
    /// ```go
    /// return symbol.Flags&ast.SymbolFlagsValue != 0 || symbol.Flags&ast.SymbolFlagsAlias != 0 &&
    ///     c.getSymbolFlagsEx(symbol, …)&ast.SymbolFlagsValue != 0
    /// ```
    ///
    /// # The alias half, and why it was missing
    ///
    /// An alias's own flags carry **no** `VALUE` bit — that is the whole reason
    /// upstream's test has a second disjunct. So a value-ness gate over the raw
    /// flags says no to every `export { x } from "./m"`, and
    /// [`Checker::get_property_of_anonymous_symbol`] turned that `no` into
    /// *"property does not exist"*.
    ///
    /// This was recorded as unported with the reason *"nothing follows aliases
    /// yet (`bd tsr-y4u.12`)"*, and that reason expired:
    /// [`Checker::get_symbol_flags`] is upstream's `getSymbolFlagsEx` and has
    /// followed aliases since — `get_type_of_alias` already takes its own
    /// `VALUE` test over it for exactly this reason. The stale note is what let
    /// the gap survive.
    ///
    /// The gap is not exotic. Every `index.parts.d.ts`-style barrel is a file
    /// of nothing but export specifiers, so `import * as P from "…/parts"` then
    /// `P.Root` reported TS2339 on every member — 1,384 diagnostics on a
    /// 22-package repository, against `tsc`'s zero.
    /// `docs/architecture/checker-notes-diag2.md` §400.
    ///
    /// **`&mut self`, because resolving an alias can create types.** The three
    /// callers all had a `&mut` receiver already.
    pub(crate) fn symbol_is_value(&mut self, symbol: SymbolId) -> bool {
        let flags = self.binder.symbols().get(symbol).flags;
        if flags.intersects(SymbolFlags::VALUE) {
            return true;
        }
        flags.intersects(SymbolFlags::ALIAS)
            && !self.alias_is_type_only(symbol)
            && self.get_symbol_flags(symbol).intersects(SymbolFlags::VALUE)
    }

    /// `getTypeOnlyAliasDeclaration(symbol) != nil`, as
    /// `getSymbolFlagsEx`'s `excludeTypeOnlyMeanings` guard uses it
    /// (`checker.go:16374`):
    ///
    /// ```go
    /// if excludeTypeOnlyMeanings && c.getTypeOnlyAliasDeclaration(symbol) != nil {
    ///     break
    /// }
    /// ```
    ///
    /// `symbolIsValue` passes `includeTypeOnlyMembers: false`, hence
    /// `excludeTypeOnlyMeanings: true` — so a **type-only** alias does not
    /// contribute its target's value-ness, and `a.A` stays an error even though
    /// `A` is a class.
    ///
    /// # This is not optional, and two corpus cases said so
    ///
    /// Following the alias without this gate silently turned
    /// `conformance/exportNamespace3` and `conformance/importEquals2` from
    /// passing to failing — both write `export type { A }` and both expect the
    /// TS2339 a type-only re-export earns. The suite summary said only
    /// `1,898 → 1,896`; `examples/casequery.rs` named them.
    ///
    /// # The walk already existed
    ///
    /// [`Checker::type_only_alias_declaration`] is §121's port of the same
    /// question, and it follows the alias **chain** rather than reading one
    /// declaration — §120 measured 7 wrong lines for stopping at the first hop.
    /// A second, syntax-only copy was written here before that one was found;
    /// it would have been wrong in exactly the way §120 records.
    fn alias_is_type_only(&mut self, symbol: SymbolId) -> bool {
        self.type_only_alias_declaration(symbol).is_some()
    }
}

impl Checker<'_, '_> {
    /// `isReadonlySymbol` (`checker.go:13849`), reduced to the shapes this
    /// binder represents: a property whose declaration carries a `readonly`
    /// modifier, a get-accessor with no set-accessor, an enum member. The
    /// `CheckFlagsReadonly`, `const`-variable and `Object.defineProperty`
    /// arms have no counterpart here yet — each is a *missing rejection* for
    /// the one caller acting on the positive (`relater.rs`'s strict-subtype
    /// readonly rule, `checker-notes-assign.md` §14).
    pub(crate) fn is_readonly_property(&self, symbol: tsr_binder::SymbolId) -> bool {
        let data = self.binder.symbols().get(symbol);
        if data.flags.intersects(tsr_binder::SymbolFlags::ENUM_MEMBER) {
            return true;
        }
        if data.flags.intersects(tsr_binder::SymbolFlags::GET_ACCESSOR)
            && !data.flags.intersects(tsr_binder::SymbolFlags::SET_ACCESSOR)
        {
            return true;
        }
        data.declarations.iter().any(|&declaration| {
            let modifiers = match self.node_map.get(declaration) {
                Some(Node::PropertySignatureDeclaration(p)) => p.modifiers,
                Some(Node::PropertyDeclaration(p)) => p.modifiers,
                _ => return false,
            };
            modifiers.iter().any(|modifier| {
                matches!(
                    modifier,
                    tsr_ast::ModifierLike::Token(token)
                        if token.kind == tsr_ast::SyntaxKind::ReadonlyKeyword
                )
            })
        })
    }
}

impl Checker<'_, '_> {
    /// Whether a property's declaration carries the postfix `?` — the
    /// optionality test that reads syntax, because this binder never writes
    /// `SymbolFlags::OPTIONAL` (the `acdeed5` trap, which once collapsed
    /// optional-property else-branches to `never` through the flag).
    pub(crate) fn property_is_optional(&self, symbol: tsr_binder::SymbolId) -> bool {
        // The binder sets SymbolFlagsOptional only for a `?` postfix; the
        // definite-assignment `!` (`remainder!: string`) is required.
        let question = |token: Option<&tsr_ast::Token<'_>>| {
            token.is_some_and(|t| t.kind == tsr_ast::SyntaxKind::QuestionToken)
        };
        self.binder.symbols().get(symbol).declarations.iter().any(|&declaration| {
            match self.node_map.get(declaration) {
                Some(Node::PropertySignatureDeclaration(p)) => question(p.postfix_token),
                Some(Node::PropertyDeclaration(p)) => question(p.postfix_token),
                Some(Node::MethodSignatureDeclaration(m)) => question(m.postfix_token),
                Some(Node::MethodDeclaration(m)) => question(m.postfix_token),
                Some(Node::ParameterDeclaration(_)) => self.is_optional_declaration(declaration),
                Some(Node::JSDocParameterOrPropertyTag(_)) => {
                    self.is_optional_declaration(declaration)
                }
                _ => false,
            }
        })
    }

    /// isObjectLiteralType (utilities.go): the semantic `ObjectLiteral` flag,
    /// retained by regularization and removed by widening.
    pub(crate) fn is_object_literal_type(&self, id: TypeId) -> bool {
        self.object_literal_spread_flags.contains_key(&id)
    }
}

impl Checker<'_, '_> {
    /// Whether a property's declaration carries the given modifier keyword —
    /// the reader behind the relation's privacy arms
    /// (`checker-notes-assign.md` §16), syntactic for the same reason
    /// [`Checker::property_is_optional`] is.
    pub(crate) fn property_has_modifier(
        &self,
        symbol: tsr_binder::SymbolId,
        kind: tsr_ast::SyntaxKind,
    ) -> bool {
        self.binder.symbols().get(symbol).declarations.iter().any(|&declaration| {
            let modifiers = match self.node_map.get(declaration) {
                Some(Node::PropertySignatureDeclaration(p)) => p.modifiers,
                Some(Node::PropertyDeclaration(p)) => p.modifiers,
                Some(Node::MethodSignatureDeclaration(m)) => m.modifiers,
                Some(Node::MethodDeclaration(m)) => m.modifiers,
                Some(Node::GetAccessorDeclaration(accessor)) => accessor.modifiers,
                Some(Node::SetAccessorDeclaration(accessor)) => accessor.modifiers,
                // A parameter property's modifiers sit on the parameter
                // (`getDeclarationModifierFlagsFromSymbol` reads the value
                // declaration, which is the parameter).
                Some(Node::ParameterDeclaration(parameter)) => parameter.modifiers,
                _ => return false,
            };
            modifiers.iter().any(|modifier| {
                matches!(
                    modifier,
                    tsr_ast::ModifierLike::Token(token) if token.kind == kind
                )
            })
        })
    }
}

#[cfg(test)]
mod property_name_tests {
    use tsr_ast::{HasNodeId, NodeId};
    use tsr_core::Arena;

    use crate::Checker;

    fn with_checker<R>(source: &str, test: impl FnOnce(&mut Checker<'_, '_>, NodeId) -> R) -> R {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "test.ts", text: source },
        );
        let root = parsed.source_file.node_id().unwrap();
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        test(&mut checker, root)
    }

    #[test]
    fn own_member_order_retains_source_order_and_static_instance_boundaries() {
        with_checker(
            "interface Callable<T> { readonly tag: string; method(value: T): T } class Shape { static tag: string; static method(): void {} instance: number }",
            |checker, root| {
                let owner = checker.binder.lookup_local(root, "Callable").unwrap();
                let ty = checker.get_declared_type_of_symbol(owner);
                assert_eq!(checker.get_property_names_of_type(ty).unwrap(), ["tag", "method"]);
                let class = checker.binder.lookup_local(root, "Shape").unwrap();
                let statics = checker.get_type_of_symbol(class);
                assert_eq!(
                    checker.get_property_names_of_type(statics).unwrap(),
                    ["prototype", "tag", "method"]
                );
                let instance = checker.get_declared_type_of_symbol(class);
                assert_eq!(checker.get_property_names_of_type(instance).unwrap(), ["instance"]);
            },
        );
    }

    #[test]
    fn canonical_empty_objects_augment_only_when_allowed_and_keep_the_global_member_identity() {
        with_checker("interface Object { toString(): string }", |checker, root| {
            let object = checker.binder.lookup_local(root, "Object").unwrap();
            let object_type = checker.get_declared_type_of_symbol(object);
            let member = checker.get_property_of_type(object_type, "toString").unwrap();
            for empty in [
                checker.intrinsics.empty_object,
                checker.intrinsics.unknown_empty_object,
                checker.intrinsics.empty_object,
            ] {
                assert_eq!(checker.get_property_of_type_ex(empty, "toString", true), None);
                assert_eq!(checker.get_property_of_type(empty, "toString"), Some(member));
                assert_eq!(checker.get_property_of_type(empty, "missing"), None);
                assert!(
                    checker.call_signatures_of_type(empty).is_some_and(|slots| slots.is_empty())
                );
                assert!(
                    checker
                        .signatures_of_type_kind(empty, crate::signatures::SignatureKind::Construct)
                        .is_some_and(|slots| slots.is_empty()),
                );
                assert_eq!(checker.get_property_of_type_ex(empty, "toString", true), None);
            }
            let other = checker.store.new_named(crate::flags::TypeFlags::OBJECT, "{}".into(), None);
            assert_eq!(checker.get_property_of_type(other, "toString"), None);
            assert!(checker.call_signatures_of_type(other).is_none());
        });
        with_checker("", |checker, _| {
            for strict in [true, false, true] {
                checker.strict_null_checks = strict;
                let unknown = checker.intrinsics.unknown;
                let expected = if strict { unknown } else { checker.intrinsics.empty_object };
                assert_eq!(checker.apparent_type(unknown), expected);
                assert_eq!(
                    checker.apparent_type(checker.intrinsics.non_primitive),
                    checker.intrinsics.empty_object,
                );
                assert_eq!(
                    checker.get_property_of_type(checker.intrinsics.empty_object, "toString"),
                    None,
                    "a missing global Object must not manufacture a property",
                );
            }
        });
    }

    fn names(source: &str, owner: &str) -> Option<Vec<String>> {
        with_checker(source, |checker, root| {
            let symbol = checker.binder.lookup_local(root, owner).unwrap();
            let ty = checker.get_declared_type_of_symbol(symbol);
            let first = checker.get_property_names_of_type(ty);
            assert_eq!(
                first,
                checker.get_property_names_of_type(ty),
                "repeat resolution changed order"
            );
            first
        })
    }

    #[test]
    fn mapped_keys_depend_on_the_concrete_reference_arguments() {
        with_checker(
            "type Keys<K extends string> = { [P in K]: number }; type One = Keys<'one'>; type Two = Keys<'two'>;",
            |checker, root| {
                let one = checker.binder.lookup_local(root, "One").unwrap();
                let two = checker.binder.lookup_local(root, "Two").unwrap();
                let one = checker.get_declared_type_of_symbol(one);
                let two = checker.get_declared_type_of_symbol(two);
                assert_ne!(one, two);
                assert_eq!(checker.get_property_names_of_type(one).unwrap(), ["one"]);
                assert_eq!(checker.get_property_names_of_type(two).unwrap(), ["two"]);
                assert_eq!(checker.get_property_names_of_type(one).unwrap(), ["one"]);
            },
        );
    }

    #[test]
    fn equal_names_do_not_make_instantiated_member_types_equal() {
        with_checker(
            "interface Box<T> { value: T } type TextBox = Box<string>; type NumberBox = Box<number>;",
            |checker, root| {
                for (name, expected) in
                    [("TextBox", "string"), ("NumberBox", "number"), ("TextBox", "string")]
                {
                    let symbol = checker.binder.lookup_local(root, name).unwrap();
                    let ty = checker.get_declared_type_of_symbol(symbol);
                    assert_eq!(checker.get_property_names_of_type(ty).unwrap(), ["value"]);
                    let member = checker.get_type_of_property_of_type(ty, "value").unwrap();
                    assert_eq!(checker.type_to_string(member), expected);
                }
            },
        );
    }

    #[test]
    fn active_late_bound_entry_does_not_complete_the_outer_name_list() {
        with_checker(
            "const key = 'late'; interface Shape { early: number; [key]: string }",
            |checker, root| {
                let owner = checker.binder.lookup_local(root, "Shape").unwrap();
                let ty = checker.get_declared_type_of_symbol(owner);
                // This is the marker late_bound_members_of publishes, with its
                // active count, while its worker runs.
                checker.late_bound_member_names.insert((owner, false), Vec::new());
                checker.perf_links.late_bound_active.insert((owner, false));
                assert_eq!(checker.get_property_names_of_type(ty).unwrap(), ["early"]);
                checker.late_bound_member_names.remove(&(owner, false));
                checker.perf_links.late_bound_active.remove(&(owner, false));
                let mut completed = checker.get_property_names_of_type(ty).unwrap();
                completed.sort();
                assert_eq!(completed, ["early", "late"]);
                // This conservative diagnostic predicate is not MembersResolved.
                assert!(!checker.declared_members_are_complete(ty));
            },
        );
    }

    #[test]
    fn static_and_instance_names_have_separate_cache_domains() {
        with_checker("class Shape { instance: number; static own: string }", |checker, root| {
            let symbol = checker.binder.lookup_local(root, "Shape").unwrap();
            let instance = checker.get_declared_type_of_symbol(symbol);
            let statics = checker.get_type_of_symbol(symbol);
            assert_eq!(checker.get_property_names_of_type(instance).unwrap(), ["instance"]);
            let mut names = checker.get_property_names_of_type(statics).unwrap();
            names.sort();
            assert_eq!(names, ["own", "prototype"]);
        });
    }

    #[test]
    fn inherited_this_member_uses_the_concrete_receiver() {
        with_checker(
            "interface Base { self: this } interface Left extends Base { left: number } interface Right extends Base { right: string } declare const left: Left; declare const right: Right; left.self; right.self; left.self;",
            |checker, root| {
                let _ = root;
                let mut expected = ["Left", "Right", "Left"].into_iter();
                for raw in 0..u32::try_from(checker.nodes.len()).unwrap() {
                    let id = NodeId::new(raw);
                    if checker.nodes.kind(id) != tsr_ast::SyntaxKind::PropertyAccessExpression {
                        continue;
                    }
                    let expression =
                        tsr_ast::Expression::try_from(checker.node_map.get(id).unwrap()).unwrap();
                    let ty = checker.check_expression(expression);
                    assert_eq!(checker.type_to_string(ty), expected.next().unwrap());
                }
                assert_eq!(expected.next(), None);
            },
        );
    }

    #[test]
    fn anonymous_spread_names_come_from_the_semantic_overlay() {
        with_checker(
            "const plain = { x: 1 }; const spread = { ...plain, y: '' };",
            |checker, root| {
                let plain = checker.binder.lookup_local(root, "plain").unwrap();
                let spread = checker.binder.lookup_local(root, "spread").unwrap();
                let plain = checker.get_type_of_symbol(plain);
                let spread = checker.get_type_of_symbol(spread);
                assert_eq!(checker.get_property_names_of_type(plain).unwrap(), ["x"]);
                let mut names = checker.get_property_names_of_type(spread).unwrap();
                names.sort();
                assert_eq!(names, ["x", "y"]);
            },
        );
    }

    #[test]
    fn inherited_name_list_keeps_the_derived_member_symbol() {
        with_checker(
            "interface Base { shared: number } interface Derived extends Base { shared: 1 }",
            |checker, root| {
                let owner = checker.binder.lookup_local(root, "Derived").unwrap();
                let own = checker.binder.symbols().get(owner).members["shared"];
                let ty = checker.get_declared_type_of_symbol(owner);
                assert_eq!(checker.get_property_names_of_type(ty).unwrap(), ["shared"]);
                assert_eq!(checker.get_property_of_type(ty, "shared"), Some(own));
            },
        );
    }

    #[test]
    fn diamond_members_are_unique_and_type_parameters_are_excluded() {
        let mut names = names(
            "interface Base<T> { base: T; shared: number } interface Left extends Base<number> { left: string; shared: number } interface Right extends Base<number> { right: string; shared: number } interface Derived extends Left, Right { own: boolean; shared: number }",
            "Derived",
        ).unwrap();
        names.sort();
        assert_eq!(names, ["base", "left", "own", "right", "shared"]);
    }

    #[test]
    fn late_bound_names_are_combined_with_inherited_and_regular_members() {
        let mut names = names(
            "const key = 'late'; interface Base { base: number } interface Derived extends Base { [key]: string; ['literal']: boolean; regular: number }",
            "Derived",
        ).unwrap();
        names.sort();
        assert_eq!(names, ["base", "late", "literal", "regular"]);
    }

    /// Native 5b1047d spreads the synthetic default wrapper before cloning its
    /// resolved members. Its raw export table still holds excluded methods.
    #[test]
    fn class_module_copy_members_filter_after_source_shadowing() {
        use tsr_ast::{NodeMap, NodeTable};

        struct Library(NodeId);
        impl crate::resolution::ModuleHost for Library {
            fn resolved_module(&self, _: NodeId, specifier: &str) -> Option<NodeId> {
                (specifier == "./lib").then_some(self.0)
            }

            fn module_resolution_found(&self, file: NodeId, specifier: &str) -> bool {
                self.resolved_module(file, specifier).is_some()
            }
        }

        let own = r#"static field = 11; static arrow = (n: number): string => 'field';
static opaque = function({ "value": value }: { value: number }): number { return value; };
static own(n: number): string { return 'method'; }
static get getter(): number { return 13; } static set setter(n: number) {}
private static hidden = 19; protected static guarded = 23;
static readonly fixed = 29; static optional?: number; static #secret = 31;"#;
        let base = "class Base { static baseField = 101; static baseArrow = (n: number): string => 'base'; static baseMethod(n: number): string { return 'base-method'; } static dropShadow = (n: number): string => 'base-field'; static keepShadow(n: number): string { return 'base-method'; } }";
        let overrides = "static dropShadow(n: number): string { return 'derived-method'; } static keepShadow = (n: number): string => 'derived-field';";
        let merged = "namespace Foo { export const mergedField = 'merged'; export function mergedFn(n: number): string { return 'merged'; } export class Box {} export namespace Inner { export const field = 37; } export interface OnlyType { field: number; } export import aliasOwn = Foo.own; }";
        for (library, inherited, namespace, function, incomplete) in [
            (format!("class Foo {{ {own} }} export = Foo;"), false, false, false, false),
            (
                format!("{base} class Foo extends Base {{ {own} {overrides} }} {merged} export = Foo;"),
                true,
                true,
                false,
                false,
            ),
            (
                "function Foo(n: number): string { return 'raw'; } namespace Foo { export const field = 11; export function own(n: number): string { return 'namespace'; } } export = Foo;".to_owned(),
                false,
                true,
                true,
                false,
            ),
            (
                format!("class Foo extends Missing {{ constructor() {{ super(); }} {own} }} export = Foo;"),
                false,
                false,
                false,
                true,
            ),
        ] {
            for strict_null in [false, true] {
                for cold in ["Head", "Tail"] {
                    let arena = Arena::new();
                    let source = "import * as Head from './lib'; import * as Twin from './lib'; import Raw = require('./lib'); import Tail = Linked; import Linked = Head;";
                    let mut nodes = NodeTable::new();
                    let mut map = NodeMap::new();
                    let mut bound = tsr_binder::BindResult::empty();
                    let mut roots = Vec::new();
                    for (name, text) in [("/lib.ts", library.as_str()), ("/use.ts", source)] {
                        let parsed = tsr_parser::parse_into(
                            &arena,
                            text,
                            tsr_parser::ParseOptions::default(),
                            &mut nodes,
                            &mut map,
                        );
                        assert!(parsed.diagnostics.is_empty());
                        roots.push(parsed.source_file.node_id().unwrap());
                        bound = tsr_binder::bind_into(
                            bound,
                            &arena,
                            parsed.source_file,
                            &nodes,
                            tsr_binder::FileInfo { name, text },
                        );
                    }
                    let locals = bound.locals(roots[1]).unwrap();
                    let host = Library(roots[0]);
                    let mut checker = Checker::with_module_host(&bound, &nodes, &map, Some(&host));
                    checker.set_strict_null_checks(strict_null);
                    let cold_type = checker.get_type_of_symbol(locals[cold]);
                    let head = checker.get_type_of_symbol(locals["Head"]);
                    let raw = checker.get_type_of_symbol(locals["Raw"]);
                    let twin = checker.get_type_of_symbol(locals["Twin"]);
                    assert!(checker.module_value_clones.contains_key(&head), "{library}");
                    assert_ne!(head, raw);
                    assert_ne!(head, twin);
                    if cold == "Head" || namespace {
                        assert_eq!(cold_type, head);
                    } else {
                        assert_eq!(cold_type, checker.intrinsics().error);
                    }
                    assert_eq!(checker.get_type_of_property_of_type(head, "default"), Some(raw));
                    for name in ["field", "own"] {
                        assert!(checker.get_property_of_type(raw, name).is_some());
                    }
                    if function {
                        for name in ["field", "own"] {
                            assert_eq!(
                                checker.get_property_of_type(head, name),
                                checker.get_property_of_type(raw, name),
                            );
                        }
                        continue;
                    }
                    let mut retained = vec!["field", "arrow", "opaque", "hidden", "guarded", "fixed", "optional"];
                    let mut excluded = vec!["own", "getter", "setter"];
                    if inherited {
                        retained.extend(["baseField", "baseArrow", "keepShadow"]);
                        excluded.extend(["baseMethod", "dropShadow"]);
                    }
                    if namespace {
                        retained.extend(["mergedField", "mergedFn", "Box", "Inner", "aliasOwn"]);
                    }
                    for name in &retained {
                        let original = checker.get_property_of_type(raw, name).unwrap();
                        for copy in [head, twin] {
                            assert_eq!(checker.get_property_of_type(copy, name), Some(original), "{name}");
                        }
                    }
                    for name in &excluded {
                        let original = checker.get_property_of_type(raw, name).unwrap();
                        assert!(!checker.is_spreadable_property(original));
                        assert_eq!(checker.get_property_of_type(head, name), None, "{name}");
                        assert_eq!(checker.get_property_of_type(twin, name), None, "{name}");
                    }
                    assert_eq!(checker.get_property_of_type(head, "OnlyType"), None);
                    assert_eq!(
                        checker.get_type_of_property_of_type(head, "opaque"),
                        Some(checker.intrinsics().error),
                        "a known retained field with an unreadable signature is not absent",
                    );
                    let prototype = checker.get_type_of_property_of_type(raw, "prototype");
                    assert!(prototype.is_some());
                    assert_eq!(checker.get_type_of_property_of_type(head, "prototype"), prototype);
                    if incomplete {
                        assert_eq!(checker.get_property_names_of_type(head), None);
                    } else {
                        retained.extend(["default", "prototype"]);
                        retained.sort_unstable();
                        let mut copied = checker.get_property_names_of_type(head).unwrap();
                        copied.sort_unstable();
                        assert_eq!(copied, retained);
                        assert_eq!(
                            checker.declared_members_are_complete(head),
                            !inherited,
                            "filtering does not widen completeness",
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn cycles_terminate_but_unfollowable_bases_do_not_become_empty() {
        let mut cyclic =
            names("interface A extends B { a: number } interface B extends A { b: string }", "A")
                .unwrap();
        cyclic.sort();
        assert_eq!(cyclic, ["a", "b"]);
        assert_eq!(
            names("type Alias = number; interface A extends Alias { a: number }", "A"),
            None
        );
    }

    #[test]
    fn composite_names_complete_constituents_in_cold_reverse_and_warm_order() {
        let source = "interface Left { left: string; shared?: number } interface Right { right: boolean; shared: 13 } type Either = Left | Right; type Both = Left & Right;";
        for order in [["Either", "Both", "Either"], ["Both", "Either", "Both"]] {
            with_checker(source, |checker, root| {
                for owner in order {
                    let symbol = checker.binder.lookup_local(root, owner).unwrap();
                    let ty = checker.get_declared_type_of_symbol(symbol);
                    let mut names = checker.get_property_names_of_type(ty).unwrap();
                    names.sort();
                    assert_eq!(
                        names,
                        if owner == "Either" {
                            vec!["shared"]
                        } else {
                            vec!["left", "right", "shared"]
                        }
                    );
                }
            });
        }
    }

    #[test]
    fn composite_union_names_accept_only_applicable_indexes() {
        let mut found = names(
            "interface Named { 'data-west': string; local: number } interface Indexed { [key: `data-${string}`]: boolean } type Either = Named | Indexed;",
            "Either",
        ).unwrap();
        found.sort();
        assert_eq!(found, ["data-west"]);
        assert_eq!(names(
            "interface Named { 7: string } interface Indexed { [key: number]: boolean } type Either = Named | Indexed;",
            "Either",
        ).unwrap(), ["7"]);
        assert_eq!(names(
            "interface Left { west: string } interface Right { east: number } type Either = Left | Right;",
            "Either",
        ).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn composite_unknown_index_relation_is_not_certified_read_partial() {
        with_checker("interface Named { value: number } interface Indexed {}", |checker, root| {
            let named = checker.binder.lookup_local(root, "Named").unwrap();
            let indexed = checker.binder.lookup_local(root, "Indexed").unwrap();
            let named = checker.get_declared_type_of_symbol(named);
            let indexed = checker.get_declared_type_of_symbol(indexed);
            let key =
                checker.store.new_named(crate::flags::TypeFlags::OBJECT, "opaque".to_owned(), None);
            // Existing table with an unsupported key stands for incomplete
            // index preparation: the bool supplier cannot prove applicability
            // but that does not make the union property's absence completed.
            checker.object_literal_index_infos.insert(
                indexed,
                vec![crate::index_signatures::IndexInfo {
                    key,
                    value: checker.intrinsics.number,
                    readonly: true,
                    declaration: None,
                    components: None,
                }],
            );
            let union = checker.get_union_type_without_reduction(&[named, indexed]);
            assert_eq!(checker.get_property_names_of_type(union), None);
        });
    }

    #[test]
    fn composite_this_member_keeps_the_whole_intersection_receiver() {
        with_checker(
            "interface Left { self: this; left: number } interface Right { self: this; right: string } type Both = Left & Right;",
            |checker, root| {
                let owner = checker.binder.lookup_local(root, "Both").unwrap();
                let ty = checker.get_declared_type_of_symbol(owner);
                let mut names = checker.get_property_names_of_type(ty).unwrap();
                names.sort();
                assert_eq!(names, ["left", "right", "self"]);
                let member =
                    checker.get_type_of_property_with_this_argument(ty, "self", ty, true).unwrap();
                let mut receiver_names = checker.get_property_names_of_type(member).unwrap();
                receiver_names.sort();
                assert_eq!(receiver_names, ["left", "right", "self"]);
            },
        );
        // The existing supplier maps direct owners, but not a base owner's
        // minted this through a derived non-generic receiver. Do not certify
        // the previously observed wrong Base result as a completed property.
        assert_eq!(
            names(
                "interface Base { self: this } interface Left extends Base { left: number } interface Right extends Base { right: string } type Both = Left & Right;",
                "Both",
            ),
            None
        );
    }

    #[test]
    fn composite_unknown_owners_and_unreadable_types_do_not_become_empty() {
        use crate::{flags::TypeFlags, types::TypeData};
        with_checker("interface Known { value: number }", |checker, root| {
            let owner = checker.binder.lookup_local(root, "Known").unwrap();
            let known = checker.get_declared_type_of_symbol(owner);
            assert_eq!(
                checker.get_property_names_of_type(checker.intrinsics.empty_object),
                Some(Vec::new())
            );
            assert_eq!(
                checker.get_property_names_of_type(checker.intrinsics.unknown_empty_object),
                Some(Vec::new())
            );
            let both =
                checker.get_intersection_type(&[known, checker.intrinsics.empty_object], None);
            assert_eq!(checker.get_property_names_of_type(both).unwrap(), ["value"]);
            let either =
                checker.get_union_type_without_reduction(&[known, checker.intrinsics.empty_object]);
            assert_eq!(checker.get_property_names_of_type(either).unwrap(), Vec::<String>::new());
            let unknown = checker.store.new_named(TypeFlags::OBJECT, "opaque".to_owned(), None);
            assert_eq!(checker.get_property_names_of_type(unknown), None);
            for unfinished in [unknown, checker.intrinsics.error, checker.intrinsics.unresolved] {
                for types in [vec![known, unfinished], vec![unfinished, known]] {
                    let composite = checker.store.intern_intersection(
                        TypeFlags::INTERSECTION,
                        TypeData::Intersection { text: "probe".to_owned(), types, symbol: None },
                    );
                    assert_eq!(checker.get_property_names_of_type(composite), None);
                }
            }
        });
        assert_eq!(
            names(
                "interface Known { value: number } interface Incomplete extends Missing { other: string } type Both = Known & Incomplete;",
                "Both",
            ),
            None
        );
        assert_eq!(
            names(
                "interface Known { value: number } interface Unreadable { bad: Missing } type Both = Known & Unreadable;",
                "Both",
            ),
            None
        );
    }

    #[test]
    fn composite_own_names_exclude_prototypes_and_decline_mapped_metadata() {
        let mut found = names(
            "interface Object { toString(): string } interface Function { apply(): number } interface Callable { (): number; field: string } interface Other { other: boolean } type Both = Callable & Other;",
            "Both",
        ).unwrap();
        found.sort();
        assert_eq!(found, ["field", "other"]);
        assert_eq!(
            names(
                "interface Written { readonly value?: number } interface Other { other: string } type Maybe<T> = { readonly [K in keyof T]?: T[K] }; type Both = Maybe<Written> & Other;",
                "Both",
            ),
            None
        );
    }

    #[test]
    fn composite_private_and_write_partial_metadata_stay_declined() {
        assert_eq!(
            names(
                "class Hidden { private secret: number; shared: string } interface Public { shared: string } type Either = Hidden | Public;",
                "Either",
            ),
            None
        );
        assert_eq!(
            names(
                "class Hidden { protected secret: number; shared: string } interface Public { shared: string } type Both = Hidden & Public;",
                "Both",
            ),
            None
        );
        with_checker("const empty = {}; const field = { value: 13 };", |checker, root| {
            let empty = checker.binder.lookup_local(root, "empty").unwrap();
            let field = checker.binder.lookup_local(root, "field").unwrap();
            let empty = checker.get_type_of_symbol(empty);
            let field = checker.get_type_of_symbol(field);
            let either = checker.get_union_type_without_reduction(&[empty, field]);
            assert_eq!(checker.get_property_names_of_type(either), None);
        });
    }

    #[test]
    fn composite_enumeration_does_not_publish_active_late_bound_names() {
        with_checker(
            "const key = 'late'; interface Left { early: number; [key]: string } interface Right { other: boolean } type Both = Left & Right;",
            |checker, root| {
                let left = checker.binder.lookup_local(root, "Left").unwrap();
                checker.late_bound_member_names.insert((left, false), Vec::new());
                checker.perf_links.late_bound_active.insert((left, false));
                let both = checker.binder.lookup_local(root, "Both").unwrap();
                let both = checker.get_declared_type_of_symbol(both);
                assert_eq!(checker.get_property_names_of_type(both), None);
                checker.late_bound_member_names.remove(&(left, false));
                checker.perf_links.late_bound_active.remove(&(left, false));
                // The existing completion contract does not certify computed
                // names even when a separate lookup has forced their value.
                assert_eq!(checker.get_property_names_of_type(both), None);
            },
        );
    }
}

#[cfg(test)]
mod completed_array_placeholder_tests {
    use super::*;
    use tsr_ast::NodeId;

    // Native 5b1047d1: both query orders produce number for child length and
    // number[] for map. The recursive child retains its original Tree identity.
    const LIB: &str = "interface Array<T> { [index: number]: T; length: number; map<U>(fn: (value: T) => U): U[]; }
interface ReadonlyArray<T> { readonly [index: number]: T; readonly length: number; }";

    fn with_checker(source: &str, test: impl FnOnce(&mut Checker<'_, '_>, NodeId)) {
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let root = Node::SourceFile(parsed.source_file).node_id().unwrap();
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "tree.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        checker.set_strict_null_checks(true);
        test(&mut checker, root);
    }

    fn value(checker: &mut Checker<'_, '_>, root: NodeId, name: &str) -> TypeId {
        let symbol = checker.binder.lookup_local(root, name).unwrap();
        checker.get_type_of_symbol(symbol)
    }

    fn publication(checker: &Checker<'_, '_>) -> String {
        format!(
            "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
            checker.store,
            checker.alias_placeholders,
            checker.declared_types,
            checker.alias_body_evaluations,
            checker.instantiations,
            checker.type_reference_targets,
            checker.symbol_types,
        )
    }

    #[test]
    fn child_length_and_map_return_match_native_in_cold_reverse_and_warm_orders() {
        let source = format!(
            "{LIB} type Tree = [string, Tree][]; declare const tree: Tree;
type ReadTree = readonly [number, ReadTree][]; declare const readTree: ReadTree;
const direct = tree[0][1].length; const mapped = tree.map(([label, child]) => child.length);
const child = tree[0][1]; const readLength = readTree[0][1].length;"
        );
        for first in ["direct", "mapped", "readLength"] {
            with_checker(&source, |checker, root| {
                value(checker, root, first);
                for order in [["direct", "readLength"], ["readLength", "direct"]] {
                    for name in order {
                        assert_eq!(value(checker, root, name), checker.intrinsics.number);
                    }
                    let mapped = value(checker, root, "mapped");
                    assert_eq!(
                        checker.type_reference_targets[&mapped].1,
                        [checker.intrinsics.number]
                    );
                }
                let owner = checker.binder.lookup_local(root, "Tree").unwrap();
                let placeholder = checker.alias_placeholders[&owner];
                let original = checker.declared_types[&owner];
                let tuple = checker.type_reference_targets[&original].1[0];
                assert_eq!(checker.tuple_element_lists[&tuple].0[1], placeholder);
                assert_eq!(value(checker, root, "child"), placeholder);
                assert_ne!(placeholder, original);
                checker.check_source_file(
                    root,
                    crate::check::FileContext { ambient: false, has_parse_errors: false },
                );
                assert!(checker.diagnostics().is_empty());
                let before = publication(checker);
                for _ in 0..3 {
                    assert_eq!(
                        checker.completed_array_placeholder_length_body(placeholder),
                        Some(checker.without_alias(original))
                    );
                    assert_eq!(publication(checker), before);
                    assert_eq!(
                        checker.get_type_of_property_with_this_argument(
                            placeholder,
                            "length",
                            placeholder,
                            true
                        ),
                        Some(checker.intrinsics.number),
                    );
                    assert_eq!(publication(checker), before);
                }
            });
        }
    }

    #[test]
    fn unfinished_foreign_ambiguous_and_captured_owners_stay_refused() {
        let source = format!(
            "{LIB} type Tree = [string, Tree][]; type Other = string; declare const tree: Tree;
function outer<X>() {{ type Local = [X, Local][]; let local!: Local; return local; }}"
        );
        with_checker(&source, |checker, root| {
            value(checker, root, "tree");
            let owner = checker.binder.lookup_local(root, "Tree").unwrap();
            let placeholder = checker.alias_placeholders[&owner];
            let completed = checker.declared_types[&owner];
            for incomplete in [
                None,
                Some(placeholder),
                Some(checker.intrinsics.error),
                Some(checker.intrinsics.unresolved),
            ] {
                if let Some(incomplete) = incomplete {
                    checker.declared_types.insert(owner, incomplete);
                } else {
                    checker.declared_types.remove(&owner);
                }
                let before = publication(checker);
                assert_eq!(checker.get_type_of_property_of_type(placeholder, "length"), None);
                assert_eq!(publication(checker), before);
            }
            checker.declared_types.insert(owner, completed);
            let target = checker.type_reference_targets[&completed].0;
            for context in 0..7 {
                match context {
                    0 => assert!(
                        checker
                            .resolutions
                            .push(owner, crate::resolution::PropertyName::DeclaredType)
                    ),
                    1 => checker.alias_evaluation_bindings.push(rustc_hash::FxHashMap::default()),
                    2 => checker.mapped_template_depth = 1,
                    3 => checker.instantiation_depth = 1,
                    4 => checker.identity_unmapped_type_parameters = true,
                    5 => checker.render_type_parameter_scope.push(("foreign".into(), owner)),
                    _ => assert!(
                        checker
                            .resolutions
                            .push(target, crate::resolution::PropertyName::DeclaredType)
                    ),
                }
                let before = publication(checker);
                assert_eq!(checker.get_type_of_property_of_type(placeholder, "length"), None);
                assert_eq!(publication(checker), before);
                match context {
                    1 => {
                        checker.alias_evaluation_bindings.pop();
                    }
                    2 => checker.mapped_template_depth = 0,
                    3 => checker.instantiation_depth = 0,
                    4 => checker.identity_unmapped_type_parameters = false,
                    5 => {
                        checker.render_type_parameter_scope.pop();
                    }
                    _ => assert!(checker.resolutions.pop()),
                }
            }
            let foreign = checker.store.new_named(TypeFlags::OBJECT, "Tree".into(), None);
            assert_eq!(checker.get_type_of_property_of_type(foreign, "length"), None);
            let other = checker.binder.lookup_local(root, "Other").unwrap();
            checker.alias_placeholders.insert(other, placeholder);
            let before = publication(checker);
            assert_eq!(checker.get_type_of_property_of_type(placeholder, "length"), None);
            assert_eq!(publication(checker), before);
            checker.alias_placeholders.remove(&other);
            let key = checker.type_reference_targets[&completed].clone();
            checker
                .type_reference_targets
                .insert(completed, (key.0, vec![checker.intrinsics.number]));
            let before = publication(checker);
            assert_eq!(checker.get_type_of_property_of_type(placeholder, "length"), None);
            assert_eq!(publication(checker), before);
            checker.type_reference_targets.insert(completed, key);
            let mut nodes = vec![checker.node_map.get(root).unwrap()];
            let local = loop {
                let node = nodes.pop().unwrap();
                if let Node::VariableDeclaration(declaration) = node
                    && matches!(declaration.name, Some(tsr_ast::BindingName::Identifier(name)) if name.text == "local")
                {
                    break checker.binder.symbol_of(declaration.node_id.unwrap()).unwrap();
                }
                tsr_ast::push_children(node, &mut nodes);
            };
            checker.get_type_of_symbol(local);
            let local_owner = checker
                .alias_placeholders
                .iter()
                .find(|(symbol, _)| checker.binder.symbols().get(**symbol).name == "Local")
                .unwrap()
                .0;
            let local_placeholder = checker.alias_placeholders[local_owner];
            let before = publication(checker);
            assert_eq!(checker.get_type_of_property_of_type(local_placeholder, "length"), None);
            assert_eq!(publication(checker), before);
        });
    }

    #[test]
    fn other_members_and_this_dependent_length_do_not_use_the_view() {
        for annotation in ["number", "T", "this"] {
            let source = format!(
                "interface Array<T> {{ [index: number]: T; length: {annotation}; other: number; }}
type Tree = [string, Tree][]; declare const tree: Tree;"
            );
            with_checker(&source, |checker, root| {
                value(checker, root, "tree");
                let owner = checker.binder.lookup_local(root, "Tree").unwrap();
                let placeholder = checker.alias_placeholders[&owner];
                for name in ["other", "0", "map", "missing"] {
                    let before = publication(checker);
                    assert_eq!(checker.get_type_of_property_of_type(placeholder, name), None);
                    assert_eq!(publication(checker), before);
                }
                if annotation != "number" {
                    let before = publication(checker);
                    assert_eq!(checker.get_type_of_property_of_type(placeholder, "length"), None);
                    assert_eq!(publication(checker), before);
                }
            });
        }
    }

    #[test]
    fn unsupported_written_roots_do_not_certify_length() {
        for body in ["[string, Tree][]", "Array<[string, Tree]>", "([string, Tree][])"] {
            let source = format!("{LIB} type Tree = {body}; declare const tree: Tree;");
            with_checker(&source, |checker, root| {
                value(checker, root, "tree");
                let owner = checker.binder.lookup_local(root, "Tree").unwrap();
                let placeholder = checker.alias_placeholders[&owner];
                let before = publication(checker);
                if body == "[string, Tree][]" {
                    assert!(checker.completed_array_placeholder_length_body(placeholder).is_some());
                } else {
                    assert!(checker.completed_array_placeholder_length_body(placeholder).is_none());
                }
                assert_eq!(publication(checker), before);
            });
        }
    }
}

/// [`Checker::property_names_of_type`]'s answer: a list built for this
/// query, or a memoised one shared with
/// [`PerfLinks::structured_property_names`](crate::perf_links::PerfLinks).
enum PropertyNames {
    Owned(Vec<String>),
    Shared(std::rc::Rc<[String]>),
}

impl PropertyNames {
    fn into_vec(self) -> Vec<String> {
        match self {
            Self::Owned(names) => names,
            Self::Shared(names) => names.to_vec(),
        }
    }

    fn into_shared(self) -> std::rc::Rc<[String]> {
        match self {
            Self::Owned(names) => names.into(),
            Self::Shared(names) => names,
        }
    }
}

impl From<Vec<String>> for PropertyNames {
    fn from(names: Vec<String>) -> Self {
        Self::Owned(names)
    }
}

/// The state of one [`Checker::collect_structured_property_names`] walk.
struct StructuredNamesWalk {
    names: Vec<String>,
    visiting: Vec<SymbolId>,
    /// Whether a base re-entered the walk (a cyclic base graph).
    cycle: bool,
    /// Whether an owner's late-bound names may have been read while their
    /// computation was still running.
    unsettled: bool,
    /// Whether published base lists may be read: only when the request was
    /// admitted by [`Checker::memo_frames`].
    memoised: bool,
}

impl StructuredNamesWalk {
    const fn memoised() -> Self {
        Self {
            names: Vec::new(),
            visiting: Vec::new(),
            cycle: false,
            unsettled: false,
            memoised: true,
        }
    }

    const fn uncached() -> Self {
        Self {
            names: Vec::new(),
            visiting: Vec::new(),
            cycle: false,
            unsettled: false,
            memoised: false,
        }
    }
}
