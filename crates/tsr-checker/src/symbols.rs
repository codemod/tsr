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

use tsr_ast::{Expression, ModuleReference, Node, NodeFlags, NodeId, SyntaxKind, TypeNode};
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
        // The branch's *position* mattered before `getTypeOfAccessors` landed
        // and still does. Omitting it let an accessor
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
        // upstream's. Now that `getTypeOfAccessors` answers rather than gapping,
        // the first boundary is observable too: an accessor merged with a method
        // answers the accessor's type where the function worker would have
        // printed a signature.
        if flags.intersects(SymbolFlags::ACCESSOR) {
            return self.get_type_of_accessors(symbol);
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
        // `checker.go:16518`.
        if flags.intersects(SymbolFlags::ALIAS) {
            return self.get_type_of_alias(symbol);
        }
        // Every `SymbolFlags` shape upstream dispatches on is now answered. What
        // remains unported is the four `CheckFlags` shapes upstream tests
        // *before* any of them — deferred, instantiated, mapped, reverse-mapped.
        self.intrinsics.error
    }

    /// The type of a `get`/`set` accessor symbol.
    ///
    /// Ported from `Checker.getTypeOfAccessors` (`checker.go:18511`), which
    /// tries four sources **in order** and falls back to `anyType`:
    ///
    /// 1. the getter's return annotation (`checker.go:18522`),
    /// 2. else the setter's parameter annotation (`checker.go:18524`),
    /// 3. else an auto-accessor property's annotation (`checker.go:18527`),
    /// 4. else the getter's inferred body return type (`checker.go:18531`),
    /// 5. else `anyType`, with an implicit-any diagnostic (`checker.go:18545`).
    ///
    /// **1, 2, 4 and 5 are ported; only 3 gaps.** The order is the whole content of
    /// the function and is not negotiable: `compiler/accessorBodyInTypeContext.types`
    /// records `set foo(v: any) { }` as `>foo : any`, so a setter annotation is a
    /// real answer and not a fallback.
    ///
    /// # Why 5 answers `any` and not `errorType`
    ///
    /// Everywhere else in this module an unported form answers `errorType`,
    /// because `errorType` is how a gap stays separable from a computed answer.
    /// Here `anyType` **is** upstream's computed answer — an accessor with no
    /// annotation anywhere and no getter body is implicitly `any`, and upstream
    /// reports it rather than failing. Answering `errorType` would turn a line
    /// this port gets right into a line it reports as missing.
    ///
    /// The distinction is only safe because case 4 is separated out first: a
    /// getter *with a body* is inferred, and when this port's inference cannot
    /// answer it yields `errorType` rather than falling into the `any` arm and
    /// claiming a result it did not compute. That separation is the one piece of
    /// this function that is not a transliteration, and it is what stops `any`
    /// from becoming a lie. It mattered more when case 4 gapped wholesale; it
    /// still matters, because `get_return_type_from_body` answers `None` for
    /// every aggregate, `async` and generator body.
    fn get_type_of_accessors(&mut self, symbol: SymbolId) -> TypeId {
        if let Some(&cached) = self.symbol_types.get(&symbol) {
            return cached;
        }
        // `checker.go:18514`: `get x(): typeof this.x` reaches its own symbol.
        if !self.resolutions.push(symbol, PropertyName::Type) {
            return self.intrinsics.error;
        }
        let computed = self.get_type_of_accessors_worker(symbol);
        let computed = if self.resolutions.pop() { computed } else { self.intrinsics.error };
        self.symbol_types.insert(symbol, computed);
        computed
    }

    fn get_type_of_accessors_worker(&mut self, symbol: SymbolId) -> TypeId {
        let declarations =
            self.binder.symbols().get(symbol).declarations.iter().copied().collect::<Vec<_>>();
        let mut getter = None;
        let mut setter = None;
        let mut other = false;
        for declaration in declarations {
            match self.node_map.get(declaration) {
                Some(Node::GetAccessorDeclaration(_)) => getter = Some(declaration),
                Some(Node::SetAccessorDeclaration(_)) => setter = Some(declaration),
                // An auto-accessor property (`accessor x = 1`) is upstream's
                // case 3/5 and needs `getWidenedTypeForVariableLikeDeclaration`
                // over a declaration this arm does not otherwise handle. Noted
                // rather than guessed at.
                _ => other = true,
            }
        }
        // `checker.go:18522` then `:18524` — the getter's annotation wins over
        // the setter's, and only the *order* makes the two distinguishable when
        // both are annotated with different types.
        if let Some(annotation) = getter.and_then(|node| self.accessor_annotation(node)) {
            return self.get_type_from_type_node(annotation);
        }
        if let Some(annotation) = setter.and_then(|node| self.accessor_annotation(node)) {
            return self.get_type_from_type_node(annotation);
        }
        if other {
            return self.intrinsics.error;
        }
        // `checker.go:18529`: an unannotated getter *with a body* is inferred
        // from that body. `compiler/accessorBodyInTypeContext.types` records
        // `get foo() { return 0 }` as `>foo : number`, not `any`.
        //
        // **`None` becomes `errorType`, never `anyType`, and this is the one
        // line in the function where that matters.** Upstream's
        // `getReturnTypeFromBody` always produces a type, so upstream never
        // falls from here to case 5. This port's inference has gaps, and every
        // one of them is a declaration upstream WOULD have inferred — so
        // letting `None` fall through to the `any` below would print a
        // plausible wrong `any` on exactly the accessors that have a real
        // answer. The `unwrap_or` is what keeps case 5's computed `any` honest.
        if let Some(getter) = getter
            && matches!(self.node_map.get(getter), Some(Node::GetAccessorDeclaration(node)) if node.body.is_some())
        {
            let inferred = self.get_return_type_from_body(getter);
            return inferred.unwrap_or(self.intrinsics.error);
        }
        // `checker.go:18545`. Upstream's answer, not this port's shrug.
        self.intrinsics.any
    }

    /// The type node an accessor declares, by `getAnnotatedAccessorTypeNode`
    /// (`checker.go:20106`).
    ///
    /// A getter annotates its **return type**; a setter annotates its **first
    /// parameter** — `getEffectiveSetAccessorTypeAnnotationNode`
    /// (`checker.go:20118`). Reading `node.r#type` for both would silently
    /// answer `None` for every setter, since a setter's own `r#type` slot is
    /// only ever filled by a grammar error.
    fn accessor_annotation(&self, declaration: NodeId) -> Option<TypeNode<'a>> {
        match self.node_map.get(declaration)? {
            Node::GetAccessorDeclaration(node) => node.r#type,
            Node::SetAccessorDeclaration(node) => node.parameters.first()?.r#type,
            _ => None,
        }
    }

    /// The type of an alias symbol — `import q = M.a`.
    ///
    /// Ported from `Checker.getTypeOfAlias` (`checker.go:18598`): resolve the
    /// alias to its target, and answer the target's type **only if the target is
    /// a value**. Upstream's comment on that test is worth keeping, because it
    /// is not merely a correctness check — without it, `getTypeOfSymbol` on a
    /// type-only target recurses back into this function and overflows the
    /// stack. A type-only target is `errorType`, and the way to its type is
    /// `getDeclaredTypeOfSymbol`.
    ///
    /// # Only the same-file slice, and the rest is not a checker problem
    ///
    /// `resolve_alias` below answers `None` for an `import q = require("m")` or
    /// any ES `import ... from`, so those keep answering `errorType`. That is
    /// not a gap this module can close: cross-file targets need globals merged
    /// across files, which this port does not do
    /// (`crates/tsr-compiler/src/lib.rs:24`, ADR-0034, `bd tsr-9or.1`).
    /// Measured over the corpus at `9e459cf`: 1,218 `import X =` declarations,
    /// of which 764 are `require(...)` and 442 name an entity; 378 of those 442
    /// have their root declared in the same unit, and 131 of THOSE are a bare
    /// identifier rather than a qualified name (the qualified form is gapped for
    /// a separate reason — see [`Checker::resolve_alias`]). So this arm
    /// addresses **at most 558 of the ALIAS bucket's 4,298 lines** (131
    /// declaration names plus 427 identifier occurrences, the latter an
    /// over-count). The remaining ~3,700 sit behind `bd tsr-9or.1` and behind
    /// symbol accessibility, not behind more work in this module.
    fn get_type_of_alias(&mut self, symbol: SymbolId) -> TypeId {
        if let Some(&cached) = self.symbol_types.get(&symbol) {
            return cached;
        }
        // `checker.go:18601`. `import a = a` reaches its own symbol, and the
        // frame is what upstream uses to answer `errorType` rather than recur.
        if !self.resolutions.push(symbol, PropertyName::Type) {
            return self.intrinsics.error;
        }
        let target = self.resolve_alias(symbol);
        let computed = match target {
            // `checker.go:18612`, and the `SymbolFlags::VALUE` test is the
            // stack-overflow guard, not a nicety.
            Some(target)
                if self.binder.symbols().get(target).flags.intersects(SymbolFlags::VALUE) =>
            {
                self.get_type_of_symbol(target)
            }
            _ => self.intrinsics.error,
        };
        let computed = if self.resolutions.pop() { computed } else { self.intrinsics.error };
        self.symbol_types.insert(symbol, computed);
        computed
    }

    /// The symbol an alias names, for the forms that resolve inside one file.
    ///
    /// Ported from `Checker.resolveAlias` (`checker.go:16266`) reduced to its
    /// `getTargetOfImportEqualsDeclaration` (`checker.go:14439`) case, and from
    /// `getSymbolOfPartOfRightHandSideOfImportEquals` (`checker.go:14474`) for
    /// the meaning to resolve in. That function's three-case comment is the
    /// whole specification, and the two meanings are **not** interchangeable:
    ///
    /// ```text
    /// import a = |b|;    // Namespace
    /// import a = |b.c|;  // Value, type, namespace
    /// ```
    ///
    /// A bare identifier resolves in `NAMESPACE` only, so `const x = 1; import
    /// a = x;` finds nothing and answers `errorType` — which is upstream's
    /// answer, not a gap. Resolving it in `VALUE` too would "fix" that line into
    /// a wrong one.
    ///
    /// # Only the bare identifier is ported, and the baselines say why
    ///
    /// Resolving `import booz = foo.bar.baz` is easy — walk `exports` — and the
    /// answer would still be wrong, because the *printed* form does not name the
    /// target. `compiler/aliasBug.types` records
    ///
    /// ```text
    /// import provide = foo;
    /// >provide : typeof foo
    ///
    /// import booz = foo.bar.baz;
    /// >booz : typeof booz
    /// ```
    ///
    /// The bare form prints the **target's** name and the qualified form prints
    /// the **alias's own**. That is not two rules: upstream's node builder emits
    /// the shortest accessible chain to the symbol, and an alias declaration
    /// always creates a one-link chain. For `foo` the direct name is already one
    /// link and wins; for `foo.bar.baz` it is three, so `booz` wins. This port
    /// has no symbol-accessibility machinery, so it would print `typeof baz` —
    /// a wrong line. The qualified form therefore answers `errorType`.
    ///
    /// **`ExternalModuleReference` answers `None` deliberately** — see
    /// [`Checker::get_type_of_alias`] for why that is `bd tsr-9or.1` and not
    /// this module's to close.
    fn resolve_alias(&mut self, symbol: SymbolId) -> Option<SymbolId> {
        let declaration = *self.binder.symbols().get(symbol).declarations.first()?;
        let Node::ImportEqualsDeclaration(node) = self.node_map.get(declaration)? else {
            // Every other alias form — ES import clauses, export specifiers,
            // `export =` — reaches its target through module resolution.
            return None;
        };
        match node.module_reference? {
            ModuleReference::Identifier(name) => {
                let found = self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    name.node_id?,
                    name.text,
                    SymbolFlags::NAMESPACE,
                )?;
                // **The meaning has to be re-checked here**, because
                // `Binder::resolve_name`'s `locals` lookup is deliberately not
                // meaning-filtered (`crates/tsr-binder/src/lib.rs:300`) while
                // upstream's is. Without this, `const x = 1; import q = x;`
                // resolves `x` and answers `1`, and `1` is a *wrong line* rather
                // than a missing one — upstream rejects the alias, and no
                // baseline in the corpus records the form at all.
                //
                // A filter in the checker rather than a fix in the binder,
                // because the binder's divergence is load-bearing for its other
                // callers and is not mine to change; this restores upstream's
                // meaning at this one call site.
                self.binder
                    .symbols()
                    .get(found)
                    .flags
                    .intersects(SymbolFlags::NAMESPACE)
                    .then_some(found)
            }
            // Resolvable, but not printable — see above.
            // Two gaps that share an answer but not a reason, and the reasons
            // are worth keeping apart even though the arms are merged here to
            // satisfy `clippy::match_same_arms`. A qualified name RESOLVES
            // fine and prints wrong, for want of symbol accessibility (see
            // above). An external module reference does not resolve at all, for
            // want of cross-file globals (`bd tsr-9or.1`, `checker.go:14441`).
            // Closing one does nothing for the other.
            ModuleReference::QualifiedName(_) | ModuleReference::ExternalModuleReference(_) => None,
        }
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
