//! Circularity detection, and the seam through which the checker reaches
//! another file.
//!
//! Ported from `Checker.pushTypeResolution`, `popTypeResolution` and
//! `findResolutionCycleStartIndex` (`internal/checker/checker.go:18758`–`18794`).
//!
//! It also holds [`ModuleHost`], the slice of upstream's `Program` interface
//! (`checker.go:547`) that the checker needs in order to resolve a module
//! specifier — the one thing a cross-file alias cannot compute for itself.
//!
//! # Why a stack rather than a flag
//!
//! The obvious implementation is a "currently resolving" bit per symbol, and it
//! is wrong in a way that only shows on a cycle of length greater than one.
//! Upstream, on finding a cycle, marks **every frame from the cycle start
//! onward** as failed:
//!
//! ```go
//! for i := resolutionCycleStartIndex; i < len(c.typeResolutions); i++ {
//!     c.typeResolutions[i].result = false
//! }
//! ```
//!
//! So in `const a = b, b = c, c = a`, all three participants resolve to an error
//! — not only `a`, the one that closed the loop. A per-symbol flag gives the
//! symbol that closed the cycle an error and lets the others quietly succeed with
//! whatever half-built answer was on the stack, which is both wrong and
//! order-dependent.
//!
//! # What is not ported
//!
//! `findResolutionCycleStartIndex` also consults `typeResolutionHasProperty`,
//! which lets a resolution that has *already* been memoised short-circuit the
//! search. That is an optimisation over a stack that is at most a few deep in
//! practice, and it needs the memo of every property kind to exist first. Left
//! out deliberately, with the behaviour unchanged: without it the search simply
//! scans further, and a memoised entry never reaches the stack anyway because the
//! caller returns before pushing.

use tsr_ast::NodeId;

/// How the checker reaches another file.
///
/// Upstream's checker does not take a concrete `Program`. It takes an
/// **interface**, declared inside the checker package itself
/// (`internal/checker/checker.go:547`), held as the field `program Program`
/// (`checker.go:581`), whose one relevant method here is
/// `GetResolvedModule` (`checker.go:558`, implemented at
/// `internal/compiler/program.go:521` as a lookup keyed on
/// `(file.Path(), {Name, Mode})`). This trait is that interface reduced to the
/// single question `resolveExternalModule` (`checker.go:15149`) asks of it, and
/// it lives here for the same reason upstream's lives in `checker.go`: the
/// checker owns the shape of what it needs, and `tsr-compiler` depends on
/// `tsr-checker` rather than the other way round.
///
/// # Why this is one method and not sixteen
///
/// Upstream's interface carries eighteen methods because upstream's
/// `resolveExternalModule` reports fourteen distinct diagnostics — about `.ts`
/// extensions, about rewritten relative imports, about a `CommonJS` file reaching
/// an ES module. **None of that is ported**, and the arm that is ported needs
/// exactly one fact: which file a specifier names. Adding the rest now would be
/// documenting an intention as though it were built.
///
/// # Why the ids are node ids and not paths
///
/// [ADR-0034](../../../docs/adr/0034-a-program-needs-one-identity-space.md) gave
/// a program one `NodeTable`, one `NodeMap` and one `BindResult`, so a
/// [`tsr_binder::SymbolId`] already names one symbol across every file
/// (`crates/tsr-compiler/src/lib.rs`, `Program::bind_source_files`). That is
/// what makes this seam small: the checker does not need cross-file *symbol*
/// access, because it already has it. It needs only the specifier-to-file map,
/// and a `SourceFile` node id is how a file is named inside that identity space.
///
/// The consequence is that `tsr-checker` names no path type, no
/// `ResolvedModule` and no `ResolutionMode` — all three are the host's
/// vocabulary. If a mode is ever needed to disambiguate two resolutions of one
/// specifier in one file, this method grows a parameter and the single call site
/// in [`crate::symbols`] follows.
pub trait ModuleHost {
    /// The file a module specifier names, or `None` if it names none.
    ///
    /// `Program.GetResolvedModule` (`internal/compiler/program.go:521`)
    /// composed with `GetSourceFileForResolvedModule`, which is how
    /// `resolveExternalModule` uses it (`checker.go:15149`).
    ///
    /// Both ids are **`SourceFile` node ids**. `None` covers upstream's
    /// unresolved module *and* its resolved-to-a-file-not-in-the-program case;
    /// the checker treats them alike because both answer `errorType`, and
    /// telling them apart is a diagnostic distinction this port has no consumer
    /// for.
    ///
    /// **Not the same as "the file is not a module."** That is
    /// `sourceFile.Symbol == nil` (`checker.go:15321`), which the checker tests
    /// for itself with `BindResult::symbol_of` on the returned id — so a host
    /// that answers `Some` for a plain script is answering correctly, and the
    /// checker still gaps.
    fn resolved_module(&self, importing_file: NodeId, specifier: &str) -> Option<NodeId>;

    /// §110: the `TypeParameterDeclaration` node ids a declaration's JSDoc
    /// `@template` tags carry, in tag order. Id-vocabulary per ADR-0034;
    /// default empty so hosts without JSDoc (unit checkers) change nothing.
    fn jsdoc_template_parameters(&self, _declaration: NodeId) -> Vec<NodeId> {
        Vec::new()
    }

    /// Did resolution name a file **at all**, whether or not the program holds
    /// it?
    ///
    /// `Program.GetResolvedModule(...).IsResolved()` (`checker.go:15208`), the
    /// half of the composition above that
    /// [`ModuleHost::resolved_module`] throws away.
    ///
    /// # Why the trait grew a second method
    ///
    /// The doc on [`ModuleHost::resolved_module`] said, correctly at the time,
    /// that its `None` covers upstream's unresolved module *and* its
    /// resolved-to-a-file-not-in-the-program case, and that *"telling them apart
    /// is a diagnostic distinction this port has no consumer for."* The check
    /// traversal ([`crate::check`]) is that consumer. Upstream reports a
    /// **different code** for each — TS2307 for the first, TS7016 / TS6142 /
    /// TS2306 for the second, all at the same position — so a rule that cannot
    /// distinguish them manufactures a wrong diagnostic on every untyped
    /// `node_modules` package and every `.tsx` reached without `--jsx`.
    /// Measured, before the gate existed: 15 of 60 wrong lines.
    ///
    /// ADR-0041's "one method rather than eighteen" is a rule about not porting
    /// the `Program` interface speculatively, not a cap. This is the second
    /// question a real caller asks.
    fn module_resolution_found(&self, importing_file: NodeId, specifier: &str) -> bool;

    /// The **file name** the resolver answered for this specifier.
    ///
    /// The third question a real caller asks, and the one that picks the code
    /// among the resolved-but-not-in-the-program family: a `.js` is TS7016, a
    /// `.tsx` reached without `--jsx` is TS6142, a resolved `.ts` that is not a
    /// module is TS2306. [`ModuleHost::module_resolution_found`] separates that
    /// family from TS2307's; only the extension separates its members.
    ///
    /// Defaulted to `None`, which reports nothing — a host that is not a real
    /// program has no resolutions to describe. §357.
    /// §143: the file path of a SourceFile node, for the relative-specifier
    /// spelling. `None` (the default) declines the file half.
    fn file_path(&self, _file: NodeId) -> Option<String> {
        None
    }

    fn resolved_module_path(&self, _importing_file: NodeId, _specifier: &str) -> Option<String> {
        None
    }

    /// The namespace an `@jsx` pragma names for this file, if it has one.
    ///
    /// `getJsxNamespaceAt` (`checker/jsx.go:1306`) resolves the factory's first
    /// identifier and looks for a `JSX` namespace among **that** symbol's
    /// exports, so `@jsx dom.createElement` takes its `IntrinsicElements` from
    /// `dom.JSX` rather than from the global `JSX`.
    ///
    /// Defaulted to `None`: a host that is not a real program has no pragmas to
    /// report, and answering `None` is answering correctly — the checker then
    /// falls back to the global lookup, which is upstream's own order. §211.
    fn jsx_factory_namespace(&self, _file: NodeId) -> Option<String> {
        None
    }

    /// Is this file a `.d.ts`?
    ///
    /// `SourceFile.IsDeclarationFile`, which the checker reads in
    /// `canHaveSyntheticDefault` (`checker.go:14850`) among other places. It is
    /// a *host* question here for the reason
    /// [ADR-0016](../../../docs/adr/0016-file-info-not-a-file-name.md) gives:
    /// nothing on the AST carries a file name, and the checker learns its
    /// **own** file's ambience from `FileContext` — which says nothing about
    /// the module on the other end of an import.
    ///
    /// Defaulted to `false`, which is the conservative answer: it sends
    /// `canHaveSyntheticDefault` down the source-file arm, where only an
    /// `export =` grants one. A host that cannot tell therefore reports TS1192
    /// where upstream might not, rather than the reverse. §531.
    fn is_declaration_file(&self, _file: NodeId) -> bool {
        false
    }
}

/// Which lazily-computed property of an entity is being resolved.
///
/// Upstream's `TypeSystemPropertyName`. Only the variants this port reaches are
/// listed; adding one means porting the code that resolves it, not inventing a
/// name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyName {
    /// `TypeSystemPropertyNameType` — the type of a symbol.
    Type,
    /// `TypeSystemPropertyNameDeclaredType` — the type a *type* symbol declares.
    ///
    /// A separate property from [`PropertyName::Type`] because upstream keys the
    /// stack on the pair: a class symbol has both a declared type (its instance
    /// type) and a type (`typeof C`), and one may legitimately be computed while
    /// the other is in progress. Sharing one key would report a cycle that is
    /// not there.
    DeclaredType,
}

/// One frame of the resolution stack.
#[derive(Debug, Clone, Copy)]
struct Resolution<K> {
    target: K,
    property: PropertyName,
    /// `false` once this frame is known to be part of a cycle.
    succeeded: bool,
}

/// The resolution stack, `Checker.typeResolutions`.
///
/// Generic in the key because upstream's is: `pushTypeResolution` takes a
/// `TypeSystemEntity`, which is a symbol for `TypeSystemPropertyNameType` and a
/// **type** for `TypeSystemPropertyNameResolvedTypeArguments`. The checker uses
/// `Resolutions<SymbolId>` today; the other keys arrive with the code that needs
/// them.
#[derive(Debug)]
pub struct Resolutions<K> {
    stack: Vec<Resolution<K>>,
}

impl<K> Default for Resolutions<K> {
    fn default() -> Self {
        Self { stack: Vec::new() }
    }
}

impl<K: Copy + PartialEq> Resolutions<K> {
    /// An empty stack.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How deep the stack is. For tests that assert it is left balanced.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Whether `(target, property)` is currently resolving, WITHOUT marking
    /// the cycle as failed — the §29 alias placeholder's read-only probe
    /// (`checker-notes-narrow.md`).
    #[must_use]
    pub fn on_stack(&self, target: K, property: PropertyName) -> bool {
        self.stack.iter().any(|r| r.target == target && r.property == property)
    }

    /// Begin resolving `property` of `symbol`.
    ///
    /// Returns `false` if that is already in progress — a cycle — having first
    /// marked every frame from the cycle's start as failed, so that all of its
    /// participants report the circularity rather than only the one that closed
    /// it. The caller must **not** pop when this returns `false`; nothing was
    /// pushed.
    pub fn push(&mut self, target: K, property: PropertyName) -> bool {
        if let Some(start) =
            self.stack.iter().rposition(|r| r.target == target && r.property == property)
        {
            for frame in &mut self.stack[start..] {
                frame.succeeded = false;
            }
            return false;
        }
        self.stack.push(Resolution { target, property, succeeded: true });
        true
    }

    /// Finish the innermost resolution, reporting whether it was cycle-free.
    ///
    /// # Panics
    ///
    /// If the stack is empty, which means a [`Resolutions::push`] that returned
    /// `false` was popped anyway.
    pub fn pop(&mut self) -> bool {
        self.stack.pop().expect("popped a resolution that was never pushed").succeeded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_resolution_that_does_not_recurse_succeeds() {
        let mut r: Resolutions<u32> = Resolutions::new();
        assert!(r.push(0u32, PropertyName::Type));
        assert!(r.pop());
        assert_eq!(r.depth(), 0, "the stack must be left balanced");
    }

    #[test]
    fn nested_resolutions_of_different_symbols_both_succeed() {
        let mut r: Resolutions<u32> = Resolutions::new();
        assert!(r.push(0u32, PropertyName::Type));
        assert!(r.push(1u32, PropertyName::Type));
        assert!(r.pop());
        assert!(r.pop());
    }

    #[test]
    fn a_direct_self_reference_is_a_cycle() {
        let mut r: Resolutions<u32> = Resolutions::new();
        assert!(r.push(0u32, PropertyName::Type));
        assert!(!r.push(0u32, PropertyName::Type), "`const a = a` must not recurse forever");
        assert!(!r.pop(), "the frame it re-entered is now known to have failed");
    }

    #[test]
    fn every_participant_in_a_longer_cycle_fails_not_only_the_one_that_closed_it() {
        // `a -> b -> c -> a`. This is the case a per-symbol "in progress" flag
        // gets wrong: it would fail `a` and let `b` and `c` succeed with a
        // half-built answer, which is both incorrect and dependent on which
        // symbol was asked for first.
        let mut r: Resolutions<u32> = Resolutions::new();
        assert!(r.push(0u32, PropertyName::Type));
        assert!(r.push(1u32, PropertyName::Type));
        assert!(r.push(2u32, PropertyName::Type));
        assert!(!r.push(0u32, PropertyName::Type), "closes the cycle");

        assert!(!r.pop(), "c is part of the cycle");
        assert!(!r.pop(), "b is part of the cycle");
        assert!(!r.pop(), "a is part of the cycle");
    }

    #[test]
    fn a_frame_outside_the_cycle_still_succeeds() {
        // `outer -> a -> b -> a`. `outer` is not a participant: the cycle starts
        // at `a`, so only frames from there on are marked. Marking the whole
        // stack would turn an unrelated enclosing resolution into an error.
        let mut r: Resolutions<u32> = Resolutions::new();
        assert!(r.push(9u32, PropertyName::Type)); // outer
        assert!(r.push(0u32, PropertyName::Type)); // a
        assert!(r.push(1u32, PropertyName::Type)); // b
        assert!(!r.push(0u32, PropertyName::Type));

        assert!(!r.pop(), "b is in the cycle");
        assert!(!r.pop(), "a is in the cycle");
        assert!(r.pop(), "outer merely contained it");
    }
}
