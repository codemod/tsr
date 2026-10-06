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
/// `ast.InternalSymbolNameExportStar`. Spelled here rather than imported for
/// the same reason the `export=` name is: the binder's constant is `pub(crate)`
/// to that crate.
const INTERNAL_EXPORT_STAR: &str = "__export";
/// `ast.InternalSymbolNameModuleExports` (`ast/symbol.go:69`): the export
/// name `export { Foo as "module.exports" }` publishes, which a `CommonJS`
/// `require` of an ES module observes as its value.
const INTERNAL_MODULE_EXPORTS: &str = "module.exports";

use tsr_binder::{SymbolFlags, SymbolId};

use crate::{checker::Checker, flags::TypeFlags, resolution::PropertyName, types::TypeId};
use tsr_diagnostics::{Diagnostic, messages};

use crate::check::spelling_suggestion;

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
        #[cfg(feature = "work-trace")]
        let _work = self.trace_symbol_work(crate::work_trace::Operation::SymbolTypeQuery, symbol);
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
        // An **export marker**: the local a module leaves behind for an exported
        // declaration. See [`Checker::get_type_of_export_value`].
        if flags.contains(SymbolFlags::EXPORT_VALUE) {
            return self.get_type_of_export_value(symbol);
        }
        // Every `SymbolFlags` shape upstream dispatches on is now answered. What
        // remains unported is the four `CheckFlags` shapes upstream tests
        // *before* any of them — deferred, instantiated, mapped, reverse-mapped.
        self.intrinsics.error
    }

    /// The type of an **export marker** — the local left behind by
    /// `export var x`, `export function f`, `export class C`.
    ///
    /// # Why a symbol with no useful flags exists at all
    ///
    /// When a declaration is exported from a module, the binder declares it
    /// **twice**: the real symbol goes into the module's `exports` with its own
    /// flags, and a *marker* goes into the file's `locals` carrying
    /// `SymbolFlags::EXPORT_VALUE` and nothing else
    /// (`binder.rs:3086`-`3098`). That is upstream's design, not a local quirk —
    /// it is what keeps an unqualified reference to an exported name resolvable
    /// while the export table stays the authority on what was exported.
    ///
    /// A reference inside the module resolves to the **marker**, whose flags
    /// carry no `VARIABLE`, `FUNCTION` or `CLASS` bit — so every arm of
    /// [`Checker::get_type_of_symbol`] above misses and the answer was
    /// `errorType`. Measured at **3,144 lines across 463 cases**: not one file
    /// and not one shape, just every reference to every exported name.
    ///
    /// # Upstream follows a link this port does not have
    ///
    /// `getExportSymbolOfValueSymbolIfExported` (`checker.go:14383`) reads
    /// `symbol.ExportSymbol` and swaps the marker for the real export symbol.
    /// [`tsr_binder::Symbol`] has no such field.
    ///
    /// So this types the marker **from the declaration it shares with its export
    /// symbol**, which is the same answer by construction: both symbols were
    /// declared from the same node, so dispatching on that node's kind reproduces
    /// what the export symbol's own flags would have selected. Adding the link to
    /// the binder would be the faithful port and is the better fix when someone
    /// owns that file; this reaches the same answer without reshaping a type
    /// every other consumer of the binder shares.
    ///
    /// **How this would be shown wrong:** an exported declaration whose export
    /// symbol's flags disagree with what its declaration kind implies. Merged
    /// declarations are where to look — `export interface I {}` beside
    /// `export const I = 1` — and a merged marker is why the fallthrough answers
    /// `errorType` rather than guessing.
    fn get_type_of_export_value(&mut self, symbol: SymbolId) -> TypeId {
        if let Some(&cached) = self.symbol_types.get(&symbol) {
            return cached;
        }
        let computed = match self.export_symbol_of(symbol) {
            // The export symbol carries the real flags, so this re-enters the
            // ordinary dispatch and every exported form — variable, function,
            // class, enum, module — is answered by the arm that already knows
            // how, rather than by a second copy of that knowledge here.
            Some(export) if export != symbol => self.get_type_of_symbol(export),
            _ => self.intrinsics.error,
        };
        self.symbol_types.insert(symbol, computed);
        computed
    }

    /// The export symbol an export marker shadows.
    ///
    /// `getExportSymbolOfValueSymbolIfExported` (`checker.go:14383`) reads
    /// `symbol.ExportSymbol`, and [`tsr_binder::Symbol`] now carries the same
    /// link. This is that read.
    ///
    /// # The reconstruction this replaces, and why it was wrong
    ///
    /// Until `033277f` the binder had no such field, so this walked to the
    /// marker's enclosing `SourceFile` and read *that file's* module symbol's
    /// `exports`. The reasoning was that the file's own symbol is the module
    /// symbol, so the lookup is scoped to the module that declared the marker.
    ///
    /// That is true, and it is the wrong module. A marker's container is
    /// whatever `declare_module_member` declared it into, and for
    /// `namespace N { export enum E {} }` it is **`N`**. Worse, a *script* file
    /// has no module symbol at all, so `self.binder.symbol_of(file)` returned
    /// `None` and the walk stopped at its first step.
    ///
    /// Measured at `058b4a9`: **2,175 assertion lines** are markers declared
    /// inside a namespace, **988** of them in a script file. See
    /// [`docs/architecture/checker-notes-nameres.md`](../../../docs/architecture/checker-notes-nameres.md)
    /// §5, which also records that the reconstruction was rejected in favour of
    /// the field rather than repaired: deciding *which* ancestor owns the
    /// exports table is exactly the decision the binder already made, and a
    /// second copy of it is a second thing to keep in step. This is the first
    /// copy having been wrong.
    fn export_symbol_of(&self, marker: SymbolId) -> Option<SymbolId> {
        self.binder.symbols().get(marker).export_symbol
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
        // Native answers `anyType` here; this port keeps `errorType` because
        // its eager object-literal members close cycles native never forms
        // (`noCircularitySelfReferentialGetter3/4`), where `any` prints wrong.
        let computed = if self.resolutions.pop() {
            computed
        } else {
            self.report_accessor_circularity(symbol);
            self.intrinsics.error
        };
        self.symbol_types.insert(symbol, computed);
        computed
    }

    /// `getTypeOfAccessors`' failed-pop arm (`checker.go:18511`): TS2502 at the
    /// first annotated accessor (getter, then setter).
    ///
    /// Native's last arm, TS7023 at an unannotated getter under
    /// `noImplicitAny`, is not ported: this port resolves object-literal and
    /// signature members eagerly, so its unannotated cycles include ones native
    /// never forms (`noCircularitySelfReferentialGetter4`).
    fn report_accessor_circularity(&mut self, symbol: SymbolId) {
        use tsr_diagnostics::{Diagnostic, messages};
        let declarations = self.binder.symbols().get(symbol).declarations.clone();
        let of_kind = |kind| {
            declarations.iter().copied().find(|&declaration| self.nodes.kind(declaration) == kind)
        };
        let getter = of_kind(SyntaxKind::GetAccessor);
        let setter = of_kind(SyntaxKind::SetAccessor);
        let Some(at) = getter
            .filter(|&node| self.accessor_annotation(node).is_some())
            .or_else(|| setter.filter(|&node| self.accessor_annotation(node).is_some()))
        else {
            return;
        };
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        if self.circularity_reported.insert(at) {
            let name = self.binder.symbols().get(symbol).name.to_string();
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::_0_IS_REFERENCED_DIRECTLY_OR_INDIRECTLY_IN_ITS_OWN_TYPE_ANNOTATION,
                    self.error_span(at),
                    [name],
                ),
            );
        }
    }

    /// `getWriteTypeOfAccessors` (`checker.go:16447`) — the type a WRITE to a
    /// property sees, which is the **setter's** parameter annotation.
    ///
    /// [`Checker::get_type_of_accessors_worker`] prefers the GETTER's
    /// annotation and falls back to the setter's; a write reverses that
    /// preference. The two differ only for a DIVERGENT accessor pair
    /// (`get x(): string` beside `set x(v: string | number | boolean)`), which
    /// is exactly the population §615 measured — `divergentAccessorsTypes1`,
    /// `2`, `7`, `8` are 22 of its 42 lines.
    ///
    /// Answers `None` when there is no setter annotation, so every other
    /// property keeps the read type and this arm cannot widen anything it does
    /// not own.
    pub(crate) fn write_type_of_accessors(&mut self, symbol: SymbolId) -> Option<TypeId> {
        let declarations =
            self.binder.symbols().get(symbol).declarations.iter().copied().collect::<Vec<_>>();
        let setter = declarations.into_iter().find(|&declaration| {
            matches!(self.node_map.get(declaration), Some(Node::SetAccessorDeclaration(_)))
        })?;
        let annotation = self.accessor_annotation(setter)?;
        let written = self.get_type_from_type_node(annotation);
        if written == self.intrinsics.error {
            return None;
        }
        // A setter annotated with a TYPE PARAMETER must not take this road: the
        // read path instantiates the member for the receiver's arguments and
        // this returns the uninstantiated parameter, so `set y(v: U)` in
        // `class C<T, U>` came out as the bare `U` where the instantiated read
        // was already right (`instancePropertyInClassType`,
        // `privateNamesAndGenericClasses-2` — two PASSING cases, measured).
        // The divergent-accessor population this arm is for is annotated with
        // concrete types.
        if self.store.get(written).flags.intersects(crate::flags::TypeFlags::TYPE_PARAMETER) {
            return None;
        }
        Some(written)
    }

    /// Native accessor symbol serialization retains both read and write types.
    pub(crate) fn accessor_write_parameter(
        &mut self,
        symbol: SymbolId,
    ) -> Option<crate::signatures::Parameter> {
        let symbol = self.binder.symbols().get(symbol);
        if !symbol.flags.contains(SymbolFlags::GET_ACCESSOR | SymbolFlags::SET_ACCESSOR) {
            return None;
        }
        let setter =
            symbol.declarations.iter().copied().find(|&id| {
                matches!(self.node_map.get(id), Some(Node::SetAccessorDeclaration(_)))
            })?;
        self.get_signature_from_declaration(setter)?.parameters.into_iter().next()
    }

    fn get_type_of_accessors_worker(&mut self, symbol: SymbolId) -> TypeId {
        let mut declarations =
            self.binder.symbols().get(symbol).declarations.iter().copied().collect::<Vec<_>>();
        // §523: a LATE-BOUND accessor pair splits across two `__computed`
        // symbols (the binder gives each computed name its own — the §383
        // method precedent), so the setter's symbol alone never sees the
        // getter upstream's merged symbol reads
        // (`set [Symbol.toPrimitive](x)` wants the getter's `string`,
        // `symbolDeclarationEmit4/10/11`). Reconstruct the pair through the
        // same sibling walk §383 uses: same spelled name, accessor kinds.
        if self.binder.symbols().get(symbol).name == "__computed"
            && let Some(parent) = self.binder.symbols().get(symbol).parent
            && let Some(own_declaration) = declarations.first().copied()
        {
            let is_static = self.property_has_modifier(symbol, SyntaxKind::StaticKeyword);
            let members = self.late_bound_members_of(parent, is_static);
            let own_name =
                members.iter().find(|(_, id)| *id == own_declaration).map(|(name, _)| name.clone());
            if let Some(own_name) = own_name {
                for (name, member) in members {
                    if name == own_name
                        && !declarations.contains(&member)
                        && matches!(
                            self.node_map.get(member),
                            Some(Node::GetAccessorDeclaration(_) | Node::SetAccessorDeclaration(_))
                        )
                    {
                        declarations.push(member);
                    }
                }
            }
        }
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
        // §31 (`checker-notes-callres.md`): an `import x = require("...")`
        // upstream never resolves — mispositioned (TS1147 territory) or
        // genuinely unfindable (the TS2307 predicate) — reads `any` at every
        // use site. A resolvable module this port cannot type keeps the
        // `errorType` gap below instead.
        if let Some(declaration) = self.declaration_of_alias_symbol(symbol)
            && let Some(specifier) = match self.node_map.get(declaration) {
                Some(Node::ImportEqualsDeclaration(node)) => match node.module_reference {
                    Some(ModuleReference::ExternalModuleReference(reference)) => {
                        reference.expression.and_then(|expression| expression.node_id())
                    }
                    _ => None,
                },
                Some(Node::VariableDeclaration(variable)) => match variable.initializer {
                    Some(Expression::CallExpression(call)) => {
                        call.arguments.first().and_then(tsr_ast::Expression::node_id)
                    }
                    _ => None,
                },
                _ => None,
            }
        {
            // Position is IRRELEVANT to the type: upstream still resolves
            // the require() against ambient modules inside a namespace
            // (TS1147 is a grammar error, not a resolution bar) —
            // `privacyGloImportParseErrors` wants `typeof errorImport` for a
            // namespace-positioned import of a QUOTED ambient module, the
            // §31 first pair's 4 adverse lines. Findability alone decides.
            let unresolvable = self.module_specifier_unfindable(specifier);
            if unresolvable {
                let any = self.intrinsics.any;
                let any = if self.resolutions.pop() { any } else { self.intrinsics.error };
                self.symbol_types.insert(symbol, any);
                return any;
            }
        }
        // §119 (`checker-notes-narrow.md`): the ES-import declaration forms —
        // ImportSpecifier, named ImportClause (a default import), and
        // NamespaceImport — take the same rule through the same calibrated
        // predicate: an UNFINDABLE specifier reads `any` at every use site
        // (upstream's unresolved-import error-answer, observable as `any`).
        // `export ... from` re-exports (ExportSpecifier, NamespaceExport) are
        // deliberately NOT admitted — different declaration kinds, different
        // upstream rule (§119 falsifier a). Findable-but-untyped modules keep
        // the errorType gap below. §113 measured this arm 2:1 when the symlink
        // corpora were unfindable; §118 mounted their links, which excludes
        // that adverse class from the arm's domain structurally.
        if let Some(declaration) = self.declaration_of_alias_symbol(symbol)
            && matches!(
                self.nodes.kind(declaration),
                SyntaxKind::ImportSpecifier
                    | SyntaxKind::ImportClause
                    | SyntaxKind::NamespaceImport
            )
            && let Some(specifier) = self.import_declaration_specifier(declaration)
            && self.module_specifier_unfindable(specifier)
        {
            let any = self.intrinsics.any;
            let any = if self.resolutions.pop() { any } else { self.intrinsics.error };
            self.symbol_types.insert(symbol, any);
            return any;
        }
        // §130 (`checker-notes-narrow.md`): an ImportSpecifier alias whose
        // module RESOLVES and whose named export's absence is ESTABLISHED
        // answers TS2305's deliberate error-any (es6ExportEqualsInterop's
        // wants). The gates are `report_missing_module_export`'s: exports
        // table non-empty (§186), plus — conservatively — NO `export *`
        // declarations at all (a star chain through an unresolvable target
        // would make absence a guess).
        if let Some(declaration) = self.declaration_of_alias_symbol(symbol)
            && self.nodes.kind(declaration) == SyntaxKind::ImportSpecifier
            && self.missing_import_export_established(declaration)
        {
            let any = self.intrinsics.any;
            let any = if self.resolutions.pop() { any } else { self.intrinsics.error };
            self.symbol_types.insert(symbol, any);
            return any;
        }
        // §131 (`checker-notes-narrow.md`): a DEFAULT import whose module
        // resolves, exports things, carries no `default`, and CANNOT have a
        // synthetic one (TS files never do unless they `export =` —
        // `canHaveSyntheticDefault`'s tail, already ported for the TS1192
        // diagnostic) reads upstream's TS1192 error-any. A module that CAN
        // have one keeps the gap — upstream resolves synthetically through
        // machinery this port lacks.
        if let Some(declaration) = self.declaration_of_alias_symbol(symbol)
            && self.nodes.kind(declaration) == SyntaxKind::ImportClause
            && self.missing_default_established(declaration)
        {
            let any = self.intrinsics.any;
            let any = if self.resolutions.pop() { any } else { self.intrinsics.error };
            self.symbol_types.insert(symbol, any);
            return any;
        }
        // §300: EVERY import binding from a SHORTHAND ambient module —
        // `declare module "abcdefgh";`, no body — is `any` upstream
        // (`isShorthandAmbientModuleSymbol`, `utilities.go:198`: the module
        // resolves to its own symbol and every member read is `any`).
        // `declarationEmitAnyComputedPropertyInClass` records
        // `import Test from "abcdefgh"` with `Test.someKey : any` throughout.
        if let Some(declaration) = self.declaration_of_alias_symbol(symbol)
            && matches!(
                self.nodes.kind(declaration),
                SyntaxKind::ImportClause
                    | SyntaxKind::ImportSpecifier
                    | SyntaxKind::NamespaceImport
            )
            && let Some(specifier) = self.import_declaration_specifier(declaration)
            && let Some(module) = self.resolve_external_module_name(declaration, specifier)
            && self.binder.symbols().get(module).declarations.iter().any(|&d| {
                matches!(self.node_map.get(d),
                    Some(Node::ModuleDeclaration(m)) if m.body.is_none())
            })
        {
            let any = self.intrinsics.any;
            let any = if self.resolutions.pop() { any } else { self.intrinsics.error };
            self.symbol_types.insert(symbol, any);
            return any;
        }
        // §144 (`checker-notes-narrow.md`): an ImportEquals ENTITY form
        // whose ROOT name resolves to NOTHING reads upstream's TS2503-family
        // error-any at every use — the §31/§119 boundary argument, entity
        // flavor. A root that RESOLVES with a failing chain keeps the gap
        // (that half is this port's qualified walk).
        if let Some(declaration) = self.declaration_of_alias_symbol(symbol)
            && let Some(Node::ImportEqualsDeclaration(node)) = self.node_map.get(declaration)
            && let Some(reference) = node.module_reference
            && !matches!(reference, ModuleReference::ExternalModuleReference(_))
        {
            let root = {
                let mut entity = match reference {
                    ModuleReference::Identifier(name) => Some(name),
                    ModuleReference::QualifiedName(mut qualified) => loop {
                        match qualified.left {
                            Some(tsr_ast::EntityName::Identifier(name)) => break Some(name),
                            Some(tsr_ast::EntityName::QualifiedName(inner)) => qualified = inner,
                            None => break None,
                        }
                    },
                    ModuleReference::ExternalModuleReference(_) => None,
                };
                entity.take()
            };
            if let Some(root) = root
                && let Some(id) = root.node_id
                && self
                    .binder
                    .resolve_name(self.nodes, self.node_map, id, root.text, SymbolFlags::NAMESPACE)
                    .is_none()
            {
                let any = self.intrinsics.any;
                let any = if self.resolutions.pop() { any } else { self.intrinsics.error };
                self.symbol_types.insert(symbol, any);
                return any;
            }
        }
        // getTypeOfAlias (checker.go:18612) uses the target's value type.
        // Qualified import-equals names are selected at the serialization
        // site by best_name, not baked into a second type for the alias.
        let target = self.resolve_alias(symbol);
        // `checker.go:18612`, and the `SymbolFlags::VALUE` test is the
        // stack-overflow guard, not a nicety. It is taken over
        // [`Checker::get_symbol_flags`] rather than over the raw flags because
        // upstream's is: `c.getSymbolFlags(targetSymbol)&SymbolFlagsValue`, and
        // an alias's own flags carry no `VALUE` bit, so `export { a }` naming
        // `import a = N` would fail a raw test on a symbol that plainly has a
        // type.
        let computed = match target {
            Some(target) if self.get_symbol_flags(target).intersects(SymbolFlags::VALUE) => {
                let value = self.get_type_of_symbol(target);
                self.module_clone_type(symbol, target, value).unwrap_or(value)
            }
            _ => self.intrinsics.error,
        };
        let computed = if self.resolutions.pop() { computed } else { self.intrinsics.error };
        self.symbol_types.insert(symbol, computed);
        computed
    }

    /// resolveESModuleSymbol/cloneTypeAsModuleType: namespace imports of a
    /// class/function copy its value members, not its signatures. Variable
    /// exports whose value happens to be callable are a different symbol shape.
    fn module_clone_type(
        &mut self,
        alias: SymbolId,
        target: SymbolId,
        value: TypeId,
    ) -> Option<TypeId> {
        let declaration = self.declaration_of_alias_symbol(alias)?;
        if self.nodes.kind(declaration) != SyntaxKind::NamespaceImport {
            return None;
        }
        let target = self.resolve_alias_fully(target);
        let target_flags = self.binder.symbols().get(target).flags;
        let kind = if target_flags.contains(SymbolFlags::CLASS) {
            crate::signatures::SignatureKind::Construct
        } else if target_flags.contains(SymbolFlags::FUNCTION) {
            crate::signatures::SignatureKind::Call
        } else {
            return None;
        };
        // resolveESModuleSymbol's `module.exports` arm answers that export
        // itself, not a module copy (`namespace_import_module_exports`).
        if let Some(owner) =
            self.nodes.parent(declaration).and_then(|clause| self.nodes.parent(clause))
            && let Some(specifier) = self.external_module_name(owner)
            && self.namespace_import_module_exports(owner, specifier).is_some()
        {
            return None;
        }
        if self.is_error(value)
            || self
                .signatures_of_type_kind(value, kind)
                .is_none_or(|signatures| signatures.is_empty())
        {
            return None;
        }
        let crate::types::TypeData::Anonymous { symbol, ref text, .. } = self.store.get(value).data
        else {
            return None;
        };
        let mut text = text.clone();
        let properties = if target_flags.intersects(SymbolFlags::CLASS | SymbolFlags::VALUE_MODULE)
        {
            None
        } else {
            let mut properties = self.callable_export_properties(target)?;
            if let Some(default) = self.module_clone_default_symbol(alias) {
                properties.push(crate::objects::AnonymousProperty {
                    accessor_write: None,
                    method: false,
                    origin: Some(default),
                    checked_declaration: None,
                    name: "default".to_owned(),
                    printed_name: "default".to_owned(),
                    printed_type: self.type_to_string(value),
                    optional: false,
                    readonly: false,
                    r#type: value,
                });
            }
            text = crate::objects::render_object_type(&crate::callable_expandos::property_members(
                &properties,
            ));
            Some(properties)
        };
        let flags = self.store.get(value).flags;
        let clone = self.store.new_anonymous(flags, text, symbol, false);
        self.module_value_clones.insert(clone, (alias, value));
        self.signature_types.insert(clone, Vec::new());
        if let Some(properties) = properties {
            self.anonymous_properties.insert(clone, (properties, false));
        }
        Some(clone)
    }

    /// The synthetic default aliases the export-equals value. Reuse that
    /// alias's value/readonly metadata rather than a constructor property.
    pub(crate) fn module_clone_default_symbol(&mut self, alias: SymbolId) -> Option<SymbolId> {
        let declaration = self.declaration_of_alias_symbol(alias)?;
        let specifier = self.import_declaration_specifier(declaration)?;
        let module = self.resolve_external_module_name(declaration, specifier)?;
        self.can_have_synthetic_default(module)
            .then(|| self.binder.symbols().get(module).exports.get("export=").copied())
            .flatten()
    }

    /// Pinned 5b1047d getSymbol (checker.go:2176) supplies target meaning to
    /// the binder's original ancestor walk for external import-equals exports.
    /// The immutable Program owns symbols and module identities; the existing
    /// alias worker owns resolution. A direct completed nonalias target may
    /// admit this export or continue to an outer meaning. Missing/alias targets
    /// decline rather than claiming absence. The same answer filters an alias
    /// found in `locals` (`getSymbol`'s alias arm; `Some(false)` continues
    /// outward), `docs/parity/notes/names-modules.md` §1. No new completion
    /// cache or image; repeated alias-worker attribution remains tsr-1yb.11.
    pub(crate) fn resolve_name_with_export_alias(
        &mut self,
        start: NodeId,
        name: &str,
        meaning: SymbolFlags,
    ) -> Option<SymbolId> {
        let binder = self.binder;
        let nodes = self.nodes;
        let node_map = self.node_map;
        binder.resolve_name_with_export_alias(
            nodes,
            node_map,
            start,
            name,
            meaning,
            |alias, mask| {
                let target = self.resolve_alias(alias)?;
                let flags = binder.symbols().get(binder.merged_symbol(target)).flags;
                if flags.intersects(SymbolFlags::ALIAS) {
                    return None;
                }
                Some(flags.intersects(mask))
            },
        )
    }

    /// A symbol's flags, **following the alias chain**.
    ///
    /// Ported from `Checker.getSymbolFlagsEx` (`checker.go:16367`) with both of
    /// its exclusion flags left `false`, which is what `getSymbolFlags`
    /// (`checker.go:16363`) passes and the only form this port reaches.
    ///
    /// # Why a symbol's own flags are not the answer
    ///
    /// `SymbolFlags::ALIAS` is disjoint from `SymbolFlags::VALUE`, so a raw
    /// flags test on an alias target says "no value" for `export { a }` naming
    /// `import a = N` — a namespace that plainly has the type `typeof N`.
    /// Upstream's `getTypeOfAlias` (`checker.go:18612`) takes its `Value` test
    /// over *this* function precisely so that a chain of aliases contributes the
    /// meaning of whatever it ends at.
    ///
    /// # The visited set is upstream's, not a local invention
    ///
    /// `seenSymbols` (`checker.go:16368`) is what stops `import a = b; export {
    /// a }` style chains from looping, and it is not a defensive cap.
    ///
    /// **It is also the only thing that makes a cross-file re-export cycle
    /// terminate in this port**, which was not true when it was written and is
    /// measured now. `a.ts` re-exporting `q` from `b.ts` while `b.ts` re-exports
    /// `q` from `a.ts` closes its loop *here*, in the chain walk, because
    /// `getTypeOfAlias`'s `VALUE` test runs over this function and
    /// [`Checker::resolve_alias`] is not itself recursive. Deleting
    /// `seen.contains(&target)` below **hangs**
    /// `tests/cross_file_aliases.rs::a_re_export_cycle_between_two_files_terminates`;
    /// that mutation is what pins this line, and it is the only one that does.
    ///
    /// An earlier draft of the cross-file arm added upstream's own
    /// `AliasTarget` resolution frame instead, on the assumption that it was the
    /// termination guard. It was not, and it could not fire at all — see
    /// [`Checker::resolve_alias`] for the measurement and for what would make it
    /// necessary.
    ///
    /// # Unsupported alias targets still decline
    ///
    /// When `resolveAlias` yields `unknownSymbol`, upstream returns
    /// `SymbolFlagsAll`. This port's `None` also covers unsupported targets,
    /// so it cannot imply native absence. Only a completed canonical-any
    /// receiver of a `CommonJS` property alias establishes the native unknown
    /// target here; its alias type still declines to `errorType`.
    pub(crate) fn get_symbol_flags(&mut self, symbol: SymbolId) -> SymbolFlags {
        let mut seen: Vec<SymbolId> = Vec::new();
        let mut current = symbol;
        let mut flags = self.binder.symbols().get(current).flags;
        while self.binder.symbols().get(current).flags.intersects(SymbolFlags::ALIAS) {
            let Some(target) = self.resolve_alias(current) else {
                if self.commonjs_property_alias_has_unknown_target(current) {
                    return SymbolFlags::from_bits_retain(u32::MAX);
                }
                break;
            };
            let target_flags = self.binder.symbols().get(target).flags;
            if target_flags.intersects(SymbolFlags::ALIAS) {
                if target == current || seen.contains(&target) {
                    break;
                }
                if seen.is_empty() {
                    seen.push(current);
                }
                seen.push(target);
            }
            flags |= target_flags;
            current = target;
        }
        flags
    }

    /// Native getTargetOfAliasLikeExpression -> resolveAlias -> getSymbolFlagsEx
    /// (5b1047d1 checker.go:14996/16266/16376). A completed canonical-any
    /// receiver has no property symbol. Read the original expression cache,
    /// never evaluate again or publish a synthetic target. Error/missing cache,
    /// active receiver ownership and loop-fixpoint state prove no such absence.
    fn commonjs_property_alias_has_unknown_target(&self, symbol: SymbolId) -> bool {
        if !self.flow_loop_stack.is_empty() {
            return false;
        }
        let Some(declaration) = self.declaration_of_alias_symbol(symbol) else { return false };
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(declaration) else {
            return false;
        };
        let Some(Expression::PropertyAccessExpression(access)) = binary.right else {
            return false;
        };
        let Some(receiver) = access.expression else { return false };
        if receiver.node_id().and_then(|id| self.node_types.get(&id).copied())
            != Some(self.intrinsics.any)
        {
            return false;
        }
        let mut root = receiver;
        while let Expression::PropertyAccessExpression(access) = root {
            let Some(receiver) = access.expression else { return false };
            root = receiver;
        }
        let Expression::Identifier(root) = root else { return false };
        let Some(owner) = root.node_id.and_then(|id| {
            self.binder.resolve_name(
                self.nodes,
                self.node_map,
                id,
                root.text,
                SymbolFlags::VALUE | SymbolFlags::TYPE | SymbolFlags::NAMESPACE,
            )
        }) else {
            return false;
        };
        !self.resolutions.on_stack(owner, PropertyName::Type)
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
    ///
    /// # `export { q }` is the other form that resolves in one file
    ///
    /// `getTargetOfAliasDeclaration` (`checker.go:15736`) dispatches on the
    /// alias declaration's kind, and its `KindExportSpecifier` case reaches
    /// `getTargetOfExportSpecifier` (`checker.go:14951`). That function branches
    /// on **the export declaration's module specifier**, not on the specifier
    /// itself: with one, it is `getExternalModuleMember`; without one, it is a
    /// plain `resolveEntityName` in the ordinary scope. `export { q }` is
    /// therefore a *local* lookup, and the only thing that made it look like
    /// module work was sharing a node kind with `export { q } from "./m"`.
    ///
    /// See [`Checker::export_specifier_target`] for the measurement and for the
    /// two forms that stay gapped.
    ///
    /// # The declaration is the **last alias-shaped** one, not the first
    ///
    /// `getDeclarationOfAliasSymbol` (`checker.go:16397`) is
    /// `core.FindLast(symbol.Declarations, ast.IsAliasSymbolDeclaration)`, and
    /// both halves of that matter for a **merged** symbol. `interface I { }`
    /// beside `export { I }` gives one symbol carrying `INTERFACE | ALIAS` whose
    /// `declarations[0]` is the `InterfaceDeclaration` — a node with no alias
    /// target at all. Reading `.first()` handed that node to the match below,
    /// which fell through to `None`, and the symbol answered `errorType` for a
    /// reason that had nothing to do with aliases.
    ///
    /// Found as **one line** in `examples/symbol_dispatch_split.rs`'s
    /// `UNCLASSIFIED KIND` control on the full corpus. One line is not why it is
    /// fixed: this is the exact shape recorded as the falsifier for the
    /// export-marker arm in `docs/architecture/checker-notes-arrays.md`
    /// (*"merged declarations are where to look — `export interface I {}` beside
    /// `export const I = 1`"*), so the control found the predicted failure and
    /// the prediction is what makes one line worth acting on.
    /// # Upstream's circularity frame is deliberately **not** ported, and this
    /// is the evidence
    ///
    /// `resolveAlias` pushes `TypeSystemPropertyNameAliasTarget`
    /// (`checker.go:16272`) and, on failure, reports
    /// `Circular_definition_of_import_alias_0`. Two files re-exporting through
    /// each other is a real shape, so that frame was written here first —
    /// a `PropertyName::AliasTarget` variant and a push/pop around the dispatch
    /// below.
    ///
    /// **It was measured and it could not fire, so it was removed.** With the
    /// frame disabled, every test in `tests/cross_file_aliases.rs` stays green,
    /// including the two-file re-export cycle. The reason is structural rather
    /// than a property of those fixtures: **this function is not
    /// self-recursive.** Its four arms reach `Binder::resolve_name`,
    /// [`Checker::export_specifier_target`],
    /// [`Checker::import_specifier_target`] and
    /// [`Checker::get_external_module_member`], and none of those calls back
    /// into `resolve_alias` or into `get_type_of_symbol` — they read symbol
    /// tables. Upstream's does recurse, through `resolveIndirectionAlias`
    /// (`checker.go:16293`), which this port does not have.
    ///
    /// A guard nobody can make fire reads as safety and supplies none;
    /// `docs/conventions.md` records the same failure one level up, in a control
    /// bucket that could only ever read zero. **What would make it necessary:**
    /// porting `resolveIndirectionAlias`, or any arm that resolves a target's
    /// own alias from inside this function. Whoever does that must restore the
    /// frame, and `a_re_export_cycle_between_two_files_terminates` is the test
    /// that will hang if they do not.
    ///
    /// # What *does* make a cycle terminate, and it is not this function
    ///
    /// [`Checker::get_symbol_flags`]'s visited set — upstream's own
    /// `seenSymbols` (`checker.go:16368`). `getTypeOfAlias` takes its `VALUE`
    /// test over the alias *chain*, so the chain walk is where a loop is closed,
    /// and the walk stops on a repeat. Deleting `seen.contains(&target)` there
    /// **hangs** `a_re_export_cycle_between_two_files_terminates`, which is the
    /// mutation that pins it.
    ///
    /// # Not memoised, where upstream memoises
    ///
    /// Upstream stores the answer in `aliasSymbolLinks[symbol].aliasTarget` and
    /// so computes each alias target once. This recomputes. The cost is repeated
    /// work on a chain, bounded because [`Checker::get_type_of_alias`] memoises
    /// the *type* in [`Checker::symbol_types`] and that is what every caller
    /// ultimately wants.
    pub fn resolve_alias(&mut self, symbol: SymbolId) -> Option<SymbolId> {
        let declaration = self.declaration_of_alias_symbol(symbol)?;
        match self.nodes.kind(declaration) {
            // getTargetOfImportEqualsDeclaration also accepts syntactic JS
            // require-initialized variables, independently of call admission.
            SyntaxKind::VariableDeclaration => {
                let Node::VariableDeclaration(variable) = self.node_map.get(declaration)? else {
                    return None;
                };
                let Some(Expression::CallExpression(call)) = variable.initializer else {
                    return None;
                };
                // Keep the immediate target. The existing alias-chain worker
                // owns cycle detection, including require/export= loops.
                let resolved = self.commonjs_require_target(call)?;
                return Some(self.import_equals_module_exports(resolved).unwrap_or(resolved));
            }
            // getTargetOfBinaryExpression (native checker.go:14990): an
            // assignment alias points at the RHS symbol in its lexical scope,
            // not the type of the whole module or an identically spelled local.
            SyntaxKind::BinaryExpression => {
                let Node::BinaryExpression(binary) = self.node_map.get(declaration)? else {
                    return None;
                };
                return match binary.right? {
                    Expression::Identifier(name) => self
                        .binder
                        .resolve_name(
                            self.nodes,
                            self.node_map,
                            name.node_id?,
                            name.text,
                            SymbolFlags::VALUE | SymbolFlags::TYPE | SymbolFlags::NAMESPACE,
                        )
                        .map(|target| self.binder.merged_symbol(target)),
                    Expression::ClassExpression(class) => self.binder.symbol_of(class.node_id?),
                    Expression::PropertyAccessExpression(access) => {
                        let tsr_ast::MemberName::Identifier(name) = access.name? else {
                            return None;
                        };
                        let receiver = access.expression?;
                        if let Some(owner) =
                            self.heritage_entity_symbol(receiver, SymbolFlags::NAMESPACE)
                            && let Some(&target) =
                                self.binder.symbols().get(owner).exports.get(name.text)
                        {
                            return Some(target);
                        }
                        // getTargetOfAliasLikeExpression's checked-expression
                        // fallback: mutable object members retain their actual
                        // receiver/property symbol, not a namespace name fit.
                        let receiver = self.check_expression(receiver);
                        self.get_property_of_type(receiver, name.text)
                    }
                    _ => None,
                };
            }
            // `getTargetOfExportSpecifier` (`checker.go:14951`) — both halves,
            // `export { q }` and `export { q } from "./m"`.
            SyntaxKind::ExportSpecifier => return self.export_specifier_target(declaration),
            // `getTargetOfImportSpecifier` (`checker.go:14647`).
            SyntaxKind::ImportSpecifier => return self.import_specifier_target(declaration),
            // `getTargetOfExportAssignment` (`checker.go:14889`) — `export = X`
            // where `X` is an identifier or a class expression. Every
            // other expression shape declines (upstream's
            // `getTargetOfAliasLikeExpression` handles more; each unported
            // shape is a miss, never a wrong target).
            SyntaxKind::ExportAssignment => return self.export_assignment_target(declaration),
            // `getTargetOfImportClause` (`checker.go:14528`) →
            // `getTargetOfModuleDefault` (`:14536`): the `module.exports`
            // arm, then the module's real `default` export. The synthetic
            // default (`canHaveSyntheticDefault`, interop) is ported only in
            // part — each such miss stays a gap (`checker-notes-modobj.md`
            // §10.11).
            SyntaxKind::ImportClause => return self.import_clause_default_target(declaration),
            // §503 re-opens §222's refusal under its own recorded condition:
            // "the qualification losses need the naming half of `bd tsr-e2u`,
            // and nothing landed since touched it" — §501 landed it.
            // `getTargetOfNamespaceExportDeclaration` (`checker.go:15011`):
            // `resolveExternalModuleSymbol(node.Parent.Symbol(),
            // dontResolveAlias=true)` — the containing file-module's symbol,
            // through its `export =`.
            SyntaxKind::NamespaceExportDeclaration => {
                let file = self.source_file_of(declaration)?;
                let module = self.binder.symbol_of(file)?;
                return Some(self.resolve_external_module_symbol(module));
            }
            // **The `NamespaceExportDeclaration` arm now exists — §503.** The
            // refusal below stood on a blocker (`bd tsr-e2u`'s naming half)
            // that §501 landed, which is exactly the re-test condition the
            // refusal's own record named. The history is kept because the
            // wrong turns are the useful part:
            // `getTargetOfNamespaceExportDeclaration` (`checker.go:15011`) is
            // three lines — `resolveExternalModuleSymbol(node.Parent.Symbol(),
            // dontResolveAlias=true)`, which this port's
            // `resolve_external_module_symbol` already matches exactly, since it
            // returns the `export=` symbol without resolving it further. §222
            // transcribed it and MEASURED IT DOWN.
            //
            // +1 case (`conformance/umd7`), 0 lost — and **−14 lines in
            // `conformance/umd-augmentation-1` alone**, of which the worst is
            // `>m : typeof m` going **right → `error`**. Not a gap becoming a
            // wrong answer this time (corollary 24's case) but a *correct* line
            // becoming wrong, which no case-level tally shows: the case was
            // already failing and stayed failing, so `+1 / −0` was the whole
            // report. The rest are qualification losses — `m.Vector` printing
            // as `Vector` — because making the UMD target reachable puts names
            // through a printer that cannot qualify them through the alias.
            //
            // That is the SAME blocker as `module_object_of`'s: `bd tsr-e2u`,
            // the naming half. Two independent arms now measure positive on
            // cases and negative on correctness for one missing capability,
            // which is the argument for building that capability rather than
            // any further arm that depends on it.
            //
            // Gating this to exclude the augmented shape would be fitting the
            // witness — corollary 20 — so it is left out whole.
            //
            // # Re-tested 2026-08-12 on a tree four arms newer; the refusal holds
            //
            // Corollary 31 says a refusal is evidence about a *tree*, so this was
            // re-run after §230, §232, §253 and §254 landed. The case delta
            // improved — **+2 instead of +1** (`umd7` and
            // `moduleAugmentationWithNonExistentNamedImport`) — and the damage is
            // **identical**: `conformance/umd-augmentation-1` still goes
            // `>m : typeof m` **right → `error`**, still loses 14 lines, still
            // spells `m.Vector` as `Vector`. Same for `umd4`, `umd5` and
            // `crashDeclareGlobalTypeofExport`.
            //
            // So the price did not move, because the blocker did not: the
            // qualification losses need the naming half of `bd tsr-e2u`, and
            // nothing landed since touched it. **Recorded so the next reader does
            // not spend the run again** — a re-test that confirms a refusal is
            // worth as much as one that overturns it, and only one of the two
            // usually gets written down.
            //
            // Consequence, correcting a note I sent the other lane:
            // `compiler/unusedImports13` is **not** one arm away. Its other
            // blocker was fixed by §254, and this one is what remains.
            _ => {}
        }
        // `getTargetOfNamespaceImport` (`checker.go:14724`) and
        // `getTargetOfNamespaceExport` (`checker.go:14742`), both of which are
        // `resolveESModuleSymbol(resolveExternalModuleName(...))`.
        //
        // **Deliberately narrower than upstream in one respect**, and it is the
        // respect this arm was refused for twice. `resolveESModuleSymbol`
        // (`checker.go:15568`) has a `cloneTypeAsModuleType` branch for a
        // namespace import, which is what lets upstream print `typeof ns3` for
        // the third of three aliases to one module. That clone is not ported.
        // Instead [`Checker::type_to_string_at`] refuses to name a module object
        // when more than one alias is in scope, so the ambiguous cases stay gaps
        // rather than becoming confidently wrong lines. Measured: 634 of 706
        // such lines have exactly one alias in scope and 630 of those name
        // correctly. See `docs/architecture/checker-notes-nameres.md` §14.
        if matches!(
            self.nodes.kind(declaration),
            SyntaxKind::NamespaceImport | SyntaxKind::NamespaceExport
        ) {
            let parent = self.nodes.parent(declaration)?;
            // A `NamespaceImport` hangs off an `ImportClause`, a
            // `NamespaceExport` directly off the `ExportDeclaration`.
            let owner = if self.nodes.kind(parent) == SyntaxKind::ImportClause {
                self.nodes.parent(parent)?
            } else {
                parent
            };
            let specifier = self.external_module_name(owner)?;
            if self.nodes.kind(declaration) == SyntaxKind::NamespaceImport
                && let Some(module_exports) = self.namespace_import_module_exports(owner, specifier)
            {
                return Some(module_exports);
            }
            return self.module_object_of(owner, specifier);
        }
        let Node::ImportEqualsDeclaration(node) = self.node_map.get(declaration)? else {
            // Every other alias form — an import clause, a namespace import,
            // `export =` — reaches its target through module resolution and
            // then prints the **alias's own** name rather than the target's.
            // `bd tsr-4jk` has the measurement: `import * as ns from "./m"`
            // records `>ns : typeof ns`, so resolving it in this port would
            // print `typeof <the stripped file path>` — a wrong line where a
            // gap stands. Same mechanism as the qualified `import a = b.c`
            // below, and it is why those forms are not built here.
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
                let flags = self.binder.symbols().get(found).flags;
                if flags.intersects(SymbolFlags::NAMESPACE) {
                    return Some(found);
                }
                if flags.intersects(SymbolFlags::ALIAS) {
                    // Force the alias even on a cold query. A known module
                    // copy retains its source's namespace meaning, not the
                    // callable value meaning of a plain exported function.
                    let value = self.get_type_of_symbol(found);
                    if let Some(&(_, source)) = self.module_value_clones.get(&value)
                        && let crate::types::TypeData::Anonymous { symbol, .. } =
                            self.store.get(source).data
                        && self
                            .binder
                            .symbols()
                            .get(symbol)
                            .flags
                            .intersects(SymbolFlags::NAMESPACE)
                    {
                        return Some(found);
                    }
                }
                None
            }
            // `getTargetOfImportEqualsDeclaration` (`checker.go:14441`) for the
            // `require("m")` half: `resolveExternalModuleName` then
            // `resolveExternalModuleSymbol`. The specifier is the argument of
            // the `require(...)` on the declaration itself rather than a
            // `module_specifier` on any ancestor — a distinction that cost one
            // probe run to find (§10).
            //
            // Unlike the namespace-import forms above, this arm **follows an
            // `export =`** (`resolveExternalModuleSymbol`, `checker.go:15556`)
            // through the assignment alias to its target — the §10.8 chain.
            // The naming constraint that used to forbid it (the target prints
            // under its own name, `typeof __React`, where upstream prints the
            // importing alias's) is answered by the rename in
            // [`crate::Checker::type_to_string_at`]'s path.
            ModuleReference::ExternalModuleReference(reference) => {
                let specifier = reference.expression?.node_id()?;
                let module = self.resolve_external_module_name(declaration, specifier)?;
                let resolved = self.resolve_external_module_symbol(module);
                if let Some(module_exports) = self.import_equals_module_exports(resolved) {
                    return Some(module_exports);
                }
                // resolveExternalModuleSymbol(..., dontResolveAlias=false)
                // follows alias exports, but a property-valued export= is
                // already the target (e.g. module.exports = 3 in JS).
                if !self.binder.symbols().get(resolved).flags.intersects(SymbolFlags::ALIAS) {
                    return Some(resolved);
                }
                self.resolve_alias(resolved)
            }
            // getSymbolOfPartOfRightHandSideOfImportEquals (checker.go:14493):
            // the complete qualified entity accepts value, type or namespace.
            ModuleReference::QualifiedName(name) => self.resolve_qualified_entity(name),
        }
    }

    /// The target of `import a = b.c`, for callers that need its **flags** and
    /// not its name.
    ///
    /// Resolution is `resolveEntityName`; serialization chooses the alias's
    /// accessible name separately at the reference site.
    pub(crate) fn qualified_alias_target(&mut self, symbol: SymbolId) -> Option<SymbolId> {
        let declaration = self.declaration_of_alias_symbol(symbol)?;
        let Node::ImportEqualsDeclaration(node) = self.node_map.get(declaration)? else {
            return None;
        };
        let ModuleReference::QualifiedName(name) = node.module_reference? else { return None };
        self.resolve_entity_name(
            tsr_ast::EntityName::QualifiedName(name),
            SymbolFlags::VALUE | SymbolFlags::TYPE | SymbolFlags::NAMESPACE,
        )
    }

    /// The declaration an alias symbol's target is read from.
    ///
    /// Ported from `Checker.getDeclarationOfAliasSymbol` (`checker.go:16397`),
    /// `core.FindLast(symbol.Declarations, ast.IsAliasSymbolDeclaration)`, over
    /// the `IsAliasSymbolDeclaration` kinds (`ast/utilities.go:2631`) this port
    /// can reach.
    ///
    /// # Which kinds are listed, and why the unreachable ones still are
    ///
    /// The predicate is a filter, so listing a kind [`Checker::resolve_alias`]
    /// answers `None` for costs nothing and buys the right *selection*: for
    /// `import a = require("./m")` merged with something else, the alias
    /// declaration is still the one that must be picked, even though the arm
    /// then declines it. Listing only the kinds this port resolves would make
    /// the predicate silently mean "the declarations we can answer", which is a
    /// different function.
    ///
    /// Four of upstream's arms are **not** listed, each because its test is
    /// unported rather than because the kind is rare:
    /// `KindImportClause` needs `Name() != nil`, `KindExportAssignment` needs
    /// `ExpressionIsAlias`, and `KindVariableDeclaration`/`KindBindingElement`
    /// and `KindBinaryExpression` are the JS `require`/`module.exports` forms
    /// behind `IsVariableDeclarationInitializedToRequire` and
    /// `GetAssignmentDeclarationKind`. Answering any of them by kind alone would
    /// be a guess, and the consequence of leaving them out is a *miss* — the
    /// walk finds no alias declaration and the symbol gaps — never a wrong
    /// target.
    /// The module specifier of the `ImportDeclaration` an import-form alias
    /// declaration sits under: `ImportSpecifier → NamedImports → ImportClause
    /// → ImportDeclaration`, or either shorter spine. §119's walk; a bounded
    /// parent climb because the spine is at most three hops.
    fn import_declaration_specifier(&self, declaration: NodeId) -> Option<NodeId> {
        let mut current = declaration;
        for _ in 0..4 {
            current = self.nodes.parent(current)?;
            if let Some(Node::ImportDeclaration(node)) = self.node_map.get(current) {
                return node.module_specifier.and_then(|specifier| specifier.node_id());
            }
        }
        None
    }

    /// §145's entity walk: root at NAMESPACE meaning, each right segment
    /// through merged exports. Any miss answers `None` — §144 owns the
    /// unresolvable-root error-answer; a resolving walk hands the leaf back.
    pub(crate) fn resolve_qualified_entity(
        &mut self,
        qualified: &tsr_ast::QualifiedName<'a>,
    ) -> Option<SymbolId> {
        let mut segments: Vec<&str> = Vec::new();
        let mut current = qualified;
        let root = loop {
            segments.push(current.right?.text);
            match current.left? {
                tsr_ast::EntityName::Identifier(name) => break name,
                tsr_ast::EntityName::QualifiedName(inner) => current = inner,
            }
        };
        let mut symbol = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            root.node_id?,
            root.text,
            SymbolFlags::NAMESPACE,
        )?;
        for segment in segments.iter().rev() {
            let merged = self.binder.merged_symbol(symbol);
            symbol = *self.binder.symbols().get(merged).exports.get(*segment)?;
        }
        Some(symbol)
    }

    pub(crate) fn declaration_of_alias_symbol(&self, symbol: SymbolId) -> Option<NodeId> {
        self.binder.symbols().get(symbol).declarations.iter().rev().copied().find(|&declaration| {
            match self.nodes.kind(declaration) {
                SyntaxKind::ImportEqualsDeclaration
                | SyntaxKind::NamespaceExportDeclaration
                | SyntaxKind::NamespaceImport
                | SyntaxKind::NamespaceExport
                | SyntaxKind::ImportSpecifier
                | SyntaxKind::ExportSpecifier => true,
                // The binder admits only unannotated, unexported JS variables
                // initialized to a bare string-literal require call.
                SyntaxKind::VariableDeclaration => {
                    self.in_js_file(declaration)
                        && matches!(self.node_map.get(declaration),
                            Some(Node::VariableDeclaration(variable))
                                if variable.r#type.is_none()
                                    && matches!(variable.initializer,
                                        Some(Expression::CallExpression(call))
                                            if call.arguments.len() == 1
                                                && matches!(call.expression, Some(Expression::Identifier(name)) if name.text == "require")
                                                && matches!(call.arguments[0], Expression::StringLiteral(_) | Expression::NoSubstitutionTemplateLiteral(_))))
                        && self.binder.symbol_of(declaration).is_some_and(|symbol| {
                            self.binder.symbols().get(symbol).flags.contains(SymbolFlags::ALIAS)
                        })
                }
                // IsAliasSymbolDeclaration accepts only CommonJS assignment
                // kinds with an alias-like RHS. A merged ALIAS flag alone
                // cannot admit later ordinary writes as new alias targets.
                SyntaxKind::BinaryExpression => {
                    self.in_js_file(declaration)
                        && matches!(self.node_map.get(declaration),
                            Some(Node::BinaryExpression(binary))
                                if binary.operator_token.is_some_and(|token| token.kind == SyntaxKind::EqualsToken)
                                    && (self.is_commonjs_export_property_assignment(binary)
                                        || binary.left.is_some_and(crate::assignment_declarations::is_module_exports_access)
                                            && !matches!(binary.right, Some(Expression::Identifier(name)) if name.text == "exports")))
                        && matches!(self.node_map.get(declaration),
                            Some(Node::BinaryExpression(binary)) if binary.right.is_some_and(|mut expression| {
                                if matches!(expression, Expression::ClassExpression(_)) {
                                    return true;
                                }
                                while let Expression::PropertyAccessExpression(access) = expression {
                                    if !matches!(access.name, Some(tsr_ast::MemberName::Identifier(_))) {
                                        return false;
                                    }
                                    let Some(receiver) = access.expression else { return false };
                                    expression = receiver;
                                }
                                matches!(expression, Expression::Identifier(_))
                            }))
                }
                // `KindExportAssignment` needs `ExpressionIsAlias`
                // (`ast/utilities.go:1872`); the shapes this port resolves are
                // an identifier and a class expression, and testing them here
                // rather than answering by kind keeps the predicate honest —
                // see the doc above.
                SyntaxKind::ExportAssignment => matches!(
                    self.node_map.get(declaration),
                    Some(Node::ExportAssignment(node))
                        if matches!(
                            node.expression,
                            Some(tsr_ast::Expression::Identifier(_) | tsr_ast::Expression::ClassExpression(_))
                        )
                ),
                // `KindImportClause` needs `Name() != nil` — a bare
                // `import "m"` declares nothing.
                SyntaxKind::ImportClause => matches!(
                    self.node_map.get(declaration),
                    Some(Node::ImportClause(node)) if node.name.is_some()
                ),
                _ => false,
            }
        })
    }

    /// `getTargetOfModuleDefault` (`checker.go:14536`), the plain half: the
    /// module's real `default` export, resolved through one more alias hop
    /// when `export default x` names a local.
    fn import_clause_default_target(&mut self, declaration: NodeId) -> Option<SymbolId> {
        let parent = self.nodes.parent(declaration)?;
        // §269: the clause's owner is a JSDoc `@import` tag in a JS file —
        // same shape, the specifier just lives on the tag.
        let specifier = match self.node_map.get(parent)? {
            // §292's narrowing: an import carrying ATTRIBUTES declines — the
            // attribute validity rules are unported, and upstream errors the
            // whole import where this road would type through it
            // (`importAttributes7/8`, the pair's 2 R→W).
            Node::ImportDeclaration(import) if import.attributes.is_none() => {
                import.module_specifier
            }
            Node::JSDocImportTag(import) => import.module_specifier,
            _ => return None,
        };
        let specifier = specifier?.node_id()?;
        let module = self.resolve_external_module_name(declaration, specifier)?;
        self.module_default_target(module, declaration, specifier)
    }

    /// TYPE naming's completed alias target, ported from native `resolveAlias`
    /// and `getTargetOfModuleDefault` (`checker.go:14536`, pinned 5b1047d).
    /// `trySymbolTable` (`symbolaccessibility.go:535`) resolves before reading
    /// exports; this reader does not change ordinary alias/value admission.
    ///
    /// Program SymbolIds/declarations and completed declaration-namespace tables
    /// own identities. The caller retains the original alias and ordered type
    /// arguments. A query-local path owns active/cyclic traversal; no separate
    /// alias, type, accessibility or member image is published. Other aliases
    /// delegate admission/completion to the existing ordinary worker. Missing/unsupported
    /// tables and active Type/DeclaredType work decline, never complete empty.
    /// Existing module resolution owns lookup; the scope/alias walk runs per
    /// naming query with no duplicate cache or performance claim (tsr-6.47.3.1.1).
    pub(crate) fn semantic_type_naming_alias_target(
        &mut self,
        symbol: SymbolId,
        meaning: SymbolFlags,
        path: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        let symbol = self.binder.merged_symbol(symbol);
        if path.contains(&symbol)
            || self.resolutions.on_stack(symbol, PropertyName::Type)
            || self.resolutions.on_stack(symbol, PropertyName::DeclaredType)
        {
            return None;
        }
        let namespace_complete = |checker: &Self, symbol| {
            let entry = checker.binder.symbols().get(symbol);
            entry.flags.intersects(SymbolFlags::MODULE)
                && !entry
                    .flags
                    .intersects(SymbolFlags::CLASS | SymbolFlags::FUNCTION | SymbolFlags::ALIAS)
                && entry.exports.is_present()
                && !entry.exports.contains_key(INTERNAL_EXPORT_STAR)
                && !entry.declarations.is_empty()
                && entry.declarations.iter().all(|&declaration| {
                    checker.source_file_of(declaration).is_some_and(|file| {
                        checker.module_host.is_some_and(|host| host.is_declaration_file(file))
                    }) && matches!(
                        checker.node_map.get(declaration),
                        Some(
                            Node::SourceFile(_)
                                | Node::ModuleDeclaration(tsr_ast::ModuleDeclaration {
                                    body: Some(_),
                                    ..
                                })
                        )
                    )
                })
        };
        path.push(symbol);
        let result = (|| {
            let entry = self.binder.symbols().get(symbol);
            if !entry.flags.contains(SymbolFlags::ALIAS) {
                if !entry.flags.intersects(meaning) || entry.declarations.is_empty() {
                    return None;
                }
                if entry.flags.intersects(SymbolFlags::MODULE) {
                    if !namespace_complete(self, symbol) {
                        return None;
                    }
                    let target = self.resolve_external_module_symbol(symbol);
                    if target != symbol {
                        return self.semantic_type_naming_alias_target(target, meaning, path);
                    }
                }
                return Some(symbol);
            }
            let declaration = self.declaration_of_alias_symbol(symbol)?;
            let target = if let Some(Node::ImportClause(_)) = self.node_map.get(declaration) {
                if matches!(
                    self.module_kind,
                    tsr_core::ModuleKind::Node16
                        | tsr_core::ModuleKind::Node18
                        | tsr_core::ModuleKind::Node20
                        | tsr_core::ModuleKind::NodeNext
                ) {
                    return None; // implied-format ownership is outside this slice
                }
                let Node::ImportDeclaration(import) =
                    self.node_map.get(self.nodes.parent(declaration)?)?
                else {
                    return None;
                };
                if import.attributes.is_some() {
                    return None;
                }
                let module = self.resolve_external_module_name(
                    declaration,
                    import.module_specifier?.node_id()?,
                )?;
                if !namespace_complete(self, module) {
                    return None;
                }
                let immediate = self.resolve_external_module_symbol(module);
                let owner = if immediate == module {
                    module
                } else {
                    self.semantic_type_naming_alias_target(immediate, SymbolFlags::NAMESPACE, path)?
                };
                // resolveExportByName reads the export= value's properties.
                // An unrepresented aliased property cannot prove default/marker absence.
                let property = |name| {
                    let found = self.binder.symbols().get(owner).exports.get(name).copied();
                    if immediate == module {
                        return Some(found);
                    }
                    match found {
                        Some(property)
                            if self
                                .binder
                                .symbols()
                                .get(property)
                                .flags
                                .contains(SymbolFlags::ALIAS) =>
                        {
                            None
                        }
                        Some(property)
                            if self
                                .binder
                                .symbols()
                                .get(property)
                                .flags
                                .intersects(SymbolFlags::VALUE) =>
                        {
                            Some(Some(property))
                        }
                        _ => Some(None),
                    }
                };
                let default = property("default")?;
                let marker = property("__esModule")?;
                // canHaveSyntheticDefault and isSyntacticDefault
                // (`checker.go:14818`, `utilities.go:250`): real syntax beats
                // synthesis; otherwise __esModule suppresses it.
                let syntactic = default.is_some_and(|default| {
                    self.binder.symbols().get(default).declarations.iter().any(|&declaration| {
                        match self.node_map.get(declaration) {
                            Some(Node::ExportAssignment(node)) => !node.is_export_equals,
                            Some(Node::ExportSpecifier(_) | Node::NamespaceExport(_)) => true,
                            Some(Node::ClassDeclaration(node)) => tsr_ast::has_syntactic_modifier(
                                node.modifiers,
                                SyntaxKind::DefaultKeyword,
                            ),
                            Some(Node::FunctionDeclaration(node)) => {
                                tsr_ast::has_syntactic_modifier(
                                    node.modifiers,
                                    SyntaxKind::DefaultKeyword,
                                )
                            }
                            Some(Node::InterfaceDeclaration(node)) => {
                                tsr_ast::has_syntactic_modifier(
                                    node.modifiers,
                                    SyntaxKind::DefaultKeyword,
                                )
                            }
                            Some(Node::TypeAliasDeclaration(node)) => {
                                tsr_ast::has_syntactic_modifier(
                                    node.modifiers,
                                    SyntaxKind::DefaultKeyword,
                                )
                            }
                            _ => false,
                        }
                    })
                });
                if !syntactic && marker.is_none() { immediate } else { default? }
            } else {
                // Import-equals privacy and every other ordinary admission
                // stay with their existing owner; no new alias form is opened.
                self.resolve_alias(symbol)?
            };
            self.semantic_type_naming_alias_target(target, meaning, path)
        })();
        path.pop();
        result
    }

    /// The default target shared by clauses and identifier-default specifiers.
    /// New synthetic defaults retain the immediate `export=` link; only a
    /// complete, uncloned plain-TS file-module chain establishes eligibility.
    ///
    /// `getTargetOfModuleDefault` (`checker.go:14536`) opens with the
    /// `module.exports` arm: a `CommonJS`-emitted specifier of an ES-module file
    /// under `module: node20`..`nodenext` takes the module's
    /// `"module.exports"` export (`resolveExportByName`, `checker.go:14615`)
    /// before any default — `__importDefault(require(m)).default` is that
    /// export's value. `specifier` is `getModuleSpecifierForImportOrExport`.
    fn module_default_target(
        &mut self,
        module: SymbolId,
        declaration: NodeId,
        specifier: NodeId,
    ) -> Option<SymbolId> {
        if self.esm_file_used_with_commonjs_syntax(module, specifier)
            && let Some(module_exports) =
                self.resolve_export_by_name(module, INTERNAL_MODULE_EXPORTS)
        {
            return Some(module_exports);
        }
        let synthetic = (|| {
            if self.module_kind != tsr_core::ModuleKind::CommonJS || self.in_js_file(declaration) {
                return None;
            }
            let immediate = self.resolve_external_module_symbol(module);
            if immediate == module {
                return None;
            }
            let mut current = immediate;
            let mut seen = Vec::new();
            // Match the existing naming walk's bound, but establish completeness
            // ourselves: resolve_alias_fully also returns partial/cyclic links.
            for _ in 0..8 {
                if !self.binder.symbols().get(current).flags.contains(SymbolFlags::ALIAS) {
                    break;
                }
                if seen.contains(&current) {
                    return None;
                }
                seen.push(current);
                let link = self.declaration_of_alias_symbol(current)?;
                let file = self.source_file_of(link)?;
                if self.nodes.parent(link) != Some(file)
                    || self.in_js_file(file)
                    || self.nodes.flags(file).contains(NodeFlags::JSON_FILE)
                    || self.module_host?.is_declaration_file(file)
                {
                    return None;
                }
                // Prefilter before resolving. In particular, resolve_alias on
                // external import-equals can follow an unchecked export= link,
                // and default/specifier aliases can reenter this reader.
                current = match self.node_map.get(link)? {
                    Node::ExportAssignment(node)
                        if node.is_export_equals
                            && matches!(node.expression, Some(Expression::Identifier(_))) =>
                    {
                        self.export_assignment_target(link)?
                    }
                    Node::ImportEqualsDeclaration(node) => {
                        let ModuleReference::ExternalModuleReference(reference) =
                            node.module_reference?
                        else {
                            return None;
                        };
                        let specifier = reference.expression?.node_id()?;
                        let imported = self.resolve_external_module_name(link, specifier)?;
                        self.resolve_external_module_symbol(imported)
                    }
                    _ => return None,
                };
                current = self.binder.merged_symbol(current);
            }
            let entry = self.binder.symbols().get(current);
            let [file] = entry.declarations.as_slice() else { return None };
            if entry.flags != SymbolFlags::VALUE_MODULE
                || self.nodes.kind(*file) != SyntaxKind::SourceFile
                || self.binder.symbol_of(*file) != Some(current)
                || self.in_js_file(*file)
                || self.nodes.flags(*file).contains(NodeFlags::JSON_FILE)
                || self.module_host?.is_declaration_file(*file)
            {
                return None;
            }
            let value = self.get_type_of_symbol(current);
            if !matches!(self.store.get(value).data,
                crate::types::TypeData::Anonymous { symbol, signature: false, .. }
                    if symbol == current)
                || self.module_value_clones.contains_key(&value)
                || !self.get_signatures_of_symbol(current)?.is_empty()
            {
                return None;
            }
            Some(immediate)
        })();
        if synthetic.is_some() {
            return synthetic;
        }
        if self.nodes.kind(declaration) != SyntaxKind::ImportClause {
            // Preserve the specifiers' existing real-default lookup, and their
            // refusals outside the proven synthetic domain. Clause-only legacy
            // JSON/JS/ambient behavior below must not expand those domains.
            return (self.resolve_external_module_symbol(module) == module)
                .then(|| self.get_export_of_module(module, "default"))
                .flatten();
        }
        if self
            .binder
            .symbols()
            .get(module)
            .declarations
            .iter()
            .any(|&id| self.nodes.flags(id).contains(NodeFlags::JSON_FILE))
        {
            return Some(self.resolve_external_module_symbol(module));
        }
        // getTargetOfModuleDefault gives a synthetic CommonJS default priority
        // over a real exports.default when no __esModule marker is present.
        if self.can_have_synthetic_default(module)
            && self
                .binder
                .symbols()
                .get(module)
                .declarations
                .iter()
                .any(|&id| self.nodes.kind(id) == SyntaxKind::SourceFile && self.in_js_file(id))
        {
            return Some(self.resolve_external_module_symbol(module));
        }
        let default = self.binder.symbols().get(module).exports.get("default").copied();
        let Some(default) = default else {
            // §132: no explicit `default` but an `export =` — the SYNTHETIC
            // default. `canHaveSyntheticDefault`'s declaration-file arm
            // grants it regardless of the interop options, and
            // `getTargetOfModuleDefault` then resolves the module symbol
            // through the assignment (es6ExportEqualsInterop's x-family
            // wants: `x2 : { a: number; b: number; }`). A module without
            // `export =` keeps the miss.
            let resolved = self.resolve_external_module_symbol(module);
            if resolved == module || !self.can_have_synthetic_default(module) {
                return None;
            }
            // AMBIENT modules only (`declare module "x"`): a real-file
            // module gaining a default alias flips the printer's chosen
            // spelling for every qualified reference through the other
            // aliases (importEquals1's 6 G→W — `types.A` became
            // `import("./a").A`); an ambient module's naming road is stable.
            let ambient =
                self.binder.symbols().get(module).declarations.iter().any(|&declaration| {
                    self.nodes.kind(declaration) == SyntaxKind::ModuleDeclaration
                });
            if !ambient {
                return None;
            }
            let target = self.resolve_alias(resolved)?;
            // ONLY variable targets: their type prints STRUCTURALLY
            // (`{ a: number; b: number; }`). A namespace/class/function
            // target prints `typeof <name>`, and adding a SECOND alias to
            // the same target broke the one-alias rename (`typeof z4` →
            // `typeof Foo`, 10 R→W on this pair) — the tsr-4jk constraint,
            // fired again; those stay gaps until per-site naming exists.
            let flags = self.binder.symbols().get(target).flags;
            if flags.intersects(SymbolFlags::NAMESPACE | SymbolFlags::CLASS | SymbolFlags::FUNCTION)
            {
                return None;
            }
            return Some(target);
        };
        let default = self.binder.merged_symbol(default);
        if self.binder.symbols().get(default).flags.intersects(SymbolFlags::ALIAS) {
            return self.resolve_alias(default);
        }
        Some(default)
    }

    /// `getTargetOfExportAssignment` (`checker.go:14889`) through
    /// `getTargetOfAliasLikeExpression` (`checker.go:14996`) for the class
    /// expression and identifier shapes: `export = class {}` answers
    /// `checkExpressionCached(expression).symbol`, and `X` of `export = X` is
    /// resolved where it is written, with the full alias meaning
    /// (`SymbolFlagsValue | Type | Namespace`, `checker.go:15751`).
    fn export_assignment_target(&mut self, declaration: NodeId) -> Option<SymbolId> {
        let Node::ExportAssignment(node) = self.node_map.get(declaration)? else {
            return None;
        };
        if let Some(expression @ tsr_ast::Expression::ClassExpression(class)) = node.expression {
            // `checkClassExpression` answers `getTypeOfSymbol` of the class's
            // own symbol, so the checked type's symbol is that symbol.
            self.check_expression(expression);
            return self
                .binder
                .symbol_of(class.node_id?)
                .map(|symbol| self.binder.merged_symbol(symbol));
        }
        let Some(tsr_ast::Expression::Identifier(name)) = node.expression else {
            return None;
        };
        let found = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            name.node_id?,
            name.text,
            SymbolFlags::VALUE | SymbolFlags::TYPE | SymbolFlags::NAMESPACE,
        )?;
        Some(self.binder.merged_symbol(found))
    }

    /// The symbol an **export specifier** names, for the half that resolves
    /// inside one file.
    ///
    /// Ported from `Checker.getTargetOfExportSpecifier` (`checker.go:14951`),
    /// reached from `getTargetOfAliasDeclaration`'s `KindExportSpecifier` case
    /// (`checker.go:15751`), which is also where the meaning comes from:
    /// `SymbolFlagsValue | SymbolFlagsType | SymbolFlagsNamespace`.
    ///
    /// # The branch is on the *export declaration*, not on the specifier
    ///
    /// Upstream's three cases are, in order: a module specifier on the
    /// grandparent `ExportDeclaration` (`getExternalModuleMember` — module
    /// resolution, which this port does not have); a string-literal name
    /// (`resolved = nil`); otherwise `resolveEntityName` in the ordinary scope.
    ///
    /// So `export { q }` and `export { q } from "./m"` share a node kind and
    /// nothing else, and only the second needs a module graph. That is the
    /// entire finding behind this arm: a row named `SymbolFlags(ALIAS) / no
    /// value declaration` reads as module work, and **19.4% of it was a local
    /// name lookup** (`docs/architecture/checker-notes-symbols.md`).
    ///
    /// # What it deliberately does not do
    ///
    /// - **`ModuleExportName::StringLiteral`** — `export { "a-b" as c }` —
    ///   answers `None`, which is upstream's `resolved = nil` and not a gap.
    /// - **`resolveIndirectionAlias`.** Upstream passes `dontRecursivelyResolve
    ///   = true` here and then takes a second step in `resolveAlias`
    ///   (`checker.go:16280`) when the target is itself a pure alias. This
    ///   returns the symbol it found, and the chain is followed one level up
    ///   instead: [`Checker::get_symbol_flags`] walks it for the `VALUE` test
    ///   and [`Checker::get_type_of_symbol`] re-enters
    ///   [`Checker::get_type_of_alias`] for the type. Both ends of that were
    ///   needed — a raw flags test rejects an alias target before the type is
    ///   ever asked for, which is what made `export { a }` naming
    ///   `import a = N` answer `errorType` in the first draft of this arm.
    /// - **No meaning filter after the lookup.** The `import a = b` arm above
    ///   needs one because it resolves in `NAMESPACE` alone while
    ///   `Binder::resolve_name`'s `locals` lookup is not meaning-filtered. Here
    ///   upstream's meaning is `Value | Type | Namespace`, which every symbol a
    ///   local lookup can return already satisfies, so a filter would be
    ///   decoration. `get_type_of_alias`'s own `SymbolFlags::VALUE` test
    ///   (`checker.go:18612`) is what makes `export { I }` for an interface
    ///   answer `errorType` — and that is the *right* answer to compute, even
    ///   though upstream prints `errorType` as `any` and this port reports it as
    ///   a gap.
    fn export_specifier_target(&mut self, declaration: NodeId) -> Option<SymbolId> {
        let Node::ExportSpecifier(specifier) = self.node_map.get(declaration)? else {
            return None;
        };
        // The grandparent is the `ExportDeclaration`: specifier -> NamedExports
        // -> ExportDeclaration. Its module specifier is the whole test.
        let clause = self.nodes.parent(declaration)?;
        let declaration_of_export = self.nodes.parent(clause)?;
        let export = self.node_map.get(declaration_of_export)?;
        let Node::ExportDeclaration(export) = export else { return None };
        if export.module_specifier.is_some() {
            if export.attributes.is_none()
                && let Some(tsr_ast::ModuleExportName::Identifier(name)) =
                    specifier.property_name.or(specifier.name)
                && name.text == "default"
            {
                let module_specifier = export.module_specifier?.node_id()?;
                let module = self.resolve_external_module_name(declaration, module_specifier)?;
                return self.module_default_target(module, declaration, module_specifier);
            }
            // `case exportDeclaration.ModuleSpecifier() != nil:
            // getExternalModuleMember(exportDeclaration, node, …)`
            // (`checker.go:14966`). The *export declaration* is the node the
            // specifier is read from, which is why the id is threaded rather
            // than the specifier's own.
            return self.get_external_module_member(declaration_of_export, declaration);
        }
        // `node.PropertyNameOrName()`: `export { q as r }` looks up `q`.
        let name = match specifier.property_name.or(specifier.name)? {
            tsr_ast::ModuleExportName::Identifier(name) => name,
            tsr_ast::ModuleExportName::StringLiteral(_) => return None,
        };
        self.binder.resolve_name(
            self.nodes,
            self.node_map,
            name.node_id?,
            name.text,
            SymbolFlags::VALUE | SymbolFlags::TYPE | SymbolFlags::NAMESPACE,
        )
    }

    /// The symbol an **import specifier** names — `import { x } from "./m"`.
    ///
    /// Ported from `Checker.getTargetOfImportSpecifier` (`checker.go:14647`),
    /// reduced to its second half. Upstream's first half handles
    /// `import { default as d }` through `getTargetOfModuleDefault`
    /// (`checker.go:14536`); for a module that has a real `export default`, that
    /// function reduces to `resolveExportByName(moduleSymbol, "default", …)`,
    /// which is the same `exports` lookup the fall-through below performs. The
    /// two differ only where there is **no** real default and upstream
    /// synthesises one — unported, and a miss rather than a wrong target.
    ///
    /// `root := node.Parent.Parent.Parent` (`checker.go:14658`) is the
    /// `ImportDeclaration`: specifier → `NamedImports` → `ImportClause` →
    /// `ImportDeclaration`. Upstream's `IsBindingElement` case is the JS
    /// destructured-`require` form, which this port does not reach.
    fn import_specifier_target(&mut self, declaration: NodeId) -> Option<SymbolId> {
        let named_imports = self.nodes.parent(declaration)?;
        let clause = self.nodes.parent(named_imports)?;
        let import = self.nodes.parent(clause)?;
        if let Some(Node::ImportSpecifier(specifier)) = self.node_map.get(declaration)
            && let Some(tsr_ast::ModuleExportName::Identifier(name)) = specifier
                .property_name
                .or(specifier.name.map(tsr_ast::ModuleExportName::Identifier))
            && name.text == "default"
            && let Some(Node::ImportDeclaration(node)) = self.node_map.get(import)
            && node.attributes.is_none()
        {
            let module_specifier = node.module_specifier?.node_id()?;
            let module = self.resolve_external_module_name(declaration, module_specifier)?;
            return self.module_default_target(module, declaration, module_specifier);
        }
        self.get_external_module_member(import, declaration)
    }

    /// The symbol a named import or re-export names inside another module.
    ///
    /// Ported from `Checker.getExternalModuleMember` (`checker.go:14667`) with
    /// `dontResolveAlias = true`, which is what both callers pass. `node` is the
    /// `ImportDeclaration` or `ExportDeclaration` that carries the module
    /// specifier; `specifier` is the `ImportSpecifier` or `ExportSpecifier` that
    /// carries the name.
    ///
    /// **This one function serves both cross-file forms this arm builds** —
    /// `import { x } from "./m"` and `export { q } from "./m"` — which is why
    /// they are one item and not two. Upstream reaches it from
    /// `getTargetOfImportSpecifier` (`checker.go:14647`) and from
    /// `getTargetOfExportSpecifier`'s module-specifier case
    /// (`checker.go:14966`).
    ///
    /// # Why these two forms and not the ones that reach a module symbol
    ///
    /// `import * as ns from "./m"` and `import a = require("./m")` resolve
    /// *more* easily — they need no name lookup at all — and are deliberately
    /// not built. Their declaration name prints the **local alias**, not the
    /// module: `conformance/exportAsNamespace4(module=commonjs).types` records
    /// `>ns : typeof ns` for `import * as ns from './0'`, because upstream's
    /// node builder emits the shortest accessible chain to the symbol. A module
    /// symbol's name here is the file path with its extension stripped
    /// (`bind_source_file_as_external_module`, `crates/tsr-binder/src/binder.rs`),
    /// so this port would print `typeof /0` — a **wrong** line where a gap
    /// stands. That is the same limitation already documented on
    /// [`Checker::resolve_alias`] for `import a = foo.bar.baz`, reaching two
    /// more forms; `bd tsr-4jk` carries the measurement (692 lines).
    ///
    /// A named import has no such problem, because the target is an ordinary
    /// export symbol carrying its own name.
    ///
    /// # What is deliberately not ported, each a miss and never a wrong target
    ///
    /// - ~~**A module with `export =`.**~~ **Partially ported.** The value
    ///   member is read from the resolved target's type, and supplemental
    ///   exports remain on the original module, as upstream's
    ///   `getExternalModuleMember` (`checker.go:14667`) requires. The two
    ///   representable shortcuts from `combineValueAndTypeSymbols`
    ///   (`checker.go:14717`) are ported too; the one case that needs a freshly
    ///   allocated synthetic symbol — separate value and type-only symbols of
    ///   the same name — stays a miss because the checker reads the binder's
    ///   immutable symbol store.
    /// - ~~**`export *` re-exports.**~~ **Ported** — see
    ///   [`Checker::get_export_from_star`]. The refusal was accurate when it was
    ///   written: the binder collected no `__export` symbol at all, so the
    ///   information the checker needed did not exist. Both halves landed
    ///   together.
    /// - **A shorthand ambient module** (`isShorthandAmbientModuleSymbol`,
    ///   `internal/checker/utilities.go:198`), which upstream answers with the
    ///   module symbol itself. Unreachable here: ambient modules are
    ///   `tryFindAmbientModule`, which
    ///   [`Checker::resolve_external_module_name`] does not port.
    fn get_external_module_member(&mut self, node: NodeId, specifier: NodeId) -> Option<SymbolId> {
        let module_specifier = self.external_module_name(node)?;
        let module_symbol = self.resolve_external_module_name(node, module_specifier)?;
        // `specifier.PropertyNameOrName()` (`checker.go:14677`). A string
        // literal name — `import { "a-b" as c }` — is a valid module export
        // name upstream; it is not looked up here because
        // `SymbolTable` keys are the identifier text and the two spellings have
        // not been checked to agree. `None` is a miss.
        let name = match self.node_map.get(specifier)? {
            Node::ImportSpecifier(node) => {
                node.property_name.or(node.name.map(tsr_ast::ModuleExportName::Identifier))
            }
            Node::ExportSpecifier(node) => node.property_name.or(node.name),
            _ => return None,
        }?;
        let tsr_ast::ModuleExportName::Identifier(name) = name else { return None };
        // `resolveESModuleSymbol` (`checker.go:15568`) reduces to
        // `resolveExternalModuleSymbol(moduleSymbol, dontResolveAlias = true)`
        // for both callers here: its synthetic-default and
        // `cloneTypeAsModuleType` arms are guarded by `namespaceImport != nil ||
        // IsImportCall(referenceParent)`, and a *named* import or re-export is
        // neither.
        let target = self.resolve_external_module_symbol(module_symbol);
        if target == module_symbol {
            return self.get_export_of_module(module_symbol, name.text);
        }

        // Native routes the export name `default` through
        // `getTargetOfModuleDefault` before this ordinary-member branch. This
        // port's caller currently reaches here instead, but selecting the
        // export-equals target's `.default` property is observably wrong: the
        // alias denotes the synthetic default module object, not that property.
        // Preserve the prior miss until the dedicated default road can retain
        // its per-site alias spelling.
        if name.text == "default" {
            return None;
        }

        // `getExternalModuleMember`'s `export =` branch: VALUE members belong
        // to the exported target's type, while supplemental TYPE/NAMESPACE
        // exports belong to the original module. Looking only in either place
        // loses the other meaning.
        let target_type = self.get_type_of_symbol(target);
        let value = self.get_property_of_type_ex(target_type, name.text, true);
        let value_was_found = value.is_some();
        // This semantic resolver must not outrun the site-aware spelling lane.
        // A pure alias target can replace a written local type name with the
        // remote declaration's name, while an object/function member can carry
        // named types whose shortest accessible chain depends on this import
        // site. Scalar members retain their native type identity: enum types
        // keep their owner for type_to_string_at's qualification, and unique
        // symbols stay unique on the import (only a copied initializer widens
        // to symbol). Neither should be replaced with its primitive base or
        // with the export-equals namespace object.
        // The lookup above separately excludes Object/Function fallback
        // members, matching native's `skipObjectFunctionPropertyAugment = true`;
        // filtering by the member's type would not suffice because fallback
        // members such as Function.length and Function.name are primitives.
        let value = if let Some(value) = value {
            let flags = self.binder.symbols().get(value).flags;
            if flags.intersects(SymbolFlags::ALIAS) {
                None
            } else {
                let value_type = self.get_type_of_symbol(value);
                let type_flags = self.store.get(value_type).flags;
                let site_independent = TypeFlags::STRING
                    | TypeFlags::NUMBER
                    | TypeFlags::BIG_INT
                    | TypeFlags::BOOLEAN
                    | TypeFlags::ES_SYMBOL
                    | TypeFlags::LITERAL
                    | TypeFlags::VOID_LIKE
                    | TypeFlags::NULL;
                type_flags
                    .intersects(
                        site_independent | TypeFlags::ENUM_LIKE | TypeFlags::UNIQUE_ES_SYMBOL,
                    )
                    .then_some(value)
            }
        } else {
            None
        };
        // getExportsOfModuleWorker (checker.go:16148) follows export= before
        // reading exports. Only TYPE/NAMESPACE-only names absent from the
        // target's exports are carried over from the original module; its
        // unrelated value exports must not replace a target property.
        let target_exports = &self.binder.symbols().get(self.binder.merged_symbol(target)).exports;
        let supplemental = target_exports.get(name.text).copied().or_else(|| {
            let supplemental = self.get_export_of_module(module_symbol, name.text)?;
            let flags = self.get_symbol_flags(supplemental);
            (flags.intersects(SymbolFlags::TYPE | SymbolFlags::NAMESPACE)
                && !flags.intersects(SymbolFlags::VALUE))
            .then_some(supplemental)
        });
        // Finding and then declining a value is different from finding no
        // value. In the former case, returning a type-only supplement would
        // silently erase the value meaning that native combines with it.
        if value_was_found && value.is_none() && supplemental.is_some() {
            return None;
        }
        match (value, supplemental) {
            (None, supplemental) => supplemental,
            (value, None) => value,
            (Some(value), Some(supplemental)) => {
                // The first two native shortcuts need no synthetic symbol.
                // Otherwise selecting either half would erase one meaning.
                let supplemental_flags = self.binder.symbols().get(supplemental).flags;
                if supplemental_flags.intersects(SymbolFlags::VALUE) {
                    Some(supplemental)
                } else {
                    let value_flags = self.binder.symbols().get(value).flags;
                    value_flags
                        .intersects(SymbolFlags::TYPE | SymbolFlags::NAMESPACE)
                        .then_some(value)
                }
            }
        }
    }

    /// TS2305 — `Module '{0}' has no exported member '{1}'.`
    ///
    /// `errorNoModuleMemberSymbol` (`checker.go:14883`), reached when
    /// [`Checker::get_external_module_member`] finds no export for the name.
    ///
    /// Upstream chooses between six messages there: TS2724 for a near miss,
    /// TS2614 when the module has a default export, then
    /// `reportNonExportedMember`'s TS2459 / TS2460 / TS2305 and
    /// `reportInvalidImportEqualsExportMember`. An `export =` module is
    /// decided only when its target is declaration-shaped; see
    /// [`Checker::export_equals_module_member_exists`]. See §228.
    pub(crate) fn report_missing_module_export(&mut self, specifier: NodeId) -> Option<()> {
        let declaration = self.import_or_export_declaration_of(specifier)?;
        let module_specifier = self.external_module_name(declaration)?;
        let module_symbol = self.resolve_external_module_name(declaration, module_specifier)?;
        // `resolveESModuleSymbol`: the `export =` target, or the module.
        // `getExportsOfModule` follows the target to its final symbol; an
        // `export =` this port cannot resolve (`export = a.b`) declines.
        let export_equals = self.resolve_external_module_symbol(module_symbol);
        let has_export_equals = export_equals != module_symbol;
        let target = if has_export_equals {
            let target = self.binder.merged_symbol(self.resolve_alias_fully(export_equals));
            // Only a declaration-shaped target is decided here — a namespace,
            // class, function or enum, whose static members are its own
            // tables. A variable's members come from `getPropertyOfType` over
            // an arbitrary declared type, whose misses in this port (unions,
            // augmented interfaces) are not proof of absence.
            let flags = self.binder.symbols().get(target).flags;
            let declaration_shaped = SymbolFlags::VALUE_MODULE
                | SymbolFlags::CLASS
                | SymbolFlags::FUNCTION
                | SymbolFlags::ENUM;
            if flags.intersects(SymbolFlags::ALIAS)
                || !flags.intersects(SymbolFlags::MODULE | declaration_shaped)
                || flags.intersects(SymbolFlags::VALUE - declaration_shaped)
            {
                return None;
            }
            target
        } else {
            module_symbol
        };
        let name = match self.node_map.get(specifier)? {
            Node::ImportSpecifier(node) => {
                node.property_name.or(node.name.map(tsr_ast::ModuleExportName::Identifier))
            }
            Node::ExportSpecifier(node) => node.property_name.or(node.name),
            _ => return None,
        }?;
        // **A string literal is a legal export name since ES2022**, and
        // `{ "missing" as x }` is exactly the shape this rule is for. §565.
        let (text, name_id) = match name {
            tsr_ast::ModuleExportName::Identifier(name) => (name.text, name.node_id?),
            tsr_ast::ModuleExportName::StringLiteral(name) => (name.text, name.node_id?),
        };
        if !has_export_equals {
            if self.get_export_of_module(module_symbol, text).is_some() {
                return None;
            }
        } else if self.export_equals_module_member_exists(module_symbol, target, text) {
            return None;
        }
        // `getExternalModuleMember` (`checker.go`): a missing `default` is
        // answered by the module itself when `canHaveSyntheticDefault` holds,
        // and is then not a missing member.
        if text == "default" && self.can_have_synthetic_default(module_symbol) {
            return None;
        }
        let entry = self.binder.symbols().get(module_symbol);
        // §186 — an empty table cannot be asked which member is missing. A
        // module this port never filled would answer "no member" for every
        // import in the file.
        // §186's decline, narrowed as §558 and §561 narrowed its other two
        // copies: a module symbol declared by a `SourceFile` had its exports
        // computed by the binder, so an empty table means *this module exports
        // nothing* rather than *this port recorded nothing*. §565.
        if entry.exports.is_empty()
            && !entry.declarations.iter().any(|&declaration| {
                matches!(self.node_map.get(declaration), Some(Node::SourceFile(_)))
            })
        {
            return None;
        }
        let entry = self.binder.symbols().get(module_symbol);
        let has_default = entry.exports.contains_key("default");
        let value_declaration = entry.value_declaration;
        // `getSuggestedSymbolForNonexistentModule(name, targetSymbol)` spells
        // against the export target's exports.
        let candidates: Vec<&str> = self
            .binder
            .symbols()
            .get(self.binder.merged_symbol(target))
            .exports
            .keys()
            .copied()
            .collect();
        // `getSuggestedSymbolForNonexistentModule` is tried **first**, so a
        // near miss makes TS2305 a wrong code at a right position — the failure
        // §185's falsifier caught for TS2694 on four of seven wrong lines.
        let suggestion = spelling_suggestion(text, &candidates).map(str::to_string);
        let span = self.nodes.span(name_id);
        let file = self.source_file_of_for_diagnostics(specifier)?;
        let module_name = self.quoted_module_name(module_specifier);
        if let Some(suggestion) = suggestion {
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::_0_HAS_NO_EXPORTED_MEMBER_NAMED_1_DID_YOU_MEAN_2,
                    span,
                    [module_name, text.to_string(), suggestion],
                ),
            );
            return None;
        }
        // `moduleSymbol.Exports[InternalSymbolNameDefault] != nil`: TS2614.
        if has_default {
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::MODULE_0_HAS_NO_EXPORTED_MEMBER_1_DID_YOU_MEAN_TO_USE_IMPORT_1_FROM_0_INSTEAD,
                    span,
                    [module_name, text.to_string()],
                ),
            );
            return None;
        }
        // `reportNonExportedMember` (`checker.go:14908`) splits again on
        // whether the module file declares the name **locally**, into three
        // outcomes of which one is built. §679.
        if let Some(source_file) = value_declaration
            && let Some(local) =
                self.binder.locals(source_file).and_then(|locals| locals.get(text).copied())
        {
            // `export =` is its own pair of outcomes: the local *is* the
            // `export =` target (`reportInvalidImportEqualsExportMember`), or
            // TS2305.
            if let Some(&export_equals) = entry.exports.get("export=") {
                if self.same_reference_identity(export_equals)
                    == self.same_reference_identity(local)
                {
                    self.report_invalid_import_equals_export_member(
                        file,
                        specifier,
                        span,
                        text,
                        &module_name,
                    );
                } else {
                    self.report(
                        file,
                        Diagnostic::with_args(
                            &messages::MODULE_0_HAS_NO_EXPORTED_MEMBER_1,
                            span,
                            [module_name, text.to_string()],
                        ),
                    );
                }
                return None;
            }
            // `findInMap(exports, sameReference(localSymbol))`: when an export
            // *is* this local under another name, TS2460 names that export.
            // `getSymbolIfSameReference` compares the merged, fully resolved
            // symbols, so `export { a as b }` — an alias — reaches the local.
            let exports: Vec<(&str, SymbolId)> =
                entry.exports.iter().map(|(&name, &symbol)| (name, symbol)).collect();
            let local = self.same_reference_identity(local);
            if let Some((exported_name, _)) = exports
                .into_iter()
                .find(|&(_, exported)| self.same_reference_identity(exported) == local)
            {
                self.report(
                    file,
                    Diagnostic::with_args(
                        &messages::MODULE_0_DECLARES_1_LOCALLY_BUT_IT_IS_EXPORTED_AS_2,
                        span,
                        [module_name, text.to_string(), exported_name.to_string()],
                    ),
                );
                return None;
            }
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::MODULE_0_DECLARES_1_LOCALLY_BUT_IT_IS_NOT_EXPORTED,
                    span,
                    [module_name, text.to_string()],
                ),
            );
            return None;
        }
        self.report(
            file,
            Diagnostic::with_args(
                &messages::MODULE_0_HAS_NO_EXPORTED_MEMBER_1,
                span,
                [module_name, text.to_string()],
            ),
        );
        None
    }

    /// `getExternalModuleMember`'s lookup for an `export =` module
    /// (`checker.go`): a property of the target's type with
    /// `skipObjectFunctionPropertyAugment`, else `getExportOfModule` over
    /// `getExportsOfModule(moduleSymbol)` — the target's exports, plus the
    /// original module's type/namespace-only exports.
    fn export_equals_module_member_exists(
        &mut self,
        module_symbol: SymbolId,
        target: SymbolId,
        name: &str,
    ) -> bool {
        let target_type = self.get_type_of_symbol(target);
        if self.get_property_of_type_ex(target_type, name, true).is_some() {
            return true;
        }
        if self.get_export_of_module(target, name).is_some() {
            return true;
        }
        let Some(&supplemental) = self.binder.symbols().get(module_symbol).exports.get(name) else {
            return false;
        };
        if matches!(name, "export=" | "__export") {
            return false;
        }
        let flags = self.get_symbol_flags(supplemental);
        flags.intersects(SymbolFlags::TYPE | SymbolFlags::NAMESPACE)
            && !flags.intersects(SymbolFlags::VALUE)
    }

    /// `reportInvalidImportEqualsExportMember` (`checker.go`): importing the
    /// `export =` target by name.
    fn report_invalid_import_equals_export_member(
        &mut self,
        file: NodeId,
        specifier: NodeId,
        span: tsr_core::Span,
        name: &str,
        module_name: &str,
    ) {
        let diagnostic = if self.module_kind >= tsr_core::ModuleKind::ES2015 {
            Diagnostic::with_args(
                &messages::_0_CAN_ONLY_BE_IMPORTED_BY_USING_A_DEFAULT_IMPORT,
                span,
                [name.to_string()],
            )
        } else if self.in_js_file(specifier) {
            Diagnostic::with_args(
                &messages::_0_CAN_ONLY_BE_IMPORTED_BY_USING_A_REQUIRE_CALL_OR_BY_USING_A_DEFAULT_IMPORT,
                span,
                [name.to_string()],
            )
        } else {
            Diagnostic::with_args(
                &messages::_0_CAN_ONLY_BE_IMPORTED_BY_USING_IMPORT_1_REQUIRE_2_OR_A_DEFAULT_IMPORT,
                span,
                [name.to_string(), name.to_string(), module_name.to_string()],
            )
        };
        self.report(file, diagnostic);
    }

    /// The identity `getSymbolIfSameReference` (`checker.go`) compares:
    /// `getMergedSymbol(resolveSymbol(getMergedSymbol(s)))`.
    fn same_reference_identity(&mut self, symbol: SymbolId) -> SymbolId {
        let merged = self.binder.merged_symbol(symbol);
        let resolved = self.resolve_alias_fully(merged);
        self.binder.merged_symbol(resolved)
    }

    /// TS2440 — `Import declaration conflicts with local declaration of '{0}'.`
    /// TS2441 — `Export declaration conflicts with exported declaration of '{0}'.`
    ///
    /// `checkAliasSymbol` (`checker.go:6736`). An alias's **local** symbol
    /// merges any other local declaration of the same name, so its flags are
    /// the union of every meaning that name already carries; the rule asks
    /// whether the imported target claims one of them.
    ///
    /// Upstream's `IsInJSFile` arm above this is unreachable rather than
    /// declined — §197 excludes `allowJs` cases from the diagnostics suite.
    pub(crate) fn check_alias_symbol(&mut self, node: NodeId) -> Option<()> {
        let declared = self.binder.symbol_of(node)?;
        let target = match self.resolve_alias(declared) {
            Some(target) => target,
            // [`Checker::resolve_alias`] refuses a **qualified** module
            // reference — *"A qualified name RESOLVES fine and prints wrong,
            // for want of symbol accessibility."* That refusal belongs to the
            // `.types` consumer: it keeps a resolved target from being printed
            // under a name this port cannot spell. This rule prints the *local*
            // symbol's name, which it already has, and reads the target only
            // for its flags — so the constraint does not reach it. §686.
            None => self.qualified_alias_target(declared)?,
        };
        // `symbol.ExportSymbol ?? symbol`, then merged: an exported alias has a
        // separate export symbol carrying the real flags (`declareModuleMember`).
        let local = self.binder.symbols().get(declared).export_symbol.unwrap_or(declared);
        let local = self.binder.merged_symbol(local);
        let flags = self.binder.symbols().get(local).flags;
        let target_flags = self.get_symbol_flags(target);
        let mut excluded = SymbolFlags::empty();
        if flags.intersects(SymbolFlags::VALUE | SymbolFlags::EXPORT_VALUE) {
            excluded |= SymbolFlags::VALUE;
        }
        if flags.intersects(SymbolFlags::TYPE) {
            excluded |= SymbolFlags::TYPE;
        }
        if flags.intersects(SymbolFlags::NAMESPACE) {
            excluded |= SymbolFlags::NAMESPACE;
        }
        if !target_flags.intersects(excluded) {
            return None;
        }
        let name = self.binder.symbols().get(local).name.to_string();
        let file = self.source_file_of_for_diagnostics(node)?;
        // `c.error(node, …)`, and upstream's `error` runs the node through
        // `getErrorSpanForNode`, which for a *named* declaration narrows to the
        // name. `import * as Lib from './f'` reports at `Lib` (column 13), not
        // at the `*` that starts the `NamespaceImport` (column 8) — see §232,
        // where reading the baseline overturned this line's first attribution.
        let span = self.error_span(node);
        let message = if self.nodes.kind(node) == SyntaxKind::ExportSpecifier {
            &messages::EXPORT_DECLARATION_CONFLICTS_WITH_EXPORTED_DECLARATION_OF_0
        } else {
            &messages::IMPORT_DECLARATION_CONFLICTS_WITH_LOCAL_DECLARATION_OF_0
        };
        self.report(file, Diagnostic::with_args(message, span, [name]));
        None
    }

    /// §130's establishment: the specifier's module resolves in-program to a
    /// plain module (no `export =` indirection), its exports table is
    /// non-empty (§186), carries NO `export *` (a star chain through an
    /// unresolvable target would make absence a guess), and the name is
    /// absent. Only then is the miss upstream's TS2305 and the alias's `any`
    /// deliberate.
    fn missing_import_export_established(&mut self, specifier: NodeId) -> bool {
        let Some(declaration) = self.import_or_export_declaration_of(specifier) else {
            return false;
        };
        let Some(module_specifier) = self.external_module_name(declaration) else { return false };
        let Some(module_symbol) = self.resolve_external_module_name(declaration, module_specifier)
        else {
            return false;
        };
        if self.resolve_external_module_symbol(module_symbol) != module_symbol {
            return false;
        }
        let name = match self.node_map.get(specifier) {
            Some(Node::ImportSpecifier(node)) => {
                node.property_name.or(node.name.map(tsr_ast::ModuleExportName::Identifier))
            }
            _ => None,
        };
        let Some(tsr_ast::ModuleExportName::Identifier(name)) = name else { return false };
        // `default` rides the interop machinery (synthetic defaults,
        // `canHaveSyntheticDefault`) — its absence from the table proves
        // nothing (allowSyntheticDefaultImports9's 7 G→W on the first pair).
        if name.text == "default" {
            return false;
        }
        if self.get_export_of_module(module_symbol, name.text).is_some() {
            return false;
        }
        let entry = self.binder.symbols().get(module_symbol);
        // A table carrying any non-identifier key (`export { x as "a-b" }`)
        // may spell the same export two ways — absence under one spelling
        // proves nothing (exportSpecifiers' 2 G→W, wants the literal `0`).
        !entry.exports.is_empty()
            && !entry.exports.contains_key(INTERNAL_EXPORT_STAR)
            && entry
                .exports
                .keys()
                .all(|key| key.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$'))
    }

    /// §131's establishment: the default import's module resolves in-program
    /// to a plain module whose non-empty, star-free, identifier-keyed
    /// exports lack `default`, and `can_have_synthetic_default` answers
    /// false (a TS module file with no `export =`).
    fn missing_default_established(&mut self, clause: NodeId) -> bool {
        let Some(import) = self.nodes.parent(clause) else { return false };
        let Some(module_specifier) = self.external_module_name(import) else { return false };
        let Some(module_symbol) = self.resolve_external_module_name(import, module_specifier)
        else {
            return false;
        };
        if self.resolve_external_module_symbol(module_symbol) != module_symbol {
            return false;
        }
        if self.can_have_synthetic_default(module_symbol) {
            return false;
        }
        // Node16/NodeNext resolve synthetic defaults by USAGE/TARGET module
        // format (`canHaveSyntheticDefault`'s head arms — ESM importing CJS
        // always has one), a mode road this port's predicate does not model
        // (nodeNextCjsNamespaceImportDefault1's 4 G→W on the first pair).
        if matches!(self.module_kind, tsr_core::ModuleKind::Node16 | tsr_core::ModuleKind::NodeNext)
        {
            return false;
        }
        let entry = self.binder.symbols().get(module_symbol);
        !entry.exports.is_empty()
            && !entry.exports.contains_key("default")
            && !entry.exports.contains_key(INTERNAL_EXPORT_STAR)
            && entry
                .exports
                .keys()
                .all(|key| key.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$'))
    }

    /// The `ImportDeclaration` or `ExportDeclaration` a specifier belongs to.
    pub(crate) fn import_or_export_declaration_of(&self, specifier: NodeId) -> Option<NodeId> {
        let mut current = specifier;
        while let Some(parent) = self.nodes.parent(current) {
            match self.nodes.kind(parent) {
                SyntaxKind::ImportDeclaration | SyntaxKind::ExportDeclaration => {
                    return Some(parent);
                }
                SyntaxKind::SourceFile => return None,
                _ => current = parent,
            }
        }
        None
    }

    /// `getFullyQualifiedName` of an external module symbol, which prints the
    /// specifier **as written, with its quotes** (`checker.go:14888`).
    fn quoted_module_name(&self, module_specifier: NodeId) -> String {
        match self.node_map.get(module_specifier) {
            Some(Node::StringLiteral(literal)) => format!("\"{}\"", literal.text),
            _ => String::new(),
        }
    }

    /// One export of a module, by name.
    ///
    /// Ported from `Checker.getExportOfModule` (`checker.go:14789`) with
    /// `dontResolveAlias = true`, under which its `resolveSymbolEx`
    /// (`checker.go:14432`) is the identity — so the whole function is the
    /// `SymbolFlagsModule` guard and one table lookup.
    ///
    /// The guard is not decoration: `getExternalModuleMember` can hand this a
    /// symbol that is not a module, and upstream answers `nil` rather than
    /// searching a table that means something else.
    pub(crate) fn get_export_of_module(
        &mut self,
        symbol: SymbolId,
        name: &str,
    ) -> Option<SymbolId> {
        let entry = self.binder.symbols().get(symbol);
        if !entry.flags.intersects(SymbolFlags::MODULE) {
            return None;
        }
        if let Some(found) = entry.exports.get(name).copied() {
            return Some(found);
        }
        // Not an own export — try the `export *` re-exports.
        self.get_export_from_star(symbol, name)
    }

    /// `resolveExportByName` (`checker.go:14615`) with `dontResolveAlias =
    /// true`: through the module's `export =` value's property when it has
    /// one, else its own export table (no `export *` walk, unlike
    /// [`Checker::get_export_of_module`]). Upstream's property read skips the
    /// global Object/Function augment; no name this is asked for can meet it.
    fn resolve_export_by_name(&mut self, module: SymbolId, name: &str) -> Option<SymbolId> {
        let export_equals = self.binder.symbols().get(module).exports.get("export=").copied();
        match export_equals {
            Some(export_equals) => {
                let value = self.get_type_of_symbol(export_equals);
                self.get_property_of_type(value, name)
            }
            None => self.binder.symbols().get(module).exports.get(name).copied(),
        }
    }

    /// The `module.exports` arm of `getTargetOfImportEqualsDeclaration`
    /// (`checker.go:14439`), for both `import x = require("m")` and the JS
    /// `const x = require("m")` declaration: under `module: node20`..`nodenext`
    /// the resolved module's `"module.exports"` export (`getExportOfModule`,
    /// `dontResolveAlias = true`) is the target. `resolved` is
    /// `resolveExternalModuleSymbol(immediate, dontResolveAlias = true)`.
    fn import_equals_module_exports(&mut self, resolved: SymbolId) -> Option<SymbolId> {
        if !self.module_kind_is_node20_through_nodenext() {
            return None;
        }
        self.get_export_of_module(resolved, INTERNAL_MODULE_EXPORTS)
    }

    /// `core.ModuleKindNode20 <= c.moduleKind && c.moduleKind <=
    /// core.ModuleKindNodeNext`, the gate every `module.exports` arm shares.
    fn module_kind_is_node20_through_nodenext(&self) -> bool {
        (tsr_core::ModuleKind::Node20..=tsr_core::ModuleKind::NodeNext).contains(&self.module_kind)
    }

    /// The condition `getTargetOfModuleDefault` (`checker.go:14536`) and
    /// `resolveESModuleSymbol` (`checker.go:15568`) put on their
    /// `module.exports` arms: the module is a file (`core.Find(Declarations,
    /// IsSourceFile)`), the module kind is `node20`..`nodenext`, the specifier
    /// is emitted as `CommonJS` syntax
    /// (`getEmitSyntaxForModuleSpecifierExpression`, `checker.go:14877`) and
    /// the file is emitted as an ES module (`GetImpliedNodeFormatForEmit`).
    /// Two host queries; nothing cached.
    fn esm_file_used_with_commonjs_syntax(&self, module: SymbolId, specifier: NodeId) -> bool {
        if !self.module_kind_is_node20_through_nodenext() {
            return false;
        }
        let Some(host) = self.module_host else { return false };
        let Some(&file) = self
            .binder
            .symbols()
            .get(module)
            .declarations
            .iter()
            .find(|&&declaration| self.nodes.kind(declaration) == SyntaxKind::SourceFile)
        else {
            return false;
        };
        // getEmitSyntaxForModuleSpecifierExpression: string literals only.
        if !matches!(
            self.nodes.kind(specifier),
            SyntaxKind::StringLiteral | SyntaxKind::NoSubstitutionTemplateLiteral
        ) {
            return false;
        }
        let Some(importing) = self.source_file_of(specifier) else { return false };
        host.emit_syntax_for_usage_location(importing, specifier) == tsr_core::ModuleKind::CommonJS
            && host.implied_node_format_for_emit(file) == tsr_core::ModuleKind::ESNext
    }

    /// The `module.exports` arm of `resolveESModuleSymbol`
    /// (`checker.go:15568`) for a namespace import `import * as ns from "m"`
    /// (`owner` is the `ImportDeclaration`, `specifier` its module
    /// specifier): the resolved module's `"module.exports"` export
    /// (`getExportOfModule`, `dontResolveAlias = true`).
    ///
    /// The earlier `getTypeWithSyntheticDefaultOnly` return needs an ES-syntax
    /// specifier (`isOnlyImportableAsDefault`), so it never precedes this
    /// `CommonJS`-syntax arm. Where the module symbol's type has signatures,
    /// upstream answers `cloneTypeAsModuleType(moduleExports, typ)` — a copy
    /// of the *module's* type; that declines here and the module-object road
    /// ([`Checker::module_object_of`], then `module_clone_type`) copies the
    /// same type under the module symbol.
    fn namespace_import_module_exports(
        &mut self,
        owner: NodeId,
        specifier: NodeId,
    ) -> Option<SymbolId> {
        if !self.module_kind_is_node20_through_nodenext()
            || !matches!(self.node_map.get(owner), Some(Node::ImportDeclaration(_)))
        {
            return None;
        }
        let module = self.resolve_external_module_name(owner, specifier)?;
        if !self.esm_file_used_with_commonjs_syntax(module, specifier) {
            return None;
        }
        let mut symbol = self.resolve_external_module_symbol(module);
        if self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::ALIAS) {
            // resolveIndirectionAlias through a pure-alias `export =`.
            symbol = self.binder.merged_symbol(self.resolve_alias_fully(symbol));
        }
        let module_exports = self.get_export_of_module(symbol, INTERNAL_MODULE_EXPORTS)?;
        // hasSignatures(getTypeOfSymbol(symbol)).
        let value = self.get_type_of_symbol(symbol);
        for kind in
            [crate::signatures::SignatureKind::Call, crate::signatures::SignatureKind::Construct]
        {
            if self
                .signatures_of_type_kind(value, kind)
                .is_some_and(|signatures| !signatures.is_empty())
            {
                return None;
            }
        }
        Some(module_exports)
    }

    /// One name, looked up through a module's `export *` declarations
    /// (`getExportsOfModuleWorker`, `checker.go:16148`).
    ///
    /// # Why this resolves one name rather than building the whole table
    ///
    /// Upstream materialises a module's complete export table and caches it on
    /// the symbol. That is the right shape and it is not what this does: the
    /// symbol store here is the binder's and is not the checker's to extend
    /// (ADR-0034 — a program has one identity space, and the checker borrows
    /// it), so a merged table would need a side table keyed by symbol with its
    /// own invalidation. Resolving per name needs neither and answers the same
    /// question; the cost is repeated walks for a module queried many times,
    /// which is bounded by the star depth and measured at nothing on the corpus.
    ///
    /// # What is faithful and what is not
    ///
    /// Faithful: the recursive walk, and the **visited set** — the ES6 spec
    /// permits `a` to `export *` from `b` while `b` exports `*` from `a`, and
    /// without the guard that is a hang rather than a wrong answer.
    ///
    /// Not ported: `extendExportSymbols`' collision table and the
    /// `Module_0_has_already_exported_a_member_named_1` diagnostic it raises for
    /// a name two stars both provide. Upstream reports *and* leaves the name
    /// unresolved; this takes the first star that provides it. A wrong answer in
    /// a case that is already an error, rather than a missing answer in every
    /// case that is not.
    ///
    /// `export type *`'s type-onlyness, which upstream tracks in a parallel
    /// map, is answered separately by
    /// [`Checker::specifier_type_only_export_star`].
    pub(crate) fn get_export_from_star(
        &mut self,
        module: SymbolId,
        name: &str,
    ) -> Option<SymbolId> {
        let mut visited = Vec::new();
        self.get_export_from_star_worker(module, name, &mut visited)
    }

    fn get_export_from_star_worker(
        &mut self,
        module: SymbolId,
        name: &str,
        visited: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        if visited.contains(&module) {
            return None;
        }
        visited.push(module);

        let stars: Vec<NodeId> = {
            let entry = self.binder.symbols().get(module);
            let star = entry.exports.get(INTERNAL_EXPORT_STAR).copied()?;
            self.binder.symbols().get(star).declarations.to_vec()
        };

        for declaration in stars {
            let specifier = match self.node_map.get(declaration) {
                Some(Node::ExportDeclaration(node)) => match node.module_specifier {
                    Some(expression) => match Node::from(expression).node_id() {
                        Some(id) => id,
                        None => continue,
                    },
                    None => continue,
                },
                _ => continue,
            };
            let Some(target) = self.resolve_external_module_name(declaration, specifier) else {
                continue;
            };
            // `export =` in a re-exported module: upstream resolves through it
            // before reading the exports.
            let target = self.resolve_external_module_symbol(target);
            if let Some(found) = self.binder.symbols().get(target).exports.get(name).copied() {
                return Some(found);
            }
            if let Some(found) = self.get_export_from_star_worker(target, name, visited) {
                return Some(found);
            }
        }
        None
    }

    /// The `export type *` declaration that makes a named import or
    /// re-export type-only: `typeOnlyExportStarMap[name]` of
    /// `getExportsOfModuleWorker` (`checker.go:16148`), which
    /// `getExportOfModule` (`checker.go:14793`) hands to
    /// `markSymbolOfAliasDeclarationIfTypeOnly` for the specifier.
    ///
    /// `declaration` is an `ImportSpecifier`, or an `ExportSpecifier` of a
    /// declaration with a module specifier. No cache: asked only when a name
    /// is not an own export of a module that has `export *` declarations, and
    /// the walk is bounded by the star graph.
    pub(crate) fn specifier_type_only_export_star(
        &mut self,
        declaration: NodeId,
    ) -> Option<NodeId> {
        let name = match self.node_map.get(declaration)? {
            Node::ImportSpecifier(specifier) => specifier
                .property_name
                .or(specifier.name.map(tsr_ast::ModuleExportName::Identifier))?,
            Node::ExportSpecifier(specifier) => specifier.property_name.or(specifier.name)?,
            _ => return None,
        };
        let name = match name {
            tsr_ast::ModuleExportName::Identifier(identifier) => identifier.text,
            tsr_ast::ModuleExportName::StringLiteral(literal) => literal.text,
        };
        let owner = self.import_or_export_declaration_of(declaration)?;
        let module_specifier = match self.node_map.get(owner)? {
            Node::ImportDeclaration(import) => import.module_specifier,
            Node::ExportDeclaration(export) => export.module_specifier,
            _ => None,
        }?
        .node_id()?;
        let module = self.resolve_external_module_name(owner, module_specifier)?;
        let module = self.resolve_external_module_symbol(module);
        // A name the module exports itself is in `nonTypeOnlyNames`, and a
        // module without `export *` records nothing.
        let entry = self.binder.symbols().get(module);
        if entry.exports.contains_key(name) || !entry.exports.contains_key(INTERNAL_EXPORT_STAR) {
            return None;
        }
        let mut walk = TypeOnlyStarWalk::default();
        self.visit_type_only_export_star(Some(module), None, false, name, &mut walk);
        if walk.non_type_only { None } else { walk.type_only_star }
    }

    /// `visit` inside `getExportsOfModuleWorker` (`checker.go:16154`),
    /// projected onto one name: answers whether the name is in the visited
    /// module's export table, and records the type-only star that would be
    /// written last into `typeOnlyExportStarMap[name]`. Every star is visited,
    /// as upstream does, because the shared visited set makes later answers
    /// depend on earlier walks.
    fn visit_type_only_export_star(
        &mut self,
        module: Option<SymbolId>,
        export_star: Option<NodeId>,
        is_type_only: bool,
        name: &str,
        walk: &mut TypeOnlyStarWalk,
    ) -> bool {
        let Some(module) = module else { return false };
        let (own, stars) = {
            let entry = self.binder.symbols().get(module);
            let own = entry.exports.contains_key(name);
            let stars = entry
                .exports
                .get(INTERNAL_EXPORT_STAR)
                .map(|&star| self.binder.symbols().get(star).declarations.to_vec());
            (own, stars)
        };
        if !is_type_only && own {
            walk.non_type_only = true;
        }
        if walk.visited.contains(&module) {
            return false;
        }
        walk.visited.push(module);
        let mut contains = own;
        for declaration in stars.unwrap_or_default() {
            let Some(Node::ExportDeclaration(export)) = self.node_map.get(declaration) else {
                continue;
            };
            let star_is_type_only = export.is_type_only;
            let target = export
                .module_specifier
                .and_then(|specifier| specifier.node_id())
                .and_then(|specifier| self.resolve_external_module_name(declaration, specifier));
            let nested = self.visit_type_only_export_star(
                target,
                Some(declaration),
                is_type_only || star_is_type_only,
                name,
                walk,
            );
            // `extendExportSymbols` never re-exports `default` through a star.
            contains |= nested && name != "default";
        }
        if contains
            && let Some(star) = export_star
            && matches!(self.node_map.get(star), Some(Node::ExportDeclaration(export)) if export.is_type_only)
        {
            walk.type_only_star = Some(star);
        }
        contains
    }

    /// §501: `resolve_alias` iterated to a fixpoint — the terminal non-alias
    /// symbol a chain of aliases reaches, or the last resolvable link. The
    /// cap is the same policy as every bounded walk here; alias chains in the
    /// corpus are ≤3 deep.
    pub(crate) fn resolve_alias_fully(&mut self, symbol: SymbolId) -> SymbolId {
        let mut current = symbol;
        for _ in 0..8 {
            if !self.binder.symbols().get(current).flags.intersects(SymbolFlags::ALIAS) {
                return current;
            }
            match self.resolve_alias(current) {
                Some(next) if next != current => current = next,
                _ => return current,
            }
        }
        current
    }

    /// Ported from `Checker.resolveExternalModuleSymbol`
    /// (`checker.go:15556`): a module that writes `export = X` is, for every
    /// purpose downstream, `X`.
    ///
    /// Returns the module symbol unchanged when there is no `export =`, which is
    /// upstream's fallthrough. The caller uses the *difference* as the test for
    /// whether an `export =` is present, so this is one function rather than a
    /// bool and a symbol.
    ///
    /// `resolveSymbolEx(…, dontResolveAlias)` is applied by upstream to the
    /// `export=` symbol; with `dontResolveAlias = true` it is the identity, and
    /// the only caller here passes `true`.
    pub(crate) fn resolve_external_module_symbol(&self, module_symbol: SymbolId) -> SymbolId {
        // `ast.InternalSymbolNameExportEquals` (`internal/ast/symbol.go:66`).
        // Spelled here rather than imported because the binder's constant is
        // `pub(crate)`; both are anchored to the same upstream line, and a test
        // in `tests/cross_file_aliases.rs` pins the spelling by construction.
        const EXPORT_EQUALS: &str = "export=";
        self.binder
            .symbols()
            .get(module_symbol)
            .exports
            .get(EXPORT_EQUALS)
            .copied()
            .unwrap_or(module_symbol)
    }

    /// The module symbol a specifier names — the one step a cross-file alias
    /// cannot take alone.
    ///
    /// Ported from `Checker.resolveExternalModuleName` (`checker.go:15101`)
    /// through `resolveExternalModuleNameWorker` (`checker.go:15122`) into
    /// `resolveExternalModule` (`checker.go:15149`), reduced to the path that
    /// produces a symbol. Upstream's function is 190 lines, of which this ports
    /// four steps:
    ///
    /// 1. the specifier must be a string literal
    ///    (`IsStringLiteralLike`, `checker.go:15123`);
    /// 2. the host maps `(importing file, text)` to a file
    ///    ([`crate::resolution::ModuleHost`]);
    /// 3. the file's own symbol **is** the module symbol
    ///    (`crates/tsr-binder/src/binder.rs`,
    ///    `bind_source_file_as_external_module`), which is upstream's
    ///    `sourceFile.Symbol` at `checker.go:15321`;
    /// 4. `nil` when the file has no symbol — upstream's
    ///    `File_0_is_not_a_module`.
    ///
    /// Everything else in those 190 lines is diagnostics: fourteen distinct
    /// messages about `.ts` extensions, rewritten relative imports and a
    /// `CommonJS` file reaching an ES module. None of it changes which symbol is
    /// returned.
    ///
    /// # Two resolutions that are not ported, both misses
    ///
    /// - **Pattern ambient modules** (`declare module "foo/*"`), upstream's
    ///   fallback after the host misses.
    /// - **`getMergedSymbol`** on the result: a module symbol that merges with a
    ///   module augmentation is answered unmerged.
    ///
    /// Each is a `None`, so each is an `errorType` — a gap, never a wrong
    /// target. `tryFindAmbientModule` was the third entry on this list until
    /// the seventh session; it is the arm below, and
    /// `docs/architecture/checker-notes-modobj.md` §10 carries its sizing.
    ///
    /// # With no host, this answers `None` and the behaviour is today's
    ///
    /// [`Checker::module_host`] is `None` for a checker built over a single
    /// bound file, which is every call site that existed before this arm. Such a
    /// checker answers `errorType` for every cross-file alias, which is exactly
    /// what it answered before — the arm is additive by construction rather than
    /// by test.
    pub(crate) fn resolve_external_module_name(
        &mut self,
        location: NodeId,
        module_specifier: NodeId,
    ) -> Option<SymbolId> {
        let text = match self.node_map.get(module_specifier)? {
            Node::StringLiteral(literal) => literal.text,
            Node::NoSubstitutionTemplateLiteral(literal) => literal.text,
            // resolveExternalModuleNameWorker accepts StringLiteralLike.
            _ => return None,
        };
        // `tryFindAmbientModule` (`checker.go:15533`), consulted **before** the
        // host exactly as `resolveExternalModule` (`checker.go:15154`) does: a
        // non-relative specifier may name a `declare module "x"`.
        if let Some(ambient) = self.ambient_module(text) {
            return Some(ambient);
        }
        let importing_file = self.source_file_of(location)?;
        let host = self.module_host?;
        let mode = self.module_resolution_mode(host, importing_file, location);
        let target = host.resolved_module_in_mode(importing_file, text, mode)?;
        // `sourceFile.Symbol != nil` (`checker.go:15321`). `None` here is a file
        // that is not an external module — upstream's `File_0_is_not_a_module` —
        // and it is the reason the host answers a *file* rather than a symbol:
        // resolving to a plain script is a successful resolution with no module
        // symbol at the end of it, and only the checker can tell those apart.
        self.binder.symbol_of(target)
    }

    /// The `contextSpecifier`/`mode` prelude of `resolveExternalModule`
    /// (`checker.go:15149`): the string literal whose usage decides the
    /// resolution mode, read from `location`, then
    /// `GetModeForUsageLocation(file, contextSpecifier)` — or the file's
    /// default mode when no specifier is found. No cache: two parent walks
    /// and a host query per resolution.
    pub(crate) fn module_resolution_mode(
        &self,
        host: &dyn crate::resolution::ModuleHost,
        importing_file: NodeId,
        location: NodeId,
    ) -> tsr_core::ResolutionMode {
        match self.context_specifier(location) {
            Some(specifier)
                if matches!(
                    self.nodes.kind(specifier),
                    SyntaxKind::StringLiteral | SyntaxKind::NoSubstitutionTemplateLiteral
                ) =>
            {
                host.mode_for_usage_location(importing_file, specifier)
            }
            _ => host.default_resolution_mode_for_file(importing_file),
        }
    }

    /// `contextSpecifier` in `resolveExternalModule` (`checker.go:15166`).
    fn context_specifier(&self, location: NodeId) -> Option<NodeId> {
        let is_string_literal_like = |node: NodeId| {
            matches!(
                self.nodes.kind(node),
                SyntaxKind::StringLiteral | SyntaxKind::NoSubstitutionTemplateLiteral
            )
        };
        let parent = self.nodes.parent(location);
        if is_string_literal_like(location)
            || parent.is_some_and(|parent| {
                matches!(self.node_map.get(parent), Some(Node::ModuleDeclaration(module))
                    if module.name.and_then(|name| name.node_id()) == Some(location))
            })
        {
            return Some(location);
        }
        match self.node_map.get(location)? {
            Node::ModuleDeclaration(module) => return module.name.and_then(|name| name.node_id()),
            // `IsLiteralImportTypeNode`.
            Node::ImportTypeNode(import_type) => {
                if let Some(tsr_ast::TypeNode::LiteralTypeNode(literal_type)) = import_type.argument
                    && let Some(literal) = literal_type.literal
                    && let Some(id) = literal.node_id()
                    && is_string_literal_like(id)
                {
                    return Some(id);
                }
            }
            // `IsVariableDeclarationInitializedToBareOrAccessedRequire`.
            Node::VariableDeclaration(variable) => {
                let mut initializer = variable.initializer;
                while let Some(
                    Expression::PropertyAccessExpression(tsr_ast::PropertyAccessExpression {
                        expression,
                        ..
                    })
                    | Expression::ElementAccessExpression(tsr_ast::ElementAccessExpression {
                        expression,
                        ..
                    }),
                ) = initializer
                {
                    initializer = *expression;
                }
                if let Some(Expression::CallExpression(call)) = initializer
                    && let Some(Expression::Identifier(callee)) = call.expression
                    && callee.text == "require"
                    && let [argument] = call.arguments
                    && let Some(id) = argument.node_id()
                    && is_string_literal_like(id)
                {
                    return Some(id);
                }
            }
            _ => {}
        }
        // `FindAncestor` starts at `location` itself.
        let ancestors = || std::iter::once(location).chain(self.nodes.ancestors(location));
        // `FindAncestor(location, IsImportCall)`.
        if let Some(call) = ancestors().find_map(|ancestor| match self.node_map.get(ancestor) {
            Some(Node::CallExpression(call))
                if matches!(call.expression, Some(Expression::KeywordExpression(keyword))
                    if keyword.node_id.is_some_and(|id| self.nodes.kind(id) == SyntaxKind::ImportKeyword)) =>
            {
                Some(call)
            }
            _ => None,
        }) {
            return call.arguments.first().and_then(tsr_ast::Expression::node_id);
        }
        // `IsImportDeclarationOrJSImportDeclaration`: a JSDoc `@import` is
        // upstream's reparsed `JSImportDeclaration`.
        if let Some(specifier) =
            ancestors().find_map(|ancestor| match self.node_map.get(ancestor) {
                Some(Node::ImportDeclaration(import)) => Some(import.module_specifier),
                Some(Node::JSDocImportTag(import)) => Some(import.module_specifier),
                _ => None,
            })
        {
            return specifier.and_then(|specifier| specifier.node_id());
        }
        if let Some(export) = ancestors().find_map(|ancestor| match self.node_map.get(ancestor) {
            Some(Node::ExportDeclaration(export)) => Some(export),
            _ => None,
        }) {
            return export.module_specifier.and_then(|specifier| specifier.node_id());
        }
        ancestors()
            .find_map(|ancestor| match self.node_map.get(ancestor) {
                Some(Node::ImportEqualsDeclaration(import)) => Some(import.module_reference),
                _ => None,
            })?
            .and_then(|reference| match reference {
                tsr_ast::ModuleReference::ExternalModuleReference(reference) => {
                    reference.expression.and_then(|expression| expression.node_id())
                }
                _ => None,
            })
    }

    /// `tryFindAmbientModule` (`checker.go:15533`): the `declare module "x"`
    /// a non-relative specifier names, or `None`.
    ///
    /// Upstream: `IsExternalModuleNameRelative` short-circuits, then
    /// `c.getSymbol(c.globals, "\""+moduleName+"\"", ast.SymbolFlagsValueModule)`
    /// with `getMergedSymbol` on the hit.
    ///
    /// **The quoted key is upstream's, and this now uses it.** It read the
    /// unquoted specifier and recovered the selection the quotes give from the
    /// declaration's *shape* instead. That was not merely a different spelling
    /// of the same lookup: without the quotes, `declare module "process"` and a
    /// global `var process` are one key in one table, which is
    /// `docs/architecture/checker-notes-diag2.md` §202. The shape test is kept
    /// below as a second gate rather than the only one — an unquoted name can
    /// no longer reach here, so it now only excludes a quoted symbol that is
    /// somehow not a module declaration.
    fn ambient_module(&self, name: &str) -> Option<SymbolId> {
        // `IsExternalModuleNameRelative` short-circuits first, exactly as
        // upstream does — and through `tsr_path`, which is the port of
        // `tspath`, rather than a local prefix test that would miss `.\`,
        // `..\` and rooted disk paths.
        if tsr_path::is_external_module_name_relative(name) {
            return None;
        }
        let symbol = self.binder.ambient_module(name)?;
        (self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::VALUE_MODULE)
            && self.is_ambient_module(symbol))
        .then_some(symbol)
    }

    /// The **module object** a specifier names: the module symbol itself, and
    /// only when the module does not write `export =`.
    ///
    /// The `export =` case answers `None` on purpose rather than handing back
    /// `resolve_external_module_symbol`'s target. That target has a declared
    /// name of its own and would print through the baked text, so it is a
    /// *different* population from the one measured in §14 — 218 lines the probe
    /// deliberately excluded — and shipping it here would be adding unmeasured
    /// surface to a slice whose whole argument is that the surface was measured.
    /// `bd tsr-e2u` carries it.
    ///
    /// # §219: the refusal was tested, and it holds — but the price is now known
    ///
    /// "Unmeasured surface" is a reason that expires the moment someone
    /// measures, so this was built and measured rather than re-argued. Removing
    /// the guard — `Some(self.resolve_external_module_symbol(module))`, which is
    /// upstream's own first line in `resolveESModuleSymbol` (`checker.go:15569`,
    /// `dontResolveAlias=true`) — is worth **+4 cases, −0, +40 lines**, every
    /// movement positive: `es6ExportAssignment2`/`4`,
    /// `es6ImportEqualsExportModuleCommonJsError`/`Es2015Error`.
    ///
    /// It is nonetheless still refused, because the doc above was right about
    /// the mechanism and the per-case tally cannot see it. Following `export =`
    /// hands the printer a `declare namespace __X` instead of a module symbol,
    /// and [`Checker::type_to_string_at`]'s interception is gated on
    /// `is_module_symbol || is_ambient_module` — so the line comes out
    /// `typeof __React` where every tsx baseline records `typeof React`, and
    /// the **two-alias gap is bypassed entirely**, which is the worse half:
    /// that gap exists because the corpus contradicts every tie-break, and
    /// walking past it turns a gap into a confident wrong name. `casedelta`
    /// counts matched lines, so a gap that becomes wrong-but-different is
    /// invisible to it — which is exactly why +4/−0 is not sufficient here.
    ///
    /// Widening that gate is the actual fix and is **not** a one-liner: adding
    /// `VALUE_MODULE | NAMESPACE_MODULE` makes the interception fire and then
    /// `module_name_at` declines, so the line becomes `error` instead, and it
    /// regresses a passing case that wants `typeof N`.
    ///
    /// **§602 corrects the parenthetical that stood here.** It read
    /// *"`NAMESPACE_MODULE` alone fires on nothing — a namespace containing a
    /// class is INSTANTIATED, so the binder stamps `VALUE_MODULE`"*. The
    /// reasoning is sound for a *namespace*, and the conclusion does not
    /// follow, because namespaces are not the only modules: measured at the
    /// §601 baseline, **46 lines across 32 cases** reach
    /// [`Checker::get_type_of_symbol`] with a `NAMESPACE_MODULE`-only symbol
    /// and answer `any` — ambient declarations and `declare global`
    /// augmentations, which are non-instantiated by construction
    /// (`ambientErrors`, `parserModuleDeclaration1`,
    /// `moduleAugmentationGlobal6_1`, `ambientExternalModuleInsideNonAmbient`,
    /// …). The count is *after* §601 moved `ConstEnumOnly` into
    /// `VALUE_MODULE`, so it is a floor rather than an artefact.
    ///
    /// What this does NOT establish is that widening the interception gate
    /// would convert them — the note's own measurement (the line becomes
    /// `error` because `module_name_at` declines) still stands and is a
    /// separate test. The correction is to the *reason*, which read as evidence
    /// that the population was empty when it is 32 cases. The two changes are
    /// one piece of work and `bd tsr-e2u` should carry both halves.
    ///
    /// **How you would know this is wrong:** if `module_name_at` learns to name
    /// an `export =` target through the importing alias, then this guard is
    /// pure loss and the +4 is free. Test it with
    /// `import * as X from 'm'` over `declare module 'm' { export = __X }`
    /// expecting `typeof X`.
    ///
    /// # What the two halves are each worth, measured rather than guessed
    ///
    /// The `tsx` corpus resolves `react` through
    /// `declare module "react" { export = __React }` (`tests/lib/react.d.ts:2354`),
    /// so the whole `typeof React` census row runs through this function. Seven
    /// of its members sit at deficit 1 — `conformance/multiline`,
    /// `correctlyMarkAliasAsReferences1`–`4`, `controlFlowOptionalChain3`,
    /// `tsxElementResolution19` — each blocked on nothing but
    /// `>React : typeof React`.
    ///
    /// **All seven are byte-identical across the experiment above.** Following
    /// `export =` moved their tallies not at all, which is the direct evidence
    /// that the resolution half and the naming half are not two independent
    /// wins to be taken in either order:
    ///
    /// - resolution alone: **+4** (the `es6ExportAssignment` family, whose
    ///   answers do not name the module object);
    /// - naming alone: **+0**, and unreachable — there is no target to name
    ///   until the guard above comes off;
    /// - both: the +4, plus up to **7** more from this row.
    ///
    /// So `bd tsr-e2u` is worth ~11 cases and cannot be half-taken. Anyone
    /// sizing it from the +4 alone will under-price it by roughly a factor of
    /// three, and anyone taking the +4 alone will ship the wrong names.
    ///
    /// # §232: the +4 was NOT part of `tsr-e2u`, and taking it cost nothing
    ///
    /// The paragraph above is right about the React row and **wrong about the
    /// +4**. Those four cases (`es6ExportAssignment2`/`4`,
    /// `es6ImportEqualsExportModule*`) resolve `export =` to a **plain value**
    /// — `export = a` over `var a = 10`, answering `number`. No name is
    /// printed, so §219's reason has nothing to object to, and they were never
    /// waiting on the naming half at all. Splitting the guard on the resolved
    /// target's module flags takes them: **+4 cases, 0 lost, and at line level
    /// 18 wrong→right against 0 right→wrong**.
    ///
    /// `bd tsr-e2u` is therefore worth ~7, not ~11 — the React row alone,
    /// which does still need both halves together. The over-count came from
    /// measuring the *wide* refusal's removal and attributing all of it to the
    /// one cause the refusal named.
    fn module_object_of(&mut self, location: NodeId, specifier: NodeId) -> Option<SymbolId> {
        let module = self.resolve_external_module_name(location, specifier)?;
        let target = self.resolve_external_module_symbol(module);
        if target == module {
            return Some(module);
        }
        // §232: the refusal above was WIDER THAN ITS REASON, which is
        // corollary 16's question asked of a refusal that is otherwise right.
        // The reason is about naming a **module object** — following `export =`
        // hands the printer a `declare namespace __React` and it prints
        // `typeof __React`. That hazard exists only when the target *is* a
        // module object. `export = a` over `var a = 10` resolves to a plain
        // variable whose answer is `number`: no name to get wrong, so nothing
        // for the reason to object to.
        //
        // So the gate is the flags, not the whole construct. Where the target
        // carries a module flag the §219 refusal stands unchanged and
        // `bd tsr-e2u`'s two halves still have to land together; everywhere
        // else there was never a naming question. Found by `checker-2`
        // re-reading my refusal against its own witnesses
        // (`es6ExportAssignment2`, `es6ImportEqualsDeclaration2`).
        // **The flags belong to the alias's TARGET, not to the alias.**
        // `resolve_external_module_symbol` is `dontResolveAlias=true`, so
        // `target` is the `export=` symbol itself and carries `ALIAS`, never a
        // module flag — testing it directly let `declare module 'm' { export =
        // __X }` through and printed `typeof __X`, the exact line §219 refused
        // for. The §219 refusal pin caught it on the first run, which is what
        // that pin was written for.
        //
        // Only the flag test follows the alias; the symbol handed back is still
        // the unresolved one, which is upstream's `dontResolveAlias` contract.
        //
        // §501 — `bd tsr-e2u`'s resolution half, landed TOGETHER with the
        // naming half this time (the §219 refusal's own stated reopening
        // condition: "if `module_name_at` learns to name an `export =` target
        // through the importing alias, then this guard is pure loss").
        // `module_alias_at` now matches candidates by their FULLY-resolved
        // target, so `import * as React from "react"` over
        // `declare module "react" { export = __React }` names the module
        // object `typeof React` — the exact test the refusal prescribed.
        Some(target)
    }

    /// The module specifier of an `ImportDeclaration` or an `ExportDeclaration`.
    ///
    /// Ported from `ast.GetExternalModuleName` (`internal/ast/utilities.go:1903`)
    /// over the two kinds [`Checker::get_external_module_member`] is reached
    /// with. Upstream also covers `ImportEqualsDeclaration`, `ModuleDeclaration`
    /// and `ImportTypeNode`; none of those reaches this function in this port,
    /// and listing a kind whose caller does not exist would be an intention
    /// documented as though it were built.
    pub(crate) fn external_module_name(&self, node: NodeId) -> Option<NodeId> {
        let specifier = match self.node_map.get(node)? {
            Node::ImportDeclaration(node) => node.module_specifier,
            Node::ExportDeclaration(node) => node.module_specifier,
            // §269: a JSDoc `@import` tag carries the specifier itself —
            // upstream never meets this because the reparser has already
            // rewritten the tag as a `JSImportDeclaration`; this port binds
            // the tag directly, so the tag IS the declaration here.
            Node::JSDocImportTag(node) => node.module_specifier,
            _ => return None,
        }?;
        specifier.node_id()
    }

    /// The `SourceFile` a node belongs to (`ast.GetSourceFileOfNode`).
    ///
    /// A parent walk, because [ADR-0003](../../../docs/adr/0003-tree-plus-side-tables.md)
    /// keeps the back-edge in [`tsr_ast::NodeTable`] rather than on the node.
    /// Under [ADR-0034](../../../docs/adr/0034-a-program-needs-one-identity-space.md)
    /// one table spans every file of a program, so the id this returns
    /// identifies a file across the whole program and is what
    /// [`crate::resolution::ModuleHost`] is keyed on.
    fn source_file_of(&self, node: NodeId) -> Option<NodeId> {
        let mut current = node;
        loop {
            if self.nodes.kind(current) == SyntaxKind::SourceFile {
                return Some(current);
            }
            match self.nodes.parent(current) {
                Some(parent) => current = parent,
                // §269: a walk that dead-ends inside a JSDoc comment crosses
                // to the comment's host — the one edge the tree deliberately
                // omits (see the parser's `attach_jsdoc`). An `@import` tag's
                // module specifier resolving against its file is the client.
                None => current = *self.jsdoc_hosts.get(&current)?,
            }
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
    pub(crate) fn get_declared_type_of_enum_member(&mut self, symbol: SymbolId) -> TypeId {
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
    /// Native 5b1047d1 publishes the callable object independently of its lazy
    /// return slots. Reserve the same declaration-owned identity before asking
    /// for signatures: returning the function is not circular return inference.
    /// The existing worker still prepares metadata/text eagerly; only its
    /// successful completion publishes `signature_types` on the reserved identity.
    /// Symbol-owned reuse and options have the private Checker lifetime. The
    /// return-body work/circularity boundary lives in `return_type_of` instead.
    fn get_type_of_func_class_enum_module(&mut self, symbol: SymbolId) -> TypeId {
        if let Some(&cached) = self.symbol_types.get(&symbol) {
            return cached;
        }
        let flags = self.binder.symbols().get(symbol).flags;
        if flags.intersects(SymbolFlags::FUNCTION | SymbolFlags::METHOD)
            && !flags.intersects(SymbolFlags::CLASS | SymbolFlags::ENUM | SymbolFlags::MODULE)
        {
            let reserved = self.store.new_anonymous(TypeFlags::OBJECT, "any".into(), symbol, true);
            self.symbol_types.insert(symbol, reserved);
            let computed = self.get_type_of_func_class_enum_module_worker(symbol, Some(reserved));
            self.symbol_types.insert(symbol, computed);
            return computed;
        }
        if !self.resolutions.push(symbol, PropertyName::Type) {
            return self.intrinsics.error;
        }
        let computed = self.get_type_of_func_class_enum_module_worker(symbol, None);
        let computed = if self.resolutions.pop() { computed } else { self.intrinsics.error };
        self.symbol_types.insert(symbol, computed);
        computed
    }

    /// Completed declaration metadata only. Native typeof admission, assignment
    /// naming and value accessibility are decisions of the site renderer.
    pub(crate) fn completed_callable_symbol(&self, ty: TypeId) -> Option<SymbolId> {
        self.signature_types.get(&ty)?;
        let crate::types::TypeData::Anonymous { symbol, .. } = self.store.get(ty).data else {
            return None;
        };
        self.binder
            .symbols()
            .get(symbol)
            .flags
            .intersects(SymbolFlags::FUNCTION | SymbolFlags::METHOD)
            .then_some(symbol)
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
    /// - **`strictNullChecks` and an optional symbol** (`checker.go:16930`) **is
    ///   now ported — §885**, at this function's tail. This bullet carried the
    ///   refusal for many sessions with the note *"only the excuse expired"*; the
    ///   arm it described is written, and the limitation is retired rather than
    ///   deleted so the list still reads as a history.
    fn get_type_of_func_class_enum_module_worker(
        &mut self,
        symbol: SymbolId,
        reserved: Option<TypeId>,
    ) -> TypeId {
        let flags = self.binder.symbols().get(symbol).flags;
        // `isShorthandAmbientModuleSymbol` (`utilities.go:198`): `declare module
        // "x";` with no body has the type `any`. A computed answer, so `anyType`.
        if flags.intersects(SymbolFlags::MODULE) && self.is_shorthand_ambient_module(symbol) {
            return self.intrinsics.any;
        }
        // §305: `getNameOfSymbolAsWritten`'s nameless fallbacks
        // (`nodebuilderimpl.go:1005`) — a nameless CLASS EXPRESSION spells as
        // the variable it initializes (`var V = class {}` prints `typeof V`)
        // or, with no such parent, as the literal `(Anonymous class)`. This is
        // the naming §168 refused to guess at; upstream's rule turned out to
        // be a two-line declaration walk, not per-site context. Gated to class
        // expressions: a nameless default-export CLASS DECLARATION spells
        // `default` through a different leg of the same function, unported.
        if self.has_a_name_no_type_query_can_spell(symbol) {
            if flags.intersects(SymbolFlags::CLASS)
                && let Some(written) = self.anonymous_class_written_name(symbol)
            {
                let printed = format!("typeof {written}");
                return self.store.new_anonymous(TypeFlags::OBJECT, printed, symbol, false);
            }
            return self.intrinsics.error;
        }
        let name = self.binder.symbols().get(symbol).name;
        // `shouldEmitTypeOfSymbol` tests enum and value module *after* class but
        // as an `||`, so a merged `function f() {} namespace f {}` symbol takes
        // the `typeof` form. Ordering the class test first therefore changes
        // nothing; what matters is that the function case comes last.
        //
        // §507: the flags are read off the MERGED symbol too — a CROSS-FILE
        // merge (`function Point()` in one file, `module Point` in another,
        // `ModuleAndFunctionWithSameNameAndCommonRoot`) leaves the function
        // half's own symbol without the module flag, and upstream's
        // `getMergedSymbol` sees the union. Same-file merges already carried
        // both flags on one symbol, which is why this only showed at file
        // boundaries.
        // Only the NAMESPACE half of the merged flags is imported: a
        // function/class/enum merging with a namespace is LEGAL and takes
        // `typeof`; a cross-file `function D` + `enum D` is a DUPLICATE
        // IDENTIFIER upstream never merges — `duplicateIdentifierEnum`
        // (a passing case) wants `() => number` on the function's line, and
        // the unfiltered merge printed `typeof D` there (the full-stop rule's
        // catch, measured in).
        let merged = self.binder.merged_symbol(symbol);
        let merged_flags =
            flags | (self.binder.symbols().get(merged).flags & SymbolFlags::VALUE_MODULE);
        if merged_flags
            .intersects(SymbolFlags::ENUM | SymbolFlags::VALUE_MODULE | SymbolFlags::CLASS)
        {
            let printed = format!("typeof {name}");
            // A `TypeQueryNode`. Upstream gives it `TypePrecedenceTypeOperator`
            // so that it parenthesises in *postfix* position — `(typeof C)[]` —
            // and not as a union constituent.
            return self.store.new_anonymous(TypeFlags::OBJECT, printed, merged, false);
        }
        // §383: a late-bound METHOD's overloads live on SIBLING declarations —
        // the binder deliberately gives each `[Symbol.iterator]` its own
        // `__computed` symbol, so the merge upstream gets from late binding is
        // reconstructed here: same spelled name, same staticness, method
        // kinds only. Two or more merge into the overload set
        // (`symbolProperty42` wants `{ (x: string): string; (x: any): any; }`
        // on every declaration's line); a lone declaration keeps its road.
        let late_bound_overloads = 'late: {
            let data = self.binder.symbols().get(symbol);
            if data.name != "__computed" {
                break 'late None;
            }
            let (Some(parent), Some(declaration)) = (data.parent, data.value_declaration) else {
                break 'late None;
            };
            let method_static = |checker: &Self, id: tsr_ast::NodeId| -> Option<bool> {
                match checker.node_map.get(id) {
                    Some(Node::MethodDeclaration(method)) => {
                        Some(method.modifiers.iter().any(|modifier| {
                            matches!(modifier, tsr_ast::ModifierLike::Token(token)
                                if token.kind == SyntaxKind::StaticKeyword)
                        }))
                    }
                    Some(Node::MethodSignatureDeclaration(_)) => Some(false),
                    _ => None,
                }
            };
            let Some(own_static) = method_static(self, declaration) else { break 'late None };
            let members = self.late_bound_members_of(parent, own_static);
            let own_name =
                members.iter().find(|(_, id)| *id == declaration).map(|(name, _)| name.clone());
            let Some(own_name) = own_name else { break 'late None };
            let siblings: Vec<tsr_ast::NodeId> = members
                .into_iter()
                .filter(|(name, id)| {
                    *name == own_name && method_static(self, *id) == Some(own_static)
                })
                .map(|(_, id)| id)
                .collect();
            if siblings.len() < 2 {
                break 'late None;
            }
            let mut merged: Vec<crate::signatures::Signature> = Vec::with_capacity(siblings.len());
            let mut printed_forms: Vec<String> = Vec::new();
            for sibling in siblings {
                let Some(signature) = self.get_signature_from_declaration(sibling) else {
                    break 'late None;
                };
                // IDENTICAL siblings collapse — an overload spelled the same
                // as its implementation is ONE signature upstream
                // (`overloadsWithComputedNames` wants `() => void`, not
                // `{ (): void; (): void; }`), while distinct spellings keep
                // the set (`symbolProperty42`'s `(x: string)` + `(x: any)`).
                let printed = self.signature_to_string(&signature);
                if !printed_forms.contains(&printed) {
                    printed_forms.push(printed);
                    merged.push(signature);
                }
            }
            if merged.len() < 2 {
                // A deduped-to-one set is that one signature — the ordinary
                // road prints it as a bare function type.
                break 'late merged.pop().map(|single| vec![single]);
            }
            Some(merged)
        };
        let signatures = if let Some(merged) = late_bound_overloads {
            merged
        } else {
            let Some(signatures) = self.get_signatures_of_symbol_for_type(symbol) else {
                return self.intrinsics.error;
            };
            signatures
        };
        // `resolveAnonymousTypeMembers` supplies callable exports, excluding
        // type-only namespace members. `createTypeNodeFromObjectType` prints
        // those properties after call signatures. Instance members still need
        // the JavaScript constructor/prototype resolution path (tsr-6.27).
        let symbol_data = self.binder.symbols().get(symbol);
        let symbols = self.binder.symbols();
        let contributes_a_property =
            |&member: &SymbolId| symbols.get(member).flags.intersects(SymbolFlags::VALUE);
        if symbol_data.members.values().any(contributes_a_property) {
            return self.intrinsics.error;
        }
        let Some(export_properties) = self.callable_export_properties(symbol) else {
            return self.intrinsics.error;
        };
        let export_members = crate::callable_expandos::property_members(&export_properties);
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
        // `[signature]` renders a bare `FunctionTypeNode`; the many-signature
        // arm renders a `TypeLiteralNode`, which is never parenthesised.
        let mut signature_node = false;
        let printed = match signatures.as_slice() {
            [] => return self.intrinsics.error,
            [signature] if export_members.is_empty() => {
                signature_node = true;
                self.signature_to_string(signature)
            }
            many => {
                // §33's second containment (`checker-notes-callres.md`):
                // overload members that REUSE a type-parameter name print
                // upstream's site-sensitive `_1` renames (the §19/§20
                // refusal); when any member also carries `const` — the shape
                // §33 newly admits — the print declines whole rather than
                // spelling the un-renamed collision.
                let mut seen = std::collections::HashSet::new();
                let mut collision = false;
                let mut any_const = false;
                for signature in many {
                    for parameter in &signature.type_parameters {
                        any_const |= parameter.is_const;
                        if !seen.insert(parameter.name.clone()) {
                            collision = true;
                        }
                    }
                }
                // §60 re-learned §33's lesson the expensive way: a
                // GENERALIZED collision decline (no const gate) measured
                // 850 R→G — the promisePermutations family prints reused
                // names UN-renamed, so renames are site-sensitive and only
                // the const-carrying prints (which have no baseline stake)
                // may decline.
                if any_const && collision {
                    return self.intrinsics.error;
                }
                let mut members: Vec<_> = many
                    .iter()
                    .map(|signature| crate::objects::Member::Signature {
                        printed: crate::objects::signature_member_text(self, signature),
                    })
                    .collect();
                members.extend(export_members);
                crate::objects::render_object_type(&members)
            }
        };
        let resolved = self.store.new_anonymous(TypeFlags::OBJECT, printed, symbol, signature_node);
        let built = if let Some(reserved) = reserved {
            self.store.complete_object(reserved, resolved);
            reserved
        } else {
            resolved
        };
        // The structure `printed` was rendered from, kept reachable from the id
        // for `instantiate_type` — see `Checker::signature_types`
        // (`bd tsr-0hc`). Both arms are recorded; the single/many distinction
        // is recovered from the length when the instantiated form re-renders.
        if !export_properties.is_empty() {
            self.anonymous_properties.insert(built, (export_properties, false));
        }
        self.signature_types.insert(built, signatures);
        debug_assert_eq!(self.completed_callable_symbol(built), Some(symbol));
        // `checker.go:16930`: an OPTIONAL method carries `| undefined`.
        //
        // ```go
        // if c.strictNullChecks && symbol.Flags&ast.SymbolFlagsOptional != 0 {
        //     return c.getOptionalType(t /*isProperty*/, true)
        // }
        // ```
        //
        // §885. The arm this function's doc comment has recorded as unwritten —
        // *"only the excuse expired"* — since the checker gained compiler
        // options (ADR-0042).
        //
        // **Read off the declarations, not off the symbol.** Upstream's binder
        // sets the flag from `getOptionalSymbolFlagForNode`
        // (`binder.go:2727`), which is the declaration's postfix `?`; this
        // binder declares `SymbolFlags::OPTIONAL` and sets it nowhere, so
        // asking the flag would answer `false` everywhere. The flag is OR'd in
        // per declaration upstream, so an overload set is optional when ANY
        // declaration is — hence `any` rather than the value declaration alone.
        //
        // This is the whole of `declare const o5: { b?(): T }`: without it
        // `o5.b` is `() => T`, and every row downstream of it — `o5.b?.()`,
        // `o5.b?.()["c"]` — loses the chain's `undefined` too, because the
        // call road's `chain_stripped` is computed from a callee type that
        // never had anything to strip.
        if self.strict_null_checks && self.symbol_declaration_is_optional(symbol) {
            return self.get_optional_type(built, true);
        }
        built
    }

    /// Upstream's `symbol.Flags&ast.SymbolFlagsOptional`, recovered from the
    /// declarations because this binder does not set the flag. See the use in
    /// [`Checker::get_type_of_func_class_enum_module_worker`].
    ///
    /// The predicate is `getOptionalSymbolFlagForNode` (`binder.go:2727`) —
    /// **`node.PostfixToken()`, not `ast.HasQuestionToken`**. The two differ on
    /// a parameter: `(x?: string) => void` carries a `QuestionToken` and no
    /// postfix token, so it contributes no optionality to the *function's*
    /// symbol. Using [`Checker::is_optional_declaration`] here instead measured
    /// 24 adverse rows across `unionTypeReduction2`, `assignmentCompatBug2` and
    /// `objectLitGetterSetter` — every one a function type with an optional
    /// parameter, and none an optional member.
    fn symbol_declaration_is_optional(&self, symbol: SymbolId) -> bool {
        self.binder.symbols().get(symbol).declarations.iter().any(|&declaration| {
            let postfix = match self.node_map.get(declaration) {
                Some(Node::PropertyDeclaration(n)) => n.postfix_token,
                Some(Node::PropertySignatureDeclaration(n)) => n.postfix_token,
                Some(Node::MethodDeclaration(n)) => n.postfix_token,
                Some(Node::MethodSignatureDeclaration(n)) => n.postfix_token,
                _ => None,
            };
            postfix.is_some_and(|token| token.kind == SyntaxKind::QuestionToken)
        })
    }

    /// Whether `symbolToTypeNode` would spell this symbol as something other than
    /// its own name.
    ///
    /// One case: **an anonymous symbol** — a default export, an unnamed class
    /// expression. Upstream's node builder generates a name for one, and this
    /// port has nothing to bake.
    ///
    /// **An ambient module — `declare module "x"` — was this predicate's second
    /// arm until the `tryFindAmbientModule` slice**
    /// (`docs/architecture/checker-notes-modobj.md` §10). It now takes the same
    /// route a *file's* module symbol already takes: the baked `typeof x` text
    /// is a placeholder that must never reach a baseline, and the guard is the
    /// rendering interception — [`Checker::type_to_string_at`] names either
    /// kind of module at the reference site through the alias search, or
    /// answers `None` and the caller renders a gap. Refusing here instead kept
    /// every line *through* an ambient module at `errorType` even after the
    /// module resolved.
    fn has_a_name_no_type_query_can_spell(&self, symbol: SymbolId) -> bool {
        let name = self.binder.symbols().get(symbol).name;
        // `__class` is `InternalSymbolNameClass` — the binder's placeholder
        // for an anonymous class expression, never a spellable name. §305's
        // first draft missed it and printed `typeof __class` raw across
        // `classExpression3/4` — the exact wrong answer §168 recorded.
        name.is_empty() || name == "__class"
    }

    /// §305: the nameless-declaration arm of `getNameOfSymbolAsWritten`
    /// (`nodebuilderimpl.go:1005`). "Declaration may be nameless, but we'll
    /// try anyway": `GetAssignedName` (`utilities.go:1486`) walks one parent —
    /// property assignment, binding element, assignment right-hand side,
    /// variable declaration — and a walk that names nothing prints the
    /// literal `(Anonymous class)`. Only for class expressions — the function
    /// forms print structurally in the `.types` baseline and never reach the
    /// `typeof` leg.
    ///
    /// A name the walk *finds* but this port cannot spell (a string-literal
    /// property name, a computed one) answers `None` — upstream prints the
    /// written text of that name node, and `(Anonymous class)` there would be
    /// a wrong answer where a gap belongs.
    pub(crate) fn anonymous_class_written_name(&self, symbol: SymbolId) -> Option<String> {
        // `symbol.Declarations[0]`, as upstream reads it — an anonymous
        // declaration carries no `value_declaration` in this binder.
        let declaration = *self.binder.symbols().get(symbol).declarations.first()?;
        let Some(Node::ClassExpression(class)) = self.node_map.get(declaration) else {
            return None;
        };
        // A decorated class EXPRESSION is the parser's error recovery, not a
        // class the source wrote (decorators attach to class declarations
        // only) — `var F = @dec () => {}` recovers as one, and upstream's
        // answer for that line is `any`, never a name
        // (`conformance/decoratorOnArrowFunction`).
        if class
            .modifiers
            .iter()
            .any(|modifier| matches!(modifier, tsr_ast::ModifierLike::Decorator(_)))
        {
            return None;
        }
        // A class expression extending a PARAMETER — the mixin pattern,
        // `return class extends Base {}` — is an INTERSECTION upstream
        // (`getBaseTypeVariableOfClass`, `checker.go:16936`), never a bare
        // `typeof (Anonymous class)`: its static side prints structurally as
        // `{ new (...): (Anonymous class); ... } & TBase`
        // (`compiler/anonClassDeclarationEmitIsAnon`). The worker's doc
        // already accepts this limit for NAMED classes; the anonymous ones
        // decline here, on the syntactic proxy for "base is a type variable".
        for clause in class.heritage_clauses {
            if clause.token.kind != SyntaxKind::ExtendsKeyword {
                continue;
            }
            for base in clause.types {
                if let Some(tsr_ast::Expression::Identifier(base_name)) = base.expression
                    && let Some(base_id) = base_name.node_id
                    && let Some(base_symbol) = self.binder.resolve_name(
                        self.nodes,
                        self.node_map,
                        base_id,
                        base_name.text,
                        SymbolFlags::VALUE,
                    )
                    && let Some(&base_declaration) =
                        self.binder.symbols().get(base_symbol).declarations.first()
                    && self.nodes.kind(base_declaration) == SyntaxKind::Parameter
                {
                    return None;
                }
            }
        }
        let Some(parent) = self.nodes.parent(declaration) else {
            return Some("(Anonymous class)".to_string());
        };
        match self.node_map.get(parent) {
            Some(Node::PropertyAssignment(property)) => {
                if let tsr_ast::PropertyName::Identifier(name) = property.name {
                    return Some(name.text.to_string());
                }
                return None;
            }
            Some(Node::BindingElement(element)) => {
                if let Some(tsr_ast::BindingName::Identifier(name)) = element.name {
                    return Some(name.text.to_string());
                }
                return None;
            }
            Some(Node::BinaryExpression(binary)) => {
                if binary.right.and_then(|right| right.node_id()) == Some(declaration) {
                    match binary.left {
                        Some(tsr_ast::Expression::Identifier(left)) => {
                            return Some(left.text.to_string());
                        }
                        Some(tsr_ast::Expression::PropertyAccessExpression(access)) => {
                            if let Some(tsr_ast::MemberName::Identifier(name)) = access.name {
                                return Some(name.text.to_string());
                            }
                            return None;
                        }
                        // The element-access arm wants the literal argument's
                        // WRITTEN text (`utilities.go:1503`) — quotes and all —
                        // which this port does not carry. Refuse, not guess.
                        Some(tsr_ast::Expression::ElementAccessExpression(_)) => return None,
                        _ => {}
                    }
                }
            }
            Some(Node::VariableDeclaration(variable)) => {
                if let Some(tsr_ast::BindingName::Identifier(name)) = variable.name {
                    return Some(name.text.to_string());
                }
            }
            _ => {}
        }
        Some("(Anonymous class)".to_string())
    }

    /// Ported from `isShorthandAmbientModule` (`utilities.go:202`): *"the only
    /// kind of module that can be missing a body is a shorthand ambient module"*.
    pub(crate) fn is_shorthand_ambient_module(&self, symbol: SymbolId) -> bool {
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
    /// The §14 stand-in for upstream's evolving-array finalization depth:
    /// an auto-array declaration (no annotation, `= []`) whose container
    /// holds 2,000 or more element-mutation statements on the same name.
    /// Returns the container to disable. The count proxies `flow.go:1404`'s
    /// per-mutation recursion, which is what actually trips upstream's cap.
    fn too_large_evolving_array(
        &mut self,
        symbol: SymbolId,
        declaration: NodeId,
    ) -> Option<NodeId> {
        let Some(Node::VariableDeclaration(node)) = self.node_map.get(declaration) else {
            return None;
        };
        if node.r#type.is_some()
            || !matches!(
                node.initializer,
                Some(tsr_ast::Expression::ArrayLiteralExpression(array))
                    if array.elements.is_empty()
            )
        {
            return None;
        }
        let name = self.binder.symbols().get(symbol).name;
        let container = self.function_or_source_file_ancestor(declaration)?;
        let container_node = self.node_map.get(container)?;
        let mut count: u32 = 0;
        let mut stack = vec![container_node];
        let mut children = Vec::new();
        while let Some(current) = stack.pop() {
            if let Node::BinaryExpression(binary) = current
                && binary
                    .operator_token
                    .is_some_and(|token| token.kind == tsr_ast::SyntaxKind::EqualsToken)
                && let Some(tsr_ast::Expression::ElementAccessExpression(access)) = binary.left
                && matches!(
                    access.expression,
                    Some(tsr_ast::Expression::Identifier(identifier))
                        if identifier.text == name
                )
            {
                count += 1;
                if count >= 2_000 {
                    return Some(container);
                }
            }
            children.clear();
            tsr_ast::push_children(current, &mut children);
            stack.extend(children.iter().copied());
        }
        None
    }

    /// §98's second fired leg: a documented SLICE of upstream's
    /// `discriminateTypeByDiscriminableItems` (`checker.go:30779`), applied
    /// at the walk's ROOT with the outermost object literal. Sibling members
    /// whose initializer is a plain literal act as discriminators: a union
    /// constituent survives only if every discriminator's member on it
    /// contains that unit. No discriminators, a discriminator no constituent
    /// answers, or an empty survivor set leave the root unchanged — the
    /// undiscriminated behaviour, never a guess. Nested levels are not
    /// discriminated (upstream re-discriminates per level; unported).
    pub(crate) fn discriminate_union_root(&mut self, t: TypeId, literal: NodeId) -> TypeId {
        use tsr_ast::SyntaxKind;
        let crate::types::TypeData::Union { types, .. } = &self.store.get(t).data else {
            return t;
        };
        let constituents = types.clone();
        let Some(Node::ObjectLiteralExpression(object)) = self.node_map.get(literal) else {
            return t;
        };
        let mut discriminators: Vec<(String, TypeId)> = Vec::new();
        for member in object.properties {
            let tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) = member else {
                continue;
            };
            let name = match assignment.name {
                tsr_ast::PropertyName::Identifier(n) => n.text.to_string(),
                tsr_ast::PropertyName::StringLiteral(n) => n.text.to_string(),
                _ => continue,
            };
            let Some(initializer) = assignment.initializer else { continue };
            let context_free = match initializer {
                tsr_ast::Expression::StringLiteral(_) | tsr_ast::Expression::NumericLiteral(_) => {
                    true
                }
                tsr_ast::Expression::KeywordExpression(keyword) => {
                    matches!(keyword.kind, SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword)
                }
                _ => false,
            };
            if !context_free {
                continue;
            }
            let checked = self.check_expression(initializer);
            if !self.store.get(checked).flags.intersects(crate::flags::TypeFlags::UNIT) {
                continue;
            }
            // The §18 fresh/regular twin: the checked literal is FRESH, a
            // constituent's member holds the REGULAR twin — compare regular.
            let unit = self.get_regular_type_of_literal_type(checked);
            discriminators.push((name, unit));
        }
        if discriminators.is_empty() {
            return t;
        }
        // Upstream's ternary algorithm (`relater.go:1212`), mirrored: a
        // constituent LACKING the member survives untouched; a non-matching
        // member eliminates only when some constituent matched; primitives
        // never enter the filtered set. "Matches" here is the unit-contains
        // test plus the unit's own base primitive and `any` — an
        // assignability slice sufficient for unit discriminators.
        let mut include: Vec<bool> =
            constituents
                .iter()
                .map(|&c| {
                    !self.store.get(c).flags.intersects(
                        crate::flags::TypeFlags::PRIMITIVE | crate::flags::TypeFlags::NEVER,
                    )
                })
                .collect();
        let mut eliminated_any = false;
        for (name, unit) in &discriminators {
            let mut matched = false;
            let mut maybe: Vec<usize> = Vec::new();
            for (i, &constituent) in constituents.iter().enumerate() {
                if !include[i] {
                    continue;
                }
                // Intersection constituents (StringAttribute = Base & {type:'string'})
                // answer their discriminant only through the distributing road —
                // the flat lookup missed them and the skipped discriminator left
                // autoIncrement's constituent alive (the first pair's 0:104).
                let Some(member) = self.contextual_property_type(constituent, name) else {
                    continue;
                };
                if member == self.intrinsics.error {
                    continue;
                }
                let base = {
                    use crate::flags::TypeFlags;
                    let flags = self.store.get(*unit).flags;
                    if flags.contains(TypeFlags::STRING_LITERAL) {
                        self.intrinsics.string
                    } else if flags.contains(TypeFlags::NUMBER_LITERAL) {
                        self.intrinsics.number
                    } else if flags.contains(TypeFlags::BOOLEAN_LITERAL) {
                        self.intrinsics.boolean
                    } else {
                        self.intrinsics.error
                    }
                };
                let is_match = member == *unit
                    || member == base
                    || member == self.intrinsics.any
                    || matches!(
                        &self.store.get(member).data,
                        crate::types::TypeData::Union { types, .. }
                            if types.contains(unit) || types.contains(&base)
                    );
                if is_match {
                    matched = true;
                } else {
                    maybe.push(i);
                }
            }
            if matched {
                for i in maybe {
                    include[i] = false;
                    eliminated_any = true;
                }
            }
        }
        if std::env::var("TSR_CTX_DEBUG").is_ok() {
            eprintln!(
                "DISCRIM: {} discriminators, eliminated_any={eliminated_any}, include={include:?}",
                discriminators.len()
            );
        }
        if !eliminated_any {
            return t;
        }
        let survivors: Vec<TypeId> =
            constituents.iter().enumerate().filter(|(i, _)| include[*i]).map(|(_, &c)| c).collect();
        if survivors.is_empty() {
            return t;
        }
        match survivors.len() {
            1 => survivors[0],
            _ => self.get_union_type_unprinted(&survivors),
        }
    }

    /// §98 (`checker-notes-narrow.md`): the member step of the §56 walk,
    /// distributing over unions and intersections the way upstream's
    /// `getTypeOfPropertyOfContextualTypeEx` (`checker.go:30555`) maps over
    /// constituents via `mapTypeEx` with `noReductions`: each union
    /// constituent that has the member contributes its type and the hits
    /// union (unprinted — the result is consumed, never printed); an
    /// intersection collects per-constituent concrete properties and
    /// intersects. Generic mapped types inside are unported and decline.
    /// The UNIQUE call signature of `callee` whose arity accepts `count`
    /// arguments, or `None` when none or several do. §789.
    ///
    /// Used only to supply a CONTEXTUAL type, never an answer, which is why it
    /// demands uniqueness where [`Checker::arity_accepts`]'s other caller
    /// (§788's `new` road) takes the first fit: a wrong context silently
    /// retypes the argument, while a decline merely leaves the widening that
    /// was already there.
    fn sole_arity_matching_signature(
        &mut self,
        callee: TypeId,
        count: usize,
    ) -> Option<crate::signatures::Signature> {
        let candidates = match self.store.get(callee).data {
            crate::types::TypeData::Anonymous { symbol, .. } => {
                self.get_signatures_of_symbol(symbol)?
            }
            crate::types::TypeData::Named { .. } => self.signature_candidates_of_named_type(
                callee,
                crate::signatures::SignatureKind::Call,
            )?,
            _ => return None,
        };
        let mut fitting =
            candidates.into_iter().filter(|candidate| Self::arity_accepts(candidate, count));
        let first = fitting.next()?;
        if fitting.next().is_some() {
            return None;
        }
        Some(first)
    }

    /// `getTypeOfConcretePropertyOfContextualType`: exact optional context drops
    /// only missing. Optionality follows the same suppliers as the ordinary read.
    fn concrete_contextual_property_type(&mut self, t: TypeId, name: &str) -> Option<TypeId> {
        let member = self.get_type_of_property_of_type(t, name)?;
        if !self.exact_optional_property_types {
            return Some(member);
        }
        let optional = self
            .anonymous_properties
            .get(&t)
            .and_then(|(properties, instantiated)| {
                instantiated
                    .then(|| properties.iter().find(|property| property.name == name))
                    .flatten()
            })
            .map(|property| property.optional)
            .or_else(|| {
                self.mapped_identity_optionality.get(&t).and_then(|(optional, _)| *optional)
            })
            .unwrap_or_else(|| {
                self.get_property_of_type(t, name)
                    .is_some_and(|property| self.property_is_optional(property))
            });
        Some(if optional { self.remove_missing_type(member) } else { member })
    }

    pub(crate) fn contextual_property_type(&mut self, t: TypeId, name: &str) -> Option<TypeId> {
        if let Some(mapped) = self.generic_mapped_contextual_property_type(t, name) {
            return Some(mapped);
        }
        match &self.store.get(t).data {
            crate::types::TypeData::Union { types, .. } => {
                let constituents = types.clone();
                let mut hits = Vec::new();
                for constituent in constituents {
                    if let Some(member) = self.contextual_property_type(constituent, name) {
                        hits.push(member);
                    }
                }
                // §98's fired leg (excessPropertyCheckWithUnions 0:17/0:84,
                // both R→W on the first measurement): upstream DISCRIMINATES
                // the union by the literal's sibling members before this
                // lookup (`discriminateTypeByDiscriminableItems`,
                // `checker.go:30779`), so it sees one constituent's member
                // where this undiscriminated walk sees them all. When the
                // hits mix a literal unit with its own base primitive, which
                // constituent governs is exactly what discrimination decides
                // — unported, so the walk declines rather than guesses.
                if self.mixed_unit_and_base(&hits) {
                    return None;
                }
                match hits.len() {
                    0 => None,
                    1 => Some(hits[0]),
                    // mapTypeEx(..., noReductions=true) preserves context
                    // from the other constituents beside any/unknown.
                    _ => Some(self.get_union_type_without_reduction(&hits)),
                }
            }
            crate::types::TypeData::Intersection { types, .. } => {
                let constituents = types.clone();
                // `T & { prop: boolean }` widens its literal members upstream
                // (objectLiteralExcessProperties' obj2/obj4, R→W on the first
                // pair when this arm answered `boolean`); a type-parameter
                // constituent makes the member's context unshowable here.
                if constituents.iter().any(|&c| {
                    self.store.get(c).flags.intersects(crate::flags::TypeFlags::TYPE_PARAMETER)
                }) {
                    return None;
                }
                let mut hits = Vec::new();
                let mut index_candidates = Vec::new();
                let mut ignore_indexes = false;
                for constituent in constituents {
                    if !self
                        .store
                        .get(constituent)
                        .flags
                        .intersects(crate::flags::TypeFlags::OBJECT)
                    {
                        continue;
                    }
                    if let Some(member) =
                        self.generic_mapped_contextual_property_type(constituent, name)
                    {
                        hits.push(member);
                        continue;
                    }
                    if let Some(member) = self.concrete_contextual_property_type(constituent, name)
                        && member != self.intrinsics.error
                    {
                        hits.push(member);
                        ignore_indexes = true;
                        index_candidates.clear();
                    } else if !ignore_indexes {
                        index_candidates.push(constituent);
                    }
                }
                // getTypeOfPropertyOfContextualTypeEx: a concrete property in
                // any constituent suppresses all index-signature candidates.
                for candidate in index_candidates {
                    if let Some(member) = self.contextual_property_type(candidate, name) {
                        hits.push(member);
                    }
                }
                // appendContextualPropertyTypeConstituent replaces any with
                // unknown so it cannot erase another member's context.
                for member in &mut hits {
                    if *member == self.intrinsics.any {
                        *member = self.intrinsics.unknown;
                    }
                }
                match hits.len() {
                    0 => None,
                    1 => Some(hits[0]),
                    _ => Some(self.get_intersection_type(&hits, None)),
                }
            }
            _ => self.concrete_contextual_property_type(t, name).or_else(|| {
                let key = self.store.intern_literal(
                    crate::flags::TypeFlags::STRING_LITERAL,
                    crate::types::TypeData::StringLiteral(name.to_owned()),
                    false,
                );
                self.get_applicable_index_info(t, key).map(|info| info.value)
            }),
        }
    }

    /// §98's decline test: across the flattened `hits`, does any literal
    /// family appear both as a unit and as its base primitive? See the
    /// fired-leg comment at the union arm.
    fn mixed_unit_and_base(&self, hits: &[TypeId]) -> bool {
        use crate::flags::TypeFlags;
        let mut units = TypeFlags::empty();
        let mut bases = TypeFlags::empty();
        let mut leaves: Vec<TypeId> = Vec::new();
        for &hit in hits {
            if let crate::types::TypeData::Union { types, .. } = &self.store.get(hit).data {
                leaves.extend(types.iter().copied());
            } else {
                leaves.push(hit);
            }
        }
        for leaf in leaves {
            let flags = self.store.get(leaf).flags;
            for (unit, base) in [
                (TypeFlags::STRING_LITERAL, TypeFlags::STRING),
                (TypeFlags::NUMBER_LITERAL, TypeFlags::NUMBER),
                (TypeFlags::BIG_INT_LITERAL, TypeFlags::BIG_INT),
                (TypeFlags::BOOLEAN_LITERAL, TypeFlags::BOOLEAN),
            ] {
                if flags.contains(unit) {
                    units |= unit;
                } else if flags.contains(base) {
                    bases |= match unit {
                        TypeFlags::STRING_LITERAL => TypeFlags::STRING_LITERAL,
                        TypeFlags::NUMBER_LITERAL => TypeFlags::NUMBER_LITERAL,
                        TypeFlags::BIG_INT_LITERAL => TypeFlags::BIG_INT_LITERAL,
                        _ => TypeFlags::BOOLEAN_LITERAL,
                    };
                }
            }
        }
        units.intersects(bases)
    }

    /// §56: the annotation-derived contextual type of an object-literal
    /// MEMBER, reached syntactically — property assignments and nested
    /// object literals only, ending at a `VariableDeclaration` with a written
    /// annotation. `None` everywhere else (the refused general machinery).
    pub(crate) fn annotation_member_context(&mut self, declaration: NodeId) -> Option<TypeId> {
        let mut path: Vec<String> = Vec::new();
        let mut current = declaration;
        loop {
            let Some(Node::PropertyAssignment(assignment)) = self.node_map.get(current) else {
                return None;
            };
            let name = match assignment.name {
                tsr_ast::PropertyName::Identifier(n) => n.text.to_string(),
                tsr_ast::PropertyName::StringLiteral(n) => n.text.to_string(),
                _ => return None,
            };
            path.push(name);
            let literal = self.nodes.parent(current)?;
            if self.nodes.kind(literal) != SyntaxKind::ObjectLiteralExpression {
                return None;
            }
            let holder = self.nodes.parent(literal)?;
            match self.nodes.kind(holder) {
                SyntaxKind::PropertyAssignment => current = holder,
                // §56.3: the ARGUMENT position, single-candidate
                // non-generic callees only — the parameter's written type
                // is the contextual member root (`getContextualTypeForArgument`,
                // the one slice whose signature this port can already
                // resolve). Reentrancy-guarded: typing the callee from
                // inside a member-symbol computation can recurse.
                SyntaxKind::CallExpression => {
                    let debug = std::env::var("TSR_CTX_DEBUG").is_ok();
                    let Some(Node::CallExpression(call)) = self.node_map.get(holder) else {
                        return None;
                    };
                    let Some(callee) = call.expression else {
                        if debug {
                            eprintln!("CTX: no callee");
                        }
                        return None;
                    };
                    // The guard keys the CALL node: resolving the signature
                    // checks the ARGUMENTS, whose object-literal members
                    // walk back to this call — the cycle the first build hit
                    // as a stack overflow (`arrayToLocaleStringES2015`).
                    if !self.narrow_value_stack.insert(holder) {
                        return None;
                    }
                    let callee_type = self.check_expression(callee);
                    let resolved_context = self.resolved_call_signatures.get(&holder).cloned();
                    let signature = resolved_context
                        .clone()
                        .or_else(|| self.resolve_call_signature(callee_type, Some(call.arguments)));
                    self.narrow_value_stack.remove(&holder);
                    // §789: an OVERLOAD SET resolves to `None` above —
                    // `resolve_call_signature` answers only for a set it can
                    // choose from — so every object-literal argument of an
                    // overloaded call lost its contextual type and widened its
                    // literal members. `f({ u: "a" })` against
                    // `f(o: { u: "a" | "b" })` recorded `{ u: string; }` where
                    // the oracle records `{ u: "a"; }`.
                    //
                    // A probe ladder showed the failure is narrower than
                    // "overloads": single signatures, methods, and SAME-ARITY
                    // overload sets all work (the last through §70's agreement
                    // path). Only MIXED-ARITY sets fail, and for those the
                    // arity IS the choice — upstream's `chooseOverload` would
                    // discard every candidate that cannot take this many
                    // arguments before anything subtler runs.
                    //
                    // So: the UNIQUE arity-accepting candidate. Not "the
                    // first" — this road supplies a contextual type rather
                    // than an answer, and a wrong context silently retypes the
                    // argument, so a tie declines and keeps the widening that
                    // was already there. Same rule as §788, one notch more
                    // conservative because the failure mode is worse.
                    let count = call.arguments.len();
                    let signature = match signature {
                        // The resolved candidate is kept only when it could
                        // actually take this call. `resolve_call_signature`
                        // answers the FIRST candidate of an overload set
                        // without consulting arity, so a mixed-arity set handed
                        // this road the wrong parameter list — which is why the
                        // probe saw `CTX: no member u`, the `u` being looked
                        // for on the first overload's `x: number`.
                        Some(resolved) if Self::arity_accepts(&resolved, count) => Some(resolved),
                        _ => self.sole_arity_matching_signature(callee_type, count),
                    };
                    let Some(signature) = signature else {
                        if debug {
                            eprintln!("CTX: no signature (callee {callee_type:?})");
                        }
                        return None;
                    };
                    if !signature.type_parameters.is_empty()
                        || (resolved_context.is_none()
                            && self.signature_declares_type_parameters(signature.declaration))
                    {
                        if debug {
                            eprintln!("CTX: generic signature");
                        }
                        return None;
                    }

                    let index = call
                        .arguments
                        .iter()
                        .position(|argument| argument.node_id() == Some(literal))?;
                    let Some(parameter) = signature.parameters.get(index) else {
                        if debug {
                            eprintln!("CTX: no parameter at {index}");
                        }
                        return None;
                    };
                    if parameter.rest {
                        return None;
                    }
                    let mut t = parameter.r#type;
                    t = self.discriminate_union_root(t, literal);
                    for name in path.iter().rev() {
                        if t == self.intrinsics.error {
                            if debug {
                                eprintln!("CTX: error before member {name}");
                            }
                            return None;
                        }
                        let Some(next) = self.contextual_property_type(t, name) else {
                            if debug {
                                eprintln!("CTX: no member {name} on {t:?}");
                            }
                            return None;
                        };
                        t = next;
                    }
                    if debug {
                        eprintln!("CTX: answer {t:?} (error={})", t == self.intrinsics.error);
                    }
                    return (t != self.intrinsics.error).then_some(t);
                }
                // §56.1: the RETURN position — the literal returned from a
                // function whose return type is WRITTEN resolves its member
                // path against that annotation, the same rule at the arc's
                // second syntactically-provable position.
                SyntaxKind::ReturnStatement => {
                    let mut function = self.nodes.parent(holder)?;
                    loop {
                        match self.nodes.kind(function) {
                            SyntaxKind::FunctionDeclaration
                            | SyntaxKind::FunctionExpression
                            | SyntaxKind::ArrowFunction
                            | SyntaxKind::MethodDeclaration => break,
                            SyntaxKind::SourceFile
                            | SyntaxKind::ClassDeclaration
                            | SyntaxKind::ClassExpression => return None,
                            _ => function = self.nodes.parent(function)?,
                        }
                    }
                    let annotation = match self.node_map.get(function)? {
                        Node::FunctionDeclaration(f) => f.r#type,
                        Node::FunctionExpression(f) => f.r#type,
                        Node::ArrowFunction(f) => f.r#type,
                        Node::MethodDeclaration(f) => f.r#type,
                        _ => None,
                    }?;
                    let mut t = self.get_type_from_type_node(annotation);
                    t = self.discriminate_union_root(t, literal);
                    for name in path.iter().rev() {
                        if t == self.intrinsics.error {
                            return None;
                        }
                        t = self.contextual_property_type(t, name)?;
                    }
                    return (t != self.intrinsics.error).then_some(t);
                }
                // §98: the ASSIGNMENT root — `c.x = { a: "a" }` contextually
                // types its right operand by the LEFT operand's type
                // (`getContextualTypeForBinaryOperand`'s equals arm,
                // `checker.go:29843`). Plain `=` only; JS files decline (the
                // `module.exports` exclusion upstream carves is not modelled).
                SyntaxKind::BinaryExpression => {
                    let Some(Node::BinaryExpression(binary)) = self.node_map.get(holder) else {
                        return None;
                    };
                    if binary
                        .operator_token
                        .is_none_or(|token| token.kind != SyntaxKind::EqualsToken)
                        || binary.right.and_then(|e| e.node_id()) != Some(literal)
                        || self.in_js_file(declaration)
                    {
                        return None;
                    }
                    let left = binary.left?;
                    if !self.narrow_value_stack.insert(holder) {
                        return None;
                    }
                    let root = self.check_expression(left);
                    self.narrow_value_stack.remove(&holder);
                    if root == self.intrinsics.error {
                        return None;
                    }
                    let mut t = root;
                    t = self.discriminate_union_root(t, literal);
                    for name in path.iter().rev() {
                        if t == self.intrinsics.error {
                            return None;
                        }
                        t = self.contextual_property_type(t, name)?;
                    }
                    return (t != self.intrinsics.error).then_some(t);
                }
                SyntaxKind::VariableDeclaration => {
                    let Some(Node::VariableDeclaration(variable)) = self.node_map.get(holder)
                    else {
                        return None;
                    };
                    // §56's second fired leg (`tryCatchFinallyControlFlow`):
                    // a LET's retained literals flow into reassignment joins
                    // this port's assignment narrowing cannot reduce
                    // (§913: `getAssignmentReducedType` IS ported, at
                    // `flow.rs`'s `get_assignment_reduced_type`; the claim here
                    // was stale. The gate it justified was already removed by
                    // §58.1 below, so only the reason needed correcting) —
                    // CONST holders only. A §58 re-admission of LET was
                    // measured at +43/12 (3.6:1, below standard) — the joins
                    // of §56-retained ANONYMOUS object literals error in the
                    // union print road (`tryCatchFinallyControlFlow`'s 11,
                    // reproduced minimally: a catch-join of
                    // `{ tag: "one" }`-typed branches), which is that road's
                    // seam, not this gate's. Re-deferred with the diagnosis.
                    // §58.1 removed the const gate: the join-of-retained-
                    // anonymous-objects seam is fixed at the union worker's
                    // gate-decline (the member-set consult, object-membered
                    // sets only), so LET holders retain like const.
                    let annotation = variable.r#type?;
                    let mut t = self.get_type_from_type_node(annotation);
                    t = self.discriminate_union_root(t, literal);
                    for name in path.iter().rev() {
                        if t == self.intrinsics.error {
                            return None;
                        }
                        t = self.contextual_property_type(t, name)?;
                    }
                    return (t != self.intrinsics.error).then_some(t);
                }
                _ => return None,
            }
        }
    }

    /// §56's retention test: the contextual member is a UNIT, or a union of
    /// UNITs — the shapes where upstream's `isLiteralOfContextualType`
    /// answers yes without the relation.
    pub(crate) fn type_wants_literal(&self, contextual: TypeId, checked: TypeId) -> bool {
        // §56's first fired leg (`widenedTypes`): the contextual unit must
        // be the checked literal's own FAMILY — `typeof undefined` is a
        // unit that wants no string.
        use crate::flags::TypeFlags;
        let family = {
            let flags = self.store.get(checked).flags;
            if flags.contains(TypeFlags::STRING_LITERAL) {
                TypeFlags::STRING_LITERAL
            } else if flags.contains(TypeFlags::NUMBER_LITERAL) {
                TypeFlags::NUMBER_LITERAL
            } else if flags.contains(TypeFlags::BIG_INT_LITERAL) {
                TypeFlags::BIG_INT_LITERAL
            } else if flags.contains(TypeFlags::BOOLEAN_LITERAL) {
                TypeFlags::BOOLEAN_LITERAL
            } else {
                return false;
            }
        };
        let unit_of_family = |checker: &Self, id: TypeId| {
            let flags = checker.store.get(id).flags;
            flags.intersects(TypeFlags::UNIT) && flags.contains(family)
        };
        if unit_of_family(self, contextual)
            && !self.store.get(contextual).flags.contains(TypeFlags::UNION)
        {
            return true;
        }
        match &self.store.get(contextual).data {
            crate::types::TypeData::Union { types, .. } => {
                types.iter().any(|&part| unit_of_family(self, part))
                    && types
                        .iter()
                        .all(|&part| self.store.get(part).flags.intersects(TypeFlags::UNIT))
            }
            _ => false,
        }
    }

    fn get_type_of_variable_or_parameter_or_property(&mut self, symbol: SymbolId) -> TypeId {
        if let Some(&cached) = self.symbol_types.get(&symbol) {
            return cached;
        }
        if self.resolutions.on_stack(symbol, PropertyName::Type)
            && let Some(declaration) = self.binder.symbols().get(symbol).value_declaration
        {
            let annotation = self.type_annotation_of(declaration);
            if let Some(annotation) = annotation
                && matches!(
                    annotation,
                    TypeNode::TypeLiteralNode(_)
                        | TypeNode::FunctionTypeNode(_)
                        | TypeNode::ConstructorTypeNode(_)
                )
                && let Some(ty) =
                    Node::from(annotation).node_id().and_then(|id| self.cached_type_literal(id))
            {
                return self.add_optionality_for_declaration(ty, declaration);
            }
            // Native obtains an unannotated variable's callable initializer
            // identity before resolving that callable's lazy returns. Only a
            // direct function initializer supplies that fact; a call result or
            // arbitrary expression still participates in symbol circularity.
            if annotation.is_none()
                && let Some(initializer) = self.initializer_of(declaration)
                && matches!(
                    initializer,
                    Expression::ArrowFunction(_) | Expression::FunctionExpression(_)
                )
                && let Some(function) =
                    initializer.node_id().and_then(|id| self.binder.symbol_of(id))
                && let Some(&ty) = self.symbol_types.get(&function)
            {
                return self.add_optionality_for_declaration(ty, declaration);
            }
        }
        let computed = self.get_type_of_variable_or_parameter_or_property_worker(symbol);
        self.symbol_types.insert(symbol, computed);
        computed
    }

    /// Ported from `Checker.getTypeOfVariableOrParameterOrPropertyWorker`
    /// (`checker.go:16578`).
    fn get_type_of_variable_or_parameter_or_property_worker(&mut self, symbol: SymbolId) -> TypeId {
        #[cfg(feature = "work-trace")]
        let _work =
            self.trace_symbol_work(crate::work_trace::Operation::VariableTypeWorker, symbol);
        let Some(declaration) = self.binder.symbols().get(symbol).value_declaration else {
            return self.intrinsics.error;
        };
        // Native 5b1047d1 getTypeOfVariableOrParameterOrPropertyWorker:
        // CommonJS `exports` denotes the source file's resolved export value;
        // `module` is an anonymous object whose own members contain `exports`.
        // The symbol cache owns completion. Constructing the wrapper does not
        // force its exports, so recursive module values still use the existing
        // value/alias resolution stack rather than publishing an active result.
        if self.binder.symbols().get(symbol).flags.contains(SymbolFlags::MODULE_EXPORTS) {
            if self.binder.symbols().get(symbol).name == "exports" {
                let Some(module) = self.binder.symbol_of(declaration) else {
                    return self.intrinsics.error;
                };
                let target = self.resolve_external_module_symbol(module);
                return self.get_type_of_symbol(target);
            }
            return self.store.new_anonymous(TypeFlags::OBJECT, "{}".to_owned(), symbol, false);
        }
        // §14 (`checker-notes-narrow.md`): upstream finalizes an evolving
        // array (`const data = []`) by walking every mutation in the
        // container; past 2,000 the depth cap trips and TS2563 disables flow
        // analysis for the containing body (`largeControlFlowGraph`). The
        // SYMBOL keeps its widened `any[]` — the baseline prints it at the
        // declaration — while every flow REFERENCE in the disabled container
        // answers upstream's `errorType`, printed `any`. The count stands in
        // for the recursion this port's iterative walk never performs.
        if let Some(container) = self.too_large_evolving_array(symbol, declaration) {
            self.flow_disabled_containers.insert(container);
        }

        // The circularity guard wraps the *whole* computation, so a type that
        // reaches itself through any depth of indirection is caught. Nothing
        // between here and `pop` may return early, or the stack unbalances.
        if !self.resolutions.push(symbol, PropertyName::Type) {
            return self.report_circularity_error(symbol, declaration);
        }

        let kind = self.nodes.kind(declaration);
        let result = match kind {
            // JSON's export= property is declared on the SourceFile itself
            // (getTypeOfVariableOrParameterOrProperty, checker.go:16589).
            SyntaxKind::SourceFile
                if self.nodes.flags(declaration).contains(NodeFlags::JSON_FILE) =>
            {
                let Some(Node::SourceFile(file)) = self.node_map.get(declaration) else {
                    return self.intrinsics.error;
                };
                match file.statements.first() {
                    Some(tsr_ast::Statement::ExpressionStatement(statement)) => {
                        let checked =
                            statement.expression.map_or(self.intrinsics.error, |expression| {
                                self.check_expression(expression)
                            });
                        let literal = self.get_widened_literal_type(checked);
                        self.widen_object_literal_freshness(literal)
                    }
                    _ => self.intrinsics.empty_object,
                }
            }
            SyntaxKind::BinaryExpression | SyntaxKind::CallExpression => {
                self.get_widened_type_for_assignment_declaration(symbol)
            }
            SyntaxKind::VariableDeclaration
            | SyntaxKind::Parameter
            | SyntaxKind::PropertyDeclaration
            | SyntaxKind::PropertySignature => {
                self.get_widened_type_for_variable_like_declaration(declaration)
            }
            // Native reparses sibling @property tags as PropertySignatures
            // under their typedef. The binder provides that lexical ownership;
            // annotation resolution and optionality use the ordinary roads.
            SyntaxKind::JSDocPropertyTag => {
                let ty = self.type_annotation_of(declaration).map_or(
                    self.intrinsics.any,
                    |annotation| {
                        let ty = self.get_type_from_type_node(annotation);
                        // getTypeFromTypeNodeWorker adds explicit undefined
                        // for {T=}, even on an exact-optional property. Keep
                        // this at the supported sibling-property consumer;
                        // parameter default-initializer flow is still separate.
                        if matches!(annotation,
                            TypeNode::JSDocTypeExpression(expression)
                                if matches!(expression.r#type,
                                    Some(TypeNode::JSDocOptionalType(_))))
                        {
                            self.get_optional_type(ty, false)
                        } else {
                            ty
                        }
                    },
                );
                self.add_optionality_for_declaration(ty, declaration)
            }
            // §292 (relocated — the first draft sat in
            // get_type_for_variable_like_declaration, which this match never
            // reaches for the kind): `export default <expr>` — the `default`
            // symbol binds as PROPERTY and types as the assignment
            // expression's REGULAR form, unwidened (`importBindingDefer`
            // wants `defer : 2`).
            SyntaxKind::ExportAssignment => {
                // A `@type` tag on the assignment KEEPS THE GAP: upstream
                // types the tag, and the importing site re-spells its aliases
                // per module (`import("./a").NumberLike[]`,
                // `exportDefaultWithJSDoc1/2`) — the §158 per-site re-render
                // wall. The first draft typed the raw expression under the
                // tag (`never[]`, 5 G->W); the second typed the tag and
                // printed the local spelling, wrong the other way.
                if self.jsdoc_cast_annotation(declaration).is_some() {
                    self.intrinsics.error
                } else {
                    match self.node_map.get(declaration) {
                        // Native's property-valued ExportAssignment arm also
                        // handles non-alias `export =` expressions. A JSDoc CAST
                        // on the expression itself is the same §158 wall as a
                        // tag on the statement (`export default
                        // /** @type {..} */([])`, exportDefaultWithJSDoc2).
                        Some(Node::ExportAssignment(assignment)) => {
                            match assignment.expression {
                                Some(expression)
                                    if expression.node_id().is_none_or(|id| {
                                        self.jsdoc_cast_annotation(id).is_none()
                                    }) =>
                                {
                                    let checked = self.check_expression(expression);
                                    // Native widenTypeForVariableLikeDeclaration
                                    // (checker.go:18254) widens a foreign unique
                                    // symbol. is_valid_es_symbol_declaration admits
                                    // only const/readonly producers, never an
                                    // ExportAssignment, so this unannotated,
                                    // non-alias export= cannot own checked's unique.
                                    if assignment.is_export_equals
                                        && self
                                            .store
                                            .get(checked)
                                            .flags
                                            .intersects(TypeFlags::UNIQUE_ES_SYMBOL)
                                    {
                                        self.intrinsics.es_symbol
                                    } else {
                                        self.get_regular_type_of_literal_type(checked)
                                    }
                                }
                                _ => self.intrinsics.error,
                            }
                        }
                        _ => self.intrinsics.error,
                    }
                }
            }
            // `checkPropertyAssignment` (`checker.go:16611`), which is
            // `checkExpressionForMutableLocation` on the initialiser.
            //
            // **The same call `objects.rs` makes for the literal's printed
            // member text**, deliberately: the member symbol's type and the
            // text the object literal prints must not drift, and sharing the
            // function is what guarantees it rather than two rules that agree
            // today. It also carries the widening boundary for free —
            // `var o = { a: 1 }` gives the member `number`, not `1`, because
            // freshness stops at the property boundary. Written without it,
            // 2,500 numeric members would print `1` where upstream prints
            // `number`, turning gaps into wrong answers.
            SyntaxKind::PropertyAssignment => {
                let Some(Node::PropertyAssignment(assignment)) = self.node_map.get(declaration)
                else {
                    return self.intrinsics.error;
                };
                match assignment.initializer {
                    // §105 slice 2a: const context beats retention — the
                    // member symbol's type is the initializer's REGULAR type
                    // (`isConstContext`, `checker.go:13615`), matching what
                    // the object literal's own print now says.
                    Some(initializer) if self.is_const_context(declaration) => {
                        let checked = self.check_expression(initializer);
                        self.get_regular_type_of_literal_type(checked)
                    }
                    Some(initializer) => {
                        // §56 (`checker-notes-narrow.md`): a fresh literal
                        // RETAINS its literal form when the annotation's
                        // member wants a unit there
                        // (`isLiteralOfContextualType`, checker.go:13838).
                        if let Some(contextual) = self.annotation_member_context(declaration) {
                            let checked = self.check_expression(initializer);
                            if self.type_wants_literal(contextual, checked) {
                                self.get_regular_type_of_literal_type(checked)
                            } else {
                                self.check_expression_for_mutable_location(initializer)
                            }
                        } else {
                            self.check_expression_for_mutable_location(initializer)
                        }
                    }
                    None => self.intrinsics.error,
                }
            }
            // `checkShorthandPropertyAssignment` (`checker.go:16613`). Upstream
            // passes `inDestructuringPattern: true` here, which is what makes it
            // read the **name** rather than an `ObjectAssignmentInitializer` —
            // the same rule `objects.rs` follows for `{ a }`.
            SyntaxKind::ShorthandPropertyAssignment => {
                let Some(Node::ShorthandPropertyAssignment(shorthand)) =
                    self.node_map.get(declaration)
                else {
                    return self.intrinsics.error;
                };
                match shorthand.name {
                    tsr_ast::PropertyName::Identifier(name) => self
                        .check_expression_for_mutable_location(tsr_ast::Expression::Identifier(
                            name,
                        )),
                    _ => self.intrinsics.error,
                }
            }
            // A destructured name — `const {a} = o`, `function f([x]: T)`.
            // Upstream sends `KindBindingElement` through the same
            // `getWidenedTypeForVariableLikeDeclaration` as the four kinds
            // above (`checker.go:16603`); this port dispatches it to
            // `crate::destructure` directly, because its refusals are
            // `errorType` and must not take the widened path's None→`any`
            // mapping — an unported leg is a gap, not an implicit any.
            SyntaxKind::BindingElement => {
                let id = self.get_type_for_binding_element(declaration);
                // §459: upstream's road for a binding element IS
                // `getWidenedTypeForVariableLikeDeclaration` (checker.go:16603),
                // and its `getWidenedType` maps a nullable-only inference to
                // `any` under non-strict — `var [a, b] = [undefined, null]`
                // records `a : any` (`wideningTuples5`). The direct dispatch
                // above skipped exactly that tail; §187's arm, one road over.
                if !self.strict_null_checks && id != self.intrinsics.error {
                    let flags = self.store.get(id).flags;
                    if flags.intersects(TypeFlags::NULLABLE)
                        && !flags.intersects(!TypeFlags::NULLABLE)
                    {
                        return self.intrinsics.any;
                    }
                }
                id
            }
            // `checkJsxAttribute` (`jsx.go:871`), which is three lines:
            // `checkExpressionForMutableLocation` on the initialiser, or
            // `trueType` when there is none — `<Elem attr />` is sugar for
            // `<Elem attr={true} />`.
            //
            // The walker reaches this through the attribute's NAME, which is a
            // declaration name, so `x` in `<obj1 x={10} />` prints `number`:
            // the widening is `checkExpressionForMutableLocation`'s, exactly as
            // it is for a property assignment above. §195.
            SyntaxKind::JsxAttribute => {
                let Some(Node::JsxAttribute(attribute)) = self.node_map.get(declaration) else {
                    return self.intrinsics.error;
                };
                match attribute.initializer {
                    Some(tsr_ast::JsxAttributeValue::StringLiteral(literal)) => self
                        .check_expression_for_mutable_location(tsr_ast::Expression::StringLiteral(
                            literal,
                        )),
                    // `checkJsxExpression` (`jsx.go:89`) is `errorType` for an
                    // empty `{}` and the inner expression's type otherwise. The
                    // spread check it also performs reports; it does not change
                    // the answer.
                    Some(tsr_ast::JsxAttributeValue::JsxExpression(expression)) => {
                        match expression.expression {
                            Some(inner) => self.check_expression_for_mutable_location(inner),
                            None => self.intrinsics.error,
                        }
                    }
                    Some(tsr_ast::JsxAttributeValue::JsxElement(element)) => {
                        self.check_jsx_element(element.node_id)
                    }
                    Some(tsr_ast::JsxAttributeValue::JsxSelfClosingElement(element)) => {
                        self.check_jsx_element(element.node_id)
                    }
                    Some(tsr_ast::JsxAttributeValue::JsxFragment(fragment)) => {
                        self.check_jsx_element(fragment.node_id)
                    }
                    None => self.intrinsics.true_type,
                }
            }
            // Unported: methods, export assignments, binary/call assignment
            // declarations and enum members.
            _ => self.intrinsics.error,
        };

        if !self.resolutions.pop() {
            // A cycle closed *below* this frame, so the answer computed above was
            // built on a partial one and must not be kept.
            return self.report_circularity_error(symbol, declaration);
        }
        result
    }

    /// Ported from `Checker.reportCircularityError` (`checker.go:18822`).
    ///
    /// An annotated declaration reports TS2502 at the declaration and yields
    /// `errorType`. Native reports from both the failed push and the failed pop
    /// of `getTypeOfVariableOrParameterOrPropertyWorker`; its diagnostic
    /// collection drops the identical second report. `circularity_reported`
    /// (declaration node ids, Checker-private, whole-check lifetime) is that
    /// de-duplication and nothing else.
    ///
    /// The unannotated arm's TS7022 is not ported yet. It reports on every
    /// unannotated cycle participant, and this port forms cycles native does
    /// not: it lacks `getResolvedSignature`'s `resolutionStart` reset
    /// (`checker.go:8417`), resolves function-type signatures eagerly
    /// (`functionWithDefaultParameterWithNoStatements16`) and asks a later
    /// `const`'s type from flow (`typeGuardNarrowsIndexedAccessOfKnownProperty10`).
    ///
    /// The **return type differs by cause**, which is easy to get wrong because
    /// the two print identically:
    ///
    /// - a self-referencing *type annotation* yields `errorType`;
    /// - a self-referencing *initialiser* yields `anyType`.
    ///
    /// `bd tsr-4sc.2`'s issue text said "errorType" flatly. It is not.
    ///
    /// # §221: this fallback is too COARSE for a function, and that is a second
    /// constituency for the lazy-return-type work §217 named
    ///
    /// A function referenced inside its own body should not reach here at all.
    /// Upstream's `getTypeOfFuncClassEnumModule` builds the anonymous type from
    /// the *signature*, which needs no body, and defers the return type behind
    /// its own resolution frame (`TypeSystemPropertyNameResolvedReturnType`).
    /// So a cycle through the body degrades **the return slot** to `any` and
    /// the signature survives: upstream prints `() => any`. This port computes
    /// the return type eagerly as part of the symbol's type, so the guard above
    /// — which wraps the *whole* computation, deliberately and correctly for
    /// everything else — fires, and the answer is `any` for the entire
    /// function.
    ///
    /// `compiler/recursiveNamedLambdaCall` is the witness: `doScrollCheck` is a
    /// named function expression calling itself through
    /// `setTimeout( doScrollCheck, 50 )`, upstream records
    /// `>doScrollCheck : () => any`, and this port answers `any`. It is a
    /// deficit-1 case, so the whole case turns on that one line.
    ///
    /// **Do not "fix" it here.** Four probes rule out the obvious narrower
    /// patches: a plain function returning `any`, a class method, a method on a
    /// generic-based class, and a *simple* self-referential function
    /// (`function f() { return g(f); }`) all already print `() => any`
    /// correctly, as does a named function expression's own variable. Only the
    /// **reference site inside the body** is wrong, which is precisely the
    /// position that distinguishes a whole-symbol frame from a return-slot
    /// frame. Returning something other than `any` from this function would
    /// break the four cases that work in order to reach the one that does not.
    ///
    /// The prerequisite is `PropertyName::ResolvedReturnType`, which does not
    /// exist (`resolution.rs:191` has `Type` and `DeclaredType`), and it cannot
    /// be added usefully until return types are computed lazily. That is the
    /// same blocker §217 recorded from the opposite direction — it is now
    /// carried by two independent findings rather than one.
    fn report_circularity_error(&mut self, symbol: SymbolId, declaration: NodeId) -> TypeId {
        use tsr_diagnostics::{Diagnostic, messages};
        if self.type_annotation_of(declaration).is_none() {
            return self.intrinsics.any;
        }
        if let Some(file) = self.source_file_of_for_diagnostics(declaration)
            && self.circularity_reported.insert(declaration)
        {
            let name = self.binder.symbols().get(symbol).name.to_string();
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::_0_IS_REFERENCED_DIRECTLY_OR_INDIRECTLY_IN_ITS_OWN_TYPE_ANNOTATION,
                    self.error_span(declaration),
                    [name],
                ),
            );
        }
        self.intrinsics.error
    }

    /// Ported from `Checker.getWidenedTypeForVariableLikeDeclaration`, which is
    /// `widenTypeForVariableLikeDeclaration(getTypeForVariableLikeDeclaration(..))`
    /// (`checker.go:16610`, `:18242`).
    pub(crate) fn get_widened_type_for_variable_like_declaration(
        &mut self,
        declaration: NodeId,
    ) -> TypeId {
        // An annotation-less catch variable is `unknown` under
        // `useUnknownInCatchVariables` and `any` without it — never the
        // ordinary implicit-any road (`checker-notes-narrow.md` §21).
        if self.type_annotation_of(declaration).is_none()
            && self
                .nodes
                .parent(declaration)
                .is_some_and(|parent| self.nodes.kind(parent) == SyntaxKind::CatchClause)
        {
            return if self.use_unknown_in_catch_variables {
                self.intrinsics.unknown
            } else {
                self.intrinsics.any
            };
        }
        // §687: `getTypeOfVariableOrParameterOrPropertyWorker`'s property arm
        // (`checker.go:16752`) — *"a property declaration with no type
        // annotation or initializer, in noImplicitAny mode"* takes its type
        // from the CONSTRUCTOR's assignments, via `getFlowTypeInConstructor`.
        //
        // `class C { property; constructor() { this.property = `foo`; } }`
        // records `property : string` (`classAttributeInferenceTemplate`);
        // this port answered `any`. §599's auto-type arm is gated to
        // `VariableDeclaration` and excludes properties, but that exclusion was
        // reasoned about a property WITH an initializer (`foo = undefined`
        // keeps `undefined`, `implicitAnyCastedValue`) — a property with NONE
        // is this separate arm.
        //
        // The assignment WALK is an approximation of upstream's flow query: it
        // unions the widened types assigned to `this.<name>` anywhere in the
        // constructor, where upstream reads the flow type at the constructor's
        // end. The two agree wherever the constructor assigns unconditionally,
        // which is every line in the population; a conditional assignment would
        // differ and is the falsifier.
        if self.no_implicit_any
            && self.nodes.kind(declaration) == SyntaxKind::PropertyDeclaration
            && self.type_annotation_of(declaration).is_none()
            && self.initializer_of(declaration).is_none()
            && let Some(name) = self.property_declaration_name_text(declaration)
            && let Some(assigned) = self.constructor_assignment_types(declaration, &name)
            && !assigned.is_empty()
        {
            return self.get_union_type(&assigned);
        }
        if let Some(id) = self.get_type_for_variable_like_declaration(declaration) {
            // SS187 `getWidenedType` (checker.go:16090): with
            // `strictNullChecks` OFF, a `null` or `undefined` type widens to
            // `any` — `const c5 = null` records `>c5 : any` under
            // `@strict: false` (`compiler/constDeclarations`). This is a
            // DECISION, not a decline, so it is gated on the flag actually
            // read from the case's options; the paired control fixture
            // asserts `null` stays `null` under `@strict: true`.
            // Explicit annotations retain their non-widening null/undefined
            // types, including the BuiltinIteratorReturn intrinsic alias.
            if !self.strict_null_checks
                && self.type_annotation_of(declaration).is_none()
                && (id == self.intrinsics.null
                    || id == self.intrinsics.undefined
                    || id == self.intrinsics.undefined_widening)
            {
                return self.intrinsics.any;
            }
            // getWidenedTypeWithContext descends array type arguments. The
            // inferred non-strict empty element is a widening undefined;
            // written undefined[] retains the ordinary undefined identity.
            if !self.strict_null_checks
                && self.is_empty_array_literal_type(id)
                && let Some(array) = self.global_type_symbol("Array")
            {
                return self.create_type_reference(array, vec![self.intrinsics.any]);
            }
            // §461: `getWidenedType` DESCENDS a tuple (`checker.go:16090`'s
            // reference walk), so an inferred `[undefined, null]` widens
            // per-element to `[any, any]` under non-strict
            // (`wideningTuples3/4`). Unannotated declarations only — an
            // annotation's written tuple never widens — and plain tuples
            // only: an optional-masked tuple keeps its shape untouched.
            if !self.strict_null_checks
                && self.type_annotation_of(declaration).is_none()
                && !self.tuple_optional_masks.contains_key(&id)
                && let Some((elements, readonly)) = self.tuple_element_lists.get(&id).cloned()
                && elements.iter().any(|&element| {
                    let flags = self.store.get(element).flags;
                    flags.intersects(TypeFlags::NULLABLE) && !flags.intersects(!TypeFlags::NULLABLE)
                })
            {
                let any = self.intrinsics.any;
                let widened: Vec<_> = elements
                    .into_iter()
                    .map(|element| {
                        let flags = self.store.get(element).flags;
                        if flags.intersects(TypeFlags::NULLABLE)
                            && !flags.intersects(!TypeFlags::NULLABLE)
                        {
                            any
                        } else {
                            element
                        }
                    })
                    .collect();
                return self.create_tuple_type(widened, readonly);
            }
            self.widen_object_literal_freshness(id)
        } else {
            // Upstream returns `anyType` for a declaration with neither an
            // annotation nor an initialiser (`checker.go:18264`) — a genuine
            // answer, the implicit any, not a gap. So `anyType` is right here
            // where `errorType` is right for an unported form. A REST
            // parameter's implicit any is `anyArrayType`
            // (`checker-notes-narrow.md` §19).
            {
                // §429: an UNANNOTATED pattern parameter takes the PATTERN's
                // implied type (`getTypeFromBindingPattern`,
                // checker.go:17904) — `function fun([a, b]) {}` prints
                // `([a, b]: [any, any]) => void` (`iterableArrayPattern10`).
                // The canonical builder owns bounded defaults, rests and
                // nested patterns; annotation/context admission stays here.
                if let Some(Node::ParameterDeclaration(parameter)) =
                    self.node_map.get(declaration)
                    && parameter.r#type.is_none()
                    && parameter.dot_dot_dot_token.is_none()
                    // A FUNCTION DECLARATION's parameter is provably
                    // uncontextual; expression/arrow parameters may be
                    // contextually typed upstream and the implied `any`
                    // there was 78 G->W (`coAndContraVariantInferences3`).
                    && self.nodes.parent(declaration).is_some_and(|f| match self.nodes.kind(f) {
                        SyntaxKind::FunctionDeclaration | SyntaxKind::MethodDeclaration => true,
                        // §561: an ARROW or FUNCTION EXPRESSION too, but ONLY
                        // where §94's predicate can SHOW there is no contextual
                        // type at its position. §429 excluded them wholesale and
                        // its recorded reason is exactly this: *"expression/arrow
                        // parameters may be contextually typed upstream and the
                        // implied `any` there was 78 G->W
                        // (`coAndContraVariantInferences3`)"*. That is a claim
                        // about CONTEXTUALLY TYPED arrows, and
                        // `has_no_contextual_type` is the machinery already built
                        // to decide it — the same refinement §169 made to the
                        // return-type gate, for the same reason.
                        //
                        // Without it every destructured parameter of an arrow
                        // gapped while the identical `function` worked:
                        // `([x]) => x`, `({m}) => m` and `({a=1}={}) => a` all
                        // answered `error` where `function r([x]) { … }` answered
                        // `([x]: [any]) => any`.
                        //
                        // FALSIFIER: if `coAndContraVariantInferences3` loses
                        // lines, the predicate is not showing what it claims and
                        // this comes straight back out.
                        SyntaxKind::ArrowFunction | SyntaxKind::FunctionExpression => {
                            self.has_no_contextual_type(f)
                        }
                        _ => false,
                    })
                    && let Some(tsr_ast::BindingName::BindingPattern(pattern)) = parameter.name
                {
                    // Admission above is unchanged. The builder owns default,
                    // nested, rest and literal-key support; unsupported work is
                    // not the computed absence that justifies implicit any.
                    return self
                        .binding_pattern_implied_type(pattern)
                        .unwrap_or(self.intrinsics.error);
                }
                if let Some(Node::ParameterDeclaration(parameter)) =
                    self.node_map.get(declaration)
                    && parameter.dot_dot_dot_token.is_some()
                    // A written annotation that merely did not reach this
                    // path must print verbatim
                    // (`declFileRestParametersOfFunctionAndFunctionType`'s
                    // `...args: any`) — only a truly annotation-less rest
                    // parameter is `any[]`.
                    && parameter.r#type.is_none()
                {
                    let any = self.intrinsics.any;
                    if let Some(array) = self.global_type_symbol("Array") {
                        return self.create_type_reference(array, vec![any]);
                    }
                }
                self.intrinsics.any
            }
        }
    }

    /// The written name of a property declaration, when it is a plain
    /// identifier or string/numeric literal. §687.
    fn property_declaration_name_text(&self, declaration: NodeId) -> Option<String> {
        match self.node_map.get(declaration)? {
            Node::PropertyDeclaration(property) => match property.name {
                tsr_ast::PropertyName::Identifier(name) => Some(name.text.to_string()),
                tsr_ast::PropertyName::StringLiteral(name) => Some(name.text.to_string()),
                _ => None,
            },
            _ => None,
        }
    }

    /// The widened types assigned to `this.<name>` in the enclosing class's
    /// constructor. §687's approximation of `getFlowTypeInConstructor`.
    fn constructor_assignment_types(
        &mut self,
        declaration: NodeId,
        name: &str,
    ) -> Option<Vec<TypeId>> {
        // `this` means the INSTANCE in a constructor and the CLASS in a static
        // block, so each side reads its own bodies. A static property is not
        // what a constructor's `this.<name> = …` assigns —
        // `class Square { static sideLength; constructor(n: number)
        // { this.sideLength = n; } }` keeps `any` upstream (`staticVisibility2`,
        // the single regression the first cut of §687 measured) — while
        // `static accessor x; static { this.x = 1; }` DOES infer `number`
        // (`classStaticBlockUseBeforeDef4`). §688.
        let is_static = matches!(self.node_map.get(declaration), Some(Node::PropertyDeclaration(property))
        if property.modifiers.iter().any(|modifier| {
            matches!(modifier, tsr_ast::ModifierLike::Token(token)
                if token.kind == SyntaxKind::StaticKeyword)
        }));
        let class = self.nodes.parent(declaration)?;
        let members = match self.node_map.get(class)? {
            Node::ClassDeclaration(node) => node.members,
            Node::ClassExpression(node) => node.members,
            _ => return None,
        };
        let bodies: Vec<NodeId> = if is_static {
            members
                .iter()
                .filter_map(|member| match member {
                    tsr_ast::ClassElement::ClassStaticBlockDeclaration(node) => node.node_id,
                    _ => None,
                })
                .collect()
        } else {
            members
                .iter()
                .filter_map(|member| match member {
                    tsr_ast::ClassElement::ConstructorDeclaration(node) => node.node_id,
                    _ => None,
                })
                .collect()
        };
        if bodies.is_empty() {
            return None;
        }
        let mut assignments = Vec::new();
        for body in bodies {
            self.collect_this_assignments(body, name, &mut assignments);
        }
        let mut types = Vec::new();
        for expression in assignments {
            let checked = self.check_expression(expression);
            if checked == self.intrinsics.error {
                return None;
            }
            let widened = self.get_base_type_of_literal_type(checked);
            if !types.contains(&widened) {
                types.push(widened);
            }
        }
        Some(types)
    }

    /// Every right-hand side of `this.<name> = …` under `node`. §687.
    fn collect_this_assignments(
        &mut self,
        node: NodeId,
        name: &str,
        out: &mut Vec<tsr_ast::Expression<'a>>,
    ) {
        if let Some(Node::BinaryExpression(binary)) = self.node_map.get(node)
            && binary.operator_token.is_some_and(|token| token.kind == SyntaxKind::EqualsToken)
            && let Some(tsr_ast::Expression::PropertyAccessExpression(access)) = binary.left
            && matches!(
                access.expression,
                Some(tsr_ast::Expression::KeywordExpression(keyword))
                    if keyword.kind == SyntaxKind::ThisKeyword
            )
            && matches!(access.name, Some(tsr_ast::MemberName::Identifier(n)) if n.text == name)
            && let Some(right) = binary.right
        {
            out.push(right);
        }
        let Some(current) = self.node_map.get(node) else { return };
        let mut children = Vec::new();
        tsr_ast::push_children(current, &mut children);
        for child in children {
            if let Some(id) = child.node_id() {
                self.collect_this_assignments(id, name, out);
            }
        }
    }

    /// Ported from `Checker.getTypeForVariableLikeDeclaration`
    /// (`checker.go:16652`), restricted to the two paths this slice covers.
    ///
    /// `None` means "nothing could be inferred", which upstream signals with a
    /// nil `*Type` and turns into the implicit `any` one level up.
    /// §38's element slice: `Array`/`ReadonlyArray` references answer the
    /// argument, tuples the union of their elements, strings `string`.
    /// `None` declines to the implicit-any road.
    pub(crate) fn for_of_element_type(&mut self, iterated: TypeId) -> Option<TypeId> {
        let yields = self.for_of_yield_types(iterated)?;
        // Only the consumer replaces an absent yield with any. Combining a
        // union of iterables must first omit constituents that only return.
        Some(if yields.is_empty() { self.intrinsics.any } else { self.get_union_type(&yields) })
    }

    /// getIteratedTypeOrElementType without the consumer's any recovery.
    /// A destructuring target uses unknown when no yield type is present.
    pub(crate) fn iterated_element_type(&mut self, iterated: TypeId) -> Option<TypeId> {
        let yields = self.for_of_yield_types(iterated)?;
        (!yields.is_empty()).then(|| self.get_union_type(&yields))
    }

    /// `checkRightHandSideOfForOf` (`checker.go:17678`) for a for-of head:
    /// the iterated element of `expression`, through the async-first resolver
    /// for `for await`. `None` when the element is undecided here.
    pub(crate) fn for_of_statement_element_type(
        &mut self,
        expression: tsr_ast::Expression<'_>,
        is_async: bool,
    ) -> Option<TypeId> {
        let iterated = self.check_expression(expression);
        if is_async {
            self.for_await_of_yield_types(iterated).map(|yields| {
                if yields.is_empty() { self.intrinsics.any } else { self.get_union_type(&yields) }
            })
        } else {
            self.for_of_element_type(iterated)
        }
    }

    /// `ForAwaitOf`'s async-first getIterationTypesOfIterableWorker and
    /// getAsyncFromSyncIterationTypes (pinned tsgo 5b1047d, checker.go:6288).
    /// Resolve each iterable union constituent before combining its yields.
    /// Preserve async absence; decline sync absence (the pinned native
    /// getAsyncFromSyncIterationTypes currently panics on its nil yield).
    fn for_await_of_yield_types(&mut self, mut source: TypeId) -> Option<Vec<TypeId>> {
        let mut seen = Vec::new();
        while self.store.get(source).flags.contains(TypeFlags::TYPE_PARAMETER) {
            if seen.contains(&source) {
                return None;
            }
            seen.push(source);
            source = self.type_parameter_constraint(source)?;
        }
        if source == self.intrinsics.error {
            return None;
        }
        if source == self.intrinsics.any {
            return Some(vec![source]);
        }
        if let crate::types::TypeData::Union { types, .. } = self.store.get(source).data.clone() {
            let yields = types
                .into_iter()
                .map(|part| self.for_await_of_yield_types(part))
                .collect::<Option<Vec<_>>>()?;
            return Some(yields.into_iter().flatten().collect());
        }
        // getIterationTypesOfIterableFast uses global symbol identity, not
        // written names. The async resolver awaits its generic yield slot.
        if let Some((target, arguments)) = self.type_reference_targets.get(&source).cloned()
            && let Some(&yield_type) = arguments.first()
            && [
                "AsyncIterable",
                "AsyncIteratorObject",
                "AsyncIterableIterator",
                "AsyncGenerator",
                "ReadableStreamAsyncIterator",
            ]
            .into_iter()
            .filter_map(|name| self.binder.globals().get(name).copied())
            .any(|symbol| self.binder.merged_symbol(symbol) == self.binder.merged_symbol(target))
        {
            return Some(vec![self.awaited_type(yield_type)?]);
        }
        if let Some(yields) = self.semantic_iterable_yield_types(source, true).ok()? {
            return Some(yields);
        }
        let yields = self.for_of_yield_types(source)?;
        if yields.is_empty() {
            return None;
        }
        let yield_type = self.get_union_type(&yields);
        Some(vec![self.awaited_type(yield_type)?])
    }

    /// None is an invalid or unresolved protocol; an empty vector is a valid
    /// iterator with no yield. Consumers choose their own absent-yield recovery.
    /// A real never yield remains a one-element vector containing never.
    fn for_of_yield_types(&mut self, iterated: TypeId) -> Option<Vec<TypeId>> {
        // getPropertyOfType reads the apparent type of a type parameter when
        // resolving its iterator; the array shortcut uses the same constraint.
        let mut iterated = iterated;
        let mut seen = Vec::new();
        while self.store.get(iterated).flags.contains(crate::flags::TypeFlags::TYPE_PARAMETER) {
            if seen.contains(&iterated) {
                return None;
            }
            seen.push(iterated);
            iterated = self.type_parameter_constraint(iterated)?;
        }
        if iterated == self.intrinsics.error {
            return None;
        }
        // getIteratedTypeOrElementType reads the numeric tuple index signature,
        // including each variadic operand's own deferred number access.
        if let Some(element) = self.variadic_tuple_index_union(iterated) {
            return Some(vec![element]);
        }
        if let Some((elements, _)) = self.tuple_element_lists.get(&iterated) {
            let elements = elements.clone();
            if elements.is_empty() {
                return None;
            }
            return Some(vec![self.get_union_type(&elements)]);
        }
        if let Some((target, arguments)) = self.type_reference_targets.get(&iterated).cloned()
            && arguments.len() == 1
        {
            let target = self.binder.merged_symbol(target);
            let is_array = self.global_type_symbol("Array").map(|s| self.binder.merged_symbol(s))
                == Some(target)
                || self.global_type_symbol("ReadonlyArray").map(|s| self.binder.merged_symbol(s))
                    == Some(target);
            // §267: the `never` decline is removed — upstream's OWN baselines
            // answer `never` for `for (let v of [])` (for-of51 through
            // for-of54, all four: `>v : never` beside `>[] : never[]`). The
            // decline cited "upstream's binding there reads `any` (the §38
            // second fired leg)"; whatever witness that leg fired on, it was
            // not this shape, and four committed baselines outrank a recalled
            // measurement. The `undefined` spelling (non-strict
            // `array_literals.rs:122`) stays declined: no baseline was found
            // answering `undefined` through this road, so it keeps the gap
            // until one is.
            if is_array && arguments[0] != self.intrinsics.undefined {
                return Some(vec![arguments[0]]);
            }
        }
        // §251. The SAME shortcut the `Array` arm above already takes, applied
        // to the other lib types whose first type argument IS their iteration
        // type by declaration:
        //
        //     interface Iterable<T, …>         { [Symbol.iterator](): Iterator<T, …> }
        //     interface IterableIterator<T, …> { … }
        //     interface Generator<T, …>        { … }
        //
        // This is not a new induction. Upstream reaches an array's element
        // type through the iteration protocol too, and the `Array` arm above
        // already declines to walk it and reads the argument instead. These
        // four types are the population that decision was implicitly about;
        // naming only `Array` was the accident.
        //
        // Witness `conformance/for-of57`: `var iter: Iterable<number>;
        // for (let num of iter) { }` wants `>num : number`.
        //
        // Arity is deliberately not constrained the way the `Array` arm
        // constrains it: `Generator<T, TReturn, TNext>` and the modern
        // `Iterable<T, TReturn, TNext>` carry three, and the first slot is the
        // yield type in every one of them.
        if let Some((target, arguments)) = self.type_reference_targets.get(&iterated).cloned()
            && let Some(&first) = arguments.first()
            && first != self.intrinsics.never
        {
            let target = self.binder.merged_symbol(target);
            // Looked up through `globals()` rather than `global_type_symbol`,
            // which hard-codes ARITY 1 (`declared.rs:1412`). The modern lib
            // declares all three with three parameters —
            // `Iterable<T, TReturn = undefined, TNext = any>` — so the arity
            // helper answers `None` for every one of them and the arm never
            // fired. Measured that way first: +0 cases and zero line
            // transitions, which is conventions corollary 27's first face and
            // is why the fire check came before the conclusion.
            // §573: `Set` and `ReadonlySet` belong to the same population and
            // were simply missing. `interface Set<T> { [Symbol.iterator]():
            // IterableIterator<T> }` — the first type argument IS the
            // iteration type, exactly as for the three above, so
            // `for (const d of new Set([1]))` answered `any` where the
            // identical `Generator` answered `number`.
            //
            // **`Map` is deliberately NOT here**: `Map<K, V>` iterates
            // `[K, V]`, a TUPLE of both arguments, so reading `arguments[0]`
            // would answer `K` and be confidently wrong. The rule this list
            // encodes is *the first argument is the iteration type*, and `Map`
            // does not satisfy it — the same reason `Array` is handled by its
            // own arm above rather than being added here.
            let is_lib_iterable =
                ["Iterable", "IterableIterator", "Generator", "Set", "ReadonlySet"]
                    .into_iter()
                    .filter_map(|name| self.binder.globals().get(name).copied())
                    .any(|symbol| self.binder.merged_symbol(symbol) == target);
            if is_lib_iterable {
                return Some(vec![first]);
            }
        }
        let flags = self.store.get(iterated).flags;
        if flags.intersects(crate::flags::TypeFlags::STRING_LITERAL)
            || iterated == self.intrinsics.string
        {
            return Some(vec![self.intrinsics.string]);
        }
        if let Ok(Some(yield_type)) = self.semantic_iterable_yield_types(iterated, false) {
            return Some(yield_type);
        }
        // Preserve the declaration-based fallback while callable return types
        // remain eager. A recursive iterator method can reach this path during
        // its own return inference (for-of33/34); reading its symbol type again
        // would invalidate the enclosing callable's resolution frame.
        if self.declares_symbol_iterator(iterated)
            && let Some(next_id) = self.class_method_declaration(iterated, "next")
            && let Some(signature) = self.get_signature_from_declaration(next_id)
            && signature.r#type != self.intrinsics.error
            && let Some(value) = self.get_type_of_property_of_type(signature.r#type, "value")
            && value != self.intrinsics.error
        {
            return Some(vec![value]);
        }
        None
    }

    /// getIterationTypesOfIterableSlow/getIterationTypesOfMethod (:6460, :6541).
    /// Ok(None) is decidably absent iteration types; Err is unsupported work,
    /// never evidence permitting async-to-sync fallback. Ok(Some([])) has
    /// iteration types but no yield; [never] is a real never yield.
    ///
    /// Private Checker owns the existing `TypeId` active set (no completed cache).
    /// Async/sync queries run sequentially; re-entry declines, and removing the
    /// active key publishes no reusable success or failure. Expensive work is
    /// the member/signature/IteratorResult walk below, reusing inherited member
    /// substitution and alias identities. No receiver or presentation image is
    /// copied. Async absence reuses the existing per-name `miss_is_established`
    /// class/interface contract, not a module or augmentation completeness claim.
    /// Existing member-work attribution is tracked in tsr-1yb.11.1.
    fn semantic_iterable_yield_types(
        &mut self,
        source: TypeId,
        is_async: bool,
    ) -> Result<Option<Vec<TypeId>>, ()> {
        if !self.resolving_iteration_types.insert(source) {
            return Err(());
        }
        let result = self.semantic_iterable_yield_types_worker(source, is_async);
        self.resolving_iteration_types.remove(&source);
        result
    }

    pub(crate) fn iteration_property_type(&mut self, source: TypeId, name: &str) -> Option<TypeId> {
        // Native function objects exist before their lazy return types resolve.
        // Our eager function type cannot be requested again while its own
        // return type is resolving: doing so marks the whole callable as a
        // failed cycle. Leave that unresolved iteration edge to its caller.
        if let Some(symbol) = self.get_property_of_type(source, name)
            && self.resolutions.on_stack(symbol, PropertyName::Type)
        {
            return None;
        }
        self.get_type_of_property_of_type(source, name)
    }

    fn semantic_iterable_yield_types_worker(
        &mut self,
        source: TypeId,
        is_async: bool,
    ) -> Result<Option<Vec<TypeId>>, ()> {
        if let crate::types::TypeData::Union { types, .. } = self.store.get(source).data.clone() {
            let types = types
                .into_iter()
                .map(|part| self.for_of_yield_types(part))
                .collect::<Option<Vec<_>>>()
                .ok_or(())?;
            return Ok(Some(types.into_iter().flatten().collect()));
        }
        let name = if is_async { "[Symbol.asyncIterator]" } else { "[Symbol.iterator]" };
        let Some(iterator) = self.iteration_property_type(source, name) else {
            if self.get_property_of_type(source, name).is_some() {
                return Err(());
            }
            return if self.declared_property_table(source).is_some()
                || self.miss_is_established(source, name)
                || self.store.get(source).flags.intersects(TypeFlags::PRIMITIVE)
            {
                Ok(None)
            } else {
                Err(())
            };
        };
        if self.is_error(iterator) {
            return Err(());
        }
        if iterator == self.intrinsics.any {
            return Ok(Some(vec![iterator]));
        }
        if self
            .get_property_of_type(source, name)
            .is_some_and(|symbol| self.property_is_optional(symbol))
        {
            return Ok(None);
        }
        let signatures = if self.store.get(iterator).flags.intersects(TypeFlags::PRIMITIVE) {
            Vec::new()
        } else {
            self.call_signatures_of_type(iterator).ok_or(())?
        };
        let mut returns = Vec::new();
        for signature in signatures {
            if self.signature_min_argument_count(&signature) == 0 {
                if self.is_error(signature.r#type) {
                    return Err(());
                }
                returns.push(signature.r#type);
            }
        }
        if returns.is_empty() {
            return Ok(None);
        }
        let iterator = self.get_intersection_type(&returns, None);
        if iterator == self.intrinsics.any {
            return Ok(Some(vec![iterator]));
        }
        let mut yields = Vec::new();
        let mut has_iteration_types = false;
        for name in ["next", "return", "throw"] {
            let Some(mut method) = self.iteration_property_type(iterator, name) else {
                if is_async
                    && (self.get_property_of_type(iterator, name).is_some()
                        || (self.declared_property_table(iterator).is_none()
                            && !self.miss_is_established(iterator, name)))
                {
                    return Err(());
                }
                if name == "next" && self.declared_property_table(iterator).is_none() {
                    return Err(());
                }
                // getIterationTypesOfMethod returns absent iteration types
                // for a missing method. Other methods may still yield.
                continue;
            };
            if self.is_error(method) {
                return Err(());
            }
            if name == "next"
                && self
                    .get_property_of_type(iterator, name)
                    .is_some_and(|symbol| self.property_is_optional(symbol))
            {
                continue;
            }
            if name != "next" {
                method = self.get_non_nullable_type(method);
            }
            if method == self.intrinsics.any {
                return Ok(Some(vec![method]));
            }
            let signatures = if self.store.get(method).flags.intersects(TypeFlags::PRIMITIVE) {
                Vec::new()
            } else {
                self.call_signatures_of_type(method).ok_or(())?
            };
            if signatures.is_empty() {
                // Native reports a diagnostic but contributes no iteration
                // types, including for a non-callable next method.
                continue;
            }
            has_iteration_types = true;
            let returns: Vec<_> = signatures.iter().map(|signature| signature.r#type).collect();
            if returns.iter().any(|&ty| self.is_error(ty)) {
                return Err(());
            }
            let mut result = self.get_intersection_type(&returns, None);
            if is_async {
                // Async slow awaits the method's IteratorResult, not its value.
                result = self.awaited_type(result).ok_or(())?;
            }
            // Native aliases already carry their body's union identity. Resolve
            // this port's named alias before separating yield and return arms.
            let mut visited = Vec::new();
            while let Some((symbol, arguments)) = self.type_reference_targets.get(&result).cloned()
            {
                if visited.contains(&result) {
                    break;
                }
                visited.push(result);
                let Some(body) = self.evaluate_alias_body(symbol, &arguments) else { break };
                if body == result {
                    break;
                }
                result = body;
            }
            let parts = match self.store.get(result).data.clone() {
                crate::types::TypeData::Union { types, .. } => types,
                _ => vec![result],
            };
            if parts.contains(&self.intrinsics.any) {
                return Ok(Some(vec![self.intrinsics.any]));
            }
            // getIterationTypesOfIteratorResult filters yield and return
            // constituents separately, then reads value on each entire union.
            // One missing yield value must not borrow another yield arm's value;
            // a present return value can still make the result valid with no yield.
            let mut values = [None, None];
            for (index, truth) in
                [self.intrinsics.false_type, self.intrinsics.true_type].into_iter().enumerate()
            {
                let mut results = Vec::new();
                for &part in &parts {
                    if self.store.get(part).flags.contains(TypeFlags::NEVER) {
                        continue;
                    }
                    let done = self
                        .get_type_of_property_of_type(part, "done")
                        .unwrap_or(self.intrinsics.false_type);
                    if self.is_error(done) {
                        return Err(());
                    }
                    match self.relate_ternary(truth, done, crate::relater::Relation::Assignable) {
                        crate::relater::Ternary::NotRelated => {}
                        crate::relater::Ternary::Unknown => return Err(()),
                        crate::relater::Ternary::Related => results.push(part),
                    }
                }
                if results.is_empty() {
                    continue;
                }
                let result = self.get_union_type(&results);
                let value = self.get_type_of_property_of_type(result, "value");
                if let Some(value) = value {
                    if self.is_error(value) {
                        return Err(());
                    }
                    values[index] = Some(value);
                } else if !results.iter().all(|&part| {
                    self.declared_property_table(part).is_some()
                        || self.store.get(part).flags.intersects(
                            TypeFlags::PRIMITIVE | TypeFlags::NON_PRIMITIVE | TypeFlags::UNKNOWN,
                        )
                }) {
                    return Err(());
                }
            }
            if let Some(value) = values[0] {
                yields.push(value);
            } else if values[1].is_none() {
                // With neither yield nor return value, native's method
                // resolver recovers an invalid IteratorResult with any.
                yields.push(self.intrinsics.any);
            }
        }
        // combineIterationTypes preserves absence until all iterable union
        // constituents have contributed their yield types. No method types
        // at all is an invalid protocol, not a completed-only iterator.
        Ok(has_iteration_types.then_some(yields))
    }

    /// Whether an iterator provably contributes no next/return/throw types.
    /// A complete table distinguishes missing methods from unresolved ones.
    pub(crate) fn iteration_methods_decidably_absent(&mut self, iterator: TypeId) -> bool {
        let Some(properties) = self.declared_property_table(iterator) else { return false };
        for name in ["next", "return", "throw"] {
            let Some((_, optional)) = properties.iter().find(|(property, _)| property == name)
            else {
                continue;
            };
            if name == "next" && *optional {
                continue;
            }
            let Some(mut method) = self.iteration_property_type(iterator, name) else {
                return false;
            };
            if self.is_error(method) || method == self.intrinsics.any {
                return false;
            }
            if name != "next" {
                method = self.get_non_nullable_type(method);
            }
            if !self.store.get(method).flags.intersects(TypeFlags::PRIMITIVE)
                && !self.call_signatures_of_type(method).is_some_and(|types| types.is_empty())
            {
                return false;
            }
        }
        true
    }

    /// §284: the class method declaration of the given name, found on the
    /// type's class declaration — the syntactic sibling of
    /// [`Checker::declares_symbol_iterator`].
    fn class_method_declaration(&self, iterated: TypeId, wanted: &str) -> Option<NodeId> {
        let crate::types::TypeData::Named { members: Some(symbol), .. } =
            self.store.get(iterated).data
        else {
            return None;
        };
        self.binder.symbols().get(symbol).declarations.iter().find_map(|&declaration| {
            let members = match self.node_map.get(declaration) {
                Some(Node::ClassDeclaration(class)) => class.members,
                Some(Node::ClassExpression(class)) => class.members,
                _ => return None,
            };
            members.iter().find_map(|member| match member {
                tsr_ast::ClassElement::MethodDeclaration(method)
                    if matches!(method.name, tsr_ast::PropertyName::Identifier(name)
                        if name.text == wanted) =>
                {
                    method.node_id
                }
                _ => None,
            })
        })
    }

    /// §284: whether the type's class declaration carries a computed
    /// `[Symbol.iterator]` member — a SYNTACTIC presence test, the §145
    /// `[Symbol.hasInstance]` precedent, which is what makes the iterator
    /// protocol reachable without late binding.
    pub(crate) fn declares_symbol_iterator(&self, iterated: TypeId) -> bool {
        let crate::types::TypeData::Named { members: Some(symbol), .. } =
            self.store.get(iterated).data
        else {
            return false;
        };
        self.binder.symbols().get(symbol).declarations.iter().any(|&declaration| {
            let members = match self.node_map.get(declaration) {
                Some(Node::ClassDeclaration(class)) => class.members,
                Some(Node::ClassExpression(class)) => class.members,
                _ => return false,
            };
            members.iter().any(|member| {
                let name = match member {
                    tsr_ast::ClassElement::MethodDeclaration(method) => &method.name,
                    _ => return false,
                };
                matches!(name, tsr_ast::PropertyName::ComputedPropertyName(computed)
                    if computed.expression.is_some_and(|e| matches!(e,
                        tsr_ast::Expression::PropertyAccessExpression(access)
                            if matches!(access.name,
                                Some(tsr_ast::MemberName::Identifier(n)) if n.text == "iterator")
                            && matches!(access.expression,
                                Some(tsr_ast::Expression::Identifier(r)) if r.text == "Symbol"))))
            })
        })
    }

    /// §495: whether iterating this type DECIDABLY fails — upstream reports
    /// the not-iterable error and `checkIteratedTypeOrElementType` answers
    /// `anyType` (`checker.go:6103`), so the element is `any` and not a gap.
    ///
    /// Decidable means the failure is provable from the class declaration
    /// alone, every gate below erring toward the gap:
    /// - every declaration is a CLASS with **no heritage clause** (a base
    ///   class could supply the protocol this walk cannot see);
    /// - either the class has **no computed-name member at all** (so no
    ///   spelling of `[Symbol.iterator]`, aliased or otherwise, can hide),
    /// - or its `[Symbol.iterator]` method's every `return` is literally
    ///   `this` (the §284 shape) and the class has **no `next` method** —
    ///   an iterator method returning anything else may carry `next` on the
    ///   returned object, which this port cannot read and must keep gapping.
    pub(crate) fn iteration_decidably_fails(&self, iterated: TypeId) -> bool {
        let crate::types::TypeData::Named { members: Some(symbol), .. } =
            self.store.get(iterated).data
        else {
            return false;
        };
        let declarations = self.binder.symbols().get(symbol).declarations.to_vec();
        if declarations.is_empty() {
            return false;
        }
        declarations.iter().all(|&declaration| {
            let (members, heritage) = match self.node_map.get(declaration) {
                Some(Node::ClassDeclaration(class)) => (class.members, class.heritage_clauses),
                Some(Node::ClassExpression(class)) => (class.members, class.heritage_clauses),
                _ => return false,
            };
            if !heritage.is_empty() {
                return false;
            }
            let computed_members = members
                .iter()
                .filter(|member| {
                    matches!(member,
                        tsr_ast::ClassElement::MethodDeclaration(method)
                            if matches!(method.name, tsr_ast::PropertyName::ComputedPropertyName(_)))
                        || matches!(member,
                            tsr_ast::ClassElement::PropertyDeclaration(property)
                                if matches!(property.name, tsr_ast::PropertyName::ComputedPropertyName(_)))
                        || matches!(member,
                            tsr_ast::ClassElement::GetAccessorDeclaration(accessor)
                                if matches!(accessor.name, tsr_ast::PropertyName::ComputedPropertyName(_)))
                })
                .count();
            if computed_members == 0 {
                return true;
            }
            // The only computed members must be the `[Symbol.iterator]`
            // method itself, its body all `return this;`, and `next` absent.
            if !self.declares_symbol_iterator(iterated) {
                return false;
            }
            let iterator_is_the_only_computed = computed_members == 1;
            let next_missing = !members.iter().any(|member| {
                matches!(member, tsr_ast::ClassElement::MethodDeclaration(method)
                    if matches!(method.name, tsr_ast::PropertyName::Identifier(name)
                        if name.text == "next"))
            });
            iterator_is_the_only_computed
                && next_missing
                && members.iter().all(|member| match member {
                    tsr_ast::ClassElement::MethodDeclaration(method)
                        if matches!(
                            method.name,
                            tsr_ast::PropertyName::ComputedPropertyName(_)
                        ) =>
                    {
                        Self::method_returns_only_this(method)
                    }
                    _ => true,
                })
        })
    }

    /// §495: every `return` statement in the method's (non-nested) body is
    /// literally `return this;`, and there is at least one.
    fn method_returns_only_this(method: &tsr_ast::MethodDeclaration<'_>) -> bool {
        let Some(tsr_ast::FunctionBody::Block(block)) = method.body else { return false };
        let mut saw_return = false;
        for statement in block.statements {
            if let tsr_ast::Statement::ReturnStatement(ret) = statement {
                saw_return = true;
                if !matches!(ret.expression, Some(tsr_ast::Expression::KeywordExpression(keyword))
                    if keyword.kind == SyntaxKind::ThisKeyword)
                {
                    return false;
                }
            }
        }
        saw_return
    }

    fn get_type_for_variable_like_declaration(&mut self, declaration: NodeId) -> Option<TypeId> {
        // §38.1, MOVED FIRST at §409: upstream's ForIn arm opens
        // `getTypeForVariableLikeDeclaration` (checker.go:16652), before the
        // annotation, so `for (var a: number in X)` still reads `a : string`
        // (`parserForInStatement5`; the annotation is the reported error,
        // not the type). The arm answers `string` or `Extract<keyof T, string>`.
        if self.nodes.kind(declaration) == SyntaxKind::VariableDeclaration
            && let Some(list) = self.nodes.parent(declaration)
            && let Some(statement) = self.nodes.parent(list)
            && self.nodes.kind(statement) == SyntaxKind::ForInStatement
        {
            return Some(self.for_in_variable_type(statement));
        }
        // §441: an UNANNOTATED setter parameter reads the accessor PAIR's
        // type — the getter's return, through the same getTypeOfAccessors
        // road the member display uses ('set x(val)' beside
        // 'get x() { return 1 }' types 'val : number',
        // `gettersAndSettersTypesAgree`). Annotated parameters never reach
        // this (the annotation arm below wins); a pair whose type does not
        // compute keeps the gap.
        if self.nodes.kind(declaration) == SyntaxKind::Parameter
            && self.type_annotation_of(declaration).is_none()
            // NOT in JS: a JS setter's parameter reads JSDoc/contextual
            // machinery this arm does not model, and the ungated draft broke
            // a PASSING case (`declarationEmitClassAccessorsJs1`, 4 R->W all
            // in .js files) — the revert rule, honoured by the gate.
            && !self.in_js_file(declaration)
            && let Some(setter) = self.nodes.parent(declaration)
            && self.nodes.kind(setter) == SyntaxKind::SetAccessor
            && let Some(symbol) = self.binder.symbol_of(setter)
        {
            let paired = self.get_type_of_symbol(symbol);
            if paired != self.intrinsics.error {
                return Some(paired);
            }
            return None;
        }
        // §38, MOVED FIRST at §419 beside its for-in twin: the for-OF arm
        // also precedes the annotation upstream — the annotation is TS2483's
        // error, never the type, so `for (var a: number of X)` with an
        // unresolvable X reads `a : any` (`parserForOfStatement5/7`). An
        // element this port cannot decide FALLS THROUGH (upstream computed a
        // real element there; a confident `any` would be a wrong answer).
        if self.nodes.kind(declaration) == SyntaxKind::VariableDeclaration
            && let Some(list) = self.nodes.parent(declaration)
            && let Some(statement) = self.nodes.parent(list)
            && self.nodes.kind(statement) == SyntaxKind::ForOfStatement
            && let Some(Node::ForInOrOfStatement(for_of)) = self.node_map.get(statement)
            && let Some(expression) = for_of.expression
        {
            let is_async = for_of.await_modifier.is_some();
            if let Some(element) = self.for_of_statement_element_type(expression, is_async) {
                let widened = self.get_widened_literal_type(element);
                return Some(widened);
            }
            // An undecidable element SWALLOWS the declaration — neither the
            // annotation nor an initializer supplies a for-of binding's type
            // upstream (`for (var a = 1 of X)` reads `a : any`,
            // `parserForOfStatement4`), so the implicit-any road answers.
            return None;
        }
        // An annotation wins over an initialiser, always.
        if let Some(annotation) = self.type_annotation_of(declaration) {
            let declared = self.get_type_from_type_node(annotation);
            // `addOptionalityEx(declaredType, isProperty, isOptional)`
            // (`checker.go:16695`): `p?: string` declares `string | undefined`.
            // See `crate::optionality`.
            return Some(self.add_optionality_for_declaration(declared, declaration));
        }
        // §273's exposure repair: a JS variable's `@type` tag IS its
        // annotation (`getEffectiveTypeAnnotationNode`'s JSDoc arm). A tag
        // type that does not compute keeps the road below, as the `@param`
        // twin does.
        if let Some(annotation) = self.jsdoc_type_annotation(declaration) {
            let declared = self.get_type_from_type_node(annotation);
            if declared != self.intrinsics.error {
                return Some(self.add_optionality_for_declaration(declared, declaration));
            }
        }
        // §269: an unannotated JS parameter reads its `@param` type — the
        // SYMBOL-line half of §110 slice 2, which had only ever fed the
        // signature: `function f(foo) {}` under `@param {Foo} foo` printed
        // `f : (foo: Foo) => void` beside `foo : any` on the very next line.
        // Upstream has no second road — `getEffectiveTypeAnnotationNode`
        // answers the reparsed JSDoc type for BOTH consumers. A doc type that
        // does not compute keeps the implicit any, as the signature half does.
        if self.nodes.kind(declaration) == SyntaxKind::Parameter
            && let Some((annotation, bracketed)) = self.jsdoc_parameter_annotation(declaration)
        {
            let mut declared = self.get_type_from_type_node(annotation);
            if declared != self.intrinsics.error {
                // Native reparses bracket names as question tokens, and {T=}
                // supplies explicit undefined. Both affect the symbol/write
                // type; an actual initializer only changes its entry flow.
                let suffix_optional = matches!(annotation,
                    TypeNode::JSDocTypeExpression(expression)
                        if matches!(expression.r#type, Some(TypeNode::JSDocOptionalType(_))));
                if self.strict_null_checks && (bracketed || suffix_optional) {
                    declared = self.get_optional_type(declared, false);
                }
                return Some(self.add_optionality_for_declaration(declared, declaration));
            }
        }
        // `getParameterTypeOfFullSignature` (`checker.go:16732`), before the
        // contextual type: a JS function's `@type` tag types its parameters.
        if self.nodes.kind(declaration) == SyntaxKind::Parameter
            && let Some(full) = self.jsdoc_full_signature_parameter_type(declaration)
        {
            return Some(full);
        }
        // "Use contextual parameter type if one is available" (`checker.go:16735`),
        // which upstream places inside the `isParameter` block **before** the
        // initialiser path below — a contextually typed parameter takes its type
        // from the context even when it has a default. See [`crate::contextual`]
        // for which contexts this port can answer; `None` from it leaves the
        // behaviour exactly as it was, ending in the implicit `any`.
        if self.nodes.kind(declaration) == SyntaxKind::Parameter
            && let Some(contextual) = self.get_contextually_typed_parameter_type(declaration)
        {
            return Some(self.add_optionality_for_declaration(contextual, declaration));
        }
        let initializer = self.initializer_of(declaration)?;

        // `const data = [];` — upstream types the **symbol** `any[]` and leaves
        // the **literal** at `never[]`, on the same declaration:
        //
        // ```
        // >data : any[]
        // >[] : never[]
        // ```
        //
        // `getTypeForVariableLikeDeclaration` (`checker.go:16652`) returns
        // `c.autoArrayType` at `checker.go:16709`, *before* it ever consults the
        // initialiser's type. `autoArrayType` is `createArrayType(autoType)`
        // (`checker.go:1360`), which prints `any[]`.
        //
        // # The placement is the whole design, and the wrong one is invisible
        //
        // This must type the **symbol**, never the literal.
        // `check_array_literal` (`array_literals.rs`) correctly answers
        // `never[]` for `[]` (`checker.go:8098`) and **51 currently-right
        // `>[] : never[]` lines depend on it staying that way**. A fix that
        // reached into the literal instead would read identically and break all
        // 51 — which is why the falsifier here is a count rather than a
        // description: `ArrayLiteralExpression` must move by **exactly zero**.
        // `nameres_evolving_array_declaration.rs` pins it, and `bd tsr-5h0`
        // carries the sizing.
        //
        // # Which of upstream's guards are ported
        //
        // Upstream's condition (`checker.go:16696`) is `noImplicitAny &&
        // IsVariableDeclaration && !IsBindingPattern(name) && no export modifier
        // && not ambient`. The three syntactic guards are ported below.
        //
        // **This comment used to continue "`noImplicitAny` is *not* [ported]:
        // this port models no compiler options and assumes strict throughout".
        // That is stale** — ADR-0042 gave the checker real compiler options and
        // `Checker::no_implicit_any` has been a wired field since. §599 gates
        // the sibling arm below on it, and the gate is load-bearing:
        // `initializersWidened` writes `// @noImplicitAny: false` and expects
        // `var x1 = null` to keep `null`. Ungated, that arm cost a PASSING case.
        //
        // The empty-array arm here is left ungated because changing it is a
        // separate measurement, not because the option is unavailable.
        if self.nodes.kind(declaration) == SyntaxKind::VariableDeclaration
            && !self.has_binding_pattern_name(declaration)
            && !self.is_exported_variable(declaration)
            && !self.combined_node_flags(declaration).intersects(NodeFlags::AMBIENT)
            && is_empty_array_literal(initializer)
            && let Some(target) = self.global_type_symbol("Array")
        {
            let any = self.intrinsics.any;
            return Some(self.create_type_reference(target, vec![any]));
        }

        // §599: the arm BESIDE the empty-array one, under the same four guards
        // (`checker.go:16697`–`:16704`). A non-`const` variable whose
        // initialiser is `null` or `undefined` takes the **control-flow tracked
        // `any`** — upstream's `autoType`, which prints `any`:
        //
        // ```ts
        // var a = null;          // variableDeclarationInnerCommentEmit — `a : any`
        // ```
        //
        // `autoType` and `autoArrayType` are siblings in one `if`; this port
        // ported the array half and not the scalar one, so the guards were
        // already here and only the arm was missing.
        //
        // **The `const` test is upstream's and load-bearing**: `const x = null`
        // is NOT auto-typed and keeps `null`, because a `const` can never be
        // reassigned and so has nothing to evolve into.
        //
        // §598 recorded this population as blocked by the `strictNullChecks`
        // default and **that diagnosis was wrong** — the gate upstream writes
        // is `noImplicitAny`, which this port assumes on (the comment above
        // says so), not `strictNullChecks`. The record is corrected in
        // STATUS §5.
        if self.no_implicit_any
            && self.nodes.kind(declaration) == SyntaxKind::VariableDeclaration
            && !self.has_binding_pattern_name(declaration)
            && !self.is_exported_variable(declaration)
            && !self.combined_node_flags(declaration).intersects(NodeFlags::AMBIENT)
            && !self.combined_node_flags(declaration).intersects(NodeFlags::CONSTANT)
            && self.is_null_or_undefined_expression(initializer)
        {
            return Some(self.intrinsics.any);
        }

        let initializer_type = self.check_expression(initializer);
        let widened = self.widen_type_inferred_from_initializer(declaration, initializer_type);
        // §96 (`checker-notes-narrow.md`): upstream wraps the initializer
        // branch too (`checker.go:16750`) — `(b? = 0)` is `number | undefined`.
        // A written `?` is required (`is_optional_declaration`), so plain
        // defaulted parameters stay bare.
        Some(self.add_optionality_for_declaration(widened, declaration))
    }

    /// Ported from `Checker.getWidenedLiteralTypeForInitializer`
    /// (`checker.go:16897`).
    ///
    /// This is the rule behind the most frequently surprising line in a `.types`
    /// baseline: `const x = "a"` is `"a"` and `let x = "a"` is `string`, from the
    /// same initialiser expression. A `const` keeps the literal; anything else
    /// widens it.
    /// `isNullOrUndefined` (`checker.go:17667`) — the `null` keyword, or an
    /// identifier that resolves to the global `undefined`.
    ///
    /// The identifier half really does need RESOLUTION and not a text match:
    /// `var undefined = null; var x = undefined;` binds a local, and upstream's
    /// test is `getResolvedSymbol(expr) == c.undefinedSymbol`. A name check
    /// would auto-type `x` off a shadowing local
    /// (`undefinedTypeAssignment3` writes exactly that shadow).
    /// §738 made this `pub(crate)`: the FLOW-side auto predicate
    /// ([`Checker::is_auto_typed_declaration`]) needs the same test this
    /// declared-type mint uses, because upstream writes it once
    /// (`checker.go:16702`) and both roads read it.
    pub(crate) fn is_null_or_undefined_expression(
        &mut self,
        expression: tsr_ast::Expression<'a>,
    ) -> bool {
        let Some(mut id) = expression.node_id() else { return false };
        // `ast.SkipParentheses`.
        while self.nodes.kind(id) == SyntaxKind::ParenthesizedExpression {
            let Some(Node::ParenthesizedExpression(paren)) = self.node_map.get(id) else {
                return false;
            };
            let Some(inner) = paren.expression.and_then(|inner| inner.node_id()) else {
                return false;
            };
            id = inner;
        }
        match self.nodes.kind(id) {
            SyntaxKind::NullKeyword => true,
            SyntaxKind::Identifier => {
                let Some(Node::Identifier(name)) = self.node_map.get(id) else { return false };
                name.text == "undefined"
                    && self
                        .binder
                        .resolve_name(
                            self.nodes,
                            self.node_map,
                            id,
                            "undefined",
                            SymbolFlags::VALUE,
                        )
                        .is_none()
            }
            _ => false,
        }
    }

    /// `widenTypeInferredFromInitializer` (checker.go): JavaScript empty
    /// literal/array inference recovers to any/any[] after literal widening.
    pub(crate) fn widen_type_inferred_from_initializer(
        &mut self,
        declaration: NodeId,
        id: TypeId,
    ) -> TypeId {
        let widened = self.get_widened_literal_type_for_initializer(declaration, id);
        if self.in_js_file(declaration) {
            if self.is_empty_literal_type(widened) {
                return self.intrinsics.any;
            }
            if self.is_empty_array_literal_type(widened) {
                let Some(array) = self.global_type_symbol("Array") else {
                    return self.intrinsics.error;
                };
                return self.create_type_reference(array, vec![self.intrinsics.any]);
            }
        }
        widened
    }

    pub(crate) fn get_widened_literal_type_for_initializer(
        &mut self,
        declaration: NodeId,
        id: TypeId,
    ) -> TypeId {
        // §62 (`checker-notes-narrow.md`): a UNIQUE symbol belongs to its
        // OWN declaration — only a direct `Symbol()`/`Symbol.for()` call
        // initializer keeps uniqueness; a COPY widens to `symbol` even
        // under const (`uniqueSymbols`' constInitToC* family, 87 lines).
        if self.store.get(id).flags.contains(crate::flags::TypeFlags::UNIQUE_ES_SYMBOL)
            // TS only (`uniqueSymbolJs2`'s JS declaration roads differ), and
            // a BINDING-PATTERN holder keeps its member's uniqueness
            // (`uniqueSymbols` pos 508).
            && !self.in_js_file(declaration)
            && !matches!(
                self.node_map.get(declaration),
                Some(Node::VariableDeclaration(v))
                    if matches!(v.name, Some(tsr_ast::BindingName::BindingPattern(_)))
            )
        {
            let direct_symbol_call = self
                .initializer_of(declaration)
                .and_then(|initializer| match initializer {
                    tsr_ast::Expression::CallExpression(call) => call.expression,
                    _ => None,
                })
                .is_some_and(|callee| match callee {
                    tsr_ast::Expression::Identifier(name) => name.text == "Symbol",
                    tsr_ast::Expression::PropertyAccessExpression(access) => matches!(
                        access.expression,
                        Some(tsr_ast::Expression::Identifier(receiver))
                            if receiver.text == "Symbol"
                    ),
                    _ => false,
                });
            if !direct_symbol_call {
                return self.intrinsics.es_symbol;
            }
        }
        if self.combined_node_flags(declaration).intersects(NodeFlags::CONSTANT) {
            return id;
        }
        // §695: `getWidenedLiteralTypeForInitializer` keeps the literal for a
        // CONST **or a READONLY** declaration (`checker.go:16898`), and only the
        // const half was ported. `class C { readonly type = E.A; }` records
        // `type : E.A` (`declarationEmitEnumReadonlyProperty`); widening gave
        // the enum's own `E`.
        //
        // `isDeclarationReadonly` (`checker/utilities.go:828`) excludes a
        // PARAMETER PROPERTY. `Checker::declaration_is_readonly` already answers
        // only for a `PropertyDeclaration`, so a parameter property — which is a
        // `ParameterDeclaration` — is excluded by construction rather than by a
        // second test.
        if self.declaration_is_readonly(declaration) {
            return id;
        }
        // §134 iteration 2: the UNION arm of `getWidenedLiteralType`
        // (checker.go:25499, `mapType`) applied at THIS road only — the
        // whole-function form measured 45:80 inverted (return-inference
        // and array-literal consumers keep their union literals; the
        // initializer is where `1 | T` declares as `number | T`).
        let widened = if let crate::types::TypeData::Union { types, .. } = &self.store.get(id).data
        {
            let constituents = types.clone();
            let mapped: Vec<_> =
                constituents.iter().map(|&c| self.get_widened_literal_type(c)).collect();
            if mapped == constituents { id } else { self.get_union_type(&mapped) }
        } else {
            self.get_widened_literal_type(id)
        };
        // §65 (keyed on the decoded axis): under `noImplicitAny` the
        // `undefined`-identifier initializer widens to `any`
        // (`controlFlowNoImplicitAny`); without the flag it keeps
        // `undefined` (`implicitAnyCastedValue`).
        if self.no_implicit_any && self.nodes.kind(declaration) == SyntaxKind::VariableDeclaration {
            // VARIABLES only — a class PROPERTY `foo = undefined` keeps
            // `undefined` (`implicitAnyCastedValue`, itself @noImplicitAny).
            let undefined_identifier = widened == self.intrinsics.undefined
                && matches!(
                    self.initializer_of(declaration),
                    Some(tsr_ast::Expression::Identifier(name)) if name.text == "undefined"
                );
            // §65.1's null twin REFUSED at 2.4:1: `var arg0 = null` keeps
            // `null` (`implicitAnyFunctionInvocationWithAnyArguements`)
            // beside null-wants-any lines in `controlFlowNoImplicitAny`,
            // and the any-typed null broke the evolving-array detection
            // (`controlFlowArrays` 3 G→W) — a fourth key exists and is not
            // yet decoded.
            if undefined_identifier {
                return self.intrinsics.any;
            }
        }
        // `getWidenedTypeWithContext`'s nullable arm (`checker.go:18368`):
        // a purely nullable WIDENING type is `any`. The widening twins exist
        // only with `strictNullChecks` OFF — strict `let x = null` keeps
        // `null` (`initializersWidened`, the §20 bar's fired leg: 69 R→W
        // before this gate). See `checker-notes-narrow.md` §20.
        if self.strict_null_checks {
            return widened;
        }
        let flags = self.store.get(widened).flags;
        if flags.intersects(crate::flags::TypeFlags::NULLABLE)
            && !flags.intersects(!crate::flags::TypeFlags::NULLABLE)
        {
            return self.intrinsics.any;
        }
        if let crate::types::TypeData::Union { types, .. } = &self.store.get(widened).data
            && types
                .iter()
                .all(|&t| self.store.get(t).flags.intersects(crate::flags::TypeFlags::NULLABLE))
        {
            return self.intrinsics.any;
        }
        widened
    }

    /// §175: the GET accessor declared beside a SET accessor — same
    /// container, same property name. `GetDeclarationOfKind(symbol,
    /// KindGetAccessor)` (`checker.go:18517`) reaches it through the shared
    /// symbol; this walks the container's members, which is the same set.
    pub(crate) fn paired_get_accessor(&self, setter: NodeId) -> Option<NodeId> {
        let name = match self.node_map.get(setter)? {
            Node::SetAccessorDeclaration(node) => match node.name {
                tsr_ast::PropertyName::Identifier(name) => name.text,
                tsr_ast::PropertyName::StringLiteral(name) => name.text,
                _ => return None,
            },
            _ => return None,
        };
        let container = self.nodes.parent(setter)?;
        let members: &[tsr_ast::ClassElement<'_>] = match self.node_map.get(container)? {
            Node::ClassDeclaration(class) => class.members,
            Node::ClassExpression(class) => class.members,
            _ => return None,
        };
        members.iter().find_map(|member| match member {
            tsr_ast::ClassElement::GetAccessorDeclaration(getter) => {
                let getter_name = match getter.name {
                    tsr_ast::PropertyName::Identifier(n) => n.text,
                    tsr_ast::PropertyName::StringLiteral(n) => n.text,
                    _ => return None,
                };
                (getter_name == name).then_some(getter.node_id).flatten()
            }
            _ => None,
        })
    }

    /// §269: the `@param` type expression an unannotated JS parameter reads —
    /// the doc-host walk `crate::signatures` makes for the signature half,
    /// repeated here for the parameter's own symbol (upstream needs only one
    /// road because `getEffectiveTypeAnnotationNode` answers the reparsed
    /// JSDoc type to every consumer).
    /// The second component preserves the reparsed bracket question token.
    pub(crate) fn jsdoc_parameter_annotation(
        &self,
        parameter: NodeId,
    ) -> Option<(TypeNode<'a>, bool)> {
        if !self.in_js_file(parameter) {
            return None;
        }
        let Some(Node::ParameterDeclaration(node)) = self.node_map.get(parameter) else {
            return None;
        };
        let Some(tsr_ast::BindingName::Identifier(identifier)) = node.name else { return None };
        let name = identifier.text;
        let function = self.nodes.parent(parameter)?;
        let mut hosts = vec![function];
        let mut current = self.nodes.parent(function);
        for _ in 0..4 {
            let Some(id) = current else { break };
            match self.nodes.kind(id) {
                SyntaxKind::VariableDeclaration
                | SyntaxKind::VariableDeclarationList
                | SyntaxKind::VariableStatement
                | SyntaxKind::PropertyAssignment
                | SyntaxKind::PropertyDeclaration
                | SyntaxKind::ExpressionStatement
                | SyntaxKind::ParenthesizedExpression
                | SyntaxKind::ExportAssignment
                | SyntaxKind::BinaryExpression => {
                    hosts.push(id);
                    current = self.nodes.parent(id);
                }
                _ => break,
            }
        }
        // An `@overload`-documented implementation is a different regime: its
        // parameters aggregate the UNION of the overload signatures' types
        // (`overloadTag1` wants `a : string | number`), which the existing
        // signature machinery already answers. A first-match read here
        // overrode it with the first overload's slot — 11 lines RIGHT→WRONG
        // on the first draft's pair, so the whole road declines when any doc
        // in scope carries an `@overload`.
        for host in &hosts {
            if let Some(docs) = self.jsdoc_entries.get(host)
                && docs.iter().any(|doc| {
                    doc.tags.iter().any(|tag| matches!(tag, tsr_ast::JSDocTag::JSDocOverloadTag(_)))
                })
            {
                return None;
            }
        }
        for host in hosts {
            let Some(docs) = self.jsdoc_entries.get(&host) else { continue };
            for doc in *docs {
                for tag in doc.tags {
                    if let tsr_ast::JSDocTag::JSDocParameterOrPropertyTag(tag) = tag
                        && matches!(tag.tag_name.text, "param" | "parameter" | "arg" | "argument")
                        && matches!(tag.name, Some(tsr_ast::EntityName::Identifier(n)) if n.text == name)
                    {
                        return tag
                            .type_expression
                            .map(|annotation| (annotation, tag.is_bracketed));
                    }
                }
            }
        }
        None
    }

    /// §273's exposure, repaired: the `@type` tag an unannotated JS variable
    /// reads — `/** @type {Map<string, V>} */ const cache = new Map()` takes
    /// the tag's type, not the initialiser's. Upstream is
    /// `getEffectiveTypeAnnotationNode`'s JSDoc arm again, the same door the
    /// `@param` road above went through. Direct hosted tags precede statement
    /// tags; statement tags each select the first still-untyped declaration.
    pub(crate) fn jsdoc_type_annotation(&self, declaration: NodeId) -> Option<TypeNode<'a>> {
        if !self.in_js_file(declaration) {
            return None;
        }
        if self.nodes.kind(declaration) != SyntaxKind::VariableDeclaration {
            return None;
        }
        let list_id = self.nodes.parent(declaration)?;
        let current = self.nodes.parent(list_id)?;
        let tag_type = |tag: &tsr_ast::JSDocTag<'a>| {
            if let tsr_ast::JSDocTag::JSDocTypeTag(tag) = tag
                && let Some(Node::JSDocTypeExpression(expression)) = tag.type_expression
            {
                expression.r#type
            } else {
                None
            }
        };
        // Retain the old reader for unrepresented/mixed or incomplete trees.
        // This is not a native all-document exclusion policy or absence proof.
        let legacy = || {
            if self.nodes.kind(current) != SyntaxKind::VariableStatement {
                return None;
            }
            for doc in *self.jsdoc_entries.get(&current)? {
                // Native's `reparseHosted` types the declaration whatever
                // else the comment declares; only the typedef exclusion is
                // retained here (a parsed `@callback` used to be an unknown
                // tag this reader never skipped).
                if doc.tags.iter().any(|tag| matches!(tag, tsr_ast::JSDocTag::JSDocTypedefTag(_))) {
                    continue;
                }
                for tag in doc.tags {
                    if let tsr_ast::JSDocTag::JSDocTypeTag(tag) = tag
                        && let Some(Node::JSDocTypeExpression(expression)) = tag.type_expression
                    {
                        return expression.r#type;
                    }
                }
            }
            None
        };
        let hosted_tags = |host| {
            let docs = self.jsdoc_entries.get(&host).copied().unwrap_or(&[]);
            if docs.iter().flat_map(|doc| doc.tags).any(|tag| {
                matches!(
                    tag,
                    tsr_ast::JSDocTag::JSDocTypedefTag(_)
                        | tsr_ast::JSDocTag::JSDocCallbackTag(_)
                        | tsr_ast::JSDocTag::JSDocUnknownTag(_)
                ) || (matches!(tag, tsr_ast::JSDocTag::JSDocTypeTag(_)) && tag_type(tag).is_none())
                    // `missing_type` recovers an omitted type as a source-less
                    // AnyKeyword. It is not a complete hosted annotation.
                    || matches!(tag_type(tag), Some(TypeNode::KeywordTypeNode(node))
                        if node.kind == SyntaxKind::AnyKeyword
                            && node.node_id.is_none_or(|id| {
                                let span = self.nodes.span(id);
                                span.start >= span.end
                            }))
            }) {
                None
            } else {
                Some(docs.last().map_or(&[][..], |doc| doc.tags))
            }
        };
        if !matches!(
            self.nodes.kind(current),
            SyntaxKind::VariableStatement | SyntaxKind::ForStatement
        ) {
            return legacy();
        }
        let Some(Node::VariableDeclarationList(list)) = self.node_map.get(list_id) else {
            return legacy();
        };
        let statement_tags = if self.nodes.kind(current) == SyntaxKind::VariableStatement {
            hosted_tags(current)
        } else {
            Some(&[][..])
        };
        let Some(statement_tags) = statement_tags else { return legacy() };
        let mut direct = Vec::with_capacity(list.declarations.len());
        for variable in list.declarations {
            if !matches!(variable.name, Some(tsr_ast::BindingName::Identifier(_))) {
                return legacy();
            }
            let Some(tags) = hosted_tags(variable.node_id?) else { return legacy() };
            direct.push(tags.iter().find_map(tag_type));
        }
        let index =
            list.declarations.iter().position(|variable| variable.node_id == Some(declaration))?;
        if let Some(annotation) = direct[index] {
            return Some(annotation);
        }
        let mut types = statement_tags.iter().filter_map(tag_type);
        for (variable, direct) in list.declarations.iter().zip(direct) {
            if variable.r#type.is_some() || direct.is_some() {
                continue;
            }
            let annotation = types.next()?;
            if variable.node_id == Some(declaration) {
                return Some(annotation);
            }
        }
        None
    }

    /// §275: the `@type` tag hanging directly off a node — the JSDoc CAST's
    /// read, `isJSDocTypeAssertion`'s tag half.
    pub(crate) fn jsdoc_cast_annotation(&self, id: NodeId) -> Option<TypeNode<'a>> {
        let docs = self.jsdoc_entries.get(&id)?;
        for doc in *docs {
            for tag in doc.tags {
                if let tsr_ast::JSDocTag::JSDocTypeTag(tag) = tag
                    && let Some(Node::JSDocTypeExpression(expression)) = tag.type_expression
                {
                    return expression.r#type;
                }
            }
        }
        None
    }

    /// The type annotation of a declaration, if it has one.
    pub(crate) fn type_annotation_of(&self, declaration: NodeId) -> Option<TypeNode<'a>> {
        match self.node_map.get(declaration)? {
            Node::VariableDeclaration(node) => node.r#type,
            Node::ParameterDeclaration(node) => node.r#type,
            Node::PropertyDeclaration(node) => node.r#type,
            Node::PropertySignatureDeclaration(node) => node.r#type,
            Node::JSDocParameterOrPropertyTag(node)
                if self.nodes.kind(declaration) == SyntaxKind::JSDocPropertyTag =>
            {
                node.type_expression
            }
            _ => None,
        }
    }

    /// Whether a variable declaration carries `export`.
    ///
    /// Upstream's `getCombinedModifierFlagsCached(declaration) & ModifierFlagsExport`
    /// (`checker.go:16698`). The modifier is not on the declaration: it is on
    /// the enclosing `VariableStatement`, with the `VariableDeclarationList`
    /// transparent between them — the same walk `combined_node_flags` makes for
    /// `const`, and the same one `Binder::has_export_modifier`
    /// (`crates/tsr-binder/src/binder.rs:2548`) makes on its ancestor stack.
    fn is_exported_variable(&self, declaration: NodeId) -> bool {
        let mut current = self.nodes.parent(declaration);
        for _ in 0..2 {
            let Some(node) = current else { return false };
            if let Some(Node::VariableStatement(statement)) = self.node_map.get(node) {
                return statement.modifiers.iter().any(|modifier| {
                    matches!(modifier, tsr_ast::ModifierLike::Token(token)
                        if token.kind == SyntaxKind::ExportKeyword)
                });
            }
            current = self.nodes.parent(node);
        }
        false
    }

    /// Whether a declaration's name is a binding pattern rather than an
    /// identifier — `const [a] = []`. Upstream's `!ast.IsBindingPattern(name)`
    /// guard (`checker.go:16697`): a destructuring declaration takes its type
    /// from the pattern, not from the initialiser.
    fn has_binding_pattern_name(&self, declaration: NodeId) -> bool {
        matches!(
            self.node_map.get(declaration),
            Some(Node::VariableDeclaration(node))
                if !matches!(node.name, Some(tsr_ast::BindingName::Identifier(_)))
        )
    }

    /// The initialiser of a declaration, if it has one.
    pub(crate) fn initializer_of(&self, declaration: NodeId) -> Option<Expression<'a>> {
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
pub(crate) fn is_identifier_text(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else { return false };
    (first.is_ascii_alphabetic() || first == '_' || first == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// Whether an expression is `[]` — an array literal with no elements.
///
/// Ported from `isEmptyArrayLiteral` (`internal/checker/utilities.go`). A
/// separate function because the *emptiness* is the whole trigger: `[1]` takes
/// the ordinary initialiser path and must keep doing so.
fn is_empty_array_literal(expression: Expression<'_>) -> bool {
    matches!(expression, Expression::ArrayLiteralExpression(literal) if literal.elements.is_empty())
}

#[cfg(test)]
mod tests {
    use super::Checker;
    use crate::types::{TypeData, TypeId};
    use tsr_ast::{Node, NodeId};

    #[test]
    fn semantic_type_naming_targets_preserve_identity_and_decline_incomplete_routes() {
        use crate::resolution::{ModuleHost, PropertyName};
        use tsr_binder::SymbolFlags;

        struct DeclarationHost;
        impl ModuleHost for DeclarationHost {
            fn resolved_module(&self, _: NodeId, _: &str) -> Option<NodeId> {
                None
            }
            fn module_resolution_found(&self, _: NodeId, _: &str) -> bool {
                false
            }
            fn is_declaration_file(&self, _: NodeId) -> bool {
                true
            }
        }
        let source = r#"
            declare module "pure" {
                namespace Original {
                    interface Pair<A, B> { left: A; right: B }
                    namespace Inner { import Local = Original; }
                }
                export = Original;
            }
            declare module "mixed" {
                function Callable(value: number): number;
                namespace Callable { interface Pair<A, B> { left: B; right: A } }
                export = Callable;
            }
            declare module "star" { export * from "pure"; }
            namespace Consumer {
                import React from "pure";
                import Unsupported from "mixed";
                import Star from "star";
                import Cycle = Loop;
                import Loop = Cycle;
            }
        "#;
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "control.d.ts", text: source },
        );
        let aliases: std::collections::BTreeMap<_, _> = bound
            .symbols()
            .iter()
            .filter(|(_, symbol)| symbol.flags.contains(SymbolFlags::ALIAS))
            .map(|(id, symbol)| (symbol.name, id))
            .collect();
        let mut checker = Checker::with_module_host(
            &bound,
            &parsed.nodes,
            &parsed.node_map,
            Some(&DeclarationHost),
        );
        checker.apply_compiler_options(&tsr_core::CompilerOptions {
            module: tsr_core::ModuleKind::CommonJS,
            ..Default::default()
        });
        let original = checker
            .resolve_alias(
                bound.symbols().get(bound.ambient_module("pure").unwrap()).exports["export="],
            )
            .unwrap();
        let pair = bound.symbols().get(original).exports["Pair"];
        let declarations = bound.symbols().get(pair).declarations.clone();
        let flags = bound.symbols().get(pair).flags;
        let publications = (
            checker.computations,
            checker.node_types.len(),
            checker.symbol_types.len(),
            checker.declared_types.len(),
            checker.qualified_reference_types.len(),
            checker.qualified_generic_reference_types.len(),
        );
        assert_eq!(checker.resolve_alias(aliases["React"]), None);
        for _ in 0..2 {
            let mut path = vec![pair];
            assert_eq!(
                checker.semantic_type_naming_alias_target(
                    aliases["React"],
                    SymbolFlags::NAMESPACE,
                    &mut path
                ),
                Some(original)
            );
            assert_eq!(path, vec![pair], "caller owns the traversal path");
            for alias in ["Unsupported", "Star", "Cycle", "Loop"] {
                let mut path = Vec::new();
                assert_eq!(
                    checker.semantic_type_naming_alias_target(
                        aliases[alias],
                        SymbolFlags::NAMESPACE,
                        &mut path
                    ),
                    None,
                    "{alias}"
                );
                assert!(path.is_empty());
            }
            let mut path = vec![original];
            assert_eq!(
                checker.semantic_type_naming_alias_target(
                    original,
                    SymbolFlags::NAMESPACE,
                    &mut path
                ),
                None
            );
            assert_eq!(path, vec![original]);
            for property in [PropertyName::Type, PropertyName::DeclaredType] {
                assert!(checker.resolutions.push(original, property));
                assert_eq!(
                    checker.semantic_type_naming_alias_target(
                        aliases["React"],
                        SymbolFlags::NAMESPACE,
                        &mut Vec::new()
                    ),
                    None
                );
                assert!(checker.resolutions.pop());
            }
            assert_eq!(
                checker.resolve_alias(aliases["React"]),
                None,
                "ordinary VALUE admission is unchanged"
            );
        }
        assert_eq!(bound.symbols().get(pair).declarations, declarations);
        assert_eq!(bound.symbols().get(pair).flags, flags);
        assert_eq!(
            (
                checker.computations,
                checker.node_types.len(),
                checker.symbol_types.len(),
                checker.declared_types.len(),
                checker.qualified_reference_types.len(),
                checker.qualified_generic_reference_types.len()
            ),
            publications,
            "reader publishes no type/reference image"
        );
        let mut no_host = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        assert_eq!(
            no_host.semantic_type_naming_alias_target(
                aliases["React"],
                SymbolFlags::NAMESPACE,
                &mut Vec::new()
            ),
            None
        );
        // Direct own identity beats aliases in that table; an already-admitted
        // alias in an inner table beats the outer direct identity, not its spelling.
        assert_eq!(checker.resolve_alias(aliases["Local"]), Some(original));
        let reference = checker.create_type_reference_public(
            pair,
            vec![checker.intrinsics.string, checker.intrinsics.number],
        );
        let direct = bound.symbols().get(pair).declarations[0];
        let inner = checker.declaration_of_alias_symbol(aliases["Local"]).unwrap();
        for reverse in [false, true] {
            let mut queries =
                [(direct, "Pair<string, number>"), (inner, "Local.Pair<string, number>")];
            if reverse {
                queries.reverse();
            }
            for _ in 0..2 {
                for (site, expected) in queries {
                    assert_eq!(
                        checker.type_to_string_at(reference, site).as_deref(),
                        Some(expected)
                    );
                }
            }
        }
    }

    #[test]
    fn commonjs_unknown_alias_requires_completed_any_receiver() {
        use crate::resolution::PropertyName;
        use tsr_ast::{Expression, NodeFlags};
        use tsr_binder::SymbolFlags;

        let source = "var receiver; exports.item = receiver.field;
                      exports.missing = notDeclared.field;
                      exports.cycle = exports.cycle;";
        let arena = tsr_core::Arena::new();
        let mut parsed = tsr_parser::parse(&arena, source);
        let root = parsed.source_file.node_id.unwrap();
        parsed.nodes.add_flags(root, NodeFlags::JAVASCRIPT_FILE);
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "control.js", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let module = bound.symbol_of(root).unwrap();
        let exports = &bound.symbols().get(module).exports;
        let item = exports["item"];
        let declaration = checker.declaration_of_alias_symbol(item).unwrap();
        let Some(Node::BinaryExpression(binary)) = parsed.node_map.get(declaration) else {
            panic!("assignment declaration");
        };
        let Some(Expression::PropertyAccessExpression(rhs)) = binary.right else {
            panic!("property RHS");
        };
        let receiver = rhs.expression.unwrap().node_id().unwrap();
        let owner = bound
            .resolve_name(&parsed.nodes, &parsed.node_map, receiver, "receiver", SymbolFlags::VALUE)
            .unwrap();
        let computations = checker.computations;
        assert!(!checker.commonjs_property_alias_has_unknown_target(item), "cold/missing cache");
        for ty in [checker.intrinsics.error, checker.intrinsics.empty_object] {
            checker.node_types.insert(receiver, ty);
            assert!(
                !checker.commonjs_property_alias_has_unknown_target(item),
                "error/typed receiver"
            );
        }
        checker.node_types.insert(receiver, checker.intrinsics.any);
        assert!(checker.commonjs_property_alias_has_unknown_target(item));
        assert!(checker.resolutions.push(owner, PropertyName::Type));
        assert!(!checker.commonjs_property_alias_has_unknown_target(item), "active owner");
        assert!(!checker.resolutions.push(owner, PropertyName::Type));
        assert!(!checker.commonjs_property_alias_has_unknown_target(item), "failed cycle");
        assert!(!checker.resolutions.pop());
        checker.flow_loop_stack.push(((0, 0, checker.intrinsics.any), Vec::new()));
        assert!(!checker.commonjs_property_alias_has_unknown_target(item), "provisional fixpoint");
        checker.flow_loop_stack.pop();
        assert!(checker.commonjs_property_alias_has_unknown_target(item));
        assert!(!checker.commonjs_property_alias_has_unknown_target(exports["missing"]));
        assert!(!checker.commonjs_property_alias_has_unknown_target(exports["cycle"]));
        assert_eq!(checker.computations, computations, "completion probe is read-only");
        assert_eq!(checker.resolutions.depth(), 0);
    }

    fn with_context(
        context: &str,
        strict: bool,
        exact: bool,
        run: impl FnOnce(&mut Checker<'_, '_>, TypeId, TypeId, TypeId),
    ) {
        let source = format!(
            "type V = {{ a: 'value' }}; type K = {{ b: 'index' }}; \
             type Read<T> = {{ readonly [P in keyof T]: T[P] }}; \
             type Part<T> = {{ [P in keyof T]?: T[P] }}; \
             type Need<T> = {{ [P in keyof T]-?: T[P] }}; type Target = {context};"
        );
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, &source);
        assert!(parsed.diagnostics.is_empty(), "{source}");
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "t.ts", text: &source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        checker.apply_compiler_options(&tsr_core::CompilerOptions {
            strict: tsr_core::Tristate::from_bool(strict),
            exact_optional_property_types: tsr_core::Tristate::from_bool(exact),
            ..Default::default()
        });
        let mut aliases = std::collections::BTreeMap::new();
        for index in 0..parsed.nodes.len() {
            let id = NodeId::new(u32::try_from(index).unwrap());
            if let Some(Node::TypeAliasDeclaration(alias)) = parsed.node_map.get(id)
                && let Some(symbol) = bound.symbol_of(id)
            {
                aliases.insert(alias.name.unwrap().text, symbol);
            }
        }
        let target = checker.get_declared_type_of_symbol(aliases["Target"]);
        let value = checker.get_declared_type_of_symbol(aliases["V"]);
        let index = checker.get_declared_type_of_symbol(aliases["K"]);
        run(&mut checker, target, value, index);
    }

    fn leaves(checker: &Checker<'_, '_>, ty: TypeId) -> Vec<TypeId> {
        let mut result = match &checker.store.get(ty).data {
            TypeData::Union { types, .. } => {
                types.iter().flat_map(|&part| leaves(checker, part)).collect()
            }
            _ => vec![ty],
        };
        result.sort_by_key(|ty| ty.index());
        result
    }

    #[test]
    fn concrete_context_removes_only_exact_optional_missing() {
        // Native getTypeOfConcretePropertyOfContextualType removes missing,
        // not undefined. Asymmetric V/K shapes expose accidental index fallback.
        for (strict, exact) in [(true, true), (true, false), (false, false)] {
            for (context, explicit_undefined, optional) in [
                ("{ x: V }", false, false),
                ("{ x?: V }", false, true),
                ("{ x?: V | undefined }", true, true),
                ("{ x: V | undefined }", true, false),
                ("{ [P in 'x']?: V }", false, true),
                ("{ [P in 'x']?: V | undefined }", true, true),
                ("Read<{ x?: V }>", false, true),
                ("Read<{ x?: V | undefined }>", true, true),
                ("{ x?: V } & { [s: string]: K }", false, true),
                ("{ [s: string]: K } & { x?: V }", false, true),
                ("{ x?: V | undefined } & { [s: string]: K }", true, true),
                ("{ [s: string]: K } & { x?: V | undefined }", true, true),
                ("{ [s: string]: V | undefined }", true, false),
            ] {
                with_context(context, strict, exact, |checker, target, value, _| {
                    let ordinary = checker.get_type_of_property_of_type(target, "x");
                    let mut expected = vec![value];
                    if strict && (explicit_undefined || (optional && !exact)) {
                        expected.push(checker.intrinsics.undefined);
                    }
                    expected.sort_by_key(|ty| ty.index());
                    for _ in 0..2 {
                        let actual = checker.contextual_property_type(target, "x").unwrap();
                        println!(
                            "CONCRETE_CONTEXT {context} strict={strict} exact={exact} \
                             ordinary={ordinary:?} reader={actual:?} leaves={:?}",
                            leaves(checker, actual)
                        );
                        assert_eq!(leaves(checker, actual), expected, "{context}");
                        assert_eq!(
                            checker.get_type_of_property_of_type(target, "x"),
                            ordinary,
                            "ordinary read must remain unchanged: {context}"
                        );
                    }
                });
            }
            for context in [
                "{ x?: never }",
                "{ x?: never } & { [s: string]: K }",
                "{ [s: string]: K } & { x?: never }",
            ] {
                with_context(context, strict, exact, |checker, target, _, _| {
                    let expected = if strict && !exact {
                        checker.intrinsics.undefined
                    } else {
                        checker.intrinsics.never
                    };
                    assert_eq!(checker.contextual_property_type(target, "x"), Some(expected));
                });
            }
            for context in ["{ x?: undefined }", "{ x: undefined }", "{ [s: string]: undefined }"] {
                with_context(context, strict, exact, |checker, target, _, _| {
                    assert_eq!(
                        checker.contextual_property_type(target, "x"),
                        Some(checker.intrinsics.undefined),
                        "{context}"
                    );
                });
            }
        }
    }

    #[test]
    fn concrete_context_optionality_follows_the_ordinary_reader() {
        for context in ["{ x: V }", "{ x?: V }"] {
            with_context(context, true, true, |checker, target, value, _| {
                let missing_value = checker.get_union_type(&[value, checker.intrinsics.missing]);
                // Instantiated synthetic metadata beats both the mapped
                // sidecar and syntax. Uninstantiated metadata is not a supplier.
                for instantiated in [false, true] {
                    for synthetic_optional in [false, true] {
                        for mapped_optional in [None, Some(false), Some(true)] {
                            checker.anonymous_properties.insert(
                                target,
                                (
                                    vec![crate::objects::AnonymousProperty {
                                        accessor_write: None,
                                        method: false,
                                        origin: None,
                                        checked_declaration: None,
                                        name: "x".into(),
                                        printed_name: "x".into(),
                                        printed_type: "V".into(),
                                        optional: synthetic_optional,
                                        readonly: false,
                                        r#type: missing_value,
                                    }],
                                    instantiated,
                                ),
                            );
                            checker
                                .mapped_identity_optionality
                                .insert(target, (mapped_optional, None));
                            let ordinary =
                                checker.get_type_of_property_of_type(target, "x").unwrap();
                            let actual = checker.contextual_property_type(target, "x").unwrap();
                            let mut expected = vec![value];
                            if instantiated && !synthetic_optional {
                                expected.push(checker.intrinsics.missing);
                            } else if !instantiated && mapped_optional == Some(true) {
                                // The mapped optional-add supplier still adds
                                // genuine undefined; this reader must retain it.
                                expected.push(checker.intrinsics.undefined);
                            }
                            expected.sort_by_key(|ty| ty.index());
                            assert_eq!(leaves(checker, actual), expected);
                            assert_eq!(
                                checker.get_type_of_property_of_type(target, "x"),
                                Some(ordinary)
                            );
                        }
                    }
                }
            });
        }
    }

    #[test]
    fn concrete_context_retains_mapped_supplier_boundaries() {
        // Optional-add still supplies real undefined rather than missing;
        // optional-remove now preserves genuine undefined in exact mode.
        for (context, retains_undefined) in
            [("Part<{ x: V }>", true), ("Need<{ x?: V | undefined }>", true)]
        {
            with_context(context, true, true, |checker, target, value, _| {
                let ordinary = checker.get_type_of_property_of_type(target, "x").unwrap();
                let actual = checker.contextual_property_type(target, "x").unwrap();
                let mut expected = vec![value];
                if retains_undefined {
                    expected.push(checker.intrinsics.undefined);
                }
                expected.sort_by_key(|ty| ty.index());
                assert_eq!(leaves(checker, actual), expected);
                assert_eq!(actual, ordinary);
            });
        }
        with_context("{ x?: undefined }", true, true, |checker, target, _, _| {
            let ordinary = checker.get_type_of_property_of_type(target, "x").unwrap();
            assert_eq!(ordinary, checker.intrinsics.undefined);
            assert_eq!(leaves(checker, ordinary), vec![checker.intrinsics.undefined]);
            assert_eq!(
                checker.contextual_property_type(target, "x"),
                Some(checker.intrinsics.undefined)
            );
        });
    }
}

/// The state `getExportsOfModuleWorker` (`checker.go:16148`) shares across
/// its recursive `visit`, for one name.
#[derive(Default)]
struct TypeOnlyStarWalk {
    visited: Vec<SymbolId>,
    /// The name is an own export of a module reached without `export type`.
    non_type_only: bool,
    /// The last `export type *` that wrote the name.
    type_only_star: Option<NodeId>,
}
