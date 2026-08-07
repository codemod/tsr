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
    fn get_symbol_flags(&mut self, symbol: SymbolId) -> SymbolFlags {
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
    pub(crate) fn resolve_alias(&mut self, symbol: SymbolId) -> Option<SymbolId> {
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
    fn declaration_of_alias_symbol(&self, symbol: SymbolId) -> Option<NodeId> {
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
        let &default = self.binder.symbols().get(module).exports.get("default")?;
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
    /// - **`export *` re-exports.** `getExportOfModule` reads
    ///   `getExportsOfSymbol` (`checker.go:15920`), which resolves star exports
    ///   through `getExportsOfModuleWorker` (`checker.go:16148`). This reads the
    ///   binder's `exports` table directly, so a name that arrives only through
    ///   `export * from "./n"` is not found.
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
    fn get_export_of_module(&self, symbol: SymbolId, name: &str) -> Option<SymbolId> {
        let entry = self.binder.symbols().get(symbol);
        if !entry.flags.intersects(SymbolFlags::MODULE) {
            return None;
        }
        entry.exports.get(name).copied()
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
    fn resolve_external_module_symbol(&self, module_symbol: SymbolId) -> SymbolId {
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
    fn resolve_external_module_name(
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
        // non-relative specifier may name a `declare module "x"`. Upstream keys
        // ambient modules in `globals` under the *quoted* name; this binder
        // stores the literal's text unquoted (`module_name`,
        // `crates/tsr-binder/src/binder.rs:4091`) and that naming is
        // load-bearing for its other consumers, so the selection upstream gets
        // from the quotes is recovered from the declaration's *shape* instead —
        // [`Checker::is_ambient_module`] — plus upstream's
        // `SymbolFlagsValueModule` meaning test.
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
    /// with `getMergedSymbol` on the hit. The quoted-name key becomes a
    /// declaration-shape test here — see the call site above for why.
    fn ambient_module(&self, name: &str) -> Option<SymbolId> {
        // `tspath.IsExternalModuleNameRelative`: `.`, `..`, `./…`, `../…`.
        if name == "." || name == ".." || name.starts_with("./") || name.starts_with("../") {
            return None;
        }
        let &symbol = self.binder.globals().get(name)?;
        let symbol = self.binder.merged_symbol(symbol);
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
    fn module_object_of(&mut self, location: NodeId, specifier: NodeId) -> Option<SymbolId> {
        let module = self.resolve_external_module_name(location, specifier)?;
        if self.resolve_external_module_symbol(module) == module { Some(module) } else { None }
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
                    Some(initializer) => self.check_expression_for_mutable_location(initializer),
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
            // Unported: methods, export assignments, binary/call assignment
            // declarations, JSX attributes and enum members.
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
        Some(self.get_widened_literal_type_for_initializer(declaration, initializer_type))
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
fn is_identifier_text(text: &str) -> bool {
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
