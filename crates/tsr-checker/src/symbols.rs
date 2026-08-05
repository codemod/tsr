//! What type a *symbol* has.
//!
//! Ported from `Checker.getTypeOfSymbol` (`checker.go:16493`) and the
//! variable/parameter/property worker beneath it. Split out of
//! [`crate::checker`] because the largest single item on the histogram is the
//! arm this module does **not** have — `getTypeOfFuncClassEnumModule`
//! (`bd tsr-4sc.8`).
//!
//! Not to be confused with [`crate::declared`], which answers what a *type*
//! symbol declares. A class `C` **declares** the instance type `C` and **has**
//! the type `typeof C`.

use tsr_ast::{Expression, Node, NodeFlags, NodeId, SyntaxKind, TypeNode};
use tsr_binder::{SymbolFlags, SymbolId};

use crate::{checker::Checker, flags::TypeFlags, resolution::PropertyName, types::TypeId};

impl<'a> Checker<'a, '_> {
    /// The type of a symbol.
    ///
    /// Ported from `Checker.getTypeOfSymbol` (`checker.go:16493`). Upstream
    /// dispatches on nine symbol shapes; this slice ports the
    /// variable/parameter/property one and returns `errorType` for the rest.
    ///
    /// **`errorType`, not `anyType`.** Both print `any`, and only one of them is
    /// a claim that the answer *is* `any`. Every unported shape must be
    /// distinguishable from a computed answer or the conformance suite cannot
    /// tell a gap from a result.
    pub fn get_type_of_symbol(&mut self, symbol: SymbolId) -> TypeId {
        let flags = self.binder.symbols().get(symbol).flags;
        // `checker.go:16506`, and it is the **first** flags branch upstream
        // takes — before variable/property and before function/method.
        //
        // `getTypeOfAccessors` is unported, so this answers `errorType`; the
        // point of the branch is *where* it sits. Omitting it let an accessor
        // that merges with a method — `interface I { get x(): number; x():
        // number; set x(value: number) }`, which carries
        // `METHOD | GET_ACCESSOR | SET_ACCESSOR` — fall through to
        // `getTypeOfFuncClassEnumModule` and print `() => number` where upstream
        // prints `number`. A wrong answer produced by a *missing dispatch arm*
        // rather than by a wrong one, which is why no fixture built from a
        // single accessor could reproduce it.
        //
        // Only the second boundary is observable. Placed after variable/property
        // instead, an accessor merged with a *property* would take the variable
        // worker and reach the same `errorType` by a different route, so no test
        // can tell those two orders apart — it is upstream's order because it is
        // upstream's, and it starts to matter when `getTypeOfAccessors` lands.
        if flags.intersects(SymbolFlags::ACCESSOR) {
            return self.intrinsics.error;
        }
        if flags.intersects(SymbolFlags::VARIABLE | SymbolFlags::PROPERTY) {
            return self.get_type_of_variable_or_parameter_or_property(symbol);
        }
        // `checker.go:16511`. The flags test above stopped being decorative when
        // this arm landed: it is now the thing that keeps a function symbol out
        // of the variable worker, whose declaration-kind match would reject it.
        if flags.intersects(
            SymbolFlags::FUNCTION
                | SymbolFlags::METHOD
                | SymbolFlags::CLASS
                | SymbolFlags::ENUM
                | SymbolFlags::VALUE_MODULE,
        ) {
            return self.get_type_of_func_class_enum_module(symbol);
        }
        // `checker.go:16515`, and it must sit *after* the arm above: an enum's
        // own symbol carries `ENUM`, its members carry `ENUM_MEMBER`, and the
        // two are different questions — `E` has `typeof E`, `E.A` has `E.A`.
        if flags.intersects(SymbolFlags::ENUM_MEMBER) {
            return self.get_type_of_enum_member(symbol);
        }
        // Unported: accessors, aliases, and the four `CheckFlags` shapes
        // upstream tests first (deferred, instantiated, mapped, reverse-mapped).
        self.intrinsics.error
    }

    /// The type of an enum member symbol.
    ///
    /// Ported from `Checker.getTypeOfEnumMember` (`checker.go:18503`) — a memo
    /// over `getDeclaredTypeOfEnumMember` (`checker.go:23927`), which is itself
    /// almost entirely a *side effect*: it forces the parent enum's declared
    /// type, and building that union is what assigns each member symbol its own
    /// declared type ([`Checker::get_declared_type_of_symbol`], via
    /// `getDeclaredTypeOfEnum` at `checker.go:23874`). The member types are not
    /// built here and must not be, or an enum would have two sets of member
    /// identities — the union's and this one's.
    ///
    /// # The name guard, and why it is not upstream's
    ///
    /// Upstream's node builder prints a member as `E.A` only when the member
    /// name is identifier text, and as `(typeof E)["fo'o"]` otherwise —
    /// `enumWithQuotedElementName2.types` records exactly that line. The member
    /// type's printed form is fixed when the union is built, so this port cannot
    /// choose between the two forms at print time (see
    /// [`crate::printing::type_to_string`]) and would emit `E.fo'o`, which is a
    /// wrong line where a gap belongs. A non-identifier member name therefore
    /// answers `errorType` here.
    fn get_type_of_enum_member(&mut self, symbol: SymbolId) -> TypeId {
        if let Some(&cached) = self.symbol_types.get(&symbol) {
            return cached;
        }
        let computed = self.get_declared_type_of_enum_member(symbol);
        self.symbol_types.insert(symbol, computed);
        computed
    }

    /// Ported from `Checker.getDeclaredTypeOfEnumMember` (`checker.go:23927`).
    ///
    /// Upstream reads `links.declaredType` again *after* forcing the parent,
    /// because forcing it is what fills the link in; the second read is not
    /// redundant and the `unwrap_or` below is upstream's fallback for a member
    /// the enum did not claim, not a guess.
    ///
    /// **A hazard this port has and upstream does not.**
    /// [`Checker::get_declared_type_of_symbol`] has no `ENUM_MEMBER` arm, so
    /// asking it about a member symbol *first* would cache `errorType` against
    /// that member and this function would then return it. Nothing reaches that
    /// today — `E.A` in type position is a qualified name and unported — but the
    /// order is load-bearing rather than incidental.
    fn get_declared_type_of_enum_member(&mut self, symbol: SymbolId) -> TypeId {
        if let Some(&cached) = self.declared_types.get(&symbol) {
            return cached;
        }
        if !is_identifier_text(self.binder.symbols().get(symbol).name) {
            return self.intrinsics.error;
        }
        let Some(parent) = self.binder.symbols().get(symbol).parent else {
            return self.intrinsics.error;
        };
        let enum_type = self.get_declared_type_of_symbol(parent);
        self.declared_types.get(&symbol).copied().unwrap_or(enum_type)
    }

    /// The type of a function, method, class, enum or value-module symbol.
    ///
    /// Ported from `Checker.getTypeOfFuncClassEnumModule` (`checker.go:16904`),
    /// sharing the memo upstream shares — `valueSymbolLinks.resolvedType`, which
    /// is [`Checker::symbol_types`] here.
    ///
    /// # A resolution frame upstream does not need here
    ///
    /// Upstream's worker creates an *empty* anonymous object type and resolves
    /// its signatures only when something asks; the recursion guard lives in
    /// `getReturnTypeOfSignature` (`checker.go:20004`) instead. This port
    /// computes a named type's printed form once, at creation
    /// ([`crate::types::TypeData::Named`]), so building the type *is* resolving
    /// the signature and the guard has to be here. The frame is a consequence of
    /// that divergence rather than an invention: without it, eagerly printing a
    /// signature that reached its own symbol would not return.
    fn get_type_of_func_class_enum_module(&mut self, symbol: SymbolId) -> TypeId {
        if let Some(&cached) = self.symbol_types.get(&symbol) {
            return cached;
        }
        if !self.resolutions.push(symbol, PropertyName::Type) {
            return self.intrinsics.error;
        }
        let computed = self.get_type_of_func_class_enum_module_worker(symbol);
        let computed = if self.resolutions.pop() { computed } else { self.intrinsics.error };
        self.symbol_types.insert(symbol, computed);
        computed
    }

    /// Ported from `Checker.getTypeOfFuncClassEnumModuleWorker`
    /// (`checker.go:16912`) and the node builder's decision about how the
    /// resulting anonymous object type prints —
    /// `NodeBuilderImpl.shouldEmitTypeOfSymbol` (`nodebuilderimpl.go:2801`) into
    /// `createAnonymousTypeNodeEx` (`nodebuilderimpl.go:2812`).
    ///
    /// # `typeof C` and `(x: string) => void` are one type shape, printed twice
    ///
    /// Upstream builds the same thing for all five symbol kinds: an anonymous
    /// object type whose symbol is this one. What differs is the *node builder*.
    /// A class, enum or value-module symbol takes `symbolToTypeNode` with
    /// `SymbolFlagsValue`, which is a type query — `typeof C`. A function or
    /// method symbol does not, because `shouldWriteTypeOfFunctionSymbol`
    /// (`nodebuilderimpl.go:2760`) requires `FlagsUseTypeOfFunction` and the
    /// `.types` baseline writer does not set it — so it expands structurally to
    /// its call signatures. `typeof X` is therefore not a separate feature from
    /// this one; it is this feature's printed form for three of its five kinds.
    ///
    /// # The limits this slice accepts
    ///
    /// - **A class with a base *type variable*** — `class C extends mixin<T>()` —
    ///   is an intersection upstream (`getBaseTypeVariableOfClass`,
    ///   `checker.go:16936`) and prints as one. This answers `typeof C`, which is
    ///   right for every class whose base constructor is an ordinary value.
    ///   Telling the two apart needs `getBaseConstructorTypeOfClass`, which needs
    ///   `checkExpression` on the heritage clause and construct signatures.
    /// - **Statics are not reachable through `typeof C`.** A class's static
    ///   members and a namespace's exports live in the symbol's `exports` table,
    ///   and [`crate::types::TypeData::Named`] points `getPropertyOfType` at
    ///   `members`. Pointing it at `members` here would resolve `C.x` against
    ///   *instance* members, which is a wrong answer where a gap belongs, so this
    ///   carries no members table at all.
    /// - **The printed name is unqualified.** A class declared inside
    ///   `namespace M` prints `typeof C`, where upstream prints `typeof M.C` at a
    ///   reference site that cannot see `C` directly. Same divergence and same
    ///   cause as the one `docs/architecture/checker.md` records for named types:
    ///   the name is computed once at creation, and upstream computes it per
    ///   reference site.
    /// - **`strictNullChecks` and an optional symbol** (`checker.go:16942`) is
    ///   not ported: there are no compiler options here, so the port is
    ///   uniformly non-strict.
    fn get_type_of_func_class_enum_module_worker(&mut self, symbol: SymbolId) -> TypeId {
        let flags = self.binder.symbols().get(symbol).flags;
        // `isShorthandAmbientModuleSymbol` (`utilities.go:198`): `declare module
        // "x";` with no body has the type `any`. A computed answer, so `anyType`.
        if flags.intersects(SymbolFlags::MODULE) && self.is_shorthand_ambient_module(symbol) {
            return self.intrinsics.any;
        }
        if self.has_a_name_no_type_query_can_spell(symbol) {
            return self.intrinsics.error;
        }
        let name = self.binder.symbols().get(symbol).name;
        // `shouldEmitTypeOfSymbol` tests enum and value module *after* class but
        // as an `||`, so a merged `function f() {} namespace f {}` symbol takes
        // the `typeof` form. Ordering the class test first therefore changes
        // nothing; what matters is that the function case comes last.
        if flags.intersects(SymbolFlags::ENUM | SymbolFlags::VALUE_MODULE | SymbolFlags::CLASS) {
            let printed = format!("typeof {name}");
            return self.store.new_anonymous(TypeFlags::OBJECT, printed, symbol);
        }
        let Some(signatures) = self.get_signatures_of_symbol(symbol) else {
            return self.intrinsics.error;
        };
        // `createTypeNodeFromObjectType` emits a bare `FunctionTypeNode` only
        // when the resolved type has **no properties and no index signatures**
        // (`nodebuilderimpl.go:2698`), and takes the same test before rendering
        // the type-literal form. A function with expando properties —
        // `function f() {} f.a = "s";` — has them, and upstream prints
        // `{ (): void; a: string; }`. Printing only the signatures there is a
        // *wrong* answer rather than a partial one, which is worse: it looks
        // like a result. Rendering the members needs member ordering this port
        // does not have, so it is a gap, and `bd tsr-4sc.8` owns it.
        let symbol_data = self.binder.symbols().get(symbol);
        if !symbol_data.exports.is_empty() || !symbol_data.members.is_empty() {
            return self.intrinsics.error;
        }
        // `createTypeNodeFromObjectType` (`nodebuilderimpl.go:2690`) emits a bare
        // `FunctionTypeNode` only for a resolved type with exactly one call
        // signature and no construct signatures (`nodebuilderimpl.go:2706`).
        // Anything else with two or more falls through to the type-literal arm
        // (`nodebuilderimpl.go:2740`), whose members are the call signatures in
        // declaration order rendered as *members* — a colon before the return
        // type, not an arrow. `compiler/overloadConsecutiveness.types:15` is the
        // spelling this reproduces: `{ (): void; (): any; }`.
        //
        // The empty case is a gap rather than upstream's `{}`
        // (`nodebuilderimpl.go:2699`): `getSignaturesOfSymbol` returns an empty
        // vector both for a symbol that genuinely has no call signature and for
        // one whose every declaration was skipped, and those two must not print
        // the same thing.
        let printed = match signatures.as_slice() {
            [] => return self.intrinsics.error,
            [signature] => self.signature_to_string(signature),
            many => {
                let mut out = String::from("{ ");
                for signature in many {
                    out.push_str(&crate::objects::signature_member_text(self, signature));
                    out.push_str("; ");
                }
                out.push('}');
                out
            }
        };
        self.store.new_anonymous(TypeFlags::OBJECT, printed, symbol)
    }

    /// Whether `symbolToTypeNode` would spell this symbol as something other than
    /// its own name.
    ///
    /// Two cases, and both would otherwise print a plausible wrong line:
    ///
    /// - **An anonymous symbol** — a default export, an unnamed class expression.
    ///   Upstream's node builder generates a name for one.
    /// - **A module declared with a string name**, `declare module "x" { }`.
    ///   Upstream writes `typeof import("x")`, which the corpus records 46 times
    ///   for `module.exports` alone; the binder stores the name unquoted, so
    ///   taking it verbatim would print `typeof x` for a module that no name in
    ///   scope refers to.
    fn has_a_name_no_type_query_can_spell(&self, symbol: SymbolId) -> bool {
        if self.binder.symbols().get(symbol).name.is_empty() {
            return true;
        }
        self.binder
            .symbols()
            .get(symbol)
            .declarations
            .iter()
            .filter_map(|&declaration| self.node_map.get(declaration))
            .any(|node| {
                matches!(
                    node,
                    Node::ModuleDeclaration(module)
                        if matches!(module.name, Some(tsr_ast::ModuleName::StringLiteral(_)))
                )
            })
    }

    /// Ported from `isShorthandAmbientModule` (`utilities.go:202`): *"the only
    /// kind of module that can be missing a body is a shorthand ambient module"*.
    fn is_shorthand_ambient_module(&self, symbol: SymbolId) -> bool {
        let Some(declaration) = self.binder.symbols().get(symbol).value_declaration else {
            return false;
        };
        matches!(self.node_map.get(declaration), Some(Node::ModuleDeclaration(node)) if node.body.is_none())
    }

    /// Ported from `Checker.getTypeOfVariableOrParameterOrProperty`
    /// (`checker.go:16544`).
    ///
    /// The memo is ADR-0013's read-drop-recurse-write: the lookup's borrow ends
    /// before the recursion, because `TypeId` is `Copy` and nothing borrowed from
    /// `self` survives into it.
    fn get_type_of_variable_or_parameter_or_property(&mut self, symbol: SymbolId) -> TypeId {
        if let Some(&cached) = self.symbol_types.get(&symbol) {
            return cached;
        }
        let computed = self.get_type_of_variable_or_parameter_or_property_worker(symbol);
        self.symbol_types.insert(symbol, computed);
        computed
    }

    /// Ported from `Checker.getTypeOfVariableOrParameterOrPropertyWorker`
    /// (`checker.go:16578`).
    fn get_type_of_variable_or_parameter_or_property_worker(&mut self, symbol: SymbolId) -> TypeId {
        let Some(declaration) = self.binder.symbols().get(symbol).value_declaration else {
            return self.intrinsics.error;
        };

        // The circularity guard wraps the *whole* computation, so a type that
        // reaches itself through any depth of indirection is caught. Nothing
        // between here and `pop` may return early, or the stack unbalances.
        if !self.resolutions.push(symbol, PropertyName::Type) {
            return self.report_circularity_error(declaration);
        }

        let kind = self.nodes.kind(declaration);
        let result = match kind {
            SyntaxKind::VariableDeclaration
            | SyntaxKind::Parameter
            | SyntaxKind::PropertyDeclaration
            | SyntaxKind::PropertySignature => {
                self.get_widened_type_for_variable_like_declaration(declaration)
            }
            // Unported: property assignments, shorthand, methods, export
            // assignments, binary/call assignment declarations, JSX attributes
            // and enum members.
            _ => self.intrinsics.error,
        };

        if !self.resolutions.pop() {
            // A cycle closed *below* this frame, so the answer computed above was
            // built on a partial one and must not be kept.
            return self.report_circularity_error(declaration);
        }
        result
    }

    /// Ported from `Checker.reportCircularityError` (`checker.go:18822`),
    /// without the diagnostics — the checker has none yet (`bd tsr-5e7.6`).
    ///
    /// The **return type differs by cause**, which is easy to get wrong because
    /// the two print identically:
    ///
    /// - a self-referencing *type annotation* yields `errorType`;
    /// - a self-referencing *initialiser* yields `anyType`.
    ///
    /// `bd tsr-4sc.2`'s issue text said "errorType" flatly. It is not.
    fn report_circularity_error(&mut self, declaration: NodeId) -> TypeId {
        if self.type_annotation_of(declaration).is_some() {
            return self.intrinsics.error;
        }
        self.intrinsics.any
    }

    /// Ported from `Checker.getWidenedTypeForVariableLikeDeclaration`, which is
    /// `widenTypeForVariableLikeDeclaration(getTypeForVariableLikeDeclaration(..))`
    /// (`checker.go:16610`, `:18242`).
    fn get_widened_type_for_variable_like_declaration(&mut self, declaration: NodeId) -> TypeId {
        match self.get_type_for_variable_like_declaration(declaration) {
            Some(id) => id,
            // Upstream returns `anyType` for a declaration with neither an
            // annotation nor an initialiser (`checker.go:18264`) — a genuine
            // answer, the implicit any, not a gap. So `anyType` is right here
            // where `errorType` is right for an unported form.
            None => self.intrinsics.any,
        }
    }

    /// Ported from `Checker.getTypeForVariableLikeDeclaration`
    /// (`checker.go:16652`), restricted to the two paths this slice covers.
    ///
    /// `None` means "nothing could be inferred", which upstream signals with a
    /// nil `*Type` and turns into the implicit `any` one level up.
    fn get_type_for_variable_like_declaration(&mut self, declaration: NodeId) -> Option<TypeId> {
        // An annotation wins over an initialiser, always.
        if let Some(annotation) = self.type_annotation_of(declaration) {
            let declared = self.get_type_from_type_node(annotation);
            // `addOptionalityEx(declaredType, isProperty, isOptional)`
            // (`checker.go:16695`): `p?: string` declares `string | undefined`.
            // See `crate::optionality`.
            return Some(self.add_optionality_for_declaration(declared, declaration));
        }
        let initializer = self.initializer_of(declaration)?;
        let initializer_type = self.check_expression(initializer);
        Some(self.get_widened_literal_type_for_initializer(declaration, initializer_type))
    }

    /// Ported from `Checker.getWidenedLiteralTypeForInitializer`
    /// (`checker.go:16897`).
    ///
    /// This is the rule behind the most frequently surprising line in a `.types`
    /// baseline: `const x = "a"` is `"a"` and `let x = "a"` is `string`, from the
    /// same initialiser expression. A `const` keeps the literal; anything else
    /// widens it.
    fn get_widened_literal_type_for_initializer(
        &mut self,
        declaration: NodeId,
        id: TypeId,
    ) -> TypeId {
        if self.combined_node_flags(declaration).intersects(NodeFlags::CONSTANT) {
            return id;
        }
        self.get_widened_literal_type(id)
    }

    /// The type annotation of a declaration, if it has one.
    pub(crate) fn type_annotation_of(&self, declaration: NodeId) -> Option<TypeNode<'a>> {
        match self.node_map.get(declaration)? {
            Node::VariableDeclaration(node) => node.r#type,
            Node::ParameterDeclaration(node) => node.r#type,
            Node::PropertyDeclaration(node) => node.r#type,
            Node::PropertySignatureDeclaration(node) => node.r#type,
            _ => None,
        }
    }

    /// The initialiser of a declaration, if it has one.
    fn initializer_of(&self, declaration: NodeId) -> Option<Expression<'a>> {
        match self.node_map.get(declaration)? {
            Node::VariableDeclaration(node) => node.initializer,
            Node::ParameterDeclaration(node) => node.initializer,
            Node::PropertyDeclaration(node) => node.initializer,
            _ => None,
        }
    }
}

/// Whether a name can be printed after a dot.
///
/// Upstream asks `scanner.IsIdentifierText(name, LanguageVariantStandard)`
/// before emitting `E.A` rather than `(typeof E)["A"]`
/// (`nodebuilderimpl.go:3269`). This covers the ASCII identifier subset only, so
/// a name outside it is a gap rather than a guess. Deliberately a second copy of
/// the same predicate in [`crate::objects`]: sharing it means widening one
/// module's private helper into the crate surface for six lines, and the two
/// have different reasons to change — that one guards a property name in an
/// object literal, this one an enum member name.
fn is_identifier_text(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else { return false };
    (first.is_ascii_alphabetic() || first == '_' || first == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}
