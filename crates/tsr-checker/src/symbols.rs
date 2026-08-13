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
        // §31 (`checker-notes-callres.md`): an `import x = require("...")`
        // upstream never resolves — mispositioned (TS1147 territory) or
        // genuinely unfindable (the TS2307 predicate) — reads `any` at every
        // use site. A resolvable module this port cannot type keeps the
        // `errorType` gap below instead.
        if let Some(declaration) = self.declaration_of_alias_symbol(symbol)
            && let Some(Node::ImportEqualsDeclaration(node)) = self.node_map.get(declaration)
            && let Some(ModuleReference::ExternalModuleReference(reference)) = node.module_reference
        {
            // Position is IRRELEVANT to the type: upstream still resolves
            // the require() against ambient modules inside a namespace
            // (TS1147 is a grammar error, not a resolution bar) —
            // `privacyGloImportParseErrors` wants `typeof errorImport` for a
            // namespace-positioned import of a QUOTED ambient module, the
            // §31 first pair's 4 adverse lines. Findability alone decides.
            let unresolvable = reference
                .expression
                .and_then(|e| e.node_id())
                .is_some_and(|id| self.module_specifier_unfindable(id));
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
        // §145 (`checker-notes-narrow.md`): a QUALIFIED ImportEquals whose
        // chain RESOLVES answers the target under the ALIAS'S OWN NAME —
        // `aliasBug.types` wants `typeof booz`, and the alias name is in
        // hand here (the tsr-4jk constraint's key). A NAMESPACE-flagged leaf
        // takes the typeof-mint (member reads flow through the target's
        // exports); a VALUE leaf types through get_type_of_symbol as any
        // resolved alias does.
        if let Some(declaration) = self.declaration_of_alias_symbol(symbol)
            && let Some(Node::ImportEqualsDeclaration(node)) = self.node_map.get(declaration)
            && let Some(ModuleReference::QualifiedName(qualified)) = node.module_reference
            && let Some(target) = self.resolve_qualified_entity(qualified)
        {
            // Iteration 2 measured the namespace typeof-mint 182-side
            // adverse (per-site spellings: `aliasBug` wants the ALIAS name,
            // `typeofInternalModules` the TARGET chain - the naming wall's
            // seventh appearance) and iteration 3 the same wall for CLASS
            // and ENUM leaves (their texts embed their own names). Only
            // NAMELESS-text leaves - functions, variables, properties -
            // answer here; the rest keep the gap.
            let flags = self.binder.symbols().get(self.binder.merged_symbol(target)).flags;
            // §156 + retry (`checker-notes-narrow.md`): the class-leaf
            // constructor mint measured 58:34, then 33:34 over §157's landed
            // instance half — the coupling is NOT instance annotations but
            // the EXPRESSION road: typing the alias unlocks property-access
            // prints (`x.c` sites) with written/qualified texts where
            // upstream prints the target's SHORT name (`typeof c`), the
            // best_name preference at expression positions. Classes keep the
            // gap until that print road exists.
            if !flags.intersects(SymbolFlags::NAMESPACE | SymbolFlags::CLASS | SymbolFlags::ENUM)
                && self.get_symbol_flags(target).intersects(SymbolFlags::VALUE)
            {
                let computed = self.get_type_of_symbol(target);
                let computed =
                    if self.resolutions.pop() { computed } else { self.intrinsics.error };
                self.symbol_types.insert(symbol, computed);
                return computed;
            }
        }
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
                self.get_type_of_symbol(target)
            }
            _ => self.intrinsics.error,
        };
        let computed = if self.resolutions.pop() { computed } else { self.intrinsics.error };
        self.symbol_types.insert(symbol, computed);
        computed
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
    /// # One divergence, and it is in the safe direction
    ///
    /// When `resolveAlias` yields `unknownSymbol`, upstream returns
    /// `SymbolFlagsAll` — every meaning, so the caller proceeds. There is no
    /// `unknownSymbol` here and [`Checker::resolve_alias`] answers `None`
    /// instead, which stops the walk and leaves the flags at what was
    /// accumulated. `getTypeOfAlias`'s `Value` test then fails and the answer is
    /// `errorType`: a gap rather than a claim. Returning "all meanings" for a
    /// target we could not find would send `get_type_of_symbol` a symbol that
    /// does not exist.
    pub(crate) fn get_symbol_flags(&mut self, symbol: SymbolId) -> SymbolFlags {
        let mut seen: Vec<SymbolId> = Vec::new();
        let mut current = symbol;
        let mut flags = self.binder.symbols().get(current).flags;
        while self.binder.symbols().get(current).flags.intersects(SymbolFlags::ALIAS) {
            let Some(target) = self.resolve_alias(current) else { break };
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
            // `getTargetOfExportSpecifier` (`checker.go:14951`) — both halves,
            // `export { q }` and `export { q } from "./m"`.
            SyntaxKind::ExportSpecifier => return self.export_specifier_target(declaration),
            // `getTargetOfImportSpecifier` (`checker.go:14647`).
            SyntaxKind::ImportSpecifier => return self.import_specifier_target(declaration),
            // `getTargetOfExportAssignment` (`checker.go:14889`) — `export = X`
            // where `X` is an identifier, resolved where it is written. Every
            // other expression shape declines (upstream's
            // `getTargetOfAliasLikeExpression` handles more; each unported
            // shape is a miss, never a wrong target).
            SyntaxKind::ExportAssignment => return self.export_assignment_target(declaration),
            // `getTargetOfImportClause` (`checker.go:14528`) →
            // `getTargetOfModuleDefault` (`:14536`), the PLAIN half only: the
            // module's real `default` export. The synthetic default
            // (`canHaveSyntheticDefault`, interop) and the `module.exports`
            // arm are not ported — each such miss stays a gap
            // (`checker-notes-modobj.md` §10.11).
            SyntaxKind::ImportClause => return self.import_clause_default_target(declaration),
            // **No `NamespaceExportDeclaration` arm, and it is not an omission.**
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
                self.binder
                    .symbols()
                    .get(found)
                    .flags
                    .intersects(SymbolFlags::NAMESPACE)
                    .then_some(found)
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
                if resolved == module {
                    return Some(module);
                }
                self.resolve_alias(resolved)
            }
            // A qualified name RESOLVES fine and prints wrong, for want of
            // symbol accessibility. Unchanged, and not the same problem as the
            // arm above: the name it would print is a *declared* one this port
            // cannot reach, not a module object it cannot spell.
            ModuleReference::QualifiedName(_) => None,
        }
    }

    /// The target of `import a = b.c`, for callers that need its **flags** and
    /// not its name.
    ///
    /// [`Checker::resolve_alias`] declines this shape for the printer's sake;
    /// see §686. Resolution itself is `resolveEntityName`, which this port
    /// already has.
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
                // `KindExportAssignment` needs `ExpressionIsAlias`
                // (`ast/utilities.go:2631`); the shape this port resolves is an
                // identifier, and testing it here rather than answering by kind
                // keeps the predicate honest — see the doc above.
                SyntaxKind::ExportAssignment => matches!(
                    self.node_map.get(declaration),
                    Some(Node::ExportAssignment(node))
                        if matches!(node.expression, Some(tsr_ast::Expression::Identifier(_)))
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
        let Node::ImportDeclaration(import) = self.node_map.get(parent)? else {
            return None;
        };
        let specifier = import.module_specifier?.node_id()?;
        let module = self.resolve_external_module_name(declaration, specifier)?;
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

    /// `getTargetOfExportAssignment` (`checker.go:14889`) for the identifier
    /// shape: resolve `X` of `export = X` where it is written, with the full
    /// alias meaning (`SymbolFlagsValue | Type | Namespace`,
    /// `checker.go:15751`).
    fn export_assignment_target(&mut self, declaration: NodeId) -> Option<SymbolId> {
        let Node::ExportAssignment(node) = self.node_map.get(declaration)? else {
            return None;
        };
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
    /// - **A module with `export =`.** Upstream reads the member off
    ///   `getTypeOfSymbol(targetSymbol)` via `getPropertyOfTypeEx` and may
    ///   combine a value symbol with a type symbol
    ///   (`combineValueAndTypeSymbols`). Both are real machinery; this answers
    ///   `None` when `exports` holds `export=`, so nothing is guessed.
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
        if self.resolve_external_module_symbol(module_symbol) != module_symbol {
            // The `export =` case, gapped above.
            return None;
        }
        self.get_export_of_module(module_symbol, name.text)
    }

    /// TS2305 — `Module '{0}' has no exported member '{1}'.`
    ///
    /// `errorNoModuleMemberSymbol` (`checker.go:14883`), reached when
    /// [`Checker::get_external_module_member`] finds no export for the name.
    ///
    /// Upstream chooses between six messages there. This reports two of them —
    /// TS2724 when the name is a near miss, TS2305 otherwise — and **evaluates
    /// the other four arms' conditions in order to decline them**. See §228:
    /// declining to emit TS2613 costs a missing line, whereas emitting TS2305
    /// in its place would be a wrong one, and the guard is a table lookup
    /// either way.
    pub(crate) fn report_missing_module_export(&mut self, specifier: NodeId) -> Option<()> {
        let declaration = self.import_or_export_declaration_of(specifier)?;
        let module_specifier = self.external_module_name(declaration)?;
        let module_symbol = self.resolve_external_module_name(declaration, module_specifier)?;
        // `export =`: the member lives on the exported type, which
        // `get_external_module_member` already declines to read.
        if self.resolve_external_module_symbol(module_symbol) != module_symbol {
            return None;
        }
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
        if self.get_export_of_module(module_symbol, text).is_some() {
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
        // `moduleSymbol.Exports[InternalSymbolNameDefault] != nil` — upstream's
        // TS2613, `Did you mean to use 'import x from …' instead?`. Declined.
        let has_default = entry.exports.contains_key("default");
        let value_declaration = entry.value_declaration;
        let candidates: Vec<&str> = entry.exports.keys().copied().collect();
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
        if has_default {
            return None;
        }
        // `reportNonExportedMember` (`checker.go:14908`) splits again on
        // whether the module file declares the name **locally**, into three
        // outcomes of which one is built. §679.
        if let Some(source_file) = value_declaration
            && let Some(local) =
                self.binder.locals(source_file).and_then(|locals| locals.get(text).copied())
        {
            // `export =` is its own pair of outcomes upstream — TS2305 or
            // `reportInvalidImportEqualsExportMember` — chosen by
            // `getSymbolIfSameReference`. Declined whole rather than guessed.
            if entry.exports.contains_key("export=") {
                return None;
            }
            // `findInMap(exports, sameReference(localSymbol))`: when an export
            // *is* this local under another name, upstream says TS2460
            // `…but it is exported as '{2}'`. That name is the second
            // argument this port would have to invent, so the branch stays a
            // decline; identity of the merged symbol is what upstream's
            // `getSymbolIfSameReference` reduces to for the local case.
            let merged = self.binder.merged_symbol(local);
            if entry.exports.values().any(|&exported| self.binder.merged_symbol(exported) == merged)
            {
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
    fn import_or_export_declaration_of(&self, specifier: NodeId) -> Option<NodeId> {
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
    fn get_export_of_module(&mut self, symbol: SymbolId, name: &str) -> Option<SymbolId> {
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
    /// Also not ported: `export type *`, whose type-onlyness upstream tracks in
    /// a parallel map. Nothing here reads it.
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
        let Node::StringLiteral(literal) = self.node_map.get(module_specifier)? else {
            // `resolveExternalModuleNameWorker` returns `nil` for anything that
            // is not a string literal (`checker.go:15123`).
            return None;
        };
        // `tryFindAmbientModule` (`checker.go:15533`), consulted **before** the
        // host exactly as `resolveExternalModule` (`checker.go:15154`) does: a
        // non-relative specifier may name a `declare module "x"`.
        if let Some(ambient) = self.ambient_module(literal.text) {
            return Some(ambient);
        }
        let importing_file = self.source_file_of(location)?;
        let target = self.module_host?.resolved_module(importing_file, literal.text)?;
        // `sourceFile.Symbol != nil` (`checker.go:15321`). `None` here is a file
        // that is not an external module — upstream's `File_0_is_not_a_module` —
        // and it is the reason the host answers a *file* rather than a symbol:
        // resolving to a plain script is a successful resolution with no module
        // symbol at the end of it, and only the checker can tell those apart.
        self.binder.symbol_of(target)
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
    /// regresses a passing case that wants `typeof N`. (`NAMESPACE_MODULE`
    /// alone fires on nothing — a namespace containing a class is
    /// INSTANTIATED, so the binder stamps `VALUE_MODULE`.) The two changes are
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
        let named = self.resolve_alias(target).unwrap_or(target);
        let is_module_object = self
            .binder
            .symbols()
            .get(named)
            .flags
            .intersects(SymbolFlags::VALUE_MODULE | SymbolFlags::NAMESPACE_MODULE);
        if is_module_object { None } else { Some(target) }
    }

    /// The module specifier of an `ImportDeclaration` or an `ExportDeclaration`.
    ///
    /// Ported from `ast.GetExternalModuleName` (`internal/ast/utilities.go:1903`)
    /// over the two kinds [`Checker::get_external_module_member`] is reached
    /// with. Upstream also covers `ImportEqualsDeclaration`, `ModuleDeclaration`
    /// and `ImportTypeNode`; none of those reaches this function in this port,
    /// and listing a kind whose caller does not exist would be an intention
    /// documented as though it were built.
    fn external_module_name(&self, node: NodeId) -> Option<NodeId> {
        let specifier = match self.node_map.get(node)? {
            Node::ImportDeclaration(node) => node.module_specifier,
            Node::ExportDeclaration(node) => node.module_specifier,
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
            current = self.nodes.parent(current)?;
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
    ///   not ported. **Its stated reason is gone**: the checker carries compiler
    ///   options now ([`Checker::apply_compiler_options`], ADR-0042) and
    ///   [`Checker::strict_null_checks`] is one of them, so "there are no
    ///   compiler options here" no longer holds. The arm is still unwritten —
    ///   only the excuse expired.
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
            // A `TypeQueryNode`. Upstream gives it `TypePrecedenceTypeOperator`
            // so that it parenthesises in *postfix* position — `(typeof C)[]` —
            // and not as a union constituent.
            return self.store.new_anonymous(TypeFlags::OBJECT, printed, symbol, false);
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
        // SS241. Upstream's test is on the RESOLVED TYPE — "no properties and no
        // index signatures" — and `exports.is_empty()` is a proxy for it. The
        // proxy is wrong for a **type-only** export, which contributes no
        // property to the anonymous object type at all:
        //
        //     function y5c() { }
        //     namespace y5c { export interface I { foo(): void } }
        //     >y5c : () => void        <- upstream, NOT `{ (): void; ... }`
        //
        // Witnesses `augmentedTypesFunction` and `augmentedTypesModules`, opened
        // independently before this was called a family (conventions corollary
        // 21) and agreeing on the construct AND the branch.
        //
        // The expando gap above is untouched: a VALUE export or member still
        // bails, because that one really does add a property this port cannot
        // order. Only the type-only population moves.
        let symbol_data = self.binder.symbols().get(symbol);
        let symbols = self.binder.symbols();
        let contributes_a_property =
            |&member: &SymbolId| symbols.get(member).flags.intersects(SymbolFlags::VALUE);
        if symbol_data.exports.values().any(contributes_a_property)
            || symbol_data.members.values().any(contributes_a_property)
        {
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
        // `[signature]` renders a bare `FunctionTypeNode`; the many-signature
        // arm renders a `TypeLiteralNode`, which is never parenthesised.
        let mut signature_node = false;
        let printed = match signatures.as_slice() {
            [] => return self.intrinsics.error,
            [signature] => {
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
                let mut out = String::from("{ ");
                for signature in many {
                    out.push_str(&crate::objects::signature_member_text(self, signature));
                    out.push_str("; ");
                }
                out.push('}');
                out
            }
        };
        let built = self.store.new_anonymous(TypeFlags::OBJECT, printed, symbol, signature_node);
        // The structure `printed` was rendered from, kept reachable from the id
        // for `instantiate_type` — see `Checker::signature_types`
        // (`bd tsr-0hc`). Both arms are recorded; the single/many distinction
        // is recovered from the length when the instantiated form re-renders.
        self.signature_types.insert(built, signatures);
        built
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
        self.binder.symbols().get(symbol).name.is_empty()
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
    fn discriminate_union_root(&mut self, t: TypeId, literal: NodeId) -> TypeId {
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
    fn contextual_property_type(&mut self, t: TypeId, name: &str) -> Option<TypeId> {
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
                    _ => Some(self.get_union_type_unprinted(&hits)),
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
                for constituent in constituents {
                    if let Some(member) = self.get_type_of_property_of_type(constituent, name)
                        && member != self.intrinsics.error
                    {
                        hits.push(member);
                    }
                }
                match hits.len() {
                    0 => None,
                    1 => Some(hits[0]),
                    _ => Some(self.get_intersection_type(&hits, None)),
                }
            }
            _ => self.get_type_of_property_of_type(t, name),
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
                    let signature = self.resolve_call_signature(callee_type, Some(call.arguments));
                    self.narrow_value_stack.remove(&holder);
                    let Some(signature) = signature else {
                        if debug {
                            eprintln!("CTX: no signature (callee {callee_type:?})");
                        }
                        return None;
                    };
                    if !signature.type_parameters.is_empty() {
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
                    // (upstream's `getAssignmentReducedType` is unported) —
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
            SyntaxKind::BindingElement => self.get_type_for_binding_element(declaration),
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
    fn report_circularity_error(&mut self, declaration: NodeId) -> TypeId {
        if self.type_annotation_of(declaration).is_some() {
            return self.intrinsics.error;
        }
        self.intrinsics.any
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
        if let Some(id) = self.get_type_for_variable_like_declaration(declaration) {
            // SS187 `getWidenedType` (checker.go:16090): with
            // `strictNullChecks` OFF, a `null` or `undefined` type widens to
            // `any` — `const c5 = null` records `>c5 : any` under
            // `@strict: false` (`compiler/constDeclarations`). This is a
            // DECISION, not a decline, so it is gated on the flag actually
            // read from the case's options; the paired control fixture
            // asserts `null` stays `null` under `@strict: true`.
            if !self.strict_null_checks
                && (id == self.intrinsics.null || id == self.intrinsics.undefined)
            {
                return self.intrinsics.any;
            }
            id
        } else {
            // Upstream returns `anyType` for a declaration with neither an
            // annotation nor an initialiser (`checker.go:18264`) — a genuine
            // answer, the implicit any, not a gap. So `anyType` is right here
            // where `errorType` is right for an unported form. A REST
            // parameter's implicit any is `anyArrayType`
            // (`checker-notes-narrow.md` §19).
            {
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

    /// Ported from `Checker.getTypeForVariableLikeDeclaration`
    /// (`checker.go:16652`), restricted to the two paths this slice covers.
    ///
    /// `None` means "nothing could be inferred", which upstream signals with a
    /// nil `*Type` and turns into the implicit `any` one level up.
    /// §38's element slice: `Array`/`ReadonlyArray` references answer the
    /// argument, tuples the union of their elements, strings `string`.
    /// `None` declines to the implicit-any road.
    fn for_of_element_type(&mut self, iterated: TypeId) -> Option<TypeId> {
        if iterated == self.intrinsics.error {
            return None;
        }
        if let Some((elements, _)) = self.tuple_element_lists.get(&iterated) {
            let elements = elements.clone();
            if elements.is_empty() {
                return None;
            }
            return Some(self.get_union_type(&elements));
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
                return Some(arguments[0]);
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
        // The protocol itself stays blocked for USER-DEFINED iterables, and
        // for the reason recorded below — a computed `[Symbol.iterator]`
        // member is filed under `__computed`, in no symbol table, because late
        // binding is unported. That blocker is real and this does not pretend
        // otherwise; it sidesteps it only where the lib's own declaration
        // makes the answer readable without the walk.
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
            let is_lib_iterable = ["Iterable", "IterableIterator", "Generator"]
                .into_iter()
                .filter_map(|name| self.binder.globals().get(name).copied())
                .any(|symbol| self.binder.merged_symbol(symbol) == target);
            if is_lib_iterable {
                return Some(first);
            }
        }
        let flags = self.store.get(iterated).flags;
        if flags.intersects(crate::flags::TypeFlags::STRING_LITERAL)
            || iterated == self.intrinsics.string
        {
            return Some(self.intrinsics.string);
        }
        // **The custom-iterable road is blocked, and the blocker is named so
        // the next reader does not re-derive it.** `for (var v of new
        // FooIterator)` wants the ITERATION PROTOCOL: read `[Symbol.iterator]`
        // off the type, take its call signature's return, read `next` off
        // that, take ITS return's `value` property. Every one of those four
        // steps has ported machinery — and the first is unreachable, because
        // the binder files a computed-name member under `__computed`, which is
        // *deliberately in no symbol table at all* (`binder.rs`'s
        // `INTERNAL_COMPUTED`): late binding is unported
        // (`member_completeness.rs:38`). There is no name to look
        // `[Symbol.iterator]` up by.
        //
        // Measured population, `nearmiss --max 2`: about ten cases, of which
        // `for-of19` through `for-of23` and `for-of30`/`31` are the clean
        // witnesses. They will convert when late binding lands and not before;
        // nothing narrower reaches them, because the protocol's first hop is
        // the one that is missing. §212.
        None
    }

    fn get_type_for_variable_like_declaration(&mut self, declaration: NodeId) -> Option<TypeId> {
        // An annotation wins over an initialiser, always.
        if let Some(annotation) = self.type_annotation_of(declaration) {
            let declared = self.get_type_from_type_node(annotation);
            // `addOptionalityEx(declaredType, isProperty, isOptional)`
            // (`checker.go:16695`): `p?: string` declares `string | undefined`.
            // See `crate::optionality`.
            return Some(self.add_optionality_for_declaration(declared, declaration));
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
        // §38.1: a for-IN binding is `string`, unconditionally
        // (`getTypeForVariableLikeDeclaration`'s ForIn arm,
        // `checker.go:16698` region — upstream returns `stringType`).
        if self.nodes.kind(declaration) == SyntaxKind::VariableDeclaration
            && let Some(list) = self.nodes.parent(declaration)
            && let Some(statement) = self.nodes.parent(list)
            && self.nodes.kind(statement) == SyntaxKind::ForInStatement
        {
            return Some(self.intrinsics.string);
        }
        // §38 (`checker-notes-callres.md`): a for-of binding takes the
        // iterated element — array references, tuples, and strings; every
        // other RHS keeps the implicit-any road.
        if self.nodes.kind(declaration) == SyntaxKind::VariableDeclaration
            && let Some(list) = self.nodes.parent(declaration)
            && let Some(statement) = self.nodes.parent(list)
            && self.nodes.kind(statement) == SyntaxKind::ForOfStatement
            && let Some(Node::ForInOrOfStatement(for_of)) = self.node_map.get(statement)
            // `for await` iterates the AWAITED element — unported, and the
            // non-async-position error renders `any` upstream; both decline.
            && for_of.await_modifier.is_none()
            && let Some(expression) = for_of.expression
        {
            let iterated = self.check_expression(expression);
            let element = self.for_of_element_type(iterated);
            if let Some(element) = element {
                let widened = self.get_widened_literal_type(element);
                return Some(widened);
            }
            return None;
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
        // `noImplicitAny` is **not**: this port models no compiler options and
        // assumes strict throughout, the same assumption `array_literals.rs`
        // and `unions.rs` already state for `strictNullChecks`. A case compiled
        // with `noImplicitAny` off would take upstream down a different path,
        // and that is a known divergence rather than an oversight.
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

        let initializer_type = self.check_expression(initializer);
        let widened = self.get_widened_literal_type_for_initializer(declaration, initializer_type);
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
